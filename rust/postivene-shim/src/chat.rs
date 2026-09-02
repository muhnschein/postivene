//! One chat's messages, as a QML-instantiable type.
//!
//! QML creates one per conversation page, so two open chats no longer share
//! a model. Loading is a batch call and events update rows in place, rather
//! than refetching the whole history per message.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Local, TimeZone};
use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;

use crate::core::connection;
use crate::json;
use crate::models::{MessageListItem, MessageListModel};

/// `DC_STATE_IN_FRESH` and `DC_STATE_IN_NOTICED`: an incoming message the
/// account has not read yet.
const UNSEEN_STATES: [u32; 2] = [10, 13];

/// How many messages are fetched in one go.
///
/// The ids are cheap and the messages are not: `get_message_list_items`
/// returns a list of numbers for the whole chat, while `get_messages`
/// builds every field of every row. So the model holds a row for *every*
/// message from the moment that list arrives, and fills in only the ones
/// somebody is looking at.
///
/// That is what makes the first message row 0 and keeps it there. The model
/// used to hold a moving window of loaded messages instead, and every way
/// of getting somewhere in a chat meant replacing its contents: the view's
/// idea of where it was went with them, positioning into a model that had
/// just been reset was unreliable in ways that depended on how fast rows
/// were measured, and a reconciliation that overlapped a move undid it. It
/// took three attempts at the symptoms to be sure the shape was the fault.
/// Whisperfish and deltachat-android both keep the whole conversation
/// addressable and neither has any of this.
const PAGE: usize = 50;

/// How far beyond what the reader can see to fill in, so that scrolling
/// does not walk into blank rows before the next fetch answers.
const MARGIN: usize = 25;

/// A message as the id list knows it: which message, and which day it is
/// under.
///
/// The day arrives here rather than with the message, and that is the point.
/// A placeholder whose day is unknown until it is fetched has to be
/// re-sectioned when it is -- and a day heading that gains its height after
/// the view has laid out is drawn on top of the row beneath it, while the
/// rows still waiting share one section headed by the epoch. Both were
/// reported: a strip saying 1 January 1970, then a real date overlapping
/// the first message.
///
/// `get_message_list_items` interleaves the core's own day markers when
/// asked for them, so this costs nothing but the flag.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Entry {
    pub message_id: u32,
    pub day_number: i64,
}

/// The page a chat opens with filled in.
///
/// `find` names a message to open at -- what a search result gives -- and
/// anything not in the chat, 0 included, opens at the newest messages.
pub(crate) fn opening_page(entries: &[Entry], find: u32) -> &[Entry] {
    let start = match entries.iter().position(|entry| entry.message_id == find) {
        // Half a page above it, so it does not land against the top edge
        // with nothing before it.
        Some(index) => index.saturating_sub(PAGE / 2),
        None => entries.len().saturating_sub(PAGE),
    };
    let end = (start + PAGE).min(entries.len());
    &entries[start..end]
}

/// The message ids of a run of entries, for handing to `get_messages`.
pub(crate) fn ids_of(entries: &[Entry]) -> Vec<u32> {
    entries.iter().map(|entry| entry.message_id).collect()
}

/// One row per message, with `items` filled in where they were fetched.
fn rows_for(entries: &[Entry], items: Vec<MessageListItem>) -> Vec<MessageListItem> {
    let mut fetched: BTreeMap<u32, MessageListItem> = items
        .into_iter()
        .map(|item| (item.message_id, item))
        .collect();
    entries
        .iter()
        .map(|entry| {
            fetched
                .remove(&entry.message_id)
                .unwrap_or_else(|| placeholder(*entry))
        })
        .collect()
}

/// The messages of one chat.
///
/// ```qml
/// ChatMessages { id: messages; account_id: 1; chat_id: 7 }
/// SilicaListView { model: messages.rows }
/// ```
#[derive(QObject, Default)]
// `loaded`, `is_group`, `reading_history` and `sending` are four bools, and
// clippy would rather they were a state enum. They are not states of one
// thing: each is an independent fact QML binds to on its own, and any
// combination of them is legitimate. Collapsing them would mean inventing a
// state machine that does not exist and hiding four bindings behind it.
#[allow(clippy::struct_excessive_bools)]
pub struct ChatMessages {
    base: qt_base_class!(trait QObject),

    /// Which account this chat belongs to. Setting it reloads.
    pub account_id: qt_property!(u32; WRITE set_account_id NOTIFY chat_changed),
    /// Which chat. Setting it reloads.
    pub chat_id: qt_property!(u32; WRITE set_chat_id NOTIFY chat_changed),
    /// True once a load has finished, however it went.
    ///
    /// An empty chat and a chat whose messages have not arrived yet are
    /// the same thing to `count`, so the placeholder was shown during
    /// every open -- a flash of "no messages yet" before the history
    /// appeared. This tells the two apart.
    pub loaded: qt_property!(bool; NOTIFY loaded_changed),
    /// Emitted when [`Self::loaded`] changes.
    pub loaded_changed: qt_signal!(),
    /// Emitted when the chat this model points at changes.
    pub chat_changed: qt_signal!(),

    /// True for a group, mailing list or broadcast: chats where a message
    /// needs to say who sent it.
    pub is_group: qt_property!(bool; NOTIFY is_group_changed),
    /// Emitted once the chat's kind is known.
    pub is_group_changed: qt_signal!(),

    /// The rows, for a `SilicaListView`'s `model`.
    pub rows: qt_property!(RefCell<MessageListModel>; CONST),

    /// How many rows there are. A `QAbstractListModel` exposes no `count`
    /// to QML, and a placeholder needs one.
    pub count: qt_property!(u32; READ count NOTIFY rows_changed),
    /// Emitted after any change to `rows`.
    pub rows_changed: qt_signal!(),

    /// Loading or sending failed. The message is the core's own.
    pub error: qt_signal!(message: QString),

    /// Reload the chat. Called automatically when the chat changes.
    pub reload: qt_method!(fn(&mut self)),

    /// Fill in the rows between `first` and `last`, and a little either
    /// side.
    ///
    /// The view asks as it scrolls. Rows outside what anyone is looking at
    /// stay as placeholders; this is the only thing that fetches messages
    /// after the chat is opened.
    pub hydrate: qt_method!(fn(&mut self, first: i32, last: i32)),
    /// True while rows are being filled in, for a quiet indicator.
    pub hydrating: qt_property!(bool; NOTIFY hydrating_changed),
    /// Emitted when [`Self::hydrating`] changes.
    pub hydrating_changed: qt_signal!(),

    /// Say where `message_id` is, so the view can go there.
    ///
    /// What a search result needs. Every message in the chat has a row from
    /// the moment the id list arrives, so this is a lookup rather than
    /// anything that has to be fetched first.
    pub reveal: qt_method!(fn(&mut self, message_id: u32)),
    /// `message_id` sits at `row`, or -1 if this chat has no such message.
    pub revealed: qt_signal!(message_id: u32, row: i32),

    /// The unsent text this chat is holding.
    ///
    /// The core's, not ours: it keeps drafts itself, so one survives the
    /// app being closed, and a chat holding one says so in its own chat
    /// list summary without anything here building that text.
    pub draft: qt_property!(QString; NOTIFY draft_changed),
    /// Emitted when the draft is loaded or written.
    pub draft_changed: qt_signal!(),
    /// Remember `text` as this chat's draft, or forget it when empty.
    pub save_draft: qt_method!(fn(&mut self, text: QString)),

    /// Feed a `core_event` in. Events for other accounts or chats are
    /// ignored, so a page can connect this without filtering first.
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),

    /// True while the reader is up in the history rather than at the
    /// newest message. Only what they can see is marked read; a message
    /// arriving further down is left unread, badge and all, until they
    /// come back to it. False by default, because that is the state a
    /// chat opens in and the flag is set after the chat id.
    pub reading_history: qt_property!(bool; WRITE set_reading_history NOTIFY reading_history_changed),
    /// Emitted when [`Self::reading_history`] changes.
    pub reading_history_changed: qt_signal!(),

    /// The message the next send replies to, 0 for none. The page sets it
    /// when the reader picks Reply and shows what is being replied to;
    /// sending clears it.
    pub quoted_message_id: qt_property!(u32; NOTIFY quote_changed),
    /// Emitted when `quoted_message_id` changes.
    pub quote_changed: qt_signal!(),

    /// Send a plain-text message to this chat.
    pub send: qt_method!(fn(&mut self, text: QString)),
    /// Send a message with a file attached; `text` may be empty.
    ///
    /// One file per message, which is the core's own shape: `misc_send_msg`
    /// takes a single `file`, and a message carries a single `viewType`.
    /// The core decides that type from the file itself, copies the file
    /// into its blob directory, and sends -- so the picked file is free to
    /// go away afterwards.
    pub send_file: qt_method!(fn(&mut self, text: QString, file_path: QString)),
    /// Mark every unread message now loaded as read. Called when the
    /// reader reaches the newest message.
    pub mark_seen_all: qt_method!(fn(&mut self)),

    /// Which row a message is in, or -1 when it is not loaded.
    ///
    /// A search result names a message, and opening its chat at the newest
    /// message rather than at the one that matched is the difference
    /// between finding something and being told where it roughly is.
    pub row_of: qt_method!(fn(&self, message_id: u32) -> i32),
    /// Delete a message. Not only here: the core also removes it from the
    /// mail server, which for this client is where it lived.
    pub delete_message: qt_method!(fn(&mut self, message_id: u32)),
    /// Try a failed message again.
    pub resend_message: qt_method!(fn(&mut self, message_id: u32)),
    /// Send a copy of one message into another chat; QML calls this.
    pub forward_to: qt_method!(fn(&mut self, message_id: u32, chat_id: u32)),
    /// True from a send being asked for until the core answers.
    ///
    /// The compose state is cleared on the answer rather than on the tap,
    /// so that a send which fails leaves the reader holding what they
    /// chose. That leaves a window -- seconds, for a large video the core
    /// has to copy into its blob directory -- in which the field still
    /// holds the text and the bar still holds the file, and tapping send
    /// again sends the whole thing a second time. It is not hypothetical:
    /// it is the first thing an impatient thumb does.
    pub sending: qt_property!(bool; NOTIFY sending_changed),
    /// Emitted when [`Self::sending`] changes.
    pub sending_changed: qt_signal!(),
    /// A message of ours reached the core and is in `rows`.
    pub sent: qt_signal!(message_id: u32),
    /// This many messages from other people were just added. Said outright
    /// rather than left to be inferred from the row count, which a deletion
    /// moves too -- and which does not move at all when a removal and an
    /// arrival land in the same reload.
    pub arrived: qt_signal!(count: u32),
}

impl ChatMessages {
    /// How many rows there are.
    pub fn count(&self) -> u32 {
        u32::try_from(self.rows.borrow().iter().count()).unwrap_or(u32::MAX)
    }

    /// Set the account and reload if it changed.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.loaded = false;
            self.loaded_changed();
            self.chat_changed();
            self.reload();
        }
    }

    /// Set the chat and reload if it changed.
    pub fn set_chat_id(&mut self, chat_id: u32) {
        if self.chat_id != chat_id {
            self.chat_id = chat_id;
            self.loaded = false;
            // Whatever the last chat was holding is not this one's.
            self.draft = QString::default();
            self.loaded_changed();
            self.draft_changed();
            self.chat_changed();
            self.reload();
            self.load_draft();
        }
    }

    /// Read back whatever this chat is holding.
    ///
    /// Its own call rather than part of the reload: a draft is one small
    /// answer and the messages are the expensive one, and waiting for the
    /// second to show the first would put the reader's own unsent words
    /// behind a page fetch.
    fn load_draft(&mut self) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        if account_id == 0 || chat_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |text: String| {
            let Some(this) = ptr.as_pinned() else { return };
            // The reader can have moved on, or started typing, while this
            // was in flight. Neither wants an older answer written over it.
            if this.borrow().chat_id != chat_id || !this.borrow().draft.to_string().is_empty() {
                return;
            }
            if text.is_empty() {
                return;
            }
            this.borrow_mut().draft = text.into();
            this.borrow().draft_changed();
        });

        runtime.spawn(async move {
            // Null when there is none, and a whole message object when
            // there is: pinned against the real core in
            // deltachat-jsonrpc/tests/real_server.rs.
            let draft: serde_json::Value = rpc
                .call("get_draft", (account_id, chat_id))
                .await
                .unwrap_or(serde_json::Value::Null);
            done(json::str_at(&draft, "text").to_string());
        });
    }

    /// Remember `text` as this chat's draft, or forget it when empty.
    pub fn save_draft(&mut self, text: QString) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        if account_id == 0 || chat_id == 0 {
            return;
        }
        let text = text.to_string();
        if self.draft.to_string() == text {
            return;
        }
        self.draft = QString::from(text.clone());
        self.draft_changed();

        let Some((rpc, runtime)) = connection() else {
            return;
        };
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |failure: Option<String>| {
            let Some(this) = ptr.as_pinned() else { return };
            if let Some(message) = failure {
                this.borrow().error(message.into());
            }
        });

        runtime.spawn(async move {
            let result = if text.is_empty() {
                rpc.call::<_, serde_json::Value>("remove_draft", (account_id, chat_id))
                    .await
            } else {
                // misc_set_draft params: account, chat, text, file,
                // filename, quoted message, view type. Not the same tail
                // as misc_send_msg, which takes a location where this
                // takes the quote. Pinned against the real core in
                // deltachat-jsonrpc/tests/real_server.rs.
                //
                // Only the text for now: a file chosen but not sent, and a
                // reply not yet made, are still the page's and not the
                // core's.
                rpc.call::<_, serde_json::Value>(
                    "misc_set_draft",
                    (
                        account_id,
                        chat_id,
                        Some(text),
                        Option::<String>::None,
                        Option::<String>::None,
                        Option::<u32>::None,
                        Option::<String>::None,
                    ),
                )
                .await
            };
            done(result.err().map(|err| err.to_string()));
        });
    }

    /// Reload the whole chat.
    pub fn reload(&mut self) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        if account_id == 0 || chat_id == 0 {
            return;
        }
        // Anything in flight is filling rows this reload is replacing; the
        // flag being down is what tells such a reply to drop.
        self.hydrating = false;
        self.hydrating_changed();
        // Already loaded, by whoever opened this page: take it and skip
        // the round trip entirely. This is what lets the transition start
        // with the rows in place rather than fill in behind it.
        if let Some((is_group, entries, items)) = crate::prefetch::take(account_id, chat_id) {
            self.is_group = is_group;
            self.rows.borrow_mut().reset_data(rows_for(&entries, items));
            self.loaded = true;
            self.is_group_changed();
            self.loaded_changed();
            self.rows_changed();
            if !self.reading_history {
                self.mark_chat_seen();
            }
            return;
        }

        let Some((rpc, runtime)) = connection() else {
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(
            move |result: Result<(bool, Vec<Entry>, Vec<MessageListItem>), String>| {
                let Some(this) = ptr.as_pinned() else { return };
                match result {
                    Ok((is_group, entries, items)) => {
                        {
                            let mut this_mut = this.borrow_mut();
                            this_mut.is_group = is_group;
                            this_mut
                                .rows
                                .borrow_mut()
                                .reset_data(rows_for(&entries, items));
                            this_mut.loaded = true;
                        }
                        this.borrow().is_group_changed();
                        this.borrow().loaded_changed();
                        this.borrow().rows_changed();
                        // Asked now, not before the fetch: the reader can
                        // scroll away, or the app go behind, while it runs.
                        let looking = !this.borrow().reading_history;
                        if looking {
                            this.borrow_mut().mark_chat_seen();
                        }
                    }
                    Err(err) => {
                        // Loaded, in the sense that matters here: the wait
                        // is over. Leaving it false would hold an empty
                        // view with no explanation for ever.
                        this.borrow_mut().loaded = true;
                        this.borrow().loaded_changed();
                        this.borrow().error(err.into());
                    }
                }
            },
        );

        runtime.spawn(async move {
            let result = async {
                let is_group = chat_is_group(&rpc, account_id, chat_id).await;
                let entries = message_entries(&rpc, account_id, chat_id).await?;
                // A row for every message, but the content of only one
                // page. Ten thousand rows is a vector of ids; ten thousand
                // *messages* is what used to be built on the Qt thread
                // before the page could show any of them.
                let items =
                    fetch_messages(&rpc, account_id, &ids_of(opening_page(&entries, 0))).await?;
                // Marking read is the callback's job, not this one's: what
                // the reader can see is only knowable once the rows land.
                Ok::<_, String>((is_group, entries, items))
            }
            .await;
            done(result);
        });
    }

    /// Say where `message_id` is, so the view can go there.
    pub fn reveal(&mut self, message_id: u32) {
        let row = self.row_of(message_id);
        self.revealed(message_id, row);
    }

    /// Fill in the rows between `first` and `last`, and `MARGIN` either
    /// side.
    ///
    /// The view asks as it scrolls, and asks generously: rows already
    /// filled in are skipped here rather than counted there.
    pub fn hydrate(&mut self, first: i32, last: i32) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        if account_id == 0 || chat_id == 0 || self.hydrating {
            return;
        }
        let count = self.rows.borrow().iter().count();
        if count == 0 {
            return;
        }
        let first = usize::try_from(first.max(0))
            .unwrap_or(0)
            .saturating_sub(MARGIN);
        let last = usize::try_from(last.max(0))
            .unwrap_or(0)
            .saturating_add(MARGIN)
            .min(count - 1);
        if first > last {
            return;
        }
        // Only what is not there yet, and never more than a page at a time:
        // a reader who flings the view the length of a long chat would
        // otherwise ask for every row they passed.
        let wanted: Vec<u32> = self
            .rows
            .borrow()
            .iter()
            .skip(first)
            .take(last - first + 1)
            .filter(|item| !item.loaded)
            .map(|item| item.message_id)
            .take(PAGE)
            .collect();
        if wanted.is_empty() {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };

        self.hydrating = true;
        self.hydrating_changed();

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Vec<MessageListItem>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            // The flag is its own guard: `reload` puts it down, so a reply
            // for rows that have since been replaced drops here.
            if !this.borrow().hydrating || this.borrow().chat_id != chat_id {
                return;
            }
            match result {
                Ok(items) => {
                    let mut filled = 0_usize;
                    {
                        let this_mut = this.borrow_mut();
                        let mut rows = this_mut.rows.borrow_mut();
                        // By id rather than by index: rows can have been
                        // added or taken out while this ran, and writing to
                        // a remembered index would put a message where
                        // another one is. Looked up through a map rather
                        // than by scanning, because the rows are the whole
                        // chat -- a scan per fetched message is a scan of
                        // ten thousand rows fifty times over.
                        let index_of: BTreeMap<u32, usize> = rows
                            .iter()
                            .enumerate()
                            .map(|(index, row)| (row.message_id, index))
                            .collect();
                        for item in items {
                            let Some(index) = index_of.get(&item.message_id).copied() else {
                                continue;
                            };
                            // Filling a row in place leaves the order and
                            // the count alone, so the map stays true.
                            rows.change_line(index, item);
                            filled += 1;
                        }
                    }
                    this.borrow_mut().hydrating = false;
                    this.borrow().hydrating_changed();
                    if filled > 0 {
                        // Changed in place, so the view keeps its position
                        // and its delegates: nothing here moves the reader.
                        this.borrow().rows_changed();
                    }
                }
                Err(err) => {
                    this.borrow_mut().hydrating = false;
                    this.borrow().hydrating_changed();
                    this.borrow().error(err.into());
                }
            }
        });

        runtime.spawn(async move {
            done(fetch_messages(&rpc, account_id, &wanted).await);
        });
    }

    /// Apply one core event.
    pub fn handle_event(&mut self, context_id: u32, kind: QString, payload_json: QString) {
        if context_id != self.account_id || self.chat_id == 0 {
            return;
        }
        let kind = kind.to_string();
        let payload: serde_json::Value =
            serde_json::from_str(&payload_json.to_string()).unwrap_or_default();
        let event_chat = json::u32_at(&payload, "chatId");
        // MsgsChanged carries chatId 0 for "several chats", and an
        // overflow carries none at all.
        if event_chat != 0 && event_chat != self.chat_id {
            return;
        }

        match kind.as_str() {
            // New or changed content: take in what we do not have yet.
            "IncomingMsg" | "MsgsChanged" | "MsgDeleted" => self.sync_rows(),
            // The core dropped events it could not queue, so what this
            // model holds may already be wrong in ways no later event will
            // mention. Start again rather than patch.
            "EventChannelOverflow" => self.reload(),
            // Delivery state only: refresh the one row it names.
            "MsgDelivered" | "MsgRead" | "MsgFailed" => {
                if let Some(message_id) = json::u32_opt(&payload, "msgId") {
                    self.refresh_one(message_id);
                }
            }
            _ => {}
        }
    }

    /// Bring the rows in line with the chat's current id list.
    ///
    /// There is no window to reconcile against any more: the rows *are* the
    /// id list, one for each, so the only differences are messages that
    /// have arrived and messages that have gone. Both are done in place, so
    /// neither moves the reader.
    fn sync_rows(&mut self) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        if account_id == 0 || chat_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Vec<Entry>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            if this.borrow().chat_id != chat_id {
                return;
            }
            let entries = match result {
                Ok(entries) => entries,
                Err(err) => {
                    this.borrow().error(err.into());
                    return;
                }
            };
            let ids = ids_of(&entries);

            let current: Vec<u32> = this
                .borrow()
                .rows
                .borrow()
                .iter()
                .map(|item| item.message_id)
                .collect();
            if current == ids {
                return;
            }

            let present: HashSet<u32> = ids.iter().copied().collect();
            let kept: Vec<u32> = current
                .iter()
                .copied()
                .filter(|id| present.contains(id))
                .collect();
            // Whatever is left has to still be the front of the chat, or
            // this is a reorder rather than an arrival and a removal.
            if !ids.starts_with(&kept) {
                this.borrow_mut().reload();
                return;
            }

            let gone: Vec<usize> = current
                .iter()
                .enumerate()
                .filter(|(_, id)| !present.contains(id))
                .map(|(index, _)| index)
                .collect();
            if !gone.is_empty() {
                let this_mut = this.borrow_mut();
                let mut rows = this_mut.rows.borrow_mut();
                // Backwards, so each index still means what it did when it
                // was worked out.
                for index in gone.into_iter().rev() {
                    rows.remove(index);
                }
            }

            let arrived: Vec<u32> = ids[kept.len()..].to_vec();
            this.borrow().rows_changed();
            if !arrived.is_empty() {
                this.borrow_mut().absorb(arrived);
            }
        });

        runtime.spawn(async move {
            done(message_entries(&rpc, account_id, chat_id).await);
        });
    }

    /// Fetch messages that have just arrived and put them at the end.
    ///
    /// Fetched rather than left as placeholders: what arrived is what the
    /// reader is told about, and a placeholder does not know whether it is
    /// theirs or somebody else's.
    fn absorb(&mut self, ids: Vec<u32>) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        let Some((rpc, runtime)) = connection() else {
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Vec<MessageListItem>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            if this.borrow().chat_id != chat_id {
                return;
            }
            let Ok(items) = result else { return };
            let mut appended = Vec::new();
            {
                let this_mut = this.borrow_mut();
                let mut rows = this_mut.rows.borrow_mut();
                for item in items {
                    // A send in flight may have pushed a row this fetch
                    // also carries: the id list was read before that reply
                    // landed. Appending it again is the duplicate that
                    // survives until reload.
                    if rows.iter().any(|row| row.message_id == item.message_id) {
                        continue;
                    }
                    appended.push(item.clone());
                    rows.push(item);
                }
            }
            if appended.is_empty() {
                return;
            }
            this.borrow().rows_changed();
            // Asked after the push rather than before the fetch, so a
            // reader who scrolled away while it ran is not credited with
            // seeing what arrived.
            let looking = !this.borrow().reading_history;
            let incoming = u32::try_from(appended.iter().filter(|item| !item.is_outgoing).count())
                .unwrap_or(u32::MAX);
            if looking {
                this.borrow().mark_items_seen(appended);
            }
            if incoming > 0 {
                this.borrow().arrived(incoming);
            }
        });

        runtime.spawn(async move {
            done(fetch_messages(&rpc, account_id, &ids).await);
        });
    }

    /// Re-read one message and replace its row.
    fn refresh_one(&mut self, message_id: u32) {
        let account_id = self.account_id;
        if !self
            .rows
            .borrow()
            .iter()
            .any(|item| item.message_id == message_id)
        {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Vec<MessageListItem>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            if let Ok(items) = result {
                if let Some(item) = items.into_iter().next() {
                    // Found again here rather than carried from before the
                    // fetch: a reload in between replaces the rows, and an
                    // index from the old ones addresses the wrong message
                    // -- or, once the list is shorter, nothing at all.
                    // `change_line` indexes without checking.
                    let existing = this
                        .borrow()
                        .rows
                        .borrow()
                        .iter()
                        .position(|row| row.message_id == item.message_id);
                    let Some(index) = existing else { return };
                    this.borrow_mut().rows.borrow_mut().change_line(index, item);
                    this.borrow().rows_changed();
                }
            }
        });

        runtime.spawn(async move {
            let result = fetch_messages(&rpc, account_id, &[message_id]).await;
            done(result);
        });
    }

    /// Which row a message is in, or -1 when it is not loaded.
    pub fn row_of(&self, message_id: u32) -> i32 {
        self.rows
            .borrow()
            .iter()
            .position(|row| row.message_id == message_id)
            .and_then(|index| i32::try_from(index).ok())
            .unwrap_or(-1)
    }

    /// Mark every unread message now loaded as read.
    pub fn mark_seen_all(&mut self) {
        let account_id = self.account_id;
        if account_id == 0 {
            return;
        }
        let items: Vec<MessageListItem> = self.rows.borrow().iter().cloned().collect();
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        runtime.spawn(async move {
            mark_seen(&rpc, account_id, &items).await;
        });
    }

    /// Send read receipts for these rows, whichever of them are unread and
    /// incoming. Takes what to mark rather than reading the model, so a
    /// sync marks what it just added instead of the whole chat again.
    fn mark_items_seen(&self, items: Vec<MessageListItem>) {
        let account_id = self.account_id;
        if account_id == 0 || items.is_empty() {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        runtime.spawn(async move {
            mark_seen(&rpc, account_id, &items).await;
        });
    }

    /// Someone started, or stopped, looking at what is on screen.
    ///
    /// Marking on the way in is not enough. The reader is not looking
    /// while the page transitions in, and a local fetch routinely
    /// finishes first -- so the check made when the load returns finds
    /// `reading_history` still true and skips. Nothing afterwards marks
    /// anything but *arriving* messages, so the chat stays unread and its
    /// badge never clears. Marking here catches the moment the page
    /// settles, and the app coming back to the foreground with a chat
    /// already open.
    pub fn set_reading_history(&mut self, reading_history: bool) {
        if self.reading_history == reading_history {
            return;
        }
        self.reading_history = reading_history;
        self.reading_history_changed();
        if !reading_history {
            self.mark_chat_seen();
        }
    }

    /// The whole chat has been looked at: tell the core so the chat list
    /// drops its badge, and send the read receipts for what is loaded.
    /// `mark_seen_all` alone does not do the first.
    fn mark_chat_seen(&mut self) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        if account_id == 0 || chat_id == 0 {
            return;
        }
        let items: Vec<MessageListItem> = self.rows.borrow().iter().cloned().collect();
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        runtime.spawn(async move {
            let _ = rpc
                .call::<_, ()>("marknoticed_chat", (account_id, chat_id))
                .await;
            mark_seen(&rpc, account_id, &items).await;
        });
    }

    /// Delete a message, here and on the mail server.
    pub fn delete_message(&mut self, message_id: u32) {
        self.act("delete_messages", message_id);
    }

    /// Try a failed message again.
    pub fn resend_message(&mut self, message_id: u32) {
        self.act("resend_messages", message_id);
    }

    /// Send a copy of one message into another chat.
    ///
    /// Not routed through the shared `act` helper: that calls
    /// `method(account_id, [ids])`, and forwarding needs the destination
    /// too. Nothing is refreshed afterwards either -- the copy lands in
    /// another chat, which this model is not the one showing.
    pub fn forward_to(&mut self, message_id: u32, chat_id: u32) {
        let account_id = self.account_id;
        if account_id == 0 || message_id == 0 || chat_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<(), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            if let Err(err) = result {
                this.borrow().error(err.into());
            }
        });

        runtime.spawn(async move {
            let result = rpc
                .call::<_, ()>("forward_messages", (account_id, vec![message_id], chat_id))
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Call `method` with `(account, [message])`, then re-read that row.
    ///
    /// A deletion changes the id list, so the event it raises reloads the
    /// model on its own. A resend does not: the list is the same, the sync
    /// fetches only ids it is missing, and the row keeps the failed state
    /// it had -- mark and "Send again" and all -- until something else
    /// refetches it.
    fn act(&mut self, method: &'static str, message_id: u32) {
        let account_id = self.account_id;
        if account_id == 0 || message_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<(), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(()) => this.borrow_mut().refresh_one(message_id),
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            let result = rpc
                .call::<_, ()>(method, (account_id, vec![message_id]))
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Send a plain-text message.
    pub fn send(&mut self, text: QString) {
        self.send_message(text.to_string(), None);
    }

    /// Send a message with a file attached.
    pub fn send_file(&mut self, text: QString, file_path: QString) {
        let path = local_path(&file_path.to_string());
        if path.is_empty() {
            self.error(QString::from("no file to send"));
            return;
        }
        let name = file_name_of(&path);
        self.send_message(text.to_string(), Some((path, name)));
    }

    /// The one send. `file` is the path the core should attach and the name
    /// the recipient should see.
    fn send_message(&mut self, text: String, file: Option<(String, String)>) {
        // One at a time. The UI disables its button too, but the guard
        // belongs here: the button is not the only way in, and the model is
        // what knows whether the core has answered.
        if self.sending {
            return;
        }
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        // Nothing to send to yet: the same guard every other call makes,
        // rather than a round trip to the core for its error.
        if account_id == 0 || chat_id == 0 {
            return;
        }
        let quoted = self.quoted_message_id;
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };

        self.sending = true;
        self.sending_changed();

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<MessageListItem, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            // Cleared before anything else, and on both paths: a send that
            // failed has to leave the reader able to try again.
            this.borrow_mut().sending = false;
            this.borrow().sending_changed();
            match result {
                Ok(item) => {
                    let message_id = item.message_id;
                    {
                        let this_mut = this.borrow_mut();
                        let mut rows = this_mut.rows.borrow_mut();
                        // The event for our own send can beat this reply,
                        // in which case the row is already there. Wherever
                        // the reader happens to be in the chat, the message
                        // goes at the end of it and nothing else moves.
                        let existing = rows.iter().position(|row| row.message_id == message_id);
                        if let Some(index) = existing {
                            rows.change_line(index, item);
                        } else {
                            rows.push(item);
                        }
                    }
                    this.borrow().rows_changed();
                    // Cleared once the message is really gone, not before
                    // it is sent: a send that fails leaves the reader with
                    // the reply they chose rather than silently dropping it.
                    if quoted != 0 {
                        this.borrow_mut().quoted_message_id = 0;
                        this.borrow().quote_changed();
                    }
                    this.borrow().sent(message_id);
                }
                Err(err) => this.borrow().error(err.into()),
            }
        });

        let (path, name) = file.unzip();
        runtime.spawn(async move {
            // misc_send_msg params: account, chat, text, file, filename,
            // location, quoted_message_id. Pinned against the real core by
            // deltachat-jsonrpc/tests/real_server.rs, which sends one of
            // each.
            let result = rpc
                .call::<_, (u32, serde_json::Value)>(
                    "misc_send_msg",
                    (
                        account_id,
                        chat_id,
                        // A caption-only send is a text message; an empty
                        // string here would be a message whose body is "".
                        (!text.is_empty()).then_some(text),
                        path,
                        name,
                        Option::<(f64, f64)>::None,
                        (quoted != 0).then_some(quoted),
                    ),
                )
                .await
                .map(|(message_id, message)| row_from(message_id, &message))
                .map_err(|err| err.to_string());
            done(result);
        });
    }
}

/// The chat's messages, oldest first, each under the day it belongs to.
///
/// The last argument asks the core to interleave day markers, which it
/// gives as the local midnight starting each day -- checked against the
/// real `deltachat-rpc-server` in three zones, because a marker at *UTC*
/// midnight would put every message in a zone behind UTC under yesterday.
/// `local_day_number` is the same function the fetched rows go through, so
/// a row's day cannot change when it is filled in.
pub(crate) async fn message_entries(
    rpc: &RpcClient,
    account_id: u32,
    chat_id: u32,
) -> Result<Vec<Entry>, String> {
    let items: Vec<serde_json::Value> = rpc
        .call("get_message_list_items", (account_id, chat_id, false, true))
        .await
        .map_err(|err| err.to_string())?;
    let mut entries = Vec::with_capacity(items.len());
    // Carried forward from the last marker seen: the core emits one before
    // each day's first message, so every message after it is under that day.
    let mut day_number = 0;
    for item in &items {
        match json::str_at(item, "kind") {
            "dayMarker" => {
                if let Some(timestamp) = item.get("timestamp").and_then(serde_json::Value::as_i64) {
                    day_number = local_day_number(timestamp);
                }
            }
            "message" => {
                // `rename_all` sits at the enum level upstream, renaming
                // variants but not fields: this really is snake_case on the
                // wire. `msgId` accepted in case that changes.
                if let Some(message_id) =
                    json::u32_opt(item, "msg_id").or_else(|| json::u32_opt(item, "msgId"))
                {
                    entries.push(Entry {
                        message_id,
                        day_number,
                    });
                }
            }
            _ => {}
        }
    }
    Ok(entries)
}

/// Fetch several messages in one call. The old code asked for them one at a
/// time, which cost a round trip per message on every refresh.
pub(crate) async fn fetch_messages(
    rpc: &RpcClient,
    account_id: u32,
    ids: &[u32],
) -> Result<Vec<MessageListItem>, String> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let loaded: BTreeMap<u32, serde_json::Value> = rpc
        .call("get_messages", (account_id, ids))
        .await
        .map_err(|err| err.to_string())?;
    Ok(ids
        .iter()
        .filter_map(|id| {
            let message = loaded.get(id)?;
            // A message the core could not load comes back as
            // `{kind: "loadingError"}`; skip it rather than render a blank.
            if json::str_at(message, "kind") == "loadingError" {
                return None;
            }
            Some(row_from(*id, message))
        })
        .collect())
}

/// Mark the incoming messages among these read: clears their fresh state
/// here and on the other devices, and sends the read receipt the sender
/// asked for. `marknoticed_chat` alone does neither.
async fn mark_seen(rpc: &RpcClient, account_id: u32, items: &[MessageListItem]) {
    let unseen: Vec<u32> = items
        .iter()
        .filter(|item| !item.is_outgoing && UNSEEN_STATES.contains(&item.state))
        .map(|item| item.message_id)
        .collect();
    if unseen.is_empty() {
        return;
    }
    let _ = rpc
        .call::<_, ()>("markseen_msgs", (account_id, unseen))
        .await;
}

/// True for a chat where a message has to say who sent it.
pub(crate) async fn chat_is_group(rpc: &RpcClient, account_id: u32, chat_id: u32) -> bool {
    let info: serde_json::Value = match rpc.call("get_basic_chat_info", (account_id, chat_id)).await
    {
        Ok(info) => info,
        Err(_) => return false,
    };
    !matches!(json::str_at(&info, "chatType"), "Single" | "")
}

/// Days since the Unix epoch on which this instant fell, in the viewer's
/// own timezone.
///
/// The zone is asked what its offset was *at that instant*, which is the
/// whole point: applying the offset in force now puts a message from the
/// other side of a daylight-saving change an hour out, so anything within
/// an hour of local midnight sits under the wrong heading -- and moves as
/// the year turns.
///
/// Falls back to UTC for a timestamp the zone cannot place, which is a
/// wrong heading rather than a missing list.
#[must_use]
pub fn local_day_number(timestamp: i64) -> i64 {
    let Some(when) = Local.timestamp_opt(timestamp, 0).single() else {
        return timestamp.div_euclid(86_400);
    };
    // Whole days between two dates, so a zone that is behind UTC does not
    // borrow a day the way `(timestamp + offset) / 86400` would.
    when.date_naive()
        .signed_duration_since(DateTime::UNIX_EPOCH.date_naive())
        .num_days()
}

/// One row from the core's message object.
/// A row standing in for a message that has not been fetched.
///
/// The model holds one of these per message in the chat from the moment the
/// id list arrives, which is what makes the first message row 0 and keeps it
/// there however far the reader scrolls.
fn placeholder(entry: Entry) -> MessageListItem {
    MessageListItem {
        message_id: entry.message_id,
        // Known before the message is, so the row is under the right day
        // heading from the start and that heading never changes size.
        day_number: entry.day_number,
        loaded: false,
        ..MessageListItem::default()
    }
}

fn row_from(message_id: u32, message: &serde_json::Value) -> MessageListItem {
    let timestamp = json::i64_at(message, "timestamp");
    let sender_name = match json::str_at(message, "overrideSenderName") {
        "" => json::text(message, "/sender/displayName"),
        name => name.into(),
    };
    MessageListItem {
        message_id,
        loaded: true,
        text: json::text(message, "/text"),
        // Contact id 1 is the well-known DC_CONTACT_ID_SELF.
        is_outgoing: json::u32_opt(message, "fromId") == Some(1),
        timestamp,
        day_number: local_day_number(timestamp),
        show_padlock: json::flag(message, "showPadlock"),
        state: json::u32_at(message, "state"),
        sender_name,
        sender_color: json::text(message, "/sender/color"),
        is_info: json::flag(message, "isInfo"),
        // Absent in the abbreviated object the fake core returns from
        // misc_send_msg, and false is right there: a message composed here
        // is never a forward. Real forwards arrive through get_messages,
        // which returns the full shape.
        is_forwarded: json::flag(message, "isForwarded"),
        quote_text: json::text(message, "/quote/text"),
        quote_author: json::text(message, "/quote/authorDisplayName"),
        file_path: json::text(message, "/file"),
        file_name: json::text(message, "/fileName"),
        view_type: json::text(message, "/viewType"),
        // Often 0 even for a picture: the core probes PNG and JPEG but
        // returned 0x0 for a valid GIF, so nothing may divide by these.
        image_width: json::i32_at(message, "dimensionsWidth"),
        image_height: json::i32_at(message, "dimensionsHeight"),
        file_mime: json::text(message, "/fileMime"),
        file_bytes: message
            .get("fileBytes")
            .and_then(serde_json::Value::as_u64)
            .map_or(0.0, |bytes| {
                // Through f64 because QML has no 64-bit integer, and
                // saturating at 4 GiB because f64 is only exact to 2^53.
                // Nothing real is lost: the core will not carry an
                // attachment anywhere near either bound.
                f64::from(u32::try_from(bytes).unwrap_or(u32::MAX))
            }),
        vcard_name: json::text(message, "/vcardContact/displayName"),
        vcard_addr: json::text(message, "/vcardContact/addr"),
        vcard_color: json::text(message, "/vcardContact/color"),
    }
}

/// The local path a picker handed back.
///
/// Silica's pickers report `filePath`, which is already a plain path, but
/// `selectedContent` and anything that has been through a `url` property
/// arrive as `file://` URLs with the awkward characters percent-encoded.
/// Both reach `send_file`, and the core takes a path.
fn local_path(raw: &str) -> String {
    let path = raw.strip_prefix("file://").unwrap_or(raw);
    if !path.contains('%') {
        return path.to_string();
    }
    let mut out = Vec::with_capacity(path.len());
    let bytes = path.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        // Anything that is not a complete, valid escape is a literal '%':
        // a file really can be called "100%.png".
        // Both digits checked here rather than left to `from_str_radix`,
        // which accepts a leading sign: without this, "%+A" would decode.
        let decoded = (bytes[index] == b'%' && index + 2 < bytes.len())
            .then(|| &bytes[index + 1..index + 3])
            .filter(|hex| hex.iter().all(u8::is_ascii_hexdigit))
            .and_then(|hex| std::str::from_utf8(hex).ok())
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        if let Some(byte) = decoded {
            out.push(byte);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    // A percent-escape that does not spell UTF-8 is not a path we can use;
    // the undecoded string at least names something.
    String::from_utf8(out).unwrap_or_else(|_| path.to_string())
}

/// What the recipient should see this file called.
///
/// The core would derive its own name from the path, and for a gallery
/// picture that path is often the camera's serial-number filename. This is
/// the same thing, but ours to change.
fn file_name_of(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{file_name_of, local_path};

    #[test]
    fn a_picked_path_survives_whichever_way_it_arrives() {
        assert_eq!(local_path("/home/user/a.png"), "/home/user/a.png");
        assert_eq!(local_path("file:///home/user/a.png"), "/home/user/a.png");
        assert_eq!(
            local_path("file:///home/user/holiday%20photo.png"),
            "/home/user/holiday photo.png"
        );
        // Multi-byte characters are percent-encoded per byte.
        assert_eq!(
            local_path("file:///home/user/caf%C3%A9.png"),
            "/home/user/café.png"
        );
    }

    #[test]
    fn a_literal_percent_is_not_an_escape() {
        assert_eq!(local_path("/home/user/100%.png"), "/home/user/100%.png");
        assert_eq!(local_path("/home/user/%zz.png"), "/home/user/%zz.png");
        // Truncated at the end, so there is nothing to decode.
        assert_eq!(local_path("/home/user/x%2"), "/home/user/x%2");
        // `from_str_radix` would take the sign and decode this to 0x0a.
        assert_eq!(local_path("/home/user/%+a.png"), "/home/user/%+a.png");
    }

    #[test]
    fn the_name_is_the_last_component() {
        assert_eq!(file_name_of("/home/user/a.png"), "a.png");
        assert_eq!(file_name_of("a.png"), "a.png");
        assert_eq!(file_name_of("/home/user/"), "user");
        assert_eq!(file_name_of(""), "");
    }
}

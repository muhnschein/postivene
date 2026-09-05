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
use crate::{links, markdown};

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
    /// The chat's name as the core shows it: the group's, or the contact's
    /// display name. Empty until read. Re-read on every event that could
    /// have changed it, so the header over the messages follows a rename
    /// made on the page beside it, or on another device.
    pub chat_name: qt_property!(QString; NOTIFY chat_name_changed),
    /// Emitted when `chat_name` is re-read and differs.
    pub chat_name_changed: qt_signal!(),

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

    /// Take known tracking parameters out of the links in what is sent.
    /// The reader's setting, handed in by the page; see `links.rs`.
    pub clean_links: qt_property!(bool; NOTIFY clean_links_changed),
    /// Emitted when [`Self::clean_links`] changes.
    pub clean_links_changed: qt_signal!(),

    /// Fetch the rest of a message the core holds only the header of.
    /// The core announces the result as a change to the message.
    pub download_full: qt_method!(fn(&mut self, message_id: u32)),

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
    /// Send a recording as a voice message: the core's `Voice` view type,
    /// which is what draws it as one at the other end rather than as a
    /// music file. The core decides the type from the file otherwise, and
    /// a recording is a sound file like any other to it.
    pub send_voice: qt_method!(fn(&mut self, file_path: QString)),
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
    /// Put `emoji` on a message as this account's reaction -- or take it
    /// off again when it is the one already there, which is what a second
    /// tap on the same emoji means. One reaction per person: a different
    /// emoji replaces the old one, as the reference clients do.
    pub react: qt_method!(fn(&mut self, message_id: u32, emoji: QString)),
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
        // The name is its own small fetch, on both paths below: the
        // prefetch does not carry it, and the page opens with the name the
        // list handed it anyway.
        self.refresh_name();
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
            "IncomingMsg" | "MsgDeleted" => self.sync_rows(),
            // The same, and one message re-read when the event names
            // one. A download the limit held back lands as this event
            // with the message's id and nothing else different: the id
            // list is as it was, so a sync alone found nothing to do and
            // the row said "Downloading…" until the chat was opened
            // again. An edit made on another device arrives the same way.
            "MsgsChanged" => {
                self.sync_rows();
                if let Some(message_id) = json::u32_opt(&payload, "msgId").filter(|id| *id != 0) {
                    self.refresh_one(message_id);
                }
            }
            // The core dropped events it could not queue, so what this
            // model holds may already be wrong in ways no later event will
            // mention. Start again rather than patch.
            "EventChannelOverflow" => self.reload(),
            // Delivery state only: refresh the one row it names. A
            // reaction is the same shape of change -- the id list is
            // untouched, since a reaction is a hidden message, and only the
            // row it lands on has anything new to show.
            "MsgDelivered" | "MsgRead" | "MsgFailed" | "ReactionsChanged" => {
                if let Some(message_id) = json::u32_opt(&payload, "msgId") {
                    self.refresh_one(message_id);
                }
            }
            // A rename, of the group or of the contact behind a one-to-one
            // chat; the core does not say whose contact changed.
            "ChatModified" | "ContactsChanged" => self.refresh_name(),
            _ => {}
        }
    }

    /// Re-read the chat's name: one `get_basic_chat_info`, cheap enough
    /// to do on every event that could have changed it. Nothing is said
    /// when it did not.
    fn refresh_name(&mut self) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        if account_id == 0 || chat_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |name: Option<String>| {
            let Some(this) = ptr.as_pinned() else { return };
            let Some(name) = name else { return };
            // Answered for a chat this model has moved on from.
            if this.borrow().chat_id != chat_id {
                return;
            }
            if this.borrow().chat_name.to_string() != name {
                this.borrow_mut().chat_name = name.into();
                this.borrow().chat_name_changed();
            }
        });
        runtime.spawn(async move {
            let name = rpc
                .call::<_, serde_json::Value>("get_basic_chat_info", (account_id, chat_id))
                .await
                .ok()
                .map(|info| json::str_at(&info, "name").to_string());
            done(name);
        });
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

    /// Fetch the rest of a message held back by the download limit.
    ///
    /// Its own call rather than `act`: the method takes one id, not a
    /// list, and the row is re-read on the `MsgsChanged` the core sends
    /// once the download lands rather than on this call's return, which
    /// only says the download was started.
    pub fn download_full(&mut self, message_id: u32) {
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
                // The state moves to InProgress at once; show that.
                Ok(()) => this.borrow_mut().refresh_one(message_id),
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            let result = rpc
                .call::<_, ()>("download_full_message", (account_id, message_id))
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
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

    /// React to a message, or take this account's reaction off it.
    ///
    /// The toggle is decided here, from the row: the core's call sets the
    /// whole list of this account's reactions, so "the same emoji again"
    /// has to become an empty list before it is sent. Not `act`: the call
    /// takes the reaction as well as the message. The row is re-read on
    /// the answer, and again on the `ReactionsChanged` the core sends --
    /// the first for the tap to show at once, the second because that is
    /// how a reaction from anyone else arrives too.
    pub fn react(&mut self, message_id: u32, emoji: QString) {
        let account_id = self.account_id;
        let emoji = emoji.to_string().trim().to_string();
        if account_id == 0 || message_id == 0 || emoji.is_empty() {
            return;
        }
        let mine = self
            .rows
            .borrow()
            .iter()
            .find(|row| row.message_id == message_id)
            .map(|row| row.my_reaction.to_string())
            .unwrap_or_default();
        let reaction: Vec<String> = if mine == emoji {
            Vec::new()
        } else {
            vec![emoji]
        };
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
            // send_reaction params: account, message, the reactions this
            // account now has on it. It answers with the id of the hidden
            // message that carries the reaction, which nothing here needs.
            // Pinned against the real core in
            // deltachat-jsonrpc/tests/real_server.rs.
            let result = rpc
                .call::<_, serde_json::Value>("send_reaction", (account_id, message_id, reaction))
                .await
                .map(|_| ())
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
        self.send_message(self.outgoing_text(&text.to_string()), None);
    }

    /// The text as it goes out: with its links cleaned when the reader
    /// asked for that, as written otherwise.
    fn outgoing_text(&self, text: &str) -> String {
        if self.clean_links {
            links::clean_text(text)
        } else {
            text.to_string()
        }
    }

    /// Send a message with a file attached.
    pub fn send_file(&mut self, text: QString, file_path: QString) {
        let path = local_path(&file_path.to_string());
        if path.is_empty() {
            self.error(QString::from("no file to send"));
            return;
        }
        let name = file_name_of(&path);
        self.send_message(self.outgoing_text(&text.to_string()), Some((path, name)));
    }

    /// Send a recording as a voice message.
    pub fn send_voice(&mut self, file_path: QString) {
        let path = local_path(&file_path.to_string());
        if path.is_empty() {
            self.error(QString::from("no recording to send"));
            return;
        }
        self.send_message(String::new(), Some((path, String::new())));
    }

    /// The one send. `file` is the path the core should attach and the name
    /// the recipient should see -- an empty name for a voice message,
    /// which is not named for anyone.
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

        // A file with no name is a voice message: the one kind the core
        // has to be told, since to it a recording is a sound file like any
        // other. It takes the shape `send_msg` takes and `misc_send_msg`
        // does not, and answers with the id alone, so the row is fetched
        // the way every other row is.
        let voice = matches!(&file, Some((_, name)) if name.is_empty());
        let (path, name) = file.unzip();
        runtime.spawn(async move {
            let result = if voice {
                // send_msg params: account, chat, MessageData -- camelCase
                // fields, the view type by its variant name. Pinned
                // against the real core by
                // deltachat-jsonrpc/tests/real_server.rs.
                let data = serde_json::json!({
                    "file": path,
                    "viewtype": "Voice",
                    "quotedMessageId": (quoted != 0).then_some(quoted),
                });
                match rpc
                    .call::<_, u32>("send_msg", (account_id, chat_id, data))
                    .await
                {
                    Ok(message_id) => fetch_messages(&rpc, account_id, &[message_id])
                        .await
                        .and_then(|items| {
                            items
                                .into_iter()
                                .next()
                                .ok_or_else(|| "the core lost the voice message".to_string())
                        }),
                    Err(err) => Err(err.to_string()),
                }
            } else {
                // misc_send_msg params: account, chat, text, file, filename,
                // location, quoted_message_id. Pinned against the real core by
                // deltachat-jsonrpc/tests/real_server.rs, which sends one of
                // each.
                rpc.call::<_, (u32, serde_json::Value)>(
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
                .map_err(|err| err.to_string())
            };
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
    let text = json::str_at(message, "text");
    let is_outgoing = json::u32_opt(message, "fromId") == Some(1);
    let state = json::u32_at(message, "state");
    let file_path = json::str_at(message, "file");
    let view_type = json::str_at(message, "viewType");
    // The core probes PNG and JPEG for their size and returned 0x0 for a
    // valid GIF. A row that knows its picture's shape before the picture
    // is decoded is a row that does not change height when it lands, so
    // a GIF's is read out of its own header: ten bytes, at the front.
    let (image_width, image_height) = match (
        json::i32_at(message, "dimensionsWidth"),
        json::i32_at(message, "dimensionsHeight"),
    ) {
        (0, 0) if view_type == "Gif" => gif_dimensions(file_path).unwrap_or((0, 0)),
        // Nothing sizes a video: the core does not probe one, and only
        // some clients write the size in when they send. The file's own
        // track header says, and with it the row is the video's shape,
        // upright or not, rather than a 16:9 guess.
        (0, 0) if view_type == "Video" => video_dimensions(file_path).unwrap_or((0, 0)),
        known => known,
    };
    MessageListItem {
        message_id,
        loaded: true,
        text: text.into(),
        // Both renderings made here, once per fetch, so a row can switch
        // between them on the reader's setting without a round trip.
        styled_text: markdown::render(text).into(),
        plain_text: markdown::strip(text).into(),
        // Absent from the abbreviated object a send answers with; a
        // message composed here is never one held back.
        download_state: json::text(message, "/downloadState"),
        // Contact id 1 is the well-known DC_CONTACT_ID_SELF.
        is_outgoing,
        timestamp,
        day_number: local_day_number(timestamp),
        show_padlock: json::flag(message, "showPadlock"),
        state,
        // Decided once, as the row is built: the row is rebuilt when the
        // core says the message changed, and by then it has usually been
        // marked seen -- but a GIF that started playing keeps its runs,
        // and one the reader has not scrolled to yet is still new to them.
        is_new: !is_outgoing && UNSEEN_STATES.contains(&state),
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
        file_path: file_path.into(),
        file_name: json::text(message, "/fileName"),
        view_type: view_type.into(),
        // Still 0 for anything neither the core nor the header read above
        // could size, so nothing may divide by these.
        image_width,
        image_height,
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
        reactions: reactions_json(message).into(),
        my_reaction: own_reaction(message).into(),
    }
}

/// A GIF's size from its logical screen descriptor: the two little-endian
/// 16-bit fields after the six-byte signature. Nothing else in the file is
/// read, and nothing is decoded; a file that is not there yet, or not a
/// GIF, is `None`.
fn gif_dimensions(path: &str) -> Option<(i32, i32)> {
    use std::io::Read;
    if path.is_empty() {
        return None;
    }
    let mut header = [0_u8; 10];
    std::fs::File::open(path)
        .ok()?
        .read_exact(&mut header)
        .ok()?;
    if &header[..4] != b"GIF8" {
        return None;
    }
    let width = i32::from(u16::from_le_bytes([header[6], header[7]]));
    let height = i32::from(u16::from_le_bytes([header[8], header[9]]));
    (width > 0 && height > 0).then_some((width, height))
}

/// A video's frame size as it is to be shown, from its MP4 track header.
///
/// The `tkhd` box carries the track's width and height and a matrix,
/// and a phone stores a video taken upright on its side with a quarter
/// turn in the matrix; so a matrix that turns is a swapped size. Only
/// box headers are read, and nothing is decoded: the file is walked box
/// by box (`ftyp`, then whatever comes, `mdat` skipped by its size,
/// `moov` and its tracks stepped into), which works whether the movie
/// header sits before or after the media. A file that is not there yet,
/// not an ISO media file (MP4, MOV, 3GP), or without a video track is
/// `None`.
fn video_dimensions(path: &str) -> Option<(i32, i32)> {
    if path.is_empty() {
        return None;
    }
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let mut at = 0;
    let mut first = true;
    while at + 8 <= len {
        let (size, kind) = mp4::box_header(&mut file, at, len)?;
        if first && &kind != b"ftyp" {
            return None;
        }
        first = false;
        if &kind == b"moov" {
            return mp4::video_track_size(&mut file, at + 8, at + size);
        }
        at += size;
    }
    None
}

/// The little of the ISO media file layout that `video_dimensions` walks.
mod mp4 {
    use std::io::{Read, Seek, SeekFrom};

    /// The box at `at`: its whole size, header included, and its type.
    /// `None` for a size that cannot be right, which ends the walk.
    pub(super) fn box_header(
        file: &mut std::fs::File,
        at: u64,
        end: u64,
    ) -> Option<(u64, [u8; 4])> {
        let mut header = [0_u8; 8];
        file.seek(SeekFrom::Start(at)).ok()?;
        file.read_exact(&mut header).ok()?;
        let kind = [header[4], header[5], header[6], header[7]];
        let size = match u32::from_be_bytes([header[0], header[1], header[2], header[3]]) {
            // To the end of the file.
            0 => end.checked_sub(at)?,
            // A 64-bit size follows the type.
            1 => {
                let mut large = [0_u8; 8];
                file.read_exact(&mut large).ok()?;
                u64::from_be_bytes(large)
            }
            size => u64::from(size),
        };
        (size >= 8 && at + size <= end).then_some((size, kind))
    }

    /// The first track between `from` and `to` with a size: audio tracks
    /// have none.
    pub(super) fn video_track_size(
        file: &mut std::fs::File,
        from: u64,
        to: u64,
    ) -> Option<(i32, i32)> {
        let mut at = from;
        while at + 8 <= to {
            let (size, kind) = box_header(file, at, to)?;
            if &kind == b"trak" {
                if let Some(found) = track_size(file, at + 8, at + size) {
                    return Some(found);
                }
            }
            at += size;
        }
        None
    }

    /// The size the track's `tkhd` gives, turned as its matrix says.
    fn track_size(file: &mut std::fs::File, from: u64, to: u64) -> Option<(i32, i32)> {
        let mut at = from;
        while at + 8 <= to {
            let (size, kind) = box_header(file, at, to)?;
            if &kind == b"tkhd" {
                return header_size(file, at + 8, size - 8);
            }
            at += size;
        }
        None
    }

    /// The width and height in a `tkhd` body, swapped for a quarter
    /// turn. The fields before the matrix are longer in version 1, whose
    /// times are 64-bit; the matrix is nine 16.16 numbers, and a quarter
    /// turn has zeros where the identity has ones.
    fn header_size(file: &mut std::fs::File, at: u64, size: u64) -> Option<(i32, i32)> {
        // Version 1's fields reach 96 bytes with the size at the end.
        let mut body = [0_u8; 96];
        let wanted = usize::try_from(size.min(96)).ok()?;
        file.seek(SeekFrom::Start(at)).ok()?;
        file.read_exact(&mut body[..wanted]).ok()?;
        let matrix = if body[0] == 1 { 52 } else { 40 };
        if wanted < matrix + 44 {
            return None;
        }
        let fixed = |offset: usize| {
            i32::from_be_bytes([
                body[offset],
                body[offset + 1],
                body[offset + 2],
                body[offset + 3],
            ])
        };
        let (a, d) = (fixed(matrix), fixed(matrix + 16));
        let width = fixed(matrix + 36) >> 16;
        let height = fixed(matrix + 40) >> 16;
        if width <= 0 || height <= 0 {
            return None;
        }
        if a == 0 && d == 0 {
            Some((height, width))
        } else {
            Some((width, height))
        }
    }
}

/// The reactions on a message, as the row carries them.
///
/// The core hands them over already counted and sorted, most frequent
/// first, under `reactions.reactions`; this keeps that order and drops
/// the per-contact breakdown, which nothing draws. Null, absent, or a
/// list with nothing in it all read as no reactions. An emoji is whatever
/// the other end sent -- the core does not check that it is one -- so the
/// row shows it as plain text.
fn reactions_json(message: &serde_json::Value) -> String {
    let Some(list) = message
        .pointer("/reactions/reactions")
        .and_then(serde_json::Value::as_array)
    else {
        return String::new();
    };
    let chips: Vec<serde_json::Value> = list
        .iter()
        .filter_map(|reaction| {
            let emoji = json::str_at(reaction, "emoji");
            if emoji.is_empty() {
                return None;
            }
            Some(serde_json::json!({
                "emoji": emoji,
                // Never 0: the core lists a reaction because someone sent
                // it, and a chip reading "👍 0" would be a lie.
                "count": json::u32_at(reaction, "count").max(1),
                "self": json::flag(reaction, "isFromSelf"),
            }))
        })
        .collect();
    if chips.is_empty() {
        String::new()
    } else {
        serde_json::Value::Array(chips).to_string()
    }
}

/// The reaction this account put on a message, empty when none.
fn own_reaction(message: &serde_json::Value) -> String {
    message
        .pointer("/reactions/reactions")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .find(|reaction| json::flag(reaction, "isFromSelf"))
        .map(|reaction| json::str_at(reaction, "emoji").to_string())
        .unwrap_or_default()
}

/// The local path a picker handed back.
///
/// Silica's pickers report `filePath`, which is already a plain path, but
/// `selectedContent` and anything that has been through a `url` property
/// arrive as `file://` URLs with the awkward characters percent-encoded.
/// Both reach `send_file`, and the core takes a path.
pub(crate) fn local_path(raw: &str) -> String {
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
    use super::{
        file_name_of, gif_dimensions, local_path, own_reaction, reactions_json, video_dimensions,
    };
    use serde_json::json;

    /// An ISO media box: its size and type, then its body.
    fn mp4_box(kind: [u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(
            &u32::try_from(body.len() + 8)
                .expect("box size")
                .to_be_bytes(),
        );
        out.extend_from_slice(&kind);
        out.extend_from_slice(body);
        out
    }

    /// A `tkhd` body, version 0, with a size and a matrix that either
    /// leaves the picture alone or gives it a quarter turn.
    fn tkhd(width: u32, height: u32, turned: bool) -> Vec<u8> {
        let mut body = vec![0_u8; 40];
        // Version 0 and flags, times, id, reserved, duration, more
        // reserved, layer, group, volume, reserved: all zero here.
        let one = 0x0001_0000_u32.to_be_bytes();
        let (a, b, c, d) = if turned {
            ([0; 4], one, (-0x0001_0000_i32).to_be_bytes(), [0; 4])
        } else {
            (one, [0; 4], [0; 4], one)
        };
        for part in [
            a,
            b,
            [0; 4],
            c,
            d,
            [0; 4],
            [0; 4],
            [0; 4],
            0x4000_0000_u32.to_be_bytes(),
        ] {
            body.extend_from_slice(&part);
        }
        body.extend_from_slice(&(width << 16).to_be_bytes());
        body.extend_from_slice(&(height << 16).to_be_bytes());
        body
    }

    #[test]
    fn a_videos_size_is_read_off_its_track_header_turned_as_the_matrix_says() {
        let dir = std::env::temp_dir().join(format!("postivene-mp4-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let ftyp = mp4_box(*b"ftyp", b"isom\0\0\0\0isomiso2mp41");
        let mdat = mp4_box(*b"mdat", &[0_u8; 64]);
        let audio = mp4_box(*b"trak", &mp4_box(*b"tkhd", &tkhd(0, 0, false)));

        // The movie header after the media, as a camera writes it, with
        // the audio track first: the video track's size is the answer.
        let landscape = mp4_box(
            *b"moov",
            &[
                audio.clone(),
                mp4_box(*b"trak", &mp4_box(*b"tkhd", &tkhd(1920, 1080, false))),
            ]
            .concat(),
        );
        let plain = dir.join("landscape.mp4");
        std::fs::write(&plain, [ftyp.clone(), mdat.clone(), landscape].concat()).expect("write");
        assert_eq!(
            video_dimensions(&plain.to_string_lossy()),
            Some((1920, 1080))
        );

        // Taken upright: stored on its side, with the turn in the matrix.
        let upright = mp4_box(
            *b"moov",
            &mp4_box(*b"trak", &mp4_box(*b"tkhd", &tkhd(1920, 1080, true))),
        );
        let turned = dir.join("upright.mp4");
        std::fs::write(&turned, [ftyp.clone(), upright, mdat.clone()].concat()).expect("write");
        assert_eq!(
            video_dimensions(&turned.to_string_lossy()),
            Some((1080, 1920))
        );

        // A media box with a 64-bit size is stepped over like any other.
        let mut large = Vec::new();
        large.extend_from_slice(&1_u32.to_be_bytes());
        large.extend_from_slice(b"mdat");
        large.extend_from_slice(&(16_u64 + 64).to_be_bytes());
        large.extend_from_slice(&[0_u8; 64]);
        let big = dir.join("large.mp4");
        std::fs::write(
            &big,
            [
                ftyp.clone(),
                large,
                mp4_box(
                    *b"moov",
                    &mp4_box(*b"trak", &mp4_box(*b"tkhd", &tkhd(640, 480, false))),
                ),
            ]
            .concat(),
        )
        .expect("write");
        assert_eq!(video_dimensions(&big.to_string_lossy()), Some((640, 480)));

        // Not a movie, not there, no video track: nothing, rather than a
        // guess.
        let gif = dir.join("clip.gif");
        std::fs::write(&gif, b"GIF89a\x2c\x01\x02\x00rest").expect("write");
        assert_eq!(video_dimensions(&gif.to_string_lossy()), None);
        let sound = dir.join("sound.mp4");
        std::fs::write(&sound, [ftyp.clone(), mp4_box(*b"moov", &audio)].concat()).expect("write");
        assert_eq!(video_dimensions(&sound.to_string_lossy()), None);
        assert_eq!(
            video_dimensions(&dir.join("missing.mp4").to_string_lossy()),
            None
        );
        assert_eq!(video_dimensions(""), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_gifs_size_is_read_off_its_header_and_nothing_elses_is() {
        let dir = std::env::temp_dir().join(format!("postivene-gif-dims-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        // 300 wide, 2 high: the descriptor is little-endian.
        let gif = dir.join("wide.gif");
        std::fs::write(&gif, b"GIF89a\x2c\x01\x02\x00\x00\x00\x00;").expect("write gif");
        let png = dir.join("dot.png");
        std::fs::write(&png, b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR").expect("write png");
        let short = dir.join("short.gif");
        std::fs::write(&short, b"GIF8").expect("write short");

        assert_eq!(gif_dimensions(&gif.to_string_lossy()), Some((300, 2)));
        assert_eq!(gif_dimensions(&png.to_string_lossy()), None);
        assert_eq!(gif_dimensions(&short.to_string_lossy()), None);
        assert_eq!(gif_dimensions(""), None);
        assert_eq!(
            gif_dimensions(&dir.join("missing.gif").to_string_lossy()),
            None
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reactions_keep_the_cores_order_and_say_which_is_ours() {
        let message = json!({
            "reactions": {
                "reactions": [
                    {"emoji": "👍", "count": 2, "isFromSelf": false},
                    {"emoji": "❤️", "count": 1, "isFromSelf": true},
                ],
                "reactionsByContact": {"1": ["❤️"], "10": ["👍"], "11": ["👍"]},
            }
        });
        assert_eq!(
            reactions_json(&message),
            r#"[{"count":2,"emoji":"👍","self":false},{"count":1,"emoji":"❤️","self":true}]"#
        );
        assert_eq!(own_reaction(&message), "❤️");
    }

    #[test]
    fn no_reactions_read_as_nothing_however_the_core_says_it() {
        for message in [
            json!({}),
            json!({"reactions": null}),
            json!({"reactions": {"reactions": [], "reactionsByContact": {}}}),
            // A reaction with no emoji is nothing to draw.
            json!({"reactions": {"reactions": [{"emoji": "", "count": 1}]}}),
        ] {
            assert_eq!(reactions_json(&message), "", "{message}");
            assert_eq!(own_reaction(&message), "", "{message}");
        }
        // A count the core left out is still one person's reaction.
        let counted = json!({"reactions": {"reactions": [{"emoji": "🙏"}]}});
        assert_eq!(
            reactions_json(&counted),
            r#"[{"count":1,"emoji":"🙏","self":false}]"#
        );
    }

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

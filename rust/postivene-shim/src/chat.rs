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
use crate::models::{MessageListItem, MessageListModel};

/// `DC_STATE_IN_FRESH` and `DC_STATE_IN_NOTICED`: an incoming message the
/// account has not read yet.
const UNSEEN_STATES: [u32; 2] = [10, 13];

/// How many messages a chat opens with, and how many more each step back
/// through the history brings in.
///
/// The ids are cheap and the messages are not: `get_message_list_items`
/// returns a list of numbers for the whole chat, while `get_messages`
/// builds every field of every row. So the model holds every id and fetches
/// only a window of messages at the end of it -- which is why paging needs
/// no cursor and no server-side paging call.
const PAGE: usize = 50;

/// The newest `count` ids.
fn newest(ids: &[u32], count: usize) -> &[u32] {
    &ids[ids.len().saturating_sub(count)..]
}

/// The window a chat opens with. Shared with the prefetch, which has to
/// load exactly what the model would have.
pub(crate) fn newest_page(ids: &[u32]) -> &[u32] {
    newest(ids, PAGE)
}

/// The page a chat opens on, and whether it reaches the end of the chat.
///
/// `find` names a message to open at -- what a search result gives -- and
/// anything not in the chat, 0 included, opens at the newest messages.
/// Opening at the newest and then walking back to the result is what made
/// jumping to one show today's messages for a moment before yanking the
/// reader off them.
pub(crate) fn opening_page(ids: &[u32], find: u32) -> (&[u32], bool) {
    let start = match ids.iter().position(|id| *id == find) {
        // Half a page above it, so it does not land against the top edge
        // with nothing before it.
        Some(index) => index.saturating_sub(PAGE / 2),
        None => ids.len().saturating_sub(PAGE),
    };
    let end = (start + PAGE).min(ids.len());
    (&ids[start..end], end == ids.len())
}

/// The ids the loaded rows stand for.
///
/// Anchored on ids rather than on counts, because a count is wrong the
/// moment anything changes. One message arrives and the newest `loaded` ids
/// no longer start where the rows do -- the oldest loaded row drops off the
/// front of the slice and reads as a deletion, which would send every
/// arrival through a full reload.
///
/// `far_end` is the newest loaded id, or `None` for a window that runs to
/// the end of the chat and takes in whatever arrives. That is where a chat
/// opens and where it stays, unless the reader goes looking for something
/// far enough back that the window is moved off the end to reach it.
fn window_from(
    ids: &[u32],
    oldest_loaded: Option<u32>,
    far_end: Option<u32>,
    loaded: usize,
) -> &[u32] {
    let Some(first) = oldest_loaded.and_then(|id| ids.iter().position(|other| *other == id)) else {
        // The oldest loaded row is gone from the chat, or nothing is
        // loaded. Falling back to a count keeps the window roughly where it
        // was, and the disagreement it leaves is what `removals` and the
        // reload behind it are for.
        return newest(ids, loaded);
    };
    let Some(far_end) = far_end else {
        return &ids[first..];
    };
    match ids.iter().rposition(|other| *other == far_end) {
        Some(last) if last >= first => &ids[first..=last],
        // The newest loaded row was deleted. Keeping the near anchor and
        // the count leaves the window one id too long at this end, which
        // `sync_rows` cannot reconcile and answers with a reload -- the
        // reader loses their place, which is worse than the alternative
        // only in that it is not silent.
        _ => &ids[first..(first + loaded).min(ids.len())],
    }
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

    /// True when the chat has messages older than the ones loaded.
    pub has_older: qt_property!(bool; NOTIFY window_changed),
    /// True while a step back through the history is in flight.
    pub loading_older: qt_property!(bool; NOTIFY window_changed),
    /// Emitted when [`Self::has_older`] or [`Self::loading_older`] changes.
    pub window_changed: qt_signal!(),
    /// Take one step further back through the history.
    pub load_older: qt_method!(fn(&mut self)),
    /// This many older rows were just put in front of the others.
    ///
    /// The view needs the count, not just the fact: rows inserted above
    /// what is on screen move it, and putting it back means knowing how
    /// far by.
    pub older_loaded: qt_signal!(count: u32),

    /// True when the chat has messages newer than the ones loaded.
    ///
    /// Only after the window has been moved off the end of the chat to
    /// reach something -- the beginning of the history, or a search result
    /// from last March. A chat opens with this false and stays that way
    /// while it is reading arrivals.
    pub has_newer: qt_property!(bool; NOTIFY window_changed),
    /// True while a step forward through the history is in flight.
    pub loading_newer: qt_property!(bool; NOTIFY window_changed),
    /// True while the window is being moved somewhere else entirely.
    pub loading_window: qt_property!(bool; NOTIFY window_changed),
    /// Take one step forward through the history.
    pub load_newer: qt_method!(fn(&mut self)),
    /// This many newer rows were just put after the others.
    pub newer_loaded: qt_signal!(count: u32),

    /// Move the window to the first messages in the chat.
    ///
    /// What the top of the list offers. Without it the only way back to
    /// the beginning of a long chat is a page at a time, and the system's
    /// own scroll-to-top lands at the top of what happens to be loaded --
    /// which reads as the start of the chat and is not.
    pub jump_oldest: qt_method!(fn(&mut self)),
    /// Move the window back to the newest messages.
    pub jump_newest: qt_method!(fn(&mut self)),
    /// The window was replaced. `row` is where to put the view, or -1 if
    /// the message it was moved for is not in it after all.
    pub window_moved: qt_signal!(row: i32),

    /// Load back to `message_id` if it is not loaded, then say where it is.
    ///
    /// What a search result needs. `row_of` answers -1 for a message the
    /// window does not reach, and a chat opened at a result from last March
    /// would otherwise land at the newest message -- the difference between
    /// finding something and being told roughly where it is.
    pub reveal: qt_method!(fn(&mut self, message_id: u32)),
    /// `message_id` is loaded and sits at `row`, or -1 if this chat has no
    /// such message.
    pub revealed: qt_signal!(message_id: u32, row: i32),

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

    /// Every message id in the chat, oldest first, whether or not its row
    /// is loaded. Cheap to hold and cheap to refresh, and it is what makes
    /// the window a slice rather than a cursor.
    all_ids: Vec<u32>,
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
            self.loaded_changed();
            self.chat_changed();
            self.reload();
        }
    }

    /// Reload the whole chat.
    pub fn reload(&mut self) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        if account_id == 0 || chat_id == 0 {
            return;
        }
        // Back to the newest page, whatever the reader had moved the window
        // onto. Putting the in-flight flags down is also what tells a reply
        // for the window being thrown away here to drop rather than splice
        // its rows into the one that replaces it.
        self.has_newer = false;
        self.loading_older = false;
        self.loading_newer = false;
        self.loading_window = false;
        self.window_changed();
        // Already loaded, by whoever opened this page: take it and skip
        // the round trip entirely. This is what lets the transition start
        // with the rows in place rather than fill in behind it.
        if let Some((is_group, ids, items, has_newer)) = crate::prefetch::take(account_id, chat_id)
        {
            self.is_group = is_group;
            self.all_ids = ids;
            self.rows.borrow_mut().reset_data(items);
            self.loaded = true;
            // A prefetch loaded for a search result sits in the middle of
            // the chat rather than at its end.
            self.has_newer = has_newer;
            self.note_window();
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
            move |result: Result<(bool, Vec<u32>, Vec<MessageListItem>), String>| {
                let Some(this) = ptr.as_pinned() else { return };
                match result {
                    Ok((is_group, ids, items)) => {
                        {
                            let mut this_mut = this.borrow_mut();
                            this_mut.is_group = is_group;
                            this_mut.all_ids = ids;
                            this_mut.rows.borrow_mut().reset_data(items);
                            this_mut.loaded = true;
                            this_mut.note_window();
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
                let ids = message_ids(&rpc, account_id, chat_id).await?;
                // Only the newest page. A chat with ten thousand messages
                // used to build ten thousand rows on the Qt thread before
                // the page could show any of them.
                let items = fetch_messages(&rpc, account_id, newest_page(&ids)).await?;
                // Marking read is the callback's job, not this one's: what
                // the reader can see is only knowable once the rows land.
                Ok::<_, String>((is_group, ids, items))
            }
            .await;
            done(result);
        });
    }

    /// The newest loaded id, or `None` for a window that runs to the end of
    /// the chat and takes in whatever arrives.
    fn far_end(&self) -> Option<u32> {
        if !self.has_newer {
            return None;
        }
        self.rows.borrow().iter().last().map(|item| item.message_id)
    }

    /// Say whether there is more history at either end, after anything that
    /// could have changed the answer.
    fn note_window(&mut self) {
        let rows = self.rows.borrow();
        let oldest = rows.iter().next().map(|item| item.message_id);
        let newest_row = rows.iter().last().map(|item| item.message_id);
        drop(rows);
        let has_older = match oldest {
            Some(oldest) => self.all_ids.first() != Some(&oldest),
            // Nothing loaded yet: whatever the chat holds is still to come.
            None => !self.all_ids.is_empty(),
        };
        // Reaching the end of the chat is what puts the window back to
        // taking in arrivals. Only ever cleared here, never set: an arrival
        // extends the id list, and setting it on that would stop a chat
        // sitting at its newest message from following the moment anything
        // came in.
        let has_newer = self.has_newer && self.all_ids.last().copied() != newest_row;
        if has_older != self.has_older || has_newer != self.has_newer {
            self.has_older = has_older;
            self.has_newer = has_newer;
            self.window_changed();
        }
    }

    /// Take a refreshed id list, keeping the window's answer in step.
    fn adopt_ids(&mut self, ids: Vec<u32>) {
        self.all_ids = ids;
        self.note_window();
    }

    /// Take one step further back through the history.
    pub fn load_older(&mut self) {
        self.extend_back(PAGE, 0);
    }

    /// Put the window where `message_id` is, then say where it ended up.
    pub fn reveal(&mut self, message_id: u32) {
        let row = self.row_of(message_id);
        if row >= 0 {
            self.revealed(message_id, row);
            return;
        }
        let Some(index) = self.all_ids.iter().position(|id| *id == message_id) else {
            // Not in this chat at all: a stale search result, or a message
            // deleted between finding it and opening it.
            self.revealed(message_id, -1);
            return;
        };
        // The window moves rather than growing. Reaching a message from
        // last March by loading everything between it and today costs
        // exactly what paging was for -- and this used to do that. Half a
        // page above it, so it does not land against the top edge with
        // nothing before it.
        self.show_window(index.saturating_sub(PAGE / 2), message_id, true);
    }

    /// Move the window to the first messages in the chat.
    pub fn jump_oldest(&mut self) {
        let Some(first) = self.all_ids.first().copied() else {
            return;
        };
        self.show_window(0, first, false);
    }

    /// Move the window back to the newest messages.
    pub fn jump_newest(&mut self) {
        let Some(last) = self.all_ids.last().copied() else {
            return;
        };
        self.show_window(self.all_ids.len().saturating_sub(PAGE), last, false);
    }

    /// Take one step forward through the history.
    pub fn load_newer(&mut self) {
        self.extend_forward(PAGE);
    }

    /// Move the window to a page of the chat starting at `start`, replacing
    /// what is loaded.
    ///
    /// `settle_on` is the message the view should be put on once the rows
    /// are in. `reveal` announces it as a search result as well, which is
    /// what makes the row flash; a plain jump to one end does not.
    fn show_window(&mut self, start: usize, settle_on: u32, reveal: bool) {
        if self.loading_window || self.all_ids.is_empty() {
            return;
        }
        let len = self.all_ids.len();
        let start = start.min(len.saturating_sub(1));
        let end = (start + PAGE).min(len);
        let wanted: Vec<u32> = self.all_ids[start..end].to_vec();
        let already = {
            let rows = self.rows.borrow();
            rows.iter().count() == wanted.len()
                && rows
                    .iter()
                    .zip(wanted.iter())
                    .all(|(row, id)| row.message_id == *id)
        };
        if already {
            let row = self.row_of(settle_on);
            self.window_moved(row);
            if reveal {
                self.revealed(settle_on, row);
            }
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        let account_id = self.account_id;
        let chat = self.chat_id;
        // Whether the new window reaches the end of the chat, which is what
        // decides whether it goes back to taking in arrivals.
        let reaches_end = end == len;

        self.loading_window = true;
        self.window_changed();

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Vec<MessageListItem>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            // The flag is its own guard: `reload` puts it down, so a reply
            // for a window that has since been thrown away drops here.
            if !this.borrow().loading_window || this.borrow().chat_id != chat {
                return;
            }
            match result {
                Ok(items) => {
                    {
                        let mut this_mut = this.borrow_mut();
                        this_mut.rows.borrow_mut().reset_data(items);
                        this_mut.has_newer = !reaches_end;
                        this_mut.loading_window = false;
                    }
                    this.borrow_mut().note_window();
                    this.borrow().window_changed();
                    this.borrow().rows_changed();
                    let row = this.borrow().row_of(settle_on);
                    this.borrow().window_moved(row);
                    if reveal {
                        this.borrow().revealed(settle_on, row);
                    }
                }
                Err(err) => {
                    this.borrow_mut().loading_window = false;
                    this.borrow().window_changed();
                    this.borrow().error(err.into());
                    if reveal {
                        this.borrow().revealed(settle_on, -1);
                    }
                }
            }
        });

        runtime.spawn(async move {
            done(fetch_messages(&rpc, account_id, &wanted).await);
        });
    }

    /// Put `extra` newer rows after the loaded ones.
    fn extend_forward(&mut self, extra: usize) {
        if self.loading_newer || self.loading_window || extra == 0 || !self.has_newer {
            return;
        }
        let account_id = self.account_id;
        let newest_row = self.rows.borrow().iter().last().map(|item| item.message_id);
        let Some(start) = newest_row
            .and_then(|id| self.all_ids.iter().rposition(|other| *other == id))
            .map(|index| index + 1)
        else {
            return;
        };
        let end = (start + extra).min(self.all_ids.len());
        let newer: Vec<u32> = self.all_ids.get(start..end).unwrap_or_default().to_vec();
        if newer.is_empty() {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };

        self.loading_newer = true;
        self.window_changed();

        let chat = self.chat_id;
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Vec<MessageListItem>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            let stale = {
                let this_ref = this.borrow();
                !this_ref.loading_newer
                    || this_ref.chat_id != chat
                    || this_ref
                        .rows
                        .borrow()
                        .iter()
                        .last()
                        .map(|item| item.message_id)
                        != newest_row
            };
            if stale {
                this.borrow_mut().loading_newer = false;
                this.borrow().window_changed();
                return;
            }
            let count = match result {
                Ok(items) => {
                    let added = items.len();
                    {
                        let this_mut = this.borrow_mut();
                        let mut rows = this_mut.rows.borrow_mut();
                        // At the back, in order: these are newer than
                        // everything already there. Appended rather than
                        // reset, so the view keeps its place.
                        for item in items {
                            rows.push(item);
                        }
                    }
                    added
                }
                Err(err) => {
                    this.borrow().error(err.into());
                    0
                }
            };
            this.borrow_mut().loading_newer = false;
            this.borrow_mut().note_window();
            this.borrow().window_changed();
            if count > 0 {
                this.borrow().rows_changed();
                this.borrow()
                    .newer_loaded(u32::try_from(count).unwrap_or(u32::MAX));
            }
        });

        runtime.spawn(async move {
            done(fetch_messages(&rpc, account_id, &newer).await);
        });
    }

    /// Put `extra` older rows in front of the loaded ones.
    ///
    /// `reveal_after` is the message to announce once they are in, for the
    /// caller that is stepping back in order to reach one; 0 for a plain
    /// step back through the history.
    fn extend_back(&mut self, extra: usize, reveal_after: u32) {
        if self.loading_older || self.loading_window || extra == 0 {
            return;
        }
        let account_id = self.account_id;
        let loaded = self.rows.borrow().iter().count();
        let end = self.all_ids.len().saturating_sub(loaded);
        let start = end.saturating_sub(extra);
        let older: Vec<u32> = self.all_ids[start..end].to_vec();
        if older.is_empty() {
            if reveal_after != 0 {
                let row = self.row_of(reveal_after);
                self.revealed(reveal_after, row);
            }
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };

        self.loading_older = true;
        self.window_changed();

        // What the window starts at now. If it starts somewhere else by the
        // time these arrive, the model was rebuilt underneath them -- an
        // EventChannelOverflow reload, or the page moving to another chat --
        // and they are rows from a window that no longer exists.
        let anchor = self.rows.borrow().iter().next().map(|item| item.message_id);
        let chat = self.chat_id;

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Vec<MessageListItem>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            let stale = {
                let this_ref = this.borrow();
                this_ref.chat_id != chat
                    || this_ref
                        .rows
                        .borrow()
                        .iter()
                        .next()
                        .map(|item| item.message_id)
                        != anchor
            };
            if stale {
                let mut this_mut = this.borrow_mut();
                this_mut.loading_older = false;
                drop(this_mut);
                this.borrow().window_changed();
                return;
            }
            // Still loading as far as anyone watching is concerned: the
            // rows are not in yet, and a spinner that stops before they
            // appear is a spinner that lied.
            let count = match result {
                Ok(items) => {
                    let added = items.len();
                    {
                        let this_mut = this.borrow_mut();
                        let mut rows = this_mut.rows.borrow_mut();
                        // In order and at the front: these are older than
                        // everything already there. Inserted rather than
                        // reset, so the view keeps its place and its
                        // delegates instead of rebuilding the lot.
                        for (offset, item) in items.into_iter().enumerate() {
                            rows.insert(offset, item);
                        }
                    }
                    added
                }
                Err(err) => {
                    this.borrow().error(err.into());
                    0
                }
            };
            {
                let mut this_mut = this.borrow_mut();
                this_mut.loading_older = false;
            }
            this.borrow_mut().note_window();
            this.borrow().window_changed();
            if count > 0 {
                this.borrow().rows_changed();
                this.borrow()
                    .older_loaded(u32::try_from(count).unwrap_or(u32::MAX));
            }
            if reveal_after != 0 {
                let row = this.borrow().row_of(reveal_after);
                this.borrow().revealed(reveal_after, row);
            }
        });

        runtime.spawn(async move {
            let result = fetch_messages(&rpc, account_id, &older).await;
            done(result);
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
        let event_chat = payload
            .get("chatId")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        // MsgsChanged carries chatId 0 for "several chats", and an
        // overflow carries none at all.
        if event_chat != 0 && event_chat != u64::from(self.chat_id) {
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
                if let Some(message_id) = payload
                    .get("msgId")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|id| u32::try_from(id).ok())
                {
                    self.refresh_one(message_id);
                }
            }
            _ => {}
        }
    }

    /// Bring `rows` in line with the chat's current id list, fetching only
    /// the messages that are not loaded yet.
    fn sync_rows(&mut self) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        // A set: this is asked once per message in the chat, and a long
        // history would otherwise make the scan quadratic.
        let known: HashSet<u32> = self
            .rows
            .borrow()
            .iter()
            .map(|item| item.message_id)
            .collect();
        // How far back the window reaches. Anything older than this is the
        // reader's to ask for; fetching it here would undo the paging on
        // the first message that arrived.
        let loaded = known.len();
        let oldest = self.rows.borrow().iter().next().map(|item| item.message_id);
        // And how far forward. A window moved off the end of the chat does
        // not take in arrivals: they are past its far end, and swallowing
        // them would drag a reader who went looking for something in last
        // March back to today one message at a time.
        let far_end = self.far_end();

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(
            move |result: Result<(Vec<u32>, Vec<MessageListItem>), String>| {
                let Some(this) = ptr.as_pinned() else { return };
                match result {
                    Ok((ids, fetched)) => {
                        let this_mut = this.borrow_mut();
                        let mut rows = this_mut.rows.borrow_mut();
                        // The window, not the whole chat: the rows stand
                        // for the newest slice of the id list, and this is
                        // the slice they have to agree with.
                        let wanted = window_from(&ids, oldest, far_end, rows.iter().count());
                        // A deletion or a reorder is rare and cheap to
                        // handle wholesale; the common case is an append.
                        let unchanged_prefix = wanted.len() >= rows.iter().count()
                            && rows
                                .iter()
                                .zip(wanted.iter())
                                .all(|(row, id)| row.message_id == *id);
                        if unchanged_prefix {
                            // A send in flight may have pushed a row this
                            // fetch also carries: the id list was read
                            // before that reply landed. Appending it again
                            // is the duplicate that survives until reload.
                            let mut appended = Vec::new();
                            for item in fetched {
                                if rows.iter().any(|row| row.message_id == item.message_id) {
                                    continue;
                                }
                                appended.push(item.clone());
                                rows.push(item);
                            }
                            drop(rows);
                            drop(this_mut);
                            this.borrow_mut().adopt_ids(ids);
                            this.borrow().rows_changed();
                            // Asked after the push rather than before the
                            // fetch, so a reader who scrolled away while it
                            // ran is not credited with seeing what arrived.
                            let looking = !this.borrow().reading_history;
                            let incoming = u32::try_from(
                                appended.iter().filter(|item| !item.is_outgoing).count(),
                            )
                            .unwrap_or(u32::MAX);
                            if looking {
                                this.borrow().mark_items_seen(appended);
                            }
                            if incoming > 0 {
                                this.borrow().arrived(incoming);
                            }
                        } else if let Some(gone) = removals(&rows, wanted) {
                            // A pure removal, so the rows that remain are
                            // still right: take the others out rather than
                            // resetting the model, which drops the view's
                            // place and lands the reader at the oldest
                            // message in the chat.
                            for index in gone.into_iter().rev() {
                                rows.remove(index);
                            }
                            drop(rows);
                            drop(this_mut);
                            this.borrow_mut().adopt_ids(ids);
                            this.borrow().rows_changed();
                        } else {
                            drop(rows);
                            drop(this_mut);
                            this.borrow_mut().reload();
                        }
                    }
                    Err(err) => this.borrow().error(err.into()),
                }
            },
        );

        runtime.spawn(async move {
            let result = async {
                let ids = message_ids(&rpc, account_id, chat_id).await?;
                // The window grows by whatever arrived, and by nothing
                // else: an id older than the window is one the reader has
                // not asked for.
                let missing: Vec<u32> = window_from(&ids, oldest, far_end, loaded)
                    .iter()
                    .copied()
                    .filter(|id| !known.contains(id))
                    .collect();
                let fetched = fetch_messages(&rpc, account_id, &missing).await?;
                Ok::<_, String>((ids, fetched))
            }
            .await;
            done(result);
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

    /// Call `method` with `(account, [message])`, then re-read that row.
    ///
    /// A deletion changes the id list, so the event it raises reloads the
    /// model on its own. A resend does not: the list is the same, the sync
    /// fetches only ids it is missing, and the row keeps the failed state
    /// it had -- mark and "Send again" and all -- until something else
    /// refetches it.
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
                    if this.borrow().has_newer {
                        // The window is somewhere else in the chat -- the
                        // reader went looking for something old and sent
                        // from there. Their message belongs at the end of
                        // the chat, and so, now, do they: put the window
                        // back rather than appending today to a page from
                        // last March, which is a window that stands for no
                        // slice of the id list and reloads on the next
                        // event anyway.
                        this.borrow_mut().reload();
                    } else {
                        {
                            let this_mut = this.borrow_mut();
                            let mut rows = this_mut.rows.borrow_mut();
                            // The event for our own send can beat this
                            // reply, in which case the row is already there.
                            let existing =
                                rows.iter().position(|row| row.message_id == message_id);
                            if let Some(index) = existing {
                                rows.change_line(index, item);
                            } else {
                                rows.push(item);
                            }
                        }
                        this.borrow().rows_changed();
                    }
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

/// The row indices to drop, when `ids` is the model's own list with some
/// taken out and nothing added or moved. `None` when it is anything else,
/// which has to go through a reload.
fn removals(rows: &MessageListModel, ids: &[u32]) -> Option<Vec<usize>> {
    let mut wanted = ids.iter();
    let mut next = wanted.next();
    let mut gone = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if next == Some(&row.message_id) {
            next = wanted.next();
        } else {
            gone.push(index);
        }
    }
    // Anything left over is an id the model does not have, in the order the
    // core wants it -- an arrival or a reorder, not a removal.
    (next.is_none() && wanted.next().is_none() && !gone.is_empty()).then_some(gone)
}

/// The chat's message ids, oldest first.
pub(crate) async fn message_ids(
    rpc: &RpcClient,
    account_id: u32,
    chat_id: u32,
) -> Result<Vec<u32>, String> {
    let items: Vec<serde_json::Value> = rpc
        .call(
            "get_message_list_items",
            (account_id, chat_id, false, false),
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(items
        .iter()
        .filter(|item| item.get("kind").and_then(serde_json::Value::as_str) == Some("message"))
        .filter_map(|item| {
            // `rename_all` sits at the enum level upstream, renaming variants
            // but not fields: this really is snake_case on the wire. `msgId`
            // accepted in case that changes.
            item.get("msg_id")
                .or_else(|| item.get("msgId"))
                .and_then(serde_json::Value::as_u64)
                .and_then(|id| u32::try_from(id).ok())
        })
        .collect())
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
            if message.get("kind").and_then(serde_json::Value::as_str) == Some("loadingError") {
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
    !matches!(
        info.get("chatType").and_then(serde_json::Value::as_str),
        Some("Single") | None
    )
}

/// Read a string field, empty when absent or null.
fn text_at(message: &serde_json::Value, pointer: &str) -> QString {
    message
        .pointer(pointer)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .into()
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
fn row_from(message_id: u32, message: &serde_json::Value) -> MessageListItem {
    let timestamp = message
        .get("timestamp")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    let sender_name = match message
        .get("overrideSenderName")
        .and_then(serde_json::Value::as_str)
    {
        Some(name) if !name.is_empty() => name.into(),
        _ => text_at(message, "/sender/displayName"),
    };
    MessageListItem {
        message_id,
        text: text_at(message, "/text"),
        // Contact id 1 is the well-known DC_CONTACT_ID_SELF.
        is_outgoing: message.get("fromId").and_then(serde_json::Value::as_u64) == Some(1),
        timestamp,
        day_number: local_day_number(timestamp),
        show_padlock: message
            .get("showPadlock")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        state: message
            .get("state")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(0),
        sender_name,
        sender_color: text_at(message, "/sender/color"),
        is_info: message
            .get("isInfo")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        // `unwrap_or(false)` covers the abbreviated object the fake core
        // returns from misc_send_msg; a message composed here is never a
        // forward anyway. Real forwards arrive through get_messages,
        // which returns the full shape.
        is_forwarded: message
            .get("isForwarded")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        quote_text: text_at(message, "/quote/text"),
        quote_author: text_at(message, "/quote/authorDisplayName"),
        file_path: text_at(message, "/file"),
        file_name: text_at(message, "/fileName"),
        view_type: text_at(message, "/viewType"),
        // Often 0 even for a picture: the core probes PNG and JPEG but
        // returned 0x0 for a valid GIF, so nothing may divide by these.
        image_width: int_field(message, "dimensionsWidth"),
        image_height: int_field(message, "dimensionsHeight"),
        file_mime: text_at(message, "/fileMime"),
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
        vcard_name: text_at(message, "/vcardContact/displayName"),
        vcard_addr: text_at(message, "/vcardContact/addr"),
        vcard_color: text_at(message, "/vcardContact/color"),
    }
}

/// A whole number from the message, 0 when the core has none. Pixel
/// dimensions and the duration all arrive this way, and all are absent
/// often enough that the absence is the normal case.
fn int_field(message: &serde_json::Value, field: &str) -> i32 {
    message
        .get(field)
        .and_then(serde_json::Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or(0)
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

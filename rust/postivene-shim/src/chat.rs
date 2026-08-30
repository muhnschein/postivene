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

/// The messages of one chat.
///
/// ```qml
/// ChatMessages { id: messages; account_id: 1; chat_id: 7 }
/// SilicaListView { model: messages.rows }
/// ```
#[derive(QObject, Default)]
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

    /// Reload the whole chat. Called automatically when the chat changes.
    pub reload: qt_method!(fn(&mut self)),

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
    /// Mark every unread message now loaded as read. Called when the
    /// reader reaches the newest message.
    pub mark_seen_all: qt_method!(fn(&mut self)),
    /// Delete a message. Not only here: the core also removes it from the
    /// mail server, which for this client is where it lived.
    pub delete_message: qt_method!(fn(&mut self, message_id: u32)),
    /// Try a failed message again.
    pub resend_message: qt_method!(fn(&mut self, message_id: u32)),
    /// Send a copy of one message into another chat; QML calls this.
    pub forward_to: qt_method!(fn(&mut self, message_id: u32, chat_id: u32)),
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
        let Some((rpc, runtime)) = connection() else {
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(
            move |result: Result<(bool, Vec<MessageListItem>), String>| {
                let Some(this) = ptr.as_pinned() else { return };
                match result {
                    Ok((is_group, items)) => {
                        {
                            let mut this_mut = this.borrow_mut();
                            this_mut.is_group = is_group;
                            this_mut.rows.borrow_mut().reset_data(items);
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
                let ids = message_ids(&rpc, account_id, chat_id).await?;
                let items = fetch_messages(&rpc, account_id, &ids).await?;
                // Marking read is the callback's job, not this one's: what
                // the reader can see is only knowable once the rows land.
                Ok::<_, String>((is_group, items))
            }
            .await;
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

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(
            move |result: Result<(Vec<u32>, Vec<MessageListItem>), String>| {
                let Some(this) = ptr.as_pinned() else { return };
                match result {
                    Ok((ids, fetched)) => {
                        let this_mut = this.borrow_mut();
                        let mut rows = this_mut.rows.borrow_mut();
                        // A deletion or a reorder is rare and cheap to
                        // handle wholesale; the common case is an append.
                        let unchanged_prefix = ids.len() >= rows.iter().count()
                            && rows
                                .iter()
                                .zip(ids.iter())
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
                        } else if let Some(gone) = removals(&rows, &ids) {
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
                let missing: Vec<u32> = ids
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
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        let quoted = self.quoted_message_id;
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<MessageListItem, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(item) => {
                    let message_id = item.message_id;
                    {
                        let this_mut = this.borrow_mut();
                        let mut rows = this_mut.rows.borrow_mut();
                        // The event for our own send can beat this reply,
                        // in which case the row is already there.
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

        let text = text.to_string();
        runtime.spawn(async move {
            // misc_send_msg params: account, chat, text, file, filename,
            // location, quoted_message_id.
            let result = rpc
                .call::<_, (u32, serde_json::Value)>(
                    "misc_send_msg",
                    (
                        account_id,
                        chat_id,
                        Some(text),
                        Option::<String>::None,
                        Option::<String>::None,
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
async fn message_ids(rpc: &RpcClient, account_id: u32, chat_id: u32) -> Result<Vec<u32>, String> {
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
async fn fetch_messages(
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
async fn chat_is_group(rpc: &RpcClient, account_id: u32, chat_id: u32) -> bool {
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
        image_width: pixels(message, "dimensionsWidth"),
        image_height: pixels(message, "dimensionsHeight"),
    }
}

/// A pixel dimension, 0 when the core has none.
fn pixels(message: &serde_json::Value, field: &str) -> i32 {
    message
        .get(field)
        .and_then(serde_json::Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or(0)
}

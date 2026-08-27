//! One chat's messages, as a QML-instantiable type.
//!
//! QML creates one per conversation page, so two open chats no longer share
//! a model. Loading is a batch call and events update rows in place, rather
//! than refetching the whole history per message.

use std::cell::RefCell;
use std::collections::BTreeMap;

use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;

use crate::core::connection;
use crate::models::{MessageListItem, MessageListModel};

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
    /// Emitted when the chat this model points at changes.
    pub chat_changed: qt_signal!(),

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

    /// Send a plain-text message to this chat.
    pub send: qt_method!(fn(&mut self, text: QString)),
    /// A message of ours reached the core and is in `rows`.
    pub sent: qt_signal!(message_id: u32),
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
            self.chat_changed();
            self.reload();
        }
    }

    /// Set the chat and reload if it changed.
    pub fn set_chat_id(&mut self, chat_id: u32) {
        if self.chat_id != chat_id {
            self.chat_id = chat_id;
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
        let done = queued_callback(move |result: Result<Vec<MessageListItem>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(items) => {
                    this.borrow_mut().rows.borrow_mut().reset_data(items);
                    this.borrow().rows_changed();
                }
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            let result = async {
                let ids = message_ids(&rpc, account_id, chat_id).await?;
                let items = fetch_messages(&rpc, account_id, &ids).await?;
                // Opening a chat means the user has seen it; the chat list
                // refreshes its unread counts on the resulting MsgsNoticed.
                let _ = rpc
                    .call::<_, ()>("marknoticed_chat", (account_id, chat_id))
                    .await;
                Ok::<_, String>(items)
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
        // MsgsChanged carries chatId 0 for "several chats".
        if event_chat != 0 && event_chat != u64::from(self.chat_id) {
            return;
        }

        match kind.as_str() {
            // New or changed content: take in what we do not have yet.
            "IncomingMsg" | "MsgsChanged" | "MsgDeleted" => self.sync_rows(),
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
        let known: Vec<u32> = self
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
                            for item in fetched {
                                rows.push(item);
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
        let Some(index) = self
            .rows
            .borrow()
            .iter()
            .position(|item| item.message_id == message_id)
        else {
            return;
        };
        let Some((rpc, runtime)) = connection() else {
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Vec<MessageListItem>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            if let Ok(items) = result {
                if let Some(item) = items.into_iter().next() {
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

    /// Send a plain-text message.
    pub fn send(&mut self, text: QString) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
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
                    this.borrow_mut().rows.borrow_mut().push(item);
                    this.borrow().rows_changed();
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
                        Option::<u32>::None,
                    ),
                )
                .await
                .map(|(message_id, message)| row_from(message_id, &message))
                .map_err(|err| err.to_string());
            done(result);
        });
    }
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

/// One row from the core's message object.
fn row_from(message_id: u32, message: &serde_json::Value) -> MessageListItem {
    MessageListItem {
        message_id,
        text: message
            .get("text")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .into(),
        // Contact id 1 is the well-known DC_CONTACT_ID_SELF.
        is_outgoing: message.get("fromId").and_then(serde_json::Value::as_u64) == Some(1),
        timestamp: message
            .get("timestamp")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0),
        show_padlock: message
            .get("showPadlock")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        state: message
            .get("state")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(0),
    }
}

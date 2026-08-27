//! The chat list, as a QML-instantiable type.
//!
//! A chat list reorders constantly: any message moves its chat to the top.
//! Rebuilding the model for that loses the scroll position and redraws every
//! row, so this one reconciles instead -- it moves the row that moved and
//! refetches only the chats whose contents changed.

use std::cell::RefCell;
use std::collections::HashMap;

use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;

use crate::core::connection;
use crate::models::{ChatListItem, ChatListModel};

/// One account's chats, most recent first.
///
/// ```qml
/// ChatList { id: chats; account_id: page.accountId }
/// SilicaListView { model: chats.rows }
/// ```
#[derive(QObject, Default)]
pub struct ChatList {
    base: qt_base_class!(trait QObject),

    /// Whose chats these are. Setting it reloads.
    pub account_id: qt_property!(u32; WRITE set_account_id NOTIFY account_changed),
    /// Emitted when the account changes.
    pub account_changed: qt_signal!(),

    /// The rows, for a `SilicaListView`'s `model`.
    pub rows: qt_property!(RefCell<ChatListModel>; CONST),

    /// How many rows there are.
    pub count: qt_property!(u32; READ count NOTIFY rows_changed),
    /// Emitted after any change to `rows`.
    pub rows_changed: qt_signal!(),

    /// Loading failed. The message is the core's own.
    pub error: qt_signal!(message: QString),

    /// Reload the whole list.
    pub reload: qt_method!(fn(&mut self)),

    /// Feed a `core_event` in. Events for other accounts are ignored.
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),
}

impl ChatList {
    /// How many rows there are.
    pub fn count(&self) -> u32 {
        u32::try_from(self.rows.borrow().iter().count()).unwrap_or(u32::MAX)
    }

    /// Set the account and reload if it changed.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.account_changed();
            self.reload();
        }
    }

    /// Reload every row.
    pub fn reload(&mut self) {
        self.refresh(None);
    }

    /// Apply one core event.
    pub fn handle_event(&mut self, context_id: u32, kind: QString, payload_json: QString) {
        if context_id != self.account_id || self.account_id == 0 {
            return;
        }
        if !matches!(
            kind.to_string().as_str(),
            "IncomingMsg" | "MsgsChanged" | "MsgsNoticed" | "MsgDelivered" | "MsgFailed"
        ) {
            return;
        }
        let payload: serde_json::Value =
            serde_json::from_str(&payload_json.to_string()).unwrap_or_default();
        // chatId 0 means "several chats"; then everything may have changed.
        let changed = payload
            .get("chatId")
            .and_then(serde_json::Value::as_u64)
            .and_then(|id| u32::try_from(id).ok())
            .filter(|id| *id != 0);
        self.refresh(changed);
    }

    /// Bring the model in line with the core.
    ///
    /// `changed` names a chat whose contents are known to have changed; its
    /// row is refetched along with any chat not in the model yet. Everything
    /// else is reused, so a message arriving in one chat costs one entry
    /// listing and one item fetch, not a rebuild.
    fn refresh(&mut self, changed: Option<u32>) {
        let account_id = self.account_id;
        if account_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        let cached: Vec<ChatListItem> = self.rows.borrow().iter().cloned().collect();
        let known: Vec<u32> = cached.iter().map(|row| row.chat_id).collect();

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Vec<ChatListItem>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(target) => {
                    {
                        let this_ref = this.borrow();
                        let mut rows = this_ref.rows.borrow_mut();
                        reconcile(&mut rows, target);
                    }
                    this.borrow().rows_changed();
                }
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            let result = async {
                let entries = chat_entries(&rpc, account_id).await?;
                let wanted: Vec<u32> = entries
                    .iter()
                    .copied()
                    .filter(|id| Some(*id) == changed || !known.contains(id))
                    .collect();
                let fresh = if wanted.is_empty() {
                    HashMap::new()
                } else {
                    chat_items(&rpc, account_id, &wanted).await?
                };
                // Target order from the core, contents from the fetch where
                // we have them and from the model where we do not.
                Ok::<_, String>(
                    entries
                        .into_iter()
                        .filter_map(|chat_id| {
                            fresh.get(&chat_id).cloned().or_else(|| {
                                cached.iter().find(|row| row.chat_id == chat_id).cloned()
                            })
                        })
                        .collect::<Vec<_>>(),
                )
            }
            .await;
            done(result);
        });
    }
}

/// Move, insert and remove rows until the model matches `target`.
///
/// The common case -- one chat moving to the top -- is one remove and one
/// insert, so every other row keeps its identity and the view keeps its
/// place.
fn reconcile(rows: &mut ChatListModel, target: Vec<ChatListItem>) {
    for (index, wanted) in target.iter().enumerate() {
        let current: Vec<u32> = rows.iter().map(|row| row.chat_id).collect();
        match current.iter().position(|id| *id == wanted.chat_id) {
            Some(found) if found == index => {
                if rows.iter().nth(index).is_some_and(|row| row != wanted) {
                    rows.change_line(index, wanted.clone());
                }
            }
            Some(found) => {
                rows.remove(found);
                rows.insert(index, wanted.clone());
            }
            None => rows.insert(index, wanted.clone()),
        }
    }
    while rows.iter().count() > target.len() {
        rows.remove(target.len());
    }
}

/// The account's chat ids, in the order the core wants them shown.
async fn chat_entries(rpc: &RpcClient, account_id: u32) -> Result<Vec<u32>, String> {
    rpc.call(
        "get_chatlist_entries",
        (
            account_id,
            Option::<u32>::None,
            Option::<String>::None,
            Option::<u32>::None,
        ),
    )
    .await
    .map_err(|err| err.to_string())
}

/// Rows for the given chats, in one call.
async fn chat_items(
    rpc: &RpcClient,
    account_id: u32,
    ids: &[u32],
) -> Result<HashMap<u32, ChatListItem>, String> {
    let items: HashMap<u32, serde_json::Value> = rpc
        .call("get_chatlist_items_by_entries", (account_id, ids))
        .await
        .map_err(|err| err.to_string())?;
    Ok(items
        .into_iter()
        .filter(|(_, item)| {
            item.get("kind").and_then(serde_json::Value::as_str) == Some("ChatListItem")
        })
        .map(|(chat_id, item)| {
            (
                chat_id,
                ChatListItem {
                    chat_id,
                    name: item
                        .get("name")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .into(),
                    preview: item
                        .get("summaryText2")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .into(),
                    unread_count: item
                        .get("freshMessageCounter")
                        .and_then(serde_json::Value::as_u64)
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or(0),
                    is_encrypted: item
                        .get("isEncrypted")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                },
            )
        })
        .collect())
}

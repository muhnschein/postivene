//! The chat list, as a QML-instantiable type.
//!
//! A chat list reorders constantly: any message moves its chat to the top.
//! Rebuilding the model for that loses the scroll position and redraws every
//! row, so this one reconciles instead -- it moves the row that moved and
//! refetches only the chats whose contents changed.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;
use serde_json::json;

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

    /// Only show chats matching this. Empty shows everything.
    ///
    /// The core does the matching, so a search finds chats this model has
    /// never loaded rather than filtering the rows already on screen.
    pub query: qt_property!(QString; WRITE set_query NOTIFY query_changed),
    /// Emitted when the query changes.
    pub query_changed: qt_signal!(),

    /// Show the archived chats instead of the ordinary ones.
    pub archived: qt_property!(bool; WRITE set_archived NOTIFY archived_changed),
    /// Emitted when the archived flag changes.
    pub archived_changed: qt_signal!(),

    /// List only chats a message can be forwarded into.
    ///
    /// The core leaves out the ones that would fail or make no sense --
    /// the device chat among them -- so a picker built on this cannot
    /// offer a destination the forward would then be refused by.
    pub for_forwarding: qt_property!(bool; WRITE set_for_forwarding NOTIFY for_forwarding_changed),
    /// Emitted when the forwarding flag changes.
    pub for_forwarding_changed: qt_signal!(),

    /// The rows, for a `SilicaListView`'s `model`.
    pub rows: qt_property!(RefCell<ChatListModel>; CONST),

    /// How many rows there are.
    pub count: qt_property!(u32; READ count NOTIFY rows_changed),
    // Stored rather than counted out of the model on demand. A section
    // heading binds to these, so QML reads them from inside the model
    // reset that sets the rows -- and a reader that borrowed the row list
    // there would find it already mutably borrowed and take the process
    // down with it.
    /// Chats kept at the top, for deciding whether to head the two
    /// groups at all: one heading over the whole list says nothing.
    pub pinned_count: qt_property!(u32; NOTIFY rows_changed),
    /// The rest, for the same decision from the other side.
    pub unpinned_count: qt_property!(u32; NOTIFY rows_changed),
    /// Unread messages across every chat, for the cover.
    ///
    /// Muted chats are counted. Muting silences the announcement, not the
    /// arithmetic -- the badge on a muted chat behaves the same way.
    pub unread_total: qt_property!(u32; READ unread_total NOTIFY rows_changed),
    /// Emitted after any change to `rows`.
    pub rows_changed: qt_signal!(),

    /// A message just arrived in this chat, and the row for it now holds
    /// the sender and text a notification wants.
    ///
    /// Only for `IncomingMsg`, and only once the refetch it triggered has
    /// landed -- announcing on the event itself would carry a preview from
    /// before the message. A muted chat is never announced: it still
    /// counts towards the badge, quietly, which is the whole point of
    /// muting.
    pub message_arrived: qt_signal!(chat_id: u32, chat_name: QString, preview: QString),

    /// Loading failed. The message is the core's own.
    pub error: qt_signal!(message: QString),

    /// Reload the whole list.
    pub reload: qt_method!(fn(&mut self)),

    /// Feed a `core_event` in. Events for other accounts are ignored.
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),

    /// Mark everything in a chat read, without opening it.
    pub mark_read: qt_method!(fn(&mut self, chat_id: u32)),
    /// Keep a chat at the top of the list, or let it sort by time again.
    pub set_pinned: qt_method!(fn(&mut self, chat_id: u32, pinned: bool)),
    /// Silence a chat, or let it speak again.
    pub set_muted: qt_method!(fn(&mut self, chat_id: u32, muted: bool)),
    /// Move a chat out of the list.
    pub archive: qt_method!(fn(&mut self, chat_id: u32)),
    /// Accept a contact request; QML calls this.
    pub accept_chat: qt_method!(fn(&mut self, chat_id: u32)),
    /// Block a contact request's sender; QML calls this.
    pub block_chat: qt_method!(fn(&mut self, chat_id: u32)),
    /// Move a chat back into the ordinary list.
    pub unarchive: qt_method!(fn(&mut self, chat_id: u32)),
    /// Delete a chat and its messages on this device.
    pub delete_chat: qt_method!(fn(&mut self, chat_id: u32)),

    /// Counts refreshes, so a slow answer to an older question cannot land
    /// on top of a newer one.
    ///
    /// Pushing the archived page sets `account_id` and `archived` in
    /// whatever order QML chooses, and each starts its own fetch -- one
    /// for the ordinary list, one for the archived. Without this the
    /// slower answer wins, and the archived page shows ordinary chats
    /// until it is opened a second time. Typing in the search field starts
    /// one per keystroke for the same reason.
    generation: u64,
}

impl ChatList {
    /// How many rows there are.
    pub fn count(&self) -> u32 {
        u32::try_from(self.rows.borrow().iter().count()).unwrap_or(u32::MAX)
    }

    /// Unread messages across every chat.
    pub fn unread_total(&self) -> u32 {
        self.rows
            .borrow()
            .iter()
            .fold(0u32, |total, row| total.saturating_add(row.unread_count))
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
        self.refresh(Refresh::All);
    }

    /// Apply one core event.
    pub fn handle_event(&mut self, context_id: u32, kind: QString, payload_json: QString) {
        if context_id != self.account_id || self.account_id == 0 {
            return;
        }
        let kind = kind.to_string();
        if !matches!(
            kind.as_str(),
            "IncomingMsg"
                | "MsgsChanged"
                | "MsgsNoticed"
                | "MsgDelivered"
                | "MsgRead"
                | "MsgFailed"
                // Pinning, muting, archiving and deleting land here.
                | "ChatModified"
                | "ChatDeleted"
                | "ChatlistChanged"
                | "ChatlistItemChanged"
                // The core dropped events it could not queue. Whatever it
                // was, this model did not see it, so nothing it holds can
                // be trusted.
                | "EventChannelOverflow"
        ) {
            return;
        }
        let payload: serde_json::Value =
            serde_json::from_str(&payload_json.to_string()).unwrap_or_default();
        // A chat id names the one row that changed. Without one -- absent,
        // null, or 0 -- the core is saying it could not work out which
        // rows are affected and every visible one has to be re-read.
        // Upstream's own words for `ChatlistItemChanged`: "If chat_id is
        // set to None, then all currently visible chats need to be
        // rerendered".
        // An overflow carries no chat id and could have hidden anything.
        if kind == "EventChannelOverflow" {
            self.refresh(Refresh::All);
            return;
        }
        let chat_id = payload
            .get("chatId")
            .and_then(serde_json::Value::as_u64)
            .and_then(|id| u32::try_from(id).ok())
            .filter(|id| *id != 0);
        let scope = chat_id.map_or(Refresh::All, Refresh::One);
        // Worth telling anyone listening about, but only a genuinely new
        // message: MsgsChanged and friends fire for messages we sent, for
        // read receipts, and for a chat being pinned.
        let announce = if kind == "IncomingMsg" { chat_id } else { None };
        self.refresh_announcing(scope, announce);
    }

    /// Set the query and reload if it changed.
    pub fn set_query(&mut self, query: QString) {
        if self.query.to_string() != query.to_string() {
            self.query = query;
            self.query_changed();
            self.refresh(Refresh::All);
        }
    }

    /// Show the archived chats, or the ordinary ones.
    pub fn set_archived(&mut self, archived: bool) {
        if self.archived != archived {
            self.archived = archived;
            self.archived_changed();
            self.refresh(Refresh::All);
        }
    }

    /// List only chats a message can be forwarded into.
    pub fn set_for_forwarding(&mut self, for_forwarding: bool) {
        if self.for_forwarding != for_forwarding {
            self.for_forwarding = for_forwarding;
            self.for_forwarding_changed();
            self.refresh(Refresh::All);
        }
    }

    /// Accept a contact request, so its chat becomes an ordinary one.
    pub fn accept_chat(&mut self, chat_id: u32) {
        self.act(chat_id, "accept_chat", serde_json::Value::Null);
    }

    /// Block the sender of a contact request.
    pub fn block_chat(&mut self, chat_id: u32) {
        self.act(chat_id, "block_chat", serde_json::Value::Null);
    }

    /// Mark everything in a chat read.
    pub fn mark_read(&mut self, chat_id: u32) {
        self.act(chat_id, "marknoticed_chat", serde_json::Value::Null);
    }

    /// Pin a chat, or unpin it.
    pub fn set_pinned(&mut self, chat_id: u32, pinned: bool) {
        let visibility = if pinned { "Pinned" } else { "Normal" };
        self.act(chat_id, "set_chat_visibility", json!([visibility]));
    }

    /// Mute a chat, or unmute it.
    pub fn set_muted(&mut self, chat_id: u32, muted: bool) {
        let kind = if muted { "Forever" } else { "NotMuted" };
        self.act(chat_id, "set_chat_mute_duration", json!([{"kind": kind}]));
    }

    /// Move a chat out of the list.
    pub fn archive(&mut self, chat_id: u32) {
        self.act(chat_id, "set_chat_visibility", json!(["Archived"]));
    }

    /// Move a chat back into the ordinary list.
    pub fn unarchive(&mut self, chat_id: u32) {
        self.act(chat_id, "set_chat_visibility", json!(["Normal"]));
    }

    /// Delete a chat and its messages on this device.
    pub fn delete_chat(&mut self, chat_id: u32) {
        self.act(chat_id, "delete_chat", serde_json::Value::Null);
    }

    /// Call `method` with `(account, chat)` plus `extra`, then refresh the
    /// row it acted on: the core does not announce every one of these.
    fn act(&mut self, chat_id: u32, method: &'static str, extra: serde_json::Value) {
        let account_id = self.account_id;
        if account_id == 0 || chat_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };

        let mut params = vec![json!(account_id), json!(chat_id)];
        if let serde_json::Value::Array(rest) = extra {
            params.extend(rest);
        }

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<(), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(()) => this.borrow_mut().refresh(Refresh::One(chat_id)),
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            let result = rpc
                .call::<_, ()>(method, params)
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Bring the model in line with the core.
    ///
    /// [`Refresh::One`] refetches the chat it names along with any chat not
    /// in the model yet, and reuses every other row -- so a message
    /// arriving in one chat costs one entry listing and one item fetch, not
    /// a rebuild. [`Refresh::All`] refetches the lot, which is what the
    /// core asks for when it reports a change it cannot attribute.
    fn refresh(&mut self, scope: Refresh) {
        self.refresh_announcing(scope, None);
    }

    /// [`Self::refresh`], and afterwards say that a message landed in
    /// `announce` -- once the row for it holds the new preview.
    fn refresh_announcing(&mut self, scope: Refresh, announce: Option<u32>) {
        let account_id = self.account_id;
        if account_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        let query = self.query.to_string();
        let archived = self.archived;
        let for_forwarding = self.for_forwarding;
        let cached: Vec<ChatListItem> = self.rows.borrow().iter().cloned().collect();
        // A set, not a list: this is asked once per entry, and a long chat
        // list would otherwise make the scan quadratic.
        let known: HashSet<u32> = cached.iter().map(|row| row.chat_id).collect();

        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Vec<ChatListItem>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            // Answered after something newer was asked: this is the answer
            // to a question that no longer describes what is on screen.
            if this.borrow().generation != generation {
                return;
            }
            match result {
                Ok(target) => {
                    {
                        // Counted before the rows are set, from the list
                        // about to become them: setting the rows is what
                        // makes QML read the counts back.
                        let pinned = target.iter().filter(|row| row.is_pinned).count();
                        let mut this_mut = this.borrow_mut();
                        this_mut.pinned_count = u32::try_from(pinned).unwrap_or(u32::MAX);
                        this_mut.unpinned_count =
                            u32::try_from(target.len() - pinned).unwrap_or(u32::MAX);
                    }
                    {
                        let this_ref = this.borrow();
                        let mut rows = this_ref.rows.borrow_mut();
                        reconcile(&mut rows, target);
                    }
                    this.borrow().rows_changed();
                    if let Some(chat_id) = announce {
                        let announcement = this
                            .borrow()
                            .rows
                            .borrow()
                            .iter()
                            .find(|row| row.chat_id == chat_id && !row.is_muted)
                            .map(|row| (row.name.clone(), row.preview.clone()));
                        if let Some((name, preview)) = announcement {
                            this.borrow().message_arrived(chat_id, name, preview);
                        }
                    }
                }
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            let result = async {
                let entries =
                    chat_entries(&rpc, account_id, &query, archived, for_forwarding).await?;
                let wanted: Vec<u32> = entries
                    .iter()
                    .copied()
                    .filter(|id| match scope {
                        Refresh::All => true,
                        Refresh::One(chat) => *id == chat || !known.contains(id),
                    })
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

/// How much of the list a refresh has to re-read from the core.
#[derive(Clone, Copy)]
enum Refresh {
    /// Every visible row. What the core means by a change it reports
    /// without naming a chat, and what `reload` has to do to be worth
    /// calling at all.
    All,
    /// This chat, plus any chat not in the model yet.
    One(u32),
}

/// Move, insert and remove rows until the model matches `target`.
///
/// The common case -- one chat moving to the top -- is one remove and one
/// insert, so every other row keeps its identity and the view keeps its
/// place.
fn reconcile(rows: &mut ChatListModel, target: Vec<ChatListItem>) {
    // The ids as they stand, kept in step with the model rather than read
    // back out of it each time round: rebuilding this per row, and reaching
    // into the model with `nth`, is what made a no-op refresh cost a scan
    // of the whole list for every chat in it.
    let mut current: Vec<u32> = rows.iter().map(|row| row.chat_id).collect();
    let keep: HashSet<u32> = target.iter().map(|row| row.chat_id).collect();

    // Gone from the core: dropped first, so what follows only ever moves
    // rows that are staying.
    let mut index = 0;
    while index < current.len() {
        if keep.contains(&current[index]) {
            index += 1;
        } else {
            rows.remove(index);
            current.remove(index);
        }
    }

    for (index, wanted) in target.iter().enumerate() {
        // Everything before `index` is already where the core wants it, so
        // only the tail is worth looking through.
        let found = current
            .iter()
            .skip(index)
            .position(|id| *id == wanted.chat_id)
            .map(|offset| index + offset);
        match found {
            Some(at) if at == index => {
                if rows[index] != *wanted {
                    rows.change_line(index, wanted.clone());
                }
            }
            Some(at) => {
                rows.remove(at);
                rows.insert(index, wanted.clone());
                let id = current.remove(at);
                current.insert(index, id);
            }
            None => {
                rows.insert(index, wanted.clone());
                current.insert(index, wanted.chat_id);
            }
        }
    }
}

/// The account's chat ids, in the order the core wants them shown.
/// `DC_GCL_ARCHIVED_ONLY`: the archived chats rather than the ordinary
/// ones. The two lists are disjoint, which is why this is a mode and not a
/// filter over what is already loaded.
const ARCHIVED_ONLY: u32 = 0x01;

/// `DC_GCL_FOR_FORWARDING`: only chats a message can be forwarded into.
const FOR_FORWARDING: u32 = 0x08;

async fn chat_entries(
    rpc: &RpcClient,
    account_id: u32,
    query: &str,
    archived: bool,
    for_forwarding: bool,
) -> Result<Vec<u32>, String> {
    // The core does the matching. Filtering the loaded rows instead would
    // only ever find chats that happened to be on screen already.
    //
    // A query of nothing but spaces is rejected by the core outright, so
    // it is trimmed here and treated as no query at all rather than
    // swapping the list for an error banner.
    let query = query.trim();
    let mut flags = 0;
    if for_forwarding {
        flags |= FOR_FORWARDING;
    }

    if archived && !query.is_empty() {
        // Two calls, because the core has no single one that means
        // "archived chats matching this". Verified against the pinned
        // binary: with ARCHIVED_ONLY set it never looks at the query --
        // asking for archived chats matching "Beta" returns the archived
        // "Alpha group" all the same -- while a plain query searches every
        // chat and *does* include archived ones. So the archived list
        // intersected with the hits is exactly what this page is asking
        // for.
        let archived_only: Vec<u32> = rpc
            .call(
                "get_chatlist_entries",
                (
                    account_id,
                    Some(flags | ARCHIVED_ONLY),
                    Option::<String>::None,
                    Option::<u32>::None,
                ),
            )
            .await
            .map_err(|err| err.to_string())?;
        let matching: Vec<u32> = rpc
            .call(
                "get_chatlist_entries",
                (
                    account_id,
                    if flags == 0 { None } else { Some(flags) },
                    Some(query.to_string()),
                    Option::<u32>::None,
                ),
            )
            .await
            .map_err(|err| err.to_string())?;
        // The archived list's order, kept: it is the one the reader sees
        // when nothing is typed, and the search should not reshuffle it.
        let hits: HashSet<u32> = matching.into_iter().collect();
        return Ok(archived_only
            .into_iter()
            .filter(|chat_id| hits.contains(chat_id))
            .collect());
    }

    if archived {
        flags |= ARCHIVED_ONLY;
    }
    let flags = if flags == 0 { None } else { Some(flags) };
    let query = if query.is_empty() {
        None
    } else {
        Some(query.to_string())
    };
    rpc.call(
        "get_chatlist_entries",
        (account_id, flags, query, Option::<u32>::None),
    )
    .await
    .map_err(|err| err.to_string())
}

/// Rows for the given chats, in one call.
pub(crate) async fn chat_items(
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
                    preview: text_at(&item, "summaryText2"),
                    preview_sender: text_at(&item, "summaryText1"),
                    unread_count: number_at(&item, "freshMessageCounter"),
                    // The core counts in milliseconds here and in seconds
                    // on a message; the UI wants one unit.
                    last_updated: item
                        .get("lastUpdated")
                        .and_then(serde_json::Value::as_i64)
                        .unwrap_or(0)
                        / 1000,
                    summary_state: number_at(&item, "summaryStatus"),
                    is_encrypted: flag_at(&item, "isEncrypted"),
                    is_pinned: flag_at(&item, "isPinned"),
                    is_muted: flag_at(&item, "isMuted"),
                    is_contact_request: flag_at(&item, "isContactRequest"),
                    color: text_at(&item, "color"),
                    avatar_path: text_at(&item, "avatarPath"),
                },
            )
        })
        .collect())
}

/// A string field, empty when absent or null.
fn text_at(item: &serde_json::Value, field: &str) -> QString {
    item.get(field)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .into()
}

/// A counter, 0 when absent.
fn number_at(item: &serde_json::Value, field: &str) -> u32 {
    item.get(field)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0)
}

/// A flag, false when absent.
fn flag_at(item: &serde_json::Value, field: &str) -> bool {
    item.get(field)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

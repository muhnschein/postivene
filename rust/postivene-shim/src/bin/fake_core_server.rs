//! A `deltachat-rpc-server` double that records what it was asked.
//!
//! Every request is appended as one JSON line to `POSTIVENE_FAKE_JOURNAL`,
//! so tests can assert which calls a UI action makes, in what order, with
//! what parameters. `get_next_event_batch` is left out: the client polls it
//! in a loop and would bury the sequence.
//!
//! Behaviour is keyed on input rather than an environment switch, so one
//! process can drive success and failure: a QR payload or address
//! containing `fail` is rejected.

use std::collections::VecDeque;
use std::io::Write;
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

/// One account, as `get_all_accounts` reports it.
struct Account {
    id: u32,
    configured: bool,
}

#[derive(Default)]
struct State {
    accounts: Vec<Account>,
    /// Events waiting to be handed out by `get_next_event_batch`.
    events: VecDeque<Value>,
    /// Message ids per chat, oldest first. Seeded on first use.
    chats: std::collections::BTreeMap<u32, Vec<u32>>,
    /// Chat ids in display order, most recent first.
    chat_order: Vec<u32>,
    next_message_id: u32,
    /// Contact id -> address. Seeded with two known contacts.
    contacts: std::collections::BTreeMap<u32, String>,
    next_contact_id: u32,
    next_chat_id: u32,
    /// Members added to each created group.
    group_members: std::collections::BTreeMap<u32, Vec<u32>>,
}

impl State {
    fn account_list(&self) -> Value {
        Value::Array(
            self.accounts
                .iter()
                .map(|account| {
                    json!({
                        "id": account.id,
                        "kind": if account.configured { "Configured" } else { "Unconfigured" },
                        "displayName": "",
                        "addr": "",
                    })
                })
                .collect(),
        )
    }

    /// Two chats with a couple of messages each, so a test can watch a
    /// model load them and then take in one more.
    fn seed_chats(&mut self) {
        if self.chats.is_empty() {
            self.chats.insert(1, vec![1, 2]);
            self.chats.insert(2, vec![10]);
            self.chat_order = vec![1, 2];
            self.next_message_id = 100;
            self.contacts.insert(10, "ada@example.org".to_string());
            self.contacts.insert(11, "grace@example.org".to_string());
            self.next_contact_id = 100;
            self.next_chat_id = 500;
        }
    }

    /// Append a message to a chat and announce it, the way a send or an
    /// incoming message does.
    fn add_message(&mut self, account_id: u32, chat_id: u32) -> u32 {
        self.seed_chats();
        self.next_message_id += 1;
        let id = self.next_message_id;
        self.chats.entry(chat_id).or_default().push(id);
        // A message moves its chat to the top, which is what makes a chat
        // list reorder rather than merely change.
        self.chat_order.retain(|chat| *chat != chat_id);
        self.chat_order.insert(0, chat_id);
        self.events.push_back(json!({
            "contextId": account_id,
            "event": {"kind": "IncomingMsg", "chatId": chat_id, "msgId": id},
        }));
        id
    }

    /// Configure an account and queue the progress events the core emits:
    /// permille steps, then 1000 for done.
    fn configure(&mut self, account_id: u32) {
        for account in &mut self.accounts {
            if account.id == account_id {
                account.configured = true;
            }
        }
        for progress in [300_u32, 1000] {
            self.events.push_back(json!({
                "contextId": account_id,
                "event": {"kind": "ConfigureProgress", "progress": progress, "comment": null},
            }));
        }
    }
}

fn journal(method: &str, params: &Value) {
    let Ok(path) = std::env::var("POSTIVENE_FAKE_JOURNAL") else {
        return;
    };
    // Polling noise would bury the sequence.
    if method == "get_next_event_batch" {
        return;
    }
    let line = json!({"method": method, "params": params}).to_string() + "\n";
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        // One write, not `writeln!`'s two: requests are handled
        // concurrently, and the newline landing separately tore lines into
        // each other.
        let _ = file.write_all(line.as_bytes());
    }
}

/// One message, shaped like the real core's. Seeded message 1 quotes,
/// 2 is unread, and 10 carries an image, so one fetch covers the cases the
/// conversation view has to render.
fn message_object(msg: u64) -> Value {
    // 2023-11-14T22:13:20Z and a day later, so a day separator has
    // something to separate.
    let timestamp = if msg == 1 {
        1_700_000_000
    } else {
        1_700_090_000
    };
    let mut message = json!({
        "kind": "message",
        "text": format!("message {msg}"),
        "fromId": 10,
        "timestamp": timestamp,
        "showPadlock": true,
        // One seeded message is unread, so a test can watch the read
        // receipt go out.
        "state": if msg == 2 { 10 } else { 16 },
        "isInfo": false,
        "viewType": "Text",
        "sender": {"id": 10, "displayName": "Ada Lovelace", "color": "#00875a"},
        "overrideSenderName": null,
        "quote": null,
        "file": null,
        "fileName": null,
        "dimensionsWidth": 0,
        "dimensionsHeight": 0,
    });
    if msg == 1 {
        message["quote"] = json!({"text": "earlier", "authorDisplayName": "Grace Hopper"});
    }
    if msg == 10 {
        message["viewType"] = json!("Image");
        message["file"] = json!("/tmp/postivene-fake/photo.jpg");
        message["fileName"] = json!("photo.jpg");
        message["dimensionsWidth"] = json!(640);
        message["dimensionsHeight"] = json!(480);
    }
    message
}

/// True for the inputs that stand in for "the server cannot be reached".
fn should_fail(value: &str) -> bool {
    value.contains("fail")
}

/// A reply delay in milliseconds, from `var`. Lets a test fix the order in
/// which two replies land.
fn delay(var: &str) -> std::time::Duration {
    std::time::Duration::from_millis(
        std::env::var(var)
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0),
    )
}

#[allow(clippy::too_many_lines)]
#[tokio::main]
async fn main() {
    let state = Arc::new(Mutex::new(State::default()));
    let stdout = Arc::new(Mutex::new(tokio::io::stdout()));

    // Stands in for the server dying under the client.
    if let Ok(after) = std::env::var("POSTIVENE_FAKE_EXIT_AFTER_MS") {
        if let Ok(millis) = after.parse() {
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(millis)).await;
                std::process::exit(0);
            });
        }
    }
    let mut lines = BufReader::new(tokio::io::stdin()).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let state = state.clone();
        let stdout = stdout.clone();
        tokio::spawn(async move {
            let Ok(request) = serde_json::from_str::<Value>(&line) else {
                return;
            };
            let Some(id) = request.get("id").cloned() else {
                return;
            };
            let method = request
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let params = request.get("params").cloned().unwrap_or(Value::Null);
            journal(&method, &params);

            let positional = |index: usize| -> Value {
                params
                    .as_array()
                    .and_then(|array| array.get(index))
                    .cloned()
                    .unwrap_or(Value::Null)
            };
            let account_id = || -> u32 {
                positional(0)
                    .as_u64()
                    .and_then(|value| u32::try_from(value).ok())
                    .unwrap_or_default()
            };

            let response = match method.as_str() {
                "get_system_info" => ok(&id, &json!({"name": "fake-core-server"})),
                "get_all_accounts" => {
                    let list = state.lock().await.account_list();
                    ok(&id, &list)
                }
                "add_account" => {
                    let mut state = state.lock().await;
                    let next = u32::try_from(state.accounts.len()).unwrap_or(0) + 1;
                    state.accounts.push(Account {
                        id: next,
                        configured: false,
                    });
                    ok(&id, &json!(next))
                }
                "set_config"
                | "start_io"
                | "stop_ongoing_process"
                | "marknoticed_chat"
                | "markseen_msgs"
                | "set_chat_visibility"
                | "set_chat_mute_duration"
                | "resend_messages" => ok(&id, &Value::Null),
                "add_transport_from_qr" => {
                    let qr = positional(1).as_str().unwrap_or_default().to_string();
                    if should_fail(&qr) {
                        err(&id, "cannot resolve chatmail server")
                    } else {
                        state.lock().await.configure(account_id());
                        ok(&id, &Value::Null)
                    }
                }
                "add_or_update_transport" => {
                    let param = positional(1);
                    let addr = param
                        .get("addr")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let has_password = param
                        .get("password")
                        .and_then(Value::as_str)
                        .is_some_and(|password| !password.is_empty());
                    if addr.is_empty() || !has_password {
                        // Matches the real core: a malformed
                        // EnteredLoginParam is an invalid-params error.
                        err(&id, "invalid params: addr and password are required")
                    } else if should_fail(addr) {
                        err(&id, "could not connect to server")
                    } else {
                        state.lock().await.configure(account_id());
                        ok(&id, &Value::Null)
                    }
                }
                "list_transports" => ok(&id, &json!([{"addr": "someone@example.org"}])),
                "get_contacts" => {
                    let mut state = state.lock().await;
                    state.seed_chats();
                    let query = positional(2).as_str().unwrap_or_default().to_lowercase();
                    let contacts: Vec<Value> = state
                        .contacts
                        .iter()
                        .filter(|(_, address)| {
                            query.is_empty() || address.to_lowercase().contains(&query)
                        })
                        .map(|(contact, address)| {
                            json!({
                                "id": contact,
                                "address": address,
                                "displayName": address.split('@').next().unwrap_or(address),
                                "isVerified": false,
                                "isKeyContact": true,
                            })
                        })
                        .collect();
                    ok(&id, &Value::Array(contacts))
                }
                "create_contact" => {
                    let mut state = state.lock().await;
                    state.seed_chats();
                    let address = positional(1).as_str().unwrap_or_default().to_string();
                    // An address already known keeps its id, as upstream does.
                    let existing = state
                        .contacts
                        .iter()
                        .find(|(_, known)| **known == address)
                        .map(|(contact, _)| *contact);
                    let contact = existing.unwrap_or_else(|| {
                        state.next_contact_id += 1;
                        state.next_contact_id
                    });
                    state.contacts.insert(contact, address);
                    ok(&id, &json!(contact))
                }
                // A join and a one-to-one both end in a fresh chat at the top.
                "create_chat_by_contact_id" | "secure_join" => {
                    let mut state = state.lock().await;
                    state.seed_chats();
                    state.next_chat_id += 1;
                    let chat = state.next_chat_id;
                    state.chats.insert(chat, Vec::new());
                    state.chat_order.insert(0, chat);
                    ok(&id, &json!(chat))
                }
                "create_group_chat" => {
                    let mut state = state.lock().await;
                    state.seed_chats();
                    state.next_chat_id += 1;
                    let chat = state.next_chat_id;
                    state.chats.insert(chat, Vec::new());
                    state.chat_order.insert(0, chat);
                    state.group_members.insert(chat, Vec::new());
                    ok(&id, &json!(chat))
                }
                "add_contact_to_chat" => {
                    let mut state = state.lock().await;
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let contact = positional(2)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    state.group_members.entry(chat).or_default().push(contact);
                    ok(&id, &Value::Null)
                }
                "check_qr" => {
                    let content = positional(1).as_str().unwrap_or_default().to_string();
                    // Enough to tell an invite from anything else, which is
                    // the only distinction the shim makes.
                    let kind = if content.contains("i.delta.chat")
                        || content.starts_with("OPENPGP4FPR:")
                    {
                        "askVerifyContact"
                    } else if content.starts_with("dcaccount:") || content.starts_with("DCACCOUNT:")
                    {
                        "account"
                    } else {
                        "text"
                    };
                    ok(&id, &json!({"kind": kind}))
                }
                "get_chat_securejoin_qr_code" => ok(
                    &id,
                    &json!("https://i.delta.chat/#ABCDEF&a=me%40example.org&n=Me"),
                ),
                "delete_messages" => {
                    let ids: Vec<u32> = positional(1)
                        .as_array()
                        .map(|array| {
                            array
                                .iter()
                                .filter_map(Value::as_u64)
                                .filter_map(|value| u32::try_from(value).ok())
                                .collect()
                        })
                        .unwrap_or_default();
                    let account = account_id();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    for messages in state.chats.values_mut() {
                        messages.retain(|msg| !ids.contains(msg));
                    }
                    // The core announces a deletion; the model reloads on it.
                    state.events.push_back(json!({
                        "contextId": account,
                        "event": {"kind": "MsgsChanged", "chatId": 0, "msgId": 0},
                    }));
                    ok(&id, &Value::Null)
                }
                "delete_chat" => {
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    state.chats.remove(&chat);
                    state.chat_order.retain(|id| *id != chat);
                    ok(&id, &Value::Null)
                }
                "get_basic_chat_info" => {
                    let chat = positional(1).as_u64().unwrap_or_default();
                    // Chat 2 is the group, so a test has both kinds.
                    ok(
                        &id,
                        &json!({
                            "id": chat,
                            "chatType": if chat == 2 { "Group" } else { "Single" },
                            "name": format!("chat {chat}"),
                        }),
                    )
                }
                "get_chatlist_entries" => {
                    let mut state = state.lock().await;
                    state.seed_chats();
                    ok(&id, &json!(state.chat_order.clone()))
                }
                "get_chatlist_items_by_entries" => {
                    let ids: Vec<u64> = positional(1)
                        .as_array()
                        .map(|array| array.iter().filter_map(Value::as_u64).collect())
                        .unwrap_or_default();
                    let mut items = serde_json::Map::new();
                    for chat in ids {
                        items.insert(
                            chat.to_string(),
                            json!({
                                "kind": "ChatListItem",
                                "name": format!("chat {chat}"),
                                "summaryText2": format!("last in {chat}"),
                                "freshMessageCounter": 0,
                                "isEncrypted": true,
                            }),
                        );
                    }
                    ok(&id, &Value::Object(items))
                }
                "get_message_list_items" => {
                    let mut state = state.lock().await;
                    state.seed_chats();
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let items: Vec<Value> = state
                        .chats
                        .get(&chat)
                        .cloned()
                        .unwrap_or_default()
                        .into_iter()
                        .map(|msg| json!({"kind": "message", "msg_id": msg}))
                        .collect();
                    ok(&id, &Value::Array(items))
                }
                "get_messages" => {
                    tokio::time::sleep(delay("POSTIVENE_FAKE_FETCH_DELAY_MS")).await;
                    // One call for many ids: the point of the batch.
                    let ids: Vec<u64> = positional(1)
                        .as_array()
                        .map(|array| array.iter().filter_map(Value::as_u64).collect())
                        .unwrap_or_default();
                    let mut loaded = serde_json::Map::new();
                    for msg in ids {
                        loaded.insert(msg.to_string(), message_object(msg));
                    }
                    ok(&id, &Value::Object(loaded))
                }
                "misc_send_msg" => {
                    let account = account_id();
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let text = positional(2).as_str().unwrap_or_default().to_string();
                    if should_fail(&text) {
                        // The real core reports a failed send as an Error
                        // event, not only as a failed call.
                        state.lock().await.events.push_back(json!({
                            "contextId": account,
                            "event": {"kind": "Error", "msg": "could not send"},
                        }));
                        err(&id, "could not send")
                    } else {
                        let msg = state.lock().await.add_message(account, chat);
                        // The event is queued above, so a delay here puts it
                        // ahead of this call's own reply -- the ordering the
                        // real core can produce, and the one that duplicated a
                        // sent row.
                        tokio::time::sleep(delay("POSTIVENE_FAKE_SEND_DELAY_MS")).await;
                        ok(
                            &id,
                            &json!([
                                msg,
                                {"text": text, "fromId": 1, "timestamp": 0,
                                 "showPadlock": true, "state": 20}
                            ]),
                        )
                    }
                }
                "get_next_event_batch" => {
                    // Blocks when empty, like the real long poll.
                    loop {
                        let queued: Vec<Value> = state.lock().await.events.drain(..).collect();
                        if !queued.is_empty() {
                            break ok(&id, &Value::Array(queued));
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                    }
                }
                _ => err(&id, "method not found"),
            };

            let mut out = stdout.lock().await;
            let _ = out.write_all(response.to_string().as_bytes()).await;
            let _ = out.write_all(b"\n").await;
            let _ = out.flush().await;
        });
    }
}

fn ok(id: &Value, result: &Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn err(id: &Value, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32000, "message": message}})
}

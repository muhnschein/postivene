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

use chrono::{Local, TimeZone};
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
    /// The archived list, which is disjoint from `chat_order`.
    archived_order: Vec<u32>,
    next_message_id: u32,
    /// Contact id -> address. Seeded with two known contacts.
    contacts: std::collections::BTreeMap<u32, String>,
    next_chat_id: u32,
    /// Members added to each created group.
    group_members: std::collections::BTreeMap<u32, Vec<u32>>,
    /// Config values per account, so a set can be read back.
    config: std::collections::BTreeMap<(u32, String), String>,
    /// The unsent text each chat is holding. The core keeps drafts, so a
    /// fake standing in for it has to as well.
    drafts: std::collections::BTreeMap<u32, String>,
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
            // A chat long enough to page through, when a test asks for
            // one. Ids count up from 1, so the newest is the highest and
            // the seeded quote and picture keep the ids they always had.
            let long = std::env::var("POSTIVENE_FAKE_LONG_CHAT")
                .ok()
                .and_then(|count| count.parse::<u32>().ok())
                .filter(|count| *count > 2);
            self.chats.insert(
                1,
                long.map_or_else(|| vec![1, 2], |count| (1..=count).collect()),
            );
            self.chats.insert(2, vec![10]);
            self.chat_order = vec![1, 2];
            // Chat 3 is archived, and appears in no ordinary listing.
            // Without it, a model asking for the archived list and a model
            // asking for the ordinary one are indistinguishable, and a
            // test cannot tell which answer it got.
            self.chats.insert(3, vec![30]);
            // An empty archive is its own case: the page hides its search
            // field when there is nothing to search, and with a chat
            // always present no test could see that.
            self.archived_order = if std::env::var("POSTIVENE_FAKE_NO_ARCHIVED").is_ok() {
                Vec::new()
            } else {
                vec![3]
            };
            // Above whatever the seeded chat used, so a message added
            // while a test runs cannot collide with one already in it.
            self.next_message_id = long.unwrap_or(0).max(100);
            self.contacts.insert(10, "ada@example.org".to_string());
            self.contacts.insert(11, "grace@example.org".to_string());
            self.next_chat_id = 500;
        }
    }

    /// Which chat a message is in. The real core carries this on the
    /// message object; a search result is unusable without it.
    fn chat_of(&self, message_id: u32) -> u32 {
        self.chats
            .iter()
            .find(|(_, messages)| messages.contains(&message_id))
            .map_or(0, |(chat, _)| *chat)
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
/// What a message says.
///
/// Short by default, because most tests assert on it. `POSTIVENE_FAKE_WORDY`
/// makes it long enough to wrap, which is what a real conversation looks
/// like to a view: a row's height then depends on how wide it is drawn, and
/// changes when that changes. Nothing else here can make a laid-out row
/// change size, and a view that cannot be made to shift cannot be shown to
/// hold its place.
fn wordy(msg: u64) -> String {
    let text = format!("message {msg}");
    if std::env::var_os("POSTIVENE_FAKE_WORDY").is_none() {
        return text;
    }
    format!("{text}, and then a good deal more of it, long enough that where it wraps depends on how wide the row is drawn")
}

/// When a message was sent.
///
/// 2023-11-14T22:13:20Z and a day later, so a day separator has something to
/// separate. Its own function because the day markers below have to agree
/// with it: a placeholder row takes its day from the marker and a filled-in
/// row from the message, and the two disagreeing is the heading changing
/// under the reader.
fn message_timestamp(msg: u64) -> i64 {
    if msg == 1 {
        1_700_000_000
    } else {
        1_700_090_000
    }
}

/// Local midnight starting the day `timestamp` falls in, which is what the
/// real core gives as a day marker -- checked against it in three zones.
fn day_start(timestamp: i64) -> i64 {
    let Some(when) = Local.timestamp_opt(timestamp, 0).single() else {
        return timestamp.div_euclid(86_400) * 86_400;
    };
    when.date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|midnight| midnight.and_local_timezone(Local).earliest())
        .map_or(timestamp, |midnight| midnight.timestamp())
}

fn message_object(msg: u64) -> Value {
    let timestamp = message_timestamp(msg);
    let info_first = msg == 1 && std::env::var_os("POSTIVENE_FAKE_INFO_FIRST").is_some();
    let mut message = json!({
        "kind": "message",
        "text": if info_first {
            "Messages are end-to-end encrypted.".to_string()
        } else {
            wordy(msg)
        },
        "fromId": 10,
        "timestamp": timestamp,
        "showPadlock": true,
        // One seeded message is unread, and so is anything added while
        // the test runs: an arrival is the case worth covering.
        "state": if msg == 2 || msg > 100 { 10 } else { 16 },
        // The first message of a real chat is the core's own "messages are
        // end-to-end encrypted" notice, which is the row the day heading
        // was reported drawn on top of.
        "isInfo": info_first,
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
                "remove_account" => {
                    let mut state = state.lock().await;
                    let gone = positional(0).as_u64().unwrap_or(0);
                    let gone = u32::try_from(gone).unwrap_or(0);
                    state.accounts.retain(|account| account.id != gone);
                    ok(&id, &Value::Null)
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
                "set_config" => {
                    let account = positional(0)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let key = positional(1).as_str().unwrap_or_default().to_string();
                    let mut state = state.lock().await;
                    // Null clears, as the real core does -- and an empty
                    // string is a value, not a clear. `selfavatar` refuses
                    // one outright there, which is why the app has to send
                    // null rather than "".
                    match positional(2).as_str() {
                        Some(value) => {
                            state.config.insert((account, key), value.to_string());
                        }
                        None => {
                            state.config.remove(&(account, key));
                        }
                    }
                    ok(&id, &Value::Null)
                }
                "get_config" => {
                    let account = positional(0)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let key = positional(1).as_str().unwrap_or_default().to_string();
                    let state = state.lock().await;
                    let value = state
                        .config
                        .get(&(account, key))
                        .cloned()
                        .map_or(Value::Null, Value::String);
                    ok(&id, &value)
                }
                "start_io"
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
                    // DC_GCL_ARCHIVED_ONLY. The two listings are disjoint,
                    // so this is which list, not a filter on one.
                    let archived_only = positional(1).as_u64().unwrap_or(0) & 0x01 != 0;
                    // Verified against the real core: with ARCHIVED_ONLY
                    // set it never looks at the query, and a plain query
                    // searches every chat *including* archived ones. A
                    // fake that filtered the archived list by the query
                    // would let a one-call implementation pass here and
                    // fail on a device.
                    let query = positional(2).as_str().unwrap_or("").to_string();
                    let searching = !query.is_empty();
                    let entries = if archived_only {
                        // The query is deliberately not consulted.
                        state.archived_order.clone()
                    } else if searching {
                        // A plain query reaches archived chats too.
                        let mut all = state.chat_order.clone();
                        all.extend(state.archived_order.iter().copied());
                        all
                    } else {
                        state.chat_order.clone()
                    };
                    // Lets a test make the *ordinary* listing the slow one,
                    // so an answer to a question the model has already
                    // moved on from arrives last. Requests are handled
                    // concurrently here, as the real core's are, so this
                    // delays only its own reply.
                    if !archived_only {
                        if let Some(delay) = std::env::var("POSTIVENE_FAKE_CHATLIST_DELAY_MS")
                            .ok()
                            .and_then(|value| value.parse::<u64>().ok())
                        {
                            drop(state);
                            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                        }
                    }
                    // The core matches the query itself; so does this, on
                    // the same name a chat item reports.
                    let entries = if searching && !archived_only {
                        let needle = query.to_lowercase();
                        entries
                            .into_iter()
                            .filter(|chat| format!("chat {chat}").contains(&needle))
                            .collect()
                    } else {
                        entries
                    };
                    ok(&id, &json!(entries))
                }
                "get_chatlist_items_by_entries" => {
                    let ids: Vec<u64> = positional(1)
                        .as_array()
                        .map(|array| array.iter().filter_map(Value::as_u64).collect())
                        .unwrap_or_default();
                    let state = state.lock().await;
                    let mut items = serde_json::Map::new();
                    for chat in ids {
                        // A chat holding a draft previews the draft, and
                        // names it in the prefix the row shows in front:
                        // "Draft", and DC_STATE_OUT_DRAFT for the state.
                        // The real core does this itself, which is pinned
                        // in deltachat-jsonrpc/tests/real_server.rs.
                        let draft = u32::try_from(chat)
                            .ok()
                            .and_then(|chat| state.drafts.get(&chat))
                            .filter(|text| !text.is_empty());
                        items.insert(
                            chat.to_string(),
                            json!({
                                "kind": "ChatListItem",
                                "name": format!("chat {chat}"),
                                "summaryText1": draft.map(|_| "Draft"),
                                "summaryText2": draft.map_or_else(
                                    || format!("last in {chat}"),
                                    Clone::clone,
                                ),
                                "summaryStatus": if draft.is_some() { 19 } else { 0 },
                                "freshMessageCounter": 0,
                                "isEncrypted": true,
                                // Chat 1 is pinned, so the ordinary list
                                // has both kinds in it and a test can see
                                // the two headings. The archived list
                                // holds one unpinned chat, which is the
                                // other case: one kind, no headings.
                                "isPinned": chat == 1,
                            }),
                        );
                    }
                    ok(&id, &Value::Object(items))
                }
                "search_messages" => {
                    let mut state = state.lock().await;
                    state.seed_chats();
                    // Three arguments; the third is a chat to search
                    // within, and null means every chat. Verified against
                    // the pinned binary: passing two is rejected.
                    let needle = positional(1).as_str().unwrap_or_default().to_lowercase();
                    let within = positional(2)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok());
                    let mut hits: Vec<u32> = Vec::new();
                    for (chat, messages) in &state.chats {
                        if within.is_some_and(|wanted| wanted != *chat) {
                            continue;
                        }
                        for msg in messages {
                            let text = format!("message {msg}");
                            if !needle.is_empty() && text.contains(&needle) {
                                hits.push(*msg);
                            }
                        }
                    }
                    ok(&id, &json!(hits))
                }
                "get_message_list_items" => {
                    // Delayed with the same knob as get_messages: a real
                    // core takes time over both, and a test that wants two
                    // of these in the air at once needs them to overlap.
                    tokio::time::sleep(delay("POSTIVENE_FAKE_FETCH_DELAY_MS")).await;
                    let mut state = state.lock().await;
                    state.seed_chats();
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    // The fourth argument asks for day markers. The real
                    // core interleaves one before each day's first message,
                    // and the model reads a placeholder row's day off them.
                    let markers = positional(3).as_bool().unwrap_or(false);
                    let mut items: Vec<Value> = Vec::new();
                    let mut day = None;
                    for msg in state.chats.get(&chat).cloned().unwrap_or_default() {
                        if markers {
                            let start = day_start(message_timestamp(u64::from(msg)));
                            if day != Some(start) {
                                items.push(json!({"kind": "dayMarker", "timestamp": start}));
                                day = Some(start);
                            }
                        }
                        items.push(json!({"kind": "message", "msg_id": msg}));
                    }
                    ok(&id, &Value::Array(items))
                }
                "get_messages" => {
                    tokio::time::sleep(delay("POSTIVENE_FAKE_FETCH_DELAY_MS")).await;
                    // One call for many ids: the point of the batch.
                    let ids: Vec<u64> = positional(1)
                        .as_array()
                        .map(|array| array.iter().filter_map(Value::as_u64).collect())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    let mut loaded = serde_json::Map::new();
                    for msg in ids {
                        let mut message = message_object(msg);
                        let chat = u32::try_from(msg).map_or(0, |msg| state.chat_of(msg));
                        message["chatId"] = json!(chat);
                        loaded.insert(msg.to_string(), message);
                    }
                    ok(&id, &Value::Object(loaded))
                }
                // Drafts, which the core keeps per chat. Enough of them to
                // drive the page: one text per chat, read back and cleared.
                "misc_set_draft" => {
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let text = positional(2).as_str().unwrap_or_default().to_string();
                    let mut state = state.lock().await;
                    state.drafts.insert(chat, text);
                    ok(&id, &Value::Null)
                }
                "remove_draft" => {
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    state.drafts.remove(&chat);
                    ok(&id, &Value::Null)
                }
                "get_draft" => {
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let state = state.lock().await;
                    // Null for none and a whole message object for one, as
                    // the real core answers.
                    match state.drafts.get(&chat) {
                        Some(text) => ok(&id, &json!({"text": text, "state": 19})),
                        None => ok(&id, &Value::Null),
                    }
                }
                "misc_send_msg" => {
                    let account = account_id();
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let text = positional(2).as_str().unwrap_or_default().to_string();
                    // The core takes one file and decides the message's
                    // view type from it. Echoed back so the row the sender
                    // sees carries the attachment, as the real one does.
                    let file = positional(3);
                    let file_name = positional(4);
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
                        // The real core reads the file; this reads the
                        // extension, which is enough to tell an image row
                        // from a paperclip one.
                        let extension = file.as_str().and_then(|path| {
                            std::path::Path::new(path)
                                .extension()
                                .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
                        });
                        let view_type = match extension.as_deref() {
                            _ if file.is_null() => "Text",
                            Some("png" | "jpg" | "jpeg") => "Image",
                            _ => "File",
                        };
                        ok(
                            &id,
                            &json!([
                                msg,
                                {"text": text, "fromId": 1, "timestamp": 0,
                                 "showPadlock": true, "state": 20,
                                 "file": file, "fileName": file_name,
                                 "viewType": view_type}
                            ]),
                        )
                    }
                }
                "get_next_event_batch" => {
                    // Blocks when empty, like the real long poll.
                    loop {
                        let queued: Vec<Value> = state.lock().await.events.drain(..).collect();
                        if !queued.is_empty() {
                            // Held back after the batch is taken, not
                            // before the wait: a delay ahead of the loop
                            // would be spent while the queue was still
                            // empty and buy nothing. This is what lets a
                            // test say that a call's own reply is dealt
                            // with before the event the same call
                            // produced -- otherwise which of the two wins
                            // is a race between queued callbacks on the
                            // Qt thread, and a test that quietly depends
                            // on one of them passes on one machine and
                            // fails on another.
                            tokio::time::sleep(delay("POSTIVENE_FAKE_EVENT_DELAY_MS")).await;
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

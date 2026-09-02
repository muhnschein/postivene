//! Integration test against a REAL `deltachat-rpc-server` binary -- the
//! Milestone 1 acceptance check (`get_system_info` over stdio) plus enough
//! offline account/chat operations to prove the transport's assumptions
//! hold against the actual core, not just the in-repo fake.
//!
//! Gated: skipped unless `DELTACHAT_RPC_SERVER` points at a real binary
//! (e.g. one extracted from upstream's `PyPI` wheel or GitHub release), so
//! `cargo test` stays green in environments that don't have one.
//!
//! Everything here is offline -- no account is ever configured against a
//! mail server -- so it runs without network access.

use std::sync::Arc;
use std::time::Duration;

use deltachat_jsonrpc::{spawn_event_loop, RpcClient};
use serde_json::Value;

/// A 1x1 PNG, written out so the core reads real pixels rather than
/// guessing from a name: header, an IHDR saying 1x1 truecolour, one
/// compressed scanline, IEND.
const ONE_PIXEL_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0xc9, 0xfe, 0x92, 0xef, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];

/// A 1x1 GIF. The core classifies it as an animation and reports no
/// dimensions for it, which is the case the conversation has to survive.
const ONE_PIXEL_GIF: &[u8] = &[
    0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0xff, 0xff, 0xff,
    0x00, 0x00, 0x00, 0x21, 0xf9, 0x04, 0x01, 0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x01,
    0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x44, 0x01, 0x00, 0x3b,
];

/// One second of silence as a WAV: PCM, mono, 8 kHz, 8-bit, so the header
/// alone states the length. Built rather than checked in so the size the
/// assertions use is arithmetic rather than a magic number.
fn one_second_wav() -> Vec<u8> {
    let (rate, bits, channels) = (8000_u32, 8_u16, 1_u16);
    // One byte per sample at 8-bit mono, so a second is `rate` bytes.
    let bytes = rate;
    let samples = vec![0x80_u8; bytes as usize];
    let mut wav = Vec::with_capacity(44 + samples.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + bytes).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes()); // bytes per second
    wav.extend_from_slice(&1_u16.to_le_bytes()); // block align
    wav.extend_from_slice(&bits.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&bytes.to_le_bytes());
    wav.extend_from_slice(&samples);
    wav
}

/// Send one file to `chat` and hand back the message the core stored, which
/// is the shape every row is built from.
async fn send_file(
    client: &RpcClient,
    account_id: u32,
    chat_id: u32,
    path: &std::path::Path,
    name: &str,
) -> Value {
    let (message_id, _): (u32, Value) = client
        .call(
            "misc_send_msg",
            (
                account_id,
                chat_id,
                Option::<String>::None,
                Some(path.to_string_lossy().into_owned()),
                Some(name.to_string()),
                Option::<(f64, f64)>::None,
                Option::<u32>::None,
            ),
        )
        .await
        .unwrap_or_else(|err| panic!("misc_send_msg for {name}: {err}"));
    let messages: std::collections::HashMap<u32, Value> = client
        .call("get_messages", (account_id, vec![message_id]))
        .await
        .unwrap_or_else(|err| panic!("get_messages for {name}: {err}"));
    messages[&message_id].clone()
}

/// Resolve the gate's value, treating a relative path as relative to the
/// repository root rather than to the process's working directory.
///
/// Cargo runs an integration test with its working directory set to the
/// *package* root, not the workspace or repository root, so the obvious
/// `DELTACHAT_RPC_SERVER=../vendor/...` -- which is what the README, the
/// Makefile and CI all naturally write -- would otherwise look for the
/// binary under `rust/`. That failure only shows up where the variable is
/// actually set, which is CI, and reads as a missing download rather than a
/// wrong path.
fn resolve(path: &str) -> String {
    let path = std::path::Path::new(path);
    if path.is_absolute() {
        return path.to_string_lossy().into_owned();
    }
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
        .to_string_lossy()
        .into_owned()
}

fn real_server() -> Option<String> {
    match std::env::var("DELTACHAT_RPC_SERVER") {
        Ok(path) if !path.is_empty() => Some(resolve(&path)),
        _ => {
            eprintln!(
                "skipping: set DELTACHAT_RPC_SERVER to a real deltachat-rpc-server binary to run"
            );
            None
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
// One long function on purpose: this walks a single live session through
// the whole surface the app depends on, in order, against one spawned
// server. Splitting it into helpers would either respawn the core per step
// or hide the ordering that is half of what is being pinned.
#[allow(clippy::too_many_lines)]
async fn offline_round_trip_against_real_core() {
    let Some(server) = real_server() else {
        return;
    };

    let accounts_dir =
        std::env::temp_dir().join(format!("postivene-real-server-test-{}", std::process::id()));
    std::fs::create_dir_all(&accounts_dir).expect("create accounts dir");

    let client = Arc::new(
        RpcClient::spawn_with_env(
            &server,
            Vec::<&str>::new(),
            [("DC_ACCOUNTS_PATH", accounts_dir.as_os_str())],
        )
        .await
        .expect("spawn real deltachat-rpc-server"),
    );

    // Milestone 1 acceptance check: get_system_info answers over stdio.
    let info: std::collections::BTreeMap<String, String> = client
        .call_unit("get_system_info")
        .await
        .expect("get_system_info");
    assert!(
        info.contains_key("deltachat_core_version"),
        "unexpected get_system_info keys: {:?}",
        info.keys().collect::<Vec<_>>()
    );

    // Account bootstrap, exactly as DeltaChatCore::add_account does it.
    let account_id: u32 = client.call_unit("add_account").await.expect("add_account");

    // Config round trip, as configure_account does (without `configure`,
    // which would need a live mail server).
    client
        .call::<_, ()>(
            "set_config",
            (account_id, "displayname", Some("Postivene Test")),
        )
        .await
        .expect("set_config");
    let name: Option<String> = client
        .call("get_config", (account_id, "displayname"))
        .await
        .expect("get_config");
    assert_eq!(name.as_deref(), Some("Postivene Test"));

    // Start the event stream BEFORE doing something that emits an event.
    let (mut events, handle) = spawn_event_loop(client.clone());

    // Contact + chat + draft: all offline, and setting a draft emits a
    // MsgsChanged event (documented upstream), which proves real event
    // delivery end to end.
    let contact_id: u32 = client
        .call(
            "create_contact",
            (account_id, "test@example.org", Some("Testy")),
        )
        .await
        .expect("create_contact");
    let chat_id: u32 = client
        .call("create_chat_by_contact_id", (account_id, contact_id))
        .await
        .expect("create_chat_by_contact_id");

    client
        .call::<_, ()>(
            "misc_set_draft",
            (
                account_id,
                chat_id,
                Some("draft from integration test"),
                Option::<String>::None,
                Option::<String>::None,
                Option::<u32>::None,
                Option::<String>::None,
            ),
        )
        .await
        .expect("misc_set_draft");

    let mut saw_msgs_changed = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout_at(deadline, events.recv()).await {
            Ok(Some(event)) => {
                if event.context_id == account_id
                    && event.event.get("kind").and_then(Value::as_str) == Some("MsgsChanged")
                {
                    saw_msgs_changed = true;
                    break;
                }
            }
            _ => break,
        }
    }
    assert!(
        saw_msgs_changed,
        "no MsgsChanged event arrived after setting a draft"
    );

    // The chat list must contain the chat we just created, through the
    // same two calls DeltaChatCore::fetch_chat_list uses.
    let entries: Vec<u32> = client
        .call(
            "get_chatlist_entries",
            (
                account_id,
                Option::<u32>::None,
                Option::<String>::None,
                Option::<u32>::None,
            ),
        )
        .await
        .expect("get_chatlist_entries");
    assert!(
        entries.contains(&chat_id),
        "chat {chat_id} missing from chat list {entries:?}"
    );
    let items: std::collections::HashMap<u32, Value> = client
        .call("get_chatlist_items_by_entries", (account_id, vec![chat_id]))
        .await
        .expect("get_chatlist_items_by_entries");
    let item = &items[&chat_id];
    assert_eq!(
        item.get("kind").and_then(Value::as_str),
        Some("ChatListItem")
    );
    assert_eq!(item.get("name").and_then(Value::as_str), Some("Testy"));

    // A chat holding a draft says so in its own summary, which is why
    // nothing in the app builds that text: `summaryText1` is the prefix a
    // row already shows in front of `summaryText2`, so a draft reads as
    // "Draft: ..." for free. `summaryStatus` 19 is DC_STATE_OUT_DRAFT, and
    // being under 20 is what keeps a delivery mark off a message that has
    // not been sent.
    assert_eq!(
        item.get("summaryText1").and_then(Value::as_str),
        Some("Draft"),
        "a chat with a draft does not name it in its summary, so the chat \
         list cannot say which chats are holding one: {item:?}"
    );
    assert_eq!(
        item.get("summaryText2").and_then(Value::as_str),
        Some("draft from integration test"),
        "the draft's own text is not what the chat list previews: {item:?}"
    );
    assert_eq!(
        item.get("summaryStatus").and_then(Value::as_u64),
        Some(19),
        "a draft's summary state is not DC_STATE_OUT_DRAFT, so the row \
         cannot tell it from something sent: {item:?}"
    );

    // Reading one back. The core keeps drafts, so this is what survives
    // the app being closed -- and it comes back as a whole message object
    // rather than a string.
    let draft: Value = client
        .call("get_draft", (account_id, chat_id))
        .await
        .expect("get_draft");
    assert_eq!(
        draft.get("text").and_then(Value::as_str),
        Some("draft from integration test"),
        "the draft did not come back: {draft:?}"
    );

    // And clearing one. `misc_set_draft` with no text is not the way --
    // there is a call for it, and it answers null afterwards rather than
    // an empty message.
    client
        .call::<_, Value>("remove_draft", (account_id, chat_id))
        .await
        .expect("remove_draft");
    let gone: Value = client
        .call("get_draft", (account_id, chat_id))
        .await
        .expect("get_draft after remove");
    assert!(gone.is_null(), "a removed draft still reads back: {gone:?}");

    // And the message-list wire shape DeltaChatCore::fetch_messages relies
    // on: `kind` is "message" and the id field is snake_case `msg_id`
    // (this exact spelling was a bug once; keep it pinned by a test that
    // talks to the real core). A draft is not a message-list entry, so
    // just assert the call works and any entries present have the shape.
    let msg_items: Vec<Value> = client
        .call(
            "get_message_list_items",
            (account_id, chat_id, false, false),
        )
        .await
        .expect("get_message_list_items");
    for item in &msg_items {
        let kind = item.get("kind").and_then(Value::as_str);
        assert!(
            kind == Some("message") || kind == Some("dayMarker"),
            "unexpected message list item kind: {item:?}"
        );
        if kind == Some("message") {
            assert!(
                item.get("msg_id").is_some(),
                "expected snake_case msg_id in {item:?}"
            );
        }
    }

    // Account listing, as DeltaChatCore::refresh_accounts consumes it:
    // tagged `kind` with verbatim variant names ("Configured"/
    // "Unconfigured") and camelCase fields. Our never-configured test
    // account must show up as Unconfigured.
    let accounts: Vec<Value> = client
        .call_unit("get_all_accounts")
        .await
        .expect("get_all_accounts");
    let ours = accounts
        .iter()
        .find(|a| a.get("id").and_then(Value::as_u64) == Some(u64::from(account_id)))
        .expect("our account missing from get_all_accounts");
    assert_eq!(
        ours.get("kind").and_then(Value::as_str),
        Some("Unconfigured"),
        "unexpected account shape: {ours:?}"
    );

    // QR classification, as DeltaChatCore::check_qr consumes it. A
    // DCACCOUNT: code is parsed purely locally (no network): expect
    // kind "account" (camelCase variant tag) with snake_case fields.
    let qr: Value = client
        .call(
            "check_qr",
            (account_id, "DCACCOUNT:https://nine.testrun.org/new"),
        )
        .await
        .expect("check_qr");
    assert_eq!(
        qr.get("kind").and_then(Value::as_str),
        Some("account"),
        "unexpected qr shape: {qr:?}"
    );
    assert_eq!(
        qr.get("domain").and_then(Value::as_str),
        Some("nine.testrun.org"),
        "unexpected qr shape: {qr:?}"
    );

    // Encryption flags, as the chat list / message models consume them.
    // The email-address contact chat above is an unencrypted
    // "address-contact" chat; a freshly created group is encrypted.
    assert_eq!(
        items[&chat_id].get("isEncrypted").and_then(Value::as_bool),
        Some(false),
        "plain-email chat should be unencrypted: {:?}",
        items[&chat_id]
    );
    let group_chat_id: u32 = client
        .call("create_group_chat", (account_id, "Test Group", false))
        .await
        .expect("create_group_chat");
    let group_items: std::collections::HashMap<u32, Value> = client
        .call(
            "get_chatlist_items_by_entries",
            (account_id, vec![group_chat_id]),
        )
        .await
        .expect("get_chatlist_items_by_entries for group");
    assert_eq!(
        group_items[&group_chat_id]
            .get("isEncrypted")
            .and_then(Value::as_bool),
        Some(true),
        "fresh group chat should be encrypted: {:?}",
        group_items[&group_chat_id]
    );

    // A group after it is made, as GroupInfo reads it. The members are
    // ids, and the contacts behind them come from a second call keyed by
    // id -- as strings, JSON having no other kind of key.
    let full: Value = client
        .call("get_full_chat_by_id", (account_id, group_chat_id))
        .await
        .expect("get_full_chat_by_id");
    assert_eq!(
        full.get("chatType").and_then(Value::as_str),
        Some("Group"),
        "unexpected chat shape: {full:?}"
    );
    let member_ids: Vec<u32> =
        serde_json::from_value(full["contactIds"].clone()).expect("contactIds is a list of ids");
    assert_eq!(
        member_ids,
        vec![1],
        "a fresh group holds only the account itself (DC_CONTACT_ID_SELF): {full:?}"
    );
    for field in ["name", "profileImage", "color", "selfInGroup", "canSend"] {
        assert!(
            full.get(field).is_some(),
            "the chat lost the {field} field: {full:?}"
        );
    }
    assert_eq!(
        full.get("selfInGroup").and_then(Value::as_bool),
        Some(true),
        "the account is not in the group it just made: {full:?}"
    );
    let members: std::collections::HashMap<u32, Value> = client
        .call("get_contacts_by_ids", (account_id, member_ids))
        .await
        .expect("get_contacts_by_ids");
    let me = &members[&1];
    assert!(
        me.get("displayName").is_some() && me.get("address").is_some(),
        "the account's own contact is not shaped like a contact: {me:?}"
    );
    // The three edits, each its own call. An address contact cannot be
    // added to an encrypted group, which is why the shim reports a
    // refused member rather than failing the whole add.
    client
        .call::<_, ()>(
            "set_chat_name",
            (account_id, group_chat_id, "Renamed Group"),
        )
        .await
        .expect("set_chat_name");
    let renamed: Value = client
        .call("get_full_chat_by_id", (account_id, group_chat_id))
        .await
        .expect("get_full_chat_by_id after rename");
    assert_eq!(
        renamed.get("name").and_then(Value::as_str),
        Some("Renamed Group"),
        "the rename did not take: {renamed:?}"
    );
    assert!(
        client
            .call::<_, ()>("set_chat_name", (account_id, group_chat_id, ""))
            .await
            .is_err(),
        "the core accepted an empty group name"
    );
    assert!(
        client
            .call::<_, ()>(
                "add_contact_to_chat",
                (account_id, group_chat_id, contact_id)
            )
            .await
            .is_err(),
        "an address contact went into an encrypted group; the shim's \
         refused-member reporting is built on this being refused"
    );
    // Removing someone who is not in the group is not an error, and null
    // is how a picture is cleared -- of a group that never had one too.
    client
        .call::<_, ()>(
            "remove_contact_from_chat",
            (account_id, group_chat_id, contact_id),
        )
        .await
        .expect("remove_contact_from_chat of a non-member");
    client
        .call::<_, ()>(
            "set_chat_profile_image",
            (account_id, group_chat_id, Option::<String>::None),
        )
        .await
        .expect("set_chat_profile_image with null");

    // Clearing the unread badge, as DeltaChatCore::open_chat does after
    // fetching messages. Pins the method name/params against the real
    // core (its MsgsNoticed side effect only fires when something was
    // actually fresh, so no event assertion here).
    client
        .call::<_, ()>("marknoticed_chat", (account_id, chat_id))
        .await
        .expect("marknoticed_chat");

    // The message object, as the conversation view consumes it. Saved
    // Messages is the one chat that sends offline, so it is where a real
    // message can be made to look at.
    // A second account, marked configured locally so the core will accept
    // a send. Nothing leaves the machine: Saved Messages has no recipient.
    let sender_id: u32 = client
        .call_unit("add_account")
        .await
        .expect("add_account for the message probe");
    for (key, value) in [
        ("configured_addr", "self@example.invalid"),
        ("displayname", "Testy"),
        ("configured", "1"),
    ] {
        client
            .call::<_, ()>("set_config", (sender_id, key, value))
            .await
            .expect("set_config");
    }
    let saved: u32 = client
        .call("create_chat_by_contact_id", (sender_id, 1))
        .await
        .expect("create_chat_by_contact_id for self");
    let (first, _): (u32, Value) = client
        .call(
            "misc_send_msg",
            (
                sender_id,
                saved,
                Some("hello there"),
                Option::<String>::None,
                Option::<String>::None,
                Option::<(f64, f64)>::None,
                Option::<u32>::None,
            ),
        )
        .await
        .expect("misc_send_msg");
    let attachment = std::env::temp_dir().join("postivene-real-server-note.txt");
    std::fs::write(&attachment, b"hi").expect("write attachment");
    let (second, _): (u32, Value) = client
        .call(
            "misc_send_msg",
            (
                sender_id,
                saved,
                Some("a reply"),
                Some(attachment.to_string_lossy().into_owned()),
                Some("note.txt".to_string()),
                Option::<(f64, f64)>::None,
                // Quoting the first message.
                Some(first),
            ),
        )
        .await
        .expect("misc_send_msg with a quote and a file");
    let messages: std::collections::HashMap<u32, Value> = client
        .call("get_messages", (sender_id, vec![first, second]))
        .await
        .expect("get_messages");
    let reply = &messages[&second];
    // Every field the message rows are built from.
    assert_eq!(
        reply.get("viewType").and_then(Value::as_str),
        Some("File"),
        "unexpected message shape: {reply:?}"
    );
    assert_eq!(
        reply.get("fileName").and_then(Value::as_str),
        Some("note.txt"),
        "unexpected message shape: {reply:?}"
    );
    for field in [
        "file",
        "isInfo",
        "timestamp",
        "state",
        "showPadlock",
        "text",
    ] {
        assert!(
            reply.get(field).is_some(),
            "message lost the {field} field: {reply:?}"
        );
    }
    assert!(
        reply.pointer("/sender/displayName").is_some() && reply.pointer("/sender/color").is_some(),
        "message says nothing about its sender: {reply:?}"
    );
    assert_eq!(
        reply.pointer("/quote/text").and_then(Value::as_str),
        Some("hello there"),
        "the quoted message did not come back with the quote: {reply:?}"
    );
    assert!(
        reply.pointer("/quote/authorDisplayName").is_some(),
        "the quote says nothing about its author: {reply:?}"
    );

    // The core decides the view type from the file, and the conversation
    // renders a picture inline on the strength of that: nothing in the app
    // classifies an attachment, and nothing should start.
    let picture = std::env::temp_dir().join("postivene-real-server-dot.png");
    // The smallest valid PNG: an 1x1 image, so the core has real pixels to
    // read rather than a name to guess from.
    std::fs::write(&picture, ONE_PIXEL_PNG).expect("write picture");
    let (third, _): (u32, Value) = client
        .call(
            "misc_send_msg",
            (
                sender_id,
                saved,
                Option::<String>::None,
                Some(picture.to_string_lossy().into_owned()),
                Some("dot.png".to_string()),
                Option::<(f64, f64)>::None,
                Option::<u32>::None,
            ),
        )
        .await
        .expect("misc_send_msg with a picture");
    let messages: std::collections::HashMap<u32, Value> = client
        .call("get_messages", (sender_id, vec![third]))
        .await
        .expect("get_messages");
    let sent_picture = &messages[&third];
    assert_eq!(
        sent_picture.get("viewType").and_then(Value::as_str),
        Some("Image"),
        "the core did not classify a PNG as an image, so the conversation \
         would show it as a paperclip: {sent_picture:?}"
    );
    for field in ["dimensionsWidth", "dimensionsHeight"] {
        assert_eq!(
            sent_picture.get(field).and_then(Value::as_i64),
            Some(1),
            "the core reported no {field}, which is what the delegate sizes \
             the picture from: {sent_picture:?}"
        );
    }

    // What the core does NOT fill in, which is as much of the contract as
    // what it does: a GIF gets no dimensions and a sound file gets no
    // duration, so the conversation must not size or label anything from
    // them without checking. This is the whole reason AttachmentPreview
    // falls back to the loaded item's own proportions and lets the audio
    // player report its own length.
    let animation = std::env::temp_dir().join("postivene-real-server-dot.gif");
    std::fs::write(&animation, ONE_PIXEL_GIF).expect("write gif");
    let sent_gif = send_file(&client, sender_id, saved, &animation, "dot.gif").await;
    assert_eq!(
        sent_gif.get("viewType").and_then(Value::as_str),
        Some("Gif"),
        "the core did not classify a GIF as an animation: {sent_gif:?}"
    );
    for field in ["dimensionsWidth", "dimensionsHeight"] {
        assert_eq!(
            sent_gif.get(field).and_then(Value::as_i64),
            Some(0),
            "the core now reports {field} for a GIF. That is an improvement, \
             but AttachmentPreview's fallback was written because it did \
             not: check the fallback still has a case, then relax this: \
             {sent_gif:?}"
        );
    }

    let tone = std::env::temp_dir().join("postivene-real-server-tone.wav");
    std::fs::write(&tone, one_second_wav()).expect("write wav");
    let sent_tone = send_file(&client, sender_id, saved, &tone, "tone.wav").await;
    assert_eq!(
        sent_tone.get("viewType").and_then(Value::as_str),
        Some("Audio"),
        "the core did not classify a WAV as audio: {sent_tone:?}"
    );
    assert_eq!(
        sent_tone.get("fileMime").and_then(Value::as_str),
        Some("audio/wav"),
        "the core reported no MIME type, which the video thumbnailer needs \
         and the file row names itself by: {sent_tone:?}"
    );
    assert_eq!(
        sent_tone.get("fileBytes").and_then(Value::as_u64),
        Some(8044),
        "the core reported the wrong size for a file it just copied: {sent_tone:?}"
    );
    assert_eq!(
        sent_tone.get("duration").and_then(Value::as_i64),
        Some(0),
        "the core now reports a duration for a file it did not before. It \
         carries whatever the sender put in the message rather than probing, \
         which is why nothing renders this field: {sent_tone:?}"
    );

    // A shared contact. The core parses the card and hands back the pieces
    // a contact row is built from, so nothing here reads vCard syntax.
    let ada: u32 = client
        .call(
            "create_contact",
            (sender_id, "ada@example.org", "Ada Lovelace"),
        )
        .await
        .expect("create_contact");
    let card: String = client
        .call("make_vcard", (sender_id, vec![ada]))
        .await
        .expect("make_vcard");
    let card_path = std::env::temp_dir().join("postivene-real-server-ada.vcf");
    std::fs::write(&card_path, &card).expect("write vcard");
    let sent_card = send_file(&client, sender_id, saved, &card_path, "ada.vcf").await;
    assert_eq!(
        sent_card.get("viewType").and_then(Value::as_str),
        Some("Vcard"),
        "the core did not classify a .vcf as a contact: {sent_card:?}"
    );
    for field in ["displayName", "addr", "color"] {
        assert!(
            sent_card
                .pointer(&format!("/vcardContact/{field}"))
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty()),
            "the parsed contact has no {field}, which the contact row is \
             built from: {sent_card:?}"
        );
    }

    // Chat kind, which decides whether a message names its sender.
    let group: u32 = client
        .call("create_group_chat", (sender_id, "Shape Group", false))
        .await
        .expect("create_group_chat");
    let info: Value = client
        .call("get_basic_chat_info", (sender_id, group))
        .await
        .expect("get_basic_chat_info");
    assert_eq!(
        info.get("chatType").and_then(Value::as_str),
        Some("Group"),
        "unexpected chat info shape: {info:?}"
    );

    // The chat-list row, as the list consumes it. Every field a row shows
    // has to be there and spelled the way the core spells it.
    let saved_items: std::collections::HashMap<u32, Value> = client
        .call("get_chatlist_items_by_entries", (sender_id, vec![saved]))
        .await
        .expect("get_chatlist_items_by_entries for the saved chat");
    let row = &saved_items[&saved];
    for field in [
        "freshMessageCounter",
        "lastUpdated",
        "summaryText1",
        "summaryText2",
        "summaryStatus",
        "isPinned",
        "isMuted",
        "isContactRequest",
        "color",
        "avatarPath",
    ] {
        assert!(
            row.get(field).is_some(),
            "the chat list row lost the {field} field: {row:?}"
        );
    }

    // What the row's context menu does. Visibility is one method with the
    // core's own variant names, and muting takes a tagged duration.
    for visibility in ["Pinned", "Archived", "Normal"] {
        client
            .call::<_, ()>("set_chat_visibility", (sender_id, group, visibility))
            .await
            .unwrap_or_else(|err| panic!("set_chat_visibility {visibility}: {err}"));
    }
    for kind in ["Forever", "NotMuted"] {
        client
            .call::<_, ()>(
                "set_chat_mute_duration",
                (sender_id, group, serde_json::json!({"kind": kind})),
            )
            .await
            .unwrap_or_else(|err| panic!("set_chat_mute_duration {kind}: {err}"));
    }
    client
        .call::<_, ()>("delete_chat", (sender_id, group))
        .await
        .expect("delete_chat");

    // What a message's context menu does. Both take a list, not a single
    // id, and a reply is `misc_send_msg` with the quoted message last --
    // pinned by the quote assertion above.
    client
        .call::<_, ()>("resend_messages", (sender_id, vec![first]))
        .await
        .expect("resend_messages");
    client
        .call::<_, ()>("delete_messages", (sender_id, vec![second]))
        .await
        .expect("delete_messages");

    // Read receipts, as ChatMessages sends them when a chat is opened.
    client
        .call::<_, ()>("markseen_msgs", (sender_id, vec![first]))
        .await
        .expect("markseen_msgs");

    // What the next round of UI work depends on, pinned here because a
    // wrong method name or config key fails only on a device -- the fake
    // core answers to whatever it is asked.

    // Forwarding. `isForwarded` is what marks a forwarded copy for the
    // sender as well as the recipient, and it is false on the original:
    // an assertion on the copy alone would pass against a field that was
    // simply always true.
    let (original, _): (u32, Value) = client
        .call(
            "misc_send_msg",
            (
                sender_id,
                saved,
                Some("to forward"),
                Option::<String>::None,
                Option::<String>::None,
                Option::<(f64, f64)>::None,
                Option::<u32>::None,
            ),
        )
        .await
        .expect("misc_send_msg for the forwarding probe");
    client
        .call::<_, ()>("forward_messages", (sender_id, vec![original], saved))
        .await
        .expect("forward_messages");
    let listed: Vec<Value> = client
        .call("get_message_list_items", (sender_id, saved, false, false))
        .await
        .expect("get_message_list_items after forwarding");
    let ids: Vec<u32> = listed
        .iter()
        .filter_map(|item| item.get("msg_id").and_then(Value::as_u64))
        .filter_map(|id| u32::try_from(id).ok())
        .collect();
    let after: std::collections::HashMap<u32, Value> = client
        .call("get_messages", (sender_id, ids))
        .await
        .expect("get_messages after forwarding");
    let forwarded_flags: Vec<bool> = after
        .values()
        .filter_map(|message| message.get("isForwarded").and_then(Value::as_bool))
        .collect();
    assert!(
        forwarded_flags.iter().any(|flag| *flag),
        "no message reports isForwarded after forward_messages: {after:?}"
    );
    assert!(
        forwarded_flags.iter().any(|flag| !*flag),
        "every message reports isForwarded, so the field marks nothing: {after:?}"
    );

    // Searching. Three arguments, the last being an optional chat to
    // search within -- passing two is rejected outright.
    let _: Vec<u32> = client
        .call(
            "search_messages",
            (sender_id, "forward", Option::<u32>::None),
        )
        .await
        .expect("search_messages takes (account, query, chat_id option)");

    // The profile fields a settings page edits. `displayname` is already
    // pinned above; these are the two it does not cover.
    client
        .call::<_, ()>("set_config", (sender_id, "selfstatus", "probing"))
        .await
        .expect("set_config selfstatus");
    let status: Option<String> = client
        .call("get_config", (sender_id, "selfstatus"))
        .await
        .expect("get_config selfstatus");
    assert_eq!(
        status.as_deref(),
        Some("probing"),
        "selfstatus did not stick"
    );

    // `selfavatar` is a *path to an image the core copies into its blob
    // directory*, not a value: an empty string is rejected outright with
    // "Copying new blobfile failed". A settings page has to hand it a
    // real file, and clear it with null rather than "".
    let avatar = std::env::temp_dir().join("postivene-real-server-avatar.png");
    // The smallest valid PNG; the core rejects what it cannot decode.
    std::fs::write(
        &avatar,
        [
            137u8, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
            8, 6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 11, 73, 68, 65, 84, 120, 156, 99, 96, 0, 2,
            0, 0, 5, 0, 1, 122, 94, 171, 63, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
        ],
    )
    .expect("write probe avatar");
    client
        .call::<_, ()>(
            "set_config",
            (
                sender_id,
                "selfavatar",
                Some(avatar.to_string_lossy().into_owned()),
            ),
        )
        .await
        .expect("set_config selfavatar with a real image path");
    let stored: Option<String> = client
        .call("get_config", (sender_id, "selfavatar"))
        .await
        .expect("get_config selfavatar");
    assert!(
        stored.as_deref().is_some_and(|path| !path.is_empty()),
        "selfavatar read back empty after being set to an image: {stored:?}"
    );
    // Null clears it. This is the shape a "remove picture" action needs.
    client
        .call::<_, ()>(
            "set_config",
            (sender_id, "selfavatar", Option::<String>::None),
        )
        .await
        .expect("set_config selfavatar null clears it");
    let _ = std::fs::remove_file(&avatar);

    let _ = std::fs::remove_file(&attachment);
    handle.stop();
    client.shutdown().await.expect("shutdown");
    let _ = std::fs::remove_dir_all(&accounts_dir);
}

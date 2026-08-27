//! The per-chat message model: what it asks the core for, and when.
//!
//! Two instances are driven at once, which is the case the old single shared
//! model got wrong -- opening a second conversation reset the first.

// Qt harness: needs `unsafe` for `env::set_var` before Qt starts
// (`unused_unsafe` because it is only unsafe from edition 2024 on),
// `borrow_as_ptr` for the engine pointer, and `single_shot` with
// whole-second Durations.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::path::PathBuf;
use std::time::Duration;

use postivene_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

/// Two models over different chats, plus a way to read their row counts and
/// to send. Written in the Qt 5.6 dialect with the shim's `snake_case`
/// names.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Postivene 1.0
    Item {
        property string lastError: ''
        ChatMessages { id: first; account_id: 1; chat_id: 1
                       onError: lastError = message }
        ChatMessages { id: second; account_id: 1; chat_id: 2
                       onError: lastError = message }
        Connections {
            target: core
            onCore_event: {
                first.handle_event(context_id, kind, payload_json)
                second.handle_event(context_id, kind, payload_json)
            }
        }
        function counts() { return first.count + '/' + second.count }
        function send(text) { first.send(text) }
        function error() { return lastError }
    }
";

fn journal_methods(journal: &PathBuf) -> Vec<(String, Value)> {
    std::fs::read_to_string(journal)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .map(|call| {
            (
                call.get("method")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                call.get("params").cloned().unwrap_or(Value::Null),
            )
        })
        .collect()
}

#[test]
fn each_chat_has_its_own_model_and_loads_in_one_batch() {
    let temp = std::env::temp_dir().join(format!("postivene-chat-model-{}", std::process::id()));
    let journal = temp.join("journal.jsonl");
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_FAKE_JOURNAL", &journal);
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
    }

    postivene_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut counts_after_load = String::new();
    let counts_ptr: *mut String = std::ptr::addr_of_mut!(counts_after_load);

    // The models load as soon as the QML sets their ids, which needs the
    // core to be up: load the probe a tick after start.
    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        let value = (*engine_ptr).invoke_method("counts".into(), &[]);
        *counts_ptr = QString::from_qvariant(value)
            .map(|text| text.to_string())
            .unwrap_or_default();
        (*engine_ptr).invoke_method(
            "send".into(),
            &[QVariant::from(QString::from("hello there"))],
        );
    });

    let mut counts_after_send = String::new();
    let sent_ptr: *mut String = std::ptr::addr_of_mut!(counts_after_send);
    single_shot(Duration::from_secs(6), move || unsafe {
        let value = (*engine_ptr).invoke_method("counts".into(), &[]);
        *sent_ptr = QString::from_qvariant(value)
            .map(|text| text.to_string())
            .unwrap_or_default();
        (*engine_ptr).quit();
    });

    engine.exec();

    let calls = journal_methods(&journal);
    let names: Vec<&str> = calls.iter().map(|(name, _)| name.as_str()).collect();

    // The two chats are seeded with two messages and one. Separate models,
    // separate contents.
    assert_eq!(
        counts_after_load, "2/1",
        "the two models do not hold their own chats. Calls were: {names:?}"
    );

    // One call for the whole chat, not one per message.
    assert!(
        !names.contains(&"get_message"),
        "messages were fetched one at a time: {names:?}"
    );
    let batches: Vec<usize> = calls
        .iter()
        .filter(|(name, _)| name == "get_messages")
        .map(|(_, params)| {
            params
                .pointer("/1")
                .and_then(Value::as_array)
                .map_or(0, Vec::len)
        })
        .collect();
    // One load per model. The two race, so which lands first is not fixed --
    // assert on the set, not the order.
    assert_eq!(batches.len(), 2, "expected one batch per model: {names:?}");
    assert!(
        batches.contains(&2),
        "the two-message chat did not come back in one call: {batches:?}"
    );

    // Sending appends to the sending model only, and the IncomingMsg the
    // core answers with must not duplicate the row.
    assert_eq!(
        counts_after_send, "3/1",
        "a sent message did not land exactly once in its own chat"
    );

    // The event path fetched only what it did not have -- which, after a
    // send, is nothing: the row is already in the model. So the event costs
    // one id-list call and no message fetch at all.
    let after_send: Vec<&str> = names
        .iter()
        .skip_while(|name| **name != "misc_send_msg")
        .skip(1)
        .copied()
        .collect();
    assert!(
        after_send.contains(&"get_message_list_items"),
        "the event did not reach the model: {names:?}"
    );
    assert!(
        !after_send.contains(&"get_messages"),
        "the event refetched messages the model already had: {names:?}"
    );
}

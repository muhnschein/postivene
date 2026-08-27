//! Starting a conversation: the three ways in.
//!
//! Until this existed nothing in the app created a chat, so these assertions
//! are about calls that had no caller before.

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

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Postivene 1.0
    Item {
        property string opened: ''
        property string lastError: ''
        property bool started: false
        ContactList {
            id: contacts
            account_id: 1
            onChat_ready: opened = opened + chat_id + ','
            onError: lastError = message
            // Drive the scenario off the data arriving rather than off a
            // clock: under load the core takes longer to come up, and a
            // fixed tick then reads an empty list.
            onRows_changed: {
                if (!started && rows.count > 1) {
                    started = true
                    contacts.open_chat_with(rows.itemAt(0).cid)
                    contacts.start_chat_with_address('new@example.org', 'New Person')
                    contacts.create_group('Team', [rows.itemAt(0).cid, rows.itemAt(1).cid])
                }
            }
        }
        Connections {
            target: core
            onStatus_changed: {
                if (core.status === 'ready') { contacts.reload() }
            }
        }
        Repeater {
            id: rows
            model: contacts.rows
            Item {
                property int cid: model.contact_id
                property string addr: model.address
            }
        }
        function names() {
            var out = ''
            for (var i = 0; i < rows.count; i++) { out += rows.itemAt(i).addr + ',' }
            return out
        }
        function firstId() { return rows.count > 0 ? rows.itemAt(0).cid : 0 }
        function report() { return opened + '#' + lastError }
    }
";

fn calls(journal: &PathBuf) -> Vec<(String, Value)> {
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
fn a_chat_can_be_started_three_ways() {
    let temp = std::env::temp_dir().join(format!("postivene-contacts-{}", std::process::id()));
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
    let mut listed = String::new();
    let listed_ptr: *mut String = std::ptr::addr_of_mut!(listed);
    let mut report = String::new();
    let report_ptr: *mut String = std::ptr::addr_of_mut!(report);

    // SAFETY for every block: these fire only while `exec()` runs on this
    // thread, and `engine` outlives it.
    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        let value = (*engine_ptr).invoke_method("names".into(), &[]);
        *listed_ptr = QString::from_qvariant(value)
            .map(|text| text.to_string())
            .unwrap_or_default();
        let value = (*engine_ptr).invoke_method("report".into(), &[]);
        *report_ptr = QString::from_qvariant(value)
            .map(|text| text.to_string())
            .unwrap_or_default();
        (*engine_ptr).quit();
    });

    engine.exec();

    let calls = calls(&journal);
    let names: Vec<&str> = calls.iter().map(|(name, _)| name.as_str()).collect();

    assert_eq!(
        listed, "ada@example.org,grace@example.org,",
        "the contact list did not load. Calls were: {names:?}"
    );

    let (opened, error) = report.split_once('#').unwrap_or(("", ""));
    assert_eq!(error, "", "a chat could not be started: {error}");
    assert_eq!(
        opened.split(',').filter(|part| !part.is_empty()).count(),
        3,
        "expected a chat from each of the three ways in, got {opened:?}"
    );

    // Contact tapped: straight to a chat, no contact created.
    assert!(
        names.contains(&"create_chat_by_contact_id"),
        "tapping a contact did not open a chat: {names:?}"
    );

    // Address typed: a contact first, then the chat.
    let created = calls
        .iter()
        .find(|(name, _)| name == "create_contact")
        .expect("an address must create a contact");
    assert_eq!(
        created.1.pointer("/1").and_then(Value::as_str),
        Some("new@example.org")
    );
    assert_eq!(
        created.1.pointer("/2").and_then(Value::as_str),
        Some("New Person"),
        "the name typed alongside the address was dropped"
    );

    // Group: created encrypted, then both members added.
    let group = calls
        .iter()
        .find(|(name, _)| name == "create_group_chat")
        .expect("the group must be created");
    assert_eq!(group.1.pointer("/1").and_then(Value::as_str), Some("Team"));
    assert_eq!(
        group.1.pointer("/2").and_then(Value::as_bool),
        Some(true),
        "the group was created unencrypted"
    );
    assert_eq!(
        names
            .iter()
            .filter(|name| **name == "add_contact_to_chat")
            .count(),
        2,
        "both members should have been added: {names:?}"
    );
}

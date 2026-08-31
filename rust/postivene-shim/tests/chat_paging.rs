//! A long chat opens on its newest messages and gives up the rest a page
//! at a time.
//!
//! The ids are cheap and the messages are not, so the model holds every id
//! and fetches a window of messages at the end of the list. What is worth
//! pinning is that the window is a window -- an arrival must extend it
//! rather than slide it, or the oldest loaded row drops off the front and
//! every incoming message turns into a full reload.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use postivene_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

/// More than two pages, so the first step back is not also the last.
const MESSAGES: u32 = 130;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Postivene 1.0
    Item {
        property int olderRuns: 0
        property int olderRows: 0
        property string revealedAt: ''
        ChatMessages { id: chat; account_id: 1; chat_id: 1 }
        Repeater {
            id: rows
            model: chat.rows
            Item { property int mid: model.message_id }
        }
        Connections {
            target: chat
            onOlder_loaded: { olderRuns += 1; olderRows += count }
            onRevealed: revealedAt = message_id + '@' + row
        }
        Connections {
            target: core
            onCore_event: chat.handle_event(context_id, kind, payload_json)
        }
        // The oldest and newest loaded row, which is what says whether the
        // window moved or grew.
        function edges() {
            if (rows.count === 0) { return 'empty' }
            return rows.itemAt(0).mid + '..' + rows.itemAt(rows.count - 1).mid
        }
        function count() { return '' + chat.count }
        function hasOlder() { return '' + chat.has_older }
        function older() { chat.load_older(); return 'ok' }
        function reveal(id) { chat.reveal(id); return 'ok' }
        function revealed() { return revealedAt }
        function olderStats() { return olderRuns + '/' + olderRows }
        // An arrival, which must extend the window rather than slide it.
        function arrive() { chat.send('from the other end'); return 'ok' }
    }
";

#[test]
fn a_long_chat_opens_on_its_newest_page_and_walks_back() {
    let temp = std::env::temp_dir().join(format!("postivene-paging-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_FAKE_JOURNAL", &journal);
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("POSTIVENE_FAKE_LONG_CHAT", MESSAGES.to_string());
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
    let mut steps: Vec<(&str, String)> = Vec::new();
    let steps_ptr: *mut Vec<(&str, String)> = std::ptr::addr_of_mut!(steps);

    macro_rules! call {
        ($name:expr $(, $arg:expr)*) => {{
            let result = (*engine_ptr).invoke_method(
                $name.into(),
                &[$(QVariant::from($arg)),*],
            );
            QString::from_qvariant(result)
                .map(|value| value.to_string())
                .unwrap_or_default()
        }};
    }

    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push(("opened", call!("count")));
        (*steps_ptr).push(("opened-edges", call!("edges")));
        (*steps_ptr).push(("opened-has-older", call!("hasOlder")));
        (*steps_ptr).push(("step-back", call!("older")));
    });
    single_shot(Duration::from_secs(5), move || unsafe {
        (*steps_ptr).push(("stepped", call!("count")));
        (*steps_ptr).push(("stepped-edges", call!("edges")));
        // An arrival on top of a stepped-back window.
        (*steps_ptr).push(("arrive", call!("arrive")));
    });
    single_shot(Duration::from_secs(7), move || unsafe {
        (*steps_ptr).push(("arrived", call!("count")));
        (*steps_ptr).push(("arrived-edges", call!("edges")));
        // A message older than everything loaded, as a search result would
        // name.
        (*steps_ptr).push(("reveal", call!("reveal", 3)));
    });
    single_shot(Duration::from_secs(9), move || unsafe {
        (*steps_ptr).push(("revealed", call!("revealed")));
        (*steps_ptr).push(("revealed-edges", call!("edges")));
        (*steps_ptr).push(("older-stats", call!("olderStats")));
        (*steps_ptr).push(("has-older-now", call!("hasOlder")));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps, &journal);
}

/// What the run has to show for itself, out of the test body.
#[allow(clippy::too_many_lines)]
fn assert_outcome(steps: &[(&str, String)], journal: &std::path::Path) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let number = |label: &str| value(label).parse::<u32>().unwrap_or(0);
    let context = format!("steps: {steps:?}");

    // A page, not the chat.
    assert_eq!(
        number("opened"),
        50,
        "a {MESSAGES}-message chat did not open on one page of messages. {context}"
    );
    assert_eq!(
        value("opened-edges"),
        "81..130",
        "the page a chat opens on is not the newest one. {context}"
    );
    assert_eq!(
        value("opened-has-older"),
        "true",
        "the model does not know it is holding part of a chat, so nothing \
         can offer the rest. {context}"
    );

    // One step back is one page more, on the old end.
    assert_eq!(
        number("stepped"),
        100,
        "a step back did not bring in a page. {context}"
    );
    assert_eq!(
        value("stepped-edges"),
        "31..130",
        "a step back moved the window instead of growing it. {context}"
    );

    // The arrival is the case a count-based window gets wrong: the newest
    // ids shift by one, and the oldest loaded row falls off the front.
    assert_eq!(
        number("arrived"),
        101,
        "a message arriving did not extend the window. {context}"
    );
    assert!(
        value("arrived-edges").starts_with("31.."),
        "a message arriving slid the window forward and dropped the oldest \
         loaded row, which reads as a deletion and costs a full reload: \
         {:?}. {context}",
        value("arrived-edges")
    );

    // A search result older than anything loaded.
    assert_eq!(
        value("revealed"),
        "3@2",
        "revealing an unloaded message did not report where it landed. \
         {context}"
    );
    assert!(
        value("revealed-edges").starts_with("1.."),
        "revealing message 3 did not bring in the messages above it: {:?}. \
         {context}",
        value("revealed-edges")
    );
    assert_eq!(
        value("has-older-now"),
        "false",
        "the whole chat is loaded and the model still offers more. {context}"
    );

    // Two steps back in total, and each one fetched: the reveal must not
    // have re-fetched what was already there.
    assert_eq!(
        value("older-stats"),
        "2/80",
        "the steps back did not bring in what they should have -- 50 for \
         the explicit one, then the 30 above message 3 that were still \
         missing. {context}"
    );

    // The point of all of it: never the whole chat in one go.
    let biggest = common::calls(journal)
        .into_iter()
        .filter(|(method, _)| method == "get_messages")
        .filter_map(|(_, params)| {
            params
                .as_array()
                .and_then(|array| array.get(1))
                .and_then(Value::as_array)
                .map(Vec::len)
        })
        .max()
        .unwrap_or(0);
    assert!(
        biggest <= 50,
        "one fetch asked the core for {biggest} messages at once; the whole \
         point is that no single fetch is the size of the chat. {context}"
    );
}

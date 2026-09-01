//! A long chat opens on its newest messages and gives up the rest a page
//! at a time.
//!
//! The ids are cheap and the messages are not, so the model holds every id
//! and fetches a window of messages out of that list. What is worth pinning
//! is that the window is a window -- an arrival must extend it rather than
//! slide it, or the oldest loaded row drops off the front and every
//! incoming message turns into a full reload.
//!
//! The window has two ends. It starts at the newest message and takes in
//! arrivals, which is the ordinary case; going to somewhere it does not
//! reach -- the beginning of a long chat, a search result from last March
//! -- moves it off the end instead of growing it back to today. A window
//! that has been moved off the end must stop taking in arrivals, or a
//! reader who went looking for something old gets dragged back to now one
//! message at a time.

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
        property int newerRuns: 0
        property string revealedAt: ''
        ChatMessages { id: chat; account_id: 1; chat_id: 1 }
        // A second model on the same chat, only to put a message into it
        // from outside. A send through `chat` is its own reply and its own
        // row; what the window has to survive is somebody else's.
        ChatMessages { id: other; account_id: 1; chat_id: 1 }
        Repeater {
            id: rows
            model: chat.rows
            Item { property int mid: model.message_id }
        }
        Connections {
            target: chat
            onOlder_loaded: { olderRuns += 1; olderRows += count }
            onNewer_loaded: newerRuns += 1
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
        function hasNewer() { return '' + chat.has_newer }
        function older() { chat.load_older(); return 'ok' }
        function newer() { chat.load_newer(); return 'ok' }
        function toOldest() { chat.jump_oldest(); return 'ok' }
        function toNewest() { chat.jump_newest(); return 'ok' }
        function reveal(id) { chat.reveal(id); return 'ok' }
        function revealed() { return revealedAt }
        function olderStats() { return olderRuns + '/' + olderRows }
        function newerRunCount() { return '' + newerRuns }
        // An arrival, which must extend the window rather than slide it.
        function arrive() { chat.send('from the other end'); return 'ok' }
        // One that this model did not send, so it reaches it as an event
        // rather than as the reply to its own call.
        function arriveElsewhere() { other.send('from someone else'); return 'ok' }
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
        (*steps_ptr).push(("has-newer-now", call!("hasNewer")));
        // An arrival with the window off the end of the chat. This is the
        // one that matters: it must land in the id list and nowhere near
        // the rows.
        (*steps_ptr).push(("arrive-away", call!("arriveElsewhere")));
    });
    single_shot(Duration::from_secs(11), move || unsafe {
        (*steps_ptr).push(("away-count", call!("count")));
        (*steps_ptr).push(("away-edges", call!("edges")));
        // Forwards, a page at a time, the mirror of the step back.
        (*steps_ptr).push(("step-forward", call!("newer")));
    });
    single_shot(Duration::from_secs(13), move || unsafe {
        (*steps_ptr).push(("forward-edges", call!("edges")));
        (*steps_ptr).push(("forward-has-newer", call!("hasNewer")));
        (*steps_ptr).push(("newer-runs", call!("newerRunCount")));
        (*steps_ptr).push(("to-newest", call!("toNewest")));
    });
    single_shot(Duration::from_secs(15), move || unsafe {
        (*steps_ptr).push(("newest-count", call!("count")));
        (*steps_ptr).push(("newest-edges", call!("edges")));
        (*steps_ptr).push(("newest-has-newer", call!("hasNewer")));
        (*steps_ptr).push(("to-oldest", call!("toOldest")));
    });
    single_shot(Duration::from_secs(17), move || unsafe {
        (*steps_ptr).push(("oldest-edges", call!("edges")));
        (*steps_ptr).push(("oldest-has-older", call!("hasOlder")));
        (*steps_ptr).push(("oldest-has-newer", call!("hasNewer")));
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
        "the window reaches the first message in the chat and the model \
         still offers older ones. {context}"
    );
    assert_eq!(
        value("has-newer-now"),
        "true",
        "the window was moved off the end of the chat and the model does \
         not know it, so nothing can offer the way back. {context}"
    );

    // One step back, and one fetch for it. Reaching message 3 moved the
    // window rather than growing it: walking back to it is what paging was
    // for in the first place, and doing it here would load everything from
    // last March to today.
    assert_eq!(
        value("older-stats"),
        "1/50",
        "the explicit step back did not bring in a page, or reaching \
         message 3 walked back to it a page at a time instead of moving \
         the window there. {context}"
    );

    // The whole reason the window has a far end.
    assert_eq!(
        value("away-count"),
        "50",
        "a message arrived while the reader was reading last March, and it \
         was pulled into the window with them. {context}"
    );
    assert_eq!(
        value("away-edges"),
        "1..50",
        "an arrival moved a window that is nowhere near the end of the \
         chat. {context}"
    );

    // Forwards, a page at a time.
    assert_eq!(
        value("forward-edges"),
        "1..100",
        "a step forward did not bring in the page below. {context}"
    );
    assert_eq!(
        value("newer-runs"),
        "1",
        "the step forward did not say how many rows went in, so nothing \
         can tell the view. {context}"
    );
    assert_eq!(
        value("forward-has-newer"),
        "true",
        "there are messages below the window and the model says there are \
         not. {context}"
    );

    // Back to the end in one go, which is what the jump button needs.
    assert_eq!(
        value("newest-count"),
        "50",
        "going back to the newest messages loaded something other than a \
         page. {context}"
    );
    assert_eq!(
        value("newest-edges"),
        "83..132",
        "going back to the newest messages did not land on them -- 132 \
         being the one that arrived while the reader was away. {context}"
    );
    assert_eq!(
        value("newest-has-newer"),
        "false",
        "the window is back at the end of the chat and the model still \
         thinks there is more below it, so it will not take in arrivals \
         again. {context}"
    );

    // And to the beginning, which is what the top of the list offers.
    assert_eq!(
        value("oldest-edges"),
        "1..50",
        "the beginning of the chat is not where the jump to it landed. \
         {context}"
    );
    assert_eq!(
        value("oldest-has-older"),
        "false",
        "the window is on the first message in the chat and the model \
         offers older ones. {context}"
    );
    assert_eq!(
        value("oldest-has-newer"),
        "true",
        "the window is at the beginning of a 132-message chat and the \
         model does not offer the way back. {context}"
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

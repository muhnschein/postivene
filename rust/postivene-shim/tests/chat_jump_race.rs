//! Going to the beginning of a chat, while the core is saying something.
//!
//! Reported from a device three times running, and the third report is what
//! gave it away: "less predictable than in earlier builds", in a chat with
//! every kind of media in it. Media is what makes a chat noisy -- the core
//! reports on downloads and on state as it works through attachments -- and
//! every one of those events makes the model reconcile its rows against a
//! fresh id list.
//!
//! That reconciliation reads where its window starts and ends *before* it
//! asks the core for anything, and compares against those anchors when the
//! answer comes back. Move the window while it is in flight -- which is
//! exactly what going to the beginning does -- and it wakes up holding the
//! old window's anchors and the new window's rows, agrees with neither, and
//! falls through to reloading the chat. A reload goes to the newest page.
//! So the reader taps for the beginning and lands back at today, or does
//! not, depending on whether an event happened to be in the air.
//!
//! The fetch here is slowed down so the two really do overlap; on a device
//! the overlap is free.

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

mod common;

const MESSAGES: u32 = 117;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Postivene 1.0
    Item {
        ChatMessages { id: chat; account_id: 1; chat_id: 1 }
        Repeater {
            id: rows
            model: chat.rows
            Item { property int mid: model.message_id }
        }
        function edges() {
            if (rows.count === 0) { return 'empty' }
            return rows.itemAt(0).mid + '..' + rows.itemAt(rows.count - 1).mid
        }
        function hasNewer() { return '' + chat.has_newer }
        function toOldest() { chat.jump_oldest(); return 'ok' }
        /// What the core says while it works through a chat's attachments.
        function stir() {
            chat.handle_event(1, 'MsgsChanged', '{}')
            return 'ok'
        }
    }
";

#[test]
fn going_to_the_beginning_survives_the_core_talking_at_the_same_time() {
    let temp = std::env::temp_dir().join(format!("postivene-jump-race-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("POSTIVENE_FAKE_LONG_CHAT", MESSAGES.to_string());
        // Long enough that the jump and the reconciliation are in the air
        // together, which on a device they are without any help.
        std::env::set_var("POSTIVENE_FAKE_FETCH_DELAY_MS", "700");
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
    single_shot(Duration::from_secs(4), move || unsafe {
        (*steps_ptr).push(("opened", call!("edges")));
        (*steps_ptr).push(("jump", call!("toOldest")));
    });
    // While the jump is still fetching: the core saying something, which in
    // a chat full of pictures it is doing all the time.
    single_shot(Duration::from_millis(4050), move || unsafe {
        (*steps_ptr).push(("stir", call!("stir")));
    });
    single_shot(Duration::from_secs(8), move || unsafe {
        (*steps_ptr).push(("landed", call!("edges")));
        (*steps_ptr).push(("has-newer", call!("hasNewer")));
        (*engine_ptr).quit();
    });

    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("opened"),
        "68..117",
        "the chat did not open on its newest page. {context}"
    );
    assert_eq!(
        value("landed"),
        "1..50",
        "going to the beginning of the chat was undone by an event landing \
         while it was in flight: the reconciliation compared the window it \
         read before asking the core against the one that is there now, \
         agreed with neither, and reloaded -- and a reload goes to the \
         newest page. {context}"
    );
    assert_eq!(
        value("has-newer"),
        "true",
        "the window is at the beginning of a {MESSAGES}-message chat and \
         does not offer the way back. {context}"
    );
}

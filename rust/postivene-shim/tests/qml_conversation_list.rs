//! Where a conversation sits when you come back to it, and when a new
//! message is allowed to move it.

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

use std::time::Duration;

use qmetaobject::*;

mod common;

/// The list, over a plain `ListModel` carrying the roles the real one has.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        width: 540
        height: 400

        ListModel { id: rows }

        Loader {
            id: loader
            anchors.fill: parent
        }

        function append(count) {
            for (var i = 0; i < count; i++) {
                rows.append({
                    text: 'message number ' + rows.count + ', long enough '
                          + 'to take a line of its own in the list',
                    is_outgoing: false, is_info: false, show_padlock: true,
                    state: 16, timestamp: 1700000000 + rows.count,
                    day_number: 19675, sender_name: 'Ada',
                    sender_color: '#00875a', quote_text: '', quote_author: '',
                    file_path: '', file_name: '', view_type: 'Text',
                    image_width: 0, image_height: 0
                })
            }
            return '' + rows.count
        }

        function load(url) {
            loader.setSource(url, { model: rows, title: 'Ada' })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function get(property) { return '' + loader.item[property] }
        function set(property, value) {
            loader.item[property] = value
            return '' + loader.item[property]
        }
        // Scrolling up into the history, the way a reader would: the view's
        // own call, so it re-anchors rather than being shoved by contentY.
        function toTop() { loader.item.positionViewAtBeginning(); return 'ok' }
        function toBottom() { loader.item.positionViewAtEnd(); return 'ok' }
        // Silica sends this when a drag or flick settles.
        function settle() { loader.item.movementEnded(); return 'ok' }
        // The view's own answer to 'is the end on screen'.
        function ended() { return '' + loader.item.atYEnd }
    }
";

fn component_url(name: &str) -> String {
    format!(
        "file://{}",
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../qml/components")
            .join(name)
            .display()
    )
}

#[test]
fn a_conversation_opens_at_the_newest_message_and_stays_where_it_is_left() {
    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));

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
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    single_shot(Duration::from_secs(1), move || unsafe {
        // A history longer than the view, as when reopening a chat.
        record!("filled", call!("append", 40));
        record!(
            "load",
            call!("load", QString::from(component_url("ConversationList.qml")))
        );
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        record!("opened-at-end", call!("ended"));
        record!(
            "opened-sticky",
            call!("get", QString::from("stickToBottom"))
        );
        // One more message arrives while the reader is at the bottom.
        call!("append", 1);
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("followed", call!("ended"));

        // The reader scrolls up into the history and stops there.
        call!("toTop");
        call!("settle");
        record!(
            "scrolled-sticky",
            call!("get", QString::from("stickToBottom"))
        );
        call!("append", 1);
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!("stayed-put", call!("ended"));

        // The reader scrolls back down to the newest message.
        call!("toBottom");
        call!("settle");
        record!(
            "returned-sticky",
            call!("get", QString::from("stickToBottom"))
        );
        call!("append", 1);
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        record!("resumed", call!("ended"));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps);
}

/// Opening lands on the newest message; an arrival moves the view only
/// when the reader is already there.
fn assert_outcome(steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the list did not load. {context}");
    assert_eq!(value("filled"), "40", "the model did not fill. {context}");
    assert_eq!(
        value("opened-at-end"),
        "true",
        "reopening a chat put the reader back at the top of the history. {context}"
    );
    assert_eq!(
        value("opened-sticky"),
        "true",
        "a freshly opened chat does not follow new messages. {context}"
    );
    assert_eq!(
        value("followed"),
        "true",
        "a message arriving while the reader was at the bottom did not scroll in. {context}"
    );
    assert_eq!(
        value("scrolled-sticky"),
        "false",
        "scrolling up into the history left the view still following. {context}"
    );
    assert_eq!(
        value("stayed-put"),
        "false",
        "a message arriving pulled the reader out of the history. {context}"
    );
    assert_eq!(
        value("returned-sticky"),
        "true",
        "scrolling back to the newest message did not resume following. {context}"
    );
    assert_eq!(
        value("resumed"),
        "true",
        "the next message did not scroll in after following resumed. {context}"
    );
}

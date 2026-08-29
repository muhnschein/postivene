//! Who gets told about an arriving message, and who does not.
//!
//! A notification for a chat the reader is already looking at is noise; a
//! notification that outlives the reader arriving is worse, because it
//! trains them to ignore the next one.

// Qt harness: see qml_chat_row.rs.
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

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        Loader { id: loader }
        function load(url) {
            loader.setSource(url, {})
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function set(property, value) {
            loader.item[property] = value
            return 'ok'
        }
        function arrived(chatId, name, preview) {
            loader.item.arrived(chatId, name, preview)
            return 'ok'
        }
        function published() {
            return '' + loader.item.publishedCount()
        }
        // What one chat's notification is currently saying.
        function saying(chatId) {
            var note = loader.item.notes[chatId]
            return note ? note.summary + '/' + note.body : 'none'
        }
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
fn a_message_is_announced_unless_the_reader_is_already_in_that_chat() {
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
        record!(
            "load",
            call!("load", QString::from(component_url("Notifier.qml")))
        );
        call!("set", QString::from("appActive"), true);
        call!("set", QString::from("viewingChatId"), 0);

        // Nobody is reading anything: both chats speak up.
        call!(
            "arrived",
            7,
            QString::from("Ada"),
            QString::from("see you there")
        );
        call!("arrived", 9, QString::from("Bob"), QString::from("ping"));
        record!("two-chats", call!("published"));
        record!("ada-says", call!("saying", 7));
        record!("bob-says", call!("saying", 9));

        // Walking into Ada's chat takes Ada's notification down, and
        // leaves Bob's alone.
        call!("set", QString::from("viewingChatId"), 7);
        record!("after-opening-ada", call!("published"));

        // A second message from Ada, while Ada is on screen, is not news.
        call!(
            "arrived",
            7,
            QString::from("Ada"),
            QString::from("still here")
        );
        record!("while-reading-ada", call!("published"));

        // The same message with the app in the background is news again:
        // on screen is not the same as seen.
        call!("set", QString::from("appActive"), false);
        call!(
            "arrived",
            7,
            QString::from("Ada"),
            QString::from("and again")
        );
        record!("while-backgrounded", call!("published"));

        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps);
}

/// Who was told, and who was spared.
fn assert_outcome(steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the notifier did not load. {context}");
    assert_eq!(
        value("two-chats"),
        "2",
        "two chats talking at once did not each get a notification. {context}"
    );
    assert_eq!(
        value("ada-says"),
        "Ada/see you there",
        "the notification does not carry the chat and the message. {context}"
    );
    assert_eq!(
        value("bob-says"),
        "Bob/ping",
        "the second chat's notification was overwritten by the first. {context}"
    );
    assert_eq!(
        value("after-opening-ada"),
        "1",
        "opening a chat did not take its notification down, or took the \
         other chat's down with it. {context}"
    );
    assert_eq!(
        value("while-reading-ada"),
        "1",
        "a message was announced into the face of someone already reading \
         that chat. {context}"
    );
    assert_eq!(
        value("while-backgrounded"),
        "2",
        "a message arriving while the app was in the background went \
         unannounced because that chat happened to be the last one open. \
         {context}"
    );
}

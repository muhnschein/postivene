//! Writing a message longer than one line, and longer than the core
//! will carry whole.
//!
//! Two things about the field a message is written in. It takes more
//! than a line: the return key used to send, so a message written here
//! was one line however long it ran, and a paragraph could not be typed
//! at all. And it says when what is in it has grown past the point where
//! the core cuts a body in two -- past that, what arrives at the other
//! end is a preview with something to tap, which is worth knowing before
//! pressing send and not worth a dialog afterwards. parla says the same
//! thing in the same place; the rule itself is pinned against the real
//! core in `deltachat-jsonrpc/tests/real_server.rs`.

// Qt harness: see qml_conversation_open.rs.
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

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import Postivene 1.0
    Item {
        width: 540
        height: 960

        Loader { id: loader; width: 540; height: 960 }
        function open(url) {
            loader.setSource(url, {
                accountId: 1, chatId: 1, status: PageStatus.Active
            })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            if (node.contentItem && node.contentItem !== node) {
                return findIn(node.contentItem, name)
            }
            return null
        }
        function type(text) {
            var field = findIn(loader.item, 'messageField')
            if (!field) { return 'missing:messageField' }
            field.text = text
            return 'ok'
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function fieldHeight() {
            var field = findIn(loader.item, 'messageField')
            return field ? '' + Math.round(field.height) : 'missing'
        }
    }
";

/// A draft past the point where the core cuts a body: 40 lines, where
/// its limit is 38.
fn a_long_draft() -> String {
    let mut out = String::new();
    for number in 1..=40 {
        use std::fmt::Write as _;
        let _ = writeln!(out, "- [ ] the {number}th thing to do today");
    }
    out
}

#[test]
#[allow(clippy::too_many_lines)]
fn the_field_takes_more_than_a_line_and_says_when_a_message_is_too_long() {
    let temp = std::env::temp_dir().join(format!("postivene-long-draft-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
    }

    postivene_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut steps: Vec<(&str, String)> = Vec::new();
    let steps_ptr: *mut Vec<(&str, String)> = std::ptr::addr_of_mut!(steps);
    let page = common::page_url_in(&tree, "ConversationPage.qml");

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

    let long = a_long_draft();

    single_shot(Duration::from_secs(1), move || unsafe {
        record!("open", call!("open", QString::from(page.clone())));
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        record!(
            "quiet",
            call!(
                "get",
                QString::from("longMessageBar"),
                QString::from("visible")
            )
        );
        record!("one-line-height", call!("fieldHeight"));
        // A paragraph: three lines, which the old field could not hold
        // at all.
        record!("typed", call!("type", QString::from("one\ntwo\nthree")));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("three-line-height", call!("fieldHeight"));
        record!(
            "still-quiet",
            call!(
                "get",
                QString::from("longMessageBar"),
                QString::from("visible")
            )
        );
        record!("long", call!("type", QString::from(long.clone())));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!(
            "warned",
            call!(
                "get",
                QString::from("longMessageBar"),
                QString::from("visible")
            )
        );
        record!(
            "warning",
            call!(
                "get",
                QString::from("longMessageLabel"),
                QString::from("text")
            )
        );
        record!("capped-height", call!("fieldHeight"));
        // Taken back out again: the notice follows the draft rather than
        // staying once it has been shown.
        record!("shortened", call!("type", QString::from("never mind")));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        record!(
            "quiet-again",
            call!(
                "get",
                QString::from("longMessageBar"),
                QString::from("visible")
            )
        );
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
    let number = |label: &str| value(label).parse::<f64>().unwrap_or(-1.0);
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("open"),
        "ok",
        "the conversation page did not load. {context}"
    );
    assert_eq!(
        value("quiet"),
        "false",
        "an empty field was told its message was too long. {context}"
    );
    assert_eq!(
        value("still-quiet"),
        "false",
        "three lines were called a long message. {context}"
    );

    let one_line = number("one-line-height");
    let three_lines = number("three-line-height");
    assert!(
        three_lines > one_line,
        "the field stayed {one_line} tall for a three-line message, so \
         the writer can see one line of what they are writing -- and the \
         line breaks in it are not line breaks at all. {context}"
    );
    // A third of the page: past that the conversation being written in
    // would be gone.
    assert!(
        number("capped-height") <= 320.0,
        "a forty-line draft grew the field to {} on a 960-tall page, \
         which leaves nothing of the chat above it. {context}",
        number("capped-height")
    );

    assert_eq!(
        value("warned"),
        "true",
        "a forty-line draft said nothing about being cut on the way out, \
         so the reader finds out from the other end. {context}"
    );
    assert!(
        value("warning").contains("Long message"),
        "the notice does not say what it is about: {:?}. {context}",
        value("warning")
    );
    assert_eq!(
        value("quiet-again"),
        "false",
        "the notice stayed after the draft was shortened. {context}"
    );
}

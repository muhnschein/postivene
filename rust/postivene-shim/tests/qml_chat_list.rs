//! The chat list is where a running app sits, so it is where the core's own
//! failures have to show up.

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

use postivene_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        Loader { id: loader }
        function load(url, accountId) {
            // Clear first, so loading the same page again really is a new
            // instance rather than the one already there.
            loader.setSource('', {})
            loader.setSource(url, { accountId: accountId })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        // Signals are callable, so the page can be driven without waiting
        // for the core to produce a real failure.
        function raise(message) { core.core_error(message) }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            return null
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
    }
";

#[test]
fn the_chat_list_shows_what_the_core_reports() {
    let temp = std::env::temp_dir().join(format!("postivene-qml-chat-list-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
        // After the error has been read, before the last read.
        std::env::set_var("POSTIVENE_FAKE_EXIT_AFTER_MS", "4500");
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
        (*steps_ptr).push((
            "load",
            call!(
                "load",
                QString::from(common::page_url("ChatListPage.qml")),
                1
            ),
        ));
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        call!("raise", QString::from("disk full"));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push((
            "error-shown",
            call!("get", QString::from("errorLabel"), QString::from("text")),
        ));
        (*steps_ptr).push((
            "error-timeout",
            call!(
                "get",
                QString::from("errorBanner"),
                QString::from("timeout")
            ),
        ));
    });

    // The server is gone by now.
    single_shot(Duration::from_secs(6), move || unsafe {
        (*steps_ptr).push((
            "dead-shown",
            call!("get", QString::from("errorLabel"), QString::from("text")),
        ));
        (*steps_ptr).push((
            "dead-timeout",
            call!(
                "get",
                QString::from("errorBanner"),
                QString::from("timeout")
            ),
        ));
        // A page opened after the core died never saw it happen. It still
        // has to say so, or it looks like an empty chat with no history.
        (*steps_ptr).push((
            "reopen",
            call!(
                "load",
                QString::from(common::page_url("ChatListPage.qml")),
                1
            ),
        ));
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        (*steps_ptr).push((
            "reopened-shown",
            call!("get", QString::from("errorLabel"), QString::from("text")),
        ));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps);
}

/// Both failures are on the page, and only the one that cannot be waited
/// out stays there.
fn assert_outcome(steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the page did not load. {context}");
    assert_eq!(
        value("error-shown"),
        "disk full",
        "a core error reached no one. {context}"
    );
    assert_eq!(
        value("error-timeout"),
        "8",
        "an error the user can dismiss should clear itself. {context}"
    );
    assert!(
        value("dead-shown").contains("Restart"),
        "the core died and the page said nothing. {context}"
    );
    assert_eq!(
        value("dead-timeout"),
        "0",
        "a message about a dead core must not time out. {context}"
    );
    assert_eq!(
        value("reopen"),
        "ok",
        "the page did not load a second time. {context}"
    );
    assert!(
        value("reopened-shown").contains("Restart"),
        "a page opened after the core died says nothing about it. {context}"
    );
}

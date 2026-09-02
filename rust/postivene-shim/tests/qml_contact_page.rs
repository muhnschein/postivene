//! The contact page shows who a one-to-one chat is with, and no address.
//!
//! Loaded headlessly against the stub Silica module and the recording
//! double: the one member of chat 1 comes up with their name, their line
//! about themselves and the state of the connection, and nothing on the
//! page is an email address.

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

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }
        function load(url, chatId) {
            loader.setSource(url, { accountId: 1, chatId: chatId, chatName: 'from the list' })
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
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        // Every string drawn anywhere on the page, for the one thing
        // none of them may be.
        function allText(node) {
            if (!node) { return '' }
            var out = (node.text !== undefined && node.visible) ? node.text + '|' : ''
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                out += allText(kids[i])
            }
            if (node.contentItem && node.contentItem !== node) {
                out += allText(node.contentItem)
            }
            return out
        }
        function texts() { return allText(loader.item) }
    }
";

#[test]
fn the_contact_page_names_the_contact_and_no_address() {
    let temp = std::env::temp_dir().join(format!("postivene-contact-page-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

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
    macro_rules! get {
        ($name:expr, $property:expr) => {
            call!("get", QString::from($name), QString::from($property))
        };
    }
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    // Chat 1 is with Ada, who is verified and has written a line.
    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::page_url("ContactPage.qml")),
                1
            )
        );
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("loaded", get!("chat", "loaded"));
        record!("name", get!("contactName", "text"));
        record!("initial", get!("avatarInitial", "text"));
        record!("status", get!("statusLabel", "text"));
        record!("status-shown", get!("statusLabel", "visible"));
        record!("encryption", get!("encryptionLabel", "text"));
        record!("texts", call!("texts"));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_page(&steps);
}

/// The contact's own words and picture, the state of the connection, and
/// nothing that looks like an address.
fn assert_page(steps: &[(&str, String)]) {
    let context = format!("steps: {steps:?}");
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };

    assert_eq!(
        value("load"),
        "ok",
        "the contact page did not load. {context}"
    );
    assert_eq!(value("loaded"), "true", "the chat never loaded. {context}");
    assert_eq!(
        value("name"),
        "ada",
        "the page does not name the contact. {context}"
    );
    assert_eq!(
        value("initial"),
        "A",
        "the avatar is not standing in with the contact's initial. {context}"
    );
    assert_eq!(
        value("status"),
        "Poet and mathematician",
        "the contact's own line about themselves is missing. {context}"
    );
    assert_eq!(
        value("status-shown"),
        "true",
        "a line that was written is hidden. {context}"
    );
    assert!(
        value("encryption").starts_with("Verified"),
        "a verified contact is not said to be. {context}"
    );
    assert!(
        !value("texts").contains('@'),
        "an email address is drawn on the contact page. {context}"
    );
}

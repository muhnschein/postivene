//! The cover, with people in it.
//!
//! Two of its three states, on the fake core's two chats: the grid of
//! everyone in grey while nothing is new, and whoever wrote drawn large
//! and in colour, with the count, once something is. The third state --
//! nobody yet -- is qml_cover_empty.rs, since it takes a core seeded
//! differently.

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

/// Loads the cover at a size, since the stub CoverBackground has none of
/// its own, and reads it back.
pub const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        Loader { id: loader }
        function load(url) {
            loader.setSource(url, { accountId: 1, width: 240, height: 360 })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function setAccount(accountId) {
            loader.item.accountId = accountId
            return 'ok'
        }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            return null
        }
        function countIn(node, name) {
            if (!node) { return 0 }
            var total = node.objectName === name && node.visible ? 1 : 0
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                total += countIn(kids[i], name)
            }
            return total
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function count(name) {
            return '' + countIn(loader.item, name)
        }
        function markUnread(chatId) {
            findIn(loader.item, 'coverChats').mark_unread(chatId)
            return 'ok'
        }
        // How many cells the cover's own shape gives the grid.
        function cells() {
            return '' + (loader.item.columns * loader.item.rows)
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_cover_draws_everyone_and_lifts_whoever_wrote() {
    let temp = std::env::temp_dir().join(format!("postivene-qml-cover-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
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

    single_shot(Duration::from_secs(1), move || unsafe {
        record!("load", call!("load", QString::from(cover_url())));
    });

    // The fake core reports no configured account, so the cover's own
    // refresh sets the account to none; the account is put back once
    // that has landed, as a real core would have reported it.
    single_shot(Duration::from_secs(2), move || unsafe {
        call!("setAccount", 1);
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!("brand", get!("brand", "text"));
        record!("empty-hidden", get!("emptyLabel", "visible"));
        record!("count-hidden", get!("unreadTotal", "visible"));
        record!("cells", call!("cells"));
        record!("grid-quiet", call!("count", QString::from("gridCell")));
        record!(
            "nobody-lifted",
            call!("count", QString::from("writerAvatar"))
        );
        record!("grid-dim-quiet", get!("avatarGrid", "opacity"));
        record!("mark", call!("markUnread", 1));
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("count-shown", get!("unreadTotal", "visible"));
        record!("count-says", get!("unreadTotal", "text"));
        record!("one-lifted", call!("count", QString::from("writerAvatar")));
        record!("lifted-in-colour", get!("writerAvatar", "monochrome"));
        record!("grid-behind", call!("count", QString::from("gridCell")));
        record!("grid-grey", get!("gridCell", "monochrome"));
        record!("grid-dim-loud", get!("avatarGrid", "opacity"));
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

    assert_eq!(value("load"), "ok", "the cover did not load. {context}");
    assert_eq!(
        value("brand"),
        "Delta",
        "the cover does not name the app in its corner. {context}"
    );
    assert_eq!(
        value("empty-hidden"),
        "false",
        "the cover says there is nobody while there are two chats. {context}"
    );
    assert_eq!(
        value("count-hidden"),
        "false",
        "a count is shown with nothing unread. {context}"
    );
    // 240 wide in three columns is 80 a cell, and 360 high less the
    // heading leaves room for three rows of them at the stub's font
    // size; two people are repeated to fill whatever that comes to.
    let cells: u32 = value("cells").parse().unwrap_or(0);
    assert!(
        cells >= 6,
        "the cover's shape gives the grid fewer than two rows. {context}"
    );
    assert_eq!(
        value("grid-quiet"),
        cells.to_string(),
        "the grid is not filled from the two people there are. {context}"
    );
    assert_eq!(
        value("nobody-lifted"),
        "0",
        "someone is drawn large with nothing unread. {context}"
    );
    assert_eq!(
        value("mark"),
        "ok",
        "marking a chat unread failed. {context}"
    );
    assert_eq!(
        value("count-shown"),
        "true",
        "the count did not appear once something was unread. {context}"
    );
    assert_eq!(
        value("count-says"),
        "1",
        "the count is not the number of unread messages. {context}"
    );
    assert_eq!(
        value("one-lifted"),
        "1",
        "the one who wrote is not drawn large, or others are. {context}"
    );
    assert_eq!(
        value("lifted-in-colour"),
        "false",
        "the one who wrote is drawn in grey like everyone else. {context}"
    );
    assert_eq!(
        value("grid-behind"),
        cells.to_string(),
        "the grid behind is not filled from whoever did not write. {context}"
    );
    assert_eq!(
        value("grid-grey"),
        "true",
        "the grid of everyone is drawn in colour. {context}"
    );
    let quiet: f64 = value("grid-dim-quiet").parse().unwrap_or(0.0);
    let loud: f64 = value("grid-dim-loud").parse().unwrap_or(1.0);
    assert!(
        loud < quiet,
        "the grid is not dimmed further behind whoever wrote. {context}"
    );
}

fn cover_url() -> String {
    format!(
        "file://{}",
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../qml/cover/CoverPage.qml")
            .display()
    )
}

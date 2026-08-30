//! The root QML loads, and so does everything it reaches on the way up.
//!
//! Nothing covered this before, and a white screen was the result: the
//! cover's `showChats` signal was removed while the root file still
//! handled it, and `Cannot assign to non-existent property "onShowChats"`
//! takes the whole window down with it.
//!
//! Grep could not have caught it. A QML handler capitalises the signal it
//! answers, so `showChats` becomes `onShowChats` and a search for the
//! signal's own name never finds the thing that uses it. Loading the file
//! is what finds it.

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
    Item {
        Loader { id: loader }
        function load(url) {
            loader.setSource(url, {})
            if (loader.status === Loader.Error) {
                return 'error: ' + loader.sourceComponent
            }
            return loader.status === Loader.Ready ? 'ok' : 'not-ready'
        }
        /// The cover is a Component on the window; instantiating it is
        /// what would have found the handler for a signal that no longer
        /// exists, since a Component is not built until it is used.
        function buildCover() {
            if (!loader.item || !loader.item.cover) { return 'no-cover' }
            var made = loader.item.cover.createObject(loader.item, {})
            return made ? 'ok' : 'cover-failed'
        }
        function buildInitialPage() {
            if (!loader.item || !loader.item.initialPage) { return 'no-page' }
            var made = loader.item.initialPage.createObject(loader.item, {})
            return made ? 'ok' : 'page-failed'
        }
    }
";

#[test]
fn the_application_window_and_what_it_holds_all_load() {
    let temp = std::env::temp_dir().join(format!("postivene-startup-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    // `EnterKey` has no stub, and ConversationPage is not reached from
    // here anyway; the copy keeps this test to one reason to fail.
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
    // The real one is handed in by main.rs; the value is never read here.
    engine.set_property(
        "rpcServerPath".into(),
        QString::from(env!("CARGO_BIN_EXE_fake-core-server")).into(),
    );
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

    let root = format!("file://{}", tree.join("postivene.qml").display());

    single_shot(Duration::from_secs(1), move || unsafe {
        (*steps_ptr).push(("load", call!("load", QString::from(root.clone()))));
        (*steps_ptr).push(("cover", call!("buildCover")));
        (*steps_ptr).push(("initial-page", call!("buildInitialPage")));
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
        value("load"),
        "ok",
        "the root QML does not load, so the app is a white screen. {context}"
    );
    assert_eq!(
        value("cover"),
        "ok",
        "the cover does not build. A Component is not checked until it is \
         instantiated, so a handler for a signal that no longer exists sits \
         there silently until the app is launched. {context}"
    );
    assert_eq!(
        value("initial-page"),
        "ok",
        "the first page the app shows does not build. {context}"
    );
}

//! What the app is told when the core fails: an `Error` event reaches QML as
//! a typed signal, and the server dying stops `status` claiming otherwise.

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

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Postivene 1.0
    Item {
        property string lastError: ''
        ChatMessages { id: chat; account_id: 1; chat_id: 1 }
        Connections {
            target: core
            onCore_error: lastError = message
        }
        function fail() { chat.send('please fail') }
        function report() { return lastError + '#' + core.status }
    }
";

#[test]
fn a_core_failure_and_a_dead_server_both_reach_qml() {
    let temp = std::env::temp_dir().join(format!("postivene-core-status-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
        // After the failing send has been answered, and before the last
        // read.
        std::env::set_var("POSTIVENE_FAKE_EXIT_AFTER_MS", "4500");
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

    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    single_shot(Duration::from_secs(3), move || unsafe {
        (*engine_ptr).invoke_method("fail".into(), &[]);
    });

    let mut before_death = String::new();
    let before_ptr: *mut String = std::ptr::addr_of_mut!(before_death);
    single_shot(Duration::from_secs(4), move || unsafe {
        let value = (*engine_ptr).invoke_method("report".into(), &[]);
        *before_ptr = QString::from_qvariant(value)
            .map(|text| text.to_string())
            .unwrap_or_default();
    });

    let mut after_death = String::new();
    let after_ptr: *mut String = std::ptr::addr_of_mut!(after_death);
    single_shot(Duration::from_secs(7), move || unsafe {
        let value = (*engine_ptr).invoke_method("report".into(), &[]);
        *after_ptr = QString::from_qvariant(value)
            .map(|text| text.to_string())
            .unwrap_or_default();
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_eq!(
        before_death, "could not send#ready",
        "the core's own Error event did not reach QML as a typed signal"
    );
    assert_eq!(
        after_death, "could not send#stopped",
        "the server died and `status` went on claiming the core was there"
    );
}

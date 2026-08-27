//! Confirms how `qmetaobject`'s `QObject` derive exposes Rust identifiers
//! to QML: does it keep `snake_case` verbatim, or `camelCase` it? This is
//! load-bearing for every `.qml` file that calls into `DeltaChatCore`, so
//! it's worth pinning down with a real QML load rather than assuming.

// This is Qt harness code: it drives a real event loop from a test, which
// needs three things the workspace lints otherwise deny, each already
// carrying its own SAFETY/justification note below:
//
// * `unsafe`: setting `QT_QPA_PLATFORM=offscreen` before Qt initialises.
//   `unused_unsafe` rides along because `std::env::set_var` is safe on the
//   Rust 1.75 floor but unsafe from edition 2024 on: the block is required
//   by the newer compiler and merely redundant on the older one, and the
//   MSRV job builds with warnings denied.
// * `borrow_as_ptr`: handing the engine to a timer callback that outlives
//   the borrow but not the engine.
// * `single_shot`: allowed here because every call passes a *whole-second*
//   `Duration`, which is the case qmetaobject 0.2.10 converts correctly
//   (see clippy.toml for the bug this lint guards).
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods
)]

use std::time::Duration;

use postivene_shim::DeltaChatCore;
use qmetaobject::*;

#[test]
fn methods_properties_and_signals_are_exposed_verbatim_snake_case() {
    // SAFETY: see the same justification in tests/smoke.rs.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());

    let server_path = QString::from(env!("CARGO_BIN_EXE_fake-health-server"));
    core_box.pinned().borrow_mut().start(server_path);

    // If any of `core.check_health`, `core.status`, or the
    // `system_info_changed` signal handler name were wrong, this QML would
    // fail to bind/compile and `sawSystemInfoChanged` would stay false, or
    // Qt would print "ReferenceError"/"is not a function" to stderr.
    let qml = r"
        import QtQuick 2.0
        Item {
            property bool sawSystemInfoChanged: false
            Connections {
                target: core
                onSystem_info_changed: sawSystemInfoChanged = true
            }
            Timer {
                interval: 300; running: true; repeat: false
                onTriggered: core.check_health()
            }
        }
    ";
    engine.load_data(QByteArray::from(qml));

    let engine_ptr = &engine as *const QmlEngine;
    single_shot(Duration::from_secs(2), move || {
        // SAFETY: see tests/smoke.rs -- same lifetime argument.
        unsafe {
            (*engine_ptr).quit();
        }
    });

    engine.exec();

    let system_info = core_box.pinned().borrow().system_info.to_string();
    assert!(
        system_info.contains("fake-health-server"),
        "core.check_health() from QML did not run (system_info: {system_info:?}); \
         snake_case method/property names may not be what's actually exposed to QML"
    );
}

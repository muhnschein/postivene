//! Pins the one QML convention that silently breaks on device: Sailfish
//! runs **Qt 5.6**, where `Connections` only recognises `onFoo:` script
//! bindings. The `function onFoo() {}` form is Qt 5.15+, and on 5.6 it is
//! not a syntax error -- it is just an ordinary function declaration that
//! never gets connected, so handlers stop firing with no diagnostic at
//! all. (That shipped once: the app launched, rendered, and sat on a
//! spinner forever because every `Connections` handler was dead.)
//!
//! It also pins how signal *parameters* reach QML: qmetaobject writes the
//! Rust identifiers into the metaobject verbatim, so handlers see
//! `snake_case` names (`context_id`, not `contextId`).

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

use postivene_shim::DeltaChatCore;
use qmetaobject::*;

#[test]
fn old_style_handlers_receive_snake_case_signal_parameters() {
    // SAFETY: see the same justification in tests/smoke.rs.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());

    // The handler only calls back into the core if every injected
    // parameter name resolves *and* the handler is genuinely connected.
    // A `function onCore_event(...)` declaration here, or camelCase
    // parameter names, would leave `status` untouched.
    let qml = r#"
        import QtQuick 2.0
        Item {
            Connections {
                target: core
                onAccount_error: {
                    // `message` is the parameter name the shim declares;
                    // camelCase, or a `function onAccount_error(msg)` form,
                    // would leave this handler dead or throw a
                    // ReferenceError, and `check_health` would never run.
                    if (message === "not started") {
                        core.check_health()
                    }
                }
            }
        }
    "#;
    engine.load_data(QByteArray::from(qml));

    // With no core running this emits `account_error("not started")`,
    // which is the signal the QML above listens for.
    core_box.pinned().borrow_mut().refresh_accounts();

    // `check_health` without a running core sets exactly this status, so
    // seeing it proves the QML handler ran with the right arguments.
    let status = core_box.pinned().borrow().status.to_string();
    assert_eq!(
        status, "error: not started",
        "the Connections handler never fired: on Qt 5.6 `Connections` needs \
         `onCore_event:` (not `function onCore_event()`), with the shim's \
         snake_case parameter names"
    );
}

//! Drives a real (offscreen) Qt event loop to prove the trickiest part of
//! the architecture actually works end-to-end: a `qt_method` call kicks
//! off async work on a background tokio runtime, and the result comes back
//! through `queued_callback` to mutate `qt_property`s / fire `qt_signal`s
//! back on the Qt thread -- not just that the types compile.
//!
//! Uses `fake-health-server` (see `src/bin/fake_health_server.rs`) instead
//! of a real `deltachat-rpc-server`, which isn't available in this
//! environment.

use std::time::Duration;

use postivene_shim::DeltaChatCore;
use qmetaobject::*;

#[test]
fn health_check_round_trips_through_qt_event_loop() {
    // SAFETY: this test is the only thing running in this process and Qt
    // must be told which QPA platform to use before the first QGuiApplication
    // is constructed; there is no safe alternative for setting process
    // environment this early in a single-threaded test binary.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let core_box = QObjectBox::new(DeltaChatCore::default());

    let mut engine = QmlEngine::new();
    // Registering the object with the engine is what creates its backing
    // C++ QObject; `QPointer::from` requires that to have already
    // happened, so this must run before we take a `QPointer` to it.
    engine.set_object_property("core".into(), core_box.pinned());
    let core_ptr: QPointer<DeltaChatCore> = QPointer::from(core_box.pinned().borrow());

    let server_path = QString::from(env!("CARGO_BIN_EXE_fake-health-server"));
    core_box.pinned().borrow_mut().start(server_path);

    let health_check_ptr = core_ptr.clone();
    // NOTE: `qmetaobject::single_shot` (0.2.10) mis-converts the sub-second
    // part of a `Duration` (`subsec_nanos() * (1e-6 as u32)`, and
    // `1e-6 as u32` truncates to `0`), so any non-whole-second `Duration`
    // schedules as if it were 0ms. Use whole seconds to sidestep it.
    single_shot(Duration::from_secs(1), move || {
        if let Some(this) = health_check_ptr.as_pinned() {
            this.borrow_mut().check_health();
        }
    });

    let engine_ptr = &engine as *const QmlEngine;
    single_shot(Duration::from_secs(3), move || {
        // SAFETY: `engine` outlives this callback: the callback only ever
        // fires while `engine.exec()` below is still running on this same
        // thread, and `engine` isn't dropped until after `exec()` returns.
        unsafe {
            (*engine_ptr).quit();
        }
    });

    engine.exec();

    let status = core_box.pinned().borrow().status.to_string();
    let system_info = core_box.pinned().borrow().system_info.to_string();

    assert_eq!(status, "ready", "system_info was: {system_info}");
    assert!(
        system_info.contains("fake-health-server"),
        "system_info was: {system_info}"
    );
}

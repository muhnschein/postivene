//! Does the *real* core accept the shapes this shim sends?
//!
//! The fake double in `tests/onboarding.rs` proves which calls a UI action
//! makes; it cannot prove those calls are well-formed, because the double
//! is written from the same reading of the API as the code under test. If
//! that reading is wrong -- a field named `mail_pw` instead of `password`,
//! positional arguments where an object belongs -- both agree and both are
//! wrong, and the failure surfaces on a phone.
//!
//! So this drives the shim against the actual pinned `deltachat-rpc-server`
//! and looks at *what kind* of error comes back. A misshapen request is
//! rejected by serde before the core does anything, with a
//! deserialization message; a well-formed request for an unreachable
//! server fails later, while configuring. Distinguishing the two is the
//! whole test, and it needs no reachable mail server: every address here is
//! in a reserved TLD that cannot resolve.
//!
//! Gated on `DELTACHAT_RPC_SERVER`, like `deltachat-jsonrpc`'s own
//! `real_server` test.

// See tests/smoke.rs for why this Qt harness needs these allows.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods
)]

use std::time::Duration;

use postivene_shim::DeltaChatCore;
use qmetaobject::*;

/// Substrings that mean "the core could not even decode the request".
/// These are what serde emits when a param shape is wrong, and any of them
/// is a failure of this test rather than of the network.
const SHAPE_ERRORS: &[&str] = &[
    "invalid type",
    "missing field",
    "unknown field",
    "unknown variant",
    "invalid params",
    "expected struct",
    "Invalid params",
];

/// Resolve the gate's value, treating a relative path as relative to the
/// repository root rather than to the process's working directory.
///
/// Cargo runs an integration test with its working directory set to the
/// *package* root, not the workspace or repository root, so the obvious
/// `DELTACHAT_RPC_SERVER=../vendor/...` -- which is what the README, the
/// Makefile and CI all naturally write -- would otherwise look for the
/// binary under `rust/`. That failure only shows up where the variable is
/// actually set, which is CI, and reads as a missing download rather than a
/// wrong path.
fn resolve(path: &str) -> String {
    let path = std::path::Path::new(path);
    if path.is_absolute() {
        return path.to_string_lossy().into_owned();
    }
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
        .to_string_lossy()
        .into_owned()
}

fn real_server() -> Option<String> {
    match std::env::var("DELTACHAT_RPC_SERVER") {
        Ok(path) if !path.is_empty() => Some(resolve(&path)),
        _ => {
            eprintln!(
                "skipping: set DELTACHAT_RPC_SERVER to a real deltachat-rpc-server binary to run"
            );
            None
        }
    }
}

#[test]
fn the_real_core_accepts_the_shapes_we_send() {
    let Some(server) = real_server() else {
        return;
    };

    let temp = std::env::temp_dir().join(format!("postivene-real-core-{}", std::process::id()));
    std::fs::create_dir_all(&temp).expect("create temp accounts dir");

    // SAFETY: single-threaded test binary; both must be set before Qt starts
    // and before the shim spawns the server that inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", &temp);
    }

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());

    // Collect every error message the shim reports, plus how the core
    // classified a `dcaccount:` payload -- the QR path's own shape check.
    let qml = r"
        import QtQuick 2.0
        Item {
            property string errors: ''
            property int created: 0
            property string qrKind: ''
            Connections {
                target: core
                onProfile_error: errors = errors + '|' + message
                onProfile_created: created = created + 1
                onQr_checked: qrKind = kind
            }
            function report() { return created + '#' + qrKind + '#' + errors }
        }
    ";
    engine.load_data(QByteArray::from(qml));

    core_box.pinned().borrow_mut().start(QString::from(server));
    let core_ptr: QPointer<DeltaChatCore> = QPointer::from(core_box.pinned().borrow());

    // Whole seconds only (qmetaobject 0.2.10; see clippy.toml).
    let email = core_ptr.clone();
    single_shot(Duration::from_secs(1), move || {
        if let Some(this) = email.as_pinned() {
            this.borrow_mut().create_profile_with_email(
                QString::from("Ada"),
                // `.invalid` is reserved by RFC 2606 and cannot resolve, so
                // this fails at connect time, never at parse time.
                QString::from("ada@postivene-test.invalid"),
                QString::from("not-a-real-password"),
            );
        }
    });

    let qr = core_ptr;
    single_shot(Duration::from_secs(9), move || {
        if let Some(this) = qr.as_pinned() {
            // Account 1 exists by now: create_profile_with_email made it.
            this.borrow_mut()
                .check_qr(1, QString::from("dcaccount:postivene-test.invalid"));
        }
    });

    let engine_ptr = &engine as *const QmlEngine;
    single_shot(Duration::from_secs(20), move || {
        // SAFETY: see tests/smoke.rs.
        unsafe {
            (*engine_ptr).quit();
        }
    });

    engine.exec();

    let report = QString::from_qvariant(engine.invoke_method("report".into(), &[]))
        .map(|value| value.to_string())
        .unwrap_or_default();
    let mut parts = report.splitn(3, '#');
    let created = parts.next().unwrap_or_default().to_string();
    let qr_kind = parts.next().unwrap_or_default().to_string();
    let errors = parts.next().unwrap_or_default().to_string();

    assert_eq!(
        created, "0",
        "an unreachable server somehow configured successfully: {report}"
    );
    assert!(
        !errors.is_empty(),
        "no result came back within the time budget. Either the core got \
         slower at failing on an unresolvable host, or the call never \
         completed. Report: {report:?}"
    );

    for marker in SHAPE_ERRORS {
        assert!(
            !errors.contains(marker),
            "the real core rejected our request shape ({marker:?}) rather \
             than failing to reach the server -- the params we send do not \
             match its API. Errors were: {errors}"
        );
    }

    // The QR path's shape: the core parsed a `dcaccount:` payload and said
    // so. A wrong `check_qr` param shape would land in `qr_error` instead
    // and leave this empty.
    assert_eq!(
        qr_kind, "account",
        "check_qr did not classify a dcaccount: payload as an account \
         invite: {report}"
    );
}

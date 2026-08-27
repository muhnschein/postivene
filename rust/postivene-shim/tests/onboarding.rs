//! What the onboarding actions actually say to the core.
//!
//! These are contract tests, not behaviour tests: they drive a real Qt
//! event loop against the recording double (`src/bin/fake_core_server.rs`)
//! and then assert on the *journal* of JSON-RPC calls it wrote. What is
//! being pinned is the thing that silently rots -- that creating a profile
//! sets a display name and hands a `dcaccount:` payload to
//! `add_transport_from_qr`, that an email login sends an
//! `EnteredLoginParam` to `add_or_update_transport`, and that neither one
//! ever calls the deprecated `configure` again (docs/ONBOARDING.md).
//!
//! Everything runs in one event loop and one process because a second
//! `QGuiApplication` in the same process is not a thing Qt allows.

// See tests/smoke.rs for why this Qt harness needs the first three allows.
// `expect_used` is allowed for the whole file rather than just the `#[test]`
// function clippy recognises: the assertion helpers below are test code too,
// and a missing recorded call should stop the test with its message.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::path::PathBuf;
use std::time::Duration;

use postivene_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

/// The probe object the test loads beside the real shim.
///
/// It is part of the assertion, not scaffolding: it records what the shim
/// *signalled*, which is the half of the contract the call journal cannot
/// show. Written in the Qt 5.6 dialect the device needs, with the shim's
/// `snake_case` signal and parameter names -- so a renamed signal or a
/// changed parameter name fails here rather than on a phone.
const PROBE_QML: &str = r"
        import QtQuick 2.0
        Item {
            property int created: 0
            property int lastAccount: 0
            property int errors: 0
            property int progressEvents: 0
            property int lastPermille: 0
            Connections {
                target: core
                onProfile_created: {
                    created = created + 1
                    lastAccount = account_id
                }
                onProfile_error: errors = errors + 1
                onConfigure_progress: {
                    progressEvents = progressEvents + 1
                    lastPermille = permille
                }
            }
            // Read back after the loop stops: invoke_method is the only way
            // into the root object from Rust.
            function summary() {
                return created + '/' + errors + '/' + progressEvents
                    + '/' + lastPermille + '/' + lastAccount
            }
        }
    ";

/// One recorded JSON-RPC request.
struct Call {
    method: String,
    params: Value,
}

fn read_journal(path: &PathBuf) -> Vec<Call> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .map(|value| Call {
            method: value
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            params: value.get("params").cloned().unwrap_or(Value::Null),
        })
        .collect()
}

fn methods(calls: &[Call]) -> Vec<&str> {
    calls.iter().map(|call| call.method.as_str()).collect()
}

fn find<'a>(calls: &'a [Call], method: &str) -> Option<&'a Call> {
    calls.iter().find(|call| call.method == method)
}

#[test]
fn onboarding_speaks_the_current_transport_api() {
    let temp = std::env::temp_dir().join(format!("postivene-onboarding-{}", std::process::id()));
    let journal = temp.join("journal.jsonl");
    let accounts = temp.join("accounts");
    std::fs::create_dir_all(&accounts).expect("create temp dirs");

    // SAFETY: single-threaded test binary, and all three have to be set
    // before Qt initialises and before the shim spawns the server that
    // inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_FAKE_JOURNAL", &journal);
        // Keeps the shim's accounts directory out of the developer's real
        // XDG data dir.
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", &accounts);
    }

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());

    // The QML is part of the assertion: it records what the shim signalled,
    // which is the half of the contract the journal cannot show. Written in
    // the Qt 5.6 dialect the device needs, with the shim's snake_case names.
    engine.load_data(QByteArray::from(PROBE_QML));

    let server = QString::from(env!("CARGO_BIN_EXE_fake-core-server"));
    core_box.pinned().borrow_mut().start(server);

    // Whole seconds only: qmetaobject 0.2.10 truncates sub-second Durations
    // (see clippy.toml). Each step gets its own tick so the journal reads in
    // the order the actions were taken.
    let core_ptr: QPointer<DeltaChatCore> = QPointer::from(core_box.pinned().borrow());

    let chatmail = core_ptr.clone();
    single_shot(Duration::from_secs(1), move || {
        if let Some(this) = chatmail.as_pinned() {
            let provider = this.borrow_mut().default_provider_qr();
            this.borrow_mut()
                .create_profile(QString::from("Ada"), provider);
        }
    });

    let failing = core_ptr.clone();
    single_shot(Duration::from_secs(3), move || {
        if let Some(this) = failing.as_pinned() {
            // `fail` is what the double treats as an unreachable server.
            this.borrow_mut().create_profile(
                QString::from("Ada"),
                QString::from("dcaccount:fail.invalid"),
            );
        }
    });

    let email = core_ptr;
    single_shot(Duration::from_secs(5), move || {
        if let Some(this) = email.as_pinned() {
            this.borrow_mut().create_profile_with_email(
                QString::from("Grace"),
                QString::from("grace@example.org"),
                QString::from("hunter2"),
            );
        }
    });

    let engine_ptr = &engine as *const QmlEngine;
    single_shot(Duration::from_secs(7), move || {
        // SAFETY: see tests/smoke.rs -- the callback only fires while
        // `exec()` is still running on this thread.
        unsafe {
            (*engine_ptr).quit();
        }
    });

    engine.exec();

    let calls = read_journal(&journal);
    assert_current_transport_api(&calls);
    assert_chatmail_profile(&calls);
    assert_email_profile(&calls);
    assert_failed_attempt_left_no_orphan(&calls);

    // The other half of the contract: the signals QML binds to actually
    // fired, with the values it binds to. A journal cannot show this, and a
    // signal renamed or given different parameter names would sail past
    // every assertion above.
    let summary = QString::from_qvariant(engine.invoke_method("summary".into(), &[]))
        .map(|value| value.to_string())
        .unwrap_or_default();
    assert!(
        !summary.is_empty(),
        "the probe QML never loaded, so nothing was recorded. The usual \
         cause is a missing QtQuick runtime plugin (`qml-module-qtquick2` on \
         Debian/Ubuntu); Qt reports it as `module \"QtQuick\" is not \
         installed` above this line."
    );
    let fields: Vec<&str> = summary.split('/').collect();
    assert_eq!(
        fields.len(),
        5,
        "QML summary() returned something unexpected; the handler names may \
         not match the shim's signals. Got {summary:?}"
    );
    assert_eq!(
        fields[0], "2",
        "expected two successful profiles: {summary}"
    );
    assert_eq!(fields[1], "1", "expected one failed profile: {summary}");
    assert!(
        fields[2].parse::<u32>().unwrap_or(0) >= 2,
        "no ConfigureProgress reached QML: {summary}"
    );
    assert_eq!(
        fields[3], "1000",
        "the last progress report should be the core's 1000 = done: {summary}"
    );
}

/// The deprecated path must stay gone. This is the most valuable assertion
/// in the file: `set_config(mail_pw)` + `configure` is what the app used to
/// do, and it is what a careless revert would restore.
fn assert_current_transport_api(calls: &[Call]) {
    let names = methods(calls);
    assert!(
        !names.is_empty(),
        "the double recorded nothing; did the server fail to start?"
    );
    assert!(
        !names.contains(&"configure"),
        "onboarding called the deprecated `configure`; use \
         add_transport_from_qr / add_or_update_transport (docs/ONBOARDING.md). \
         Calls were: {names:?}"
    );
    for call in calls {
        if call.method == "set_config" {
            let key = call
                .params
                .get(1)
                .and_then(Value::as_str)
                .unwrap_or_default();
            assert_ne!(
                key, "mail_pw",
                "credentials were pushed through set_config; they belong in \
                 an EnteredLoginParam (docs/ONBOARDING.md)"
            );
        }
    }
}

/// A display name, then the provider payload -- and the default provider is
/// the one the reference client uses.
fn assert_chatmail_profile(calls: &[Call]) {
    let names = methods(calls);
    let display_name = calls
        .iter()
        .find(|call| {
            call.method == "set_config"
                && call.params.get(1).and_then(Value::as_str) == Some("displayname")
        })
        .expect("create_profile must set a display name");
    assert_eq!(
        display_name.params.get(2).and_then(Value::as_str),
        Some("Ada")
    );

    let from_qr = find(calls, "add_transport_from_qr")
        .expect("create_profile must go through add_transport_from_qr");
    assert_eq!(
        from_qr.params.get(1).and_then(Value::as_str),
        Some("dcaccount:nine.testrun.org"),
        "the default provider payload changed"
    );

    // The display name is set *before* the transport call, so it is in place
    // when the core announces the account to the network.
    let name_index = names.iter().position(|method| *method == "set_config");
    let transport_index = names
        .iter()
        .position(|method| *method == "add_transport_from_qr");
    assert!(
        name_index < transport_index,
        "display name must be set before the transport is added: {names:?}"
    );
}

/// Exactly the two required fields of `EnteredLoginParam`, as an object --
/// not positional arguments, not `set_config` calls.
fn assert_email_profile(calls: &[Call]) {
    let transport = find(calls, "add_or_update_transport")
        .expect("create_profile_with_email must go through add_or_update_transport");
    let param = transport.params.get(1).expect("param object");
    assert_eq!(
        param.get("addr").and_then(Value::as_str),
        Some("grace@example.org")
    );
    assert_eq!(
        param.get("password").and_then(Value::as_str),
        Some("hunter2")
    );
}

/// A failed attempt must not strand an account: the retry reuses the
/// unconfigured one rather than adding another.
fn assert_failed_attempt_left_no_orphan(calls: &[Call]) {
    let names = methods(calls);
    let added = names
        .iter()
        .filter(|method| **method == "add_account")
        .count();
    assert_eq!(
        added, 2,
        "expected one account for the chatmail profile and one for the email \
         profile -- the failed attempt should have reused the unconfigured \
         account, not added a third. Calls were: {names:?}"
    );
}

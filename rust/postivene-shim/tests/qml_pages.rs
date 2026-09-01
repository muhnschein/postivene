//! Loads the real onboarding pages and drives them headlessly.
//!
//! `Sailfish.Silica` ships only in the SDK, so `tests/silica-stubs/`
//! declares just enough of each component for the page files to instantiate
//! under host Qt. The stubs imitate no layout or behaviour: this says what a
//! page *does* -- which shim methods it calls, where it navigates, how it
//! reacts -- not how it looks.
//!
//! Interaction goes through `objectName`, which is production-safe, rather
//! than test hooks in the pages.

// Qt harness: needs `unsafe` for `env::set_var` before Qt starts
// (`unused_unsafe` because it is only unsafe from edition 2024 on),
// `borrow_as_ptr` for the engine pointer, and `single_shot` with
// whole-second Durations.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used,
    // qt_method! declarations must match the generated dispatcher's
    // by-value parameters; see postivene-shim/src/lib.rs.
    clippy::needless_pass_by_value
)]

use std::cell::RefCell;
use std::ffi::CString;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use postivene_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

use common::PageStackProbe;

/// QML forbids capitalised property names, so a `.qml` stub cannot provide
/// `BusyIndicatorSize.Large`; a registered enum can.
#[derive(QEnum)]
#[repr(u8)]
enum BusyIndicatorSize {
    Small = 0,
    Medium = 1,
    Large = 2,
}

/// Same, for `TruncationMode.Fade`.
#[derive(QEnum)]
#[repr(u8)]
enum TruncationMode {
    Elide = 0,
    Fade = 1,
}

/// Owns a `Loader` and walks the loaded page's children by `objectName`.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        id: root
        Loader { id: loader }

        function load(url) {
            loader.source = url
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            // List delegates hang off the view's contentItem, not its
            // children.
            if (node.contentItem && node.contentItem !== node) {
                return findIn(node.contentItem, name)
            }
            return null
        }
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        function setText(name, value) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.text = value
            return 'ok'
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function pageProperty(property) {
            return loader.item ? '' + loader.item[property] : 'no-page'
        }
    }
";

fn qml_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml")
}

fn stubs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/silica-stubs")
}

fn page_url(name: &str) -> String {
    format!("file://{}", qml_dir().join("pages").join(name).display())
}

/// Each step and what it recorded. One step per tick: the shim answers
/// asynchronously and `single_shot` only handles whole seconds.
type Steps = Rc<RefCell<Vec<(String, String)>>>;

fn record(steps: &Steps, label: &str, value: QString) {
    steps
        .borrow_mut()
        .push((label.to_string(), value.to_string()));
}

fn value_of<'a>(steps: &'a [(String, String)], label: &str) -> &'a str {
    steps
        .iter()
        .find(|(name, _)| name == label)
        .map_or("<step did not run>", |(_, value)| value.as_str())
}

/// Temp directories and environment, returning the journal path.
fn prepare_environment() -> PathBuf {
    let temp = std::env::temp_dir().join(format!("postivene-qml-pages-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    // SAFETY: single-threaded, and set before Qt starts and before the
    // server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_FAKE_JOURNAL", &journal);
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
    }
    journal
}

// The engine, the QObject boxes and every step share one scope: all must
// outlive `exec()`. The assertions are in the helpers below.
#[allow(clippy::too_many_lines)]
#[test]
fn onboarding_pages_drive_the_core_and_navigate() {
    let journal = prepare_environment();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let stack_box = QObjectBox::new(PageStackProbe::default());

    let mut engine = QmlEngine::new();
    // Without this the page files do not parse their imports.
    engine.add_import_path(QString::from(stubs_dir().to_string_lossy().into_owned()));
    // The two enum namespaces the .qml stubs cannot express.
    let uri = CString::new("Sailfish.Silica").expect("static uri");
    qml_register_enum::<BusyIndicatorSize>(
        &uri,
        1,
        0,
        &CString::new("BusyIndicatorSize").expect("static name"),
    );
    qml_register_enum::<TruncationMode>(
        &uri,
        1,
        0,
        &CString::new("TruncationMode").expect("static name"),
    );
    engine.set_object_property("core".into(), core_box.pinned());
    engine.set_object_property("pageStack".into(), stack_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let steps: Steps = Rc::new(RefCell::new(Vec::new()));
    let engine_ptr = std::ptr::addr_of_mut!(engine);

    // SAFETY: these callbacks fire only while `exec()` is running on this
    // thread, and `engine` outlives it.
    macro_rules! call {
        ($name:expr $(, $arg:expr)*) => {{
            let result = unsafe {
                (*engine_ptr).invoke_method(
                    $name.into(),
                    &[$(QVariant::from(QString::from($arg))),*],
                )
            };
            QString::from_qvariant(result).unwrap_or_default()
        }};
    }

    // One step per tick; whole seconds only (clippy.toml).
    let s = steps.clone();
    single_shot(Duration::from_secs(1), move || {
        record(
            &s,
            "welcome-load",
            call!("load", page_url("WelcomePage.qml")),
        );
    });

    let s = steps.clone();
    single_shot(Duration::from_secs(2), move || {
        record(&s, "welcome-probing", call!("pageProperty", "probing"));
        record(&s, "welcome-click", call!("click", "createProfileButton"));
    });

    let s = steps.clone();
    single_shot(Duration::from_secs(3), move || {
        record(
            &s,
            "create-load",
            call!("load", page_url("CreateProfilePage.qml")),
        );
        record(&s, "create-provider", call!("pageProperty", "providerQr"));
        record(&s, "create-name", call!("setText", "nameField", "Ada"));
    });

    let s = steps.clone();
    single_shot(Duration::from_secs(4), move || {
        record(&s, "create-click", call!("click", "createButton"));
    });

    let s = steps.clone();
    single_shot(Duration::from_secs(6), move || {
        record(&s, "create-permille", call!("pageProperty", "permille"));
        record(&s, "create-error", call!("pageProperty", "errorMessage"));
        record(
            &s,
            "email-load",
            call!("load", page_url("EmailLoginPage.qml")),
        );
    });

    let s = steps.clone();
    single_shot(Duration::from_secs(7), move || {
        // The page must refuse locally rather than call the core.
        record(&s, "email-click-empty", call!("click", "loginButton"));
    });

    let s = steps.clone();
    single_shot(Duration::from_secs(8), move || {
        record(&s, "email-error", call!("pageProperty", "errorMessage"));
        record(
            &s,
            "email-error-shown",
            call!("get", "errorLabel", "visible"),
        );
    });

    single_shot(Duration::from_secs(9), move || unsafe {
        (*engine_ptr).quit();
    });

    engine.exec();

    let steps = steps.borrow();
    let navigation = stack_box.pinned().borrow().log.to_string();
    let properties = stack_box.pinned().borrow().last_properties.to_string();
    let context = format!("steps: {steps:?}\nnavigation: {navigation}");

    assert_pages_loaded(&steps, &context);
    assert_welcome_and_navigation(&steps, &navigation, &properties, &context);
    assert_profile_creation(&steps, &context);
    assert_email_validation(&steps, &context);
    assert_wire_calls(&journal);
}

/// Every page file must instantiate; nothing below means anything if not.
fn assert_pages_loaded(steps: &[(String, String)], context: &str) {
    for step in ["welcome-load", "create-load", "email-load"] {
        assert_eq!(value_of(steps, step), "ok", "{step} failed. {context}");
    }
}

/// With no configured account the welcome page stops probing and shows its
/// buttons, and its button opens the profile page.
fn assert_welcome_and_navigation(
    steps: &[(String, String)],
    navigation: &str,
    properties: &str,
    context: &str,
) {
    assert_eq!(
        value_of(steps, "welcome-probing"),
        "false",
        "the welcome page never stopped probing. {context}"
    );
    assert_eq!(value_of(steps, "welcome-click"), "ok", "{context}");
    assert!(
        navigation.contains("push:CreateProfilePage.qml"),
        "Create New Profile did not open the profile page. {context}"
    );
    assert!(
        navigation.contains("replaceAbove:ChatListPage.qml"),
        "a created profile did not land in the chat list. {context}"
    );
    assert!(
        properties.contains("accountId="),
        "the chat list was opened without an account id: {properties}"
    );
}

/// The provider comes from the shim, and progress reaches the page.
fn assert_profile_creation(steps: &[(String, String)], context: &str) {
    assert_eq!(
        value_of(steps, "create-provider"),
        "dcaccount:nine.testrun.org",
        "{context}"
    );
    assert_eq!(
        value_of(steps, "create-permille"),
        "1000",
        "ConfigureProgress never reached the page. {context}"
    );
    assert_eq!(
        value_of(steps, "create-error"),
        "",
        "a successful profile left an error on the page. {context}"
    );
}

/// An empty login form: a visible message, no RPC.
fn assert_email_validation(steps: &[(String, String)], context: &str) {
    assert!(
        !value_of(steps, "email-error").is_empty(),
        "submitting an empty login form reported nothing. {context}"
    );
    assert_eq!(
        value_of(steps, "email-error-shown"),
        "true",
        "the error message is set but not shown. {context}"
    );
}

/// A tap produced the right calls on the wire.
fn assert_wire_calls(journal: &std::path::Path) {
    let calls = common::records(journal);
    let method_names: Vec<&str> = calls
        .iter()
        .filter_map(|call| call.get("method").and_then(Value::as_str))
        .collect();
    assert!(
        method_names.contains(&"add_transport_from_qr"),
        "tapping create did not reach add_transport_from_qr: {method_names:?}"
    );
    assert!(
        !method_names.contains(&"configure"),
        "the pages called the deprecated `configure`: {method_names:?}"
    );
    assert!(
        !method_names.contains(&"add_or_update_transport"),
        "the empty login form was sent to the core anyway: {method_names:?}"
    );
    let display_name = calls.iter().find(|call| {
        call.get("method").and_then(Value::as_str) == Some("set_config")
            && call.pointer("/params/1").and_then(Value::as_str) == Some("displayname")
    });
    assert_eq!(
        display_name
            .and_then(|call| call.pointer("/params/2"))
            .and_then(Value::as_str),
        Some("Ada"),
        "the name typed into the page did not reach the core"
    );
}

//! Loads the real onboarding pages and drives them, headlessly.
//!
//! Sailfish's `Sailfish.Silica` module ships only inside the Sailfish SDK,
//! so until now nothing in this repository could load a page file at all --
//! the pages were checked by `qmllint` for syntax and read by eye for
//! everything else. That is how two device-only bugs shipped (a window that
//! was never shown, signal handlers that never connected), and it is why
//! `tests/silica-stubs/` exists: a minimal `Sailfish.Silica` module,
//! declaring just enough of each component for the page files to
//! instantiate under host Qt.
//!
//! What the stubs deliberately do *not* do is imitate Silica's behaviour or
//! layout. Nothing here can tell you a page looks right. What it can tell
//! you is what the page *does*: which shim methods it calls, with which
//! arguments, what it navigates to, and how it reacts to the core's
//! answers. That is the logic that used to be verifiable only by flashing a
//! phone.
//!
//! Interaction goes through `objectName`, which the pages set on the
//! controls a test needs to reach. That is production-safe (Silica and
//! accessibility tools use it too), unlike exposing test hooks.

// See tests/smoke.rs for why this Qt harness needs the first three allows;
// `expect_used` covers the whole file because the helpers below are test
// code too.
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

/// Silica's `BusyIndicatorSize` is a C++ enum namespace, and QML forbids
/// property names starting with a capital letter, so a `.qml` stub cannot
/// provide `BusyIndicatorSize.Large`. Registering a real enum from here can.
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

/// A stand-in for Silica's `pageStack`, which is a context property in the
/// app and therefore has to be one here too (a QML object declared in the
/// probe would not be visible inside a separately loaded page component).
///
/// It records rather than navigates: what a test wants to know is that
/// tapping *this* went to *that* page with *those* properties.
#[derive(QObject, Default)]
struct PageStackProbe {
    base: qt_base_class!(trait QObject),
    /// `push:CreateProfilePage.qml|replace:ChatListPage.qml|...`
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),
    /// The properties handed to the most recent push/replace, as JSON.
    last_properties: qt_property!(QString; NOTIFY log_changed),

    push: qt_method!(fn(&mut self, page: QString, properties: QVariantMap)),
    replace: qt_method!(fn(&mut self, page: QString, properties: QVariantMap)),
    pop: qt_method!(fn(&mut self)),
}

impl PageStackProbe {
    fn record(&mut self, action: &str, page: &QString, properties: &QVariantMap) {
        // Only the file name matters to a test; the rest of the URL is the
        // absolute path of whatever checkout this is.
        let page = page.to_string();
        let name = page.rsplit('/').next().unwrap_or(&page).to_string();
        let current = self.log.to_string();
        self.log = format!("{current}{action}:{name}|").into();
        // `QVariantMap` in qttypes 0.2 cannot be iterated, only queried, so
        // the probe reads the keys the pages actually pass. Adding a new one
        // here is the price of asserting on it -- which is fine: a test that
        // checks a property nobody passes is not a test.
        let mut rendered = Vec::new();
        for key in ["accountId", "chatId", "chatName"] {
            let value = properties.value(QString::from(key), QVariant::default());
            let text = i32::from_qvariant(value.clone())
                .map(|number| number.to_string())
                .or_else(|| QString::from_qvariant(value).map(|text| text.to_string()));
            if let Some(text) = text {
                rendered.push(format!("{key}={text}"));
            }
        }
        self.last_properties = rendered.join(",").into();
        self.log_changed();
    }

    fn push(&mut self, page: QString, properties: QVariantMap) {
        self.record("push", &page, &properties);
    }

    fn replace(&mut self, page: QString, properties: QVariantMap) {
        self.record("replace", &page, &properties);
    }

    fn pop(&mut self) {
        let current = self.log.to_string();
        self.log = format!("{current}pop|").into();
        self.log_changed();
    }
}

/// The harness object the test drives the pages through. It owns a `Loader`
/// and walks the loaded page's children by `objectName`.
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

/// Every step the test takes, and what it recorded. Steps run one per
/// event-loop tick because the shim answers asynchronously and
/// `qmetaobject`'s `single_shot` only handles whole seconds.
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

/// Temp directories and the three environment variables the harness needs,
/// returning the journal path the double will write to.
fn prepare_environment() -> PathBuf {
    let temp = std::env::temp_dir().join(format!("postivene-qml-pages-{}", std::process::id()));
    let journal = temp.join("journal.jsonl");
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    // SAFETY: single-threaded test binary; all three must be set before Qt
    // starts and before the shim spawns the server that inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_FAKE_JOURNAL", &journal);
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
    }
    journal
}

// The engine, the two QObject boxes and every scheduled step have to share
// one scope: all of them must outlive `exec()`, and each step needs the
// engine pointer. The assertions are already factored out into the helpers
// below; what is left is the event loop and its schedule, which does not
// decompose further without making it harder to read.
#[allow(clippy::too_many_lines)]
#[test]
fn onboarding_pages_drive_the_core_and_navigate() {
    let journal = prepare_environment();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let stack_box = QObjectBox::new(PageStackProbe::default());

    let mut engine = QmlEngine::new();
    // The stub Sailfish.Silica module. Without this the page files do not
    // even parse their imports.
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

    // SAFETY for every block below: these callbacks only fire while
    // `engine.exec()` is running on this same thread, and `engine` is not
    // dropped until after `exec()` returns. Same argument as tests/smoke.rs.
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

    // One step per tick: the shim answers asynchronously, and
    // qmetaobject 0.2.10 only schedules whole seconds (see clippy.toml).
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
        // Nothing filled in: the page must refuse locally rather than
        // sending an empty address to the core.
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

/// Every page file must instantiate. A missing stub or a typo shows up here
/// first, and nothing else would mean anything without it.
fn assert_pages_loaded(steps: &[(String, String)], context: &str) {
    for step in ["welcome-load", "create-load", "email-load"] {
        assert_eq!(value_of(steps, step), "ok", "{step} failed. {context}");
    }
}

/// The welcome page settles -- no configured account exists in the double,
/// so it stops probing and shows its buttons rather than spinning forever,
/// which is what the old setup page did on device -- and its button opens
/// the profile page, which on success lands in the chat list.
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
        navigation.contains("replace:ChatListPage.qml"),
        "a created profile did not land in the chat list. {context}"
    );
    assert!(
        properties.contains("accountId="),
        "the chat list was opened without an account id: {properties}"
    );
}

/// The page takes its provider from the shim rather than hardcoding a
/// hostname, and the core's progress reaches it.
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

/// An empty login form must produce a visible message and no RPC.
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

/// And the whole way down: a tap produced the right calls on the wire.
fn assert_wire_calls(journal: &std::path::Path) {
    let calls: Vec<Value> = std::fs::read_to_string(journal)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect();
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

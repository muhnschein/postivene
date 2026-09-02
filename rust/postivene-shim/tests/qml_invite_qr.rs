//! The invite page draws the account's invite as a code, and offers the
//! scanner beside the paste field.
//!
//! Loaded headlessly against the stub Silica module and the recording
//! double: once the invite arrives the code has modules and the picture is
//! shown from a file; the scan button pushes the scanner page, and what
//! it hands back is followed the way a pasted link is, into a chat that
//! replaces the scanner and this page both.

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
    // by-value parameters, and the derive transmutes a QVariant field;
    // see postivene-shim/src/lib.rs.
    clippy::needless_pass_by_value,
    clippy::useless_transmute
)]

use std::time::Duration;

use postivene_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// Silica's `pageStack`, recorded rather than performed. `push` hands back
/// a stand-in the probe QML defines -- something with a `scanned` signal
/// -- so the page connects to it the way it connects to the real scanner,
/// and the test fires it from QML.
///
/// Method names are camelCase because they stand in for Silica's own API.
#[allow(non_snake_case)]
#[derive(QObject, Default)]
struct PageStackProbe {
    base: qt_base_class!(trait QObject),
    /// `push:ScanPage.qml|replaceAbove:ConversationPage.qml|...`
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),
    /// The stand-in, as QML handed it over.
    stand_in: QVariant,

    push: qt_method!(fn(&mut self, page: QString) -> QVariant),
    replaceAbove:
        qt_method!(fn(&mut self, target: QVariant, page: QString, properties: QVariantMap)),
    previousPage: qt_method!(fn(&mut self, page: QVariant) -> QVariant),
}

#[allow(non_snake_case)]
impl PageStackProbe {
    fn note(&mut self, action: &str, page: &QString) {
        let page = page.to_string();
        let name = page.rsplit('/').next().unwrap_or(&page).to_string();
        let current = self.log.to_string();
        self.log = format!("{current}{action}:{name}|").into();
        self.log_changed();
    }

    fn push(&mut self, page: QString) -> QVariant {
        self.note("push", &page);
        self.stand_in.clone()
    }

    fn replaceAbove(&mut self, _target: QVariant, page: QString, _properties: QVariantMap) {
        self.note("replaceAbove", &page);
    }

    /// The page below, which this record does not model. `&mut self` is
    /// what `qt_method!` dispatches to.
    #[allow(clippy::unused_self)]
    fn previousPage(&mut self, _page: QVariant) -> QVariant {
        QVariant::default()
    }
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }
        // What a pushed scanner page is to the invite page: something
        // with a `scanned` signal to connect to.
        QtObject {
            id: fakeScanner
            signal scanned(string text)
        }
        function standIn() { return fakeScanner }
        function fire(text) { fakeScanner.scanned(text); return 'ok' }
        function load(url) {
            loader.setSource(url, { accountId: 1 })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            if (node.contentItem && node.contentItem !== node) {
                return findIn(node.contentItem, name)
            }
            return null
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
    }
";

#[test]
fn the_invite_is_drawn_as_a_code_and_a_scanned_one_is_followed() {
    let temp = std::env::temp_dir().join(format!("postivene-invite-qr-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_FAKE_JOURNAL", &journal);
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
    }

    postivene_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let stack_box = QObjectBox::new(PageStackProbe::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.set_object_property("pageStack".into(), stack_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));
    // The stand-in the probe defined, for `push` to hand back.
    let stand_in = engine.invoke_method("standIn".into(), &[]);
    stack_box.pinned().borrow_mut().stand_in = stand_in;

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let stack_ptr: *const QObjectBox<PageStackProbe> = std::ptr::addr_of!(stack_box);
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

    single_shot(Duration::from_secs(2), move || unsafe {
        record!(
            "load",
            call!("load", QString::from(common::page_url("InvitePage.qml")))
        );
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        // The invite has arrived from the core and is drawn.
        record!("invite", get!("myInviteLabel", "text"));
        record!("code-size", get!("qr", "size"));
        record!("code-shown", get!("inviteQr", "visible"));
        record!("code-image", get!("inviteQr", "source"));
        // Scanning: the page pushes the scanner and follows what it says.
        record!("scan", call!("click", QString::from("scanButton")));
        record!(
            "fire",
            call!(
                "fire",
                QString::from("https://i.delta.chat/#FEDCBA&a=them%40example.org")
            )
        );
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        record!("navigation", (*stack_ptr).pinned().borrow().log.to_string());
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_page(&steps, &common::methods(&journal));
}

/// The code is the invite; the scanner is pushed, and its answer is
/// followed into a chat.
fn assert_page(steps: &[(&str, String)], calls: &[String]) {
    let context = format!("steps: {steps:?}\ncalls: {calls:?}");
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };

    assert_eq!(
        value("load"),
        "ok",
        "the invite page did not load. {context}"
    );
    assert!(
        value("invite").starts_with("https://i.delta.chat/"),
        "the invite never arrived. {context}"
    );
    let size: u32 = value("code-size").parse().unwrap_or(0);
    assert!(
        size >= 21,
        "the invite was not encoded as a code of at least version 1. {context}"
    );
    assert_eq!(
        value("code-shown"),
        "true",
        "the code is not shown once there is one. {context}"
    );
    let image = value("code-image");
    assert!(
        image.starts_with("file://")
            && std::path::Path::new(&image)
                .extension()
                .is_some_and(|ext| ext == "pgm"),
        "the code is not shown from a file the shim wrote: {image:?}. {context}"
    );
    assert_eq!(value("scan"), "ok", "the scan button is missing. {context}");
    assert!(
        value("navigation").starts_with("push:ScanPage.qml|"),
        "the scan button did not push the scanner. {context}"
    );
    assert!(
        calls.iter().any(|name| name == "secure_join"),
        "the scanned invite was not followed. {context}"
    );
    // Above the page below this one: the scanner is still up, and goes
    // with it. Not a pop and a replace, which Silica would drop the second
    // of, mid-transition.
    assert!(
        value("navigation").ends_with("replaceAbove:ConversationPage.qml|"),
        "following the scanned invite did not open the chat over both pages. {context}"
    );
}

//! The page a webxdc app runs on: what it points the view at, and what it
//! says while there is nothing to point it at yet.
//!
//! The view itself is a stub (`tests/silica-stubs`), because the browser
//! engine is a device question this cannot answer. What it *can* answer is
//! everything around it: that the page starts the app, hands the view the
//! address the shim served it on, names the app in its own header, offers
//! the source only when the app has one, and stops serving on the way out.

// Qt harness: see qml_pages.rs.
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

/// Silica's `pageStack`, recorded rather than performed: the page pops
/// itself when the message it is running is deleted.
#[derive(QObject, Default)]
struct StackProbe {
    base: qt_base_class!(trait QObject),
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),
    pop: qt_method!(fn(&mut self)),
}

impl StackProbe {
    fn pop(&mut self) {
        let current = self.log.to_string();
        self.log = format!("{current}pop;").into();
        self.log_changed();
    }
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }
        function load(url, accountId, messageId) {
            loader.setSource('', {})
            loader.setSource(url, {
                accountId: accountId,
                messageId: messageId,
                appName: 'Checkers'
            })
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
        // The core says the message is gone, which is the one thing that
        // takes the page away by itself. Built with JSON.stringify so the
        // payload is quoted the way the shim's own events are.
        function deleted() {
            var app = findIn(loader.item, 'app')
            if (!app) { return 'missing:app' }
            app.handle_event(1, 'WebxdcInstanceDeleted',
                             JSON.stringify({ msgId: 5 }))
            return 'ok'
        }
        // The view saying it has arrived, which is the engine's job and
        // the one thing a stub cannot do by itself.
        function arrived() {
            var view = findIn(loader.item, 'webxdcView')
            if (!view) { return 'missing:webxdcView' }
            view.loadProgress = 100
            view.loaded = true
            return 'ok'
        }
        // An app trying to take the view somewhere else, which is what
        // window.location does.
        function wander(where) {
            var view = findIn(loader.item, 'webxdcView')
            if (!view) { return 'missing:webxdcView' }
            view.url = where
            return '' + view.url
        }
        // Leaving the page, as the stack does on the way out.
        function leave() {
            loader.item.status = PageStatus.Deactivating
            return 'ok'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_page_runs_the_app_the_shim_serves_and_stops_it_on_the_way_out() {
    let temp = std::env::temp_dir().join(format!("postivene-webxdc-page-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_FAKE_JOURNAL", &journal);
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("XDG_CACHE_HOME", temp.join("cache"));
    }

    postivene_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let stack_box = QObjectBox::new(StackProbe::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.set_object_property("pageStack".into(), stack_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let stack_ptr = std::ptr::addr_of!(stack_box);
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
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    single_shot(Duration::from_secs(2), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::page_url("WebxdcPage.qml")),
                1,
                5
            )
        );
        // Before the host has answered: the view has nowhere to go and
        // the page says it is working on it.
        record!(
            "early-url",
            call!("get", QString::from("webxdcView"), QString::from("url"))
        );
        record!(
            "early-busy",
            call!("get", QString::from("webxdcBusy"), QString::from("running"))
        );
        record!(
            "early-title",
            call!("get", QString::from("webxdcHeader"), QString::from("title"))
        );
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!(
            "url",
            call!("get", QString::from("webxdcView"), QString::from("url"))
        );
        // Served, but not drawn yet: the page is still saying so, and
        // saying what it is waiting on.
        record!(
            "serving-busy",
            call!("get", QString::from("webxdcBusy"), QString::from("running"))
        );
        record!(
            "serving-said",
            call!("get", QString::from("webxdcWaiting"), QString::from("text"))
        );
        record!(
            "polling",
            call!("get", QString::from("webxdcPoll"), QString::from("running"))
        );
        // The engine says it has the app.
        record!("arrived", call!("arrived"));
        record!(
            "busy",
            call!("get", QString::from("webxdcBusy"), QString::from("running"))
        );
        record!(
            "said",
            call!(
                "get",
                QString::from("webxdcWaiting"),
                QString::from("visible")
            )
        );
        record!(
            "still-polling",
            call!("get", QString::from("webxdcPoll"), QString::from("running"))
        );
        record!(
            "title",
            call!("get", QString::from("webxdcHeader"), QString::from("title"))
        );
        record!(
            "banner",
            call!("get", QString::from("errorBanner"), QString::from("text"))
        );
        // Off its own address, and back where it belongs.
        record!(
            "wander",
            call!("wander", QString::from("https://example.org/pay"))
        );
        record!("leave", call!("leave"));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        // Left: nothing is being served any more.
        record!(
            "after-leave",
            call!("get", QString::from("webxdcView"), QString::from("url"))
        );
        record!("deleted", call!("deleted"));
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("popped", (*stack_ptr).pinned().borrow().log.to_string());
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

    assert_eq!(value("load"), "ok", "the page did not load. {context}");
    assert_eq!(
        value("early-url"),
        "",
        "the view was pointed somewhere before the app was served. {context}"
    );
    assert_eq!(
        value("early-busy"),
        "true",
        "the page said nothing while the app was coming up. {context}"
    );
    assert_eq!(
        value("early-title"),
        "Checkers",
        "the page did not carry over the name the row already knew. {context}"
    );

    assert!(
        value("url").starts_with("http://127.0.0.1:"),
        "the view was not pointed at the app the shim serves: {}. {context}",
        value("url")
    );
    assert!(
        value("url").ends_with("/index.html"),
        "the view was not pointed at the app's own page: {}. {context}",
        value("url")
    );
    assert_eq!(
        value("serving-busy"),
        "true",
        "the page stopped saying it was working while the app had still \
         drawn nothing. {context}"
    );
    // The three things a phone cannot otherwise be asked: where the app
    // is, how far the engine got, and how much of the app it asked for.
    assert!(
        value("serving-said").starts_with("127.0.0.1:")
            && value("serving-said").ends_with("· 0% · 0"),
        "the page did not say what it was waiting on: {}. {context}",
        value("serving-said")
    );
    assert_eq!(
        value("polling"),
        "true",
        "nothing was reading how much of the app had been asked for. \
         {context}"
    );
    assert_eq!(value("arrived"), "ok", "the view did not arrive. {context}");
    assert_eq!(
        value("busy"),
        "false",
        "the page is still saying it is working after the app came up. {context}"
    );
    assert_eq!(
        value("said"),
        "false",
        "the page is still saying what it is waiting for after the app \
         came up. {context}"
    );
    assert_eq!(
        value("still-polling"),
        "false",
        "a running app is still being asked how it is getting on. {context}"
    );
    assert_eq!(
        value("title"),
        "Checkers",
        "the header does not name the app the core says it is. {context}"
    );
    assert_eq!(
        value("banner"),
        "",
        "the page reported an error: {}. {context}",
        value("banner")
    );

    assert!(
        value("wander").starts_with("http://127.0.0.1:"),
        "an app navigated itself off its own address and stayed there: {}. \
         {context}",
        value("wander")
    );

    assert_eq!(
        value("after-leave"),
        "",
        "the app is still being served after the page was left. {context}"
    );
    assert_eq!(
        value("popped"),
        "pop;",
        "a page running an app that has been deleted stayed open. {context}"
    );
}

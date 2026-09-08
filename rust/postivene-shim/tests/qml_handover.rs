//! What is offered for a file a webxdc app has handed over.
//!
//! The webxdc call is `sendToChat` and a chat is the only destination its
//! name can carry. But an app draws a *download* over it -- `sharer`'s is
//! a download arrow -- and what a reader means by that is the file, on
//! their phone. So the two answers are opening it and keeping it, and a
//! chat is not among them.
//!
//! The page that raises this names `Sailfish.WebView` and never loads off
//! a device; the dialog it pushes does, so what the reader is offered can
//! be checked here.

// Qt harness: see qml_reactions.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        width: 540
        height: 960

        property string chosen: ''

        Loader { id: dialog; width: 540; height: 960 }
        function load(url, name) {
            dialog.setSource(url, {
                filePath: '/tmp/postivene-handover/webxdc/outbox/5/' + name,
                fileName: name
            })
            if (dialog.status !== Loader.Ready) { return 'load-failed' }
            dialog.item.openChosen.connect(function () {
                chosen += 'open;'
            })
            dialog.item.saveChosen.connect(function () {
                chosen += 'save;'
            })
            return 'ok'
        }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            return null
        }
        function get(name, property) {
            var item = findIn(dialog.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function pick(name) {
            var item = findIn(dialog.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        function picked() { return chosen }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_file_an_app_produced_can_be_opened_or_kept() {
    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
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

    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::page_url("HandoverDialog.qml")),
                // A name an app chose, with markup in it: a Label that
                // drew this as anything but text would be taking an
                // app's word for its own formatting.
                QString::from("<b>report</b>.pdf")
            )
        );
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        record!(
            "name",
            call!("get", QString::from("handoverName"), QString::from("text"))
        );
        record!(
            "name-format",
            call!(
                "get",
                QString::from("handoverName"),
                QString::from("textFormat")
            )
        );
        record!(
            "open-there",
            call!(
                "get",
                QString::from("handoverOpen"),
                QString::from("visible")
            )
        );
        record!(
            "save-there",
            call!(
                "get",
                QString::from("handoverSave"),
                QString::from("visible")
            )
        );
        record!("open", call!("pick", QString::from("handoverOpen")));
        record!("save", call!("pick", QString::from("handoverSave")));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("picked", call!("picked"));
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

    assert_eq!(value("load"), "ok", "the dialog did not load. {context}");
    assert_eq!(
        value("name"),
        "<b>report</b>.pdf",
        "the dialog does not say which file it is asking about. {context}"
    );
    assert_eq!(
        value("name-format"),
        "0",
        "the file's name is not drawn as plain text, so an app can put \
         markup in the name of the file it hands over and have it drawn. \
         {context}"
    );
    assert_eq!(
        (value("open-there"), value("save-there")),
        ("true".to_string(), "true".to_string()),
        "the two answers a download means are not both on offer. {context}"
    );
    assert_eq!(
        value("picked"),
        "open;save;",
        "picking one of them did not say which was picked, so the page \
         has nothing to act on. {context}"
    );
}

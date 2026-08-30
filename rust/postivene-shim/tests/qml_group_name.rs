//! Naming a group has to reach the page that creates it.
//!
//! `nameField` was declared inside the list's `header`, which is a
//! Component property and therefore its own scope. Every reference to it
//! from the page -- the Create button's `enabled`, the summary row, and
//! the `create_group` call itself -- was reading an undefined name. The
//! button could never enable, so a group could not be created at all.

// Qt harness: see qml_chat_list.rs.
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

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        Loader { id: loader }
        function load(url, accountId) {
            loader.setSource('', {})
            loader.setSource(url, { accountId: accountId })
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
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn typing_a_group_name_enables_creating_the_group() {
    let temp = std::env::temp_dir().join(format!("postivene-group-name-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
    }

    postivene_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

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

    single_shot(Duration::from_secs(1), move || unsafe {
        (*steps_ptr).push((
            "load",
            call!(
                "load",
                QString::from(common::page_url("NewGroupPage.qml")),
                1
            ),
        ));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        // Nothing typed: creating must be refused.
        (*steps_ptr).push((
            "empty",
            call!(
                "get",
                QString::from("createButton"),
                QString::from("enabled")
            ),
        ));
        (*steps_ptr).push((
            "typed",
            call!(
                "setText",
                QString::from("nameField"),
                QString::from("Walking group")
            ),
        ));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        (*steps_ptr).push((
            "named",
            call!(
                "get",
                QString::from("createButton"),
                QString::from("enabled")
            ),
        ));
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

    assert_eq!(
        value("load"),
        "ok",
        "the group page did not load. {context}"
    );
    assert_eq!(
        value("typed"),
        "ok",
        "the name field was not reachable. {context}"
    );
    assert_eq!(
        value("empty"),
        "false",
        "an unnamed group could be created. {context}"
    );
    assert_eq!(
        value("named"),
        "true",
        "typing a name did not reach the page, so the group can never be \
         created. {context}"
    );
}

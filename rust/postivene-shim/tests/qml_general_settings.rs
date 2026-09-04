//! The settings page, and the object every page reads the settings
//! through.
//!
//! Each control on the page writes one value of the `Settings` singleton,
//! and the choice it shows follows the value back -- so a change made
//! anywhere reaches the page, and a change made on the page reaches the
//! conversation. Both are loaded here against a stub `ConfigurationValue`
//! that holds a value and stores nothing.

// Qt harness: see qml_pages.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::path::PathBuf;
use std::time::Duration;

use qmetaobject::*;

mod common;

/// The probe imports the app's components by absolute URL: it is loaded
/// from data, which has no directory of its own to resolve one against.
fn probe_qml() -> String {
    let components = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components");
    format!(
        r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import 'file://{}'
    Item {{
        Loader {{ id: loader }}
        function load(url) {{
            loader.setSource(url, {{}})
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }}
        function findIn(node, name) {{
            if (!node) {{ return null }}
            if (node.objectName === name) {{ return node }}
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {{
                var hit = findIn(kids[i], name)
                if (hit) {{ return hit }}
            }}
            if (node.contentItem && node.contentItem !== node) {{
                return findIn(node.contentItem, name)
            }}
            // A ComboBox's items live in its menu, which is not among
            // its children.
            if (node.menu) {{
                var inMenu = findIn(node.menu, name)
                if (inMenu) {{ return inMenu }}
            }}
            return null
        }}
        function get(name, property) {{
            var item = findIn(loader.item, name)
            if (!item) {{ return 'missing:' + name }}
            return '' + item[property]
        }}
        function click(name) {{
            var item = findIn(loader.item, name)
            if (!item) {{ return 'missing:' + name }}
            item.clicked()
            return 'ok'
        }}
        // The app's side of the same keys.
        function appReads(name) {{ return '' + Settings[name] }}
        function appWrites(name, value) {{ Settings[name] = value; return 'ok' }}
        function appKey(name) {{ return '' + Settings[name].key }}
    }}
",
        components.display()
    )
}

#[test]
#[allow(clippy::too_many_lines)]
fn the_settings_page_writes_what_the_app_reads() {
    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(probe_qml()));

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

    single_shot(Duration::from_secs(1), move || unsafe {
        // The keys, under the app's own path.
        record!(
            "app-markdown-key",
            call!("appKey", QString::from("markdownConfig"))
        );
        record!(
            "app-links-key",
            call!("appKey", QString::from("cleanLinksConfig"))
        );
        record!(
            "app-download-key",
            call!("appKey", QString::from("downloadLimitConfig"))
        );
        record!(
            "app-notification-key",
            call!("appKey", QString::from("notificationDetailConfig"))
        );
        record!(
            "load",
            call!("load", QString::from(common::page_url("SettingsPage.qml")))
        );
        // What a fresh phone shows.
        record!(
            "markdown-default",
            call!("appReads", QString::from("markdownMode"))
        );
        record!("markdown-index", get!("markdownCombo", "currentIndex"));
        record!(
            "download-default",
            call!("appReads", QString::from("downloadLimit"))
        );
        record!("download-index", get!("downloadCombo", "currentIndex"));
        record!(
            "links-default",
            call!("appReads", QString::from("cleanLinks"))
        );
        record!("links-switch", get!("cleanLinksSwitch", "checked"));
        record!(
            "notification-default",
            call!("appReads", QString::from("notificationDetail"))
        );
        record!(
            "notification-index",
            get!("notificationCombo", "currentIndex")
        );
        // Each control writes its setting, and the choice shown follows it.
        record!(
            "pick-markdown",
            call!("click", QString::from("markdownOption1"))
        );
        record!(
            "markdown-picked",
            call!("appReads", QString::from("markdownMode"))
        );
        record!("markdown-shown", get!("markdownCombo", "currentIndex"));
        record!(
            "pick-download",
            call!("click", QString::from("downloadOption32768"))
        );
        record!(
            "download-picked",
            call!("appReads", QString::from("downloadLimit"))
        );
        record!("download-shown", get!("downloadCombo", "currentIndex"));
        record!(
            "pick-always",
            call!("click", QString::from("downloadOption0"))
        );
        record!(
            "always-picked",
            call!("appReads", QString::from("downloadLimit"))
        );
        record!("always-shown", get!("downloadCombo", "currentIndex"));
        record!(
            "flip-links",
            call!("click", QString::from("cleanLinksSwitch"))
        );
        record!("links-on", call!("appReads", QString::from("cleanLinks")));
        record!("links-switch-on", get!("cleanLinksSwitch", "checked"));
        record!(
            "flip-back",
            call!("click", QString::from("cleanLinksSwitch"))
        );
        record!("links-off", call!("appReads", QString::from("cleanLinks")));
        record!(
            "pick-notification",
            call!("click", QString::from("notificationOption2"))
        );
        record!(
            "notification-picked",
            call!("appReads", QString::from("notificationDetail"))
        );
        record!(
            "notification-shown",
            get!("notificationCombo", "currentIndex")
        );
        // The other direction: a change made anywhere else reaches the
        // page's choice.
        record!(
            "app-write",
            call!("appWrites", QString::from("markdownMode"), 2)
        );
        record!("page-follows", get!("markdownCombo", "currentIndex"));
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
        "the settings page did not load. {context}"
    );
    for (label, expected) in [
        ("markdown-default", "0"),
        ("markdown-index", "0"),
        ("download-default", "1048576"),
        ("download-index", "3"),
        ("links-default", "false"),
        ("links-switch", "false"),
        ("pick-markdown", "ok"),
        ("markdown-picked", "1"),
        ("markdown-shown", "1"),
        ("pick-download", "ok"),
        ("download-picked", "32768"),
        ("download-shown", "0"),
        ("pick-always", "ok"),
        ("always-picked", "0"),
        ("always-shown", "6"),
        ("flip-links", "ok"),
        ("links-on", "true"),
        ("links-switch-on", "true"),
        ("flip-back", "ok"),
        ("links-off", "false"),
        ("app-markdown-key", "/apps/harbour-postivene/markdown_mode"),
        ("app-links-key", "/apps/harbour-postivene/clean_links"),
        ("app-download-key", "/apps/harbour-postivene/download_limit"),
        (
            "app-notification-key",
            "/apps/harbour-postivene/notification_detail",
        ),
        ("notification-default", "0"),
        ("notification-index", "0"),
        ("pick-notification", "ok"),
        ("notification-picked", "2"),
        ("notification-shown", "2"),
        ("app-write", "ok"),
        ("page-follows", "2"),
    ] {
        assert_eq!(value(label), expected, "{label} is wrong. {context}");
    }
}

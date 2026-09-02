//! The conversation header keeps its title out of the display cutout.
//!
//! A short title never reached the notch; a long one, fading on the left,
//! ran straight under it. The label takes the wider span beside the cutout,
//! so the text ends before the notch whichever side it is on -- and on a
//! screen without one, the margins are the page's.

// Qt harness: see qml_chat_list.rs.
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
    import Sailfish.Silica 1.0
    Item {
        width: 1080
        Loader { id: loader; width: 1080 }
        function load(url) {
            loader.setSource(url, { title: 'a very long name that fades on the left' })
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
        function margins() {
            var label = findIn(loader.item, 'headerTitle')
            if (!label) { return 'missing:headerTitle' }
            return label.anchors.leftMargin + ',' + label.anchors.rightMargin
        }
        // A notch in the middle, as on most phones: room on both sides.
        function notchCentred() {
            Screen.topCutout = Qt.rect(400, 0, 280, 90)
            return 'ok'
        }
        // A hole in the top-right corner: the text has to stop before it.
        function holeTopRight() {
            Screen.topCutout = Qt.rect(960, 0, 90, 90)
            return 'ok'
        }
        function noCutout() {
            Screen.topCutout = Qt.rect(0, 0, 0, 0)
            return 'ok'
        }
    }
";

#[test]
fn the_header_keeps_its_title_out_of_the_cutout() {
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
                QString::from(common::component_url("ConversationHeader.qml"))
            )
        );
        record!("plain", call!("margins"));
        call!("notchCentred");
        record!("centred", call!("margins"));
        call!("holeTopRight");
        record!("top-right", call!("margins"));
        call!("noCutout");
        record!("cleared", call!("margins"));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_margins(&steps);
}

/// The page margin without a cutout; beside the cutout with one.
fn assert_margins(steps: &[(&str, String)]) {
    let context = format!("steps: {steps:?}");
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let pair = |label: &str| -> (f64, f64) {
        let text = value(label);
        let mut parts = text.split(',');
        let left = parts.next().and_then(|n| n.parse().ok()).unwrap_or(-1.0);
        let right = parts.next().and_then(|n| n.parse().ok()).unwrap_or(-1.0);
        (left, right)
    };

    assert_eq!(value("load"), "ok", "the header did not load. {context}");
    // The stub theme's horizontalPageMargin.
    assert_eq!(
        pair("plain"),
        (24.0, 24.0),
        "without a cutout the title does not use the page margins. {context}"
    );
    // Centred notch from 400 to 680: the right side is as wide, so the
    // title sits right of the notch, clear of it by a padding.
    let (left, right) = pair("centred");
    assert!(
        left >= 680.0 && (right - 24.0).abs() < 0.5,
        "a centred notch did not push the title's left edge past it: \
         left {left}, right {right}. {context}"
    );
    // A hole at the top right from 960 to 1050: the left side is wider,
    // so the title ends before the hole.
    let (left, right) = pair("top-right");
    assert!(
        (left - 24.0).abs() < 0.5 && right >= 1080.0 - 960.0,
        "a top-right hole did not pull the title's right edge before it: \
         left {left}, right {right}. {context}"
    );
    assert_eq!(
        pair("cleared"),
        (24.0, 24.0),
        "the margins did not go back once the cutout was gone. {context}"
    );
}

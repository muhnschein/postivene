//! The conversation header keeps its title out of the display cutout.
//!
//! A short title never reached the notch; a long one, fading on the left,
//! ran straight under it. The title is drawn below the cutout, by its
//! height; on a screen without one, nothing moves.

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
        // The header's height, and where the title's centre sits below
        // the top edge.
        function layout() {
            var label = findIn(loader.item, 'headerTitle')
            if (!label) { return 'missing:headerTitle' }
            return loader.item.height + ',' + label.anchors.leftMargin + ','
                + label.anchors.rightMargin + ',' + label.anchors.verticalCenterOffset
        }
        // A notch in the middle, as on most phones.
        function notchCentred() {
            Screen.topCutout = Qt.rect(400, 0, 280, 90)
            return 'ok'
        }
        // A hole in the top-right corner, starting a little below the
        // edge: the inset is its bottom, not its height.
        function holeTopRight() {
            Screen.topCutout = Qt.rect(960, 10, 90, 80)
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
        record!("plain", call!("layout"));
        call!("notchCentred");
        record!("centred", call!("layout"));
        call!("holeTopRight");
        record!("top-right", call!("layout"));
        call!("noCutout");
        record!("cleared", call!("layout"));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_margins(&steps);
}

/// The plain header without a cutout; taller by the cutout's reach with
/// one, the title centred in the part below it, margins untouched.
fn assert_margins(steps: &[(&str, String)]) {
    let context = format!("steps: {steps:?}");
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    // Height, left margin, right margin, vertical centre offset.
    let layout = |label: &str| -> Vec<f64> {
        value(label)
            .split(',')
            .map(|n| n.parse().unwrap_or(-1.0))
            .collect()
    };
    let close = |a: f64, b: f64| (a - b).abs() < 0.5;

    assert_eq!(value("load"), "ok", "the header did not load. {context}");
    // The stub theme: itemSizeLarge 120, horizontalPageMargin 24.
    let plain = layout("plain");
    assert!(
        close(plain[0], 120.0)
            && close(plain[1], 24.0)
            && close(plain[2], 24.0)
            && close(plain[3], 0.0),
        "without a cutout the header is not the plain one. {context}"
    );
    // A notch 90 deep: the header grows by 90, the title moves down by
    // half of that, so it is centred in what is left below the notch.
    let centred = layout("centred");
    assert!(
        close(centred[0], 210.0) && close(centred[3], 45.0),
        "a centred notch did not move the title below it: {centred:?}. {context}"
    );
    assert!(
        close(centred[1], 24.0) && close(centred[2], 24.0),
        "a notch changed the title's side margins: {centred:?}. {context}"
    );
    // A hole from 10 to 90 down: the inset is where it ends, not how tall
    // it is.
    let hole = layout("top-right");
    assert!(
        close(hole[0], 210.0) && close(hole[3], 45.0),
        "a hole below the edge is not measured to its bottom: {hole:?}. {context}"
    );
    let cleared = layout("cleared");
    assert!(
        close(cleared[0], 120.0) && close(cleared[3], 0.0),
        "the header did not go back once the cutout was gone. {context}"
    );
}

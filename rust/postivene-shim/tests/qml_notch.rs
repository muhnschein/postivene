//! The conversation header is laid out as Silica's `PageHeader` lays out
//! its own, and keeps a long title to its own side on a screen with a
//! cutout.
//!
//! `PageHeader` does nothing about a cutout: its titles are short enough
//! never to reach one. A chat's name is not, and fading on the left it ran
//! under the notch. The one thing that does not depend on where the notch
//! is -- which the device reports in a shape this tree cannot read -- is
//! keeping the title to the right-hand part of the header.

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
        function load(url, title) {
            loader.setSource('', {})
            loader.setSource(url, { title: title })
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
        // Header height, the title's width, its left edge, and whether it
        // is drawn in the page's colour.
        function layout() {
            var label = findIn(loader.item, 'headerTitle')
            if (!label) { return 'missing:headerTitle' }
            return loader.item.height + ',' + label.width + ',' + label.x + ','
                + (label.color == Theme.primaryColor ? 'primary' : 'highlight')
        }
        function setInteractive(on) {
            loader.item.interactive = on
            return 'ok'
        }
        // A notch: where it is does not matter to the header, only that
        // there is one.
        function notch() {
            Screen.topCutout = Qt.rect(400, 0, 280, 90)
            return 'ok'
        }
        function noCutout() {
            Screen.topCutout = Qt.rect(0, 0, 0, 0)
            return 'ok'
        }
    }
";

#[test]
fn the_header_keeps_a_long_title_to_its_own_side_of_a_cutout() {
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

    let header = common::component_url("ConversationHeader.qml");
    let long = "a very long name that would run the whole way across the header and under a notch";
    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!("load", QString::from(header.clone()), QString::from(long))
        );
        record!("plain", call!("layout"));
        call!("notch");
        record!("notched", call!("layout"));
        call!("noCutout");
        record!("cleared", call!("layout"));
        // A short title is not widened to the room it has.
        record!(
            "load-short",
            call!("load", QString::from(header.clone()), QString::from("Ada"))
        );
        record!("short", call!("layout"));
        call!("setInteractive", true);
        record!("leads", call!("layout"));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_layout(&steps);
}

/// `PageHeader`'s height and colours, the full width less margins for a
/// long title, and no more than the right-hand part with a cutout.
fn assert_layout(steps: &[(&str, String)]) {
    let context = format!("steps: {steps:?}");
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let layout = |label: &str| -> (f64, f64, f64, String) {
        let text = value(label);
        let mut parts = text.split(',');
        let mut number = || parts.next().and_then(|n| n.parse().ok()).unwrap_or(-1.0);
        let (height, width, x) = (number(), number(), number());
        (
            height,
            width,
            x,
            parts.next().unwrap_or_default().to_string(),
        )
    };
    let close = |a: f64, b: f64| (a - b).abs() < 0.5;

    for label in ["load", "load-short"] {
        assert_eq!(value(label), "ok", "the header did not load. {context}");
    }
    // The stub theme: itemSizeLarge 120, horizontalPageMargin 24, and a
    // header that is highlight-coloured until it leads somewhere.
    // A long title reaches well past the middle, ends at the right
    // margin, and never crosses the left one.
    let (height, width, x, colour) = layout("plain");
    assert!(
        close(height, 120.0)
            && width > 1080.0 * 0.45
            && x >= 24.0 - 0.5
            && close(x + width, 1080.0 - 24.0)
            && colour == "highlight",
        "without a cutout a long title is not laid out as PageHeader lays \
         one out: {height} {width} {x} {colour}. {context}"
    );
    // With one, the title ends at the same right margin and starts no
    // further left than the right 45% of the header.
    let (height, width, x, _) = layout("notched");
    assert!(
        close(height, 120.0) && width <= 1080.0 * 0.45 + 0.5 && x >= 1080.0 * 0.55 - 24.5,
        "with a cutout a long title still runs across the header: \
         {height} {width} {x}. {context}"
    );
    let (_, width, _, _) = layout("cleared");
    assert!(
        width > 1080.0 * 0.45,
        "the title did not get its width back once the cutout was gone. {context}"
    );
    // A short title is as wide as its text and no wider, right-aligned.
    let (_, width, x, _) = layout("short");
    assert!(
        width < 200.0 && close(x + width, 1080.0 - 24.0),
        "a short title is not right-aligned at its own width: {width} {x}. {context}"
    );
    let (_, _, _, colour) = layout("leads");
    assert_eq!(
        colour, "primary",
        "a header that leads somewhere is not drawn in the page's colour. {context}"
    );
}

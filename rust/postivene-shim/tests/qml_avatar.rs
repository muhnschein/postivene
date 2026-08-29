//! A chat with a picture has to read as a circle, like the generated
//! initials beside it do.
//!
//! An `Image` does not inherit its parent's corner radius, and `clip` cuts
//! only to the bounding box, so the picture is drawn through an
//! `OpacityMask` instead. This pins that: the raw image must not be what
//! is on screen.

// Qt harness: see qml_chat_row.rs.
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
        Loader { id: loader }
        function load(url) {
            loader.setSource(url, { width: 540 })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function set(property, value) {
            loader.item[property] = value
            return 'ok'
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
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
    }
";

fn component_url(name: &str) -> String {
    format!(
        "file://{}",
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../qml/components")
            .join(name)
            .display()
    )
}

#[test]
fn a_picture_avatar_is_drawn_through_a_round_mask() {
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
        record!(
            "load",
            call!("load", QString::from(component_url("ChatListDelegate.qml")))
        );
        record!(
            "name",
            call!("set", QString::from("chatName"), QString::from("Ada"))
        );

        // No picture: the initial stands in, and nothing is masked.
        record!("plain-masked", get!("avatarMasked", "visible"));

        call!(
            "set",
            QString::from("avatarPath"),
            QString::from("/tmp/ada.png")
        );
        record!("picture-masked", get!("avatarMasked", "visible"));
        record!("picture-raw", get!("avatarImage", "visible"));
        record!("mask-radius", get!("avatarMask", "radius"));
        record!("mask-width", get!("avatarMask", "width"));

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

    assert_eq!(value("load"), "ok", "the row did not load. {context}");
    assert_eq!(
        value("plain-masked"),
        "false",
        "a chat with no picture still drew a masked image. {context}"
    );
    assert_eq!(
        value("picture-masked"),
        "true",
        "a chat with a picture did not draw it through the mask. {context}"
    );
    assert_eq!(
        value("picture-raw"),
        "false",
        "the unmasked image is on screen, so the avatar renders square. {context}"
    );

    // A circle, not a rounded rectangle: the radius is half the width.
    let radius: f64 = value("mask-radius").parse().unwrap_or_default();
    let width: f64 = value("mask-width").parse().unwrap_or_default();
    assert!(width > 0.0, "the mask has no width. {context}");
    assert!(
        (radius - width / 2.0).abs() < 0.5,
        "the mask is not a circle: radius {radius} of width {width}. {context}"
    );
}

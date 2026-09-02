//! Scanning a QR code: the page runs the camera only while it is on
//! screen, and a frame with a code in it comes back as the code's text.
//!
//! The frame is made here rather than by a camera: the page's own `QrCode`
//! encodes an invite, the test draws those modules into the PGM Qt would
//! have written, and the page's `QrScanner` reads it back through the same
//! call the viewfinder timer makes. That is the whole shim path under the
//! Qt event loop, less the optics.

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
    // by-value parameters; see postivene-shim/src/lib.rs.
    clippy::needless_pass_by_value
)]

use std::path::Path;
use std::time::Duration;

use qmetaobject::*;

mod common;

/// Silica's `pageStack`, recorded rather than performed.
#[derive(QObject, Default)]
struct PageStackProbe {
    base: qt_base_class!(trait QObject),
    /// `pop|...`
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),
    pop: qt_method!(fn(&mut self)),
}

impl PageStackProbe {
    fn pop(&mut self) {
        let current = self.log.to_string();
        self.log = format!("{current}pop|").into();
        self.log_changed();
    }
}

const INVITE: &str = "https://i.delta.chat/#0123456789ABCDEF&a=them%40example.org&n=Them";

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import Postivene 1.0
    Item {
        property string heard: ''
        Loader { id: loader }
        // What the page draws for the test to photograph.
        QrCode { id: code }
        function encode(text) {
            code.text = text
            return code.size + ':' + code.modules
        }
        function load(url) {
            loader.setSource(url, { status: PageStatus.Activating })
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            loader.item.scanned.connect(function(text) { heard = text })
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
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function setStatus(status) { loader.item.status = status; return 'ok' }
        function decode(path) {
            var scanner = findIn(loader.item, 'scanner')
            if (!scanner) { return 'missing:scanner' }
            scanner.decode(path)
            return 'ok'
        }
        function heardText() { return heard }
    }
";

/// The modules as the page's `QrCode` reports them, drawn into a P5 PGM
/// with a quiet zone, each module four pixels: what Qt writes for a
/// `.pgm` path, less the camera.
fn write_frame(encoded: &str, path: &Path) -> Result<(), String> {
    let (size, modules) = encoded
        .split_once(':')
        .ok_or_else(|| format!("no size in {encoded:?}"))?;
    let size: usize = size.parse().map_err(|err| format!("size: {err}"))?;
    if size == 0 {
        return Err("the code has no modules".to_string());
    }
    let (quiet, scale) = (4, 4);
    let edge = (size + 2 * quiet) * scale;
    let mut pixels = vec![u8::MAX; edge * edge];
    for (row, line) in modules.lines().enumerate() {
        for (column, module) in line.chars().enumerate() {
            if module != '1' {
                continue;
            }
            for dy in 0..scale {
                for dx in 0..scale {
                    pixels[((row + quiet) * scale + dy) * edge + (column + quiet) * scale + dx] = 0;
                }
            }
        }
    }
    let mut bytes = format!("P5\n{edge} {edge}\n255\n").into_bytes();
    bytes.extend_from_slice(&pixels);
    std::fs::write(path, bytes).map_err(|err| err.to_string())
}

#[test]
fn a_code_held_up_to_the_page_comes_back_as_its_text() {
    let temp = std::env::temp_dir().join(format!("postivene-scan-{}", std::process::id()));
    std::fs::create_dir_all(&temp).expect("create temp dir");
    let frame = temp.join("frame.pgm");

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("XDG_CACHE_HOME", &temp);
    }

    postivene_shim::register_qml_types();

    let stack_box = QObjectBox::new(PageStackProbe::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("pageStack".into(), stack_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

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

    let frame_path = frame.clone();
    single_shot(Duration::from_secs(1), move || unsafe {
        // The picture: the page's own encoder, drawn into a file.
        let encoded = call!("encode", QString::from(INVITE));
        record!(
            "frame",
            write_frame(&encoded, &frame_path).map_or_else(|err| err, |()| "ok".to_string())
        );
        record!(
            "load",
            call!("load", QString::from(common::page_url("ScanPage.qml")))
        );
        // Still arriving: the camera is not running and nothing is grabbed.
        record!("camera-before", get!("camera", "running"));
        record!("grabbing-before", get!("grabber", "running"));
        call!("setStatus", 2);
        record!("camera-active", get!("camera", "running"));
        record!("grabbing-active", get!("grabber", "running"));
        // The viewfinder timer's own call, with the frame made above.
        record!(
            "decode",
            call!(
                "decode",
                QString::from(frame_path.to_string_lossy().into_owned())
            )
        );
        record!("busy", get!("scanner", "busy"));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("heard", call!("heardText"));
        record!("camera-after", get!("camera", "running"));
        record!("grabbing-after", get!("grabber", "running"));
        record!("navigation", (*stack_ptr).pinned().borrow().log.to_string());
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_scan(&steps);
}

/// The camera follows the page's status, the frame decodes to the invite,
/// and the page hands it on and goes.
fn assert_scan(steps: &[(&str, String)]) {
    let context = format!("steps: {steps:?}");
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };

    for label in ["frame", "load", "decode"] {
        assert_eq!(value(label), "ok", "step {label} failed. {context}");
    }
    assert_eq!(
        value("camera-before"),
        "false",
        "the camera runs before the page is on screen. {context}"
    );
    assert_eq!(
        value("grabbing-before"),
        "false",
        "frames are grabbed before the page is on screen. {context}"
    );
    assert_eq!(
        value("camera-active"),
        "true",
        "the camera does not start with the page. {context}"
    );
    assert_eq!(
        value("grabbing-active"),
        "true",
        "frames are not grabbed while the page is on screen. {context}"
    );
    assert_eq!(
        value("busy"),
        "true",
        "the scanner does not say it is busy while decoding, so frames \
         would queue behind it. {context}"
    );
    assert_eq!(
        value("heard"),
        INVITE,
        "the frame did not come back as the invite it carried. {context}"
    );
    assert_eq!(
        value("camera-after"),
        "false",
        "the camera keeps running after a code was read. {context}"
    );
    assert_eq!(
        value("grabbing-after"),
        "false",
        "frames keep being grabbed after a code was read. {context}"
    );
    assert_eq!(
        value("navigation"),
        "pop|",
        "the page did not go once the code was read. {context}"
    );
}

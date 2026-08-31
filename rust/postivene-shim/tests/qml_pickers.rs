//! The two attachment pickers, loaded on their own.
//!
//! They are pages rather than Components inside `ConversationPage`, precisely
//! so that a `Sailfish.Pickers` type that is not there costs one button
//! instead of the whole conversation -- which also means nothing else loads
//! them, and without this nothing would notice them breaking.
//!
//! What can be checked here is our half: that each page reports the chosen
//! path on `picked`, and reports nothing when the picker was left without a
//! choice. That the Silica types themselves exist is a device question, and
//! `tests/silica-stubs` cannot answer it.

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
    Item {
        property string reported: ''
        Loader { id: loader }
        function load(url) {
            loader.setSource('', {})
            loader.setSource(url, {})
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            reported = ''
            loader.item.picked.connect(function (path) { reported = path })
            return 'ok'
        }
        // What the picker does when someone chooses a file.
        function choose(path) {
            loader.item.selectedContentProperties = { filePath: path }
            return 'ok'
        }
        function heard() { return reported }
    }
";

#[test]
fn each_picker_reports_the_file_that_was_chosen() {
    // No core here: these pages talk to nothing but their caller, so there
    // is no server to spawn and no accounts directory to point it at.
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

    single_shot(Duration::from_secs(1), move || unsafe {
        for (label, page) in [
            ("photo-load", "AttachPhotoPage.qml"),
            ("photo-heard", ""),
            ("file-load", "AttachFilePage.qml"),
            ("file-heard", ""),
        ] {
            if page.is_empty() {
                // Nothing chosen yet: an empty path must not be reported as
                // a file, or cancelling the picker would arm an attachment.
                (*steps_ptr).push((label, call!("heard")));
                continue;
            }
            (*steps_ptr).push((label, call!("load", QString::from(common::page_url(page)))));
        }
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        // The photo page has been replaced by the file page, so this drives
        // the one still loaded and the assertions below say which.
        (*steps_ptr).push((
            "choose",
            call!("choose", QString::from("/home/user/Documents/report.pdf")),
        ));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push(("chosen-heard", call!("heard")));
        (*steps_ptr).push((
            "photo-reload",
            call!(
                "load",
                QString::from(common::page_url("AttachPhotoPage.qml"))
            ),
        ));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        (*steps_ptr).push((
            "choose-photo",
            call!(
                "choose",
                QString::from("/home/user/Pictures/holiday photo.png")
            ),
        ));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        (*steps_ptr).push(("photo-chosen-heard", call!("heard")));
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

    for label in ["photo-load", "file-load", "photo-reload"] {
        assert_eq!(value(label), "ok", "{label} did not load. {context}");
    }
    for label in ["photo-heard", "file-heard"] {
        assert_eq!(
            value(label),
            "",
            "a picker reported a file before one was chosen, so cancelling it \
             would arm an attachment. {context}"
        );
    }
    assert_eq!(
        value("chosen-heard"),
        "/home/user/Documents/report.pdf",
        "the file picker did not report what was chosen. {context}"
    );
    assert_eq!(
        value("photo-chosen-heard"),
        "/home/user/Pictures/holiday photo.png",
        "the photo picker did not report what was chosen. {context}"
    );
}

//! A file somebody sent, and the two things there are to do with it.
//!
//! Tapping a picture opens the picture page and tapping a video opens
//! the video page; everything else used to go straight to the phone,
//! which for a note, a to-do list or a patch means nothing happens at
//! all -- no installed app claims those, and the handover fails without
//! a word. What this pins is the page that took that tap over: that it
//! shows a text file rather than only naming it, that it says so when it
//! cannot, and that opening elsewhere and keeping a copy are both on
//! offer either way.

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
        height: 900

        Loader { id: page }
        function load(url, properties) {
            page.setSource('', {})
            page.setSource(url, properties)
            return page.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function loadNote(url, path, name) {
            return load(url, {
                fileUrl: path, fileName: name,
                fileMime: 'application/octet-stream', fileBytes: 34,
                isText: true
            })
        }
        function loadOther(url, path) {
            return load(url, {
                fileUrl: path, fileName: 'holiday.zip',
                fileMime: 'application/zip', fileBytes: 2048,
                isText: false
            })
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
            var item = findIn(page.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        // What the pull-down menu offers, in order.
        function options() {
            var names = ['openExternally', 'saveToDevice']
            var parts = []
            for (var i = 0; i < names.length; i++) {
                var item = findIn(page.item, names[i])
                parts.push(item ? item.text : 'missing:' + names[i])
            }
            return parts.join('|')
        }
        function save() {
            var item = findIn(page.item, 'saveToDevice')
            if (!item) { return 'missing:saveToDevice' }
            item.clicked()
            return 'ok'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_note_is_read_here_and_anything_else_says_what_it_is() {
    let temp = std::env::temp_dir().join(format!("postivene-file-page-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).expect("create temp dir");
    let note = temp.join("TODO.md");
    std::fs::write(&note, "# TODO\n\n- [ ] read this on a phone\n").expect("write the note");
    let archive = temp.join("holiday.zip");
    std::fs::write(&archive, b"PK\x03\x04not really").expect("write the archive");
    let downloads = temp.join("Downloads");

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    postivene_shim::register_qml_types();

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));
    // The stub's StandardPaths is writable, so the copy lands somewhere
    // this test can look at rather than in the runner's home.
    engine.invoke_method("void".into(), &[]);

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

    let page = common::page_url("FilePage.qml");
    let other_page = page.clone();
    let note_url = format!("file://{}", note.display());
    let archive_url = format!("file://{}", archive.display());

    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "note",
            call!(
                "loadNote",
                QString::from(page.clone()),
                QString::from(note_url.clone()),
                QString::from("TODO.md")
            )
        );
        record!(
            "note-body",
            call!("get", QString::from("bodyLabel"), QString::from("text"))
        );
        record!(
            "note-name",
            call!("get", QString::from("fileNameLabel"), QString::from("text"))
        );
        record!(
            "note-facts",
            call!(
                "get",
                QString::from("fileFactsLabel"),
                QString::from("text")
            )
        );
        record!(
            "note-excuse",
            call!(
                "get",
                QString::from("nothingToShowLabel"),
                QString::from("visible")
            )
        );
        record!(
            "note-plain",
            call!(
                "get",
                QString::from("bodyLabel"),
                QString::from("textFormat")
            )
        );
        record!("note-options", call!("options"));
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        record!(
            "other",
            call!(
                "loadOther",
                QString::from(other_page.clone()),
                QString::from(archive_url.clone())
            )
        );
        record!(
            "other-body",
            call!("get", QString::from("bodyLabel"), QString::from("visible"))
        );
        record!(
            "other-excuse",
            call!(
                "get",
                QString::from("nothingToShowLabel"),
                QString::from("visible")
            )
        );
        record!("other-options", call!("options"));
        record!("saved", call!("save"));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!(
            "saved-notice",
            call!("get", QString::from("noticeLabel"), QString::from("text"))
        );
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

    assert_eq!(value("note"), "ok", "the file page did not load. {context}");
    assert_eq!(
        value("note-body"),
        "# TODO\n\n- [ ] read this on a phone\n",
        "a note somebody sent was not shown, which is the whole of what \
         a reader wanted from it. {context}"
    );
    assert_eq!(
        value("note-name"),
        "TODO.md",
        "the page does not say which file it is showing. {context}"
    );
    assert!(
        value("note-facts").contains("34"),
        "the page says nothing about how big the file is: \
         {:?}. {context}",
        value("note-facts")
    );
    assert_eq!(
        value("note-excuse"),
        "false",
        "the page said it could not show a file it was showing. {context}"
    );
    // Text.PlainText is 0: a file from the other end is not markup, for
    // the reason every message body is pinned the same way.
    assert_eq!(
        value("note-plain"),
        "0",
        "the file's contents were drawn as anything but plain text. \
         {context}"
    );
    assert_eq!(
        value("note-options"),
        "Open in another app|Save to Downloads",
        "the page does not offer both of the things there are to do with \
         a file. {context}"
    );

    assert_eq!(
        value("other-body"),
        "false",
        "the page tried to show an archive as words. {context}"
    );
    assert_eq!(
        value("other-excuse"),
        "true",
        "a file the app cannot show left the page blank, with no reason \
         for it and no sign that the menu holds the way out. {context}"
    );
    assert_eq!(
        value("other-options"),
        "Open in another app|Save to Downloads",
        "a file the app cannot show offered no way to open it elsewhere. \
         {context}"
    );

    assert_eq!(
        value("saved-notice"),
        "Saved to Downloads",
        "keeping a copy said nothing about where it went. {context}"
    );
    let stub_downloads = std::path::Path::new("/tmp/postivene-stub-standardpaths/Downloads");
    assert!(
        stub_downloads.join("holiday.zip").exists(),
        "no copy was made in {}, so Save did nothing. {context}",
        stub_downloads.display()
    );
    let _ = downloads;
}

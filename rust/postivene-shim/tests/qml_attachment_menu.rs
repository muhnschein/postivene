//! Opening a received file, and keeping it.
//!
//! A tap on a message opens whatever it carries. The long press has
//! both in words: Open, and Save -- the copy being the thing a tap
//! cannot ask for, and the reason a file somebody sent stays theirs
//! rather than the chat's. What this pins is that both are offered on a
//! message that carries a file, that neither is offered on one that does
//! not, and that each says which file it means.

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

        property string raised: ''

        ListModel { id: rows }
        Loader { id: list; width: 540; height: 900 }
        function load(url) {
            // A note, and a message with nothing attached to it.
            rows.append({
                message_id: 7, text: '', styled_text: '', plain_text: '',
                is_outgoing: false, is_info: false, show_padlock: true,
                state: 16, timestamp: 1700000000, day_number: 19675,
                sender_name: 'Ada', sender_color: '#00875a',
                is_forwarded: false, quote_text: '', quote_author: '',
                file_path: '/tmp/postivene-menu/TODO.md',
                file_name: 'TODO.md',
                file_mime: 'application/octet-stream', file_bytes: 34,
                view_type: 'File', image_width: 0, image_height: 0,
                is_new: false, has_html: false,
                download_state: 'Done', vcard_name: '', vcard_addr: '',
                vcard_color: '', webxdc_name: '', webxdc_document: '',
                webxdc_summary: '', webxdc_icon: '', reactions: '',
                my_reaction: '', loaded: true
            })
            rows.append({
                message_id: 8, text: 'just words', styled_text: '',
                plain_text: '', is_outgoing: false, is_info: false,
                show_padlock: true, state: 16, timestamp: 1700000100,
                day_number: 19675, sender_name: 'Ada',
                sender_color: '#00875a', is_forwarded: false,
                quote_text: '', quote_author: '', file_path: '',
                file_name: '', file_mime: '', file_bytes: 0,
                view_type: 'Text', image_width: 0, image_height: 0,
                is_new: false, has_html: false,
                download_state: 'Done', vcard_name: '', vcard_addr: '',
                vcard_color: '', webxdc_name: '', webxdc_document: '',
                webxdc_summary: '', webxdc_icon: '', reactions: '',
                my_reaction: '', loaded: true
            })
            list.setSource(url, { model: rows })
            if (list.status !== Loader.Ready) { return 'load-failed' }
            list.item.openRequested.connect(
                function(fileUrl, fileName, viewType, previewWidth) {
                    raised += 'open:' + fileName + ':' + viewType + ';'
                })
            list.item.saveRequested.connect(function(fileUrl, viewType) {
                raised += 'save:' + fileUrl + ':' + viewType + ';'
            })
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
        function findAll(node, name, out) {
            if (!node) { return out }
            if (node.objectName === name) { out.push(node) }
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {
                findAll(kids[i], name, out)
            }
            return out
        }
        /// The rows, oldest first, without counting one twice: a
        /// ListItem is reachable through its children and through its
        /// contentItem both.
        function rowsInOrder() {
            var found = findAll(list.item, 'messageRow', [])
            var seen = []
            for (var i = 0; i < found.length; i++) {
                if (seen.indexOf(found[i]) === -1) { seen.push(found[i]) }
            }
            seen.sort(function(a, b) { return a.y - b.y })
            return seen
        }
        /// Whether one of the menu's items is on offer on a row.
        function offered(index, name) {
            var rows = rowsInOrder()
            if (index >= rows.length) { return 'missing:row' }
            var menu = rows[index].menu
            if (!menu) { return 'no-menu' }
            var item = findIn(menu, name)
            return item ? '' + item.visible : 'missing:' + name
        }
        function labelOf(index, name) {
            var rows = rowsInOrder()
            if (index >= rows.length) { return 'missing:row' }
            var item = findIn(rows[index].menu, name)
            return item ? item.text : 'missing:' + name
        }
        function pick(index, name) {
            var rows = rowsInOrder()
            if (index >= rows.length) { return 'missing:row' }
            var item = findIn(rows[index].menu, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        function raisedSignal() { return raised }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_message_carrying_a_file_offers_to_open_it_and_to_keep_it() {
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
                QString::from(common::component_url("ConversationList.qml"))
            )
        );
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        record!("file-open", call!("offered", 0, QString::from("openItem")));
        record!("file-save", call!("offered", 0, QString::from("saveItem")));
        record!("open-label", call!("labelOf", 0, QString::from("openItem")));
        record!("save-label", call!("labelOf", 0, QString::from("saveItem")));
        record!("words-open", call!("offered", 1, QString::from("openItem")));
        record!("words-save", call!("offered", 1, QString::from("saveItem")));
        record!("picked-open", call!("pick", 0, QString::from("openItem")));
        record!("picked-save", call!("pick", 0, QString::from("saveItem")));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("raised", call!("raisedSignal"));
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

    assert_eq!(value("load"), "ok", "the list did not load. {context}");
    assert_eq!(
        value("file-open"),
        "true",
        "a message carrying a note does not offer to open it. {context}"
    );
    assert_eq!(
        value("file-save"),
        "true",
        "a message carrying a note does not offer to keep a copy, which \
         is the one thing a tap on it cannot do. {context}"
    );
    assert_eq!(
        (value("open-label"), value("save-label")),
        ("Open".to_string(), "Save".to_string()),
        "the two offers do not say what they do. {context}"
    );
    assert_eq!(
        value("words-open"),
        "false",
        "a message of three words offered to open a file it has not got. \
         {context}"
    );
    assert_eq!(
        value("words-save"),
        "false",
        "a message of three words offered to save a file it has not got. \
         {context}"
    );

    assert_eq!(
        value("raised"),
        "open:TODO.md:File;save:file:///tmp/postivene-menu/TODO.md:File;",
        "the menu did not say which file it meant, or what kind it is: \
         the page decides where a copy goes from the kind. {context}"
    );
}

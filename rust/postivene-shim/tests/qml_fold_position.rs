//! Folding a long message back leaves the reader looking at it.
//!
//! A row that was filling the screen and is suddenly a dozen lines takes
//! everything below it up with it. The reader who had scrolled into the
//! middle of what they were reading is then looking at whatever happens
//! to be at that height -- another message, or the end of the chat --
//! rather than at the message they just folded, which is the one place
//! they meant to be.

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
        height: 600

        ListModel { id: rows }
        Loader { id: list; width: 540; height: 600 }

        function row(id, body) {
            return {
                message_id: id, text: body, styled_text: '',
                plain_text: '', is_outgoing: false, is_info: false,
                show_padlock: true, state: 16,
                timestamp: 1700000000 + id, day_number: 19675,
                sender_name: 'Ada', sender_color: '#00875a',
                is_forwarded: false, quote_text: '', quote_author: '',
                file_path: '', file_name: '', file_mime: '', file_bytes: 0,
                view_type: 'Text', image_width: 0, image_height: 0,
                is_new: false, has_html: false, download_state: 'Done',
                vcard_name: '', vcard_addr: '', vcard_color: '',
                webxdc_name: '', webxdc_document: '', webxdc_summary: '',
                webxdc_icon: '', reactions: '', my_reaction: '',
                loaded: true
            }
        }
        function load(url, longBody) {
            // Two remarks, the long message, and a chat's worth of
            // remarks after it. Enough below it that folding it does not
            // simply run the view into the end of the list: with a short
            // chat the view is pinned to the bottom either way and
            // nothing here would be measuring anything.
            rows.append(row(1, 'first'))
            rows.append(row(2, 'second'))
            rows.append(row(3, longBody))
            for (var i = 4; i < 34; i++) {
                rows.append(row(i, 'remark ' + i))
            }
            list.setSource(url, { model: rows })
            return list.status === Loader.Ready ? 'ok' : 'load-failed'
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
        /// The row carrying the long message, found by what is in it:
        /// which delegates exist and in what order changes as the view
        /// scrolls, so a position cannot name a row.
        function longRow() {
            var found = findAll(list.item, 'messageRow', [])
            for (var i = 0; i < found.length; i++) {
                var body = findIn(found[i], 'messageLabel')
                if (body && ('' + body.text).indexOf('line 1 of') === 0) {
                    return found[i]
                }
            }
            return null
        }
        /// Scroll back to the long message: the view opens on the
        /// newest message, and with a chat this long the older ones have
        /// no delegates at all until it is scrolled to them.
        function goTo() {
            list.item.positionViewAtIndex(2, ListView.Beginning)
            return 'ok'
        }
        function fold(open) {
            // What the row's own Expand does, which is the list's to do:
            // a row cannot remember whether it is open.
            list.item.toggleExpanded(3, 2)
            return 'ok'
        }
        function rowHeight() {
            var row = longRow()
            return row ? '' + Math.round(row.height) : 'missing:row'
        }
        /// Read deep inside the opened message, as somebody reading it
        /// would be.
        function readInto(depth) {
            var row = longRow()
            if (!row) { return 'missing:row' }
            list.item.contentY = row.y + depth
            return '' + Math.round(list.item.contentY)
        }
        /// Whether any of the row is on the screen, and where.
        function rowOnScreen() {
            var row = longRow()
            if (!row) { return 'missing:row' }
            var top = row.y - list.item.contentY
            var bottom = top + row.height
            var seen = bottom > 0 && top < list.item.height
            return seen + '@' + Math.round(top) + ':' + Math.round(bottom)
        }
    }
";

/// A message long enough to fill the screen when it is opened out.
fn a_long_message() -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    for number in 1..=40 {
        let _ = writeln!(out, "line {number} of something worth reading");
    }
    out
}

#[test]
fn folding_a_message_back_puts_the_view_on_it() {
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

    let long = a_long_message();

    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::component_url("ConversationList.qml")),
                QString::from(long.clone())
            )
        );
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        record!("scrolled", call!("goTo"));
        record!("folded-height", call!("rowHeight"));
        record!("open", call!("fold", false));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("open-height", call!("rowHeight"));
        // Near the end of it, which is where reading it leaves you --
        // and, once it is folded, well past the row.
        record!("read-into", call!("readInto", 700));
        record!("close", call!("fold", true));
    });

    // Past the moment the view lays itself out again.
    single_shot(Duration::from_secs(5), move || unsafe {
        record!("after", call!("rowOnScreen"));
        record!("after-height", call!("rowHeight"));
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
    let number = |label: &str| value(label).parse::<f64>().unwrap_or(-1.0);
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the list did not load. {context}");
    assert_eq!(
        value("scrolled"),
        "ok",
        "the view could not be put on the long message. {context}"
    );
    assert!(
        number("open-height") > number("folded-height"),
        "opening the message out did not make its row taller \\
         ({} to {}), so nothing here is being measured. {context}",
        number("folded-height"),
        number("open-height")
    );
    assert!(
        number("read-into") > 0.0,
        "the view could not be scrolled into the opened message. \
         {context}"
    );
    assert!(
        (number("after-height") - number("folded-height")).abs() < 1.0,
        "the row did not go back to its folded height. {context}"
    );

    let after = value("after");
    assert!(
        after.starts_with("true@"),
        "folding the message back left the reader somewhere else: the \
         row is at {after} in a screen 600 tall. {context}"
    );
}

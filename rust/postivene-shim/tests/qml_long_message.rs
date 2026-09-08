//! A message too long for the bubble it arrived in.
//!
//! A conversation is a column of remarks, and somebody's to-do document
//! is not one: drawn whole it fills the screen, pushes the rest of the
//! chat out of it, and makes a row nobody can scroll past. So the row
//! shows the first few lines and offers the rest -- opened out here, or
//! read on a page of its own.
//!
//! What this pins is where the offers appear and where they do not: a
//! short message must not grow two words of chrome it has no use for,
//! and a message the sending core cut must offer the page even when the
//! little it has fits, because the rest of it is not on this phone at
//! all.
//!
//! And that the two of them never land on top of each other. A bubble is
//! as wide as its widest line, so a long message of short lines makes a
//! narrow one -- and "Expand" and "View full message" were drawn into
//! the same few pixels.
//!
//! And what they are *never* about: an attachment. A picture or a
//! document with no caption has no body to fold, and a message the core
//! is still holding back has none of it yet -- neither was excluded at
//! first, and an RPM arriving in an open chat grew an Expand and a View
//! full message it had no use for.
//!
//! The delegate and the list are loaded on their own, as the other QML
//! tests load them.

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

        // The delegate on its own.
        Loader { id: delegate }
        function loadDelegate(url) {
            delegate.setSource(url, { width: 540 })
            return delegate.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function set(property, value) {
            delegate.item[property] = value
            return 'ok'
        }
        function ask(property) { return '' + delegate.item[property] }
        /// Where the two offers are, in the row that holds them: the
        /// gap between the end of one and the start of the other, and
        /// whether they are on the same line at all.
        function offerGap() {
            var expand = findIn(delegate.item, 'expandButton')
            var full = findIn(delegate.item, 'fullButton')
            if (!expand || !full) { return 'missing:offers' }
            var sameLine = Math.abs(expand.y - full.y) < 1
            var gap = sameLine ? full.x - (expand.x + expand.width)
                               : full.y - (expand.y + expand.height)
            return (sameLine ? 'row' : 'column') + '@' + Math.round(gap)
        }
        function bubbleWidth() {
            var bubble = findIn(delegate.item, 'bubble')
            return bubble ? '' + Math.round(bubble.width) : 'missing:bubble'
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
            var item = findIn(delegate.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }

        // The list, over a plain model with two rows in it: one long
        // message and one the core cut.
        ListModel { id: rows }
        Loader { id: list; width: 540; height: 900 }
        function loadList(url, longText) {
            rows.append({
                message_id: 7, text: longText, styled_text: '',
                plain_text: '', is_outgoing: false, is_info: false,
                show_padlock: true, state: 16, timestamp: 1700000000,
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
            list.item.fullTextRequested.connect(function(id, author) {
                raised = 'full:' + id + ':' + author
            })
            return 'ok'
        }
        function rowBody() {
            var row = findIn(list.item, 'messageRow')
            if (!row) { return null }
            return findIn(row, 'bodyActions')
        }
        function rowExpanded() {
            var row = findIn(list.item, 'messageRow')
            if (!row) { return 'missing:messageRow' }
            var body = findIn(row, 'messageLabel')
            if (!body) { return 'missing:messageLabel' }
            // The delegate the row built, whose `expanded` the list feeds.
            return '' + list.item.isExpanded(7)
        }
        function tapExpand() {
            var actions = rowBody()
            if (!actions) { return 'missing:bodyActions' }
            var button = findIn(actions, 'expandButton')
            if (!button) { return 'missing:expandButton' }
            // The MouseArea inside it, which is what the tap lands on.
            button.children[0].clicked(null)
            return 'ok'
        }
        function tapFull() {
            var actions = rowBody()
            if (!actions) { return 'missing:bodyActions' }
            var button = findIn(actions, 'fullButton')
            if (!button) { return 'missing:fullButton' }
            button.children[0].clicked(null)
            return 'ok'
        }
        function raisedSignal() { return raised }
    }
";

/// The same length in short lines. A bubble is as wide as its widest
/// line, so this one comes out narrow -- narrower than the two offers
/// under it, which is how they came to be drawn over each other.
fn a_narrow_message() -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    for number in 1..=40 {
        let _ = writeln!(out, "hi {number}");
    }
    out
}

/// A message nobody would want drawn whole in a bubble.
fn a_long_message() -> String {
    let mut out = String::new();
    for number in 1..=40 {
        use std::fmt::Write as _;
        let _ = writeln!(out, "- [ ] the {number}th thing to do today");
    }
    out
}

#[test]
#[allow(clippy::too_many_lines)]
fn a_long_body_is_cut_to_a_few_lines_with_the_rest_on_offer() {
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
    let long_for_list = long.clone();

    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "loadDelegate",
                QString::from(common::component_url("MessageDelegate.qml"))
            )
        );
        // A remark, which is what nearly every message is.
        call!(
            "set",
            QString::from("messageText"),
            QString::from("Buy milk")
        );
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        record!(
            "short-actions",
            call!(
                "get",
                QString::from("bodyActions"),
                QString::from("visible")
            )
        );
        record!("short-height", call!("ask", QString::from("height")));
        call!(
            "set",
            QString::from("messageText"),
            QString::from(long.clone())
        );
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("long-gap", call!("offerGap"));
        record!("long-bubble", call!("bubbleWidth"));
        record!(
            "long-actions",
            call!(
                "get",
                QString::from("bodyActions"),
                QString::from("visible")
            )
        );
        record!(
            "long-expand",
            call!("get", QString::from("expandButton"), QString::from("text"))
        );
        record!(
            "long-full",
            call!("get", QString::from("fullButton"), QString::from("visible"))
        );
        record!(
            "long-lines",
            call!(
                "get",
                QString::from("messageLabel"),
                QString::from("lineCount")
            )
        );
        record!(
            "long-truncated",
            call!(
                "get",
                QString::from("messageLabel"),
                QString::from("truncated")
            )
        );
        record!("collapsed-height", call!("ask", QString::from("height")));
        call!("set", QString::from("expanded"), true);
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!(
            "open-expand",
            call!("get", QString::from("expandButton"), QString::from("text"))
        );
        record!(
            "open-lines",
            call!(
                "get",
                QString::from("messageLabel"),
                QString::from("lineCount")
            )
        );
        record!("open-height", call!("ask", QString::from("height")));
        // A message the core cut: what is here is short, and the rest is
        // not on this phone.
        call!("set", QString::from("expanded"), false);
        call!(
            "set",
            QString::from("messageText"),
            QString::from("# Groceries\n[...]")
        );
        call!("set", QString::from("hasHtml"), true);
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        record!(
            "cut-full",
            call!("get", QString::from("fullButton"), QString::from("visible"))
        );
        record!(
            "cut-expand",
            call!(
                "get",
                QString::from("expandButton"),
                QString::from("visible")
            )
        );
        // Forty short lines: the bubble is as wide as its widest line,
        // so this is the narrow one the offers were drawn over each
        // other in.
        call!(
            "set",
            QString::from("messageText"),
            QString::from(a_narrow_message())
        );
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("narrow-gap", call!("offerGap"));
        record!("narrow-bubble", call!("bubbleWidth"));
        record!(
            "list",
            call!(
                "loadList",
                QString::from(common::component_url("ConversationList.qml")),
                QString::from(long_for_list.clone())
            )
        );
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        record!("row-open-before", call!("rowExpanded"));
        record!("row-tap", call!("tapExpand"));
    });

    single_shot(Duration::from_secs(8), move || unsafe {
        record!("row-open-after", call!("rowExpanded"));
        record!("row-tap-again", call!("tapExpand"));
    });

    single_shot(Duration::from_secs(9), move || unsafe {
        record!("row-open-again", call!("rowExpanded"));
        record!("row-full", call!("tapFull"));
        record!("row-raised", call!("raisedSignal"));

        // An attachment: a file, no caption, and the core saying it has
        // an HTML part -- which is the shape that grew the offers.
        call!("set", QString::from("expanded"), false);
        call!("set", QString::from("messageText"), QString::from(""));
        call!("set", QString::from("hasHtml"), true);
        call!("set", QString::from("viewType"), QString::from("File"));
        call!(
            "set",
            QString::from("filePath"),
            QString::from("/tmp/postivene-long-message/postivene.rpm")
        );
        call!(
            "set",
            QString::from("fileName"),
            QString::from("postivene.rpm")
        );
    });

    single_shot(Duration::from_secs(10), move || unsafe {
        record!(
            "file-actions",
            call!(
                "get",
                QString::from("bodyActions"),
                QString::from("visible")
            )
        );
        // And a message the core is holding back: a header and a
        // download offer, with none of the words here yet.
        call!("set", QString::from("filePath"), QString::from(""));
        call!("set", QString::from("viewType"), QString::from("Text"));
        call!(
            "set",
            QString::from("messageText"),
            QString::from("a message that has not been fetched")
        );
        call!(
            "set",
            QString::from("downloadState"),
            QString::from("Available")
        );
    });

    single_shot(Duration::from_secs(11), move || unsafe {
        record!(
            "held-actions",
            call!(
                "get",
                QString::from("bodyActions"),
                QString::from("visible")
            )
        );
        record!(
            "held-download",
            call!(
                "get",
                QString::from("downloadButton"),
                QString::from("visible")
            )
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
    let number = |label: &str| value(label).parse::<f64>().unwrap_or(-1.0);
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the delegate did not load. {context}");

    assert_eq!(
        value("short-actions"),
        "false",
        "a message of three words offered to be opened out. {context}"
    );

    assert_eq!(
        value("long-actions"),
        "true",
        "a forty-line message offered nothing: the reader can see the \
         first lines of it and has no way to the rest. {context}"
    );
    assert_eq!(
        value("long-truncated"),
        "true",
        "the bubble drew the whole of a forty-line message. {context}"
    );
    assert_eq!(
        value("long-expand"),
        "Expand",
        "the offer on a folded message does not say what it does. \
         {context}"
    );
    assert_eq!(
        value("long-full"),
        "true",
        "a long message did not offer a page of its own. {context}"
    );
    let collapsed_lines = number("long-lines");
    assert!(
        collapsed_lines > 1.0 && collapsed_lines <= 12.0,
        "the folded body is {collapsed_lines} lines, not the dozen the \
         row caps it at. {context}"
    );

    assert_eq!(
        value("open-expand"),
        "Collapse",
        "an opened message does not offer to be folded again -- and once \
         open, the label's own `truncated` is false, so the offer has to \
         come from the row's state. {context}"
    );
    let open_lines = number("open-lines");
    assert!(
        open_lines > collapsed_lines,
        "opening the message out showed {open_lines} lines, no more than \
         the {collapsed_lines} it was folded to. {context}"
    );
    assert!(
        number("open-height") > number("collapsed-height"),
        "the row did not grow when the message was opened out, so the \
         rest of it is drawn over whatever is below. {context}"
    );

    // Side by side with a gap between them on a wide bubble, and never
    // overlapping: a negative gap is one label drawn over the other.
    let gap = |label: &str| -> f64 {
        value(label)
            .rsplit('@')
            .next()
            .and_then(|number| number.parse().ok())
            .unwrap_or(-1.0)
    };
    assert!(
        value("long-gap").starts_with("row@") && gap("long-gap") >= 0.0,
        "the two offers are not side by side with room between them on \
         a wide message: {}. {context}",
        value("long-gap")
    );
    assert!(
        gap("narrow-gap") >= 0.0,
        "the offers overlap by {} on a message of short lines -- the \
         bubble is as wide as its widest line, and that is narrower \
         than the two of them. {context}",
        -gap("narrow-gap")
    );
    assert!(
        number("narrow-bubble") > 0.0,
        "the narrow bubble was not measured. {context}"
    );

    assert_eq!(
        value("cut-full"),
        "true",
        "a message the sending core cut did not offer the page, so its \
         `[...]` is the end of it as far as the reader can tell. {context}"
    );
    assert_eq!(
        value("cut-expand"),
        "false",
        "a cut message offered to be opened out here, which would show \
         the same `[...]` again: the rest is not on this phone. {context}"
    );

    assert_eq!(value("list"), "ok", "the list did not load. {context}");
    assert_eq!(
        value("row-open-before"),
        "false",
        "a message was open before anyone asked. {context}"
    );
    assert_eq!(
        value("row-open-after"),
        "true",
        "tapping Expand did not open the row: the list is what remembers \
         which messages are open, since a row is rebuilt every time it \
         scrolls back into view. {context}"
    );
    assert_eq!(
        value("row-open-again"),
        "false",
        "tapping Collapse did not fold the row back. {context}"
    );
    assert_eq!(
        value("row-raised"),
        "full:7:Ada",
        "asking for the page did not name the message and who wrote it. \
         {context}"
    );

    assert_eq!(
        value("file-actions"),
        "false",
        "an attachment with no caption offered to be opened out and read \
         on a page of its own -- there is no body there to do either \
         with. {context}"
    );
    assert_eq!(
        value("held-actions"),
        "false",
        "a message the core is still holding back offered the rest of \
         its words, which are not on this phone yet. {context}"
    );
    assert_eq!(
        value("held-download"),
        "true",
        "the one offer a held-back message should carry is gone. \
         {context}"
    );
}

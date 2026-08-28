//! Where a conversation sits when you come back to it, and when a new
//! message is allowed to move it.

// Qt harness: needs `unsafe` for `env::set_var` before Qt starts
// (`unused_unsafe` because it is only unsafe from edition 2024 on),
// `borrow_as_ptr` for the engine pointer, and `single_shot` with
// whole-second Durations.
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

/// The list, over a plain `ListModel` carrying the roles the real one has.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        width: 540
        height: 400

        ListModel { id: rows }

        Loader {
            id: loader
            anchors.fill: parent
        }

        function append(count) {
            for (var i = 0; i < count; i++) {
                rows.append({
                    text: 'message number ' + rows.count + ', long enough '
                          + 'to take a line of its own in the list',
                    is_outgoing: false, is_info: false, show_padlock: true,
                    state: 16, timestamp: 1700000000 + rows.count,
                    day_number: 19675, sender_name: 'Ada',
                    sender_color: '#00875a', quote_text: '', quote_author: '',
                    file_path: '', file_name: '', view_type: 'Text',
                    image_width: 0, image_height: 0
                })
            }
            return '' + rows.count
        }

        property string raised: ''
        property int arrivals: 0

        function load(url) {
            loader.setSource(url, { model: rows, title: 'Ada' })
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            var view = loader.item
            // As the page binds it.
            view.messageCount = Qt.binding(function() { return rows.count })
            view.replyRequested.connect(function(id, body, author) {
                raised = 'reply:' + id + ':' + body + ':' + author
            })
            view.copyRequested.connect(function(body) { raised = 'copy:' + body })
            view.deleteRequested.connect(function(id) { raised = 'delete:' + id })
            view.resendRequested.connect(function(id) { raised = 'resend:' + id })
            view.arrivedAtNewest.connect(function() { arrivals += 1 })
            return 'ok'
        }
        function raisedSignal() { return raised }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            if (node.contentItem && node.contentItem !== node) {
                return findIn(node.contentItem, name)
            }
            return null
        }
        // The menu is a property of the row, not a visual child of it.
        function pickMenu(name) {
            var row = findIn(loader.item, 'messageRow')
            if (!row) { return 'missing:messageRow' }
            if (!row.menu) { return 'no-menu' }
            var item = findIn(row.menu, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        function get(property) { return '' + loader.item[property] }
        function set(property, value) {
            loader.item[property] = value
            return '' + loader.item[property]
        }
        // Scrolling up into the history, the way a reader would: the view's
        // own call, so it re-anchors rather than being shoved by contentY.
        function toTop() { loader.item.positionViewAtBeginning(); return 'ok' }
        // Silica raises these around a drag. A row is measured as it comes
        // into view, so scrolling up grows contentHeight by itself.
        function beginDrag() { loader.item.movementStarted(); return 'ok' }
        function arrivedCount() { return '' + arrivals }
        function toBottom() { loader.item.positionViewAtEnd(); return 'ok' }
        function jump() { loader.item.jumpToNewest(); return 'ok' }
        // Silica sends this when a drag or flick settles.
        function settle() { loader.item.movementEnded(); return 'ok' }
        // The view's own answer to 'is the end on screen'.
        function ended() { return '' + loader.item.atYEnd }
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
fn a_conversation_opens_at_the_newest_message_and_stays_where_it_is_left() {
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
        // A history longer than the view, as when reopening a chat.
        record!("filled", call!("append", 40));
        record!(
            "load",
            call!("load", QString::from(component_url("ConversationList.qml")))
        );
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        record!("opened-at-end", call!("ended"));
        record!(
            "opened-sticky",
            call!("get", QString::from("stickToBottom"))
        );
        // One more message arrives while the reader is at the bottom.
        call!("append", 1);
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("followed", call!("ended"));

        // The reader scrolls up into the history and stops there.
        call!("toTop");
        call!("settle");
        record!(
            "scrolled-sticky",
            call!("get", QString::from("stickToBottom"))
        );
        call!("append", 1);
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!("stayed-put", call!("ended"));

        // The reader scrolls back down to the newest message.
        call!("toBottom");
        call!("settle");
        record!(
            "returned-sticky",
            call!("get", QString::from("stickToBottom"))
        );
        call!("append", 1);
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        record!("resumed", call!("ended"));

        // The menu raises what the page then acts on.
        record!(
            "reply-picked",
            call!("pickMenu", QString::from("replyItem"))
        );
        record!("reply", call!("raisedSignal"));
        call!("pickMenu", QString::from("copyItem"));
        record!("copy", call!("raisedSignal"));
        call!("pickMenu", QString::from("deleteItem"));
        record!("delete", call!("raisedSignal"));
        call!("pickMenu", QString::from("resendItem"));
        record!("resend", call!("raisedSignal"));

        // Scrolled away again, so an arrival is counted rather than shown.
        call!("toTop");
        call!("settle");
        call!("append", 2);
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("missed", call!("get", QString::from("missedCount")));

        call!("jump");
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        record!("jumped", call!("ended"));
        record!("missed-cleared", call!("get", QString::from("missedCount")));
        record!("arrived", call!("arrivedCount"));

        // Following the newest message, as after opening a chat. The
        // reader takes hold and drags up into the history.
        call!("beginDrag");
        record!("held-follows", call!("get", QString::from("following")));
        call!("toTop");
        // A row measured mid-drag, or a message arriving: either moves
        // `contentHeight`, which is what hauled the reader back down.
        call!("append", 1);
    });

    single_shot(Duration::from_secs(8), move || unsafe {
        record!("held-stayed", call!("ended"));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps);
}

/// Opening lands on the newest message; an arrival moves the view only
/// when the reader is already there.
// One assertion per thing checked, in the order the steps ran.
#[allow(clippy::too_many_lines)]
fn assert_outcome(steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the list did not load. {context}");
    assert_eq!(value("filled"), "40", "the model did not fill. {context}");
    assert_eq!(
        value("opened-at-end"),
        "true",
        "reopening a chat put the reader back at the top of the history. {context}"
    );
    assert_eq!(
        value("opened-sticky"),
        "true",
        "a freshly opened chat does not follow new messages. {context}"
    );
    assert_eq!(
        value("followed"),
        "true",
        "a message arriving while the reader was at the bottom did not scroll in. {context}"
    );
    assert_eq!(
        value("scrolled-sticky"),
        "false",
        "scrolling up into the history left the view still following. {context}"
    );
    assert_eq!(
        value("stayed-put"),
        "false",
        "a message arriving pulled the reader out of the history. {context}"
    );
    assert_eq!(
        value("returned-sticky"),
        "true",
        "scrolling back to the newest message did not resume following. {context}"
    );
    assert_eq!(
        value("resumed"),
        "true",
        "the next message did not scroll in after following resumed. {context}"
    );

    assert_eq!(
        value("reply-picked"),
        "ok",
        "the row has no context menu. {context}"
    );
    // The reply carries what the page needs to show what is being answered.
    assert_eq!(
        value("reply"),
        "reply:0:message number 0, long enough to take a line of its own in the list:Ada",
        "Reply did not name the message it is replying to. {context}"
    );
    assert!(
        value("copy").starts_with("copy:message number 0"),
        "Copy did not carry the message's text: {}. {context}",
        value("copy")
    );
    assert_eq!(
        value("delete"),
        "delete:0",
        "Delete did not name its message. {context}"
    );
    assert_eq!(
        value("resend"),
        "resend:0",
        "Send again did not name its message. {context}"
    );

    assert_eq!(
        value("missed"),
        "2",
        "messages arriving out of sight were not counted. {context}"
    );
    assert_eq!(
        value("held-follows"),
        "false",
        "the view still follows while the reader has hold of it, which hauls \
         them back down the moment they scroll up. {context}"
    );
    assert_eq!(
        value("held-stayed"),
        "false",
        "the reader was dragged back to the newest message mid-drag. {context}"
    );
    assert!(
        value("arrived").parse::<i32>().unwrap_or_default() >= 1,
        "reaching the newest message was not announced, so nothing marks it \
         read. {context}"
    );
    assert_eq!(
        value("jumped"),
        "true",
        "jumping to the newest message did not move the view. {context}"
    );
    assert_eq!(
        value("missed-cleared"),
        "0",
        "the count of missed messages survived jumping to them. {context}"
    );
}

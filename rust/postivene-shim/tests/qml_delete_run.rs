//! Deleting several messages, one after another.
//!
//! Each one waits a moment before it goes, so the reader can say they
//! did not mean it. That wait used to belong to the row -- Silica's
//! `ListItem.remorseAction` puts it there -- and a row does not outlive
//! the thing this is used for: deleting a message destroys a row, and
//! the delete before it in a run is what destroys the row the next one
//! is counting down on. Deleting a handful in a row lost most of them.
//!
//! So the wait belongs to the list, which outlives every row in it, and
//! what this pins is that nothing happening to the rows can lose a
//! delete: a row destroyed mid-countdown, a delete asked for while
//! others are already waiting, and the reader leaving the chat before
//! the wait is up.
//!
//! And the way back: a tap on a message on its way out puts it back, and
//! puts back only that one.

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

        ListModel { id: rows }
        Loader { id: list; width: 540; height: 900 }

        /// Every delete the list asked for, in order.
        property string sent: ''

        function load(url, count) {
            for (var i = 0; i < count; i++) {
                rows.append({
                    message_id: i + 1,
                    text: 'message number ' + (i + 1),
                    is_outgoing: false, is_info: false, show_padlock: true,
                    state: 16, timestamp: 1700000000 + i,
                    day_number: 19675, sender_name: 'Ada',
                    sender_color: '#00875a', quote_text: '',
                    quote_author: '', file_path: '', file_name: '',
                    view_type: 'Text', image_width: 0, image_height: 0,
                    // Without this the delegate's own `visible` is
                    // handed an undefined and keeps whatever it had, so
                    // a message on its way out is still drawn.
                    loaded: true
                })
            }
            list.setSource(url, { model: rows })
            if (list.status !== Loader.Ready) { return 'load-failed' }
            list.item.deleteRequested.connect(function(id) {
                sent += id + ';'
            })
            // Long enough that a step of this test lands inside one
            // wait, short enough that it does not sit through four
            // seconds of the real one.
            list.item.pendingDelay = 2500
            return 'ok'
        }
        function sentSoFar() { return sent }

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
        function findAll(node, name, out) {
            if (!node) { return out }
            if (node.objectName === name) { out.push(node) }
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {
                findAll(kids[i], name, out)
            }
            return out
        }
        /// The row showing this message. A row does not say which message
        /// it holds, but the menu's reaction strip does, and the menu
        /// belongs to the row.
        function rowFor(messageId) {
            var found = findAll(list.item, 'messageRow', [])
            for (var i = 0; i < found.length; i++) {
                var menu = found[i].menu
                var picker = menu ? findIn(menu, 'reactionPicker') : null
                if (picker && picker.messageId === messageId) {
                    return found[i]
                }
            }
            return null
        }
        /// Delete, from that message's own menu.
        function deleteRow(messageId) {
            var row = rowFor(messageId)
            if (!row) { return 'missing:row:' + messageId }
            var item = findIn(row.menu, 'deleteItem')
            if (!item) { return 'missing:deleteItem' }
            item.clicked()
            return 'ok'
        }
        /// A tap on the message, which is the way back.
        function tapRow(messageId) {
            var row = rowFor(messageId)
            if (!row) { return 'missing:row:' + messageId }
            row.clicked()
            return 'ok'
        }
        /// Whether that message is drawn as one on its way out.
        function goingOut(messageId) {
            var row = rowFor(messageId)
            if (!row) { return 'missing:row:' + messageId }
            var going = findIn(row, 'doomedRow')
            var body = findIn(row, 'messageLabel')
            if (!going || !body) { return 'missing:parts' }
            return going.visible + '/' + body.visible
        }
        /// What a row on its way out says.
        function goingText(messageId) {
            var row = rowFor(messageId)
            if (!row) { return 'missing:row:' + messageId }
            var label = findIn(row, 'doomedLabel')
            return label ? label.text : 'missing:doomedLabel'
        }
        // Destroys a delegate mid-countdown, as the core's own event does
        // once the delete before it has landed.
        function removeRow(index) { rows.remove(index); return 'ok' }
        function rowCount() { return '' + rows.count }
        // What the page does on its way out of the chat.
        function leaveChat() { list.item.flushDeletes(); return 'ok' }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn every_delete_in_a_run_arrives() {
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
                QString::from(common::component_url("ConversationList.qml")),
                5
            )
        );
    });

    // A run: three deletes inside one wait, with the list changing under
    // them in the middle -- which is what the first delete of a run does
    // to the rows the rest are waiting on.
    single_shot(Duration::from_secs(2), move || unsafe {
        record!("delete-1", call!("deleteRow", 1));
        record!("delete-2", call!("deleteRow", 2));
        record!("going-1", call!("goingOut", 1));
        record!("going-text", call!("goingText", 1));
        record!("untouched-4", call!("goingOut", 4));
        record!("sent-so-far", call!("sentSoFar"));
        call!("removeRow", 0);
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("after-removal", call!("sentSoFar"));
        record!("delete-3", call!("deleteRow", 3));
        // And one the reader changes their mind about, in the same run.
        record!("delete-4", call!("deleteRow", 4));
        record!("spare-4", call!("tapRow", 4));
        record!("spared-4", call!("goingOut", 4));
        record!("still-going-3", call!("goingOut", 3));
    });

    // Past the wait: everything asked for has gone, and only that.
    single_shot(Duration::from_secs(6), move || unsafe {
        record!("sent", call!("sentSoFar"));
        record!("back-4", call!("goingOut", 4));
        // The reader asks for one more and leaves the chat before the
        // wait is up.
        record!("delete-5", call!("deleteRow", 5));
        record!("leaving", call!("sentSoFar"));
        call!("leaveChat");
        record!("left", call!("sentSoFar"));
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
    for label in ["delete-1", "delete-2", "delete-3", "delete-4", "delete-5"] {
        assert_eq!(
            value(label),
            "ok",
            "{label} did not reach the row's menu. {context}"
        );
    }

    assert_eq!(
        value("going-1"),
        "true/false",
        "a message asked for and waiting to go is still drawn as a \
         message, or has nothing in its place. {context}"
    );
    assert_eq!(
        value("going-text"),
        "Deleting",
        "a message on its way out does not say what is happening to it. \
         {context}"
    );
    assert_eq!(
        value("untouched-4"),
        "false/true",
        "asking for one message to go marked another as going too. \
         {context}"
    );
    assert_eq!(
        value("sent-so-far"),
        "",
        "a message went the moment it was asked for, so there was never \
         a moment to change your mind. {context}"
    );
    assert_eq!(
        value("after-removal"),
        "",
        "destroying a row sent the deletes early: the wait is the \
         reader's, and a row going away is not them changing their mind. \
         {context}"
    );

    assert_eq!(
        value("spare-4"),
        "ok",
        "a message on its way out did not take the tap that puts it \
         back. {context}"
    );
    assert_eq!(
        value("spared-4"),
        "false/true",
        "tapping a message on its way out did not put it back. {context}"
    );
    assert_eq!(
        value("still-going-3"),
        "true/false",
        "putting one message back put another one back with it. {context}"
    );

    // The run: three asked for, one taken back, and a row destroyed in
    // the middle of all of it.
    let sent_text = value("sent");
    let sent: Vec<&str> = sent_text
        .split(';')
        .filter(|part| !part.is_empty())
        .collect();
    let mut ordered = sent.clone();
    ordered.sort_unstable();
    assert_eq!(
        ordered,
        ["1", "2", "3"],
        "a run of deletes did not all arrive, or one nobody asked for \
         did: {sent:?}. This is the whole bug -- the first delete of a \
         run destroys the rows the rest are waiting on. {context}"
    );
    assert_eq!(
        value("back-4"),
        "false/true",
        "the message that was put back went anyway. {context}"
    );

    assert_eq!(
        value("leaving"),
        value("sent"),
        "the last delete went before the wait was up. {context}"
    );
    assert_eq!(
        value("left"),
        format!("{}5;", value("sent")),
        "leaving the chat with a delete still waiting lost it. A reader \
         who asked for a message to go and then left asked for it to go. \
         {context}"
    );
}

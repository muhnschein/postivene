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
                    // 1-based and distinct: with every id 0 an assertion on
                    // one cannot tell a right answer from a missing role.
                    message_id: rows.count + 1,
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
            // As the page does: the model says what arrived, the view
            // counts it. Differencing the row count instead would count a
            // message the reader sent, and miss an arrival that landed in
            // the same reload as a deletion.
            if (loader.item) { loader.item.noteArrivals(count) }
            return '' + rows.count
        }

        // A row the reader sent. It moves the count without being an
        // arrival, so nothing tells the view about it.
        function appendOwn() {
            rows.append({
                message_id: rows.count + 1, text: 'mine',
                is_outgoing: true, is_info: false, show_padlock: true,
                state: 26, timestamp: 1700000000 + rows.count,
                day_number: 19675, sender_name: 'Me',
                sender_color: '#00875a', quote_text: '', quote_author: '',
                file_path: '', file_name: '', view_type: 'Text',
                image_width: 0, image_height: 0
            })
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
        // Whether Send again is offered for a row in this delivery state.
        // Clicking it says nothing about that: findIn reaches an invisible
        // item just as well, so the gate needs asking about directly --
        // and the row has to be laid out again first, which is why setting
        // it up and reading it are separate calls a tick apart.
        function resetToState(state) {
            rows.clear()
            rows.append({
                message_id: 1, text: 'one', is_outgoing: true,
                is_info: false, show_padlock: true, state: state,
                timestamp: 1700000000, day_number: 19675,
                sender_name: 'Me', sender_color: '#00875a',
                quote_text: '', quote_author: '', file_path: '',
                file_name: '', view_type: 'Text',
                image_width: 0, image_height: 0
            })
            return 'ok'
        }
        function resendVisible() {
            var row = findIn(loader.item, 'messageRow')
            if (!row || !row.menu) { return 'no-menu' }
            var item = findIn(row.menu, 'resendItem')
            return item ? '' + item.visible : 'missing'
        }
        function clearRaised() { raised = ''; return 'ok' }
        function emptyModel() { rows.clear(); return '' + rows.count }
        // Destroys the delegate the menu belongs to, as a reload or a
        // reorder does.
        function removeRow(index) { rows.remove(index); return 'ok' }
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
        // Stop a little short of the end, the way a real scroll does while
        // rows are still being measured.
        function stopJustShort(gap) {
            var view = loader.item
            view.positionViewAtEnd()
            view.contentY = view.contentY - gap
            return '' + view.atYEnd
        }
        function near() { return '' + loader.item.nearBottom }
        // The placeholder's own enabled state, read off the item rather
        // than recomputed -- a probe that repeated the binding would pass
        // whatever the component did.
        function setLoaded(value) { loader.item.loaded = value; return 'ok' }
        function placeholderOn() {
            var item = findIn(loader.item, 'emptyPlaceholder')
            return item ? '' + item.enabled : 'missing:emptyPlaceholder'
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

// A script of timed steps, in the order they happen; splitting it would
// hide that order for no gain.
#[allow(clippy::too_many_lines)]
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
        call!("pickMenu", QString::from("resendItem"));
        record!("resend", call!("raisedSignal"));
        // Deferred by the stub, as Silica defers it: read on the next step.
        call!("pickMenu", QString::from("deleteItem"));

        // Scrolled away again, so an arrival is counted rather than shown.
        call!("toTop");
        call!("settle");
        call!("append", 2);
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("delete", call!("raisedSignal"));
        record!("missed", call!("get", QString::from("missedCount")));

        // The reader's own message is not one they missed.
        call!("appendOwn");
        record!(
            "missed-after-own",
            call!("get", QString::from("missedCount"))
        );

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

        // Away from the newest message, then a flick still gliding when
        // the jump button is tapped. The button is not part of the list,
        // so nothing else stops that flick.
        call!("toTop");
        call!("settle");
        call!("beginDrag");
        call!("jump");
    });

    single_shot(Duration::from_secs(9), move || unsafe {
        record!("jumped-mid-flick", call!("ended"));

        // Picked, then the row destroyed before the countdown ends -- which
        // is what a reload or a reorder does to it. Silica runs the action
        // on the way out, so it must still name the message that was
        // picked. Cleared first: a stale value would otherwise read as a
        // fresh one and the assertion would hold either way.
        call!("clearRaised");
        call!("pickMenu", QString::from("deleteItem"));
        call!("removeRow", 0);
    });

    single_shot(Duration::from_secs(10), move || unsafe {
        record!("delete-after-removal", call!("raisedSignal"));

        // Last, because it replaces the model's contents.
        call!("resetToState", 24);
    });

    single_shot(Duration::from_secs(11), move || unsafe {
        record!("resend-when-failed", call!("resendVisible"));
        call!("resetToState", 26);
    });

    single_shot(Duration::from_secs(12), move || unsafe {
        record!("resend-when-delivered", call!("resendVisible"));
        // Refill: resetToState left a handful of rows, and a list too
        // short to scroll is near the bottom by definition -- there would
        // be nothing to scroll away from.
        call!("append", 40);
        call!("beginDrag");
        call!("toTop");
        call!("settle");
    });

    single_shot(Duration::from_secs(13), move || unsafe {
        record!("away", call!("get", QString::from("stickToBottom")));
        call!("beginDrag");
        record!("short-atyend", call!("stopJustShort", 12.0));
        record!("short-near", call!("near"));
        call!("settle");
    });

    single_shot(Duration::from_secs(14), move || unsafe {
        record!("short-sticks", call!("get", QString::from("stickToBottom")));
        // Empty the list and put it back to not-yet-loaded, which is what
        // opening a chat looks like before its messages arrive.
        call!("emptyModel");
        call!("setLoaded", false);
    });

    single_shot(Duration::from_secs(15), move || unsafe {
        record!("placeholder-while-loading", call!("placeholderOn"));
        call!("setLoaded", true);
    });

    single_shot(Duration::from_secs(16), move || unsafe {
        record!("placeholder-when-empty", call!("placeholderOn"));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps);
    assert_near_bottom(&steps);
}

/// Opening lands on the newest message; an arrival moves the view only
/// when the reader is already there.
///
/// And stopping a line short of the newest message counts as arriving:
/// the jump-to-newest button could not otherwise be dismissed.
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
        "reply:1:message number 0, long enough to take a line of its own in the list:Ada",
        "Reply did not name the message it is replying to. {context}"
    );
    assert!(
        value("copy").starts_with("copy:message number 0"),
        "Copy did not carry the message's text: {}. {context}",
        value("copy")
    );
    assert_eq!(
        value("delete"),
        "delete:1",
        "Delete did not name its message. {context}"
    );
    assert_eq!(
        value("resend"),
        "resend:1",
        "Send again did not name its message. {context}"
    );

    assert_eq!(
        value("delete-after-removal"),
        "delete:1",
        "a delete whose row was destroyed mid-countdown did not name the \
         message that was picked -- read from the delegate as it went, \
         `model` resolves to nothing and the deletion is dropped. {context}"
    );

    // DC_STATE_OUT_FAILED is the only state worth offering it in. Clicking
    // the item proves it is wired; whether a reader can ever reach it is a
    // separate question, and was not being asked.
    assert_eq!(
        value("resend-when-failed"),
        "true",
        "Send again is hidden on a message that failed, which is the one \
         case it exists for. {context}"
    );
    assert_eq!(
        value("resend-when-delivered"),
        "false",
        "Send again is offered on a message that was delivered. {context}"
    );

    assert_eq!(
        value("missed"),
        "2",
        "messages arriving out of sight were not counted. {context}"
    );
    assert_eq!(
        value("missed-after-own"),
        "2",
        "a message the reader sent themselves was counted as one they \
         missed, which is what differencing the row count does. {context}"
    );
    assert_eq!(
        value("held-follows"),
        "false",
        "the view still follows while the reader has hold of it, which hauls \
         them back down the moment they scroll up. {context}"
    );
    // The jump has to move the view even when the reader tapped it during
    // a flick: it clears the missed badge and tells the page the newest
    // message has been reached, and doing that without scrolling marks
    // messages read that are still out of sight.
    assert_eq!(
        value("jumped-mid-flick"),
        "true",
        "tapping jump-to-newest during a flick left the view up in the \
         history, having already cleared the badge and reported arrival. \
         {context}"
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

/// A reader who stops just short of the end has arrived, so the
/// jump-to-newest button goes away.
fn assert_near_bottom(steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("away"),
        "false",
        "scrolling up did not clear stickToBottom, so nothing below is under \
         test. {context}"
    );
    // The arrangement: genuinely short of the end by the view's own
    // reckoning. Without this the case being tested is not the one that
    // was reported.
    assert_eq!(
        value("short-atyend"),
        "false",
        "stopping short still counted as atYEnd, so this no longer \
         reproduces the button that would not go away. {context}"
    );
    assert_eq!(
        value("short-near"),
        "true",
        "a reader a few pixels from the end is not counted as near it. {context}"
    );
    assert_eq!(
        value("placeholder-while-loading"),
        "false",
        "\"no messages yet\" is shown while the chat is still loading, which \
         is the flash of an empty chat seen when opening a busy one. {context}"
    );
    assert_eq!(
        value("placeholder-when-empty"),
        "true",
        "a chat that really is empty says nothing at all. {context}"
    );
    assert_eq!(
        value("short-sticks"),
        "true",
        "settling just short of the newest message left the jump button up, \
         with no way to dismiss it. {context}"
    );
}

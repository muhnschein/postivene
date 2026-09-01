//! Opening a long chat somewhere other than its end.
//!
//! Two reports from a device, and one cause. Tapping a search result showed
//! today's messages for a moment before yanking the reader up to the one
//! they had asked for -- because the chat opened on its newest page and
//! then walked back. And the system's own scroll-to-top gesture landed at
//! the top of whatever happened to be loaded and called it the top of the
//! chat, which in a chat with ten thousand messages in it is out by years.
//!
//! Both are answered by the window having two ends. A search result opens
//! the page it is on, with the newest messages not loaded at all; the top
//! of the list offers the beginning of the chat, which is somewhere the
//! window can be moved to rather than something it has to grow to reach.
//!
//! `chat_paging.rs` has the model's side of this. What only a view can
//! answer is whether the rows are on the page as it is built -- there is no
//! flash to see headlessly, but a page built holding the found message and
//! not today's is a page with nothing to flash.

// Qt harness: see qml_conversation_open.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use postivene_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// Long enough that the message below is nowhere near either end.
const MESSAGES: u32 = 130;
/// The message a search found, in the middle of the chat.
const FOUND: u32 = 65;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import Postivene 1.0
    Item {
        Loader { id: loader }
        Repeater {
            id: mirror
            Item { property int mid: model.message_id }
        }
        property int readyFor: 0
        ChatPrefetch {
            id: prefetch
            account_id: 1
            onReady: readyFor = chat_id
        }
        // What the chat list does when a search result is tapped.
        function prefetchAt(chatId, messageId) {
            prefetch.start(chatId, messageId)
            return 'ok'
        }
        function prefetched() { return '' + readyFor }

        function load(url, accountId, chatId, findId) {
            loader.setSource('', {})
            loader.setSource(url, {
                accountId: accountId,
                chatId: chatId,
                findMessageId: findId,
                status: PageStatus.Activating
            })
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            mirror.model = find('messages').rows
            return 'ok'
        }
        function settle() { loader.item.status = PageStatus.Active; return 'ok' }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            if (node.contentItem && node.contentItem !== node) {
                return findIn(node.contentItem, name)
            }
            return null
        }
        function find(name) { return findIn(loader.item, name) }
        function get(name, property) {
            var item = find(name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        /// The oldest and newest loaded row, which is what says where in the
        /// chat the page is.
        function edges() {
            if (mirror.count === 0) { return 'empty' }
            return mirror.itemAt(0).mid + '..' + mirror.itemAt(mirror.count - 1).mid
        }
        /// The control at the top of the list, tapped.
        function tapBeginning() {
            var item = find('toOldest')
            if (!item) { return 'missing:toOldest' }
            if (!item.visible) { return 'not-offered' }
            item.clicked()
            return 'ok'
        }
    }
";

#[test]
fn a_search_result_is_on_the_page_and_the_beginning_is_a_tap_away() {
    let temp = std::env::temp_dir().join(format!("postivene-qml-window-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("POSTIVENE_FAKE_LONG_CHAT", MESSAGES.to_string());
    }

    postivene_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

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
        (*steps_ptr).push(("prefetch", call!("prefetchAt", 1, i32::try_from(FOUND).unwrap_or(0))));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push(("prefetched", call!("prefetched")));
        (*steps_ptr).push((
            "load",
            call!(
                "load",
                QString::from(common::page_url_in(&tree, "ConversationPage.qml")),
                1,
                1,
                i32::try_from(FOUND).unwrap_or(0)
            ),
        ));
        // In the same callback as the load: whatever is here was put here
        // by the page building itself, with no turn in between for a
        // transition to run, let alone for a jump to be seen.
        (*steps_ptr).push(("edges-while-arriving", call!("edges")));
        (*steps_ptr).push(("settle", call!("settle")));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        (*steps_ptr).push((
            "offers-newer",
            call!("get", QString::from("messages"), QString::from("has_newer")),
        ));
        (*steps_ptr).push((
            "offers-older",
            call!("get", QString::from("messages"), QString::from("has_older")),
        ));
        (*steps_ptr).push(("to-beginning", call!("tapBeginning")));
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        (*steps_ptr).push(("beginning-edges", call!("edges")));
        (*steps_ptr).push((
            "beginning-has-older",
            call!("get", QString::from("messages"), QString::from("has_older")),
        ));
        (*steps_ptr).push((
            "beginning-has-newer",
            call!("get", QString::from("messages"), QString::from("has_newer")),
        ));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps);
}

/// What the run has to show for itself, out of the test body.
fn assert_outcome(steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the page did not load. {context}");
    assert_eq!(
        value("prefetched"),
        "1",
        "the prefetch never answered, so this run proves nothing about what \
         a page does with one. {context}"
    );

    // Half a page above message 65 and half a page below: the window is
    // centred on what was searched for, and today is not in it.
    assert_eq!(
        value("edges-while-arriving"),
        "40..89",
        "the page was not built on the message the search found. Ending at \
         {MESSAGES} means it opened on the newest page and would have had \
         to jump off it, which is the flash of today's messages this is \
         here to stop. {context}"
    );

    assert_eq!(
        value("offers-newer"),
        "true",
        "the window is in the middle of the chat and the model does not \
         offer the way back to the newest messages. {context}"
    );
    assert_eq!(
        value("offers-older"),
        "true",
        "there are 39 messages above the window and the model says there \
         are none. {context}"
    );

    // The other report: the top of the list is not the top of the chat, and
    // has to offer it.
    assert_eq!(
        value("to-beginning"),
        "ok",
        "nothing at the top of the list offers the beginning of the chat, \
         so the system's scroll-to-top lands on the top of whatever is \
         loaded and says nothing about the rest. {context}"
    );
    assert_eq!(
        value("beginning-edges"),
        "1..50",
        "going to the beginning of the chat did not land on its first \
         messages. {context}"
    );
    assert_eq!(
        value("beginning-has-older"),
        "false",
        "the window is on the first message in the chat and the model \
         still offers older ones. {context}"
    );
    assert_eq!(
        value("beginning-has-newer"),
        "true",
        "the window is at the beginning of a {MESSAGES}-message chat and \
         the model does not offer the way back. {context}"
    );
}

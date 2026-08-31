//! Stepping back through a long chat, on the real page.
//!
//! The model side is `chat_paging.rs`. What can only be checked with a view
//! is the part that decides whether paging is usable at all: rows inserted
//! above the ones on screen push them down by their own height, and that
//! height is not known until they are laid out. If nothing puts the view
//! back, reaching the top of a chat throws the reader further back than
//! they asked to go, once per page, for ever.

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

/// Two pages and a bit, so the first step back is not also the last.
const MESSAGES: u32 = 130;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }
        // Mirrors the page's model, so a row index can be turned back into
        // a message id without reaching into a delegate.
        Repeater {
            id: mirror
            Item { property int mid: model.message_id }
        }
        function load(url, accountId, chatId) {
            loader.setSource('', {})
            loader.setSource(url, {
                accountId: accountId,
                chatId: chatId,
                status: PageStatus.Activating
            })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function settle() {
            loader.item.status = PageStatus.Active
            mirror.model = find('messages').rows
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
        function rowCount() { return '' + mirror.count }
        /// The message id of the row at the top of the view -- what the
        /// reader is looking at, and what must not move.
        function topId() {
            var list = find('messageList')
            if (!list) { return 'missing:messageList' }
            // Downwards from the top edge until a row answers: above the
            // oldest row sits the header that carries the spinner, and
            // between rows sit the day separators, and indexAt reports
            // neither as a row.
            for (var y = 1; y < list.height; y += 8) {
                var index = list.indexAt(list.width / 2, list.contentY + y)
                if (index >= 0 && index < mirror.count) {
                    return '' + mirror.itemAt(index).mid
                }
            }
            return 'no-row'
        }
        /// To the top and stop following, which is what a reader dragging
        /// back through the history leaves behind -- and what asks for the
        /// page above.
        function toTop() {
            find('messageList').jumpToRow(0)
            return 'ok'
        }
    }
";

#[test]
fn reaching_the_top_brings_in_more_without_moving_the_reader() {
    let temp = std::env::temp_dir().join(format!("postivene-qml-paging-{}", std::process::id()));
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
        (*steps_ptr).push((
            "load",
            call!(
                "load",
                QString::from(common::page_url_in(&tree, "ConversationPage.qml")),
                1,
                1
            ),
        ));
    });
    single_shot(Duration::from_secs(2), move || unsafe {
        (*steps_ptr).push(("settle", call!("settle")));
    });
    single_shot(Duration::from_secs(4), move || unsafe {
        (*steps_ptr).push(("opened-rows", call!("rowCount")));
        (*steps_ptr).push((
            "spinner-offered",
            call!("get", QString::from("olderBusy"), QString::from("visible")),
        ));
        // To the top, which is what asks for the page above.
        (*steps_ptr).push(("scrolled", call!("toTop")));
        // Read in the same callback: this is the row the reader is looking
        // at when the request goes out, and the one that must not move.
        (*steps_ptr).push(("top-before", call!("topId")));
    });
    single_shot(Duration::from_secs(6), move || unsafe {
        (*steps_ptr).push(("after-rows", call!("rowCount")));
        (*steps_ptr).push(("top-after", call!("topId")));
    });

    single_shot(Duration::from_secs(8), move || unsafe {
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
        value("opened-rows"),
        "50",
        "a {MESSAGES}-message chat did not open on one page. {context}"
    );
    assert_eq!(
        value("spinner-offered"),
        "true",
        "there is more history and nothing above the oldest row says so. {context}"
    );

    assert_eq!(
        value("after-rows"),
        "100",
        "reaching the top of the loaded messages did not bring in the page \
         above them. {context}"
    );
    // The whole point. Without putting the view back, fifty rows go in
    // above the reader and take the view with them.
    assert_eq!(
        value("top-after"),
        value("top-before"),
        "the rows that arrived above the reader carried the view off with \
         them: they were looking at message {:?} and ended up at {:?}. \
         {context}",
        value("top-before"),
        value("top-after")
    );
    assert_ne!(
        value("top-before"),
        "no-row",
        "the test never established which row the reader was looking at, so \
         it proved nothing. {context}"
    );
}

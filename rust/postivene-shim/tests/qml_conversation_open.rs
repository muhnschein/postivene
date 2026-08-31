//! Opening a chat must not start fetching it mid-transition.
//!
//! A binding from `page.chatId` to the model started the fetch the moment
//! the page was created -- while it was still transitioning in. Building
//! every row of a long history in one go on the Qt thread is what made
//! that transition stutter, freeze, and then continue. The chat is handed
//! over once the page reports Active instead.
//!
//! The page is loaded from a copy of the QML tree with `EnterKey` taken
//! out; see `common::qml_tree_without_enter_key` for why the shipped file
//! cannot be loaded headlessly as it stands.

// Qt harness: see qml_chat_list.rs.
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

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }
        // Created the way pageStack.push does, still on its way in.
        function loadInactive(url, accountId, chatId) {
            loader.setSource('', {})
            loader.setSource(url, {
                accountId: accountId,
                chatId: chatId,
                status: PageStatus.Activating
            })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
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
        function chatOfModel() {
            var model = findIn(loader.item, 'messages')
            return model ? '' + model.chat_id : 'missing:messages'
        }
    }
";

#[test]
fn a_chat_is_not_fetched_until_its_page_has_arrived() {
    let temp = std::env::temp_dir().join(format!("postivene-open-chat-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
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
                "loadInactive",
                QString::from(common::page_url_in(&tree, "ConversationPage.qml")),
                1,
                1
            ),
        ));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push(("while-arriving", call!("chatOfModel")));
        (*steps_ptr).push(("settle", call!("settle")));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        (*steps_ptr).push(("after-arriving", call!("chatOfModel")));
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

    assert_eq!(
        value("load"),
        "ok",
        "the conversation page did not load. {context}"
    );
    assert_eq!(
        value("while-arriving"),
        "0",
        "the fetch started while the page was still transitioning in, which is \
         what makes opening a busy chat stutter. {context}"
    );
    assert_eq!(
        value("after-arriving"),
        "1",
        "the chat was never handed to the model once the page arrived, so it \
         would stay empty for ever. {context}"
    );
}

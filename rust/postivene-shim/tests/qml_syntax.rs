//! Guards QML rules that no host-Qt run can check: syntax newer than
//! Sailfish's Qt 5.6, and list rows whose height ignores their text.
//!
//! A text scan on purpose: host Qt 5.15 accepts the newer form and only
//! warns, while on device the handlers silently never fire.

use std::fs;
use std::path::{Path, PathBuf};

fn qml_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "qml") {
                out.push(path);
            }
        }
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml");
    let mut files = Vec::new();
    walk(&root, &mut files);
    files.sort();
    files
}

#[test]
fn qml_avoids_qt_5_15_only_signal_handler_syntax() {
    let files = qml_files();
    assert!(!files.is_empty(), "found no .qml files to check");

    let mut offenders = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file).expect("read qml");
        for (number, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("function on") {
                offenders.push(format!("{}:{}: {trimmed}", file.display(), number + 1));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "`function onFoo() {{ ... }}` inside Connections is Qt 5.15+ syntax. \
         Sailfish runs Qt 5.6, where it is not an error but is never connected, \
         so the handler silently never runs. Use `onFoo: {{ ... }}` with the \
         shim's snake_case parameter names instead.\n  {}",
        offenders.join("\n  ")
    );
}

/// A list row holding a wrapping `Label` must take its height from that
/// label. With a constant `contentHeight` a long message -- a device
/// message runs to a dozen wrapped lines -- overlaps its neighbours and the
/// header.
///
/// A scan rather than a measurement: `tests/qml_conversation_list.rs` loads
/// the list, but a row collapsed to nothing would leave its assertions
/// about scrolling passing anyway.
#[test]
fn wrapping_list_rows_size_to_their_text() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components/ConversationList.qml");
    let text = fs::read_to_string(&path).expect("read ConversationList.qml");

    let height = text
        .lines()
        .find(|line| line.trim_start().starts_with("contentHeight:"))
        .unwrap_or("");
    assert!(
        height.contains("body.height"),
        "the message row's contentHeight does not follow the delegate it \
         holds; what the delegate itself measures is in \
         tests/qml_conversation.rs: {height:?}"
    );
}

/// Every `model.<role>` the conversation delegate binds has to be a field
/// the model actually has: a rename on either side is otherwise a blank
/// message row that nothing catches.
#[test]
fn the_conversation_delegate_binds_only_real_message_roles() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components/ConversationList.qml");
    let text = fs::read_to_string(&path).expect("read ConversationList.qml");

    let roles: Vec<String> =
        <postivene_shim::MessageListItem as qmetaobject::listmodel::SimpleListItem>::names()
            .iter()
            .map(std::string::ToString::to_string)
            .collect();

    let mut bound = Vec::new();
    for tail in text.split("model.").skip(1) {
        let role: String = tail
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if !role.is_empty() {
            bound.push(role);
        }
    }
    assert!(
        !bound.is_empty(),
        "the delegate binds nothing from the model any more"
    );
    for role in &bound {
        assert!(
            roles.contains(role),
            "the delegate binds model.{role}, which MessageListItem does not have: {roles:?}"
        );
    }
}

/// A list draws its delegates outside its own box unless told not to, and
/// both list pages sit above translucent things -- a message field, an
/// error banner -- that the content then shows through.
#[test]
fn list_pages_clip_and_leave_room_for_what_sits_below_them() {
    for (page, list) in [
        ("ConversationPage.qml", "../components/ConversationList.qml"),
        ("ChatListPage.qml", "ChatListPage.qml"),
    ] {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/pages");
        let text =
            fs::read_to_string(dir.join(page)).unwrap_or_else(|err| panic!("read {page}: {err}"));
        let list_text =
            fs::read_to_string(dir.join(list)).unwrap_or_else(|err| panic!("read {list}: {err}"));
        assert!(
            list_text.contains("clip: true"),
            "{list} does not clip, so its rows draw over what is below them"
        );
        assert!(
            text.contains("bottom: banner.top"),
            "{page}'s list runs under the banner instead of stopping at it"
        );
        // The stopped state is a state: a page opened after the core died
        // never saw the transition that a handler would have caught.
        assert!(
            text.contains("core.status === \"stopped\" ? page.coreStoppedMessage"),
            "{page} waits for the core to stop rather than reading that it has"
        );
    }
}

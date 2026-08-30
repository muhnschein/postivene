//! Guards QML rules that no host-Qt run can check: syntax newer than
//! Sailfish's Qt 5.6, and list rows whose height ignores their text.
//!
//! A text scan on purpose: host Qt 5.15 accepts the newer form and only
//! warns, while on device the handlers silently never fire.

use std::fs;
use std::path::{Path, PathBuf};

/// The lines of the element that declares `marker`, up to the blank line
/// that follows it. A whole-file `contains` cannot say which element a
/// string came from, and once two of them carry it the check stops being
/// able to fail.
fn block_of(text: &str, marker: &str) -> String {
    text.lines()
        .skip_while(|line| !line.contains(marker))
        .take_while(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

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

/// Every `model.<role>` a delegate binds has to be a field the model
/// actually has: a rename on either side is otherwise a blank row that
/// nothing catches.
#[test]
fn delegates_bind_only_roles_their_models_have() {
    /// The `model.<role>` names a file reads.
    fn bound_roles(text: &str) -> Vec<String> {
        text.split("model.")
            .skip(1)
            .map(|tail| {
                tail.chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect::<String>()
            })
            .filter(|role| !role.is_empty())
            .collect()
    }

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cases: [(&str, Vec<String>); 5] = [
        (
            "qml/components/ConversationList.qml",
            names_of::<postivene_shim::MessageListItem>(),
        ),
        (
            "qml/pages/ChatListPage.qml",
            names_of::<postivene_shim::ChatListItem>(),
        ),
        (
            "qml/pages/NewChatPage.qml",
            names_of::<postivene_shim::ContactItem>(),
        ),
        (
            "qml/pages/NewGroupPage.qml",
            names_of::<postivene_shim::ContactItem>(),
        ),
        (
            "qml/components/SearchResultsList.qml",
            names_of::<postivene_shim::SearchItem>(),
        ),
    ];

    for (file, roles) in cases {
        let text =
            fs::read_to_string(root.join(file)).unwrap_or_else(|err| panic!("read {file}: {err}"));
        let bound = bound_roles(&text);
        assert!(
            !bound.is_empty(),
            "{file} binds nothing from its model any more"
        );
        for role in &bound {
            assert!(
                roles.contains(role),
                "{file} binds model.{role}, which its model does not have: {roles:?}"
            );
        }
    }
}

/// The role names a model row exposes to QML.
fn names_of<T: qmetaobject::listmodel::SimpleListItem>() -> Vec<String> {
    T::names()
        .iter()
        .map(std::string::ToString::to_string)
        .collect()
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
        // Scoped to the list's own block: the jump button anchors to the
        // same thing, so searching the whole file finds a match whatever
        // the list does.
        let list_block = block_of(&text, "id: listView");
        assert!(
            list_block.contains("bottom: banner.top"),
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

/// Copying a message says so, and the reply bar and jump button are the
/// components the tests measure rather than one-off items on the page.
#[test]
fn the_conversation_page_uses_the_pieces_that_are_tested() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/pages/ConversationPage.qml");
    let text = fs::read_to_string(&path).expect("read ConversationPage.qml");

    for piece in ["ReplyBar {", "JumpButton {", "Banner {"] {
        assert!(
            text.contains(piece),
            "the page does not use {piece} -- what a test measures is then not what runs"
        );
    }
    assert!(
        text.contains("Clipboard.text = body") && text.contains("notice.show("),
        "copying a message does not say that it happened"
    );
    // The field insets itself on the left; without the same on the right
    // the send button sits nearer the edge than the field does. Scoped to
    // the row: the jump button carries the same inset, so a whole-file
    // search cannot fail.
    assert!(
        block_of(&text, "id: inputRow").contains("rightMargin: Theme.horizontalPageMargin"),
        "the send button is flush against the edge of the screen"
    );
}

/// Anything showing a string the other end chose has to say it is plain
/// text. The default is `Text.AutoText`, which sniffs the string and
/// switches to rich text when it looks like markup -- so a message body of
/// `<img src="https://tracker/p.gif">` fetches that image the moment its
/// row is drawn, from an app whose whole point is that the network cannot
/// watch. It also mismeasures: the hidden copies that size a bubble would
/// measure the rendered width, not the literal one.
#[test]
fn text_from_the_other_end_is_pinned_to_plain() {
    // Bindings the core fills in from a message, a contact or a chat.
    // Anything reading one of these is showing remote input.
    const REMOTE: [&str; 14] = [
        "model.",
        "root.messageText",
        "root.quoteText",
        "root.quoteAuthor",
        "root.senderName",
        "root.chatName",
        "root.preview",
        "root.previewSender",
        "root.fileName",
        "root.filePath",
        "root.author",
        "root.body",
        "root.text",
        "page.myInvite",
    ];

    /// The element a line sits in, as the nearest `Foo {` above it.
    fn element_of(lines: &[&str], index: usize) -> String {
        lines[..index]
            .iter()
            .rev()
            .find_map(|line| {
                let trimmed = line.trim();
                let name = trimmed.strip_suffix('{')?.trim();
                (!name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '.'))
                    .then(|| name.to_string())
            })
            .unwrap_or_default()
    }

    let mut offenders = Vec::new();
    for file in qml_files() {
        let text = fs::read_to_string(&file).expect("read qml");
        let lines: Vec<&str> = text.lines().collect();
        for (number, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if !trimmed.starts_with("text:") {
                continue;
            }
            // Only the things that render text themselves. A MenuItem's
            // label is a translated literal, whatever it switches on.
            let element = element_of(&lines, number);
            if element != "Label" && element != "Text" {
                continue;
            }
            // The binding runs on while its lines stay further indented
            // than the `text:` that opened it.
            let indent = line.len() - trimmed.len();
            let mut value = trimmed.to_string();
            for next in &lines[number + 1..] {
                let next_indent = next.len() - next.trim_start().len();
                if next.trim().is_empty() || next_indent <= indent {
                    break;
                }
                value.push(' ');
                value.push_str(next.trim());
            }
            if !REMOTE.iter().any(|name| value.contains(name)) {
                continue;
            }
            // `textFormat` sits in the same element block, which starts at
            // the `{` found above.
            let start = number.saturating_sub(16);
            let block = lines[start..=number].join("\n");
            if !block.contains("textFormat:") {
                offenders.push(format!("{}:{}: {trimmed}", file.display(), number + 1));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "these show text the other end chose without saying it is plain, so \
         Qt decides for itself whether it is markup:\n  {}",
        offenders.join("\n  ")
    );
}

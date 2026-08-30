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

/// A row's time label must position itself off the row, never off the
/// label whose width it decides.
///
/// `SearchResultRow` had the two referring to each other -- the title's
/// width subtracted the time label's width, and the time label anchored
/// to the title's top. On a device that cycle left the title at its
/// natural width and a long chat name ran off the right-hand edge; the
/// offscreen engine used by the tests resolves it and shows nothing, so
/// this is a scan rather than a measurement. `ChatListDelegate` is the
/// shape that works: its time label is placed off `root.width`, and the
/// name takes its width from that label's `x`.
#[test]
fn a_time_label_does_not_depend_on_the_label_it_sizes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components");
    for (file, time_id, sized_id) in [
        ("SearchResultRow.qml", "id: timeLabel", "titleLabel"),
        ("ChatListDelegate.qml", "id: timeLabelItem", "nameLabel"),
    ] {
        let text =
            fs::read_to_string(root.join(file)).unwrap_or_else(|err| panic!("read {file}: {err}"));
        let block = block_of(&text, time_id);
        assert!(
            !block.is_empty(),
            "{file} has no {time_id} any more; this check is measuring nothing"
        );
        assert!(
            !block.contains(sized_id),
            "{file}'s time label positions itself off {sized_id}, whose width \
             is decided by this label -- that is the loop that put a long \
             title off the side of the screen. Place it off the row instead, \
             as ChatListDelegate does. Block was:\n{block}"
        );
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

/// The file with every comment and string body blanked out, newlines kept.
///
/// A scan that does not do this reads prose and translated text as code:
/// a URL in a string ends a line comment, and from there every quote
/// pairs up with the wrong one and whole blocks of real code disappear.
fn code_only(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let blank = |c: char| if c == '\n' { '\n' } else { ' ' };
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let here = chars[i];
        let next = chars.get(i + 1).copied();
        if here == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                out.push(' ');
                i += 1;
            }
        } else if here == '/' && next == Some('*') {
            while i < chars.len() {
                if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    out.push_str("  ");
                    i += 2;
                    break;
                }
                out.push(blank(chars[i]));
                i += 1;
            }
        } else if here == '"' || here == '\'' {
            out.push(' ');
            i += 1;
            while i < chars.len() && chars[i] != here {
                if chars[i] == '\\' {
                    out.push(' ');
                    i += 1;
                }
                if i < chars.len() {
                    out.push(blank(chars[i]));
                    i += 1;
                }
            }
            if i < chars.len() {
                out.push(' ');
                i += 1;
            }
        } else {
            out.push(here);
            i += 1;
        }
    }
    out
}

/// Identifiers and single punctuation marks, in order.
fn tokens(code: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    for c in code.chars() {
        if c.is_alphanumeric() || c == '_' || c == '$' {
            word.push(c);
            continue;
        }
        if !word.is_empty() {
            out.push(std::mem::take(&mut word));
        }
        if !c.is_whitespace() {
            out.push(c.to_string());
        }
    }
    if !word.is_empty() {
        out.push(word);
    }
    out
}

/// Every name the file itself introduces: ids, properties and their
/// types, functions and their parameters, signal parameters, `var`s.
///
/// Deliberately greedy -- a type name landing in here alongside the
/// property it types costs nothing. Over-collecting only makes the check
/// weaker, while missing a declaration would make it wrong.
fn declared_names(code: &str) -> std::collections::HashSet<String> {
    fn is_name(token: &str) -> bool {
        token
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
    }

    let tokens = tokens(code);
    let mut names = std::collections::HashSet::new();
    let mut i = 0;
    while i < tokens.len() {
        // Everything after the keyword, up to the token that ends the
        // declaration.
        let (skip, stop): (usize, &[&str]) = match tokens[i].as_str() {
            "id" if tokens.get(i + 1).map(String::as_str) == Some(":") => (2, &[]),
            "property" => (1, &[":", "{", "}"]),
            "function" | "signal" => (1, &[")", "{"]),
            "var" => (1, &["="]),
            _ => {
                i += 1;
                continue;
            }
        };
        let mut j = i + skip;
        // A bare `id:` or `var` names exactly one thing.
        let single = stop.is_empty() || tokens[i] == "var";
        while j < tokens.len() && !stop.contains(&tokens[j].as_str()) {
            if is_name(&tokens[j]) {
                names.insert(tokens[j].clone());
                if single {
                    break;
                }
            }
            j += 1;
        }
        i = j.max(i + 1);
    }
    names
}

/// Every `foo.` in the code, with the line it is on: a name being read
/// for something on it.
fn qualified_uses(code: &str) -> Vec<(usize, String)> {
    let chars: Vec<char> = code.chars().collect();
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let mut out = Vec::new();
    for (at, c) in chars.iter().enumerate() {
        if *c != '.' {
            continue;
        }
        let mut end = at;
        while end > 0 && chars[end - 1].is_whitespace() {
            end -= 1;
        }
        let mut start = end;
        while start > 0 && is_word(chars[start - 1]) {
            start -= 1;
        }
        if start == end {
            continue;
        }
        // `a.b.c` reads `b` off `a`; only the head of the chain is a name
        // that has to exist on its own.
        if start > 0 && (chars[start - 1] == '.' || is_word(chars[start - 1])) {
            continue;
        }
        let name: String = chars[start..end].iter().collect();
        // Types, attached properties and enums are capitalised; numbers
        // are not names at all.
        if !name.starts_with(|c: char| c.is_lowercase() && c.is_alphabetic()) {
            continue;
        }
        let line = chars[..start].iter().filter(|c| **c == '\n').count() + 1;
        out.push((line, name));
    }
    out
}

/// A name read as `<name>.something` has to be one the file introduces or
/// one QML puts in scope.
///
/// An id that is not there any more is not a load error. The binding that
/// reads it throws at runtime, that one binding never runs, and whatever
/// it fed keeps its default -- everything else on the page carries on. A
/// patch deleted `SearchResultRow`'s `Avatar` and left the three
/// references to it behind; the row's height is measured off it, so every
/// search result collapsed to nothing and a search showed "Chats (1)"
/// with no chat under it. The file still parsed, the page still loaded,
/// and nothing in the suite could see it.
#[test]
fn qml_reads_no_name_that_is_not_there() {
    // What QML puts in scope without the file saying so.
    const IN_SCOPE: [&str; 12] = [
        // Grouped properties, and properties of the element being
        // configured read without qualifying them.
        "anchors",
        "font",
        "icon",
        "text",
        "parent",
        // A delegate's scope.
        "model",
        "modelData",
        "section",
        "index",
        // Set from Rust in main.rs, and Silica's own.
        "core",
        "pageStack",
        // ContentPickerPage hands its answer to the handler under this
        // name; see SettingsPage.
        "selectedContentProperties",
    ];

    let files = qml_files();
    assert!(!files.is_empty(), "found no .qml files to check");

    let mut offenders = Vec::new();
    for file in &files {
        let code = code_only(&fs::read_to_string(file).expect("read qml"));
        let declared = declared_names(&code);
        let mut seen = Vec::new();
        for (line, name) in qualified_uses(&code) {
            if declared.contains(&name) || IN_SCOPE.contains(&name.as_str()) {
                continue;
            }
            if seen.contains(&name) {
                continue;
            }
            seen.push(name.clone());
            offenders.push(format!("{}:{line}: {name}.…", file.display()));
        }
    }

    assert!(
        offenders.is_empty(),
        "these read something off a name the file never declares. If it was \
         an id, the element is gone and the references were left behind: the \
         binding throws at load and silently keeps its default, which is how \
         a row ends up with no height. If it is something QML puts in scope, \
         add it to IN_SCOPE with a note saying where it comes from.\n  {}",
        offenders.join("\n  ")
    );
}

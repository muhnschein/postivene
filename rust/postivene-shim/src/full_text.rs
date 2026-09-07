//! The whole of something long enough to want a page of its own.
//!
//! Two things in a conversation are too long for the bubble they arrive
//! in: a message the sending core cut in two, whose rest is only behind
//! `get_message_html`, and a text file somebody attached -- a to-do list,
//! a note, a patch -- which the phone has nothing to open with.
//!
//! Both end the same way: a page, a scrollbar, and the words. So one
//! model serves both, and which of the two it is depends on what it was
//! given: a `message_id`, or a `file_path`. parla's reader dialog is
//! built the same way, from a message or from a file, for the same
//! reason -- there is one screen's worth of behaviour here, not two.
//!
//! The Markdown renderings are made here as the message list makes its
//! own, so the page draws what the reader's setting asks for without
//! going back to the shim for it.

use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;

use crate::chat::local_path;
use crate::core::connection;
use crate::json;
use crate::{html, markdown};

/// As much of a file as this will read.
///
/// A phone has to hold it, render it, and lay it out as one `Text` item;
/// a megabyte of prose is already more than anybody scrolls through, and
/// the log file somebody attached could be a hundred. What is past the
/// cap is not shown and the page says so, which is better than a screen
/// that never finishes drawing.
const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// What one load found.
#[derive(Default)]
struct Loaded {
    text: String,
    /// Only part of the file was read; see [`MAX_FILE_BYTES`].
    clipped: bool,
}

/// The whole text of one message, or of one text file.
///
/// ```qml
/// FullText {
///     id: whole
///     account_id: page.accountId
///     message_id: page.messageId
/// }
/// Label { text: whole.text }
/// ```
#[derive(QObject, Default)]
pub struct FullText {
    base: qt_base_class!(trait QObject),

    /// Which account the message belongs to. Setting it reloads.
    pub account_id: qt_property!(u32; WRITE set_account_id NOTIFY source_changed),
    /// Which message to read whole, or 0 to read a file instead.
    /// Setting it reloads.
    pub message_id: qt_property!(u32; WRITE set_message_id NOTIFY source_changed),
    /// A text file to read instead of a message: a `file://` URL or a
    /// plain path. Setting it reloads.
    pub file_path: qt_property!(QString; WRITE set_file_path NOTIFY source_changed),
    /// Emitted when what is being read changes.
    pub source_changed: qt_signal!(),

    /// The text, as written.
    pub text: qt_property!(QString; NOTIFY loaded_changed),
    /// The same rendered as `Text.StyledText`, for when Markdown is
    /// drawn. Made here so the page renders it once rather than on every
    /// scroll.
    pub styled_text: qt_property!(QString; NOTIFY loaded_changed),
    /// The same with its Markdown markers taken out.
    pub plain_text: qt_property!(QString; NOTIFY loaded_changed),
    /// Only the beginning of the file is here: it is longer than this
    /// will read.
    pub clipped: qt_property!(bool; NOTIFY loaded_changed),
    /// A load is under way. What the page shows a spinner for.
    pub loading: qt_property!(bool; NOTIFY loaded_changed),
    /// True once a load has finished, however it went.
    pub loaded: qt_property!(bool; NOTIFY loaded_changed),
    /// Emitted after every load, and when one starts.
    pub loaded_changed: qt_signal!(),

    /// Nothing could be read. The message is the core's own, or the
    /// file system's.
    pub error: qt_signal!(message: QString),

    /// Read it again.
    pub reload: qt_method!(fn(&mut self)),

    /// Counts loads, so a slow answer to an older question cannot land
    /// on top of a newer one; see `ChatInfo`.
    generation: u64,
}

impl FullText {
    /// Set the account and reload if it changed.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.source_changed();
            self.reload();
        }
    }

    /// Set the message and reload if it changed.
    pub fn set_message_id(&mut self, message_id: u32) {
        if self.message_id != message_id {
            self.message_id = message_id;
            self.source_changed();
            self.reload();
        }
    }

    /// Set the file and reload if it changed.
    pub fn set_file_path(&mut self, file_path: QString) {
        if self.file_path.to_string() != file_path.to_string() {
            self.file_path = file_path;
            self.source_changed();
            self.reload();
        }
    }

    /// Read it again.
    ///
    /// A file is read here and now: it is on this phone, and a page that
    /// spins before showing a to-do list somebody attached is a page
    /// that feels slower than it is. A message goes to the core, which
    /// is a round trip and an await.
    pub fn reload(&mut self) {
        let path = local_path(&self.file_path.to_string());
        if !path.is_empty() {
            let found = read_file(&path);
            self.finish(found);
            return;
        }

        let (account_id, message_id) = (self.account_id, self.message_id);
        if account_id == 0 || message_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.fail("not started".to_string());
            return;
        };
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.loading = true;
        self.loaded = false;
        self.loaded_changed();

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Loaded, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            if this.borrow().generation != generation {
                return;
            }
            this.borrow_mut().finish(result);
        });

        runtime.spawn(async move {
            done(fetch(&rpc, account_id, message_id).await);
        });
    }

    /// Take what a load found, however it went.
    fn finish(&mut self, result: Result<Loaded, String>) {
        match result {
            Ok(found) => {
                self.text = found.text.as_str().into();
                self.styled_text = markdown::render(&found.text).into();
                self.plain_text = markdown::strip(&found.text).into();
                self.clipped = found.clipped;
                self.loading = false;
                self.loaded = true;
                self.loaded_changed();
            }
            Err(message) => self.fail(message),
        }
    }

    /// Nothing to show, and why.
    fn fail(&mut self, message: String) {
        self.text = QString::default();
        self.styled_text = QString::default();
        self.plain_text = QString::default();
        self.clipped = false;
        self.loading = false;
        // Loaded in the sense that matters: the wait is over.
        self.loaded = true;
        self.loaded_changed();
        self.error(message.into());
    }
}

/// The whole text of one message.
///
/// `hasHtml` marks the ones that were cut: for those the rest is in the
/// HTML part, and only there. For everything else the message's own text
/// *is* the whole of it, and asking the core for an HTML part it has not
/// got would be a round trip for an empty string.
async fn fetch(rpc: &RpcClient, account_id: u32, message_id: u32) -> Result<Loaded, String> {
    let message: serde_json::Value = rpc
        .call("get_message", (account_id, message_id))
        .await
        .map_err(|err| err.to_string())?;
    let text = json::str_at(&message, "text").to_string();
    if !json::flag(&message, "hasHtml") {
        return Ok(Loaded {
            text,
            clipped: false,
        });
    }

    let whole: Option<String> = rpc
        .call("get_message_html", (account_id, message_id))
        .await
        .map_err(|err| err.to_string())?;
    let whole = whole.unwrap_or_default();
    Ok(Loaded {
        // Falling back to the cut version rather than to nothing: a
        // message whose HTML part did not survive its trip still has the
        // words that did.
        text: match html::to_text(&whole) {
            found if found.is_empty() => text,
            found => found,
        },
        clipped: false,
    })
}

/// As much of a text file as [`MAX_FILE_BYTES`] allows.
///
/// Read as bytes and converted, rather than read as a string: a file
/// somebody attached is not promised to be UTF-8, and one bad byte in a
/// log file is not a reason to show nothing. What cannot be decoded
/// becomes the replacement character, which is what every other reader
/// does with it.
fn read_file(path: &str) -> Result<Loaded, String> {
    use std::io::Read;

    let size = std::fs::metadata(path)
        .map_err(|err| format!("cannot read {path}: {err}"))?
        .len();
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|err| format!("cannot read {path}: {err}"))?
        .take(MAX_FILE_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|err| format!("cannot read {path}: {err}"))?;
    Ok(Loaded {
        text: String::from_utf8_lossy(&bytes).into_owned(),
        clipped: size > MAX_FILE_BYTES,
    })
}

/// The endings that mean a file is words when nothing else says so.
const TEXT_ENDINGS: [&str; 22] = [
    ".md",
    ".markdown",
    ".txt",
    ".text",
    ".log",
    ".json",
    ".xml",
    ".yaml",
    ".yml",
    ".toml",
    ".ini",
    ".conf",
    ".cfg",
    ".csv",
    ".tsv",
    ".patch",
    ".diff",
    ".sh",
    ".py",
    ".rs",
    ".qml",
    ".srt",
];

/// Whether a file is one this app can show as words.
///
/// The phone opens a picture, a video and a PDF; it has nothing for a
/// `.md`, a `.log`, a patch or a `.json`, and hands those back with
/// nothing happening at all -- which is how a note somebody sent becomes
/// a row that cannot be tapped. Those are the ones worth a reader of our
/// own.
///
/// The MIME type first, because the core reads the file to get it. The
/// name is the fallback: `text/markdown` is not registered everywhere,
/// and a `.md` arrives as `application/octet-stream` often enough that
/// going by the type alone would leave the very file this is for out.
pub(crate) fn looks_like_text(mime: &str, name: &str) -> bool {
    let mime = mime.trim().to_ascii_lowercase();
    // `text/*` is the whole family; the rest are the text formats that
    // were given a type of their own outside it.
    if mime.starts_with("text/")
        || matches!(
            mime.as_str(),
            "application/json"
                | "application/xml"
                | "application/x-sh"
                | "application/javascript"
                | "application/x-yaml"
                | "application/yaml"
                | "application/toml"
        )
    {
        return true;
    }
    // A type that says it is something else is believed: a `.txt` that
    // the core sniffed as a PNG is a PNG.
    if !mime.is_empty() && !mime.starts_with("application/octet-stream") {
        return false;
    }
    let name = name.trim().to_ascii_lowercase();
    TEXT_ENDINGS.iter().any(|ending| name.ends_with(ending))
}

#[cfg(test)]
mod tests {
    use super::looks_like_text;

    #[test]
    fn anything_the_core_calls_text_is_text() {
        assert!(looks_like_text("text/plain", "notes"));
        assert!(looks_like_text("text/markdown", "todo.md"));
        assert!(looks_like_text("TEXT/CSV; charset=utf-8", "rows.csv"));
    }

    #[test]
    fn the_text_formats_with_types_of_their_own_are_text_too() {
        assert!(looks_like_text("application/json", "map.json"));
        assert!(looks_like_text("application/x-sh", "build"));
    }

    #[test]
    fn a_picture_is_not_text_whatever_it_is_called() {
        assert!(!looks_like_text("image/png", "screenshot.txt"));
        assert!(!looks_like_text("application/pdf", "report.pdf"));
        assert!(!looks_like_text("audio/ogg", "note.ogg"));
    }

    #[test]
    fn the_name_decides_when_the_type_says_nothing_useful() {
        // The case this is for: a to-do list the core could not place.
        assert!(looks_like_text("application/octet-stream", "TODO.md"));
        assert!(looks_like_text("", "server.log"));
        assert!(!looks_like_text("", "holiday.jpg"));
        assert!(!looks_like_text("application/octet-stream", "archive.zip"));
    }

    #[test]
    fn nothing_at_all_is_not_text() {
        assert!(!looks_like_text("", ""));
    }

    #[test]
    fn an_ending_has_to_be_the_ending() {
        // "readme.mdx" is not markdown, and "x.md.zip" is an archive.
        assert!(!looks_like_text("", "x.md.zip"));
        assert!(looks_like_text("", "readme.MD"));
    }
}

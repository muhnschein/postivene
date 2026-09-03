//! Markdown in a message, rendered or taken out.
//!
//! Delta Chat messages are plain text by convention and Markdown by
//! habit: the other clients render `**bold**` and `` `code` `` and turn
//! links blue, so a message written on one of them arrives here full of
//! asterisks. Sailfish's Qt 5.6 has no Markdown of its own (`Text` learnt
//! it in 5.14), so this is a small renderer for the subset people
//! actually type -- emphasis, strikethrough, code, headings, links -- and
//! nothing that needs a block model.
//!
//! What it emits is `Text.StyledText`, and what makes that safe is that
//! it emits *only* what it chose to: every character of the message goes
//! out escaped, and the handful of tags are the ones this file writes. A
//! message body of `<img src="https://tracker/p.gif">` was the reason
//! every label in the app is pinned to plain text (`tests/qml_syntax.rs`),
//! and it stays a string of angle brackets here. Links are the one thing
//! that can reach the network, and only on a tap.
//!
//! Three modes, as parla offers them (github.com/trufae/parla): render,
//! strip -- the markers taken out and the words kept -- and off.

use crate::links::url_spans;

/// The message rendered as `Text.StyledText`: every character escaped,
/// and the Markdown the reader wrote turned into the few tags Qt draws.
pub(crate) fn render(text: &str) -> String {
    convert(text, true)
}

/// The message with its Markdown markers taken out and the words kept,
/// for a reader who wants neither asterisks nor formatting.
pub(crate) fn strip(text: &str) -> String {
    convert(text, false)
}

/// Both modes are one walk: `styled` decides whether a marker becomes a
/// tag or nothing.
///
/// Line by line, because fences and headings are lines. A fence line is
/// a marker and nothing else -- it opens or closes a block and is never
/// shown, its language word included -- so it leaves no line behind: the
/// block's tag goes on the line after it, or the line before.
fn convert(text: &str, styled: bool) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut in_fence = false;
    // The next line drawn opens a block.
    let mut opening = false;
    for line in text.split('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            if in_fence && styled {
                // A block with nothing in it is no block at all.
                if opening {
                    opening = false;
                } else if let Some(last) = lines.last_mut() {
                    last.push_str("</pre>");
                }
            } else if styled {
                opening = true;
            }
            in_fence = !in_fence;
            continue;
        }
        let mut out = String::with_capacity(line.len() + 8);
        if opening {
            out.push_str("<pre>");
            opening = false;
        }
        if in_fence {
            push_escaped(&mut out, line, styled);
        } else {
            block(line, trimmed, styled, &mut out);
        }
        lines.push(out);
    }
    if in_fence && styled && !opening {
        // A block that was never closed is still a block.
        if let Some(last) = lines.last_mut() {
            last.push_str("</pre>");
        }
    }
    lines.join("\n")
}

/// One line outside a code block: a heading, or ordinary text.
fn block(line: &str, trimmed: &str, styled: bool, out: &mut String) {
    // A heading: one to six hashes, a space, the text.
    let hashes = trimmed.bytes().take_while(|byte| *byte == b'#').count();
    if (1..=6).contains(&hashes) && trimmed.as_bytes().get(hashes) == Some(&b' ') {
        let heading = trimmed[hashes + 1..].trim();
        if styled {
            out.push_str("<b>");
        }
        inline(heading, styled, out);
        if styled {
            out.push_str("</b>");
        }
        return;
    }
    inline(line, styled, out);
}

/// The characters `StyledText` reads as markup, escaped; everything else as
/// it is. In plain mode nothing is escaped.
fn push_escaped(out: &mut String, text: &str, styled: bool) {
    if !styled {
        out.push_str(text);
        return;
    }
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

/// One line of ordinary text: emphasis, code, links, and everything else
/// escaped.
fn inline(line: &str, styled: bool, out: &mut String) {
    let links = url_spans(line);
    let bytes = line.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        // A bare link, as it stands. Its own characters are never markup,
        // so a `_` in a path is a `_` and not an italic.
        if let Some((start, end)) = links.iter().find(|(start, _)| *start == at) {
            let url = &line[*start..*end];
            push_link(out, url, url, styled);
            at = *end;
            continue;
        }
        let rest = &line[at..];
        // A backslash escapes the marker after it.
        if bytes[at] == b'\\' {
            if let Some(next) = rest[1..].chars().next() {
                if "\\`*_~[]()#".contains(next) {
                    push_escaped(out, &rest[1..=next.len_utf8()], styled);
                    at += 1 + next.len_utf8();
                    continue;
                }
            }
        }
        if let Some(consumed) = code_span(rest, styled, out)
            .or_else(|| link_span(rest, styled, out))
            .or_else(|| emphasis_span(line, at, styled, out))
        {
            at += consumed;
            continue;
        }
        // Whatever it is, one character of it, escaped.
        let c = rest.chars().next().unwrap_or(' ');
        push_escaped(out, &rest[..c.len_utf8()], styled);
        at += c.len_utf8();
    }
}

/// `` `code` ``: the contents are shown as typed, markers and all.
fn code_span(rest: &str, styled: bool, out: &mut String) -> Option<usize> {
    let inner = rest.strip_prefix('`')?;
    let close = inner.find('`').filter(|close| *close > 0)?;
    if styled {
        out.push_str("<tt>");
    }
    push_escaped(out, &inner[..close], styled);
    if styled {
        out.push_str("</tt>");
    }
    Some(close + 2)
}

/// `[text](https://...)`: the text is shown, the link is followed on a
/// tap. Only web and mail links: a `file:` or `javascript:` one is not
/// something a message gets to open.
fn link_span(rest: &str, styled: bool, out: &mut String) -> Option<usize> {
    let inner = rest.strip_prefix('[')?;
    let close = inner.find(']')?;
    let text = &inner[..close];
    let after = &inner[close + 1..];
    let url_part = after.strip_prefix('(')?;
    let url_end = url_part.find(')')?;
    let url = url_part[..url_end].trim();
    if text.is_empty() || url.contains(char::is_whitespace) {
        return None;
    }
    if !(url.starts_with("https://") || url.starts_with("http://") || url.starts_with("mailto:")) {
        return None;
    }
    push_link(out, url, text, styled);
    Some(1 + close + 1 + 1 + url_end + 1)
}

fn push_link(out: &mut String, url: &str, text: &str, styled: bool) {
    if styled {
        out.push_str("<a href=\"");
        push_escaped(out, url, true);
        out.push_str("\">");
        push_escaped(out, text, true);
        out.push_str("</a>");
    } else {
        out.push_str(text);
    }
}

/// `**bold**`, `__bold__`, `*italic*`, `_italic_`, `~~struck~~`. The
/// underscore forms only at word boundaries, so `snake_case_names` stay
/// as written; the others whenever the marker hugs its text.
fn emphasis_span(line: &str, at: usize, styled: bool, out: &mut String) -> Option<usize> {
    let rest = &line[at..];
    let (marker, tag): (&str, &str) = if rest.starts_with("**") {
        ("**", "b")
    } else if rest.starts_with("__") {
        ("__", "b")
    } else if rest.starts_with("~~") {
        ("~~", "s")
    } else if rest.starts_with('*') {
        ("*", "i")
    } else if rest.starts_with('_') {
        ("_", "i")
    } else {
        return None;
    };
    let underscore = marker.starts_with('_');
    if underscore && is_word(line[..at].chars().next_back()) {
        return None;
    }
    let inner = &rest[marker.len()..];
    // The text has to start right after the marker, and end right before
    // the closing one: `a * b * c` is arithmetic, not italics.
    if inner.starts_with(char::is_whitespace) || inner.is_empty() {
        return None;
    }
    let close = find_closer(inner, marker)?;
    let text = &inner[..close];
    if text.ends_with(char::is_whitespace) {
        return None;
    }
    let end = at + marker.len() + close + marker.len();
    if underscore && is_word(line[end..].chars().next()) {
        return None;
    }
    if styled {
        out.push('<');
        out.push_str(tag);
        out.push('>');
    }
    inline(text, styled, out);
    if styled {
        out.push_str("</");
        out.push_str(tag);
        out.push('>');
    }
    Some(end - at)
}

/// Where `marker` closes `inner`, skipping a marker that would leave the
/// text empty. A one-character marker must not match the first half of
/// a doubled one, so `*a **b** c*` closes at the lone star.
fn find_closer(inner: &str, marker: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(offset) = inner[from..].find(marker) {
        let index = from + offset;
        if index == 0 {
            from = index + marker.len();
            continue;
        }
        let single = marker.len() == 1;
        let doubled = inner[index..].starts_with(&marker.repeat(2));
        if single && doubled {
            from = index + 2;
            continue;
        }
        return Some(index);
    }
    None
}

/// Whether a character is part of a word, so an underscore beside it is
/// part of the word too.
fn is_word(c: Option<char>) -> bool {
    c.is_some_and(char::is_alphanumeric)
}

#[cfg(test)]
mod tests {
    use super::{render, strip};

    #[test]
    fn emphasis_code_and_headings_become_tags() {
        assert_eq!(
            render("**bold** and *it* and ~~no~~"),
            "<b>bold</b> and <i>it</i> and <s>no</s>"
        );
        assert_eq!(render("__bold__ _it_"), "<b>bold</b> <i>it</i>");
        assert_eq!(render("say `a < b` here"), "say <tt>a &lt; b</tt> here");
        assert_eq!(render("# Title\nbody"), "<b>Title</b>\nbody");
        assert_eq!(render("```\nx = 1\n```\nafter"), "<pre>x = 1</pre>\nafter");
        assert_eq!(
            render("before\n```rust\nlet x;\n```"),
            "before\n<pre>let x;</pre>"
        );
        assert_eq!(render("```\n```\nx"), "x");
        assert_eq!(render("```\nopen"), "<pre>open</pre>");
        // Nested: the inner markers still count.
        assert_eq!(render("**bold *and* more**"), "<b>bold <i>and</i> more</b>");
    }

    #[test]
    fn what_is_not_markdown_is_left_alone() {
        assert_eq!(render("2 * 3 * 4"), "2 * 3 * 4");
        assert_eq!(render("snake_case_name"), "snake_case_name");
        assert_eq!(render("a ** b"), "a ** b");
        assert_eq!(render("*unclosed"), "*unclosed");
        assert_eq!(render("**"), "**");
        assert_eq!(render(r"\*not\*"), "*not*");
        assert_eq!(render(""), "");
    }

    #[test]
    fn links_are_anchors_and_only_web_and_mail_ones() {
        assert_eq!(
            render("see https://example.org/a_b?x=1, ok"),
            "see <a href=\"https://example.org/a_b?x=1\">https://example.org/a_b?x=1</a>, ok"
        );
        assert_eq!(
            render("[the page](https://example.org/)"),
            "<a href=\"https://example.org/\">the page</a>"
        );
        assert_eq!(
            render("[mail](mailto:a@example.org)"),
            "<a href=\"mailto:a@example.org\">mail</a>"
        );
        // Not followed: anything that is not a web or a mail link.
        assert_eq!(
            render("[x](javascript:alert(1))"),
            "[x](javascript:alert(1))"
        );
        assert_eq!(render("[x](file:///etc/passwd)"), "[x](file:///etc/passwd)");
    }

    #[test]
    fn markup_in_a_message_stays_text() {
        // The reason every label in the app is plain text: a tracking
        // pixel in a body must not fetch anything when its row is drawn.
        let rendered = render("<img src=\"https://tracker/p.gif\"> & <a href=\"x\">y</a>");
        assert!(!rendered.contains("<img"), "{rendered}");
        assert!(!rendered.contains("<a href=\"x\""), "{rendered}");
        // The address inside is still a link -- followed on a tap, and
        // fetched by nothing else.
        assert_eq!(
            rendered,
            "&lt;img src=&quot;<a href=\"https://tracker/p.gif\">https://tracker/p.gif</a>&quot;&gt; \
             &amp; &lt;a href=&quot;x&quot;&gt;y&lt;/a&gt;"
        );
        assert_eq!(render("<b>plain</b>"), "&lt;b&gt;plain&lt;/b&gt;");
        // The same inside code and inside a link's text.
        assert_eq!(render("`<b>`"), "<tt>&lt;b&gt;</tt>");
        assert_eq!(
            render("[<i>](https://example.org/?a=1&b=2)"),
            "<a href=\"https://example.org/?a=1&amp;b=2\">&lt;i&gt;</a>"
        );
    }

    #[test]
    fn stripping_keeps_the_words_and_drops_the_markers() {
        assert_eq!(
            strip("**bold** and *it* `code` ~~gone~~"),
            "bold and it code gone"
        );
        assert_eq!(
            strip("## Heading\n[text](https://example.org)"),
            "Heading\ntext"
        );
        assert_eq!(strip("```\nx < 1\n```"), "x < 1");
        // Nothing escaped in plain text.
        assert_eq!(strip("a < b & c"), "a < b & c");
        assert_eq!(strip("2 * 3 * 4"), "2 * 3 * 4");
    }
}

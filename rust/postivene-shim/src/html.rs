//! The core's message HTML, as text.
//!
//! A long message is not sent whole. The sending core cuts the body at
//! `DC_DESIRED_TEXT_LEN` and puts the rest in an HTML part, so what
//! arrives in `text` ends in `[...]` and the whole of it is only behind
//! `get_message_html`. Every client that offers "show full message" goes
//! that way; there is no other way to the rest of the words.
//!
//! What comes back is a mail's HTML, and this app has no business
//! rendering one: a `WebView` for a message body is the tracking pixel
//! the whole of `markdown.rs` exists to keep out. So the markup is taken
//! off and the words are kept, which is what the reader wanted -- and
//! what parla does with the same answer, for the same reason.
//!
//! This is not an HTML parser and cannot become one. It is four
//! substitutions over the shape mail bodies actually have, and its output
//! is plain text: whatever it fails to strip is shown as the angle
//! brackets it is, never as markup.

/// The words in a message's HTML part, with the tags taken out.
///
/// Block ends become line breaks, so paragraphs and list items stay
/// separate lines rather than running together; `head`, `script` and
/// `style` go out whole, contents and all, because none of what is in
/// them is anything the reader wrote.
pub(crate) fn to_text(html: &str) -> String {
    let text = drop_hidden(html);
    let text = replace_tags(&text);
    let text = decode_entities(&text);
    collapse_blank_lines(text.trim())
}

/// `<head>`, `<script>` and `<style>` with everything inside them.
///
/// Written as a scan rather than a regex: the crate has no regex engine
/// and is not gaining one for four keywords.
fn drop_hidden(html: &str) -> String {
    const HIDDEN: [&str; 3] = ["head", "script", "style"];
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    'outer: while !rest.is_empty() {
        let Some(open) = rest.find('<') else {
            out.push_str(rest);
            break;
        };
        let (before, from_tag) = rest.split_at(open);
        out.push_str(before);
        for name in HIDDEN {
            if opens(from_tag, name) {
                // Everything up to the matching close, or -- for markup
                // that never closes it -- the rest of the document,
                // which is the safer of the two readings.
                let closing = format!("</{name}");
                rest = match find_ignoring_case(from_tag, &closing) {
                    Some(at) => &from_tag[at..],
                    None => "",
                };
                // Past the close tag itself.
                rest = rest.find('>').map_or("", |at| &rest[at + 1..]);
                continue 'outer;
            }
        }
        out.push('<');
        rest = &from_tag[1..];
    }
    out
}

/// Whether `text` starts with an opening tag named `name`.
fn opens(text: &str, name: &str) -> bool {
    let Some(after) = text.strip_prefix('<') else {
        return false;
    };
    let Some(after) = strip_prefix_ignoring_case(after, name) else {
        return false;
    };
    // `<head>` and `<style type=...>` open one; `<header>` does not.
    after.starts_with('>') || after.starts_with(|c: char| c.is_ascii_whitespace())
}

/// Every tag out, with the ones that end a line leaving one behind.
fn replace_tags(html: &str) -> String {
    // The tags a reader would see as a line ending. `br` is the break
    // itself; the rest are blocks, and their *closing* tag is where the
    // line ends.
    const BREAKS: [&str; 2] = ["br", "hr"];
    const BLOCKS: [&str; 12] = [
        "p", "div", "section", "article", "li", "tr", "h1", "h2", "h3", "h4", "h5", "h6",
    ];
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while !rest.is_empty() {
        let Some(open) = rest.find('<') else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..open]);
        let from_tag = &rest[open..];
        // A `<` that does not begin a tag is a stray angle bracket in
        // the text -- "3 < 4" -- and stays one. A tag's name starts
        // with a letter, a `/`, or the `!` of a comment or a doctype;
        // anything else after the bracket is arithmetic.
        if !from_tag[1..].starts_with(|c: char| c.is_ascii_alphabetic() || c == '/' || c == '!') {
            out.push('<');
            rest = &from_tag[1..];
            continue;
        }
        // A `<` with no `>` after it is the same thing at the end of the
        // text: keep it and stop.
        let Some(close) = from_tag.find('>') else {
            out.push_str(from_tag);
            break;
        };
        let tag = &from_tag[1..close];
        let name = tag
            .trim_start_matches('/')
            .split(|c: char| c.is_ascii_whitespace() || c == '/')
            .next()
            .unwrap_or("");
        let ends_line = BREAKS.iter().any(|kind| name.eq_ignore_ascii_case(kind))
            || (tag.starts_with('/') && BLOCKS.iter().any(|kind| name.eq_ignore_ascii_case(kind)));
        if ends_line {
            out.push('\n');
        }
        rest = &from_tag[close + 1..];
    }
    out
}

/// The handful of entities a mail body actually carries. `&amp;` last, so
/// an `&amp;lt;` in the source does not turn into a `<`.
fn decode_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// A mail's markup puts a tag on its own line, and every one of those
/// left a blank line behind. Two in a row is a paragraph break; three is
/// the markup showing through.
fn collapse_blank_lines(text: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut blanks = 0;
    for line in text.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() {
            blanks += 1;
            if blanks > 1 {
                continue;
            }
        } else {
            blanks = 0;
        }
        out.push(line);
    }
    out.join("\n")
}

/// `haystack.find(needle)` without regard to case, for ASCII needles.
fn find_ignoring_case(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .char_indices()
        .find(|(at, _)| haystack[*at..].to_ascii_lowercase().starts_with(needle))
        .map(|(at, _)| at)
}

/// `text.strip_prefix(prefix)` without regard to case, for ASCII
/// prefixes.
fn strip_prefix_ignoring_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    if text.len() >= prefix.len() && text[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&text[prefix.len()..])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::to_text;

    #[test]
    fn the_words_survive_and_the_tags_do_not() {
        assert_eq!(
            to_text("<html><body><p>Buy milk</p><p>Call Ada</p></body></html>"),
            "Buy milk\nCall Ada"
        );
    }

    #[test]
    fn a_list_stays_one_item_to_a_line() {
        assert_eq!(
            to_text("<ul><li>one</li><li>two</li><li>three</li></ul>"),
            "one\ntwo\nthree"
        );
    }

    #[test]
    fn a_break_breaks_the_line() {
        assert_eq!(to_text("first<br>second<br/>third"), "first\nsecond\nthird");
    }

    #[test]
    fn what_the_reader_never_wrote_goes_out_whole() {
        assert_eq!(
            to_text(
                "<head><title>ignored</title></head><body>\
                 <style>p { color: red }</style>kept\
                 <script>alert('no')</script></body>"
            ),
            "kept"
        );
    }

    #[test]
    fn a_tag_whose_name_merely_starts_the_same_is_not_one_of_them() {
        // `<header>` is not `<head>`, and its words are the reader's.
        assert_eq!(to_text("<header>Shopping</header>list"), "Shoppinglist");
    }

    #[test]
    fn entities_come_back_as_the_characters_they_stand_for() {
        assert_eq!(
            to_text("<p>a &lt;b&gt; &amp; &quot;c&quot;&nbsp;d</p>"),
            "a <b> & \"c\" d"
        );
    }

    #[test]
    fn an_escaped_entity_is_not_decoded_twice() {
        // `&amp;lt;` is the reader writing `&lt;`, not writing `<`.
        assert_eq!(to_text("<p>&amp;lt;</p>"), "&lt;");
    }

    #[test]
    fn a_stray_angle_bracket_is_text_like_any_other() {
        // Nothing here begins a tag: a name cannot start with a space
        // or a digit.
        assert_eq!(to_text("<p>3 < 4 and 5 > 2</p>"), "3 < 4 and 5 > 2");
        assert_eq!(to_text("a < b"), "a < b");
    }

    #[test]
    fn a_comment_is_not_the_readers_words() {
        assert_eq!(to_text("<p>kept<!-- hidden --></p>"), "kept");
    }

    #[test]
    fn the_markups_own_blank_lines_do_not_pile_up() {
        assert_eq!(
            to_text("<div>\n<p>one</p>\n\n<p>two</p>\n</div>"),
            "one\n\ntwo"
        );
    }

    #[test]
    fn nothing_in_is_nothing_out() {
        assert_eq!(to_text(""), "");
        assert_eq!(to_text("   \n  "), "");
    }

    #[test]
    fn text_with_no_markup_at_all_is_left_as_it_is() {
        assert_eq!(to_text("just words\nand a line"), "just words\nand a line");
    }

    #[test]
    fn a_tag_that_never_closes_takes_the_rest_with_it() {
        // Whichever way this is read, none of it is the reader's words.
        assert_eq!(to_text("kept<script>alert('no')"), "kept");
    }
}

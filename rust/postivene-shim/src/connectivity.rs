//! What the core says about a profile's connection and its storage.
//!
//! `get_connectivity` is a number, and `get_connectivity_html` is a page
//! written for a web view: headings, a list per relay, a progress bar for
//! the mailbox quota. Silica has no web view this app may use, so the one
//! fact worth showing -- how full the mailbox is -- is read off that page
//! the way parla reads it (github.com/trufae/parla, `storage_quota.vala`):
//! the percentage the core wrote on its own bar, and the sentence it wrote
//! beside it, in whatever language the core is in. Nothing here computes
//! a quota; the core did, and this finds where it put the answer.

// The core's `get_connectivity` bands, which the profile page puts words
// to: 1000 not connected, 2000 connecting, 3000 connected and working,
// 4000 connected and idle. Anything at or above a band is in it; the
// values in between are the core's own finer steps.

/// The mailbox quota as the report states it: the percentage used, and
/// the core's own words for the amounts.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Quota {
    pub percent: u32,
    pub text: String,
}

/// The first quota bar in a connectivity report, if there is one. A relay
/// that reports no quota, or a report from before the first connection,
/// has none.
pub(crate) fn quota_from_report(html: &str) -> Option<Quota> {
    // The bar the core draws: `<div class="progress grey" style="width:
    // 12%">12%</div>`. The number inside the div is the real percentage;
    // the width is capped at 100.
    let bar = html.find("class=\"progress")?;
    let after_bar = &html[bar..];
    let open_end = after_bar.find('>')? + 1;
    let inner_end = after_bar[open_end..].find('<')? + open_end;
    let percent: u32 = after_bar[open_end..inner_end]
        .trim()
        .trim_end_matches('%')
        .parse()
        .ok()?;
    // The sentence before the bar, inside the same list item: back from
    // the bar to the `<li>` that holds it, then its tags dropped.
    let item_start = html[..bar].rfind("<li>")? + "<li>".len();
    let bar_div = html[..bar].rfind("<div")?;
    let text = strip_tags(&html[item_start..bar_div]);
    Some(Quota { percent, text })
}

/// The text of a fragment of HTML: tags dropped, the few entities the
/// core writes put back, whitespace collapsed.
fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut rest = html;
    while let Some(c) = rest.chars().next() {
        rest = &rest[c.len_utf8()..];
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            _ if in_tag => {}
            '&' => {
                let (decoded, skip) = decode_entity(rest);
                out.push_str(decoded);
                rest = &rest[skip..];
            }
            _ => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The character an entity stands for, and how much of the text it took.
/// An entity this does not know is left as its ampersand.
fn decode_entity(rest: &str) -> (&'static str, usize) {
    for (entity, decoded) in [
        ("amp;", "&"),
        ("lt;", "<"),
        ("gt;", ">"),
        ("quot;", "\""),
        ("apos;", "'"),
        ("nbsp;", " "),
        ("#39;", "'"),
    ] {
        if rest.starts_with(entity) {
            return (decoded, entity.len());
        }
    }
    ("&", 0)
}

#[cfg(test)]
mod tests {
    use super::{quota_from_report, Quota};

    /// The shape the core writes, less the style block.
    const REPORT: &str = "<html><body><h3>Incoming messages</h3><ul>\
        <li class=\"transport\"><span class=\"dot green\"></span> <b>nine.testrun.org:</b> Connected<br />\
        <ul class=\"quota-list\"><li>1.34 GiB of 2 GiB used\
        <div class=\"bar\"><div class=\"progress grey\" style=\"width: 67%\">67%</div></div>\
        </li></ul></li></ul><h3>Outgoing messages</h3><ul><li>Connected</li></ul></body></html>";

    #[test]
    fn the_quota_is_read_off_the_bar_the_core_drew() {
        assert_eq!(
            quota_from_report(REPORT),
            Some(Quota {
                percent: 67,
                text: "1.34 GiB of 2 GiB used".to_string()
            })
        );
    }

    #[test]
    fn a_report_without_a_bar_has_no_quota() {
        assert_eq!(
            quota_from_report("<html><body><h3>Not connected</h3></body></html>"),
            None
        );
        assert_eq!(quota_from_report(""), None);
        // Over-full: the width is capped but the number is not.
        let full = REPORT.replace("width: 67%\">67%", "width: 100%\">120%");
        assert_eq!(
            quota_from_report(&full).map(|quota| quota.percent),
            Some(120)
        );
    }

    #[test]
    fn entities_and_tags_come_out_of_the_words() {
        let report = REPORT.replace(
            "1.34 GiB of 2 GiB used",
            "<b>Storage:</b> 1.34&nbsp;GiB of 2 GiB used &amp; counting",
        );
        assert_eq!(
            quota_from_report(&report).map(|quota| quota.text),
            Some("Storage: 1.34 GiB of 2 GiB used & counting".to_string())
        );
    }
}

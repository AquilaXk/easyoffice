//! Hyperlinks in ODF (PD-218): a link is a `text:a` element inside the paragraph of a cell (ODF 1.2
//! 6.1.8), `xlink:href` its address and `office:title` its tip. The model is `sheet_links`.
//!
//! An address inside the workbook is `#Sheet1.A1` (a dot between the sheet and the cell, ODF 1.2
//! 9.2.1); the model keeps the spreadsheet file format's `#Sheet1!A1`, so the two are converted here.
//! Every address goes through the policy of `sheet_links`.

use crate::sheet_links::CellLink;
use crate::XmlSanitizer;

/// The model's target for an `xlink:href`: `#Sheet1.A1` becomes `#Sheet1!A1`; anything else stays.
pub(crate) fn href_to_target(href: &str) -> String {
    let Some(location) = href.strip_prefix('#') else {
        return href.to_string();
    };
    let location = location.trim_start_matches('$');
    let sheet_end = if let Some(quoted) = location.strip_prefix('\'') {
        quoted_end(quoted).map(|at| at + 1)
    } else {
        location.find('.')
    };
    match sheet_end {
        Some(at) if location[at..].starts_with('.') => {
            format!(
                "#{}!{}",
                &location[..at],
                location[at + 1..].replace('$', "")
            )
        }
        _ => format!("#{location}"),
    }
}

/// The index, in the whole location, of the quote that closes a quoted sheet name; `quoted` is the
/// location after its opening quote and a doubled quote stands for one inside the name.
fn quoted_end(quoted: &str) -> Option<usize> {
    let bytes = quoted.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            if bytes.get(i + 1) == Some(&b'\'') {
                i += 2;
                continue;
            }
            return Some(i + 1);
        }
        i += 1;
    }
    None
}

/// The `xlink:href` for a model target: `#Sheet1!A1` becomes `#Sheet1.A1`.
pub(crate) fn target_to_href(target: &str) -> String {
    let Some(location) = target.strip_prefix('#') else {
        return target.to_string();
    };
    let split = if let Some(quoted) = location.strip_prefix('\'') {
        quoted_end(quoted)
            .filter(|at| location[*at + 1..].starts_with('!'))
            .map(|at| at + 1)
    } else {
        location.find('!')
    };
    match split {
        Some(at) => format!("#{}.{}", &location[..at], &location[at + 1..]),
        None => target.to_string(),
    }
}

/// Wraps the text of the first paragraph written since `from` in a `text:a` for `link`. A cell without a
/// paragraph has no text to link and stays as it is.
pub(crate) fn wrap_first_paragraph(content: &mut String, from: usize, link: &CellLink) {
    const OPEN: &str = "<text:p>";
    const CLOSE: &str = "</text:p>";
    let Some(open) = content[from..].find(OPEN).map(|at| from + at + OPEN.len()) else {
        return;
    };
    let Some(close) = content[open..].find(CLOSE).map(|at| open + at) else {
        return;
    };
    let mut anchor = format!(
        "<text:a xlink:type=\"simple\" xlink:href=\"{}\"",
        XmlSanitizer::escape(&target_to_href(&link.target))
    );
    if !link.tip.is_empty() {
        anchor.push_str(&format!(
            " office:title=\"{}\"",
            XmlSanitizer::escape(&link.tip)
        ));
    }
    anchor.push('>');
    content.insert_str(close, "</text:a>");
    content.insert_str(open, &anchor);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workbook_locations_convert_both_ways() {
        for (href, target) in [
            ("#Sheet2.A1", "#Sheet2!A1"),
            ("#$Sheet2.$A$1:$B$3", "#Sheet2!A1:B3"),
            ("#'My Sheet'.A1:B2", "#'My Sheet'!A1:B2"),
            ("#'It''s'.C3", "#'It''s'!C3"),
            ("#Total", "#Total"),
            ("https://example.com/a.b", "https://example.com/a.b"),
        ] {
            assert_eq!(href_to_target(href), target, "{href}");
        }
        for (target, href) in [
            ("#Sheet2!A1", "#Sheet2.A1"),
            ("#'My Sheet'!A1:B2", "#'My Sheet'.A1:B2"),
            ("#Total", "#Total"),
            ("https://example.com/a!b", "https://example.com/a!b"),
        ] {
            assert_eq!(target_to_href(target), href, "{target}");
        }
    }

    #[test]
    fn the_first_paragraph_is_wrapped() {
        let link = CellLink::new("https://example.com/?a=1&b=2", "팁 \"하나\"").unwrap();
        let mut xml =
            String::from("<x/><table:table-cell><text:p>글자</text:p></table:table-cell>");
        wrap_first_paragraph(&mut xml, 4, &link);
        assert_eq!(
            xml,
            "<x/><table:table-cell><text:p><text:a xlink:type=\"simple\" xlink:href=\"https://example.com/?a=1&amp;b=2\" office:title=\"팁 &quot;하나&quot;\">글자</text:a></text:p></table:table-cell>"
        );
        let mut empty = String::from("<table:table-cell/>");
        wrap_first_paragraph(&mut empty, 0, &link);
        assert_eq!(empty, "<table:table-cell/>");
    }
}

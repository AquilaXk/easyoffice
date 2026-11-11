//! RTF writer for [`RichDocument`]: one `\pard\plain ... \par` line per paragraph, so each line
//! starts from default paragraph and character state and a heading's size cannot reach the next
//! paragraph. Paragraph properties written: alignment, outline level and heading size, list
//! (`\lsN\ilvl0` against a `\listtable`), left and first-line indent, and line spacing
//! (`\slN\slmult1`). Run formatting: bold, italic, underline, strike, size, font, color,
//! highlight, and super/subscript. The reader in `rich_read_rtf.rs` parses all of these back.

use crate::rich_document::{
    RichAlignment, RichBlock, RichDocument, RichListKind, RichParagraph, RichRun, RichScript,
};
use crate::rich_rtf_table::rtf_table_lines;
use crate::rich_run_writers::{has_extra_run_format, RtfRunTables};
use crate::rich_writers::heading_half_points;
use crate::RtfCodec;

/// Formatting control words the RTF writer emits (each followed by one delimiter space).
const RTF_FORMAT_WORDS: [&str; 28] = [
    "pard",
    "ql",
    "qc",
    "qr",
    "qj",
    "b",
    "i",
    "ul",
    "ulnone",
    "strike",
    "fs",
    "li",
    "fi",
    "sl",
    "slmult",
    "ls",
    "ilvl",
    "super",
    "sub",
    "nosupersub",
    "pnlvlblt",
    "pnlvlbody",
    "outlinelevel",
    "plain",
    "s",
    "f",
    "cf",
    "highlight",
];

/// Left indent of a list paragraph in twips (the list levels below use the same value).
const LIST_LEFT_INDENT_TWIPS: u32 = 720;
/// Hanging indent of the list levels in twips.
const LIST_HANGING_TWIPS: u32 = 360;
/// `\slN` of single spacing with `\slmult1` (240ths of a line).
const SINGLE_LINE_240THS: u32 = 240;

/// `\lsN` of each list kind: the reader's `\ls2` is the numbered list, any other number a bullet.
const BULLET_LIST_OVERRIDE: u32 = 1;
const ORDERED_LIST_OVERRIDE: u32 = 2;

/// Removes the formatting control words written by [`encode_rtf_rich`] (alignment, bold, italic,
/// underline, strike, size, indent, spacing, script, list, outline level) so the line decoder sees
/// escaped text only. Escapes (`\\`, `\{`, `\}`), `\uN?`, and `\line` are kept for the text
/// unescaper.
pub(crate) fn strip_rtf_format_words(input: &str) -> String {
    let rest = input.strip_prefix(' ').unwrap_or(input);
    let bytes = rest.as_bytes();
    let mut out = String::with_capacity(rest.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            let ch = rest[i..].chars().next().unwrap_or(' ');
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        let word_start = i + 1;
        let mut j = word_start;
        while j < bytes.len() && bytes[j].is_ascii_lowercase() {
            j += 1;
        }
        let word = &rest[word_start..j];
        if word.is_empty() {
            // Control symbol (`\\`, `\{`, `\}`): keep both bytes.
            let end = (i + 2).min(bytes.len());
            out.push_str(&rest[i..end]);
            i = end;
            continue;
        }
        if !RTF_FORMAT_WORDS.contains(&word) {
            out.push_str(&rest[i..j]);
            i = j;
            continue;
        }
        let mut k = j;
        if k < bytes.len() && bytes[k] == b'-' {
            k += 1;
        }
        while k < bytes.len() && bytes[k].is_ascii_digit() {
            k += 1;
        }
        if k < bytes.len() && bytes[k] == b' ' {
            k += 1;
        }
        i = k;
    }
    out
}

fn rtf_alignment_word(a: RichAlignment) -> &'static str {
    match a {
        RichAlignment::Left => "\\ql",
        RichAlignment::Center => "\\qc",
        RichAlignment::Right => "\\qr",
        RichAlignment::Justify => "\\qj",
    }
}

fn is_heading(p: &RichParagraph) -> bool {
    (1..=6).contains(&p.heading_level)
}

/// A list paragraph (a heading is never a list item).
fn list_kind_of(p: &RichParagraph) -> Option<RichListKind> {
    p.list.filter(|_| !is_heading(p))
}

fn list_override_id(kind: RichListKind) -> u32 {
    match kind {
        RichListKind::Bullet => BULLET_LIST_OVERRIDE,
        RichListKind::Ordered => ORDERED_LIST_OVERRIDE,
    }
}

/// `{\*\listtable ...}` and `{\*\listoverridetable ...}` for the bullet and numbered list.
fn list_table_lines() -> [String; 2] {
    let level = |nfc: u32, text: &str, numbers: &str| {
        format!(
            "{{\\listlevel\\levelnfc{nfc}\\levelstartat1\\leveljc0{{\\leveltext{text};}}{{\\levelnumbers{numbers};}}\\fi-{LIST_HANGING_TWIPS}\\li{LIST_LEFT_INDENT_TWIPS}}}"
        )
    };
    let bullet = level(23, "\\'01\\uc0\\u8226 ", "");
    let ordered = level(0, "\\'02\\'00.", "\\'01");
    [
        format!(
            "{{\\*\\listtable{{\\list\\listtemplateid{BULLET_LIST_OVERRIDE}{bullet}\\listid{BULLET_LIST_OVERRIDE}}}{{\\list\\listtemplateid{ORDERED_LIST_OVERRIDE}{ordered}\\listid{ORDERED_LIST_OVERRIDE}}}}}"
        ),
        format!(
            "{{\\*\\listoverridetable{{\\listoverride\\listid{BULLET_LIST_OVERRIDE}\\listoverridecount0\\ls{BULLET_LIST_OVERRIDE}}}{{\\listoverride\\listid{ORDERED_LIST_OVERRIDE}\\listoverridecount0\\ls{ORDERED_LIST_OVERRIDE}}}}}"
        ),
    ]
}

/// `\slN` for a percentage of single line spacing (inverse of the reader's rounding).
fn line_spacing_240ths(percent: u16) -> u32 {
    (u32::from(percent) * SINGLE_LINE_240THS + 50) / 100
}

/// One RTF paragraph line (`\pard\plain ... \par`).
fn rtf_paragraph_line(p: &RichParagraph, tables: &RtfRunTables) -> String {
    let mut line = String::from("\\pard\\plain");
    if let Some(a) = p.alignment {
        line.push_str(rtf_alignment_word(a));
    }
    // Character words every run restores after its own `\plain`.
    let mut base = String::new();
    let list = list_kind_of(p);
    if is_heading(p) {
        let size = heading_half_points(p.heading_level);
        line.push_str(&format!("\\outlinelevel{}\\fs{size}", p.heading_level - 1));
        base = format!("\\fs{size} ");
    } else if let Some(kind) = list {
        line.push_str(&format!("\\ls{}\\ilvl0", list_override_id(kind)));
    }
    let left = p
        .left_indent_twips
        .filter(|v| *v > 0)
        .or_else(|| list.map(|_| LIST_LEFT_INDENT_TWIPS));
    if let Some(left) = left {
        line.push_str(&format!("\\li{left}"));
    }
    if let Some(first) = p.first_line_indent_twips.filter(|v| *v != 0) {
        line.push_str(&format!("\\fi{first}"));
    }
    if let Some(percent) = p.line_spacing_percent.filter(|v| *v > 0) {
        line.push_str(&format!("\\sl{}\\slmult1", line_spacing_240ths(percent)));
    }
    line.push(' ');
    for run in &p.runs {
        push_rtf_run(&mut line, run, tables, &base);
    }
    line.push_str("\\par");
    line
}

/// One run. A run with size, font, color, highlight, or script ends with `\plain` (which also
/// drops the paragraph's own character words), so `base` is written again after it.
fn push_rtf_run(line: &mut String, run: &RichRun, tables: &RtfRunTables, base: &str) {
    let toggles = [
        (run.bold, "\\b ", "\\b0 "),
        (run.italic, "\\i ", "\\i0 "),
        (run.underline, "\\ul ", "\\ulnone "),
        (run.strike, "\\strike ", "\\strike0 "),
    ];
    for (on, open, _) in toggles {
        if on {
            line.push_str(open);
        }
    }
    match run.script {
        RichScript::Normal => {}
        RichScript::Superscript => line.push_str("\\super "),
        RichScript::Subscript => line.push_str("\\sub "),
    }
    line.push_str(&tables.run_words(run));
    line.push_str(&RtfCodec::escape_rtf_text(&run.text));
    if has_extra_run_format(run) {
        line.push_str("\\plain ");
        line.push_str(base);
        return;
    }
    for (on, _, close) in toggles {
        if on {
            line.push_str(close);
        }
    }
}

pub(crate) fn encode_rtf_rich(doc: &RichDocument) -> String {
    let mut lines: Vec<String> = Vec::with_capacity(doc.blocks.len() + 6);
    lines.push("{\\rtf1\\ansi\\deff0".to_string());
    lines.push(format!(
        "{{\\info{{\\title {}}}}}",
        RtfCodec::escape_rtf_text(&doc.display_title())
    ));
    let tables = RtfRunTables::collect(doc.blocks.iter().flat_map(|b| match b {
        RichBlock::Paragraph(p) => p.runs.iter(),
        RichBlock::Table(_) | RichBlock::Image(_) => [].iter(),
    }));
    lines.extend(tables.header_lines());
    let has_list = doc
        .blocks
        .iter()
        .any(|b| matches!(b, RichBlock::Paragraph(p) if list_kind_of(p).is_some()));
    if has_list {
        lines.extend(list_table_lines());
    }
    let mut table_idx = 0usize;
    for block in &doc.blocks {
        match block {
            RichBlock::Paragraph(p) => lines.push(rtf_paragraph_line(p, &tables)),
            RichBlock::Image(img) => {
                let stand_in = RichParagraph {
                    runs: vec![RichRun::plain(img.fallback_text())],
                    ..RichParagraph::default()
                };
                lines.push(rtf_paragraph_line(&stand_in, &tables));
            }
            RichBlock::Table(table) => {
                lines.extend(rtf_table_lines(table, table_idx));
                table_idx += 1;
            }
        }
    }
    lines.push("}".to_string());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtf_format_word_stripping_keeps_escapes_and_unicode() {
        assert_eq!(
            strip_rtf_format_words("\\qc\\outlinelevel0\\fs32 \\b A\\\\b\\b0 \\u-21504?\\line x"),
            "A\\\\b\\u-21504?\\line x"
        );
        assert_eq!(strip_rtf_format_words(" plain"), "plain");
    }

    #[test]
    fn paragraph_property_words_are_stripped_with_their_numbers() {
        assert_eq!(
            strip_rtf_format_words(
                "\\plain\\ls2\\ilvl0\\li720\\fi-360\\sl360\\slmult1 \\super 2\\plain x"
            ),
            "2x"
        );
    }

    #[test]
    fn line_spacing_percent_survives_the_reader_rounding() {
        for percent in 1..=1000u16 {
            let sl = i64::from(line_spacing_240ths(percent));
            assert_eq!((sl * 100 + 120) / 240, i64::from(percent), "{percent}%");
        }
    }
}

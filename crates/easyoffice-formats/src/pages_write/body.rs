//! The body text storage: paragraphs as one string and the attribute tables that style it.
//!
//! Text and formatting are stored apart. Field 3 is the text (paragraphs separated by `\n`);
//! everything else is a table of entries anchored to UTF-16 positions: the paragraph-style table
//! (field 5) has an entry for every paragraph, the character-style table (field 8) one entry per
//! change of look, and the list, paragraph-data, layout and bidi tables one entry at 0.
//!
//! 참조 구현: iwork-rs (MIT, Copyright (c) 2026 Zebra Pig), `FORMAT.md` "Text — TSWP.StorageArchive"
//! (454-594행)와 "The paragraph-style table has an entry for every paragraph"(728-770행),
//! `src/create.rs` 의 `body_storage`(1058-1088행).

use super::package::Blueprint;
use super::styles::{CharLook, Styles};
use super::wire::{msg, reference, string, varint, Fld};
use crate::{EasyOfficeError, RichAlignment, RichBlock, RichDocument, RichParagraph, RichRun};

const TYPE_STORAGE: u32 = 2001;
/// The reader's own ceilings (`rich_read_pages`): a document it would refuse is not written.
pub(super) const MAX_PARAGRAPHS: usize = 200_000;
pub(super) const MAX_RUNS: usize = 1_000_000;
/// Line break inside a paragraph (what the apps write for Shift+Return).
const LINE_BREAK: char = '\u{2028}';

/// Text as the storage holds it: breaks become line breaks, controls and the object marker go.
pub(super) fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push(LINE_BREAK);
            }
            '\n' | '\u{B}' | '\u{C}' | '\u{2028}' | '\u{2029}' => out.push(LINE_BREAK),
            '\t' => out.push(c),
            '\u{FFFC}' => {}
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// What the writer drops from a run or a paragraph (counted for the save warning).
pub(super) fn run_loses_format(run: &RichRun) -> bool {
    run.strike
        || run.link.is_some()
        || run.font.is_some()
        || run.highlight.is_some()
        || run.script != crate::RichScript::Normal
}

pub(super) fn paragraph_loses_format(p: &RichParagraph) -> bool {
    p.left_indent_twips.is_some()
        || p.first_line_indent_twips.is_some()
        || p.line_spacing_percent.is_some()
}

fn look_of(run: &RichRun) -> CharLook {
    let hex = run.color.as_deref().unwrap_or_default();
    let channel = |at: usize| {
        hex.get(at..at + 2)
            .and_then(|h| u8::from_str_radix(h, 16).ok())
    };
    CharLook {
        bold: run.bold,
        italic: run.italic,
        underline: run.underline,
        size_half_pt: run.size_half_pt.filter(|s| *s > 0),
        color: match (channel(0), channel(2), channel(4)) {
            (Some(r), Some(g), Some(b)) if hex.len() == 6 => Some([r, g, b]),
            _ => None,
        },
    }
}

fn alignment_value(alignment: RichAlignment) -> u64 {
    match alignment {
        RichAlignment::Left => 0,
        RichAlignment::Right => 1,
        RichAlignment::Center => 2,
        RichAlignment::Justify => 3,
    }
}

/// The paragraphs the body holds: the paragraph blocks of the document, in order.
pub(super) fn paragraphs(doc: &RichDocument) -> impl Iterator<Item = &RichParagraph> {
    doc.blocks.iter().filter_map(|b| match b {
        RichBlock::Paragraph(p) => Some(p),
        _ => None,
    })
}

fn table(number: u32, entries: Vec<Fld>) -> Fld {
    msg(number, entries)
}

fn entry(index: u64, style: Option<u64>) -> Fld {
    let mut fields = vec![varint(1, index)];
    fields.extend(style.map(|id| reference(2, id)));
    msg(1, fields)
}

fn single(number: u32, style: u64) -> Fld {
    table(number, vec![entry(0, Some(style))])
}

fn zero_entry(number: u32) -> Fld {
    table(
        number,
        vec![msg(1, vec![varint(1, 0), varint(2, 0), varint(3, 0)])],
    )
}

/// Writes the body storage into the `Document` component and returns its identifier.
pub(super) fn write_body(
    bp: &mut Blueprint,
    styles: &mut Styles,
    doc: &RichDocument,
) -> Result<u64, EasyOfficeError> {
    let mut text = String::new();
    let mut offset = 0u64;
    let mut paragraph_entries: Vec<Fld> = Vec::new();
    let mut character_entries: Vec<(u64, Option<u64>)> = Vec::new();
    let mut current: Option<u64> = None;
    let mut count = 0usize;
    let mut runs = 0usize;
    for paragraph in paragraphs(doc) {
        count += 1;
        if count > MAX_PARAGRAPHS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "Pages 쓰기: 문단 수가 열기 한도를 넘었습니다".to_string(),
            ));
        }
        if count > 1 {
            text.push('\n');
            offset += 1;
        }
        let base = match usize::from(paragraph.heading_level).min(6) {
            0 => styles.body,
            level => styles.headings[level - 1],
        };
        let style = match paragraph.alignment {
            Some(alignment) => styles.aligned(bp, base, alignment_value(alignment))?,
            None => base,
        };
        paragraph_entries.push(entry(offset, Some(style)));
        for run in &paragraph.runs {
            let piece = clean(&run.text);
            if piece.is_empty() {
                continue;
            }
            runs += 1;
            if runs > MAX_RUNS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "Pages 쓰기: 글자 구간 수가 열기 한도를 넘었습니다".to_string(),
                ));
            }
            let look = look_of(run);
            let wanted = if look.is_plain() {
                None
            } else {
                Some(styles.character(bp, look)?)
            };
            if wanted != current {
                character_entries.push((offset, wanted));
                current = wanted;
            }
            text.push_str(&piece);
            offset += piece.encode_utf16().count() as u64;
        }
    }
    if count == 0 {
        paragraph_entries.push(entry(0, Some(styles.body)));
    }

    let mut fields = vec![varint(1, 0), reference(2, styles.stylesheet)];
    if !text.is_empty() {
        fields.push(string(3, &text));
    }
    fields.push(table(5, paragraph_entries));
    fields.push(zero_entry(6));
    fields.push(single(7, styles.list));
    if !character_entries.is_empty() {
        // The first entry is at 0 (no object when the text starts unformatted).
        if character_entries[0].0 != 0 {
            character_entries.insert(0, (0, None));
        }
        fields.push(table(
            8,
            character_entries
                .into_iter()
                .map(|(at, id)| entry(at, id))
                .collect(),
        ));
    }
    fields.push(varint(10, 1));
    fields.push(single(12, styles.body_column));
    fields.push(zero_entry(14));
    fields.push(zero_entry(24));
    let component = bp.document();
    bp.add(component, TYPE_STORAGE, &fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breaks_and_controls_are_cleaned() {
        assert_eq!(
            clean("a\r\nb\nc\u{0}d\u{FFFC}e\tf"),
            "a\u{2028}b\u{2028}cde\tf"
        );
    }

    #[test]
    fn colors_parse_from_six_hex_digits_only() {
        let run = |c: &str| RichRun {
            color: Some(c.to_string()),
            ..RichRun::plain("x")
        };
        assert_eq!(look_of(&run("FF8000")).color, Some([255, 128, 0]));
        assert_eq!(look_of(&run("FF80")).color, None);
        assert_eq!(look_of(&run("GG0000")).color, None);
        assert!(look_of(&RichRun::plain("x")).is_plain());
    }
}

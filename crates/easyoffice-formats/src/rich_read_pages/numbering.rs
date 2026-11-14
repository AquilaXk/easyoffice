//! Document-level note settings (`TP.SettingsArchive`: footnote or endnote kind, number format
//! and restart rule) and the label text they produce, plus list number formats of list styles.
//!
//! 검증 필요: the field numbers and enum values are from public notes (independent MIT
//! implementation); the fixtures carry the default settings only (footnotes, numeric, continuous).

use crate::pages_proto::Message;
use crate::pages_schema::settings;
use crate::{RichListNumber, RichNumberEnclosure, RichNumberFormat};

/// Footnote settings of a document.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct NoteSettings {
    /// Notes are collected at the end of the document or section instead of the page.
    pub endnotes: bool,
    pub format: u64,
    pub numbering: u64,
}

/// Document settings that change how the body is read.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct DocumentSettings {
    pub notes: NoteSettings,
    /// A page-layout document: no flowing body, content lives in drawables on pages.
    pub page_layout: bool,
}

impl DocumentSettings {
    pub(super) fn read(payload: &Message<'_>) -> Self {
        Self {
            notes: NoteSettings {
                endnotes: payload.varint(settings::NOTE_KIND).unwrap_or(0) != 0,
                format: payload.varint(settings::NOTE_FORMAT).unwrap_or(0),
                numbering: payload.varint(settings::NOTE_NUMBERING).unwrap_or(0),
            },
            page_layout: payload.varint(settings::BODY) == Some(0),
        }
    }
}

const SYMBOLS: [&str; 6] = ["*", "\u{2020}", "\u{2021}", "\u{A7}", "\u{2016}", "\u{B6}"];
const KANJI_DIGITS: [char; 10] = ['〇', '一', '二', '三', '四', '五', '六', '七', '八', '九'];
const KANJI_TEN: char = '十';
const ROMAN: [(usize, &str); 13] = [
    (1000, "m"),
    (900, "cm"),
    (500, "d"),
    (400, "cd"),
    (100, "c"),
    (90, "xc"),
    (50, "l"),
    (40, "xl"),
    (10, "x"),
    (9, "ix"),
    (5, "v"),
    (4, "iv"),
    (1, "i"),
];
const MAX_ROMAN: usize = 3999;
const MAX_SYMBOL_REPEAT: usize = 64;
const FULLWIDTH_ZERO: u32 = 0xFF10;

fn roman(mut n: usize) -> String {
    let mut out = String::new();
    for (value, text) in ROMAN {
        while n >= value {
            out.push_str(text);
            n -= value;
        }
    }
    out
}

fn kanji(n: usize) -> String {
    let digit = |d: usize| KANJI_DIGITS[d % 10];
    match n {
        1..=9 => digit(n).to_string(),
        10 => KANJI_TEN.to_string(),
        11..=99 => {
            let tens = if n / 10 == 1 {
                String::new()
            } else {
                digit(n / 10).to_string()
            };
            let ones = if n % 10 == 0 {
                String::new()
            } else {
                digit(n % 10).to_string()
            };
            format!("{tens}{KANJI_TEN}{ones}")
        }
        _ => n.to_string(),
    }
}

/// Text of the `number`-th (1-based) note mark in a note format.
pub(super) fn note_label(format: u64, number: usize) -> String {
    match format {
        settings::FORMAT_ROMAN if (1..=MAX_ROMAN).contains(&number) => roman(number),
        settings::FORMAT_SYMBOLIC if number >= 1 => {
            let k = number - 1;
            SYMBOLS[k % SYMBOLS.len()].repeat((k / SYMBOLS.len() + 1).min(MAX_SYMBOL_REPEAT))
        }
        settings::FORMAT_JAPANESE_NUMERIC => number
            .to_string()
            .chars()
            .filter_map(|c| c.to_digit(10))
            .filter_map(|d| char::from_u32(FULLWIDTH_ZERO + d))
            .collect(),
        settings::FORMAT_JAPANESE_IDEOGRAPHIC => kanji(number),
        _ => number.to_string(),
    }
}

/// Number format of a list level from the style's number-format value; `None` for an unknown value.
/// 0-2 decimal, 3-5 upper-case roman, 6-8 lower-case roman, 9-14 Latin letters (lower then upper
/// case; 9 = `a.` is the fixtures' a-b-c list), each group as `1.`, `(1)`, `1)`; 45 is the Hangul
/// syllable list of the Korean locale (fixtures). 검증 필요 beyond the fixture values.
pub(super) fn list_number(value: u64) -> Option<RichListNumber> {
    let (format, group) = match value {
        0..=2 => (RichNumberFormat::Decimal, value),
        3..=5 => (RichNumberFormat::UpperRoman, value - 3),
        6..=8 => (RichNumberFormat::LowerRoman, value - 6),
        9..=11 => (RichNumberFormat::LowerLatin, value - 9),
        12..=14 => (RichNumberFormat::UpperLatin, value - 12),
        45 => (RichNumberFormat::HangulSyllable, 0),
        _ => return None,
    };
    let enclosure = match group {
        0 => RichNumberEnclosure::Dot,
        1 => RichNumberEnclosure::Parentheses,
        _ => RichNumberEnclosure::RightParenthesis,
    };
    Some(RichListNumber { format, enclosure })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_labels() {
        assert_eq!(note_label(0, 12), "12");
        assert_eq!(note_label(settings::FORMAT_ROMAN, 14), "xiv");
        assert_eq!(note_label(settings::FORMAT_ROMAN, 4000), "4000");
        assert_eq!(note_label(settings::FORMAT_SYMBOLIC, 1), "*");
        assert_eq!(note_label(settings::FORMAT_SYMBOLIC, 7), "**");
        assert_eq!(note_label(settings::FORMAT_JAPANESE_NUMERIC, 12), "１２");
        assert_eq!(
            note_label(settings::FORMAT_JAPANESE_IDEOGRAPHIC, 12),
            "十二"
        );
        assert_eq!(
            note_label(settings::FORMAT_JAPANESE_IDEOGRAPHIC, 20),
            "二十"
        );
        assert_eq!(note_label(99, 3), "3");
    }

    #[test]
    fn note_label_symbolic_caps_repeat_at_64() {
        let huge = note_label(settings::FORMAT_SYMBOLIC, usize::MAX);
        assert_eq!(huge.chars().count(), 64);
        assert_eq!(
            note_label(settings::FORMAT_SYMBOLIC, 6 * 63 + 1)
                .chars()
                .count(),
            64
        );
    }

    #[test]
    fn list_numbers() {
        let n = list_number(10).unwrap();
        assert_eq!(
            (n.format, n.enclosure),
            (
                RichNumberFormat::LowerLatin,
                RichNumberEnclosure::Parentheses
            )
        );
        assert_eq!(list_number(0).unwrap().format, RichNumberFormat::Decimal);
        assert!(list_number(7).is_some() && list_number(200).is_none());
    }
}

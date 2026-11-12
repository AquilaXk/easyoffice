//! Conditional formatting and data validation rules of a sheet (PD-171).
//!
//! The model is shared by the XLSX/ODS readers and writers (`xlsx_rules.rs`, `ods_rules.rs`), the
//! page's JSON (`easyoffice-app`) and the evaluator (`easyoffice-sheet`). Operands stay text: a
//! number, a quoted string or a formula, so a rule survives a round trip without being evaluated.
//! Rules the model cannot represent are not stored; the reader names them in
//! [`SheetRules::unsupported`] so the caller can warn instead of dropping them silently.

use crate::rtf_xml_helpers::format_col_letters;

// The readers and writers of the model sit beside it; they are internal to the crate.
#[path = "ods_rules.rs"]
pub(crate) mod ods_rules;
#[path = "ods_rules_read.rs"]
pub(crate) mod ods_rules_read;
#[path = "xlsx_rules.rs"]
pub(crate) mod xlsx_rules;

/// Rules (formatting plus validation) one sheet may carry.
pub const MAX_RULES_PER_SHEET: usize = 256;
/// Ranges one rule may cover.
pub const MAX_RANGES_PER_RULE: usize = 32;
/// Characters of one operand, formula, message or list item.
pub const MAX_RULE_TEXT_CHARS: usize = 1_024;
/// Thresholds of a colour scale or icon set (spreadsheet programs allow 2 or 3 and up to 5).
pub const MAX_RULE_STOPS: usize = 5;

/// Zero-based inclusive cell rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellBox {
    pub row0: u32,
    pub col0: u32,
    pub row1: u32,
    pub col1: u32,
}

const LAST_ROW: u32 = 1_048_575;
const LAST_COL: u32 = 16_383;

fn parse_col_letters(s: &str) -> Option<u32> {
    if s.is_empty() || s.len() > 3 || !s.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let col = s.bytes().fold(0u32, |n, b| {
        n * 26 + u32::from(b.to_ascii_uppercase() - b'A' + 1)
    }) - 1;
    (col <= LAST_COL).then_some(col)
}

fn parse_row_digits(s: &str) -> Option<u32> {
    if s.is_empty() || s.len() > 7 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let row: u32 = s.parse().ok()?;
    (1..=LAST_ROW + 1).contains(&row).then(|| row - 1)
}

/// One endpoint: `A1` (both), `A` (column only) or `1` (row only). `$` marks are ignored.
fn parse_endpoint(s: &str) -> Option<(Option<u32>, Option<u32>)> {
    let s: String = s.chars().filter(|&c| c != '$').collect();
    let split = s.find(|c: char| c.is_ascii_digit()).unwrap_or(s.len());
    let (letters, digits) = s.split_at(split);
    let col = if letters.is_empty() {
        None
    } else {
        Some(parse_col_letters(letters)?)
    };
    let row = if digits.is_empty() {
        None
    } else {
        Some(parse_row_digits(digits)?)
    };
    (col.is_some() || row.is_some()).then_some((row, col))
}

impl CellBox {
    pub const fn cell(row: u32, col: u32) -> Self {
        Self {
            row0: row,
            col0: col,
            row1: row,
            col1: col,
        }
    }

    #[inline]
    pub const fn contains(&self, row: u32, col: u32) -> bool {
        row >= self.row0 && row <= self.row1 && col >= self.col0 && col <= self.col1
    }

    pub const fn area(&self) -> u64 {
        (self.row1 - self.row0 + 1) as u64 * (self.col1 - self.col0 + 1) as u64
    }

    /// `A1`, `A1:B7`, `A:C` or `2:5` (any `$` marks and the sheet prefix are ignored by callers).
    pub fn parse(text: &str) -> Option<Self> {
        let (head, tail) = text.split_once(':').unwrap_or((text, text));
        let (r0, c0) = parse_endpoint(head.trim())?;
        let (r1, c1) = parse_endpoint(tail.trim())?;
        let (row0, row1) = match (r0, r1) {
            (Some(a), Some(b)) => (a.min(b), a.max(b)),
            (None, None) => (0, LAST_ROW),
            _ => return None,
        };
        let (col0, col1) = match (c0, c1) {
            (Some(a), Some(b)) => (a.min(b), a.max(b)),
            (None, None) => (0, LAST_COL),
            _ => return None,
        };
        Some(Self {
            row0,
            col0,
            row1,
            col1,
        })
    }

    pub fn to_a1(&self) -> String {
        let corner = |r: u32, c: u32| format!("{}{}", format_col_letters(c), r + 1);
        if self.row0 == self.row1 && self.col0 == self.col1 {
            corner(self.row0, self.col0)
        } else {
            format!(
                "{}:{}",
                corner(self.row0, self.col0),
                corner(self.row1, self.col1)
            )
        }
    }
}

/// Space-separated `sqref` list. Unparsable parts are dropped.
pub fn parse_sqref(text: &str) -> Vec<CellBox> {
    text.split_whitespace()
        .filter_map(CellBox::parse)
        .take(MAX_RANGES_PER_RULE)
        .collect()
}

pub fn format_sqref(boxes: &[CellBox]) -> String {
    boxes
        .iter()
        .map(CellBox::to_a1)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Comparison of a cell against one or two operands (OOXML `ST_ConditionalFormattingOperator`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmp {
    Between,
    NotBetween,
    Equal,
    NotEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
}

impl Cmp {
    const NAMES: [(Cmp, &'static str); 8] = [
        (Cmp::Between, "between"),
        (Cmp::NotBetween, "notBetween"),
        (Cmp::Equal, "equal"),
        (Cmp::NotEqual, "notEqual"),
        (Cmp::Greater, "greaterThan"),
        (Cmp::GreaterEqual, "greaterThanOrEqual"),
        (Cmp::Less, "lessThan"),
        (Cmp::LessEqual, "lessThanOrEqual"),
    ];

    pub fn name(self) -> &'static str {
        Self::NAMES
            .iter()
            .find(|(c, _)| *c == self)
            .map_or("equal", |(_, n)| n)
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .find(|(_, n)| *n == name)
            .map(|(c, _)| *c)
    }

    pub const fn takes_two(self) -> bool {
        matches!(self, Cmp::Between | Cmp::NotBetween)
    }
}

pub const FLAG_BOLD: u8 = 1;
pub const FLAG_ITALIC: u8 = 2;
pub const FLAG_UNDERLINE: u8 = 4;
pub const FLAG_STRIKE: u8 = 8;

/// Differential format of a rule: the parts a cell shows when the rule matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct CfStyle {
    /// Fill, `0xRRGGBB`.
    pub bg: Option<u32>,
    /// Font colour, `0xRRGGBB`.
    pub fg: Option<u32>,
    /// `FLAG_*` bits.
    pub flags: u8,
}

impl CfStyle {
    pub const fn is_plain(&self) -> bool {
        self.bg.is_none() && self.fg.is_none() && self.flags == 0
    }
}

/// How a threshold of a scale, bar or icon set is found (OOXML `cfvo`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Num,
    Min,
    Max,
    Percent,
    Percentile,
    Formula,
}

impl ValueKind {
    const NAMES: [(ValueKind, &'static str); 6] = [
        (ValueKind::Num, "num"),
        (ValueKind::Min, "min"),
        (ValueKind::Max, "max"),
        (ValueKind::Percent, "percent"),
        (ValueKind::Percentile, "percentile"),
        (ValueKind::Formula, "formula"),
    ];

    pub fn name(self) -> &'static str {
        Self::NAMES
            .iter()
            .find(|(k, _)| *k == self)
            .map_or("num", |(_, n)| n)
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .find(|(_, n)| *n == name)
            .map(|(k, _)| *k)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfValue {
    pub kind: ValueKind,
    /// Number, percent, percentile or formula text; empty for `min`/`max`.
    pub text: String,
    /// Icon sets: the icon applies at `>=` the threshold (true) or only above it.
    pub gte: bool,
}

impl CfValue {
    pub fn new(kind: ValueKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
            gte: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextOp {
    Contains,
    NotContains,
    Begins,
    Ends,
}

/// Icon sets in OOXML order. An icon set with `n` icons is drawn lowest first.
pub const ICON_SETS: [(&str, u8); 20] = [
    ("3Arrows", 3),
    ("3ArrowsGray", 3),
    ("3Flags", 3),
    ("3TrafficLights1", 3),
    ("3TrafficLights2", 3),
    ("3Signs", 3),
    ("3Symbols", 3),
    ("3Symbols2", 3),
    ("3Stars", 3),
    ("3Triangles", 3),
    ("4Arrows", 4),
    ("4ArrowsGray", 4),
    ("4RedToBlack", 4),
    ("4Rating", 4),
    ("4TrafficLights", 4),
    ("5Arrows", 5),
    ("5ArrowsGray", 5),
    ("5Rating", 5),
    ("5Quarters", 5),
    ("5Boxes", 5),
];

pub fn icon_set_index(name: &str) -> Option<u8> {
    ICON_SETS
        .iter()
        .position(|(n, _)| *n == name)
        .map(|i| i as u8)
}

#[derive(Debug, Clone, PartialEq)]
pub enum CfKind {
    /// Cell value against one or two operands.
    CellIs {
        cmp: Cmp,
        a: String,
        b: String,
    },
    Text {
        op: TextOp,
        text: String,
    },
    /// Empty or whitespace-only cells (`not`: any other cell).
    Blanks {
        not: bool,
    },
    /// Error values (`not`: any other cell).
    Errors {
        not: bool,
    },
    /// Top or bottom `n` values, or `n` percent.
    Top {
        n: u32,
        percent: bool,
        bottom: bool,
    },
    /// Above or below the range average, `std_dev` standard deviations away.
    Average {
        above: bool,
        equal: bool,
        std_dev: u8,
    },
    Duplicates {
        unique: bool,
    },
    /// A formula written for the top-left cell of the first range.
    Expression(String),
    /// Two or three `(threshold, colour)` stops, lowest first.
    ColorScale(Vec<(CfValue, u32)>),
    DataBar {
        min: CfValue,
        max: CfValue,
        positive: u32,
        negative: u32,
        gradient: bool,
        show_value: bool,
    },
    IconSet {
        set: u8,
        /// One threshold per icon, lowest first; the first is always met.
        thresholds: Vec<CfValue>,
        reverse: bool,
        show_value: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CfRule {
    pub ranges: Vec<CellBox>,
    /// Lower numbers win; unique per sheet.
    pub priority: u32,
    pub stop_if_true: bool,
    pub kind: CfKind,
    /// Used by the kinds that format by style (not scale, bar or icon set).
    pub style: CfStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DvKind {
    Whole,
    Decimal,
    List,
    Date,
    Time,
    TextLength,
    Custom,
}

impl DvKind {
    const NAMES: [(DvKind, &'static str); 7] = [
        (DvKind::Whole, "whole"),
        (DvKind::Decimal, "decimal"),
        (DvKind::List, "list"),
        (DvKind::Date, "date"),
        (DvKind::Time, "time"),
        (DvKind::TextLength, "textLength"),
        (DvKind::Custom, "custom"),
    ];

    pub fn name(self) -> &'static str {
        Self::NAMES
            .iter()
            .find(|(k, _)| *k == self)
            .map_or("whole", |(_, n)| n)
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .find(|(_, n)| *n == name)
            .map(|(k, _)| *k)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DvAlert {
    Stop,
    Warning,
    Information,
}

impl DvAlert {
    pub fn name(self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::Warning => "warning",
            Self::Information => "information",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "stop" => Some(Self::Stop),
            "warning" => Some(Self::Warning),
            "information" => Some(Self::Information),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DataValidation {
    pub ranges: Vec<CellBox>,
    pub kind: DvKind,
    pub cmp: Cmp,
    /// First operand. For `List`: `a,b,c` items, or `=Sheet1!$A$1:$A$9` (leading `=`) for a range.
    pub a: String,
    pub b: String,
    pub allow_blank: bool,
    pub show_dropdown: bool,
    pub alert: DvAlert,
    pub show_error: bool,
    pub error_title: String,
    pub error: String,
    pub show_prompt: bool,
    pub prompt_title: String,
    pub prompt: String,
}

impl DataValidation {
    pub fn new(ranges: Vec<CellBox>, kind: DvKind) -> Self {
        Self {
            ranges,
            kind,
            cmp: Cmp::Between,
            a: String::new(),
            b: String::new(),
            allow_blank: true,
            show_dropdown: true,
            alert: DvAlert::Stop,
            show_error: true,
            error_title: String::new(),
            error: String::new(),
            show_prompt: false,
            prompt_title: String::new(),
            prompt: String::new(),
        }
    }
}

/// Rules of one sheet.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SheetRules {
    pub cf: Vec<CfRule>,
    pub dv: Vec<DataValidation>,
    /// Rules in the file that the model cannot hold (kind names, one entry per rule).
    pub unsupported: Vec<String>,
}

impl SheetRules {
    pub fn is_empty(&self) -> bool {
        self.cf.is_empty() && self.dv.is_empty()
    }

    pub fn len(&self) -> usize {
        self.cf.len() + self.dv.len()
    }

    /// Clamps the lists to the sheet limits (a reader never grows past them).
    pub fn truncate_to_limits(&mut self) {
        let room = MAX_RULES_PER_SHEET.saturating_sub(self.cf.len());
        self.dv.truncate(room);
        self.cf.truncate(MAX_RULES_PER_SHEET);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxes_parse_and_format() {
        let b = CellBox::parse("$B$2:D10").unwrap();
        assert_eq!(
            b,
            CellBox {
                row0: 1,
                col0: 1,
                row1: 9,
                col1: 3
            }
        );
        assert_eq!(b.to_a1(), "B2:D10");
        assert_eq!(CellBox::parse("c3").unwrap().to_a1(), "C3");
        assert_eq!(CellBox::parse("D10:B2").unwrap(), b);
        let col = CellBox::parse("A:A").unwrap();
        assert_eq!(
            (col.row0, col.row1, col.col0, col.col1),
            (0, LAST_ROW, 0, 0)
        );
        assert_eq!(CellBox::parse("2:3").unwrap().area(), 2 * 16_384);
        assert!(CellBox::parse("A1:B").is_none());
        assert!(CellBox::parse("ZZZZ1").is_none());
        assert!(CellBox::parse("A0").is_none());
        assert!(CellBox::parse("").is_none());
    }

    #[test]
    fn sqref_lists_are_bounded() {
        let text = (1..=100)
            .map(|r| format!("A{r}"))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(parse_sqref(&text).len(), MAX_RANGES_PER_RULE);
        assert_eq!(format_sqref(&parse_sqref("A1 B2:C3")), "A1 B2:C3");
    }

    #[test]
    fn names_round_trip() {
        for (cmp, name) in Cmp::NAMES {
            assert_eq!(Cmp::from_name(name), Some(cmp));
            assert_eq!(cmp.name(), name);
        }
        assert_eq!(icon_set_index("5Quarters"), Some(18));
        assert_eq!(DvKind::from_name("textLength"), Some(DvKind::TextLength));
    }
}

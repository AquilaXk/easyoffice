//! What one column's condition accepts: a list of values, or one or two custom conditions.
//!
//! 참조 구현: Univer `packages/sheets-filter/src/models/filter-model.ts` lines 470-580
//! (`generateFilterFn`, `filterByValuesFnFactory`, `customFilterFnFactory`, `generateCustomFilterFn`)
//! and `custom-filters.ts` lines 52-152 (`greaterThan` ... `notEquals`, `textMatch`), Apache-2.0,
//! Copyright 2023-present DreamNum Co., Ltd., commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9.
//! The rules are the reference's: a numeric operator accepts only number cells and a condition
//! whose text is not a number accepts nothing; "not equal" accepts every non-number cell and,
//! for a pattern, excludes the cells that match it (an empty cell is not excluded); a text
//! condition is a whole-text wildcard match, and a pattern of one space means "not empty".
//!
//! Modifications: letter case is ignored for "not equal" patterns too (the reference compares
//! them exactly), a plain number written as text also equals a number cell that has the same
//! value (`1.0` against `1`), the wildcard match is the linear `wildcard` module, and the cell
//! text is the displayed text the caller passes in.

use super::aux::{CellAux, NO_COLOR};
use super::dates::DateKeys;
use super::wildcard::Wildcard;
use easyoffice_formats::{CustomCondition, CustomOp, FilterCriteria};
use std::collections::HashSet;
use std::hash::{BuildHasherDefault, Hasher};

/// A multiply-rotate hash for short texts (the lists the filter keeps are not attacker-chosen
/// keys of a long-lived table, and a bucket collision only costs a comparison).
#[derive(Default)]
pub(super) struct TextHasher(u64);

impl Hasher for TextHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        const K: u64 = 0x517c_c1b7_2722_0a95;
        let mut chunks = bytes.chunks_exact(8);
        for chunk in &mut chunks {
            let word = u64::from_le_bytes(chunk.try_into().unwrap_or([0; 8]));
            self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(K);
        }
        let mut tail = 0u64;
        for (i, &b) in chunks.remainder().iter().enumerate() {
            tail |= u64::from(b) << (8 * i);
        }
        self.0 = (self.0.rotate_left(5) ^ tail ^ (bytes.len() as u64)).wrapping_mul(K);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}

pub(super) type TextSet = HashSet<Box<str>, BuildHasherDefault<TextHasher>>;

/// The number a cell text stands for.
#[inline]
pub fn number_of(text: &str) -> Option<f64> {
    let first = *text.as_bytes().first()?;
    // Cheap rejection: a number starts with a digit, a sign or a point (or space padding).
    if !(first.is_ascii_digit() || matches!(first, b'-' | b'+' | b'.' | b' ')) {
        return None;
    }
    text.trim().parse::<f64>().ok().filter(|n| n.is_finite())
}

#[derive(Clone, Copy)]
enum Cmp {
    Gt,
    Ge,
    Lt,
    Le,
}

enum Cond {
    /// A numeric operator with a text that is not a number.
    Never,
    /// Text match; `number` is the text's value when it is a plain number.
    Text {
        pattern: Wildcard,
        number: Option<f64>,
        empty: bool,
    },
    /// "Not equal" with a pattern; a pattern of one space (or none) means "not empty".
    NotText {
        pattern: Wildcard,
        not_empty: bool,
    },
    Compare(Cmp, f64),
    Equal(f64),
    NotEqual(f64),
}

impl Cond {
    fn compile(c: &CustomCondition) -> Option<Self> {
        let number = number_of(&c.text);
        Some(match c.op {
            None => Self::Text {
                pattern: Wildcard::new(&c.text)?,
                number: number.filter(|_| !c.text.contains(['*', '?', '~'])),
                empty: c.text.is_empty(),
            },
            Some(CustomOp::NotEqual) if number.is_none() => Self::NotText {
                pattern: Wildcard::new(&c.text)?,
                not_empty: c.text == " " || c.text.is_empty(),
            },
            Some(op) => number.map_or(Self::Never, |n| Self::numeric(op, n)),
        })
    }

    fn numeric(op: CustomOp, n: f64) -> Self {
        match op {
            CustomOp::Equal => Self::Equal(n),
            CustomOp::NotEqual => Self::NotEqual(n),
            CustomOp::GreaterThan => Self::Compare(Cmp::Gt, n),
            CustomOp::GreaterThanOrEqual => Self::Compare(Cmp::Ge, n),
            CustomOp::LessThan => Self::Compare(Cmp::Lt, n),
            CustomOp::LessThanOrEqual => Self::Compare(Cmp::Le, n),
        }
    }

    fn accepts(&self, text: &str, scratch: &mut Vec<char>) -> bool {
        match self {
            Self::Never => false,
            Self::Text {
                pattern,
                number,
                empty,
            } => {
                if text.is_empty() {
                    return *empty;
                }
                if let Some(n) = number {
                    if number_of(text) == Some(*n) {
                        return true;
                    }
                }
                pattern.is_match(text, scratch)
            }
            Self::NotText { pattern, not_empty } => {
                if *not_empty {
                    return !text.is_empty();
                }
                text.is_empty() || !pattern.is_match(text, scratch)
            }
            Self::Compare(cmp, rhs) => number_of(text).is_some_and(|n| match cmp {
                Cmp::Gt => n > *rhs,
                Cmp::Ge => n >= *rhs,
                Cmp::Lt => n < *rhs,
                Cmp::Le => n <= *rhs,
            }),
            Self::Equal(rhs) => number_of(text) == Some(*rhs),
            Self::NotEqual(rhs) => number_of(text) != Some(*rhs),
        }
    }
}

enum Test {
    Values {
        blank: bool,
        set: TextSet,
        dates: DateKeys,
    },
    One(Cond),
    And(Cond, Cond),
    Or(Cond, Cond),
    /// The fill (or font) colour of the cell, `NO_COLOR` for the cells with none.
    Color {
        font: bool,
        color: u32,
    },
    /// Compares the cell with the other cells of the column; judged by `stats`, never per cell.
    Statistic,
}

/// A column's condition ready to test cell texts.
pub struct ColumnMatcher(Test);

impl ColumnMatcher {
    /// `None` when a pattern is over the wildcard bound.
    pub fn compile(criteria: &FilterCriteria) -> Option<Self> {
        Some(Self(match criteria {
            FilterCriteria::Values {
                blank,
                values,
                dates,
            } => Test::Values {
                blank: *blank,
                set: values.iter().map(|v| v.as_str().into()).collect(),
                dates: DateKeys::new(dates),
            },
            FilterCriteria::Color { font, color } => Test::Color {
                font: *font,
                color: color.unwrap_or(NO_COLOR),
            },
            FilterCriteria::Top { .. } | FilterCriteria::Average { .. } => Test::Statistic,
            FilterCriteria::Custom { and, first, second } => {
                let first = Cond::compile(first)?;
                match (second, and) {
                    (None, _) => Test::One(first),
                    (Some(second), true) => Test::And(first, Cond::compile(second)?),
                    (Some(second), false) => Test::Or(first, Cond::compile(second)?),
                }
            }
        }))
    }

    /// Whether the criterion compares a cell with the rest of its column (the largest numbers, the
    /// average): those are judged after the others, over the rows still visible.
    pub const fn is_statistic(&self) -> bool {
        matches!(self.0, Test::Statistic)
    }

    /// Whether a row whose cell shows `text` stays visible, judging by the text alone (a colour or a
    /// date group needs [`Self::accepts_at`]). `scratch` is reused between calls.
    pub fn accepts(&self, text: &str, scratch: &mut Vec<char>) -> bool {
        self.accepts_at(0, text, &CellAux::default(), scratch)
    }

    /// Whether data row `i`, whose cell shows `text` and has the facts `aux`, stays visible.
    pub fn accepts_at(&self, i: usize, text: &str, aux: &CellAux, scratch: &mut Vec<char>) -> bool {
        match &self.0 {
            Test::Values { blank, set, dates } => {
                if text.is_empty() {
                    *blank
                } else {
                    set.contains(text) || aux.date(i).is_some_and(|serial| dates.covers(serial))
                }
            }
            Test::One(a) => a.accepts(text, scratch),
            Test::And(a, b) => a.accepts(text, scratch) && b.accepts(text, scratch),
            Test::Or(a, b) => a.accepts(text, scratch) || b.accepts(text, scratch),
            Test::Color { font, color } => !aux.has_color(*font) || aux.color(*font, i) == *color,
            Test::Statistic => true,
        }
    }
}

//! Rewrites the references in formula text for a row or column insertion or deletion.
//!
//! 참조 구현: Univer `packages/sheets-formula/src/controllers/utils/ref-range-formula.ts` lines
//! 361-403 and `packages/sheets/src/services/ref-range/util.ts` lines 469-690 (see the header of
//! `axis/mod.rs`, Apache-2.0, Copyright 2023-present DreamNum Co., Ltd.). The reference rebuilds a
//! formula from its token sequence; this port scans the text, so it is the same decision per
//! reference with the formula's other characters left as written. Absolute (`$`) and relative
//! references move alike, as in every spreadsheet program.
//!
//! References that name another sheet move only when that sheet is the edited one. A reference
//! whose cells are all deleted becomes `#REF!` (the sheet name in front of it goes too).

use super::{Axis, AxisEdit, EditKind};
use crate::PackedCellCoordinate;

const MAX_COL_LETTERS: usize = 3;

#[inline]
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// One end of a reference, zero-based, with its `$` marks. A cell has both parts, an end of
/// `A:C` only a column and an end of `1:3` only a row.
#[derive(Clone, Copy)]
struct Endpoint {
    col: Option<(u32, bool)>,
    row: Option<(u32, bool)>,
    end: usize,
}

impl Endpoint {
    fn on(&self, axis: Axis) -> Option<(u32, bool)> {
        match axis {
            Axis::Row => self.row,
            Axis::Col => self.col,
        }
    }

    fn set(&mut self, axis: Axis, value: u32) {
        let part = match axis {
            Axis::Row => &mut self.row,
            Axis::Col => &mut self.col,
        };
        if let Some((v, _)) = part {
            *v = value;
        }
    }

    fn is_cell(&self) -> bool {
        self.col.is_some() && self.row.is_some()
    }

    fn write(&self, out: &mut String) {
        if let Some((col, dollar)) = self.col {
            if dollar {
                out.push('$');
            }
            let mut letters = [0u8; 4];
            let mut pos = letters.len();
            let mut n = col + 1;
            while n > 0 {
                pos -= 1;
                letters[pos] = b'A' + ((n - 1) % 26) as u8;
                n = (n - 1) / 26;
            }
            out.extend(letters[pos..].iter().map(|&b| char::from(b)));
        }
        if let Some((row, dollar)) = self.row {
            if dollar {
                out.push('$');
            }
            out.push_str(&(row + 1).to_string());
        }
    }
}

/// An endpoint at `i`: `$A$1`, `A`, or `$12`. Letters first, then digits; nothing else.
fn parse_endpoint(bytes: &[u8], i: usize) -> Option<Endpoint> {
    let mut cur = i;
    let col_dollar = bytes.get(cur) == Some(&b'$');
    if col_dollar {
        cur += 1;
    }
    let letters_start = cur;
    while bytes.get(cur).is_some_and(u8::is_ascii_alphabetic) {
        cur += 1;
    }
    let letters = cur - letters_start;
    if letters > MAX_COL_LETTERS {
        return None;
    }
    let col = (letters > 0).then(|| {
        bytes[letters_start..cur].iter().fold(0u32, |n, b| {
            n * 26 + u32::from(b.to_ascii_uppercase() - b'A' + 1)
        }) - 1
    });
    if col.is_some_and(|c| c >= PackedCellCoordinate::MAX_COLS) {
        return None;
    }
    let (row_dollar, digits_start) = if letters > 0 && bytes.get(cur) == Some(&b'$') {
        (true, cur + 1)
    } else if letters == 0 {
        (col_dollar, cur)
    } else {
        (false, cur)
    };
    let mut end = digits_start;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    let digits = end - digits_start;
    if letters == 0 && digits == 0 {
        return None;
    }
    if digits == 0 && row_dollar && letters > 0 {
        return None;
    }
    let row = if digits > 0 {
        if bytes[digits_start] == b'0' || digits > 7 {
            return None;
        }
        let row: u32 = std::str::from_utf8(&bytes[digits_start..end])
            .ok()?
            .parse()
            .ok()?;
        if row > PackedCellCoordinate::MAX_ROWS {
            return None;
        }
        Some((row - 1, row_dollar))
    } else {
        None
    };
    Some(Endpoint {
        col: col.map(|c| (c, col_dollar)),
        row,
        end: if digits > 0 { end } else { cur },
    })
}

/// A reference starting at `i`: a cell, `A1:B2`, `A:C` or `1:3`; `second` is the range's far end.
#[derive(Clone)]
pub(super) struct Unit {
    first: Endpoint,
    second: Option<Endpoint>,
    /// Text between the colon and the second end (a repeated sheet name), copied as written.
    tail_prefix: std::ops::Range<usize>,
    end: usize,
}

impl Unit {
    /// The sheet prefix repeated in front of a range's second end (`Q1!A1:Q1!B2`), if any.
    pub(super) fn repeated_prefix(&self) -> Option<std::ops::Range<usize>> {
        (!self.tail_prefix.is_empty()).then(|| self.tail_prefix.clone())
    }

    /// Where the cell or range ends.
    pub(super) fn end(&self) -> usize {
        self.end
    }
}

fn follows_ident(bytes: &[u8], end: usize) -> bool {
    bytes
        .get(end)
        .is_some_and(|&b| is_ident_byte(b) || b == b'(' || b == b'!' || b == b'.' || !b.is_ascii())
}

/// Length of a sheet prefix at `i` (`Q1!`, `'My ''Sheet'!`) and the sheet's name.
fn prefix_at(text: &str, i: usize) -> Option<(usize, String)> {
    let bytes = text.as_bytes();
    if bytes.get(i) == Some(&b'\'') {
        let mut name = String::new();
        let mut cur = i + 1;
        loop {
            let rel = text[cur..].find('\'')?;
            name.push_str(&text[cur..cur + rel]);
            cur += rel + 1;
            if bytes.get(cur) == Some(&b'\'') {
                name.push('\'');
                cur += 1;
            } else {
                break;
            }
        }
        return (bytes.get(cur) == Some(&b'!')).then(|| (cur + 1 - i, name));
    }
    let run = bytes[i..]
        .iter()
        .take_while(|&&b| is_ident_byte(b) || b == b'.' || !b.is_ascii())
        .count();
    (run > 0 && bytes.get(i + run) == Some(&b'!')).then(|| (run + 1, text[i..i + run].to_string()))
}

fn parse_unit(text: &str, i: usize, head_sheet: Option<&str>) -> Option<Unit> {
    let bytes = text.as_bytes();
    let first = parse_endpoint(bytes, i)?;
    if bytes.get(first.end) == Some(&b':') {
        let mut at = first.end + 1;
        let mut tail_prefix = at..at;
        if let (Some(head), Some((len, name))) = (head_sheet, prefix_at(text, at)) {
            if name.eq_ignore_ascii_case(head) {
                tail_prefix = at..at + len;
                at += len;
            }
        }
        if let Some(second) = parse_endpoint(bytes, at) {
            let same_kind = first.col.is_some() == second.col.is_some()
                && first.row.is_some() == second.row.is_some();
            if same_kind && !follows_ident(bytes, second.end) {
                return Some(Unit {
                    first,
                    second: Some(second),
                    tail_prefix,
                    end: second.end,
                });
            }
        }
    }
    (first.is_cell() && !follows_ident(bytes, first.end)).then_some(Unit {
        first,
        second: None,
        tail_prefix: first.end..first.end,
        end: first.end,
    })
}

enum Outcome {
    Same,
    Deleted,
    Moved,
}

/// Applies `edit` to the reference's span on the edited axis. `collapse` keeps a deleted cell
/// reference pointing at the cell that took its place (a rule's anchor refs, see `rules`).
fn move_unit(unit: &mut Unit, edit: &AxisEdit, collapse: bool) -> Outcome {
    let a0 = unit.first.on(edit.axis);
    let a1 = unit.second.as_ref().map_or(a0, |s| s.on(edit.axis));
    let (Some((v0, _)), Some((v1, _))) = (a0, a1) else {
        return Outcome::Same;
    };
    let (lo, hi) = (v0.min(v1), v0.max(v1));
    let shifted = edit.shift_span(lo, hi).or_else(|| {
        (collapse && unit.second.is_none() && edit.kind == EditKind::Delete)
            .then_some((edit.at.min(edit.limit() - 1), edit.at.min(edit.limit() - 1)))
    });
    let Some((new_lo, new_hi)) = shifted else {
        return Outcome::Deleted;
    };
    if (new_lo, new_hi) == (lo, hi) {
        return Outcome::Same;
    }
    let (n0, n1) = if v0 <= v1 {
        (new_lo, new_hi)
    } else {
        (new_hi, new_lo)
    };
    unit.first.set(edit.axis, n0);
    if let Some(second) = unit.second.as_mut() {
        second.set(edit.axis, n1);
    }
    Outcome::Moved
}

/// One reference in formula text: where it starts, the sheet prefix in front of it (`Q1!`, with
/// the sheet's name) and the cell or range after the prefix when there is one.
pub(super) struct Site {
    pub(super) start: usize,
    pub(super) prefix_len: usize,
    pub(super) sheet: Option<String>,
    pub(super) unit: Option<Unit>,
}

impl Site {
    pub(super) fn unit_start(&self) -> usize {
        self.start + self.prefix_len
    }

    /// Where the site's text ends: after the cell or range, or after the prefix alone.
    fn end(&self) -> usize {
        self.unit.as_ref().map_or(self.unit_start(), Unit::end)
    }
}

/// `text` with the sites `replace` answers for rewritten: it gets each reference (outside string
/// literals) and returns the text that takes the place of `text[start..end]`, or `None` to keep it.
/// `None` when nothing changed.
pub(super) fn scan_refs(
    text: &str,
    mut replace: impl FnMut(&Site) -> Option<String>,
) -> Option<String> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut out: Option<String> = None;
    let mut copied = 0usize;
    let mut i = 0usize;
    while i < len {
        let b = bytes[i];
        if b == b'"' {
            // A string literal; a doubled quote stays inside it.
            i += 1;
            while i < len {
                if bytes[i] == b'"' {
                    if bytes.get(i + 1) == Some(&b'"') {
                        i += 2;
                        continue;
                    }
                    break;
                }
                i += 1;
            }
            i += 1;
            continue;
        }
        let prev_ident = i > 0 && (is_ident_byte(bytes[i - 1]) || bytes[i - 1] == b'.');
        let starts_token = b == b'\'' || b == b'$' || is_ident_byte(b) || !b.is_ascii();
        if prev_ident || !starts_token {
            i += text[i..].chars().next().map_or(1, char::len_utf8);
            continue;
        }
        let (prefix_len, sheet) = match prefix_at(text, i) {
            Some((len, name)) => (len, Some(name)),
            None => (0, None),
        };
        let unit = parse_unit(text, i + prefix_len, sheet.as_deref());
        if unit.is_none() && prefix_len == 0 {
            // Not a reference: skip one character.
            i += text[i..].chars().next().map_or(1, char::len_utf8);
            continue;
        }
        let site = Site {
            start: i,
            prefix_len,
            sheet,
            unit,
        };
        let end = site.end();
        if let Some(new) = replace(&site) {
            let buf = out.get_or_insert_with(|| String::with_capacity(len + 8));
            buf.push_str(&text[copied..i]);
            buf.push_str(&new);
            copied = end;
        }
        i = end;
    }
    out.map(|mut buf| {
        buf.push_str(&text[copied..]);
        buf
    })
}

/// `text` with every reference to cells of `target_sheet` moved for `edit`, or `None` when
/// nothing changes. `own_sheet` is the sheet that holds the text (a reference without a sheet
/// name points there).
pub fn rewrite_formula_refs(
    text: &str,
    own_sheet: &str,
    target_sheet: &str,
    edit: &AxisEdit,
    collapse: bool,
) -> Option<String> {
    scan_refs(text, |site| {
        let mut unit = site.unit.clone()?;
        let sheet = site.sheet.as_deref().unwrap_or(own_sheet);
        if !sheet.eq_ignore_ascii_case(target_sheet) {
            return None;
        }
        match move_unit(&mut unit, edit, collapse) {
            Outcome::Same => None,
            Outcome::Deleted => Some("#REF!".to_string()),
            Outcome::Moved => {
                let mut buf = String::new();
                buf.push_str(&text[site.start..site.unit_start()]);
                unit.first.write(&mut buf);
                if let Some(second) = unit.second {
                    buf.push(':');
                    buf.push_str(&text[unit.tail_prefix.clone()]);
                    second.write(&mut buf);
                }
                Some(buf)
            }
        }
    })
}

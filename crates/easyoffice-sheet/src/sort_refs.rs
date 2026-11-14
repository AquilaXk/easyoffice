//! Reference rewriting for `SheetEngine::sort_rows`.
//!
//! A formula that moves from one sorted row to another follows the usual spreadsheet sort rule:
//! a relative row reference that points into the sorted rows moves with the formula, a reference
//! that points outside them (or uses `$`, or names another sheet) stays as written, and a moved
//! reference that would leave the grid becomes `#REF!`. Formulas outside the sorted rows are not
//! rewritten.

use crate::PackedCellCoordinate;

/// Longest column-letter run (exclusive upper bound) that can form an A1 column reference.
const MAX_COL_LETTERS_EXCLUSIVE: usize = 4;

/// The sorted rows (0-indexed, inclusive) and the move of one formula inside them.
#[derive(Clone, Copy)]
pub(crate) struct SortedRows {
    first: u32,
    last: u32,
    delta: i64,
}

impl SortedRows {
    pub(crate) fn new(first: u32, last: u32, orig_row: u32, target_row: u32) -> Self {
        Self {
            first,
            last,
            delta: i64::from(target_row) - i64::from(orig_row),
        }
    }

    fn follows(&self, r: &CellRef) -> bool {
        !r.row_dollar && (self.first..=self.last).contains(&(r.row - 1))
    }

    /// New 1-based row of a following reference, or `None` when it leaves the grid.
    fn moved(&self, r: &CellRef) -> Option<u32> {
        let row = i64::from(r.row) + self.delta;
        (1..=i64::from(PackedCellCoordinate::MAX_ROWS))
            .contains(&row)
            .then_some(row as u32)
    }
}

#[inline]
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Consumes an optional `$` at `cur`; returns the new index and whether one was present.
#[inline]
fn take_dollar(bytes: &[u8], cur: usize) -> (usize, bool) {
    if bytes.get(cur) == Some(&b'$') {
        (cur + 1, true)
    } else {
        (cur, false)
    }
}

/// Length of an unquoted sheet-name prefix (`Q1!`, `Sheet2!`) starting at `i`, `!` included, so the
/// name's digits are never read as a row. Non-ASCII bytes belong to the name.
fn sheet_prefix_len(bytes: &[u8], i: usize) -> Option<usize> {
    let run = bytes[i..]
        .iter()
        .take_while(|&&b| is_ident_byte(b) || b == b'.' || !b.is_ascii())
        .count();
    (run > 0 && bytes.get(i + run) == Some(&b'!')).then_some(run + 1)
}

/// One A1-style cell reference in the formula text.
struct CellRef {
    /// Column part without `$`.
    col: std::ops::Range<usize>,
    col_dollar: bool,
    row_dollar: bool,
    /// 1-based row as written.
    row: u32,
    end: usize,
}

/// Parses an A1-style reference starting at `i`; `None` when `i` does not start one.
fn parse_cell_ref(raw: &str, i: usize) -> Option<CellRef> {
    let bytes = raw.as_bytes();
    let len = bytes.len();
    let (col_start, col_dollar) = take_dollar(bytes, i);
    let mut cur = col_start;
    while cur < len
        && bytes[cur].is_ascii_alphabetic()
        && (cur - col_start) < MAX_COL_LETTERS_EXCLUSIVE
    {
        cur += 1;
    }
    let col = col_start..cur;
    if !(1..=3).contains(&col.len()) {
        return None;
    }
    let (row_start, row_dollar) = take_dollar(bytes, cur);
    if !bytes
        .get(row_start)
        .is_some_and(|b| b.is_ascii_digit() && *b != b'0')
    {
        return None;
    }
    cur = row_start;
    while cur < len && bytes[cur].is_ascii_digit() {
        cur += 1;
    }
    if bytes
        .get(cur)
        .is_some_and(|&b| is_ident_byte(b) || b == b'(')
    {
        return None;
    }
    let row = raw[row_start..cur].parse().ok()?;
    Some(CellRef {
        col,
        col_dollar,
        row_dollar,
        row,
        end: cur,
    })
}

fn push_cell_ref(raw: &str, r: &CellRef, row: u32, out: &mut String) {
    if r.col_dollar {
        out.push('$');
    }
    out.push_str(&raw[r.col.clone()]);
    if r.row_dollar {
        out.push('$');
    }
    out.push_str(&row.to_string());
}

/// The second end of an `A1:B2` range whose first end is `first`.
fn range_tail(raw: &str, first: &CellRef) -> Option<CellRef> {
    (raw.as_bytes().get(first.end) == Some(&b':'))
        .then(|| parse_cell_ref(raw, first.end + 1))
        .flatten()
}

/// Rewrites the reference or `A1:B2` range starting at `i` into `out`; returns the index after it.
/// A range follows the formula only when both of its ends do.
fn rewrite_ref_unit(
    raw: &str,
    first: &CellRef,
    i: usize,
    rows: SortedRows,
    out: &mut String,
) -> usize {
    let tail = range_tail(raw, first);
    let end = tail.as_ref().map_or(first.end, |t| t.end);
    if !rows.follows(first) || tail.as_ref().is_some_and(|t| !rows.follows(t)) {
        out.push_str(&raw[i..end]);
        return end;
    }
    match (rows.moved(first), tail.as_ref().map(|t| (t, rows.moved(t)))) {
        (Some(row), None) => push_cell_ref(raw, first, row, out),
        (Some(row), Some((t, Some(tail_row)))) => {
            push_cell_ref(raw, first, row, out);
            out.push(':');
            push_cell_ref(raw, t, tail_row, out);
        }
        _ => out.push_str("#REF!"),
    }
    end
}

/// Rewrites the A1 references of the formula `raw` (in the sheet named `own_sheet`) for a move
/// described by `rows`. Text that is not a formula comes back unchanged.
pub(crate) fn adjust_formula_row_refs(raw: &str, own_sheet: &str, rows: SortedRows) -> String {
    if rows.delta == 0 || !raw.trim_start().starts_with('=') {
        return raw.to_string();
    }
    let bytes = raw.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len + 8);
    let mut i = 0usize;
    let mut in_double_quotes = false;
    let mut in_single_quotes = false;
    let mut foreign = false;
    let mut quote_start = 0usize;

    while i < len {
        let b = bytes[i];
        if b == b'"' && !in_single_quotes {
            in_double_quotes = !in_double_quotes;
        } else if b == b'\'' && !in_double_quotes {
            in_single_quotes = !in_single_quotes;
            if !in_single_quotes && bytes.get(i + 1) == Some(&b'!') {
                foreign = !raw[quote_start..i].eq_ignore_ascii_case(own_sheet);
                out.push_str("'!");
                i += 2;
                continue;
            }
            quote_start = i + 1;
        } else if !in_double_quotes && !in_single_quotes {
            if b == b'[' {
                // A structured reference names columns, not cells (PD-225): it stays as written.
                if let Some(end) = easyoffice_formats::group_end(raw, i) {
                    out.push_str(&raw[i..end]);
                    i = end;
                    continue;
                }
            }
            let prev_is_ident = i > 0 && (is_ident_byte(bytes[i - 1]) || bytes[i - 1] == b'.');
            if !prev_is_ident {
                if let Some(prefix) = sheet_prefix_len(bytes, i) {
                    foreign = !raw[i..i + prefix - 1].eq_ignore_ascii_case(own_sheet);
                    out.push_str(&raw[i..i + prefix]);
                    i += prefix;
                    continue;
                }
                if let Some(first) = parse_cell_ref(raw, i) {
                    i = if foreign {
                        let end = range_tail(raw, &first).map_or(first.end, |t| t.end);
                        out.push_str(&raw[i..end]);
                        end
                    } else {
                        rewrite_ref_unit(raw, &first, i, rows, &mut out)
                    };
                    foreign = false;
                    continue;
                }
            }
        }
        let ch = raw[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

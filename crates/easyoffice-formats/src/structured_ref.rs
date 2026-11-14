//! Structured references to the tables of a sheet (PD-225): `Table1[Q1]`, `Table1[[#This Row],[Q1]]`,
//! `Table1[@Q1]`, `Table1[[#Headers],[#Data],[Q1]:[Q3]]`, `Table1[#All]` and the unqualified `[@Q1]` of a
//! formula inside a table. This module holds what the engine and the file writers share: the syntax of one
//! bracket group, the rows and columns a reference stands for in a table, the scan of formula text for the
//! references, and the rewrites that scan serves (the canonical file spelling, a plain range, a renamed
//! column).
//!
//! 참조 구현: IronCalc `base/src/expressions/lexer/structured_references.rs` (the grammar `table "["
//! arguments "]"`, the five `#` items, the escape `'` before `[`, `]`, `#`, `'` and `@`) and
//! `base/src/expressions/parser/mod.rs` lines 1189-1370 (the rows each item selects: data without an item,
//! `#Totals` of a table with no totals row is `#REF!`, `#This Row` is the formula's row), MIT OR
//! Apache-2.0, commit 01b39c1ad26252f7b6c810440e8433a932daee07; Univer
//! `packages/engine-formula/src/engine/reference-object/table-reference-object.ts` (splitting the body into a
//! section and a column or column range at the first top-level comma, one layer of brackets around each
//! part, the escape set, column names compared as written), Apache-2.0, Copyright 2023-present DreamNum Co.,
//! Ltd., commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9.
//! Source and license text: `THIRD_PARTY_NOTICES.md`.
//!
//! Modifications: both references read leniently (a second `#` item, an unknown `#` word or a missing column
//! falls back to the data area or to column index -1) and the first one turns the reference into a range
//! while it parses, which loses the text. Here the syntax is strict (an item list must be one the
//! spreadsheet program allows, a column must exist), the text is kept and resolved when a formula is
//! compiled, the bracket nesting and the number of items are bounded, nothing recurses, and the same scan
//! rewrites the text for a file (`Table1[[#This Row],[Q1]]` for `[@Q1]`), for a format with no tables (a
//! range) and for a renamed column.

use crate::sheet_table::SheetTable;
use crate::CellBox;

/// Brackets one reference may nest (`Table1[[#This Row],[a]:[b]]` nests two).
pub const MAX_STRUCTURED_DEPTH: usize = 4;

pub const ITEM_DATA: u8 = 1;
pub const ITEM_HEADERS: u8 = 1 << 1;
pub const ITEM_TOTALS: u8 = 1 << 2;
pub const ITEM_ALL: u8 = 1 << 3;
pub const ITEM_THIS_ROW: u8 = 1 << 4;

const ITEM_NAMES: [(&str, u8); 5] = [
    ("#all", ITEM_ALL),
    ("#data", ITEM_DATA),
    ("#headers", ITEM_HEADERS),
    ("#totals", ITEM_TOTALS),
    ("#this row", ITEM_THIS_ROW),
];

/// The columns a reference names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Columns {
    /// Every column of the table.
    All,
    One(String),
    /// `[a]:[b]`, both ends as written.
    Span(String, String),
}

/// One bracket group of a structured reference, without the table's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuredRef {
    /// The `ITEM_*` bits given; none means the data rows.
    pub items: u8,
    pub columns: Columns,
    /// Written with the `@` of the spreadsheet program's screen (`[@Q1]`), a spelling of `#This Row`.
    pub at: bool,
}

/// Why a reference stands for no cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefError {
    /// The item or a column is not there (`#REF!`).
    Ref,
    /// `#This Row` outside the data rows (`#VALUE!`).
    Value,
}

#[inline]
fn is_escapable(b: u8) -> bool {
    matches!(b, b'[' | b']' | b'#' | b'\'' | b'@')
}

/// The index just past the `]` that closes the `[` at `open`, or `None` when it is never closed or the
/// brackets nest deeper than [`MAX_STRUCTURED_DEPTH`]. A `'` escapes the bracket, `#`, `'` or `@` after it.
pub fn group_end(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' if bytes.get(i + 1).copied().is_some_and(is_escapable) => {
                i += 2;
                continue;
            }
            b'[' => {
                depth += 1;
                if depth > MAX_STRUCTURED_DEPTH {
                    return None;
                }
            }
            b']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// A column name as written (`'` escapes) to the name itself.
fn unescape(raw: &str) -> String {
    if !raw.contains('\'') {
        return raw.to_string();
    }
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\''
            && chars
                .peek()
                .is_some_and(|&n| n.is_ascii() && is_escapable(n as u8))
        {
            out.extend(chars.next());
        } else {
            out.push(c);
        }
    }
    out
}

/// Whether `raw` holds a `[` or `]` no `'` escapes.
fn has_bare_bracket(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' if bytes.get(i + 1).copied().is_some_and(is_escapable) => i += 2,
            b'[' | b']' => return true,
            _ => i += 1,
        }
    }
    false
}

fn item_of(word: &str) -> Option<u8> {
    let folded = word.trim().to_ascii_lowercase();
    ITEM_NAMES
        .iter()
        .find_map(|&(name, bit)| (name == folded).then_some(bit))
}

/// Whether a set of items is one the spreadsheet program takes: a lone item, or the neighbours `#Headers`
/// with `#Data` and `#Data` with `#Totals` (all three are `#All`).
fn items_allowed(items: u8) -> bool {
    let rows = ITEM_HEADERS | ITEM_DATA | ITEM_TOTALS;
    match items {
        0 => true,
        _ if items & ITEM_ALL != 0 => items == ITEM_ALL,
        _ if items & ITEM_THIS_ROW != 0 => items == ITEM_THIS_ROW,
        _ => items & !rows == 0 && items != ITEM_HEADERS | ITEM_TOTALS,
    }
}

struct Cursor<'a> {
    text: &'a str,
    at: usize,
}

impl<'a> Cursor<'a> {
    fn skip_spaces(&mut self) {
        while self.text.as_bytes().get(self.at) == Some(&b' ') {
            self.at += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }

    fn done(&mut self) -> bool {
        self.skip_spaces();
        self.at >= self.text.len()
    }

    /// The text inside the bracket group at the cursor; the cursor moves past it.
    fn group(&mut self) -> Option<&'a str> {
        self.skip_spaces();
        if self.peek()? != b'[' {
            return None;
        }
        let end = group_end(self.text, self.at)?;
        let inner = &self.text[self.at + 1..end - 1];
        self.at = end;
        Some(inner)
    }
}

/// A column element: `[a]` or `[a]:[b]` at the cursor.
fn column_element(cur: &mut Cursor<'_>) -> Option<Columns> {
    let first = cur.group()?;
    if has_bare_bracket(first) {
        return None;
    }
    cur.skip_spaces();
    if cur.peek() != Some(b':') {
        return Some(Columns::One(unescape(first)));
    }
    cur.at += 1;
    let second = cur.group()?;
    if has_bare_bracket(second) {
        return None;
    }
    Some(Columns::Span(unescape(first), unescape(second)))
}

/// The elements after an item list: `[#Data],[#Totals],[a]:[b]`.
fn element_list(inner: &str) -> Option<StructuredRef> {
    let mut cur = Cursor { text: inner, at: 0 };
    let (mut items, mut columns) = (0u8, None);
    loop {
        cur.skip_spaces();
        let mark = cur.at;
        let content = cur.group()?;
        if content.trim_start().starts_with('#') {
            items |= item_of(content)?;
        } else {
            cur.at = mark;
            if columns.replace(column_element(&mut cur)?).is_some() {
                return None;
            }
        }
        if cur.done() {
            break;
        }
        if cur.peek() != Some(b',') {
            return None;
        }
        cur.at += 1;
    }
    items_allowed(items).then_some(StructuredRef {
        items,
        columns: columns.unwrap_or(Columns::All),
        at: false,
    })
}

/// The part after `@`: nothing, a bare name or `[a]`, `[a]:[b]`.
fn at_form(rest: &str) -> Option<StructuredRef> {
    let columns = if rest.trim().is_empty() {
        Columns::All
    } else if rest.trim_start().starts_with('[') {
        let mut cur = Cursor { text: rest, at: 0 };
        let columns = column_element(&mut cur)?;
        if !cur.done() {
            return None;
        }
        columns
    } else if has_bare_bracket(rest) {
        return None;
    } else {
        Columns::One(unescape(rest))
    };
    Some(StructuredRef {
        items: ITEM_THIS_ROW,
        columns,
        at: true,
    })
}

/// Parses the bracket group `group` (`[Q1]`, `[[#This Row],[Q1]]`, `[@Q1]`, ...); `None` when it is not
/// one the syntax allows.
pub fn parse_group(group: &str) -> Option<StructuredRef> {
    if group.len() < 2 || group_end(group, 0)? != group.len() {
        return None;
    }
    let inner = &group[1..group.len() - 1];
    let s = inner.trim_start();
    if s.is_empty() {
        return Some(StructuredRef {
            items: 0,
            columns: Columns::All,
            at: false,
        });
    }
    match s.as_bytes()[0] {
        b'@' => at_form(&s[1..]),
        b'#' => Some(StructuredRef {
            items: item_of(s)?,
            columns: Columns::All,
            at: false,
        }),
        b'[' => element_list(inner),
        _ if has_bare_bracket(inner) => None,
        _ => Some(StructuredRef {
            items: 0,
            columns: Columns::One(unescape(inner)),
            at: false,
        }),
    }
}

/// The rows (first, last) of `table` a set of items stands for. `row` is the formula's own row, which
/// `#This Row` needs.
fn rows_of(items: u8, table: &SheetTable, row: Option<u32>) -> Result<(u32, u32), RefError> {
    let top = table.range.row0;
    let bottom = table.range.row1;
    let first_data = top + u32::from(table.header);
    let last_data = bottom - u32::from(table.totals);
    let need = |ok: bool, span: (u32, u32)| if ok { Ok(span) } else { Err(RefError::Ref) };
    match items {
        0 | ITEM_DATA => Ok((first_data, last_data)),
        ITEM_HEADERS => need(table.header, (top, top)),
        ITEM_TOTALS => need(table.totals, (bottom, bottom)),
        ITEM_ALL => Ok((top, bottom)),
        ITEM_THIS_ROW => row
            .filter(|r| (first_data..=last_data).contains(r))
            .map(|r| (r, r))
            .ok_or(RefError::Value),
        x if x == ITEM_HEADERS | ITEM_DATA => need(table.header, (top, last_data)),
        x if x == ITEM_DATA | ITEM_TOTALS => need(table.totals, (first_data, bottom)),
        _ => need(table.header && table.totals, (top, bottom)),
    }
}

/// The cells of `table` a reference stands for. `column_of` finds a column by its name and answers its
/// position in the table; `row` is the zero-based row of the formula (for `#This Row`).
pub fn resolve(
    sref: &StructuredRef,
    table: &SheetTable,
    column_of: impl Fn(&str) -> Option<u32>,
    row: Option<u32>,
) -> Result<CellBox, RefError> {
    let (row0, row1) = rows_of(sref.items, table, row)?;
    let col = |name: &str| column_of(name).ok_or(RefError::Ref);
    let (first, last) = match &sref.columns {
        Columns::All => (0, table.width() - 1),
        Columns::One(a) => {
            let c = col(a)?;
            (c, c)
        }
        Columns::Span(a, b) => {
            let (x, y) = (col(a)?, col(b)?);
            (x.min(y), x.max(y))
        }
    };
    Ok(CellBox {
        row0,
        col0: table.range.col0 + first,
        row1,
        col1: table.range.col0 + last,
    })
}

/// Whether two names (columns, tables, sheets) are the same one: names compare without case.
pub fn same_name(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b) || (!a.is_ascii() && a.to_lowercase() == b.to_lowercase())
}

/// The position of the column `name` in `table`, by its list of names.
pub fn column_position(table: &SheetTable, name: &str) -> Option<u32> {
    table
        .columns
        .iter()
        .position(|c| same_name(c, name))
        .map(|i| i as u32)
}

// ---- writing ---------------------------------------------------------------------------------------

/// A column name with the `'` escapes the syntax needs.
fn escape_column(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 2);
    for (i, c) in name.chars().enumerate() {
        if matches!(c, '[' | ']' | '#' | '\'') || (i == 0 && c == '@') {
            out.push('\'');
        }
        out.push(c);
    }
    out
}

/// Whether a column name must sit in brackets of its own (`[[Q1 $]]`).
fn needs_inner_brackets(name: &str) -> bool {
    name.chars().any(|c| {
        c.is_control() || (c.is_ascii() && !(c.is_ascii_alphanumeric() || c == '_' || c == ' '))
    })
}

fn write_columns(out: &mut String, columns: &Columns, force_brackets: bool) {
    let one = |out: &mut String, name: &str, inner: bool| {
        if inner {
            out.push('[');
        }
        out.push_str(&escape_column(name));
        if inner {
            out.push(']');
        }
    };
    match columns {
        Columns::All => {}
        Columns::One(a) => one(out, a, force_brackets || needs_inner_brackets(a)),
        Columns::Span(a, b) => {
            one(out, a, true);
            out.push(':');
            one(out, b, true);
        }
    }
}

/// The group of `sref` in the spelling the files use (`[Q1]`, `[[#This Row],[Q1]]`, `[#All]`,
/// `[[#Headers],[#Data],[Q1]:[Q2]]`); the `@` spelling is kept when `sref.at` says it was written.
pub fn write_group(sref: &StructuredRef) -> String {
    let mut out = String::from("[");
    if sref.at {
        out.push('@');
        if let Columns::One(a) = &sref.columns {
            if needs_inner_brackets(a) {
                out.push('[');
                out.push_str(&escape_column(a));
                out.push(']');
            } else {
                out.push_str(&escape_column(a));
            }
        } else {
            write_columns(&mut out, &sref.columns, true);
        }
        out.push(']');
        return out;
    }
    let listed: Vec<&str> = [
        (ITEM_HEADERS, "#Headers"),
        (ITEM_DATA, "#Data"),
        (ITEM_TOTALS, "#Totals"),
        (ITEM_ALL, "#All"),
        (ITEM_THIS_ROW, "#This Row"),
    ]
    .iter()
    .filter_map(|&(bit, name)| (sref.items & bit != 0).then_some(name))
    .collect();
    match (listed.as_slice(), &sref.columns) {
        ([], Columns::All) => {}
        ([], columns) => write_columns(&mut out, columns, false),
        ([item], Columns::All) => out.push_str(item),
        (items, columns) => {
            for item in items {
                out.push('[');
                out.push_str(item);
                out.push_str("],");
            }
            if matches!(columns, Columns::All) {
                out.pop();
            } else {
                write_columns(&mut out, columns, true);
            }
        }
    }
    out.push(']');
    out
}

fn quote_sheet(name: &str) -> String {
    let plain = name
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    let letters = name.bytes().take_while(u8::is_ascii_alphabetic).count();
    let cell_like = (1..=3).contains(&letters)
        && name.len() > letters
        && name.bytes().skip(letters).all(|b| b.is_ascii_digit());
    if plain
        && !cell_like
        && !name.eq_ignore_ascii_case("TRUE")
        && !name.eq_ignore_ascii_case("FALSE")
    {
        name.to_string()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

fn col_letters(col: u32) -> String {
    let mut n = col + 1;
    let mut out = Vec::new();
    while n > 0 {
        out.push(b'A' + ((n - 1) % 26) as u8);
        n = (n - 1) / 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// The A1 text of `cells`, on `sheet` when that is not the formula's own sheet. `relative_row` leaves the
/// row without `$` (a `#This Row` reference follows a fill).
fn a1_text(cells: CellBox, sheet: Option<&str>, relative_row: bool) -> String {
    let row_mark = if relative_row { "" } else { "$" };
    let end = |col: u32, row: u32| format!("${}{row_mark}{}", col_letters(col), row + 1);
    let mut out = sheet.map_or_else(String::new, |s| format!("{}!", quote_sheet(s)));
    out.push_str(&end(cells.col0, cells.row0));
    if (cells.row0, cells.col0) != (cells.row1, cells.col1) {
        out.push(':');
        out.push_str(&end(cells.col1, cells.row1));
    }
    out
}

// ---- scanning formula text -------------------------------------------------------------------------

/// A structured reference in formula text.
pub struct Found<'a> {
    /// The reference's text, table name included.
    pub start: usize,
    pub end: usize,
    /// The table's name; `None` for a reference inside a table that does not name it (`[@Q1]`).
    pub table: Option<&'a str>,
    /// `None` when the group breaks the syntax.
    pub sref: Option<StructuredRef>,
}

#[inline]
fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'\\') || !b.is_ascii()
}

/// The end of the quoted run (string or sheet name) opening at `i`.
fn quoted_end(bytes: &[u8], i: usize) -> usize {
    let quote = bytes[i];
    let mut j = i + 1;
    while j < bytes.len() {
        if bytes[j] == quote {
            if bytes.get(j + 1) == Some(&quote) {
                j += 2;
                continue;
            }
            return j + 1;
        }
        j += 1;
    }
    bytes.len()
}

/// Calls `replace` for each structured reference of `text` (outside quoted text) and returns the text with
/// the answers put in; `None` when nothing was replaced.
pub fn rewrite_refs(
    text: &str,
    mut replace: impl FnMut(&Found<'_>) -> Option<String>,
) -> Option<String> {
    let bytes = text.as_bytes();
    if !bytes.contains(&b'[') {
        return None;
    }
    let mut out: Option<String> = None;
    let mut copied = 0usize;
    let mut name_start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' || b == b'\'' {
            i = quoted_end(bytes, i);
            continue;
        }
        if b != b'[' {
            if !(i > 0 && is_name_byte(bytes[i - 1])) {
                name_start = i;
            }
            i += 1;
            continue;
        }
        let qualified = i > 0 && is_name_byte(bytes[i - 1]);
        let next = text[i + 1..]
            .trim_start_matches(' ')
            .as_bytes()
            .first()
            .copied();
        if !qualified && !matches!(next, Some(b'@' | b'#' | b'[')) {
            i += 1;
            continue;
        }
        let Some(end) = group_end(text, i) else {
            i += 1;
            continue;
        };
        let start = if qualified { name_start } else { i };
        let found = Found {
            start,
            end,
            table: qualified.then(|| &text[start..i]),
            sref: parse_group(&text[i..end]),
        };
        if let Some(new) = replace(&found) {
            let buf = out.get_or_insert_with(|| String::with_capacity(text.len() + 16));
            buf.push_str(&text[copied..start]);
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

/// Whether `text` may hold a structured reference (a cheap test before a scan).
pub fn may_hold_structured_ref(text: &str) -> bool {
    text.as_bytes().contains(&b'[')
}

/// `formula` (text without `=`) with the references written as the file format holds them: the table's name in
/// front of an unqualified reference (`own_table` is the table the formula's cell is in), `#This Row` for `@`
/// and the group in its one spelling (a column name with a special character in double brackets). A reference that names no table or breaks the syntax becomes `#REF!`.
pub fn to_file_refs(formula: &str, own_table: Option<&str>) -> Option<String> {
    rewrite_refs(formula, |found| {
        let Some(sref) = found.sref.as_ref() else {
            return Some("#REF!".to_string());
        };
        let Some(table) = found.table.or(own_table) else {
            return Some("#REF!".to_string());
        };
        let canonical = StructuredRef {
            at: false,
            ..sref.clone()
        };
        let written = format!("{table}{}", write_group(&canonical));
        (written != formula[found.start..found.end]).then_some(written)
    })
}

/// The tables a formula's references may name.
pub struct TableLookup<'a> {
    /// The table of the name given, with the name of the sheet it is on.
    pub find: &'a dyn Fn(&str) -> Option<(&'a str, &'a SheetTable)>,
    /// The table the formula's cell is in, with its sheet's name.
    pub own: Option<(&'a str, &'a SheetTable)>,
}

/// `formula` (text without `=`) with every structured reference written as the cells it stands for, for a
/// format that has no tables. `sheet` is the sheet of the formula and `row` its zero-based row. A reference
/// that stands for nothing becomes `#REF!` (or `#VALUE!` for `#This Row` outside the table).
pub fn to_range_refs(
    formula: &str,
    sheet: &str,
    row: u32,
    lookup: &TableLookup<'_>,
) -> Option<String> {
    rewrite_refs(formula, |found| {
        let resolved = found
            .sref
            .as_ref()
            .zip(found.table.map_or(lookup.own, |t| (lookup.find)(t)))
            .ok_or(RefError::Ref)
            .and_then(|(sref, (table_sheet, table))| {
                let same_sheet = same_name(table_sheet, sheet);
                let this_row = (same_sheet && sref.items == ITEM_THIS_ROW).then_some(row);
                let cells = resolve(sref, table, |n| column_position(table, n), this_row)?;
                let prefix = (!same_sheet).then_some(table_sheet);
                Ok(a1_text(cells, prefix, sref.items == ITEM_THIS_ROW))
            });
        Some(match resolved {
            Ok(text) => text,
            Err(RefError::Ref) => "#REF!".to_string(),
            Err(RefError::Value) => "#VALUE!".to_string(),
        })
    })
}

/// `formula` (text without `=`) with the column `from` of table `table` renamed `to`. `own_table` is the
/// table the formula's cell is in (for references that do not name one).
pub fn rename_column_refs(
    formula: &str,
    own_table: Option<&str>,
    table: &str,
    from: &str,
    to: &str,
) -> Option<String> {
    let rename = |name: &String| {
        if same_name(name, from) {
            to.to_string()
        } else {
            name.clone()
        }
    };
    rewrite_refs(formula, |found| {
        let sref = found.sref.as_ref()?;
        let named = found.table.or(own_table)?;
        if !same_name(named, table) {
            return None;
        }
        let columns = match &sref.columns {
            Columns::All => return None,
            Columns::One(a) => Columns::One(rename(a)),
            Columns::Span(a, b) => Columns::Span(rename(a), rename(b)),
        };
        if columns == sref.columns {
            return None;
        }
        let renamed = StructuredRef {
            columns,
            ..sref.clone()
        };
        Some(format!(
            "{}{}",
            found.table.unwrap_or_default(),
            write_group(&renamed)
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(group: &str) -> StructuredRef {
        parse_group(group).unwrap_or_else(|| panic!("{group} should parse"))
    }

    fn one(name: &str) -> Columns {
        Columns::One(name.to_string())
    }

    fn table() -> SheetTable {
        // B3:E10: header row 3, data rows 4-9, totals row 10; columns a b c d.
        let mut t = SheetTable::new("T", CellBox::parse("B3:E10").unwrap());
        t.totals = true;
        t.columns = ["a", "b c", "d$", "e"].map(String::from).to_vec();
        t
    }

    fn cells(group: &str, row: Option<u32>) -> Result<String, RefError> {
        let t = table();
        resolve(&p(group), &t, |n| column_position(&t, n), row).map(|b| b.to_a1())
    }

    #[test]
    fn groups_parse() {
        assert_eq!(p("[a]").columns, one("a"));
        assert_eq!(p("[Q 1]").columns, one("Q 1"));
        assert_eq!(p("[[a]]").columns, one("a"));
        assert_eq!(
            p("[[a]:[b]]").columns,
            Columns::Span("a".into(), "b".into())
        );
        assert_eq!(p("[#All]").items, ITEM_ALL);
        assert_eq!(p("[#this row]").items, ITEM_THIS_ROW);
        assert_eq!(p("[]").columns, Columns::All);
        let r = p("[[#This Row],[a]]");
        assert_eq!((r.items, r.columns, r.at), (ITEM_THIS_ROW, one("a"), false));
        let r = p("[@a]");
        assert_eq!((r.items, r.columns, r.at), (ITEM_THIS_ROW, one("a"), true));
        assert_eq!(p("[@[a b]]").columns, one("a b"));
        assert_eq!(
            p("[@[a]:[b]]").columns,
            Columns::Span("a".into(), "b".into())
        );
        assert_eq!(p("[@]").columns, Columns::All);
        let r = p("[[#Headers], [#Data], [a]:[b]]");
        assert_eq!(r.items, ITEM_HEADERS | ITEM_DATA);
        assert_eq!(p("[[#Totals],[a]]").items, ITEM_TOTALS);
    }

    #[test]
    fn escapes_read_and_write() {
        assert_eq!(p("[it''s]").columns, one("it's"));
        assert_eq!(p("[[a'[1']]]").columns, one("a[1]"));
        assert_eq!(p("[a'#]").columns, one("a#"));
        assert_eq!(p("[@['@x]]").columns, one("@x"));
        let r = StructuredRef {
            items: 0,
            columns: one("a[1]#'"),
            at: false,
        };
        assert_eq!(write_group(&r), "[[a'[1']'#'']]");
        assert_eq!(parse_group(&write_group(&r)), Some(r));
    }

    #[test]
    fn bad_groups_are_refused() {
        for bad in [
            "[#Nope]",
            "[[#All],[#Data]]",
            "[[#This Row],[#Data]]",
            "[[#Headers],[#Totals]]",
            "[[a],[b]]",
            "[[a]:[b]:[c]]",
            "[a",
            "a]",
            "[[a]",
            "[a[b]]",
            "[@[a]x]",
            "[[#Data] [a]]",
            "[[#Data],]",
            "[#Data,[a]]",
            "[[[[[a]]]]]",
        ] {
            assert!(parse_group(bad).is_none(), "{bad}");
        }
        assert_eq!(group_end("[[[[[a]]]]]", 0), None);
    }

    #[test]
    fn rows_and_columns_of_a_reference() {
        assert_eq!(cells("[a]", None).unwrap(), "B4:B9");
        assert_eq!(cells("[[a]:[d$]]", None).unwrap(), "B4:D9");
        assert_eq!(cells("[[d$]:[a]]", None).unwrap(), "B4:D9");
        assert_eq!(cells("[#All]", None).unwrap(), "B3:E10");
        assert_eq!(cells("[#Headers]", None).unwrap(), "B3:E3");
        assert_eq!(cells("[[#Totals],[e]]", None).unwrap(), "E10");
        assert_eq!(cells("[[#Headers],[#Data],[a]]", None).unwrap(), "B3:B9");
        assert_eq!(cells("[[#Data],[#Totals],[a]]", None).unwrap(), "B4:B10");
        assert_eq!(
            cells("[[#Totals],[#Headers],[#Data],[a]]", None).unwrap(),
            "B3:B10"
        );
        assert_eq!(cells("[@[b c]]", Some(5)).unwrap(), "C6");
        assert_eq!(cells("[@]", Some(3)).unwrap(), "B4:E4");
        assert_eq!(cells("[@a]", Some(9)), Err(RefError::Value));
        assert_eq!(cells("[@a]", Some(2)), Err(RefError::Value));
        assert_eq!(cells("[@a]", None), Err(RefError::Value));
        assert_eq!(cells("[nope]", None), Err(RefError::Ref));
        assert_eq!(cells("[[a]:[nope]]", None), Err(RefError::Ref));
        let mut t = table();
        t.totals = false;
        t.range.row1 = 8;
        let at = |t: &SheetTable, g: &str| resolve(&p(g), t, |n| column_position(t, n), None);
        assert_eq!(at(&t, "[#Totals]"), Err(RefError::Ref));
        assert_eq!(at(&t, "[[#Data],[#Totals]]"), Err(RefError::Ref));
        assert_eq!(at(&t, "[#All]").unwrap().to_a1(), "B3:E9");
        t.header = false;
        assert_eq!(at(&t, "[#Headers]"), Err(RefError::Ref));
        assert_eq!(at(&t, "[a]").unwrap().to_a1(), "B3:B9");
    }

    #[test]
    fn groups_write_in_the_file_spelling() {
        for (group, written) in [
            ("[a]", "[a]"),
            ("[Q 1]", "[Q 1]"),
            ("[d$]", "[[d$]]"),
            ("[[#this row],[a]]", "[[#This Row],[a]]"),
            ("[[#All]]", "[#All]"),
            ("[[a]:[b]]", "[[a]:[b]]"),
            (
                "[[#Headers],[#Data],[a]:[b]]",
                "[[#Headers],[#Data],[a]:[b]]",
            ),
            ("[@a]", "[@a]"),
            ("[@[d$]]", "[@[d$]]"),
            ("[@[a]:[b]]", "[@[a]:[b]]"),
            ("[]", "[]"),
        ] {
            assert_eq!(write_group(&p(group)), written, "{group}");
        }
    }

    #[test]
    fn the_scan_finds_references_outside_quotes() {
        let seen = std::cell::RefCell::new(Vec::new());
        let out = rewrite_refs(
            r#"SUM(T[a])+"T[b]"+'T[c]'!A1+[@a]+Other[[#This Row],[x]]+LAMBDA(q,[r])+F(x)[1]"#,
            |f| {
                seen.borrow_mut()
                    .push((f.table.map(str::to_string), f.sref.is_some()));
                None
            },
        );
        assert!(out.is_none());
        assert_eq!(
            seen.into_inner(),
            [
                (Some("T".into()), true),
                (None, true),
                (Some("Other".into()), true),
            ]
        );
    }

    #[test]
    fn file_spelling_names_the_table_and_this_row() {
        assert_eq!(
            to_file_refs("[@a]*2+T[a]", Some("T")).as_deref(),
            Some("T[[#This Row],[a]]*2+T[a]")
        );
        assert_eq!(
            to_file_refs("T[@[b c]]", None).as_deref(),
            Some("T[[#This Row],[b c]]")
        );
        assert_eq!(to_file_refs("[@a]", None).as_deref(), Some("#REF!"));
        assert_eq!(to_file_refs("T[[#This Row],[a]]", Some("T")), None);
        // A column name with a special character is written in double brackets, as the documentation says.
        assert_eq!(
            to_file_refs("SUM(T[d$])", None).as_deref(),
            Some("SUM(T[[d$]])")
        );
        assert_eq!(to_file_refs("T[[d$]]+T[ b c ]", None), None);
        assert_eq!(to_file_refs("T[bad", Some("T")), None);
        assert_eq!(
            to_file_refs("T[[a],[b]]", Some("T")).as_deref(),
            Some("#REF!")
        );
    }

    #[test]
    fn ranges_for_formats_without_tables() {
        let t = table();
        let find = |name: &str| same_name(name, "t").then_some(("Sheet 1", &t));
        let lookup = TableLookup {
            find: &find,
            own: Some(("Sheet 1", &t)),
        };
        let go = |f: &str, sheet: &str, row: u32| to_range_refs(f, sheet, row, &lookup).unwrap();
        assert_eq!(go("SUM(T[a])", "Sheet 1", 0), "SUM($B$4:$B$9)");
        assert_eq!(go("SUM(t[a])", "Other", 0), "SUM('Sheet 1'!$B$4:$B$9)");
        assert_eq!(go("T[[#This Row],[a]]*2", "Sheet 1", 5), "$B6*2");
        assert_eq!(go("[@a]", "Sheet 1", 5), "$B6");
        assert_eq!(go("T[[#This Row],[a]]", "Other", 5), "#VALUE!");
        assert_eq!(go("T[x]+Nope[a]", "Sheet 1", 0), "#REF!+#REF!");
        assert_eq!(go("T[#All]", "Sheet 1", 0), "$B$3:$E$10");
    }

    #[test]
    fn a_renamed_column_is_written_again() {
        assert_eq!(
            rename_column_refs(
                "SUM(T[a])+T[[#This Row],[A]]+[@a]",
                Some("t"),
                "T",
                "a",
                "z$"
            )
            .as_deref(),
            Some("SUM(T[[z$]])+T[[#This Row],[z$]]+[@[z$]]")
        );
        assert_eq!(rename_column_refs("U[a]+T[b c]", None, "T", "a", "z"), None);
        assert_eq!(
            rename_column_refs("T[[a]:[b c]]", None, "T", "b c", "q").as_deref(),
            Some("T[[a]:[q]]")
        );
        assert_eq!(rename_column_refs("T[#All]", None, "T", "a", "z"), None);
    }

    #[test]
    fn hostile_text_stays_linear() {
        let bomb = format!("T[{}]", "'['".repeat(4000));
        let _ = parse_group(&bomb[1..]);
        let deep = format!("T{}a{}", "[".repeat(5000), "]".repeat(5000));
        assert!(rewrite_refs(&deep, |_| None).is_none());
        let many = "T[a]+".repeat(2000);
        let out = rewrite_refs(&many, |_| Some("X".into())).unwrap();
        assert_eq!(out.len(), 2000 * 2);
    }
}

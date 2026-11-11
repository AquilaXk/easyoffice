//! One-pass SpreadsheetML reader: the shared string table and the cells, formulas, cell style
//! indexes and column widths of a worksheet, read from a single event stream each.
//!
//! 참조 구현 (reference implementation): calamine 0.38.0 (MIT, Copyright (c) 2016 Johann Tuffe),
//! https://github.com/tafia/calamine, `src/xlsx/mod.rs` (`read_shared_strings` at line 350, the
//! `worksheet_cells_reader` cell loop). Like there, one pull-reader loop keeps a small state per cell (position,
//! type, formula, value) and the shared strings sit in one table that is built once. Here the table
//! is a single arena string with end offsets, so a cell costs one allocation (its own text) and the
//! table costs two. The cell rules are those of the earlier substring scanner, so results stay
//! identical. Safe Rust only, no dependency (PD-053, PD-095).

use std::borrow::Cow;
use std::collections::HashMap;

use crate::rtf_xml_helpers::{format_col_letters, parse_a1_zero_based};
use crate::sheet_styling::xlsx_styles::xlsx_width_to_px;
use crate::sheet_styling::{CellStyle, SheetStyling, MAX_STYLED_CELLS};
use crate::xml_stream::{unescape_cow, Attrs, XmlEvent, XmlReader};
use crate::{EasyOfficeError, OoxmlCodec};

/// Attribute `name`, also written with the `eo:` prefix.
fn plain_name(name: &str) -> &str {
    name.strip_prefix("eo:").unwrap_or(name)
}

/// The shared string table of `xl/sharedStrings.xml`: all strings in one arena.
#[derive(Default)]
pub(crate) struct SharedStrings {
    text: String,
    ends: Vec<usize>,
}

impl SharedStrings {
    pub(crate) fn get(&self, index: usize) -> Option<&str> {
        let end = *self.ends.get(index)?;
        let start = index.checked_sub(1).map_or(0, |prev| self.ends[prev]);
        Some(&self.text[start..end])
    }

    fn close_item(&mut self, start: usize) {
        if self.text[start..].contains("_x") {
            let escaped = self.text.split_off(start);
            decode_xlsx_escapes_into(&escaped, &mut self.text);
        }
        self.ends.push(self.text.len());
    }

    #[cfg(test)]
    pub(crate) fn from_strings(strings: &[String]) -> Self {
        let mut table = Self::default();
        for s in strings {
            table.text.push_str(s);
            table.ends.push(table.text.len());
        }
        table
    }
}

/// Reads every `si` item: the text of its `t` elements outside `rPh` phonetic runs.
pub(crate) fn read_shared_strings(xml: &str) -> Result<SharedStrings, EasyOfficeError> {
    let mut table = SharedStrings::default();
    let mut reader = XmlReader::unbounded(xml);
    let mut item_start: Option<usize> = None;
    let mut phonetic_depth = 0usize;
    let mut in_text = false;
    while let Some(event) = reader.next_event()? {
        match event {
            XmlEvent::Start { name: "si", .. } if reader.last_start_was_empty() => {
                table.ends.push(table.text.len());
            }
            XmlEvent::Start { name: "si", .. } => {
                item_start = Some(table.text.len());
                phonetic_depth = 0;
            }
            XmlEvent::Start { name: "rPh", .. } if item_start.is_some() => phonetic_depth += 1,
            XmlEvent::End("rPh") => phonetic_depth = phonetic_depth.saturating_sub(1),
            XmlEvent::Start { name: "t", .. }
                if item_start.is_some()
                    && phonetic_depth == 0
                    && !reader.last_start_was_empty() =>
            {
                in_text = true;
            }
            XmlEvent::End("t") => in_text = false,
            XmlEvent::Text(text) if in_text => table.text.push_str(&text),
            XmlEvent::End("si") => {
                if let Some(start) = item_start.take() {
                    table.close_item(start);
                }
            }
            _ => {}
        }
    }
    Ok(table)
}

/// What one worksheet yields: its cells and, when asked for, their styling.
pub(crate) struct WorksheetScan {
    pub(crate) cells: Vec<(u32, u32, String)>,
    pub(crate) styling: SheetStyling,
}

/// Which part of a `c` element the text events belong to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Capture {
    None,
    Formula,
    Value,
    InlineText,
}

/// The formula element of a cell: attributes of the first `f` and its text.
struct Formula<'a> {
    shared: bool,
    index: Option<u32>,
    body: Cow<'a, str>,
}

/// Fields of the `c` element being read.
struct CellState<'a> {
    pos: (u32, u32),
    cell_type: Cow<'a, str>,
    formula: Option<Formula<'a>>,
    value: Option<Cow<'a, str>>,
    inline: Cow<'a, str>,
    has_inline: bool,
    phonetic_depth: usize,
    capture: Capture,
}

fn with_equals(formula: &str) -> String {
    let mut text = String::with_capacity(formula.len() + 1);
    text.push('=');
    text.push_str(formula);
    text
}

fn append<'a>(acc: &mut Cow<'a, str>, text: Cow<'a, str>) {
    if acc.is_empty() {
        *acc = text;
    } else {
        acc.to_mut().push_str(&text);
    }
}

/// The zero-based position of a cell from `row`/`col`, else `r`, else `(0, 0)`.
fn cell_position(
    row: Option<&str>,
    col: Option<&str>,
    a1: Option<&str>,
) -> Result<(u32, u32), EasyOfficeError> {
    let numbers = row
        .and_then(|v| v.parse::<u32>().ok())
        .zip(col.and_then(|v| v.parse::<u32>().ok()));
    if let Some((r, c)) = numbers {
        if r >= OoxmlCodec::MAX_XLSX_ROWS || c >= OoxmlCodec::MAX_XLSX_COLS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "XLSX 셀 좌표가 ECMA-376 허용 범위를 초과했습니다.".to_string(),
            ));
        }
        return Ok((r, c));
    }
    match a1 {
        Some(a1) => parse_a1_zero_based(a1).ok_or_else(|| {
            EasyOfficeError::ResourceBudgetExceeded(
                "XLSX A1 셀 참조가 ECMA-376 허용 범위를 초과했거나 잘못되었습니다.".to_string(),
            )
        }),
        None => Ok((0, 0)),
    }
}

struct SheetScanner<'s> {
    shared: &'s SharedStrings,
    xf_styles: &'s [CellStyle],
    want_styling: bool,
    cells: Vec<(u32, u32, String)>,
    styling: SheetStyling,
    /// First master formula text of each shared formula index with the cell it sits in.
    masters: HashMap<u32, (u32, u32, String)>,
}

impl<'s> SheetScanner<'s> {
    /// Opens a `c` element; only a cell with content is read, styling also covers empty ones.
    fn open_cell<'a>(
        &mut self,
        attrs: &Attrs<'a>,
        empty: bool,
    ) -> Result<Option<CellState<'a>>, EasyOfficeError> {
        let (mut a1, mut row, mut col, mut cell_type, mut style) = (None, None, None, None, None);
        for (name, value) in attrs.iter_raw() {
            let slot = match plain_name(name) {
                "r" => &mut a1,
                "row" => &mut row,
                "col" => &mut col,
                "t" => &mut cell_type,
                "s" => &mut style,
                _ => continue,
            };
            slot.get_or_insert_with(|| unescape_cow(value));
        }
        if self.want_styling && !self.xf_styles.is_empty() {
            self.record_style(style.as_deref(), a1.as_deref());
        }
        if empty {
            return Ok(None);
        }
        let pos = cell_position(row.as_deref(), col.as_deref(), a1.as_deref())?;
        Ok(Some(CellState {
            pos,
            cell_type: cell_type.unwrap_or_default(),
            formula: None,
            value: None,
            inline: Cow::Borrowed(""),
            has_inline: false,
            phonetic_depth: 0,
            capture: Capture::None,
        }))
    }

    fn record_style(&mut self, style: Option<&str>, a1: Option<&str>) {
        if self.styling.cells.len() >= MAX_STYLED_CELLS {
            return;
        }
        let Some(style) = style
            .and_then(|v| v.parse::<usize>().ok())
            .and_then(|i| self.xf_styles.get(i))
            .filter(|s| !s.is_plain())
        else {
            return;
        };
        if let Some(pos) = a1.and_then(parse_a1_zero_based) {
            self.styling.cells.insert(pos, *style);
        }
    }

    /// A `col` element with a custom width becomes a column width in CSS pixels.
    fn record_column(&mut self, attrs: &Attrs<'_>) {
        let (mut custom, mut min, mut max, mut width) = (None, None, None, None);
        for (name, value) in attrs.iter_raw() {
            let slot = match plain_name(name) {
                "customWidth" => &mut custom,
                "min" => &mut min,
                "max" => &mut max,
                "width" => &mut width,
                _ => continue,
            };
            slot.get_or_insert_with(|| unescape_cow(value));
        }
        if matches!(custom.as_deref(), None | Some("0" | "false")) {
            return;
        }
        let parse = |v: Option<Cow<'_, str>>| v?.parse::<u32>().ok();
        let (Some(min), Some(max)) = (parse(min), parse(max)) else {
            return;
        };
        let Some(width) = width.and_then(|w| w.parse::<f64>().ok()) else {
            return;
        };
        if !width.is_finite() || width <= 0.0 || min == 0 || max < min {
            return;
        }
        let px = xlsx_width_to_px(width);
        for col in min..=max.min(OoxmlCodec::MAX_XLSX_COLS) {
            if self.styling.columns.len() >= MAX_STYLED_CELLS {
                break;
            }
            self.styling.columns.insert(col - 1, px);
        }
    }

    /// Feeds an event inside a `c` element.
    fn feed<'a>(&self, cell: &mut CellState<'a>, event: XmlEvent<'a>, empty_start: bool) {
        match event {
            XmlEvent::Start { name: "f", attrs } if cell.formula.is_none() => {
                let (mut kind, mut index) = (None, None);
                for (name, value) in attrs.iter_raw() {
                    match plain_name(name) {
                        "t" => kind.get_or_insert_with(|| unescape_cow(value)),
                        "si" => index.get_or_insert_with(|| unescape_cow(value)),
                        _ => continue,
                    };
                }
                cell.formula = Some(Formula {
                    shared: kind.as_deref() == Some("shared"),
                    index: index.and_then(|s| s.parse::<u32>().ok()),
                    body: Cow::Borrowed(""),
                });
                cell.capture = Capture::Formula;
            }
            XmlEvent::Start { name: "v", .. } if cell.value.is_none() && !empty_start => {
                cell.value = Some(Cow::Borrowed(""));
                cell.capture = Capture::Value;
            }
            XmlEvent::Start { name: "is", .. } => cell.has_inline = true,
            XmlEvent::Start { name: "rPh", .. } => cell.phonetic_depth += 1,
            XmlEvent::End("rPh") => cell.phonetic_depth = cell.phonetic_depth.saturating_sub(1),
            XmlEvent::Start { name: "t", .. } if cell.phonetic_depth == 0 && !empty_start => {
                cell.capture = Capture::InlineText;
            }
            XmlEvent::End("f" | "v" | "t") => cell.capture = Capture::None,
            XmlEvent::Text(text) => match cell.capture {
                Capture::Formula => {
                    if let Some(f) = cell.formula.as_mut() {
                        append(&mut f.body, text);
                    }
                }
                Capture::Value => {
                    if let Some(v) = cell.value.as_mut() {
                        append(v, text);
                    }
                }
                Capture::InlineText => append(&mut cell.inline, text),
                Capture::None => {}
            },
            _ => {}
        }
    }

    /// The cell's formula as cell text (`=` first), or `None` when it has no usable one.
    fn formula_text(&mut self, cell: &CellState<'_>) -> Option<String> {
        let formula = cell.formula.as_ref()?;
        let (row, col) = cell.pos;
        if !formula.body.is_empty() {
            if let (true, Some(index)) = (formula.shared, formula.index) {
                self.masters
                    .insert(index, (row, col, formula.body.to_string()));
            }
            return Some(with_equals(&formula.body));
        }
        if formula.shared {
            let (mr, mc, master) = self.masters.get(&formula.index?)?;
            let dr = i64::from(row) - i64::from(*mr);
            let dc = i64::from(col) - i64::from(*mc);
            return shift_shared_formula(master, dr, dc).map(|text| with_equals(&text));
        }
        None
    }

    fn finish_cell(&mut self, cell: CellState<'_>) -> Result<(), EasyOfficeError> {
        let (row, col) = cell.pos;
        if let Some(formula) = self.formula_text(&cell) {
            self.cells.push((row, col, formula));
        } else if cell.cell_type == "inlineStr" || (cell.cell_type.is_empty() && cell.has_inline) {
            self.cells
                .push((row, col, decode_xlsx_escapes(cell.inline)));
        } else if let Some(raw) = cell.value {
            let text = match &*cell.cell_type {
                "s" => self.shared_string(&raw)?,
                "b" => match raw.trim() {
                    "1" | "true" | "TRUE" => "TRUE".to_string(),
                    "0" | "false" | "FALSE" => "FALSE".to_string(),
                    _ => raw.into_owned(),
                },
                "d" => raw.trim().to_string(),
                _ => decode_xlsx_escapes(raw),
            };
            self.cells.push((row, col, text));
        }
        Ok(())
    }

    fn shared_string(&self, raw: &str) -> Result<String, EasyOfficeError> {
        raw.trim()
            .parse::<usize>()
            .ok()
            .and_then(|index| self.shared.get(index))
            .map(str::to_string)
            .ok_or_else(|| {
                EasyOfficeError::CorruptedContainer(format!(
                    "XLSX 공유 문자열 인덱스 '{raw}'를 읽을 수 없습니다."
                ))
            })
    }
}

/// Reads the cells of `sheet_xml` in document order; with `want_styling` also the style of every
/// cell that `xf_styles` marks and the custom column widths.
pub(crate) fn scan_worksheet(
    sheet_xml: &str,
    shared: &SharedStrings,
    xf_styles: &[CellStyle],
    want_styling: bool,
) -> Result<WorksheetScan, EasyOfficeError> {
    let mut scanner = SheetScanner {
        shared,
        xf_styles,
        want_styling,
        cells: Vec::new(),
        styling: SheetStyling::default(),
        masters: HashMap::new(),
    };
    let mut reader = XmlReader::unbounded(sheet_xml);
    let mut cell: Option<CellState<'_>> = None;
    while let Some(event) = reader.next_event()? {
        match event {
            XmlEvent::Start {
                name: "c",
                ref attrs,
            } => {
                cell = scanner.open_cell(attrs, reader.last_start_was_empty())?;
            }
            XmlEvent::Start {
                name: "col",
                ref attrs,
            } if want_styling => {
                scanner.record_column(attrs);
            }
            XmlEvent::End("c") => {
                if let Some(done) = cell.take() {
                    scanner.finish_cell(done)?;
                }
            }
            event => {
                if let Some(open) = cell.as_mut() {
                    scanner.feed(open, event, reader.last_start_was_empty());
                }
            }
        }
    }
    Ok(WorksheetScan {
        cells: scanner.cells,
        styling: scanner.styling,
    })
}

/// Decodes ECMA-376 `_xHHHH_` UTF-16 escape sequences (including surrogate pairs) in XLSX strings.
pub(crate) fn decode_xlsx_escapes(s: Cow<'_, str>) -> String {
    if !s.as_bytes().contains(&b'_') || !s.contains("_x") {
        return s.into_owned();
    }
    let mut out = String::with_capacity(s.len());
    decode_xlsx_escapes_into(&s, &mut out);
    out
}

/// Appends `s` to `out` with its `_xHHHH_` escapes decoded.
fn decode_xlsx_escapes_into(s: &str, out: &mut String) {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut high_surrogate: Option<u16> = None;
    while i < bytes.len() {
        if i + 7 <= bytes.len() && bytes[i] == b'_' && bytes[i + 1] == b'x' && bytes[i + 6] == b'_'
        {
            if let Ok(hex_str) = std::str::from_utf8(&bytes[i + 2..i + 6]) {
                if let Ok(unit) = u16::from_str_radix(hex_str, 16) {
                    if (0xD800..=0xDBFF).contains(&unit) {
                        if high_surrogate.replace(unit).is_some() {
                            out.push(char::REPLACEMENT_CHARACTER);
                        }
                        i += 7;
                        continue;
                    } else if (0xDC00..=0xDFFF).contains(&unit) {
                        if let Some(hi) = high_surrogate.take() {
                            let cp = 0x10000
                                + (((u32::from(hi) - 0xD800) << 10) | (u32::from(unit) - 0xDC00));
                            out.push(char::from_u32(cp).unwrap_or(char::REPLACEMENT_CHARACTER));
                            i += 7;
                            continue;
                        }
                    } else {
                        if high_surrogate.take().is_some() {
                            out.push(char::REPLACEMENT_CHARACTER);
                        }
                        out.push(
                            char::from_u32(u32::from(unit)).unwrap_or(char::REPLACEMENT_CHARACTER),
                        );
                        i += 7;
                        continue;
                    }
                }
            }
        }
        if high_surrogate.take().is_some() {
            out.push(char::REPLACEMENT_CHARACTER);
        }
        let ch = s[i..].chars().next().unwrap_or(char::REPLACEMENT_CHARACTER);
        out.push(ch);
        i += ch.len_utf8();
    }
    if high_surrogate.is_some() {
        out.push(char::REPLACEMENT_CHARACTER);
    }
}

fn shift_shared_formula(formula: &str, dr: i64, dc: i64) -> Option<String> {
    if formula.as_bytes().contains(&b'[') {
        return None;
    }
    let bytes = formula.as_bytes();
    let mut out = String::with_capacity(formula.len() + 8);
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            out.push('"');
            i += 1;
            while i < bytes.len() {
                let b = bytes[i];
                out.push(b as char);
                i += 1;
                if b == b'"' {
                    if i < bytes.len() && bytes[i] == b'"' {
                        out.push('"');
                        i += 1;
                    } else {
                        break;
                    }
                }
            }
            continue;
        }
        let prev_ok = i == 0
            || !(bytes[i - 1].is_ascii_alphanumeric() || matches!(bytes[i - 1], b'_' | b'.'));
        if prev_ok {
            if let Some((end_idx, abs_col, col0, abs_row, row0)) = match_a1_ref(bytes, i) {
                let new_col = if abs_col {
                    col0 as i64
                } else {
                    col0 as i64 + dc
                };
                let new_row = if abs_row {
                    row0 as i64
                } else {
                    row0 as i64 + dr
                };
                if !(0..=16_383).contains(&new_col) || !(0..=1_048_575).contains(&new_row) {
                    return None;
                }
                if abs_col {
                    out.push('$');
                }
                out.push_str(&format_col_letters(new_col as u32));
                if abs_row {
                    out.push('$');
                }
                out.push_str(&(new_row + 1).to_string());
                i = end_idx;
                continue;
            }
        }
        let ch = formula[i..].chars().next()?;
        out.push(ch);
        i += ch.len_utf8();
    }
    Some(out)
}

fn match_a1_ref(bytes: &[u8], start: usize) -> Option<(usize, bool, u32, bool, u32)> {
    let mut p = start;
    let abs_col = if p < bytes.len() && bytes[p] == b'$' {
        p += 1;
        true
    } else {
        false
    };
    let col_start = p;
    let mut col: u32 = 0;
    while p < bytes.len() && bytes[p].is_ascii_alphabetic() && p - col_start < 3 {
        col = col * 26 + (bytes[p].to_ascii_uppercase() - b'A') as u32 + 1;
        p += 1;
    }
    if p == col_start || (p < bytes.len() && bytes[p].is_ascii_alphabetic()) {
        return None;
    }
    let abs_row = if p < bytes.len() && bytes[p] == b'$' {
        p += 1;
        true
    } else {
        false
    };
    let row_start = p;
    if p >= bytes.len() || !bytes[p].is_ascii_digit() || bytes[p] == b'0' {
        return None;
    }
    let mut row: u32 = 0;
    while p < bytes.len() && bytes[p].is_ascii_digit() && p - row_start < 7 {
        row = row.checked_mul(10)?.checked_add((bytes[p] - b'0') as u32)?;
        p += 1;
    }
    if p < bytes.len()
        && (bytes[p].is_ascii_alphanumeric() || matches!(bytes[p], b'_' | b'.' | b'(' | b'!'))
    {
        return None;
    }
    let col0 = col.checked_sub(1)?;
    let row0 = row.checked_sub(1)?;
    (col0 <= 16_383 && row0 <= 1_048_575).then_some((p, abs_col, col0, abs_row, row0))
}

#[cfg(test)]
#[path = "xlsx_sheet/tests.rs"]
mod tests;

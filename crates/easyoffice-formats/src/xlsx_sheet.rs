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

use crate::decode_budget::DecodeBudget;
use crate::rtf_xml_helpers::{format_col_letters, parse_a1_zero_based};
use crate::sheet_styling::sheet_view::{push_level, Outline, MAX_OUTLINE_ROWS};
use crate::sheet_styling::xlsx_filter::FilterScan;
use crate::sheet_styling::xlsx_layout::{add_run, has_room, parse_merge};
use crate::sheet_styling::xlsx_page::PageScan;
use crate::sheet_styling::xlsx_protect::ProtectScan;
use crate::sheet_styling::xlsx_styles::{xlsx_width_to_px, XfStyles};
use crate::sheet_styling::xlsx_view::{level_attr, ThemeTab, ViewScan, LAST_COL};
use crate::sheet_styling::{run_total, CellStyle, SheetStyling, MAX_HIDDEN_ROWS, MAX_STYLED_CELLS};
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
    /// A tab colour that names a theme slot; the workbook's theme turns it into RGB.
    pub(crate) tab_theme: Option<ThemeTab>,
    /// The cells whose format points at the "Hyperlink" cell style (PD-223).
    pub(crate) link_styled: Vec<(u32, u32)>,
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
    /// The `ref` of an array formula (`t="array"`): the cells its results fill.
    array_ref: Option<Cow<'a, str>>,
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

/// The zero-based position of a cell from `row`/`col`, else `r`, else `follow`: the cell after the
/// previous one in its row (ECMA-376 18.3.1.4 makes `r` optional).
fn cell_position(
    row: Option<&str>,
    col: Option<&str>,
    a1: Option<&str>,
    follow: (u32, u32),
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
        None if follow.0 >= OoxmlCodec::MAX_XLSX_ROWS || follow.1 >= OoxmlCodec::MAX_XLSX_COLS => {
            Err(EasyOfficeError::ResourceBudgetExceeded(
                "XLSX 셀 위치가 ECMA-376 허용 범위를 초과했습니다.".to_string(),
            ))
        }
        None => Ok(follow),
    }
}

/// Whether the boolean attribute `name` (`1`, `true`) is set.
fn attrs_flag(attrs: &Attrs<'_>, name: &str) -> bool {
    attrs
        .iter_raw()
        .any(|(n, v)| plain_name(n) == name && matches!(&*unescape_cow(v), "1" | "true"))
}

/// What the decoded-budget error names when a cell's copy of a string or formula is refused.
const CELL_BUDGET_WHAT: &str = "XLSX 셀 텍스트";

struct SheetScanner<'s> {
    shared: &'s SharedStrings,
    budget: &'s mut DecodeBudget,
    xf_styles: &'s [CellStyle],
    /// Per format: it points at the "Hyperlink" cell style; the cells that use one are listed in `link_styled`.
    link_xfs: &'s [bool],
    link_styled: Vec<(u32, u32)>,
    want_styling: bool,
    cells: Vec<(u32, u32, String)>,
    styling: SheetStyling,
    /// First master formula text of each shared formula index with the cell it sits in.
    masters: HashMap<u32, (u32, u32, String)>,
    /// Row of the open `row` element and the row, column a following cell or row without `r` gets.
    row: u32,
    next_row: u32,
    next_col: u32,
    /// Rows and columns put in groups so far, and the rows and columns that carry `collapsed`.
    outline_rows: u64,
    outline_cols: u64,
    marks_rows: Vec<u32>,
    marks_cols: Vec<u32>,
}

impl<'s> SheetScanner<'s> {
    /// Opens a `row`: its `r` (1-based), else the row after the previous one.
    fn open_row(&mut self, attrs: &Attrs<'_>) -> Result<(), EasyOfficeError> {
        let declared = attrs
            .iter_raw()
            .find(|(name, _)| plain_name(name) == "r")
            .and_then(|(_, value)| unescape_cow(value).parse::<u32>().ok())
            .and_then(|r| r.checked_sub(1));
        self.row = declared.unwrap_or(self.next_row);
        if self.row >= OoxmlCodec::MAX_XLSX_ROWS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "XLSX 행 번호가 ECMA-376 허용 범위를 초과했습니다.".to_string(),
            ));
        }
        self.next_row = self.row + 1;
        self.next_col = 0;
        if self.want_styling && has_room(&self.styling) && attrs_flag(attrs, "hidden") {
            add_run(&mut self.styling.hidden_rows, self.row, self.row);
        }
        if self.want_styling {
            self.record_row_group(attrs);
            self.record_row_lock(attrs);
        }
        Ok(())
    }

    /// The format of the open `row` (`s` with `customFormat`) unlocks it (PD-216).
    fn record_row_lock(&mut self, attrs: &Attrs<'_>) {
        if attrs_flag(attrs, "customFormat")
            && self.xf_unlocked(attrs, "s")
            && run_total(&self.styling.unlocked_rows) < MAX_HIDDEN_ROWS + MAX_OUTLINE_ROWS
        {
            add_run(&mut self.styling.unlocked_rows, self.row, self.row);
        }
    }

    /// Whether the cell format `attr` of an element names says unlocked.
    fn xf_unlocked(&self, attrs: &Attrs<'_>, attr: &str) -> bool {
        attrs
            .iter_raw()
            .find(|(name, _)| plain_name(name) == attr)
            .and_then(|(_, value)| unescape_cow(value).parse::<usize>().ok())
            .and_then(|i| self.xf_styles.get(i))
            .is_some_and(|style| style.unlocked)
    }

    /// The group level and the `collapsed` mark of the open `row`.
    fn record_row_group(&mut self, attrs: &Attrs<'_>) {
        let at = (self.row, self.row);
        let rows = &mut self.styling.outline.rows;
        push_level(
            rows,
            &mut self.outline_rows,
            MAX_OUTLINE_ROWS,
            at,
            level_attr(attrs),
        );
        if attrs_flag(attrs, "collapsed") && (self.marks_rows.len() as u64) < MAX_OUTLINE_ROWS {
            self.marks_rows.push(self.row);
        }
    }

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
        let follow = (self.row, self.next_col);
        if self.want_styling && !self.xf_styles.is_empty() {
            let at = a1
                .as_deref()
                .and_then(parse_a1_zero_based)
                .unwrap_or(follow);
            self.record_style(style.as_deref(), at);
        }
        if empty {
            self.next_col = a1
                .as_deref()
                .and_then(parse_a1_zero_based)
                .map_or(follow.1, |(_, c)| c)
                .saturating_add(1);
            return Ok(None);
        }
        let pos = cell_position(row.as_deref(), col.as_deref(), a1.as_deref(), follow)?;
        self.next_col = pos.1.saturating_add(1);
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

    /// Takes the view of the sheet and tells the rows and columns a folded group hid from the ones
    /// the user hid.
    fn finish_view(&mut self, view: &ViewScan) {
        if !self.want_styling {
            return;
        }
        let styling = &mut self.styling;
        styling.freeze = view.freeze;
        styling.view = view.state();
        styling.tab_color = view.tab_rgb;
        let outline = &mut styling.outline;
        if outline.is_empty() {
            return;
        }
        (outline.summary_above, outline.summary_left) = (view.summary_above, view.summary_left);
        self.marks_rows.sort_unstable();
        self.marks_cols.sort_unstable();
        outline.collapsed_rows = Outline::split_collapsed(
            &outline.rows,
            &mut styling.hidden_rows,
            &self.marks_rows,
            outline.summary_above,
        );
        outline.collapsed_cols = Outline::split_collapsed(
            &outline.cols,
            &mut styling.hidden_cols,
            &self.marks_cols,
            outline.summary_left,
        );
        outline.open_rows = Outline::open_inside(
            &outline.rows,
            &outline.collapsed_rows,
            &self.marks_rows,
            outline.summary_above,
        );
        outline.open_cols = Outline::open_inside(
            &outline.cols,
            &outline.collapsed_cols,
            &self.marks_cols,
            outline.summary_left,
        );
    }

    fn record_style(&mut self, style: Option<&str>, at: (u32, u32)) {
        if self.styling.cells.len() >= MAX_STYLED_CELLS {
            return;
        }
        let Some((index, style)) = style
            .and_then(|v| v.parse::<usize>().ok())
            .and_then(|i| Some((i, self.xf_styles.get(i)?)))
            .filter(|(_, s)| !s.is_plain())
        else {
            return;
        };
        self.styling.cells.insert(at, *style);
        if self.link_xfs.get(index).copied().unwrap_or(false) {
            self.link_styled.push(at);
        }
    }

    /// A `mergeCell` becomes a merged range.
    fn record_merge(&mut self, attrs: &Attrs<'_>) {
        if !has_room(&self.styling) {
            return;
        }
        let merge = attrs
            .iter_raw()
            .find(|(name, _)| plain_name(name) == "ref")
            .and_then(|(_, value)| parse_merge(&unescape_cow(value)));
        self.styling.merges.extend(merge);
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
        let parse = |v: Option<Cow<'_, str>>| v?.parse::<u32>().ok();
        let (Some(min), Some(max)) = (parse(min), parse(max)) else {
            return;
        };
        if min >= 1 && max >= min {
            let span = (min - 1, (max - 1).min(LAST_COL));
            if self.xf_unlocked(attrs, "style") {
                add_run(&mut self.styling.unlocked_cols, span.0, span.1);
            }
            if attrs_flag(attrs, "hidden") {
                add_run(&mut self.styling.hidden_cols, span.0, span.1);
            }
            let cols = &mut self.styling.outline.cols;
            push_level(
                cols,
                &mut self.outline_cols,
                u64::from(LAST_COL) + 1,
                span,
                level_attr(attrs),
            );
            if attrs_flag(attrs, "collapsed") {
                self.marks_cols.extend(span.0..=span.1);
            }
        }
        if matches!(custom.as_deref(), None | Some("0" | "false")) {
            return;
        }
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
                let (mut kind, mut index, mut array_ref) = (None, None, None);
                for (name, value) in attrs.iter_raw() {
                    match plain_name(name) {
                        "t" => kind.get_or_insert_with(|| unescape_cow(value)),
                        "si" => index.get_or_insert_with(|| unescape_cow(value)),
                        "ref" => array_ref.get_or_insert_with(|| unescape_cow(value)),
                        _ => continue,
                    };
                }
                let is_array = kind.as_deref() == Some("array");
                cell.formula = Some(Formula {
                    shared: kind.as_deref() == Some("shared"),
                    index: index.and_then(|s| s.parse::<u32>().ok()),
                    body: Cow::Borrowed(""),
                    array_ref: array_ref.filter(|_| is_array),
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

    /// The cell's formula as cell text (`=` first), or `None` when it has no usable one. The
    /// copies it makes (a master kept for its slaves, a slave's shifted text) are charged to the
    /// decoded budget.
    fn formula_text(&mut self, cell: &CellState<'_>) -> Result<Option<String>, EasyOfficeError> {
        let Some(formula) = cell.formula.as_ref() else {
            return Ok(None);
        };
        let (row, col) = cell.pos;
        if !formula.body.is_empty() {
            if let (true, Some(index)) = (formula.shared, formula.index) {
                self.budget.charge(formula.body.len(), CELL_BUDGET_WHAT)?;
                self.masters
                    .insert(index, (row, col, formula.body.to_string()));
            }
            self.budget
                .charge_cell(formula.body.len() + 1, CELL_BUDGET_WHAT)?;
            return Ok(Some(with_equals(&crate::from_file_formula(&formula.body))));
        }
        if !formula.shared {
            return Ok(None);
        }
        let Some((mr, mc, master)) = formula.index.and_then(|index| self.masters.get(&index))
        else {
            return Ok(None);
        };
        let dr = i64::from(row) - i64::from(*mr);
        let dc = i64::from(col) - i64::from(*mc);
        // The shifted text can be longer than the master (`A1` becomes `XFD1048576`), so it is
        // charged by its own length; the temporary comes from a master already paid for.
        let Some(shifted) = shift_shared_formula(master, dr, dc) else {
            return Ok(None);
        };
        self.budget
            .charge_cell(shifted.len() + 1, CELL_BUDGET_WHAT)?;
        Ok(Some(with_equals(&crate::from_file_formula(&shifted))))
    }

    /// The text a finished cell contributes, charged to the decoded budget; `None` when the cell
    /// has no content.
    fn cell_text(&mut self, cell: CellState<'_>) -> Result<Option<String>, EasyOfficeError> {
        if let Some(formula) = self.formula_text(&cell)? {
            return Ok(Some(formula));
        }
        if cell.cell_type == "inlineStr" || (cell.cell_type.is_empty() && cell.has_inline) {
            let text = decode_xlsx_escapes(cell.inline);
            self.budget.charge_cell(text.len(), CELL_BUDGET_WHAT)?;
            return Ok(Some(text));
        }
        let Some(raw) = cell.value else {
            return Ok(None);
        };
        let text = match &*cell.cell_type {
            "s" => {
                let shared = self.shared_string(&raw)?;
                self.budget.charge_cell(shared.len(), CELL_BUDGET_WHAT)?;
                return Ok(Some(shared.to_string()));
            }
            "b" => match raw.trim() {
                "1" | "true" | "TRUE" => "TRUE".to_string(),
                "0" | "false" | "FALSE" => "FALSE".to_string(),
                _ => raw.into_owned(),
            },
            "d" => raw.trim().to_string(),
            _ => decode_xlsx_escapes(raw),
        };
        self.budget.charge_cell(text.len(), CELL_BUDGET_WHAT)?;
        Ok(Some(text))
    }

    fn finish_cell(&mut self, cell: CellState<'_>) -> Result<(), EasyOfficeError> {
        let (row, col) = cell.pos;
        if let Some(reference) = cell.formula.as_ref().and_then(|f| f.array_ref.as_deref()) {
            self.record_spill(row, col, reference)?;
        }
        if let Some(text) = self.cell_text(cell)? {
            self.cells.push((row, col, text));
        }
        Ok(())
    }

    /// Forgets the cached results of the arrays: the engine calculates them.
    fn drop_spilled_cells(&mut self) {
        self.styling.spills = crate::normalize_spills(
            &self.styling.spills,
            OoxmlCodec::MAX_XLSX_ROWS,
            OoxmlCodec::MAX_XLSX_COLS,
        );
        if self.styling.spills.is_empty() {
            return;
        }
        let spills = &self.styling.spills;
        self.cells.retain(|(row, col, _)| {
            let before = spills.partition_point(|r| r.row <= *row);
            !crate::is_covered_by_spills(&spills[..before], *row, *col)
        });
    }

    /// An array formula over more than one cell is an array that spills: the cells after its first
    /// hold the results the file cached.
    fn record_spill(&mut self, row: u32, col: u32, reference: &str) -> Result<(), EasyOfficeError> {
        if !self.want_styling {
            return Ok(());
        }
        let Some(area) = crate::sheet_styling::xlsx_layout::parse_merge(reference) else {
            return Ok(());
        };
        if (area.row0, area.col0) != (row, col) {
            return Ok(());
        }
        let (Some(rows), Some(cols)) = (
            area.row1
                .checked_sub(area.row0)
                .and_then(|d| d.checked_add(1)),
            area.col1
                .checked_sub(area.col0)
                .and_then(|d| d.checked_add(1)),
        ) else {
            return Ok(());
        };
        if rows > 1 || cols > 1 {
            if self.styling.spills.len() >= crate::MAX_SPILL_REFS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "XLSX 시트당 배열 수식 스필 개수가 허용 상한을 초과했습니다.".to_string(),
                ));
            }
            self.styling.spills.push(crate::SpillRef {
                row,
                col,
                rows,
                cols,
            });
        }
        Ok(())
    }

    fn shared_string(&self, raw: &str) -> Result<&'s str, EasyOfficeError> {
        raw.trim()
            .parse::<usize>()
            .ok()
            .and_then(|index| self.shared.get(index))
            .ok_or_else(|| {
                EasyOfficeError::CorruptedContainer(format!(
                    "XLSX 공유 문자열 인덱스 '{raw}'를 읽을 수 없습니다."
                ))
            })
    }
}

/// Reads the cells of `sheet_xml` in document order; with `want_styling` also the style of every
/// cell that `xf_styles` marks and the custom column widths. The text of the cells is charged to
/// `budget`, which the caller shares across the sheets of one workbook.
pub(crate) fn scan_worksheet(
    sheet_xml: &str,
    shared: &SharedStrings,
    xf: &XfStyles,
    want_styling: bool,
    budget: &mut DecodeBudget,
) -> Result<WorksheetScan, EasyOfficeError> {
    let mut scanner = SheetScanner {
        shared,
        budget,
        xf_styles: &xf.styles,
        link_xfs: &xf.link_xfs,
        link_styled: Vec::new(),
        want_styling,
        cells: Vec::new(),
        styling: SheetStyling::default(),
        masters: HashMap::new(),
        row: 0,
        next_row: 0,
        next_col: 0,
        outline_rows: 0,
        outline_cols: 0,
        marks_rows: Vec::new(),
        marks_cols: Vec::new(),
    };
    let mut reader = XmlReader::unbounded(sheet_xml);
    let mut cell: Option<CellState<'_>> = None;
    let mut filter = FilterScan::new(&xf.dxf_colors);
    let mut view = ViewScan::default();
    let mut page = PageScan::default();
    let mut protect = ProtectScan::default();
    while let Some(event) = reader.next_event()? {
        if want_styling {
            filter.feed(&event);
            view.feed(&event);
            page.feed(&event);
            protect.feed(&event);
        }
        match event {
            XmlEvent::Start {
                name: "c",
                ref attrs,
            } => {
                cell = scanner.open_cell(attrs, reader.last_start_was_empty())?;
            }
            XmlEvent::Start {
                name: "row",
                ref attrs,
            } => scanner.open_row(attrs)?,
            XmlEvent::Start {
                name: "col",
                ref attrs,
            } if want_styling => {
                scanner.record_column(attrs);
            }
            XmlEvent::Start {
                name: "mergeCell",
                ref attrs,
            } if want_styling => scanner.record_merge(attrs),
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
    scanner.styling.filter = filter.finish();
    scanner.styling.page = page.finish();
    scanner.styling.protect = protect.finish()?;
    scanner.styling.normalize_locks();
    scanner.finish_view(&view);
    scanner.drop_spilled_cells();
    Ok(WorksheetScan {
        cells: scanner.cells,
        styling: scanner.styling,
        tab_theme: view.tab_theme,
        link_styled: scanner.link_styled,
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

/// The index after the quoted run (`"text"` or `'Sheet'`) that starts at `start`, where a doubled
/// quote is part of the run; the end of `bytes` for a run that never closes. Quotes are ASCII, so
/// the index is always a character boundary.
fn quoted_end(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == quote {
            if bytes.get(i + 1) == Some(&quote) {
                i += 2;
                continue;
            }
            return i + 1;
        }
        i += 1;
    }
    bytes.len()
}

fn shift_shared_formula(formula: &str, dr: i64, dc: i64) -> Option<String> {
    let bytes = formula.as_bytes();
    let mut out = String::with_capacity(formula.len() + 8);
    let mut i = 0usize;
    while i < bytes.len() {
        // The group of a structured reference names columns, not cells: it is copied as written (PD-225).
        if bytes[i] == b'[' {
            let end = crate::group_end(formula, i)?;
            out.push_str(&formula[i..end]);
            i = end;
            continue;
        }
        // Quoted text and quoted sheet names pass untouched: a doubled quote stays inside.
        if matches!(bytes[i], b'"' | b'\'') {
            let end = quoted_end(bytes, i);
            out.push_str(&formula[i..end]);
            i = end;
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

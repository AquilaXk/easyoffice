// 참조 구현: jmcnamara/rust_xlsxwriter (MIT OR Apache-2.0, Copyright 2022-2026 John McNamara, src/chart.rs, src/drawing.rs; see THIRD_PARTY_NOTICES.md)

//! Spreadsheet chart model, fail-closed resource budgets, single-pass series derivation,
//! and O(1) cell lookup index for XLSX/PDF/JSON pipelines (`PD-254`).

use std::collections::BTreeMap;
use std::fmt::Write;

use crate::cell_numbers::numeric_cell_value;
use crate::cell_type::pack_cell_pos;
use crate::pages_proto::FastU64Map;
use crate::rtf_xml_helpers::format_col_letters;
use crate::sheet_styling::sheet_anchor_edit::{AnchorCells, AnchorEdit};
use crate::sheet_styling::sheet_media_notes::MAX_SHEET_IMAGE_DIM_PX;
use crate::sheet_styling::sheet_row_offsets::{RowOffsets, SheetGrid};
use crate::{
    CellBox, EasyOfficeError, OdsUnpacked, OoxmlCodec, SheetStyling, XlsxCachedValue,
    XlsxFormulaCache,
};

pub const MAX_CHARTS_PER_SHEET: usize = 64;
pub const MAX_WORKBOOK_CHARTS: usize = 256;
pub const MAX_SERIES_PER_CHART: usize = 32;
pub const MAX_POINTS_PER_SERIES: usize = 4_096;
pub const MAX_CHART_TEXT_BYTES: usize = 512;

pub const DEFAULT_CHART_WIDTH_PX: u32 = 420;
pub const DEFAULT_CHART_HEIGHT_PX: u32 = 260;

pub const EMU_PER_PX: u64 = 9_525;

/// Supported chart types (`column`, `bar`, `line`, `pie`, `scatter`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChartKind {
    #[default]
    Column,
    Bar,
    Line,
    Pie,
    Scatter,
}

impl ChartKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Column => "column",
            Self::Bar => "bar",
            Self::Line => "line",
            Self::Pie => "pie",
            Self::Scatter => "scatter",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim();
        if t.eq_ignore_ascii_case("column") {
            Some(Self::Column)
        } else if t.eq_ignore_ascii_case("bar") {
            Some(Self::Bar)
        } else if t.eq_ignore_ascii_case("line") {
            Some(Self::Line)
        } else if t.eq_ignore_ascii_case("pie") {
            Some(Self::Pie)
        } else if t.eq_ignore_ascii_case("scatter") {
            Some(Self::Scatter)
        } else {
            None
        }
    }
}

/// Legend placement (`none`, `right`, `bottom`, `top`, `left`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChartLegendPos {
    None,
    #[default]
    Right,
    Bottom,
    Top,
    Left,
}

impl ChartLegendPos {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Right => "right",
            Self::Bottom => "bottom",
            Self::Top => "top",
            Self::Left => "left",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim();
        if t.eq_ignore_ascii_case("none") {
            Some(Self::None)
        } else if t.eq_ignore_ascii_case("right") || t.eq_ignore_ascii_case("r") {
            Some(Self::Right)
        } else if t.eq_ignore_ascii_case("bottom") || t.eq_ignore_ascii_case("b") {
            Some(Self::Bottom)
        } else if t.eq_ignore_ascii_case("top") || t.eq_ignore_ascii_case("t") {
            Some(Self::Top)
        } else if t.eq_ignore_ascii_case("left") || t.eq_ignore_ascii_case("l") {
            Some(Self::Left)
        } else {
            None
        }
    }

    pub const fn ooxml_val(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Right => Some("r"),
            Self::Bottom => Some("b"),
            Self::Top => Some("t"),
            Self::Left => Some("l"),
        }
    }

    pub fn from_ooxml(val: &str) -> Self {
        match val.trim() {
            "b" => Self::Bottom,
            "t" => Self::Top,
            "l" => Self::Left,
            _ => Self::Right,
        }
    }
}

/// Two-cell DrawingML anchor paired with pixel coordinates (each size `1..=MAX_SHEET_IMAGE_DIM_PX`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartAnchor {
    pub edit_as: AnchorEdit,
    pub from_col: u32,
    pub from_row: u32,
    pub to_col: u32,
    pub to_row: u32,
    pub col_off_emu: u32,
    pub row_off_emu: u32,
    pub to_col_off_emu: u32,
    pub to_row_off_emu: u32,
    pub left_px: u32,
    pub top_px: u32,
    pub width_px: u32,
    pub height_px: u32,
}

impl Default for ChartAnchor {
    fn default() -> Self {
        Self {
            edit_as: AnchorEdit::TwoCell,
            from_col: 0,
            from_row: 0,
            to_col: 0,
            to_row: 0,
            col_off_emu: 0,
            row_off_emu: 0,
            to_col_off_emu: 0,
            to_row_off_emu: 0,
            left_px: 0,
            top_px: 0,
            width_px: DEFAULT_CHART_WIDTH_PX,
            height_px: DEFAULT_CHART_HEIGHT_PX,
        }
    }
}

/// Cumulative x-coordinate (CSS px) of `col` in O(|columns|) using sparse custom widths; the other columns are
/// `default_px` wide (`SheetStyling::default_col_px`, PD-305).
pub fn col_left_px(col: u32, columns: &BTreeMap<u32, u32>, default_px: u32) -> u64 {
    let mut x = u64::from(col) * u64::from(default_px);
    for (&c, &w) in columns.range(..col) {
        let _ = c;
        let clamped = w.max(1);
        x = x
            .saturating_add(u64::from(clamped))
            .saturating_sub(u64::from(default_px));
    }
    x
}

#[inline]
pub(crate) fn emu_to_px_rounded(emu: u32) -> u64 {
    (u64::from(emu) + EMU_PER_PX / 2) / EMU_PER_PX
}

pub(crate) fn px_to_col_and_emu(
    target_px: u64,
    columns: &BTreeMap<u32, u32>,
    default_px: u32,
) -> (u32, u32) {
    let default_px = u64::from(default_px.max(1));
    let max_col = OoxmlCodec::MAX_XLSX_COLS - 1;
    let mut cur_col = 0u32;
    let mut cur_x = 0u64;
    for (&col, &w) in columns {
        if col > max_col {
            break;
        }
        let gap_cols = u64::from(col - cur_col);
        let gap_px = gap_cols * default_px;
        if target_px < cur_x + gap_px {
            let rem = target_px - cur_x;
            let dc = (rem / default_px) as u32;
            let off_px = rem % default_px;
            return (
                cur_col + dc,
                (off_px * EMU_PER_PX).min(u64::from(u32::MAX)) as u32,
            );
        }
        cur_x += gap_px;
        let cw = u64::from(w.max(1));
        if target_px < cur_x + cw {
            let off_px = target_px - cur_x;
            return (col, (off_px * EMU_PER_PX).min(u64::from(u32::MAX)) as u32);
        }
        cur_x += cw;
        cur_col = col + 1;
    }
    let rem = target_px.saturating_sub(cur_x);
    let dc = rem / default_px;
    if u64::from(cur_col) + dc > u64::from(max_col) {
        return (max_col, 0);
    }
    let off_px = rem % default_px;
    (
        cur_col + dc as u32,
        (off_px * EMU_PER_PX).min(u64::from(u32::MAX)) as u32,
    )
}

/// The row at the vertical offset `target_px` and the EMU the offset lies below the row's top edge.
pub(crate) fn px_to_row_and_emu(target_px: u64, rows: &RowOffsets) -> (u32, u32) {
    let (row, off_px) = rows.locate(target_px);
    (row, (off_px * EMU_PER_PX).min(u64::from(u32::MAX)) as u32)
}

impl ChartAnchor {
    pub fn has_explicit_grid(&self) -> bool {
        (self.to_col > self.from_col || self.to_col_off_emu > self.col_off_emu)
            && (self.to_row > self.from_row || self.to_row_off_emu > self.row_off_emu)
    }

    /// Recomputes `left_px`, `top_px`, `width_px`, `height_px` from two-cell grid coordinates.
    pub fn sync_px_from_grid(&mut self, grid: &SheetGrid<'_>) {
        let rows = grid.rows();
        let x0 = grid
            .col_left(self.from_col)
            .saturating_add(emu_to_px_rounded(self.col_off_emu));
        let y0 = rows
            .top(self.from_row)
            .saturating_add(emu_to_px_rounded(self.row_off_emu));
        let x1 = grid
            .col_left(self.to_col)
            .saturating_add(emu_to_px_rounded(self.to_col_off_emu));
        let y1 = rows
            .top(self.to_row)
            .saturating_add(emu_to_px_rounded(self.to_row_off_emu));
        self.left_px = x0.min(u64::from(u32::MAX)) as u32;
        self.top_px = y0.min(u64::from(u32::MAX)) as u32;
        if x1 > x0 {
            self.width_px = u32::try_from(x1 - x0)
                .unwrap_or(u32::MAX)
                .clamp(1, MAX_SHEET_IMAGE_DIM_PX);
        }
        if y1 > y0 {
            self.height_px = u32::try_from(y1 - y0)
                .unwrap_or(u32::MAX)
                .clamp(1, MAX_SHEET_IMAGE_DIM_PX);
        }
    }

    /// Recomputes two-cell grid coordinates and EMU offsets from pixel coordinates.
    pub fn sync_grid_from_px(&mut self, grid: &SheetGrid<'_>) {
        let rows = grid.rows();
        let w = u64::from(self.width_px.clamp(1, MAX_SHEET_IMAGE_DIM_PX));
        let h = u64::from(self.height_px.clamp(1, MAX_SHEET_IMAGE_DIM_PX));
        let left = u64::from(self.left_px);
        let top = u64::from(self.top_px);
        let (fc, fce) = grid.col_at(left);
        let (fr, fre) = px_to_row_and_emu(top, rows);
        let (tc, tce) = grid.col_at(left.saturating_add(w));
        let (tr, tre) = px_to_row_and_emu(top.saturating_add(h), rows);
        self.from_col = fc;
        self.col_off_emu = fce;
        self.from_row = fr;
        self.row_off_emu = fre;
        self.to_col = tc;
        self.to_col_off_emu = tce;
        self.to_row = tr;
        self.to_row_off_emu = tre;
    }

    /// Returns effective two-cell `(from_col, from_row, col_off, row_off, to_col, to_row, to_col_off, to_row_off)`
    /// for DrawingML export and PDF intersection.
    pub fn effective_two_cell(&self, grid: &SheetGrid<'_>) -> AnchorCells {
        let mut copy = *self;
        copy.settle(grid, Self::sync_grid_from_px);
        copy.cells()
    }

    pub fn within_bounds(&self) -> bool {
        // The size a file gives is kept whatever it is (the page limits the size of a chart a user drags to, not the file).
        (1..=MAX_SHEET_IMAGE_DIM_PX).contains(&self.width_px)
            && (1..=MAX_SHEET_IMAGE_DIM_PX).contains(&self.height_px)
            && self.from_col < OoxmlCodec::MAX_XLSX_COLS
            && self.to_col < OoxmlCodec::MAX_XLSX_COLS
            && self.from_row < OoxmlCodec::MAX_XLSX_ROWS
            && self.to_row < OoxmlCodec::MAX_XLSX_ROWS
    }
}

/// Series reference inside a [`SheetChart`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChartSeriesRef {
    pub name: String,
    pub name_ref: Option<String>,
    pub categories_ref: Option<String>,
    pub values_ref: String,
}

impl ChartSeriesRef {
    pub fn within_bounds(&self) -> bool {
        self.name.len() <= MAX_CHART_TEXT_BYTES
            && self
                .name_ref
                .as_deref()
                .is_none_or(|s| s.len() <= MAX_CHART_TEXT_BYTES)
            && self
                .categories_ref
                .as_deref()
                .is_none_or(|s| s.len() <= MAX_CHART_TEXT_BYTES)
            && self.values_ref.len() <= MAX_CHART_TEXT_BYTES
            && ref_point_count(&self.values_ref).is_none_or(|n| n <= MAX_POINTS_PER_SERIES as u64)
            && self
                .categories_ref
                .as_deref()
                .and_then(ref_point_count)
                .is_none_or(|n| n <= MAX_POINTS_PER_SERIES as u64)
    }
}

/// Spreadsheet chart definition stored in `SheetStyling::charts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetChart {
    pub id: String,
    pub kind: ChartKind,
    pub title: String,
    pub data_range: String,
    pub switch_row_col: bool,
    pub legend: ChartLegendPos,
    pub anchor: ChartAnchor,
    pub series: Vec<ChartSeriesRef>,
}

impl Default for SheetChart {
    fn default() -> Self {
        Self {
            id: "chart-1".to_string(),
            kind: ChartKind::Column,
            title: String::new(),
            data_range: String::new(),
            switch_row_col: false,
            legend: ChartLegendPos::Right,
            anchor: ChartAnchor::default(),
            series: Vec::new(),
        }
    }
}

impl SheetChart {
    pub fn within_bounds(&self) -> bool {
        !self.id.trim().is_empty()
            && self.id.len() <= MAX_CHART_TEXT_BYTES
            && self.title.len() <= MAX_CHART_TEXT_BYTES
            && self.data_range.len() <= MAX_CHART_TEXT_BYTES
            && self.series.len() <= MAX_SERIES_PER_CHART
            && self.anchor.within_bounds()
            && self.series.iter().all(ChartSeriesRef::within_bounds)
            && data_range_within_points_budget(&self.data_range, self.switch_row_col)
    }

    /// Derives series references from `data_range` and `switch_row_col` in a single pass
    /// when `series` is empty (single row or single column yields one series).
    pub fn resolved_series(&self, sheet_name: &str, sheets: &[OdsUnpacked]) -> Vec<ChartSeriesRef> {
        if !self.series.is_empty() {
            return self
                .series
                .iter()
                .take(MAX_SERIES_PER_CHART)
                .cloned()
                .collect();
        }
        let Some((target_sheet, area)) = parse_sheet_and_box(sheet_name, &self.data_range) else {
            return Vec::new();
        };
        let rows = area.row1 - area.row0 + 1;
        let cols = area.col1 - area.col0 + 1;
        if rows == 1 || cols == 1 {
            let name = if self.title.trim().is_empty() {
                "Series 1".to_string()
            } else {
                self.title.clone()
            };
            return vec![ChartSeriesRef {
                name,
                name_ref: None,
                categories_ref: None,
                values_ref: format_sheet_range_ref(&target_sheet, area),
            }];
        }

        let sheet_cells = sheets
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(&target_sheet))
            .map(|(_, cells)| cells.as_slice())
            .unwrap_or(&[]);

        // Single pass over the target sheet cells inside `area`:
        // record header/category non-numeric flags and header labels only for row0 and col0.
        let mut row0_non_num_after_corner = false;
        let mut row0_non_empty_after_corner = 0u32;
        let mut col0_non_num_after_corner = false;
        let mut corner_non_empty = false;
        let mut row0_labels: BTreeMap<u32, String> = BTreeMap::new();
        let mut col0_labels: BTreeMap<u32, String> = BTreeMap::new();

        for &(r, c, ref raw) in sheet_cells {
            if !area.contains(r, c) {
                continue;
            }
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            let is_text_header = !trimmed.starts_with('=') && numeric_cell_value(trimmed).is_none();
            if r == area.row0 && c == area.col0 {
                corner_non_empty = true;
            }
            if r == area.row0 {
                row0_labels.insert(c, raw.clone());
                if c > area.col0 {
                    row0_non_empty_after_corner += 1;
                    if is_text_header {
                        row0_non_num_after_corner = true;
                    }
                }
            }
            if c == area.col0 {
                col0_labels.insert(r, raw.clone());
                if r > area.row0 && is_text_header {
                    col0_non_num_after_corner = true;
                }
            }
        }

        let mut out = Vec::new();
        if !self.switch_row_col {
            let has_col_headers = row0_non_num_after_corner
                || (!corner_non_empty
                    && row0_non_empty_after_corner > 0
                    && col0_non_num_after_corner);
            let data_row0 = if has_col_headers {
                area.row0 + 1
            } else {
                area.row0
            };
            let has_cat_col = col0_non_num_after_corner;
            let first_val_col = if has_cat_col {
                area.col0 + 1
            } else {
                area.col0
            };
            let cat_ref = has_cat_col.then(|| {
                format_sheet_range_ref(
                    &target_sheet,
                    CellBox {
                        row0: data_row0,
                        col0: area.col0,
                        row1: area.row1,
                        col1: area.col0,
                    },
                )
            });
            for (idx, col) in (first_val_col..=area.col1)
                .take(MAX_SERIES_PER_CHART)
                .enumerate()
            {
                let (name, name_ref) = if has_col_headers {
                    let cell = CellBox::cell(area.row0, col);
                    let label = row0_labels
                        .get(&col)
                        .filter(|s| !s.trim().is_empty())
                        .cloned()
                        .unwrap_or_else(|| format!("Series {}", idx + 1));
                    (label, Some(format_sheet_range_ref(&target_sheet, cell)))
                } else {
                    (format!("Series {}", idx + 1), None)
                };
                out.push(ChartSeriesRef {
                    name,
                    name_ref,
                    categories_ref: cat_ref.clone(),
                    values_ref: format_sheet_range_ref(
                        &target_sheet,
                        CellBox {
                            row0: data_row0,
                            col0: col,
                            row1: area.row1,
                            col1: col,
                        },
                    ),
                });
            }
        } else {
            let has_row_headers = col0_non_num_after_corner;
            let data_col0 = if has_row_headers {
                area.col0 + 1
            } else {
                area.col0
            };
            let has_cat_row = row0_non_num_after_corner
                || (!corner_non_empty && row0_non_empty_after_corner > 0 && has_row_headers);
            let first_val_row = if has_cat_row {
                area.row0 + 1
            } else {
                area.row0
            };
            let cat_ref = has_cat_row.then(|| {
                format_sheet_range_ref(
                    &target_sheet,
                    CellBox {
                        row0: area.row0,
                        col0: data_col0,
                        row1: area.row0,
                        col1: area.col1,
                    },
                )
            });
            for (idx, row) in (first_val_row..=area.row1)
                .take(MAX_SERIES_PER_CHART)
                .enumerate()
            {
                let (name, name_ref) = if has_row_headers {
                    let cell = CellBox::cell(row, area.col0);
                    let label = col0_labels
                        .get(&row)
                        .filter(|s| !s.trim().is_empty())
                        .cloned()
                        .unwrap_or_else(|| format!("Series {}", idx + 1));
                    (label, Some(format_sheet_range_ref(&target_sheet, cell)))
                } else {
                    (format!("Series {}", idx + 1), None)
                };
                out.push(ChartSeriesRef {
                    name,
                    name_ref,
                    categories_ref: cat_ref.clone(),
                    values_ref: format_sheet_range_ref(
                        &target_sheet,
                        CellBox {
                            row0: row,
                            col0: data_col0,
                            row1: row,
                            col1: area.col1,
                        },
                    ),
                });
            }
        }
        out
    }
}

fn data_range_within_points_budget(data_range: &str, switch_row_col: bool) -> bool {
    let Some((_, area)) = parse_sheet_and_box("", data_range) else {
        return true;
    };
    let rows = area.row1 - area.row0 + 1;
    let cols = area.col1 - area.col0 + 1;
    if rows == 1 || cols == 1 {
        return area.area() <= MAX_POINTS_PER_SERIES as u64;
    }
    let points = if switch_row_col { cols } else { rows };
    points <= MAX_POINTS_PER_SERIES as u32
}

pub fn ref_point_count(raw_ref: &str) -> Option<u64> {
    let (_, area) = parse_sheet_and_box("", raw_ref)?;
    Some(area.area())
}

fn split_sheet_bang(s: &str) -> Option<(&str, &str)> {
    let bytes = s.as_bytes();
    if bytes.first() == Some(&b'\'') {
        let mut i = 1usize;
        while i < bytes.len() {
            if bytes[i] == b'\'' {
                if bytes.get(i + 1) == Some(&b'\'') {
                    i += 2;
                    continue;
                }
                if bytes.get(i + 1) == Some(&b'!') {
                    return Some((&s[..=i], &s[i + 2..]));
                }
                return None;
            }
            i += 1;
        }
        None
    } else {
        s.split_once('!')
    }
}

fn unquote_sheet_name(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    if let Some(inner) = t.strip_prefix('\'') {
        let body = inner.strip_suffix('\'')?;
        let unescaped = body.replace("''", "'");
        (!unescaped.trim().is_empty()).then_some(unescaped)
    } else {
        Some(t.to_string())
    }
}

/// Parses `"Sheet1!$A$2:$A$5"`, `"'My Sheet'!A2:A5"`, or `"A2:A5"` into `(sheet_name, CellBox)`.
pub fn parse_sheet_and_box(default_sheet: &str, raw_ref: &str) -> Option<(String, CellBox)> {
    let trimmed = raw_ref.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (sheet, a1_part) = if let Some((prefix, rest)) = split_sheet_bang(trimmed) {
        (unquote_sheet_name(prefix)?, rest.trim())
    } else {
        (default_sheet.to_string(), trimmed)
    };
    let area = CellBox::parse(a1_part)?;
    Some((sheet, area))
}

/// Whether `name` reads as a cell in the R1C1 style: `R`, `C`, `RC`, `R1`, `C1`, `R1C1`.
fn is_r1c1_like(name: &str) -> bool {
    let bytes = name.as_bytes();
    let digits = |from: usize| {
        bytes[from..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count()
    };
    let mut end = 0;
    if matches!(bytes.first(), Some(b'R' | b'r')) {
        end = 1 + digits(1);
    }
    if matches!(bytes.get(end), Some(b'C' | b'c')) {
        end += 1 + digits(end + 1);
    }
    end > 0 && end == bytes.len()
}

/// Whether a sheet name needs quotes in front of `!`: anything but an ASCII identifier that cannot be read
/// as a cell (`A1`, `XFD10`, `R1C1`) or a boolean.
pub fn sheet_name_needs_quotes(name: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    let bytes = name.as_bytes();
    if !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_') {
        return true;
    }
    if !bytes
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        return true;
    }
    if name.eq_ignore_ascii_case("TRUE") || name.eq_ignore_ascii_case("FALSE") || is_r1c1_like(name)
    {
        return true;
    }
    let split = name.find(|c: char| c.is_ascii_digit());
    if let Some(idx) = split {
        let (letters, digits) = name.split_at(idx);
        if (1..=3).contains(&letters.len()) && digits.bytes().all(|b| b.is_ascii_digit()) {
            return true;
        }
    }
    false
}

pub fn format_sheet_range_ref(sheet_name: &str, range: CellBox) -> String {
    let mut out = String::with_capacity(sheet_name.len() + 20);
    if !sheet_name.is_empty() {
        if sheet_name_needs_quotes(sheet_name) {
            out.push('\'');
            for ch in sheet_name.chars() {
                if ch == '\'' {
                    out.push_str("''");
                } else {
                    out.push(ch);
                }
            }
            out.push('\'');
        } else {
            out.push_str(sheet_name);
        }
        out.push('!');
    }
    let _ = write!(
        out,
        "${}${}",
        format_col_letters(range.col0),
        range.row0 + 1
    );
    if range.row0 != range.row1 || range.col0 != range.col1 {
        let _ = write!(
            out,
            ":${}${}",
            format_col_letters(range.col1),
            range.row1 + 1
        );
    }
    out
}

/// Validates workbook and per-sheet chart budgets (`MAX_WORKBOOK_CHARTS`, `MAX_CHARTS_PER_SHEET`,
/// `MAX_SERIES_PER_CHART`, `MAX_POINTS_PER_SERIES`, `MAX_CHART_TEXT_BYTES`).
pub fn validate_workbook_charts(stylings: &[SheetStyling]) -> Result<(), EasyOfficeError> {
    let mut total = 0usize;
    for st in stylings {
        if st.charts.len() > MAX_CHARTS_PER_SHEET {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "시트 차트 수({})가 허용 상한({MAX_CHARTS_PER_SHEET})을 초과했습니다.",
                st.charts.len()
            )));
        }
        total += st.charts.len();
        if total > MAX_WORKBOOK_CHARTS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "통합 문서 전체 차트 수({total})가 허용 상한({MAX_WORKBOOK_CHARTS})을 초과했습니다."
            )));
        }
        for chart in &st.charts {
            if !chart.within_bounds() {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "차트 '{}'의 계열·포인트·텍스트 또는 크기 예산이 허용 상한을 초과했습니다.",
                    chart.id
                )));
            }
        }
    }
    Ok(())
}

/// Materialized values of one chart series for XLSX cache writing and PDF vector rendering.
#[derive(Debug, Clone, Default)]
pub struct ResolvedSeriesData {
    pub name: String,
    pub categories: Vec<String>,
    pub values: Vec<Option<f64>>,
    pub categories_numeric: bool,
}

/// O(1) per-sheet packed cell coordinate lookup table built in a single pass.
pub struct SheetCellIndex<'a> {
    sheets: Vec<(&'a str, FastU64Map<&'a str>)>,
}

impl<'a> SheetCellIndex<'a> {
    pub fn build(sheets: &'a [OdsUnpacked]) -> Self {
        let mut indexed = Vec::with_capacity(sheets.len());
        for (name, cells) in sheets {
            let mut map = FastU64Map::with_capacity_and_hasher(cells.len(), Default::default());
            for &(row, col, ref raw) in cells {
                map.insert(pack_cell_pos(row, col), raw.as_str());
            }
            indexed.push((name.as_str(), map));
        }
        Self { sheets: indexed }
    }

    #[inline]
    pub fn sheet_idx(&self, name: &str) -> Option<usize> {
        self.sheets
            .iter()
            .position(|(n, _)| n.eq_ignore_ascii_case(name))
    }

    #[inline]
    pub fn get(&self, sheet: &str, row: u32, col: u32) -> Option<&'a str> {
        let idx = self.sheet_idx(sheet)?;
        self.sheets[idx].1.get(&pack_cell_pos(row, col)).copied()
    }

    pub fn cell_display_text(
        &self,
        sheet: &str,
        row: u32,
        col: u32,
        caches: &[XlsxFormulaCache],
    ) -> Option<String> {
        let idx = self.sheet_idx(sheet)?;
        if let Some(cached) = caches.get(idx).and_then(|c| c.get(&(row, col))) {
            return Some(match cached {
                XlsxCachedValue::Number(n) if n.is_finite() => n.to_string(),
                XlsxCachedValue::Number(_) => String::new(),
                XlsxCachedValue::Text(t) => t.clone(),
                XlsxCachedValue::Boolean(b) => {
                    if *b {
                        "TRUE".to_string()
                    } else {
                        "FALSE".to_string()
                    }
                }
            });
        }
        self.sheets[idx]
            .1
            .get(&pack_cell_pos(row, col))
            .map(|&s| s.to_string())
    }

    pub fn cell_number(
        &self,
        sheet: &str,
        row: u32,
        col: u32,
        caches: &[XlsxFormulaCache],
    ) -> Option<f64> {
        let idx = self.sheet_idx(sheet)?;
        if let Some(cached) = caches.get(idx).and_then(|c| c.get(&(row, col))) {
            return match cached {
                XlsxCachedValue::Number(n) if n.is_finite() => Some(*n),
                _ => None,
            };
        }
        let raw = self.sheets[idx].1.get(&pack_cell_pos(row, col)).copied()?;
        parse_chart_number(raw)
    }

    pub fn resolve_series(
        &self,
        default_sheet: &str,
        caches: &[XlsxFormulaCache],
        series: &ChartSeriesRef,
    ) -> ResolvedSeriesData {
        let mut name = series.name.clone();
        if let Some(nref) = series.name_ref.as_deref() {
            if let Some((s, b)) = parse_sheet_and_box(default_sheet, nref) {
                if let Some(text) = self.cell_display_text(&s, b.row0, b.col0, caches) {
                    if !text.is_empty() {
                        name = text;
                    }
                }
            }
        }

        let mut categories = Vec::new();
        let mut categories_numeric = false;
        if let Some(cref) = series.categories_ref.as_deref() {
            if let Some((s, b)) = parse_sheet_and_box(default_sheet, cref) {
                let cap = b.area().min(MAX_POINTS_PER_SERIES as u64) as usize;
                categories.reserve(cap);
                let mut any_val = false;
                let mut all_num = true;
                'cat_loop: for r in b.row0..=b.row1 {
                    for c in b.col0..=b.col1 {
                        if categories.len() >= MAX_POINTS_PER_SERIES {
                            break 'cat_loop;
                        }
                        let text = self.cell_display_text(&s, r, c, caches).unwrap_or_default();
                        if !text.trim().is_empty() {
                            any_val = true;
                            if self.cell_number(&s, r, c, caches).is_none() {
                                all_num = false;
                            }
                        }
                        categories.push(text);
                    }
                }
                categories_numeric = any_val && all_num;
            }
        }

        let mut values = Vec::new();
        if let Some((s, b)) = parse_sheet_and_box(default_sheet, &series.values_ref) {
            let cap = b.area().min(MAX_POINTS_PER_SERIES as u64) as usize;
            values.reserve(cap);
            'val_loop: for r in b.row0..=b.row1 {
                for c in b.col0..=b.col1 {
                    if values.len() >= MAX_POINTS_PER_SERIES {
                        break 'val_loop;
                    }
                    values.push(self.cell_number(&s, r, c, caches));
                }
            }
        }

        ResolvedSeriesData {
            name,
            categories,
            values,
            categories_numeric,
        }
    }
}

pub fn parse_chart_number(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.starts_with('=') {
        return None;
    }
    if let Some(n) = numeric_cell_value(trimmed) {
        return Some(n);
    }
    if trimmed.contains(',') {
        let clean: String = trimmed.chars().filter(|&ch| ch != ',').collect();
        return clean.parse::<f64>().ok().filter(|n| n.is_finite());
    }
    if let Some(pct) = trimmed.strip_suffix('%') {
        let clean: String = pct.trim().chars().filter(|&ch| ch != ',').collect();
        return clean
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .map(|n| n / 100.0);
    }
    trimmed.parse::<f64>().ok().filter(|n| n.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheet_names_that_read_as_cells_are_quoted_in_chart_references() {
        let a1 = CellBox::parse("A2:B3").unwrap();
        for name in [
            "A1", "XFD10", "R1C1", "r1c1", "R", "C", "RC", "R12", "C3", "RC5", "R2C", "TRUE",
            "My Sheet", "매출",
        ] {
            let quoted = format_sheet_range_ref(name, a1);
            assert!(quoted.starts_with('\''), "{name}: {quoted}");
        }
        for name in ["Sheet1", "Report", "Rows", "Col", "Data_2026", "RCAA1"] {
            assert_eq!(
                format_sheet_range_ref(name, a1),
                format!("{name}!$A$2:$B$3")
            );
        }
    }
}

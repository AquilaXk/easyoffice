//! Workbook and worksheet metadata exposed to `CELL` and `INFO` formulas (PD-331).

use std::collections::BTreeMap;
use std::sync::Arc;

use easyoffice_formats::{
    resolve_code, sheet_name_needs_quotes, DefaultFont, HAlign, SheetStyling, DEFAULT_COL_PX,
    MAX_DIGIT_PX, STANDARD_DIGIT_PX,
};

use crate::axis::{shift_runs, shift_widths, Axis, AxisEdit};
use crate::memo::CellMap;
use crate::{EasyOfficeError, PackedCellCoordinate, SheetEngine, SheetLock, WorkbookEngine};

const DEFAULT_RELEASE: &str = "16.0";

/// Workbook-level file and runtime environment metadata for `CELL` and `INFO` (PD-331).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkbookMeta {
    /// Base file name without directory; empty means an unsaved workbook.
    pub file_name: String,
    /// Explicit directory prefix supplied by a caller or test; `None` exposes no host path.
    pub directory: Option<String>,
    /// Override for `INFO("release")`; `None` defaults to `"16.0"`.
    pub release: Option<String>,
    /// Override for `INFO("osversion")`; `None` uses the static target platform string.
    pub osversion: Option<String>,
}

impl WorkbookMeta {
    /// Release string reported by `INFO("release")`.
    #[inline]
    pub(crate) fn effective_release(&self) -> &str {
        self.release
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_RELEASE)
    }

    /// Operating-system version string reported by `INFO("osversion")`.
    #[inline]
    pub(crate) fn effective_osversion(&self) -> &str {
        if let Some(custom) = self
            .osversion
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return custom;
        }
        default_osversion()
    }

    /// Explicitly configured directory with its trailing path separator, or empty when unset.
    pub(crate) fn directory_with_sep(&self) -> String {
        let Some(raw) = self
            .directory
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            return String::new();
        };
        if raw.ends_with('/') || raw.ends_with('\\') {
            raw.to_string()
        } else if raw.contains('\\') {
            format!("{raw}\\")
        } else {
            format!("{raw}/")
        }
    }

    /// Value returned by `CELL("filename", [ref])`: `""` when unsaved, else `{dir}[{file}]{sheet}`.
    pub(crate) fn filename_for_sheet(&self, sheet_name: &str) -> String {
        let file = self.file_name.trim();
        if file.is_empty() {
            return String::new();
        }
        format!("{}[{}]{}", self.directory_with_sep(), file, sheet_name)
    }

    /// Formats a cross-sheet target reference for `CELL("address", Sheet2!B3)`.
    pub(crate) fn qualified_address(
        &self,
        sheet_name: &str,
        coord: PackedCellCoordinate,
    ) -> String {
        let col_label = PackedCellCoordinate::format_col_label(coord.col());
        let addr = format!("${col_label}${}", coord.row() + 1);
        let file = self.file_name.trim();
        if file.is_empty() {
            return format!("{}!{addr}", crate::axis::quote_sheet_name(sheet_name));
        }
        let file_needs_quotes = !file
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.');
        let needs_quotes = sheet_name_needs_quotes(sheet_name) || file_needs_quotes;
        if needs_quotes {
            let escaped_file = file.replace('\'', "''");
            let escaped_sheet = sheet_name.replace('\'', "''");
            format!("'[{escaped_file}]{escaped_sheet}'!{addr}")
        } else {
            format!("[{file}]{sheet_name}!{addr}")
        }
    }
}

#[inline]
const fn default_osversion() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "Macintosh (ARM64) 14.00"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "Macintosh (Intel) 14.00"
    }
    #[cfg(all(
        target_os = "macos",
        not(any(target_arch = "aarch64", target_arch = "x86_64"))
    ))]
    {
        "Macintosh 14.00"
    }
    #[cfg(target_os = "windows")]
    {
        "Windows (64-bit) NT 10.00"
    }
    #[cfg(target_os = "linux")]
    {
        "Linux"
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        "Unknown"
    }
}

#[inline]
fn run_contains(runs: &[(u32, u32)], target: u32) -> bool {
    let idx = runs.partition_point(|&(first, _)| first <= target);
    idx > 0 && runs[idx - 1].1 >= target
}

fn push_cell_run(runs: &mut Vec<(u32, u32, u32)>, row: u32, col: u32) {
    if let Some((r, _, last_c)) = runs.last_mut() {
        if *r == row && col <= last_c.saturating_add(1) {
            *last_c = (*last_c).max(col);
            return;
        }
    }
    runs.push((row, col, col));
}

/// Per-sheet cell formats, alignments, lock mask, column widths, and top-left scroll origin (PD-331).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetCellMeta {
    /// Non-zero number format ID per cell (`< 164` built-in, `>= 164` index into `custom_num_fmts`).
    pub num_fmts: CellMap<u16>,
    /// Custom number format codes keyed by `id - CUSTOM_BASE`.
    pub custom_num_fmts: Vec<String>,
    /// Non-General horizontal alignments per cell.
    pub h_aligns: CellMap<HAlign>,
    /// Cell lock status independent of whether sheet protection is active.
    pub lock: SheetLock,
    /// Explicit column widths in CSS pixels keyed by 0-based column index.
    pub col_widths_px: BTreeMap<u32, u32>,
    /// Default column width in CSS pixels (typically `72`).
    pub default_col_px: u32,
    /// Maximum digit width in CSS pixels of the workbook default font (`1..=64`, default `8`).
    pub digit_px: u32,
    /// Hidden columns as sorted, disjoint zero-based inclusive runs.
    pub hidden_cols: Vec<(u32, u32)>,
    /// Top-left visible scroll cell `(row, col)` (0-based) for `INFO("origin")`.
    pub origin: (u32, u32),
}

impl Default for SheetCellMeta {
    fn default() -> Self {
        Self {
            num_fmts: CellMap::default(),
            custom_num_fmts: Vec::new(),
            h_aligns: CellMap::default(),
            lock: SheetLock::default(),
            col_widths_px: BTreeMap::new(),
            default_col_px: DEFAULT_COL_PX,
            digit_px: STANDARD_DIGIT_PX,
            hidden_cols: Vec::new(),
            origin: (0, 0),
        }
    }
}

impl SheetCellMeta {
    /// Extracts compact evaluation metadata from a parsed [`SheetStyling`].
    pub fn from_styling(styling: &SheetStyling) -> Self {
        let mut num_fmts = CellMap::default();
        let mut h_aligns = CellMap::default();
        let mut unlocked_cells = Vec::new();
        let mut locked_cells = Vec::new();

        for (&(row, col), style) in &styling.cells {
            if let Ok(coord) = PackedCellCoordinate::new(row, col) {
                if style.num_fmt != 0 {
                    num_fmts.insert(coord, style.num_fmt);
                }
                if style.h_align != HAlign::General {
                    h_aligns.insert(coord, style.h_align);
                }
            }
            if style.unlocked {
                push_cell_run(&mut unlocked_cells, row, col);
            } else if style.force_locked
                || run_contains(&styling.unlocked_rows, row)
                || run_contains(&styling.unlocked_cols, col)
            {
                push_cell_run(&mut locked_cells, row, col);
            }
        }

        Self {
            num_fmts,
            custom_num_fmts: styling.num_fmts.as_ref().clone(),
            h_aligns,
            lock: SheetLock {
                unlocked_cells,
                locked_cells,
                unlocked_rows: styling.unlocked_rows.clone(),
                unlocked_cols: styling.unlocked_cols.clone(),
            },
            col_widths_px: styling.columns.clone(),
            default_col_px: styling.default_col_px(),
            digit_px: styling
                .default_font
                .as_ref()
                .map_or(STANDARD_DIGIT_PX, DefaultFont::digit_px)
                .clamp(1, MAX_DIGIT_PX),
            hidden_cols: styling.hidden_cols.clone(),
            origin: styling.view.top_left,
        }
    }

    /// Shifts cell metadata positions after a row or column insert/delete.
    pub(crate) fn shifted(&self, edit: &AxisEdit) -> Self {
        let map_cells = |src: &CellMap<u16>| -> CellMap<u16> {
            let mut out = CellMap::default();
            for (&coord, &val) in src {
                if let Some(moved) = edit.map_coord(coord) {
                    out.insert(moved, val);
                }
            }
            out
        };
        let mut h_aligns = CellMap::default();
        for (&coord, &val) in &self.h_aligns {
            if let Some(moved) = edit.map_coord(coord) {
                h_aligns.insert(moved, val);
            }
        }
        Self {
            num_fmts: map_cells(&self.num_fmts),
            custom_num_fmts: self.custom_num_fmts.clone(),
            h_aligns,
            lock: self.lock.shifted(edit),
            col_widths_px: shift_widths(&self.col_widths_px, edit),
            default_col_px: self.default_col_px,
            digit_px: self.digit_px,
            hidden_cols: if edit.axis == Axis::Col {
                shift_runs(&self.hidden_cols, edit)
            } else {
                self.hidden_cols.clone()
            },
            origin: self.origin,
        }
    }

    /// Resolves the active number format code for `coord`, or `None` for General.
    #[inline]
    pub(crate) fn resolve_format_code(&self, coord: PackedCellCoordinate) -> Option<&str> {
        let id = self.num_fmts.get(&coord).copied().filter(|&n| n != 0)?;
        resolve_code(id, &self.custom_num_fmts)
    }

    /// Horizontal alignment of `coord` (`General` when absent).
    #[inline]
    pub(crate) fn h_align(&self, coord: PackedCellCoordinate) -> HAlign {
        self.h_aligns
            .get(&coord)
            .copied()
            .unwrap_or(HAlign::General)
    }

    /// Whether `coord` is locked (`true` by default even when sheet protection is inactive).
    #[inline]
    pub(crate) fn is_locked(&self, coord: PackedCellCoordinate) -> bool {
        !self.lock.is_unlocked(coord.row(), coord.col())
    }

    /// Returns `(rounded_char_width, is_default)` for zero-based `col`.
    pub(crate) fn col_width_info(&self, col: u32) -> (u32, bool) {
        if run_contains(&self.hidden_cols, col) {
            return (0, false);
        }
        let (px, is_default) = match self.col_widths_px.get(&col).copied() {
            Some(0) => return (0, false),
            Some(explicit) => (explicit, false),
            None => (self.default_col_px, true),
        };
        let digit = self.digit_px.clamp(1, MAX_DIGIT_PX);
        let chars = if px < digit + 5 {
            f64::from(px) / f64::from(digit + 5)
        } else {
            f64::from(px - 5) / f64::from(digit)
        };
        (chars.round() as u32, is_default)
    }

    /// Origin string formatted for `INFO("origin")` (e.g. `"$A:$A$1"`).
    pub(crate) fn origin_text(&self) -> String {
        let col = PackedCellCoordinate::format_col_label(self.origin.1);
        format!("$A:${col}${}", self.origin.0 + 1)
    }
}

impl SheetEngine {
    /// Sets per-sheet metadata used by `CELL` and `INFO`. Returns `true` when it changed.
    pub fn set_cell_meta(&mut self, meta: Option<SheetCellMeta>) -> bool {
        let next = meta.map(Arc::new);
        if self.cell_meta == next {
            return false;
        }
        self.cell_meta = next;
        true
    }

    /// Current per-sheet `CELL`/`INFO` metadata, if set.
    pub fn cell_meta(&self) -> Option<&SheetCellMeta> {
        self.cell_meta.as_deref()
    }

    /// Sets workbook-level file and platform metadata on a standalone [`SheetEngine`].
    pub fn set_workbook_meta(&mut self, meta: WorkbookMeta) {
        self.workbook_meta = Arc::new(meta);
    }

    /// Workbook-level `CELL`/`INFO` metadata attached to this sheet engine.
    pub fn workbook_meta(&self) -> &WorkbookMeta {
        &self.workbook_meta
    }
}

impl WorkbookEngine {
    /// Attaches workbook-level `CELL`/`INFO` metadata and propagates it to all sheets.
    pub fn set_meta(&mut self, meta: WorkbookMeta) {
        let shared = Arc::new(meta);
        self.meta = Arc::clone(&shared);
        for sheet in self.sheets_mut() {
            sheet.workbook_meta = Arc::clone(&shared);
        }
    }

    /// Workbook-level `CELL`/`INFO` metadata.
    pub fn meta(&self) -> &WorkbookMeta {
        &self.meta
    }

    /// Sets per-sheet `CELL`/`INFO` metadata on `sheet_name`.
    pub fn set_sheet_cell_meta(
        &mut self,
        sheet_name: &str,
        meta: Option<SheetCellMeta>,
    ) -> Result<(), EasyOfficeError> {
        let sheet = self.sheet_mut(sheet_name).ok_or_else(|| {
            EasyOfficeError::InvalidArgument(format!("시트를 찾을 수 없습니다: {sheet_name}"))
        })?;
        sheet.set_cell_meta(meta);
        Ok(())
    }
}

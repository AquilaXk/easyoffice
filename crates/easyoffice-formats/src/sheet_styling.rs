//! Spreadsheet cell formatting and column widths carried next to the plain cell values (PD-089).
//!
//! The value model of a sheet stays `(row, col, raw)`; formatting travels beside it as
//! [`SheetStyling`] so every existing codec signature keeps working. XLSX (`styles.xml`, `s="N"`,
//! `<cols>`) and ODS (automatic cell/column styles) read and write it.

use std::collections::BTreeMap;
use std::sync::Arc;

// XLSX and ODS style tables live beside this module's model; they are internal to the crate.
#[path = "ods_grid.rs"]
pub(crate) mod ods_grid;
#[path = "ods_names.rs"]
pub(crate) mod ods_names;
#[path = "ods_numfmt.rs"]
pub(crate) mod ods_numfmt;
#[path = "ods_page.rs"]
pub(crate) mod ods_page;
#[path = "ods_protect.rs"]
pub(crate) mod ods_protect;
#[path = "ods_styles.rs"]
pub(crate) mod ods_styles;
#[path = "ods_view.rs"]
pub(crate) mod ods_view;
#[path = "sheet_filter.rs"]
pub(crate) mod sheet_filter;
#[path = "sheet_look.rs"]
pub(crate) mod sheet_look;
#[path = "sheet_names.rs"]
pub(crate) mod sheet_names;
#[path = "sheet_numfmt.rs"]
pub(crate) mod sheet_numfmt;
#[path = "sheet_page.rs"]
pub(crate) mod sheet_page;
#[path = "sheet_protect.rs"]
pub(crate) mod sheet_protect;
#[path = "sheet_rules.rs"]
pub(crate) mod sheet_rules;
#[path = "sheet_view.rs"]
pub(crate) mod sheet_view;
#[path = "xlsx_filter.rs"]
pub(crate) mod xlsx_filter;
#[path = "xlsx_layout.rs"]
pub(crate) mod xlsx_layout;
#[path = "xlsx_names.rs"]
pub(crate) mod xlsx_names;
#[path = "xlsx_page.rs"]
pub(crate) mod xlsx_page;
#[path = "xlsx_protect.rs"]
pub(crate) mod xlsx_protect;
#[path = "xlsx_sheet.rs"]
pub(crate) mod xlsx_sheet;
#[path = "xlsx_styles.rs"]
pub(crate) mod xlsx_styles;
#[path = "xlsx_view.rs"]
pub(crate) mod xlsx_view;

pub use sheet_filter::*;
pub use sheet_look::{
    parse_hex_rgb, BorderEdge, BorderLine, CellBorders, HAlign, VAlign, BORDER_DEFAULT_RGB,
};
pub use sheet_names::*;
pub use sheet_numfmt::*;
pub use sheet_page::*;
pub use sheet_protect::*;
pub use sheet_rules::*;
pub use sheet_view::*;
pub use xlsx_layout::{intersect_runs, normalize_runs, run_total, MAX_HIDDEN_ROWS, MAX_MERGES};

use crate::{DocumentFormat, EasyOfficeError, OdfCodec, OoxmlCodec};

/// Largest number of styled cells or sized columns one sheet may carry.
pub const MAX_STYLED_CELLS: usize = 500_000;
/// Smallest and largest column width (CSS px) the editor and the writers accept.
pub const MIN_COLUMN_PX: u32 = 8;
pub const MAX_COLUMN_PX: u32 = 2_000;
/// Spreadsheet programs limit a font size to 1..=409 pt.
pub const MIN_SIZE_PT: f64 = 1.0;
pub const MAX_SIZE_PT: f64 = 409.0;

pub const STYLE_BOLD: u8 = 1 << 0;
pub const STYLE_ITALIC: u8 = 1 << 1;
pub const STYLE_UNDERLINE: u8 = 1 << 2;
pub const STYLE_STRIKE: u8 = 1 << 3;
const NUM_FMT_SHIFT: u32 = 8;
const FLAGS_MASK: u64 = 0x0F;
const H_ALIGN_SHIFT: u32 = 4;
const V_ALIGN_SHIFT: u32 = 6;
const WRAP_BIT: u64 = 1 << 24;
const UNLOCKED_BIT: u64 = 1 << 25;
const FORMULA_HIDDEN_BIT: u64 = 1 << 26;
const FORCE_LOCKED_BIT: u64 = 1 << 27;
const SIZE_SHIFT: u32 = 40;

/// Formatting of one cell: the flags, font size, number format, alignment, wrapping and borders
/// the sheet editor can set (PD-084, PD-193, PD-207).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CellStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// Font size in points; `None` keeps the workbook default.
    pub size_pt: Option<f64>,
    /// Number format id: `0` General, a built-in id, or `CUSTOM_BASE + n` into the sheet's
    /// `num_fmts` (see [`sheet_numfmt`]).
    pub num_fmt: u16,
    pub h_align: HAlign,
    pub v_align: VAlign,
    /// Long text breaks into lines inside the cell.
    pub wrap: bool,
    pub borders: CellBorders,
    /// The cell may be changed on a protected sheet (cells are locked by default, PD-216).
    pub unlocked: bool,
    /// The cell is locked inside an unlocked row or column (a locked cell with no format otherwise carries
    /// none, which would read as the row's or column's lock).
    pub force_locked: bool,
    /// A protected sheet does not show the formula of the cell in the formula bar (PD-216).
    pub formula_hidden: bool,
}

/// Style identity for $O(1)$ word comparison and hashing: one packed 64-bit word
/// (`size_centipt << 40 | locked << 27 | hidden << 26 | unlocked << 25 | wrap << 24 | num_fmt << 8 | v_align << 6 | h_align << 4 | flags`, the
/// size compared in hundredths of a point) and the four border words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) struct StyleKey {
    bits: u64,
    borders: CellBorders,
}

impl StyleKey {
    #[inline]
    pub(crate) const fn flags(self) -> u8 {
        (self.bits & FLAGS_MASK) as u8
    }

    #[inline]
    pub(crate) const fn h_align(self) -> HAlign {
        match (self.bits >> H_ALIGN_SHIFT) & 3 {
            1 => HAlign::Left,
            2 => HAlign::Center,
            3 => HAlign::Right,
            _ => HAlign::General,
        }
    }

    #[inline]
    pub(crate) const fn v_align(self) -> VAlign {
        match (self.bits >> V_ALIGN_SHIFT) & 3 {
            1 => VAlign::Top,
            2 => VAlign::Center,
            _ => VAlign::Bottom,
        }
    }

    #[inline]
    pub(crate) const fn wrap(self) -> bool {
        self.bits & WRAP_BIT != 0
    }

    #[inline]
    pub(crate) const fn borders(self) -> CellBorders {
        self.borders
    }

    #[inline]
    pub(crate) const fn unlocked(self) -> bool {
        self.bits & UNLOCKED_BIT != 0
    }

    #[inline]
    pub(crate) const fn force_locked(self) -> bool {
        self.bits & FORCE_LOCKED_BIT != 0
    }

    #[inline]
    pub(crate) const fn formula_hidden(self) -> bool {
        self.bits & FORMULA_HIDDEN_BIT != 0
    }

    #[inline]
    pub(crate) const fn bold(self) -> bool {
        (self.flags() & STYLE_BOLD) != 0
    }

    #[inline]
    pub(crate) const fn italic(self) -> bool {
        (self.flags() & STYLE_ITALIC) != 0
    }

    #[inline]
    pub(crate) const fn underline(self) -> bool {
        (self.flags() & STYLE_UNDERLINE) != 0
    }

    #[inline]
    pub(crate) const fn strike(self) -> bool {
        (self.flags() & STYLE_STRIKE) != 0
    }

    #[inline]
    pub(crate) const fn num_fmt(self) -> u16 {
        ((self.bits >> NUM_FMT_SHIFT) & 0xFFFF) as u16
    }

    /// The key with the number format id replaced (workbook-wide ids differ from a sheet's).
    #[inline]
    pub(crate) const fn with_num_fmt(self, id: u16) -> Self {
        Self {
            bits: (self.bits & !(0xFFFF << NUM_FMT_SHIFT)) | ((id as u64) << NUM_FMT_SHIFT),
            borders: self.borders,
        }
    }

    /// The key reduced to what the font element of a style depends on: the flags and the size.
    #[inline]
    pub(crate) const fn font_part(self) -> Self {
        Self {
            bits: self.bits & (FLAGS_MASK | (!0u64 << SIZE_SHIFT)),
            borders: CellBorders {
                top: BorderEdge::NONE,
                right: BorderEdge::NONE,
                bottom: BorderEdge::NONE,
                left: BorderEdge::NONE,
            },
        }
    }

    #[inline]
    pub(crate) const fn size_centipt(self) -> Option<u32> {
        let centi = (self.bits >> SIZE_SHIFT) as u32;
        if centi == 0 {
            None
        } else {
            Some(centi)
        }
    }
}

impl CellStyle {
    #[inline]
    pub const fn flags(&self) -> u8 {
        (if self.bold { STYLE_BOLD } else { 0 })
            | (if self.italic { STYLE_ITALIC } else { 0 })
            | (if self.underline { STYLE_UNDERLINE } else { 0 })
            | (if self.strike { STYLE_STRIKE } else { 0 })
    }

    /// A style that changes nothing relative to the default cell.
    #[inline]
    pub fn is_plain(&self) -> bool {
        self.flags() == 0
            && self.size_pt.is_none()
            && self.num_fmt == 0
            && self.h_align == HAlign::General
            && self.v_align == VAlign::Bottom
            && !self.wrap
            && self.borders.is_none()
            && !self.unlocked
            && !self.force_locked
            && !self.formula_hidden
    }

    #[inline]
    pub(crate) fn key(&self) -> StyleKey {
        let centi = self
            .size_pt
            .map_or(0u64, |s| ((s * 100.0).round() as u32) as u64);
        StyleKey {
            bits: (centi << SIZE_SHIFT)
                | (u64::from(self.force_locked) * FORCE_LOCKED_BIT)
                | (u64::from(self.formula_hidden) * FORMULA_HIDDEN_BIT)
                | (u64::from(self.unlocked) * UNLOCKED_BIT)
                | (u64::from(self.wrap) * WRAP_BIT)
                | (u64::from(self.num_fmt) << NUM_FMT_SHIFT)
                | ((self.v_align as u64) << V_ALIGN_SHIFT)
                | ((self.h_align as u64) << H_ALIGN_SHIFT)
                | u64::from(self.flags()),
            borders: self.borders,
        }
    }
}

/// Formatting of one sheet: styled cells by zero-based `(row, col)` and custom column widths in
/// CSS pixels by zero-based column.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SheetStyling {
    pub cells: BTreeMap<(u32, u32), CellStyle>,
    pub columns: BTreeMap<u32, u32>,
    /// Conditional formatting and data validation (PD-171).
    pub rules: SheetRules,
    /// Custom number format codes: the cell id `CUSTOM_BASE + n` names entry `n` (PD-193). A
    /// workbook's sheets share one table.
    pub num_fmts: Arc<Vec<String>>,
    /// Number formats a reader replaced with General because they broke a bound (PD-193).
    pub num_fmts_rejected: u32,
    /// Merged ranges (each more than one cell).
    pub merges: Vec<CellBox>,
    /// Hidden rows and columns as sorted, disjoint, zero-based inclusive runs.
    pub hidden_rows: Vec<(u32, u32)>,
    pub hidden_cols: Vec<(u32, u32)>,
    /// The auto filter (PD-198). The rows it hides are `filter.hidden`, apart from `hidden_rows`.
    pub filter: Option<SheetFilter>,
    /// Frozen rows and columns (PD-208).
    pub freeze: Option<Freeze>,
    /// Colour of the sheet's tab as `0xRRGGBB` (PD-208).
    pub tab_color: Option<u32>,
    /// The sheet is hidden: it has no tab until the user shows it again (PD-209).
    pub hidden: bool,
    /// Row and column groups and the groups the user folded (PD-208).
    pub outline: Outline,
    /// Paper, margins, scale, print area and repeated rows and columns for printing (PD-213).
    pub page: SheetPage,
    /// The defined names of the workbook (PD-214), carried by the first sheet like the number format
    /// count; the other sheets keep it empty.
    pub defined_names: Vec<DefinedName>,
    /// Names a reader left out because the workbook had more than [`MAX_NAMES`] (PD-214).
    pub names_rejected: u32,
    /// The protection of the sheet (PD-216); `None` for a sheet that has none.
    pub protect: Option<SheetProtection>,
    /// Rows and columns whose own format is unlocked, as sorted, disjoint, zero-based inclusive runs: a
    /// cell of them with no format of its own is unlocked (PD-216).
    pub unlocked_rows: Vec<(u32, u32)>,
    pub unlocked_cols: Vec<(u32, u32)>,
    /// The protection of the workbook structure (PD-216), carried by the first sheet like the names.
    pub workbook_protect: Option<WorkbookProtection>,
}

impl SheetStyling {
    /// `stylings` with the first sheet shown (see [`hides_every_sheet`]).
    pub(crate) fn with_first_shown(stylings: &[SheetStyling]) -> Vec<SheetStyling> {
        let mut shown = stylings.to_vec();
        if let Some(first) = shown.first_mut() {
            first.hidden = false;
        }
        shown
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
            && self.columns.is_empty()
            && self.rules.is_empty()
            && self.merges.is_empty()
            && self.hidden_rows.is_empty()
            && self.hidden_cols.is_empty()
            && self.filter.is_none()
            && self.freeze.is_none()
            && self.tab_color.is_none()
            && !self.hidden
            && self.outline.is_empty()
            && self.page.is_default()
            && self.defined_names.is_empty()
            && self.protect.is_none()
            && self.unlocked_rows.is_empty()
            && self.unlocked_cols.is_empty()
            && self.workbook_protect.is_none()
    }

    /// Whether the frozen pane, the tab colour, the groups and the page setup are within the bounds of a sheet that
    /// has `max_row` rows and `max_col` columns.
    pub fn view_within_bounds(&self, max_row: u32, max_col: u32) -> bool {
        self.freeze
            .is_none_or(|f| Freeze::new(f.rows, f.cols).is_some())
            && self.tab_color.is_none_or(|rgb| rgb <= 0xFF_FFFF)
            && self.outline.within_bounds(max_row, max_col)
            && self.page.within_bounds(max_row, max_col)
            && self
                .protect
                .as_ref()
                .is_none_or(SheetProtection::within_bounds)
            && self
                .workbook_protect
                .as_ref()
                .is_none_or(WorkbookProtection::within_bounds)
            && self.unlocked_rows.last().is_none_or(|r| r.1 < max_row)
            && self.unlocked_cols.last().is_none_or(|c| c.1 < max_col)
            && run_total(&self.unlocked_rows) <= MAX_HIDDEN_ROWS
    }

    /// Drops the "kept locked" mark of cells that no unlocked row or column covers: they are locked anyway and
    /// a file writes the mark on every cell format that names `locked="1"` (PD-216).
    pub(crate) fn normalize_locks(&mut self) {
        let (rows, cols) = (&self.unlocked_rows, &self.unlocked_cols);
        for (&(row, col), style) in &mut self.cells {
            let covered =
                xlsx_layout::run_contains(rows, row) || xlsx_layout::run_contains(cols, col);
            if style.force_locked && !covered {
                style.force_locked = false;
            }
        }
        self.cells.retain(|_, style| !style.is_plain());
    }

    /// Whether the cell at `(row, col)` may be changed on a protected sheet: its own format says unlocked,
    /// or it has none and its row or column is unlocked (PD-216).
    pub fn cell_unlocked(&self, row: u32, col: u32) -> bool {
        match self.cells.get(&(row, col)) {
            Some(style) => style.unlocked,
            None => {
                xlsx_layout::run_contains(&self.unlocked_rows, row)
                    || xlsx_layout::run_contains(&self.unlocked_cols, col)
            }
        }
    }

    /// Whether the user, a folded group or the filter hides `row`.
    pub fn row_hidden(&self, row: u32) -> bool {
        xlsx_layout::run_contains(&self.hidden_rows, row)
            || xlsx_layout::run_contains(&self.outline.collapsed_rows, row)
            || self.filter.as_ref().is_some_and(|f| f.hides(row))
    }
}

/// Whether `stylings` hide every one of the `count` sheets of a workbook. A workbook shows at
/// least one sheet, so a writer shows the first one and a reader does the same (PD-209); the page
/// never sends or keeps such a state, a file made by another program might have it.
pub(crate) fn hides_every_sheet(stylings: &[SheetStyling], count: usize) -> bool {
    count > 0 && stylings.len() >= count && stylings[..count].iter().all(|s| s.hidden)
}

/// Formats a size in hundredths of a point without trailing zeros (`1125` -> `11.25`).
pub(crate) fn format_centipt(centi: u32) -> String {
    let text = format!("{}.{:02}", centi / 100, centi % 100);
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Reads the formatting of every sheet of an XLSX or ODS file, keyed by sheet name. Other
/// formats carry none.
pub fn read_sheet_styling(
    bytes: &[u8],
    format: DocumentFormat,
) -> Result<Vec<(String, SheetStyling)>, EasyOfficeError> {
    match format {
        DocumentFormat::Xlsx => OoxmlCodec::unpack_xlsx_styling(bytes),
        DocumentFormat::Ods => OdfCodec::unpack_ods_styling(bytes),
        _ => Ok(Vec::new()),
    }
}

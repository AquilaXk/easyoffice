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
#[path = "ods_numfmt.rs"]
pub(crate) mod ods_numfmt;
#[path = "ods_styles.rs"]
pub(crate) mod ods_styles;
#[path = "ods_view.rs"]
pub(crate) mod ods_view;
#[path = "sheet_filter.rs"]
pub(crate) mod sheet_filter;
#[path = "sheet_look.rs"]
pub(crate) mod sheet_look;
#[path = "sheet_numfmt.rs"]
pub(crate) mod sheet_numfmt;
#[path = "sheet_rules.rs"]
pub(crate) mod sheet_rules;
#[path = "sheet_view.rs"]
pub(crate) mod sheet_view;
#[path = "xlsx_filter.rs"]
pub(crate) mod xlsx_filter;
#[path = "xlsx_layout.rs"]
pub(crate) mod xlsx_layout;
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
pub use sheet_numfmt::*;
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
}

/// Style identity for $O(1)$ word comparison and hashing: one packed 64-bit word
/// (`size_centipt << 40 | wrap << 24 | num_fmt << 8 | v_align << 6 | h_align << 4 | flags`, the
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
    }

    #[inline]
    pub(crate) fn key(&self) -> StyleKey {
        let centi = self
            .size_pt
            .map_or(0u64, |s| ((s * 100.0).round() as u32) as u64);
        StyleKey {
            bits: (centi << SIZE_SHIFT)
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
    /// Row and column groups and the groups the user folded (PD-208).
    pub outline: Outline,
}

impl SheetStyling {
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
            && self.outline.is_empty()
    }

    /// Whether the frozen pane, the tab colour and the groups are within the bounds of a sheet that
    /// has `max_row` rows and `max_col` columns.
    pub fn view_within_bounds(&self, max_row: u32, max_col: u32) -> bool {
        self.freeze
            .is_none_or(|f| Freeze::new(f.rows, f.cols).is_some())
            && self.tab_color.is_none_or(|rgb| rgb <= 0xFF_FFFF)
            && self.outline.within_bounds(max_row, max_col)
    }

    /// Whether the user, a folded group or the filter hides `row`.
    pub fn row_hidden(&self, row: u32) -> bool {
        xlsx_layout::run_contains(&self.hidden_rows, row)
            || xlsx_layout::run_contains(&self.outline.collapsed_rows, row)
            || self.filter.as_ref().is_some_and(|f| f.hides(row))
    }
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

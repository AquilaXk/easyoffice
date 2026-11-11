//! Spreadsheet cell formatting and column widths carried next to the plain cell values (PD-089).
//!
//! The value model of a sheet stays `(row, col, raw)`; formatting travels beside it as
//! [`SheetStyling`] so every existing codec signature keeps working. XLSX (`styles.xml`, `s="N"`,
//! `<cols>`) and ODS (automatic cell/column styles) read and write it.

use std::collections::BTreeMap;

// XLSX and ODS style tables live beside this module's model; they are internal to the crate.
#[path = "ods_grid.rs"]
pub(crate) mod ods_grid;
#[path = "ods_styles.rs"]
pub(crate) mod ods_styles;
#[path = "sheet_rules.rs"]
pub(crate) mod sheet_rules;
#[path = "xlsx_sheet.rs"]
pub(crate) mod xlsx_sheet;
#[path = "xlsx_styles.rs"]
pub(crate) mod xlsx_styles;

pub use sheet_rules::*;

use crate::{DocumentFormat, EasyOfficeError, OdfCodec, OoxmlCodec};

/// Largest number of styled cells or sized columns one sheet may carry.
pub const MAX_STYLED_CELLS: usize = 500_000;
/// Smallest and largest column width (CSS px) the editor and the writers accept.
pub const MIN_COLUMN_PX: u32 = 8;
pub const MAX_COLUMN_PX: u32 = 2_000;
/// Excel limits a font size to 1..=409 pt.
pub const MIN_SIZE_PT: f64 = 1.0;
pub const MAX_SIZE_PT: f64 = 409.0;

pub const STYLE_BOLD: u8 = 1 << 0;
pub const STYLE_ITALIC: u8 = 1 << 1;
pub const STYLE_UNDERLINE: u8 = 1 << 2;
pub const STYLE_STRIKE: u8 = 1 << 3;

/// Formatting of one cell: the flags and font size the sheet editor can set (PD-084).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CellStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// Font size in points; `None` keeps the workbook default.
    pub size_pt: Option<f64>,
}

/// Packed 64-bit style identity (`(centipt_or_zero as u64) << 40 | u64::from(flags)`) for $O(1)$
/// word comparison and hashing; the size is compared in hundredths of a point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) struct StyleKey(pub(crate) u64);

impl StyleKey {
    #[inline]
    pub(crate) const fn flags(self) -> u8 {
        (self.0 & 0xFF) as u8
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
    pub(crate) const fn size_centipt(self) -> Option<u32> {
        let centi = (self.0 >> 40) as u32;
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
        self.flags() == 0 && self.size_pt.is_none()
    }

    #[inline]
    pub(crate) fn key(&self) -> StyleKey {
        let centi = self
            .size_pt
            .map_or(0u64, |s| ((s * 100.0).round() as u32) as u64);
        StyleKey((centi << 40) | u64::from(self.flags()))
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
}

impl SheetStyling {
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty() && self.columns.is_empty() && self.rules.is_empty()
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

//! Row heights of a worksheet (PD-278): the height of a row and whether a user set it (`customHeight` in
//! XLSX, `style:use-optimal-row-height="false"` in ODS), kept beside the cell values in [`SheetStyling`].
//!
//! 참조 구현: Univer `packages/sheets/src/commands/mutations/set-worksheet-row-height.mutation.ts`
//! (`SetWorksheetRowHeightMutation`, `SetWorksheetRowIsAutoHeightMutation`: a row has a height `h` and a flag
//! `ia` that says the height follows the content), Apache-2.0, Copyright 2023-present DreamNum Co., Ltd.,
//! commit 4767b3b309467c283bbd52b725536ca73861e0fc. The reference stores no file format; the file side
//! (`<row ht customHeight>`, `style:row-height`, `style:use-optimal-row-height`) follows ECMA-376 18.3.1.73
//! and ODF 1.2 section 17.17, and the 409 pt ceiling is the one the spreadsheet programs keep.
//!
//! The default row height of a sheet (PD-286: `<sheetFormatPr defaultRowHeight customHeight>`, the row style of the band
//! that ends an ODS table) is kept in `SheetStyling::default_row_height`. 참조 구현: IronCalc
//! `xlsx/src/import/worksheets.rs` lines 255-270 (`get_default_column_width`: the one `sheetFormatPr` of the sheet, an
//! attribute that is not a number falls back to the program's default), Apache-2.0 or MIT, Copyright (c) IronCalc
//! contributors, commit 87b684bb04a87c63bdd6343c29036450b1cfd358. The reference reads the column width only; the row
//! side (`customHeight`, the range check, the ODS band) follows ECMA-376 18.3.1.81 and ODF 1.2 section 17.17. No line is
//! copied.

use crate::sheet_styling::format_centipt;
use crate::xml_stream::Attrs;
use crate::SheetStyling;
use std::collections::BTreeMap;
use std::fmt::Write;

/// The tallest row, in hundredths of a point (409 pt).
pub const MAX_ROW_HEIGHT_CENTIPT: u32 = 40_900;
/// Rows with a height one sheet may carry.
pub const MAX_ROW_HEIGHTS: usize = 100_000;
/// Longest text a default row height may be written as; a longer value is not a height.
const MAX_HEIGHT_TEXT: usize = 32;
/// The height the page draws a row at when a sheet has no default row height of a user, in CSS pixels (`sheet-grid.js`
/// `ROW_H`).
pub const GRID_ROW_PX: f64 = 24.0;

const PX_PER_PT: f64 = 4.0 / 3.0;

/// The height of one row. `custom` is true when a user set it; a height that follows the content is kept as the
/// file gave it, so a file that is read and written again keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RowHeight {
    /// Hundredths of a point, `1..=`[`MAX_ROW_HEIGHT_CENTIPT`].
    pub centipt: u32,
    pub custom: bool,
}

/// Zero-based row to its height.
pub type RowHeights = BTreeMap<u32, RowHeight>;

impl RowHeight {
    /// The height of `points`, rounded to hundredths; none for a value that is not finite, is not above 0 (a row of
    /// height 0 is a hidden row, which has its own flag) or is above 409 pt.
    pub fn from_points(points: f64, custom: bool) -> Option<Self> {
        if !points.is_finite()
            || points <= 0.0
            || points > f64::from(MAX_ROW_HEIGHT_CENTIPT) / 100.0
        {
            return None;
        }
        let centipt = (points * 100.0).round() as u32;
        (centipt > 0).then_some(Self { centipt, custom })
    }

    /// The height from a text (`"15.75"`), none for anything [`Self::from_points`] refuses.
    pub fn parse(text: &str, custom: bool) -> Option<Self> {
        Self::from_points(text.trim().parse().ok()?, custom)
    }

    /// The default row height of an attribute pair (`defaultRowHeight`, `customHeight`): none for a missing value, a
    /// value that is not a height or one longer than any height is written.
    pub(crate) fn parse_default(height: Option<&str>, custom: Option<&str>) -> Option<Self> {
        let text = height.filter(|t| t.len() <= MAX_HEIGHT_TEXT)?;
        Self::parse(text, matches!(custom.map(str::trim), Some("1" | "true")))
    }

    /// The height in points as text without trailing zeros (`1575` -> `15.75`).
    pub fn points_text(self) -> String {
        format_centipt(self.centipt)
    }

    /// The height in CSS pixels.
    pub fn px(self) -> f64 {
        f64::from(self.centipt) / 100.0 * PX_PER_PT
    }
}

/// Whether `heights` and the sheet's default row height fit the model of a sheet with `max_row` rows: at most
/// [`MAX_ROW_HEIGHTS`] rows, all inside the sheet, every height in range.
pub fn row_heights_within_bounds(
    heights: &RowHeights,
    default: Option<RowHeight>,
    max_row: u32,
) -> bool {
    let in_range = |h: &RowHeight| (1..=MAX_ROW_HEIGHT_CENTIPT).contains(&h.centipt);
    heights.len() <= MAX_ROW_HEIGHTS
        && heights
            .last_key_value()
            .is_none_or(|(&row, _)| row < max_row)
        && heights.values().all(in_range)
        && default.as_ref().is_none_or(in_range)
}

impl SheetStyling {
    /// The default row height a user set. One that follows the font (`customHeight` off) is the program's own measure of
    /// a row: the page keeps its grid row height for it (PD-286) and an ODS file does not carry it (PD-300).
    pub fn user_default_row_height(&self) -> Option<RowHeight> {
        self.default_row_height.filter(|h| h.custom)
    }

    /// The height in CSS pixels of the visible `row` as the page draws it: the height of the row, else the default row
    /// height a user set, else `grid_px`.
    pub fn row_height_px(&self, row: u32, grid_px: f64) -> f64 {
        self.row_heights
            .get(&row)
            .map_or_else(|| self.base_row_px(grid_px), |h| h.px())
    }

    /// The height in CSS pixels of a row with no height of its own: the default row height a user set, else `grid_px`.
    pub fn base_row_px(&self, grid_px: f64) -> f64 {
        self.user_default_row_height()
            .map_or(grid_px, RowHeight::px)
    }

    /// The height `row` takes on the page and in the PDF (PD-287, PD-299): none for a hidden row, else
    /// [`Self::row_height_px`]. The one rule the page, the PDF and the position of pictures and charts share.
    pub fn shown_row_px(&self, row: u32, grid_px: f64) -> f64 {
        if self.row_hidden(row) {
            0.0
        } else {
            self.row_height_px(row, grid_px)
        }
    }

    /// Records the height of the `row` element `attrs` belong to (`ht`, `customHeight`). A value that is not a finite
    /// height of 0..=409 pt is ignored, and the sheet takes no more than [`MAX_ROW_HEIGHTS`] rows.
    pub(crate) fn record_xlsx_row_height(&mut self, attrs: &Attrs<'_>, row: u32) {
        if self.row_heights.len() >= MAX_ROW_HEIGHTS {
            return;
        }
        let (mut ht, mut custom) = (None, false);
        for (name, value) in attrs.iter_raw() {
            match name.strip_prefix("eo:").unwrap_or(name) {
                "ht" => ht = ht.or(Some(value)),
                "customHeight" => custom = matches!(value.trim(), "1" | "true"),
                _ => {}
            }
        }
        if let Some(height) = ht.and_then(|text| RowHeight::parse(text, custom)) {
            self.row_heights.insert(row, height);
        }
    }
}

/// ` ht=".." customHeight="1"` of `row`'s `<row>` start tag, nothing for a row with no height.
pub(crate) fn xlsx_row_attrs(heights: &RowHeights, row: u32, xml: &mut String) {
    let Some(height) = heights.get(&row) else {
        return;
    };
    let _ = write!(xml, " ht=\"{}\"", height.points_text());
    if height.custom {
        xml.push_str(" customHeight=\"1\"");
    }
}

/// The height a `style:table-row-properties` element sets. It is `custom` when a user set it
/// (`style:use-optimal-row-height="false"`); a row that follows its content has no height of its own, the program that
/// reads the file measures it, so only the band that ends a table keeps such a height (as the sheet's default).
pub(crate) fn read_ods_row_properties(
    props: &str,
    length_to_pt: impl Fn(&str) -> Option<f64>,
) -> Option<RowHeight> {
    use crate::rtf_xml_helpers::extract_attr_from_fragment as attr;
    let custom = attr(props, "style:use-optimal-row-height") == Some("false");
    let text = attr(props, "style:row-height").filter(|t| t.len() <= MAX_HEIGHT_TEXT)?;
    RowHeight::from_points(length_to_pt(text)?, custom)
}

/// The automatic style `ro<index>` of one distinct row height: `style:row-height` in points and whether the row follows its content.
pub(crate) fn ods_row_style_xml(index: usize, height: RowHeight) -> String {
    format!(
        "<style:style style:name=\"ro{index}\" style:family=\"table-row\"><style:table-row-properties style:row-height=\"{}pt\" style:use-optimal-row-height=\"{}\"/></style:style>",
        height.points_text(),
        !height.custom
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heights_are_finite_points_up_to_409() {
        assert_eq!(RowHeight::parse("15.75", true).unwrap().centipt, 1575);
        assert_eq!(RowHeight::parse(" 409 ", false).unwrap().centipt, 40_900);
        for bad in [
            "0", "-1", "409.01", "NaN", "inf", "1e400", "", "abc", "0.004",
        ] {
            assert!(RowHeight::parse(bad, true).is_none(), "{bad}");
        }
        assert_eq!(
            RowHeight::parse("14.4", true).unwrap().points_text(),
            "14.4"
        );
        assert!((RowHeight::parse("30", true).unwrap().px() - 40.0).abs() < 1e-9);
    }

    #[test]
    fn bounds_check_count_rows_and_values() {
        let one = RowHeight::parse("20", true).unwrap();
        let mut heights = RowHeights::from([(5, one)]);
        assert!(row_heights_within_bounds(&heights, None, 6));
        assert!(!row_heights_within_bounds(&heights, None, 5));
        heights.insert(
            1,
            RowHeight {
                centipt: 0,
                custom: true,
            },
        );
        assert!(!row_heights_within_bounds(&heights, None, 6));
        heights.insert(
            1,
            RowHeight {
                centipt: MAX_ROW_HEIGHT_CENTIPT + 1,
                custom: true,
            },
        );
        assert!(!row_heights_within_bounds(&heights, None, 6));
        let many: RowHeights = (0..=MAX_ROW_HEIGHTS as u32).map(|r| (r, one)).collect();
        assert!(!row_heights_within_bounds(&many, None, u32::MAX));
    }
}

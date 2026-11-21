//! Row heights of a worksheet (PD-278): the height of a row and whether a user set it (`customHeight` in
//! XLSX, `style:use-optimal-row-height="false"` in ODS), kept beside the cell values in [`SheetStyling`].
//!
//! 참조 구현: Univer `packages/sheets/src/commands/mutations/set-worksheet-row-height.mutation.ts`
//! (`SetWorksheetRowHeightMutation`, `SetWorksheetRowIsAutoHeightMutation`: a row has a height `h` and a flag
//! `ia` that says the height follows the content), Apache-2.0, Copyright 2023-present DreamNum Co., Ltd.,
//! commit 4767b3b309467c283bbd52b725536ca73861e0fc. The reference stores no file format; the file side
//! (`<row ht customHeight>`, `style:row-height`, `style:use-optimal-row-height`) follows ECMA-376 18.3.1.73
//! and ODF 1.2 section 17.17, and the 409 pt ceiling is the one the spreadsheet programs keep.

use crate::sheet_styling::format_centipt;
use crate::xml_stream::Attrs;
use crate::SheetStyling;
use std::collections::BTreeMap;
use std::fmt::Write;

/// The tallest row, in hundredths of a point (409 pt).
pub const MAX_ROW_HEIGHT_CENTIPT: u32 = 40_900;
/// Rows with a height one sheet may carry.
pub const MAX_ROW_HEIGHTS: usize = 100_000;

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

    /// The height in points as text without trailing zeros (`1575` -> `15.75`).
    pub fn points_text(self) -> String {
        format_centipt(self.centipt)
    }

    /// The height in CSS pixels.
    pub fn px(self) -> f64 {
        f64::from(self.centipt) / 100.0 * PX_PER_PT
    }
}

/// Whether `heights` fit the model of a sheet with `max_row` rows: at most [`MAX_ROW_HEIGHTS`] rows, all inside the sheet,
/// every height in range.
pub fn row_heights_within_bounds(heights: &RowHeights, max_row: u32) -> bool {
    heights.len() <= MAX_ROW_HEIGHTS
        && heights
            .last_key_value()
            .is_none_or(|(&row, _)| row < max_row)
        && heights
            .values()
            .all(|h| (1..=MAX_ROW_HEIGHT_CENTIPT).contains(&h.centipt))
}

impl SheetStyling {
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

/// The height a `style:table-row-properties` element sets, when a user set it (`style:use-optimal-row-height="false"`):
/// a row that follows its content has no height of its own here, the program that reads the file measures it.
pub(crate) fn read_ods_row_properties(
    props: &str,
    length_to_pt: impl Fn(&str) -> Option<f64>,
) -> Option<RowHeight> {
    use crate::rtf_xml_helpers::extract_attr_from_fragment as attr;
    if attr(props, "style:use-optimal-row-height") != Some("false") {
        return None;
    }
    RowHeight::from_points(length_to_pt(attr(props, "style:row-height")?)?, true)
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
        assert!(row_heights_within_bounds(&heights, 6));
        assert!(!row_heights_within_bounds(&heights, 5));
        heights.insert(
            1,
            RowHeight {
                centipt: 0,
                custom: true,
            },
        );
        assert!(!row_heights_within_bounds(&heights, 6));
        heights.insert(
            1,
            RowHeight {
                centipt: MAX_ROW_HEIGHT_CENTIPT + 1,
                custom: true,
            },
        );
        assert!(!row_heights_within_bounds(&heights, 6));
        let many: RowHeights = (0..=MAX_ROW_HEIGHTS as u32).map(|r| (r, one)).collect();
        assert!(!row_heights_within_bounds(&many, u32::MAX));
    }
}

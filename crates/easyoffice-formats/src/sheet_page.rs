//! The page setup of a worksheet for printing: paper, orientation, margins, scale (a percentage or a number
//! of pages across and down), centring, grid lines and headings, the print area and the rows and columns
//! repeated on every page (PD-213).
//!
//! 참조 구현: none. The open spreadsheet sources compared (IronCalc `xlsx/src/import/worksheets.rs` only names
//! `pageSetup` in a comment, Fortune-sheet has no print model, Univer `packages/sheets-ui/src/services/
//! print-interceptor.service.ts` is a hook for a print range and holds no setup; Apache-2.0 and MIT) keep no
//! print area, margins or scale, so the model follows the standards: ECMA-376 18.3.1.62 `pageSetup`
//! (`paperSize`, `scale` 10 to 400, `fitToWidth`, `fitToHeight`, `orientation`), 18.3.1.63 `pageMargins`
//! (inches), 18.3.1.70 `printOptions`, 18.3.1.65 `pageSetUpPr@fitToPage`, and the built-in defined names
//! `_xlnm.Print_Area` and `_xlnm.Print_Titles` of 18.2.5; ODF 1.2 `style:page-layout-properties` and
//! `table:print-ranges` for ODS.
//!
//! Every size is bounded: the ranges, the repeated rows and columns, the scale, the pages across and down,
//! and the margins, so a hostile file or payload cannot make the print path build an unbounded page list.

use crate::CellBox;

/// Print ranges one sheet may carry.
pub const MAX_PRINT_RANGES: usize = 64;
/// Rows and columns repeated on every page. A page rarely holds more than a few dozen rows.
pub const MAX_TITLE_ROWS: u32 = 256;
pub const MAX_TITLE_COLS: u32 = 64;
/// The scale in percent (ECMA-376 18.3.1.62 `scale`).
pub const MIN_PAGE_SCALE: u16 = 10;
pub const MAX_PAGE_SCALE: u16 = 400;
/// Pages across or down a fitted sheet may cover.
pub const MAX_FIT_PAGES: u16 = 1_000;
/// Largest margin, in thousandths of an inch (22 inches, as for a document page).
pub const MAX_MARGIN_MILS: u32 = 22_000;

/// Paper sizes by the code of `pageSetup@paperSize` (ECMA-376 18.18.52 `ST_PaperSize`).
pub const PAPER_LETTER: u16 = 1;
pub const PAPER_LEGAL: u16 = 5;
pub const PAPER_A3: u16 = 8;
pub const PAPER_A4: u16 = 9;
pub const PAPER_B4: u16 = 12;
pub const PAPER_B5: u16 = 13;
/// The papers the page setup offers, in the order of the list.
pub const PAPER_CHOICES: [u16; 6] = [
    PAPER_A4,
    PAPER_A3,
    PAPER_B4,
    PAPER_B5,
    PAPER_LETTER,
    PAPER_LEGAL,
];
/// Highest `paperSize` code of the standard.
pub const MAX_PAPER_CODE: u16 = 118;

/// How much of the sheet goes on a page: a percentage, or a fit to a number of pages across and down
/// (`0` leaves that direction as long as it needs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageScale {
    Percent(u16),
    Fit { width: u16, height: u16 },
}

/// Margins in thousandths of an inch (`pageMargins` counts inches).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageMargins {
    pub left: u32,
    pub right: u32,
    pub top: u32,
    pub bottom: u32,
    pub header: u32,
    pub footer: u32,
}

impl PageMargins {
    /// What a worksheet without `pageMargins` prints with (ECMA-376 18.3.1.63 gives no default; the usual
    /// spreadsheet value is 0.7 inch at the sides, 0.75 above and below, 0.3 for the header and footer).
    pub const NORMAL: Self = Self {
        left: 700,
        right: 700,
        top: 750,
        bottom: 750,
        header: 300,
        footer: 300,
    };

    pub fn within_bounds(&self) -> bool {
        [
            self.left,
            self.right,
            self.top,
            self.bottom,
            self.header,
            self.footer,
        ]
        .iter()
        .all(|v| *v <= MAX_MARGIN_MILS)
    }
}

/// The page setup of one worksheet; the default prints the used range on the printer's paper at 100%.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetPage {
    pub landscape: bool,
    /// `pageSetup@paperSize`; `None` is the printer's own paper.
    pub paper: Option<u16>,
    /// `None` is [`PageMargins::NORMAL`].
    pub margins: Option<PageMargins>,
    pub scale: PageScale,
    pub center_horizontal: bool,
    pub center_vertical: bool,
    pub grid_lines: bool,
    pub headings: bool,
    /// The print area as zero-based rectangles; empty prints the used range.
    pub print_area: Vec<CellBox>,
    /// Rows and columns repeated on every page: zero-based inclusive runs.
    pub title_rows: Option<(u32, u32)>,
    pub title_cols: Option<(u32, u32)>,
}

impl Default for SheetPage {
    fn default() -> Self {
        Self {
            landscape: false,
            paper: None,
            margins: None,
            scale: PageScale::Percent(100),
            center_horizontal: false,
            center_vertical: false,
            grid_lines: false,
            headings: false,
            print_area: Vec::new(),
            title_rows: None,
            title_cols: None,
        }
    }
}

fn run_within(run: Option<(u32, u32)>, limit: u32, max: u32) -> bool {
    run.is_none_or(|(a, b)| a <= b && b < limit && b - a < max)
}

impl SheetPage {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Whether the scale, the margins and the paper are in range, and the print area and the repeated runs
    /// lie in a sheet of `max_row` rows and `max_col` columns and under the bounds.
    pub fn within_bounds(&self, max_row: u32, max_col: u32) -> bool {
        let scale_ok = match self.scale {
            PageScale::Percent(p) => (MIN_PAGE_SCALE..=MAX_PAGE_SCALE).contains(&p),
            PageScale::Fit { width, height } => width <= MAX_FIT_PAGES && height <= MAX_FIT_PAGES,
        };
        scale_ok
            && self.paper.is_none_or(|p| (1..=MAX_PAPER_CODE).contains(&p))
            && self.margins.is_none_or(|m| m.within_bounds())
            && self.print_area.len() <= MAX_PRINT_RANGES
            && self.print_area.iter().all(|b| {
                b.row0 <= b.row1 && b.col0 <= b.col1 && b.row1 < max_row && b.col1 < max_col
            })
            && run_within(self.title_rows, max_row, MAX_TITLE_ROWS)
            && run_within(self.title_cols, max_col, MAX_TITLE_COLS)
    }

    /// The paper's size in millimetres, upright (width, height); A4 for a code the page setup does not know.
    pub fn paper_mm(&self) -> (f64, f64) {
        match self.paper {
            Some(PAPER_LETTER) => (215.9, 279.4),
            Some(PAPER_LEGAL) => (215.9, 355.6),
            Some(PAPER_A3) => (297.0, 420.0),
            Some(PAPER_B4) => (257.0, 364.0),
            Some(PAPER_B5) => (182.0, 257.0),
            _ => (210.0, 297.0),
        }
    }
}

/// `mils` thousandths of an inch as the inches a file keeps: `0.7`, `0.75`, `1`.
pub(crate) fn mils_text(mils: u32) -> String {
    let text = format!("{}.{:03}", mils / 1000, mils % 1000);
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Inches as thousandths of an inch (`0.7` is 700); `None` for a negative, a non-number or one over the bound.
pub(crate) fn inches_to_mils(text: &str) -> Option<u32> {
    let value = text.trim().parse::<f64>().ok()?;
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    let mils = (value * 1000.0).round();
    (mils <= f64::from(MAX_MARGIN_MILS)).then_some(mils as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn margins_are_exact_in_thousandths_of_an_inch() {
        for mils in [0, 1, 300, 700, 750, 1000, 1250, 22_000] {
            assert_eq!(inches_to_mils(&mils_text(mils)), Some(mils), "{mils}");
        }
        assert_eq!(mils_text(700), "0.7");
        assert_eq!(mils_text(1000), "1");
        assert_eq!(inches_to_mils("-0.1"), None);
        assert_eq!(inches_to_mils("23"), None);
        assert_eq!(inches_to_mils("NaN"), None);
    }

    #[test]
    fn bounds_cover_scale_pages_ranges_and_repeated_runs() {
        let ok = SheetPage::default();
        assert!(ok.within_bounds(100, 10));
        let mut bad = ok.clone();
        bad.scale = PageScale::Percent(9);
        assert!(!bad.within_bounds(100, 10));
        bad.scale = PageScale::Percent(401);
        assert!(!bad.within_bounds(100, 10));
        bad.scale = PageScale::Fit {
            width: 1,
            height: MAX_FIT_PAGES + 1,
        };
        assert!(!bad.within_bounds(100, 10));
        let mut bad = ok.clone();
        bad.title_rows = Some((0, MAX_TITLE_ROWS));
        assert!(!bad.within_bounds(1_000_000, 10));
        bad.title_rows = Some((5, 4));
        assert!(!bad.within_bounds(100, 10));
        let mut bad = ok.clone();
        bad.print_area = vec![CellBox::cell(0, 0); MAX_PRINT_RANGES + 1];
        assert!(!bad.within_bounds(100, 10));
        bad.print_area = vec![CellBox::cell(100, 0)];
        assert!(!bad.within_bounds(100, 10));
        let mut bad = ok;
        bad.margins = Some(PageMargins {
            left: MAX_MARGIN_MILS + 1,
            ..PageMargins::NORMAL
        });
        assert!(!bad.within_bounds(100, 10));
    }
}

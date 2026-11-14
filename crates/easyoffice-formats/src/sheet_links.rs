//! Hyperlinks of sheet cells (PD-218): the model, the address policy and the bounds every reader,
//! writer and request of the page follows.
//!
//! 참조 구현: Univer `packages/sheets-hyper-link/src/types/interfaces/i-hyper-link.ts` (a link has a
//! `payload` address, an optional `tooltip` and the cell it belongs to) and `models/hyper-link.model.ts`
//! (one link per cell); Apache-2.0, Copyright 2023-present DreamNum Co., Ltd., commit
//! 5a06a7842c8e13fa0646d0733dc3662cf4b330b9. IronCalc `base/src/types.rs` lines 172-190 (`Link`: an address
//! outside the workbook or a location inside it, the displayed text is the cell's own content) and
//! `xlsx/src/import/worksheets.rs` lines 732-806 (`load_hyperlinks`: a `ref` range, a range of more than
//! 10,000 cells gives its link to the first cell only); MIT OR Apache-2.0, commit
//! 01b39c1ad26252f7b6c810440e8433a932daee07. See `THIRD_PARTY_NOTICES.md`. The address policy is this
//! repository's own and the same as the document editor's (`safe_link_target`): a plain web or mail
//! address, or a jump inside the workbook (`#Sheet1!A1`, `#Name`); every other scheme is refused.
//!
//! The cell's displayed text is the cell value, so a link keeps no text of its own. The readers and
//! writers sit beside the model: `xlsx_links.rs` (`<hyperlinks>` and the sheet's relationships) and
//! `ods_links.rs` (`text:a`).

use std::collections::BTreeMap;

use crate::rich_document::safe_link_target;
use crate::CellBox;

// The readers and writers of the model sit beside it; they are internal to the crate.
#[path = "ods_links.rs"]
pub(crate) mod ods_links;
#[path = "xls_links.rs"]
pub(crate) mod xls_links;
#[path = "xlsx_links.rs"]
pub(crate) mod xlsx_links;

/// Links one sheet may carry (the limit of the binary spreadsheet format). Past this a reader drops
/// the rest and a request is refused.
pub const MAX_LINKS_PER_SHEET: usize = 65_530;
/// Characters of an in-workbook location after the `#` (the spreadsheet format's own limit).
pub const MAX_LOCATION_CHARS: usize = 255;
/// Characters of a screen tip (the spreadsheet format's own limit).
pub const MAX_LINK_TIP_CHARS: usize = 255;
/// Cells one `ref` range may give its link to; a larger range links its first cell only.
pub const MAX_LINK_AREA_CELLS: u64 = 10_000;
/// Cells a document's readers may visit to place links, all sheets together (the size of one sheet). A range
/// is charged its area before it is walked, so many ranges over the same cells cannot repeat the walk.
pub const MAX_LINK_SCAN_CELLS: u64 = 1_048_576;
/// Bytes of addresses and screen tips a document may hold in all its sheets.
pub const MAX_LINK_BYTES_PER_DOCUMENT: usize = 16 * 1024 * 1024;

/// The hyperlink of one cell. `target` is a web or mail address, or `#` and a location inside the
/// workbook (`#Sheet1!B2`, `#'Sheet 2'!A1:B3`, `#Name`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CellLink {
    pub target: String,
    /// The tip shown over the link; empty for none.
    pub tip: String,
}

/// The links of a sheet by zero-based `(row, column)`.
pub type SheetLinks = BTreeMap<(u32, u32), CellLink>;

fn control(c: char) -> bool {
    c < ' ' || ('\u{7f}'..='\u{9f}').contains(&c)
}

/// The address as it is kept, or `None` when the policy refuses it: a plain web or mail address, or a
/// `#` location of at most [`MAX_LOCATION_CHARS`] characters with no control character.
pub fn link_target(raw: &str) -> Option<String> {
    let text = raw.trim();
    if let Some(location) = text.strip_prefix('#') {
        let fits = !location.is_empty()
            && location.chars().count() <= MAX_LOCATION_CHARS
            && !location.chars().any(control);
        return fits.then(|| text.to_string());
    }
    safe_link_target(text).map(str::to_string)
}

/// The tip as it is kept: at most [`MAX_LINK_TIP_CHARS`] characters, control characters dropped.
pub fn link_tip(raw: &str) -> String {
    raw.chars()
        .filter(|c| !control(*c) || *c == '\n')
        .take(MAX_LINK_TIP_CHARS)
        .collect()
}

impl CellLink {
    /// A link for `target` and `tip` under the policy, or `None` when the address is refused.
    pub fn new(target: &str, tip: &str) -> Option<Self> {
        Some(Self {
            target: link_target(target)?,
            tip: link_tip(tip),
        })
    }

    /// Whether the link jumps inside the workbook.
    pub fn is_internal(&self) -> bool {
        self.target.starts_with('#')
    }

    fn bytes(&self) -> usize {
        self.target.len() + self.tip.len() + std::mem::size_of::<CellLink>()
    }
}

/// What the readers of a document share: the allowance of bytes and of cells to visit.
#[derive(Debug)]
pub(crate) struct LinkAllowance {
    bytes_left: usize,
    cells_left: u64,
    /// Cells the walks have visited (a test reads it).
    walked: u64,
}

impl Default for LinkAllowance {
    fn default() -> Self {
        Self {
            bytes_left: MAX_LINK_BYTES_PER_DOCUMENT,
            cells_left: MAX_LINK_SCAN_CELLS,
            walked: 0,
        }
    }
}

impl LinkAllowance {
    /// Takes `cells` out of the allowance of cells to visit; false when it is used up.
    fn visit(&mut self, cells: u64) -> bool {
        match self.cells_left.checked_sub(cells) {
            Some(left) => {
                self.cells_left = left;
                true
            }
            None => false,
        }
    }
}

/// The links a reader collects for one sheet, with the ones the policy or a bound turned away.
#[derive(Debug, Default)]
pub(crate) struct LinkCollector {
    pub(crate) links: SheetLinks,
    pub(crate) rejected: u32,
}

impl LinkCollector {
    /// Keeps the link of one cell when the policy and the bounds allow it; the first link of a cell wins.
    /// Returns false once a bound is reached, so a caller that loops over cells stops there.
    #[cfg(test)]
    pub(crate) fn add(
        &mut self,
        allowance: &mut LinkAllowance,
        at: (u32, u32),
        target: &str,
        tip: &str,
    ) -> bool {
        self.add_area(
            allowance,
            CellBox {
                row0: at.0,
                col0: at.1,
                row1: at.0,
                col1: at.1,
            },
            target,
            tip,
        )
    }

    /// Keeps the link for every cell of `area`, as far as the bounds allow. The policy is applied once and the
    /// area is charged to the document's allowance of cells to visit before it is walked; an area over
    /// [`MAX_LINK_AREA_CELLS`] or over what is left of the allowance links its first cell only, and the walk
    /// stops at the first cell that already has a link. Returns false once a bound is reached.
    pub(crate) fn add_area(
        &mut self,
        allowance: &mut LinkAllowance,
        area: CellBox,
        target: &str,
        tip: &str,
    ) -> bool {
        let Some(link) = CellLink::new(target, tip) else {
            self.rejected = self.rejected.saturating_add(1);
            return true;
        };
        let mut cells = area.area();
        if cells > MAX_LINK_AREA_CELLS || cells > allowance.cells_left {
            self.rejected = self.rejected.saturating_add(1);
            cells = 1;
        }
        if !allowance.visit(cells) {
            self.rejected = self.rejected.saturating_add(1);
            return false;
        }
        let (row1, col1) = if cells == 1 {
            (area.row0, area.col0)
        } else {
            (area.row1, area.col1)
        };
        for row in area.row0..=row1 {
            for col in area.col0..=col1 {
                allowance.walked += 1;
                if self.links.contains_key(&(row, col)) {
                    return true;
                }
                let cost = link.bytes();
                if self.links.len() >= MAX_LINKS_PER_SHEET || cost > allowance.bytes_left {
                    self.rejected = self.rejected.saturating_add(1);
                    return false;
                }
                allowance.bytes_left -= cost;
                self.links.insert((row, col), link.clone());
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_policy_takes_web_mail_and_workbook_targets_only() {
        for good in [
            "https://example.com/a?b=1",
            "  http://a.b  ",
            "mailto:me@example.com",
            "#Sheet1!B2",
            "#'Sheet 2'!A1:B3",
            "#Total",
        ] {
            assert!(link_target(good).is_some(), "{good}");
        }
        for bad in [
            "",
            "#",
            "javascript:alert(1)",
            " JaVaScRiPt:alert(1)",
            "file:///etc/passwd",
            "data:text/html,x",
            "vbscript:x",
            "ftp://a.b",
            "tel:1",
            "https://",
            "https://a b",
            "https://a.b/\u{7}",
            "mailto:",
            "example.com",
            "//example.com",
            "#A1\u{0}",
            "../other.xlsx",
            "\\\\server\\share",
        ] {
            assert_eq!(link_target(bad), None, "{bad:?}");
        }
        assert_eq!(
            link_target(&format!("https://a.b/{}", "x".repeat(2_048))),
            None
        );
        assert_eq!(
            link_target(&format!("#{}", "A".repeat(MAX_LOCATION_CHARS + 1))),
            None
        );
        assert!(link_target(&format!("#{}", "A".repeat(MAX_LOCATION_CHARS))).is_some());
    }

    #[test]
    fn a_tip_is_cut_and_stripped() {
        assert_eq!(link_tip("a\u{0}b\nc"), "ab\nc");
        assert_eq!(
            link_tip(&"가".repeat(300)).chars().count(),
            MAX_LINK_TIP_CHARS
        );
    }

    #[test]
    fn the_collector_counts_what_it_turns_away_and_keeps_the_first_link_of_a_cell() {
        let mut allowance = LinkAllowance::default();
        let mut got = LinkCollector::default();
        got.add(&mut allowance, (0, 0), "https://a.b", "tip");
        got.add(&mut allowance, (0, 0), "https://c.d", "");
        got.add(&mut allowance, (0, 1), "javascript:x", "");
        assert_eq!(got.links.len(), 1);
        assert_eq!(got.links[&(0, 0)].target, "https://a.b");
        assert_eq!(got.rejected, 1);
    }

    #[test]
    fn the_same_range_a_hundred_thousand_times_walks_it_once() {
        let mut allowance = LinkAllowance::default();
        let mut got = LinkCollector::default();
        let block = CellBox::parse("A1:CV100").unwrap();
        for _ in 0..131_060 {
            got.add_area(&mut allowance, block, "https://a.b", "");
        }
        assert_eq!(got.links.len(), 10_000);
        // The first walk covers the block; every other one stops at its first cell, which has a link, and once the
        // document's allowance of cells is charged the rest are turned away without a walk.
        assert!(
            allowance.walked >= 10_000 && allowance.walked <= 10_000 + 131_059,
            "walked {}",
            allowance.walked
        );
        assert!(allowance.walked < 20_000, "walked {}", allowance.walked);
        assert_eq!(allowance.cells_left, 0);
    }

    #[test]
    fn many_different_ranges_and_many_sheets_share_one_budget_of_cells() {
        let mut allowance = LinkAllowance::default();
        let mut sheets = Vec::new();
        for sheet in 0..8 {
            let mut got = LinkCollector::default();
            for i in 0..400u32 {
                // 400 disjoint blocks of 10,000 cells per sheet, 8 sheets: 32 million cells asked for.
                let row0 = (i % 100) * 100 + 1_000 * sheet;
                let col0 = (i / 100) * 100;
                let block = CellBox {
                    row0,
                    col0,
                    row1: row0 + 99,
                    col1: col0 + 99,
                };
                got.add_area(&mut allowance, block, "https://a.b", "");
            }
            sheets.push(got);
        }
        assert!(
            allowance.walked <= MAX_LINK_SCAN_CELLS + 8 * 400,
            "walked {}",
            allowance.walked
        );
        assert!(sheets.iter().map(|g| g.rejected).sum::<u32>() > 0);
    }

    #[test]
    fn the_collector_stops_at_the_bounds() {
        let mut allowance = LinkAllowance::default();
        let mut got = LinkCollector::default();
        let whole_sheet = CellBox::parse("A1:XFD1048576").unwrap();
        got.add_area(&mut allowance, whole_sheet, "https://a.b", "");
        assert_eq!(
            got.links.len(),
            1,
            "a range over the area bound links its first cell only"
        );
        assert_eq!(got.rejected, 1);
        let mut got = LinkCollector::default();
        let block = CellBox::parse("A1:CV100").unwrap();
        got.add_area(&mut allowance, block, "https://a.b", "");
        assert_eq!(got.links.len(), 10_000);
        for row in 100..70_000 {
            got.add(&mut allowance, (row, 0), "https://a.b", "");
        }
        assert_eq!(got.links.len(), MAX_LINKS_PER_SHEET);

        let mut small = LinkAllowance {
            bytes_left: 300,
            ..LinkAllowance::default()
        };
        let mut got = LinkCollector::default();
        for row in 0..100 {
            got.add(&mut small, (row, 0), "https://example.com/a", "");
        }
        assert!(got.links.len() < 100 && got.rejected > 0);
    }
}

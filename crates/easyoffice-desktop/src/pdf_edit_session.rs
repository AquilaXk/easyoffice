//! The PDF a window is editing: the files as they were opened, and the plan of the result (PD-156).
//!
//! An edit never rewrites a file. The session keeps each opened PDF untouched and a list of the
//! pages the result will have (which file, which page, how much more turned), plus the stamp and
//! watermark. Rotating, merging and choosing a range change the plan only; the new PDF is written
//! from the plan when it is saved (`pdf_edit_build.rs`), so nothing is lost by editing twice.

use crate::pdf_edit_budget::{
    Deadline, MAX_PAGES, MAX_SESSION_BYTES, MAX_SOURCES, MAX_SPLIT_FILES,
};
use crate::pdf_edit_build::{build, PagePlan};
use crate::pdf_edit_inspect::inspect;
use crate::pdf_edit_overlay::Overlay;
use std::ops::Range;
use std::sync::Arc;
use usvg::fontdb::Database;

const MSG_ROTATION: &str = "회전 각도는 90도 단위여야 합니다.";
const MSG_RANGE_SYNTAX: &str = "쪽 범위를 읽을 수 없습니다. 예: 1-3, 5, 7- (쉼표로 나눕니다)";

#[derive(Clone)]
pub struct EditSession {
    /// The opened file's name without its extension: the base of the names the results get.
    stem: String,
    sources: Vec<Arc<Vec<u8>>>,
    pages: Vec<PagePlan>,
    overlay: Overlay,
}

impl EditSession {
    /// Opens a PDF for editing; it passes the checks of `inspect` or is refused.
    pub fn open(bytes: Vec<u8>, deadline: &Deadline) -> Result<Self, String> {
        let bytes = Arc::new(bytes);
        let info = inspect(&bytes, deadline)?;
        Ok(Self {
            stem: String::new(),
            sources: vec![bytes],
            pages: plan_of(0, info.pages),
            overlay: Overlay::default(),
        })
    }

    pub fn with_stem(mut self, stem: &str) -> Self {
        self.stem = stem.to_string();
        self
    }

    pub fn stem(&self) -> &str {
        &self.stem
    }

    /// Pads the plan to `count` pages (page limit tests).
    #[cfg(test)]
    pub fn pages_for_test(&mut self, count: usize) {
        let first = self.pages[0];
        self.pages.resize(count, first);
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    pub fn overlay(&self) -> &Overlay {
        &self.overlay
    }

    /// The extra turn of the first page (what the rotation badge shows).
    pub fn first_turn(&self) -> u16 {
        self.pages.first().map_or(0, |p| p.turn)
    }

    /// Turns the pages of `range` (all pages without one) `degrees` more, clockwise.
    pub fn rotate(&mut self, range: Option<&str>, degrees: i32) -> Result<(), String> {
        if degrees % 90 != 0 {
            return Err(MSG_ROTATION.to_string());
        }
        let chosen = match range {
            Some(spec) => parse_ranges(spec, self.pages.len())?,
            None => std::iter::once(0..self.pages.len()).collect(),
        };
        for range in chosen {
            for page in &mut self.pages[range] {
                let turned = (i32::from(page.turn) + degrees).rem_euclid(360);
                page.turn = u16::try_from(turned).unwrap_or(0);
            }
        }
        Ok(())
    }

    /// Makes every page's extra turn exactly `degrees` (0, 90, 180 or 270): the page's rotation
    /// control shows one number for the whole file, and this is how that number reaches the plan
    /// whatever was turned or undone on the way.
    pub fn set_turn(&mut self, degrees: i32) -> Result<(), String> {
        if degrees % 90 != 0 {
            return Err(MSG_ROTATION.to_string());
        }
        let turn = u16::try_from(degrees.rem_euclid(360)).unwrap_or(0);
        for page in &mut self.pages {
            page.turn = turn;
        }
        Ok(())
    }

    pub fn set_overlay(&mut self, overlay: Overlay) {
        self.overlay = overlay;
    }

    /// Appends all pages of `files` after the current ones. Each file passes `inspect`, and the
    /// totals (files, bytes, pages) stay within the budgets, or nothing is appended.
    pub fn merge(&mut self, files: Vec<Vec<u8>>, deadline: &Deadline) -> Result<usize, String> {
        if self.sources.len() + files.len() > MAX_SOURCES {
            return Err(format!(
                "함께 편집할 수 있는 PDF는 {MAX_SOURCES}개까지입니다."
            ));
        }
        let held: usize = self.sources.iter().map(|s| s.len()).sum();
        let mut bytes = held;
        let mut added = Vec::with_capacity(files.len());
        let mut pages = self.pages.len();
        for file in files {
            bytes = bytes.saturating_add(file.len());
            if bytes > MAX_SESSION_BYTES {
                return Err(format!(
                    "함께 편집하는 PDF의 합계가 허용 크기({}MiB)를 초과합니다.",
                    MAX_SESSION_BYTES / 1_048_576
                ));
            }
            let file = Arc::new(file);
            let info = inspect(&file, deadline)?;
            pages += info.pages;
            if pages > MAX_PAGES {
                return Err(format!("합친 쪽 수가 한도({MAX_PAGES})를 초과합니다."));
            }
            added.push((file, info.pages));
        }
        let merged = added.len();
        for (file, count) in added {
            let source = self.sources.len();
            self.sources.push(file);
            self.pages.extend(plan_of(source, count));
        }
        Ok(merged)
    }

    /// The page ranges of a split: `spec` ("1-3, 5, 7-") or, without one, one file per page.
    pub fn split_ranges(&self, spec: Option<&str>) -> Result<Vec<Range<usize>>, String> {
        let ranges = match spec.map(str::trim).filter(|s| !s.is_empty()) {
            Some(spec) => parse_ranges(spec, self.pages.len())?,
            None => (0..self.pages.len()).map(|n| n..n + 1).collect(),
        };
        if ranges.len() > MAX_SPLIT_FILES {
            return Err(format!(
                "한 번에 나눌 수 있는 파일은 {MAX_SPLIT_FILES}개까지입니다. 쪽 범위를 지정해 주세요."
            ));
        }
        Ok(ranges)
    }

    /// Writes the PDF of `range` of the plan (all of it without one).
    pub fn render(
        &self,
        range: Option<Range<usize>>,
        fonts: &Arc<Database>,
        deadline: &Deadline,
    ) -> Result<Vec<u8>, String> {
        let pages = match range {
            Some(range) => self.pages.get(range).ok_or("없는 쪽 범위입니다.")?,
            None => &self.pages[..],
        };
        build(&self.sources, pages, &self.overlay, fonts, deadline)
    }
}

/// The counts only: the bytes of the files never go into a log line or an assertion message.
impl std::fmt::Debug for EditSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditSession")
            .field("sources", &self.sources.len())
            .field("pages", &self.pages.len())
            .finish_non_exhaustive()
    }
}

fn plan_of(source: usize, count: usize) -> Vec<PagePlan> {
    (0..count)
        .map(|index| PagePlan {
            source,
            index,
            turn: 0,
        })
        .collect()
}

/// Reads "1-3, 5, 7-" (pages from 1) into ranges of positions. A range is `a-b`, a single page, or
/// open at one end (`7-` runs to the last page, `-3` starts at the first). Anything else, a page
/// outside `1..=total` and a range that runs backwards are refused.
pub fn parse_ranges(spec: &str, total: usize) -> Result<Vec<Range<usize>>, String> {
    let mut ranges = Vec::new();
    for item in spec.split([',', ';']).map(str::trim) {
        if item.is_empty() {
            return Err(MSG_RANGE_SYNTAX.to_string());
        }
        let (from, to) = match item.split_once('-') {
            None => {
                let page = page_number(item, total)?;
                (page, page)
            }
            Some((a, b)) if a.trim().is_empty() && b.trim().is_empty() => {
                return Err(MSG_RANGE_SYNTAX.to_string());
            }
            Some((a, b)) => (
                if a.trim().is_empty() {
                    1
                } else {
                    page_number(a, total)?
                },
                if b.trim().is_empty() {
                    total
                } else {
                    page_number(b, total)?
                },
            ),
        };
        if from > to {
            return Err(format!("쪽 범위가 거꾸로입니다: {item}"));
        }
        ranges.push(from - 1..to);
        if ranges.len() > MAX_PAGES {
            return Err(MSG_RANGE_SYNTAX.to_string());
        }
    }
    Ok(ranges)
}

fn page_number(text: &str, total: usize) -> Result<usize, String> {
    let text = text.trim();
    if text.is_empty() || text.len() > 9 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(MSG_RANGE_SYNTAX.to_string());
    }
    let page: usize = text.parse().map_err(|_| MSG_RANGE_SYNTAX.to_string())?;
    if page == 0 || page > total {
        return Err(format!("없는 쪽입니다: {page} (전체 {total}쪽)"));
    }
    Ok(page)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::single_range_in_vec_init)] // the expected values are lists of ranges
    fn ranges_are_read_from_one_and_ends_may_be_open() {
        assert_eq!(parse_ranges("1-3, 5, 7-", 9), Ok(vec![0..3, 4..5, 6..9]));
        assert_eq!(parse_ranges("-2", 5), Ok(vec![0..2]));
        assert_eq!(parse_ranges(" 2 ; 4 ", 5), Ok(vec![1..2, 3..4]));
        assert_eq!(parse_ranges("1-1", 1), Ok(vec![0..1]));
    }

    #[test]
    fn bad_ranges_are_refused_with_a_sentence() {
        for (spec, total) in [
            ("", 5),
            (",", 5),
            ("0", 5),
            ("6", 5),
            ("1-6", 5),
            ("3-2", 5),
            ("a", 5),
            ("1--3", 5),
            ("1-2-3", 5),
            ("+1", 5),
            ("1e1", 5),
            ("99999999999", 5),
            ("-", 5),
            ("1,,2", 5),
        ] {
            let err = parse_ranges(spec, total).expect_err(spec);
            assert!(!err.is_empty(), "{spec}");
        }
    }

    fn session(count: usize) -> EditSession {
        EditSession {
            stem: String::new(),
            sources: vec![Arc::new(Vec::new())],
            pages: plan_of(0, count),
            overlay: Overlay::default(),
        }
    }

    #[test]
    fn rotating_adds_up_and_wraps() {
        let mut s = session(3);
        s.rotate(None, 90).unwrap();
        s.rotate(Some("2-3"), 90).unwrap();
        s.rotate(Some("3"), -270).unwrap();
        let turns: Vec<u16> = s.pages.iter().map(|p| p.turn).collect();
        assert_eq!(turns, [90, 180, 270]);
        s.rotate(None, -90).unwrap();
        assert_eq!(
            s.pages.iter().map(|p| p.turn).collect::<Vec<_>>(),
            [0, 90, 180]
        );
        assert_eq!(s.first_turn(), 0);
        assert!(s.rotate(None, 45).is_err());
        assert!(s.rotate(Some("4"), 90).is_err());
    }

    #[test]
    fn a_turn_is_set_for_every_page_not_added() {
        let mut s = session(3);
        s.rotate(Some("2"), 90).unwrap();
        s.set_turn(270).unwrap();
        assert!(s.pages.iter().all(|p| p.turn == 270));
        s.set_turn(-90).unwrap();
        assert!(s.pages.iter().all(|p| p.turn == 270));
        s.set_turn(360).unwrap();
        assert!(s.pages.iter().all(|p| p.turn == 0));
        assert!(s.set_turn(30).is_err());
    }

    #[test]
    fn a_split_defaults_to_one_file_per_page_within_the_file_limit() {
        let s = session(3);
        assert_eq!(s.split_ranges(None).unwrap(), vec![0..1, 1..2, 2..3]);
        assert_eq!(s.split_ranges(Some("1-2, 3")).unwrap(), vec![0..2, 2..3]);
        assert!(session(MAX_SPLIT_FILES + 1).split_ranges(None).is_err());
        assert!(session(MAX_SPLIT_FILES + 1)
            .split_ranges(Some("1-"))
            .is_ok());
    }
}

//! Merged cells of a slide table (`PD-402`): the spans an element keeps in [`crate::ElementFx::spans`], their
//! limits, and the cover map the file writers and the PDF drawing use to know which cell is the origin of a merge
//! and which cells it covers.
//!
//! Reference implementation (3-step check): (1) no maintained crate models a DrawingML table grid, (2) the deck model
//! kept plain rows of text, so the spans ride in the element's effect entry like the other per-element extras,
//! (3) the file convention follows PptxGenJS `src/gen-xml.ts` lines 213-282 (MIT, Copyright (c) 2015-present Brent Ely,
//! commit 3c9ec1b687c1): the origin cell carries `gridSpan` and `rowSpan`; every cell a merge covers is written as an
//! empty `a:tc` with `hMerge="1"` (covered to the right of the origin) and `vMerge="1"` (covered below it), a cell
//! covered in both ways carries both. PptxGenJS builds the grid from the spans it is given and trusts them; here a
//! span outside the grid, over the table limits or over another span is refused (a file read drops it).

use crate::xml_stream::{attr, XmlEvent};
use crate::{EasyOfficeError, MAX_SLIDE_TABLE_COLS, MAX_SLIDE_TABLE_ROWS};

/// One merged range of a table: the top-left cell and how many rows and columns it spans (each at least 1, one of them
/// above 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableSpan {
    pub row: u16,
    pub col: u16,
    pub row_span: u16,
    pub col_span: u16,
}

/// What a cell of the grid is in a merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CellCover {
    #[default]
    Plain,
    /// The top-left cell of a merge; it keeps the text and the fill.
    Origin(TableSpan),
    /// Covered by a merge: `h` when it lies to the right of the origin's row, `v` when below the origin's row.
    Covered { h: bool, v: bool },
}

impl TableSpan {
    fn rows(self) -> std::ops::Range<usize> {
        usize::from(self.row)..usize::from(self.row) + usize::from(self.row_span)
    }

    fn cols(self) -> std::ops::Range<usize> {
        usize::from(self.col)..usize::from(self.col) + usize::from(self.col_span)
    }

    fn fits(self, rows: usize, cols: usize) -> bool {
        self.row_span >= 1
            && self.col_span >= 1
            && (self.row_span > 1 || self.col_span > 1)
            && self.rows().end <= rows.min(MAX_SLIDE_TABLE_ROWS)
            && self.cols().end <= cols.min(MAX_SLIDE_TABLE_COLS)
    }
}

/// The cover map of a `rows` x `cols` grid; a span that does not fit or crosses an earlier one is left out, so a hostile
/// list can neither index outside the grid nor cover a cell twice.
pub fn cover_map(spans: &[TableSpan], rows: usize, cols: usize) -> Vec<Vec<CellCover>> {
    let mut map = vec![vec![CellCover::Plain; cols]; rows];
    for span in spans.iter().copied().filter(|s| s.fits(rows, cols)) {
        let free = span
            .rows()
            .all(|r| span.cols().all(|c| map[r][c] == CellCover::Plain));
        if !free {
            continue;
        }
        for r in span.rows() {
            for c in span.cols() {
                map[r][c] = if (r, c) == (usize::from(span.row), usize::from(span.col)) {
                    CellCover::Origin(span)
                } else {
                    CellCover::Covered {
                        h: c > usize::from(span.col),
                        v: r > usize::from(span.row),
                    }
                };
            }
        }
    }
    map
}

/// The spans of `spans` that a `rows` x `cols` grid accepts, in reading order (what a file writer emits).
pub fn accepted_spans(spans: &[TableSpan], rows: usize, cols: usize) -> Vec<TableSpan> {
    let mut out: Vec<TableSpan> = cover_map(spans, rows, cols)
        .into_iter()
        .flatten()
        .filter_map(|c| match c {
            CellCover::Origin(s) => Some(s),
            _ => None,
        })
        .collect();
    out.sort_by_key(|s| (s.row, s.col));
    out
}

/// The element and attribute names of a table dialect: DrawingML (`a:tr`, `a:tc`, `gridSpan`, `rowSpan`) or ODF
/// (`table:table-row`, `table:table-cell` and `table:covered-table-cell`, `table:number-*-spanned`).
pub(crate) struct SpanDialect {
    pub row: &'static str,
    pub cells: &'static [&'static str],
    pub col_span: &'static str,
    pub row_span: &'static str,
    /// The attribute that repeats a cell over several columns (ODF only).
    pub col_repeat: Option<&'static str>,
}

pub(crate) const DRAWINGML: SpanDialect = SpanDialect {
    row: "tr",
    cells: &["tc"],
    col_span: "gridSpan",
    row_span: "rowSpan",
    col_repeat: None,
};

pub(crate) const ODF: SpanDialect = SpanDialect {
    row: "table-row",
    cells: &["table-cell", "covered-table-cell"],
    col_span: "table:number-columns-spanned",
    row_span: "table:number-rows-spanned",
    col_repeat: Some("table:number-columns-repeated"),
};

/// Merged cells of a table subtree: every cell that starts a merge, kept through [`accepted_spans`], so a span past the
/// grid, past the table limits or over another span is dropped.
pub(crate) fn scan_spans(
    events: &[XmlEvent<'_>],
    dialect: &SpanDialect,
    rows: usize,
    cols: usize,
) -> Vec<TableSpan> {
    let local = crate::xml_stream::local_name;
    let (mut row, mut col) = (0usize, 0usize);
    let mut spans = Vec::new();
    for ev in events {
        let XmlEvent::Start { name, attrs } = ev else {
            continue;
        };
        let tag = local(name);
        if tag == dialect.row {
            row = row.wrapping_add(1);
            col = 0;
        } else if dialect.cells.contains(&tag) {
            let span_of = |key: &str| {
                attr(attrs, key)
                    .or_else(|| attr(attrs, local(key)))
                    .and_then(|v| v.trim().parse::<u16>().ok())
                    .filter(|v| *v >= 1)
                    .unwrap_or(1)
            };
            let (col_span, row_span) = (span_of(dialect.col_span), span_of(dialect.row_span));
            if col_span > 1 || row_span > 1 {
                if let (Ok(r), Ok(c)) = (u16::try_from(row.wrapping_sub(1)), u16::try_from(col)) {
                    spans.push(TableSpan {
                        row: r,
                        col: c,
                        row_span,
                        col_span,
                    });
                }
            }
            // A repeated cell takes that many columns (the reader refused a row past the column limit).
            let repeat = dialect
                .col_repeat
                .and_then(|key| attr(attrs, key).or_else(|| attr(attrs, local(key))))
                .and_then(|v| v.trim().parse::<usize>().ok())
                .filter(|n| (1..=MAX_SLIDE_TABLE_COLS).contains(n))
                .unwrap_or(1);
            col += repeat;
        }
    }
    accepted_spans(&spans, rows, cols)
}

/// Fail-Closed check of the spans a save request carries: every span inside the grid, none over another.
pub fn check_spans(spans: &[TableSpan], rows: usize, cols: usize) -> Result<(), EasyOfficeError> {
    if spans.len() > MAX_SLIDE_TABLE_ROWS * MAX_SLIDE_TABLE_COLS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "슬라이드 표 병합 수가 상한을 초과했습니다.".to_string(),
        ));
    }
    if let Some(bad) = spans.iter().find(|s| !s.fits(rows, cols)) {
        return Err(EasyOfficeError::InvalidArgument(format!(
            "슬라이드 표 병합({}행 {}열, {}x{})이 표 범위를 벗어났습니다.",
            bad.row, bad.col, bad.row_span, bad.col_span
        )));
    }
    if accepted_spans(spans, rows, cols).len() != spans.len() {
        return Err(EasyOfficeError::InvalidArgument(
            "슬라이드 표 병합이 서로 겹칩니다.".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(row: u16, col: u16, row_span: u16, col_span: u16) -> TableSpan {
        TableSpan {
            row,
            col,
            row_span,
            col_span,
        }
    }

    #[test]
    fn covered_cells_carry_the_h_and_v_marks_of_the_file_convention() {
        // PptxGenJS gen-xml.ts 213-232 example: A0 rowspan 3, B1 rowspan 2, C1 colspan 2.
        let map = cover_map(
            &[span(0, 0, 3, 1), span(1, 1, 2, 1), span(1, 2, 1, 2)],
            3,
            4,
        );
        assert!(matches!(map[0][0], CellCover::Origin(_)));
        assert_eq!(map[1][0], CellCover::Covered { h: false, v: true });
        assert_eq!(map[2][0], CellCover::Covered { h: false, v: true });
        assert_eq!(map[2][1], CellCover::Covered { h: false, v: true });
        assert_eq!(map[1][3], CellCover::Covered { h: true, v: false });
        let both = cover_map(&[span(0, 0, 2, 2)], 2, 2);
        assert_eq!(both[1][1], CellCover::Covered { h: true, v: true });
    }

    #[test]
    fn hostile_spans_are_refused_or_dropped() {
        assert!(check_spans(&[span(0, 0, 2, 2), span(1, 1, 2, 1)], 3, 3).is_err());
        assert!(check_spans(&[span(0, 0, 4, 1)], 3, 3).is_err());
        assert!(check_spans(&[span(2, 2, 1, 2)], 3, 3).is_err());
        assert!(check_spans(&[span(0, 0, 0, 2)], 3, 3).is_err());
        assert!(check_spans(&[span(0, 0, 1, 1)], 3, 3).is_err());
        assert!(check_spans(&[span(u16::MAX, u16::MAX, u16::MAX, u16::MAX)], 3, 3).is_err());
        let map = cover_map(
            &[span(0, 0, 9, 9), span(0, 0, 2, 2), span(1, 1, 2, 2)],
            3,
            3,
        );
        let origins = map
            .iter()
            .flatten()
            .filter(|c| matches!(c, CellCover::Origin(_)))
            .count();
        assert_eq!(
            origins, 1,
            "the oversize span is dropped and the overlap is not applied"
        );
    }
}

//! RTF 1.9 table rows for [`RichTable`]: `\trowd`, cell right edges (`\cellxN`, twips), merge marks
//! (`\clmgf`/`\clmrg` across, `\clvmgf`/`\clvmrg` down), `\cell` and `\row`; and the reverse
//! mapping from the marks a reader saw to validated [`CellMerge`] regions.

use std::collections::HashMap;

use crate::rich_table::{CellMerge, CellSpan, RichTable, SpanMap};
use crate::RtfCodec;

/// Table width the writer splits evenly between the columns (6.5 inches).
const TABLE_WIDTH_TWIPS: usize = 9_360;

/// Horizontal merge mark of a cell definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum HMark {
    #[default]
    None,
    First,
    Merged,
}

/// Vertical merge mark of a cell definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum VMark {
    #[default]
    None,
    First,
    Merged,
}

/// Merge marks of one cell definition (the control words before its `\cellx`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct CellMarks {
    pub h: HMark,
    pub v: VMark,
}

fn push_marks(out: &mut String, spans: &SpanMap, row: usize, col: usize) {
    let (anchor_row, anchor_col, v) = match spans.at(row, col) {
        CellSpan::Plain => return,
        CellSpan::Anchor { row_span, .. } => (row, col, (row_span > 1).then_some("\\clvmgf")),
        CellSpan::Covered {
            row_offset,
            col_offset,
        } => (
            row - row_offset as usize,
            col - col_offset as usize,
            (row_offset > 0).then_some("\\clvmrg"),
        ),
    };
    if let Some(v) = v {
        out.push_str(v);
    }
    if let CellSpan::Anchor { col_span, .. } = spans.at(anchor_row, anchor_col) {
        if col_span > 1 {
            out.push_str(if col == anchor_col {
                "\\clmgf"
            } else {
                "\\clmrg"
            });
        }
    }
}

/// One `{\trowd ... \row}` line per table row; `table_idx` numbers the table for the reader.
pub(crate) fn rtf_table_lines(table: &RichTable, table_idx: usize) -> Vec<String> {
    let cols = table.column_count().max(1);
    let spans = table.span_map();
    table
        .rows
        .iter()
        .enumerate()
        .map(|(r, row)| {
            let mut line = format!("{{\\trowd{{\\*\\tblidx {table_idx}}} ");
            for c in 0..row.len() {
                if let Some(spans) = &spans {
                    push_marks(&mut line, spans, r, c);
                }
                line.push_str(&format!("\\cellx{}", TABLE_WIDTH_TWIPS * (c + 1) / cols));
            }
            line.push(' ');
            for cell in row {
                line.push_str(&RtfCodec::escape_rtf_text(cell));
                line.push_str("\\cell ");
            }
            line.push_str("\\row}");
            line
        })
        .collect()
}

/// Escaped cell texts of one `{\trowd ... \row}` line, for both layouts: standard (every `\cellx`
/// before the first cell) and the older one (`\cellx` right before each cell's text). Cell
/// definitions (`\cellx`, `\cl*` merge marks) are dropped; escapes stay for the unescaper.
pub(crate) fn flat_row_cells(line: &str) -> Vec<String> {
    let body = line.strip_prefix("{\\trowd").unwrap_or(line);
    let body = match body.strip_prefix("{\\*\\tblidx") {
        Some(rest) => rest.split_once('}').map_or("", |(_, tail)| tail),
        None => body,
    };
    let body = body.strip_prefix(' ').unwrap_or(body);
    let bytes = body.as_bytes();
    let (mut cells, mut cur) = (Vec::new(), String::new());
    let (mut copied, mut i) = (0usize, 0usize);
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            i += 1;
            continue;
        }
        let word_end = bytes[i + 1..]
            .iter()
            .position(|b| !b.is_ascii_lowercase())
            .map_or(bytes.len(), |n| i + 1 + n);
        let word = &body[i + 1..word_end];
        let is_def = matches!(word, "cellx" | "clmgf" | "clmrg" | "clvmgf" | "clvmrg");
        if word.is_empty() || !(is_def || word == "cell") {
            // Control symbol or other control word: keep it in the text.
            i = word_end.max(i + 2);
            continue;
        }
        cur.push_str(&body[copied..i]);
        let mut end = word_end;
        if word == "cellx" {
            while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                end += 1;
            }
        }
        if bytes.get(end) == Some(&b' ') {
            end += 1;
        }
        if word == "cell" {
            cells.push(std::mem::take(&mut cur));
        }
        copied = end;
        i = end;
    }
    cells
}

/// Width of the horizontal run that starts at `col`, in cells.
fn run_width(marks: &[CellMarks], col: usize) -> u32 {
    let merged = marks[col + 1..]
        .iter()
        .take_while(|m| m.h == HMark::Merged)
        .count();
    u32::try_from(merged + 1).unwrap_or(u32::MAX)
}

/// Turns per-row cell marks into candidate regions. A vertical run continues only into the cell
/// directly below with the same width; anything else starts a new region. The caller validates the
/// result with [`RichTable::with_merges`].
pub(crate) fn merges_from_marks(rows: &[Vec<CellMarks>]) -> Vec<CellMerge> {
    let mut merges: Vec<CellMerge> = Vec::new();
    let mut open: HashMap<usize, (usize, u32)> = HashMap::new();
    for (r, marks) in rows.iter().enumerate() {
        let mut next_open = HashMap::new();
        for c in 0..marks.len() {
            if marks[c].h == HMark::Merged {
                continue;
            }
            let width = run_width(marks, c);
            let continued = (marks[c].v == VMark::Merged)
                .then(|| open.get(&c).copied().filter(|(_, w)| *w == width))
                .flatten();
            if let Some((idx, w)) = continued {
                merges[idx].row_span = merges[idx].row_span.saturating_add(1);
                next_open.insert(c, (idx, w));
            } else if marks[c].v == VMark::First || width > 1 {
                merges.push(CellMerge {
                    row: r as u32,
                    col: c as u32,
                    row_span: 1,
                    col_span: width,
                });
                if marks[c].v == VMark::First {
                    next_open.insert(c, (merges.len() - 1, width));
                }
            }
        }
        open = next_open;
    }
    merges
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(rows: &[&[&str]]) -> Vec<Vec<String>> {
        rows.iter()
            .map(|r| r.iter().map(|c| c.to_string()).collect())
            .collect()
    }

    #[test]
    fn flat_reader_reads_standard_and_old_layouts() {
        let standard = "{\\trowd{\\*\\tblidx 0} \\clmgf\\cellx3000\\clmrg\\cellx6000 a \\\\cell\\cell b\\line c\\cell \\row}";
        assert_eq!(flat_row_cells(standard), vec!["a \\\\cell", "b\\line c"]);
        let old = "{\\trowd{\\*\\tblidx 0} \\cellx a\\cell \\cellx b\\cell \\row}";
        assert_eq!(flat_row_cells(old), vec!["a", "b"]);
    }

    #[test]
    fn plain_table_gets_even_cell_edges() {
        let table = RichTable::new(grid(&[&["a", "b", "c"]]));
        assert_eq!(
            rtf_table_lines(&table, 0),
            vec!["{\\trowd{\\*\\tblidx 0} \\cellx3120\\cellx6240\\cellx9360 a\\cell b\\cell c\\cell \\row}"]
        );
    }

    #[test]
    fn merged_regions_write_marks_and_cellx_for_covered_cells() {
        let (table, dropped) = RichTable::with_merges(
            grid(&[&["a", "", "x"], &["", "", "y"]]),
            vec![CellMerge {
                row: 0,
                col: 0,
                row_span: 2,
                col_span: 2,
            }],
        );
        assert_eq!(dropped, 0);
        let lines = rtf_table_lines(&table, 1);
        assert!(lines[0].contains(" \\clvmgf\\clmgf\\cellx3120\\clmrg\\cellx6240\\cellx9360 "));
        assert!(
            lines[1].contains(" \\clvmrg\\clmgf\\cellx3120\\clvmrg\\clmrg\\cellx6240\\cellx9360 ")
        );
    }

    #[test]
    fn marks_round_trip_to_regions() {
        let mark = |h, v| CellMarks { h, v };
        let rows = vec![
            vec![
                mark(HMark::First, VMark::First),
                mark(HMark::Merged, VMark::First),
                mark(HMark::None, VMark::None),
            ],
            vec![
                mark(HMark::First, VMark::Merged),
                mark(HMark::Merged, VMark::Merged),
                mark(HMark::None, VMark::None),
            ],
        ];
        assert_eq!(
            merges_from_marks(&rows),
            vec![CellMerge {
                row: 0,
                col: 0,
                row_span: 2,
                col_span: 2
            }]
        );
    }
}

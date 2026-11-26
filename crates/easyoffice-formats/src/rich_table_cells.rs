//! Per-cell rich paragraph formatting (inner runs, alignment, and line spacing) for [`RichTable`].

use crate::{RichAlignment, RichParagraph, RichRun, RichScript};

/// Maximum cells carrying rich paragraph formatting per table.
pub const MAX_CELL_PARAGRAPH_ENTRIES: usize = 50_000;
/// Maximum paragraphs accepted in one rich table cell.
pub const MAX_PARAGRAPHS_PER_CELL: usize = 2_000;
/// Maximum runs accepted in one rich table cell paragraph.
pub const MAX_RUNS_PER_CELL_PARA: usize = 10_000;

/// One paragraph inside a table cell: runs plus cell paragraph alignment and line spacing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RichCellParagraph {
    pub runs: Vec<RichRun>,
    pub alignment: Option<RichAlignment>,
    pub line_spacing_percent: Option<u16>,
}

/// Rich paragraph content for cell `(row, col)` of a [`crate::RichTable`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TableCellParagraphs {
    pub row: u32,
    pub col: u32,
    pub paragraphs: Vec<RichCellParagraph>,
}

#[inline]
fn run_has_formatting(run: &RichRun) -> bool {
    run.bold
        || run.italic
        || run.underline
        || run.strike
        || run.size_half_pt.is_some()
        || run.font.is_some()
        || run.color.is_some()
        || run.highlight.is_some()
        || run.script != RichScript::Normal
}

/// True when any paragraph in the cell has non-default character formatting, alignment, or line spacing.
pub(crate) fn cell_para_has_formatting(paragraphs: &[RichCellParagraph]) -> bool {
    paragraphs.iter().any(|p| {
        matches!(
            p.alignment,
            Some(RichAlignment::Center | RichAlignment::Right | RichAlignment::Justify)
        ) || p.line_spacing_percent.is_some_and(|pct| pct != 100)
            || p.runs.iter().any(run_has_formatting)
    })
}

/// In-bounds, non-default cell paragraph entries capped at [`MAX_CELL_PARAGRAPH_ENTRIES`], sorted by
/// `(row, col)` and deduplicated (last entry wins).
pub(crate) fn validate_cell_paragraphs(
    rows: &[Vec<String>],
    entries: &[TableCellParagraphs],
) -> Vec<TableCellParagraphs> {
    if entries.is_empty() || rows.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<TableCellParagraphs> = entries
        .iter()
        .filter(|e| {
            !e.paragraphs.is_empty()
                && cell_para_has_formatting(&e.paragraphs)
                && usize::try_from(e.row)
                    .ok()
                    .zip(usize::try_from(e.col).ok())
                    .is_some_and(|(r, c)| rows.get(r).and_then(|row| row.get(c)).is_some())
        })
        .take(MAX_CELL_PARAGRAPH_ENTRIES)
        .cloned()
        .collect();
    out.sort_by_key(|e| (u64::from(e.row) << 32) | u64::from(e.col));
    let mut write = 0usize;
    for read in 0..out.len() {
        if write > 0 && out[write - 1].row == out[read].row && out[write - 1].col == out[read].col {
            out[write - 1] = out[read].clone();
        } else {
            if write != read {
                out.swap(write, read);
            }
            write += 1;
        }
    }
    out.truncate(write);
    out
}

/// Advances `cursor` to return the rich paragraphs for `(row, col)`, if any.
pub(crate) fn slice_cell_paragraphs<'a>(
    entries: &'a [TableCellParagraphs],
    cursor: &mut usize,
    row: usize,
    col: usize,
) -> Option<&'a [RichCellParagraph]> {
    let target = (row as u32, col as u32);
    while *cursor < entries.len() && (entries[*cursor].row, entries[*cursor].col) < target {
        *cursor += 1;
    }
    if *cursor < entries.len() && (entries[*cursor].row, entries[*cursor].col) == target {
        let res = entries[*cursor].paragraphs.as_slice();
        *cursor += 1;
        Some(res)
    } else {
        None
    }
}

/// Additional heap-owned string bytes carried by one [`RichRun`] beyond `text` and `link`
/// (which streaming walkers charge on push).
#[inline]
pub(crate) fn run_extra_metadata_bytes(run: &RichRun) -> usize {
    let rev_bytes = run
        .revision
        .as_ref()
        .map_or(0, |rev| rev.author.len().saturating_add(rev.date.len()));
    run.font
        .as_deref()
        .map_or(0, str::len)
        .saturating_add(run.color.as_deref().map_or(0, str::len))
        .saturating_add(run.highlight.as_deref().map_or(0, str::len))
        .saturating_add(rev_bytes)
}

/// Estimates total struct and heap byte cost of one cell's rich paragraphs for `DecodeBudget`.
pub(crate) fn estimate_cell_paragraphs_bytes(paras: &[RichCellParagraph]) -> usize {
    let mut total = paras
        .len()
        .saturating_mul(std::mem::size_of::<RichCellParagraph>());
    for p in paras {
        total = total.saturating_add(p.runs.len().saturating_mul(std::mem::size_of::<RichRun>()));
        for r in &p.runs {
            let run_bytes = r
                .text
                .len()
                .saturating_add(r.link.as_deref().map_or(0, str::len))
                .saturating_add(run_extra_metadata_bytes(r));
            total = total.saturating_add(run_bytes);
        }
    }
    total
}

/// Single-pass extraction for cell paragraphs: returns plain cell text, UTF-8 byte hyperlink spans,
/// and (when non-default formatting fits within cell limits) the structured [`RichCellParagraph`] list.
/// When a cell exceeds [`MAX_PARAGRAPHS_PER_CELL`] or any paragraph exceeds [`MAX_RUNS_PER_CELL_PARA`],
/// structured formatting drops to `None` so plain `text` retains all paragraphs without silent truncation.
#[allow(clippy::type_complexity)]
pub(crate) fn join_rich_cell_paragraphs(
    paras: &[RichParagraph],
) -> (
    String,
    Vec<(u32, u32, String)>,
    Option<Vec<RichCellParagraph>>,
) {
    let mut end_idx = paras.len();
    while end_idx > 0
        && paras[end_idx - 1].runs.iter().all(|r| r.text.is_empty())
        && paras[end_idx - 1].alignment.is_none()
        && paras[end_idx - 1].line_spacing_percent.is_none()
    {
        end_idx -= 1;
    }
    let active = &paras[..end_idx];
    if active.is_empty() {
        return (String::new(), Vec::new(), None);
    }
    let total_bytes: usize = active
        .iter()
        .map(|p| p.runs.iter().map(|r| r.text.len()).sum::<usize>())
        .sum::<usize>()
        + active.len().saturating_sub(1);
    let mut text = String::with_capacity(total_bytes);
    let mut spans: Vec<(u32, u32, String)> = Vec::new();
    let mut rich_overflow = active.len() > MAX_PARAGRAPHS_PER_CELL;
    let mut cell_paras = if rich_overflow {
        Vec::new()
    } else {
        Vec::with_capacity(active.len())
    };
    for (p_idx, p) in active.iter().enumerate() {
        if p_idx > 0 {
            text.push('\n');
        }
        let mut kept_runs = if rich_overflow {
            Vec::new()
        } else {
            Vec::with_capacity(p.runs.len().min(MAX_RUNS_PER_CELL_PARA))
        };
        for run in &p.runs {
            if run.text.is_empty() {
                continue;
            }
            let start = text.len();
            text.push_str(&run.text);
            let end = text.len();
            if let (Some(url), Ok(s), Ok(e)) = (&run.link, u32::try_from(start), u32::try_from(end))
            {
                if let Some(last) = spans.last_mut().filter(|(_, le, lu)| *le == s && lu == url) {
                    last.1 = e;
                } else {
                    spans.push((s, e, url.clone()));
                }
            }
            if !rich_overflow {
                if kept_runs.len() < MAX_RUNS_PER_CELL_PARA {
                    kept_runs.push(run.clone());
                } else {
                    rich_overflow = true;
                    kept_runs = Vec::new();
                    cell_paras = Vec::new();
                }
            }
        }
        if !rich_overflow {
            cell_paras.push(RichCellParagraph {
                runs: kept_runs,
                alignment: p.alignment,
                line_spacing_percent: p.line_spacing_percent,
            });
        }
    }
    let rich = (!rich_overflow && cell_para_has_formatting(&cell_paras)).then_some(cell_paras);
    (text, spans, rich)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RichRevision, RichRevisionKind};

    #[test]
    fn overflow_paragraphs_or_runs_drop_rich_to_none_while_keeping_all_plain_text() {
        let mut paras: Vec<RichParagraph> = (0..=MAX_PARAGRAPHS_PER_CELL)
            .map(|i| RichParagraph {
                runs: vec![RichRun {
                    text: format!("p{i}"),
                    bold: true,
                    ..RichRun::default()
                }],
                ..RichParagraph::default()
            })
            .collect();
        let (text, _spans, rich) = join_rich_cell_paragraphs(&paras);
        assert!(rich.is_none());
        assert!(text.ends_with(&format!("p{MAX_PARAGRAPHS_PER_CELL}")));
        assert_eq!(text.lines().count(), MAX_PARAGRAPHS_PER_CELL + 1);

        paras.truncate(1);
        paras[0].runs = (0..=MAX_RUNS_PER_CELL_PARA)
            .map(|i| RichRun {
                text: if i == MAX_RUNS_PER_CELL_PARA {
                    "TAIL".to_string()
                } else {
                    "a".to_string()
                },
                italic: true,
                ..RichRun::default()
            })
            .collect();
        let (run_text, _spans, run_rich) = join_rich_cell_paragraphs(&paras);
        assert!(run_rich.is_none());
        assert!(run_text.ends_with("TAIL"));
        assert_eq!(run_text.len(), MAX_RUNS_PER_CELL_PARA + 4);
    }

    #[test]
    fn estimate_cell_paragraphs_bytes_counts_structs_and_strings() {
        let paras = vec![RichCellParagraph {
            runs: vec![RichRun {
                text: "abc".to_string(),
                link: Some("https://example.org".to_string()),
                font: Some("Pretendard".to_string()),
                color: Some("112233".to_string()),
                highlight: Some("FFFF00".to_string()),
                revision: Some(RichRevision {
                    id: 1,
                    kind: RichRevisionKind::Insert,
                    author: "QA".to_string(),
                    date: "2026-11-26".to_string(),
                }),
                ..RichRun::default()
            }],
            alignment: Some(RichAlignment::Center),
            line_spacing_percent: Some(150),
        }];
        let expected = std::mem::size_of::<RichCellParagraph>()
            + std::mem::size_of::<RichRun>()
            + 3
            + "https://example.org".len()
            + "Pretendard".len()
            + 6
            + 6
            + 2
            + 10;
        assert_eq!(estimate_cell_paragraphs_bytes(&paras), expected);
    }
}

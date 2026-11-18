//! The text a sheet's cells show when they leave the grid: the PDF and the exports to other
//! formats (PD-235). A cell with a number format shows what the engine's formatter gives, the
//! same code path as the grid and `TEXT`; a cell without one shows its General text.

use easyoffice_formats::{resolve_code, CellShown, SheetShown, SheetStyling};
use easyoffice_sheet::{compile_for_page, general_display, CompiledNumFmt};

use std::borrow::Cow;

/// What the cells of a sheet become.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Show {
    /// A drawn page (PDF): General rule for plain cells, and the colour, alignment and fill of
    /// each formatted cell go to the page.
    Page,
    /// A table in a document: General rule for plain cells, formatted text with its fill once.
    Table,
    /// A value file (CSV): formatted text with its fill once, plain cells keep their stored text.
    Values,
}

/// Resolves and pre-compiles each distinct `num_fmt` id (`0..=676`) at most once (`O(1)` array lookup).
struct NumFmtMemo {
    slots: Vec<Option<Option<CompiledNumFmt>>>,
}

impl NumFmtMemo {
    const fn new() -> Self {
        Self { slots: Vec::new() }
    }

    fn compiled<'a>(
        &'a mut self,
        styling: &SheetStyling,
        at: (u32, u32),
    ) -> Option<&'a CompiledNumFmt> {
        let id = styling.cells.get(&at)?.num_fmt;
        if id == 0 {
            return None;
        }
        let idx = usize::from(id);
        if self.slots.len() <= idx {
            self.slots.resize(idx + 1, None);
        }
        if self.slots[idx].is_none() {
            let entry = resolve_code(id, &styling.num_fmts).map(compile_for_page);
            self.slots[idx] = Some(entry);
        }
        self.slots[idx].as_ref().and_then(Option::as_ref)
    }
}

/// Replaces the text of every cell in `cells` by its display under `mode`; for [`Show::Page`] it
/// returns the colour, alignment and fill of the cells a format changed.
pub(crate) fn show_cells(
    cells: &mut [(u32, u32, String)],
    styling: Option<&SheetStyling>,
    mode: Show,
) -> SheetShown {
    let mut shown = SheetShown::new();
    let mut memo = NumFmtMemo::new();
    for (row, col, text) in cells.iter_mut() {
        let at = (*row, *col);
        let formatted = styling
            .and_then(|s| memo.compiled(s, at))
            .and_then(|fmt| fmt.format(text));
        match formatted {
            Some(cell) if mode == Show::Page => {
                shown.insert(
                    at,
                    CellShown {
                        numeric: cell.numeric,
                        color: cell.color,
                        fill: cell.fill,
                    },
                );
                *text = cell.text;
            }
            Some(cell) => *text = cell.flat(),
            None if mode != Show::Values => {
                if let Cow::Owned(general) = general_display(text) {
                    *text = general;
                }
            }
            None => {}
        }
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::*;
    use easyoffice_formats::{CellStyle, CUSTOM_BASE};
    use easyoffice_sheet::format_cell_value;
    use std::sync::Arc;
    use std::time::Instant;

    fn sample_50k() -> (Vec<(u32, u32, String)>, SheetStyling) {
        let mut cells = Vec::with_capacity(50_000);
        let mut styling = SheetStyling {
            num_fmts: Arc::new(vec![
                "#,##0.00;[Red]-#,##0.00".to_string(),
                "yyyy-mm-dd".to_string(),
                "_(\"$\"* #,##0.00_)".to_string(),
            ]),
            ..SheetStyling::default()
        };
        for r in 0..10_000u32 {
            for c in 0..5u32 {
                let raw = match c {
                    0 => format!("{}", (r as i64 - 5000) as f64 * 12.345),
                    1 => format!("{}", 45_000 + (r % 365)),
                    2 => format!("{}", r as f64 * 0.75),
                    3 => format!("{}", r as f64 / 3.0),
                    _ => format!("item-{r}"),
                };
                cells.push((r, c, raw));
                if c < 3 {
                    styling.cells.insert(
                        (r, c),
                        CellStyle {
                            num_fmt: CUSTOM_BASE + c as u16,
                            ..CellStyle::default()
                        },
                    );
                }
            }
        }
        (cells, styling)
    }

    #[test]
    fn memoized_show_cells_matches_direct_per_cell_formatting() {
        let (mut a, styling) = sample_50k();
        a.truncate(200);
        let mut b = a.clone();
        let shown_a = show_cells(&mut a, Some(&styling), Show::Page);
        for (row, col, text) in b.iter_mut() {
            let code = styling
                .cells
                .get(&(*row, *col))
                .and_then(|s| (s.num_fmt != 0).then(|| resolve_code(s.num_fmt, &styling.num_fmts)))
                .flatten();
            if let Some(cell) = code.and_then(|c| format_cell_value(c, text)) {
                *text = cell.text;
            } else if let Cow::Owned(gen) = general_display(text) {
                *text = gen;
            }
        }
        assert_eq!(a, b);
        assert_eq!(shown_a.len(), 120);
    }

    #[test]
    #[ignore]
    fn bench_show_cells_50k_release() {
        let (base_cells, styling) = sample_50k();
        let mut old_times = Vec::new();
        let mut new_times = Vec::new();
        for _ in 0..7 {
            let mut cells = base_cells.clone();
            let t0 = Instant::now();
            for (row, col, text) in cells.iter_mut() {
                let at = (*row, *col);
                let code = styling
                    .cells
                    .get(&at)
                    .and_then(|s| {
                        (s.num_fmt != 0).then(|| resolve_code(s.num_fmt, &styling.num_fmts))
                    })
                    .flatten();
                if let Some(c) = code {
                    let compiled = easyoffice_sheet::compile_for_page(c);
                    let _numeric = text.trim().parse::<f64>().is_ok_and(f64::is_finite);
                    if let Some(cell) = compiled.format(text) {
                        *text = cell.flat();
                    }
                } else if let Cow::Owned(gen) = general_display(text) {
                    *text = gen;
                }
            }
            old_times.push(t0.elapsed().as_secs_f64() * 1000.0);

            let mut cells2 = base_cells.clone();
            let t1 = Instant::now();
            let _ = show_cells(&mut cells2, Some(&styling), Show::Table);
            new_times.push(t1.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(cells, cells2);
        }
        old_times.sort_by(f64::total_cmp);
        new_times.sort_by(f64::total_cmp);
        println!(
            "50k cells Table display median: uncached+double_parse={:.2} ms -> memo+single_parse={:.2} ms",
            old_times[3], new_times[3]
        );
    }
}

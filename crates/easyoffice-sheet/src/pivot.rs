//! Pivot tables (PD-171): the values of a range grouped by a row field and an optional column field.
//!
//! The groups are collected in one pass over the rows. The number of cells the answer would have
//! (row groups times column groups) is checked each time a group appears, before any grid exists,
//! and the pass stops at a wall-clock deadline, so a hostile range ends in an error, not in memory
//! exhaustion or a hang. Each cell read starts its own step budget.

use crate::{
    CellRange, EvalContext, EvaluatedCellValue, PackedCellCoordinate, PivotAggregation,
    PivotTableSummary, SheetEngine, WorkbookEngine,
};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Rows between two clock reads.
const CLOCK_ROWS: u32 = 1024;
pub const MAX_PIVOT_CELLS: usize = 100_000;

/// What a pivot takes from its range: offsets are columns counted from the range's first column.
#[derive(Debug, Clone)]
pub struct PivotSpec {
    pub range: CellRange,
    pub header: bool,
    pub row: usize,
    pub col: Option<usize>,
    pub value: usize,
    pub agg: PivotAggregation,
}

/// Why a pivot was not made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PivotError {
    /// The sheet is not in the workbook.
    NoSheet,
    /// Row groups times column groups pass the cell limit.
    TooLarge,
    /// The deadline passed before every row was read.
    TimedOut,
}

/// Display text and case-folded canonical form of a pivot key cell.
fn normalize_pivot_key(v: EvaluatedCellValue) -> (String, String) {
    let disp = match v {
        EvaluatedCellValue::Number(n) => crate::coercion::format_general_number(n),
        EvaluatedCellValue::Text(s) => match crate::coercion::parse_strict_number(&s) {
            Some(n) => crate::coercion::format_general_number(n),
            None => s,
        },
        other => other.to_display_string(),
    };
    let canon = disp.to_lowercase();
    (disp, canon)
}

fn pivot_total_key() -> (String, String) {
    ("Total".to_string(), "total".to_string())
}

/// Numeric contribution of a pivot value cell (`COUNT` also counts non-empty non-numbers).
fn pivot_numeric_value(val: &EvaluatedCellValue, aggregation: PivotAggregation) -> Option<f64> {
    if let EvaluatedCellValue::Number(num) = val {
        return Some(*num);
    }
    let counts_non_empty = aggregation == PivotAggregation::Count
        && !matches!(val, EvaluatedCellValue::Empty)
        && !val.to_display_string().is_empty();
    counts_non_empty.then_some(1.0)
}

#[derive(Debug, Clone, Copy)]
struct PivotAccumulator {
    count: usize,
    sum: f64,
    mean: f64,
    m2: f64,
    min: f64,
    max: f64,
}

impl Default for PivotAccumulator {
    fn default() -> Self {
        Self {
            count: 0,
            sum: 0.0,
            mean: 0.0,
            m2: 0.0,
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
        }
    }
}

impl PivotAccumulator {
    #[inline]
    fn add(&mut self, val: f64) {
        self.count += 1;
        self.sum += val;
        let delta = val - self.mean;
        self.mean += delta / (self.count as f64);
        let delta2 = val - self.mean;
        self.m2 += delta * delta2;
        if val < self.min {
            self.min = val;
        }
        if val > self.max {
            self.max = val;
        }
    }

    #[inline]
    fn result(&self, agg: PivotAggregation) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        match agg {
            PivotAggregation::Sum => self.sum,
            PivotAggregation::Average => self.sum / (self.count as f64),
            PivotAggregation::Count => self.count as f64,
            PivotAggregation::Min => self.min,
            PivotAggregation::Max => self.max,
        }
    }
}

/// An ordered set of group keys: the position of each key by its canonical form.
#[derive(Default)]
struct Groups {
    keys: Vec<String>,
    index: HashMap<String, usize>,
    accs: Vec<PivotAccumulator>,
}

impl Groups {
    fn position(&mut self, display: String, canonical: String) -> usize {
        if let Some(&at) = self.index.get(&canonical) {
            return at;
        }
        let at = self.keys.len();
        self.keys.push(display);
        self.accs.push(PivotAccumulator::default());
        self.index.insert(canonical, at);
        at
    }
}

/// The groups and totals of `spec`, reading each cell through `value_at`. `max_cells` limits the
/// cells of the answer; `deadline` is when the pass gives up. `value_at` fails when the source of
/// the values has run out of time.
fn tally(
    spec: &PivotSpec,
    max_cells: usize,
    deadline: Option<Instant>,
    mut value_at: impl FnMut(PackedCellCoordinate) -> Result<EvaluatedCellValue, PivotError>,
) -> Result<PivotTableSummary, PivotError> {
    let (row_count, col_count) = spec.range.dimensions();
    let min_r = spec.range.start.row().min(spec.range.end.row());
    let min_c = spec.range.start.col().min(spec.range.end.col());
    let first_row = usize::from(spec.header);
    let mut rows = Groups::default();
    let mut cols = Groups::default();
    let mut cell_buckets: HashMap<(usize, usize), PivotAccumulator> = HashMap::new();
    let mut grand_acc = PivotAccumulator::default();

    if spec.row < col_count && spec.value < col_count {
        let key_col = min_c + spec.row as u32;
        let val_col = min_c + spec.value as u32;
        let col_col = spec
            .col
            .filter(|&c| c < col_count)
            .map(|c| min_c + c as u32);
        for r_off in first_row..row_count {
            if r_off as u32 % CLOCK_ROWS == 0 && deadline.is_some_and(|d| Instant::now() >= d) {
                return Err(PivotError::TimedOut);
            }
            let row_idx = min_r + r_off as u32;
            let (Ok(kc), Ok(vc)) = (
                PackedCellCoordinate::new(row_idx, key_col),
                PackedCellCoordinate::new(row_idx, val_col),
            ) else {
                continue;
            };
            let (key_str, canonical_row) = normalize_pivot_key(value_at(kc)?);
            if key_str.is_empty() {
                continue;
            }
            let r_idx = rows.position(key_str, canonical_row);
            if rows.keys.len() * cols.keys.len().max(1) > max_cells {
                return Err(PivotError::TooLarge);
            }
            let (col_str, canonical_col) = match col_col {
                Some(cc) => column_key(row_idx, cc, &mut value_at)?,
                None => pivot_total_key(),
            };
            let c_idx = cols.position(col_str, canonical_col);
            if rows.keys.len() * cols.keys.len() > max_cells {
                return Err(PivotError::TooLarge);
            }
            if let Some(num) = pivot_numeric_value(&value_at(vc)?, spec.agg) {
                cell_buckets.entry((r_idx, c_idx)).or_default().add(num);
                rows.accs[r_idx].add(num);
                cols.accs[c_idx].add(num);
                grand_acc.add(num);
            }
        }
    }
    if cols.keys.is_empty() {
        cols.keys.push("Total".to_string());
        cols.accs.push(PivotAccumulator::default());
    }
    let total = |accs: &[PivotAccumulator]| -> Vec<f64> {
        accs.iter().map(|a| a.result(spec.agg)).collect()
    };
    let matrix = (0..rows.keys.len())
        .map(|r| {
            (0..cols.keys.len())
                .map(|c| {
                    cell_buckets
                        .get(&(r, c))
                        .map_or(0.0, |a| a.result(spec.agg))
                })
                .collect()
        })
        .collect();
    Ok(PivotTableSummary {
        row_totals: total(&rows.accs),
        column_totals: total(&cols.accs),
        grand_total: grand_acc.result(spec.agg),
        row_keys: rows.keys,
        column_keys: cols.keys,
        matrix,
    })
}

/// Column key of the pivot row `row_idx` taken from column `cc_idx`.
fn column_key(
    row_idx: u32,
    cc_idx: u32,
    value_at: &mut impl FnMut(PackedCellCoordinate) -> Result<EvaluatedCellValue, PivotError>,
) -> Result<(String, String), PivotError> {
    let Ok(cc) = PackedCellCoordinate::new(row_idx, cc_idx) else {
        return Ok(pivot_total_key());
    };
    let (cs, canon) = normalize_pivot_key(value_at(cc)?);
    Ok(if cs.is_empty() {
        ("(Blank)".to_string(), "(blank)".to_string())
    } else {
        (cs, canon)
    })
}

impl SheetEngine {
    pub fn compute_pivot(
        &self,
        range: &CellRange,
        has_header_row: bool,
        row_field_col_offset: usize,
        value_field_col_offset: usize,
        aggregation: PivotAggregation,
    ) -> PivotTableSummary {
        self.compute_pivot_grid(
            range,
            has_header_row,
            row_field_col_offset,
            None,
            value_field_col_offset,
            aggregation,
        )
    }

    /// The pivot of this sheet alone (a formula that names another sheet reads `#REF!`), with no
    /// limit on the answer: a caller that takes a request from outside uses
    /// [`WorkbookEngine::compute_pivot_within`].
    pub fn compute_pivot_grid(
        &self,
        range: &CellRange,
        has_header_row: bool,
        row_field_col_offset: usize,
        column_field_col_offset: Option<usize>,
        value_field_col_offset: usize,
        aggregation: PivotAggregation,
    ) -> PivotTableSummary {
        let spec = PivotSpec {
            range: *range,
            header: has_header_row,
            row: row_field_col_offset,
            col: column_field_col_offset,
            value: value_field_col_offset,
            agg: aggregation,
        };
        let mut ctx = EvalContext::standalone_id(self.resolved_sheet_id());
        tally(&spec, MAX_PIVOT_CELLS, None, |coord| {
            ctx.reset_steps();
            Ok(self.eval_coord_ctx(coord, &mut ctx, 0))
        })
        .unwrap_or_default()
    }
}

impl WorkbookEngine {
    /// The pivot of `sheet` over the values the whole workbook gives its cells (other sheets, defined
    /// names and arrays that spill included). The answer has at most `max_cells` cells and the
    /// pass takes at most `limit`.
    pub fn compute_pivot_within(
        &self,
        sheet: &str,
        spec: &PivotSpec,
        max_cells: usize,
        limit: Duration,
    ) -> Result<PivotTableSummary, PivotError> {
        let engine = self.sheet(sheet).ok_or(PivotError::NoSheet)?;
        let deadline = Instant::now().checked_add(limit);
        if self.has_array_formulas() {
            if let Some(settled) = self.settle(limit) {
                if settled.outcome.timed_out {
                    return Err(PivotError::TimedOut);
                }
                let values = settled.session.sheet_values(sheet);
                return tally(spec, max_cells, deadline, |coord| {
                    Ok(values
                        .get(&coord)
                        .cloned()
                        .unwrap_or(EvaluatedCellValue::Empty))
                });
            }
        }
        let mut ctx =
            EvalContext::with_workbook(self, engine.resolved_sheet_id()).with_time_limit(limit);
        tally(spec, max_cells, deadline, |coord| {
            ctx.reset_steps();
            let value = engine.eval_coord_ctx(coord, &mut ctx, 0);
            if ctx.expired() {
                return Err(PivotError::TimedOut);
            }
            Ok(value)
        })
    }
}

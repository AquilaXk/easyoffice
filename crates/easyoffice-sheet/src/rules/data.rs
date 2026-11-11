//! Cell values for rule evaluation and the per-range statistics cache.
//!
//! A rule never reads the workbook directly: it asks a [`Cells`] source, which the session backs
//! with its formula memo and the stateless path backs with a one-shot evaluation. Statistics
//! (minimum, maximum, sum, a sorted copy for ranks and percentiles, standard deviation) are
//! computed once per distinct range list and shared by every rule on it until a cell in the range
//! changes (`StatsCache::invalidate`).

use crate::{EvalContext, EvaluatedCellValue, PackedCellCoordinate, SheetEngine};
use easyoffice_formats::CellBox;
use std::cell::OnceCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Largest area (cells) of a range scanned cell by cell to find empty cells; larger ranges only
/// look at populated cells, so `A:A` stays cheap.
pub(crate) const BLANK_SCAN_CELLS: u64 = 65_536;

pub(crate) trait Cells {
    fn value(&mut self, coord: PackedCellCoordinate) -> EvaluatedCellValue;
    /// Populated cells inside `b`, row-major.
    fn populated(&mut self, b: &CellBox) -> Vec<PackedCellCoordinate>;
    /// Evaluates formula text without the leading `=`.
    fn eval_formula(&mut self, formula: &str) -> EvaluatedCellValue;
}

/// [`Cells`] over one sheet of a workbook and an evaluation context that already holds the values
/// of its formula cells (the session memo, or a pre-pass).
pub(crate) struct SheetCells<'a, 'c> {
    pub sheet: &'a SheetEngine,
    pub ctx: &'c mut EvalContext<'a>,
}

impl Cells for SheetCells<'_, '_> {
    fn value(&mut self, coord: PackedCellCoordinate) -> EvaluatedCellValue {
        self.ctx.steps = 0;
        self.sheet.eval_coord_ctx(coord, self.ctx, 0)
    }

    fn populated(&mut self, b: &CellBox) -> Vec<PackedCellCoordinate> {
        self.sheet
            .cell_index()
            .range_coords(b.row0, b.row1, b.col0, b.col1)
    }

    fn eval_formula(&mut self, formula: &str) -> EvaluatedCellValue {
        self.ctx.steps = 0;
        self.sheet
            .eval_raw_string_ctx(&format!("={formula}"), self.ctx, 0)
    }
}

/// Fills `ctx` with the values of every formula cell inside `boxes` (stateless path).
pub(crate) fn prepare_formulas(sheet: &SheetEngine, boxes: &[CellBox], ctx: &mut EvalContext<'_>) {
    let mut roots = Vec::new();
    for b in boxes {
        for coord in sheet
            .cell_index()
            .range_coords(b.row0, b.row1, b.col0, b.col1)
        {
            if crate::dag::is_formula(sheet.compiled_asts.get(&coord)) {
                roots.push(coord);
            }
        }
    }
    sheet.pre_evaluate_dag_iterative(&roots, ctx);
}

/// Values of one range list: the non-empty cells (plus empty ones when asked and the area is
/// small), and the numeric view with its statistics.
pub(crate) struct RangeData {
    pub cells: Vec<(PackedCellCoordinate, EvaluatedCellValue)>,
    /// `(cell, number)` of the cells holding a number, in `cells` order.
    pub numbers: Vec<(PackedCellCoordinate, f64)>,
    pub min: f64,
    pub max: f64,
    pub sum: f64,
    sorted: OnceCell<Vec<f64>>,
}

impl RangeData {
    fn load(cells: &mut dyn Cells, boxes: &[CellBox], with_blanks: bool) -> Self {
        let mut coords: Vec<PackedCellCoordinate> = Vec::new();
        for b in boxes {
            if with_blanks && b.area() <= BLANK_SCAN_CELLS {
                for r in b.row0..=b.row1 {
                    for c in b.col0..=b.col1 {
                        coords.push(PackedCellCoordinate((u64::from(r) << 32) | u64::from(c)));
                    }
                }
            } else {
                coords.extend(cells.populated(b));
            }
        }
        coords.sort_unstable();
        coords.dedup();
        let mut data = Self {
            cells: Vec::with_capacity(coords.len()),
            numbers: Vec::new(),
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
            sum: 0.0,
            sorted: OnceCell::new(),
        };
        for coord in coords {
            let value = cells.value(coord);
            if let EvaluatedCellValue::Number(n) = value {
                if n.is_finite() {
                    data.numbers.push((coord, n));
                    data.min = data.min.min(n);
                    data.max = data.max.max(n);
                    data.sum += n;
                }
            }
            data.cells.push((coord, value));
        }
        data
    }

    pub fn count(&self) -> usize {
        self.numbers.len()
    }

    pub fn average(&self) -> Option<f64> {
        (!self.numbers.is_empty()).then(|| self.sum / self.numbers.len() as f64)
    }

    /// Sample standard deviation (`STDEV.S`).
    pub fn std_dev(&self) -> Option<f64> {
        let n = self.numbers.len();
        let mean = self.average()?;
        (n > 1).then(|| {
            let ss: f64 = self
                .numbers
                .iter()
                .map(|&(_, v)| (v - mean) * (v - mean))
                .sum();
            (ss / (n - 1) as f64).sqrt()
        })
    }

    /// The numbers in ascending order, sorted once.
    pub fn sorted(&self) -> &[f64] {
        self.sorted.get_or_init(|| {
            let mut v: Vec<f64> = self.numbers.iter().map(|&(_, n)| n).collect();
            v.sort_unstable_by(f64::total_cmp);
            v
        })
    }

    /// `PERCENTILE.INC` by linear interpolation between ranks.
    pub fn percentile(&self, percent: f64) -> Option<f64> {
        let list = self.sorted();
        let last = list.len().checked_sub(1)?;
        let index = last as f64 * percent.clamp(0.0, 100.0) / 100.0;
        let lo = index.floor() as usize;
        let hi = (lo + 1).min(last);
        Some(list[lo] + (list[hi] - list[lo]) * (index - lo as f64))
    }
}

/// Range data shared by the rules that cover the same ranges.
#[derive(Default)]
pub(crate) struct StatsCache {
    entries: HashMap<(Vec<CellBox>, bool), Rc<RangeData>>,
}

impl StatsCache {
    pub fn get(
        &mut self,
        cells: &mut dyn Cells,
        boxes: &[CellBox],
        with_blanks: bool,
    ) -> Rc<RangeData> {
        self.entries
            .entry((boxes.to_vec(), with_blanks))
            .or_insert_with(|| Rc::new(RangeData::load(cells, boxes, with_blanks)))
            .clone()
    }

    /// Forgets every entry that covers one of the changed cells.
    pub fn invalidate(&mut self, touched: &[PackedCellCoordinate]) {
        self.entries.retain(|(boxes, _), _| {
            !touched
                .iter()
                .any(|t| boxes.iter().any(|b| b.contains(t.row(), t.col())))
        });
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

//! Streaming range aggregates (SUM, AVERAGE, MIN, MAX, COUNT*, PRODUCT) over cell
//! ranges: populated cells are folded in a fixed lane order so results stay identical across the
//! dense and sparse walks, and memoized values are borrowed instead of cloned.

use crate::types::CellRange;
use crate::{
    EvalContext, EvaluatedCellValue, Expr, FormulaError, PackedCellCoordinate, SheetEngine,
};

/// Number of independent accumulation lanes (a power of two so lane selection is a mask).
const LANE_COUNT: usize = 4;
const LANE_MASK: usize = LANE_COUNT - 1;
/// Minimum cell count before checking `CellIndex::count_in_range` for count-only aggregates.
const COUNT_SPARSE_MIN_CELLS: usize = 64;
/// Ranges above this size may switch to a walk over populated cells only.
const SPARSE_MIN_CELLS: usize = 1024;
/// A range is sparse when it is larger than this multiple of the populated cell count.
const SPARSE_RATIO: usize = 2;

/// Inclusive `(min_row, max_row, min_col, max_col)` bounds.
type Bounds = (u32, u32, u32, u32);

/// Aggregate kinds are const generics so the per-cell kind dispatch is resolved at compile time.
const KIND_SUM: u8 = 0;
const KIND_AVERAGE: u8 = 1;
const KIND_PRODUCT: u8 = 2;
const KIND_MIN: u8 = 3;
const KIND_MAX: u8 = 4;
const KIND_COUNT: u8 = 5;
const KIND_COUNTA: u8 = 6;
const KIND_COUNTBLANK: u8 = 7;
const KIND_UNKNOWN: u8 = 8;

fn range_bounds(range: &CellRange) -> Bounds {
    (
        range.start.row().min(range.end.row()),
        range.start.row().max(range.end.row()),
        range.start.col().min(range.end.col()),
        range.start.col().max(range.end.col()),
    )
}

/// Sorted coordinates of populated cells that lie inside `bounds`.
#[inline]
fn sorted_population(
    sheet: &SheetEngine,
    (min_r, max_r, min_c, max_c): Bounds,
) -> Vec<PackedCellCoordinate> {
    sheet.cell_index().range_coords(min_r, max_r, min_c, max_c)
}

/// Lane-ordered accumulator state for one aggregate call (`Copy` so walks can keep it in registers).
#[derive(Clone, Copy)]
struct AggFold<const K: u8> {
    sums: [f64; LANE_COUNT],
    products: [f64; LANE_COUNT],
    num_count: usize,
    item_count: usize,
    min_val: Option<f64>,
    max_val: Option<f64>,
}

impl<const K: u8> AggFold<K> {
    fn new() -> Self {
        Self {
            sums: [0.0; LANE_COUNT],
            products: [1.0; LANE_COUNT],
            num_count: 0,
            item_count: 0,
            min_val: None,
            max_val: None,
        }
    }

    /// Numeric value of `val`: range cells count only numbers, direct arguments are coerced.
    #[inline]
    fn extract_num(
        val: &EvaluatedCellValue,
        from_range: bool,
    ) -> Result<Option<f64>, FormulaError> {
        if from_range {
            return Ok(match val {
                EvaluatedCellValue::Number(n) => Some(*n),
                _ => None,
            });
        }
        match val {
            EvaluatedCellValue::Empty => Ok(None),
            _ => val.as_number().map(Some).ok_or(FormulaError::Value),
        }
    }

    fn fold(
        &mut self,
        v: &EvaluatedCellValue,
        lane: usize,
        from_range: bool,
    ) -> Result<(), FormulaError> {
        if let EvaluatedCellValue::Error(e) = v {
            return Err(*e);
        }
        match K {
            KIND_SUM | KIND_AVERAGE => {
                if let Some(n) = Self::extract_num(v, from_range)? {
                    self.num_count += 1;
                    self.sums[lane & LANE_MASK] += n;
                }
            }
            KIND_PRODUCT => {
                if let Some(n) = Self::extract_num(v, from_range)? {
                    self.num_count += 1;
                    self.products[lane & LANE_MASK] *= n;
                }
            }
            KIND_MIN => {
                if let Some(n) = Self::extract_num(v, from_range)? {
                    self.min_val = Some(self.min_val.map_or(n, |m| m.min(n)));
                }
            }
            KIND_MAX => {
                if let Some(n) = Self::extract_num(v, from_range)? {
                    self.max_val = Some(self.max_val.map_or(n, |m| m.max(n)));
                }
            }
            KIND_COUNT => {
                self.item_count += usize::from(matches!(v, EvaluatedCellValue::Number(_)))
            }
            KIND_COUNTA => self.item_count += usize::from(!matches!(v, EvaluatedCellValue::Empty)),
            KIND_COUNTBLANK => {
                let blank = matches!(v, EvaluatedCellValue::Empty)
                    || matches!(v, EvaluatedCellValue::Text(s) if s.is_empty());
                self.item_count += usize::from(blank);
            }
            _ => {}
        }
        Ok(())
    }

    /// Accounts for `count` unpopulated cells skipped by a sparse walk.
    fn add_skipped_blanks(&mut self, count: usize) {
        if K == KIND_COUNTBLANK {
            self.item_count += count;
        }
    }

    fn finish(&self) -> Result<EvaluatedCellValue, FormulaError> {
        let [a0, a1, a2, a3] = self.sums;
        let [p0, p1, p2, p3] = self.products;
        let total_sum = (a0 + a1) + (a2 + a3);
        let value = match K {
            KIND_SUM => total_sum,
            KIND_PRODUCT if self.num_count == 0 => 0.0,
            KIND_PRODUCT => (p0 * p1) * (p2 * p3),
            KIND_AVERAGE if self.num_count == 0 => return Err(FormulaError::DivByZero),
            KIND_AVERAGE => total_sum / (self.num_count as f64),
            KIND_MIN => self.min_val.unwrap_or(0.0),
            KIND_MAX => self.max_val.unwrap_or(0.0),
            KIND_COUNT | KIND_COUNTA | KIND_COUNTBLANK => self.item_count as f64,
            _ => return Err(FormulaError::Name),
        };
        Ok(EvaluatedCellValue::Number(value))
    }
}

impl SheetEngine {
    pub(crate) fn eval_streaming_range_agg_ctx(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        match name {
            "SUM" => self.agg_eval::<KIND_SUM>(args, ctx, depth),
            "AVERAGE" | "AVG" => self.agg_eval::<KIND_AVERAGE>(args, ctx, depth),
            "PRODUCT" => self.agg_eval::<KIND_PRODUCT>(args, ctx, depth),
            "MIN" => self.agg_eval::<KIND_MIN>(args, ctx, depth),
            "MAX" => self.agg_eval::<KIND_MAX>(args, ctx, depth),
            "COUNT" => self.agg_eval::<KIND_COUNT>(args, ctx, depth),
            "COUNTA" => self.agg_eval::<KIND_COUNTA>(args, ctx, depth),
            "COUNTBLANK" => self.agg_eval::<KIND_COUNTBLANK>(args, ctx, depth),
            _ => self.agg_eval::<KIND_UNKNOWN>(args, ctx, depth),
        }
    }

    fn agg_eval<const K: u8>(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        let mut fold = AggFold::<K>::new();
        for arg in args {
            let arg = arg.peeled();
            match arg {
                Expr::Range(range) => fold = self.agg_range(fold, range, ctx, depth)?,
                Expr::SheetRange { sheet, range } => {
                    fold = self.agg_sheet_range(fold, sheet, range, ctx, depth)?;
                }
                Expr::Cell(_) | Expr::SheetCell { .. } => {
                    let v = self.eval_expr_ctx(arg, ctx, depth);
                    fold.fold(&v, 0, true)?;
                }
                Expr::FuncCall { name: fn_name, .. }
                    if crate::functions_matrix::is_array_fn(fn_name) =>
                {
                    if let Some(m) = self.matrix_expr(arg, ctx, depth)? {
                        for idx in 0..m.data.len() {
                            fold.fold(&m.value_at(idx), idx, true)?;
                        }
                    }
                }
                Expr::FuncCall { name: fn_name, .. }
                    if matches!(fn_name.as_str(), "INDIRECT" | "OFFSET" | "TRANSPOSE") =>
                {
                    fold = self.agg_reference_func(fold, arg, ctx, depth)?;
                }
                _ => {
                    let v = self.eval_expr_ctx(arg, ctx, depth);
                    fold.fold(&v, 0, false)?;
                }
            }
        }
        fold.finish()
    }

    fn agg_block<const K: u8>(
        &self,
        mut fold: AggFold<K>,
        sheet_opt: Option<&str>,
        bounds: Bounds,
        total: usize,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<AggFold<K>, FormulaError> {
        let (min_r, max_r, min_c, max_c) = bounds;
        let check_sparse = total > SPARSE_MIN_CELLS
            || (matches!(K, KIND_COUNT | KIND_COUNTA | KIND_COUNTBLANK)
                && total > COUNT_SPARSE_MIN_CELLS);
        if check_sparse {
            let wb_opt = ctx.workbook;
            let target = match sheet_opt {
                None => Some(self),
                Some(name) if crate::WorkbookEngine::is_same_sheet_identity(name, &self.name) => {
                    Some(self)
                }
                Some(name) => wb_opt.and_then(|wb| wb.sheet(name)),
            };
            if let Some(ts) = target {
                let pop_count = ts.cell_index().count_in_range(min_r, max_r, min_c, max_c);
                if pop_count == 0 {
                    ctx.steps = ctx.steps.saturating_add(1);
                    if ctx.steps & crate::deadline::POLL_STEPS_MASK == 0 {
                        ctx.poll_deadline();
                    }
                    if ctx.steps > Self::MAX_EVAL_STEPS || ctx.expired() {
                        return Err(FormulaError::Value);
                    }
                    fold.add_skipped_blanks(total);
                    return Ok(fold);
                }
                if total > pop_count.saturating_mul(SPARSE_RATIO) {
                    let pop = sorted_population(ts, bounds);
                    let pop_len = pop.len();
                    for (idx, coord) in pop.into_iter().enumerate() {
                        ts.with_coord_value(coord, ctx, depth, |v| fold.fold(v, idx, true))?;
                    }
                    fold.add_skipped_blanks(total.saturating_sub(pop_len));
                    return Ok(fold);
                }
            }
        }
        for r in min_r..=max_r {
            for c in min_c..=max_c {
                let coord = PackedCellCoordinate::new(r, c).map_err(|_| FormulaError::Ref)?;
                self.with_sheet_coord_value(sheet_opt, coord, ctx, depth, |v| {
                    fold.fold(v, c as usize, true)
                })?;
            }
        }
        Ok(fold)
    }

    fn agg_range<const K: u8>(
        &self,
        fold: AggFold<K>,
        range: &CellRange,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<AggFold<K>, FormulaError> {
        self.agg_block(
            fold,
            None,
            range_bounds(range),
            range.cell_count(),
            ctx,
            depth,
        )
    }

    fn agg_sheet_range<const K: u8>(
        &self,
        fold: AggFold<K>,
        sheet: &str,
        range: &CellRange,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<AggFold<K>, FormulaError> {
        self.agg_block(
            fold,
            Some(sheet),
            range_bounds(range),
            range.cell_count(),
            ctx,
            depth,
        )
    }

    /// INDIRECT / OFFSET / TRANSPOSE arguments: stream the referenced range when possible.
    fn agg_reference_func<const K: u8>(
        &self,
        mut fold: AggFold<K>,
        arg: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<AggFold<K>, FormulaError> {
        let Some(spec) = self.extract_streaming_range_ctx(arg, ctx, depth)? else {
            let v = self.eval_expr_ctx(arg, ctx, depth);
            fold.fold(&v, 0, false)?;
            return Ok(fold);
        };
        let (ref sheet_opt, min_r, min_c, rows, cols, transposed) = spec;
        let total = rows.saturating_mul(cols);
        if !transposed && rows > 0 && cols > 0 {
            let max_r = min_r + (rows as u32) - 1;
            let max_c = min_c + (cols as u32) - 1;
            return self.agg_block(
                fold,
                sheet_opt.as_deref(),
                (min_r, max_r, min_c, max_c),
                total,
                ctx,
                depth,
            );
        }
        for dr in 0..rows {
            for dc in 0..cols {
                let v = self.eval_streaming_cell_allow_error(&spec, dr, dc, ctx, depth)?;
                fold.fold(&v, dr * cols + dc, true)?;
            }
        }
        Ok(fold)
    }
}

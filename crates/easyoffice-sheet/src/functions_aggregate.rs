//! `AGGREGATE` (PD-205): the 19 function codes with the 8 ignore options.
//!
//! Reference implementation: `packages/engine-formula/src/functions/math/aggregate/index.ts` and
//! `src/basics/statistical.ts` (`getAggregateResult`, `getArrayValuesByAggregateIgnoreOptions`,
//! `getLargeResult`, `getPercentileIncResult` and the other order statistics) of the Univer formula
//! engine, commit `5a06a7842c8e13fa0646d0733dc3662cf4b330b9`, Apache-2.0, Copyright 2023-present
//! DreamNum Co., Ltd. See `THIRD_PARTY_NOTICES.md`.
//!
//! Codes 1 to 13 take references and code 14 to 19 (`LARGE`, `SMALL`, `PERCENTILE.INC`,
//! `QUARTILE.INC`, `PERCENTILE.EXC`, `QUARTILE.EXC`) take one array and `k`. Options 0 to 3 skip
//! cells that hold a `SUBTOTAL` or `AGGREGATE` (the same set `SUBTOTAL` skips), 2, 3, 6 and 7 skip
//! error values. The formula engine does not know which rows the editor hides, so the options that
//! skip hidden rows (1, 3, 5, 7) skip none, as `SUBTOTAL` 101 to 111 do.

use crate::range_refs::StreamingRangeSpec;
use crate::{
    EvalContext, EvaluatedCellValue as V, Expr, FormulaError as E, PackedCellCoordinate,
    SheetEngine,
};
use std::collections::HashMap;

type Fallible<T> = Result<T, E>;

const MAX_FUNCTION_CODE: i64 = 19;
const MAX_OPTION: i64 = 7;
/// First code of the order statistics that take an array and `k`.
const FIRST_ARRAY_CODE: i64 = 14;
/// Options 0 to 3 skip nested `SUBTOTAL` / `AGGREGATE` cells, 4 to 7 keep them.
const NESTED_OPTIONS: i64 = 4;
/// Ranges above this size with few populated cells are walked through the cell index.
const SPARSE_MIN_CELLS: usize = 1024;

#[derive(Clone, Copy)]
struct Skip {
    errors: bool,
    nested: bool,
}

impl Skip {
    fn from_option(option: i64) -> Self {
        Self {
            errors: matches!(option, 2 | 3 | 6 | 7),
            nested: option < NESTED_OPTIONS,
        }
    }
}

/// Running state of the codes 1 to 13 over the numbers of the references.
#[derive(Default)]
struct Fold {
    count: usize,
    counta: usize,
    n: usize,
    sum: f64,
    product: f64,
    max: f64,
    min: f64,
    mean: f64,
    m2: f64,
    values: Vec<f64>,
}

impl Fold {
    fn new() -> Self {
        Self {
            product: 1.0,
            ..Self::default()
        }
    }

    fn add_number(&mut self, x: f64, keep_values: bool) {
        self.n += 1;
        if self.n == 1 {
            self.max = x;
            self.min = x;
        } else {
            self.max = self.max.max(x);
            self.min = self.min.min(x);
        }
        self.sum += x;
        self.product *= x;
        let delta = x - self.mean;
        self.mean += delta / self.n as f64;
        self.m2 += delta * (x - self.mean);
        if keep_values {
            self.values.push(x);
        }
    }

    fn variance(&self, sample: bool) -> Fallible<f64> {
        let divisor = self.n as f64 - f64::from(u8::from(sample));
        if self.n == 0 || divisor <= 0.0 {
            Err(E::DivByZero)
        } else {
            Ok(self.m2 / divisor)
        }
    }

    fn result(&mut self, code: i64) -> Fallible<f64> {
        match code {
            1 if self.n == 0 => Err(E::DivByZero),
            1 => Ok(self.sum / self.n as f64),
            2 => Ok(self.count as f64),
            3 => Ok(self.counta as f64),
            4 => Ok(self.max),
            5 => Ok(self.min),
            6 => Ok(if self.n == 0 { 0.0 } else { self.product }),
            7 => self.variance(true).map(f64::sqrt),
            8 => self.variance(false).map(f64::sqrt),
            9 => Ok(self.sum),
            10 => self.variance(true),
            11 => self.variance(false),
            12 => self.median(),
            _ => self.mode(),
        }
    }

    fn median(&mut self) -> Fallible<f64> {
        let n = self.values.len();
        if n == 0 {
            return Err(E::Num);
        }
        let mid = n / 2;
        let (_, upper, _) = self.values.select_nth_unstable_by(mid, f64::total_cmp);
        let upper = *upper;
        if n % 2 == 1 {
            return Ok(upper);
        }
        let lower = self.values[..mid]
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        Ok((lower + upper) / 2.0)
    }

    /// `MODE.SNGL`: the value with the most repeats, the earliest one on a tie; `#N/A` without a repeat.
    fn mode(&self) -> Fallible<f64> {
        let mut counts: HashMap<u64, (usize, usize)> = HashMap::with_capacity(self.values.len());
        for (order, &x) in self.values.iter().enumerate() {
            counts
                .entry((x + 0.0).to_bits())
                .and_modify(|e| e.0 += 1)
                .or_insert((1, order));
        }
        let best = counts
            .values()
            .max_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)))
            .copied();
        match best {
            Some((count, order)) if count > 1 => Ok(self.values[order]),
            _ => Err(E::Na),
        }
    }
}

/// `LARGE`, `SMALL`, `PERCENTILE.*` and `QUARTILE.*` of the codes 14 to 19.
fn order_statistic(code: i64, values: &mut [f64], k: f64) -> Fallible<f64> {
    let n = values.len();
    match code {
        14 | 15 => {
            if k < 1.0 || k > n as f64 {
                return Err(E::Num);
            }
            let rank = if code == 14 { k.ceil() } else { k.floor() } as usize;
            let index = if code == 14 { n - rank } else { rank - 1 };
            Ok(*values.select_nth_unstable_by(index, f64::total_cmp).1)
        }
        _ => {
            let exclusive = code >= 18;
            let quartile = code == 17 || code == 19;
            let range_ok = match (quartile, exclusive) {
                (false, false) => (0.0..=1.0).contains(&k),
                (true, false) => (0.0..=4.0).contains(&k),
                (true, true) => k > 0.0 && k < 4.0,
                (false, true) => true,
            };
            if !range_ok {
                return Err(E::Num);
            }
            let fraction = if quartile { k / 4.0 } else { k };
            let nf = n as f64;
            let position = if exclusive {
                if fraction < 1.0 / (nf + 1.0) || fraction > 1.0 - 1.0 / (nf + 1.0) {
                    return Err(E::Num);
                }
                (fraction * (nf + 1.0) - 1.0).max(0.0)
            } else {
                fraction * (nf - 1.0)
            };
            let lower = position.floor();
            let (_, &mut low_value, upper_part) =
                values.select_nth_unstable_by(lower as usize, f64::total_cmp);
            let rest = position - lower;
            if rest == 0.0 {
                return Ok(low_value);
            }
            let high_value = upper_part.iter().copied().fold(f64::INFINITY, f64::min);
            Ok(low_value + rest * (high_value - low_value))
        }
    }
}

impl SheetEngine {
    /// Calls `f` with the value of each cell of the range that the skip rules keep. A big range
    /// with few cells (a whole column) visits only the populated cells, in row-major order.
    fn walk_aggregate_range(
        &self,
        spec: &StreamingRangeSpec<'_>,
        skip: Skip,
        ctx: &mut EvalContext<'_>,
        depth: usize,
        f: &mut dyn FnMut(V) -> Fallible<()>,
    ) -> Fallible<()> {
        let (sheet, min_r, min_c, rows, cols, transposed) = spec;
        let sheet = sheet.as_deref();
        let total = rows.saturating_mul(*cols);
        let populated = self
            .resolve_sheet_ref(sheet, ctx)
            .filter(|target| {
                !transposed && total > SPARSE_MIN_CELLS && total > target.cells.len() * 2
            })
            .map(|target| {
                let last_row = min_r + (*rows as u32).saturating_sub(1);
                let last_col = min_c + (*cols as u32).saturating_sub(1);
                target
                    .cell_index()
                    .range_coords(*min_r, last_row, *min_c, last_col)
            });
        let mut visit = |coord: PackedCellCoordinate, ctx: &mut EvalContext<'_>| {
            if skip.nested && self.is_nested_subtotal_cell(sheet, coord, ctx) {
                return Ok(());
            }
            f(self.eval_sheet_coord(sheet, coord, ctx, depth))
        };
        if let Some(coords) = populated {
            for coord in coords {
                visit(coord, ctx)?;
            }
            return Ok(());
        }
        for dr in 0..*rows {
            for dc in 0..*cols {
                if ctx.steps > Self::MAX_EVAL_STEPS {
                    return Err(E::Value);
                }
                let (r, c) = if *transposed {
                    (min_r + dc as u32, min_c + dr as u32)
                } else {
                    (min_r + dr as u32, min_c + dc as u32)
                };
                visit(PackedCellCoordinate::new(r, c).map_err(|_| E::Ref)?, ctx)?;
            }
        }
        Ok(())
    }

    fn aggregate_fold(
        &self,
        code: i64,
        skip: Skip,
        specs: &[StreamingRangeSpec<'_>],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<f64> {
        let mut fold = Fold::new();
        let keep_values = code >= 12;
        for spec in specs {
            self.walk_aggregate_range(spec, skip, ctx, depth, &mut |v| {
                match v {
                    V::Number(_) if code == 2 => fold.count += 1,
                    _ if code == 2 => {}
                    V::Empty if code == 3 => {}
                    _ if code == 3 => fold.counta += 1,
                    V::Error(e) if !skip.errors => return Err(e),
                    V::Number(x) => fold.add_number(x, keep_values),
                    _ => {}
                }
                Ok(())
            })?;
        }
        fold.result(code)
    }

    /// The numbers of the array argument of codes 14 to 19.
    fn aggregate_values(
        &self,
        arg: &Expr,
        skip: Skip,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<Vec<f64>> {
        let mut values = Vec::new();
        let mut take = |v: V| match v {
            V::Number(x) => {
                values.push(x);
                Ok(())
            }
            V::Error(e) if !skip.errors => Err(e),
            _ => Ok(()),
        };
        if let Some(spec) = self.extract_streaming_range_ctx(arg, ctx, depth)? {
            self.walk_aggregate_range(&spec, skip, ctx, depth, &mut take)?;
        } else {
            // a value written into the call: a logical value counts as 1 or 0
            take(match self.eval_arg_val(arg, ctx, depth) {
                Ok(V::Boolean(b)) => V::Number(f64::from(u8::from(b))),
                Ok(other) => other,
                Err(e) => V::Error(e),
            })?;
        }
        if values.is_empty() {
            return Err(E::Num);
        }
        Ok(values)
    }

    fn aggregate_k(&self, arg: &Expr, ctx: &mut EvalContext<'_>, depth: usize) -> Fallible<f64> {
        if let Some((.., rows, cols, _)) = self.extract_streaming_range_ctx(arg, ctx, depth)? {
            if rows * cols > 1 {
                return Err(E::Value);
            }
        }
        self.eval_arg_num_blank_zero(arg, ctx, depth)
    }

    pub(crate) fn eval_aggregate(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<V> {
        if args.len() < 3 {
            return Err(E::Value);
        }
        let code = self.eval_arg_num_blank_zero(&args[0], ctx, depth)?.floor() as i64;
        let option = self.eval_arg_num_blank_zero(&args[1], ctx, depth)?.floor() as i64;
        if !(1..=MAX_FUNCTION_CODE).contains(&code) || !(0..=MAX_OPTION).contains(&option) {
            return Err(E::Value);
        }
        let skip = Skip::from_option(option);
        let refs = &args[2..];
        if code >= FIRST_ARRAY_CODE {
            if refs.len() != 2 {
                return Err(E::Value);
            }
            let k = self.aggregate_k(&refs[1], ctx, depth)?;
            let mut values = self.aggregate_values(&refs[0], skip, ctx, depth)?;
            return order_statistic(code, &mut values, k).map(V::Number);
        }
        let mut specs = Vec::with_capacity(refs.len());
        for arg in refs {
            // a value that is not a reference makes the call `#VALUE!`
            specs.push(
                self.extract_streaming_range_ctx(arg, ctx, depth)?
                    .ok_or(E::Value)?,
            );
        }
        self.aggregate_fold(code, skip, &specs, ctx, depth)
            .map(V::Number)
    }
}

//! Trend and distribution-shape functions: `LINEST`, `LOGEST`, `TREND`, `GROWTH`, `FREQUENCY`,
//! `MODE.MULT`, `PROB` (PD-201).
//!
//! Reference implementation: `packages/engine-formula/src/functions/statistical/{linest,logest,
//! trend,growth,frequency,mode-mult,prob}/index.ts` and `src/basics/statistical.ts`
//! (`checkKnownsArrayDimensions`, `getKnownsArrayValues`) of the Univer formula engine, commit
//! `5a06a7842c8e13fa0646d0733dc3662cf4b330b9`, Apache-2.0, Copyright 2023-present DreamNum Co.,
//! Ltd. See `THIRD_PARTY_NOTICES.md`. The regression numerics are in `functions_regression.rs`.
//!
//! Array results follow PD-195: a matrix result is its top-left cell in a cell, the whole matrix
//! under `INDEX` and the aggregates. A cell of such a matrix that holds `#N/A` or `#NUM!` (the
//! `LINEST` statistics) is a quiet NaN whose payload names the error ([`error_cell`]).
//!
//! Differences from the original: `LOGEST` and `GROWTH` give `#NUM!` for a known y that is not
//! positive; `PROB` compares the probability sum with 1 within 1e-9. An operand of the regression
//! functions and the bins and probabilities are read whole and limited to [`MAX_MATRIX_CELLS`];
//! the data of `FREQUENCY` and `MODE.MULT` are streamed, so a whole column is fine.

use crate::functions_matrix::{Matrix, MAX_MATRIX_CELLS};
use crate::functions_regression::{
    estimate_multiple, estimate_simple, fit, serial_numbers, slope_intercept, to_matrix, transpose,
};
use crate::{EvalContext, EvaluatedCellValue as V, Expr, FormulaError as E, SheetEngine};

type Fallible<T> = Result<T, E>;
/// Known y, known x and new x values as rows.
type Operands = (Vec<Vec<f64>>, Vec<Vec<f64>>, Vec<Vec<f64>>);

/// Largest total of the probabilities of `PROB` that still counts as 1.
const PROB_SUM_TOLERANCE: f64 = 1e-9;

const NA_BITS: u64 = 0x7FF8_0000_0000_0001;
const NUM_BITS: u64 = 0x7FF8_0000_0000_0002;

/// The matrix cell that stands for `#N/A`.
pub(crate) fn na_cell() -> f64 {
    f64::from_bits(NA_BITS)
}

/// The matrix cell that stands for `#NUM!`.
pub(crate) fn error_cell(error: E) -> f64 {
    f64::from_bits(if error == E::Na { NA_BITS } else { NUM_BITS })
}

/// A matrix cell as a value: any NaN is the error its payload names (`#NUM!` for an arithmetic NaN).
pub(crate) fn cell_value(x: f64) -> V {
    if !x.is_nan() {
        return V::Number(x);
    }
    V::Error(if x.to_bits() == NA_BITS {
        E::Na
    } else {
        E::Num
    })
}

pub(crate) fn is_trend_array_fn(name: &str) -> bool {
    matches!(
        name,
        "LINEST" | "LOGEST" | "TREND" | "GROWTH" | "FREQUENCY" | "MODE.MULT"
    )
}

/// An evaluated operand: a range, an array function result or a single value.
struct Grid {
    rows: usize,
    cols: usize,
    vals: Vec<V>,
}

impl Grid {
    fn dims(&self) -> (usize, usize) {
        (self.rows, self.cols)
    }

    /// The numbers of a regression operand: an error, blank, logical or text cell is `#VALUE!`.
    fn known_values(&self) -> Fallible<Vec<Vec<f64>>> {
        let mut out = vec![Vec::with_capacity(self.cols); self.rows];
        for (i, v) in self.vals.iter().enumerate() {
            match v {
                V::Number(n) => out[i / self.cols].push(*n),
                _ => return Err(E::Value),
            }
        }
        Ok(out)
    }
}

/// The two shapes a regression takes, by the reference's rule (`knownYs` one row against several
/// predictor rows, or one column against several predictor columns).
fn is_multiple(ys: &[Vec<f64>], xs: &[Vec<f64>]) -> bool {
    (ys.len() == 1 && xs.len() > 1) || (ys[0].len() == 1 && xs[0].len() > 1)
}

/// `#N/A` when there are fewer observations than predictors.
fn check_observations(ys: &[Vec<f64>], xs: &[Vec<f64>], constb: bool) -> Fallible<()> {
    let extra = usize::from(constb);
    if (ys.len() == 1 && xs.len() > 1 && xs.len() + extra > ys[0].len())
        || (ys[0].len() == 1 && xs[0].len() > 1 && xs[0].len() + extra > ys.len())
    {
        return Err(E::Na);
    }
    Ok(())
}

/// `checkKnownsArrayDimensions`: `#REF!` when the shapes of the operands do not fit.
fn check_dimensions(
    ys: (usize, usize),
    xs: Option<(usize, usize)>,
    new_xs: Option<(usize, usize)>,
) -> Fallible<()> {
    let (yr, yc) = ys;
    let (xr, xc) = xs.unwrap_or(ys);
    if xs.is_some()
        && ((yr == 1 && xc != yc)
            || (yc == 1 && xr != yr)
            || (yr != 1 && yc != 1 && (xr != yr || xc != yc)))
    {
        return Err(E::Ref);
    }
    if let Some((nr, nc)) = new_xs {
        if (yr == 1 && xr > 1 && nr != xr) || (yc == 1 && xc > 1 && nc != xc) {
            return Err(E::Ref);
        }
    }
    Ok(())
}

impl SheetEngine {
    /// One operand as a grid: errors stay in the grid so the caller decides their meaning.
    fn grid_operand(&self, expr: &Expr, ctx: &mut EvalContext<'_>, depth: usize) -> Fallible<Grid> {
        if let Expr::ErrorLiteral(e) = expr {
            return Ok(Grid {
                rows: 1,
                cols: 1,
                vals: vec![V::Error(*e)],
            });
        }
        if let Some(m) = self.matrix_expr(expr, ctx, depth)? {
            let vals = (0..m.data.len()).map(|i| m.value_at(i)).collect();
            return Ok(Grid {
                rows: m.rows,
                cols: m.cols,
                vals,
            });
        }
        let Some(spec) = self.extract_streaming_range_ctx(expr, ctx, depth)? else {
            let v = self.eval_expr_ctx(expr, ctx, depth);
            return Ok(Grid {
                rows: 1,
                cols: 1,
                vals: vec![v],
            });
        };
        let (_, _, _, rows, cols, _) = spec;
        if rows.saturating_mul(cols) > MAX_MATRIX_CELLS {
            return Err(E::Value);
        }
        let mut vals = Vec::with_capacity(rows * cols);
        for r in 0..rows {
            for c in 0..cols {
                vals.push(self.eval_streaming_cell_allow_error(&spec, r, c, ctx, depth)?);
            }
        }
        Ok(Grid { rows, cols, vals })
    }

    /// Calls `f` with every number of the operand (other kinds are skipped, an error ends the
    /// call); a range over [`MAX_MATRIX_CELLS`] cells is streamed instead of read whole. `Ok(true)`
    /// when the operand is one blank value.
    fn for_each_number(
        &self,
        expr: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
        mut f: impl FnMut(f64),
    ) -> Fallible<bool> {
        if let Some((_, _, _, rows, cols, _)) =
            self.extract_streaming_range_ctx(expr, ctx, depth)?
        {
            if rows.saturating_mul(cols) > MAX_MATRIX_CELLS {
                self.visit_args_streaming(std::slice::from_ref(expr), ctx, depth, |v| {
                    if let V::Number(n) = v {
                        f(n);
                    }
                    Ok(())
                })?;
                return Ok(false);
            }
        }
        let grid = self.grid_operand(expr, ctx, depth)?;
        for v in &grid.vals {
            match v {
                V::Error(e) => return Err(*e),
                V::Number(n) => f(*n),
                _ => {}
            }
        }
        Ok(grid.vals.len() == 1 && grid.vals[0] == V::Empty)
    }

    /// An optional logical argument: a number is true unless 0, text must be a number, a blank is false.
    fn flag_arg(
        &self,
        arg: Option<&Expr>,
        default: bool,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<bool> {
        let Some(arg) = arg else { return Ok(default) };
        let grid = self.grid_operand(arg, ctx, depth)?;
        match &grid.vals[0] {
            V::Error(e) => Err(*e),
            V::Boolean(b) => Ok(*b),
            V::Empty => Ok(false),
            other => other.as_number().map(|n| n != 0.0).ok_or(E::Value),
        }
    }

    /// The known y, known x and (optionally) new x operands of a regression function.
    fn regression_operands(
        &self,
        args: &[Expr],
        with_new_x: bool,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<Operands> {
        if args.is_empty() || args.len() > 4 {
            return Err(E::Value);
        }
        let ys = self.grid_operand(&args[0], ctx, depth)?;
        let given =
            |arg: Option<&Expr>, me: &Self, ctx: &mut EvalContext<'_>| -> Fallible<Option<Grid>> {
                match arg {
                    None | Some(Expr::Missing) => Ok(None),
                    Some(arg) => {
                        let g = me.grid_operand(arg, ctx, depth)?;
                        Ok(if g.vals.len() == 1 && g.vals[0] == V::Empty {
                            None
                        } else {
                            Some(g)
                        })
                    }
                }
            };
        let xs = given(args.get(1), self, ctx)?;
        let new_xs = if with_new_x {
            given(args.get(2), self, ctx)?
        } else {
            None
        };
        check_dimensions(
            ys.dims(),
            xs.as_ref().map(Grid::dims),
            new_xs.as_ref().map(Grid::dims),
        )?;
        let y_values = ys.known_values()?;
        let x_values = match &xs {
            Some(g) => g.known_values()?,
            None => serial_numbers(ys.rows, ys.cols),
        };
        let new_x_values = match &new_xs {
            Some(g) => g.known_values()?,
            None => x_values.clone(),
        };
        Ok((y_values, x_values, new_x_values))
    }

    pub(crate) fn eval_trend_matrix(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<Matrix> {
        match name {
            "LINEST" | "LOGEST" => {
                let (ys, xs, _) = self.regression_operands(args, false, ctx, depth)?;
                let constb = self.flag_arg(args.get(2), true, ctx, depth)?;
                let stats = self.flag_arg(args.get(3), false, ctx, depth)?;
                let exponential = name == "LOGEST";
                if is_multiple(&ys, &xs) {
                    check_observations(&ys, &xs, constb)?;
                    estimate_multiple(&ys, &xs, constb, stats, exponential)
                } else {
                    estimate_simple(&ys, &xs, constb, stats, exponential)
                }
            }
            "TREND" | "GROWTH" => {
                let (ys, xs, new_xs) = self.regression_operands(args, true, ctx, depth)?;
                let constb = self.flag_arg(args.get(3), true, ctx, depth)?;
                self.predict(&ys, &xs, &new_xs, constb, name == "GROWTH")
            }
            "FREQUENCY" => self.eval_frequency(args, ctx, depth),
            "MODE.MULT" => self.eval_mode_mult(args, ctx, depth),
            _ => Err(E::Name),
        }
    }

    fn predict(
        &self,
        ys: &[Vec<f64>],
        xs: &[Vec<f64>],
        new_xs: &[Vec<f64>],
        constb: bool,
        exponential: bool,
    ) -> Fallible<Matrix> {
        if !is_multiple(ys, xs) {
            let (m, b, _) = slope_intercept(
                &xs.iter().flatten().copied().collect::<Vec<_>>(),
                &ys.iter().flatten().copied().collect::<Vec<_>>(),
                constb,
                exponential,
            )?;
            if !exponential && m.is_nan() {
                return Err(E::Na);
            }
            let rows: Vec<Vec<f64>> = new_xs
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|&x| {
                            if exponential {
                                b * m.powf(x)
                            } else {
                                m * x + b
                            }
                        })
                        .collect()
                })
                .collect();
            return Ok(to_matrix(&rows));
        }
        check_observations(ys, xs, constb)?;
        let one_row = ys.len() == 1 && ys[0].len() > 1;
        let f = fit(ys, xs, new_xs, constb, exponential)?;
        let cl = f.coefficients.len();
        let b = f.coefficients[cl - 1];
        let column: Vec<Vec<f64>> = f
            .new_x
            .iter()
            .map(|row| {
                let terms = (0..cl - 1)
                    .rev()
                    .map(|j| (f.coefficients[cl - 2 - j], row[j]));
                vec![if exponential {
                    terms.fold(b, |acc, (m, x)| acc * m.powf(x))
                } else {
                    terms.fold(b, |acc, (m, x)| acc + m * x)
                }]
            })
            .collect();
        Ok(to_matrix(&if one_row {
            transpose(&column)
        } else {
            column
        }))
    }

    fn eval_frequency(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<Matrix> {
        if args.len() != 2 {
            return Err(E::Value);
        }
        let mut values = Vec::new();
        let data_is_blank = self.for_each_number(&args[0], ctx, depth, |n| values.push(n))?;
        let bins = self.grid_operand(&args[1], ctx, depth)?;
        if data_is_blank || (bins.vals.len() == 1 && bins.vals[0] == V::Empty) {
            return Err(E::Value);
        }
        let mut edges: Vec<(f64, usize)> = bins
            .vals
            .iter()
            .filter_map(|v| if let V::Number(n) = v { Some(*n) } else { None })
            .enumerate()
            .map(|(i, n)| (n, i))
            .collect();
        if edges.is_empty() {
            edges.push((0.0, 0));
        }
        edges.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut counts = vec![0.0; edges.len() + 1];
        for v in values {
            // the first edge not below the value owns it; past the last edge: the overflow slot
            let at = edges.partition_point(|&(edge, _)| edge < v);
            counts[edges.get(at).map_or(edges.len(), |&(_, original)| original)] += 1.0;
        }
        Ok(Matrix {
            rows: counts.len(),
            cols: 1,
            data: counts,
        })
    }

    fn eval_mode_mult(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<Matrix> {
        if args.is_empty() {
            return Err(E::Value);
        }
        let mut counts: std::collections::HashMap<u64, (usize, usize, f64)> =
            std::collections::HashMap::new();
        let mut max_count = 1;
        for arg in args {
            if let Expr::Text(text) = arg {
                if crate::coercion::parse_value_text(text).is_none() {
                    return Err(E::Value);
                }
            }
            self.for_each_number(arg, ctx, depth, |n| {
                let key = if n == 0.0 {
                    0.0f64.to_bits()
                } else {
                    n.to_bits()
                };
                let order = counts.len();
                let entry = counts.entry(key).or_insert((0, order, n));
                entry.0 += 1;
                max_count = max_count.max(entry.0);
            })?;
        }
        if counts.is_empty() || max_count == 1 {
            return Err(E::Na);
        }
        let mut modes: Vec<(usize, f64)> = counts
            .values()
            .filter(|(count, _, _)| *count == max_count)
            .map(|&(_, order, value)| (order, value))
            .collect();
        modes.sort_by_key(|&(order, _)| order);
        let data: Vec<f64> = modes.into_iter().map(|(_, v)| v).collect();
        Ok(Matrix {
            rows: data.len(),
            cols: 1,
            data,
        })
    }

    pub(crate) fn eval_prob(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<V> {
        if !(3..=4).contains(&args.len()) {
            return Err(E::Value);
        }
        let x_grid = self.grid_operand(&args[0], ctx, depth)?;
        let p_grid = self.grid_operand(&args[1], ctx, depth)?;
        let scalar = |i: usize, me: &Self, ctx: &mut EvalContext<'_>| -> Fallible<f64> {
            let g = me.grid_operand(&args[i], ctx, depth)?;
            match &g.vals[0] {
                V::Error(e) => Err(*e),
                V::Boolean(b) => Ok(f64::from(u8::from(*b))),
                V::Empty => Ok(0.0),
                other => other.as_number().ok_or(E::Value),
            }
        };
        let lower = scalar(2, self, ctx)?;
        let upper = match args.get(3) {
            None | Some(Expr::Missing) => lower,
            Some(arg) => {
                let g = self.grid_operand(arg, ctx, depth)?;
                if g.vals[0] == V::Empty {
                    lower
                } else {
                    scalar(3, self, ctx)?
                }
            }
        };
        for g in [&x_grid, &p_grid] {
            if let Some(V::Error(e)) = g.vals.iter().find(|v| matches!(v, V::Error(_))) {
                return Err(*e);
            }
        }
        if (x_grid.vals.len() == 1 && x_grid.vals[0] == V::Empty)
            || (p_grid.vals.len() == 1 && p_grid.vals[0] == V::Empty)
        {
            return Err(E::Value);
        }
        if x_grid.vals.len() != p_grid.vals.len() {
            return Err(E::Na);
        }
        let pairs: Vec<(f64, f64)> = x_grid
            .vals
            .iter()
            .zip(&p_grid.vals)
            .filter_map(|(x, p)| match (x, p) {
                (V::Number(x), V::Number(p)) => Some((*x, *p)),
                _ => None,
            })
            .collect();
        if pairs.is_empty() {
            return Err(E::DivByZero);
        }
        if (pairs.iter().map(|&(_, p)| p).sum::<f64>() - 1.0).abs() > PROB_SUM_TOLERANCE {
            return Err(E::Num);
        }
        Ok(V::Number(
            pairs
                .iter()
                .filter(|&&(x, _)| x >= lower && x <= upper)
                .map(|&(_, p)| p)
                .sum(),
        ))
    }
}

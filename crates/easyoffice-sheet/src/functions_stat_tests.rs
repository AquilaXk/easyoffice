//! Significance tests and weighted statistics (PD-205): `CHISQ.TEST`, `F.TEST`, `T.TEST`,
//! `AVERAGE.WEIGHTED`, `MARGINOFERROR` and the legacy names `CHITEST`, `FTEST`, `TTEST`.
//!
//! Reference implementation: `packages/engine-formula/src/functions/statistical/{chisq-test,f-test,
//! t-test,average-weighted,marginoferror}/index.ts` and `src/basics/statistical.ts`
//! (`getTwoArrayNumberValues`) of the Univer formula engine, commit
//! `5a06a7842c8e13fa0646d0733dc3662cf4b330b9`, Apache-2.0, Copyright 2023-present DreamNum Co.,
//! Ltd. See `THIRD_PARTY_NOTICES.md`.
//!
//! Only numbers count in a range: blanks, logical values and text are skipped (the reference also
//! reads numeric text as a number; PD-205 lists the difference). A range that is one cell is a
//! scalar, as in the reference.

use crate::functions_dist::{dist_value, t_cdf};
use crate::functions_dist_more::f_sf;
use crate::special::gamma_regularized;
use crate::{EvalContext, EvaluatedCellValue as V, Expr, FormulaError as E, SheetEngine};

type Fallible<T> = Result<T, E>;

/// A rectangle of values read from a range argument.
struct Grid {
    rows: usize,
    cols: usize,
    cells: Vec<V>,
}

impl Grid {
    fn len(&self) -> usize {
        self.rows * self.cols
    }

    /// The numbers of the grid in row-major order.
    fn numbers(&self) -> Vec<f64> {
        self.cells
            .iter()
            .filter_map(|v| match v {
                V::Number(x) => Some(*x),
                _ => None,
            })
            .collect()
    }
}

/// Sum of squares about the mean and the count.
fn spread(values: &[f64]) -> (f64, f64) {
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    (values.iter().map(|x| (x - mean) * (x - mean)).sum(), n)
}

fn chisq_test(actual: &Grid, expected: &Grid) -> Fallible<f64> {
    if actual.len() == 1 || expected.len() == 1 {
        let blank = matches!(actual.cells[0], V::Empty) || matches!(expected.cells[0], V::Empty);
        return Err(if blank { E::Value } else { E::Na });
    }
    if actual.len() != expected.len() {
        return Err(E::Na);
    }
    let mut statistic = 0.0;
    let mut paired = false;
    for (a, e) in actual.cells.iter().zip(&expected.cells) {
        if let (V::Number(a), V::Number(e)) = (a, e) {
            paired = true;
            if *e == 0.0 {
                return Err(E::DivByZero);
            }
            statistic += (a - e) * (a - e) / e;
        }
    }
    if !paired {
        return Err(E::DivByZero);
    }
    let df = match (actual.rows, actual.cols) {
        (1, c) => c - 1,
        (r, 1) => r - 1,
        (r, c) => (r - 1) * (c - 1),
    };
    Ok(gamma_regularized(df as f64 / 2.0, statistic / 2.0).1)
}

/// Variance and degrees of freedom of one `F.TEST` sample.
fn f_sample(grid: &Grid) -> Fallible<(f64, f64)> {
    if grid.len() == 1 {
        return Err(match grid.cells[0] {
            V::Error(e) => e,
            V::Empty => E::Value,
            _ => E::DivByZero,
        });
    }
    let values = grid.numbers();
    if values.len() <= 1 {
        return Err(E::DivByZero);
    }
    let (squares, n) = spread(&values);
    let variance = squares / (n - 1.0);
    if variance == 0.0 {
        return Err(E::DivByZero);
    }
    Ok((variance, n - 1.0))
}

fn f_test(first: &Grid, second: &Grid) -> Fallible<f64> {
    let (v1, d1) = f_sample(first)?;
    let (v2, d2) = f_sample(second)?;
    let p = 2.0 * f_sf(v1 / v2, d1, d2);
    Ok(if p > 1.0 { 2.0 - p } else { p })
}

/// Pairs of the two samples for the paired test, or the error the reference gives.
fn paired_samples(a: &Grid, b: &Grid) -> Fallible<(Vec<f64>, Vec<f64>)> {
    let single = a.len() == 1 || b.len() == 1;
    if single && (matches!(a.cells[0], V::Empty) || matches!(b.cells[0], V::Empty)) {
        return Err(E::Value);
    }
    if a.len() != b.len() {
        return Err(E::Na);
    }
    let (mut first, mut second) = (Vec::new(), Vec::new());
    for (x, y) in a.cells.iter().zip(&b.cells) {
        if let (V::Number(x), V::Number(y)) = (x, y) {
            first.push(*x);
            second.push(*y);
        }
    }
    if first.len() < 2 {
        return Err(E::DivByZero);
    }
    Ok((first, second))
}

/// A single-cell sample that is blank is `#VALUE!`; otherwise its numbers.
fn sample_numbers(grid: &Grid) -> Fallible<Vec<f64>> {
    if grid.len() == 1 && matches!(grid.cells[0], V::Empty) {
        return Err(E::Value);
    }
    Ok(grid.numbers())
}

/// The statistic `x` and degrees of freedom of the three `T.TEST` types.
fn t_statistic(first: &[f64], second: &[f64], kind: i64) -> Fallible<(f64, f64)> {
    let (n1, n2) = (first.len() as f64, second.len() as f64);
    let sum = |v: &[f64]| v.iter().sum::<f64>();
    let sum_sq = |v: &[f64]| v.iter().map(|x| x * x).sum::<f64>();
    match kind {
        1 => {
            let sum_diff = sum(first) - sum(second);
            let sum_sq_diff: f64 = first
                .iter()
                .zip(second)
                .map(|(a, b)| (a - b) * (a - b))
                .sum();
            let df = n1 - 1.0;
            let den = n1 * sum_sq_diff - sum_diff * sum_diff;
            if den == 0.0 {
                return Err(E::DivByZero);
            }
            Ok((sum_diff.abs() * (df / den).sqrt(), df))
        }
        2 => {
            let (s1, s2) = (sum(first), sum(second));
            let den = ((sum_sq(first) - s1 * s1 / n1) + (sum_sq(second) - s2 * s2 / n2)).sqrt();
            if den == 0.0 {
                return Err(E::DivByZero);
            }
            let df = n1 - 1.0 + n2 - 1.0;
            let scale = (n1 * n2 * df / (n1 + n2)).sqrt();
            Ok(((s1 / n1 - s2 / n2).abs() / den * scale, df))
        }
        _ => {
            let (s1, s2) = (sum(first), sum(second));
            let v1 = (sum_sq(first) - s1 * s1 / n1) / (n1 * (n1 - 1.0));
            let v2 = (sum_sq(second) - s2 * s2 / n2) / (n2 * (n2 - 1.0));
            if v1 + v2 == 0.0 {
                return Err(E::DivByZero);
            }
            let share = v1 / (v1 + v2);
            let df =
                1.0 / (share * share / (n1 - 1.0) + (1.0 - share) * (1.0 - share) / (n2 - 1.0));
            Ok(((s1 / n1 - s2 / n2).abs() / (v1 + v2).sqrt(), df))
        }
    }
}

fn average_weighted(pairs: &[(Grid, Option<Grid>)], odd: bool) -> Fallible<f64> {
    let mut mismatch = false;
    for (values, weights) in pairs {
        if let Some(weights) = weights {
            mismatch |= values.rows != weights.rows || values.cols != weights.cols;
        }
    }
    if mismatch {
        return Err(E::Value);
    }
    if odd {
        return Err(E::Na);
    }
    let (mut sum, mut total) = (0.0, 0.0);
    for (values, weights) in pairs {
        let Some(weights) = weights else { continue };
        for (value, weight) in values.cells.iter().zip(&weights.cells) {
            match (value, weight) {
                (V::Number(v), V::Number(w)) if *w >= 0.0 => {
                    sum += v * w;
                    total += w;
                }
                (V::Number(_), _) | (_, V::Number(_)) => return Err(E::Value),
                _ => {}
            }
        }
    }
    if total == 0.0 {
        return Err(E::DivByZero);
    }
    Ok(sum / total)
}

impl SheetEngine {
    fn read_grid(&self, arg: &Expr, ctx: &mut EvalContext<'_>, depth: usize) -> Fallible<Grid> {
        let (rows, cols, cells, _) = self.eval_expr_2d_grid(arg, ctx, depth)?;
        Ok(Grid { rows, cols, cells })
    }

    /// The error cells of an `AVERAGE.WEIGHTED` argument list come out first, in argument order.
    fn read_weighted_pairs(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<Vec<(Grid, Option<Grid>)>> {
        let mut pairs = Vec::with_capacity(args.len().div_ceil(2));
        for chunk in args.chunks(2) {
            let values = self.read_grid(&chunk[0], ctx, depth)?;
            let weights = match chunk.get(1) {
                Some(arg) => Some(self.read_grid(arg, ctx, depth)?),
                None => None,
            };
            pairs.push((values, weights));
        }
        Ok(pairs)
    }

    fn t_test(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> Fallible<f64> {
        let first = self.read_grid(&args[0], ctx, depth)?;
        let second = self.read_grid(&args[1], ctx, depth)?;
        let tails = self.eval_arg_num_blank_zero(&args[2], ctx, depth)?.floor();
        let kind = self.eval_arg_num_blank_zero(&args[3], ctx, depth)?.floor();
        if ![1.0, 2.0].contains(&tails) || ![1.0, 2.0, 3.0].contains(&kind) {
            return Err(E::Num);
        }
        let (a, b) = if kind == 1.0 {
            paired_samples(&first, &second)?
        } else {
            (sample_numbers(&first)?, sample_numbers(&second)?)
        };
        if a.len() < 2 || b.len() < 2 {
            return Err(E::DivByZero);
        }
        let (x, df) = t_statistic(&a, &b, kind as i64)?;
        let tail = t_cdf(-x, df);
        let p = if tails == 2.0 { tail * 2.0 } else { tail };
        if p.is_finite() {
            Ok(p)
        } else {
            Err(E::Num)
        }
    }

    fn margin_of_error(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<f64> {
        let values = self.read_grid(&args[0], ctx, depth)?.numbers();
        let confidence = self.eval_arg_num_blank_zero(&args[1], ctx, depth)?;
        if confidence <= 0.0 || confidence >= 1.0 {
            return Err(E::Num);
        }
        if values.len() < 2 {
            return Err(E::DivByZero);
        }
        let (squares, n) = spread(&values);
        let sd = (squares / (n - 1.0)).sqrt();
        if sd <= 0.0 {
            return Err(E::Num);
        }
        let t = dist_value("T.INV.2T", &[1.0 - confidence, n - 1.0])?;
        Ok(t.abs() * sd / n.sqrt())
    }

    pub(crate) fn eval_stat_tests_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<V> {
        let (min, max) = match name {
            "T.TEST" | "TTEST" => (4, 4),
            "AVERAGE.WEIGHTED" => (2, 254),
            _ => (2, 2),
        };
        if !(min..=max).contains(&args.len()) {
            return Err(E::Value);
        }
        let number = match name {
            "CHISQ.TEST" | "CHITEST" => {
                let actual = self.read_grid(&args[0], ctx, depth)?;
                let expected = self.read_grid(&args[1], ctx, depth)?;
                chisq_test(&actual, &expected)?
            }
            "F.TEST" | "FTEST" => {
                let first = self.read_grid(&args[0], ctx, depth)?;
                let second = self.read_grid(&args[1], ctx, depth)?;
                f_test(&first, &second)?
            }
            "T.TEST" | "TTEST" => self.t_test(args, ctx, depth)?,
            "AVERAGE.WEIGHTED" => {
                let pairs = self.read_weighted_pairs(args, ctx, depth)?;
                average_weighted(&pairs, args.len() % 2 == 1)?
            }
            _ => self.margin_of_error(args, ctx, depth)?,
        };
        Ok(V::Number(number))
    }
}

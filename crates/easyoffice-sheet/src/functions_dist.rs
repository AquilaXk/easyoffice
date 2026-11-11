//! Probability distribution worksheet functions: `T.DIST`, `CHISQ.DIST`, `BETA.DIST`,
//! `GAMMA.DIST`, `BINOM.DIST`, `NORM.DIST` families with their inverses and the legacy names,
//! plus `GAMMA` and `GAMMALN`. Numerics live in [`crate::special`] (ported from `statrs`).

use crate::functions_complex::{std_normal_cdf, std_normal_pdf};
use crate::special::{
    beta_regularized, gamma, gamma_regularized, invert, ln_beta, ln_gamma, Monotone,
};
use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};
use std::f64::consts::PI;

type Num = Result<f64, FormulaError>;

/// Largest degrees-of-freedom / trial count accepted (beyond it the CDFs lose all precision).
const MAX_COUNT: f64 = 1e10;

/// `(min args, max args, index of the TRUE/FALSE `cumulative` argument)`.
fn dist_spec(name: &str) -> Option<(usize, usize, Option<usize>)> {
    Some(match name {
        "T.DIST" | "CHISQ.DIST" => (3, 3, Some(2)),
        "TDIST" => (3, 3, None),
        "T.DIST.RT" | "T.DIST.2T" | "T.INV" | "T.INV.2T" | "TINV" | "CHISQ.DIST.RT"
        | "CHISQ.INV" | "CHISQ.INV.RT" | "CHIDIST" | "CHIINV" => (2, 2, None),
        "BETA.DIST" => (4, 6, Some(3)),
        "BETA.INV" | "BETADIST" | "BETAINV" => (3, 5, None),
        "GAMMA.DIST" | "GAMMADIST" | "BINOM.DIST" | "BINOMDIST" | "NORM.DIST" | "NORMDIST" => {
            (4, 4, Some(3))
        }
        "GAMMA.INV" | "GAMMAINV" | "BINOM.INV" | "CRITBINOM" | "NORM.INV" | "NORMINV" => {
            (3, 3, None)
        }
        "GAMMALN" | "GAMMALN.PRECISE" | "GAMMA" | "NORM.S.INV" | "NORMSINV" => (1, 1, None),
        _ => return None,
    })
}

/// The `i`-th argument, or `default` when absent or skipped.
fn opt(a: &[f64], i: usize, default: f64) -> f64 {
    a.get(i).copied().filter(|v| !v.is_nan()).unwrap_or(default)
}

fn need(ok: bool) -> Result<(), FormulaError> {
    ok.then_some(()).ok_or(FormulaError::Num)
}

/// Degrees of freedom truncated to an integer in `1..=MAX_COUNT`.
fn degrees(v: f64) -> Num {
    let df = v.trunc();
    need((1.0..=MAX_COUNT).contains(&df))?;
    Ok(df)
}

fn t_cdf(x: f64, df: f64) -> f64 {
    if x == 0.0 {
        return 0.5;
    }
    let tail = 0.5 * beta_regularized(df / 2.0, 0.5, df / (df + x * x));
    if x > 0.0 {
        1.0 - tail
    } else {
        tail
    }
}

fn t_pdf(x: f64, df: f64) -> f64 {
    let ln = ln_gamma((df + 1.0) / 2.0)
        - ln_gamma(df / 2.0)
        - 0.5 * (df * PI).ln()
        - (df + 1.0) / 2.0 * (x * x / df).ln_1p();
    ln.exp()
}

/// Lower-tail quantile of a distribution symmetric about zero: the tail below zero is solved
/// directly, so extreme probabilities keep full precision.
fn symmetric_inverse(cdf: impl Fn(f64) -> f64, p: f64) -> f64 {
    if p == 0.5 {
        return 0.0;
    }
    let lower = p.min(1.0 - p);
    let x = invert(
        &cdf,
        lower,
        (f64::NEG_INFINITY, 0.0),
        1.0,
        Monotone::Increasing,
    );
    if p > 0.5 {
        -x
    } else {
        x
    }
}

fn chisq_pdf(x: f64, df: f64) -> Num {
    let half = df / 2.0;
    if x == 0.0 {
        return match df {
            1.0 => Err(FormulaError::Num),
            2.0 => Ok(0.5),
            _ => Ok(0.0),
        };
    }
    Ok(((half - 1.0) * x.ln() - x / 2.0 - half * std::f64::consts::LN_2 - ln_gamma(half)).exp())
}

fn t_family(name: &str, a: &[f64]) -> Num {
    let (x, p) = (a[0], a[0]);
    let df = degrees(a[1])?;
    match name {
        "T.DIST" => Ok(if a[2] != 0.0 {
            t_cdf(x, df)
        } else {
            t_pdf(x, df)
        }),
        "T.DIST.RT" => Ok(t_cdf(-x, df)),
        "T.DIST.2T" => {
            need(x >= 0.0)?;
            Ok((2.0 * t_cdf(-x, df)).min(1.0))
        }
        "TDIST" => {
            need(x >= 0.0)?;
            match a[2].trunc() as i64 {
                1 => Ok(t_cdf(-x, df)),
                2 => Ok((2.0 * t_cdf(-x, df)).min(1.0)),
                _ => Err(FormulaError::Num),
            }
        }
        "T.INV" => {
            need(p > 0.0 && p < 1.0)?;
            Ok(symmetric_inverse(|v| t_cdf(v, df), p))
        }
        _ => {
            // T.INV.2T, TINV
            need(p > 0.0 && p <= 1.0)?;
            Ok(-symmetric_inverse(|v| t_cdf(v, df), p / 2.0))
        }
    }
}

fn chisq_family(name: &str, a: &[f64]) -> Num {
    let (x, p) = (a[0], a[0]);
    let df = degrees(a[1])?;
    let half = df / 2.0;
    let support = (0.0, f64::INFINITY);
    match name {
        "CHISQ.DIST" if a[2] != 0.0 => {
            need(x >= 0.0)?;
            Ok(gamma_regularized(half, x / 2.0).0)
        }
        "CHISQ.DIST" => {
            need(x >= 0.0)?;
            chisq_pdf(x, df)
        }
        "CHISQ.DIST.RT" | "CHIDIST" => {
            need(x >= 0.0)?;
            Ok(gamma_regularized(half, x / 2.0).1)
        }
        "CHISQ.INV" => {
            need((0.0..1.0).contains(&p))?;
            Ok(invert(
                |v| gamma_regularized(half, v / 2.0).0,
                p,
                support,
                df,
                Monotone::Increasing,
            ))
        }
        _ => {
            // CHISQ.INV.RT, CHIINV
            need(p > 0.0 && p <= 1.0)?;
            Ok(invert(
                |v| gamma_regularized(half, v / 2.0).1,
                p,
                support,
                df,
                Monotone::Decreasing,
            ))
        }
    }
}

/// `ln(z^e)` where `0^e` is `-inf` for `e > 0`, `+inf` for `e < 0` and `1` for `e == 0`.
fn ln_pow(z: f64, e: f64) -> f64 {
    if e == 0.0 {
        0.0
    } else {
        e * z.ln()
    }
}

fn beta_family(name: &str, a: &[f64]) -> Num {
    let legacy = matches!(name, "BETADIST" | "BETAINV");
    let ab_at = if name == "BETA.DIST" { 4 } else { 3 };
    let (lo, hi) = (opt(a, ab_at, 0.0), opt(a, ab_at + 1, 1.0));
    let (alpha, beta) = (a[1], a[2]);
    need(alpha > 0.0 && beta > 0.0 && lo < hi)?;
    let width = hi - lo;
    if name.ends_with("INV") {
        let p = a[0];
        need(p > 0.0 && p <= 1.0)?;
        if p == 1.0 {
            return Ok(hi);
        }
        let z = invert(
            |v| beta_regularized(alpha, beta, v),
            p,
            (0.0, 1.0),
            0.5,
            Monotone::Increasing,
        );
        return Ok(lo + z * width);
    }
    need(a[0] >= lo && a[0] <= hi)?;
    let z = (a[0] - lo) / width;
    if legacy || (name == "BETA.DIST" && a[3] != 0.0) {
        return Ok(beta_regularized(alpha, beta, z));
    }
    let ln = ln_pow(z, alpha - 1.0) + ln_pow(1.0 - z, beta - 1.0) - ln_beta(alpha, beta);
    Ok(ln.exp() / width)
}

fn gamma_family(name: &str, a: &[f64]) -> Num {
    match name {
        "GAMMALN" | "GAMMALN.PRECISE" => {
            need(a[0] > 0.0)?;
            Ok(ln_gamma(a[0]))
        }
        "GAMMA" => gamma(a[0]).ok_or(FormulaError::Num),
        "GAMMA.INV" | "GAMMAINV" => {
            let (p, alpha, scale) = (a[0], a[1], a[2]);
            need((0.0..1.0).contains(&p) && alpha > 0.0 && scale > 0.0)?;
            let y = invert(
                |v| gamma_regularized(alpha, v).0,
                p,
                (0.0, f64::INFINITY),
                alpha,
                Monotone::Increasing,
            );
            Ok(y * scale)
        }
        _ => {
            // GAMMA.DIST, GAMMADIST
            let (x, alpha, scale) = (a[0], a[1], a[2]);
            need(x >= 0.0 && alpha > 0.0 && scale > 0.0)?;
            if a[3] != 0.0 {
                return Ok(gamma_regularized(alpha, x / scale).0);
            }
            let ln = ln_pow(x, alpha - 1.0) - x / scale - alpha * scale.ln() - ln_gamma(alpha);
            Ok(ln.exp())
        }
    }
}

fn ln_choose(n: f64, k: f64) -> f64 {
    ln_gamma(n + 1.0) - ln_gamma(k + 1.0) - ln_gamma(n - k + 1.0)
}

fn binom_cdf(k: f64, n: f64, p: f64) -> f64 {
    if k >= n {
        1.0
    } else {
        beta_regularized(n - k, k + 1.0, 1.0 - p)
    }
}

fn binom_family(name: &str, a: &[f64]) -> Num {
    if matches!(name, "BINOM.INV" | "CRITBINOM") {
        let (n, p, alpha) = (a[0].trunc(), a[1], a[2]);
        need((0.0..=MAX_COUNT).contains(&n) && (0.0..=1.0).contains(&p))?;
        need((0.0..=1.0).contains(&alpha))?;
        let (mut lo, mut hi) = (0.0, n);
        while lo < hi {
            let mid = (lo + (hi - lo) / 2.0).floor();
            if binom_cdf(mid, n, p) >= alpha {
                hi = mid;
            } else {
                lo = mid + 1.0;
            }
        }
        return Ok(lo);
    }
    let (k, n, p) = (a[0].trunc(), a[1].trunc(), a[2]);
    need((0.0..=MAX_COUNT).contains(&n) && (0.0..=n).contains(&k) && (0.0..=1.0).contains(&p))?;
    if a[3] != 0.0 {
        return Ok(binom_cdf(k, n, p));
    }
    Ok(match p {
        _ if p == 0.0 => f64::from(k == 0.0),
        _ if p == 1.0 => f64::from(k == n),
        _ => (ln_choose(n, k) + k * p.ln() + (n - k) * (-p).ln_1p()).exp(),
    })
}

fn norm_family(name: &str, a: &[f64]) -> Num {
    match name {
        "NORM.S.INV" | "NORMSINV" => {
            need(a[0] > 0.0 && a[0] < 1.0)?;
            Ok(symmetric_inverse(std_normal_cdf, a[0]))
        }
        "NORM.INV" | "NORMINV" => {
            need(a[0] > 0.0 && a[0] < 1.0 && a[2] > 0.0)?;
            Ok(a[1] + a[2] * symmetric_inverse(std_normal_cdf, a[0]))
        }
        _ => {
            // NORM.DIST, NORMDIST
            need(a[2] > 0.0)?;
            let z = (a[0] - a[1]) / a[2];
            Ok(if a[3] != 0.0 {
                std_normal_cdf(z)
            } else {
                std_normal_pdf(z) / a[2]
            })
        }
    }
}

/// Pure evaluation of a distribution function over its already coerced arguments
/// (`TRUE`/`FALSE` as `1.0`/`0.0`).
pub(crate) fn dist_value(name: &str, a: &[f64]) -> Num {
    let value = match name {
        n if n.starts_with("T.") || n == "TDIST" || n == "TINV" => t_family(n, a),
        n if n.starts_with("CHI") => chisq_family(n, a),
        n if n.starts_with("BETA") => beta_family(n, a),
        n if n.starts_with("GAMMA") => gamma_family(n, a),
        n if n.starts_with("BINOM") || n == "CRITBINOM" => binom_family(n, a),
        n => norm_family(n, a),
    }?;
    value.is_finite().then_some(value).ok_or(FormulaError::Num)
}

impl SheetEngine {
    pub(crate) fn eval_dist_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        let (min, max, bool_at) = dist_spec(name).ok_or(FormulaError::Name)?;
        if !(min..=max).contains(&args.len()) {
            return Err(FormulaError::Value);
        }
        let mut values = Vec::with_capacity(args.len());
        for (i, arg) in args.iter().enumerate() {
            let v = if matches!(arg, Expr::Missing) && i >= min {
                f64::NAN
            } else if Some(i) == bool_at {
                let flag = self.eval_arg_val(arg, ctx, depth)?;
                f64::from(flag.as_bool().ok_or(FormulaError::Value)?)
            } else {
                self.eval_strict_num_arg(arg, ctx, depth)?
            };
            values.push(v);
        }
        dist_value(name, &values).map(EvaluatedCellValue::Number)
    }
}

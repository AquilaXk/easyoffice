//! More probability distributions (PD-205): `F.DIST`, `F.DIST.RT`, `F.INV`, `F.INV.RT`,
//! `EXPON.DIST`, `WEIBULL.DIST`, `LOGNORM.DIST`, `LOGNORM.INV`, `POISSON.DIST`, `NEGBINOM.DIST`,
//! `HYPGEOM.DIST`, `BINOM.DIST.RANGE`, `CONFIDENCE.NORM`, `CONFIDENCE.T` and the legacy names
//! `FDIST`, `FINV`, `EXPONDIST`, `WEIBULL`, `LOGNORMDIST`, `LOGINV`, `POISSON`, `NEGBINOMDIST`,
//! `HYPGEOMDIST`, `CONFIDENCE`.
//!
//! Reference implementation: the argument rules and domains of
//! `packages/engine-formula/src/functions/{statistical,compatibility}/*/index.ts` of the Univer
//! formula engine, commit `5a06a7842c8e13fa0646d0733dc3662cf4b330b9`, Apache-2.0, Copyright
//! 2023-present DreamNum Co., Ltd. See `THIRD_PARTY_NOTICES.md`. The values come from the
//! regularized incomplete beta and gamma functions of [`crate::special`] instead of the
//! reference's series, and the tails are computed directly (`F.DIST.RT`, `F.INV.RT`).
//!
//! The hypergeometric cumulative sum walks outward from the mode of the clamped range with a term
//! ratio and stops at the negligible terms, so it neither underflows at the start nor loops over a
//! huge range (at most [`MAX_SUM_TERMS`] terms, then `#NUM!`).

use crate::functions_complex::std_normal_cdf;
use crate::functions_dist::{degrees, dist_value, ln_choose, need, opt, Num};
use crate::special::{beta_regularized, gamma_regularized, invert, ln_beta, ln_gamma, Monotone};
use crate::FormulaError;
use std::f64::consts::PI;

/// Longest run of terms a cumulative sum adds.
const MAX_SUM_TERMS: f64 = 20_000_000.0;
/// A term this much smaller than the running sum ends the walk away from the mode.
const NEGLIGIBLE: f64 = 1e-18;
/// Widest `BINOM.DIST.RANGE` span summed term by term (wider spans use the cumulative form).
const MAX_DIRECT_RANGE: f64 = 4096.0;
const MAX_COUNT: f64 = 1e10;

/// `(min args, max args, index of the TRUE/FALSE `cumulative` argument)` of the names this file
/// evaluates.
pub(crate) fn spec(name: &str) -> Option<(usize, usize, Option<usize>)> {
    Some(match name {
        "F.DIST" | "WEIBULL.DIST" | "LOGNORM.DIST" | "NEGBINOM.DIST" => (4, 4, Some(3)),
        "WEIBULL" => (4, 4, Some(3)),
        "EXPON.DIST" | "EXPONDIST" | "POISSON.DIST" | "POISSON" => (3, 3, Some(2)),
        "HYPGEOM.DIST" => (5, 5, Some(4)),
        "HYPGEOMDIST" => (4, 4, None),
        "BINOM.DIST.RANGE" => (3, 4, None),
        "F.DIST.RT" | "FDIST" | "F.INV" | "F.INV.RT" | "FINV" | "LOGNORM.INV" | "LOGINV"
        | "LOGNORMDIST" | "NEGBINOMDIST" | "CONFIDENCE.NORM" | "CONFIDENCE" | "CONFIDENCE.T" => {
            (3, 3, None)
        }
        _ => return None,
    })
}

/// Probability density or cumulative value, chosen by `cumulative`.
fn pick(cumulative: f64, cdf: impl FnOnce() -> f64, pdf: impl FnOnce() -> f64) -> f64 {
    if cumulative != 0.0 {
        cdf()
    } else {
        pdf()
    }
}

pub(crate) fn f_cdf(x: f64, d1: f64, d2: f64) -> f64 {
    let t = d1 * x;
    beta_regularized(d1 / 2.0, d2 / 2.0, t / (t + d2))
}

pub(crate) fn f_sf(x: f64, d1: f64, d2: f64) -> f64 {
    beta_regularized(d2 / 2.0, d1 / 2.0, d2 / (d2 + d1 * x))
}

fn f_pdf(x: f64, d1: f64, d2: f64) -> Num {
    if x == 0.0 {
        return match d1 {
            1.0 => Err(FormulaError::Num),
            2.0 => Ok(1.0),
            _ => Ok(0.0),
        };
    }
    let ln = 0.5 * d1 * d1.ln() + 0.5 * d2 * d2.ln() + (0.5 * d1 - 1.0) * x.ln()
        - 0.5 * (d1 + d2) * (d2 + d1 * x).ln()
        - ln_beta(d1 / 2.0, d2 / 2.0);
    Ok(ln.exp())
}

fn f_family(name: &str, a: &[f64]) -> Num {
    let (d1, d2) = (degrees(a[1])?, degrees(a[2])?);
    let x = a[0];
    match name {
        "F.DIST" => {
            need(x >= 0.0)?;
            if a[3] != 0.0 {
                Ok(f_cdf(x, d1, d2))
            } else {
                f_pdf(x, d1, d2)
            }
        }
        "F.DIST.RT" | "FDIST" => {
            need(x >= 0.0)?;
            Ok(f_sf(x, d1, d2))
        }
        _ => {
            need((0.0..=1.0).contains(&x))?;
            // the quantile at the closed end of the support is 0, at the open end it is infinite
            let (zero_at, infinite_at) = if name == "F.INV" {
                (0.0, 1.0)
            } else {
                (1.0, 0.0)
            };
            need(x != infinite_at)?;
            if x == zero_at {
                return Ok(0.0);
            }
            let inverse = if name == "F.INV" {
                invert(
                    |v| f_cdf(v, d1, d2),
                    x,
                    (0.0, f64::INFINITY),
                    1.0,
                    Monotone::Increasing,
                )
            } else {
                invert(
                    |v| f_sf(v, d1, d2),
                    x,
                    (0.0, f64::INFINITY),
                    1.0,
                    Monotone::Decreasing,
                )
            };
            Ok(inverse)
        }
    }
}

fn expon(a: &[f64]) -> Num {
    let (x, lambda) = (a[0], a[1]);
    need(x >= 0.0 && lambda > 0.0)?;
    Ok(pick(
        a[2],
        || -(-lambda * x).exp_m1(),
        || lambda * (-lambda * x).exp(),
    ))
}

fn weibull(a: &[f64]) -> Num {
    let (x, alpha, beta) = (a[0], a[1], a[2]);
    need(x >= 0.0 && alpha > 0.0 && beta > 0.0)?;
    let power = (x / beta).powf(alpha);
    if a[3] != 0.0 {
        return Ok(-(-power).exp_m1());
    }
    if x == 0.0 {
        return match alpha {
            1.0 => Ok(1.0 / beta),
            _ if alpha > 1.0 => Ok(0.0),
            _ => Err(FormulaError::Num),
        };
    }
    Ok((alpha.ln() - alpha * beta.ln() + (alpha - 1.0) * x.ln() - power).exp())
}

fn lognorm(name: &str, a: &[f64]) -> Num {
    let (mean, sd) = (a[1], a[2]);
    need(sd > 0.0)?;
    if matches!(name, "LOGNORM.INV" | "LOGINV") {
        need(a[0] > 0.0 && a[0] < 1.0)?;
        let z = dist_value("NORM.S.INV", &[a[0]])?;
        return Ok((mean + sd * z).exp());
    }
    let x = a[0];
    need(x > 0.0)?;
    let z = (x.ln() - mean) / sd;
    let cumulative = name == "LOGNORMDIST" || a[3] != 0.0;
    if cumulative {
        return Ok(std_normal_cdf(z));
    }
    Ok((-0.5 * z * z).exp() / (x * sd * (2.0 * PI).sqrt()))
}

fn poisson(a: &[f64]) -> Num {
    let (x, mean) = (a[0].floor(), a[1]);
    need(x >= 0.0 && mean >= 0.0)?;
    if mean == 0.0 {
        return Ok(pick(a[2], || 1.0, || f64::from(x == 0.0)));
    }
    Ok(pick(
        a[2],
        || gamma_regularized(x + 1.0, mean).1,
        || (x * mean.ln() - mean - ln_gamma(x + 1.0)).exp(),
    ))
}

fn negbinom(name: &str, a: &[f64]) -> Num {
    let (f, s, p) = (a[0].floor(), a[1].floor(), a[2]);
    need(f >= 0.0 && s >= 1.0 && p > 0.0 && p < 1.0)?;
    let pdf = || {
        (ln_gamma(f + s) - ln_gamma(f + 1.0) - ln_gamma(s) + s * p.ln() + f * (-p).ln_1p()).exp()
    };
    if name == "NEGBINOMDIST" {
        return Ok(pdf());
    }
    Ok(pick(a[3], || beta_regularized(s, f + 1.0, p), pdf))
}

/// Terms of the hypergeometric distribution with `k` successes drawn, sample `n`, `big_k`
/// successes among `pop`.
struct Hypergeometric {
    n: f64,
    big_k: f64,
    pop: f64,
}

impl Hypergeometric {
    fn lowest(&self) -> f64 {
        (self.n - self.pop + self.big_k).max(0.0)
    }

    fn pmf(&self, k: f64) -> f64 {
        (ln_choose(self.big_k, k) + ln_choose(self.pop - self.big_k, self.n - k)
            - ln_choose(self.pop, self.n))
        .exp()
    }

    /// `pmf(k + 1) / pmf(k)`.
    fn up_ratio(&self, k: f64) -> f64 {
        (self.big_k - k) * (self.n - k) / ((k + 1.0) * (self.pop - self.big_k - self.n + k + 1.0))
    }

    /// `pmf(k - 1) / pmf(k)`.
    fn down_ratio(&self, k: f64) -> f64 {
        k * (self.pop - self.big_k - self.n + k) / ((self.big_k - k + 1.0) * (self.n - k + 1.0))
    }

    fn cdf(&self, k: f64) -> Num {
        let low = self.lowest();
        let mode = ((self.n + 1.0) * (self.big_k + 1.0) / (self.pop + 2.0))
            .floor()
            .clamp(low, k);
        if k - low > MAX_SUM_TERMS {
            return Err(FormulaError::Num);
        }
        let start = self.pmf(mode);
        let mut sum = start;
        let mut term = start;
        let mut i = mode;
        while i < k {
            term *= self.up_ratio(i);
            i += 1.0;
            sum += term;
            if term < sum * NEGLIGIBLE {
                break;
            }
        }
        term = start;
        i = mode;
        while i > low {
            term *= self.down_ratio(i);
            i -= 1.0;
            sum += term;
            if term < sum * NEGLIGIBLE {
                break;
            }
        }
        Ok(sum.min(1.0))
    }
}

fn hypgeom(name: &str, a: &[f64]) -> Num {
    let (k, n, big_k, pop) = (a[0].floor(), a[1].floor(), a[2].floor(), a[3].floor());
    let valid = k >= 0.0
        && k <= n
        && k <= big_k
        && k >= n - pop + big_k
        && n > 0.0
        && n <= pop
        && big_k > 0.0
        && big_k <= pop
        && pop > 0.0
        && pop <= MAX_COUNT;
    need(valid)?;
    let h = Hypergeometric { n, big_k, pop };
    let value = if name == "HYPGEOM.DIST" && a[4] != 0.0 {
        h.cdf(k)?
    } else {
        h.pmf(k)
    };
    Ok(if value.is_nan() { 0.0 } else { value })
}

fn binomial_pmf(k: f64, n: f64, p: f64) -> f64 {
    match p {
        _ if p == 0.0 => f64::from(k == 0.0),
        _ if p == 1.0 => f64::from(k == n),
        _ => (ln_choose(n, k) + k * p.ln() + (n - k) * (-p).ln_1p()).exp(),
    }
}

fn binomial_cdf(k: f64, n: f64, p: f64) -> f64 {
    if k < 0.0 {
        0.0
    } else if k >= n {
        1.0
    } else {
        beta_regularized(n - k, k + 1.0, 1.0 - p)
    }
}

fn binom_range(a: &[f64]) -> Num {
    let (n, p, s) = (a[0].floor(), a[1], a[2].floor());
    let s2 = opt(a, 3, s).floor();
    need((0.0..=MAX_COUNT).contains(&n) && (0.0..=1.0).contains(&p))?;
    need(s >= 0.0 && s <= n && s2 >= s && s2 <= n)?;
    if s2 - s < MAX_DIRECT_RANGE {
        let mut sum = 0.0;
        let mut k = s;
        while k <= s2 {
            sum += binomial_pmf(k, n, p);
            k += 1.0;
        }
        return Ok(sum);
    }
    Ok((binomial_cdf(s2, n, p) - binomial_cdf(s - 1.0, n, p)).max(0.0))
}

fn confidence(name: &str, a: &[f64]) -> Num {
    let (alpha, sd, size) = (a[0], a[1], a[2].floor());
    need(alpha > 0.0 && alpha < 1.0 && sd > 0.0 && size >= 1.0)?;
    if name == "CONFIDENCE.T" {
        if size == 1.0 {
            return Err(FormulaError::DivByZero);
        }
        return Ok(dist_value("T.INV.2T", &[alpha, size - 1.0])?.abs() * sd / size.sqrt());
    }
    Ok(dist_value("NORM.S.INV", &[alpha / 2.0])?.abs() * sd / size.sqrt())
}

/// Pure evaluation over the coerced arguments (`TRUE`/`FALSE` as `1.0`/`0.0`, a skipped
/// optional argument as NaN).
pub(crate) fn value(name: &str, a: &[f64]) -> Num {
    match name {
        n if n.starts_with("F.") || n == "FDIST" || n == "FINV" => f_family(n, a),
        "EXPON.DIST" | "EXPONDIST" => expon(a),
        "WEIBULL.DIST" | "WEIBULL" => weibull(a),
        n if n.starts_with("LOG") => lognorm(n, a),
        "POISSON.DIST" | "POISSON" => poisson(a),
        n if n.starts_with("NEGBINOM") => negbinom(n, a),
        n if n.starts_with("HYPGEOM") => hypgeom(n, a),
        "BINOM.DIST.RANGE" => binom_range(a),
        n => confidence(n, a),
    }
}

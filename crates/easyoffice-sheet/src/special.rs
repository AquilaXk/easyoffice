//! Special functions behind the distribution worksheet functions: log-gamma, the regularized
//! incomplete gamma and beta functions, and a monotone-CDF inverter.
//!
//! Reference implementation: `statrs` (<https://github.com/statrs-dev/statrs>, MIT License,
//! Copyright (c) 2016 Michael Ma): `function/gamma.rs` (`ln_gamma` Lanczos approximation with
//! g = 10.900511, `gamma_lr` / `gamma_ur` series and continued fraction) and `function/beta.rs`
//! (`beta_reg` continued fraction with the symmetry transform). Ported to safe Rust with the
//! loops bounded and the upper tail computed directly. The inverter is written here.

use std::f64::consts::{E, PI};

const LN_PI: f64 = 1.144_729_885_849_400_2;
const LN_2_SQRT_E_OVER_PI: f64 = 0.620_782_237_635_245_2;
const LANCZOS_G: f64 = 10.900_511;
#[allow(clippy::excessive_precision)]
const LANCZOS_DK: [f64; 11] = [
    2.485_740_891_387_535_6e-5,
    1.051_423_785_817_219_7,
    -3.456_870_972_220_162_3,
    4.512_277_094_668_948,
    -2.982_852_253_235_766_5,
    1.056_397_115_771_267,
    -1.954_287_731_916_458_7e-1,
    1.709_705_434_044_412_2e-2,
    -5.719_261_174_043_057_8e-4,
    4.633_994_733_599_057e-6,
    -2.719_949_084_886_077e-9,
];

/// Relative precision target of the series and continued fractions.
const EPS: f64 = 1e-15;
/// Iteration bound of the series and continued fractions (convergence needs `O(sqrt(a))`).
const MAX_ITER: usize = 100_000;
const BIG: f64 = 4_503_599_627_370_496.0;
const BIG_INV: f64 = 2.220_446_049_250_313e-16;
/// `ln(f64::MAX)`: below `-LN_MAX` the exponent underflows to zero.
const LN_MAX: f64 = 709.782_712_893_384;
const INVERT_MAX_STEPS: usize = 400;

/// `ln(Gamma(x))` for `x > 0` (reflection formula below `0.5`).
pub(crate) fn ln_gamma(x: f64) -> f64 {
    if x < 0.5 {
        return LN_PI - (PI * x).sin().ln() - ln_gamma(1.0 - x);
    }
    let mut sum = LANCZOS_DK[0];
    for (i, dk) in LANCZOS_DK.iter().enumerate().skip(1) {
        sum += dk / (x + i as f64 - 1.0);
    }
    sum.ln() + LN_2_SQRT_E_OVER_PI + (x - 0.5) * ((x - 0.5 + LANCZOS_G) / E).ln()
}

/// `Gamma(x)`; `None` at the poles (`0` and negative integers). Integers use exact products.
pub(crate) fn gamma(x: f64) -> Option<f64> {
    if x == x.floor() {
        if x <= 0.0 {
            return None;
        }
        if x <= 171.0 {
            return Some((1..x as u64).fold(1.0, |acc, k| acc * k as f64));
        }
        return Some(f64::INFINITY);
    }
    if x < 0.5 {
        return Some(PI / ((PI * x).sin() * gamma(1.0 - x)?));
    }
    Some(ln_gamma(x).exp())
}

/// Regularized incomplete gamma `(P(a, x), Q(a, x))` for `a > 0`, `x >= 0`.
pub(crate) fn gamma_regularized(a: f64, x: f64) -> (f64, f64) {
    if x <= 0.0 {
        return (0.0, 1.0);
    }
    let ax = a * x.ln() - x - ln_gamma(a);
    if ax < -LN_MAX {
        return if a < x { (1.0, 0.0) } else { (0.0, 1.0) };
    }
    if x <= 1.0 || x <= a {
        let (mut r, mut c, mut ans) = (a, 1.0, 1.0);
        for _ in 0..MAX_ITER {
            r += 1.0;
            c *= x / r;
            ans += c;
            if c / ans <= EPS {
                break;
            }
        }
        let lower = ax.exp() * ans / a;
        return (lower, 1.0 - lower);
    }
    let upper = ax.exp() * gamma_continued_fraction(a, x);
    (1.0 - upper, upper)
}

fn gamma_continued_fraction(a: f64, x: f64) -> f64 {
    let mut y = 1.0 - a;
    let mut z = x + y + 1.0;
    let mut c = 0.0;
    let (mut pkm2, mut qkm2) = (1.0, x);
    let (mut pkm1, mut qkm1) = (x + 1.0, z * x);
    let mut ans = pkm1 / qkm1;
    for _ in 0..MAX_ITER {
        c += 1.0;
        y += 1.0;
        z += 2.0;
        let yc = y * c;
        let pk = pkm1 * z - pkm2 * yc;
        let qk = qkm1 * z - qkm2 * yc;
        let t = if qk != 0.0 {
            let r = pk / qk;
            let t = ((ans - r) / r).abs();
            ans = r;
            t
        } else {
            1.0
        };
        (pkm2, pkm1, qkm2, qkm1) = (pkm1, pk, qkm1, qk);
        if pk.abs() > BIG {
            pkm2 *= BIG_INV;
            pkm1 *= BIG_INV;
            qkm2 *= BIG_INV;
            qkm1 *= BIG_INV;
        }
        if t <= EPS {
            break;
        }
    }
    ans
}

/// `ln B(a, b)`.
pub(crate) fn ln_beta(a: f64, b: f64) -> f64 {
    ln_gamma(a) + ln_gamma(b) - ln_gamma(a + b)
}

/// Regularized incomplete beta `I_x(a, b)` for `a, b > 0`, `0 <= x <= 1`.
pub(crate) fn beta_regularized(a: f64, b: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let bt = (a * x.ln() + b * (1.0 - x).ln() - ln_beta(a, b)).exp();
    if x >= (a + 1.0) / (a + b + 2.0) {
        1.0 - bt * beta_continued_fraction(b, a, 1.0 - x) / b
    } else {
        bt * beta_continued_fraction(a, b, x) / a
    }
}

/// Lentz evaluation of the incomplete beta continued fraction.
fn beta_continued_fraction(a: f64, b: f64, x: f64) -> f64 {
    let fpmin = f64::MIN_POSITIVE / f64::EPSILON;
    let clamp = |v: f64| if v.abs() < fpmin { fpmin } else { v };
    let (qab, qap, qam) = (a + b, a + 1.0, a - 1.0);
    let mut c = 1.0;
    let mut d = 1.0 / clamp(1.0 - qab * x / qap);
    let mut h = d;
    for m in 1..MAX_ITER {
        let m = m as f64;
        let m2 = 2.0 * m;
        let aa = m * (b - m) * x / ((qam + m2) * (a + m2));
        d = 1.0 / clamp(1.0 + aa * d);
        c = clamp(1.0 + aa / c);
        h *= d * c;
        let aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
        d = 1.0 / clamp(1.0 + aa * d);
        c = clamp(1.0 + aa / c);
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() <= f64::EPSILON {
            break;
        }
    }
    h
}

/// Direction of the function handed to [`invert`].
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Monotone {
    Increasing,
    Decreasing,
}

/// Solves `f(x) = target` for a monotone `f` on `support` (either bound may be infinite: the
/// bracket then grows geometrically from `start`). Bisection to full `f64` resolution, so the
/// result is as accurate as `f` itself.
pub(crate) fn invert(
    f: impl Fn(f64) -> f64,
    target: f64,
    support: (f64, f64),
    start: f64,
    direction: Monotone,
) -> f64 {
    // Whether `x` lies left of the root.
    let left_of_root = |x: f64| match direction {
        Monotone::Increasing => f(x) < target,
        Monotone::Decreasing => f(x) > target,
    };
    let (mut lo, mut hi) = support;
    let mut probe = start.max(f64::MIN_POSITIVE);
    if hi.is_infinite() {
        hi = lo.max(0.0) + probe;
        while left_of_root(hi) && hi.is_finite() {
            probe *= 2.0;
            hi = lo.max(0.0) + probe;
        }
    }
    probe = start.max(f64::MIN_POSITIVE);
    if lo.is_infinite() {
        lo = hi.min(0.0) - probe;
        while !left_of_root(lo) && lo.is_finite() {
            probe *= 2.0;
            lo = hi.min(0.0) - probe;
        }
    }
    for _ in 0..INVERT_MAX_STEPS {
        let mid = 0.5 * (lo + hi);
        if mid == lo || mid == hi {
            break;
        }
        if left_of_root(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

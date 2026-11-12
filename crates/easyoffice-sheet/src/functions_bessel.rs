//! Bessel functions: `BESSELI`, `BESSELJ`, `BESSELK`, `BESSELY` (PD-201).
//!
//! Reference implementation: `packages/engine-formula/src/basics/engineering.ts` (the `BESSEL`
//! namespace: rational approximations for orders 0 and 1, upward recurrence, Miller's downward
//! recurrence for `besselj` and `besseli`) and `src/functions/engineering/{besseli,besselj,besselk,
//! bessely}/index.ts` of the Univer formula engine, commit `5a06a7842c8e13fa0646d0733dc3662cf4b330b9`,
//! Apache-2.0, Copyright 2023-present DreamNum Co., Ltd. See `THIRD_PARTY_NOTICES.md`.
//!
//! The order is limited to [`MAX_BESSEL_ORDER`] (the recurrences take one step per order); a
//! larger order gives `#NUM!`.

use crate::functions_fin_args::ArgRule;
use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};

/// Largest order the functions accept.
pub(crate) const MAX_BESSEL_ORDER: f64 = 100_000.0;

/// The reference's 9-digit `2/pi`, kept so the results match it digit for digit.
#[allow(clippy::approx_constant)]
const W: f64 = 0.636_619_772;

fn horner(coefficients: &[f64], v: f64) -> f64 {
    coefficients.iter().fold(0.0, |z, &c| v * z + c)
}

/// Upward recurrence from the orders 0 and 1 (`sign` is -1 for J and Y, 1 for K).
fn bessel_iter(x: f64, n: i64, f0: f64, f1: f64, sign: f64) -> f64 {
    if n == 0 {
        return f0;
    }
    if n == 1 {
        return f1;
    }
    let tdx = 2.0 / x;
    let (mut prev, mut cur, mut next) = (f0, f1, f1);
    for o in 1..n {
        next = cur * o as f64 * tdx + sign * prev;
        prev = cur;
        cur = next;
    }
    next
}

/// The order-0 and order-1 functions of one kind.
type LowOrders = (fn(f64) -> f64, fn(f64) -> f64);

/// Shared order handling of `bessely` and `besselk`: `nonzero` is 1 for Y and 2 for K.
fn bessel_wrapped(x: f64, n: i64, orders: LowOrders, nonzero: u8, sign: f64) -> f64 {
    if x == 0.0 {
        return if nonzero == 1 {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    if x < 0.0 {
        return f64::NAN;
    }
    match n {
        0 => orders.0(x),
        1 => orders.1(x),
        n if n < 0 => f64::NAN,
        n => bessel_iter(x, n, orders.0(x), orders.1(x), sign),
    }
}

fn j0(x: f64) -> f64 {
    const A1: [f64; 6] = [
        -184.9052456,
        77392.33017,
        -11214424.18,
        651619640.7,
        -13362590354.0,
        57568490574.0,
    ];
    const A2: [f64; 6] = [
        1.0,
        267.8532712,
        59272.64853,
        9494680.718,
        1029532985.0,
        57568490411.0,
    ];
    const B1: [f64; 5] = [
        0.2093887211e-6,
        -0.2073370639e-5,
        0.2734510407e-4,
        -0.1098628627e-2,
        1.0,
    ];
    const B2: [f64; 5] = [
        -0.934935152e-7,
        0.7621095161e-6,
        -0.6911147651e-5,
        0.1430488765e-3,
        -0.1562499995e-1,
    ];
    let y = x * x;
    if x < 8.0 {
        return horner(&A1, y) / horner(&A2, y);
    }
    let xx = x - 0.785398164;
    let y = 64.0 / y;
    (W / x).sqrt() * (xx.cos() * horner(&B1, y) - xx.sin() * horner(&B2, y) * 8.0 / x)
}

fn j1(x: f64) -> f64 {
    const A1: [f64; 6] = [
        -30.16036606,
        15704.48260,
        -2972611.439,
        242396853.1,
        -7895059235.0,
        72362614232.0,
    ];
    const A2: [f64; 6] = [
        1.0,
        376.9991397,
        99447.43394,
        18583304.74,
        2300535178.0,
        144725228442.0,
    ];
    const B1: [f64; 5] = [
        -0.240337019e-6,
        0.2457520174e-5,
        -0.3516396496e-4,
        0.183105e-2,
        1.0,
    ];
    const B2: [f64; 5] = [
        0.105787412e-6,
        -0.88228987e-6,
        0.8449199096e-5,
        -0.2002690873e-3,
        0.04687499995,
    ];
    let y = x * x;
    let xx = x.abs() - 2.356194491;
    if x.abs() < 8.0 {
        return x * horner(&A1, y) / horner(&A2, y);
    }
    let y = 64.0 / y;
    let a = (W / x.abs()).sqrt()
        * (xx.cos() * horner(&B1, y) - xx.sin() * horner(&B2, y) * 8.0 / x.abs());
    if x < 0.0 {
        -a
    } else {
        a
    }
}

pub(crate) fn bessel_j(x: f64, n: i64) -> f64 {
    if !x.is_finite() {
        return if x.is_nan() { x } else { 0.0 };
    }
    let odd_sign = if n % 2 != 0 { -1.0 } else { 1.0 };
    if n < 0 {
        return odd_sign * bessel_j(x, -n);
    }
    if x < 0.0 {
        return odd_sign * bessel_j(-x, n);
    }
    match n {
        0 => return j0(x),
        1 => return j1(x),
        _ if x == 0.0 => return 0.0,
        _ => {}
    }
    if x > n as f64 {
        return bessel_iter(x, n, j0(x), j1(x), -1.0);
    }
    // Miller's downward recurrence, normalised by the even-order sum
    let m = 2 * ((n + (40.0 * n as f64).sqrt().floor() as i64) / 2);
    let (mut jsum, mut bjp, mut sum, mut bj, mut ret) = (false, 0.0, 0.0, 1.0, 0.0);
    let tox = 2.0 / x;
    for j in (1..=m).rev() {
        let bjm = j as f64 * tox * bj - bjp;
        bjp = bj;
        bj = bjm;
        if bj.abs() > 1e10 {
            bj *= 1e-10;
            bjp *= 1e-10;
            ret *= 1e-10;
            sum *= 1e-10;
        }
        if jsum {
            sum += bj;
        }
        jsum = !jsum;
        if j == n {
            ret = bjp;
        }
        if m - j > 100 && ret == 0.0 {
            return f64::NAN;
        }
    }
    sum = 2.0 * sum - bj;
    ret / sum
}

fn y0(x: f64) -> f64 {
    const A1: [f64; 6] = [
        228.4622733,
        -86327.92757,
        10879881.29,
        -512359803.6,
        7062834065.0,
        -2957821389.0,
    ];
    const A2: [f64; 6] = [
        1.0,
        226.1030244,
        47447.26470,
        7189466.438,
        745249964.8,
        40076544269.0,
    ];
    const B1: [f64; 5] = [
        0.2093887211e-6,
        -0.2073370639e-5,
        0.2734510407e-4,
        -0.1098628627e-2,
        1.0,
    ];
    const B2: [f64; 5] = [
        -0.934945152e-7,
        0.7621095161e-6,
        -0.6911147651e-5,
        0.1430488765e-3,
        -0.1562499995e-1,
    ];
    let y = x * x;
    let xx = x - 0.785398164;
    if x < 8.0 {
        return horner(&A1, y) / horner(&A2, y) + W * bessel_j(x, 0) * x.ln();
    }
    let y = 64.0 / y;
    (W / x).sqrt() * (xx.sin() * horner(&B1, y) + xx.cos() * horner(&B2, y) * 8.0 / x)
}

fn y1(x: f64) -> f64 {
    const A1: [f64; 6] = [
        0.8511937935e4,
        -0.4237922726e7,
        0.7349264551e9,
        -0.5153438139e11,
        0.1275274390e13,
        -0.4900604943e13,
    ];
    const A2: [f64; 7] = [
        1.0,
        0.3549632885e3,
        0.1020426050e6,
        0.2245904002e8,
        0.3733650367e10,
        0.4244419664e12,
        0.2499580570e14,
    ];
    const B1: [f64; 5] = [
        -0.240337019e-6,
        0.2457520174e-5,
        -0.3516396496e-4,
        0.183105e-2,
        1.0,
    ];
    const B2: [f64; 5] = [
        0.105787412e-6,
        -0.88228987e-6,
        0.8449199096e-5,
        -0.2002690873e-3,
        0.04687499995,
    ];
    let y = x * x;
    let xx = x - 2.356194491;
    if x < 8.0 {
        return x * horner(&A1, y) / horner(&A2, y) + W * (bessel_j(x, 1) * x.ln() - 1.0 / x);
    }
    let y = 64.0 / y;
    (W / x).sqrt() * (xx.sin() * horner(&B1, y) + xx.cos() * horner(&B2, y) * 8.0 / x)
}

pub(crate) fn bessel_y(x: f64, n: i64) -> f64 {
    bessel_wrapped(x, n, (y0, y1), 1, -1.0)
}

fn i0(x: f64) -> f64 {
    const A: [f64; 7] = [
        0.45813e-2,
        0.360768e-1,
        0.2659732,
        1.2067492,
        3.0899424,
        3.5156229,
        1.0,
    ];
    const B: [f64; 9] = [
        0.392377e-2,
        -0.1647633e-1,
        0.2635537e-1,
        -0.2057706e-1,
        0.916281e-2,
        -0.157565e-2,
        0.225319e-2,
        0.1328592e-1,
        0.39894228,
    ];
    if x <= 3.75 {
        return horner(&A, x * x / (3.75 * 3.75));
    }
    x.abs().exp() / x.abs().sqrt() * horner(&B, 3.75 / x.abs())
}

fn i1(x: f64) -> f64 {
    const A: [f64; 7] = [
        0.32411e-3,
        0.301532e-2,
        0.2658733e-1,
        0.15084934,
        0.51498869,
        0.87890594,
        0.5,
    ];
    const B: [f64; 9] = [
        -0.420059e-2,
        0.1787654e-1,
        -0.2895312e-1,
        0.2282967e-1,
        -0.1031555e-1,
        0.163801e-2,
        -0.362018e-2,
        -0.3988024e-1,
        0.39894228,
    ];
    if x < 3.75 {
        return x * horner(&A, x * x / (3.75 * 3.75));
    }
    (if x < 0.0 { -1.0 } else { 1.0 }) * x.abs().exp() / x.abs().sqrt() * horner(&B, 3.75 / x.abs())
}

/// JavaScript `Math.round`: halves round towards positive infinity.
fn js_round(v: f64) -> f64 {
    (v + 0.5).floor()
}

pub(crate) fn bessel_i(x: f64, n: i64) -> f64 {
    match n {
        0 => return i0(x),
        1 => return i1(x),
        n if n < 0 => return f64::NAN,
        _ => {}
    }
    if x == 0.0 {
        return 0.0;
    }
    if x == f64::INFINITY {
        return f64::INFINITY;
    }
    let tox = 2.0 / x.abs();
    let (mut bip, mut bi, mut ret) = (0.0, 1.0, 0.0);
    let m = 2 * js_round((n as f64 + js_round((40.0 * n as f64).sqrt())) / 2.0) as i64;
    for j in (1..=m).rev() {
        let bim = j as f64 * tox * bi + bip;
        bip = bi;
        bi = bim;
        if bi.abs() > 1e10 {
            bi *= 1e-10;
            bip *= 1e-10;
            ret *= 1e-10;
        }
        if j == n {
            ret = bip;
        }
        if m - j > 100 && ret == 0.0 {
            return f64::NAN;
        }
    }
    ret *= i0(x) / bi;
    if x < 0.0 && n % 2 != 0 {
        -ret
    } else {
        ret
    }
}

fn k0(x: f64) -> f64 {
    const A: [f64; 7] = [
        0.74e-5,
        0.10750e-3,
        0.262698e-2,
        0.3488590e-1,
        0.23069756,
        0.42278420,
        -0.57721566,
    ];
    const B: [f64; 7] = [
        0.53208e-3,
        -0.251540e-2,
        0.587872e-2,
        -0.1062446e-1,
        0.2189568e-1,
        -0.7832358e-1,
        1.25331414,
    ];
    if x <= 2.0 {
        return -(x / 2.0).ln() * bessel_i(x, 0) + horner(&A, x * x / 4.0);
    }
    (-x).exp() / x.sqrt() * horner(&B, 2.0 / x)
}

fn k1(x: f64) -> f64 {
    const A: [f64; 7] = [
        -0.4686e-4,
        -0.110404e-2,
        -0.1919402e-1,
        -0.18156897,
        -0.67278579,
        0.15443144,
        1.0,
    ];
    const B: [f64; 7] = [
        -0.68245e-3,
        0.325614e-2,
        -0.780353e-2,
        0.1504268e-1,
        -0.3655620e-1,
        0.23498619,
        1.25331414,
    ];
    if x <= 2.0 {
        return (x / 2.0).ln() * bessel_i(x, 1) + (1.0 / x) * horner(&A, x * x / 4.0);
    }
    (-x).exp() / x.sqrt() * horner(&B, 2.0 / x)
}

pub(crate) fn bessel_k(x: f64, n: i64) -> f64 {
    bessel_wrapped(x, n, (k0, k1), 2, 1.0)
}

impl SheetEngine {
    pub(crate) fn eval_bessel_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        let a = self.fin_args(args, (2, 2), ArgRule::BlankNa, ctx, depth)?;
        let (x, order) = (a.num(0)?, a.num(1)?.floor());
        if !(0.0..=MAX_BESSEL_ORDER).contains(&order) {
            return Err(FormulaError::Num);
        }
        let n = order as i64;
        let result = match name {
            "BESSELJ" => bessel_j(x, n),
            "BESSELY" => bessel_y(x, n),
            "BESSELI" => bessel_i(x, n),
            _ => bessel_k(x, n),
        };
        if result.is_finite() {
            Ok(EvaluatedCellValue::Number(result))
        } else {
            Err(FormulaError::Num)
        }
    }
}

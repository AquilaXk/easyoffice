//! Numerical optimization and spectral autocorrelation kernels for `functions_forecast_ets_cr`:
//! box-bounded Nelder-Mead simplex search and Wiener-Khinchin radix-2 FFT period detection.
//!
//! License: Apache-2.0. Box-bounded Nelder-Mead simplex search (`nelder_mead`) adapted from SciPy
//! `scipy/optimize/_optimize.py` lines 678-920 (`_minimize_neldermead`, BSD-3-Clause, Copyright
//! (c) 2001-2002 Enthought, Inc. 2003, SciPy Developers; Lagarias, Reeds, Wright and Wright 1998,
//! *SIAM Journal on Optimization* 9(1), 112-147), specialized to stack-allocated `[f64; 3]`
//! vertices with interior reflection on `[0, 1]`; see `THIRD_PARTY_NOTICES.md`. Radix-2 FFT
//! autocorrelation follows Cooley and Tukey (1965) and the Wiener-Khinchin theorem.

use crate::functions_forecast_ets_cr::{
    charge_bytes, charge_ops, BudgetFn, EtsError, Init, Params, MAX_EVALS, MAX_SEASONALITY,
};
use std::f64::consts::PI;

/// Values tried for each smoothing coefficient in the coarse grid stage.
const GRID: [f64; 6] = [0.02, 0.1, 0.25, 0.5, 0.75, 0.95];
/// Starting points the Nelder-Mead simplex search is run from.
const STARTS: usize = 3;
/// Initial coordinate perturbation of a simplex vertex in `[0, 1]`.
const SIMPLEX_STEP: f64 = 0.08;
/// Coordinate and relative loss tolerances at which the simplex search stops.
const SIMPLEX_XTOL: f64 = 1e-6;
const SIMPLEX_FTOL: f64 = 1e-10;
/// Product `len * max_lag` above which autocorrelation switches to radix-2 FFT.
const FFT_THRESHOLD: usize = 4_096;
/// A candidate period must reach this share of the highest autocorrelation peak.
const PEAK_SHARE: f64 = 0.9;
/// Difference variance (relative to the mean square of the data) at or below which the data is a line.
const FLAT_VARIANCE: f64 = 1e-20;

/// Smoothing coefficients that minimize the SSE of the one-step errors: a coarse grid in
/// `[0, 1]^k`, then a box-bounded Nelder-Mead simplex search from the best grid points.
pub(crate) fn optimize_bounded(
    y: &[f64],
    init: &Init,
    sse_of: impl Fn(&[f64], &Init, Params, &mut Vec<f64>) -> f64,
    budget: BudgetFn,
) -> Result<Params, EtsError> {
    let seasonal = !init.seasonal.is_empty();
    let mut ring = Vec::with_capacity(init.seasonal.len());
    let mut evals = 0usize;
    let mut aborted = false;
    let eval_loss = |p: Params,
                     ring: &mut Vec<f64>,
                     evals: &mut usize,
                     aborted: &mut bool,
                     budget: BudgetFn|
     -> f64 {
        if *aborted {
            return f64::INFINITY;
        }
        if charge_ops(budget, y.len()).is_err() {
            *aborted = true;
            return f64::INFINITY;
        }
        *evals += 1;
        let sse = sse_of(y, init, p, ring);
        if sse.is_finite() {
            sse
        } else {
            f64::INFINITY
        }
    };
    let gammas: &[f64] = if seasonal { &GRID } else { &[0.0] };
    let mut best = [(
        f64::INFINITY,
        Params {
            alpha: GRID[0],
            beta: GRID[0],
            gamma: 0.0,
        },
    ); STARTS];
    let mut filled = 0usize;
    for &alpha in &GRID {
        for &beta in &GRID {
            for &gamma in gammas {
                let p = Params { alpha, beta, gamma };
                let value = eval_loss(p, &mut ring, &mut evals, &mut aborted, budget);
                if aborted {
                    return Err(EtsError::BudgetExceeded);
                }
                let at = best[..filled]
                    .iter()
                    .position(|&(v, _)| value < v)
                    .unwrap_or(filled);
                if at < STARTS {
                    let end = filled.min(STARTS - 1);
                    best.copy_within(at..end, at + 1);
                    best[at] = (value, p);
                    filled = (filled + 1).min(STARTS);
                }
            }
        }
    }
    if best[0].0 == 0.0 {
        return Ok(best[0].1);
    }
    let per_start = MAX_EVALS.saturating_sub(evals) / filled.max(1);
    let mut winner = best[0];
    for &(start_value, start) in &best[..filled] {
        let found = nelder_mead(start, start_value, seasonal, per_start, &mut |p, local| {
            let mut dummy = 0usize;
            *local += 1;
            eval_loss(p, &mut ring, &mut dummy, &mut aborted, budget)
        });
        if aborted {
            return Err(EtsError::BudgetExceeded);
        }
        if found.0 < winner.0 {
            winner = found;
        }
    }
    Ok(winner.1)
}

/// Box-bounded Nelder-Mead simplex search in `[0, 1]^dims` (`dims` is 2 or 3).
fn nelder_mead(
    start: Params,
    start_value: f64,
    seasonal: bool,
    max_evals: usize,
    loss: &mut dyn FnMut(Params, &mut usize) -> f64,
) -> (f64, Params) {
    let dims = if seasonal { 3 } else { 2 };
    let mut sim = [[0.0; 3]; 4];
    let mut fsim = [f64::INFINITY; 4];
    sim[0] = start.to_coords();
    fsim[0] = start_value;
    let mut evals = 0usize;
    for k in 0..dims {
        let mut v = sim[0];
        let shifted = v[k] + SIMPLEX_STEP;
        v[k] = (if shifted > 1.0 {
            2.0 - shifted
        } else {
            shifted
        })
        .clamp(0.0, 1.0);
        sim[k + 1] = v;
        if evals >= max_evals {
            return (fsim[0], Params::from_coords(sim[0], seasonal));
        }
        fsim[k + 1] = loss(Params::from_coords(v, seasonal), &mut evals);
    }
    sort_simplex(&mut sim, &mut fsim, dims + 1);

    let inv_d = 1.0 / dims as f64;
    while evals < max_evals {
        let (mut xspread, mut fspread) = (0.0_f64, 0.0_f64);
        for j in 1..=dims {
            fspread = fspread.max((fsim[j] - fsim[0]).abs());
            for d in 0..dims {
                xspread = xspread.max((sim[j][d] - sim[0][d]).abs());
            }
        }
        if xspread <= SIMPLEX_XTOL && fspread <= SIMPLEX_FTOL * fsim[0].abs().max(1.0) {
            break;
        }
        let mut xbar = [0.0; 3];
        for vertex in &sim[..dims] {
            for d in 0..dims {
                xbar[d] += vertex[d] * inv_d;
            }
        }
        let worst = sim[dims];
        let xr = step_box(&xbar, &worst, 1.0, dims);
        let fxr = loss(Params::from_coords(xr, seasonal), &mut evals);
        let mut shrink = false;
        if fxr < fsim[0] {
            if evals < max_evals {
                let xe = step_box(&xbar, &worst, 2.0, dims);
                let fxe = loss(Params::from_coords(xe, seasonal), &mut evals);
                if fxe < fxr {
                    (sim[dims], fsim[dims]) = (xe, fxe);
                } else {
                    (sim[dims], fsim[dims]) = (xr, fxr);
                }
            } else {
                (sim[dims], fsim[dims]) = (xr, fxr);
            }
        } else if fxr < fsim[dims - 1] {
            (sim[dims], fsim[dims]) = (xr, fxr);
        } else if evals < max_evals {
            let (scale, limit) = if fxr < fsim[dims] {
                (0.5, fxr)
            } else {
                (-0.5, fsim[dims])
            };
            let xc = step_box(&xbar, &worst, scale, dims);
            let fxc = loss(Params::from_coords(xc, seasonal), &mut evals);
            if fxc < limit || (scale > 0.0 && fxc == limit) {
                (sim[dims], fsim[dims]) = (xc, fxc);
            } else {
                shrink = true;
            }
        } else {
            break;
        }
        if shrink {
            for j in 1..=dims {
                if evals >= max_evals {
                    break;
                }
                for d in 0..dims {
                    sim[j][d] = (sim[0][d] + 0.5 * (sim[j][d] - sim[0][d])).clamp(0.0, 1.0);
                }
                fsim[j] = loss(Params::from_coords(sim[j], seasonal), &mut evals);
            }
        }
        sort_simplex(&mut sim, &mut fsim, dims + 1);
    }
    (fsim[0], Params::from_coords(sim[0], seasonal))
}

#[inline]
fn step_box(xbar: &[f64; 3], worst: &[f64; 3], scale: f64, dims: usize) -> [f64; 3] {
    let mut out = [0.0; 3];
    for d in 0..dims {
        out[d] = (xbar[d] + scale * (xbar[d] - worst[d])).clamp(0.0, 1.0);
    }
    out
}

#[inline]
fn sort_simplex(sim: &mut [[f64; 3]; 4], fsim: &mut [f64; 4], count: usize) {
    for i in 1..count {
        let mut j = i;
        while j > 0 && fsim[j] < fsim[j - 1] {
            fsim.swap(j, j - 1);
            sim.swap(j, j - 1);
            j -= 1;
        }
    }
}

/// Iterative in-place Cooley-Tukey radix-2 FFT using hardware bit-reversal (`usize::reverse_bits`)
/// and chunked twiddle recurrences. `re.len()` must be a power of two (`>= 2`).
fn fft_radix2(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let shift = usize::BITS - n.trailing_zeros();
    for i in 0..n {
        let j = i.reverse_bits() >> shift;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut half = 1usize;
    while half < n {
        let step = half << 1;
        let angle = -PI / half as f64;
        let (dsin, dcos) = angle.sin_cos();
        for base in (0..n).step_by(step) {
            let (mut wre, mut wim) = (1.0, 0.0);
            for k in 0..half {
                if k != 0 && (k & 63) == 0 {
                    (wim, wre) = (angle * k as f64).sin_cos();
                }
                let i0 = base + k;
                let i1 = i0 + half;
                let tre = wre * re[i1] - wim * im[i1];
                let tim = wre * im[i1] + wim * re[i1];
                re[i1] = re[i0] - tre;
                im[i1] = im[i0] - tim;
                re[i0] += tre;
                im[i0] += tim;
                let next_wre = wre * dcos - wim * dsin;
                wim = wre * dsin + wim * dcos;
                wre = next_wre;
            }
        }
        half = step;
    }
}

/// Normalized autocorrelation `r[0..=max_lag]` via Wiener-Khinchin theorem (`O(N log N)`).
pub(crate) fn autocorr_fft(
    d: &[f64],
    max_lag: usize,
    budget: BudgetFn,
) -> Result<Vec<f64>, EtsError> {
    let fft_len = (d.len() * 2).next_power_of_two();
    let log2 = fft_len.trailing_zeros() as usize;
    charge_ops(budget, fft_len.saturating_mul(log2 * 4))?;
    charge_bytes(budget, fft_len.saturating_mul(16))?;
    let mut re = vec![0.0; fft_len];
    let mut im = vec![0.0; fft_len];
    re[..d.len()].copy_from_slice(d);
    fft_radix2(&mut re, &mut im);
    for (r, i) in re.iter_mut().zip(&mut im) {
        *r = *r * *r + *i * *i;
        *i = 0.0;
    }
    fft_radix2(&mut re, &mut im);
    let c0 = re[0];
    Ok((0..=max_lag)
        .map(|k| if k == 0 { 1.0 } else { re[k] / c0 })
        .collect())
}

pub(crate) fn autocorr_direct(
    d: &[f64],
    c0: f64,
    max_lag: usize,
    budget: BudgetFn,
) -> Result<Vec<f64>, EtsError> {
    charge_ops(budget, d.len().saturating_mul(max_lag))?;
    Ok((0..=max_lag)
        .map(|k| {
            if k == 0 {
                1.0
            } else {
                d[k..].iter().zip(d).map(|(a, b)| a * b).sum::<f64>() / c0
            }
        })
        .collect())
}

/// Normalized autocorrelation peaks of the first differences and the period they imply.
/// Returns `1` when no seasonal cycle is detected.
pub(crate) fn detect_period_bounded(y: &[f64], budget: BudgetFn) -> Result<usize, EtsError> {
    let n = y.len();
    if n < 6 {
        return Ok(1);
    }
    let mut d: Vec<f64> = y.windows(2).map(|w| w[1] - w[0]).collect();
    let mean = d.iter().sum::<f64>() / d.len() as f64;
    d.iter_mut().for_each(|v| *v -= mean);
    let c0: f64 = d.iter().map(|v| v * v).sum();
    let mean_square = y.iter().map(|v| v * v).sum::<f64>() / n as f64;
    if c0.is_nan() || c0 <= FLAT_VARIANCE * mean_square * d.len() as f64 {
        return Ok(1);
    }
    let max_lag = MAX_SEASONALITY.min(n / 2);
    let r = if d.len().saturating_mul(max_lag) > FFT_THRESHOLD {
        autocorr_fft(&d, max_lag, budget)?
    } else {
        autocorr_direct(&d, c0, max_lag, budget)?
    };
    let bound = 1.96 / (d.len() as f64).sqrt();
    let is_peak = |k: usize| {
        let next = if k < max_lag {
            r[k + 1]
        } else {
            f64::NEG_INFINITY
        };
        r[k] > bound && r[k] >= r[k - 1] && r[k] > next
    };
    let mut top = f64::NEG_INFINITY;
    for k in 2..=max_lag {
        if is_peak(k) && r[k] > top {
            top = r[k];
        }
    }
    if !top.is_finite() {
        return Ok(1);
    }
    let threshold = PEAK_SHARE * top;
    Ok((2..=max_lag)
        .find(|&k| is_peak(k) && r[k] >= threshold)
        .unwrap_or(1))
}

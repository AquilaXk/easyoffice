//! Regression numerics of `LINEST`, `LOGEST`, `TREND` and `GROWTH` (PD-201).
//!
//! Reference implementation: `packages/engine-formula/src/basics/statistical.ts`
//! (`getSlopeAndIntercept`, `solveLeastSquaresByQR`, `getKnownsArrayCoefficients`) and
//! `src/functions/statistical/{linest,logest}/index.ts` (the statistics rows) and
//! `src/basics/math.ts` (`inverseMatrixByLUD`, the pseudoinverse fallback) of the Univer formula
//! engine, commit `5a06a7842c8e13fa0646d0733dc3662cf4b330b9`, Apache-2.0, Copyright 2023-present
//! DreamNum Co., Ltd. See `THIRD_PARTY_NOTICES.md`. The operands and the argument rules are in
//! `functions_trend.rs`.
//!
//! The rank-deficient fallback is the pseudoinverse of the normal matrix computed by a Jacobi
//! eigenvalue sweep (the reference runs a bidiagonal SVD; both give the minimum-norm solution).
//! A regression is bounded: [`MAX_PREDICTORS`] predictors, [`MAX_FIT_OPS`] multiply-adds and
//! [`MAX_JACOBI_SWEEPS`] sweeps; over a limit the result is `#VALUE!`.

use crate::functions_matrix::{inverse_with, multiply, Matrix};
use crate::functions_trend::{error_cell, na_cell};
use crate::FormulaError as E;

type Fallible<T> = Result<T, E>;

/// Predictor columns (including the constant) of one regression.
const MAX_PREDICTORS: usize = 256;
/// Multiply-adds of one regression (the $O(n k^2)$ fit).
const MAX_FIT_OPS: usize = 64_000_000;
/// Sweeps of the eigenvalue iteration of the rank-deficient fallback.
const MAX_JACOBI_SWEEPS: usize = 60;
/// Pivot threshold of the normal-matrix inverse.
const MIN_PIVOT: f64 = 1e-11;

pub(crate) fn serial_numbers(rows: usize, cols: usize) -> Vec<Vec<f64>> {
    (0..rows)
        .map(|r| (0..cols).map(|c| (r * cols + c + 1) as f64).collect())
        .collect()
}

pub(crate) fn transpose(m: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let cols = m.first().map_or(0, Vec::len);
    (0..cols)
        .map(|c| m.iter().map(|row| row[c]).collect())
        .collect()
}

pub(crate) fn to_matrix(rows: &[Vec<f64>]) -> Matrix {
    let cols = rows.first().map_or(0, Vec::len);
    Matrix {
        rows: rows.len(),
        cols,
        data: rows.iter().flatten().copied().collect(),
    }
}

pub(crate) fn from_matrix(m: &Matrix) -> Vec<Vec<f64>> {
    m.data.chunks(m.cols.max(1)).map(<[f64]>::to_vec).collect()
}

/// Regression of one response column on the predictors, as `getKnownsArrayCoefficients` does it.
pub(crate) struct Fit {
    /// `[m_k, ..., m_1, b]`.
    pub(crate) coefficients: Vec<f64>,
    /// The (transformed) responses, one per observation.
    pub(crate) y: Vec<f64>,
    /// The predictor rows as fitted (with the constant column when `constb`).
    pub(crate) x: Vec<Vec<f64>>,
    /// The predictor rows of the points to predict (without the constant column).
    pub(crate) new_x: Vec<Vec<f64>>,
    pub(crate) xtx_inverse: Vec<Vec<f64>>,
}

/// Least squares by Householder reflections on column-scaled predictors; `None` when the
/// predictors are rank deficient. The predictors are stored by column.
pub(crate) fn solve_least_squares(x: &[Vec<f64>], y: &[f64]) -> Option<Vec<f64>> {
    let (rows, columns) = (x.len(), x.first()?.len());
    if columns == 0 || rows < columns || y.len() != rows {
        return None;
    }
    let mut qr: Vec<Vec<f64>> = (0..columns)
        .map(|c| x.iter().map(|row| row[c]).collect())
        .collect();
    let mut scales = Vec::with_capacity(columns);
    for column in &mut qr {
        let scale = column.iter().map(|v| v * v).sum::<f64>().sqrt();
        if !scale.is_finite() || scale == 0.0 {
            return None;
        }
        scales.push(scale);
        column.iter_mut().for_each(|v| *v /= scale);
    }
    let mut transformed = y.to_vec();
    let eps = f64::EPSILON * rows as f64;
    for pivot in 0..columns {
        let norm = qr[pivot][pivot..].iter().map(|v| v * v).sum::<f64>().sqrt();
        if !norm.is_finite() || norm <= eps {
            return None;
        }
        let alpha = if qr[pivot][pivot] >= 0.0 { -norm } else { norm };
        let mut reflector = qr[pivot][pivot..].to_vec();
        reflector[0] -= alpha;
        let reflector_norm: f64 = reflector.iter().map(|v| v * v).sum();
        if !reflector_norm.is_finite() || reflector_norm == 0.0 {
            return None;
        }
        let reflect = |target: &mut [f64]| {
            let projection: f64 = reflector.iter().zip(&*target).map(|(r, t)| r * t).sum();
            let factor = 2.0 * projection / reflector_norm;
            for (t, r) in target.iter_mut().zip(&reflector) {
                *t -= factor * r;
            }
        };
        for column in &mut qr[pivot..] {
            reflect(&mut column[pivot..]);
        }
        reflect(&mut transformed[pivot..]);
    }
    let mut coefficients = vec![0.0; columns];
    for row in (0..columns).rev() {
        let diagonal = qr[row][row];
        if !diagonal.is_finite() || diagonal.abs() <= eps {
            return None;
        }
        let remainder: f64 = (row + 1..columns)
            .map(|c| qr[c][row] * coefficients[c])
            .sum();
        coefficients[row] = (transformed[row] - remainder) / diagonal;
    }
    Some(
        coefficients
            .iter()
            .zip(&scales)
            .map(|(c, s)| c / s)
            .collect(),
    )
}

/// Pseudoinverse of a symmetric positive semi-definite matrix by cyclic Jacobi rotations.
pub(crate) fn pseudo_inverse_symmetric(m: &Matrix) -> Option<Matrix> {
    let n = m.rows;
    let mut a = m.data.clone();
    let mut v = vec![0.0; n * n];
    for i in 0..n {
        v[i * n + i] = 1.0;
    }
    for _ in 0..MAX_JACOBI_SWEEPS {
        let off: f64 = (0..n)
            .flat_map(|i| (0..n).filter(move |&j| j != i).map(move |j| (i, j)))
            .map(|(i, j)| a[i * n + j] * a[i * n + j])
            .sum();
        let diagonal: f64 = (0..n).map(|i| a[i * n + i] * a[i * n + i]).sum();
        if off <= f64::EPSILON * f64::EPSILON * diagonal.max(f64::MIN_POSITIVE) {
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                let apq = a[p * n + q];
                if apq == 0.0 {
                    continue;
                }
                let theta = (a[q * n + q] - a[p * n + p]) / (2.0 * apq);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let (c, s) = (1.0 / (t * t + 1.0).sqrt(), t / (t * t + 1.0).sqrt());
                for k in 0..n {
                    let (akp, akq) = (a[k * n + p], a[k * n + q]);
                    a[k * n + p] = c * akp - s * akq;
                    a[k * n + q] = s * akp + c * akq;
                }
                for k in 0..n {
                    let (apk, aqk) = (a[p * n + k], a[q * n + k]);
                    a[p * n + k] = c * apk - s * aqk;
                    a[q * n + k] = s * apk + c * aqk;
                }
                for k in 0..n {
                    let (vkp, vkq) = (v[k * n + p], v[k * n + q]);
                    v[k * n + p] = c * vkp - s * vkq;
                    v[k * n + q] = s * vkp + c * vkq;
                }
            }
        }
    }
    let largest = (0..n).map(|i| a[i * n + i].abs()).fold(0.0, f64::max);
    if !largest.is_finite() {
        return None;
    }
    let threshold = n as f64 * f64::EPSILON * largest;
    let mut out = Matrix::zeros(n, n);
    for k in 0..n {
        let lambda = a[k * n + k];
        if lambda.abs() <= threshold {
            continue;
        }
        for i in 0..n {
            for j in 0..n {
                out.data[i * n + j] += v[i * n + k] * v[j * n + k] / lambda;
            }
        }
    }
    Some(out)
}

/// `getKnownsArrayCoefficients`: the coefficients `[m_k, ..., m_1, b]`, the fitted rows and the
/// inverse of the normal matrix.
pub(crate) fn fit(
    ys: &[Vec<f64>],
    xs: &[Vec<f64>],
    new_xs: &[Vec<f64>],
    constb: bool,
    exponential: bool,
) -> Fallible<Fit> {
    let one_row = ys.len() == 1 && ys[0].len() > 1;
    let mut y: Vec<Vec<f64>> = ys.to_vec();
    if exponential {
        if y.iter().flatten().any(|&v| v <= 0.0 || v.is_nan()) {
            return Err(E::Num);
        }
        for v in y.iter_mut().flatten() {
            *v = v.ln();
        }
    }
    let (mut x, mut new_x) = (xs.to_vec(), new_xs.to_vec());
    if one_row {
        y = transpose(&y);
        x = transpose(&x);
        new_x = transpose(&new_x);
    }
    if constb {
        for row in &mut x {
            row.push(1.0);
        }
    }
    let (n, k) = (x.len(), x.first().map_or(0, Vec::len));
    if k == 0 || k > MAX_PREDICTORS || n.saturating_mul(k).saturating_mul(k) > MAX_FIT_OPS {
        return Err(E::Value);
    }
    let xm = to_matrix(&x);
    let xtx = multiply(&xm.transposed(), &xm);
    let inverse = inverse_with(&xtx, true, MIN_PIVOT)
        .or_else(|| pseudo_inverse_symmetric(&xtx))
        .ok_or(E::Na)?;
    let y_column: Vec<f64> = y.iter().map(|row| row[0]).collect();
    let mut solved = match solve_least_squares(&x, &y_column) {
        Some(coefficients) => coefficients,
        None => {
            let xty = multiply(&xm.transposed(), &to_matrix(&y));
            multiply(&inverse, &xty).data
        }
    };
    if !constb {
        solved.push(0.0);
    }
    let intercept = solved.pop().unwrap_or(0.0);
    solved.reverse();
    solved.push(intercept);
    if exponential {
        for c in &mut solved {
            *c = c.exp();
        }
    }
    Ok(Fit {
        coefficients: solved,
        y: y_column,
        x,
        new_x,
        xtx_inverse: from_matrix(&inverse),
    })
}

/// Slope and intercept of one predictor (`getSlopeAndIntercept`); the response is `ln y` when
/// `exponential`, and the coefficients are then exponentiated.
pub(crate) fn slope_intercept(
    xs: &[f64],
    ys: &[f64],
    constb: bool,
    exponential: bool,
) -> Fallible<(f64, f64, Vec<f64>)> {
    let y: Vec<f64> = if exponential {
        if ys.iter().any(|&v| v <= 0.0 || v.is_nan()) {
            return Err(E::Num);
        }
        ys.iter().map(|v| v.ln()).collect()
    } else {
        ys.to_vec()
    };
    let n = y.len() as f64;
    let (mut slope, mut intercept);
    if constb {
        let (mut sx, mut sy, mut sxx, mut sxy) = (0.0, 0.0, 0.0, 0.0);
        for (&x, &v) in xs.iter().zip(&y) {
            sx += x;
            sy += v;
            sxx += x * x;
            sxy += x * v;
        }
        slope = (n * sxy - sx * sy) / (n * sxx - sx * sx);
        intercept = 1.0 / n * sy - slope * (1.0 / n) * sx;
    } else {
        let sxx: f64 = xs.iter().map(|x| x * x).sum();
        let sxy: f64 = xs.iter().zip(&y).map(|(x, v)| x * v).sum();
        slope = sxy / sxx;
        intercept = 0.0;
    }
    if exponential {
        slope = slope.exp();
        intercept = intercept.exp();
    }
    if slope.is_nan() && !constb {
        slope = 0.0;
    }
    Ok((slope, intercept, y))
}

/// `LINEST` and `LOGEST` for several predictors.
pub(crate) fn estimate_multiple(
    ys: &[Vec<f64>],
    xs: &[Vec<f64>],
    constb: bool,
    stats: bool,
    exponential: bool,
) -> Fallible<Matrix> {
    let f = fit(ys, xs, xs, constb, exponential)?;
    let cl = f.coefficients.len();
    if !stats {
        return Ok(to_matrix(&[f.coefficients]));
    }
    let n = f.y.len();
    let mean_y = if constb {
        f.y.iter().sum::<f64>() / n as f64
    } else {
        0.0
    };
    let k = f.xtx_inverse.len();
    let df = n as f64 - k as f64;
    let b = f.coefficients[cl - 1];
    let predicted: Vec<f64> =
        f.x.iter()
            .map(|row| {
                let terms = (0..cl - 1)
                    .rev()
                    .map(|j| (f.coefficients[cl - 2 - j], row[j]));
                if exponential {
                    terms.fold(b, |acc, (m, x)| acc * m.powf(x)).ln()
                } else {
                    terms.fold(b, |acc, (m, x)| acc + m * x)
                }
            })
            .collect();
    let (mut sstotal, mut ssresid) = (0.0, 0.0);
    for (&y, &p) in f.y.iter().zip(&predicted) {
        sstotal += (y - mean_y).powi(2);
        if exponential && !constb && !p.is_finite() {
            continue;
        }
        ssresid += (y - p).powi(2);
    }
    if exponential && !ssresid.is_finite() {
        ssresid = 0.0;
    }
    let ssreg = sstotal - ssresid;
    let r2 = if sstotal == 0.0 { 0.0 } else { ssreg / sstotal };
    let mut se: Vec<f64> = (0..k)
        .rev()
        .map(|i| {
            if df > 0.0 {
                (ssresid / df * f.xtx_inverse[i][i]).sqrt()
            } else {
                0.0
            }
        })
        .collect();
    if constb {
        let seb = se.remove(0);
        se.push(seb);
    } else {
        se.push(na_cell());
    }
    let sey = if df > 0.0 { (ssresid / df).sqrt() } else { 0.0 };
    let statistic = if df > 0.0 {
        (ssreg / (cl - 1) as f64) / (ssresid / df)
    } else {
        error_cell(E::Num)
    };
    let pad = |a: f64, b: f64| {
        let mut row = vec![a, b];
        row.resize(cl, na_cell());
        row
    };
    Ok(to_matrix(&[
        f.coefficients.clone(),
        se,
        pad(r2, sey),
        pad(statistic, df),
        pad(ssreg, ssresid),
    ]))
}

/// `LINEST` and `LOGEST` for one predictor.
pub(crate) fn estimate_simple(
    ys: &[Vec<f64>],
    xs: &[Vec<f64>],
    constb: bool,
    stats: bool,
    exponential: bool,
) -> Fallible<Matrix> {
    let x: Vec<f64> = xs.iter().flatten().copied().collect();
    let y_flat: Vec<f64> = ys.iter().flatten().copied().collect();
    let (m, b, y) = slope_intercept(&x, &y_flat, constb, exponential)?;
    if m.is_nan() {
        return Err(E::Na);
    }
    if !stats {
        return Ok(to_matrix(&[vec![m, b]]));
    }
    let n = y.len() as f64;
    let (mut mean_y, mut mean_x, mut df) = (0.0, 0.0, n - 1.0);
    if constb {
        mean_y = y.iter().sum::<f64>() / n;
        mean_x = x.iter().sum::<f64>() / n;
        df = n - 2.0;
    }
    let (mut sstotal, mut ssresid, mut ssx) = (0.0, 0.0, 0.0);
    for (&xv, &yv) in x.iter().zip(&y) {
        sstotal += (yv - mean_y).powi(2);
        let fitted = if exponential {
            (b * m.powf(xv)).ln()
        } else {
            m * xv + b
        };
        ssresid += (yv - fitted).powi(2);
        ssx += (xv - mean_x).powi(2);
    }
    let ssreg = sstotal - ssresid;
    let r2 = if !exponential && ssreg == sstotal {
        1.0
    } else if exponential && sstotal == 0.0 {
        0.0
    } else {
        ssreg / sstotal
    };
    let (mut se, mut seb, mut sey, mut statistic) = (0.0, 0.0, 0.0, 0.0);
    if df > 0.0 {
        if ssx > 0.0 {
            se = (ssresid / df / ssx).sqrt();
            seb = (ssresid / df * (1.0 / n + mean_x * mean_x / ssx)).sqrt();
        }
        sey = (ssresid / df).sqrt();
        statistic = ssreg / (ssresid / df);
    } else if !exponential {
        statistic = error_cell(E::Num);
    }
    if !constb {
        seb = na_cell();
    }
    Ok(to_matrix(&[
        vec![m, b],
        vec![se, seb],
        vec![r2, sey],
        vec![statistic, df],
        vec![ssreg, ssresid],
    ]))
}

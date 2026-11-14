//! Matrix functions: `MMULT`, `MDETERM`, `MINVERSE`, `MUNIT`.
//!
//! Reference implementation: the LU decomposition (`performLUDecomposition`, row-major
//! pivot search, back substitution in `transformMatrix`), the determinant shortcuts for 1x1 and
//! 2x2 matrices, the pivot thresholds (`1e-11` for `MDETERM`, machine epsilon for `MINVERSE`) and the
//! error rules (blank, logical and non-numeric text cells are `#VALUE!`, a singular matrix is `#NUM!`,
//! a non-square matrix is `#VALUE!`) are ported from the Univer formula engine
//! (`packages/engine-formula/src/functions/math/{mmult,mdeterm,minverse,munit}/index.ts` and
//! `src/basics/math.ts`, commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9), Apache-2.0,
//! Copyright 2023-present DreamNum Co., Ltd. See `THIRD_PARTY_NOTICES.md`.
//!
//! Differences from the original: matrices live in one row-major `Vec<f64>` (no row vectors),
//! `MMULT` multiplies in `i-k-j` order (the same per-cell summation order, contiguous inner loop),
//! and every size is bounded (`MAX_MATRIX_CELLS`, `MAX_INVERT_DIM`, `MAX_MMULT_OPS`; over a limit the
//! result is `#VALUE!`).
//!
//! The cell model holds one value per cell, so an array result cannot be stored. A matrix function in
//! a scalar position returns its top-left element; as an argument of `INDEX`, `SUM`-style aggregates,
//! `SUMPRODUCT` or another matrix function it is expanded to the full matrix ([`SheetEngine::matrix_expr`]).

use crate::coercion::coerce_to_number;
use crate::functions_lookup::{check_lookup_arity, effective_index};
use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};

type CellResult = Result<EvaluatedCellValue, FormulaError>;

/// Cells of one operand or result (512 x 512).
pub(crate) const MAX_MATRIX_CELLS: usize = 262_144;
/// Side length limit of `MDETERM` and `MINVERSE` (the LU decomposition costs $O(n^3)$).
const MAX_INVERT_DIM: usize = 256;
/// Multiply-add limit of one `MMULT` (`rows * inner * cols`).
const MAX_MMULT_OPS: usize = 64_000_000;
/// Pivot threshold of `MDETERM`: a smaller pivot makes the determinant 0.
const DETERM_MIN_PIVOT: f64 = 1e-11;

/// Row-major dense matrix.
pub(crate) struct Matrix {
    pub(crate) rows: usize,
    pub(crate) cols: usize,
    pub(crate) data: Vec<f64>,
}

impl Matrix {
    /// Cell `idx` as a value: a number, or the error a result function stored with
    /// [`crate::functions_trend::error_cell`].
    pub(crate) fn value_at(&self, idx: usize) -> EvaluatedCellValue {
        crate::functions_trend::cell_value(self.data[idx])
    }

    pub(crate) fn zeros(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            data: vec![0.0; rows * cols],
        }
    }

    pub(crate) fn transposed(&self) -> Self {
        let mut out = Self::zeros(self.cols, self.rows);
        for r in 0..self.rows {
            for c in 0..self.cols {
                out.data[c * self.rows + r] = self.data[r * self.cols + c];
            }
        }
        out
    }

    fn swap_rows(&mut self, a: usize, b: usize) {
        if a == b {
            return;
        }
        let (lo, hi) = (a.min(b), a.max(b));
        let (head, tail) = self.data.split_at_mut(hi * self.cols);
        head[lo * self.cols..(lo + 1) * self.cols].swap_with_slice(&mut tail[..self.cols]);
    }
}

/// Functions whose result is a matrix (see the module note).
pub(crate) fn is_array_fn(name: &str) -> bool {
    matches!(name, "MMULT" | "MINVERSE" | "MUNIT")
        || crate::functions_trend::is_trend_array_fn(name)
}

/// Result of [`lu_decompose`].
struct Lu {
    /// `false` after an odd number of row swaps.
    even_swaps: bool,
    singular: bool,
    lu: Matrix,
    perm: Vec<usize>,
}

/// Crout-style LU decomposition with partial pivoting; a pivot not above `min_pivot` (or not
/// finite) marks the matrix singular. `transpose` decomposes the transpose, as the determinant does.
fn lu_decompose_with(
    m: &Matrix,
    transpose: bool,
    min_pivot: f64,
    mut ctx: Option<&mut EvalContext<'_>>,
) -> Result<Lu, FormulaError> {
    let n = m.rows;
    let mut dec = if transpose {
        m.transposed()
    } else {
        Matrix {
            rows: n,
            cols: n,
            data: m.data.clone(),
        }
    };
    let mut lu = Matrix::zeros(n, n);
    let mut perm: Vec<usize> = (0..n).collect();
    let mut even_swaps = true;
    for c in 0..n {
        for i in 0..c {
            if let Some(c_ref) = ctx.as_deref_mut() {
                c_ref.charge_steps((i >> 6).max(1))?;
            }
            let mut value = dec.data[i * n + c];
            for k in 0..i {
                value -= lu.data[i * n + k] * lu.data[k * n + c];
            }
            lu.data[i * n + c] = value;
        }
        let mut max_pivot = f64::NEG_INFINITY;
        let mut pivot_row = c;
        for r in c..n {
            if let Some(c_ref) = ctx.as_deref_mut() {
                c_ref.charge_steps((c >> 6).max(1))?;
            }
            let mut value = dec.data[r * n + c];
            for k in 0..c {
                value -= lu.data[r * n + k] * lu.data[k * n + c];
            }
            lu.data[r * n + c] = value;
            if value.abs() > max_pivot {
                max_pivot = value.abs();
                pivot_row = r;
            }
        }
        let pivot = lu.data[pivot_row * n + c].abs();
        if !pivot.is_finite() || pivot <= min_pivot {
            return Ok(Lu {
                even_swaps,
                singular: true,
                lu,
                perm,
            });
        }
        if pivot_row != c {
            lu.swap_rows(c, pivot_row);
            dec.swap_rows(c, pivot_row);
            perm.swap(c, pivot_row);
            even_swaps = !even_swaps;
        }
        let pivot_element = lu.data[c * n + c];
        for r in c + 1..n {
            lu.data[r * n + c] /= pivot_element;
        }
    }
    Ok(Lu {
        even_swaps,
        singular: false,
        lu,
        perm,
    })
}

/// Determinant: closed forms up to 2x2, otherwise the product of the LU pivots (0 when singular).
fn determinant_with(m: &Matrix, ctx: &mut EvalContext<'_>) -> Result<f64, FormulaError> {
    match m.rows {
        1 => Ok(m.data[0]),
        2 => Ok(m.data[0] * m.data[3] - m.data[1] * m.data[2]),
        n => {
            let lu = lu_decompose_with(m, true, DETERM_MIN_PIVOT, Some(ctx))?;
            if lu.singular {
                return Ok(0.0);
            }
            let det = (0..n).fold(if lu.even_swaps { 1.0 } else { -1.0 }, |acc, i| {
                acc * lu.lu.data[i * n + i]
            });
            // -0 becomes 0
            Ok(if det == 0.0 { 0.0 } else { det })
        }
    }
}

/// [`inverse`] with the decomposition's transpose flag and pivot threshold chosen by the caller.
pub(crate) fn inverse_with(m: &Matrix, transpose: bool, min_pivot: f64) -> Option<Matrix> {
    inverse_with_ctx(m, transpose, min_pivot, None).unwrap_or(None)
}

fn inverse_with_ctx(
    m: &Matrix,
    transpose: bool,
    min_pivot: f64,
    mut ctx: Option<&mut EvalContext<'_>>,
) -> Result<Option<Matrix>, FormulaError> {
    let n = m.rows;
    let Lu {
        singular, lu, perm, ..
    } = lu_decompose_with(m, transpose, min_pivot, ctx.as_deref_mut())?;
    if singular {
        return Ok(None);
    }
    let mut out = Matrix::zeros(n, n);
    for (row, &src) in perm.iter().enumerate() {
        out.data[row * n + src] = 1.0;
    }
    for pivot in 0..n {
        for row in pivot + 1..n {
            if let Some(c_ref) = ctx.as_deref_mut() {
                c_ref.charge_steps((n >> 6).max(1))?;
            }
            let factor = lu.data[row * n + pivot];
            for col in 0..n {
                out.data[row * n + col] -= out.data[pivot * n + col] * factor;
            }
        }
    }
    for row in (0..n).rev() {
        let pivot_element = lu.data[row * n + row];
        for col in 0..n {
            out.data[row * n + col] /= pivot_element;
        }
        for upper in 0..row {
            if let Some(c_ref) = ctx.as_deref_mut() {
                c_ref.charge_steps((n >> 6).max(1))?;
            }
            let factor = lu.data[upper * n + row];
            for col in 0..n {
                out.data[upper * n + col] -= out.data[row * n + col] * factor;
            }
        }
    }
    Ok(Some(out))
}

/// `a * b` with the sum of each cell taken in inner-index order.
pub(crate) fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    multiply_with_ctx(a, b, None).unwrap_or_else(|_| Matrix::zeros(a.rows, b.cols))
}

fn multiply_with_ctx(
    a: &Matrix,
    b: &Matrix,
    mut ctx: Option<&mut EvalContext<'_>>,
) -> Result<Matrix, FormulaError> {
    let mut out = Matrix::zeros(a.rows, b.cols);
    let step_cost = (b.cols >> 6).max(1);
    for i in 0..a.rows {
        let out_row = &mut out.data[i * b.cols..(i + 1) * b.cols];
        for k in 0..a.cols {
            if let Some(c_ref) = ctx.as_deref_mut() {
                c_ref.charge_steps(step_cost)?;
            }
            let scale = a.data[i * a.cols + k];
            let b_row = &b.data[k * b.cols..(k + 1) * b.cols];
            for (o, &x) in out_row.iter_mut().zip(b_row) {
                *o += scale * x;
            }
        }
    }
    Ok(out)
}

/// A matrix cell: numbers and numeric text; blank, logical and other text cells are `#VALUE!`.
fn matrix_number(v: EvaluatedCellValue) -> Result<f64, FormulaError> {
    match v {
        EvaluatedCellValue::Error(e) => Err(e),
        EvaluatedCellValue::Empty | EvaluatedCellValue::Boolean(_) => Err(FormulaError::Value),
        other => coerce_to_number(&other),
    }
}

impl SheetEngine {
    pub(crate) fn eval_matrix_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        match name {
            "MDETERM" => {
                check_lookup_arity(args, 1, 1)?;
                let m = self.matrix_operand(&args[0], ctx, depth)?;
                if m.rows != m.cols || m.rows > MAX_INVERT_DIM {
                    return Err(FormulaError::Value);
                }
                Ok(EvaluatedCellValue::Number(determinant_with(&m, ctx)?))
            }
            "MMULT" => {
                let (a, b) = self.mmult_operands(args, ctx, depth)?;
                ctx.charge_steps((a.cols >> 6).max(1))?;
                let top_left = (0..a.cols).fold(0.0, |sum, k| sum + a.data[k] * b.data[k * b.cols]);
                Ok(EvaluatedCellValue::Number(top_left))
            }
            _ => {
                let m = self.eval_matrix_call(name, args, ctx, depth)?;
                Ok(m.value_at(0))
            }
        }
    }

    /// Both `MMULT` operands, checked for conformable shape and the operation limit.
    fn mmult_operands(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<(Matrix, Matrix), FormulaError> {
        check_lookup_arity(args, 2, 2)?;
        let a = self.matrix_operand(&args[0], ctx, depth)?;
        let b = self.matrix_operand(&args[1], ctx, depth)?;
        let ops = a.rows.saturating_mul(a.cols).saturating_mul(b.cols);
        if a.cols != b.rows || ops > MAX_MMULT_OPS {
            return Err(FormulaError::Value);
        }
        Ok((a, b))
    }

    /// The full matrix when `expr` is a call of a matrix-valued function, else `None`.
    pub(crate) fn matrix_expr(
        &self,
        expr: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Option<Matrix>, FormulaError> {
        match expr.peeled() {
            Expr::FuncCall { name, args } if is_array_fn(name) => {
                self.eval_matrix_call(name, args, ctx, depth).map(Some)
            }
            _ => Ok(None),
        }
    }

    /// `INDEX(array, row, [column])` over an expanded matrix argument; `args` is the whole
    /// `INDEX` argument list.
    pub(crate) fn index_matrix(
        &self,
        m: &Matrix,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        let row_num = self.eval_arg_num(&args[1], ctx, depth)?.trunc() as isize;
        let col_num = match args.get(2) {
            Some(arg) => self.eval_arg_num(arg, ctx, depth)?.trunc() as isize,
            None => 1,
        };
        let (row, col) = effective_index(args.len(), (row_num, col_num), (m.rows, m.cols));
        if row < 1 || col < 1 || row as usize > m.rows || col as usize > m.cols {
            return Err(FormulaError::Ref);
        }
        Ok(m.value_at((row as usize - 1) * m.cols + (col as usize - 1)))
    }

    fn eval_matrix_call(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Matrix, FormulaError> {
        match name {
            "MMULT" => {
                let (a, b) = self.mmult_operands(args, ctx, depth)?;
                multiply_with_ctx(&a, &b, Some(ctx))
            }
            "MINVERSE" => {
                check_lookup_arity(args, 1, 1)?;
                let m = self.matrix_operand(&args[0], ctx, depth)?;
                if m.rows != m.cols || m.rows > MAX_INVERT_DIM {
                    return Err(FormulaError::Value);
                }
                inverse_with_ctx(&m, false, f64::EPSILON, Some(ctx))?.ok_or(FormulaError::Num)
            }
            "MUNIT" => {
                check_lookup_arity(args, 1, 1)?;
                let n = self.eval_arg_num(&args[0], ctx, depth)?.floor();
                if n < 1.0 || n * n > MAX_MATRIX_CELLS as f64 {
                    return Err(FormulaError::Value);
                }
                let n = n as usize;
                ctx.charge_steps((n >> 6).max(1))?;
                let mut m = Matrix::zeros(n, n);
                for i in 0..n {
                    m.data[i * n + i] = 1.0;
                }
                Ok(m)
            }
            _ => self.eval_trend_matrix(name, args, ctx, depth),
        }
    }

    /// One matrix argument: a range, a nested matrix function (also under `TRANSPOSE`) or a
    /// single value (1x1).
    fn matrix_operand(
        &self,
        expr: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Matrix, FormulaError> {
        if let Some(m) = self.matrix_expr(expr, ctx, depth + 1)? {
            return Ok(m);
        }
        if let Expr::FuncCall { name, args } = expr.peeled() {
            if name == "TRANSPOSE" && args.len() == 1 {
                if let Some(m) = self.matrix_expr(&args[0], ctx, depth + 1)? {
                    return Ok(m.transposed());
                }
            }
        }
        let Some(spec) = self.extract_streaming_range_ctx(expr, ctx, depth)? else {
            let v = self.eval_arg_val(expr, ctx, depth)?;
            return Ok(Matrix {
                rows: 1,
                cols: 1,
                data: vec![matrix_number(v)?],
            });
        };
        let (_, _, _, rows, cols, _) = spec;
        if rows.saturating_mul(cols) > MAX_MATRIX_CELLS {
            return Err(FormulaError::Value);
        }
        let mut m = Matrix::zeros(rows, cols);
        for r in 0..rows {
            ctx.charge_steps((cols >> 6).max(1))?;
            for c in 0..cols {
                let v = self.eval_streaming_cell_allow_error(&spec, r, c, ctx, depth)?;
                m.data[r * cols + c] = matrix_number(v)?;
            }
        }
        Ok(m)
    }
}

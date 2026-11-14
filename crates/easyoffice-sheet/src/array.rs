//! Array values of dynamic array formulas (PD-217): the grid a formula calculates, the `LAMBDA`
//! closures it passes around, and the scope of `LET` variables.

use crate::{EvaluatedCellValue, Expr, FormulaError};
use std::sync::Arc;

/// Cells one array may hold. It is the evaluation step budget (`MAX_EVAL_STEPS`): an array that is
/// bigger is refused with `#SPILL!` (the "out of memory" cause) before it is allocated.
pub(crate) const MAX_ARRAY_CELLS: usize = crate::MAX_EVAL_STEPS;

/// Per-root-cell dynamic array memory budget (128 MiB): prevents chained/nested arrays from
/// allocating arbitrarily large heaps within a single root formula evaluation.
pub(crate) const MAX_ARRAY_BYTES: usize = 128 * 1024 * 1024;

/// Nested calls of `LAMBDA` values. A function that calls itself through a variable ends here
/// with `#NUM!` instead of the Rust stack.
pub(crate) const MAX_LAMBDA_DEPTH: usize = 64;

/// Rows by columns of values, row by row.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Grid {
    pub(crate) rows: usize,
    pub(crate) cols: usize,
    pub(crate) data: Vec<EvaluatedCellValue>,
}

impl Grid {
    pub(crate) fn scalar(value: EvaluatedCellValue) -> Self {
        Self {
            rows: 1,
            cols: 1,
            data: vec![value],
        }
    }

    pub(crate) fn error(e: FormulaError) -> Self {
        Self::scalar(EvaluatedCellValue::Error(e))
    }

    pub(crate) fn from_rows(rows: usize, cols: usize, data: Vec<EvaluatedCellValue>) -> Self {
        debug_assert_eq!(rows * cols, data.len());
        Self { rows, cols, data }
    }

    pub(crate) fn from_rows_checked(
        ctx: &mut crate::EvalContext<'_>,
        rows: usize,
        cols: usize,
        data: Vec<EvaluatedCellValue>,
    ) -> Result<Self, FormulaError> {
        ctx.charge_array_cells(rows, cols)?;
        Ok(Self::from_rows(rows, cols, data))
    }

    /// A grid of `rows` by `cols` cells filled by `f(row, col)`; `#SPILL!` past the per-array or
    /// per-root array memory budget.
    pub(crate) fn build_checked(
        ctx: &mut crate::EvalContext<'_>,
        rows: usize,
        cols: usize,
        mut f: impl FnMut(usize, usize) -> EvaluatedCellValue,
    ) -> Result<Self, FormulaError> {
        ctx.charge_array_cells(rows, cols)?;
        let mut data = Vec::with_capacity(rows * cols);
        for r in 0..rows {
            for c in 0..cols {
                data.push(f(r, c));
            }
        }
        Ok(Self { rows, cols, data })
    }

    #[inline]
    pub(crate) fn get(&self, r: usize, c: usize) -> &EvaluatedCellValue {
        &self.data[r * self.cols + c]
    }

    pub(crate) fn is_scalar(&self) -> bool {
        self.data.len() == 1
    }

    pub(crate) fn top_left(&self) -> EvaluatedCellValue {
        self.data
            .first()
            .cloned()
            .unwrap_or(EvaluatedCellValue::Empty)
    }

    pub(crate) fn transposed(&self) -> Self {
        let mut data = Vec::with_capacity(self.data.len());
        for c in 0..self.cols {
            for r in 0..self.rows {
                data.push(self.get(r, c).clone());
            }
        }
        Self {
            rows: self.cols,
            cols: self.rows,
            data,
        }
    }

    pub(crate) fn transposed_checked(
        &self,
        ctx: &mut crate::EvalContext<'_>,
    ) -> Result<Self, FormulaError> {
        ctx.charge_array_cells(self.cols, self.rows)?;
        Ok(self.transposed())
    }

    /// The rows listed by `keep`, in that order.
    pub(crate) fn take_rows_checked(
        &self,
        ctx: &mut crate::EvalContext<'_>,
        keep: &[usize],
    ) -> Result<Self, FormulaError> {
        ctx.charge_array_cells(keep.len(), self.cols)?;
        let mut data = Vec::with_capacity(keep.len() * self.cols);
        for &r in keep {
            data.extend_from_slice(&self.data[r * self.cols..(r + 1) * self.cols]);
        }
        Ok(Self {
            rows: keep.len(),
            cols: self.cols,
            data,
        })
    }

    /// The columns listed by `keep`, in that order.
    pub(crate) fn take_cols_checked(
        &self,
        ctx: &mut crate::EvalContext<'_>,
        keep: &[usize],
    ) -> Result<Self, FormulaError> {
        ctx.charge_array_cells(self.rows, keep.len())?;
        let mut data = Vec::with_capacity(self.rows * keep.len());
        for r in 0..self.rows {
            for &c in keep {
                data.push(self.get(r, c).clone());
            }
        }
        Ok(Self {
            rows: self.rows,
            cols: keep.len(),
            data,
        })
    }
}

/// `Ok` when `rows * cols` cells fit the budget.
pub(crate) fn check_cells(rows: usize, cols: usize) -> Result<(), FormulaError> {
    if rows.checked_mul(cols).is_some_and(|n| n <= MAX_ARRAY_CELLS) {
        Ok(())
    } else {
        Err(FormulaError::Spill)
    }
}

/// Byte cost of `rows * cols` array cells, checked against [`MAX_ARRAY_CELLS`] and overflow.
pub(crate) fn array_bytes_for(rows: usize, cols: usize) -> Result<usize, FormulaError> {
    check_cells(rows, cols)?;
    rows.checked_mul(cols)
        .and_then(|n| n.checked_mul(std::mem::size_of::<EvaluatedCellValue>()))
        .ok_or(FormulaError::Spill)
}

/// A `LAMBDA` and the scope it was made in.
#[derive(Debug)]
pub(crate) struct Closure {
    pub(crate) params: Vec<String>,
    /// Parameters from this one on are optional.
    pub(crate) required: usize,
    pub(crate) body: Expr,
    pub(crate) env: Env,
}

/// What an expression stands for while an array formula is calculated.
#[derive(Debug, Clone)]
pub(crate) enum Val {
    Grid(Arc<Grid>),
    Lambda(Arc<Closure>),
    /// A reference a variable holds (`LET(x, A1, ROW(x))`): it stays a reference where the scalar
    /// engine wants one. It lives only in a scope; reading the variable gives the cells' values.
    Ref(Arc<Expr>),
    /// A `LAMBDA` parameter the call left out (`ISOMITTED`); read as an empty value.
    Omitted,
}

impl Val {
    pub(crate) fn scalar(value: EvaluatedCellValue) -> Self {
        Self::Grid(Arc::new(Grid::scalar(value)))
    }

    pub(crate) fn error(e: FormulaError) -> Self {
        Self::scalar(EvaluatedCellValue::Error(e))
    }

    pub(crate) fn grid(grid: Grid) -> Self {
        Self::Grid(Arc::new(grid))
    }

    /// The grid; a `LAMBDA` where values are wanted is `#VALUE!`.
    pub(crate) fn into_grid(self) -> Result<Arc<Grid>, FormulaError> {
        match self {
            Self::Grid(grid) => Ok(grid),
            Self::Lambda(_) | Self::Ref(_) | Self::Omitted => Err(FormulaError::Value),
        }
    }
}

/// `LET` variables and `LAMBDA` parameters in scope, innermost first.
#[derive(Debug, Clone, Default)]
pub(crate) struct Env(Option<Arc<Binding>>);

#[derive(Debug)]
struct Binding {
    name: String,
    val: Val,
    next: Env,
}

impl Env {
    pub(crate) fn bind(&self, name: &str, val: Val) -> Self {
        Self(Some(Arc::new(Binding {
            name: name.to_string(),
            val,
            next: self.clone(),
        })))
    }

    pub(crate) fn get(&self, name: &str) -> Option<&Val> {
        let mut cur = &self.0;
        while let Some(binding) = cur {
            if binding.name == name {
                return Some(&binding.val);
            }
            cur = &binding.next.0;
        }
        None
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_none()
    }
}

/// The name of the virtual sheet that holds temporary array number `index` (a control character
/// starts it, so no sheet can have the name).
pub(crate) fn virtual_sheet_name(index: usize) -> String {
    format!("\u{1}{index}")
}

pub(crate) fn virtual_sheet_index(name: &str) -> Option<usize> {
    name.strip_prefix('\u{1}')?.parse().ok()
}

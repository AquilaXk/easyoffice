//! Multi-function vectors (`HSTACK`/`VSTACK`), custom labels, and header layout helpers for
//! `GROUPBY` and `PIVOTBY`.

use crate::array::{Closure, FuncElem, FuncGrid, Val};
use crate::{EvaluatedCellValue as V, FormulaError};
use std::sync::Arc;

/// Maximum value columns or stacked aggregation functions per call.
pub(crate) const MAX_VALUE_COLUMNS: usize = 64;

/// A validated aggregation function slot with an optional user-supplied header label.
#[derive(Clone)]
pub(crate) struct AggFunc {
    pub(crate) closure: Arc<Closure>,
    pub(crate) label: Option<V>,
    pub(crate) wants_total: bool,
}

impl AggFunc {
    fn from_closure(closure: Arc<Closure>, label: Option<V>) -> Self {
        let wants_total = closure.params.len() >= 2 && closure.required <= 2;
        let label = label.map(|v| match v {
            V::Empty => V::Text(String::new()),
            other => other,
        });
        Self {
            closure,
            label,
            wants_total,
        }
    }

    pub(crate) fn header_value(&self) -> V {
        if let Some(label) = &self.label {
            return label.clone();
        }
        match &self.closure.name {
            Some(name) => V::Text(name.clone()),
            None => V::Text("LAMBDA".to_string()),
        }
    }
}

/// How multiple aggregation functions lay out in the output table.
#[derive(Copy, Clone, Eq, PartialEq)]
pub(crate) enum FuncOrientation {
    /// Single `1 x 1` function without user-supplied custom label (`100%` legacy header shape).
    Single,
    /// Horizontal function vector (`1 x F`, `2 x F`) or custom-labeled single (`1 x 2`, `2 x 1`).
    Horizontal,
    /// Vertical function vector (`F x 1`, `F x 2`, `F >= 2`).
    Vertical,
}

/// Validated aggregation function plan for `GROUPBY` / `PIVOTBY`.
pub(crate) struct FuncPlan {
    pub(crate) orientation: FuncOrientation,
    pub(crate) funcs: Vec<AggFunc>,
}

impl FuncPlan {
    pub(crate) fn fn_cols(&self) -> usize {
        if self.orientation == FuncOrientation::Vertical {
            1
        } else {
            self.funcs.len()
        }
    }

    pub(crate) fn sort_func_col(&self, idx: usize, m: usize) -> (usize, usize) {
        if self.orientation == FuncOrientation::Vertical {
            (0, idx)
        } else {
            (idx / m, idx % m)
        }
    }
}

/// Axis and option inputs for [`OutputShape::compute`].
pub(crate) struct OutputSpec {
    pub(crate) kr: usize,
    pub(crate) kc: usize,
    pub(crate) m: usize,
    pub(crate) has_col_fields: bool,
    pub(crate) show_headers: bool,
    pub(crate) row_count: usize,
    pub(crate) col_count: usize,
}

/// Header and grid geometry derived from the axes and [`FuncPlan`].
pub(crate) struct OutputShape {
    pub(crate) top: usize,
    pub(crate) out_rows: usize,
    pub(crate) out_cols: usize,
    pub(crate) prefix_cols: usize,
    pub(crate) show_field_row: bool,
    pub(crate) show_fn_row: bool,
}

impl OutputShape {
    pub(crate) fn compute(plan: &FuncPlan, spec: OutputSpec) -> Result<Self, FormulaError> {
        let OutputSpec {
            kr,
            kc,
            m,
            has_col_fields,
            show_headers,
            row_count,
            col_count,
        } = spec;
        let f_len = plan.funcs.len();
        let (top, out_rows, val_cols, prefix_cols, show_field_row, show_fn_row) =
            match plan.orientation {
                FuncOrientation::Single => {
                    let show_field_row = show_headers || (has_col_fields && m > 1);
                    let top = if has_col_fields {
                        kc + usize::from(show_field_row)
                    } else {
                        usize::from(show_headers)
                    };
                    let out_rows = top.checked_add(row_count).ok_or(FormulaError::Spill)?;
                    let val_cols = col_count.checked_mul(m).ok_or(FormulaError::Spill)?;
                    (top, out_rows, val_cols, kr, show_field_row, false)
                }
                FuncOrientation::Horizontal => {
                    let (top, show_field_row) = if m == 1 {
                        (kc + 1, false)
                    } else {
                        (kc + 2, true)
                    };
                    let out_rows = top.checked_add(row_count).ok_or(FormulaError::Spill)?;
                    let block = m.checked_mul(f_len).ok_or(FormulaError::Spill)?;
                    let val_cols = col_count.checked_mul(block).ok_or(FormulaError::Spill)?;
                    (top, out_rows, val_cols, kr, show_field_row, true)
                }
                FuncOrientation::Vertical => {
                    let show_field_row = show_headers || (has_col_fields && m > 1);
                    let top = if has_col_fields {
                        kc + usize::from(show_field_row)
                    } else {
                        usize::from(show_headers)
                    };
                    let body_rows = row_count.checked_mul(f_len).ok_or(FormulaError::Spill)?;
                    let out_rows = top.checked_add(body_rows).ok_or(FormulaError::Spill)?;
                    let val_cols = col_count.checked_mul(m).ok_or(FormulaError::Spill)?;
                    (top, out_rows, val_cols, kr + 1, show_field_row, false)
                }
            };
        let out_cols = prefix_cols
            .checked_add(val_cols)
            .ok_or(FormulaError::Spill)?;
        Ok(Self {
            top,
            out_rows,
            out_cols,
            prefix_cols,
            show_field_row,
            show_fn_row,
        })
    }
}

fn as_lambda(elem: &FuncElem) -> Result<&Arc<Closure>, FormulaError> {
    match elem {
        FuncElem::Lambda(closure) => Ok(closure),
        FuncElem::Scalar(_) => Err(FormulaError::Value),
    }
}

fn as_label(elem: &FuncElem) -> Result<V, FormulaError> {
    match elem {
        FuncElem::Scalar(v) => Ok(v.clone()),
        FuncElem::Lambda(_) => Err(FormulaError::Value),
    }
}

fn make_plan(orientation: FuncOrientation, funcs: Vec<AggFunc>) -> Result<FuncPlan, FormulaError> {
    if funcs.is_empty() || funcs.len() > MAX_VALUE_COLUMNS {
        return Err(FormulaError::Value);
    }
    Ok(FuncPlan { orientation, funcs })
}

fn parse_func_grid(fg: &FuncGrid) -> Result<FuncPlan, FormulaError> {
    match (fg.rows, fg.cols) {
        (0, _) | (_, 0) => Err(FormulaError::Value),
        (1, 1) => {
            let c = Arc::clone(as_lambda(fg.get(0, 0))?);
            make_plan(
                FuncOrientation::Single,
                vec![AggFunc::from_closure(c, None)],
            )
        }
        (1, 2) => match (fg.get(0, 0), fg.get(0, 1)) {
            (FuncElem::Lambda(c0), FuncElem::Lambda(c1)) => make_plan(
                FuncOrientation::Horizontal,
                vec![
                    AggFunc::from_closure(Arc::clone(c0), None),
                    AggFunc::from_closure(Arc::clone(c1), None),
                ],
            ),
            (FuncElem::Lambda(c0), FuncElem::Scalar(lbl)) => make_plan(
                FuncOrientation::Horizontal,
                vec![AggFunc::from_closure(Arc::clone(c0), Some(lbl.clone()))],
            ),
            _ => Err(FormulaError::Value),
        },
        (2, 1) => match (fg.get(0, 0), fg.get(1, 0)) {
            (FuncElem::Lambda(c0), FuncElem::Lambda(c1)) => make_plan(
                FuncOrientation::Vertical,
                vec![
                    AggFunc::from_closure(Arc::clone(c0), None),
                    AggFunc::from_closure(Arc::clone(c1), None),
                ],
            ),
            (FuncElem::Lambda(c0), FuncElem::Scalar(lbl)) => make_plan(
                FuncOrientation::Horizontal,
                vec![AggFunc::from_closure(Arc::clone(c0), Some(lbl.clone()))],
            ),
            _ => Err(FormulaError::Value),
        },
        (1, cols) => {
            if cols > MAX_VALUE_COLUMNS {
                return Err(FormulaError::Value);
            }
            let mut funcs = Vec::with_capacity(cols);
            for c in 0..cols {
                let closure = Arc::clone(as_lambda(fg.get(0, c))?);
                funcs.push(AggFunc::from_closure(closure, None));
            }
            make_plan(FuncOrientation::Horizontal, funcs)
        }
        (rows, 1) => {
            if rows > MAX_VALUE_COLUMNS {
                return Err(FormulaError::Value);
            }
            let mut funcs = Vec::with_capacity(rows);
            for r in 0..rows {
                let closure = Arc::clone(as_lambda(fg.get(r, 0))?);
                funcs.push(AggFunc::from_closure(closure, None));
            }
            make_plan(FuncOrientation::Vertical, funcs)
        }
        (2, cols) => {
            let is_horiz_2d = (0..cols).all(|c| matches!(fg.get(0, c), FuncElem::Lambda(_)))
                && (0..cols).all(|c| matches!(fg.get(1, c), FuncElem::Scalar(_)));
            if is_horiz_2d {
                if cols > MAX_VALUE_COLUMNS {
                    return Err(FormulaError::Value);
                }
                let mut funcs = Vec::with_capacity(cols);
                for c in 0..cols {
                    let closure = Arc::clone(as_lambda(fg.get(0, c))?);
                    let lbl = as_label(fg.get(1, c))?;
                    funcs.push(AggFunc::from_closure(closure, Some(lbl)));
                }
                return make_plan(FuncOrientation::Horizontal, funcs);
            }
            if cols == 2 {
                return parse_vertical_2col(fg);
            }
            Err(FormulaError::Value)
        }
        (rows, 2) if rows >= 3 => parse_vertical_2col(fg),
        _ => Err(FormulaError::Value),
    }
}

fn parse_vertical_2col(fg: &FuncGrid) -> Result<FuncPlan, FormulaError> {
    if fg.rows > MAX_VALUE_COLUMNS {
        return Err(FormulaError::Value);
    }
    let mut funcs = Vec::with_capacity(fg.rows);
    for r in 0..fg.rows {
        let closure = Arc::clone(as_lambda(fg.get(r, 0))?);
        let lbl = as_label(fg.get(r, 1))?;
        funcs.push(AggFunc::from_closure(closure, Some(lbl)));
    }
    make_plan(FuncOrientation::Vertical, funcs)
}

/// Validates and decodes the `function` argument (`Val::Lambda` or `Val::FuncGrid`).
pub(crate) fn parse_func_plan(val: Val) -> Result<FuncPlan, FormulaError> {
    match val {
        Val::Lambda(closure) => make_plan(
            FuncOrientation::Single,
            vec![AggFunc::from_closure(closure, None)],
        ),
        Val::FuncGrid(fg) => parse_func_grid(&fg),
        Val::Grid(g) => match g.data.as_slice() {
            [V::Error(e)] => Err(*e),
            _ => Err(FormulaError::Value),
        },
        Val::Ref(_) | Val::Omitted => Err(FormulaError::Value),
    }
}

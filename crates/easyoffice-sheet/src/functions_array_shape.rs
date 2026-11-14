//! `TAKE`, `DROP`, `TOCOL`, `TOROW`, `VSTACK`, `HSTACK`, `CHOOSEROWS`, `CHOOSECOLS`, `EXPAND`, `WRAPROWS`,
//! `WRAPCOLS` and `ISOMITTED` (PD-217): arrays cut, joined and reshaped.
//!
//! 참조 구현: IronCalc `base/src/functions/lookup_and_reference/drop_take.rs` (`fn_take` 13-69,
//! `fn_drop` 72-125), `tocol_torow.rs` (`fn_tocol`, `fn_torow`, `flatten_array_impl`),
//! `hstack_vstack.rs` (`fn_hstack`, `fn_vstack`), `choosecols_chooserows.rs`, `expand.rs`
//! (`fn_expand`), `wrapcols_wraprows.rs` (`fn_wrapcols`, `fn_wraprows`, `flatten_vector`,
//! `get_pad_with`), `information/isomitted.rs` (기준 커밋 01b39c1ad26252f7b6c810440e8433a932daee07).
//! MIT OR Apache-2.0, Copyright 2023 EqualTo GmbH, 2023-present IronCalc contributors, 2026-present
//! IronCalc GmbH. The bodies are ported onto this engine's grid; a size that is left out
//! (`TAKE(a,,2)`) means the whole side, every result is checked against the array budget, and an
//! omitted `LAMBDA` parameter is a variable bound to nothing instead of an empty argument.

use crate::array::{Env, Grid, Val};
use crate::array_eval::ValResult;
use crate::{EvalContext, EvaluatedCellValue as V, Expr, FormulaError, SheetEngine};
use std::sync::Arc;

/// Rows or columns `n` selects out of `len`: the first `n`, or the last `-n`.
fn take_range(len: usize, n: i64) -> std::ops::Range<usize> {
    let count = usize::try_from(n.unsigned_abs())
        .unwrap_or(usize::MAX)
        .min(len);
    if n >= 0 {
        0..count
    } else {
        len - count..len
    }
}

/// What `DROP` leaves of `len` after dropping `n` from the front (`n` >= 0) or `-n` from the back.
fn drop_range(len: usize, n: i64) -> std::ops::Range<usize> {
    let count = usize::try_from(n.unsigned_abs())
        .unwrap_or(usize::MAX)
        .min(len);
    if n >= 0 {
        count..len
    } else {
        0..len - count
    }
}

fn slice(
    ctx: &mut EvalContext<'_>,
    grid: &Grid,
    rows: std::ops::Range<usize>,
    cols: std::ops::Range<usize>,
) -> Result<Grid, FormulaError> {
    Grid::build_checked(ctx, rows.len(), cols.len(), |r, c| {
        grid.get(rows.start + r, cols.start + c).clone()
    })
}

/// The padding value of `EXPAND`/`WRAP*`: `#N/A` unless one is given.
fn pad_of(value: Option<V>) -> V {
    match value {
        None => V::Error(FormulaError::Na),
        Some(V::Empty) => V::Number(0.0),
        Some(v) => v,
    }
}

impl SheetEngine {
    fn size_arg(
        &self,
        args: &[Expr],
        i: usize,
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Option<i64>, FormulaError> {
        match args.get(i) {
            None | Some(Expr::Missing) => Ok(None),
            Some(_) => Ok(Some(
                self.arg_number(args, i, 0.0, env, ctx, depth)?.trunc() as i64,
            )),
        }
    }

    fn shape_grid(
        &self,
        arg: &Expr,
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Arc<Grid>, FormulaError> {
        let grid = self.eval_val(arg, env, ctx, depth + 1)?.into_grid()?;
        match grid.data.as_slice() {
            [V::Error(e)] => Err(*e),
            _ => Ok(grid),
        }
    }

    /// `TAKE(array, rows, [columns])` and `DROP(array, rows, [columns])`.
    pub(crate) fn fn_take_drop(
        &self,
        take: bool,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.len() < 2 || args.len() > 3 {
            return Err(FormulaError::Value);
        }
        let grid = self.shape_grid(&args[0], env, ctx, depth)?;
        let rows = self.size_arg(args, 1, env, ctx, depth)?;
        let cols = self.size_arg(args, 2, env, ctx, depth)?;
        if take && (rows == Some(0) || cols == Some(0)) {
            return Err(FormulaError::Calc);
        }
        let pick = |len: usize, n: Option<i64>| match (take, n) {
            (_, None) => 0..len,
            (true, Some(n)) => take_range(len, n),
            (false, Some(n)) => drop_range(len, n),
        };
        let (row_range, col_range) = (pick(grid.rows, rows), pick(grid.cols, cols));
        if row_range.is_empty() || col_range.is_empty() {
            return Err(FormulaError::Calc);
        }
        Ok(Val::grid(slice(ctx, &grid, row_range, col_range)?))
    }

    /// `TOCOL(array, [ignore], [scan_by_column])` and `TOROW(...)`.
    pub(crate) fn fn_tocol_torow(
        &self,
        column: bool,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.is_empty() || args.len() > 3 {
            return Err(FormulaError::Value);
        }
        let grid = self.shape_grid(&args[0], env, ctx, depth)?;
        let ignore = self.arg_number(args, 1, 0.0, env, ctx, depth)?.trunc() as i64;
        if !(0..=3).contains(&ignore) {
            return Err(FormulaError::Value);
        }
        let by_col = self.arg_bool(args, 2, env, ctx, depth)?;
        let order: Vec<(usize, usize)> = if by_col {
            (0..grid.cols)
                .flat_map(|c| (0..grid.rows).map(move |r| (r, c)))
                .collect()
        } else {
            (0..grid.rows)
                .flat_map(|r| (0..grid.cols).map(move |c| (r, c)))
                .collect()
        };
        let flat: Vec<V> = order
            .into_iter()
            .map(|(r, c)| grid.get(r, c).clone())
            .filter(|v| {
                let blank = matches!(v, V::Empty);
                let error = matches!(v, V::Error(_));
                match ignore {
                    1 => !blank,
                    2 => !error,
                    3 => !blank && !error,
                    _ => true,
                }
            })
            .collect();
        if flat.is_empty() {
            return Err(FormulaError::Calc);
        }
        let n = flat.len();
        Ok(Val::grid(if column {
            Grid::from_rows_checked(ctx, n, 1, flat)?
        } else {
            Grid::from_rows_checked(ctx, 1, n, flat)?
        }))
    }

    /// `VSTACK(array1, ...)` and `HSTACK(array1, ...)`: the short side is padded with `#N/A`.
    pub(crate) fn fn_stack(
        &self,
        vertical: bool,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.is_empty() {
            return Err(FormulaError::Value);
        }
        let mut grids = Vec::with_capacity(args.len());
        for arg in args {
            grids.push(self.shape_grid(arg, env, ctx, depth)?);
        }
        let na = V::Error(FormulaError::Na);
        if vertical {
            let cols = grids.iter().map(|g| g.cols).max().unwrap_or(0);
            let rows: usize = grids.iter().map(|g| g.rows).sum();
            ctx.charge_array_cells(rows, cols)?;
            let mut data = Vec::with_capacity(rows * cols);
            for g in &grids {
                for r in 0..g.rows {
                    for c in 0..cols {
                        data.push(if c < g.cols {
                            g.get(r, c).clone()
                        } else {
                            na.clone()
                        });
                    }
                }
            }
            Ok(Val::grid(Grid::from_rows(rows, cols, data)))
        } else {
            let rows = grids.iter().map(|g| g.rows).max().unwrap_or(0);
            let cols: usize = grids.iter().map(|g| g.cols).sum();
            ctx.charge_array_cells(rows, cols)?;
            let mut data = Vec::with_capacity(rows * cols);
            for r in 0..rows {
                for g in &grids {
                    for c in 0..g.cols {
                        data.push(if r < g.rows {
                            g.get(r, c).clone()
                        } else {
                            na.clone()
                        });
                    }
                }
            }
            Ok(Val::grid(Grid::from_rows(rows, cols, data)))
        }
    }

    /// `CHOOSEROWS(array, row1, ...)` and `CHOOSECOLS(array, col1, ...)`: negative numbers count from the end.
    pub(crate) fn fn_choose(
        &self,
        rows: bool,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.len() < 2 {
            return Err(FormulaError::Value);
        }
        let grid = self.shape_grid(&args[0], env, ctx, depth)?;
        let len = (if rows { grid.rows } else { grid.cols }) as i64;
        let mut picks = Vec::with_capacity(args.len() - 1);
        for i in 1..args.len() {
            let n = self.arg_number(args, i, 0.0, env, ctx, depth)?.trunc() as i64;
            if n == 0 || n.unsigned_abs() > len.unsigned_abs() {
                return Err(FormulaError::Value);
            }
            picks.push(if n > 0 { n - 1 } else { len + n } as usize);
        }
        Ok(Val::grid(if rows {
            grid.take_rows_checked(ctx, &picks)?
        } else {
            grid.take_cols_checked(ctx, &picks)?
        }))
    }

    /// `EXPAND(array, rows, [columns], [pad_with])`.
    pub(crate) fn fn_expand(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.len() < 2 || args.len() > 4 {
            return Err(FormulaError::Value);
        }
        let grid = self.shape_grid(&args[0], env, ctx, depth)?;
        let rows = self
            .size_arg(args, 1, env, ctx, depth)?
            .map_or(grid.rows, |n| n.max(0) as usize);
        let cols = self
            .size_arg(args, 2, env, ctx, depth)?
            .map_or(grid.cols, |n| n.max(0) as usize);
        if rows < grid.rows || cols < grid.cols {
            return Err(FormulaError::Value);
        }
        let pad = match args.get(3) {
            None | Some(Expr::Missing) => pad_of(None),
            Some(_) => pad_of(Some(self.arg_single(args, 3, None, env, ctx, depth)?)),
        };
        let out = Grid::build_checked(ctx, rows, cols, |r, c| {
            if r < grid.rows && c < grid.cols {
                grid.get(r, c).clone()
            } else {
                pad.clone()
            }
        })?;
        Ok(Val::grid(out))
    }

    /// `WRAPROWS(vector, wrap_count, [pad_with])` and `WRAPCOLS(...)`.
    pub(crate) fn fn_wrap(
        &self,
        by_rows: bool,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.len() < 2 || args.len() > 3 {
            return Err(FormulaError::Value);
        }
        let grid = self.shape_grid(&args[0], env, ctx, depth)?;
        if grid.rows != 1 && grid.cols != 1 {
            return Err(FormulaError::Value);
        }
        let count = self.arg_number(args, 1, 0.0, env, ctx, depth)?.trunc();
        if count < 1.0 {
            return Err(FormulaError::Value);
        }
        let count = count as usize;
        let pad = match args.get(2) {
            None | Some(Expr::Missing) => pad_of(None),
            Some(_) => pad_of(Some(self.arg_single(args, 2, None, env, ctx, depth)?)),
        };
        let n = grid.data.len();
        let lines = n.div_ceil(count);
        let at = |i: usize| grid.data.get(i).cloned().unwrap_or_else(|| pad.clone());
        let out = if by_rows {
            Grid::build_checked(ctx, lines, count, |r, c| at(r * count + c))?
        } else {
            Grid::build_checked(ctx, count, lines, |r, c| at(c * count + r))?
        };
        Ok(Val::grid(out))
    }

    /// `ISOMITTED(parameter)`: whether the call left a `LAMBDA` parameter out.
    pub(crate) fn fn_isomitted(&self, args: &[Expr], env: &Env) -> ValResult {
        match args {
            [Expr::Var(name)] => Ok(Val::scalar(V::Boolean(matches!(
                env.get(name),
                Some(Val::Omitted)
            )))),
            _ => Err(FormulaError::Value),
        }
    }
}

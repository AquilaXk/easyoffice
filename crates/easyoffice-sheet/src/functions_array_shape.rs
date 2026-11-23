//! `TAKE`, `DROP`, `TOCOL`, `TOROW`, `VSTACK`, `HSTACK`, `CHOOSEROWS`, `CHOOSECOLS`, `EXPAND`, `WRAPROWS`,
//! `WRAPCOLS`, `ISOMITTED` (PD-217) and `TRIMRANGE`: arrays cut, joined and reshaped.
//!
//! 참조 구현: IronCalc `base/src/functions/lookup_and_reference/drop_take.rs` (`fn_take` 13-69,
//! `fn_drop` 72-125), `tocol_torow.rs` (`fn_tocol`, `fn_torow`, `flatten_array_impl`),
//! `hstack_vstack.rs` (`fn_hstack`, `fn_vstack`), `choosecols_chooserows.rs`, `expand.rs`
//! (`fn_expand`), `wrapcols_wraprows.rs` (`fn_wrapcols`, `fn_wraprows`, `flatten_vector`,
//! `get_pad_with`), `information/isomitted.rs` (기준 커밋 01b39c1ad26252f7b6c810440e8433a932daee07).
//! MIT OR Apache-2.0, Copyright 2023 EqualTo GmbH, 2023-present IronCalc contributors, 2026-present
//! IronCalc GmbH. `TRIMRANGE` is IronCalc `lookup_and_reference/trimrange.rs` (`is_blank` 8-10,
//! `row_is_empty` 13-15, `col_is_empty` 18-20, `fn_trimrange` 34-134; 기준 커밋
//! f93662006b8fb1c401dc870bbda26c5d60ac6037) with the same helpers and branches. The bodies are
//! ported onto this engine's grid; a size that is left out
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
    Grid::build_with_links_checked(ctx, rows.len(), cols.len(), |r, c| {
        let (rr, cc) = (rows.start + r, cols.start + c);
        (grid.get(rr, cc).clone(), grid.link_id(rr, cc))
    })
}

/// Returns true if an array cell counts as a blank/empty cell for TRIMRANGE purposes.
fn is_blank(node: &V) -> bool {
    matches!(node, V::Empty)
}

/// Returns true if every cell in row `r` is blank.
fn row_is_empty(data: &Grid, r: usize) -> bool {
    (0..data.cols).all(|c| is_blank(data.get(r, c)))
}

/// Returns true if every row has a blank value at column index `col`.
fn col_is_empty(data: &Grid, col: usize) -> bool {
    (0..data.rows).all(|r| is_blank(data.get(r, col)))
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
        let keep = |v: &V| {
            let blank = matches!(v, V::Empty);
            let error = matches!(v, V::Error(_));
            match ignore {
                1 => !blank,
                2 => !error,
                3 => !blank && !error,
                _ => true,
            }
        };
        // Emit directly into flat vector in a single pass without allocating intermediate coordinate vectors.
        let mut flat = Vec::new();
        let mut links: Option<Vec<u32>> = None;
        let mut push_cell = |r: usize, c: usize| {
            let v = grid.get(r, c).clone();
            if keep(&v) {
                let lid = grid.link_id(r, c);
                if lid != 0 {
                    let cur = flat.len();
                    let lvec = links.get_or_insert_with(|| vec![0; cur]);
                    lvec.resize(cur, 0);
                    lvec.push(lid);
                } else if let Some(lvec) = links.as_mut() {
                    lvec.push(0);
                }
                flat.push(v);
            }
        };
        if by_col {
            for c in 0..grid.cols {
                for r in 0..grid.rows {
                    push_cell(r, c);
                }
            }
        } else {
            for r in 0..grid.rows {
                for c in 0..grid.cols {
                    push_cell(r, c);
                }
            }
        }
        if flat.is_empty() {
            return Err(FormulaError::Calc);
        }
        let n = flat.len();
        Ok(Val::grid(if column {
            Grid::from_rows_with_links_checked(ctx, n, 1, flat, links)?
        } else {
            Grid::from_rows_with_links_checked(ctx, 1, n, flat, links)?
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
            let total = rows * cols;
            ctx.charge_array_cells(rows, cols)?;
            let mut data = Vec::with_capacity(total);
            let mut links: Option<Vec<u32>> = None;
            let mut idx = 0usize;
            for g in &grids {
                for r in 0..g.rows {
                    for c in 0..cols {
                        let (cell, lid) = if c < g.cols {
                            (g.get(r, c).clone(), g.link_id(r, c))
                        } else {
                            (na.clone(), 0)
                        };
                        let hb = cell.heap_bytes();
                        if hb > 0 {
                            ctx.charge_array_extra_bytes(hb)?;
                        }
                        data.push(cell);
                        if lid != 0 {
                            links.get_or_insert_with(|| vec![0; total])[idx] = lid;
                        }
                        if (idx & 4095) == 4095 {
                            ctx.charge_steps(1)?;
                            ctx.poll_deadline();
                            if ctx.expired() {
                                return Err(FormulaError::Value);
                            }
                        }
                        idx += 1;
                    }
                }
            }
            Ok(Val::grid(Grid::from_rows_with_links(
                rows, cols, data, links,
            )))
        } else {
            let rows = grids.iter().map(|g| g.rows).max().unwrap_or(0);
            let cols: usize = grids.iter().map(|g| g.cols).sum();
            let total = rows * cols;
            ctx.charge_array_cells(rows, cols)?;
            let mut data = Vec::with_capacity(total);
            let mut links: Option<Vec<u32>> = None;
            let mut idx = 0usize;
            for r in 0..rows {
                for g in &grids {
                    for c in 0..g.cols {
                        let (cell, lid) = if r < g.rows {
                            (g.get(r, c).clone(), g.link_id(r, c))
                        } else {
                            (na.clone(), 0)
                        };
                        let hb = cell.heap_bytes();
                        if hb > 0 {
                            ctx.charge_array_extra_bytes(hb)?;
                        }
                        data.push(cell);
                        if lid != 0 {
                            links.get_or_insert_with(|| vec![0; total])[idx] = lid;
                        }
                        if (idx & 4095) == 4095 {
                            ctx.charge_steps(1)?;
                            ctx.poll_deadline();
                            if ctx.expired() {
                                return Err(FormulaError::Value);
                            }
                        }
                        idx += 1;
                    }
                }
            }
            Ok(Val::grid(Grid::from_rows_with_links(
                rows, cols, data, links,
            )))
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
        let out = Grid::build_with_links_checked(ctx, rows, cols, |r, c| {
            if r < grid.rows && c < grid.cols {
                (grid.get(r, c).clone(), grid.link_id(r, c))
            } else {
                (pad.clone(), 0)
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
        let at = |i: usize| {
            (
                grid.data.get(i).cloned().unwrap_or_else(|| pad.clone()),
                grid.link_id_flat(i),
            )
        };
        let out = if by_rows {
            Grid::build_with_links_checked(ctx, lines, count, |r, c| at(r * count + c))?
        } else {
            Grid::build_with_links_checked(ctx, count, lines, |r, c| at(c * count + r))?
        };
        Ok(Val::grid(out))
    }

    /// `=TRIMRANGE(range, [trim_rows], [trim_cols])`
    ///
    /// Trims blank rows and/or columns from the outer edges of a range or array.
    ///
    /// trim_rows / trim_cols:
    ///   * 0 - no trimming
    ///   * 1 - trim leading (top / left) blanks only
    ///   * 2 - trim trailing (bottom / right) blanks only
    ///   * 3 - trim both leading and trailing blanks (default)
    ///
    /// Returns `#REF!` if every cell in the range is blank.
    pub(crate) fn fn_trimrange(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.is_empty() || args.len() > 3 {
            return Err(FormulaError::Value);
        }

        let data = self.shape_grid(&args[0], env, ctx, depth)?;

        let trim_rows = if args.len() >= 2 {
            self.arg_number(args, 1, 3.0, env, ctx, depth)? as i32
        } else {
            3
        };

        let trim_cols = if args.len() == 3 {
            self.arg_number(args, 2, 3.0, env, ctx, depth)? as i32
        } else {
            3
        };

        if !matches!(trim_rows, 0..=3) {
            // trim_rows must be 0, 1, 2, or 3
            return Err(FormulaError::Value);
        }
        if !matches!(trim_cols, 0..=3) {
            // trim_cols must be 0, 1, 2, or 3
            return Err(FormulaError::Value);
        }

        let num_rows = data.rows;
        let num_cols = data.cols;

        // Determine row slice bounds
        let row_start = if trim_rows == 1 || trim_rows == 3 {
            (0..num_rows)
                .find(|&r| !row_is_empty(&data, r))
                .unwrap_or(num_rows)
        } else {
            0
        };
        let row_end = if trim_rows == 2 || trim_rows == 3 {
            (0..num_rows)
                .rev()
                .find(|&r| !row_is_empty(&data, r))
                .map(|r| r + 1)
                .unwrap_or(0)
        } else {
            num_rows
        };

        // Determine column slice bounds
        let col_start = if trim_cols == 1 || trim_cols == 3 {
            (0..num_cols)
                .find(|&c| !col_is_empty(&data, c))
                .unwrap_or(num_cols)
        } else {
            0
        };
        let col_end = if trim_cols == 2 || trim_cols == 3 {
            (0..num_cols)
                .rev()
                .find(|&c| !col_is_empty(&data, c))
                .map(|c| c + 1)
                .unwrap_or(0)
        } else {
            num_cols
        };

        if row_start >= row_end || col_start >= col_end {
            // TRIMRANGE result is empty
            return Err(FormulaError::Ref);
        }

        Ok(Val::grid(slice(
            ctx,
            &data,
            row_start..row_end,
            col_start..col_end,
        )?))
    }

    /// `INDEX(array, row, [column])` on an array that is not a reference: a zero or left out number
    /// gives the whole column or row, a negative one is `#VALUE!`, one past the array is `#REF!`.
    pub(crate) fn fn_index_grid(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args
            .get(3)
            .is_some_and(|a| !matches!(a.peeled(), Expr::Missing))
        {
            let area = self.arg_number(args, 3, 1.0, env, ctx, depth)?.trunc();
            if area < 1.0 {
                return Err(FormulaError::Value);
            }
            if area > 1.0 {
                return Err(FormulaError::Ref);
            }
        }
        let grid = self.shape_grid(&args[0], env, ctx, depth)?;
        let mut number = |i: usize| -> Result<Option<i64>, FormulaError> {
            match args.get(i) {
                None | Some(Expr::Missing) => Ok(None),
                Some(_) => Ok(Some(
                    self.arg_number(args, i, 0.0, env, ctx, depth)?.trunc() as i64,
                )),
            }
        };
        let (mut row, mut col) = (number(1)?, number(2)?);
        if row.is_some_and(|n| n < 0) || col.is_some_and(|n| n < 0) {
            return Err(FormulaError::Value);
        }
        // One number on a single row or column picks the element.
        if args.len() == 2 && grid.rows == 1 {
            (row, col) = (None, row);
        }
        let line = |n: Option<i64>, len: usize| match n.unwrap_or(0) {
            0 => Ok(0..len),
            n if n as usize <= len => Ok(n as usize - 1..n as usize),
            _ => Err(FormulaError::Ref),
        };
        let (rows, cols) = (line(row, grid.rows)?, line(col, grid.cols)?);
        Ok(Val::grid(slice(ctx, &grid, rows, cols)?))
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

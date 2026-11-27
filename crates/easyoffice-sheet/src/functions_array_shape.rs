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

use crate::array::{Closure, Env, FuncElem, FuncGrid, Grid, Val};
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

enum StackItem {
    Grid(Arc<Grid>),
    Lambda(Arc<Closure>),
    FuncGrid(Arc<FuncGrid>),
}

impl StackItem {
    fn rows(&self) -> usize {
        match self {
            Self::Grid(g) => g.rows,
            Self::Lambda(_) => 1,
            Self::FuncGrid(fg) => fg.rows,
        }
    }

    fn cols(&self) -> usize {
        match self {
            Self::Grid(g) => g.cols,
            Self::Lambda(_) => 1,
            Self::FuncGrid(fg) => fg.cols,
        }
    }

    fn elem(&self, r: usize, c: usize) -> FuncElem {
        match self {
            Self::Grid(g) => FuncElem::Scalar(g.get(r, c).clone()),
            Self::Lambda(l) => FuncElem::Lambda(Arc::clone(l)),
            Self::FuncGrid(fg) => fg.get(r, c).clone(),
        }
    }
}

fn stack_func_grid(vertical: bool, items: &[StackItem], ctx: &mut EvalContext<'_>) -> ValResult {
    let na = V::Error(FormulaError::Na);
    let (rows, cols) = if vertical {
        let cols = items.iter().map(StackItem::cols).max().unwrap_or(0);
        let rows = items
            .iter()
            .try_fold(0usize, |acc, it| acc.checked_add(it.rows()))
            .ok_or(FormulaError::Spill)?;
        (rows, cols)
    } else {
        let rows = items.iter().map(StackItem::rows).max().unwrap_or(0);
        let cols = items
            .iter()
            .try_fold(0usize, |acc, it| acc.checked_add(it.cols()))
            .ok_or(FormulaError::Spill)?;
        (rows, cols)
    };
    ctx.charge_array_cells(rows, cols)?;
    let total = rows * cols;
    let mut data = Vec::with_capacity(total);
    let mut push_elem = |elem: FuncElem, idx: usize| -> Result<(), FormulaError> {
        if let FuncElem::Scalar(cell) = &elem {
            let hb = cell.heap_bytes();
            if hb > 0 {
                ctx.charge_array_extra_bytes(hb)?;
            }
        }
        data.push(elem);
        if (idx & 4095) == 4095 {
            ctx.charge_steps(1)?;
            ctx.poll_deadline();
            if ctx.expired() {
                return Err(FormulaError::Value);
            }
        }
        Ok(())
    };
    let mut idx = 0usize;
    if vertical {
        for item in items {
            let (ir, ic) = (item.rows(), item.cols());
            for r in 0..ir {
                for c in 0..cols {
                    let elem = if c < ic {
                        item.elem(r, c)
                    } else {
                        FuncElem::Scalar(na.clone())
                    };
                    push_elem(elem, idx)?;
                    idx += 1;
                }
            }
        }
    } else {
        for r in 0..rows {
            for item in items {
                let (ir, ic) = (item.rows(), item.cols());
                for c in 0..ic {
                    let elem = if r < ir {
                        item.elem(r, c)
                    } else {
                        FuncElem::Scalar(na.clone())
                    };
                    push_elem(elem, idx)?;
                    idx += 1;
                }
            }
        }
    }
    Ok(Val::func_grid(FuncGrid { rows, cols, data }))
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

    fn shape_grids(
        &self,
        arg: &Expr,
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Vec<Arc<Grid>>, FormulaError> {
        let bound = Self::bound_ref(arg, env);
        let target = bound.as_ref().unwrap_or_else(|| arg.peeled());
        if let Expr::SheetSpan(sp) = target {
            let sheets = self.resolve_sheet_span(sp, ctx)?;
            let mut grids = Vec::with_capacity(sheets.len());
            for sh in sheets {
                let grid = self
                    .read_ref_grid(&Expr::sheet_range(sh, sp.range), ctx, depth + 1)?
                    .ok_or(FormulaError::Ref)?;
                grids.push(Arc::new(grid));
            }
            return Ok(grids);
        }
        Ok(vec![self.shape_grid(arg, env, ctx, depth)?])
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
        let grids = self.shape_grids(&args[0], env, ctx, depth)?;
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
        let mut push_cell = |grid: &Grid, r: usize, c: usize| {
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
        for grid in &grids {
            if by_col {
                for c in 0..grid.cols {
                    for r in 0..grid.rows {
                        push_cell(grid, r, c);
                    }
                }
            } else {
                for r in 0..grid.rows {
                    for c in 0..grid.cols {
                        push_cell(grid, r, c);
                    }
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
        let mut items = Vec::with_capacity(args.len());
        let mut has_func = false;
        for arg in args {
            let bound = Self::bound_ref(arg, env);
            let target = bound.as_ref().unwrap_or_else(|| arg.peeled());
            if let Expr::SheetSpan(sp) = target {
                let sheets = self.resolve_sheet_span(sp, ctx)?;
                for sh in sheets {
                    let grid = self
                        .read_ref_grid(&Expr::sheet_range(sh, sp.range), ctx, depth + 1)?
                        .ok_or(FormulaError::Ref)?;
                    items.push(StackItem::Grid(Arc::new(grid)));
                }
                continue;
            }
            match self.eval_val(arg, env, ctx, depth + 1)? {
                Val::Grid(g) => match g.data.as_slice() {
                    [V::Error(e)] => return Err(*e),
                    _ => items.push(StackItem::Grid(g)),
                },
                Val::Lambda(closure) => {
                    has_func = true;
                    items.push(StackItem::Lambda(closure));
                }
                Val::FuncGrid(fg) => {
                    has_func = true;
                    items.push(StackItem::FuncGrid(fg));
                }
                Val::Ref(_) | Val::Omitted => return Err(FormulaError::Value),
            }
        }
        if has_func {
            return stack_func_grid(vertical, &items, ctx);
        }
        let grids: Vec<Arc<Grid>> = items
            .into_iter()
            .map(|it| match it {
                StackItem::Grid(g) => g,
                StackItem::Lambda(_) | StackItem::FuncGrid(_) => unreachable!(),
            })
            .collect();
        let na = V::Error(FormulaError::Na);
        if vertical {
            let cols = grids.iter().map(|g| g.cols).max().unwrap_or(0);
            let rows = grids
                .iter()
                .try_fold(0usize, |acc, g| acc.checked_add(g.rows))
                .ok_or(FormulaError::Spill)?;
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
            let cols = grids
                .iter()
                .try_fold(0usize, |acc, g| acc.checked_add(g.cols))
                .ok_or(FormulaError::Spill)?;
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

    fn trim_mode_arg(
        &self,
        args: &[Expr],
        i: usize,
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<u8, FormulaError> {
        match args.get(i) {
            None | Some(Expr::Missing) => Ok(3),
            Some(_) => {
                let n = self.arg_number(args, i, 0.0, env, ctx, depth)?.trunc();
                if !(0.0..=3.0).contains(&n) {
                    Err(FormulaError::Value)
                } else {
                    Ok(n as u8)
                }
            }
        }
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

        let substituted_arg0 = (!env.is_empty() && crate::array_eval::uses_vars(&args[0]))
            .then(|| crate::array_eval::subst_env_bindings(&args[0], env));
        let arg0 = substituted_arg0.as_ref().unwrap_or(&args[0]);
        if let Some((sheet_opt, range, transposed)) =
            self.extract_range_and_sheet_ctx(arg0, ctx, depth + 1)?
        {
            if !Self::is_virtual_sheet(sheet_opt.as_deref()) {
                let trim_rows = self.trim_mode_arg(args, 1, env, ctx, depth)?;
                let trim_cols = self.trim_mode_arg(args, 2, env, ctx, depth)?;
                let trimmed = self.trim_sheet_range(
                    sheet_opt.as_deref(),
                    range,
                    transposed,
                    (trim_rows, trim_cols),
                    ctx,
                    depth + 1,
                )?;
                let min_r = trimmed.start.row().min(trimmed.end.row());
                let max_r = trimmed.start.row().max(trimmed.end.row());
                let min_c = trimmed.start.col().min(trimmed.end.col());
                let max_c = trimmed.start.col().max(trimmed.end.col());
                let phys_rows = (max_r - min_r + 1) as usize;
                let phys_cols = (max_c - min_c + 1) as usize;
                let (rows, cols) = if transposed {
                    (phys_cols, phys_rows)
                } else {
                    (phys_rows, phys_cols)
                };
                let spec = (sheet_opt, min_r, min_c, rows, cols, transposed);
                return Ok(Val::grid(self.read_streaming_spec_grid(
                    spec,
                    ctx,
                    depth + 1,
                )?));
            }
        }

        let data = self.shape_grid(&args[0], env, ctx, depth)?;
        let trim_rows = self.trim_mode_arg(args, 1, env, ctx, depth)?;
        let trim_cols = self.trim_mode_arg(args, 2, env, ctx, depth)?;

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
        if (1..args.len().min(3)).any(|i| crate::array_class::arrayish(&args[i])) {
            return self.index_items(&grid, args, env, ctx, depth);
        }
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

    /// `INDEX` whose `row_num` or `column_num` is an array: one result per item, the two arrays
    /// paired up the way an operator pairs them (a one-row or one-column array repeats). An item
    /// that is negative is `#VALUE!`, one past the range `#REF!`; an item `0` (a whole line) of a
    /// side with more than one element would put an array in a cell, so it is `#VALUE!`.
    fn index_items(
        &self,
        grid: &Grid,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        let mut numbers = Vec::with_capacity(2);
        for i in 1..3 {
            numbers.push(match args.get(i) {
                None | Some(Expr::Missing) => None,
                Some(arg) => Some(self.shape_grid(arg, env, ctx, depth)?),
            });
        }
        let present: Vec<&Grid> = numbers.iter().flatten().map(|g| g.as_ref()).collect();
        let (rows, cols) = crate::array_eval::broadcast_dims(&present);
        let single_row = args.len() == 2 && grid.rows == 1;
        let item =
            |g: &Option<Arc<Grid>>, r: usize, c: usize| -> Result<Option<i64>, FormulaError> {
                let Some(g) = g else { return Ok(None) };
                let n = match crate::array_eval::broadcast_at(g, r, c) {
                    V::Error(e) => return Err(e),
                    V::Empty => 0.0,
                    other => other.as_number().ok_or(FormulaError::Value)?.trunc(),
                };
                if n < 0.0 {
                    return Err(FormulaError::Value);
                }
                Ok(Some(n as i64))
            };
        let pick = |n: Option<i64>, len: usize| match n.unwrap_or(0) {
            0 if len == 1 => Ok(0),
            0 => Err(FormulaError::Value),
            n if n as usize <= len => Ok(n as usize - 1),
            _ => Err(FormulaError::Ref),
        };
        let at = |r: usize, c: usize| -> Result<(usize, usize), FormulaError> {
            let (mut row, mut col) = (item(&numbers[0], r, c)?, item(&numbers[1], r, c)?);
            if single_row {
                (row, col) = (None, row);
            }
            Ok((pick(row, grid.rows)?, pick(col, grid.cols)?))
        };
        Ok(Val::grid(Grid::build_with_links_checked(
            ctx,
            rows,
            cols,
            |r, c| match at(r, c) {
                Ok((rr, cc)) => (grid.get(rr, cc).clone(), grid.link_id(rr, cc)),
                Err(e) => (V::Error(e), 0),
            },
        )?))
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

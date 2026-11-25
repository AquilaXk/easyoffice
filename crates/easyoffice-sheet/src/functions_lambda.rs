//! `LET`, `LAMBDA` and the functions that call one: `MAP`, `REDUCE`, `SCAN`, `BYROW`, `BYCOL`,
//! `MAKEARRAY` (PD-217).
//!
//! 참조 구현: IronCalc `base/src/functions/logical/let.rs` (`fn_let` 111-160), `lambda.rs`
//! (`call_lambda` 13-62, `fn_lambda` 135-180, 인수 개수 검사 182-), `map_reduce.rs` (`fn_map` 34-100,
//! `fn_reduce` 102-147), `scan.rs` (`fn_scan` 33-104), `bycol_byrow.rs` (`fn_bycol`, `fn_byrow`,
//! `fn_makearray`) (기준 커밋 6dc9e8bae4b8d066276f399de7b3e1cf991c220b). MIT OR Apache-2.0,
//! Copyright 2023 EqualTo GmbH, 2023-present IronCalc contributors, 2026-present IronCalc GmbH. The
//! reference patches variable ids into a copy of the syntax tree for every call; this port binds
//! the variables in a scope that closures capture (`Env`), so the tree is never copied or patched.
//! A function that returns an array where one value is wanted is `#CALC!` (the reference says
//! `#VALUE!`; the spreadsheet program says `#CALC!`).

use crate::array::{Env, Grid, Val, MAX_LAMBDA_DEPTH};
use crate::array_eval::ValResult;
use crate::{EvalContext, EvaluatedCellValue as V, Expr, FormulaError, SheetEngine};
use std::sync::Arc;

/// The one value a lambda returned; an array is `#CALC!`.
fn element(val: Val) -> V {
    element_with_link(val).0
}

fn element_with_link(val: Val) -> (V, u32) {
    match val {
        Val::Grid(g) if g.is_scalar() => (g.top_left(), g.link_id_flat(0)),
        Val::Grid(_) | Val::Lambda(_) | Val::FuncGrid(_) | Val::Ref(_) | Val::Omitted => {
            (V::Error(FormulaError::Calc), 0)
        }
    }
}

impl SheetEngine {
    /// Adapts an eta-reduced built-in closure (`SUM`, `MAX`, ...) when passed through `LET`/`IF`
    /// to a combinator or direct call expecting a different arity (`args.len()`).
    pub(crate) fn apply_eta_builtin(
        &self,
        upper: &str,
        args: Vec<Val>,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if ctx.lambda_depth >= MAX_LAMBDA_DEPTH {
            return Err(FormulaError::Num);
        }
        let mut params = Vec::with_capacity(args.len());
        let mut env = Env::default();
        for (i, val) in args.into_iter().enumerate() {
            let pname = format!("ETA{}", i + 1);
            env = env.bind(&pname, val);
            params.push(Expr::Var(pname));
        }
        let body = Expr::func_call(upper.to_string(), params);
        ctx.lambda_depth += 1;
        let result = self.eval_val(&body, &env, ctx, depth + 1);
        ctx.lambda_depth -= 1;
        result
    }

    /// `LET(name1, value1, [name2, value2, ...], body)`.
    pub(crate) fn fn_let(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.len() < 3 || args.len().is_multiple_of(2) {
            return Err(FormulaError::Value);
        }
        let mut scope = env.clone();
        for pair in args[..args.len() - 1].chunks(2) {
            let Expr::Var(name) = &pair[0] else {
                return Err(FormulaError::Value);
            };
            let value = self.eval_bound(&pair[1], &scope, ctx, depth + 1)?;
            scope = scope.bind(name, value);
        }
        self.eval_val(&args[args.len() - 1], &scope, ctx, depth + 1)
    }

    /// `LAMBDA(parameter1, [parameter2, ...], body)`: a value that is called later.
    pub(crate) fn fn_lambda(&self, args: &[Expr], env: &Env) -> ValResult {
        let Some((body, params)) = args.split_last() else {
            return Err(FormulaError::Value);
        };
        let mut names = Vec::with_capacity(params.len());
        let mut required = params.len();
        for (i, param) in params.iter().enumerate() {
            let Expr::Var(name) = param else {
                return Err(FormulaError::Value);
            };
            // A parameter written `[x]` is optional, and so are the ones after it.
            match name.strip_prefix('?') {
                Some(plain) => {
                    required = required.min(i);
                    names.push(plain.to_string());
                }
                None => names.push(name.clone()),
            }
        }
        Ok(self.make_closure(names, required, body, env))
    }

    fn lambda_arg_arity(
        &self,
        arg: &Expr,
        arity: usize,
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Val, FormulaError> {
        let val = match arg.peeled() {
            Expr::Name(n) => self
                .eval_name_for_eta(n, Some(arity), ctx, depth)
                .unwrap_or_else(|| Val::error(FormulaError::Name)),
            _ => self.eval_val(arg, env, ctx, depth + 1)?,
        };
        match val {
            f @ Val::Lambda(_) => Ok(f),
            Val::Grid(g) => match g.data.as_slice() {
                [V::Error(e)] => Err(*e),
                _ => Err(FormulaError::Value),
            },
            Val::FuncGrid(_) | Val::Ref(_) | Val::Omitted => Err(FormulaError::Value),
        }
    }

    /// `MAP(array1, [array2, ...], lambda)`.
    pub(crate) fn fn_map(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.len() < 2 {
            return Err(FormulaError::Value);
        }
        let (arrays, f) = args.split_at(args.len() - 1);
        let f = self.lambda_arg_arity(&f[0], arrays.len(), env, ctx, depth)?;
        let mut grids: Vec<Arc<Grid>> = Vec::with_capacity(arrays.len());
        for arg in arrays {
            grids.push(self.eval_val(arg, env, ctx, depth + 1)?.into_grid()?);
        }
        let (rows, cols) = (grids[0].rows, grids[0].cols);
        if grids.iter().any(|g| g.rows != rows || g.cols != cols) {
            return Err(FormulaError::Value);
        }
        let total = rows * cols;
        ctx.charge_array_cells(rows, cols)?;
        let mut data = Vec::with_capacity(total);
        let mut links: Option<Vec<u32>> = None;
        for i in 0..total {
            let call_args = grids
                .iter()
                .map(|g| Val::grid(Grid::scalar_with_link(g.data[i].clone(), g.link_id_flat(i))))
                .collect();
            let (cell, lid) = element_with_link(self.apply_val(&f, call_args, ctx, depth + 1)?);
            let hb = cell.heap_bytes();
            if hb > 0 {
                ctx.charge_array_extra_bytes(hb)?;
            }
            data.push(cell);
            if lid != 0 {
                links.get_or_insert_with(|| vec![0; total])[i] = lid;
            }
            if (i & 4095) == 4095 {
                ctx.charge_steps(1)?;
                ctx.poll_deadline();
                if ctx.expired() {
                    return Err(FormulaError::Value);
                }
            }
        }
        Ok(Val::grid(Grid::from_rows_with_links(
            rows, cols, data, links,
        )))
    }

    /// The initial value (when given), the array and the lambda of `REDUCE` and `SCAN`.
    fn fold_args(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<(Option<V>, Arc<Grid>, Val), FormulaError> {
        let (array_at, lambda_at) = match args.len() {
            2 => (0, 1),
            3 => (1, 2),
            _ => return Err(FormulaError::Value),
        };
        let f = self.lambda_arg_arity(&args[lambda_at], 2, env, ctx, depth)?;
        let grid = self
            .eval_val(&args[array_at], env, ctx, depth + 1)?
            .into_grid()?;
        let initial = if args.len() == 3 {
            match element(self.eval_val(&args[0], env, ctx, depth + 1)?) {
                V::Error(e) => return Err(e),
                v => Some(v),
            }
        } else {
            None
        };
        Ok((initial, grid, f))
    }

    /// `REDUCE([initial_value], array, lambda)`.
    pub(crate) fn fn_reduce(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        let (initial, grid, f) = self.fold_args(args, env, ctx, depth)?;
        let (mut acc, start) = match initial {
            Some(v) => (v, 0),
            None => (grid.data[0].clone(), 1),
        };
        for (i, item) in grid.data[start..].iter().enumerate() {
            let call_args = vec![Val::scalar(acc), Val::scalar(item.clone())];
            acc = element(self.apply_val(&f, call_args, ctx, depth + 1)?);
            if let V::Error(e) = acc {
                return Err(e);
            }
            if (i & 4095) == 4095 {
                ctx.charge_steps(1)?;
                ctx.poll_deadline();
                if ctx.expired() {
                    return Err(FormulaError::Value);
                }
            }
        }
        Ok(Val::scalar(acc))
    }

    /// `SCAN([initial_value], array, lambda)`: the running result at every element.
    pub(crate) fn fn_scan(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        let (initial, grid, f) = self.fold_args(args, env, ctx, depth)?;
        ctx.charge_array_cells(grid.rows, grid.cols)?;
        let mut acc = initial.clone().unwrap_or_else(|| grid.data[0].clone());
        let start = usize::from(initial.is_none());
        let mut data = Vec::with_capacity(grid.data.len());
        if initial.is_none() {
            let hb = acc.heap_bytes();
            if hb > 0 {
                ctx.charge_array_extra_bytes(hb)?;
            }
            data.push(acc.clone());
        }
        for (i, item) in grid.data[start..].iter().enumerate() {
            let call_args = vec![Val::scalar(acc), Val::scalar(item.clone())];
            acc = element(self.apply_val(&f, call_args, ctx, depth + 1)?);
            if let V::Error(e) = acc {
                return Err(e);
            }
            let hb = acc.heap_bytes();
            if hb > 0 {
                ctx.charge_array_extra_bytes(hb)?;
            }
            data.push(acc.clone());
            if (i & 4095) == 4095 {
                ctx.charge_steps(1)?;
                ctx.poll_deadline();
                if ctx.expired() {
                    return Err(FormulaError::Value);
                }
            }
        }
        Ok(Val::grid(Grid::from_rows(grid.rows, grid.cols, data)))
    }

    /// `BYROW(array, lambda)` and `BYCOL(array, lambda)`: the lambda gets one row or column at a time.
    pub(crate) fn fn_byrow_bycol(
        &self,
        by_row: bool,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.len() != 2 {
            return Err(FormulaError::Value);
        }
        let grid = self.eval_val(&args[0], env, ctx, depth + 1)?.into_grid()?;
        let f = self.lambda_arg_arity(&args[1], 1, env, ctx, depth)?;
        let lines = if by_row { grid.rows } else { grid.cols };
        if by_row {
            ctx.charge_array_cells(lines, 1)?;
        } else {
            ctx.charge_array_cells(1, lines)?;
        }
        let mut data = Vec::with_capacity(lines);
        let mut links: Option<Vec<u32>> = None;
        for i in 0..lines {
            let line = if by_row {
                grid.take_rows_checked(ctx, &[i])?
            } else {
                grid.take_cols_checked(ctx, &[i])?
            };
            let (cell, lid) =
                element_with_link(self.apply_val(&f, vec![Val::grid(line)], ctx, depth + 1)?);
            let hb = cell.heap_bytes();
            if hb > 0 {
                ctx.charge_array_extra_bytes(hb)?;
            }
            data.push(cell);
            if lid != 0 {
                links.get_or_insert_with(|| vec![0; lines])[i] = lid;
            }
            if (i & 4095) == 4095 {
                ctx.charge_steps(1)?;
                ctx.poll_deadline();
                if ctx.expired() {
                    return Err(FormulaError::Value);
                }
            }
        }
        Ok(Val::grid(if by_row {
            Grid::from_rows_with_links(lines, 1, data, links)
        } else {
            Grid::from_rows_with_links(1, lines, data, links)
        }))
    }

    /// `MAKEARRAY(rows, columns, lambda)`.
    pub(crate) fn fn_makearray(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.len() != 3 {
            return Err(FormulaError::Value);
        }
        let rows = self.arg_number(args, 0, 1.0, env, ctx, depth)?.floor();
        let cols = self.arg_number(args, 1, 1.0, env, ctx, depth)?.floor();
        if rows < 1.0 || cols < 1.0 {
            return Err(FormulaError::Value);
        }
        if rows > 1e9 || cols > 1e9 {
            return Err(FormulaError::Spill);
        }
        let (rows, cols) = (rows as usize, cols as usize);
        let total = rows * cols;
        ctx.charge_array_cells(rows, cols)?;
        let f = self.lambda_arg_arity(&args[2], 2, env, ctx, depth)?;
        let mut data = Vec::with_capacity(total);
        let mut links: Option<Vec<u32>> = None;
        let mut idx = 0usize;
        for r in 1..=rows {
            for c in 1..=cols {
                let call_args = vec![
                    Val::scalar(V::Number(r as f64)),
                    Val::scalar(V::Number(c as f64)),
                ];
                let (cell, lid) =
                    element_with_link(self.apply_val(&f, call_args, ctx, depth + 1)?);
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
        Ok(Val::grid(Grid::from_rows_with_links(
            rows, cols, data, links,
        )))
    }
}

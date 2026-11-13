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

use crate::array::{check_cells, Env, Grid, Val};
use crate::array_eval::ValResult;
use crate::{EvalContext, EvaluatedCellValue as V, Expr, FormulaError, SheetEngine};
use std::sync::Arc;

/// The one value a lambda returned; an array is `#CALC!`.
fn element(val: Val) -> V {
    match val {
        Val::Grid(g) if g.is_scalar() => g.top_left(),
        Val::Grid(_) => V::Error(FormulaError::Calc),
        Val::Lambda(_) | Val::Ref(_) | Val::Omitted => V::Error(FormulaError::Calc),
    }
}

impl SheetEngine {
    /// `LET(name1, value1, [name2, value2, ...], body)`.
    pub(crate) fn fn_let(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.len() < 3 || args.len() % 2 == 0 {
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

    fn lambda_arg(
        &self,
        arg: &Expr,
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Val, FormulaError> {
        match self.eval_val(arg, env, ctx, depth + 1)? {
            f @ Val::Lambda(_) => Ok(f),
            Val::Grid(g) => match g.data.as_slice() {
                [V::Error(e)] => Err(*e),
                _ => Err(FormulaError::Value),
            },
            Val::Ref(_) | Val::Omitted => Err(FormulaError::Value),
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
        let f = self.lambda_arg(&f[0], env, ctx, depth)?;
        let mut grids: Vec<Arc<Grid>> = Vec::with_capacity(arrays.len());
        for arg in arrays {
            grids.push(self.eval_val(arg, env, ctx, depth + 1)?.into_grid()?);
        }
        let (rows, cols) = (grids[0].rows, grids[0].cols);
        if grids.iter().any(|g| g.rows != rows || g.cols != cols) {
            return Err(FormulaError::Value);
        }
        let mut data = Vec::with_capacity(rows * cols);
        for i in 0..rows * cols {
            let call_args = grids
                .iter()
                .map(|g| Val::scalar(g.data[i].clone()))
                .collect();
            data.push(element(self.apply_val(&f, call_args, ctx, depth + 1)?));
        }
        Ok(Val::grid(Grid::from_rows(rows, cols, data)))
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
        let f = self.lambda_arg(&args[lambda_at], env, ctx, depth)?;
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
        for item in &grid.data[start..] {
            let call_args = vec![Val::scalar(acc), Val::scalar(item.clone())];
            acc = element(self.apply_val(&f, call_args, ctx, depth + 1)?);
            if let V::Error(e) = acc {
                return Err(e);
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
        let mut acc = initial.clone().unwrap_or_else(|| grid.data[0].clone());
        let start = usize::from(initial.is_none());
        let mut data = Vec::with_capacity(grid.data.len());
        if initial.is_none() {
            data.push(acc.clone());
        }
        for item in &grid.data[start..] {
            let call_args = vec![Val::scalar(acc), Val::scalar(item.clone())];
            acc = element(self.apply_val(&f, call_args, ctx, depth + 1)?);
            if let V::Error(e) = acc {
                return Err(e);
            }
            data.push(acc.clone());
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
        let f = self.lambda_arg(&args[1], env, ctx, depth)?;
        let lines = if by_row { grid.rows } else { grid.cols };
        let mut data = Vec::with_capacity(lines);
        for i in 0..lines {
            let line = if by_row {
                grid.take_rows(&[i])
            } else {
                grid.take_cols(&[i])
            };
            data.push(element(self.apply_val(
                &f,
                vec![Val::grid(line)],
                ctx,
                depth + 1,
            )?));
        }
        Ok(Val::grid(if by_row {
            Grid::from_rows(lines, 1, data)
        } else {
            Grid::from_rows(1, lines, data)
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
        check_cells(rows, cols)?;
        let f = self.lambda_arg(&args[2], env, ctx, depth)?;
        let mut data = Vec::with_capacity(rows * cols);
        for r in 1..=rows {
            for c in 1..=cols {
                let call_args = vec![
                    Val::scalar(V::Number(r as f64)),
                    Val::scalar(V::Number(c as f64)),
                ];
                data.push(element(self.apply_val(&f, call_args, ctx, depth + 1)?));
            }
        }
        Ok(Val::grid(Grid::from_rows(rows, cols, data)))
    }
}

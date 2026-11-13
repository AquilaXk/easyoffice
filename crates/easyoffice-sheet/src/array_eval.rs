//! The array evaluator (PD-217): calculates an expression to a grid of values, element by element
//! where an operator or a one-value function meets a range, and calls the array functions, `LET`
//! and `LAMBDA`. An expression the scalar engine already evaluates is handed to it, with every array
//! argument replaced by a literal (one value) or a temporary range (more), so every function that
//! reads ranges keeps working on array results.

use crate::array::{check_cells, virtual_sheet_name, Closure, Env, Grid, Val, MAX_LAMBDA_DEPTH};
use crate::array_class::{arrayish, is_lifted_fn, needs_array};
use crate::{
    BinaryOp, CellRange, EvalContext, EvaluatedCellValue as V, Expr, FormulaError,
    PackedCellCoordinate, SheetEngine,
};
use std::sync::Arc;

pub(crate) type ValResult = Result<Val, FormulaError>;

/// The one value of an argument that must be a single value: `Err` for an error, an array or a
/// `LAMBDA`.
pub(crate) fn single(val: Val) -> Result<V, FormulaError> {
    let grid = val.into_grid()?;
    match grid.data.as_slice() {
        [V::Error(e)] => Err(*e),
        [one] => Ok(one.clone()),
        _ => Err(FormulaError::Value),
    }
}

/// The element of `grid` at `(r, c)` when it stands for a `rows` by `cols` result: a one-row or
/// one-column grid repeats along that side and a missing element is `#N/A`.
fn broadcast_at(grid: &Grid, r: usize, c: usize) -> V {
    let r = if grid.rows == 1 { 0 } else { r };
    let c = if grid.cols == 1 { 0 } else { c };
    if r < grid.rows && c < grid.cols {
        grid.get(r, c).clone()
    } else {
        V::Error(FormulaError::Na)
    }
}

fn broadcast_dims(grids: &[&Grid]) -> (usize, usize) {
    let side = |f: fn(&Grid) -> usize| grids.iter().map(|g| f(g)).fold(1usize, usize::max);
    (side(|g| g.rows), side(|g| g.cols))
}

impl SheetEngine {
    /// Calculates `expr` to a value; a formula's own errors are values, `Err` only ends the whole
    /// calculation (a budget, a depth).
    pub(crate) fn eval_val(
        &self,
        expr: &Expr,
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        Ok(self
            .eval_val_inner(expr, env, ctx, depth)
            .unwrap_or_else(Val::error))
    }

    fn eval_val_inner(
        &self,
        expr: &Expr,
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        ctx.steps = ctx.steps.saturating_add(1);
        if ctx.steps > Self::MAX_EVAL_STEPS || depth > Self::MAX_RECURSION_DEPTH {
            return Err(FormulaError::Value);
        }
        if ctx.steps & crate::deadline::POLL_STEPS_MASK == 0 {
            ctx.poll_deadline();
        }
        match expr {
            Expr::Shared(shared) => self.eval_val(&shared.expr, env, ctx, depth),
            Expr::Missing => Ok(Val::scalar(V::Empty)),
            Expr::Array(lit) => Ok(Val::grid(Grid::from_rows(
                lit.rows,
                lit.cols,
                lit.data.clone(),
            ))),
            Expr::Var(name) => match env.get(name) {
                Some(Val::Ref(reference)) => {
                    let reference = Arc::clone(reference);
                    self.eval_val(&reference, &Env::default(), ctx, depth + 1)
                }
                Some(val) => Ok(val.clone()),
                None => Ok(Val::error(FormulaError::Name)),
            },
            Expr::SpillRef { sheet, coord } => {
                self.spill_ref_grid(sheet.as_deref(), *coord, ctx, depth)
            }
            Expr::UnaryPlus(inner) | Expr::UnaryMinus(inner) | Expr::PostfixPercent(inner) => {
                let grid = self.eval_val(inner, env, ctx, depth + 1)?.into_grid()?;
                Ok(Val::grid(unary_grid(expr, &grid)))
            }
            Expr::Binary { op, lhs, rhs } => {
                let l = self.eval_val(lhs, env, ctx, depth + 1)?.into_grid()?;
                let r = self.eval_val(rhs, env, ctx, depth + 1)?.into_grid()?;
                binary_grid(*op, &l, &r).map(Val::grid)
            }
            Expr::Call { callee, args } => {
                let f = self.eval_val(callee, env, ctx, depth + 1)?;
                let mut vals = Vec::with_capacity(args.len());
                for arg in args {
                    vals.push(self.eval_bound(arg, env, ctx, depth + 1)?);
                }
                self.apply_val(&f, vals, ctx, depth + 1)
            }
            Expr::FuncCall { name, args } => self.eval_val_call(name, args, env, ctx, depth),
            Expr::Cell(_) | Expr::SheetCell { .. } | Expr::Range(_) | Expr::SheetRange { .. } => {
                match self.read_ref_grid(expr, ctx, depth)? {
                    Some(grid) => Ok(Val::grid(grid)),
                    None => Ok(Val::error(FormulaError::Ref)),
                }
            }
            Expr::Number(_)
            | Expr::Text(_)
            | Expr::Boolean(_)
            | Expr::ErrorLiteral(_)
            | Expr::Name { .. } => Ok(Val::scalar(self.eval_expr_ctx(expr, ctx, depth))),
        }
    }

    /// The value a `LET` variable or a `LAMBDA` parameter is bound to: a reference stays one.
    pub(crate) fn eval_bound(
        &self,
        expr: &Expr,
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        match expr.peeled() {
            Expr::Cell(_) | Expr::SheetCell { .. } | Expr::Range(_) | Expr::SheetRange { .. } => {
                Ok(Val::Ref(Arc::new(expr.peeled().clone())))
            }
            Expr::Var(name) => match env.get(name) {
                Some(val @ Val::Ref(_)) => Ok(val.clone()),
                _ => self.eval_val(expr, env, ctx, depth),
            },
            _ => self.eval_val(expr, env, ctx, depth),
        }
    }

    /// The reference a bare variable in `expr` holds, for a call the scalar engine runs.
    fn bound_ref(expr: &Expr, env: &Env) -> Option<Expr> {
        match expr.peeled() {
            Expr::Var(name) => match env.get(name) {
                Some(Val::Ref(reference)) => Some((**reference).clone()),
                _ => None,
            },
            _ => None,
        }
    }

    /// The grid a reference stands for, `None` when `expr` is not a reference.
    pub(crate) fn read_ref_grid(
        &self,
        expr: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Option<Grid>, FormulaError> {
        let Some(spec) = (match self.extract_streaming_range_ctx(expr, ctx, depth) {
            Ok(spec) => spec,
            Err(e) => return Ok(Some(Grid::error(e))),
        }) else {
            return Ok(None);
        };
        let (ref sheet_opt, min_r, min_c, rows, cols, transposed) = spec;
        check_cells(rows, cols)?;
        let total = rows * cols;
        // A big range over few cells reads only the populated ones.
        let populated = (!transposed && total > 1024)
            .then(|| self.resolve_sheet_ref(sheet_opt.as_deref(), ctx))
            .flatten()
            .filter(|ts| total > ts.cell_index().sorted_coords.len().saturating_mul(2))
            .map(|ts| {
                ts.cell_index().range_coords(
                    min_r,
                    min_r + rows as u32 - 1,
                    min_c,
                    min_c + cols as u32 - 1,
                )
            });
        let mut data = vec![V::Empty; total];
        if let Some(coords) = populated {
            for coord in coords {
                let (dr, dc) = (
                    (coord.row() - min_r) as usize,
                    (coord.col() - min_c) as usize,
                );
                data[dr * cols + dc] =
                    self.eval_sheet_coord(sheet_opt.as_deref(), coord, ctx, depth);
            }
        } else {
            ctx.steps = ctx.steps.saturating_add(total);
            for dr in 0..rows {
                for dc in 0..cols {
                    let v = match self.eval_streaming_cell_allow_error(&spec, dr, dc, ctx, depth) {
                        Ok(v) => v,
                        Err(e) => V::Error(e),
                    };
                    data[dr * cols + dc] = v;
                }
            }
        }
        Ok(Some(Grid::from_rows(rows, cols, data)))
    }

    /// `A1#`: the cells of the spill range of the formula at `coord` (one value for a formula that
    /// does not spill, `#REF!` for a cell that holds no formula).
    fn spill_ref_grid(
        &self,
        sheet_opt: Option<&str>,
        coord: PackedCellCoordinate,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        let anchor = self.eval_sheet_coord(sheet_opt, coord, ctx, depth);
        let Some((is_array, area)) = self
            .resolve_sheet_ref(sheet_opt, ctx)
            .map(|t| (t.array_formulas.contains(&coord), t.spill_area(coord)))
        else {
            return Ok(Val::error(FormulaError::Ref));
        };
        if !is_array || matches!(anchor, V::Error(FormulaError::Spill)) {
            return Ok(Val::error(FormulaError::Ref));
        }
        let (rows, cols) = area.unwrap_or((1, 1));
        let grid = Grid::build(rows, cols, |r, c| {
            if r == 0 && c == 0 {
                anchor.clone()
            } else {
                match PackedCellCoordinate::new(coord.row() + r as u32, coord.col() + c as u32) {
                    Ok(at) => self.eval_sheet_coord(sheet_opt, at, ctx, depth),
                    Err(_) => V::Error(FormulaError::Ref),
                }
            }
        })?;
        Ok(Val::grid(grid))
    }

    fn eval_val_call(
        &self,
        name: &str,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if let Some(f) = env.get(name) {
            let f = f.clone();
            let mut vals = Vec::with_capacity(args.len());
            for arg in args {
                vals.push(self.eval_bound(arg, env, ctx, depth + 1)?);
            }
            return self.apply_val(&f, vals, ctx, depth + 1);
        }
        match name {
            "LET" => self.fn_let(args, env, ctx, depth),
            "LAMBDA" => self.fn_lambda(args, env),
            "FILTER" => self.fn_filter(args, env, ctx, depth),
            "SORT" => self.fn_sort(args, env, ctx, depth),
            "SORTBY" => self.fn_sortby(args, env, ctx, depth),
            "UNIQUE" => self.fn_unique(args, env, ctx, depth),
            "SEQUENCE" => self.fn_sequence(args, env, ctx, depth),
            "RANDARRAY" => self.fn_randarray(args, env, ctx, depth),
            "MAKEARRAY" => self.fn_makearray(args, env, ctx, depth),
            "MAP" => self.fn_map(args, env, ctx, depth),
            "REDUCE" => self.fn_reduce(args, env, ctx, depth),
            "SCAN" => self.fn_scan(args, env, ctx, depth),
            "BYROW" | "BYCOL" => self.fn_byrow_bycol(name == "BYROW", args, env, ctx, depth),
            "TRANSPOSE" if args.len() == 1 => {
                let grid = self.eval_val(&args[0], env, ctx, depth + 1)?.into_grid()?;
                Ok(Val::grid(grid.transposed()))
            }
            "XLOOKUP" => self.fn_xlookup_array(args, env, ctx, depth),
            "IF" => self.fn_if_array(args, env, ctx, depth),
            "IFERROR" | "IFNA" => self.fn_iferror_array(name == "IFNA", args, env, ctx, depth),
            "OFFSET" | "ROW" | "COLUMN"
                if args
                    .first()
                    .is_some_and(|a| Self::bound_ref(a, env).is_none() && needs_array(a)) =>
            {
                Ok(Val::error(FormulaError::Value))
            }
            "INDIRECT" | "OFFSET" => {
                let mark = ctx.temp.len();
                let call = Expr::FuncCall {
                    name: name.to_string(),
                    args: self.substitute_args(args, env, ctx, depth)?,
                };
                let grid = self.read_ref_grid(&call, ctx, depth);
                ctx.temp.truncate(mark);
                match grid? {
                    Some(grid) => Ok(Val::grid(grid)),
                    None => Ok(Val::error(FormulaError::Ref)),
                }
            }
            _ if crate::functions_matrix::is_array_fn(name) => {
                let mark = ctx.temp.len();
                let call = Expr::FuncCall {
                    name: name.to_string(),
                    args: self.substitute_args(args, env, ctx, depth)?,
                };
                let result = self.matrix_expr(&call, ctx, depth);
                ctx.temp.truncate(mark);
                match result {
                    Ok(Some(m)) => Ok(Val::grid(Grid::from_rows(
                        m.rows,
                        m.cols,
                        (0..m.data.len()).map(|i| m.value_at(i)).collect(),
                    ))),
                    Ok(None) => Ok(Val::error(FormulaError::Name)),
                    Err(e) => Ok(Val::error(e)),
                }
            }
            _ if is_lifted_fn(name) => self.lifted_call(name, args, env, ctx, depth),
            _ => {
                let mark = ctx.temp.len();
                let args = self.substitute_args(args, env, ctx, depth)?;
                let value = self.eval_func_ctx(name, &args, ctx, depth);
                ctx.temp.truncate(mark);
                Ok(Val::scalar(value))
            }
        }
    }

    /// Calls a `LAMBDA` value.
    pub(crate) fn apply_val(
        &self,
        f: &Val,
        args: Vec<Val>,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        let Val::Lambda(closure) = f else {
            return Ok(Val::error(FormulaError::Value));
        };
        if args.len() != closure.params.len() {
            return Ok(Val::error(FormulaError::Value));
        }
        if ctx.lambda_depth >= MAX_LAMBDA_DEPTH {
            return Err(FormulaError::Num);
        }
        let mut env = closure.env.clone();
        for (param, arg) in closure.params.iter().zip(args) {
            env = env.bind(param, arg);
        }
        ctx.lambda_depth += 1;
        let result = self.eval_val(&closure.body, &env, ctx, depth + 1);
        ctx.lambda_depth -= 1;
        result
    }

    pub(crate) fn make_closure(&self, params: Vec<String>, body: &Expr, env: &Env) -> Val {
        Val::Lambda(Arc::new(Closure {
            params,
            body: body.clone(),
            env: env.clone(),
        }))
    }

    /// `args` of a call the scalar engine runs, with the ones that are arrays or use variables
    /// replaced: by a literal (one value) or by a range of a temporary sheet (more).
    fn substitute_args(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Vec<Expr>, FormulaError> {
        let mut out = Vec::with_capacity(args.len());
        for arg in args {
            if let Some(reference) = Self::bound_ref(arg, env) {
                out.push(reference);
                continue;
            }
            let plain_ref = matches!(
                arg.peeled(),
                Expr::Cell(_) | Expr::SheetCell { .. } | Expr::Range(_) | Expr::SheetRange { .. }
            );
            if plain_ref || !(needs_array(arg) || (!env.is_empty() && uses_vars(arg))) {
                out.push(arg.clone());
                continue;
            }
            let grid = self.eval_val(arg, env, ctx, depth + 1)?.into_grid()?;
            out.push(self.as_arg(&grid, ctx)?);
        }
        Ok(out)
    }

    /// `grid` as an argument expression of the scalar engine.
    pub(crate) fn as_arg(
        &self,
        grid: &Arc<Grid>,
        ctx: &mut EvalContext<'_>,
    ) -> Result<Expr, FormulaError> {
        if grid.is_scalar() {
            return Ok(literal(grid.get(0, 0)));
        }
        let rows = u32::try_from(grid.rows).map_err(|_| FormulaError::Spill)?;
        let cols = u32::try_from(grid.cols).map_err(|_| FormulaError::Spill)?;
        let end = PackedCellCoordinate::new(rows - 1, cols - 1).map_err(|_| FormulaError::Spill)?;
        let start = PackedCellCoordinate::new(0, 0).map_err(|_| FormulaError::Spill)?;
        let range = CellRange::new(start, end).map_err(|_| FormulaError::Spill)?;
        ctx.temp.push(Arc::clone(grid));
        Ok(Expr::SheetRange {
            sheet: virtual_sheet_name(ctx.temp.len() - 1),
            range,
        })
    }

    /// A one-value function over arrays: one call per element, the arrays stretched to the same shape.
    fn lifted_call(
        &self,
        name: &str,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        let mut grids: Vec<Option<Arc<Grid>>> = Vec::with_capacity(args.len());
        let mut call_args: Vec<Expr> = args.to_vec();
        for (i, arg) in args.iter().enumerate() {
            let single_cell = |e: &Expr| matches!(e, Expr::Cell(_) | Expr::SheetCell { .. });
            match Self::bound_ref(arg, env) {
                Some(reference) if single_cell(&reference) => {
                    call_args[i] = reference;
                    grids.push(None);
                    continue;
                }
                _ => {}
            }
            if arrayish(arg) || (!env.is_empty() && uses_vars(arg)) {
                grids.push(Some(self.eval_val(arg, env, ctx, depth + 1)?.into_grid()?));
            } else {
                grids.push(None);
            }
        }
        let present: Vec<&Grid> = grids.iter().flatten().map(|g| &**g).collect();
        let (rows, cols) = broadcast_dims(&present);
        check_cells(rows, cols)?;
        let mut data = Vec::with_capacity(rows * cols);
        for r in 0..rows {
            for c in 0..cols {
                for (slot, grid) in call_args.iter_mut().zip(&grids) {
                    if let Some(g) = grid {
                        *slot = literal(&broadcast_at(g, r, c));
                    }
                }
                data.push(self.eval_func_ctx(name, &call_args, ctx, depth));
            }
        }
        Ok(Val::grid(Grid::from_rows(rows, cols, data)))
    }
}

/// Whether `expr` reads a `LET` variable or calls one.
fn uses_vars(expr: &Expr) -> bool {
    let mut found = false;
    crate::names::walk(expr, |node| {
        found |= matches!(node, Expr::Var(_) | Expr::Call { .. });
    });
    found
}

/// A value as an expression.
pub(crate) fn literal(value: &V) -> Expr {
    match value {
        V::Number(n) => Expr::Number(*n),
        V::Text(t) => Expr::Text(t.clone()),
        V::Boolean(b) => Expr::Boolean(*b),
        V::Error(e) => Expr::ErrorLiteral(*e),
        V::Empty => Expr::Text(String::new()),
    }
}

fn unary_grid(op: &Expr, grid: &Grid) -> Grid {
    let data = grid
        .data
        .iter()
        .map(|v| match v {
            V::Error(e) => V::Error(*e),
            other => match other
                .as_number()
                .or(matches!(other, V::Empty).then_some(0.0))
            {
                Some(n) => V::Number(match op {
                    Expr::UnaryMinus(_) => -n,
                    Expr::PostfixPercent(_) => n / 100.0,
                    _ => n,
                }),
                None => V::Error(FormulaError::Value),
            },
        })
        .collect();
    Grid::from_rows(grid.rows, grid.cols, data)
}

fn binary_grid(op: BinaryOp, l: &Grid, r: &Grid) -> Result<Grid, FormulaError> {
    let (rows, cols) = broadcast_dims(&[l, r]);
    check_cells(rows, cols)?;
    let mut data = Vec::with_capacity(rows * cols);
    for i in 0..rows {
        for j in 0..cols {
            let (a, b) = (broadcast_at(l, i, j), broadcast_at(r, i, j));
            data.push(match (a, b) {
                (V::Error(e), _) | (_, V::Error(e)) => V::Error(e),
                (a, b) => SheetEngine::apply_binary_op(op, a, b),
            });
        }
    }
    Ok(Grid::from_rows(rows, cols, data))
}

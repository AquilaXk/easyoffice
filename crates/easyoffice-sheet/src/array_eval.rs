//! The array evaluator (PD-217): calculates an expression to a grid of values, element by element
//! where an operator or a one-value function meets a range, and calls the array functions, `LET`
//! and `LAMBDA`. An expression the scalar engine already evaluates is handed to it, with every array
//! argument replaced by a literal (one value) or a temporary range (more), so every function that
//! reads ranges keeps working on array results.

use crate::array::{virtual_sheet_name, Closure, Env, Grid, Val, MAX_LAMBDA_DEPTH};
use crate::array_class::{arrayish, index_returns_line, is_lifted_fn, needs_array};
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
            Expr::Missing | Expr::Blank => Ok(Val::scalar(V::Empty)),
            Expr::Array(lit) => Ok(Val::grid(Grid::from_rows_checked(
                ctx,
                lit.rows,
                lit.cols,
                lit.data.clone(),
            )?)),
            Expr::Var(name) => match env.get(name) {
                Some(Val::Ref(reference)) => {
                    let reference = Arc::clone(reference);
                    self.eval_val(&reference, &Env::default(), ctx, depth + 1)
                }
                Some(Val::Omitted) => Ok(Val::scalar(V::Empty)),
                Some(val) => Ok(val.clone()),
                None => Ok(Val::error(FormulaError::Name)),
            },
            Expr::SpillRef(sp) => self.spill_ref_grid(sp.sheet.as_deref(), sp.coord, ctx, depth),
            Expr::UnaryPlus(inner) | Expr::UnaryMinus(inner) | Expr::PostfixPercent(inner) => {
                let grid = self.eval_val(inner, env, ctx, depth + 1)?.into_grid()?;
                Ok(Val::grid(unary_grid(ctx, expr, &grid)?))
            }
            Expr::Binary { op, lhs, rhs } => {
                let l = self.eval_val(lhs, env, ctx, depth + 1)?.into_grid()?;
                let r = self.eval_val(rhs, env, ctx, depth + 1)?.into_grid()?;
                binary_grid(ctx, *op, &l, &r).map(Val::grid)
            }
            Expr::Call(c) => {
                let callee = &c.callee;
                let args = &c.args;
                let f = self.eval_val(callee, env, ctx, depth + 1)?;
                let mut vals = Vec::with_capacity(args.len());
                for arg in args {
                    vals.push(self.eval_bound(arg, env, ctx, depth + 1)?);
                }
                self.apply_val(&f, vals, ctx, depth + 1)
            }
            Expr::FuncCall(f) => self.eval_val_call(&f.name, &f.args, env, ctx, depth),
            Expr::Cell(_) | Expr::SheetCell(_) | Expr::Range(_) | Expr::SheetRange(_) => {
                match self.read_ref_grid(expr, ctx, depth)? {
                    Some(grid) => Ok(Val::grid(grid)),
                    None => Ok(Val::error(FormulaError::Ref)),
                }
            }
            Expr::Number(_)
            | Expr::Text(_)
            | Expr::Boolean(_)
            | Expr::ErrorLiteral(_)
            | Expr::Name(_) => Ok(Val::scalar(self.eval_expr_ctx(expr, ctx, depth))),
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
            Expr::Cell(_) | Expr::SheetCell(_) | Expr::Range(_) | Expr::SheetRange(_) => {
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
        ctx.charge_array_cells(rows, cols)?;
        let total = rows * cols;
        let mut data = vec![V::Empty; total];
        // A sparse range over few populated cells (per range, not whole sheet) avoids dense walks.
        let sparse_pop = (!transposed && total > 64)
            .then(|| self.resolve_sheet_ref(sheet_opt.as_deref(), ctx))
            .flatten()
            .and_then(|ts| {
                let max_r = min_r + rows as u32 - 1;
                let max_c = min_c + cols as u32 - 1;
                let pop_count = ts.cell_index().count_in_range(min_r, max_r, min_c, max_c);
                if pop_count == 0 {
                    Some(Vec::new())
                } else if total > pop_count.saturating_mul(2) {
                    Some(ts.cell_index().range_coords(min_r, max_r, min_c, max_c))
                } else {
                    None
                }
            });
        if let Some(coords) = sparse_pop {
            ctx.steps = ctx.steps.saturating_add(1);
            if ctx.steps > Self::MAX_EVAL_STEPS {
                return Err(FormulaError::Value);
            }
            for coord in coords {
                let (dr, dc) = (
                    (coord.row() - min_r) as usize,
                    (coord.col() - min_c) as usize,
                );
                let v = self.eval_sheet_coord(sheet_opt.as_deref(), coord, ctx, depth);
                let hb = v.heap_bytes();
                if hb > 0 {
                    ctx.charge_array_extra_bytes(hb)?;
                }
                data[dr * cols + dc] = v;
            }
        } else {
            ctx.steps = ctx.steps.saturating_add(total);
            for dr in 0..rows {
                for dc in 0..cols {
                    let v = match self.eval_streaming_cell_allow_error(&spec, dr, dc, ctx, depth) {
                        Ok(v) => v,
                        Err(e) => V::Error(e),
                    };
                    let hb = v.heap_bytes();
                    if hb > 0 {
                        ctx.charge_array_extra_bytes(hb)?;
                    }
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
        let target_id = self
            .resolve_sheet_ref(sheet_opt, ctx)
            .map(SheetEngine::resolved_sheet_id);
        if let Some(grid) =
            target_id.and_then(|id| ctx.fresh.get(&crate::PackedWorkbookCoord::new(id, coord).0))
        {
            return Ok(Val::Grid(Arc::clone(grid)));
        }
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
        ctx.charge_array_cells(rows, cols)?;
        let mut data = Vec::with_capacity(rows * cols);
        for r in 0..rows {
            for c in 0..cols {
                let v = if r == 0 && c == 0 {
                    anchor.clone()
                } else {
                    match PackedCellCoordinate::new(coord.row() + r as u32, coord.col() + c as u32)
                    {
                        Ok(at) => self.eval_sheet_coord(sheet_opt, at, ctx, depth),
                        Err(_) => V::Error(FormulaError::Ref),
                    }
                };
                let hb = v.heap_bytes();
                if hb > 0 {
                    ctx.charge_array_extra_bytes(hb)?;
                }
                data.push(v);
            }
        }
        Ok(Val::grid(Grid::from_rows(rows, cols, data)))
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
                Ok(Val::grid(grid.transposed_checked(ctx)?))
            }
            "XLOOKUP" => self.fn_xlookup_array(args, env, ctx, depth),
            "TAKE" | "DROP" => self.fn_take_drop(name == "TAKE", args, env, ctx, depth),
            "TOCOL" | "TOROW" => self.fn_tocol_torow(name == "TOCOL", args, env, ctx, depth),
            "VSTACK" | "HSTACK" => self.fn_stack(name == "VSTACK", args, env, ctx, depth),
            "CHOOSEROWS" | "CHOOSECOLS" => {
                self.fn_choose(name == "CHOOSEROWS", args, env, ctx, depth)
            }
            "EXPAND" => self.fn_expand(args, env, ctx, depth),
            "WRAPROWS" | "WRAPCOLS" => self.fn_wrap(name == "WRAPROWS", args, env, ctx, depth),
            "ISOMITTED" => self.fn_isomitted(args, env),
            "TYPE" if args.len() == 1 && arrayish(&args[0]) => {
                let grid = self.eval_val(&args[0], env, ctx, depth + 1)?.into_grid()?;
                // An array is 64; a single value is its own type.
                Ok(Val::scalar(V::Number(match grid.data.as_slice() {
                    [V::Text(_)] => 2.0,
                    [V::Boolean(_)] => 4.0,
                    [V::Error(_)] => 16.0,
                    [_] => 1.0,
                    _ => 64.0,
                })))
            }
            "TRIMRANGE" => self.fn_trimrange(args, env, ctx, depth),
            "GROUPBY" => self.fn_groupby(args, env, ctx, depth),
            "PIVOTBY" => self.fn_pivotby(args, env, ctx, depth),
            "FORECAST.ETS"
            | "FORECAST.ETS.CONFINT"
            | "FORECAST.ETS.SEASONALITY"
            | "FORECAST.ETS.STAT" => self.fn_forecast_ets(name, args, env, ctx, depth),
            "REGEXTEST" => self.fn_regextest(args, env, ctx, depth),
            "REGEXEXTRACT" => self.fn_regexextract(args, env, ctx, depth),
            "REGEXREPLACE" => self.fn_regexreplace(args, env, ctx, depth),
            "TEXTSPLIT" => self.fn_textsplit(args, env, ctx, depth),
            "ARRAYTOTEXT" => self.fn_arraytotext(args, env, ctx, depth),
            "IF" => self.fn_if_array(args, env, ctx, depth),
            "IFERROR" | "IFNA" => self.fn_iferror_array(name == "IFNA", args, env, ctx, depth),
            "IFS" => self.fn_ifs_array(args, env, ctx, depth),
            "SWITCH" => self.fn_switch_array(args, env, ctx, depth),
            "CHOOSE" => self.fn_choose_branch_array(args, env, ctx, depth),
            "OFFSET" | "ROW" | "COLUMN"
                if args
                    .first()
                    .is_some_and(|a| Self::bound_ref(a, env).is_none() && needs_array(a)) =>
            {
                Ok(Val::error(FormulaError::Value))
            }
            "INDEX" if index_returns_line(args) => {
                let mark = ctx.temp.len();
                let call = Expr::func_call(
                    name.to_string(),
                    self.substitute_args(args, env, ctx, depth)?,
                );
                let grid = self.read_ref_grid(&call, ctx, depth);
                ctx.temp.truncate(mark);
                match grid? {
                    Some(grid) => Ok(Val::grid(grid)),
                    None => self.fn_index_grid(args, env, ctx, depth),
                }
            }
            "INDIRECT" | "OFFSET" => {
                let mark = ctx.temp.len();
                let call = Expr::func_call(
                    name.to_string(),
                    self.substitute_args(args, env, ctx, depth)?,
                );
                let grid = self.read_ref_grid(&call, ctx, depth);
                ctx.temp.truncate(mark);
                match grid? {
                    Some(grid) => Ok(Val::grid(grid)),
                    None => Ok(Val::error(FormulaError::Ref)),
                }
            }
            _ if crate::functions_matrix::is_array_fn(name) => {
                let mark = ctx.temp.len();
                let call = Expr::func_call(
                    name.to_string(),
                    self.substitute_args(args, env, ctx, depth)?,
                );
                let result = self.matrix_expr(&call, ctx, depth);
                ctx.temp.truncate(mark);
                match result {
                    Ok(Some(m)) => Ok(Val::grid(Grid::from_rows_checked(
                        ctx,
                        m.rows,
                        m.cols,
                        (0..m.data.len()).map(|i| m.value_at(i)).collect(),
                    )?)),
                    Ok(None) => Ok(Val::error(FormulaError::Name)),
                    Err(e) => Ok(Val::error(e)),
                }
            }
            _ if is_lifted_fn(name) => self.lifted_call(name, args, env, ctx, depth),
            _ => {
                let mark = ctx.temp.len();
                let args = self.substitute_args(args, env, ctx, depth)?;
                let prev_link = ctx.last_hyperlink_id;
                ctx.last_hyperlink_id = 0;
                let value = self.eval_func_ctx(name, &args, ctx, depth);
                let link_id =
                    if name == "HYPERLINK" || (name == "INDEX" && !matches!(value, V::Error(_))) {
                        ctx.last_hyperlink_id
                    } else {
                        0
                    };
                ctx.last_hyperlink_id = prev_link;
                ctx.temp.truncate(mark);
                Ok(Val::grid(Grid::scalar_with_link(value, link_id)))
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
        if args.len() < closure.required || args.len() > closure.params.len() {
            return Ok(Val::error(FormulaError::Value));
        }
        if ctx.lambda_depth >= MAX_LAMBDA_DEPTH {
            return Err(FormulaError::Num);
        }
        let mut env = closure.env.clone();
        let mut args = args.into_iter();
        for param in &closure.params {
            env = env.bind(param, args.next().unwrap_or(Val::Omitted));
        }
        ctx.lambda_depth += 1;
        let result = self.eval_val(&closure.body, &env, ctx, depth + 1);
        ctx.lambda_depth -= 1;
        result
    }

    pub(crate) fn make_closure(
        &self,
        params: Vec<String>,
        required: usize,
        body: &Expr,
        env: &Env,
    ) -> Val {
        Val::Lambda(Arc::new(Closure {
            params,
            required,
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
                Expr::Cell(_) | Expr::SheetCell(_) | Expr::Range(_) | Expr::SheetRange(_)
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
        Ok(Expr::sheet_range(
            virtual_sheet_name(ctx.temp.len() - 1),
            range,
        ))
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
        if name == "HYPERLINK" {
            return self.lifted_hyperlink(args, env, ctx, depth);
        }
        let mut grids: Vec<Option<Arc<Grid>>> = Vec::with_capacity(args.len());
        let mut call_args: Vec<Expr> = args.to_vec();
        for (i, arg) in args.iter().enumerate() {
            let single_cell = |e: &Expr| matches!(e, Expr::Cell(_) | Expr::SheetCell(_));
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
        ctx.charge_array_cells(rows, cols)?;
        let mut data = Vec::with_capacity(rows * cols);
        let mut idx = 0usize;
        for r in 0..rows {
            for c in 0..cols {
                for (slot, grid) in call_args.iter_mut().zip(&grids) {
                    if let Some(g) = grid {
                        *slot = literal(&broadcast_at(g, r, c));
                    }
                }
                let val = self.eval_func_ctx(name, &call_args, ctx, depth);
                let hb = val.heap_bytes();
                if hb > 0 {
                    ctx.charge_array_extra_bytes(hb)?;
                }
                data.push(val);
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
        Ok(Val::grid(Grid::from_rows(rows, cols, data)))
    }

    /// Element-wise `HYPERLINK(target, [friendly_name])` over scalar or array arguments without intermediate AST clones.
    fn lifted_hyperlink(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.is_empty() || args.len() > 2 {
            return Ok(Val::error(FormulaError::Value));
        }
        let target_grid = self.eval_val(&args[0], env, ctx, depth + 1)?.into_grid()?;
        let name_grid = match args.get(1) {
            Some(arg) if !matches!(arg, Expr::Missing) => {
                Some(self.eval_val(arg, env, ctx, depth + 1)?.into_grid()?)
            }
            _ => None,
        };
        let (rows, cols) = match &name_grid {
            Some(ng) => broadcast_dims(&[&target_grid, ng]),
            None => (target_grid.rows.max(1), target_grid.cols.max(1)),
        };
        let to_res = |v: V| match v {
            V::Error(e) => Err(e),
            other => Ok(other),
        };
        ctx.charge_array_cells(rows, cols)?;
        let total = rows * cols;
        let mut data = Vec::with_capacity(total);
        let mut links: Option<Vec<u32>> = None;
        let prev = ctx.last_hyperlink_id;
        for r in 0..rows {
            for c in 0..cols {
                let idx = data.len();
                ctx.last_hyperlink_id = 0;
                let t_res = to_res(broadcast_at(&target_grid, r, c));
                let n_res = name_grid.as_ref().map(|ng| to_res(broadcast_at(ng, r, c)));
                let cell = match Self::eval_hyperlink_vals(t_res, n_res, ctx) {
                    Ok(v) => v,
                    Err(e) => V::Error(e),
                };
                let link_id = ctx.last_hyperlink_id;
                let hb = cell.heap_bytes();
                if hb > 0 {
                    ctx.last_hyperlink_id = prev;
                    ctx.charge_array_extra_bytes(hb)?;
                }
                data.push(cell);
                if link_id != 0 {
                    let lv = links.get_or_insert_with(|| vec![0u32; total]);
                    lv[idx] = link_id;
                }
                if (idx & 4095) == 4095 {
                    ctx.last_hyperlink_id = prev;
                    ctx.charge_steps(1)?;
                    ctx.poll_deadline();
                    if ctx.expired() {
                        return Err(FormulaError::Value);
                    }
                }
            }
        }
        ctx.last_hyperlink_id = prev;
        Ok(Val::grid(Grid::from_rows_with_links(
            rows, cols, data, links,
        )))
    }
}

/// Whether `expr` reads a `LET` variable or calls one.
fn uses_vars(expr: &Expr) -> bool {
    let mut found = false;
    crate::names::walk(expr, |node| {
        found |= matches!(node, Expr::Var(_) | Expr::Call(_));
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
        V::Empty => Expr::Blank,
    }
}

fn unary_grid(ctx: &mut EvalContext<'_>, op: &Expr, grid: &Grid) -> Result<Grid, FormulaError> {
    ctx.charge_array_cells(grid.rows, grid.cols)?;
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
    Ok(Grid::from_rows(grid.rows, grid.cols, data))
}

fn binary_grid(
    ctx: &mut EvalContext<'_>,
    op: BinaryOp,
    l: &Grid,
    r: &Grid,
) -> Result<Grid, FormulaError> {
    let (rows, cols) = broadcast_dims(&[l, r]);
    ctx.charge_array_cells(rows, cols)?;
    let mut data = Vec::with_capacity(rows * cols);
    let mut idx = 0usize;
    for i in 0..rows {
        for j in 0..cols {
            let (a, b) = (broadcast_at(l, i, j), broadcast_at(r, i, j));
            let val = match (a, b) {
                (V::Error(e), _) | (_, V::Error(e)) => V::Error(e),
                (a, b) => SheetEngine::apply_binary_op(op, a, b),
            };
            let hb = val.heap_bytes();
            if hb > 0 {
                ctx.charge_array_extra_bytes(hb)?;
            }
            data.push(val);
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
    Ok(Grid::from_rows(rows, cols, data))
}

//! `SEQUENCE`, `RANDARRAY`, the array forms of `IF`, `IFERROR`, `IFNA` and `XLOOKUP` (PD-217).
//!
//! 참조 구현: IronCalc `base/src/functions/math_and_trigonometry/sequence.rs` (`fn_sequence` 10-125),
//! `random.rs` (`fn_randarray` 58-190), `array_size.rs` (크기 상한 1-27) (기준 커밋
//! 6dc9e8bae4b8d066276f399de7b3e1cf991c220b). MIT OR Apache-2.0, Copyright 2023 EqualTo GmbH,
//! 2023-present IronCalc contributors, 2026-present IronCalc GmbH. The size limit is the array budget
//! here (`MAX_ARRAY_CELLS`) and past it the result is `#SPILL!`, the spreadsheet program's answer for
//! an array that is too big. The reference has no array `XLOOKUP`; the match position comes from
//! this engine's `XMATCH`, so the match and search modes are the ones `XLOOKUP` already has.

use crate::array::{Env, Grid, Val};
use crate::array_eval::ValResult;
use crate::{
    EvalContext, EvaluatedCellValue as V, Expr, FormulaError, PackedCellCoordinate, SheetEngine,
};

impl SheetEngine {
    /// A size argument: truncated; `0` is `#CALC!`, a negative number `#VALUE!`, past the grid `#SPILL!`.
    fn arg_size(
        &self,
        args: &[Expr],
        i: usize,
        limit: u32,
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<usize, FormulaError> {
        let n = self.arg_number(args, i, 1.0, env, ctx, depth)?.floor();
        if n == 0.0 {
            return Err(FormulaError::Calc);
        }
        if n < 0.0 {
            return Err(FormulaError::Value);
        }
        if n > f64::from(limit) {
            return Err(FormulaError::Spill);
        }
        Ok(n as usize)
    }

    /// `SEQUENCE(rows, [columns], [start], [step])`.
    pub(crate) fn fn_sequence(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.is_empty() || args.len() > 4 {
            return Err(FormulaError::Value);
        }
        let rows = self.arg_size(args, 0, PackedCellCoordinate::MAX_ROWS, env, ctx, depth)?;
        let cols = self.arg_size(args, 1, PackedCellCoordinate::MAX_COLS, env, ctx, depth)?;
        let start = self.arg_number(args, 2, 1.0, env, ctx, depth)?;
        let step = self.arg_number(args, 3, 1.0, env, ctx, depth)?;
        let grid = Grid::build_checked(ctx, rows, cols, |r, c| {
            V::Number(start + (r * cols + c) as f64 * step)
        })?;
        Ok(Val::grid(grid))
    }

    /// `RANDARRAY([rows], [columns], [min], [max], [whole_number])`.
    pub(crate) fn fn_randarray(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.len() > 5 {
            return Err(FormulaError::Value);
        }
        let rows = self.arg_size(args, 0, PackedCellCoordinate::MAX_ROWS, env, ctx, depth)?;
        let cols = self.arg_size(args, 1, PackedCellCoordinate::MAX_COLS, env, ctx, depth)?;
        let min = self.arg_number(args, 2, 0.0, env, ctx, depth)?;
        let max = self.arg_number(args, 3, 1.0, env, ctx, depth)?;
        if min > max {
            return Err(FormulaError::Value);
        }
        let whole = self.arg_bool(args, 4, env, ctx, depth)?;
        ctx.charge_array_cells(rows, cols)?;
        let mut data = Vec::with_capacity(rows * cols);
        for _ in 0..rows * cols {
            let unit = ctx.next_rand_f64();
            data.push(V::Number(if whole {
                let low = min.floor();
                low + (unit * (max.floor() - low + 1.0)).floor()
            } else {
                min + unit * (max - min)
            }));
        }
        Ok(Val::grid(Grid::from_rows(rows, cols, data)))
    }

    /// `IF` over arrays: a single condition picks a branch (which stays whole, array or not), an array
    /// condition picks element by element.
    pub(crate) fn fn_if_array(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if !(2..=3).contains(&args.len()) {
            return Err(FormulaError::Value);
        }
        let cond = self.eval_val(&args[0], env, ctx, depth + 1)?.into_grid()?;
        let pick = |v: &V| match v {
            V::Error(e) => Err(*e),
            V::Empty => Ok(false),
            other => other.as_bool().ok_or(FormulaError::Value),
        };
        let branch = |this: &Self, i: usize, ctx: &mut EvalContext<'_>| match args.get(i) {
            Some(arg) => this.eval_val(arg, env, ctx, depth + 1),
            None => Ok(Val::scalar(V::Boolean(false))),
        };
        if cond.is_scalar() {
            return match pick(&cond.data[0]) {
                Ok(true) => branch(self, 1, ctx),
                Ok(false) => branch(self, 2, ctx),
                Err(e) => Ok(Val::error(e)),
            };
        }
        let yes = branch(self, 1, ctx)?.into_grid()?;
        let no = branch(self, 2, ctx)?.into_grid()?;
        let (rows, cols) = (
            cond.rows.max(yes.rows).max(no.rows),
            cond.cols.max(yes.cols).max(no.cols),
        );
        let grid = Grid::build_checked(ctx, rows, cols, |r, c| {
            let at = |g: &Grid| {
                let (r, c) = (
                    if g.rows == 1 { 0 } else { r },
                    if g.cols == 1 { 0 } else { c },
                );
                (r < g.rows && c < g.cols).then(|| g.get(r, c).clone())
            };
            match at(&cond) {
                Some(flag) => match pick(&flag) {
                    Ok(true) => at(&yes),
                    Ok(false) => at(&no),
                    Err(e) => Some(V::Error(e)),
                },
                None => None,
            }
            .unwrap_or(V::Error(FormulaError::Na))
        })?;
        Ok(Val::grid(grid))
    }

    /// `IFERROR(value, value_if_error)` and `IFNA(value, value_if_na)` over arrays.
    pub(crate) fn fn_iferror_array(
        &self,
        only_na: bool,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.len() != 2 {
            return Err(FormulaError::Value);
        }
        let value = self.eval_val(&args[0], env, ctx, depth + 1)?.into_grid()?;
        let caught = |v: &V| match v {
            V::Error(e) => !only_na || *e == FormulaError::Na,
            _ => false,
        };
        if !value.data.iter().any(caught) {
            return Ok(Val::Grid(value));
        }
        let fallback = self.eval_val(&args[1], env, ctx, depth + 1)?.into_grid()?;
        let (rows, cols) = (value.rows.max(fallback.rows), value.cols.max(fallback.cols));
        let grid = Grid::build_checked(ctx, rows, cols, |r, c| {
            let at = |g: &Grid| {
                let (r, c) = (
                    if g.rows == 1 { 0 } else { r },
                    if g.cols == 1 { 0 } else { c },
                );
                (r < g.rows && c < g.cols).then(|| g.get(r, c).clone())
            };
            match at(&value) {
                Some(v) if !caught(&v) => v,
                Some(_) => at(&fallback).unwrap_or(V::Error(FormulaError::Na)),
                None => V::Error(FormulaError::Na),
            }
        })?;
        Ok(Val::grid(grid))
    }

    /// `XLOOKUP` whose lookup values or return range are arrays: one result per lookup value, or the
    /// whole row (column) of the return range for a single match.
    pub(crate) fn fn_xlookup_array(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if !(3..=6).contains(&args.len()) {
            return Err(FormulaError::Value);
        }
        let lookup = self.eval_val(&args[0], env, ctx, depth + 1)?.into_grid()?;
        let mark = ctx.temp.len();
        let result = self.xlookup_with(&lookup, args, env, ctx, depth);
        ctx.temp.truncate(mark);
        result
    }

    fn xlookup_with(
        &self,
        lookup: &Grid,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        let mut rest: Vec<Expr> = args[1..].to_vec();
        for i in [0usize, 1] {
            if crate::array_class::needs_array(&rest[i]) || !env.is_empty() {
                let grid = self.eval_val(&rest[i], env, ctx, depth + 1)?.into_grid()?;
                rest[i] = self.as_arg(&grid, ctx)?;
            }
        }
        let return_range = self.ref_spec_dims(&rest[1], ctx, depth);
        let wide = match (self.ref_spec_dims(&rest[0], ctx, depth), return_range) {
            (Some((_, 1)), Some((_, cols))) => cols > 1,
            (Some((1, _)), Some((rows, _))) => rows > 1,
            _ => false,
        };
        if !wide {
            ctx.charge_array_cells(lookup.rows, lookup.cols)?;
            let mut data = Vec::with_capacity(lookup.rows * lookup.cols);
            for r in 0..lookup.rows {
                for c in 0..lookup.cols {
                    let mut one = vec![crate::array_eval::literal(lookup.get(r, c))];
                    one.extend(rest.iter().cloned());
                    data.push(self.eval_func_ctx("XLOOKUP", &one, ctx, depth));
                }
            }
            return Ok(Val::grid(Grid::from_rows(lookup.rows, lookup.cols, data)));
        }
        if !lookup.is_scalar() {
            return Err(FormulaError::Value);
        }
        let mut find = vec![
            crate::array_eval::literal(lookup.get(0, 0)),
            rest[0].clone(),
        ];
        for i in [3usize, 4] {
            find.push(rest.get(i).cloned().unwrap_or(Expr::Missing));
        }
        while matches!(find.last(), Some(Expr::Missing)) && find.len() > 2 {
            find.pop();
        }
        let found = match self.eval_func_ctx("XMATCH", &find, ctx, depth) {
            V::Number(n) => n as usize - 1,
            V::Error(FormulaError::Na) => {
                return match rest.get(2) {
                    Some(Expr::Missing) | None => Err(FormulaError::Na),
                    Some(fallback) => self.eval_val(fallback, env, ctx, depth + 1),
                }
            }
            V::Error(e) => return Err(e),
            _ => return Err(FormulaError::Value),
        };
        let Some(spec) = self.extract_streaming_range_ctx(&rest[1], ctx, depth)? else {
            return Err(FormulaError::Value);
        };
        let (rows, cols) = (spec.3, spec.4);
        let vertical = rows > 1
            && cols >= 1
            && self
                .ref_spec_dims(&rest[0], ctx, depth)
                .is_some_and(|d| d.1 == 1);
        let (out_rows, out_cols) = if vertical { (1, cols) } else { (rows, 1) };
        ctx.charge_array_cells(out_rows, out_cols)?;
        let mut data = Vec::with_capacity(out_rows * out_cols);
        for k in 0..out_rows * out_cols {
            let (dr, dc) = if vertical { (found, k) } else { (k, found) };
            data.push(
                match self.eval_streaming_cell_allow_error(&spec, dr, dc, ctx, depth) {
                    Ok(v) => v,
                    Err(e) => V::Error(e),
                },
            );
        }
        Ok(Val::grid(Grid::from_rows(out_rows, out_cols, data)))
    }

    /// Rows and columns of a reference expression, `None` for anything else.
    fn ref_spec_dims(
        &self,
        expr: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Option<(usize, usize)> {
        let spec = self.extract_streaming_range_ctx(expr, ctx, depth).ok()??;
        Some((spec.3, spec.4))
    }

    /// The area of the formula at `coord`, from the last placement.
    pub(crate) fn spill_area(&self, coord: PackedCellCoordinate) -> Option<(usize, usize)> {
        self.spill.area(coord)
    }
}

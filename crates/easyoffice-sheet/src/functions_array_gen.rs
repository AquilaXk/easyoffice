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
        let grid = Grid::build_with_links_checked(ctx, rows, cols, |r, c| {
            let at = |g: &Grid| {
                let (r, c) = (
                    if g.rows == 1 { 0 } else { r },
                    if g.cols == 1 { 0 } else { c },
                );
                (r < g.rows && c < g.cols).then(|| (g.get(r, c).clone(), g.link_id(r, c)))
            };
            match at(&cond) {
                Some((flag, _)) => match pick(&flag) {
                    Ok(true) => at(&yes),
                    Ok(false) => at(&no),
                    Err(e) => Some((V::Error(e), 0)),
                },
                None => None,
            }
            .unwrap_or((V::Error(FormulaError::Na), 0))
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
        if value.is_scalar() {
            return self.eval_val(&args[1], env, ctx, depth + 1);
        }
        let fallback = self.eval_val(&args[1], env, ctx, depth + 1)?.into_grid()?;
        let (rows, cols) = (value.rows.max(fallback.rows), value.cols.max(fallback.cols));
        let grid = Grid::build_with_links_checked(ctx, rows, cols, |r, c| {
            let at = |g: &Grid| {
                let (r, c) = (
                    if g.rows == 1 { 0 } else { r },
                    if g.cols == 1 { 0 } else { c },
                );
                (r < g.rows && c < g.cols).then(|| (g.get(r, c).clone(), g.link_id(r, c)))
            };
            match at(&value) {
                Some((v, lid)) if !caught(&v) => (v, lid),
                Some(_) => at(&fallback).unwrap_or((V::Error(FormulaError::Na), 0)),
                None => (V::Error(FormulaError::Na), 0),
            }
        })?;
        Ok(Val::grid(grid))
    }

    /// `IFS(cond1, val1, ...)` preserving hyperlink handles and short-circuiting scalar conditions.
    pub(crate) fn fn_ifs_array(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.len() < 2 || args.len() % 2 != 0 {
            return Err(FormulaError::Value);
        }
        let to_bool = |v: &V| match v {
            V::Error(e) => Err(*e),
            V::Empty => Ok(false),
            other => other.as_bool().ok_or(FormulaError::Value),
        };
        if !args.iter().step_by(2).any(crate::array_class::arrayish) {
            for pair in args.chunks(2) {
                let cond = self.eval_val(&pair[0], env, ctx, depth + 1)?.into_grid()?;
                if cond.is_scalar() {
                    match to_bool(&cond.data[0]) {
                        Ok(true) => return self.eval_val(&pair[1], env, ctx, depth + 1),
                        Ok(false) => continue,
                        Err(e) => return Ok(Val::error(e)),
                    }
                }
            }
            return Ok(Val::error(FormulaError::Na));
        }
        let mut pairs = Vec::with_capacity(args.len() / 2);
        let (mut rows, mut cols) = (1usize, 1usize);
        for pair in args.chunks(2) {
            let c = self.eval_val(&pair[0], env, ctx, depth + 1)?.into_grid()?;
            let v = self.eval_val(&pair[1], env, ctx, depth + 1)?.into_grid()?;
            rows = rows.max(c.rows).max(v.rows);
            cols = cols.max(c.cols).max(v.cols);
            pairs.push((c, v));
        }
        let grid = Grid::build_with_links_checked(ctx, rows, cols, |r, c| {
            let at = |g: &Grid| {
                let (rr, cc) = (
                    if g.rows == 1 { 0 } else { r },
                    if g.cols == 1 { 0 } else { c },
                );
                (rr < g.rows && cc < g.cols).then(|| (g.get(rr, cc).clone(), g.link_id(rr, cc)))
            };
            for (cg, vg) in &pairs {
                let Some((flag, _)) = at(cg) else {
                    return (V::Error(FormulaError::Na), 0);
                };
                match to_bool(&flag) {
                    Ok(true) => return at(vg).unwrap_or((V::Error(FormulaError::Na), 0)),
                    Ok(false) => {}
                    Err(e) => return (V::Error(e), 0),
                }
            }
            (V::Error(FormulaError::Na), 0)
        })?;
        Ok(Val::grid(grid))
    }

    /// `SWITCH(expr, val1, res1, ..., [default])` preserving hyperlink handles.
    pub(crate) fn fn_switch_array(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.len() < 3 {
            return Err(FormulaError::Value);
        }
        let target = self.eval_val(&args[0], env, ctx, depth + 1)?.into_grid()?;
        let rest = &args[1..];
        if target.is_scalar()
            && !rest
                .chunks_exact(2)
                .any(|p| crate::array_class::arrayish(&p[0]))
        {
            if let V::Error(e) = target.data[0] {
                return Ok(Val::error(e));
            }
            for pair in rest.chunks_exact(2) {
                let k = self.eval_val(&pair[0], env, ctx, depth + 1)?.into_grid()?;
                if k.is_scalar() {
                    if let V::Error(e) = k.data[0] {
                        return Ok(Val::error(e));
                    }
                    if Self::values_match(&target.data[0], &k.data[0]) {
                        return self.eval_val(&pair[1], env, ctx, depth + 1);
                    }
                }
            }
            return match rest.len() % 2 {
                1 => self.eval_val(&rest[rest.len() - 1], env, ctx, depth + 1),
                _ => Ok(Val::error(FormulaError::Na)),
            };
        }
        let mut pairs = Vec::with_capacity(rest.len() / 2);
        let (mut rows, mut cols) = (target.rows, target.cols);
        for pair in rest.chunks_exact(2) {
            let k = self.eval_val(&pair[0], env, ctx, depth + 1)?.into_grid()?;
            let v = self.eval_val(&pair[1], env, ctx, depth + 1)?.into_grid()?;
            rows = rows.max(k.rows).max(v.rows);
            cols = cols.max(k.cols).max(v.cols);
            pairs.push((k, v));
        }
        let default = if rest.len() % 2 == 1 {
            let d = self
                .eval_val(&rest[rest.len() - 1], env, ctx, depth + 1)?
                .into_grid()?;
            rows = rows.max(d.rows);
            cols = cols.max(d.cols);
            Some(d)
        } else {
            None
        };
        let grid = Grid::build_with_links_checked(ctx, rows, cols, |r, c| {
            let at = |g: &Grid| {
                let (rr, cc) = (
                    if g.rows == 1 { 0 } else { r },
                    if g.cols == 1 { 0 } else { c },
                );
                (rr < g.rows && cc < g.cols).then(|| (g.get(rr, cc).clone(), g.link_id(rr, cc)))
            };
            let Some((tv, _)) = at(&target) else {
                return (V::Error(FormulaError::Na), 0);
            };
            if let V::Error(e) = tv {
                return (V::Error(e), 0);
            }
            for (kg, vg) in &pairs {
                let Some((kv, _)) = at(kg) else {
                    return (V::Error(FormulaError::Na), 0);
                };
                if let V::Error(e) = kv {
                    return (V::Error(e), 0);
                }
                if Self::values_match(&tv, &kv) {
                    return at(vg).unwrap_or((V::Error(FormulaError::Na), 0));
                }
            }
            match &default {
                Some(dg) => at(dg).unwrap_or((V::Error(FormulaError::Na), 0)),
                None => (V::Error(FormulaError::Na), 0),
            }
        })?;
        Ok(Val::grid(grid))
    }

    /// `CHOOSE(index_num, value1, ...)` preserving arrays and hyperlink handles.
    pub(crate) fn fn_choose_branch_array(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.len() < 2 {
            return Err(FormulaError::Value);
        }
        let idx_grid = self.eval_val(&args[0], env, ctx, depth + 1)?.into_grid()?;
        let pick_idx = |v: &V| -> Result<usize, FormulaError> {
            if let V::Error(e) = v {
                return Err(*e);
            }
            let n = v.as_number().ok_or(FormulaError::Value)?.trunc() as isize;
            if n < 1 || (n as usize) >= args.len() {
                return Err(FormulaError::Value);
            }
            Ok(n as usize)
        };
        if idx_grid.is_scalar() {
            return match pick_idx(&idx_grid.data[0]) {
                Ok(i) => self.eval_val(&args[i], env, ctx, depth + 1),
                Err(e) => Ok(Val::error(e)),
            };
        }
        let mut choices: Vec<Option<std::sync::Arc<Grid>>> = vec![None; args.len()];
        let (mut rows, mut cols) = (idx_grid.rows, idx_grid.cols);
        for v in &idx_grid.data {
            if let Ok(i) = pick_idx(v) {
                if choices[i].is_none() {
                    let g = self.eval_val(&args[i], env, ctx, depth + 1)?.into_grid()?;
                    rows = rows.max(g.rows);
                    cols = cols.max(g.cols);
                    choices[i] = Some(g);
                }
            }
        }
        let grid = Grid::build_with_links_checked(ctx, rows, cols, |r, c| {
            let at = |g: &Grid| {
                let (rr, cc) = (
                    if g.rows == 1 { 0 } else { r },
                    if g.cols == 1 { 0 } else { c },
                );
                (rr < g.rows && cc < g.cols).then(|| (g.get(rr, cc).clone(), g.link_id(rr, cc)))
            };
            let Some((iv, _)) = at(&idx_grid) else {
                return (V::Error(FormulaError::Na), 0);
            };
            match pick_idx(&iv) {
                Ok(i) => choices[i]
                    .as_deref()
                    .and_then(at)
                    .unwrap_or((V::Error(FormulaError::Na), 0)),
                Err(e) => (V::Error(e), 0),
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
                    let cell = self.eval_func_ctx("XLOOKUP", &one, ctx, depth);
                    let hb = cell.heap_bytes();
                    if hb > 0 {
                        ctx.charge_array_extra_bytes(hb)?;
                    }
                    data.push(cell);
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
            let cell = match self.eval_streaming_cell_allow_error(&spec, dr, dc, ctx, depth) {
                Ok(v) => v,
                Err(e) => V::Error(e),
            };
            let hb = cell.heap_bytes();
            if hb > 0 {
                ctx.charge_array_extra_bytes(hb)?;
            }
            data.push(cell);
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

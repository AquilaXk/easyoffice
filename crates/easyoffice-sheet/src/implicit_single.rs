//! Implicit intersection operator `@` / `_xlfn.SINGLE(arg)` (MS-XLSX §2.2.2).
//!
//! Collapses single-column or single-row range/spill references onto the caller's row or column,
//! preserves 1x1 references, returns the top-left element of non-reference dynamic arrays, and
//! rejects 2D or out-of-range intersections with `#VALUE!` (missing target sheets with `#REF!`).

use std::borrow::Cow;

use crate::array::Env;
use crate::array_class::needs_array;
use crate::array_eval::uses_vars;
use crate::range_refs::ResolvedRangeRef;
use crate::{
    CellRange, EvalContext, EvaluatedCellValue as V, Expr, FormulaError, PackedCellCoordinate,
    PackedWorkbookCoord, SheetEngine,
};

impl SheetEngine {
    fn intersect_range_ref<'b>(
        &self,
        sheet: Option<Cow<'b, str>>,
        range: CellRange,
        ctx: &EvalContext<'_>,
    ) -> Result<Option<ResolvedRangeRef<'b>>, FormulaError> {
        if let Some(ref sh) = sheet {
            if !self.has_sheet_in_ctx(sh, ctx) {
                return Err(FormulaError::Ref);
            }
        }
        let min_r = range.start.row().min(range.end.row());
        let max_r = range.start.row().max(range.end.row());
        let min_c = range.start.col().min(range.end.col());
        let max_c = range.start.col().max(range.end.col());
        let picked = if min_r == max_r && min_c == max_c {
            PackedCellCoordinate::new(min_r, min_c).map_err(|_| FormulaError::Ref)?
        } else if min_c == max_c {
            let caller = ctx.current_coord.ok_or(FormulaError::Value)?;
            if !(min_r..=max_r).contains(&caller.row()) {
                return Err(FormulaError::Value);
            }
            PackedCellCoordinate::new(caller.row(), min_c).map_err(|_| FormulaError::Ref)?
        } else if min_r == max_r {
            let caller = ctx.current_coord.ok_or(FormulaError::Value)?;
            if !(min_c..=max_c).contains(&caller.col()) {
                return Err(FormulaError::Value);
            }
            PackedCellCoordinate::new(min_r, caller.col()).map_err(|_| FormulaError::Ref)?
        } else {
            return Err(FormulaError::Value);
        };
        let cell_range = CellRange::new(picked, picked).map_err(|_| FormulaError::Ref)?;
        Ok(Some((sheet, cell_range, false)))
    }

    pub(crate) fn spill_ref_range<'b>(
        &self,
        sheet: Option<&'b str>,
        coord: PackedCellCoordinate,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<(Option<Cow<'b, str>>, CellRange), FormulaError> {
        let target_id = self
            .resolve_sheet_ref(sheet, ctx)
            .ok_or(FormulaError::Ref)?
            .resolved_sheet_id();
        let anchor_val = self.eval_sheet_coord(sheet, coord, ctx, depth);
        let key = PackedWorkbookCoord::new(target_id, coord).0;
        let (rows, cols) = if let Some(grid) = ctx.fresh.get(&key) {
            (grid.rows, grid.cols)
        } else {
            let Some((is_array, area)) = self
                .resolve_sheet_ref(sheet, ctx)
                .map(|t| (t.array_formulas.contains(&coord), t.spill_area(coord)))
            else {
                return Err(FormulaError::Ref);
            };
            if !is_array || matches!(anchor_val, V::Error(FormulaError::Spill)) {
                return Err(FormulaError::Ref);
            }
            area.unwrap_or((1, 1))
        };
        let end_r = coord
            .row()
            .checked_add(u32::try_from(rows.saturating_sub(1)).map_err(|_| FormulaError::Ref)?)
            .ok_or(FormulaError::Ref)?;
        let end_c = coord
            .col()
            .checked_add(u32::try_from(cols.saturating_sub(1)).map_err(|_| FormulaError::Ref)?)
            .ok_or(FormulaError::Ref)?;
        let end = PackedCellCoordinate::new(end_r, end_c).map_err(|_| FormulaError::Ref)?;
        let range = CellRange::new(coord, end).map_err(|_| FormulaError::Ref)?;
        Ok((sheet.map(Cow::Borrowed), range))
    }

    /// Resolves the single-cell reference yielded by `SINGLE(arg)` when `arg` denotes a cell,
    /// range, spill reference, `Expr::Shared` name, or reference-returning function call.
    pub(crate) fn resolve_single_ref<'b>(
        &self,
        expr: &'b Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Option<ResolvedRangeRef<'b>>, FormulaError> {
        if depth > Self::MAX_RECURSION_DEPTH {
            return Err(FormulaError::Ref);
        }
        match expr.peeled() {
            Expr::ErrorLiteral(e) => Err(*e),
            Expr::Cell(c) => {
                let r = CellRange::new(*c, *c).map_err(|_| FormulaError::Ref)?;
                Ok(Some((None, r, false)))
            }
            Expr::SheetCell(sc) => {
                if !self.has_sheet_in_ctx(&sc.sheet, ctx) {
                    return Err(FormulaError::Ref);
                }
                let r = CellRange::new(sc.coord, sc.coord).map_err(|_| FormulaError::Ref)?;
                Ok(Some((Some(Cow::Borrowed(sc.sheet.as_str())), r, false)))
            }
            Expr::Range(r) => self.intersect_range_ref(None, *r, ctx),
            Expr::SheetRange(sr) => {
                self.intersect_range_ref(Some(Cow::Borrowed(sr.sheet.as_str())), sr.range, ctx)
            }
            Expr::SpillRef(sp) => {
                let (sheet, range) =
                    self.spill_ref_range(sp.sheet.as_deref(), sp.coord, ctx, depth + 1)?;
                self.intersect_range_ref(sheet, range, ctx)
            }
            Expr::FuncCall(f)
                if matches!(
                    f.name.as_str(),
                    "INDIRECT"
                        | "OFFSET"
                        | "INDEX"
                        | "CHOOSE"
                        | "IF"
                        | "SINGLE"
                        | "TRIMRANGE"
                        | ":"
                ) =>
            {
                let Some((sheet, range, _)) =
                    self.extract_range_and_sheet_ctx(expr, ctx, depth + 1)?
                else {
                    return Ok(None);
                };
                self.intersect_range_ref(sheet, range, ctx)
            }
            _ => Ok(None),
        }
    }

    pub(crate) fn eval_implicit_single(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<V, FormulaError> {
        self.eval_implicit_single_in_env(args, &Env::default(), ctx, depth)
    }

    pub(crate) fn eval_implicit_single_in_env(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<V, FormulaError> {
        if args.len() != 1 {
            return Err(FormulaError::Value);
        }
        let arg = &args[0];
        if !env.is_empty() {
            if let Some(bound_ref) = Self::bound_ref(arg, env) {
                return self.eval_single_bound_expr(&bound_ref, env, ctx, depth);
            }
        }
        self.eval_single_bound_expr(arg, env, ctx, depth)
    }

    fn eval_single_bound_expr(
        &self,
        arg: &Expr,
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<V, FormulaError> {
        if let Some((sheet_opt, range, _)) = self.resolve_single_ref(arg, ctx, depth + 1)? {
            let picked = range.start;
            if let Expr::SpillRef(sp) = arg.peeled() {
                if let Some(target) = self.resolve_sheet_ref(sheet_opt.as_deref(), ctx) {
                    let key = PackedWorkbookCoord::new(target.resolved_sheet_id(), sp.coord).0;
                    if let Some(grid) = ctx.fresh.get(&key) {
                        let dr = (picked.row() - sp.coord.row()) as usize;
                        let dc = (picked.col() - sp.coord.col()) as usize;
                        return Ok(grid.get(dr, dc).clone());
                    }
                }
            }
            return Ok(self.eval_sheet_coord(sheet_opt.as_deref(), picked, ctx, depth + 1));
        }
        if needs_array(arg) || (!env.is_empty() && uses_vars(arg)) {
            let grid = self.eval_val(arg, env, ctx, depth + 1)?.into_grid()?;
            return Ok(grid.top_left());
        }
        Ok(self.eval_expr_ctx(arg, ctx, depth + 1))
    }
}

//! Dynamic range reference extraction (`:`, `INDEX`, `CHOOSE`, `IF`) with fail-closed sheet,
//! virtual-sheet, spill-ref, and dynamic-read tracking.

use crate::{
    CellRange, EvalContext, EvaluatedCellValue, Expr, FormulaError, PackedCellCoordinate,
    SheetEngine, WorkbookEngine,
};
use std::borrow::Cow;

use crate::range_refs::ResolvedRangeRef;

impl SheetEngine {
    #[inline]
    pub(crate) fn is_virtual_sheet(sheet: Option<&str>) -> bool {
        sheet.is_some_and(|s| crate::array::virtual_sheet_index(s).is_some())
    }

    pub(crate) fn extract_index_ref<'b>(
        &self,
        args: &'b [Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Option<ResolvedRangeRef<'b>>, FormulaError> {
        if !(2..=4).contains(&args.len()) {
            return Err(FormulaError::Value);
        }
        if let Some(arg) = args.get(3).filter(|a| !matches!(a.peeled(), Expr::Missing)) {
            let area = self.eval_arg_num(arg, ctx, depth + 1)?.trunc();
            if area < 1.0 {
                return Err(FormulaError::Value);
            }
            if area > 1.0 {
                return Err(FormulaError::Ref);
            }
        }
        let Some((sheet_opt, base, transposed)) =
            self.extract_range_and_sheet_ctx(&args[0], ctx, depth + 1)?
        else {
            return Ok(None);
        };
        if transposed {
            return Ok(None);
        }
        let (rows, cols) = base.dimensions();
        let mut number = |i: usize| -> Result<Option<i64>, FormulaError> {
            match args.get(i) {
                None | Some(Expr::Missing) => Ok(None),
                Some(arg) => {
                    let n = self.eval_arg_num(arg, ctx, depth + 1)?.trunc() as i64;
                    if n < 0 {
                        return Err(FormulaError::Value);
                    }
                    Ok(Some(n))
                }
            }
        };
        let (row, col) = (number(1)?, number(2)?);
        let eff_len = args.len().min(3);
        let (row_arg, col_arg, row_all, col_all) = if eff_len == 3 {
            (row, col, row.unwrap_or(0) == 0, col.unwrap_or(0) == 0)
        } else if rows == 1 && cols > 1 {
            (Some(1), row, false, row.unwrap_or(0) == 0)
        } else if rows > 1 && cols > 1 {
            (row, None, row.unwrap_or(0) == 0, true)
        } else {
            (row, Some(1), row.unwrap_or(0) == 0, false)
        };
        let pick = |all: bool, n: Option<i64>, len: usize| -> Result<(u32, u32), FormulaError> {
            if all {
                return Ok((0, len as u32 - 1));
            }
            let n = n.unwrap_or(1);
            if !(1..=len as i64).contains(&n) {
                return Err(FormulaError::Ref);
            }
            let pos = n as u32 - 1;
            Ok((pos, pos))
        };
        let (r0, r1) = pick(row_all, row_arg, rows)?;
        let (c0, c1) = pick(col_all, col_arg, cols)?;
        let (min_r, min_c) = (
            base.start.row().min(base.end.row()),
            base.start.col().min(base.end.col()),
        );
        let corner = |r: u32, c: u32| PackedCellCoordinate::new(min_r + r, min_c + c);
        let range = CellRange::new(
            corner(r0, c0).map_err(|_| FormulaError::Ref)?,
            corner(r1, c1).map_err(|_| FormulaError::Ref)?,
        )
        .map_err(|_| FormulaError::Ref)?;
        Ok(Some((sheet_opt, range, false)))
    }

    pub(crate) fn extract_choose_ref<'b>(
        &self,
        args: &'b [Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Option<ResolvedRangeRef<'b>>, FormulaError> {
        if args.len() < 2 {
            return Err(FormulaError::Value);
        }
        if crate::array_class::arrayish(&args[0]) {
            return Ok(None);
        }
        let idx = self.eval_arg_num(&args[0], ctx, depth + 1)?.trunc() as isize;
        if idx < 1 || (idx as usize) >= args.len() {
            return Err(FormulaError::Value);
        }
        self.extract_range_and_sheet_ctx(&args[idx as usize], ctx, depth + 1)
    }

    pub(crate) fn extract_if_ref<'b>(
        &self,
        args: &'b [Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Option<ResolvedRangeRef<'b>>, FormulaError> {
        if !(2..=3).contains(&args.len()) {
            return Err(FormulaError::Value);
        }
        if crate::array_class::arrayish(&args[0]) {
            return Ok(None);
        }
        let cond = match self.eval_arg_val(&args[0], ctx, depth + 1)? {
            EvaluatedCellValue::Empty => false,
            other => other.as_bool().ok_or(FormulaError::Value)?,
        };
        if cond {
            self.extract_range_and_sheet_ctx(&args[1], ctx, depth + 1)
        } else if let Some(branch) = args.get(2).filter(|a| !matches!(a.peeled(), Expr::Missing)) {
            self.extract_range_and_sheet_ctx(branch, ctx, depth + 1)
        } else {
            Ok(None)
        }
    }

    fn resolve_colon_endpoint<'b>(
        &self,
        ep: &'b Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<(Option<Cow<'b, str>>, CellRange), FormulaError> {
        let ep = ep.peeled();
        if let Expr::SpillRef(sp) = ep {
            return self.spill_ref_range(sp.sheet.as_deref(), sp.coord, ctx, depth + 1);
        }
        if let Some((sh, range, _)) = self.extract_range_and_sheet_ctx(ep, ctx, depth + 1)? {
            if Self::is_virtual_sheet(sh.as_deref()) {
                return Err(FormulaError::Value);
            }
            return Ok((sh, range));
        }
        if crate::array_class::needs_array(ep) {
            let val = self.eval_val(ep, &crate::array::Env::default(), ctx, depth + 1)?;
            let grid = val.into_grid()?;
            if let [EvaluatedCellValue::Error(e)] = grid.data.as_slice() {
                return Err(*e);
            }
            return Err(FormulaError::Value);
        }
        match self.eval_expr_ctx(ep, ctx, depth + 1) {
            EvaluatedCellValue::Error(e) => Err(e),
            _ => Err(FormulaError::Value),
        }
    }

    pub(crate) fn extract_colon_range_ref<'b>(
        &self,
        args: &'b [Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Option<ResolvedRangeRef<'b>>, FormulaError> {
        if args.len() != 2 {
            return Err(FormulaError::Value);
        }
        let (sh_a, ra) = self.resolve_colon_endpoint(&args[0], ctx, depth)?;
        let (sh_b, rb) = self.resolve_colon_endpoint(&args[1], ctx, depth)?;
        if let Some(ref sa) = sh_a {
            if !self.has_sheet_in_ctx(sa, ctx) {
                return Err(FormulaError::Ref);
            }
        }
        if let Some(ref sb) = sh_b {
            if !self.has_sheet_in_ctx(sb, ctx) {
                return Err(FormulaError::Ref);
            }
        }
        let eff_a = sh_a.as_deref().unwrap_or(&self.name);
        let eff_b = sh_b.as_deref().unwrap_or(&self.name);
        if !WorkbookEngine::is_same_sheet_identity(eff_a, eff_b) {
            return Err(FormulaError::Ref);
        }
        let out_sheet = if WorkbookEngine::is_same_sheet_identity(eff_a, &self.name) {
            None
        } else {
            sh_a.or(sh_b)
        };
        let r0 = ra
            .start
            .row()
            .min(ra.end.row())
            .min(rb.start.row())
            .min(rb.end.row());
        let r1 = ra
            .start
            .row()
            .max(ra.end.row())
            .max(rb.start.row())
            .max(rb.end.row());
        let c0 = ra
            .start
            .col()
            .min(ra.end.col())
            .min(rb.start.col())
            .min(rb.end.col());
        let c1 = ra
            .start
            .col()
            .max(ra.end.col())
            .max(rb.start.col())
            .max(rb.end.col());
        let start = PackedCellCoordinate::new(r0, c0).map_err(|_| FormulaError::Ref)?;
        let end = PackedCellCoordinate::new(r1, c1).map_err(|_| FormulaError::Ref)?;
        let range = CellRange::new(start, end).map_err(|_| FormulaError::Ref)?;
        let resolved = (out_sheet, range, false);
        self.note_dynamic_read(Some(&resolved), ctx);
        Ok(Some(resolved))
    }
}

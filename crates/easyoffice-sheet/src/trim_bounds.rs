//! Sparse used-range clipping for `TRIMRANGE` over full column, row, and sheet references.

use crate::dag::CellIndex;
use crate::range_refs::ResolvedRangeRef;
use crate::{
    CellRange, EvalContext, EvaluatedCellValue, Expr, FormulaError, PackedCellCoordinate,
    SheetEngine,
};

impl CellIndex {
    /// Returns the tightest bounding rectangle `(min_r, max_r, min_c, max_c)` of populated cells
    /// inside `[min_r..=max_r, min_c..=max_c]`, or `None` when no cell in that rectangle is indexed.
    pub(crate) fn populated_bounds_in_range(
        &self,
        min_r: u32,
        max_r: u32,
        min_c: u32,
        max_c: u32,
    ) -> Option<(u32, u32, u32, u32)> {
        if min_r > max_r || min_c > max_c || self.sorted_coords.is_empty() {
            return None;
        }
        if min_c == max_c {
            let rows = self.col_rows_slice(min_c, min_r, max_r);
            return (!rows.is_empty()).then(|| (rows[0], rows[rows.len() - 1], min_c, min_c));
        }
        if min_r == max_r {
            let coords = self.row_coords_slice(min_r, min_c, max_c);
            return (!coords.is_empty()).then(|| {
                (
                    min_r,
                    min_r,
                    coords[0].col(),
                    coords[coords.len() - 1].col(),
                )
            });
        }
        let min_key = PackedCellCoordinate((u64::from(min_r) << 32) | u64::from(min_c));
        let max_key = PackedCellCoordinate((u64::from(max_r) << 32) | u64::from(max_c));
        let lo_s = self.sorted_coords.partition_point(|&c| c < min_key);
        let hi_s = self.sorted_coords.partition_point(|&c| c <= max_key);
        if lo_s >= hi_s {
            return None;
        }
        let mut bounds: Option<(u32, u32, u32, u32)> = None;
        let mut expand = |r: u32, c: u32| {
            bounds = Some(match bounds {
                Some((r0, r1, c0, c1)) => (r0.min(r), r1.max(r), c0.min(c), c1.max(c)),
                None => (r, r, c, c),
            });
        };
        let col_span = (max_c - min_c + 1) as usize;
        let active = col_span.min(self.by_col.len());
        if hi_s - lo_s <= active.saturating_mul(4) {
            for &coord in &self.sorted_coords[lo_s..hi_s] {
                let c = coord.col();
                if (min_c..=max_c).contains(&c) {
                    expand(coord.row(), c);
                }
            }
        } else if col_span <= self.by_col.len().saturating_mul(2) {
            for col in min_c..=max_c {
                let rows = self.col_rows_slice(col, min_r, max_r);
                if let (Some(&first_r), Some(&last_r)) = (rows.first(), rows.last()) {
                    expand(first_r, col);
                    expand(last_r, col);
                }
            }
        } else {
            for (&col, rows) in &self.by_col {
                if (min_c..=max_c).contains(&col) {
                    let lo = rows.partition_point(|&r| r < min_r);
                    let hi = rows.partition_point(|&r| r <= max_r);
                    if lo < hi {
                        expand(rows[lo], col);
                        expand(rows[hi - 1], col);
                    }
                }
            }
        }
        bounds
    }
}

#[inline]
fn merge_overlay_bounds(
    index_bounds: Option<(u32, u32, u32, u32)>,
    overlay: Option<(PackedCellCoordinate, bool)>,
    sr0: u32,
    sr1: u32,
    sc0: u32,
    sc1: u32,
) -> Option<(u32, u32, u32, u32)> {
    let ov = overlay.filter(|&(c, non_blank)| {
        non_blank && (sr0..=sr1).contains(&c.row()) && (sc0..=sc1).contains(&c.col())
    });
    match (index_bounds, ov) {
        (Some((r0, r1, c0, c1)), Some((c, _))) => Some((
            r0.min(c.row()),
            r1.max(c.row()),
            c0.min(c.col()),
            c1.max(c.col()),
        )),
        (None, Some((c, _))) => Some((c.row(), c.row(), c.col(), c.col())),
        (other, None) => other,
    }
}

#[derive(Clone, Copy)]
struct TrimScope<'a> {
    sheet_opt: Option<&'a str>,
    overlay: Option<(PackedCellCoordinate, bool)>,
    depth: usize,
}

impl SheetEngine {
    fn pop_bounds_in_sheet(
        &self,
        scope: TrimScope<'_>,
        r_span: (u32, u32),
        c_span: (u32, u32),
        ctx: &EvalContext<'_>,
    ) -> Result<Option<(u32, u32, u32, u32)>, FormulaError> {
        let (sr0, sr1) = r_span;
        let (sc0, sc1) = c_span;
        if sr0 > sr1 || sc0 > sc1 {
            return Ok(None);
        }
        let ts = self
            .resolve_sheet_ref(scope.sheet_opt, ctx)
            .ok_or(FormulaError::Ref)?;
        let ib = ts
            .cell_index()
            .populated_bounds_in_range(sr0, sr1, sc0, sc1);
        Ok(merge_overlay_bounds(ib, scope.overlay, sr0, sr1, sc0, sc1))
    }

    fn row_has_non_blank(
        &self,
        scope: TrimScope<'_>,
        row: u32,
        c_span: (u32, u32),
        ctx: &mut EvalContext<'_>,
    ) -> Result<bool, FormulaError> {
        let (sc0, sc1) = c_span;
        let mut coords = self
            .resolve_sheet_ref(scope.sheet_opt, ctx)
            .ok_or(FormulaError::Ref)?
            .cell_index()
            .row_coords_slice(row, sc0, sc1)
            .to_vec();
        if let Some((oc, true)) = scope.overlay {
            if oc.row() == row && (sc0..=sc1).contains(&oc.col()) && !coords.contains(&oc) {
                coords.push(oc);
            }
        }
        for coord in coords {
            let v = self.eval_sheet_coord(scope.sheet_opt, coord, ctx, scope.depth);
            if ctx.steps > Self::MAX_EVAL_STEPS || ctx.expired() {
                return Err(FormulaError::Value);
            }
            if !matches!(v, EvaluatedCellValue::Empty) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn col_has_non_blank(
        &self,
        scope: TrimScope<'_>,
        col: u32,
        r_span: (u32, u32),
        ctx: &mut EvalContext<'_>,
    ) -> Result<bool, FormulaError> {
        let (sr0, sr1) = r_span;
        let mut rows = self
            .resolve_sheet_ref(scope.sheet_opt, ctx)
            .ok_or(FormulaError::Ref)?
            .cell_index()
            .col_rows_slice(col, sr0, sr1)
            .to_vec();
        if let Some((oc, true)) = scope.overlay {
            if oc.col() == col && (sr0..=sr1).contains(&oc.row()) && !rows.contains(&oc.row()) {
                rows.push(oc.row());
            }
        }
        for r in rows {
            let coord = PackedCellCoordinate::new(r, col).map_err(|_| FormulaError::Ref)?;
            let v = self.eval_sheet_coord(scope.sheet_opt, coord, ctx, scope.depth);
            if ctx.steps > Self::MAX_EVAL_STEPS || ctx.expired() {
                return Err(FormulaError::Value);
            }
            if !matches!(v, EvaluatedCellValue::Empty) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn edge_non_blank_row(
        &self,
        scope: TrimScope<'_>,
        mut r_span: (u32, u32),
        c_span: (u32, u32),
        leading: bool,
        ctx: &mut EvalContext<'_>,
    ) -> Result<u32, FormulaError> {
        while let Some((pr0, pr1, _, _)) = self.pop_bounds_in_sheet(scope, r_span, c_span, ctx)? {
            let row = if leading { pr0 } else { pr1 };
            if self.row_has_non_blank(scope, row, c_span, ctx)? {
                return Ok(row);
            }
            if pr0 >= pr1 {
                break;
            }
            if leading {
                r_span.0 = pr0 + 1;
            } else {
                r_span.1 = pr1 - 1;
            }
        }
        Err(FormulaError::Ref)
    }

    fn edge_non_blank_col(
        &self,
        scope: TrimScope<'_>,
        r_span: (u32, u32),
        mut c_span: (u32, u32),
        leading: bool,
        ctx: &mut EvalContext<'_>,
    ) -> Result<u32, FormulaError> {
        while let Some((_, _, pc0, pc1)) = self.pop_bounds_in_sheet(scope, r_span, c_span, ctx)? {
            let col = if leading { pc0 } else { pc1 };
            if self.col_has_non_blank(scope, col, r_span, ctx)? {
                return Ok(col);
            }
            if pc0 >= pc1 {
                break;
            }
            if leading {
                c_span.0 = pc0 + 1;
            } else {
                c_span.1 = pc1 - 1;
            }
        }
        Err(FormulaError::Ref)
    }

    /// Trims outer unpopulated or empty edges of a real-sheet range in $O(\log N)$ index time
    /// before array materialization, returning `#REF!` if no non-blank cell remains.
    pub(crate) fn trim_sheet_range(
        &self,
        sheet_opt: Option<&str>,
        range: CellRange,
        transposed: bool,
        modes: (u8, u8),
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<CellRange, FormulaError> {
        let r0 = range.start.row().min(range.end.row());
        let r1 = range.start.row().max(range.end.row());
        let c0 = range.start.col().min(range.end.col());
        let c1 = range.start.col().max(range.end.col());
        let (phys_trim_r, phys_trim_c) = if transposed {
            (modes.1, modes.0)
        } else {
            modes
        };
        if phys_trim_r == 0 && phys_trim_c == 0 {
            let start = PackedCellCoordinate::new(r0, c0).map_err(|_| FormulaError::Ref)?;
            let end = PackedCellCoordinate::new(r1, c1).map_err(|_| FormulaError::Ref)?;
            return CellRange::new(start, end).map_err(|_| FormulaError::Ref);
        }
        let sid = self
            .resolve_sheet_ref(sheet_opt, ctx)
            .ok_or(FormulaError::Ref)?
            .resolved_sheet_id();
        let overlay = ctx.cell_overlay.as_ref().and_then(|(os, oc, ov)| {
            (*os == sid && (r0..=r1).contains(&oc.row()) && (c0..=c1).contains(&oc.col()))
                .then_some((*oc, !matches!(ov, EvaluatedCellValue::Empty)))
        });
        let scope = TrimScope {
            sheet_opt,
            overlay,
            depth,
        };
        let Some((cur_r0, cur_r1, cur_c0, cur_c1)) =
            self.pop_bounds_in_sheet(scope, (r0, r1), (c0, c1), ctx)?
        else {
            return Err(FormulaError::Ref);
        };

        let c_pop = (cur_c0, cur_c1);
        let new_r0 = if phys_trim_r == 1 || phys_trim_r == 3 {
            self.edge_non_blank_row(scope, (cur_r0, cur_r1), c_pop, true, ctx)?
        } else {
            r0
        };
        let new_r1 = if phys_trim_r == 2 || phys_trim_r == 3 {
            let lo = if phys_trim_r == 3 { new_r0 } else { cur_r0 };
            self.edge_non_blank_row(scope, (lo, cur_r1), c_pop, false, ctx)?
        } else {
            r1
        };

        let r_out = (new_r0, new_r1);
        let new_c0 = if phys_trim_c == 1 || phys_trim_c == 3 {
            self.edge_non_blank_col(scope, r_out, (cur_c0, cur_c1), true, ctx)?
        } else {
            c0
        };
        let new_c1 = if phys_trim_c == 2 || phys_trim_c == 3 {
            let lo = if phys_trim_c == 3 { new_c0 } else { cur_c0 };
            self.edge_non_blank_col(scope, r_out, (lo, cur_c1), false, ctx)?
        } else {
            c1
        };

        let start = PackedCellCoordinate::new(new_r0, new_c0).map_err(|_| FormulaError::Ref)?;
        let end = PackedCellCoordinate::new(new_r1, new_c1).map_err(|_| FormulaError::Ref)?;
        CellRange::new(start, end).map_err(|_| FormulaError::Ref)
    }

    /// Resolves `TRIMRANGE(range, [trim_rows], [trim_cols])` to its trimmed real-sheet
    /// [`CellRange`] when the first argument is a non-transposed real reference and neither
    /// trim-mode argument is an array expression.
    pub(crate) fn extract_trimrange_ref<'b>(
        &self,
        args: &'b [Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Option<ResolvedRangeRef<'b>>, FormulaError> {
        if args.is_empty() || args.len() > 3 {
            return Err(FormulaError::Value);
        }
        let Some((sheet_opt, range, transposed)) =
            self.extract_range_and_sheet_ctx(&args[0], ctx, depth + 1)?
        else {
            return Ok(None);
        };
        if transposed || Self::is_virtual_sheet(sheet_opt.as_deref()) {
            return Ok(None);
        }
        if args.get(1).is_some_and(crate::array_class::arrayish)
            || args.get(2).is_some_and(crate::array_class::arrayish)
        {
            return Ok(None);
        }
        let parse_mode = |idx: usize, ctx: &mut EvalContext<'_>| -> Result<u8, FormulaError> {
            match args.get(idx) {
                None => Ok(3),
                Some(arg) if matches!(arg.peeled(), Expr::Missing) => Ok(3),
                Some(arg) => {
                    let v = self.eval_arg_val(arg, ctx, depth + 1)?;
                    let n = match v {
                        EvaluatedCellValue::Empty => 0.0,
                        other => other.as_number().ok_or(FormulaError::Value)?,
                    }
                    .trunc();
                    if !(0.0..=3.0).contains(&n) {
                        Err(FormulaError::Value)
                    } else {
                        Ok(n as u8)
                    }
                }
            }
        };
        let trim_rows = parse_mode(1, ctx)?;
        let trim_cols = parse_mode(2, ctx)?;
        let trimmed = self.trim_sheet_range(
            sheet_opt.as_deref(),
            range,
            false,
            (trim_rows, trim_cols),
            ctx,
            depth + 1,
        )?;
        Ok(Some((sheet_opt, trimmed, false)))
    }
}

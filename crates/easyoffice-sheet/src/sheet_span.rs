//! 3D sheet-span references (`Sheet1:Sheet3!A1`, `'Sheet 1:Sheet 3'!A1:B5`).

use crate::dag::{DepScanner, DepWalk, ScanSite};
use crate::expr::SheetSpanExpr;
use crate::functions::{cmp_f64, select_pair};
use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine, WorkbookEngine};

#[inline]
fn is_variadic_3d_fn(name: &str) -> bool {
    matches!(
        name,
        "SUM"
            | "SUMSQ"
            | "PRODUCT"
            | "AVERAGE"
            | "AVG"
            | "AVERAGEA"
            | "AVEDEV"
            | "COUNT"
            | "COUNTA"
            | "MIN"
            | "MINA"
            | "MAX"
            | "MAXA"
            | "MEDIAN"
            | "STDEV"
            | "STDEV.S"
            | "STDEVP"
            | "STDEV.P"
            | "STDEVA"
            | "STDEVPA"
            | "VAR"
            | "VAR.S"
            | "VARP"
            | "VAR.P"
            | "VARA"
            | "VARPA"
            | "DEVSQ"
            | "SKEW"
            | "SKEW.P"
            | "KURT"
            | "AND"
            | "OR"
            | "XOR"
    )
}

#[inline]
fn is_streaming_agg_3d_fn(name: &str) -> bool {
    matches!(
        name,
        "SUM" | "AVERAGE" | "AVG" | "MIN" | "MAX" | "COUNT" | "COUNTA" | "PRODUCT"
    )
}

/// Splits a single-quoted `Start:End` prefix token into two non-empty sheet names when it holds
/// exactly one colon (e.g. `'Sheet 1:Sheet 3'!A1`).
#[inline]
pub(crate) fn split_quoted_sheet_span(name: &str) -> Option<(&str, &str)> {
    let (start, end) = name.split_once(':')?;
    (!start.is_empty() && !end.is_empty() && !end.contains(':')).then_some((start, end))
}

impl WorkbookEngine {
    /// Tab-order sheet index range covered by `sp`, or `None` when an endpoint sheet is missing.
    pub(crate) fn sheet_span_indices(
        &self,
        sp: &SheetSpanExpr,
    ) -> Option<std::ops::RangeInclusive<usize>> {
        if let Some(whole) = sp.quoted_whole.as_deref() {
            if let Some(idx) = self.find_sheet_index(whole) {
                return Some(idx..=idx);
            }
        }
        let start = self.find_sheet_index(&sp.start_sheet)?;
        let end = self.find_sheet_index(&sp.end_sheet)?;
        Some(start.min(end)..=start.max(end))
    }
}

impl SheetEngine {
    /// Returns the literal single sheet name if `sp` came from `'A:B'!` and an actual sheet named
    /// `"A:B"` exists in the evaluation context.
    #[inline]
    pub(crate) fn exact_quoted_sheet<'a>(
        &self,
        sp: &'a SheetSpanExpr,
        ctx: &EvalContext<'_>,
    ) -> Option<&'a str> {
        sp.quoted_whole
            .as_deref()
            .filter(|w| self.has_sheet_in_ctx(w, ctx))
    }

    #[inline]
    pub(crate) fn args_have_sheet_span_candidate(args: &[Expr]) -> bool {
        args.iter()
            .any(|a| matches!(a.peeled(), Expr::SheetSpan(_)))
    }

    pub(crate) fn args_have_true_sheet_span(&self, args: &[Expr], ctx: &EvalContext<'_>) -> bool {
        args.iter().any(|a| {
            matches!(
                a.peeled(),
                Expr::SheetSpan(sp) if self.exact_quoted_sheet(sp, ctx).is_none()
            )
        })
    }

    /// Resolves the ordered slice of sheet names spanned by `sp` (`#REF!` if an endpoint is missing).
    pub(crate) fn resolve_sheet_span(
        &self,
        sp: &SheetSpanExpr,
        ctx: &EvalContext<'_>,
    ) -> Result<Vec<String>, FormulaError> {
        if let Some(whole) = self.exact_quoted_sheet(sp, ctx) {
            return Ok(vec![whole.to_string()]);
        }
        if let Some(wb) = ctx.workbook {
            let indices = wb.sheet_span_indices(sp).ok_or(FormulaError::Ref)?;
            return Ok(wb.sheets()[indices]
                .iter()
                .map(|s| s.name.clone())
                .collect());
        }
        if WorkbookEngine::is_same_sheet_identity(&self.name, &sp.start_sheet)
            && WorkbookEngine::is_same_sheet_identity(&self.name, &sp.end_sheet)
        {
            Ok(vec![self.name.clone()])
        } else {
            Err(FormulaError::Ref)
        }
    }

    /// Expands one `SheetSpan` into per-sheet `Expr::SheetRange` items in tab order.
    pub(crate) fn expand_sheet_span_exprs(
        &self,
        sp: &SheetSpanExpr,
        ctx: &mut EvalContext<'_>,
    ) -> Result<Vec<Expr>, FormulaError> {
        let sheets = self.resolve_sheet_span(sp, ctx)?;
        ctx.charge_steps(sheets.len().max(1))?;
        Ok(sheets
            .into_iter()
            .map(|sh| Expr::sheet_range(sh, sp.range))
            .collect())
    }

    /// Scalar / bare evaluation of a `SheetSpan`: single existing literal sheet reads normally,
    /// missing endpoint sheets yield `#REF!`, and multi-sheet spans yield `#VALUE!`.
    pub(crate) fn eval_sheet_span_scalar(
        &self,
        sp: &SheetSpanExpr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> EvaluatedCellValue {
        if let Some(whole) = self.exact_quoted_sheet(sp, ctx) {
            return if sp.range.start == sp.range.end {
                self.eval_sheet_coord(Some(whole), sp.range.start, ctx, depth)
            } else {
                EvaluatedCellValue::Error(FormulaError::Value)
            };
        }
        match self.resolve_sheet_span(sp, ctx) {
            Err(e) => EvaluatedCellValue::Error(e),
            Ok(_) => EvaluatedCellValue::Error(FormulaError::Value),
        }
    }

    fn collect_sheet_span_numbers(
        &self,
        sp: &SheetSpanExpr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Vec<f64>, FormulaError> {
        let sheet_exprs = self.expand_sheet_span_exprs(sp, ctx)?;
        let mut nums = Vec::new();
        self.visit_args_streaming(&sheet_exprs, ctx, depth, |v| {
            if let EvaluatedCellValue::Number(x) = v {
                nums.push(x);
            }
            Ok(())
        })?;
        Ok(nums)
    }

    fn reject_all_sheet_spans(
        &self,
        args: &[Expr],
        ctx: &EvalContext<'_>,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        for arg in args {
            if let Expr::SheetSpan(sp) = arg.peeled() {
                self.resolve_sheet_span(sp, ctx)?;
            }
        }
        Err(FormulaError::Value)
    }

    /// Evaluates a function call whose top-level arguments contain at least one 3D `SheetSpan`.
    pub(crate) fn eval_func_with_sheet_span(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        match name {
            "ISREF" => {
                if args.len() != 1 {
                    return Err(FormulaError::Value);
                }
                if let Expr::SheetSpan(sp) = args[0].peeled() {
                    return Ok(EvaluatedCellValue::Boolean(
                        self.resolve_sheet_span(sp, ctx).is_ok(),
                    ));
                }
                self.reject_all_sheet_spans(args, ctx)
            }
            "SHEETS" => {
                if args.len() != 1 {
                    return Err(FormulaError::Value);
                }
                if let Expr::SheetSpan(sp) = args[0].peeled() {
                    let count = self.resolve_sheet_span(sp, ctx)?.len();
                    return Ok(EvaluatedCellValue::Number(count as f64));
                }
                self.reject_all_sheet_spans(args, ctx)
            }
            _ if is_variadic_3d_fn(name) => {
                let mut expanded = Vec::with_capacity(args.len());
                for arg in args {
                    if let Expr::SheetSpan(sp) = arg.peeled() {
                        expanded.extend(self.expand_sheet_span_exprs(sp, ctx)?);
                    } else {
                        expanded.push(arg.clone());
                    }
                }
                ctx.charge_steps(expanded.len().max(1))?;
                if is_streaming_agg_3d_fn(name) {
                    self.eval_streaming_range_agg_ctx(name, &expanded, ctx, depth)
                } else {
                    self.eval_func_inner(name, &expanded, ctx, depth)
                }
            }
            "SMALL" | "LARGE" | "PERCENTILE" | "PERCENTILE.INC" | "PERCENTILE.EXC" | "QUARTILE"
            | "QUARTILE.INC" | "QUARTILE.EXC" | "TRIMMEAN" => {
                if args.len() != 2 {
                    return Err(FormulaError::Value);
                }
                if let Expr::SheetSpan(sp) = args[1].peeled() {
                    self.resolve_sheet_span(sp, ctx)?;
                    return Err(FormulaError::Value);
                }
                let Expr::SheetSpan(sp) = args[0].peeled() else {
                    return self.reject_all_sheet_spans(args, ctx);
                };
                let nums = self.collect_sheet_span_numbers(sp, ctx, depth)?;
                let param = self.eval_arg_num(&args[1], ctx, depth)?;
                match name {
                    "SMALL" | "LARGE" => {
                        Self::small_large_from_nums(name == "SMALL", nums, param.trunc() as isize)
                    }
                    "TRIMMEAN" => Self::trimmean_from_nums(nums, param),
                    _ => Self::percentile_from_nums(name, nums, param),
                }
            }
            "RANK" | "RANK.EQ" | "RANK.AVG" => {
                if !(2..=3).contains(&args.len()) {
                    return Err(FormulaError::Value);
                }
                let Expr::SheetSpan(sp) = args[1].peeled() else {
                    return self.reject_all_sheet_spans(args, ctx);
                };
                let sheet_exprs = self.expand_sheet_span_exprs(sp, ctx)?;
                self.stat_rank_over(
                    name == "RANK.AVG",
                    &args[0],
                    &sheet_exprs,
                    args.get(2),
                    ctx,
                    depth,
                )
            }
            _ => self.reject_all_sheet_spans(args, ctx),
        }
    }

    pub(crate) fn trimmean_from_nums(
        mut nums: Vec<f64>,
        pct: f64,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if nums.is_empty() || !(0.0..1.0).contains(&pct) {
            return Err(FormulaError::Num);
        }
        let g = ((nums.len() as f64 * pct).floor() as usize) / 2;
        if g * 2 >= nums.len() {
            return Err(FormulaError::Num);
        }
        nums.sort_unstable_by(cmp_f64);
        let slice = &nums[g..nums.len() - g];
        Ok(EvaluatedCellValue::Number(
            slice.iter().sum::<f64>() / (slice.len() as f64),
        ))
    }

    pub(crate) fn small_large_from_nums(
        is_small: bool,
        mut nums: Vec<f64>,
        k: isize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if k < 1 || (k as usize) > nums.len() {
            return Err(FormulaError::Num);
        }
        let idx = if is_small {
            (k as usize) - 1
        } else {
            nums.len() - (k as usize)
        };
        let (_, &mut val, _) = nums.select_nth_unstable_by(idx, cmp_f64);
        Ok(EvaluatedCellValue::Number(val))
    }

    pub(crate) fn percentile_from_nums(
        name: &str,
        mut nums: Vec<f64>,
        param: f64,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if nums.is_empty() {
            return Err(FormulaError::Num);
        }
        let is_exc = name.ends_with(".EXC");
        let p = if name.starts_with("QUARTILE") {
            let q = param.trunc() as i32;
            let valid = if is_exc { 1..=3 } else { 0..=4 };
            if !valid.contains(&q) {
                return Err(FormulaError::Num);
            }
            (q as f64) * 0.25
        } else if is_exc && (param <= 0.0 || param >= 1.0)
            || !is_exc && !(0.0..=1.0).contains(&param)
        {
            return Err(FormulaError::Num);
        } else {
            param
        };
        let pos = if is_exc {
            let raw = p * ((nums.len() + 1) as f64);
            if raw < 1.0 || raw > nums.len() as f64 {
                return Err(FormulaError::Num);
            }
            raw - 1.0
        } else {
            p * ((nums.len() - 1) as f64)
        };
        let lo = pos.floor() as usize;
        let hi = pos.ceil() as usize;
        let (v_lo, v_hi) = select_pair(&mut nums, lo, hi);
        Ok(EvaluatedCellValue::Number(
            v_lo + (pos - lo as f64) * (v_hi - v_lo),
        ))
    }

    pub(crate) fn stat_rank_over(
        &self,
        is_avg: bool,
        target_arg: &Expr,
        ref_exprs: &[Expr],
        order_arg: Option<&Expr>,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        let target = self.eval_arg_num(target_arg, ctx, depth)?;
        let order = match order_arg {
            Some(arg) => self.eval_arg_num(arg, ctx, depth)?.trunc() as i32,
            None => 0,
        };
        let mut equal_count = 0usize;
        let mut better_count = 0usize;
        self.visit_args_streaming(ref_exprs, ctx, depth, |v| {
            if let EvaluatedCellValue::Number(x) = v {
                if x == target {
                    equal_count += 1;
                } else if (order == 0 && x > target) || (order != 0 && x < target) {
                    better_count += 1;
                }
            }
            Ok(())
        })?;
        if equal_count == 0 {
            return Err(FormulaError::Na);
        }
        let rank = if is_avg {
            (better_count as f64) + ((equal_count + 1) as f64) * 0.5
        } else {
            (better_count + 1) as f64
        };
        Ok(EvaluatedCellValue::Number(rank))
    }
}

impl<'x> DepScanner<'x> {
    /// Registers every worksheet rectangle in `sp` so edits on interior sheets trigger recalc.
    pub(crate) fn scan_sheet_span(
        &mut self,
        walk: &mut DepWalk<'_>,
        sp: &'x SheetSpanExpr,
        site: ScanSite<'x>,
    ) {
        if let Some(wb) = self.wb {
            let Some(indices) = wb.sheet_span_indices(sp) else {
                return;
            };
            for idx in indices {
                let name = &wb.sheets()[idx].name;
                if let Some(ds) = self.sheets.resolve(self.root, Some(wb), name) {
                    let dep_sheet = self.sheets.get(ds);
                    let dep_sid = dep_sheet.resolved_sheet_id();
                    walk.range(dep_sheet, ds, dep_sid, sp.range, &mut self.scratch);
                }
            }
        } else if sp
            .quoted_whole
            .as_deref()
            .is_some_and(|w| WorkbookEngine::is_same_sheet_identity(w, &self.root.name))
            || (WorkbookEngine::is_same_sheet_identity(&sp.start_sheet, &self.root.name)
                && WorkbookEngine::is_same_sheet_identity(&sp.end_sheet, &self.root.name))
        {
            walk.range(site.sheet, site.slot, site.sid, sp.range, &mut self.scratch);
        }
    }
}

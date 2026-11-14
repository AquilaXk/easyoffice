//! Single- and multi-criteria range aggregation (`COUNTIF`, `SUMIF`, `AVERAGEIF`, `COUNTIFS`,
//! `SUMIFS`, `AVERAGEIFS`, `MINIFS`, `MAXIFS`) backed by `CellIndex` column range lookups.

use crate::coercion::wildcard_match;
use crate::functions_date_fin::parse_iso_date_serial;
use crate::range_refs::StreamingRangeSpec;
use crate::{
    EvalContext, EvaluatedCellValue, Expr, FormulaError, PackedCellCoordinate, SheetEngine,
};

/// Ranges above this many cells may switch to a populated-cell walk.
const SPARSE_BLOCK_MIN_CELLS: usize = 1024;

#[derive(Clone, Copy)]
pub(crate) enum CritNumOp {
    Ge,
    Le,
    Ne,
    Gt,
    Lt,
    Eq,
}

pub(crate) enum CompiledCriteria {
    Number(f64),
    NumericCmp(CritNumOp, f64),
    IsoDateEq(f64),
    IsoDateCmp(CritNumOp, f64),
    TextEq(String),
    TextNe(String),
    TextOrd(CritNumOp, String),
    Wildcard(String),
    NotWildcard(String),
}

impl CompiledCriteria {
    #[inline]
    fn has_wildcard_meta(s: &str) -> bool {
        s.contains('*') || s.contains('?') || s.contains('~')
    }

    #[inline]
    pub(crate) fn compile(crit: &EvaluatedCellValue) -> Self {
        match crit {
            EvaluatedCellValue::Number(cn) => Self::Number(*cn),
            EvaluatedCellValue::Empty => Self::TextEq(String::new()),
            EvaluatedCellValue::Boolean(b) => {
                Self::TextEq(if *b { "true" } else { "false" }.to_string())
            }
            EvaluatedCellValue::Error(e) => Self::TextEq(e.code().to_lowercase()),
            EvaluatedCellValue::Text(s) => Self::compile_text(s),
        }
    }

    /// Compiles a text criterion (`">=5"`, `"abc*"`, `"2026-01-01"`, ...). Surrounding spaces stay
    /// part of a text match (`" "` matches a single space); only the numeric and date parsers
    /// ignore them.
    fn compile_text(text: &str) -> Self {
        const OPERATORS: [(&str, CritNumOp); 6] = [
            (">=", CritNumOp::Ge),
            ("<=", CritNumOp::Le),
            ("<>", CritNumOp::Ne),
            (">", CritNumOp::Gt),
            ("<", CritNumOp::Lt),
            ("=", CritNumOp::Eq),
        ];
        if let Some((op_kind, rhs)) = OPERATORS
            .iter()
            .find_map(|(op_str, op_kind)| text.strip_prefix(op_str).map(|rhs| (*op_kind, rhs)))
        {
            return Self::compile_operator_rhs(op_kind, rhs);
        }
        if let Some(rhs_num) = crate::coercion::parse_strict_number(text) {
            return Self::Number(rhs_num);
        }
        if let Some(rhs_serial) = parse_iso_date_serial(text) {
            return Self::IsoDateEq(rhs_serial);
        }
        if Self::has_wildcard_meta(text) {
            Self::Wildcard(text.to_string())
        } else {
            Self::TextEq(text.to_lowercase())
        }
    }

    /// Compiles the right-hand side of an operator criterion.
    fn compile_operator_rhs(op_kind: CritNumOp, rhs: &str) -> Self {
        if let Some(rhs_num) = crate::coercion::parse_strict_number(rhs) {
            return Self::NumericCmp(op_kind, rhs_num);
        }
        if let Some(rhs_serial) = parse_iso_date_serial(rhs) {
            return Self::IsoDateCmp(op_kind, rhs_serial);
        }
        let is_eq = matches!(op_kind, CritNumOp::Eq);
        if (is_eq || matches!(op_kind, CritNumOp::Ne)) && Self::has_wildcard_meta(rhs) {
            return if is_eq {
                Self::Wildcard(rhs.to_string())
            } else {
                Self::NotWildcard(rhs.to_string())
            };
        }
        let rhs_lower = rhs.to_lowercase();
        match op_kind {
            CritNumOp::Eq => Self::TextEq(rhs_lower),
            CritNumOp::Ne => Self::TextNe(rhs_lower),
            _ => Self::TextOrd(op_kind, rhs_lower),
        }
    }

    #[inline]
    fn str_eq_lowercase(text: &str, lower_target: &str) -> bool {
        if text.is_ascii() && lower_target.is_ascii() {
            text.eq_ignore_ascii_case(lower_target)
        } else {
            text.chars()
                .flat_map(char::to_lowercase)
                .eq(lower_target.chars())
        }
    }

    #[inline]
    fn cell_eq_lowercase(cell: &EvaluatedCellValue, lower_target: &str) -> bool {
        match cell {
            EvaluatedCellValue::Text(s) => Self::str_eq_lowercase(s, lower_target),
            EvaluatedCellValue::Empty => lower_target.is_empty(),
            EvaluatedCellValue::Boolean(b) => {
                Self::str_eq_lowercase(if *b { "true" } else { "false" }, lower_target)
            }
            EvaluatedCellValue::Error(e) => Self::str_eq_lowercase(e.code(), lower_target),
            EvaluatedCellValue::Number(_) => {
                Self::str_eq_lowercase(&cell.to_display_string(), lower_target)
            }
        }
    }

    #[inline]
    fn cell_iso_date_serial(cell: &EvaluatedCellValue) -> Option<f64> {
        match cell {
            EvaluatedCellValue::Number(n) => Some(*n),
            EvaluatedCellValue::Text(ts) => parse_iso_date_serial(ts),
            _ => None,
        }
    }

    #[inline]
    fn num_eq(lhs_num: f64, rhs_num: f64) -> bool {
        if lhs_num == 0.0 || rhs_num == 0.0 {
            lhs_num == rhs_num
        } else {
            (lhs_num - rhs_num).abs() <= 1e-12 * lhs_num.abs().max(rhs_num.abs())
        }
    }

    #[inline]
    fn cmp_f64(op: CritNumOp, lhs_num: f64, rhs_num: f64) -> bool {
        match op {
            CritNumOp::Ge => lhs_num > rhs_num || Self::num_eq(lhs_num, rhs_num),
            CritNumOp::Le => lhs_num < rhs_num || Self::num_eq(lhs_num, rhs_num),
            CritNumOp::Ne => !Self::num_eq(lhs_num, rhs_num),
            CritNumOp::Gt => lhs_num > rhs_num && !Self::num_eq(lhs_num, rhs_num),
            CritNumOp::Lt => lhs_num < rhs_num && !Self::num_eq(lhs_num, rhs_num),
            CritNumOp::Eq => Self::num_eq(lhs_num, rhs_num),
        }
    }

    #[inline]
    fn cell_numeric_for_criteria(cell: &EvaluatedCellValue) -> Option<f64> {
        match cell {
            EvaluatedCellValue::Number(n) => Some(*n),
            EvaluatedCellValue::Text(s) => crate::coercion::parse_strict_number(s),
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn matches(&self, cell: &EvaluatedCellValue) -> bool {
        if let EvaluatedCellValue::Error(e) = cell {
            return match *self {
                Self::TextEq(ref rhs_lower) => Self::str_eq_lowercase(e.code(), rhs_lower),
                Self::TextNe(ref rhs_lower) => !Self::str_eq_lowercase(e.code(), rhs_lower),
                Self::NumericCmp(CritNumOp::Ne, _)
                | Self::IsoDateCmp(CritNumOp::Ne, _)
                | Self::NotWildcard(_) => true,
                _ => false,
            };
        }
        match *self {
            Self::Number(cn) => {
                Self::cell_numeric_for_criteria(cell).is_some_and(|v| Self::num_eq(v, cn))
            }
            Self::NumericCmp(op, rhs_num) => {
                let Some(lhs_num) = Self::cell_numeric_for_criteria(cell) else {
                    return matches!(op, CritNumOp::Ne);
                };
                Self::cmp_f64(op, lhs_num, rhs_num)
            }
            Self::IsoDateEq(rhs_serial) => Self::cell_iso_date_serial(cell)
                .is_some_and(|lhs_num| Self::num_eq(lhs_num, rhs_serial)),
            Self::IsoDateCmp(op, rhs_serial) => {
                let Some(lhs_num) = Self::cell_iso_date_serial(cell) else {
                    return matches!(op, CritNumOp::Ne);
                };
                Self::cmp_f64(op, lhs_num, rhs_serial)
            }
            Self::TextEq(ref rhs_lower) => Self::cell_eq_lowercase(cell, rhs_lower),
            Self::TextNe(ref rhs_lower) => !Self::cell_eq_lowercase(cell, rhs_lower),
            Self::TextOrd(op, ref rhs_lower) => {
                let EvaluatedCellValue::Text(s) = cell else {
                    return false;
                };
                let lhs_lower = s.to_lowercase();
                match op {
                    CritNumOp::Ge => lhs_lower.as_str() >= rhs_lower.as_str(),
                    CritNumOp::Le => lhs_lower.as_str() <= rhs_lower.as_str(),
                    CritNumOp::Gt => lhs_lower.as_str() > rhs_lower.as_str(),
                    CritNumOp::Lt => lhs_lower.as_str() < rhs_lower.as_str(),
                    CritNumOp::Eq => lhs_lower == *rhs_lower,
                    CritNumOp::Ne => lhs_lower != *rhs_lower,
                }
            }
            Self::Wildcard(ref pat) => match cell {
                EvaluatedCellValue::Text(s) => wildcard_match(s, pat),
                _ => false,
            },
            Self::NotWildcard(ref pat) => match cell {
                EvaluatedCellValue::Text(s) => !wildcard_match(s, pat),
                _ => true,
            },
        }
    }
}

/// Inputs of a multi-criteria aggregate (`COUNTIFS`, `SUMIFS`, ...).
struct MultiCritJob<'a> {
    specs: Vec<StreamingRangeSpec<'a>>,
    crits: Vec<CompiledCriteria>,
    target_spec: Option<StreamingRangeSpec<'a>>,
    r_count: usize,
    c_count: usize,
}

/// Running totals of matching cells.
#[derive(Default)]
struct CritTotals {
    sum: f64,
    count: usize,
    min_val: Option<f64>,
    max_val: Option<f64>,
}

impl SheetEngine {
    /// Exact populated cell count in a non-transposed block, if resolvable to a real sheet.
    fn block_pop_count(
        &self,
        sheet: Option<&str>,
        origin: (u32, u32),
        counts: (usize, usize),
        ctx: &EvalContext<'_>,
    ) -> Option<usize> {
        let (min_r, min_c) = origin;
        let (r_count, c_count) = counts;
        if r_count == 0 || c_count == 0 {
            return Some(0);
        }
        let ts = self.resolve_sheet_ref(sheet, ctx)?;
        let max_r = min_r + (r_count as u32) - 1;
        let max_c = min_c + (c_count as u32) - 1;
        Some(ts.cell_index().count_in_range(min_r, max_r, min_c, max_c))
    }

    /// Populated coordinates inside the anchored block when the block is large and sparse.
    /// Returns `Some(vec)` only when `pop_count > 0`; callers handle `pop_count == 0` via `block_pop_count`.
    fn sparse_block_population(
        &self,
        sheet: Option<&str>,
        origin: (u32, u32),
        counts: (usize, usize),
        min_cells: usize,
        ctx: &EvalContext<'_>,
    ) -> Option<Vec<PackedCellCoordinate>> {
        let (min_r, min_c) = origin;
        let (r_count, c_count) = counts;
        let total = r_count.saturating_mul(c_count);
        if total <= min_cells {
            return None;
        }
        let ts = self.resolve_sheet_ref(sheet, ctx)?;
        let max_r = min_r + (r_count as u32).saturating_sub(1);
        let max_c = min_c + (c_count as u32).saturating_sub(1);
        let pop_count = ts.cell_index().count_in_range(min_r, max_r, min_c, max_c);
        if total <= pop_count.saturating_mul(2) {
            return None;
        }
        if pop_count == 0 {
            return Some(Vec::new());
        }
        Some(ts.cell_index().range_coords(min_r, max_r, min_c, max_c))
    }

    pub(crate) fn eval_single_criteria_streaming(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if name == "COUNTIF" {
            return self.eval_countif(args, ctx, depth);
        }
        self.eval_sumif_family(name, args, ctx, depth)
    }

    fn eval_countif(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if args.len() != 2 {
            return Err(FormulaError::Value);
        }
        let Some(range_spec) = self.extract_streaming_range_ctx(&args[0], ctx, depth)? else {
            let val = self.eval_expr_ctx(&args[0], ctx, depth);
            let crit = self.eval_arg_val(&args[1], ctx, depth)?;
            let hit = CompiledCriteria::compile(&crit).matches(&val);
            return Ok(EvaluatedCellValue::Number(f64::from(u8::from(hit))));
        };
        let crit_val = self.eval_arg_val(&args[1], ctx, depth)?;
        let compiled_crit = CompiledCriteria::compile(&crit_val);
        let (ref sheet_opt, min_r, min_c, r_count, c_count, transposed) = range_spec;
        let total = r_count.saturating_mul(c_count);
        if !transposed && total > 64 {
            if let Some(pop_count) = self.block_pop_count(
                sheet_opt.as_deref(),
                (min_r, min_c),
                (r_count, c_count),
                ctx,
            ) {
                if pop_count == 0 {
                    ctx.steps = ctx.steps.saturating_add(1);
                    if ctx.steps > Self::MAX_EVAL_STEPS {
                        return Err(FormulaError::Value);
                    }
                    let matched = usize::from(compiled_crit.matches(&EvaluatedCellValue::Empty));
                    return Ok(EvaluatedCellValue::Number((total * matched) as f64));
                }
                if total > pop_count.saturating_mul(2) {
                    let ts = self.resolve_sheet_ref(sheet_opt.as_deref(), ctx).unwrap();
                    let max_r = min_r + (r_count as u32).saturating_sub(1);
                    let max_c = min_c + (c_count as u32).saturating_sub(1);
                    let pop_coords = ts.cell_index().range_coords(min_r, max_r, min_c, max_c);
                    let pop_len = pop_coords.len();
                    let mut cnt = 0usize;
                    for coord in pop_coords {
                        let val = self.eval_sheet_coord(sheet_opt.as_deref(), coord, ctx, depth);
                        cnt += usize::from(compiled_crit.matches(&val));
                    }
                    if compiled_crit.matches(&EvaluatedCellValue::Empty) {
                        cnt += total.saturating_sub(pop_len);
                    }
                    return Ok(EvaluatedCellValue::Number(cnt as f64));
                }
            }
        }
        let mut cnt = 0usize;
        for dr in 0..r_count {
            for dc in 0..c_count {
                let val = self.eval_streaming_cell_allow_error(&range_spec, dr, dc, ctx, depth)?;
                cnt += usize::from(compiled_crit.matches(&val));
            }
        }
        Ok(EvaluatedCellValue::Number(cnt as f64))
    }

    /// Resolves the SUMIF / AVERAGEIF sum range, broadcasting a single cell to the criteria shape.
    fn resolve_sum_target<'b>(
        &self,
        arg: &'b Expr,
        shape: (usize, usize),
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<StreamingRangeSpec<'b>, FormulaError> {
        let (r_count, c_count) = shape;
        let Some(mut ts) = self.extract_streaming_range_ctx(arg, ctx, depth)? else {
            return Err(FormulaError::Value);
        };
        if ts.3 == 1 && ts.4 == 1 && (r_count != 1 || c_count != 1) {
            if ts.1 as usize + r_count > PackedCellCoordinate::MAX_ROWS as usize
                || ts.2 as usize + c_count > PackedCellCoordinate::MAX_COLS as usize
            {
                return Err(FormulaError::Ref);
            }
            ts.3 = r_count;
            ts.4 = c_count;
        } else if ts.3 != r_count || ts.4 != c_count {
            return Err(FormulaError::Value);
        }
        Ok(ts)
    }

    fn eval_sumif_family(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if !(2..=3).contains(&args.len()) {
            return Err(FormulaError::Value);
        }
        let Some(range_spec) = self.extract_streaming_range_ctx(&args[0], ctx, depth)? else {
            return Err(FormulaError::Value);
        };
        let crit_val = self.eval_arg_val(&args[1], ctx, depth)?;
        let compiled_crit = CompiledCriteria::compile(&crit_val);
        let (_, _, _, r_count, c_count, transposed) = range_spec;
        let target_spec = match args.get(2) {
            Some(arg) => Some(self.resolve_sum_target(arg, (r_count, c_count), ctx, depth)?),
            None => None,
        };
        let mut anchor_spec = target_spec.as_ref().unwrap_or(&range_spec);
        if !transposed && !anchor_spec.5 && !compiled_crit.matches(&EvaluatedCellValue::Empty) {
            if let (Some(crit_pop), Some(targ_pop)) = (
                self.block_pop_count(
                    range_spec.0.as_deref(),
                    (range_spec.1, range_spec.2),
                    (r_count, c_count),
                    ctx,
                ),
                Some(anchor_spec).and_then(|sp| {
                    self.block_pop_count(sp.0.as_deref(), (sp.1, sp.2), (r_count, c_count), ctx)
                }),
            ) {
                if crit_pop < targ_pop {
                    anchor_spec = &range_spec;
                }
            }
        }
        let sparse = if transposed || anchor_spec.5 {
            None
        } else {
            self.sparse_block_population(
                anchor_spec.0.as_deref(),
                (anchor_spec.1, anchor_spec.2),
                (r_count, c_count),
                64,
                ctx,
            )
        };

        let mut sum = 0.0f64;
        let mut count = 0usize;
        let mut accumulate =
            |dr: usize, dc: usize, ctx: &mut EvalContext<'_>| -> Result<(), FormulaError> {
                let val = self.eval_streaming_cell_allow_error(&range_spec, dr, dc, ctx, depth)?;
                if !compiled_crit.matches(&val) {
                    return Ok(());
                }
                let target_v = match target_spec {
                    Some(ref ts) => self.eval_streaming_cell(ts, dr, dc, ctx, depth)?,
                    None => val,
                };
                if let EvaluatedCellValue::Number(n) = target_v {
                    sum += n;
                    count += 1;
                }
                Ok(())
            };
        if let Some(pop_coords) = sparse {
            for coord in pop_coords {
                let dr = (coord.row() - anchor_spec.1) as usize;
                let dc = (coord.col() - anchor_spec.2) as usize;
                accumulate(dr, dc, ctx)?;
            }
        } else {
            for dr in 0..r_count {
                for dc in 0..c_count {
                    accumulate(dr, dc, ctx)?;
                }
            }
        }

        if name != "AVERAGEIF" {
            return Ok(EvaluatedCellValue::Number(sum));
        }
        if count == 0 {
            return Err(FormulaError::DivByZero);
        }
        Ok(EvaluatedCellValue::Number(sum / (count as f64)))
    }

    pub(crate) fn eval_multi_criteria_streaming(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        let (target_spec, crit_args) = if name == "COUNTIFS" {
            if args.len() < 2 || (args.len() & 1) != 0 {
                return Err(FormulaError::Value);
            }
            (None, args)
        } else {
            if args.len() < 3 || (args.len() & 1) == 0 {
                return Err(FormulaError::Value);
            }
            let Some(ts) = self.extract_streaming_range_ctx(&args[0], ctx, depth)? else {
                return Err(FormulaError::Value);
            };
            (Some(ts), &args[1..])
        };

        let pair_count = crit_args.len() / 2;
        let mut specs = Vec::with_capacity(pair_count);
        let mut crit_vals = Vec::with_capacity(pair_count);
        let mut expected_dims: Option<(usize, usize)> = target_spec.as_ref().map(|ts| (ts.3, ts.4));

        for pair in crit_args.chunks(2) {
            let Some(sp) = self.extract_streaming_range_ctx(&pair[0], ctx, depth)? else {
                return Err(FormulaError::Value);
            };
            let dims = (sp.3, sp.4);
            if let Some(exp) = expected_dims {
                if dims != exp {
                    return Err(FormulaError::Value);
                }
            } else {
                expected_dims = Some(dims);
            }
            specs.push(sp);
            crit_vals.push(self.eval_arg_val(&pair[1], ctx, depth)?);
        }

        let (r_count, c_count) = expected_dims.ok_or(FormulaError::Value)?;
        let job = MultiCritJob {
            specs,
            crits: crit_vals.iter().map(CompiledCriteria::compile).collect(),
            target_spec,
            r_count,
            c_count,
        };
        let totals = self.run_multi_criteria(&job, ctx, depth)?;
        let (sum, count) = (totals.sum, totals.count);
        match name {
            "COUNTIFS" => Ok(EvaluatedCellValue::Number(count as f64)),
            "AVERAGEIFS" => {
                if count == 0 {
                    Err(FormulaError::DivByZero)
                } else {
                    Ok(EvaluatedCellValue::Number(sum / (count as f64)))
                }
            }
            "MINIFS" => Ok(EvaluatedCellValue::Number(totals.min_val.unwrap_or(0.0))),
            "MAXIFS" => Ok(EvaluatedCellValue::Number(totals.max_val.unwrap_or(0.0))),
            _ => Ok(EvaluatedCellValue::Number(sum)),
        }
    }

    /// Whether the cell at `(dr, dc)` satisfies every criteria pair.
    fn cell_passes_all(
        &self,
        job: &MultiCritJob<'_>,
        dr: usize,
        dc: usize,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<bool, FormulaError> {
        for (sp, cc) in job.specs.iter().zip(job.crits.iter()) {
            let val = self.eval_streaming_cell_allow_error(sp, dr, dc, ctx, depth)?;
            if !cc.matches(&val) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Adds the cell at `(dr, dc)` (which passed all criteria) to the totals.
    fn accumulate_match(
        &self,
        job: &MultiCritJob<'_>,
        dr: usize,
        dc: usize,
        totals: &mut CritTotals,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<(), FormulaError> {
        let Some(ref ts) = job.target_spec else {
            totals.count += 1;
            return Ok(());
        };
        if let EvaluatedCellValue::Number(n) = self.eval_streaming_cell(ts, dr, dc, ctx, depth)? {
            totals.sum += n;
            totals.count += 1;
            totals.min_val = Some(totals.min_val.map_or(n, |v| v.min(n)));
            totals.max_val = Some(totals.max_val.map_or(n, |v| v.max(n)));
        }
        Ok(())
    }

    fn run_multi_criteria(
        &self,
        job: &MultiCritJob<'_>,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<CritTotals, FormulaError> {
        let total = job.r_count.saturating_mul(job.c_count);
        let mut totals = CritTotals::default();
        let best_sparse = if total > 64 {
            let crit_cands = job
                .specs
                .iter()
                .zip(job.crits.iter())
                .filter(|(sp, cc)| !sp.5 && !cc.matches(&EvaluatedCellValue::Empty))
                .map(|(sp, _)| sp);
            let target_cand = job.target_spec.as_ref().filter(|ts| !ts.5).into_iter();
            crit_cands
                .chain(target_cand)
                .filter_map(|sp| {
                    let pop = self.block_pop_count(
                        sp.0.as_deref(),
                        (sp.1, sp.2),
                        (job.r_count, job.c_count),
                        ctx,
                    )?;
                    Some((sp, pop))
                })
                .min_by_key(|&(_, pop)| pop)
        } else {
            None
        };
        if let Some((anchor, min_pop)) = best_sparse.filter(|&(_, pop)| {
            pop == 0 || total > pop.saturating_mul(2).max(SPARSE_BLOCK_MIN_CELLS)
        }) {
            if min_pop == 0 {
                ctx.steps = ctx.steps.saturating_add(1);
                return Ok(totals);
            }
            self.multi_criteria_sparse(job, anchor, &mut totals, ctx, depth)?;
        } else if total > 64 && job.target_spec.is_none() && job.specs.iter().all(|sp| !sp.5) {
            let pop_sum: Option<usize> = job
                .specs
                .iter()
                .map(|sp| {
                    self.block_pop_count(
                        sp.0.as_deref(),
                        (sp.1, sp.2),
                        (job.r_count, job.c_count),
                        ctx,
                    )
                })
                .sum();
            if pop_sum.is_some_and(|sum| total > sum.saturating_mul(2)) {
                totals.count = self.multi_criteria_count_by_population(job, total, ctx, depth)?;
            } else {
                for dr in 0..job.r_count {
                    for dc in 0..job.c_count {
                        if self.cell_passes_all(job, dr, dc, ctx, depth)? {
                            self.accumulate_match(job, dr, dc, &mut totals, ctx, depth)?;
                        }
                    }
                }
            }
        } else {
            for dr in 0..job.r_count {
                for dc in 0..job.c_count {
                    if self.cell_passes_all(job, dr, dc, ctx, depth)? {
                        self.accumulate_match(job, dr, dc, &mut totals, ctx, depth)?;
                    }
                }
            }
        }
        Ok(totals)
    }

    /// Walks only the populated cells of the anchor range.
    fn multi_criteria_sparse(
        &self,
        job: &MultiCritJob<'_>,
        anchor: &StreamingRangeSpec<'_>,
        totals: &mut CritTotals,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<(), FormulaError> {
        let Some(anchor_sheet) = self.resolve_sheet_ref(anchor.0.as_deref(), ctx) else {
            return Ok(());
        };
        let max_r = anchor.1 + (job.r_count as u32).saturating_sub(1);
        let max_c = anchor.2 + (job.c_count as u32).saturating_sub(1);
        let pop_coords = anchor_sheet
            .cell_index()
            .range_coords(anchor.1, max_r, anchor.2, max_c);
        for coord in pop_coords {
            let dr = (coord.row() - anchor.1) as usize;
            let dc = (coord.col() - anchor.2) as usize;
            if self.cell_passes_all(job, dr, dc, ctx, depth)? {
                self.accumulate_match(job, dr, dc, totals, ctx, depth)?;
            }
        }
        Ok(())
    }

    /// COUNTIFS over mostly-empty ranges: count = total minus populated non-matching cells.
    fn multi_criteria_count_by_population(
        &self,
        job: &MultiCritJob<'_>,
        total: usize,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<usize, FormulaError> {
        let mut pop_offsets: Vec<(usize, usize)> = Vec::new();
        for sp in &job.specs {
            let Some(sh) = self.resolve_sheet_ref(sp.0.as_deref(), ctx) else {
                continue;
            };
            let max_r = sp.1 + (job.r_count as u32).saturating_sub(1);
            let max_c = sp.2 + (job.c_count as u32).saturating_sub(1);
            for c in sh.cell_index().range_coords(sp.1, max_r, sp.2, max_c) {
                pop_offsets.push(((c.row() - sp.1) as usize, (c.col() - sp.2) as usize));
            }
        }
        pop_offsets.sort_unstable();
        pop_offsets.dedup();
        let mut non_matching_pop = 0usize;
        for (dr, dc) in pop_offsets {
            if !self.cell_passes_all(job, dr, dc, ctx, depth)? {
                non_matching_pop += 1;
            }
        }
        Ok(total.saturating_sub(non_matching_pop))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_criteria_keep_surrounding_spaces() {
        let mut s = SheetEngine::new("SpaceCrit");
        s.set_cell("A1", " ").unwrap();
        s.set_cell("A2", "x").unwrap();
        s.set_cell("A3", "  ").unwrap();
        assert_eq!(
            s.evaluate_formula_str("=COUNTIF(A1:A3, \" \")"),
            EvaluatedCellValue::Number(1.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=COUNTIF(A1:A3, \"=  \")"),
            EvaluatedCellValue::Number(1.0)
        );
        s.set_cell("B1", "5").unwrap();
        assert_eq!(
            s.evaluate_formula_str("=COUNTIF(B1:B1, \" >= 5 \")"),
            EvaluatedCellValue::Number(0.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=COUNTIF(B1:B1, \">= 5 \")"),
            EvaluatedCellValue::Number(1.0)
        );
    }

    #[test]
    fn cell_index_sparse_range_queries_and_invalidation() {
        let mut s = SheetEngine::new("SparseTest");
        s.set_cell("A5", "10").unwrap();
        s.set_cell("A500", "25").unwrap();
        s.set_cell("A2000", "40").unwrap();
        s.set_cell("B5", "100").unwrap();
        s.set_cell("B500", "250").unwrap();
        s.set_cell("B2000", "400").unwrap();

        assert_eq!(
            s.evaluate_formula_str("=SUM(A1:B5000)"),
            EvaluatedCellValue::Number(825.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=SUMIF(A1:A5000, \">15\", B1:B5000)"),
            EvaluatedCellValue::Number(650.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=COUNTIF(A1:A5000, \">=25\")"),
            EvaluatedCellValue::Number(2.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=SUMIFS(B1:B5000, A1:A5000, \">10\", B1:B5000, \"<300\")"),
            EvaluatedCellValue::Number(250.0)
        );

        // Editing a cell invalidates the cached CellIndex.
        s.set_cell("A1200", "30").unwrap();
        s.set_cell("B1200", "200").unwrap();
        assert_eq!(
            s.evaluate_formula_str("=SUMIFS(B1:B5000, A1:A5000, \">10\", B1:B5000, \"<300\")"),
            EvaluatedCellValue::Number(450.0)
        );
    }

    #[test]
    fn q2_sparse_col_avoids_dense_scan_when_other_cols_populated() {
        let mut s = SheetEngine::new("SparseQ2");
        // Populate 8000 rows in Z so whole-sheet `cells.len() * 2` > 15_000 (defeating
        // whole-sheet heuristics while A1:A15000 has only 2 populated cells).
        for r in 1..=8000u32 {
            s.set_cell(&format!("Z{r}"), r.to_string()).unwrap();
        }
        s.set_cell("A5", "10").unwrap();
        s.set_cell("A20", "=\"\"").unwrap();

        assert_eq!(
            s.evaluate_formula_str("=COUNT(A1:A15000)"),
            EvaluatedCellValue::Number(1.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=COUNTA(A1:A15000)"),
            EvaluatedCellValue::Number(2.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=COUNTBLANK(A1:A15000)"),
            EvaluatedCellValue::Number(14999.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=COUNTIF(A1:A15000, \">5\")"),
            EvaluatedCellValue::Number(1.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=COUNTIF(A1:A15000, \"\")"),
            EvaluatedCellValue::Number(14999.0)
        );

        // 150 calls stay under MAX_FORMULA_LEN (8192) while dense scans would hit 2.25M > MAX_EVAL_STEPS.
        for (expr, expected) in [
            ("COUNT(A1:A15000)", 150.0),
            ("COUNTA(A1:A15000)", 300.0),
            ("COUNTBLANK(A1:A15000)", 150.0 * 14999.0),
            ("COUNTIF(A1:A15000, \">5\")", 150.0),
            ("COUNTIFS(Z1:Z15000, \">0\", B1:B15000, \">0\")", 0.0),
        ] {
            let joined = (0..150).map(|_| expr).collect::<Vec<_>>().join("+");
            assert_eq!(
                s.evaluate_formula_str(&format!("={joined}")),
                EvaluatedCellValue::Number(expected),
                "expr={expr}"
            );
        }

        // Array condition evaluation in sparse range (`array_eval::read_ref_grid`): `"" > 5` is TRUE in spreadsheet comparison order (10 and "" => 2 hits).
        let arr_cond = (0..40)
            .map(|_| "SUM(--(A1:A15000>5))")
            .collect::<Vec<_>>()
            .join("+");
        assert_eq!(
            s.evaluate_formula_str(&format!("={arr_cond}")),
            EvaluatedCellValue::Number(80.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=SUM(--(A1:A15000=10))"),
            EvaluatedCellValue::Number(1.0)
        );

        // Spilled cells committed via RecalcSession remain accurately counted in sparse ranges.
        let mut wb = crate::WorkbookEngine::new();
        wb.add_sheet("SpillQ2");
        wb.set_cell("SpillQ2", "A10", "=SEQUENCE(5)").unwrap();
        wb.set_cell("SpillQ2", "B1", "=COUNT(A1:A3500)").unwrap();
        wb.set_cell("SpillQ2", "B2", "=COUNTA(A1:A3500)").unwrap();
        wb.set_cell("SpillQ2", "B3", "=COUNTBLANK(A1:A3500)")
            .unwrap();
        wb.set_cell("SpillQ2", "B4", "=COUNTIF(A1:A3500, \">2\")")
            .unwrap();
        let mut session = crate::RecalcSession::new(wb).unwrap();
        session.recalculate();
        let a1 = |a: &str| PackedCellCoordinate::parse_a1(a).unwrap();
        assert_eq!(
            session.cell_value("SpillQ2", a1("B1")),
            EvaluatedCellValue::Number(5.0)
        );
        assert_eq!(
            session.cell_value("SpillQ2", a1("B2")),
            EvaluatedCellValue::Number(5.0)
        );
        assert_eq!(
            session.cell_value("SpillQ2", a1("B3")),
            EvaluatedCellValue::Number(3495.0)
        );
        assert_eq!(
            session.cell_value("SpillQ2", a1("B4")),
            EvaluatedCellValue::Number(3.0)
        );
    }
}

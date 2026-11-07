use crate::coercion::wildcard_match;
use crate::functions_date_fin::parse_iso_date_serial;
use crate::{
    CellRange, EvalContext, EvaluatedCellValue, Expr, FormulaError, PackedCellCoordinate,
    SheetEngine, WorkbookEngine,
};
use std::borrow::Cow;

/// Ranges above this many cells may switch to a populated-cell walk.
const SPARSE_BLOCK_MIN_CELLS: usize = 1024;

pub(crate) type StreamingRangeSpec<'a> = (Option<Cow<'a, str>>, u32, u32, usize, usize, bool);
pub(crate) type ResolvedRangeRef<'a> = (Option<Cow<'a, str>>, CellRange, bool);

fn split_indirect_sheet_bang(s: &str) -> Result<Option<(&str, &str)>, FormulaError> {
    let bytes = s.as_bytes();
    let mut in_quotes = false;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' if in_quotes => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                    i += 2;
                    continue;
                }
                in_quotes = false;
            }
            b'\'' => {
                in_quotes = true;
            }
            b'!' if !in_quotes => {
                return Ok(Some((&s[..i], &s[i + 1..])));
            }
            _ => {}
        }
        i += 1;
    }
    if in_quotes {
        return Err(FormulaError::Ref);
    }
    Ok(None)
}

fn split_once_unquoted_colon(s: &str) -> Result<Option<(&str, &str)>, FormulaError> {
    let bytes = s.as_bytes();
    let mut in_quotes = false;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' if in_quotes => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                    i += 2;
                    continue;
                }
                in_quotes = false;
            }
            b'\'' => {
                in_quotes = true;
            }
            b':' if !in_quotes => {
                return Ok(Some((&s[..i], &s[i + 1..])));
            }
            _ => {}
        }
        i += 1;
    }
    if in_quotes {
        return Err(FormulaError::Ref);
    }
    Ok(None)
}

fn parse_indirect_sheet_token(sh_raw: &str) -> Result<String, FormulaError> {
    let sh_tok = sh_raw.trim();
    if sh_tok.starts_with('\'') {
        WorkbookEngine::unquote_formula_sheet_token(sh_tok).ok_or(FormulaError::Ref)
    } else if crate::parser::is_valid_unquoted_sheet_ident(sh_tok) {
        Ok(sh_tok.to_string())
    } else {
        Err(FormulaError::Ref)
    }
}

fn parse_indirect_endpoint(
    part: &str,
) -> Result<(Option<String>, PackedCellCoordinate), FormulaError> {
    let trimmed = part.trim();
    if let Some((sh_raw, cell_part)) = split_indirect_sheet_bang(trimmed)? {
        let sh = parse_indirect_sheet_token(sh_raw)?;
        let coord =
            PackedCellCoordinate::parse_a1(cell_part.trim()).map_err(|_| FormulaError::Ref)?;
        Ok((Some(sh), coord))
    } else {
        let coord = PackedCellCoordinate::parse_a1(trimmed).map_err(|_| FormulaError::Ref)?;
        Ok((None, coord))
    }
}

/// Splits an optional `Sheet!` prefix off one side of an INDIRECT range.
fn split_indirect_side(raw: &str) -> Result<(Option<String>, &str), FormulaError> {
    match split_indirect_sheet_bang(raw.trim())? {
        Some((sh_raw, pt)) => Ok((Some(parse_indirect_sheet_token(sh_raw)?), pt.trim())),
        None => Ok((None, raw.trim())),
    }
}

impl SheetEngine {
    #[inline]
    pub(crate) fn has_sheet_in_ctx(&self, sheet: &str, ctx: &EvalContext<'_>) -> bool {
        WorkbookEngine::is_same_sheet_identity(sheet, &self.name)
            || ctx.workbook.and_then(|wb| wb.sheet(sheet)).is_some()
    }

    /// Attaches an optional (already parsed) sheet name to `range`, validating it exists.
    fn qualify_indirect_range<'b>(
        &self,
        sheet: Option<String>,
        range: CellRange,
        ctx: &EvalContext<'_>,
    ) -> Result<Option<ResolvedRangeRef<'b>>, FormulaError> {
        match sheet {
            Some(sh_clean) => {
                if sh_clean.is_empty() || !self.has_sheet_in_ctx(&sh_clean, ctx) {
                    return Err(FormulaError::Ref);
                }
                Ok(Some((Some(Cow::Owned(sh_clean)), range, false)))
            }
            None => Ok(Some((None, range, false))),
        }
    }

    fn extract_indirect_ref<'b>(
        &self,
        args: &'b [Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Option<ResolvedRangeRef<'b>>, FormulaError> {
        if args.is_empty() || args.len() > 2 {
            return Err(FormulaError::Value);
        }
        let ref_str = self
            .eval_arg_val(&args[0], ctx, depth + 1)?
            .to_display_string();
        if args.len() == 2 {
            let mode_val = self.eval_arg_val(&args[1], ctx, depth + 1)?;
            if mode_val.as_bool() != Some(true) {
                return Err(FormulaError::Value);
            }
        }
        let trimmed = ref_str.trim();
        if trimmed.is_empty() {
            return Err(FormulaError::Ref);
        }
        let Some((lhs_raw, rhs_raw)) = split_once_unquoted_colon(trimmed)? else {
            let (sh_opt, coord) = parse_indirect_endpoint(trimmed)?;
            let range = CellRange::new(coord, coord).map_err(|_| FormulaError::Ref)?;
            return self.qualify_indirect_range(sh_opt, range, ctx);
        };
        let (lhs_sh, lhs_tok) = split_indirect_side(lhs_raw)?;
        let (rhs_sh, rhs_tok) = split_indirect_side(rhs_raw)?;
        if let Some(ref rs) = rhs_sh {
            let same = lhs_sh
                .as_ref()
                .is_some_and(|ls| WorkbookEngine::is_same_sheet_identity(ls, rs));
            if !same {
                return Err(FormulaError::Ref);
            }
        }
        let range =
            CellRange::parse(&format!("{lhs_tok}:{rhs_tok}")).map_err(|_| FormulaError::Ref)?;
        self.qualify_indirect_range(lhs_sh, range, ctx)
    }

    fn extract_offset_ref<'b>(
        &self,
        args: &'b [Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Option<ResolvedRangeRef<'b>>, FormulaError> {
        if args.len() < 3 || args.len() > 5 {
            return Err(FormulaError::Value);
        }
        let Some((sheet_opt, base_range, _)) =
            self.extract_range_and_sheet_ctx(&args[0], ctx, depth + 1)?
        else {
            return Err(FormulaError::Value);
        };
        let dr = self.eval_arg_num(&args[1], ctx, depth + 1)?.trunc() as i64;
        let dc = self.eval_arg_num(&args[2], ctx, depth + 1)?.trunc() as i64;
        let (base_rows, base_cols) = base_range.dimensions();
        let height = match args.get(3) {
            Some(arg) => self.eval_arg_num(arg, ctx, depth + 1)?.trunc() as i64,
            None => base_rows as i64,
        };
        let width = match args.get(4) {
            Some(arg) => self.eval_arg_num(arg, ctx, depth + 1)?.trunc() as i64,
            None => base_cols as i64,
        };
        if height < 1 || width < 1 {
            return Err(FormulaError::Value);
        }
        let start_r = base_range.start.row().min(base_range.end.row()) as i64 + dr;
        let start_c = base_range.start.col().min(base_range.end.col()) as i64 + dc;
        if start_r < 0 || start_c < 0 {
            return Err(FormulaError::Ref);
        }
        let start_coord = PackedCellCoordinate::new(start_r as u32, start_c as u32)
            .map_err(|_| FormulaError::Ref)?;
        let end_coord =
            PackedCellCoordinate::new((start_r + height - 1) as u32, (start_c + width - 1) as u32)
                .map_err(|_| FormulaError::Ref)?;
        let range = CellRange::new(start_coord, end_coord).map_err(|_| FormulaError::Ref)?;
        Ok(Some((sheet_opt, range, false)))
    }

    pub(crate) fn extract_range_and_sheet_ctx<'b>(
        &self,
        expr: &'b Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Option<ResolvedRangeRef<'b>>, FormulaError> {
        if depth > Self::MAX_RECURSION_DEPTH {
            return Err(FormulaError::Ref);
        }
        match expr {
            Expr::ErrorLiteral(e) => Err(*e),
            Expr::Cell(c) => Ok(CellRange::new(*c, *c).ok().map(|r| (None, r, false))),
            Expr::Range(r) => Ok(Some((None, *r, false))),
            Expr::SheetCell { sheet, coord } => {
                if !self.has_sheet_in_ctx(sheet, ctx) {
                    return Err(FormulaError::Ref);
                }
                Ok(CellRange::new(*coord, *coord)
                    .ok()
                    .map(|r| (Some(Cow::Borrowed(sheet.as_str())), r, false)))
            }
            Expr::SheetRange { sheet, range } => {
                if !self.has_sheet_in_ctx(sheet, ctx) {
                    return Err(FormulaError::Ref);
                }
                Ok(Some((Some(Cow::Borrowed(sheet.as_str())), *range, false)))
            }
            Expr::FuncCall { name, args } if name == "TRANSPOSE" && args.len() == 1 => Ok(self
                .extract_range_and_sheet_ctx(&args[0], ctx, depth + 1)?
                .map(|(s, r, t)| (s, r, !t))),
            Expr::FuncCall { name, args } if name == "INDIRECT" => {
                self.extract_indirect_ref(args, ctx, depth)
            }
            Expr::FuncCall { name, args } if name == "OFFSET" => {
                self.extract_offset_ref(args, ctx, depth)
            }
            _ => Ok(None),
        }
    }

    #[inline]
    pub(crate) fn resolve_sheet_ref<'b>(
        &'b self,
        sheet_opt: Option<&str>,
        ctx: &'b EvalContext<'_>,
    ) -> Option<&'b SheetEngine> {
        match sheet_opt {
            None => Some(self),
            Some(target_name)
                if WorkbookEngine::is_same_sheet_identity(target_name, &self.name) =>
            {
                Some(self)
            }
            Some(target_name) => ctx.workbook.and_then(|wb| wb.sheet(target_name)),
        }
    }

    pub(crate) fn extract_streaming_range_ctx<'b>(
        &self,
        expr: &'b Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Option<StreamingRangeSpec<'b>>, FormulaError> {
        let Some((sheet_opt, range, transposed)) =
            self.extract_range_and_sheet_ctx(expr, ctx, depth)?
        else {
            return Ok(None);
        };
        let min_r = range.start.row().min(range.end.row());
        let max_r = range.start.row().max(range.end.row());
        let min_c = range.start.col().min(range.end.col());
        let max_c = range.start.col().max(range.end.col());
        let phys_rows = (max_r - min_r + 1) as usize;
        let phys_cols = (max_c - min_c + 1) as usize;
        let (rows, cols) = if transposed {
            (phys_cols, phys_rows)
        } else {
            (phys_rows, phys_cols)
        };
        Ok(Some((sheet_opt, min_r, min_c, rows, cols, transposed)))
    }

    #[inline]
    pub(crate) fn eval_streaming_cell_allow_error(
        &self,
        spec: &StreamingRangeSpec<'_>,
        dr: usize,
        dc: usize,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        // Dense walks over very wide ranges stop once the per-root step budget is spent
        // instead of spinning through billions of empty coordinates.
        if ctx.steps > Self::MAX_EVAL_STEPS {
            return Err(FormulaError::Value);
        }
        let (ref sheet_opt, min_r, min_c, _rows, _cols, transposed) = *spec;
        let (r, c) = if transposed {
            (min_r + dc as u32, min_c + dr as u32)
        } else {
            (min_r + dr as u32, min_c + dc as u32)
        };
        let coord = PackedCellCoordinate::new(r, c).map_err(|_| FormulaError::Ref)?;
        Ok(self.eval_sheet_coord(sheet_opt.as_deref(), coord, ctx, depth))
    }

    #[inline]
    pub(crate) fn eval_streaming_cell(
        &self,
        spec: &StreamingRangeSpec<'_>,
        dr: usize,
        dc: usize,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        let v = self.eval_streaming_cell_allow_error(spec, dr, dc, ctx, depth)?;
        if let EvaluatedCellValue::Error(e) = v {
            return Err(e);
        }
        Ok(v)
    }

    pub(crate) fn visit_args_streaming<F>(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
        mut f: F,
    ) -> Result<(), FormulaError>
    where
        F: FnMut(EvaluatedCellValue) -> Result<(), FormulaError>,
    {
        for arg in args {
            if let Some(spec) = self.extract_streaming_range_ctx(arg, ctx, depth)? {
                let (ref sheet_opt, min_r, min_c, rows, cols, transposed) = spec;
                let total = rows.saturating_mul(cols);
                if !transposed
                    && total > 1024
                    && self
                        .resolve_sheet_ref(sheet_opt.as_deref(), ctx)
                        .is_some_and(|ts| total > ts.cells.len().saturating_mul(2))
                {
                    let max_r = min_r + (rows as u32).saturating_sub(1);
                    let max_c = min_c + (cols as u32).saturating_sub(1);
                    let mut pop_coords: Vec<PackedCellCoordinate> = self
                        .resolve_sheet_ref(sheet_opt.as_deref(), ctx)
                        .unwrap()
                        .cells
                        .keys()
                        .copied()
                        .filter(|c| {
                            c.row() >= min_r
                                && c.row() <= max_r
                                && c.col() >= min_c
                                && c.col() <= max_c
                        })
                        .collect();
                    pop_coords.sort_unstable();
                    for coord in pop_coords {
                        let v = self.eval_sheet_coord(sheet_opt.as_deref(), coord, ctx, depth);
                        if let EvaluatedCellValue::Error(e) = v {
                            return Err(e);
                        }
                        f(v)?;
                    }
                } else {
                    for dr in 0..rows {
                        for dc in 0..cols {
                            let v = self.eval_streaming_cell(&spec, dr, dc, ctx, depth)?;
                            f(v)?;
                        }
                    }
                }
            } else {
                let v = self.eval_arg_val(arg, ctx, depth)?;
                f(v)?;
            }
        }
        Ok(())
    }

    pub(crate) fn eval_range_vec(
        &self,
        expr: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Vec<EvaluatedCellValue>, FormulaError> {
        self.expand_args_ctx(std::slice::from_ref(expr), ctx, depth)
    }
}

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
            EvaluatedCellValue::Text(s) => Self::compile_text(s.trim()),
        }
    }

    /// Compiles a text criterion (`">=5"`, `"abc*"`, `"2026-01-01"`, ...).
    fn compile_text(trimmed: &str) -> Self {
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
            .find_map(|(op_str, op_kind)| trimmed.strip_prefix(op_str).map(|rhs| (*op_kind, rhs)))
        {
            return Self::compile_operator_rhs(op_kind, rhs.trim());
        }
        if let Some(rhs_num) = crate::coercion::parse_strict_number(trimmed) {
            return Self::Number(rhs_num);
        }
        if let Some(rhs_serial) = parse_iso_date_serial(trimmed) {
            return Self::IsoDateEq(rhs_serial);
        }
        if Self::has_wildcard_meta(trimmed) {
            Self::Wildcard(trimmed.to_string())
        } else {
            Self::TextEq(trimmed.to_lowercase())
        }
    }

    /// Compiles the right-hand side of an operator criterion.
    fn compile_operator_rhs(op_kind: CritNumOp, rhs_trim: &str) -> Self {
        if let Some(rhs_num) = crate::coercion::parse_strict_number(rhs_trim) {
            return Self::NumericCmp(op_kind, rhs_num);
        }
        if let Some(rhs_serial) = parse_iso_date_serial(rhs_trim) {
            return Self::IsoDateCmp(op_kind, rhs_serial);
        }
        let is_eq = matches!(op_kind, CritNumOp::Eq);
        if (is_eq || matches!(op_kind, CritNumOp::Ne)) && Self::has_wildcard_meta(rhs_trim) {
            return if is_eq {
                Self::Wildcard(rhs_trim.to_string())
            } else {
                Self::NotWildcard(rhs_trim.to_string())
            };
        }
        let rhs_lower = rhs_trim.to_lowercase();
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

/// Result shape of an element-wise operation: a 1x1 operand broadcasts to the other shape.
fn broadcast_shape(l: (usize, usize), r: (usize, usize)) -> Result<(usize, usize), FormulaError> {
    if l == (1, 1) {
        Ok(r)
    } else if r == (1, 1) || l == r {
        Ok(l)
    } else {
        Err(FormulaError::Value)
    }
}

/// Element `idx` of a grid, or its only element when the grid is a scalar.
#[inline]
fn broadcast_item(vals: &[EvaluatedCellValue], idx: usize) -> &EvaluatedCellValue {
    if vals.len() == 1 {
        &vals[0]
    } else {
        &vals[idx]
    }
}

impl SheetEngine {
    fn eval_expr_2d_grid(
        &self,
        expr: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<(usize, usize, Vec<EvaluatedCellValue>, bool), FormulaError> {
        if let Some(spec) = self.extract_streaming_range_ctx(expr, ctx, depth)? {
            let (_, _, _, rows, cols, _) = spec;
            if rows.saturating_mul(cols) > Self::MAX_EVAL_STEPS {
                return Err(FormulaError::Value);
            }
            let mut vals =
                Vec::with_capacity(rows.saturating_mul(cols).min(CellRange::MAX_EXPANDED_CELLS));
            for dr in 0..rows {
                for dc in 0..cols {
                    vals.push(self.eval_streaming_cell(&spec, dr, dc, ctx, depth)?);
                }
            }
            return Ok((rows, cols, vals, false));
        }
        match expr {
            Expr::UnaryPlus(inner) | Expr::UnaryMinus(inner) => {
                let is_neg = matches!(expr, Expr::UnaryMinus(_));
                let (r, c, vals, _) = self.eval_expr_2d_grid(inner, ctx, depth + 1)?;
                let out = vals
                    .into_iter()
                    .map(|v| {
                        if let EvaluatedCellValue::Error(e) = v {
                            return Err(e);
                        }
                        let n = v.as_number().ok_or(FormulaError::Value)?;
                        Ok(EvaluatedCellValue::Number(if is_neg { -n } else { n }))
                    })
                    .collect::<Result<Vec<_>, FormulaError>>()?;
                Ok((r, c, out, true))
            }
            Expr::Binary { op, lhs, rhs } => {
                let (r1, c1, v1, _) = self.eval_expr_2d_grid(lhs, ctx, depth + 1)?;
                let (r2, c2, v2, _) = self.eval_expr_2d_grid(rhs, ctx, depth + 1)?;
                let (rows, cols) = broadcast_shape((r1, c1), (r2, c2))?;
                let total = rows.saturating_mul(cols);
                let mut out = Vec::with_capacity(total);
                for idx in 0..total {
                    let lv = broadcast_item(&v1, idx).clone();
                    let rv = broadcast_item(&v2, idx).clone();
                    let res = Self::apply_binary_op(*op, lv, rv);
                    if let EvaluatedCellValue::Error(e) = res {
                        return Err(e);
                    }
                    out.push(res);
                }
                Ok((rows, cols, out, true))
            }
            _ => {
                let v = self.eval_arg_val(expr, ctx, depth)?;
                Ok((1, 1, vec![v], false))
            }
        }
    }

    pub(crate) fn eval_sumproduct_streaming(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if args.is_empty() {
            return Err(FormulaError::Value);
        }
        if args.len() == 2 {
            if let (Some(first_spec), Some(second_spec)) = (
                self.extract_streaming_range_ctx(&args[0], ctx, depth)?,
                self.extract_streaming_range_ctx(&args[1], ctx, depth)?,
            ) {
                let (_, _, _, r_count, c_count, _) = first_spec;
                if second_spec.3 != r_count || second_spec.4 != c_count {
                    return Err(FormulaError::Value);
                }
                let mut acc0 = 0.0f64;
                let mut acc1 = 0.0f64;
                let mut acc2 = 0.0f64;
                let mut acc3 = 0.0f64;
                let mut lane = 0usize;
                for dr in 0..r_count {
                    for dc in 0..c_count {
                        let a = match self.eval_streaming_cell(&first_spec, dr, dc, ctx, depth)? {
                            EvaluatedCellValue::Number(n) => n,
                            _ => 0.0,
                        };
                        let b = match self.eval_streaming_cell(&second_spec, dr, dc, ctx, depth)? {
                            EvaluatedCellValue::Number(n) => n,
                            _ => 0.0,
                        };
                        let p = a * b;
                        match lane & 3 {
                            0 => acc0 += p,
                            1 => acc1 += p,
                            2 => acc2 += p,
                            _ => acc3 += p,
                        }
                        lane += 1;
                    }
                }
                return Ok(EvaluatedCellValue::Number((acc0 + acc1) + (acc2 + acc3)));
            }
        }
        let mut grids = Vec::with_capacity(args.len());
        let mut expected_dims: Option<(usize, usize)> = None;
        for arg in args {
            let (r, c, vals, is_comp) = self.eval_expr_2d_grid(arg, ctx, depth)?;
            if let Some(exp) = expected_dims {
                if (r, c) != exp {
                    return Err(FormulaError::Value);
                }
            } else {
                expected_dims = Some((r, c));
            }
            grids.push((vals, is_comp));
        }
        let (r_count, c_count) = expected_dims.unwrap_or((0, 0));
        let total = r_count.saturating_mul(c_count);
        let mut sum = 0.0f64;
        for idx in 0..total {
            let mut row_prod = 1.0f64;
            for (vals, is_comp) in &grids {
                let n = match &vals[idx] {
                    EvaluatedCellValue::Number(n) => *n,
                    EvaluatedCellValue::Boolean(b) if *is_comp => u8::from(*b) as f64,
                    _ => 0.0,
                };
                row_prod *= n;
            }
            sum += row_prod;
        }
        Ok(EvaluatedCellValue::Number(sum))
    }

    /// Populated coordinates inside the anchored block when the block is large and sparse.
    fn sparse_block_population(
        &self,
        sheet: Option<&str>,
        origin: (u32, u32),
        counts: (usize, usize),
        ctx: &mut EvalContext<'_>,
    ) -> Option<Vec<PackedCellCoordinate>> {
        let (min_r, min_c) = origin;
        let (r_count, c_count) = counts;
        let total = r_count.saturating_mul(c_count);
        if total <= SPARSE_BLOCK_MIN_CELLS {
            return None;
        }
        let ts = self
            .resolve_sheet_ref(sheet, ctx)
            .filter(|ts| total > ts.cells.len().saturating_mul(2))?;
        let max_r = min_r + (r_count as u32).saturating_sub(1);
        let max_c = min_c + (c_count as u32).saturating_sub(1);
        Some(
            ts.cells
                .keys()
                .copied()
                .filter(|c| {
                    c.row() >= min_r && c.row() <= max_r && c.col() >= min_c && c.col() <= max_c
                })
                .collect(),
        )
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
        let sparse = if transposed {
            None
        } else {
            self.sparse_block_population(
                sheet_opt.as_deref(),
                (min_r, min_c),
                (r_count, c_count),
                ctx,
            )
        };
        let mut cnt = 0usize;
        if let Some(pop_coords) = sparse {
            let pop_len = pop_coords.len();
            for coord in pop_coords {
                let val = self.eval_sheet_coord(sheet_opt.as_deref(), coord, ctx, depth);
                cnt += usize::from(compiled_crit.matches(&val));
            }
            if compiled_crit.matches(&EvaluatedCellValue::Empty) {
                cnt += total.saturating_sub(pop_len);
            }
            return Ok(EvaluatedCellValue::Number(cnt as f64));
        }
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
        if args.len() < 2 || args.len() > 3 {
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
        let anchor_spec = target_spec.as_ref().unwrap_or(&range_spec);
        let sparse = if transposed || anchor_spec.5 {
            None
        } else {
            self.sparse_block_population(
                anchor_spec.0.as_deref(),
                (anchor_spec.1, anchor_spec.2),
                (r_count, c_count),
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
        if let Some(mut pop_coords) = sparse {
            pop_coords.sort_unstable();
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
}

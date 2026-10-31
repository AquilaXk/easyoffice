use crate::functions_date_fin::{parse_iso_date_serial, wildcard_match};
use crate::{
    CellRange, EvalContext, EvaluatedCellValue, Expr, FormulaError, PackedCellCoordinate,
    SheetEngine, WorkbookEngine,
};
use std::borrow::Cow;

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

impl SheetEngine {
    #[inline]
    fn has_sheet_in_ctx(&self, sheet: &str, ctx: &EvalContext<'_>) -> bool {
        WorkbookEngine::is_same_sheet_identity(sheet, &self.name)
            || ctx.workbook.and_then(|wb| wb.sheet(sheet)).is_some()
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
                if let Some((lhs_raw, rhs_raw)) = split_once_unquoted_colon(trimmed)? {
                    let (lhs_sh, start_coord) = parse_indirect_endpoint(lhs_raw)?;
                    let (rhs_sh, end_coord) = parse_indirect_endpoint(rhs_raw)?;
                    if let Some(ref rs) = rhs_sh {
                        let Some(ref ls) = lhs_sh else {
                            return Err(FormulaError::Ref);
                        };
                        if !WorkbookEngine::is_same_sheet_identity(ls, rs) {
                            return Err(FormulaError::Ref);
                        }
                    }
                    let range =
                        CellRange::new(start_coord, end_coord).map_err(|_| FormulaError::Ref)?;
                    if let Some(sh_clean) = lhs_sh {
                        if sh_clean.is_empty() || !self.has_sheet_in_ctx(&sh_clean, ctx) {
                            return Err(FormulaError::Ref);
                        }
                        Ok(Some((Some(Cow::Owned(sh_clean)), range, false)))
                    } else {
                        Ok(Some((None, range, false)))
                    }
                } else {
                    let (sh_opt, coord) = parse_indirect_endpoint(trimmed)?;
                    let range = CellRange::new(coord, coord).map_err(|_| FormulaError::Ref)?;
                    if let Some(sh_clean) = sh_opt {
                        if sh_clean.is_empty() || !self.has_sheet_in_ctx(&sh_clean, ctx) {
                            return Err(FormulaError::Ref);
                        }
                        Ok(Some((Some(Cow::Owned(sh_clean)), range, false)))
                    } else {
                        Ok(Some((None, range, false)))
                    }
                }
            }
            Expr::FuncCall { name, args } if name == "OFFSET" => {
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
                let height = if args.len() >= 4 {
                    self.eval_arg_num(&args[3], ctx, depth + 1)?.trunc() as i64
                } else {
                    base_rows as i64
                };
                let width = if args.len() == 5 {
                    self.eval_arg_num(&args[4], ctx, depth + 1)?.trunc() as i64
                } else {
                    base_cols as i64
                };
                if height < 1 || width < 1 {
                    return Err(FormulaError::Value);
                }
                let start_r = base_range.start.row().min(base_range.end.row()) as i64 + dr;
                let start_c = base_range.start.col().min(base_range.end.col()) as i64 + dc;
                if start_r < 0 || start_c < 0 {
                    return Err(FormulaError::Ref);
                }
                let end_r = start_r + height - 1;
                let end_c = start_c + width - 1;
                let start_coord = PackedCellCoordinate::new(start_r as u32, start_c as u32)
                    .map_err(|_| FormulaError::Ref)?;
                let end_coord = PackedCellCoordinate::new(end_r as u32, end_c as u32)
                    .map_err(|_| FormulaError::Ref)?;
                let range =
                    CellRange::new(start_coord, end_coord).map_err(|_| FormulaError::Ref)?;
                Ok(Some((sheet_opt, range, false)))
            }
            _ => Ok(None),
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
        if range.cell_count() > CellRange::MAX_EXPANDED_CELLS {
            return Err(FormulaError::Ref);
        }
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
                let (_, _, _, rows, cols, _) = spec;
                for dr in 0..rows {
                    for dc in 0..cols {
                        let v = self.eval_streaming_cell(&spec, dr, dc, ctx, depth)?;
                        f(v)?;
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
enum CritNumOp {
    Ge,
    Le,
    Ne,
    Gt,
    Lt,
    Eq,
}

enum CompiledCriteria {
    Number(f64),
    NumericCmp(CritNumOp, f64),
    IsoDateEq(f64),
    IsoDateCmp(CritNumOp, f64),
    TextEq(String),
    TextNe(String),
    TextOrd(CritNumOp, String),
    Wildcard(String),
}

impl CompiledCriteria {
    #[inline]
    fn compile(crit: &EvaluatedCellValue) -> Self {
        match crit {
            EvaluatedCellValue::Number(cn) => Self::Number(*cn),
            EvaluatedCellValue::Empty => Self::TextEq(String::new()),
            EvaluatedCellValue::Boolean(b) => {
                Self::TextEq(if *b { "true" } else { "false" }.to_string())
            }
            EvaluatedCellValue::Error(e) => Self::TextEq(e.code().to_lowercase()),
            EvaluatedCellValue::Text(s) => {
                let trimmed = s.trim();
                for (op_str, op_kind) in [
                    (">=", CritNumOp::Ge),
                    ("<=", CritNumOp::Le),
                    ("<>", CritNumOp::Ne),
                    (">", CritNumOp::Gt),
                    ("<", CritNumOp::Lt),
                    ("=", CritNumOp::Eq),
                ] {
                    if let Some(rhs) = trimmed.strip_prefix(op_str) {
                        let rhs_trim = rhs.trim();
                        if let Ok(rhs_num) = rhs_trim.parse::<f64>() {
                            return Self::NumericCmp(op_kind, rhs_num);
                        }
                        if let Some(rhs_serial) = parse_iso_date_serial(rhs_trim) {
                            return Self::IsoDateCmp(op_kind, rhs_serial);
                        }
                        let rhs_lower = rhs_trim.to_lowercase();
                        return match op_kind {
                            CritNumOp::Eq => Self::TextEq(rhs_lower),
                            CritNumOp::Ne => Self::TextNe(rhs_lower),
                            _ => Self::TextOrd(op_kind, rhs_lower),
                        };
                    }
                }
                if let Ok(rhs_num) = trimmed.parse::<f64>() {
                    return Self::Number(rhs_num);
                }
                if let Some(rhs_serial) = parse_iso_date_serial(trimmed) {
                    return Self::IsoDateEq(rhs_serial);
                }
                if trimmed.contains('*') || trimmed.contains('?') || trimmed.contains('~') {
                    Self::Wildcard(trimmed.to_string())
                } else {
                    Self::TextEq(trimmed.to_lowercase())
                }
            }
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
    fn cmp_f64(op: CritNumOp, lhs_num: f64, rhs_num: f64) -> bool {
        match op {
            CritNumOp::Ge => lhs_num >= rhs_num - 1e-12,
            CritNumOp::Le => lhs_num <= rhs_num + 1e-12,
            CritNumOp::Ne => (lhs_num - rhs_num).abs() >= 1e-12,
            CritNumOp::Gt => lhs_num > rhs_num,
            CritNumOp::Lt => lhs_num < rhs_num,
            CritNumOp::Eq => (lhs_num - rhs_num).abs() < 1e-12,
        }
    }

    #[inline]
    fn matches(&self, cell: &EvaluatedCellValue) -> bool {
        if let EvaluatedCellValue::Error(e) = cell {
            return match *self {
                Self::TextEq(ref rhs_lower) => Self::str_eq_lowercase(e.code(), rhs_lower),
                Self::TextNe(ref rhs_lower) => !Self::str_eq_lowercase(e.code(), rhs_lower),
                Self::NumericCmp(CritNumOp::Ne, _) | Self::IsoDateCmp(CritNumOp::Ne, _) => true,
                _ => false,
            };
        }
        match *self {
            Self::Number(cn) => cell.as_number().is_some_and(|v| (v - cn).abs() < 1e-9),
            Self::NumericCmp(op, rhs_num) => {
                let Some(lhs_num) = cell.as_number() else {
                    return matches!(op, CritNumOp::Ne);
                };
                Self::cmp_f64(op, lhs_num, rhs_num)
            }
            Self::IsoDateEq(rhs_serial) => Self::cell_iso_date_serial(cell)
                .is_some_and(|lhs_num| (lhs_num - rhs_serial).abs() < 1e-9),
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
        }
    }
}

impl SheetEngine {
    pub(crate) fn eval_sumproduct_streaming(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if args.is_empty() {
            return Err(FormulaError::Value);
        }
        let Some(first_spec) = self.extract_streaming_range_ctx(&args[0], ctx, depth)? else {
            let mut prod = 1.0f64;
            for arg in args {
                if self.extract_streaming_range_ctx(arg, ctx, depth)?.is_some() {
                    return Err(FormulaError::Value);
                }
                prod *= self
                    .eval_arg_val(arg, ctx, depth)?
                    .as_number()
                    .unwrap_or(0.0);
            }
            return Ok(EvaluatedCellValue::Number(prod));
        };
        let (_, _, _, r_count, c_count, _) = first_spec;
        if args.len() == 2 {
            if let Some(second_spec) = self.extract_streaming_range_ctx(&args[1], ctx, depth)? {
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
        for arg in &args[1..] {
            let Some((_, _, _, r, c, _)) = self.extract_streaming_range_ctx(arg, ctx, depth)?
            else {
                if r_count != 1 || c_count != 1 {
                    return Err(FormulaError::Value);
                }
                continue;
            };
            if r != r_count || c != c_count {
                return Err(FormulaError::Value);
            }
        }
        let mut sum = 0.0f64;
        for dr in 0..r_count {
            for dc in 0..c_count {
                let mut row_prod = 1.0f64;
                for arg in args {
                    let v = if let Some(sp) = self.extract_streaming_range_ctx(arg, ctx, depth)? {
                        self.eval_streaming_cell(&sp, dr, dc, ctx, depth)?
                    } else {
                        self.eval_arg_val(arg, ctx, depth)?
                    };
                    let n = match v {
                        EvaluatedCellValue::Number(n) => n,
                        _ => 0.0,
                    };
                    row_prod *= n;
                }
                sum += row_prod;
            }
        }
        Ok(EvaluatedCellValue::Number(sum))
    }

    pub(crate) fn eval_single_criteria_streaming(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if name == "COUNTIF" {
            if args.len() != 2 {
                return Err(FormulaError::Value);
            }
            let Some(range_spec) = self.extract_streaming_range_ctx(&args[0], ctx, depth)? else {
                let val = self.eval_expr_ctx(&args[0], ctx, depth);
                let crit = self.eval_arg_val(&args[1], ctx, depth)?;
                let compiled_crit = CompiledCriteria::compile(&crit);
                return Ok(EvaluatedCellValue::Number(if compiled_crit.matches(&val) {
                    1.0
                } else {
                    0.0
                }));
            };
            let crit_val = self.eval_arg_val(&args[1], ctx, depth)?;
            let compiled_crit = CompiledCriteria::compile(&crit_val);
            let (_, _, _, r_count, c_count, _) = range_spec;
            let mut cnt = 0usize;
            for dr in 0..r_count {
                for dc in 0..c_count {
                    let val =
                        self.eval_streaming_cell_allow_error(&range_spec, dr, dc, ctx, depth)?;
                    if compiled_crit.matches(&val) {
                        cnt += 1;
                    }
                }
            }
            return Ok(EvaluatedCellValue::Number(cnt as f64));
        }

        if args.len() < 2 || args.len() > 3 {
            return Err(FormulaError::Value);
        }
        let Some(range_spec) = self.extract_streaming_range_ctx(&args[0], ctx, depth)? else {
            return Err(FormulaError::Value);
        };
        let crit_val = self.eval_arg_val(&args[1], ctx, depth)?;
        let compiled_crit = CompiledCriteria::compile(&crit_val);
        let (_, _, _, r_count, c_count, _) = range_spec;
        let target_spec = if args.len() == 3 {
            let Some(ts) = self.extract_streaming_range_ctx(&args[2], ctx, depth)? else {
                return Err(FormulaError::Value);
            };
            if ts.3 != r_count || ts.4 != c_count {
                return Err(FormulaError::Value);
            }
            Some(ts)
        } else {
            None
        };

        let mut sum = 0.0f64;
        let mut count = 0usize;
        for dr in 0..r_count {
            for dc in 0..c_count {
                let val = self.eval_streaming_cell_allow_error(&range_spec, dr, dc, ctx, depth)?;
                if compiled_crit.matches(&val) {
                    let target_v = if let Some(ref ts) = target_spec {
                        self.eval_streaming_cell(ts, dr, dc, ctx, depth)?
                    } else {
                        val
                    };
                    if let EvaluatedCellValue::Number(n) = target_v {
                        sum += n;
                        count += 1;
                    }
                }
            }
        }

        if name == "AVERAGEIF" {
            if count == 0 {
                return Err(FormulaError::DivByZero);
            }
            Ok(EvaluatedCellValue::Number(sum / (count as f64)))
        } else {
            Ok(EvaluatedCellValue::Number(sum))
        }
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
        let compiled_crits: Vec<CompiledCriteria> =
            crit_vals.iter().map(CompiledCriteria::compile).collect();

        let mut sum = 0.0f64;
        let mut count = 0usize;
        let mut min_val: Option<f64> = None;
        let mut max_val: Option<f64> = None;
        for dr in 0..r_count {
            'cell_loop: for dc in 0..c_count {
                for (sp, cc) in specs.iter().zip(compiled_crits.iter()) {
                    let val = self.eval_streaming_cell_allow_error(sp, dr, dc, ctx, depth)?;
                    if !cc.matches(&val) {
                        continue 'cell_loop;
                    }
                }
                if let Some(ref ts) = target_spec {
                    let tv = self.eval_streaming_cell(ts, dr, dc, ctx, depth)?;
                    if let EvaluatedCellValue::Number(n) = tv {
                        sum += n;
                        count += 1;
                        min_val = Some(min_val.map_or(n, |v| v.min(n)));
                        max_val = Some(max_val.map_or(n, |v| v.max(n)));
                    }
                } else {
                    count += 1;
                }
            }
        }

        match name {
            "COUNTIFS" => Ok(EvaluatedCellValue::Number(count as f64)),
            "AVERAGEIFS" => {
                if count == 0 {
                    Err(FormulaError::DivByZero)
                } else {
                    Ok(EvaluatedCellValue::Number(sum / (count as f64)))
                }
            }
            "MINIFS" => Ok(EvaluatedCellValue::Number(min_val.unwrap_or(0.0))),
            "MAXIFS" => Ok(EvaluatedCellValue::Number(max_val.unwrap_or(0.0))),
            _ => Ok(EvaluatedCellValue::Number(sum)),
        }
    }

    pub(crate) fn eval_subtotal(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if args.len() < 2 {
            return Err(FormulaError::Value);
        }
        let fn_code = self.eval_arg_num(&args[0], ctx, depth)?.trunc() as i32;
        let base_code = if (101..=111).contains(&fn_code) {
            fn_code - 100
        } else {
            fn_code
        };
        if !(1..=11).contains(&base_code) {
            return Err(FormulaError::Value);
        }

        let mut sum = 0.0f64;
        let mut product = 1.0f64;
        let mut num_count = 0usize;
        let mut non_empty_count = 0usize;
        let mut min_val: Option<f64> = None;
        let mut max_val: Option<f64> = None;
        let mut mean = 0.0f64;
        let mut m2 = 0.0f64;

        let mut fold_val = |v: EvaluatedCellValue| -> Result<(), FormulaError> {
            match v {
                EvaluatedCellValue::Error(e) => return Err(e),
                EvaluatedCellValue::Empty => {}
                EvaluatedCellValue::Number(n) => {
                    non_empty_count += 1;
                    sum += n;
                    product *= n;
                    num_count += 1;
                    min_val = Some(min_val.map_or(n, |m| m.min(n)));
                    max_val = Some(max_val.map_or(n, |m| m.max(n)));
                    let delta = n - mean;
                    mean += delta / (num_count as f64);
                    m2 += delta * (n - mean);
                }
                EvaluatedCellValue::Boolean(_) | EvaluatedCellValue::Text(_) => {
                    non_empty_count += 1;
                }
            }
            Ok(())
        };

        for arg in &args[1..] {
            if let Some(spec) = self.extract_streaming_range_ctx(arg, ctx, depth)? {
                let (ref sheet_opt, min_r, min_c, rows, cols, transposed) = spec;
                for dr in 0..rows {
                    for dc in 0..cols {
                        let (r, c) = if transposed {
                            (min_r + dc as u32, min_c + dr as u32)
                        } else {
                            (min_r + dr as u32, min_c + dc as u32)
                        };
                        let coord =
                            PackedCellCoordinate::new(r, c).map_err(|_| FormulaError::Ref)?;
                        if self.is_nested_subtotal_cell(sheet_opt.as_deref(), coord, ctx) {
                            continue;
                        }
                        let v = self.eval_sheet_coord(sheet_opt.as_deref(), coord, ctx, depth);
                        fold_val(v)?;
                    }
                }
            } else if !crate::expr_contains_subtotal(arg) {
                fold_val(self.eval_arg_val(arg, ctx, depth)?)?;
            }
        }

        match base_code {
            1 => {
                if num_count == 0 {
                    Err(FormulaError::DivByZero)
                } else {
                    Ok(EvaluatedCellValue::Number(sum / (num_count as f64)))
                }
            }
            2 => Ok(EvaluatedCellValue::Number(num_count as f64)),
            3 => Ok(EvaluatedCellValue::Number(non_empty_count as f64)),
            4 => Ok(EvaluatedCellValue::Number(max_val.unwrap_or(0.0))),
            5 => Ok(EvaluatedCellValue::Number(min_val.unwrap_or(0.0))),
            6 => Ok(EvaluatedCellValue::Number(if num_count == 0 {
                0.0
            } else {
                product
            })),
            7 => {
                if num_count < 2 {
                    Err(FormulaError::DivByZero)
                } else {
                    Ok(EvaluatedCellValue::Number(
                        (m2 / ((num_count - 1) as f64)).sqrt(),
                    ))
                }
            }
            8 => {
                if num_count == 0 {
                    Err(FormulaError::DivByZero)
                } else {
                    Ok(EvaluatedCellValue::Number((m2 / (num_count as f64)).sqrt()))
                }
            }
            9 => Ok(EvaluatedCellValue::Number(sum)),
            10 => {
                if num_count < 2 {
                    Err(FormulaError::DivByZero)
                } else {
                    Ok(EvaluatedCellValue::Number(m2 / ((num_count - 1) as f64)))
                }
            }
            11 => {
                if num_count == 0 {
                    Err(FormulaError::DivByZero)
                } else {
                    Ok(EvaluatedCellValue::Number(m2 / (num_count as f64)))
                }
            }
            _ => Err(FormulaError::Value),
        }
    }
}

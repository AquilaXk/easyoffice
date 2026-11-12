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
        if !(3..=5).contains(&args.len()) {
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
                    let pop_coords = self
                        .resolve_sheet_ref(sheet_opt.as_deref(), ctx)
                        .unwrap()
                        .cell_index()
                        .range_coords(min_r, max_r, min_c, max_c);
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
            } else if let Some(m) = self.matrix_expr(arg, ctx, depth)? {
                for idx in 0..m.data.len() {
                    f(m.value_at(idx))?;
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
    pub(crate) fn eval_expr_2d_grid(
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
                if let Some(m) = self.matrix_expr(expr, ctx, depth)? {
                    let vals = (0..m.data.len()).map(|i| m.value_at(i)).collect();
                    return Ok((m.rows, m.cols, vals, false));
                }
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
}

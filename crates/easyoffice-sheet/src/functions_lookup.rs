use crate::range_refs::StreamingRangeSpec;
use crate::{
    EvalContext, EvaluatedCellValue, Expr, FormulaError, PackedCellCoordinate, SheetEngine,
};

type CellResult = Result<EvaluatedCellValue, FormulaError>;

/// Lookup values only compete for approximate matches when they share a type.
fn same_value_type(a: &EvaluatedCellValue, b: &EvaluatedCellValue) -> bool {
    matches!(
        (a, b),
        (EvaluatedCellValue::Number(_), EvaluatedCellValue::Number(_))
            | (EvaluatedCellValue::Text(_), EvaluatedCellValue::Text(_))
            | (
                EvaluatedCellValue::Boolean(_),
                EvaluatedCellValue::Boolean(_)
            )
    )
}

fn check_lookup_arity(args: &[Expr], min: usize, max: usize) -> Result<(), FormulaError> {
    if args.len() < min || args.len() > max {
        Err(FormulaError::Value)
    } else {
        Ok(())
    }
}

fn number_value(v: f64) -> CellResult {
    Ok(EvaluatedCellValue::Number(v))
}

fn packed(row: u32, col: u32) -> Result<PackedCellCoordinate, FormulaError> {
    PackedCellCoordinate::new(row, col).map_err(|_| FormulaError::Ref)
}

/// Position of a VLOOKUP / HLOOKUP table: key line and result line offsets.
struct TableGeometry {
    min_r: u32,
    min_c: u32,
    is_vertical: bool,
    target_idx: u32,
}

impl TableGeometry {
    /// Key cell and result cell coordinates for the `off`-th row (VLOOKUP) or column (HLOOKUP).
    fn coord_pair(
        &self,
        off: u32,
    ) -> Result<(PackedCellCoordinate, PackedCellCoordinate), FormulaError> {
        if self.is_vertical {
            Ok((
                packed(self.min_r + off, self.min_c)?,
                packed(self.min_r + off, self.min_c + self.target_idx)?,
            ))
        } else {
            Ok((
                packed(self.min_r, self.min_c + off)?,
                packed(self.min_r + self.target_idx, self.min_c + off)?,
            ))
        }
    }
}

/// Keeps the greatest key that is `<=` the lookup value (sorted approximate match).
fn update_best_floor(
    best: &mut Option<(u32, EvaluatedCellValue)>,
    off: u32,
    key: EvaluatedCellValue,
    lookup: &EvaluatedCellValue,
) {
    let Some(ord) = SheetEngine::compare_excel_values(&key, lookup) else {
        return;
    };
    if !ord.is_le() {
        return;
    }
    let replace = best.as_ref().is_none_or(|(_, prev)| {
        SheetEngine::compare_excel_values(&key, prev).is_some_and(|o| o.is_ge())
    });
    if replace {
        *best = Some((off, key));
    }
}

/// XLOOKUP match / search mode pair.
struct XlookupModes {
    match_mode: i32,
    search_mode: i32,
}

impl XlookupModes {
    fn cell_matches(&self, key: &EvaluatedCellValue, lookup: &EvaluatedCellValue) -> bool {
        if self.match_mode == 2 {
            SheetEngine::values_match_wildcard(key, lookup)
        } else {
            SheetEngine::values_match(key, lookup)
        }
    }

    /// Tracks the nearest smaller (-1) / larger (1) candidate for approximate matches.
    fn update_approx(
        &self,
        best: &mut Option<(usize, EvaluatedCellValue)>,
        idx: usize,
        key: &EvaluatedCellValue,
        lookup: &EvaluatedCellValue,
    ) {
        let mode = self.match_mode;
        if !(mode == -1 || mode == 1) || !same_value_type(key, lookup) {
            return;
        }
        let Some(ord) = SheetEngine::compare_excel_values(key, lookup) else {
            return;
        };
        let valid = (mode == -1 && ord.is_lt()) || (mode == 1 && ord.is_gt());
        if !valid {
            return;
        }
        let replace = best.as_ref().is_none_or(|(_, prev)| {
            SheetEngine::compare_excel_values(key, prev).is_some_and(|o| {
                if mode == -1 {
                    o.is_gt()
                } else {
                    o.is_lt()
                }
            })
        });
        if replace {
            *best = Some((idx, key.clone()));
        }
    }

    #[inline]
    fn scan_index(&self, step: usize, total_len: usize) -> usize {
        if self.search_mode < 0 {
            total_len - 1 - step
        } else {
            step
        }
    }
}

/// MATCH candidate tracking for approximate (1 / -1) modes.
struct MatchState {
    match_type: i32,
    best: Option<(usize, EvaluatedCellValue)>,
}

impl MatchState {
    /// Returns true on an exact (wildcard) hit for `match_type == 0`.
    fn consider(
        &mut self,
        idx: usize,
        v: &EvaluatedCellValue,
        lookup: &EvaluatedCellValue,
    ) -> bool {
        if self.match_type == 0 {
            return SheetEngine::values_match_wildcard(v, lookup);
        }
        if !same_value_type(v, lookup) {
            return false;
        }
        let Some(ord) = SheetEngine::compare_excel_values(v, lookup) else {
            return false;
        };
        let better = if self.match_type > 0 && ord.is_le() {
            self.beats_best(v, |o| o.is_ge())
        } else if self.match_type < 0 && ord.is_ge() {
            self.beats_best(v, |o| o.is_le())
        } else {
            false
        };
        if better {
            self.best = Some((idx, v.clone()));
        }
        false
    }

    fn beats_best(&self, v: &EvaluatedCellValue, accept: fn(std::cmp::Ordering) -> bool) -> bool {
        self.best
            .as_ref()
            .is_none_or(|(_, prev)| SheetEngine::compare_excel_values(v, prev).is_some_and(accept))
    }

    fn result(&self) -> CellResult {
        match &self.best {
            Some((best_idx, _)) => number_value(*best_idx as f64),
            None => Err(FormulaError::Na),
        }
    }
}

/// Applies INDEX's single-row / single-column shortcuts to the requested position.
fn effective_index(
    arg_count: usize,
    (row_num, col_num): (isize, isize),
    (rows, cols): (usize, usize),
) -> (isize, isize) {
    if arg_count == 2 && rows == 1 && cols > 1 && row_num >= 1 && (row_num as usize) <= cols {
        (1, row_num)
    } else if arg_count == 3 && row_num == 0 && col_num >= 1 && (col_num as usize) <= cols {
        (1, col_num)
    } else if arg_count == 3 && col_num == 0 && row_num >= 1 && (row_num as usize) <= rows {
        (row_num, 1)
    } else {
        (row_num, col_num)
    }
}

impl SheetEngine {
    pub(crate) fn eval_lookup_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        match name {
            "VLOOKUP" | "HLOOKUP" => self.lookup_table(name == "VLOOKUP", args, ctx, depth),
            "XLOOKUP" => self.lookup_x(args, ctx, depth),
            "INDEX" => self.lookup_index(args, ctx, depth),
            "MATCH" => self.lookup_match(args, ctx, depth),
            "CHOOSE" => self.lookup_choose(args, ctx, depth),
            "OFFSET" | "INDIRECT" => self.lookup_reference(name, args, ctx, depth),
            "ROW" | "COLUMN" => self.lookup_row_column(name == "ROW", args, ctx, depth),
            "ROWS" | "COLUMNS" => self.lookup_rows_columns(name == "ROWS", args, ctx, depth),
            "TRANSPOSE" => self.lookup_transpose(args, ctx, depth),
            _ => Err(FormulaError::Name),
        }
    }

    fn lookup_table(
        &self,
        is_vertical: bool,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        check_lookup_arity(args, 3, 4)?;
        let lookup_val = self.eval_arg_val(&args[0], ctx, depth)?;
        let Some((sheet_opt, range, _)) = self.extract_range_and_sheet_ctx(&args[1], ctx, depth)?
        else {
            return Err(FormulaError::Value);
        };
        let idx_num = self.eval_arg_num(&args[2], ctx, depth)?.trunc() as isize;
        let range_lookup = match args.get(3) {
            Some(arg) => self
                .eval_arg_val(arg, ctx, depth)?
                .as_bool()
                .ok_or(FormulaError::Value)?,
            None => true,
        };
        let (rows, cols) = range.dimensions();
        let (outer_count, bound_limit) = if is_vertical {
            (rows, cols)
        } else {
            (cols, rows)
        };
        if idx_num < 1 || (idx_num as usize) > bound_limit {
            return Err(FormulaError::Ref);
        }
        let geom = TableGeometry {
            min_r: range.start.row().min(range.end.row()),
            min_c: range.start.col().min(range.end.col()),
            is_vertical,
            target_idx: idx_num as u32 - 1,
        };
        let sheet = sheet_opt.as_deref();
        let mut best: Option<(u32, EvaluatedCellValue)> = None;
        for off in 0..outer_count {
            let (kc, rc) = geom.coord_pair(off as u32)?;
            let kv = self.eval_sheet_coord(sheet, kc, ctx, depth);
            if !range_lookup {
                if Self::values_match_wildcard(&kv, &lookup_val) {
                    return Ok(self.eval_sheet_coord(sheet, rc, ctx, depth));
                }
            } else if same_value_type(&kv, &lookup_val) {
                update_best_floor(&mut best, off as u32, kv, &lookup_val);
            }
        }
        let Some((best_off, _)) = best else {
            return Err(FormulaError::Na);
        };
        let (_, rc) = geom.coord_pair(best_off)?;
        Ok(self.eval_sheet_coord(sheet, rc, ctx, depth))
    }

    /// Optional numeric XLOOKUP mode argument (missing or omitted gives `default`).
    fn xlookup_mode(
        &self,
        args: &[Expr],
        idx: usize,
        default: i32,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<i32, FormulaError> {
        match args.get(idx) {
            Some(arg) if !matches!(arg, Expr::Missing) => {
                Ok(self.eval_arg_num(arg, ctx, depth)?.trunc() as i32)
            }
            _ => Ok(default),
        }
    }

    /// The `if_not_found` result: argument 4 when present, otherwise `#N/A`.
    fn xlookup_not_found(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        match args.get(3) {
            Some(arg) if !matches!(arg, Expr::Missing) => Ok(self.eval_expr_ctx(arg, ctx, depth)),
            _ => Err(FormulaError::Na),
        }
    }

    fn lookup_x(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        check_lookup_arity(args, 3, 6)?;
        let lookup_val = self.eval_arg_val(&args[0], ctx, depth)?;
        let match_mode = self.xlookup_mode(args, 4, 0, ctx, depth)?;
        if !matches!(match_mode, -1..=2) {
            return Err(FormulaError::Value);
        }
        let search_mode = self.xlookup_mode(args, 5, 1, ctx, depth)?;
        if !matches!(search_mode, -2 | -1 | 1 | 2) {
            return Err(FormulaError::Value);
        }
        let modes = XlookupModes {
            match_mode,
            search_mode,
        };
        let specs = (
            self.extract_streaming_range_ctx(&args[1], ctx, depth)?,
            self.extract_streaming_range_ctx(&args[2], ctx, depth)?,
        );
        if let (Some(lookup_spec), Some(return_spec)) = specs {
            return self.xlookup_streaming(
                &modes,
                &lookup_val,
                (&lookup_spec, &return_spec),
                args,
                ctx,
                depth,
            );
        }
        let lookup_arr = self.eval_range_vec(&args[1], ctx, depth)?;
        let return_arr = self.eval_range_vec(&args[2], ctx, depth)?;
        if lookup_arr.len() != return_arr.len() {
            return Err(FormulaError::Value);
        }
        let total_len = lookup_arr.len();
        let mut best_approx: Option<(usize, EvaluatedCellValue)> = None;
        for step in 0..total_len {
            let idx = modes.scan_index(step, total_len);
            let k = &lookup_arr[idx];
            if modes.cell_matches(k, &lookup_val) {
                return Ok(return_arr[idx].clone());
            }
            modes.update_approx(&mut best_approx, idx, k, &lookup_val);
        }
        match best_approx {
            Some((best_idx, _)) => Ok(return_arr[best_idx].clone()),
            None => self.xlookup_not_found(args, ctx, depth),
        }
    }

    fn xlookup_streaming(
        &self,
        modes: &XlookupModes,
        lookup_val: &EvaluatedCellValue,
        specs: (&StreamingRangeSpec<'_>, &StreamingRangeSpec<'_>),
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        let (lookup_spec, return_spec) = specs;
        let (_, _, _, l_rows, l_cols, _) = *lookup_spec;
        let (_, _, _, r_rows, r_cols, _) = *return_spec;
        let total_len = l_rows * l_cols;
        if total_len != r_rows * r_cols || r_cols == 0 || l_cols == 0 {
            return Err(FormulaError::Value);
        }
        let mut best_approx: Option<(usize, EvaluatedCellValue)> = None;
        for step in 0..total_len {
            let flat_idx = modes.scan_index(step, total_len);
            let k = self.eval_streaming_cell_allow_error(
                lookup_spec,
                flat_idx / l_cols,
                flat_idx % l_cols,
                ctx,
                depth,
            )?;
            if modes.cell_matches(&k, lookup_val) {
                return self.eval_streaming_cell(
                    return_spec,
                    flat_idx / r_cols,
                    flat_idx % r_cols,
                    ctx,
                    depth,
                );
            }
            modes.update_approx(&mut best_approx, flat_idx, &k, lookup_val);
        }
        match best_approx {
            Some((best_idx, _)) => self.eval_streaming_cell(
                return_spec,
                best_idx / r_cols,
                best_idx % r_cols,
                ctx,
                depth,
            ),
            None => self.xlookup_not_found(args, ctx, depth),
        }
    }

    fn lookup_index(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        check_lookup_arity(args, 2, 3)?;
        let Some((sheet_opt, range, transposed)) =
            self.extract_range_and_sheet_ctx(&args[0], ctx, depth)?
        else {
            return Err(FormulaError::Value);
        };
        let row_num = self.eval_arg_num(&args[1], ctx, depth)?.trunc() as isize;
        let col_num = match args.get(2) {
            Some(arg) => self.eval_arg_num(arg, ctx, depth)?.trunc() as isize,
            None => 1,
        };
        let (orig_rows, orig_cols) = range.dimensions();
        let (rows, cols) = if transposed {
            (orig_cols, orig_rows)
        } else {
            (orig_rows, orig_cols)
        };
        let min_r = range.start.row().min(range.end.row());
        let min_c = range.start.col().min(range.end.col());
        let (eff_r, eff_c) = effective_index(args.len(), (row_num, col_num), (rows, cols));
        if eff_r < 1 || eff_c < 1 || (eff_r as usize) > rows || (eff_c as usize) > cols {
            return Err(FormulaError::Ref);
        }
        let (r_off, c_off) = if transposed {
            (eff_c as u32 - 1, eff_r as u32 - 1)
        } else {
            (eff_r as u32 - 1, eff_c as u32 - 1)
        };
        let coord = packed(min_r + r_off, min_c + c_off)?;
        Ok(self.eval_sheet_coord(sheet_opt.as_deref(), coord, ctx, depth))
    }

    fn lookup_match(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        check_lookup_arity(args, 2, 3)?;
        let lookup_val = self.eval_arg_val(&args[0], ctx, depth)?;
        let match_type = match args.get(2) {
            Some(arg) => self.eval_arg_num(arg, ctx, depth)?.trunc() as i32,
            None => 1,
        };
        let mut state = MatchState {
            match_type,
            best: None,
        };
        if let Some(spec) = self.extract_streaming_range_ctx(&args[1], ctx, depth)? {
            let (_, _, _, rows, cols, _) = spec;
            let mut idx = 0usize;
            for r_off in 0..rows {
                for c_off in 0..cols {
                    idx += 1;
                    let v =
                        self.eval_streaming_cell_allow_error(&spec, r_off, c_off, ctx, depth)?;
                    if state.consider(idx, &v, &lookup_val) {
                        return number_value(idx as f64);
                    }
                }
            }
            return state.result();
        }
        let arr = self.eval_range_vec(&args[1], ctx, depth)?;
        for (zero_idx, v) in arr.iter().enumerate() {
            if state.consider(zero_idx + 1, v, &lookup_val) {
                return number_value((zero_idx + 1) as f64);
            }
        }
        state.result()
    }

    fn lookup_choose(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        check_lookup_arity(args, 2, usize::MAX)?;
        let idx = self.eval_arg_num(&args[0], ctx, depth)?.trunc() as isize;
        if idx < 1 || (idx as usize) >= args.len() {
            return Err(FormulaError::Value);
        }
        Ok(self.eval_expr_ctx(&args[idx as usize], ctx, depth))
    }

    fn lookup_reference(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        let expr = Expr::FuncCall {
            name: name.to_string(),
            args: args.to_vec(),
        };
        let Some((sheet_opt, range, _)) = self.extract_range_and_sheet_ctx(&expr, ctx, depth)?
        else {
            return Err(FormulaError::Value);
        };
        if range.start != range.end {
            return Err(FormulaError::Value);
        }
        Ok(self.eval_sheet_coord(sheet_opt.as_deref(), range.start, ctx, depth))
    }

    fn lookup_row_column(
        &self,
        is_row: bool,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        if args.is_empty() {
            let v = ctx
                .current_coord
                .map_or(1, |c| if is_row { c.row() + 1 } else { c.col() + 1 });
            return number_value(v as f64);
        }
        check_lookup_arity(args, 1, 1)?;
        let Some((_, range, _)) = self.extract_range_and_sheet_ctx(&args[0], ctx, depth)? else {
            return Err(FormulaError::Value);
        };
        let v = if is_row {
            range.start.row().min(range.end.row()) + 1
        } else {
            range.start.col().min(range.end.col()) + 1
        };
        number_value(v as f64)
    }

    fn lookup_rows_columns(
        &self,
        is_rows: bool,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        check_lookup_arity(args, 1, 1)?;
        let Some((_, range, transposed)) =
            self.extract_range_and_sheet_ctx(&args[0], ctx, depth)?
        else {
            let _ = self.eval_arg_val(&args[0], ctx, depth)?;
            return number_value(1.0);
        };
        let (r, c) = range.dimensions();
        number_value(match (is_rows, transposed) {
            (true, false) | (false, true) => r,
            (true, true) | (false, false) => c,
        } as f64)
    }

    fn lookup_transpose(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        check_lookup_arity(args, 1, 1)?;
        let mut vals = self.eval_range_vec(&args[0], ctx, depth)?;
        if vals.len() == 1 {
            return Ok(vals.swap_remove(0));
        }
        let joined = vals
            .iter()
            .map(EvaluatedCellValue::to_display_string)
            .collect::<Vec<_>>()
            .join(",");
        Ok(EvaluatedCellValue::Text(joined))
    }
}

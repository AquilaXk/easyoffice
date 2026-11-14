pub(crate) use crate::lookup_index::{
    check_lookup_arity, number_value, packed, same_value_type, sorted_approx_binary_search,
    update_best_floor, CellResult, MatchState, SliceAxis, TableGeometry, XlookupModes,
};
use crate::range_refs::StreamingRangeSpec;
use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};

/// Applies INDEX's single-row / single-column shortcuts to the requested position.
pub(crate) fn effective_index(
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
            "XMATCH" => self.lookup_xmatch(args, ctx, depth),
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
        let Some((sheet_opt, range, transposed)) =
            self.extract_range_and_sheet_ctx(&args[1], ctx, depth)?
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
        if !range_lookup && !transposed && outer_count > 0 {
            let (axis, fixed, start) = if is_vertical {
                (SliceAxis::Col, geom.min_c, geom.min_r)
            } else {
                (SliceAxis::Row, geom.min_r, geom.min_c)
            };
            if let Some(hit) = self.exact_lookup_slice(
                sheet,
                axis,
                fixed,
                start,
                start + outer_count as u32 - 1,
                &lookup_val,
                true,
                false,
                ctx,
                depth,
            )? {
                return match hit {
                    Some(off) => {
                        let (_, rc) = geom.coord_pair(off)?;
                        Ok(self.eval_sheet_coord(sheet, rc, ctx, depth))
                    }
                    None => Err(FormulaError::Na),
                };
            }
        }
        if range_lookup && outer_count > 0 {
            let bsearch = sorted_approx_binary_search(outer_count, true, &lookup_val, |off| {
                let (kc, _) = geom.coord_pair(off as u32)?;
                Ok(self.eval_sheet_coord(sheet, kc, ctx, depth))
            })?;
            if let Ok(hit) = bsearch {
                let Some(best_off) = hit else {
                    return Err(FormulaError::Na);
                };
                let (_, rc) = geom.coord_pair(best_off as u32)?;
                return Ok(self.eval_sheet_coord(sheet, rc, ctx, depth));
            }
        }
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
        if modes.search_mode.abs() == 2 {
            return match modes
                .binary_search(total_len, &lookup_val, |i| Ok(lookup_arr[i].clone()))?
            {
                Some(idx) => Ok(return_arr[idx].clone()),
                None => self.xlookup_not_found(args, ctx, depth),
            };
        }
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

    /// `XMATCH(lookup, array, [match_mode], [search_mode])`: the 1-based position of the match,
    /// with the same match and search modes as `XLOOKUP`.
    fn lookup_xmatch(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        check_lookup_arity(args, 2, 4)?;
        let lookup_val = self.eval_arg_val(&args[0], ctx, depth)?;
        let match_mode = self.xlookup_mode(args, 2, 0, ctx, depth)?;
        let search_mode = self.xlookup_mode(args, 3, 1, ctx, depth)?;
        if !matches!(match_mode, -1..=2) || !matches!(search_mode, -2 | -1 | 1 | 2) {
            return Err(FormulaError::Value);
        }
        let modes = XlookupModes {
            match_mode,
            search_mode,
        };
        if (match_mode == 0 || match_mode == 2) && search_mode.abs() == 1 {
            if let Some((ref sheet_opt, min_r, min_c, rows, cols, transposed)) =
                self.extract_streaming_range_ctx(&args[1], ctx, depth)?
            {
                if !transposed && rows > 0 && cols > 0 && (rows == 1 || cols == 1) {
                    let (axis, fixed, start, len) = if cols == 1 {
                        (SliceAxis::Col, min_c, min_r, rows as u32)
                    } else {
                        (SliceAxis::Row, min_r, min_c, cols as u32)
                    };
                    if let Some(hit) = self.exact_lookup_slice(
                        sheet_opt.as_deref(),
                        axis,
                        fixed,
                        start,
                        start + len - 1,
                        &lookup_val,
                        match_mode == 2,
                        search_mode < 0,
                        ctx,
                        depth,
                    )? {
                        return match hit {
                            Some(off) => number_value((off + 1) as f64),
                            None => Err(FormulaError::Na),
                        };
                    }
                }
            }
        }
        let arr = self.eval_range_vec(&args[1], ctx, depth)?;
        let hit = if search_mode.abs() == 2 {
            modes.binary_search(arr.len(), &lookup_val, |i| Ok(arr[i].clone()))?
        } else {
            let mut best_approx: Option<(usize, EvaluatedCellValue)> = None;
            let exact = (0..arr.len())
                .map(|step| modes.scan_index(step, arr.len()))
                .find(|&idx| {
                    let hit = modes.cell_matches(&arr[idx], &lookup_val);
                    if !hit {
                        modes.update_approx(&mut best_approx, idx, &arr[idx], &lookup_val);
                    }
                    hit
                });
            exact.or(best_approx.map(|(idx, _)| idx))
        };
        match hit {
            Some(idx) => number_value((idx + 1) as f64),
            None => Err(FormulaError::Na),
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
        let (ref l_sheet_opt, l_min_r, l_min_c, l_rows, l_cols, l_transposed) = *lookup_spec;
        if (modes.match_mode == 0 || modes.match_mode == 2)
            && modes.search_mode.abs() == 1
            && !l_transposed
            && (l_rows == 1 || l_cols == 1)
        {
            let (axis, fixed, start, len) = if l_cols == 1 {
                (SliceAxis::Col, l_min_c, l_min_r, l_rows as u32)
            } else {
                (SliceAxis::Row, l_min_r, l_min_c, l_cols as u32)
            };
            if let Some(hit) = self.exact_lookup_slice(
                l_sheet_opt.as_deref(),
                axis,
                fixed,
                start,
                start + len - 1,
                lookup_val,
                modes.match_mode == 2,
                modes.search_mode < 0,
                ctx,
                depth,
            )? {
                return match hit {
                    Some(off) => {
                        let off = off as usize;
                        self.eval_streaming_cell(
                            return_spec,
                            off / r_cols,
                            off % r_cols,
                            ctx,
                            depth,
                        )
                    }
                    None => self.xlookup_not_found(args, ctx, depth),
                };
            }
        }
        if modes.search_mode.abs() == 2 {
            let hit = modes.binary_search(total_len, lookup_val, |flat_idx| {
                self.eval_streaming_cell_allow_error(
                    lookup_spec,
                    flat_idx / l_cols,
                    flat_idx % l_cols,
                    ctx,
                    depth,
                )
            })?;
            return match hit {
                Some(idx) => {
                    self.eval_streaming_cell(return_spec, idx / r_cols, idx % r_cols, ctx, depth)
                }
                None => self.xlookup_not_found(args, ctx, depth),
            };
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
        if let Some(matrix) = self.matrix_expr(&args[0], ctx, depth)? {
            return self.index_matrix(&matrix, args, ctx, depth);
        }
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
            let (ref sheet_opt, min_r, min_c, rows, cols, transposed) = spec;
            let total_len = rows.saturating_mul(cols);
            if match_type == 0 && !transposed && total_len > 0 && (rows == 1 || cols == 1) {
                let (axis, fixed, start, len) = if cols == 1 {
                    (SliceAxis::Col, min_c, min_r, rows as u32)
                } else {
                    (SliceAxis::Row, min_r, min_c, cols as u32)
                };
                if let Some(hit) = self.exact_lookup_slice(
                    sheet_opt.as_deref(),
                    axis,
                    fixed,
                    start,
                    start + len - 1,
                    &lookup_val,
                    true,
                    false,
                    ctx,
                    depth,
                )? {
                    return match hit {
                        Some(off) => number_value((off + 1) as f64),
                        None => Err(FormulaError::Na),
                    };
                }
            }
            if match_type != 0 && total_len > 0 && cols > 0 {
                let bsearch = sorted_approx_binary_search(
                    total_len,
                    match_type > 0,
                    &lookup_val,
                    |flat_idx| {
                        self.eval_streaming_cell_allow_error(
                            &spec,
                            flat_idx / cols,
                            flat_idx % cols,
                            ctx,
                            depth,
                        )
                    },
                )?;
                if let Ok(hit) = bsearch {
                    return match hit {
                        Some(zero_idx) => number_value((zero_idx + 1) as f64),
                        None => Err(FormulaError::Na),
                    };
                }
            }
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
        if match_type != 0 && !arr.is_empty() {
            let bsearch =
                sorted_approx_binary_search(arr.len(), match_type > 0, &lookup_val, |idx| {
                    Ok(arr[idx].clone())
                })?;
            if let Ok(hit) = bsearch {
                return match hit {
                    Some(zero_idx) => number_value((zero_idx + 1) as f64),
                    None => Err(FormulaError::Na),
                };
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xlookup_binary_search_ascending_and_descending() {
        let mut s = SheetEngine::new("Sheet1");
        for (i, (asc, desc, ret)) in [
            ("10", "50", "100"),
            ("20", "40", "200"),
            ("30", "30", "300"),
            ("40", "20", "400"),
            ("50", "10", "500"),
        ]
        .into_iter()
        .enumerate()
        {
            let r = i + 1;
            s.set_cell(&format!("A{r}"), asc).unwrap();
            s.set_cell(&format!("B{r}"), desc).unwrap();
            s.set_cell(&format!("C{r}"), ret).unwrap();
        }
        assert_eq!(
            s.evaluate_formula_str("=XLOOKUP(30, A1:A5, C1:C5, -1, 0, 2)"),
            EvaluatedCellValue::Number(300.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=XLOOKUP(25, A1:A5, C1:C5, -1, -1, 2)"),
            EvaluatedCellValue::Number(200.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=XLOOKUP(25, A1:A5, C1:C5, -1, 1, 2)"),
            EvaluatedCellValue::Number(300.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=XLOOKUP(20, B1:B5, C1:C5, -1, 0, -2)"),
            EvaluatedCellValue::Number(400.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=XLOOKUP(25, B1:B5, C1:C5, -1, -1, -2)"),
            EvaluatedCellValue::Number(400.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=XLOOKUP(25, B1:B5, C1:C5, -1, 1, -2)"),
            EvaluatedCellValue::Number(300.0)
        );
    }

    #[test]
    fn binary_search_skips_error_cells_in_either_order() {
        let mut s = SheetEngine::new("Sheet1");
        for (i, (asc, desc)) in [
            ("10", "50"),
            ("20", "40"),
            ("=NA()", "=NA()"),
            ("40", "20"),
            ("50", "10"),
        ]
        .into_iter()
        .enumerate()
        {
            s.set_cell(&format!("A{}", i + 1), asc).unwrap();
            s.set_cell(&format!("B{}", i + 1), desc).unwrap();
            s.set_cell(&format!("C{}", i + 1), (i + 1).to_string())
                .unwrap();
        }
        assert_eq!(
            s.evaluate_formula_str("=XLOOKUP(10, A1:A5, C1:C5, -1, 0, 2)"),
            EvaluatedCellValue::Number(1.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=XLOOKUP(50, A1:A5, C1:C5, -1, 0, 2)"),
            EvaluatedCellValue::Number(5.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=XLOOKUP(50, B1:B5, C1:C5, -1, 0, -2)"),
            EvaluatedCellValue::Number(1.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=XLOOKUP(10, B1:B5, C1:C5, -1, 0, -2)"),
            EvaluatedCellValue::Number(5.0)
        );
    }

    #[test]
    fn binary_search_returns_the_first_of_duplicate_keys() {
        let mut s = SheetEngine::new("Sheet1");
        for (i, (k, v)) in [(10, 100), (20, 200), (20, 201), (20, 202), (30, 300)]
            .into_iter()
            .enumerate()
        {
            s.set_cell(&format!("A{}", i + 1), k.to_string()).unwrap();
            s.set_cell(&format!("B{}", i + 1), (60 - k).to_string())
                .unwrap();
            s.set_cell(&format!("C{}", i + 1), v.to_string()).unwrap();
        }
        assert_eq!(
            s.evaluate_formula_str("=XLOOKUP(20, A1:A5, C1:C5, -1, 0, 2)"),
            EvaluatedCellValue::Number(200.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=XMATCH(20, A1:A5, 0, 2)"),
            EvaluatedCellValue::Number(2.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=XMATCH(40, B1:B5, 0, -2)"),
            EvaluatedCellValue::Number(2.0)
        );
    }

    #[test]
    fn match_and_vlookup_binary_search_with_fallback() {
        let mut s = SheetEngine::new("Sheet1");
        for (i, (asc, desc, ret)) in [
            ("10", "50", "100"),
            ("20", "40", "200"),
            ("20", "40", "250"),
            ("30", "30", "300"),
            ("50", "10", "500"),
        ]
        .into_iter()
        .enumerate()
        {
            let r = i + 1;
            s.set_cell(&format!("A{r}"), asc).unwrap();
            s.set_cell(&format!("B{r}"), desc).unwrap();
            s.set_cell(&format!("C{r}"), ret).unwrap();
        }
        assert_eq!(
            s.evaluate_formula_str("=MATCH(25, A1:A5, 1)"),
            EvaluatedCellValue::Number(3.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=MATCH(25, B1:B5, -1)"),
            EvaluatedCellValue::Number(4.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=VLOOKUP(20, A1:C5, 3, TRUE)"),
            EvaluatedCellValue::Number(250.0)
        );
        // Mixed-type cell at midpoint triggers fallback to linear scan.
        s.set_cell("A3", "label").unwrap();
        assert_eq!(
            s.evaluate_formula_str("=MATCH(25, A1:A5, 1)"),
            EvaluatedCellValue::Number(2.0)
        );
    }
}

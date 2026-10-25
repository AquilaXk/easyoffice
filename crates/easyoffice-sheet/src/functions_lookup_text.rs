use crate::{
    EvalContext, EvaluatedCellValue, Expr, FormulaError, PackedCellCoordinate, SheetEngine,
};

impl SheetEngine {
    pub(crate) fn eval_lookup_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        match name {
            "VLOOKUP" | "HLOOKUP" => {
                if args.len() < 3 || args.len() > 4 {
                    return Err(FormulaError::Value);
                }
                let lookup_val = self.eval_arg_val(&args[0], ctx, depth)?;
                let Some((sheet_opt, range, _)) = self.extract_range_and_sheet(&args[1]) else {
                    return Err(FormulaError::Value);
                };
                let idx_num = self.eval_arg_num(&args[2], ctx, depth)?.trunc() as isize;
                let range_lookup = if args.len() == 4 {
                    self.eval_arg_val(&args[3], ctx, depth)?
                        .as_bool()
                        .ok_or(FormulaError::Value)?
                } else {
                    true
                };
                let (rows, cols) = range.dimensions();
                let min_r = range.start.row().min(range.end.row());
                let min_c = range.start.col().min(range.end.col());
                let same_type = |a: &EvaluatedCellValue, b: &EvaluatedCellValue| -> bool {
                    matches!(
                        (a, b),
                        (EvaluatedCellValue::Number(_), EvaluatedCellValue::Number(_))
                            | (EvaluatedCellValue::Text(_), EvaluatedCellValue::Text(_))
                            | (
                                EvaluatedCellValue::Boolean(_),
                                EvaluatedCellValue::Boolean(_)
                            )
                    )
                };
                if name == "VLOOKUP" {
                    if idx_num < 1 || (idx_num as usize) > cols {
                        return Err(FormulaError::Ref);
                    }
                    let target_c = min_c + (idx_num as u32 - 1);
                    let mut best: Option<(u32, EvaluatedCellValue)> = None;
                    for r_off in 0..rows {
                        let kc = PackedCellCoordinate::new(min_r + r_off as u32, min_c)
                            .map_err(|_| FormulaError::Ref)?;
                        let kv = self.eval_sheet_coord(sheet_opt, kc, ctx, depth + 1);
                        if !range_lookup {
                            if Self::values_match(&kv, &lookup_val) {
                                let rc = PackedCellCoordinate::new(min_r + r_off as u32, target_c)
                                    .map_err(|_| FormulaError::Ref)?;
                                return Ok(self.eval_sheet_coord(sheet_opt, rc, ctx, depth + 1));
                            }
                        } else if same_type(&kv, &lookup_val) {
                            if let Some(ord) = Self::compare_excel_values(&kv, &lookup_val) {
                                if ord.is_le() {
                                    let replace = match &best {
                                        None => true,
                                        Some((_, prev)) => Self::compare_excel_values(&kv, prev)
                                            .is_some_and(|o| o.is_ge()),
                                    };
                                    if replace {
                                        best = Some((r_off as u32, kv));
                                    }
                                }
                            }
                        }
                    }
                    if let Some((best_r_off, _)) = best {
                        let rc = PackedCellCoordinate::new(min_r + best_r_off, target_c)
                            .map_err(|_| FormulaError::Ref)?;
                        return Ok(self.eval_sheet_coord(sheet_opt, rc, ctx, depth + 1));
                    }
                } else {
                    if idx_num < 1 || (idx_num as usize) > rows {
                        return Err(FormulaError::Ref);
                    }
                    let target_r = min_r + (idx_num as u32 - 1);
                    let mut best: Option<(u32, EvaluatedCellValue)> = None;
                    for c_off in 0..cols {
                        let kc = PackedCellCoordinate::new(min_r, min_c + c_off as u32)
                            .map_err(|_| FormulaError::Ref)?;
                        let kv = self.eval_sheet_coord(sheet_opt, kc, ctx, depth + 1);
                        if !range_lookup {
                            if Self::values_match(&kv, &lookup_val) {
                                let rc = PackedCellCoordinate::new(target_r, min_c + c_off as u32)
                                    .map_err(|_| FormulaError::Ref)?;
                                return Ok(self.eval_sheet_coord(sheet_opt, rc, ctx, depth + 1));
                            }
                        } else if same_type(&kv, &lookup_val) {
                            if let Some(ord) = Self::compare_excel_values(&kv, &lookup_val) {
                                if ord.is_le() {
                                    let replace = match &best {
                                        None => true,
                                        Some((_, prev)) => Self::compare_excel_values(&kv, prev)
                                            .is_some_and(|o| o.is_ge()),
                                    };
                                    if replace {
                                        best = Some((c_off as u32, kv));
                                    }
                                }
                            }
                        }
                    }
                    if let Some((best_c_off, _)) = best {
                        let rc = PackedCellCoordinate::new(target_r, min_c + best_c_off)
                            .map_err(|_| FormulaError::Ref)?;
                        return Ok(self.eval_sheet_coord(sheet_opt, rc, ctx, depth + 1));
                    }
                }
                Err(FormulaError::Na)
            }
            "XLOOKUP" => {
                if args.len() < 3 || args.len() > 6 {
                    return Err(FormulaError::Value);
                }
                let lookup_val = self.eval_arg_val(&args[0], ctx, depth)?;
                if let (Some(lookup_spec), Some(return_spec)) = (
                    self.extract_streaming_range(&args[1])?,
                    self.extract_streaming_range(&args[2])?,
                ) {
                    let (_, _, _, l_rows, l_cols, _) = lookup_spec;
                    let (_, _, _, r_rows, r_cols, _) = return_spec;
                    if l_rows * l_cols != r_rows * r_cols || r_cols == 0 {
                        return Err(FormulaError::Value);
                    }
                    let mut flat_idx = 0usize;
                    for r_off in 0..l_rows {
                        for c_off in 0..l_cols {
                            let k = self.eval_streaming_cell_allow_error(
                                lookup_spec,
                                r_off,
                                c_off,
                                ctx,
                                depth,
                            )?;
                            if Self::values_match(&k, &lookup_val) {
                                return self.eval_streaming_cell(
                                    return_spec,
                                    flat_idx / r_cols,
                                    flat_idx % r_cols,
                                    ctx,
                                    depth,
                                );
                            }
                            flat_idx += 1;
                        }
                    }
                    if args.len() >= 4 {
                        return Ok(self.eval_expr_ctx(&args[3], ctx, depth + 1));
                    } else {
                        return Err(FormulaError::Na);
                    }
                }
                let lookup_arr = self.eval_range_vec(&args[1], ctx, depth)?;
                let return_arr = self.eval_range_vec(&args[2], ctx, depth)?;
                if lookup_arr.len() != return_arr.len() {
                    return Err(FormulaError::Value);
                }
                for (k, v) in lookup_arr.iter().zip(return_arr) {
                    if Self::values_match(k, &lookup_val) {
                        return Ok(v);
                    }
                }
                if args.len() >= 4 {
                    Ok(self.eval_expr_ctx(&args[3], ctx, depth + 1))
                } else {
                    Err(FormulaError::Na)
                }
            }
            "INDEX" => {
                if args.len() < 2 || args.len() > 3 {
                    return Err(FormulaError::Value);
                }
                let Some((sheet_opt, range, transposed)) = self.extract_range_and_sheet(&args[0])
                else {
                    return Err(FormulaError::Value);
                };
                let row_num = self.eval_arg_num(&args[1], ctx, depth)?.trunc() as isize;
                let col_num = if args.len() == 3 {
                    self.eval_arg_num(&args[2], ctx, depth)?.trunc() as isize
                } else {
                    1
                };
                let (orig_rows, orig_cols) = range.dimensions();
                let (rows, cols) = if transposed {
                    (orig_cols, orig_rows)
                } else {
                    (orig_rows, orig_cols)
                };
                if row_num < 1
                    || col_num < 1
                    || (row_num as usize) > rows
                    || (col_num as usize) > cols
                {
                    return Err(FormulaError::Ref);
                }
                let min_r = range.start.row().min(range.end.row());
                let min_c = range.start.col().min(range.end.col());
                let (r_off, c_off) = if transposed {
                    (col_num as u32 - 1, row_num as u32 - 1)
                } else {
                    (row_num as u32 - 1, col_num as u32 - 1)
                };
                let coord = PackedCellCoordinate::new(min_r + r_off, min_c + c_off)
                    .map_err(|_| FormulaError::Ref)?;
                Ok(self.eval_sheet_coord(sheet_opt, coord, ctx, depth + 1))
            }
            "MATCH" => {
                if args.len() < 2 || args.len() > 3 {
                    return Err(FormulaError::Value);
                }
                let lookup_val = self.eval_arg_val(&args[0], ctx, depth)?;
                let match_type = if args.len() == 3 {
                    self.eval_arg_num(&args[2], ctx, depth)?.trunc() as i32
                } else {
                    1
                };
                let same_type = |a: &EvaluatedCellValue, b: &EvaluatedCellValue| -> bool {
                    matches!(
                        (a, b),
                        (EvaluatedCellValue::Number(_), EvaluatedCellValue::Number(_))
                            | (EvaluatedCellValue::Text(_), EvaluatedCellValue::Text(_))
                            | (
                                EvaluatedCellValue::Boolean(_),
                                EvaluatedCellValue::Boolean(_)
                            )
                    )
                };
                if let Some(spec) = self.extract_streaming_range(&args[1])? {
                    let (_, _, _, rows, cols, _) = spec;
                    let mut idx = 0usize;
                    let mut best: Option<(usize, EvaluatedCellValue)> = None;
                    for r_off in 0..rows {
                        for c_off in 0..cols {
                            idx += 1;
                            let v = self
                                .eval_streaming_cell_allow_error(spec, r_off, c_off, ctx, depth)?;
                            if match_type == 0 {
                                if Self::values_match(&v, &lookup_val) {
                                    return Ok(EvaluatedCellValue::Number(idx as f64));
                                }
                            } else if same_type(&v, &lookup_val) {
                                if let Some(ord) = Self::compare_excel_values(&v, &lookup_val) {
                                    if match_type > 0 && ord.is_le() {
                                        let replace = match &best {
                                            None => true,
                                            Some((_, prev)) => Self::compare_excel_values(&v, prev)
                                                .is_some_and(|o| o.is_ge()),
                                        };
                                        if replace {
                                            best = Some((idx, v));
                                        }
                                    } else if match_type < 0 && ord.is_ge() {
                                        let replace = match &best {
                                            None => true,
                                            Some((_, prev)) => Self::compare_excel_values(&v, prev)
                                                .is_some_and(|o| o.is_le()),
                                        };
                                        if replace {
                                            best = Some((idx, v));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if let Some((best_idx, _)) = best {
                        return Ok(EvaluatedCellValue::Number(best_idx as f64));
                    }
                    return Err(FormulaError::Na);
                }
                let arr = self.eval_range_vec(&args[1], ctx, depth)?;
                for (idx, v) in arr.iter().enumerate() {
                    if Self::values_match(v, &lookup_val) {
                        return Ok(EvaluatedCellValue::Number((idx + 1) as f64));
                    }
                }
                Err(FormulaError::Na)
            }
            "CHOOSE" => {
                if args.len() < 2 {
                    return Err(FormulaError::Value);
                }
                let idx = self.eval_arg_num(&args[0], ctx, depth)?.trunc() as isize;
                if idx < 1 || (idx as usize) >= args.len() {
                    return Err(FormulaError::Value);
                }
                Ok(self.eval_expr_ctx(&args[idx as usize], ctx, depth + 1))
            }
            "OFFSET" => {
                if args.len() < 3 || args.len() > 5 {
                    return Err(FormulaError::Value);
                }
                let Some((sheet_opt, range, _)) = self.extract_range_and_sheet(&args[0]) else {
                    return Err(FormulaError::Value);
                };
                let dr = self.eval_arg_num(&args[1], ctx, depth)?.trunc() as i64;
                let dc = self.eval_arg_num(&args[2], ctx, depth)?.trunc() as i64;
                let base_r = range.start.row().min(range.end.row()) as i64 + dr;
                let base_c = range.start.col().min(range.end.col()) as i64 + dc;
                if base_r < 0 || base_c < 0 {
                    return Err(FormulaError::Ref);
                }
                let coord = PackedCellCoordinate::new(base_r as u32, base_c as u32)
                    .map_err(|_| FormulaError::Ref)?;
                Ok(self.eval_sheet_coord(sheet_opt, coord, ctx, depth + 1))
            }
            "ROW" | "COLUMN" => {
                if args.is_empty() {
                    return Ok(EvaluatedCellValue::Number(1.0));
                }
                if args.len() != 1 {
                    return Err(FormulaError::Value);
                }
                let Some((_, range, _)) = self.extract_range_and_sheet(&args[0]) else {
                    return Err(FormulaError::Value);
                };
                let v = if name == "ROW" {
                    range.start.row().min(range.end.row()) + 1
                } else {
                    range.start.col().min(range.end.col()) + 1
                };
                Ok(EvaluatedCellValue::Number(v as f64))
            }
            "ROWS" | "COLUMNS" => {
                if args.len() != 1 {
                    return Err(FormulaError::Value);
                }
                if let Some((_, range, transposed)) = self.extract_range_and_sheet(&args[0]) {
                    let (r, c) = range.dimensions();
                    let ans = match (name == "ROWS", transposed) {
                        (true, false) | (false, true) => r,
                        (true, true) | (false, false) => c,
                    };
                    Ok(EvaluatedCellValue::Number(ans as f64))
                } else {
                    let _ = self.eval_arg_val(&args[0], ctx, depth)?;
                    Ok(EvaluatedCellValue::Number(1.0))
                }
            }
            "TRANSPOSE" => {
                if args.len() != 1 {
                    return Err(FormulaError::Value);
                }
                let vals = self.eval_range_vec(&args[0], ctx, depth)?;
                if vals.len() == 1 {
                    Ok(vals.into_iter().next().unwrap())
                } else {
                    let joined = vals
                        .iter()
                        .map(EvaluatedCellValue::to_display_string)
                        .collect::<Vec<_>>()
                        .join(",");
                    Ok(EvaluatedCellValue::Text(joined))
                }
            }
            "INDIRECT" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(FormulaError::Value);
                }
                let ref_str = self.eval_arg_val(&args[0], ctx, depth)?.to_display_string();
                let trimmed = ref_str.trim();
                if let Some((sh, cell_part)) = trimmed.split_once('!') {
                    let sh_clean = sh.trim().trim_matches('\'');
                    let coord =
                        PackedCellCoordinate::parse_a1(cell_part).map_err(|_| FormulaError::Ref)?;
                    Ok(self.eval_sheet_coord(Some(sh_clean), coord, ctx, depth + 1))
                } else {
                    let coord =
                        PackedCellCoordinate::parse_a1(trimmed).map_err(|_| FormulaError::Ref)?;
                    Ok(self.eval_sheet_coord(None, coord, ctx, depth + 1))
                }
            }
            _ => Err(FormulaError::Name),
        }
    }

    pub(crate) fn eval_text_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        match name {
            "LEN" => {
                if args.len() != 1 {
                    return Err(FormulaError::Value);
                }
                let s = self.eval_arg_val(&args[0], ctx, depth)?.to_display_string();
                Ok(EvaluatedCellValue::Number(s.chars().count() as f64))
            }
            "CONCAT" | "CONCATENATE" => {
                let vals = self.expand_args_ctx(args, ctx, depth)?;
                let mut out = String::new();
                for v in vals {
                    out.push_str(&v.to_display_string());
                }
                Ok(EvaluatedCellValue::Text(out))
            }
            "TEXTJOIN" => {
                if args.len() < 3 {
                    return Err(FormulaError::Value);
                }
                let delim = self.eval_arg_val(&args[0], ctx, depth)?.to_display_string();
                let ignore_empty = self
                    .eval_arg_val(&args[1], ctx, depth)?
                    .as_bool()
                    .ok_or(FormulaError::Value)?;
                let vals = self.expand_args_ctx(&args[2..], ctx, depth)?;
                let items: Vec<String> = vals
                    .into_iter()
                    .map(|v| v.to_display_string())
                    .filter(|s| !ignore_empty || !s.is_empty())
                    .collect();
                Ok(EvaluatedCellValue::Text(items.join(&delim)))
            }
            "LEFT" | "RIGHT" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(FormulaError::Value);
                }
                let s: Vec<char> = self
                    .eval_arg_val(&args[0], ctx, depth)?
                    .to_display_string()
                    .chars()
                    .collect();
                let n = if args.len() == 2 {
                    let k = self.eval_arg_num(&args[1], ctx, depth)?.trunc() as isize;
                    if k < 0 {
                        return Err(FormulaError::Value);
                    }
                    k as usize
                } else {
                    1
                };
                let take = n.min(s.len());
                let res: String = if name == "LEFT" {
                    s[..take].iter().collect()
                } else {
                    s[s.len() - take..].iter().collect()
                };
                Ok(EvaluatedCellValue::Text(res))
            }
            "MID" => {
                if args.len() != 3 {
                    return Err(FormulaError::Value);
                }
                let s: Vec<char> = self
                    .eval_arg_val(&args[0], ctx, depth)?
                    .to_display_string()
                    .chars()
                    .collect();
                let start = self.eval_arg_num(&args[1], ctx, depth)?.trunc() as isize;
                let count = self.eval_arg_num(&args[2], ctx, depth)?.trunc() as isize;
                if start < 1 || count < 0 {
                    return Err(FormulaError::Value);
                }
                let st = ((start - 1) as usize).min(s.len());
                let end = (st + count as usize).min(s.len());
                Ok(EvaluatedCellValue::Text(s[st..end].iter().collect()))
            }
            "TRIM" | "UPPER" | "LOWER" | "PROPER" => {
                if args.len() != 1 {
                    return Err(FormulaError::Value);
                }
                let s = self.eval_arg_val(&args[0], ctx, depth)?.to_display_string();
                let out = match name {
                    "TRIM" => s.split_whitespace().collect::<Vec<_>>().join(" "),
                    "UPPER" => s.to_uppercase(),
                    "LOWER" => s.to_lowercase(),
                    _ => {
                        let mut res = String::with_capacity(s.len());
                        let mut cap_next = true;
                        for c in s.chars() {
                            if c.is_alphabetic() {
                                if cap_next {
                                    res.extend(c.to_uppercase());
                                    cap_next = false;
                                } else {
                                    res.extend(c.to_lowercase());
                                }
                            } else {
                                cap_next = true;
                                res.push(c);
                            }
                        }
                        res
                    }
                };
                Ok(EvaluatedCellValue::Text(out))
            }
            "SUBSTITUTE" => {
                if args.len() < 3 || args.len() > 4 {
                    return Err(FormulaError::Value);
                }
                let text = self.eval_arg_val(&args[0], ctx, depth)?.to_display_string();
                let old_t = self.eval_arg_val(&args[1], ctx, depth)?.to_display_string();
                let new_t = self.eval_arg_val(&args[2], ctx, depth)?.to_display_string();
                if old_t.is_empty() {
                    return Ok(EvaluatedCellValue::Text(text));
                }
                if args.len() == 4 {
                    let inst = self.eval_arg_num(&args[3], ctx, depth)?.trunc() as isize;
                    if inst < 1 {
                        return Err(FormulaError::Value);
                    }
                    let mut out = String::new();
                    let mut rem = text.as_str();
                    let mut count = 0isize;
                    while let Some(pos) = rem.find(&old_t) {
                        count += 1;
                        out.push_str(&rem[..pos]);
                        if count == inst {
                            out.push_str(&new_t);
                            out.push_str(&rem[pos + old_t.len()..]);
                            return Ok(EvaluatedCellValue::Text(out));
                        }
                        out.push_str(&old_t);
                        rem = &rem[pos + old_t.len()..];
                    }
                    out.push_str(rem);
                    Ok(EvaluatedCellValue::Text(out))
                } else {
                    Ok(EvaluatedCellValue::Text(text.replace(&old_t, &new_t)))
                }
            }
            "REPLACE" => {
                if args.len() != 4 {
                    return Err(FormulaError::Value);
                }
                let chars: Vec<char> = self
                    .eval_arg_val(&args[0], ctx, depth)?
                    .to_display_string()
                    .chars()
                    .collect();
                let start = self.eval_arg_num(&args[1], ctx, depth)?.trunc() as isize;
                let num_chars = self.eval_arg_num(&args[2], ctx, depth)?.trunc() as isize;
                let new_t = self.eval_arg_val(&args[3], ctx, depth)?.to_display_string();
                if start < 1 || num_chars < 0 {
                    return Err(FormulaError::Value);
                }
                let st = ((start - 1) as usize).min(chars.len());
                let end = (st + num_chars as usize).min(chars.len());
                let mut out: String = chars[..st].iter().collect();
                out.push_str(&new_t);
                out.extend(chars[end..].iter());
                Ok(EvaluatedCellValue::Text(out))
            }
            "FIND" | "SEARCH" => {
                if args.len() < 2 || args.len() > 3 {
                    return Err(FormulaError::Value);
                }
                let needle_raw = self.eval_arg_val(&args[0], ctx, depth)?.to_display_string();
                let hay_raw = self.eval_arg_val(&args[1], ctx, depth)?.to_display_string();
                let start = if args.len() == 3 {
                    self.eval_arg_num(&args[2], ctx, depth)?.trunc() as isize
                } else {
                    1
                };
                let hay_chars: Vec<char> = if name == "SEARCH" {
                    hay_raw.to_lowercase().chars().collect()
                } else {
                    hay_raw.chars().collect()
                };
                let needle_chars: Vec<char> = if name == "SEARCH" {
                    needle_raw.to_lowercase().chars().collect()
                } else {
                    needle_raw.chars().collect()
                };
                if start < 1 || (start as usize) > hay_chars.len().max(1) {
                    return Err(FormulaError::Value);
                }
                let st = (start - 1) as usize;
                if needle_chars.is_empty() {
                    return Ok(EvaluatedCellValue::Number(start as f64));
                }
                for idx in st..=hay_chars.len().saturating_sub(needle_chars.len()) {
                    if hay_chars[idx..].starts_with(&needle_chars) {
                        return Ok(EvaluatedCellValue::Number((idx + 1) as f64));
                    }
                }
                Err(FormulaError::Value)
            }
            "EXACT" => {
                if args.len() != 2 {
                    return Err(FormulaError::Value);
                }
                let a = self.eval_arg_val(&args[0], ctx, depth)?.to_display_string();
                let b = self.eval_arg_val(&args[1], ctx, depth)?.to_display_string();
                Ok(EvaluatedCellValue::Boolean(a == b))
            }
            "REPT" => {
                if args.len() != 2 {
                    return Err(FormulaError::Value);
                }
                let s = self.eval_arg_val(&args[0], ctx, depth)?.to_display_string();
                let n = self.eval_arg_num(&args[1], ctx, depth)?.trunc() as isize;
                if n < 0 || s.len().saturating_mul(n as usize) > 32_767 {
                    return Err(FormulaError::Value);
                }
                Ok(EvaluatedCellValue::Text(s.repeat(n as usize)))
            }
            "VALUE" => {
                if args.len() != 1 {
                    return Err(FormulaError::Value);
                }
                let v = self.eval_arg_val(&args[0], ctx, depth)?;
                if let Some(n) = v.as_number() {
                    return Ok(EvaluatedCellValue::Number(n));
                }
                let cleaned = v.to_display_string().trim().replace([',', '$'], "");
                if let Some(pct) = cleaned.strip_suffix('%') {
                    let n = pct.trim().parse::<f64>().map_err(|_| FormulaError::Value)?;
                    Ok(EvaluatedCellValue::Number(n / 100.0))
                } else {
                    let n = cleaned.parse::<f64>().map_err(|_| FormulaError::Value)?;
                    Ok(EvaluatedCellValue::Number(n))
                }
            }
            "TEXT" => {
                if args.len() != 2 {
                    return Err(FormulaError::Value);
                }
                let val = self.eval_arg_val(&args[0], ctx, depth)?;
                let fmt_str = self.eval_arg_val(&args[1], ctx, depth)?.to_display_string();
                if let Some(num) = val.as_number() {
                    if fmt_str.ends_with('%') {
                        let decimals = fmt_str
                            .split('.')
                            .nth(1)
                            .map_or(0, |d| d.trim_end_matches('%').len());
                        Ok(EvaluatedCellValue::Text(format!(
                            "{:.prec$}%",
                            num * 100.0,
                            prec = decimals
                        )))
                    } else if let Some((_, dec_part)) = fmt_str.split_once('.') {
                        let decimals = dec_part.chars().filter(|&c| c == '0' || c == '#').count();
                        Ok(EvaluatedCellValue::Text(format!(
                            "{num:.prec$}",
                            prec = decimals
                        )))
                    } else {
                        Ok(EvaluatedCellValue::Text(format!("{:.0}", num.round())))
                    }
                } else {
                    Ok(EvaluatedCellValue::Text(val.to_display_string()))
                }
            }
            "CHAR" => {
                if args.len() != 1 {
                    return Err(FormulaError::Value);
                }
                let code = self.eval_arg_num(&args[0], ctx, depth)?.trunc() as i64;
                if !(1..=1_114_111).contains(&code) {
                    return Err(FormulaError::Value);
                }
                let ch = char::from_u32(code as u32).ok_or(FormulaError::Value)?;
                Ok(EvaluatedCellValue::Text(ch.to_string()))
            }
            "CODE" => {
                if args.len() != 1 {
                    return Err(FormulaError::Value);
                }
                let s = self.eval_arg_val(&args[0], ctx, depth)?.to_display_string();
                let ch = s.chars().next().ok_or(FormulaError::Value)?;
                Ok(EvaluatedCellValue::Number((ch as u32) as f64))
            }
            _ => Err(FormulaError::Name),
        }
    }

    pub(crate) fn eval_logical_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        match name {
            "IF" => {
                if args.len() < 2 || args.len() > 3 {
                    return Err(FormulaError::Value);
                }
                let flag = self
                    .eval_arg_val(&args[0], ctx, depth)?
                    .as_bool()
                    .ok_or(FormulaError::Value)?;
                if flag {
                    Ok(self.eval_expr_ctx(&args[1], ctx, depth + 1))
                } else if args.len() == 3 {
                    Ok(self.eval_expr_ctx(&args[2], ctx, depth + 1))
                } else {
                    Ok(EvaluatedCellValue::Boolean(false))
                }
            }
            "IFS" => {
                if args.len() < 2 || !args.len().is_multiple_of(2) {
                    return Err(FormulaError::Value);
                }
                for pair in args.chunks(2) {
                    let cond = self.eval_arg_val(&pair[0], ctx, depth)?;
                    if cond.as_bool().ok_or(FormulaError::Value)? {
                        return Ok(self.eval_expr_ctx(&pair[1], ctx, depth + 1));
                    }
                }
                Err(FormulaError::Na)
            }
            "AND" | "OR" | "XOR" => {
                let vals = self.expand_args_ctx(args, ctx, depth)?;
                if vals.is_empty() {
                    return Err(FormulaError::Value);
                }
                let mut trues = 0usize;
                let mut seen = 0usize;
                for v in vals {
                    if matches!(v, EvaluatedCellValue::Empty) {
                        continue;
                    }
                    let b = v.as_bool().ok_or(FormulaError::Value)?;
                    seen += 1;
                    if b {
                        trues += 1;
                    }
                }
                if seen == 0 {
                    return Err(FormulaError::Value);
                }
                Ok(EvaluatedCellValue::Boolean(match name {
                    "AND" => trues == seen,
                    "OR" => trues > 0,
                    _ => trues % 2 == 1,
                }))
            }
            "NOT" => {
                if args.len() != 1 {
                    return Err(FormulaError::Value);
                }
                let b = self
                    .eval_arg_val(&args[0], ctx, depth)?
                    .as_bool()
                    .ok_or(FormulaError::Value)?;
                Ok(EvaluatedCellValue::Boolean(!b))
            }
            "IFERROR" | "IFNA" => {
                if args.len() != 2 {
                    return Err(FormulaError::Value);
                }
                let primary = self.eval_expr_ctx(&args[0], ctx, depth + 1);
                let catch = match primary {
                    EvaluatedCellValue::Error(FormulaError::Na) => true,
                    EvaluatedCellValue::Error(_) => name == "IFERROR",
                    _ => false,
                };
                Ok(if catch {
                    self.eval_expr_ctx(&args[1], ctx, depth + 1)
                } else {
                    primary
                })
            }
            "SWITCH" => {
                if args.len() < 3 {
                    return Err(FormulaError::Value);
                }
                let target = self.eval_arg_val(&args[0], ctx, depth)?;
                let rest = &args[1..];
                for i in 0..(rest.len() / 2) {
                    let k = self.eval_arg_val(&rest[i * 2], ctx, depth)?;
                    if Self::values_match(&target, &k) {
                        return Ok(self.eval_expr_ctx(&rest[i * 2 + 1], ctx, depth + 1));
                    }
                }
                if rest.len() % 2 == 1 {
                    Ok(self.eval_expr_ctx(rest.last().unwrap(), ctx, depth + 1))
                } else {
                    Err(FormulaError::Na)
                }
            }
            "TRUE" => Ok(EvaluatedCellValue::Boolean(true)),
            "FALSE" => Ok(EvaluatedCellValue::Boolean(false)),
            _ => Err(FormulaError::Name),
        }
    }
}

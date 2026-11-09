use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};

type CellResult = Result<EvaluatedCellValue, FormulaError>;

const MAX_CHAR_CODE: i64 = 1_114_111;
const MIN_PRINTABLE: u32 = 0x20;
const FIXED_MIN_DECIMALS: f64 = -100.0;
const FIXED_MAX_DECIMALS: f64 = 20.0;
const FIXED_DEFAULT_DECIMALS: i32 = 2;

fn check_text_arity(args: &[Expr], min: usize, max: usize) -> Result<(), FormulaError> {
    if args.len() < min || args.len() > max {
        Err(FormulaError::Value)
    } else {
        Ok(())
    }
}

/// Wraps `out` as a text value, failing when it exceeds the cell text limit.
fn bounded_text(out: String) -> CellResult {
    if out.chars().count() > SheetEngine::MAX_CELL_TEXT_CHARS {
        return Err(FormulaError::Value);
    }
    Ok(EvaluatedCellValue::Text(out))
}

fn number_cell(v: f64) -> CellResult {
    Ok(EvaluatedCellValue::Number(v))
}

/// Running character budget for text built piece by piece.
#[derive(Default)]
struct TextBudget {
    total_chars: usize,
}

impl TextBudget {
    /// Reserves `chars` characters; fails when the cell text limit is exceeded.
    #[inline]
    fn reserve(&mut self, chars: usize) -> Result<(), FormulaError> {
        self.total_chars = self.total_chars.saturating_add(chars);
        if self.total_chars > SheetEngine::MAX_CELL_TEXT_CHARS {
            return Err(FormulaError::Value);
        }
        Ok(())
    }

    fn push(&mut self, out: &mut String, piece: &str) -> Result<(), FormulaError> {
        self.push_counted(out, piece, piece.chars().count())
    }

    fn push_counted(
        &mut self,
        out: &mut String,
        piece: &str,
        chars: usize,
    ) -> Result<(), FormulaError> {
        self.reserve(chars)?;
        out.push_str(piece);
        Ok(())
    }
}

fn proper_case(s: &str) -> String {
    let mut res = String::with_capacity(s.len());
    let mut cap_next = true;
    for c in s.chars() {
        if !c.is_alphabetic() {
            cap_next = true;
            res.push(c);
        } else if cap_next {
            res.extend(c.to_uppercase());
            cap_next = false;
        } else {
            res.extend(c.to_lowercase());
        }
    }
    res
}

/// SUBSTITUTE core: replaces every (or only the `target`-th) occurrence of `old`.
fn substitute_text(text: &str, old: &str, new: &str, target: Option<usize>) -> CellResult {
    if old.is_empty() {
        return bounded_text(text.to_string());
    }
    let old_chars = old.chars().count();
    let new_chars = new.chars().count();
    let mut out = String::new();
    let mut budget = TextBudget::default();
    let mut rem = text;
    let mut count = 0usize;
    while let Some(pos) = rem.find(old) {
        count += 1;
        budget.push(&mut out, &rem[..pos])?;
        let tail = &rem[pos + old.len()..];
        if target.is_some_and(|t| t != count) {
            budget.push_counted(&mut out, old, old_chars)?;
        } else {
            budget.push_counted(&mut out, new, new_chars)?;
            if target.is_some() {
                budget.push(&mut out, tail)?;
                return Ok(EvaluatedCellValue::Text(out));
            }
        }
        rem = tail;
    }
    budget.push(&mut out, rem)?;
    Ok(EvaluatedCellValue::Text(out))
}

/// SEARCH: case-insensitive wildcard search; `start` is 1-based.
fn search_wildcard(needle: &str, hay: &str, start: isize) -> CellResult {
    let byte_indices: Vec<usize> = hay.char_indices().map(|(i, _)| i).collect();
    if start < 1 || (start as usize) > byte_indices.len().max(1) {
        return Err(FormulaError::Value);
    }
    if needle.is_empty() {
        return number_cell(start as f64);
    }
    let st = (start - 1) as usize;
    for (char_idx, &byte_pos) in byte_indices.iter().enumerate().skip(st) {
        if crate::coercion::wildcard_prefix_match(&hay[byte_pos..], needle) {
            return number_cell((char_idx + 1) as f64);
        }
    }
    Err(FormulaError::Value)
}

/// FIND: case-sensitive literal search; `start` is 1-based.
fn find_literal(needle: &str, hay: &str, start: isize) -> CellResult {
    if start < 1 {
        return Err(FormulaError::Value);
    }
    let st = (start - 1) as usize;
    if hay.is_ascii() && needle.is_ascii() {
        if st + 1 > hay.len().max(1) {
            return Err(FormulaError::Value);
        }
        if needle.is_empty() {
            return number_cell(start as f64);
        }
        return hay
            .get(st..)
            .and_then(|s| s.find(needle))
            .map(|p| EvaluatedCellValue::Number((st + p + 1) as f64))
            .ok_or(FormulaError::Value);
    }
    let hay_len = hay.chars().count();
    if st + 1 > hay_len.max(1) {
        return Err(FormulaError::Value);
    }
    if needle.is_empty() {
        return number_cell(start as f64);
    }
    let start_byte = hay.char_indices().nth(st).map_or(hay.len(), |(i, _)| i);
    let rel_byte = hay[start_byte..].find(needle).ok_or(FormulaError::Value)?;
    let char_offset = hay[start_byte..start_byte + rel_byte].chars().count();
    number_cell((st + char_offset + 1) as f64)
}

fn find_delim_matches(text: &str, delim: &str, case_insensitive: bool) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if !case_insensitive {
        let mut offset = 0;
        while let Some(pos) = text[offset..].find(delim) {
            let s = offset + pos;
            let e = s + delim.len();
            out.push((s, e));
            offset = e;
        }
        return out;
    }
    let d_low = delim.to_lowercase();
    let d_chars = d_low.chars().count();
    let mut skip_until = 0usize;
    for (b_idx, _) in text.char_indices() {
        if b_idx < skip_until {
            continue;
        }
        let rem = &text[b_idx..];
        let end_rel = rem
            .char_indices()
            .nth(d_chars)
            .map_or(rem.len(), |(i, _)| i);
        if rem.chars().count() >= d_chars && rem[..end_rel].to_lowercase() == d_low {
            out.push((b_idx, b_idx + end_rel));
            skip_until = b_idx + end_rel;
        }
    }
    out
}

fn parse_numbervalue(text: &str, dec_sep: char, grp_sep: char) -> CellResult {
    if dec_sep == grp_sep {
        return Err(FormulaError::Value);
    }
    let mut s = text.trim();
    if s.is_empty() {
        return number_cell(0.0);
    }
    let mut pct = 0i32;
    while let Some(rest) = s.strip_suffix('%') {
        pct += 1;
        s = rest.trim_end();
    }
    let mut norm = String::with_capacity(s.len());
    let mut seen_dec = false;
    for ch in s.chars() {
        if ch == dec_sep {
            if seen_dec {
                return Err(FormulaError::Value);
            }
            seen_dec = true;
            norm.push('.');
        } else if ch == grp_sep {
            if seen_dec {
                return Err(FormulaError::Value);
            }
        } else if !ch.is_whitespace() {
            norm.push(ch);
        }
    }
    let v = norm.parse::<f64>().map_err(|_| FormulaError::Value)?;
    if !v.is_finite() {
        return Err(FormulaError::Value);
    }
    number_cell(v / 100.0_f64.powi(pct))
}

impl SheetEngine {
    pub(crate) fn eval_text_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        match name {
            "LEN" => {
                check_text_arity(args, 1, 1)?;
                let s = self.text_arg(&args[0], ctx, depth)?;
                number_cell(s.chars().count() as f64)
            }
            "CONCAT" | "CONCATENATE" => self.text_concat(args, ctx, depth),
            "TEXTJOIN" => self.text_join(args, ctx, depth),
            "LEFT" | "RIGHT" => self.text_left_right(name == "LEFT", args, ctx, depth),
            "MID" => self.text_mid(args, ctx, depth),
            "TRIM" | "UPPER" | "LOWER" | "PROPER" => self.text_case(name, args, ctx, depth),
            "SUBSTITUTE" => self.text_substitute(args, ctx, depth),
            "REPLACE" => self.text_replace(args, ctx, depth),
            "FIND" | "SEARCH" => self.text_find(name == "SEARCH", args, ctx, depth),
            "EXACT" => {
                check_text_arity(args, 2, 2)?;
                let a = self.text_arg(&args[0], ctx, depth)?;
                let b = self.text_arg(&args[1], ctx, depth)?;
                Ok(EvaluatedCellValue::Boolean(a == b))
            }
            "REPT" => self.text_rept(args, ctx, depth),
            "VALUE" => {
                check_text_arity(args, 1, 1)?;
                let v = self.eval_arg_val(&args[0], ctx, depth)?;
                crate::coercion::coerce_to_number(&v).map(EvaluatedCellValue::Number)
            }
            "NUMBERVALUE" => self.text_numbervalue(args, ctx, depth),
            "VALUETOTEXT" => self.text_valuetotext(args, ctx, depth),
            "TEXTBEFORE" | "TEXTAFTER" => {
                self.text_before_after(name == "TEXTBEFORE", args, ctx, depth)
            }
            "TEXT" => self.text_format(args, ctx, depth),
            "FIXED" => self.text_fixed(args, ctx, depth),
            "CLEAN" => {
                check_text_arity(args, 1, 1)?;
                let s = self.text_arg(&args[0], ctx, depth)?;
                Ok(EvaluatedCellValue::Text(
                    s.chars().filter(|&c| (c as u32) >= MIN_PRINTABLE).collect(),
                ))
            }
            "CHAR" | "UNICHAR" => {
                check_text_arity(args, 1, 1)?;
                let code = self.eval_arg_num(&args[0], ctx, depth)?.trunc() as i64;
                if !(1..=MAX_CHAR_CODE).contains(&code) {
                    return Err(FormulaError::Value);
                }
                let ch = char::from_u32(code as u32).ok_or(FormulaError::Value)?;
                Ok(EvaluatedCellValue::Text(ch.to_string()))
            }
            "CODE" | "UNICODE" => {
                check_text_arity(args, 1, 1)?;
                let s = self.text_arg(&args[0], ctx, depth)?;
                let ch = s.chars().next().ok_or(FormulaError::Value)?;
                number_cell((ch as u32) as f64)
            }
            _ => Err(FormulaError::Name),
        }
    }

    /// Evaluates `arg` and renders it as display text.
    fn text_arg(
        &self,
        arg: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<String, FormulaError> {
        Ok(self.eval_arg_val(arg, ctx, depth)?.to_display_string())
    }

    fn text_chars_arg(
        &self,
        arg: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Vec<char>, FormulaError> {
        Ok(self.text_arg(arg, ctx, depth)?.chars().collect())
    }

    fn text_concat(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        let vals = self.expand_args_ctx(args, ctx, depth)?;
        let mut out = String::new();
        let mut budget = TextBudget::default();
        for v in vals {
            budget.push(&mut out, &v.to_display_string())?;
        }
        Ok(EvaluatedCellValue::Text(out))
    }

    fn text_join(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        check_text_arity(args, 3, usize::MAX)?;
        let delim = self.text_arg(&args[0], ctx, depth)?;
        let delim_chars = delim.chars().count();
        let ignore_empty = self
            .eval_arg_val(&args[1], ctx, depth)?
            .as_bool()
            .ok_or(FormulaError::Value)?;
        let vals = self.expand_args_ctx(&args[2..], ctx, depth)?;
        let mut out = String::new();
        let mut budget = TextBudget::default();
        let mut first = true;
        for v in vals {
            let s = v.to_display_string();
            if ignore_empty && s.is_empty() {
                continue;
            }
            if !first {
                budget.push_counted(&mut out, &delim, delim_chars)?;
            }
            first = false;
            budget.push(&mut out, &s)?;
        }
        Ok(EvaluatedCellValue::Text(out))
    }

    fn text_left_right(
        &self,
        is_left: bool,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        check_text_arity(args, 1, 2)?;
        let s = self.text_arg(&args[0], ctx, depth)?;
        let n = match args.get(1) {
            Some(arg) => {
                let k = self.eval_arg_num(arg, ctx, depth)?.trunc() as isize;
                usize::try_from(k).map_err(|_| FormulaError::Value)?
            }
            None => 1,
        };
        let sub = if s.is_ascii() {
            let take = n.min(s.len());
            if is_left {
                &s[..take]
            } else {
                &s[s.len() - take..]
            }
        } else if is_left {
            let end = s.char_indices().nth(n).map_or(s.len(), |(i, _)| i);
            &s[..end]
        } else {
            let start = if n == 0 {
                s.len()
            } else {
                s.char_indices().rev().nth(n - 1).map_or(0, |(i, _)| i)
            };
            &s[start..]
        };
        Ok(EvaluatedCellValue::Text(sub.to_string()))
    }

    fn text_mid(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        check_text_arity(args, 3, 3)?;
        let s = self.text_arg(&args[0], ctx, depth)?;
        let start = self.eval_arg_num(&args[1], ctx, depth)?.trunc() as isize;
        let count = self.eval_arg_num(&args[2], ctx, depth)?.trunc() as isize;
        if start < 1 || count < 0 {
            return Err(FormulaError::Value);
        }
        let st = (start - 1) as usize;
        let cnt = count as usize;
        let sub = if s.is_ascii() {
            let b_st = st.min(s.len());
            let b_end = b_st.saturating_add(cnt).min(s.len());
            &s[b_st..b_end]
        } else {
            let b_st = s.char_indices().nth(st).map_or(s.len(), |(i, _)| i);
            let rem = &s[b_st..];
            let b_end = rem.char_indices().nth(cnt).map_or(rem.len(), |(i, _)| i);
            &rem[..b_end]
        };
        Ok(EvaluatedCellValue::Text(sub.to_string()))
    }

    fn text_case(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        check_text_arity(args, 1, 1)?;
        let s = self.text_arg(&args[0], ctx, depth)?;
        bounded_text(match name {
            "TRIM" => s.split_whitespace().collect::<Vec<_>>().join(" "),
            "UPPER" => s.to_uppercase(),
            "LOWER" => s.to_lowercase(),
            _ => proper_case(&s),
        })
    }

    fn text_substitute(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        check_text_arity(args, 3, 4)?;
        let text = self.text_arg(&args[0], ctx, depth)?;
        let old_t = self.text_arg(&args[1], ctx, depth)?;
        let new_t = self.text_arg(&args[2], ctx, depth)?;
        let target_inst = match args.get(3) {
            Some(arg) => {
                let inst = self.eval_arg_num(arg, ctx, depth)?.trunc() as isize;
                Some(
                    usize::try_from(inst)
                        .ok()
                        .filter(|&i| i >= 1)
                        .ok_or(FormulaError::Value)?,
                )
            }
            None => None,
        };
        substitute_text(&text, &old_t, &new_t, target_inst)
    }

    fn text_replace(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        check_text_arity(args, 4, 4)?;
        let chars = self.text_chars_arg(&args[0], ctx, depth)?;
        let start = self.eval_arg_num(&args[1], ctx, depth)?.trunc() as isize;
        let num_chars = self.eval_arg_num(&args[2], ctx, depth)?.trunc() as isize;
        let new_t = self.text_arg(&args[3], ctx, depth)?;
        if start < 1 || num_chars < 0 {
            return Err(FormulaError::Value);
        }
        let st = ((start - 1) as usize).min(chars.len());
        let end = (st + num_chars as usize).min(chars.len());
        let suffix_len = chars.len() - end;
        let mut budget = TextBudget::default();
        budget.reserve(
            st.saturating_add(new_t.chars().count())
                .saturating_add(suffix_len),
        )?;
        let mut out: String = chars[..st].iter().collect();
        out.push_str(&new_t);
        out.extend(chars[end..].iter());
        Ok(EvaluatedCellValue::Text(out))
    }

    fn text_find(
        &self,
        is_search: bool,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        check_text_arity(args, 2, 3)?;
        let needle = self.text_arg(&args[0], ctx, depth)?;
        let hay = self.text_arg(&args[1], ctx, depth)?;
        let start = match args.get(2) {
            Some(arg) => self.eval_arg_num(arg, ctx, depth)?.trunc() as isize,
            None => 1,
        };
        if is_search {
            search_wildcard(&needle, &hay, start)
        } else {
            find_literal(&needle, &hay, start)
        }
    }

    fn text_rept(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        check_text_arity(args, 2, 2)?;
        let s = self.text_arg(&args[0], ctx, depth)?;
        let n = self.eval_arg_num(&args[1], ctx, depth)?.trunc() as isize;
        if n < 0 || s.chars().count().saturating_mul(n as usize) > Self::MAX_CELL_TEXT_CHARS {
            return Err(FormulaError::Value);
        }
        Ok(EvaluatedCellValue::Text(s.repeat(n as usize)))
    }

    fn text_format(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        check_text_arity(args, 2, 2)?;
        let val = self.eval_arg_val(&args[0], ctx, depth)?;
        let fmt_str = self.text_arg(&args[1], ctx, depth)?;
        let out = match val.as_number() {
            Some(num) => crate::coercion::format_number_with_code(num, &fmt_str)?,
            None => {
                crate::coercion::format_text_value_with_code(&val.to_display_string(), &fmt_str)
            }
        };
        bounded_text(out)
    }

    fn text_fixed(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        check_text_arity(args, 1, 3)?;
        let num = self.eval_arg_num(&args[0], ctx, depth)?;
        let decimals = match args.get(1) {
            Some(arg) if !matches!(arg, Expr::Missing) => {
                self.eval_arg_num(arg, ctx, depth)?
                    .trunc()
                    .clamp(FIXED_MIN_DECIMALS, FIXED_MAX_DECIMALS) as i32
            }
            _ => FIXED_DEFAULT_DECIMALS,
        };
        let no_commas = match args.get(2) {
            Some(arg) if !matches!(arg, Expr::Missing) => self
                .eval_arg_val(arg, ctx, depth)?
                .as_bool()
                .ok_or(FormulaError::Value)?,
            _ => false,
        };
        bounded_text(crate::coercion::format_fixed_number(
            num, decimals, !no_commas,
        ))
    }

    fn opt_int_arg(
        &self,
        args: &[Expr],
        idx: usize,
        def: isize,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<isize, FormulaError> {
        match args.get(idx) {
            Some(a) if !matches!(a, Expr::Missing) => {
                Ok(self.eval_arg_num(a, ctx, depth)?.trunc() as isize)
            }
            _ => Ok(def),
        }
    }

    fn text_numbervalue(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        check_text_arity(args, 1, 3)?;
        let text = self.text_arg(&args[0], ctx, depth)?;
        let mut sep = |idx: usize, def: char| -> Result<char, FormulaError> {
            match args.get(idx) {
                Some(a) if !matches!(a, Expr::Missing) => self
                    .text_arg(a, ctx, depth)?
                    .chars()
                    .next()
                    .ok_or(FormulaError::Value),
                _ => Ok(def),
            }
        };
        let dec = sep(1, '.')?;
        let grp = sep(2, ',')?;
        parse_numbervalue(&text, dec, grp)
    }

    fn text_valuetotext(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        check_text_arity(args, 1, 2)?;
        let val = self.eval_expr_ctx(&args[0], ctx, depth);
        let fmt = self.opt_int_arg(args, 1, 0, ctx, depth)?;
        if fmt != 0 && fmt != 1 {
            return Err(FormulaError::Value);
        }
        let s = match (&val, fmt) {
            (EvaluatedCellValue::Text(t), 1) => format!("\"{}\"", t.replace('"', "\"\"")),
            _ => val.to_display_string(),
        };
        bounded_text(s)
    }

    fn text_before_after(
        &self,
        is_before: bool,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        check_text_arity(args, 2, 6)?;
        let text = self.text_arg(&args[0], ctx, depth)?;
        let delim = self.text_arg(&args[1], ctx, depth)?;
        let inst = self.opt_int_arg(args, 2, 1, ctx, depth)?;
        let mode = self.opt_int_arg(args, 3, 0, ctx, depth)?;
        let match_end = self.opt_int_arg(args, 4, 0, ctx, depth)?;
        if inst == 0 || (mode != 0 && mode != 1) || (match_end != 0 && match_end != 1) {
            return Err(FormulaError::Value);
        }
        if delim.is_empty() {
            let take = if (is_before && inst < 0) || (!is_before && inst > 0) {
                text
            } else {
                String::new()
            };
            return Ok(EvaluatedCellValue::Text(take));
        }
        let hits = find_delim_matches(&text, &delim, mode == 1);
        let picked = if inst > 0 {
            hits.get((inst - 1) as usize).copied()
        } else {
            hits.len()
                .checked_sub((-inst) as usize)
                .and_then(|i| hits.get(i).copied())
        };
        if let Some((s, e)) = picked {
            let sub = if is_before { &text[..s] } else { &text[e..] };
            return Ok(EvaluatedCellValue::Text(sub.to_string()));
        }
        if match_end == 1 && (inst == 1 || inst == -1) {
            let take = if (is_before && inst > 0) || (!is_before && inst < 0) {
                text
            } else {
                String::new()
            };
            return Ok(EvaluatedCellValue::Text(take));
        }
        match args.get(5) {
            Some(fb) if !matches!(fb, Expr::Missing) => Ok(self.eval_expr_ctx(fb, ctx, depth)),
            _ => Err(FormulaError::Na),
        }
    }

    pub(crate) fn eval_logical_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        match name {
            "IF" => self.logical_if(args, ctx, depth),
            "IFS" => self.logical_ifs(args, ctx, depth),
            "AND" | "OR" | "XOR" => self.logical_fold(name, args, ctx, depth),
            "NOT" => {
                check_text_arity(args, 1, 1)?;
                let b = self.bool_arg(&args[0], ctx, depth)?;
                Ok(EvaluatedCellValue::Boolean(!b))
            }
            "IFERROR" | "IFNA" => {
                check_text_arity(args, 2, 2)?;
                let primary = self.eval_expr_ctx(&args[0], ctx, depth);
                let catch = match primary {
                    EvaluatedCellValue::Error(FormulaError::Na) => true,
                    EvaluatedCellValue::Error(_) => name == "IFERROR",
                    _ => false,
                };
                Ok(if catch {
                    self.eval_expr_ctx(&args[1], ctx, depth)
                } else {
                    primary
                })
            }
            "SWITCH" => self.logical_switch(args, ctx, depth),
            "TRUE" => Ok(EvaluatedCellValue::Boolean(true)),
            "FALSE" => Ok(EvaluatedCellValue::Boolean(false)),
            _ => Err(FormulaError::Name),
        }
    }

    fn bool_arg(
        &self,
        arg: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<bool, FormulaError> {
        self.eval_arg_val(arg, ctx, depth)?
            .as_bool()
            .ok_or(FormulaError::Value)
    }

    fn logical_if(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        check_text_arity(args, 2, 3)?;
        if self.bool_arg(&args[0], ctx, depth)? {
            return Ok(self.eval_expr_ctx(&args[1], ctx, depth));
        }
        match args.get(2) {
            Some(otherwise) => Ok(self.eval_expr_ctx(otherwise, ctx, depth)),
            None => Ok(EvaluatedCellValue::Boolean(false)),
        }
    }

    fn logical_ifs(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        if args.len() < 2 || args.len() % 2 != 0 {
            return Err(FormulaError::Value);
        }
        for pair in args.chunks(2) {
            if self.bool_arg(&pair[0], ctx, depth)? {
                return Ok(self.eval_expr_ctx(&pair[1], ctx, depth));
            }
        }
        Err(FormulaError::Na)
    }

    /// AND / OR / XOR over all non-empty arguments.
    fn logical_fold(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        let vals = self.expand_args_ctx(args, ctx, depth)?;
        let mut trues = 0usize;
        let mut seen = 0usize;
        for v in vals
            .iter()
            .filter(|v| !matches!(v, EvaluatedCellValue::Empty))
        {
            seen += 1;
            if v.as_bool().ok_or(FormulaError::Value)? {
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

    fn logical_switch(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        check_text_arity(args, 3, usize::MAX)?;
        let target = self.eval_arg_val(&args[0], ctx, depth)?;
        let rest = &args[1..];
        for pair in rest.chunks_exact(2) {
            let k = self.eval_arg_val(&pair[0], ctx, depth)?;
            if Self::values_match(&target, &k) {
                return Ok(self.eval_expr_ctx(&pair[1], ctx, depth));
            }
        }
        match rest.len() % 2 {
            1 => Ok(self.eval_expr_ctx(&rest[rest.len() - 1], ctx, depth)),
            _ => Err(FormulaError::Na),
        }
    }
}

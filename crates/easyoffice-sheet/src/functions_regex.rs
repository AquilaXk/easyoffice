//! `REGEXEXTRACT`, `REGEXREPLACE` and `REGEXTEST`: text cut, rewritten and tested by a pattern.
//!
//! 참조 구현: IronCalc `base/src/functions/text/regex_functions.rs` (`fn_regexextract` 24-86,
//! `fn_regexreplace` 88-123, `fn_regextest` 125-148), 기준 커밋
//! f93662006b8fb1c401dc870bbda26c5d60ac6037. MIT OR Apache-2.0, Copyright 2023 EqualTo GmbH,
//! 2023-present IronCalc contributors, 2026-present IronCalc GmbH. The matching is done by the
//! `regex` crate (MIT OR Apache-2.0), as in the reference.
//!
//! The function names, the order of the checks, the branches and the error values are the
//! reference's (an invalid pattern is `#VALUE!`, no match is `#N/A`, `REGEXEXTRACT` can give every
//! match as a row). Adapter changes only: `CalcResult` is `Result<Val, FormulaError>`, arguments are
//! read through `arg_single`. Changes beyond the adapter, all for untrusted input or for the
//! documented arguments of the functions:
//! - the pattern is compiled by a `RegexBuilder` with a length limit, `size_limit`,
//!   `dfa_size_limit` and `nest_limit`, so a hostile pattern is `#VALUE!` (the engine matches in
//!   linear time, there is no catastrophic backtracking);
//! - `REGEXEXTRACT` reads `return_mode` 0 (the whole first match, the reference gives group 1 when
//!   the pattern has a group), 1 (every match) and 2 (the groups of the first match), and every
//!   function reads the case sensitivity argument; `REGEXREPLACE` reads `occurrence`;
//! - a result text is limited to the cell text size.

use crate::array::{Env, Grid, Val};
use crate::array_eval::ValResult;
use crate::{EvalContext, EvaluatedCellValue as V, Expr, FormulaError, SheetEngine};
use regex::{Regex, RegexBuilder};

/// Compiled program size and lazy DFA cache one pattern may use, in bytes.
const REGEX_SIZE_LIMIT: usize = 1 << 20;
/// Deepest nesting of groups a pattern may have.
const REGEX_NEST_LIMIT: u32 = 64;
/// Longest pattern, in bytes.
const MAX_PATTERN_BYTES: usize = 4096;

fn new_regex(pattern: &str, case_insensitive: bool) -> Result<Regex, FormulaError> {
    if pattern.len() > MAX_PATTERN_BYTES {
        return Err(FormulaError::Value);
    }
    RegexBuilder::new(pattern)
        .case_insensitive(case_insensitive)
        .size_limit(REGEX_SIZE_LIMIT)
        .dfa_size_limit(REGEX_SIZE_LIMIT)
        .nest_limit(REGEX_NEST_LIMIT)
        .build()
        // Invalid regular expression
        .map_err(|_| FormulaError::Value)
}

impl SheetEngine {
    fn get_string(
        &self,
        args: &[Expr],
        i: usize,
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<String, FormulaError> {
        match self.arg_single(args, i, None, env, ctx, depth)? {
            V::Error(e) => Err(e),
            v => Ok(v.to_display_string()),
        }
    }

    /// The case sensitivity argument: 0 (default) matches case, 1 ignores it.
    fn get_case_insensitive(
        &self,
        args: &[Expr],
        i: usize,
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<bool, FormulaError> {
        match self.arg_number(args, i, 0.0, env, ctx, depth)?.trunc() as i64 {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(FormulaError::Value),
        }
    }

    /// REGEXEXTRACT(text, regular_expression, [return_mode], [case_sensitivity])
    ///
    /// Returns the first substring of `text` that matches `regular_expression`.
    /// When the optional `return_mode` argument is 1, returns a horizontal array of
    /// every match (one match per column) instead of only the first one; 2 returns the
    /// capture groups of the first match.
    /// Returns #N/A if there is no match, #VALUE! if the regex is invalid.
    pub(crate) fn fn_regexextract(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.len() < 2 || args.len() > 4 {
            return Err(FormulaError::Value);
        }
        let text = self.get_string(args, 0, env, ctx, depth)?;
        let pattern = self.get_string(args, 1, env, ctx, depth)?;
        let return_mode = if args.len() >= 3 {
            self.arg_number(args, 2, 0.0, env, ctx, depth)?.trunc() as i64
        } else {
            0
        };
        if !(0..=2).contains(&return_mode) {
            return Err(FormulaError::Value);
        }
        let case_insensitive = self.get_case_insensitive(args, 3, env, ctx, depth)?;
        let re = new_regex(&pattern, case_insensitive)?;
        let row: Vec<V> = if return_mode == 1 {
            re.find_iter(&text)
                .map(|m| V::Text(m.as_str().to_string()))
                .collect()
        } else if return_mode == 2 {
            match re.captures(&text) {
                None => vec![],
                Some(caps) => caps
                    .iter()
                    .skip(1)
                    .map(|g| V::Text(g.map_or("", |m| m.as_str()).to_string()))
                    .collect(),
            }
        } else {
            match re.find(&text) {
                None => vec![],
                Some(m) => vec![V::Text(m.as_str().to_string())],
            }
        };
        if row.is_empty() {
            // No match found
            return Err(FormulaError::Na);
        }
        let columns = row.len();
        Ok(Val::grid(Grid::from_rows_checked(ctx, 1, columns, row)?))
    }

    /// REGEXREPLACE(text, regular_expression, replacement, [occurrence], [case_sensitivity])
    ///
    /// Replaces every substring of `text` that matches `regular_expression` with
    /// `replacement` (only the `occurrence`-th match when it is given, counted from the end
    /// when negative). Returns #VALUE! if the regex is invalid.
    pub(crate) fn fn_regexreplace(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.len() < 3 || args.len() > 5 {
            return Err(FormulaError::Value);
        }
        let text = self.get_string(args, 0, env, ctx, depth)?;
        let pattern = self.get_string(args, 1, env, ctx, depth)?;
        let replacement = self.get_string(args, 2, env, ctx, depth)?;
        let occurrence = self.arg_number(args, 3, 0.0, env, ctx, depth)?.trunc() as i64;
        let case_insensitive = self.get_case_insensitive(args, 4, env, ctx, depth)?;
        let re = new_regex(&pattern, case_insensitive)?;
        let all: Vec<_> = re.captures_iter(&text).collect();
        let picked = match occurrence {
            0 => 0..all.len(),
            n if n > 0 => {
                let i = usize::try_from(n - 1).unwrap_or(usize::MAX).min(all.len());
                i..(i + 1).min(all.len())
            }
            n => match all.len().checked_sub(n.unsigned_abs() as usize) {
                Some(i) => i..i + 1,
                None => 0..0,
            },
        };
        let mut out = String::new();
        let mut tail = 0usize;
        for caps in &all[picked] {
            let whole = caps.get(0).map_or(0..0, |m| m.range());
            out.push_str(&text[tail..whole.start]);
            caps.expand(&replacement, &mut out);
            tail = whole.end;
            if out.len() > Self::MAX_CELL_TEXT_CHARS * 4 {
                return Err(FormulaError::Value);
            }
        }
        out.push_str(&text[tail..]);
        if out.chars().count() > Self::MAX_CELL_TEXT_CHARS {
            return Err(FormulaError::Value);
        }
        Ok(Val::scalar(V::Text(out)))
    }

    /// REGEXTEST(text, regular_expression, [case_sensitivity])
    ///
    /// Returns `TRUE` if `regular_expression` matches anywhere in `text`, `FALSE`
    /// otherwise. Returns #VALUE! if the regex is invalid.
    pub(crate) fn fn_regextest(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.len() < 2 || args.len() > 3 {
            return Err(FormulaError::Value);
        }
        let text = self.get_string(args, 0, env, ctx, depth)?;
        let pattern = self.get_string(args, 1, env, ctx, depth)?;
        let case_insensitive = self.get_case_insensitive(args, 2, env, ctx, depth)?;
        let re = new_regex(&pattern, case_insensitive)?;
        Ok(Val::scalar(V::Boolean(re.is_match(&text))))
    }
}

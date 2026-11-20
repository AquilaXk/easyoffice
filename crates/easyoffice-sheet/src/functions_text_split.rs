//! `TEXTSPLIT` and `ARRAYTOTEXT`: text cut into an array, and an array written out as text.
//!
//! 참조 구현: IronCalc `base/src/functions/text/textsplit.rs` (`fn_textsplit` 25-170,
//! `extract_delimiters` 174-232, `split_by_any` 238-281) and `text/arraytotext.rs`
//! (`fn_arraytotext` 13-81, `format_array_value` 84-113), 기준 커밋
//! f93662006b8fb1c401dc870bbda26c5d60ac6037. MIT OR Apache-2.0, Copyright 2023 EqualTo GmbH,
//! 2023-present IronCalc contributors, 2026-present IronCalc GmbH. The bodies keep the reference's
//! branches and error values. Changes: the text is cut on `char` positions (a case fold that keeps
//! one `char` per `char`, where the reference lower-cases the whole string and reuses byte offsets),
//! the work (`text length * delimiters`) and the result size are bounded, and a result text is
//! limited to the cell text size.

use crate::array::{Env, Grid, Val};
use crate::array_eval::ValResult;
use crate::{EvalContext, EvaluatedCellValue as V, Expr, FormulaError, SheetEngine};

/// Most `text length * delimiter count` comparisons one `TEXTSPLIT` call makes.
const MAX_SPLIT_WORK: usize = 4_000_000;

/// The same `char` with its case folded one-to-one, so positions in the folded text are positions in
/// the original.
fn fold(c: char) -> char {
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(one), None) => one,
        _ => c,
    }
}

/// Cuts `text[range]` at the earliest match of any delimiter (the first listed wins a tie).
/// `cmp` is `text` as it is compared, `delims` the delimiters as they are compared.
fn split_by_any(
    cmp: &[char],
    range: std::ops::Range<usize>,
    delims: &[Vec<char>],
) -> Vec<std::ops::Range<usize>> {
    if delims.is_empty() {
        return vec![range];
    }
    let mut parts = Vec::new();
    let (mut start, mut pos) = (range.start, range.start);
    while pos < range.end {
        let hit = delims
            .iter()
            .find(|d| pos + d.len() <= range.end && cmp[pos..pos + d.len()] == d[..]);
        match hit {
            Some(d) => {
                parts.push(start..pos);
                pos += d.len();
                start = pos;
            }
            None => pos += 1,
        }
    }
    parts.push(start..range.end);
    parts
}

fn text_of(v: V) -> Result<String, FormulaError> {
    match v {
        V::Error(e) => Err(e),
        other => Ok(other.to_display_string()),
    }
}

/// One element of an `ARRAYTOTEXT` result.
fn array_value_text(v: &V, strict: bool) -> String {
    match v {
        V::Text(s) if strict => format!("\"{}\"", s.replace('"', "\"\"")),
        other => other.to_display_string(),
    }
}

impl SheetEngine {
    /// The non-empty delimiters one `TEXTSPLIT` argument lists; an omitted argument lists none.
    fn split_delimiters(
        &self,
        args: &[Expr],
        i: usize,
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Option<Vec<String>>, FormulaError> {
        let Some(arg) = args.get(i).filter(|a| !matches!(a, Expr::Missing)) else {
            return Ok(None);
        };
        let grid = self.eval_val(arg, env, ctx, depth + 1)?.into_grid()?;
        let mut out = Vec::new();
        for v in grid.data.iter() {
            match v {
                V::Text(s) => out.push(s.clone()),
                V::Number(_) => out.push(v.to_display_string()),
                V::Empty => {}
                V::Error(e) if grid.data.len() == 1 => return Err(*e),
                _ => return Err(FormulaError::Value),
            }
        }
        Ok(Some(out))
    }

    /// `TEXTSPLIT(text, col_delimiter, [row_delimiter], [ignore_empty], [match_mode], [pad_with])`.
    pub(crate) fn fn_textsplit(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if !(2..=6).contains(&args.len()) {
            return Err(FormulaError::Value);
        }
        let text = text_of(self.arg_single(args, 0, None, env, ctx, depth)?)?;
        let col_delims = self
            .split_delimiters(args, 1, env, ctx, depth)?
            .unwrap_or_default();
        // An explicit empty string is not a delimiter; an omitted one is allowed beside a row delimiter.
        if col_delims.iter().any(|d| d.is_empty()) {
            return Err(FormulaError::Value);
        }
        let row_delims: Vec<String> = self
            .split_delimiters(args, 2, env, ctx, depth)?
            .unwrap_or_default()
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect();
        if col_delims.is_empty() && row_delims.is_empty() {
            return Err(FormulaError::Value);
        }
        let ignore_empty = self.arg_bool(args, 3, env, ctx, depth)?;
        let case_insensitive = self.arg_number(args, 4, 0.0, env, ctx, depth)? != 0.0;
        let pad = match args.get(5) {
            None | Some(Expr::Missing) => V::Error(FormulaError::Na),
            Some(_) => match self.arg_single(args, 5, None, env, ctx, depth)? {
                V::Empty => V::Error(FormulaError::Na),
                v => v,
            },
        };

        let chars: Vec<char> = text.chars().collect();
        let delim_count = col_delims.len() + row_delims.len();
        if chars.len().saturating_mul(delim_count) > MAX_SPLIT_WORK {
            return Err(FormulaError::Value);
        }
        ctx.charge_steps(chars.len().saturating_mul(delim_count) / 1024 + 1)?;
        let prepare = |list: &[String]| -> Vec<Vec<char>> {
            list.iter()
                .map(|d| {
                    d.chars()
                        .map(|c| if case_insensitive { fold(c) } else { c })
                        .collect()
                })
                .collect()
        };
        let cmp: Vec<char> = chars
            .iter()
            .map(|&c| if case_insensitive { fold(c) } else { c })
            .collect();
        let (col_cmp, row_cmp) = (prepare(&col_delims), prepare(&row_delims));

        let rows: Vec<Vec<std::ops::Range<usize>>> = split_by_any(&cmp, 0..chars.len(), &row_cmp)
            .into_iter()
            .filter(|row| !(ignore_empty && row.is_empty()))
            .map(|row| {
                let cols = split_by_any(&cmp, row, &col_cmp);
                if ignore_empty {
                    cols.into_iter().filter(|c| !c.is_empty()).collect()
                } else {
                    cols
                }
            })
            .collect();
        if rows.iter().all(|r| r.is_empty()) {
            return Err(FormulaError::Value);
        }
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        let grid = Grid::build_checked(ctx, rows.len(), width, |r, c| match rows[r].get(c) {
            Some(range) => V::Text(chars[range.clone()].iter().collect()),
            None => pad.clone(),
        })?;
        Ok(Val::grid(grid))
    }

    /// `ARRAYTOTEXT(array, [format])`: format 0 joins the values with `, `, format 1 writes an array
    /// constant with quoted text.
    pub(crate) fn fn_arraytotext(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if !(1..=2).contains(&args.len()) {
            return Err(FormulaError::Value);
        }
        let strict = match self.arg_number(args, 1, 0.0, env, ctx, depth)?.floor() as i64 {
            0 => false,
            1 => true,
            _ => return Err(FormulaError::Value),
        };
        let grid = self.eval_val(&args[0], env, ctx, depth + 1)?.into_grid()?;
        let mut out = String::new();
        let mut chars = 0usize;
        let mut push = |piece: &str, out: &mut String| -> Result<(), FormulaError> {
            chars += piece.chars().count();
            if chars > Self::MAX_CELL_TEXT_CHARS {
                return Err(FormulaError::Value);
            }
            out.push_str(piece);
            Ok(())
        };
        if grid.data.len() == 1 {
            if let V::Error(e) = grid.get(0, 0) {
                return Err(*e);
            }
            push(&array_value_text(grid.get(0, 0), strict), &mut out)?;
        } else if strict {
            push("{", &mut out)?;
            for r in 0..grid.rows {
                if r > 0 {
                    push(";", &mut out)?;
                }
                for c in 0..grid.cols {
                    if c > 0 {
                        push(",", &mut out)?;
                    }
                    push(&array_value_text(grid.get(r, c), true), &mut out)?;
                }
            }
            push("}", &mut out)?;
        } else {
            for (i, v) in grid.data.iter().enumerate() {
                if i > 0 {
                    push(", ", &mut out)?;
                }
                push(&array_value_text(v, false), &mut out)?;
            }
        }
        Ok(Val::scalar(V::Text(out)))
    }
}

//! `TEXTSPLIT` and `ARRAYTOTEXT`: text cut into an array, and an array written out as text.
//!
//! 참조 구현: IronCalc `base/src/functions/text/textsplit.rs` (`fn_textsplit` 25-170,
//! `extract_delimiters` 174-232, `split_by_any` 238-281) and `text/arraytotext.rs`
//! (`fn_arraytotext` 13-81, `format_array_value` 84-113), 기준 커밋
//! f93662006b8fb1c401dc870bbda26c5d60ac6037. MIT OR Apache-2.0, Copyright 2023 EqualTo GmbH,
//! 2023-present IronCalc contributors, 2026-present IronCalc GmbH.
//!
//! The function names, the helper names, the order of the argument checks and the error values are
//! the reference's, so a fix upstream can be followed here line by line. Adapter changes only:
//! `CalcResult` is `Result<Val, FormulaError>`, `ArrayNode` is [`EvaluatedCellValue`], the
//! arguments are read through `arg_single` and `eval_val`. Changes beyond the adapter: the text is
//! cut on `char` positions with a case fold that keeps one `char` per `char` (the reference
//! lower-cases the whole string and reuses byte offsets), the work (`text length * delimiters`) and
//! the result size are bounded, and a result text is limited to the cell text size.

use crate::array::{Env, Grid, Val};
use crate::array_eval::ValResult;
use crate::{EvalContext, EvaluatedCellValue as V, Expr, FormulaError, SheetEngine};

/// Most `text length * delimiter count` comparisons one `TEXTSPLIT` call makes.
const MAX_SPLIT_WORK: usize = 4_000_000;

/// The same `char` with its case folded one-to-one, so positions in the folded text are positions in
/// the original (the reference's `to_lowercase()` on a whole string does not keep them).
fn fold(c: char) -> char {
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(one), None) => one,
        _ => c,
    }
}

/// Split `original` by the first (earliest) occurrence of any non-empty delimiter
/// in `cmp_delims`, using `cmp` (a parallel comparison text, e.g. case folded) to
/// locate match positions. The same positions are applied to `original`.
fn split_by_any(original: &[char], cmp: &[char], cmp_delims: &[Vec<char>]) -> Vec<Vec<char>> {
    let delims: Vec<&Vec<char>> = cmp_delims.iter().filter(|d| !d.is_empty()).collect();

    if delims.is_empty() {
        return vec![original.to_vec()];
    }

    let mut result = Vec::new();
    let mut pos = 0usize;

    loop {
        // Find the earliest match among all delimiters.
        let best = (pos..cmp.len()).find_map(|at| {
            delims
                .iter()
                .find(|d| cmp[at..].starts_with(d))
                .map(|d| (at, d.len()))
        });

        match best {
            None => {
                result.push(original[pos..].to_vec());
                break;
            }
            Some((match_pos, delim_len)) => {
                result.push(original[pos..match_pos].to_vec());
                pos = match_pos + delim_len;
            }
        }
    }

    result
}

fn array_to_string(chars: &[char]) -> V {
    V::Text(chars.iter().collect())
}

/// A value as text; an error is returned as it is.
fn get_string(v: V) -> Result<String, FormulaError> {
    match v {
        V::Error(e) => Err(e),
        other => Ok(other.to_display_string()),
    }
}

fn format_array_value(node: &V, strict: bool) -> String {
    match node {
        V::Text(s) if strict => format!("\"{}\"", s.replace('"', "\"\"")),
        // Numbers, booleans and errors are written as the sheet shows them; an empty place is "".
        other => other.to_display_string(),
    }
}

impl SheetEngine {
    /// `=TEXTSPLIT(text, col_delimiter, [row_delimiter], [ignore_empty], [match_mode], [pad_with])`
    ///
    /// Splits text into a 2-D array using column and/or row delimiters.
    /// Each delimiter argument can be a single string or an array of strings: any
    /// matching delimiter triggers a split.
    ///   * text          - text to split (required)
    ///   * col_delimiter - one or more delimiters that split into columns (required)
    ///   * row_delimiter - one or more delimiters that split into rows (default: no row split)
    ///   * ignore_empty  - TRUE to skip empty segments (default FALSE)
    ///   * match_mode    - 0 = case-sensitive (default), 1 = case-insensitive
    ///   * pad_with      - value to pad short rows (default: #N/A error)
    pub(crate) fn fn_textsplit(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.len() < 2 || args.len() > 6 {
            return Err(FormulaError::Value);
        }

        let text = get_string(self.arg_single(args, 0, None, env, ctx, depth)?)?;

        let col_delims = self.extract_delimiters(args, 1, env, ctx, depth)?;

        // An explicit empty string "" as a delimiter is invalid.
        // An omitted col_delimiter (empty vec) is allowed when row_delimiter is provided.
        if col_delims.iter().any(|d| d.is_empty()) {
            return Err(FormulaError::Value);
        }

        let row_delims: Vec<String> = if args.len() >= 3 {
            // A bare empty-arg (omitted) gives no row splitting
            self.extract_delimiters(args, 2, env, ctx, depth)?
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            vec![]
        };

        if col_delims.is_empty() && row_delims.is_empty() {
            return Err(FormulaError::Value);
        }

        let ignore_empty = if args.len() >= 4 {
            self.arg_bool(args, 3, env, ctx, depth)?
        } else {
            false
        };

        let case_insensitive = if args.len() >= 5 {
            self.arg_number(args, 4, 0.0, env, ctx, depth)? != 0.0
        } else {
            false
        };

        let pad_with: Option<V> = if args.len() >= 6 {
            match args[5] {
                Expr::Missing => None,
                _ => match self.arg_single(args, 5, None, env, ctx, depth)? {
                    V::Empty => None,
                    v => Some(v),
                },
            }
        } else {
            None
        };

        let na_node = V::Error(FormulaError::Na);
        let pad_node = pad_with.as_ref().unwrap_or(&na_node);

        // Normalize to comparison text for optional case-insensitive matching.
        // The fold keeps one `char` per `char`, so positions derived from `cmp_text` are
        // valid indices into the original `text`.
        let text: Vec<char> = text.chars().collect();
        let delim_count = col_delims.len() + row_delims.len();
        if text.len().saturating_mul(delim_count) > MAX_SPLIT_WORK {
            return Err(FormulaError::Value);
        }
        ctx.charge_steps(text.len().saturating_mul(delim_count) / 1024 + 1)?;
        let normalize = |s: &[char]| -> Vec<char> {
            s.iter()
                .map(|&c| if case_insensitive { fold(c) } else { c })
                .collect()
        };
        let cmp_text = normalize(&text);
        let cmp_delims = |list: &[String]| -> Vec<Vec<char>> {
            list.iter()
                .map(|s| normalize(&s.chars().collect::<Vec<_>>()))
                .collect()
        };
        let (cmp_col_delims, cmp_row_delims) = (cmp_delims(&col_delims), cmp_delims(&row_delims));

        let rows_of_cols: Vec<Vec<Vec<char>>> = if !cmp_row_delims.is_empty() {
            // Split into rows first, then each row into columns.
            let row_parts = split_by_any(&text, &cmp_text, &cmp_row_delims);
            let cmp_row_parts = split_by_any(&cmp_text, &cmp_text, &cmp_row_delims);

            row_parts
                .into_iter()
                .zip(cmp_row_parts)
                .filter_map(|(orig_row, cmp_row_part)| {
                    if ignore_empty && orig_row.is_empty() {
                        return None;
                    }
                    let cols = split_by_any(&orig_row, &cmp_row_part, &cmp_col_delims);
                    let cols = if ignore_empty {
                        cols.into_iter().filter(|s| !s.is_empty()).collect()
                    } else {
                        cols
                    };
                    Some(cols)
                })
                .collect()
        } else {
            // No row delimiter: single row.
            let cols = split_by_any(&text, &cmp_text, &cmp_col_delims);
            let cols = if ignore_empty {
                cols.into_iter().filter(|s| !s.is_empty()).collect()
            } else {
                cols
            };
            vec![cols]
        };

        if rows_of_cols.is_empty() || rows_of_cols.iter().all(|r| r.is_empty()) {
            return Err(FormulaError::Value);
        }

        let max_cols = rows_of_cols.iter().map(|r| r.len()).max().unwrap_or(0);

        let result =
            Grid::build_checked(
                ctx,
                rows_of_cols.len(),
                max_cols,
                |r, c| match rows_of_cols[r].get(c) {
                    Some(cols) => array_to_string(cols),
                    None => pad_node.clone(),
                },
            )?;
        Ok(Val::grid(result))
    }

    /// The delimiters one `TEXTSPLIT` argument holds: a single string, an inline array or a range of
    /// strings. An omitted argument holds none.
    fn extract_delimiters(
        &self,
        args: &[Expr],
        i: usize,
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Vec<String>, FormulaError> {
        if matches!(args[i], Expr::Missing) {
            return Ok(vec![]);
        }
        let grid = self.eval_val(&args[i], env, ctx, depth + 1)?.into_grid()?;
        let mut delims = Vec::new();
        for item in grid.data.iter() {
            match item {
                V::Text(s) => delims.push(s.clone()),
                V::Number(_) => delims.push(item.to_display_string()),
                V::Empty => {}
                // An error alone is returned as it is.
                V::Error(e) if grid.data.len() == 1 => return Err(*e),
                _ => return Err(FormulaError::Value),
            }
        }
        Ok(delims)
    }

    /// ARRAYTOTEXT(array, [format])
    /// format=0 (default, concise): values joined with ", "
    /// format=1 (strict): wrapped in {} with "," between columns and ";" between rows,
    /// and strings are quoted
    pub(crate) fn fn_arraytotext(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.is_empty() || args.len() > 2 {
            return Err(FormulaError::Value);
        }
        let format = if args.len() == 2 {
            self.arg_number(args, 1, 0.0, env, ctx, depth)?.floor() as i32
        } else {
            0
        };
        if format != 0 && format != 1 {
            return Err(FormulaError::Value);
        }
        let strict = format == 1;

        let array = self.eval_val(&args[0], env, ctx, depth + 1)?.into_grid()?;

        let result = if array.data.len() == 1 {
            // A single value is written as it is: a scalar error is returned as an error.
            if let V::Error(e) = array.get(0, 0) {
                return Err(*e);
            }
            format_array_value(array.get(0, 0), strict)
        } else if strict {
            // Strict: {col1,col2;row2col1,row2col2}
            let mut rows = Vec::with_capacity(array.rows);
            for r in 0..array.rows {
                let mut row = Vec::with_capacity(array.cols);
                for c in 0..array.cols {
                    row.push(format_array_value(array.get(r, c), true));
                }
                rows.push(row.join(","));
            }
            format!("{{{}}}", rows.join(";"))
        } else {
            // Concise: flatten all elements row by row, join with ", "
            let mut elements = Vec::with_capacity(array.data.len());
            for v in array.data.iter() {
                elements.push(format_array_value(v, false));
            }
            elements.join(", ")
        };
        // The text of a cell is at most 32,767 characters.
        if result.chars().count() > Self::MAX_CELL_TEXT_CHARS {
            return Err(FormulaError::Value);
        }
        Ok(Val::scalar(V::Text(result)))
    }
}

//! Byte text functions (PD-205): `LENB`, `LEFTB`, `MIDB`, `RIGHTB`, `FINDB`, `SEARCHB`,
//! `REPLACEB`, `ASC`, `DBCS`.
//!
//! Reference implementation: `packages/engine-formula/src/functions/text/{lenb,leftb,midb,rightb,
//! findb,searchb,replaceb,asc,dbcs}/index.ts` of the Univer formula engine, commit
//! `5a06a7842c8e13fa0646d0733dc3662cf4b330b9`, Apache-2.0, Copyright 2023-present DreamNum Co.,
//! Ltd. See `THIRD_PARTY_NOTICES.md`. The byte counts come from the fixed table in
//! [`crate::byte_width`]. Positions and lengths are in bytes. A byte boundary inside a 2-byte
//! character turns that character into one space (the half that is left is one byte wide).

use crate::byte_width::{slice_bytes, text_bytes, to_full_width, to_half_width};
use crate::functions_lookup_text::{bounded_text, find_literal, search_wildcard};
use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};

type CellResult = Result<EvaluatedCellValue, FormulaError>;

fn check_arity(args: &[Expr], min: usize, max: usize) -> Result<(), FormulaError> {
    if (min..=max).contains(&args.len()) {
        Ok(())
    } else {
        Err(FormulaError::Value)
    }
}

fn text(out: String) -> CellResult {
    Ok(EvaluatedCellValue::Text(out))
}

/// A byte position or count as the reference reads it: floor, and `None` below zero.
fn byte_count(x: f64) -> Option<usize> {
    let floored = x.floor();
    (floored >= 0.0).then_some(floored as usize)
}

/// `FINDB` / `SEARCHB` over the bytes from `start` (1-based): the text from there on is searched by
/// character, and the character position found is turned back into a byte position.
fn find_bytes(is_search: bool, needle: &str, hay: &str, start: f64) -> CellResult {
    let start = start.floor();
    let total = text_bytes(hay);
    if start < 1.0 || start > total.max(1) as f64 {
        return Err(FormulaError::Value);
    }
    let from = start as usize - 1;
    let tail = slice_bytes(hay, from, total);
    let found = if is_search {
        search_wildcard(needle, &tail, 1)?
    } else {
        find_literal(needle, &tail, 1)?
    };
    let EvaluatedCellValue::Number(chars_before) = found else {
        return Err(FormulaError::Value);
    };
    let before: String = tail.chars().take(chars_before as usize - 1).collect();
    Ok(EvaluatedCellValue::Number(
        (from + text_bytes(&before) + 1) as f64,
    ))
}

impl SheetEngine {
    fn byte_count_arg(
        &self,
        arg: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Option<usize>, FormulaError> {
        Ok(byte_count(self.eval_arg_num_blank_zero(arg, ctx, depth)?))
    }

    pub(crate) fn eval_text_bytes_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        match name {
            "LENB" | "ASC" | "DBCS" => {
                check_arity(args, 1, 1)?;
                let s = self.text_arg(&args[0], ctx, depth)?;
                match name {
                    "LENB" => Ok(EvaluatedCellValue::Number(text_bytes(&s) as f64)),
                    "ASC" => bounded_text(to_half_width(&s)),
                    _ => bounded_text(to_full_width(&s)),
                }
            }
            "LEFTB" | "RIGHTB" => {
                check_arity(args, 1, 2)?;
                let s = self.text_arg(&args[0], ctx, depth)?;
                let n = match args.get(1) {
                    Some(arg) => self.byte_count_arg(arg, ctx, depth)?,
                    None => Some(1),
                }
                .ok_or(FormulaError::Value)?;
                if name == "LEFTB" {
                    return text(slice_bytes(&s, 0, n));
                }
                let total = text_bytes(&s);
                text(slice_bytes(&s, total.saturating_sub(n), total))
            }
            "MIDB" => {
                check_arity(args, 3, 3)?;
                let s = self.text_arg(&args[0], ctx, depth)?;
                let start = self.eval_arg_num_blank_zero(&args[1], ctx, depth)?.floor();
                let count = self
                    .byte_count_arg(&args[2], ctx, depth)?
                    .ok_or(FormulaError::Value)?;
                if start < 1.0 {
                    return Err(FormulaError::Value);
                }
                let from = start as usize - 1;
                text(slice_bytes(&s, from, from.saturating_add(count)))
            }
            "REPLACEB" => {
                check_arity(args, 4, 4)?;
                let old = self.text_arg(&args[0], ctx, depth)?;
                let start = self.eval_arg_num_blank_zero(&args[1], ctx, depth)?.floor();
                let count = self
                    .byte_count_arg(&args[2], ctx, depth)?
                    .ok_or(FormulaError::Value)?;
                let new = self.text_arg(&args[3], ctx, depth)?;
                if start < 1.0 {
                    return Err(FormulaError::Value);
                }
                let from = start as usize - 1;
                let total = text_bytes(&old);
                let mut out = slice_bytes(&old, 0, from);
                out.push_str(&new);
                out.push_str(&slice_bytes(&old, from.saturating_add(count), total));
                bounded_text(out)
            }
            // FINDB, SEARCHB
            _ => {
                check_arity(args, 2, 3)?;
                let needle = self.text_arg(&args[0], ctx, depth)?;
                let hay = self.text_arg(&args[1], ctx, depth)?;
                let start = match args.get(2) {
                    Some(arg) => self.eval_arg_num_blank_zero(arg, ctx, depth)?,
                    None => 1.0,
                };
                find_bytes(name == "SEARCHB", &needle, &hay, start)
            }
        }
    }
}

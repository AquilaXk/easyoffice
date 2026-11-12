//! Lookup and reference functions: `LOOKUP`, `HYPERLINK`, `ISFORMULA`, `FORMULATEXT`, `ADDRESS`,
//! `AREAS`.
//!
//! Reference implementation: the vector / array forms of `LOOKUP` (a row or column vector, or the
//! first row or column of a 2-D array searched with the last row or column as the result), the
//! `ADDRESS` argument rules (absolute-type 1 to 4, A1 or R1C1 text, a quoted sheet prefix), the
//! `FORMULATEXT` / `ISFORMULA` rules (a non-reference argument is `#N/A`, a cell without a formula
//! is `#N/A` and `FALSE`) and the `AREAS` rule (a reference counts 1, anything else is `#VALUE!`)
//! are ported from the Univer formula engine
//! (`packages/engine-formula/src/functions/{lookup/lookup,lookup/address,lookup/areas,
//! lookup/formulatext,lookup/hyperlink,information/isformula}/index.ts`, commit
//! 5a06a7842c8e13fa0646d0733dc3662cf4b330b9), Apache-2.0, Copyright 2023-present DreamNum Co.,
//! Ltd. See `THIRD_PARTY_NOTICES.md`.
//!
//! Differences from the original: `LOOKUP` reuses the approximate-match binary search of `VLOOKUP`
//! (type-aware, linear fallback on unsorted data), a relative R1C1 `ADDRESS` is counted from the
//! formula cell as the file format defines it, the row and column are checked against the sheet size,
//! `HYPERLINK` keeps the type of its friendly name and accepts only the link targets of the document
//! editor (`http`, `https`, `mailto`), and a multi-cell reference gives its first cell (cells hold one
//! value).

use crate::functions_lookup::{
    check_lookup_arity, same_value_type, sorted_approx_binary_search, update_best_floor,
};
use crate::parser::needs_sheet_quoting;
use crate::{
    EvalContext, EvaluatedCellValue, Expr, FormulaError, PackedCellCoordinate, SheetEngine,
};
use easyoffice_formats::rich_document::safe_link_target;

type CellResult = Result<EvaluatedCellValue, FormulaError>;

/// Where `LOOKUP` finds its keys and results: `len` cells from `key` / `result`, `step` apart.
struct Line {
    len: usize,
    key: (usize, usize),
    result: (usize, usize),
    step: (usize, usize),
}

impl Line {
    fn at(start: (usize, usize), step: (usize, usize), i: usize) -> (usize, usize) {
        (start.0 + step.0 * i, start.1 + step.1 * i)
    }
}

/// An `ADDRESS` row or column: a whole number in `1..=max`.
fn address_index(v: &EvaluatedCellValue, max: u32) -> Result<u32, FormulaError> {
    let n = v.as_number().ok_or(FormulaError::Value)?.trunc();
    if n >= 1.0 && n <= f64::from(max) {
        Ok(n as u32)
    } else {
        Err(FormulaError::Value)
    }
}

/// One R1C1 part: the absolute number, or the offset from the formula cell in brackets.
fn r1c1_part(label: char, absolute: bool, target: u32, current: u32) -> String {
    if absolute {
        return format!("{label}{target}");
    }
    match i64::from(target) - i64::from(current) {
        0 => label.to_string(),
        offset => format!("{label}[{offset}]"),
    }
}

/// The text of `ADDRESS`: absolute type 1 (row and column), 2 (row), 3 (column) or 4 (none).
fn address_text(
    (row, col): (u32, u32),
    abs_type: u32,
    a1: bool,
    current: (u32, u32),
    sheet: &str,
) -> String {
    let (row_abs, col_abs) = (matches!(abs_type, 1 | 2), matches!(abs_type, 1 | 3));
    let reference = if a1 {
        format!(
            "{}{}{}{row}",
            if col_abs { "$" } else { "" },
            PackedCellCoordinate::format_col_label(col - 1),
            if row_abs { "$" } else { "" },
        )
    } else {
        let r = r1c1_part('R', row_abs, row, current.0 + 1);
        r + &r1c1_part('C', col_abs, col, current.1 + 1)
    };
    if sheet.is_empty() {
        reference
    } else if needs_sheet_quoting(sheet) {
        format!("'{}'!{reference}", sheet.replace('\'', "''"))
    } else {
        format!("{sheet}!{reference}")
    }
}

impl SheetEngine {
    pub(crate) fn eval_lookup_ref_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        match name {
            "LOOKUP" => self.lookup_lookup(args, ctx, depth),
            "HYPERLINK" => self.lookup_hyperlink(args, ctx, depth),
            "ISFORMULA" | "FORMULATEXT" => {
                self.lookup_formula_cell(name == "FORMULATEXT", args, ctx, depth)
            }
            "ADDRESS" => self.lookup_address(args, ctx, depth),
            "AREAS" => {
                check_lookup_arity(args, 1, 1)?;
                match self.extract_range_and_sheet_ctx(&args[0], ctx, depth)? {
                    Some(_) => Ok(EvaluatedCellValue::Number(1.0)),
                    None => Err(FormulaError::Value),
                }
            }
            _ => Err(FormulaError::Name),
        }
    }

    /// `LOOKUP(value, lookup_vector, [result_vector])` or `LOOKUP(value, array)`: the last key not
    /// above `value` in a sorted vector.
    fn lookup_lookup(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        check_lookup_arity(args, 2, 3)?;
        let mut needle = self.eval_arg_val(&args[0], ctx, depth)?;
        if needle == EvaluatedCellValue::Empty {
            needle = EvaluatedCellValue::Number(0.0);
        }
        let keys = self
            .extract_streaming_range_ctx(&args[1], ctx, depth)
            .map_err(|_| FormulaError::Ref)?
            .ok_or(FormulaError::Value)?;
        let (rows, cols) = (keys.3, keys.4);
        let is_vector = rows == 1 || cols == 1;
        let results = match args.get(2) {
            Some(arg) if is_vector => {
                let spec = self
                    .extract_streaming_range_ctx(arg, ctx, depth)?
                    .ok_or(FormulaError::Ref)?;
                if (spec.3, spec.4) != (rows, cols) {
                    return Err(FormulaError::Ref);
                }
                spec
            }
            _ => keys.clone(),
        };
        let line = match (is_vector, rows == 1, cols > rows) {
            (true, true, _) => Line {
                len: cols,
                key: (0, 0),
                result: (0, 0),
                step: (0, 1),
            },
            (true, false, _) => Line {
                len: rows,
                key: (0, 0),
                result: (0, 0),
                step: (1, 0),
            },
            (false, _, true) => Line {
                len: cols,
                key: (0, 0),
                result: (rows - 1, 0),
                step: (0, 1),
            },
            (false, _, false) => Line {
                len: rows,
                key: (0, 0),
                result: (0, cols - 1),
                step: (1, 0),
            },
        };
        let key_at = |i: usize, ctx: &mut EvalContext<'_>| {
            let (dr, dc) = Line::at(line.key, line.step, i);
            self.eval_streaming_cell_allow_error(&keys, dr, dc, ctx, depth)
        };
        let hit = match sorted_approx_binary_search(line.len, true, &needle, |i| key_at(i, ctx))? {
            Ok(hit) => hit,
            Err(()) => {
                let mut best = None;
                for i in 0..line.len {
                    let key = key_at(i, ctx)?;
                    if same_value_type(&key, &needle) {
                        update_best_floor(&mut best, i as u32, key, &needle);
                    }
                }
                best.map(|(i, _)| i as usize)
            }
        };
        let (dr, dc) = Line::at(line.result, line.step, hit.ok_or(FormulaError::Na)?);
        match self.eval_streaming_cell_allow_error(&results, dr, dc, ctx, depth)? {
            EvaluatedCellValue::Empty => Ok(EvaluatedCellValue::Number(0.0)),
            v => Ok(v),
        }
    }

    /// `HYPERLINK(target, [name])`: the friendly name (the target text without one). The cell holds
    /// no link, so the target only has to be one the editors would open (PD-192).
    fn lookup_hyperlink(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        check_lookup_arity(args, 1, 2)?;
        let target = self.eval_arg_val(&args[0], ctx, depth)?.to_display_string();
        let name = match args.get(1) {
            Some(arg) if !matches!(arg, Expr::Missing) => Some(self.eval_arg_val(arg, ctx, depth)?),
            _ => None,
        };
        if safe_link_target(&target).is_none() {
            return Err(FormulaError::Value);
        }
        Ok(match name {
            Some(EvaluatedCellValue::Empty) => EvaluatedCellValue::Number(0.0),
            Some(v) => v,
            None => EvaluatedCellValue::Text(target),
        })
    }

    /// `FORMULATEXT(ref)` (`as_text`) or `ISFORMULA(ref)` on the first cell of the reference.
    fn lookup_formula_cell(
        &self,
        as_text: bool,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        check_lookup_arity(args, 1, 1)?;
        let located = match self.extract_range_and_sheet_ctx(&args[0], ctx, depth) {
            Ok(located) => located,
            Err(e) if !as_text => return Err(e),
            Err(_) => None,
        };
        let Some((sheet_opt, range, _)) = located else {
            return Err(FormulaError::Na);
        };
        let sheet = self
            .resolve_sheet_ref(sheet_opt.as_deref(), ctx)
            .ok_or(FormulaError::Ref)?;
        let corner = PackedCellCoordinate::new(
            range.start.row().min(range.end.row()),
            range.start.col().min(range.end.col()),
        )
        .map_err(|_| FormulaError::Ref)?;
        let formula = sheet
            .cells
            .get(&corner)
            .map(|raw| raw.trim())
            .filter(|raw| {
                raw.strip_prefix('=')
                    .is_some_and(|body| !body.trim().is_empty())
            });
        match (as_text, formula) {
            (false, found) => Ok(EvaluatedCellValue::Boolean(found.is_some())),
            (true, Some(text)) => Ok(EvaluatedCellValue::Text(text.to_string())),
            (true, None) => Err(FormulaError::Na),
        }
    }

    /// `ADDRESS(row, column, [abs_type], [a1], [sheet])`.
    fn lookup_address(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        check_lookup_arity(args, 2, 5)?;
        let row = self.eval_arg_val(&args[0], ctx, depth)?;
        let col = self.eval_arg_val(&args[1], ctx, depth)?;
        let mut optional = [None, None, None];
        for (slot, arg) in optional.iter_mut().zip(args.iter().skip(2)) {
            if !matches!(arg, Expr::Missing) {
                *slot = Some(self.eval_arg_val(arg, ctx, depth)?)
                    .filter(|v| *v != EvaluatedCellValue::Empty);
            }
        }
        let [abs_type, a1, sheet] = optional;
        let abs_type = match abs_type {
            Some(v) => v.as_number().ok_or(FormulaError::Value)?.trunc(),
            None => 1.0,
        };
        if !(1.0..=4.0).contains(&abs_type) {
            return Err(FormulaError::Value);
        }
        let a1 = match a1 {
            Some(v) => v.as_bool().ok_or(FormulaError::Value)?,
            None => true,
        };
        let cell = (
            address_index(&row, PackedCellCoordinate::MAX_ROWS)?,
            address_index(&col, PackedCellCoordinate::MAX_COLS)?,
        );
        let current = ctx.current_coord.map_or((0, 0), |c| (c.row(), c.col()));
        let sheet = sheet.map(|v| v.to_display_string()).unwrap_or_default();
        Ok(EvaluatedCellValue::Text(address_text(
            cell,
            abs_type as u32,
            a1,
            current,
            &sheet,
        )))
    }
}

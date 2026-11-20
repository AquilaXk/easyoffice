//! Information functions about references and the workbook: `ISREF`, `SHEET`, `SHEETS`, `CELL` and
//! `INFO`.
//!
//! 참조 구현: IronCalc `base/src/functions/information/mod.rs` (`fn_isref` 108-114, `fn_sheet`
//! 259-338, `fn_sheets` 376-389, `fn_cell` 394-481, `fn_info` 485-526), 기준 커밋
//! f93662006b8fb1c401dc870bbda26c5d60ac6037. MIT OR Apache-2.0, Copyright 2023 EqualTo GmbH,
//! 2023-present IronCalc contributors, 2026-present IronCalc GmbH. The branches and error values of
//! the reference are kept. Changes: a reference is what `extract_range_and_sheet_ctx` resolves (so
//! `OFFSET`, `INDIRECT` and defined names count), `CELL("filename")` and the info types that need a
//! file name, a directory or a release number are `#N/A` because the engine does not know them, and
//! `SHEETS` with a reference counts the one sheet the reference is on.

use crate::{
    EvalContext, EvaluatedCellValue as V, Expr, FormulaError, PackedCellCoordinate, SheetEngine,
    WorkbookEngine,
};

type CellResult = Result<V, FormulaError>;

impl SheetEngine {
    /// The 1-based position of the sheet `name` in the workbook of `ctx`.
    fn sheet_position(&self, name: &str, ctx: &EvalContext<'_>) -> Option<usize> {
        let wb = ctx.workbook?;
        wb.sheet_names()
            .iter()
            .position(|n| WorkbookEngine::is_same_sheet_identity(name, n))
            .map(|i| i + 1)
    }

    /// Whether `expr` is a reference (a cell, a range, a name for one, or `OFFSET` / `INDIRECT` that
    /// resolve to one). An error inside the argument makes it no reference.
    fn gives_reference(&self, expr: &Expr, ctx: &mut EvalContext<'_>, depth: usize) -> bool {
        // An array an earlier step made stands in a virtual sheet: it is a value, not a reference.
        matches!(
            self.extract_range_and_sheet_ctx(expr, ctx, depth),
            Ok(Some((sheet, _, _)))
                if sheet.as_deref().is_none_or(|s| crate::array::virtual_sheet_index(s).is_none())
        )
    }

    pub(crate) fn eval_info_ref_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        match name {
            "ISREF" => match args {
                [arg] => Ok(V::Boolean(self.gives_reference(arg, ctx, depth))),
                _ => Err(FormulaError::Value),
            },
            "SHEET" => self.info_sheet(args, ctx, depth),
            "SHEETS" => match args {
                [] => Ok(V::Number(
                    ctx.workbook.map_or(1, |wb| wb.sheet_count()) as f64
                )),
                [arg] if self.gives_reference(arg, ctx, depth) => Ok(V::Number(1.0)),
                [_] => Err(FormulaError::Value),
                _ => Err(FormulaError::Value),
            },
            "CELL" => self.info_cell(args, ctx, depth),
            "INFO" => self.info_info(args, ctx, depth),
            _ => Err(FormulaError::Name),
        }
    }

    /// `SHEET([value])`: the number of the sheet a reference is on, or of the sheet a name belongs to.
    fn info_sheet(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        let found = match args {
            [] => self.sheet_position(&self.name, ctx),
            [arg] => match self.extract_range_and_sheet_ctx(arg, ctx, depth) {
                Ok(Some((sheet, _, _))) => {
                    self.sheet_position(sheet.as_deref().unwrap_or(&self.name), ctx)
                }
                _ => {
                    let value = self.eval_arg_val(arg, ctx, depth)?;
                    match &value {
                        V::Text(text) => self.sheet_position(text, ctx),
                        _ => None,
                    }
                }
            },
            _ => return Err(FormulaError::Value),
        };
        found.map(|n| V::Number(n as f64)).ok_or(FormulaError::Na)
    }

    /// `CELL(info_type, [reference])`; the info type is always English.
    fn info_cell(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        if args.is_empty() || args.len() > 2 {
            return Err(FormulaError::Value);
        }
        let kind = self.eval_arg_val(&args[0], ctx, depth)?.to_display_string();
        let (sheet, corner) = match args.get(1) {
            Some(arg) => {
                let (sheet, range, _) = self
                    .extract_range_and_sheet_ctx(arg, ctx, depth)?
                    .ok_or(FormulaError::Value)?;
                let corner = PackedCellCoordinate::new(
                    range.start.row().min(range.end.row()),
                    range.start.col().min(range.end.col()),
                )
                .map_err(|_| FormulaError::Ref)?;
                (sheet.map(|s| s.into_owned()), corner)
            }
            None => (None, ctx.current_coord.ok_or(FormulaError::Value)?),
        };
        match kind.to_ascii_uppercase().as_str() {
            "ADDRESS" => {
                if sheet.is_some() {
                    return Err(FormulaError::Na);
                }
                Ok(V::Text(format!(
                    "${}${}",
                    PackedCellCoordinate::format_col_label(corner.col()),
                    corner.row() + 1
                )))
            }
            "COL" => Ok(V::Number(f64::from(corner.col()) + 1.0)),
            "ROW" => Ok(V::Number(f64::from(corner.row()) + 1.0)),
            "CONTENTS" => Ok(self.eval_sheet_coord(sheet.as_deref(), corner, ctx, depth)),
            "TYPE" => Ok(V::Text(
                match self.eval_sheet_coord(sheet.as_deref(), corner, ctx, depth) {
                    V::Empty => "b",
                    V::Text(_) => "l",
                    _ => "v",
                }
                .to_string(),
            )),
            "FILENAME" | "FORMAT" | "WIDTH" | "COLOR" | "PARENTHESES" | "PREFIX" | "PROTECT" => {
                Err(FormulaError::Na)
            }
            _ => Err(FormulaError::Value),
        }
    }

    /// `INFO(type_text)`; the type text is always English.
    fn info_info(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        let [arg] = args else {
            return Err(FormulaError::Value);
        };
        let kind = self.eval_arg_val(arg, ctx, depth)?.to_display_string();
        match kind.to_ascii_lowercase().as_str() {
            "numfile" => Ok(V::Number(
                ctx.workbook.map_or(1, |wb| wb.sheet_count()) as f64
            )),
            "recalc" => Ok(V::Text("Automatic".to_string())),
            "system" => Ok(V::Text(
                if cfg!(target_os = "macos") {
                    "mac"
                } else {
                    "pcdos"
                }
                .to_string(),
            )),
            "directory" | "origin" | "osversion" | "release" | "memavail" | "memused"
            | "totmem" => Err(FormulaError::Na),
            _ => Err(FormulaError::Value),
        }
    }
}

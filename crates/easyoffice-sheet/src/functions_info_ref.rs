//! Information functions about references and the workbook: `ISREF`, `SHEET`, `SHEETS`, `CELL` and
//! `INFO`.
//!
//! 참조 구현: IronCalc `base/src/functions/information/mod.rs` (`fn_isref` 108-114,
//! `gives_reference` 116-139, `fn_sheet` 259-338, `fn_sheets` 376-389, `fn_cell` 394-481,
//! `fn_info` 485-526), 기준 커밋 f93662006b8fb1c401dc870bbda26c5d60ac6037. MIT OR Apache-2.0,
//! Copyright 2023 EqualTo GmbH, 2023-present IronCalc contributors, 2026-present IronCalc GmbH.
//!
//! The function names, the branch order and the error values are the reference's. Adapter changes
//! only: a `Node` is an [`Expr`], a reference is what `extract_range_and_sheet_ctx` resolves, the
//! sheet of a cell is read from the workbook of the evaluation context. Changes beyond the adapter:
//! `CELL` types that need a file name or a format are `#N/A` (the engine does not carry them to a
//! formula), the info types that need a directory, a release number or a time zone list are `#N/A`,
//! `SHEETS` with a reference counts the one sheet the reference is on, an array an earlier step made
//! is no reference.

use crate::{
    EvalContext, EvaluatedCellValue as V, Expr, FormulaError, PackedCellCoordinate, SheetEngine,
    WorkbookEngine,
};

type CellResult = Result<V, FormulaError>;

impl SheetEngine {
    pub(crate) fn eval_info_ref_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        match name {
            "ISREF" => self.fn_isref(args, ctx, depth),
            "SHEET" => self.fn_sheet(args, ctx, depth),
            "SHEETS" => self.fn_sheets(args, ctx, depth),
            "CELL" => self.fn_cell(args, ctx, depth),
            "INFO" => self.fn_info(args, ctx, depth),
            _ => Err(FormulaError::Name),
        }
    }

    /// The 1-based position of the sheet `name` in the workbook of `ctx`.
    fn sheet_position(&self, name: &str, ctx: &EvalContext<'_>) -> Option<usize> {
        let wb = ctx.workbook?;
        wb.sheet_names()
            .iter()
            .position(|n| WorkbookEngine::is_same_sheet_identity(name, n))
            .map(|i| i + 1)
    }

    fn sheet_count(ctx: &EvalContext<'_>) -> usize {
        ctx.workbook.map_or(1, |wb| wb.sheet_count())
    }

    // Returns true if it is a reference or evaluates to a reference
    // But DOES NOT evaluate
    pub(crate) fn fn_isref(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        if args.len() != 1 {
            return Err(FormulaError::Value);
        }
        Ok(V::Boolean(self.gives_reference(&args[0], ctx, depth)))
    }

    /// Whether what `node` gives is a reference, as ISREF sees it: a cell, a range, `OFFSET`,
    /// `INDIRECT`, `INDEX` of a reference, `CHOOSE` of references, or a name for one. An array an
    /// earlier step made stands in a virtual sheet: it is a value, not a reference. An error inside
    /// the argument makes it no reference.
    fn gives_reference(&self, node: &Expr, ctx: &mut EvalContext<'_>, depth: usize) -> bool {
        match self.extract_range_and_sheet_ctx(node, ctx, depth) {
            Ok(Some((sheet, _, _))) => sheet
                .as_deref()
                .is_none_or(|s| crate::array::virtual_sheet_index(s).is_none()),
            _ => false,
        }
    }

    pub(crate) fn fn_sheet(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        let arg_count = args.len();
        if arg_count > 1 {
            return Err(FormulaError::Value);
        }
        if arg_count == 0 {
            return self
                .sheet_position(&self.name, ctx)
                .map(|n| V::Number(n as f64))
                .ok_or(FormulaError::Na);
        }
        // The arg could be a reference (any cell of the sheet) or the name of a sheet
        match self.extract_range_and_sheet_ctx(&args[0], ctx, depth) {
            Ok(Some((sheet, _, _))) => {
                if let Some(n) = self.sheet_position(sheet.as_deref().unwrap_or(&self.name), ctx) {
                    return Ok(V::Number(n as f64));
                }
            }
            Err(e) if !matches!(args[0].peeled(), Expr::Text(_)) => return Err(e),
            _ => {
                // Now it should be the name of a sheet
                let sheet_name = match self.eval_arg_val(&args[0], ctx, depth)? {
                    V::Text(s) => s,
                    other => other.to_display_string(),
                };
                if let Some(n) = self.sheet_position(&sheet_name, ctx) {
                    return Ok(V::Number(n as f64));
                }
            }
        }
        // Invalid name
        Err(FormulaError::Na)
    }

    pub(crate) fn fn_sheets(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        let arg_count = args.len();
        if arg_count > 1 {
            return Err(FormulaError::Value);
        }
        if arg_count == 1 {
            // The reference engine does not implement an argument; a reference is on the one sheet.
            return if self.gives_reference(&args[0], ctx, depth) {
                Ok(V::Number(1.0))
            } else {
                Err(FormulaError::Value)
            };
        }
        Ok(V::Number(Self::sheet_count(ctx) as f64))
    }

    /// CELL(info_type, [reference])
    /// NB: In the reference app "info_type" is localized. Here it is always in English.
    pub(crate) fn fn_cell(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        let arg_count = args.len();
        if arg_count == 0 || arg_count > 2 {
            return Err(FormulaError::Value);
        }
        let (sheet, reference) = if arg_count == 2 {
            // we just take the top-left cell of the reference
            let (sheet, range, _) = self
                .extract_range_and_sheet_ctx(&args[1], ctx, depth)?
                .ok_or(FormulaError::Value)?;
            let corner = PackedCellCoordinate::new(
                range.start.row().min(range.end.row()),
                range.start.col().min(range.end.col()),
            )
            .map_err(|_| FormulaError::Ref)?;
            (sheet.map(|s| s.into_owned()), corner)
        } else {
            (None, ctx.current_coord.ok_or(FormulaError::Value)?)
        };
        let info_type = match self.eval_arg_val(&args[0], ctx, depth)? {
            V::Text(s) => s.to_uppercase(),
            other => other.to_display_string().to_uppercase(),
        };
        match info_type.as_str() {
            "ADDRESS" => {
                if sheet.is_some() {
                    // References to other sheets not implemented
                    return Err(FormulaError::Na);
                }
                let column = PackedCellCoordinate::format_col_label(reference.col());
                Ok(V::Text(format!("${}${}", column, reference.row() + 1)))
            }
            "COL" => Ok(V::Number(f64::from(reference.col()) + 1.0)),
            "COLOR" | "FORMAT" | "PARENTHESES" | "PREFIX" | "PROTECT" | "WIDTH" => {
                // info_type not implemented
                Err(FormulaError::Na)
            }
            "CONTENTS" => Ok(self.eval_sheet_coord(sheet.as_deref(), reference, ctx, depth)),
            // The file name of the workbook is not known to a formula
            "FILENAME" => Err(FormulaError::Na),
            "ROW" => Ok(V::Number(f64::from(reference.row()) + 1.0)),
            "TYPE" => {
                let cell_type = match self.eval_sheet_coord(sheet.as_deref(), reference, ctx, depth)
                {
                    V::Empty => "b",
                    V::Text(_) => "l",
                    V::Number(_) | V::Boolean(_) | V::Error(_) => "v",
                };
                Ok(V::Text(cell_type.to_string()))
            }
            // Invalid info_type
            _ => Err(FormulaError::Value),
        }
    }

    /// INFO(text_type)
    /// NB: In the reference app "text_type" is localized. Here it is always in English.
    pub(crate) fn fn_info(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        if args.len() != 1 {
            return Err(FormulaError::Value);
        }
        let type_text = match self.eval_arg_val(&args[0], ctx, depth)? {
            V::Text(s) => s.to_uppercase(),
            other => other.to_display_string().to_uppercase(),
        };
        match type_text.as_str() {
            // type_text not implemented
            "DIRECTORY" | "ORIGIN" | "OSVERSION" | "RELEASE" | "TIMEZONE" | "TIMEZONES" => {
                Err(FormulaError::Na)
            }

            // For now we just show the count of sheets in the current workbook
            "NUMFILE" => Ok(V::Number(Self::sheet_count(ctx) as f64)),

            // At the moment we always do automatic recalc
            "RECALC" => Ok(V::Text("Automatic".to_string())),
            "SYSTEM" => Ok(V::Text(
                if cfg!(target_os = "macos") {
                    "mac"
                } else {
                    "pcdos"
                }
                .to_string(),
            )),
            // Invalid type_text
            _ => Err(FormulaError::Value),
        }
    }
}

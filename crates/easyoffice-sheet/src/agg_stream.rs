//! Streaming range aggregates (SUM, AVERAGE, MIN, MAX, COUNT*, PRODUCT) over cell
//! ranges: populated cells are folded in a fixed lane order so results stay identical across the
//! dense and sparse walks, and memoized values are borrowed instead of cloned.

use crate::{
    EvalContext, EvaluatedCellValue, Expr, FormulaError, PackedCellCoordinate, SheetEngine,
};

impl SheetEngine {
    pub(crate) fn eval_streaming_range_agg_ctx(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        let mut acc0 = 0.0f64;
        let mut acc1 = 0.0f64;
        let mut acc2 = 0.0f64;
        let mut acc3 = 0.0f64;
        let mut p0 = 1.0f64;
        let mut p1 = 1.0f64;
        let mut p2 = 1.0f64;
        let mut p3 = 1.0f64;
        let mut num_count = 0usize;
        let mut item_count = 0usize;
        let mut min_val: Option<f64> = None;
        let mut max_val: Option<f64> = None;

        let mut fold_cell =
            |v: &EvaluatedCellValue, lane: usize, from_range: bool| -> Result<(), FormulaError> {
                if let EvaluatedCellValue::Error(e) = v {
                    return Err(*e);
                }
                let extract_num = |val: &EvaluatedCellValue| -> Result<Option<f64>, FormulaError> {
                    if from_range {
                        Ok(match val {
                            EvaluatedCellValue::Number(n) => Some(*n),
                            _ => None,
                        })
                    } else {
                        match val {
                            EvaluatedCellValue::Empty => Ok(None),
                            _ => val.as_number().map(Some).ok_or(FormulaError::Value),
                        }
                    }
                };
                match name {
                    "SUM" | "AVERAGE" | "AVG" => {
                        if let Some(n) = extract_num(v)? {
                            num_count += 1;
                            match lane & 3 {
                                0 => acc0 += n,
                                1 => acc1 += n,
                                2 => acc2 += n,
                                _ => acc3 += n,
                            }
                        }
                    }
                    "PRODUCT" => {
                        if let Some(n) = extract_num(v)? {
                            num_count += 1;
                            match lane & 3 {
                                0 => p0 *= n,
                                1 => p1 *= n,
                                2 => p2 *= n,
                                _ => p3 *= n,
                            }
                        }
                    }
                    "MIN" => {
                        if let Some(n) = extract_num(v)? {
                            min_val = Some(min_val.map_or(n, |m| m.min(n)));
                        }
                    }
                    "MAX" => {
                        if let Some(n) = extract_num(v)? {
                            max_val = Some(max_val.map_or(n, |m| m.max(n)));
                        }
                    }
                    "COUNT" => {
                        if matches!(v, EvaluatedCellValue::Number(_)) {
                            item_count += 1;
                        }
                    }
                    "COUNTA" => {
                        if !matches!(v, EvaluatedCellValue::Empty) {
                            item_count += 1;
                        }
                    }
                    "COUNTBLANK"
                        if matches!(v, EvaluatedCellValue::Empty)
                            || matches!(v, EvaluatedCellValue::Text(s) if s.is_empty()) =>
                    {
                        item_count += 1;
                    }
                    _ => {}
                }
                Ok(())
            };

        for arg in args {
            match arg {
                Expr::Range(range) => {
                    let min_r = range.start.row().min(range.end.row());
                    let max_r = range.start.row().max(range.end.row());
                    let min_c = range.start.col().min(range.end.col());
                    let max_c = range.start.col().max(range.end.col());
                    let total = range.cell_count();
                    if total > 1024 && total > self.cells.len().saturating_mul(2) {
                        let mut pop: Vec<PackedCellCoordinate> = self
                            .cells
                            .keys()
                            .copied()
                            .filter(|c| {
                                c.row() >= min_r
                                    && c.row() <= max_r
                                    && c.col() >= min_c
                                    && c.col() <= max_c
                            })
                            .collect();
                        pop.sort_unstable();
                        let pop_len = pop.len();
                        for (idx, coord) in pop.into_iter().enumerate() {
                            self.with_coord_value(coord, ctx, depth, |v| {
                                fold_cell(v, idx & 3, true)
                            })?;
                        }
                        if name == "COUNTBLANK" {
                            for _ in 0..total.saturating_sub(pop_len) {
                                fold_cell(&EvaluatedCellValue::Empty, 0, true)?;
                            }
                        }
                    } else {
                        for r in min_r..=max_r {
                            for c in min_c..=max_c {
                                let coord = PackedCellCoordinate::new(r, c)
                                    .map_err(|_| FormulaError::Ref)?;
                                self.with_coord_value(coord, ctx, depth, |v| {
                                    fold_cell(v, (c as usize) & 3, true)
                                })?;
                            }
                        }
                    }
                }
                Expr::SheetRange { sheet, range } => {
                    let min_r = range.start.row().min(range.end.row());
                    let max_r = range.start.row().max(range.end.row());
                    let min_c = range.start.col().min(range.end.col());
                    let max_c = range.start.col().max(range.end.col());
                    let total = range.cell_count();
                    let sparse_pop = self
                        .resolve_sheet_ref(Some(sheet), ctx)
                        .filter(|ts| total > 1024 && total > ts.cells.len().saturating_mul(2))
                        .map(|ts| {
                            let mut pop: Vec<PackedCellCoordinate> = ts
                                .cells
                                .keys()
                                .copied()
                                .filter(|c| {
                                    c.row() >= min_r
                                        && c.row() <= max_r
                                        && c.col() >= min_c
                                        && c.col() <= max_c
                                })
                                .collect();
                            pop.sort_unstable();
                            pop
                        });
                    if let Some(pop) = sparse_pop {
                        let pop_len = pop.len();
                        for (idx, coord) in pop.into_iter().enumerate() {
                            self.with_sheet_coord_value(Some(sheet), coord, ctx, depth, |v| {
                                fold_cell(v, idx & 3, true)
                            })?;
                        }
                        if name == "COUNTBLANK" {
                            for _ in 0..total.saturating_sub(pop_len) {
                                fold_cell(&EvaluatedCellValue::Empty, 0, true)?;
                            }
                        }
                    } else {
                        for r in min_r..=max_r {
                            for c in min_c..=max_c {
                                let coord = PackedCellCoordinate::new(r, c)
                                    .map_err(|_| FormulaError::Ref)?;
                                self.with_sheet_coord_value(Some(sheet), coord, ctx, depth, |v| {
                                    fold_cell(v, (c as usize) & 3, true)
                                })?;
                            }
                        }
                    }
                }
                Expr::Cell(_) | Expr::SheetCell { .. } => {
                    let v = self.eval_expr_ctx(arg, ctx, depth);
                    fold_cell(&v, 0, true)?;
                }
                Expr::FuncCall { name: fn_name, .. }
                    if matches!(fn_name.as_str(), "INDIRECT" | "OFFSET" | "TRANSPOSE") =>
                {
                    if let Some(spec) = self.extract_streaming_range_ctx(arg, ctx, depth)? {
                        let (ref sheet_opt, min_r, min_c, rows, cols, transposed) = spec;
                        let total = rows.saturating_mul(cols);
                        let sparse_pop = (!transposed)
                            .then(|| self.resolve_sheet_ref(sheet_opt.as_deref(), ctx))
                            .flatten()
                            .filter(|ts| total > 1024 && total > ts.cells.len().saturating_mul(2))
                            .map(|ts| {
                                let max_r = min_r + (rows as u32).saturating_sub(1);
                                let max_c = min_c + (cols as u32).saturating_sub(1);
                                let mut pop: Vec<PackedCellCoordinate> = ts
                                    .cells
                                    .keys()
                                    .copied()
                                    .filter(|c| {
                                        c.row() >= min_r
                                            && c.row() <= max_r
                                            && c.col() >= min_c
                                            && c.col() <= max_c
                                    })
                                    .collect();
                                pop.sort_unstable();
                                pop
                            });
                        if let Some(pop) = sparse_pop {
                            let pop_len = pop.len();
                            for (idx, coord) in pop.into_iter().enumerate() {
                                self.with_sheet_coord_value(
                                    sheet_opt.as_deref(),
                                    coord,
                                    ctx,
                                    depth,
                                    |v| fold_cell(v, idx & 3, true),
                                )?;
                            }
                            if name == "COUNTBLANK" {
                                for _ in 0..total.saturating_sub(pop_len) {
                                    fold_cell(&EvaluatedCellValue::Empty, 0, true)?;
                                }
                            }
                        } else {
                            for dr in 0..rows {
                                for dc in 0..cols {
                                    let v = self.eval_streaming_cell_allow_error(
                                        &spec, dr, dc, ctx, depth,
                                    )?;
                                    fold_cell(&v, (dr * cols + dc) & 3, true)?;
                                }
                            }
                        }
                    } else {
                        let v = self.eval_expr_ctx(arg, ctx, depth);
                        fold_cell(&v, 0, false)?;
                    }
                }
                _ => {
                    let v = self.eval_expr_ctx(arg, ctx, depth);
                    fold_cell(&v, 0, false)?;
                }
            }
        }

        let total_sum = (acc0 + acc1) + (acc2 + acc3);
        match name {
            "SUM" => Ok(EvaluatedCellValue::Number(total_sum)),
            "PRODUCT" => Ok(EvaluatedCellValue::Number(if num_count == 0 {
                0.0
            } else {
                (p0 * p1) * (p2 * p3)
            })),
            "AVERAGE" | "AVG" => {
                if num_count == 0 {
                    Err(FormulaError::DivByZero)
                } else {
                    Ok(EvaluatedCellValue::Number(total_sum / (num_count as f64)))
                }
            }
            "MIN" => Ok(EvaluatedCellValue::Number(min_val.unwrap_or(0.0))),
            "MAX" => Ok(EvaluatedCellValue::Number(max_val.unwrap_or(0.0))),
            "COUNT" | "COUNTA" | "COUNTBLANK" => Ok(EvaluatedCellValue::Number(item_count as f64)),
            _ => Err(FormulaError::Name),
        }
    }
}

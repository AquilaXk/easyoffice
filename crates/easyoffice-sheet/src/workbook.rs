use crate::{
    CellRange, EvalContext, EvaluatedCellValue, FormulaError, PackedCellCoordinate,
    PivotAggregation, PivotTableSummary, SheetEngine,
};
use easyoffice_formats::EasyOfficeError;
use std::cmp::Ordering;
use std::collections::HashMap;

/// Multi-sheet workbook engine supporting cross-sheet references (`=Sheet2!A1`, `=SUM(Sheet2!A1:A3)`)
/// and cross-sheet cycle detection.
#[derive(Debug, Clone, Default)]
pub struct WorkbookEngine {
    sheets: Vec<SheetEngine>,
}

impl WorkbookEngine {
    pub fn new() -> Self {
        Self { sheets: Vec::new() }
    }

    pub fn unquote_formula_sheet_token(s: &str) -> Option<String> {
        if s.len() < 2 || !s.starts_with('\'') || !s.ends_with('\'') {
            return None;
        }
        let inner = &s[1..s.len() - 1];
        let bytes = inner.as_bytes();
        let mut out = String::with_capacity(inner.len());
        let mut i = 0usize;
        while i < bytes.len() {
            if let Some(rel) = bytes[i..].iter().position(|&b| b == b'\'') {
                if rel > 0 {
                    out.push_str(&inner[i..i + rel]);
                    i += rel;
                }
                if bytes.get(i + 1) == Some(&b'\'') {
                    out.push('\'');
                    i += 2;
                } else {
                    return None;
                }
            } else {
                out.push_str(&inner[i..]);
                break;
            }
        }
        Some(out)
    }

    pub fn canonical_sheet_key(raw: &str) -> String {
        if raw.trim().is_empty() {
            String::new()
        } else {
            raw.to_ascii_uppercase()
        }
    }

    /// Same identity as comparing canonical_sheet_keys, without allocating.
    pub fn is_same_sheet_identity(a: &str, b: &str) -> bool {
        !a.trim().is_empty() && a.eq_ignore_ascii_case(b)
    }

    pub(crate) fn find_sheet_index(&self, name: &str) -> Option<usize> {
        if name.trim().is_empty() {
            return None;
        }
        self.sheets
            .iter()
            .position(|s| !s.name.trim().is_empty() && s.name.eq_ignore_ascii_case(name))
    }

    pub fn add_sheet(&mut self, name: impl Into<String>) -> &mut SheetEngine {
        let raw = name.into();
        let norm = if raw.trim().is_empty() {
            "Sheet1"
        } else {
            raw.as_str()
        };
        if let Some(pos) = self.find_sheet_index(norm) {
            return &mut self.sheets[pos];
        }
        let mut engine = SheetEngine::new(norm);
        while engine.sheet_id == 0 || self.sheets.iter().any(|s| s.sheet_id == engine.sheet_id) {
            engine.sheet_id = engine.sheet_id.wrapping_add(1);
            if engine.sheet_id == 0 {
                engine.sheet_id = 1;
            }
        }
        engine.has_workbook_assigned_id = true;
        self.sheets.push(engine);
        self.sheets.last_mut().unwrap()
    }

    pub fn sheet(&self, name: &str) -> Option<&SheetEngine> {
        self.find_sheet_index(name).map(|pos| &self.sheets[pos])
    }

    pub fn sheet_mut(&mut self, name: &str) -> Option<&mut SheetEngine> {
        self.find_sheet_index(name)
            .map(move |pos| &mut self.sheets[pos])
    }

    pub(crate) fn sheets(&self) -> &[SheetEngine] {
        &self.sheets
    }

    pub(crate) fn sheets_mut(&mut self) -> &mut [SheetEngine] {
        &mut self.sheets
    }

    pub fn sheet_count(&self) -> usize {
        self.sheets.len()
    }

    pub fn sheet_names(&self) -> Vec<&str> {
        self.sheets.iter().map(|s| s.name.as_str()).collect()
    }

    pub fn set_cell(
        &mut self,
        sheet_name: &str,
        a1: &str,
        raw_value: impl Into<String>,
    ) -> Result<(), EasyOfficeError> {
        let sheet = self.sheet_mut(sheet_name).ok_or_else(|| {
            EasyOfficeError::InvalidArgument(format!("시트를 찾을 수 없습니다: {sheet_name}"))
        })?;
        sheet.set_cell(a1, raw_value)
    }

    pub fn evaluate_cell(&self, sheet_name: &str, a1: &str) -> EvaluatedCellValue {
        let Some(sheet) = self.sheet(sheet_name) else {
            return EvaluatedCellValue::Error(FormulaError::Ref);
        };
        let Ok(coord) = PackedCellCoordinate::parse_a1(a1) else {
            return EvaluatedCellValue::Error(FormulaError::Ref);
        };
        let mut ctx = EvalContext::with_workbook(self, sheet.resolved_sheet_id());
        sheet.pre_evaluate_dag_iterative(std::slice::from_ref(&coord), &mut ctx);
        ctx.steps = 0;
        sheet.eval_coord_ctx(coord, &mut ctx, 0)
    }

    pub fn evaluate_formula_on_sheet(&self, sheet_name: &str, formula: &str) -> EvaluatedCellValue {
        self.evaluate_formula_on_sheet_at(sheet_name, None, formula)
    }

    pub fn evaluate_formula_on_sheet_at(
        &self,
        sheet_name: &str,
        anchor: Option<PackedCellCoordinate>,
        formula: &str,
    ) -> EvaluatedCellValue {
        let Some(sheet) = self.sheet(sheet_name) else {
            return EvaluatedCellValue::Error(FormulaError::Ref);
        };
        let mut ctx = EvalContext::with_workbook(self, sheet.resolved_sheet_id());
        ctx.current_coord = anchor;
        sheet.eval_raw_string_ctx(formula, &mut ctx, 0)
    }

    /// Evaluates many targets (cell addresses or `=` formulas) in one shared memoized pass, so
    /// formula cells referenced by several targets are computed once, and reports whether any
    /// circular reference was encountered.
    pub fn evaluate_targets(&self, targets: &[WorkbookEvalTarget<'_>]) -> WorkbookBatchEvaluation {
        let primary_id = targets
            .first()
            .and_then(|t| self.sheet(t.sheet))
            .map_or(0, SheetEngine::resolved_sheet_id);
        let mut ctx = EvalContext::with_workbook(self, primary_id);

        if targets.len() > 1 {
            let mut roots_by_sheet: Vec<Vec<PackedCellCoordinate>> =
                vec![Vec::new(); self.sheets.len()];
            for target in targets {
                if let Some(idx) = self.find_sheet_index(target.sheet) {
                    let expr = target.expr.trim();
                    if crate::parser::strip_open_formula_prefix(expr).is_none() {
                        if let Ok(coord) = PackedCellCoordinate::parse_a1(expr) {
                            roots_by_sheet[idx].push(coord);
                        }
                    }
                }
            }
            for (idx, roots) in roots_by_sheet.into_iter().enumerate() {
                if !roots.is_empty() {
                    self.sheets[idx].pre_evaluate_dag_iterative(&roots, &mut ctx);
                }
            }
        }

        let mut values = Vec::with_capacity(targets.len());
        for target in targets {
            let Some(sheet) = self.sheet(target.sheet) else {
                values.push(EvaluatedCellValue::Error(FormulaError::Ref));
                continue;
            };
            ctx.steps = 0;
            let expr = target.expr.trim();
            let val = if crate::parser::strip_open_formula_prefix(expr).is_some() {
                ctx.current_coord = target.anchor;
                let v = sheet.eval_raw_string_ctx(expr, &mut ctx, 0);
                ctx.current_coord = None;
                v
            } else if let Ok(coord) = PackedCellCoordinate::parse_a1(expr) {
                if !ctx.memo.contains_at(sheet.resolved_sheet_id(), coord) {
                    sheet.pre_evaluate_dag_iterative(std::slice::from_ref(&coord), &mut ctx);
                }
                ctx.steps = 0;
                sheet.eval_coord_ctx(coord, &mut ctx, 0)
            } else {
                EvaluatedCellValue::Error(FormulaError::Ref)
            };
            values.push(val);
        }
        WorkbookBatchEvaluation {
            values,
            circular: ctx.circular,
        }
    }
}

/// One `evaluate_targets` request: a cell address or `=` formula evaluated on `sheet`,
/// with an optional anchor cell for `ROW()`/`COLUMN()` in formulas.
#[derive(Debug, Clone, Copy)]
pub struct WorkbookEvalTarget<'a> {
    pub sheet: &'a str,
    pub expr: &'a str,
    pub anchor: Option<PackedCellCoordinate>,
}

/// Values for each `WorkbookEvalTarget` plus a circular-reference flag (cycles display `#NUM!`).
#[derive(Debug, Clone, PartialEq)]
pub struct WorkbookBatchEvaluation {
    pub values: Vec<EvaluatedCellValue>,
    pub circular: bool,
}

impl SheetEngine {
    /// Sorts all populated columns in `start_row..=end_row` (0-indexed) by `primary_col`
    /// and optional `secondary_col`.
    pub fn sort_rows(
        &mut self,
        start_row: u32,
        end_row: u32,
        primary_col: u32,
        primary_asc: bool,
        secondary_col: Option<u32>,
        secondary_asc: bool,
    ) -> Result<(), EasyOfficeError> {
        if start_row > end_row {
            return Err(EasyOfficeError::InvalidArgument(
                "정렬 시작 행이 끝 행보다 클 수 없습니다.".to_string(),
            ));
        }
        if end_row >= PackedCellCoordinate::MAX_ROWS
            || primary_col >= PackedCellCoordinate::MAX_COLS
        {
            return Err(EasyOfficeError::InvalidArgument(
                "정렬 행 또는 열 인덱스가 허용 범위를 초과했습니다.".to_string(),
            ));
        }
        if let Some(sc) = secondary_col {
            if sc >= PackedCellCoordinate::MAX_COLS {
                return Err(EasyOfficeError::InvalidArgument(
                    "보조 정렬 열 인덱스가 허용 범위를 초과했습니다.".to_string(),
                ));
            }
        }

        #[derive(Clone)]
        struct RowEntry {
            orig_row: u32,
            primary_val: EvaluatedCellValue,
            secondary_val: Option<EvaluatedCellValue>,
            cells: Vec<(u32, String)>,
        }

        let row_span = (end_row - start_row + 1) as usize;
        let mut ctx = EvalContext::standalone_id(self.resolved_sheet_id());
        let mut row_map: HashMap<u32, Vec<(u32, String)>> = HashMap::with_capacity(row_span);
        for (&coord, raw) in &self.cells {
            let r = coord.row();
            if (start_row..=end_row).contains(&r) {
                row_map
                    .entry(r)
                    .or_default()
                    .push((coord.col(), raw.clone()));
            }
        }

        let mut rows_data: Vec<RowEntry> = Vec::with_capacity(row_span);
        for r in start_row..=end_row {
            let p_coord = PackedCellCoordinate::new(r, primary_col)?;
            let primary_val = self.eval_coord_ctx(p_coord, &mut ctx, 0);
            let secondary_val = if let Some(sc) = secondary_col {
                let s_coord = PackedCellCoordinate::new(r, sc)?;
                Some(self.eval_coord_ctx(s_coord, &mut ctx, 0))
            } else {
                None
            };
            let mut row_cells = row_map.remove(&r).unwrap_or_default();
            row_cells.sort_unstable_by_key(|(c, _)| *c);
            rows_data.push(RowEntry {
                orig_row: r,
                primary_val,
                secondary_val,
                cells: row_cells,
            });
        }

        rows_data.sort_by(|a, b| {
            let cmp_p = compare_cell_values(&a.primary_val, &b.primary_val, primary_asc);
            if cmp_p != Ordering::Equal {
                return cmp_p;
            }
            if let (Some(sa), Some(sb)) = (&a.secondary_val, &b.secondary_val) {
                compare_cell_values(sa, sb, secondary_asc)
            } else {
                Ordering::Equal
            }
        });

        self.cells
            .retain(|coord, _| coord.row() < start_row || coord.row() > end_row);

        for (offset, entry) in rows_data.into_iter().enumerate() {
            let target_r = start_row + offset as u32;
            for (col, raw) in entry.cells {
                let adjusted = adjust_formula_row_refs(&raw, entry.orig_row, target_r);
                let coord = PackedCellCoordinate::new(target_r, col)?;
                self.cells.insert(coord, adjusted);
            }
        }
        self.rebuild_compiled_asts();

        Ok(())
    }

    pub fn formatted_cell(&self, a1: &str) -> String {
        let Ok(coord) = PackedCellCoordinate::parse_a1(a1) else {
            return "#REF!".to_string();
        };
        let val = self.evaluate_cell(a1);
        if let EvaluatedCellValue::Number(n) = val {
            if let Some(Ok(crate::Expr::FuncCall { name, .. })) = self.compiled_asts.get(&coord) {
                if matches!(
                    name.as_str(),
                    "DATE" | "EDATE" | "EOMONTH" | "WORKDAY" | "TODAY" | "DATEVALUE" | "오늘"
                ) && (1.0..=2_958_465.999_999).contains(&n)
                {
                    return crate::coercion::format_text_pattern(n, "yyyy-mm-dd");
                }
            }
        }
        val.to_display_string()
    }

    pub fn compute_pivot(
        &self,
        range: &CellRange,
        has_header_row: bool,
        row_field_col_offset: usize,
        value_field_col_offset: usize,
        aggregation: PivotAggregation,
    ) -> PivotTableSummary {
        self.compute_pivot_grid(
            range,
            has_header_row,
            row_field_col_offset,
            None,
            value_field_col_offset,
            aggregation,
        )
    }

    pub fn compute_pivot_grid(
        &self,
        range: &CellRange,
        has_header_row: bool,
        row_field_col_offset: usize,
        column_field_col_offset: Option<usize>,
        value_field_col_offset: usize,
        aggregation: PivotAggregation,
    ) -> PivotTableSummary {
        let (row_count, col_count) = range.dimensions();
        let min_r = range.start.row().min(range.end.row());
        let min_c = range.start.col().min(range.end.col());
        let start_row_idx = if has_header_row { 1 } else { 0 };

        let mut ctx = EvalContext::standalone_id(self.resolved_sheet_id());
        let mut row_keys: Vec<String> = Vec::new();
        let mut row_key_idx: HashMap<String, usize> = HashMap::new();
        let mut col_keys: Vec<String> = Vec::new();
        let mut col_key_idx: HashMap<String, usize> = HashMap::new();
        let mut cell_buckets: HashMap<(usize, usize), Vec<f64>> = HashMap::new();
        let mut row_buckets: Vec<Vec<f64>> = Vec::new();
        let mut col_buckets: Vec<Vec<f64>> = Vec::new();
        let mut all_values: Vec<f64> = Vec::new();

        if row_field_col_offset < col_count && value_field_col_offset < col_count {
            let key_col = min_c + row_field_col_offset as u32;
            let val_col = min_c + value_field_col_offset as u32;
            let opt_col_col = column_field_col_offset
                .filter(|&c| c < col_count)
                .map(|c| min_c + c as u32);
            for r_off in start_row_idx..row_count {
                let row_idx = min_r + r_off as u32;
                let (Ok(kc), Ok(vc)) = (
                    PackedCellCoordinate::new(row_idx, key_col),
                    PackedCellCoordinate::new(row_idx, val_col),
                ) else {
                    continue;
                };
                let (key_str, canonical_row) =
                    normalize_pivot_key(self.eval_coord_ctx(kc, &mut ctx, 0));
                if key_str.is_empty() {
                    continue;
                }
                let r_idx = match row_key_idx.get(&canonical_row) {
                    Some(&idx) => idx,
                    None => {
                        let idx = row_keys.len();
                        row_keys.push(key_str);
                        row_buckets.push(Vec::new());
                        row_key_idx.insert(canonical_row, idx);
                        idx
                    }
                };
                let (col_str, canonical_col) = match opt_col_col {
                    Some(cc_idx) => self.pivot_column_key(row_idx, cc_idx, &mut ctx),
                    None => pivot_total_key(),
                };
                let c_idx = match col_key_idx.get(&canonical_col) {
                    Some(&idx) => idx,
                    None => {
                        let idx = col_keys.len();
                        col_keys.push(col_str);
                        col_buckets.push(Vec::new());
                        col_key_idx.insert(canonical_col, idx);
                        idx
                    }
                };
                let val = self.eval_coord_ctx(vc, &mut ctx, 0);
                if let Some(num) = pivot_numeric_value(&val, aggregation) {
                    cell_buckets.entry((r_idx, c_idx)).or_default().push(num);
                    row_buckets[r_idx].push(num);
                    col_buckets[c_idx].push(num);
                    all_values.push(num);
                }
            }
        }
        if col_keys.is_empty() {
            col_keys.push("Total".to_string());
            col_buckets.push(Vec::new());
        }

        let row_totals: Vec<f64> = row_buckets
            .iter()
            .map(|b| Self::aggregate_slice(b, aggregation))
            .collect();
        let column_totals: Vec<f64> = col_buckets
            .iter()
            .map(|b| Self::aggregate_slice(b, aggregation))
            .collect();
        let matrix: Vec<Vec<f64>> = (0..row_keys.len())
            .map(|r_idx| {
                (0..col_keys.len())
                    .map(|c_idx| {
                        Self::aggregate_slice(
                            cell_buckets.get(&(r_idx, c_idx)).map_or(&[], Vec::as_slice),
                            aggregation,
                        )
                    })
                    .collect()
            })
            .collect();
        let grand_total = Self::aggregate_slice(&all_values, aggregation);

        PivotTableSummary {
            row_keys,
            column_keys: col_keys,
            matrix,
            row_totals,
            column_totals,
            grand_total,
        }
    }

    /// Column key of the pivot row `row_idx` taken from column `cc_idx`.
    fn pivot_column_key(
        &self,
        row_idx: u32,
        cc_idx: u32,
        ctx: &mut EvalContext<'_>,
    ) -> (String, String) {
        let Ok(cc) = PackedCellCoordinate::new(row_idx, cc_idx) else {
            return pivot_total_key();
        };
        let (cs, canon) = normalize_pivot_key(self.eval_coord_ctx(cc, ctx, 0));
        if cs.is_empty() {
            ("(Blank)".to_string(), "(blank)".to_string())
        } else {
            (cs, canon)
        }
    }

    fn aggregate_slice(values: &[f64], agg: PivotAggregation) -> f64 {
        if values.is_empty() {
            return 0.0;
        }
        match agg {
            PivotAggregation::Sum => values.iter().sum(),
            PivotAggregation::Average => values.iter().sum::<f64>() / (values.len() as f64),
            PivotAggregation::Count => values.len() as f64,
            PivotAggregation::Min => values.iter().copied().fold(f64::INFINITY, f64::min),
            PivotAggregation::Max => values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        }
    }

    /// Secant / Newton-Raphson solver up to 100 iterations finding the value of `variable_cell`
    /// that makes `target_cell` evaluate to `target_value` within `1e-6`.
    pub fn goal_seek(
        &mut self,
        target_cell: &str,
        target_value: f64,
        variable_cell: &str,
    ) -> Result<f64, EasyOfficeError> {
        let _target_coord = PackedCellCoordinate::parse_a1(target_cell)?;
        let var_coord = PackedCellCoordinate::parse_a1(variable_cell)?;
        if !target_value.is_finite() {
            return Err(EasyOfficeError::InvalidArgument(
                "목표값은 유한한 실수여야 합니다.".to_string(),
            ));
        }

        let original_raw = self.cells.get(&var_coord).cloned();
        let initial_guess = self.evaluate_cell(variable_cell).as_number().unwrap_or(0.0);

        let eval_at = |sheet: &mut SheetEngine, x: f64| -> Option<f64> {
            sheet.set_coord_number(var_coord, x);
            sheet
                .evaluate_cell(target_cell)
                .as_number()
                .map(|y| y - target_value)
        };

        let mut x0 = initial_guess;
        let Some(f0) = eval_at(self, x0) else {
            restore_cell(self, var_coord, original_raw);
            return Err(EasyOfficeError::FormulaError(
                "목표 셀이 수치로 평가되지 않습니다.".to_string(),
            ));
        };
        if f0.abs() < 1e-6 {
            return Ok(x0);
        }

        let mut x1 = if x0.abs() > 1e-6 { x0 * 1.05 } else { 1.0 };
        let Some(mut f1) = eval_at(self, x1) else {
            restore_cell(self, var_coord, original_raw);
            return Err(EasyOfficeError::FormulaError(
                "목표값 찾기 초기 구간 평가에 실패했습니다.".to_string(),
            ));
        };
        let mut f0 = f0;

        for _ in 0..100 {
            if f1.abs() < 1e-6 {
                let rounded = if (x1 - x1.round()).abs() < 1e-9 {
                    x1.round()
                } else {
                    x1
                };
                self.set_coord_number(var_coord, rounded);
                return Ok(rounded);
            }

            let diff = f1 - f0;
            let next_x = if diff.abs() > 1e-14 {
                x1 - f1 * (x1 - x0) / diff
            } else {
                let eps = 1e-5;
                let Some(f_eps) = eval_at(self, x1 + eps) else {
                    break;
                };
                let deriv = (f_eps - f1) / eps;
                if deriv.abs() < 1e-14 {
                    break;
                }
                x1 - f1 / deriv
            };

            if !next_x.is_finite() {
                break;
            }
            x0 = x1;
            f0 = f1;
            x1 = next_x;
            let Some(next_f) = eval_at(self, x1) else {
                break;
            };
            f1 = next_f;
        }

        if f1.abs() < 1e-6 {
            let rounded = if (x1 - x1.round()).abs() < 1e-9 {
                x1.round()
            } else {
                x1
            };
            self.set_coord_number(var_coord, rounded);
            return Ok(rounded);
        }

        restore_cell(self, var_coord, original_raw);
        Err(EasyOfficeError::FormulaError(
            "100회 반복 내에 목표값 찾기가 수렴하지 않았습니다.".to_string(),
        ))
    }
}

/// Display text and case-folded canonical form of a pivot key cell.
fn normalize_pivot_key(v: EvaluatedCellValue) -> (String, String) {
    let disp = match v {
        EvaluatedCellValue::Number(n) => crate::coercion::format_general_number(n),
        EvaluatedCellValue::Text(s) => match crate::coercion::parse_strict_number(&s) {
            Some(n) => crate::coercion::format_general_number(n),
            None => s,
        },
        other => other.to_display_string(),
    };
    let canon = disp.to_lowercase();
    (disp, canon)
}

fn pivot_total_key() -> (String, String) {
    ("Total".to_string(), "total".to_string())
}

/// Numeric contribution of a pivot value cell (`COUNT` also counts non-empty non-numbers).
fn pivot_numeric_value(val: &EvaluatedCellValue, aggregation: PivotAggregation) -> Option<f64> {
    if let EvaluatedCellValue::Number(num) = val {
        return Some(*num);
    }
    let counts_non_empty = aggregation == PivotAggregation::Count
        && !matches!(val, EvaluatedCellValue::Empty)
        && !val.to_display_string().is_empty();
    counts_non_empty.then_some(1.0)
}

/// Longest column-letter run (exclusive upper bound) that can form an A1 column reference.
const MAX_COL_LETTERS_EXCLUSIVE: usize = 4;

#[inline]
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Consumes an optional `$` at `cur`; returns the new index and whether one was present.
#[inline]
fn take_dollar(bytes: &[u8], cur: usize) -> (usize, bool) {
    if bytes.get(cur) == Some(&b'$') {
        (cur + 1, true)
    } else {
        (cur, false)
    }
}

/// Length of an unquoted sheet-name prefix (`Q1!`, `Sheet2!`) starting at `i`, `!` included, so the
/// name's digits are never read as a row. Non-ASCII bytes belong to the name.
fn sheet_prefix_len(bytes: &[u8], i: usize) -> Option<usize> {
    let run = bytes[i..]
        .iter()
        .take_while(|&&b| is_ident_byte(b) || b == b'.' || !b.is_ascii())
        .count();
    (run > 0 && bytes.get(i + run) == Some(&b'!')).then_some(run + 1)
}

/// Rewrites an A1-style reference starting at `i` into `out`, shifting relative rows by `delta`.
/// Returns the index after the reference, or `None` when `i` does not start a reference.
fn rewrite_cell_ref(raw: &str, i: usize, delta: i64, out: &mut String) -> Option<usize> {
    let bytes = raw.as_bytes();
    let len = bytes.len();
    let (col_start, has_col_dollar) = take_dollar(bytes, i);
    let mut cur = col_start;
    while cur < len
        && bytes[cur].is_ascii_alphabetic()
        && (cur - col_start) < MAX_COL_LETTERS_EXCLUSIVE
    {
        cur += 1;
    }
    let col_len = cur - col_start;
    if !(1..=3).contains(&col_len) {
        return None;
    }
    let (row_start, has_row_dollar) = take_dollar(bytes, cur);
    if !bytes
        .get(row_start)
        .is_some_and(|b| b.is_ascii_digit() && *b != b'0')
    {
        return None;
    }
    cur = row_start;
    while cur < len && bytes[cur].is_ascii_digit() {
        cur += 1;
    }
    if bytes
        .get(cur)
        .is_some_and(|&b| is_ident_byte(b) || b == b'(')
    {
        return None;
    }
    if has_col_dollar {
        out.push('$');
    }
    out.push_str(&raw[col_start..col_start + col_len]);
    if has_row_dollar {
        out.push('$');
        out.push_str(&raw[row_start..cur]);
    } else if let Ok(r_1based) = raw[row_start..cur].parse::<i64>() {
        let new_r = (r_1based + delta).clamp(1, PackedCellCoordinate::MAX_ROWS as i64);
        out.push_str(&new_r.to_string());
    } else {
        out.push_str(&raw[row_start..cur]);
    }
    Some(cur)
}

fn adjust_formula_row_refs(raw: &str, orig_row: u32, target_row: u32) -> String {
    if orig_row == target_row || !raw.trim_start().starts_with('=') {
        return raw.to_string();
    }
    let delta = (target_row as i64) - (orig_row as i64);
    let bytes = raw.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len + 8);
    let mut i = 0usize;
    let mut in_double_quotes = false;
    let mut in_single_quotes = false;

    while i < len {
        let b = bytes[i];
        if b == b'"' && !in_single_quotes {
            in_double_quotes = !in_double_quotes;
        } else if b == b'\'' && !in_double_quotes {
            in_single_quotes = !in_single_quotes;
        } else if !in_double_quotes && !in_single_quotes {
            let prev_is_ident = i > 0 && (is_ident_byte(bytes[i - 1]) || bytes[i - 1] == b'.');
            if let Some(prefix) = (!prev_is_ident)
                .then(|| sheet_prefix_len(bytes, i))
                .flatten()
            {
                out.push_str(&raw[i..i + prefix]);
                i += prefix;
                continue;
            }
            if let Some(next) = (!prev_is_ident)
                .then(|| rewrite_cell_ref(raw, i, delta, &mut out))
                .flatten()
            {
                i = next;
                continue;
            }
        }
        let ch = raw[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn restore_cell(sheet: &mut SheetEngine, coord: PackedCellCoordinate, orig: Option<String>) {
    match orig {
        Some(v) => {
            sheet.cells.insert(coord, v);
        }
        None => {
            sheet.cells.remove(&coord);
        }
    }
    sheet.rebuild_compiled_asts();
}

fn compare_cell_values(a: &EvaluatedCellValue, b: &EvaluatedCellValue, asc: bool) -> Ordering {
    match (a, b) {
        (EvaluatedCellValue::Empty, EvaluatedCellValue::Empty) => Ordering::Equal,
        (EvaluatedCellValue::Empty, _) => Ordering::Greater,
        (_, EvaluatedCellValue::Empty) => Ordering::Less,
        _ => {
            let ord = if let (Some(na), Some(nb)) = (a.as_number(), b.as_number()) {
                na.partial_cmp(&nb).unwrap_or(Ordering::Equal)
            } else {
                a.to_display_string()
                    .to_lowercase()
                    .cmp(&b.to_display_string().to_lowercase())
            };
            if asc {
                ord
            } else {
                ord.reverse()
            }
        }
    }
}

/// Supported chart visualization types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartType {
    Bar,
    Line,
    Pie,
    Scatter,
    Area,
}

/// Normalized data point in `[0.0, 1.0]` coordinate space for SVG/Canvas chart rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartPoint {
    pub label: String,
    pub raw_value: f64,
    pub normalized_x: f64,
    pub normalized_y: f64,
    pub pie_share: f64,
}

/// Computed chart series model with normalized coordinates and SVG path geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartModel {
    pub chart_type: ChartType,
    pub points: Vec<ChartPoint>,
    pub min_value: f64,
    pub max_value: f64,
    pub total_value: f64,
    pub svg_path: String,
}

/// Builder computing normalized `[0.0, 1.0]` chart series and SVG/Canvas geometry.
pub struct ChartSeriesBuilder;

impl ChartSeriesBuilder {
    pub fn build(
        sheet: &SheetEngine,
        chart_type: ChartType,
        label_range: CellRange,
        value_range: CellRange,
    ) -> Result<ChartModel, EasyOfficeError> {
        let label_coords = label_range.expand()?;
        let value_coords = value_range.expand()?;
        if label_coords.is_empty() || label_coords.len() != value_coords.len() {
            return Err(EasyOfficeError::InvalidArgument(
                "차트 레이블 범위와 값 범위의 셀 개수가 일치해야 합니다.".to_string(),
            ));
        }

        let mut ctx = EvalContext::standalone_id(sheet.resolved_sheet_id());
        let mut labels = Vec::with_capacity(label_coords.len());
        let mut values = Vec::with_capacity(value_coords.len());
        for (&lc, &vc) in label_coords.iter().zip(value_coords.iter()) {
            labels.push(sheet.eval_coord_ctx(lc, &mut ctx, 0).to_display_string());
            let v = sheet
                .eval_coord_ctx(vc, &mut ctx, 0)
                .as_number()
                .unwrap_or(0.0);
            values.push(if v.is_finite() { v } else { 0.0 });
        }

        let min_value = values.iter().copied().fold(f64::INFINITY, f64::min);
        let max_value = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let max_abs = min_value.abs().max(max_value.abs());
        let scale = if max_abs > 1e150 && max_abs.is_finite() {
            1.0 / max_abs
        } else {
            1.0
        };
        let total_abs: f64 = values.iter().map(|v| v.abs() * scale).sum();
        let raw_total: f64 = values.iter().sum();
        let total_value = if raw_total.is_finite() {
            raw_total
        } else {
            raw_total.signum() * f64::MAX
        };
        let scaled_base_min = (min_value * scale).min(0.0);
        let scaled_span = ((max_value * scale) - scaled_base_min).max(1e-9);

        let n = values.len();
        let mut points = Vec::with_capacity(n);
        let mut svg_cmds = Vec::with_capacity(n + 2);

        for (i, (label, raw_value)) in labels.into_iter().zip(values).enumerate() {
            let normalized_x = if n <= 1 {
                0.5
            } else {
                (i as f64) / ((n - 1) as f64)
            };
            let raw_ny = ((raw_value * scale) - scaled_base_min) / scaled_span;
            let normalized_y = if raw_ny.is_finite() {
                raw_ny.clamp(0.0, 1.0)
            } else {
                0.5
            };
            let raw_pie = if total_abs > 1e-12 && total_abs.is_finite() {
                (raw_value.abs() * scale) / total_abs
            } else {
                0.0
            };
            let pie_share = if raw_pie.is_finite() {
                raw_pie.clamp(0.0, 1.0)
            } else {
                0.0
            };
            let cmd = if i == 0 { "M" } else { "L" };
            svg_cmds.push(format!("{cmd} {normalized_x:.4} {:.4}", 1.0 - normalized_y));
            points.push(ChartPoint {
                label,
                raw_value,
                normalized_x,
                normalized_y,
                pie_share,
            });
        }

        if chart_type == ChartType::Area && !points.is_empty() {
            svg_cmds.push("L 1.0000 1.0000 L 0.0000 1.0000 Z".to_string());
        }

        Ok(ChartModel {
            chart_type,
            points,
            min_value,
            max_value,
            total_value,
            svg_path: svg_cmds.join(" "),
        })
    }
}

#[cfg(test)]
#[path = "workbook_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "workbook_sort_tests.rs"]
mod sort_tests;

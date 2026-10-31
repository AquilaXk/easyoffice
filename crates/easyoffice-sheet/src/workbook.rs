use crate::{
    CellRange, EvalContext, EvaluatedCellValue, FormulaError, PackedCellCoordinate,
    PivotAggregation, PivotTableSummary, SheetEngine,
};
use easyoffice_formats::EasyOfficeError;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

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

    fn unquote_balanced_single_quotes_once(s: &str) -> Option<String> {
        if s.len() < 2 || !s.starts_with('\'') || !s.ends_with('\'') {
            return None;
        }
        let inner = &s[1..s.len() - 1];
        let bytes = inner.as_bytes();
        let mut out = String::with_capacity(inner.len());
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] == b'\'' {
                if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                    out.push('\'');
                    i += 2;
                } else {
                    return None;
                }
            } else {
                let ch = inner[i..].chars().next()?;
                out.push(ch);
                i += ch.len_utf8();
            }
        }
        Some(out)
    }

    pub fn canonical_sheet_key(raw: &str) -> String {
        let mut cur = raw.trim().to_string();
        while let Some(unquoted) = Self::unquote_balanced_single_quotes_once(&cur) {
            cur = unquoted.trim().to_string();
        }
        cur.make_ascii_uppercase();
        cur
    }

    pub fn is_same_sheet_identity(a: &str, b: &str) -> bool {
        let ka = Self::canonical_sheet_key(a);
        let kb = Self::canonical_sheet_key(b);
        !ka.is_empty() && ka == kb
    }

    fn find_sheet_index(&self, name: &str) -> Option<usize> {
        let key = Self::canonical_sheet_key(name);
        if key.is_empty() {
            return None;
        }
        self.sheets
            .iter()
            .position(|s| Self::canonical_sheet_key(&s.name) == key)
    }

    pub fn add_sheet(&mut self, name: impl Into<String>) -> &mut SheetEngine {
        let raw = name.into();
        let trimmed = raw.trim();
        let norm = if Self::canonical_sheet_key(trimmed).is_empty() {
            "Sheet1"
        } else {
            trimmed
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
        sheet.eval_coord_ctx(coord, &mut ctx, 0)
    }

    pub fn evaluate_formula_on_sheet(&self, sheet_name: &str, formula: &str) -> EvaluatedCellValue {
        let Some(sheet) = self.sheet(sheet_name) else {
            return EvaluatedCellValue::Error(FormulaError::Ref);
        };
        let mut ctx = EvalContext::with_workbook(self, sheet.resolved_sheet_id());
        sheet.eval_raw_string_ctx(formula, &mut ctx, 0)
    }
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
            if r >= start_row && r <= end_row {
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

    pub fn compute_pivot(
        &self,
        range: &CellRange,
        has_header_row: bool,
        row_field_col_offset: usize,
        value_field_col_offset: usize,
        aggregation: PivotAggregation,
    ) -> PivotTableSummary {
        let (row_count, col_count) = range.dimensions();
        let min_r = range.start.row().min(range.end.row());
        let min_c = range.start.col().min(range.end.col());
        let start_row_idx = if has_header_row { 1 } else { 0 };

        let mut ctx = EvalContext::standalone_id(self.resolved_sheet_id());
        let mut row_keys: Vec<String> = Vec::new();
        let mut row_key_set: HashSet<String> = HashSet::new();
        let mut buckets: HashMap<String, Vec<f64>> = HashMap::new();
        let mut all_values: Vec<f64> = Vec::new();

        if row_field_col_offset < col_count && value_field_col_offset < col_count {
            let key_col = min_c + row_field_col_offset as u32;
            let val_col = min_c + value_field_col_offset as u32;
            for r_off in start_row_idx..row_count {
                let row_idx = min_r + r_off as u32;
                if let (Ok(kc), Ok(vc)) = (
                    PackedCellCoordinate::new(row_idx, key_col),
                    PackedCellCoordinate::new(row_idx, val_col),
                ) {
                    let key_str = self.eval_coord_ctx(kc, &mut ctx, 0).to_display_string();
                    if key_str.is_empty() {
                        continue;
                    }
                    if row_key_set.insert(key_str.clone()) {
                        row_keys.push(key_str.clone());
                    }
                    let val = self.eval_coord_ctx(vc, &mut ctx, 0);
                    if let Some(num) = val.as_number() {
                        buckets.entry(key_str).or_default().push(num);
                        all_values.push(num);
                    } else if aggregation == PivotAggregation::Count
                        && !matches!(val, EvaluatedCellValue::Empty)
                        && !val.to_display_string().is_empty()
                    {
                        buckets.entry(key_str).or_default().push(1.0);
                        all_values.push(1.0);
                    }
                }
            }
        }

        let row_totals: Vec<f64> = row_keys
            .iter()
            .map(|k| Self::aggregate_slice(buckets.get(k).map_or(&[], Vec::as_slice), aggregation))
            .collect();
        let matrix: Vec<Vec<f64>> = row_totals.iter().map(|v| vec![*v]).collect();
        let grand_total = Self::aggregate_slice(&all_values, aggregation);

        PivotTableSummary {
            row_keys,
            column_keys: vec!["Total".to_string()],
            matrix,
            row_totals,
            column_totals: vec![grand_total],
            grand_total,
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
        let Some(mut f0) = eval_at(self, x0) else {
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
            out.push('"');
            i += 1;
            continue;
        }
        if b == b'\'' && !in_double_quotes {
            in_single_quotes = !in_single_quotes;
            out.push('\'');
            i += 1;
            continue;
        }
        if !in_double_quotes && !in_single_quotes {
            let prev_is_ident = i > 0
                && (bytes[i - 1].is_ascii_alphanumeric()
                    || bytes[i - 1] == b'_'
                    || bytes[i - 1] == b'.');
            if !prev_is_ident {
                let mut cur = i;
                let has_col_dollar = if cur < len && bytes[cur] == b'$' {
                    cur += 1;
                    true
                } else {
                    false
                };
                let col_start = cur;
                while cur < len && bytes[cur].is_ascii_alphabetic() && (cur - col_start) < 4 {
                    cur += 1;
                }
                let col_len = cur - col_start;
                if (1..=3).contains(&col_len) {
                    let has_row_dollar = if cur < len && bytes[cur] == b'$' {
                        cur += 1;
                        true
                    } else {
                        false
                    };
                    let row_start = cur;
                    if cur < len && bytes[cur].is_ascii_digit() && bytes[cur] != b'0' {
                        while cur < len && bytes[cur].is_ascii_digit() {
                            cur += 1;
                        }
                        let next_is_ident_or_call = cur < len
                            && (bytes[cur].is_ascii_alphanumeric()
                                || bytes[cur] == b'_'
                                || bytes[cur] == b'(');
                        if !next_is_ident_or_call {
                            if has_col_dollar {
                                out.push('$');
                            }
                            out.push_str(&raw[col_start..col_start + col_len]);
                            if has_row_dollar {
                                out.push('$');
                                out.push_str(&raw[row_start..cur]);
                            } else if let Ok(r_1based) = raw[row_start..cur].parse::<i64>() {
                                let new_r = (r_1based + delta)
                                    .clamp(1, PackedCellCoordinate::MAX_ROWS as i64);
                                out.push_str(&new_r.to_string());
                            } else {
                                out.push_str(&raw[row_start..cur]);
                            }
                            i = cur;
                            continue;
                        }
                    }
                }
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
            values.push(
                sheet
                    .eval_coord_ctx(vc, &mut ctx, 0)
                    .as_number()
                    .unwrap_or(0.0),
            );
        }

        let min_value = values.iter().copied().fold(f64::INFINITY, f64::min);
        let max_value = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let total_abs: f64 = values.iter().map(|v| v.abs()).sum();
        let total_value: f64 = values.iter().sum();
        let span = (max_value - min_value.min(0.0)).max(1e-9);
        let base_min = min_value.min(0.0);

        let n = values.len();
        let mut points = Vec::with_capacity(n);
        let mut svg_cmds = Vec::with_capacity(n + 2);

        for (i, (label, raw_value)) in labels.into_iter().zip(values).enumerate() {
            let normalized_x = if n <= 1 {
                0.5
            } else {
                (i as f64) / ((n - 1) as f64)
            };
            let normalized_y = ((raw_value - base_min) / span).clamp(0.0, 1.0);
            let pie_share = if total_abs > 1e-12 {
                (raw_value.abs() / total_abs).clamp(0.0, 1.0)
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

use crate::{
    CellRange, EvalContext, EvaluatedCellValue, FormulaError, PackedCellCoordinate, SheetEngine,
};
use easyoffice_formats::EasyOfficeError;
use std::cmp::Ordering;

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

    pub fn add_sheet(&mut self, name: impl Into<String>) -> &mut SheetEngine {
        let name_str = name.into();
        if let Some(pos) = self
            .sheets
            .iter()
            .position(|s| s.name.eq_ignore_ascii_case(&name_str))
        {
            return &mut self.sheets[pos];
        }
        self.sheets.push(SheetEngine::new(name_str));
        self.sheets.last_mut().unwrap()
    }

    pub fn sheet(&self, name: &str) -> Option<&SheetEngine> {
        let clean = name.trim().trim_matches('\'');
        self.sheets
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(clean))
    }

    pub fn sheet_mut(&mut self, name: &str) -> Option<&mut SheetEngine> {
        let clean = name.trim().trim_matches('\'');
        self.sheets
            .iter_mut()
            .find(|s| s.name.eq_ignore_ascii_case(clean))
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
        let mut ctx = EvalContext::with_workbook(self, &sheet.name);
        sheet.eval_coord_ctx(coord, &mut ctx, 0)
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
            primary_val: EvaluatedCellValue,
            secondary_val: Option<EvaluatedCellValue>,
            cells: Vec<(u32, String)>,
        }

        let mut rows_data: Vec<RowEntry> = Vec::with_capacity((end_row - start_row + 1) as usize);
        for r in start_row..=end_row {
            let p_coord = PackedCellCoordinate::new(r, primary_col)?;
            let primary_val = self.evaluate_cell(&p_coord.to_a1());
            let secondary_val = if let Some(sc) = secondary_col {
                let s_coord = PackedCellCoordinate::new(r, sc)?;
                Some(self.evaluate_cell(&s_coord.to_a1()))
            } else {
                None
            };
            let mut row_cells: Vec<(u32, String)> = self
                .cells
                .iter()
                .filter_map(|(coord, raw)| {
                    if coord.row() == r {
                        Some((coord.col(), raw.clone()))
                    } else {
                        None
                    }
                })
                .collect();
            row_cells.sort_by_key(|(c, _)| *c);
            rows_data.push(RowEntry {
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
                let coord = PackedCellCoordinate::new(target_r, col)?;
                self.cells.insert(coord, raw);
            }
        }

        Ok(())
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
            sheet.cells.insert(var_coord, format!("{x}"));
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
                self.cells.insert(var_coord, format!("{rounded}"));
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
            self.cells.insert(var_coord, format!("{rounded}"));
            return Ok(rounded);
        }

        restore_cell(self, var_coord, original_raw);
        Err(EasyOfficeError::FormulaError(
            "100회 반복 내에 목표값 찾기가 수렴하지 않았습니다.".to_string(),
        ))
    }
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

        let mut labels = Vec::with_capacity(label_coords.len());
        let mut values = Vec::with_capacity(value_coords.len());
        for (lc, vc) in label_coords.iter().zip(value_coords.iter()) {
            labels.push(sheet.evaluate_cell(&lc.to_a1()).to_display_string());
            values.push(sheet.evaluate_cell(&vc.to_a1()).as_number().unwrap_or(0.0));
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
mod tests {
    use super::*;

    #[test]
    fn test_all_122_canonical_functions_evaluated() {
        let names = SheetEngine::supported_function_names();
        assert_eq!(names.len(), 122);

        let mut s = SheetEngine::new("함수검증");
        s.set_cell("A1", "10").unwrap();
        s.set_cell("A2", "20").unwrap();
        s.set_cell("A3", "20").unwrap();
        s.set_cell("A4", "40").unwrap();
        s.set_cell("B1", "1").unwrap();
        s.set_cell("B2", "2").unwrap();
        s.set_cell("B3", "3").unwrap();
        s.set_cell("B4", "4").unwrap();
        s.set_cell("C1", "-1000").unwrap();
        s.set_cell("C2", "400").unwrap();
        s.set_cell("C3", "500").unwrap();
        s.set_cell("C4", "600").unwrap();

        // Math & Trig
        assert_eq!(
            s.evaluate_formula_str("=PRODUCT(A1:A2)"),
            EvaluatedCellValue::Number(200.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=SUMPRODUCT(A1:A4, B1:B4)"),
            EvaluatedCellValue::Number(270.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=SUMIF(A1:A4, \">=20\", B1:B4)"),
            EvaluatedCellValue::Number(9.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=SUMIFS(B1:B4, A1:A4, \">=20\", B1:B4, \">2\")"),
            EvaluatedCellValue::Number(7.0)
        );
        assert_eq!(
            s.evaluate_formula_str(
                "=SIGN(-5)+ROUNDUP(1.21,1)+ROUNDDOWN(1.29,1)+INT(3.9)+TRUNC(4.8)"
            ),
            EvaluatedCellValue::Number(8.5)
        );
        assert_eq!(
            s.evaluate_formula_str("=CEILING(4.2, 2)+FLOOR(4.9, 2)+QUOTIENT(17, 5)"),
            EvaluatedCellValue::Number(13.0)
        );
        assert_eq!(
            s.evaluate_formula_str(
                "=ROUND(LN(EXP(2))+LOG10(100)+LOG(8,2)+SIN(0)+COS(0)+TAN(0)+RAND(), 2)"
            ),
            EvaluatedCellValue::Number(8.5)
        );
        assert!(
            (s.evaluate_formula_str("=PI()").as_number().unwrap() - std::f64::consts::PI).abs()
                < 1e-9
        );

        // Statistical
        assert_eq!(
            s.evaluate_formula_str("=AVERAGEA(A1:A4)"),
            EvaluatedCellValue::Number(22.5)
        );
        assert_eq!(
            s.evaluate_formula_str("=AVERAGEIF(A1:A4, \"20\", B1:B4)"),
            EvaluatedCellValue::Number(2.5)
        );
        assert_eq!(
            s.evaluate_formula_str("=AVERAGEIFS(B1:B4, A1:A4, \">=20\")"),
            EvaluatedCellValue::Number(3.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=MODE(A1:A4)"),
            EvaluatedCellValue::Number(20.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=SMALL(A1:A4, 2)+LARGE(A1:A4, 1)"),
            EvaluatedCellValue::Number(60.0)
        );
        assert_eq!(
            s.evaluate_formula_str(
                "=COUNTBLANK(D1:D3)+COUNTIF(A1:A4, 20)+COUNTIFS(A1:A4, \">=20\", B1:B4, \">=3\")"
            ),
            EvaluatedCellValue::Number(7.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=ROUND(STDEVP(A1:A4)+VAR(A1:A4)+VARP(A1:A4), 2)"),
            EvaluatedCellValue::Number(287.98)
        );
        assert_eq!(
            s.evaluate_formula_str("=RANK(20, A1:A4)+PERCENTILE(A1:A4, 0.5)+QUARTILE(A1:A4, 2)"),
            EvaluatedCellValue::Number(42.0)
        );

        // Logical
        assert_eq!(
            s.evaluate_formula_str("=IFS(A1>50, \"A\", A2=20, \"B\")"),
            EvaluatedCellValue::Text("B".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=XOR(TRUE(), FALSE())"),
            EvaluatedCellValue::Boolean(true)
        );
        assert_eq!(
            s.evaluate_formula_str("=IFNA(MATCH(999, A1:A4, 0), 42)"),
            EvaluatedCellValue::Number(42.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=SWITCH(2, 1, \"일\", 2, \"이\", \"기타\")"),
            EvaluatedCellValue::Text("이".to_string())
        );

        // Lookup & Reference
        assert_eq!(
            s.evaluate_formula_str("=HLOOKUP(10, A1:B2, 2, FALSE)"),
            EvaluatedCellValue::Number(20.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=XLOOKUP(40, A1:A4, B1:B4, 0)"),
            EvaluatedCellValue::Number(4.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=CHOOSE(2, \"가\", \"나\", \"다\")"),
            EvaluatedCellValue::Text("나".to_string())
        );
        assert_eq!(s.evaluate_formula_str("=OFFSET(A1, 2, 1)+ROW(B3)+COLUMN(C1)+ROWS(A1:B4)+COLUMNS(A1:C2)+ROWS(TRANSPOSE(A1:C2))"), EvaluatedCellValue::Number(19.0));

        // Text
        assert_eq!(
            s.evaluate_formula_str("=TEXTJOIN(\"-\", TRUE, \"Easy\", \"\", \"Office\")"),
            EvaluatedCellValue::Text("Easy-Office".to_string())
        );
        assert_eq!(s.evaluate_formula_str("=CONCATENATE(LEFT(\"EasyOffice\", 4), MID(\"EasyOffice\", 5, 3), RIGHT(\"EasyOffice\", 3))"), EvaluatedCellValue::Text("EasyOffice".to_string()));
        assert_eq!(
            s.evaluate_formula_str("=PROPER(\"easy office suite\")"),
            EvaluatedCellValue::Text("Easy Office Suite".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str(
                "=SUBSTITUTE(REPLACE(\"2025-Q1\", 1, 4, \"2026\"), \"Q1\", \"Q2\")"
            ),
            EvaluatedCellValue::Text("2026-Q2".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str(
                "=FIND(\"Office\", \"EasyOffice\")+SEARCH(\"office\", \"EasyOffice\")"
            ),
            EvaluatedCellValue::Number(10.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=EXACT(\"AB\", \"AB\")"),
            EvaluatedCellValue::Boolean(true)
        );
        assert_eq!(
            s.evaluate_formula_str("=REPT(\"A\", 3)&TEXT(12.345, \"0.00\")&CHAR(CODE(\"Z\"))"),
            EvaluatedCellValue::Text("AAA12.35Z".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=VALUE(\"1,250.5\")"),
            EvaluatedCellValue::Number(1250.5)
        );

        // Date & Time
        assert_eq!(
            s.evaluate_formula_str(
                "=YEAR(DATE(2026, 10, 24))+MONTH(DATE(2026, 10, 24))+DAY(DATE(2026, 10, 24))"
            ),
            EvaluatedCellValue::Number(2060.0)
        );
        assert_eq!(
            s.evaluate_formula_str(
                "=HOUR(TIME(14, 30, 45))+MINUTE(TIME(14, 30, 45))+SECOND(TIME(14, 30, 45))"
            ),
            EvaluatedCellValue::Number(89.0)
        );
        assert_eq!(
            s.evaluate_formula_str(
                "=DAYS(DATE(2026, 1, 11), DATE(2026, 1, 1))+MONTH(EDATE(DATE(2026, 1, 15), 2))"
            ),
            EvaluatedCellValue::Number(13.0)
        );

        // Financial
        assert!(
            s.evaluate_formula_str("=PMT(0.05, 10, 1000)")
                .as_number()
                .unwrap()
                < -129.0
        );
        assert!(
            s.evaluate_formula_str("=IPMT(0.05, 1, 10, 1000)+PPMT(0.05, 1, 10, 1000)")
                .as_number()
                .unwrap()
                < -129.0
        );
        assert!(
            s.evaluate_formula_str("=FV(0.05, 10, -100, -1000)")
                .as_number()
                .unwrap()
                > 2800.0
        );
        assert!(
            s.evaluate_formula_str("=PV(0.05, 10, -100)")
                .as_number()
                .unwrap()
                > 770.0
        );
        assert!(s.evaluate_formula_str("=IRR(C1:C4)").as_number().unwrap() > 0.2);
        assert!(
            s.evaluate_formula_str("=RATE(10, -129.5046, 1000)")
                .as_number()
                .unwrap()
                > 0.049
        );
        assert_eq!(
            s.evaluate_formula_str("=ROUND(NPER(0.05, -129.504575, 1000), 1)"),
            EvaluatedCellValue::Number(10.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=SLN(10000, 1000, 9)+DDB(10000, 1000, 5, 1)"),
            EvaluatedCellValue::Number(5000.0)
        );

        // Information
        assert_eq!(s.evaluate_formula_str("=AND(ISNUMBER(1), ISTEXT(\"a\"), ISLOGICAL(TRUE()), ISERR(1/0), ISERROR(#N/A), ISNA(MATCH(99, A1:A4, 0)), ISEVEN(4), ISODD(5))"), EvaluatedCellValue::Boolean(true));
    }

    #[test]
    fn test_workbook_cross_sheet_formulas_and_sort_goal_seek_chart() {
        let mut wb = WorkbookEngine::new();
        wb.add_sheet("Sheet1");
        wb.add_sheet("Sheet2");
        wb.set_cell("Sheet2", "A1", "50").unwrap();
        wb.set_cell("Sheet2", "A2", "70").unwrap();
        wb.set_cell("Sheet2", "A3", "80").unwrap();
        wb.set_cell("Sheet1", "B1", "=Sheet2!A1 * 2").unwrap();
        wb.set_cell("Sheet1", "B2", "=SUM(Sheet2!A1:A3)").unwrap();

        assert_eq!(
            wb.evaluate_cell("Sheet1", "B1"),
            EvaluatedCellValue::Number(100.0)
        );
        assert_eq!(
            wb.evaluate_cell("Sheet1", "B2"),
            EvaluatedCellValue::Number(200.0)
        );

        // Cross-sheet cycle detection
        wb.set_cell("Sheet1", "C1", "=Sheet2!C1 + 1").unwrap();
        wb.set_cell("Sheet2", "C1", "=Sheet1!C1 + 1").unwrap();
        assert_eq!(
            wb.evaluate_cell("Sheet1", "C1"),
            EvaluatedCellValue::Error(FormulaError::Cycle)
        );

        // Sort rows & Goal Seek & ChartSeriesBuilder
        let sheet = wb.sheet_mut("Sheet2").unwrap();
        sheet.set_cell("A1", "30").unwrap();
        sheet.set_cell("B1", "나").unwrap();
        sheet.set_cell("A2", "10").unwrap();
        sheet.set_cell("B2", "가").unwrap();
        sheet.set_cell("A3", "20").unwrap();
        sheet.set_cell("B3", "다").unwrap();
        sheet.sort_rows(0, 2, 0, true, Some(1), true).unwrap();
        assert_eq!(sheet.evaluate_cell("A1"), EvaluatedCellValue::Number(10.0));
        assert_eq!(sheet.evaluate_cell("A3"), EvaluatedCellValue::Number(30.0));

        sheet.set_cell("D1", "5").unwrap();
        sheet.set_cell("D2", "=D1 * 12 + 40").unwrap();
        let solved = sheet.goal_seek("D2", 160.0, "D1").unwrap();
        assert!((solved - 10.0).abs() < 1e-5);
        assert_eq!(sheet.evaluate_cell("D2"), EvaluatedCellValue::Number(160.0));

        let chart = ChartSeriesBuilder::build(
            sheet,
            ChartType::Bar,
            CellRange::parse("B1:B3").unwrap(),
            CellRange::parse("A1:A3").unwrap(),
        )
        .unwrap();
        assert_eq!(chart.points.len(), 3);
        assert!((chart.points[2].normalized_y - 1.0).abs() < 1e-6);
    }
}

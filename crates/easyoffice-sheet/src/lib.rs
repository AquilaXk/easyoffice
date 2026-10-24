use easyoffice_formats::EasyOfficeError;
use std::collections::HashMap;
use std::fmt;

pub mod functions;
mod functions_date_fin;
mod functions_lookup_text;
mod parser;
pub mod workbook;

pub use functions::*;
pub use parser::{
    compile_formula_expr, compute_sheet_id, BinaryOp, Expr, PackedWorkbookCoord, VisitingTracker,
};
pub use workbook::*;

/// Bit-packed 64-bit spreadsheet cell coordinate (`row` in upper 32 bits, `col` in lower 32 bits).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackedCellCoordinate(pub u64);

impl PackedCellCoordinate {
    pub const MAX_ROWS: u32 = 1_000_000;
    pub const MAX_COLS: u32 = 16_384;

    #[inline]
    pub fn new(row: u32, col: u32) -> Result<Self, EasyOfficeError> {
        if row >= Self::MAX_ROWS || col >= Self::MAX_COLS {
            return Err(EasyOfficeError::InvalidArgument(format!(
                "셀 좌표가 허용 범위(row < {}, col < {})를 초과했습니다: ({row}, {col})",
                Self::MAX_ROWS,
                Self::MAX_COLS
            )));
        }
        Ok(Self(((row as u64) << 32) | (col as u64)))
    }

    #[inline]
    pub fn row(self) -> u32 {
        (self.0 >> 32) as u32
    }

    #[inline]
    pub fn col(self) -> u32 {
        (self.0 & 0xFFFF_FFFF) as u32
    }

    pub fn parse_a1(input: &str) -> Result<Self, EasyOfficeError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(EasyOfficeError::InvalidArgument(
                "빈 셀 주소입니다.".to_string(),
            ));
        }

        let mut col_acc: u32 = 0;
        let mut row_acc: u32 = 0;
        let mut letter_count = 0usize;
        let mut digit_count = 0usize;

        for b in trimmed.bytes() {
            if b == b'$' {
                continue;
            }
            let upper = if b.is_ascii_lowercase() { b - 32 } else { b };
            if upper.is_ascii_uppercase() {
                if digit_count > 0 || letter_count >= 3 {
                    return Err(EasyOfficeError::InvalidArgument(format!(
                        "유효하지 않은 A1 셀 주소: {input}"
                    )));
                }
                letter_count += 1;
                col_acc = col_acc * 26 + ((upper - b'A') as u32 + 1);
            } else if b.is_ascii_digit() {
                if letter_count == 0 {
                    return Err(EasyOfficeError::InvalidArgument(format!(
                        "유효하지 않은 A1 셀 주소: {input}"
                    )));
                }
                digit_count += 1;
                row_acc = row_acc
                    .checked_mul(10)
                    .and_then(|v| v.checked_add((b - b'0') as u32))
                    .ok_or_else(|| {
                        EasyOfficeError::InvalidArgument(format!("셀 행 번호 오버플로: {input}"))
                    })?;
            } else {
                return Err(EasyOfficeError::InvalidArgument(format!(
                    "유효하지 않은 A1 셀 주소: {input}"
                )));
            }
        }

        if letter_count == 0 || digit_count == 0 || row_acc == 0 {
            return Err(EasyOfficeError::InvalidArgument(format!(
                "유효하지 않은 A1 셀 주소: {input}"
            )));
        }

        Self::new(row_acc - 1, col_acc - 1)
    }

    pub fn format_col_label(col: u32) -> String {
        let mut idx = col + 1;
        let mut buf = [0u8; 8];
        let mut pos = buf.len();
        while idx > 0 && pos > 0 {
            let rem = ((idx - 1) % 26) as u8;
            pos -= 1;
            buf[pos] = b'A' + rem;
            idx = (idx - 1) / 26;
        }
        String::from_utf8_lossy(&buf[pos..]).into_owned()
    }

    pub fn to_a1(self) -> String {
        format!("{}{}", Self::format_col_label(self.col()), self.row() + 1)
    }
}

impl fmt::Display for PackedCellCoordinate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_a1())
    }
}

/// Rectangular cell range (`start..=end`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellRange {
    pub start: PackedCellCoordinate,
    pub end: PackedCellCoordinate,
}

impl CellRange {
    pub const MAX_EXPANDED_CELL_COUNT: usize = 100_000;
    pub const MAX_EXPANDED_CELLS: usize = Self::MAX_EXPANDED_CELL_COUNT;

    #[inline]
    pub fn cell_count(&self) -> usize {
        let (rows, cols) = self.dimensions();
        rows.saturating_mul(cols)
    }

    pub fn new(
        start: PackedCellCoordinate,
        end: PackedCellCoordinate,
    ) -> Result<Self, EasyOfficeError> {
        let min_r = start.row().min(end.row());
        let max_r = start.row().max(end.row());
        let min_c = start.col().min(end.col());
        let max_c = start.col().max(end.col());
        let rows = (max_r - min_r + 1) as usize;
        let cols = (max_c - min_c + 1) as usize;
        let count = rows.saturating_mul(cols);
        if count > Self::MAX_EXPANDED_CELL_COUNT {
            return Err(EasyOfficeError::InvalidArgument(format!(
                "셀 범위 확장 개수({count})가 상한({})을 초과했습니다.",
                Self::MAX_EXPANDED_CELL_COUNT
            )));
        }
        Ok(Self { start, end })
    }

    pub fn parse(notation: &str) -> Result<Self, EasyOfficeError> {
        if let Some((lhs, rhs)) = notation.split_once(':') {
            let start = PackedCellCoordinate::parse_a1(lhs)?;
            let end = PackedCellCoordinate::parse_a1(rhs)?;
            Self::new(start, end)
        } else {
            let single = PackedCellCoordinate::parse_a1(notation)?;
            Self::new(single, single)
        }
    }

    pub fn expand(&self) -> Result<Vec<PackedCellCoordinate>, EasyOfficeError> {
        let min_r = self.start.row().min(self.end.row());
        let max_r = self.start.row().max(self.end.row());
        let min_c = self.start.col().min(self.end.col());
        let max_c = self.start.col().max(self.end.col());
        let rows = (max_r - min_r + 1) as usize;
        let cols = (max_c - min_c + 1) as usize;
        let total = rows.saturating_mul(cols);
        if total > Self::MAX_EXPANDED_CELL_COUNT {
            return Err(EasyOfficeError::InvalidArgument(
                "셀 범위 확장 한도 초과".to_string(),
            ));
        }
        let mut coords = Vec::with_capacity(total);
        for r in min_r..=max_r {
            for c in min_c..=max_c {
                coords.push(PackedCellCoordinate::new(r, c)?);
            }
        }
        Ok(coords)
    }

    pub fn dimensions(&self) -> (usize, usize) {
        let min_r = self.start.row().min(self.end.row());
        let max_r = self.start.row().max(self.end.row());
        let min_c = self.start.col().min(self.end.col());
        let max_c = self.start.col().max(self.end.col());
        ((max_r - min_r + 1) as usize, (max_c - min_c + 1) as usize)
    }
}

/// Standard spreadsheet formula errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FormulaError {
    DivByZero,
    Value,
    Ref,
    Cycle,
    Name,
    Na,
    Num,
}

impl FormulaError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::DivByZero => "#DIV/0!",
            Self::Value => "#VALUE!",
            Self::Ref => "#REF!",
            Self::Cycle => "#CYCLE!",
            Self::Name => "#NAME?",
            Self::Na => "#N/A",
            Self::Num => "#NUM!",
        }
    }
}

impl fmt::Display for FormulaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.code())
    }
}

/// Evaluated cell value in a worksheet.
#[derive(Debug, Clone, PartialEq)]
pub enum EvaluatedCellValue {
    Empty,
    Number(f64),
    Text(String),
    Boolean(bool),
    Error(FormulaError),
}

impl EvaluatedCellValue {
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(v) => Some(*v),
            Self::Boolean(b) => Some(if *b { 1.0 } else { 0.0 }),
            Self::Text(s) => s.trim().parse::<f64>().ok(),
            Self::Empty | Self::Error(_) => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Boolean(b) => Some(*b),
            Self::Number(n) => Some(*n != 0.0),
            Self::Text(s) => {
                if s.eq_ignore_ascii_case("TRUE") {
                    Some(true)
                } else if s.eq_ignore_ascii_case("FALSE") {
                    Some(false)
                } else {
                    None
                }
            }
            Self::Empty => Some(false),
            Self::Error(_) => None,
        }
    }

    pub fn to_display_string(&self) -> String {
        match self {
            Self::Empty => String::new(),
            Self::Number(n) => {
                if n.fract() == 0.0 {
                    format!("{n:.0}")
                } else {
                    format!("{n}")
                }
            }
            Self::Text(s) => s.clone(),
            Self::Boolean(b) => {
                if *b {
                    "TRUE".to_string()
                } else {
                    "FALSE".to_string()
                }
            }
            Self::Error(e) => e.code().to_string(),
        }
    }
}

impl fmt::Display for EvaluatedCellValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_display_string())
    }
}

/// Conditional formatting predicate rules for worksheet ranges.
#[derive(Debug, Clone, PartialEq)]
pub enum ConditionalFormatRule {
    GreaterThan(f64),
    LessThan(f64),
    Between(f64, f64),
    ContainsText(String),
    DuplicateValues,
}

impl ConditionalFormatRule {
    pub fn matches(&self, value: &EvaluatedCellValue, range_values: &[EvaluatedCellValue]) -> bool {
        match self {
            Self::GreaterThan(threshold) => value.as_number().is_some_and(|v| v > *threshold),
            Self::LessThan(threshold) => value.as_number().is_some_and(|v| v < *threshold),
            Self::Between(low, high) => value.as_number().is_some_and(|v| {
                let min_v = low.min(*high);
                let max_v = low.max(*high);
                v >= min_v && v <= max_v
            }),
            Self::ContainsText(needle) => {
                if needle.is_empty() {
                    return false;
                }
                value
                    .to_display_string()
                    .to_lowercase()
                    .contains(&needle.to_lowercase())
            }
            Self::DuplicateValues => {
                if *value == EvaluatedCellValue::Empty {
                    return false;
                }
                range_values.iter().filter(|item| *item == value).count() > 1
            }
        }
    }
}

/// Cell input data validation rule.
#[derive(Debug, Clone, PartialEq)]
pub enum DataValidationRule {
    NumberRange { min: f64, max: f64 },
    AllowedList(Vec<String>),
    NonEmptyText,
    MaxTextLength(usize),
}

impl DataValidationRule {
    pub fn validate(&self, value: &EvaluatedCellValue) -> bool {
        match self {
            Self::NumberRange { min, max } => {
                value.as_number().is_some_and(|v| v >= *min && v <= *max)
            }
            Self::AllowedList(list) => {
                let disp = value.to_display_string();
                list.iter().any(|item| item.eq_ignore_ascii_case(&disp))
            }
            Self::NonEmptyText => {
                *value != EvaluatedCellValue::Empty && !value.to_display_string().trim().is_empty()
            }
            Self::MaxTextLength(max_len) => {
                *value != EvaluatedCellValue::Empty
                    && value.to_display_string().chars().count() <= *max_len
            }
        }
    }
}

/// Aggregation functions supported by PivotTableSummary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PivotAggregation {
    Sum,
    Average,
    Count,
    Min,
    Max,
}

/// Computed Pivot Table summary over a worksheet range.
#[derive(Debug, Clone, PartialEq)]
pub struct PivotTableSummary {
    pub row_keys: Vec<String>,
    pub column_keys: Vec<String>,
    pub matrix: Vec<Vec<f64>>,
    pub row_totals: Vec<f64>,
    pub column_totals: Vec<f64>,
    pub grand_total: f64,
}

pub(crate) struct EvalContext<'a> {
    pub workbook: Option<&'a WorkbookEngine>,
    pub visiting: VisitingTracker,
    pub memo: HashMap<PackedWorkbookCoord, EvaluatedCellValue>,
}

impl<'a> EvalContext<'a> {
    #[inline]
    pub fn standalone_id(sheet_id: u16) -> Self {
        Self {
            workbook: None,
            visiting: VisitingTracker::new(sheet_id),
            memo: HashMap::new(),
        }
    }

    #[inline]
    pub fn with_workbook(workbook: &'a WorkbookEngine, sheet_id: u16) -> Self {
        Self {
            workbook: Some(workbook),
            visiting: VisitingTracker::new(sheet_id),
            memo: HashMap::new(),
        }
    }
}

/// Spreadsheet calculation engine with $O(V + E)$ cycle detection, pre-compiled ASTs, and DAG memoization.
#[derive(Debug, Clone, Default)]
pub struct SheetEngine {
    pub name: String,
    pub(crate) sheet_id: u16,
    pub(crate) cells: HashMap<PackedCellCoordinate, String>,
    pub(crate) compiled_asts: HashMap<PackedCellCoordinate, Result<Expr, FormulaError>>,
    pub(crate) dense_numbers: Vec<f64>,
    pub conditional_rules: Vec<ConditionalFormatRule>,
    pub validation_rule: Option<DataValidationRule>,
}

impl SheetEngine {
    pub const MAX_RECURSION_DEPTH: usize = 64;
    const DENSE_ROWS: usize = 256;
    const DENSE_COLS: usize = 16;

    pub fn new(name: impl Into<String>) -> Self {
        let name_str = name.into();
        let sheet_id = compute_sheet_id(&name_str);
        Self {
            name: name_str,
            sheet_id,
            cells: HashMap::new(),
            compiled_asts: HashMap::new(),
            dense_numbers: Vec::new(),
            conditional_rules: Vec::new(),
            validation_rule: None,
        }
    }

    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
        self.sheet_id = compute_sheet_id(&self.name);
    }

    #[inline]
    pub fn raw_cells(&self) -> &HashMap<PackedCellCoordinate, String> {
        &self.cells
    }

    #[inline]
    pub(crate) fn resolved_sheet_id(&self) -> u16 {
        if self.sheet_id != 0 {
            self.sheet_id
        } else {
            compute_sheet_id(&self.name)
        }
    }

    fn compile_cell_entry(raw: &str) -> Option<Result<Expr, FormulaError>> {
        let trimmed = raw.trim();
        if let Some(formula_body) = trimmed.strip_prefix('=') {
            let expr_src = formula_body.trim();
            if expr_src.is_empty() {
                None
            } else {
                Some(compile_formula_expr(expr_src))
            }
        } else if let Ok(num) = trimmed.parse::<f64>() {
            if num.is_finite() {
                Some(Ok(Expr::Number(num)))
            } else {
                Some(Err(FormulaError::Num))
            }
        } else if trimmed.eq_ignore_ascii_case("TRUE") {
            Some(Ok(Expr::Boolean(true)))
        } else if trimmed.eq_ignore_ascii_case("FALSE") {
            Some(Ok(Expr::Boolean(false)))
        } else {
            None
        }
    }

    #[inline]
    fn update_dense_number(
        &mut self,
        coord: PackedCellCoordinate,
        compiled: Option<&Result<Expr, FormulaError>>,
    ) {
        let r = coord.row() as usize;
        let c = coord.col() as usize;
        if r < Self::DENSE_ROWS && c < Self::DENSE_COLS {
            let idx = (r << 4) | c;
            if let Some(Ok(Expr::Number(n))) = compiled {
                if self.dense_numbers.is_empty() {
                    self.dense_numbers = vec![f64::NAN; Self::DENSE_ROWS * Self::DENSE_COLS];
                }
                self.dense_numbers[idx] = *n;
            } else if !self.dense_numbers.is_empty() {
                self.dense_numbers[idx] = f64::NAN;
            }
        }
    }

    pub fn set_cell(
        &mut self,
        a1: &str,
        raw_value: impl Into<String>,
    ) -> Result<(), EasyOfficeError> {
        let coord = PackedCellCoordinate::parse_a1(a1)?;
        let val = raw_value.into();
        if val.is_empty() {
            self.cells.remove(&coord);
            self.compiled_asts.remove(&coord);
            self.update_dense_number(coord, None);
        } else {
            let compiled = Self::compile_cell_entry(&val);
            self.update_dense_number(coord, compiled.as_ref());
            if let Some(comp) = compiled {
                self.compiled_asts.insert(coord, comp);
            } else {
                self.compiled_asts.remove(&coord);
            }
            self.cells.insert(coord, val);
        }
        Ok(())
    }

    #[inline]
    pub(crate) fn set_coord_number(&mut self, coord: PackedCellCoordinate, num: f64) {
        let comp = Ok(Expr::Number(num));
        self.update_dense_number(coord, Some(&comp));
        self.compiled_asts.insert(coord, comp);
        self.cells.insert(coord, format!("{num}"));
    }

    pub(crate) fn rebuild_compiled_asts(&mut self) {
        if self.sheet_id == 0 {
            self.sheet_id = compute_sheet_id(&self.name);
        }
        self.compiled_asts.clear();
        self.dense_numbers.clear();
        let coords_and_compiled: Vec<(PackedCellCoordinate, Option<Result<Expr, FormulaError>>)> =
            self.cells
                .iter()
                .map(|(&coord, raw)| (coord, Self::compile_cell_entry(raw)))
                .collect();
        for (coord, compiled) in coords_and_compiled {
            self.update_dense_number(coord, compiled.as_ref());
            if let Some(comp) = compiled {
                self.compiled_asts.insert(coord, comp);
            }
        }
    }

    pub fn get_raw_cell(&self, a1: &str) -> Option<&str> {
        let coord = PackedCellCoordinate::parse_a1(a1).ok()?;
        self.cells.get(&coord).map(String::as_str)
    }

    pub fn evaluate_cell(&self, a1: &str) -> EvaluatedCellValue {
        let Ok(coord) = PackedCellCoordinate::parse_a1(a1) else {
            return EvaluatedCellValue::Error(FormulaError::Ref);
        };
        let mut ctx = EvalContext::standalone_id(self.resolved_sheet_id());
        self.eval_coord_ctx(coord, &mut ctx, 0)
    }

    /// Evaluates all populated cells in the worksheet in a single shared DAG memoization pass
    /// using pre-compiled ASTs and the inline `VisitingTracker` bitset.
    pub fn recalculate_all(&self) -> HashMap<PackedCellCoordinate, EvaluatedCellValue> {
        let mut ctx = EvalContext::standalone_id(self.resolved_sheet_id());
        let mut results = HashMap::with_capacity(self.cells.len());
        for &coord in self.cells.keys() {
            let val = self.eval_coord_ctx(coord, &mut ctx, 0);
            results.insert(coord, val);
        }
        results
    }

    pub fn evaluate_formula_str(&self, formula: &str) -> EvaluatedCellValue {
        let mut ctx = EvalContext::standalone_id(self.resolved_sheet_id());
        self.eval_raw_string_ctx(formula, &mut ctx, 0)
    }

    #[inline]
    pub(crate) fn eval_sheet_coord(
        &self,
        sheet_opt: Option<&str>,
        coord: PackedCellCoordinate,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> EvaluatedCellValue {
        match sheet_opt {
            None => self.eval_coord_ctx(coord, ctx, depth),
            Some(target_name) => {
                if target_name.eq_ignore_ascii_case(&self.name) {
                    return self.eval_coord_ctx(coord, ctx, depth);
                }
                let Some(wb) = ctx.workbook else {
                    return EvaluatedCellValue::Error(FormulaError::Ref);
                };
                let Some(target_sheet) = wb.sheet(target_name) else {
                    return EvaluatedCellValue::Error(FormulaError::Ref);
                };
                target_sheet.eval_coord_ctx(coord, ctx, depth)
            }
        }
    }

    #[inline]
    pub(crate) fn eval_coord_ctx(
        &self,
        coord: PackedCellCoordinate,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> EvaluatedCellValue {
        if depth > Self::MAX_RECURSION_DEPTH {
            return EvaluatedCellValue::Error(FormulaError::Cycle);
        }
        let r = coord.row() as usize;
        let c = coord.col() as usize;
        if r < Self::DENSE_ROWS && c < Self::DENSE_COLS {
            if let Some(&n) = self.dense_numbers.get((r << 4) | c) {
                if !n.is_nan() {
                    return EvaluatedCellValue::Number(n);
                }
            }
        }
        let Some(compiled) = self.compiled_asts.get(&coord) else {
            return match self.cells.get(&coord) {
                None => EvaluatedCellValue::Empty,
                Some(raw) => {
                    if raw.trim() == "=" {
                        EvaluatedCellValue::Empty
                    } else {
                        EvaluatedCellValue::Text(raw.clone())
                    }
                }
            };
        };
        match compiled {
            Ok(Expr::Number(n)) => return EvaluatedCellValue::Number(*n),
            Ok(Expr::Boolean(b)) => return EvaluatedCellValue::Boolean(*b),
            Err(e) => return EvaluatedCellValue::Error(*e),
            Ok(_) => {}
        }

        let sid = self.resolved_sheet_id();
        let key = PackedWorkbookCoord::new(sid, coord);
        if let Some(cached) = ctx.memo.get(&key) {
            return cached.clone();
        }
        if !ctx.visiting.insert(sid, coord) {
            return EvaluatedCellValue::Error(FormulaError::Cycle);
        }

        let result = match compiled {
            Ok(expr) => self.eval_expr_ctx(expr, ctx, depth + 1),
            Err(e) => EvaluatedCellValue::Error(*e),
        };

        ctx.visiting.remove(sid, coord);
        ctx.memo.insert(key, result.clone());
        result
    }

    pub(crate) fn eval_raw_string_ctx(
        &self,
        raw: &str,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> EvaluatedCellValue {
        if raw.is_empty() {
            return EvaluatedCellValue::Empty;
        }
        let trimmed = raw.trim();
        if let Some(formula_body) = trimmed.strip_prefix('=') {
            let expr_src = formula_body.trim();
            if expr_src.is_empty() {
                return EvaluatedCellValue::Empty;
            }
            match compile_formula_expr(expr_src) {
                Ok(expr) => self.eval_expr_ctx(&expr, ctx, depth),
                Err(e) => EvaluatedCellValue::Error(e),
            }
        } else if let Ok(num) = trimmed.parse::<f64>() {
            if num.is_finite() {
                EvaluatedCellValue::Number(num)
            } else {
                EvaluatedCellValue::Error(FormulaError::Num)
            }
        } else if trimmed.eq_ignore_ascii_case("TRUE") {
            EvaluatedCellValue::Boolean(true)
        } else if trimmed.eq_ignore_ascii_case("FALSE") {
            EvaluatedCellValue::Boolean(false)
        } else {
            EvaluatedCellValue::Text(raw.to_string())
        }
    }

    pub(crate) fn eval_expr_ctx(
        &self,
        expr: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> EvaluatedCellValue {
        if depth > Self::MAX_RECURSION_DEPTH {
            return EvaluatedCellValue::Error(FormulaError::Cycle);
        }
        match expr {
            Expr::Number(n) => EvaluatedCellValue::Number(*n),
            Expr::Text(s) => EvaluatedCellValue::Text(s.clone()),
            Expr::Boolean(b) => EvaluatedCellValue::Boolean(*b),
            Expr::ErrorLiteral(e) => EvaluatedCellValue::Error(*e),
            Expr::Cell(coord) => self.eval_coord_ctx(*coord, ctx, depth + 1),
            Expr::SheetCell { sheet, coord } => {
                self.eval_sheet_coord(Some(sheet), *coord, ctx, depth + 1)
            }
            Expr::Range(_) | Expr::SheetRange { .. } => {
                EvaluatedCellValue::Error(FormulaError::Value)
            }
            Expr::UnaryPlus(inner) => {
                let val = self.eval_expr_ctx(inner, ctx, depth + 1);
                if let EvaluatedCellValue::Error(e) = val {
                    return EvaluatedCellValue::Error(e);
                }
                match val.as_number() {
                    Some(n) => EvaluatedCellValue::Number(n),
                    None => EvaluatedCellValue::Error(FormulaError::Value),
                }
            }
            Expr::UnaryMinus(inner) => {
                let val = self.eval_expr_ctx(inner, ctx, depth + 1);
                if let EvaluatedCellValue::Error(e) = val {
                    return EvaluatedCellValue::Error(e);
                }
                match val.as_number() {
                    Some(n) => EvaluatedCellValue::Number(-n),
                    None => EvaluatedCellValue::Error(FormulaError::Value),
                }
            }
            Expr::PostfixPercent(inner) => {
                let val = self.eval_expr_ctx(inner, ctx, depth + 1);
                if let EvaluatedCellValue::Error(e) = val {
                    return EvaluatedCellValue::Error(e);
                }
                match val.as_number() {
                    Some(n) => EvaluatedCellValue::Number(n / 100.0),
                    None => EvaluatedCellValue::Error(FormulaError::Value),
                }
            }
            Expr::Binary { op, lhs, rhs } => {
                let l_val = self.eval_expr_ctx(lhs, ctx, depth + 1);
                if let EvaluatedCellValue::Error(e) = l_val {
                    return EvaluatedCellValue::Error(e);
                }
                let r_val = self.eval_expr_ctx(rhs, ctx, depth + 1);
                if let EvaluatedCellValue::Error(e) = r_val {
                    return EvaluatedCellValue::Error(e);
                }

                match op {
                    BinaryOp::Concat => EvaluatedCellValue::Text(format!(
                        "{}{}",
                        l_val.to_display_string(),
                        r_val.to_display_string()
                    )),
                    BinaryOp::Add
                    | BinaryOp::Sub
                    | BinaryOp::Mul
                    | BinaryOp::Div
                    | BinaryOp::Pow => {
                        let ln = match l_val {
                            EvaluatedCellValue::Empty => 0.0,
                            _ => match l_val.as_number() {
                                Some(v) => v,
                                None => return EvaluatedCellValue::Error(FormulaError::Value),
                            },
                        };
                        let rn = match r_val {
                            EvaluatedCellValue::Empty => 0.0,
                            _ => match r_val.as_number() {
                                Some(v) => v,
                                None => return EvaluatedCellValue::Error(FormulaError::Value),
                            },
                        };
                        let res = match op {
                            BinaryOp::Add => ln + rn,
                            BinaryOp::Sub => ln - rn,
                            BinaryOp::Mul => ln * rn,
                            BinaryOp::Div => {
                                if rn == 0.0 {
                                    return EvaluatedCellValue::Error(FormulaError::DivByZero);
                                }
                                ln / rn
                            }
                            BinaryOp::Pow => ln.powf(rn),
                            _ => unreachable!(),
                        };
                        if res.is_finite() {
                            EvaluatedCellValue::Number(res)
                        } else {
                            EvaluatedCellValue::Error(FormulaError::Num)
                        }
                    }
                    BinaryOp::Eq
                    | BinaryOp::Ne
                    | BinaryOp::Lt
                    | BinaryOp::Le
                    | BinaryOp::Gt
                    | BinaryOp::Ge => {
                        let ord =
                            if let (Some(ln), Some(rn)) = (l_val.as_number(), r_val.as_number()) {
                                ln.partial_cmp(&rn)
                            } else {
                                Some(
                                    l_val
                                        .to_display_string()
                                        .to_lowercase()
                                        .cmp(&r_val.to_display_string().to_lowercase()),
                                )
                            };
                        let Some(ordering) = ord else {
                            return EvaluatedCellValue::Error(FormulaError::Value);
                        };
                        let flag = match op {
                            BinaryOp::Eq => ordering.is_eq(),
                            BinaryOp::Ne => ordering.is_ne(),
                            BinaryOp::Lt => ordering.is_lt(),
                            BinaryOp::Le => ordering.is_le(),
                            BinaryOp::Gt => ordering.is_gt(),
                            BinaryOp::Ge => ordering.is_ge(),
                            _ => unreachable!(),
                        };
                        EvaluatedCellValue::Boolean(flag)
                    }
                }
            }
            Expr::FuncCall { name, args } => match name.as_str() {
                "SUM" | "AVERAGE" | "AVG" | "MIN" | "MAX" | "COUNT" | "COUNTA" | "COUNTBLANK"
                | "PRODUCT" => {
                    match self.eval_streaming_range_agg_ctx(name, args, ctx, depth + 1) {
                        Ok(v) => v,
                        Err(e) => EvaluatedCellValue::Error(e),
                    }
                }
                _ => self.eval_func_ctx(name, args, ctx, depth + 1),
            },
        }
    }

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

        let mut fold_cell = |v: EvaluatedCellValue, lane: usize| -> Result<(), FormulaError> {
            if let EvaluatedCellValue::Error(e) = v {
                return Err(e);
            }
            match name {
                "SUM" | "AVERAGE" | "AVG" => {
                    if let Some(n) = v.as_number() {
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
                    if let Some(n) = v.as_number() {
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
                    if let Some(n) = v.as_number() {
                        min_val = Some(min_val.map_or(n, |m| m.min(n)));
                    }
                }
                "MAX" => {
                    if let Some(n) = v.as_number() {
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
                        || matches!(&v, EvaluatedCellValue::Text(s) if s.is_empty()) =>
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
                    if range.cell_count() > CellRange::MAX_EXPANDED_CELLS {
                        return Err(FormulaError::Ref);
                    }
                    let min_r = range.start.row().min(range.end.row());
                    let max_r = range.start.row().max(range.end.row());
                    let min_c = range.start.col().min(range.end.col());
                    let max_c = range.start.col().max(range.end.col());
                    for r in min_r..=max_r {
                        let mut c = min_c;
                        while c + 3 <= max_c {
                            let c0 =
                                PackedCellCoordinate::new(r, c).map_err(|_| FormulaError::Ref)?;
                            let c1 = PackedCellCoordinate::new(r, c + 1)
                                .map_err(|_| FormulaError::Ref)?;
                            let c2 = PackedCellCoordinate::new(r, c + 2)
                                .map_err(|_| FormulaError::Ref)?;
                            let c3 = PackedCellCoordinate::new(r, c + 3)
                                .map_err(|_| FormulaError::Ref)?;
                            fold_cell(self.eval_coord_ctx(c0, ctx, depth + 1), 0)?;
                            fold_cell(self.eval_coord_ctx(c1, ctx, depth + 1), 1)?;
                            fold_cell(self.eval_coord_ctx(c2, ctx, depth + 1), 2)?;
                            fold_cell(self.eval_coord_ctx(c3, ctx, depth + 1), 3)?;
                            c += 4;
                        }
                        while c <= max_c {
                            let coord =
                                PackedCellCoordinate::new(r, c).map_err(|_| FormulaError::Ref)?;
                            fold_cell(self.eval_coord_ctx(coord, ctx, depth + 1), 0)?;
                            c += 1;
                        }
                    }
                }
                Expr::SheetRange { sheet, range } => {
                    if range.cell_count() > CellRange::MAX_EXPANDED_CELLS {
                        return Err(FormulaError::Ref);
                    }
                    let min_r = range.start.row().min(range.end.row());
                    let max_r = range.start.row().max(range.end.row());
                    let min_c = range.start.col().min(range.end.col());
                    let max_c = range.start.col().max(range.end.col());
                    for r in min_r..=max_r {
                        let mut c = min_c;
                        while c + 3 <= max_c {
                            let c0 =
                                PackedCellCoordinate::new(r, c).map_err(|_| FormulaError::Ref)?;
                            let c1 = PackedCellCoordinate::new(r, c + 1)
                                .map_err(|_| FormulaError::Ref)?;
                            let c2 = PackedCellCoordinate::new(r, c + 2)
                                .map_err(|_| FormulaError::Ref)?;
                            let c3 = PackedCellCoordinate::new(r, c + 3)
                                .map_err(|_| FormulaError::Ref)?;
                            fold_cell(self.eval_sheet_coord(Some(sheet), c0, ctx, depth + 1), 0)?;
                            fold_cell(self.eval_sheet_coord(Some(sheet), c1, ctx, depth + 1), 1)?;
                            fold_cell(self.eval_sheet_coord(Some(sheet), c2, ctx, depth + 1), 2)?;
                            fold_cell(self.eval_sheet_coord(Some(sheet), c3, ctx, depth + 1), 3)?;
                            c += 4;
                        }
                        while c <= max_c {
                            let coord =
                                PackedCellCoordinate::new(r, c).map_err(|_| FormulaError::Ref)?;
                            fold_cell(
                                self.eval_sheet_coord(Some(sheet), coord, ctx, depth + 1),
                                0,
                            )?;
                            c += 1;
                        }
                    }
                }
                _ => {
                    let v = self.eval_expr_ctx(arg, ctx, depth + 1);
                    fold_cell(v, 0)?;
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

    pub(crate) fn expand_args_ctx(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Vec<EvaluatedCellValue>, FormulaError> {
        let mut out = Vec::new();
        for arg in args {
            match arg {
                Expr::Range(range) => {
                    let coords = range.expand().map_err(|_| FormulaError::Ref)?;
                    for c in coords {
                        let v = self.eval_coord_ctx(c, ctx, depth + 1);
                        if let EvaluatedCellValue::Error(e) = v {
                            return Err(e);
                        }
                        out.push(v);
                    }
                }
                Expr::SheetRange { sheet, range } => {
                    let coords = range.expand().map_err(|_| FormulaError::Ref)?;
                    for c in coords {
                        let v = self.eval_sheet_coord(Some(sheet), c, ctx, depth + 1);
                        if let EvaluatedCellValue::Error(e) = v {
                            return Err(e);
                        }
                        out.push(v);
                    }
                }
                _ => {
                    let v = self.eval_expr_ctx(arg, ctx, depth + 1);
                    if let EvaluatedCellValue::Error(e) = v {
                        return Err(e);
                    }
                    out.push(v);
                }
            }
        }
        Ok(out)
    }

    pub(crate) fn values_match(a: &EvaluatedCellValue, b: &EvaluatedCellValue) -> bool {
        if let (Some(na), Some(nb)) = (a.as_number(), b.as_number()) {
            (na - nb).abs() < 1e-9
        } else {
            a.to_display_string()
                .eq_ignore_ascii_case(&b.to_display_string())
        }
    }
}

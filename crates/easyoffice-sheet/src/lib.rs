use easyoffice_formats::EasyOfficeError;
use std::collections::{HashMap, HashSet};
use std::fmt;

pub mod functions;
mod functions_date_fin;
mod functions_lookup_text;
mod parser;
pub mod workbook;

pub use functions::*;
pub(crate) use parser::*;
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
    pub current_sheet: String,
    pub visiting: HashSet<(String, PackedCellCoordinate)>,
    pub memo: HashMap<(String, PackedCellCoordinate), EvaluatedCellValue>,
}

impl<'a> EvalContext<'a> {
    pub fn standalone(sheet_name: &str) -> Self {
        Self {
            workbook: None,
            current_sheet: sheet_name.to_ascii_lowercase(),
            visiting: HashSet::new(),
            memo: HashMap::new(),
        }
    }

    pub fn with_workbook(workbook: &'a WorkbookEngine, sheet_name: &str) -> Self {
        Self {
            workbook: Some(workbook),
            current_sheet: sheet_name.to_ascii_lowercase(),
            visiting: HashSet::new(),
            memo: HashMap::new(),
        }
    }
}

/// Spreadsheet calculation engine with $O(V + E)$ cycle detection and DAG memoization.
#[derive(Debug, Clone, Default)]
pub struct SheetEngine {
    pub name: String,
    pub(crate) cells: HashMap<PackedCellCoordinate, String>,
    pub conditional_rules: Vec<ConditionalFormatRule>,
    pub validation_rule: Option<DataValidationRule>,
}

impl SheetEngine {
    pub const MAX_RECURSION_DEPTH: usize = 64;

    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            cells: HashMap::new(),
            conditional_rules: Vec::new(),
            validation_rule: None,
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
        } else {
            self.cells.insert(coord, val);
        }
        Ok(())
    }

    pub fn get_raw_cell(&self, a1: &str) -> Option<&str> {
        let coord = PackedCellCoordinate::parse_a1(a1).ok()?;
        self.cells.get(&coord).map(String::as_str)
    }

    pub fn evaluate_cell(&self, a1: &str) -> EvaluatedCellValue {
        let Ok(coord) = PackedCellCoordinate::parse_a1(a1) else {
            return EvaluatedCellValue::Error(FormulaError::Ref);
        };
        let mut ctx = EvalContext::standalone(&self.name);
        self.eval_coord_ctx(coord, &mut ctx, 0)
    }

    pub fn evaluate_formula_str(&self, formula: &str) -> EvaluatedCellValue {
        let mut ctx = EvalContext::standalone(&self.name);
        self.eval_raw_string_ctx(formula, &mut ctx, 0)
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

        let mut ctx = EvalContext::standalone(&self.name);
        let mut row_keys: Vec<String> = Vec::new();
        let mut buckets: HashMap<String, Vec<f64>> = HashMap::new();
        let mut all_values: Vec<f64> = Vec::new();

        for r_off in start_row_idx..row_count {
            if row_field_col_offset >= col_count || value_field_col_offset >= col_count {
                continue;
            }
            let key_coord = PackedCellCoordinate::new(
                min_r + r_off as u32,
                min_c + row_field_col_offset as u32,
            );
            let val_coord = PackedCellCoordinate::new(
                min_r + r_off as u32,
                min_c + value_field_col_offset as u32,
            );
            if let (Ok(kc), Ok(vc)) = (key_coord, val_coord) {
                let key_str = self.eval_coord_ctx(kc, &mut ctx, 0).to_display_string();
                if key_str.is_empty() {
                    continue;
                }
                if !row_keys.contains(&key_str) {
                    row_keys.push(key_str.clone());
                }
                if let Some(num) = self.eval_coord_ctx(vc, &mut ctx, 0).as_number() {
                    buckets.entry(key_str).or_default().push(num);
                    all_values.push(num);
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
                let prev_sheet = std::mem::replace(
                    &mut ctx.current_sheet,
                    target_sheet.name.to_ascii_lowercase(),
                );
                let res = target_sheet.eval_coord_ctx(coord, ctx, depth);
                ctx.current_sheet = prev_sheet;
                res
            }
        }
    }

    pub(crate) fn eval_coord_ctx(
        &self,
        coord: PackedCellCoordinate,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> EvaluatedCellValue {
        if depth > Self::MAX_RECURSION_DEPTH {
            return EvaluatedCellValue::Error(FormulaError::Cycle);
        }
        let key = (self.name.to_ascii_lowercase(), coord);
        if let Some(cached) = ctx.memo.get(&key) {
            return cached.clone();
        }
        if !ctx.visiting.insert(key.clone()) {
            return EvaluatedCellValue::Error(FormulaError::Cycle);
        }

        let result = match self.cells.get(&coord) {
            None => EvaluatedCellValue::Empty,
            Some(raw) => self.eval_raw_string_ctx(raw, ctx, depth + 1),
        };

        ctx.visiting.remove(&key);
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
            let tokens = match tokenize_formula(expr_src) {
                Ok(t) => t,
                Err(e) => return EvaluatedCellValue::Error(e),
            };
            let mut parser = Parser::new(tokens);
            let expr = match parser.parse_expression() {
                Ok(ex) if parser.pos == parser.tokens.len() => ex,
                Ok(_) => return EvaluatedCellValue::Error(FormulaError::Value),
                Err(e) => return EvaluatedCellValue::Error(e),
            };
            self.eval_expr_ctx(&expr, ctx, depth)
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
            Expr::FuncCall { name, args } => self.eval_func_ctx(name, args, ctx, depth + 1),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packed_cell_coordinate_and_a1_roundtrip() {
        let c = PackedCellCoordinate::parse_a1("B12").unwrap();
        assert_eq!(c.row(), 11);
        assert_eq!(c.col(), 1);
        assert_eq!(c.to_a1(), "B12");

        let xfd = PackedCellCoordinate::parse_a1("$XFD$1000000").unwrap();
        assert_eq!(xfd.row(), 999_999);
        assert_eq!(xfd.col(), 16_383);
        assert_eq!(xfd.to_a1(), "XFD1000000");

        assert!(PackedCellCoordinate::new(1_000_000, 0).is_err());
        assert!(PackedCellCoordinate::new(0, 16_384).is_err());
    }

    #[test]
    fn test_formula_evaluation_operators_isblank_and_functions() {
        let mut sheet = SheetEngine::new("재무시트");
        sheet.set_cell("A1", "100").unwrap();
        sheet.set_cell("A2", "200").unwrap();
        sheet.set_cell("A3", "=+$A$1+A2*15%").unwrap();
        sheet.set_cell("B1", "=\"\"").unwrap();
        sheet.set_cell("B2", "=ISBLANK(C1)").unwrap();
        sheet.set_cell("B3", "=ISBLANK(B1)").unwrap();
        sheet.set_cell("D1", "=SUM(A1:A3)").unwrap();
        sheet.set_cell("D2", "=AVERAGE(A1:A2)").unwrap();
        sheet
            .set_cell("D3", "=IF(D1>200, UPPER(\"pass\"), \"fail\")")
            .unwrap();

        assert_eq!(sheet.evaluate_cell("A3"), EvaluatedCellValue::Number(130.0));
        assert_eq!(sheet.evaluate_cell("B2"), EvaluatedCellValue::Boolean(true));
        assert_eq!(
            sheet.evaluate_cell("B3"),
            EvaluatedCellValue::Boolean(false)
        );
        assert_eq!(sheet.evaluate_cell("D1"), EvaluatedCellValue::Number(430.0));
        assert_eq!(sheet.evaluate_cell("D2"), EvaluatedCellValue::Number(150.0));
        assert_eq!(
            sheet.evaluate_cell("D3"),
            EvaluatedCellValue::Text("PASS".to_string())
        );
    }

    #[test]
    fn test_cycle_detection_lookup_npv_and_pivot() {
        let mut sheet = SheetEngine::new("분석시트");
        sheet.set_cell("A1", "=B1+1").unwrap();
        sheet.set_cell("B1", "=A1+1").unwrap();
        assert_eq!(
            sheet.evaluate_cell("A1"),
            EvaluatedCellValue::Error(FormulaError::Cycle)
        );

        sheet.set_cell("A1", "부서").unwrap();
        sheet.set_cell("B1", "매출").unwrap();
        sheet.set_cell("A2", "플랫폼").unwrap();
        sheet.set_cell("B2", "1200").unwrap();
        sheet.set_cell("A3", "코어엔진").unwrap();
        sheet.set_cell("B3", "1800").unwrap();
        sheet.set_cell("A4", "플랫폼").unwrap();
        sheet.set_cell("B4", "800").unwrap();

        sheet
            .set_cell("C1", "=VLOOKUP(\"코어엔진\", A2:B4, 2, FALSE)")
            .unwrap();
        sheet.set_cell("C2", "=INDEX(A2:B4, 2, 2)").unwrap();
        sheet
            .set_cell("C3", "=MATCH(\"코어엔진\", A2:A4, 0)")
            .unwrap();
        sheet.set_cell("C4", "=ROUND(NPV(0.1, B2:B3), 2)").unwrap();
        sheet
            .set_cell("C5", "=SQRT(16)+POWER(2,3)+MOD(10,3)")
            .unwrap();
        sheet.set_cell("C6", "=CONCAT(A2, \"-\", A3)").unwrap();
        sheet.set_cell("C7", "=MEDIAN(B2:B4)").unwrap();
        sheet.set_cell("C8", "=ROUND(STDEV(B2:B4), 2)").unwrap();
        sheet
            .set_cell("C9", "=VLOOKUP(\"코어엔진\", A2:B4, 1/0, FALSE)")
            .unwrap();

        assert_eq!(
            sheet.evaluate_cell("C1"),
            EvaluatedCellValue::Number(1800.0)
        );
        assert_eq!(
            sheet.evaluate_cell("C2"),
            EvaluatedCellValue::Number(1800.0)
        );
        assert_eq!(sheet.evaluate_cell("C3"), EvaluatedCellValue::Number(2.0));
        assert_eq!(
            sheet.evaluate_cell("C4"),
            EvaluatedCellValue::Number(2578.51)
        );
        assert_eq!(sheet.evaluate_cell("C5"), EvaluatedCellValue::Number(13.0));
        assert_eq!(
            sheet.evaluate_cell("C6"),
            EvaluatedCellValue::Text("플랫폼-코어엔진".to_string())
        );
        assert_eq!(
            sheet.evaluate_cell("C7"),
            EvaluatedCellValue::Number(1200.0)
        );
        assert_eq!(
            sheet.evaluate_cell("C8"),
            EvaluatedCellValue::Number(503.32)
        );
        assert_eq!(
            sheet.evaluate_cell("C9"),
            EvaluatedCellValue::Error(FormulaError::DivByZero)
        );

        let range = CellRange::parse("A1:B4").unwrap();
        let pivot = sheet.compute_pivot(&range, true, 0, 1, PivotAggregation::Sum);
        assert_eq!(
            pivot.row_keys,
            vec!["플랫폼".to_string(), "코어엔진".to_string()]
        );
        assert_eq!(pivot.row_totals, vec![2000.0, 1800.0]);
        assert_eq!(pivot.grand_total, 3800.0);
    }
}

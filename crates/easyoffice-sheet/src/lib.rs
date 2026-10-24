use easyoffice_formats::EasyOfficeError;
use std::collections::{HashMap, HashSet};
use std::fmt;

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
                        EasyOfficeError::InvalidArgument(format!(
                            "셀 행 번호 오버플로: {input}"
                        ))
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

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Text(String),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Percent,
    Ampersand,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    LParen,
    RParen,
    Comma,
    Colon,
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Number(f64),
    Text(String),
    Boolean(bool),
    Cell(PackedCellCoordinate),
    Range(CellRange),
    UnaryPlus(Box<Expr>),
    UnaryMinus(Box<Expr>),
    PostfixPercent(Box<Expr>),
    Binary {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    FuncCall {
        name: String,
        args: Vec<Expr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Concat,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

fn tokenize_formula(src: &str) -> Result<Vec<Token>, FormulaError> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        if ch.is_whitespace() {
            i += 1;
            continue;
        }
        match ch {
            '+' => {
                tokens.push(Token::Plus);
                i += 1;
            }
            '-' => {
                tokens.push(Token::Minus);
                i += 1;
            }
            '*' => {
                tokens.push(Token::Star);
                i += 1;
            }
            '/' => {
                tokens.push(Token::Slash);
                i += 1;
            }
            '^' => {
                tokens.push(Token::Caret);
                i += 1;
            }
            '%' => {
                tokens.push(Token::Percent);
                i += 1;
            }
            '&' => {
                tokens.push(Token::Ampersand);
                i += 1;
            }
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            ',' | ';' => {
                tokens.push(Token::Comma);
                i += 1;
            }
            ':' => {
                tokens.push(Token::Colon);
                i += 1;
            }
            '=' => {
                tokens.push(Token::Eq);
                i += 1;
            }
            '<' => {
                if i + 1 < chars.len() && chars[i + 1] == '>' {
                    tokens.push(Token::Ne);
                    i += 2;
                } else if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token::Le);
                    i += 2;
                } else {
                    tokens.push(Token::Lt);
                    i += 1;
                }
            }
            '>' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token::Ge);
                    i += 2;
                } else {
                    tokens.push(Token::Gt);
                    i += 1;
                }
            }
            '"' => {
                i += 1;
                let mut text = String::new();
                let mut closed = false;
                while i < chars.len() {
                    if chars[i] == '"' {
                        if i + 1 < chars.len() && chars[i + 1] == '"' {
                            text.push('"');
                            i += 2;
                        } else {
                            i += 1;
                            closed = true;
                            break;
                        }
                    } else {
                        text.push(chars[i]);
                        i += 1;
                    }
                }
                if !closed {
                    return Err(FormulaError::Value);
                }
                tokens.push(Token::Text(text));
            }
            c if c.is_ascii_digit() || c == '.' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let num_str: String = chars[start..i].iter().collect();
                let val = num_str.parse::<f64>().map_err(|_| FormulaError::Value)?;
                if !val.is_finite() {
                    return Err(FormulaError::Num);
                }
                tokens.push(Token::Number(val));
            }
            c if c.is_ascii_alphabetic() || c == '$' || c == '_' => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_ascii_alphanumeric() || chars[i] == '$' || chars[i] == '_')
                {
                    i += 1;
                }
                let ident: String = chars[start..i].iter().collect();
                tokens.push(Token::Ident(ident));
            }
            _ => return Err(FormulaError::Value),
        }
    }

    Ok(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        if self.pos < self.tokens.len() {
            let tok = self.tokens[self.pos].clone();
            self.pos += 1;
            Some(tok)
        } else {
            None
        }
    }

    fn parse_expression(&mut self) -> Result<Expr, FormulaError> {
        self.parse_comparison()
    }

    fn parse_comparison(&mut self) -> Result<Expr, FormulaError> {
        let mut lhs = self.parse_concat()?;
        while let Some(tok) = self.peek() {
            let op = match tok {
                Token::Eq => BinaryOp::Eq,
                Token::Ne => BinaryOp::Ne,
                Token::Lt => BinaryOp::Lt,
                Token::Le => BinaryOp::Le,
                Token::Gt => BinaryOp::Gt,
                Token::Ge => BinaryOp::Ge,
                _ => break,
            };
            self.next();
            let rhs = self.parse_concat()?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_concat(&mut self) -> Result<Expr, FormulaError> {
        let mut lhs = self.parse_additive()?;
        while matches!(self.peek(), Some(Token::Ampersand)) {
            self.next();
            let rhs = self.parse_additive()?;
            lhs = Expr::Binary {
                op: BinaryOp::Concat,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_additive(&mut self) -> Result<Expr, FormulaError> {
        let mut lhs = self.parse_multiplicative()?;
        while let Some(tok) = self.peek() {
            let op = match tok {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                _ => break,
            };
            self.next();
            let rhs = self.parse_multiplicative()?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, FormulaError> {
        let mut lhs = self.parse_power()?;
        while let Some(tok) = self.peek() {
            let op = match tok {
                Token::Star => BinaryOp::Mul,
                Token::Slash => BinaryOp::Div,
                _ => break,
            };
            self.next();
            let rhs = self.parse_power()?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_power(&mut self) -> Result<Expr, FormulaError> {
        let mut lhs = self.parse_unary()?;
        while matches!(self.peek(), Some(Token::Caret)) {
            self.next();
            let rhs = self.parse_unary()?;
            lhs = Expr::Binary {
                op: BinaryOp::Pow,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<Expr, FormulaError> {
        if matches!(self.peek(), Some(Token::Plus)) {
            self.next();
            return Ok(Expr::UnaryPlus(Box::new(self.parse_unary()?)));
        }
        if matches!(self.peek(), Some(Token::Minus)) {
            self.next();
            return Ok(Expr::UnaryMinus(Box::new(self.parse_unary()?)));
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr, FormulaError> {
        let mut expr = self.parse_primary()?;
        while matches!(self.peek(), Some(Token::Percent)) {
            self.next();
            expr = Expr::PostfixPercent(Box::new(expr));
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, FormulaError> {
        match self.next() {
            Some(Token::Number(n)) => Ok(Expr::Number(n)),
            Some(Token::Text(s)) => Ok(Expr::Text(s)),
            Some(Token::LParen) => {
                let inner = self.parse_expression()?;
                if !matches!(self.next(), Some(Token::RParen)) {
                    return Err(FormulaError::Value);
                }
                Ok(inner)
            }
            Some(Token::Ident(name)) => {
                if matches!(self.peek(), Some(Token::LParen)) {
                    self.next();
                    let mut args = Vec::new();
                    if !matches!(self.peek(), Some(Token::RParen)) {
                        loop {
                            args.push(self.parse_expression()?);
                            if matches!(self.peek(), Some(Token::Comma)) {
                                self.next();
                            } else {
                                break;
                            }
                        }
                    }
                    if !matches!(self.next(), Some(Token::RParen)) {
                        return Err(FormulaError::Value);
                    }
                    return Ok(Expr::FuncCall {
                        name: name.to_ascii_uppercase(),
                        args,
                    });
                }

                if name.eq_ignore_ascii_case("TRUE") {
                    return Ok(Expr::Boolean(true));
                }
                if name.eq_ignore_ascii_case("FALSE") {
                    return Ok(Expr::Boolean(false));
                }

                let start_coord =
                    PackedCellCoordinate::parse_a1(&name).map_err(|_| FormulaError::Name)?;
                if matches!(self.peek(), Some(Token::Colon)) {
                    self.next();
                    let Some(Token::Ident(end_name)) = self.next() else {
                        return Err(FormulaError::Ref);
                    };
                    let end_coord =
                        PackedCellCoordinate::parse_a1(&end_name).map_err(|_| FormulaError::Ref)?;
                    let range =
                        CellRange::new(start_coord, end_coord).map_err(|_| FormulaError::Ref)?;
                    return Ok(Expr::Range(range));
                }
                Ok(Expr::Cell(start_coord))
            }
            _ => Err(FormulaError::Value),
        }
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
            Self::GreaterThan(threshold) => value.as_number().map_or(false, |v| v > *threshold),
            Self::LessThan(threshold) => value.as_number().map_or(false, |v| v < *threshold),
            Self::Between(low, high) => value.as_number().map_or(false, |v| {
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
            Self::NumberRange { min, max } => value
                .as_number()
                .map_or(false, |v| v >= *min && v <= *max),
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

/// Spreadsheet calculation engine with $O(V + E)$ cycle detection and DAG memoization.
#[derive(Debug, Clone, Default)]
pub struct SheetEngine {
    pub name: String,
    cells: HashMap<PackedCellCoordinate, String>,
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

    pub fn set_cell(&mut self, a1: &str, raw_value: impl Into<String>) -> Result<(), EasyOfficeError> {
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
        let mut visiting = HashSet::new();
        let mut memo = HashMap::new();
        self.eval_coord(coord, &mut visiting, &mut memo, 0)
    }

    pub fn evaluate_formula_str(&self, formula: &str) -> EvaluatedCellValue {
        let mut visiting = HashSet::new();
        let mut memo = HashMap::new();
        self.eval_raw_string(formula, &mut visiting, &mut memo, 0)
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

        let mut visiting = HashSet::new();
        let mut memo = HashMap::new();
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
                let key_str = self
                    .eval_coord(kc, &mut visiting, &mut memo, 0)
                    .to_display_string();
                if key_str.is_empty() {
                    continue;
                }
                if !row_keys.contains(&key_str) {
                    row_keys.push(key_str.clone());
                }
                if let Some(num) = self
                    .eval_coord(vc, &mut visiting, &mut memo, 0)
                    .as_number()
                {
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

    fn eval_coord(
        &self,
        coord: PackedCellCoordinate,
        visiting: &mut HashSet<PackedCellCoordinate>,
        memo: &mut HashMap<PackedCellCoordinate, EvaluatedCellValue>,
        depth: usize,
    ) -> EvaluatedCellValue {
        if depth > Self::MAX_RECURSION_DEPTH {
            return EvaluatedCellValue::Error(FormulaError::Cycle);
        }
        if let Some(cached) = memo.get(&coord) {
            return cached.clone();
        }
        if !visiting.insert(coord) {
            return EvaluatedCellValue::Error(FormulaError::Cycle);
        }

        let result = match self.cells.get(&coord) {
            None => EvaluatedCellValue::Empty,
            Some(raw) => self.eval_raw_string(raw, visiting, memo, depth + 1),
        };

        visiting.remove(&coord);
        memo.insert(coord, result.clone());
        result
    }

    fn eval_raw_string(
        &self,
        raw: &str,
        visiting: &mut HashSet<PackedCellCoordinate>,
        memo: &mut HashMap<PackedCellCoordinate, EvaluatedCellValue>,
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
            self.eval_expr(&expr, visiting, memo, depth)
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

    fn eval_expr(
        &self,
        expr: &Expr,
        visiting: &mut HashSet<PackedCellCoordinate>,
        memo: &mut HashMap<PackedCellCoordinate, EvaluatedCellValue>,
        depth: usize,
    ) -> EvaluatedCellValue {
        if depth > Self::MAX_RECURSION_DEPTH {
            return EvaluatedCellValue::Error(FormulaError::Cycle);
        }
        match expr {
            Expr::Number(n) => EvaluatedCellValue::Number(*n),
            Expr::Text(s) => EvaluatedCellValue::Text(s.clone()),
            Expr::Boolean(b) => EvaluatedCellValue::Boolean(*b),
            Expr::Cell(coord) => self.eval_coord(*coord, visiting, memo, depth + 1),
            Expr::Range(_) => EvaluatedCellValue::Error(FormulaError::Value),
            Expr::UnaryPlus(inner) => {
                let val = self.eval_expr(inner, visiting, memo, depth + 1);
                if let EvaluatedCellValue::Error(e) = val {
                    return EvaluatedCellValue::Error(e);
                }
                match val.as_number() {
                    Some(n) => EvaluatedCellValue::Number(n),
                    None => EvaluatedCellValue::Error(FormulaError::Value),
                }
            }
            Expr::UnaryMinus(inner) => {
                let val = self.eval_expr(inner, visiting, memo, depth + 1);
                if let EvaluatedCellValue::Error(e) = val {
                    return EvaluatedCellValue::Error(e);
                }
                match val.as_number() {
                    Some(n) => EvaluatedCellValue::Number(-n),
                    None => EvaluatedCellValue::Error(FormulaError::Value),
                }
            }
            Expr::PostfixPercent(inner) => {
                let val = self.eval_expr(inner, visiting, memo, depth + 1);
                if let EvaluatedCellValue::Error(e) = val {
                    return EvaluatedCellValue::Error(e);
                }
                match val.as_number() {
                    Some(n) => EvaluatedCellValue::Number(n / 100.0),
                    None => EvaluatedCellValue::Error(FormulaError::Value),
                }
            }
            Expr::Binary { op, lhs, rhs } => {
                let l_val = self.eval_expr(lhs, visiting, memo, depth + 1);
                if let EvaluatedCellValue::Error(e) = l_val {
                    return EvaluatedCellValue::Error(e);
                }
                let r_val = self.eval_expr(rhs, visiting, memo, depth + 1);
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
                        let ord = if let (Some(ln), Some(rn)) = (l_val.as_number(), r_val.as_number())
                        {
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
            Expr::FuncCall { name, args } => self.eval_func(name, args, visiting, memo, depth + 1),
        }
    }

    fn expand_args(
        &self,
        args: &[Expr],
        visiting: &mut HashSet<PackedCellCoordinate>,
        memo: &mut HashMap<PackedCellCoordinate, EvaluatedCellValue>,
        depth: usize,
    ) -> Result<Vec<EvaluatedCellValue>, FormulaError> {
        let mut out = Vec::new();
        for arg in args {
            match arg {
                Expr::Range(range) => {
                    let coords = range.expand().map_err(|_| FormulaError::Ref)?;
                    for c in coords {
                        let v = self.eval_coord(c, visiting, memo, depth + 1);
                        if let EvaluatedCellValue::Error(e) = v {
                            return Err(e);
                        }
                        out.push(v);
                    }
                }
                _ => {
                    let v = self.eval_expr(arg, visiting, memo, depth + 1);
                    if let EvaluatedCellValue::Error(e) = v {
                        return Err(e);
                    }
                    out.push(v);
                }
            }
        }
        Ok(out)
    }

    fn eval_func(
        &self,
        name: &str,
        args: &[Expr],
        visiting: &mut HashSet<PackedCellCoordinate>,
        memo: &mut HashMap<PackedCellCoordinate, EvaluatedCellValue>,
        depth: usize,
    ) -> EvaluatedCellValue {
        match name {
            "SUM" => match self.expand_args(args, visiting, memo, depth) {
                Ok(vals) => {
                    let sum: f64 = vals.iter().filter_map(EvaluatedCellValue::as_number).sum();
                    EvaluatedCellValue::Number(sum)
                }
                Err(e) => EvaluatedCellValue::Error(e),
            },
            "AVERAGE" => match self.expand_args(args, visiting, memo, depth) {
                Ok(vals) => {
                    let nums: Vec<f64> =
                        vals.iter().filter_map(EvaluatedCellValue::as_number).collect();
                    if nums.is_empty() {
                        EvaluatedCellValue::Error(FormulaError::DivByZero)
                    } else {
                        EvaluatedCellValue::Number(nums.iter().sum::<f64>() / (nums.len() as f64))
                    }
                }
                Err(e) => EvaluatedCellValue::Error(e),
            },
            "MIN" => match self.expand_args(args, visiting, memo, depth) {
                Ok(vals) => {
                    let min_val = vals
                        .iter()
                        .filter_map(EvaluatedCellValue::as_number)
                        .reduce(f64::min)
                        .unwrap_or(0.0);
                    EvaluatedCellValue::Number(min_val)
                }
                Err(e) => EvaluatedCellValue::Error(e),
            },
            "MAX" => match self.expand_args(args, visiting, memo, depth) {
                Ok(vals) => {
                    let max_val = vals
                        .iter()
                        .filter_map(EvaluatedCellValue::as_number)
                        .reduce(f64::max)
                        .unwrap_or(0.0);
                    EvaluatedCellValue::Number(max_val)
                }
                Err(e) => EvaluatedCellValue::Error(e),
            },
            "COUNT" => match self.expand_args(args, visiting, memo, depth) {
                Ok(vals) => {
                    let count = vals
                        .iter()
                        .filter(|v| matches!(v, EvaluatedCellValue::Number(_)))
                        .count();
                    EvaluatedCellValue::Number(count as f64)
                }
                Err(e) => EvaluatedCellValue::Error(e),
            },
            "COUNTA" => match self.expand_args(args, visiting, memo, depth) {
                Ok(vals) => {
                    let count = vals
                        .iter()
                        .filter(|v| !matches!(v, EvaluatedCellValue::Empty))
                        .count();
                    EvaluatedCellValue::Number(count as f64)
                }
                Err(e) => EvaluatedCellValue::Error(e),
            },
            "IF" => {
                if args.len() < 2 || args.len() > 3 {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                }
                let cond = self.eval_expr(&args[0], visiting, memo, depth + 1);
                if let EvaluatedCellValue::Error(e) = cond {
                    return EvaluatedCellValue::Error(e);
                }
                let Some(flag) = cond.as_bool() else {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                };
                if flag {
                    self.eval_expr(&args[1], visiting, memo, depth + 1)
                } else if args.len() == 3 {
                    self.eval_expr(&args[2], visiting, memo, depth + 1)
                } else {
                    EvaluatedCellValue::Boolean(false)
                }
            }
            "IFERROR" => {
                if args.len() != 2 {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                }
                let primary = self.eval_expr(&args[0], visiting, memo, depth + 1);
                if matches!(primary, EvaluatedCellValue::Error(_)) {
                    self.eval_expr(&args[1], visiting, memo, depth + 1)
                } else {
                    primary
                }
            }
            "AND" => match self.expand_args(args, visiting, memo, depth) {
                Ok(vals) if !vals.is_empty() => {
                    let mut result = true;
                    for v in vals {
                        if matches!(v, EvaluatedCellValue::Empty) {
                            continue;
                        }
                        let Some(b) = v.as_bool() else {
                            return EvaluatedCellValue::Error(FormulaError::Value);
                        };
                        if !b {
                            result = false;
                        }
                    }
                    EvaluatedCellValue::Boolean(result)
                }
                Ok(_) => EvaluatedCellValue::Error(FormulaError::Value),
                Err(e) => EvaluatedCellValue::Error(e),
            },
            "OR" => match self.expand_args(args, visiting, memo, depth) {
                Ok(vals) if !vals.is_empty() => {
                    let mut result = false;
                    for v in vals {
                        if matches!(v, EvaluatedCellValue::Empty) {
                            continue;
                        }
                        let Some(b) = v.as_bool() else {
                            return EvaluatedCellValue::Error(FormulaError::Value);
                        };
                        if b {
                            result = true;
                        }
                    }
                    EvaluatedCellValue::Boolean(result)
                }
                Ok(_) => EvaluatedCellValue::Error(FormulaError::Value),
                Err(e) => EvaluatedCellValue::Error(e),
            },
            "NOT" => {
                if args.len() != 1 {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                }
                let val = self.eval_expr(&args[0], visiting, memo, depth + 1);
                if let EvaluatedCellValue::Error(e) = val {
                    return EvaluatedCellValue::Error(e);
                }
                match val.as_bool() {
                    Some(b) => EvaluatedCellValue::Boolean(!b),
                    None => EvaluatedCellValue::Error(FormulaError::Value),
                }
            }
            "ABS" => {
                if args.len() != 1 {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                }
                let val = self.eval_expr(&args[0], visiting, memo, depth + 1);
                if let EvaluatedCellValue::Error(e) = val {
                    return EvaluatedCellValue::Error(e);
                }
                match val.as_number() {
                    Some(n) => EvaluatedCellValue::Number(n.abs()),
                    None => EvaluatedCellValue::Error(FormulaError::Value),
                }
            }
            "ROUND" => {
                if args.len() != 2 {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                }
                let v0 = self.eval_expr(&args[0], visiting, memo, depth + 1);
                if let EvaluatedCellValue::Error(e) = v0 {
                    return EvaluatedCellValue::Error(e);
                }
                let v1 = self.eval_expr(&args[1], visiting, memo, depth + 1);
                if let EvaluatedCellValue::Error(e) = v1 {
                    return EvaluatedCellValue::Error(e);
                }
                let (Some(num), Some(digits)) = (v0.as_number(), v1.as_number()) else {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                };
                let exp = digits.trunc().clamp(-300.0, 300.0) as i32;
                let factor = 10f64.powi(exp);
                if !factor.is_finite() || factor == 0.0 {
                    return EvaluatedCellValue::Number(num);
                }
                EvaluatedCellValue::Number((num * factor).round() / factor)
            }
            "SQRT" => {
                if args.len() != 1 {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                }
                let val = self.eval_expr(&args[0], visiting, memo, depth + 1);
                if let EvaluatedCellValue::Error(e) = val {
                    return EvaluatedCellValue::Error(e);
                }
                let Some(n) = val.as_number() else {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                };
                if n < 0.0 {
                    return EvaluatedCellValue::Error(FormulaError::Num);
                }
                EvaluatedCellValue::Number(n.sqrt())
            }
            "POWER" => {
                if args.len() != 2 {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                }
                let v0 = self.eval_expr(&args[0], visiting, memo, depth + 1);
                if let EvaluatedCellValue::Error(e) = v0 {
                    return EvaluatedCellValue::Error(e);
                }
                let v1 = self.eval_expr(&args[1], visiting, memo, depth + 1);
                if let EvaluatedCellValue::Error(e) = v1 {
                    return EvaluatedCellValue::Error(e);
                }
                let (Some(base), Some(exp)) = (v0.as_number(), v1.as_number()) else {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                };
                let res = base.powf(exp);
                if !res.is_finite() {
                    return EvaluatedCellValue::Error(FormulaError::Num);
                }
                EvaluatedCellValue::Number(res)
            }
            "MOD" => {
                if args.len() != 2 {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                }
                let v0 = self.eval_expr(&args[0], visiting, memo, depth + 1);
                if let EvaluatedCellValue::Error(e) = v0 {
                    return EvaluatedCellValue::Error(e);
                }
                let v1 = self.eval_expr(&args[1], visiting, memo, depth + 1);
                if let EvaluatedCellValue::Error(e) = v1 {
                    return EvaluatedCellValue::Error(e);
                }
                let (Some(num), Some(div)) = (v0.as_number(), v1.as_number()) else {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                };
                if div.abs() < 1e-15 {
                    return EvaluatedCellValue::Error(FormulaError::DivByZero);
                }
                EvaluatedCellValue::Number(num % div)
            }
            "CONCAT" => {
                let vals = match self.expand_args(args, visiting, memo, depth) {
                    Ok(v) => v,
                    Err(e) => return EvaluatedCellValue::Error(e),
                };
                let mut joined = String::new();
                for v in vals {
                    if let EvaluatedCellValue::Error(e) = v {
                        return EvaluatedCellValue::Error(e);
                    }
                    joined.push_str(&v.to_display_string());
                }
                EvaluatedCellValue::Text(joined)
            }
            "MEDIAN" => {
                let vals = match self.expand_args(args, visiting, memo, depth) {
                    Ok(v) => v,
                    Err(e) => return EvaluatedCellValue::Error(e),
                };
                let mut nums: Vec<f64> = vals
                    .iter()
                    .filter_map(EvaluatedCellValue::as_number)
                    .collect();
                if nums.is_empty() {
                    return EvaluatedCellValue::Error(FormulaError::Num);
                }
                nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let mid = nums.len() / 2;
                let median = if nums.len() % 2 == 1 {
                    nums[mid]
                } else {
                    (nums[mid - 1] + nums[mid]) / 2.0
                };
                EvaluatedCellValue::Number(median)
            }
            "STDEV" => {
                let vals = match self.expand_args(args, visiting, memo, depth) {
                    Ok(v) => v,
                    Err(e) => return EvaluatedCellValue::Error(e),
                };
                let nums: Vec<f64> = vals
                    .iter()
                    .filter_map(EvaluatedCellValue::as_number)
                    .collect();
                if nums.len() < 2 {
                    return EvaluatedCellValue::Error(FormulaError::DivByZero);
                }
                let mean = nums.iter().sum::<f64>() / (nums.len() as f64);
                let var = nums.iter().map(|x| (x - mean).powi(2)).sum::<f64>()
                    / ((nums.len() - 1) as f64);
                EvaluatedCellValue::Number(var.sqrt())
            }
            "LEN" => {
                if args.len() != 1 {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                }
                let val = self.eval_expr(&args[0], visiting, memo, depth + 1);
                if let EvaluatedCellValue::Error(e) = val {
                    return EvaluatedCellValue::Error(e);
                }
                EvaluatedCellValue::Number(val.to_display_string().chars().count() as f64)
            }
            "UPPER" => {
                if args.len() != 1 {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                }
                let val = self.eval_expr(&args[0], visiting, memo, depth + 1);
                if let EvaluatedCellValue::Error(e) = val {
                    return EvaluatedCellValue::Error(e);
                }
                EvaluatedCellValue::Text(val.to_display_string().to_uppercase())
            }
            "LOWER" => {
                if args.len() != 1 {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                }
                let val = self.eval_expr(&args[0], visiting, memo, depth + 1);
                if let EvaluatedCellValue::Error(e) = val {
                    return EvaluatedCellValue::Error(e);
                }
                EvaluatedCellValue::Text(val.to_display_string().to_lowercase())
            }
            "TRIM" => {
                if args.len() != 1 {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                }
                let val = self.eval_expr(&args[0], visiting, memo, depth + 1);
                if let EvaluatedCellValue::Error(e) = val {
                    return EvaluatedCellValue::Error(e);
                }
                let trimmed = val
                    .to_display_string()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                EvaluatedCellValue::Text(trimmed)
            }
            "ISBLANK" => {
                if args.len() != 1 {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                }
                // ECMA-376 standard: only genuinely empty cells evaluate to TRUE; `=""` evaluates to Text("") which is FALSE.
                let val = self.eval_expr(&args[0], visiting, memo, depth + 1);
                EvaluatedCellValue::Boolean(matches!(val, EvaluatedCellValue::Empty))
            }
            "VLOOKUP" => {
                if args.len() < 3 || args.len() > 4 {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                }
                let lookup_val = self.eval_expr(&args[0], visiting, memo, depth + 1);
                if let EvaluatedCellValue::Error(e) = lookup_val {
                    return EvaluatedCellValue::Error(e);
                }
                let Expr::Range(range) = &args[1] else {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                };
                let col_idx_val = self.eval_expr(&args[2], visiting, memo, depth + 1);
                if let EvaluatedCellValue::Error(e) = col_idx_val {
                    return EvaluatedCellValue::Error(e);
                }
                let Some(col_idx_f) = col_idx_val.as_number() else {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                };
                let col_idx = col_idx_f.trunc() as isize;
                let (rows, cols) = range.dimensions();
                if col_idx < 1 || (col_idx as usize) > cols {
                    return EvaluatedCellValue::Error(FormulaError::Ref);
                }
                let min_r = range.start.row().min(range.end.row());
                let min_c = range.start.col().min(range.end.col());
                let target_c = min_c + (col_idx as u32 - 1);

                for r_off in 0..rows {
                    let key_coord = match PackedCellCoordinate::new(min_r + r_off as u32, min_c) {
                        Ok(c) => c,
                        Err(_) => return EvaluatedCellValue::Error(FormulaError::Ref),
                    };
                    let cell_val = self.eval_coord(key_coord, visiting, memo, depth + 1);
                    if Self::values_match(&cell_val, &lookup_val) {
                        let res_coord =
                            match PackedCellCoordinate::new(min_r + r_off as u32, target_c) {
                                Ok(c) => c,
                                Err(_) => return EvaluatedCellValue::Error(FormulaError::Ref),
                            };
                        return self.eval_coord(res_coord, visiting, memo, depth + 1);
                    }
                }
                EvaluatedCellValue::Error(FormulaError::Na)
            }
            "INDEX" => {
                if args.len() < 2 || args.len() > 3 {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                }
                let Expr::Range(range) = &args[0] else {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                };
                let row_val = self.eval_expr(&args[1], visiting, memo, depth + 1);
                if let EvaluatedCellValue::Error(e) = row_val {
                    return EvaluatedCellValue::Error(e);
                }
                let Some(row_f) = row_val.as_number() else {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                };
                let row_num = row_f.trunc() as isize;
                let col_num = if args.len() == 3 {
                    let col_val = self.eval_expr(&args[2], visiting, memo, depth + 1);
                    if let EvaluatedCellValue::Error(e) = col_val {
                        return EvaluatedCellValue::Error(e);
                    }
                    let Some(col_f) = col_val.as_number() else {
                        return EvaluatedCellValue::Error(FormulaError::Value);
                    };
                    col_f.trunc() as isize
                } else {
                    1
                };
                let (rows, cols) = range.dimensions();
                if row_num < 1
                    || col_num < 1
                    || (row_num as usize) > rows
                    || (col_num as usize) > cols
                {
                    return EvaluatedCellValue::Error(FormulaError::Ref);
                }
                let min_r = range.start.row().min(range.end.row());
                let min_c = range.start.col().min(range.end.col());
                match PackedCellCoordinate::new(
                    min_r + (row_num as u32 - 1),
                    min_c + (col_num as u32 - 1),
                ) {
                    Ok(coord) => self.eval_coord(coord, visiting, memo, depth + 1),
                    Err(_) => EvaluatedCellValue::Error(FormulaError::Ref),
                }
            }
            "MATCH" => {
                if args.len() < 2 || args.len() > 3 {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                }
                let lookup_val = self.eval_expr(&args[0], visiting, memo, depth + 1);
                if let EvaluatedCellValue::Error(e) = lookup_val {
                    return EvaluatedCellValue::Error(e);
                }
                let Expr::Range(range) = &args[1] else {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                };
                let Ok(coords) = range.expand() else {
                    return EvaluatedCellValue::Error(FormulaError::Ref);
                };
                for (idx, coord) in coords.into_iter().enumerate() {
                    let v = self.eval_coord(coord, visiting, memo, depth + 1);
                    if Self::values_match(&v, &lookup_val) {
                        return EvaluatedCellValue::Number((idx + 1) as f64);
                    }
                }
                EvaluatedCellValue::Error(FormulaError::Na)
            }
            "NPV" => {
                if args.len() < 2 {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                }
                let rate_val = self.eval_expr(&args[0], visiting, memo, depth + 1);
                if let EvaluatedCellValue::Error(e) = rate_val {
                    return EvaluatedCellValue::Error(e);
                }
                let Some(rate) = rate_val.as_number() else {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                };
                if rate <= -1.0 {
                    return EvaluatedCellValue::Error(FormulaError::DivByZero);
                }
                let cashflows = match self.expand_args(&args[1..], visiting, memo, depth) {
                    Ok(vals) => vals,
                    Err(e) => return EvaluatedCellValue::Error(e),
                };
                let mut npv = 0.0;
                for (t, v) in cashflows
                    .iter()
                    .filter_map(EvaluatedCellValue::as_number)
                    .enumerate()
                {
                    npv += v / (1.0 + rate).powi((t + 1) as i32);
                }
                EvaluatedCellValue::Number(npv)
            }
            _ => EvaluatedCellValue::Error(FormulaError::Name),
        }
    }

    fn values_match(a: &EvaluatedCellValue, b: &EvaluatedCellValue) -> bool {
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
        sheet.set_cell("D3", "=IF(D1>200, UPPER(\"pass\"), \"fail\")").unwrap();

        assert_eq!(sheet.evaluate_cell("A3"), EvaluatedCellValue::Number(130.0));
        assert_eq!(
            sheet.evaluate_cell("B2"),
            EvaluatedCellValue::Boolean(true)
        );
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
        sheet.set_cell("C3", "=MATCH(\"코어엔진\", A2:A4, 0)").unwrap();
        sheet.set_cell("C4", "=ROUND(NPV(0.1, B2:B3), 2)").unwrap();
        sheet.set_cell("C5", "=SQRT(16)+POWER(2,3)+MOD(10,3)").unwrap();
        sheet.set_cell("C6", "=CONCAT(A2, \"-\", A3)").unwrap();
        sheet.set_cell("C7", "=MEDIAN(B2:B4)").unwrap();
        sheet.set_cell("C8", "=ROUND(STDEV(B2:B4), 2)").unwrap();
        sheet.set_cell("C9", "=VLOOKUP(\"코어엔진\", A2:B4, 1/0, FALSE)").unwrap();

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

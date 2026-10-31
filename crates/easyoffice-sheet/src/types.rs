use easyoffice_formats::EasyOfficeError;
use std::fmt;

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
    pub const fn row(self) -> u32 {
        (self.0 >> 32) as u32
    }

    #[inline]
    pub const fn col(self) -> u32 {
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
                let mut hits = 0usize;
                for item in range_values {
                    if item == value {
                        hits += 1;
                        if hits > 1 {
                            return true;
                        }
                    }
                }
                false
            }
        }
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PivotAggregation {
    Sum,
    Average,
    Count,
    Min,
    Max,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PivotTableSummary {
    pub row_keys: Vec<String>,
    pub column_keys: Vec<String>,
    pub matrix: Vec<Vec<f64>>,
    pub row_totals: Vec<f64>,
    pub column_totals: Vec<f64>,
    pub grand_total: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackedWorkbookCoord(pub u64);

impl PackedWorkbookCoord {
    #[inline]
    pub const fn new(sheet_id: u16, coord: PackedCellCoordinate) -> Self {
        let row_bits = ((coord.row() as u64) & 0xFFFF_FFFF) << 16;
        let col_bits = (coord.col() as u64) & 0xFFFF;
        Self(((sheet_id as u64) << 48) | row_bits | col_bits)
    }

    #[inline]
    pub const fn sheet_id(self) -> u16 {
        (self.0 >> 48) as u16
    }

    #[inline]
    pub const fn row(self) -> u32 {
        ((self.0 >> 16) & 0xFFFF_FFFF) as u32
    }

    #[inline]
    pub const fn col(self) -> u32 {
        (self.0 & 0xFFFF) as u32
    }
}

#[inline]
pub fn compute_sheet_id(name: &str) -> u16 {
    let mut hash: u32 = 0x811C_9DC5;
    for b in name.bytes() {
        hash ^= u32::from(b.to_ascii_lowercase());
        hash = hash.wrapping_mul(0x0100_0193);
    }
    let folded = ((hash >> 16) ^ (hash & 0xFFFF)) as u16;
    if folded == 0 {
        1
    } else {
        folded
    }
}

pub struct VisitingTracker {
    primary_sheet_id: u16,
    grid_bitset: [u64; 16],
    stack: [u64; 66],
    len: usize,
    spill: Vec<u64>,
}

impl VisitingTracker {
    #[inline]
    pub const fn new(primary_sheet_id: u16) -> Self {
        Self {
            primary_sheet_id,
            grid_bitset: [0u64; 16],
            stack: [0u64; 66],
            len: 0,
            spill: Vec::new(),
        }
    }

    #[inline]
    pub fn insert(&mut self, sheet_id: u16, coord: PackedCellCoordinate) -> bool {
        let r = coord.row();
        let c = coord.col();
        if sheet_id == self.primary_sheet_id && r < 64 && c < 16 {
            let bit_idx = (r as usize) * 16 + (c as usize);
            let word = &mut self.grid_bitset[bit_idx >> 6];
            let mask = 1u64 << (bit_idx & 63);
            if (*word & mask) != 0 {
                return false;
            }
            *word |= mask;
            return true;
        }
        let packed = PackedWorkbookCoord::new(sheet_id, coord).0;
        for i in 0..self.len {
            if self.stack[i] == packed {
                return false;
            }
        }
        if self.spill.contains(&packed) {
            return false;
        }
        if self.len < self.stack.len() {
            self.stack[self.len] = packed;
            self.len += 1;
        } else {
            self.spill.push(packed);
        }
        true
    }

    #[inline]
    pub fn remove(&mut self, sheet_id: u16, coord: PackedCellCoordinate) {
        let r = coord.row();
        let c = coord.col();
        if sheet_id == self.primary_sheet_id && r < 64 && c < 16 {
            let bit_idx = (r as usize) * 16 + (c as usize);
            self.grid_bitset[bit_idx >> 6] &= !(1u64 << (bit_idx & 63));
            return;
        }
        let packed = PackedWorkbookCoord::new(sheet_id, coord).0;
        if let Some(pos) = self.spill.iter().rposition(|&x| x == packed) {
            self.spill.swap_remove(pos);
        } else if self.len > 0 && self.stack[self.len - 1] == packed {
            self.len -= 1;
        } else if let Some(pos) = self.stack[..self.len].iter().rposition(|&x| x == packed) {
            self.stack[pos] = self.stack[self.len - 1];
            self.len -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn test_korean_formulas_openformula_prefixes_and_quoted_sheet_brackets() {
        let mut sheet = SheetEngine::new("매출");
        sheet.set_cell("A1", "=\"한글\"").unwrap();
        sheet.set_cell("A2", "='매출'!A1").unwrap();
        sheet.set_cell("A3", "=매출!A1").unwrap();
        assert_eq!(
            sheet.evaluate_cell("A1"),
            EvaluatedCellValue::Text("한글".to_string())
        );
        assert_eq!(
            sheet.evaluate_cell("A2"),
            EvaluatedCellValue::Text("한글".to_string())
        );
        assert_eq!(
            sheet.evaluate_cell("A3"),
            EvaluatedCellValue::Text("한글".to_string())
        );

        sheet.set_cell("A1", "10").unwrap();
        sheet.set_cell("A2", "20").unwrap();
        sheet.set_cell("B1", "of:=SUM([.A1:.A2])").unwrap();
        sheet.set_cell("B2", "oooc:=SUM([.A1:.A2])").unwrap();
        sheet.set_cell("B3", "msoxl:=SUM(A1:A2)").unwrap();
        assert_eq!(sheet.evaluate_cell("B1"), EvaluatedCellValue::Number(30.0));
        assert_eq!(sheet.evaluate_cell("B2"), EvaluatedCellValue::Number(30.0));
        assert_eq!(sheet.evaluate_cell("B3"), EvaluatedCellValue::Number(30.0));

        let mut wb = WorkbookEngine::new();
        wb.add_sheet("2026 [Q1]").set_cell("A1", "42").unwrap();
        wb.add_sheet("Q1:Q2").set_cell("A1", "55").unwrap();
        {
            let ab = wb.add_sheet("A.B");
            ab.set_cell("C1", "10").unwrap();
            ab.set_cell("D2", "25").unwrap();
        }
        {
            let main_sheet = wb.add_sheet("요약");
            main_sheet.set_cell("A1", "=['2026 [Q1]'!A1]").unwrap();
            main_sheet.set_cell("A2", "='2026 [Q1]'!A1").unwrap();
            main_sheet.set_cell("A3", "of:=['Q1:Q2'.A1]").unwrap();
            main_sheet
                .set_cell("A4", "of:=SUM(['A.B'.C1:.D2])")
                .unwrap();
            main_sheet
                .set_cell("A5", "of:=SUM(['Q1:Q2'.A1:'A.B'.D2])")
                .unwrap();
            main_sheet
                .set_cell("A6", "of:=SUM([.A1:'A.B'.D2])")
                .unwrap();
        }
        assert_eq!(
            wb.evaluate_cell("요약", "A1"),
            EvaluatedCellValue::Number(42.0)
        );
        assert_eq!(
            wb.evaluate_cell("요약", "A2"),
            EvaluatedCellValue::Number(42.0)
        );
        assert_eq!(
            wb.evaluate_cell("요약", "A3"),
            EvaluatedCellValue::Number(55.0)
        );
        assert_eq!(
            wb.evaluate_cell("요약", "A4"),
            EvaluatedCellValue::Number(35.0)
        );
        assert_eq!(
            wb.evaluate_cell("요약", "A5"),
            EvaluatedCellValue::Error(FormulaError::Ref)
        );
        assert_eq!(
            wb.evaluate_cell("요약", "A6"),
            EvaluatedCellValue::Error(FormulaError::Ref)
        );

        sheet.set_cell("C1", "10").unwrap();
        sheet.set_cell("D1", "100").unwrap();
        sheet.set_cell("C2", "10").unwrap();
        sheet.set_cell("D2", "200").unwrap();
        sheet.set_cell("C3", "20").unwrap();
        sheet.set_cell("D3", "300").unwrap();
        sheet
            .set_cell("E1", "=VLOOKUP(10, C1:D3, 2, TRUE)")
            .unwrap();
        assert_eq!(sheet.evaluate_cell("E1"), EvaluatedCellValue::Number(200.0));
    }

    #[test]
    fn test_dynamic_ranges_row_col_npv_and_criteria_parity() {
        let mut wb = WorkbookEngine::new();
        {
            let s1 = wb.add_sheet("Sheet1");
            s1.set_cell("A1", "10").unwrap();
            s1.set_cell("A2", "20").unwrap();
            s1.set_cell("A3", "30").unwrap();
            s1.set_cell("B1", "=SUM(INDIRECT(\"A1:A3\"))").unwrap();
            s1.set_cell("B2", "=SUM(OFFSET(A1, 0, 0, 3, 1))").unwrap();
            s1.set_cell("B3", "=SUBTOTAL(9, A1:A3)").unwrap();
            s1.set_cell("B4", "=SUBTOTAL(109, INDIRECT(\"Sheet2!A1:B2\"))")
                .unwrap();
            s1.set_cell("C5", "=ROW()*10+COLUMN()").unwrap();
            s1.set_cell("D1", "110").unwrap();
            s1.set_cell("D2", "TRUE").unwrap();
            s1.set_cell("D3", "text-not-num").unwrap();
            s1.set_cell("D4", "121").unwrap();
            s1.set_cell("E1", "=ROUND(NPV(0.1, D1:D4), 2)").unwrap();
            s1.set_cell("F1", "2025-12-31").unwrap();
            s1.set_cell("F2", "2026-03-15").unwrap();
            s1.set_cell("F3", "=DATE(2026, 6, 1)").unwrap();
            s1.set_cell("G1", "Alpha").unwrap();
            s1.set_cell("G2", "Bravo").unwrap();
            s1.set_cell("G3", "Charlie").unwrap();
            s1.set_cell("H1", "Alpha").unwrap();
            s1.set_cell("H2", "가나다").unwrap();
            s1.set_cell("H3", "2026-05-01").unwrap();
            s1.set_cell("J1", "10").unwrap();
            s1.set_cell("J2", "20").unwrap();
            s1.set_cell("J3", "=SUBTOTAL(9, J1:J2)").unwrap();
            s1.set_cell("J4", "30").unwrap();
            s1.set_cell("J5", "40").unwrap();
            s1.set_cell("J6", "=SUBTOTAL(9, J1:J5)").unwrap();
            s1.set_cell("J7", "=ROUND(SUBTOTAL(9, J1:J2), 0)").unwrap();
            s1.set_cell("J8", "=+SUBTOTAL(9, J1:J2)").unwrap();
            s1.set_cell("J9", "=SUBTOTAL(9, J1:J8)").unwrap();
        }
        {
            let s2 = wb.add_sheet("Sheet2");
            s2.set_cell("A1", "5").unwrap();
            s2.set_cell("B1", "15").unwrap();
            s2.set_cell("A2", "25").unwrap();
            s2.set_cell("B2", "35").unwrap();
        }
        assert_eq!(
            wb.evaluate_cell("Sheet1", "B1"),
            EvaluatedCellValue::Number(60.0)
        );
        assert_eq!(
            wb.evaluate_cell("Sheet1", "B2"),
            EvaluatedCellValue::Number(60.0)
        );
        assert_eq!(
            wb.evaluate_cell("Sheet1", "B3"),
            EvaluatedCellValue::Number(60.0)
        );
        assert_eq!(
            wb.evaluate_cell("Sheet1", "B4"),
            EvaluatedCellValue::Number(80.0)
        );
        assert_eq!(
            wb.evaluate_cell("Sheet1", "C5"),
            EvaluatedCellValue::Number(53.0)
        );
        assert_eq!(
            wb.evaluate_cell("Sheet1", "E1"),
            EvaluatedCellValue::Number(200.0)
        );
        assert_eq!(
            wb.evaluate_formula_on_sheet("Sheet1", "=COUNTIF(F1:F3, \">2026-01-01\")"),
            EvaluatedCellValue::Number(2.0)
        );
        assert_eq!(
            wb.evaluate_formula_on_sheet("Sheet1", "=COUNTIF(H1:H3, \">2026-01-01\")"),
            EvaluatedCellValue::Number(1.0)
        );
        assert_eq!(
            wb.evaluate_formula_on_sheet("Sheet1", "=COUNTIF(H1:H3, \"2026-05-01\")"),
            EvaluatedCellValue::Number(1.0)
        );
        assert_eq!(
            wb.evaluate_formula_on_sheet("Sheet1", "=COUNTIF(G1:G3, \"2026-05-01\")"),
            EvaluatedCellValue::Number(0.0)
        );
        assert_eq!(
            wb.evaluate_cell("Sheet1", "J3"),
            EvaluatedCellValue::Number(30.0)
        );
        assert_eq!(
            wb.evaluate_cell("Sheet1", "J6"),
            EvaluatedCellValue::Number(100.0)
        );
        assert_eq!(
            wb.evaluate_cell("Sheet1", "J9"),
            EvaluatedCellValue::Number(100.0)
        );
        assert_eq!(
            wb.evaluate_formula_on_sheet("Sheet1", "=SUBTOTAL(9, J1:J2, SUBTOTAL(9, J4:J5))"),
            EvaluatedCellValue::Number(30.0)
        );
        assert_eq!(
            wb.evaluate_formula_on_sheet("Sheet1", "=COUNTIF(G1:G3, \">=Bravo\")"),
            EvaluatedCellValue::Number(2.0)
        );
        assert_eq!(
            wb.evaluate_formula_on_sheet("Sheet1", "=SUMIF(A1:A3, \">10\", A1:A2)"),
            EvaluatedCellValue::Error(FormulaError::Value)
        );
        assert_eq!(
            wb.evaluate_formula_on_sheet("Sheet1", "=MINIFS(A1:A3, G1:G3, \">=Bravo\")"),
            EvaluatedCellValue::Number(20.0)
        );
        assert_eq!(
            wb.evaluate_formula_on_sheet("Sheet1", "=MAXIFS(A1:A3, G1:G3, \">=Bravo\")"),
            EvaluatedCellValue::Number(30.0)
        );
    }
}

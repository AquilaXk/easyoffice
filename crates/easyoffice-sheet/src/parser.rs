use crate::{CellRange, FormulaError, PackedCellCoordinate};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Token {
    Number(f64),
    Text(String),
    Ident(String),
    ErrorLiteral(FormulaError),
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
    Bang,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    Text(String),
    Boolean(bool),
    ErrorLiteral(FormulaError),
    Cell(PackedCellCoordinate),
    Range(CellRange),
    SheetCell {
        sheet: String,
        coord: PackedCellCoordinate,
    },
    SheetRange {
        sheet: String,
        range: CellRange,
    },
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
pub enum BinaryOp {
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

pub(crate) fn tokenize_formula(src: &str) -> Result<Vec<Token>, FormulaError> {
    let bytes = src.as_bytes();
    let mut tokens = Vec::with_capacity((bytes.len() / 2).max(4));
    let mut i = 0usize;

    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        match b {
            b'+' => {
                tokens.push(Token::Plus);
                i += 1;
            }
            b'-' => {
                tokens.push(Token::Minus);
                i += 1;
            }
            b'*' => {
                tokens.push(Token::Star);
                i += 1;
            }
            b'/' => {
                tokens.push(Token::Slash);
                i += 1;
            }
            b'^' => {
                tokens.push(Token::Caret);
                i += 1;
            }
            b'%' => {
                tokens.push(Token::Percent);
                i += 1;
            }
            b'&' => {
                tokens.push(Token::Ampersand);
                i += 1;
            }
            b'(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            b')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            b',' | b';' => {
                tokens.push(Token::Comma);
                i += 1;
            }
            b':' => {
                tokens.push(Token::Colon);
                i += 1;
            }
            b'!' => {
                tokens.push(Token::Bang);
                i += 1;
            }
            b'=' => {
                tokens.push(Token::Eq);
                i += 1;
            }
            b'<' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'>' {
                    tokens.push(Token::Ne);
                    i += 2;
                } else if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(Token::Le);
                    i += 2;
                } else {
                    tokens.push(Token::Lt);
                    i += 1;
                }
            }
            b'>' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(Token::Ge);
                    i += 2;
                } else {
                    tokens.push(Token::Gt);
                    i += 1;
                }
            }
            b'"' => {
                i += 1;
                let mut text = String::new();
                let mut closed = false;
                while i < bytes.len() {
                    if bytes[i] == b'"' {
                        if i + 1 < bytes.len() && bytes[i + 1] == b'"' {
                            text.push('"');
                            i += 2;
                        } else {
                            i += 1;
                            closed = true;
                            break;
                        }
                    } else {
                        let ch = src[i..].chars().next().ok_or(FormulaError::Value)?;
                        text.push(ch);
                        i += ch.len_utf8();
                    }
                }
                if !closed {
                    return Err(FormulaError::Value);
                }
                tokens.push(Token::Text(text));
            }
            b'\'' => {
                i += 1;
                let mut ident = String::new();
                let mut closed = false;
                while i < bytes.len() {
                    if bytes[i] == b'\'' {
                        if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                            ident.push('\'');
                            i += 2;
                        } else {
                            i += 1;
                            closed = true;
                            break;
                        }
                    } else {
                        let ch = src[i..].chars().next().ok_or(FormulaError::Value)?;
                        ident.push(ch);
                        i += ch.len_utf8();
                    }
                }
                if !closed {
                    return Err(FormulaError::Value);
                }
                tokens.push(Token::Ident(ident));
            }
            b'#' => {
                let start = i;
                while i < bytes.len()
                    && !bytes[i].is_ascii_whitespace()
                    && !matches!(bytes[i], b')' | b',' | b';')
                {
                    let c = bytes[i];
                    i += 1;
                    if c == b'!' || c == b'?' {
                        break;
                    }
                }
                let err = match src[start..i].to_ascii_uppercase().as_str() {
                    "#DIV/0!" => FormulaError::DivByZero,
                    "#VALUE!" => FormulaError::Value,
                    "#REF!" => FormulaError::Ref,
                    "#CYCLE!" => FormulaError::Cycle,
                    "#NAME?" => FormulaError::Name,
                    "#N/A" => FormulaError::Na,
                    "#NUM!" => FormulaError::Num,
                    _ => return Err(FormulaError::Value),
                };
                tokens.push(Token::ErrorLiteral(err));
            }
            b'0'..=b'9' | b'.' => {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                    i += 1;
                }
                let val = src[start..i]
                    .parse::<f64>()
                    .map_err(|_| FormulaError::Value)?;
                if !val.is_finite() {
                    return Err(FormulaError::Num);
                }
                tokens.push(Token::Number(val));
            }
            b'A'..=b'Z' | b'a'..=b'z' | b'$' | b'_' => {
                let start = i;
                while i < bytes.len() {
                    let cur = bytes[i];
                    if cur.is_ascii_alphanumeric() || cur == b'$' || cur == b'_' {
                        i += 1;
                    } else if !cur.is_ascii() {
                        let ch = src[i..].chars().next().ok_or(FormulaError::Value)?;
                        if ch.is_alphanumeric() {
                            i += ch.len_utf8();
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                tokens.push(Token::Ident(src[start..i].to_string()));
            }
            _ => {
                let ch = src[i..].chars().next().ok_or(FormulaError::Value)?;
                if ch.is_whitespace() {
                    i += ch.len_utf8();
                } else if ch.is_alphabetic() {
                    let start = i;
                    while i < bytes.len() {
                        let c = src[i..].chars().next().ok_or(FormulaError::Value)?;
                        if c.is_alphanumeric() || c == '$' || c == '_' {
                            i += c.len_utf8();
                        } else {
                            break;
                        }
                    }
                    tokens.push(Token::Ident(src[start..i].to_string()));
                } else {
                    return Err(FormulaError::Value);
                }
            }
        }
    }

    Ok(tokens)
}

pub(crate) struct Parser {
    pub(crate) tokens: Vec<Token>,
    pub(crate) pos: usize,
}

impl Parser {
    pub(crate) fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    pub(crate) fn peek(&self) -> Option<&Token> {
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

    pub(crate) fn parse_expression(&mut self) -> Result<Expr, FormulaError> {
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
            Some(Token::ErrorLiteral(e)) => Ok(Expr::ErrorLiteral(e)),
            Some(Token::LParen) => {
                let inner = self.parse_expression()?;
                if !matches!(self.next(), Some(Token::RParen)) {
                    return Err(FormulaError::Value);
                }
                Ok(inner)
            }
            Some(Token::Ident(name)) => {
                if matches!(self.peek(), Some(Token::Bang)) {
                    self.next();
                    let Some(Token::Ident(cell_ident)) = self.next() else {
                        return Err(FormulaError::Ref);
                    };
                    let start_coord = PackedCellCoordinate::parse_a1(&cell_ident)
                        .map_err(|_| FormulaError::Ref)?;
                    if matches!(self.peek(), Some(Token::Colon)) {
                        self.next();
                        let end_ident = match self.next() {
                            Some(Token::Ident(s)) => {
                                if matches!(self.peek(), Some(Token::Bang)) {
                                    self.next();
                                    let Some(Token::Ident(s2)) = self.next() else {
                                        return Err(FormulaError::Ref);
                                    };
                                    s2
                                } else {
                                    s
                                }
                            }
                            _ => return Err(FormulaError::Ref),
                        };
                        let end_coord = PackedCellCoordinate::parse_a1(&end_ident)
                            .map_err(|_| FormulaError::Ref)?;
                        let range = CellRange::new(start_coord, end_coord)
                            .map_err(|_| FormulaError::Ref)?;
                        return Ok(Expr::SheetRange { sheet: name, range });
                    }
                    return Ok(Expr::SheetCell {
                        sheet: name,
                        coord: start_coord,
                    });
                }

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

pub fn compile_formula_expr(expr_src: &str) -> Result<Expr, FormulaError> {
    let tokens = tokenize_formula(expr_src)?;
    let mut parser = Parser::new(tokens);
    let expr = parser.parse_expression()?;
    if parser.pos != parser.tokens.len() {
        return Err(FormulaError::Value);
    }
    Ok(expr)
}

/// Compact 64-bit workbook coordinate packing `sheet_id (16 bits) | row (32 bits) | col (16 bits)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackedWorkbookCoord(pub u64);

impl PackedWorkbookCoord {
    #[inline]
    pub const fn new(sheet_id: u16, coord: PackedCellCoordinate) -> Self {
        let row_bits = ((coord.0 >> 32) & 0xFFFF_FFFF) << 16;
        let col_bits = coord.0 & 0xFFFF;
        Self(((sheet_id as u64) << 48) | row_bits | col_bits)
    }

    #[inline]
    pub const fn sheet_id(self) -> u16 {
        (self.0 >> 48) as u16
    }
}

/// Computes a deterministic 16-bit case-insensitive FNV-1a sheet identifier.
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

/// Zero-heap-allocation cycle tracker combining an inline 1,024-cell bitset (`64 rows × 16 cols`
/// on the root sheet) with a fixed-capacity 66-entry recursion stack.
pub struct VisitingTracker {
    primary_sheet_id: u16,
    grid_bitset: [u64; 16],
    stack: [u64; 66],
    len: usize,
}

impl VisitingTracker {
    #[inline]
    pub const fn new(primary_sheet_id: u16) -> Self {
        Self {
            primary_sheet_id,
            grid_bitset: [0u64; 16],
            stack: [0u64; 66],
            len: 0,
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
        if self.len >= self.stack.len() {
            return false;
        }
        self.stack[self.len] = packed;
        self.len += 1;
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
        if self.len > 0 && self.stack[self.len - 1] == packed {
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
        let all = sheet.recalculate_all();
        assert_eq!(
            all.get(&PackedCellCoordinate::parse_a1("D1").unwrap()),
            Some(&EvaluatedCellValue::Number(430.0))
        );
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

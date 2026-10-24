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
pub(crate) enum Expr {
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
pub(crate) enum BinaryOp {
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
            '!' => {
                tokens.push(Token::Bang);
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
            '\'' => {
                i += 1;
                let mut ident = String::new();
                let mut closed = false;
                while i < chars.len() {
                    if chars[i] == '\'' {
                        if i + 1 < chars.len() && chars[i + 1] == '\'' {
                            ident.push('\'');
                            i += 2;
                        } else {
                            i += 1;
                            closed = true;
                            break;
                        }
                    } else {
                        ident.push(chars[i]);
                        i += 1;
                    }
                }
                if !closed {
                    return Err(FormulaError::Value);
                }
                tokens.push(Token::Ident(ident));
            }
            '#' => {
                let start = i;
                while i < chars.len()
                    && !chars[i].is_whitespace()
                    && !matches!(chars[i], ')' | ',' | ';')
                {
                    let c = chars[i];
                    i += 1;
                    if c == '!' || c == '?' {
                        break;
                    }
                }
                let err_str: String = chars[start..i].iter().collect();
                let err = match err_str.to_ascii_uppercase().as_str() {
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
            c if c.is_alphabetic() || c == '$' || c == '_' => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_alphanumeric() || chars[i] == '$' || chars[i] == '_')
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

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

pub const MAX_FORMULA_LEN: usize = 8_192;
pub const MAX_TOKENS: usize = 2_048;
pub const MAX_PARSE_DEPTH: usize = 64;

fn split_once_unquoted(s: &str, delimiter: char) -> Option<(&str, &str)> {
    let mut in_single_quote = false;
    for (idx, ch) in s.char_indices() {
        if ch == '\'' {
            in_single_quote = !in_single_quote;
        } else if ch == delimiter && !in_single_quote {
            return Some((&s[..idx], &s[idx + ch.len_utf8()..]));
        }
    }
    None
}

fn rfind_unquoted_dot(s: &str) -> Option<usize> {
    let mut in_single_quote = false;
    let mut last_dot = None;
    for (idx, ch) in s.char_indices() {
        if ch == '\'' {
            in_single_quote = !in_single_quote;
        } else if ch == '.' && !in_single_quote {
            last_dot = Some(idx);
        }
    }
    last_dot
}

fn looks_like_ascii_cell_token(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut letters = 0usize;
    let mut digits = 0usize;
    for &b in bytes {
        if b.is_ascii_alphabetic() {
            if digits > 0 || letters >= 3 {
                return false;
            }
            letters += 1;
        } else if b.is_ascii_digit() {
            if letters == 0 {
                return false;
            }
            digits += 1;
        } else {
            return false;
        }
    }
    letters > 0 && digits > 0
}

pub(crate) fn is_valid_unquoted_sheet_ident(raw_sheet: &str) -> bool {
    let mut chars = raw_sheet.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_alphabetic() || first == '_' || first == '$') {
        return false;
    }
    chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

fn needs_sheet_quoting(raw_sheet: &str) -> bool {
    !is_valid_unquoted_sheet_ident(raw_sheet)
        || raw_sheet.contains('$')
        || raw_sheet.eq_ignore_ascii_case("TRUE")
        || raw_sheet.eq_ignore_ascii_case("FALSE")
        || looks_like_ascii_cell_token(raw_sheet)
}

fn classify_failed_a1_token(raw: &str) -> FormulaError {
    let bytes = raw.trim().as_bytes();
    let mut i = usize::from(bytes.first() == Some(&b'$'));
    let mut letters = 0usize;
    while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
        letters += 1;
        i += 1;
    }
    if !(1..=3).contains(&letters) {
        return FormulaError::Name;
    }
    if bytes.get(i) == Some(&b'$') {
        i += 1;
    }
    let digits = &bytes[i..];
    if !digits.is_empty() && digits.iter().all(u8::is_ascii_digit) {
        FormulaError::Ref
    } else {
        FormulaError::Name
    }
}

fn normalize_bracketed_open_formula_ref(inner: &str) -> String {
    fn split_sheet_and_cell(part: &str) -> (Option<String>, &str) {
        let trimmed = part.trim();
        if let Some(rest) = trimmed.strip_prefix('.') {
            return (None, rest.trim());
        }
        if let Some(dot_idx) = rfind_unquoted_dot(trimmed) {
            let raw_sheet = trimmed[..dot_idx].trim().trim_start_matches('$');
            let cell_part = trimmed[dot_idx + 1..].trim();
            if raw_sheet.is_empty() {
                return (None, cell_part);
            }
            let formatted_sheet = if raw_sheet.starts_with('\'') && raw_sheet.ends_with('\'') {
                raw_sheet.to_string()
            } else if needs_sheet_quoting(raw_sheet) {
                format!("'{}'", raw_sheet.replace('\'', "''"))
            } else {
                raw_sheet.to_string()
            };
            return (Some(formatted_sheet), cell_part);
        }
        (None, trimmed)
    }

    fn format_endpoint(part: &str) -> String {
        let (sheet, cell) = split_sheet_and_cell(part);
        if let Some(sheet) = sheet {
            format!("{sheet}!{cell}")
        } else {
            cell.to_string()
        }
    }

    if let Some((lhs, rhs)) = split_once_unquoted(inner, ':') {
        return format!("{}:{}", format_endpoint(lhs), format_endpoint(rhs));
    }

    format_endpoint(inner)
}

#[inline]
fn strip_formula_namespace(raw_src: &str) -> &str {
    let trimmed = raw_src.trim();
    if trimmed
        .get(..3)
        .is_some_and(|s| s.eq_ignore_ascii_case("of:"))
    {
        trimmed[3..].trim_start()
    } else if trimmed
        .get(..5)
        .is_some_and(|s| s.eq_ignore_ascii_case("oooc:"))
    {
        trimmed[5..].trim_start()
    } else if trimmed
        .get(..6)
        .is_some_and(|s| s.eq_ignore_ascii_case("msoxl:"))
    {
        trimmed[6..].trim_start()
    } else {
        trimmed
    }
}

#[inline]
pub(crate) fn strip_formula_prefix(raw_src: &str) -> Option<&str> {
    strip_formula_namespace(raw_src).strip_prefix('=')
}

/// Normalizes ODF OpenFormula (`of:=`, `[.A1:.B2]`, `['Sheet 1'.A1:.B2]`, `[Sheet1.A1]`) references into standard A1 syntax.
pub fn normalize_open_formula_references(raw_src: &str) -> std::borrow::Cow<'_, str> {
    let after_ns = strip_formula_namespace(raw_src);
    let stripped = after_ns.strip_prefix('=').unwrap_or(after_ns);
    if !stripped.contains('[') {
        return std::borrow::Cow::Borrowed(stripped);
    }

    let mut out = String::with_capacity(stripped.len());
    let mut in_double_quote = false;
    let mut in_single_quote = false;
    let mut chars = stripped.char_indices().peekable();
    while let Some((idx, ch)) = chars.next() {
        if ch == '"' && !in_single_quote {
            in_double_quote = !in_double_quote;
            out.push(ch);
            continue;
        }
        if ch == '\'' && !in_double_quote {
            if in_single_quote && chars.peek().is_some_and(|&(_, next_ch)| next_ch == '\'') {
                out.push('\'');
                out.push('\'');
                chars.next();
            } else {
                in_single_quote = !in_single_quote;
                out.push(ch);
            }
            continue;
        }
        if !in_double_quote && !in_single_quote && ch == '[' {
            let start_inner = idx + ch.len_utf8();
            let mut close_idx = None;
            let mut bracket_single_quote = false;
            while let Some(&(c_idx, c_ch)) = chars.peek() {
                chars.next();
                if c_ch == '\'' {
                    bracket_single_quote = !bracket_single_quote;
                } else if c_ch == ']' && !bracket_single_quote {
                    close_idx = Some(c_idx);
                    break;
                }
            }
            if let Some(end_inner) = close_idx {
                let inner = &stripped[start_inner..end_inner];
                out.push_str(&normalize_bracketed_open_formula_ref(inner));
                continue;
            }
            out.push('[');
            out.push_str(&stripped[start_inner..]);
            break;
        }
        out.push(ch);
    }
    std::borrow::Cow::Owned(out)
}

pub(crate) fn tokenize_formula(raw_src: &str) -> Result<Vec<Token>, FormulaError> {
    if raw_src.len() > MAX_FORMULA_LEN {
        return Err(FormulaError::Value);
    }
    let normalized = normalize_open_formula_references(raw_src);
    let src: &str = normalized.as_ref();
    if src.len() > MAX_FORMULA_LEN {
        return Err(FormulaError::Value);
    }
    let bytes = src.as_bytes();
    let mut tokens = Vec::with_capacity((bytes.len() / 2).clamp(4, MAX_TOKENS));
    let mut i = 0usize;

    while i < bytes.len() {
        if tokens.len() >= MAX_TOKENS {
            return Err(FormulaError::Value);
        }
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
                    "#VALUE!" | "#NULL!" => FormulaError::Value,
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
                if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
                    if i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
                        i += 1;
                        while i < bytes.len() && bytes[i].is_ascii_digit() {
                            i += 1;
                        }
                    } else if i + 2 < bytes.len()
                        && (bytes[i + 1] == b'+' || bytes[i + 1] == b'-')
                        && bytes[i + 2].is_ascii_digit()
                    {
                        i += 2;
                        while i < bytes.len() && bytes[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
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
            let tok = std::mem::replace(&mut self.tokens[self.pos], Token::Comma);
            self.pos += 1;
            Some(tok)
        } else {
            None
        }
    }

    pub(crate) fn parse_expression(&mut self, depth: usize) -> Result<Expr, FormulaError> {
        if depth > MAX_PARSE_DEPTH {
            return Err(FormulaError::Value);
        }
        self.parse_comparison(depth)
    }

    fn parse_comparison(&mut self, depth: usize) -> Result<Expr, FormulaError> {
        let mut lhs = self.parse_concat(depth)?;
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
            let rhs = self.parse_concat(depth)?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_concat(&mut self, depth: usize) -> Result<Expr, FormulaError> {
        let mut lhs = self.parse_additive(depth)?;
        while matches!(self.peek(), Some(Token::Ampersand)) {
            self.next();
            let rhs = self.parse_additive(depth)?;
            lhs = Expr::Binary {
                op: BinaryOp::Concat,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_additive(&mut self, depth: usize) -> Result<Expr, FormulaError> {
        let mut lhs = self.parse_multiplicative(depth)?;
        while let Some(tok) = self.peek() {
            let op = match tok {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                _ => break,
            };
            self.next();
            let rhs = self.parse_multiplicative(depth)?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_multiplicative(&mut self, depth: usize) -> Result<Expr, FormulaError> {
        let mut lhs = self.parse_power(depth)?;
        while let Some(tok) = self.peek() {
            let op = match tok {
                Token::Star => BinaryOp::Mul,
                Token::Slash => BinaryOp::Div,
                _ => break,
            };
            self.next();
            let rhs = self.parse_power(depth)?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_power(&mut self, depth: usize) -> Result<Expr, FormulaError> {
        let mut lhs = self.parse_unary(depth)?;
        while matches!(self.peek(), Some(Token::Caret)) {
            self.next();
            let rhs = self.parse_unary(depth)?;
            lhs = Expr::Binary {
                op: BinaryOp::Pow,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self, depth: usize) -> Result<Expr, FormulaError> {
        if depth > MAX_PARSE_DEPTH {
            return Err(FormulaError::Value);
        }
        if matches!(self.peek(), Some(Token::Plus)) {
            self.next();
            return Ok(Expr::UnaryPlus(Box::new(self.parse_unary(depth + 1)?)));
        }
        if matches!(self.peek(), Some(Token::Minus)) {
            self.next();
            return Ok(Expr::UnaryMinus(Box::new(self.parse_unary(depth + 1)?)));
        }
        self.parse_postfix(depth)
    }

    fn parse_postfix(&mut self, depth: usize) -> Result<Expr, FormulaError> {
        let mut expr = self.parse_primary(depth)?;
        let mut pct_depth = depth;
        while matches!(self.peek(), Some(Token::Percent)) {
            pct_depth += 1;
            if pct_depth > MAX_PARSE_DEPTH {
                return Err(FormulaError::Value);
            }
            self.next();
            expr = Expr::PostfixPercent(Box::new(expr));
        }
        Ok(expr)
    }

    fn parse_primary(&mut self, depth: usize) -> Result<Expr, FormulaError> {
        if depth > MAX_PARSE_DEPTH {
            return Err(FormulaError::Value);
        }
        match self.next() {
            Some(Token::Number(n)) => Ok(Expr::Number(n)),
            Some(Token::Text(s)) => Ok(Expr::Text(s)),
            Some(Token::ErrorLiteral(e)) => Ok(Expr::ErrorLiteral(e)),
            Some(Token::LParen) => {
                let inner = self.parse_expression(depth + 1)?;
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
                    let start_coord = PackedCellCoordinate::parse_a1(&cell_ident);
                    if matches!(self.peek(), Some(Token::Colon)) {
                        self.next();
                        let Some(Token::Ident(s)) = self.next() else {
                            return Err(FormulaError::Ref);
                        };
                        let end_ident = if matches!(self.peek(), Some(Token::Bang)) {
                            self.next();
                            if !crate::WorkbookEngine::is_same_sheet_identity(&s, &name) {
                                return Err(FormulaError::Ref);
                            }
                            let Some(Token::Ident(s2)) = self.next() else {
                                return Err(FormulaError::Ref);
                            };
                            s2
                        } else {
                            s
                        };
                        let (Ok(sc), Ok(ec)) =
                            (start_coord, PackedCellCoordinate::parse_a1(&end_ident))
                        else {
                            return Ok(Expr::ErrorLiteral(FormulaError::Ref));
                        };
                        let Ok(range) = CellRange::new(sc, ec) else {
                            return Ok(Expr::ErrorLiteral(FormulaError::Ref));
                        };
                        return Ok(Expr::SheetRange { sheet: name, range });
                    }
                    let Ok(sc) = start_coord else {
                        return Ok(Expr::ErrorLiteral(FormulaError::Ref));
                    };
                    return Ok(Expr::SheetCell {
                        sheet: name,
                        coord: sc,
                    });
                }

                if matches!(self.peek(), Some(Token::LParen)) {
                    self.next();
                    let mut args = Vec::new();
                    if !matches!(self.peek(), Some(Token::RParen)) {
                        loop {
                            args.push(self.parse_expression(depth + 1)?);
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

                let start_coord = PackedCellCoordinate::parse_a1(&name);
                if matches!(self.peek(), Some(Token::Colon)) {
                    self.next();
                    let Some(Token::Ident(end_name)) = self.next() else {
                        return Err(FormulaError::Ref);
                    };
                    if matches!(self.peek(), Some(Token::Bang)) {
                        return Err(FormulaError::Ref);
                    }
                    let (Ok(sc), Ok(ec)) = (start_coord, PackedCellCoordinate::parse_a1(&end_name))
                    else {
                        return Ok(Expr::ErrorLiteral(FormulaError::Ref));
                    };
                    let Ok(range) = CellRange::new(sc, ec) else {
                        return Ok(Expr::ErrorLiteral(FormulaError::Ref));
                    };
                    return Ok(Expr::Range(range));
                }
                let Ok(sc) = start_coord else {
                    return Ok(Expr::ErrorLiteral(classify_failed_a1_token(&name)));
                };
                Ok(Expr::Cell(sc))
            }
            _ => Err(FormulaError::Value),
        }
    }
}

pub fn compile_formula_expr(expr_src: &str) -> Result<Expr, FormulaError> {
    let tokens = tokenize_formula(expr_src)?;
    let mut parser = Parser::new(tokens);
    let expr = parser.parse_expression(0)?;
    if parser.pos != parser.tokens.len() {
        return Err(FormulaError::Value);
    }
    Ok(expr)
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

        let a1 = PackedWorkbookCoord::new(7, PackedCellCoordinate::parse_a1("A1").unwrap());
        let a2 = PackedWorkbookCoord::new(7, PackedCellCoordinate::parse_a1("A2").unwrap());
        let a70000 = PackedWorkbookCoord::new(7, PackedCellCoordinate::parse_a1("A70000").unwrap());
        assert_ne!(a1, a2);
        assert_ne!(a1, a70000);
        assert_ne!(a2, a70000);
        assert_eq!(a70000.sheet_id(), 7);
        assert_eq!(a70000.row(), 69_999);
        assert_eq!(a70000.col(), 0);

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

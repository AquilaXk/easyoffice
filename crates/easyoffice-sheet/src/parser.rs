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
    Missing,
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
pub const MAX_TOKENS: usize = 8_192;
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
pub(crate) fn strip_open_formula_prefix(raw_src: &str) -> Option<&str> {
    let trimmed = raw_src.trim();
    let after_ns = if trimmed
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
    };
    after_ns.strip_prefix('=')
}

#[inline]
pub(crate) fn strip_formula_prefix(raw_src: &str) -> Option<&str> {
    raw_src.trim().strip_prefix('=')
}

fn normalize_open_formula_body(stripped: &str) -> std::borrow::Cow<'_, str> {
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

/// Normalizes ODF OpenFormula (`of:=`, `[.A1:.B2]`, `['Sheet 1'.A1:.B2]`, `[Sheet1.A1]`) references into standard A1 syntax.
pub fn normalize_open_formula_references(raw_src: &str) -> String {
    if let Some(stripped) = strip_open_formula_prefix(raw_src) {
        let body = normalize_open_formula_body(stripped);
        format!("={body}")
    } else {
        normalize_open_formula_body(raw_src.trim()).into_owned()
    }
}

pub(crate) fn tokenize_formula(raw_src: &str) -> Result<Vec<Token>, FormulaError> {
    if raw_src.len() > MAX_FORMULA_LEN {
        return Err(FormulaError::Value);
    }
    let normalized = normalize_open_formula_body(raw_src);
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
                let rest = &src[start..];
                if rest.len() >= 4
                    && rest[..4].eq_ignore_ascii_case("#N/A")
                    && !rest.as_bytes().get(4).is_some_and(|c| {
                        c.is_ascii_alphanumeric() || matches!(c, b'_' | b'!' | b'?')
                    })
                {
                    i += 4;
                    tokens.push(Token::ErrorLiteral(FormulaError::Na));
                    continue;
                }
                while i < bytes.len()
                    && !bytes[i].is_ascii_whitespace()
                    && !matches!(
                        bytes[i],
                        b')' | b'('
                            | b','
                            | b';'
                            | b'+'
                            | b'-'
                            | b'*'
                            | b'^'
                            | b'&'
                            | b'='
                            | b'<'
                            | b'>'
                            | b'%'
                    )
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

    fn next_range_endpoint_string(&mut self) -> Option<String> {
        match self.next() {
            Some(Token::Ident(s)) => Some(s),
            Some(Token::Number(n)) if n.fract() == 0.0 && n >= 1.0 => Some(format!("{}", n as u64)),
            _ => None,
        }
    }

    fn parse_primary(&mut self, depth: usize) -> Result<Expr, FormulaError> {
        if depth > MAX_PARSE_DEPTH {
            return Err(FormulaError::Value);
        }
        match self.next() {
            Some(Token::Number(n)) => {
                if matches!(self.peek(), Some(Token::Colon)) {
                    self.next();
                    if n.fract() != 0.0 || n < 1.0 {
                        return Ok(Expr::ErrorLiteral(FormulaError::Ref));
                    }
                    let Some(end_str) = self.next_range_endpoint_string() else {
                        return Err(FormulaError::Ref);
                    };
                    if matches!(self.peek(), Some(Token::Bang)) {
                        return Err(FormulaError::Ref);
                    }
                    let notation = format!("{}:{end_str}", n as u64);
                    let Ok(range) = CellRange::parse(&notation) else {
                        return Ok(Expr::ErrorLiteral(FormulaError::Ref));
                    };
                    return Ok(Expr::Range(range));
                }
                Ok(Expr::Number(n))
            }
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
                    let Some(cell_ident) = self.next_range_endpoint_string() else {
                        return Err(FormulaError::Ref);
                    };
                    if matches!(self.peek(), Some(Token::Colon)) {
                        self.next();
                        let Some(s) = self.next_range_endpoint_string() else {
                            return Err(FormulaError::Ref);
                        };
                        let (end_ident, same_sheet) = if matches!(self.peek(), Some(Token::Bang)) {
                            self.next();
                            let Some(s2) = self.next_range_endpoint_string() else {
                                return Err(FormulaError::Ref);
                            };
                            (s2, crate::WorkbookEngine::is_same_sheet_identity(&s, &name))
                        } else {
                            (s, true)
                        };
                        if !same_sheet {
                            return Ok(Expr::ErrorLiteral(FormulaError::Ref));
                        }
                        let notation = format!("{cell_ident}:{end_ident}");
                        let Ok(range) = CellRange::parse(&notation) else {
                            return Ok(Expr::ErrorLiteral(FormulaError::Ref));
                        };
                        return Ok(Expr::SheetRange { sheet: name, range });
                    }
                    let Ok(sc) = PackedCellCoordinate::parse_a1(&cell_ident) else {
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
                            if matches!(self.peek(), Some(Token::Comma) | Some(Token::RParen)) {
                                args.push(Expr::Missing);
                            } else {
                                args.push(self.parse_expression(depth + 1)?);
                            }
                            if matches!(self.peek(), Some(Token::Comma)) {
                                self.next();
                                if matches!(self.peek(), Some(Token::RParen)) {
                                    args.push(Expr::Missing);
                                    break;
                                }
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

                if matches!(self.peek(), Some(Token::Colon)) {
                    self.next();
                    let Some(end_name) = self.next_range_endpoint_string() else {
                        return Err(FormulaError::Ref);
                    };
                    if matches!(self.peek(), Some(Token::Bang)) {
                        return Err(FormulaError::Ref);
                    }
                    let notation = format!("{name}:{end_name}");
                    let Ok(range) = CellRange::parse(&notation) else {
                        return Ok(Expr::ErrorLiteral(FormulaError::Ref));
                    };
                    return Ok(Expr::Range(range));
                }
                let Ok(sc) = PackedCellCoordinate::parse_a1(&name) else {
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

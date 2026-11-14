use crate::names::{name_key, NameKey};
use crate::table_index::{implicit_key, TableCtx};
use crate::{CellRange, FormulaError, PackedCellCoordinate};
use std::sync::Arc;

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
    LBrace,
    RBrace,
    /// The row separator of an array constant (outside braces `;` reads as a comma).
    Semicolon,
    /// The bracket group of a structured reference (`[Q1]`, `[[#This Row],[Q1]]`, `[@Q1]`), the table's
    /// name before it (if any) being the identifier token in front.
    Structured(String),
    /// The spill range operator after a reference (`A1#`).
    Hash,
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
    /// A defined name (PD-214), optionally written with its sheet (`Sheet1!Rate`). A formula compiled
    /// in a workbook has these replaced by the definitions; one that is still here reads `#NAME?`.
    Name {
        scope: Option<String>,
        name: String,
    },
    /// An expanded definition of a defined name that many formulas use, shared instead of copied (PD-214). It stands
    /// only where a value is read (an operand, a whole formula): a function's arguments get their own copy of the
    /// top layer, so the functions that look at the shape of an argument see references and calls as written.
    Shared(Arc<SharedExpr>),
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
    /// An array constant (`{1,2;3,4}`).
    Array(Arc<ArrayLit>),
    /// A variable of `LET` or a parameter of `LAMBDA` (upper case); it reads `#NAME?` outside them.
    Var(String),
    /// The spill range of the formula at a cell (`A1#`, `Sheet1!A1#`).
    SpillRef {
        sheet: Option<String>,
        coord: PackedCellCoordinate,
    },
    /// A call of a `LAMBDA` value: `LAMBDA(x, x+1)(2)`, a variable that holds one, or a defined name.
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
}

/// The values of an array constant, row by row.
#[derive(Debug, Clone, PartialEq)]
pub struct ArrayLit {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<crate::EvaluatedCellValue>,
}

/// A syntax tree that formulas share, with the number of nodes it stands for.
#[derive(Debug, Clone, PartialEq)]
pub struct SharedExpr {
    pub expr: Expr,
    /// Nodes of the tree with every shared tree inside it counted in full (what evaluating it takes).
    pub nodes: usize,
}

impl Expr {
    /// The expression itself, looking through [`Expr::Shared`].
    pub fn peeled(&self) -> &Expr {
        let mut cur = self;
        while let Expr::Shared(shared) = cur {
            cur = &shared.expr;
        }
        cur
    }
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

pub(crate) fn needs_sheet_quoting(raw_sheet: &str) -> bool {
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

/// Consumes characters up to the `]` that closes an open bracket (quotes may contain `]`);
/// returns its byte index, or `None` when the bracket is never closed.
fn find_bracket_close(chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>) -> Option<usize> {
    let mut in_quote = false;
    for (c_idx, c_ch) in chars.by_ref() {
        if c_ch == '\'' {
            in_quote = !in_quote;
        } else if c_ch == ']' && !in_quote {
            return Some(c_idx);
        }
    }
    None
}

/// Whether the `[` at `open` starts a structured reference: it follows a table's name, or its first character
/// is `@`, `#` or `[` (an ODF reference starts with a dot, a quote or a sheet name).
fn is_structured_group(text: &str, open: usize, out: &str) -> bool {
    let named = out
        .chars()
        .next_back()
        .is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '\\'));
    named
        || matches!(
            text[open + 1..].trim_start_matches(' ').as_bytes().first(),
            Some(b'@' | b'#' | b'[')
        )
}

pub(crate) fn normalize_open_formula_body(stripped: &str) -> std::borrow::Cow<'_, str> {
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
        if !in_double_quote
            && !in_single_quote
            && ch == '['
            && is_structured_group(stripped, idx, &out)
        {
            // `Table1[Q1]`, `[@Q1]`: a structured reference (PD-225), which the lexer reads as one token.
            let end = easyoffice_formats::group_end(stripped, idx).unwrap_or(stripped.len());
            out.push_str(&stripped[idx..end]);
            while chars.peek().is_some_and(|&(i, _)| i < end) {
                chars.next();
            }
            continue;
        }
        if !in_double_quote && !in_single_quote && ch == '[' {
            let start_inner = idx + ch.len_utf8();
            if let Some(end_inner) = find_bracket_close(&mut chars) {
                let inner = &stripped[start_inner..end_inner];
                // `[x]` in a parameter list of `LAMBDA`: an optional parameter (a reference always has a dot).
                let before = out.trim_end().chars().next_back();
                let word = !inner.is_empty()
                    && inner.chars().all(|c| c.is_alphanumeric() || c == '_')
                    && !inner.starts_with(|c: char| c.is_ascii_digit());
                if word && matches!(before, Some('(' | ',')) {
                    out.push_str("_xlop.");
                    out.push_str(inner);
                } else {
                    out.push_str(&normalize_bracketed_open_formula_ref(inner));
                }
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

#[derive(Clone, Copy, Debug)]
pub(crate) struct RefEndpointSpec {
    pub(crate) coord: PackedCellCoordinate,
    pub(crate) shift_row: bool,
    pub(crate) shift_col: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum RefSpec {
    Fixed,
    Cell(RefEndpointSpec),
    Range {
        start: RefEndpointSpec,
        end: RefEndpointSpec,
    },
}

fn cell_endpoint_spec(raw: &str, coord: PackedCellCoordinate) -> RefEndpointSpec {
    let (col_abs, after_col_dollar) = match raw.strip_prefix('$') {
        Some(rest) => (true, rest),
        None => (false, raw),
    };
    let letters_end = after_col_dollar
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(after_col_dollar.len());
    let (letters, rest) = after_col_dollar.split_at(letters_end);
    let (row_abs, digits) = match rest.strip_prefix('$') {
        Some(r) => (true, r),
        None => (false, rest),
    };
    let is_cell = !letters.is_empty()
        && letters.len() <= 3
        && !digits.is_empty()
        && digits.len() <= 7
        && digits.bytes().all(|b| b.is_ascii_digit());
    RefEndpointSpec {
        coord,
        shift_row: is_cell && !row_abs,
        shift_col: is_cell && !col_abs,
    }
}

fn range_ref_spec(start_raw: &str, end_raw: &str) -> RefSpec {
    match (
        PackedCellCoordinate::parse_a1(start_raw),
        PackedCellCoordinate::parse_a1(end_raw),
    ) {
        (Ok(c0), Ok(c1)) => {
            let s0 = cell_endpoint_spec(start_raw, c0);
            let s1 = cell_endpoint_spec(end_raw, c1);
            if s0.shift_row || s0.shift_col || s1.shift_row || s1.shift_col {
                RefSpec::Range { start: s0, end: s1 }
            } else {
                RefSpec::Fixed
            }
        }
        _ => RefSpec::Fixed,
    }
}

pub(crate) use crate::lexer::tokenize_formula;

pub(crate) struct Parser<'a> {
    pub(crate) tokens: Vec<Token>,
    pub(crate) pos: usize,
    /// A defined name was read (see [`Expr::Name`]).
    pub(crate) saw_name: bool,
    /// `LET` or `LAMBDA` was read: its variables are told from defined names after parsing.
    pub(crate) saw_scope: bool,
    /// The tables structured references are resolved against; without them such a reference reads `#NAME?`.
    tables: Option<&'a TableCtx<'a>>,
    /// The hash of each table a structured reference named (see [`crate::table_index`]).
    table_keys: Vec<NameKey>,
    /// Optional per-leaf shiftability descriptors recorded alongside parsed references.
    ref_specs: Option<Vec<RefSpec>>,
}

impl<'a> Parser<'a> {
    pub(crate) fn new(tokens: Vec<Token>, tables: Option<&'a TableCtx<'a>>) -> Self {
        Self {
            tokens,
            pos: 0,
            saw_name: false,
            saw_scope: false,
            tables,
            table_keys: Vec::new(),
            ref_specs: None,
        }
    }

    fn record_ref(&mut self, spec: RefSpec) {
        if let Some(specs) = &mut self.ref_specs {
            specs.push(spec);
        }
    }

    /// A structured reference: `table` is the name in front of the group (none for `[@Q1]`).
    fn structured_reference(
        &mut self,
        table: Option<&str>,
        group: &str,
    ) -> Result<Expr, FormulaError> {
        let sref = easyoffice_formats::parse_group(group).ok_or(FormulaError::Value)?;
        self.table_keys.push(match table {
            Some(name) => name_key(&easyoffice_formats::fold_name(name)),
            None => implicit_key(self.tables.map_or("", |ctx| ctx.sheet)),
        });
        let resolved = match self.tables {
            Some(ctx) => ctx.index.resolve(ctx, table, &sref),
            None => Expr::ErrorLiteral(FormulaError::Name),
        };
        if matches!(
            resolved,
            Expr::Cell(_)
                | Expr::Range(_)
                | Expr::SheetCell { .. }
                | Expr::SheetRange { .. }
                | Expr::SpillRef { .. }
        ) {
            self.record_ref(RefSpec::Fixed);
        }
        Ok(resolved)
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
        // `LAMBDA(x, x+1)(2)`: a function call result that is called again.
        while matches!(self.peek(), Some(Token::LParen))
            && (matches!(&expr, Expr::FuncCall { name, .. } if name == "LAMBDA")
                || matches!(&expr, Expr::Call { .. }))
        {
            self.next();
            let args = self.parse_call_args(depth)?;
            expr = Expr::Call {
                callee: Box::new(expr),
                args,
            };
        }
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
            Some(Token::Number(n)) => self.parse_number_primary(n),
            Some(Token::Text(s)) => Ok(Expr::Text(s)),
            Some(Token::ErrorLiteral(e)) => Ok(Expr::ErrorLiteral(e)),
            Some(Token::LParen) => {
                let inner = self.parse_expression(depth + 1)?;
                if !matches!(self.next(), Some(Token::RParen)) {
                    return Err(FormulaError::Value);
                }
                Ok(inner)
            }
            Some(Token::Ident(name)) => self.parse_ident_primary(name, depth),
            Some(Token::Structured(group)) => self.structured_reference(None, &group),
            Some(Token::LBrace) => self.parse_array_literal(),
            _ => Err(FormulaError::Value),
        }
    }

    /// A number, or a whole-row range such as `1:3`.
    fn parse_number_primary(&mut self, n: f64) -> Result<Expr, FormulaError> {
        if !matches!(self.peek(), Some(Token::Colon)) {
            return Ok(Expr::Number(n));
        }
        self.next();
        if n.fract() != 0.0 || n < 1.0 {
            return Ok(Expr::ErrorLiteral(FormulaError::Ref));
        }
        self.finish_range(&format!("{}", n as u64))
    }

    /// Reads the range end after `start:` and builds the range (a trailing `!` is invalid).
    fn finish_range(&mut self, start: &str) -> Result<Expr, FormulaError> {
        let Some(end) = self.next_range_endpoint_string() else {
            return Err(FormulaError::Ref);
        };
        if matches!(self.peek(), Some(Token::Bang)) {
            return Err(FormulaError::Ref);
        }
        match CellRange::parse_endpoints(start, &end) {
            Ok(range) => {
                if self.ref_specs.is_some() {
                    let spec = range_ref_spec(start, &end);
                    self.record_ref(spec);
                }
                Ok(Expr::Range(range))
            }
            Err(_) => Ok(Expr::ErrorLiteral(FormulaError::Ref)),
        }
    }

    fn parse_ident_primary(&mut self, name: String, depth: usize) -> Result<Expr, FormulaError> {
        if matches!(self.peek(), Some(Token::Bang)) {
            self.next();
            return self.parse_sheet_reference(name);
        }
        if let Some(Token::Structured(group)) = self.peek() {
            let group = group.clone();
            self.next();
            return self.structured_reference(Some(&name), &group);
        }
        if matches!(self.peek(), Some(Token::LParen)) {
            self.next();
            let args = self.parse_call_args(depth)?;
            let canonical = match normalize_korean_function_aliases(&name) {
                std::borrow::Cow::Borrowed(s)
                    if s.len() != name.len() || s.as_ptr() != name.as_ptr() =>
                {
                    s.to_string()
                }
                std::borrow::Cow::Owned(s) => s,
                _ => name.to_ascii_uppercase(),
            };
            self.saw_scope |= canonical == "LET" || canonical == "LAMBDA";
            // A name that is no function may be a defined name that holds a `LAMBDA`.
            self.saw_name |= !crate::names::is_function(&canonical);
            if canonical == "ANCHORARRAY" {
                if let [Expr::Cell(coord)] = args.as_slice() {
                    return Ok(Expr::SpillRef {
                        sheet: None,
                        coord: *coord,
                    });
                }
            }
            return Ok(Expr::FuncCall {
                name: canonical,
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
            return self.finish_range(&name);
        }
        match PackedCellCoordinate::parse_a1(&name) {
            Ok(sc) if self.take_hash() => {
                if self.ref_specs.is_some() {
                    self.record_ref(RefSpec::Cell(cell_endpoint_spec(&name, sc)));
                }
                Ok(Expr::SpillRef {
                    sheet: None,
                    coord: sc,
                })
            }
            Ok(sc) => {
                if self.ref_specs.is_some() {
                    self.record_ref(RefSpec::Cell(cell_endpoint_spec(&name, sc)));
                }
                Ok(Expr::Cell(sc))
            }
            Err(_) => Ok(self.name_or_error(None, name)),
        }
    }

    /// A word that is not a cell address: a defined name, or the error its shape says.
    fn name_or_error(&mut self, scope: Option<String>, name: String) -> Expr {
        // `r` and `c` alone are valid names of `LET` variables and `LAMBDA` parameters.
        let loose = matches!(name.as_str(), "r" | "R" | "c" | "C");
        match classify_failed_a1_token(&name) {
            _ if loose && scope.is_none() => {
                self.saw_name = true;
                Expr::Name { scope, name }
            }
            FormulaError::Name if easyoffice_formats::defined_name_problem(&name).is_none() => {
                self.saw_name = true;
                Expr::Name { scope, name }
            }
            error => Expr::ErrorLiteral(error),
        }
    }

    /// `Sheet!A1` or `Sheet!A1:B2` (also `Sheet!A1:Sheet!B2`) after the `!`.
    fn parse_sheet_reference(&mut self, name: String) -> Result<Expr, FormulaError> {
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
            return match CellRange::parse_endpoints(&cell_ident, &end_ident) {
                Ok(range) => {
                    if self.ref_specs.is_some() {
                        let spec = range_ref_spec(&cell_ident, &end_ident);
                        self.record_ref(spec);
                    }
                    Ok(Expr::SheetRange { sheet: name, range })
                }
                Err(_) => Ok(Expr::ErrorLiteral(FormulaError::Ref)),
            };
        }
        match PackedCellCoordinate::parse_a1(&cell_ident) {
            Ok(coord) if self.take_hash() => {
                if self.ref_specs.is_some() {
                    self.record_ref(RefSpec::Cell(cell_endpoint_spec(&cell_ident, coord)));
                }
                Ok(Expr::SpillRef {
                    sheet: Some(name),
                    coord,
                })
            }
            Ok(coord) => {
                if self.ref_specs.is_some() {
                    self.record_ref(RefSpec::Cell(cell_endpoint_spec(&cell_ident, coord)));
                }
                Ok(Expr::SheetCell { sheet: name, coord })
            }
            Err(_) => match self.name_or_error(Some(name), cell_ident) {
                named @ Expr::Name { .. } => Ok(named),
                _ => Ok(Expr::ErrorLiteral(FormulaError::Ref)),
            },
        }
    }

    /// Function call arguments after `(`; empty slots become [`Expr::Missing`].
    fn parse_call_args(&mut self, depth: usize) -> Result<Vec<Expr>, FormulaError> {
        let mut args = Vec::new();
        if !matches!(self.peek(), Some(Token::RParen)) {
            loop {
                if matches!(self.peek(), Some(Token::Comma | Token::RParen)) {
                    args.push(Expr::Missing);
                } else {
                    args.push(self.parse_expression(depth + 1)?);
                }
                if !matches!(self.peek(), Some(Token::Comma)) {
                    break;
                }
                self.next();
                if matches!(self.peek(), Some(Token::RParen)) {
                    args.push(Expr::Missing);
                    break;
                }
            }
        }
        if !matches!(self.next(), Some(Token::RParen)) {
            return Err(FormulaError::Value);
        }
        Ok(args)
    }
}

#[inline]
pub(crate) fn normalize_korean_function_aliases(expr: &str) -> std::borrow::Cow<'_, str> {
    if expr.is_ascii() {
        return std::borrow::Cow::Borrowed(expr);
    }
    match expr {
        "합계" => std::borrow::Cow::Borrowed("SUM"),
        "평균" => std::borrow::Cow::Borrowed("AVERAGE"),
        "최대" | "최댓값" => std::borrow::Cow::Borrowed("MAX"),
        "최소" | "최솟값" => std::borrow::Cow::Borrowed("MIN"),
        "개수" => std::borrow::Cow::Borrowed("COUNT"),
        "오늘" => std::borrow::Cow::Borrowed("TODAY"),
        "지금" => std::borrow::Cow::Borrowed("NOW"),
        "만약" | "조건" => std::borrow::Cow::Borrowed("IF"),
        "반올림" => std::borrow::Cow::Borrowed("ROUND"),
        _ => std::borrow::Cow::Borrowed(expr),
    }
}

/// A formula body parsed: the syntax tree, whether it uses defined names (see [`Expr::Name`]) and the hash
/// of each table a structured reference in it names.
pub(crate) struct Parsed {
    pub(crate) expr: Expr,
    pub(crate) saw_name: bool,
    pub(crate) table_keys: Vec<NameKey>,
}

/// Parses a formula body; structured references are resolved against `tables` (see [`TableCtx`]).
pub(crate) fn parse_formula_in(
    expr_src: &str,
    tables: Option<&TableCtx<'_>>,
) -> Result<Parsed, FormulaError> {
    parse_formula_inner(expr_src, tables, false).map(|(parsed, _)| parsed)
}

/// Parses a formula body while collecting shiftability specs for each written reference node.
pub(crate) fn parse_formula_with_refs(
    expr_src: &str,
    tables: Option<&TableCtx<'_>>,
) -> Result<(Parsed, Vec<RefSpec>), FormulaError> {
    parse_formula_inner(expr_src, tables, true)
}

fn parse_formula_inner(
    expr_src: &str,
    tables: Option<&TableCtx<'_>>,
    capture_refs: bool,
) -> Result<(Parsed, Vec<RefSpec>), FormulaError> {
    let tokens = tokenize_formula(expr_src)?;
    let mut parser = Parser::new(tokens, tables);
    if capture_refs {
        parser.ref_specs = Some(Vec::new());
    }
    let mut expr = parser.parse_expression(0)?;
    if parser.pos != parser.tokens.len() {
        return Err(FormulaError::Value);
    }
    if parser.saw_scope {
        crate::parser_array::resolve_scopes(&mut expr);
    }
    let specs = parser.ref_specs.take().unwrap_or_default();
    Ok((
        Parsed {
            expr,
            saw_name: parser.saw_name,
            table_keys: parser.table_keys,
        },
        specs,
    ))
}

/// Parses a formula body with no tables around it. The flag says whether it uses defined names.
pub(crate) fn parse_formula(expr_src: &str) -> Result<(Expr, bool), FormulaError> {
    parse_formula_in(expr_src, None).map(|p| (p.expr, p.saw_name))
}

/// Parses a formula body with no workbook around it: a defined name reads `#NAME?`.
pub fn compile_formula_expr(expr_src: &str) -> Result<Expr, FormulaError> {
    let (mut expr, saw_name) = parse_formula(expr_src)?;
    if saw_name {
        crate::names::walk_mut(&mut expr, |node, _| {
            if matches!(node, Expr::Name { .. }) {
                *node = Expr::ErrorLiteral(FormulaError::Name);
            }
            true
        });
    }
    Ok(expr)
}

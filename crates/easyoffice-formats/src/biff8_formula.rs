//! BIFF8 formula tokens (`rgce`, MS-XLS 2.5.198) for the `.xls` writer and reader (PD-178).
//!
//! A FORMULA record holds the expression as a postfix token string, not as text. This module
//! converts between the text of a formula and the tokens of a small set every Excel reads:
//! numbers, strings, booleans, cell and range references (`$` flags kept), the operators
//! `+ - * / ^ & = <> < <= > >=`, unary `+`/`-`, parentheses, and a table of common functions.
//! Anything else (sheet references, names, percent, error literals, other functions) is refused
//! by [`encode_rgce`] and left to the cached value by [`decode_rgce`].
//!
//! Token codes and the function table follow MS-XLS 2.5.198 (Ptg) and 2.5.198.17 (Ftab). There is
//! no reference code to port: the tokenizer is a precedence-climbing parser written for this table
//! (spreadsheet writers that cover this much of BIFF8 are GPL-licensed or tied to a runtime).

use crate::{
    EasyOfficeError, XlsxCachedValue, MAX_BIFF8_COL, MAX_BIFF8_RECORD_DATA_LEN, MAX_BIFF8_ROW,
};

const PTG_ADD: u8 = 0x03;
const PTG_UPLUS: u8 = 0x12;
const PTG_UMINUS: u8 = 0x13;
const PTG_PAREN: u8 = 0x15;
const PTG_STR: u8 = 0x17;
const PTG_BOOL: u8 = 0x1D;
const PTG_INT: u8 = 0x1E;
const PTG_NUM: u8 = 0x1F;
const PTG_FUNC: u8 = 0x41;
const PTG_FUNC_VAR: u8 = 0x42;
const PTG_REF_VALUE: u8 = 0x44;
const PTG_REF_REFERENCE: u8 = 0x24;
const PTG_AREA_VALUE: u8 = 0x45;
const PTG_AREA_REFERENCE: u8 = 0x25;
/// Column field flags of a reference: relative column, relative row.
const COL_RELATIVE: u16 = 0x4000;
const ROW_RELATIVE: u16 = 0x8000;
const COL_MASK: u16 = 0x3FFF;
/// A string token holds up to 255 UTF-16 units (`cch` is one byte).
const MAX_STR_UNITS: usize = 255;
/// Nesting of parentheses and calls the parser follows (it recurses).
const MAX_DEPTH: usize = 64;

/// Binary operators from the lowest precedence to the highest: each row is one level, tokens are
/// `(text, ptg)`; longer texts come first so `<=` is not read as `<`.
const BINARY_LEVELS: [&[(&str, u8)]; 4] = [
    &[
        ("<=", 0x0A),
        (">=", 0x0C),
        ("<>", 0x0E),
        ("<", 0x09),
        (">", 0x0D),
        ("=", 0x0B),
    ],
    &[("&", 0x08)],
    &[("+", PTG_ADD), ("-", 0x04)],
    &[("*", 0x05), ("/", 0x06)],
];
const POWER: (&str, u8) = ("^", 0x07);

/// How a function is called: `Fixed` ones take exactly `n` arguments (`ptgFunc`), `Var` ones
/// `min..=max` (`ptgFuncVar`).
#[derive(Clone, Copy)]
enum Arity {
    Fixed(u8),
    Var(u8, u8),
}

/// `(name, Ftab index, arity, arguments are references)`. Reference arguments (`SUM(A1:B2)`) use
/// the reference class of the operand token, value arguments the value class.
const FUNCTIONS: [(&str, u16, Arity, bool); 28] = [
    ("COUNT", 0, Arity::Var(1, 30), true),
    ("IF", 1, Arity::Var(2, 3), false),
    ("ISNA", 2, Arity::Fixed(1), false),
    ("ISERROR", 3, Arity::Fixed(1), false),
    ("SUM", 4, Arity::Var(1, 30), true),
    ("AVERAGE", 5, Arity::Var(1, 30), true),
    ("MIN", 6, Arity::Var(1, 30), true),
    ("MAX", 7, Arity::Var(1, 30), true),
    ("PI", 19, Arity::Fixed(0), false),
    ("SQRT", 20, Arity::Fixed(1), false),
    ("EXP", 21, Arity::Fixed(1), false),
    ("LN", 22, Arity::Fixed(1), false),
    ("LOG10", 23, Arity::Fixed(1), false),
    ("ABS", 24, Arity::Fixed(1), false),
    ("INT", 25, Arity::Fixed(1), false),
    ("SIGN", 26, Arity::Fixed(1), false),
    ("ROUND", 27, Arity::Fixed(2), false),
    ("MID", 31, Arity::Fixed(3), false),
    ("LEN", 32, Arity::Fixed(1), false),
    ("VALUE", 33, Arity::Fixed(1), false),
    ("AND", 36, Arity::Var(1, 30), false),
    ("OR", 37, Arity::Var(1, 30), false),
    ("NOT", 38, Arity::Fixed(1), false),
    ("MOD", 39, Arity::Fixed(2), false),
    ("LEFT", 115, Arity::Var(1, 2), false),
    ("RIGHT", 116, Arity::Var(1, 2), false),
    ("COUNTA", 169, Arity::Var(1, 30), true),
    ("CONCATENATE", 336, Arity::Var(1, 30), false),
];

const BIFF8_FORMULA: u16 = 0x0006;
const BIFF8_STRING: u16 = 0x0207;
/// Bytes of a FORMULA record before the tokens (MS-XLS 2.4.127): cell, format index, cached result
/// (8), flags (2), chain (4) and the token count (2).
pub(crate) const FORMULA_FIXED_LEN: usize = 22;
/// `fAlwaysCalc`: the reader recomputes the cell, so a missing cached result is replaced on load.
const FLAG_ALWAYS_CALC: u16 = 0x0001;
/// Result tag of a cached result that is not a number (the last two bytes are `0xFFFF`).
const RESULT_STRING: u8 = 0;
const RESULT_BOOLEAN: u8 = 1;

/// The FORMULA record of the cell (and the STRING record that carries a text result).
pub(crate) fn formula_records(
    row: u16,
    col: u16,
    rgce: &[u8],
    cached: Option<&XlsxCachedValue>,
) -> Vec<u8> {
    let text = match cached {
        Some(XlsxCachedValue::Text(text)) if string_record_len(text).is_some() => {
            Some(text.as_str())
        }
        _ => None,
    };
    let result: [u8; 8] = match cached {
        Some(XlsxCachedValue::Number(n)) if n.is_finite() => n.to_le_bytes(),
        Some(XlsxCachedValue::Boolean(b)) => [RESULT_BOOLEAN, 0, u8::from(*b), 0, 0, 0, 0xFF, 0xFF],
        Some(XlsxCachedValue::Text(_)) if text.is_some() => {
            [RESULT_STRING, 0, 0, 0, 0, 0, 0xFF, 0xFF]
        }
        _ => 0.0f64.to_le_bytes(),
    };
    let mut out = Vec::with_capacity(4 + FORMULA_FIXED_LEN + rgce.len());
    out.extend(BIFF8_FORMULA.to_le_bytes());
    out.extend(((FORMULA_FIXED_LEN + rgce.len()) as u16).to_le_bytes());
    out.extend(row.to_le_bytes());
    out.extend(col.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out.extend(result);
    out.extend(FLAG_ALWAYS_CALC.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend((rgce.len() as u16).to_le_bytes());
    out.extend(rgce);
    if let Some(text) = text {
        string_record(&mut out, text);
    }
    out
}

/// Bytes of the STRING record body for `text` (MS-XLS 2.4.269), or `None` when the text does not fit
/// one record; the cell then keeps no cached text and holds the recomputed result on load.
fn string_record_len(text: &str) -> Option<usize> {
    let units = text.encode_utf16().count();
    let wide = text.encode_utf16().any(|u| u > 0xFF);
    let body = 3 + if wide { units * 2 } else { units };
    (body <= MAX_BIFF8_RECORD_DATA_LEN).then_some(body)
}

fn string_record(out: &mut Vec<u8>, text: &str) {
    let units: Vec<u16> = text.encode_utf16().collect();
    let wide = units.iter().any(|u| *u > 0xFF);
    let body = string_record_len(text).unwrap_or(3);
    out.extend(BIFF8_STRING.to_le_bytes());
    out.extend((body as u16).to_le_bytes());
    out.extend((units.len() as u16).to_le_bytes());
    out.push(u8::from(wide));
    if wide {
        out.extend(units.iter().flat_map(|u| u.to_le_bytes()));
    } else {
        out.extend(units.iter().map(|u| *u as u8));
    }
}

/// The tokens of the formula `expr` of cell (`row`, `col`), within one record.
pub(crate) fn checked_rgce(row: u32, col: u32, expr: &str) -> Result<Vec<u8>, EasyOfficeError> {
    let rgce = encode_rgce(expr).map_err(|reason| {
        EasyOfficeError::InvalidArgument(format!(
            "BIFF8 수식으로 쓸 수 없는 식입니다 (행 {}, 열 {}): ={expr} ({reason})",
            row + 1,
            col + 1
        ))
    })?;
    let record_len = FORMULA_FIXED_LEN + rgce.len();
    if record_len > MAX_BIFF8_RECORD_DATA_LEN {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "BIFF8 수식 레코드({record_len}바이트)가 레코드 한도 {MAX_BIFF8_RECORD_DATA_LEN}바이트를 초과했습니다."
        )));
    }
    Ok(rgce)
}

/// The `=expr` text of a FORMULA record's tokens, or `None` for tokens outside the set above (the
/// cached result is then the cell's value).
pub(crate) fn formula_text(rgce: &[u8]) -> Option<String> {
    decode_rgce(rgce).map(|text| format!("={text}"))
}

/// The tokens of `expr` (a formula without its leading `=`).
pub(crate) fn encode_rgce(expr: &str) -> Result<Vec<u8>, String> {
    let mut parser = Parser {
        chars: expr.chars().collect(),
        pos: 0,
        out: Vec::new(),
    };
    parser.expression(0, 0)?;
    parser.skip_spaces();
    if parser.pos != parser.chars.len() {
        return Err(format!("{}번째 글자를 해석하지 못했습니다", parser.pos + 1));
    }
    Ok(parser.out)
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
    out: Vec<u8>,
}

impl Parser {
    fn skip_spaces(&mut self) {
        while self.chars.get(self.pos) == Some(&' ') {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.skip_spaces();
        self.chars.get(self.pos).copied()
    }

    fn eat(&mut self, text: &str) -> bool {
        self.skip_spaces();
        let len = text.chars().count();
        let matches = self.chars.len() >= self.pos + len
            && self.chars[self.pos..self.pos + len]
                .iter()
                .copied()
                .eq(text.chars());
        if matches {
            self.pos += len;
        }
        matches
    }

    /// One precedence level of left-associative binary operators, then the level above it.
    fn expression(&mut self, level: usize, depth: usize) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err("식의 중첩이 너무 깊습니다".to_string());
        }
        let Some(operators) = BINARY_LEVELS.get(level) else {
            return self.power(depth);
        };
        self.expression(level + 1, depth)?;
        'next: loop {
            for &(text, ptg) in *operators {
                if self.eat(text) {
                    self.expression(level + 1, depth)?;
                    self.out.push(ptg);
                    continue 'next;
                }
            }
            return Ok(());
        }
    }

    fn power(&mut self, depth: usize) -> Result<(), String> {
        self.unary(depth)?;
        while self.eat(POWER.0) {
            self.unary(depth)?;
            self.out.push(POWER.1);
        }
        Ok(())
    }

    /// Excel binds a unary sign tighter than `^`, so `-2^2` is `(-2)^2`.
    fn unary(&mut self, depth: usize) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err("식의 중첩이 너무 깊습니다".to_string());
        }
        let sign = if self.eat("-") {
            Some(PTG_UMINUS)
        } else if self.eat("+") {
            Some(PTG_UPLUS)
        } else {
            None
        };
        match sign {
            Some(ptg) => {
                self.unary(depth + 1)?;
                self.out.push(ptg);
                Ok(())
            }
            None => self.primary(depth),
        }
    }

    fn primary(&mut self, depth: usize) -> Result<(), String> {
        match self.peek() {
            Some('(') => {
                self.pos += 1;
                self.expression(0, depth + 1)?;
                if !self.eat(")") {
                    return Err("닫는 괄호가 없습니다".to_string());
                }
                self.out.push(PTG_PAREN);
                Ok(())
            }
            Some('"') => self.string(),
            Some(c) if c.is_ascii_digit() || c == '.' => self.number(),
            Some(c) if c.is_ascii_alphabetic() || c == '$' => self.name_or_reference(depth, false),
            Some(c) => Err(format!("지원하지 않는 글자 '{c}'")),
            None => Err("식이 끝났습니다".to_string()),
        }
    }

    fn string(&mut self) -> Result<(), String> {
        self.pos += 1;
        let mut units: Vec<u16> = Vec::new();
        loop {
            match self.chars.get(self.pos) {
                None => return Err("문자열이 닫히지 않았습니다".to_string()),
                Some('"') if self.chars.get(self.pos + 1) == Some(&'"') => {
                    units.push(u16::from(b'"'));
                    self.pos += 2;
                }
                Some('"') => {
                    self.pos += 1;
                    break;
                }
                Some(c) => {
                    let mut buf = [0u16; 2];
                    units.extend_from_slice(c.encode_utf16(&mut buf));
                    self.pos += 1;
                }
            }
        }
        if units.len() > MAX_STR_UNITS {
            return Err(format!("문자열이 {MAX_STR_UNITS}자를 넘습니다"));
        }
        self.out.push(PTG_STR);
        self.out.push(units.len() as u8);
        if units.iter().all(|u| *u <= 0xFF) {
            self.out.push(0);
            self.out.extend(units.iter().map(|u| *u as u8));
        } else {
            self.out.push(1);
            self.out.extend(units.iter().flat_map(|u| u.to_le_bytes()));
        }
        Ok(())
    }

    fn number(&mut self) -> Result<(), String> {
        let start = self.pos;
        let digits = |p: &Self, at: usize| p.chars.get(at).is_some_and(char::is_ascii_digit);
        while digits(self, self.pos) {
            self.pos += 1;
        }
        if self.chars.get(self.pos) == Some(&'.') {
            self.pos += 1;
            while digits(self, self.pos) {
                self.pos += 1;
            }
        }
        if matches!(self.chars.get(self.pos), Some('e' | 'E')) {
            let mut at = self.pos + 1;
            if matches!(self.chars.get(at), Some('+' | '-')) {
                at += 1;
            }
            if digits(self, at) {
                self.pos = at;
                while digits(self, self.pos) {
                    self.pos += 1;
                }
            }
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        let value: f64 = text
            .parse()
            .map_err(|_| format!("숫자 '{text}'를 읽지 못했습니다"))?;
        if !value.is_finite() {
            return Err(format!("숫자 '{text}'가 범위를 넘습니다"));
        }
        if value.fract() == 0.0 && (0.0..=65_535.0).contains(&value) && !text.contains(['e', 'E']) {
            self.out.push(PTG_INT);
            self.out.extend((value as u16).to_le_bytes());
        } else {
            self.out.push(PTG_NUM);
            self.out.extend(value.to_le_bytes());
        }
        Ok(())
    }

    /// A function call, `TRUE`/`FALSE`, a cell, or a range of two cells. `reference` asks for the
    /// reference class (the argument of `SUM` and the like).
    fn name_or_reference(&mut self, depth: usize, reference: bool) -> Result<(), String> {
        let start = self.pos;
        while self
            .chars
            .get(self.pos)
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '$' || *c == '_' || *c == '.')
        {
            self.pos += 1;
        }
        let word: String = self.chars[start..self.pos].iter().collect();
        if self.chars.get(self.pos) == Some(&'(') {
            return self.call(&word, depth);
        }
        if let Some(value) = bool_literal(&word) {
            self.out.extend([PTG_BOOL, u8::from(value)]);
            return Ok(());
        }
        let first = parse_cell(&word)?;
        if self.chars.get(self.pos) == Some(&':') {
            self.pos += 1;
            let second_start = self.pos;
            while self
                .chars
                .get(self.pos)
                .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '$')
            {
                self.pos += 1;
            }
            let second: String = self.chars[second_start..self.pos].iter().collect();
            let last = parse_cell(&second)?;
            self.out.push(if reference {
                PTG_AREA_REFERENCE
            } else {
                PTG_AREA_VALUE
            });
            self.out.extend(first.row.to_le_bytes());
            self.out.extend(last.row.to_le_bytes());
            self.out.extend(first.col_field().to_le_bytes());
            self.out.extend(last.col_field().to_le_bytes());
        } else {
            self.out.push(if reference {
                PTG_REF_REFERENCE
            } else {
                PTG_REF_VALUE
            });
            self.out.extend(first.row.to_le_bytes());
            self.out.extend(first.col_field().to_le_bytes());
        }
        Ok(())
    }

    fn call(&mut self, name: &str, depth: usize) -> Result<(), String> {
        let upper = name.to_ascii_uppercase();
        let &(_, index, arity, reference_args) = FUNCTIONS
            .iter()
            .find(|f| f.0 == upper)
            .ok_or_else(|| format!("지원하지 않는 함수 {upper}"))?;
        self.pos += 1;
        let mut count = 0usize;
        if self.peek() != Some(')') {
            loop {
                self.argument(reference_args, depth + 1)?;
                count += 1;
                if !self.eat(",") {
                    break;
                }
            }
        }
        if !self.eat(")") {
            return Err("함수의 닫는 괄호가 없습니다".to_string());
        }
        match arity {
            Arity::Fixed(n) if count == usize::from(n) => {
                self.out.push(PTG_FUNC);
                self.out.extend(index.to_le_bytes());
            }
            Arity::Var(min, max) if (usize::from(min)..=usize::from(max)).contains(&count) => {
                self.out.push(PTG_FUNC_VAR);
                self.out.push(count as u8);
                self.out.extend(index.to_le_bytes());
            }
            _ => return Err(format!("{upper}의 인수 개수({count})가 맞지 않습니다")),
        }
        Ok(())
    }

    /// An argument: a bare reference or range takes the function's class, anything else is a
    /// whole expression.
    fn argument(&mut self, reference: bool, depth: usize) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err("식의 중첩이 너무 깊습니다".to_string());
        }
        let rest_is_bare = self.bare_reference_end().is_some();
        if reference && rest_is_bare {
            self.name_or_reference(depth, true)
        } else {
            self.expression(0, depth)
        }
    }

    /// The position after a reference or range that is the whole argument (followed by `,` or
    /// `)`), or `None` when the argument is anything else.
    fn bare_reference_end(&mut self) -> Option<usize> {
        self.skip_spaces();
        let mut at = self.pos;
        let take = |chars: &[char], mut at: usize| {
            while chars
                .get(at)
                .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '$')
            {
                at += 1;
            }
            at
        };
        at = take(&self.chars, at);
        if self.chars.get(at) == Some(&':') {
            at = take(&self.chars, at + 1);
        }
        let word: String = self.chars[self.pos..at].iter().collect();
        let first_word = word.split(':').next().unwrap_or("");
        if at == self.pos || bool_literal(first_word).is_some() || parse_cell(first_word).is_err() {
            return None;
        }
        let mut next = at;
        while self.chars.get(next) == Some(&' ') {
            next += 1;
        }
        matches!(self.chars.get(next), Some(',' | ')')).then_some(at)
    }
}

fn bool_literal(word: &str) -> Option<bool> {
    match word.to_ascii_uppercase().as_str() {
        "TRUE" => Some(true),
        "FALSE" => Some(false),
        _ => None,
    }
}

struct Cell {
    row: u16,
    col: u16,
    row_relative: bool,
    col_relative: bool,
}

impl Cell {
    fn col_field(&self) -> u16 {
        self.col
            | if self.col_relative { COL_RELATIVE } else { 0 }
            | if self.row_relative { ROW_RELATIVE } else { 0 }
    }
}

/// `A1`, `$B$2`: letters then digits, each part with an optional `$`.
fn parse_cell(word: &str) -> Result<Cell, String> {
    let bytes = word.as_bytes();
    let mut at = 0;
    let col_relative = bytes.first() != Some(&b'$');
    if !col_relative {
        at += 1;
    }
    let letters_start = at;
    while bytes.get(at).is_some_and(u8::is_ascii_alphabetic) {
        at += 1;
    }
    let letters = &word[letters_start..at];
    let row_relative = bytes.get(at) != Some(&b'$');
    if !row_relative {
        at += 1;
    }
    let digits = &word[at..];
    if letters.is_empty()
        || letters.len() > 3
        || digits.is_empty()
        || !digits.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(format!(
            "'{word}'는 셀 주소가 아닙니다 (이름·시트 참조는 쓸 수 없습니다)"
        ));
    }
    let col = letters.bytes().fold(0u32, |n, b| {
        n * 26 + u32::from(b.to_ascii_uppercase() - b'A') + 1
    }) - 1;
    let row: u32 = digits
        .parse::<u32>()
        .ok()
        .and_then(|n| n.checked_sub(1))
        .ok_or_else(|| format!("'{word}'의 행 번호가 맞지 않습니다"))?;
    if col > MAX_BIFF8_COL || row > MAX_BIFF8_ROW {
        return Err(format!("'{word}'가 BIFF8 범위(IV65536)를 넘습니다"));
    }
    Ok(Cell {
        row: row as u16,
        col: col as u16,
        row_relative,
        col_relative,
    })
}

/// The text of `rgce` (without `=`), or `None` for any token outside the set above or malformed
/// tokens (the reader then uses the cached value).
pub(crate) fn decode_rgce(rgce: &[u8]) -> Option<String> {
    let mut stack: Vec<String> = Vec::new();
    let mut at = 0usize;
    while at < rgce.len() {
        let ptg = rgce[at];
        at += 1;
        match ptg {
            0x03..=0x0E => {
                let right = stack.pop()?;
                let left = stack.pop()?;
                let op = match ptg {
                    0x03 => "+",
                    0x04 => "-",
                    0x05 => "*",
                    0x06 => "/",
                    0x07 => "^",
                    0x08 => "&",
                    0x09 => "<",
                    0x0A => "<=",
                    0x0B => "=",
                    0x0C => ">=",
                    0x0D => ">",
                    _ => "<>",
                };
                stack.push(format!("{left}{op}{right}"));
            }
            PTG_UPLUS | PTG_UMINUS => {
                let operand = stack.pop()?;
                let sign = if ptg == PTG_UMINUS { "-" } else { "+" };
                stack.push(format!("{sign}{operand}"));
            }
            PTG_PAREN => {
                let inner = stack.pop()?;
                stack.push(format!("({inner})"));
            }
            PTG_STR => {
                let cch = usize::from(*rgce.get(at)?);
                let wide = rgce.get(at + 1)? & 1 != 0;
                at += 2;
                let text = if wide {
                    let raw = rgce.get(at..at + cch * 2)?;
                    at += cch * 2;
                    let units: Vec<u16> = raw
                        .chunks_exact(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .collect();
                    String::from_utf16(&units).ok()?
                } else {
                    let raw = rgce.get(at..at + cch)?;
                    at += cch;
                    raw.iter().map(|b| char::from(*b)).collect()
                };
                stack.push(format!("\"{}\"", text.replace('"', "\"\"")));
            }
            PTG_BOOL => {
                let value = *rgce.get(at)? != 0;
                at += 1;
                stack.push(if value { "TRUE" } else { "FALSE" }.to_string());
            }
            PTG_INT => {
                let value = u16::from_le_bytes(rgce.get(at..at + 2)?.try_into().ok()?);
                at += 2;
                stack.push(value.to_string());
            }
            PTG_NUM => {
                let value = f64::from_le_bytes(rgce.get(at..at + 8)?.try_into().ok()?);
                at += 8;
                value.is_finite().then_some(())?;
                stack.push(value.to_string());
            }
            0x24 | 0x44 | 0x64 => {
                let row = u16::from_le_bytes(rgce.get(at..at + 2)?.try_into().ok()?);
                let col = u16::from_le_bytes(rgce.get(at + 2..at + 4)?.try_into().ok()?);
                at += 4;
                stack.push(cell_text(row, col));
            }
            0x25 | 0x45 | 0x65 => {
                let raw = rgce.get(at..at + 8)?;
                at += 8;
                let word = |i: usize| u16::from_le_bytes([raw[i], raw[i + 1]]);
                stack.push(format!(
                    "{}:{}",
                    cell_text(word(0), word(4)),
                    cell_text(word(2), word(6))
                ));
            }
            PTG_FUNC | 0x21 | 0x61 => {
                let index = u16::from_le_bytes(rgce.get(at..at + 2)?.try_into().ok()?);
                at += 2;
                let &(name, _, arity, _) = FUNCTIONS.iter().find(|f| f.1 == index)?;
                let Arity::Fixed(count) = arity else {
                    return None;
                };
                call_text(&mut stack, name, usize::from(count))?;
            }
            PTG_FUNC_VAR | 0x22 | 0x62 => {
                let count = usize::from(*rgce.get(at)? & 0x7F);
                let index = u16::from_le_bytes(rgce.get(at + 1..at + 3)?.try_into().ok()?) & 0x7FFF;
                at += 3;
                let &(name, _, _, _) = FUNCTIONS.iter().find(|f| f.1 == index)?;
                call_text(&mut stack, name, count)?;
            }
            _ => return None,
        }
    }
    (stack.len() == 1).then(|| stack.remove(0))
}

fn call_text(stack: &mut Vec<String>, name: &str, count: usize) -> Option<()> {
    let at = stack.len().checked_sub(count)?;
    let args = stack.split_off(at).join(",");
    stack.push(format!("{name}({args})"));
    Some(())
}

fn cell_text(row: u16, col_field: u16) -> String {
    let mut col = u32::from(col_field & COL_MASK) + 1;
    let mut letters = Vec::new();
    while col > 0 {
        col -= 1;
        letters.push(b'A' + (col % 26) as u8);
        col /= 26;
    }
    letters.reverse();
    let letters = String::from_utf8_lossy(&letters).into_owned();
    let col_mark = if col_field & COL_RELATIVE == 0 {
        "$"
    } else {
        ""
    };
    let row_mark = if col_field & ROW_RELATIVE == 0 {
        "$"
    } else {
        ""
    };
    format!("{col_mark}{letters}{row_mark}{}", u32::from(row) + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_addition_is_two_references_and_ptg_add() {
        // MS-XLS: ptgRef (value class 0x44), row, col with both relative flags, ptgAdd.
        assert_eq!(
            encode_rgce("A1+B1").unwrap(),
            [0x44, 0, 0, 0, 0xC0, 0x44, 0, 0, 1, 0xC0, 0x03]
        );
    }

    #[test]
    fn sum_of_a_range_uses_the_reference_class_and_ptg_func_var() {
        assert_eq!(
            encode_rgce("SUM(A1:B2)").unwrap(),
            [0x25, 0, 0, 1, 0, 0, 0xC0, 1, 0xC0, 0x42, 1, 4, 0]
        );
    }

    #[test]
    fn absolute_flags_literals_and_text_survive_a_round_trip() {
        for text in [
            "$A$1*2.5",
            "IF(A1>=10,\"큰\"\"값\",\"작은\")",
            "-(B2+3)^2",
            "ROUND(AVERAGE(A1:C3),2)&\"x\"",
            "AND(TRUE,A1<>B1)",
            "70000+1000",
        ] {
            let rgce = encode_rgce(text).unwrap();
            assert_eq!(decode_rgce(&rgce).as_deref(), Some(text), "{text}");
        }
    }

    #[test]
    fn operator_precedence_follows_the_postfix_order() {
        // 1+2*3 = 1 2 3 * +
        assert_eq!(
            encode_rgce("1+2*3").unwrap(),
            [0x1E, 1, 0, 0x1E, 2, 0, 0x1E, 3, 0, 0x05, 0x03]
        );
        // (1+2)*3 = 1 2 + paren 3 *
        assert_eq!(
            encode_rgce("(1+2)*3").unwrap(),
            [0x1E, 1, 0, 0x1E, 2, 0, 0x03, 0x15, 0x1E, 3, 0, 0x05]
        );
    }

    #[test]
    fn what_the_token_set_does_not_cover_is_refused() {
        for text in [
            "Sheet2!A1",
            "NAME",
            "UNKNOWN(1)",
            "A1%",
            "#N/A",
            "SUM(",
            "ROUND(1)",
            "A1 B1",
            "IV65537",
            "XFE1",
            "\u{1F600}",
        ] {
            assert!(encode_rgce(text).is_err(), "{text}");
        }
    }

    #[test]
    fn malformed_tokens_decode_to_none() {
        assert_eq!(decode_rgce(&[]), None);
        assert_eq!(decode_rgce(&[0x44, 0, 0]), None);
        assert_eq!(decode_rgce(&[0x03]), None);
        assert_eq!(decode_rgce(&[0x19, 0x10, 0, 0]), None);
        assert_eq!(decode_rgce(&[0x1E, 1, 0, 0x1E, 2, 0]), None);
    }

    #[test]
    fn deep_nesting_is_refused_not_a_stack_overflow() {
        let deep = format!("{}1{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(encode_rgce(&deep).is_err());
        let unary = format!("{}1", "-".repeat(10_000));
        assert!(encode_rgce(&unary).is_err());
    }
}

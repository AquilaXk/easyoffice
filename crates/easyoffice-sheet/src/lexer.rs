//! Lexing a formula body into tokens (see [`crate::parser`]).

use crate::parser::{Token, MAX_FORMULA_LEN, MAX_TOKENS};
use crate::FormulaError;
use std::borrow::Cow;

/// Token for operators and punctuation that are always a single byte.
#[inline]
fn single_byte_token(b: u8) -> Option<Token<'static>> {
    Some(match b {
        b'+' => Token::Plus,
        b'-' => Token::Minus,
        b'*' => Token::Star,
        b'/' => Token::Slash,
        b'^' => Token::Caret,
        b'%' => Token::Percent,
        b'&' => Token::Ampersand,
        b'(' => Token::LParen,
        b')' => Token::RParen,
        b',' | b';' => Token::Comma,
        b':' => Token::Colon,
        b'!' => Token::Bang,
        b'=' => Token::Eq,
        _ => return None,
    })
}

/// Lexes `<`, `<>`, `<=`, `>` or `>=` at `i`; returns the token and its byte length.
fn lex_comparison(bytes: &[u8], i: usize) -> (Token<'static>, usize) {
    let next = bytes.get(i + 1).copied();
    match (bytes[i], next) {
        (b'<', Some(b'>')) => (Token::Ne, 2),
        (b'<', Some(b'=')) => (Token::Le, 2),
        (b'<', _) => (Token::Lt, 1),
        (_, Some(b'=')) => (Token::Ge, 2),
        _ => (Token::Gt, 1),
    }
}

/// Lexes a quoted run starting at the opening `quote` at `i` (doubled quotes escape);
/// returns the content and the index after the closing quote.
/// If no doubled quotes are present, borrows the slice with zero heap allocations.
fn lex_quoted<'a>(
    src: &'a str,
    i: usize,
    quote: u8,
) -> Result<(Cow<'a, str>, usize), FormulaError> {
    let bytes = src.as_bytes();
    let mut cur = i + 1;
    let mut has_escapes = false;
    let content_start = cur;

    while cur < bytes.len() {
        let Some(rel) = bytes[cur..].iter().position(|&b| b == quote) else {
            return Err(FormulaError::Value);
        };
        cur += rel;
        if bytes.get(cur + 1) == Some(&quote) {
            has_escapes = true;
            cur += 2;
        } else {
            let content_end = cur;
            let next_idx = cur + 1;
            if !has_escapes {
                return Ok((Cow::Borrowed(&src[content_start..content_end]), next_idx));
            } else {
                let mut text = String::with_capacity(content_end - content_start);
                let mut p = content_start;
                while p < content_end {
                    let rel = bytes[p..content_end]
                        .iter()
                        .position(|&b| b == quote)
                        .unwrap_or(content_end - p);
                    if rel > 0 {
                        text.push_str(&src[p..p + rel]);
                        p += rel;
                    }
                    if p < content_end && bytes[p] == quote && bytes.get(p + 1) == Some(&quote) {
                        text.push(quote as char);
                        p += 2;
                    }
                }
                return Ok((Cow::Owned(text), next_idx));
            }
        }
    }
    Err(FormulaError::Value)
}

/// Bytes that terminate a `#ERROR!` literal.
#[inline]
fn is_error_literal_stop(b: u8) -> bool {
    b.is_ascii_whitespace()
        || matches!(
            b,
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
}

/// Lexes a `#...` error literal at `start`; returns the token and the next index.
fn lex_error_literal(src: &str, start: usize) -> Result<(Token<'static>, usize), FormulaError> {
    let bytes = src.as_bytes();
    let rest = &src[start..];
    if rest.len() >= 4
        && rest[..4].eq_ignore_ascii_case("#N/A")
        && !rest
            .as_bytes()
            .get(4)
            .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'!' | b'?'))
    {
        return Ok((Token::ErrorLiteral(FormulaError::Na), start + 4));
    }
    let mut i = start;
    while i < bytes.len() && !is_error_literal_stop(bytes[i]) {
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
        "#SPILL!" => FormulaError::Spill,
        "#CALC!" => FormulaError::Calc,
        "#CONNECT!" => FormulaError::Connect,
        "#BLOCKED!" => FormulaError::Blocked,
        _ => return Err(FormulaError::Value),
    };
    Ok((Token::ErrorLiteral(err), i))
}

/// Advances past consecutive ASCII digits.
#[inline]
fn skip_digits(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    i
}

/// Lexes a numeric literal (digits, dots and an optional exponent) at `start`.
fn lex_number(src: &str, start: usize) -> Result<(Token<'static>, usize), FormulaError> {
    let bytes = src.as_bytes();
    let mut i = start;
    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
        i += 1;
    }
    if matches!(bytes.get(i), Some(b'e' | b'E')) {
        let signed = matches!(bytes.get(i + 1), Some(b'+' | b'-'));
        let digit_at = i + 1 + usize::from(signed);
        if bytes.get(digit_at).is_some_and(u8::is_ascii_digit) {
            i = skip_digits(bytes, digit_at);
        }
    }
    let val = src[start..i]
        .parse::<f64>()
        .map_err(|_| FormulaError::Value)?;
    if !val.is_finite() {
        return Err(FormulaError::Num);
    }
    Ok((Token::Number(val), i))
}

/// Lexes an identifier that starts with an ASCII letter, `$` or `_` without heap allocation.
fn lex_ascii_ident<'a>(src: &'a str, start: usize) -> Result<(Token<'a>, usize), FormulaError> {
    let bytes = src.as_bytes();
    let mut i = start;
    while i < bytes.len() {
        let cur = bytes[i];
        if cur.is_ascii_alphanumeric() || matches!(cur, b'$' | b'_' | b'.') {
            i += 1;
            continue;
        }
        if cur.is_ascii() {
            break;
        }
        let ch = src[i..].chars().next().ok_or(FormulaError::Value)?;
        if !ch.is_alphanumeric() {
            break;
        }
        i += ch.len_utf8();
    }
    Ok((Token::Ident(Cow::Borrowed(&src[start..i])), i))
}

/// Handles a non-ASCII start byte: skips Unicode whitespace or lexes a Unicode identifier without heap allocation.
fn lex_other<'a>(
    src: &'a str,
    start: usize,
    tokens: &mut Vec<Token<'a>>,
) -> Result<usize, FormulaError> {
    let ch = src[start..].chars().next().ok_or(FormulaError::Value)?;
    if ch.is_whitespace() {
        return Ok(start + ch.len_utf8());
    }
    if !ch.is_alphabetic() {
        return Err(FormulaError::Value);
    }
    let mut i = start;
    while i < src.len() {
        let c = src[i..].chars().next().ok_or(FormulaError::Value)?;
        if !(c.is_alphanumeric() || c == '$' || c == '_') {
            break;
        }
        i += c.len_utf8();
    }
    tokens.push(Token::Ident(Cow::Borrowed(&src[start..i])));
    Ok(i)
}

/// Lexes the token starting at non-whitespace byte `i`; returns the next index.
fn lex_token<'a>(
    src: &'a str,
    i: usize,
    tokens: &mut Vec<Token<'a>>,
) -> Result<usize, FormulaError> {
    let bytes = src.as_bytes();
    let b = bytes[i];
    if let Some(tok) = single_byte_token(b) {
        tokens.push(tok);
        return Ok(i + 1);
    }
    let (tok, next) = match b {
        b'<' | b'>' => {
            let (tok, len) = lex_comparison(bytes, i);
            (tok, i + len)
        }
        b'"' => {
            let (text, next) = lex_quoted(src, i, b'"')?;
            (Token::Text(text), next)
        }
        b'\'' => {
            let (ident, next) = lex_quoted(src, i, b'\'')?;
            (Token::Ident(ident), next)
        }
        b'#' => lex_error_literal(src, i)?,
        b'0'..=b'9' | b'.' => lex_number(src, i)?,
        b'A'..=b'Z' | b'a'..=b'z' | b'$' | b'_' => lex_ascii_ident(src, i)?,
        _ => return lex_other(src, i, tokens),
    };
    tokens.push(tok);
    Ok(next)
}

pub(crate) fn tokenize_formula<'a>(src: &'a str) -> Result<Vec<Token<'a>>, FormulaError> {
    if src.len() > MAX_FORMULA_LEN {
        return Err(FormulaError::Value);
    }
    let bytes = src.as_bytes();
    let mut tokens = Vec::with_capacity((bytes.len() / 2).clamp(4, MAX_TOKENS));
    let mut i = 0usize;
    let mut braces = 0usize;

    while i < bytes.len() {
        if tokens.len() >= MAX_TOKENS {
            return Err(FormulaError::Value);
        }
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let b = bytes[i];
        if b == b'[' {
            // The group of a structured reference (the ODF references were rewritten before).
            let end = easyoffice_formats::group_end(src, i).ok_or(FormulaError::Value)?;
            tokens.push(Token::Structured(Cow::Borrowed(&src[i..end])));
            i = end;
            continue;
        }
        let special = match b {
            b'{' => {
                braces += 1;
                Some(Token::LBrace)
            }
            b'}' if braces > 0 => {
                braces -= 1;
                Some(Token::RBrace)
            }
            b';' if braces > 0 => Some(Token::Semicolon),
            // `A1#`: the operator sits right after the reference; anywhere else `#` starts an error.
            b'#' if matches!(tokens.last(), Some(Token::Ident(_)))
                && i > 0
                && !bytes[i - 1].is_ascii_whitespace()
                && !bytes.get(i + 1).is_some_and(|c| c.is_ascii_alphabetic()) =>
            {
                Some(Token::Hash)
            }
            _ => None,
        };
        if let Some(tok) = special {
            tokens.push(tok);
            i += 1;
            continue;
        }
        i = lex_token(src, i, &mut tokens)?;
    }
    if braces != 0 {
        return Err(FormulaError::Value);
    }

    Ok(tokens)
}

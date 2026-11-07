//! Recursive PDF object value parser (dictionaries, arrays, strings, names, tokens, references).

use super::{PdfObjValue, MAX_PDF_NESTING_DEPTH};
use crate::pdf_stream_parser::{
    decode_pdf_name, is_pdf_delim_or_ws, is_standalone_pdf_kw, skip_pdf_ws_and_comments,
    step_indexed_non_code_token, step_pdf_non_code_token, ParenJumpTable, PdfNonCodeKind,
};

type PdfValueParse = Result<Option<(PdfObjValue, usize)>, ()>;

#[inline]
fn is_dict_close_at(bytes: &[u8], p: usize, limit: usize) -> bool {
    p + 1 < limit && bytes[p] == b'>' && bytes[p + 1] == b'>'
}

/// Parses `<< /Key value ... >>` with `p` just after the opening `<<`.
fn parse_pdf_dict(
    bytes: &[u8],
    paren_table: Option<&ParenJumpTable>,
    mut p: usize,
    limit: usize,
    depth: usize,
) -> PdfValueParse {
    let mut entries = Vec::new();
    loop {
        p = skip_pdf_ws_and_comments(bytes, p, limit);
        if p >= limit {
            return Err(());
        }
        if is_dict_close_at(bytes, p, limit) {
            return Ok(Some((PdfObjValue::Dict(entries), p + 2)));
        }
        if bytes[p] != b'/' {
            return Err(());
        }
        let k = pdf_regular_run_end(bytes, p + 1, limit);
        let key = decode_pdf_name(&bytes[p + 1..k]);
        let Some((val, next_p)) =
            parse_pdf_value_with_table(bytes, paren_table, k, limit, depth + 1)?
        else {
            return Err(());
        };
        entries.push((key, val));
        p = next_p;
    }
}

/// Parses `[ item ... ]` with `p` just after the opening `[`.
fn parse_pdf_array(
    bytes: &[u8],
    paren_table: Option<&ParenJumpTable>,
    mut p: usize,
    limit: usize,
    depth: usize,
) -> PdfValueParse {
    let mut items = Vec::new();
    loop {
        p = skip_pdf_ws_and_comments(bytes, p, limit);
        if p >= limit {
            return Err(());
        }
        if bytes[p] == b']' {
            return Ok(Some((PdfObjValue::Array(items), p + 1)));
        }
        let Some((val, next_p)) =
            parse_pdf_value_with_table(bytes, paren_table, p, limit, depth + 1)?
        else {
            return Err(());
        };
        items.push(val);
        p = next_p;
    }
}

/// End of the run of regular (non-delimiter, non-whitespace) bytes starting at `from`.
#[inline]
fn pdf_regular_run_end(bytes: &[u8], from: usize, limit: usize) -> usize {
    let mut k = from;
    while k < limit && !is_pdf_delim_or_ws(bytes[k]) {
        k += 1;
    }
    k
}

/// Parses a literal `( ... )` or hex `< ... >` string at `p`.
fn parse_pdf_string(
    bytes: &[u8],
    paren_table: Option<&ParenJumpTable>,
    p: usize,
    limit: usize,
) -> PdfValueParse {
    let step = match paren_table {
        Some(table) => step_indexed_non_code_token(bytes, table, p, limit)?.ok_or(())?,
        None => step_pdf_non_code_token(&bytes[..limit], p)?.ok_or(())?,
    };
    match step.kind {
        PdfNonCodeKind::ParenString {
            inner_start,
            inner_end,
        } => Ok(Some((
            PdfObjValue::ParenStr(bytes[inner_start..inner_end].to_vec()),
            step.next_index,
        ))),
        PdfNonCodeKind::HexString {
            inner_start,
            inner_end,
        } => Ok(Some((
            PdfObjValue::HexStr(bytes[inner_start..inner_end].to_vec()),
            step.next_index,
        ))),
        _ => Err(()),
    }
}

/// Tries to read `<num> <gen> R` after the object number `tok_str` ending at `k`.
fn parse_pdf_reference(
    bytes: &[u8],
    tok_str: &str,
    k: usize,
    limit: usize,
) -> Result<Option<(PdfObjValue, usize)>, ()> {
    let q1 = skip_pdf_ws_and_comments(bytes, k, limit);
    if q1 <= k || q1 >= limit || !bytes[q1].is_ascii_digit() {
        return Ok(None);
    }
    let mut k2 = q1;
    while k2 < limit && bytes[k2].is_ascii_digit() {
        k2 += 1;
    }
    if k2 != limit && !is_pdf_delim_or_ws(bytes[k2]) {
        return Ok(None);
    }
    let q2 = skip_pdf_ws_and_comments(bytes, k2, limit);
    if q2 <= k2 || !is_standalone_pdf_kw(&bytes[..limit], q2, b"R") {
        return Ok(None);
    }
    let gen_str = std::str::from_utf8(&bytes[q1..k2]).map_err(|_| ())?;
    Ok(match (tok_str.parse::<u32>(), gen_str.parse::<u16>()) {
        (Ok(on), Ok(gn)) => Some((PdfObjValue::Ref(on, gn), q2 + 1)),
        _ => None,
    })
}

/// Parses a bare token: keyword, reference, integer or real number.
fn parse_pdf_token(bytes: &[u8], p: usize, limit: usize, depth: usize) -> PdfValueParse {
    let k = pdf_regular_run_end(bytes, p, limit);
    let tok = &bytes[p..k];
    match tok {
        b"null" => return Ok(Some((PdfObjValue::Null, k))),
        b"true" | b"false" => return Ok(Some((PdfObjValue::Scalar, k))),
        b"endobj" if depth == 0 => return Ok(None),
        b"endobj" | b"stream" | b"endstream" | b"xref" | b"trailer" | b"startxref" => {
            return Err(())
        }
        _ => {}
    }
    let tok_str = std::str::from_utf8(tok).map_err(|_| ())?;
    if tok.iter().all(u8::is_ascii_digit) {
        if let Some(reference) = parse_pdf_reference(bytes, tok_str, k, limit)? {
            return Ok(Some(reference));
        }
    }
    if let Ok(n) = tok_str.parse::<i64>() {
        return Ok(Some((PdfObjValue::Int(n), k)));
    }
    let looks_numeric = tok
        .iter()
        .all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.'))
        && tok.iter().any(u8::is_ascii_digit);
    if looks_numeric && tok_str.parse::<f64>().is_ok_and(f64::is_finite) {
        return Ok(Some((PdfObjValue::Scalar, k)));
    }
    Err(())
}

pub(in crate::text_pdf_codecs) fn parse_pdf_value_with_table(
    bytes: &[u8],
    paren_table: Option<&ParenJumpTable>,
    pos: usize,
    limit: usize,
    depth: usize,
) -> PdfValueParse {
    if depth > MAX_PDF_NESTING_DEPTH {
        return Err(());
    }
    let p = skip_pdf_ws_and_comments(bytes, pos, limit);
    if p >= limit {
        return Ok(None);
    }
    if p + 1 < limit && bytes[p] == b'<' && bytes[p + 1] == b'<' {
        return parse_pdf_dict(bytes, paren_table, p + 2, limit, depth);
    }
    if is_dict_close_at(bytes, p, limit) || bytes[p] == b']' {
        return Err(());
    }
    match bytes[p] {
        b'[' => parse_pdf_array(bytes, paren_table, p + 1, limit, depth),
        b'(' | b'<' => parse_pdf_string(bytes, paren_table, p, limit),
        b'/' => {
            let k = pdf_regular_run_end(bytes, p + 1, limit);
            Ok(Some((
                PdfObjValue::Name(decode_pdf_name(&bytes[p + 1..k])),
                k,
            )))
        }
        b if is_pdf_delim_or_ws(b) => Err(()),
        _ => parse_pdf_token(bytes, p, limit, depth),
    }
}

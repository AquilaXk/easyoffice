//! Recursive PDF object value parser (dictionaries, arrays, strings, names, tokens, references)
//! and multi-stage PDF stream filter pipeline (`/FlateDecode`/`/Fl`, `/ASCIIHexDecode`/`/AHx`,
//! `/ASCII85Decode`/`/A85`, adapted from `lopdf` MIT / `pypdf` BSD-3-Clause with fail-closed budgets).

use super::{
    apply_flate_decode_parms_at, dict_get, resolve_value, PdfIndirectObject, PdfObjValue,
    MAX_PDF_COLLECTION_LEN, MAX_PDF_NESTING_DEPTH, MAX_PDF_STREAM_DECODE_BYTES,
    MAX_PDF_STRING_BYTES,
};
use crate::pdf_stream_parser::{
    decode_pdf_name, is_pdf_delim_or_ws, is_standalone_pdf_kw, skip_pdf_ws_and_comments,
    step_indexed_non_code_token, step_pdf_non_code_token, ParenJumpTable, PdfNonCodeKind,
};
use crate::EasyOfficeError;
use std::collections::HashMap;

pub(super) const MAX_FILTER_CHAIN_LEN: usize = 4;

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
        if entries.len() >= MAX_PDF_COLLECTION_LEN {
            return Err(());
        }
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
        if items.len() >= MAX_PDF_COLLECTION_LEN {
            return Err(());
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
        } => {
            if inner_end.saturating_sub(inner_start) > MAX_PDF_STRING_BYTES {
                return Err(());
            }
            Ok(Some((
                PdfObjValue::ParenStr(bytes[inner_start..inner_end].to_vec()),
                step.next_index,
            )))
        }
        PdfNonCodeKind::HexString {
            inner_start,
            inner_end,
        } => {
            if inner_end.saturating_sub(inner_start) > MAX_PDF_STRING_BYTES {
                return Err(());
            }
            Ok(Some((
                PdfObjValue::HexStr(bytes[inner_start..inner_end].to_vec()),
                step.next_index,
            )))
        }
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

#[inline]
fn is_pdf_filter_ws(b: u8) -> bool {
    matches!(b, 0x00 | 0x09 | 0x0A | 0x0C | 0x0D | 0x20)
}

fn decode_ascii_hex_stream(input: &[u8], max_out: usize) -> Result<Vec<u8>, EasyOfficeError> {
    let mut out = Vec::with_capacity((input.len() / 2).min(max_out).min(65_536));
    let mut high_nibble: Option<u8> = None;
    for &b in input {
        if is_pdf_filter_ws(b) {
            continue;
        }
        if b == b'>' {
            break;
        }
        let d = match b {
            b'0'..=b'9' => b - b'0',
            b'A'..=b'F' => b - b'A' + 10,
            b'a'..=b'f' => b - b'a' + 10,
            _ => {
                return Err(EasyOfficeError::CorruptedContainer(
                    "PDF ASCIIHexDecode 스트림에 유효하지 않은 16진수 바이트가 있습니다."
                        .to_string(),
                ))
            }
        };
        if let Some(hi) = high_nibble.take() {
            if out.len() >= max_out {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "PDF ASCIIHexDecode 스트림 바이트 예산을 초과했습니다.".to_string(),
                ));
            }
            out.push((hi << 4) | d);
        } else {
            high_nibble = Some(d);
        }
    }
    if let Some(hi) = high_nibble {
        if out.len() >= max_out {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "PDF ASCIIHexDecode 스트림 바이트 예산을 초과했습니다.".to_string(),
            ));
        }
        out.push(hi << 4);
    }
    Ok(out)
}

fn decode_ascii85_stream(input: &[u8], max_out: usize) -> Result<Vec<u8>, EasyOfficeError> {
    let mut out = Vec::with_capacity((input.len() * 4 / 5).min(max_out).min(65_536));
    let mut tuple = [0u8; 5];
    let mut tuple_len = 0usize;
    let mut i = 0usize;
    while i < input.len() {
        let b = input[i];
        i += 1;
        if is_pdf_filter_ws(b) {
            continue;
        }
        if b == b'~' {
            while i < input.len() && is_pdf_filter_ws(input[i]) {
                i += 1;
            }
            if input.get(i) != Some(&b'>') {
                return Err(EasyOfficeError::CorruptedContainer(
                    "PDF ASCII85Decode EOD 마커('~>')가 손상되었습니다.".to_string(),
                ));
            }
            break;
        }
        if b == b'z' {
            if tuple_len != 0 {
                return Err(EasyOfficeError::CorruptedContainer(
                    "PDF ASCII85Decode 5-튜플 내부에 'z'가 위치할 수 없습니다.".to_string(),
                ));
            }
            if out.len().saturating_add(4) > max_out {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "PDF ASCII85Decode 스트림 바이트 예산을 초과했습니다.".to_string(),
                ));
            }
            out.extend_from_slice(&[0, 0, 0, 0]);
        } else if (b'!'..=b'u').contains(&b) {
            tuple[tuple_len] = b - b'!';
            tuple_len += 1;
            if tuple_len == 5 {
                let val = tuple.iter().fold(0u64, |acc, &d| acc * 85 + u64::from(d));
                if val > u64::from(u32::MAX) {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "PDF ASCII85Decode 5-튜플 값이 32비트 상한을 초과했습니다.".to_string(),
                    ));
                }
                if out.len().saturating_add(4) > max_out {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "PDF ASCII85Decode 스트림 바이트 예산을 초과했습니다.".to_string(),
                    ));
                }
                out.extend_from_slice(&(val as u32).to_be_bytes());
                tuple_len = 0;
            }
        } else {
            return Err(EasyOfficeError::CorruptedContainer(
                "PDF ASCII85Decode 스트림에 유효하지 않은 바이트가 있습니다.".to_string(),
            ));
        }
    }
    if tuple_len == 1 {
        return Err(EasyOfficeError::CorruptedContainer(
            "PDF ASCII85Decode 마지막 튜플 바이트 수가 1일 수 없습니다.".to_string(),
        ));
    }
    if tuple_len > 1 {
        for slot in &mut tuple[tuple_len..5] {
            *slot = b'u' - b'!';
        }
        let val = tuple.iter().fold(0u64, |acc, &d| acc * 85 + u64::from(d));
        if val > u64::from(u32::MAX) {
            return Err(EasyOfficeError::CorruptedContainer(
                "PDF ASCII85Decode 마지막 튜플 값이 32비트 상한을 초과했습니다.".to_string(),
            ));
        }
        let emit_len = tuple_len - 1;
        if out.len().saturating_add(emit_len) > max_out {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "PDF ASCII85Decode 스트림 바이트 예산을 초과했습니다.".to_string(),
            ));
        }
        let bytes4 = (val as u32).to_be_bytes();
        out.extend_from_slice(&bytes4[..emit_len]);
    }
    Ok(out)
}

/// Decodes a PDF stream's `/Filter` pipeline (up to [`MAX_FILTER_CHAIN_LEN`] stages).
/// Returns `Ok(None)` when the stream has no `/Filter`.
pub(in crate::text_pdf_codecs) fn decode_pdf_stream_filters(
    raw_stream: &[u8],
    entries: &[(String, PdfObjValue)],
    objects: &HashMap<(u32, u16), PdfIndirectObject>,
    remaining_budget: &mut usize,
) -> Result<Option<Vec<u8>>, EasyOfficeError> {
    let Some(raw_filter) = dict_get(entries, "/Filter") else {
        return Ok(None);
    };
    let Some(filter_val) = resolve_value(raw_filter, objects) else {
        return Err(EasyOfficeError::CorruptedContainer(
            "PDF 스트림 /Filter 간접 참조를 해석할 수 없습니다.".to_string(),
        ));
    };
    let filters: Vec<&str> = match filter_val {
        PdfObjValue::Null => return Ok(None),
        PdfObjValue::Name(n) => vec![n.as_str()],
        PdfObjValue::Array(arr) if arr.is_empty() => return Ok(None),
        PdfObjValue::Array(arr) => {
            if arr.len() > MAX_FILTER_CHAIN_LEN {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "PDF 스트림 /Filter 체인 길이가 상한(4)을 초과했습니다.".to_string(),
                ));
            }
            let mut names = Vec::with_capacity(arr.len());
            for item in arr {
                match resolve_value(item, objects) {
                    Some(PdfObjValue::Name(n)) => names.push(n.as_str()),
                    None => {
                        return Err(EasyOfficeError::CorruptedContainer(
                            "PDF 스트림 /Filter 배열 원소 간접 참조를 해석할 수 없습니다."
                                .to_string(),
                        ))
                    }
                    _ => {
                        return Err(EasyOfficeError::CorruptedContainer(
                            "지원하지 않는 PDF 스트림 필터 배열입니다.".to_string(),
                        ))
                    }
                }
            }
            names
        }
        _ => {
            return Err(EasyOfficeError::CorruptedContainer(
                "지원하지 않는 PDF 스트림 필터입니다.".to_string(),
            ))
        }
    };
    let mut current: Option<Vec<u8>> = None;
    for (stage_idx, &filter_name) in filters.iter().enumerate() {
        let cap = MAX_PDF_STREAM_DECODE_BYTES.min(*remaining_budget);
        let input: &[u8] = current.as_deref().unwrap_or(raw_stream);
        match filter_name {
            "/FlateDecode" | "/Fl" => {
                let inflated = match crate::deflate::inflate_auto(input, cap) {
                    Ok(d) => d,
                    Err(e @ EasyOfficeError::ResourceBudgetExceeded(_)) => return Err(e),
                    Err(_) => {
                        return Err(EasyOfficeError::CorruptedContainer(
                            "PDF FlateDecode 스트림 압축 해제에 실패했습니다.".to_string(),
                        ))
                    }
                };
                *remaining_budget = remaining_budget.saturating_sub(inflated.len());
                let decoded = apply_flate_decode_parms_at(
                    inflated,
                    entries,
                    stage_idx,
                    filters.len(),
                    objects,
                )
                .map_err(|detail| EasyOfficeError::CorruptedContainer(detail.to_string()))?;
                current = Some(decoded);
            }
            "/ASCIIHexDecode" | "/AHx" => {
                let decoded = decode_ascii_hex_stream(input, cap)?;
                *remaining_budget = remaining_budget.saturating_sub(decoded.len());
                current = Some(decoded);
            }
            "/ASCII85Decode" | "/A85" => {
                let decoded = decode_ascii85_stream(input, cap)?;
                *remaining_budget = remaining_budget.saturating_sub(decoded.len());
                current = Some(decoded);
            }
            _ => {
                return Err(EasyOfficeError::CorruptedContainer(
                    "지원하지 않는 PDF 스트림 필터입니다.".to_string(),
                ))
            }
        }
    }
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_hex_ascii85_and_chained_filter_pipeline() {
        let objects = HashMap::new();
        let mut budget = 1024 * 1024;
        let ahx_entries = vec![(
            "/Filter".to_string(),
            PdfObjValue::Name("/ASCIIHexDecode".to_string()),
        )];
        let decoded_hex =
            decode_pdf_stream_filters(b"48 65 6C 6C 6F 21>", &ahx_entries, &objects, &mut budget)
                .unwrap()
                .unwrap();
        assert_eq!(decoded_hex, b"Hello!");

        let a85_entries = vec![(
            "/Filter".to_string(),
            PdfObjValue::Name("/ASCII85Decode".to_string()),
        )];
        let decoded_a85 =
            decode_pdf_stream_filters(b"87cURD]i,\"Ebo80~>", &a85_entries, &objects, &mut budget)
                .unwrap()
                .unwrap();
        assert_eq!(decoded_a85, b"Hello World!");

        // 'z' zero-group expansion
        let decoded_z = decode_pdf_stream_filters(b"z~>", &a85_entries, &objects, &mut budget)
            .unwrap()
            .unwrap();
        assert_eq!(decoded_z, &[0, 0, 0, 0]);

        // Multi-stage [/ASCIIHexDecode /FlateDecode] pipeline
        let compressed = crate::deflate::deflate_zlib(b"BT (FilterChainOK) Tj ET");
        let hex_encoded: String = compressed.iter().map(|b| format!("{b:02X}")).collect();
        let chain_entries = vec![(
            "/Filter".to_string(),
            PdfObjValue::Array(vec![
                PdfObjValue::Name("/AHx".to_string()),
                PdfObjValue::Name("/Fl".to_string()),
            ]),
        )];
        let chained = decode_pdf_stream_filters(
            hex_encoded.as_bytes(),
            &chain_entries,
            &objects,
            &mut budget,
        )
        .unwrap()
        .unwrap();
        assert_eq!(chained, b"BT (FilterChainOK) Tj ET");

        // Multi-stage [/ASCII85Decode /FlateDecode] pipeline
        let mut a85_buf = Vec::new();
        for chunk in compressed.chunks(4) {
            let mut block = [0u8; 4];
            block[..chunk.len()].copy_from_slice(chunk);
            let mut val = u32::from_be_bytes(block) as u64;
            let mut digits = [0u8; 5];
            for d in digits.iter_mut().rev() {
                *d = (val % 85) as u8 + b'!';
                val /= 85;
            }
            a85_buf.extend_from_slice(&digits[..chunk.len() + 1]);
        }
        a85_buf.extend_from_slice(b"~>");
        let a85_flate_entries = vec![(
            "/Filter".to_string(),
            PdfObjValue::Array(vec![
                PdfObjValue::Name("/ASCII85Decode".to_string()),
                PdfObjValue::Name("/FlateDecode".to_string()),
            ]),
        )];
        let chained_a85 =
            decode_pdf_stream_filters(&a85_buf, &a85_flate_entries, &objects, &mut budget)
                .unwrap()
                .unwrap();
        assert_eq!(chained_a85, b"BT (FilterChainOK) Tj ET");

        // Reject filter chains longer than MAX_FILTER_CHAIN_LEN (4)
        let too_long_entries = vec![(
            "/Filter".to_string(),
            PdfObjValue::Array(vec![
                PdfObjValue::Name("/AHx".to_string()),
                PdfObjValue::Name("/AHx".to_string()),
                PdfObjValue::Name("/AHx".to_string()),
                PdfObjValue::Name("/AHx".to_string()),
                PdfObjValue::Name("/AHx".to_string()),
            ]),
        )];
        assert!(matches!(
            decode_pdf_stream_filters(b"30>", &too_long_entries, &objects, &mut budget),
            Err(EasyOfficeError::ResourceBudgetExceeded(_))
        ));
    }

    #[test]
    fn hostile_pdf_collection_len_and_string_bytes_fail_closed() {
        let huge_arr = format!("[{}]", "0 ".repeat(16_385));
        assert!(
            parse_pdf_value_with_table(huge_arr.as_bytes(), None, 0, huge_arr.len(), 0).is_err()
        );

        let huge_dict = format!("<<{}>>", "/A 0 ".repeat(16_385));
        assert!(
            parse_pdf_value_with_table(huge_dict.as_bytes(), None, 0, huge_dict.len(), 0).is_err()
        );

        let mut huge_str = Vec::with_capacity((1024 * 1024) + 3);
        huge_str.push(b'(');
        huge_str.resize((1024 * 1024) + 2, b'A');
        huge_str.push(b')');
        assert!(parse_pdf_value_with_table(&huge_str, None, 0, huge_str.len(), 0).is_err());
    }
}

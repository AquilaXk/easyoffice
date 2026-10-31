#[path = "pdf_damaged_encrypt.rs"]
mod pdf_damaged_encrypt;

use crate::pdf_stream_parser::{
    contains_subslice, decode_pdf_hex_string, decode_pdf_name, has_structural_after_endobj,
    is_pdf_delim_or_ws, is_standalone_pdf_kw, parse_indirect_obj_header, skip_pdf_ws_and_comments,
    span_crosses_object_boundary, step_indexed_non_code_token, step_pdf_non_code_token,
    unescape_pdf_text, validate_balanced_pdf_literals, ParenJumpTable, PdfNonCodeKind,
    PdfNonCodeStep,
};
use crate::EasyOfficeError;
pub(super) use pdf_damaged_encrypt::fail_corrupted_or_encrypted;
use std::collections::HashMap;

pub(super) const MAX_PDF_NESTING_DEPTH: usize = 64;
pub(super) const MAX_PDF_STREAM_DECODE_BYTES: usize = 16 * 1024 * 1024;
pub(super) const MAX_PDF_TOTAL_DECODED_BYTES: usize = 32 * 1024 * 1024;
pub(super) const MAX_PDF_PAGE_TREE_DEPTH: usize = 64;
pub(super) const MAX_PDF_CONTENTS_ARRAY_LEN: usize = 256;

#[derive(Debug, Clone)]
pub(super) enum PdfObjValue {
    Null,
    Scalar,
    Int(i64),
    Name(String),
    ParenStr(Vec<u8>),
    HexStr(Vec<u8>),
    Array(Vec<PdfObjValue>),
    Dict(Vec<(String, PdfObjValue)>),
    Ref(u32, u16),
}

pub(super) struct PdfIndirectObject {
    pub(super) val: PdfObjValue,
    pub(super) stream_range: Option<(usize, usize)>,
    pub(super) order_seq: usize,
}

pub(super) enum PageNodeTarget<'a> {
    Ref(u32, u16),
    Inline(&'a [(String, PdfObjValue)]),
}

pub(super) fn dict_get<'a>(
    entries: &'a [(String, PdfObjValue)],
    key: &str,
) -> Option<&'a PdfObjValue> {
    entries.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v)
}

pub(super) fn dict_has_non_null_key(entries: &[(String, PdfObjValue)], key: &str) -> bool {
    dict_get(entries, key).is_some_and(|v| !matches!(v, PdfObjValue::Null))
}

pub(super) fn dict_name_eq(entries: &[(String, PdfObjValue)], key: &str, expected: &str) -> bool {
    matches!(dict_get(entries, key), Some(PdfObjValue::Name(n)) if n == expected)
}

pub(super) fn is_studio_metadata_dict(entries: &[(String, PdfObjValue)]) -> bool {
    dict_name_eq(entries, "/Type", "/Metadata")
        && dict_name_eq(entries, "/Subtype", "/EasyOfficeStudio")
}

pub(super) fn resolve_value<'a>(
    mut val: &'a PdfObjValue,
    objects: &'a HashMap<(u32, u16), PdfIndirectObject>,
) -> Option<&'a PdfObjValue> {
    for _ in 0..MAX_PDF_NESTING_DEPTH {
        match val {
            PdfObjValue::Ref(on, gn) => {
                val = &objects.get(&(*on, *gn))?.val;
            }
            other => return Some(other),
        }
    }
    None
}

pub(super) fn resolve_int(
    val: &PdfObjValue,
    objects: &HashMap<(u32, u16), PdfIndirectObject>,
) -> Option<i64> {
    match resolve_value(val, objects)? {
        PdfObjValue::Int(n) => Some(*n),
        _ => None,
    }
}

pub(super) fn decode_text_value(
    val: &PdfObjValue,
    objects: &HashMap<(u32, u16), PdfIndirectObject>,
) -> Option<String> {
    match resolve_value(val, objects)? {
        PdfObjValue::ParenStr(raw) => Some(unescape_pdf_text(&String::from_utf8_lossy(raw))),
        PdfObjValue::HexStr(raw) => decode_pdf_hex_string(&String::from_utf8_lossy(raw), false),
        _ => None,
    }
}

pub(super) fn resolve_catalog_and_pages_target<'a>(
    objects: &'a HashMap<(u32, u16), PdfIndirectObject>,
    ordered_objs: &[((u32, u16), &'a PdfIndirectObject)],
    trailers: &'a [Vec<(String, PdfObjValue)>],
) -> (String, Option<PageNodeTarget<'a>>) {
    let root_val = trailers
        .iter()
        .rev()
        .find_map(|tr| dict_get(tr, "/Root"))
        .or_else(|| {
            ordered_objs.iter().rev().find_map(|(_, o)| {
                if let PdfObjValue::Dict(ref d) = o.val {
                    if dict_name_eq(d, "/Type", "/XRef") {
                        return dict_get(d, "/Root");
                    }
                }
                None
            })
        });

    let catalog_entries: Option<&'a [(String, PdfObjValue)]> = root_val
        .and_then(|rv| match resolve_value(rv, objects) {
            Some(PdfObjValue::Dict(d)) => Some(d.as_slice()),
            _ => None,
        })
        .or_else(|| {
            ordered_objs.iter().rev().find_map(|(_, o)| {
                if let PdfObjValue::Dict(ref d) = o.val {
                    if dict_name_eq(d, "/Type", "/Catalog") {
                        return Some(d.as_slice());
                    }
                }
                None
            })
        });

    let mut title = String::from("PDF 문서");
    if let Some(cat) = catalog_entries {
        if let Some(t_val) = dict_get(cat, "/Title") {
            if let Some(decoded_t) = decode_text_value(t_val, objects) {
                title = decoded_t;
            }
        }
    }

    let pages_target = catalog_entries
        .and_then(|cat| dict_get(cat, "/Pages"))
        .and_then(|pv| match pv {
            PdfObjValue::Ref(on, gn) => Some(PageNodeTarget::Ref(*on, *gn)),
            PdfObjValue::Dict(d) => Some(PageNodeTarget::Inline(d)),
            _ => None,
        })
        .or_else(|| {
            ordered_objs.iter().find_map(|&((on, gn), o)| {
                if let PdfObjValue::Dict(ref d) = o.val {
                    if dict_name_eq(d, "/Type", "/Pages") && !dict_has_non_null_key(d, "/Parent") {
                        return Some(PageNodeTarget::Ref(on, gn));
                    }
                }
                None
            })
        });

    (title, pages_target)
}

pub(super) fn parse_pdf_value(
    bytes: &[u8],
    pos: usize,
    limit: usize,
    depth: usize,
) -> Result<Option<(PdfObjValue, usize)>, ()> {
    parse_pdf_value_with_table(bytes, None, pos, limit, depth)
}

pub(super) fn parse_pdf_value_with_table(
    bytes: &[u8],
    paren_table: Option<&ParenJumpTable>,
    pos: usize,
    limit: usize,
    depth: usize,
) -> Result<Option<(PdfObjValue, usize)>, ()> {
    if depth > MAX_PDF_NESTING_DEPTH {
        return Err(());
    }
    let mut p = skip_pdf_ws_and_comments(bytes, pos, limit);
    if p >= limit {
        return Ok(None);
    }
    if p + 1 < limit && bytes[p] == b'<' && bytes[p + 1] == b'<' {
        p += 2;
        let mut entries = Vec::new();
        loop {
            p = skip_pdf_ws_and_comments(bytes, p, limit);
            if p >= limit {
                return Err(());
            }
            if p + 1 < limit && bytes[p] == b'>' && bytes[p + 1] == b'>' {
                return Ok(Some((PdfObjValue::Dict(entries), p + 2)));
            }
            if bytes[p] != b'/' {
                return Err(());
            }
            let mut k = p + 1;
            while k < limit && !is_pdf_delim_or_ws(bytes[k]) {
                k += 1;
            }
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
    if (p + 1 < limit && bytes[p] == b'>' && bytes[p + 1] == b'>') || bytes[p] == b']' {
        return Err(());
    }
    if bytes[p] == b'[' {
        p += 1;
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
    if bytes[p] == b'(' || bytes[p] == b'<' {
        let step = if let Some(table) = paren_table {
            step_indexed_non_code_token(bytes, table, p, limit)?.ok_or(())?
        } else {
            step_pdf_non_code_token(&bytes[..limit], p)?.ok_or(())?
        };
        return match step.kind {
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
        };
    }
    if bytes[p] == b'/' {
        let mut k = p + 1;
        while k < limit && !is_pdf_delim_or_ws(bytes[k]) {
            k += 1;
        }
        return Ok(Some((
            PdfObjValue::Name(decode_pdf_name(&bytes[p + 1..k])),
            k,
        )));
    }
    if is_pdf_delim_or_ws(bytes[p]) {
        return Err(());
    }
    let mut k = p;
    while k < limit && !is_pdf_delim_or_ws(bytes[k]) {
        k += 1;
    }
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
    if tok.iter().all(|b| b.is_ascii_digit()) {
        let q1 = skip_pdf_ws_and_comments(bytes, k, limit);
        if q1 > k && q1 < limit && bytes[q1].is_ascii_digit() {
            let mut k2 = q1;
            while k2 < limit && bytes[k2].is_ascii_digit() {
                k2 += 1;
            }
            if k2 == limit || is_pdf_delim_or_ws(bytes[k2]) {
                let q2 = skip_pdf_ws_and_comments(bytes, k2, limit);
                if q2 > k2 && is_standalone_pdf_kw(&bytes[..limit], q2, b"R") {
                    if let (Ok(on), Ok(gn)) = (
                        tok_str.parse::<u32>(),
                        std::str::from_utf8(&bytes[q1..k2])
                            .map_err(|_| ())?
                            .parse::<u16>(),
                    ) {
                        return Ok(Some((PdfObjValue::Ref(on, gn), q2 + 1)));
                    }
                }
            }
        }
    }
    if let Ok(n) = tok_str.parse::<i64>() {
        return Ok(Some((PdfObjValue::Int(n), k)));
    }
    if tok
        .iter()
        .all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.'))
        && tok.iter().any(|b| b.is_ascii_digit())
    {
        if let Ok(f) = tok_str.parse::<f64>() {
            if f.is_finite() {
                return Ok(Some((PdfObjValue::Scalar, k)));
            }
        }
    }
    Err(())
}

fn scan_stream_to_endstream(
    bytes: &[u8],
    start: usize,
    skip_pdf_strings: bool,
) -> Option<(usize, usize)> {
    let kw = b"endstream";
    let mut i = start;
    while i < bytes.len() {
        if skip_pdf_strings {
            match step_pdf_non_code_token(bytes, i) {
                Ok(Some(step)) => {
                    i = step.next_index;
                    continue;
                }
                Ok(None) => {}
                Err(()) => return None,
            }
        }
        if is_standalone_pdf_kw(bytes, i, b"endobj") && has_structural_after_endobj(bytes, i + 6) {
            return None;
        }
        if is_standalone_pdf_kw(bytes, i, kw) {
            let mut trim_end = i;
            if trim_end > start && bytes[trim_end - 1] == b'\n' {
                trim_end -= 1;
                if trim_end > start && bytes[trim_end - 1] == b'\r' {
                    trim_end -= 1;
                }
            } else if trim_end > start && bytes[trim_end - 1] == b'\r' {
                trim_end -= 1;
            }
            if span_crosses_object_boundary(bytes, start, trim_end) {
                return None;
            }
            return Some((trim_end, i + kw.len()));
        }
        i += 1;
    }
    None
}

pub(super) fn verify_direct_stream_bounds(
    bytes: &[u8],
    start: usize,
    len: usize,
) -> Option<(usize, usize, usize)> {
    let end_pos = start.checked_add(len)?;
    if end_pos > bytes.len() {
        return None;
    }
    let mut kw_pos = end_pos;
    if kw_pos < bytes.len() && bytes[kw_pos] == b'\r' {
        kw_pos += 1;
        if kw_pos < bytes.len() && bytes[kw_pos] == b'\n' {
            kw_pos += 1;
        }
    } else if kw_pos < bytes.len() && bytes[kw_pos] == b'\n' {
        kw_pos += 1;
    }
    if !is_standalone_pdf_kw(bytes, kw_pos, b"endstream") {
        return None;
    }
    let after_endstream = kw_pos + 9;
    let after_ws = skip_pdf_ws_and_comments(bytes, after_endstream, bytes.len());
    if !is_standalone_pdf_kw(bytes, after_ws, b"endobj") {
        return None;
    }
    Some((end_pos, after_endstream, after_ws + 6))
}

pub(super) fn resolve_stream_bounds(
    bytes: &[u8],
    start: usize,
    direct_length: Option<usize>,
) -> Option<(usize, usize)> {
    if let Some(len) = direct_length {
        if let Some((end_pos, after_endstream, _)) = verify_direct_stream_bounds(bytes, start, len)
        {
            return Some((end_pos, after_endstream));
        }
    }
    scan_stream_to_endstream(bytes, start, true)
        .or_else(|| scan_stream_to_endstream(bytes, start, false))
}

fn lookup_forward_int_obj(
    bytes: &[u8],
    stream_start: usize,
    target_obj: u32,
    target_gen: u16,
) -> Option<usize> {
    let mut q = stream_start;
    let mut fallback = None;
    while q < bytes.len() {
        if let Some((on, gn, after_hdr)) = parse_indirect_obj_header(bytes, q, bytes.len()) {
            if on == target_obj && gn == target_gen {
                if let Ok(Some((PdfObjValue::Int(n), after_val))) =
                    parse_pdf_value(bytes, after_hdr, bytes.len(), 0)
                {
                    let end_p = skip_pdf_ws_and_comments(bytes, after_val, bytes.len());
                    if is_standalone_pdf_kw(bytes, end_p, b"endobj") {
                        if let Ok(u) = usize::try_from(n) {
                            if verify_direct_stream_bounds(bytes, stream_start, u).is_some() {
                                return Some(u);
                            }
                            fallback.get_or_insert(u);
                        }
                    }
                }
            }
            q = after_hdr;
            continue;
        }
        q += 1;
    }
    fallback
}

pub(super) fn resolve_stream_length_hint(
    bytes: &[u8],
    stream_start: usize,
    entries: &[(String, PdfObjValue)],
    objects: &HashMap<(u32, u16), PdfIndirectObject>,
) -> Option<usize> {
    match dict_get(entries, "/Length")? {
        PdfObjValue::Int(n) => usize::try_from(*n).ok(),
        PdfObjValue::Ref(lo, lg) => {
            if let Some(obj) = objects.get(&(*lo, *lg)) {
                if let PdfObjValue::Int(n) = obj.val {
                    return usize::try_from(n).ok();
                }
            }
            lookup_forward_int_obj(bytes, stream_start, *lo, *lg)
        }
        _ => None,
    }
}

pub(super) fn dict_has_flate_filter(
    entries: &[(String, PdfObjValue)],
    objects: &HashMap<(u32, u16), PdfIndirectObject>,
) -> Result<bool, &'static str> {
    let Some(raw_filter) = dict_get(entries, "/Filter") else {
        return Ok(false);
    };
    let Some(filter_val) = resolve_value(raw_filter, objects) else {
        return Err("PDF 스트림 /Filter 간접 참조를 해석할 수 없습니다.");
    };
    match filter_val {
        PdfObjValue::Null => Ok(false),
        PdfObjValue::Name(n) if n == "/FlateDecode" => Ok(true),
        PdfObjValue::Array(arr) if arr.is_empty() => Ok(false),
        PdfObjValue::Array(arr) if arr.len() == 1 => match resolve_value(&arr[0], objects) {
            Some(PdfObjValue::Name(n)) if n == "/FlateDecode" => Ok(true),
            None => Err("PDF 스트림 /Filter 배열 원소 간접 참조를 해석할 수 없습니다."),
            _ => Err("지원하지 않는 PDF 스트림 필터 배열입니다."),
        },
        _ => Err("지원하지 않는 PDF 스트림 필터입니다."),
    }
}

pub(super) fn decode_and_validate_pdf_streams(
    bytes: &[u8],
    objects: &HashMap<(u32, u16), PdfIndirectObject>,
    ordered_objs: &[((u32, u16), &PdfIndirectObject)],
    remaining_budget: &mut usize,
    trailer_b64: &mut Option<String>,
    trailer_b64_count: &mut usize,
) -> Result<HashMap<(u32, u16), Vec<u8>>, EasyOfficeError> {
    let mut flate_cache: HashMap<(u32, u16), Vec<u8>> = HashMap::new();
    for &(obj_id, obj) in ordered_objs {
        let Some((s_start, s_end)) = obj.stream_range else {
            continue;
        };
        let PdfObjValue::Dict(ref entries) = obj.val else {
            return Err(EasyOfficeError::CorruptedContainer(
                "PDF 스트림 객체 헤더가 딕셔너리가 아닙니다.".to_string(),
            ));
        };
        let raw_stream = &bytes[s_start..s_end];
        let is_flate = dict_has_flate_filter(entries, objects)
            .map_err(|detail| EasyOfficeError::CorruptedContainer(detail.to_string()))?;
        let is_meta = is_studio_metadata_dict(entries);
        if is_flate {
            let cap = MAX_PDF_STREAM_DECODE_BYTES.min(*remaining_budget);
            let decoded = match crate::deflate::inflate_auto(raw_stream, cap) {
                Ok(d) => d,
                Err(e @ EasyOfficeError::ResourceBudgetExceeded(_)) => return Err(e),
                Err(_) => {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "PDF FlateDecode 스트림 압축 해제에 실패했습니다.".to_string(),
                    ))
                }
            };
            *remaining_budget = remaining_budget.saturating_sub(decoded.len());
            if is_meta {
                let meta_text = String::from_utf8_lossy(&decoded);
                for s_line in meta_text.split(['\r', '\n']) {
                    if let Some(rest) = s_line.trim().strip_prefix("%EasyOfficePDFModelBase64:") {
                        *trailer_b64_count += 1;
                        *trailer_b64 = Some(rest.trim().to_string());
                    }
                }
            } else if stream_contains_forbidden_b64_comment(&decoded) {
                return Err(EasyOfficeError::SecurityViolation(
                    "PDF 압축 비메타데이터 스트림 내부에 허용되지 않는 %EasyOfficePDFModelBase64 주석이 발견되었습니다."
                        .to_string(),
                ));
            }
            flate_cache.insert(obj_id, decoded);
        } else if is_meta {
            let cap = MAX_PDF_STREAM_DECODE_BYTES.min(*remaining_budget);
            if raw_stream.len() > cap {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "PDF 메타데이터 스트림 바이트 예산을 초과했습니다.".to_string(),
                ));
            }
            *remaining_budget = remaining_budget.saturating_sub(raw_stream.len());
            let meta_text = String::from_utf8_lossy(raw_stream);
            for s_line in meta_text.split(['\r', '\n']) {
                if let Some(rest) = s_line.trim().strip_prefix("%EasyOfficePDFModelBase64:") {
                    *trailer_b64_count += 1;
                    *trailer_b64 = Some(rest.trim().to_string());
                }
            }
        } else if stream_contains_forbidden_b64_comment(raw_stream) {
            return Err(EasyOfficeError::SecurityViolation(
                "PDF 비메타데이터 스트림 내부에 허용되지 않는 %EasyOfficePDFModelBase64 주석이 발견되었습니다."
                    .to_string(),
            ));
        }
    }
    Ok(flate_cache)
}

fn has_pdf_comment_with_prefix(stream_bytes: &[u8], prefix: &[u8], skip_pdf_strings: bool) -> bool {
    if !skip_pdf_strings {
        return contains_subslice(stream_bytes, prefix);
    }
    let mut i = 0usize;
    while i < stream_bytes.len() {
        match step_pdf_non_code_token(stream_bytes, i) {
            Ok(Some(PdfNonCodeStep {
                next_index,
                kind: PdfNonCodeKind::Comment { start, end },
            })) => {
                let mut cs = start;
                while cs < end && stream_bytes[cs].is_ascii_whitespace() {
                    cs += 1;
                }
                if end - cs >= prefix.len() && &stream_bytes[cs..cs + prefix.len()] == prefix {
                    return true;
                }
                i = next_index;
            }
            Ok(Some(step)) => i = step.next_index,
            Ok(None) | Err(()) => i += 1,
        }
    }
    false
}

pub(super) fn stream_contains_forbidden_b64_comment(stream_bytes: &[u8]) -> bool {
    if validate_balanced_pdf_literals(stream_bytes) {
        has_pdf_comment_with_prefix(stream_bytes, b"%EasyOfficePDFModelBase64:", true)
    } else {
        contains_subslice(stream_bytes, b"%EasyOfficePDFModelBase64:")
    }
}

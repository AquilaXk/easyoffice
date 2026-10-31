#[path = "pdf_damaged_encrypt.rs"]
mod pdf_damaged_encrypt;

use crate::pdf_stream_parser::{
    contains_subslice, decode_pdf_hex_string, decode_pdf_name, has_structural_after_endobj,
    is_pdf_delim_or_ws, is_standalone_pdf_kw, parse_indirect_obj_header, skip_pdf_ws_and_comments,
    skip_pdf_ws_stopping_at_eof, span_crosses_object_boundary, step_indexed_non_code_token,
    step_pdf_non_code_token, unescape_pdf_text, validate_balanced_pdf_literals, ParenJumpTable,
    PdfNonCodeKind, PdfNonCodeStep,
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

pub(super) fn value_contains_encrypt_bytes(val: &PdfObjValue) -> bool {
    match val {
        PdfObjValue::ParenStr(raw) => {
            contains_subslice(raw, b"/Encrypt") || contains_subslice(raw, b"/#45")
        }
        PdfObjValue::Array(items) => items.iter().any(value_contains_encrypt_bytes),
        PdfObjValue::Dict(entries) => entries.iter().any(|(_, v)| value_contains_encrypt_bytes(v)),
        _ => false,
    }
}

pub(super) fn value_contains_xref_and_encrypt(val: &PdfObjValue) -> bool {
    match val {
        PdfObjValue::ParenStr(raw) => {
            (contains_subslice(raw, b"/XRef") || contains_subslice(raw, b"#58Ref"))
                && (contains_subslice(raw, b"/Encrypt") || contains_subslice(raw, b"/#45"))
        }
        PdfObjValue::Array(items) => items.iter().any(value_contains_xref_and_encrypt),
        PdfObjValue::Dict(entries) => entries
            .iter()
            .any(|(_, v)| value_contains_xref_and_encrypt(v)),
        _ => false,
    }
}

pub(super) fn trailer_or_xref_has_spliced_encrypt(
    entries: &[(String, PdfObjValue)],
    has_encrypt: bool,
) -> bool {
    if entries.iter().any(|(_, v)| value_contains_encrypt_bytes(v)) {
        return true;
    }
    let Some(val) = dict_get(entries, "/ID") else {
        return false;
    };
    match val {
        PdfObjValue::Ref(_, _) if !has_encrypt => false,
        PdfObjValue::Array(items) if items.len() == 2 => items
            .iter()
            .any(|item| !matches!(item, PdfObjValue::ParenStr(_) | PdfObjValue::HexStr(_))),
        _ => true,
    }
}

pub(super) fn indirect_dict_has_spliced_encrypt(entries: &[(String, PdfObjValue)]) -> bool {
    if entries
        .iter()
        .any(|(_, v)| value_contains_xref_and_encrypt(v))
    {
        return true;
    }
    if dict_name_eq(entries, "/Type", "/XRef") {
        let has_enc = dict_has_non_null_key(entries, "/Encrypt");
        return trailer_or_xref_has_spliced_encrypt(entries, has_enc);
    }
    false
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

pub(super) fn parse_direct_int_endobj(
    bytes: &[u8],
    after_hdr: usize,
    limit: usize,
) -> Result<(usize, usize), usize> {
    let p = skip_pdf_ws_stopping_at_eof(bytes, after_hdr, limit);
    if p >= limit || parse_indirect_obj_header(bytes, p, limit).is_some() {
        return Err(p.max(after_hdr));
    }
    let mut d = p;
    if matches!(bytes[d], b'+' | b'-') {
        d += 1;
    }
    let digit_start = d;
    while d < limit && bytes[d].is_ascii_digit() {
        d += 1;
    }
    if d == digit_start || (d < limit && !is_pdf_delim_or_ws(bytes[d])) {
        return Err(d.max(after_hdr));
    }
    let end_p = skip_pdf_ws_stopping_at_eof(bytes, d, limit);
    if !is_standalone_pdf_kw(&bytes[..limit], end_p, b"endobj") {
        return Err(end_p.max(after_hdr));
    }
    let Ok(s) = std::str::from_utf8(&bytes[p..d]) else {
        return Err(end_p + 6);
    };
    let Ok(n) = s.parse::<i64>() else {
        return Err(end_p + 6);
    };
    let Ok(u) = usize::try_from(n) else {
        return Err(end_p + 6);
    };
    Ok((u, end_p + 6))
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
        if bytes[q] == b'%' {
            while q < bytes.len() && bytes[q] != b'\n' && bytes[q] != b'\r' {
                q += 1;
            }
            continue;
        }
        if bytes[q].is_ascii_digit() && (q == 0 || is_pdf_delim_or_ws(bytes[q - 1])) {
            if let Some((on, gn, after_hdr)) = parse_indirect_obj_header(bytes, q, bytes.len()) {
                match parse_direct_int_endobj(bytes, after_hdr, bytes.len()) {
                    Ok((u, next_q)) => {
                        if on == target_obj
                            && gn == target_gen
                            && q >= stream_start.saturating_add(u)
                        {
                            if verify_direct_stream_bounds(bytes, stream_start, u).is_some() {
                                return Some(u);
                            }
                            fallback.get_or_insert(u);
                        }
                        q = next_q;
                    }
                    Err(next_q) => {
                        q = next_q.max(after_hdr);
                    }
                }
                continue;
            }
            while q < bytes.len() && bytes[q].is_ascii_digit() {
                q += 1;
            }
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

fn strip_ws(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn wrap_line_64(s: &str, trim_blank: bool) -> Vec<String> {
    let target = if trim_blank { s.trim() } else { s };
    if trim_blank && target.is_empty() {
        return Vec::new();
    }
    let chars: Vec<char> = target.chars().collect();
    if chars.len() <= 64 {
        vec![target.to_string()]
    } else {
        chars.chunks(64).map(|c| c.iter().collect()).collect()
    }
}

fn expand_logical_pages_to_chunks(logical_pages: &[&str], mode: u8) -> Vec<String> {
    const MAX_LINES_PER_PDF_PAGE: usize = 42;
    let mut out = Vec::with_capacity(logical_pages.len());
    for &text in logical_pages {
        let lines: Vec<String> = match mode {
            0 => text.split('\n').map(str::to_string).collect(),
            1 => text
                .split('\n')
                .flat_map(|l| wrap_line_64(l, false))
                .collect(),
            _ => text
                .split('\n')
                .flat_map(|l| wrap_line_64(l, true))
                .collect(),
        };
        if lines.len() <= MAX_LINES_PER_PDF_PAGE {
            out.push(text.to_string());
        } else {
            out.extend(lines.chunks(MAX_LINES_PER_PDF_PAGE).map(|c| c.join("\n")));
        }
    }
    out
}

fn same_ws_stripped(a: &[&str], b: &[&str]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| strip_ws(x) == strip_ws(y))
}

fn matches_pages_1to1(logical_pages: &[&str], page_body_texts: &[&str]) -> bool {
    if same_ws_stripped(logical_pages, page_body_texts) {
        return true;
    }
    [0u8, 1u8, 2u8].into_iter().any(|mode| {
        let chunks = expand_logical_pages_to_chunks(logical_pages, mode);
        let refs: Vec<&str> = chunks.iter().map(String::as_str).collect();
        same_ws_stripped(&refs, page_body_texts)
    })
}

fn extract_validated_page_text(p_entries: &[(String, crate::StrictJsonValue)]) -> Option<&str> {
    let ext_str = match crate::StrictJsonValue::get_field(p_entries, "extractedText") {
        Some(v) => Some(v.as_str()?),
        None => None,
    };
    let body_str = match crate::StrictJsonValue::get_field(p_entries, "bodyText") {
        Some(v) => Some(v.as_str()?),
        None => None,
    };
    if matches!((ext_str, body_str), (Some(a), Some(b)) if a != b) {
        return None;
    }
    ext_str.or(body_str)
}

pub(super) fn validate_pdf_trailer_json(
    decoded: &str,
    expected_title: &str,
    page_body_texts: &[&str],
    parsed_watermark: Option<&str>,
) -> Option<String> {
    let Ok(root) = crate::StrictJsonValue::parse(decoded) else {
        return None;
    };
    let root_entries = root.as_object()?;
    let json_title = crate::StrictJsonValue::get_str(root_entries, "title")?;
    let normalized_json_title = json_title.replace(['\r', '\n'], " ");
    if json_title != expected_title && normalized_json_title != expected_title {
        return None;
    }
    match crate::StrictJsonValue::get_field(root_entries, "watermarkText") {
        Some(crate::StrictJsonValue::Str(wm)) => {
            if wm.is_empty() {
                if parsed_watermark.is_some() {
                    return None;
                }
            } else if parsed_watermark != Some(wm.as_str()) {
                return None;
            }
        }
        Some(crate::StrictJsonValue::Null) | None => {
            if parsed_watermark.is_some() {
                return None;
            }
        }
        Some(_) => return None,
    }
    let root_body_field = crate::StrictJsonValue::get_field(root_entries, "bodyText");
    if let Some(pages_val) = crate::StrictJsonValue::get_field(root_entries, "pages") {
        let page_items = pages_val.as_array()?;
        if page_items.is_empty() || page_items.len() > page_body_texts.len() {
            return None;
        }
        let mut json_page_texts = Vec::with_capacity(page_items.len());
        for item in page_items {
            let p_entries = item.as_object()?;
            let text = extract_validated_page_text(p_entries)?;
            json_page_texts.push(text);
        }
        if let Some(rb) = root_body_field {
            let rb_str = rb.as_str()?;
            if strip_ws(rb_str) != strip_ws(&json_page_texts.join("\n")) {
                return None;
            }
        }
        if matches_pages_1to1(&json_page_texts, page_body_texts) {
            return Some(json_title.to_string());
        }
        return None;
    }
    if let Some(rb) = root_body_field {
        let body_str = rb.as_str()?;
        if matches_pages_1to1(&[body_str], page_body_texts) {
            return Some(json_title.to_string());
        }
    }
    None
}

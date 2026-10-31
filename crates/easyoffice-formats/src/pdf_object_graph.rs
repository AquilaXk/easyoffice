use crate::pdf_stream_parser::{
    advance_past_stream_eol, contains_subslice, decode_pdf_hex_string, decode_pdf_name,
    has_structural_after_endobj, is_pdf_delim_or_ws, is_pdf_ws, is_standalone_pdf_kw,
    match_indirect_obj_header, parse_indirect_obj_header, skip_damaged_truncated_stream,
    skip_pdf_ws_and_comments, span_crosses_object_boundary, step_damaged_non_code,
    step_pdf_non_code_token, unescape_pdf_text, validate_balanced_pdf_literals, DamagedPdfTables,
    PdfNonCodeKind, PdfNonCodeStep,
};
use crate::{DocumentFormat, EasyOfficeError};
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
            let Some((val, next_p)) = parse_pdf_value(bytes, k, limit, depth + 1)? else {
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
            let Some((val, next_p)) = parse_pdf_value(bytes, p, limit, depth + 1)? else {
                return Err(());
            };
            items.push(val);
            p = next_p;
        }
    }
    if bytes[p] == b'(' || bytes[p] == b'<' {
        let step = step_pdf_non_code_token(&bytes[..limit], p)?.ok_or(())?;
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

fn verify_direct_stream_bounds(
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

fn match_damaged_indirect_ref(slice: &[u8], after_first_digits: usize) -> Option<usize> {
    let q1 = skip_pdf_ws_and_comments(slice, after_first_digits, slice.len());
    if q1 == after_first_digits || q1 >= slice.len() || !slice[q1].is_ascii_digit() {
        return None;
    }
    let mut k2 = q1;
    while k2 < slice.len() && slice[k2].is_ascii_digit() {
        k2 += 1;
    }
    if k2 < slice.len() && !is_pdf_delim_or_ws(slice[k2]) {
        return None;
    }
    let q2 = skip_pdf_ws_and_comments(slice, k2, slice.len());
    if q2 == k2 || !is_standalone_pdf_kw(slice, q2, b"R") {
        return None;
    }
    Some(q2 + 1)
}

fn scan_damaged_top_dict(
    bytes: &[u8],
    tables: &DamagedPdfTables,
    start: usize,
    end: usize,
) -> (bool, bool, Option<usize>) {
    let slice = &bytes[..end];
    let mut i = start;
    let mut dict_depth = 0usize;
    let mut array_depth = 0usize;
    let mut expecting_key = false;
    let mut current_key: Option<String> = None;
    let mut is_xref_type = false;
    let mut has_encrypt_key = false;
    let mut direct_length: Option<usize> = None;

    while i < slice.len() {
        let b = slice[i];
        if is_pdf_ws(b) {
            i += 1;
            continue;
        }
        if b == b'%' {
            if let Some(next_i) = step_damaged_non_code(bytes, tables, i, bytes.len()) {
                i = next_i.min(slice.len());
                continue;
            }
        }
        if b == b'<' && i + 1 < slice.len() && slice[i + 1] == b'<' {
            if dict_depth == 0 {
                dict_depth = 1;
                array_depth = 0;
                expecting_key = true;
                current_key = None;
            } else {
                if dict_depth == 1
                    && array_depth == 0
                    && !expecting_key
                    && current_key.as_deref() == Some("/Encrypt")
                {
                    has_encrypt_key = true;
                }
                dict_depth += 1;
            }
            i += 2;
            continue;
        }
        if b == b'>' && i + 1 < slice.len() && slice[i + 1] == b'>' {
            if dict_depth == 2 && array_depth == 0 {
                dict_depth = 1;
                expecting_key = true;
                current_key = None;
            } else {
                dict_depth = dict_depth.saturating_sub(1);
            }
            i += 2;
            continue;
        }
        if dict_depth != 1 {
            if matches!(b, b'(' | b'<') {
                if let Some(next_i) = step_damaged_non_code(bytes, tables, i, bytes.len()) {
                    i = next_i.min(slice.len());
                } else {
                    i += 1;
                }
            } else {
                i += 1;
            }
            continue;
        }
        if b == b'[' {
            if array_depth == 0 && !expecting_key && current_key.as_deref() == Some("/Encrypt") {
                has_encrypt_key = true;
            }
            array_depth += 1;
            i += 1;
            continue;
        }
        if b == b']' {
            if array_depth == 1 {
                array_depth = 0;
                expecting_key = true;
                current_key = None;
            } else {
                array_depth = array_depth.saturating_sub(1);
            }
            i += 1;
            continue;
        }
        if array_depth > 0 {
            if matches!(b, b'(' | b'<') {
                if let Some(next_i) = step_damaged_non_code(bytes, tables, i, bytes.len()) {
                    i = next_i.min(slice.len());
                } else {
                    i += 1;
                }
            } else {
                i += 1;
            }
            continue;
        }
        if expecting_key {
            if b == b'/' {
                let mut k = i + 1;
                while k < slice.len() && !is_pdf_delim_or_ws(slice[k]) {
                    k += 1;
                }
                current_key = Some(decode_pdf_name(&slice[i + 1..k]));
                expecting_key = false;
                i = k;
            } else if matches!(b, b'(' | b'<') {
                if let Some(next_i) = step_damaged_non_code(bytes, tables, i, bytes.len()) {
                    i = next_i.min(slice.len());
                } else {
                    i += 1;
                }
            } else if is_pdf_delim_or_ws(b) {
                i += 1;
            } else {
                while i < slice.len() && !is_pdf_delim_or_ws(slice[i]) {
                    i += 1;
                }
            }
            continue;
        }
        if matches!(b, b'(' | b'<') {
            if current_key.as_deref() == Some("/Encrypt") {
                has_encrypt_key = true;
            }
            if let Some(next_i) = step_damaged_non_code(bytes, tables, i, bytes.len()) {
                i = next_i.min(slice.len());
            } else {
                i += 1;
            }
            expecting_key = true;
            current_key = None;
            continue;
        }
        if b == b'/' {
            let mut k = i + 1;
            while k < slice.len() && !is_pdf_delim_or_ws(slice[k]) {
                k += 1;
            }
            let val_name = decode_pdf_name(&slice[i + 1..k]);
            if current_key.as_deref() == Some("/Type") && val_name == "/XRef" {
                is_xref_type = true;
            }
            if current_key.as_deref() == Some("/Encrypt") {
                has_encrypt_key = true;
            }
            expecting_key = true;
            current_key = None;
            i = k;
            continue;
        }
        if is_pdf_delim_or_ws(b) {
            i += 1;
            continue;
        }
        let mut k = i;
        while k < slice.len() && !is_pdf_delim_or_ws(slice[k]) {
            k += 1;
        }
        let tok = &slice[i..k];
        if tok.iter().all(|c| c.is_ascii_digit()) {
            if let Some(after_ref) = match_damaged_indirect_ref(slice, k) {
                if current_key.as_deref() == Some("/Encrypt") {
                    has_encrypt_key = true;
                }
                expecting_key = true;
                current_key = None;
                i = after_ref;
                continue;
            }
        }
        if current_key.as_deref() == Some("/Encrypt") && tok != b"null" {
            has_encrypt_key = true;
        }
        if current_key.as_deref() == Some("/Length") {
            direct_length = std::str::from_utf8(tok)
                .ok()
                .and_then(|s| s.parse::<usize>().ok());
        }
        expecting_key = true;
        current_key = None;
        i = k;
    }

    (is_xref_type, has_encrypt_key, direct_length)
}

fn try_step_undamaged_object(bytes: &[u8], after_obj: usize) -> Option<(bool, usize)> {
    let (val, next_p) = parse_pdf_value(bytes, after_obj, bytes.len(), 0).ok()??;
    if let PdfObjValue::Dict(ref entries) = val {
        if dict_name_eq(entries, "/Type", "/XRef") && dict_has_non_null_key(entries, "/Encrypt") {
            return Some((true, next_p));
        }
    }
    let kw_p = skip_pdf_ws_and_comments(bytes, next_p, bytes.len());
    if is_standalone_pdf_kw(bytes, kw_p, b"stream") {
        let PdfObjValue::Dict(ref entries) = val else {
            return None;
        };
        let stream_start = advance_past_stream_eol(bytes, kw_p + 6);
        let len_hint = match dict_get(entries, "/Length") {
            Some(PdfObjValue::Int(n)) => usize::try_from(*n).ok(),
            Some(PdfObjValue::Ref(lo, lg)) => lookup_forward_int_obj(bytes, stream_start, *lo, *lg),
            _ => None,
        };
        let after_stream = if let Some((_, after_endstream)) =
            resolve_stream_bounds(bytes, stream_start, len_hint)
        {
            let end_p = skip_pdf_ws_and_comments(bytes, after_endstream, bytes.len());
            if is_standalone_pdf_kw(bytes, end_p, b"endobj") {
                end_p + 6
            } else {
                after_endstream
            }
        } else {
            skip_damaged_truncated_stream(bytes, stream_start)
        };
        return Some((false, after_stream));
    }
    if is_standalone_pdf_kw(bytes, kw_p, b"endobj") {
        return Some((false, kw_p + 6));
    }
    None
}

fn has_encrypt_in_damaged_or_cipher_pdf(bytes: &[u8]) -> bool {
    let tables = DamagedPdfTables::build(bytes);
    let mut p = 0usize;
    while p < bytes.len() {
        if is_pdf_ws(bytes[p]) {
            p += 1;
            continue;
        }
        if bytes[p] == b'%' {
            while p < bytes.len() && bytes[p] != b'\n' && bytes[p] != b'\r' {
                p += 1;
            }
            continue;
        }
        if is_standalone_pdf_kw(bytes, p, b"trailer") {
            let start = p + 7;
            if let Ok(Some((PdfObjValue::Dict(tr_entries), next_p))) =
                parse_pdf_value(bytes, start, bytes.len(), 0)
            {
                if dict_has_non_null_key(&tr_entries, "/Encrypt") {
                    return true;
                }
                p = next_p.max(start);
                continue;
            }
            let cap = (start + 32_768).min(bytes.len());
            let mut end = cap;
            let mut q = start;
            while q < cap {
                if q + 5 <= cap && &bytes[q..q + 5] == b"%%EOF" {
                    end = q;
                    break;
                }
                if let Some(next_q) = step_damaged_non_code(bytes, &tables, q, bytes.len()) {
                    q = next_q;
                    continue;
                }
                if is_standalone_pdf_kw(bytes, q, b"startxref")
                    || match_indirect_obj_header(bytes, q, cap).is_some()
                {
                    end = q;
                    break;
                }
                q += 1;
            }
            let end_clamped = end.min(bytes.len());
            if start < end_clamped {
                let (_, has_enc, _) = scan_damaged_top_dict(bytes, &tables, start, end_clamped);
                if has_enc {
                    return true;
                }
            }
            p = end_clamped.max(start);
            continue;
        }
        if let Some(after_obj) = match_indirect_obj_header(bytes, p, bytes.len()) {
            if let Some((found_enc, next_pos)) = try_step_undamaged_object(bytes, after_obj) {
                if found_enc {
                    return true;
                }
                p = next_pos.max(after_obj);
                continue;
            }
            let cap = (after_obj + 32_768).min(bytes.len());
            let mut end = cap;
            let mut hit_stream = false;
            let mut q = after_obj;
            while q < cap {
                if let Some(next_q) = step_damaged_non_code(bytes, &tables, q, bytes.len()) {
                    q = next_q;
                    continue;
                }
                if is_standalone_pdf_kw(bytes, q, b"stream") {
                    end = q;
                    hit_stream = true;
                    break;
                }
                if is_standalone_pdf_kw(bytes, q, b"endobj")
                    || is_standalone_pdf_kw(bytes, q, b"xref")
                    || is_standalone_pdf_kw(bytes, q, b"trailer")
                {
                    end = q;
                    break;
                }
                q += 1;
            }
            let end_clamped = end.min(bytes.len());
            let (is_xref, has_enc, direct_len) =
                scan_damaged_top_dict(bytes, &tables, after_obj, end_clamped);
            if is_xref && has_enc {
                return true;
            }
            if hit_stream {
                let stream_start = advance_past_stream_eol(bytes, end_clamped + 6);
                if let Some((_, _, after_endobj)) =
                    direct_len.and_then(|len| verify_direct_stream_bounds(bytes, stream_start, len))
                {
                    p = after_endobj;
                } else if let Some((_, after_endstream)) =
                    resolve_stream_bounds(bytes, stream_start, direct_len)
                {
                    p = after_endstream;
                } else {
                    p = skip_damaged_truncated_stream(bytes, stream_start);
                }
            } else {
                p = end_clamped.max(after_obj);
            }
            continue;
        }
        p += 1;
    }
    false
}

pub(super) fn fail_corrupted_or_encrypted(
    bytes: &[u8],
    has_encrypt: bool,
    detail: &str,
) -> EasyOfficeError {
    if has_encrypt || has_encrypt_in_damaged_or_cipher_pdf(bytes) {
        EasyOfficeError::EncryptedDocumentUnsupported(DocumentFormat::Pdf)
    } else {
        EasyOfficeError::CorruptedContainer(detail.to_string())
    }
}

use crate::pdf_stream_parser::{
    advance_past_stream_eol, contains_subslice, decode_pdf_hex_string, decode_pdf_name,
    has_structural_after_endobj, is_pdf_delim_or_ws, is_pdf_ws, is_standalone_pdf_kw,
    match_indirect_obj_header, parse_indirect_obj_header, raw_span_crosses_object_boundary,
    skip_pdf_ws_and_comments, span_crosses_object_boundary, step_pdf_non_code_token,
    unescape_pdf_text, validate_balanced_pdf_literals, PdfNonCodeKind, PdfNonCodeStep,
};
use crate::{DocumentFormat, EasyOfficeError};
use std::borrow::Cow;
use std::collections::HashMap;

pub(super) const MAX_PDF_NESTING_DEPTH: usize = 64;
pub(super) const MAX_PDF_STREAM_DECODE_BYTES: usize = 16 * 1024 * 1024;

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
                    if let PdfNonCodeKind::ParenString {
                        inner_start,
                        inner_end,
                    } = step.kind
                    {
                        if raw_span_crosses_object_boundary(&bytes[inner_start..inner_end]) {
                            return None;
                        }
                    }
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

pub(super) fn resolve_stream_bounds(
    bytes: &[u8],
    start: usize,
    direct_length: Option<usize>,
) -> Option<(usize, usize)> {
    let kw = b"endstream";
    if let Some(len) = direct_length {
        if let Some(end_pos) = start.checked_add(len) {
            if end_pos <= bytes.len() {
                let mut kw_pos = end_pos;
                if kw_pos < bytes.len() && bytes[kw_pos] == b'\r' {
                    kw_pos += 1;
                    if kw_pos < bytes.len() && bytes[kw_pos] == b'\n' {
                        kw_pos += 1;
                    }
                } else if kw_pos < bytes.len() && bytes[kw_pos] == b'\n' {
                    kw_pos += 1;
                }
                if is_standalone_pdf_kw(bytes, kw_pos, kw) {
                    if span_crosses_object_boundary(bytes, start, end_pos) {
                        return None;
                    }
                    return Some((end_pos, kw_pos + kw.len()));
                }
            }
        }
    }
    scan_stream_to_endstream(bytes, start, true)
        .or_else(|| scan_stream_to_endstream(bytes, start, false))
}

fn lookup_forward_int_obj(
    bytes: &[u8],
    from_pos: usize,
    target_obj: u32,
    target_gen: u16,
) -> Option<usize> {
    let mut q = from_pos;
    let mut found = None;
    while q < bytes.len() {
        if let Some((on, gn, after_hdr)) = parse_indirect_obj_header(bytes, q, bytes.len()) {
            if on == target_obj && gn == target_gen {
                if let Ok(Some((PdfObjValue::Int(n), after_val))) =
                    parse_pdf_value(bytes, after_hdr, bytes.len(), 0)
                {
                    let end_p = skip_pdf_ws_and_comments(bytes, after_val, bytes.len());
                    if is_standalone_pdf_kw(bytes, end_p, b"endobj") {
                        if let Ok(u) = usize::try_from(n) {
                            found = Some(u);
                        }
                    }
                }
            }
            q = after_hdr;
            continue;
        }
        q += 1;
    }
    found
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
            _ => Err("지원하지 않는 PDF 스트림 필터 배열입니다."),
        },
        _ => Err("지원하지 않는 PDF 스트림 필터입니다."),
    }
}

pub(super) fn decode_stream_bytes<'a>(
    entries: &[(String, PdfObjValue)],
    raw_stream: &'a [u8],
    objects: &HashMap<(u32, u16), PdfIndirectObject>,
    full_pdf: &[u8],
    has_encrypt: bool,
) -> Result<Cow<'a, [u8]>, EasyOfficeError> {
    match dict_has_flate_filter(entries, objects) {
        Ok(false) => Ok(Cow::Borrowed(raw_stream)),
        Ok(true) => match crate::deflate::inflate_auto(raw_stream, MAX_PDF_STREAM_DECODE_BYTES) {
            Ok(decoded) => Ok(Cow::Owned(decoded)),
            Err(e @ EasyOfficeError::ResourceBudgetExceeded(_)) => Err(e),
            Err(_) => Err(fail_corrupted_or_encrypted(
                full_pdf,
                has_encrypt,
                "PDF FlateDecode 스트림 압축 해제에 실패했습니다.",
            )),
        },
        Err(detail) => Err(fail_corrupted_or_encrypted(full_pdf, has_encrypt, detail)),
    }
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

fn scan_damaged_top_dict(slice: &[u8]) -> (bool, bool, Option<usize>) {
    let mut i = 0usize;
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
            if let Ok(Some(step)) = step_pdf_non_code_token(slice, i) {
                i = step.next_index;
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
                match step_pdf_non_code_token(slice, i) {
                    Ok(Some(step)) => i = step.next_index,
                    _ => i += 1,
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
                match step_pdf_non_code_token(slice, i) {
                    Ok(Some(step)) => i = step.next_index,
                    _ => i += 1,
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
                match step_pdf_non_code_token(slice, i) {
                    Ok(Some(step)) => i = step.next_index,
                    _ => i += 1,
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
            match step_pdf_non_code_token(slice, i) {
                Ok(Some(step)) => i = step.next_index,
                _ => i += 1,
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

fn step_damaged_non_code(bytes: &[u8], pos: usize, limit: usize) -> Option<usize> {
    let slice = &bytes[..limit];
    match step_pdf_non_code_token(slice, pos) {
        Ok(Some(PdfNonCodeStep {
            next_index,
            kind: PdfNonCodeKind::Comment { .. },
        })) => Some(next_index),
        Ok(Some(PdfNonCodeStep {
            next_index,
            kind:
                PdfNonCodeKind::ParenString {
                    inner_start,
                    inner_end,
                }
                | PdfNonCodeKind::HexString {
                    inner_start,
                    inner_end,
                },
        })) => {
            if raw_span_crosses_object_boundary(&slice[inner_start..inner_end]) {
                None
            } else {
                Some(next_index)
            }
        }
        _ => None,
    }
}

fn skip_damaged_truncated_stream(bytes: &[u8], stream_start: usize) -> usize {
    let mut q = stream_start;
    while q < bytes.len() {
        if let Some(next_q) = step_damaged_non_code(bytes, q, bytes.len()) {
            q = next_q;
            continue;
        }
        if is_standalone_pdf_kw(bytes, q, b"endobj") && has_structural_after_endobj(bytes, q + 6) {
            return q + 6;
        }
        q += 1;
    }
    bytes.len()
}

fn has_encrypt_in_damaged_or_cipher_pdf(bytes: &[u8]) -> bool {
    let mut p = 0usize;
    while p < bytes.len() {
        if is_pdf_ws(bytes[p]) {
            p += 1;
            continue;
        }
        if let Some(next_p) = step_damaged_non_code(bytes, p, bytes.len()) {
            p = next_p;
            continue;
        }
        if is_standalone_pdf_kw(bytes, p, b"trailer") {
            let start = p + 7;
            let cap = (start + 32_768).min(bytes.len());
            let mut end = cap;
            let mut q = start;
            while q < cap {
                if q + 5 <= cap && &bytes[q..q + 5] == b"%%EOF" {
                    end = q;
                    break;
                }
                if let Some(next_q) = step_damaged_non_code(bytes, q, cap) {
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
            let (_, has_enc, _) = scan_damaged_top_dict(&bytes[start..end]);
            if has_enc {
                return true;
            }
            p = end.max(start);
            continue;
        }
        if let Some(after_obj) = match_indirect_obj_header(bytes, p, bytes.len()) {
            let cap = (after_obj + 32_768).min(bytes.len());
            let mut end = cap;
            let mut hit_stream = false;
            let mut q = after_obj;
            while q < cap {
                if let Some(next_q) = step_damaged_non_code(bytes, q, cap) {
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
            let (is_xref, has_enc, direct_len) = scan_damaged_top_dict(&bytes[after_obj..end]);
            if is_xref && has_enc {
                return true;
            }
            if hit_stream {
                let stream_start = advance_past_stream_eol(bytes, end + 6);
                if let Some((_, after_endstream)) =
                    resolve_stream_bounds(bytes, stream_start, direct_len)
                {
                    p = after_endstream;
                } else {
                    p = skip_damaged_truncated_stream(bytes, stream_start);
                }
            } else {
                p = end.max(after_obj);
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

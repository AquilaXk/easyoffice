use super::{
    build_eof_and_int_index, dict_get, dict_has_non_null_key, dict_name_eq, lookup_indexed_int_obj,
    parse_pdf_value_with_table, resolve_stream_bounds, verify_direct_stream_bounds, IntObjIndex,
    PdfObjValue,
};
use crate::pdf_stream_parser::{
    advance_past_stream_eol, decode_pdf_name, has_structural_after_endobj_cached,
    is_line_eof_marker, is_pdf_delim_or_ws, is_pdf_ws, is_standalone_pdf_kw,
    match_indirect_obj_header, skip_damaged_truncated_stream, skip_pdf_ws_stopping_at_eof,
    step_indexed_non_code_token, ParenJumpTable, PdfNonCodeKind,
};
use crate::{DocumentFormat, EasyOfficeError};

fn classify_raw_paren_splice(raw: &[u8]) -> (bool, bool) {
    let len = raw.len();
    let mut paren_depth = 0i32;
    let mut dict_depth = 0i32;
    let mut cross_obj = false;
    let mut name_splice = false;
    let mut endobj_cache = (0usize, 0usize);
    let mut i = 0usize;
    while i < len {
        match raw[i] {
            b'\\' => {
                i = (i + 2).min(len);
                continue;
            }
            b'(' => paren_depth += 1,
            b')' => {
                paren_depth -= 1;
                if paren_depth < 0 {
                    return (true, true);
                }
            }
            b'<' if paren_depth == 0 && i + 1 < len && raw[i + 1] == b'<' => {
                dict_depth += 1;
                i += 2;
                continue;
            }
            b'>' if paren_depth == 0 && i + 1 < len && raw[i + 1] == b'>' => {
                dict_depth -= 1;
                if dict_depth < 0 {
                    name_splice = true;
                }
                i += 2;
                continue;
            }
            b'/' if paren_depth == 0
                && dict_depth <= 0
                && (i == 0 || (is_pdf_delim_or_ws(raw[i - 1]) && raw[i - 1] != b'/')) =>
            {
                let mut k = i + 1;
                while k < len && !is_pdf_delim_or_ws(raw[k]) {
                    k += 1;
                }
                if k > i + 1 {
                    let q = skip_pdf_ws_stopping_at_eof(raw, k, len);
                    if q < len && matches!(raw[q], b'(' | b'[') {
                        name_splice = true;
                    }
                }
                i = k;
                continue;
            }
            _ if paren_depth == 0 => {
                if is_standalone_pdf_kw(raw, i, b"endobj") {
                    let mut b = i;
                    while b > 0 && matches!(raw[b - 1], b' ' | b'\t') {
                        b -= 1;
                    }
                    let prec = b > 0
                        && (matches!(raw[b - 1], b'\n' | b'\r' | b']')
                            || (b >= 2 && raw[b - 2] == b'>' && raw[b - 1] == b'>'));
                    let mut a = i + 6;
                    while a < len && matches!(raw[a], b' ' | b'\t') {
                        a += 1;
                    }
                    let foll = a < len && matches!(raw[a], b'\n' | b'\r' | b'%');
                    if (prec || foll)
                        && has_structural_after_endobj_cached(raw, i + 6, &mut endobj_cache)
                    {
                        cross_obj = true;
                    }
                } else if i > 0 && is_line_eof_marker(raw, i) {
                    cross_obj = true;
                } else if i > 0 && matches!(raw[i - 1], b'\n' | b'\r') {
                    let mut t = i;
                    while t < len && matches!(raw[t], b' ' | b'\t') {
                        t += 1;
                    }
                    let is_stream = is_standalone_pdf_kw(raw, t, b"stream")
                        && (t + 6 >= len || matches!(raw[t + 6], b'\r' | b'\n' | b' ' | b'\t'));
                    let is_xref = is_standalone_pdf_kw(raw, t, b"xref")
                        && t + 4 < len
                        && (is_pdf_ws(raw[t + 4]) || raw[t + 4] == b'%');
                    let is_startxref = is_standalone_pdf_kw(raw, t, b"startxref") && {
                        let qs = skip_pdf_ws_stopping_at_eof(raw, t + 9, len);
                        qs > t + 9 && qs < len && raw[qs].is_ascii_digit()
                    };
                    if is_stream || is_xref || is_startxref {
                        cross_obj = true;
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    let cross = cross_obj || paren_depth != 0;
    (cross, cross || dict_depth != 0 || name_splice)
}

fn raw_paren_str_has_splice_markers(raw: &[u8]) -> bool {
    classify_raw_paren_splice(raw).1
}

fn bytes_contain_pdf_name(haystack: &[u8], expected: &str) -> bool {
    let mut i = 0usize;
    while i < haystack.len() {
        if haystack[i] == b'/' {
            let mut k = i + 1;
            while k < haystack.len() && !is_pdf_delim_or_ws(haystack[k]) {
                k += 1;
            }
            if k > i + 1 && decode_pdf_name(&haystack[i + 1..k]) == expected {
                return true;
            }
            i = k;
        } else {
            i += 1;
        }
    }
    false
}

fn contains_trailer_dict_kw(haystack: &[u8]) -> bool {
    let mut i = 0usize;
    while i + 7 <= haystack.len() {
        if is_standalone_pdf_kw(haystack, i, b"trailer") {
            let qt = skip_pdf_ws_stopping_at_eof(haystack, i + 7, haystack.len());
            if qt + 1 < haystack.len() && haystack[qt] == b'<' && haystack[qt + 1] == b'<' {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn is_type_xref_entry_at(bytes: &[u8], i: usize, limit: usize) -> bool {
    let end = limit.min(bytes.len());
    if i >= end || bytes[i] != b'/' {
        return false;
    }
    let mut k = i + 1;
    while k < end && !is_pdf_delim_or_ws(bytes[k]) {
        k += 1;
    }
    if k == i + 1 || decode_pdf_name(&bytes[i + 1..k]) != "/Type" {
        return false;
    }
    let q = skip_pdf_ws_stopping_at_eof(bytes, k, end);
    if q >= end || bytes[q] != b'/' {
        return false;
    }
    let mut m = q + 1;
    while m < end && !is_pdf_delim_or_ws(bytes[m]) {
        m += 1;
    }
    m > q + 1 && decode_pdf_name(&bytes[q + 1..m]) == "/XRef"
}

fn is_encrypt_entry_at(bytes: &[u8], i: usize, limit: usize) -> Option<usize> {
    let end = limit.min(bytes.len());
    if i >= end || bytes[i] != b'/' {
        return None;
    }
    let mut k = i + 1;
    while k < end && !is_pdf_delim_or_ws(bytes[k]) {
        k += 1;
    }
    if k == i + 1 || decode_pdf_name(&bytes[i + 1..k]) != "/Encrypt" {
        return None;
    }
    let q = skip_pdf_ws_stopping_at_eof(bytes, k, end);
    if q + 1 < end && bytes[q] == b'<' && bytes[q + 1] == b'<' {
        return Some(k);
    }
    if q < end && bytes[q].is_ascii_digit() {
        let mut d1 = q;
        while d1 < end && bytes[d1].is_ascii_digit() {
            d1 += 1;
        }
        let q2 = skip_pdf_ws_stopping_at_eof(bytes, d1, end);
        if q2 > d1 && q2 < end && bytes[q2].is_ascii_digit() {
            let mut d2 = q2;
            while d2 < end && bytes[d2].is_ascii_digit() {
                d2 += 1;
            }
            let q3 = skip_pdf_ws_stopping_at_eof(bytes, d2, end);
            if q3 > d2 && is_standalone_pdf_kw(&bytes[..end], q3, b"R") {
                return Some(k);
            }
        }
    }
    None
}

fn bytes_contain_encrypt_entry(haystack: &[u8]) -> bool {
    let len = haystack.len();
    let mut i = 0usize;
    while i < len {
        if haystack[i] == b'/' {
            if is_encrypt_entry_at(haystack, i, len).is_some() {
                return true;
            }
            let mut k = i + 1;
            while k < len && !is_pdf_delim_or_ws(haystack[k]) {
                k += 1;
            }
            i = k;
        } else {
            i += 1;
        }
    }
    false
}

fn value_contains_spliced_encrypt(val: &PdfObjValue) -> bool {
    match val {
        PdfObjValue::ParenStr(raw) => {
            raw_paren_str_has_splice_markers(raw) && bytes_contain_encrypt_entry(raw)
        }
        PdfObjValue::Array(items) => items.iter().any(value_contains_spliced_encrypt),
        PdfObjValue::Dict(entries) => entries
            .iter()
            .any(|(_, v)| value_contains_spliced_encrypt(v)),
        _ => false,
    }
}

fn value_contains_spliced_xref_or_trailer(val: &PdfObjValue) -> bool {
    match val {
        PdfObjValue::ParenStr(raw) => {
            raw_paren_str_has_splice_markers(raw)
                && (bytes_contain_pdf_name(raw, "/XRef") || contains_trailer_dict_kw(raw))
        }
        PdfObjValue::Array(items) => items.iter().any(value_contains_spliced_xref_or_trailer),
        PdfObjValue::Dict(entries) => entries
            .iter()
            .any(|(_, v)| value_contains_spliced_xref_or_trailer(v)),
        _ => false,
    }
}

fn value_contains_xref_or_trailer_and_encrypt(val: &PdfObjValue) -> bool {
    match val {
        PdfObjValue::ParenStr(raw) => {
            raw_paren_str_has_splice_markers(raw)
                && (bytes_contain_pdf_name(raw, "/XRef") || contains_trailer_dict_kw(raw))
                && bytes_contain_encrypt_entry(raw)
        }
        PdfObjValue::Array(items) => items.iter().any(value_contains_xref_or_trailer_and_encrypt),
        PdfObjValue::Dict(entries) => entries
            .iter()
            .any(|(_, v)| value_contains_xref_or_trailer_and_encrypt(v)),
        _ => false,
    }
}

pub(in crate::text_pdf_codecs) fn trailer_or_xref_has_spliced_encrypt(
    entries: &[(String, PdfObjValue)],
    has_encrypt: bool,
) -> Option<bool> {
    if entries
        .iter()
        .any(|(_, v)| value_contains_spliced_encrypt(v))
    {
        return Some(true);
    }
    let val = dict_get(entries, "/ID")?;
    let malformed = match val {
        PdfObjValue::Null | PdfObjValue::Ref(_, _) if !has_encrypt => false,
        PdfObjValue::Array(items) if items.len() == 2 => items
            .iter()
            .any(|item| !matches!(item, PdfObjValue::ParenStr(_) | PdfObjValue::HexStr(_))),
        _ => true,
    };
    malformed.then_some(has_encrypt)
}

pub(in crate::text_pdf_codecs) fn indirect_val_has_spliced_encrypt(
    val: &PdfObjValue,
) -> Option<bool> {
    if value_contains_xref_or_trailer_and_encrypt(val) {
        return Some(true);
    }
    if let PdfObjValue::Dict(entries) = val {
        if dict_has_non_null_key(entries, "/Encrypt")
            && entries
                .iter()
                .any(|(_, v)| value_contains_spliced_xref_or_trailer(v))
        {
            return Some(true);
        }
        if dict_name_eq(entries, "/Type", "/XRef") {
            let has_enc = dict_has_non_null_key(entries, "/Encrypt");
            return trailer_or_xref_has_spliced_encrypt(entries, has_enc);
        }
    }
    None
}

#[derive(Clone, Copy)]
enum DamagedLengthHint {
    Direct(usize),
    Indirect(u32, u16),
}

struct DamagedParenTables<'a> {
    strict: &'a ParenJumpTable,
    damaged: &'a ParenJumpTable,
}

fn is_damaged_top_boundary(
    bytes: &[u8],
    q: usize,
    limit: usize,
    allow_stream: bool,
    endobj_cache: &mut (usize, usize),
) -> Option<bool> {
    if q >= limit {
        return Some(false);
    }
    if is_line_eof_marker(bytes, q) {
        return Some(false);
    }
    if allow_stream
        && is_standalone_pdf_kw(bytes, q, b"stream")
        && (q + 6 >= bytes.len() || matches!(bytes[q + 6], b'\r' | b'\n' | b' ' | b'\t'))
    {
        return Some(true);
    }
    if is_standalone_pdf_kw(bytes, q, b"endstream") {
        return Some(false);
    }
    if is_standalone_pdf_kw(bytes, q, b"endobj")
        && (q + 6 >= bytes.len() || has_structural_after_endobj_cached(bytes, q + 6, endobj_cache))
    {
        return Some(false);
    }
    if is_standalone_pdf_kw(bytes, q, b"xref")
        && q + 4 < bytes.len()
        && (is_pdf_ws(bytes[q + 4]) || bytes[q + 4] == b'%')
    {
        return Some(false);
    }
    if is_standalone_pdf_kw(bytes, q, b"trailer") {
        let qt = skip_pdf_ws_stopping_at_eof(bytes, q + 7, bytes.len());
        if qt + 1 < bytes.len() && bytes[qt] == b'<' && bytes[qt + 1] == b'<' {
            return Some(false);
        }
    }
    if is_standalone_pdf_kw(bytes, q, b"startxref") {
        let qs = skip_pdf_ws_stopping_at_eof(bytes, q + 9, bytes.len());
        if qs > q + 9 && qs < bytes.len() && bytes[qs].is_ascii_digit() {
            return Some(false);
        }
    }
    if match_indirect_obj_header(bytes, q, bytes.len()).is_some() {
        return Some(false);
    }
    None
}

fn is_outer_dict_close(
    bytes: &[u8],
    p: usize,
    limit: usize,
    allow_stream: bool,
    endobj_cache: &mut (usize, usize),
) -> bool {
    if p + 1 >= limit || bytes[p] != b'>' || bytes[p + 1] != b'>' {
        return false;
    }
    let after_dict = skip_pdf_ws_stopping_at_eof(bytes, p + 2, limit);
    after_dict >= limit
        || is_damaged_top_boundary(bytes, after_dict, limit, allow_stream, endobj_cache).is_some()
}

pub(super) fn step_closed_unspliced_single(
    bytes: &[u8],
    paren_table: &ParenJumpTable,
    q: usize,
    limit: usize,
) -> Option<usize> {
    if q >= limit || !matches!(bytes[q], b'(' | b'<') {
        return None;
    }
    if q + 1 < limit && bytes[q] == b'<' && bytes[q + 1] == b'<' {
        return None;
    }
    let step = step_indexed_non_code_token(bytes, paren_table, q, limit).ok()??;
    if let PdfNonCodeKind::ParenString {
        inner_start,
        inner_end,
    } = step.kind
    {
        let raw = &bytes[inner_start..inner_end];
        let (cross_obj, any_splice) = classify_raw_paren_splice(raw);
        if cross_obj
            || (any_splice
                && (bytes_contain_encrypt_entry(raw)
                    || bytes_contain_pdf_name(raw, "/XRef")
                    || contains_trailer_dict_kw(raw)))
        {
            return None;
        }
    }
    Some(step.next_index)
}

fn step_closed_unspliced_literal(
    bytes: &[u8],
    tables: &DamagedParenTables<'_>,
    q: usize,
    limit: usize,
) -> Option<usize> {
    step_closed_unspliced_single(bytes, tables.strict, q, limit)
        .or_else(|| step_closed_unspliced_single(bytes, tables.damaged, q, limit))
}

fn parse_value_prefer_strict(
    bytes: &[u8],
    tables: &DamagedParenTables<'_>,
    p: usize,
    limit: usize,
    depth: usize,
) -> Option<(PdfObjValue, usize)> {
    if let Ok(Some((val, next_p))) =
        parse_pdf_value_with_table(bytes, Some(tables.strict), p, limit, depth)
    {
        if !value_contains_spliced_encrypt(&val) && !value_contains_spliced_xref_or_trailer(&val) {
            return Some((val, next_p));
        }
    }
    parse_pdf_value_with_table(bytes, Some(tables.damaged), p, limit, depth)
        .ok()
        .flatten()
}

fn record_spliced_dict_entries(bytes: &[u8], q: usize, limit: usize, spliced: &mut (bool, bool)) {
    if bytes[q] == b'/' {
        if is_encrypt_entry_at(bytes, q, limit).is_some() {
            spliced.1 = true;
        }
        if is_type_xref_entry_at(bytes, q, limit) {
            spliced.0 = true;
        }
    }
}

fn step_clean_comment_or_literal(
    bytes: &[u8],
    tables: &DamagedParenTables<'_>,
    q: usize,
    limit: usize,
    unclosed_paren: &mut bool,
) -> Option<usize> {
    if *unclosed_paren {
        return None;
    }
    if bytes[q] == b'%' && !is_line_eof_marker(bytes, q) {
        let next_q = skip_pdf_ws_stopping_at_eof(bytes, q, limit);
        if next_q > q {
            return Some(next_q);
        }
    }
    if let Some(next_q) = step_closed_unspliced_literal(bytes, tables, q, limit) {
        return Some(next_q);
    }
    if bytes[q] == b'(' {
        *unclosed_paren = true;
    }
    None
}

fn find_id_array_end(
    bytes: &[u8],
    tables: &DamagedParenTables<'_>,
    open_bracket: usize,
    limit: usize,
    allow_stream: bool,
    endobj_cache: &mut (usize, usize),
) -> Option<usize> {
    if open_bracket >= limit || bytes[open_bracket] != b'[' {
        return None;
    }
    let mut unclosed_paren = false;
    let mut q = skip_pdf_ws_stopping_at_eof(bytes, open_bracket + 1, limit);
    while q < limit {
        if let Some(next_q) =
            step_clean_comment_or_literal(bytes, tables, q, limit, &mut unclosed_paren)
        {
            q = next_q;
            continue;
        }
        let check_boundary = !unclosed_paren || q == 0 || matches!(bytes[q - 1], b'\n' | b'\r');
        if check_boundary
            && is_damaged_top_boundary(bytes, q, limit, allow_stream, endobj_cache).is_some()
        {
            return None;
        }
        if bytes[q] == b']' {
            let span = &bytes[open_bracket..q];
            if bytes_contain_encrypt_entry(span) {
                return None;
            }
            let after = skip_pdf_ws_stopping_at_eof(bytes, q + 1, limit);
            let at_next_key =
                after + 1 < limit && bytes[after] == b'/' && !is_pdf_delim_or_ws(bytes[after + 1]);
            if at_next_key || is_outer_dict_close(bytes, after, limit, allow_stream, endobj_cache) {
                return Some(q + 1);
            }
        }
        q += 1;
    }
    None
}

fn recover_broken_dict_value(
    bytes: &[u8],
    tables: &DamagedParenTables<'_>,
    p: usize,
    limit: usize,
    allow_stream: bool,
    endobj_cache: &mut (usize, usize),
    spliced: &mut (bool, bool),
) -> (usize, Option<bool>) {
    let mut unclosed_paren = false;
    let is_arr = bytes[p] == b'[';
    let is_dict = p + 1 < limit && bytes[p] == b'<' && bytes[p + 1] == b'<';
    if is_arr || is_dict {
        let mut depth = 1usize;
        let mut q = p + if is_dict { 2 } else { 1 };
        while q < limit {
            if let Some(next_q) =
                step_clean_comment_or_literal(bytes, tables, q, limit, &mut unclosed_paren)
            {
                q = next_q;
                continue;
            }
            if let Some(hs) = is_damaged_top_boundary(bytes, q, limit, allow_stream, endobj_cache) {
                return (q, Some(hs));
            }
            record_spliced_dict_entries(bytes, q, limit, spliced);
            if is_arr {
                if bytes[q] == b'[' {
                    depth += 1;
                } else if bytes[q] == b']' {
                    depth -= 1;
                    if depth == 0 {
                        return (q + 1, None);
                    }
                } else if q + 1 < limit && bytes[q] == b'>' && bytes[q + 1] == b'>' && depth == 1 {
                    return (q, None);
                }
            } else if q + 1 < limit && bytes[q] == b'<' && bytes[q + 1] == b'<' {
                depth += 1;
                q += 2;
                continue;
            } else if q + 1 < limit && bytes[q] == b'>' && bytes[q + 1] == b'>' {
                depth -= 1;
                if depth == 0 {
                    return (q + 2, None);
                }
                q += 2;
                continue;
            }
            q += 1;
        }
        return (limit, None);
    }
    let mut q = p;
    while q < limit {
        if let Some(next_q) =
            step_clean_comment_or_literal(bytes, tables, q, limit, &mut unclosed_paren)
        {
            q = next_q;
            continue;
        }
        if let Some(hs) = is_damaged_top_boundary(bytes, q, limit, allow_stream, endobj_cache) {
            return (q, Some(hs));
        }
        if q > p
            && ((q + 1 < limit && bytes[q] == b'>' && bytes[q + 1] == b'>') || bytes[q] == b'/')
        {
            return (q, None);
        }
        q += 1;
    }
    (limit, None)
}

fn walk_damaged_dict_or_body(
    bytes: &[u8],
    tables: &DamagedParenTables<'_>,
    start: usize,
    limit: usize,
    allow_stream: bool,
    endobj_cache: &mut (usize, usize),
) -> (bool, bool, Option<DamagedLengthHint>, usize, bool) {
    let p0 = skip_pdf_ws_stopping_at_eof(bytes, start, limit);
    if p0 >= limit {
        return (false, false, None, limit, false);
    }
    if p0 + 1 >= limit || bytes[p0] != b'<' || bytes[p0 + 1] != b'<' {
        let mut unclosed_paren = false;
        let mut spliced = (false, false);
        let mut q = p0;
        while q < limit {
            if let Some(next_q) =
                step_clean_comment_or_literal(bytes, tables, q, limit, &mut unclosed_paren)
            {
                q = next_q;
                continue;
            }
            if let Some(hs) = is_damaged_top_boundary(bytes, q, limit, allow_stream, endobj_cache) {
                return (spliced.0, spliced.1, None, q, hs);
            }
            record_spliced_dict_entries(bytes, q, limit, &mut spliced);
            q += 1;
        }
        return (spliced.0, spliced.1, None, limit, false);
    }

    let mut is_xref_type = false;
    let mut spliced = (false, false);
    let mut has_encrypt_key = false;
    let mut length_hint: Option<DamagedLengthHint> = None;
    let mut p = p0 + 2;

    while p < limit {
        p = skip_pdf_ws_stopping_at_eof(bytes, p, limit);
        if p >= limit {
            break;
        }
        if p + 1 < limit && bytes[p] == b'>' && bytes[p + 1] == b'>' {
            p += 2;
            if is_outer_dict_close(bytes, p - 2, limit, allow_stream, endobj_cache) {
                break;
            }
            continue;
        }
        if let Some(hs) = is_damaged_top_boundary(bytes, p, limit, allow_stream, endobj_cache) {
            return (
                is_xref_type || spliced.0,
                has_encrypt_key || spliced.1,
                length_hint,
                p,
                hs,
            );
        }
        if bytes[p] != b'/' {
            let (next_p, hit) = recover_broken_dict_value(
                bytes,
                tables,
                p,
                limit,
                allow_stream,
                endobj_cache,
                &mut spliced,
            );
            if let Some(hs) = hit {
                return (
                    is_xref_type || spliced.0,
                    has_encrypt_key || spliced.1,
                    length_hint,
                    next_p,
                    hs,
                );
            }
            p = next_p.max(p + 1);
            continue;
        }
        let mut k = p + 1;
        while k < limit && !is_pdf_delim_or_ws(bytes[k]) {
            k += 1;
        }
        let key = decode_pdf_name(&bytes[p + 1..k]);
        p = skip_pdf_ws_stopping_at_eof(bytes, k, limit);
        if p >= limit {
            break;
        }
        if p + 1 < limit && bytes[p] == b'>' && bytes[p + 1] == b'>' {
            p += 2;
            if is_outer_dict_close(bytes, p - 2, limit, allow_stream, endobj_cache) {
                break;
            }
            continue;
        }
        if let Some(hs) = is_damaged_top_boundary(bytes, p, limit, allow_stream, endobj_cache) {
            return (
                is_xref_type || spliced.0,
                has_encrypt_key || spliced.1,
                length_hint,
                p,
                hs,
            );
        }
        if key == "/Encrypt" && !is_standalone_pdf_kw(&bytes[..limit], p, b"null") {
            has_encrypt_key = true;
        }
        let parsed_val = parse_value_prefer_strict(bytes, tables, p, limit, 1);
        let accepted_val = parsed_val.and_then(|(val, next_p)| {
            let valid_follow = if matches!(val, PdfObjValue::ParenStr(_) | PdfObjValue::Array(_)) {
                let after_v = skip_pdf_ws_stopping_at_eof(bytes, next_p, limit);
                after_v < limit
                    && (bytes[after_v] == b'/'
                        || (after_v + 1 < limit
                            && bytes[after_v] == b'>'
                            && bytes[after_v + 1] == b'>'))
            } else {
                true
            };
            valid_follow.then_some((val, next_p))
        });
        if let Some((val, next_p)) = accepted_val {
            if value_contains_spliced_encrypt(&val) {
                spliced.1 = true;
            }
            if value_contains_spliced_xref_or_trailer(&val) {
                spliced.0 = true;
            }
            if key == "/Type" && matches!(&val, PdfObjValue::Name(n) if n == "/XRef") {
                is_xref_type = true;
            }
            if key == "/Encrypt" && !matches!(val, PdfObjValue::Null) {
                has_encrypt_key = true;
            }
            if key == "/Length" {
                length_hint = match val {
                    PdfObjValue::Int(n) => usize::try_from(n).ok().map(DamagedLengthHint::Direct),
                    PdfObjValue::Ref(lo, lg) => Some(DamagedLengthHint::Indirect(lo, lg)),
                    _ => None,
                };
            }
            p = next_p;
        } else {
            if key == "/ID" && bytes[p] == b'[' {
                if let Some(id_end) =
                    find_id_array_end(bytes, tables, p, limit, allow_stream, endobj_cache)
                {
                    p = id_end;
                    continue;
                }
            }
            let recover_start = if key == "/ID" && bytes[p] == b'[' {
                (p + 1).min(limit)
            } else {
                p
            };
            let (next_p, hit) = recover_broken_dict_value(
                bytes,
                tables,
                recover_start,
                limit,
                allow_stream,
                endobj_cache,
                &mut spliced,
            );
            if let Some(hs) = hit {
                return (
                    is_xref_type || spliced.0,
                    has_encrypt_key || spliced.1,
                    length_hint,
                    next_p,
                    hs,
                );
            }
            p = next_p.max(p + 1);
        }
    }

    let is_xref_out = is_xref_type || spliced.0;
    let has_enc_out = has_encrypt_key || spliced.1;
    let after_dict = skip_pdf_ws_stopping_at_eof(bytes, p, limit);
    if allow_stream
        && is_standalone_pdf_kw(bytes, after_dict, b"stream")
        && (after_dict + 6 >= bytes.len()
            || matches!(bytes[after_dict + 6], b'\r' | b'\n' | b' ' | b'\t'))
    {
        return (is_xref_out, has_enc_out, length_hint, after_dict, true);
    }
    if is_standalone_pdf_kw(bytes, after_dict, b"endobj") {
        return (is_xref_out, has_enc_out, length_hint, after_dict + 6, false);
    }
    (is_xref_out, has_enc_out, length_hint, p, false)
}

fn advance_past_damaged_stream(
    bytes: &[u8],
    stream_start: usize,
    resolved_len: Option<usize>,
) -> usize {
    if let Some((_, _, after_endobj)) =
        resolved_len.and_then(|len| verify_direct_stream_bounds(bytes, stream_start, len))
    {
        return after_endobj;
    }
    if let Some((_, after_endstream)) = resolve_stream_bounds(bytes, stream_start, resolved_len) {
        let end_p = skip_pdf_ws_stopping_at_eof(bytes, after_endstream, bytes.len());
        if is_standalone_pdf_kw(bytes, end_p, b"endobj") {
            end_p + 6
        } else {
            after_endstream
        }
    } else {
        skip_damaged_truncated_stream(bytes, stream_start)
    }
}

fn try_step_undamaged_object(
    bytes: &[u8],
    tables: &DamagedParenTables<'_>,
    int_objs: &IntObjIndex,
    after_obj: usize,
    rev_limit: usize,
) -> Option<(bool, usize)> {
    let (val, next_p) = parse_value_prefer_strict(bytes, tables, after_obj, rev_limit, 0)?;
    if let Some(spliced_enc) = indirect_val_has_spliced_encrypt(&val) {
        if spliced_enc {
            return Some((true, next_p));
        }
        return None;
    }
    if let PdfObjValue::Dict(ref entries) = val {
        if dict_name_eq(entries, "/Type", "/XRef") && dict_has_non_null_key(entries, "/Encrypt") {
            return Some((true, next_p));
        }
    }
    let kw_p = skip_pdf_ws_stopping_at_eof(bytes, next_p, rev_limit);
    if is_standalone_pdf_kw(bytes, kw_p, b"stream") {
        let PdfObjValue::Dict(ref entries) = val else {
            return None;
        };
        let stream_start = advance_past_stream_eol(bytes, kw_p + 6);
        let len_hint = match dict_get(entries, "/Length") {
            Some(PdfObjValue::Int(n)) => usize::try_from(*n).ok(),
            Some(PdfObjValue::Ref(lo, lg)) => {
                lookup_indexed_int_obj(int_objs, bytes, stream_start, *lo, *lg)
            }
            _ => None,
        };
        return Some((
            false,
            advance_past_damaged_stream(bytes, stream_start, len_hint),
        ));
    }
    if is_standalone_pdf_kw(bytes, kw_p, b"endobj") {
        return Some((false, kw_p + 6));
    }
    None
}

fn has_encrypt_in_damaged_or_cipher_pdf(bytes: &[u8], start_offset: usize) -> bool {
    let strict_table = ParenJumpTable::build(bytes);
    let damaged_table = ParenJumpTable::build_damaged(bytes);
    let tables = DamagedParenTables {
        strict: &strict_table,
        damaged: &damaged_table,
    };
    let (eof_markers, int_objs) = build_eof_and_int_index(bytes, &strict_table);
    let mut endobj_cache = (0usize, 0usize);
    let mut p = start_offset.min(bytes.len());
    while p < bytes.len() {
        if is_pdf_ws(bytes[p]) {
            p += 1;
            continue;
        }
        if bytes[p] == b'%' {
            if is_line_eof_marker(bytes, p) {
                p += 5;
                continue;
            }
            while p < bytes.len() && bytes[p] != b'\n' && bytes[p] != b'\r' {
                p += 1;
            }
            continue;
        }
        if is_standalone_pdf_kw(bytes, p, b"trailer") {
            let start = p + 7;
            let eof_idx = eof_markers.partition_point(|&pos| pos < start);
            let trailer_limit = eof_markers.get(eof_idx).copied().unwrap_or(bytes.len());
            let (_, has_enc, _, next_pos, _) = walk_damaged_dict_or_body(
                bytes,
                &tables,
                start,
                trailer_limit,
                false,
                &mut endobj_cache,
            );
            if has_enc {
                return true;
            }
            p = next_pos.max(start);
            continue;
        }
        if let Some(after_obj) = match_indirect_obj_header(bytes, p, bytes.len()) {
            let eof_idx = eof_markers.partition_point(|&pos| pos < after_obj);
            let rev_limit = eof_markers.get(eof_idx).copied().unwrap_or(bytes.len());
            if let Some((found_enc, next_pos)) =
                try_step_undamaged_object(bytes, &tables, &int_objs, after_obj, rev_limit)
            {
                if found_enc {
                    return true;
                }
                p = next_pos.max(after_obj);
                continue;
            }
            let (is_xref, has_enc, len_hint, next_pos, hit_stream) = walk_damaged_dict_or_body(
                bytes,
                &tables,
                after_obj,
                rev_limit,
                true,
                &mut endobj_cache,
            );
            if is_xref && has_enc {
                return true;
            }
            if hit_stream {
                let stream_start = advance_past_stream_eol(bytes, next_pos + 6);
                let resolved_len = len_hint.and_then(|h| match h {
                    DamagedLengthHint::Direct(n) => Some(n),
                    DamagedLengthHint::Indirect(lo, lg) => {
                        lookup_indexed_int_obj(&int_objs, bytes, stream_start, lo, lg)
                    }
                });
                p = advance_past_damaged_stream(bytes, stream_start, resolved_len);
            } else {
                p = next_pos.max(after_obj);
            }
            continue;
        }
        p += 1;
    }
    false
}

pub(in crate::text_pdf_codecs) fn fail_corrupted_or_encrypted(
    bytes: &[u8],
    start_offset: usize,
    has_encrypt: bool,
    detail: &str,
) -> EasyOfficeError {
    if has_encrypt || has_encrypt_in_damaged_or_cipher_pdf(bytes, start_offset) {
        EasyOfficeError::EncryptedDocumentUnsupported(DocumentFormat::Pdf)
    } else {
        EasyOfficeError::CorruptedContainer(detail.to_string())
    }
}

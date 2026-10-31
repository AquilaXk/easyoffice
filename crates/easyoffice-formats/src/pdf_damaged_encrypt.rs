use super::{
    dict_get, dict_has_non_null_key, dict_name_eq, parse_pdf_value_with_table,
    resolve_stream_bounds, verify_direct_stream_bounds, PdfObjValue,
};
use crate::pdf_stream_parser::{
    advance_past_stream_eol, decode_pdf_name, has_structural_after_endobj, is_pdf_delim_or_ws,
    is_pdf_ws, is_standalone_pdf_kw, match_indirect_obj_header, parse_indirect_obj_header,
    skip_damaged_truncated_stream, ParenJumpTable,
};
use crate::{DocumentFormat, EasyOfficeError};
use std::collections::HashMap;

fn skip_ws_stopping_at_eof_marker(bytes: &[u8], mut p: usize, limit: usize) -> usize {
    while p < limit {
        if is_pdf_ws(bytes[p]) {
            p += 1;
        } else if bytes[p] == b'%' {
            if p + 5 <= bytes.len() && &bytes[p..p + 5] == b"%%EOF" {
                break;
            }
            p += 1;
            while p < limit && bytes[p] != b'\n' && bytes[p] != b'\r' {
                p += 1;
            }
        } else {
            break;
        }
    }
    p
}

fn is_damaged_top_boundary(
    bytes: &[u8],
    q: usize,
    limit: usize,
    allow_stream: bool,
) -> Option<bool> {
    if q >= limit {
        return Some(false);
    }
    if q + 5 <= bytes.len() && &bytes[q..q + 5] == b"%%EOF" {
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
        && (q + 6 >= bytes.len() || has_structural_after_endobj(bytes, q + 6))
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
        let qt = skip_ws_stopping_at_eof_marker(bytes, q + 7, bytes.len());
        if qt + 1 < bytes.len() && bytes[qt] == b'<' && bytes[qt + 1] == b'<' {
            return Some(false);
        }
    }
    if is_standalone_pdf_kw(bytes, q, b"startxref") {
        let qs = skip_ws_stopping_at_eof_marker(bytes, q + 9, bytes.len());
        if qs > q + 9 && qs < bytes.len() && bytes[qs].is_ascii_digit() {
            return Some(false);
        }
    }
    if match_indirect_obj_header(bytes, q, bytes.len()).is_some() {
        return Some(false);
    }
    None
}

type IntObjIndex = HashMap<(u32, u16), Vec<(usize, usize)>>;

fn build_eof_and_int_index(
    bytes: &[u8],
    paren_table: &ParenJumpTable,
) -> (Vec<usize>, IntObjIndex) {
    let mut eof_markers = Vec::new();
    let mut int_objs: IntObjIndex = HashMap::new();
    let mut q = 0usize;
    while q < bytes.len() {
        if q + 5 <= bytes.len() && &bytes[q..q + 5] == b"%%EOF" {
            eof_markers.push(q);
            q += 5;
            continue;
        }
        if let Some((on, gn, after_hdr)) = parse_indirect_obj_header(bytes, q, bytes.len()) {
            if let Ok(Some((PdfObjValue::Int(n), after_val))) =
                parse_pdf_value_with_table(bytes, Some(paren_table), after_hdr, bytes.len(), 0)
            {
                let end_p = skip_ws_stopping_at_eof_marker(bytes, after_val, bytes.len());
                if is_standalone_pdf_kw(bytes, end_p, b"endobj") {
                    if let Ok(u) = usize::try_from(n) {
                        int_objs.entry((on, gn)).or_default().push((q, u));
                    }
                    q = end_p + 6;
                    continue;
                }
            }
        }
        q += 1;
    }
    (eof_markers, int_objs)
}

fn lookup_indexed_int_obj(
    int_objs: &IntObjIndex,
    bytes: &[u8],
    stream_start: usize,
    target_obj: u32,
    target_gen: u16,
) -> Option<usize> {
    let candidates = int_objs.get(&(target_obj, target_gen))?;
    let mut fallback = None;
    for &(obj_pos, len_val) in candidates {
        if obj_pos < stream_start {
            continue;
        }
        if verify_direct_stream_bounds(bytes, stream_start, len_val).is_some() {
            return Some(len_val);
        }
        fallback.get_or_insert(len_val);
    }
    fallback.or_else(|| candidates.last().map(|&(_, v)| v))
}

fn recover_broken_dict_value(
    bytes: &[u8],
    p: usize,
    limit: usize,
    allow_stream: bool,
) -> (usize, Option<bool>) {
    if bytes[p] == b'[' {
        let mut depth = 1usize;
        let mut q = p + 1;
        while q < limit {
            if let Some(hs) = is_damaged_top_boundary(bytes, q, limit, allow_stream) {
                return (q, Some(hs));
            }
            if bytes[q] == b'%' {
                q = skip_ws_stopping_at_eof_marker(bytes, q, limit);
                continue;
            }
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
            q += 1;
        }
        return (limit, None);
    }
    if p + 1 < limit && bytes[p] == b'<' && bytes[p + 1] == b'<' {
        let mut depth = 1usize;
        let mut q = p + 2;
        while q < limit {
            if let Some(hs) = is_damaged_top_boundary(bytes, q, limit, allow_stream) {
                return (q, Some(hs));
            }
            if bytes[q] == b'%' {
                q = skip_ws_stopping_at_eof_marker(bytes, q, limit);
                continue;
            }
            if q + 1 < limit && bytes[q] == b'<' && bytes[q + 1] == b'<' {
                depth += 1;
                q += 2;
                continue;
            }
            if q + 1 < limit && bytes[q] == b'>' && bytes[q + 1] == b'>' {
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
    let mut q = p + 1;
    while q < limit {
        if let Some(hs) = is_damaged_top_boundary(bytes, q, limit, allow_stream) {
            return (q, Some(hs));
        }
        if (q + 1 < limit && bytes[q] == b'>' && bytes[q + 1] == b'>') || bytes[q] == b'/' {
            return (q, None);
        }
        q += 1;
    }
    (limit, None)
}

fn walk_damaged_dict_or_body(
    bytes: &[u8],
    paren_table: &ParenJumpTable,
    int_objs: &IntObjIndex,
    start: usize,
    limit: usize,
    allow_stream: bool,
) -> (bool, bool, Option<usize>, usize, bool) {
    let p0 = skip_ws_stopping_at_eof_marker(bytes, start, limit);
    if p0 >= limit {
        return (false, false, None, limit, false);
    }
    if p0 + 1 >= limit || bytes[p0] != b'<' || bytes[p0 + 1] != b'<' {
        let mut q = p0;
        if bytes[q] == b'[' {
            q += 1;
            while q < limit {
                q = skip_ws_stopping_at_eof_marker(bytes, q, limit);
                if q >= limit || bytes[q] == b']' {
                    break;
                }
                if let Ok(Some((_, next_q))) =
                    parse_pdf_value_with_table(bytes, Some(paren_table), q, limit, 1)
                {
                    q = next_q;
                } else {
                    break;
                }
            }
        }
        while q < limit {
            if let Some(hs) = is_damaged_top_boundary(bytes, q, limit, allow_stream) {
                return (false, false, None, q, hs);
            }
            if bytes[q] == b'%' {
                q = skip_ws_stopping_at_eof_marker(bytes, q, limit);
                continue;
            }
            q += 1;
        }
        return (false, false, None, limit, false);
    }

    let mut is_xref_type = false;
    let mut has_encrypt_key = false;
    let mut direct_length: Option<usize> = None;
    let mut effective_limit = limit;
    let mut boundary_hit: Option<bool> = None;
    let mut p = p0 + 2;
    let mut scan_cursor = p;

    while p < effective_limit {
        p = skip_ws_stopping_at_eof_marker(bytes, p, effective_limit);
        if p >= effective_limit {
            break;
        }
        if p + 1 < effective_limit && bytes[p] == b'>' && bytes[p + 1] == b'>' {
            p += 2;
            boundary_hit = None;
            break;
        }
        if let Some(hs) = is_damaged_top_boundary(bytes, p, limit, allow_stream) {
            return (is_xref_type, has_encrypt_key, direct_length, p, hs);
        }
        if bytes[p] != b'/' {
            let (next_p, hit) = recover_broken_dict_value(bytes, p, effective_limit, allow_stream);
            if let Some(hs) = hit {
                return (is_xref_type, has_encrypt_key, direct_length, next_p, hs);
            }
            p = next_p.max(p + 1);
            continue;
        }
        let mut k = p + 1;
        while k < effective_limit && !is_pdf_delim_or_ws(bytes[k]) {
            k += 1;
        }
        let key = decode_pdf_name(&bytes[p + 1..k]);
        p = skip_ws_stopping_at_eof_marker(bytes, k, effective_limit);
        if p >= effective_limit {
            break;
        }
        if p + 1 < effective_limit && bytes[p] == b'>' && bytes[p + 1] == b'>' {
            p += 2;
            boundary_hit = None;
            break;
        }
        if let Some(hs) = is_damaged_top_boundary(bytes, p, limit, allow_stream) {
            return (is_xref_type, has_encrypt_key, direct_length, p, hs);
        }
        if key == "/Encrypt" && !is_standalone_pdf_kw(&bytes[..effective_limit], p, b"null") {
            has_encrypt_key = true;
        }
        let parsed_val =
            parse_pdf_value_with_table(bytes, Some(paren_table), p, effective_limit, 1)
                .ok()
                .flatten();
        let accepted_val = parsed_val.and_then(|(val, next_p)| {
            scan_cursor = scan_cursor.max(p + 1);
            while scan_cursor < next_p {
                if let Some(hs) = is_damaged_top_boundary(bytes, scan_cursor, limit, allow_stream) {
                    effective_limit = scan_cursor;
                    boundary_hit = Some(hs);
                    break;
                }
                scan_cursor += 1;
            }
            if next_p > effective_limit {
                return None;
            }
            let valid_follow = if matches!(val, PdfObjValue::ParenStr(_)) {
                let after_v = skip_ws_stopping_at_eof_marker(bytes, next_p, effective_limit);
                after_v < effective_limit
                    && (bytes[after_v] == b'/'
                        || (after_v + 1 < effective_limit
                            && bytes[after_v] == b'>'
                            && bytes[after_v + 1] == b'>'))
            } else {
                true
            };
            valid_follow.then_some((val, next_p))
        });
        if let Some((val, next_p)) = accepted_val {
            if key == "/Type" && matches!(&val, PdfObjValue::Name(n) if n == "/XRef") {
                is_xref_type = true;
            }
            if key == "/Encrypt" && !matches!(val, PdfObjValue::Null) {
                has_encrypt_key = true;
            }
            if key == "/Length" {
                direct_length = match val {
                    PdfObjValue::Int(n) => usize::try_from(n).ok(),
                    PdfObjValue::Ref(lo, lg) => {
                        lookup_indexed_int_obj(int_objs, bytes, next_p, lo, lg)
                    }
                    _ => None,
                };
            }
            p = next_p;
        } else {
            let (next_p, hit) = recover_broken_dict_value(bytes, p, effective_limit, allow_stream);
            if let Some(hs) = hit {
                return (is_xref_type, has_encrypt_key, direct_length, next_p, hs);
            }
            p = next_p.max(p + 1);
        }
    }

    if let Some(hs) = boundary_hit {
        return (
            is_xref_type,
            has_encrypt_key,
            direct_length,
            effective_limit,
            hs,
        );
    }

    let after_dict = skip_ws_stopping_at_eof_marker(bytes, p, limit);
    if allow_stream
        && is_standalone_pdf_kw(bytes, after_dict, b"stream")
        && (after_dict + 6 >= bytes.len()
            || matches!(bytes[after_dict + 6], b'\r' | b'\n' | b' ' | b'\t'))
    {
        return (
            is_xref_type,
            has_encrypt_key,
            direct_length,
            after_dict,
            true,
        );
    }
    if is_standalone_pdf_kw(bytes, after_dict, b"endobj") {
        return (
            is_xref_type,
            has_encrypt_key,
            direct_length,
            after_dict + 6,
            false,
        );
    }
    (is_xref_type, has_encrypt_key, direct_length, p, false)
}

fn try_step_undamaged_object(
    bytes: &[u8],
    paren_table: &ParenJumpTable,
    int_objs: &IntObjIndex,
    after_obj: usize,
    rev_limit: usize,
) -> Option<(bool, usize)> {
    let (val, next_p) =
        parse_pdf_value_with_table(bytes, Some(paren_table), after_obj, rev_limit, 0).ok()??;
    if let PdfObjValue::Dict(ref entries) = val {
        if dict_name_eq(entries, "/Type", "/XRef") && dict_has_non_null_key(entries, "/Encrypt") {
            return Some((true, next_p));
        }
    }
    let kw_p = skip_ws_stopping_at_eof_marker(bytes, next_p, rev_limit);
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
        let after_stream = if let Some((_, after_endstream)) =
            resolve_stream_bounds(bytes, stream_start, len_hint)
        {
            let end_p = skip_ws_stopping_at_eof_marker(bytes, after_endstream, bytes.len());
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
    let paren_table = ParenJumpTable::build(bytes);
    let (eof_markers, int_objs) = build_eof_and_int_index(bytes, &paren_table);
    let mut p = 0usize;
    while p < bytes.len() {
        if is_pdf_ws(bytes[p]) {
            p += 1;
            continue;
        }
        if bytes[p] == b'%' {
            if p + 5 <= bytes.len() && &bytes[p..p + 5] == b"%%EOF" {
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
            if let Ok(Some((PdfObjValue::Dict(tr_entries), next_p))) =
                parse_pdf_value_with_table(bytes, Some(&paren_table), start, trailer_limit, 0)
            {
                let after_tr = skip_ws_stopping_at_eof_marker(bytes, next_p, trailer_limit);
                let clean_end = after_tr >= trailer_limit
                    || is_standalone_pdf_kw(bytes, after_tr, b"startxref")
                    || is_standalone_pdf_kw(bytes, after_tr, b"xref")
                    || is_standalone_pdf_kw(bytes, after_tr, b"trailer")
                    || match_indirect_obj_header(bytes, after_tr, bytes.len()).is_some();
                if clean_end {
                    if dict_has_non_null_key(&tr_entries, "/Encrypt") {
                        return true;
                    }
                    p = next_p.max(start);
                    continue;
                }
            }
            let (_, has_enc, _, next_pos, _) = walk_damaged_dict_or_body(
                bytes,
                &paren_table,
                &int_objs,
                start,
                trailer_limit,
                false,
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
                try_step_undamaged_object(bytes, &paren_table, &int_objs, after_obj, rev_limit)
            {
                if found_enc {
                    return true;
                }
                p = next_pos.max(after_obj);
                continue;
            }
            let (is_xref, has_enc, direct_len, next_pos, hit_stream) = walk_damaged_dict_or_body(
                bytes,
                &paren_table,
                &int_objs,
                after_obj,
                rev_limit,
                true,
            );
            if is_xref && has_enc {
                return true;
            }
            if hit_stream {
                let stream_start = advance_past_stream_eol(bytes, next_pos + 6);
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
    has_encrypt: bool,
    detail: &str,
) -> EasyOfficeError {
    if has_encrypt || has_encrypt_in_damaged_or_cipher_pdf(bytes) {
        EasyOfficeError::EncryptedDocumentUnsupported(DocumentFormat::Pdf)
    } else {
        EasyOfficeError::CorruptedContainer(detail.to_string())
    }
}

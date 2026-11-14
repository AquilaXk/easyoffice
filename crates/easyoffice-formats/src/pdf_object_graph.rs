#[path = "pdf_damaged_encrypt.rs"]
mod pdf_damaged_encrypt;
#[path = "pdf_opaque_stream.rs"]
mod pdf_opaque_stream;
pub(super) use pdf_opaque_stream::is_opaque_binary_stream;
use pdf_opaque_stream::validate_opaque_stream_declaration;
#[path = "pdf_splice_classifier.rs"]
mod pdf_splice_classifier;
#[path = "pdf_value_parser.rs"]
mod pdf_value_parser;
pub(super) use pdf_value_parser::{decode_pdf_stream_filters, parse_pdf_value_with_table};

use crate::pdf_stream_parser::{
    advance_past_stream_eol, contains_subslice, decode_pdf_hex_string,
    has_structural_after_endobj_cached, is_line_eof_marker, is_pdf_delim_or_ws,
    is_standalone_pdf_kw, is_valid_startxref_at, parse_indirect_obj_header,
    skip_pdf_ws_and_comments, skip_pdf_ws_stopping_at_eof, span_crosses_object_boundary,
    step_pdf_non_code_token, unescape_pdf_text, validate_balanced_pdf_literals, ParenJumpTable,
    PdfNonCodeKind, PdfNonCodeStep,
};
use crate::EasyOfficeError;
pub(super) use pdf_damaged_encrypt::fail_corrupted_or_encrypted;
use pdf_damaged_encrypt::step_closed_unspliced_single;
pub(super) use pdf_splice_classifier::{
    indirect_val_has_spliced_encrypt, trailer_or_xref_has_spliced_encrypt,
};
use std::collections::HashMap;

pub(super) type IntObjIndex = HashMap<(u32, u16), Vec<(usize, usize)>>;

pub(super) const MAX_PDF_NESTING_DEPTH: usize = 64;
pub(super) const MAX_PDF_STREAM_DECODE_BYTES: usize = 16 * 1024 * 1024;
pub(super) const MAX_PDF_TOTAL_DECODED_BYTES: usize = 32 * 1024 * 1024;
pub(super) const MAX_PDF_PAGE_TREE_DEPTH: usize = 64;
pub(super) const MAX_PDF_CONTENTS_ARRAY_LEN: usize = 256;
pub(super) const MAX_PDF_INDIRECT_OBJECTS: usize = 65_536;
pub(super) const MAX_PDF_OBJSTM_ENTRIES: usize = 4_096;
pub(super) const MAX_PDF_COLLECTION_LEN: usize = 16_384;
pub(super) const MAX_PDF_STRING_BYTES: usize = 1024 * 1024;
const MODEL_B64_COMMENT_PREFIX: &[u8] = b"%EasyOfficePDFModelBase64:";

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
    let mut found_cat_title = false;
    if let Some(cat) = catalog_entries {
        if let Some(t_val) = dict_get(cat, "/Title") {
            if let Some(decoded_t) = decode_text_value(t_val, objects) {
                title = decoded_t;
                found_cat_title = true;
            }
        }
    }
    if !found_cat_title {
        let info_val = trailers
            .iter()
            .rev()
            .find_map(|tr| dict_get(tr, "/Info"))
            .or_else(|| {
                ordered_objs.iter().rev().find_map(|(_, o)| {
                    if let PdfObjValue::Dict(ref d) = o.val {
                        if dict_name_eq(d, "/Type", "/XRef") {
                            return dict_get(d, "/Info");
                        }
                    }
                    None
                })
            });
        if let Some(iv) = info_val {
            if let Some(PdfObjValue::Dict(info_dict)) = resolve_value(iv, objects) {
                if let Some(t_val) = dict_get(info_dict, "/Title") {
                    if let Some(decoded_t) = decode_text_value(t_val, objects) {
                        title = decoded_t;
                    }
                }
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

fn scan_stream_to_endstream(
    bytes: &[u8],
    start: usize,
    skip_pdf_strings: bool,
) -> Option<(usize, usize)> {
    let kw = b"endstream";
    let mut endobj_cache = (0usize, 0usize);
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
        if is_standalone_pdf_kw(bytes, i, b"endobj")
            && has_structural_after_endobj_cached(bytes, i + 6, &mut endobj_cache)
        {
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

/// For an object whose dictionary is followed by `stream`, the offset after its `endobj` when the
/// declared (direct or previously indexed indirect) length verifies.
fn jump_over_indirect_stream(
    bytes: &[u8],
    strict_table: &ParenJumpTable,
    after_hdr: usize,
    int_objs: &IntObjIndex,
    eof_markers: &[usize],
) -> Option<usize> {
    let Ok(Some((PdfObjValue::Dict(entries), dict_end))) =
        parse_pdf_value_with_table(bytes, Some(strict_table), after_hdr, bytes.len(), 0)
    else {
        return None;
    };
    let kw_p = skip_pdf_ws_and_comments(bytes, dict_end, bytes.len());
    if !is_standalone_pdf_kw(bytes, kw_p, b"stream") {
        return None;
    }
    let stream_start = advance_past_stream_eol(bytes, kw_p + 6);
    let declared_len = match dict_get(&entries, "/Length") {
        Some(PdfObjValue::Int(n)) => usize::try_from(*n).ok(),
        Some(PdfObjValue::Ref(lo, lg)) => {
            lookup_indexed_int_obj(int_objs, eof_markers, bytes, stream_start, *lo, *lg)
        }
        _ => None,
    };
    declared_len
        .and_then(|u| verify_direct_stream_bounds(bytes, stream_start, u))
        .map(|(_, _, after_endobj)| after_endobj)
}

pub(super) fn build_eof_and_int_index(
    bytes: &[u8],
    strict_table: &ParenJumpTable,
) -> (Vec<usize>, IntObjIndex) {
    let mut eof_markers = Vec::new();
    let mut int_objs: IntObjIndex = HashMap::new();
    let mut seen_startxref_in_rev = false;
    let mut endobj_cache = (0usize, 0usize);
    let mut q = 0usize;
    while q < bytes.len() {
        if let Some(next_q) = step_closed_unspliced_single(bytes, strict_table, q, bytes.len()) {
            q = next_q;
            continue;
        }
        if bytes[q] == b'%' {
            if is_line_eof_marker(bytes, q) {
                let after_eof = skip_pdf_ws_and_comments(bytes, q + 5, bytes.len());
                if seen_startxref_in_rev
                    || after_eof >= bytes.len()
                    || has_structural_after_endobj_cached(bytes, q + 5, &mut endobj_cache)
                {
                    eof_markers.push(q);
                    seen_startxref_in_rev = false;
                }
            }
            while q < bytes.len() && bytes[q] != b'\n' && bytes[q] != b'\r' {
                q += 1;
            }
            continue;
        }
        if is_valid_startxref_at(bytes, q, bytes.len()) {
            seen_startxref_in_rev = true;
            q += 9;
            continue;
        }
        if bytes[q].is_ascii_digit() && (q == 0 || is_pdf_delim_or_ws(bytes[q - 1])) {
            if let Some((on, gn, after_hdr)) = parse_indirect_obj_header(bytes, q, bytes.len()) {
                match parse_direct_int_endobj(bytes, after_hdr, bytes.len()) {
                    Ok((u, next_q)) => {
                        int_objs.entry((on, gn)).or_default().push((q, u));
                        q = next_q;
                    }
                    Err(next_q) => {
                        let jumped_stream = jump_over_indirect_stream(
                            bytes,
                            strict_table,
                            after_hdr,
                            &int_objs,
                            &eof_markers,
                        );
                        q = jumped_stream.unwrap_or_else(|| next_q.max(after_hdr));
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
    (eof_markers, int_objs)
}

pub(super) fn lookup_indexed_int_obj(
    int_objs: &IntObjIndex,
    eof_markers: &[usize],
    bytes: &[u8],
    stream_start: usize,
    target_obj: u32,
    target_gen: u16,
) -> Option<usize> {
    let candidates = int_objs.get(&(target_obj, target_gen))?;
    let rev_idx = eof_markers.partition_point(|&pos| pos < stream_start);
    let rev_limit = eof_markers.get(rev_idx).copied().unwrap_or(bytes.len());
    let min_post_pos = |len_val: usize| stream_start.saturating_add(len_val);
    for &(obj_pos, len_val) in candidates {
        if obj_pos >= min_post_pos(len_val)
            && obj_pos < rev_limit
            && verify_direct_stream_bounds(bytes, stream_start, len_val).is_some()
        {
            return Some(len_val);
        }
    }
    for &(obj_pos, len_val) in candidates.iter().rev() {
        if obj_pos < stream_start
            && verify_direct_stream_bounds(bytes, stream_start, len_val).is_some()
        {
            return Some(len_val);
        }
    }
    for &(obj_pos, len_val) in candidates {
        if obj_pos >= rev_limit
            && obj_pos >= min_post_pos(len_val)
            && verify_direct_stream_bounds(bytes, stream_start, len_val).is_some()
        {
            return Some(len_val);
        }
    }
    None
}

pub(super) fn resolve_stream_length_hint(
    bytes: &[u8],
    stream_start: usize,
    entries: &[(String, PdfObjValue)],
    objects: &HashMap<(u32, u16), PdfIndirectObject>,
    strict_table: &ParenJumpTable,
    int_index: &mut Option<(Vec<usize>, IntObjIndex)>,
) -> Option<usize> {
    match dict_get(entries, "/Length")? {
        PdfObjValue::Int(n) => usize::try_from(*n).ok(),
        PdfObjValue::Ref(lo, lg) => {
            let (eof_markers, idx) =
                int_index.get_or_insert_with(|| build_eof_and_int_index(bytes, strict_table));
            if let Some(u) = lookup_indexed_int_obj(idx, eof_markers, bytes, stream_start, *lo, *lg)
            {
                return Some(u);
            }
            if let Some(obj) = objects.get(&(*lo, *lg)) {
                if let PdfObjValue::Int(n) = obj.val {
                    if let Ok(u) = usize::try_from(n) {
                        if verify_direct_stream_bounds(bytes, stream_start, u).is_some() {
                            return Some(u);
                        }
                    }
                }
            }
            None
        }
        _ => None,
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
        if is_opaque_binary_stream(entries) {
            validate_opaque_stream_declaration(entries, objects, raw_stream.len())?;
            if contains_subslice(raw_stream, MODEL_B64_COMMENT_PREFIX) {
                return Err(EasyOfficeError::SecurityViolation(
                    "PDF 비메타데이터 스트림 내부에 허용되지 않는 %EasyOfficePDFModelBase64 주석이 발견되었습니다."
                        .to_string(),
                ));
            }
            continue;
        }
        let decoded_opt =
            decode_pdf_stream_filters(raw_stream, entries, objects, remaining_budget)?;
        let is_meta = is_studio_metadata_dict(entries);
        if let Some(decoded) = decoded_opt {
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

pub(super) fn expand_pdf_object_streams(
    bytes: &[u8],
    objects: &mut HashMap<(u32, u16), PdfIndirectObject>,
    flate_cache: &HashMap<(u32, u16), Vec<u8>>,
    next_order_seq: &mut usize,
) -> Result<(), EasyOfficeError> {
    type ObjStmHeader = ((u32, u16), usize, usize, Option<(usize, usize)>, usize);
    let mut obj_stms: Vec<ObjStmHeader> = objects
        .iter()
        .filter_map(|(&id, obj)| {
            let PdfObjValue::Dict(ref d) = obj.val else {
                return None;
            };
            if !dict_name_eq(d, "/Type", "/ObjStm") {
                return None;
            }
            let n = usize::try_from(resolve_int(dict_get(d, "/N")?, objects)?).ok()?;
            let first = usize::try_from(resolve_int(dict_get(d, "/First")?, objects)?).ok()?;
            Some((id, n, first, obj.stream_range, obj.order_seq))
        })
        .collect();
    obj_stms.sort_by_key(|x| x.4);

    for (id, n, first, stream_range, _) in obj_stms {
        let stream_slice: &[u8] = if let Some(decoded) = flate_cache.get(&id) {
            decoded.as_slice()
        } else if let Some((s, e)) = stream_range {
            &bytes[s..e]
        } else {
            continue;
        };
        if n > MAX_PDF_OBJSTM_ENTRIES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "PDF ObjStm 엔트리 수({n})가 상한({MAX_PDF_OBJSTM_ENTRIES})을 초과했습니다."
            )));
        }
        if first > stream_slice.len() {
            continue;
        }
        let header_str = String::from_utf8_lossy(&stream_slice[..first]);
        let need = n.saturating_mul(2);
        let nums: Vec<usize> = header_str
            .split_ascii_whitespace()
            .filter_map(|tok| tok.parse::<usize>().ok())
            .take(need)
            .collect();
        if nums.len() < need {
            continue;
        }
        for i in 0..n {
            let emb_obj_num = nums[i * 2] as u32;
            let rel_off = nums[i * 2 + 1];
            let obj_start = first.saturating_add(rel_off);
            if obj_start >= stream_slice.len() {
                continue;
            }
            if let Ok(Some((val, _))) =
                parse_pdf_value_with_table(stream_slice, None, obj_start, stream_slice.len(), 0)
            {
                let key = (emb_obj_num, 0);
                if !objects.contains_key(&key) {
                    if objects.len() >= MAX_PDF_INDIRECT_OBJECTS {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                            "PDF 간접 객체 수가 상한({MAX_PDF_INDIRECT_OBJECTS})을 초과했습니다."
                        )));
                    }
                    *next_order_seq += 1;
                    objects.insert(
                        key,
                        PdfIndirectObject {
                            val,
                            stream_range: None,
                            order_seq: *next_order_seq,
                        },
                    );
                }
            }
        }
    }
    Ok(())
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
        has_pdf_comment_with_prefix(stream_bytes, MODEL_B64_COMMENT_PREFIX, true)
    } else {
        contains_subslice(stream_bytes, MODEL_B64_COMMENT_PREFIX)
    }
}

/// Applies PDF `/DecodeParms` `/Predictor` (`2` TIFF horizontal differencing and `10..=15` PNG
/// row filters None=0, Sub=1, Up=2, Average=3, Paeth=4).
/// Adapted from MIT and BSD-3-Clause reference implementations (see `THIRD_PARTY_NOTICES.md`) with
/// fail-closed bounds checking (`1..=65536` columns).
pub(super) fn apply_flate_decode_parms_at(
    mut data: Vec<u8>,
    entries: &[(String, PdfObjValue)],
    stage_idx: usize,
    filter_count: usize,
    objects: &HashMap<(u32, u16), PdfIndirectObject>,
) -> Result<Vec<u8>, &'static str> {
    let Some(raw_parms) = dict_get(entries, "/DecodeParms") else {
        return Ok(data);
    };
    let Some(parms_val) = resolve_value(raw_parms, objects) else {
        return Err("PDF 스트림 /DecodeParms 간접 참조를 해석할 수 없습니다.");
    };
    let parms = match parms_val {
        PdfObjValue::Null => return Ok(data),
        PdfObjValue::Dict(d) => d,
        PdfObjValue::Array(arr) if arr.is_empty() => return Ok(data),
        PdfObjValue::Array(arr) if arr.len() == filter_count => {
            match resolve_value(&arr[stage_idx], objects) {
                Some(PdfObjValue::Null) => return Ok(data),
                Some(PdfObjValue::Dict(d)) => d,
                _ => return Err("PDF 스트림 /DecodeParms 배열 원소가 올바르지 않습니다."),
            }
        }
        _ => return Err("PDF 스트림 /DecodeParms 형식이 올바르지 않습니다."),
    };
    let predictor = match dict_get(parms, "/Predictor") {
        Some(v) => resolve_int(v, objects).ok_or("PDF /Predictor 값이 정수가 아닙니다.")?,
        None => 1,
    };
    if predictor == 1 {
        return Ok(data);
    }
    let columns = match dict_get(parms, "/Columns") {
        Some(v) => resolve_int(v, objects).ok_or("PDF /Columns 값이 정수가 아닙니다.")?,
        None => 1,
    };
    if !(1..=65536).contains(&columns) {
        return Err("PDF /Columns 값이 허용 범위(1..=65536)를 벗어났습니다.");
    }
    let colors = match dict_get(parms, "/Colors") {
        Some(v) => resolve_int(v, objects).ok_or("PDF /Colors 값이 정수가 아닙니다.")?,
        None => 1,
    };
    if !(1..=16).contains(&colors) {
        return Err("PDF /Colors 값이 허용 범위(1..=16)를 벗어났습니다.");
    }
    let bpc = match dict_get(parms, "/BitsPerComponent") {
        Some(v) => resolve_int(v, objects).ok_or("PDF /BitsPerComponent 값이 정수가 아닙니다.")?,
        None => 8,
    };
    if !matches!(bpc, 1 | 2 | 4 | 8 | 16) {
        return Err("PDF /BitsPerComponent 값이 허용되지 않습니다.");
    }
    let cols_u = columns as usize;
    let colors_u = colors as usize;
    let bpc_u = bpc as usize;
    let bpp = (colors_u * bpc_u).div_ceil(8).max(1);
    let row_bytes = cols_u
        .checked_mul(colors_u)
        .and_then(|v| v.checked_mul(bpc_u))
        .map(|bits| bits.div_ceil(8))
        .ok_or("PDF /Predictor 행 크기가 오버플로우되었습니다.")?;
    if row_bytes == 0 {
        return Err("PDF /Predictor 행 크기가 0입니다.");
    }
    if data.is_empty() {
        return Ok(data);
    }
    match predictor {
        2 => {
            if bpc_u != 8 || data.len() % row_bytes != 0 {
                return Err("PDF TIFF Predictor 2 스트림 길이가 행 크기의 배수가 아닙니다.");
            }
            for row in data.chunks_exact_mut(row_bytes) {
                for i in bpp..row_bytes {
                    row[i] = row[i].wrapping_add(row[i - bpp]);
                }
            }
            Ok(data)
        }
        10..=15 => {
            let stride = row_bytes
                .checked_add(1)
                .ok_or("PDF PNG Predictor stride 오버플로우입니다.")?;
            if data.len() % stride != 0 {
                return Err("PDF PNG Predictor 스트림 길이가 stride의 배수가 아닙니다.");
            }
            let mut out = vec![0u8; (data.len() / stride) * row_bytes];
            for (r, src_chunk) in data.chunks_exact(stride).enumerate() {
                let (filter, raw) = (src_chunk[0], &src_chunk[1..]);
                if filter > 4 {
                    return Err("지원하지 않는 PDF PNG Predictor 행 필터입니다.");
                }
                let (prev_rows, cur_and_rest) = out.split_at_mut(r * row_bytes);
                let cur = &mut cur_and_rest[..row_bytes];
                let prev = r
                    .checked_sub(1)
                    .map(|p| &prev_rows[p * row_bytes..(p + 1) * row_bytes]);
                for i in 0..row_bytes {
                    let left = if i >= bpp { cur[i - bpp] } else { 0 };
                    let up = prev.map_or(0, |p| p[i]);
                    let up_left = if i >= bpp {
                        prev.map_or(0, |p| p[i - bpp])
                    } else {
                        0
                    };
                    let pred = match filter {
                        0 => 0,
                        1 => left,
                        2 => up,
                        3 => ((u16::from(left) + u16::from(up)) >> 1) as u8,
                        _ => paeth_predict_byte(left, up, up_left),
                    };
                    cur[i] = raw[i].wrapping_add(pred);
                }
            }
            Ok(out)
        }
        _ => Err("지원하지 않는 PDF /Predictor 값입니다."),
    }
}

#[inline]
fn paeth_predict_byte(a: u8, b: u8, c: u8) -> u8 {
    let (ia, ib, ic) = (i16::from(a), i16::from(b), i16::from(c));
    let p = ia + ib - ic;
    let (pa, pb, pc) = ((p - ia).abs(), (p - ib).abs(), (p - ic).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
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
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            x.bytes()
                .filter(|b| !b.is_ascii_whitespace())
                .eq(y.bytes().filter(|b| !b.is_ascii_whitespace()))
        })
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
    let normalized_json_title: String = json_title
        .chars()
        .map(|c| {
            if matches!(
                c,
                '\t' | '\r' | '\n' | '\x0B' | '\x0C' | '\u{0085}' | '\u{2028}' | '\u{2029}'
            ) {
                ' '
            } else {
                c
            }
        })
        .collect();
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
            if !rb_str
                .bytes()
                .filter(|b| !b.is_ascii_whitespace())
                .eq(json_page_texts
                    .iter()
                    .flat_map(|s| s.bytes())
                    .filter(|b| !b.is_ascii_whitespace()))
            {
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

#[cfg(test)]
#[path = "pdf_object_graph/tests.rs"]
mod tests;

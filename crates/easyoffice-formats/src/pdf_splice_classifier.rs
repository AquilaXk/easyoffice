use super::{dict_get, dict_has_non_null_key, dict_name_eq, PdfObjValue};
use crate::pdf_stream_parser::{
    decode_pdf_name, has_structural_after_endobj_cached, is_line_eof_marker, is_pdf_delim_or_ws,
    is_standalone_pdf_kw, is_structural_endobj_at, is_valid_xref_follow, match_indirect_obj_header,
    skip_pdf_ws_stopping_at_eof,
};

#[derive(Clone, Copy)]
pub(super) struct ParenSpliceClass {
    pub(super) cross_obj: bool,
    pub(super) any_splice: bool,
}

#[derive(Clone, Copy, Default)]
struct ParenScanFrame {
    dict_depth: i32,
    has_obj_or_trailer_dict: bool,
    opened_after_key: bool,
    key_bracket_depth: i32,
}

pub(super) fn is_pdf_dict_open_follow(raw: &[u8], after_open: usize) -> bool {
    let len = raw.len();
    let q = skip_pdf_ws_stopping_at_eof(raw, after_open, len);
    (q + 1 < len && raw[q] == b'/' && !is_pdf_delim_or_ws(raw[q + 1]))
        || (q + 1 < len && raw[q] == b'>' && raw[q + 1] == b'>')
}

fn is_same_line_dict_stream(raw: &[u8], after_close: usize) -> bool {
    let len = raw.len();
    let mut s = after_close;
    while s < len && matches!(raw[s], b' ' | b'\t') {
        s += 1;
    }
    if !is_standalone_pdf_kw(raw, s, b"stream") {
        return false;
    }
    let mut a = s + 6;
    while a < len && matches!(raw[a], b' ' | b'\t') {
        a += 1;
    }
    a >= len || matches!(raw[a], b'\r' | b'\n')
}

pub(super) fn classify_raw_paren_splice(raw: &[u8]) -> ParenSpliceClass {
    let len = raw.len();
    let mut frames = vec![ParenScanFrame::default()];
    let mut cross_obj = false;
    let mut dict_splice = false;
    let mut name_splice = false;
    let mut seen_startxref = false;
    let mut next_obj_or_trailer_dict_at: Option<usize> = None;
    let mut key_paren_open_at: Option<usize> = None;
    let mut key_bracket_open_at: Option<usize> = None;
    let mut endobj_cache = (0usize, 0usize);
    let mut i = 0usize;
    while i < len {
        if raw[i] == b'\\' {
            i = (i + 2).min(len);
            continue;
        }
        if is_structural_endobj_at(raw, i, &mut endobj_cache) {
            cross_obj = true;
        }
        if is_standalone_pdf_kw(raw, i, b"trailer") {
            let qt = skip_pdf_ws_stopping_at_eof(raw, i + 7, len);
            if qt + 1 < len && raw[qt] == b'<' && raw[qt + 1] == b'<' {
                next_obj_or_trailer_dict_at = Some(qt);
            }
        }
        if i > 0 && matches!(raw[i - 1], b'\n' | b'\r') && raw[i] != b'\n' {
            let mut t = i;
            while t < len && matches!(raw[t], b' ' | b'\t') {
                t += 1;
            }
            if is_line_eof_marker(raw, t)
                && (seen_startxref
                    || has_structural_after_endobj_cached(raw, t + 5, &mut endobj_cache))
            {
                cross_obj = true;
            }
            if is_standalone_pdf_kw(raw, t, b"stream")
                && (t + 6 >= len || matches!(raw[t + 6], b'\r' | b'\n' | b' ' | b'\t'))
            {
                cross_obj = true;
            }
            if is_valid_xref_follow(raw, t) {
                cross_obj = true;
            }
            if is_standalone_pdf_kw(raw, t, b"startxref") {
                let qs = skip_pdf_ws_stopping_at_eof(raw, t + 9, len);
                if qs > t + 9 && qs < len && raw[qs].is_ascii_digit() {
                    seen_startxref = true;
                    cross_obj = true;
                }
            }
            if let Some(after_hdr) = match_indirect_obj_header(raw, t, len) {
                let qo = skip_pdf_ws_stopping_at_eof(raw, after_hdr, len);
                if qo + 1 < len && raw[qo] == b'<' && raw[qo + 1] == b'<' {
                    next_obj_or_trailer_dict_at = Some(qo);
                }
            }
        }
        match raw[i] {
            b'(' => {
                let opened_after_key = frames.len() == 1 && key_paren_open_at == Some(i);
                frames.push(ParenScanFrame {
                    opened_after_key,
                    ..Default::default()
                });
            }
            b')' => {
                if frames.len() <= 1 {
                    return ParenSpliceClass {
                        cross_obj: true,
                        any_splice: true,
                    };
                }
                let popped = frames.pop().unwrap_or_default();
                if popped.dict_depth > 0 {
                    dict_splice = true;
                    if popped.has_obj_or_trailer_dict {
                        cross_obj = true;
                    }
                }
                if popped.opened_after_key
                    && frames.len() == 1
                    && skip_pdf_ws_stopping_at_eof(raw, i + 1, len) >= len
                {
                    name_splice = true;
                }
            }
            b'<' if i + 1 < len && raw[i + 1] == b'<' => {
                if is_pdf_dict_open_follow(raw, i + 2) {
                    if let Some(frame) = frames.last_mut() {
                        frame.dict_depth += 1;
                        if next_obj_or_trailer_dict_at == Some(i) {
                            frame.has_obj_or_trailer_dict = true;
                        }
                    }
                }
                i += 2;
                continue;
            }
            b'>' if i + 1 < len && raw[i + 1] == b'>' => {
                if is_same_line_dict_stream(raw, i + 2) {
                    cross_obj = true;
                }
                if let Some(frame) = frames.last_mut() {
                    if frame.dict_depth > 0 {
                        frame.dict_depth -= 1;
                        if frame.dict_depth == 0 {
                            frame.has_obj_or_trailer_dict = false;
                        }
                    } else if is_pdf_dict_open_follow(raw, i + 2) {
                        dict_splice = true;
                    }
                }
                i += 2;
                continue;
            }
            b'[' if frames.len() == 1 => {
                if key_bracket_open_at == Some(i) || frames[0].key_bracket_depth > 0 {
                    frames[0].key_bracket_depth += 1;
                }
            }
            b']' if frames.len() == 1 && frames[0].key_bracket_depth > 0 => {
                frames[0].key_bracket_depth -= 1;
            }
            b'/' if frames.len() == 1
                && frames[0].dict_depth == 0
                && (i == 0 || (is_pdf_delim_or_ws(raw[i - 1]) && raw[i - 1] != b'/')) =>
            {
                if let Some(after_entry) = is_encrypt_entry_at(raw, i, len)
                    .or_else(|| is_type_xref_entry_end_at(raw, i, len))
                {
                    let next = skip_pdf_ws_stopping_at_eof(raw, after_entry, len);
                    if next + 1 < len && raw[next] == b'/' && !is_pdf_delim_or_ws(raw[next + 1]) {
                        name_splice = true;
                    }
                }
                let mut k = i + 1;
                while k < len && !is_pdf_delim_or_ws(raw[k]) {
                    k += 1;
                }
                if k > i + 1 {
                    let q = skip_pdf_ws_stopping_at_eof(raw, k, len);
                    if q < len && raw[q] == b'(' {
                        key_paren_open_at = Some(q);
                    } else if q < len && raw[q] == b'[' {
                        key_bracket_open_at = Some(q);
                    }
                }
                i = k;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    let outer = frames[0];
    if outer.dict_depth > 0 {
        dict_splice = true;
        if outer.has_obj_or_trailer_dict {
            cross_obj = true;
        }
    }
    if outer.key_bracket_depth > 0 {
        name_splice = true;
    }
    let cross = cross_obj || frames.len() != 1;
    ParenSpliceClass {
        cross_obj: cross,
        any_splice: cross || dict_splice || name_splice,
    }
}

pub(super) fn bytes_contain_pdf_name(haystack: &[u8], expected: &str) -> bool {
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

pub(super) fn contains_trailer_dict_kw(haystack: &[u8]) -> bool {
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

fn is_type_xref_entry_end_at(bytes: &[u8], i: usize, limit: usize) -> Option<usize> {
    let end = limit.min(bytes.len());
    if i >= end || bytes[i] != b'/' {
        return None;
    }
    let mut k = i + 1;
    while k < end && !is_pdf_delim_or_ws(bytes[k]) {
        k += 1;
    }
    if k == i + 1 || decode_pdf_name(&bytes[i + 1..k]) != "/Type" {
        return None;
    }
    let q = skip_pdf_ws_stopping_at_eof(bytes, k, end);
    if q >= end || bytes[q] != b'/' {
        return None;
    }
    let mut m = q + 1;
    while m < end && !is_pdf_delim_or_ws(bytes[m]) {
        m += 1;
    }
    (m > q + 1 && decode_pdf_name(&bytes[q + 1..m]) == "/XRef").then_some(m)
}

pub(super) fn is_type_xref_entry_at(bytes: &[u8], i: usize, limit: usize) -> bool {
    is_type_xref_entry_end_at(bytes, i, limit).is_some()
}

pub(super) fn is_encrypt_entry_at(bytes: &[u8], i: usize, limit: usize) -> Option<usize> {
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
                return Some(q3 + 1);
            }
        }
    }
    None
}

pub(super) fn bytes_contain_encrypt_entry(haystack: &[u8]) -> bool {
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

#[derive(Clone, Copy, Default)]
pub(super) struct SplicedFlags {
    pub(super) any_splice: bool,
    pub(super) encrypt: bool,
    pub(super) xref_or_trailer: bool,
    pub(super) both_in_one: bool,
}

impl SplicedFlags {
    fn merge(&mut self, other: Self) {
        self.any_splice |= other.any_splice;
        self.encrypt |= other.encrypt;
        self.xref_or_trailer |= other.xref_or_trailer;
        self.both_in_one |= other.both_in_one;
    }
}

pub(super) fn summarize_spliced_value(val: &PdfObjValue) -> SplicedFlags {
    match val {
        PdfObjValue::ParenStr(raw) => {
            if !classify_raw_paren_splice(raw).any_splice {
                return SplicedFlags::default();
            }
            let encrypt = bytes_contain_encrypt_entry(raw);
            let xref_or_trailer =
                bytes_contain_pdf_name(raw, "/XRef") || contains_trailer_dict_kw(raw);
            SplicedFlags {
                any_splice: true,
                encrypt,
                xref_or_trailer,
                both_in_one: encrypt && xref_or_trailer,
            }
        }
        PdfObjValue::Array(items) => {
            let mut out = SplicedFlags::default();
            for item in items {
                out.merge(summarize_spliced_value(item));
            }
            out
        }
        PdfObjValue::Dict(entries) => {
            let mut out = SplicedFlags::default();
            for (_, v) in entries {
                out.merge(summarize_spliced_value(v));
            }
            if out.any_splice {
                if dict_has_non_null_key(entries, "/Encrypt") {
                    out.encrypt = true;
                }
                if dict_name_eq(entries, "/Type", "/XRef") {
                    out.xref_or_trailer = true;
                }
                if out.encrypt && out.xref_or_trailer {
                    out.both_in_one = true;
                }
            }
            out
        }
        _ => SplicedFlags::default(),
    }
}

pub(in crate::text_pdf_codecs) fn trailer_or_xref_has_spliced_encrypt(
    entries: &[(String, PdfObjValue)],
    has_encrypt: bool,
) -> Option<bool> {
    if entries
        .iter()
        .any(|(_, v)| summarize_spliced_value(v).encrypt)
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
    let summary = summarize_spliced_value(val);
    if summary.both_in_one {
        return Some(true);
    }
    if let PdfObjValue::Dict(entries) = val {
        if dict_has_non_null_key(entries, "/Encrypt") && summary.xref_or_trailer {
            return Some(true);
        }
        if dict_name_eq(entries, "/Type", "/XRef") {
            let has_enc = dict_has_non_null_key(entries, "/Encrypt");
            return trailer_or_xref_has_spliced_encrypt(entries, has_enc);
        }
    }
    None
}

use super::{dict_get, dict_has_non_null_key, dict_name_eq, PdfObjValue};
use crate::pdf_stream_parser::{
    decode_pdf_name, has_structural_after_endobj_cached, is_line_eof_marker, is_pdf_delim_or_ws,
    is_pdf_obj_body_start, is_pdf_ws, is_preceded_by_eol_ws, is_standalone_pdf_kw, is_stream_kw_at,
    is_structural_endobj_at, is_trailer_dict_at, is_valid_startxref_at, is_valid_xref_follow,
    match_indirect_obj_header, skip_pdf_ws_stopping_at_eof,
};

#[derive(Clone, Copy)]
pub(super) struct ParenSpliceClass {
    pub(super) cross_obj: bool,
    pub(super) any_splice: bool,
}

#[derive(Clone, Copy, Default)]
struct ParenScanFrame {
    dict_depth: i32,
    key_bracket_depth: i32,
    obj_bracket_depth: i32,
    has_obj_or_trailer_dict: bool,
    line_start_obj_or_trailer_dict: bool,
    dict_opened_for_key: bool,
    paren_opened_for_key: bool,
    expect_key_val: bool,
    after_pdf_kv: bool,
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

fn step_scalar_or_ref_value(raw: &[u8], i: usize, len: usize) -> Option<usize> {
    for kw in [b"true".as_slice(), b"false".as_slice(), b"null".as_slice()] {
        if is_standalone_pdf_kw(&raw[..len], i, kw) {
            return Some(i + kw.len());
        }
    }
    let mut d = i;
    if d < len && matches!(raw[d], b'+' | b'-') {
        d += 1;
    }
    let digit_start = d;
    let mut seen_dot = false;
    let mut seen_digit = false;
    while d < len && !is_pdf_delim_or_ws(raw[d]) {
        if raw[d].is_ascii_digit() {
            seen_digit = true;
        } else if raw[d] == b'.' && !seen_dot {
            seen_dot = true;
        } else {
            return None;
        }
        d += 1;
    }
    if !seen_digit {
        return None;
    }
    if digit_start == i && !seen_dot {
        let q2 = skip_pdf_ws_stopping_at_eof(raw, d, len);
        if q2 > d && q2 < len && raw[q2].is_ascii_digit() {
            let mut d2 = q2;
            while d2 < len && raw[d2].is_ascii_digit() {
                d2 += 1;
            }
            let q3 = skip_pdf_ws_stopping_at_eof(raw, d2, len);
            if q3 > d2 && is_standalone_pdf_kw(&raw[..len], q3, b"R") {
                return Some(q3 + 1);
            }
        }
    }
    Some(d)
}

fn is_plain_comment_line(raw: &[u8], i: usize, len: usize) -> Option<usize> {
    if raw[i] != b'%' || is_line_eof_marker(raw, i) {
        return None;
    }
    let mut j = i + 1;
    while j < len && raw[j] != b'\n' && raw[j] != b'\r' {
        if matches!(raw[j], b'(' | b')' | b'\\') {
            return None;
        }
        j += 1;
    }
    Some(j)
}

fn apply_unclosed_frame_flags(
    frame: ParenScanFrame,
    cross_obj: &mut bool,
    dict_splice: &mut bool,
    name_splice: &mut bool,
) {
    if frame.dict_depth > 0 {
        *dict_splice = true;
        if frame.has_obj_or_trailer_dict {
            *cross_obj = true;
        }
    }
    if frame.key_bracket_depth > 0 {
        *name_splice = true;
    }
    if frame.obj_bracket_depth > 0 {
        *cross_obj = true;
    }
}

pub(super) fn classify_raw_paren_splice(raw: &[u8]) -> ParenSpliceClass {
    let len = raw.len();
    let mut frames = vec![ParenScanFrame::default()];
    let mut cross_obj = false;
    let mut dict_splice = false;
    let mut name_splice = false;
    let mut seen_startxref = false;
    let mut seen_line_start_obj = false;
    let mut next_obj_or_trailer_dict_at: Option<(usize, bool)> = None;
    let mut next_obj_array_at: Option<usize> = None;
    let mut endobj_cache = (0usize, 0usize);
    let mut i = 0usize;
    while i < len {
        if raw[i] == b'\\' {
            i = (i + 2).min(len);
            continue;
        }
        if is_structural_endobj_at(raw, i, &mut endobj_cache)
            || (seen_line_start_obj && is_standalone_pdf_kw(raw, i, b"endobj"))
        {
            cross_obj = true;
        }
        if let Some(qt) = is_trailer_dict_at(raw, i, len) {
            next_obj_or_trailer_dict_at = Some((qt, is_preceded_by_eol_ws(raw, i)));
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
            if is_stream_kw_at(raw, t, len) || is_valid_xref_follow(raw, t) {
                cross_obj = true;
            }
            if is_valid_startxref_at(raw, t, len) {
                seen_startxref = true;
                cross_obj = true;
            }
            if let Some(after_hdr) = match_indirect_obj_header(raw, t, len) {
                if let Some(qo) = is_pdf_obj_body_start(raw, after_hdr, len) {
                    seen_line_start_obj = true;
                    if qo + 1 < len && raw[qo] == b'<' && raw[qo + 1] == b'<' {
                        next_obj_or_trailer_dict_at = Some((qo, true));
                    } else if raw[qo] == b'[' {
                        next_obj_array_at = Some(qo);
                    }
                }
            }
        }
        if is_pdf_ws(raw[i]) {
            i += 1;
            continue;
        }
        match raw[i] {
            b'(' => {
                let opened_for_key = frames.last().is_some_and(|f| {
                    f.dict_depth == 0
                        && f.key_bracket_depth == 0
                        && f.obj_bracket_depth == 0
                        && f.expect_key_val
                });
                if let Some(parent) = frames.last_mut() {
                    parent.expect_key_val = false;
                    parent.after_pdf_kv = false;
                }
                frames.push(ParenScanFrame {
                    paren_opened_for_key: opened_for_key,
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
                apply_unclosed_frame_flags(
                    popped,
                    &mut cross_obj,
                    &mut dict_splice,
                    &mut name_splice,
                );
                if popped.paren_opened_for_key {
                    if let Some(parent) = frames.last_mut() {
                        parent.expect_key_val = false;
                        parent.after_pdf_kv = true;
                    }
                }
            }
            b'<' if i + 1 < len && raw[i + 1] == b'<' => {
                if is_pdf_dict_open_follow(raw, i + 2) {
                    if let Some(frame) = frames.last_mut() {
                        if frame.dict_depth == 0 {
                            frame.dict_opened_for_key = frame.key_bracket_depth == 0
                                && frame.obj_bracket_depth == 0
                                && frame.expect_key_val;
                            frame.expect_key_val = false;
                            frame.after_pdf_kv = false;
                        }
                        frame.dict_depth += 1;
                        if let Some((dict_at, at_line_start)) = next_obj_or_trailer_dict_at {
                            if dict_at == i {
                                frame.has_obj_or_trailer_dict = true;
                                if at_line_start && frame.dict_depth == 1 {
                                    frame.line_start_obj_or_trailer_dict = true;
                                }
                            }
                        }
                    }
                } else if let Some(frame) = frames.last_mut() {
                    frame.expect_key_val = false;
                    frame.after_pdf_kv = false;
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
                            if frame.line_start_obj_or_trailer_dict {
                                let after_close = skip_pdf_ws_stopping_at_eof(raw, i + 2, len);
                                if after_close < len && raw[after_close] != b')' {
                                    cross_obj = true;
                                }
                            }
                            frame.has_obj_or_trailer_dict = false;
                            frame.line_start_obj_or_trailer_dict = false;
                            if frame.dict_opened_for_key {
                                frame.after_pdf_kv = true;
                                frame.dict_opened_for_key = false;
                            }
                        }
                    } else {
                        frame.expect_key_val = false;
                        frame.after_pdf_kv = false;
                        if is_pdf_dict_open_follow(raw, i + 2) {
                            dict_splice = true;
                        }
                    }
                }
                i += 2;
                continue;
            }
            b'<' => {
                if let Some(frame) = frames.last_mut() {
                    if frame.dict_depth == 0
                        && frame.key_bracket_depth == 0
                        && frame.obj_bracket_depth == 0
                    {
                        if frame.expect_key_val {
                            let mut h = i + 1;
                            while h < len
                                && raw[h] != b'>'
                                && (raw[h].is_ascii_hexdigit() || is_pdf_ws(raw[h]))
                            {
                                h += 1;
                            }
                            if h < len && raw[h] == b'>' {
                                frame.expect_key_val = false;
                                frame.after_pdf_kv = true;
                                i = h + 1;
                                continue;
                            }
                        }
                        frame.expect_key_val = false;
                        frame.after_pdf_kv = false;
                    }
                }
            }
            b'[' => {
                if let Some(frame) = frames.last_mut() {
                    if frame.dict_depth == 0 {
                        if frame.key_bracket_depth > 0 || frame.expect_key_val {
                            frame.expect_key_val = false;
                            frame.after_pdf_kv = false;
                            frame.key_bracket_depth += 1;
                        } else if frame.obj_bracket_depth > 0 || next_obj_array_at == Some(i) {
                            frame.expect_key_val = false;
                            frame.after_pdf_kv = false;
                            frame.obj_bracket_depth += 1;
                        } else {
                            frame.expect_key_val = false;
                            frame.after_pdf_kv = false;
                        }
                    }
                }
            }
            b']' => {
                if let Some(frame) = frames.last_mut() {
                    if frame.dict_depth == 0 {
                        if frame.key_bracket_depth > 0 {
                            frame.key_bracket_depth -= 1;
                            if frame.key_bracket_depth == 0 {
                                frame.after_pdf_kv = true;
                            }
                        } else if frame.obj_bracket_depth > 0 {
                            frame.obj_bracket_depth -= 1;
                            if frame.obj_bracket_depth == 0 {
                                let after_arr = skip_pdf_ws_stopping_at_eof(raw, i + 1, len);
                                if after_arr < len && raw[after_arr] != b')' {
                                    cross_obj = true;
                                }
                            }
                        } else {
                            frame.expect_key_val = false;
                            frame.after_pdf_kv = false;
                            if is_pdf_dict_open_follow(raw, i + 1) {
                                dict_splice = true;
                            }
                        }
                    }
                }
            }
            b'/' if i == 0 || (is_pdf_delim_or_ws(raw[i - 1]) && raw[i - 1] != b'/') => {
                let mut k = i + 1;
                while k < len && !is_pdf_delim_or_ws(raw[k]) {
                    k += 1;
                }
                let is_nested = frames.len() > 1;
                if let Some(frame) = frames.last_mut() {
                    if frame.dict_depth == 0
                        && frame.key_bracket_depth == 0
                        && frame.obj_bracket_depth == 0
                    {
                        if let Some(after_entry) = is_encrypt_entry_at(raw, i, len)
                            .or_else(|| is_type_xref_entry_end_at(raw, i, len))
                        {
                            let next = skip_pdf_ws_stopping_at_eof(raw, after_entry, len);
                            let next_is_key = next + 1 < len
                                && raw[next] == b'/'
                                && !is_pdf_delim_or_ws(raw[next + 1]);
                            let next_is_close =
                                next + 1 < len && raw[next] == b'>' && raw[next + 1] == b'>';
                            let at_frame_close = next >= len || (is_nested && raw[next] == b')');
                            if next_is_key
                                || next_is_close
                                || (at_frame_close && frame.after_pdf_kv && !frame.expect_key_val)
                            {
                                name_splice = true;
                            }
                        }
                        if k > i + 1 {
                            if frame.expect_key_val {
                                frame.expect_key_val = false;
                                frame.after_pdf_kv = true;
                            } else {
                                frame.expect_key_val = true;
                                frame.after_pdf_kv = false;
                            }
                        } else {
                            frame.expect_key_val = false;
                            frame.after_pdf_kv = false;
                        }
                        i = k;
                        continue;
                    }
                }
                i = k;
                continue;
            }
            _ => {
                if let Some(frame) = frames.last_mut() {
                    if frame.dict_depth == 0
                        && frame.key_bracket_depth == 0
                        && frame.obj_bracket_depth == 0
                    {
                        if frame.expect_key_val || frame.after_pdf_kv {
                            if let Some(eol) = is_plain_comment_line(raw, i, len) {
                                i = eol;
                                continue;
                            }
                        }
                        if frame.expect_key_val {
                            if let Some(after_val) = step_scalar_or_ref_value(raw, i, len) {
                                frame.expect_key_val = false;
                                frame.after_pdf_kv = true;
                                i = after_val;
                                continue;
                            }
                        }
                        frame.expect_key_val = false;
                        frame.after_pdf_kv = false;
                    }
                }
            }
        }
        i += 1;
    }
    apply_unclosed_frame_flags(
        frames[0],
        &mut cross_obj,
        &mut dict_splice,
        &mut name_splice,
    );
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
        if is_trailer_dict_at(haystack, i, haystack.len()).is_some() {
            return true;
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

fn skip_balanced_pdf_dict(bytes: &[u8], open_pos: usize, limit: usize) -> Option<usize> {
    let end = limit.min(bytes.len());
    if open_pos + 1 >= end
        || bytes[open_pos] != b'<'
        || bytes[open_pos + 1] != b'<'
        || !is_pdf_dict_open_follow(&bytes[..end], open_pos + 2)
    {
        return None;
    }
    let mut depth = 1usize;
    let mut p = open_pos + 2;
    while p < end {
        if bytes[p] == b'%' && !is_line_eof_marker(bytes, p) {
            while p < end && bytes[p] != b'\n' && bytes[p] != b'\r' {
                p += 1;
            }
            continue;
        }
        if bytes[p] == b'(' {
            let mut paren_depth = 1usize;
            let mut j = p + 1;
            while j < end && paren_depth > 0 {
                if bytes[j] == b'\\' {
                    j = (j + 2).min(end);
                    continue;
                }
                if bytes[j] == b'(' {
                    paren_depth += 1;
                } else if bytes[j] == b')' {
                    paren_depth -= 1;
                }
                j += 1;
            }
            if paren_depth != 0 {
                return None;
            }
            p = j;
            continue;
        }
        if p + 1 < end && bytes[p] == b'<' && bytes[p + 1] == b'<' {
            depth += 1;
            p += 2;
            continue;
        }
        if p + 1 < end && bytes[p] == b'>' && bytes[p + 1] == b'>' {
            depth -= 1;
            p += 2;
            if depth == 0 {
                return Some(p);
            }
            continue;
        }
        if bytes[p] == b'<' {
            let mut j = p + 1;
            while j < end && bytes[j] != b'>' {
                j += 1;
            }
            if j >= end {
                return None;
            }
            p = j + 1;
            continue;
        }
        p += 1;
    }
    None
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
    if q + 1 < end
        && bytes[q] == b'<'
        && bytes[q + 1] == b'<'
        && is_pdf_dict_open_follow(&bytes[..end], q + 2)
    {
        return Some(skip_balanced_pdf_dict(bytes, q, end).unwrap_or(end));
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
                let self_enc = dict_has_non_null_key(entries, "/Encrypt");
                let self_xref = dict_name_eq(entries, "/Type", "/XRef");
                out.encrypt |= self_enc;
                out.xref_or_trailer |= self_xref;
                if (self_enc || self_xref) && out.encrypt && out.xref_or_trailer {
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
        if dict_name_eq(entries, "/Type", "/XRef") {
            let has_enc = dict_has_non_null_key(entries, "/Encrypt");
            return trailer_or_xref_has_spliced_encrypt(entries, has_enc);
        }
    }
    None
}

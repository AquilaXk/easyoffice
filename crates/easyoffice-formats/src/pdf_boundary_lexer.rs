#[derive(Debug, Clone, Copy)]
pub(crate) enum PdfNonCodeKind {
    Comment {
        start: usize,
        end: usize,
    },
    ParenString {
        inner_start: usize,
        inner_end: usize,
    },
    DictOpen,
    DictClose,
    HexString {
        inner_start: usize,
        inner_end: usize,
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PdfNonCodeStep {
    pub(crate) next_index: usize,
    pub(crate) kind: PdfNonCodeKind,
}

pub(crate) fn is_pdf_ws(b: u8) -> bool {
    matches!(b, 0x00 | 0x09 | 0x0A | 0x0C | 0x0D | 0x20)
}

pub(crate) fn is_pdf_delim_or_ws(b: u8) -> bool {
    is_pdf_ws(b) || b"()<>[]{}/%".contains(&b)
}

pub(crate) fn decode_pdf_name(raw_after_slash: &[u8]) -> String {
    let mut out = Vec::with_capacity(raw_after_slash.len() + 1);
    out.push(b'/');
    let mut i = 0usize;
    while i < raw_after_slash.len() {
        if raw_after_slash[i] == b'#'
            && i + 2 < raw_after_slash.len()
            && raw_after_slash[i + 1].is_ascii_hexdigit()
            && raw_after_slash[i + 2].is_ascii_hexdigit()
        {
            if let Ok(hex_str) = std::str::from_utf8(&raw_after_slash[i + 1..i + 3]) {
                if let Ok(byte) = u8::from_str_radix(hex_str, 16) {
                    out.push(byte);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(raw_after_slash[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub(crate) fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack.len() >= needle.len()
        && haystack.windows(needle.len()).any(|w| w == needle)
}

pub(crate) fn is_standalone_pdf_kw(bytes: &[u8], pos: usize, kw: &[u8]) -> bool {
    pos + kw.len() <= bytes.len()
        && &bytes[pos..pos + kw.len()] == kw
        && (pos == 0 || is_pdf_delim_or_ws(bytes[pos - 1]))
        && (pos + kw.len() == bytes.len() || is_pdf_delim_or_ws(bytes[pos + kw.len()]))
}

pub(crate) fn skip_pdf_ws_only(bytes: &[u8], mut p: usize, limit: usize) -> usize {
    while p < limit && is_pdf_ws(bytes[p]) {
        p += 1;
    }
    p
}

pub(crate) fn skip_pdf_ws_and_comments(bytes: &[u8], mut p: usize, limit: usize) -> usize {
    while p < limit {
        if is_pdf_ws(bytes[p]) {
            p += 1;
        } else if bytes[p] == b'%' {
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

pub(crate) fn is_preceded_by_eol_ws(bytes: &[u8], q: usize) -> bool {
    let mut b = q;
    while b > 0 && matches!(bytes[b - 1], b' ' | b'\t') {
        b -= 1;
    }
    b == 0 || matches!(bytes[b - 1], b'\n' | b'\r')
}

pub(crate) fn is_preceded_by_dict_close_ws(bytes: &[u8], q: usize) -> bool {
    let mut b = q;
    while b > 0 && matches!(bytes[b - 1], b' ' | b'\t') {
        b -= 1;
    }
    b >= 2 && bytes[b - 2] == b'>' && bytes[b - 1] == b'>'
}

pub(crate) fn parse_indirect_obj_header(
    bytes: &[u8],
    pos: usize,
    limit: usize,
) -> Option<(u32, u16, usize)> {
    if pos >= limit
        || !bytes[pos].is_ascii_digit()
        || (pos > 0 && !is_pdf_delim_or_ws(bytes[pos - 1]))
    {
        return None;
    }
    let mut q = pos;
    while q < limit && bytes[q].is_ascii_digit() {
        q += 1;
    }
    let obj_num = std::str::from_utf8(&bytes[pos..q])
        .ok()?
        .parse::<u32>()
        .ok()?;
    let q_gen = skip_pdf_ws_only(bytes, q, limit);
    if q_gen == q || q_gen >= limit || !bytes[q_gen].is_ascii_digit() {
        return None;
    }
    let mut r = q_gen;
    while r < limit && bytes[r].is_ascii_digit() {
        r += 1;
    }
    let gen_num = std::str::from_utf8(&bytes[q_gen..r])
        .ok()?
        .parse::<u16>()
        .ok()?;
    let r_obj = skip_pdf_ws_only(bytes, r, limit);
    if r_obj == r || r_obj + 3 > limit || !is_standalone_pdf_kw(&bytes[..limit], r_obj, b"obj") {
        return None;
    }
    Some((obj_num, gen_num, r_obj + 3))
}

pub(crate) fn match_indirect_obj_header(bytes: &[u8], pos: usize, limit: usize) -> Option<usize> {
    parse_indirect_obj_header(bytes, pos, limit).map(|(_, _, end)| end)
}

pub(crate) fn is_line_eof_marker(bytes: &[u8], pos: usize) -> bool {
    if pos + 5 > bytes.len() || &bytes[pos..pos + 5] != b"%%EOF" {
        return false;
    }
    if pos + 5 < bytes.len() && !is_pdf_delim_or_ws(bytes[pos + 5]) {
        return false;
    }
    is_preceded_by_eol_ws(bytes, pos)
}

pub(crate) fn skip_pdf_ws_stopping_at_eof(bytes: &[u8], mut pos: usize, limit: usize) -> usize {
    let end = limit.min(bytes.len());
    while pos < end {
        if is_pdf_ws(bytes[pos]) {
            pos += 1;
        } else if bytes[pos] == b'%' {
            if is_line_eof_marker(bytes, pos) {
                break;
            }
            while pos < end && bytes[pos] != b'\n' && bytes[pos] != b'\r' {
                pos += 1;
            }
        } else {
            break;
        }
    }
    pos
}

pub(crate) fn is_trailer_dict_at(bytes: &[u8], pos: usize, limit: usize) -> Option<usize> {
    let end = limit.min(bytes.len());
    if !is_standalone_pdf_kw(&bytes[..end], pos, b"trailer") {
        return None;
    }
    let qt = skip_pdf_ws_stopping_at_eof(bytes, pos + 7, end);
    (qt + 1 < end && bytes[qt] == b'<' && bytes[qt + 1] == b'<').then_some(qt)
}

pub(crate) fn is_valid_startxref_at(bytes: &[u8], pos: usize, limit: usize) -> bool {
    let end = limit.min(bytes.len());
    if !is_standalone_pdf_kw(&bytes[..end], pos, b"startxref") {
        return false;
    }
    let qs = skip_pdf_ws_stopping_at_eof(bytes, pos + 9, end);
    qs > pos + 9 && qs < end && bytes[qs].is_ascii_digit()
}

pub(crate) fn is_stream_kw_at(bytes: &[u8], pos: usize, limit: usize) -> bool {
    let end = limit.min(bytes.len());
    is_standalone_pdf_kw(&bytes[..end], pos, b"stream")
        && (pos + 6 >= end || matches!(bytes[pos + 6], b'\r' | b'\n' | b' ' | b'\t'))
}

pub(crate) fn is_pdf_obj_body_start(bytes: &[u8], after_hdr: usize, limit: usize) -> Option<usize> {
    let end = limit.min(bytes.len());
    let qo = skip_pdf_ws_stopping_at_eof(bytes, after_hdr, end);
    if qo >= end {
        return None;
    }
    let ok = match bytes[qo] {
        b'<' | b'[' | b'(' => true,
        b'/' => qo + 1 < end && !is_pdf_delim_or_ws(bytes[qo + 1]),
        b'0'..=b'9' => true,
        b'+' | b'-' | b'.' => {
            qo + 1 < end && (bytes[qo + 1].is_ascii_digit() || bytes[qo + 1] == b'.')
        }
        _ => {
            is_standalone_pdf_kw(&bytes[..end], qo, b"true")
                || is_standalone_pdf_kw(&bytes[..end], qo, b"false")
                || is_standalone_pdf_kw(&bytes[..end], qo, b"null")
        }
    };
    ok.then_some(qo)
}

pub(crate) fn has_structural_after_endobj_cached(
    slice: &[u8],
    after_endobj: usize,
    cache: &mut (usize, usize),
) -> bool {
    let len = slice.len();
    if after_endobj >= len || (!is_pdf_ws(slice[after_endobj]) && slice[after_endobj] != b'%') {
        return false;
    }
    if cache.0 <= after_endobj && after_endobj < cache.1 {
        return false;
    }
    let q = skip_pdf_ws_and_comments(slice, after_endobj, len);
    if match_indirect_obj_header(slice, q, len).is_some() {
        return true;
    }
    if q + 4 <= len
        && is_standalone_pdf_kw(slice, q, b"xref")
        && (q + 4 == len || is_pdf_ws(slice[q + 4]) || slice[q + 4] == b'%')
    {
        return true;
    }
    if q + 7 < len && is_standalone_pdf_kw(slice, q, b"trailer") {
        let qt = skip_pdf_ws_and_comments(slice, q + 7, len);
        if qt + 1 < len && slice[qt] == b'<' && slice[qt + 1] == b'<' {
            return true;
        }
    }
    if q + 9 < len && is_standalone_pdf_kw(slice, q, b"startxref") {
        let qs = skip_pdf_ws_and_comments(slice, q + 9, len);
        if qs > q + 9 && qs < len && slice[qs].is_ascii_digit() {
            return true;
        }
    }
    *cache = (after_endobj, q.max(after_endobj + 1));
    false
}

pub(crate) fn advance_past_stream_eol(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'\r' {
        i += 1;
        if i < bytes.len() && bytes[i] == b'\n' {
            i += 1;
        }
    } else if i < bytes.len() && bytes[i] == b'\n' {
        i += 1;
    }
    i
}

pub(crate) fn step_pdf_non_code_token(
    bytes: &[u8],
    i: usize,
) -> Result<Option<PdfNonCodeStep>, ()> {
    match bytes[i] {
        b'%' => {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b'\n' && bytes[j] != b'\r' {
                j += 1;
            }
            Ok(Some(PdfNonCodeStep {
                next_index: j,
                kind: PdfNonCodeKind::Comment { start: i, end: j },
            }))
        }
        b'(' => {
            let mut depth = 1usize;
            let mut j = i + 1;
            while j < bytes.len() && depth > 0 {
                if bytes[j] == b'\\' {
                    j = (j + 2).min(bytes.len());
                    continue;
                }
                if bytes[j] == b'(' {
                    depth += 1;
                } else if bytes[j] == b')' {
                    depth -= 1;
                }
                j += 1;
            }
            if depth != 0 {
                Err(())
            } else {
                Ok(Some(PdfNonCodeStep {
                    next_index: j,
                    kind: PdfNonCodeKind::ParenString {
                        inner_start: i + 1,
                        inner_end: j - 1,
                    },
                }))
            }
        }
        b'<' if i + 1 < bytes.len() && bytes[i + 1] == b'<' => Ok(Some(PdfNonCodeStep {
            next_index: i + 2,
            kind: PdfNonCodeKind::DictOpen,
        })),
        b'>' if i + 1 < bytes.len() && bytes[i + 1] == b'>' => Ok(Some(PdfNonCodeStep {
            next_index: i + 2,
            kind: PdfNonCodeKind::DictClose,
        })),
        b'<' => {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b'>' {
                if !bytes[j].is_ascii_hexdigit() && !is_pdf_ws(bytes[j]) {
                    return Err(());
                }
                j += 1;
            }
            if j >= bytes.len() {
                Err(())
            } else {
                Ok(Some(PdfNonCodeStep {
                    next_index: j + 1,
                    kind: PdfNonCodeKind::HexString {
                        inner_start: i + 1,
                        inner_end: j,
                    },
                }))
            }
        }
        _ => Ok(None),
    }
}

pub(crate) fn validate_balanced_pdf_literals(stream_bytes: &[u8]) -> bool {
    let mut i = 0usize;
    while i < stream_bytes.len() {
        match step_pdf_non_code_token(stream_bytes, i) {
            Ok(Some(step)) => {
                i = step.next_index;
            }
            Ok(None) => i += 1,
            Err(()) => return false,
        }
    }
    true
}

pub(crate) fn span_crosses_object_boundary(bytes: &[u8], start: usize, end: usize) -> bool {
    if start >= end || end > bytes.len() {
        return false;
    }
    let slice = &bytes[start..end];
    let len = slice.len();
    let paren_table = ParenJumpTable::build(slice);
    let mut endobj_cache = (0usize, 0usize);
    let mut p = 0usize;
    while p < len {
        match step_indexed_non_code_token(slice, &paren_table, p, len) {
            Ok(Some(step)) => {
                p = step.next_index;
                continue;
            }
            Err(()) => {
                p += 1;
                continue;
            }
            Ok(None) => {}
        }
        if p + 9 <= len && is_standalone_pdf_kw(slice, p, b"endstream") {
            let q = skip_pdf_ws_and_comments(slice, p + 9, len);
            if (q + 6 <= len
                && is_standalone_pdf_kw(slice, q, b"endobj")
                && has_structural_after_endobj_cached(slice, q + 6, &mut endobj_cache))
                || match_indirect_obj_header(slice, q, len).is_some()
            {
                return true;
            }
        } else if p + 6 <= len
            && is_standalone_pdf_kw(slice, p, b"endobj")
            && has_structural_after_endobj_cached(slice, p + 6, &mut endobj_cache)
        {
            return true;
        }
        p += 1;
    }
    false
}

const MAX_PAREN_JUMP_DEPTH: usize = 4096;

pub(crate) fn is_structural_endobj_at(
    bytes: &[u8],
    i: usize,
    endobj_cache: &mut (usize, usize),
) -> bool {
    let len = bytes.len();
    if !is_standalone_pdf_kw(bytes, i, b"endobj") {
        return false;
    }
    let mut b = i;
    while b > 0 && matches!(bytes[b - 1], b' ' | b'\t') {
        b -= 1;
    }
    let preceded_structurally = b > 0
        && (matches!(bytes[b - 1], b'\n' | b'\r' | b']')
            || (b >= 2 && bytes[b - 2] == b'>' && bytes[b - 1] == b'>'));
    let mut a = i + 6;
    while a < len && matches!(bytes[a], b' ' | b'\t') {
        a += 1;
    }
    let followed_by_eol = a < len && matches!(bytes[a], b'\n' | b'\r' | b'%');
    (preceded_structurally || followed_by_eol)
        && has_structural_after_endobj_cached(bytes, i + 6, endobj_cache)
}

pub(crate) fn is_valid_xref_follow(bytes: &[u8], xref_pos: usize) -> bool {
    let len = bytes.len();
    if !is_standalone_pdf_kw(bytes, xref_pos, b"xref")
        || xref_pos + 4 >= len
        || (!is_pdf_ws(bytes[xref_pos + 4]) && bytes[xref_pos + 4] != b'%')
    {
        return false;
    }
    let qx = skip_pdf_ws_stopping_at_eof(bytes, xref_pos + 4, len);
    let mut qd = qx;
    while qd < len && bytes[qd].is_ascii_digit() {
        qd += 1;
    }
    let qd_ws = skip_pdf_ws_only(bytes, qd, len);
    is_line_eof_marker(bytes, qx)
        || is_standalone_pdf_kw(bytes, qx, b"trailer")
        || (qd > qx && qd_ws > qd && qd_ws < len && bytes[qd_ws].is_ascii_digit())
}

pub(crate) fn is_paren_reset_boundary(
    bytes: &[u8],
    i: usize,
    endobj_cache: &mut (usize, usize),
) -> Option<usize> {
    let len = bytes.len();
    if is_line_eof_marker(bytes, i) {
        let mut eol = i + 5;
        while eol < len && bytes[eol] != b'\n' && bytes[eol] != b'\r' {
            eol += 1;
        }
        return Some(eol);
    }
    if is_structural_endobj_at(bytes, i, endobj_cache) {
        return Some(i + 6);
    }
    if i > 0 && matches!(bytes[i - 1], b'\n' | b'\r') && bytes[i] != b'\n' {
        let mut line_tok = i;
        while line_tok < len && matches!(bytes[line_tok], b' ' | b'\t') {
            line_tok += 1;
        }
        if is_trailer_dict_at(bytes, line_tok, len).is_some()
            || is_valid_xref_follow(bytes, line_tok)
            || is_valid_startxref_at(bytes, line_tok, len)
        {
            return Some(line_tok);
        }
        if let Some((_, _, after_hdr)) = parse_indirect_obj_header(bytes, line_tok, len) {
            if is_pdf_obj_body_start(bytes, after_hdr, len).is_some() {
                return Some(line_tok);
            }
        }
    }
    None
}

pub(crate) struct ParenJumpTable {
    pairs: Vec<(u32, u32)>,
}

impl ParenJumpTable {
    pub(crate) fn build(bytes: &[u8]) -> Self {
        Self::build_inner(bytes, false)
    }

    pub(crate) fn build_damaged(bytes: &[u8]) -> Self {
        Self::build_inner(bytes, true)
    }

    fn build_inner(bytes: &[u8], reset_on_boundaries: bool) -> Self {
        let len = bytes.len();
        let mut pairs: Vec<(u32, u32)> = Vec::new();
        let mut stack: std::collections::VecDeque<u32> = std::collections::VecDeque::new();
        let mut endobj_cache = (0usize, 0usize);
        let mut i = 0usize;
        while i < len {
            if reset_on_boundaries && !stack.is_empty() {
                if let Some(next_i) = is_paren_reset_boundary(bytes, i, &mut endobj_cache) {
                    stack.clear();
                    if next_i > i {
                        i = next_i;
                        continue;
                    }
                }
            }
            let in_paren = !stack.is_empty();
            if !in_paren && bytes[i] == b'%' {
                while i < len && bytes[i] != b'\n' && bytes[i] != b'\r' {
                    i += 1;
                }
                continue;
            }
            if in_paren && bytes[i] == b'\\' {
                i = (i + 2).min(len);
                continue;
            }
            if bytes[i] == b'(' {
                if let Ok(pos32) = u32::try_from(i) {
                    if stack.len() == MAX_PAREN_JUMP_DEPTH {
                        stack.pop_front();
                    }
                    stack.push_back(pos32);
                }
            } else if bytes[i] == b')' {
                if let Some(open_pos) = stack.pop_back() {
                    if let Ok(end32) = u32::try_from(i + 1) {
                        pairs.push((open_pos, end32));
                    }
                }
            }
            i += 1;
        }
        pairs.sort_unstable_by_key(|&(open_pos, _)| open_pos);
        Self { pairs }
    }

    pub(crate) fn close_end(&self, open_pos: usize, limit: usize) -> Option<usize> {
        let open32 = u32::try_from(open_pos).ok()?;
        let idx = self.pairs.binary_search_by_key(&open32, |&(o, _)| o).ok()?;
        let end_pos = self.pairs[idx].1 as usize;
        (end_pos <= limit).then_some(end_pos)
    }
}

pub(crate) fn step_indexed_non_code_token(
    bytes: &[u8],
    table: &ParenJumpTable,
    i: usize,
    limit: usize,
) -> Result<Option<PdfNonCodeStep>, ()> {
    if i >= limit {
        return Ok(None);
    }
    if bytes[i] == b'(' {
        let next_index = table.close_end(i, limit).ok_or(())?;
        return Ok(Some(PdfNonCodeStep {
            next_index,
            kind: PdfNonCodeKind::ParenString {
                inner_start: i + 1,
                inner_end: next_index - 1,
            },
        }));
    }
    step_pdf_non_code_token(&bytes[..limit], i)
}

pub(crate) fn skip_damaged_truncated_stream(bytes: &[u8], stream_start: usize) -> usize {
    let mut endobj_cache = (0usize, 0usize);
    let mut q = stream_start;
    while q < bytes.len() {
        if is_standalone_pdf_kw(bytes, q, b"endobj")
            && (q + 6 >= bytes.len()
                || has_structural_after_endobj_cached(bytes, q + 6, &mut endobj_cache))
        {
            return q + 6;
        }
        q += 1;
    }
    bytes.len()
}

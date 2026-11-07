use super::pdf_splice_classifier::{
    bytes_contain_encrypt_entry, bytes_contain_pdf_name, classify_raw_paren_splice,
    contains_trailer_dict_kw, indirect_val_has_spliced_encrypt, is_encrypt_entry_at,
    is_pdf_dict_open_follow, is_type_xref_entry_at, summarize_spliced_value,
};
use super::{
    build_eof_and_int_index, dict_get, dict_has_non_null_key, dict_name_eq, lookup_indexed_int_obj,
    parse_pdf_value_with_table, resolve_stream_bounds, verify_direct_stream_bounds, IntObjIndex,
    PdfObjValue,
};
use crate::pdf_stream_parser::{
    advance_past_stream_eol, decode_pdf_name, has_structural_after_endobj_cached,
    is_line_eof_marker, is_pdf_delim_or_ws, is_pdf_ws, is_preceded_by_dict_close_ws,
    is_preceded_by_eol_ws, is_standalone_pdf_kw, is_stream_kw_at, is_structural_endobj_at,
    is_trailer_dict_at, is_valid_startxref_at, is_valid_xref_follow, match_indirect_obj_header,
    skip_damaged_truncated_stream, skip_pdf_ws_stopping_at_eof, step_indexed_non_code_token,
    ParenJumpTable, PdfNonCodeKind,
};
use crate::{DocumentFormat, EasyOfficeError};

#[derive(Clone, Copy, Default)]
struct RawSplicedHits {
    xref: bool,
    encrypt: bool,
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
    raw_paren_depth: usize,
    endobj_cache: &mut (usize, usize),
) -> Option<bool> {
    if q >= limit {
        return Some(false);
    }
    if is_line_eof_marker(bytes, q) {
        return Some(false);
    }
    let at_line_start = is_preceded_by_eol_ws(bytes, q);
    if allow_stream
        && is_stream_kw_at(bytes, q, bytes.len())
        && (raw_paren_depth == 0 || at_line_start || is_preceded_by_dict_close_ws(bytes, q))
    {
        return Some(true);
    }
    if (raw_paren_depth == 0 || at_line_start) && is_standalone_pdf_kw(bytes, q, b"endstream") {
        return Some(false);
    }
    if raw_paren_depth > 0 {
        if is_structural_endobj_at(bytes, q, endobj_cache) {
            return Some(false);
        }
    } else if is_standalone_pdf_kw(bytes, q, b"endobj")
        && (skip_pdf_ws_stopping_at_eof(bytes, q + 6, limit) >= limit
            || has_structural_after_endobj_cached(bytes, q + 6, endobj_cache))
    {
        return Some(false);
    }
    if (raw_paren_depth == 0 || at_line_start) && is_valid_xref_follow(bytes, q) {
        return Some(false);
    }
    if is_trailer_dict_at(bytes, q, bytes.len()).is_some() {
        return Some(false);
    }
    if (raw_paren_depth == 0 || at_line_start) && is_valid_startxref_at(bytes, q, bytes.len()) {
        return Some(false);
    }
    if (raw_paren_depth == 0 || at_line_start)
        && match_indirect_obj_header(bytes, q, bytes.len()).is_some()
    {
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
        || is_damaged_top_boundary(bytes, after_dict, limit, allow_stream, 0, endobj_cache)
            .is_some()
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
        let cls = classify_raw_paren_splice(raw);
        if cls.cross_obj
            || (cls.any_splice
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
        let summary = summarize_spliced_value(&val);
        if !summary.any_splice {
            return Some((val, next_p));
        }
    }
    parse_pdf_value_with_table(bytes, Some(tables.damaged), p, limit, depth)
        .ok()
        .flatten()
}

fn is_structural_spliced_follow(bytes: &[u8], after_entry: usize, limit: usize) -> bool {
    let next = skip_pdf_ws_stopping_at_eof(bytes, after_entry, limit);
    next >= limit
        || (next + 1 < limit && bytes[next] == b'/' && !is_pdf_delim_or_ws(bytes[next + 1]))
        || (next + 1 < limit && bytes[next] == b'>' && bytes[next + 1] == b'>')
        || matches!(bytes[next], b']' | b')')
        || is_stream_kw_at(bytes, next, bytes.len())
        || is_standalone_pdf_kw(bytes, next, b"endobj")
        || is_valid_xref_follow(bytes, next)
        || is_trailer_dict_at(bytes, next, bytes.len()).is_some()
        || is_valid_startxref_at(bytes, next, bytes.len())
        || is_line_eof_marker(bytes, next)
}

fn record_spliced_dict_entries(bytes: &[u8], q: usize, limit: usize, spliced: &mut RawSplicedHits) {
    if bytes[q] == b'/' {
        if let Some(after_enc) = is_encrypt_entry_at(bytes, q, limit) {
            if is_structural_spliced_follow(bytes, after_enc, limit) {
                spliced.encrypt = true;
            }
        }
        if is_type_xref_entry_at(bytes, q, limit) {
            spliced.xref = true;
        }
    }
}

fn step_clean_comment_or_literal(
    bytes: &[u8],
    tables: &DamagedParenTables<'_>,
    q: usize,
    limit: usize,
    raw_paren_depth: &mut usize,
) -> Option<usize> {
    if *raw_paren_depth > 0 {
        if bytes[q] == b'\\' {
            return Some((q + 2).min(limit));
        }
        if bytes[q] == b'(' {
            *raw_paren_depth += 1;
        } else if bytes[q] == b')' {
            *raw_paren_depth -= 1;
        }
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
        *raw_paren_depth = 1;
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
    let mut raw_paren_depth = 0usize;
    let mut q = skip_pdf_ws_stopping_at_eof(bytes, open_bracket + 1, limit);
    while q < limit {
        if let Some(next_q) =
            step_clean_comment_or_literal(bytes, tables, q, limit, &mut raw_paren_depth)
        {
            q = next_q;
            continue;
        }
        if is_damaged_top_boundary(bytes, q, limit, allow_stream, raw_paren_depth, endobj_cache)
            .is_some()
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

/// `>>` at `q` that closes the outer dictionary or is followed by another dictionary opener.
#[inline]
fn is_closing_pair_at(
    bytes: &[u8],
    q: usize,
    limit: usize,
    allow_stream: bool,
    endobj_cache: &mut (usize, usize),
) -> bool {
    q + 1 < limit
        && bytes[q] == b'>'
        && bytes[q + 1] == b'>'
        && (is_outer_dict_close(bytes, q, limit, allow_stream, endobj_cache)
            || is_pdf_dict_open_follow(&bytes[..limit], q + 2))
}

fn recover_broken_dict_value(
    bytes: &[u8],
    tables: &DamagedParenTables<'_>,
    p: usize,
    limit: usize,
    allow_stream: bool,
    endobj_cache: &mut (usize, usize),
    spliced: &mut RawSplicedHits,
) -> (usize, Option<bool>) {
    let is_arr = bytes[p] == b'[';
    let is_dict = p + 1 < limit && bytes[p] == b'<' && bytes[p + 1] == b'<';
    if is_arr || is_dict {
        recover_nested_value(
            bytes,
            tables,
            (p, limit),
            (is_arr, is_dict),
            allow_stream,
            endobj_cache,
            spliced,
        )
    } else {
        recover_bare_value(
            bytes,
            tables,
            (p, limit),
            allow_stream,
            endobj_cache,
            spliced,
        )
    }
}

/// Recovery scan for a value that starts with `[` (array) or `<<` (dictionary).
fn recover_nested_value(
    bytes: &[u8],
    tables: &DamagedParenTables<'_>,
    (p, limit): (usize, usize),
    (is_arr, is_dict): (bool, bool),
    allow_stream: bool,
    endobj_cache: &mut (usize, usize),
    spliced: &mut RawSplicedHits,
) -> (usize, Option<bool>) {
    let mut raw_paren_depth = 0usize;
    let mut depth = 1usize;
    let mut q = p + if is_dict { 2 } else { 1 };
    while q < limit {
        if let Some(next_q) =
            step_clean_comment_or_literal(bytes, tables, q, limit, &mut raw_paren_depth)
        {
            q = next_q;
            continue;
        }
        if let Some(hs) =
            is_damaged_top_boundary(bytes, q, limit, allow_stream, raw_paren_depth, endobj_cache)
        {
            return (q, Some(hs));
        }
        record_spliced_dict_entries(bytes, q, limit, spliced);
        if raw_paren_depth > 0 {
            if is_arr && depth == 1 && bytes[q] == b']' {
                let after = skip_pdf_ws_stopping_at_eof(bytes, q + 1, limit);
                if is_outer_dict_close(bytes, after, limit, allow_stream, endobj_cache)
                    || is_pdf_dict_open_follow(&bytes[..limit], q + 1)
                {
                    return (q + 1, None);
                }
            }
            if depth == 1 && is_closing_pair_at(bytes, q, limit, allow_stream, endobj_cache) {
                return (if is_dict { q + 2 } else { q }, None);
            }
            q += 1;
            continue;
        }
        if is_arr {
            if bytes[q] == b'[' {
                depth += 1;
            } else if bytes[q] == b']' {
                depth -= 1;
                if depth == 0 {
                    return (q + 1, None);
                }
            } else if depth == 1 && is_closing_pair_at(bytes, q, limit, allow_stream, endobj_cache)
            {
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
    (limit, None)
}

/// Recovery scan for a scalar / name value: runs to the next key, `>>` or boundary.
fn recover_bare_value(
    bytes: &[u8],
    tables: &DamagedParenTables<'_>,
    (p, limit): (usize, usize),
    allow_stream: bool,
    endobj_cache: &mut (usize, usize),
    spliced: &mut RawSplicedHits,
) -> (usize, Option<bool>) {
    let mut raw_paren_depth = 0usize;
    let mut q = p;
    while q < limit {
        if let Some(next_q) =
            step_clean_comment_or_literal(bytes, tables, q, limit, &mut raw_paren_depth)
        {
            q = next_q;
            continue;
        }
        if let Some(hs) =
            is_damaged_top_boundary(bytes, q, limit, allow_stream, raw_paren_depth, endobj_cache)
        {
            return (q, Some(hs));
        }
        record_spliced_dict_entries(bytes, q, limit, spliced);
        if raw_paren_depth > 0 {
            if is_closing_pair_at(bytes, q, limit, allow_stream, endobj_cache) {
                return (q, None);
            }
            let spliced_entry = bytes[q] == b'/'
                && (is_encrypt_entry_at(bytes, q, limit)
                    .is_some_and(|ae| is_structural_spliced_follow(bytes, ae, limit))
                    || is_type_xref_entry_at(bytes, q, limit));
            if spliced_entry {
                return (q, None);
            }
        } else if q > p
            && ((q + 1 < limit && bytes[q] == b'>' && bytes[q + 1] == b'>') || bytes[q] == b'/')
        {
            return (q, None);
        }
        q += 1;
    }
    (limit, None)
}

/// Outcome of one dictionary-entry step in the damaged-object walker.
enum DictStep {
    /// Continue scanning at this offset.
    Next(usize),
    /// The outer dictionary ended; leave the loop with this offset.
    Done(usize),
    /// A top-level boundary was hit at this offset (`true` when it is a stream start).
    Boundary(usize, bool),
}

type DamagedWalkResult = (bool, bool, Option<DamagedLengthHint>, usize, bool);

/// Mutable state while walking one damaged dictionary.
#[derive(Default)]
struct DamagedDictWalk {
    is_xref_type: bool,
    has_encrypt_key: bool,
    length_hint: Option<DamagedLengthHint>,
    spliced: RawSplicedHits,
}

/// Shared inputs of every walker step.
struct WalkEnv<'a, 'b> {
    bytes: &'a [u8],
    tables: &'a DamagedParenTables<'b>,
    limit: usize,
    allow_stream: bool,
}

impl DamagedDictWalk {
    fn result(&self, pos: usize, hit_stream: bool) -> DamagedWalkResult {
        (
            self.is_xref_type || self.spliced.xref,
            self.has_encrypt_key || self.spliced.encrypt,
            self.length_hint,
            pos,
            hit_stream,
        )
    }

    /// Handles a `>>` close or a top-level boundary at `p`, when present.
    fn close_or_boundary(
        env: &WalkEnv<'_, '_>,
        p: usize,
        cache: &mut (usize, usize),
    ) -> Option<DictStep> {
        let WalkEnv {
            bytes,
            limit,
            allow_stream,
            ..
        } = *env;
        if p + 1 < limit && bytes[p] == b'>' && bytes[p + 1] == b'>' {
            return Some(
                if is_outer_dict_close(bytes, p, limit, allow_stream, cache) {
                    DictStep::Done(p + 2)
                } else {
                    DictStep::Next(p + 2)
                },
            );
        }
        is_damaged_top_boundary(bytes, p, limit, allow_stream, 0, cache)
            .map(|hs| DictStep::Boundary(p, hs))
    }

    /// Runs value recovery from `start`; a boundary hit ends the walk.
    fn recover(
        &mut self,
        env: &WalkEnv<'_, '_>,
        start: usize,
        floor: usize,
        cache: &mut (usize, usize),
    ) -> DictStep {
        let (next_p, hit) = recover_broken_dict_value(
            env.bytes,
            env.tables,
            start,
            env.limit,
            env.allow_stream,
            cache,
            &mut self.spliced,
        );
        match hit {
            Some(hs) => DictStep::Boundary(next_p, hs),
            None => DictStep::Next(next_p.max(floor + 1)),
        }
    }

    /// Whether a parsed value is followed by a plausible next key, dict close or boundary.
    fn has_valid_follow(env: &WalkEnv<'_, '_>, next_p: usize, cache: &mut (usize, usize)) -> bool {
        let WalkEnv {
            bytes,
            limit,
            allow_stream,
            ..
        } = *env;
        let after_v = skip_pdf_ws_stopping_at_eof(bytes, next_p, limit);
        after_v >= limit
            || (after_v + 1 < limit
                && bytes[after_v] == b'/'
                && !is_pdf_delim_or_ws(bytes[after_v + 1]))
            || (after_v + 1 < limit && bytes[after_v] == b'>' && bytes[after_v + 1] == b'>')
            || is_damaged_top_boundary(bytes, after_v, limit, allow_stream, 0, cache).is_some()
    }

    /// Records the semantic effect of an accepted `key value` pair.
    fn apply_value(&mut self, key: &str, val: PdfObjValue) {
        let summary = summarize_spliced_value(&val);
        self.spliced.encrypt |= summary.encrypt;
        self.spliced.xref |= summary.xref_or_trailer;
        if key == "/Type" && matches!(&val, PdfObjValue::Name(n) if n == "/XRef") {
            self.is_xref_type = true;
        }
        if key == "/Encrypt" && !matches!(val, PdfObjValue::Null) {
            self.has_encrypt_key = true;
        }
        if key == "/Length" {
            self.length_hint = match val {
                PdfObjValue::Int(n) => usize::try_from(n).ok().map(DamagedLengthHint::Direct),
                PdfObjValue::Ref(lo, lg) => Some(DamagedLengthHint::Indirect(lo, lg)),
                _ => None,
            };
        }
    }

    /// Handles a value that failed strict parsing or has no valid follow.
    fn rejected_value(
        &mut self,
        env: &WalkEnv<'_, '_>,
        key: &str,
        p: usize,
        cache: &mut (usize, usize),
    ) -> DictStep {
        let bytes = env.bytes;
        if p + 1 < env.limit && bytes[p] == b'/' && !is_pdf_delim_or_ws(bytes[p + 1]) {
            return DictStep::Next(p);
        }
        let in_id_array = key == "/ID" && bytes[p] == b'[';
        if in_id_array {
            if let Some(id_end) =
                find_id_array_end(bytes, env.tables, p, env.limit, env.allow_stream, cache)
            {
                return DictStep::Next(id_end);
            }
        }
        let recover_start = if in_id_array {
            (p + 1).min(env.limit)
        } else {
            p
        };
        self.recover(env, recover_start, p, cache)
    }

    /// Handles a `/Key value` entry starting at the `/` at `p`.
    fn key_entry(
        &mut self,
        env: &WalkEnv<'_, '_>,
        p: usize,
        cache: &mut (usize, usize),
    ) -> DictStep {
        let WalkEnv {
            bytes,
            tables,
            limit,
            ..
        } = *env;
        let mut k = p + 1;
        while k < limit && !is_pdf_delim_or_ws(bytes[k]) {
            k += 1;
        }
        let key = decode_pdf_name(&bytes[p + 1..k]);
        let p = skip_pdf_ws_stopping_at_eof(bytes, k, limit);
        if p >= limit {
            return DictStep::Done(p);
        }
        if let Some(step) = Self::close_or_boundary(env, p, cache) {
            return step;
        }
        if key == "/Encrypt" && !is_standalone_pdf_kw(&bytes[..limit], p, b"null") {
            self.has_encrypt_key = true;
        }
        let accepted = parse_value_prefer_strict(bytes, tables, p, limit, 1)
            .filter(|&(_, next_p)| Self::has_valid_follow(env, next_p, cache));
        match accepted {
            Some((val, next_p)) => {
                self.apply_value(&key, val);
                DictStep::Next(next_p)
            }
            None => self.rejected_value(env, &key, p, cache),
        }
    }

    /// One step of the entry loop; `p` is non-whitespace and below the limit.
    fn step(&mut self, env: &WalkEnv<'_, '_>, p: usize, cache: &mut (usize, usize)) -> DictStep {
        if let Some(step) = Self::close_or_boundary(env, p, cache) {
            return step;
        }
        if env.bytes[p] == b'/' {
            self.key_entry(env, p, cache)
        } else {
            self.recover(env, p, p, cache)
        }
    }
}

/// Walks a non-dictionary body up to its boundary.
fn walk_damaged_body(
    env: &WalkEnv<'_, '_>,
    p0: usize,
    cache: &mut (usize, usize),
) -> DamagedWalkResult {
    let WalkEnv {
        bytes,
        tables,
        limit,
        allow_stream,
    } = *env;
    let mut raw_paren_depth = 0usize;
    let mut spliced = RawSplicedHits::default();
    let mut q = p0;
    while q < limit {
        if let Some(next_q) =
            step_clean_comment_or_literal(bytes, tables, q, limit, &mut raw_paren_depth)
        {
            q = next_q;
            continue;
        }
        if let Some(hs) =
            is_damaged_top_boundary(bytes, q, limit, allow_stream, raw_paren_depth, cache)
        {
            return (spliced.xref, spliced.encrypt, None, q, hs);
        }
        record_spliced_dict_entries(bytes, q, limit, &mut spliced);
        q += 1;
    }
    (spliced.xref, spliced.encrypt, None, limit, false)
}

fn walk_damaged_dict_or_body(
    bytes: &[u8],
    tables: &DamagedParenTables<'_>,
    start: usize,
    limit: usize,
    allow_stream: bool,
    endobj_cache: &mut (usize, usize),
) -> DamagedWalkResult {
    let env = WalkEnv {
        bytes,
        tables,
        limit,
        allow_stream,
    };
    let p0 = skip_pdf_ws_stopping_at_eof(bytes, start, limit);
    if p0 >= limit {
        return (false, false, None, limit, false);
    }
    if p0 + 1 >= limit || bytes[p0] != b'<' || bytes[p0 + 1] != b'<' {
        return walk_damaged_body(&env, p0, endobj_cache);
    }

    let mut walk = DamagedDictWalk::default();
    let mut p = p0 + 2;
    while p < limit {
        p = skip_pdf_ws_stopping_at_eof(bytes, p, limit);
        if p >= limit {
            break;
        }
        match walk.step(&env, p, endobj_cache) {
            DictStep::Next(next) => p = next,
            DictStep::Done(next) => {
                p = next;
                break;
            }
            DictStep::Boundary(pos, hs) => return walk.result(pos, hs),
        }
    }

    let after_dict = skip_pdf_ws_stopping_at_eof(bytes, p, limit);
    if allow_stream && is_stream_kw_at(bytes, after_dict, bytes.len()) {
        return walk.result(after_dict, true);
    }
    if is_standalone_pdf_kw(bytes, after_dict, b"endobj") {
        return walk.result(after_dict + 6, false);
    }
    walk.result(p, false)
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
    eof_markers: &[usize],
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
                lookup_indexed_int_obj(int_objs, eof_markers, bytes, stream_start, *lo, *lg)
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
            if let Some((found_enc, next_pos)) = try_step_undamaged_object(
                bytes,
                &tables,
                &int_objs,
                &eof_markers,
                after_obj,
                rev_limit,
            ) {
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
                        lookup_indexed_int_obj(&int_objs, &eof_markers, bytes, stream_start, lo, lg)
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

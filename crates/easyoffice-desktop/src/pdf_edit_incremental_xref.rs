//! Cross-reference (`xref` table and PDF 1.5+ `/Type /XRef` stream) inspection and bounds
//! enforcement for ISO 32000-1 incremental updates (PD-349).
//!
//! Before any bytes are appended to an original PDF, [`inspect_xref`] walks the entire `/Prev`
//! chain independently of the PDF reader so repaired/forged `startxref` offsets, circular `/Prev`
//! links, hybrid `/XRefStm` tables, unsupported `/Encrypt` trailers or oversized `/Size` entries
//! cleanly trigger safe fallback without partial writes or memory blow-ups.

use crate::pdf_edit_budget::MAX_OBJECTS;
use crate::pdf_edit_incremental_val::IncVal;
use crate::pdf_edit_value::Budget;
use flate2::read::ZlibDecoder;
use hayro_syntax::object::{
    Array, Dict, FromBytes, MaybeRef, Name, Object, ObjectIdentifier, Stream,
};
use hayro_syntax::Pdf as ReadPdf;
use std::collections::HashSet;
use std::io::Read;

const MAX_TAIL_SCAN_BYTES: usize = 2_048;
/// Reserve one level so appending an incremental update stays within 128 total sections.
const MAX_EXISTING_XREF_CHAIN_DEPTH: usize = 127;
const MAX_XREF_STREAM_BYTES: usize = 16 * 1024 * 1024;
const MAX_W_BYTES: usize = 32;

fn bitset_contains(words: &[u64], limit: u32, obj: u32) -> bool {
    obj < limit && (words[(obj as usize) >> 6] & (1u64 << (obj & 63))) != 0
}

fn bitset_insert(words: &mut [u64], limit: u32, obj: u32) -> bool {
    if obj >= limit {
        return false;
    }
    let idx = (obj as usize) >> 6;
    let bit = 1u64 << (obj & 63);
    let fresh = (words[idx] & bit) == 0;
    words[idx] |= bit;
    fresh
}

struct ActiveCheckState {
    limit: u32,
    settled: Vec<u64>,
    active_type1: Vec<u64>,
    needed_objstms: Vec<u32>,
    needed_objstm_set: Vec<u64>,
    root_ref: ObjectIdentifier,
    seen_root: bool,
}

impl ActiveCheckState {
    fn new(limit: u32, root_ref: ObjectIdentifier) -> Self {
        let count = (limit as usize).div_ceil(64);
        Self {
            limit,
            settled: vec![0u64; count],
            active_type1: vec![0u64; count],
            needed_objstms: Vec::new(),
            needed_objstm_set: vec![0u64; count],
            root_ref,
            seen_root: false,
        }
    }

    fn settle(&mut self, obj: u32) -> bool {
        bitset_insert(&mut self.settled, self.limit, obj)
    }

    fn check_root(&mut self, obj_num: u32, gen_num: u16) -> bool {
        if obj_num != self.root_ref.obj_number {
            return true;
        }
        if i32::from(gen_num) != self.root_ref.gen_number {
            return false;
        }
        self.seen_root = true;
        true
    }

    fn record_needed_objstm(&mut self, objstm_num: u32) -> bool {
        if objstm_num == 0 || objstm_num >= self.limit {
            return false;
        }
        if bitset_insert(&mut self.needed_objstm_set, self.limit, objstm_num) {
            self.needed_objstms.push(objstm_num);
        }
        true
    }
}

/// Cross-reference structure used by the original PDF and preserved for incremental updates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum XRefFormat {
    Table,
    Stream,
}

#[derive(Clone, Debug)]
pub(crate) struct XRefMeta {
    pub format: XRefFormat,
    pub startxref: u64,
    pub orig_size: u32,
    pub root_ref: ObjectIdentifier,
    pub info_ref: Option<ObjectIdentifier>,
    pub id_entry: Option<IncVal>,
}

#[derive(Clone, Debug)]
pub(crate) enum XRefInspectResult {
    Ready(XRefMeta),
    Fallback(&'static str),
}

fn fb(reason: &'static str) -> Result<SectionParse, String> {
    Ok(SectionParse::Fallback(reason))
}

fn inspect_fb(reason: &'static str) -> Result<XRefInspectResult, String> {
    Ok(XRefInspectResult::Fallback(reason))
}

/// Inspects the trailing `%%EOF`, `startxref` and full `/Prev` chain of `bytes`.
pub(crate) fn inspect_xref(bytes: &[u8], pdf: &ReadPdf) -> Result<XRefInspectResult, String> {
    let Some(startxref) = parse_tail_startxref(bytes) else {
        return inspect_fb(
            "원본 PDF의 끝(startxref/%%EOF) 구조를 확인할 수 없어 새 PDF로 다시 저장했습니다.",
        );
    };

    let mut seen_offsets = HashSet::new();
    let mut cur_offset = startxref;
    let mut depth = 0usize;
    let mut latest_format: Option<XRefFormat> = None;
    let mut latest_meta: Option<XRefMeta> = None;
    let mut active: Option<ActiveCheckState> = None;

    loop {
        if depth >= MAX_EXISTING_XREF_CHAIN_DEPTH {
            return inspect_fb(
                "원본 PDF의 증분 갱신 이력이 허용 깊이를 초과해 새 PDF로 다시 저장했습니다.",
            );
        }
        if !seen_offsets.insert(cur_offset) {
            return inspect_fb(
                "원본 PDF의 교차 참조(/Prev)에 순환 참조가 있어 새 PDF로 다시 저장했습니다.",
            );
        }
        depth += 1;

        let Ok(offset_usize) = usize::try_from(cur_offset) else {
            return inspect_fb(
                "원본 PDF의 교차 참조 위치가 올바르지 않아 새 PDF로 다시 저장했습니다.",
            );
        };
        if offset_usize >= bytes.len() {
            return inspect_fb(
                "원본 PDF의 startxref 위치가 파일 크기를 벗어나 새 PDF로 다시 저장했습니다.",
            );
        }

        let section = &bytes[offset_usize..];
        let parsed = if is_classic_xref_start(section) {
            if latest_format == Some(XRefFormat::Stream) {
                return inspect_fb(
                    "원본 PDF의 교차 참조 형식이 섞여 있어 새 PDF로 다시 저장했습니다.",
                );
            }
            latest_format = Some(XRefFormat::Table);
            parse_classic_section(
                bytes,
                offset_usize,
                pdf.xref().root_id(),
                latest_meta.as_ref().map(|m| m.orig_size),
                &mut active,
            )?
        } else {
            if latest_format == Some(XRefFormat::Table) {
                return inspect_fb(
                    "원본 PDF의 교차 참조 형식이 섞여 있어 새 PDF로 다시 저장했습니다.",
                );
            }
            latest_format = Some(XRefFormat::Stream);
            parse_stream_section(
                bytes,
                offset_usize,
                pdf.xref().root_id(),
                latest_meta.as_ref().map(|m| m.orig_size),
                &mut active,
            )?
        };

        let info = match parsed {
            SectionParse::Ok(info) => info,
            SectionParse::Fallback(reason) => return inspect_fb(reason),
        };

        if let Some(meta) = &mut latest_meta {
            if meta.info_ref.is_none() {
                meta.info_ref = info
                    .info_ref
                    .filter(|id| id.obj_number > 0 && id.obj_number < meta.orig_size);
            }
            if meta.id_entry.is_none() {
                meta.id_entry = info.id_entry;
            }
        } else {
            let Some(root_ref) = info.root_ref else {
                return inspect_fb(
                    "원본 PDF 트레일러에 /Root 참조가 없어 새 PDF로 다시 저장했습니다.",
                );
            };
            latest_meta = Some(XRefMeta {
                format: latest_format.unwrap_or(XRefFormat::Table),
                startxref,
                orig_size: info.size,
                root_ref,
                info_ref: info.info_ref,
                id_entry: info.id_entry,
            });
        }

        match info.prev {
            Some(prev) => cur_offset = prev,
            None => break,
        }
    }

    let Some(state) = active else {
        return inspect_fb("원본 PDF의 교차 참조 정보를 읽을 수 없어 새 PDF로 다시 저장했습니다.");
    };
    let objstms_ok = state.needed_objstms.iter().all(|&id| {
        bitset_contains(&state.active_type1, state.limit, id)
            && pdf
                .xref()
                .get::<Stream<'_>>(ObjectIdentifier::new(id, 0))
                .is_some_and(|s| s.dict().get::<Name<'_>>(b"Type").as_deref() == Some(b"ObjStm"))
    });
    if !state.seen_root || !objstms_ok {
        return inspect_fb("원본 PDF의 교차 참조가 복구 모드로 열려 새 PDF로 다시 저장했습니다.");
    }

    match latest_meta {
        Some(meta) => Ok(XRefInspectResult::Ready(meta)),
        None => inspect_fb("원본 PDF의 교차 참조 정보를 읽을 수 없어 새 PDF로 다시 저장했습니다."),
    }
}

struct SectionInfo {
    size: u32,
    root_ref: Option<ObjectIdentifier>,
    info_ref: Option<ObjectIdentifier>,
    id_entry: Option<IncVal>,
    prev: Option<u64>,
}

enum SectionParse {
    Ok(SectionInfo),
    Fallback(&'static str),
}

fn is_pdf_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n' | 0x00 | 0x0c)
}

fn is_pdf_delim(b: u8) -> bool {
    matches!(
        b,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

fn is_classic_xref_start(section: &[u8]) -> bool {
    section.starts_with(b"xref") && section.get(4).is_some_and(|&b| is_pdf_ws(b))
}

/// Parses `startxref <offset> %%EOF` strictly from the last 2048 bytes with no trailing junk.
fn parse_tail_startxref(bytes: &[u8]) -> Option<u64> {
    let tail_start = bytes.len().saturating_sub(MAX_TAIL_SCAN_BYTES);
    let tail = &bytes[tail_start..];
    let mut end = tail.len();
    while end > 0 && is_pdf_ws(tail[end - 1]) {
        end -= 1;
    }
    let before_eof = tail.get(..end)?.strip_suffix(b"%%EOF")?;
    let mut pos = before_eof.len();
    while pos > 0 && is_pdf_ws(before_eof[pos - 1]) {
        pos -= 1;
    }
    let digits_end = pos;
    while pos > 0 && before_eof[pos - 1].is_ascii_digit() {
        pos -= 1;
    }
    if pos == digits_end {
        return None;
    }
    let offset_str = std::str::from_utf8(&before_eof[pos..digits_end]).ok()?;
    let offset = offset_str.parse::<u64>().ok()?;
    while pos > 0 && is_pdf_ws(before_eof[pos - 1]) {
        pos -= 1;
    }
    let before_kw = before_eof.get(..pos)?.strip_suffix(b"startxref")?;
    let abs_before = tail_start + before_kw.len();
    if abs_before > 0 && !is_pdf_ws(bytes[abs_before - 1]) {
        return None;
    }
    if offset >= bytes.len() as u64 {
        return None;
    }
    Some(offset)
}

fn parse_classic_section(
    all_bytes: &[u8],
    section_offset: usize,
    expected_root: ObjectIdentifier,
    latest_size: Option<u32>,
    active: &mut Option<ActiveCheckState>,
) -> Result<SectionParse, String> {
    let section = &all_bytes[section_offset..];
    let mut pos = b"xref".len();
    skip_ws(section, &mut pos);

    let mut max_end = 0u32;
    let mut subsection_count = 0usize;
    let mut subsections: Vec<(u32, u32, usize)> = Vec::new();
    loop {
        skip_ws(section, &mut pos);
        if starts_with_word(section, pos, b"trailer") {
            pos += b"trailer".len();
            break;
        }
        subsection_count += 1;
        if subsection_count > MAX_OBJECTS {
            return fb("원본 PDF의 xref 하위 절 수가 한도를 초과해 새 PDF로 다시 저장했습니다.");
        }
        let Some(first) = read_u64(section, &mut pos) else {
            return fb("원본 PDF의 xref 하위 절 번호가 손상되어 새 PDF로 다시 저장했습니다.");
        };
        skip_ws(section, &mut pos);
        let Some(count) = read_u64(section, &mut pos) else {
            return fb("원본 PDF의 xref 하위 절 개수가 손상되어 새 PDF로 다시 저장했습니다.");
        };
        let Some(end) = first.checked_add(count) else {
            return fb("원본 PDF의 xref 객체 번호 범위가 오버플로되어 새 PDF로 다시 저장했습니다.");
        };
        if count > MAX_OBJECTS as u64 || end > MAX_OBJECTS as u64 {
            return fb("원본 PDF의 xref 객체 수가 허용 한도를 초과해 새 PDF로 다시 저장했습니다.");
        }
        if let Some(limit) = latest_size {
            if end > u64::from(limit) {
                return fb(
                    "원본 PDF의 xref 하위 절 범위가 /Size를 벗어나 새 PDF로 다시 저장했습니다.",
                );
            }
        }
        let Ok(end_u32) = u32::try_from(end) else {
            return fb("원본 PDF의 xref 객체 번호가 한도를 초과해 새 PDF로 다시 저장했습니다.");
        };
        max_end = max_end.max(end_u32);

        // Consume line ending after `<first> <count>` before reading 20-byte entries.
        skip_horiz_ws(section, &mut pos);
        if section.get(pos..pos + 2) == Some(b"\r\n") {
            pos += 2;
        } else if matches!(section.get(pos), Some(&b'\r' | &b'\n')) {
            pos += 1;
        } else if count > 0 {
            return fb(
                "원본 PDF의 xref 하위 절 헤더 줄바꿈이 올바르지 않아 새 PDF로 다시 저장했습니다.",
            );
        }

        let Some(entry_bytes) = usize::try_from(count).ok().and_then(|c| c.checked_mul(20)) else {
            return fb("원본 PDF의 xref 크기가 파일 크기를 초과해 새 PDF로 다시 저장했습니다.");
        };
        if section.len().saturating_sub(pos) < entry_bytes {
            return fb("원본 PDF의 xref 표가 중간에 잘려 새 PDF로 다시 저장했습니다.");
        }
        let entries_start = pos;
        for _ in 0..count {
            if !consume_classic_entry(section, &mut pos) {
                return fb("원본 PDF의 xref 항목 형식이 올바르지 않아 새 PDF로 다시 저장했습니다.");
            }
        }
        subsections.push((first as u32, count as u32, entries_start));
    }

    skip_ws(section, &mut pos);
    let Some(trailer) = Dict::from_bytes(&section[pos..]) else {
        return fb("원본 PDF의 trailer 사전을 읽을 수 없어 새 PDF로 다시 저장했습니다.");
    };
    let info = match validate_trailer_dict(
        &trailer,
        all_bytes.len(),
        max_end,
        latest_size,
        active.is_none(),
    )? {
        SectionParse::Ok(info) => info,
        other @ SectionParse::Fallback(_) => return Ok(other),
    };

    let state = match init_active_state(active, &info, expected_root) {
        Ok(state) => state,
        Err(reason) => return fb(reason),
    };
    for (first, count, entries_start) in subsections {
        for idx in 0..count {
            let obj_num = first + idx;
            let e_pos = entries_start + (idx as usize) * 20;
            let row = &section[e_pos..e_pos + 20];
            if !state.settle(obj_num) || row[17] != b'n' {
                continue;
            }
            let Some(obj_off) = parse_ascii_usize(&row[..10]) else {
                return fb("원본 PDF의 xref 오프셋이 손상되어 새 PDF로 다시 저장했습니다.");
            };
            let Some(gen_num) = parse_ascii_u16(&row[11..16]) else {
                return fb("원본 PDF의 xref 세대 번호가 손상되어 새 PDF로 다시 저장했습니다.");
            };
            if !verify_indirect_obj_header(all_bytes, obj_off, obj_num, gen_num)
                || !state.check_root(obj_num, gen_num)
            {
                return fb(
                    "원본 PDF의 xref 항목이 실제 객체 위치와 달라 새 PDF로 다시 저장했습니다.",
                );
            }
        }
    }
    Ok(SectionParse::Ok(info))
}

/// Enforces ISO 32000-1 §7.5.4 exact 20-byte cross-reference entry format (`SP LF`, `SP CR`, or `CR LF`).
fn consume_classic_entry(section: &[u8], pos: &mut usize) -> bool {
    let Some(row) = section.get(*pos..*pos + 20) else {
        return false;
    };
    if !row[..10].iter().all(u8::is_ascii_digit)
        || row[10] != b' '
        || !row[11..16].iter().all(u8::is_ascii_digit)
        || row[16] != b' '
        || !matches!(row[17], b'n' | b'f')
        || !matches!(&row[18..20], b" \n" | b" \r" | b"\r\n")
    {
        return false;
    }
    *pos += 20;
    true
}

fn parse_stream_section(
    all_bytes: &[u8],
    section_offset: usize,
    expected_root: ObjectIdentifier,
    latest_size: Option<u32>,
    active: &mut Option<ActiveCheckState>,
) -> Result<SectionParse, String> {
    let section = &all_bytes[section_offset..];
    let mut pos = 0usize;
    let Some(obj_num) = read_u64(section, &mut pos) else {
        return fb(
            "원본 PDF의 /Type /XRef 스트림 객체 번호가 올바르지 않아 새 PDF로 다시 저장했습니다.",
        );
    };
    skip_ws(section, &mut pos);
    let Some(gen_num) = read_u64(section, &mut pos) else {
        return fb("원본 PDF의 /Type /XRef 세대 번호가 올바르지 않아 새 PDF로 다시 저장했습니다.");
    };
    skip_ws(section, &mut pos);
    if !starts_with_word_or_delim(section, pos, b"obj") {
        return fb("원본 PDF의 /Type /XRef 객체 헤더가 올바르지 않아 새 PDF로 다시 저장했습니다.");
    }
    pos += b"obj".len();
    skip_ws(section, &mut pos);

    if obj_num == 0 || obj_num > MAX_OBJECTS as u64 || gen_num > u64::from(u16::MAX) {
        return fb(
            "원본 PDF의 /Type /XRef 객체 번호가 허용 범위를 벗어나 새 PDF로 다시 저장했습니다.",
        );
    }

    let Some(stream) = Stream::from_bytes(&section[pos..]) else {
        return fb("원본 PDF의 /Type /XRef 스트림을 해석할 수 없어 새 PDF로 다시 저장했습니다.");
    };
    let dict = stream.dict();
    if dict.get::<Name<'_>>(b"Type").as_deref() != Some(b"XRef") {
        return fb(
            "원본 PDF의 startxref 대상이 /Type /XRef가 아니어서 새 PDF로 다시 저장했습니다.",
        );
    }
    let info = match validate_trailer_dict(dict, all_bytes.len(), 0, latest_size, active.is_none())?
    {
        SectionParse::Ok(info) => info,
        other @ SectionParse::Fallback(_) => return Ok(other),
    };
    let effective_limit = latest_size.unwrap_or(info.size);
    if obj_num >= u64::from(effective_limit) {
        return fb(
            "원본 PDF의 /Type /XRef 객체 번호가 /Size 이상이어서 새 PDF로 다시 저장했습니다.",
        );
    }

    let Some(w_arr) = dict.get::<Array<'_>>(b"W") else {
        return fb("원본 PDF의 /XRef /W 배열이 없어 새 PDF로 다시 저장했습니다.");
    };
    let w_raw: Vec<MaybeRef<Object<'_>>> = w_arr.raw_iter().take(4).collect();
    if w_raw.len() != 3 {
        return fb("원본 PDF의 /XRef /W 형식이 올바르지 않아 새 PDF로 다시 저장했습니다.");
    }
    let mut w = [0usize; 3];
    for (slot, item) in w.iter_mut().zip(&w_raw) {
        let MaybeRef::NotRef(Object::Number(n)) = item else {
            return fb("원본 PDF의 /XRef /W 형식이 올바르지 않아 새 PDF로 다시 저장했습니다.");
        };
        let f = n.as_f64();
        if !f.is_finite() || f.fract() != 0.0 || f < 0.0 || f > MAX_W_BYTES as f64 {
            return fb("원본 PDF의 /XRef /W 너비를 지원하지 않아 새 PDF로 다시 저장했습니다.");
        }
        *slot = n.as_i64() as usize;
    }
    let (w0, w1, w2) = (w[0], w[1], w[2]);
    let stride = w0 + w1 + w2;
    if stride == 0
        || stride > MAX_W_BYTES
        || !matches!(w0, 0 | 1)
        || !matches!(w1, 1 | 2 | 3 | 4 | 8)
        || !matches!(w2, 0 | 1 | 2 | 3 | 4 | 8)
    {
        return fb("원본 PDF의 /XRef /W 너비를 지원하지 않아 새 PDF로 다시 저장했습니다.");
    }

    let idx_arr = dict.get::<Array<'_>>(b"Index");
    let total_entries = if let Some(ref arr) = idx_arr {
        match count_xref_index_entries(arr, effective_limit) {
            Ok(n) => n,
            Err(reason) => return fb(reason),
        }
    } else {
        info.size as usize
    };

    let Some(entries_data) = decode_xref_stream_payload(&stream, stride, total_entries) else {
        return fb("원본 PDF의 /XRef 스트림 크기가 유효하지 않거나 한도를 초과해 새 PDF로 다시 저장했습니다.");
    };

    let state = match init_active_state(active, &info, expected_root) {
        Ok(state) => state,
        Err(reason) => return fb(reason),
    };
    if let Err(reason) = validate_xref_stream_entries(
        all_bytes,
        &entries_data,
        idx_arr.as_ref(),
        info.size,
        (w0, w1, w2),
        state,
    ) {
        return fb(reason);
    }

    Ok(SectionParse::Ok(info))
}

fn init_active_state<'a>(
    active: &'a mut Option<ActiveCheckState>,
    info: &SectionInfo,
    expected_root: ObjectIdentifier,
) -> Result<&'a mut ActiveCheckState, &'static str> {
    if active.is_none() {
        let root_ref = info
            .root_ref
            .ok_or("원본 PDF 트레일러에 /Root 참조가 없어 새 PDF로 다시 저장했습니다.")?;
        if root_ref != expected_root {
            return Err(
                "원본 PDF의 /Root가 교차 참조 복구 결과와 달라 새 PDF로 다시 저장했습니다.",
            );
        }
        *active = Some(ActiveCheckState::new(info.size, root_ref));
    }
    Ok(active.as_mut().expect("initialized"))
}

fn count_xref_index_entries(arr: &Array<'_>, effective_limit: u32) -> Result<usize, &'static str> {
    let mut iter = arr.raw_iter();
    let mut total = 0usize;
    let mut pairs = 0usize;
    while let Some(first_raw) = iter.next() {
        let Some(count_raw) = iter.next() else {
            return Err("원본 PDF의 /XRef /Index 길이가 홀수여서 새 PDF로 다시 저장했습니다.");
        };
        pairs += 1;
        if pairs > MAX_OBJECTS {
            return Err(
                "원본 PDF의 /XRef /Index 항목 수가 한도를 초과해 새 PDF로 다시 저장했습니다.",
            );
        }
        let (MaybeRef::NotRef(Object::Number(fn_num)), MaybeRef::NotRef(Object::Number(cn_num))) =
            (first_raw, count_raw)
        else {
            return Err("원본 PDF의 /XRef /Index 형식이 올바르지 않아 새 PDF로 다시 저장했습니다.");
        };
        let (ff, cf) = (fn_num.as_f64(), cn_num.as_f64());
        if !ff.is_finite() || ff.fract() != 0.0 || !cf.is_finite() || cf.fract() != 0.0 {
            return Err("원본 PDF의 /XRef /Index 형식이 올바르지 않아 새 PDF로 다시 저장했습니다.");
        }
        let (first, count) = (fn_num.as_i64(), cn_num.as_i64());
        if first < 0 || count < 0 {
            return Err("원본 PDF의 /XRef /Index에 음수가 있어 새 PDF로 다시 저장했습니다.");
        }
        let Some(end) = first.checked_add(count) else {
            return Err("원본 PDF의 /XRef /Index 범위가 오버플로되어 새 PDF로 다시 저장했습니다.");
        };
        if end > i64::from(effective_limit) {
            return Err(
                "원본 PDF의 /XRef /Index 범위가 /Size를 초과해 새 PDF로 다시 저장했습니다.",
            );
        }
        total = total.saturating_add(count as usize);
        if total > MAX_OBJECTS {
            return Err("원본 PDF의 /XRef 항목 수가 한도를 초과해 새 PDF로 다시 저장했습니다.");
        }
    }
    Ok(total)
}

fn decode_xref_stream_payload(
    stream: &Stream<'_>,
    stride: usize,
    total_entries: usize,
) -> Option<Vec<u8>> {
    let dict = stream.dict();
    let dp_dict = [b"DecodeParms".as_slice(), b"DP".as_slice()]
        .into_iter()
        .find_map(|k| {
            dict.get::<Dict<'_>>(k).or_else(|| {
                dict.get::<Array<'_>>(k)
                    .and_then(|arr| arr.iter::<Dict<'_>>().next())
            })
        });
    let predictor = dp_dict
        .as_ref()
        .and_then(|dp| dp.get::<i64>(b"Predictor"))
        .unwrap_or(1);
    if predictor != 1 && !(10..=15).contains(&predictor) {
        return None;
    }
    if predictor >= 10 {
        if let Some(dp) = &dp_dict {
            if dp
                .get::<i64>(b"Columns")
                .is_some_and(|c| c != stride as i64)
                || dp.get::<i64>(b"Colors").is_some_and(|c| c != 1)
                || dp.get::<i64>(b"BitsPerComponent").is_some_and(|b| b != 8)
            {
                return None;
            }
        } else if stride != 1 {
            return None;
        }
    }
    let row_len = if predictor >= 10 { stride + 1 } else { stride };
    let expected_len = total_entries.checked_mul(row_len)?;
    if expected_len > MAX_XREF_STREAM_BYTES {
        return None;
    }

    let filter = [b"Filter".as_slice(), b"F".as_slice()]
        .into_iter()
        .find_map(|k| match dict.get::<Name<'_>>(k) {
            Some(n) => Some(Ok(n.as_str().to_string())),
            None => dict.get::<Array<'_>>(k).map(|arr| {
                let mut names = arr.iter::<Name<'_>>();
                let first = names.next().map(|n| n.as_str().to_string());
                if names.next().is_some() {
                    Err(())
                } else {
                    first.ok_or(())
                }
            }),
        });
    let filter_name = match filter {
        None => None,
        Some(Ok(name)) => Some(name),
        Some(Err(())) => return None,
    };

    let raw = stream.raw_data();
    let decoded = match filter_name.as_deref() {
        None => {
            if raw.len() != expected_len {
                return None;
            }
            raw.to_vec()
        }
        Some("FlateDecode") => {
            let cap = u64::try_from(expected_len).ok()?.saturating_add(1);
            let mut dec = ZlibDecoder::new(raw.as_ref()).take(cap);
            let mut buf = Vec::new();
            dec.read_to_end(&mut buf).ok()?;
            if buf.len() != expected_len {
                return None;
            }
            buf
        }
        _ => return None,
    };
    if predictor == 1 {
        Some(decoded)
    } else {
        let out = stream.decoded().ok()?;
        (out.len() == total_entries.checked_mul(stride)?).then(|| out.into_owned())
    }
}

fn validate_xref_stream_entries(
    all_bytes: &[u8],
    payload: &[u8],
    idx_arr: Option<&Array<'_>>,
    size: u32,
    (w0, w1, w2): (usize, usize, usize),
    state: &mut ActiveCheckState,
) -> Result<(), &'static str> {
    let stride = w0 + w1 + w2;
    let mut row_idx = 0usize;
    let mut check_run = |first: u32, count: u32| -> Result<(), &'static str> {
        for i in 0..count {
            let obj_num = first + i;
            let row = payload
                .get(row_idx * stride..(row_idx + 1) * stride)
                .ok_or("원본 PDF의 /XRef 스트림 항목이 부족해 새 PDF로 다시 저장했습니다.")?;
            row_idx += 1;
            let f_type = if w0 == 0 { 1u8 } else { row[0] };
            if !matches!(f_type, 0..=2) {
                return Err(
                    "원본 PDF의 /XRef 항목 형식이 올바르지 않아 새 PDF로 다시 저장했습니다.",
                );
            }
            if !state.settle(obj_num) || f_type == 0 {
                continue;
            }
            let f1 = read_be_u64(&row[w0..w0 + w1]);
            let f2 = if w2 == 0 {
                0
            } else {
                read_be_u64(&row[w0 + w1..stride])
            };
            match f_type {
                1 => {
                    let Ok(off) = usize::try_from(f1) else {
                        return Err(
                            "원본 PDF의 /XRef 오프셋이 올바르지 않아 새 PDF로 다시 저장했습니다.",
                        );
                    };
                    let Ok(gen_u16) = u16::try_from(f2) else {
                        return Err("원본 PDF의 /XRef 세대 번호가 올바르지 않아 새 PDF로 다시 저장했습니다.");
                    };
                    if !verify_indirect_obj_header(all_bytes, off, obj_num, gen_u16)
                        || !state.check_root(obj_num, gen_u16)
                    {
                        return Err("원본 PDF의 /XRef 항목이 실제 객체 위치와 달라 새 PDF로 다시 저장했습니다.");
                    }
                    bitset_insert(&mut state.active_type1, state.limit, obj_num);
                }
                2 => {
                    let Ok(objstm_num) = u32::try_from(f1) else {
                        return Err(
                            "원본 PDF의 /ObjStm 번호가 올바르지 않아 새 PDF로 다시 저장했습니다.",
                        );
                    };
                    if !state.check_root(obj_num, 0) || !state.record_needed_objstm(objstm_num) {
                        return Err(
                            "원본 PDF의 /ObjStm 참조가 올바르지 않아 새 PDF로 다시 저장했습니다.",
                        );
                    }
                }
                _ => {}
            }
        }
        Ok(())
    };

    if let Some(arr) = idx_arr {
        let mut iter = arr.raw_iter();
        while let (
            Some(MaybeRef::NotRef(Object::Number(f))),
            Some(MaybeRef::NotRef(Object::Number(c))),
        ) = (iter.next(), iter.next())
        {
            check_run(f.as_i64() as u32, c.as_i64() as u32)?;
        }
    } else {
        check_run(0, size)?;
    }
    Ok(())
}

fn validate_trailer_dict(
    dict: &Dict<'_>,
    total_len: usize,
    max_subsection_end: u32,
    latest_size: Option<u32>,
    read_id: bool,
) -> Result<SectionParse, String> {
    if has_non_null_entry(dict, b"Encrypt") {
        return fb("암호화 트레일러가 있어 증분 저장 대신 새 PDF로 다시 저장했습니다.");
    }
    if has_non_null_entry(dict, b"XRefStm") {
        return fb("하이브리드 XRef(/XRefStm) 구조는 안전을 위해 새 PDF로 다시 저장했습니다.");
    }
    let Some(size_i64) = dict.get::<i64>(b"Size") else {
        return fb("원본 PDF 트레일러에 /Size가 없어 새 PDF로 다시 저장했습니다.");
    };
    if size_i64 < 1 || size_i64 > i64::from(MAX_OBJECTS as u32) {
        return fb("원본 PDF의 /Size가 허용 범위를 벗어나 새 PDF로 다시 저장했습니다.");
    }
    let size = size_i64 as u32;
    if size < max_subsection_end {
        return fb("원본 PDF의 xref 범위가 트레일러 /Size보다 커서 새 PDF로 다시 저장했습니다.");
    }
    if let Some(limit) = latest_size {
        if size > limit {
            return fb("이전 트레일러의 /Size가 최신 /Size보다 커서 새 PDF로 다시 저장했습니다.");
        }
    }

    let root_ref = dict.get_ref(b"Root").map(ObjectIdentifier::from);
    if let Some(root) = root_ref {
        if root.obj_number == 0 || root.obj_number >= latest_size.unwrap_or(size) {
            return fb(
                "원본 PDF의 /Root 객체 번호가 /Size 범위를 벗어나 새 PDF로 다시 저장했습니다.",
            );
        }
    }

    let info_ref = dict
        .get_ref(b"Info")
        .map(ObjectIdentifier::from)
        .filter(|id| id.obj_number > 0 && id.obj_number < latest_size.unwrap_or(size));

    let prev = if has_non_null_entry(dict, b"Prev") {
        let Some(p) = dict.get::<i64>(b"Prev") else {
            return fb("원본 PDF의 /Prev 오프셋이 올바르지 않아 새 PDF로 다시 저장했습니다.");
        };
        if p < 0 || p >= total_len as i64 {
            return fb("원본 PDF의 /Prev 오프셋이 파일 범위를 벗어나 새 PDF로 다시 저장했습니다.");
        }
        Some(p as u64)
    } else {
        None
    };

    let id_entry = if read_id {
        dict.get::<Array<'_>>(b"ID").and_then(|arr| {
            let items: Vec<MaybeRef<Object<'_>>> = arr.raw_iter().take(3).collect();
            if items.len() != 2 {
                return None;
            }
            let mut budget = Budget::default();
            let a = IncVal::read_raw(&items[0], &mut budget).ok()?;
            let b = IncVal::read_raw(&items[1], &mut budget).ok()?;
            matches!((&a, &b), (IncVal::Str(_), IncVal::Str(_)))
                .then_some(IncVal::Array(vec![a, b]))
        })
    } else {
        None
    };

    Ok(SectionParse::Ok(SectionInfo {
        size,
        root_ref,
        info_ref,
        id_entry,
        prev,
    }))
}

fn has_non_null_entry(dict: &Dict<'_>, key: &[u8]) -> bool {
    dict.get_raw::<Object<'_>>(key)
        .is_some_and(|raw| !matches!(raw, MaybeRef::NotRef(Object::Null(_))))
}

fn verify_indirect_obj_header(
    bytes: &[u8],
    offset: usize,
    expected_obj: u32,
    expected_gen: u16,
) -> bool {
    if offset >= bytes.len() {
        return false;
    }
    let mut pos = offset;
    skip_ws(bytes, &mut pos);
    let Some(got_obj) = read_u64(bytes, &mut pos) else {
        return false;
    };
    if got_obj != u64::from(expected_obj) {
        return false;
    }
    let ws1 = pos;
    skip_ws(bytes, &mut pos);
    if pos == ws1 {
        return false;
    }
    let Some(got_gen) = read_u64(bytes, &mut pos) else {
        return false;
    };
    if got_gen != u64::from(expected_gen) {
        return false;
    }
    let ws2 = pos;
    skip_ws(bytes, &mut pos);
    pos > ws2 && starts_with_word_or_delim(bytes, pos, b"obj")
}

fn read_be_u64(slice: &[u8]) -> u64 {
    let mut acc = 0u64;
    for &b in slice {
        acc = (acc << 8) | u64::from(b);
    }
    acc
}

fn parse_ascii_usize(slice: &[u8]) -> Option<usize> {
    let mut acc = 0usize;
    for &b in slice {
        if !b.is_ascii_digit() {
            return None;
        }
        acc = acc.checked_mul(10)?.checked_add(usize::from(b - b'0'))?;
    }
    Some(acc)
}

fn parse_ascii_u16(slice: &[u8]) -> Option<u16> {
    let v = parse_ascii_usize(slice)?;
    u16::try_from(v).ok()
}

fn skip_ws(bytes: &[u8], pos: &mut usize) {
    while *pos < bytes.len() && is_pdf_ws(bytes[*pos]) {
        *pos += 1;
    }
}

fn skip_horiz_ws(bytes: &[u8], pos: &mut usize) {
    while *pos < bytes.len() && matches!(bytes[*pos], b' ' | b'\t') {
        *pos += 1;
    }
}

fn starts_with_word(bytes: &[u8], pos: usize, word: &[u8]) -> bool {
    let Some(rest) = bytes.get(pos..) else {
        return false;
    };
    rest.starts_with(word) && rest.get(word.len()).is_none_or(|&b| is_pdf_ws(b))
}

fn starts_with_word_or_delim(bytes: &[u8], pos: usize, word: &[u8]) -> bool {
    let Some(rest) = bytes.get(pos..) else {
        return false;
    };
    rest.starts_with(word)
        && rest
            .get(word.len())
            .is_none_or(|&b| is_pdf_ws(b) || is_pdf_delim(b))
}

fn read_u64(bytes: &[u8], pos: &mut usize) -> Option<u64> {
    let start = *pos;
    let mut val = 0u64;
    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        val = val
            .checked_mul(10)?
            .checked_add(u64::from(bytes[*pos] - b'0'))?;
        *pos += 1;
    }
    (*pos > start).then_some(val)
}

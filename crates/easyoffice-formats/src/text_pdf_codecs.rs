#[path = "pdf_object_graph.rs"]
mod pdf_object_graph;

use crate::pdf_stream_parser::{
    advance_past_stream_eol, contains_subslice, decode_base64_bytes_impl, escape_pdf_text,
    is_pdf_delim_or_ws, is_standalone_pdf_kw, parse_indirect_obj_header,
    parse_pdf_content_stream_with_cmap, skip_pdf_ws_and_comments, span_crosses_object_boundary,
    step_pdf_non_code_token, unescape_pdf_text, validate_balanced_pdf_literals, ParenJumpTable,
    PdfNonCodeKind, PdfNonCodeStep, PdfToUnicodeMap,
};
use crate::{ContainerFamily, DocumentFormat, EasyOfficeError, FormatDetector};
use pdf_object_graph::{
    decode_and_validate_pdf_streams, dict_get, dict_has_non_null_key, dict_name_eq,
    expand_pdf_object_streams, fail_corrupted_or_encrypted, indirect_val_has_spliced_encrypt,
    parse_pdf_value_with_table, resolve_catalog_and_pages_target, resolve_int,
    resolve_stream_bounds, resolve_stream_length_hint, resolve_value,
    trailer_or_xref_has_spliced_encrypt, validate_pdf_trailer_json, IntObjIndex, PageNodeTarget,
    PdfIndirectObject, PdfObjValue, MAX_PDF_CONTENTS_ARRAY_LEN, MAX_PDF_PAGE_TREE_DEPTH,
    MAX_PDF_TOTAL_DECODED_BYTES,
};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

pub use crate::csv_markdown_codecs::{CommonMarkCodec, CsvCodec, MarkdownBlock};

const MAX_PDF_PAGE_TREE_NODES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfPageModel {
    pub page_number: usize,
    pub rotation_degrees: i32,
    pub body_text: String,
    pub approval_stamp: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfDocumentModel {
    pub title: String,
    pub pages: Vec<PdfPageModel>,
    pub watermark_text: Option<String>,
}

struct ScannedPdfStructure {
    has_encrypt: bool,
    title: String,
    watermark_text: Option<String>,
    pages: Vec<PdfPageModel>,
    trailer_b64: Option<String>,
    trailer_b64_count: usize,
}

/// Objects sorted by their order of appearance in the file.
fn ordered_by_sequence(
    objects: &HashMap<(u32, u16), PdfIndirectObject>,
) -> Vec<((u32, u16), &PdfIndirectObject)> {
    let mut ordered: Vec<((u32, u16), &PdfIndirectObject)> =
        objects.iter().map(|(&id, o)| (id, o)).collect();
    ordered.sort_by_key(|(_, o)| o.order_seq);
    ordered
}

/// Merges every `/ToUnicode` CMap stream found among the objects.
fn build_tounicode_map(
    bytes: &[u8],
    ordered_objs: &[((u32, u16), &PdfIndirectObject)],
    flate_cache: &HashMap<(u32, u16), Vec<u8>>,
) -> PdfToUnicodeMap {
    let mut cmap = PdfToUnicodeMap::default();
    for &(id, obj) in ordered_objs {
        let stream_slice: Option<&[u8]> = match flate_cache.get(&id) {
            Some(dec) => Some(dec.as_slice()),
            None => obj.stream_range.map(|(s, e)| &bytes[s..e]),
        };
        if let Some(s_bytes) = stream_slice.filter(|s| contains_subslice(s, b"begincmap")) {
            PdfToUnicodeMap::parse_cmap_stream(&String::from_utf8_lossy(s_bytes), &mut cmap);
        }
    }
    cmap
}

/// State of the top-level token scan over a PDF file.
struct PdfTopLevelScan<'a> {
    bytes: &'a [u8],
    paren_table: ParenJumpTable,
    int_index: Option<(Vec<usize>, IntObjIndex)>,
    objects: HashMap<(u32, u16), PdfIndirectObject>,
    trailers: Vec<Vec<(String, PdfObjValue)>>,
    stream_spans: Vec<(usize, usize)>,
    has_encrypt: bool,
    order_seq: usize,
}

impl<'a> PdfTopLevelScan<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            paren_table: ParenJumpTable::build(bytes),
            int_index: None,
            objects: HashMap::new(),
            trailers: Vec::new(),
            stream_spans: Vec::new(),
            has_encrypt: false,
            order_seq: 0,
        }
    }

    fn corrupted(&self, at: usize, has_encrypt: bool, detail: &str) -> EasyOfficeError {
        fail_corrupted_or_encrypted(self.bytes, at, has_encrypt, detail)
    }

    fn run(&mut self) -> Result<(), EasyOfficeError> {
        let bytes = self.bytes;
        let mut p = 0usize;
        while p < bytes.len() {
            p = skip_pdf_ws_and_comments(bytes, p, bytes.len());
            if p >= bytes.len() {
                break;
            }
            p = self.scan_token(p)?;
        }
        Ok(())
    }

    /// Scans the top-level construct starting at `p`; returns the offset after it.
    fn scan_token(&mut self, p: usize) -> Result<usize, EasyOfficeError> {
        let bytes = self.bytes;
        if let Some((obj_num, gen_num, after_obj)) =
            parse_indirect_obj_header(bytes, p, bytes.len())
        {
            return self.scan_indirect_object(p, (obj_num, gen_num), after_obj);
        }
        if is_standalone_pdf_kw(bytes, p, b"xref") {
            return Ok(Self::skip_xref_table(bytes, p + 4));
        }
        if is_standalone_pdf_kw(bytes, p, b"trailer") {
            return self.scan_trailer(p);
        }
        if is_standalone_pdf_kw(bytes, p, b"startxref") {
            return self.scan_startxref(p);
        }
        Err(self.corrupted(
            p,
            self.has_encrypt,
            "PDF 상위 구조에 해석할 수 없는 손상 토큰이 존재합니다.",
        ))
    }

    fn scan_indirect_object(
        &mut self,
        p: usize,
        id: (u32, u16),
        after_obj: usize,
    ) -> Result<usize, EasyOfficeError> {
        let bytes = self.bytes;
        let parsed_val =
            parse_pdf_value_with_table(bytes, Some(&self.paren_table), after_obj, bytes.len(), 0)
                .map_err(|_| {
                self.corrupted(
                    p,
                    self.has_encrypt,
                    "PDF 간접 객체 구문이 손상되었거나 리터럴이 닫히지 않았습니다.",
                )
            })?;
        let (val, mut next_p) = parsed_val.unwrap_or((PdfObjValue::Null, after_obj));
        if let Some(spliced_enc) = indirect_val_has_spliced_encrypt(&val) {
            return Err(self.corrupted(
                p,
                self.has_encrypt || spliced_enc,
                "PDF 상호참조 딕셔너리 구조가 손상되었습니다.",
            ));
        }
        if let PdfObjValue::Dict(ref entries) = val {
            if dict_name_eq(entries, "/Type", "/XRef") && dict_has_non_null_key(entries, "/Encrypt")
            {
                self.has_encrypt = true;
            }
        }
        next_p = skip_pdf_ws_and_comments(bytes, next_p, bytes.len());
        let mut stream_range = None;
        if is_standalone_pdf_kw(bytes, next_p, b"stream") {
            let (range, after_stream) = self.scan_stream_body(p, &val, next_p)?;
            stream_range = Some(range);
            next_p = skip_pdf_ws_and_comments(bytes, after_stream, bytes.len());
        }
        if !is_standalone_pdf_kw(bytes, next_p, b"endobj") {
            return Err(self.corrupted(
                p,
                self.has_encrypt,
                "PDF 간접 객체에 대응하는 endobj 토큰이 누락되었습니다.",
            ));
        }
        self.order_seq += 1;
        self.objects.insert(
            id,
            PdfIndirectObject {
                val,
                stream_range,
                order_seq: self.order_seq,
            },
        );
        Ok(next_p + 6)
    }

    /// Resolves the stream payload range following the object dictionary `val`.
    /// Returns `((start, end), offset after endstream)`.
    fn scan_stream_body(
        &mut self,
        obj_start: usize,
        val: &PdfObjValue,
        stream_kw_pos: usize,
    ) -> Result<((usize, usize), usize), EasyOfficeError> {
        let bytes = self.bytes;
        let PdfObjValue::Dict(entries) = val else {
            return Err(self.corrupted(
                obj_start,
                self.has_encrypt,
                "PDF stream 앞에 객체 딕셔너리가 없습니다.",
            ));
        };
        let stream_start = advance_past_stream_eol(bytes, stream_kw_pos + 6);
        let len_hint = resolve_stream_length_hint(
            bytes,
            stream_start,
            entries,
            &self.objects,
            &self.paren_table,
            &mut self.int_index,
        );
        let Some((stream_end, after_endstream)) =
            resolve_stream_bounds(bytes, stream_start, len_hint)
        else {
            return Err(self.corrupted(
                obj_start,
                self.has_encrypt,
                "PDF stream 블록에 대응하는 endstream 토큰이 누락되었거나 경계가 손상되었습니다.",
            ));
        };
        self.stream_spans.push((stream_kw_pos, after_endstream));
        Ok(((stream_start, stream_end), after_endstream))
    }

    /// Skips cross-reference table tokens (`n`, `f` and digits); returns the offset after them.
    fn skip_xref_table(bytes: &[u8], mut p: usize) -> usize {
        loop {
            p = skip_pdf_ws_and_comments(bytes, p, bytes.len());
            if p >= bytes.len() || parse_indirect_obj_header(bytes, p, bytes.len()).is_some() {
                return p;
            }
            let mut k = p;
            while k < bytes.len() && !is_pdf_delim_or_ws(bytes[k]) {
                k += 1;
            }
            let tok = &bytes[p..k];
            let is_xref_token = tok == b"n" || tok == b"f" || tok.iter().all(u8::is_ascii_digit);
            if k == p || !is_xref_token {
                return p;
            }
            p = k;
        }
    }

    fn scan_trailer(&mut self, p: usize) -> Result<usize, EasyOfficeError> {
        let bytes = self.bytes;
        let parsed_tr =
            parse_pdf_value_with_table(bytes, Some(&self.paren_table), p + 7, bytes.len(), 0)
                .map_err(|_| {
                    self.corrupted(
                        p,
                        self.has_encrypt,
                        "PDF trailer 딕셔너리가 손상되었습니다.",
                    )
                })?;
        let Some((PdfObjValue::Dict(tr_entries), next_p)) = parsed_tr else {
            return Err(self.corrupted(
                p,
                self.has_encrypt,
                "PDF trailer 뒤에 딕셔너리가 없습니다.",
            ));
        };
        let tr_has_enc = dict_has_non_null_key(&tr_entries, "/Encrypt");
        if let Some(spliced_enc) = trailer_or_xref_has_spliced_encrypt(&tr_entries, tr_has_enc) {
            return Err(self.corrupted(
                p,
                self.has_encrypt || tr_has_enc || spliced_enc,
                "PDF trailer /ID 배열 구조가 손상되었습니다.",
            ));
        }
        self.has_encrypt |= tr_has_enc;
        self.trailers.push(tr_entries);
        Ok(next_p)
    }

    fn scan_startxref(&mut self, sx_pos: usize) -> Result<usize, EasyOfficeError> {
        let bytes = self.bytes;
        let start_off = skip_pdf_ws_and_comments(bytes, sx_pos + 9, bytes.len());
        let mut p = start_off;
        while p < bytes.len() && bytes[p].is_ascii_digit() {
            p += 1;
        }
        if p == start_off {
            return Err(self.corrupted(
                sx_pos,
                self.has_encrypt,
                "PDF startxref 오프셋 숫자가 누락되었습니다.",
            ));
        }
        Ok(p)
    }
}

fn unclosed_literal_error() -> EasyOfficeError {
    EasyOfficeError::CorruptedContainer(
        "PDF 페이지 콘텐츠 스트림의 문자열 리터럴이 닫히지 않았습니다.".to_string(),
    )
}

fn content_budget_error() -> EasyOfficeError {
    EasyOfficeError::SecurityViolation(
        "PDF 페이지 콘텐츠 스트림 누적 바이트 예산(32MiB)을 초과했습니다.".to_string(),
    )
}

/// Resolves one content stream to its decoded (or raw) bytes, charging the shared byte budget.
fn resolve_content_slice<'a>(
    state: &mut PageWalkState<'a>,
    so: u32,
    sg: u16,
    s_start: usize,
    s_end: usize,
) -> Result<&'a [u8], EasyOfficeError> {
    if let Some(decoded) = state.flate_cache.get(&(so, sg)) {
        let slice = decoded.as_slice();
        if !validate_balanced_pdf_literals(slice) {
            return Err(unclosed_literal_error());
        }
        if !state.first_used_flate.insert((so, sg)) {
            state.remaining_budget = state
                .remaining_budget
                .checked_sub(slice.len())
                .ok_or_else(content_budget_error)?;
        }
        return Ok(slice);
    }
    let slice = &state.bytes[s_start..s_end];
    if !validate_balanced_pdf_literals(slice) {
        return Err(unclosed_literal_error());
    }
    if span_crosses_object_boundary(state.bytes, s_start, s_end) {
        return Err(EasyOfficeError::CorruptedContainer(
            "PDF 페이지 스트림 내부에 닫히지 않은 객체 경계 토큰이 감지되었습니다.".to_string(),
        ));
    }
    state.remaining_budget = state
        .remaining_budget
        .checked_sub(slice.len())
        .ok_or_else(content_budget_error)?;
    Ok(slice)
}

/// Appends the stream behind the `/Contents` reference `(so, sg)` to `out`.
fn push_content_ref<'a>(
    state: &mut PageWalkState<'a>,
    out: &mut Vec<&'a [u8]>,
    so: u32,
    sg: u16,
) -> Result<(), EasyOfficeError> {
    let Some(stream_obj) = state.objects.get(&(so, sg)) else {
        return Err(EasyOfficeError::CorruptedContainer(
            "PDF /Contents 간접 스트림 객체를 찾을 수 없습니다.".to_string(),
        ));
    };
    let Some((s_start, s_end)) = stream_obj.stream_range else {
        return Err(EasyOfficeError::CorruptedContainer(
            "PDF /Contents 대상 객체에 stream 본문이 없습니다.".to_string(),
        ));
    };
    if !matches!(stream_obj.val, PdfObjValue::Dict(_)) {
        return Err(EasyOfficeError::CorruptedContainer(
            "PDF 스트림 객체 헤더가 딕셔너리가 아닙니다.".to_string(),
        ));
    }
    out.push(resolve_content_slice(state, so, sg, s_start, s_end)?);
    Ok(())
}

/// Streams of a `/Contents` array (null entries skipped, element count capped).
fn collect_contents_array<'a>(
    state: &mut PageWalkState<'a>,
    arr: &[PdfObjValue],
) -> Result<Vec<&'a [u8]>, EasyOfficeError> {
    if arr.len() > MAX_PDF_CONTENTS_ARRAY_LEN {
        return Err(EasyOfficeError::CorruptedContainer(
            "PDF /Contents 배열 원소 상한(256)을 초과했습니다.".to_string(),
        ));
    }
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        match item {
            PdfObjValue::Ref(so, sg) => push_content_ref(state, &mut out, *so, *sg)?,
            PdfObjValue::Null => {}
            _ => {
                return Err(EasyOfficeError::CorruptedContainer(
                    "PDF /Contents 배열 원소가 스트림 간접 참조가 아닙니다.".to_string(),
                ))
            }
        }
    }
    Ok(out)
}

struct PageWalkState<'a> {
    bytes: &'a [u8],
    objects: &'a HashMap<(u32, u16), PdfIndirectObject>,
    flate_cache: &'a HashMap<(u32, u16), Vec<u8>>,
    cmap: &'a PdfToUnicodeMap,
    first_used_flate: HashSet<(u32, u16)>,
    remaining_budget: usize,
    visited: HashSet<(u32, u16)>,
    node_count: usize,
    pages: Vec<PdfPageModel>,
    watermark_text: Option<String>,
}

pub struct PdfSerializer;

impl PdfSerializer {
    pub fn parse(bytes: &[u8]) -> Result<PdfDocumentModel, EasyOfficeError> {
        let detected = FormatDetector::detect(bytes, Some("pdf"))?;
        if detected != DocumentFormat::Pdf {
            return Err(EasyOfficeError::SignatureMismatch {
                expected: ContainerFamily::PdfIso32000,
                detail: "유효한 PDF 시그니처가 아닙니다.".to_string(),
            });
        }
        if (!contains_subslice(bytes, b"xref") && !contains_subslice(bytes, b"/XRef"))
            || !contains_subslice(bytes, b"%%EOF")
        {
            return Err(fail_corrupted_or_encrypted(
                bytes,
                0,
                false,
                "PDF xref 테이블 또는 %%EOF 트레일러가 누락되었습니다.",
            ));
        }

        let mut scanned = Self::scan_pdf_structure(bytes)?;
        if scanned.has_encrypt {
            return Err(EasyOfficeError::EncryptedDocumentUnsupported(
                DocumentFormat::Pdf,
            ));
        }
        if scanned.pages.is_empty() {
            return Err(fail_corrupted_or_encrypted(
                bytes,
                0,
                false,
                "PDF 페이지 스트림을 찾을 수 없습니다.",
            ));
        }
        if scanned.trailer_b64_count > 1 {
            return Err(EasyOfficeError::SecurityViolation(
                "PDF 트레일러 주석(%EasyOfficePDFModelBase64)이 중복 주입되었습니다.".to_string(),
            ));
        }
        if let Some(b64) = scanned.trailer_b64 {
            if b64.is_empty() {
                return Err(EasyOfficeError::SecurityViolation(
                    "PDF 트레일러 주석(%EasyOfficePDFModelBase64)이 비어 있습니다.".to_string(),
                ));
            }
            let Some(decoded) = Self::decode_base64_utf8(&b64) else {
                return Err(EasyOfficeError::SecurityViolation(
                    "PDF 트레일러 주석(%EasyOfficePDFModelBase64)의 Base64/UTF-8 디코딩에 실패했습니다."
                        .to_string(),
                ));
            };
            let page_body_refs: Vec<&str> =
                scanned.pages.iter().map(|p| p.body_text.as_str()).collect();
            let Some(validated_title) = validate_pdf_trailer_json(
                &decoded,
                &scanned.title,
                &page_body_refs,
                scanned.watermark_text.as_deref(),
            ) else {
                return Err(EasyOfficeError::SecurityViolation(
                    "PDF 트레일러 주석(%EasyOfficePDFModelBase64)이 본문 스트림과 일치하지 않습니다."
                        .to_string(),
                ));
            };
            scanned.title = validated_title;
        }

        Ok(PdfDocumentModel {
            title: scanned.title,
            pages: scanned.pages,
            watermark_text: scanned.watermark_text,
        })
    }

    fn collect_outside_stream_b64_comments(
        bytes: &[u8],
        stream_spans: &[(usize, usize)],
        trailer_b64: &mut Option<String>,
        trailer_b64_count: &mut usize,
    ) {
        let mut cursor = 0usize;
        for &(s_start, s_end) in stream_spans {
            if cursor < s_start {
                Self::scan_segment_b64_comments(
                    &bytes[cursor..s_start],
                    trailer_b64,
                    trailer_b64_count,
                );
            }
            cursor = cursor.max(s_end);
        }
        if cursor < bytes.len() {
            Self::scan_segment_b64_comments(&bytes[cursor..], trailer_b64, trailer_b64_count);
        }
    }

    fn scan_segment_b64_comments(
        seg: &[u8],
        trailer_b64: &mut Option<String>,
        trailer_b64_count: &mut usize,
    ) {
        let mut i = 0usize;
        while i < seg.len() {
            match step_pdf_non_code_token(seg, i) {
                Ok(Some(PdfNonCodeStep {
                    next_index,
                    kind: PdfNonCodeKind::Comment { start, end },
                })) => {
                    let comment_line = String::from_utf8_lossy(&seg[start..end]);
                    if let Some(rest) = comment_line
                        .trim()
                        .strip_prefix("%EasyOfficePDFModelBase64:")
                    {
                        *trailer_b64_count += 1;
                        *trailer_b64 = Some(rest.trim().to_string());
                    }
                    i = next_index;
                }
                Ok(Some(step)) => i = step.next_index,
                Ok(None) | Err(()) => i += 1,
            }
        }
    }

    fn collect_page_content_slices<'a>(
        state: &mut PageWalkState<'a>,
        page_entries: &[(String, PdfObjValue)],
        inline_stream: Option<((u32, u16), (usize, usize))>,
    ) -> Result<Vec<&'a [u8]>, EasyOfficeError> {
        match dict_get(page_entries, "/Contents") {
            None | Some(PdfObjValue::Null) => match inline_stream {
                Some(((so, sg), (s_start, s_end))) => {
                    Ok(vec![resolve_content_slice(state, so, sg, s_start, s_end)?])
                }
                None => Ok(Vec::new()),
            },
            Some(PdfObjValue::Ref(co, cg)) => {
                let Some(target) = state.objects.get(&(*co, *cg)) else {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "PDF /Contents 간접 참조 대상을 찾을 수 없습니다.".to_string(),
                    ));
                };
                if target.stream_range.is_some() {
                    let mut out = Vec::with_capacity(1);
                    push_content_ref(state, &mut out, *co, *cg)?;
                    Ok(out)
                } else if let PdfObjValue::Array(ref arr) = target.val {
                    collect_contents_array(state, arr)
                } else {
                    Err(EasyOfficeError::CorruptedContainer(
                        "PDF /Contents 참조 대상이 스트림 또는 배열이 아닙니다.".to_string(),
                    ))
                }
            }
            Some(PdfObjValue::Array(arr)) => collect_contents_array(state, arr),
            Some(_) => Err(EasyOfficeError::CorruptedContainer(
                "PDF /Page /Contents 항목 타입이 올바르지 않습니다.".to_string(),
            )),
        }
    }

    fn emit_page_from_node<'a>(
        state: &mut PageWalkState<'a>,
        page_entries: &[(String, PdfObjValue)],
        inline_stream: Option<((u32, u16), (usize, usize))>,
        inherited_rotation: i32,
    ) -> Result<(), EasyOfficeError> {
        let rotation = dict_get(page_entries, "/Rotate")
            .and_then(|v| resolve_int(v, state.objects))
            .and_then(|n| i32::try_from(n).ok())
            .unwrap_or(inherited_rotation);
        let page_number = dict_get(page_entries, "/PageNumber")
            .and_then(|v| resolve_int(v, state.objects))
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(state.pages.len() + 1);
        let slices = Self::collect_page_content_slices(state, page_entries, inline_stream)?;
        if slices.is_empty() {
            state.pages.push(PdfPageModel {
                page_number,
                rotation_degrees: rotation,
                body_text: String::new(),
                approval_stamp: None,
            });
            return Ok(());
        }
        let merged_cow: Cow<'_, [u8]> = if slices.len() == 1 {
            Cow::Borrowed(slices[0])
        } else {
            let joiners = slices.len() - 1;
            let Some(next_b) = state.remaining_budget.checked_sub(joiners) else {
                return Err(EasyOfficeError::SecurityViolation(
                    "PDF 페이지 콘텐츠 스트림 누적 바이트 예산(32MiB)을 초과했습니다.".to_string(),
                ));
            };
            state.remaining_budget = next_b;
            let total_len: usize = slices.iter().map(|s| s.len()).sum::<usize>() + joiners;
            let mut joined = Vec::with_capacity(total_len);
            for (idx, chunk) in slices.iter().enumerate() {
                if idx > 0 {
                    joined.push(b'\n');
                }
                joined.extend_from_slice(chunk);
            }
            Cow::Owned(joined)
        };
        let stream_text = String::from_utf8_lossy(merged_cow.as_ref());
        let parsed = parse_pdf_content_stream_with_cmap(&stream_text, Some(state.cmap));
        if let Some(wm) = parsed.watermark {
            state.watermark_text = Some(wm);
        }
        state.pages.push(PdfPageModel {
            page_number,
            rotation_degrees: rotation,
            body_text: parsed.body_text,
            approval_stamp: parsed.stamp,
        });
        Ok(())
    }

    fn walk_page_tree_node<'a>(
        state: &mut PageWalkState<'a>,
        target: PageNodeTarget<'a>,
        inherited_rotation: i32,
        depth: usize,
    ) -> Result<(), EasyOfficeError> {
        if depth > MAX_PDF_PAGE_TREE_DEPTH {
            return Err(EasyOfficeError::CorruptedContainer(
                "PDF 페이지 트리 중첩 깊이 상한(64)을 초과했습니다.".to_string(),
            ));
        }
        state.node_count += 1;
        if state.node_count > MAX_PDF_PAGE_TREE_NODES {
            return Err(EasyOfficeError::CorruptedContainer(
                "PDF 페이지 트리 노드 상한(4096)을 초과했습니다.".to_string(),
            ));
        }
        let (entries, inline_stream) = match target {
            PageNodeTarget::Ref(on, gn) => {
                if !state.visited.insert((on, gn)) {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "PDF 페이지 트리에서 순환 참조 또는 중복 노드가 감지되었습니다."
                            .to_string(),
                    ));
                }
                let Some(obj) = state.objects.get(&(on, gn)) else {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "PDF 페이지 트리 간접 객체를 찾을 수 없습니다.".to_string(),
                    ));
                };
                let PdfObjValue::Dict(ref d) = obj.val else {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "PDF 페이지 트리 노드가 딕셔너리가 아닙니다.".to_string(),
                    ));
                };
                (d.as_slice(), obj.stream_range.map(|r| ((on, gn), r)))
            }
            PageNodeTarget::Inline(d) => (d, None),
        };

        let effective_rotation = dict_get(entries, "/Rotate")
            .and_then(|v| resolve_int(v, state.objects))
            .and_then(|n| i32::try_from(n).ok())
            .unwrap_or(inherited_rotation);

        let is_pages = dict_name_eq(entries, "/Type", "/Pages")
            || (dict_get(entries, "/Type").is_none() && dict_get(entries, "/Kids").is_some());
        if is_pages {
            let Some(kids_raw) = dict_get(entries, "/Kids") else {
                return Err(EasyOfficeError::CorruptedContainer(
                    "PDF /Pages 노드에 /Kids 배열이 누락되었습니다.".to_string(),
                ));
            };
            let Some(PdfObjValue::Array(kids)) = resolve_value(kids_raw, state.objects) else {
                return Err(EasyOfficeError::CorruptedContainer(
                    "PDF /Pages /Kids 항목이 배열이 아닙니다.".to_string(),
                ));
            };
            for kid in kids {
                match kid {
                    PdfObjValue::Ref(ko, kg) => Self::walk_page_tree_node(
                        state,
                        PageNodeTarget::Ref(*ko, *kg),
                        effective_rotation,
                        depth + 1,
                    )?,
                    PdfObjValue::Dict(kd) => Self::walk_page_tree_node(
                        state,
                        PageNodeTarget::Inline(kd.as_slice()),
                        effective_rotation,
                        depth + 1,
                    )?,
                    _ => {
                        return Err(EasyOfficeError::CorruptedContainer(
                            "PDF /Kids 원소가 간접 참조 또는 딕셔너리가 아닙니다.".to_string(),
                        ))
                    }
                }
            }
            return Ok(());
        }

        let is_page =
            dict_name_eq(entries, "/Type", "/Page") || dict_get(entries, "/Type").is_none();
        if !is_page {
            return Err(EasyOfficeError::CorruptedContainer(
                "PDF 페이지 트리에 허용되지 않는 객체 /Type이 포함되었습니다.".to_string(),
            ));
        }
        Self::emit_page_from_node(state, entries, inline_stream, effective_rotation)
    }

    fn scan_pdf_structure(bytes: &[u8]) -> Result<ScannedPdfStructure, EasyOfficeError> {
        let mut scan = PdfTopLevelScan::new(bytes);
        scan.run()?;
        if scan.has_encrypt {
            return Ok(ScannedPdfStructure {
                has_encrypt: true,
                title: String::new(),
                watermark_text: None,
                pages: Vec::new(),
                trailer_b64: None,
                trailer_b64_count: 0,
            });
        }
        let PdfTopLevelScan {
            mut objects,
            trailers,
            stream_spans,
            mut order_seq,
            ..
        } = scan;
        let mut trailer_b64: Option<String> = None;
        let mut trailer_b64_count = 0usize;

        Self::collect_outside_stream_b64_comments(
            bytes,
            &stream_spans,
            &mut trailer_b64,
            &mut trailer_b64_count,
        );

        let mut remaining_budget = MAX_PDF_TOTAL_DECODED_BYTES;
        let flate_cache = {
            let ordered_objs = ordered_by_sequence(&objects);
            decode_and_validate_pdf_streams(
                bytes,
                &objects,
                &ordered_objs,
                &mut remaining_budget,
                &mut trailer_b64,
                &mut trailer_b64_count,
            )?
        };

        expand_pdf_object_streams(bytes, &mut objects, &flate_cache, &mut order_seq);
        let ordered_objs = ordered_by_sequence(&objects);
        let cmap = build_tounicode_map(bytes, &ordered_objs, &flate_cache);
        let (title, pages_target) =
            resolve_catalog_and_pages_target(&objects, &ordered_objs, &trailers);

        let mut walk_state = PageWalkState {
            bytes,
            objects: &objects,
            flate_cache: &flate_cache,
            cmap: &cmap,
            first_used_flate: HashSet::new(),
            remaining_budget,
            visited: HashSet::new(),
            node_count: 0,
            pages: Vec::new(),
            watermark_text: None,
        };
        match pages_target {
            Some(target) => Self::walk_page_tree_node(&mut walk_state, target, 0, 0)?,
            None => Self::emit_unlinked_pages(&mut walk_state, &ordered_objs)?,
        }

        Ok(ScannedPdfStructure {
            has_encrypt: false,
            title,
            watermark_text: walk_state.watermark_text,
            pages: walk_state.pages,
            trailer_b64,
            trailer_b64_count,
        })
    }

    /// Emits every `/Page` object in file order when no page tree root was found.
    fn emit_unlinked_pages(
        walk_state: &mut PageWalkState<'_>,
        ordered_objs: &[((u32, u16), &PdfIndirectObject)],
    ) -> Result<(), EasyOfficeError> {
        for &(id, obj) in ordered_objs {
            let PdfObjValue::Dict(ref d) = obj.val else {
                continue;
            };
            if !dict_name_eq(d, "/Type", "/Page") {
                continue;
            }
            walk_state.node_count += 1;
            if walk_state.node_count > MAX_PDF_PAGE_TREE_NODES {
                return Err(EasyOfficeError::CorruptedContainer(
                    "PDF 페이지 트리 노드 상한(4096)을 초과했습니다.".to_string(),
                ));
            }
            Self::emit_page_from_node(walk_state, d, obj.stream_range.map(|r| (id, r)), 0)?;
        }
        Ok(())
    }

    pub fn decode_base64_bytes(input: &str) -> Option<Vec<u8>> {
        decode_base64_bytes_impl(input)
    }

    fn decode_base64_utf8(input: &str) -> Option<String> {
        Self::decode_base64_bytes(input).and_then(|bytes| String::from_utf8(bytes).ok())
    }
    pub fn escape_pdf_text(text: &str) -> String {
        escape_pdf_text(text)
    }

    pub fn unescape_pdf_text(text: &str) -> String {
        unescape_pdf_text(text)
    }
}

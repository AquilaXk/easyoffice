#[path = "pdf_object_graph.rs"]
mod pdf_object_graph;

use crate::pdf_stream_parser::{
    advance_past_stream_eol, contains_subslice, escape_pdf_text, is_pdf_delim_or_ws,
    is_standalone_pdf_kw, parse_indirect_obj_header, parse_pdf_content_stream,
    skip_pdf_ws_and_comments, span_crosses_object_boundary, step_pdf_non_code_token,
    unescape_pdf_text, validate_balanced_pdf_literals, validate_pdf_trailer_json, PdfNonCodeKind,
    PdfNonCodeStep,
};
use crate::{ContainerFamily, DocumentFormat, EasyOfficeError, FormatDetector};
use pdf_object_graph::{
    decode_and_validate_pdf_streams, dict_get, dict_has_non_null_key, dict_name_eq,
    fail_corrupted_or_encrypted, parse_pdf_value, resolve_catalog_and_pages_target, resolve_int,
    resolve_stream_bounds, resolve_stream_length_hint, resolve_value, PageNodeTarget,
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

struct PageWalkState<'a> {
    bytes: &'a [u8],
    objects: &'a HashMap<(u32, u16), PdfIndirectObject>,
    flate_cache: &'a HashMap<(u32, u16), Vec<u8>>,
    first_used_flate: HashSet<(u32, u16)>,
    remaining_budget: usize,
    visited: HashSet<(u32, u16)>,
    node_count: usize,
    pages: Vec<PdfPageModel>,
    watermark_text: Option<String>,
}

pub struct PdfSerializer;

impl PdfSerializer {
    pub fn serialize(doc: &PdfDocumentModel) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::with_capacity(2048);
        let push_line = |buf: &mut Vec<u8>, line: &str| {
            buf.extend_from_slice(line.as_bytes());
            buf.push(b'\n');
        };

        push_line(&mut out, "%PDF-1.4");

        const MAX_LINES_PER_PDF_PAGE: usize = 42;
        let source_pages: Vec<PdfPageModel> = if doc.pages.is_empty() {
            vec![PdfPageModel {
                page_number: 1,
                rotation_degrees: 0,
                body_text: String::new(),
                approval_stamp: None,
            }]
        } else {
            doc.pages.clone()
        };
        let mut effective_pages: Vec<PdfPageModel> = Vec::with_capacity(source_pages.len());
        for page in source_pages {
            let lines: Vec<&str> = page.body_text.split('\n').collect();
            if lines.len() <= MAX_LINES_PER_PDF_PAGE {
                effective_pages.push(page);
            } else {
                for chunk in lines.chunks(MAX_LINES_PER_PDF_PAGE) {
                    effective_pages.push(PdfPageModel {
                        page_number: effective_pages.len() + 1,
                        rotation_degrees: page.rotation_degrees,
                        body_text: chunk.join("\n"),
                        approval_stamp: page.approval_stamp.clone(),
                    });
                }
            }
        }
        let page_count = effective_pages.len();
        let page_obj_ids: Vec<usize> = (0..page_count).map(|i| 3 + i * 2).collect();
        let kids_refs = page_obj_ids
            .iter()
            .map(|id| format!("{id} 0 R"))
            .collect::<Vec<_>>()
            .join(" ");

        let cmap_obj = 3 + page_count * 2;
        let total_objects = cmap_obj + 1;
        let mut offsets: Vec<usize> = vec![0; total_objects];

        offsets[1] = out.len();
        push_line(&mut out, "1 0 obj");
        push_line(
            &mut out,
            &format!(
                "<< /Type /Catalog /Pages 2 0 R /Title ({}) >>",
                Self::escape_pdf_text(&doc.title)
            ),
        );
        push_line(&mut out, "endobj");

        offsets[2] = out.len();
        push_line(&mut out, "2 0 obj");
        push_line(
            &mut out,
            &format!("<< /Type /Pages /Count {page_count} /Kids [{kids_refs}] >>"),
        );
        push_line(&mut out, "endobj");

        for (idx, page) in effective_pages.iter().enumerate() {
            let page_obj = 3 + idx * 2;
            let content_obj = page_obj + 1;

            let mut stream_lines: Vec<String> = Vec::new();
            for (l_idx, line_str) in page.body_text.split('\n').enumerate() {
                let y = 760 - (l_idx as i32) * 16;
                let (font_tag, operand) = Self::format_pdf_tj_operand(line_str);
                stream_lines.push(format!("BT {font_tag} 12 Tf 54 {y} Td {operand} Tj ET"));
            }
            if let Some(stamp) = &page.approval_stamp {
                let stamp_text = format!("[STAMP] {stamp}");
                let (font_tag, operand) = Self::format_pdf_tj_operand(&stamp_text);
                stream_lines.push(format!("BT {font_tag} 10 Tf 386 604 Td {operand} Tj ET"));
            }
            if let Some(wm) = &doc.watermark_text {
                let wm_text = format!("[WM] {wm}");
                let (font_tag, operand) = Self::format_pdf_tj_operand(&wm_text);
                stream_lines.push(format!("BT {font_tag} 10 Tf 320 746 Td {operand} Tj ET"));
            }
            let stream_body = stream_lines.join("\n");

            offsets[page_obj] = out.len();
            push_line(&mut out, &format!("{page_obj} 0 obj"));
            push_line(
                &mut out,
                &format!(
                    "<< /Type /Page /Parent 2 0 R /PageNumber {} /Rotate {} /MediaBox [0 0 595 842] /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> /F2 << /Type /Font /Subtype /Type0 /BaseFont /EasyOfficeGothic-Medium /Encoding /Identity-H /DescendantFonts [ << /Type /Font /Subtype /CIDFontType2 /BaseFont /EasyOfficeGothic-Medium /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /DW 1000 >> ] /ToUnicode {cmap_obj} 0 R >> >> >> /Contents {content_obj} 0 R >>",
                    page.page_number, page.rotation_degrees
                ),
            );
            push_line(&mut out, "endobj");

            offsets[content_obj] = out.len();
            push_line(&mut out, &format!("{content_obj} 0 obj"));
            push_line(&mut out, &format!("<< /Length {} >>", stream_body.len()));
            push_line(&mut out, "stream");
            push_line(&mut out, &stream_body);
            push_line(&mut out, "endstream");
            push_line(&mut out, "endobj");
        }

        let cmap_stream = "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n1 beginbfrange\n<0000> <FFFF> <0000>\nendbfrange\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend";
        offsets[cmap_obj] = out.len();
        push_line(&mut out, &format!("{cmap_obj} 0 obj"));
        push_line(
            &mut out,
            &format!(
                "<< /Type /CMap /CMapName /Adobe-Identity-UCS /Length {} >>",
                cmap_stream.len()
            ),
        );
        push_line(&mut out, "stream");
        push_line(&mut out, cmap_stream);
        push_line(&mut out, "endstream");
        push_line(&mut out, "endobj");

        let xref_offset = out.len();
        push_line(&mut out, "xref");
        push_line(&mut out, &format!("0 {total_objects}"));
        push_line(&mut out, "0000000000 65535 f ");
        for offset in offsets.iter().skip(1) {
            push_line(&mut out, &format!("{offset:010} 00000 n "));
        }
        push_line(&mut out, "trailer");
        push_line(
            &mut out,
            &format!("<< /Size {total_objects} /Root 1 0 R >>"),
        );
        push_line(&mut out, "startxref");
        push_line(&mut out, &xref_offset.to_string());
        push_line(&mut out, "%%EOF");

        out
    }

    pub fn parse(bytes: &[u8]) -> Result<PdfDocumentModel, EasyOfficeError> {
        let detected = FormatDetector::detect(bytes, Some("pdf"))?;
        if detected != DocumentFormat::Pdf {
            return Err(EasyOfficeError::SignatureMismatch {
                expected: ContainerFamily::PdfIso32000,
                detail: "유효한 PDF 시그니처가 아닙니다.".to_string(),
            });
        }
        if !contains_subslice(bytes, b"xref") || !contains_subslice(bytes, b"%%EOF") {
            return Err(fail_corrupted_or_encrypted(
                bytes,
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
            return Err(EasyOfficeError::CorruptedContainer(
                "PDF 페이지 스트림을 찾을 수 없습니다.".to_string(),
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
        let mut resolve_one =
            |so: u32, sg: u16, s_start: usize, s_end: usize| -> Result<&'a [u8], EasyOfficeError> {
                if let Some(decoded) = state.flate_cache.get(&(so, sg)) {
                    let slice = decoded.as_slice();
                    if !validate_balanced_pdf_literals(slice) {
                        return Err(EasyOfficeError::CorruptedContainer(
                            "PDF 페이지 콘텐츠 스트림의 문자열 리터럴이 닫히지 않았습니다."
                                .to_string(),
                        ));
                    }
                    if !state.first_used_flate.insert((so, sg)) {
                        let Some(next_b) = state.remaining_budget.checked_sub(slice.len()) else {
                            return Err(EasyOfficeError::SecurityViolation(
                                "PDF 페이지 콘텐츠 스트림 누적 바이트 예산(32MiB)을 초과했습니다."
                                    .to_string(),
                            ));
                        };
                        state.remaining_budget = next_b;
                    }
                    Ok(slice)
                } else {
                    if span_crosses_object_boundary(state.bytes, s_start, s_end) {
                        return Err(EasyOfficeError::CorruptedContainer(
                            "PDF 페이지 스트림 내부에 닫히지 않은 객체 경계 토큰이 감지되었습니다."
                                .to_string(),
                        ));
                    }
                    let slice = &state.bytes[s_start..s_end];
                    if !validate_balanced_pdf_literals(slice) {
                        return Err(EasyOfficeError::CorruptedContainer(
                            "PDF 페이지 콘텐츠 스트림의 문자열 리터럴이 닫히지 않았습니다."
                                .to_string(),
                        ));
                    }
                    let Some(next_b) = state.remaining_budget.checked_sub(slice.len()) else {
                        return Err(EasyOfficeError::SecurityViolation(
                            "PDF 페이지 콘텐츠 스트림 누적 바이트 예산(32MiB)을 초과했습니다."
                                .to_string(),
                        ));
                    };
                    state.remaining_budget = next_b;
                    Ok(slice)
                }
            };

        let mut push_ref =
            |out: &mut Vec<&'a [u8]>, so: u32, sg: u16| -> Result<(), EasyOfficeError> {
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
                out.push(resolve_one(so, sg, s_start, s_end)?);
                Ok(())
            };

        match dict_get(page_entries, "/Contents") {
            None | Some(PdfObjValue::Null) => {
                if let Some(((so, sg), (s_start, s_end))) = inline_stream {
                    Ok(vec![resolve_one(so, sg, s_start, s_end)?])
                } else {
                    Ok(Vec::new())
                }
            }
            Some(PdfObjValue::Ref(co, cg)) => {
                let Some(target) = state.objects.get(&(*co, *cg)) else {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "PDF /Contents 간접 참조 대상을 찾을 수 없습니다.".to_string(),
                    ));
                };
                if target.stream_range.is_some() {
                    let mut out = Vec::with_capacity(1);
                    push_ref(&mut out, *co, *cg)?;
                    Ok(out)
                } else if let PdfObjValue::Array(ref arr) = target.val {
                    if arr.len() > MAX_PDF_CONTENTS_ARRAY_LEN {
                        return Err(EasyOfficeError::CorruptedContainer(
                            "PDF /Contents 배열 원소 상한(256)을 초과했습니다.".to_string(),
                        ));
                    }
                    let mut out = Vec::with_capacity(arr.len());
                    for item in arr {
                        match item {
                            PdfObjValue::Ref(so, sg) => push_ref(&mut out, *so, *sg)?,
                            PdfObjValue::Null => {}
                            _ => {
                                return Err(EasyOfficeError::CorruptedContainer(
                                    "PDF /Contents 배열 원소가 스트림 간접 참조가 아닙니다."
                                        .to_string(),
                                ))
                            }
                        }
                    }
                    Ok(out)
                } else {
                    Err(EasyOfficeError::CorruptedContainer(
                        "PDF /Contents 참조 대상이 스트림 또는 배열이 아닙니다.".to_string(),
                    ))
                }
            }
            Some(PdfObjValue::Array(arr)) => {
                if arr.len() > MAX_PDF_CONTENTS_ARRAY_LEN {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "PDF /Contents 배열 원소 상한(256)을 초과했습니다.".to_string(),
                    ));
                }
                let mut out = Vec::with_capacity(arr.len());
                for item in arr {
                    match item {
                        PdfObjValue::Ref(so, sg) => push_ref(&mut out, *so, *sg)?,
                        PdfObjValue::Null => {}
                        _ => {
                            return Err(EasyOfficeError::CorruptedContainer(
                                "PDF /Contents 배열 원소가 스트림 간접 참조가 아닙니다."
                                    .to_string(),
                            ))
                        }
                    }
                }
                Ok(out)
            }
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
        let parsed = parse_pdf_content_stream(&stream_text);
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
        let mut objects: HashMap<(u32, u16), PdfIndirectObject> = HashMap::new();
        let mut trailers: Vec<Vec<(String, PdfObjValue)>> = Vec::new();
        let mut stream_spans: Vec<(usize, usize)> = Vec::new();
        let mut has_encrypt = false;
        let mut trailer_b64: Option<String> = None;
        let mut trailer_b64_count = 0usize;
        let mut order_seq = 0usize;

        let mut p = 0usize;
        while p < bytes.len() {
            p = skip_pdf_ws_and_comments(bytes, p, bytes.len());
            if p >= bytes.len() {
                break;
            }
            if let Some((obj_num, gen_num, after_obj)) =
                parse_indirect_obj_header(bytes, p, bytes.len())
            {
                let parsed_val =
                    parse_pdf_value(bytes, after_obj, bytes.len(), 0).map_err(|_| {
                        fail_corrupted_or_encrypted(
                            &bytes[p..],
                            has_encrypt,
                            "PDF 간접 객체 구문이 손상되었거나 리터럴이 닫히지 않았습니다.",
                        )
                    })?;
                let (val, mut next_p) = parsed_val.unwrap_or((PdfObjValue::Null, after_obj));
                if let PdfObjValue::Dict(ref entries) = val {
                    if dict_name_eq(entries, "/Type", "/XRef")
                        && dict_has_non_null_key(entries, "/Encrypt")
                    {
                        has_encrypt = true;
                    }
                }
                next_p = skip_pdf_ws_and_comments(bytes, next_p, bytes.len());
                let mut stream_range = None;
                if is_standalone_pdf_kw(bytes, next_p, b"stream") {
                    let PdfObjValue::Dict(ref entries) = val else {
                        return Err(fail_corrupted_or_encrypted(
                            &bytes[p..],
                            has_encrypt,
                            "PDF stream 앞에 객체 딕셔너리가 없습니다.",
                        ));
                    };
                    let stream_kw_pos = next_p;
                    let stream_start = advance_past_stream_eol(bytes, next_p + 6);
                    let len_hint =
                        resolve_stream_length_hint(bytes, stream_start, entries, &objects);
                    let Some((stream_end, after_endstream)) =
                        resolve_stream_bounds(bytes, stream_start, len_hint)
                    else {
                        return Err(fail_corrupted_or_encrypted(
                            &bytes[p..],
                            has_encrypt,
                            "PDF stream 블록에 대응하는 endstream 토큰이 누락되었거나 경계가 손상되었습니다.",
                        ));
                    };
                    stream_spans.push((stream_kw_pos, after_endstream));
                    stream_range = Some((stream_start, stream_end));
                    next_p = skip_pdf_ws_and_comments(bytes, after_endstream, bytes.len());
                }
                if !is_standalone_pdf_kw(bytes, next_p, b"endobj") {
                    return Err(fail_corrupted_or_encrypted(
                        &bytes[p..],
                        has_encrypt,
                        "PDF 간접 객체에 대응하는 endobj 토큰이 누락되었습니다.",
                    ));
                }
                order_seq += 1;
                objects.insert(
                    (obj_num, gen_num),
                    PdfIndirectObject {
                        val,
                        stream_range,
                        order_seq,
                    },
                );
                p = next_p + 6;
                continue;
            }
            if is_standalone_pdf_kw(bytes, p, b"xref") {
                p += 4;
                loop {
                    p = skip_pdf_ws_and_comments(bytes, p, bytes.len());
                    if p >= bytes.len()
                        || parse_indirect_obj_header(bytes, p, bytes.len()).is_some()
                    {
                        break;
                    }
                    let mut k = p;
                    while k < bytes.len() && !is_pdf_delim_or_ws(bytes[k]) {
                        k += 1;
                    }
                    if k == p {
                        break;
                    }
                    let tok = &bytes[p..k];
                    if tok == b"n" || tok == b"f" || tok.iter().all(|b| b.is_ascii_digit()) {
                        p = k;
                    } else {
                        break;
                    }
                }
                continue;
            }
            if is_standalone_pdf_kw(bytes, p, b"trailer") {
                let parsed_tr = parse_pdf_value(bytes, p + 7, bytes.len(), 0).map_err(|_| {
                    fail_corrupted_or_encrypted(
                        &bytes[p..],
                        has_encrypt,
                        "PDF trailer 딕셔너리가 손상되었습니다.",
                    )
                })?;
                let Some((PdfObjValue::Dict(tr_entries), next_p)) = parsed_tr else {
                    return Err(fail_corrupted_or_encrypted(
                        &bytes[p..],
                        has_encrypt,
                        "PDF trailer 뒤에 딕셔너리가 없습니다.",
                    ));
                };
                if dict_has_non_null_key(&tr_entries, "/Encrypt") {
                    has_encrypt = true;
                }
                trailers.push(tr_entries);
                p = next_p;
                continue;
            }
            if is_standalone_pdf_kw(bytes, p, b"startxref") {
                let sx_pos = p;
                p = skip_pdf_ws_and_comments(bytes, p + 9, bytes.len());
                let start_off = p;
                while p < bytes.len() && bytes[p].is_ascii_digit() {
                    p += 1;
                }
                if p == start_off {
                    return Err(fail_corrupted_or_encrypted(
                        &bytes[sx_pos..],
                        has_encrypt,
                        "PDF startxref 오프셋 숫자가 누락되었습니다.",
                    ));
                }
                continue;
            }
            return Err(fail_corrupted_or_encrypted(
                &bytes[p..],
                has_encrypt,
                "PDF 상위 구조에 해석할 수 없는 손상 토큰이 존재합니다.",
            ));
        }

        if has_encrypt {
            return Ok(ScannedPdfStructure {
                has_encrypt: true,
                title: String::new(),
                watermark_text: None,
                pages: Vec::new(),
                trailer_b64: None,
                trailer_b64_count: 0,
            });
        }

        Self::collect_outside_stream_b64_comments(
            bytes,
            &stream_spans,
            &mut trailer_b64,
            &mut trailer_b64_count,
        );

        let mut ordered_objs: Vec<((u32, u16), &PdfIndirectObject)> =
            objects.iter().map(|(&id, o)| (id, o)).collect();
        ordered_objs.sort_by_key(|(_, o)| o.order_seq);

        let mut remaining_budget = MAX_PDF_TOTAL_DECODED_BYTES;
        let flate_cache = decode_and_validate_pdf_streams(
            bytes,
            &objects,
            &ordered_objs,
            &mut remaining_budget,
            &mut trailer_b64,
            &mut trailer_b64_count,
        )?;

        let (title, pages_target) =
            resolve_catalog_and_pages_target(&objects, &ordered_objs, &trailers);

        let mut walk_state = PageWalkState {
            bytes,
            objects: &objects,
            flate_cache: &flate_cache,
            first_used_flate: HashSet::new(),
            remaining_budget,
            visited: HashSet::new(),
            node_count: 0,
            pages: Vec::new(),
            watermark_text: None,
        };

        if let Some(target) = pages_target {
            Self::walk_page_tree_node(&mut walk_state, target, 0, 0)?;
        } else {
            for &((on, gn), obj) in &ordered_objs {
                if let PdfObjValue::Dict(ref d) = obj.val {
                    if dict_name_eq(d, "/Type", "/Page") {
                        walk_state.node_count += 1;
                        if walk_state.node_count > MAX_PDF_PAGE_TREE_NODES {
                            return Err(EasyOfficeError::CorruptedContainer(
                                "PDF 페이지 트리 노드 상한(4096)을 초과했습니다.".to_string(),
                            ));
                        }
                        Self::emit_page_from_node(
                            &mut walk_state,
                            d,
                            obj.stream_range.map(|r| ((on, gn), r)),
                            0,
                        )?;
                    }
                }
            }
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

    pub fn decode_base64_bytes(input: &str) -> Option<Vec<u8>> {
        let clean: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
        if clean.is_empty() || (clean.len() & 3) != 0 {
            return None;
        }
        let val = |b: u8| -> Option<u8> {
            match b {
                b'A'..=b'Z' => Some(b - b'A'),
                b'a'..=b'z' => Some(b - b'a' + 26),
                b'0'..=b'9' => Some(b - b'0' + 52),
                b'+' => Some(62),
                b'/' => Some(63),
                _ => None,
            }
        };
        let mut out = Vec::with_capacity((clean.len() / 4) * 3);
        let mut idx = 0usize;
        while idx + 4 <= clean.len() {
            if (clean[idx + 2] == b'=' || clean[idx + 3] == b'=') && idx + 4 != clean.len() {
                return None;
            }
            let a = val(clean[idx])?;
            let b = val(clean[idx + 1])?;
            out.push((a << 2) | (b >> 4));
            if clean[idx + 2] != b'=' {
                let c = val(clean[idx + 2])?;
                out.push(((b & 0x0F) << 4) | (c >> 2));
                if clean[idx + 3] != b'=' {
                    let d = val(clean[idx + 3])?;
                    out.push(((c & 0x03) << 6) | d);
                }
            } else if clean[idx + 3] != b'=' {
                return None;
            }
            idx += 4;
        }
        Some(out)
    }

    fn decode_base64_utf8(input: &str) -> Option<String> {
        Self::decode_base64_bytes(input).and_then(|bytes| String::from_utf8(bytes).ok())
    }

    fn encode_utf16be_hex(text: &str) -> String {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        let mut out = String::with_capacity(text.len() * 4);
        for unit in text.encode_utf16() {
            out.push(HEX[((unit >> 12) & 0x0F) as usize] as char);
            out.push(HEX[((unit >> 8) & 0x0F) as usize] as char);
            out.push(HEX[((unit >> 4) & 0x0F) as usize] as char);
            out.push(HEX[(unit & 0x0F) as usize] as char);
        }
        out
    }

    fn format_pdf_tj_operand(text: &str) -> (&'static str, String) {
        if text.is_ascii() {
            ("/F1", format!("({})", Self::escape_pdf_text(text)))
        } else {
            ("/F2", format!("<{}>", Self::encode_utf16be_hex(text)))
        }
    }

    pub fn escape_pdf_text(text: &str) -> String {
        escape_pdf_text(text)
    }

    pub fn unescape_pdf_text(text: &str) -> String {
        unescape_pdf_text(text)
    }
}

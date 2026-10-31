#[path = "pdf_object_graph.rs"]
mod pdf_object_graph;

use crate::pdf_stream_parser::{
    advance_past_stream_eol, contains_subslice, escape_pdf_text, is_pdf_delim_or_ws,
    is_standalone_pdf_kw, parse_indirect_obj_header, parse_pdf_content_stream,
    skip_pdf_ws_and_comments, step_pdf_non_code_token, unescape_pdf_text,
    validate_balanced_pdf_literals, validate_pdf_trailer_json, PdfNonCodeKind, PdfNonCodeStep,
};
use crate::{ContainerFamily, DocumentFormat, EasyOfficeError, FormatDetector};
use pdf_object_graph::{
    decode_stream_bytes, dict_get, dict_has_flate_filter, dict_has_non_null_key, dict_name_eq,
    fail_corrupted_or_encrypted, is_studio_metadata_dict, parse_pdf_value,
    resolve_catalog_and_pages_target, resolve_int, resolve_stream_bounds,
    resolve_stream_length_hint, resolve_value, stream_contains_forbidden_b64_comment,
    PageNodeTarget, PdfIndirectObject, PdfObjValue,
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
            return Err(fail_corrupted_or_encrypted(
                bytes,
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
        bytes: &'a [u8],
        objects: &'a HashMap<(u32, u16), PdfIndirectObject>,
        page_entries: &[(String, PdfObjValue)],
        inline_stream: Option<(usize, usize)>,
        has_encrypt: bool,
    ) -> Result<Vec<Cow<'a, [u8]>>, EasyOfficeError> {
        let decode_stream_obj = |so: u32, sg: u16| -> Result<Cow<'a, [u8]>, EasyOfficeError> {
            let Some(stream_obj) = objects.get(&(so, sg)) else {
                return Err(fail_corrupted_or_encrypted(
                    bytes,
                    has_encrypt,
                    "PDF /Contents 간접 스트림 객체를 찾을 수 없습니다.",
                ));
            };
            let Some((s_start, s_end)) = stream_obj.stream_range else {
                return Err(fail_corrupted_or_encrypted(
                    bytes,
                    has_encrypt,
                    "PDF /Contents 대상 객체에 stream 본문이 없습니다.",
                ));
            };
            let PdfObjValue::Dict(ref s_entries) = stream_obj.val else {
                return Err(fail_corrupted_or_encrypted(
                    bytes,
                    has_encrypt,
                    "PDF 스트림 객체 헤더가 딕셔너리가 아닙니다.",
                ));
            };
            let decoded = decode_stream_bytes(
                s_entries,
                &bytes[s_start..s_end],
                objects,
                bytes,
                has_encrypt,
            )?;
            if matches!(decoded, Cow::Owned(_))
                && stream_contains_forbidden_b64_comment(decoded.as_ref())
            {
                return Err(EasyOfficeError::SecurityViolation(
                    "PDF 압축 페이지 스트림 내부에 허용되지 않는 %EasyOfficePDFModelBase64 주석이 발견되었습니다."
                        .to_string(),
                ));
            }
            if !validate_balanced_pdf_literals(decoded.as_ref()) {
                return Err(fail_corrupted_or_encrypted(
                    bytes,
                    has_encrypt,
                    "PDF 페이지 콘텐츠 스트림의 문자열 리터럴이 닫히지 않았습니다.",
                ));
            }
            Ok(decoded)
        };

        match dict_get(page_entries, "/Contents") {
            None | Some(PdfObjValue::Null) => {
                if let Some((s_start, s_end)) = inline_stream {
                    let decoded = decode_stream_bytes(
                        page_entries,
                        &bytes[s_start..s_end],
                        objects,
                        bytes,
                        has_encrypt,
                    )?;
                    if !validate_balanced_pdf_literals(decoded.as_ref()) {
                        return Err(fail_corrupted_or_encrypted(
                            bytes,
                            has_encrypt,
                            "PDF 인라인 페이지 스트림의 문자열 리터럴이 닫히지 않았습니다.",
                        ));
                    }
                    Ok(vec![decoded])
                } else {
                    Ok(Vec::new())
                }
            }
            Some(PdfObjValue::Ref(co, cg)) => {
                let Some(target) = objects.get(&(*co, *cg)) else {
                    return Err(fail_corrupted_or_encrypted(
                        bytes,
                        has_encrypt,
                        "PDF /Contents 간접 참조 대상을 찾을 수 없습니다.",
                    ));
                };
                if target.stream_range.is_some() {
                    Ok(vec![decode_stream_obj(*co, *cg)?])
                } else if let PdfObjValue::Array(ref arr) = target.val {
                    let mut out = Vec::with_capacity(arr.len());
                    for item in arr {
                        match item {
                            PdfObjValue::Ref(so, sg) => out.push(decode_stream_obj(*so, *sg)?),
                            PdfObjValue::Null => {}
                            _ => {
                                return Err(fail_corrupted_or_encrypted(
                                    bytes,
                                    has_encrypt,
                                    "PDF /Contents 배열 원소가 스트림 간접 참조가 아닙니다.",
                                ))
                            }
                        }
                    }
                    Ok(out)
                } else {
                    Err(fail_corrupted_or_encrypted(
                        bytes,
                        has_encrypt,
                        "PDF /Contents 참조 대상이 스트림 또는 배열이 아닙니다.",
                    ))
                }
            }
            Some(PdfObjValue::Array(arr)) => {
                let mut out = Vec::with_capacity(arr.len());
                for item in arr {
                    match item {
                        PdfObjValue::Ref(so, sg) => out.push(decode_stream_obj(*so, *sg)?),
                        PdfObjValue::Null => {}
                        _ => {
                            return Err(fail_corrupted_or_encrypted(
                                bytes,
                                has_encrypt,
                                "PDF /Contents 배열 원소가 스트림 간접 참조가 아닙니다.",
                            ))
                        }
                    }
                }
                Ok(out)
            }
            Some(_) => Err(fail_corrupted_or_encrypted(
                bytes,
                has_encrypt,
                "PDF /Page /Contents 항목 타입이 올바르지 않습니다.",
            )),
        }
    }

    fn emit_page_from_node(
        bytes: &[u8],
        objects: &HashMap<(u32, u16), PdfIndirectObject>,
        page_entries: &[(String, PdfObjValue)],
        inline_stream: Option<(usize, usize)>,
        inherited_rotation: i32,
        pages: &mut Vec<PdfPageModel>,
        watermark_text: &mut Option<String>,
    ) -> Result<(), EasyOfficeError> {
        let rotation = dict_get(page_entries, "/Rotate")
            .and_then(|v| resolve_int(v, objects))
            .and_then(|n| i32::try_from(n).ok())
            .unwrap_or(inherited_rotation);
        let page_number = dict_get(page_entries, "/PageNumber")
            .and_then(|v| resolve_int(v, objects))
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(pages.len() + 1);
        let slices =
            Self::collect_page_content_slices(bytes, objects, page_entries, inline_stream, false)?;
        if slices.is_empty() {
            pages.push(PdfPageModel {
                page_number,
                rotation_degrees: rotation,
                body_text: String::new(),
                approval_stamp: None,
            });
            return Ok(());
        }
        let merged_cow: Cow<'_, [u8]> = if slices.len() == 1 {
            slices.into_iter().next().unwrap()
        } else {
            let total_len: usize =
                slices.iter().map(|s| s.len()).sum::<usize>() + slices.len().saturating_sub(1);
            let mut joined = Vec::with_capacity(total_len);
            for (idx, chunk) in slices.iter().enumerate() {
                if idx > 0 {
                    joined.push(b'\n');
                }
                joined.extend_from_slice(chunk.as_ref());
            }
            Cow::Owned(joined)
        };
        let stream_text = String::from_utf8_lossy(merged_cow.as_ref());
        let parsed = parse_pdf_content_stream(&stream_text);
        if let Some(wm) = parsed.watermark {
            *watermark_text = Some(wm);
        }
        pages.push(PdfPageModel {
            page_number,
            rotation_degrees: rotation,
            body_text: parsed.body_text,
            approval_stamp: parsed.stamp,
        });
        Ok(())
    }

    fn walk_page_tree_node<'a>(
        bytes: &'a [u8],
        objects: &'a HashMap<(u32, u16), PdfIndirectObject>,
        target: PageNodeTarget<'a>,
        inherited_rotation: i32,
        visited: &mut HashSet<(u32, u16)>,
        node_count: &mut usize,
        out: &mut (Vec<PdfPageModel>, Option<String>),
    ) -> Result<(), EasyOfficeError> {
        *node_count += 1;
        if *node_count > MAX_PDF_PAGE_TREE_NODES {
            return Err(EasyOfficeError::CorruptedContainer(
                "PDF 페이지 트리 노드 상한(4096)을 초과했습니다.".to_string(),
            ));
        }
        let (entries, inline_stream) = match target {
            PageNodeTarget::Ref(on, gn) => {
                if !visited.insert((on, gn)) {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "PDF 페이지 트리에서 순환 참조 또는 중복 노드가 감지되었습니다."
                            .to_string(),
                    ));
                }
                let Some(obj) = objects.get(&(on, gn)) else {
                    return Err(fail_corrupted_or_encrypted(
                        bytes,
                        false,
                        "PDF 페이지 트리 간접 객체를 찾을 수 없습니다.",
                    ));
                };
                let PdfObjValue::Dict(ref d) = obj.val else {
                    return Err(fail_corrupted_or_encrypted(
                        bytes,
                        false,
                        "PDF 페이지 트리 노드가 딕셔너리가 아닙니다.",
                    ));
                };
                (d.as_slice(), obj.stream_range)
            }
            PageNodeTarget::Inline(d) => (d, None),
        };

        let effective_rotation = dict_get(entries, "/Rotate")
            .and_then(|v| resolve_int(v, objects))
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
            let Some(PdfObjValue::Array(kids)) = resolve_value(kids_raw, objects) else {
                return Err(EasyOfficeError::CorruptedContainer(
                    "PDF /Pages /Kids 항목이 배열이 아닙니다.".to_string(),
                ));
            };
            for kid in kids {
                match kid {
                    PdfObjValue::Ref(ko, kg) => Self::walk_page_tree_node(
                        bytes,
                        objects,
                        PageNodeTarget::Ref(*ko, *kg),
                        effective_rotation,
                        visited,
                        node_count,
                        out,
                    )?,
                    PdfObjValue::Dict(kd) => Self::walk_page_tree_node(
                        bytes,
                        objects,
                        PageNodeTarget::Inline(kd.as_slice()),
                        effective_rotation,
                        visited,
                        node_count,
                        out,
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
        Self::emit_page_from_node(
            bytes,
            objects,
            entries,
            inline_stream,
            effective_rotation,
            &mut out.0,
            &mut out.1,
        )
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
                            bytes,
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
                            bytes,
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
                            bytes,
                            has_encrypt,
                            "PDF stream 블록에 대응하는 endstream 토큰이 누락되었거나 경계가 손상되었습니다.",
                        ));
                    };
                    stream_spans.push((stream_kw_pos, after_endstream));
                    let raw_stream = &bytes[stream_start..stream_end];
                    if is_studio_metadata_dict(entries) {
                        let decoded_meta =
                            decode_stream_bytes(entries, raw_stream, &objects, bytes, has_encrypt)?;
                        let meta_text = String::from_utf8_lossy(decoded_meta.as_ref());
                        for s_line in meta_text.split(['\r', '\n']) {
                            if let Some(rest) =
                                s_line.trim().strip_prefix("%EasyOfficePDFModelBase64:")
                            {
                                trailer_b64_count += 1;
                                trailer_b64 = Some(rest.trim().to_string());
                            }
                        }
                    } else {
                        if stream_contains_forbidden_b64_comment(raw_stream) {
                            return Err(EasyOfficeError::SecurityViolation(
                                "PDF 비메타데이터 스트림 내부에 허용되지 않는 %EasyOfficePDFModelBase64 주석이 발견되었습니다."
                                    .to_string(),
                            ));
                        }
                        if matches!(dict_has_flate_filter(entries, &objects), Ok(true)) {
                            if let Ok(inflated) =
                                crate::deflate::inflate_auto(raw_stream, 4 * 1024 * 1024)
                            {
                                if stream_contains_forbidden_b64_comment(&inflated) {
                                    return Err(EasyOfficeError::SecurityViolation(
                                        "PDF 압축 비메타데이터 스트림 내부에 허용되지 않는 %EasyOfficePDFModelBase64 주석이 발견되었습니다."
                                            .to_string(),
                                    ));
                                }
                            }
                        }
                    }
                    stream_range = Some((stream_start, stream_end));
                    next_p = skip_pdf_ws_and_comments(bytes, after_endstream, bytes.len());
                }
                if !is_standalone_pdf_kw(bytes, next_p, b"endobj") {
                    return Err(fail_corrupted_or_encrypted(
                        bytes,
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
                        bytes,
                        has_encrypt,
                        "PDF trailer 딕셔너리가 손상되었습니다.",
                    )
                })?;
                let Some((PdfObjValue::Dict(tr_entries), next_p)) = parsed_tr else {
                    return Err(fail_corrupted_or_encrypted(
                        bytes,
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
                p = skip_pdf_ws_and_comments(bytes, p + 9, bytes.len());
                let start_off = p;
                while p < bytes.len() && bytes[p].is_ascii_digit() {
                    p += 1;
                }
                if p == start_off {
                    return Err(fail_corrupted_or_encrypted(
                        bytes,
                        has_encrypt,
                        "PDF startxref 오프셋 숫자가 누락되었습니다.",
                    ));
                }
                continue;
            }
            return Err(fail_corrupted_or_encrypted(
                bytes,
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

        let (title, pages_target) =
            resolve_catalog_and_pages_target(&objects, &ordered_objs, &trailers);

        let mut walk_out: (Vec<PdfPageModel>, Option<String>) = (Vec::new(), None);
        let mut visited: HashSet<(u32, u16)> = HashSet::new();
        let mut node_count = 0usize;

        if let Some(target) = pages_target {
            Self::walk_page_tree_node(
                bytes,
                &objects,
                target,
                0,
                &mut visited,
                &mut node_count,
                &mut walk_out,
            )?;
        } else {
            for (_, obj) in &ordered_objs {
                if let PdfObjValue::Dict(ref d) = obj.val {
                    if dict_name_eq(d, "/Type", "/Page") {
                        node_count += 1;
                        if node_count > MAX_PDF_PAGE_TREE_NODES {
                            return Err(EasyOfficeError::CorruptedContainer(
                                "PDF 페이지 트리 노드 상한(4096)을 초과했습니다.".to_string(),
                            ));
                        }
                        Self::emit_page_from_node(
                            bytes,
                            &objects,
                            d,
                            obj.stream_range,
                            0,
                            &mut walk_out.0,
                            &mut walk_out.1,
                        )?;
                    }
                }
            }
        }

        Ok(ScannedPdfStructure {
            has_encrypt: false,
            title,
            watermark_text: walk_out.1,
            pages: walk_out.0,
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

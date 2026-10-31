use crate::pdf_stream_parser::{
    advance_past_stream_eol, contains_subslice, escape_pdf_text, is_pdf_delim_or_ws,
    is_standalone_pdf_kw, match_indirect_obj_header, parse_pdf_content_stream,
    span_crosses_object_boundary, step_pdf_non_code_token, stream_has_top_level_token,
    unescape_pdf_text, validate_balanced_pdf_literals, validate_pdf_trailer_json, PdfNonCodeKind,
    PdfNonCodeStep,
};
use crate::{ContainerFamily, DocumentFormat, EasyOfficeError, FormatDetector};

pub use crate::csv_markdown_codecs::{CommonMarkCodec, CsvCodec, MarkdownBlock};

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

#[derive(Default)]
struct PdfObjectHeaderState {
    is_catalog: bool,
    is_page: bool,
    has_type_metadata: bool,
    has_subtype_studio: bool,
    is_non_page_stream: bool,
    direct_length: Option<usize>,
    length_indirect_stage: u8,
    title: Option<String>,
    page_number: Option<usize>,
    rotation: Option<i32>,
}

impl PdfObjectHeaderState {
    fn is_metadata_stream(&self) -> bool {
        self.has_type_metadata && self.has_subtype_studio
    }
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
            return Err(EasyOfficeError::CorruptedContainer(
                "PDF xref 테이블 또는 %%EOF 트레일러가 누락되었습니다.".to_string(),
            ));
        }

        let mut scanned = Self::scan_pdf_structure(bytes)?;
        if scanned.has_encrypt {
            return Err(EasyOfficeError::EncryptedDocumentUnsupported(
                DocumentFormat::Pdf,
            ));
        }
        if scanned.pages.is_empty() {
            return Err(Self::fail_corrupted_or_encrypted(
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

    fn has_pdf_comment_with_prefix(
        stream_bytes: &[u8],
        prefix: &[u8],
        skip_pdf_strings: bool,
    ) -> bool {
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

    fn resolve_stream_bounds(
        bytes: &[u8],
        start: usize,
        direct_length: Option<usize>,
        skip_pdf_strings: bool,
    ) -> Option<(usize, usize)> {
        let kw = b"endstream";
        if let Some(len) = direct_length {
            if let Some(end_pos) = start.checked_add(len) {
                if end_pos <= bytes.len() {
                    let mut kw_pos = end_pos;
                    if kw_pos < bytes.len() && bytes[kw_pos] == b'\r' {
                        kw_pos += 1;
                        if kw_pos < bytes.len() && bytes[kw_pos] == b'\n' {
                            kw_pos += 1;
                        }
                    } else if kw_pos < bytes.len() && bytes[kw_pos] == b'\n' {
                        kw_pos += 1;
                    }
                    if is_standalone_pdf_kw(bytes, kw_pos, kw) {
                        if span_crosses_object_boundary(bytes, start, end_pos) {
                            return None;
                        }
                        if skip_pdf_strings
                            && !validate_balanced_pdf_literals(&bytes[start..end_pos])
                        {
                            return None;
                        }
                        return Some((end_pos, kw_pos + kw.len()));
                    }
                }
            }
        }
        let mut i = start;
        while i < bytes.len() {
            if skip_pdf_strings {
                match step_pdf_non_code_token(bytes, i) {
                    Ok(Some(step)) => {
                        if matches!(
                            step.kind,
                            PdfNonCodeKind::ParenString { .. } | PdfNonCodeKind::HexString { .. }
                        ) && span_crosses_object_boundary(bytes, i + 1, step.next_index)
                        {
                            return None;
                        }
                        i = step.next_index;
                        continue;
                    }
                    Ok(None) => {}
                    Err(()) => return None,
                }
            }
            if is_standalone_pdf_kw(bytes, i, b"endobj") {
                let q =
                    crate::pdf_stream_parser::skip_pdf_ws_and_comments(bytes, i + 6, bytes.len());
                if match_indirect_obj_header(bytes, q, bytes.len()).is_some()
                    || is_standalone_pdf_kw(bytes, q, b"xref")
                    || is_standalone_pdf_kw(bytes, q, b"trailer")
                    || is_standalone_pdf_kw(bytes, q, b"startxref")
                {
                    return None;
                }
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
                return Some((trim_end, i + kw.len()));
            }
            i += 1;
        }
        None
    }

    fn slice_has_dict_encrypt_key(slice: &[u8]) -> bool {
        let mut i = 0usize;
        let mut dict_depth = 0usize;
        while i < slice.len() {
            let b = slice[i];
            if b.is_ascii_whitespace() {
                i += 1;
                continue;
            }
            if b == b'<' && i + 1 < slice.len() && slice[i + 1] == b'<' {
                dict_depth += 1;
                i += 2;
                continue;
            }
            if b == b'>' && i + 1 < slice.len() && slice[i + 1] == b'>' {
                dict_depth = dict_depth.saturating_sub(1);
                i += 2;
                continue;
            }
            match step_pdf_non_code_token(slice, i) {
                Ok(Some(step)) => {
                    i = step.next_index;
                    continue;
                }
                Err(()) => {
                    i += 1;
                    continue;
                }
                Ok(None) => {}
            }
            if b == b'/' {
                let start_name = i;
                i += 1;
                while i < slice.len() && !is_pdf_delim_or_ws(slice[i]) {
                    i += 1;
                }
                if dict_depth >= 1 && &slice[start_name..i] == b"/Encrypt" {
                    return true;
                }
                continue;
            }
            i += 1;
        }
        false
    }

    fn has_encrypt_in_damaged_or_cipher_pdf(bytes: &[u8]) -> bool {
        let mut p = 0usize;
        while p < bytes.len() {
            if is_standalone_pdf_kw(bytes, p, b"trailer") {
                let start = p + 7;
                let cap = (start + 16_384).min(bytes.len());
                let mut end = cap;
                let mut q = start;
                while q < cap {
                    if is_standalone_pdf_kw(bytes, q, b"startxref")
                        || (q + 5 <= cap && &bytes[q..q + 5] == b"%%EOF")
                    {
                        end = q;
                        break;
                    }
                    q += 1;
                }
                if Self::slice_has_dict_encrypt_key(&bytes[start..end]) {
                    return true;
                }
                p = end.max(start);
                continue;
            }
            if let Some(after_obj) = match_indirect_obj_header(bytes, p, bytes.len()) {
                let cap = (after_obj + 16_384).min(bytes.len());
                let mut end = cap;
                let mut q = after_obj;
                while q < cap {
                    if is_standalone_pdf_kw(bytes, q, b"stream")
                        || is_standalone_pdf_kw(bytes, q, b"endobj")
                        || is_standalone_pdf_kw(bytes, q, b"xref")
                        || is_standalone_pdf_kw(bytes, q, b"trailer")
                    {
                        end = q;
                        break;
                    }
                    q += 1;
                }
                if Self::slice_has_dict_encrypt_key(&bytes[after_obj..end]) {
                    return true;
                }
                p = end.max(after_obj);
                continue;
            }
            p += 1;
        }
        false
    }

    fn fail_corrupted_or_encrypted(
        bytes: &[u8],
        has_encrypt: bool,
        detail: &str,
    ) -> EasyOfficeError {
        if has_encrypt || Self::has_encrypt_in_damaged_or_cipher_pdf(bytes) {
            EasyOfficeError::EncryptedDocumentUnsupported(DocumentFormat::Pdf)
        } else {
            EasyOfficeError::CorruptedContainer(detail.to_string())
        }
    }

    fn scan_pdf_structure(bytes: &[u8]) -> Result<ScannedPdfStructure, EasyOfficeError> {
        let mut i = 0usize;
        let mut dict_depth = 0usize;
        let mut has_encrypt = false;
        let mut title = String::from("PDF 문서");
        let mut watermark_text: Option<String> = None;
        let mut pages: Vec<PdfPageModel> = Vec::new();
        let mut pending_page_number: Option<usize> = None;
        let mut pending_rotation = 0i32;
        let mut trailer_b64: Option<String> = None;
        let mut trailer_b64_count = 0usize;
        let mut obj = PdfObjectHeaderState::default();
        let mut prev_name: Option<String> = None;

        let commit_obj_header = |obj: &mut PdfObjectHeaderState,
                                 title: &mut String,
                                 pending_pn: &mut Option<usize>,
                                 pending_rot: &mut i32| {
            if obj.is_catalog {
                if let Some(t) = obj.title.take() {
                    *title = unescape_pdf_text(&t);
                }
            }
            if obj.is_page {
                if let Some(pn) = obj.page_number.take() {
                    *pending_pn = Some(pn);
                }
                if let Some(rot) = obj.rotation.take() {
                    *pending_rot = rot;
                }
            }
        };

        while i < bytes.len() {
            let b = bytes[i];
            if b.is_ascii_whitespace() {
                i += 1;
                continue;
            }
            match step_pdf_non_code_token(bytes, i) {
                Ok(Some(PdfNonCodeStep {
                    next_index,
                    kind: PdfNonCodeKind::Comment { start, end },
                })) => {
                    let comment_line = String::from_utf8_lossy(&bytes[start..end]);
                    if let Some(rest) = comment_line
                        .trim()
                        .strip_prefix("%EasyOfficePDFModelBase64:")
                    {
                        trailer_b64_count += 1;
                        trailer_b64 = Some(rest.trim().to_string());
                    }
                    i = next_index;
                    continue;
                }
                Ok(Some(PdfNonCodeStep {
                    next_index,
                    kind:
                        PdfNonCodeKind::ParenString {
                            inner_start,
                            inner_end,
                        },
                })) => {
                    if span_crosses_object_boundary(bytes, inner_start, next_index) {
                        return Err(Self::fail_corrupted_or_encrypted(
                            bytes,
                            has_encrypt,
                            "PDF 객체 문자열 리터럴이 객체 경계를 침범했습니다.",
                        ));
                    }
                    obj.length_indirect_stage = 0;
                    if dict_depth == 1 && prev_name.as_deref() == Some("/Title") {
                        obj.title = Some(
                            String::from_utf8_lossy(&bytes[inner_start..inner_end]).into_owned(),
                        );
                    }
                    prev_name = None;
                    i = next_index;
                    continue;
                }
                Ok(Some(PdfNonCodeStep {
                    next_index,
                    kind: PdfNonCodeKind::DictOpen,
                })) => {
                    dict_depth += 1;
                    obj.length_indirect_stage = 0;
                    prev_name = None;
                    i = next_index;
                    continue;
                }
                Ok(Some(PdfNonCodeStep {
                    next_index,
                    kind: PdfNonCodeKind::DictClose,
                })) => {
                    dict_depth = dict_depth.saturating_sub(1);
                    obj.length_indirect_stage = 0;
                    prev_name = None;
                    i = next_index;
                    continue;
                }
                Ok(Some(PdfNonCodeStep {
                    next_index,
                    kind:
                        PdfNonCodeKind::HexString {
                            inner_start,
                            inner_end,
                        },
                })) => {
                    let valid_hex = bytes[inner_start..inner_end]
                        .iter()
                        .all(|c| c.is_ascii_hexdigit() || c.is_ascii_whitespace());
                    if !valid_hex || span_crosses_object_boundary(bytes, inner_start, next_index) {
                        return Err(Self::fail_corrupted_or_encrypted(
                            bytes,
                            has_encrypt,
                            "PDF 16진수 문자열 리터럴이 손상되었거나 닫히지 않았습니다.",
                        ));
                    }
                    obj.length_indirect_stage = 0;
                    prev_name = None;
                    i = next_index;
                    continue;
                }
                Err(()) => {
                    return Err(Self::fail_corrupted_or_encrypted(
                        bytes,
                        has_encrypt,
                        "PDF 객체 문자열 리터럴이 닫히지 않았습니다.",
                    ));
                }
                Ok(None) => {}
            }
            if b"[]>{}".contains(&b) {
                obj.length_indirect_stage = 0;
                prev_name = None;
                i += 1;
                continue;
            }
            if b == b'/' {
                obj.length_indirect_stage = 0;
                let start_name = i;
                i += 1;
                while i < bytes.len() && !is_pdf_delim_or_ws(bytes[i]) {
                    i += 1;
                }
                let name_cow = String::from_utf8_lossy(&bytes[start_name..i]);
                let name = name_cow.as_ref();
                if dict_depth >= 1 && name == "/Encrypt" {
                    has_encrypt = true;
                }
                if dict_depth != 1 {
                    prev_name = None;
                    continue;
                }
                if matches!(name, "/Length1" | "/Length2" | "/Length3") {
                    obj.is_non_page_stream = true;
                }
                match (prev_name.as_deref(), name) {
                    (Some("/Type"), "/Catalog") => {
                        obj.is_catalog = true;
                        obj.is_non_page_stream = true;
                        prev_name = None;
                    }
                    (Some("/Type"), "/Page") => {
                        obj.is_page = true;
                        prev_name = None;
                    }
                    (Some("/Type"), "/Metadata") => {
                        obj.has_type_metadata = true;
                        obj.is_non_page_stream = true;
                        prev_name = None;
                    }
                    (
                        Some("/Type"),
                        "/CMap" | "/XRef" | "/ObjStm" | "/XObject" | "/Font" | "/FontDescriptor"
                        | "/Pages",
                    ) => {
                        obj.is_non_page_stream = true;
                        prev_name = None;
                    }
                    (Some("/Subtype"), "/EasyOfficeStudio") => {
                        obj.has_subtype_studio = true;
                        obj.is_non_page_stream = true;
                        prev_name = None;
                    }
                    (
                        Some("/Subtype"),
                        "/Image" | "/Form" | "/Type1C" | "/CIDFontType0C" | "/OpenType" | "/XML"
                        | "/TrueType" | "/Type1",
                    ) => {
                        obj.is_non_page_stream = true;
                        prev_name = None;
                    }
                    _ => {
                        prev_name = Some(name.to_string());
                    }
                }
                continue;
            }

            let start_tok = i;
            while i < bytes.len() && !is_pdf_delim_or_ws(bytes[i]) {
                i += 1;
            }
            let tok_cow = String::from_utf8_lossy(&bytes[start_tok..i]);
            let tok = tok_cow.as_ref();
            match tok {
                "obj" => {
                    obj = PdfObjectHeaderState::default();
                    dict_depth = 0;
                    prev_name = None;
                }
                "endobj" => {
                    commit_obj_header(
                        &mut obj,
                        &mut title,
                        &mut pending_page_number,
                        &mut pending_rotation,
                    );
                    obj = PdfObjectHeaderState::default();
                    dict_depth = 0;
                    prev_name = None;
                }
                "begincmap" => {
                    obj.is_non_page_stream = true;
                    obj.length_indirect_stage = 0;
                    prev_name = None;
                }
                "stream" => {
                    commit_obj_header(
                        &mut obj,
                        &mut title,
                        &mut pending_page_number,
                        &mut pending_rotation,
                    );
                    dict_depth = 0;
                    prev_name = None;
                    i = advance_past_stream_eol(bytes, i);
                    let mut is_non_page = obj.is_non_page_stream;
                    let resolved = match Self::resolve_stream_bounds(
                        bytes,
                        i,
                        obj.direct_length,
                        !is_non_page,
                    ) {
                        Some(bounds) => Some(bounds),
                        None if !is_non_page => {
                            Self::resolve_stream_bounds(bytes, i, obj.direct_length, false).filter(
                                |&(end, _)| {
                                    stream_has_top_level_token(&bytes[i..end], b"begincmap")
                                },
                            )
                        }
                        None => None,
                    };
                    let Some((stream_end, after_endstream)) = resolved else {
                        return Err(Self::fail_corrupted_or_encrypted(
                            bytes,
                            has_encrypt,
                            "PDF stream 블록에 대응하는 endstream 토큰이 누락되었거나 문자열 리터럴이 닫히지 않았습니다.",
                        ));
                    };
                    let stream_bytes = &bytes[i..stream_end];
                    if !is_non_page && stream_has_top_level_token(stream_bytes, b"begincmap") {
                        is_non_page = true;
                    }
                    if obj.is_metadata_stream() {
                        let meta_text = String::from_utf8_lossy(stream_bytes);
                        for s_line in meta_text.split(['\r', '\n']) {
                            let s_trim = s_line.trim();
                            if let Some(rest) = s_trim.strip_prefix("%EasyOfficePDFModelBase64:") {
                                trailer_b64_count += 1;
                                trailer_b64 = Some(rest.trim().to_string());
                            }
                        }
                    } else {
                        if Self::has_pdf_comment_with_prefix(
                            stream_bytes,
                            b"%EasyOfficePDFModelBase64:",
                            !is_non_page,
                        ) {
                            return Err(EasyOfficeError::SecurityViolation(
                                "PDF 비메타데이터 스트림 내부에 허용되지 않는 %EasyOfficePDFModelBase64 주석이 발견되었습니다."
                                    .to_string(),
                            ));
                        }
                        if !is_non_page {
                            let stream_text = String::from_utf8_lossy(stream_bytes);
                            let parsed_stream = parse_pdf_content_stream(&stream_text);
                            if let Some(wm) = parsed_stream.watermark {
                                watermark_text = Some(wm);
                            }
                            pages.push(PdfPageModel {
                                page_number: pending_page_number.take().unwrap_or(pages.len() + 1),
                                rotation_degrees: pending_rotation,
                                body_text: parsed_stream.body_text,
                                approval_stamp: parsed_stream.stamp,
                            });
                            pending_rotation = 0;
                        }
                    }
                    i = after_endstream;
                }
                _ => {
                    if dict_depth == 1 {
                        match prev_name.as_deref() {
                            Some("/PageNumber") => {
                                obj.page_number = tok.parse::<usize>().ok();
                            }
                            Some("/Rotate") => {
                                obj.rotation = tok.parse::<i32>().ok();
                            }
                            Some("/Length") => {
                                if let Ok(len) = tok.parse::<usize>() {
                                    obj.direct_length = Some(len);
                                    obj.length_indirect_stage = 1;
                                }
                            }
                            _ => {
                                if obj.length_indirect_stage == 1 && tok.parse::<usize>().is_ok() {
                                    obj.length_indirect_stage = 2;
                                } else if obj.length_indirect_stage == 2 && tok == "R" {
                                    obj.direct_length = None;
                                    obj.length_indirect_stage = 0;
                                } else {
                                    obj.length_indirect_stage = 0;
                                }
                            }
                        }
                    }
                    prev_name = None;
                }
            }
        }

        Ok(ScannedPdfStructure {
            has_encrypt,
            title,
            watermark_text,
            pages,
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

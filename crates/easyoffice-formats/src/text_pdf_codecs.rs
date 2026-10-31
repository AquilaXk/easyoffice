use crate::pdf_stream_parser::{
    escape_pdf_text, parse_pdf_content_stream, unescape_pdf_text, validate_pdf_trailer_json,
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

struct PdfNonCodeStep {
    next_index: usize,
    comment_span: Option<(usize, usize)>,
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
        let raw = String::from_utf8_lossy(bytes);
        if !raw.contains("xref") || !raw.contains("%%EOF") {
            return Err(EasyOfficeError::CorruptedContainer(
                "PDF xref 테이블 또는 %%EOF 트레일러가 누락되었습니다.".to_string(),
            ));
        }

        let mut scanned = Self::scan_pdf_structure(&raw)?;
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

    fn is_pdf_delim_or_ws(b: u8) -> bool {
        b.is_ascii_whitespace() || b"()<>[]{}/%".contains(&b)
    }

    fn step_pdf_non_code_token(bytes: &[u8], i: usize) -> Result<Option<PdfNonCodeStep>, ()> {
        match bytes[i] {
            b'%' => {
                let mut j = i + 1;
                while j < bytes.len() && bytes[j] != b'\n' && bytes[j] != b'\r' {
                    j += 1;
                }
                Ok(Some(PdfNonCodeStep {
                    next_index: j,
                    comment_span: Some((i, j)),
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
                        comment_span: None,
                    }))
                }
            }
            b'<' if i + 1 < bytes.len() && bytes[i + 1] == b'<' => Ok(Some(PdfNonCodeStep {
                next_index: i + 2,
                comment_span: None,
            })),
            b'>' if i + 1 < bytes.len() && bytes[i + 1] == b'>' => Ok(Some(PdfNonCodeStep {
                next_index: i + 2,
                comment_span: None,
            })),
            b'<' => {
                let mut j = i + 1;
                while j < bytes.len() && bytes[j] != b'>' {
                    j += 1;
                }
                if j >= bytes.len() {
                    Err(())
                } else {
                    Ok(Some(PdfNonCodeStep {
                        next_index: j + 1,
                        comment_span: None,
                    }))
                }
            }
            _ => Ok(None),
        }
    }

    fn validate_balanced_pdf_literals(stream_slice: &str) -> bool {
        let bytes = stream_slice.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            match Self::step_pdf_non_code_token(bytes, i) {
                Ok(Some(step)) => i = step.next_index,
                Ok(None) => i += 1,
                Err(()) => return false,
            }
        }
        true
    }

    fn span_crosses_endstream_endobj(bytes: &[u8], start: usize, end: usize) -> bool {
        let kw = b"endstream";
        let eo = b"endobj";
        let mut p = start;
        while p + kw.len() <= end {
            if &bytes[p..p + kw.len()] == kw {
                let prev_ok = p == 0 || Self::is_pdf_delim_or_ws(bytes[p - 1]);
                let after_kw = p + kw.len();
                let next_ok = after_kw == bytes.len() || Self::is_pdf_delim_or_ws(bytes[after_kw]);
                if prev_ok && next_ok {
                    let mut q = after_kw;
                    while q < end && bytes[q].is_ascii_whitespace() {
                        q += 1;
                    }
                    if q + eo.len() <= end
                        && &bytes[q..q + eo.len()] == eo
                        && (q + eo.len() == bytes.len()
                            || Self::is_pdf_delim_or_ws(bytes[q + eo.len()]))
                    {
                        return true;
                    }
                }
            }
            p += 1;
        }
        false
    }

    fn has_pdf_comment_with_prefix(
        stream_slice: &str,
        prefix: &str,
        skip_pdf_strings: bool,
    ) -> bool {
        if !skip_pdf_strings {
            return stream_slice.contains(prefix);
        }
        let bytes = stream_slice.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            match Self::step_pdf_non_code_token(bytes, i) {
                Ok(Some(PdfNonCodeStep {
                    next_index,
                    comment_span: Some((cs, ce)),
                })) => {
                    if stream_slice[cs..ce].trim().starts_with(prefix) {
                        return true;
                    }
                    i = next_index;
                }
                Ok(Some(step)) => i = step.next_index,
                Ok(None) => i += 1,
                Err(()) => return false,
            }
        }
        false
    }

    fn resolve_stream_bounds(
        raw: &str,
        start: usize,
        direct_length: Option<usize>,
        skip_pdf_strings: bool,
    ) -> Option<(usize, usize)> {
        let bytes = raw.as_bytes();
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
                    if kw_pos + kw.len() <= bytes.len()
                        && &bytes[kw_pos..kw_pos + kw.len()] == kw
                        && (kw_pos + kw.len() == bytes.len()
                            || Self::is_pdf_delim_or_ws(bytes[kw_pos + kw.len()]))
                    {
                        if skip_pdf_strings
                            && !Self::validate_balanced_pdf_literals(&raw[start..end_pos])
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
                match Self::step_pdf_non_code_token(bytes, i) {
                    Ok(Some(step)) => {
                        if (bytes[i] == b'(' || bytes[i] == b'<')
                            && step.next_index > i + 2
                            && Self::span_crosses_endstream_endobj(bytes, i + 1, step.next_index)
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
            if i + kw.len() <= bytes.len() && &bytes[i..i + kw.len()] == kw {
                let prev_ok = i == start || Self::is_pdf_delim_or_ws(bytes[i - 1]);
                let next_ok =
                    i + kw.len() == bytes.len() || Self::is_pdf_delim_or_ws(bytes[i + kw.len()]);
                if prev_ok && next_ok {
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
            }
            i += 1;
        }
        None
    }

    fn scan_slice_for_encrypt_entry(slice: &str, skip_streams: bool) -> bool {
        let bytes = slice.as_bytes();
        let mut i = 0usize;
        let mut dict_depth = 0usize;
        while i < bytes.len() {
            let b = bytes[i];
            if b.is_ascii_whitespace() {
                i += 1;
                continue;
            }
            if b == b'<' && i + 1 < bytes.len() && bytes[i + 1] == b'<' {
                dict_depth += 1;
                i += 2;
                continue;
            }
            if b == b'>' && i + 1 < bytes.len() && bytes[i + 1] == b'>' {
                dict_depth = dict_depth.saturating_sub(1);
                i += 2;
                continue;
            }
            match Self::step_pdf_non_code_token(bytes, i) {
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
                while i < bytes.len() && !Self::is_pdf_delim_or_ws(bytes[i]) {
                    i += 1;
                }
                if dict_depth >= 1 && &slice[start_name..i] == "/Encrypt" {
                    return true;
                }
                continue;
            }
            if Self::is_pdf_delim_or_ws(b) {
                i += 1;
                continue;
            }
            let start_tok = i;
            while i < bytes.len() && !Self::is_pdf_delim_or_ws(bytes[i]) {
                i += 1;
            }
            if skip_streams && dict_depth == 0 && &slice[start_tok..i] == "stream" {
                if i < bytes.len() && bytes[i] == b'\r' {
                    i += 1;
                    if i < bytes.len() && bytes[i] == b'\n' {
                        i += 1;
                    }
                } else if i < bytes.len()
                    && (bytes[i] == b'\n' || bytes[i] == b' ' || bytes[i] == b'\t')
                {
                    i += 1;
                }
                if let Some((_, after_end)) = Self::resolve_stream_bounds(slice, i, None, false) {
                    i = after_end;
                }
            }
        }
        false
    }

    fn detect_pdf_encryption_outside_streams(raw: &str) -> bool {
        if Self::scan_slice_for_encrypt_entry(raw, true) {
            return true;
        }
        if let Some(last_es) = raw.rfind("endstream") {
            let tail = &raw[last_es + "endstream".len()..];
            if Self::scan_slice_for_encrypt_entry(tail, false) {
                return true;
            }
        }
        false
    }

    fn scan_pdf_structure(raw: &str) -> Result<ScannedPdfStructure, EasyOfficeError> {
        if Self::detect_pdf_encryption_outside_streams(raw) {
            return Ok(ScannedPdfStructure {
                has_encrypt: true,
                title: String::from("PDF 문서"),
                watermark_text: None,
                pages: Vec::new(),
                trailer_b64: None,
                trailer_b64_count: 0,
            });
        }
        let bytes = raw.as_bytes();
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
            if b == b'%' {
                let start_comment = i;
                while i < bytes.len() && bytes[i] != b'\n' && bytes[i] != b'\r' {
                    i += 1;
                }
                let comment_line = raw[start_comment..i].trim();
                if let Some(rest) = comment_line.strip_prefix("%EasyOfficePDFModelBase64:") {
                    trailer_b64_count += 1;
                    trailer_b64 = Some(rest.trim().to_string());
                }
                continue;
            }
            if b == b'(' {
                obj.length_indirect_stage = 0;
                let start_inner = i + 1;
                let mut depth = 1usize;
                i += 1;
                while i < bytes.len() && depth > 0 {
                    if bytes[i] == b'\\' {
                        i = (i + 2).min(bytes.len());
                        continue;
                    }
                    if bytes[i] == b'(' {
                        depth += 1;
                    } else if bytes[i] == b')' {
                        depth -= 1;
                        if depth == 0 {
                            let inner = &raw[start_inner..i];
                            if dict_depth == 1 && prev_name.as_deref() == Some("/Title") {
                                obj.title = Some(inner.to_string());
                            }
                            prev_name = None;
                            i += 1;
                            break;
                        }
                    }
                    i += 1;
                }
                continue;
            }
            if b == b'<' && i + 1 < bytes.len() && bytes[i + 1] == b'<' {
                dict_depth += 1;
                obj.length_indirect_stage = 0;
                prev_name = None;
                i += 2;
                continue;
            }
            if b == b'>' && i + 1 < bytes.len() && bytes[i + 1] == b'>' {
                dict_depth = dict_depth.saturating_sub(1);
                obj.length_indirect_stage = 0;
                prev_name = None;
                i += 2;
                continue;
            }
            if b == b'<' {
                obj.length_indirect_stage = 0;
                i += 1;
                while i < bytes.len() && bytes[i] != b'>' {
                    i += 1;
                }
                i = (i + 1).min(bytes.len());
                prev_name = None;
                continue;
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
                while i < bytes.len() && !Self::is_pdf_delim_or_ws(bytes[i]) {
                    i += 1;
                }
                let name = &raw[start_name..i];
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
            while i < bytes.len() && !Self::is_pdf_delim_or_ws(bytes[i]) {
                i += 1;
            }
            let tok = &raw[start_tok..i];
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
                    if i < bytes.len() && bytes[i] == b'\r' {
                        i += 1;
                        if i < bytes.len() && bytes[i] == b'\n' {
                            i += 1;
                        }
                    } else if i < bytes.len()
                        && (bytes[i] == b'\n' || bytes[i] == b' ' || bytes[i] == b'\t')
                    {
                        i += 1;
                    }
                    let Some((stream_end, after_endstream)) = Self::resolve_stream_bounds(
                        raw,
                        i,
                        obj.direct_length,
                        !obj.is_non_page_stream,
                    ) else {
                        if has_encrypt {
                            return Err(EasyOfficeError::EncryptedDocumentUnsupported(
                                DocumentFormat::Pdf,
                            ));
                        }
                        return Err(EasyOfficeError::CorruptedContainer(
                            "PDF stream 블록에 대응하는 endstream 토큰이 누락되었거나 문자열 리터럴이 닫히지 않았습니다."
                                .to_string(),
                        ));
                    };
                    let stream_slice = &raw[i..stream_end];
                    if obj.is_metadata_stream() {
                        for s_line in stream_slice.split(['\r', '\n']) {
                            let s_trim = s_line.trim();
                            if let Some(rest) = s_trim.strip_prefix("%EasyOfficePDFModelBase64:") {
                                trailer_b64_count += 1;
                                trailer_b64 = Some(rest.trim().to_string());
                            }
                        }
                    } else {
                        if Self::has_pdf_comment_with_prefix(
                            stream_slice,
                            "%EasyOfficePDFModelBase64:",
                            !obj.is_non_page_stream,
                        ) {
                            return Err(EasyOfficeError::SecurityViolation(
                                "PDF 비메타데이터 스트림 내부에 허용되지 않는 %EasyOfficePDFModelBase64 주석이 발견되었습니다."
                                    .to_string(),
                            ));
                        }
                        if !(obj.is_non_page_stream || stream_slice.contains("begincmap")) {
                            let parsed_stream = parse_pdf_content_stream(stream_slice);
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

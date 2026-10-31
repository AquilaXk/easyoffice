use crate::pdf_stream_parser::{escape_pdf_text, parse_pdf_content_stream, unescape_pdf_text};
use crate::{ContainerFamily, DocumentFormat, EasyOfficeError, FormatDetector, StrictJsonValue};

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
            let Some(validated_title) = Self::validate_pdf_trailer_json(
                &decoded,
                &scanned.title,
                &scanned.pages,
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

    fn has_pdf_comment_with_prefix(
        stream_slice: &str,
        prefix: &str,
        skip_pdf_strings: bool,
    ) -> bool {
        if !skip_pdf_strings {
            return stream_slice.lines().any(|l| l.trim().starts_with(prefix));
        }
        let bytes = stream_slice.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] == b'%' {
                let start_comment = i;
                i += 1;
                while i < bytes.len() && bytes[i] != b'\n' && bytes[i] != b'\r' {
                    i += 1;
                }
                if stream_slice[start_comment..i].trim().starts_with(prefix) {
                    return true;
                }
                continue;
            }
            if bytes[i] == b'(' {
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
                    }
                    i += 1;
                }
                continue;
            }
            if bytes[i] == b'<' && i + 1 < bytes.len() && bytes[i + 1] != b'<' {
                i += 1;
                while i < bytes.len() && bytes[i] != b'>' {
                    i += 1;
                }
                i = (i + 1).min(bytes.len());
                continue;
            }
            i += 1;
        }
        false
    }

    fn find_endstream_offset(
        raw: &str,
        start: usize,
        skip_pdf_strings: bool,
    ) -> Option<(usize, usize)> {
        let bytes = raw.as_bytes();
        let mut i = start;
        let kw = b"endstream";
        while i < bytes.len() {
            if skip_pdf_strings && bytes[i] == b'%' {
                i += 1;
                while i < bytes.len() && bytes[i] != b'\n' && bytes[i] != b'\r' {
                    i += 1;
                }
                continue;
            }
            if skip_pdf_strings && bytes[i] == b'(' {
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
                    }
                    i += 1;
                }
                if depth != 0 {
                    return None;
                }
                continue;
            }
            if skip_pdf_strings && bytes[i] == b'<' && i + 1 < bytes.len() && bytes[i + 1] != b'<' {
                i += 1;
                while i < bytes.len() && bytes[i] != b'>' {
                    i += 1;
                }
                if i >= bytes.len() {
                    return None;
                }
                i += 1;
                continue;
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

    fn scan_pdf_structure(raw: &str) -> Result<ScannedPdfStructure, EasyOfficeError> {
        let bytes = raw.as_bytes();
        let mut i = 0usize;
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
                            if prev_name.as_deref() == Some("/Title") {
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
                prev_name = None;
                i += 2;
                continue;
            }
            if b == b'>' && i + 1 < bytes.len() && bytes[i + 1] == b'>' {
                prev_name = None;
                i += 2;
                continue;
            }
            if b == b'<' {
                i += 1;
                while i < bytes.len() && bytes[i] != b'>' {
                    i += 1;
                }
                i = (i + 1).min(bytes.len());
                prev_name = None;
                continue;
            }
            if b"[]>{}".contains(&b) {
                prev_name = None;
                i += 1;
                continue;
            }
            if b == b'/' {
                let start_name = i;
                i += 1;
                while i < bytes.len() && !Self::is_pdf_delim_or_ws(bytes[i]) {
                    i += 1;
                }
                let name = &raw[start_name..i];
                if name == "/Encrypt" {
                    has_encrypt = true;
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
                    prev_name = None;
                }
                "begincmap" => {
                    obj.is_non_page_stream = true;
                    prev_name = None;
                }
                "stream" => {
                    commit_obj_header(
                        &mut obj,
                        &mut title,
                        &mut pending_page_number,
                        &mut pending_rotation,
                    );
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
                    let Some((stream_end, after_endstream)) =
                        Self::find_endstream_offset(raw, i, !obj.is_non_page_stream)
                    else {
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
                        for s_line in stream_slice.lines() {
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
                    match prev_name.as_deref() {
                        Some("/PageNumber") => {
                            obj.page_number = tok.parse::<usize>().ok();
                        }
                        Some("/Rotate") => {
                            obj.rotation = tok.parse::<i32>().ok();
                        }
                        _ => {}
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

    fn strip_ws(s: &str) -> String {
        s.chars().filter(|c| !c.is_whitespace()).collect()
    }

    fn wrap_line_64_raw(line: &str) -> Vec<String> {
        let chars: Vec<char> = line.chars().collect();
        if chars.len() <= 64 {
            return vec![line.to_string()];
        }
        chars.chunks(64).map(|c| c.iter().collect()).collect()
    }

    fn wrap_line_64_swift_trimmed(line: &str) -> Vec<String> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }
        let chars: Vec<char> = trimmed.chars().collect();
        if chars.len() <= 64 {
            return vec![trimmed.to_string()];
        }
        chars.chunks(64).map(|c| c.iter().collect()).collect()
    }

    fn expand_logical_pages_to_chunks(logical_pages: &[&str], mode: u8) -> Vec<String> {
        const MAX_LINES_PER_PDF_PAGE: usize = 42;
        let mut out = Vec::with_capacity(logical_pages.len());
        for &text in logical_pages {
            let lines: Vec<String> = match mode {
                0 => text.split('\n').map(str::to_string).collect(),
                1 => text.split('\n').flat_map(Self::wrap_line_64_raw).collect(),
                _ => text
                    .split('\n')
                    .flat_map(Self::wrap_line_64_swift_trimmed)
                    .collect(),
            };
            if lines.len() <= MAX_LINES_PER_PDF_PAGE {
                out.push(text.to_string());
            } else {
                for chunk in lines.chunks(MAX_LINES_PER_PDF_PAGE) {
                    out.push(chunk.join("\n"));
                }
            }
        }
        out
    }

    fn matches_pages_1to1(logical_pages: &[&str], pages: &[PdfPageModel]) -> bool {
        if logical_pages.len() == pages.len() {
            return logical_pages
                .iter()
                .zip(pages.iter())
                .all(|(jt, p)| Self::strip_ws(jt) == Self::strip_ws(&p.body_text));
        }
        for mode in [0u8, 1u8, 2u8] {
            let chunks = Self::expand_logical_pages_to_chunks(logical_pages, mode);
            if chunks.len() == pages.len()
                && chunks
                    .iter()
                    .zip(pages.iter())
                    .all(|(ct, p)| Self::strip_ws(ct) == Self::strip_ws(&p.body_text))
            {
                return true;
            }
        }
        false
    }

    fn extract_validated_page_text(p_entries: &[(String, StrictJsonValue)]) -> Option<&str> {
        let ext_val = StrictJsonValue::get_field(p_entries, "extractedText");
        let body_val = StrictJsonValue::get_field(p_entries, "bodyText");
        let ext_str = match ext_val {
            Some(v) => Some(v.as_str()?),
            None => None,
        };
        let body_str = match body_val {
            Some(v) => Some(v.as_str()?),
            None => None,
        };
        if let (Some(a), Some(b)) = (ext_str, body_str) {
            if a != b {
                return None;
            }
        }
        ext_str.or(body_str)
    }

    fn validate_pdf_trailer_json(
        decoded: &str,
        expected_title: &str,
        pages: &[PdfPageModel],
        parsed_watermark: Option<&str>,
    ) -> Option<String> {
        let Ok(root) = StrictJsonValue::parse(decoded) else {
            return None;
        };
        let root_entries = root.as_object()?;
        let json_title = StrictJsonValue::get_str(root_entries, "title")?;
        let normalized_json_title = json_title.replace(['\r', '\n'], " ");
        if json_title != expected_title && normalized_json_title != expected_title {
            return None;
        }
        match StrictJsonValue::get_field(root_entries, "watermarkText") {
            Some(StrictJsonValue::Str(wm)) => {
                if wm.is_empty() {
                    if parsed_watermark.is_some() {
                        return None;
                    }
                } else if parsed_watermark != Some(wm.as_str()) {
                    return None;
                }
            }
            Some(StrictJsonValue::Null) | None => {
                if parsed_watermark.is_some() {
                    return None;
                }
            }
            Some(_) => return None,
        }
        let root_body_field = StrictJsonValue::get_field(root_entries, "bodyText");
        if let Some(pages_val) = StrictJsonValue::get_field(root_entries, "pages") {
            let page_items = pages_val.as_array()?;
            if page_items.is_empty() || page_items.len() > pages.len() {
                return None;
            }
            let mut json_page_texts = Vec::with_capacity(page_items.len());
            for item in page_items {
                let p_entries = item.as_object()?;
                let text = Self::extract_validated_page_text(p_entries)?;
                json_page_texts.push(text);
            }
            if let Some(rb) = root_body_field {
                let rb_str = rb.as_str()?;
                if Self::strip_ws(rb_str) != Self::strip_ws(&json_page_texts.join("\n")) {
                    return None;
                }
            }
            if Self::matches_pages_1to1(&json_page_texts, pages) {
                return Some(json_title.to_string());
            }
            return None;
        }
        if let Some(rb) = root_body_field {
            let body_str = rb.as_str()?;
            if Self::matches_pages_1to1(&[body_str], pages) {
                return Some(json_title.to_string());
            }
        }
        None
    }

    pub fn escape_pdf_text(text: &str) -> String {
        escape_pdf_text(text)
    }

    pub fn unescape_pdf_text(text: &str) -> String {
        unescape_pdf_text(text)
    }
}

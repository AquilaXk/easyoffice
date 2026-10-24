use crate::{ContainerFamily, DocumentFormat, EasyOfficeError, FormatDetector};

/// RFC 4180 state-machine CSV / TSV parser and serializer.
pub struct CsvCodec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CsvParseState {
    Start,
    InUnquoted,
    InQuoted,
    QuoteInQuoted,
}

impl CsvCodec {
    pub fn parse(input: &str, delimiter: char) -> Result<Vec<Vec<String>>, EasyOfficeError> {
        if input.is_empty() {
            return Ok(Vec::new());
        }
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut current_row: Vec<String> = Vec::new();
        let mut current_field = String::new();
        let mut state = CsvParseState::Start;
        let mut chars = input.chars().peekable();

        while let Some(ch) = chars.next() {
            match state {
                CsvParseState::Start => {
                    if ch == '"' {
                        current_field.clear();
                        state = CsvParseState::InQuoted;
                    } else if ch == delimiter {
                        current_row.push(String::new());
                    } else if ch == '\r' || ch == '\n' {
                        if ch == '\r' && chars.peek() == Some(&'\n') {
                            chars.next();
                        }
                        current_row.push(String::new());
                        rows.push(std::mem::take(&mut current_row));
                    } else {
                        current_field.clear();
                        current_field.push(ch);
                        state = CsvParseState::InUnquoted;
                    }
                }
                CsvParseState::InUnquoted => {
                    if ch == '"' {
                        return Err(EasyOfficeError::CorruptedContainer(
                            "비따옴표 CSV 필드 내부에 예기치 않은 따옴표가 발견되었습니다."
                                .to_string(),
                        ));
                    } else if ch == delimiter {
                        current_row.push(std::mem::take(&mut current_field));
                        state = CsvParseState::Start;
                    } else if ch == '\r' || ch == '\n' {
                        if ch == '\r' && chars.peek() == Some(&'\n') {
                            chars.next();
                        }
                        current_row.push(std::mem::take(&mut current_field));
                        rows.push(std::mem::take(&mut current_row));
                        state = CsvParseState::Start;
                    } else {
                        current_field.push(ch);
                    }
                }
                CsvParseState::InQuoted => {
                    if ch == '"' {
                        state = CsvParseState::QuoteInQuoted;
                    } else {
                        current_field.push(ch);
                    }
                }
                CsvParseState::QuoteInQuoted => {
                    if ch == '"' {
                        current_field.push('"');
                        state = CsvParseState::InQuoted;
                    } else if ch == delimiter {
                        current_row.push(std::mem::take(&mut current_field));
                        state = CsvParseState::Start;
                    } else if ch == '\r' || ch == '\n' {
                        if ch == '\r' && chars.peek() == Some(&'\n') {
                            chars.next();
                        }
                        current_row.push(std::mem::take(&mut current_field));
                        rows.push(std::mem::take(&mut current_row));
                        state = CsvParseState::Start;
                    } else {
                        return Err(EasyOfficeError::CorruptedContainer(
                            "따옴표 CSV 필드 닫힘 이후 잘못된 문자가 발견되었습니다.".to_string(),
                        ));
                    }
                }
            }
        }

        match state {
            CsvParseState::InQuoted => {
                return Err(EasyOfficeError::CorruptedContainer(
                    "닫히지 않은 따옴표 CSV 필드가 있습니다.".to_string(),
                ));
            }
            CsvParseState::QuoteInQuoted | CsvParseState::InUnquoted => {
                current_row.push(current_field);
                rows.push(current_row);
            }
            CsvParseState::Start => {
                if !current_row.is_empty() {
                    current_row.push(String::new());
                    rows.push(current_row);
                }
            }
        }

        Ok(rows)
    }

    pub fn serialize(rows: &[Vec<String>], delimiter: char) -> String {
        let delim_str = delimiter.to_string();
        rows.iter()
            .map(|row| {
                row.iter()
                    .map(|field| Self::serialize_field(field, delimiter))
                    .collect::<Vec<_>>()
                    .join(&delim_str)
            })
            .collect::<Vec<_>>()
            .join("\r\n")
    }

    fn serialize_field(field: &str, delimiter: char) -> String {
        if field.contains(delimiter)
            || field.contains('"')
            || field.contains('\n')
            || field.contains('\r')
        {
            let escaped = field.replace('"', "\"\"");
            format!("\"{escaped}\"")
        } else {
            field.to_string()
        }
    }
}

/// Block element inside a CommonMark Markdown document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkdownBlock {
    Heading { level: u8, text: String },
    Paragraph(String),
    BlockQuote(String),
    BulletList(Vec<String>),
    NumberedList(Vec<String>),
}

/// CommonMark Markdown parser and serializer.
pub struct CommonMarkCodec;

impl CommonMarkCodec {
    pub fn parse(markdown: &str) -> Result<Vec<MarkdownBlock>, EasyOfficeError> {
        if markdown.as_bytes().contains(&0) {
            return Err(EasyOfficeError::InvalidUtf8(
                "Markdown 스트림에 NUL 바이트가 포함되어 있습니다.".to_string(),
            ));
        }

        let mut blocks: Vec<MarkdownBlock> = Vec::new();
        let mut bullet_acc: Vec<String> = Vec::new();
        let mut numbered_acc: Vec<String> = Vec::new();

        let flush_lists = |blocks: &mut Vec<MarkdownBlock>,
                           bullet_acc: &mut Vec<String>,
                           numbered_acc: &mut Vec<String>| {
            if !bullet_acc.is_empty() {
                blocks.push(MarkdownBlock::BulletList(std::mem::take(bullet_acc)));
            }
            if !numbered_acc.is_empty() {
                blocks.push(MarkdownBlock::NumberedList(std::mem::take(numbered_acc)));
            }
        };

        for raw_line in markdown.lines() {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() {
                flush_lists(&mut blocks, &mut bullet_acc, &mut numbered_acc);
                continue;
            }

            if let Some(rest) = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
            {
                if !numbered_acc.is_empty() {
                    blocks.push(MarkdownBlock::NumberedList(std::mem::take(
                        &mut numbered_acc,
                    )));
                }
                bullet_acc.push(rest.trim().to_string());
                continue;
            }

            if let Some(dot_idx) = trimmed.find(". ") {
                let prefix = &trimmed[..dot_idx];
                if !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit()) {
                    if !bullet_acc.is_empty() {
                        blocks.push(MarkdownBlock::BulletList(std::mem::take(&mut bullet_acc)));
                    }
                    numbered_acc.push(trimmed[dot_idx + 2..].trim().to_string());
                    continue;
                }
            }

            flush_lists(&mut blocks, &mut bullet_acc, &mut numbered_acc);

            if let Some(h4) = trimmed.strip_prefix("#### ") {
                blocks.push(MarkdownBlock::Heading {
                    level: 4,
                    text: h4.trim().to_string(),
                });
            } else if let Some(h3) = trimmed.strip_prefix("### ") {
                blocks.push(MarkdownBlock::Heading {
                    level: 3,
                    text: h3.trim().to_string(),
                });
            } else if let Some(h2) = trimmed.strip_prefix("## ") {
                blocks.push(MarkdownBlock::Heading {
                    level: 2,
                    text: h2.trim().to_string(),
                });
            } else if let Some(h1) = trimmed.strip_prefix("# ") {
                blocks.push(MarkdownBlock::Heading {
                    level: 1,
                    text: h1.trim().to_string(),
                });
            } else if let Some(quote) = trimmed.strip_prefix("> ") {
                blocks.push(MarkdownBlock::BlockQuote(quote.trim().to_string()));
            } else {
                blocks.push(MarkdownBlock::Paragraph(trimmed.to_string()));
            }
        }

        flush_lists(&mut blocks, &mut bullet_acc, &mut numbered_acc);
        Ok(blocks)
    }

    pub fn render(blocks: &[MarkdownBlock]) -> String {
        let mut rendered: Vec<String> = Vec::with_capacity(blocks.len());
        for block in blocks {
            match block {
                MarkdownBlock::Heading { level, text } => {
                    let hashes = "#".repeat((*level).clamp(1, 6) as usize);
                    rendered.push(format!("{hashes} {text}"));
                }
                MarkdownBlock::Paragraph(text) => rendered.push(text.clone()),
                MarkdownBlock::BlockQuote(text) => rendered.push(format!("> {text}")),
                MarkdownBlock::BulletList(items) => {
                    let lines = items
                        .iter()
                        .map(|item| format!("- {item}"))
                        .collect::<Vec<_>>()
                        .join("\n");
                    rendered.push(lines);
                }
                MarkdownBlock::NumberedList(items) => {
                    let lines = items
                        .iter()
                        .enumerate()
                        .map(|(idx, item)| format!("{}. {item}", idx + 1))
                        .collect::<Vec<_>>()
                        .join("\n");
                    rendered.push(lines);
                }
            }
        }
        rendered.join("\n\n")
    }
}

/// Page model for ISO 32000-1 PDF serialization and parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfPageModel {
    pub page_number: usize,
    pub rotation_degrees: i32,
    pub body_text: String,
    pub approval_stamp: Option<String>,
}

/// Document model for ISO 32000-1 PDF serialization and parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfDocumentModel {
    pub title: String,
    pub pages: Vec<PdfPageModel>,
    pub watermark_text: Option<String>,
}

/// ISO 32000-1 (`%PDF-1.4`) binary serializer and parser with byte-accurate `xref` table.
pub struct PdfSerializer;

impl PdfSerializer {
    pub fn serialize(doc: &PdfDocumentModel) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::with_capacity(2048);
        let push_line = |buf: &mut Vec<u8>, line: &str| {
            buf.extend_from_slice(line.as_bytes());
            buf.push(b'\n');
        };

        push_line(&mut out, "%PDF-1.4");

        let effective_pages: Vec<PdfPageModel> = if doc.pages.is_empty() {
            vec![PdfPageModel {
                page_number: 1,
                rotation_degrees: 0,
                body_text: String::new(),
                approval_stamp: None,
            }]
        } else {
            doc.pages.clone()
        };
        let page_count = effective_pages.len();
        let page_obj_ids: Vec<usize> = (0..page_count).map(|i| 3 + i * 2).collect();
        let kids_refs = page_obj_ids
            .iter()
            .map(|id| format!("{id} 0 R"))
            .collect::<Vec<_>>()
            .join(" ");

        let total_objects = 3 + page_count * 2;
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
            stream_lines.push(format!(
                "BT /F1 12 Tf 54 780 Td ({}) Tj ET",
                Self::escape_pdf_text(&page.body_text)
            ));
            if let Some(stamp) = &page.approval_stamp {
                stream_lines.push(format!(
                    "BT /F1 10 Tf 386 604 Td ([STAMP] {}) Tj ET",
                    Self::escape_pdf_text(stamp)
                ));
            }
            if let Some(wm) = &doc.watermark_text {
                stream_lines.push(format!(
                    "BT /F1 10 Tf 320 746 Td ([WM] {}) Tj ET",
                    Self::escape_pdf_text(wm)
                ));
            }
            let stream_body = stream_lines.join("\n");

            offsets[page_obj] = out.len();
            push_line(&mut out, &format!("{page_obj} 0 obj"));
            push_line(
                &mut out,
                &format!(
                    "<< /Type /Page /Parent 2 0 R /PageNumber {} /Rotate {} /MediaBox [0 0 595 842] /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >> /Contents {content_obj} 0 R >>",
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
        if raw.contains("/Encrypt") {
            return Err(EasyOfficeError::EncryptedDocumentUnsupported(
                DocumentFormat::Pdf,
            ));
        }

        let mut title = String::from("PDF 문서");
        let mut watermark_text: Option<String> = None;
        let mut pages: Vec<PdfPageModel> = Vec::new();
        let mut current_page_number: Option<usize> = None;
        let mut current_rotation = 0;
        let mut current_body = String::new();
        let mut current_stamp: Option<String> = None;
        let mut in_stream = false;

        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.contains("/Type /Catalog") {
                if let Some(t) = Self::extract_paren_token(trimmed, "/Title (") {
                    title = Self::unescape_pdf_text(&t);
                }
            }
            if trimmed.contains("/Type /Page") && !trimmed.contains("/Type /Pages") {
                if let Some(pn_idx) = trimmed.find("/PageNumber ") {
                    let after = &trimmed[pn_idx + 12..];
                    let num_str: String =
                        after.chars().take_while(|c| c.is_ascii_digit()).collect();
                    current_page_number = num_str.parse::<usize>().ok();
                }
                if let Some(rot_idx) = trimmed.find("/Rotate ") {
                    let after = &trimmed[rot_idx + 8..];
                    let num_str: String = after
                        .chars()
                        .take_while(|c| c.is_ascii_digit() || *c == '-')
                        .collect();
                    current_rotation = num_str.parse::<i32>().unwrap_or(0);
                }
            }
            if trimmed == "stream" {
                in_stream = true;
                current_body.clear();
                current_stamp = None;
                continue;
            }
            if trimmed == "endstream" {
                if in_stream {
                    pages.push(PdfPageModel {
                        page_number: current_page_number.take().unwrap_or(pages.len() + 1),
                        rotation_degrees: current_rotation,
                        body_text: current_body.clone(),
                        approval_stamp: current_stamp.take(),
                    });
                    current_rotation = 0;
                    in_stream = false;
                }
                continue;
            }
            if in_stream && trimmed.starts_with("BT ") {
                if let (Some(open), Some(close)) = (trimmed.find('('), trimmed.rfind(')')) {
                    if open < close {
                        let content = Self::unescape_pdf_text(&trimmed[open + 1..close]);
                        if let Some(wm) = content.strip_prefix("[WM] ") {
                            watermark_text = Some(wm.to_string());
                        } else if let Some(stamp) = content.strip_prefix("[STAMP] ") {
                            current_stamp = Some(stamp.to_string());
                        } else {
                            if !current_body.is_empty() {
                                current_body.push('\n');
                            }
                            current_body.push_str(&content);
                        }
                    }
                }
            }
        }

        if pages.is_empty() {
            return Err(EasyOfficeError::CorruptedContainer(
                "PDF 페이지 스트림을 찾을 수 없습니다.".to_string(),
            ));
        }

        Ok(PdfDocumentModel {
            title,
            pages,
            watermark_text,
        })
    }

    fn extract_paren_token(line: &str, prefix: &str) -> Option<String> {
        let start = line.find(prefix)? + prefix.len();
        let rest = &line[start..];
        let mut out = String::new();
        let mut chars = rest.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                out.push('\\');
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else if c == ')' {
                return Some(out);
            } else {
                out.push(c);
            }
        }
        None
    }

    pub fn escape_pdf_text(text: &str) -> String {
        let mut out = String::with_capacity(text.len() + 8);
        for ch in text.chars() {
            match ch {
                '\\' => out.push_str("\\\\"),
                '(' => out.push_str("\\("),
                ')' => out.push_str("\\)"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                _ => out.push(ch),
            }
        }
        out
    }

    pub fn unescape_pdf_text(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('r') => out.push('\r'),
                    Some(escaped) => out.push(escaped),
                    None => out.push('\\'),
                }
            } else {
                out.push(ch);
            }
        }
        out
    }
}

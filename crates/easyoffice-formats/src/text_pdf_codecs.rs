use std::borrow::Cow;

use crate::{ContainerFamily, DocumentFormat, EasyOfficeError, FormatDetector, XmlSanitizer};

/// RFC 4180 64-bit SWAR byte-slice and state-machine CSV / TSV parser and serializer.
pub struct CsvCodec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CsvParseState {
    Start,
    InUnquoted,
    InQuoted,
    QuoteInQuoted,
}

impl CsvCodec {
    pub const MAX_ROWS: usize = 1_000_000;
    pub const MAX_COLS: usize = 16_384;

    #[inline]
    fn find_csv_unquoted_special_swar(bytes: &[u8], start: usize, delim_b: u8) -> usize {
        const LO: u64 = 0x0101_0101_0101_0101;
        const HI: u64 = 0x8080_8080_8080_8080;
        let splat_delim = (delim_b as u64).wrapping_mul(LO);
        let splat_quote = (b'"' as u64) * LO;
        let splat_lf = (b'\n' as u64) * LO;
        let splat_cr = (b'\r' as u64) * LO;
        let mut idx = start;
        while idx + 8 <= bytes.len() {
            let word = u64::from_le_bytes(bytes[idx..idx + 8].try_into().unwrap());
            let d0 = word ^ splat_delim;
            let d1 = word ^ splat_quote;
            let d2 = word ^ splat_lf;
            let d3 = word ^ splat_cr;
            let m0 = d0.wrapping_sub(LO) & !d0;
            let m1 = d1.wrapping_sub(LO) & !d1;
            let m2 = d2.wrapping_sub(LO) & !d2;
            let m3 = d3.wrapping_sub(LO) & !d3;
            let mask = (m0 | m1 | m2 | m3) & HI;
            if mask != 0 {
                return idx + ((mask.trailing_zeros() >> 3) as usize);
            }
            idx += 8;
        }
        while idx < bytes.len() {
            let b = bytes[idx];
            if b == delim_b || b == b'"' || b == b'\n' || b == b'\r' {
                return idx;
            }
            idx += 1;
        }
        bytes.len()
    }

    pub fn visit_fields_swar<'a, F>(
        input: &'a str,
        delimiter: char,
        mut on_field: F,
    ) -> Result<(usize, usize), EasyOfficeError>
    where
        F: FnMut(usize, usize, Cow<'a, str>),
    {
        if input.is_empty() {
            return Ok((0, 0));
        }
        if !delimiter.is_ascii() {
            let rows = Self::parse_scalar_baseline(input, delimiter)?;
            let row_count = rows.len();
            let mut max_c = 0usize;
            for (r, row) in rows.into_iter().enumerate() {
                max_c = max_c.max(row.len());
                for (c, cell) in row.into_iter().enumerate() {
                    on_field(r, c, Cow::Owned(cell));
                }
            }
            return Ok((row_count, max_c));
        }

        let delim_b = delimiter as u8;
        let bytes = input.as_bytes();
        let mut row_idx = 0usize;
        let mut col_idx = 0usize;
        let mut max_cols = 0usize;
        let mut pos = 0usize;
        let mut trailing_delim = false;

        while pos < bytes.len() {
            if col_idx >= Self::MAX_COLS || row_idx >= Self::MAX_ROWS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "CSV 행 또는 열 개수가 허용 예산을 초과했습니다.".to_string(),
                ));
            }
            let b = bytes[pos];
            if b == b'"' {
                trailing_delim = false;
                pos += 1;
                let first_start = pos;
                let Some(first_q) = XmlSanitizer::find_byte_swar(bytes, pos, b'"') else {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "닫히지 않은 따옴표 CSV 필드가 있습니다.".to_string(),
                    ));
                };
                let field_cow = if first_q + 1 < bytes.len() && bytes[first_q + 1] == b'"' {
                    let mut owned = String::with_capacity(32);
                    if first_q > first_start {
                        owned.push_str(&input[first_start..first_q]);
                    }
                    owned.push('"');
                    pos = first_q + 2;
                    loop {
                        let Some(q_pos) = XmlSanitizer::find_byte_swar(bytes, pos, b'"') else {
                            return Err(EasyOfficeError::CorruptedContainer(
                                "닫히지 않은 따옴표 CSV 필드가 있습니다.".to_string(),
                            ));
                        };
                        if q_pos > pos {
                            owned.push_str(&input[pos..q_pos]);
                        }
                        pos = q_pos + 1;
                        if pos < bytes.len() && bytes[pos] == b'"' {
                            owned.push('"');
                            pos += 1;
                        } else {
                            break;
                        }
                    }
                    std::borrow::Cow::Owned(owned)
                } else {
                    pos = first_q + 1;
                    std::borrow::Cow::Borrowed(&input[first_start..first_q])
                };

                if pos >= bytes.len() {
                    on_field(row_idx, col_idx, field_cow);
                    col_idx += 1;
                    max_cols = max_cols.max(col_idx);
                    row_idx += 1;
                    col_idx = 0;
                    break;
                }
                let next_b = bytes[pos];
                if next_b == delim_b {
                    on_field(row_idx, col_idx, field_cow);
                    col_idx += 1;
                    pos += 1;
                    trailing_delim = true;
                } else if next_b == b'\r' || next_b == b'\n' {
                    if next_b == b'\r' && pos + 1 < bytes.len() && bytes[pos + 1] == b'\n' {
                        pos += 2;
                    } else {
                        pos += 1;
                    }
                    on_field(row_idx, col_idx, field_cow);
                    col_idx += 1;
                    max_cols = max_cols.max(col_idx);
                    row_idx += 1;
                    col_idx = 0;
                } else {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "따옴표 CSV 필드 닫힘 이후 잘못된 문자가 발견되었습니다.".to_string(),
                    ));
                }
            } else if b == delim_b {
                on_field(row_idx, col_idx, std::borrow::Cow::Borrowed(""));
                col_idx += 1;
                pos += 1;
                trailing_delim = true;
            } else if b == b'\r' || b == b'\n' {
                if b == b'\r' && pos + 1 < bytes.len() && bytes[pos + 1] == b'\n' {
                    pos += 2;
                } else {
                    pos += 1;
                }
                on_field(row_idx, col_idx, std::borrow::Cow::Borrowed(""));
                col_idx += 1;
                max_cols = max_cols.max(col_idx);
                row_idx += 1;
                col_idx = 0;
                trailing_delim = false;
            } else {
                trailing_delim = false;
                let end_pos = Self::find_csv_unquoted_special_swar(bytes, pos, delim_b);
                if end_pos == bytes.len() {
                    on_field(
                        row_idx,
                        col_idx,
                        std::borrow::Cow::Borrowed(&input[pos..end_pos]),
                    );
                    col_idx += 1;
                    max_cols = max_cols.max(col_idx);
                    row_idx += 1;
                    col_idx = 0;
                    break;
                }
                let term = bytes[end_pos];
                if term == b'"' {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "비따옴표 CSV 필드 내부에 예기치 않은 따옴표가 발견되었습니다.".to_string(),
                    ));
                } else if term == delim_b {
                    on_field(
                        row_idx,
                        col_idx,
                        std::borrow::Cow::Borrowed(&input[pos..end_pos]),
                    );
                    col_idx += 1;
                    pos = end_pos + 1;
                    trailing_delim = true;
                } else {
                    on_field(
                        row_idx,
                        col_idx,
                        std::borrow::Cow::Borrowed(&input[pos..end_pos]),
                    );
                    col_idx += 1;
                    max_cols = max_cols.max(col_idx);
                    row_idx += 1;
                    col_idx = 0;
                    if term == b'\r' && end_pos + 1 < bytes.len() && bytes[end_pos + 1] == b'\n' {
                        pos = end_pos + 2;
                    } else {
                        pos = end_pos + 1;
                    }
                }
            }
        }

        if trailing_delim && col_idx > 0 {
            if col_idx >= Self::MAX_COLS || row_idx >= Self::MAX_ROWS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "CSV 행 또는 열 개수가 허용 예산을 초과했습니다.".to_string(),
                ));
            }
            on_field(row_idx, col_idx, std::borrow::Cow::Borrowed(""));
            col_idx += 1;
            max_cols = max_cols.max(col_idx);
            row_idx += 1;
        }

        Ok((row_idx, max_cols))
    }

    pub fn parse(input: &str, delimiter: char) -> Result<Vec<Vec<String>>, EasyOfficeError> {
        if input.is_empty() {
            return Ok(Vec::new());
        }
        if delimiter.is_ascii() {
            Self::parse_swar_ascii(input, delimiter as u8)
        } else {
            Self::parse_scalar_baseline(input, delimiter)
        }
    }

    fn parse_swar_ascii(input: &str, delim_b: u8) -> Result<Vec<Vec<String>>, EasyOfficeError> {
        let mut rows: Vec<Vec<String>> = Vec::new();
        Self::visit_fields_swar(input, delim_b as char, |r, _c, field| {
            while rows.len() <= r {
                rows.push(Vec::new());
            }
            rows[r].push(field.into_owned());
        })?;
        Ok(rows)
    }

    pub fn parse_scalar_baseline(
        input: &str,
        delimiter: char,
    ) -> Result<Vec<Vec<String>>, EasyOfficeError> {
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
        let total_bytes: usize = rows
            .iter()
            .map(|r| r.iter().map(|f| f.len() + 2).sum::<usize>() + 2)
            .sum();
        let mut out = String::with_capacity(total_bytes);
        for (r_idx, row) in rows.iter().enumerate() {
            if r_idx > 0 {
                out.push_str("\r\n");
            }
            for (c_idx, field) in row.iter().enumerate() {
                if c_idx > 0 {
                    out.push(delimiter);
                }
                Self::write_serialized_field(&mut out, field, delimiter);
            }
        }
        out
    }

    fn write_serialized_field(out: &mut String, field: &str, delimiter: char) {
        if field.contains(delimiter)
            || field.contains('"')
            || field.contains('\n')
            || field.contains('\r')
        {
            out.push('"');
            for ch in field.chars() {
                if ch == '"' {
                    out.push_str("\"\"");
                } else {
                    out.push(ch);
                }
            }
            out.push('"');
        } else {
            out.push_str(field);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_swar_csv_parser_large_rows_escaped_quotes_crlf_lf_and_korean_utf8() {
        let mut table = Vec::with_capacity(64);
        for r in 0..64 {
            table.push(vec![
                format!("기획조정실_{r:03}"),
                format!("\"예산집행, 1분기 \"\"승인\"\" ({r})\""),
                format!("{}", (r + 1) * 1250),
                format!("\"다중행 메모 1행\n2행 상세내역 {r}\""),
                "디지털행정혁신과".to_string(),
                String::new(),
            ]);
        }

        let crlf_csv = table
            .iter()
            .map(|row| row.join(","))
            .collect::<Vec<_>>()
            .join("\r\n");
        let lf_csv = crlf_csv.replace("\r\n", "\n");

        let parsed_crlf = CsvCodec::parse(&crlf_csv, ',').unwrap();
        let baseline_crlf = CsvCodec::parse_scalar_baseline(&crlf_csv, ',').unwrap();
        let parsed_lf = CsvCodec::parse(&lf_csv, ',').unwrap();

        assert_eq!(parsed_crlf.len(), 64);
        assert_eq!(parsed_crlf, baseline_crlf);
        assert_eq!(parsed_lf, baseline_crlf);
        assert_eq!(parsed_crlf[0][0], "기획조정실_000");
        assert_eq!(parsed_crlf[0][1], "예산집행, 1분기 \"승인\" (0)");
        assert_eq!(parsed_crlf[0][3], "다중행 메모 1행\n2행 상세내역 0");
        assert_eq!(parsed_crlf[0][5], "");

        assert!(CsvCodec::parse("\"unclosed quote", ',').is_err());
        assert!(CsvCodec::parse("unquoted\"quote,val", ',').is_err());

        let mut visited_borrowed = 0usize;
        let mut visited_owned = 0usize;
        let mut visited_rows = vec![Vec::<String>::new()];
        CsvCodec::visit_fields_swar(&crlf_csv, ',', |r_idx, _c_idx, field| {
            while visited_rows.len() <= r_idx {
                visited_rows.push(Vec::new());
            }
            match &field {
                Cow::Borrowed(_) => visited_borrowed += 1,
                Cow::Owned(_) => visited_owned += 1,
            }
            visited_rows[r_idx].push(field.into_owned());
        })
        .unwrap();
        assert_eq!(visited_rows, parsed_crlf);
        assert_eq!(visited_owned, 64);
        assert_eq!(visited_borrowed, 64 * 5);
        assert_eq!(
            crate::rtf_xml_helpers::unescape_xml_single_pass("A&#0;&#x01;&#xFFFE;&#65;B"),
            "A&#0;&#x01;&#xFFFE;AB"
        );
    }
}

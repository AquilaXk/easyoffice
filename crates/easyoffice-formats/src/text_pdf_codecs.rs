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
                        if current_row.len() >= Self::MAX_COLS || rows.len() >= Self::MAX_ROWS {
                            return Err(EasyOfficeError::ResourceBudgetExceeded(
                                "CSV 행 또는 열 개수가 허용 예산을 초과했습니다.".to_string(),
                            ));
                        }
                        current_row.push(String::new());
                    } else if ch == '\r' || ch == '\n' {
                        if ch == '\r' && chars.peek() == Some(&'\n') {
                            chars.next();
                        }
                        if current_row.len() >= Self::MAX_COLS || rows.len() >= Self::MAX_ROWS {
                            return Err(EasyOfficeError::ResourceBudgetExceeded(
                                "CSV 행 또는 열 개수가 허용 예산을 초과했습니다.".to_string(),
                            ));
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
                        if current_row.len() >= Self::MAX_COLS || rows.len() >= Self::MAX_ROWS {
                            return Err(EasyOfficeError::ResourceBudgetExceeded(
                                "CSV 행 또는 열 개수가 허용 예산을 초과했습니다.".to_string(),
                            ));
                        }
                        current_row.push(std::mem::take(&mut current_field));
                        state = CsvParseState::Start;
                    } else if ch == '\r' || ch == '\n' {
                        if ch == '\r' && chars.peek() == Some(&'\n') {
                            chars.next();
                        }
                        if current_row.len() >= Self::MAX_COLS || rows.len() >= Self::MAX_ROWS {
                            return Err(EasyOfficeError::ResourceBudgetExceeded(
                                "CSV 행 또는 열 개수가 허용 예산을 초과했습니다.".to_string(),
                            ));
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
                        if current_row.len() >= Self::MAX_COLS || rows.len() >= Self::MAX_ROWS {
                            return Err(EasyOfficeError::ResourceBudgetExceeded(
                                "CSV 행 또는 열 개수가 허용 예산을 초과했습니다.".to_string(),
                            ));
                        }
                        current_row.push(std::mem::take(&mut current_field));
                        state = CsvParseState::Start;
                    } else if ch == '\r' || ch == '\n' {
                        if ch == '\r' && chars.peek() == Some(&'\n') {
                            chars.next();
                        }
                        if current_row.len() >= Self::MAX_COLS || rows.len() >= Self::MAX_ROWS {
                            return Err(EasyOfficeError::ResourceBudgetExceeded(
                                "CSV 행 또는 열 개수가 허용 예산을 초과했습니다.".to_string(),
                            ));
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
                if current_row.len() >= Self::MAX_COLS || rows.len() >= Self::MAX_ROWS {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "CSV 행 또는 열 개수가 허용 예산을 초과했습니다.".to_string(),
                    ));
                }
                current_row.push(current_field);
                rows.push(current_row);
            }
            CsvParseState::Start => {
                if !current_row.is_empty() {
                    if current_row.len() >= Self::MAX_COLS || rows.len() >= Self::MAX_ROWS {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(
                            "CSV 행 또는 열 개수가 허용 예산을 초과했습니다.".to_string(),
                        ));
                    }
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
    Table(Vec<Vec<String>>),
}

/// CommonMark Markdown parser and serializer.
pub struct CommonMarkCodec;

impl CommonMarkCodec {
    fn parse_markdown_table_row(inner: &str) -> Vec<String> {
        let mut cells = Vec::new();
        let mut cur = String::new();
        let mut chars = inner.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                if let Some(&next_ch) = chars.peek() {
                    if next_ch == '|' {
                        cur.push('|');
                        chars.next();
                        continue;
                    }
                }
                cur.push(ch);
            } else if ch == '|' {
                cells.push(cur.trim().to_string());
                cur.clear();
            } else {
                cur.push(ch);
            }
        }
        cells.push(cur.trim().to_string());
        cells
    }

    pub fn parse(markdown: &str) -> Result<Vec<MarkdownBlock>, EasyOfficeError> {
        if markdown.as_bytes().contains(&0) {
            return Err(EasyOfficeError::InvalidUtf8(
                "Markdown 스트림에 NUL 바이트가 포함되어 있습니다.".to_string(),
            ));
        }

        let mut blocks: Vec<MarkdownBlock> = Vec::new();
        let mut bullet_acc: Vec<String> = Vec::new();
        let mut numbered_acc: Vec<String> = Vec::new();
        let mut table_acc: Vec<Vec<String>> = Vec::new();
        let mut table_seen_sep = false;

        let flush_lists = |blocks: &mut Vec<MarkdownBlock>,
                           bullet_acc: &mut Vec<String>,
                           numbered_acc: &mut Vec<String>,
                           table_acc: &mut Vec<Vec<String>>,
                           table_seen_sep: &mut bool| {
            if !bullet_acc.is_empty() {
                blocks.push(MarkdownBlock::BulletList(std::mem::take(bullet_acc)));
            }
            if !numbered_acc.is_empty() {
                blocks.push(MarkdownBlock::NumberedList(std::mem::take(numbered_acc)));
            }
            if !table_acc.is_empty() {
                blocks.push(MarkdownBlock::Table(std::mem::take(table_acc)));
            }
            *table_seen_sep = false;
        };

        for raw_line in markdown.lines() {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() {
                flush_lists(
                    &mut blocks,
                    &mut bullet_acc,
                    &mut numbered_acc,
                    &mut table_acc,
                    &mut table_seen_sep,
                );
                continue;
            }

            if trimmed.len() >= 2 && trimmed.starts_with('|') && trimmed.ends_with('|') {
                if !bullet_acc.is_empty() {
                    blocks.push(MarkdownBlock::BulletList(std::mem::take(&mut bullet_acc)));
                }
                if !numbered_acc.is_empty() {
                    blocks.push(MarkdownBlock::NumberedList(std::mem::take(
                        &mut numbered_acc,
                    )));
                }
                let inner = trimmed.strip_prefix('|').unwrap_or(trimmed);
                let inner = inner.strip_suffix('|').unwrap_or(inner);
                let cells = Self::parse_markdown_table_row(inner);
                let is_sep = table_acc.len() == 1
                    && !table_seen_sep
                    && !cells.is_empty()
                    && cells.iter().all(|c| {
                        let core = c.trim().trim_matches(':');
                        core.len() >= 3 && core.chars().all(|ch| ch == '-')
                    });
                if is_sep {
                    table_seen_sep = true;
                } else {
                    table_acc.push(cells);
                }
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
                if !table_acc.is_empty() {
                    blocks.push(MarkdownBlock::Table(std::mem::take(&mut table_acc)));
                    table_seen_sep = false;
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
                    if !table_acc.is_empty() {
                        blocks.push(MarkdownBlock::Table(std::mem::take(&mut table_acc)));
                        table_seen_sep = false;
                    }
                    numbered_acc.push(trimmed[dot_idx + 2..].trim().to_string());
                    continue;
                }
            }

            flush_lists(
                &mut blocks,
                &mut bullet_acc,
                &mut numbered_acc,
                &mut table_acc,
                &mut table_seen_sep,
            );

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

        flush_lists(
            &mut blocks,
            &mut bullet_acc,
            &mut numbered_acc,
            &mut table_acc,
            &mut table_seen_sep,
        );
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
                MarkdownBlock::Table(rows) => {
                    let col_count = rows.iter().map(|r| r.len()).max().unwrap_or(0);
                    if col_count == 0 {
                        continue;
                    }
                    let mut table_lines = Vec::with_capacity(rows.len() + 1);
                    for (r_idx, row) in rows.iter().enumerate() {
                        let mut line = String::from("|");
                        for c_idx in 0..col_count {
                            let cell = row
                                .get(c_idx)
                                .map(|s| s.replace('|', "\\|").replace('\n', " "))
                                .unwrap_or_default();
                            line.push(' ');
                            line.push_str(&cell);
                            line.push_str(" |");
                        }
                        table_lines.push(line);
                        if r_idx == 0 {
                            let mut sep = String::from("|");
                            for _ in 0..col_count {
                                sep.push_str(" --- |");
                            }
                            table_lines.push(sep);
                        }
                    }
                    rendered.push(table_lines.join("\n"));
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum PdfTrailerJsonValue {
    Null,
    Bool,
    Number,
    Str(String),
    Array(Vec<PdfTrailerJsonValue>),
    Object(Vec<(String, PdfTrailerJsonValue)>),
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
        if Self::has_structural_encrypt_token(&raw) {
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
        let mut fallback_stream_buf = String::new();
        let mut current_stamp: Option<String> = None;
        let mut in_stream = false;
        let mut seen_body_line = false;
        let mut is_metadata_obj = false;
        let mut trailer_b64: Option<String> = None;
        let mut trailer_b64_count = 0usize;

        for line in raw.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("%EasyOfficePDFModelBase64:") {
                trailer_b64_count += 1;
                trailer_b64 = Some(rest.trim().to_string());
                continue;
            }
            if trimmed == "endobj" {
                is_metadata_obj = false;
                continue;
            }
            if trimmed.contains("/Type /Metadata")
                || trimmed.contains("/Type /CMap")
                || trimmed == "begincmap"
            {
                is_metadata_obj = true;
            }
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
                fallback_stream_buf.clear();
                current_stamp = None;
                seen_body_line = false;
                continue;
            }
            if trimmed == "endstream" {
                if in_stream {
                    if !is_metadata_obj {
                        let mut fallback_body = String::new();
                        let mut seen_fallback_line = false;
                        for content in Self::extract_pdf_stream_text_segments(&fallback_stream_buf)
                        {
                            if let Some(wm) = content.strip_prefix("[WM] ") {
                                watermark_text = Some(wm.to_string());
                                continue;
                            }
                            if let Some(stamp) = content.strip_prefix("[STAMP] ") {
                                current_stamp = Some(stamp.to_string());
                                continue;
                            }
                            if seen_fallback_line {
                                fallback_body.push('\n');
                            }
                            seen_fallback_line = true;
                            fallback_body.push_str(&content);
                        }
                        let page_body = if seen_body_line {
                            current_body.clone()
                        } else {
                            fallback_body
                        };
                        pages.push(PdfPageModel {
                            page_number: current_page_number.take().unwrap_or(pages.len() + 1),
                            rotation_degrees: current_rotation,
                            body_text: page_body,
                            approval_stamp: current_stamp.take(),
                        });
                    }
                    current_rotation = 0;
                    in_stream = false;
                }
                continue;
            }
            if in_stream && !is_metadata_obj {
                let is_body_line = trimmed.starts_with("BT /F1 12 Tf 54 ")
                    || trimmed.starts_with("BT /F2 12 Tf 54 ")
                    || trimmed.starts_with("BT 3 Tr /F1 1 Tf 0 0 Td ");
                let is_form_meta_line = trimmed.starts_with("BT 3 Tr /F1 1 Tf 0 2 Td ");
                if is_form_meta_line {
                    continue;
                }
                if is_body_line {
                    for content in Self::extract_pdf_stream_text_segments(trimmed) {
                        if seen_body_line {
                            current_body.push('\n');
                        }
                        seen_body_line = true;
                        current_body.push_str(&content);
                    }
                    continue;
                }
                if !fallback_stream_buf.is_empty() {
                    fallback_stream_buf.push('\n');
                }
                fallback_stream_buf.push_str(line);
            }
        }

        if pages.is_empty() {
            return Err(EasyOfficeError::CorruptedContainer(
                "PDF 페이지 스트림을 찾을 수 없습니다.".to_string(),
            ));
        }

        if trailer_b64_count > 1 {
            return Err(EasyOfficeError::SecurityViolation(
                "PDF 트레일러 주석(%EasyOfficePDFModelBase64)이 중복 주입되었습니다.".to_string(),
            ));
        }
        if let Some(b64) = trailer_b64 {
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
            if !Self::validate_pdf_trailer_json(&decoded, &title, &pages, watermark_text.as_deref())
            {
                return Err(EasyOfficeError::SecurityViolation(
                    "PDF 트레일러 주석(%EasyOfficePDFModelBase64)이 본문 스트림과 일치하지 않습니다."
                        .to_string(),
                ));
            }
        }

        Ok(PdfDocumentModel {
            title,
            pages,
            watermark_text,
        })
    }

    fn has_structural_encrypt_token(raw: &str) -> bool {
        let mut in_stream = false;
        let mut structural = String::with_capacity(raw.len().min(4096));
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed == "stream" {
                in_stream = true;
                continue;
            }
            if trimmed == "endstream" {
                in_stream = false;
                continue;
            }
            if in_stream {
                continue;
            }
            let mut chars = trimmed.chars().peekable();
            let mut paren_depth: usize = 0;
            while let Some(c) = chars.next() {
                if paren_depth > 0 {
                    if c == '\\' {
                        let _ = chars.next();
                    } else if c == '(' {
                        paren_depth += 1;
                    } else if c == ')' {
                        paren_depth -= 1;
                    }
                    continue;
                }
                if c == '%' {
                    break;
                }
                if c == '(' {
                    paren_depth = 1;
                    structural.push(' ');
                    continue;
                }
                structural.push(c);
            }
            structural.push('\n');
        }

        let bytes = structural.as_bytes();
        let needle = b"/Encrypt";
        let mut pos = 0;
        while pos + needle.len() <= bytes.len() {
            if let Some(rel) = structural[pos..].find("/Encrypt") {
                let idx = pos + rel;
                let after_idx = idx + needle.len();
                let next_ok = match bytes.get(after_idx) {
                    None => true,
                    Some(b) => b.is_ascii_whitespace() || b"()<>[]{}/%".contains(b),
                };
                if next_ok {
                    return true;
                }
                pos = idx + 1;
            } else {
                break;
            }
        }
        false
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

    fn parse_json_string_literal(
        chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    ) -> Option<String> {
        let mut out = String::new();
        while let Some(c) = chars.next() {
            if c == '"' {
                return Some(out);
            }
            if (c as u32) < 0x20 {
                return None;
            }
            if c != '\\' {
                out.push(c);
                continue;
            }
            match chars.next()? {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                '/' => out.push('/'),
                'b' => out.push('\u{0008}'),
                'f' => out.push('\u{000C}'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'u' => {
                    let mut hex4 = String::with_capacity(4);
                    for _ in 0..4 {
                        hex4.push(chars.next()?);
                    }
                    let hi = u16::from_str_radix(&hex4, 16).ok()?;
                    if (0xD800..=0xDBFF).contains(&hi) {
                        if chars.peek() == Some(&'\\') {
                            let _ = chars.next();
                            if chars.peek() == Some(&'u') {
                                let _ = chars.next();
                                let mut lo_hex = String::with_capacity(4);
                                for _ in 0..4 {
                                    lo_hex.push(chars.next()?);
                                }
                                let lo = u16::from_str_radix(&lo_hex, 16).ok()?;
                                if (0xDC00..=0xDFFF).contains(&lo) {
                                    let cp = 0x10000
                                        + (((hi as u32 - 0xD800) << 10) | (lo as u32 - 0xDC00));
                                    out.push(char::from_u32(cp)?);
                                    continue;
                                }
                            }
                        }
                        return None;
                    }
                    out.push(char::from_u32(hi as u32)?);
                }
                _ => return None,
            }
        }
        None
    }

    fn skip_json_ws(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
        while chars
            .peek()
            .is_some_and(|ch| matches!(ch, ' ' | '\t' | '\r' | '\n'))
        {
            let _ = chars.next();
        }
    }

    fn parse_json_number(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<()> {
        if chars.peek() == Some(&'-') {
            let _ = chars.next();
        }
        match chars.next()? {
            '0' => {}
            '1'..='9' => {
                while chars.peek().is_some_and(|c| c.is_ascii_digit()) {
                    let _ = chars.next();
                }
            }
            _ => return None,
        }
        if chars.peek() == Some(&'.') {
            let _ = chars.next();
            if !chars.peek().is_some_and(|c| c.is_ascii_digit()) {
                return None;
            }
            while chars.peek().is_some_and(|c| c.is_ascii_digit()) {
                let _ = chars.next();
            }
        }
        if chars.peek().is_some_and(|c| *c == 'e' || *c == 'E') {
            let _ = chars.next();
            if chars.peek().is_some_and(|c| *c == '+' || *c == '-') {
                let _ = chars.next();
            }
            if !chars.peek().is_some_and(|c| c.is_ascii_digit()) {
                return None;
            }
            while chars.peek().is_some_and(|c| c.is_ascii_digit()) {
                let _ = chars.next();
            }
        }
        Some(())
    }

    fn consume_json_keyword(
        chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
        kw: &str,
    ) -> Option<()> {
        for expected in kw.chars() {
            if chars.next()? != expected {
                return None;
            }
        }
        Some(())
    }

    fn parse_json_value(
        chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
        depth: usize,
    ) -> Option<PdfTrailerJsonValue> {
        if depth > 32 {
            return None;
        }
        Self::skip_json_ws(chars);
        let peek = *chars.peek()?;
        match peek {
            '"' => {
                let _ = chars.next();
                Some(PdfTrailerJsonValue::Str(Self::parse_json_string_literal(
                    chars,
                )?))
            }
            '{' => {
                let _ = chars.next();
                Self::skip_json_ws(chars);
                let mut entries: Vec<(String, PdfTrailerJsonValue)> = Vec::new();
                if chars.peek() == Some(&'}') {
                    let _ = chars.next();
                    return Some(PdfTrailerJsonValue::Object(entries));
                }
                loop {
                    Self::skip_json_ws(chars);
                    if chars.next()? != '"' {
                        return None;
                    }
                    let key = Self::parse_json_string_literal(chars)?;
                    if entries.iter().any(|(k, _)| k == &key) {
                        return None;
                    }
                    Self::skip_json_ws(chars);
                    if chars.next()? != ':' {
                        return None;
                    }
                    let val = Self::parse_json_value(chars, depth + 1)?;
                    entries.push((key, val));
                    Self::skip_json_ws(chars);
                    match chars.next()? {
                        ',' => continue,
                        '}' => break,
                        _ => return None,
                    }
                }
                Some(PdfTrailerJsonValue::Object(entries))
            }
            '[' => {
                let _ = chars.next();
                Self::skip_json_ws(chars);
                let mut items = Vec::new();
                if chars.peek() == Some(&']') {
                    let _ = chars.next();
                    return Some(PdfTrailerJsonValue::Array(items));
                }
                loop {
                    let val = Self::parse_json_value(chars, depth + 1)?;
                    items.push(val);
                    Self::skip_json_ws(chars);
                    match chars.next()? {
                        ',' => continue,
                        ']' => break,
                        _ => return None,
                    }
                }
                Some(PdfTrailerJsonValue::Array(items))
            }
            't' => {
                Self::consume_json_keyword(chars, "true")?;
                Some(PdfTrailerJsonValue::Bool)
            }
            'f' => {
                Self::consume_json_keyword(chars, "false")?;
                Some(PdfTrailerJsonValue::Bool)
            }
            'n' => {
                Self::consume_json_keyword(chars, "null")?;
                Some(PdfTrailerJsonValue::Null)
            }
            '-' | '0'..='9' => {
                Self::parse_json_number(chars)?;
                Some(PdfTrailerJsonValue::Number)
            }
            _ => None,
        }
    }

    fn parse_json_document(input: &str) -> Option<PdfTrailerJsonValue> {
        let mut chars = input.chars().peekable();
        let root = Self::parse_json_value(&mut chars, 0)?;
        Self::skip_json_ws(&mut chars);
        if chars.peek().is_some() {
            return None;
        }
        Some(root)
    }

    fn strip_ws(s: &str) -> String {
        s.chars().filter(|c| !c.is_whitespace()).collect()
    }

    fn validate_pdf_trailer_json(
        decoded: &str,
        expected_title: &str,
        pages: &[PdfPageModel],
        parsed_watermark: Option<&str>,
    ) -> bool {
        let Some(PdfTrailerJsonValue::Object(root_entries)) = Self::parse_json_document(decoded)
        else {
            return false;
        };
        let Some(PdfTrailerJsonValue::Str(json_title)) = root_entries
            .iter()
            .find(|(k, _)| k == "title")
            .map(|(_, v)| v)
        else {
            return false;
        };
        if json_title != expected_title {
            return false;
        }
        if let Some(PdfTrailerJsonValue::Str(wm)) = root_entries
            .iter()
            .find(|(k, _)| k == "watermarkText")
            .map(|(_, v)| v)
        {
            if !wm.is_empty() && parsed_watermark != Some(wm.as_str()) {
                return false;
            }
        }
        if let Some(pages_val) = root_entries
            .iter()
            .find(|(k, _)| k == "pages")
            .map(|(_, v)| v)
        {
            let PdfTrailerJsonValue::Array(page_items) = pages_val else {
                return false;
            };
            if page_items.is_empty() || page_items.len() > pages.len() {
                return false;
            }
            let mut json_page_texts = Vec::with_capacity(page_items.len());
            for item in page_items {
                let PdfTrailerJsonValue::Object(p_entries) = item else {
                    return false;
                };
                let page_str = p_entries
                    .iter()
                    .find(|(k, _)| k == "extractedText" || k == "bodyText")
                    .and_then(|(_, v)| match v {
                        PdfTrailerJsonValue::Str(s) => Some(s.as_str()),
                        _ => None,
                    });
                let Some(text) = page_str else {
                    return false;
                };
                json_page_texts.push(text);
            }
            if json_page_texts.len() == pages.len() {
                return json_page_texts
                    .iter()
                    .zip(pages.iter())
                    .all(|(jt, p)| Self::strip_ws(jt) == Self::strip_ws(&p.body_text));
            }
            let visible_concat: String = pages
                .iter()
                .map(|p| p.body_text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            return Self::strip_ws(&json_page_texts.join("\n")) == Self::strip_ws(&visible_concat);
        }
        if let Some(PdfTrailerJsonValue::Str(body_str)) = root_entries
            .iter()
            .find(|(k, _)| k == "bodyText")
            .map(|(_, v)| v)
        {
            let visible_concat: String = pages
                .iter()
                .map(|p| p.body_text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            return Self::strip_ws(body_str) == Self::strip_ws(&visible_concat);
        }
        false
    }

    fn extract_pdf_stream_text_segments(stream: &str) -> Vec<String> {
        let bytes = stream.as_bytes();
        let mut i = 0usize;
        let mut in_text_object = false;
        let mut active_font_utf16be = false;
        let mut last_slash_name: Option<&str> = None;
        let mut in_array = false;
        let mut array_buf = String::new();
        let mut array_has_text = false;
        let mut segments = Vec::new();

        while i < bytes.len() {
            let b = bytes[i];
            if b.is_ascii_whitespace() {
                i += 1;
                continue;
            }
            if b == b'%' {
                i += 1;
                while i < bytes.len() && bytes[i] != b'\n' && bytes[i] != b'\r' {
                    i += 1;
                }
                continue;
            }
            if b == b'[' {
                if in_text_object {
                    in_array = true;
                    array_buf.clear();
                    array_has_text = false;
                }
                i += 1;
                continue;
            }
            if b == b']' {
                if in_text_object && in_array {
                    in_array = false;
                    if array_has_text {
                        segments.push(std::mem::take(&mut array_buf));
                    } else {
                        array_buf.clear();
                    }
                    array_has_text = false;
                }
                i += 1;
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
                            let raw_lit = &stream[start_inner..i];
                            if in_text_object {
                                let decoded = Self::unescape_pdf_text(raw_lit);
                                if in_array {
                                    array_buf.push_str(&decoded);
                                    array_has_text = true;
                                } else {
                                    segments.push(decoded);
                                }
                            }
                            i += 1;
                            break;
                        }
                    }
                    i += 1;
                }
                continue;
            }
            if b == b'<' {
                if i + 1 < bytes.len() && bytes[i + 1] == b'<' {
                    i += 2;
                    continue;
                }
                let start_hex = i + 1;
                i += 1;
                while i < bytes.len() && bytes[i] != b'>' {
                    i += 1;
                }
                if i < bytes.len() {
                    let raw_hex = &stream[start_hex..i];
                    if in_text_object {
                        if let Some(decoded) =
                            Self::decode_pdf_hex_string(raw_hex, active_font_utf16be)
                        {
                            if in_array {
                                array_buf.push_str(&decoded);
                                array_has_text = true;
                            } else {
                                segments.push(decoded);
                            }
                        }
                    }
                    i += 1;
                }
                continue;
            }
            if b == b'>' || b == b'{' || b == b'}' {
                i += 1;
                continue;
            }
            if b == b'/' {
                let start = i;
                i += 1;
                while i < bytes.len()
                    && !bytes[i].is_ascii_whitespace()
                    && !b"()<>[]{}/%".contains(&bytes[i])
                {
                    i += 1;
                }
                last_slash_name = Some(&stream[start..i]);
                continue;
            }
            let start = i;
            while i < bytes.len()
                && !bytes[i].is_ascii_whitespace()
                && !b"()<>[]{}/%".contains(&bytes[i])
            {
                i += 1;
            }
            let tok = &stream[start..i];
            if !in_text_object {
                if tok == "BT" {
                    in_text_object = true;
                    active_font_utf16be = false;
                    last_slash_name = None;
                    in_array = false;
                    array_buf.clear();
                    array_has_text = false;
                }
            } else if tok == "Tf" {
                if let Some(font_name) = last_slash_name {
                    active_font_utf16be = font_name == "/F2";
                }
            } else if tok == "ET" {
                in_text_object = false;
                active_font_utf16be = false;
                last_slash_name = None;
                in_array = false;
                array_buf.clear();
                array_has_text = false;
            }
        }
        segments
    }

    fn decode_pdf_hex_string(hex: &str, prefer_utf16be: bool) -> Option<String> {
        let mut clean: Vec<u8> = hex.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
        if clean.is_empty() {
            return Some(String::new());
        }
        if !clean.iter().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        if clean.len() % 2 != 0 {
            clean.push(b'0');
        }
        let mut bytes = Vec::with_capacity(clean.len() / 2);
        let mut idx = 0usize;
        while idx + 2 <= clean.len() {
            let chunk = std::str::from_utf8(&clean[idx..idx + 2]).ok()?;
            bytes.push(u8::from_str_radix(chunk, 16).ok()?);
            idx += 2;
        }
        if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
            if rest.len() % 2 != 0 {
                return None;
            }
            let units: Vec<u16> = rest
                .chunks_exact(2)
                .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                .collect();
            return Some(String::from_utf16_lossy(&units));
        }
        if prefer_utf16be && bytes.len() % 2 == 0 {
            let units: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                .collect();
            return Some(String::from_utf16_lossy(&units));
        }
        match String::from_utf8(bytes) {
            Ok(s) => Some(s),
            Err(e) => Some(e.into_bytes().into_iter().map(char::from).collect()),
        }
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
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('r') => out.push('\r'),
                    Some('t') => out.push('\t'),
                    Some('b') => out.push('\u{0008}'),
                    Some('f') => out.push('\u{000C}'),
                    Some('\r') => {
                        if chars.peek() == Some(&'\n') {
                            let _ = chars.next();
                        }
                    }
                    Some('\n') => {}
                    Some(oct @ '0'..='7') => {
                        let mut val = (oct as u32) - ('0' as u32);
                        for _ in 0..2 {
                            if let Some(&next_oct @ '0'..='7') = chars.peek() {
                                let _ = chars.next();
                                val = (val * 8) + ((next_oct as u32) - ('0' as u32));
                            } else {
                                break;
                            }
                        }
                        out.push(char::from((val & 0xFF) as u8));
                    }
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

    #[test]
    fn test_f5_ods_xlsx_hwpx_and_pdf_standard_compatibility() {
        use crate::{
            HwpxCodec, HwpxPayload, OdfCodec, OoxmlCodec, OoxmlXlsxPayload, ZipArchiveCodec,
            ZipSecurityPolicy,
        };

        let ods_bytes = OdfCodec::pack_ods(
            "예산표",
            &[
                (0, 0, "항목".to_string()),
                (2, 3, "450".to_string()),
                (4, 1, "=D3*2".to_string()),
            ],
        )
        .unwrap();
        let ods_entries =
            ZipArchiveCodec::unpack(&ods_bytes, &ZipSecurityPolicy::default()).unwrap();
        let ods_xml = std::str::from_utf8(
            &ods_entries
                .iter()
                .find(|e| e.path == "content.xml")
                .unwrap()
                .data,
        )
        .unwrap();
        assert!(
            ods_xml.matches("<table:table-row").count() >= 3,
            "pack_ods must emit multiple table:table-row elements: {ods_xml}"
        );

        let std_ods_xml = concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" office:version=\"1.2\">",
            "<office:body><office:spreadsheet><table:table table:name=\"표준시트\">",
            "<table:table-row>",
            "<table:table-cell office:value-type=\"string\"><text:p>A1</text:p></table:table-cell>",
            "<table:table-cell table:number-columns-repeated=\"2\"/>",
            "<table:table-cell office:value-type=\"float\" office:value=\"42\"><text:p>42</text:p></table:table-cell>",
            "</table:table-row>",
            "<table:table-row table:number-rows-repeated=\"2\"><table:table-cell/></table:table-row>",
            "<table:table-row>",
            "<table:table-cell table:number-columns-repeated=\"1\"/>",
            "<table:table-cell table:formula=\"of:=SUM(D1:D3)\" office:value-type=\"float\" office:value=\"42\"><text:p>42</text:p></table:table-cell>",
            "</table:table-row>",
            "</table:table></office:spreadsheet></office:body></office:document-content>"
        );
        let std_ods_zip = ZipArchiveCodec::pack(&[
            (
                "mimetype".to_string(),
                OdfCodec::MIMETYPE_ODS.as_bytes().to_vec(),
            ),
            ("content.xml".to_string(), std_ods_xml.as_bytes().to_vec()),
        ])
        .unwrap();
        let (sheet_name, unpacked_cells) = OdfCodec::unpack_ods(&std_ods_zip).unwrap();
        assert_eq!(sheet_name, "표준시트");
        assert!(unpacked_cells.contains(&(0, 0, "A1".to_string())));
        assert!(unpacked_cells.contains(&(0, 3, "42".to_string())));
        assert!(unpacked_cells.contains(&(3, 1, "=SUM(D1:D3)".to_string())));

        let xlsx_bytes = OoxmlCodec::pack_xlsx(&OoxmlXlsxPayload {
            sheet_name: "Sheet1".to_string(),
            cells: vec![(0, 0, "A1".to_string()), (2, 1, "B3".to_string())],
        })
        .unwrap();
        let xlsx_entries =
            ZipArchiveCodec::unpack(&xlsx_bytes, &ZipSecurityPolicy::default()).unwrap();
        let sheet_xml = std::str::from_utf8(
            &xlsx_entries
                .iter()
                .find(|e| e.path == "xl/worksheets/sheet1.xml")
                .unwrap()
                .data,
        )
        .unwrap();
        assert!(sheet_xml.contains("<row r=\"1\">"));
        assert!(sheet_xml.contains("<row r=\"3\">"));

        let hwpx_bytes = HwpxCodec::pack(&HwpxPayload {
            title: "기안문".to_string(),
            is_template: false,
            official_agency: Some("행정안전부".to_string()),
            approval_lines: vec!["기안".to_string()],
            paragraphs: vec!["본문".to_string()],
            tables: Vec::new(),
        })
        .unwrap();
        let hwpx_entries =
            ZipArchiveCodec::unpack(&hwpx_bytes, &ZipSecurityPolicy::default()).unwrap();
        assert!(
            hwpx_entries
                .iter()
                .any(|e| e.path == "Contents/content.hpf"),
            "HwpxCodec::pack must include Contents/content.hpf"
        );

        let pdf_with_literal_encrypt = PdfSerializer::serialize(&PdfDocumentModel {
            title: "Guide to /Encrypt in PDF".to_string(),
            pages: vec![PdfPageModel {
                page_number: 1,
                rotation_degrees: 0,
                body_text: "Section 1 discusses /Encrypt dictionary.".to_string(),
                approval_stamp: None,
            }],
            watermark_text: None,
        });
        assert!(
            PdfSerializer::parse(&pdf_with_literal_encrypt).is_ok(),
            "Literal /Encrypt inside text or stream must not trigger false positive"
        );

        let mut pdf_with_b64_json = pdf_with_literal_encrypt.clone();
        let b64_trailer = "\n%EasyOfficePDFModelBase64:eyJ0aXRsZSI6Ikd1aWRlIHRvIC9FbmNyeXB0IGluIFBERiIsImJvZHlUZXh0IjoiU2VjdGlvbiAxIGRpc2N1c3NlcyAvRW5jcnlwdCBkaWN0aW9uYXJ5LiJ9\n";
        pdf_with_b64_json.extend_from_slice(b64_trailer.as_bytes());
        assert!(
            PdfSerializer::parse(&pdf_with_b64_json).is_ok(),
            "Valid Base64 JSON trailer must be accepted"
        );

        let mut pdf_with_spoofed_b64_json = pdf_with_literal_encrypt;
        let spoofed_trailer = "\n%EasyOfficePDFModelBase64:eyJ0aXRsZSI6IlNwb29mZWQiLCJib2R5VGV4dCI6IkNvbXBsZXRlbHkgZGlmZmVyZW50IGNvbnRlbnQifQ==\n";
        pdf_with_spoofed_b64_json.extend_from_slice(spoofed_trailer.as_bytes());
        assert!(matches!(
            PdfSerializer::parse(&pdf_with_spoofed_b64_json),
            Err(EasyOfficeError::SecurityViolation(_))
        ));

        let korean_pdf_model = PdfDocumentModel {
            title: "한글 보고서".to_string(),
            pages: vec![PdfPageModel {
                page_number: 1,
                rotation_degrees: 0,
                body_text: "첫째 줄 한글 본문\nASCII Line 2".to_string(),
                approval_stamp: Some("전결승인".to_string()),
            }],
            watermark_text: Some("대외비문서".to_string()),
        };
        let korean_pdf_bytes = PdfSerializer::serialize(&korean_pdf_model);
        let korean_pdf_raw = String::from_utf8_lossy(&korean_pdf_bytes);
        assert!(korean_pdf_raw.contains("BT /F2 12 Tf 54 760 Td <"));
        assert!(korean_pdf_raw.contains("BT /F1 12 Tf 54 744 Td (ASCII Line 2) Tj ET"));
        assert!(korean_pdf_raw.contains("BT /F2 10 Tf 386 604 Td <"));
        assert!(korean_pdf_raw.contains("BT /F2 10 Tf 320 746 Td <"));
        assert!(!korean_pdf_raw.contains("(첫째 줄 한글 본문)"));
        let parsed_korean_pdf = PdfSerializer::parse(&korean_pdf_bytes).unwrap();
        assert_eq!(parsed_korean_pdf, korean_pdf_model);

        let leading_blank_pdf_model = PdfDocumentModel {
            title: "빈 줄 보존 테스트".to_string(),
            pages: vec![PdfPageModel {
                page_number: 1,
                rotation_degrees: 0,
                body_text: "\n둘째 줄 한글 본문".to_string(),
                approval_stamp: None,
            }],
            watermark_text: None,
        };
        let leading_blank_bytes = PdfSerializer::serialize(&leading_blank_pdf_model);
        let parsed_leading_blank = PdfSerializer::parse(&leading_blank_bytes).unwrap();
        assert_eq!(parsed_leading_blank, leading_blank_pdf_model);

        let newline_only_pdf_bytes = PdfSerializer::serialize(&PdfDocumentModel {
            title: "줄바꿈 전용 문서".to_string(),
            pages: vec![PdfPageModel {
                page_number: 1,
                rotation_degrees: 0,
                body_text: "\n".to_string(),
                approval_stamp: None,
            }],
            watermark_text: None,
        });
        let mut spoofed_newline_pdf = newline_only_pdf_bytes;
        spoofed_newline_pdf.extend_from_slice(spoofed_trailer.as_bytes());
        assert!(matches!(
            PdfSerializer::parse(&spoofed_newline_pdf),
            Err(EasyOfficeError::SecurityViolation(_))
        ));

        let external_pdf = concat!(
            "%PDF-1.7\n",
            "1 0 obj << /Type /Catalog /Pages 2 0 R /Title (External Spec) >> endobj\n",
            "2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n",
            "3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R >> endobj\n",
            "4 0 obj << /Length 98 >>\nstream\n",
            "BT /F1 11 Tf 72 720 Td (External Line 1) Tj ET\n",
            "BT /F2 11 Tf 72 704 Td <FEFFAC00B098> Tj ET\n",
            "endstream\nendobj\n",
            "xref\n0 5\ntrailer << /Root 1 0 R /Size 5 >>\nstartxref\n0\n%%EOF\n"
        );
        let parsed_external = PdfSerializer::parse(external_pdf.as_bytes()).unwrap();
        assert_eq!(parsed_external.title, "External Spec");
        assert_eq!(parsed_external.pages[0].body_text, "External Line 1\n가나");

        let multiline_external_pdf = concat!(
            "%PDF-1.7\n",
            "1 0 obj << /Type /Catalog /Pages 2 0 R /Title (Multi-Line External) >> endobj\n",
            "2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n",
            "3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R >> endobj\n",
            "4 0 obj << /Length 180 >>\nstream\n",
            "q 0 0 0 rg BT /F1 11 Tf 72 740 Td (Prefixed Header) Tj ET Q\n",
            "BT\n",
            "/F1 11 Tf\n",
            "72 720 Td\n",
            "[(Hel) -15 (lo) 20 ( ) <576F726C64>] TJ\n",
            "0 -16 Td\n",
            "<48656C6C> Tj\n",
            "0 -16 Td\n",
            "<FEFFAC00B098> Tj\n",
            "ET\n",
            "endstream\nendobj\n",
            "xref\n0 5\ntrailer << /Root 1 0 R /Size 5 >>\nstartxref\n0\n%%EOF\n"
        );
        let parsed_multiline = PdfSerializer::parse(multiline_external_pdf.as_bytes()).unwrap();
        assert_eq!(
            parsed_multiline.pages[0].body_text,
            "Prefixed Header\nHello World\nHell\n가나"
        );

        let empty_title_whitespace_pdf = PdfSerializer::serialize(&PdfDocumentModel {
            title: String::new(),
            pages: vec![PdfPageModel {
                page_number: 1,
                rotation_degrees: 0,
                body_text: "   ".to_string(),
                approval_stamp: None,
            }],
            watermark_text: None,
        });
        let mut non_json_trailer_pdf = empty_title_whitespace_pdf.clone();
        non_json_trailer_pdf.extend_from_slice(b"\n%EasyOfficePDFModelBase64:SGVsbG8gV29ybGQ=\n");
        assert!(matches!(
            PdfSerializer::parse(&non_json_trailer_pdf),
            Err(EasyOfficeError::SecurityViolation(_))
        ));

        let mut spoofed_empty_title_pdf = empty_title_whitespace_pdf.clone();
        spoofed_empty_title_pdf.extend_from_slice(
            b"\n%EasyOfficePDFModelBase64:eyJ0aXRsZSI6IkluamVjdGVkIiwiYm9keVRleHQiOiIifQ==\n",
        );
        assert!(matches!(
            PdfSerializer::parse(&spoofed_empty_title_pdf),
            Err(EasyOfficeError::SecurityViolation(_))
        ));

        let mut spoofed_ws_body_pdf = empty_title_whitespace_pdf.clone();
        spoofed_ws_body_pdf.extend_from_slice(
            b"\n%EasyOfficePDFModelBase64:eyJ0aXRsZSI6IiIsImJvZHlUZXh0IjoiU3Bvb2ZlZCJ9\n",
        );
        assert!(matches!(
            PdfSerializer::parse(&spoofed_ws_body_pdf),
            Err(EasyOfficeError::SecurityViolation(_))
        ));

        let mut valid_empty_pdf = empty_title_whitespace_pdf.clone();
        valid_empty_pdf.extend_from_slice(
            b"\n%EasyOfficePDFModelBase64:eyJ0aXRsZSI6IiIsInBhZ2VzIjpbeyJleHRyYWN0ZWRUZXh0IjoiICAgIn1dfQ==\n",
        );
        assert!(PdfSerializer::parse(&valid_empty_pdf).is_ok());

        let swift_outline_json = r#"{"annotations":[],"formFields":[],"id":"00000000-0000-0000-0000-000000000001","inkStrokes":[],"isRedacted":false,"outline":[{"children":[],"id":"00000000-0000-0000-0000-000000000002","targetPageNumber":1,"title":"1장 개요"}],"pages":[{"extractedText":"본문 단락","id":"00000000-0000-0000-0000-000000000003","pageNumber":1,"rotationDegrees":0}],"redactedPhrases":[],"title":"연간계약서","watermarkText":""}"#;
        let mut swift_outline_pdf = PdfSerializer::serialize(&PdfDocumentModel {
            title: "연간계약서".to_string(),
            pages: vec![PdfPageModel {
                page_number: 1,
                rotation_degrees: 0,
                body_text: "본문 단락".to_string(),
                approval_stamp: None,
            }],
            watermark_text: None,
        });
        let b64_encode = |raw: &[u8]| -> String {
            const TBL: &[u8; 64] =
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let mut out = String::new();
            for chunk in raw.chunks(3) {
                let b0 = chunk[0];
                let b1 = *chunk.get(1).unwrap_or(&0);
                let b2 = *chunk.get(2).unwrap_or(&0);
                out.push(TBL[(b0 >> 2) as usize] as char);
                out.push(TBL[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
                if chunk.len() > 1 {
                    out.push(TBL[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
                } else {
                    out.push('=');
                }
                if chunk.len() > 2 {
                    out.push(TBL[(b2 & 0x3F) as usize] as char);
                } else {
                    out.push('=');
                }
            }
            out
        };
        swift_outline_pdf.extend_from_slice(
            format!(
                "\n%EasyOfficePDFModelBase64:{}\n",
                b64_encode(swift_outline_json.as_bytes())
            )
            .as_bytes(),
        );
        assert!(PdfSerializer::parse(&swift_outline_pdf).is_ok());

        for bad_json in [
            r#"{"title":"","title":"Spoofed","pages":[{"extractedText":"   "}]}"#,
            r#"{ @#$ "title":"","pages":[{"extractedText":"   "}] }"#,
            r#"{"title":"","pages":[{"extractedText":"   "}]} TRAILING"#,
        ] {
            let mut bad_pdf = empty_title_whitespace_pdf.clone();
            bad_pdf.extend_from_slice(
                format!(
                    "\n%EasyOfficePDFModelBase64:{}\n",
                    b64_encode(bad_json.as_bytes())
                )
                .as_bytes(),
            );
            assert!(matches!(
                PdfSerializer::parse(&bad_pdf),
                Err(EasyOfficeError::SecurityViolation(_))
            ));
        }

        let two_page_pdf = PdfSerializer::serialize(&PdfDocumentModel {
            title: "TwoPages".to_string(),
            pages: vec![
                PdfPageModel {
                    page_number: 1,
                    rotation_degrees: 0,
                    body_text: "A".to_string(),
                    approval_stamp: None,
                },
                PdfPageModel {
                    page_number: 2,
                    rotation_degrees: 0,
                    body_text: "B".to_string(),
                    approval_stamp: None,
                },
            ],
            watermark_text: None,
        });
        let mut boundary_spoof_pdf = two_page_pdf;
        let boundary_json =
            r#"{"title":"TwoPages","pages":[{"extractedText":"AB"},{"extractedText":""}]}"#;
        boundary_spoof_pdf.extend_from_slice(
            format!(
                "\n%EasyOfficePDFModelBase64:{}\n",
                b64_encode(boundary_json.as_bytes())
            )
            .as_bytes(),
        );
        assert!(matches!(
            PdfSerializer::parse(&boundary_spoof_pdf),
            Err(EasyOfficeError::SecurityViolation(_))
        ));

        let multiline_tj_comment_pdf = b"%PDF-1.7\n1 0 obj << /Type /Catalog /Title (StreamEdge) >> endobj\n2 0 obj << /Type /Page /PageNumber 1 >> endobj\n3 0 obj << /Length 160 >>\nstream\nBT\n[3 2] 0 d\n/F1 11 Tf\n[(Hel) -15 (lo )\n(World)] TJ % inline (FakeComment) ET\n(\\101\\tLine\\\nCont) Tj\nET\nendstream\nendobj\nxref\n0 4\ntrailer << /Size 4 /Root 1 0 R >>\nstartxref\n0\n%%EOF\n";
        let parsed_edge = PdfSerializer::parse(multiline_tj_comment_pdf).unwrap();
        assert_eq!(parsed_edge.pages[0].body_text, "Hello World\nA\tLineCont");

        assert_eq!(PdfSerializer::decode_base64_bytes("YQ==YQ=="), None);
    }

    #[test]
    fn test_f14_csv_scalar_baseline_enforces_max_cols() {
        let too_many_cols = "a,".repeat(CsvCodec::MAX_COLS + 1);
        assert!(matches!(
            CsvCodec::parse_scalar_baseline(&too_many_cols, ','),
            Err(EasyOfficeError::ResourceBudgetExceeded(_))
        ));
    }
}

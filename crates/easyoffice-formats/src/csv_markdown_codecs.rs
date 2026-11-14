use std::borrow::Cow;

use crate::decode_budget::DecodeBudget;
use crate::{EasyOfficeError, XmlSanitizer};

pub struct CsvCodec;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum CsvParseState {
    #[default]
    Start,
    InUnquoted,
    InQuoted,
    QuoteInQuoted,
}

fn csv_budget_error() -> EasyOfficeError {
    EasyOfficeError::ResourceBudgetExceeded(
        "CSV 행 또는 열 개수가 허용 예산을 초과했습니다.".to_string(),
    )
}

fn csv_unexpected_quote_error() -> EasyOfficeError {
    EasyOfficeError::CorruptedContainer(
        "비따옴표 CSV 필드 내부에 예기치 않은 따옴표가 발견되었습니다.".to_string(),
    )
}

fn csv_unclosed_quote_error() -> EasyOfficeError {
    EasyOfficeError::CorruptedContainer("닫히지 않은 따옴표 CSV 필드가 있습니다.".to_string())
}

/// Byte-level SWAR CSV scanner state (positions index into `bytes`).
struct SwarCsvScanner<'a> {
    input: &'a str,
    bytes: &'a [u8],
    delim_b: u8,
    row_idx: usize,
    col_idx: usize,
    max_cols: usize,
    pos: usize,
    trailing_delim: bool,
    cell_count: usize,
    budget: DecodeBudget,
}

impl<'a> SwarCsvScanner<'a> {
    fn new(input: &'a str, delim_b: u8) -> Self {
        Self {
            input,
            bytes: input.as_bytes(),
            delim_b,
            row_idx: 0,
            col_idx: 0,
            max_cols: 0,
            pos: 0,
            trailing_delim: false,
            cell_count: 0,
            budget: DecodeBudget::new(),
        }
    }

    #[inline]
    fn check_budget(&self) -> Result<(), EasyOfficeError> {
        if self.col_idx >= CsvCodec::MAX_COLS || self.row_idx >= CsvCodec::MAX_ROWS {
            return Err(csv_budget_error());
        }
        Ok(())
    }

    /// Delivers a field and advances to the next column.
    #[inline]
    fn emit<F: FnMut(usize, usize, Cow<'a, str>)>(
        &mut self,
        on_field: &mut F,
        field: Cow<'a, str>,
    ) -> Result<(), EasyOfficeError> {
        self.cell_count += 1;
        if self.cell_count > CsvCodec::MAX_GRID_CELLS {
            return Err(csv_budget_error());
        }
        self.budget.charge_cell(field.len(), "CSV/TSV 셀")?;
        on_field(self.row_idx, self.col_idx, field);
        self.col_idx += 1;
        Ok(())
    }

    #[inline]
    fn end_row(&mut self) {
        self.max_cols = self.max_cols.max(self.col_idx);
        self.row_idx += 1;
        self.col_idx = 0;
    }

    /// Index after the line break whose first byte is at `at` (`\r\n` counts as one).
    #[inline]
    fn after_eol(&self, at: usize) -> usize {
        if self.bytes[at] == b'\r' && self.bytes.get(at + 1) == Some(&b'\n') {
            at + 2
        } else {
            at + 1
        }
    }

    /// Reads a quoted field whose opening quote was consumed; leaves `pos` after the closing quote.
    fn quoted_field(&mut self) -> Result<Cow<'a, str>, EasyOfficeError> {
        let (input, bytes) = (self.input, self.bytes);
        let first_start = self.pos;
        let first_q = XmlSanitizer::find_byte_swar(bytes, first_start, b'"')
            .ok_or_else(csv_unclosed_quote_error)?;
        if bytes.get(first_q + 1) != Some(&b'"') {
            self.pos = first_q + 1;
            return Ok(Cow::Borrowed(&input[first_start..first_q]));
        }
        let mut owned = String::with_capacity(32);
        owned.push_str(&input[first_start..first_q]);
        owned.push('"');
        let mut pos = first_q + 2;
        loop {
            let q_pos = XmlSanitizer::find_byte_swar(bytes, pos, b'"')
                .ok_or_else(csv_unclosed_quote_error)?;
            owned.push_str(&input[pos..q_pos]);
            pos = q_pos + 1;
            if bytes.get(pos) != Some(&b'"') {
                break;
            }
            owned.push('"');
            pos += 1;
        }
        self.pos = pos;
        Ok(Cow::Owned(owned))
    }

    /// Scans one field or separator; returns `false` when the input ended.
    fn step<F: FnMut(usize, usize, Cow<'a, str>)>(
        &mut self,
        on_field: &mut F,
    ) -> Result<bool, EasyOfficeError> {
        let b = self.bytes[self.pos];
        if b == b'"' {
            return self.step_quoted(on_field);
        }
        if b == self.delim_b {
            self.emit(on_field, Cow::Borrowed(""))?;
            self.pos += 1;
            self.trailing_delim = true;
            return Ok(true);
        }
        if b == b'\r' || b == b'\n' {
            self.pos = self.after_eol(self.pos);
            self.emit(on_field, Cow::Borrowed(""))?;
            self.end_row();
            self.trailing_delim = false;
            return Ok(true);
        }
        self.step_unquoted(on_field)
    }

    fn step_quoted<F: FnMut(usize, usize, Cow<'a, str>)>(
        &mut self,
        on_field: &mut F,
    ) -> Result<bool, EasyOfficeError> {
        self.trailing_delim = false;
        self.pos += 1;
        let field = self.quoted_field()?;
        let Some(&next_b) = self.bytes.get(self.pos) else {
            self.emit(on_field, field)?;
            self.end_row();
            return Ok(false);
        };
        if next_b == self.delim_b {
            self.emit(on_field, field)?;
            self.pos += 1;
            self.trailing_delim = true;
        } else if next_b == b'\r' || next_b == b'\n' {
            self.pos = self.after_eol(self.pos);
            self.emit(on_field, field)?;
            self.end_row();
        } else {
            return Err(EasyOfficeError::CorruptedContainer(
                "따옴표 CSV 필드 닫힘 이후 잘못된 문자가 발견되었습니다.".to_string(),
            ));
        }
        Ok(true)
    }

    fn step_unquoted<F: FnMut(usize, usize, Cow<'a, str>)>(
        &mut self,
        on_field: &mut F,
    ) -> Result<bool, EasyOfficeError> {
        self.trailing_delim = false;
        let pos = self.pos;
        let end_pos = CsvCodec::find_csv_unquoted_special_swar(self.bytes, pos, self.delim_b);
        let field = Cow::Borrowed(&self.input[pos..end_pos]);
        if end_pos == self.bytes.len() {
            self.emit(on_field, field)?;
            self.end_row();
            return Ok(false);
        }
        let term = self.bytes[end_pos];
        if term == b'"' {
            return Err(csv_unexpected_quote_error());
        }
        self.emit(on_field, field)?;
        if term == self.delim_b {
            self.pos = end_pos + 1;
            self.trailing_delim = true;
        } else {
            self.end_row();
            self.pos = self.after_eol(end_pos);
        }
        Ok(true)
    }

    /// Emits the final empty field after a trailing delimiter and returns `(rows, max_cols)`.
    fn finish<F: FnMut(usize, usize, Cow<'a, str>)>(
        mut self,
        on_field: &mut F,
    ) -> Result<(usize, usize), EasyOfficeError> {
        if self.trailing_delim && self.col_idx > 0 {
            self.check_budget()?;
            self.emit(on_field, Cow::Borrowed(""))?;
            self.max_cols = self.max_cols.max(self.col_idx);
            self.row_idx += 1;
        }
        Ok((self.row_idx, self.max_cols))
    }
}

/// Scalar (character-level) CSV parser used for non-ASCII delimiters.
#[derive(Default)]
struct ScalarCsvParser {
    rows: Vec<Vec<String>>,
    current_row: Vec<String>,
    current_field: String,
    state: CsvParseState,
    cell_count: usize,
    budget: DecodeBudget,
}

impl ScalarCsvParser {
    fn check_budget(&self) -> Result<(), EasyOfficeError> {
        if self.current_row.len() >= CsvCodec::MAX_COLS || self.rows.len() >= CsvCodec::MAX_ROWS {
            return Err(csv_budget_error());
        }
        Ok(())
    }

    fn push_cell(&mut self, text: String) -> Result<(), EasyOfficeError> {
        self.check_budget()?;
        self.cell_count += 1;
        if self.cell_count > CsvCodec::MAX_GRID_CELLS {
            return Err(csv_budget_error());
        }
        self.budget.charge_cell(text.len(), "CSV/TSV 셀")?;
        self.current_row.push(text);
        Ok(())
    }

    /// Ends the current field (budget checked) and returns to the start state.
    fn end_field(&mut self) -> Result<(), EasyOfficeError> {
        let field = std::mem::take(&mut self.current_field);
        self.push_cell(field)?;
        self.state = CsvParseState::Start;
        Ok(())
    }

    /// Ends the current field and row.
    fn end_row(&mut self) -> Result<(), EasyOfficeError> {
        self.end_field()?;
        self.rows.push(std::mem::take(&mut self.current_row));
        Ok(())
    }

    fn feed(
        &mut self,
        ch: char,
        delimiter: char,
        chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    ) -> Result<(), EasyOfficeError> {
        let is_eol = ch == '\r' || ch == '\n';
        if is_eol
            && ch == '\r'
            && chars.peek() == Some(&'\n')
            && self.state != CsvParseState::InQuoted
        {
            chars.next();
        }
        match self.state {
            CsvParseState::Start if ch == '"' => {
                self.current_field.clear();
                self.state = CsvParseState::InQuoted;
            }
            CsvParseState::Start if ch == delimiter || is_eol => {
                // An empty field: `current_field` is empty in the start state.
                if is_eol {
                    self.end_row()?;
                } else {
                    self.push_cell(String::new())?;
                }
            }
            CsvParseState::Start => {
                self.current_field.clear();
                self.current_field.push(ch);
                self.state = CsvParseState::InUnquoted;
            }
            CsvParseState::InUnquoted if ch == '"' => return Err(csv_unexpected_quote_error()),
            CsvParseState::InQuoted => {
                if ch == '"' {
                    self.state = CsvParseState::QuoteInQuoted;
                } else {
                    self.current_field.push(ch);
                }
            }
            CsvParseState::QuoteInQuoted if ch == '"' => {
                self.current_field.push('"');
                self.state = CsvParseState::InQuoted;
            }
            CsvParseState::QuoteInQuoted if !(ch == delimiter || is_eol) => {
                return Err(EasyOfficeError::CorruptedContainer(
                    "따옴표 CSV 필드 닫힘 이후 잘못된 문자가 발견되었습니다.".to_string(),
                ));
            }
            CsvParseState::InUnquoted | CsvParseState::QuoteInQuoted => {
                if is_eol {
                    self.end_row()?;
                } else if ch == delimiter {
                    self.end_field()?;
                } else {
                    self.current_field.push(ch);
                }
            }
        }
        Ok(())
    }

    fn finish(mut self) -> Result<Vec<Vec<String>>, EasyOfficeError> {
        match self.state {
            CsvParseState::InQuoted => return Err(csv_unclosed_quote_error()),
            CsvParseState::QuoteInQuoted | CsvParseState::InUnquoted => {
                let tail = std::mem::take(&mut self.current_field);
                self.push_cell(tail)?;
                self.rows.push(std::mem::take(&mut self.current_row));
            }
            CsvParseState::Start => {
                if !self.current_row.is_empty() {
                    self.push_cell(String::new())?;
                    self.rows.push(std::mem::take(&mut self.current_row));
                }
            }
        }
        Ok(self.rows)
    }
}

impl CsvCodec {
    pub const MAX_ROWS: usize = 1_000_000;
    pub const MAX_COLS: usize = 16_384;
    /// Cells (rows x columns) of the text grid a sheet is written from.
    pub const MAX_GRID_CELLS: usize = 2_000_000;

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

    #[inline]
    fn find_byte_swar(bytes: &[u8], start: usize, target: u8) -> Option<usize> {
        const LO: u64 = 0x0101_0101_0101_0101;
        const HI: u64 = 0x8080_8080_8080_8080;
        let splat = (target as u64).wrapping_mul(LO);
        let mut idx = start;
        while idx + 8 <= bytes.len() {
            let word = u64::from_le_bytes(bytes[idx..idx + 8].try_into().unwrap());
            let d = word ^ splat;
            let mask = d.wrapping_sub(LO) & !d & HI;
            if mask != 0 {
                return Some(idx + ((mask.trailing_zeros() >> 3) as usize));
            }
            idx += 8;
        }
        while idx < bytes.len() {
            if bytes[idx] == target {
                return Some(idx);
            }
            idx += 1;
        }
        None
    }

    pub fn visit_fields_swar<'a, F>(
        input: &'a str,
        delimiter: char,
        mut on_field: F,
    ) -> Result<(usize, usize), EasyOfficeError>
    where
        F: FnMut(usize, usize, Cow<'a, str>),
    {
        let input = input.strip_prefix('\u{FEFF}').unwrap_or(input);
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

        let mut scanner = SwarCsvScanner::new(input, delimiter as u8);
        while scanner.pos < scanner.bytes.len() {
            scanner.check_budget()?;
            if !scanner.step(&mut on_field)? {
                break;
            }
        }
        scanner.finish(&mut on_field)
    }

    /// UTF-8 byte-order mark written before CSV/TSV output so spreadsheet applications on
    /// Korean-locale systems decode the text as UTF-8 rather than the legacy code page.
    pub const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

    /// Serializes rows and prefixes the UTF-8 BOM ([`Self::UTF8_BOM`]) in a single buffer.
    pub fn serialize_with_bom(rows: &[Vec<String>], delimiter: char) -> Vec<u8> {
        let mut out =
            String::with_capacity(Self::UTF8_BOM.len() + Self::estimate_serialized_bytes(rows));
        out.push('\u{FEFF}');
        Self::serialize_into(&mut out, rows, delimiter);
        out.into_bytes()
    }

    pub fn parse(input: &str, delimiter: char) -> Result<Vec<Vec<String>>, EasyOfficeError> {
        let input = input.strip_prefix('\u{FEFF}').unwrap_or(input);
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
        let input = input.strip_prefix('\u{FEFF}').unwrap_or(input);
        if input.is_empty() {
            return Ok(Vec::new());
        }
        let mut parser = ScalarCsvParser::default();
        let mut chars = input.chars().peekable();
        while let Some(ch) = chars.next() {
            parser.feed(ch, delimiter, &mut chars)?;
        }
        parser.finish()
    }

    fn estimate_serialized_bytes(rows: &[Vec<String>]) -> usize {
        rows.iter()
            .map(|r| r.iter().map(|f| f.len() + 2).sum::<usize>() + 2)
            .sum()
    }

    fn serialize_into(out: &mut String, rows: &[Vec<String>], delimiter: char) {
        for (r_idx, row) in rows.iter().enumerate() {
            if r_idx > 0 {
                out.push_str("\r\n");
            }
            for (c_idx, field) in row.iter().enumerate() {
                if c_idx > 0 {
                    out.push(delimiter);
                }
                Self::write_serialized_field(out, field, delimiter);
            }
        }
    }

    pub fn serialize(rows: &[Vec<String>], delimiter: char) -> String {
        let mut out = String::with_capacity(Self::estimate_serialized_bytes(rows));
        Self::serialize_into(&mut out, rows, delimiter);
        out
    }

    fn write_serialized_field(out: &mut String, field: &str, delimiter: char) {
        let (hit, hit_is_quote) = if delimiter.is_ascii() {
            let bytes = field.as_bytes();
            let hit = Self::find_csv_unquoted_special_swar(bytes, 0, delimiter as u8);
            if hit == bytes.len() {
                out.push_str(field);
                return;
            }
            out.push('"');
            out.push_str(&field[..hit]);
            let mut start = if bytes[hit] == b'"' {
                out.push_str("\"\"");
                hit + 1
            } else {
                hit
            };
            while let Some(q) = Self::find_byte_swar(bytes, start, b'"') {
                out.push_str(&field[start..q]);
                out.push_str("\"\"");
                start = q + 1;
            }
            out.push_str(&field[start..]);
            out.push('"');
            return;
        } else {
            let Some((hit, ch)) = field
                .char_indices()
                .find(|&(_, c)| c == delimiter || c == '"' || c == '\n' || c == '\r')
            else {
                out.push_str(field);
                return;
            };
            (hit, ch == '"')
        };
        out.push('"');
        out.push_str(&field[..hit]);
        let mut start = if hit_is_quote {
            out.push_str("\"\"");
            hit + 1
        } else {
            hit
        };
        for (rel, _) in field[start..].match_indices('"') {
            let q = start + rel;
            out.push_str(&field[start..q]);
            out.push_str("\"\"");
            start = q + 1;
        }
        out.push_str(&field[start..]);
        out.push('"');
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkdownBlock {
    Heading { level: u8, text: String },
    Paragraph(String),
    BlockQuote(String),
    BulletList(Vec<String>),
    NumberedList(Vec<String>),
    Table(Vec<Vec<String>>),
}

pub struct CommonMarkCodec;

/// True for a table separator cell such as `---`, `:--:` (at least three dashes).
fn is_table_separator_cell(cell: &str) -> bool {
    let core = cell.trim().trim_matches(':');
    core.len() >= 3 && core.chars().all(|ch| ch == '-')
}

/// Heading level and text for an ATX heading line (`#` to `######`).
fn atx_heading(trimmed: &str) -> Option<(u8, &str)> {
    [
        ("###### ", 6u8),
        ("##### ", 5),
        ("#### ", 4),
        ("### ", 3),
        ("## ", 2),
        ("# ", 1),
    ]
    .iter()
    .find_map(|(prefix, level)| {
        trimmed
            .strip_prefix(prefix)
            .map(|text| (*level, text.trim()))
    })
}

/// Block accumulator for the line-oriented Markdown parser.
#[derive(Default)]
struct MarkdownAccumulator {
    blocks: Vec<MarkdownBlock>,
    bullets: Vec<String>,
    numbered: Vec<String>,
    table: Vec<Vec<String>>,
    table_seen_sep: bool,
    table_cells: usize,
    budget: DecodeBudget,
}

impl MarkdownAccumulator {
    fn flush_bullets(&mut self) {
        if !self.bullets.is_empty() {
            self.blocks
                .push(MarkdownBlock::BulletList(std::mem::take(&mut self.bullets)));
        }
    }

    fn flush_numbered(&mut self) {
        if !self.numbered.is_empty() {
            self.blocks.push(MarkdownBlock::NumberedList(std::mem::take(
                &mut self.numbered,
            )));
        }
    }

    fn flush_table(&mut self) {
        if !self.table.is_empty() {
            self.blocks
                .push(MarkdownBlock::Table(std::mem::take(&mut self.table)));
        }
        self.table_seen_sep = false;
    }

    fn flush_all(&mut self) {
        self.flush_bullets();
        self.flush_numbered();
        self.flush_table();
    }

    fn feed_line(
        &mut self,
        trimmed: &str,
        parse_row: fn(&str) -> Vec<String>,
    ) -> Result<(), EasyOfficeError> {
        if trimmed.is_empty() {
            self.flush_all();
        } else if trimmed.len() >= 2 && trimmed.starts_with('|') && trimmed.ends_with('|') {
            self.feed_table_row(trimmed, parse_row)?;
        } else if let Some(rest) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            self.flush_numbered();
            self.flush_table();
            let item = rest.trim();
            self.budget.charge_block(item.len(), "Markdown 목록 항목")?;
            self.bullets.push(item.to_string());
        } else if let Some(item) = numbered_item(trimmed) {
            self.flush_bullets();
            self.flush_table();
            self.budget.charge_block(item.len(), "Markdown 번호 항목")?;
            self.numbered.push(item.to_string());
        } else {
            self.flush_all();
            self.budget.charge_block(trimmed.len(), "Markdown 블록")?;
            self.blocks.push(match atx_heading(trimmed) {
                Some((level, text)) => MarkdownBlock::Heading {
                    level,
                    text: text.to_string(),
                },
                None => match trimmed.strip_prefix("> ") {
                    Some(quote) => MarkdownBlock::BlockQuote(quote.trim().to_string()),
                    None => MarkdownBlock::Paragraph(trimmed.to_string()),
                },
            });
        }
        Ok(())
    }

    fn feed_table_row(
        &mut self,
        trimmed: &str,
        parse_row: fn(&str) -> Vec<String>,
    ) -> Result<(), EasyOfficeError> {
        self.flush_bullets();
        self.flush_numbered();
        let inner = trimmed.strip_prefix('|').unwrap_or(trimmed);
        let inner = inner.strip_suffix('|').unwrap_or(inner);
        let cells = parse_row(inner);
        for cell in &cells {
            self.budget.charge_cell(cell.len(), "Markdown 표 셀")?;
        }
        let is_sep = self.table.len() == 1
            && !self.table_seen_sep
            && !cells.is_empty()
            && cells.iter().all(|c| is_table_separator_cell(c));
        if is_sep {
            self.table_seen_sep = true;
        } else {
            self.table_cells = self.table_cells.saturating_add(cells.len());
            if self.table_cells > CsvCodec::MAX_GRID_CELLS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "Markdown 표 셀 수가 상한({}개)을 초과했습니다.",
                    CsvCodec::MAX_GRID_CELLS
                )));
            }
            self.table.push(cells);
        }
        Ok(())
    }
}

/// Item text of a `N. item` line (digits only before the first `. `).
fn numbered_item(trimmed: &str) -> Option<&str> {
    let dot_idx = trimmed.find(". ")?;
    let prefix = &trimmed[..dot_idx];
    (!prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit()))
        .then(|| trimmed[dot_idx + 2..].trim())
}

impl CommonMarkCodec {
    pub(crate) fn parse_markdown_table_row(inner: &str) -> Vec<String> {
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

        let mut acc = MarkdownAccumulator::default();
        for raw_line in markdown.lines() {
            acc.feed_line(raw_line.trim(), Self::parse_markdown_table_row)?;
        }
        acc.flush_all();
        Ok(acc.blocks)
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
                    let col_count = rows.iter().map(Vec::len).max().unwrap_or(0);
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

#[cfg(test)]
#[path = "csv_markdown_codecs/tests.rs"]
mod tests;

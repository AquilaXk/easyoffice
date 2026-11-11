//! One-pass reader of the table grid of an ODS `content.xml`: tables, columns, rows and cells come
//! out of a single event stream and go to a sink, which keeps what it needs (cell text with the
//! budgets, or cell and column styles). Replaces the per-cell substring searches that found each
//! open tag and its matching close tag again.
//!
//! 참조 구현 (reference implementation): calamine 0.38.0 (MIT, Copyright (c) 2016 Johann Tuffe),
//! https://github.com/tafia/calamine, `src/ods.rs` (`read_table`, `read_row`): one
//! pull-reader loop with a state per table, row and cell. The budgets, the repeat rules and the
//! cell text priority are this repository's own, unchanged from the substring scanner. Safe Rust
//! only, no dependency (PD-053, PD-095).

use std::borrow::Cow;

use crate::decode_budget::DecodeBudget;
use crate::odf_hwpx_codecs::{ods_budget_error, OdsMultiUnpacked};
use crate::sheet_styling::ods_styles::{parse_automatic_styles, StyleSink};
use crate::sheet_styling::SheetStyling;
use crate::xml_stream::{unescape_cow, Attrs, XmlEvent, XmlReader};
use crate::{EasyOfficeError, OdfCodec, XmlSanitizer};

/// Spaces one `text:s` element may stand for.
const MAX_TEXT_SPACES: usize = 4096;

/// One `table:table-cell` or `table:covered-table-cell` of a row.
pub(crate) struct OdsCell<'a> {
    pub(crate) covered: bool,
    /// `table:number-columns-repeated`, at least 1.
    pub(crate) col_repeat: u32,
    pub(crate) explicit_row: Option<u32>,
    pub(crate) explicit_col: Option<u32>,
    /// The cell text by priority (formula, paragraphs, value); only read when text is wanted.
    pub(crate) text: Option<String>,
    /// `table:style-name`, as written.
    pub(crate) style: Option<&'a str>,
}

/// Passes the grid to two sinks, the first one's error ends the walk.
impl<A: OdsSink, B: OdsSink> OdsSink for (A, B) {
    fn table(&mut self, attrs: &Attrs<'_>) {
        self.0.table(attrs);
        self.1.table(attrs);
    }
    fn column(&mut self, attrs: &Attrs<'_>) {
        self.0.column(attrs);
        self.1.column(attrs);
    }
    fn row(&mut self, repeat: u32, cells: &[OdsCell<'_>]) -> Result<(), EasyOfficeError> {
        self.0.row(repeat, cells)?;
        self.1.row(repeat, cells)
    }
    fn table_end(&mut self) {
        self.0.table_end();
        self.1.table_end();
    }
}

/// Receives the grid in document order.
pub(crate) trait OdsSink {
    fn table(&mut self, attrs: &Attrs<'_>);
    /// A `table:table-column` before the first row of the table.
    fn column(&mut self, _attrs: &Attrs<'_>) {}
    fn row(&mut self, repeat: u32, cells: &[OdsCell<'_>]) -> Result<(), EasyOfficeError>;
    fn table_end(&mut self);
}

/// `table:number-rows-repeated` and the like: at least 1, 1 when absent or unreadable.
pub(crate) fn repeat_value(value: &str) -> u32 {
    value.parse::<u32>().unwrap_or(1).max(1)
}

fn plain_name(name: &str) -> &str {
    name.strip_prefix("eo:").unwrap_or(name)
}

/// The text fields of the cell being read.
#[derive(Default)]
struct CellFields<'a> {
    formula: Option<Cow<'a, str>>,
    boolean: Option<&'a str>,
    value: Option<Cow<'a, str>>,
    string_value: Option<Cow<'a, str>>,
    date_value: Option<Cow<'a, str>>,
    paragraphs: String,
    paragraph_count: usize,
}

struct CellBuilder<'a> {
    cell: OdsCell<'a>,
    fields: CellFields<'a>,
    depth: usize,
    paragraph_depth: usize,
    /// The `text:p` just started was `<text:p/>`: its end event is not a paragraph end.
    empty_paragraph: bool,
}

impl<'a> CellBuilder<'a> {
    fn open(attrs: &Attrs<'a>, covered: bool, want_text: bool) -> Self {
        let mut cell = OdsCell {
            covered,
            col_repeat: 1,
            explicit_row: None,
            explicit_col: None,
            text: None,
            style: None,
        };
        let mut fields = CellFields::default();
        for (name, value) in attrs.iter_raw() {
            match (plain_name(name), want_text) {
                ("table:number-columns-repeated", _) => cell.col_repeat = repeat_value(value),
                ("row", _) if cell.explicit_row.is_none() => cell.explicit_row = value.parse().ok(),
                ("col", _) if cell.explicit_col.is_none() => cell.explicit_col = value.parse().ok(),
                ("table:style-name", _) if cell.style.is_none() => cell.style = Some(value),
                ("table:formula", true) => {
                    fields.formula.get_or_insert_with(|| unescape_cow(value));
                }
                ("office:boolean-value", true) => {
                    fields.boolean.get_or_insert(value);
                }
                ("office:value", true) => {
                    fields.value.get_or_insert_with(|| unescape_cow(value));
                }
                ("office:string-value", true) => {
                    fields
                        .string_value
                        .get_or_insert_with(|| unescape_cow(value));
                }
                ("office:date-value", true) => {
                    fields.date_value.get_or_insert_with(|| unescape_cow(value));
                }
                _ => {}
            }
        }
        Self {
            cell,
            fields,
            depth: 1,
            paragraph_depth: 0,
            empty_paragraph: false,
        }
    }

    /// Feeds one event inside the cell; true when the cell element has ended.
    fn feed(&mut self, event: &XmlEvent<'a>, empty_start: bool) -> bool {
        match event {
            XmlEvent::Start { name, attrs } => {
                self.depth += 1;
                self.start(name, attrs, empty_start);
            }
            XmlEvent::End(name) => {
                self.depth -= 1;
                if *name == "text:p" {
                    if self.empty_paragraph {
                        self.empty_paragraph = false;
                    } else {
                        self.paragraph_depth = self.paragraph_depth.saturating_sub(1);
                    }
                }
            }
            XmlEvent::Text(text) if self.paragraph_depth > 0 => {
                self.fields.paragraphs.push_str(text);
            }
            XmlEvent::Text(_) => {}
        }
        self.depth == 0
    }

    fn start(&mut self, name: &str, attrs: &Attrs<'_>, empty_start: bool) {
        match name {
            "text:p" if empty_start => self.empty_paragraph = true,
            "text:p" => {
                if self.paragraph_depth == 0 {
                    if self.fields.paragraph_count > 0 {
                        self.fields.paragraphs.push('\n');
                    }
                    self.fields.paragraph_count += 1;
                }
                self.paragraph_depth += 1;
            }
            _ if self.paragraph_depth == 0 => {}
            "text:s" => {
                let count = attrs
                    .iter_raw()
                    .find(|(n, _)| matches!(*n, "text:c" | "c"))
                    .and_then(|(_, v)| unescape_cow(v).trim().parse::<usize>().ok())
                    .unwrap_or(1)
                    .min(MAX_TEXT_SPACES);
                self.fields
                    .paragraphs
                    .extend(std::iter::repeat_n(' ', count));
            }
            "text:tab" => self.fields.paragraphs.push('\t'),
            "text:line-break" => self.fields.paragraphs.push('\n'),
            _ => {}
        }
    }

    fn finish(mut self, want_text: bool) -> OdsCell<'a> {
        if want_text && !self.cell.covered {
            self.cell.text = self.resolve_text();
        }
        self.cell
    }

    /// Cell text by priority: formula, paragraph text, office value/boolean/date, then empty for
    /// explicit cells.
    fn resolve_text(&mut self) -> Option<String> {
        let f = &mut self.fields;
        if let Some(raw) = f.formula.take() {
            return Some(OdfCodec::normalize_ods_formula(&raw));
        }
        let has_paragraph = f.paragraph_count > 0;
        if has_paragraph && !f.paragraphs.is_empty() {
            return Some(std::mem::take(&mut f.paragraphs));
        }
        if let Some(flag) = f.boolean {
            let flag = flag.trim();
            let truthy = flag.eq_ignore_ascii_case("true") || flag == "1";
            return Some(if truthy { "TRUE" } else { "FALSE" }.to_string());
        }
        let explicit = self.cell.explicit_row.is_some() || self.cell.explicit_col.is_some();
        f.value
            .take()
            .or_else(|| f.string_value.take())
            .or_else(|| f.date_value.take())
            .map(Cow::into_owned)
            .or_else(|| (has_paragraph || explicit).then(String::new))
    }
}

/// Walks every top-level table of `xml`, handing tables, columns and rows to `sink`.
pub(crate) fn walk_ods<S: OdsSink>(
    xml: &str,
    want_text: bool,
    sink: &mut S,
) -> Result<(), EasyOfficeError> {
    let mut reader = XmlReader::unbounded(xml);
    let mut table_nesting = 0usize;
    let mut seen_row = false;
    let mut row_repeat: Option<u32> = None;
    let mut cells: Vec<OdsCell<'_>> = Vec::new();
    let mut cell: Option<CellBuilder<'_>> = None;
    while let Some(event) = reader.next_event()? {
        if let Some(open) = cell.as_mut() {
            if open.feed(&event, reader.last_start_was_empty()) {
                if let Some(done) = cell.take() {
                    cells.push(done.finish(want_text));
                }
            }
            continue;
        }
        match event {
            XmlEvent::Start {
                name: "table:table",
                ref attrs,
            } => {
                if table_nesting == 0 {
                    seen_row = false;
                    sink.table(attrs);
                }
                table_nesting += 1;
            }
            XmlEvent::End("table:table") if table_nesting > 0 => {
                table_nesting -= 1;
                if table_nesting == 0 {
                    sink.table_end();
                }
            }
            _ if table_nesting == 0 => {}
            XmlEvent::Start {
                name: "table:table-column",
                ref attrs,
            } if !seen_row && row_repeat.is_none() => {
                sink.column(attrs);
            }
            XmlEvent::Start {
                name: "table:table-row",
                ref attrs,
            } if row_repeat.is_none() => {
                seen_row = true;
                cells.clear();
                row_repeat = Some(
                    attrs
                        .iter_raw()
                        .find(|(n, _)| *n == "table:number-rows-repeated")
                        .map_or(1, |(_, v)| repeat_value(v)),
                );
            }
            XmlEvent::End("table:table-row") => {
                if let Some(repeat) = row_repeat.take() {
                    sink.row(repeat, &cells)?;
                }
            }
            XmlEvent::Start {
                name: name @ ("table:table-cell" | "table:covered-table-cell"),
                ref attrs,
            } if row_repeat.is_some() => {
                cell = Some(CellBuilder::open(
                    attrs,
                    name == "table:covered-table-cell",
                    want_text,
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

/// The sheet name of a `table:table` for the text reader: `table:name`, else `name`.
pub(crate) fn table_name(attrs: &Attrs<'_>) -> Option<String> {
    let find = |key: &str| {
        attrs
            .iter_raw()
            .find(|(n, _)| plain_name(n) == key)
            .map(|(_, v)| XmlSanitizer::unescape(v))
    };
    find("table:name").or_else(|| find("name"))
}

// ---- ODS cell text within the row, column and cell budgets ----

struct TextSink {
    sheets: OdsMultiUnpacked,
    name: String,
    cells: Vec<(u32, u32, String)>,
    current_row: u32,
    total_cells: usize,
    /// Decoded bytes of the cell texts copied into `cells`, over all tables of the document.
    budget: DecodeBudget,
}

impl TextSink {
    /// Expands one row (with column repeats) at `target_row` into the cells of the table.
    fn emit_row(
        &mut self,
        cells: &[OdsCell<'_>],
        last_populated: usize,
        target_row: u32,
    ) -> Result<(), EasyOfficeError> {
        let mut current_col: u32 = 0;
        for (index, cell) in cells.iter().enumerate() {
            let next_col = current_col
                .checked_add(cell.col_repeat)
                .ok_or_else(|| ods_budget_error("ODS 열 반복 횟수가 허용 범위를 초과했습니다."))?;
            if index <= last_populated && next_col > OdfCodec::MAX_COLS + 1 {
                return Err(ods_budget_error(
                    "ODS 열 인덱스가 허용 범위를 초과했습니다.",
                ));
            }
            if let Some(text) = &cell.text {
                for offset in 0..cell.col_repeat {
                    let r = cell.explicit_row.unwrap_or(target_row);
                    let c = cell
                        .explicit_col
                        .map(|ec| ec.saturating_add(offset))
                        .unwrap_or(current_col + offset);
                    if r > OdfCodec::MAX_ROWS || c > OdfCodec::MAX_COLS {
                        return Err(ods_budget_error("ODS 셀 좌표가 허용 범위를 초과했습니다."));
                    }
                    self.total_cells += 1;
                    if self.total_cells > OdfCodec::MAX_CELLS {
                        return Err(ods_budget_error(
                            "ODS 셀 개수가 허용 예산(200,000)을 초과했습니다.",
                        ));
                    }
                    self.budget.charge_cell(text.len(), "ODS 셀 텍스트")?;
                    self.cells.push((r, c, text.clone()));
                }
            }
            current_col = next_col;
        }
        Ok(())
    }
}

impl OdsSink for TextSink {
    fn table(&mut self, attrs: &Attrs<'_>) {
        self.name = table_name(attrs).unwrap_or_else(|| format!("Sheet{}", self.sheets.len() + 1));
        self.cells = Vec::new();
        self.current_row = 0;
    }

    fn row(&mut self, repeat: u32, cells: &[OdsCell<'_>]) -> Result<(), EasyOfficeError> {
        if repeat > OdfCodec::MAX_ROWS + 1 {
            return Err(ods_budget_error(
                "ODS 행 반복 횟수가 허용 범위를 초과했습니다.",
            ));
        }
        let mut last_populated = None;
        for (index, cell) in cells.iter().enumerate() {
            if cell.col_repeat > OdfCodec::MAX_COLS + 1 {
                return Err(ods_budget_error(
                    "ODS 열 반복 횟수가 허용 범위를 초과했습니다.",
                ));
            }
            if cell.text.is_some() {
                last_populated = Some(index);
            }
        }
        let Some(last_populated) = last_populated else {
            self.current_row = self.current_row.saturating_add(repeat);
            return Ok(());
        };
        let next_row = self
            .current_row
            .checked_add(repeat)
            .ok_or_else(|| ods_budget_error("ODS 행 반복 횟수가 허용 범위를 초과했습니다."))?;
        if next_row > OdfCodec::MAX_ROWS + 1 {
            return Err(ods_budget_error(
                "ODS 행 인덱스가 허용 범위를 초과했습니다.",
            ));
        }
        for offset in 0..repeat {
            let target_row = self.current_row + offset;
            if target_row > OdfCodec::MAX_ROWS {
                return Err(ods_budget_error("ODS 행 좌표가 허용 범위를 초과했습니다."));
            }
            self.emit_row(cells, last_populated, target_row)?;
        }
        self.current_row = next_row;
        Ok(())
    }

    fn table_end(&mut self) {
        let name = std::mem::take(&mut self.name);
        self.sheets.push((name, std::mem::take(&mut self.cells)));
    }
}

impl TextSink {
    fn new() -> Self {
        Self {
            sheets: Vec::new(),
            name: String::new(),
            cells: Vec::new(),
            current_row: 0,
            total_cells: 0,
            budget: DecodeBudget::new(),
        }
    }

    /// The sheets read; a document without a table has one empty `Sheet1`.
    fn into_sheets(self) -> OdsMultiUnpacked {
        if self.sheets.is_empty() {
            return vec![("Sheet1".to_string(), Vec::new())];
        }
        self.sheets
    }
}

/// Reads every table of `content_xml`.
pub(crate) fn read_ods_sheets(content_xml: &str) -> Result<OdsMultiUnpacked, EasyOfficeError> {
    let mut sink = TextSink::new();
    walk_ods(content_xml, true, &mut sink)?;
    Ok(sink.into_sheets())
}

/// The tables and their styling from one walk of `content_xml`.
pub(crate) fn read_ods_sheets_with_styling(
    content_xml: &str,
) -> Result<(OdsMultiUnpacked, Vec<(String, SheetStyling)>), EasyOfficeError> {
    let styles = parse_automatic_styles(content_xml);
    let mut sinks = (TextSink::new(), StyleSink::new(&styles));
    walk_ods(content_xml, true, &mut sinks)?;
    Ok((sinks.0.into_sheets(), sinks.1.into_tables()))
}

#[cfg(test)]
#[path = "ods_grid/tests.rs"]
mod tests;

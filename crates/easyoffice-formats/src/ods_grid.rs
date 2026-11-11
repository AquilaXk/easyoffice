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
    /// `table:style-name`; only read when text is not wanted.
    pub(crate) style: Option<&'a str>,
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
                ("table:style-name", false) if cell.style.is_none() => cell.style = Some(value),
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

#[cfg(test)]
mod tests;

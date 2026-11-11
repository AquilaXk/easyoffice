//! OpenDocument spreadsheet cell and column styles (PD-089): automatic styles written into
//! `content.xml` and read back (`style:text-properties`, `style:column-width`).

use std::collections::{BTreeMap, HashMap};

use crate::ods_grid::{repeat_value, walk_ods, OdsCell, OdsSink};
use crate::rtf_xml_helpers::{
    extract_attr_from_fragment, extract_tags_or_self_closing_with_attrs, extract_tags_with_attrs,
};
use crate::sheet_styling::{
    format_centipt, CellStyle, SheetStyling, StyleKey, MAX_COLUMN_PX, MAX_STYLED_CELLS,
    MIN_COLUMN_PX,
};
use crate::xml_stream::{unescape_cow, Attrs};
use crate::{EasyOfficeError, OdfCodec, XmlSanitizer};

const PX_PER_INCH: f64 = 96.0;
const CM_PER_INCH: f64 = 2.54;
const MM_PER_INCH: f64 = 25.4;
const PT_PER_INCH: f64 = 72.0;

/// Automatic style names for a workbook: `ce<N>` per distinct cell style and `co<N>` per distinct
/// column width, in first-use order.
pub(crate) struct OdsStyleTable {
    cells: Vec<StyleKey>,
    cell_index: HashMap<StyleKey, usize>,
    widths: Vec<u32>,
    width_index: HashMap<u32, usize>,
}

impl OdsStyleTable {
    pub(crate) fn build(stylings: &[SheetStyling]) -> Self {
        let mut table = Self {
            cells: Vec::new(),
            cell_index: HashMap::new(),
            widths: Vec::new(),
            width_index: HashMap::new(),
        };
        for styling in stylings {
            for style in styling.cells.values().filter(|s| !s.is_plain()) {
                let key = style.key();
                table.cell_index.entry(key).or_insert_with(|| {
                    table.cells.push(key);
                    table.cells.len()
                });
            }
            for &px in styling.columns.values() {
                table.width_index.entry(px).or_insert_with(|| {
                    table.widths.push(px);
                    table.widths.len()
                });
            }
        }
        table
    }

    pub(crate) fn cell_style_name(&self, style: &CellStyle) -> Option<String> {
        if style.is_plain() {
            return None;
        }
        self.cell_index.get(&style.key()).map(|i| format!("ce{i}"))
    }

    pub(crate) fn column_style_name(&self, px: u32) -> Option<String> {
        self.width_index.get(&px).map(|i| format!("co{i}"))
    }

    /// `<office:automatic-styles>` element (empty string when there is nothing to declare).
    pub(crate) fn automatic_styles_xml(&self) -> String {
        if self.cells.is_empty() && self.widths.is_empty() {
            return String::new();
        }
        let mut xml = String::from("<office:automatic-styles>");
        for (i, &px) in self.widths.iter().enumerate() {
            xml.push_str(&format!(
                "<style:style style:name=\"co{}\" style:family=\"table-column\"><style:table-column-properties style:column-width=\"{:.4}in\"/></style:style>",
                i + 1,
                f64::from(px) / PX_PER_INCH
            ));
        }
        for (i, &k) in self.cells.iter().enumerate() {
            let mut props = String::new();
            if k.bold() {
                props.push_str(" fo:font-weight=\"bold\"");
            }
            if k.italic() {
                props.push_str(" fo:font-style=\"italic\"");
            }
            if k.underline() {
                props.push_str(" style:text-underline-style=\"solid\" style:text-underline-width=\"auto\" style:text-underline-color=\"font-color\"");
            }
            if k.strike() {
                props.push_str(" style:text-line-through-style=\"solid\"");
            }
            if let Some(centi) = k.size_centipt() {
                props.push_str(&format!(" fo:font-size=\"{}pt\"", format_centipt(centi)));
            }
            xml.push_str(&format!(
                "<style:style style:name=\"ce{}\" style:family=\"table-cell\"><style:text-properties{props}/></style:style>",
                i + 1
            ));
        }
        xml.push_str("</office:automatic-styles>");
        xml
    }
}

/// `<table:table-column>` elements for the custom widths (empty string when there are none);
/// columns without a width are filled with unstyled repeated columns.
pub(crate) fn table_columns_xml(columns: &BTreeMap<u32, u32>, table: &OdsStyleTable) -> String {
    let mut xml = String::new();
    let mut next = 0u32;
    let mut runs: Vec<(u32, u32, u32)> = Vec::new();
    for (&col, &px) in columns {
        match runs.last_mut() {
            Some((_, end, w)) if *w == px && *end + 1 == col => *end = col,
            _ => runs.push((col, col, px)),
        }
    }
    for (start, end, px) in runs {
        if start > next {
            xml.push_str(&repeated_column(None, start - next));
        }
        xml.push_str(&repeated_column(
            table.column_style_name(px).as_deref(),
            end - start + 1,
        ));
        next = end + 1;
    }
    xml
}

fn repeated_column(style: Option<&str>, count: u32) -> String {
    let style = style.map_or(String::new(), |s| {
        format!(" table:style-name=\"{}\"", XmlSanitizer::escape(s))
    });
    let repeat = if count > 1 {
        format!(" table:number-columns-repeated=\"{count}\"")
    } else {
        String::new()
    };
    format!("<table:table-column{style}{repeat}/>")
}

fn length_to_px(value: &str) -> Option<f64> {
    let value = value.trim();
    let split = value.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))?;
    let number: f64 = value[..split].parse().ok()?;
    let per_inch = match value[split..].trim() {
        "in" => 1.0,
        "cm" => CM_PER_INCH,
        "mm" => MM_PER_INCH,
        "pt" => PT_PER_INCH,
        "px" => PX_PER_INCH,
        _ => return None,
    };
    Some(number / per_inch * PX_PER_INCH).filter(|px| px.is_finite())
}

fn parse_text_properties(attrs: &str) -> CellStyle {
    let get = |name: &str| extract_attr_from_fragment(attrs, name);
    let on = |name: &str| get(name).is_some_and(|v| v != "none");
    CellStyle {
        bold: get("fo:font-weight")
            .is_some_and(|w| w == "bold" || w.parse::<u32>().is_ok_and(|n| n >= 600)),
        italic: get("fo:font-style").is_some_and(|s| s == "italic" || s == "oblique"),
        underline: on("style:text-underline-style"),
        strike: on("style:text-line-through-style"),
        size_pt: get("fo:font-size")
            .and_then(length_to_px)
            .map(|px| (px / PX_PER_INCH * PT_PER_INCH * 100.0).round() / 100.0)
            .filter(|pt| *pt >= 1.0),
    }
}

/// Cell styles and column widths declared by the automatic styles of `content.xml`.
pub(crate) struct AutomaticStyles {
    pub(crate) cells: HashMap<String, CellStyle>,
    pub(crate) columns: HashMap<String, u32>,
}

pub(crate) fn parse_automatic_styles(content_xml: &str) -> AutomaticStyles {
    let mut styles = AutomaticStyles {
        cells: HashMap::new(),
        columns: HashMap::new(),
    };
    let Some((_, auto)) = extract_tags_with_attrs(content_xml, "office:automatic-styles")
        .into_iter()
        .next()
    else {
        return styles;
    };
    for (attrs, body) in extract_tags_with_attrs(auto, "style:style") {
        let (Some(name), Some(family)) = (
            extract_attr_from_fragment(attrs, "style:name"),
            extract_attr_from_fragment(attrs, "style:family"),
        ) else {
            continue;
        };
        let name = XmlSanitizer::unescape(name);
        if family == "table-cell" {
            if let Some((props, _)) =
                extract_tags_or_self_closing_with_attrs(body, "style:text-properties")
                    .into_iter()
                    .next()
            {
                styles.cells.insert(name, parse_text_properties(props));
            }
        } else if family == "table-column" {
            let px = extract_tags_or_self_closing_with_attrs(body, "style:table-column-properties")
                .into_iter()
                .next()
                .and_then(|(props, _)| extract_attr_from_fragment(props, "style:column-width"))
                .and_then(length_to_px);
            if let Some(px) = px {
                let px = px
                    .round()
                    .clamp(f64::from(MIN_COLUMN_PX), f64::from(MAX_COLUMN_PX));
                styles.columns.insert(name, px as u32);
            }
        }
    }
    styles
}

/// Collects cell and column styles of each table while the grid is walked.
pub(crate) struct StyleSink<'s> {
    styles: &'s AutomaticStyles,
    tables: Vec<(String, SheetStyling)>,
    columns: BTreeMap<u32, u32>,
    cells: BTreeMap<(u32, u32), CellStyle>,
    name: String,
    row: u32,
    column: u32,
}

impl<'s> StyleSink<'s> {
    pub(crate) fn new(styles: &'s AutomaticStyles) -> Self {
        Self {
            styles,
            tables: Vec::new(),
            columns: BTreeMap::new(),
            cells: BTreeMap::new(),
            name: String::new(),
            row: 0,
            column: 0,
        }
    }

    pub(crate) fn into_tables(self) -> Vec<(String, SheetStyling)> {
        self.tables
    }
}

impl OdsSink for StyleSink<'_> {
    fn table(&mut self, attrs: &Attrs<'_>) {
        self.name = attrs
            .get("table:name")
            .map(|n| n.into_owned())
            .unwrap_or_else(|| format!("Sheet{}", self.tables.len() + 1));
        self.columns = BTreeMap::new();
        self.cells = BTreeMap::new();
        self.row = 0;
        self.column = 0;
    }

    fn column(&mut self, attrs: &Attrs<'_>) {
        let repeat = attrs
            .get("table:number-columns-repeated")
            .map_or(1, |v| repeat_value(&v));
        let px = attrs
            .get("table:style-name")
            .and_then(|n| self.styles.columns.get(&*n).copied());
        if let Some(px) = px {
            for c in self.column
                ..self
                    .column
                    .saturating_add(repeat)
                    .min(OdfCodec::MAX_COLS + 1)
            {
                if self.columns.len() >= MAX_STYLED_CELLS {
                    break;
                }
                self.columns.insert(c, px);
            }
        }
        self.column = self.column.saturating_add(repeat);
    }

    fn row(&mut self, repeat: u32, cells: &[OdsCell<'_>]) -> Result<(), EasyOfficeError> {
        let mut col = 0u32;
        for cell in cells {
            let style = cell
                .style
                .and_then(|n| self.styles.cells.get(&*unescape_cow(n)))
                .filter(|s| !s.is_plain());
            if let Some(style) = style {
                // A style repeated across a whole row/column band (a "format the sheet" fill) is
                // not individual cell formatting; only bounded blocks are kept.
                let block = u64::from(repeat) * u64::from(cell.col_repeat);
                let in_grid = u64::from(self.row) + u64::from(repeat)
                    <= u64::from(OdfCodec::MAX_ROWS) + 1
                    && u64::from(col) + u64::from(cell.col_repeat)
                        <= u64::from(OdfCodec::MAX_COLS) + 1;
                if in_grid && block <= 1024 && self.cells.len() + block as usize <= MAX_STYLED_CELLS
                {
                    for r in self.row..self.row + repeat {
                        for c in col..col + cell.col_repeat {
                            self.cells.insert((r, c), *style);
                        }
                    }
                }
            }
            col = col.saturating_add(cell.col_repeat);
        }
        self.row = self.row.saturating_add(repeat);
        Ok(())
    }

    fn table_end(&mut self) {
        let styling = SheetStyling {
            cells: std::mem::take(&mut self.cells),
            columns: std::mem::take(&mut self.columns),
        };
        self.tables.push((std::mem::take(&mut self.name), styling));
    }
}

/// Styling of every table of an ODS `content.xml`, keyed by table name.
pub(crate) fn parse_ods_styling(
    content_xml: &str,
) -> Result<Vec<(String, SheetStyling)>, EasyOfficeError> {
    let styles = parse_automatic_styles(content_xml);
    let mut sink = StyleSink::new(&styles);
    walk_ods(content_xml, false, &mut sink)?;
    Ok(sink.into_tables())
}

//! OpenDocument spreadsheet cell and column styles (PD-089): automatic styles written into
//! `content.xml` and read back (`style:text-properties`, `style:column-width`).

use std::collections::{BTreeMap, HashMap};

use crate::rtf_xml_helpers::{
    extract_attr_from_fragment, extract_balanced_elements, extract_ods_cells_from_row,
    extract_tags_or_self_closing_with_attrs, extract_tags_with_attrs,
};
use crate::sheet_styling::{
    format_centipt, CellStyle, SheetStyling, StyleKey, MAX_COLUMN_PX, MAX_STYLED_CELLS,
    MIN_COLUMN_PX,
};
use crate::{OdfCodec, XmlSanitizer};

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
        for (i, &(bold, italic, underline, strike, size)) in self.cells.iter().enumerate() {
            let mut props = String::new();
            if bold {
                props.push_str(" fo:font-weight=\"bold\"");
            }
            if italic {
                props.push_str(" fo:font-style=\"italic\"");
            }
            if underline {
                props.push_str(" style:text-underline-style=\"solid\" style:text-underline-width=\"auto\" style:text-underline-color=\"font-color\"");
            }
            if strike {
                props.push_str(" style:text-line-through-style=\"solid\"");
            }
            if let Some(centi) = size {
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
            .and_then(|s| length_to_px(&s))
            .map(|px| (px / PX_PER_INCH * PT_PER_INCH * 100.0).round() / 100.0)
            .filter(|pt| *pt >= 1.0),
    }
}

/// Cell styles and column widths declared by the automatic styles of `content.xml`.
struct AutomaticStyles {
    cells: HashMap<String, CellStyle>,
    columns: HashMap<String, u32>,
}

fn parse_automatic_styles(content_xml: &str) -> AutomaticStyles {
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
        let name = XmlSanitizer::unescape(&name);
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
                .and_then(|w| length_to_px(&w));
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

fn repeat_of(attrs: &str, name: &str) -> u32 {
    extract_attr_from_fragment(attrs, name)
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(1)
        .max(1)
}

fn parse_table_columns(table_xml: &str, styles: &AutomaticStyles) -> BTreeMap<u32, u32> {
    let head = &table_xml[..table_xml
        .find("<table:table-row")
        .unwrap_or(table_xml.len())];
    let mut columns = BTreeMap::new();
    let mut col = 0u32;
    for (attrs, _) in extract_balanced_elements(head, "table:table-column") {
        let repeat = repeat_of(attrs, "table:number-columns-repeated");
        let px = extract_attr_from_fragment(attrs, "table:style-name")
            .and_then(|n| styles.columns.get(&XmlSanitizer::unescape(&n)).copied());
        if let Some(px) = px {
            for c in col..col.saturating_add(repeat).min(OdfCodec::MAX_COLS + 1) {
                if columns.len() >= MAX_STYLED_CELLS {
                    break;
                }
                columns.insert(c, px);
            }
        }
        col = col.saturating_add(repeat);
    }
    columns
}

fn parse_table_cells(table_xml: &str, styles: &AutomaticStyles) -> BTreeMap<(u32, u32), CellStyle> {
    let mut cells = BTreeMap::new();
    let mut row = 0u32;
    for (row_attrs, row_inner) in extract_balanced_elements(table_xml, "table:table-row") {
        let row_repeat = repeat_of(row_attrs, "table:number-rows-repeated");
        let mut col = 0u32;
        for (_, attrs, _) in extract_ods_cells_from_row(row_inner) {
            let col_repeat = repeat_of(attrs, "table:number-columns-repeated");
            let style = extract_attr_from_fragment(attrs, "table:style-name")
                .and_then(|n| styles.cells.get(&XmlSanitizer::unescape(&n)))
                .filter(|s| !s.is_plain());
            if let Some(style) = style {
                // A style repeated across a whole row/column band (a "format the sheet" fill) is
                // not individual cell formatting; only bounded blocks are kept.
                let block = u64::from(row_repeat) * u64::from(col_repeat);
                let in_grid = u64::from(row) + u64::from(row_repeat)
                    <= u64::from(OdfCodec::MAX_ROWS) + 1
                    && u64::from(col) + u64::from(col_repeat) <= u64::from(OdfCodec::MAX_COLS) + 1;
                if in_grid && block <= 1024 && cells.len() + block as usize <= MAX_STYLED_CELLS {
                    for r in row..row + row_repeat {
                        for c in col..col + col_repeat {
                            cells.insert((r, c), *style);
                        }
                    }
                }
            }
            col = col.saturating_add(col_repeat);
        }
        row = row.saturating_add(row_repeat);
    }
    cells
}

/// Styling of every table of an ODS `content.xml`, keyed by table name.
pub(crate) fn parse_ods_styling(content_xml: &str) -> Vec<(String, SheetStyling)> {
    let styles = parse_automatic_styles(content_xml);
    extract_balanced_elements(content_xml, "table:table")
        .into_iter()
        .enumerate()
        .map(|(idx, (attrs, inner))| {
            let name = extract_attr_from_fragment(attrs, "table:name")
                .map(|s| XmlSanitizer::unescape(&s))
                .unwrap_or_else(|| format!("Sheet{}", idx + 1));
            let styling = SheetStyling {
                cells: parse_table_cells(inner, &styles),
                columns: parse_table_columns(inner, &styles),
            };
            (name, styling)
        })
        .collect()
}

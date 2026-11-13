//! OpenDocument spreadsheet cell and column styles (PD-089): automatic styles written into
//! `content.xml` and read back (`style:text-properties`, `style:column-width`).

use std::collections::{BTreeMap, HashMap};

use crate::rtf_xml_helpers::{
    extract_attr_from_fragment, extract_balanced_elements, extract_tags_or_self_closing_with_attrs,
    extract_tags_with_attrs,
};
use crate::sheet_styling::ods_grid::{repeat_value, walk_ods, OdsCell, OdsSink};
use crate::sheet_styling::ods_numfmt::{data_style_xml, read_data_styles};
use crate::sheet_styling::ods_view::{table_props, GroupWriter, TableProps};
use crate::sheet_styling::sheet_links::ods_links::href_to_target;
use crate::sheet_styling::sheet_links::{LinkAllowance, LinkCollector};
use crate::sheet_styling::sheet_look::{ods_border_edge, ods_border_value};
use crate::sheet_styling::sheet_rules::ods_rules_read::{OdsRuleReader, ValidationSink};
use crate::sheet_styling::sheet_view::{push_level, Outline, MAX_OUTLINE_LEVEL, MAX_OUTLINE_ROWS};
use crate::sheet_styling::{
    format_centipt, resolve_code, BorderLine, CellBorders, CellStyle, HAlign, IdMemo, NumFmtTable,
    SheetStyling, StyleKey, VAlign, MAX_COLUMN_PX, MAX_STYLED_CELLS, MIN_COLUMN_PX,
};
use crate::xml_stream::{unescape_cow, Attrs};
use crate::{CellBox, EasyOfficeError, OdfCodec, XmlSanitizer};
use std::sync::Arc;

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
    /// Custom number format codes of the whole workbook (PD-193); `eonf<id>` data styles.
    num_fmts: NumFmtTable,
    /// Workbook-wide number format ids in first-use order, each written as one data style.
    data_ids: Vec<u16>,
}

impl OdsStyleTable {
    pub(crate) fn build(stylings: &[SheetStyling]) -> Self {
        let mut table = Self {
            cells: Vec::new(),
            cell_index: HashMap::new(),
            widths: Vec::new(),
            width_index: HashMap::new(),
            num_fmts: NumFmtTable::default(),
            data_ids: Vec::new(),
        };
        for styling in stylings {
            let mut ids = IdMemo::new();
            let unlocked = (!styling.unlocked_cols.is_empty()).then(unlocked_style);
            for style in styling
                .cells
                .values()
                .copied()
                .chain(unlocked)
                .filter(|s| !s.is_plain())
            {
                let id = ids.get(style.num_fmt, styling, |code| table.num_fmts.intern(code));
                let key = style.key().with_num_fmt(id);
                if key == StyleKey::default() || table.cell_index.contains_key(&key) {
                    continue;
                }
                if id != 0 && !table.data_ids.contains(&id) {
                    table.data_ids.push(id);
                }
                table.cells.push(key);
                table.cell_index.insert(key, table.cells.len());
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

    pub(crate) fn cell_style_name(
        &self,
        style: &CellStyle,
        styling: &SheetStyling,
        ids: &mut IdMemo,
    ) -> Option<String> {
        if style.is_plain() {
            return None;
        }
        let id = ids.get(style.num_fmt, styling, |code| {
            self.num_fmts.lookup(code).unwrap_or(0)
        });
        self.cell_index
            .get(&style.key().with_num_fmt(id))
            .map(|i| format!("ce{i}"))
    }

    /// The name of the cell style of an unlocked column (PD-216).
    pub(crate) fn unlocked_name(&self) -> Option<String> {
        self.cell_index
            .get(&unlocked_style().key())
            .map(|i| format!("ce{i}"))
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
        for &id in &self.data_ids {
            if let Some(code) = resolve_code(id, self.num_fmts.codes()) {
                xml.push_str(&data_style_xml(&format!("eonf{id}"), code));
            }
        }
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
            let data = match k.num_fmt() {
                0 => String::new(),
                id => format!(" style:data-style-name=\"eonf{id}\""),
            };
            xml.push_str(&format!(
                "<style:style style:name=\"ce{}\" style:family=\"table-cell\"{data}>{}<style:text-properties{props}/></style:style>",
                i + 1,
                cell_look_xml(k)
            ));
        }
        xml.push_str("</office:automatic-styles>");
        xml
    }
}

/// The format of an unlocked column: the default look with the lock off (PD-216).
fn unlocked_style() -> CellStyle {
    CellStyle {
        unlocked: true,
        ..CellStyle::default()
    }
}

/// `style:cell-protect` of a style: `none` for an unlocked cell, `protected` for one kept locked inside an
/// unlocked column, and the formula-hidden token for a hidden formula (ODF 1.2 20.211); the default is
/// `protected`, so a plain locked cell says nothing.
fn cell_protect(k: StyleKey) -> Option<&'static str> {
    match (k.unlocked(), k.force_locked(), k.formula_hidden()) {
        (true, _, false) => Some("none"),
        (true, _, true) => Some("formula-hidden"),
        (false, true, false) => Some("protected"),
        (false, _, true) => Some("protected formula-hidden"),
        (false, false, false) => None,
    }
}

/// The lock state a `style:cell-protect` value says (the attribute absent is `protected`).
fn parse_cell_protect(value: &str, style: &mut CellStyle) {
    let tokens: Vec<&str> = value.split_whitespace().collect();
    let has = |t: &str| tokens.contains(&t);
    let hidden_all = has("hidden-and-protected");
    style.formula_hidden = has("formula-hidden") || hidden_all;
    let locked = has("protected") || hidden_all;
    style.unlocked = !locked;
    style.force_locked = locked;
}

/// The cell and paragraph properties of a style's vertical alignment, wrapping, borders and
/// horizontal alignment (PD-207); empty when it has none.
fn cell_look_xml(k: StyleKey) -> String {
    let mut cell = String::new();
    if let Some(protect) = cell_protect(k) {
        cell.push_str(&format!(" style:cell-protect=\"{protect}\""));
    }
    if let Some(v) = k.v_align().ods() {
        cell.push_str(&format!(" style:vertical-align=\"{v}\""));
    }
    if k.wrap() {
        cell.push_str(" fo:wrap-option=\"wrap\"");
    }
    let b = k.borders();
    for (side, edge) in [
        ("top", b.top),
        ("right", b.right),
        ("bottom", b.bottom),
        ("left", b.left),
    ] {
        if edge.is_none() {
            continue;
        }
        cell.push_str(&format!(" fo:border-{side}=\"{}\"", ods_border_value(edge)));
        if edge.line() == BorderLine::Double {
            cell.push_str(&format!(
                " style:border-line-width-{side}=\"0.5pt 0.5pt 0.5pt\""
            ));
        }
    }
    let mut xml = String::new();
    if !cell.is_empty() {
        xml.push_str(&format!("<style:table-cell-properties{cell}/>"));
    }
    if let Some(h) = k.h_align().ods() {
        xml.push_str(&format!(
            "<style:paragraph-properties fo:text-align=\"{h}\"/>"
        ));
    }
    xml
}

/// Alignment, wrapping and borders out of the cell and paragraph properties of a style body.
fn parse_cell_look(body: &str, style: &mut CellStyle) {
    if let Some((attrs, _)) =
        extract_tags_or_self_closing_with_attrs(body, "style:paragraph-properties")
            .into_iter()
            .next()
    {
        style.h_align = extract_attr_from_fragment(attrs, "fo:text-align")
            .map_or(HAlign::General, HAlign::from_ods);
    }
    let Some((attrs, _)) =
        extract_tags_or_self_closing_with_attrs(body, "style:table-cell-properties")
            .into_iter()
            .next()
    else {
        return;
    };
    let get = |name: &str| extract_attr_from_fragment(attrs, name);
    if let Some(value) = get("style:cell-protect") {
        parse_cell_protect(value, style);
    }
    style.v_align = get("style:vertical-align").map_or(VAlign::Bottom, VAlign::from_ods);
    style.wrap = get("fo:wrap-option") == Some("wrap");
    let all = get("fo:border").map(ods_border_edge);
    let edge = |side: &str| {
        get(&format!("fo:border-{side}"))
            .map(ods_border_edge)
            .or(all)
            .unwrap_or_default()
    };
    style.borders = CellBorders {
        top: edge("top"),
        right: edge("right"),
        bottom: edge("bottom"),
        left: edge("left"),
    };
}

/// `<table:table-column>` elements for the custom widths and the column groups (empty string when
/// there are none); columns without a width are filled with unstyled repeated columns.
pub(crate) fn table_columns_xml(styling: &SheetStyling, table: &OdsStyleTable) -> String {
    let outline = &styling.outline;
    let mut cuts: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for &col in styling.columns.keys() {
        cuts.extend([col, col + 1]);
    }
    for &(first, last) in &styling.unlocked_cols {
        cuts.extend([first, last + 1]);
    }
    for &(first, last, _) in &outline.cols {
        cuts.extend([first, last + 1]);
    }
    let header_end = crate::sheet_styling::ods_page::title_end(styling.page.title_cols);
    if let Some(end) = header_end {
        cuts.extend([0, end]);
    }
    let mut groups = GroupWriter::new(
        "table:table-column-group",
        &outline.cols,
        &outline.collapsed_cols,
    );
    // Stretches of equal width and level, from the cuts.
    let mut stretches: Vec<(u32, u32, Option<u32>, u8, bool)> = Vec::new();
    let cuts: Vec<u32> = cuts.into_iter().collect();
    for pair in cuts.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        let (px, level) = (styling.columns.get(&from).copied(), groups.level(from));
        let unlocked =
            crate::sheet_styling::xlsx_layout::run_contains(&styling.unlocked_cols, from)
                && table.unlocked_name().is_some();
        if px.is_none() && level == 0 && !unlocked && header_end.is_none_or(|end| from >= end) {
            continue;
        }
        match stretches.last_mut() {
            Some(last)
                if last.1 == from
                    && last.2 == px
                    && last.3 == level
                    && last.4 == unlocked
                    && header_end != Some(from) =>
            {
                last.1 = to
            }
            _ => stretches.push((from, to, px, level, unlocked)),
        }
    }
    let mut xml = String::new();
    let mut next = 0u32;
    let unlocked_name = table.unlocked_name();
    for (from, to, px, _, unlocked) in stretches {
        if from > next {
            groups.close_all(&mut xml);
            xml.push_str(&repeated_column(None, None, from - next));
        }
        if from == 0 && header_end.is_some() {
            xml.push_str("<table:table-header-columns>");
        }
        groups.move_to(&mut xml, from);
        xml.push_str(&repeated_column(
            px.and_then(|px| table.column_style_name(px)).as_deref(),
            unlocked_name.as_deref().filter(|_| unlocked),
            to - from,
        ));
        next = to;
        if header_end == Some(to) {
            groups.close_all(&mut xml);
            xml.push_str("</table:table-header-columns>");
        }
    }
    groups.close_all(&mut xml);
    xml
}

fn repeated_column(style: Option<&str>, default_cell: Option<&str>, count: u32) -> String {
    let mut style = style.map_or(String::new(), |s| {
        format!(" table:style-name=\"{}\"", XmlSanitizer::escape(s))
    });
    if let Some(cell_style) = default_cell {
        style.push_str(&format!(
            " table:default-cell-style-name=\"{}\"",
            XmlSanitizer::escape(cell_style)
        ));
    }
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
        ..CellStyle::default()
    }
}

/// Cell styles and column widths declared by the automatic styles of `content.xml`.
pub(crate) struct AutomaticStyles {
    pub(crate) cells: HashMap<String, CellStyle>,
    pub(crate) columns: HashMap<String, u32>,
    /// Tab colours and hidden states of the table styles (PD-208, PD-209).
    pub(crate) table_props: HashMap<String, TableProps>,
    /// Custom number format codes the cell styles refer to, and how many codes were replaced
    /// with General (PD-193).
    pub(crate) num_fmts: Arc<Vec<String>>,
    pub(crate) num_fmts_rejected: u32,
}

pub(crate) fn parse_automatic_styles(content_xml: &str) -> AutomaticStyles {
    let mut styles = AutomaticStyles {
        cells: HashMap::new(),
        columns: HashMap::new(),
        table_props: HashMap::new(),
        num_fmts: Arc::default(),
        num_fmts_rejected: 0,
    };
    let Some((_, auto)) = extract_tags_with_attrs(content_xml, "office:automatic-styles")
        .into_iter()
        .next()
    else {
        return styles;
    };
    styles.table_props = table_props(auto);
    let data_codes = read_data_styles(auto);
    let mut num_fmts = NumFmtTable::default();
    for (attrs, body) in extract_tags_with_attrs(auto, "style:style") {
        let (Some(name), Some(family)) = (
            extract_attr_from_fragment(attrs, "style:name"),
            extract_attr_from_fragment(attrs, "style:family"),
        ) else {
            continue;
        };
        let name = XmlSanitizer::unescape(name);
        if family == "table-cell" {
            let mut style = extract_tags_or_self_closing_with_attrs(body, "style:text-properties")
                .into_iter()
                .next()
                .map_or_else(CellStyle::default, |(props, _)| {
                    parse_text_properties(props)
                });
            parse_cell_look(body, &mut style);
            let code = extract_attr_from_fragment(attrs, "style:data-style-name")
                .and_then(|data| data_codes.get(&*XmlSanitizer::unescape(data)));
            if let Some(code) = code {
                style.num_fmt = num_fmts.intern(code);
            }
            styles.cells.insert(name, style);
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
    (styles.num_fmts, styles.num_fmts_rejected) = num_fmts.finish();
    styles
}

/// Collects cell and column styles of each table while the grid is walked.
pub(crate) struct StyleSink<'s> {
    styles: &'s AutomaticStyles,
    tables: Vec<(String, SheetStyling)>,
    columns: BTreeMap<u32, u32>,
    /// Columns whose default cell style is unlocked (PD-216).
    unlocked_cols: Vec<(u32, u32)>,
    cells: BTreeMap<(u32, u32), CellStyle>,
    name: String,
    row: u32,
    column: u32,
    /// The tab colour, the groups and the open groups (folded or not) of the table being read.
    tab_color: Option<u32>,
    hidden: bool,
    outline: Outline,
    open_groups: [Vec<bool>; 2],
    grouped: [u64; 2],
    /// The links of the table being read and what the document may still hold (PD-218).
    links: LinkCollector,
    link_allowance: LinkAllowance,
}

impl<'s> StyleSink<'s> {
    pub(crate) fn new(styles: &'s AutomaticStyles) -> Self {
        Self {
            styles,
            tables: Vec::new(),
            columns: BTreeMap::new(),
            unlocked_cols: Vec::new(),
            cells: BTreeMap::new(),
            name: String::new(),
            row: 0,
            column: 0,
            tab_color: None,
            hidden: false,
            outline: Outline::default(),
            open_groups: [Vec::new(), Vec::new()],
            grouped: [0, 0],
            links: LinkCollector::default(),
            link_allowance: LinkAllowance::default(),
        }
    }

    /// The group level and folded state of the rows (`axis` 0) or columns (1) `first..=last`.
    fn record_group(&mut self, axis: usize, first: u32, last: u32) {
        let depth = self.open_groups[axis].len();
        if depth == 0 || depth > usize::from(MAX_OUTLINE_LEVEL) {
            return;
        }
        let (levels, collapsed, limit) = match axis {
            0 => (
                &mut self.outline.rows,
                &mut self.outline.collapsed_rows,
                MAX_OUTLINE_ROWS,
            ),
            _ => (
                &mut self.outline.cols,
                &mut self.outline.collapsed_cols,
                u64::from(OdfCodec::MAX_COLS) + 1,
            ),
        };
        let last = last.min(if axis == 0 {
            OdfCodec::MAX_ROWS
        } else {
            OdfCodec::MAX_COLS
        });
        let before = self.grouped[axis];
        push_level(
            levels,
            &mut self.grouped[axis],
            limit,
            (first, last),
            depth as u8,
        );
        // A run the bounds refused is not in a group, so it cannot be folded either.
        if self.grouped[axis] > before && self.open_groups[axis].last() == Some(&true) {
            crate::sheet_styling::xlsx_layout::add_run(collapsed, first, last);
        }
    }

    /// Keeps the link of the cells `col..col + col_repeat` of the rows `self.row..self.row + repeat`; a
    /// repeated band is not individual linking, so only bounded blocks are kept (as for styles).
    fn record_link(&mut self, col: u32, repeat: u32, col_repeat: u32, href: &str, title: &str) {
        let target = href_to_target(href);
        let in_grid = u64::from(self.row) + u64::from(repeat) <= u64::from(OdfCodec::MAX_ROWS) + 1
            && u64::from(col) + u64::from(col_repeat) <= u64::from(OdfCodec::MAX_COLS) + 1;
        if !in_grid || u64::from(repeat) * u64::from(col_repeat) > 1024 {
            self.links.rejected = self.links.rejected.saturating_add(1);
            return;
        }
        let block = CellBox {
            row0: self.row,
            col0: col,
            row1: self.row + repeat - 1,
            col1: col + col_repeat - 1,
        };
        self.links
            .add_area(&mut self.link_allowance, block, &target, title);
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
        self.unlocked_cols = Vec::new();
        self.cells = BTreeMap::new();
        self.row = 0;
        self.column = 0;
        let props = attrs
            .get("table:style-name")
            .and_then(|n| self.styles.table_props.get(&*n).copied())
            .unwrap_or_default();
        self.tab_color = props.color;
        self.hidden = props.hidden;
        self.outline = Outline::default();
        self.open_groups = [Vec::new(), Vec::new()];
        self.grouped = [0, 0];
        self.links = LinkCollector::default();
    }

    fn group(&mut self, columns: bool, display: bool, open: bool) {
        let stack = &mut self.open_groups[usize::from(columns)];
        if !open {
            stack.pop();
        } else if stack.len() < 64 {
            // A group inside a folded one is folded too.
            let folded = !display || stack.last() == Some(&true);
            stack.push(folded);
        }
    }

    fn column(&mut self, attrs: &Attrs<'_>) {
        let repeat = attrs
            .get("table:number-columns-repeated")
            .map_or(1, |v| repeat_value(&v));
        self.record_group(1, self.column, self.column.saturating_add(repeat - 1));
        let unlocked = attrs
            .get("table:default-cell-style-name")
            .and_then(|n| self.styles.cells.get(&*n))
            .is_some_and(|style| style.unlocked);
        if unlocked {
            let last = self
                .column
                .saturating_add(repeat - 1)
                .min(OdfCodec::MAX_COLS);
            crate::sheet_styling::xlsx_layout::add_run(&mut self.unlocked_cols, self.column, last);
        }
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
        self.record_group(0, self.row, self.row.saturating_add(repeat - 1));
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
            if let Some((href, title)) = &cell.link {
                self.record_link(col, repeat, cell.col_repeat, href, title);
            }
            col = col.saturating_add(cell.col_repeat);
        }
        self.row = self.row.saturating_add(repeat);
        Ok(())
    }

    fn table_end(&mut self) {
        let mut styling = SheetStyling {
            cells: std::mem::take(&mut self.cells),
            columns: std::mem::take(&mut self.columns),
            unlocked_cols: std::mem::take(&mut self.unlocked_cols),
            num_fmts: Arc::clone(&self.styles.num_fmts),
            // The count belongs to the workbook; the first table carries it.
            num_fmts_rejected: if self.tables.is_empty() {
                self.styles.num_fmts_rejected
            } else {
                0
            },
            tab_color: self.tab_color.take(),
            hidden: std::mem::take(&mut self.hidden),
            outline: std::mem::take(&mut self.outline),
            ..SheetStyling::default()
        };
        let links = std::mem::take(&mut self.links);
        (styling.links, styling.links_rejected) = (links.links, links.rejected);
        styling.normalize_locks();
        self.tables.push((std::mem::take(&mut self.name), styling));
    }
}

/// Conditional formatting and validation rules of every table (a text scan, skipped when the
/// file has none).
pub(crate) fn with_rules(
    content_xml: &str,
    mut tables: Vec<(String, SheetStyling)>,
    validation: ValidationSink,
) -> Vec<(String, SheetStyling)> {
    let validated = validation.into_tables();
    let mut reader = OdsRuleReader::new(content_xml);
    if !reader.has_rules() {
        return tables;
    }
    let tables_xml = extract_balanced_elements(content_xml, "table:table");
    for (i, (_, styling)) in tables.iter_mut().enumerate() {
        let (Some((_, inner)), Some(cells)) = (tables_xml.get(i), validated.get(i)) else {
            continue;
        };
        styling.rules = reader.table_rules(inner, cells);
    }
    tables
}

/// Styling of every table of an ODS `content.xml`, keyed by table name.
pub(crate) fn parse_ods_styling(
    content_xml: &str,
) -> Result<Vec<(String, SheetStyling)>, EasyOfficeError> {
    let styles = parse_automatic_styles(content_xml);
    let mut sinks = (StyleSink::new(&styles), ValidationSink::default());
    walk_ods(content_xml, false, &mut sinks)?;
    Ok(with_rules(content_xml, sinks.0.into_tables(), sinks.1))
}

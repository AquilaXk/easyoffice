//! OpenDocument spreadsheet cell and column styles (PD-089): automatic styles written into
//! `content.xml` and read back (`style:text-properties`, `style:column-width`).

use std::collections::HashMap;

use crate::cell_type::{pack_cell_pos, CellValueKind};
use crate::pages_proto::FastU64Map;
use crate::rtf_xml_helpers::{
    extract_attr_from_fragment, extract_balanced_elements, extract_tags_or_self_closing_with_attrs,
    extract_tags_or_self_closing_with_attrs_limit, extract_tags_with_attrs,
};
use crate::sheet_styling::ods_default_style::{
    append_column_width_styles_xml, length_to_pt, ods_color, parse_column_width_px,
    parse_table_col_width, parse_table_row_height, parse_text_properties, quote_ods_font_family,
    read_default_styles, settle_col_widths, ColWidthRun, OdsDefaultStyles,
};
use crate::sheet_styling::ods_grid::{repeat_value, walk_ods, OdsCell, OdsSink};
use crate::sheet_styling::ods_numfmt::{data_style_xml, read_data_styles};
use crate::sheet_styling::ods_view::{table_props, TableProps};
use crate::sheet_styling::sheet_links::ods_links::href_to_target;
use crate::sheet_styling::sheet_links::{LinkAllowance, LinkCollector};
use crate::sheet_styling::sheet_look::{ods_border_attrs, ods_borders_from};
use crate::sheet_styling::sheet_row_height::{
    kept_default_row_height, ods_row_style_xml, read_ods_row_properties, settle_heights, HeightRun,
    RowHeight, MAX_ROW_BLOCK, MAX_ROW_HEIGHTS,
};
use crate::sheet_styling::sheet_rules::ods_rules_read::{OdsRuleReader, ValidationSink};
use crate::sheet_styling::sheet_view::{
    push_level, LevelRun, Outline, MAX_OUTLINE_LEVEL, MAX_OUTLINE_ROWS,
};
use crate::sheet_styling::{
    format_centipt, resolve_code, resolve_font_name, CellStyle, DefaultColWidth, DefaultFont,
    FontTable, HAlign, IdMemo, NumFmtTable, SheetStyling, StyleKey, VAlign, MAX_STYLED_CELLS,
};
use crate::xml_stream::{unescape_cow, Attrs};
use crate::{CellBox, EasyOfficeError, OdfCodec, XmlSanitizer};
use std::sync::Arc;

/// Automatic style names for a workbook: `ce<N>` per distinct cell style and `co<N>` per distinct
/// column width, in first-use order.
pub(crate) struct OdsStyleTable {
    cells: Vec<StyleKey>,
    cell_index: HashMap<StyleKey, usize>,
    widths: Vec<u32>,
    width_index: HashMap<u32, usize>,
    /// Distinct row heights of the workbook, in first-use order: `ro<N>` names entry `N - 1` (PD-278).
    row_heights: Vec<RowHeight>,
    row_height_index: HashMap<RowHeight, usize>,
    /// Custom number format codes of the whole workbook (PD-193); `eonf<id>` data styles.
    num_fmts: NumFmtTable,
    /// Non-default font family names of the whole workbook (PD-244).
    font_names: FontTable,
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
            row_heights: Vec::new(),
            row_height_index: HashMap::new(),
            num_fmts: NumFmtTable::default(),
            font_names: FontTable::default(),
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
                let fid = ids.font(style.font, styling, |name| table.font_names.intern(name));
                let key = style.key().with_num_fmt(id).with_font(fid);
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
            if styling.default_col_width.is_some() {
                let def_px = styling.default_col_px();
                table.width_index.entry(def_px).or_insert_with(|| {
                    table.widths.push(def_px);
                    table.widths.len()
                });
            }
            for &height in styling
                .row_heights
                .values()
                .chain(&styling.user_default_row_height())
            {
                table.row_height_index.entry(height).or_insert_with(|| {
                    table.row_heights.push(height);
                    table.row_heights.len()
                });
            }
        }
        table
    }

    /// The name of the row style of a row with this height.
    pub(crate) fn row_style_name(&self, height: RowHeight) -> Option<String> {
        self.row_height_index.get(&height).map(|i| format!("ro{i}"))
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
        let fid = ids.font(style.font, styling, |name| {
            self.font_names.lookup(name).unwrap_or(0)
        });
        self.cell_index
            .get(&style.key().with_num_fmt(id).with_font(fid))
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
        if self.cells.is_empty() && self.widths.is_empty() && self.row_heights.is_empty() {
            return String::new();
        }
        let mut xml = String::from("<office:automatic-styles>");
        for &id in &self.data_ids {
            if let Some(code) = resolve_code(id, self.num_fmts.codes()) {
                xml.push_str(&data_style_xml(&format!("eonf{id}"), code));
            }
        }
        append_column_width_styles_xml(&self.widths, &mut xml);
        for (i, &height) in self.row_heights.iter().enumerate() {
            xml.push_str(&ods_row_style_xml(i + 1, height));
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
            if let Some(rgb) = k.font_color() {
                props.push_str(&format!(" fo:color=\"#{rgb:06x}\""));
            }
            if let Some(name) = resolve_font_name(k.font(), self.font_names.names()) {
                props.push_str(&format!(
                    " fo:font-family=\"{}\"",
                    quote_ods_font_family(name)
                ));
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

/// The cell and paragraph properties of a style's vertical alignment, wrapping, rotation, borders and
/// horizontal alignment (PD-207, PD-356); empty when it has none.
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
    let rot = k.text_rotation();
    if rot != 0 {
        cell.push_str(&format!(" style:rotation-angle=\"{rot}\""));
    }
    if let Some(rgb) = k.fill() {
        cell.push_str(&format!(" fo:background-color=\"#{rgb:06x}\""));
    }
    cell.push_str(&ods_border_attrs(k.borders()));
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

fn parse_ods_rotation(raw: &str) -> i16 {
    let Ok(mut deg) = raw.trim().parse::<i32>() else {
        return 0;
    };
    if (270..=359).contains(&deg) {
        deg -= 360;
    }
    if (-90..=90).contains(&deg) {
        deg as i16
    } else {
        0
    }
}

/// Alignment, wrapping, rotation, borders and fill out of the cell and paragraph properties of a style body.
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
    style.text_rotation = get("style:rotation-angle").map_or(0, parse_ods_rotation);
    style.fill = get("fo:background-color").and_then(ods_color);
    style.borders = ods_borders_from(get);
}

/// Cell styles and column widths declared by the automatic styles of `content.xml`.
pub(crate) struct AutomaticStyles {
    pub(crate) cells: HashMap<String, CellStyle>,
    pub(crate) columns: HashMap<String, u32>,
    /// Row heights by row style name, with whether a user set them (PD-278). A height that follows the content is kept
    /// for the band that ends a table, which is the default row height of the sheet (PD-286).
    pub(crate) rows: HashMap<String, RowHeight>,
    /// Tab colours and hidden states of the table styles (PD-208, PD-209).
    pub(crate) table_props: HashMap<String, TableProps>,
    /// Custom number format codes the cell styles refer to, and how many codes were replaced
    /// with General (PD-193).
    pub(crate) num_fmts: Arc<Vec<String>>,
    pub(crate) num_fmts_rejected: u32,
    pub(crate) fonts: Arc<Vec<String>>,
    pub(crate) fonts_rejected: u32,
}

pub(crate) fn parse_automatic_styles(content_xml: &str) -> AutomaticStyles {
    let mut styles = AutomaticStyles {
        cells: HashMap::new(),
        columns: HashMap::new(),
        rows: HashMap::new(),
        table_props: HashMap::new(),
        num_fmts: Arc::default(),
        num_fmts_rejected: 0,
        fonts: Arc::default(),
        fonts_rejected: 0,
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
    let mut font_table = FontTable::default();
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
                    parse_text_properties(props, &mut font_table)
                });
            parse_cell_look(body, &mut style);
            let code = extract_attr_from_fragment(attrs, "style:data-style-name")
                .and_then(|data| data_codes.get(&*XmlSanitizer::unescape(data)));
            if let Some(code) = code {
                style.num_fmt = num_fmts.intern(code);
            }
            styles.cells.insert(name, style);
        } else if family == "table-column" {
            if let Some(px) = parse_column_width_px(body) {
                styles.columns.insert(name, px);
            }
        } else if family == "table-row" {
            let height = extract_tags_or_self_closing_with_attrs_limit(
                body,
                "style:table-row-properties",
                1,
            )
            .into_iter()
            .next()
            .and_then(|(props, _)| read_ods_row_properties(props, length_to_pt));
            if let Some(height) = height {
                styles.rows.insert(name, height);
            }
        }
    }
    (styles.num_fmts, styles.num_fmts_rejected) = num_fmts.finish();
    (styles.fonts, styles.fonts_rejected) = font_table.finish();
    styles
}

/// Rows one table may list as hidden by a filter; more are not recorded.
const MAX_FILTERED_ROWS: u32 = 1_048_576;

/// Collects cell and column styles of each table while the grid is walked.
pub(crate) struct StyleSink<'s> {
    styles: &'s AutomaticStyles,
    default_font: Option<DefaultFont>,
    tables: Vec<(String, SheetStyling)>,
    col_runs: Vec<ColWidthRun>,
    default_col_width: Option<DefaultColWidth>,
    /// Columns whose default cell style is unlocked (PD-216).
    unlocked_cols: Vec<(u32, u32)>,
    cells: std::collections::BTreeMap<(u32, u32), CellStyle>,
    cell_types: FastU64Map<CellValueKind>,
    name: String,
    row: u32,
    column: u32,
    /// The tab colour, the groups and the open groups (folded or not) of the table being read.
    tab_color: Option<u32>,
    hidden: bool,
    outline: Outline,
    /// The groups open now, per axis: `(hidden, display)`: whether the group's rows are hidden (it or a group around
    /// it is folded) and whether the group itself is shown open.
    open_groups: [Vec<(bool, bool)>; 2],
    /// The groups shown open inside folded ones, as the runs a table lists them (`Outline::settle_open`).
    open_runs: [Vec<LevelRun>; 2],
    grouped: [u64; 2],
    /// The links of the table being read and what the document may still hold (PD-218).
    links: LinkCollector,
    link_allowance: LinkAllowance,
    /// The rows an auto filter hides in the table being read, and whether the next row is one.
    filtered: Vec<(u32, u32)>,
    /// The rows in `filtered`, counted as they are added (a sum over the runs per row would be quadratic).
    filtered_total: u64,
    next_filtered: bool,
    /// The heights the user set on the rows of the table being read, as stretches of rows, and the one the next row has.
    /// The rows with the sheet's default height are among them until the end of the table says what that is (PD-300).
    height_runs: Vec<HeightRun>,
    next_height: Option<RowHeight>,
    /// The height of the band of rows that ends the table being read: its default row height (PD-286).
    default_height: Option<RowHeight>,
    /// Per-table `eo:default-row-height` attribute when different from the workbook default (PD-345).
    eo_default_height: Option<RowHeight>,
    /// Workbook `<style:default-style style:family="table-row">` height from `styles.xml` / header (PD-345).
    wb_default_row_height: Option<RowHeight>,
}

impl<'s> StyleSink<'s> {
    #[allow(dead_code)]
    pub(crate) fn new(styles: &'s AutomaticStyles) -> Self {
        Self::new_with_defaults(styles, None, None)
    }

    pub(crate) fn new_with_defaults(
        styles: &'s AutomaticStyles,
        default_font: Option<&DefaultFont>,
        wb_default_row_height: Option<RowHeight>,
    ) -> Self {
        Self {
            styles,
            default_font: default_font.cloned(),
            tables: Vec::new(),
            col_runs: Vec::new(),
            default_col_width: None,
            unlocked_cols: Vec::new(),
            cells: std::collections::BTreeMap::new(),
            cell_types: FastU64Map::default(),
            name: String::new(),
            row: 0,
            column: 0,
            tab_color: None,
            hidden: false,
            outline: Outline::default(),
            open_groups: [Vec::new(), Vec::new()],
            open_runs: [Vec::new(), Vec::new()],
            grouped: [0, 0],
            links: LinkCollector::default(),
            link_allowance: LinkAllowance::default(),
            filtered: Vec::new(),
            filtered_total: 0,
            next_filtered: false,
            height_runs: Vec::new(),
            next_height: None,
            default_height: None,
            eo_default_height: None,
            wb_default_row_height,
        }
    }

    /// The outline of the table with the groups it shows open inside folded ones.
    fn settled_outline(&mut self) -> Outline {
        let mut outline = std::mem::take(&mut self.outline);
        let [rows, cols] = std::mem::take(&mut self.open_runs);
        outline.open_rows = Outline::settle_open(&outline.rows, &outline.collapsed_rows, rows);
        outline.open_cols = Outline::settle_open(&outline.cols, &outline.collapsed_cols, cols);
        outline
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
        if self.grouped[axis] > before {
            if let Some(&(true, shown_open)) = self.open_groups[axis].last() {
                crate::sheet_styling::xlsx_layout::add_run(collapsed, first, last);
                if shown_open {
                    self.open_runs[axis].push((first, last, depth as u8));
                }
            }
        }
    }

    /// Keeps the height of the rows `self.row..self.row + repeat` a user set. A repeated band is the sheet's default look,
    /// not a height someone set row by row, so only bounded blocks are kept (as for styles); the band that runs to the
    /// last row of the grid gives the sheet its default row height, whoever set it (PD-286).
    fn record_row_height(&mut self, repeat: u32) {
        let Some(height) = self.next_height.take() else {
            return;
        };
        let end = u64::from(self.row) + u64::from(repeat);
        if end > u64::from(OdfCodec::MAX_ROWS) + 1 {
            return;
        }
        if repeat > MAX_ROW_BLOCK {
            if end == u64::from(OdfCodec::MAX_ROWS) + 1 {
                self.default_height = Some(height);
            }
        } else if height.custom {
            self.push_height_run(repeat, height);
        }
    }

    /// Adds the rows `self.row..self.row + rows` to the stretches, joined to the last one when it ends where they begin
    /// with the same height. A stretch is no more than a row, so a table makes at most [`MAX_ROW_HEIGHTS`] of them and
    /// the rows of the default height (the one stretch of a sheet that sets it on every row) cost one.
    fn push_height_run(&mut self, rows: u32, height: RowHeight) {
        let first = self.row;
        if let Some(last) = self.height_runs.last_mut() {
            if last.height == height && last.first + last.rows == first {
                last.rows += rows;
                return;
            }
        }
        if self.height_runs.len() < MAX_ROW_HEIGHTS {
            self.height_runs.push(HeightRun {
                first,
                rows,
                height,
            });
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

    fn push_col_width_run(&mut self, first: u32, cols: u32, px: u32) {
        if cols == 0 {
            return;
        }
        if let Some(last) = self.col_runs.last_mut() {
            if last.px == px && last.first + last.cols == first {
                last.cols += cols;
                return;
            }
        }
        if self.col_runs.len() <= OdfCodec::MAX_COLS as usize {
            self.col_runs.push(ColWidthRun { first, cols, px });
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
        self.col_runs.clear();
        self.default_col_width = parse_table_col_width(attrs);
        self.unlocked_cols = Vec::new();
        self.cells = std::collections::BTreeMap::new();
        self.cell_types = FastU64Map::default();
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
        self.open_runs = [Vec::new(), Vec::new()];
        self.grouped = [0, 0];
        self.links = LinkCollector::default();
        self.filtered = Vec::new();
        self.filtered_total = 0;
        self.height_runs = Vec::new();
        self.default_height = None;
        self.eo_default_height = parse_table_row_height(attrs);
    }

    fn row_filtered(&mut self, filtered: bool) {
        self.next_filtered = filtered;
    }

    fn row_style(&mut self, style: Option<&str>) {
        self.next_height = style.and_then(|name| self.styles.rows.get(name).copied());
    }

    fn group(&mut self, columns: bool, display: bool, open: bool) {
        let stack = &mut self.open_groups[usize::from(columns)];
        if !open {
            stack.pop();
        } else if stack.len() < 64 {
            // The rows of a group inside a folded one are hidden, whether or not the group is folded itself.
            let hidden = !display || stack.last().is_some_and(|g| g.0);
            stack.push((hidden, display));
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
            let end = u64::from(self.column) + u64::from(repeat);
            if end <= u64::from(OdfCodec::MAX_COLS) + 1 {
                self.push_col_width_run(self.column, repeat, px);
            }
        }
        self.column = self.column.saturating_add(repeat);
    }

    fn row(&mut self, repeat: u32, cells: &[OdsCell<'_>]) -> Result<(), EasyOfficeError> {
        self.record_group(0, self.row, self.row.saturating_add(repeat - 1));
        self.record_row_height(repeat);
        let mut col = 0u32;
        for cell in cells {
            let style = cell
                .style
                .and_then(|n| self.styles.cells.get(&*unescape_cow(n)))
                .filter(|s| !s.is_plain());
            let block = u64::from(repeat) * u64::from(cell.col_repeat);
            let in_grid = u64::from(self.row) + u64::from(repeat)
                <= u64::from(OdfCodec::MAX_ROWS) + 1
                && u64::from(col) + u64::from(cell.col_repeat) <= u64::from(OdfCodec::MAX_COLS) + 1;
            if let Some(style) = style {
                // A style repeated across a whole row/column band (a "format the sheet" fill) is
                // not individual cell formatting; only bounded blocks are kept.
                if in_grid && block <= 1024 && self.cells.len() + block as usize <= MAX_STYLED_CELLS
                {
                    for r in self.row..self.row + repeat {
                        for c in col..col + cell.col_repeat {
                            self.cells.insert((r, c), *style);
                        }
                    }
                }
            }
            if let (Some(text), Some(kind)) = (&cell.text, cell.kind) {
                if !text.is_empty()
                    && kind != CellValueKind::infer(text)
                    && in_grid
                    && block <= 1024
                    && self.cell_types.len() + block as usize <= MAX_STYLED_CELLS
                {
                    let base_r = cell.explicit_row.unwrap_or(self.row);
                    let base_c = cell.explicit_col.unwrap_or(col);
                    for dr in 0..repeat {
                        for dc in 0..cell.col_repeat {
                            self.cell_types
                                .insert(pack_cell_pos(base_r + dr, base_c + dc), kind);
                        }
                    }
                }
            }
            if let Some((href, title)) = &cell.link {
                self.record_link(col, repeat, cell.col_repeat, href, title);
            }
            col = col.saturating_add(cell.col_repeat);
        }
        if std::mem::take(&mut self.next_filtered)
            && self.filtered_total < u64::from(MAX_FILTERED_ROWS)
        {
            let last = self.row.saturating_add(repeat - 1).min(OdfCodec::MAX_ROWS);
            crate::sheet_styling::xlsx_layout::add_run(&mut self.filtered, self.row, last);
            self.filtered_total += u64::from(last.saturating_sub(self.row)) + 1;
        }
        self.row = self.row.saturating_add(repeat);
        Ok(())
    }

    fn table_end(&mut self) {
        let raw_default = self
            .default_height
            .take()
            .or_else(|| self.eo_default_height.take())
            .or(self.wb_default_row_height);
        let default_row_height = kept_default_row_height(raw_default, self.default_font.as_ref());
        let (columns, default_col_width) = settle_col_widths(
            std::mem::take(&mut self.col_runs),
            self.default_col_width.take(),
            self.default_font.as_ref(),
        );
        let mut styling = SheetStyling {
            cells: std::mem::take(&mut self.cells),
            cell_types: std::mem::take(&mut self.cell_types),
            columns,
            default_col_width,
            default_font: self.default_font.clone(),
            unlocked_cols: std::mem::take(&mut self.unlocked_cols),
            num_fmts: Arc::clone(&self.styles.num_fmts),
            // The count belongs to the workbook; the first table carries it.
            num_fmts_rejected: if self.tables.is_empty() {
                self.styles.num_fmts_rejected
            } else {
                0
            },
            fonts: Arc::clone(&self.styles.fonts),
            fonts_rejected: if self.tables.is_empty() {
                self.styles.fonts_rejected
            } else {
                0
            },
            tab_color: self.tab_color.take(),
            hidden: std::mem::take(&mut self.hidden),
            outline: self.settled_outline(),
            filtered_rows: std::mem::take(&mut self.filtered),
            // A row that has the default height is not a height of its own (the writer styles every row with it).
            row_heights: settle_heights(std::mem::take(&mut self.height_runs), default_row_height),
            default_row_height,
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
#[allow(dead_code)]
pub(crate) fn parse_ods_styling(
    content_xml: &str,
) -> Result<Vec<(String, SheetStyling)>, EasyOfficeError> {
    let defaults = read_default_styles(None, content_xml)?;
    parse_ods_styling_with_defaults(content_xml, &defaults)
}

/// Styling of every table of an ODS `content.xml`, settling column widths and row heights with workbook `defaults`.
pub(crate) fn parse_ods_styling_with_defaults(
    content_xml: &str,
    defaults: &OdsDefaultStyles,
) -> Result<Vec<(String, SheetStyling)>, EasyOfficeError> {
    let styles = parse_automatic_styles(content_xml);
    let mut sinks = (
        StyleSink::new_with_defaults(&styles, defaults.font.as_ref(), defaults.row_height),
        ValidationSink::default(),
    );
    let mut budget = crate::decode_budget::DecodeBudget::new();
    let date1904 = crate::sheet_styling::ods_date_system::is_date1904(content_xml);
    walk_ods(content_xml, true, date1904, &mut sinks, &mut budget)?;
    Ok(with_rules(content_xml, sinks.0.into_tables(), sinks.1))
}

//! SpreadsheetML cell styles and column widths (PD-089): `xl/styles.xml` writer and reader, the
//! `<cols>` element, and the per-cell `s` attribute.
//!
//! Writer layout: `numFmts` holds the custom number format codes (ids from 164, PD-193); `fonts`
//! holds the default font (index 0) plus one font per distinct font look; `cellXfs` holds the
//! default format (index 0) plus one format per distinct style (font look, number format,
//! alignment and borders); `borders` holds the empty border (index 0) plus one entry per distinct
//! set of edges (PD-207).
//! Elements follow the ECMA-376 `CT_Stylesheet` sequence (`numFmts`, `fonts`, `fills`, `borders`,
//! `cellStyleXfs`, `cellXfs`, `cellStyles`); spreadsheet programs reject any other order.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::rtf_xml_helpers::{
    extract_attr_from_fragment, extract_tag_bodies, extract_tags_or_self_closing_with_attrs,
};
use crate::sheet_styling::sheet_filter::color_criteria;
use crate::sheet_styling::sheet_look::{xlsx_color_rgb, BorderEdge, BorderLine, CellBorders};
use crate::sheet_styling::sheet_rules::xlsx_rules::DxfTable;
use crate::sheet_styling::{
    format_centipt, CellStyle, HAlign, IdMemo, NumFmtTable, SheetStyling, StyleKey, VAlign,
    CUSTOM_BASE, MAX_COLUMN_PX, MIN_COLUMN_PX,
};
use crate::XmlSanitizer;
use std::sync::Arc;

/// Width of the widest digit of the default font (Calibri 11) in CSS pixels. The spreadsheet column width
/// unit is the number of such digits; the stored width includes the 5 px cell padding:
/// `width = trunc(256 * px / MDW) / 256`, hence `px = round(width * MDW)`.
pub(crate) const MAX_DIGIT_WIDTH_PX: f64 = 7.0;
const DEFAULT_SIZE_PT: f64 = 11.0;
/// `cellStyles` entries a reader looks at (a spreadsheet program keeps fewer than this many cell styles).
const MAX_CELL_STYLE_ENTRIES: usize = 65_536;
/// Border definitions a reader keeps; a stylesheet with more reads the rest as no border.
pub(crate) const MAX_BORDER_DEFS: usize = 65_536;

/// Converts a column width in CSS pixels to the SpreadsheetML `width` attribute value.
pub(crate) fn px_to_xlsx_width(px: u32) -> f64 {
    (f64::from(px) / MAX_DIGIT_WIDTH_PX * 256.0).floor() / 256.0
}

/// Converts a SpreadsheetML `width` attribute value to CSS pixels.
pub(crate) fn xlsx_width_to_px(width: f64) -> u32 {
    (width * MAX_DIGIT_WIDTH_PX)
        .round()
        .clamp(f64::from(MIN_COLUMN_PX), f64::from(MAX_COLUMN_PX)) as u32
}

/// The format of an unlocked row or column: the default look with the lock off (PD-216).
fn unlocked_style() -> CellStyle {
    CellStyle {
        unlocked: true,
        ..CellStyle::default()
    }
}

/// Colour of the spreadsheet program's "Hyperlink" and "Followed Hyperlink" cell styles (ARGB).
const HYPERLINK_COLOR: &str = "FF0563C1";
/// The font of the "Hyperlink" cell style: the default font, underlined, in [`HYPERLINK_COLOR`] (PD-223).
const HYPERLINK_STYLE_KEY: StyleKey = StyleKey::LINK_ONLY;
/// `builtinId` of the "Hyperlink" and "Followed Hyperlink" cell styles (ECMA-376 18.8.7).
const BUILTIN_HYPERLINK: &str = "8";
const BUILTIN_FOLLOWED_HYPERLINK: &str = "9";
/// `cellStyleXfs` index the writer gives the "Hyperlink" style (index 0 is Normal).
const HYPERLINK_STYLE_XF: usize = 1;

/// Deduplicated styles of a whole workbook, in first-use order.
pub(crate) struct XlsxStyleTable {
    /// Distinct font looks (flags and size), font id `n + 1`.
    fonts: Vec<StyleKey>,
    font_index: HashMap<StyleKey, usize>,
    /// Distinct styles with workbook-wide number format ids, `cellXfs` index `n + 1`.
    styles: Vec<StyleKey>,
    index: HashMap<StyleKey, usize>,
    /// Distinct non-empty border sets, border id `n + 1`.
    borders: Vec<CellBorders>,
    border_index: HashMap<CellBorders, usize>,
    /// Custom number format codes of the whole workbook.
    num_fmts: NumFmtTable,
    /// Differential formats of the conditional formatting rules (PD-171).
    pub(crate) dxf: DxfTable,
}

impl XlsxStyleTable {
    pub(crate) fn build(stylings: &[SheetStyling]) -> Self {
        let mut table = Self {
            fonts: Vec::new(),
            font_index: HashMap::new(),
            styles: Vec::new(),
            index: HashMap::new(),
            borders: Vec::new(),
            border_index: HashMap::new(),
            num_fmts: NumFmtTable::default(),
            dxf: DxfTable::build(
                stylings
                    .iter()
                    .flat_map(|st| st.rules.cf.iter().map(|rule| rule.style)),
            )
            .with_colors(stylings.iter().flat_map(|st| {
                let sheet = st.filter.iter().flat_map(|f| color_criteria(&f.columns));
                let tables = st
                    .tables
                    .iter()
                    .filter_map(|t| t.filter.as_deref())
                    .flat_map(color_criteria);
                sheet.chain(tables)
            })),
        };
        for styling in stylings {
            let mut ids = IdMemo::new();
            let unlocked = (!styling.unlocked_rows.is_empty() || !styling.unlocked_cols.is_empty())
                .then(unlocked_style);
            let linked = styling
                .links
                .keys()
                .map(|&(row, col)| (styling.link_cell_style(row, col), true));
            let plain = styling
                .cells
                .values()
                .copied()
                .chain(unlocked)
                .filter(|s| !s.is_plain())
                .map(|style| (style, false));
            for (style, link) in plain.chain(linked) {
                let id = ids.get(style.num_fmt, styling, |code| table.num_fmts.intern(code));
                let key = style.key().with_num_fmt(id);
                let key = if link { key.with_link() } else { key };
                if key == StyleKey::default() || table.index.contains_key(&key) {
                    continue;
                }
                table.styles.push(key);
                table.index.insert(key, table.styles.len());
                let borders = key.borders();
                if !borders.is_none() && !table.border_index.contains_key(&borders) {
                    table.borders.push(borders);
                    table.border_index.insert(borders, table.borders.len());
                }
                let font = key.font_part();
                if font != StyleKey::default() && !table.font_index.contains_key(&font) {
                    table.fonts.push(font);
                    table.font_index.insert(font, table.fonts.len());
                }
            }
        }
        if table.styles.iter().any(|key| key.link()) {
            let font = HYPERLINK_STYLE_KEY.font_part();
            if !table.font_index.contains_key(&font) {
                table.fonts.push(font);
                table.font_index.insert(font, table.fonts.len());
            }
        }
        table
    }

    /// `cellXfs` index of the format of an unlocked row or column (0 when the table has none).
    pub(crate) fn unlocked_xf(&self) -> usize {
        self.index
            .get(&unlocked_style().key())
            .copied()
            .unwrap_or(0)
    }

    /// `cellXfs` index of `style` of `styling`; 0 (the default format) for plain styles. `ids` is
    /// one memo per sheet.
    pub(crate) fn xf_index(
        &self,
        style: &CellStyle,
        styling: &SheetStyling,
        ids: &mut IdMemo,
        link: bool,
    ) -> usize {
        if style.is_plain() && !link {
            return 0;
        }
        let id = ids.get(style.num_fmt, styling, |code| {
            self.num_fmts.lookup(code).unwrap_or(0)
        });
        let key = style.key().with_num_fmt(id);
        self.index
            .get(&if link { key.with_link() } else { key })
            .copied()
            .unwrap_or(0)
    }

    /// `cellXfs` index of the cell `(row, col)`: its own format, the hyperlink look when it has a link (PD-223), or
    /// the lock of its unlocked row or column; 0 for a cell with none of them. `ids` is one memo per sheet.
    pub(crate) fn cell_xf(
        &self,
        styling: &SheetStyling,
        row: u32,
        col: u32,
        ids: &mut IdMemo,
    ) -> usize {
        let link = styling.links.contains_key(&(row, col));
        match styling.cells.get(&(row, col)) {
            Some(style) => self.xf_index(style, styling, ids, link),
            None if link => self.xf_index(&styling.link_cell_style(row, col), styling, ids, true),
            None if styling.cell_unlocked(row, col) => self.unlocked_xf(),
            None => 0,
        }
    }

    fn font_xml(key: Option<StyleKey>) -> String {
        let mut out = String::from("<font>");
        if let Some(k) = key {
            for (on, tag) in [
                (k.bold(), "<b/>"),
                (k.italic(), "<i/>"),
                (k.strike(), "<strike/>"),
                (k.underline(), "<u/>"),
            ] {
                if on {
                    out.push_str(tag);
                }
            }
            if k.link() && !k.underline() {
                out.push_str("<u/>");
            }
            let sz = k
                .size_centipt()
                .map_or_else(|| "11".to_string(), format_centipt);
            out.push_str(&format!("<sz val=\"{sz}\"/>"));
            if k.link() {
                out.push_str(&format!("<color rgb=\"{HYPERLINK_COLOR}\"/>"));
            }
        } else {
            out.push_str("<sz val=\"11\"/>");
        }
        out.push_str("<name val=\"Calibri\"/></font>");
        out
    }

    fn num_fmts_xml(&self) -> String {
        let codes = self.num_fmts.codes();
        if codes.is_empty() {
            return String::new();
        }
        let mut xml = format!("<numFmts count=\"{}\">", codes.len());
        for (n, code) in codes.iter().enumerate() {
            xml.push_str(&format!(
                "<numFmt numFmtId=\"{}\" formatCode=\"{}\"/>",
                usize::from(CUSTOM_BASE) + n,
                XmlSanitizer::escape(code)
            ));
        }
        xml.push_str("</numFmts>");
        xml
    }

    fn border_xml(borders: CellBorders) -> String {
        let mut out = String::from("<border>");
        for (tag, edge) in [
            ("left", borders.left),
            ("right", borders.right),
            ("top", borders.top),
            ("bottom", borders.bottom),
        ] {
            match edge.line().xlsx() {
                Some(style) => out.push_str(&format!(
                    "<{tag} style=\"{style}\"><color rgb=\"FF{:06X}\"/></{tag}>",
                    edge.rgb()
                )),
                None => out.push_str(&format!("<{tag}/>")),
            }
        }
        out.push_str("<diagonal/></border>");
        out
    }

    fn alignment_xml(key: StyleKey) -> String {
        let mut out = String::new();
        if let Some(h) = key.h_align().xlsx() {
            out.push_str(&format!(" horizontal=\"{h}\""));
        }
        if let Some(v) = key.v_align().xlsx() {
            out.push_str(&format!(" vertical=\"{v}\""));
        }
        if key.wrap() {
            out.push_str(" wrapText=\"1\"");
        }
        if out.is_empty() {
            out
        } else {
            format!("<alignment{out}/>")
        }
    }

    fn xf_xml(&self, key: StyleKey) -> String {
        let font_id = self.font_index.get(&key.font_part()).copied().unwrap_or(0);
        let border_id = self.border_index.get(&key.borders()).copied().unwrap_or(0);
        let num_fmt = key.num_fmt();
        let style_xf = if key.link() { HYPERLINK_STYLE_XF } else { 0 };
        let mut xf = format!(
            "<xf numFmtId=\"{num_fmt}\" fontId=\"{font_id}\" fillId=\"0\" borderId=\"{border_id}\" xfId=\"{style_xf}\""
        );
        if num_fmt != 0 {
            xf.push_str(" applyNumberFormat=\"1\"");
        }
        if font_id != 0 {
            xf.push_str(" applyFont=\"1\"");
        }
        if border_id != 0 {
            xf.push_str(" applyBorder=\"1\"");
        }
        let alignment = Self::alignment_xml(key);
        let protection = Self::protection_xml(key);
        if alignment.is_empty() && protection.is_empty() {
            xf.push_str("/>");
            return xf;
        }
        if !alignment.is_empty() {
            xf.push_str(" applyAlignment=\"1\"");
        }
        if !protection.is_empty() {
            xf.push_str(" applyProtection=\"1\"");
        }
        xf.push_str(&format!(">{alignment}{protection}</xf>"));
        xf
    }

    /// `<protection>` of a format: `locked="0"` for an unlocked cell, `locked="1"` for one kept locked inside
    /// an unlocked row or column, `hidden="1"` for a hidden formula (ECMA-376 18.8.33).
    fn protection_xml(key: StyleKey) -> String {
        let mut out = String::new();
        if key.unlocked() {
            out.push_str(" locked=\"0\"");
        } else if key.force_locked() {
            out.push_str(" locked=\"1\"");
        }
        if key.formula_hidden() {
            out.push_str(" hidden=\"1\"");
        }
        if out.is_empty() {
            out
        } else {
            format!("<protection{out}/>")
        }
    }

    /// `cellStyleXfs`: Normal and, when a cell carries a link, the "Hyperlink" style with its own font.
    fn cell_style_xfs_xml(&self, linked: bool) -> String {
        let normal = "<xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/>";
        if !linked {
            return format!("<cellStyleXfs count=\"1\">{normal}</cellStyleXfs>");
        }
        let font_id = self
            .font_index
            .get(&HYPERLINK_STYLE_KEY.font_part())
            .copied()
            .unwrap_or(0);
        format!(
            "<cellStyleXfs count=\"2\">{normal}<xf numFmtId=\"0\" fontId=\"{font_id}\" fillId=\"0\" borderId=\"0\" applyNumberFormat=\"0\" applyFill=\"0\" applyBorder=\"0\" applyAlignment=\"0\" applyProtection=\"0\"/></cellStyleXfs>"
        )
    }

    pub(crate) fn to_xml(&self) -> String {
        let mut xml = String::from(concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">"
        ));
        xml.push_str(&self.num_fmts_xml());
        xml.push_str(&format!("<fonts count=\"{}\">", self.fonts.len() + 1));
        xml.push_str(&Self::font_xml(None));
        for key in &self.fonts {
            xml.push_str(&Self::font_xml(Some(*key)));
        }
        xml.push_str("</fonts>");
        xml.push_str("<fills count=\"1\"><fill><patternFill patternType=\"none\"/></fill></fills>");
        xml.push_str(&format!("<borders count=\"{}\">", self.borders.len() + 1));
        xml.push_str(&Self::border_xml(CellBorders::default()));
        for &borders in &self.borders {
            xml.push_str(&Self::border_xml(borders));
        }
        xml.push_str("</borders>");
        let linked = self.styles.iter().any(|key| key.link());
        xml.push_str(&self.cell_style_xfs_xml(linked));
        xml.push_str(&format!("<cellXfs count=\"{}\">", self.styles.len() + 1));
        xml.push_str("<xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/>");
        for &key in &self.styles {
            xml.push_str(&self.xf_xml(key));
        }
        xml.push_str("</cellXfs>");
        xml.push_str(&cell_styles_xml(linked));
        xml.push_str(&self.dxf.to_xml());
        xml.push_str("</styleSheet>");
        xml
    }
}

/// `cellStyles`: Normal and, when a cell carries a link, "Hyperlink" (`builtinId` 8).
fn cell_styles_xml(linked: bool) -> String {
    if linked {
        format!("<cellStyles count=\"2\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/><cellStyle name=\"Hyperlink\" xfId=\"{HYPERLINK_STYLE_XF}\" builtinId=\"{BUILTIN_HYPERLINK}\"/></cellStyles>")
    } else {
        "<cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles>".to_string()
    }
}

/// `<cols>` element (empty string when no column has a custom width). Adjacent columns with the
/// same width share one `<col>`.
pub(crate) fn cols_xml(columns: &BTreeMap<u32, u32>) -> String {
    if columns.is_empty() {
        return String::new();
    }
    let mut runs: Vec<(u32, u32, u32)> = Vec::new();
    for (&col, &px) in columns {
        match runs.last_mut() {
            Some((_, end, w)) if *w == px && *end + 1 == col => *end = col,
            _ => runs.push((col, col, px)),
        }
    }
    let mut xml = String::from("<cols>");
    for (start, end, px) in runs {
        xml.push_str(&format!(
            "<col min=\"{}\" max=\"{}\" width=\"{}\" customWidth=\"1\"/>",
            start + 1,
            end + 1,
            px_to_xlsx_width(px)
        ));
    }
    xml.push_str("</cols>");
    xml
}

fn truthy(attrs: &str, name: &str, default: bool) -> bool {
    match extract_attr_from_fragment(attrs, name) {
        None => default,
        Some("0" | "false") => false,
        Some(_) => true,
    }
}

fn parse_font(body: &str) -> CellStyle {
    let flag = |tag: &str| {
        extract_tags_or_self_closing_with_attrs(body, tag)
            .first()
            .is_some_and(|(attrs, _)| truthy(attrs, "val", true))
    };
    let underline = extract_tags_or_self_closing_with_attrs(body, "u")
        .first()
        .is_some_and(|(attrs, _)| extract_attr_from_fragment(attrs, "val") != Some("none"));
    let size_pt = extract_tags_or_self_closing_with_attrs(body, "sz")
        .first()
        .and_then(|(attrs, _)| extract_attr_from_fragment(attrs, "val"))
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|s| s.is_finite());
    CellStyle {
        bold: flag("b"),
        italic: flag("i"),
        underline,
        strike: flag("strike"),
        size_pt,
        ..CellStyle::default()
    }
}

/// One edge of a `border` element: the style word and the color of the side `tag`.
fn parse_border_edge(body: &str, tag: &str) -> BorderEdge {
    let Some((attrs, inner)) = extract_tags_or_self_closing_with_attrs(body, tag)
        .into_iter()
        .next()
    else {
        return BorderEdge::NONE;
    };
    let line =
        extract_attr_from_fragment(attrs, "style").map_or(BorderLine::None, BorderLine::from_xlsx);
    let rgb = extract_tags_or_self_closing_with_attrs(inner, "color")
        .first()
        .map_or(0, |(color, _)| xlsx_color_rgb(color));
    BorderEdge::new(line, rgb)
}

/// The edge sets of the `borders` element, in file order (at most [`MAX_BORDER_DEFS`]).
fn parse_borders(styles_xml: &str) -> Vec<CellBorders> {
    let Some(body) = extract_tag_bodies(styles_xml, "borders").into_iter().next() else {
        return Vec::new();
    };
    extract_tags_or_self_closing_with_attrs(body, "border")
        .into_iter()
        .take(MAX_BORDER_DEFS)
        .map(|(_, border)| CellBorders {
            top: parse_border_edge(border, "top"),
            right: parse_border_edge(border, "right"),
            bottom: parse_border_edge(border, "bottom"),
            left: parse_border_edge(border, "left"),
        })
        .collect()
}

/// What `xl/styles.xml` says about cell formats: the style of every `cellXfs` entry and the
/// workbook's custom number format codes (PD-193).
#[derive(Default)]
pub(crate) struct XfStyles {
    pub(crate) styles: Vec<CellStyle>,
    /// Per `cellXfs` entry: it points at the "Hyperlink" or "Followed Hyperlink" cell style (PD-223).
    pub(crate) link_xfs: Vec<bool>,
    pub(crate) num_fmts: Arc<Vec<String>>,
    pub(crate) rejected: u32,
    /// The colours of the differential formats, for the colour filters (PD-226). Set by the caller
    /// that has the theme.
    pub(crate) dxf_colors: Vec<crate::sheet_styling::xlsx_filter_dxf::DxfColors>,
}

/// `numFmtId` -> `formatCode` of the `numFmts` element.
fn read_num_fmt_codes(styles_xml: &str) -> HashMap<u32, String> {
    let Some(body) = extract_tag_bodies(styles_xml, "numFmts").into_iter().next() else {
        return HashMap::new();
    };
    extract_tags_or_self_closing_with_attrs(body, "numFmt")
        .into_iter()
        .filter_map(|(attrs, _)| {
            let id = extract_attr_from_fragment(attrs, "numFmtId")?
                .parse()
                .ok()?;
            let code = extract_attr_from_fragment(attrs, "formatCode")?;
            Some((id, XmlSanitizer::unescape(code)))
        })
        .collect()
}

/// The model id of one `numFmtId`: a defined custom code wins over a built-in id, an id nothing
/// defines is General (and counted when it was not 0).
fn model_num_fmt(id: u32, codes: &HashMap<u32, String>, table: &mut NumFmtTable) -> u16 {
    if let Some(code) = codes.get(&id) {
        return table.intern(code);
    }
    match u16::try_from(id) {
        Ok(0) => 0,
        Ok(id) if id < CUSTOM_BASE && crate::sheet_styling::is_known_builtin(id) => id,
        _ => {
            table.reject();
            0
        }
    }
}

/// The `cellStyleXfs` indexes of the "Hyperlink" and "Followed Hyperlink" cell styles (`builtinId` 8 and 9)
/// that `cellStyles` names, whatever the style is called.
/// At most [`MAX_CELL_STYLE_ENTRIES`] entries are read; a style past that is read as an ordinary one.
fn hyperlink_style_xfs(styles_xml: &str) -> HashSet<u32> {
    let Some(body) = extract_tag_bodies(styles_xml, "cellStyles")
        .into_iter()
        .next()
    else {
        return HashSet::new();
    };
    extract_tags_or_self_closing_with_attrs(body, "cellStyle")
        .into_iter()
        .take(MAX_CELL_STYLE_ENTRIES)
        .filter(|(attrs, _)| {
            matches!(
                extract_attr_from_fragment(attrs, "builtinId"),
                Some(BUILTIN_HYPERLINK | BUILTIN_FOLLOWED_HYPERLINK)
            )
        })
        .filter_map(|(attrs, _)| extract_attr_from_fragment(attrs, "xfId")?.parse().ok())
        .collect()
}

/// Style of every `cellXfs` entry, resolved against `fonts` and `numFmts`. A font size equal to
/// the default font's (font 0) reads as "no explicit size".
pub(crate) fn parse_cell_xf_styles(styles_xml: &str) -> XfStyles {
    let Some(fonts_xml) = extract_tag_bodies(styles_xml, "fonts").into_iter().next() else {
        return XfStyles::default();
    };
    let fonts: Vec<CellStyle> = extract_tag_bodies(fonts_xml, "font")
        .into_iter()
        .map(parse_font)
        .collect();
    let default_size = fonts
        .first()
        .and_then(|f| f.size_pt)
        .unwrap_or(DEFAULT_SIZE_PT);
    let Some(xfs_xml) = extract_tag_bodies(styles_xml, "cellXfs").into_iter().next() else {
        return XfStyles::default();
    };
    let codes = read_num_fmt_codes(styles_xml);
    let borders = parse_borders(styles_xml);
    let link_styles = hyperlink_style_xfs(styles_xml);
    let mut table = NumFmtTable::default();
    let mut link_xfs = Vec::new();
    let styles = extract_tags_or_self_closing_with_attrs(xfs_xml, "xf")
        .into_iter()
        .map(|(attrs, body)| {
            let number = |name: &str| {
                extract_attr_from_fragment(attrs, name).and_then(|v| v.parse::<u32>().ok())
            };
            let font_id = number("fontId").unwrap_or(0) as usize;
            let mut style = fonts.get(font_id).copied().unwrap_or_default();
            link_xfs.push(number("xfId").is_some_and(|id| link_styles.contains(&id)));
            if style
                .size_pt
                .is_some_and(|s| (s - default_size).abs() < 0.005)
            {
                style.size_pt = None;
            }
            style.num_fmt = model_num_fmt(number("numFmtId").unwrap_or(0), &codes, &mut table);
            let border_id = number("borderId").unwrap_or(0) as usize;
            style.borders = borders.get(border_id).copied().unwrap_or_default();
            if let Some((look, _)) = extract_tags_or_self_closing_with_attrs(body, "alignment")
                .into_iter()
                .next()
            {
                let word = |name| extract_attr_from_fragment(look, name).unwrap_or("");
                style.h_align = HAlign::from_xlsx(word("horizontal"));
                style.v_align = VAlign::from_xlsx(word("vertical"));
                style.wrap = truthy(look, "wrapText", false);
            }
            if let Some((lock, _)) = extract_tags_or_self_closing_with_attrs(body, "protection")
                .into_iter()
                .next()
            {
                let locked = truthy(lock, "locked", true);
                style.unlocked = !locked;
                style.force_locked = locked && extract_attr_from_fragment(lock, "locked").is_some();
                style.formula_hidden = truthy(lock, "hidden", false);
            }
            style
        })
        .collect();
    let (num_fmts, rejected) = table.finish();
    XfStyles {
        styles,
        link_xfs,
        num_fmts,
        rejected,
        dxf_colors: Vec::new(),
    }
}

/// Takes the underline off the cells of `link_styled` (cells whose format points at the "Hyperlink" cell style)
/// that have a link: the style gives them that underline because of the link and not as a look of their own
/// (PD-223). A cell with the style and no link keeps its underline.
pub(crate) fn strip_link_look(styling: &mut SheetStyling, link_styled: &[(u32, u32)]) {
    for at in link_styled {
        if !styling.links.contains_key(at) {
            continue;
        }
        if let Some(style) = styling.cells.get_mut(at) {
            style.underline = false;
            if style.is_plain() {
                styling.cells.remove(at);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{OdfCodec, OoxmlCodec, OoxmlXlsxPayload, ZipArchiveCodec, ZipSecurityPolicy};

    #[test]
    fn xlsx_escapes_inline_str_iso_date_and_preserve_space() {
        let payload = OoxmlXlsxPayload {
            sheet_name: "Sheet1".to_string(),
            cells: vec![
                (0, 0, "  leading and trailing  ".to_string()),
                (0, 1, "line1\nline2".to_string()),
            ],
        };
        let packed = OoxmlCodec::pack_xlsx(&payload).unwrap();
        let entries = ZipArchiveCodec::unpack(&packed, &ZipSecurityPolicy::default()).unwrap();
        let sst_entry = entries
            .iter()
            .find(|e| e.path == "xl/sharedStrings.xml")
            .unwrap();
        let sst_xml = std::str::from_utf8(&sst_entry.data).unwrap();
        assert!(sst_xml.contains("<t xml:space=\"preserve\">  leading and trailing  </t>"));
        assert!(sst_xml.contains("<t xml:space=\"preserve\">line1\nline2</t>"));
        let unpacked = OoxmlCodec::unpack_xlsx(&packed).unwrap();
        assert_eq!(unpacked.cells, payload.cells);

        let custom_entries: Vec<(String, Vec<u8>)> = entries
            .into_iter()
            .map(|e| {
                if e.path == "xl/sharedStrings.xml" {
                    (
                        e.path,
                        b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" count=\"2\" uniqueCount=\"2\"><si><t>Hello_x000A_World</t></si><si><t>_xD83D__xDE00_</t></si></sst>".to_vec(),
                    )
                } else if e.path == "xl/worksheets/sheet1.xml" {
                    (
                        e.path,
                        b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData><row r=\"1\"><c r=\"A1\" t=\"s\"><v>0</v></c><c r=\"B1\" t=\"s\"><v>1</v></c><c r=\"C1\"><is><t>Inline_x0020_NoType</t></is></c><c r=\"D1\" t=\"d\"><v> 2026-11-10 </v></c></row></sheetData></worksheet>".to_vec(),
                    )
                } else {
                    (e.path, e.data)
                }
            })
            .collect();
        let custom_xlsx = ZipArchiveCodec::pack_compressed(&custom_entries).unwrap();
        let parsed = OoxmlCodec::unpack_xlsx(&custom_xlsx).unwrap();
        assert_eq!(
            parsed.cells,
            vec![
                (0, 0, "Hello\nWorld".to_string()),
                (0, 1, "😀".to_string()),
                (0, 2, "Inline NoType".to_string()),
                (0, 3, "2026-11-10".to_string()),
            ]
        );
    }

    #[test]
    fn ods_boolean_and_date_value_fallback_when_text_p_is_empty() {
        let base_ods = OdfCodec::pack_ods("Sheet1", &[(0, 0, "1".to_string())]).unwrap();
        let entries = ZipArchiveCodec::unpack(&base_ods, &ZipSecurityPolicy::default()).unwrap();
        let custom_entries: Vec<(String, Vec<u8>)> = entries
            .into_iter()
            .map(|e| {
                if e.path == "content.xml" {
                    (
                        e.path,
                        concat!(
                            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
                            "<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
                            "xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" ",
                            "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\">",
                            "<office:body><office:spreadsheet><table:table table:name=\"Sheet1\">",
                            "<table:table-row>",
                            "<table:table-cell office:value-type=\"boolean\" office:boolean-value=\"true\"><text:p></text:p></table:table-cell>",
                            "<table:table-cell office:value-type=\"boolean\" office:boolean-value=\"false\"/>",
                            "<table:table-cell office:value-type=\"date\" office:date-value=\"2026-11-10\"><text:p></text:p></table:table-cell>",
                            "</table:table-row>",
                            "</table:table></office:spreadsheet></office:body></office:document-content>"
                        )
                        .as_bytes()
                        .to_vec(),
                    )
                } else {
                    (e.path, e.data)
                }
            })
            .collect();
        let custom_ods = ZipArchiveCodec::pack_compressed(&custom_entries).unwrap();
        let (_, cells) = OdfCodec::unpack_ods(&custom_ods).unwrap();
        assert_eq!(
            cells,
            vec![
                (0, 0, "TRUE".to_string()),
                (0, 1, "FALSE".to_string()),
                (0, 2, "2026-11-10".to_string()),
            ]
        );
    }
}

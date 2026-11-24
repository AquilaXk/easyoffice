//! ODS default styles (`<style:default-style style:family="table-cell" | "table-row">`), default
//! column widths, and O(runs) column-width band settling (PD-308, PD-345).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use crate::rtf_xml_helpers::{
    extract_attr_from_fragment, extract_tags_or_self_closing_with_attrs_limit,
};
use crate::sheet_styling::ods_styles::OdsStyleTable;
use crate::sheet_styling::ods_view::GroupWriter;
use crate::sheet_styling::sheet_look::parse_hex_rgb;
use crate::sheet_styling::sheet_row_height::{
    parse_ods_row_height_attr, read_ods_row_properties_attrs, RowHeight,
};
use crate::sheet_styling::xlsx_normal_font::written_font_pair;
use crate::sheet_styling::{
    CellStyle, DefaultColWidth, DefaultFont, FontTable, SheetStyling, MAX_COLUMN_PX,
    MAX_STYLED_CELLS, MIN_COLUMN_PX, STANDARD_DIGIT_PX,
};
use crate::xml_stream::{unescape_cow, Attrs, XmlEvent, XmlReader};
use crate::{EasyOfficeError, OdfCodec, XmlSanitizer};

pub(crate) const PX_PER_INCH: f64 = 96.0;
const CM_PER_INCH: f64 = 2.54;
const MM_PER_INCH: f64 = 25.4;
pub(crate) const PT_PER_INCH: f64 = 72.0;

const MAX_DEFAULT_STYLE_TAGS: usize = 64;
const MAX_FONT_SIZE_TEXT: usize = 32;
/// Maximum XML nesting depth accepted while scanning default styles in `styles.xml` or the pre-body header of `content.xml`.
pub(crate) const MAX_STYLE_XML_DEPTH: usize = 128;
/// Minimum repeated column stretch at the end of a sheet treated as a default column-width band.
const MIN_COL_BAND_COLS: u32 = 64;
/// Minimum sheet column count for a trailing repeated stretch to qualify as a sheet-wide band.
const MIN_COL_BAND_END: u32 = 1024;

/// Repeated columns with an equal width in pixels (`first..first + cols`), coalesced in O(runs).
pub(crate) struct ColWidthRun {
    pub(crate) first: u32,
    pub(crate) cols: u32,
    pub(crate) px: u32,
}

/// Converts an ODF length (`"2.5cm"`, `"14pt"`, `"72px"`) to CSS pixels at 96 dpi.
pub(crate) fn length_to_px(value: &str) -> Option<f64> {
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

/// Converts an ODF length (`"11pt"`, `"0.15in"`) to typographic points (72 pt/in).
pub(crate) fn length_to_pt(value: &str) -> Option<f64> {
    length_to_px(value).map(|px| px / PX_PER_INCH * PT_PER_INCH)
}

/// `#rrggbb` of an `fo:color` or `fo:background-color`; `transparent` and anything else is no colour.
pub(crate) fn ods_color(value: &str) -> Option<u32> {
    parse_hex_rgb(value.strip_prefix('#')?)
}

/// First font family token from an already XML-unescaped CSS/ODF `fo:font-family` value.
fn first_family_token(unescaped: &str) -> Option<String> {
    let first = unescaped.split(',').next().unwrap_or(unescaped).trim();
    (!first.is_empty()).then(|| first.to_string())
}

/// First unescaped CSS font family name from an ODF `fo:font-family` or fallback font attribute.
fn first_ods_font_family(props: &str, include_asian: bool) -> Option<String> {
    let raw = extract_attr_from_fragment(props, "fo:font-family")
        .or_else(|| extract_attr_from_fragment(props, "style:font-name"))
        .or_else(|| {
            include_asian
                .then(|| {
                    extract_attr_from_fragment(props, "style:font-family-asian")
                        .or_else(|| extract_attr_from_fragment(props, "fo:font-family-asian"))
                        .or_else(|| extract_attr_from_fragment(props, "style:font-name-asian"))
                })
                .flatten()
        })
        .map(XmlSanitizer::unescape)?;
    first_family_token(&raw)
}

/// Parses `<style:text-properties>` of a cell automatic style into a [`CellStyle`].
pub(crate) fn parse_text_properties(attrs: &str, font_table: &mut FontTable) -> CellStyle {
    let get = |name: &str| extract_attr_from_fragment(attrs, name);
    let on = |name: &str| get(name).is_some_and(|v| v != "none");
    let font = first_ods_font_family(attrs, false)
        .map(|first| font_table.intern(&first))
        .unwrap_or(0);
    CellStyle {
        bold: get("fo:font-weight")
            .is_some_and(|w| w == "bold" || w.parse::<u32>().is_ok_and(|n| n >= 600)),
        italic: get("fo:font-style").is_some_and(|s| s == "italic" || s == "oblique"),
        underline: on("style:text-underline-style"),
        strike: on("style:text-line-through-style"),
        size_pt: get("fo:font-size")
            .and_then(length_to_pt)
            .map(|pt| (pt * 100.0).round() / 100.0)
            .filter(|pt| *pt >= 1.0),
        // `use-window-font-color` is the automatic colour whatever `fo:color` says.
        font_color: get("fo:color")
            .and_then(ods_color)
            .filter(|_| get("style:use-window-font-color") != Some("true")),
        font,
        ..CellStyle::default()
    }
}

/// Quotes an XML-escaped CSS/ODF font family name when it contains whitespace.
pub(crate) fn quote_ods_font_family(name: &str) -> String {
    let esc = XmlSanitizer::escape(name);
    if name.contains(' ') {
        format!("'{esc}'")
    } else {
        esc
    }
}

/// `<style:default-style style:family="table-cell">` emitted inside `<office:styles>` of `styles.xml`.
pub(crate) fn default_style_xml(font: Option<&DefaultFont>) -> String {
    let (name, size) = written_font_pair(font);
    let quoted = quote_ods_font_family(&name);
    format!(
        "<style:default-style style:family=\"table-cell\"><style:text-properties fo:font-family=\"{quoted}\" style:font-family-asian=\"{quoted}\" fo:font-size=\"{size}pt\" style:font-size-asian=\"{size}pt\"/></style:default-style>"
    )
}

/// `<style:default-style style:family="table-row">` emitted inside `<office:styles>` of `styles.xml` (PD-345).
pub(crate) fn default_row_style_xml(height: RowHeight) -> String {
    format!(
        "<style:default-style style:family=\"table-row\"><style:table-row-properties style:row-height=\"{}pt\" style:use-optimal-row-height=\"{}\"/></style:default-style>",
        height.points_text(),
        !height.custom
    )
}

/// Selects the workbook-wide automatic (`custom == false`) default row height emitted in `styles.xml`,
/// returning `None` if any active non-custom sheet still expects `None` (including sheets beyond `stylings.len()`).
pub(crate) fn workbook_default_row_height(
    stylings: &[SheetStyling],
    sheet_count: usize,
) -> Option<RowHeight> {
    if stylings.len() < sheet_count {
        return None;
    }
    let active = &stylings[..sheet_count];
    let mut best: Option<(RowHeight, usize)> = None;
    for (idx, sheet) in active.iter().enumerate() {
        let height = sheet.default_row_height?;
        if height.custom {
            continue;
        }
        if best.is_some_and(|(chosen, _)| chosen == height) {
            continue;
        }
        let count = active[idx..]
            .iter()
            .filter(|s| s.default_row_height == Some(height))
            .count();
        if best.is_none_or(|(_, best_count)| count > best_count) {
            best = Some((height, count));
        }
    }
    best.map(|(height, _)| height)
}

/// `eo:default-row-height` attribute emitted on `<table:table>` when a sheet's automatic (`custom == false`)
/// default row height differs from the workbook `<style:default-style style:family="table-row">` default (PD-345).
pub(crate) fn table_row_height_attr(
    sheet_default: Option<RowHeight>,
    wb_default: Option<RowHeight>,
) -> String {
    match sheet_default {
        Some(height) if !height.custom && Some(height) != wb_default => {
            format!(" eo:default-row-height=\"{}pt\"", height.points_text())
        }
        _ => String::new(),
    }
}

/// Parses `eo:default-row-height` from `<table:table>` attributes as a non-custom (`custom == false`) height.
pub(crate) fn parse_table_row_height(attrs: &Attrs<'_>) -> Option<RowHeight> {
    let raw = attrs
        .iter_raw()
        .find(|(name, _)| *name == "eo:default-row-height")
        .map(|(_, value)| value)?;
    parse_ods_row_height_attr(raw, false, length_to_pt)
}

/// Workbook-level defaults decoded from `<style:default-style>` entries in `styles.xml` / `content.xml`.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct OdsDefaultStyles {
    pub(crate) font: Option<DefaultFont>,
    pub(crate) row_height: Option<RowHeight>,
}

/// Reads workbook `<style:default-style>` entries (`table-cell` font and `table-row` height) in a single
/// bounded pass per XML stream (`styles.xml` first, `content.xml` pre-body fallback for missing families).
pub(crate) fn read_default_styles(
    styles_xml: Option<&str>,
    content_xml: &str,
) -> Result<OdsDefaultStyles, EasyOfficeError> {
    let from_styles = match styles_xml {
        Some(xml) => find_default_styles_in_xml(xml, false)?,
        None => FoundDefaultStyles::default(),
    };
    if (from_styles.font.is_some() && from_styles.row_height.is_some())
        || (!content_xml.contains("<office:styles") && !content_xml.contains("<!DOCTYPE"))
    {
        return Ok(OdsDefaultStyles {
            font: from_styles.font.flatten(),
            row_height: from_styles.row_height.flatten(),
        });
    }
    let from_content = find_default_styles_in_xml(content_xml, true)?;
    Ok(OdsDefaultStyles {
        font: from_styles.font.or(from_content.font).flatten(),
        row_height: from_styles.row_height.or(from_content.row_height).flatten(),
    })
}

#[derive(Default)]
struct FoundDefaultStyles {
    font: Option<Option<DefaultFont>>,
    row_height: Option<Option<RowHeight>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DefaultTarget {
    TableCell,
    TableRow,
    Other,
}

fn raw_default_style_family<'a>(attrs: &Attrs<'a>) -> Option<std::borrow::Cow<'a, str>> {
    const MAX_FAMILY_RAW_BYTES: usize = 64;
    attrs
        .iter_raw()
        .find(|(name, _)| *name == "style:family")
        .map(|(_, value)| value)
        .filter(|raw| raw.len() <= MAX_FAMILY_RAW_BYTES)
        .map(unescape_cow)
}

fn find_default_styles_in_xml(
    xml: &str,
    stop_at_body: bool,
) -> Result<FoundDefaultStyles, EasyOfficeError> {
    let mut found = FoundDefaultStyles::default();
    let mut stack: Vec<&str> = Vec::with_capacity(16);
    let mut ignored_tag: Option<(&str, usize)> = None;
    let mut office_styles_idx: Option<usize> = None;
    let mut active_default: Option<(usize, DefaultTarget, bool)> = None;
    let mut default_style_count = 0usize;
    let mut reader = XmlReader::new(xml);

    for event in &mut reader {
        match event? {
            XmlEvent::Start { name, attrs } => {
                if stop_at_body && (name == "office:body" || name == "table:table") {
                    break;
                }
                if let Some((ign, ref mut nesting)) = ignored_tag {
                    if name == ign {
                        *nesting += 1;
                    }
                    continue;
                }
                if stack.len() >= MAX_STYLE_XML_DEPTH {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "ODS 스타일 XML 중첩 깊이가 허용 한도를 초과했습니다.".to_string(),
                    ));
                }
                if name == "office:automatic-styles" || name == "office:master-styles" {
                    ignored_tag = Some((name, 1));
                    continue;
                }
                stack.push(name);
                if name == "office:styles" && office_styles_idx.is_none() {
                    office_styles_idx = Some(stack.len() - 1);
                    continue;
                }
                if let Some(&mut (ds_idx, target, ref mut props_seen)) = active_default.as_mut() {
                    if stack.len() == ds_idx + 2 && !*props_seen {
                        match (target, name) {
                            (DefaultTarget::TableCell, "style:text-properties") => {
                                *props_seen = true;
                                found.font = Some(parse_default_text_properties_attrs(&attrs));
                            }
                            (DefaultTarget::TableRow, "style:table-row-properties") => {
                                *props_seen = true;
                                found.row_height =
                                    Some(read_ods_row_properties_attrs(&attrs, length_to_pt));
                            }
                            _ => {}
                        }
                    }
                    continue;
                }
                if name == "style:default-style"
                    && office_styles_idx.is_some_and(|s_idx| stack.len() == s_idx + 2)
                    && default_style_count < MAX_DEFAULT_STYLE_TAGS
                {
                    default_style_count += 1;
                    let ds_idx = stack.len() - 1;
                    match raw_default_style_family(&attrs).as_deref() {
                        Some("table-cell") if found.font.is_none() => {
                            found.font = Some(DefaultFont::kept(None, None));
                            active_default = Some((ds_idx, DefaultTarget::TableCell, false));
                        }
                        Some("table-row") if found.row_height.is_none() => {
                            found.row_height = Some(None);
                            active_default = Some((ds_idx, DefaultTarget::TableRow, false));
                        }
                        _ => {
                            active_default = Some((ds_idx, DefaultTarget::Other, false));
                        }
                    }
                }
            }
            XmlEvent::End(name) => {
                if let Some((ign, ref mut nesting)) = ignored_tag {
                    if name == ign {
                        *nesting -= 1;
                        if *nesting == 0 {
                            ignored_tag = None;
                        }
                    }
                    continue;
                }
                if let Some(pos) = stack.iter().rposition(|&open| open == name) {
                    stack.truncate(pos);
                    if active_default.is_some_and(|(ds_idx, _, _)| ds_idx >= pos) {
                        active_default = None;
                    }
                    if office_styles_idx.is_some_and(|s_idx| s_idx >= pos) {
                        office_styles_idx = None;
                    }
                }
            }
            XmlEvent::Text(_) => {}
        }
    }
    Ok(found)
}

fn parse_default_text_properties_attrs(attrs: &Attrs<'_>) -> Option<DefaultFont> {
    const MAX_FONT_NAME_RAW: usize = 2048;
    const MAX_RAW_FONT_SIZE: usize = MAX_FONT_SIZE_TEXT * 6;
    let mut best_family: Option<(u8, &str)> = None;
    let mut best_size: Option<(u8, &str)> = None;
    for (name, value) in attrs.iter_raw() {
        let fam_prio = match name {
            "fo:font-family" => Some(0u8),
            "style:font-name" => Some(1),
            "style:font-family-asian" => Some(2),
            "fo:font-family-asian" => Some(3),
            "style:font-name-asian" => Some(4),
            _ => None,
        };
        if let Some(prio) = fam_prio {
            if best_family.is_none_or(|(p, _)| prio < p) {
                best_family = Some((prio, value));
            }
            continue;
        }
        let size_prio = match name {
            "fo:font-size" => Some(0u8),
            "style:font-size-asian" => Some(1),
            _ => None,
        };
        if let Some(prio) = size_prio {
            if best_size.is_none_or(|(p, _)| prio < p) {
                best_size = Some((prio, value));
            }
        }
    }
    let first_face = best_family
        .map(|(_, raw)| raw)
        .filter(|raw| raw.len() <= MAX_FONT_NAME_RAW)
        .and_then(|raw| first_family_token(&unescape_cow(raw)));
    let size_pt = best_size
        .map(|(_, raw)| raw)
        .filter(|raw| raw.len() <= MAX_RAW_FONT_SIZE)
        .and_then(|raw| {
            let decoded = unescape_cow(raw);
            (decoded.len() <= MAX_FONT_SIZE_TEXT)
                .then(|| length_to_pt(&decoded))
                .flatten()
        });
    DefaultFont::kept(first_face.as_deref(), size_pt)
}

/// `eo:base-col-width` / `eo:default-col-width` attributes emitted on `<table:table>` when present.
pub(crate) fn table_col_width_attrs(dcw: Option<DefaultColWidth>) -> String {
    let Some(dcw) = dcw else {
        return String::new();
    };
    let mut xml = String::new();
    if let Some(base) = dcw.base {
        let _ = write!(xml, " eo:base-col-width=\"{base}\"");
    }
    if let Some(width) = dcw.width {
        let _ = write!(xml, " eo:default-col-width=\"{width}\"");
    }
    xml
}

/// Parses `eo:default-col-width` / `eo:base-col-width` from `<table:table>` attributes.
pub(crate) fn parse_table_col_width(attrs: &Attrs<'_>) -> Option<DefaultColWidth> {
    let width = attrs.get("eo:default-col-width");
    let base = attrs.get("eo:base-col-width");
    DefaultColWidth::parse(width.as_deref(), base.as_deref())
}

/// Emits `<style:style style:name="coN" style:family="table-column">` entries for `widths`.
pub(crate) fn append_column_width_styles_xml(widths: &[u32], xml: &mut String) {
    for (i, &px) in widths.iter().enumerate() {
        let _ = write!(
            xml,
            "<style:style style:name=\"co{}\" style:family=\"table-column\"><style:table-column-properties style:column-width=\"{:.4}in\"/></style:style>",
            i + 1,
            f64::from(px) / PX_PER_INCH
        );
    }
}

/// Parses a `table-column` automatic style's width in clamped CSS pixels.
pub(crate) fn parse_column_width_px(body: &str) -> Option<u32> {
    let px =
        extract_tags_or_self_closing_with_attrs_limit(body, "style:table-column-properties", 1)
            .into_iter()
            .next()
            .and_then(|(props, _)| extract_attr_from_fragment(props, "style:column-width"))
            .and_then(length_to_px)?;
    Some(
        px.round()
            .clamp(f64::from(MIN_COLUMN_PX), f64::from(MAX_COLUMN_PX)) as u32,
    )
}

/// Settles O(runs) column width stretches at `table_end`: detects a trailing default band
/// (`cols > 64 && first + cols >= 1024`), strips default-width columns, and recovers an implicit
/// `DefaultColWidth` from the band width when `eo:` attributes were absent.
pub(crate) fn settle_col_widths(
    runs: Vec<ColWidthRun>,
    eo_default: Option<DefaultColWidth>,
    default_font: Option<&DefaultFont>,
) -> (BTreeMap<u32, u32>, Option<DefaultColWidth>) {
    let digit_px = default_font.map_or(STANDARD_DIGIT_PX, DefaultFont::digit_px);
    let trailing_band = runs
        .last()
        .filter(|r| {
            r.cols > MIN_COL_BAND_COLS && r.first.saturating_add(r.cols) >= MIN_COL_BAND_END
        })
        .map(|r| r.px);
    let band_px =
        trailing_band.or_else(|| eo_default.map(|dcw| crate::default_col_px(Some(dcw), digit_px)));

    let default_col_width = eo_default.or_else(|| {
        let px = trailing_band?;
        if px == crate::default_col_px(None, digit_px) {
            None
        } else {
            let width = crate::sheet_styling::xlsx_styles::px_to_xlsx_width(px, digit_px);
            Some(DefaultColWidth {
                width: Some(width),
                base: None,
            })
        }
    });

    let mut columns = BTreeMap::new();
    for run in runs {
        if Some(run.px) == band_px {
            continue;
        }
        let end = run
            .first
            .saturating_add(run.cols)
            .min(OdfCodec::MAX_COLS + 1);
        for c in run.first..end {
            if columns.len() >= MAX_STYLED_CELLS {
                return (columns, default_col_width);
            }
            columns.insert(c, run.px);
        }
    }
    (columns, default_col_width)
}

/// `<table:table-column>` elements for custom widths, column groups, and (when set) the sheet's
/// default column-width band across `16,384` columns.
pub(crate) fn table_columns_xml(styling: &SheetStyling, table: &OdsStyleTable) -> String {
    let outline = &styling.outline;
    let default_style = styling
        .default_col_width
        .and_then(|_| table.column_style_name(styling.default_col_px()));
    let mut cuts: BTreeSet<u32> = BTreeSet::new();
    for &col in styling.columns.keys() {
        cuts.extend([col, col + 1]);
    }
    for &(first, last) in &styling.unlocked_cols {
        cuts.extend([first, last + 1]);
    }
    for &(first, last, _) in &outline.cols {
        cuts.extend([first, last + 1]);
    }
    for &(first, last) in &outline.collapsed_cols {
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
        &outline.open_cols,
    );
    let mut stretches: Vec<(u32, u32, Option<u32>, u8, bool, bool)> = Vec::new();
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
        let collapsed =
            crate::sheet_styling::xlsx_layout::run_contains(&outline.collapsed_cols, from);
        match stretches.last_mut() {
            Some(last)
                if last.1 == from
                    && last.2 == px
                    && last.3 == level
                    && last.4 == unlocked
                    && last.5 == collapsed
                    && header_end != Some(from) =>
            {
                last.1 = to
            }
            _ => stretches.push((from, to, px, level, unlocked, collapsed)),
        }
    }
    let mut xml = String::new();
    let mut next = 0u32;
    let unlocked_name = table.unlocked_name();
    for (from, to, px, _, unlocked, collapsed) in stretches {
        if from > next {
            groups.close_all(&mut xml);
            xml.push_str(&repeated_column(
                default_style.as_deref(),
                None,
                false,
                from - next,
            ));
        }
        if from == 0 && header_end.is_some() {
            xml.push_str("<table:table-header-columns>");
        }
        groups.move_to(&mut xml, from);
        let style_name = px
            .and_then(|px| table.column_style_name(px))
            .or_else(|| default_style.clone());
        xml.push_str(&repeated_column(
            style_name.as_deref(),
            unlocked_name.as_deref().filter(|_| unlocked),
            collapsed,
            to - from,
        ));
        next = to;
        if header_end == Some(to) {
            groups.close_all(&mut xml);
            xml.push_str("</table:table-header-columns>");
        }
    }
    groups.close_all(&mut xml);
    if let Some(ref style_name) = default_style {
        if next <= OdfCodec::MAX_COLS {
            xml.push_str(&repeated_column(
                Some(style_name.as_str()),
                None,
                false,
                OdfCodec::MAX_COLS + 1 - next,
            ));
        }
    }
    xml
}

fn repeated_column(
    style: Option<&str>,
    default_cell: Option<&str>,
    collapsed: bool,
    count: u32,
) -> String {
    let mut style = style.map_or(String::new(), |s| {
        format!(" table:style-name=\"{}\"", XmlSanitizer::escape(s))
    });
    if let Some(cell_style) = default_cell {
        style.push_str(&format!(
            " table:default-cell-style-name=\"{}\"",
            XmlSanitizer::escape(cell_style)
        ));
    }
    if collapsed {
        style.push_str(" table:visibility=\"collapse\"");
    }
    let repeat = if count > 1 {
        format!(" table:number-columns-repeated=\"{count}\"")
    } else {
        String::new()
    };
    format!("<table:table-column{style}{repeat}/>")
}

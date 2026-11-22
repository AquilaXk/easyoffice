//! The font of the Normal cell style of a workbook, which is the default font (PD-305, PD-306): which `fonts` entry it is when
//! `styles.xml` is read, and the name and size of the font 0 the writer says.

use crate::rtf_xml_helpers::{
    extract_attr_from_fragment, extract_tag_bodies, extract_tags_or_self_closing_with_attrs,
};
use crate::sheet_styling::xlsx_styles::{MAX_CELL_STYLE_ENTRIES, MAX_XF_DEFS};
use crate::sheet_styling::{format_centipt, SheetStyling};

/// The font id of the Normal cell style (`cellStyle` with `builtinId` 0, ECMA-376 18.8.7): the `fontId` of the `cellStyleXfs`
/// entry its `xfId` names. Font 0 when the file does not say, or says something that is not there.
pub(crate) fn normal_font_id(styles_xml: &str) -> usize {
    let attr = |attrs: &str, name: &str| {
        extract_attr_from_fragment(attrs, name)?
            .parse::<usize>()
            .ok()
    };
    let normal_xf = extract_tag_bodies(styles_xml, "cellStyles")
        .into_iter()
        .next()
        .and_then(|body| {
            extract_tags_or_self_closing_with_attrs(body, "cellStyle")
                .into_iter()
                .take(MAX_CELL_STYLE_ENTRIES)
                .find(|(attrs, _)| extract_attr_from_fragment(attrs, "builtinId") == Some("0"))
                .and_then(|(attrs, _)| attr(attrs, "xfId"))
        })
        .unwrap_or(0);
    extract_tag_bodies(styles_xml, "cellStyleXfs")
        .into_iter()
        .next()
        .and_then(|body| {
            extract_tags_or_self_closing_with_attrs(body, "xf")
                .into_iter()
                .take(MAX_XF_DEFS)
                .nth(normal_xf)
                .and_then(|(attrs, _)| attr(attrs, "fontId"))
        })
        .unwrap_or(0)
}

/// The name and the size text (points, no trailing zeros) of the font 0 a workbook writes: the first sheet's default font.
pub(crate) fn written_default_font(first: Option<&SheetStyling>) -> (String, String) {
    let default = SheetStyling::default();
    let styling = first.unwrap_or(&default);
    let centi = (styling.default_font_size_pt() * 100.0).round() as u32;
    (
        styling.default_font_name().to_string(),
        format_centipt(centi),
    )
}

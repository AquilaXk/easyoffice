//! SpreadsheetML cell styles and column widths (PD-089): `xl/styles.xml` writer and reader, the
//! `<cols>` element, and the per-cell `s` attribute.
//!
//! Writer layout: `fonts` holds the default font (index 0) plus one font per distinct style;
//! `cellXfs` holds the default format (index 0) plus one format per distinct font, so format `N`
//! uses font `N`. Elements follow the ECMA-376 `CT_Stylesheet` sequence (`fonts`, `fills`,
//! `borders`, `cellStyleXfs`, `cellXfs`, `cellStyles`); Excel rejects any other order.

use std::collections::{BTreeMap, HashMap};

use crate::rtf_xml_helpers::{
    extract_attr_from_fragment, extract_tag_bodies, extract_tags_or_self_closing_with_attrs,
};
use crate::sheet_styling::sheet_rules::xlsx_rules::DxfTable;
use crate::sheet_styling::{
    format_centipt, CellStyle, SheetStyling, StyleKey, MAX_COLUMN_PX, MIN_COLUMN_PX,
};

/// Width of the widest digit of the default font (Calibri 11) in CSS pixels. Excel's column width
/// unit is the number of such digits; the stored width includes the 5 px cell padding:
/// `width = trunc(256 * px / MDW) / 256`, hence `px = round(width * MDW)`.
pub(crate) const MAX_DIGIT_WIDTH_PX: f64 = 7.0;
const DEFAULT_SIZE_PT: f64 = 11.0;

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

/// Deduplicated styles of a whole workbook, in first-use order.
pub(crate) struct XlsxStyleTable {
    styles: Vec<StyleKey>,
    index: HashMap<StyleKey, usize>,
    /// Differential formats of the conditional formatting rules (PD-171).
    pub(crate) dxf: DxfTable,
}

impl XlsxStyleTable {
    pub(crate) fn build(stylings: &[SheetStyling]) -> Self {
        let mut table = Self {
            styles: Vec::new(),
            index: HashMap::new(),
            dxf: DxfTable::build(
                stylings
                    .iter()
                    .flat_map(|st| st.rules.cf.iter().map(|rule| rule.style)),
            ),
        };
        for styling in stylings {
            for style in styling.cells.values().filter(|s| !s.is_plain()) {
                let key = style.key();
                table.index.entry(key).or_insert_with(|| {
                    table.styles.push(key);
                    table.styles.len()
                });
            }
        }
        table
    }

    /// `cellXfs` index of `style`; 0 (the default format) for plain styles.
    pub(crate) fn xf_index(&self, style: &CellStyle) -> usize {
        if style.is_plain() {
            return 0;
        }
        self.index.get(&style.key()).copied().unwrap_or(0)
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
            let sz = k
                .size_centipt()
                .map_or_else(|| "11".to_string(), format_centipt);
            out.push_str(&format!("<sz val=\"{sz}\"/>"));
        } else {
            out.push_str("<sz val=\"11\"/>");
        }
        out.push_str("<name val=\"Calibri\"/></font>");
        out
    }

    pub(crate) fn to_xml(&self) -> String {
        let count = self.styles.len() + 1;
        let mut xml = String::from(concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">"
        ));
        xml.push_str(&format!("<fonts count=\"{count}\">"));
        xml.push_str(&Self::font_xml(None));
        for key in &self.styles {
            xml.push_str(&Self::font_xml(Some(*key)));
        }
        xml.push_str("</fonts>");
        xml.push_str(concat!(
            "<fills count=\"1\"><fill><patternFill patternType=\"none\"/></fill></fills>",
            "<borders count=\"1\"><border><left/><right/><top/><bottom/><diagonal/></border></borders>",
            "<cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>",
        ));
        xml.push_str(&format!("<cellXfs count=\"{count}\">"));
        xml.push_str("<xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/>");
        for font_id in 1..count {
            xml.push_str(&format!(
                "<xf numFmtId=\"0\" fontId=\"{font_id}\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyFont=\"1\"/>"
            ));
        }
        xml.push_str("</cellXfs>");
        xml.push_str(
            "<cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles>",
        );
        xml.push_str(&self.dxf.to_xml());
        xml.push_str("</styleSheet>");
        xml
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
    }
}

/// Style of every `cellXfs` entry, resolved against `fonts`. A font size equal to the default
/// font's (font 0) reads as "no explicit size".
pub(crate) fn parse_cell_xf_styles(styles_xml: &str) -> Vec<CellStyle> {
    let Some(fonts_xml) = extract_tag_bodies(styles_xml, "fonts").into_iter().next() else {
        return Vec::new();
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
        return Vec::new();
    };
    extract_tags_or_self_closing_with_attrs(xfs_xml, "xf")
        .into_iter()
        .map(|(attrs, _)| {
            let font_id = extract_attr_from_fragment(attrs, "fontId")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(0);
            let mut style = fonts.get(font_id).copied().unwrap_or_default();
            if style
                .size_pt
                .is_some_and(|s| (s - default_size).abs() < 0.005)
            {
                style.size_pt = None;
            }
            style
        })
        .collect()
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

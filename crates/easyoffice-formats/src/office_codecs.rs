use crate::decode_budget::DecodeBudget;
pub use crate::odf_hwpx_codecs::*;
pub use crate::rtf_xml_helpers::*;
use crate::sheet_styling::sheet_rules::xlsx_rules::{
    parse_dxf_styles, parse_worksheet_rules, rules_xml,
};
use crate::sheet_styling::sheet_view::{level_at, Outline};
use crate::sheet_styling::xlsx_filter::autofilter_xml;
use crate::sheet_styling::xlsx_layout::{layout_cols_xml, merge_cells_xml};
use crate::sheet_styling::xlsx_sheet::{read_shared_strings, scan_worksheet, SharedStrings};
use crate::sheet_styling::xlsx_styles::{parse_cell_xf_styles, XfStyles, XlsxStyleTable};
use crate::sheet_styling::xlsx_view::{
    apply_tint, sheet_format_pr_xml, sheet_pr_xml, sheet_views_xml, theme_palette, DEFAULT_THEME,
};
use crate::sheet_styling::{hides_every_sheet, IdMemo, SheetStyling};
use crate::{EasyOfficeError, XmlSanitizer, ZipArchiveCodec, ZipSecurityPolicy};
use std::sync::Arc;

/// Payload for OOXML WordprocessingML (`.docx`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OoxmlDocxPayload {
    pub title: String,
    pub paragraphs: Vec<String>,
    pub tables: Vec<Vec<Vec<String>>>,
}

/// Payload for OOXML SpreadsheetML (`.xlsx`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OoxmlXlsxPayload {
    pub sheet_name: String,
    pub cells: Vec<(u32, u32, String)>,
}

/// Payload for OOXML PresentationML (`.pptx` and `.ppsx`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OoxmlPptxPayload {
    pub title: String,
    pub is_slideshow: bool,
    pub slides: Vec<(String, Vec<String>, String)>,
}

/// Computed result of a formula cell, written as the SpreadsheetML cached value (`<v>`).
#[derive(Debug, Clone, PartialEq)]
pub enum XlsxCachedValue {
    Number(f64),
    Text(String),
    Boolean(bool),
}

/// Per-sheet formula cache keyed by zero-based `(row, col)`.
pub type XlsxFormulaCache = std::collections::HashMap<(u32, u32), XlsxCachedValue>;

/// ECMA-376 OOXML container codec for `.docx`, `.xlsx`, `.pptx`, and `.ppsx`.
pub struct OoxmlCodec;

const XLSX_ROOT_RELS: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
    "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
    "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/>",
    "</Relationships>"
);

type SharedStringMap<'a> = std::collections::HashMap<&'a str, usize>;

fn build_content_types(sheet_count: usize) -> String {
    let mut content_types = String::from(concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">",
        "<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>",
        "<Default Extension=\"xml\" ContentType=\"application/xml\"/>",
        "<Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>",
    ));
    for idx in 1..=sheet_count {
        content_types.push_str(&format!(
            "<Override PartName=\"/xl/worksheets/sheet{idx}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"
        ));
    }
    content_types.push_str(concat!(
        "<Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>",
        "<Override PartName=\"/xl/sharedStrings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml\"/>",
        "</Types>"
    ));
    content_types
}

/// Which sheets `stylings` hides. A workbook shows at least one sheet, so when every sheet is
/// hidden the first one is shown; the page never sends that, a file made by another program might.
fn hidden_sheets(sheet_count: usize, stylings: &[SheetStyling]) -> Vec<bool> {
    let mut hidden: Vec<bool> = (0..sheet_count)
        .map(|i| stylings.get(i).is_some_and(|s| s.hidden))
        .collect();
    if hides_every_sheet(stylings, sheet_count) {
        hidden[0] = false;
    }
    hidden
}

/// `xl/workbook.xml` and `xl/_rels/workbook.xml.rels` contents. A hidden sheet is written as
/// `state="hidden"`, and the book opens on the first sheet that is shown (ECMA-376 18.2.19
/// `sheet@state`, 18.2.1 `bookViews`).
fn build_workbook_parts(
    active_sheets: &[OdsUnpacked],
    hidden: &[bool],
    stylings: &[SheetStyling],
) -> (String, String) {
    let first_shown = hidden.iter().position(|&h| !h).unwrap_or(0);
    let mut workbook_xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">",
    );
    if first_shown > 0 {
        workbook_xml.push_str(&format!(
            "<bookViews><workbookView activeTab=\"{first_shown}\"/></bookViews>"
        ));
    }
    workbook_xml.push_str("<sheets>");
    let mut wb_rels_xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
    );
    for (i, (s_name, _)) in active_sheets.iter().enumerate() {
        let num = i + 1;
        let clean_name = if s_name.is_empty() {
            format!("Sheet{num}")
        } else {
            s_name.clone()
        };
        let state = if hidden[i] { " state=\"hidden\"" } else { "" };
        workbook_xml.push_str(&format!(
            "<sheet name=\"{}\" sheetId=\"{num}\"{state} r:id=\"rId{num}\"/>",
            XmlSanitizer::escape(&clean_name)
        ));
        wb_rels_xml.push_str(&format!(
            "<Relationship Id=\"rId{num}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{num}.xml\"/>"
        ));
    }
    workbook_xml.push_str("</sheets>");
    let names: Vec<String> = active_sheets
        .iter()
        .enumerate()
        .map(|(i, (name, _))| {
            if name.is_empty() {
                format!("Sheet{}", i + 1)
            } else {
                name.clone()
            }
        })
        .collect();
    workbook_xml.push_str(&crate::sheet_styling::xlsx_page::print_names_xml(
        &names, stylings,
    ));
    workbook_xml.push_str("<calcPr calcId=\"191029\" fullCalcOnLoad=\"1\"/></workbook>");
    let styles_rid = active_sheets.len() + 1;
    let sst_rid = active_sheets.len() + 2;
    wb_rels_xml.push_str(&format!(
        "<Relationship Id=\"rId{styles_rid}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>"
    ));
    wb_rels_xml.push_str(&format!(
        "<Relationship Id=\"rId{sst_rid}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings\" Target=\"sharedStrings.xml\"/></Relationships>"
    ));
    (workbook_xml, wb_rels_xml)
}

/// Unique non-numeric, non-formula strings with their indices and the total occurrence count.
fn collect_shared_strings(
    active_sheets: &[OdsUnpacked],
) -> (Vec<&str>, SharedStringMap<'_>, usize) {
    let mut shared_strings: Vec<&str> = Vec::new();
    let mut sst_map = SharedStringMap::new();
    let mut total_str_count = 0usize;
    for (_, cells) in active_sheets {
        for (_, _, val) in cells {
            if val.starts_with('=') || crate::cell_numbers::numeric_cell_value(val).is_some() {
                continue;
            }
            total_str_count += 1;
            if !sst_map.contains_key(val.as_str()) {
                sst_map.insert(val.as_str(), shared_strings.len());
                shared_strings.push(val.as_str());
            }
        }
    }
    (shared_strings, sst_map, total_str_count)
}

fn build_shared_strings_xml(shared_strings: &[&str], total_str_count: usize) -> String {
    let mut sst_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" count=\"{total_str_count}\" uniqueCount=\"{}\">",
        shared_strings.len()
    );
    for s in shared_strings {
        if s.starts_with(char::is_whitespace)
            || s.ends_with(char::is_whitespace)
            || s.contains(['\n', '\r'])
        {
            sst_xml.push_str("<si><t xml:space=\"preserve\">");
        } else {
            sst_xml.push_str("<si><t>");
        }
        sst_xml.push_str(&XmlSanitizer::escape(s));
        sst_xml.push_str("</t></si>");
    }
    sst_xml.push_str("</sst>");
    sst_xml
}

/// Appends the XML of one cell (`raw` is the stored text: formula, number, string or blank).
fn push_cell_xml(
    sheet_xml: &mut String,
    col_ref: &str,
    row_num: u32,
    xf: usize,
    raw: &str,
    cached: Option<&XlsxCachedValue>,
    sst_map: &SharedStringMap<'_>,
) {
    use std::fmt::Write;
    let _ = write!(sheet_xml, "<c r=\"{col_ref}{row_num}\"");
    if xf > 0 {
        let _ = write!(sheet_xml, " s=\"{xf}\"");
    }
    if raw.is_empty() {
        sheet_xml.push_str("/>");
    } else if let Some(formula) = raw.strip_prefix('=') {
        let f = XmlSanitizer::escape(formula);
        match cached {
            Some(XlsxCachedValue::Number(n)) if n.is_finite() => {
                let _ = write!(sheet_xml, "><f>{f}</f><v>{n}</v></c>");
            }
            Some(XlsxCachedValue::Text(text)) => {
                let _ = write!(
                    sheet_xml,
                    " t=\"str\"><f>{f}</f><v>{}</v></c>",
                    XmlSanitizer::escape(text)
                );
            }
            Some(XlsxCachedValue::Boolean(b)) => {
                let _ = write!(sheet_xml, " t=\"b\"><f>{f}</f><v>{}</v></c>", u8::from(*b));
            }
            _ => {
                let _ = write!(sheet_xml, "><f>{f}</f></c>");
            }
        }
    } else if crate::cell_numbers::numeric_cell_value(raw).is_some() {
        let _ = write!(
            sheet_xml,
            " t=\"n\"><v>{}</v></c>",
            XmlSanitizer::escape(raw)
        );
    } else {
        let idx = sst_map.get(raw).copied().unwrap_or(0);
        let _ = write!(sheet_xml, " t=\"s\"><v>{idx}</v></c>");
    }
}

/// Cells grouped by row; styled blank cells are included so their style is preserved.
fn group_cells_by_row<'a>(
    cells: &'a [(u32, u32, String)],
    styling: Option<&SheetStyling>,
    blank: &'a String,
) -> std::collections::BTreeMap<u32, Vec<(u32, &'a String)>> {
    let mut rows_map: std::collections::BTreeMap<u32, Vec<(u32, &String)>> =
        std::collections::BTreeMap::new();
    for (row, col, raw) in cells {
        rows_map.entry(*row).or_default().push((*col, raw));
    }
    if let Some(st) = styling {
        let valued: std::collections::HashSet<(u32, u32)> =
            cells.iter().map(|(r, c, _)| (*r, *c)).collect();
        for (&(row, col), style) in &st.cells {
            if !style.is_plain() && !valued.contains(&(row, col)) {
                rows_map.entry(row).or_default().push((col, blank));
            }
        }
        let filtered = st.filter.iter().flat_map(|f| f.hidden.iter());
        let hidden = st.hidden_rows.iter().chain(&st.outline.collapsed_rows);
        for &(start, end) in hidden.chain(filtered) {
            for row in start..=end {
                rows_map.entry(row).or_default();
            }
        }
        for &(start, end, _) in &st.outline.rows {
            for row in start..=end {
                rows_map.entry(row).or_default();
            }
        }
        for row in summary_rows(st) {
            rows_map.entry(row).or_default();
        }
    }
    rows_map
}

/// The rows that carry `collapsed`: the summary of every group folded as a whole.
fn summary_rows(styling: &SheetStyling) -> Vec<u32> {
    let outline = &styling.outline;
    Outline::summary_marks(
        &outline.rows,
        &outline.collapsed_rows,
        outline.summary_above,
    )
}

fn build_sheet_xml(
    cells: &[(u32, u32, String)],
    cache: Option<&XlsxFormulaCache>,
    styling: Option<&SheetStyling>,
    style_table: &XlsxStyleTable,
    sst_map: &SharedStringMap<'_>,
) -> String {
    use std::fmt::Write;
    let blank = String::new();
    let rows_map = group_cells_by_row(cells, styling, &blank);
    let mut sheet_xml = String::with_capacity(cells.len().saturating_mul(64) + 256);
    sheet_xml.push_str(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">",
    );
    if let Some(st) = styling {
        sheet_xml.push_str(&sheet_pr_xml(st));
        sheet_xml.push_str(&sheet_views_xml(st.freeze));
        sheet_xml.push_str(&sheet_format_pr_xml(st));
        sheet_xml.push_str(&layout_cols_xml(st));
    }
    let marks = styling.map(summary_rows).unwrap_or_default();
    sheet_xml.push_str("<sheetData>");
    let mut col_buf = [0u8; 4];
    let mut ids = IdMemo::new();
    for (row, mut row_cells) in rows_map {
        row_cells.sort_by_key(|(col, _)| *col);
        let row_num = row.saturating_add(1);
        let hidden = styling.is_some_and(|st| st.row_hidden(row));
        let _ = write!(
            sheet_xml,
            "<row r=\"{row_num}\"{}",
            if hidden { " hidden=\"1\"" } else { "" }
        );
        if let Some(st) = styling {
            let level = level_at(&st.outline.rows, row);
            if level > 0 {
                let _ = write!(sheet_xml, " outlineLevel=\"{level}\"");
            }
            if marks.binary_search(&row).is_ok() {
                sheet_xml.push_str(" collapsed=\"1\"");
            }
        }
        sheet_xml.push('>');
        for (col, raw) in row_cells {
            let col_ref = write_col_letters(&mut col_buf, col as usize);
            let xf = styling.map_or(0, |st| {
                st.cells
                    .get(&(row, col))
                    .map_or(0, |style| style_table.xf_index(style, st, &mut ids))
            });
            let cached = cache.and_then(|c| c.get(&(row, col)));
            push_cell_xml(&mut sheet_xml, col_ref, row_num, xf, raw, cached, sst_map);
        }
        sheet_xml.push_str("</row>");
    }
    sheet_xml.push_str("</sheetData>");
    if let Some(st) = styling {
        if let Some(filter) = &st.filter {
            sheet_xml.push_str(&autofilter_xml(filter));
        }
        sheet_xml.push_str(&merge_cells_xml(&st.merges));
        sheet_xml.push_str(&rules_xml(&st.rules, &style_table.dxf));
        sheet_xml.push_str(&crate::sheet_styling::xlsx_page::page_tail_xml(&st.page));
    }
    sheet_xml.push_str("</worksheet>");
    sheet_xml
}

impl OoxmlCodec {
    pub const MAX_XLSX_ROWS: u32 = 1_048_576;
    pub const MAX_XLSX_COLS: u32 = 16_384;
    /// Worksheets one workbook may list. Spreadsheet programs limit them by memory only; the bound keeps a
    /// workbook part that lists millions of them from becoming millions of scans (PD-175).
    pub const MAX_XLSX_SHEETS: usize = 1_024;

    pub fn pack_docx(payload: &OoxmlDocxPayload) -> Result<Vec<u8>, EasyOfficeError> {
        let content_types = concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">",
            "<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>",
            "<Default Extension=\"xml\" ContentType=\"application/xml\"/>",
            "<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>",
            "<Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/>",
            "</Types>"
        );
        let rels = concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
            "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>",
            "<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/>",
            "</Relationships>"
        );
        let core_xml = crate::ooxml_shared_parts::core_properties_xml(&payload.title);

        let mut body = String::with_capacity(512);
        body.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
        body.push_str("<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>");

        for p in &payload.paragraphs {
            body.push_str("<w:p><w:r><w:t xml:space=\"preserve\">");
            body.push_str(&XmlSanitizer::escape(p));
            body.push_str("</w:t></w:r></w:p>");
        }

        for tbl in &payload.tables {
            body.push_str("<w:tbl>");
            crate::ooxml_shared_parts::push_docx_table_properties(&mut body, tbl);
            for row in tbl {
                body.push_str("<w:tr>");
                for cell in row {
                    body.push_str("<w:tc><w:p><w:r><w:t>");
                    body.push_str(&XmlSanitizer::escape(cell));
                    body.push_str("</w:t></w:r></w:p></w:tc>");
                }
                body.push_str("</w:tr>");
            }
            body.push_str("</w:tbl>");
        }

        body.push_str("<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/></w:sectPr>");
        body.push_str("</w:body></w:document>");

        XmlSanitizer::validate_safe_xml(&body)?;
        XmlSanitizer::validate_safe_xml(&core_xml)?;

        ZipArchiveCodec::pack_compressed(&[
            (
                "[Content_Types].xml".to_string(),
                content_types.as_bytes().to_vec(),
            ),
            ("_rels/.rels".to_string(), rels.as_bytes().to_vec()),
            ("word/document.xml".to_string(), body.into_bytes()),
            ("docProps/core.xml".to_string(), core_xml.into_bytes()),
        ])
    }

    pub fn unpack_docx(bytes: &[u8]) -> Result<OoxmlDocxPayload, EasyOfficeError> {
        let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())?;
        let parts = checked_zip_xml_entries(&entries)?;
        let xml = checked_part(&parts, "word/document.xml").ok_or_else(|| {
            EasyOfficeError::CorruptedContainer(
                "DOCX 필수 엔트리 'word/document.xml'이 누락되었습니다.".to_string(),
            )
        })?;

        let core_title = entries
            .iter()
            .find(|e| e.path == "docProps/core.xml")
            .and_then(|e| std::str::from_utf8(&e.data).ok())
            .and_then(|c_xml| extract_tag_bodies(c_xml, "dc:title").into_iter().next())
            .map(XmlSanitizer::unescape)
            .filter(|s| !s.is_empty());

        let title = extract_attr(xml, "<w:document", "title")
            .map(XmlSanitizer::unescape)
            .filter(|s| !s.is_empty())
            .or(core_title)
            .unwrap_or_else(|| "문서".to_string());

        let mut tables: Vec<Vec<Vec<String>>> = Vec::new();
        for tbl_inner in extract_tag_bodies(xml, "w:tbl") {
            let mut rows = Vec::new();
            for tr_inner in extract_tag_bodies(tbl_inner, "w:tr") {
                let mut cells = Vec::new();
                for tc_inner in extract_tag_bodies(tr_inner, "w:tc") {
                    let cell_text = extract_tag_bodies(tc_inner, "w:t")
                        .into_iter()
                        .map(XmlSanitizer::unescape)
                        .collect::<Vec<_>>()
                        .join("");
                    cells.push(cell_text);
                }
                rows.push(cells);
            }
            tables.push(rows);
        }

        let xml_without_tables = strip_tag_blocks(xml, "w:tbl");
        let mut paragraphs: Vec<String> = Vec::new();
        for (_, p_inner) in extract_balanced_elements(&xml_without_tables, "w:p") {
            let text = extract_tag_bodies(p_inner, "w:t")
                .into_iter()
                .map(XmlSanitizer::unescape)
                .collect::<Vec<_>>()
                .join("");
            paragraphs.push(text);
        }

        Ok(OoxmlDocxPayload {
            title,
            paragraphs,
            tables,
        })
    }

    pub fn pack_xlsx(payload: &OoxmlXlsxPayload) -> Result<Vec<u8>, EasyOfficeError> {
        Self::pack_xlsx_multi(&[(payload.sheet_name.clone(), payload.cells.clone())])
    }

    pub fn pack_xlsx_multi(sheets: &[OdsUnpacked]) -> Result<Vec<u8>, EasyOfficeError> {
        Self::pack_xlsx_multi_with_formula_cache(sheets, &[])
    }

    /// Packs sheets, writing `caches[i]` (zero-based `(row, col)` → computed value) as the cached
    /// `<v>` of formula cells in sheet `i`. Formula cells without a cached value carry no `<v>`;
    /// `workbook.xml` always sets `<calcPr fullCalcOnLoad="1"/>` so consumers recalculate on open.
    pub fn pack_xlsx_multi_with_formula_cache(
        sheets: &[OdsUnpacked],
        caches: &[XlsxFormulaCache],
    ) -> Result<Vec<u8>, EasyOfficeError> {
        Self::pack_xlsx_multi_styled(sheets, caches, &[])
    }

    /// Like [`Self::pack_xlsx_multi_with_formula_cache`], additionally writing `stylings[i]` (cell
    /// formatting and column widths, PD-089) for sheet `i`: deduplicated `fonts`/`cellXfs` in
    /// `xl/styles.xml`, the `s` attribute on styled cells (blank styled cells included), and
    /// `<cols>`.
    pub fn pack_xlsx_multi_styled(
        sheets: &[OdsUnpacked],
        caches: &[XlsxFormulaCache],
        stylings: &[SheetStyling],
    ) -> Result<Vec<u8>, EasyOfficeError> {
        let default_sheet = vec![("Sheet1".to_string(), Vec::new())];
        let active_sheets = if sheets.is_empty() {
            default_sheet.as_slice()
        } else {
            sheets
        };

        Self::check_sheet_count(active_sheets.len())?;
        Self::validate_xlsx_bounds(active_sheets, stylings)?;
        let style_table = XlsxStyleTable::build(stylings);
        let hidden = hidden_sheets(active_sheets.len(), stylings);
        let (workbook_xml, wb_rels_xml) = build_workbook_parts(active_sheets, &hidden, stylings);
        let styles_xml = style_table.to_xml();
        let (shared_strings, sst_map, total_str_count) = collect_shared_strings(active_sheets);
        let sst_xml = build_shared_strings_xml(&shared_strings, total_str_count);

        XmlSanitizer::validate_safe_xml(&workbook_xml)?;
        XmlSanitizer::validate_safe_xml(&wb_rels_xml)?;
        XmlSanitizer::validate_safe_xml(&sst_xml)?;

        let mut entries = vec![
            (
                "[Content_Types].xml".to_string(),
                build_content_types(active_sheets.len()).into_bytes(),
            ),
            ("xl/workbook.xml".to_string(), workbook_xml.into_bytes()),
            (
                "_rels/.rels".to_string(),
                XLSX_ROOT_RELS.as_bytes().to_vec(),
            ),
            (
                "xl/_rels/workbook.xml.rels".to_string(),
                wb_rels_xml.into_bytes(),
            ),
            ("xl/styles.xml".to_string(), styles_xml.into_bytes()),
            ("xl/sharedStrings.xml".to_string(), sst_xml.into_bytes()),
        ];

        for (i, (_, cells)) in active_sheets.iter().enumerate() {
            let sheet_xml = build_sheet_xml(
                cells,
                caches.get(i),
                stylings.get(i),
                &style_table,
                &sst_map,
            );
            XmlSanitizer::validate_safe_xml(&sheet_xml)?;
            entries.push((
                format!("xl/worksheets/sheet{}.xml", i + 1),
                sheet_xml.into_bytes(),
            ));
        }

        ZipArchiveCodec::pack_compressed(&entries)
    }

    /// Fails when a workbook lists more than [`Self::MAX_XLSX_SHEETS`] worksheets, so a file this
    /// codec writes is one it can read back.
    fn check_sheet_count(count: usize) -> Result<(), EasyOfficeError> {
        if count > Self::MAX_XLSX_SHEETS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "XLSX 워크시트 개수({count})가 허용 상한({})을 초과했습니다.",
                Self::MAX_XLSX_SHEETS
            )));
        }
        Ok(())
    }

    /// Rejects cell / style coordinates outside the ECMA-376 limits.
    fn validate_xlsx_bounds(
        active_sheets: &[OdsUnpacked],
        stylings: &[SheetStyling],
    ) -> Result<(), EasyOfficeError> {
        for (_, cells) in active_sheets {
            for &(row, col, _) in cells {
                if row >= Self::MAX_XLSX_ROWS || col >= Self::MAX_XLSX_COLS {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                        "XLSX 셀 좌표({row}, {col})가 ECMA-376 허용 범위({}x{})를 초과했습니다.",
                        Self::MAX_XLSX_ROWS,
                        Self::MAX_XLSX_COLS
                    )));
                }
            }
        }
        for styling in stylings {
            let cell = styling
                .cells
                .keys()
                .copied()
                .find(|&(r, c)| r >= Self::MAX_XLSX_ROWS || c >= Self::MAX_XLSX_COLS);
            let column = styling
                .columns
                .keys()
                .find(|&&c| c >= Self::MAX_XLSX_COLS)
                .map(|&c| (0, c));
            let layout = styling
                .merges
                .iter()
                .any(|m| m.row1 >= Self::MAX_XLSX_ROWS || m.col1 >= Self::MAX_XLSX_COLS)
                || styling
                    .hidden_rows
                    .iter()
                    .any(|r| r.1 >= Self::MAX_XLSX_ROWS)
                || styling
                    .hidden_cols
                    .iter()
                    .any(|c| c.1 >= Self::MAX_XLSX_COLS)
                || styling.merges.len() > crate::MAX_MERGES
                || crate::run_total(&styling.hidden_rows) > crate::MAX_HIDDEN_ROWS
                || styling.filter.as_ref().is_some_and(|f| {
                    !f.within_bounds()
                        || f.range.row1 >= Self::MAX_XLSX_ROWS
                        || f.range.col1 >= Self::MAX_XLSX_COLS
                });
            if layout {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "XLSX 병합·숨김 범위가 허용 범위를 초과했습니다.".to_string(),
                ));
            }
            if !styling.view_within_bounds(Self::MAX_XLSX_ROWS, Self::MAX_XLSX_COLS) {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "XLSX 틀 고정·탭 색·그룹 값이 허용 범위를 초과했습니다.".to_string(),
                ));
            }
            if let Some((row, col)) = cell.or(column) {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "XLSX 서식 좌표({row}, {col})가 ECMA-376 허용 범위({}x{})를 초과했습니다.",
                    Self::MAX_XLSX_ROWS,
                    Self::MAX_XLSX_COLS
                )));
            }
        }
        Ok(())
    }

    pub fn unpack_xlsx(bytes: &[u8]) -> Result<OoxmlXlsxPayload, EasyOfficeError> {
        let mut sheets = Self::unpack_xlsx_multi(bytes)?;
        let (sheet_name, cells) = if sheets.is_empty() {
            ("Sheet1".to_string(), Vec::new())
        } else {
            sheets.remove(0)
        };
        Ok(OoxmlXlsxPayload { sheet_name, cells })
    }

    pub fn unpack_xlsx_multi(bytes: &[u8]) -> Result<Vec<OdsUnpacked>, EasyOfficeError> {
        Ok(Self::unpack_xlsx_inner(bytes, false)?.0)
    }

    /// Cell formatting and column widths of every sheet, keyed by sheet name (PD-089).
    pub fn unpack_xlsx_styling(
        bytes: &[u8],
    ) -> Result<Vec<(String, SheetStyling)>, EasyOfficeError> {
        Ok(Self::unpack_xlsx_inner(bytes, true)?.1)
    }

    /// Unpacks all XLSX worksheets together with their cell and column styling in a single ZIP pass.
    #[allow(clippy::type_complexity)]
    pub fn unpack_xlsx_with_styling(
        bytes: &[u8],
    ) -> Result<(Vec<OdsUnpacked>, Vec<(String, SheetStyling)>), EasyOfficeError> {
        Self::unpack_xlsx_inner(bytes, true)
    }

    #[allow(clippy::type_complexity)]
    fn unpack_xlsx_inner(
        bytes: &[u8],
        with_styling: bool,
    ) -> Result<(Vec<OdsUnpacked>, Vec<(String, SheetStyling)>), EasyOfficeError> {
        let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())?;
        validate_zip_xml_entries(&entries)?;
        let entry_by_path: std::collections::HashMap<&str, &[u8]> = entries
            .iter()
            .map(|e| (e.path.as_str(), e.data.as_slice()))
            .collect();
        let wb_data = entry_by_path
            .get("xl/workbook.xml")
            .copied()
            .ok_or_else(|| {
                EasyOfficeError::CorruptedContainer(
                    "XLSX 필수 엔트리 'xl/workbook.xml'이 누락되었습니다.".to_string(),
                )
            })?;
        let wb_xml = std::str::from_utf8(wb_data)
            .map_err(|_| EasyOfficeError::InvalidUtf8("xl/workbook.xml UTF-8 오류".to_string()))?;
        XmlSanitizer::validate_safe_xml(wb_xml)?;

        let mut rel_targets: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        if let Some(&rels_data) = entry_by_path.get("xl/_rels/workbook.xml.rels") {
            let rels_xml = std::str::from_utf8(rels_data).map_err(|_| {
                EasyOfficeError::InvalidUtf8("xl/_rels/workbook.xml.rels UTF-8 오류".to_string())
            })?;
            XmlSanitizer::validate_safe_xml(rels_xml)?;
            for (r_attrs, _) in extract_balanced_elements(rels_xml, "Relationship") {
                if let (Some(id), Some(target)) = (
                    extract_attr_from_fragment(r_attrs, "Id"),
                    extract_attr_from_fragment(r_attrs, "Target"),
                ) {
                    let normalized = if let Some(stripped) = target.strip_prefix('/') {
                        stripped.to_string()
                    } else if target.starts_with("xl/") {
                        target.to_string()
                    } else {
                        format!("xl/{target}")
                    };
                    rel_targets.insert(id.to_string(), normalized);
                }
            }
        }

        let mut shared_strings = SharedStrings::default();
        if let Some(&sst_data) = entry_by_path.get("xl/sharedStrings.xml") {
            let sst_xml = std::str::from_utf8(sst_data).map_err(|_| {
                EasyOfficeError::InvalidUtf8("xl/sharedStrings.xml UTF-8 오류".to_string())
            })?;
            XmlSanitizer::validate_safe_xml(sst_xml)?;
            shared_strings = read_shared_strings(sst_xml)?;
        }

        let sheet_tags = extract_balanced_elements(wb_xml, "sheet");
        Self::check_sheet_count(sheet_tags.len())?;
        let mut sheet_specs: Vec<(String, String, bool)> = Vec::new();
        for (idx, (s_attrs, _)) in sheet_tags.into_iter().enumerate() {
            let name = extract_attr_from_fragment(s_attrs, "name")
                .map(XmlSanitizer::unescape)
                .unwrap_or_else(|| format!("Sheet{}", idx + 1));
            let target_path = extract_attr_from_fragment(s_attrs, "r:id")
                .or_else(|| extract_attr_from_fragment(s_attrs, "id"))
                .and_then(|rid| rel_targets.get(rid).cloned())
                .unwrap_or_else(|| format!("xl/worksheets/sheet{}.xml", idx + 1));
            // `veryHidden` is hidden here too: the page has one kind of hidden sheet.
            let hidden = matches!(
                extract_attr_from_fragment(s_attrs, "state"),
                Some("hidden" | "veryHidden")
            );
            sheet_specs.push((name, target_path, hidden));
        }
        if sheet_specs.is_empty() {
            sheet_specs.push((
                "Sheet1".to_string(),
                "xl/worksheets/sheet1.xml".to_string(),
                false,
            ));
        }
        // A workbook shows at least one sheet: a file that hides them all opens on its first.
        if sheet_specs.iter().all(|spec| spec.2) {
            sheet_specs[0].2 = false;
        }

        let xf_styles = if with_styling {
            entry_by_path
                .get("xl/styles.xml")
                .and_then(|d| std::str::from_utf8(d).ok())
                .map(parse_cell_xf_styles)
                .unwrap_or_default()
        } else {
            XfStyles::default()
        };
        let dxf_styles = if with_styling {
            entry_by_path
                .get("xl/styles.xml")
                .and_then(|d| std::str::from_utf8(d).ok())
                .map(parse_dxf_styles)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let theme = with_styling.then(|| {
            entry_by_path
                .get("xl/theme/theme1.xml")
                .and_then(|d| std::str::from_utf8(d).ok())
                .map_or(DEFAULT_THEME, theme_palette)
        });
        let mut stylings = Vec::new();
        let workbook_names: Vec<String> = sheet_specs.iter().map(|spec| spec.0.clone()).collect();
        let mut parsed_sheets = Vec::with_capacity(sheet_specs.len());
        let mut scanned_parts = std::collections::HashSet::new();
        let mut budget = DecodeBudget::new();
        for (idx, (sheet_name, target_path, hidden)) in sheet_specs.into_iter().enumerate() {
            let part = entry_by_path
                .get_key_value(target_path.as_str())
                .or_else(|| {
                    let fallback = format!("xl/worksheets/sheet{}.xml", idx + 1);
                    entry_by_path.get_key_value(fallback.as_str())
                });
            let Some((&part_path, &s_data)) = part else {
                if idx == 0 {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "XLSX 필수 엔트리 'xl/worksheets/sheet1.xml'이 누락되었습니다.".to_string(),
                    ));
                }
                continue;
            };
            // Each worksheet part is scanned once: naming one part from many `<sheet>` entries
            // would repeat the scan (and its copies) per entry.
            if !scanned_parts.insert(part_path) {
                return Err(EasyOfficeError::CorruptedContainer(format!(
                    "XLSX 워크시트 파트 '{part_path}'를 여러 시트가 가리킵니다."
                )));
            }
            let sheet_xml = std::str::from_utf8(s_data).map_err(|_| {
                EasyOfficeError::InvalidUtf8("xl/worksheets/sheet.xml UTF-8 오류".to_string())
            })?;
            XmlSanitizer::validate_safe_xml(sheet_xml)?;
            let scan = scan_worksheet(
                sheet_xml,
                &shared_strings,
                &xf_styles.styles,
                with_styling,
                &mut budget,
            )?;
            if with_styling {
                let mut styling = scan.styling;
                styling.num_fmts = Arc::clone(&xf_styles.num_fmts);
                styling.hidden = hidden;
                // The count belongs to the workbook; one sheet carries it.
                styling.num_fmts_rejected = if idx == 0 { xf_styles.rejected } else { 0 };
                styling.rules = parse_worksheet_rules(sheet_xml, &dxf_styles);
                if let (None, Some(tab), Some(theme)) = (styling.tab_color, scan.tab_theme, &theme)
                {
                    styling.tab_color = Some(apply_tint(theme[tab.index], tab.tint));
                }
                stylings.push((sheet_name.clone(), styling));
            }
            parsed_sheets.push((sheet_name, scan.cells));
        }

        if with_styling {
            crate::sheet_styling::xlsx_page::apply_print_names(
                wb_xml,
                &workbook_names,
                &mut stylings,
            );
        }
        Ok((parsed_sheets, stylings))
    }
}

/// The XML entries of a package as `(path, text)`, each UTF-8 checked and scanned for DOCTYPE and
/// entity declarations once. A reader that takes its parts from here does not check them again.
pub(crate) fn checked_zip_xml_entries(
    entries: &[crate::ZipArchiveEntry],
) -> Result<Vec<(&str, &str)>, EasyOfficeError> {
    let mut parts = Vec::new();
    for entry in entries {
        if entry.path.ends_with('/') {
            continue;
        }
        let lower = entry.path.to_ascii_lowercase();
        if lower.ends_with(".xml") || lower.ends_with(".rels") || lower.ends_with(".hpf") {
            let xml = std::str::from_utf8(&entry.data).map_err(|_| {
                EasyOfficeError::InvalidUtf8(format!("ZIP XML 엔트리 UTF-8 오류: {}", entry.path))
            })?;
            XmlSanitizer::validate_safe_xml(xml)?;
            parts.push((entry.path.as_str(), xml));
        }
    }
    Ok(parts)
}

pub(crate) fn validate_zip_xml_entries(
    entries: &[crate::ZipArchiveEntry],
) -> Result<(), EasyOfficeError> {
    checked_zip_xml_entries(entries).map(drop)
}

/// The text of `path` among `parts` from [`checked_zip_xml_entries`].
pub(crate) fn checked_part<'a>(parts: &[(&str, &'a str)], path: &str) -> Option<&'a str> {
    parts.iter().find(|(p, _)| *p == path).map(|(_, xml)| *xml)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(
        cell_xml: &str,
        shared: &[String],
    ) -> Result<Vec<(u32, u32, String)>, EasyOfficeError> {
        let sheet = format!("<sheetData><row r=\"1\">{cell_xml}</row></sheetData>");
        let table = SharedStrings::from_strings(shared);
        scan_worksheet(&sheet, &table, &[], false, &mut DecodeBudget::new()).map(|scan| scan.cells)
    }

    #[test]
    fn shared_string_index_resolves_and_corrupt_index_fails_closed() {
        let shared = vec!["가".to_string(), "나".to_string()];
        let ok = parse(r#"<c r="A1" t="s"><v>1</v></c>"#, &shared).unwrap();
        assert_eq!(ok, vec![(0, 0, "나".to_string())]);
        for bad in ["abc", "", "-1", "2", "99999999999999999999"] {
            let xml = format!(r#"<c r="A1" t="s"><v>{bad}</v></c>"#);
            assert!(
                matches!(
                    parse(&xml, &shared),
                    Err(EasyOfficeError::CorruptedContainer(_))
                ),
                "index {bad:?} must not fall back to another string"
            );
        }
    }
}

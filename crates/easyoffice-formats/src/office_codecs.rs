use crate::decode_budget::DecodeBudget;
pub use crate::odf_hwpx_codecs::*;
pub use crate::rtf_xml_helpers::*;
use crate::sheet_styling::sheet_links::xlsx_links::{links_xml, read_links, sheet_relationships};
use crate::sheet_styling::sheet_links::LinkAllowance;
use crate::sheet_styling::sheet_rules::xlsx_rules::{parse_dxf_styles, parse_worksheet_rules};
use crate::sheet_styling::sheet_table::xlsx_tables::{
    merge_relationships, read_sheet_tables, HeaderCells, TableWriter,
};
use crate::sheet_styling::table_style::parse_table_styles;
use crate::sheet_styling::xlsx_charts::{read_sheet_charts, ChartWriter};
use crate::sheet_styling::xlsx_color::{ColorScheme, XlsxColor};
use crate::sheet_styling::xlsx_filter_dxf::{dxf_bodies, parse_dxf_colors};
use crate::sheet_styling::xlsx_sheet::{read_shared_strings, scan_worksheet, SharedStrings};
use crate::sheet_styling::xlsx_styles::{parse_cell_xf_styles, XfStyles, XlsxStyleTable};
use crate::sheet_styling::xlsx_view::{apply_tint, theme_palette, DEFAULT_THEME};
use crate::sheet_styling::{
    merges_clear_of_tables, sheet_states, unique_table_names, SheetState, SheetStyling,
    TableRegistry,
};
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

/// `xl/workbook.xml` and `xl/_rels/workbook.xml.rels` contents. A hidden sheet is written as
/// `state="hidden"` or `state="veryHidden"`, and the book opens on the first sheet that is shown
/// (ECMA-376 18.2.19 `sheet@state`, 18.2.1 `bookViews`).
fn build_workbook_parts(
    active_sheets: &[OdsUnpacked],
    states: &[SheetState],
    stylings: &[SheetStyling],
) -> (String, String) {
    let first_shown = states
        .iter()
        .position(|&s| s == SheetState::Visible)
        .unwrap_or(0);
    let mut workbook_xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">",
    );
    if let Some(protect) = stylings.first().and_then(|st| st.workbook_protect.as_ref()) {
        workbook_xml
            .push_str(&crate::sheet_styling::xlsx_protect::workbook_protection_xml(protect));
    }
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
        let state = states[i]
            .xlsx()
            .map_or(String::new(), |text| format!(" state=\"{text}\""));
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
    workbook_xml.push_str(&crate::sheet_styling::xlsx_names::defined_names_xml(
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

/// Unique text-cell strings with their indices and the total occurrence count.
fn collect_shared_strings<'a>(
    active_sheets: &'a [OdsUnpacked],
    stylings: &[SheetStyling],
) -> (Vec<&'a str>, SharedStringMap<'a>, usize) {
    let mut shared_strings: Vec<&'a str> = Vec::new();
    let mut sst_map = SharedStringMap::new();
    let mut total_str_count = 0usize;
    for (sheet_idx, (_, cells)) in active_sheets.iter().enumerate() {
        let styling = stylings.get(sheet_idx);
        for &(row, col, ref val) in cells {
            if val.is_empty() {
                continue;
            }
            let kind = styling.map_or_else(
                || crate::CellValueKind::infer(val),
                |st| st.cell_kind(row, col, val),
            );
            let needs_sst = matches!(kind, crate::CellValueKind::Text)
                || (kind == crate::CellValueKind::Number
                    && crate::cell_numbers::numeric_cell_value(val).is_none()
                    && !val.trim().parse::<f64>().is_ok_and(f64::is_finite));
            if !needs_sst {
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

use crate::sheet_styling::xlsx_sheet_writer::build_sheet_xml;

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
        let mut budget = DecodeBudget::new();
        budget.charge(title.len(), "DOCX 제목")?;

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
                    budget.charge_cell(cell_text.len(), "DOCX 표 셀")?;
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
            budget.charge_block(text.len(), "DOCX 단락")?;
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
        // A workbook the reader would cut short is not written (a cell past the limit would lose its format).
        if !style_table.within_read_limits() {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "서로 다른 셀 서식이 65,535가지를 넘어 XLSX로 저장할 수 없습니다.".to_string(),
            ));
        }
        let states = sheet_states(active_sheets.len(), stylings);
        let (mut workbook_xml, wb_rels_xml) =
            build_workbook_parts(active_sheets, &states, stylings);
        let styles_xml = style_table.to_xml();
        let (shared_strings, sst_map, total_str_count) =
            collect_shared_strings(active_sheets, stylings);
        let sst_xml = build_shared_strings_xml(&shared_strings, total_str_count);
        XmlSanitizer::validate_safe_xml(&sst_xml)?;

        let sheet_tables = unique_table_names(
            (0..active_sheets.len()).map(|i| stylings.get(i).map_or(&[][..], |st| &st.tables[..])),
            stylings
                .first()
                .into_iter()
                .flat_map(|st| st.defined_names.iter().map(|n| n.name.as_str())),
        );
        // A structured reference is written the way the file format holds it (PD-225).
        let sheet_cells: Vec<_> = active_sheets
            .iter()
            .zip(&sheet_tables)
            .enumerate()
            .map(|(i, ((_, cells), tables))| {
                crate::sheet_styling::structured_cells::file_formulas(
                    cells,
                    tables,
                    stylings.get(i),
                )
            })
            .collect();
        let mut table_writer = TableWriter::default();
        let sheet_parts: Vec<_> = sheet_cells
            .iter()
            .zip(&sheet_tables)
            .enumerate()
            .map(|(i, (cells, tables))| {
                table_writer.add_sheet(tables, cells, caches.get(i), &style_table.dxf)
            })
            .collect();
        let mut chart_writer = ChartWriter::new(active_sheets, caches, stylings)?;
        let chart_parts: Vec<_> = active_sheets
            .iter()
            .enumerate()
            .map(|(i, (name, _))| chart_writer.add_sheet(name, stylings.get(i)))
            .collect::<Result<_, _>>()?;
        let wb_slicer_ext = table_writer.slicers.workbook_ext_xml();
        if !wb_slicer_ext.is_empty() {
            workbook_xml =
                workbook_xml.replace("</workbook>", &format!("{wb_slicer_ext}</workbook>"));
        }
        let has_spills = stylings.iter().any(|s| !s.spills.is_empty());
        let (content_types, mut wb_rels_xml) = if has_spills {
            (
                crate::sheet_styling::xlsx_parts::build_content_types(active_sheets.len()).replace(
                    "</Types>",
                    crate::sheet_styling::xlsx_parts::DYNAMIC_ARRAY_OVERRIDE,
                ),
                wb_rels_xml.replace(
                    "</Relationships>",
                    crate::sheet_styling::xlsx_parts::DYNAMIC_ARRAY_RELATIONSHIP,
                ),
            )
        } else {
            (
                crate::sheet_styling::xlsx_parts::build_content_types(active_sheets.len()),
                wb_rels_xml,
            )
        };
        if !table_writer.slicers.wb_rels.is_empty() {
            wb_rels_xml = wb_rels_xml.replace(
                "</Relationships>",
                &format!("{}</Relationships>", table_writer.slicers.wb_rels),
            );
        }
        let custom_theme = stylings
            .first()
            .and_then(|s| s.workbook_theme)
            .filter(|&t| t != DEFAULT_THEME);
        let extra_types = format!(
            "{}{}{}",
            table_writer.content_types,
            table_writer.slicers.content_types,
            chart_writer.content_types
        );
        let (content_types, wb_rels_xml) = if custom_theme.is_some() {
            (
                content_types.replace(
                    "</Types>",
                    &format!(
                        "{extra_types}{}</Types>",
                        crate::sheet_styling::xlsx_slicers::THEME_CONTENT_OVERRIDE
                    ),
                ),
                wb_rels_xml.replace(
                    "</Relationships>",
                    &format!(
                        "{}</Relationships>",
                        crate::sheet_styling::xlsx_slicers::THEME_WORKBOOK_REL
                    ),
                ),
            )
        } else {
            (
                content_types.replace("</Types>", &format!("{extra_types}</Types>")),
                wb_rels_xml,
            )
        };
        XmlSanitizer::validate_safe_xml(&workbook_xml)?;
        XmlSanitizer::validate_safe_xml(&wb_rels_xml)?;
        let mut entries = vec![
            (
                "[Content_Types].xml".to_string(),
                content_types.into_bytes(),
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

        if let Some(theme) = custom_theme {
            entries.push((
                "xl/theme/theme1.xml".to_string(),
                crate::sheet_styling::xlsx_slicers::build_theme_xml(theme).into_bytes(),
            ));
        }

        if has_spills {
            entries.push((
                "xl/metadata.xml".to_string(),
                crate::sheet_styling::xlsx_parts::DYNAMIC_ARRAY_METADATA
                    .as_bytes()
                    .to_vec(),
            ));
        }
        for (i, cells) in sheet_cells.iter().enumerate() {
            let (links, link_rels) = stylings
                .get(i)
                .map_or((String::new(), None), |st| links_xml(&st.links));
            let tables = &sheet_parts[i];
            let charts_part = &chart_parts[i];
            let sheet_xml = build_sheet_xml(
                tables.cells.as_deref().unwrap_or(cells),
                caches.get(i),
                stylings.get(i),
                &style_table,
                &sst_map,
                (&links, &charts_part.drawing_xml, &tables.parts_xml),
                &HeaderCells::new(&sheet_tables[i]),
            );
            XmlSanitizer::validate_safe_xml(&sheet_xml)?;
            entries.push((
                format!("xl/worksheets/sheet{}.xml", i + 1),
                sheet_xml.into_bytes(),
            ));
            let extra_rels = if charts_part.rels.is_empty() {
                tables.rels.as_str()
            } else if tables.rels.is_empty() {
                charts_part.rels.as_str()
            } else {
                ""
            };
            let combined_extra_rels;
            let extra_rels =
                if extra_rels.is_empty() && !charts_part.rels.is_empty() && !tables.rels.is_empty()
                {
                    combined_extra_rels = format!("{}{}", charts_part.rels, tables.rels);
                    combined_extra_rels.as_str()
                } else {
                    extra_rels
                };
            if let Some(rels) = merge_relationships(link_rels, extra_rels) {
                XmlSanitizer::validate_safe_xml(&rels)?;
                entries.push((
                    format!("xl/worksheets/_rels/sheet{}.xml.rels", i + 1),
                    rels.into_bytes(),
                ));
            }
        }

        for (_, data) in &table_writer.slicers.parts {
            if let Ok(xml) = std::str::from_utf8(data) {
                XmlSanitizer::validate_safe_xml(xml)?;
            }
        }
        entries.extend(table_writer.parts);
        entries.extend(table_writer.slicers.parts);
        entries.extend(chart_writer.parts);
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
        let mut sheet_specs: Vec<(String, String, SheetState)> = Vec::new();
        for (idx, (s_attrs, _)) in sheet_tags.into_iter().enumerate() {
            let name = extract_attr_from_fragment(s_attrs, "name")
                .map(XmlSanitizer::unescape)
                .unwrap_or_else(|| format!("Sheet{}", idx + 1));
            let target_path = extract_attr_from_fragment(s_attrs, "r:id")
                .or_else(|| extract_attr_from_fragment(s_attrs, "id"))
                .and_then(|rid| rel_targets.get(rid).cloned())
                .unwrap_or_else(|| format!("xl/worksheets/sheet{}.xml", idx + 1));
            let state = SheetState::from_xlsx(extract_attr_from_fragment(s_attrs, "state"));
            sheet_specs.push((name, target_path, state));
        }
        if sheet_specs.is_empty() {
            sheet_specs.push((
                "Sheet1".to_string(),
                "xl/worksheets/sheet1.xml".to_string(),
                SheetState::Visible,
            ));
        }
        // A workbook shows at least one sheet: a file that hides them all opens on its first.
        if sheet_specs.iter().all(|spec| spec.2 != SheetState::Visible) {
            sheet_specs[0].2 = SheetState::Visible;
        }

        let styles_xml = entry_by_path
            .get("xl/styles.xml")
            .and_then(|d| std::str::from_utf8(d).ok())
            .filter(|_| with_styling);
        let theme = with_styling.then(|| {
            entry_by_path
                .get("xl/theme/theme1.xml")
                .and_then(|d| std::str::from_utf8(d).ok())
                .map_or(DEFAULT_THEME, theme_palette)
        });
        let colors = theme.map(|theme| match styles_xml {
            Some(styles) => ColorScheme::from_styles(theme, styles),
            None => ColorScheme::new(theme),
        });
        let mut xf_styles = match (styles_xml, &colors) {
            (Some(styles), Some(colors)) => parse_cell_xf_styles(styles, colors),
            _ => XfStyles::default(),
        };
        // The differential formats are listed once for every reader of them (an empty entry takes an id).
        let dxf_list = styles_xml.map(dxf_bodies).unwrap_or_default();
        let dxf_styles = parse_dxf_styles(&dxf_list);
        if let (Some(colors), Some(styles)) = (&colors, styles_xml) {
            xf_styles.dxf_colors = parse_dxf_colors(&dxf_list, colors);
            xf_styles.table_styles = parse_table_styles(styles, &dxf_list, colors);
        }
        let mut stylings = Vec::new();
        let workbook_names: Vec<String> = sheet_specs.iter().map(|spec| spec.0.clone()).collect();
        let mut parsed_sheets = Vec::with_capacity(sheet_specs.len());
        let mut scanned_parts = std::collections::HashSet::new();
        let mut budget = DecodeBudget::new();
        let mut link_allowance = LinkAllowance::default();
        let mut table_registry = TableRegistry::default();
        let mut workbook_charts = 0usize;
        for (idx, (sheet_name, target_path, state)) in sheet_specs.into_iter().enumerate() {
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
                &xf_styles,
                with_styling,
                &mut budget,
            )?;
            if with_styling {
                let mut styling = scan.styling;
                styling.num_fmts = Arc::clone(&xf_styles.num_fmts);
                styling.fonts = Arc::clone(&xf_styles.fonts);
                state.apply(&mut styling);
                // The counts belong to the workbook; one sheet carries them.
                styling.num_fmts_rejected = if idx == 0 { xf_styles.rejected } else { 0 };
                styling.fonts_rejected = if idx == 0 {
                    xf_styles.fonts_rejected
                } else {
                    0
                };
                styling.rules = parse_worksheet_rules(sheet_xml, &dxf_styles);
                let relationships = sheet_relationships(&entry_by_path, part_path);
                let links = read_links(sheet_xml, &relationships, &mut link_allowance);
                (styling.links, styling.links_rejected) = (links.links, links.rejected);
                let link_color = colors
                    .as_ref()
                    .map_or(DEFAULT_THEME[10], ColorScheme::hyperlink);
                crate::sheet_styling::xlsx_styles::strip_link_look(
                    &mut styling,
                    &scan.link_styled,
                    link_color,
                );
                styling.tables_rejected = read_sheet_tables(
                    stylings.len(),
                    sheet_xml,
                    part_path,
                    &entry_by_path,
                    &mut table_registry,
                    &mut styling.tables,
                    &xf_styles.dxf_colors,
                    &xf_styles.table_styles,
                )?;
                styling.charts_rejected = read_sheet_charts(
                    &sheet_name,
                    sheet_xml,
                    part_path,
                    &entry_by_path,
                    &styling.columns,
                    &mut workbook_charts,
                    &mut styling.charts,
                )?;
                if let std::borrow::Cow::Owned(kept) =
                    merges_clear_of_tables(&styling.merges, &styling.tables)
                {
                    styling.merges_dropped = (styling.merges.len() - kept.len()) as u32;
                    styling.merges = kept;
                }
                if let (None, Some(tab), Some(theme)) = (styling.tab_color, scan.tab_theme, &theme)
                {
                    styling.tab_color = Some(apply_tint(theme[tab.index], tab.tint));
                }
                if let (None, Some(index), Some(colors)) =
                    (styling.tab_color, scan.tab_indexed, &colors)
                {
                    if let XlsxColor::Rgb(rgb) = colors.resolve(&format!("indexed=\"{index}\"")) {
                        styling.tab_color = Some(rgb);
                    }
                }
                stylings.push((sheet_name.clone(), styling));
            }
            parsed_sheets.push((sheet_name, scan.cells));
        }

        if with_styling {
            crate::sheet_styling::xlsx_slicers::read_table_slicer_parts(
                &entry_by_path,
                &table_registry.table_ids,
                &mut stylings,
            );
            crate::sheet_styling::xlsx_page::apply_print_names(
                wb_xml,
                &workbook_names,
                &mut stylings,
            );
            if let Some((_, first)) = stylings.first_mut() {
                (first.defined_names, first.names_rejected) =
                    crate::sheet_styling::xlsx_names::read_defined_names(wb_xml, &workbook_names);
                first.workbook_protect =
                    crate::sheet_styling::xlsx_protect::read_workbook_protection(wb_xml)?;
                first.workbook_theme = theme.filter(|&t| t != DEFAULT_THEME);
            }
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
#[path = "office_codecs/tests.rs"]
mod tests;

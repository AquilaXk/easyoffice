pub use crate::odf_hwpx_codecs::*;
pub use crate::rtf_xml_helpers::*;
use crate::sheet_styling::xlsx_styles::{
    cols_xml, parse_cell_xf_styles, parse_worksheet_styling, XlsxStyleTable,
};
use crate::sheet_styling::SheetStyling;
use crate::{EasyOfficeError, XmlSanitizer, ZipArchiveCodec, ZipSecurityPolicy};

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

impl OoxmlCodec {
    pub const MAX_XLSX_ROWS: u32 = 1_048_576;
    pub const MAX_XLSX_COLS: u32 = 16_384;

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
        validate_zip_xml_entries(&entries)?;
        let doc_entry = entries
            .iter()
            .find(|e| e.path == "word/document.xml")
            .ok_or_else(|| {
                EasyOfficeError::CorruptedContainer(
                    "DOCX 필수 엔트리 'word/document.xml'이 누락되었습니다.".to_string(),
                )
            })?;
        let xml = std::str::from_utf8(&doc_entry.data).map_err(|_| {
            EasyOfficeError::InvalidUtf8("word/document.xml UTF-8 오류".to_string())
        })?;
        XmlSanitizer::validate_safe_xml(xml)?;

        let core_title = entries
            .iter()
            .find(|e| e.path == "docProps/core.xml")
            .and_then(|e| std::str::from_utf8(&e.data).ok())
            .and_then(|c_xml| extract_tag_bodies(c_xml, "dc:title").into_iter().next())
            .map(XmlSanitizer::unescape)
            .filter(|s| !s.is_empty());

        let title = extract_attr(xml, "<w:document", "title")
            .map(|s| XmlSanitizer::unescape(&s))
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
            if let Some((row, col)) = cell.or(column) {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "XLSX 서식 좌표({row}, {col})가 ECMA-376 허용 범위({}x{})를 초과했습니다.",
                    Self::MAX_XLSX_ROWS,
                    Self::MAX_XLSX_COLS
                )));
            }
        }
        let style_table = XlsxStyleTable::build(stylings);

        let mut content_types = String::from(concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">",
            "<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>",
            "<Default Extension=\"xml\" ContentType=\"application/xml\"/>",
            "<Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>",
        ));
        for idx in 1..=active_sheets.len() {
            content_types.push_str(&format!(
                "<Override PartName=\"/xl/worksheets/sheet{idx}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"
            ));
        }
        content_types.push_str(concat!(
            "<Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>",
            "<Override PartName=\"/xl/sharedStrings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml\"/>",
            "</Types>"
        ));

        let root_rels = concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
            "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/>",
            "</Relationships>"
        );

        let mut workbook_xml = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets>",
        );
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
            workbook_xml.push_str(&format!(
                "<sheet name=\"{}\" sheetId=\"{num}\" r:id=\"rId{num}\"/>",
                XmlSanitizer::escape(&clean_name)
            ));
            wb_rels_xml.push_str(&format!(
                "<Relationship Id=\"rId{num}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{num}.xml\"/>"
            ));
        }
        workbook_xml
            .push_str("</sheets><calcPr calcId=\"191029\" fullCalcOnLoad=\"1\"/></workbook>");
        let styles_rid = active_sheets.len() + 1;
        let sst_rid = active_sheets.len() + 2;
        wb_rels_xml.push_str(&format!(
            "<Relationship Id=\"rId{styles_rid}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>"
        ));
        wb_rels_xml.push_str(&format!(
            "<Relationship Id=\"rId{sst_rid}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings\" Target=\"sharedStrings.xml\"/></Relationships>"
        ));

        let styles_xml = style_table.to_xml();

        let mut shared_strings: Vec<&str> = Vec::new();
        let mut sst_map: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        let mut total_str_count = 0usize;
        for (_, cells) in active_sheets {
            for (_, _, val) in cells {
                if !val.starts_with('=') && val.parse::<f64>().is_err() {
                    total_str_count += 1;
                    if !sst_map.contains_key(val.as_str()) {
                        let idx = shared_strings.len();
                        shared_strings.push(val.as_str());
                        sst_map.insert(val.as_str(), idx);
                    }
                }
            }
        }

        let mut sst_xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" count=\"{total_str_count}\" uniqueCount=\"{}\">",
            shared_strings.len()
        );
        for s in &shared_strings {
            sst_xml.push_str("<si><t>");
            sst_xml.push_str(&XmlSanitizer::escape(s));
            sst_xml.push_str("</t></si>");
        }
        sst_xml.push_str("</sst>");

        XmlSanitizer::validate_safe_xml(&workbook_xml)?;
        XmlSanitizer::validate_safe_xml(&wb_rels_xml)?;
        XmlSanitizer::validate_safe_xml(&sst_xml)?;

        let mut entries = vec![
            (
                "[Content_Types].xml".to_string(),
                content_types.into_bytes(),
            ),
            ("xl/workbook.xml".to_string(), workbook_xml.into_bytes()),
            ("_rels/.rels".to_string(), root_rels.as_bytes().to_vec()),
            (
                "xl/_rels/workbook.xml.rels".to_string(),
                wb_rels_xml.into_bytes(),
            ),
            ("xl/styles.xml".to_string(), styles_xml.into_bytes()),
            ("xl/sharedStrings.xml".to_string(), sst_xml.into_bytes()),
        ];

        for (i, (_, cells)) in active_sheets.iter().enumerate() {
            let cache = caches.get(i);
            let styling = stylings.get(i);
            let blank = String::new();
            let mut rows_map: std::collections::BTreeMap<u32, Vec<(u32, &String)>> =
                std::collections::BTreeMap::new();
            for (row, col, raw) in cells {
                rows_map.entry(*row).or_default().push((*col, raw));
            }
            // Styled cells without a value are still written (`<c r=".." s=".."/>`).
            if let Some(st) = styling {
                let valued: std::collections::HashSet<(u32, u32)> =
                    cells.iter().map(|(r, c, _)| (*r, *c)).collect();
                for (&(row, col), style) in &st.cells {
                    if !style.is_plain() && !valued.contains(&(row, col)) {
                        rows_map.entry(row).or_default().push((col, &blank));
                    }
                }
            }

            let mut sheet_xml = String::from(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">",
            );
            if let Some(st) = styling {
                sheet_xml.push_str(&cols_xml(&st.columns));
            }
            sheet_xml.push_str("<sheetData>");
            for (row, mut row_cells) in rows_map {
                row_cells.sort_by_key(|(col, _)| *col);
                let row_num = row.saturating_add(1);
                sheet_xml.push_str(&format!("<row r=\"{row_num}\">"));
                for (col, raw) in row_cells {
                    let a1 = format!("{}{}", format_col_letters(col), row_num);
                    let xf = styling
                        .and_then(|st| st.cells.get(&(row, col)))
                        .map_or(0, |style| style_table.xf_index(style));
                    let s = if xf > 0 {
                        format!(" s=\"{xf}\"")
                    } else {
                        String::new()
                    };
                    if raw.is_empty() {
                        sheet_xml.push_str(&format!("<c r=\"{a1}\"{s}/>"));
                    } else if let Some(formula) = raw.strip_prefix('=') {
                        let f = XmlSanitizer::escape(formula);
                        match cache.and_then(|c| c.get(&(row, col))) {
                            Some(XlsxCachedValue::Number(n)) if n.is_finite() => sheet_xml
                                .push_str(&format!("<c r=\"{a1}\"{s}><f>{f}</f><v>{n}</v></c>")),
                            Some(XlsxCachedValue::Text(text)) => sheet_xml.push_str(&format!(
                                "<c r=\"{a1}\"{s} t=\"str\"><f>{f}</f><v>{}</v></c>",
                                XmlSanitizer::escape(text)
                            )),
                            Some(XlsxCachedValue::Boolean(b)) => sheet_xml.push_str(&format!(
                                "<c r=\"{a1}\"{s} t=\"b\"><f>{f}</f><v>{}</v></c>",
                                u8::from(*b)
                            )),
                            _ => sheet_xml.push_str(&format!("<c r=\"{a1}\"{s}><f>{f}</f></c>")),
                        }
                    } else if raw.parse::<f64>().is_ok() {
                        sheet_xml.push_str(&format!(
                            "<c r=\"{a1}\"{s} t=\"n\"><v>{}</v></c>",
                            XmlSanitizer::escape(raw)
                        ));
                    } else {
                        let idx = sst_map.get(raw.as_str()).copied().unwrap_or(0);
                        sheet_xml.push_str(&format!("<c r=\"{a1}\"{s} t=\"s\"><v>{idx}</v></c>"));
                    }
                }
                sheet_xml.push_str("</row>");
            }
            sheet_xml.push_str("</sheetData></worksheet>");
            XmlSanitizer::validate_safe_xml(&sheet_xml)?;
            entries.push((
                format!("xl/worksheets/sheet{}.xml", i + 1),
                sheet_xml.into_bytes(),
            ));
        }

        ZipArchiveCodec::pack_compressed(&entries)
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

    #[allow(clippy::type_complexity)]
    fn unpack_xlsx_inner(
        bytes: &[u8],
        with_styling: bool,
    ) -> Result<(Vec<OdsUnpacked>, Vec<(String, SheetStyling)>), EasyOfficeError> {
        let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())?;
        validate_zip_xml_entries(&entries)?;
        let wb_entry = entries
            .iter()
            .find(|e| e.path == "xl/workbook.xml")
            .ok_or_else(|| {
                EasyOfficeError::CorruptedContainer(
                    "XLSX 필수 엔트리 'xl/workbook.xml'이 누락되었습니다.".to_string(),
                )
            })?;
        let wb_xml = std::str::from_utf8(&wb_entry.data)
            .map_err(|_| EasyOfficeError::InvalidUtf8("xl/workbook.xml UTF-8 오류".to_string()))?;
        XmlSanitizer::validate_safe_xml(wb_xml)?;

        let mut rel_targets: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        if let Some(rels_entry) = entries
            .iter()
            .find(|e| e.path == "xl/_rels/workbook.xml.rels")
        {
            let rels_xml = std::str::from_utf8(&rels_entry.data).map_err(|_| {
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
                        target
                    } else {
                        format!("xl/{target}")
                    };
                    rel_targets.insert(id, normalized);
                }
            }
        }

        let mut shared_strings: Vec<String> = Vec::new();
        if let Some(sst_entry) = entries.iter().find(|e| e.path == "xl/sharedStrings.xml") {
            let sst_xml = std::str::from_utf8(&sst_entry.data).map_err(|_| {
                EasyOfficeError::InvalidUtf8("xl/sharedStrings.xml UTF-8 오류".to_string())
            })?;
            XmlSanitizer::validate_safe_xml(sst_xml)?;
            for (_, si_inner) in extract_balanced_elements(sst_xml, "si") {
                if si_inner.trim().is_empty() {
                    shared_strings.push(String::new());
                } else {
                    let without_phonetic = strip_tag_blocks(si_inner, "rPh");
                    let joined = extract_tag_bodies(&without_phonetic, "t")
                        .into_iter()
                        .map(XmlSanitizer::unescape)
                        .collect::<Vec<_>>()
                        .join("");
                    shared_strings.push(joined);
                }
            }
        }

        let sheet_tags = extract_balanced_elements(wb_xml, "sheet");
        let mut sheet_specs: Vec<(String, String)> = Vec::new();
        for (idx, (s_attrs, _)) in sheet_tags.into_iter().enumerate() {
            let name = extract_attr_from_fragment(s_attrs, "name")
                .map(|s| XmlSanitizer::unescape(&s))
                .unwrap_or_else(|| format!("Sheet{}", idx + 1));
            let target_path = extract_attr_from_fragment(s_attrs, "r:id")
                .or_else(|| extract_attr_from_fragment(s_attrs, "id"))
                .and_then(|rid| rel_targets.get(&rid).cloned())
                .unwrap_or_else(|| format!("xl/worksheets/sheet{}.xml", idx + 1));
            sheet_specs.push((name, target_path));
        }
        if sheet_specs.is_empty() {
            sheet_specs.push(("Sheet1".to_string(), "xl/worksheets/sheet1.xml".to_string()));
        }

        let xf_styles = if with_styling {
            entries
                .iter()
                .find(|e| e.path == "xl/styles.xml")
                .and_then(|e| std::str::from_utf8(&e.data).ok())
                .map(parse_cell_xf_styles)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let mut stylings = Vec::new();
        let mut parsed_sheets = Vec::with_capacity(sheet_specs.len());
        for (idx, (sheet_name, target_path)) in sheet_specs.into_iter().enumerate() {
            let sheet_entry = entries.iter().find(|e| e.path == target_path).or_else(|| {
                let fallback = format!("xl/worksheets/sheet{}.xml", idx + 1);
                entries.iter().find(|e| e.path == fallback)
            });
            let Some(s_entry) = sheet_entry else {
                if idx == 0 {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "XLSX 필수 엔트리 'xl/worksheets/sheet1.xml'이 누락되었습니다.".to_string(),
                    ));
                }
                continue;
            };
            let sheet_xml = std::str::from_utf8(&s_entry.data).map_err(|_| {
                EasyOfficeError::InvalidUtf8("xl/worksheets/sheet.xml UTF-8 오류".to_string())
            })?;
            XmlSanitizer::validate_safe_xml(sheet_xml)?;
            if with_styling {
                stylings.push((
                    sheet_name.clone(),
                    parse_worksheet_styling(sheet_xml, &xf_styles),
                ));
            }
            let cells = Self::parse_xlsx_worksheet_cells(sheet_xml, &shared_strings)?;
            parsed_sheets.push((sheet_name, cells));
        }

        Ok((parsed_sheets, stylings))
    }

    fn parse_xlsx_worksheet_cells(
        sheet_xml: &str,
        shared_strings: &[String],
    ) -> Result<Vec<(u32, u32, String)>, EasyOfficeError> {
        let mut cells: Vec<(u32, u32, String)> = Vec::new();
        for (attrs_raw, inner) in extract_tags_with_attrs(sheet_xml, "c") {
            let r_attr = extract_attr_from_fragment(attrs_raw, "r");
            let (row, col) = if let (Some(r), Some(c)) = (
                extract_attr_from_fragment(attrs_raw, "row").and_then(|v| v.parse::<u32>().ok()),
                extract_attr_from_fragment(attrs_raw, "col").and_then(|v| v.parse::<u32>().ok()),
            ) {
                if r >= Self::MAX_XLSX_ROWS || c >= Self::MAX_XLSX_COLS {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "XLSX 셀 좌표가 ECMA-376 허용 범위를 초과했습니다.".to_string(),
                    ));
                }
                (r, c)
            } else if let Some(a1) = r_attr {
                parse_a1_ZeroBased(&a1).ok_or_else(|| {
                    EasyOfficeError::ResourceBudgetExceeded(
                        "XLSX A1 셀 참조가 ECMA-376 허용 범위를 초과했거나 잘못되었습니다."
                            .to_string(),
                    )
                })?
            } else {
                (0, 0)
            };

            let cell_type = extract_attr_from_fragment(attrs_raw, "t").unwrap_or_default();
            if let Some(f_body) = extract_tag_bodies(inner, "f").first() {
                cells.push((row, col, format!("={}", XmlSanitizer::unescape(f_body))));
            } else if cell_type == "inlineStr" {
                let without_phonetic = strip_tag_blocks(inner, "rPh");
                let inline_txt = extract_tag_bodies(&without_phonetic, "t")
                    .into_iter()
                    .map(XmlSanitizer::unescape)
                    .collect::<Vec<_>>()
                    .join("");
                cells.push((row, col, inline_txt));
            } else if let Some(v_body) = extract_tag_bodies(inner, "v").first() {
                let raw_v = XmlSanitizer::unescape(v_body);
                if cell_type == "s" {
                    let idx = raw_v.parse::<usize>().unwrap_or(0);
                    let resolved = shared_strings.get(idx).cloned().unwrap_or(raw_v);
                    cells.push((row, col, resolved));
                } else {
                    cells.push((row, col, raw_v));
                }
            }
        }
        Ok(cells)
    }
}

pub(crate) fn validate_zip_xml_entries(
    entries: &[crate::ZipArchiveEntry],
) -> Result<(), EasyOfficeError> {
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
        }
    }
    Ok(())
}

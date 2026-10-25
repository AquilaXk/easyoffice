use crate::{ContainerFamily, EasyOfficeError, XmlSanitizer, ZipArchiveCodec, ZipSecurityPolicy};

/// Unpacked ODF text document (`(title, paragraphs, tables)`).
pub type OdtUnpacked = (String, Vec<String>, Vec<Vec<Vec<String>>>);
/// Unpacked ODF spreadsheet (`(sheet_name, cells)`).
pub type OdsUnpacked = (String, Vec<(u32, u32, String)>);
/// Unpacked ODF presentation (`(title, slides)`).
pub type OdpUnpacked = (String, Vec<(String, Vec<String>, String)>);

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

/// ECMA-376 OOXML container codec for `.docx`, `.xlsx`, `.pptx`, and `.ppsx`.
pub struct OoxmlCodec;

impl OoxmlCodec {
    pub fn pack_docx(payload: &OoxmlDocxPayload) -> Result<Vec<u8>, EasyOfficeError> {
        let content_types = concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">",
            "<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>",
            "<Default Extension=\"xml\" ContentType=\"application/xml\"/>",
            "<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>",
            "</Types>"
        );
        let rels = concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
            "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>",
            "</Relationships>"
        );

        let mut body = String::with_capacity(512);
        body.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
        body.push_str("<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" title=\"");
        body.push_str(&XmlSanitizer::escape(&payload.title));
        body.push_str("\"><w:body>");

        for p in &payload.paragraphs {
            body.push_str("<w:p><w:r><w:t>");
            body.push_str(&XmlSanitizer::escape(p));
            body.push_str("</w:t></w:r></w:p>");
        }

        for tbl in &payload.tables {
            body.push_str("<w:tbl>");
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

        ZipArchiveCodec::pack(&[
            (
                "[Content_Types].xml".to_string(),
                content_types.as_bytes().to_vec(),
            ),
            ("_rels/.rels".to_string(), rels.as_bytes().to_vec()),
            ("word/document.xml".to_string(), body.into_bytes()),
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

        let title = extract_attr(xml, "<w:document", "title")
            .map(|s| XmlSanitizer::unescape(&s))
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
        for p_inner in extract_tag_bodies(&xml_without_tables, "w:p") {
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
        let content_types = concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">",
            "<Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>",
            "<Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>",
            "<Override PartName=\"/xl/sharedStrings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml\"/>",
            "</Types>"
        );
        let workbook_xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets><sheet name=\"{}\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>",
            XmlSanitizer::escape(&payload.sheet_name)
        );

        let mut shared_strings: Vec<&str> = Vec::new();
        let mut sst_map: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for (_, _, val) in &payload.cells {
            if !val.starts_with('=')
                && val.parse::<f64>().is_err()
                && !sst_map.contains_key(val.as_str())
            {
                let idx = shared_strings.len();
                shared_strings.push(val.as_str());
                sst_map.insert(val.as_str(), idx);
            }
        }

        let mut sst_xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" count=\"{}\" uniqueCount=\"{}\">",
            shared_strings.len(),
            shared_strings.len()
        );
        for s in &shared_strings {
            sst_xml.push_str("<si><t>");
            sst_xml.push_str(&XmlSanitizer::escape(s));
            sst_xml.push_str("</t></si>");
        }
        sst_xml.push_str("</sst>");

        let mut rows_map: std::collections::BTreeMap<u32, Vec<(u32, &String)>> =
            std::collections::BTreeMap::new();
        for (row, col, raw) in &payload.cells {
            rows_map.entry(*row).or_default().push((*col, raw));
        }

        let mut sheet_xml = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>",
        );
        for (row, mut row_cells) in rows_map {
            row_cells.sort_by_key(|(col, _)| *col);
            let row_num = row.saturating_add(1);
            sheet_xml.push_str(&format!("<row r=\"{row_num}\">"));
            for (col, raw) in row_cells {
                let a1 = format!("{}{}", format_col_letters(col), row_num);
                if let Some(formula) = raw.strip_prefix('=') {
                    sheet_xml.push_str(&format!(
                        "<c r=\"{a1}\" row=\"{row}\" col=\"{col}\"><f>{}</f><v>0</v></c>",
                        XmlSanitizer::escape(formula)
                    ));
                } else if raw.parse::<f64>().is_ok() {
                    sheet_xml.push_str(&format!(
                        "<c r=\"{a1}\" row=\"{row}\" col=\"{col}\" t=\"n\"><v>{}</v></c>",
                        XmlSanitizer::escape(raw)
                    ));
                } else {
                    let idx = sst_map.get(raw.as_str()).copied().unwrap_or(0);
                    sheet_xml.push_str(&format!(
                        "<c r=\"{a1}\" row=\"{row}\" col=\"{col}\" t=\"s\"><v>{idx}</v></c>"
                    ));
                }
            }
            sheet_xml.push_str("</row>");
        }
        sheet_xml.push_str("</sheetData></worksheet>");

        XmlSanitizer::validate_safe_xml(&workbook_xml)?;
        XmlSanitizer::validate_safe_xml(&sst_xml)?;
        XmlSanitizer::validate_safe_xml(&sheet_xml)?;

        ZipArchiveCodec::pack(&[
            (
                "[Content_Types].xml".to_string(),
                content_types.as_bytes().to_vec(),
            ),
            ("xl/workbook.xml".to_string(), workbook_xml.into_bytes()),
            ("xl/sharedStrings.xml".to_string(), sst_xml.into_bytes()),
            (
                "xl/worksheets/sheet1.xml".to_string(),
                sheet_xml.into_bytes(),
            ),
        ])
    }

    pub fn unpack_xlsx(bytes: &[u8]) -> Result<OoxmlXlsxPayload, EasyOfficeError> {
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

        let sheet_name = extract_attr(wb_xml, "<sheet", "name")
            .map(|s| XmlSanitizer::unescape(&s))
            .unwrap_or_else(|| "Sheet1".to_string());

        let mut shared_strings: Vec<String> = Vec::new();
        if let Some(sst_entry) = entries.iter().find(|e| e.path == "xl/sharedStrings.xml") {
            let sst_xml = std::str::from_utf8(&sst_entry.data).map_err(|_| {
                EasyOfficeError::InvalidUtf8("xl/sharedStrings.xml UTF-8 오류".to_string())
            })?;
            XmlSanitizer::validate_safe_xml(sst_xml)?;
            for si in extract_tag_bodies(sst_xml, "si") {
                let joined = extract_tag_bodies(si, "t")
                    .into_iter()
                    .map(XmlSanitizer::unescape)
                    .collect::<Vec<_>>()
                    .join("");
                shared_strings.push(joined);
            }
        }

        let sheet_entry = entries
            .iter()
            .find(|e| e.path == "xl/worksheets/sheet1.xml")
            .ok_or_else(|| {
                EasyOfficeError::CorruptedContainer(
                    "XLSX 필수 엔트리 'xl/worksheets/sheet1.xml'이 누락되었습니다.".to_string(),
                )
            })?;
        let sheet_xml = std::str::from_utf8(&sheet_entry.data).map_err(|_| {
            EasyOfficeError::InvalidUtf8("xl/worksheets/sheet1.xml UTF-8 오류".to_string())
        })?;
        XmlSanitizer::validate_safe_xml(sheet_xml)?;

        let mut cells: Vec<(u32, u32, String)> = Vec::new();
        for (attrs_raw, inner) in extract_tags_with_attrs(sheet_xml, "c") {
            let r_attr = extract_attr_from_fragment(attrs_raw, "r");
            let (row, col) = if let (Some(r), Some(c)) = (
                extract_attr_from_fragment(attrs_raw, "row").and_then(|v| v.parse::<u32>().ok()),
                extract_attr_from_fragment(attrs_raw, "col").and_then(|v| v.parse::<u32>().ok()),
            ) {
                if r > 1_048_575 || c > 16_383 {
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
                let inline_txt = extract_tag_bodies(inner, "t")
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

        Ok(OoxmlXlsxPayload { sheet_name, cells })
    }

    pub fn pack_pptx(payload: &OoxmlPptxPayload) -> Result<Vec<u8>, EasyOfficeError> {
        let main_ct = if payload.is_slideshow {
            "application/vnd.openxmlformats-officedocument.presentationml.slideshow.main+xml"
        } else {
            "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"
        };
        let content_types = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Override PartName=\"/ppt/presentation.xml\" ContentType=\"{main_ct}\"/></Types>"
        );
        let pres_xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:presentation xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" title=\"{}\" slideshow=\"{}\"><p:sldIdLst count=\"{}\"/></p:presentation>",
            XmlSanitizer::escape(&payload.title),
            if payload.is_slideshow { "1" } else { "0" },
            payload.slides.len()
        );
        XmlSanitizer::validate_safe_xml(&pres_xml)?;

        let mut entries = vec![
            (
                "[Content_Types].xml".to_string(),
                content_types.into_bytes(),
            ),
            ("ppt/presentation.xml".to_string(), pres_xml.into_bytes()),
        ];

        for (idx, (s_title, bullets, notes)) in payload.slides.iter().enumerate() {
            let mut sld_xml = format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:sld xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" title=\"{}\" notes=\"{}\"><p:cSld>",
                XmlSanitizer::escape(s_title),
                XmlSanitizer::escape(notes)
            );
            for b in bullets {
                sld_xml.push_str("<a:t>");
                sld_xml.push_str(&XmlSanitizer::escape(b));
                sld_xml.push_str("</a:t>");
            }
            sld_xml.push_str("</p:cSld></p:sld>");
            XmlSanitizer::validate_safe_xml(&sld_xml)?;
            entries.push((
                format!("ppt/slides/slide{}.xml", idx + 1),
                sld_xml.into_bytes(),
            ));
        }

        ZipArchiveCodec::pack(&entries)
    }

    pub fn unpack_pptx(bytes: &[u8]) -> Result<OoxmlPptxPayload, EasyOfficeError> {
        let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())?;
        validate_zip_xml_entries(&entries)?;
        let pres_entry = entries
            .iter()
            .find(|e| e.path == "ppt/presentation.xml")
            .ok_or_else(|| {
                EasyOfficeError::CorruptedContainer(
                    "PPTX 필수 엔트리 'ppt/presentation.xml'이 누락되었습니다.".to_string(),
                )
            })?;
        let pres_xml = std::str::from_utf8(&pres_entry.data).map_err(|_| {
            EasyOfficeError::InvalidUtf8("ppt/presentation.xml UTF-8 오류".to_string())
        })?;
        XmlSanitizer::validate_safe_xml(pres_xml)?;

        let title = extract_attr(pres_xml, "<p:presentation", "title")
            .map(|s| XmlSanitizer::unescape(&s))
            .unwrap_or_else(|| "발표자료".to_string());
        let is_slideshow = extract_attr(pres_xml, "<p:presentation", "slideshow")
            .map(|s| s == "1")
            .unwrap_or(false);

        let mut slide_entries: Vec<_> = entries
            .iter()
            .filter(|e| e.path.starts_with("ppt/slides/slide") && e.path.ends_with(".xml"))
            .collect();
        slide_entries.sort_by(|a, b| {
            let num_a = a
                .path
                .strip_prefix("ppt/slides/slide")
                .and_then(|s| s.strip_suffix(".xml"))
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(usize::MAX);
            let num_b = b
                .path
                .strip_prefix("ppt/slides/slide")
                .and_then(|s| s.strip_suffix(".xml"))
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(usize::MAX);
            num_a.cmp(&num_b).then_with(|| a.path.cmp(&b.path))
        });

        let mut slides = Vec::new();
        for s_entry in slide_entries {
            let s_xml = std::str::from_utf8(&s_entry.data).map_err(|_| {
                EasyOfficeError::InvalidUtf8("ppt/slides/slide.xml UTF-8 오류".to_string())
            })?;
            XmlSanitizer::validate_safe_xml(s_xml)?;
            let mut bullets: Vec<String> = extract_tag_bodies(s_xml, "a:t")
                .into_iter()
                .map(XmlSanitizer::unescape)
                .collect();
            let s_title = extract_attr(s_xml, "<p:sld", "title")
                .map(|s| XmlSanitizer::unescape(&s))
                .unwrap_or_else(|| {
                    if !bullets.is_empty() {
                        bullets.remove(0)
                    } else {
                        String::new()
                    }
                });
            let notes = extract_attr(s_xml, "<p:sld", "notes")
                .map(|s| XmlSanitizer::unescape(&s))
                .unwrap_or_default();
            slides.push((s_title, bullets, notes));
        }

        Ok(OoxmlPptxPayload {
            title,
            is_slideshow,
            slides,
        })
    }
}

/// ISO/IEC 26300 OpenDocument Format (ODF) container codec for `.odt`, `.ods`, and `.odp`.
pub struct OdfCodec;

impl OdfCodec {
    pub const MIMETYPE_ODT: &'static str = "application/vnd.oasis.opendocument.text";
    pub const MIMETYPE_ODS: &'static str = "application/vnd.oasis.opendocument.spreadsheet";
    pub const MIMETYPE_ODP: &'static str = "application/vnd.oasis.opendocument.presentation";
    pub const MAX_CELLS: usize = 200_000;
    pub const MAX_ROWS: u32 = 1_048_575;
    pub const MAX_COLS: u32 = 16_383;

    pub fn pack_odt(
        title: &str,
        paragraphs: &[String],
        tables: &[Vec<Vec<String>>],
    ) -> Result<Vec<u8>, EasyOfficeError> {
        let manifest = Self::build_manifest(Self::MIMETYPE_ODT);
        let mut content = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" office:version=\"1.2\" title=\"{}\"><office:body><office:text>",
            XmlSanitizer::escape(title)
        );
        for p in paragraphs {
            content.push_str("<text:p>");
            content.push_str(&XmlSanitizer::escape(p));
            content.push_str("</text:p>");
        }
        for tbl in tables {
            content.push_str("<table:table>");
            for row in tbl {
                content.push_str("<table:table-row>");
                for cell in row {
                    content.push_str("<table:table-cell><text:p>");
                    content.push_str(&XmlSanitizer::escape(cell));
                    content.push_str("</text:p></table:table-cell>");
                }
                content.push_str("</table:table-row>");
            }
            content.push_str("</table:table>");
        }
        content.push_str("</office:text></office:body></office:document-content>");
        XmlSanitizer::validate_safe_xml(&content)?;

        ZipArchiveCodec::pack(&[
            (
                "mimetype".to_string(),
                Self::MIMETYPE_ODT.as_bytes().to_vec(),
            ),
            ("META-INF/manifest.xml".to_string(), manifest.into_bytes()),
            ("content.xml".to_string(), content.into_bytes()),
        ])
    }

    pub fn unpack_odt(bytes: &[u8]) -> Result<OdtUnpacked, EasyOfficeError> {
        let content_xml = Self::extract_content_xml(bytes, Self::MIMETYPE_ODT)?;
        let title = extract_attr(&content_xml, "<office:document-content", "title")
            .map(|s| XmlSanitizer::unescape(&s))
            .unwrap_or_else(|| "ODT 문서".to_string());

        let mut tables = Vec::new();
        for tbl_body in extract_tag_bodies(&content_xml, "table:table") {
            let mut rows = Vec::new();
            for row_body in extract_tag_bodies(tbl_body, "table:table-row") {
                let mut cells = Vec::new();
                for cell_body in extract_tag_bodies(row_body, "table:table-cell") {
                    let cell_text = extract_tag_bodies(cell_body, "text:p")
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

        let without_tables = strip_tag_blocks(&content_xml, "table:table");
        let paragraphs = extract_tag_bodies(&without_tables, "text:p")
            .into_iter()
            .map(XmlSanitizer::unescape)
            .collect();

        Ok((title, paragraphs, tables))
    }

    pub fn pack_ods(
        sheet_name: &str,
        cells: &[(u32, u32, String)],
    ) -> Result<Vec<u8>, EasyOfficeError> {
        if cells.len() > Self::MAX_CELLS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "ODS 셀 개수가 허용 예산을 초과했습니다.".to_string(),
            ));
        }
        let manifest = Self::build_manifest(Self::MIMETYPE_ODS);
        let mut grid: std::collections::BTreeMap<u32, std::collections::BTreeMap<u32, &String>> =
            std::collections::BTreeMap::new();
        for (row, col, raw) in cells {
            if *row > Self::MAX_ROWS || *col > Self::MAX_COLS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "ODS 셀 좌표가 허용 범위를 초과했습니다.".to_string(),
                ));
            }
            grid.entry(*row).or_default().insert(*col, raw);
        }

        let mut content = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" office:version=\"1.2\"><office:body><office:spreadsheet><table:table table:name=\"{}\">",
            XmlSanitizer::escape(sheet_name)
        );

        let mut expected_row: u32 = 0;
        for (row, row_cells) in grid {
            if row > expected_row {
                let gap = row - expected_row;
                if gap == 1 {
                    content.push_str("<table:table-row><table:table-cell/></table:table-row>");
                } else {
                    content.push_str(&format!(
                        "<table:table-row table:number-rows-repeated=\"{gap}\"><table:table-cell/></table:table-row>"
                    ));
                }
            }
            content.push_str("<table:table-row>");
            let mut expected_col: u32 = 0;
            for (col, raw) in row_cells {
                if col > expected_col {
                    let gap = col - expected_col;
                    if gap == 1 {
                        content.push_str("<table:table-cell/>");
                    } else {
                        content.push_str(&format!(
                            "<table:table-cell table:number-columns-repeated=\"{gap}\"/>"
                        ));
                    }
                }
                if let Some(formula) = raw.strip_prefix('=') {
                    let escaped_formula = XmlSanitizer::escape(formula);
                    content.push_str(&format!(
                        "<table:table-cell row=\"{row}\" col=\"{col}\" table:formula=\"of:={escaped_formula}\" office:value-type=\"string\"><text:p>{}</text:p></table:table-cell>",
                        XmlSanitizer::escape(raw)
                    ));
                } else if raw.parse::<f64>().is_ok() {
                    let escaped_val = XmlSanitizer::escape(raw);
                    content.push_str(&format!(
                        "<table:table-cell row=\"{row}\" col=\"{col}\" office:value-type=\"float\" office:value=\"{escaped_val}\"><text:p>{escaped_val}</text:p></table:table-cell>"
                    ));
                } else {
                    content.push_str(&format!(
                        "<table:table-cell row=\"{row}\" col=\"{col}\" office:value-type=\"string\"><text:p>{}</text:p></table:table-cell>",
                        XmlSanitizer::escape(raw)
                    ));
                }
                expected_col = col.saturating_add(1);
            }
            content.push_str("</table:table-row>");
            expected_row = row.saturating_add(1);
        }

        content.push_str(
            "</table:table></office:spreadsheet></office:body></office:document-content>",
        );
        XmlSanitizer::validate_safe_xml(&content)?;

        ZipArchiveCodec::pack(&[
            (
                "mimetype".to_string(),
                Self::MIMETYPE_ODS.as_bytes().to_vec(),
            ),
            ("META-INF/manifest.xml".to_string(), manifest.into_bytes()),
            ("content.xml".to_string(), content.into_bytes()),
        ])
    }

    pub fn unpack_ods(bytes: &[u8]) -> Result<OdsUnpacked, EasyOfficeError> {
        let content_xml = Self::extract_content_xml(bytes, Self::MIMETYPE_ODS)?;
        let sheet_name = extract_attr(&content_xml, "<table:table", "table:name")
            .map(|s| XmlSanitizer::unescape(&s))
            .unwrap_or_else(|| "Sheet1".to_string());

        let mut cells = Vec::new();
        let mut current_row: u32 = 0;

        for (row_attrs, row_inner) in
            extract_tags_or_self_closing_with_attrs(&content_xml, "table:table-row")
        {
            let row_repeat = extract_attr_from_fragment(row_attrs, "table:number-rows-repeated")
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(1)
                .max(1);
            let next_row = current_row.checked_add(row_repeat).ok_or_else(|| {
                EasyOfficeError::ResourceBudgetExceeded(
                    "ODS 행 반복 횟수가 허용 범위를 초과했습니다.".to_string(),
                )
            })?;
            if next_row > Self::MAX_ROWS + 1 {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "ODS 행 인덱스가 허용 범위를 초과했습니다.".to_string(),
                ));
            }

            type ParsedOdsRowCell = (Option<u32>, Option<u32>, u32, Option<String>);
            let raw_cells = extract_ods_cells_from_row(row_inner);
            let mut parsed_row_cells: Vec<ParsedOdsRowCell> = Vec::new();
            let mut has_any_populated = false;
            for (_is_self_closing, cell_attrs, cell_inner) in raw_cells {
                let col_repeat =
                    extract_attr_from_fragment(cell_attrs, "table:number-columns-repeated")
                        .and_then(|v| v.parse::<u32>().ok())
                        .unwrap_or(1)
                        .max(1);
                let explicit_row = extract_attr_from_fragment(cell_attrs, "row")
                    .and_then(|v| v.parse::<u32>().ok());
                let explicit_col = extract_attr_from_fragment(cell_attrs, "col")
                    .and_then(|v| v.parse::<u32>().ok());

                let formula_val =
                    extract_attr_from_fragment(cell_attrs, "table:formula").map(|raw_f| {
                        let unescaped = XmlSanitizer::unescape(&raw_f);
                        let stripped = unescaped
                            .strip_prefix("of:")
                            .or_else(|| unescaped.strip_prefix("oooc:"))
                            .or_else(|| unescaped.strip_prefix("msoxl:"))
                            .unwrap_or(&unescaped);
                        if stripped.starts_with('=') {
                            stripped.to_string()
                        } else {
                            format!("={stripped}")
                        }
                    });

                let text_p_bodies = extract_tag_bodies(cell_inner, "text:p");
                let office_val = extract_attr_from_fragment(cell_attrs, "office:value")
                    .or_else(|| extract_attr_from_fragment(cell_attrs, "office:string-value"))
                    .map(|s| XmlSanitizer::unescape(&s));

                let resolved_val = if let Some(f) = formula_val {
                    Some(f)
                } else if !text_p_bodies.is_empty() {
                    Some(
                        text_p_bodies
                            .into_iter()
                            .map(XmlSanitizer::unescape)
                            .collect::<Vec<_>>()
                            .join("\n"),
                    )
                } else if let Some(ov) = office_val {
                    Some(ov)
                } else if explicit_row.is_some() || explicit_col.is_some() {
                    Some(String::new())
                } else {
                    None
                };

                if resolved_val.is_some() {
                    has_any_populated = true;
                }
                parsed_row_cells.push((explicit_row, explicit_col, col_repeat, resolved_val));
            }

            if has_any_populated {
                for r_offset in 0..row_repeat {
                    let target_row = current_row + r_offset;
                    if target_row > Self::MAX_ROWS {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(
                            "ODS 행 좌표가 허용 범위를 초과했습니다.".to_string(),
                        ));
                    }
                    let mut current_col: u32 = 0;
                    for (explicit_row, explicit_col, col_repeat, resolved_val) in &parsed_row_cells
                    {
                        let next_col = current_col.checked_add(*col_repeat).ok_or_else(|| {
                            EasyOfficeError::ResourceBudgetExceeded(
                                "ODS 열 반복 횟수가 허용 범위를 초과했습니다.".to_string(),
                            )
                        })?;
                        if next_col > Self::MAX_COLS + 1 {
                            return Err(EasyOfficeError::ResourceBudgetExceeded(
                                "ODS 열 인덱스가 허용 범위를 초과했습니다.".to_string(),
                            ));
                        }
                        if let Some(val) = resolved_val {
                            for c_offset in 0..*col_repeat {
                                let r = explicit_row.unwrap_or(target_row);
                                let c = explicit_col
                                    .map(|ec| ec.saturating_add(c_offset))
                                    .unwrap_or(current_col + c_offset);
                                if r > Self::MAX_ROWS || c > Self::MAX_COLS {
                                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                                        "ODS 셀 좌표가 허용 범위를 초과했습니다.".to_string(),
                                    ));
                                }
                                if cells.len() >= Self::MAX_CELLS {
                                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                                        "ODS 셀 개수가 허용 예산(200,000)을 초과했습니다."
                                            .to_string(),
                                    ));
                                }
                                cells.push((r, c, val.clone()));
                            }
                        }
                        current_col = next_col;
                    }
                }
            }
            current_row = next_row;
        }

        Ok((sheet_name, cells))
    }

    pub fn pack_odp(
        title: &str,
        slides: &[(String, Vec<String>, String)],
    ) -> Result<Vec<u8>, EasyOfficeError> {
        let manifest = Self::build_manifest(Self::MIMETYPE_ODP);
        let mut content = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" office:version=\"1.2\" title=\"{}\"><office:body><office:presentation>",
            XmlSanitizer::escape(title)
        );
        for (s_title, bullets, notes) in slides {
            content.push_str(&format!(
                "<draw:page draw:name=\"{}\" notes=\"{}\">",
                XmlSanitizer::escape(s_title),
                XmlSanitizer::escape(notes)
            ));
            for b in bullets {
                content.push_str("<text:p>");
                content.push_str(&XmlSanitizer::escape(b));
                content.push_str("</text:p>");
            }
            content.push_str("</draw:page>");
        }
        content.push_str("</office:presentation></office:body></office:document-content>");
        XmlSanitizer::validate_safe_xml(&content)?;

        ZipArchiveCodec::pack(&[
            (
                "mimetype".to_string(),
                Self::MIMETYPE_ODP.as_bytes().to_vec(),
            ),
            ("META-INF/manifest.xml".to_string(), manifest.into_bytes()),
            ("content.xml".to_string(), content.into_bytes()),
        ])
    }

    pub fn unpack_odp(bytes: &[u8]) -> Result<OdpUnpacked, EasyOfficeError> {
        let content_xml = Self::extract_content_xml(bytes, Self::MIMETYPE_ODP)?;
        let title = extract_attr(&content_xml, "<office:document-content", "title")
            .map(|s| XmlSanitizer::unescape(&s))
            .unwrap_or_else(|| "발표자료".to_string());

        let mut slides = Vec::new();
        for (attrs, inner) in extract_tags_with_attrs(&content_xml, "draw:page") {
            let s_title = extract_attr_from_fragment(attrs, "draw:name")
                .map(|s| XmlSanitizer::unescape(&s))
                .unwrap_or_default();
            let notes = extract_attr_from_fragment(attrs, "notes")
                .map(|s| XmlSanitizer::unescape(&s))
                .unwrap_or_default();
            let bullets = extract_tag_bodies(inner, "text:p")
                .into_iter()
                .map(XmlSanitizer::unescape)
                .collect();
            slides.push((s_title, bullets, notes));
        }

        Ok((title, slides))
    }

    fn build_manifest(mime: &str) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\"><manifest:file-entry manifest:full-path=\"/\" manifest:media-type=\"{mime}\"/><manifest:file-entry manifest:full-path=\"content.xml\" manifest:media-type=\"text/xml\"/></manifest:manifest>"
        )
    }

    fn extract_content_xml(bytes: &[u8], expected_mime: &str) -> Result<String, EasyOfficeError> {
        let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())?;
        validate_zip_xml_entries(&entries)?;
        if let Some(mime_entry) = entries.iter().find(|e| e.path == "mimetype") {
            let actual_mime = std::str::from_utf8(&mime_entry.data).unwrap_or_default();
            if actual_mime.trim() != expected_mime {
                return Err(EasyOfficeError::SignatureMismatch {
                    expected: ContainerFamily::ZipOdf,
                    detail: format!(
                        "ODF mimetype 불일치 (expected: {expected_mime}, actual: {actual_mime})"
                    ),
                });
            }
        }
        let content_entry = entries
            .iter()
            .find(|e| e.path == "content.xml")
            .ok_or_else(|| {
                EasyOfficeError::CorruptedContainer(
                    "ODF 필수 엔트리 'content.xml'이 누락되었습니다.".to_string(),
                )
            })?;
        let xml = std::str::from_utf8(&content_entry.data)
            .map_err(|_| EasyOfficeError::InvalidUtf8("ODF content.xml UTF-8 오류".to_string()))?;
        XmlSanitizer::validate_safe_xml(xml)?;
        Ok(xml.to_string())
    }
}

/// KS X 6101 OWPML payload for `.hwpx` and `.hwtx`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HwpxPayload {
    pub title: String,
    pub is_template: bool,
    pub official_agency: Option<String>,
    pub approval_lines: Vec<String>,
    pub paragraphs: Vec<String>,
    pub tables: Vec<Vec<Vec<String>>>,
}

/// KS X 6101 OWPML container codec for `.hwpx` and `.hwtx`.
pub struct HwpxCodec;

impl HwpxCodec {
    pub const MIMETYPE_HWPX: &'static str = "application/hwp+zip";

    pub fn pack(payload: &HwpxPayload) -> Result<Vec<u8>, EasyOfficeError> {
        let container_xml = concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<ocf:container xmlns:ocf=\"urn:oasis:names:tc:opendocument:xmlns:container\">",
            "<ocf:rootfiles><ocf:rootfile full-path=\"Contents/content.hpf\" media-type=\"application/hwpml-package+xml\"/></ocf:rootfiles>",
            "</ocf:container>"
        );
        let content_hpf = format!(
            concat!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
                "<opf:package xmlns:opf=\"http://www.idpf.org/2007/opf/\" version=\"1.0\" unique-identifier=\"bookid\">",
                "<opf:metadata><opf:title>{}</opf:title><opf:language>ko</opf:language></opf:metadata>",
                "<opf:manifest>",
                "<opf:item id=\"header\" href=\"Contents/header.xml\" media-type=\"application/xml\"/>",
                "<opf:item id=\"section0\" href=\"Contents/section0.xml\" media-type=\"application/xml\"/>",
                "</opf:manifest>",
                "<opf:spine><opf:itemref idref=\"header\" linear=\"no\"/><opf:itemref idref=\"section0\" linear=\"yes\"/></opf:spine>",
                "</opf:package>"
            ),
            XmlSanitizer::escape(&payload.title)
        );
        let agency_attr = payload.official_agency.as_deref().unwrap_or("");
        let approval_joined = payload.approval_lines.join("|");
        let header_xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><hh:head xmlns:hh=\"http://www.owpml.org/owpml/2011/head\" version=\"1.4\" title=\"{}\" isTemplate=\"{}\" officialAgency=\"{}\" approvalLines=\"{}\"/>",
            XmlSanitizer::escape(&payload.title),
            if payload.is_template { "1" } else { "0" },
            XmlSanitizer::escape(agency_attr),
            XmlSanitizer::escape(&approval_joined)
        );

        let mut sec_xml = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><hs:sec xmlns:hs=\"http://www.owpml.org/owpml/2011/section\" xmlns:hp=\"http://www.owpml.org/owpml/2011/paragraph\">",
        );
        for p in &payload.paragraphs {
            sec_xml.push_str("<hp:p><hp:run><hp:t>");
            sec_xml.push_str(&XmlSanitizer::escape(p));
            sec_xml.push_str("</hp:t></hp:run></hp:p>");
        }
        for tbl in &payload.tables {
            sec_xml.push_str("<hp:tbl>");
            for row in tbl {
                sec_xml.push_str("<hp:tr>");
                for cell in row {
                    sec_xml.push_str("<hp:tc><hp:t>");
                    sec_xml.push_str(&XmlSanitizer::escape(cell));
                    sec_xml.push_str("</hp:t></hp:tc>");
                }
                sec_xml.push_str("</hp:tr>");
            }
            sec_xml.push_str("</hp:tbl>");
        }
        sec_xml.push_str("</hs:sec>");

        XmlSanitizer::validate_safe_xml(&content_hpf)?;
        XmlSanitizer::validate_safe_xml(&header_xml)?;
        XmlSanitizer::validate_safe_xml(&sec_xml)?;

        ZipArchiveCodec::pack(&[
            (
                "mimetype".to_string(),
                Self::MIMETYPE_HWPX.as_bytes().to_vec(),
            ),
            (
                "META-INF/container.xml".to_string(),
                container_xml.as_bytes().to_vec(),
            ),
            ("Contents/content.hpf".to_string(), content_hpf.into_bytes()),
            ("Contents/header.xml".to_string(), header_xml.into_bytes()),
            ("Contents/section0.xml".to_string(), sec_xml.into_bytes()),
        ])
    }

    pub fn unpack(bytes: &[u8]) -> Result<HwpxPayload, EasyOfficeError> {
        let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())?;
        validate_zip_xml_entries(&entries)?;
        let header_entry = entries
            .iter()
            .find(|e| e.path == "Contents/header.xml")
            .ok_or_else(|| {
                EasyOfficeError::CorruptedContainer(
                    "HWPX 필수 엔트리 'Contents/header.xml'이 누락되었습니다.".to_string(),
                )
            })?;
        let header_xml = std::str::from_utf8(&header_entry.data).map_err(|_| {
            EasyOfficeError::InvalidUtf8("Contents/header.xml UTF-8 오류".to_string())
        })?;
        XmlSanitizer::validate_safe_xml(header_xml)?;

        let title = extract_attr(header_xml, "<hh:head", "title")
            .map(|s| XmlSanitizer::unescape(&s))
            .unwrap_or_else(|| "HWPX 문서".to_string());
        let is_template = extract_attr(header_xml, "<hh:head", "isTemplate")
            .map(|s| s == "1")
            .unwrap_or(false);
        let official_agency = extract_attr(header_xml, "<hh:head", "officialAgency")
            .map(|s| XmlSanitizer::unescape(&s))
            .filter(|s| !s.is_empty());
        let approval_lines: Vec<String> = extract_attr(header_xml, "<hh:head", "approvalLines")
            .map(|s| XmlSanitizer::unescape(&s))
            .filter(|s| !s.is_empty())
            .map(|s| s.split('|').map(ToString::to_string).collect())
            .unwrap_or_default();

        let sec_entry = entries
            .iter()
            .find(|e| e.path == "Contents/section0.xml")
            .ok_or_else(|| {
                EasyOfficeError::CorruptedContainer(
                    "HWPX 필수 엔트리 'Contents/section0.xml'이 누락되었습니다.".to_string(),
                )
            })?;
        let sec_xml = std::str::from_utf8(&sec_entry.data).map_err(|_| {
            EasyOfficeError::InvalidUtf8("Contents/section0.xml UTF-8 오류".to_string())
        })?;
        XmlSanitizer::validate_safe_xml(sec_xml)?;

        let mut tables = Vec::new();
        for tbl_body in extract_tag_bodies(sec_xml, "hp:tbl") {
            let mut rows = Vec::new();
            for tr_body in extract_tag_bodies(tbl_body, "hp:tr") {
                let mut cells = Vec::new();
                for tc_body in extract_tag_bodies(tr_body, "hp:tc") {
                    let cell_text = extract_tag_bodies(tc_body, "hp:t")
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

        let sec_without_tables = strip_tag_blocks(sec_xml, "hp:tbl");
        let mut paragraphs = Vec::new();
        for p_body in extract_tag_bodies(&sec_without_tables, "hp:p") {
            let text = extract_tag_bodies(p_body, "hp:t")
                .into_iter()
                .map(XmlSanitizer::unescape)
                .collect::<Vec<_>>()
                .join("");
            paragraphs.push(text);
        }

        Ok(HwpxPayload {
            title,
            is_template,
            official_agency,
            approval_lines,
            paragraphs,
            tables,
        })
    }
}

fn validate_zip_xml_entries(entries: &[crate::ZipArchiveEntry]) -> Result<(), EasyOfficeError> {
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

pub use crate::rtf_xml_helpers::*;

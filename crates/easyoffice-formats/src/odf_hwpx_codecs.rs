use crate::office_codecs::validate_zip_xml_entries;
use crate::rtf_xml_helpers::{
    extract_attr, extract_attr_from_fragment, extract_balanced_elements,
    extract_odf_blocks_in_order, extract_odf_inline_text, extract_ods_cells_from_row,
    extract_tag_bodies, scan_single_a1_coord, strip_tag_blocks, to_open_formula_syntax_impl,
};
use crate::{ContainerFamily, EasyOfficeError, XmlSanitizer, ZipArchiveCodec, ZipSecurityPolicy};

/// Unpacked ODF text document (`(title, paragraphs, tables)`).
pub type OdtUnpacked = (String, Vec<String>, Vec<Vec<Vec<String>>>);
/// Unpacked ODF spreadsheet (`(sheet_name, cells)`).
pub type OdsUnpacked = (String, Vec<(u32, u32, String)>);
/// Unpacked multi-sheet ODF spreadsheet (`Vec<(sheet_name, cells)>`).
pub type OdsMultiUnpacked = Vec<(String, Vec<(u32, u32, String)>)>;
/// Unpacked ODF presentation (`(title, slides)`).
pub type OdpUnpacked = (String, Vec<(String, Vec<String>, String)>);

/// ISO/IEC 26300 OpenDocument Format (ODF) container codec for `.odt`, `.ods`, and `.odp`.
pub struct OdfCodec;

impl OdfCodec {
    pub const MIMETYPE_ODT: &'static str = "application/vnd.oasis.opendocument.text";
    pub const MIMETYPE_ODS: &'static str = "application/vnd.oasis.opendocument.spreadsheet";
    pub const MIMETYPE_ODP: &'static str = "application/vnd.oasis.opendocument.presentation";
    pub const MAX_CELLS: usize = 200_000;
    pub const MAX_ROWS: u32 = 1_048_575;
    pub const MAX_COLS: u32 = 16_383;

    pub fn to_open_formula_syntax(formula_without_eq: &str) -> String {
        to_open_formula_syntax_impl(formula_without_eq)
    }

    pub fn normalize_ods_formula(raw: &str) -> String {
        let trimmed = raw.trim();
        let stripped = ["of:", "oooc:", "msoxl:"]
            .into_iter()
            .find_map(|pfx| {
                trimmed
                    .get(..pfx.len())
                    .filter(|s| s.eq_ignore_ascii_case(pfx))
                    .map(|_| trimmed[pfx.len()..].trim_start())
            })
            .unwrap_or(trimmed);
        let body = stripped.strip_prefix('=').unwrap_or(stripped);
        let mut out = String::with_capacity(body.len() + 1);
        out.push('=');
        let mut in_dq = false;
        let mut in_sq = false;
        let mut chars = body.char_indices().peekable();
        while let Some((idx, ch)) = chars.next() {
            if ch == '"' && !in_sq {
                in_dq = !in_dq;
                out.push(ch);
            } else if ch == '\'' && !in_dq {
                if in_sq && chars.peek().is_some_and(|&(_, n)| n == '\'') {
                    out.push_str("''");
                    chars.next();
                } else {
                    in_sq = !in_sq;
                    out.push(ch);
                }
            } else if !in_dq && !in_sq && ch == '[' {
                let start = idx + ch.len_utf8();
                let mut close_idx = None;
                let mut b_sq = false;
                while let Some(&(c_idx, c_ch)) = chars.peek() {
                    chars.next();
                    if c_ch == '\'' {
                        b_sq = !b_sq;
                    } else if c_ch == ']' && !b_sq {
                        close_idx = Some(c_idx);
                        break;
                    }
                }
                if let Some(end) = close_idx {
                    out.push_str(&Self::convert_openformula_bracket_inner(&body[start..end]));
                } else {
                    out.push('[');
                    out.push_str(&body[start..]);
                    break;
                }
            } else {
                out.push(ch);
            }
        }
        out
    }

    fn convert_openformula_bracket_inner<'a>(inner: &'a str) -> String {
        let is_simple_ident = |s: &str| {
            s.chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_')
                && s.chars().all(|c| c.is_alphanumeric() || c == '_')
                && !s.eq_ignore_ascii_case("TRUE")
                && !s.eq_ignore_ascii_case("FALSE")
                && scan_single_a1_coord(s.as_bytes()).is_none()
        };
        let split_ep = |ep: &'a str| -> (Option<String>, &'a str) {
            let ep = ep.trim();
            if let Some(rest) = ep.strip_prefix('.') {
                return (None, rest.trim());
            }
            let (mut in_sq, mut last_dot) = (false, None);
            for (idx, ch) in ep.char_indices() {
                if ch == '\'' {
                    in_sq = !in_sq;
                } else if ch == '.' && !in_sq {
                    last_dot = Some(idx);
                }
            }
            let Some(dot) = last_dot else {
                return (None, ep);
            };
            let raw_sh = ep[..dot].trim().trim_start_matches('$');
            let cell = ep[dot + 1..].trim();
            if raw_sh.is_empty() {
                return (None, cell);
            }
            let fmt = if raw_sh.len() >= 2 && raw_sh.starts_with('\'') && raw_sh.ends_with('\'') {
                let unq = &raw_sh[1..raw_sh.len() - 1];
                if is_simple_ident(unq) {
                    unq.to_string()
                } else {
                    raw_sh.to_string()
                }
            } else if is_simple_ident(raw_sh) {
                raw_sh.to_string()
            } else {
                format!("'{}'", raw_sh.replace('\'', "''"))
            };
            (Some(fmt), cell)
        };
        let (mut in_sq, mut colon) = (false, None);
        for (idx, ch) in inner.char_indices() {
            if ch == '\'' {
                in_sq = !in_sq;
            } else if ch == ':' && !in_sq {
                colon = Some(idx);
                break;
            }
        }
        if let Some(c_idx) = colon {
            let (ls, lc) = split_ep(&inner[..c_idx]);
            let (rs, rc) = split_ep(&inner[c_idx + 1..]);
            let lp = ls
                .as_ref()
                .map_or_else(|| lc.to_string(), |s| format!("{s}!{lc}"));
            let rp = match (&ls, &rs) {
                (_, None) => rc.to_string(),
                (Some(l), Some(r)) if l == r => rc.to_string(),
                (_, Some(r)) => format!("{r}!{rc}"),
            };
            format!("{lp}:{rp}")
        } else {
            let (sh, c) = split_ep(inner);
            sh.map_or_else(|| c.to_string(), |s| format!("{s}!{c}"))
        }
    }

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
        let (content_xml, meta_xml_opt) =
            Self::extract_content_and_meta_xml(bytes, Self::MIMETYPE_ODT)?;
        let title = extract_attr(&content_xml, "<office:document-content", "title")
            .map(|s| XmlSanitizer::unescape(&s))
            .filter(|s| !s.is_empty())
            .or_else(|| {
                meta_xml_opt
                    .as_deref()
                    .and_then(|m| extract_tag_bodies(m, "dc:title").into_iter().next())
                    .map(XmlSanitizer::unescape)
                    .filter(|s| !s.is_empty())
            })
            .or_else(|| {
                extract_tag_bodies(&content_xml, "dc:title")
                    .into_iter()
                    .next()
                    .map(XmlSanitizer::unescape)
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_else(|| "ODT 문서".to_string());

        let mut tables = Vec::new();
        for tbl_body in extract_tag_bodies(&content_xml, "table:table") {
            let mut rows = Vec::new();
            for row_body in extract_tag_bodies(tbl_body, "table:table-row") {
                let mut cells = Vec::new();
                for cell_body in extract_tag_bodies(row_body, "table:table-cell") {
                    let cell_blocks = extract_odf_blocks_in_order(cell_body, &["text:p", "text:h"]);
                    let cell_text = if !cell_blocks.is_empty() {
                        cell_blocks
                            .into_iter()
                            .map(|(_, _, b)| extract_odf_inline_text(b))
                            .collect::<Vec<_>>()
                            .join("\n")
                    } else {
                        extract_odf_inline_text(cell_body)
                    };
                    cells.push(cell_text);
                }
                rows.push(cells);
            }
            tables.push(rows);
        }

        let without_tables = strip_tag_blocks(&content_xml, "table:table");
        let paragraphs = extract_odf_blocks_in_order(&without_tables, &["text:p", "text:h"])
            .into_iter()
            .map(|(_, _, body)| extract_odf_inline_text(body))
            .collect();

        Ok((title, paragraphs, tables))
    }

    pub fn pack_ods(
        sheet_name: &str,
        cells: &[(u32, u32, String)],
    ) -> Result<Vec<u8>, EasyOfficeError> {
        Self::pack_ods_multi(&[(sheet_name.to_string(), cells.to_vec())])
    }

    pub fn pack_ods_multi(sheets: &[OdsUnpacked]) -> Result<Vec<u8>, EasyOfficeError> {
        let mut total_cells = 0usize;
        for (_, cells) in sheets {
            total_cells = total_cells.saturating_add(cells.len());
            if total_cells > Self::MAX_CELLS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "ODS 셀 개수가 허용 예산을 초과했습니다.".to_string(),
                ));
            }
            for (row, col, _) in cells {
                if *row > Self::MAX_ROWS || *col > Self::MAX_COLS {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "ODS 셀 좌표가 허용 범위를 초과했습니다.".to_string(),
                    ));
                }
            }
        }

        let manifest = Self::build_manifest(Self::MIMETYPE_ODS);
        let mut content = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" office:version=\"1.2\"><office:body><office:spreadsheet>",
        );

        let default_sheet = vec![("Sheet1".to_string(), Vec::new())];
        let active_sheets = if sheets.is_empty() {
            default_sheet.as_slice()
        } else {
            sheets
        };

        for (sheet_name, cells) in active_sheets {
            let mut grid: std::collections::BTreeMap<
                u32,
                std::collections::BTreeMap<u32, &String>,
            > = std::collections::BTreeMap::new();
            for (row, col, raw) in cells {
                grid.entry(*row).or_default().insert(*col, raw);
            }

            content.push_str(&format!(
                "<table:table table:name=\"{}\">",
                XmlSanitizer::escape(sheet_name)
            ));

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
                        let of_expr = Self::to_open_formula_syntax(formula);
                        let escaped_formula = XmlSanitizer::escape(&of_expr);
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
            content.push_str("</table:table>");
        }

        content.push_str("</office:spreadsheet></office:body></office:document-content>");
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
        let mut sheets = Self::unpack_ods_multi(bytes)?;
        if sheets.is_empty() {
            Ok(("Sheet1".to_string(), Vec::new()))
        } else {
            Ok(sheets.remove(0))
        }
    }

    pub fn unpack_ods_multi(bytes: &[u8]) -> Result<OdsMultiUnpacked, EasyOfficeError> {
        let (content_xml, _) = Self::extract_content_and_meta_xml(bytes, Self::MIMETYPE_ODS)?;
        let table_blocks = extract_balanced_elements(&content_xml, "table:table");
        if table_blocks.is_empty() {
            return Ok(vec![("Sheet1".to_string(), Vec::new())]);
        }

        let mut total_cells_across_sheets = 0usize;
        let mut all_sheets = Vec::with_capacity(table_blocks.len());

        for (idx, (tbl_attrs, tbl_inner)) in table_blocks.into_iter().enumerate() {
            let sheet_name = extract_attr_from_fragment(tbl_attrs, "table:name")
                .or_else(|| extract_attr_from_fragment(tbl_attrs, "name"))
                .map(|s| XmlSanitizer::unescape(&s))
                .unwrap_or_else(|| format!("Sheet{}", idx + 1));

            let mut cells = Vec::new();
            let mut current_row: u32 = 0;

            for (row_attrs, row_inner) in extract_balanced_elements(tbl_inner, "table:table-row") {
                let row_repeat =
                    extract_attr_from_fragment(row_attrs, "table:number-rows-repeated")
                        .and_then(|v| v.parse::<u32>().ok())
                        .unwrap_or(1)
                        .max(1);
                if row_repeat > Self::MAX_ROWS + 1 {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "ODS 행 반복 횟수가 허용 범위를 초과했습니다.".to_string(),
                    ));
                }

                type ParsedOdsRowCell = (Option<u32>, Option<u32>, u32, Option<String>);
                let raw_cells = extract_ods_cells_from_row(row_inner);
                let mut parsed_row_cells: Vec<ParsedOdsRowCell> = Vec::new();
                let mut last_populated_idx: Option<usize> = None;

                for (cell_idx, (is_covered, cell_attrs, cell_inner)) in
                    raw_cells.into_iter().enumerate()
                {
                    let col_repeat =
                        extract_attr_from_fragment(cell_attrs, "table:number-columns-repeated")
                            .and_then(|v| v.parse::<u32>().ok())
                            .unwrap_or(1)
                            .max(1);
                    if col_repeat > Self::MAX_COLS + 1 {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(
                            "ODS 열 반복 횟수가 허용 범위를 초과했습니다.".to_string(),
                        ));
                    }
                    if is_covered {
                        parsed_row_cells.push((None, None, col_repeat, None));
                        continue;
                    }

                    let explicit_row = extract_attr_from_fragment(cell_attrs, "row")
                        .and_then(|v| v.parse::<u32>().ok());
                    let explicit_col = extract_attr_from_fragment(cell_attrs, "col")
                        .and_then(|v| v.parse::<u32>().ok());

                    let formula_val =
                        extract_attr_from_fragment(cell_attrs, "table:formula").map(|raw_f| {
                            let unescaped = XmlSanitizer::unescape(&raw_f);
                            Self::normalize_ods_formula(&unescaped)
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
                                .map(extract_odf_inline_text)
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
                        last_populated_idx = Some(cell_idx);
                    }
                    parsed_row_cells.push((explicit_row, explicit_col, col_repeat, resolved_val));
                }

                if let Some(last_pop) = last_populated_idx {
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
                    for r_offset in 0..row_repeat {
                        let target_row = current_row + r_offset;
                        if target_row > Self::MAX_ROWS {
                            return Err(EasyOfficeError::ResourceBudgetExceeded(
                                "ODS 행 좌표가 허용 범위를 초과했습니다.".to_string(),
                            ));
                        }
                        let mut current_col: u32 = 0;
                        for (c_idx, (explicit_row, explicit_col, col_repeat, resolved_val)) in
                            parsed_row_cells.iter().enumerate()
                        {
                            let next_col =
                                current_col.checked_add(*col_repeat).ok_or_else(|| {
                                    EasyOfficeError::ResourceBudgetExceeded(
                                        "ODS 열 반복 횟수가 허용 범위를 초과했습니다.".to_string(),
                                    )
                                })?;
                            if c_idx <= last_pop && next_col > Self::MAX_COLS + 1 {
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
                                    total_cells_across_sheets += 1;
                                    if total_cells_across_sheets > Self::MAX_CELLS {
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
                    current_row = next_row;
                } else {
                    for (_, _, col_repeat, _) in &parsed_row_cells {
                        if *col_repeat > Self::MAX_COLS + 1 {
                            return Err(EasyOfficeError::ResourceBudgetExceeded(
                                "ODS 열 인덱스가 허용 범위를 초과했습니다.".to_string(),
                            ));
                        }
                    }
                    current_row = current_row.saturating_add(row_repeat);
                }
            }

            all_sheets.push((sheet_name, cells));
        }

        Ok(all_sheets)
    }

    pub fn pack_odp(
        title: &str,
        slides: &[(String, Vec<String>, String)],
    ) -> Result<Vec<u8>, EasyOfficeError> {
        let manifest = Self::build_manifest(Self::MIMETYPE_ODP);
        let mut content = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" xmlns:presentation=\"urn:oasis:names:tc:opendocument:xmlns:presentation:1.0\" office:version=\"1.2\" title=\"{}\"><office:body><office:presentation>",
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
        let (content_xml, meta_xml_opt) =
            Self::extract_content_and_meta_xml(bytes, Self::MIMETYPE_ODP)?;
        let title = extract_attr(&content_xml, "<office:document-content", "title")
            .map(|s| XmlSanitizer::unescape(&s))
            .filter(|s| !s.is_empty())
            .or_else(|| {
                meta_xml_opt
                    .as_deref()
                    .and_then(|m| extract_tag_bodies(m, "dc:title").into_iter().next())
                    .map(XmlSanitizer::unescape)
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_else(|| "발표자료".to_string());

        let mut slides = Vec::new();
        for (attrs, inner) in extract_balanced_elements(&content_xml, "draw:page") {
            let mut notes = extract_attr_from_fragment(attrs, "notes")
                .map(|s| XmlSanitizer::unescape(&s))
                .unwrap_or_default();

            let notes_blocks = extract_balanced_elements(inner, "presentation:notes");
            if notes.is_empty() && !notes_blocks.is_empty() {
                let mut note_lines = Vec::new();
                for (_, n_inner) in &notes_blocks {
                    let cleaned = strip_tag_blocks(n_inner, "text:page-number");
                    for (_, _, p_body) in
                        extract_odf_blocks_in_order(&cleaned, &["text:p", "text:h"])
                    {
                        let txt = extract_odf_inline_text(p_body);
                        if !txt.trim().is_empty() {
                            note_lines.push(txt);
                        }
                    }
                }
                notes = note_lines.join("\n");
            }

            let without_notes = strip_tag_blocks(inner, "presentation:notes");
            let without_page_num = strip_tag_blocks(&without_notes, "text:page-number");

            let mut frame_title: Option<String> = None;
            for (f_attrs, f_inner) in extract_balanced_elements(&without_page_num, "draw:frame") {
                if extract_attr_from_fragment(f_attrs, "presentation:class").as_deref()
                    == Some("title")
                {
                    let t_parts: Vec<String> =
                        extract_odf_blocks_in_order(f_inner, &["text:p", "text:h"])
                            .into_iter()
                            .map(|(_, _, b)| extract_odf_inline_text(b))
                            .filter(|s| !s.is_empty())
                            .collect();
                    if !t_parts.is_empty() {
                        frame_title = Some(t_parts.join(" "));
                        break;
                    }
                }
            }

            let attr_title = extract_attr_from_fragment(attrs, "draw:name")
                .or_else(|| extract_attr_from_fragment(attrs, "title"))
                .map(|s| XmlSanitizer::unescape(&s))
                .unwrap_or_default();

            let mut bullets: Vec<String> =
                extract_odf_blocks_in_order(&without_notes, &["text:p", "text:h"])
                    .into_iter()
                    .filter_map(|(_, _, b)| {
                        let had_page_num = b.contains("<text:page-number");
                        let cleaned = strip_tag_blocks(b, "text:page-number");
                        let txt = extract_odf_inline_text(&cleaned);
                        if had_page_num && txt.trim().is_empty() {
                            None
                        } else {
                            Some(txt)
                        }
                    })
                    .collect();

            let s_title = if let Some(ft) = frame_title {
                if bullets.first() == Some(&ft) {
                    bullets.remove(0);
                }
                ft
            } else {
                attr_title
            };

            slides.push((s_title, bullets, notes));
        }

        Ok((title, slides))
    }

    fn build_manifest(mime: &str) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\"><manifest:file-entry manifest:full-path=\"/\" manifest:media-type=\"{mime}\"/><manifest:file-entry manifest:full-path=\"content.xml\" manifest:media-type=\"text/xml\"/></manifest:manifest>"
        )
    }

    fn extract_content_and_meta_xml(
        bytes: &[u8],
        expected_mime: &str,
    ) -> Result<(String, Option<String>), EasyOfficeError> {
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

        let meta_xml = if let Some(m_entry) = entries.iter().find(|e| e.path == "meta.xml") {
            let m_str = std::str::from_utf8(&m_entry.data)
                .map_err(|_| EasyOfficeError::InvalidUtf8("ODF meta.xml UTF-8 오류".to_string()))?;
            XmlSanitizer::validate_safe_xml(m_str)?;
            Some(m_str.to_string())
        } else {
            None
        };

        Ok((xml.to_string(), meta_xml))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HwpxPayload {
    pub title: String,
    pub is_template: bool,
    pub official_agency: Option<String>,
    pub approval_lines: Vec<String>,
    pub paragraphs: Vec<String>,
    pub tables: Vec<Vec<Vec<String>>>,
}

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

        let hpf_title = entries
            .iter()
            .find(|e| e.path == "Contents/content.hpf")
            .and_then(|e| std::str::from_utf8(&e.data).ok())
            .and_then(|xml| extract_tag_bodies(xml, "opf:title").into_iter().next())
            .map(XmlSanitizer::unescape)
            .filter(|s| !s.is_empty());

        let title = extract_attr(header_xml, "<hh:head", "title")
            .map(|s| XmlSanitizer::unescape(&s))
            .filter(|s| !s.is_empty())
            .or(hpf_title)
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

        let mut sec_entries: Vec<(usize, &crate::ZipArchiveEntry)> = entries
            .iter()
            .filter_map(|e| {
                let idx = e
                    .path
                    .strip_prefix("Contents/section")
                    .and_then(|s| s.strip_suffix(".xml"))
                    .and_then(|s| s.parse::<usize>().ok())?;
                Some((idx, e))
            })
            .collect();
        sec_entries.sort_by_key(|(idx, _)| *idx);

        if sec_entries.is_empty() {
            return Err(EasyOfficeError::CorruptedContainer(
                "HWPX 필수 엔트리 'Contents/section0.xml'이 누락되었습니다.".to_string(),
            ));
        }

        let mut tables = Vec::new();
        let mut paragraphs = Vec::new();

        for (_, sec_entry) in sec_entries {
            let sec_xml = std::str::from_utf8(&sec_entry.data).map_err(|_| {
                EasyOfficeError::InvalidUtf8("Contents/section.xml UTF-8 오류".to_string())
            })?;
            XmlSanitizer::validate_safe_xml(sec_xml)?;

            for tbl_body in extract_tag_bodies(sec_xml, "hp:tbl") {
                tables.push(extract_hwpx_table_rows(tbl_body)?);
                let tbl_without_tc = strip_tag_blocks(tbl_body, "hp:tc");
                append_hwpx_equations(&tbl_without_tc, &mut paragraphs)?;
            }

            let sec_without_tables = strip_tag_blocks(sec_xml, "hp:tbl");
            append_hwpx_paragraphs_and_eqs(&sec_without_tables, &mut paragraphs)?;
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

fn extract_hwpx_paragraphs_and_eqs(
    p_body: &str,
    out: &mut Vec<String>,
) -> Result<(), EasyOfficeError> {
    let p_without_eq = strip_tag_blocks(p_body, "hp:equation");
    let text = extract_tag_bodies(&p_without_eq, "hp:t")
        .into_iter()
        .map(XmlSanitizer::unescape)
        .collect::<Vec<_>>()
        .join("");
    if !text.is_empty() || !p_body.contains("<hp:equation") {
        out.push(text);
    }
    append_hwpx_equations(p_body, out)
}

fn append_hwpx_paragraphs_and_eqs(xml: &str, out: &mut Vec<String>) -> Result<(), EasyOfficeError> {
    for p_body in extract_tag_bodies(xml, "hp:p") {
        extract_hwpx_paragraphs_and_eqs(p_body, out)?;
    }
    let without_p = strip_tag_blocks(xml, "hp:p");
    append_hwpx_equations(&without_p, out)
}

fn extract_hwpx_cell_text(tc_body: &str) -> Result<String, EasyOfficeError> {
    let cell_paras = extract_tag_bodies(tc_body, "hp:p");
    let mut parts = Vec::new();
    if !cell_paras.is_empty() {
        for p in cell_paras {
            extract_hwpx_paragraphs_and_eqs(p, &mut parts)?;
        }
        let tc_no_p = strip_tag_blocks(tc_body, "hp:p");
        append_hwpx_equations(&tc_no_p, &mut parts)?;
    } else {
        let tc_no_eq = strip_tag_blocks(tc_body, "hp:equation");
        let direct_t = extract_tag_bodies(&tc_no_eq, "hp:t");
        let text_nodes = if !direct_t.is_empty() {
            direct_t
        } else {
            extract_tag_bodies(&tc_no_eq, "hp:cellText")
        };
        let t = text_nodes
            .into_iter()
            .map(XmlSanitizer::unescape)
            .collect::<Vec<_>>()
            .join("");
        if !t.is_empty() || !tc_body.contains("<hp:equation") {
            parts.push(t);
        }
        append_hwpx_equations(tc_body, &mut parts)?;
    }
    Ok(parts.join("\n"))
}

fn extract_hwpx_table_rows(tbl_body: &str) -> Result<Vec<Vec<String>>, EasyOfficeError> {
    let mut rows = Vec::new();
    let tr_bodies = extract_tag_bodies(tbl_body, "hp:tr");
    let row_slices = if !tr_bodies.is_empty() {
        tr_bodies
    } else {
        vec![tbl_body]
    };
    for tr_body in row_slices {
        let mut cells = Vec::new();
        for tc_body in extract_tag_bodies(tr_body, "hp:tc") {
            cells.push(extract_hwpx_cell_text(tc_body)?);
        }
        rows.push(cells);
    }
    Ok(rows)
}

fn append_hwpx_equations(xml: &str, out: &mut Vec<String>) -> Result<(), EasyOfficeError> {
    for eq_body in extract_tag_bodies(xml, "hp:equation") {
        for raw_script in extract_tag_bodies(eq_body, "hp:script") {
            let script = XmlSanitizer::unescape(raw_script);
            let trimmed = script.trim();
            if !trimmed.is_empty() {
                let latex = crate::hwp_eqedit::convert_eqedit_to_latex(trimmed)?;
                out.push(format!("$${latex}$$"));
            }
        }
    }
    Ok(())
}

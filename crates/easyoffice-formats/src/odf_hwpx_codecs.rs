use crate::office_codecs::validate_zip_xml_entries;
use crate::rtf_xml_helpers::{
    extract_attr, extract_attr_from_fragment, extract_balanced_elements,
    extract_odf_blocks_in_order, extract_odf_inline_text, extract_ods_cells_from_row,
    extract_tag_bodies, scan_single_a1_coord, strip_tag_blocks, to_open_formula_syntax_impl,
};
use crate::sheet_styling::ods_styles::{parse_ods_styling, table_columns_xml, OdsStyleTable};
use crate::sheet_styling::SheetStyling;
use crate::{ContainerFamily, EasyOfficeError, XmlSanitizer, ZipArchiveCodec, ZipSecurityPolicy};

/// Unpacked ODF text document (`(title, paragraphs, tables)`).
pub type OdtUnpacked = (String, Vec<String>, Vec<Vec<Vec<String>>>);
/// Unpacked ODF spreadsheet (`(sheet_name, cells)`).
pub type OdsUnpacked = (String, Vec<(u32, u32, String)>);
/// Unpacked multi-sheet ODF spreadsheet (`Vec<(sheet_name, cells)>`).
pub type OdsMultiUnpacked = Vec<(String, Vec<(u32, u32, String)>)>;

/// Appends the empty-row filler for a gap of `gap` rows.
fn push_empty_rows(content: &mut String, gap: u32) {
    if gap == 1 {
        content.push_str("<table:table-row><table:table-cell/></table:table-row>");
    } else {
        content.push_str(&format!(
            "<table:table-row table:number-rows-repeated=\"{gap}\"><table:table-cell/></table:table-row>"
        ));
    }
}

/// Appends the empty-cell filler for a gap of `gap` columns.
fn push_empty_cells(content: &mut String, gap: u32) {
    if gap == 1 {
        content.push_str("<table:table-cell/>");
    } else {
        content.push_str(&format!(
            "<table:table-cell table:number-columns-repeated=\"{gap}\"/>"
        ));
    }
}

/// One parsed ODS cell: explicit row, explicit column, column repeat and resolved text.
type ParsedOdsRowCell = (Option<u32>, Option<u32>, u32, Option<String>);

fn ods_budget_error(message: &str) -> EasyOfficeError {
    EasyOfficeError::ResourceBudgetExceeded(message.to_string())
}

fn parse_u32_attr(attrs: &str, name: &str) -> Option<u32> {
    extract_attr_from_fragment(attrs, name).and_then(|v| v.parse::<u32>().ok())
}

/// Repeat count attribute (defaults to 1, minimum 1).
fn parse_repeat_attr(attrs: &str, name: &str) -> u32 {
    parse_u32_attr(attrs, name).unwrap_or(1).max(1)
}

/// Unpacked ODF presentation (`(title, slides)`).
pub type OdpUnpacked = (String, Vec<(String, Vec<String>, String)>);

/// ISO/IEC 26300 OpenDocument Format (ODF) container codec for `.odt`, `.ods`, and `.odp`.
pub struct OdfCodec;

impl OdfCodec {
    pub const MIMETYPE_ODT: &str = "application/vnd.oasis.opendocument.text";
    pub const MIMETYPE_ODS: &str = "application/vnd.oasis.opendocument.spreadsheet";
    pub const MIMETYPE_ODP: &str = "application/vnd.oasis.opendocument.presentation";
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

        ZipArchiveCodec::pack_compressed(&[
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
            .map(XmlSanitizer::unescape)
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
        Self::pack_ods_multi_styled(sheets, &[])
    }

    /// Like [`Self::pack_ods_multi`], additionally writing `stylings[i]` (cell formatting and
    /// column widths, PD-089) for sheet `i` as automatic cell and column styles.
    pub fn pack_ods_multi_styled(
        sheets: &[OdsUnpacked],
        stylings: &[SheetStyling],
    ) -> Result<Vec<u8>, EasyOfficeError> {
        Self::pack_ods_multi_styled_with_cache(sheets, &[], stylings)
    }

    /// Like [`Self::pack_ods_multi_styled`], additionally writing evaluated formula cached values.
    pub fn pack_ods_multi_styled_with_cache(
        sheets: &[OdsUnpacked],
        caches: &[crate::XlsxFormulaCache],
        stylings: &[SheetStyling],
    ) -> Result<Vec<u8>, EasyOfficeError> {
        Self::validate_ods_bounds(sheets, stylings)?;
        let style_table = OdsStyleTable::build(stylings);

        let manifest = Self::build_manifest(Self::MIMETYPE_ODS);
        let mut content = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" office:version=\"1.2\">",
        );
        content.push_str(&style_table.automatic_styles_xml());
        content.push_str("<office:body><office:spreadsheet>");

        let default_sheet = vec![("Sheet1".to_string(), Vec::new())];
        let active_sheets = if sheets.is_empty() {
            default_sheet.as_slice()
        } else {
            sheets
        };
        for (sheet_idx, (sheet_name, cells)) in active_sheets.iter().enumerate() {
            Self::push_ods_table(
                &mut content,
                sheet_name,
                cells,
                caches.get(sheet_idx),
                stylings.get(sheet_idx),
                &style_table,
            );
        }

        content.push_str("</office:spreadsheet></office:body></office:document-content>");
        XmlSanitizer::validate_safe_xml(&content)?;

        ZipArchiveCodec::pack_compressed(&[
            (
                "mimetype".to_string(),
                Self::MIMETYPE_ODS.as_bytes().to_vec(),
            ),
            ("META-INF/manifest.xml".to_string(), manifest.into_bytes()),
            ("content.xml".to_string(), content.into_bytes()),
        ])
    }

    /// Rejects cell / style coordinates and counts above the ODS budgets.
    fn validate_ods_bounds(
        sheets: &[OdsUnpacked],
        stylings: &[SheetStyling],
    ) -> Result<(), EasyOfficeError> {
        let mut total_cells = 0usize;
        for (_, cells) in sheets {
            total_cells = total_cells.saturating_add(cells.len());
            if total_cells > Self::MAX_CELLS {
                return Err(ods_budget_error("ODS 셀 개수가 허용 예산을 초과했습니다."));
            }
            if cells
                .iter()
                .any(|(row, col, _)| *row > Self::MAX_ROWS || *col > Self::MAX_COLS)
            {
                return Err(ods_budget_error("ODS 셀 좌표가 허용 범위를 초과했습니다."));
            }
        }
        for styling in stylings {
            let bad_cell = styling
                .cells
                .keys()
                .any(|&(r, c)| r > Self::MAX_ROWS || c > Self::MAX_COLS);
            if bad_cell || styling.columns.keys().any(|&c| c > Self::MAX_COLS) {
                return Err(ods_budget_error(
                    "ODS 서식 좌표가 허용 범위를 초과했습니다.",
                ));
            }
        }
        Ok(())
    }

    /// Appends one `<table:table>` with sparse-row / sparse-column gap elements.
    fn push_ods_table(
        content: &mut String,
        sheet_name: &str,
        cells: &[(u32, u32, String)],
        cache: Option<&crate::XlsxFormulaCache>,
        styling: Option<&SheetStyling>,
        style_table: &OdsStyleTable,
    ) {
        let blank = String::new();
        let mut grid: std::collections::BTreeMap<u32, std::collections::BTreeMap<u32, &String>> =
            std::collections::BTreeMap::new();
        for (row, col, raw) in cells {
            grid.entry(*row).or_default().insert(*col, raw);
        }
        // Styled cells without a value are still written (`<table:table-cell table:style-name=".."/>`).
        if let Some(st) = styling {
            for (&(row, col), style) in &st.cells {
                if !style.is_plain() {
                    grid.entry(row).or_default().entry(col).or_insert(&blank);
                }
            }
        }
        content.push_str(&format!(
            "<table:table table:name=\"{}\">",
            XmlSanitizer::escape(sheet_name)
        ));
        if let Some(st) = styling {
            content.push_str(&table_columns_xml(&st.columns, style_table));
        }
        let mut expected_row: u32 = 0;
        for (row, row_cells) in grid {
            if row > expected_row {
                push_empty_rows(content, row - expected_row);
            }
            content.push_str("<table:table-row>");
            let mut expected_col: u32 = 0;
            for (col, raw) in row_cells {
                if col > expected_col {
                    push_empty_cells(content, col - expected_col);
                }
                let style_attr = styling
                    .and_then(|st| st.cells.get(&(row, col)))
                    .and_then(|style| style_table.cell_style_name(style))
                    .map_or(String::new(), |name| {
                        format!(" table:style-name=\"{name}\"")
                    });
                let cached = cache.and_then(|c| c.get(&(row, col)));
                Self::push_ods_cell(content, (row, col), raw, cached, &style_attr);
                expected_col = col.saturating_add(1);
            }
            content.push_str("</table:table-row>");
            expected_row = row.saturating_add(1);
        }
        content.push_str("</table:table>");
    }

    fn push_ods_cell(
        content: &mut String,
        (row, col): (u32, u32),
        raw: &str,
        cached: Option<&crate::XlsxCachedValue>,
        style_attr: &str,
    ) {
        if raw.is_empty() {
            content.push_str(&format!("<table:table-cell{style_attr}/>"));
        } else if let Some(formula) = raw.strip_prefix('=') {
            let of_expr = Self::to_open_formula_syntax(formula);
            let escaped_formula = XmlSanitizer::escape(&of_expr);
            match cached {
                Some(crate::XlsxCachedValue::Number(n)) if n.is_finite() => {
                    let disp = crate::LegacyOle2Codec::format_biff_number(*n);
                    content.push_str(&format!(
                        "<table:table-cell row=\"{row}\" col=\"{col}\"{style_attr} table:formula=\"of:={escaped_formula}\" office:value-type=\"float\" office:value=\"{n}\"><text:p>{disp}</text:p></table:table-cell>"
                    ));
                }
                Some(crate::XlsxCachedValue::Boolean(b)) => {
                    let disp = if *b { "TRUE" } else { "FALSE" };
                    let val = if *b { "true" } else { "false" };
                    content.push_str(&format!(
                        "<table:table-cell row=\"{row}\" col=\"{col}\"{style_attr} table:formula=\"of:={escaped_formula}\" office:value-type=\"boolean\" office:boolean-value=\"{val}\"><text:p>{disp}</text:p></table:table-cell>"
                    ));
                }
                Some(crate::XlsxCachedValue::Text(text)) => {
                    content.push_str(&format!(
                        "<table:table-cell row=\"{row}\" col=\"{col}\"{style_attr} table:formula=\"of:={escaped_formula}\" office:value-type=\"string\"><text:p>{}</text:p></table:table-cell>",
                        XmlSanitizer::escape(text)
                    ));
                }
                _ => {
                    content.push_str(&format!(
                        "<table:table-cell row=\"{row}\" col=\"{col}\"{style_attr} table:formula=\"of:={escaped_formula}\" office:value-type=\"string\"><text:p>{}</text:p></table:table-cell>",
                        XmlSanitizer::escape(raw)
                    ));
                }
            }
        } else if crate::LegacyOle2Codec::parse_finite_f64(raw).is_some() {
            let escaped_val = XmlSanitizer::escape(raw);
            content.push_str(&format!(
                "<table:table-cell row=\"{row}\" col=\"{col}\"{style_attr} office:value-type=\"float\" office:value=\"{escaped_val}\"><text:p>{escaped_val}</text:p></table:table-cell>"
            ));
        } else {
            content.push_str(&format!(
                "<table:table-cell row=\"{row}\" col=\"{col}\"{style_attr} office:value-type=\"string\"><text:p>{}</text:p></table:table-cell>",
                XmlSanitizer::escape(raw)
            ));
        }
    }

    pub fn unpack_ods(bytes: &[u8]) -> Result<OdsUnpacked, EasyOfficeError> {
        let mut sheets = Self::unpack_ods_multi(bytes)?;
        if sheets.is_empty() {
            Ok(("Sheet1".to_string(), Vec::new()))
        } else {
            Ok(sheets.remove(0))
        }
    }

    /// Cell formatting and column widths of every table, keyed by sheet name (PD-089).
    pub fn unpack_ods_styling(
        bytes: &[u8],
    ) -> Result<Vec<(String, SheetStyling)>, EasyOfficeError> {
        let (content_xml, _) = Self::extract_content_and_meta_xml(bytes, Self::MIMETYPE_ODS)?;
        Ok(parse_ods_styling(&content_xml))
    }

    pub fn unpack_ods_multi(bytes: &[u8]) -> Result<OdsMultiUnpacked, EasyOfficeError> {
        let (content_xml, _) = Self::extract_content_and_meta_xml(bytes, Self::MIMETYPE_ODS)?;
        Self::parse_ods_multi_xml(&content_xml)
    }

    /// Unpacks all ODS tables together with their cell and column styling in a single ZIP pass.
    pub fn unpack_ods_with_styling(
        bytes: &[u8],
    ) -> Result<(OdsMultiUnpacked, Vec<(String, SheetStyling)>), EasyOfficeError> {
        let (content_xml, _) = Self::extract_content_and_meta_xml(bytes, Self::MIMETYPE_ODS)?;
        let sheets = Self::parse_ods_multi_xml(&content_xml)?;
        let styling = parse_ods_styling(&content_xml);
        Ok((sheets, styling))
    }

    fn parse_ods_multi_xml(content_xml: &str) -> Result<OdsMultiUnpacked, EasyOfficeError> {
        let table_blocks = extract_balanced_elements(content_xml, "table:table");
        if table_blocks.is_empty() {
            return Ok(vec![("Sheet1".to_string(), Vec::new())]);
        }

        let mut total_cells_across_sheets = 0usize;
        let mut all_sheets = Vec::with_capacity(table_blocks.len());
        for (idx, (tbl_attrs, tbl_inner)) in table_blocks.into_iter().enumerate() {
            let sheet_name = extract_attr_from_fragment(tbl_attrs, "table:name")
                .or_else(|| extract_attr_from_fragment(tbl_attrs, "name"))
                .map(XmlSanitizer::unescape)
                .unwrap_or_else(|| format!("Sheet{}", idx + 1));
            let cells = Self::parse_ods_sheet_cells(tbl_inner, &mut total_cells_across_sheets)?;
            all_sheets.push((sheet_name, cells));
        }
        Ok(all_sheets)
    }

    /// Parses all rows of one ODS table into `(row, col, text)` cells within the shared budget.
    fn parse_ods_sheet_cells(
        tbl_inner: &str,
        total_cells: &mut usize,
    ) -> Result<Vec<(u32, u32, String)>, EasyOfficeError> {
        let mut cells = Vec::new();
        let mut current_row: u32 = 0;
        for (row_attrs, row_inner) in extract_balanced_elements(tbl_inner, "table:table-row") {
            let row_repeat = parse_repeat_attr(row_attrs, "table:number-rows-repeated");
            if row_repeat > Self::MAX_ROWS + 1 {
                return Err(ods_budget_error(
                    "ODS 행 반복 횟수가 허용 범위를 초과했습니다.",
                ));
            }
            let (parsed_row_cells, last_populated_idx) = Self::parse_ods_row_cells(row_inner)?;
            match last_populated_idx {
                Some(last_pop) => {
                    let next_row = current_row.checked_add(row_repeat).ok_or_else(|| {
                        ods_budget_error("ODS 행 반복 횟수가 허용 범위를 초과했습니다.")
                    })?;
                    if next_row > Self::MAX_ROWS + 1 {
                        return Err(ods_budget_error(
                            "ODS 행 인덱스가 허용 범위를 초과했습니다.",
                        ));
                    }
                    for r_offset in 0..row_repeat {
                        let target_row = current_row + r_offset;
                        if target_row > Self::MAX_ROWS {
                            return Err(ods_budget_error(
                                "ODS 행 좌표가 허용 범위를 초과했습니다.",
                            ));
                        }
                        Self::emit_ods_row(
                            &parsed_row_cells,
                            last_pop,
                            target_row,
                            &mut cells,
                            total_cells,
                        )?;
                    }
                    current_row = next_row;
                }
                None => {
                    if parsed_row_cells
                        .iter()
                        .any(|cell| cell.2 > Self::MAX_COLS + 1)
                    {
                        return Err(ods_budget_error(
                            "ODS 열 인덱스가 허용 범위를 초과했습니다.",
                        ));
                    }
                    current_row = current_row.saturating_add(row_repeat);
                }
            }
        }
        Ok(cells)
    }

    /// Parses the cells of one row; also returns the index of the last populated cell.
    fn parse_ods_row_cells(
        row_inner: &str,
    ) -> Result<(Vec<ParsedOdsRowCell>, Option<usize>), EasyOfficeError> {
        let mut parsed_row_cells: Vec<ParsedOdsRowCell> = Vec::new();
        let mut last_populated_idx: Option<usize> = None;
        for (cell_idx, (is_covered, cell_attrs, cell_inner)) in
            extract_ods_cells_from_row(row_inner)
                .into_iter()
                .enumerate()
        {
            let col_repeat = parse_repeat_attr(cell_attrs, "table:number-columns-repeated");
            if col_repeat > Self::MAX_COLS + 1 {
                return Err(ods_budget_error(
                    "ODS 열 반복 횟수가 허용 범위를 초과했습니다.",
                ));
            }
            if is_covered {
                parsed_row_cells.push((None, None, col_repeat, None));
                continue;
            }
            let explicit_row = parse_u32_attr(cell_attrs, "row");
            let explicit_col = parse_u32_attr(cell_attrs, "col");
            let resolved_val = Self::resolve_ods_cell_text(
                cell_attrs,
                cell_inner,
                explicit_row.is_some() || explicit_col.is_some(),
            );
            if resolved_val.is_some() {
                last_populated_idx = Some(cell_idx);
            }
            parsed_row_cells.push((explicit_row, explicit_col, col_repeat, resolved_val));
        }
        Ok((parsed_row_cells, last_populated_idx))
    }

    /// Cell text by priority: formula, paragraph text, office value/boolean/date, then empty for explicit cells.
    fn resolve_ods_cell_text(
        cell_attrs: &str,
        cell_inner: &str,
        has_explicit_position: bool,
    ) -> Option<String> {
        if let Some(raw_f) = extract_attr_from_fragment(cell_attrs, "table:formula") {
            return Some(Self::normalize_ods_formula(&XmlSanitizer::unescape(raw_f)));
        }
        let text_p_bodies = extract_tag_bodies(cell_inner, "text:p");
        let has_text_p = !text_p_bodies.is_empty();
        if has_text_p {
            let joined = text_p_bodies
                .into_iter()
                .map(extract_odf_inline_text)
                .collect::<Vec<_>>()
                .join("\n");
            if !joined.is_empty() {
                return Some(joined);
            }
        }
        if let Some(b_val) = extract_attr_from_fragment(cell_attrs, "office:boolean-value") {
            let trimmed = b_val.trim();
            return Some(if trimmed.eq_ignore_ascii_case("true") || trimmed == "1" {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            });
        }
        extract_attr_from_fragment(cell_attrs, "office:value")
            .or_else(|| extract_attr_from_fragment(cell_attrs, "office:string-value"))
            .or_else(|| extract_attr_from_fragment(cell_attrs, "office:date-value"))
            .map(XmlSanitizer::unescape)
            .or_else(|| (has_text_p || has_explicit_position).then(String::new))
    }

    /// Expands one parsed row (with column repeats) at `target_row` into `cells`.
    fn emit_ods_row(
        parsed_row_cells: &[ParsedOdsRowCell],
        last_pop: usize,
        target_row: u32,
        cells: &mut Vec<(u32, u32, String)>,
        total_cells: &mut usize,
    ) -> Result<(), EasyOfficeError> {
        let mut current_col: u32 = 0;
        for (c_idx, (explicit_row, explicit_col, col_repeat, resolved_val)) in
            parsed_row_cells.iter().enumerate()
        {
            let next_col = current_col
                .checked_add(*col_repeat)
                .ok_or_else(|| ods_budget_error("ODS 열 반복 횟수가 허용 범위를 초과했습니다."))?;
            if c_idx <= last_pop && next_col > Self::MAX_COLS + 1 {
                return Err(ods_budget_error(
                    "ODS 열 인덱스가 허용 범위를 초과했습니다.",
                ));
            }
            if let Some(val) = resolved_val {
                for c_offset in 0..*col_repeat {
                    let r = explicit_row.unwrap_or(target_row);
                    let c = explicit_col
                        .map(|ec| ec.saturating_add(c_offset))
                        .unwrap_or(current_col + c_offset);
                    if r > Self::MAX_ROWS || c > Self::MAX_COLS {
                        return Err(ods_budget_error("ODS 셀 좌표가 허용 범위를 초과했습니다."));
                    }
                    *total_cells += 1;
                    if *total_cells > Self::MAX_CELLS {
                        return Err(ods_budget_error(
                            "ODS 셀 개수가 허용 예산(200,000)을 초과했습니다.",
                        ));
                    }
                    cells.push((r, c, val.clone()));
                }
            }
            current_col = next_col;
        }
        Ok(())
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
                "<draw:page draw:name=\"{}\">",
                XmlSanitizer::escape(s_title)
            ));
            if !s_title.is_empty() {
                content.push_str(&format!(
                    "<draw:frame draw:layer=\"layout\" presentation:class=\"title\"><draw:text-box><text:p>{}</text:p></draw:text-box></draw:frame>",
                    XmlSanitizer::escape(s_title)
                ));
            }
            content.push_str("<draw:frame draw:layer=\"layout\"><draw:text-box>");
            for b in bullets {
                content.push_str("<text:p>");
                content.push_str(&XmlSanitizer::escape(b));
                content.push_str("</text:p>");
            }
            content.push_str("</draw:text-box></draw:frame>");
            if !notes.is_empty() {
                content.push_str("<presentation:notes><draw:frame><draw:text-box>");
                for line in notes.split('\n') {
                    content.push_str("<text:p>");
                    content.push_str(&XmlSanitizer::escape(line));
                    content.push_str("</text:p>");
                }
                content.push_str("</draw:text-box></draw:frame></presentation:notes>");
            }
            content.push_str("</draw:page>");
        }
        content.push_str("</office:presentation></office:body></office:document-content>");
        XmlSanitizer::validate_safe_xml(&content)?;

        ZipArchiveCodec::pack_compressed(&[
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
            .map(XmlSanitizer::unescape)
            .filter(|s| !s.is_empty())
            .or_else(|| {
                meta_xml_opt
                    .as_deref()
                    .and_then(|m| extract_tag_bodies(m, "dc:title").into_iter().next())
                    .map(XmlSanitizer::unescape)
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_else(|| "발표자료".to_string());

        let slides = extract_balanced_elements(&content_xml, "draw:page")
            .into_iter()
            .map(|(attrs, inner)| Self::parse_odp_slide(attrs, inner))
            .collect();
        Ok((title, slides))
    }

    /// Speaker notes of a slide: the `notes` attribute, else the notes pages' paragraph text.
    fn odp_slide_notes(attrs: &str, inner: &str) -> String {
        let notes = extract_attr_from_fragment(attrs, "notes")
            .map(XmlSanitizer::unescape)
            .unwrap_or_default();
        let notes_blocks = extract_balanced_elements(inner, "presentation:notes");
        if !notes.is_empty() || notes_blocks.is_empty() {
            return notes;
        }
        let mut note_lines = Vec::new();
        for (_, n_inner) in &notes_blocks {
            let cleaned = strip_tag_blocks(n_inner, "text:page-number");
            for (_, _, p_body) in extract_odf_blocks_in_order(&cleaned, &["text:p", "text:h"]) {
                let txt = extract_odf_inline_text(p_body);
                if !txt.trim().is_empty() {
                    note_lines.push(txt);
                }
            }
        }
        note_lines.join("\n")
    }

    /// Text of the first non-empty title-class frame.
    fn odp_frame_title(without_page_num: &str) -> Option<String> {
        extract_balanced_elements(without_page_num, "draw:frame")
            .into_iter()
            .filter(|(f_attrs, _)| {
                extract_attr_from_fragment(f_attrs, "presentation:class") == Some("title")
            })
            .find_map(|(_, f_inner)| {
                let t_parts: Vec<String> =
                    extract_odf_blocks_in_order(f_inner, &["text:p", "text:h"])
                        .into_iter()
                        .map(|(_, _, b)| extract_odf_inline_text(b))
                        .filter(|s| !s.is_empty())
                        .collect();
                (!t_parts.is_empty()).then(|| t_parts.join(" "))
            })
    }

    fn parse_odp_slide(attrs: &str, inner: &str) -> (String, Vec<String>, String) {
        let notes = Self::odp_slide_notes(attrs, inner);
        let without_notes = strip_tag_blocks(inner, "presentation:notes");
        let without_page_num = strip_tag_blocks(&without_notes, "text:page-number");
        let frame_title = Self::odp_frame_title(&without_page_num);
        let attr_title = extract_attr_from_fragment(attrs, "draw:name")
            .or_else(|| extract_attr_from_fragment(attrs, "title"))
            .map(XmlSanitizer::unescape)
            .unwrap_or_default();

        let mut bullets: Vec<String> =
            extract_odf_blocks_in_order(&without_notes, &["text:p", "text:h"])
                .into_iter()
                .filter_map(|(_, _, b)| {
                    let had_page_num = b.contains("<text:page-number");
                    let cleaned = strip_tag_blocks(b, "text:page-number");
                    let txt = extract_odf_inline_text(&cleaned);
                    (!(had_page_num && txt.trim().is_empty())).then_some(txt)
                })
                .collect();

        let s_title = match frame_title {
            Some(ft) => {
                if bullets.first() == Some(&ft) {
                    bullets.remove(0);
                }
                ft
            }
            None => attr_title,
        };
        (s_title, bullets, notes)
    }

    pub(crate) fn build_manifest(mime: &str) -> String {
        Self::build_manifest_with_media(mime, &[])
    }

    /// Manifest listing `content.xml` plus embedded media parts `(path, media type)`.
    pub(crate) fn build_manifest_with_media(mime: &str, media: &[(String, &str)]) -> String {
        let mut xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\"><manifest:file-entry manifest:full-path=\"/\" manifest:media-type=\"{mime}\"/><manifest:file-entry manifest:full-path=\"content.xml\" manifest:media-type=\"text/xml\"/>"
        );
        for (path, media_type) in media {
            xml.push_str(&format!(
                "<manifest:file-entry manifest:full-path=\"{}\" manifest:media-type=\"{media_type}\"/>",
                XmlSanitizer::escape(path)
            ));
        }
        xml.push_str("</manifest:manifest>");
        xml
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

pub use crate::hwpx_codec::{HwpxCodec, HwpxPayload};

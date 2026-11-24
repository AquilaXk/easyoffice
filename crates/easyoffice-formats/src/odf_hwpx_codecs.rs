use crate::cell_type::CellValueKind;
use crate::rtf_xml_helpers::{
    extract_attr, extract_odf_blocks_in_order, extract_odf_inline_text, extract_tag_bodies,
    scan_single_a1_coord, strip_tag_blocks, to_open_formula_syntax_impl,
};
use crate::sheet_styling::ods_default_style::{
    read_default_styles, table_col_width_attrs, table_columns_xml, table_row_height_attr,
    workbook_default_row_height,
};
use crate::sheet_styling::ods_grid::{read_ods_sheets, read_ods_sheets_with_defaults};
use crate::sheet_styling::ods_names::LOEXT_NS;
use crate::sheet_styling::ods_numfmt::{EO_NS, NUMBER_NS};
use crate::sheet_styling::ods_page::title_end;
use crate::sheet_styling::ods_styles::{parse_ods_styling_with_defaults, OdsStyleTable};
use crate::sheet_styling::ods_view::{settings_xml, GroupWriter, TabStyles, TABLEOOO_NS};
use crate::sheet_styling::sheet_rules::ods_rules::{OdsRuleTable, CALCEXT_NS};
use crate::sheet_styling::{hides_every_sheet, IdMemo, SheetStyling};
use crate::{EasyOfficeError, XmlSanitizer, ZipArchiveCodec};

/// Unpacked ODF text document (`(title, paragraphs, tables)`).
pub type OdtUnpacked = (String, Vec<String>, Vec<Vec<Vec<String>>>);
/// Unpacked ODF spreadsheet (`(sheet_name, cells)`).
pub type OdsUnpacked = (String, Vec<(u32, u32, String)>);
/// Unpacked multi-sheet ODF spreadsheet (`Vec<(sheet_name, cells)>`).
pub type OdsMultiUnpacked = Vec<(String, Vec<(u32, u32, String)>)>;

/// Appends the empty-row filler for a gap of `gap` rows; `style_attr` is the row style of the sheet's default row height
/// (` table:style-name="ro1"`), empty for none.
fn push_empty_rows(content: &mut String, gap: u32, style_attr: &str) {
    if gap == 1 {
        content.push_str(&format!(
            "<table:table-row{style_attr}><table:table-cell/></table:table-row>"
        ));
    } else {
        content.push_str(&format!(
            "<table:table-row{style_attr} table:number-rows-repeated=\"{gap}\"><table:table-cell/></table:table-row>"
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

pub(crate) fn ods_budget_error(message: &str) -> EasyOfficeError {
    EasyOfficeError::ResourceBudgetExceeded(message.to_string())
}

/// Unpacked ODF presentation (`(title, slides)`).
pub type OdpUnpacked = (String, Vec<(String, Vec<String>, String)>);

/// ISO/IEC 26300 OpenDocument Format (ODF) container codec for `.odt`, `.ods`, and `.odp`.
pub struct OdfCodec;

/// What the table writer needs to name validations and write rules.
struct RuleContext<'a> {
    table: &'a OdsRuleTable,
    sheet: usize,
    stylings: &'a [SheetStyling],
    sheet_names: &'a [&'a str],
}

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
        let mut budget = crate::decode_budget::DecodeBudget::new();
        budget.charge(title.len(), "ODT 제목")?;

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
                    budget.charge_cell(cell_text.len(), "ODT 표 셀")?;
                    cells.push(cell_text);
                }
                rows.push(cells);
            }
            tables.push(rows);
        }

        let without_tables = strip_tag_blocks(&content_xml, "table:table");
        let mut paragraphs = Vec::new();
        for (_, _, body) in extract_odf_blocks_in_order(&without_tables, &["text:p", "text:h"]) {
            let text = extract_odf_inline_text(body);
            budget.charge_block(text.len(), "ODT 단락")?;
            paragraphs.push(text);
        }

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
        let shown;
        let stylings = if hides_every_sheet(stylings, sheets.len()) {
            shown = SheetStyling::with_first_shown(stylings);
            &shown[..]
        } else {
            stylings
        };
        let style_table = OdsStyleTable::build(stylings);
        let rule_table = OdsRuleTable::build(stylings);
        let tab_styles = TabStyles::build(stylings);

        let mut content = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" office:version=\"1.2\"",
        );
        content.push_str(&format!(
            " xmlns:number=\"{NUMBER_NS}\" xmlns:eo=\"{EO_NS}\" xmlns:loext=\"{LOEXT_NS}\" xmlns:calcext=\"{CALCEXT_NS}\" xmlns:tableooo=\"{TABLEOOO_NS}\">"
        ));
        content.push_str(&Self::automatic_styles_with_rules(
            &style_table,
            &rule_table,
            &tab_styles,
        ));
        content.push_str("<office:body><office:spreadsheet");
        if let Some(protect) = stylings.first().and_then(|st| st.workbook_protect.as_ref()) {
            content.push_str(&crate::sheet_styling::ods_protect::body_attrs(protect));
        }
        content.push('>');
        content
            .push_str(&crate::sheet_styling::ods_date_system::calculation_settings_xml(stylings));

        let default_sheet = vec![("Sheet1".to_string(), Vec::new())];
        let active_sheets = if sheets.is_empty() {
            default_sheet.as_slice()
        } else {
            sheets
        };
        let names: Vec<&str> = active_sheets.iter().map(|(n, _)| n.as_str()).collect();
        let default_font = stylings.first().and_then(|st| st.default_font.as_ref());
        let wb_default_row_height = workbook_default_row_height(stylings, active_sheets.len());
        content.push_str(&rule_table.validations_xml(stylings, &names));
        for (sheet_idx, (sheet_name, cells)) in active_sheets.iter().enumerate() {
            let rules = RuleContext {
                table: &rule_table,
                sheet: sheet_idx,
                stylings,
                sheet_names: &names,
            };
            // A format with no tables holds the cells a structured reference stands for (PD-225).
            let cells = crate::sheet_styling::structured_cells::range_formulas(
                cells, sheet_idx, &names, stylings,
            );
            Self::push_ods_table(
                &mut content,
                sheet_name,
                &cells,
                caches.get(sheet_idx),
                stylings.get(sheet_idx),
                wb_default_row_height,
                (&style_table, &rules, &tab_styles),
            );
        }

        content.push_str(&crate::sheet_styling::structured_cells::names_xml(
            stylings, &names, None,
        ));
        content.push_str(
            &crate::sheet_styling::sheet_table::ods_tables::database_ranges_xml(stylings, &names),
        );
        content.push_str("</office:spreadsheet></office:body></office:document-content>");
        XmlSanitizer::validate_safe_xml(&content)?;

        let settings = settings_xml(&names, stylings);
        let page_styles = crate::sheet_styling::ods_page::styles_xml(
            tab_styles.layouts(),
            stylings.iter().any(|st| !st.links.is_empty()),
            default_font,
            wb_default_row_height,
        );
        let mut media = vec![("styles.xml".to_string(), "text/xml")];
        if settings.is_some() {
            media.push(("settings.xml".to_string(), "text/xml"));
        }
        let manifest = Self::build_manifest_with_media(Self::MIMETYPE_ODS, &media);
        XmlSanitizer::validate_safe_xml(&page_styles)?;
        let mut parts = vec![
            (
                "mimetype".to_string(),
                Self::MIMETYPE_ODS.as_bytes().to_vec(),
            ),
            ("META-INF/manifest.xml".to_string(), manifest.into_bytes()),
            ("content.xml".to_string(), content.into_bytes()),
            ("styles.xml".to_string(), page_styles.into_bytes()),
        ];
        if let Some(settings) = settings {
            parts.push(("settings.xml".to_string(), settings.into_bytes()));
        }
        ZipArchiveCodec::pack_compressed(&parts)
    }

    /// The automatic styles of the cells, columns and conditional formatting rules.
    fn automatic_styles_with_rules(
        styles: &OdsStyleTable,
        rules: &OdsRuleTable,
        tabs: &TabStyles,
    ) -> String {
        let base = styles.automatic_styles_xml();
        let extra = rules.styles_xml() + &tabs.xml();
        if extra.is_empty() {
            base
        } else if base.is_empty() {
            format!("<office:automatic-styles>{extra}</office:automatic-styles>")
        } else {
            base.replace(
                "</office:automatic-styles>",
                &format!("{extra}</office:automatic-styles>"),
            )
        }
    }

    /// Appends one `<table:table>` with sparse-row / sparse-column gap elements.
    fn push_ods_table(
        content: &mut String,
        sheet_name: &str,
        cells: &[(u32, u32, String)],
        cache: Option<&crate::XlsxFormulaCache>,
        styling: Option<&SheetStyling>,
        wb_default_row_height: Option<crate::RowHeight>,
        (style_table, rules, tab_styles): (&OdsStyleTable, &RuleContext<'_>, &TabStyles),
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
            // Cells that only carry a data validation.
            for (row, col) in rules.table.blank_cells(&st.rules) {
                grid.entry(row).or_default().entry(col).or_insert(&blank);
            }
            // Cells that only carry a hyperlink (an empty `text:a` holds it).
            for &(row, col) in st.links.keys() {
                grid.entry(row).or_default().entry(col).or_insert(&blank);
            }
            // The repeated rows are written, empty ones too, so the wrapper has its extent.
            for row in 0..title_end(st.page.title_rows).unwrap_or(0) {
                grid.entry(row).or_default();
            }
            // The rows of a group are written, empty ones too, so the group has its extent.
            for &(first, last, _) in &st.outline.rows {
                for row in first..=last {
                    grid.entry(row).or_default();
                }
            }
            // The rows with a height are written, empty ones too, to carry it (PD-278).
            for &row in st.row_heights.keys() {
                grid.entry(row).or_default();
            }
            // The rows a filter hides are written, empty ones too, to carry their visibility (PD-226).
            for row in st.filter_hidden_runs().flat_map(|&(a, b)| a..=b) {
                grid.entry(row).or_default();
            }
        }
        let tab_style = styling
            .and_then(|st| tab_styles.name(st))
            .map_or(String::new(), |name| {
                format!(" table:style-name=\"{name}\"")
            });
        let print_ranges = styling.map_or(String::new(), |st| {
            crate::sheet_styling::ods_page::print_ranges_attr(sheet_name, &st.page)
        });
        let protect_attrs = styling.and_then(|st| st.protect.as_ref()).map_or(
            String::new(),
            crate::sheet_styling::ods_protect::table_attrs,
        );
        let col_width_attrs = styling
            .map(|st| table_col_width_attrs(st.default_col_width))
            .unwrap_or_default();
        let row_height_attr = table_row_height_attr(
            styling.and_then(|st| st.default_row_height),
            wb_default_row_height,
        );
        content.push_str(&format!(
            "<table:table table:name=\"{}\"{tab_style}{print_ranges}{protect_attrs}{col_width_attrs}{row_height_attr}>",
            XmlSanitizer::escape(sheet_name)
        ));
        if let Some(protect) = styling.and_then(|st| st.protect.as_ref()) {
            content.push_str(&crate::sheet_styling::ods_protect::table_protection_xml(
                protect,
            ));
        }
        if let Some(st) = styling {
            content.push_str(&table_columns_xml(st, style_table));
            content.push_str(&rules.table.conditional_xml(sheet_name, &st.rules));
        }
        let mut expected_row: u32 = 0;
        // The rows with no height of their own take the default row height a user set; the band after the last row says it
        // for the rest of the sheet (PD-286). A default that follows the font stays in `styles.xml` / `eo:default-row-height` (PD-300, PD-345).
        let default_style = styling
            .and_then(SheetStyling::user_default_row_height)
            .and_then(|height| style_table.row_style_name(height))
            .map_or(String::new(), |name| {
                format!(" table:style-name=\"{name}\"")
            });
        let mut ids = IdMemo::new();
        let no_levels = crate::Outline::default();
        let outline = styling.map_or(&no_levels, |st| &st.outline);
        let mut groups = GroupWriter::new(
            "table:table-row-group",
            &outline.rows,
            &outline.collapsed_rows,
            &outline.open_rows,
        );
        let header_end = styling.and_then(|st| title_end(st.page.title_rows));
        let matrices: std::collections::HashMap<(u32, u32), crate::SpillRef> = styling
            .map(|st| st.spills.iter().map(|r| ((r.row, r.col), *r)).collect())
            .unwrap_or_default();
        for (row, row_cells) in grid {
            if row > expected_row {
                groups.close_all(content);
                push_empty_rows(content, row - expected_row, &default_style);
            }
            if row == 0 && header_end.is_some() {
                content.push_str("<table:table-header-rows>");
            }
            groups.move_to(content, row);
            let filtered = styling.is_some_and(|st| st.filter_hides(row));
            // The rows a folded group hid are collapsed too, which is how the other spreadsheet programs tell a folded
            // group from an open one; a reader here takes the fold from the group's `table:display`.
            let folded = styling.is_some_and(|st| {
                crate::sheet_styling::xlsx_layout::run_contains(&st.outline.collapsed_rows, row)
            });
            let visibility = if filtered {
                " table:visibility=\"filter\""
            } else if folded {
                " table:visibility=\"collapse\""
            } else {
                ""
            };
            let height_style = styling
                .and_then(|st| st.row_heights.get(&row))
                .and_then(|&height| style_table.row_style_name(height))
                .map_or_else(
                    || default_style.clone(),
                    |name| format!(" table:style-name=\"{name}\""),
                );
            content.push_str(&format!("<table:table-row{visibility}{height_style}>"));
            let mut expected_col: u32 = 0;
            for (col, raw) in row_cells {
                if col > expected_col {
                    push_empty_cells(content, col - expected_col);
                }
                let style_attr = styling
                    .and_then(|st| match st.cells.get(&(row, col)) {
                        Some(style) => style_table.cell_style_name(style, st, &mut ids),
                        None if st.cell_unlocked(row, col) => style_table.unlocked_name(),
                        None => None,
                    })
                    .map_or(String::new(), |name| {
                        format!(" table:style-name=\"{name}\"")
                    })
                    + rules.table.cell_attr(rules.sheet, rules.stylings, row, col);
                let cached = cache.and_then(|c| c.get(&(row, col)));
                // An array that spills is a matrix formula over its area (the cells after the first
                // are written as the values the formula gave).
                let raw_kind = styling.map_or_else(
                    || CellValueKind::infer(raw),
                    |st| st.cell_kind(row, col, raw),
                );
                let style_attr = match matrices.get(&(row, col)) {
                    Some(area) if raw_kind == CellValueKind::Formula => format!(
                        "{style_attr} table:number-matrix-columns-spanned=\"{}\" table:number-matrix-rows-spanned=\"{}\"",
                        area.cols, area.rows
                    ),
                    _ => style_attr,
                };
                let filled = raw_kind == CellValueKind::Formula
                    && styling.is_some_and(|st| {
                        st.spills.iter().any(|a| a.covers_after_anchor(row, col))
                    });
                let kind = if filled {
                    CellValueKind::Text
                } else {
                    raw_kind
                };
                let at = content.len();
                Self::push_ods_cell(content, (row, col), raw, kind, cached, &style_attr);
                if !filled {
                    if let Some(link) = styling.and_then(|st| st.links.get(&(row, col))) {
                        crate::sheet_styling::sheet_links::ods_links::wrap_first_paragraph(
                            content, at, link,
                        );
                    }
                }
                expected_col = col.saturating_add(1);
            }
            content.push_str("</table:table-row>");
            expected_row = row.saturating_add(1);
            if header_end == Some(row + 1) {
                groups.close_all(content);
                content.push_str("</table:table-header-rows>");
            }
        }
        groups.close_all(content);
        if !default_style.is_empty() && expected_row <= Self::MAX_ROWS {
            push_empty_rows(content, Self::MAX_ROWS + 1 - expected_row, &default_style);
        }
        content.push_str(&crate::sheet_styling::structured_cells::names_xml(
            rules.stylings,
            rules.sheet_names,
            Some(sheet_name),
        ));
        content.push_str("</table:table>");
    }

    fn push_ods_cell(
        content: &mut String,
        (row, col): (u32, u32),
        raw: &str,
        kind: CellValueKind,
        cached: Option<&crate::XlsxCachedValue>,
        style_attr: &str,
    ) {
        if raw.is_empty() {
            content.push_str(&format!("<table:table-cell{style_attr}/>"));
            return;
        }
        match kind {
            CellValueKind::Formula => {
                if let Some(formula) = raw.strip_prefix('=') {
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
                                "<table:table-cell row=\"{row}\" col=\"{col}\"{style_attr} table:formula=\"of:={escaped_formula}\"/>"
                            ));
                        }
                    }
                } else {
                    content.push_str(&format!(
                        "<table:table-cell row=\"{row}\" col=\"{col}\"{style_attr} office:value-type=\"string\"><text:p>{}</text:p></table:table-cell>",
                        XmlSanitizer::escape(raw)
                    ));
                }
            }
            CellValueKind::Number
                if crate::cell_numbers::numeric_cell_value(raw).is_some()
                    || raw.trim().parse::<f64>().is_ok_and(f64::is_finite) =>
            {
                let escaped_val = XmlSanitizer::escape(raw);
                content.push_str(&format!(
                    "<table:table-cell row=\"{row}\" col=\"{col}\"{style_attr} office:value-type=\"float\" office:value=\"{escaped_val}\"><text:p>{escaped_val}</text:p></table:table-cell>"
                ));
            }
            CellValueKind::Boolean => {
                let truthy = !raw.trim().eq_ignore_ascii_case("FALSE") && raw.trim() != "0";
                let (val, disp) = if truthy {
                    ("true", "TRUE")
                } else {
                    ("false", "FALSE")
                };
                content.push_str(&format!(
                    "<table:table-cell row=\"{row}\" col=\"{col}\"{style_attr} office:value-type=\"boolean\" office:boolean-value=\"{val}\"><text:p>{disp}</text:p></table:table-cell>"
                ));
            }
            CellValueKind::Error => {
                content.push_str(&format!(
                    "<table:table-cell row=\"{row}\" col=\"{col}\"{style_attr} calcext:value-type=\"error\"><text:p>{}</text:p></table:table-cell>",
                    XmlSanitizer::escape(raw)
                ));
            }
            CellValueKind::Text | CellValueKind::Number => {
                content.push_str(&format!(
                    "<table:table-cell row=\"{row}\" col=\"{col}\"{style_attr} office:value-type=\"string\"><text:p>{}</text:p></table:table-cell>",
                    XmlSanitizer::escape(raw)
                ));
            }
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
        let (content_xml, _, settings, styles) = Self::extract_parts(bytes, Self::MIMETYPE_ODS)?;
        let defaults = read_default_styles(styles.as_deref(), &content_xml)?;
        let mut stylings = parse_ods_styling_with_defaults(&content_xml, &defaults)?;
        Self::apply_views(&mut stylings, settings.as_deref());
        crate::sheet_styling::ods_page::read_pages(&content_xml, styles.as_deref(), &mut stylings);
        crate::sheet_styling::ods_names::read_names(&content_xml, &mut stylings);
        crate::sheet_styling::sheet_table::ods_tables::read_tables(&content_xml, &mut stylings)?;
        crate::sheet_styling::ods_protect::read_protection(&content_xml, &mut stylings)?;
        crate::sheet_styling::ods_date_system::read_date_system(&content_xml, &mut stylings);
        Self::show_one_sheet(&mut stylings);
        Ok(stylings)
    }

    pub fn unpack_ods_multi(bytes: &[u8]) -> Result<OdsMultiUnpacked, EasyOfficeError> {
        let (content_xml, _) = Self::extract_content_and_meta_xml(bytes, Self::MIMETYPE_ODS)?;
        read_ods_sheets(&content_xml)
    }

    /// Unpacks all ODS tables together with their cell and column styling in a single ZIP pass.
    pub fn unpack_ods_with_styling(
        bytes: &[u8],
    ) -> Result<(OdsMultiUnpacked, Vec<(String, SheetStyling)>), EasyOfficeError> {
        let (content_xml, _, settings, styles) = Self::extract_parts(bytes, Self::MIMETYPE_ODS)?;
        let defaults = read_default_styles(styles.as_deref(), &content_xml)?;
        let (sheets, mut stylings) = read_ods_sheets_with_defaults(&content_xml, &defaults)?;
        Self::apply_views(&mut stylings, settings.as_deref());
        crate::sheet_styling::ods_page::read_pages(&content_xml, styles.as_deref(), &mut stylings);
        crate::sheet_styling::ods_names::read_names(&content_xml, &mut stylings);
        crate::sheet_styling::sheet_table::ods_tables::read_tables(&content_xml, &mut stylings)?;
        crate::sheet_styling::ods_protect::read_protection(&content_xml, &mut stylings)?;
        crate::sheet_styling::ods_date_system::read_date_system(&content_xml, &mut stylings);
        Self::show_one_sheet(&mut stylings);
        Ok((sheets, stylings))
    }
}

pub use crate::hwpx_codec::{HwpxCodec, HwpxPayload};

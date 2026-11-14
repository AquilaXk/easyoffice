use super::*;
use crate::odf_hwpx_codecs::ods_budget_error;
use crate::rtf_xml_helpers::{
    extract_attr_from_fragment, extract_balanced_elements, extract_odf_inline_text,
    extract_ods_cells_from_row, extract_tag_bodies,
};
use crate::sheet_styling::ods_styles::{parse_automatic_styles, AutomaticStyles};
use crate::sheet_styling::{CellStyle, SheetStyling, MAX_STYLED_CELLS};
use crate::{OdsMultiUnpacked, XmlSanitizer};
use std::collections::BTreeMap;

type ParsedOdsRowCell = (Option<u32>, Option<u32>, u32, Option<String>);

fn parse_u32_attr(attrs: &str, name: &str) -> Option<u32> {
    extract_attr_from_fragment(attrs, name).and_then(|v| v.parse::<u32>().ok())
}

fn parse_repeat_attr(attrs: &str, name: &str) -> u32 {
    parse_u32_attr(attrs, name).unwrap_or(1).max(1)
}

/// The earlier substring scanners, kept verbatim as the reference the one-pass walker is compared to.
struct Legacy;

impl Legacy {
    pub(super) fn parse_ods_multi_xml(
        content_xml: &str,
    ) -> Result<OdsMultiUnpacked, EasyOfficeError> {
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
            if row_repeat > OdfCodec::MAX_ROWS + 1 {
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
                    if next_row > OdfCodec::MAX_ROWS + 1 {
                        return Err(ods_budget_error(
                            "ODS 행 인덱스가 허용 범위를 초과했습니다.",
                        ));
                    }
                    for r_offset in 0..row_repeat {
                        let target_row = current_row + r_offset;
                        if target_row > OdfCodec::MAX_ROWS {
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
                        .any(|cell| cell.2 > OdfCodec::MAX_COLS + 1)
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
            if col_repeat > OdfCodec::MAX_COLS + 1 {
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
            return Some(OdfCodec::normalize_ods_formula(&XmlSanitizer::unescape(
                raw_f,
            )));
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
            if c_idx <= last_pop && next_col > OdfCodec::MAX_COLS + 1 {
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
                    if r > OdfCodec::MAX_ROWS || c > OdfCodec::MAX_COLS {
                        return Err(ods_budget_error("ODS 셀 좌표가 허용 범위를 초과했습니다."));
                    }
                    *total_cells += 1;
                    if *total_cells > OdfCodec::MAX_CELLS {
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
}

fn repeat_of(attrs: &str, name: &str) -> u32 {
    extract_attr_from_fragment(attrs, name)
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(1)
        .max(1)
}

fn legacy_table_columns(table_xml: &str, styles: &AutomaticStyles) -> BTreeMap<u32, u32> {
    let head = &table_xml[..table_xml
        .find("<table:table-row")
        .unwrap_or(table_xml.len())];
    let mut columns = BTreeMap::new();
    let mut col = 0u32;
    for (attrs, _) in extract_balanced_elements(head, "table:table-column") {
        let repeat = repeat_of(attrs, "table:number-columns-repeated");
        let px = extract_attr_from_fragment(attrs, "table:style-name")
            .and_then(|n| styles.columns.get(&XmlSanitizer::unescape(n)).copied());
        if let Some(px) = px {
            for c in col..col.saturating_add(repeat).min(OdfCodec::MAX_COLS + 1) {
                if columns.len() >= MAX_STYLED_CELLS {
                    break;
                }
                columns.insert(c, px);
            }
        }
        col = col.saturating_add(repeat);
    }
    columns
}

fn legacy_table_cells(
    table_xml: &str,
    styles: &AutomaticStyles,
) -> BTreeMap<(u32, u32), CellStyle> {
    let mut cells = BTreeMap::new();
    let mut row = 0u32;
    for (row_attrs, row_inner) in extract_balanced_elements(table_xml, "table:table-row") {
        let row_repeat = repeat_of(row_attrs, "table:number-rows-repeated");
        let mut col = 0u32;
        for (_, attrs, _) in extract_ods_cells_from_row(row_inner) {
            let col_repeat = repeat_of(attrs, "table:number-columns-repeated");
            let style = extract_attr_from_fragment(attrs, "table:style-name")
                .and_then(|n| styles.cells.get(&XmlSanitizer::unescape(n)))
                .filter(|s| !s.is_plain());
            if let Some(style) = style {
                // A style repeated across a whole row/column band (a "format the sheet" fill) is
                // not individual cell formatting; only bounded blocks are kept.
                let block = u64::from(row_repeat) * u64::from(col_repeat);
                let in_grid = u64::from(row) + u64::from(row_repeat)
                    <= u64::from(OdfCodec::MAX_ROWS) + 1
                    && u64::from(col) + u64::from(col_repeat) <= u64::from(OdfCodec::MAX_COLS) + 1;
                if in_grid && block <= 1024 && cells.len() + block as usize <= MAX_STYLED_CELLS {
                    for r in row..row + row_repeat {
                        for c in col..col + col_repeat {
                            cells.insert((r, c), *style);
                        }
                    }
                }
            }
            col = col.saturating_add(col_repeat);
        }
        row = row.saturating_add(row_repeat);
    }
    cells
}

/// Styling of every table of an ODS `content.xml`, keyed by table name.
fn legacy_ods_styling(content_xml: &str) -> Vec<(String, SheetStyling)> {
    let styles = parse_automatic_styles(content_xml);
    extract_balanced_elements(content_xml, "table:table")
        .into_iter()
        .enumerate()
        .map(|(idx, (attrs, inner))| {
            let name = extract_attr_from_fragment(attrs, "table:name")
                .map(XmlSanitizer::unescape)
                .unwrap_or_else(|| format!("Sheet{}", idx + 1));
            let styling = SheetStyling {
                cells: legacy_table_cells(inner, &styles),
                columns: legacy_table_columns(inner, &styles),
                ..SheetStyling::default()
            };
            (name, styling)
        })
        .collect()
}

// Samples where the shown text and the raw value agree: the substring scanner took the shown text
// first, the reader takes the raw value (`tests/ods_cell_values.rs` covers where they differ).
const CELL_ROWS: &[&str] = &[
    r#"<table:table-row><table:table-cell office:value-type="string"><text:p>가</text:p></table:table-cell><table:table-cell office:value="3"/><table:covered-table-cell/></table:table-row>"#,
    r#"<table:table-row table:number-rows-repeated="3"><table:table-cell table:number-columns-repeated="2" office:value-type="float" office:value="1.5"><text:p>1.5</text:p></table:table-cell></table:table-row>"#,
    r#"<table:table-row><table:table-cell table:formula="of:=SUM([.A1:.A3])" office:value="6"/><table:table-cell table:formula="=A1&amp;&quot;x&quot;"/></table:table-row>"#,
    r#"<table:table-row><table:table-cell><text:p>a<text:s text:c="3"/>b<text:tab/>c<text:line-break/>d<text:span>e &amp; f</text:span></text:p><text:p>second</text:p></table:table-cell></table:table-row>"#,
    r#"<table:table-row><table:table-cell><text:p></text:p></table:table-cell><table:table-cell><text:p></text:p><text:p></text:p></table:table-cell></table:table-row>"#,
    r#"<table:table-row><table:table-cell office:boolean-value="true"/><table:table-cell office:boolean-value=" 0 "/><table:table-cell office:value-type="date" office:date-value="2026-11-11"/><table:table-cell office:string-value="s&amp;t"/></table:table-row>"#,
    r#"<table:table-row/><table:table-row table:number-rows-repeated="5"><table:table-cell/></table:table-row><table:table-row><table:table-cell office:value="9"/></table:table-row>"#,
    r#"<table:table-row><table:table-cell row="4" col="2" office:value="1"/><table:table-cell row="7"/><table:table-cell eo:col="9"><text:p></text:p></table:table-cell></table:table-row>"#,
    r#"<table:table-header-rows><table:table-row><table:table-cell office:value="1"/></table:table-row></table:table-header-rows><table:table-row-group><table:table-row><table:table-cell office:value="2"/></table:table-row></table:table-row-group>"#,
    r#"<table:table-row><table:table-cell><table:table><table:table-row><table:table-cell><text:p>nested</text:p></table:table-cell></table:table-row></table:table><text:p>outer</text:p></table:table-cell><table:table-cell office:value="2"/></table:table-row>"#,
    r#"<table:table-row><table:table-cell table:number-columns-repeated="1000" office:value="1"/><table:table-cell office:value="2"/></table:table-row>"#,
    r#"<table:table-row><table:table-cell office:value="1"/><table:table-cell table:number-columns-repeated="16384"/></table:table-row>"#,
    r#"<table:table-row><table:table-cell table:number-columns-repeated="16385" office:value="1"/></table:table-row>"#,
    r#"<table:table-row table:number-rows-repeated="1048577"><table:table-cell office:value="1"/></table:table-row>"#,
    r#"<table:table-row table:number-rows-repeated="1048576"><table:table-cell office:value="1"/></table:table-row>"#,
    r#"<table:table-row table:number-rows-repeated="3000000000"><table:table-cell/></table:table-row>"#,
    r#"<table:table-row table:number-rows-repeated="500000"><table:table-cell office:value="1"/></table:table-row>"#,
    r#"<table:table-row><table:table-cell row="1048576" office:value="1"/></table:table-row>"#,
];

fn content(tables: &[(&str, &str)]) -> String {
    let mut xml = String::from("<office:document-content><office:body><office:spreadsheet>");
    for (attrs, rows) in tables {
        xml.push_str(&format!("<table:table {attrs}>{rows}</table:table>"));
    }
    xml.push_str("</office:spreadsheet></office:body></office:document-content>");
    xml
}

fn assert_same_sheets(xml: &str) {
    let old = Legacy::parse_ods_multi_xml(xml);
    let new = crate::sheet_styling::ods_grid::read_ods_sheets(xml);
    match (&old, &new) {
        (Ok(a), Ok(b)) => assert_eq!(a, b, "sheets differ for {xml}"),
        (Err(a), Err(b)) => assert_eq!(
            std::mem::discriminant(a),
            std::mem::discriminant(b),
            "{xml}: {a:?} vs {b:?}"
        ),
        _ => panic!("{xml}: old {old:?}, new {new:?}"),
    }
}

#[test]
fn sheets_match_the_substring_scanner() {
    for row in CELL_ROWS {
        assert_same_sheets(&content(&[(r#"table:name="S&amp;1""#, row)]));
    }
    assert_same_sheets(&content(&[]));
    assert_same_sheets(&content(&[
        ("", CELL_ROWS[0]),
        ("name=\"n2\"", CELL_ROWS[1]),
    ]));
    assert_same_sheets(&content(&[
        ("table:name=\"A\"", ""),
        ("table:name=\"B\"/", ""),
    ]));
    assert_same_sheets(&content(&[("table:name=\"A\"", &CELL_ROWS[..12].concat())]));
}

#[test]
fn cell_budget_is_shared_across_sheets() {
    let row = r#"<table:table-row table:number-rows-repeated="150000"><table:table-cell office:value="1"/></table:table-row>"#;
    assert_same_sheets(&content(&[
        ("table:name=\"A\"", row),
        ("table:name=\"B\"", row),
    ]));
    assert!(
        crate::sheet_styling::ods_grid::read_ods_sheets(&content(&[("", row), ("", row)])).is_err()
    );
}

#[test]
fn styling_matches_the_substring_scanner() {
    let styles = r#"<office:automatic-styles><style:style style:name="ce1" style:family="table-cell"><style:text-properties fo:font-weight="bold"/></style:style><style:style style:name="ce2" style:family="table-cell"><style:text-properties fo:font-style="italic" fo:font-size="14pt"/></style:style><style:style style:name="co1" style:family="table-column"><style:table-column-properties style:column-width="2cm"/></style:style></office:automatic-styles>"#;
    let rows = r#"<table:table-column table:style-name="co1" table:number-columns-repeated="2"/><table:table-column/><table:table-column table:style-name="co1"/><table:table-row><table:table-cell table:style-name="ce1"/><table:table-cell table:style-name="ce2" table:number-columns-repeated="3"/><table:covered-table-cell table:style-name="ce1"/><table:table-cell table:style-name="nope"/></table:table-row><table:table-row table:number-rows-repeated="2"><table:table-cell table:style-name="ce1"><text:p>x</text:p></table:table-cell></table:table-row><table:table-row table:number-rows-repeated="2000"><table:table-cell table:style-name="ce1" table:number-columns-repeated="2000"/></table:table-row><table:table-row table:number-rows-repeated="1048576"><table:table-cell table:style-name="ce1"/></table:table-row>"#;
    let xml = format!(
        "<office:document-content>{styles}<office:body><office:spreadsheet><table:table table:name=\"T&amp;1\">{rows}</table:table><table:table>{rows}</table:table></office:spreadsheet></office:body></office:document-content>"
    );
    let old = legacy_ods_styling(&xml);
    let new = crate::sheet_styling::ods_styles::parse_ods_styling(&xml).unwrap();
    assert_eq!(old, new);
    let (sheets, together) =
        crate::sheet_styling::ods_grid::read_ods_sheets_with_styling(&xml).unwrap();
    assert_eq!(together, old, "one walk gives the same styling");
    assert_eq!(sheets, Legacy::parse_ods_multi_xml(&xml).unwrap());
    assert_eq!(old[0].0, "T&1");
    assert!(!old[0].1.cells.is_empty() && !old[0].1.columns.is_empty());
}

#[test]
fn hostile_grids_stay_bounded() {
    // Many rows and deeply nested junk inside a cell need no event limit and no stack.
    let mut rows = String::new();
    for _ in 0..50_000 {
        rows.push_str(r#"<table:table-row><table:table-cell office:value="1"/></table:table-row>"#);
    }
    let sheets = crate::sheet_styling::ods_grid::read_ods_sheets(&content(&[("", &rows)])).unwrap();
    assert_eq!(sheets[0].1.len(), 50_000);
    let deep = format!(
        "<table:table-row><table:table-cell>{}<text:p>x</text:p>{}</table:table-cell></table:table-row>",
        "<a>".repeat(100_000),
        "</a>".repeat(100_000)
    );
    assert_same_sheets(&content(&[("", &deep)]));
    // Truncated markup is refused, not read past the end.
    assert!(crate::sheet_styling::ods_grid::read_ods_sheets(
        "<table:table><table:table-row><table:table-cell"
    )
    .is_err());
    // More than the cell budget fails closed.
    let many = r#"<table:table-row table:number-rows-repeated="1000000"><table:table-cell table:number-columns-repeated="16000" office:value="1"/></table:table-row>"#;
    assert!(crate::sheet_styling::ods_grid::read_ods_sheets(&content(&[("", many)])).is_err());
}

/// A 1,000 x 104 table with styled text, number and formula cells.
fn big_content() -> String {
    let mut rows = String::new();
    for r in 0..1_000u32 {
        rows.push_str("<table:table-row table:style-name=\"ro1\">");
        for c in 0..104u32 {
            match (r + c) % 3 {
                0 => rows.push_str(&format!(
                    "<table:table-cell table:style-name=\"ce1\" office:value-type=\"string\" calcext:value-type=\"string\"><text:p>문자열 {r} {c}</text:p></table:table-cell>"
                )),
                1 => rows.push_str(&format!(
                    "<table:table-cell office:value-type=\"float\" office:value=\"{r}.5\" calcext:value-type=\"float\"><text:p>{r}.5</text:p></table:table-cell>"
                )),
                _ => rows.push_str(&format!(
                    "<table:table-cell table:formula=\"of:=SUM([.A1:.B{}])\" office:value-type=\"float\" office:value=\"3\"><text:p>3</text:p></table:table-cell>",
                    r + 1
                )),
            }
        }
        rows.push_str("</table:table-row>");
    }
    content(&[("table:name=\"Sheet1\"", &rows)])
}

#[test]
#[ignore = "measurement: cargo test --release -p easyoffice-formats --lib ods_grid::tests::measure -- --ignored --nocapture"]
fn measure_one_pass_against_the_substring_scanner() {
    let xml = big_content();
    let time = |f: &dyn Fn() -> usize| {
        let mut best = f64::MAX;
        for _ in 0..15 {
            let t = std::time::Instant::now();
            assert_eq!(std::hint::black_box(f()), 104_000);
            best = best.min(t.elapsed().as_secs_f64() * 1000.0);
        }
        best
    };
    let old = time(&|| Legacy::parse_ods_multi_xml(&xml).unwrap()[0].1.len());
    let new = time(&|| {
        crate::sheet_styling::ods_grid::read_ods_sheets(&xml).unwrap()[0]
            .1
            .len()
    });
    println!(
        "ods table {} bytes: substring scanner {old:.2} ms, one-pass {new:.2} ms",
        xml.len()
    );
}

#[test]
fn hostile_ods_exceeding_max_sheets_fails_closed() {
    let tables: Vec<(&str, &str)> = (0..1_025).map(|_| ("", "")).collect();
    let xml = content(&tables);
    assert!(crate::sheet_styling::ods_grid::read_ods_sheets(&xml).is_err());
}

#[test]
fn hostile_ods_exceeding_max_spills_per_sheet_fails_closed() {
    let repeated_spill_rows = r#"<table:table-row table:number-rows-repeated="4097"><table:table-cell table:formula="of:=A1" table:number-matrix-rows-spanned="2" table:number-matrix-columns-spanned="1"><text:p>1</text:p></table:table-cell></table:table-row>"#;
    let xml = content(&[("", repeated_spill_rows)]);
    let err = crate::sheet_styling::ods_grid::read_ods_sheets_with_styling(&xml);
    assert!(err.is_err());
}

#[test]
fn giant_spill_covers_without_hashset_expansion() {
    let xml = content(&[(
        "table:name=\"Sheet1\"",
        r#"<table:table-row><table:table-cell table:formula="of:=SEQUENCE(1000,2000)" table:number-matrix-rows-spanned="1000" table:number-matrix-columns-spanned="2000"><text:p>anchor</text:p></table:table-cell></table:table-row><table:table-row table:number-rows-repeated="499"/><table:table-row><table:table-cell table:number-columns-repeated="1000"/><table:table-cell><text:p>covered</text:p></table:table-cell><table:table-cell table:number-columns-repeated="1000"/><table:table-cell><text:p>outside_col</text:p></table:table-cell></table:table-row>"#,
    )]);
    let (sheets, stylings) =
        crate::sheet_styling::ods_grid::read_ods_sheets_with_styling(&xml).unwrap();
    assert_eq!(stylings[0].1.spills.len(), 1);
    let cells = &sheets[0].1;
    assert!(cells
        .iter()
        .any(|&(r, c, ref v)| (r, c) == (0, 0) && v == "=SEQUENCE(1000,2000)"));
    assert!(!cells.iter().any(|&(r, c, _)| (r, c) == (500, 1000)));
    assert!(cells
        .iter()
        .any(|&(r, c, ref v)| (r, c) == (500, 2001) && v == "outside_col"));
}

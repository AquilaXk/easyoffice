use super::*;
use crate::rtf_xml_helpers::{
    extract_attr_from_fragment, extract_balanced_elements, extract_tag_bodies,
    extract_tags_or_self_closing_with_attrs, extract_tags_with_attrs, strip_tag_blocks,
};
use crate::XmlSanitizer;

fn decode_xlsx_escapes_owned(s: String) -> String {
    decode_xlsx_escapes(Cow::Owned(s))
}

fn truthy(attrs: &str, name: &str, default: bool) -> bool {
    match extract_attr_from_fragment(attrs, name) {
        None => default,
        Some("0" | "false") => false,
        Some(_) => true,
    }
}

/// The earlier substring scanners, kept verbatim as the reference the one-pass reader is compared to.
struct Legacy;

impl Legacy {
    fn shared_strings(sst_xml: &str) -> Vec<String> {
        let mut out = Vec::new();
        for (_, si_inner) in extract_balanced_elements(sst_xml, "si") {
            if si_inner.trim().is_empty() {
                out.push(String::new());
            } else {
                let without_phonetic = strip_tag_blocks(si_inner, "rPh");
                let joined = extract_tag_bodies(&without_phonetic, "t")
                    .into_iter()
                    .map(XmlSanitizer::unescape)
                    .collect::<Vec<_>>()
                    .join("");
                out.push(decode_xlsx_escapes_owned(joined));
            }
        }
        out
    }

    pub(super) fn parse_xlsx_worksheet_cells(
        sheet_xml: &str,
        shared_strings: &[String],
    ) -> Result<Vec<(u32, u32, String)>, EasyOfficeError> {
        let mut cells: Vec<(u32, u32, String)> = Vec::new();
        let mut shared_masters: std::collections::HashMap<u32, (u32, u32, String)> =
            std::collections::HashMap::new();
        for (attrs_raw, inner) in extract_tags_with_attrs(sheet_xml, "c") {
            let r_attr = extract_attr_from_fragment(attrs_raw, "r");
            let (row, col) = if let (Some(r), Some(c)) = (
                extract_attr_from_fragment(attrs_raw, "row").and_then(|v| v.parse::<u32>().ok()),
                extract_attr_from_fragment(attrs_raw, "col").and_then(|v| v.parse::<u32>().ok()),
            ) {
                if r >= OoxmlCodec::MAX_XLSX_ROWS || c >= OoxmlCodec::MAX_XLSX_COLS {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "XLSX 셀 좌표가 ECMA-376 허용 범위를 초과했습니다.".to_string(),
                    ));
                }
                (r, c)
            } else if let Some(a1) = r_attr {
                parse_a1_zero_based(a1).ok_or_else(|| {
                    EasyOfficeError::ResourceBudgetExceeded(
                        "XLSX A1 셀 참조가 ECMA-376 허용 범위를 초과했거나 잘못되었습니다."
                            .to_string(),
                    )
                })?
            } else {
                (0, 0)
            };

            let cell_type = extract_attr_from_fragment(attrs_raw, "t").unwrap_or_default();
            if let Some(formula_str) =
                Legacy::resolve_xlsx_formula(inner, row, col, &mut shared_masters)
            {
                cells.push((row, col, format!("={formula_str}")));
            } else if cell_type == "inlineStr" || (cell_type.is_empty() && inner.contains("<is")) {
                let without_phonetic = strip_tag_blocks(inner, "rPh");
                let inline_txt = extract_tag_bodies(&without_phonetic, "t")
                    .into_iter()
                    .map(XmlSanitizer::unescape)
                    .collect::<Vec<_>>()
                    .join("");
                cells.push((row, col, decode_xlsx_escapes_owned(inline_txt)));
            } else if let Some(v_body) = extract_tag_bodies(inner, "v").first() {
                let raw_v = XmlSanitizer::unescape(v_body);
                if cell_type == "s" {
                    let resolved = raw_v
                        .trim()
                        .parse::<usize>()
                        .ok()
                        .and_then(|idx| shared_strings.get(idx))
                        .ok_or_else(|| {
                            EasyOfficeError::CorruptedContainer(format!(
                                "XLSX 공유 문자열 인덱스 '{raw_v}'를 읽을 수 없습니다."
                            ))
                        })?;
                    cells.push((row, col, resolved.clone()));
                } else if cell_type == "b" {
                    let b_str = match raw_v.trim() {
                        "1" | "true" | "TRUE" => "TRUE".to_string(),
                        "0" | "false" | "FALSE" => "FALSE".to_string(),
                        _ => raw_v,
                    };
                    cells.push((row, col, b_str));
                } else if cell_type == "d" {
                    cells.push((row, col, raw_v.trim().to_string()));
                } else {
                    cells.push((row, col, decode_xlsx_escapes_owned(raw_v)));
                }
            }
        }
        Ok(cells)
    }

    fn resolve_xlsx_formula(
        inner: &str,
        row: u32,
        col: u32,
        shared_masters: &mut std::collections::HashMap<u32, (u32, u32, String)>,
    ) -> Option<String> {
        let &(f_attrs, f_body) = extract_tags_or_self_closing_with_attrs(inner, "f").first()?;
        let is_shared = extract_attr_from_fragment(f_attrs, "t") == Some("shared");
        let si = extract_attr_from_fragment(f_attrs, "si").and_then(|s| s.parse::<u32>().ok());
        if !f_body.is_empty() {
            let unescaped = XmlSanitizer::unescape(f_body);
            if is_shared {
                if let Some(si_idx) = si {
                    shared_masters.insert(si_idx, (row, col, unescaped.clone()));
                }
            }
            return Some(unescaped);
        }
        if is_shared {
            let (mr, mc, ref master_expr) = *shared_masters.get(&si?)?;
            let dr = row as i64 - mr as i64;
            let dc = col as i64 - mc as i64;
            return shift_shared_formula(master_expr, dr, dc);
        }
        None
    }
}

/// Styled cells (`s="N"`) and custom column widths of one worksheet.
fn legacy_worksheet_styling(sheet_xml: &str, xf_styles: &[CellStyle]) -> SheetStyling {
    let mut styling = SheetStyling::default();
    if !xf_styles.is_empty() {
        for (attrs, _) in extract_tags_or_self_closing_with_attrs(sheet_xml, "c") {
            let Some(style) = extract_attr_from_fragment(attrs, "s")
                .and_then(|v| v.parse::<usize>().ok())
                .and_then(|i| xf_styles.get(i))
                .filter(|s| !s.is_plain())
            else {
                continue;
            };
            let Some((row, col)) =
                extract_attr_from_fragment(attrs, "r").and_then(parse_a1_zero_based)
            else {
                continue;
            };
            if styling.cells.len() >= MAX_STYLED_CELLS {
                break;
            }
            styling.cells.insert((row, col), *style);
        }
    }
    for (attrs, _) in extract_tags_or_self_closing_with_attrs(sheet_xml, "col") {
        if !truthy(attrs, "customWidth", false) {
            continue;
        }
        let parse = |name: &str| extract_attr_from_fragment(attrs, name)?.parse::<u32>().ok();
        let (Some(min), Some(max)) = (parse("min"), parse("max")) else {
            continue;
        };
        let Some(width) =
            extract_attr_from_fragment(attrs, "width").and_then(|w| w.parse::<f64>().ok())
        else {
            continue;
        };
        if !width.is_finite() || width <= 0.0 || min == 0 || max < min {
            continue;
        }
        let px = xlsx_width_to_px(width);
        for col in min..=max.min(crate::OoxmlCodec::MAX_XLSX_COLS) {
            if styling.columns.len() >= MAX_STYLED_CELLS {
                break;
            }
            styling.columns.insert(col - 1, px);
        }
    }
    styling
}

fn sheet(body: &str) -> String {
    format!("<worksheet><sheetData><row r=\"1\">{body}</row></sheetData></worksheet>")
}

fn new_cells(xml: &str, shared: &[String]) -> Result<Vec<(u32, u32, String)>, EasyOfficeError> {
    let table = SharedStrings::from_strings(shared);
    scan_worksheet(xml, &table, &[], false, &mut DecodeBudget::new()).map(|scan| scan.cells)
}

/// The same cells, or the same kind of error.
fn assert_same_cells(xml: &str, shared: &[String]) {
    let old = Legacy::parse_xlsx_worksheet_cells(xml, shared);
    let new = new_cells(xml, shared);
    match (&old, &new) {
        (Ok(a), Ok(b)) => assert_eq!(a, b, "cells differ for {xml}"),
        (Err(a), Err(b)) => assert_eq!(
            std::mem::discriminant(a),
            std::mem::discriminant(b),
            "{xml}: {a:?} vs {b:?}"
        ),
        _ => panic!("{xml}: old {old:?}, new {new:?}"),
    }
}

fn shared_fixture() -> Vec<String> {
    ["가", "나", "", "a&b", "x\ny"].map(String::from).to_vec()
}

const CELL_SAMPLES: &[&str] = &[
    r#"<c r="A1" t="s"><v>1</v></c><c r="B1" t="s"><v>3</v></c>"#,
    r#"<c r="A1"><v>3.5</v></c><c r="B1" s="2"><v>-7</v></c><c r="C1"><v></v></c>"#,
    r#"<c r="A1"><v/></c><c r="B1"><v/><v>9</v></c><c r="C1"/>"#,
    r#"<c r="A1" t="b"><v>1</v></c><c r="B1" t="b"><v> false </v></c><c r="C1" t="b"><v>x</v></c>"#,
    r#"<c r="A1" t="d"><v> 2026-11-10 </v></c><c r="B1" t="str"><v>hi &amp; bye</v></c>"#,
    r#"<c r="A1" t="inlineStr"><is><t>a</t><t xml:space="preserve"> b</t></is></c>"#,
    r#"<c r="A1" t="inlineStr"><is><r><t>x</t></r><rPh sb="0" eb="1"><t>YY</t></rPh><r><t>z</t></r></is></c>"#,
    r#"<c r="A1"><is><t>NoType_x0041_</t></is></c><c r="B1" t="inlineStr"/>"#,
    r#"<c r="A1" t="inlineStr"><is/></c><c r="B1" t="inlineStr"><is></is></c>"#,
    r#"<c r="A1"><f>SUM(B1:B3)</f><v>6</v></c><c r="B1"><f>A1&amp;&quot;x&quot;</f></c>"#,
    r#"<c r="A1"><f t="shared" ref="A1:A3" si="0">B1+C1</f><v>1</v></c><c r="A2"><f t="shared" si="0"/><v>2</v></c><c r="A3"><f t="shared" si="0"/></c>"#,
    r#"<c r="A1"><f t="shared" si="0"/><v>5</v></c><c r="B1"><f t="array">x</f><v>3</v></c><c r="C1"><f/><v>4</v></c>"#,
    r#"<c r="A1"><f>1</f><f>2</f><v>7</v><v>8</v></c>"#,
    r#"<c row="3" col="4" t="s"><v>0</v></c><c r="Z9" row="1" col="x"><v>1</v></c>"#,
    r#"<c eo:r="C3"><v>1</v></c><c eo:row="5" eo:col="6"><v>2</v></c>"#,
    r#"<c r='A1' t='s'><v>0</v></c>"#,
    r#"<c r="A1" t="s"><v>0</v></c>

    <c r="B1" t="s"><v>1</v></c>"#,
    r#"<c r="A1" t="s"><v>2</v></c><c r="B1"><v>_x0041__xD83D__xDE00_</v></c>"#,
    // errors
    r#"<c r="A1" t="s"><v>abc</v></c>"#,
    r#"<c r="A1" t="s"><v>9</v></c>"#,
    r#"<c r="A1" t="s"><v></v></c>"#,
    r#"<c r="A1" t="s"><v>-1</v></c>"#,
    r#"<c r="XFE1"><v>1</v></c>"#,
    r#"<c r="A1048577"><v>1</v></c>"#,
    r#"<c row="1048576" col="0"><v>1</v></c>"#,
    r#"<c row="0" col="16384"><v>1</v></c>"#,
    r#"<c r="A1"><v>1</v></c><c r="!!"><v>1</v></c>"#,
    r#"<c r="!!"/>"#,
];

#[test]
fn cells_match_the_substring_scanner() {
    let shared = shared_fixture();
    for body in CELL_SAMPLES {
        assert_same_cells(&sheet(body), &shared);
    }
}

#[test]
fn cells_match_on_every_prefix_of_a_mixed_row() {
    let shared = shared_fixture();
    let all = CELL_SAMPLES[..12].concat();
    let xml = sheet(&all);
    // Valid XML only: cut after each closing `</c>` or `/>` of a cell.
    for (at, _) in xml.match_indices("</c>").chain(xml.match_indices("/></c>")) {
        let cut = format!("{}</row></sheetData></worksheet>", &xml[..at + 4]);
        assert_same_cells(&cut, &shared);
    }
}

#[test]
fn shared_formulas_shift_from_their_master() {
    let xml = sheet(
        r#"<c r="A1"><f t="shared" si="0">B1+$C1+D$2</f></c><c r="A2"><f t="shared" si="0"/></c><c r="B3"><f t="shared" si="0"/></c>"#,
    );
    let cells = new_cells(&xml, &[]).unwrap();
    assert_eq!(cells[0].2, "=B1+$C1+D$2");
    assert_eq!(cells[1].2, "=B2+$C2+D$2");
    assert_eq!(cells[2].2, "=C3+$C3+E$2");
}

#[test]
fn shared_string_table_matches_the_substring_scanner() {
    for sst in [
        "<sst><si><t>가</t></si><si><t xml:space=\"preserve\"> a </t></si><si/><si></si><si> </si></sst>",
        "<sst><si><r><t>x</t></r><r><t>y&amp;z</t></r><rPh sb=\"0\" eb=\"1\"><t>PH</t></rPh></si></sst>",
        "<sst><si><t>_x0041_</t></si><si><t>_xD83D__xDE00_</t></si><si><t>_x00</t><t>41_</t></si></sst>",
        "<sst><si><t/><t>k</t></si><si><rPh><t>a</t></rPh></si></sst>",
        "<sst></sst>",
    ] {
        let old = Legacy::shared_strings(sst);
        let new = read_shared_strings(sst).unwrap();
        let got: Vec<&str> = (0..old.len()).map(|i| new.get(i).unwrap()).collect();
        assert_eq!(got, old, "{sst}");
        assert!(new.get(old.len()).is_none());
    }
}

#[test]
fn styling_matches_the_substring_scanner() {
    let styles = [
        CellStyle::default(),
        CellStyle {
            bold: true,
            ..CellStyle::default()
        },
        CellStyle {
            italic: true,
            size_pt: Some(14.0),
            ..CellStyle::default()
        },
    ];
    let xml = r#"<worksheet><cols><col min="1" max="2" width="20" customWidth="1"/><col min="3" max="3" width="9"/><col min="4" max="5" width="12.5" customWidth="true"/><col min="2" max="1" width="5" customWidth="1"/><col min="6" max="6" width="7" customWidth="0"/><col min="x" max="9" width="7" customWidth="1"/></cols><sheetData><row><c r="A1" s="1"><v>1</v></c><c r="B1" s="2"/><c r="C1" s="0"/><c r="D1" s="9"/><c r="E1" s="1" t="s"><v>0</v></c><c s="1"/><c r="F1" s="x"/></row></sheetData></worksheet>"#;
    let table = SharedStrings::from_strings(&["a".to_string()]);
    let old = legacy_worksheet_styling(xml, &styles);
    let new = scan_worksheet(xml, &table, &styles, true, &mut DecodeBudget::new())
        .unwrap()
        .styling;
    // The cell without `r` after E1 is F1 (ECMA-376 18.3.1.4); the substring scanner skipped it.
    let mut expected = old.clone();
    expected.cells.insert((0, 5), styles[1]);
    assert_eq!(expected, new);
    assert_eq!(old.cells.len(), 3);
    assert!(!old.columns.is_empty());
    let none = scan_worksheet(xml, &table, &styles, false, &mut DecodeBudget::new())
        .unwrap()
        .styling;
    assert!(none.is_empty(), "no styling unless asked for");
    let no_xf = scan_worksheet(xml, &table, &[], true, &mut DecodeBudget::new())
        .unwrap()
        .styling;
    assert_eq!(no_xf, legacy_worksheet_styling(xml, &[]));
}

#[test]
fn malformed_markup_fails_closed() {
    for xml in [
        "<worksheet><sheetData><row><c r=\"A1\"><v>1</v>",
        "<worksheet><c r=A1><v>1</v></c></worksheet>",
        "<worksheet><c r=\"A1\"><v>1</c",
    ] {
        let shared = shared_fixture();
        let _ = new_cells(xml, &shared);
    }
    assert!(new_cells("<worksheet><c r=A1><v>1</v></c></worksheet>", &[]).is_err());
}

#[test]
fn hostile_sheets_stay_linear_and_bounded() {
    // Hundreds of thousands of cells, a huge value, and deep junk nesting need no event limit.
    let mut body = String::new();
    for i in 0..200_000u32 {
        body.push_str(&format!("<c r=\"A{}\"><v>{i}</v></c>", i + 1));
    }
    let cells = new_cells(&sheet(&body), &[]).unwrap();
    assert_eq!(cells.len(), 200_000);
    let big = sheet(&format!("<c r=\"A1\"><v>{}</v></c>", "9".repeat(8 << 20)));
    assert_eq!(new_cells(&big, &[]).unwrap()[0].2.len(), 8 << 20);
    let deep = sheet(&format!(
        "<c r=\"A1\"><v>1</v>{}{}</c>",
        "<x>".repeat(100_000),
        "</x>".repeat(100_000)
    ));
    assert_eq!(new_cells(&deep, &[]).unwrap().len(), 1);
    // A truncated entity stays literal text.
    let cells = new_cells(&sheet("<c r=\"A1\"><v>1&amp</v></c>"), &[]).unwrap();
    assert_eq!(cells[0].2, "1&amp");
}

#[test]
fn cdata_text_is_taken_literally_without_markup() {
    // The substring scanner kept the `<![CDATA[` wrapper in the value; the reader unwraps it.
    let shared = shared_fixture();
    let cells = new_cells(
        &sheet(r#"<c r="A1" t="s"><v><![CDATA[1]]></v></c>"#),
        &shared,
    )
    .unwrap();
    assert_eq!(cells, vec![(0, 0, "나".to_string())]);
}

/// A 1,000 x 104 sheet: text through the shared table, numbers, and a quarter formulas.
fn big_sheet() -> (String, Vec<String>) {
    let shared: Vec<String> = (0..500).map(|i| format!("문자열 {i} text")).collect();
    let mut xml = String::from("<worksheet><sheetData>");
    for r in 0..1_000u32 {
        xml.push_str(&format!("<row r=\"{}\">", r + 1));
        for c in 0..104u32 {
            let a1 = format!("{}{}", crate::rtf_xml_helpers::format_col_letters(c), r + 1);
            match (r + c) % 4 {
                0 => xml.push_str(&format!(
                    "<c r=\"{a1}\" t=\"s\"><v>{}</v></c>",
                    (r * c) % 500
                )),
                1 => xml.push_str(&format!("<c r=\"{a1}\"><v>{}.5</v></c>", r * c)),
                2 => xml.push_str(&format!(
                    "<c r=\"{a1}\"><f>SUM(A1:B{})</f><v>3</v></c>",
                    r + 1
                )),
                _ => xml.push_str(&format!("<c r=\"{a1}\" s=\"1\"><v>{}</v></c>", r + c)),
            }
        }
        xml.push_str("</row>");
    }
    xml.push_str("</sheetData></worksheet>");
    (xml, shared)
}

#[test]
#[ignore = "measurement: cargo test --release -p easyoffice-formats --lib xlsx_sheet::tests::measure -- --ignored --nocapture"]
fn measure_one_pass_against_the_substring_scanner() {
    let (xml, shared) = big_sheet();
    let table = SharedStrings::from_strings(&shared);
    let time = |f: &dyn Fn() -> usize| {
        let mut best = f64::MAX;
        for _ in 0..15 {
            let t = std::time::Instant::now();
            assert_eq!(std::hint::black_box(f()), 104_000);
            best = best.min(t.elapsed().as_secs_f64() * 1000.0);
        }
        best
    };
    let old = time(&|| {
        Legacy::parse_xlsx_worksheet_cells(&xml, &shared)
            .unwrap()
            .len()
    });
    let new = time(&|| {
        scan_worksheet(&xml, &table, &[], false, &mut DecodeBudget::new())
            .unwrap()
            .cells
            .len()
    });
    println!(
        "xlsx sheet {} bytes: substring scanner {old:.2} ms, one-pass {new:.2} ms",
        xml.len()
    );
}

#[test]
fn hidden_rows_and_columns_and_merges_are_read() {
    let xml = r#"<worksheet><cols><col min="2" max="3" width="9" hidden="true"/><col min="5" max="5" width="12" customWidth="1"/></cols><sheetData><row r="1"><c r="A1"><v>1</v></c></row><row r="2" hidden="1"><c r="A2"><v>2</v></c></row><row r="3" hidden="0"/><row r="4" hidden="1"/></sheetData><mergeCells count="3"><mergeCell ref="A1:C1"/><mergeCell ref="B5"/><mergeCell ref="x"/></mergeCells></worksheet>"#;
    let table = SharedStrings::from_strings(&[]);
    let scan = scan_worksheet(xml, &table, &[], true, &mut DecodeBudget::new()).unwrap();
    assert_eq!(scan.styling.hidden_rows, vec![(1, 1), (3, 3)]);
    assert_eq!(scan.styling.hidden_cols, vec![(1, 2)]);
    assert_eq!(scan.styling.merges.len(), 1);
    assert_eq!(scan.styling.merges[0].to_a1(), "A1:C1");
    assert_eq!(scan.styling.columns.len(), 1);
    let none = scan_worksheet(xml, &table, &[], false, &mut DecodeBudget::new()).unwrap();
    assert!(none.styling.is_empty());
}

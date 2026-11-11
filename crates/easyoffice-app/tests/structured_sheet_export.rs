//! Saving a multi-sheet workbook as XLS keeps every sheet with computed values; CSV and TSV hold
//! one sheet, say so, and never cut the sheet at an arbitrary size without a warning.

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::LegacyOle2Codec;

fn json_str_field<'a>(json: &'a str, key: &str) -> &'a str {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    &json[start..end]
}

fn convert(payload: &str) -> (String, Vec<u8>) {
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", payload).unwrap();
    let bytes =
        EasyOfficeIpcRouter::decode_base64(json_str_field(&json, "convertedBase64")).unwrap();
    (json, bytes)
}

fn warnings(json: &str) -> &str {
    let start = json.find("\"warnings\":[").unwrap();
    let end = json[start..].find(']').unwrap() + start;
    &json[start..end]
}

fn csv_text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec())
        .unwrap()
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
}

const SHEETS: &str = r#""sheets":[
  {"name":"매출","cells":[{"ref":"A1","raw":"10"},{"ref":"A2","raw":"20"},{"ref":"A3","raw":"=SUM(A1:A2)"},{"ref":"B1","raw":"분기"}]},
  {"name":"요약","cells":[{"ref":"A1","raw":"합계 두 배"},{"ref":"B1","raw":"=매출!A3*2"}]},
  {"name":"빈 시트","cells":[]}
]"#;

fn payload(dst: &str, sheets: &str) -> String {
    format!(r#"{{"sourceFormat":"xlsx","targetFormat":"{dst}","title":"예산",{sheets}}}"#)
}

#[test]
fn xls_save_keeps_every_sheet_and_writes_computed_values_for_formulas() {
    let (json, bytes) = convert(&payload("xls", SHEETS));
    let sheets = LegacyOle2Codec::unpack_xls_multi(&bytes).unwrap();
    assert_eq!(
        sheets
            .iter()
            .map(|s| s.sheet_name.as_str())
            .collect::<Vec<_>>(),
        ["매출", "요약", "빈 시트"]
    );
    let cell = |s: usize, row: u32, col: u32| {
        sheets[s]
            .cells
            .iter()
            .find(|(r, c, _)| (*r, *c) == (row, col))
            .map(|(_, _, v)| v.as_str())
    };
    assert_eq!(
        cell(0, 2, 0),
        Some("30"),
        "SUM is stored as its value, not as formula text"
    );
    assert_eq!(cell(0, 0, 1), Some("분기"));
    assert_eq!(cell(1, 0, 1), Some("60"), "cross-sheet references resolve");
    assert!(
        sheets
            .iter()
            .flat_map(|s| &s.cells)
            .all(|(_, _, v)| v != "=SUM(A1:A2)"),
        "no formula text is written into a cell"
    );
    assert!(warnings(&json).contains("수식"), "{json}");
}

#[test]
fn csv_and_tsv_save_the_first_sheet_with_values_and_warn_about_the_rest() {
    for (dst, sep) in [("csv", ","), ("tsv", "\t")] {
        let (json, bytes) = convert(&payload(dst, SHEETS));
        let text = csv_text(&bytes);
        assert_eq!(
            text.lines().collect::<Vec<_>>(),
            [
                format!("10{sep}분기"),
                format!("20{sep}"),
                format!("30{sep}")
            ],
            "{dst}"
        );
        let w = warnings(&json);
        assert!(w.contains("첫 시트"), "{dst}: {w}");
        assert!(
            w.contains("요약") && w.contains("빈 시트"),
            "{dst}: the dropped sheets are named: {w}"
        );
    }
}

#[test]
fn single_sheet_workbooks_save_to_csv_without_a_sheet_warning() {
    let one = r#""sheets":[{"name":"매출","cells":[{"ref":"A1","raw":"1"}]}]"#;
    let (json, _) = convert(&payload("csv", one));
    assert!(!warnings(&json).contains("시트"), "{json}");
}

#[test]
fn csv_keeps_cells_beyond_the_old_512_by_64_cut() {
    let wide = r#""sheets":[{"name":"큰 시트","cells":[
      {"ref":"A1","raw":"처음"},{"ref":"A700","raw":"아래"},{"ref":"BO1","raw":"오른쪽"}]}]"#;
    let (json, bytes) = convert(&payload("csv", wide));
    let lines: Vec<String> = csv_text(&bytes).lines().map(str::to_string).collect();
    assert_eq!(lines.len(), 700, "row 700 is kept");
    assert_eq!(lines[699].split(',').next(), Some("아래"));
    assert!(
        lines[0].ends_with(",오른쪽"),
        "column BO (67) is kept: {}",
        lines[0]
    );
    assert!(!warnings(&json).contains("제외"), "nothing was cut: {json}");
}

#[test]
fn cells_past_what_the_csv_reader_accepts_are_dropped_with_a_warning() {
    let past = r#""sheets":[{"name":"끝","cells":[{"ref":"A1","raw":"남음"},{"ref":"A1000002","raw":"넘침"}]}]"#;
    let (json, bytes) = convert(&payload("csv", past));
    let text = csv_text(&bytes);
    assert!(text.starts_with("남음"), "{text}");
    assert!(!text.contains("넘침"));
    assert!(
        warnings(&json).contains("1개") && warnings(&json).contains("제외"),
        "{json}"
    );
}

#[test]
fn a_sheet_too_sparse_and_wide_to_write_as_a_grid_is_refused_not_cut() {
    let huge = r#""sheets":[{"name":"거대","cells":[{"ref":"A1000000","raw":"x"},{"ref":"XFD1","raw":"y"}]}]"#;
    let err = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload("csv", huge))
        .unwrap_err()
        .to_string();
    assert!(err.contains("너무 커서"), "{err}");
}

#[test]
fn xls_save_preserves_computed_string_starting_with_equals_as_text() {
    let eq_str = r#""sheets":[{"name":"수식문자열","cells":[{"ref":"A1","raw":"=CONCAT(\"=\",\"SUM(A1:A2)\")"},{"ref":"A2","raw":"=\"=\""}]}]"#;
    let (_, bytes) = convert(&payload("xls", eq_str));
    let sheets = LegacyOle2Codec::unpack_xls_multi(&bytes).unwrap();
    assert_eq!(
        sheets[0].cells,
        vec![(0, 0, "=SUM(A1:A2)".to_string()), (1, 0, "=".to_string()),],
        "computed string starting with '=' must be stored in SST, not as a 0.0 BIFF8 formula"
    );
}

#[test]
fn ods_save_writes_evaluated_formula_display_and_float_value() {
    use easyoffice_formats::{OdfCodec, ZipArchiveCodec, ZipSecurityPolicy};
    let (_, bytes) = convert(&payload("ods", SHEETS));
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let content_xml = entries
        .iter()
        .find(|e| e.path == "content.xml")
        .map(|e| std::str::from_utf8(&e.data).unwrap())
        .unwrap();
    assert!(
        content_xml.contains("office:value-type=\"float\" office:value=\"30\"><text:p>30</text:p>"),
        "{content_xml}"
    );
    assert!(
        content_xml.contains("office:value-type=\"float\" office:value=\"60\"><text:p>60</text:p>"),
        "{content_xml}"
    );
    let unpacked = OdfCodec::unpack_ods_multi(&bytes).unwrap();
    assert!(unpacked[0].1.contains(&(2, 0, "=SUM(A1:A2)".to_string())));
}

fn first_table(bytes: &[u8], file: &str) -> Vec<Vec<String>> {
    use easyoffice_formats::{RichBlock, UniversalFormatConverter};
    let decoded = UniversalFormatConverter::decode_rich(bytes, Some(file)).unwrap();
    let easyoffice_formats::DecodedContent::Document(doc) = decoded.content else {
        panic!("document expected");
    };
    doc.blocks
        .into_iter()
        .find_map(|b| match b {
            RichBlock::Table(t) => Some(t.rows),
            _ => None,
        })
        .expect("table block")
}

#[test]
fn document_export_of_a_sparse_sheet_is_a_rectangular_grid_over_the_used_range() {
    let sparse = r#""sheets":[{"name":"성기","cells":[
      {"ref":"B2","raw":"a"},{"ref":"D2","raw":"c"},{"ref":"B5","raw":"d"}]}]"#;
    let (_, bytes) = convert(&payload("docx", sparse));
    let rows = first_table(&bytes, "x.docx");
    assert_eq!(
        rows,
        vec![
            vec!["a", "", "c"],
            vec!["", "", ""],
            vec!["", "", ""],
            vec!["d", "", ""],
        ],
        "empty rows inside the used range are kept and every row has the same width"
    );
}

#[test]
fn a_document_grid_over_a_huge_used_range_is_refused_not_cut() {
    let huge = r#""sheets":[{"name":"거대","cells":[{"ref":"A1","raw":"x"},{"ref":"XFD1048576","raw":"y"}]}]"#;
    let err = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload("docx", huge))
        .unwrap_err()
        .to_string();
    assert!(err.contains("너무 커서"), "{err}");
}

#[test]
fn xlsx_and_ods_sources_convert_directly_with_bold_size_and_column_width_kept() {
    use easyoffice_formats::{
        read_sheet_styling, CellStyle, DocumentFormat, OoxmlCodec, SheetStyling,
    };
    let cells = vec![(0u32, 0u32, "굵게".to_string()), (1, 1, "값".to_string())];
    let mut styling = SheetStyling::default();
    styling.cells.insert(
        (0, 0),
        CellStyle {
            bold: true,
            size_pt: Some(14.0),
            ..CellStyle::default()
        },
    );
    styling.columns.insert(1, 160);
    let xlsx = OoxmlCodec::pack_xlsx_multi_styled(
        &[("시트".to_string(), cells)],
        &[],
        std::slice::from_ref(&styling),
    )
    .unwrap();
    let direct = |bytes: &[u8], file: &str, dst: &str| {
        let payload = format!(
            r#"{{"fileName":"{file}","targetFormat":"{dst}","sourceBase64":"{}"}}"#,
            EasyOfficeIpcRouter::encode_base64(bytes)
        );
        convert(&payload).1
    };
    let ods = direct(&xlsx, "s.xlsx", "ods");
    let back = read_sheet_styling(&ods, DocumentFormat::Ods).unwrap();
    assert_eq!(back[0].1, styling, "XLSX to ODS keeps the styling");
    let xlsx_again = direct(&ods, "s.ods", "xlsx");
    let back = read_sheet_styling(&xlsx_again, DocumentFormat::Xlsx).unwrap();
    assert_eq!(back[0].1, styling, "ODS to XLSX keeps the styling");
}

#[test]
fn xls_save_renames_sheets_to_the_excel_rules_and_says_so() {
    let long = "가".repeat(40);
    let sheets = format!(
        r#""sheets":[{{"name":"예산:1/4","cells":[{{"ref":"A1","raw":"1"}}]}},
        {{"name":"{long}","cells":[]}},{{"name":"요약","cells":[]}}]"#
    );
    let (json, bytes) = convert(&payload("xls", &sheets));
    let names: Vec<String> = LegacyOle2Codec::unpack_xls_multi(&bytes)
        .unwrap()
        .into_iter()
        .map(|s| s.sheet_name)
        .collect();
    assert_eq!(names, ["예산_1_4", &"가".repeat(31), "요약"]);
    assert!(
        json.contains("시트 2개의 이름을 바꿨습니다") && json.contains("'예산:1/4' → '예산_1_4'"),
        "{json}"
    );
    let (json, _) = convert(&payload("xls", SHEETS));
    assert!(!json.contains("이름을 바꿨습니다"), "{json}");
}

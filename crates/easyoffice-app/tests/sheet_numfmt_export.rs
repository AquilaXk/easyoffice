//! A cell's number format shows in the PDF and in the exports to other formats (PD-235): the text
//! is the one the grid and `TEXT` give, a format colour reaches the PDF, and a CSV holds the
//! displayed text of a formatted cell.

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{
    CellStyle, OdfCodec, OoxmlCodec, RichBlock, SheetStyling, UniversalFormatConverter, CUSTOM_BASE,
};
use std::sync::Arc;

fn convert(payload: &str) -> Vec<u8> {
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", payload).unwrap();
    let marker = "\"convertedBase64\":\"";
    let start = json.find(marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    EasyOfficeIpcRouter::decode_base64(&json[start..end]).unwrap()
}

const CELLS: &str = r##""numFmts":["yyyy-mm-dd","#,##0.00;[Red]-#,##0.00","0%","_(\"$\"* #,##0.00_)","[>100]\"big\";0.0"],"cells":[
  {"ref":"A1","raw":"45000","style":{"nf":1}},
  {"ref":"A2","raw":"1234.5","style":{"nf":2}},
  {"ref":"A3","raw":"-1234.5","style":{"nf":2}},
  {"ref":"A4","raw":"0.256","style":{"nf":3}},
  {"ref":"A5","raw":"0.30000000000000004"},
  {"ref":"A6","raw":"=A4*2","style":{"nf":3}},
  {"ref":"A7","raw":"99.5","style":{"nf":5}},
  {"ref":"A8","raw":"101","style":{"nf":5}},
  {"ref":"A9","raw":"7.5","style":{"nf":4}}"##;

/// Column A is 120 px wide: the formatted texts do not fit the 64 px of a default column (PD-305), and a PDF cuts what does not.
fn payload(target: &str) -> String {
    format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"{target}","title":"t","sheets":[{{"name":"s","columns":[{{"index":1,"widthPx":120}}],{CELLS}]}}]}}"#
    )
}

fn csv_rows(bytes: &[u8]) -> Vec<String> {
    String::from_utf8(bytes.to_vec())
        .unwrap()
        .trim_start_matches('\u{feff}')
        .lines()
        .map(|line| line.strip_suffix(',').unwrap_or(line).to_string())
        .collect()
}

#[test]
fn csv_holds_the_displayed_text_of_formatted_cells_and_the_stored_text_of_the_rest() {
    let rows = csv_rows(&convert(&payload("csv")));
    assert_eq!(
        rows,
        [
            "2023-03-15",
            "\"1,234.50\"",
            "\"-1,234.50\"",
            "26%",
            "0.30000000000000004",
            "51%",
            "99.5",
            "big",
            " $ 7.50 "
        ]
    );
}

#[test]
fn pdf_shows_formatted_text_and_colours_the_negative_section_red() {
    let pdf = String::from_utf8_lossy(&convert(&payload("pdf"))).into_owned();
    for shown in [
        "(2023-03-15)",
        "(1,234.50)",
        "(-1,234.50)",
        "(26%)",
        "(51%)",
        "(0.3)",
        "(big)",
    ] {
        assert!(pdf.contains(shown), "{shown} is missing");
    }
    assert!(!pdf.contains("0.30000000000000004") && !pdf.contains("45000"));
    let red = pdf
        .find("1.000 0.000 0.000 rg")
        .expect("red section colour");
    assert!(
        pdf[red..].starts_with("1.000 0.000 0.000 rg")
            && pdf[red..red + 200].contains("(-1,234.50)")
    );
    assert_eq!(
        pdf.matches("1.000 0.000 0.000 rg").count(),
        1,
        "only the negative is red"
    );
    // The accounting format fills the cell: the dollar sign and the number are far apart.
    assert!(pdf.contains("$        ") && pdf.contains("7.50 )"), "{pdf}");
}

#[test]
fn a_document_table_holds_the_formatted_text() {
    let bytes = convert(&payload("docx"));
    let decoded = UniversalFormatConverter::decode_rich(&bytes, Some("x.docx")).unwrap();
    let easyoffice_formats::DecodedContent::Document(doc) = decoded.content else {
        panic!("document");
    };
    let table = doc
        .blocks
        .into_iter()
        .find_map(|b| match b {
            RichBlock::Table(t) => Some(t.rows),
            _ => None,
        })
        .expect("table");
    let column: Vec<&str> = table.iter().map(|row| row[0].as_str()).collect();
    assert_eq!(
        column,
        [
            "2023-03-15",
            "1,234.50",
            "-1,234.50",
            "26%",
            "0.3",
            "51%",
            "99.5",
            "big",
            " $ 7.50 "
        ]
    );
}

type WorkbookSheets = Vec<(String, Vec<(u32, u32, String)>)>;

fn styled_workbook() -> (WorkbookSheets, SheetStyling) {
    let cells = vec![
        (0, 0, "45000".to_string()),
        (1, 0, "-1234.5".to_string()),
        (2, 0, "0.256".to_string()),
    ];
    let mut styling = SheetStyling {
        num_fmts: Arc::new(vec![
            "yyyy-mm-dd".to_string(),
            "#,##0.00;[Red]-#,##0.00".to_string(),
            "0.0%".to_string(),
        ]),
        // Wide enough for the formatted texts, which a PDF cuts where a column ends (PD-305).
        columns: [(0, 120)].into(),
        ..SheetStyling::default()
    };
    for (row, id) in [(0, 0), (1, 1), (2, 2)] {
        styling.cells.insert(
            (row, 0),
            CellStyle {
                num_fmt: CUSTOM_BASE + id,
                ..CellStyle::default()
            },
        );
    }
    (vec![("s".to_string(), cells)], styling)
}

fn direct(bytes: &[u8], file: &str, target: &str) -> Vec<u8> {
    convert(&format!(
        r#"{{"fileName":"{file}","targetFormat":"{target}","sourceBase64":"{}"}}"#,
        EasyOfficeIpcRouter::encode_base64(bytes)
    ))
}

#[test]
fn xlsx_and_ods_files_convert_to_csv_and_pdf_with_their_number_formats() {
    let (sheets, styling) = styled_workbook();
    let xlsx =
        OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], std::slice::from_ref(&styling)).unwrap();
    let ods = OdfCodec::pack_ods_multi_styled(&sheets, std::slice::from_ref(&styling)).unwrap();
    for (bytes, name) in [(&xlsx, "f.xlsx"), (&ods, "f.ods")] {
        assert_eq!(
            csv_rows(&direct(bytes, name, "csv")),
            ["2023-03-15", "\"-1,234.50\"", "25.6%"],
            "{name}"
        );
        let pdf = String::from_utf8_lossy(&direct(bytes, name, "pdf")).into_owned();
        assert!(
            pdf.contains("(2023-03-15)") && pdf.contains("(-1,234.50)"),
            "{name}"
        );
        assert!(pdf.contains("1.000 0.000 0.000 rg"), "{name}");
    }
}

#[test]
fn an_xls_file_converts_to_csv_with_the_formats_it_carries() {
    let path = format!(
        "{}/../easyoffice-sheet/tests/data/xls_formats/DateFormats.xls",
        env!("CARGO_MANIFEST_DIR")
    );
    let bytes = std::fs::read(path).expect("fixture");
    let text = String::from_utf8(direct(&bytes, "DateFormats.xls", "csv")).unwrap();
    let lines: Vec<&str> = text.trim_start_matches('\u{feff}').lines().collect();
    assert_eq!(
        lines,
        [
            "US date,10-Aug-07",
            "US date,8/10/07",
            "UK date,2007-08-10",
            "UK date,\"Friday, August 10, 2007\"",
            "UK date,10/08/07"
        ]
    );
}

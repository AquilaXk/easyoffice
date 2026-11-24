//! End-to-end IPC and XLSX/ODS roundtrip tests verifying sheet PDF wrapped text and auto row height (`PD-343`).

use std::collections::BTreeMap;

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{CellStyle, OdfCodec, OoxmlCodec, SheetStyling};

fn convert_pdf_bytes_from_json(sheet_body: &str) -> Result<Vec<u8>, String> {
    let payload = format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"pdf","title":"t","sheets":[{{"name":"s",{sheet_body}}}]}}"#
    );
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload)
        .map_err(|e| e.to_string())?;
    let marker = "\"convertedBase64\":\"";
    let start = json.find(marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    Ok(EasyOfficeIpcRouter::decode_base64(&json[start..end]).unwrap())
}

fn convert_pdf_from_json(sheet_body: &str) -> Result<String, String> {
    convert_pdf_bytes_from_json(sheet_body)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

fn convert_pdf_from_bytes(source_format: &str, bytes: &[u8]) -> String {
    let b64 = EasyOfficeIpcRouter::encode_base64(bytes);
    let payload = format!(
        r#"{{"sourceFormat":"{source_format}","targetFormat":"pdf","title":"t","sourceBase64":"{b64}"}}"#
    );
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).unwrap();
    let marker = "\"convertedBase64\":\"";
    let start = json.find(marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    let out = EasyOfficeIpcRouter::decode_base64(&json[start..end]).unwrap();
    String::from_utf8_lossy(&out).into_owned()
}

fn rgb(value: u32) -> String {
    let c = |shift: u32| f64::from((value >> shift) & 0xFF) / 255.0;
    format!("{:.3} {:.3} {:.3}", c(16), c(8), c(0))
}

fn first_fill_height(pdf: &str, color: u32) -> f64 {
    let prefix = format!("q {} rg ", rgb(color));
    let line = pdf.lines().find(|l| l.starts_with(&prefix)).unwrap();
    let words: Vec<&str> = line.split(' ').collect();
    words[8].parse::<f64>().unwrap()
}

#[test]
fn structured_json_pdf_wraps_text_expands_auto_row_and_keeps_custom_row_height() {
    let body = concat!(
        r#""columnWidths":{"0":64},"#,
        r#""rowHeights":[[3,20]],"#,
        r#""cells":["#,
        r#"{"ref":"A1","raw":"Alpha Beta Gamma","style":{"wrap":true,"fill":"112233","ha":"center","va":"middle"}},"#,
        r#"{"ref":"A2","raw":"Next"}"#,
        r#",{"ref":"A3","raw":"Fixed1\nFixed2\nFixed3","style":{"wrap":true,"fill":"445566"}}"#,
        r#"]"#
    );
    let pdf = convert_pdf_from_json(body).unwrap();
    assert!(pdf.contains("(Alpha) Tj"), "{pdf}");
    assert!(pdf.contains("(Beta) Tj"), "{pdf}");
    assert!(pdf.contains("(Gamma) Tj"), "{pdf}");
    assert!((first_fill_height(&pdf, 0x112233) - 45.0).abs() < 0.05);
    assert!((first_fill_height(&pdf, 0x445566) - 20.0).abs() < 0.05);
    assert!(pdf.contains("re W n\n"));
}

#[test]
fn xlsx_and_ods_roundtrip_preserve_wrap_and_expand_sheet_pdf_rows() {
    let mut styling = SheetStyling {
        columns: BTreeMap::from([(0, 64)]),
        ..SheetStyling::default()
    };
    styling.cells.insert(
        (0, 0),
        CellStyle {
            wrap: true,
            fill: Some(0xAABBCC),
            ..CellStyle::default()
        },
    );
    styling.cells.insert(
        (1, 0),
        CellStyle {
            wrap: true,
            fill: Some(0xDDEEFF),
            ..CellStyle::default()
        },
    );
    let sheets = vec![(
        "Sheet1".to_string(),
        vec![
            (0, 0, "L1\nL2\nL3".to_string()),
            (1, 0, "WordA WordB WordC".to_string()),
            (2, 0, "Below".to_string()),
        ],
    )];

    let xlsx =
        OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], std::slice::from_ref(&styling)).unwrap();
    let xlsx_pdf = convert_pdf_from_bytes("xlsx", &xlsx);
    assert!(xlsx_pdf.contains("(L1) Tj"));
    assert!(xlsx_pdf.contains("(L2) Tj"));
    assert!(xlsx_pdf.contains("(L3) Tj"));
    assert!(xlsx_pdf.contains("(WordA) Tj"));
    assert!(xlsx_pdf.contains("(WordB) Tj"));
    assert!(xlsx_pdf.contains("(WordC) Tj"));
    assert!((first_fill_height(&xlsx_pdf, 0xAABBCC) - 45.0).abs() < 0.05);
    assert!((first_fill_height(&xlsx_pdf, 0xDDEEFF) - 45.0).abs() < 0.05);

    let ods = OdfCodec::pack_ods_multi_styled_with_cache(&sheets, &[], &[styling]).unwrap();
    let ods_pdf = convert_pdf_from_bytes("ods", &ods);
    assert!(ods_pdf.contains("(L1) Tj"));
    assert!(ods_pdf.contains("(L2) Tj"));
    assert!(ods_pdf.contains("(L3) Tj"));
    assert!(ods_pdf.contains("(WordA) Tj"));
    assert!(ods_pdf.contains("(WordB) Tj"));
    assert!(ods_pdf.contains("(WordC) Tj"));
    assert!((first_fill_height(&ods_pdf, 0xAABBCC) - 45.0).abs() < 0.05);
    assert!((first_fill_height(&ods_pdf, 0xDDEEFF) - 45.0).abs() < 0.05);
}

#[test]
fn large_sheet_with_sparse_wrapped_cells_stays_fast_and_within_size_budget() {
    let plain_cells: Vec<String> = (0..200u32)
        .flat_map(|r| {
            (0..100u32).map(move |c| {
                format!(
                    r#"{{"ref":"{}{}{}","raw":"42"}}"#,
                    if c >= 26 {
                        char::from(b'A' + u8::try_from(c / 26 - 1).unwrap()).to_string()
                    } else {
                        String::new()
                    },
                    char::from(b'A' + u8::try_from(c % 26).unwrap()),
                    r + 1
                )
            })
        })
        .collect();
    let wrapped_cells: Vec<String> = (0..200u32)
        .flat_map(|r| {
            (0..100u32).map(move |c| {
                let col = format!(
                    "{}{}",
                    if c >= 26 {
                        char::from(b'A' + u8::try_from(c / 26 - 1).unwrap()).to_string()
                    } else {
                        String::new()
                    },
                    char::from(b'A' + u8::try_from(c % 26).unwrap()),
                );
                if c == 0 && r < 10 {
                    format!(
                        r#"{{"ref":"{col}{}","raw":"W1 W2 W3","style":{{"wrap":true}}}}"#,
                        r + 1
                    )
                } else {
                    format!(r#"{{"ref":"{col}{}","raw":"42"}}"#, r + 1)
                }
            })
        })
        .collect();
    let plain_pdf =
        convert_pdf_bytes_from_json(&format!(r#""cells":[{}]"#, plain_cells.join(","))).unwrap();
    let started = std::time::Instant::now();
    let wrapped_pdf =
        convert_pdf_bytes_from_json(&format!(r#""cells":[{}]"#, wrapped_cells.join(","))).unwrap();
    assert!(started.elapsed().as_secs() < 5);
    assert!(
        wrapped_pdf.len() * 2 <= plain_pdf.len() * 3,
        "20k sheet with wrapped rows ({} bytes) exceeded 1.5x of plain ({} bytes)",
        wrapped_pdf.len(),
        plain_pdf.len()
    );
}

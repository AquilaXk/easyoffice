//! End-to-end IPC and XLSX/ODS roundtrip tests verifying that cell font styling
//! (`bold`, `italic`, `underline`, `strike`, `sizePt`) renders in engine sheet PDF output (PD-340).

use std::collections::BTreeMap;

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{CellStyle, CfKind, CfRule, CfStyle, OdfCodec, OoxmlCodec, SheetStyling};

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

#[test]
fn structured_json_pdf_renders_cell_font_bits_size_and_rule_flag_union() {
    let body = concat!(
        r#""cells":["#,
        r#"{"ref":"A1","raw":"Styled","style":{"bold":true,"italic":true,"underline":true,"strike":true,"sizePt":15,"fc":"114488"}},"#,
        r#"{"ref":"A2","raw":"Both","style":{"bold":true,"sizePt":12}},"#,
        r#"{"ref":"B2","raw":"Plain"}"#,
        r#"],"#,
        r#""rules":{"cf":[{"r":"A2","p":1,"t":"cellIs","op":"equal","a":"\"Both\"","fl":2}]}"#
    );
    let pdf = convert_pdf_from_json(body).unwrap();
    let blue = rgb(0x114488);
    assert!(pdf.contains("/F1 15.00 Tf"), "{pdf}");
    assert!(pdf.contains("/F1 12.00 Tf"), "{pdf}");
    assert!(pdf.contains("/F1 10.00 Tf"), "{pdf}");
    assert!(
        pdf.contains(&format!("{blue} rg {blue} RG 1 0 0.21 1")),
        "{pdf}"
    );
    assert!(pdf.contains(" 2 Tr 0.45 w "), "15pt bold outline: {pdf}");
    assert_eq!(
        pdf.lines()
            .filter(|l| l.contains(&format!("{blue} RG")))
            .count(),
        3,
        "bold stroke + underline + strike for A1"
    );
    // A2 combines cell bold (`2 Tr 0.36 w` at 12pt) with rule italic (`1 0 0.21 1`).
    assert!(pdf.contains(" 2 Tr 0.36 w "), "12pt bold outline: {pdf}");
    assert_eq!(pdf.matches(" 0.21 1 ").count(), 2, "A1 and A2 italic");
}

#[test]
fn structured_json_rejects_out_of_range_size_pt_fail_closed() {
    for bad_size in ["0", "-5", "409.5"] {
        let body = format!(r#""cells":[{{"ref":"A1","raw":"x","style":{{"sizePt":{bad_size}}}}}]"#);
        assert!(convert_pdf_from_json(&body).is_err(), "{bad_size}");
    }
}

#[test]
fn xlsx_and_ods_roundtrip_preserve_cell_font_styles_into_pdf() {
    let mut styling = SheetStyling {
        columns: BTreeMap::from([(0, 150)]),
        ..SheetStyling::default()
    };
    styling.cells.insert(
        (0, 0),
        CellStyle {
            bold: true,
            italic: true,
            underline: true,
            strike: true,
            size_pt: Some(14.0),
            font_color: Some(0x2266AA),
            ..CellStyle::default()
        },
    );
    styling.cells.insert(
        (1, 0),
        CellStyle {
            bold: true,
            size_pt: Some(13.5),
            ..CellStyle::default()
        },
    );
    let sheets = vec![(
        "Sheet1".to_string(),
        vec![(0, 0, "Title".to_string()), (1, 0, "Sub".to_string())],
    )];

    let mut xlsx_styling = styling.clone();
    xlsx_styling.rules.cf.push(CfRule {
        ranges: vec![easyoffice_formats::CellBox::cell(1, 0)],
        priority: 1,
        stop_if_true: false,
        kind: CfKind::Text {
            op: easyoffice_formats::TextOp::Contains,
            text: "Sub".to_string(),
        },
        style: CfStyle {
            flags: easyoffice_formats::FLAG_ITALIC,
            ..CfStyle::default()
        },
    });
    let xlsx = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[xlsx_styling]).unwrap();
    let xlsx_pdf = convert_pdf_from_bytes("xlsx", &xlsx);
    assert!(xlsx_pdf.contains("/F1 14.00 Tf"), "{xlsx_pdf}");
    assert!(xlsx_pdf.contains("/F1 13.50 Tf"), "{xlsx_pdf}");
    assert!(xlsx_pdf.contains(" 2 Tr 0.42 w "), "{xlsx_pdf}");
    assert_eq!(xlsx_pdf.matches(" 0.21 1 ").count(), 2, "{xlsx_pdf}");
    assert_eq!(
        xlsx_pdf
            .lines()
            .filter(|l| l.contains(&format!("{} RG", rgb(0x2266AA))))
            .count(),
        3
    );

    let ods = OdfCodec::pack_ods_multi_styled_with_cache(&sheets, &[], &[styling]).unwrap();
    let ods_pdf = convert_pdf_from_bytes("ods", &ods);
    assert!(ods_pdf.contains("/F1 14.00 Tf"), "{ods_pdf}");
    assert!(ods_pdf.contains("/F1 13.50 Tf"), "{ods_pdf}");
    assert!(ods_pdf.contains(" 2 Tr 0.42 w "), "{ods_pdf}");
    assert_eq!(ods_pdf.matches(" 0.21 1 ").count(), 1, "{ods_pdf}");
}

fn col_name(col: u32) -> String {
    let letter = |n: u32| char::from(b'A' + u8::try_from(n).unwrap());
    if col < 26 {
        letter(col).to_string()
    } else {
        format!("{}{}", letter(col / 26 - 1), letter(col % 26))
    }
}

#[test]
fn large_styled_sheet_coalesces_cell_font_runs_within_pdf_size_budget() {
    let plain_cells: Vec<String> = (0..200u32)
        .flat_map(|r| {
            (0..100u32).map(move |c| format!(r#"{{"ref":"{}{}","raw":"42"}}"#, col_name(c), r + 1))
        })
        .collect();
    let styled_cells: Vec<String> = (0..200u32)
        .flat_map(|r| {
            (0..100u32).map(move |c| {
                let style = if (c / 10) % 2 == 1 {
                    r#","style":{"bold":true,"sizePt":12,"fc":"9C0006"}"#
                } else {
                    ""
                };
                format!(r#"{{"ref":"{}{}","raw":"42"{style}}}"#, col_name(c), r + 1)
            })
        })
        .collect();
    let plain_pdf =
        convert_pdf_bytes_from_json(&format!(r#""cells":[{}]"#, plain_cells.join(","))).unwrap();
    let styled_pdf =
        convert_pdf_bytes_from_json(&format!(r#""cells":[{}]"#, styled_cells.join(","))).unwrap();
    assert!(
        styled_pdf.len() * 2 <= plain_pdf.len() * 3,
        "20k styled bands ({} bytes) exceeded 1.5x of plain ({} bytes)",
        styled_pdf.len(),
        plain_pdf.len()
    );
}

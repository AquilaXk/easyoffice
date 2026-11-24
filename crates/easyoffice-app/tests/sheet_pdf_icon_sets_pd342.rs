//! End-to-end IPC PDF export tests for conditional formatting icon sets (`PD-342`).

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{
    icon_set_index, CellBox, CfKind, CfRule, CfStyle, CfValue, OdfCodec, OoxmlCodec, SheetStyling,
    ValueKind,
};

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

fn convert_bytes(sheet: &str) -> (Vec<u8>, String) {
    let payload = format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"pdf","title":"icons","sheets":[{{"name":"s",{sheet}}}]}}"#
    );
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).unwrap();
    let marker = "\"convertedBase64\":\"";
    let start = json.find(marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    (
        EasyOfficeIpcRouter::decode_base64(&json[start..end]).unwrap(),
        json,
    )
}

fn convert(sheet: &str) -> (String, String) {
    let (bytes, json) = convert_bytes(sheet);
    (String::from_utf8_lossy(&bytes).into_owned(), json)
}

fn cells(raws: &[(&str, &str)]) -> String {
    let list: Vec<String> = raws
        .iter()
        .map(|(cell, raw)| format!(r#"{{"ref":"{cell}","raw":"{raw}"}}"#))
        .collect();
    format!(r#""cells":[{}]"#, list.join(","))
}

#[test]
fn icon_sets_evaluate_formulas_and_reverse_order_into_pdf_forms() {
    let sheet = format!(
        r##"{},"rules":{{"cf":[
          {{"r":"A1:A3","p":1,"t":"icons","set":"3Arrows","th":[{{"k":"num","v":"0"}},{{"k":"num","v":"15"}},{{"k":"num","v":"30"}}],"rev":true}},
          {{"r":"B1:B5","p":2,"t":"icons","set":"5Quarters","th":[{{"k":"num","v":"0"}},{{"k":"num","v":"20"}},{{"k":"num","v":"40"}},{{"k":"num","v":"60"}},{{"k":"num","v":"80"}}]}},
          {{"r":"C1:C4","p":3,"t":"icons","set":"4Rating","th":[{{"k":"num","v":"0"}},{{"k":"num","v":"25"}},{{"k":"num","v":"50"}},{{"k":"num","v":"75"}}]}}
        ]}}"##,
        cells(&[
            ("A1", "10"),
            ("A2", "=10+10"),
            ("A3", "40"),
            ("B1", "10"),
            ("B2", "30"),
            ("B3", "50"),
            ("B4", "70"),
            ("B5", "90"),
            ("C1", "10"),
            ("C2", "30"),
            ("C3", "60"),
            ("C4", "85"),
        ])
    );
    let (pdf, json) = convert(&sheet);
    assert!(!json.contains("조건부 서식"), "{json}");
    // 3Arrows (3) + 5Quarters (5) + 4Rating (4) = 12 distinct icon forms and 12 placements.
    assert_eq!(pdf.matches("/Subtype /Form").count(), 12, "{pdf}");
    assert_eq!(pdf.matches(" Do Q").count(), 12);
    // 4Rating levels 1..=3 use /ca 0.3 ExtGState.
    assert_eq!(pdf.matches("/ca 0.3").count(), 3);
    // All cell values remain visible since show defaults to true.
    assert!(pdf.contains("(20) Tj") && pdf.contains("(90) Tj") && pdf.contains("(85) Tj"));

    // Verify rev:true reversed 3Arrows: lowest value 10 gets green up (last icon), highest 40 gets red down.
    let green_up = "0.118 0.557 0.243 rg 12.000 20.000 m 21.000 4.000 l 3.000 4.000 l h f";
    let red_down = "0.851 0.188 0.145 rg 12.000 4.000 m 3.000 20.000 l 21.000 20.000 l h f";
    assert!(pdf.contains(green_up), "missing reversed green up arrow");
    assert!(pdf.contains(red_down), "missing reversed red down arrow");
}

#[test]
fn icon_set_with_show_false_hides_numbers_and_draws_centered_icons_via_ipc() {
    let sheet = format!(
        r##"{},"rules":{{"cf":[{{"r":"A1:C1","p":1,"t":"icons","set":"3Stars","th":[{{"k":"num","v":"0"}},{{"k":"num","v":"50"}},{{"k":"num","v":"80"}}],"show":false}}]}}"##,
        cells(&[("A1", "10"), ("B1", "60"), ("C1", "95")])
    );
    let (pdf, _) = convert(&sheet);
    assert!(!pdf.contains("(10) Tj") && !pdf.contains("(60) Tj") && !pdf.contains("(95) Tj"));
    assert_eq!(pdf.matches("/Subtype /Form").count(), 3);
    assert_eq!(pdf.matches(" Do Q").count(), 3);
}

fn column_name(col: u32) -> String {
    let letter = |n: u32| char::from(b'A' + u8::try_from(n).unwrap());
    if col < 26 {
        letter(col).to_string()
    } else {
        format!("{}{}", letter(col / 26 - 1), letter(col % 26))
    }
}

#[test]
fn big_sheet_20000_cells_with_icon_set_stays_compact_and_shares_five_forms() {
    let raws: Vec<String> = (0..200u32)
        .flat_map(|r| {
            (0..100u32).map(move |c| {
                format!(
                    r#"{{"ref":"{}{}","raw":"{}"}}"#,
                    column_name(c),
                    r + 1,
                    (r * 7 + c * 13) % 100
                )
            })
        })
        .collect();
    let cells_json = format!(r#""cells":[{}]"#, raws.join(","));
    let rules = r##","rules":{"cf":[{
        "r":"A1:CV200",
        "p":1,
        "t":"icons",
        "set":"5Arrows",
        "th":[{"k":"num","v":"0"},{"k":"num","v":"20"},{"k":"num","v":"40"},{"k":"num","v":"60"},{"k":"num","v":"80"}]
    }]}"##;

    let (plain, _) = convert_bytes(&cells_json);
    let (ruled, _) = convert_bytes(&format!("{cells_json}{rules}"));
    let ruled_text = String::from_utf8_lossy(&ruled);
    assert!(ruled_text.contains("/FlateDecode"));
    assert_eq!(ruled_text.matches("/Subtype /Form").count(), 5);
    assert!(
        ruled.len() < plain.len() * 2,
        "20k icon sheet too large: {} bytes with icons vs {} plain",
        ruled.len(),
        plain.len()
    );
}

#[test]
fn xlsx_and_ods_roundtrip_preserve_icon_sets_into_pdf() {
    let sheets = vec![(
        "Sheet1".to_string(),
        vec![
            (0, 0, "10".to_string()),
            (1, 0, "50".to_string()),
            (2, 0, "90".to_string()),
        ],
    )];
    let mut styling = SheetStyling::default();
    styling.rules.cf.push(CfRule {
        ranges: vec![CellBox {
            row0: 0,
            col0: 0,
            row1: 2,
            col1: 0,
        }],
        priority: 1,
        stop_if_true: false,
        kind: CfKind::IconSet {
            set: icon_set_index("3TrafficLights2").unwrap(),
            thresholds: vec![
                CfValue::new(ValueKind::Percent, "0"),
                CfValue::new(ValueKind::Percent, "33"),
                CfValue::new(ValueKind::Percent, "67"),
            ],
            reverse: false,
            show_value: false,
        },
        style: CfStyle::default(),
    });

    let xlsx = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[styling.clone()]).unwrap();
    let xlsx_pdf = convert_pdf_from_bytes("xlsx", &xlsx);
    assert_eq!(xlsx_pdf.matches("/Subtype /Form").count(), 3, "{xlsx_pdf}");
    assert_eq!(xlsx_pdf.matches(" Do Q").count(), 3);
    assert!(!xlsx_pdf.contains("(10) Tj") && !xlsx_pdf.contains("(90) Tj"));

    let ods = OdfCodec::pack_ods_multi_styled_with_cache(&sheets, &[], &[styling]).unwrap();
    let ods_pdf = convert_pdf_from_bytes("ods", &ods);
    assert_eq!(ods_pdf.matches("/Subtype /Form").count(), 3, "{ods_pdf}");
    assert_eq!(ods_pdf.matches(" Do Q").count(), 3);
    assert!(!ods_pdf.contains("(10) Tj") && !ods_pdf.contains("(90) Tj"));
}

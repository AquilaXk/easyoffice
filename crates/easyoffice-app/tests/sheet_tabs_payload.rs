//! The order of the sheets and the sheets that have no tab survive an XLSX and an ODS save and
//! open (PD-209); a file or a request that would leave no sheet shown, or that lies about the
//! flag, cannot.

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{
    read_sheet_styling, DocumentFormat, OoxmlCodec, ZipArchiveCodec, ZipSecurityPolicy,
};

fn json_str_field<'a>(json: &'a str, key: &str) -> &'a str {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    &json[start..end]
}

fn sheet(name: &str, extra: &str) -> String {
    format!(r#"{{"name":"{name}","cells":[{{"ref":"A1","raw":"{name}"}}]{extra}}}"#)
}

fn save(dst: &str, sheets: &[String]) -> Result<Vec<u8>, String> {
    let payload = format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"{dst}","title":"t","sheets":[{}]}}"#,
        sheets.join(",")
    );
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload)
        .map_err(|e| e.to_string())?;
    Ok(EasyOfficeIpcRouter::decode_base64(json_str_field(&json, "convertedBase64")).unwrap())
}

fn names_and_hidden(dst: &str, bytes: &[u8]) -> Vec<(String, bool)> {
    let format = match dst {
        "xlsx" => DocumentFormat::Xlsx,
        "xls" => DocumentFormat::Xls,
        _ => DocumentFormat::Ods,
    };
    read_sheet_styling(bytes, format)
        .unwrap()
        .into_iter()
        .map(|(name, styling)| (name, styling.hidden))
        .collect()
}

fn open(dst: &str, bytes: &[u8]) -> String {
    let payload = format!(
        r#"{{"action":"decode","fileName":"s.{dst}","sourceBase64":"{}"}}"#,
        EasyOfficeIpcRouter::encode_base64(bytes)
    );
    EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).unwrap()
}

fn workbook_xml(bytes: &[u8]) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let entry = entries
        .iter()
        .find(|e| e.path == "xl/workbook.xml")
        .unwrap();
    String::from_utf8(entry.data.clone()).unwrap()
}

#[test]
fn order_and_hidden_sheets_come_back_unchanged() {
    let sheets = [
        sheet("셋째", ""),
        sheet("첫째", r#","hidden":true"#),
        sheet("Sheet 3", r#","tabColor":"3366cc","hidden":true"#),
        sheet("마지막", ""),
    ];
    for dst in ["xlsx", "ods"] {
        let bytes = save(dst, &sheets).unwrap();
        let back = names_and_hidden(dst, &bytes);
        assert_eq!(
            back,
            vec![
                ("셋째".to_string(), false),
                ("첫째".to_string(), true),
                ("Sheet 3".to_string(), true),
                ("마지막".to_string(), false),
            ],
            "{dst}"
        );
        let opened = open(dst, &bytes);
        assert_eq!(
            opened.matches("\"hidden\":true").count(),
            2,
            "{dst}: {opened}"
        );
        // The order of the sheets is the order of the answer.
        let at = |name: &str| opened.find(&format!("\"name\":\"{name}\"")).unwrap();
        assert!(
            at("셋째") < at("첫째") && at("첫째") < at("Sheet 3") && at("Sheet 3") < at("마지막")
        );
        // A sheet that is shown says nothing about it.
        let plain = open(dst, &save(dst, &[sheet("A", ""), sheet("B", "")]).unwrap());
        assert!(!plain.contains("hidden"), "{dst}");
    }
}

#[test]
fn a_book_that_starts_with_a_hidden_sheet_opens_on_the_first_shown_one() {
    let bytes = save(
        "xlsx",
        &[sheet("숨김", r#","hidden":true"#), sheet("보임", "")],
    )
    .unwrap();
    let xml = workbook_xml(&bytes);
    assert!(
        xml.contains("<bookViews><workbookView activeTab=\"1\"/></bookViews><sheets>"),
        "{xml}"
    );
    assert!(
        xml.contains("name=\"숨김\" sheetId=\"1\" state=\"hidden\""),
        "{xml}"
    );
    let plain =
        workbook_xml(&save("xlsx", &[sheet("A", ""), sheet("B", r#","hidden":true"#)]).unwrap());
    assert!(!plain.contains("bookViews"), "{plain}");
}

#[test]
fn a_book_with_every_sheet_hidden_keeps_its_first_sheet_shown() {
    let all = [
        sheet("A", r#","hidden":true"#),
        sheet("B", r#","hidden":true"#),
    ];
    for dst in ["xlsx", "ods"] {
        let bytes = save(dst, &all).unwrap();
        assert_eq!(
            names_and_hidden(dst, &bytes),
            vec![("A".to_string(), false), ("B".to_string(), true)],
            "{dst}"
        );
    }
    // A file made elsewhere that hides every sheet opens on its first.
    let bytes = save("xlsx", &[sheet("A", ""), sheet("B", "")]).unwrap();
    let hidden = workbook_xml(&bytes)
        .replace("sheetId=\"1\"", "sheetId=\"1\" state=\"veryHidden\"")
        .replace("sheetId=\"2\"", "sheetId=\"2\" state=\"hidden\"");
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let repacked: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            let data = if e.path == "xl/workbook.xml" {
                hidden.clone().into_bytes()
            } else {
                e.data
            };
            (e.path, data)
        })
        .collect();
    let file = ZipArchiveCodec::pack(&repacked).unwrap();
    let styling = OoxmlCodec::unpack_xlsx_with_styling(&file).unwrap().1;
    assert_eq!(
        styling.iter().map(|(_, s)| s.hidden).collect::<Vec<_>>(),
        vec![false, true]
    );
}

#[test]
fn a_bad_hidden_value_refuses_the_save() {
    for extra in [
        r#","hidden":"yes""#,
        r#","hidden":1"#,
        r#","hidden":null"#,
        r#","hidden":[true]"#,
    ] {
        assert!(save("xlsx", &[sheet("A", extra)]).is_err(), "{extra}");
    }
}

#[test]
fn many_sheets_keep_their_order_and_flags() {
    // The save takes 256 sheets (`MAX_SHEETS` of the structured request) and refuses the next.
    let make = |n: usize| -> Vec<String> {
        (1..=n)
            .map(|i| {
                sheet(
                    &format!("S{i}"),
                    if i % 3 == 0 { r#","hidden":true"# } else { "" },
                )
            })
            .collect()
    };
    for dst in ["xlsx", "ods"] {
        let back = names_and_hidden(dst, &save(dst, &make(256)).unwrap());
        assert_eq!(back.len(), 256, "{dst}");
        for (i, (name, hidden)) in back.iter().enumerate() {
            assert_eq!(name, &format!("S{}", i + 1));
            assert_eq!(*hidden, (i + 1) % 3 == 0, "{name}");
        }
        assert!(save(dst, &make(257)).is_err(), "{dst}");
    }
}

fn states(dst: &str, bytes: &[u8]) -> Vec<(bool, bool)> {
    let format = match dst {
        "xlsx" => DocumentFormat::Xlsx,
        "xls" => DocumentFormat::Xls,
        _ => DocumentFormat::Ods,
    };
    read_sheet_styling(bytes, format)
        .unwrap()
        .into_iter()
        .map(|(_, styling)| (styling.hidden, styling.very_hidden))
        .collect()
}

#[test]
fn a_very_hidden_sheet_keeps_its_state_in_xlsx_and_xls_and_is_hidden_in_ods() {
    let sheets = [
        sheet("보임", ""),
        sheet("숨김", r#","hidden":true"#),
        sheet("깊은", r#","veryHidden":true"#),
    ];
    let want = vec![(false, false), (true, false), (true, true)];
    let xlsx = save("xlsx", &sheets).unwrap();
    let xml = workbook_xml(&xlsx);
    assert!(
        xml.contains("name=\"숨김\" sheetId=\"2\" state=\"hidden\""),
        "{xml}"
    );
    assert!(
        xml.contains("name=\"깊은\" sheetId=\"3\" state=\"veryHidden\""),
        "{xml}"
    );
    assert_eq!(states("xlsx", &xlsx), want);
    assert_eq!(states("xls", &save("xls", &sheets).unwrap()), want);
    assert_eq!(
        states("ods", &save("ods", &sheets).unwrap()),
        vec![(false, false), (true, false), (true, false)]
    );
    // The open answer tells the page which hidden sheet the menu must not list.
    let opened = open("xlsx", &xlsx);
    assert_eq!(opened.matches("\"veryHidden\":true").count(), 1, "{opened}");
    assert_eq!(opened.matches("\"hidden\":true").count(), 2, "{opened}");
}

#[test]
fn an_xls_that_hides_every_sheet_opens_on_the_first_and_a_request_that_lies_is_refused() {
    let all = [
        sheet("A", r#","veryHidden":true"#),
        sheet("B", r#","hidden":true"#),
    ];
    assert_eq!(
        states("xls", &save("xls", &all).unwrap()),
        vec![(false, false), (true, false)]
    );
    for extra in [
        r#","veryHidden":"yes""#,
        r#","veryHidden":1"#,
        r#","veryHidden":null"#,
    ] {
        assert!(save("xlsx", &[sheet("A", extra)]).is_err(), "{extra}");
    }
}

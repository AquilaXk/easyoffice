//! Cell hyperlinks over the save and open commands (PD-218, PD-223): the page's `links` reach XLSX, ODS and
//! XLS, come back when the file is opened and cross formats; a format without links keeps the text; a
//! request with an address the policy refuses is not saved.

use super::*;
use easyoffice_formats::StrictJsonValue;

fn dispatch(payload: &str) -> String {
    EasyOfficeIpcRouter::dispatch("convert_document_format", payload).unwrap()
}

fn base64_field(saved: &str) -> Vec<u8> {
    let start = saved.find("\"convertedBase64\":\"").unwrap() + 19;
    let end = start + saved[start..].find('"').unwrap();
    EasyOfficeIpcRouter::decode_base64(&saved[start..end]).unwrap()
}

const SHEETS: &str = concat!(
    "\"sheets\":[{\"name\":\"Data\",\"cells\":[",
    "{\"ref\":\"A1\",\"raw\":\"홈페이지\"},{\"ref\":\"A2\",\"raw\":\"둘째 시트\"},{\"ref\":\"A3\",\"raw\":\"=A1&\\\"!\\\"\"},",
    "{\"ref\":\"B2\",\"raw\":\"5\"}],",
    "\"links\":[{\"ref\":\"A1\",\"target\":\"https://example.com/a?b=1&c=2\",\"tip\":\"열기\"},",
    "{\"ref\":\"A2\",\"target\":\"#'두 번째'!B2\"},{\"ref\":\"A3\",\"target\":\"mailto:me@example.com\"}]},",
    "{\"name\":\"두 번째\",\"cells\":[{\"ref\":\"B2\",\"raw\":\"도착\"}]}]"
);

fn saved(format: &str) -> Vec<u8> {
    base64_field(&dispatch(&format!(
        "{{\"targetFormat\":\"{format}\",\"title\":\"l\",{SHEETS}}}"
    )))
}

fn opened(file: &str, bytes: &[u8]) -> StrictJsonValue {
    let json = dispatch(&format!(
        "{{\"action\":\"decode\",\"fileName\":\"{file}\",\"sourceBase64\":\"{}\"}}",
        EasyOfficeIpcRouter::encode_base64(bytes)
    ));
    StrictJsonValue::parse(&json).unwrap()
}

fn links_of(opened: &StrictJsonValue, sheet: usize) -> String {
    let sheets = StrictJsonValue::get_field(opened.as_object().unwrap(), "sheets").unwrap();
    let sheet = sheets.as_array().unwrap()[sheet].as_object().unwrap();
    StrictJsonValue::get_field(sheet, "links")
        .map_or_else(String::new, StrictJsonValue::to_json_string)
}

const EXPECTED: &str = concat!(
    "[{\"ref\":\"A1\",\"target\":\"https://example.com/a?b=1&c=2\",\"tip\":\"열기\"},",
    "{\"ref\":\"A2\",\"target\":\"#'두 번째'!B2\"},{\"ref\":\"A3\",\"target\":\"mailto:me@example.com\"}]"
);

#[test]
fn xlsx_and_ods_carry_the_links_back_to_the_page() {
    for (format, file) in [("xlsx", "l.xlsx"), ("ods", "l.ods")] {
        let back = opened(file, &saved(format));
        assert_eq!(links_of(&back, 0), EXPECTED, "{format}");
        assert_eq!(links_of(&back, 1), "", "{format}");
    }
}

#[test]
fn links_cross_from_xlsx_to_ods_and_back() {
    let xlsx = saved("xlsx");
    let ods = base64_field(&dispatch(&format!(
        "{{\"fileName\":\"l.xlsx\",\"sourceBase64\":\"{}\",\"targetFormat\":\"ods\"}}",
        EasyOfficeIpcRouter::encode_base64(&xlsx)
    )));
    assert_eq!(links_of(&opened("l.ods", &ods), 0), EXPECTED);
    let again = base64_field(&dispatch(&format!(
        "{{\"fileName\":\"l.ods\",\"sourceBase64\":\"{}\",\"targetFormat\":\"xlsx\"}}",
        EasyOfficeIpcRouter::encode_base64(&ods)
    )));
    assert_eq!(links_of(&opened("l.xlsx", &again), 0), EXPECTED);
}

#[test]
fn xls_keeps_the_links_and_csv_keeps_the_text_only() {
    assert_eq!(links_of(&opened("l.xls", &saved("xls")), 0), EXPECTED);
    assert_eq!(links_of(&opened("l.csv", &saved("csv")), 0), "");
    let csv = String::from_utf8(saved("csv")).unwrap();
    assert!(
        csv.contains("홈페이지") && !csv.contains("example.com"),
        "{csv}"
    );
}

#[test]
fn links_cross_through_xls_in_both_directions() {
    let convert = |file: &str, bytes: &[u8], target: &str| {
        base64_field(&dispatch(&format!(
            "{{\"fileName\":\"{file}\",\"sourceBase64\":\"{}\",\"targetFormat\":\"{target}\"}}",
            EasyOfficeIpcRouter::encode_base64(bytes)
        )))
    };
    let xls = convert("l.xlsx", &saved("xlsx"), "xls");
    assert_eq!(links_of(&opened("l.xls", &xls), 0), EXPECTED);
    let ods = convert("l.xls", &xls, "ods");
    assert_eq!(links_of(&opened("l.ods", &ods), 0), EXPECTED);
    let xlsx = convert("l.ods", &convert("l.xls", &saved("xls"), "ods"), "xlsx");
    assert_eq!(links_of(&opened("l.xlsx", &xlsx), 0), EXPECTED);
}

#[test]
fn an_xls_save_leaves_out_links_off_its_grid_with_a_warning() {
    let payload = concat!(
        "{\"targetFormat\":\"xls\",\"sheets\":[{\"name\":\"S\",\"cells\":[{\"ref\":\"A1\",\"raw\":\"x\"}],",
        "\"links\":[{\"ref\":\"A1\",\"target\":\"https://a.b/\"},{\"ref\":\"A70000\",\"target\":\"https://c.d/\"}]}]}"
    );
    let saved = dispatch(payload);
    assert!(saved.contains("하이퍼링크 1개는 XLS의 범위"), "{saved}");
    let back = opened("s.xls", &base64_field(&saved));
    assert_eq!(
        links_of(&back, 0),
        "[{\"ref\":\"A1\",\"target\":\"https://a.b/\"}]"
    );
}

#[test]
fn a_save_with_an_unsafe_address_is_refused() {
    for bad in [
        "{\"ref\":\"A1\",\"target\":\"javascript:alert(1)\"}",
        "{\"ref\":\"A1\",\"target\":\"file:///etc/passwd\"}",
        "{\"ref\":\"A1\",\"target\":\"data:text/html,x\"}",
        "{\"ref\":\"A1\",\"target\":\"\"}",
    ] {
        let payload = format!(
            "{{\"targetFormat\":\"xlsx\",\"sheets\":[{{\"name\":\"S\",\"cells\":[{{\"ref\":\"A1\",\"raw\":\"x\"}}],\"links\":[{bad}]}}]}}"
        );
        assert!(
            EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn links_a_file_could_not_keep_open_with_a_warning() {
    let xlsx = saved("xlsx");
    let mut entries = easyoffice_formats::ZipArchiveCodec::unpack(
        &xlsx,
        &easyoffice_formats::ZipSecurityPolicy::default(),
    )
    .unwrap();
    for e in &mut entries {
        if e.path.ends_with("sheet1.xml.rels") {
            e.data = String::from_utf8(e.data.clone())
                .unwrap()
                .replace("mailto:me@example.com", "file:///etc/passwd")
                .into_bytes();
        }
    }
    let hostile = easyoffice_formats::ZipArchiveCodec::pack_compressed(
        &entries
            .into_iter()
            .map(|e| (e.path, e.data))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let back = opened("h.xlsx", &hostile);
    assert!(!links_of(&back, 0).contains("file:"));
    let warnings = StrictJsonValue::get_field(back.as_object().unwrap(), "warnings")
        .unwrap()
        .to_json_string();
    assert!(warnings.contains("하이퍼링크 1개"), "{warnings}");
}

//! PD-264: every `evaluate_sheet` request that carries `date1904` calculates, formats and filters in the 1904 date system:
//! stateless evaluation, the session (`open`/`apply`), `format` and the date filter periods.

use easyoffice_app::EasyOfficeIpcRouter;

fn call(payload: &str) -> String {
    EasyOfficeIpcRouter::dispatch("evaluate_sheet", payload).unwrap()
}

/// 2024-01-01 is 45292 in the 1900 system and 43830 in the 1904 system.
#[test]
fn a_stateless_evaluation_counts_from_1904() {
    for (flag, want) in [("", "45292"), (r#","date1904":true"#, "43830")] {
        let json = call(&format!(
            r#"{{"sheet":"S","sheets":["S"],"expr":"=DATE(2024,1,1)","cells":[]{flag}}}"#
        ));
        assert!(json.contains(want), "{flag}: {json}");
    }
}

#[test]
fn a_session_calculates_in_the_system_of_its_requests() {
    let opened = call(r#"{"sessionAction":"open","sheets":["S"],"date1904":true}"#);
    let id = opened.split('"').nth(3).unwrap().to_string();
    let applied = call(&format!(
        r#"{{"sessionAction":"apply","session":"{id}","date1904":true,"changes":[{{"sheet":"S","cell":"A1","value":"=DATE(2024,1,1)"}}]}}"#
    ));
    assert!(applied.contains("43830"), "{applied}");
    call(&format!(r#"{{"sessionAction":"close","session":"{id}"}}"#));
}

#[test]
fn format_shows_the_date_of_the_system() {
    let shown = call(
        r#"{"sessionAction":"format","date1904":true,"items":[{"code":"yyyy-mm-dd","values":["43830","0"]}]}"#,
    );
    assert!(
        shown.contains("2024-01-01") && shown.contains("1904-01-01"),
        "{shown}"
    );
    let plain =
        call(r#"{"sessionAction":"format","items":[{"code":"yyyy-mm-dd","values":["45292"]}]}"#);
    assert!(plain.contains("2024-01-01"), "{plain}");
}

/// The page sends today's serial in the workbook's system; the period boundaries are computed from it.
#[test]
fn a_date_filter_period_is_cut_in_the_system() {
    // Today = 2024-01-15 (43844 in 1904). 43829 = 2023-12-31, 43830 = 2024-01-01, 43860 = 2024-01-31, 43861 = 2024-02-01.
    let data =
        r#"[{"col":1,"values":["43829","43830","43860","43861"],"fmt":[[0,3,"yyyy-mm-dd"]]}]"#;
    let filter = r#"{"range":"A1:A5","columns":[{"col":1,"dynamic":"thisMonth"}]}"#;
    let json = call(&format!(
        r#"{{"sessionAction":"filter","op":"apply","date1904":true,"today":43844,"filter":{filter},"data":{data}}}"#
    ));
    assert_eq!(json, r#"{"hidden":[[2,2],[5,5]],"count":2}"#);
}

/// A value pasted from `=DATE(2024,1,1)` in a 1904 workbook is saved as 43830 under `date1904`, which a spreadsheet program
/// reads as 2024-01-01 (the cached result of the formula too).
#[test]
fn a_saved_1904_workbook_holds_the_serials_of_its_own_system() {
    use easyoffice_formats::{ZipArchiveCodec, ZipSecurityPolicy};
    let payload = r#"{"sourceFormat":"xlsx","targetFormat":"xlsx","title":"t","date1904":true,"sheets":[{"name":"S","cells":[
        {"ref":"A1","raw":"43830"},{"ref":"A2","raw":"=DATE(2024,1,1)"}]}]}"#;
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", payload).unwrap();
    let marker = "\"convertedBase64\":\"";
    let start = json.find(marker).unwrap() + marker.len();
    let b64 = &json[start..json[start..].find('"').unwrap() + start];
    let bytes = EasyOfficeIpcRouter::decode_base64(b64).unwrap();
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let text = |path: &str| {
        String::from_utf8(
            entries
                .iter()
                .find(|e| e.path == path)
                .unwrap()
                .data
                .clone(),
        )
        .unwrap()
    };
    assert!(text("xl/workbook.xml").contains("date1904=\"1\""));
    let sheet = text("xl/worksheets/sheet1.xml");
    assert_eq!(sheet.matches("<v>43830</v>").count(), 2, "{sheet}");
}

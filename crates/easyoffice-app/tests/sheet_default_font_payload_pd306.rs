//! The default font of a workbook reaches the page and comes back (PD-306): a CSV, a new workbook and an ODS have the standard
//! font (Malgun Gothic 11 pt, 72 px columns) and say nothing of it; an XLSX with another Normal font says it as
//! `defaultFont` with the `defaultColPx` of a sheet that says nothing, and a save that carries `defaultFont` writes the same
//! font again with the widths of the columns in the same digit. A font that is not a bounded font refuses the save.

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

fn decode(file_name: &str, bytes: &[u8]) -> String {
    let payload = format!(
        r#"{{"action":"decode","fileName":"{file_name}","sourceBase64":"{}"}}"#,
        EasyOfficeIpcRouter::encode_base64(bytes)
    );
    EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).unwrap()
}

/// A workbook of one sheet with a column of 100 px, saved to `dst`; `font` is a top-level key such as `,"defaultFont":null`.
fn save(dst: &str, font: &str) -> Result<Vec<u8>, String> {
    let payload = format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"{dst}","title":"t"{font},"sheets":[{{"name":"시트","cells":[{{"ref":"A1","raw":"가"}}],"columns":[{{"index":2,"widthPx":100}},{{"index":3,"widthPx":72}}]}}]}}"#
    );
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload)
        .map_err(|e| e.to_string())?;
    Ok(EasyOfficeIpcRouter::decode_base64(json_str_field(&json, "convertedBase64")).unwrap())
}

fn part(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let entry = entries.into_iter().find(|e| e.path == path).unwrap();
    String::from_utf8(entry.data).unwrap()
}

fn first_font(bytes: &[u8]) -> String {
    let styles = part(bytes, "xl/styles.xml");
    let from = &styles[styles.find("<fonts").unwrap()..];
    let start = from.find("<font>").unwrap();
    from[start..start + from[start..].find("</font>").unwrap() + 7].to_string()
}

#[test]
fn a_save_without_a_font_writes_the_standard_one_and_the_page_is_told_nothing() {
    let bytes = save("xlsx", "").unwrap();
    assert_eq!(
        first_font(&bytes),
        "<font><sz val=\"11\"/><name val=\"맑은 고딕\"/></font>"
    );
    // 100 px and 72 px are 12.5 and 9 characters of the 8 px digit.
    let sheet = part(&bytes, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains(r#"<col min="2" max="2" width="12.5" customWidth="1"/>"#));
    assert!(sheet.contains(r#"<col min="3" max="3" width="9" customWidth="1"/>"#));
    let opened = decode("s.xlsx", &bytes);
    assert!(!opened.contains("defaultFont") && !opened.contains("defaultColPx"));
    // The standard font named by the request is the same as none.
    let named = save("xlsx", r#","defaultFont":{"name":"맑은 고딕","size":11}"#).unwrap();
    assert_eq!(first_font(&named), first_font(&bytes));
    let null = save("xlsx", r#","defaultFont":null"#).unwrap();
    assert_eq!(first_font(&null), first_font(&bytes));
}

#[test]
fn a_csv_opens_with_the_standard_font_and_saves_as_a_korean_workbook() {
    let opened = decode("a.csv", "가,나\n1,2\n".as_bytes());
    assert!(!opened.contains("defaultFont"), "{opened}");
    let tsv = decode("a.tsv", "가\t나\n".as_bytes());
    assert!(!tsv.contains("defaultFont"), "{tsv}");
    let payload = r#"{"sourceFormat":"csv","targetFormat":"xlsx","title":"t","sheets":[{"name":"a","cells":[{"ref":"A1","raw":"가"}]}]}"#;
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", payload).unwrap();
    let bytes =
        EasyOfficeIpcRouter::decode_base64(json_str_field(&json, "convertedBase64")).unwrap();
    assert_eq!(
        first_font(&bytes),
        "<font><sz val=\"11\"/><name val=\"맑은 고딕\"/></font>"
    );
    let styling = read_sheet_styling(&bytes, DocumentFormat::Xlsx)
        .unwrap()
        .remove(0)
        .1;
    assert_eq!(styling.default_col_px(), 72);
}

/// A file with Arial 12 pt (a 9 px digit): the page is told the font and the column of a sheet that says nothing; saving what
/// the page got back writes Arial 12 again and the columns stay the same pixels.
#[test]
fn another_normal_font_round_trips_through_the_page() {
    const FONT: &str = r#","defaultFont":{"name":"Arial","size":12}"#;
    let bytes = save("xlsx", FONT).unwrap();
    assert_eq!(
        first_font(&bytes),
        "<font><sz val=\"12\"/><name val=\"Arial\"/></font>"
    );
    // 100 px and 72 px are 11.111... and 8 characters of the 9 px digit.
    let sheet = part(&bytes, "xl/worksheets/sheet1.xml");
    assert!(
        sheet.contains(r#"<col min="2" max="2" width="11.109375" customWidth="1"/>"#),
        "{sheet}"
    );
    assert!(
        sheet.contains(r#"<col min="3" max="3" width="8" customWidth="1"/>"#),
        "{sheet}"
    );
    let opened = decode("s.xlsx", &bytes);
    assert!(
        opened.contains(r#","defaultFont":{"name":"Arial","size":12},"defaultColPx":80"#),
        "{opened}"
    );
    // The columns the page gets are the pixels it sent.
    assert!(
        opened.contains(r#""columns":[{"index":2,"widthPx":100},{"index":3,"widthPx":72}]"#),
        "{opened}"
    );
    let again = save("xlsx", FONT).unwrap();
    assert_eq!(part(&again, "xl/styles.xml"), part(&bytes, "xl/styles.xml"));
    let read = read_sheet_styling(&again, DocumentFormat::Xlsx)
        .unwrap()
        .remove(0)
        .1;
    assert_eq!(read.columns, [(1, 100), (2, 72)].into());
    assert_eq!(read.default_col_px(), 80);
    // A workbook the engine wrote itself without a font of its own is read as the standard one.
    let new =
        OoxmlCodec::pack_xlsx_multi(&[("S".to_string(), vec![(0, 0, "x".to_string())])]).unwrap();
    assert!(!decode("n.xlsx", &new).contains("defaultFont"));
}

#[test]
fn a_font_that_is_not_a_bounded_font_refuses_the_save() {
    let long = "F".repeat(65);
    for font in [
        format!(r#","defaultFont":{{"name":"{long}","size":11}}"#),
        r#","defaultFont":{"name":"Arial","size":409.5}"#.to_string(),
        r#","defaultFont":{"name":"Arial","size":0}"#.to_string(),
        r#","defaultFont":{"name":"Arial","size":-5}"#.to_string(),
        r#","defaultFont":{"name":"Arial","size":1e999}"#.to_string(),
        r#","defaultFont":{"name":"Arial","size":"12"}"#.to_string(),
        r#","defaultFont":{"name":"Evil;Font{}","size":11}"#.to_string(),
        r#","defaultFont":{"name":"","size":11}"#.to_string(),
        r#","defaultFont":{"name":"Arial"}"#.to_string(),
        r#","defaultFont":{"name":"Arial","size":11,"digit":9}"#.to_string(),
        r#","defaultFont":"Arial""#.to_string(),
        r#","defaultFont":[]"#.to_string(),
        r#","defaultFont":12"#.to_string(),
    ] {
        assert!(save("xlsx", &font).is_err(), "{font}");
    }
    // The bounds themselves are fine.
    let longest = "F".repeat(64);
    let payload = format!(r#","defaultFont":{{"name":"{longest}","size":409}}"#);
    let bytes = save("xlsx", &payload).unwrap();
    assert!(first_font(&bytes).contains(&format!("<name val=\"{longest}\"/>")));
}

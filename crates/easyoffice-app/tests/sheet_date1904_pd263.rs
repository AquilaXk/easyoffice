//! PD-263: a workbook in the 1904 date system keeps it through open and save, and its dates show and export in that system.

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{CellStyle, OoxmlCodec, SheetStyling, ZipArchiveCodec, ZipSecurityPolicy};

/// `date1904="1"`, number format 14 and the value 43000 (2021-09-23 in the 1904 system, 2017-09-22 in the 1900 one).
fn workbook_1904() -> Vec<u8> {
    let mut styling = SheetStyling {
        date1904: true,
        ..SheetStyling::default()
    };
    styling.cells.insert(
        (0, 0),
        CellStyle {
            num_fmt: 14,
            ..CellStyle::default()
        },
    );
    OoxmlCodec::pack_xlsx_multi_styled(
        &[("S".to_string(), vec![(0, 0, "43000".to_string())])],
        &[],
        &[styling],
    )
    .unwrap()
}

fn field(json: &str, key: &str) -> String {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker).unwrap() + marker.len();
    json[start..json[start..].find('"').unwrap() + start].to_string()
}

fn decode(bytes: &[u8]) -> String {
    let b64 = EasyOfficeIpcRouter::encode_base64(bytes);
    EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &format!(r#"{{"action":"decode","fileName":"d.xlsx","sourceBase64":"{b64}"}}"#),
    )
    .unwrap()
}

fn convert(decoded: &str, target: &str) -> Vec<u8> {
    let save = decoded.replacen(
        r#""action":"decode""#,
        &format!(r#""targetFormat":"{target}""#),
        1,
    );
    let saved = EasyOfficeIpcRouter::dispatch("convert_document_format", &save).unwrap();
    EasyOfficeIpcRouter::decode_base64(&field(&saved, "convertedBase64")).unwrap()
}

#[test]
fn open_and_save_keep_date1904() {
    let decoded = decode(&workbook_1904());
    assert!(decoded.contains("\"date1904\":true"), "{decoded}");
    let saved = convert(&decoded, "xlsx");
    let entries = ZipArchiveCodec::unpack(&saved, &ZipSecurityPolicy::default()).unwrap();
    let book = entries
        .iter()
        .find(|e| e.path == "xl/workbook.xml")
        .unwrap();
    assert!(String::from_utf8_lossy(&book.data).contains("date1904=\"1\""));
}

#[test]
fn a_1900_workbook_has_no_flag() {
    let plain = OoxmlCodec::pack_xlsx_multi_styled(
        &[("S".to_string(), vec![(0, 0, "43000".to_string())])],
        &[],
        &[SheetStyling::default()],
    )
    .unwrap();
    assert!(!decode(&plain).contains("date1904"));
}

#[test]
fn exports_show_the_date_of_the_workbooks_own_system() {
    let decoded = decode(&workbook_1904());
    let csv = String::from_utf8(convert(&decoded, "csv")).unwrap();
    assert!(csv.contains("2021"), "{csv}");
    assert!(!csv.contains("2017"), "{csv}");
}

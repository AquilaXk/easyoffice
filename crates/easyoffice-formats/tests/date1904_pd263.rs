//! PD-263: the 1904 date system (XLSX `workbookPr date1904`, XLS `DATEMODE`, ODS `table:null-date`) is read, kept on the first
//! sheet's styling and written again, so a save does not move every date by 1,462 days.

use easyoffice_formats::{
    LegacyOle2Codec, LegacyXlsPayload, OdfCodec, OoxmlCodec, SheetStyling, ZipArchiveCodec,
    ZipSecurityPolicy,
};

fn cells() -> Vec<(u32, u32, String)> {
    vec![(0, 0, "43000".to_string())]
}

fn styling(date1904: bool) -> SheetStyling {
    SheetStyling {
        date1904,
        ..SheetStyling::default()
    }
}

fn part(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let entry = entries.iter().find(|e| e.path == path).unwrap();
    String::from_utf8(entry.data.clone()).unwrap()
}

#[test]
fn xlsx_keeps_the_date_system() {
    for flag in [true, false] {
        let xlsx = OoxmlCodec::pack_xlsx_multi_styled(
            &[("S".to_string(), cells())],
            &[],
            &[styling(flag)],
        )
        .unwrap();
        let book = part(&xlsx, "xl/workbook.xml");
        assert_eq!(book.contains("date1904=\"1\""), flag, "{book}");
        let (_, back) = OoxmlCodec::unpack_xlsx_with_styling(&xlsx).unwrap();
        assert_eq!(back[0].1.date1904, flag);
    }
}

#[test]
fn xlsx_reads_the_flag_of_a_file_written_elsewhere() {
    let xlsx =
        OoxmlCodec::pack_xlsx_multi_styled(&[("S".to_string(), cells())], &[], &[styling(false)])
            .unwrap();
    for (attr, want) in [
        ("date1904=\"1\"", true),
        ("date1904=\"true\"", true),
        ("date1904=\"0\"", false),
    ] {
        let patched = patch(
            &xlsx,
            "xl/workbook.xml",
            "<sheets>",
            &format!("<workbookPr {attr}/><sheets>"),
        );
        let (_, back) = OoxmlCodec::unpack_xlsx_with_styling(&patched).unwrap();
        assert_eq!(back[0].1.date1904, want, "{attr}");
    }
}

#[test]
fn xlsx_iso_date_cells_follow_the_date_system() {
    let xlsx = OoxmlCodec::pack_xlsx_multi_styled(
        &[("S".to_string(), vec![(0, 0, "x".to_string())])],
        &[],
        &[styling(true)],
    )
    .unwrap();
    let sheet = part(&xlsx, "xl/worksheets/sheet1.xml");
    let with_date = sheet.replace(
        "</sheetData>",
        "<row r=\"2\"><c r=\"A2\" t=\"d\"><v>2026-11-10</v></c></row></sheetData>",
    );
    let patched = patch(&xlsx, "xl/worksheets/sheet1.xml", &sheet, &with_date);
    let (sheets, _) = OoxmlCodec::unpack_xlsx_with_styling(&patched).unwrap();
    assert!(
        sheets[0].1.contains(&(1, 0, "44874".to_string())),
        "{:?}",
        sheets[0].1
    );
}

#[test]
fn xls_keeps_the_date_system() {
    for flag in [true, false] {
        let xls = LegacyOle2Codec::pack_xls_multi_values_styled(
            &[LegacyXlsPayload {
                sheet_name: "S".to_string(),
                cells: cells(),
            }],
            &[],
            &[styling(flag)],
        )
        .unwrap();
        let (_, back) = LegacyOle2Codec::unpack_xls_with_styling(&xls).unwrap();
        assert_eq!(back[0].1.date1904, flag);
    }
}

#[test]
fn ods_keeps_the_date_system() {
    for flag in [true, false] {
        let ods = OdfCodec::pack_ods_multi_styled(&[("S".to_string(), cells())], &[styling(flag)])
            .unwrap();
        let content = part(&ods, "content.xml");
        assert_eq!(content.contains("table:null-date"), flag, "{content}");
        let (_, back) = OdfCodec::unpack_ods_with_styling(&ods).unwrap();
        assert_eq!(back[0].1.date1904, flag);
    }
    // A null date other than the two systems is not a date system this app counts from.
    let ods =
        OdfCodec::pack_ods_multi_styled(&[("S".to_string(), cells())], &[styling(true)]).unwrap();
    let content = part(&ods, "content.xml");
    let odd = content.replace("1904-01-01", "1950-01-01");
    let patched = patch(&ods, "content.xml", &content, &odd);
    let (_, back) = OdfCodec::unpack_ods_with_styling(&patched).unwrap();
    assert!(!back[0].1.date1904);
}

/// The package with `find` replaced by `replace` in one part.
fn patch(bytes: &[u8], path: &str, find: &str, replace: &str) -> Vec<u8> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let rebuilt: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            if e.path == path {
                let text = String::from_utf8(e.data)
                    .unwrap()
                    .replacen(find, replace, 1);
                (e.path, text.into_bytes())
            } else {
                (e.path, e.data)
            }
        })
        .collect();
    ZipArchiveCodec::pack_compressed(&rebuilt).unwrap()
}

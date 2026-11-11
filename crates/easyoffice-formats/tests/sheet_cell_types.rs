//! Sheet writers keep text that looks like a number as text (PD-172). `03045` and a 20-digit ID
//! typed as text must not become numbers (a number loses the leading zero and the digits beyond the
//! 15 an IEEE double keeps), while real numbers stay numbers.

use easyoffice_formats::{
    LegacyOle2Codec, LegacyXlsPayload, OdfCodec, Ole2ContainerCodec, OoxmlCodec, ZipArchiveCodec,
    ZipSecurityPolicy,
};

const CASES: [(&str, bool); 14] = [
    ("03045", false),
    ("0101234567", false),
    ("12345678901234567890", false),
    ("1234567890123456", false),
    ("+821012345678", false),
    ("00.5", false),
    ("-007", false),
    ("42", true),
    ("-0.5", true),
    ("0", true),
    ("0.25", true),
    ("123456789012345", true),
    ("1.5e3", true),
    ("12.50", true),
];

fn cells() -> Vec<(u32, u32, String)> {
    CASES
        .iter()
        .enumerate()
        .map(|(row, (text, _))| (row as u32, 0, (*text).to_string()))
        .collect()
}

fn part(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let entry = entries.iter().find(|e| e.path == path).unwrap();
    String::from_utf8(entry.data.clone()).unwrap()
}

#[test]
fn xlsx_numbers_are_numeric_cells_and_numeric_looking_text_is_a_shared_string() {
    let bytes = OoxmlCodec::pack_xlsx_multi(&[("S".to_string(), cells())]).unwrap();
    let sheet = part(&bytes, "xl/worksheets/sheet1.xml");
    let shared = part(&bytes, "xl/sharedStrings.xml");
    for (row, (text, numeric)) in CASES.iter().enumerate() {
        let r = row + 1;
        if *numeric {
            assert!(
                sheet.contains(&format!("<c r=\"A{r}\" t=\"n\"><v>{text}</v></c>")),
                "{text}: {sheet}"
            );
        } else {
            assert!(
                sheet.contains(&format!("<c r=\"A{r}\" t=\"s\"><v>")),
                "{text}: {sheet}"
            );
            assert!(
                shared.contains(&format!("<t>{text}</t>")),
                "{text}: {shared}"
            );
        }
    }
    let back = OoxmlCodec::unpack_xlsx_multi(&bytes).unwrap();
    assert_eq!(back[0].1, cells());
}

#[test]
fn ods_numbers_are_float_cells_and_numeric_looking_text_is_a_string_cell() {
    let bytes = OdfCodec::pack_ods_multi_styled_with_cache(&[("S".to_string(), cells())], &[], &[])
        .unwrap();
    let content = part(&bytes, "content.xml");
    for (text, numeric) in CASES {
        let float = format!("office:value-type=\"float\" office:value=\"{text}\"");
        let string = format!("office:value-type=\"string\"><text:p>{text}</text:p>");
        assert_eq!(content.contains(&float), numeric, "{text}: {content}");
        assert_eq!(content.contains(&string), !numeric, "{text}: {content}");
    }
    let back = OdfCodec::unpack_ods_multi(&bytes).unwrap();
    assert_eq!(back[0].1, cells());
}

#[test]
fn xls_numbers_are_number_records_and_numeric_looking_text_is_a_label() {
    let xls = LegacyOle2Codec::pack_xls(&LegacyXlsPayload {
        sheet_name: "S".to_string(),
        cells: cells(),
    })
    .unwrap();
    let data = Ole2ContainerCodec::unpack(&xls)
        .unwrap()
        .into_iter()
        .find(|s| s.name == "Workbook")
        .unwrap()
        .data;
    let (mut numbers, mut labels) = (0, 0);
    let mut at = 0;
    while at + 4 <= data.len() {
        match u16::from_le_bytes([data[at], data[at + 1]]) {
            0x0203 => numbers += 1,
            0x00FD => labels += 1,
            _ => {}
        }
        at += 4 + usize::from(u16::from_le_bytes([data[at + 2], data[at + 3]]));
    }
    assert_eq!(numbers, CASES.iter().filter(|(_, n)| *n).count());
    assert_eq!(labels, CASES.iter().filter(|(_, n)| !*n).count());
    let back = LegacyOle2Codec::unpack_xls(&xls).unwrap();
    for (row, (text, numeric)) in CASES.iter().enumerate() {
        let cell = back.cells.iter().find(|c| c.0 == row as u32).unwrap();
        if *numeric {
            let (a, b) = (cell.2.parse::<f64>().unwrap(), text.parse::<f64>().unwrap());
            assert_eq!(a, b, "{text}");
        } else {
            assert_eq!(cell.2, *text);
        }
    }
}

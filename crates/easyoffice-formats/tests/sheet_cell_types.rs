//! Sheet writers keep text that looks like a number as text (PD-178). `03045` and a 20-digit ID
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

#[test]
fn explicit_cell_kinds_round_trip_across_xlsx_ods_and_xls_and_stay_sparse() {
    use easyoffice_formats::{CellValueKind, SheetStyling, MAX_STYLED_CELLS};

    // Row 0: '123 (explicit Text)
    // Row 1: '=1+1 (explicit Text, NOT a formula!)
    // Row 2: 'TRUE (explicit Text, NOT a boolean!)
    // Row 3: '#N/A (explicit Text, NOT an error!)
    // Row 4: TRUE (inferred Boolean -> 0 entries in sparse delta)
    // Row 5: FALSE (inferred Boolean -> 0 entries in sparse delta)
    // Row 6: #DIV/0! (inferred Error -> 0 entries in sparse delta)
    // Row 7: #N/A (inferred Error -> 0 entries in sparse delta)
    // Row 8: plain 42 (inferred Number -> 0 entries in sparse delta)
    let raw_cells = vec![
        (0, 0, "123".to_string()),
        (1, 0, "=1+1".to_string()),
        (2, 0, "TRUE".to_string()),
        (3, 0, "#N/A".to_string()),
        (4, 0, "TRUE".to_string()),
        (5, 0, "FALSE".to_string()),
        (6, 0, "#DIV/0!".to_string()),
        (7, 0, "#N/A".to_string()),
        (8, 0, "42".to_string()),
    ];
    let mut styling = SheetStyling::default();
    styling.set_cell_kind(0, 0, "123", CellValueKind::Text);
    styling.set_cell_kind(1, 0, "=1+1", CellValueKind::Text);
    styling.set_cell_kind(2, 0, "TRUE", CellValueKind::Text);
    styling.set_cell_kind(3, 0, "#N/A", CellValueKind::Text);
    styling.set_cell_kind(4, 0, "TRUE", CellValueKind::Boolean);
    styling.set_cell_kind(5, 0, "FALSE", CellValueKind::Boolean);
    styling.set_cell_kind(6, 0, "#DIV/0!", CellValueKind::Error);
    styling.set_cell_kind(7, 0, "#N/A", CellValueKind::Error);
    styling.set_cell_kind(8, 0, "42", CellValueKind::Number);
    // Only the 4 cells whose explicit kind differs from `infer(raw)` occupy `cell_types`.
    assert_eq!(styling.cell_types.len(), 4);

    // 1. XLSX round-trip
    let xlsx = OoxmlCodec::pack_xlsx_multi_styled(
        &[("S".to_string(), raw_cells.clone())],
        &[],
        std::slice::from_ref(&styling),
    )
    .unwrap();
    let sheet_xml = part(&xlsx, "xl/worksheets/sheet1.xml");
    assert!(sheet_xml.contains("<c r=\"A1\" t=\"s\">"), "{sheet_xml}");
    assert!(sheet_xml.contains("<c r=\"A2\" t=\"s\">"), "{sheet_xml}");
    assert!(sheet_xml.contains("<c r=\"A3\" t=\"s\">"), "{sheet_xml}");
    assert!(sheet_xml.contains("<c r=\"A4\" t=\"s\">"), "{sheet_xml}");
    assert!(
        sheet_xml.contains("<c r=\"A5\" t=\"b\"><v>1</v></c>"),
        "{sheet_xml}"
    );
    assert!(
        sheet_xml.contains("<c r=\"A6\" t=\"b\"><v>0</v></c>"),
        "{sheet_xml}"
    );
    assert!(
        sheet_xml.contains("<c r=\"A7\" t=\"e\"><v>#DIV/0!</v></c>"),
        "{sheet_xml}"
    );
    assert!(
        sheet_xml.contains("<c r=\"A8\" t=\"e\"><v>#N/A</v></c>"),
        "{sheet_xml}"
    );
    let (xlsx_sheets, xlsx_stylings) = OoxmlCodec::unpack_xlsx_with_styling(&xlsx).unwrap();
    assert_eq!(xlsx_sheets[0].1, raw_cells);
    for r in 0..4u32 {
        assert_eq!(
            xlsx_stylings[0].1.cell_kind(r, 0, &raw_cells[r as usize].2),
            CellValueKind::Text
        );
    }
    assert_eq!(
        xlsx_stylings[0].1.cell_kind(4, 0, "TRUE"),
        CellValueKind::Boolean
    );
    assert_eq!(
        xlsx_stylings[0].1.cell_kind(6, 0, "#DIV/0!"),
        CellValueKind::Error
    );
    assert_eq!(
        xlsx_stylings[0].1.cell_kind(8, 0, "42"),
        CellValueKind::Number
    );

    // 2. ODS round-trip
    let ods = OdfCodec::pack_ods_multi_styled(
        &[("S".to_string(), raw_cells.clone())],
        std::slice::from_ref(&styling),
    )
    .unwrap();
    let ods_xml = part(&ods, "content.xml");
    assert!(
        ods_xml.contains("office:value-type=\"boolean\" office:boolean-value=\"true\""),
        "{ods_xml}"
    );
    assert!(
        ods_xml.contains("calcext:value-type=\"error\"><text:p>#DIV/0!</text:p>"),
        "{ods_xml}"
    );
    let (ods_sheets, ods_stylings) = OdfCodec::unpack_ods_with_styling(&ods).unwrap();
    assert_eq!(ods_sheets[0].1, raw_cells);
    for r in 0..4u32 {
        assert_eq!(
            ods_stylings[0].1.cell_kind(r, 0, &raw_cells[r as usize].2),
            CellValueKind::Text
        );
    }
    assert_eq!(
        ods_stylings[0].1.cell_kind(4, 0, "TRUE"),
        CellValueKind::Boolean
    );
    assert_eq!(
        ods_stylings[0].1.cell_kind(6, 0, "#DIV/0!"),
        CellValueKind::Error
    );

    // 3. BIFF8 XLS round-trip (including MS-XLS §2.4.24 BoolErr byte order)
    let xls = LegacyOle2Codec::pack_xls_multi_values_styled(
        &[LegacyXlsPayload {
            sheet_name: "S".to_string(),
            cells: raw_cells.clone(),
        }],
        &[],
        std::slice::from_ref(&styling),
    )
    .unwrap();
    let (xls_sheets, xls_stylings) = LegacyOle2Codec::unpack_xls_with_styling(&xls).unwrap();
    assert_eq!(xls_sheets[0].1, raw_cells);
    for r in 0..4u32 {
        assert_eq!(
            xls_stylings[0].1.cell_kind(r, 0, &raw_cells[r as usize].2),
            CellValueKind::Text
        );
    }
    assert_eq!(
        xls_stylings[0].1.cell_kind(4, 0, "TRUE"),
        CellValueKind::Boolean
    );
    assert_eq!(
        xls_stylings[0].1.cell_kind(5, 0, "FALSE"),
        CellValueKind::Boolean
    );
    assert_eq!(
        xls_stylings[0].1.cell_kind(6, 0, "#DIV/0!"),
        CellValueKind::Error
    );
    assert_eq!(
        xls_stylings[0].1.cell_kind(7, 0, "#N/A"),
        CellValueKind::Error
    );

    // 4. `MAX_STYLED_CELLS` cap on `cell_types`
    let mut capped = SheetStyling::default();
    for i in 0..(MAX_STYLED_CELLS as u32 + 16) {
        capped.set_cell_kind(i, 0, "1", CellValueKind::Text);
    }
    assert_eq!(capped.cell_types.len(), MAX_STYLED_CELLS);
}

#[test]
fn modern_blocked_and_connect_errors_roundtrip_across_xlsx_ods_xls_and_xlfn_formulas() {
    use easyoffice_formats::{CellValueKind, SheetStyling};

    let raw_cells = vec![
        (0, 0, "#CONNECT!".to_string()),
        (1, 0, "#BLOCKED!".to_string()),
        (2, 0, "#BLOCKED!".to_string()),
        (
            3,
            0,
            "=IMAGE(\"https://example.invalid/a.png\")".to_string(),
        ),
        (4, 0, "=STOCKHISTORY(\"MSFT\",DATE(2026,1,1))".to_string()),
    ];
    let mut styling = SheetStyling::default();
    styling.set_cell_kind(0, 0, "#CONNECT!", CellValueKind::Error);
    styling.set_cell_kind(1, 0, "#BLOCKED!", CellValueKind::Error);
    styling.set_cell_kind(2, 0, "#BLOCKED!", CellValueKind::Text);
    assert_eq!(styling.cell_types.len(), 1);

    let xlsx = OoxmlCodec::pack_xlsx_multi_styled(
        &[("S".to_string(), raw_cells.clone())],
        &[],
        std::slice::from_ref(&styling),
    )
    .unwrap();
    let sheet_xml = part(&xlsx, "xl/worksheets/sheet1.xml");
    assert!(
        sheet_xml.contains("<c r=\"A1\" t=\"e\"><v>#CONNECT!</v></c>"),
        "{sheet_xml}"
    );
    assert!(
        sheet_xml.contains("<c r=\"A2\" t=\"e\"><v>#BLOCKED!</v></c>"),
        "{sheet_xml}"
    );
    assert!(sheet_xml.contains("<c r=\"A3\" t=\"s\">"), "{sheet_xml}");
    assert!(sheet_xml.contains("_xlfn.IMAGE("), "{sheet_xml}");
    assert!(sheet_xml.contains("_xlfn.STOCKHISTORY("), "{sheet_xml}");

    let (xlsx_sheets, xlsx_stylings) = OoxmlCodec::unpack_xlsx_with_styling(&xlsx).unwrap();
    assert_eq!(xlsx_sheets[0].1, raw_cells);
    assert_eq!(
        xlsx_stylings[0].1.cell_kind(0, 0, "#CONNECT!"),
        CellValueKind::Error
    );
    assert_eq!(
        xlsx_stylings[0].1.cell_kind(1, 0, "#BLOCKED!"),
        CellValueKind::Error
    );
    assert_eq!(
        xlsx_stylings[0].1.cell_kind(2, 0, "#BLOCKED!"),
        CellValueKind::Text
    );

    let ods = OdfCodec::pack_ods_multi_styled(
        &[("S".to_string(), raw_cells[..3].to_vec())],
        std::slice::from_ref(&styling),
    )
    .unwrap();
    let ods_xml = part(&ods, "content.xml");
    assert!(
        ods_xml.contains("calcext:value-type=\"error\"><text:p>#CONNECT!</text:p>"),
        "{ods_xml}"
    );
    assert!(
        ods_xml.contains("calcext:value-type=\"error\"><text:p>#BLOCKED!</text:p>"),
        "{ods_xml}"
    );
    let (ods_sheets, ods_stylings) = OdfCodec::unpack_ods_with_styling(&ods).unwrap();
    assert_eq!(ods_sheets[0].1, raw_cells[..3]);
    assert_eq!(
        ods_stylings[0].1.cell_kind(0, 0, "#CONNECT!"),
        CellValueKind::Error
    );
    assert_eq!(
        ods_stylings[0].1.cell_kind(2, 0, "#BLOCKED!"),
        CellValueKind::Text
    );

    let xls = LegacyOle2Codec::pack_xls_multi_values_styled(
        &[LegacyXlsPayload {
            sheet_name: "S".to_string(),
            cells: raw_cells[..3].to_vec(),
        }],
        &[],
        std::slice::from_ref(&styling),
    )
    .unwrap();
    let (xls_sheets, xls_stylings) = LegacyOle2Codec::unpack_xls_with_styling(&xls).unwrap();
    assert_eq!(xls_sheets[0].1, raw_cells[..3]);
    assert_eq!(
        xls_stylings[0].1.cell_kind(0, 0, "#CONNECT!"),
        CellValueKind::Error
    );
    assert_eq!(
        xls_stylings[0].1.cell_kind(1, 0, "#BLOCKED!"),
        CellValueKind::Error
    );
    // Modern error literals without MS-XLS §2.5.10 BErr codes travel via LABELSST in BIFF8
    // (`biff8_xls_reader.rs`) and read back as inferred `Error`, matching `#SPILL!` and `#CALC!`.
    assert_eq!(
        xls_stylings[0].1.cell_kind(2, 0, "#BLOCKED!"),
        CellValueKind::Error
    );
}

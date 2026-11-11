//! Malicious spreadsheets whose decoded output dwarfs the input (PD-175): every fan-out the
//! streaming readers allow must hit the document-wide decoded-byte budget instead of memory.

use easyoffice_formats::{EasyOfficeError, OoxmlCodec, ZipArchiveCodec};

const MIB: usize = 1024 * 1024;

fn xlsx(workbook: &str, shared: Option<&str>, sheets: &[(String, String)]) -> Vec<u8> {
    let mut entries = vec![("xl/workbook.xml".to_string(), workbook.as_bytes().to_vec())];
    if let Some(shared) = shared {
        entries.push((
            "xl/sharedStrings.xml".to_string(),
            shared.as_bytes().to_vec(),
        ));
    }
    for (path, xml) in sheets {
        entries.push((path.clone(), xml.as_bytes().to_vec()));
    }
    ZipArchiveCodec::pack(&entries).expect("pack xlsx fixture")
}

fn one_sheet_workbook() -> &'static str {
    r#"<workbook><sheets><sheet name="S" sheetId="1"/></sheets></workbook>"#
}

fn sheet_of(cells: &str) -> (String, String) {
    (
        "xl/worksheets/sheet1.xml".to_string(),
        format!("<worksheet><sheetData><row r=\"1\">{cells}</row></sheetData></worksheet>"),
    )
}

fn assert_budget_error<T>(result: Result<T, EasyOfficeError>, what: &str) {
    match result {
        Err(EasyOfficeError::ResourceBudgetExceeded(_)) => {}
        Err(other) => panic!("{what}: expected ResourceBudgetExceeded, got {other:?}"),
        Ok(_) => panic!("{what}: expected ResourceBudgetExceeded, the input was accepted"),
    }
}

#[test]
fn one_shared_string_referenced_by_many_cells_hits_the_decoded_budget() {
    // 300 cells x 1 MiB = 300 MiB of copies out of a 1 MiB string table and a 10 KB sheet.
    let shared = format!("<sst><si><t>{}</t></si></sst>", "A".repeat(MIB));
    let cells: String = (0..300)
        .map(|i| format!("<c r=\"A{}\" t=\"s\"><v>0</v></c>", i + 1))
        .collect();
    let bytes = xlsx(one_sheet_workbook(), Some(&shared), &[sheet_of(&cells)]);
    assert_budget_error(
        OoxmlCodec::unpack_xlsx_multi(&bytes),
        "shared string fan-out",
    );
}

#[test]
fn shared_formula_slaves_copying_a_huge_master_hit_the_decoded_budget() {
    let master = format!("\"{}\"", "A".repeat(MIB));
    let mut cells =
        format!("<c r=\"A1\"><f t=\"shared\" ref=\"A1:A400\" si=\"0\">{master}</f></c>");
    for i in 2..=300 {
        cells.push_str(&format!("<c r=\"A{i}\"><f t=\"shared\" si=\"0\"/></c>"));
    }
    let bytes = xlsx(one_sheet_workbook(), None, &[sheet_of(&cells)]);
    assert_budget_error(
        OoxmlCodec::unpack_xlsx_multi(&bytes),
        "shared formula fan-out",
    );
}

#[test]
fn shared_formula_slaves_whose_references_grow_are_charged_for_the_shifted_text() {
    // "A1+" shifted to the far corner becomes "XFD1048576+": 3.7 times the master. 400 slaves of a
    // 256 KiB master are 100 MiB as stored on the master's length and 376 MiB as shifted.
    let master = "A1+".repeat(86_000);
    let mut cells =
        format!("<c r=\"A1\"><f t=\"shared\" ref=\"A1:XFD1048576\" si=\"0\">{master}0</f></c>");
    for k in 0..400 {
        cells.push_str(&format!(
            "<c r=\"XFD{}\"><f t=\"shared\" si=\"0\"/></c>",
            1_048_576 - k
        ));
    }
    let bytes = xlsx(one_sheet_workbook(), None, &[sheet_of(&cells)]);
    assert_budget_error(
        OoxmlCodec::unpack_xlsx_multi(&bytes),
        "growing shared formula",
    );
}

#[test]
fn the_budget_spans_all_sheets_of_the_workbook() {
    // Two sheets of 150 x 1 MiB each: neither passes 256 MiB alone, together they do.
    let shared = format!("<sst><si><t>{}</t></si></sst>", "A".repeat(MIB));
    let cells: String = (0..150)
        .map(|i| format!("<c r=\"A{}\" t=\"s\"><v>0</v></c>", i + 1))
        .collect();
    let workbook = r#"<workbook><sheets><sheet name="One" sheetId="1"/><sheet name="Two" sheetId="2"/></sheets></workbook>"#;
    let sheet = |n: usize| {
        (
            format!("xl/worksheets/sheet{n}.xml"),
            format!("<worksheet><sheetData><row r=\"1\">{cells}</row></sheetData></worksheet>"),
        )
    };
    let bytes = xlsx(workbook, Some(&shared), &[sheet(1), sheet(2)]);
    assert_budget_error(OoxmlCodec::unpack_xlsx_multi(&bytes), "two-sheet fan-out");
}

#[test]
fn several_sheet_entries_naming_one_worksheet_part_are_refused() {
    let workbook = r#"<workbook xmlns:r="r"><sheets><sheet name="A" sheetId="1" r:id="rId1"/><sheet name="B" sheetId="2" r:id="rId1"/></sheets></workbook>"#;
    let rels = r#"<Relationships><Relationship Id="rId1" Type="t" Target="worksheets/sheet1.xml"/></Relationships>"#;
    let mut entries = vec![
        ("xl/workbook.xml".to_string(), workbook.as_bytes().to_vec()),
        (
            "xl/_rels/workbook.xml.rels".to_string(),
            rels.as_bytes().to_vec(),
        ),
    ];
    let (path, xml) = sheet_of("<c r=\"A1\"><v>1</v></c>");
    entries.push((path, xml.into_bytes()));
    let bytes = ZipArchiveCodec::pack(&entries).unwrap();
    assert!(
        OoxmlCodec::unpack_xlsx_multi(&bytes).is_err(),
        "a worksheet part must not be scanned once per <sheet> entry"
    );
}

#[test]
fn a_sheet_list_beyond_the_sheet_limit_is_refused() {
    let sheets: String = (1..=2_000)
        .map(|i| format!("<sheet name=\"S{i}\" sheetId=\"{i}\"/>"))
        .collect();
    let workbook = format!("<workbook><sheets>{sheets}</sheets></workbook>");
    let bytes = xlsx(&workbook, None, &[sheet_of("<c r=\"A1\"><v>1</v></c>")]);
    assert_budget_error(OoxmlCodec::unpack_xlsx_multi(&bytes), "2,000 sheet entries");
}

#[test]
fn a_large_ordinary_sheet_still_opens() {
    let strings: String = (0..1_000)
        .map(|i| format!("<si><t>text {i}</t></si>"))
        .collect();
    let shared = format!("<sst>{strings}</sst>");
    let rows: String = (0..100_000)
        .map(|i| {
            let (row, col) = (i / 10 + 1, i % 10);
            let letter = (b'A' + col as u8) as char;
            if col < 5 {
                format!("<c r=\"{letter}{row}\"><v>{i}</v></c>")
            } else {
                format!("<c r=\"{letter}{row}\" t=\"s\"><v>{}</v></c>", i % 1_000)
            }
        })
        .collect();
    let sheet = (
        "xl/worksheets/sheet1.xml".to_string(),
        format!("<worksheet><sheetData><row r=\"1\">{rows}</row></sheetData></worksheet>"),
    );
    let bytes = xlsx(one_sheet_workbook(), Some(&shared), &[sheet]);
    let sheets = OoxmlCodec::unpack_xlsx_multi(&bytes).expect("ordinary 100k-cell sheet opens");
    assert_eq!(sheets[0].1.len(), 100_000);
}

#[test]
fn a_workbook_the_reader_would_refuse_is_not_written() {
    let sheets: Vec<easyoffice_formats::OdsUnpacked> = (0..=OoxmlCodec::MAX_XLSX_SHEETS)
        .map(|i| (format!("S{i}"), vec![(0, 0, "x".to_string())]))
        .collect();
    assert_budget_error(
        OoxmlCodec::pack_xlsx_multi(&sheets),
        "pack over the sheet limit",
    );
    let at_limit = &sheets[..OoxmlCodec::MAX_XLSX_SHEETS];
    let bytes = OoxmlCodec::pack_xlsx_multi(at_limit).expect("the limit itself is written");
    assert_eq!(
        OoxmlCodec::unpack_xlsx_multi(&bytes).unwrap().len(),
        OoxmlCodec::MAX_XLSX_SHEETS
    );
}

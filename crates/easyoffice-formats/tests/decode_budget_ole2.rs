//! Legacy OLE2 documents (BIFF8 workbook, Word, Hangul 5.0) whose records point at the same bytes
//! over and over must hit the decoded-byte budget, not memory (PD-175).

use easyoffice_formats::{EasyOfficeError, LegacyOle2Codec, LegacyXlsPayload};

fn assert_budget_error<T>(result: Result<T, EasyOfficeError>, what: &str) {
    match result {
        Err(EasyOfficeError::ResourceBudgetExceeded(_)) => {}
        Err(other) => panic!("{what}: expected ResourceBudgetExceeded, got {other:?}"),
        Ok(_) => panic!("{what}: expected ResourceBudgetExceeded, the input was accepted"),
    }
}

#[test]
fn many_label_cells_naming_one_big_shared_string_hit_the_decoded_budget() {
    // One 8,000-character string and 40,000 14-byte LABELSST records: 320 MB of copies out of a
    // workbook stream of about 570 KB.
    let text = "A".repeat(8_000);
    let cells = (0..40_000u32).map(|row| (row, 0, text.clone())).collect();
    let packed = LegacyOle2Codec::pack_xls_multi(&[LegacyXlsPayload {
        sheet_name: "S".to_string(),
        cells,
    }])
    .expect("pack xls fixture");
    assert_budget_error(
        LegacyOle2Codec::unpack_xls_multi(&packed),
        "LABELSST fan-out",
    );
}

#[test]
fn an_ordinary_sheet_of_text_cells_still_opens() {
    let cells = (0..20_000u32)
        .map(|row| (row, row % 8, format!("cell {row}")))
        .collect();
    let packed = LegacyOle2Codec::pack_xls_multi(&[LegacyXlsPayload {
        sheet_name: "S".to_string(),
        cells,
    }])
    .expect("pack xls fixture");
    let sheets = LegacyOle2Codec::unpack_xls_multi(&packed).expect("ordinary xls opens");
    assert_eq!(sheets[0].cells.len(), 20_000);
}

//! A spreadsheet whose repeated cells copy one big string many times must hit the decoded-byte
//! budget, not memory (PD-175). The ODS cell-count cap (200,000) alone allows 200,000 copies.

use easyoffice_formats::{EasyOfficeError, OdfCodec, ZipArchiveCodec};

const MIB: usize = 1024 * 1024;

fn ods(content: &str) -> Vec<u8> {
    ZipArchiveCodec::pack(&[
        (
            "mimetype".to_string(),
            b"application/vnd.oasis.opendocument.spreadsheet".to_vec(),
        ),
        ("content.xml".to_string(), content.as_bytes().to_vec()),
    ])
    .expect("pack ods fixture")
}

fn document(table_body: &str) -> String {
    format!(
        "<office:document-content><office:body><office:spreadsheet><table:table table:name=\"S\">{table_body}</table:table></office:spreadsheet></office:body></office:document-content>"
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
fn a_column_repeated_big_string_hits_the_decoded_budget() {
    // 300 repeated copies of a 1 MiB cell: 300 MiB out of a 1 MiB part.
    let cell = format!(
        "<table:table-cell office:value-type=\"string\" table:number-columns-repeated=\"300\"><text:p>{}</text:p></table:table-cell>",
        "A".repeat(MIB)
    );
    let bytes = ods(&document(&format!(
        "<table:table-row>{cell}</table:table-row>"
    )));
    assert_budget_error(OdfCodec::unpack_ods_multi(&bytes), "column repeat");
}

#[test]
fn a_row_repeated_big_string_hits_the_decoded_budget() {
    let cell = format!(
        "<table:table-cell office:value-type=\"string\"><text:p>{}</text:p></table:table-cell>",
        "A".repeat(MIB)
    );
    let row =
        format!("<table:table-row table:number-rows-repeated=\"300\">{cell}</table:table-row>");
    let bytes = ods(&document(&row));
    assert_budget_error(OdfCodec::unpack_ods_multi(&bytes), "row repeat");
}

#[test]
fn ordinary_repeats_still_open() {
    let row = "<table:table-row table:number-rows-repeated=\"1000\"><table:table-cell office:value-type=\"string\" table:number-columns-repeated=\"20\"><text:p>x</text:p></table:table-cell></table:table-row>";
    let bytes = ods(&document(row));
    let sheets = OdfCodec::unpack_ods_multi(&bytes).expect("20,000 small cells open");
    assert_eq!(sheets[0].1.len(), 20_000);
}

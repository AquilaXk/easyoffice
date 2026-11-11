//! ODS cells are read by their raw value, not by the text a number format displays (PD-172): a cell
//! shown as `1,234` holds 1234, a date shown as `11월 11일` holds `2026-11-11`, and a
//! self-closing `<text:p/>` is still a paragraph.

use easyoffice_formats::{OdfCodec, ZipArchiveCodec, ZipSecurityPolicy};

fn ods_with_table(rows: &str) -> Vec<u8> {
    let base = OdfCodec::pack_ods_multi_styled_with_cache(
        &[("S".to_string(), vec![(0, 0, "x".to_string())])],
        &[],
        &[],
    )
    .unwrap();
    let mut entries: Vec<(String, Vec<u8>)> =
        ZipArchiveCodec::unpack(&base, &ZipSecurityPolicy::default())
            .unwrap()
            .into_iter()
            .map(|e| (e.path, e.data))
            .collect();
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" office:version=\"1.2\"><office:body><office:spreadsheet><table:table table:name=\"S\">{rows}</table:table></office:spreadsheet></office:body></office:document-content>"
    );
    entries
        .iter_mut()
        .find(|(path, _)| path == "content.xml")
        .unwrap()
        .1 = content.into_bytes();
    ZipArchiveCodec::pack(&entries).unwrap()
}

fn read(rows: &str) -> Vec<(u32, u32, String)> {
    OdfCodec::unpack_ods_multi(&ods_with_table(rows))
        .unwrap()
        .remove(0)
        .1
}

#[test]
fn typed_cells_give_their_raw_value_not_the_displayed_text() {
    let cells = read(concat!(
        "<table:table-row>",
        "<table:table-cell office:value-type=\"float\" office:value=\"1234\"><text:p>1,234</text:p></table:table-cell>",
        "<table:table-cell office:value-type=\"percentage\" office:value=\"0.25\"><text:p>25%</text:p></table:table-cell>",
        "<table:table-cell office:value-type=\"date\" office:date-value=\"2026-11-11\"><text:p>11월 11일</text:p></table:table-cell>",
        "<table:table-cell office:value-type=\"boolean\" office:boolean-value=\"true\"><text:p>참</text:p></table:table-cell>",
        "<table:table-cell office:value-type=\"string\" office:string-value=\"raw\"><text:p>shown</text:p></table:table-cell>",
        "<table:table-cell office:value-type=\"string\"><text:p>plain</text:p></table:table-cell>",
        "</table:table-row>"
    ));
    let texts: Vec<&str> = cells.iter().map(|c| c.2.as_str()).collect();
    assert_eq!(
        texts,
        ["1234", "0.25", "2026-11-11", "TRUE", "raw", "plain"]
    );
}

#[test]
fn a_formula_still_wins_over_its_value() {
    let cells = read(
        "<table:table-row><table:table-cell table:formula=\"of:=[.A2]*2\" office:value-type=\"float\" office:value=\"6\"><text:p>6</text:p></table:table-cell></table:table-row>",
    );
    assert_eq!(cells, [(0, 0, "=A2*2".to_string())]);
}

#[test]
fn a_self_closing_paragraph_counts_as_a_paragraph() {
    let cells = read(concat!(
        "<table:table-row>",
        "<table:table-cell office:value-type=\"string\"><text:p>a</text:p><text:p/><text:p>b</text:p></table:table-cell>",
        "<table:table-cell office:value-type=\"string\"><text:p/><text:p>c</text:p></table:table-cell>",
        "<table:table-cell office:value-type=\"string\"><text:p/></table:table-cell>",
        "<table:table-cell><text:p/></table:table-cell>",
        "</table:table-row>"
    ));
    let texts: Vec<&str> = cells.iter().map(|c| c.2.as_str()).collect();
    assert_eq!(texts, ["a\n\nb", "\nc", "", ""]);
}

//! `UniversalFormatConverter::convert` keeps block order (tables between their paragraphs) and the
//! paragraph/cell text when a document is converted to another document format, and still
//! produces sheet content when a document is converted into a workbook.

use easyoffice_formats::{
    DecodedContent, DocumentFormat, RichBlock, RichDocument, UniversalFormatConverter,
};

const FIXTURE: &str = "eo-pages-lists-tables.pages";

fn fixture_bytes() -> Vec<u8> {
    let path = format!(
        "{}/tests/fixtures/pages/{FIXTURE}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(path).expect("fixture readable")
}

fn open_document(bytes: &[u8], hint: &str) -> RichDocument {
    let decoded = UniversalFormatConverter::decode_rich(bytes, Some(hint)).expect("decode");
    match decoded.content {
        DecodedContent::Document(doc) => doc,
        _ => panic!("{hint}: expected a document"),
    }
}

/// Non-empty paragraph text and tables, in document order.
fn outline(doc: &RichDocument) -> Vec<String> {
    doc.blocks
        .iter()
        .filter_map(|block| match block {
            RichBlock::Paragraph(p) => Some(p.plain_text()).filter(|text| !text.is_empty()),
            RichBlock::Table(table) => Some(format!(
                "[table {}x{}]",
                table.rows.len(),
                table.rows.first().map_or(0, Vec::len)
            )),
            RichBlock::Image(_) => Some("[image]".to_string()),
        })
        .collect()
}

#[test]
fn document_to_document_conversion_keeps_tables_between_their_paragraphs() {
    let bytes = fixture_bytes();
    let expected = outline(&open_document(&bytes, FIXTURE));
    assert!(expected.iter().filter(|b| b.starts_with("[table")).count() == 4);
    for target in [
        DocumentFormat::Docx,
        DocumentFormat::Hwpx,
        DocumentFormat::Odt,
    ] {
        let converted = UniversalFormatConverter::convert(&bytes, Some(FIXTURE), target)
            .unwrap_or_else(|e| panic!("convert to {target:?}: {e}"));
        let reread = open_document(&converted, &format!("out.{}", target.extension()));
        assert_eq!(outline(&reread), expected, "{target:?}");
    }
}

#[test]
fn document_to_markdown_places_each_table_after_its_heading() {
    let bytes = fixture_bytes();
    let md = UniversalFormatConverter::convert(&bytes, Some(FIXTURE), DocumentFormat::Markdown)
        .expect("convert to Markdown");
    let text = String::from_utf8(md).expect("utf-8");
    let heading = text.find("5. 표 1").expect("table 1 heading");
    let first_cell = text.find("구분").expect("table 1 header cell");
    let next_heading = text.find("6. 표 2").expect("table 2 heading");
    assert!(
        heading < first_cell && first_cell < next_heading,
        "table 1 sits under its heading"
    );
}

#[test]
fn document_to_workbook_conversion_still_produces_cells() {
    let bytes = fixture_bytes();
    let xlsx = UniversalFormatConverter::convert(&bytes, Some(FIXTURE), DocumentFormat::Xlsx)
        .expect("convert to XLSX");
    let (_, model) = UniversalFormatConverter::decode(&xlsx, Some("xlsx")).expect("reopen XLSX");
    assert!(!model.sheet_cells.is_empty(), "the workbook has cells");
}

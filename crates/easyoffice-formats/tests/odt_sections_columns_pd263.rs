//! PD-263: ODT writes the column setup per section (`text:section`), not once for the whole page, and reads it back per paragraph.

use easyoffice_formats::{
    DecodedContent, DocumentFormat, RichBlock, RichColumns, RichDocument, RichParagraph, RichRun,
    UniversalFormatConverter,
};

fn columns(count: u8, gap_twips: u16) -> Option<RichColumns> {
    Some(RichColumns {
        count,
        equal: true,
        gap_twips,
    })
}

fn para(text: &str, columns: Option<RichColumns>) -> RichBlock {
    RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun::plain(text)],
        columns,
        ..RichParagraph::default()
    })
}

fn read_columns(doc: &RichDocument) -> Vec<(String, Option<RichColumns>)> {
    doc.blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some((p.plain_text(), p.columns.filter(|c| c.count > 1))),
            _ => None,
        })
        .collect()
}

#[test]
fn each_section_keeps_its_own_column_count_and_gap() {
    let doc = RichDocument {
        blocks: vec![
            para("한 단", None),
            para("두 단 첫째", columns(2, 720)),
            para("두 단 둘째", columns(2, 720)),
            para("세 단", columns(3, 360)),
            para("다시 한 단", None),
        ],
        ..RichDocument::default()
    };
    let odt = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Odt, None).unwrap();
    let decoded = UniversalFormatConverter::decode_rich(&odt, Some("a.odt")).unwrap();
    let DecodedContent::Document(read) = decoded.content else {
        panic!("document");
    };
    assert_eq!(
        read_columns(&read),
        vec![
            ("한 단".to_string(), None),
            ("두 단 첫째".to_string(), columns(2, 720)),
            ("두 단 둘째".to_string(), columns(2, 720)),
            ("세 단".to_string(), columns(3, 360)),
            ("다시 한 단".to_string(), None),
        ]
    );
}

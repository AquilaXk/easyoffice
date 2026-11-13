//! Markdown table cells read inline syntax like a paragraph does (a cell holds text only, so the
//! formatting marks are dropped and the text stays), and cell text written by the Markdown writer
//! reads back unchanged.

use easyoffice_formats::{
    DecodedContent, DocumentFormat, RichBlock, RichDocument, UniversalFormatConverter,
};

fn read_tables(markdown: &str) -> Vec<Vec<Vec<String>>> {
    let decoded = UniversalFormatConverter::decode_rich(markdown.as_bytes(), Some("a.md")).unwrap();
    let DecodedContent::Document(doc) = decoded.content else {
        panic!("document expected");
    };
    doc.blocks
        .into_iter()
        .filter_map(|b| match b {
            RichBlock::Table(t) => Some(t.rows),
            _ => None,
        })
        .collect()
}

#[test]
fn inline_syntax_in_cells_is_resolved_to_its_text() {
    let md = "| **굵게** | `코드` | [링크](http://x) |\n| --- | --- | --- |\n| H<sub>2</sub>O | \\*별\\* | ~~삭제~~ 끝 |\n";
    let rows = &read_tables(md)[0];
    assert_eq!(rows[0], ["굵게", "코드", "링크"]);
    assert_eq!(rows[1], ["H2O", "*별*", "삭제 끝"]);
}

#[test]
fn pipeless_tables_resolve_cell_syntax_too() {
    let md = "이름 | 값\n--- | ---\n*기울임* | `x`\n";
    let rows = &read_tables(md)[0];
    assert_eq!(rows[1], ["기울임", "x"]);
}

#[test]
fn cell_text_with_markdown_characters_survives_a_markdown_round_trip() {
    let cells = [
        "2*3*4", "a_b_c", "[x](y)", "a\\|b", "a|b", "<b>", "H~2~O", "x^2^", "==y==", "`z`", "#1",
        "!w", "끝\\",
    ];
    let doc = RichDocument {
        page_frame: Default::default(),
        title: "표".to_string(),
        blocks: vec![RichBlock::Table(
            vec![
                cells.iter().map(ToString::to_string).collect(),
                cells.iter().map(|c| format!("{c} 뒤")).collect(),
            ]
            .into(),
        )],
        official_agency: None,
    };
    let bytes =
        UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Markdown, None).unwrap();
    let back = read_tables(std::str::from_utf8(&bytes).unwrap());
    assert_eq!(back.len(), 1, "{}", String::from_utf8_lossy(&bytes));
    let RichBlock::Table(original) = &doc.blocks[0] else {
        unreachable!()
    };
    assert_eq!(
        back[0],
        original.rows,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
}

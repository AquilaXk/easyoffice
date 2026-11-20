//! PD-262: an inspect result stays small however much text the document holds, and one that would
//! outgrow the output ceiling is refused instead of built.

use easyoffice_app::readback::{inspect_source_bytes, ReadbackQuery};
use easyoffice_formats::{
    DocumentFormat, EasyOfficeError, RichBlock, RichDocument, RichParagraph, RichRun,
    UniversalFormatConverter,
};

fn headings_docx(count: usize, text: &str) -> Vec<u8> {
    let blocks = (0..count)
        .map(|_| {
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain(text)],
                heading_level: 1,
                ..RichParagraph::default()
            })
        })
        .collect();
    let doc = RichDocument {
        blocks,
        ..RichDocument::default()
    };
    UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None).expect("encode docx")
}

#[test]
fn long_paragraph_and_heading_text_is_cut_to_a_preview() {
    // 64 KB 제목 600개: 개요와 블록 목록에 한 번씩만 실려도 77 MB가 된다.
    let bytes = headings_docx(600, &"가나다라".repeat(5_400));
    let out = inspect_source_bytes(&bytes, Some("a.docx"), None, &ReadbackQuery::default())
        .expect("inspect");
    assert!(
        out.len() < 4 * 1024 * 1024,
        "inspect result is {} bytes",
        out.len()
    );
    assert!(
        out.contains("\"blockIndex\":599"),
        "every heading is still listed"
    );
}

#[test]
fn a_result_over_the_output_ceiling_is_refused() {
    // 600자 제목 1만 개를 maxItems 1만으로: 미리보기로 잘려도 개요와 블록 목록이 8 MiB를 넘는다.
    let bytes = headings_docx(10_000, &"가나다라".repeat(150));
    let query = ReadbackQuery {
        max_items: 10_000,
        ..ReadbackQuery::default()
    };
    match inspect_source_bytes(&bytes, Some("a.docx"), None, &query) {
        Err(EasyOfficeError::ResourceBudgetExceeded(_)) => {}
        other => panic!(
            "expected ResourceBudgetExceeded, got {:?}",
            other.map(|s| s.len())
        ),
    }
}

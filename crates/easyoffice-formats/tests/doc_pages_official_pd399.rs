//! The Pages window takes the date and time, the bookmark, the watermark, the page break, the highlight and the capitals
//! from the word processor (PD-399). What `.pages` keeps is written; what it cannot hold is counted by the save warnings
//! (`doc_save_loss` for the Word-style items, `pages_write_loss` for the rest) instead of being dropped without a word.

use easyoffice_formats::{
    doc_save_loss, pages_write_loss, CharAdvanced, DocumentFormat, RichBlock, RichDocument,
    RichPageFrame, RichParagraph, RichRun, TextFx,
};

fn doc_with(paragraph: RichParagraph, watermark: Option<&str>) -> RichDocument {
    RichDocument {
        title: "표본".to_string(),
        blocks: vec![RichBlock::Paragraph(paragraph)],
        page_frame: RichPageFrame {
            watermark: watermark.map(str::to_string),
            ..RichPageFrame::default()
        },
        ..RichDocument::default()
    }
}

fn paragraph(runs: Vec<RichRun>) -> RichParagraph {
    RichParagraph {
        runs,
        ..RichParagraph::default()
    }
}

fn warnings(doc: &RichDocument) -> Vec<String> {
    let mut out = doc_save_loss(doc, DocumentFormat::Pages).warnings("PAGES");
    out.extend(pages_write_loss(doc).warnings());
    out
}

#[test]
fn a_plain_document_saves_to_pages_without_a_warning() {
    let doc = doc_with(paragraph(vec![RichRun::plain("본문")]), None);
    assert!(warnings(&doc).is_empty());
}

#[test]
fn a_date_field_a_bookmark_and_a_watermark_are_warned_about_in_a_pages_save() {
    let date = RichRun {
        field_code: Some("DATE \\@ \"yyyy-MM-dd\"".to_string()),
        ..RichRun::plain("2026-11-28")
    };
    let doc = doc_with(paragraph(vec![RichRun::plain("작성일 "), date]), None);
    let lines = warnings(&doc);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("PAGES 저장에서 빠진 필드·책갈피"),
        "{lines:?}"
    );

    let start = RichRun {
        bookmark_start: Some("결론".to_string()),
        ..RichRun::plain("끝")
    };
    let bookmarked = doc_with(paragraph(vec![start]), None);
    assert_eq!(
        warnings(&bookmarked).len(),
        1,
        "{:?}",
        warnings(&bookmarked)
    );

    let marked = doc_with(paragraph(vec![RichRun::plain("본문")]), Some("초안"));
    let lines = warnings(&marked);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("워터마크"), "{lines:?}");
}

#[test]
fn a_page_break_and_the_capitals_are_counted_as_formats_pages_does_not_write() {
    let mut broken = paragraph(vec![RichRun::plain("둘째 쪽")]);
    broken.page_break_before = true;
    let lines = warnings(&doc_with(broken, None));
    assert_eq!(lines, ["Pages 저장에서 빠진 서식 1곳"]);

    let capitals = RichRun {
        advanced: CharAdvanced {
            fx: TextFx {
                all_caps: true,
                ..TextFx::default()
            },
            ..CharAdvanced::default()
        },
        ..RichRun::plain("abc")
    };
    let lines = warnings(&doc_with(paragraph(vec![capitals]), None));
    assert_eq!(lines, ["Pages 저장에서 빠진 서식 1곳"]);
}

#[test]
fn the_highlight_is_written_to_pages_and_is_not_a_loss() {
    let highlighted = RichRun {
        highlight: Some("FFFF00".to_string()),
        ..RichRun::plain("강조")
    };
    assert!(warnings(&doc_with(paragraph(vec![highlighted]), None)).is_empty());
}

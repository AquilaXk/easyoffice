//! Structural check against a real, locally provided `.pages` file (never committed).
//!
//! Run with `EASYOFFICE_PAGES_SAMPLE=<path> cargo test -p easyoffice-formats --test
//! pages_real_sample -- --ignored --nocapture`. Only counts are printed; no document text.

use std::time::Instant;

use easyoffice_formats::{DecodedContent, DocumentFormat, RichBlock, UniversalFormatConverter};

#[test]
#[ignore = "needs EASYOFFICE_PAGES_SAMPLE=<path to a real .pages file>"]
fn real_sample_decodes_structurally() {
    let Ok(path) = std::env::var("EASYOFFICE_PAGES_SAMPLE") else {
        eprintln!("EASYOFFICE_PAGES_SAMPLE is not set; nothing to check");
        return;
    };
    let bytes = std::fs::read(&path).expect("sample readable");
    let start = Instant::now();
    let doc = UniversalFormatConverter::decode_rich(&bytes, Some("sample.pages")).expect("decode");
    let elapsed = start.elapsed();
    assert_eq!(doc.format, DocumentFormat::Pages);
    let DecodedContent::Document(d) = doc.content else {
        panic!("expected a document");
    };
    let paragraphs = d
        .blocks
        .iter()
        .filter(|b| matches!(b, RichBlock::Paragraph(_)))
        .count();
    let tables: Vec<(usize, usize)> = d
        .blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Table(rows) => Some((rows.len(), rows.first().map_or(0, Vec::len))),
            RichBlock::Paragraph(_) => None,
        })
        .collect();
    let runs: usize = d
        .blocks
        .iter()
        .map(|b| match b {
            RichBlock::Paragraph(p) => p.runs.len(),
            RichBlock::Table(_) => 0,
        })
        .sum();
    let text_cells: usize = d
        .blocks
        .iter()
        .map(|b| match b {
            RichBlock::Table(rows) => rows.iter().flatten().filter(|c| !c.is_empty()).count(),
            RichBlock::Paragraph(_) => 0,
        })
        .sum();
    let headings = d
        .blocks
        .iter()
        .filter(|b| matches!(b, RichBlock::Paragraph(p) if p.heading_level > 0))
        .count();
    eprintln!(
        "paragraphs={paragraphs} runs={runs} headings={headings} tables={} (dims {tables:?}) non-empty table cells={text_cells} warnings={} elapsed={elapsed:?}",
        tables.len(),
        doc.warnings.len()
    );
    for w in &doc.warnings {
        // warnings only contain kinds and counts
        eprintln!("warning: {w}");
    }
    assert!(paragraphs > 0, "no paragraphs decoded");
    assert!(runs > 0, "no runs decoded");
    assert!(!tables.is_empty(), "no tables detected");
    assert!(elapsed.as_secs() < 30, "decode too slow: {elapsed:?}");
}

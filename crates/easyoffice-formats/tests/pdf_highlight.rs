//! Highlight and underline annotations (ISO 32000 §12.5.6.10) written over every occurrence of a
//! phrase: one annotation per occurrence and page, with one quad per line the occurrence covers,
//! at the place the text is drawn.

use easyoffice_formats::{
    PdfHighlight, PdfHighlightKind, PdfSerializer, PdfWriteOptions, RichBlock, RichDocument,
    RichImage, RichParagraph, RichRun, UniversalFormatConverter,
};

/// The body text starts at x = 54 and its first baseline is y = 760; lines are 16 pt apart; a text
/// quad runs from 3 pt under the baseline to 10 pt over it; the font is 12 pt.
const LEFT: f64 = 54.0;
const FIRST_BASELINE: f64 = 760.0;
const PITCH: f64 = 16.0;
const BELOW: f64 = 3.0;
const ABOVE: f64 = 10.0;
const EM: f64 = 12.0;

fn doc(lines: &[&str]) -> RichDocument {
    RichDocument {
        page_frame: Default::default(),
        title: "형광펜".to_string(),
        blocks: lines
            .iter()
            .map(|l| {
                RichBlock::Paragraph(RichParagraph {
                    runs: vec![RichRun::plain(*l)],
                    ..RichParagraph::default()
                })
            })
            .collect(),
        official_agency: None,
    }
}

fn highlight(text: &str, kind: PdfHighlightKind) -> PdfHighlight {
    PdfHighlight {
        text: text.to_string(),
        kind,
        color: [0xFF, 0xFF, 0x00],
    }
}

fn write(doc: &RichDocument, highlight: PdfHighlight) -> (String, Vec<String>) {
    let options = PdfWriteOptions {
        highlight: Some(highlight),
        ..PdfWriteOptions::default()
    };
    let (bytes, warnings) = UniversalFormatConverter::encode_rich_pdf(doc, &options);
    // The structure is ASCII; text strings are hex, so a lossy view keeps every dictionary.
    (String::from_utf8_lossy(&bytes).into_owned(), warnings)
}

/// Every `/QuadPoints [...]` of the file as numbers.
fn quads(pdf: &str) -> Vec<Vec<f64>> {
    pdf.match_indices("/QuadPoints [")
        .map(|(at, m)| {
            let rest = &pdf[at + m.len()..];
            rest[..rest.find(']').unwrap()]
                .split_whitespace()
                .map(|n| n.parse().unwrap())
                .collect()
        })
        .collect()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.01
}

/// The quad of a rectangle in the order text markup annotations use: top-left, top-right,
/// bottom-left, bottom-right.
fn quad(x0: f64, x1: f64, baseline: f64) -> Vec<f64> {
    let (top, bottom) = (baseline + ABOVE, baseline - BELOW);
    vec![x0, top, x1, top, x0, bottom, x1, bottom]
}

fn assert_quad(actual: &[f64], expected: &[f64]) {
    assert_eq!(actual.len(), expected.len(), "{actual:?}");
    for (a, e) in actual.iter().zip(expected) {
        assert!(close(*a, *e), "{actual:?} != {expected:?}");
    }
}

#[test]
fn a_korean_phrase_is_marked_where_it_is_drawn() {
    // "가나다 " is 3 em + a half-em space before the phrase; the phrase is 5 em wide.
    let (pdf, warnings) = write(
        &doc(&["가나다 하이라이트 라마"]),
        highlight("하이라이트", PdfHighlightKind::Highlight),
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(pdf.matches("/Subtype /Highlight").count(), 1, "{pdf}");
    let x0 = LEFT + 3.5 * EM;
    assert_eq!(quads(&pdf).len(), 1);
    assert_quad(&quads(&pdf)[0], &quad(x0, x0 + 5.0 * EM, FIRST_BASELINE));
    assert!(pdf.contains("/C [1 1 0]"), "{pdf}");
    assert!(pdf.contains("/F 4"), "{pdf}");
    assert!(pdf.contains("/Annots ["), "{pdf}");
    assert!(
        pdf.contains("/BM /Multiply"),
        "a highlight blends by multiplying: {pdf}"
    );
}

#[test]
fn an_underline_is_an_underline_annotation_with_its_own_appearance() {
    let (pdf, _) = write(
        &doc(&["밑줄 칠 문장"]),
        PdfHighlight {
            color: [0xFF, 0x00, 0x00],
            ..highlight("칠", PdfHighlightKind::Underline)
        },
    );
    assert_eq!(pdf.matches("/Subtype /Underline").count(), 1, "{pdf}");
    assert_eq!(pdf.matches("/Subtype /Highlight").count(), 0);
    assert!(pdf.contains("/C [1 0 0]"), "{pdf}");
    let x0 = LEFT + 2.5 * EM;
    assert_quad(&quads(&pdf)[0], &quad(x0, x0 + EM, FIRST_BASELINE));
    assert!(
        pdf.contains("1 0 0 RG"),
        "the line is stroked in the color: {pdf}"
    );
}

#[test]
fn latin_text_uses_the_helvetica_advances_and_mixed_lines_the_korean_font_widths() {
    // Helvetica: "Hello " = 722+556+222+222+556+278 = 2556, "World" = 944+556+333+222+556 = 2611.
    let (pdf, _) = write(
        &doc(&["Hello World"]),
        highlight("World", PdfHighlightKind::Highlight),
    );
    let x0 = LEFT + EM * 2.556;
    assert_quad(&quads(&pdf)[0], &quad(x0, x0 + EM * 2.611, FIRST_BASELINE));
    // A line with Hangul is shown in the Korean font: ASCII is half an em wide there.
    let (pdf, _) = write(
        &doc(&["Hello 월드 끝"]),
        highlight("월드", PdfHighlightKind::Highlight),
    );
    let x0 = LEFT + 3.0 * EM;
    assert_quad(&quads(&pdf)[0], &quad(x0, x0 + 2.0 * EM, FIRST_BASELINE));
}

#[test]
fn every_occurrence_gets_an_annotation_on_the_line_it_is_on() {
    let (pdf, _) = write(
        &doc(&["가나 가", "다 가나"]),
        highlight("가", PdfHighlightKind::Highlight),
    );
    let found = quads(&pdf);
    assert_eq!(found.len(), 3, "{found:?}");
    assert_quad(&found[0], &quad(LEFT, LEFT + EM, FIRST_BASELINE));
    let second = LEFT + 2.5 * EM;
    assert_quad(&found[1], &quad(second, second + EM, FIRST_BASELINE));
    assert_quad(
        &found[2],
        &quad(LEFT + 1.5 * EM, LEFT + 2.5 * EM, FIRST_BASELINE - PITCH),
    );
}

#[test]
fn an_occurrence_across_a_line_break_gets_one_annotation_with_a_quad_per_line() {
    // 40 Hangul fill a line (40 x 12 = 480 pt of 487): the phrase's first character ends line 1.
    let text = format!("{}가나{}", "라".repeat(39), "라".repeat(10));
    let (pdf, _) = write(
        &doc(&[&text]),
        highlight("가나", PdfHighlightKind::Highlight),
    );
    assert_eq!(pdf.matches("/Subtype /Highlight").count(), 1, "{pdf}");
    let found = quads(&pdf);
    assert_eq!(found.len(), 1);
    let first = LEFT + 39.0 * EM;
    let mut expected = quad(first, first + EM, FIRST_BASELINE);
    expected.extend(quad(LEFT, LEFT + EM, FIRST_BASELINE - PITCH));
    assert_quad(&found[0], &expected);
}

#[test]
fn a_phrase_across_a_break_at_a_space_is_marked_on_both_lines_without_the_space() {
    // The break falls at the space between the words; each line is marked for its own words.
    let text = format!("{} 가나 다라", "마".repeat(37));
    let (pdf, _) = write(
        &doc(&[&text]),
        highlight("가나 다라", PdfHighlightKind::Highlight),
    );
    let found = quads(&pdf);
    assert_eq!(found.len(), 1, "{found:?}");
    let first = LEFT + 37.0 * EM + 0.5 * EM;
    let mut expected = quad(first, first + 2.0 * EM, FIRST_BASELINE);
    expected.extend(quad(LEFT, LEFT + 2.0 * EM, FIRST_BASELINE - PITCH));
    assert_quad(&found[0], &expected);
}

#[test]
fn an_occurrence_across_a_page_break_has_one_annotation_per_page() {
    // 42 lines fit a page; the phrase starts on the last line of page 1 and ends on page 2.
    let mut lines: Vec<String> = (0..41).map(|i| format!("줄 {i}")).collect();
    lines.push(format!("{}가나", "라".repeat(39)));
    lines.push("끝".to_string());
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let (pdf, _) = write(&doc(&refs), highlight("가나", PdfHighlightKind::Highlight));
    assert_eq!(
        pdf.matches("/Subtype /Highlight").count(),
        2,
        "one per page: {pdf}"
    );
    let found = quads(&pdf);
    assert_eq!(found.len(), 2);
    let last_baseline = FIRST_BASELINE - 41.0 * PITCH;
    let first = LEFT + 39.0 * EM;
    assert_quad(&found[0], &quad(first, first + EM, last_baseline));
    assert_quad(&found[1], &quad(LEFT, LEFT + EM, FIRST_BASELINE));
}

#[test]
fn the_phrase_is_written_as_an_escaped_text_string() {
    let (pdf, _) = write(
        &doc(&["x (a\\b) y"]),
        highlight("(a\\b)", PdfHighlightKind::Highlight),
    );
    assert!(pdf.contains("/Contents (\\(a\\\\b\\))"), "{pdf}");
    let (pdf, _) = write(
        &doc(&["가나다"]),
        highlight("나", PdfHighlightKind::Highlight),
    );
    assert!(pdf.contains("/Contents <FEFFB098>"), "{pdf}");
}

#[test]
fn a_phrase_that_is_not_in_the_text_writes_no_annotation_and_says_so() {
    let (pdf, warnings) = write(
        &doc(&["본문"]),
        highlight("없음", PdfHighlightKind::Highlight),
    );
    assert!(
        !pdf.contains("/Subtype /Highlight") && !pdf.contains("/Annots"),
        "{pdf}"
    );
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("찾지 못"), "{warnings:?}");
}

#[test]
fn at_most_ten_thousand_annotations_are_written_and_the_rest_is_reported() {
    let text = "가 ".repeat(12_000);
    let started = std::time::Instant::now();
    let (pdf, warnings) = write(&doc(&[&text]), highlight("가", PdfHighlightKind::Highlight));
    assert_eq!(pdf.matches("/Subtype /Highlight").count(), 10_000);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("2000"), "{warnings:?}");
    assert!(started.elapsed().as_secs() < 30, "{:?}", started.elapsed());
}

#[test]
fn the_file_with_annotations_reads_back_with_the_same_text() {
    let lines = ["첫 줄 가나다", "둘째 줄 가나다 라마바", "셋째 가나다"];
    let plain =
        UniversalFormatConverter::encode_rich_pdf(&doc(&lines), &PdfWriteOptions::default()).0;
    let options = PdfWriteOptions {
        watermark: Some("대외비".to_string()),
        highlight: Some(highlight("가나다", PdfHighlightKind::Underline)),
        ..PdfWriteOptions::default()
    };
    let (marked, warnings) = UniversalFormatConverter::encode_rich_pdf(&doc(&lines), &options);
    assert!(warnings.is_empty());
    let before = PdfSerializer::parse(&plain).unwrap();
    let after = PdfSerializer::parse(&marked).unwrap();
    assert_eq!(after.pages.len(), before.pages.len());
    assert_eq!(after.pages[0].body_text, before.pages[0].body_text);
    assert_eq!(after.watermark_text.as_deref(), Some("대외비"));
}

#[test]
fn highlights_work_on_documents_with_pictures_too() {
    let png = {
        // 1x1 PNG
        const B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
        easyoffice_formats::base64_codec::decode(B64).unwrap()
    };
    let mut document = doc(&["그림 앞 가나", "그림 뒤 가나"]);
    document
        .blocks
        .insert(1, RichBlock::Image(RichImage::new(png, "점").unwrap()));
    let (pdf, _) = write(&document, highlight("가나", PdfHighlightKind::Highlight));
    assert_eq!(pdf.matches("/Subtype /Highlight").count(), 2, "{pdf}");
    let found = quads(&pdf);
    // Each text page starts at the first baseline.
    assert!(
        close(found[0][1], FIRST_BASELINE + ABOVE) && close(found[1][1], FIRST_BASELINE + ABOVE)
    );
}

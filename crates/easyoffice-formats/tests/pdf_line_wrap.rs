//! The PDF writer breaks a long paragraph at the text width of the A4 page (a word boundary when it
//! can, inside a word only when the word alone is wider than a line) and continues on the next page.

use easyoffice_formats::{PdfDocumentModel, PdfPageModel, PdfSerializer};

/// A4 width minus the 54 pt margins, in thousandths of the 12 pt font size.
const LINE_UNITS: u32 = (595 - 2 * 54) * 1000 / 12;

fn model(body: &str) -> PdfDocumentModel {
    PdfDocumentModel {
        title: "줄바꿈".to_string(),
        pages: vec![PdfPageModel {
            page_number: 1,
            rotation_degrees: 0,
            body_text: body.to_string(),
            approval_stamp: None,
        }],
        watermark_text: None,
    }
}

fn written_pages(body: &str) -> Vec<Vec<String>> {
    let bytes = PdfSerializer::serialize(&model(body));
    PdfSerializer::parse(&bytes)
        .unwrap()
        .pages
        .iter()
        .map(|p| p.body_text.split('\n').map(str::to_string).collect())
        .collect()
}

/// The widths the written file declares for the ASCII codes of the Korean font (`/W [1 [...]]`, code 0x20 first).
fn declared_ascii_widths() -> Vec<u32> {
    let bytes = PdfSerializer::serialize(&model("한"));
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let start = text
        .find("/W [1 [")
        .expect("the Korean font declares widths")
        + "/W [1 [".len();
    let end = start + text[start..].find(']').unwrap();
    text[start..end]
        .split_whitespace()
        .map(|n| n.parse().unwrap())
        .collect()
}

/// Width of a non-ASCII line in the Korean font: full-width glyphs are 1000, ASCII the declared width.
fn cid_units(line: &str) -> u32 {
    let ascii = declared_ascii_widths();
    line.chars()
        .map(|c| {
            if c.is_ascii() {
                ascii[c as usize - 0x20]
            } else {
                1000
            }
        })
        .sum()
}

#[test]
fn a_long_korean_paragraph_wraps_inside_the_page_width() {
    let sentence = "대한민국의 국민이 되는 요건은 법률로 정한다 ";
    let body = sentence.repeat(12);
    let pages = written_pages(&body);
    let lines: Vec<&String> = pages.iter().flatten().collect();
    assert!(lines.len() > 5, "{lines:?}");
    for line in &lines {
        assert!(cid_units(line) <= LINE_UNITS, "too wide: {line}");
        assert!(!line.ends_with(' '), "trailing space kept: {line:?}");
    }
    let rejoined: String = lines
        .iter()
        .map(|l| l.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(
        rejoined,
        body.trim_end(),
        "nothing lost, breaks only at spaces"
    );
}

#[test]
fn english_words_are_not_split_and_mixed_scripts_stay_inside_the_width() {
    let body = "The quick brown fox 점프합니다 over the lazy dog and keeps running 계속 달립니다 "
        .repeat(8);
    let pages = written_pages(&body);
    for line in pages.iter().flatten() {
        if !line.is_ascii() {
            assert!(cid_units(line) <= LINE_UNITS, "too wide: {line}");
        }
        for word in line.split(' ') {
            assert!(body.contains(word), "a word was cut: {word:?}");
        }
    }
    assert!(pages.iter().flatten().count() > 4);
}

#[test]
fn a_word_wider_than_a_line_is_split_and_loses_no_text() {
    let long_word = "가".repeat(150);
    let pages = written_pages(&format!("앞 {long_word} 뒤"));
    let lines: Vec<&String> = pages.iter().flatten().collect();
    assert!(lines.len() >= 4, "{lines:?}");
    for line in &lines {
        assert!(cid_units(line) <= LINE_UNITS, "{line}");
    }
    let hangul: usize = lines
        .iter()
        .map(|l| l.chars().filter(|c| *c == '가').count())
        .sum();
    assert_eq!(hangul, 150);

    let url = "x".repeat(400);
    let ascii_lines: Vec<String> = written_pages(&url).into_iter().flatten().collect();
    assert!(ascii_lines.len() >= 3);
    assert_eq!(ascii_lines.concat(), url);
}

#[test]
fn short_lines_and_blank_lines_are_unchanged() {
    let body = "첫 줄\n\n  들여쓴 줄\nplain";
    assert_eq!(
        written_pages(body),
        vec![vec!["첫 줄", "", "  들여쓴 줄", "plain"]]
    );
}

#[test]
fn wrapped_lines_continue_on_the_next_page_with_sequential_page_numbers() {
    let paragraph = "가나다라 마바사 아자차카 타파하 ".repeat(30);
    let body = std::iter::repeat_n(paragraph.as_str(), 8)
        .collect::<Vec<_>>()
        .join("\n");
    let bytes = PdfSerializer::serialize(&model(&body));
    let parsed = PdfSerializer::parse(&bytes).unwrap();
    assert!(parsed.pages.len() >= 3, "{}", parsed.pages.len());
    for (idx, page) in parsed.pages.iter().enumerate() {
        assert_eq!(page.page_number, idx + 1);
        assert!(page.body_text.split('\n').count() <= 42);
    }
}

mod write_options {
    use easyoffice_formats::{
        PdfSerializer, PdfWriteOptions, RichBlock, RichDocument, RichParagraph, RichRun,
        UniversalFormatConverter,
    };

    fn doc() -> RichDocument {
        RichDocument {
            page_frame: Default::default(),
            title: "옵션".to_string(),
            blocks: vec![RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("본문 한 줄")],
                ..RichParagraph::default()
            })],
            official_agency: None,
        }
    }

    #[test]
    fn rotation_watermark_and_stamp_are_written_as_page_fields() {
        let options = PdfWriteOptions {
            rotation_degrees: 90,
            watermark: Some("대외비".to_string()),
            approval_stamp: Some("승인필".to_string()),
            highlight: None,
        };
        let (bytes, warnings) = UniversalFormatConverter::encode_rich_pdf(&doc(), &options);
        assert!(warnings.is_empty());
        let parsed = PdfSerializer::parse(&bytes).unwrap();
        assert_eq!(parsed.watermark_text.as_deref(), Some("대외비"));
        assert_eq!(parsed.pages[0].rotation_degrees, 90);
        assert_eq!(parsed.pages[0].approval_stamp.as_deref(), Some("승인필"));
        assert_eq!(parsed.pages[0].body_text, "본문 한 줄");
    }

    #[test]
    fn default_options_give_the_same_bytes_as_encode_rich() {
        let plain = UniversalFormatConverter::encode_rich(
            &doc(),
            easyoffice_formats::DocumentFormat::Pdf,
            None,
        )
        .unwrap();
        assert_eq!(
            plain,
            UniversalFormatConverter::encode_rich_pdf(&doc(), &PdfWriteOptions::default()).0
        );
    }
}

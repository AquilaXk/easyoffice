//! Tests of what the export does to text beyond embedding it (PD-131): private-use characters in
//! the text map, and synthetic bold and italic. They read the objects of the exported PDF.

use super::*;
use crate::pdf_synthetic;
use usvg::fontdb::{FaceInfo, Language, Style, Weight};

/// The bundled Regular face plus two registered faces that stand for real bold and real italic
/// ones (the same font file under another family, weight and style), so the "a real face is never
/// emphasised again" rules are tested without depending on installed fonts.
fn styled_db() -> Option<Arc<Database>> {
    let mut db = crate::pdf_fonts::tests::bundled_only_db()?;
    let base = db.faces().next()?.clone();
    for (family, weight, style) in [
        ("Test Bold", Weight::BOLD, Style::Normal),
        ("Test Italic", Weight::NORMAL, Style::Italic),
    ] {
        db.push_face_info(FaceInfo {
            families: vec![(family.to_string(), Language::English_UnitedStates)],
            weight,
            style,
            ..base.clone()
        });
    }
    Some(Arc::new(db))
}

fn page(body: &str) -> String {
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="320" height="120">{body}</svg>"#)
}

fn export(db: &Arc<Database>, svg: &str) -> Vec<u8> {
    let mut assembler = PdfAssembler::new(db, true);
    assembler.add_pages(&[svg.to_string()]).expect("pages");
    assembler.finish().expect("pdf")
}

/// The decoded content of every Flate stream of the PDF, one after the other.
fn inflated_streams(pdf: &[u8]) -> String {
    const START: &[u8] = b"stream\n";
    const END: &[u8] = b"endstream";
    let mut text = String::new();
    let mut at = 0;
    while let Some(rel) = pdf[at..].windows(START.len()).position(|w| w == START) {
        let begin = at + rel + START.len();
        let Some(len) = pdf[begin..].windows(END.len()).position(|w| w == END) else {
            break;
        };
        if let Ok(bytes) = miniz_oxide::inflate::decompress_to_vec_zlib(&pdf[begin..begin + len]) {
            text.push_str(&String::from_utf8_lossy(&bytes));
        }
        at = begin + len + END.len();
    }
    text
}

fn glyph_runs(content: &str) -> usize {
    content.matches(" Tj").count()
}

const BOLD: &str = r##"<text x="16" y="64" font-family="serif" font-size="24" font-weight="bold" fill="#112233">AB가</text>"##;
const PLAIN: &str =
    r##"<text x="16" y="64" font-family="serif" font-size="24" fill="#112233">AB가</text>"##;

/// One page per character: a face without a glyph draws every unknown character as the same
/// missing glyph, and a map keeps one text per glyph.
#[test]
fn a_private_use_character_is_copied_and_searched_as_its_standard_text() {
    let Some(db) = styled_db() else { return };
    // U+F02B1 is an enclosed 1 of the word processor, U+F06C a round bullet, U+F03E0 unknown.
    let pages: Vec<String> = ["A&#xF02B1;가", "A&#xF06C;가", "A&#xF03E0;가"]
        .iter()
        .map(|text| {
            page(&format!(
                r#"<text x="16" y="64" font-family="serif" font-size="24">{text}</text>"#
            ))
        })
        .collect();
    let mut assembler = PdfAssembler::new(&db, true);
    assembler.add_pages(&pages).expect("pages");
    let pdf = assembler.finish().expect("pdf");
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.contains("<2460    >"), "the enclosed 1 is U+2460");
    assert!(text.contains("<25CF>"), "the bullet is U+25CF");
    assert!(
        !text.contains("DB80DEB1") && !text.contains("<F06C>"),
        "no known code is left"
    );
    assert!(
        text.contains("DB80DFE0"),
        "an unknown private-use code is kept as it is"
    );
    assert!(
        text.contains("<D55C>") || text.contains("<AC00>"),
        "other text is untouched"
    );
    assert_eq!(
        verify_exported_pdf(&pdf, 3, true).map(|check| check.warning),
        Ok(None)
    );
}

/// The rewrite keeps every byte offset: the cross-reference table written before it still points at
/// each object, so the file opens without repair.
#[test]
fn the_rewritten_file_keeps_its_cross_reference_offsets() {
    let Some(db) = styled_db() else { return };
    let pdf = export(
        &db,
        &page(r#"<text x="16" y="64" font-family="serif" font-size="24">&#xF02B1;A</text>"#),
    );
    let text = String::from_utf8_lossy(&pdf).into_owned();
    assert!(text.contains("<2460    >"));
    let tail = text.rsplit("startxref\n").next().expect("startxref");
    let table: usize = tail
        .lines()
        .next()
        .expect("offset")
        .parse()
        .expect("number");
    assert!(
        pdf[table..].starts_with(b"xref\n"),
        "the table is where startxref says"
    );
    let entries = text[table..].lines().filter(|l| l.ends_with(" n")).count();
    assert!(entries > 3);
    for line in text[table..].lines().filter(|l| l.ends_with(" n")) {
        let offset: usize = line[..10].parse().expect("offset");
        assert!(
            pdf[offset..].iter().take(12).any(|b| *b == b'o'),
            "an object starts at {offset}"
        );
    }
}

#[test]
fn bold_text_on_a_regular_face_is_stroked_once() {
    let Some(db) = styled_db() else { return };
    let bold = inflated_streams(&export(&db, &page(BOLD)));
    let plain = inflated_streams(&export(&db, &page(PLAIN)));
    assert!(bold.contains("2 Tr"), "fill and stroke in one run: {bold}");
    assert!(bold.contains("0.48 w"), "2% of 24: {bold}");
    assert!(
        bold.contains("0.06666667 0.13333334 0.2 SCN"),
        "the fill colour: {bold}"
    );
    assert!(!plain.contains("2 Tr") && !plain.contains("0.48 w"));
    assert_eq!(
        glyph_runs(&bold),
        glyph_runs(&plain),
        "the text is emitted once"
    );
}

#[test]
fn a_real_bold_face_is_not_emboldened_again() {
    let Some(db) = styled_db() else { return };
    let real = BOLD.replace("serif", "Test Bold");
    let content = inflated_streams(&export(&db, &page(&real)));
    assert!(
        !content.contains("2 Tr") && !content.contains(" w\n"),
        "{content}"
    );
}

#[test]
fn italic_text_on_an_upright_face_is_sheared_about_its_baseline() {
    let Some(db) = styled_db() else { return };
    let italic = r#"<text x="16" y="64" font-family="serif" font-size="24" font-style="italic">Slant</text>"#;
    let content = inflated_streams(&export(&db, &page(italic)));
    // x' = x - 0.25 y + 0.25 baseline: the matrix keeps the baseline (y = 64) in place.
    assert!(content.contains("1 0 -0.25 1 16 0 cm"), "{content}");
    let upright = inflated_streams(&export(
        &db,
        &page(&italic.replace(" font-style=\"italic\"", "")),
    ));
    assert!(!upright.contains("-0.25"));
}

#[test]
fn a_real_italic_face_is_not_sheared_again() {
    let Some(db) = styled_db() else { return };
    let real = r#"<text x="16" y="64" font-family="Test Italic" font-size="24" font-style="italic">Slant</text>"#;
    let content = inflated_streams(&export(&db, &page(real)));
    assert!(glyph_runs(&content) > 0, "the text is drawn: {content}");
    assert!(!content.contains("-0.25"), "{content}");
}

#[test]
fn bold_italic_text_gets_both_and_an_existing_style_is_kept() {
    let Some(db) = styled_db() else { return };
    let both = r##"<text x="10" y="50" font-family="serif" font-size="20" font-weight="700" font-style="italic" style="fill:#000000" transform="translate(0 0)">Both</text>"##;
    let options = parse_options(&db);
    let (_, report) = pdf_synthetic::parse_page(&page(both), &options).expect("page");
    assert_eq!(
        report,
        pdf_synthetic::Report {
            bold: 1,
            italic: 1,
            unsupported: 0
        }
    );
    let content = inflated_streams(&export(&db, &page(both)));
    assert!(
        content.contains("2 Tr") && content.contains("-0.25"),
        "{content}"
    );
}

fn report_of(db: &Arc<Database>, body: &str) -> pdf_synthetic::Report {
    pdf_synthetic::parse_page(&page(body), &parse_options(db))
        .expect("page")
        .1
}

#[test]
fn mixed_text_is_left_as_it_is_and_counted() {
    let Some(db) = styled_db() else { return };
    // Bold and regular spans in one element: stroking the whole element would embolden both.
    let mixed = r#"<text x="16" y="64" font-family="serif" font-size="24" font-weight="bold">A<tspan font-weight="normal">B</tspan></text>"#;
    let report = report_of(&db, mixed);
    assert_eq!((report.bold, report.unsupported), (0, 1));
    assert!(!inflated_streams(&export(&db, &page(mixed))).contains("2 Tr"));
    // Bold text whose fill is a gradient: the stroke colour would not match.
    let gradient = r##"<defs><linearGradient id="g"><stop offset="0" stop-color="#000"/><stop offset="1" stop-color="#fff"/></linearGradient></defs><text x="16" y="64" font-family="serif" font-size="24" font-weight="bold" fill="url(#g)">A</text>"##;
    assert_eq!(report_of(&db, gradient).unsupported, 1);
    // A stroke of its own is respected.
    let stroked = r##"<text x="16" y="64" font-family="serif" font-size="24" font-weight="bold" stroke="#f00">A</text>"##;
    assert_eq!(report_of(&db, stroked), pdf_synthetic::Report::default());
}

/// Text that asks for nothing is hidden in the analysis copy only: the page that is converted
/// still has it, and a style sheet (classes could make anything bold) turns the reduction off.
#[test]
fn only_the_analysis_copy_is_reduced() {
    let Some(db) = styled_db() else { return };
    let picture = r#"<image x="0" y="0" width="4" height="4" href="data:image/png;base64,AAAA"/>"#;
    let content = inflated_streams(&export(&db, &page(&format!("{PLAIN}{BOLD}{picture}"))));
    assert!(content.contains("2 Tr"), "bold is drawn");
    // Both lines (three glyphs each) are in the converted page, the plain one too.
    assert_eq!(glyph_runs(&content), 6, "{content}");
    // A class can make text bold: with a style sheet every text element is looked at.
    let sheet = r#"<style>.b { font-weight: bold }</style>"#;
    let classed = r#"<text class="b" x="16" y="64" font-family="serif" font-size="24">AB</text>"#;
    let report = report_of(&db, &format!("{sheet}{classed}"));
    assert_eq!(report.bold, 1, "{report:?}");
}

#[test]
fn a_page_without_bold_or_italic_is_not_analysed_and_a_hidden_word_is_harmless() {
    let Some(db) = styled_db() else { return };
    assert_eq!(report_of(&db, PLAIN), pdf_synthetic::Report::default());
    // The word in the text asks nothing of the font: no edit.
    let word = r#"<text x="16" y="64" font-family="serif" font-size="24">bold italic</text>"#;
    assert_eq!(report_of(&db, word), pdf_synthetic::Report::default());
    // A medium weight is not bold.
    let medium =
        r#"<text x="16" y="64" font-family="serif" font-size="24" font-weight="500">A</text>"#;
    assert_eq!(report_of(&db, medium), pdf_synthetic::Report::default());
}

#[test]
fn the_same_pages_give_the_same_bytes_with_synthesis() {
    let Some(db) = styled_db() else { return };
    let svg = page(&format!("{BOLD}{}", PLAIN.replace("y=\"64\"", "y=\"100\"")));
    assert_eq!(export(&db, &svg), export(&db, &svg));
}

/// Hostile pages: ids that collide with the generated ones, repeated ids, a huge number of text
/// elements and broken markup. None panics or costs more than the size of the page.
#[test]
fn hostile_pages_are_bounded() {
    let Some(db) = styled_db() else { return };
    let options = parse_options(&db);

    // An element already named like a generated id, with a longer run of underscores.
    let named = format!(
        r#"<text id="eo-pdf-style-___0" x="1" y="30" font-family="serif" font-size="20">x</text>{BOLD}"#
    );
    assert_eq!(report_of(&db, &named).bold, 1);

    // Two elements that ask for bold under one id cannot be told apart: neither is touched. A twin
    // that asks for nothing is not part of the analysis and does not matter.
    let twin = |body: &str| body.replacen("<text", "<text id=\"t\"", 1);
    let twins = format!("{}{}", twin(BOLD), twin(BOLD));
    assert_eq!(report_of(&db, &twins).bold, 0);
    let with_plain = format!("{}{}", twin(BOLD), twin(PLAIN));
    assert_eq!(report_of(&db, &with_plain).bold, 1);

    // Thousands of ids that all continue the prefix: one pass, not one pass per id.
    let ids: String = (0..20_000)
        .map(|i| format!(r#"<g id="eo-pdf-style-{}"/>"#, "_".repeat(i % 50)))
        .collect();
    let started = std::time::Instant::now();
    assert_eq!(report_of(&db, &format!("{ids}{BOLD}")).bold, 1);
    assert!(started.elapsed().as_secs() < 5, "{:?}", started.elapsed());

    // More text elements than the limit: converted as they are, no synthesis.
    let many = format!("{BOLD}{}", "<text/>".repeat(50_001));
    assert_eq!(report_of(&db, &many), pdf_synthetic::Report::default());

    // Broken markup that mentions bold is an error, as without the feature.
    assert!(pdf_synthetic::parse_page("<svg bold", &options).is_err());
    assert!(
        pdf_synthetic::parse_page(r#"<svg xmlns="x" font-weight="bold"><text>"#, &options).is_err()
    );
}

/// The 300-page report of the PD-114 measurement, with a bold line every fifth line, an italic
/// line every seventh, and an enclosed number and a bullet in every line (private-use codes):
/// `cargo test --release -p easyoffice-desktop hundreds_of_styled -- --ignored --nocapture`.
/// The same pages without the emphasis are exported for the comparison.
fn styled_report_page(page: usize, styled: bool) -> String {
    let mut svg = String::from(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="794" height="1123"><rect width="794" height="1123" fill="white"/>"#,
    );
    for line in 0..46 {
        let y = 80 + line * 21;
        let family = ["함초롬바탕", "맑은 고딕", "휴먼명조", "굴림"][line % 4];
        let weight = if styled && line % 5 == 0 {
            " font-weight=\"bold\""
        } else {
            ""
        };
        let slant = if styled && line % 7 == 0 {
            " font-style=\"italic\""
        } else {
            ""
        };
        let marks = "&#xF02B1;&#xF06C;";
        svg.push_str(&format!(
            r#"<text x="72" y="{y}" font-family="{family}" font-size="14"{weight}{slant}>제{page}쪽 {line}째 줄: 검색 가능한 PDF를 만드는 한글 문서 내보내기 시험 문장입니다 {marks} ABC abc 12345</text><line x1="72" y1="{}" x2="722" y2="{}" stroke="black" stroke-width="0.5"/>"#,
            y + 4,
            y + 4
        ));
    }
    svg.push_str("</svg>");
    svg
}

#[test]
#[ignore = "measurement, run by hand"]
fn hundreds_of_styled_pages_are_measured() {
    let Some(db) = super::tests::system_font_db() else {
        return;
    };
    for styled in [false, true] {
        let pages: Vec<String> = (0..300).map(|i| styled_report_page(i, styled)).collect();
        let started = std::time::Instant::now();
        let mut assembler = PdfAssembler::new(&db, true);
        assembler.add_pages(&pages).expect("pages");
        let pdf = assembler.finish().expect("pdf");
        eprintln!(
            "MEASURE styled={styled} pages=300 svg_bytes={} pdf_bytes={} elapsed={:?}",
            pages.iter().map(String::len).sum::<usize>(),
            pdf.len(),
            started.elapsed()
        );
        assert!(verify_exported_pdf(&pdf, 300, true).is_ok());
    }
}

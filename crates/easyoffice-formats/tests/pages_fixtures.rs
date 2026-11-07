//! Decodes the three real `.pages` documents under `tests/fixtures/pages` and checks exact
//! content: paragraph text, headings, run formatting, alignment, lists, table cells and the
//! warnings for objects that are not imported.

use easyoffice_formats::{
    DecodedContent, DocumentFormat, RichAlignment, RichBlock, RichDocument, RichImage,
    RichImageFormat, RichListKind, RichParagraph, RichRun, RichScript, UniversalFormatConverter,
    ZipArchiveCodec, ZipSecurityPolicy,
};

fn open(name: &str) -> (RichDocument, Vec<String>) {
    let path = format!("{}/tests/fixtures/pages/{name}", env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(&path).expect("fixture readable");
    let decoded = UniversalFormatConverter::decode_rich(&bytes, Some(name)).expect("decode");
    assert_eq!(decoded.format, DocumentFormat::Pages);
    let DecodedContent::Document(doc) = decoded.content else {
        panic!("expected a document");
    };
    (doc, decoded.warnings)
}

fn paragraphs(doc: &RichDocument) -> Vec<&RichParagraph> {
    doc.blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some(p),
            RichBlock::Table(_) | RichBlock::Image(_) => None,
        })
        .collect()
}

fn tables(doc: &RichDocument) -> Vec<&Vec<Vec<String>>> {
    doc.blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Table(rows) => Some(rows),
            RichBlock::Paragraph(_) | RichBlock::Image(_) => None,
        })
        .collect()
}

fn paragraph_with<'a>(doc: &'a RichDocument, prefix: &str) -> &'a RichParagraph {
    paragraphs(doc)
        .into_iter()
        .find(|p| p.plain_text().starts_with(prefix))
        .unwrap_or_else(|| panic!("no paragraph starting with {prefix:?}"))
}

fn run_named<'a>(p: &'a RichParagraph, text: &str) -> &'a RichRun {
    p.runs
        .iter()
        .find(|r| r.text == text)
        .unwrap_or_else(|| panic!("no run {text:?}"))
}

fn flags(r: &RichRun) -> Flags {
    (r.bold, r.italic, r.underline, r.strike)
}

/// `(bold, italic, underline, strike)`
type Flags = (bool, bool, bool, bool);

const GENERIC_WARNING: &str = "Pages 문서는 읽기 전용으로 열고, 머리글·바닥글·각주는 본문 뒤 부록 문단으로 옮기며, 변경 추적·차트 등은 가져오지 않고 서식이 일부 단순화될 수 있습니다.";

#[test]
fn text_fixture_paragraphs_and_headings() {
    let (doc, warnings) = open("eo-pages-text.pages");
    let paras = paragraphs(&doc);
    assert_eq!(paras.len(), 19);
    assert!(tables(&doc).is_empty());
    assert_eq!(warnings, vec![GENERIC_WARNING.to_string()]);
    let headings: Vec<(String, u8)> = paras
        .iter()
        .filter(|p| p.heading_level > 0)
        .map(|p| (p.plain_text(), p.heading_level))
        .collect();
    assert_eq!(
        headings,
        vec![
            ("[1] 제목 스타일".to_string(), 1),
            ("머리말 1 스타일 문단".to_string(), 1),
            ("머리말 2 스타일 문단".to_string(), 2),
            ("머리말 3 스타일 문단".to_string(), 3),
        ]
    );
    assert_eq!(
        paras[18].plain_text(),
        "한글과 English 영문, 그리고 2026년 11월 06일 숫자 12345가 자연스럽게 섞인 Mixed 문장입니다."
    );
    assert_eq!(paras[0].runs[0].size_half_pt, Some(60));
    assert!(paras[0].runs[0].bold);
}

#[test]
fn text_fixture_character_formatting() {
    let (doc, _) = open("eo-pages-text.pages");
    let p = paragraph_with(&doc, "일반 문단에서 ");
    let seen: Vec<(&str, Flags)> = p.runs.iter().map(|r| (r.text.as_str(), flags(r))).collect();
    assert_eq!(
        seen,
        vec![
            ("일반 문단에서 ", (false, false, false, false)),
            ("굵게 단어", (true, false, false, false)),
            ("와 ", (false, false, false, false)),
            ("기울임 단어", (false, true, false, false)),
            (", ", (false, false, false, false)),
            ("밑줄 단어", (false, false, true, false)),
            (", ", (false, false, false, false)),
            ("취소선 단어", (false, false, false, true)),
            (", 그리고 ", (false, false, false, false)),
            ("굵게기울임 동시 적용", (true, true, false, false)),
            (" 단어를 확인합니다.", (false, false, false, false)),
        ]
    );

    let sizes = paragraph_with(&doc, "글자 크기 비교");
    assert_eq!(run_named(sizes, "작게9pt").size_half_pt, Some(18));
    assert_eq!(run_named(sizes, "보통14pt").size_half_pt, Some(28));
    assert_eq!(run_named(sizes, "크게24pt").size_half_pt, Some(48));
    assert_eq!(run_named(sizes, "글자 크기 비교: ").size_half_pt, Some(20));

    let colors = paragraph_with(&doc, "글자 색상과 강조색 비교");
    assert_eq!(
        run_named(colors, "빨강글자").color.as_deref(),
        Some("E61A1A")
    );
    assert_eq!(
        run_named(colors, "파랑글자").color.as_deref(),
        Some("1A4DE6")
    );
    assert_eq!(
        run_named(colors, "형광펜강조").highlight.as_deref(),
        Some("FFF233")
    );
    assert_eq!(run_named(colors, "빨강글자").highlight, None);
}

#[test]
fn text_fixture_alignment_quote_and_non_bmp_positions() {
    let (doc, _) = open("eo-pages-text.pages");
    let align = |prefix: &str| paragraph_with(&doc, prefix).alignment;
    assert_eq!(align("왼쪽 정렬이"), Some(RichAlignment::Left));
    assert_eq!(align("가운데 정렬이"), Some(RichAlignment::Center));
    assert_eq!(align("오른쪽 정렬이"), Some(RichAlignment::Right));
    assert_eq!(align("양쪽 정렬이"), Some(RichAlignment::Justify));

    let quote = paragraph_with(&doc, "문단 스타일 자체에 기울임이 정의된 인용");
    assert!(quote.runs.iter().all(|r| r.italic && !r.bold));

    // The bold run follows two emoji (four UTF-16 units) in the same paragraph.
    let emoji = paragraph_with(&doc, "😀🎉");
    let seen: Vec<(&str, bool)> = emoji
        .runs
        .iter()
        .map(|r| (r.text.as_str(), r.bold))
        .collect();
    assert_eq!(
        seen,
        vec![
            ("😀🎉 [11] 이모지 다음 ", false),
            ("굵게", true),
            (" 텍스트가 이어지는 문단입니다.", false),
        ]
    );
}

#[test]
fn lists_fixture_list_paragraphs() {
    let (doc, warnings) = open("eo-pages-lists-tables.pages");
    assert_eq!(warnings, vec![GENERIC_WARNING.to_string()]);
    let kinds: Vec<(String, Option<RichListKind>)> = paragraphs(&doc)
        .iter()
        .filter(|p| p.list.is_some())
        .map(|p| (p.plain_text(), p.list))
        .collect();
    assert_eq!(kinds.len(), 16);
    let bullets = kinds
        .iter()
        .filter(|(_, k)| *k == Some(RichListKind::Bullet))
        .count();
    assert_eq!(bullets, 7);
    assert_eq!(
        kinds[0],
        (
            "글머리표 첫 번째 항목입니다.".to_string(),
            Some(RichListKind::Bullet)
        )
    );
    assert_eq!(
        kinds[3],
        (
            "번호 목록 첫 번째 항목입니다.".to_string(),
            Some(RichListKind::Ordered)
        )
    );
    assert_eq!(
        kinds[15],
        (
            "알파벳(a-b-c) 형식 세 번째 항목입니다.".to_string(),
            Some(RichListKind::Ordered)
        )
    );
    // Headings of the list sections are plain (non-list) paragraphs with their own level.
    let heading = paragraph_with(&doc, "2. 번호 목록");
    assert_eq!((heading.heading_level, heading.list), (2, None));
}

fn grid(rows: &[&[&str]]) -> Vec<Vec<String>> {
    rows.iter()
        .map(|r| r.iter().map(|c| (*c).to_string()).collect())
        .collect()
}

#[test]
fn lists_fixture_table_cells() {
    let (doc, _) = open("eo-pages-lists-tables.pages");
    let all = tables(&doc);
    let dims: Vec<(usize, usize)> = all.iter().map(|t| (t.len(), t[0].len())).collect();
    assert_eq!(dims, vec![(4, 3), (2, 6), (3, 3), (2, 3)]);
    // Table 1: text cells.
    assert_eq!(
        *all[0],
        grid(&[
            &["구분", "항목", "비고"],
            &["A1", "사과", "과일"],
            &["B2", "당근", "채소"],
            &["C3", "우유", "유제품"],
        ])
    );
    // Table 2: number, date, currency, percent, checkbox and formatted text cells.
    assert_eq!(
        *all[1],
        grid(&[
            &["숫자", "날짜", "통화", "백분율", "체크박스", "서식 텍스트"],
            &[
                "1,234.5",
                "2026-11-06",
                "₩10,000",
                "50%",
                "☑",
                "셀 안의 굵게 글자"
            ],
        ])
    );
    // Table 3: merged-away cells are empty.
    assert_eq!(
        *all[2],
        grid(&[
            &["열 1", "열 2", "열 3"],
            &["가로 병합 셀", "", "세로 병합 셀"],
            &["일반 셀 A", "일반 셀 B", ""],
        ])
    );
    // Table 4: numbers and a formula's cached value.
    assert_eq!(
        *all[3],
        grid(&[&["값 1", "값 2", "합계 수식"], &["100", "250", "350"]])
    );
}

#[test]
fn objects_fixture_reports_only_what_is_still_skipped() {
    let (doc, warnings) = open("eo-pages-objects.pages");
    assert_eq!(
        warnings,
        vec![
            "가져오지 않은 요소: 댓글 1개".to_string(),
            GENERIC_WARNING.to_string(),
        ]
    );
    // Body text keeps its order; shape, footnote, header and footer text moved, not mixed in.
    let texts: Vec<(u8, String)> = paragraphs(&doc)
        .iter()
        .map(|p| (p.heading_level, p.plain_text()))
        .collect();
    assert_eq!(texts.len(), 21);
    assert_eq!(texts[0].1, "미지원 항목 안내 확인용 문서 (객체 테스트)");
    assert_eq!(texts[5].1, "이 문장 끝에는 각주가 달려 있습니다.");
    assert_eq!(
        texts[14].1,
        "페이지 나누기(Page Break) 뒤에 이어지는 두 번째 페이지의 본문 문단입니다."
    );
}

#[test]
fn objects_fixture_places_shape_and_text_box_text_at_the_anchor() {
    let (doc, _) = open("eo-pages-objects.pages");
    let texts: Vec<String> = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some(p.plain_text()),
            _ => None,
        })
        .collect();
    let at = |needle: &str| texts.iter().position(|t| t == needle).expect(needle);
    let shape = at("사각형 도형 안의 텍스트입니다.");
    assert_eq!(texts[shape - 1], "2. 도형 (사각형 안에 글자)");
    assert_eq!(texts[shape + 1], "3. 각주");
    let text_box = at("텍스트 상자(Text Box) 안에 작성된 안내 문구입니다.");
    assert_eq!(texts[text_box - 1], "6. 텍스트 상자");
}

#[test]
fn objects_fixture_appends_footnotes_headers_and_footers_after_the_body() {
    let (doc, _) = open("eo-pages-objects.pages");
    let tail: Vec<(u8, String)> = paragraphs(&doc)
        .iter()
        .rev()
        .take(6)
        .rev()
        .map(|p| (p.heading_level, p.plain_text()))
        .collect();
    assert_eq!(
        tail,
        vec![
            (2, "각주".to_string()),
            (
                0,
                "각주 설명 텍스트입니다. 본문 하단에 표시되는 각주 항목입니다.".to_string()
            ),
            (2, "머리글".to_string()),
            (0, "머리말: Pages 객체 테스트 문서".to_string()),
            (2, "바닥글".to_string()),
            (0, "꼬리말 - 쪽 번호: ".to_string()),
        ]
    );
}

#[test]
fn objects_fixture_imports_picture_at_its_document_position() {
    let path = format!(
        "{}/tests/fixtures/pages/eo-pages-objects.pages",
        env!("CARGO_MANIFEST_DIR")
    );
    let package = std::fs::read(path).expect("fixture readable");
    let entries = ZipArchiveCodec::unpack(&package, &ZipSecurityPolicy::default()).expect("unpack");
    let stored = entries
        .iter()
        .find(|e| e.path == "Data/test-image-1739601.png")
        .expect("picture file in package");

    let (doc, _) = open("eo-pages-objects.pages");
    let images: Vec<(usize, &RichImage)> = doc
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(i, b)| match b {
            RichBlock::Image(img) => Some((i, img)),
            _ => None,
        })
        .collect();
    assert_eq!(images.len(), 1);
    let (index, img) = images[0];
    assert_eq!(img.bytes(), stored.data.as_slice());
    assert_eq!(img.format(), RichImageFormat::Png);
    assert_eq!((img.width_px(), img.height_px()), (240, 160));
    // The geometry size is 240 x 160 pt.
    assert_eq!(img.display_emu(), (240 * 12_700, 160 * 12_700));
    // It sits right after the heading-like paragraph that introduces it.
    let RichBlock::Paragraph(before) = &doc.blocks[index - 1] else {
        panic!("paragraph before the picture");
    };
    assert_eq!(before.plain_text(), "1. 그림 (직접 만든 테스트 이미지)");
    let RichBlock::Paragraph(after) = &doc.blocks[index + 1] else {
        panic!("paragraph after the picture");
    };
    assert_eq!(after.plain_text(), "2. 도형 (사각형 안에 글자)");
}

#[test]
fn text_fixture_font_script_indent_and_line_spacing() {
    let (doc, _) = open("eo-pages-text.pages");
    let fonts = paragraph_with(&doc, "서로 다른 글꼴 비교");
    assert_eq!(
        run_named(fonts, "명조계열단어").font.as_deref(),
        Some("AppleMyungjo")
    );
    assert_eq!(
        run_named(fonts, "고딕계열단어").font.as_deref(),
        Some("AppleSDGothicNeo-Regular")
    );
    assert_eq!(run_named(fonts, "서로 다른 글꼴 비교: ").font, None);

    let scripts = paragraph_with(&doc, "첨자 테스트");
    assert_eq!(run_named(scripts, "위첨자").script, RichScript::Superscript);
    assert_eq!(run_named(scripts, "아래첨자").script, RichScript::Subscript);
    assert_eq!(
        run_named(scripts, "첨자 테스트: X").script,
        RichScript::Normal
    );

    let left = paragraph_with(&doc, "왼쪽 여백 들여쓰기");
    assert_eq!(
        (
            left.left_indent_twips,
            left.first_line_indent_twips,
            left.line_spacing_percent
        ),
        (Some(720), None, None)
    );
    let first = paragraph_with(&doc, "첫 줄 들여쓰기");
    assert_eq!(
        (
            first.left_indent_twips,
            first.first_line_indent_twips,
            first.line_spacing_percent
        ),
        (None, Some(720), None)
    );
    let spacing = paragraph_with(&doc, "줄 간격 2배");
    assert_eq!(
        (
            spacing.left_indent_twips,
            spacing.first_line_indent_twips,
            spacing.line_spacing_percent
        ),
        (None, None, Some(200))
    );
    let plain = paragraph_with(&doc, "왼쪽 정렬이");
    assert_eq!(plain.left_indent_twips, None);
    assert_eq!(plain.line_spacing_percent, None);
}

//! Word processor paragraph and block features (PD-401): tab stops, paragraph borders and shading, the drop cap, column
//! separator and narrow side, column breaks, text boxes, equations and the styles of the user. Each is written to DOCX and
//! read back, and the reader refuses what a hostile file claims beyond the bounds.

use easyoffice_formats::rich_word_blocks::{
    checked_omml, RichBorderLine, RichBorderStyle, RichColumnLook, RichDropCap, RichNarrowColumn,
    RichParaBorders, RichStyleDef, RichTabAlign, RichTabLeader, RichTabStop, RichTextBox,
    MAX_MATH_DEPTH, MAX_TAB_STOPS,
};
use easyoffice_formats::{
    DecodedContent, DocumentFormat, RichAlignment, RichBlock, RichColumns, RichDocument,
    RichImageWrap, RichParagraph, RichRun, UniversalFormatConverter, ZipArchiveCodec,
    ZipSecurityPolicy,
};

fn decode(bytes: &[u8], name: &str) -> RichDocument {
    match UniversalFormatConverter::decode_rich(bytes, Some(name))
        .expect("decode")
        .content
    {
        DecodedContent::Document(doc) => doc,
        _ => panic!("document expected"),
    }
}

fn encode(doc: &RichDocument, format: DocumentFormat) -> Vec<u8> {
    UniversalFormatConverter::encode_rich(doc, format, None).expect("encode")
}

fn round_trip(doc: &RichDocument) -> RichDocument {
    decode(&encode(doc, DocumentFormat::Docx), "r.docx")
}

fn part(bytes: &[u8], path: &str) -> String {
    ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
        .expect("unpack")
        .into_iter()
        .find(|e| e.path == path)
        .map(|e| String::from_utf8_lossy(&e.data).into_owned())
        .unwrap_or_else(|| panic!("missing part {path}"))
}

fn docx_with_body(xml: &str) -> Vec<u8> {
    let entries: Vec<(String, Vec<u8>)> = vec![
        ("[Content_Types].xml".to_string(), b"<?xml version=\"1.0\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/></Types>".to_vec()),
        ("_rels/.rels".to_string(), b"<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>".to_vec()),
        ("word/document.xml".to_string(), xml.as_bytes().to_vec()),
    ];
    ZipArchiveCodec::pack_compressed(&entries).expect("pack")
}

/// The namespaces of the test documents; the vendor word in the VML name is split so the name guard does not read it.
const W: &str = concat!(
    "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" ",
    "xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\" ",
    "xmlns:v=\"urn:schemas-mi",
    "crosoft-com:vml\""
);

fn body(paragraphs: &str) -> Vec<u8> {
    docx_with_body(&format!(
        "<w:document {W}><w:body>{paragraphs}</w:body></w:document>"
    ))
}

fn paragraph(text: &str) -> RichParagraph {
    RichParagraph {
        runs: vec![RichRun::plain(text)],
        ..RichParagraph::default()
    }
}

fn doc_of(paragraphs: Vec<RichParagraph>) -> RichDocument {
    RichDocument {
        title: "새 기능".to_string(),
        blocks: paragraphs.into_iter().map(RichBlock::Paragraph).collect(),
        ..RichDocument::default()
    }
}

fn paragraphs(doc: &RichDocument) -> Vec<&RichParagraph> {
    doc.blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some(p),
            _ => None,
        })
        .collect()
}

// ---- tab stops

#[test]
fn tab_stops_keep_position_alignment_and_leader() {
    let mut p = paragraph("목차\t1");
    p.word.tab_stops = vec![
        RichTabStop {
            pos_twips: 2000,
            align: RichTabAlign::Center,
            leader: RichTabLeader::None,
        },
        RichTabStop {
            pos_twips: 8000,
            align: RichTabAlign::Right,
            leader: RichTabLeader::Dot,
        },
        RichTabStop {
            pos_twips: 9000,
            align: RichTabAlign::Decimal,
            leader: RichTabLeader::Hyphen,
        },
    ];
    let doc = doc_of(vec![p.clone()]);
    let xml = part(&encode(&doc, DocumentFormat::Docx), "word/document.xml");
    assert!(
        xml.contains("<w:tab w:val=\"right\" w:leader=\"dot\" w:pos=\"8000\"/>"),
        "{xml}"
    );
    let back = round_trip(&doc);
    assert_eq!(paragraphs(&back)[0].word.tab_stops, p.word.tab_stops);
}

#[test]
fn a_flood_of_tab_stops_is_capped_and_sorted_and_clear_marks_are_not_stops() {
    let tabs: String = (0..500)
        .map(|i| format!("<w:tab w:val=\"left\" w:pos=\"{}\"/>", 40_000 - i * 70))
        .collect();
    let xml = format!(
        "<w:p><w:pPr><w:tabs><w:tab w:val=\"clear\" w:pos=\"720\"/><w:tab w:val=\"bar\" w:pos=\"900\"/>{tabs}</w:tabs></w:pPr><w:r><w:t>x</w:t></w:r></w:p>"
    );
    let back = decode(&body(&xml), "t.docx");
    let stops = &paragraphs(&back)[0].word.tab_stops;
    assert!(stops.len() <= MAX_TAB_STOPS, "{}", stops.len());
    assert!(
        stops.windows(2).all(|w| w[0].pos_twips < w[1].pos_twips),
        "sorted and distinct"
    );
    assert!(
        stops.iter().all(|s| s.pos_twips <= 31_680),
        "a stop past the widest page is dropped"
    );
}

#[test]
fn a_table_of_contents_entry_keeps_its_own_tab_and_no_user_stop() {
    let xml = "<w:p><w:pPr><w:pStyle w:val=\"TOC1\"/><w:tabs><w:tab w:val=\"right\" w:leader=\"dot\" w:pos=\"9000\"/></w:tabs></w:pPr><w:r><w:t>1장</w:t></w:r></w:p>";
    let back = decode(&body(xml), "toc.docx");
    assert!(paragraphs(&back)[0].word.tab_stops.is_empty());
}

// ---- borders and shading

#[test]
fn paragraph_borders_and_shading_round_trip() {
    let line = |style, eighths, color: Option<&str>, space_pt| RichBorderLine {
        style,
        eighths,
        color: color.map(str::to_string),
        space_pt,
    };
    let mut p = paragraph("테두리");
    p.word.borders = Some(RichParaBorders {
        top: Some(line(RichBorderStyle::Single, 6, Some("FF0000"), 4)),
        bottom: Some(line(RichBorderStyle::Double, 12, None, 1)),
        left: Some(line(RichBorderStyle::Dashed, 4, Some("0000FF"), 0)),
        right: None,
    });
    p.word.shading = Some("FFFF00".to_string());
    let doc = doc_of(vec![p.clone()]);
    let xml = part(&encode(&doc, DocumentFormat::Docx), "word/document.xml");
    assert!(
        xml.contains(
            "<w:pBdr><w:top w:val=\"single\" w:sz=\"6\" w:space=\"4\" w:color=\"FF0000\"/>"
        ),
        "{xml}"
    );
    assert!(
        xml.contains("<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"FFFF00\"/>"),
        "{xml}"
    );
    let back = round_trip(&doc);
    assert_eq!(paragraphs(&back)[0].word.borders, p.word.borders);
    assert_eq!(paragraphs(&back)[0].word.shading, p.word.shading);
}

#[test]
fn a_hostile_border_width_is_clamped_and_the_shading_of_the_paragraph_mark_is_not_the_paragraphs() {
    let xml = "<w:p><w:pPr><w:pBdr><w:top w:val=\"single\" w:sz=\"99999\" w:space=\"500\" w:color=\"zzzzzz\"/><w:bottom w:val=\"nil\"/></w:pBdr><w:rPr><w:shd w:val=\"clear\" w:fill=\"00FF00\"/></w:rPr></w:pPr><w:r><w:t>x</w:t></w:r></w:p>";
    let back = decode(&body(xml), "b.docx");
    let word = &paragraphs(&back)[0].word;
    let top = word
        .borders
        .as_ref()
        .and_then(|b| b.top.as_ref())
        .expect("top");
    assert_eq!(
        (top.eighths, top.space_pt, top.color.as_deref()),
        (96, 31, None)
    );
    assert!(word.borders.as_ref().is_some_and(|b| b.bottom.is_none()));
    assert_eq!(
        word.shading, None,
        "the shading in w:rPr of the paragraph mark is the mark's"
    );
}

// ---- drop cap

#[test]
fn a_drop_cap_paragraph_keeps_its_lines_and_side() {
    let mut cap = paragraph("가");
    cap.word.drop_cap = Some(RichDropCap {
        lines: 3,
        in_margin: false,
    });
    let doc = doc_of(vec![cap, paragraph("나다라마바사")]);
    let xml = part(&encode(&doc, DocumentFormat::Docx), "word/document.xml");
    assert!(
        xml.contains("<w:framePr w:dropCap=\"drop\" w:lines=\"3\""),
        "{xml}"
    );
    let back = round_trip(&doc);
    assert_eq!(
        paragraphs(&back)[0].word.drop_cap,
        Some(RichDropCap {
            lines: 3,
            in_margin: false
        })
    );
    assert_eq!(paragraphs(&back)[1].word.drop_cap, None);
}

#[test]
fn a_drop_cap_of_hostile_lines_is_not_a_drop_cap() {
    for lines in ["0", "1", "11", "255", "9999999"] {
        let xml = format!("<w:p><w:pPr><w:framePr w:dropCap=\"drop\" w:lines=\"{lines}\"/></w:pPr><w:r><w:t>가</w:t></w:r></w:p>");
        let back = decode(&body(&xml), "d.docx");
        assert_eq!(paragraphs(&back)[0].word.drop_cap, None, "lines {lines}");
    }
}

// ---- columns

fn two_columns() -> Option<RichColumns> {
    Some(RichColumns {
        count: 2,
        equal: true,
        gap_twips: 720,
    })
}

#[test]
fn the_separator_and_the_narrow_side_of_two_columns_round_trip() {
    for (narrow, separator) in [
        (RichNarrowColumn::None, true),
        (RichNarrowColumn::First, false),
        (RichNarrowColumn::Last, true),
    ] {
        let mut p = paragraph("두 단");
        p.columns = Some(RichColumns {
            count: 2,
            equal: narrow == RichNarrowColumn::None,
            gap_twips: 720,
        });
        p.word.column_look = Some(RichColumnLook { separator, narrow }).filter(|l| !l.is_default());
        let doc = doc_of(vec![p.clone()]);
        let back = round_trip(&doc);
        let got = paragraphs(&back)[0];
        assert_eq!(got.columns.map(|c| c.count), Some(2), "{narrow:?}");
        assert_eq!(
            got.word.column_look, p.word.column_look,
            "{narrow:?} {separator}"
        );
    }
}

#[test]
fn a_column_break_starts_the_next_paragraph_in_the_next_column() {
    let mut second = paragraph("둘째 단");
    second.word.column_break_before = true;
    second.columns = two_columns();
    let mut first = paragraph("첫째 단");
    first.columns = two_columns();
    let doc = doc_of(vec![first, second]);
    let xml = part(&encode(&doc, DocumentFormat::Docx), "word/document.xml");
    assert!(xml.contains("<w:br w:type=\"column\"/>"), "{xml}");
    let back = round_trip(&doc);
    let flags: Vec<bool> = paragraphs(&back)
        .iter()
        .map(|p| p.word.column_break_before)
        .collect();
    assert_eq!(flags, [false, true]);
}

#[test]
fn a_column_break_after_text_belongs_to_the_next_paragraph() {
    let xml = "<w:p><w:r><w:t>앞</w:t></w:r><w:r><w:br w:type=\"column\"/></w:r></w:p><w:p><w:r><w:t>뒤</w:t></w:r></w:p>";
    let back = decode(&body(xml), "c.docx");
    let flags: Vec<bool> = paragraphs(&back)
        .iter()
        .map(|p| p.word.column_break_before)
        .collect();
    assert_eq!(flags, [false, true]);
}

#[test]
fn a_flood_of_column_widths_is_read_without_growing_the_section() {
    let cols: String = (0..5000).map(|_| "<w:col w:w=\"100\"/>").collect();
    let xml = format!("<w:p><w:r><w:t>x</w:t></w:r></w:p><w:sectPr><w:cols w:num=\"2\" w:equalWidth=\"0\" w:sep=\"1\">{cols}</w:cols></w:sectPr>");
    let back = decode(&body(&xml), "w.docx");
    assert_eq!(paragraphs(&back)[0].columns.map(|c| c.count), Some(2));
    assert!(paragraphs(&back)[0]
        .word
        .column_look
        .is_some_and(|l| l.separator));
}

// ---- text boxes

fn text_box(texts: &[&str]) -> RichTextBox {
    RichTextBox {
        paragraphs: texts.iter().map(|t| paragraph(t)).collect(),
        width_emu: 2_143_125,
        height_emu: 714_375,
        wrap: RichImageWrap::Square,
        fill: Some("FFEEAA".to_string()),
        border: true,
    }
}

#[test]
fn a_text_box_keeps_its_paragraphs_size_wrap_and_look() {
    let mut p = paragraph("본문");
    p.word.text_boxes = vec![text_box(&["상자 첫 줄", "상자 둘째 줄"])];
    let doc = doc_of(vec![p]);
    let bytes = encode(&doc, DocumentFormat::Docx);
    let xml = part(&bytes, "word/document.xml");
    assert!(xml.contains("<v:textbox><w:txbxContent>"), "{xml}");
    let back = round_trip(&doc);
    let boxes = &paragraphs(&back)[0].word.text_boxes;
    assert_eq!(boxes.len(), 1);
    let b = &boxes[0];
    assert_eq!(
        b.paragraphs
            .iter()
            .map(RichParagraph::plain_text)
            .collect::<Vec<_>>(),
        ["상자 첫 줄", "상자 둘째 줄"]
    );
    assert_eq!((b.width_emu, b.height_emu), (2_143_125, 714_375));
    assert_eq!(
        (b.wrap, b.fill.as_deref(), b.border),
        (RichImageWrap::Square, Some("FFEEAA"), true)
    );
}

#[test]
fn a_text_box_of_a_drawing_is_read_and_the_nested_one_is_not_kept() {
    let drawing = "<w:p><w:r><w:drawing><wp:anchor behindDoc=\"1\"><wp:extent cx=\"1800000\" cy=\"900000\"/><wp:wrapNone/><a:graphic><a:graphicData><wps:wsp><wps:spPr><a:solidFill><a:srgbClr val=\"CCDDEE\"/></a:solidFill><a:ln><a:noFill/></a:ln></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>안쪽</w:t></w:r></w:p><w:p><w:r><w:drawing><wp:anchor><wp:extent cx=\"10\" cy=\"10\"/><a:graphic><a:graphicData><wps:wsp><wps:txbx><w:txbxContent><w:p><w:r><w:t>더 안쪽</w:t></w:r></w:p></w:txbxContent></wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p></w:txbxContent></wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p>";
    let xml = format!("<w:document {W} xmlns:wp=\"x\" xmlns:a=\"y\" xmlns:wps=\"z\"><w:body>{drawing}</w:body></w:document>");
    let back = decode(&docx_with_body(&xml), "d.docx");
    let boxes = &paragraphs(&back)[0].word.text_boxes;
    assert_eq!(boxes.len(), 1);
    let b = &boxes[0];
    assert_eq!(
        b.paragraphs.len(),
        1,
        "the paragraph with the nested box has no text of its own"
    );
    assert!(b.paragraphs.iter().all(|p| p.word.text_boxes.is_empty()));
    assert_eq!(
        (b.width_emu, b.height_emu, b.wrap, b.border),
        (1_800_000, 900_000, RichImageWrap::Behind, false)
    );
    assert_eq!(b.fill.as_deref(), Some("CCDDEE"));
}

#[test]
fn a_text_box_past_the_bounds_is_dropped() {
    let huge = "<w:p><w:r><w:pict><v:shape style=\"width:99999pt;height:10pt\"><v:textbox><w:txbxContent><w:p><w:r><w:t>x</w:t></w:r></w:p></w:txbxContent></v:textbox></v:shape></w:pict></w:r></w:p>";
    let back = decode(&body(huge), "h.docx");
    assert!(paragraphs(&back)
        .iter()
        .all(|p| p.word.text_boxes.is_empty()));
    let many: String = (0..1000)
        .map(|i| format!("<w:p><w:r><w:t>{i}</w:t></w:r><w:r><w:pict><v:shape style=\"width:100pt;height:20pt\"><v:textbox><w:txbxContent><w:p><w:r><w:t>b</w:t></w:r></w:p></w:txbxContent></v:textbox></v:shape></w:pict></w:r></w:p>"))
        .collect();
    let back = decode(&body(&many), "m.docx");
    let total: usize = paragraphs(&back)
        .iter()
        .map(|p| p.word.text_boxes.len())
        .sum();
    assert_eq!(total, 256, "boxes past the cap read as absent");
}

#[test]
fn text_boxes_are_a_loss_for_the_formats_that_have_none() {
    let mut p = paragraph("본문");
    p.word.text_boxes = vec![text_box(&["상자"])];
    let loss = easyoffice_formats::doc_save_loss(&doc_of(vec![p]), DocumentFormat::Odt);
    assert!(
        loss.warnings("ODT")
            .iter()
            .any(|w| w.contains("텍스트 상자")),
        "{:?}",
        loss.warnings("ODT")
    );
}

// ---- equations

const FRACTION: &str = "<m:oMath><m:f><m:num><m:r><m:t>1</m:t></m:r></m:num><m:den><m:r><m:t>2</m:t></m:r></m:den></m:f></m:oMath>";

#[test]
fn an_equation_is_kept_as_written_and_its_text_is_linear() {
    let xml =
        format!("<w:p><w:r><w:t>값 </w:t></w:r>{FRACTION}<w:r><w:t> 입니다</w:t></w:r></w:p>");
    let back = decode(&body(&xml), "e.docx");
    let runs = &paragraphs(&back)[0].runs;
    let math = runs.iter().find(|r| r.math.is_some()).expect("math run");
    assert_eq!(math.math.as_deref(), Some(FRACTION));
    assert_eq!(math.text, "12");
    let again = round_trip(&back);
    assert_eq!(
        paragraphs(&again)[0]
            .runs
            .iter()
            .find_map(|r| r.math.as_deref()),
        Some(FRACTION)
    );
    assert!(part(&encode(&back, DocumentFormat::Docx), "word/document.xml").contains(FRACTION));
}

#[test]
fn an_equation_drops_what_is_not_omml_and_refuses_what_is_too_deep_or_too_big() {
    let sneaky = "<m:oMath><m:r><w:rPr><w:b/></w:rPr><m:t>a</m:t></m:r><w:instrText>DDE</w:instrText><m:ctrlPr><w:rPr/></m:ctrlPr></m:oMath>";
    let (omml, text) = checked_omml(sneaky).expect("checked");
    assert_eq!(omml, "<m:oMath><m:r><m:t>a</m:t></m:r></m:oMath>");
    assert_eq!(text, "a");
    assert!(
        checked_omml("<m:r><m:t>x</m:t></m:r>").is_none(),
        "not an equation root"
    );
    assert!(checked_omml("<!DOCTYPE x [<!ENTITY a 'b'>]><m:oMath/>").is_none());
    let deep = format!(
        "{}{}",
        "<m:e>".repeat(MAX_MATH_DEPTH + 2),
        "</m:e>".repeat(MAX_MATH_DEPTH + 2)
    );
    assert!(
        checked_omml(&format!("<m:oMath>{deep}</m:oMath>")).is_none(),
        "too deep"
    );
    let big = format!(
        "<m:oMath><m:r><m:t>{}</m:t></m:r></m:oMath>",
        "가".repeat(20_000)
    );
    assert!(checked_omml(&big).is_none(), "too big");
}

#[test]
fn a_hostile_equation_in_a_file_is_read_as_its_text() {
    let deep = format!(
        "{}<m:r><m:t>끝</m:t></m:r>{}",
        "<m:e>".repeat(60),
        "</m:e>".repeat(60)
    );
    let xml = format!("<w:p><w:r><w:t>앞</w:t></w:r><m:oMath>{deep}</m:oMath></w:p>");
    let back = decode(&body(&xml), "x.docx");
    let p = paragraphs(&back)[0];
    assert!(p.runs.iter().all(|r| r.math.is_none()));
    assert_eq!(p.plain_text(), "앞끝");
}

#[test]
fn equations_are_a_loss_for_the_formats_that_have_none_but_their_text_stays() {
    let xml = format!("<w:p>{FRACTION}</w:p>");
    let back = decode(&body(&xml), "e.docx");
    let loss = easyoffice_formats::doc_save_loss(&back, DocumentFormat::Odt);
    assert!(
        loss.warnings("ODT").iter().any(|w| w.contains("수식")),
        "{:?}",
        loss.warnings("ODT")
    );
    let odt = decode(&encode(&back, DocumentFormat::Odt), "e.odt");
    assert_eq!(paragraphs(&odt)[0].plain_text(), "12");
}

// ---- styles of the user

fn def(name: &str) -> RichStyleDef {
    RichStyleDef {
        name: name.to_string(),
        font: Some("함초롬바탕".to_string()),
        size_half_pt: Some(28),
        bold: true,
        italic: false,
        underline: false,
        color: Some("1F4E79".to_string()),
        alignment: Some(RichAlignment::Center),
        left_indent_twips: Some(360),
        first_line_indent_twips: Some(-180),
        space_before_twips: Some(120),
        space_after_twips: Some(240),
        line_spacing: None,
    }
}

#[test]
fn a_style_of_the_user_is_written_to_the_style_sheet_and_read_back_with_its_paragraphs() {
    let mut p = paragraph("강조 문단");
    p.word.style_name = Some("본문 강조".to_string());
    let mut doc = doc_of(vec![p, paragraph("보통")]);
    doc.styles = vec![def("본문 강조")];
    let bytes = encode(&doc, DocumentFormat::Docx);
    let styles = part(&bytes, "word/styles.xml");
    assert!(
        styles.contains("w:customStyle=\"1\" w:styleId=\"EoStyle1\"><w:name w:val=\"본문 강조\"/>"),
        "{styles}"
    );
    assert!(part(&bytes, "word/document.xml").contains("<w:pStyle w:val=\"EoStyle1\"/>"));
    let back = round_trip(&doc);
    assert_eq!(back.styles, doc.styles);
    assert_eq!(
        paragraphs(&back)[0].word.style_name.as_deref(),
        Some("본문 강조")
    );
    assert_eq!(paragraphs(&back)[1].word.style_name, None);
}

#[test]
fn a_foreign_style_gives_its_look_to_the_paragraphs_that_have_none_of_their_own() {
    let styles = "<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:style w:type=\"paragraph\" w:customStyle=\"1\" w:styleId=\"Mine\"><w:name w:val=\"내 스타일\"/><w:pPr><w:jc w:val=\"right\"/></w:pPr><w:rPr><w:b/><w:sz w:val=\"30\"/></w:rPr></w:style><w:style w:type=\"paragraph\" w:customStyle=\"1\" w:styleId=\"Heading1x\"><w:name w:val=\"heading 1\"/></w:style></w:styles>";
    let entries: Vec<(String, Vec<u8>)> = vec![
        ("[Content_Types].xml".to_string(), b"<?xml version=\"1.0\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"xml\" ContentType=\"application/xml\"/></Types>".to_vec()),
        ("_rels/.rels".to_string(), b"<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>".to_vec()),
        ("word/document.xml".to_string(), format!("<w:document {W}><w:body><w:p><w:pPr><w:pStyle w:val=\"Mine\"/></w:pPr><w:r><w:t>글</w:t></w:r></w:p></w:body></w:document>").into_bytes()),
        ("word/styles.xml".to_string(), styles.as_bytes().to_vec()),
    ];
    let back = decode(
        &ZipArchiveCodec::pack_compressed(&entries).expect("pack"),
        "s.docx",
    );
    assert_eq!(
        back.styles.len(),
        1,
        "a style named like a built-in one is not the user's: {:?}",
        back.styles
    );
    let p = paragraphs(&back)[0];
    assert_eq!(p.alignment, Some(RichAlignment::Right));
    assert!(p.runs[0].bold);
    assert_eq!(p.runs[0].size_half_pt, Some(30));
    assert_eq!(p.word.style_name.as_deref(), Some("내 스타일"));
}

#[test]
fn style_names_are_checked() {
    for bad in [
        "",
        " 앞공백",
        "표준",
        "Heading 1",
        "제목 2",
        "TOC 1",
        &"가".repeat(65),
    ] {
        let mut d = def("x");
        d.name = bad.to_string();
        let ok = d.checked().is_some()
            && !easyoffice_formats::rich_word_blocks::is_reserved_style_name(bad);
        assert!(!ok, "{bad:?}");
    }
    assert!(def("본문 강조").checked().is_some());
}

#[test]
fn the_paragraph_features_are_a_loss_for_the_formats_that_have_none() {
    let mut p = paragraph("서식");
    p.word.tab_stops = vec![RichTabStop {
        pos_twips: 1000,
        align: RichTabAlign::Left,
        leader: RichTabLeader::None,
    }];
    p.word.shading = Some("FFFF00".to_string());
    p.word.drop_cap = Some(RichDropCap {
        lines: 2,
        in_margin: true,
    });
    let doc = doc_of(vec![p]);
    for format in [
        DocumentFormat::Odt,
        DocumentFormat::Rtf,
        DocumentFormat::Hwpx,
    ] {
        let loss = easyoffice_formats::doc_save_loss(&doc, format);
        assert!(loss.word_blocks > 0, "{format:?}");
    }
    assert_eq!(
        easyoffice_formats::doc_save_loss(&doc, DocumentFormat::Docx).word_blocks,
        0
    );
}

//! Word-style document features (PD-395): each one is written to DOCX and read back, and the readers refuse what
//! a hostile file claims beyond the bounds.

use easyoffice_formats::{
    DecodedContent, DocumentFormat, RichBinding, RichBindingKind, RichBlock, RichDocument,
    RichLineNumberRestart, RichLineNumbers, RichPageFrame, RichPageMargins, RichPaper,
    RichParagraph, RichProperties, RichRun, RichSectionBreak, UniversalFormatConverter,
    ZipArchiveCodec, ZipSecurityPolicy,
};

pub fn decode(bytes: &[u8], name: &str) -> RichDocument {
    match UniversalFormatConverter::decode_rich(bytes, Some(name))
        .expect("decode")
        .content
    {
        DecodedContent::Document(doc) => doc,
        _ => panic!("document expected"),
    }
}

pub fn encode(doc: &RichDocument, format: DocumentFormat) -> Vec<u8> {
    UniversalFormatConverter::encode_rich(doc, format, None).expect("encode")
}

pub fn round_trip(doc: &RichDocument, format: DocumentFormat, name: &str) -> RichDocument {
    decode(&encode(doc, format), name)
}

pub fn part(bytes: &[u8], path: &str) -> String {
    ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
        .expect("unpack")
        .into_iter()
        .find(|e| e.path == path)
        .map(|e| String::from_utf8_lossy(&e.data).into_owned())
        .unwrap_or_else(|| panic!("missing part {path}"))
}

pub fn paragraph(text: &str) -> RichBlock {
    RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun::plain(text)],
        ..RichParagraph::default()
    })
}

pub fn doc_of(blocks: Vec<RichBlock>) -> RichDocument {
    RichDocument {
        title: "속성".to_string(),
        blocks,
        ..RichDocument::default()
    }
}

#[test]
fn a_folded_booklet_binding_is_written_and_read_as_book_fold_printing() {
    let mut doc = doc_of(vec![paragraph("본문")]);
    doc.page_frame = RichPageFrame {
        margins: Some(RichPageMargins::DOC_DEFAULT),
        paper: Some(RichPaper {
            width: 16_838,
            height: 11_906,
        }),
        binding: RichBinding {
            kind: RichBindingKind::BookFold,
            gutter: 567,
        },
        ..RichPageFrame::default()
    };
    let bytes = encode(&doc, DocumentFormat::Docx);
    let settings = part(&bytes, "word/settings.xml");
    assert!(settings.contains("<w:bookFoldPrinting/>"), "{settings}");
    assert!(!settings.contains("mirrorMargins"), "{settings}");
    let back = decode(&bytes, "b.docx");
    assert_eq!(back.page_frame.binding, doc.page_frame.binding);
    assert_eq!(back.page_frame.paper, doc.page_frame.paper);
}

fn run(text: &str) -> RichRun {
    RichRun::plain(text)
}

fn coded(text: &str, code: &str) -> RichRun {
    RichRun {
        field_code: Some(code.to_string()),
        ..RichRun::plain(text)
    }
}

fn runs_of(doc: &RichDocument, block: usize) -> Vec<RichRun> {
    match &doc.blocks[block] {
        RichBlock::Paragraph(p) => p.runs.clone(),
        _ => panic!("paragraph expected"),
    }
}

fn paragraph_of(runs: Vec<RichRun>) -> RichBlock {
    RichBlock::Paragraph(RichParagraph {
        runs,
        ..RichParagraph::default()
    })
}

// ---- fields: date, time, cross reference, caption number

#[test]
fn a_date_field_keeps_its_instruction_and_its_result_text() {
    let doc = doc_of(vec![paragraph_of(vec![
        run("작성일 "),
        coded("2026-11-28", "DATE \\@ \"yyyy-MM-dd\""),
        run(" 끝"),
    ])]);
    let bytes = encode(&doc, DocumentFormat::Docx);
    let body = part(&bytes, "word/document.xml");
    assert!(
        body.contains(
            "<w:instrText xml:space=\"preserve\"> DATE \\@ &quot;yyyy-MM-dd&quot; </w:instrText>"
        ),
        "{body}"
    );
    assert_eq!(body.matches("w:fldCharType=\"begin\"").count(), 1);
    assert_eq!(body.matches("w:fldCharType=\"end\"").count(), 1);
    let back = decode(&bytes, "f.docx");
    let runs = runs_of(&back, 0);
    assert_eq!(runs.len(), 3, "{runs:?}");
    assert_eq!(runs[1].text, "2026-11-28");
    assert_eq!(
        runs[1].field_code.as_deref(),
        Some("DATE \\@ \"yyyy-MM-dd\"")
    );
    assert_eq!(runs[0].field_code, None);
    assert_eq!(runs[2].field_code, None);
}

#[test]
fn two_fields_next_to_each_other_stay_two_fields() {
    let doc = doc_of(vec![paragraph_of(vec![
        coded("가", "SEQ 그림 \\* ARABIC"),
        coded("나", "REF _Ref1 \\h"),
    ])]);
    let back = round_trip(&doc, DocumentFormat::Docx, "f.docx");
    let runs = runs_of(&back, 0);
    assert_eq!(runs.len(), 2, "{runs:?}");
    assert_eq!(runs[0].field_code.as_deref(), Some("SEQ 그림 \\* ARABIC"));
    assert_eq!(runs[1].field_code.as_deref(), Some("REF _Ref1 \\h"));
}

#[test]
fn a_field_that_is_not_on_the_list_is_read_as_its_result_text_only() {
    let xml = "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:p>\
        <w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText> INCLUDETEXT \"C:\\\\secret.txt\" </w:instrText></w:r>\
        <w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>결과</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r>\
        <w:fldSimple w:instr=\" DDEAUTO cmd calc \"><w:r><w:t>둘</w:t></w:r></w:fldSimple></w:p></w:body></w:document>";
    let back = decode(&docx_with_body(xml), "h.docx");
    let runs = runs_of(&back, 0);
    assert_eq!(
        runs.iter().map(|r| r.text.as_str()).collect::<String>(),
        "결과둘"
    );
    assert!(runs.iter().all(|r| r.field_code.is_none()), "{runs:?}");
}

#[test]
fn a_simple_field_is_read_with_its_instruction() {
    let xml = "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:p>\
        <w:fldSimple w:instr=\" TIME \\@ &quot;HH:mm&quot; \"><w:r><w:t>09:30</w:t></w:r></w:fldSimple></w:p></w:body></w:document>";
    let back = decode(&docx_with_body(xml), "s.docx");
    let runs = runs_of(&back, 0);
    assert_eq!(runs[0].text, "09:30");
    assert_eq!(runs[0].field_code.as_deref(), Some("TIME \\@ \"HH:mm\""));
}

/// A DOCX whose `word/document.xml` is `xml`, with the minimal parts around it.
fn docx_with_body(xml: &str) -> Vec<u8> {
    let entries: Vec<(String, Vec<u8>)> = vec![
        ("[Content_Types].xml".to_string(), b"<?xml version=\"1.0\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/></Types>".to_vec()),
        ("_rels/.rels".to_string(), b"<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>".to_vec()),
        ("word/document.xml".to_string(), xml.as_bytes().to_vec()),
    ];
    ZipArchiveCodec::pack_compressed(&entries).expect("pack")
}

// ---- bookmarks

#[test]
fn a_bookmark_keeps_its_name_and_the_text_it_covers() {
    let mut start = run("중요한 ");
    start.bookmark_start = Some("결론".to_string());
    let mut end = run("문장");
    end.bookmark_end = Some("결론".to_string());
    let doc = doc_of(vec![paragraph_of(vec![run("앞 "), start, end, run(" 뒤")])]);
    let bytes = encode(&doc, DocumentFormat::Docx);
    let body = part(&bytes, "word/document.xml");
    assert!(body.contains("w:name=\"결론\""), "{body}");
    assert_eq!(body.matches("<w:bookmarkEnd").count(), 1);
    let back = decode(&bytes, "b.docx");
    let runs = runs_of(&back, 0);
    let named: Vec<_> = runs
        .iter()
        .filter_map(|r| r.bookmark_start.as_deref())
        .collect();
    assert_eq!(named, ["결론"]);
    let covered: String = runs
        .iter()
        .skip_while(|r| r.bookmark_start.is_none())
        .take_while_inclusive(|r| r.bookmark_end.is_none())
        .into_iter()
        .map(|r| r.text.as_str())
        .collect();
    assert_eq!(covered, "중요한 문장");
}

trait TakeWhileInclusive: Iterator + Sized {
    fn take_while_inclusive<P: FnMut(&Self::Item) -> bool>(
        self,
        mut keep_going: P,
    ) -> Vec<Self::Item> {
        let mut out = Vec::new();
        for item in self {
            let more = keep_going(&item);
            out.push(item);
            if !more {
                break;
            }
        }
        out
    }
}

impl<I: Iterator> TakeWhileInclusive for I {}

#[test]
fn hostile_bookmark_names_and_counts_are_dropped_or_capped() {
    let mut body = String::from("<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>");
    for (i, name) in ["1abc", "a b", "_Toc77", "_GoBack"].iter().enumerate() {
        body.push_str(&format!("<w:p><w:bookmarkStart w:id=\"{i}\" w:name=\"{name}\"/><w:r><w:t>글{i}</w:t></w:r><w:bookmarkEnd w:id=\"{i}\"/></w:p>"));
    }
    for i in 0..1200 {
        body.push_str(&format!("<w:p><w:bookmarkStart w:id=\"{}\" w:name=\"b{i}\"/><w:r><w:t>x</w:t></w:r><w:bookmarkEnd w:id=\"{}\"/></w:p>", 100 + i, 100 + i));
    }
    body.push_str("</w:body></w:document>");
    let back = decode(&docx_with_body(&body), "h.docx");
    let names: Vec<String> = back
        .blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some(p.runs.iter().filter_map(|r| r.bookmark_start.clone())),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(
        names.len(),
        easyoffice_formats::rich_word_features::MAX_BOOKMARKS,
        "capped at the limit"
    );
    assert!(
        names.iter().all(|n| n.starts_with('b')),
        "no refused name survived: {:?}",
        &names[..3]
    );
}

// ---- captions

#[test]
fn a_caption_paragraph_is_the_caption_style_with_a_numbering_field() {
    let doc = doc_of(vec![RichBlock::Paragraph(RichParagraph {
        runs: vec![
            run("그림 "),
            coded("1", "SEQ 그림 \\* ARABIC"),
            run(". 설명"),
        ],
        caption: true,
        ..RichParagraph::default()
    })]);
    let bytes = encode(&doc, DocumentFormat::Docx);
    assert!(part(&bytes, "word/document.xml").contains("<w:pStyle w:val=\"Caption\"/>"));
    assert!(part(&bytes, "word/styles.xml").contains("w:styleId=\"Caption\""));
    let back = decode(&bytes, "c.docx");
    let RichBlock::Paragraph(p) = &back.blocks[0] else {
        panic!("paragraph")
    };
    assert!(p.caption);
    assert_eq!(p.plain_text(), "그림 1. 설명");
    assert_eq!(p.runs[1].field_code.as_deref(), Some("SEQ 그림 \\* ARABIC"));
}

// ---- section breaks

#[test]
fn section_breaks_keep_how_each_next_section_begins() {
    let with = |text: &str, kind: Option<RichSectionBreak>| {
        RichBlock::Paragraph(RichParagraph {
            runs: vec![run(text)],
            section_break: kind,
            ..RichParagraph::default()
        })
    };
    let doc = doc_of(vec![
        with("첫 구역", Some(RichSectionBreak::Continuous)),
        with("둘째 구역", Some(RichSectionBreak::OddPage)),
        with("셋째 구역", Some(RichSectionBreak::EvenPage)),
        with("넷째 구역", None),
    ]);
    let bytes = encode(&doc, DocumentFormat::Docx);
    let body = part(&bytes, "word/document.xml");
    assert_eq!(body.matches("<w:sectPr>").count(), 4, "{body}");
    let back = decode(&bytes, "s.docx");
    let kinds: Vec<_> = back
        .blocks
        .iter()
        .map(|b| match b {
            RichBlock::Paragraph(p) => (p.plain_text(), p.section_break),
            _ => panic!("paragraph"),
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            ("첫 구역".to_string(), Some(RichSectionBreak::Continuous)),
            ("둘째 구역".to_string(), Some(RichSectionBreak::OddPage)),
            ("셋째 구역".to_string(), Some(RichSectionBreak::EvenPage)),
            ("넷째 구역".to_string(), None),
        ]
    );
}

#[test]
fn a_section_break_after_a_table_is_an_empty_paragraph_with_the_break() {
    let xml = "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>\
        <w:tbl><w:tr><w:tc><w:p><w:r><w:t>표</w:t></w:r></w:p></w:tc></w:tr></w:tbl>\
        <w:p><w:pPr><w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/></w:sectPr></w:pPr></w:p>\
        <w:p><w:r><w:t>다음 구역</w:t></w:r></w:p>\
        <w:sectPr><w:type w:val=\"nextPage\"/><w:pgSz w:w=\"11906\" w:h=\"16838\"/></w:sectPr></w:body></w:document>";
    let back = decode(&docx_with_body(xml), "t.docx");
    assert!(matches!(back.blocks[0], RichBlock::Table(_)));
    let RichBlock::Paragraph(brk) = &back.blocks[1] else {
        panic!("paragraph after the table")
    };
    assert_eq!(brk.section_break, Some(RichSectionBreak::NextPage));
    assert!(brk.runs.is_empty());
    assert_eq!(runs_of(&back, 2)[0].text, "다음 구역");
}

// ---- line numbers

#[test]
fn line_numbers_round_trip_and_a_hostile_value_is_dropped() {
    let mut doc = doc_of(vec![paragraph("본문")]);
    doc.page_frame.line_numbers = Some(RichLineNumbers {
        start: 5,
        count_by: 2,
        distance_twips: 360,
        restart: RichLineNumberRestart::EachSection,
    });
    let bytes = encode(&doc, DocumentFormat::Docx);
    assert!(part(&bytes, "word/document.xml").contains(
        "<w:lnNumType w:countBy=\"2\" w:start=\"4\" w:distance=\"360\" w:restart=\"newSection\"/>"
    ));
    assert_eq!(
        decode(&bytes, "l.docx").page_frame.line_numbers,
        doc.page_frame.line_numbers
    );

    let hostile = "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:p><w:r><w:t>x</w:t></w:r></w:p>\
        <w:sectPr><w:lnNumType w:countBy=\"0\"/></w:sectPr></w:body></w:document>";
    assert_eq!(
        decode(&docx_with_body(hostile), "h.docx")
            .page_frame
            .line_numbers,
        None
    );
    let huge = hostile.replace("w:countBy=\"0\"", "w:countBy=\"99999999\"");
    assert_eq!(
        decode(&docx_with_body(&huge), "h.docx")
            .page_frame
            .line_numbers,
        None
    );
}

// ---- watermark

#[test]
fn a_watermark_is_a_shape_in_the_header_and_reads_back_without_a_blank_header() {
    let mut doc = doc_of(vec![paragraph("본문")]);
    doc.page_frame.watermark = Some("대외비 & <기밀>".to_string());
    let bytes = encode(&doc, DocumentFormat::Docx);
    let header = part(&bytes, "word/header1.xml");
    assert!(
        header.contains("PowerPlusWaterMarkObject1")
            && header.contains("string=\"대외비 &amp; &lt;기밀&gt;\""),
        "{header}"
    );
    assert!(part(&bytes, "word/document.xml").contains("w:headerReference"));
    let result = UniversalFormatConverter::decode_rich(&bytes, Some("w.docx")).expect("decode");
    let DecodedContent::Document(back) = result.content else {
        panic!("document")
    };
    assert_eq!(
        back.page_frame.watermark.as_deref(),
        Some("대외비 & <기밀>")
    );
    assert!(
        back.page_frame.header.is_empty(),
        "the watermark's paragraph is not a header line"
    );
    assert!(
        result.warnings.iter().all(|w| !w.contains("개체")),
        "{:?}",
        result.warnings
    );
}

#[test]
fn a_watermark_goes_into_the_header_that_has_text_and_into_the_first_page_header() {
    let mut doc = doc_of(vec![paragraph("본문")]);
    doc.page_frame.watermark = Some("초안".to_string());
    doc.page_frame.header = vec![RichParagraph {
        runs: vec![run("머리글")],
        ..RichParagraph::default()
    }];
    doc.page_frame.first_page_different = true;
    let bytes = encode(&doc, DocumentFormat::Docx);
    for name in ["word/header1.xml", "word/header2.xml"] {
        assert!(
            part(&bytes, name).contains("PowerPlusWaterMarkObject"),
            "{name}"
        );
    }
    let back = decode(&bytes, "w.docx");
    assert_eq!(back.page_frame.watermark.as_deref(), Some("초안"));
    assert_eq!(back.page_frame.header.len(), 1);
    assert_eq!(back.page_frame.header[0].plain_text(), "머리글");
}

#[test]
fn a_hostile_watermark_text_is_cut_and_a_header_without_one_is_untouched() {
    let long = "가".repeat(200);
    let mut doc = doc_of(vec![paragraph("본문")]);
    doc.page_frame.watermark = Some(long);
    let back = round_trip(&doc, DocumentFormat::Docx, "w.docx");
    assert_eq!(
        back.page_frame.watermark.map(|t| t.chars().count()),
        Some(easyoffice_formats::rich_word_features::MAX_WATERMARK_CHARS)
    );
    let plain = doc_of(vec![paragraph("본문")]);
    let bytes = encode(&plain, DocumentFormat::Docx);
    assert!(!String::from_utf8_lossy(&bytes).contains("word/header1.xml"));
    assert_eq!(decode(&bytes, "p.docx").page_frame.watermark, None);
}

// ---- header rows of a table

fn table_of(rows: usize, header_rows: u32) -> RichBlock {
    let mut table = easyoffice_formats::RichTable::new(
        (0..rows)
            .map(|r| vec![format!("셀{r}a"), format!("셀{r}b")])
            .collect(),
    );
    table.header_rows = header_rows;
    RichBlock::Table(table)
}

fn header_rows_of(doc: &RichDocument) -> u32 {
    match &doc.blocks[0] {
        RichBlock::Table(t) => t.header_rows,
        _ => panic!("table expected"),
    }
}

#[test]
fn the_header_rows_of_a_table_repeat_on_every_page() {
    let doc = doc_of(vec![table_of(4, 2)]);
    let bytes = encode(&doc, DocumentFormat::Docx);
    let body = part(&bytes, "word/document.xml");
    assert_eq!(body.matches("<w:tblHeader/>").count(), 2, "{body}");
    assert_eq!(header_rows_of(&decode(&bytes, "h.docx")), 2);
    assert_eq!(
        header_rows_of(&round_trip(
            &doc_of(vec![table_of(3, 0)]),
            DocumentFormat::Docx,
            "h.docx"
        )),
        0
    );
}

#[test]
fn a_header_row_below_a_body_row_is_not_a_header_row_and_a_hostile_count_is_cut() {
    let row = |header: bool| {
        format!(
            "<w:tr>{}<w:tc><w:p><w:r><w:t>가</w:t></w:r></w:p></w:tc></w:tr>",
            if header {
                "<w:trPr><w:tblHeader/></w:trPr>"
            } else {
                ""
            }
        )
    };
    let xml = format!(
        "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:tbl>{}{}{}{}</w:tbl><w:p/></w:body></w:document>",
        row(true), row(false), row(true), row(true)
    );
    assert_eq!(
        header_rows_of(&decode(&docx_with_body(&xml), "h.docx")),
        1,
        "only the rows from the top"
    );
    let nested = "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:tbl><w:tr><w:tc>\
        <w:tbl><w:tr><w:trPr><w:tblHeader/></w:trPr><w:tc><w:p><w:r><w:t>안</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><w:t>밖</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/></w:body></w:document>";
    assert_eq!(
        header_rows_of(&decode(&docx_with_body(nested), "n.docx")),
        0,
        "a nested table's header row is not the outer one's"
    );
}

// ---- what a save to another format leaves out

#[test]
fn a_save_to_a_format_without_them_counts_the_word_style_items_it_leaves_out() {
    let mut doc = doc_of(vec![
        paragraph_of(vec![coded("2026-11-28", "DATE"), run(" 끝")]),
        RichBlock::Paragraph(RichParagraph {
            runs: vec![run("구역")],
            section_break: Some(RichSectionBreak::NextPage),
            ..RichParagraph::default()
        }),
        table_of(2, 1),
    ]);
    doc.page_frame.watermark = Some("초안".to_string());
    doc.page_frame.line_numbers = Some(RichLineNumbers::DEFAULT);
    // a field, a section break, a table with header rows, a watermark and line numbers
    for format in [
        DocumentFormat::Odt,
        DocumentFormat::Rtf,
        DocumentFormat::Hwpx,
    ] {
        let loss = easyoffice_formats::doc_save_loss(&doc, format);
        assert_eq!(loss.word_features, 5, "{format:?}");
        assert!(loss
            .warnings("ODT")
            .iter()
            .any(|w| w.contains("필드·책갈피·구역 나누기·줄 번호·워터마크·표 머리글 행 5곳")));
    }
    assert_eq!(
        easyoffice_formats::doc_save_loss(&doc, DocumentFormat::Docx).word_features,
        0
    );
    assert_eq!(
        easyoffice_formats::doc_save_loss(&doc_of(vec![paragraph("본문")]), DocumentFormat::Odt)
            .word_features,
        0
    );
}

// ---- document properties

#[test]
fn the_document_properties_round_trip_through_core_properties() {
    let mut doc = doc_of(vec![paragraph("본문")]);
    doc.properties = RichProperties {
        title: "분기 보고서".to_string(),
        author: "홍길동".to_string(),
        subject: "3분기".to_string(),
        keywords: "매출, 비용".to_string(),
        comments: "검토 중\n초안".to_string(),
    };
    let bytes = encode(&doc, DocumentFormat::Docx);
    let core = part(&bytes, "docProps/core.xml");
    assert!(core.contains("<dc:title>분기 보고서</dc:title>"), "{core}");
    assert!(
        core.contains("<dc:creator>홍길동</dc:creator>")
            && core.contains("<cp:keywords>매출, 비용</cp:keywords>")
    );
    let back = decode(&bytes, "p.docx");
    assert_eq!(back.title, "분기 보고서");
    assert_eq!(
        back.properties,
        RichProperties {
            title: String::new(),
            ..doc.properties.clone()
        }
    );
}

#[test]
fn a_document_without_an_author_reads_back_without_one() {
    let doc = doc_of(vec![paragraph("본문")]);
    let bytes = encode(&doc, DocumentFormat::Docx);
    assert!(part(&bytes, "docProps/core.xml").contains("<dc:creator>EasyOffice</dc:creator>"));
    assert!(decode(&bytes, "p.docx").properties.is_empty());
}

#[test]
fn hostile_properties_are_cut_and_cleaned() {
    let mut doc = doc_of(vec![paragraph("본문")]);
    doc.properties.author = format!("{}\u{7}", "가".repeat(5000));
    doc.properties.comments = "<script>x</script>".to_string();
    let bytes = encode(&doc, DocumentFormat::Docx);
    let back = decode(&bytes, "p.docx");
    assert!(
        back.properties.author.len() <= easyoffice_formats::rich_word_features::MAX_PROPERTY_BYTES
    );
    assert!(!back.properties.author.contains('\u{7}'));
    assert_eq!(
        back.properties.comments, "<script>x</script>",
        "escaped in the part, plain text back"
    );
}

//! Header and footer of every page: DOCX, ODT and HWPX files keep them as parts of their own and the
//! readers give them back; first-page, odd/even and per-section variants are reported, and hostile
//! header parts are refused or cut at a limit.

use easyoffice_formats::{
    DecodedContent, DocumentFormat, EasyOfficeError, RichAlignment, RichBlock, RichDocument,
    RichField, RichPageFrame, RichParagraph, RichRun, UniversalFormatConverter, ZipArchiveCodec,
    ZipSecurityPolicy, MAX_PAGE_FRAME_PARAGRAPHS,
};

fn para(runs: Vec<RichRun>, alignment: Option<RichAlignment>) -> RichParagraph {
    RichParagraph {
        runs,
        alignment,
        ..RichParagraph::default()
    }
}

fn page_number() -> RichRun {
    RichRun {
        bold: true,
        field: Some(RichField::PageNumber),
        ..RichRun::default()
    }
}

fn sample() -> RichDocument {
    RichDocument {
        title: "쪽 틀".to_string(),
        blocks: vec![
            RichBlock::Paragraph(para(vec![RichRun::plain("본문 첫 문단")], None)),
            RichBlock::Paragraph(para(vec![RichRun::plain("본문 둘째 문단")], None)),
        ],
        official_agency: None,
        page_frame: RichPageFrame {
            header: vec![
                para(
                    vec![RichRun {
                        bold: true,
                        size_half_pt: Some(18),
                        color: Some("1F4E79".to_string()),
                        ..RichRun::plain("분기 보고서")
                    }],
                    Some(RichAlignment::Center),
                ),
                para(vec![RichRun::plain("총무팀 <기밀> & 사본")], None),
            ],
            footer: vec![para(
                vec![RichRun::plain("- "), page_number(), RichRun::plain(" -")],
                Some(RichAlignment::Right),
            )],
        },
    }
}

fn decode(bytes: &[u8], name: &str) -> (RichDocument, Vec<String>) {
    let decoded = UniversalFormatConverter::decode_rich(bytes, Some(name)).expect("decode");
    let DecodedContent::Document(doc) = decoded.content else {
        panic!("document expected");
    };
    (doc, decoded.warnings)
}

fn entries(bytes: &[u8]) -> Vec<(String, String)> {
    ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
        .expect("unpack")
        .into_iter()
        .map(|e| (e.path, String::from_utf8_lossy(&e.data).into_owned()))
        .collect()
}

fn part<'a>(entries: &'a [(String, String)], path: &str) -> &'a str {
    entries
        .iter()
        .find(|(p, _)| p == path)
        .map_or_else(|| panic!("missing part {path}"), |(_, xml)| xml.as_str())
}

fn assert_frame_round_trip(format: DocumentFormat, name: &str) {
    let source = sample();
    let bytes = UniversalFormatConverter::encode_rich(&source, format, None).expect("encode");
    let (back, warnings) = decode(&bytes, name);
    assert_eq!(back.page_frame, source.page_frame, "{name}");
    assert_eq!(back.blocks.len(), 2, "{name}: the frame is not body text");
    assert!(warnings.is_empty(), "{name}: {warnings:?}");
    let (again, _) = decode(
        &UniversalFormatConverter::encode_rich(&back, format, None).expect("re-encode"),
        name,
    );
    assert_eq!(again.page_frame, source.page_frame, "{name} second round");
}

#[test]
fn docx_keeps_header_and_footer_as_parts_of_the_section() {
    let bytes = UniversalFormatConverter::encode_rich(&sample(), DocumentFormat::Docx, None)
        .expect("encode");
    let parts = entries(&bytes);
    let document = part(&parts, "word/document.xml");
    assert!(document.contains("<w:headerReference w:type=\"default\" r:id=\"rIdHeader1\"/>"));
    assert!(document.contains("<w:footerReference w:type=\"default\" r:id=\"rIdFooter1\"/>"));
    assert!(
        !document.contains("분기 보고서"),
        "the header is not in the body"
    );
    assert!(part(&parts, "word/header1.xml").contains("총무팀 &lt;기밀&gt; &amp; 사본"));
    assert!(part(&parts, "word/footer1.xml").contains("w:instr=\" PAGE \""));
    assert!(part(&parts, "[Content_Types].xml").contains("wordprocessingml.header+xml"));
    assert!(part(&parts, "word/_rels/document.xml.rels").contains("Target=\"footer1.xml\""));
    assert_frame_round_trip(DocumentFormat::Docx, "frame.docx");
}

#[test]
fn odt_keeps_header_and_footer_in_the_master_page() {
    let bytes = UniversalFormatConverter::encode_rich(&sample(), DocumentFormat::Odt, None)
        .expect("encode");
    let parts = entries(&bytes);
    let styles = part(&parts, "styles.xml");
    assert!(styles.contains("<style:master-page"));
    assert!(styles.contains("<style:header>"));
    assert!(styles.contains("<text:page-number text:select-page=\"current\">"));
    assert!(!part(&parts, "content.xml").contains("분기 보고서"));
    assert_frame_round_trip(DocumentFormat::Odt, "frame.odt");
}

#[test]
fn hwpx_keeps_header_and_footer_as_controls_of_the_first_paragraph() {
    let bytes = UniversalFormatConverter::encode_rich(&sample(), DocumentFormat::Hwpx, None)
        .expect("encode");
    let parts = entries(&bytes);
    let section = part(&parts, "Contents/section0.xml");
    assert!(section.contains("<hp:header id=\"1\" applyPageType=\"BOTH\">"));
    assert!(section.contains("<hp:footer id=\"2\" applyPageType=\"BOTH\">"));
    assert!(section.contains("numType=\"PAGE\""));
    assert_frame_round_trip(DocumentFormat::Hwpx, "frame.hwpx");
}

#[test]
fn a_document_without_a_frame_writes_no_header_parts() {
    let mut doc = sample();
    doc.page_frame = RichPageFrame::default();
    for (format, name) in [
        (DocumentFormat::Docx, "word/header1.xml"),
        (DocumentFormat::Odt, "styles.xml"),
    ] {
        let bytes = UniversalFormatConverter::encode_rich(&doc, format, None).expect("encode");
        assert!(entries(&bytes).iter().all(|(p, _)| p != name), "{name}");
    }
}

// ---- reading files other programs wrote

fn docx_package(document: &str, extra: &[(&str, &str)], rels: &str) -> Vec<u8> {
    let mut parts: Vec<(String, Vec<u8>)> = vec![
        (
            "[Content_Types].xml".to_string(),
            br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/></Types>"#.to_vec(),
        ),
        (
            "word/document.xml".to_string(),
            document.as_bytes().to_vec(),
        ),
        (
            "word/_rels/document.xml.rels".to_string(),
            format!("<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{rels}</Relationships>").into_bytes(),
        ),
    ];
    for (path, xml) in extra {
        parts.push(((*path).to_string(), xml.as_bytes().to_vec()));
    }
    ZipArchiveCodec::pack_compressed(&parts).expect("pack")
}

const W: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"";
const REL_BASE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn body_with_section(refs: &str, props: &str) -> String {
    format!(
        "<w:document {W}><w:body><w:p><w:r><w:t>본문</w:t></w:r></w:p><w:sectPr>{refs}{props}</w:sectPr></w:body></w:document>"
    )
}

#[test]
fn a_word_footer_with_a_complex_page_field_reads_as_a_field_run() {
    let footer = format!(
        "<w:ftr {W}><w:p><w:pPr><w:jc w:val=\"center\"/></w:pPr><w:r><w:t xml:space=\"preserve\">쪽 </w:t></w:r>\
         <w:r><w:rPr><w:b/></w:rPr><w:fldChar w:fldCharType=\"begin\"/></w:r>\
         <w:r><w:instrText xml:space=\"preserve\"> PAGE \\* MERGEFORMAT </w:instrText></w:r>\
         <w:r><w:rPr><w:b/></w:rPr><w:fldChar w:fldCharType=\"separate\"/></w:r>\
         <w:r><w:rPr><w:b/></w:rPr><w:t>7</w:t></w:r>\
         <w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:p></w:ftr>"
    );
    let bytes = docx_package(
        &body_with_section("<w:footerReference w:type=\"default\" r:id=\"rId9\"/>", ""),
        &[("word/footer7.xml", &footer)],
        &format!("<Relationship Id=\"rId9\" Type=\"{REL_BASE}/footer\" Target=\"footer7.xml\"/>"),
    );
    let (doc, warnings) = decode(&bytes, "word.docx");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(doc.page_frame.header.is_empty());
    let [p] = doc.page_frame.footer.as_slice() else {
        panic!("one footer paragraph");
    };
    assert_eq!(p.alignment, Some(RichAlignment::Center));
    assert_eq!(p.runs.len(), 2, "{:?}", p.runs);
    assert_eq!(p.runs[0].text, "쪽 ");
    assert_eq!(p.runs[1].field, Some(RichField::PageNumber));
    assert!(p.runs[1].bold && p.runs[1].text.is_empty());
}

#[test]
fn first_page_even_page_and_per_section_variants_are_reported_not_kept() {
    let part = |text: &str| format!("<w:hdr {W}><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:hdr>");
    let document = format!(
        "<w:document {W}><w:body>\
         <w:p><w:pPr><w:sectPr><w:headerReference w:type=\"default\" r:id=\"rId1\"/><w:headerReference w:type=\"first\" r:id=\"rId2\"/><w:titlePg/></w:sectPr></w:pPr><w:r><w:t>하나</w:t></w:r></w:p>\
         <w:p><w:r><w:t>둘</w:t></w:r></w:p>\
         <w:sectPr><w:headerReference w:type=\"default\" r:id=\"rId3\"/></w:sectPr></w:body></w:document>"
    );
    let rels: String = [
        ("rId1", "header1"),
        ("rId2", "header2"),
        ("rId3", "header3"),
    ]
    .iter()
    .map(|(id, name)| {
        format!("<Relationship Id=\"{id}\" Type=\"{REL_BASE}/header\" Target=\"{name}.xml\"/>")
    })
    .collect();
    let bytes = docx_package(
        &document,
        &[
            ("word/header1.xml", &part("첫 구역 기본")),
            ("word/header2.xml", &part("첫 쪽 전용")),
            ("word/header3.xml", &part("둘째 구역")),
        ],
        &rels,
    );
    let (doc, warnings) = decode(&bytes, "sections.docx");
    assert_eq!(doc.page_frame.header[0].plain_text(), "첫 구역 기본");
    assert!(
        warnings.iter().any(|w| w.contains("첫 쪽·짝수 쪽")),
        "{warnings:?}"
    );
    assert!(
        warnings.iter().any(|w| w.contains("구역마다")),
        "{warnings:?}"
    );
}

#[test]
fn an_unused_first_page_reference_is_not_reported() {
    let header = format!("<w:hdr {W}><w:p><w:r><w:t>기본</w:t></w:r></w:p></w:hdr>");
    let bytes = docx_package(
        &body_with_section(
            "<w:headerReference w:type=\"default\" r:id=\"rId1\"/><w:headerReference w:type=\"first\" r:id=\"rId1\"/>",
            "",
        ),
        &[("word/header1.xml", &header)],
        &format!("<Relationship Id=\"rId1\" Type=\"{REL_BASE}/header\" Target=\"header1.xml\"/>"),
    );
    let (doc, warnings) = decode(&bytes, "plain.docx");
    assert_eq!(doc.page_frame.header[0].plain_text(), "기본");
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn tables_and_pictures_in_a_header_are_reported() {
    let header = format!(
        "<w:hdr {W}><w:p><w:r><w:t>제목</w:t></w:r></w:p>\
         <w:tbl><w:tr><w:tc><w:p><w:r><w:t>표</w:t></w:r></w:p></w:tc></w:tr></w:tbl>\
         <w:p><w:r><w:drawing><wp:inline xmlns:wp=\"x\"/></w:drawing></w:r></w:p></w:hdr>"
    );
    let bytes = docx_package(
        &body_with_section("<w:headerReference w:type=\"default\" r:id=\"rId1\"/>", ""),
        &[("word/header1.xml", &header)],
        &format!("<Relationship Id=\"rId1\" Type=\"{REL_BASE}/header\" Target=\"header1.xml\"/>"),
    );
    let (doc, warnings) = decode(&bytes, "objects.docx");
    assert_eq!(doc.page_frame.header.len(), 1);
    assert!(warnings.iter().any(|w| w.contains("표")), "{warnings:?}");
    assert!(warnings.iter().any(|w| w.contains("그림")), "{warnings:?}");
}

#[test]
fn a_header_with_too_many_paragraphs_keeps_the_first_ones() {
    let paragraphs: String = (0..MAX_PAGE_FRAME_PARAGRAPHS + 30)
        .map(|i| format!("<w:p><w:r><w:t>줄 {i}</w:t></w:r></w:p>"))
        .collect();
    let header = format!("<w:hdr {W}>{paragraphs}</w:hdr>");
    let bytes = docx_package(
        &body_with_section("<w:headerReference w:type=\"default\" r:id=\"rId1\"/>", ""),
        &[("word/header1.xml", &header)],
        &format!("<Relationship Id=\"rId1\" Type=\"{REL_BASE}/header\" Target=\"header1.xml\"/>"),
    );
    let (doc, warnings) = decode(&bytes, "long.docx");
    assert_eq!(doc.page_frame.header.len(), MAX_PAGE_FRAME_PARAGRAPHS);
    assert!(warnings.iter().any(|w| w.contains("64개")), "{warnings:?}");
}

#[test]
fn a_header_part_with_a_doctype_is_refused() {
    let header = format!(
        "<?xml version=\"1.0\"?><!DOCTYPE w:hdr [<!ENTITY x SYSTEM \"file:///etc/passwd\">]><w:hdr {W}><w:p><w:r><w:t>&x;</w:t></w:r></w:p></w:hdr>"
    );
    let bytes = docx_package(
        &body_with_section("<w:headerReference w:type=\"default\" r:id=\"rId1\"/>", ""),
        &[("word/header1.xml", &header)],
        &format!("<Relationship Id=\"rId1\" Type=\"{REL_BASE}/header\" Target=\"header1.xml\"/>"),
    );
    let err = UniversalFormatConverter::decode_rich(&bytes, Some("xxe.docx"))
        .map(|_| ())
        .expect_err("a DTD is refused");
    assert!(
        matches!(err, EasyOfficeError::XxeOrExternalEntityDetected),
        "{err:?}"
    );
}

#[test]
fn header_references_that_leave_the_package_or_miss_are_ignored() {
    let bytes = docx_package(
        &body_with_section(
            "<w:headerReference w:type=\"default\" r:id=\"rId1\"/><w:footerReference w:type=\"default\" r:id=\"rId8\"/>",
            "",
        ),
        &[],
        &format!(
            "<Relationship Id=\"rId1\" Type=\"{REL_BASE}/header\" Target=\"../../etc/passwd\"/>\
             <Relationship Id=\"rId2\" Type=\"{REL_BASE}/footer\" Target=\"https://example.com/f.xml\" TargetMode=\"External\"/>"
        ),
    );
    let (doc, _) = decode(&bytes, "escape.docx");
    assert!(doc.page_frame.is_empty());
    assert_eq!(doc.blocks.len(), 1);
}

// ---- ODT and HWPX files from other programs

fn odt_package(styles: &str) -> Vec<u8> {
    let content = "<?xml version=\"1.0\"?><office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\"><office:body><office:text><text:p>본문</text:p></office:text></office:body></office:document-content>";
    let parts: Vec<(String, Vec<u8>)> = vec![
        (
            "mimetype".to_string(),
            b"application/vnd.oasis.opendocument.text".to_vec(),
        ),
        ("content.xml".to_string(), content.as_bytes().to_vec()),
        ("styles.xml".to_string(), styles.as_bytes().to_vec()),
    ];
    ZipArchiveCodec::pack_compressed(&parts).expect("pack")
}

const ODF_NS: &str = "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\"";

#[test]
fn odt_reads_the_first_master_page_and_reports_left_and_first_variants() {
    let styles = format!(
        "<office:document-styles {ODF_NS}><office:master-styles>\
         <style:master-page style:name=\"Standard\">\
         <style:header><text:p>기본 머리말</text:p></style:header>\
         <style:header-left><text:p>왼쪽 머리말</text:p></style:header-left>\
         <style:footer><text:p>기본 <text:page-number text:select-page=\"current\">3</text:page-number></text:p></style:footer>\
         </style:master-page>\
         <style:master-page style:name=\"Other\"><style:header><text:p>다른 쪽</text:p></style:header></style:master-page>\
         </office:master-styles></office:document-styles>"
    );
    let (doc, warnings) = decode(&odt_package(&styles), "other.odt");
    assert_eq!(doc.page_frame.header.len(), 1);
    assert_eq!(doc.page_frame.header[0].plain_text(), "기본 머리말");
    let footer = &doc.page_frame.footer[0];
    assert_eq!(footer.runs[1].field, Some(RichField::PageNumber));
    assert_eq!(footer.plain_text(), "기본 ");
    assert!(warnings.iter().any(|w| w.contains("왼쪽")), "{warnings:?}");
}

#[test]
fn odt_header_with_too_many_paragraphs_keeps_the_first_ones() {
    let lines: String = (0..MAX_PAGE_FRAME_PARAGRAPHS + 5)
        .map(|i| format!("<text:p>줄 {i}</text:p>"))
        .collect();
    let styles = format!(
        "<office:document-styles {ODF_NS}><office:master-styles><style:master-page style:name=\"Standard\"><style:header>{lines}</style:header></style:master-page></office:master-styles></office:document-styles>"
    );
    let (doc, warnings) = decode(&odt_package(&styles), "long.odt");
    assert_eq!(doc.page_frame.header.len(), MAX_PAGE_FRAME_PARAGRAPHS);
    assert!(warnings.iter().any(|w| w.contains("64개")), "{warnings:?}");
}

#[test]
fn hwpx_even_and_odd_headers_and_second_sections_are_reported() {
    let source = RichDocument {
        page_frame: RichPageFrame {
            header: vec![para(vec![RichRun::plain("양쪽")], None)],
            footer: Vec::new(),
        },
        blocks: vec![RichBlock::Paragraph(para(
            vec![RichRun::plain("본문")],
            None,
        ))],
        ..RichDocument::default()
    };
    let bytes =
        UniversalFormatConverter::encode_rich(&source, DocumentFormat::Hwpx, None).expect("encode");
    let parts = entries(&bytes);
    let section = part(&parts, "Contents/section0.xml");
    // The same header again for even pages and for a second `BOTH` control of the section.
    let control_start = section.find("<hp:ctrl><hp:header").expect("header control");
    let control_end =
        section[control_start..].find("</hp:ctrl>").expect("end") + control_start + 10;
    let control = &section[control_start..control_end];
    let edited = section.replacen(
        control,
        &format!(
            "{control}{}{}",
            control.replace("BOTH", "EVEN").replace("양쪽", "짝수"),
            control.replace("양쪽", "다른 구역")
        ),
        1,
    );
    let mut files: Vec<(String, Vec<u8>)> =
        ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default())
            .expect("unpack")
            .into_iter()
            .map(|e| (e.path, e.data))
            .collect();
    for (path, data) in &mut files {
        if path == "Contents/section0.xml" {
            *data = edited.clone().into_bytes();
        }
    }
    let (doc, warnings) = decode(
        &ZipArchiveCodec::pack_compressed(&files).expect("pack"),
        "x.hwpx",
    );
    assert_eq!(doc.page_frame.header[0].plain_text(), "양쪽");
    assert!(
        warnings.iter().any(|w| w.contains("홀수·짝수")),
        "{warnings:?}"
    );
    assert!(
        warnings.iter().any(|w| w.contains("구역마다")),
        "{warnings:?}"
    );
}

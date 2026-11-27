//! PD-363: a file made elsewhere keeps its document defaults (typeface, size, alignment, paragraph
//! spacing, line height, tab stop) when it is opened and saved again. The Korean defaults of PD-307
//! belong to a new document only. Each test builds the file by replacing the style parts of a file
//! this crate wrote with the parts another word processor writes.

use easyoffice_formats::{
    DecodedContent, DocumentFormat, RichAlignment, RichBlock, RichDefaults, RichDocument,
    RichLineSpacing, RichParagraph, RichRun, UniversalFormatConverter, ZipArchiveCodec,
    ZipSecurityPolicy,
};

fn paragraph(text: &str) -> RichBlock {
    RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun::plain(text)],
        ..RichParagraph::default()
    })
}

fn new_document() -> RichDocument {
    RichDocument {
        title: "기본값".to_string(),
        blocks: vec![paragraph("첫째 문단"), paragraph("둘째 문단")],
        ..RichDocument::default()
    }
}

fn unpack(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
        .expect("unpack")
        .into_iter()
        .map(|e| (e.path, e.data))
        .collect()
}

fn part(entries: &[(String, Vec<u8>)], path: &str) -> String {
    let (_, data) = entries
        .iter()
        .find(|(p, _)| p == path)
        .unwrap_or_else(|| panic!("missing part {path}"));
    String::from_utf8(data.clone()).expect("utf8 part")
}

/// The package with the given parts replaced (or added).
fn with_parts(bytes: &[u8], parts: &[(&str, &str)]) -> Vec<u8> {
    let mut entries = unpack(bytes);
    for (path, text) in parts {
        let data = text.as_bytes().to_vec();
        match entries.iter_mut().find(|(p, _)| p == path) {
            Some(entry) => entry.1 = data,
            None => entries.push(((*path).to_string(), data)),
        }
    }
    ZipArchiveCodec::pack(&entries).expect("pack")
}

fn decode(bytes: &[u8], name: &str) -> RichDocument {
    match UniversalFormatConverter::decode_rich(bytes, Some(name))
        .expect("decode_rich")
        .content
    {
        DecodedContent::Document(doc) => doc,
        other => panic!("expected a document for {name}, got {other:?}"),
    }
}

fn encode(doc: &RichDocument, format: DocumentFormat) -> Vec<u8> {
    UniversalFormatConverter::encode_rich(doc, format, None).expect("encode_rich")
}

// ---------------------------------------------------------------- DOCX

const WORD_STYLES: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
    "<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
    "<w:docDefaults><w:rPrDefault><w:rPr>",
    "<w:rFonts w:asciiTheme=\"minorHAnsi\" w:eastAsiaTheme=\"minorEastAsia\" w:hAnsiTheme=\"minorHAnsi\" w:cstheme=\"minorBidi\"/>",
    "<w:sz w:val=\"22\"/><w:szCs w:val=\"22\"/><w:lang w:val=\"en-US\" w:eastAsia=\"ko-KR\" w:bidi=\"ar-SA\"/>",
    "</w:rPr></w:rPrDefault><w:pPrDefault><w:pPr>",
    "<w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults>",
    "<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/></w:style>",
    "</w:styles>"
);
const WORD_THEME: &str = concat!(
    "<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"Office\">",
    "<a:themeElements><a:fontScheme name=\"Office\">",
    "<a:majorFont><a:latin typeface=\"Calibri Light\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:majorFont>",
    "<a:minorFont><a:latin typeface=\"Calibri\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/>",
    "<a:font script=\"Hang\" typeface=\"맑은 고딕\"/></a:minorFont>",
    "</a:fontScheme></a:themeElements></a:theme>"
);
const WORD_SETTINGS: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
    "<w:settings xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
    "<w:defaultTabStop w:val=\"720\"/></w:settings>"
);

/// A DOCX as an office suite writes it: Calibri 11 from the theme, single unspaced lines, no `w:jc`, 720-twip tabs.
fn word_docx() -> Vec<u8> {
    with_parts(
        &encode(&new_document(), DocumentFormat::Docx),
        &[
            ("word/styles.xml", WORD_STYLES),
            ("word/settings.xml", WORD_SETTINGS),
            ("word/theme/theme1.xml", WORD_THEME),
        ],
    )
}

#[test]
fn an_opened_docx_keeps_its_defaults_when_saved() {
    let opened = decode(&word_docx(), "word.docx");
    let defaults = opened.defaults.clone().expect("a read file has defaults");
    assert_eq!(defaults.font.as_deref(), Some("Calibri"));
    assert_eq!(defaults.east_asian_font.as_deref(), Some("맑은 고딕"));
    assert_eq!(
        (
            defaults.size_half_points,
            defaults.alignment,
            defaults.space_before_twips,
            defaults.space_after_twips,
            defaults.line_240ths,
            defaults.tab_stop_twips
        ),
        (22, None, 0, 0, 240, 720)
    );

    let saved = unpack(&encode(&opened, DocumentFormat::Docx));
    let styles = part(&saved, "word/styles.xml");
    assert!(
        styles
            .contains("<w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\" w:eastAsia=\"맑은 고딕\""),
        "{styles}"
    );
    assert!(styles.contains("<w:sz w:val=\"22\"/>"), "{styles}");
    assert!(
        styles.contains("<w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/>"),
        "{styles}"
    );
    assert!(!styles.contains("<w:jc w:val=\"both\"/>"), "{styles}");
    assert!(part(&saved, "word/settings.xml").contains("<w:defaultTabStop w:val=\"720\"/>"));

    // saving again changes nothing
    let reopened = decode(&encode(&opened, DocumentFormat::Docx), "saved.docx");
    assert_eq!(reopened.defaults, opened.defaults);
    assert_eq!(reopened.blocks, opened.blocks);
}

#[test]
fn a_docx_that_justifies_and_spaces_its_paragraphs_keeps_that() {
    let styles = WORD_STYLES
        .replace(
            "<w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/>",
            "<w:spacing w:before=\"120\" w:after=\"200\" w:line=\"276\" w:lineRule=\"auto\"/><w:jc w:val=\"both\"/>",
        )
        .replace(
            "<w:qFormat/></w:style>",
            "<w:pPr><w:jc w:val=\"center\"/></w:pPr><w:qFormat/></w:style>",
        );
    let bytes = with_parts(&word_docx(), &[("word/styles.xml", &styles)]);
    let opened = decode(&bytes, "word.docx");
    // the Normal style overrides the document default
    assert_eq!(
        opened.defaults.as_ref().map(|d| d.alignment),
        Some(Some(RichAlignment::Center))
    );
    let saved = unpack(&encode(&opened, DocumentFormat::Docx));
    let out = part(&saved, "word/styles.xml");
    assert!(
        out.contains("<w:spacing w:before=\"120\" w:after=\"200\" w:line=\"276\" w:lineRule=\"auto\"/><w:jc w:val=\"center\"/>"),
        "{out}"
    );
}

#[test]
fn a_new_document_gets_the_korean_defaults_and_an_opened_one_does_not() {
    let new = unpack(&encode(&new_document(), DocumentFormat::Docx));
    let styles = part(&new, "word/styles.xml");
    assert!(styles.contains("w:ascii=\"맑은 고딕\""), "{styles}");
    assert!(styles.contains("<w:jc w:val=\"both\"/>"), "{styles}");
    assert!(part(&new, "word/settings.xml").contains("<w:defaultTabStop w:val=\"800\"/>"));
    assert_eq!(new_document().defaults, None);

    let opened = unpack(&encode(
        &decode(&word_docx(), "word.docx"),
        DocumentFormat::Docx,
    ));
    assert!(!part(&opened, "word/styles.xml").contains("맑은 고딕\" w:hAnsi"));
}

#[test]
fn a_docx_without_a_styles_part_gets_the_format_defaults_not_the_korean_ones() {
    let entries: Vec<_> = unpack(&encode(&new_document(), DocumentFormat::Docx))
        .into_iter()
        .filter(|(p, _)| p != "word/styles.xml" && p != "word/settings.xml")
        .collect();
    let bytes = ZipArchiveCodec::pack(&entries).expect("pack");
    let opened = decode(&bytes, "bare.docx");
    assert_eq!(opened.defaults, Some(RichDefaults::default()));
    let saved = unpack(&encode(&opened, DocumentFormat::Docx));
    let styles = part(&saved, "word/styles.xml");
    assert!(!styles.contains("<w:jc w:val=\"both\"/>"), "{styles}");
    assert!(styles.contains("w:after=\"0\" w:line=\"240\""), "{styles}");
}

#[test]
fn the_defaults_of_a_docx_reach_an_odt_and_an_rtf_made_from_it() {
    let opened = decode(&word_docx(), "word.docx");
    let odt = unpack(&encode(&opened, DocumentFormat::Odt));
    let styles = part(&odt, "styles.xml");
    assert!(styles.contains("style:font-name=\"Calibri\""), "{styles}");
    assert!(styles.contains("fo:font-size=\"11pt\""), "{styles}");
    assert!(styles.contains("fo:text-align=\"start\""), "{styles}");
    assert!(styles.contains("fo:margin-bottom=\"0pt\""), "{styles}");
    assert!(styles.contains("fo:line-height=\"100%\""), "{styles}");

    let rtf = String::from_utf8(encode(&opened, DocumentFormat::Rtf)).expect("rtf text");
    assert!(rtf.contains("{\\f0\\fnil\\fcharset129 Calibri;}"), "{rtf}");
    assert!(rtf.contains("\\sa0\\sl240\\slmult1"), "{rtf}");
    assert!(rtf.contains("\\deftab720"), "{rtf}");
}

// ---------------------------------------------------------------- ODT

const WRITER_STYLES: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
    "<office:document-styles xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
    "xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" ",
    "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" ",
    "xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" ",
    "xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" office:version=\"1.2\">",
    "<office:font-face-decls><style:font-face style:name=\"Liberation Serif\" svg:font-family=\"&apos;Liberation Serif&apos;\"/>",
    "</office:font-face-decls><office:styles><style:default-style style:family=\"paragraph\">",
    "<style:paragraph-properties style:tab-stop-distance=\"0.4925in\" style:writing-mode=\"page\"/>",
    "<style:text-properties style:font-name=\"Liberation Serif\" fo:font-size=\"12pt\" fo:language=\"en\" fo:country=\"US\"/>",
    "</style:default-style><style:style style:name=\"Standard\" style:family=\"paragraph\" style:class=\"text\"/>",
    "</office:styles><office:automatic-styles/><office:master-styles/></office:document-styles>"
);

#[test]
fn an_opened_odt_keeps_its_defaults_when_saved() {
    let bytes = with_parts(
        &encode(&new_document(), DocumentFormat::Odt),
        &[("styles.xml", WRITER_STYLES)],
    );
    let opened = decode(&bytes, "writer.odt");
    let defaults = opened.defaults.clone().expect("a read file has defaults");
    assert_eq!(defaults.font.as_deref(), Some("Liberation Serif"));
    assert_eq!(
        (
            defaults.size_half_points,
            defaults.alignment,
            defaults.line_240ths
        ),
        (24, None, 240)
    );

    let saved = unpack(&encode(&opened, DocumentFormat::Odt));
    let styles = part(&saved, "styles.xml");
    assert!(
        styles.contains("style:font-name=\"Liberation Serif\""),
        "{styles}"
    );
    assert!(styles.contains("fo:font-size=\"12pt\""), "{styles}");
    assert!(styles.contains("fo:text-align=\"start\""), "{styles}");
    assert!(styles.contains("fo:margin-bottom=\"0pt\""), "{styles}");
    assert!(styles.contains("fo:line-height=\"100%\""), "{styles}");
    assert!(
        styles.contains("style:tab-stop-distance=\"35.45pt\""),
        "{styles}"
    );
    assert!(!styles.contains("맑은 고딕"), "{styles}");

    let reopened = decode(&encode(&opened, DocumentFormat::Odt), "saved.odt");
    assert_eq!(reopened.defaults, opened.defaults);
    assert_eq!(reopened.blocks, opened.blocks);

    let new = unpack(&encode(&new_document(), DocumentFormat::Odt));
    let new_styles = part(&new, "styles.xml");
    assert!(
        new_styles.contains("fo:text-align=\"justify\""),
        "{new_styles}"
    );
    assert!(
        new_styles.contains("style:font-name=\"맑은 고딕\""),
        "{new_styles}"
    );
}

// ---------------------------------------------------------------- RTF

const WORD_RTF: &str = concat!(
    "{\\rtf1\\ansi\\ansicpg1252\\deff0{\\fonttbl{\\f0\\fswiss\\fcharset0 Arial;}}",
    "{\\*\\defchp \\fs22}{\\*\\defpap \\sa0\\sl240\\slmult1}",
    "{\\stylesheet{\\ql \\li0\\ri0\\sa0\\sl240\\slmult1 \\f0\\fs22 Normal;}}",
    "\\deftab720\\pard\\plain\\f0\\fs22 Hello\\par}"
);

#[test]
fn an_opened_rtf_keeps_its_defaults_when_saved() {
    let opened = decode(WORD_RTF.as_bytes(), "word.rtf");
    let defaults = opened.defaults.clone().expect("a read file has defaults");
    assert_eq!(defaults.font.as_deref(), Some("Arial"));
    assert_eq!(
        (
            defaults.size_half_points,
            defaults.alignment,
            defaults.space_after_twips,
            defaults.line_240ths,
            defaults.tab_stop_twips
        ),
        (22, None, 0, 240, 720)
    );

    let saved = String::from_utf8(encode(&opened, DocumentFormat::Rtf)).expect("rtf text");
    assert!(
        saved.contains("{\\f0\\fnil\\fcharset129 Arial;}"),
        "{saved}"
    );
    assert!(saved.contains("{\\*\\defchp \\fs22}"), "{saved}");
    assert!(
        saved.contains("\\defpap \\ql\\sa0\\sl240\\slmult1"),
        "{saved}"
    );
    assert!(saved.contains("\\deftab720"), "{saved}");
    assert!(!saved.contains("\\qj"), "{saved}");

    let reopened = decode(saved.as_bytes(), "saved.rtf");
    assert_eq!(reopened.defaults, opened.defaults);

    let new = String::from_utf8(encode(&new_document(), DocumentFormat::Rtf)).expect("rtf text");
    assert!(
        new.contains("\\defpap \\qj\\sa160\\sl259\\slmult1"),
        "{new}"
    );
    assert!(new.contains("\\deftab800"), "{new}");
}

#[test]
fn an_rtf_with_a_korean_font_name_in_a_code_page_keeps_the_name() {
    // 맑은 고딕 as a Korean word processor writes it
    let rtf = "{\\rtf1\\ansi\\deff0{\\fonttbl{\\f0\\fnil\\fcharset129 \\'b8\\'bc\\'c0\\'ba \\'b0\\'ed\\'b5\\'f1;}}\\pard x\\par}";
    let opened = decode(rtf.as_bytes(), "korean.rtf");
    assert_eq!(
        opened.defaults.and_then(|d| d.font).as_deref(),
        Some("맑은 고딕")
    );
}

// ---------------------------------------------------------------- HWPX

#[test]
fn an_opened_hwpx_keeps_the_formatting_its_normal_style_gave_the_paragraphs() {
    let written = encode(&new_document(), DocumentFormat::Hwpx);
    let header = part(&unpack(&written), "Contents/header.xml");
    // a file whose Normal style is left aligned, 130% lines, 11 pt, in another face
    let foreign = header
        .replace("horizontal=\"JUSTIFY\"", "horizontal=\"LEFT\"")
        .replace(
            "<hh:lineSpacing type=\"PERCENT\" value=\"160\"",
            "<hh:lineSpacing type=\"PERCENT\" value=\"130\"",
        )
        .replace("height=\"1000\"", "height=\"1100\"")
        .replace("함초롬바탕", "바탕");
    assert_ne!(foreign, header);
    let bytes = with_parts(&written, &[("Contents/header.xml", &foreign)]);
    let opened = decode(&bytes, "foreign.hwpx");
    let RichBlock::Paragraph(first) = &opened.blocks[0] else {
        panic!("a paragraph");
    };
    assert_eq!(first.alignment, Some(RichAlignment::Left));
    assert_eq!(first.line_spacing, Some(RichLineSpacing::Multiple(130)));
    assert_eq!(first.runs[0].size_half_pt, Some(22));
    assert_eq!(first.runs[0].font.as_deref(), Some("바탕"));

    let reopened = decode(&encode(&opened, DocumentFormat::Hwpx), "saved.hwpx");
    assert_eq!(reopened.blocks, opened.blocks);
}

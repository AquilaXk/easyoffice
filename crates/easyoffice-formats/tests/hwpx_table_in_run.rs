//! A table inside a paragraph's run (`<hp:run><hp:t>텍스트</hp:t><hp:tbl>…</hp:tbl></hp:run>`) comes
//! after the text that precedes it in the paragraph, and before the text that follows it.

use easyoffice_formats::{DecodedContent, RichBlock, UniversalFormatConverter, ZipArchiveCodec};

const HEADER: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
    "<hh:head xmlns:hh=\"http://www.owpml.org/owpml/2011/head\" xmlns:hc=\"http://www.owpml.org/owpml/2011/core\">",
    "<hh:refList></hh:refList></hh:head>"
);

const TABLE: &str = concat!(
    "<hp:tbl rowCnt=\"1\" colCnt=\"2\"><hp:tr>",
    "<hp:tc><hp:subList><hp:p><hp:run charPrIDRef=\"0\"><hp:t>가</hp:t></hp:run></hp:p></hp:subList>",
    "<hp:cellAddr colAddr=\"0\" rowAddr=\"0\"/></hp:tc>",
    "<hp:tc><hp:subList><hp:p><hp:run charPrIDRef=\"0\"><hp:t>나</hp:t></hp:run></hp:p></hp:subList>",
    "<hp:cellAddr colAddr=\"1\" rowAddr=\"0\"/></hp:tc>",
    "</hp:tr></hp:tbl>"
);

fn kinds(section_body: &str) -> Vec<String> {
    let section = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><hs:sec xmlns:hs=\"http://www.owpml.org/owpml/2011/section\" xmlns:hp=\"http://www.owpml.org/owpml/2011/paragraph\">{section_body}</hs:sec>"
    );
    let bytes = ZipArchiveCodec::pack(&[
        ("mimetype".to_string(), b"application/hwp+zip".to_vec()),
        (
            "Contents/header.xml".to_string(),
            HEADER.as_bytes().to_vec(),
        ),
        ("Contents/section0.xml".to_string(), section.into_bytes()),
    ])
    .unwrap();
    let decoded = UniversalFormatConverter::decode_rich(&bytes, Some("a.hwpx")).unwrap();
    let DecodedContent::Document(doc) = decoded.content else {
        panic!("document expected");
    };
    doc.blocks
        .iter()
        .map(|b| match b {
            RichBlock::Paragraph(p) => format!("P:{}", p.plain_text()),
            RichBlock::Table(t) => format!(
                "T:{}",
                t.rows
                    .iter()
                    .map(|r| r.join(","))
                    .collect::<Vec<_>>()
                    .join(";")
            ),
            RichBlock::Image(_) => "I".to_string(),
        })
        .collect()
}

#[test]
fn text_before_a_table_in_the_same_run_comes_first() {
    let body = format!(
        "<hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\"><hp:t>앞 텍스트</hp:t>{TABLE}</hp:run></hp:p>"
    );
    assert_eq!(kinds(&body), ["P:앞 텍스트", "T:가,나"]);
}

#[test]
fn text_after_the_table_in_the_paragraph_follows_it() {
    let body = format!(
        "<hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\"><hp:t>앞</hp:t>{TABLE}</hp:run><hp:run charPrIDRef=\"0\"><hp:t>뒤</hp:t></hp:run></hp:p>"
    );
    assert_eq!(kinds(&body), ["P:앞", "T:가,나", "P:뒤"]);
}

#[test]
fn a_table_only_paragraph_and_neighbouring_paragraphs_keep_their_order() {
    let body = format!(
        "<hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\"><hp:t>위</hp:t></hp:run></hp:p><hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\">{TABLE}</hp:run></hp:p><hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\"><hp:t>아래</hp:t></hp:run></hp:p>"
    );
    assert_eq!(kinds(&body), ["P:위", "T:가,나", "P:아래"]);
}

#[test]
fn text_in_an_earlier_run_of_the_paragraph_also_precedes_the_table() {
    let body = format!(
        "<hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\"><hp:t>첫 런</hp:t></hp:run><hp:run charPrIDRef=\"0\">{TABLE}</hp:run></hp:p>"
    );
    assert_eq!(kinds(&body), ["P:첫 런", "T:가,나"]);
}

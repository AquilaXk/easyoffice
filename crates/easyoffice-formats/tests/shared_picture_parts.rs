//! A picture used several times in a document is stored once in the saved DOCX, ODT or HWPX
//! package and every reference points to that part (PD-184). Bytes are compared on every hash
//! match, so a different picture is never merged.

use easyoffice_formats::{
    DecodedContent, DocumentFormat, RichBlock, RichDocument, RichImage, RichParagraph, RichRun,
    UniversalFormatConverter, ZipArchiveCodec, ZipSecurityPolicy,
};
use std::collections::HashSet;

#[allow(dead_code)]
mod image_fixture;

const REPEATS: usize = 40;

fn picture(png: Vec<u8>, alt: &str) -> RichImage {
    RichImage::new(png, alt).unwrap()
}

/// `(document, expected picture bytes in order)`: one picture 40 times (clones), a second one
/// twice, a separately built copy of the first, and a third that has the first's size.
fn document() -> (RichDocument, Vec<Vec<u8>>) {
    let first = picture(image_fixture::make_png(64, 48, 2, 3), "첫째");
    let second = picture(image_fixture::make_png(32, 32, 6, 4), "둘째");
    let rebuilt = picture(image_fixture::make_png(64, 48, 2, 3), "복제");
    let third = picture(image_fixture::make_png(64, 48, 6, 4), "셋째");
    let mut images = vec![first.clone(); REPEATS];
    images.extend([second.clone(), second, rebuilt, third]);
    let mut blocks = Vec::new();
    for (index, image) in images.iter().enumerate() {
        blocks.push(RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun::plain(format!("문단 {index}"))],
            ..RichParagraph::default()
        }));
        blocks.push(RichBlock::Image(image.clone()));
    }
    let expected = images.iter().map(|i| i.bytes().to_vec()).collect();
    let doc = RichDocument {
        title: "공유 그림".into(),
        blocks,
        official_agency: None,
    };
    (doc, expected)
}

fn entries(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
        .unwrap()
        .into_iter()
        .map(|e| (e.path, e.data))
        .collect()
}

fn text_of(entries: &[(String, Vec<u8>)], path: &str) -> String {
    let (_, data) = entries.iter().find(|(p, _)| p == path).unwrap();
    String::from_utf8(data.clone()).unwrap()
}

/// The text between each `prefix` in `xml` and the next quote, in order.
fn quoted_after<'a>(xml: &'a str, prefix: &str) -> Vec<&'a str> {
    xml.match_indices(prefix)
        .map(|(at, _)| {
            let rest = &xml[at + prefix.len()..];
            &rest[..rest.find('"').unwrap()]
        })
        .collect()
}

/// Values of `attribute="…"` in `xml`, in order.
fn attribute_values<'a>(xml: &'a str, attribute: &str) -> Vec<&'a str> {
    quoted_after(xml, &format!("{attribute}=\""))
}

fn distinct(values: &[&str]) -> usize {
    values.iter().collect::<HashSet<_>>().len()
}

fn media_parts<'a>(entries: &'a [(String, Vec<u8>)], dir: &str) -> Vec<&'a (String, Vec<u8>)> {
    entries.iter().filter(|(p, _)| p.starts_with(dir)).collect()
}

fn encode(format: DocumentFormat) -> Vec<u8> {
    UniversalFormatConverter::encode_rich(&document().0, format, None).unwrap()
}

fn read_back(bytes: &[u8], name: &str) -> Vec<Vec<u8>> {
    let decoded = UniversalFormatConverter::decode_rich(bytes, Some(name)).unwrap();
    let DecodedContent::Document(doc) = decoded.content else {
        panic!("document expected");
    };
    doc.blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Image(i) => Some(i.bytes().to_vec()),
            _ => None,
        })
        .collect()
}

const REFERENCES: usize = REPEATS + 4;
const PARTS: usize = 3;

#[test]
fn docx_stores_a_repeated_picture_once() {
    let bytes = encode(DocumentFormat::Docx);
    let entries = entries(&bytes);
    let parts = media_parts(&entries, "word/media/");
    assert_eq!(parts.len(), PARTS);
    let document = text_of(&entries, "word/document.xml");
    let embeds = attribute_values(&document, "r:embed");
    assert_eq!((embeds.len(), distinct(&embeds)), (REFERENCES, PARTS));
    let drawing_ids = quoted_after(&document, "<wp:docPr id=\"");
    assert_eq!(
        distinct(&drawing_ids),
        REFERENCES,
        "drawing ids stay unique"
    );
    let rels = text_of(&entries, "word/_rels/document.xml.rels");
    let targets = attribute_values(&rels, "Target")
        .into_iter()
        .filter(|t| t.starts_with("media/"))
        .count();
    assert_eq!(targets, PARTS);
    assert_eq!(read_back(&bytes, "a.docx"), document_expected());
}

#[test]
fn odt_stores_a_repeated_picture_once() {
    let bytes = encode(DocumentFormat::Odt);
    let entries = entries(&bytes);
    assert_eq!(media_parts(&entries, "Pictures/").len(), PARTS);
    let content = text_of(&entries, "content.xml");
    let hrefs = attribute_values(&content, "xlink:href");
    assert_eq!((hrefs.len(), distinct(&hrefs)), (REFERENCES, PARTS));
    let names = attribute_values(&content, "draw:name");
    assert_eq!((names.len(), distinct(&names)), (REFERENCES, REFERENCES));
    let manifest = text_of(&entries, "META-INF/manifest.xml");
    assert_eq!(manifest.matches("Pictures/image").count(), PARTS);
    assert_eq!(read_back(&bytes, "a.odt"), document_expected());
}

#[test]
fn hwpx_stores_a_repeated_picture_once() {
    let bytes = encode(DocumentFormat::Hwpx);
    let entries = entries(&bytes);
    assert_eq!(media_parts(&entries, "BinData/").len(), PARTS);
    let section = text_of(&entries, "Contents/section0.xml");
    let refs = attribute_values(&section, "binaryItemIDRef");
    assert_eq!((refs.len(), distinct(&refs)), (REFERENCES, PARTS));
    let ids = attribute_values(&section, "instid");
    assert_eq!((ids.len(), distinct(&ids)), (REFERENCES, REFERENCES));
    let package = text_of(&entries, "Contents/content.hpf");
    assert_eq!(package.matches("isEmbeded=\"1\"").count(), PARTS);
    assert_eq!(read_back(&bytes, "a.hwpx"), document_expected());
}

fn document_expected() -> Vec<Vec<u8>> {
    document().1
}

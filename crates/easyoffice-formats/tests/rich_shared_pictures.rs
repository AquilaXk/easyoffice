//! A picture referenced several times in one document is validated and copied once: the readers
//! keep one checked picture per media entry and every reference shares its bytes.

use easyoffice_formats::{
    DecodedContent, DocumentFormat, RichBlock, RichDocument, RichImage, UniversalFormatConverter,
    ZipArchiveCodec, ZipSecurityPolicy,
};

mod image_fixture;

const REFERENCES: usize = 40;

fn picture_blocks(doc: &RichDocument) -> Vec<&RichImage> {
    doc.blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Image(img) => Some(img),
            _ => None,
        })
        .collect()
}

/// A DOCX whose body repeats the one picture paragraph `REFERENCES` times, all pointing at the
/// same media entry.
fn docx_with_repeated_picture() -> Vec<u8> {
    docx_with_repeated_picture_of(&image_fixture::make_png(64, 48, 2, 3), REFERENCES)
}

fn docx_with_repeated_picture_of(png: &[u8], references: usize) -> Vec<u8> {
    let single = RichDocument {
        title: "한 번".into(),
        blocks: vec![RichBlock::Image(
            RichImage::new(png.to_vec(), "그림").unwrap(),
        )],
        official_agency: None,
    };
    let docx = UniversalFormatConverter::encode_rich(&single, DocumentFormat::Docx, None).unwrap();
    let entries = ZipArchiveCodec::unpack(&docx, &ZipSecurityPolicy::default()).unwrap();
    let mut parts: Vec<(String, Vec<u8>)> = entries.into_iter().map(|e| (e.path, e.data)).collect();
    let (_, document) = parts
        .iter_mut()
        .find(|(path, _)| path == "word/document.xml")
        .unwrap();
    let xml = String::from_utf8(document.clone()).unwrap();
    let start = xml.find("<w:p>").or_else(|| xml.find("<w:p ")).unwrap();
    let end = xml.rfind("</w:p>").unwrap() + "</w:p>".len();
    let repeated = xml[start..end].repeat(references);
    *document = format!("{}{}{}", &xml[..start], repeated, &xml[end..]).into_bytes();
    ZipArchiveCodec::pack(&parts).unwrap()
}

#[test]
fn a_picture_used_many_times_shares_one_copy_of_its_bytes() {
    let decoded =
        UniversalFormatConverter::decode_rich(&docx_with_repeated_picture(), Some("docx")).unwrap();
    let DecodedContent::Document(doc) = &decoded.content else {
        panic!("a document");
    };
    let pictures = picture_blocks(doc);
    assert_eq!(pictures.len(), REFERENCES);
    let first = pictures[0].bytes().as_ptr();
    assert!(
        pictures.iter().all(|p| p.bytes().as_ptr() == first),
        "every reference shares the first copy"
    );
    assert!(pictures.iter().all(|p| p.alt() == "그림"));
}

/// Measurement for `docs/performance.md`, not a check:
/// `cargo test --release -p easyoffice-formats --test rich_shared_pictures -- --ignored --nocapture`.
#[test]
#[ignore = "measurement"]
fn measure_repeated_picture_decode() {
    let docx = docx_with_repeated_picture_of(&image_fixture::make_png(2400, 1600, 2, 3), 100);
    let mut best = f64::MAX;
    for _ in 0..15 {
        let started = std::time::Instant::now();
        let decoded = UniversalFormatConverter::decode_rich(&docx, Some("docx")).unwrap();
        best = best.min(started.elapsed().as_secs_f64() * 1000.0);
        std::hint::black_box(decoded);
    }
    println!(
        "docx {} bytes, 100 references: best of 15 = {best:.2} ms",
        docx.len()
    );
}

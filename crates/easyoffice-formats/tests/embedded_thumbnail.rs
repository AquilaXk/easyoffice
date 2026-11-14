//! Preview picture embedded in saved HWP, HWPX and DOCX files (PD-132): the picture lands in the
//! format's own place, nothing else in the document changes, and bad input is refused.

use easyoffice_formats::{
    embed_thumbnail, supports_thumbnail, DocumentFormat, EasyOfficeError, Ole2ContainerCodec,
    RichBlock, RichDocument, RichImage, RichParagraph, RichRun, UniversalDocumentModel,
    UniversalFormatConverter, ZipArchiveCodec, ZipSecurityPolicy, MAX_EMBED_DOCUMENT_BYTES,
    MAX_THUMBNAIL_PNG_BYTES, MAX_THUMBNAIL_SIDE_PX,
};

#[allow(dead_code)]
mod image_fixture;
use image_fixture::{make_png, png_chunk};

fn thumbnail() -> Vec<u8> {
    make_png(181, 256, 2, 3)
}

fn model() -> UniversalDocumentModel {
    UniversalDocumentModel {
        title: "미리보기".to_string(),
        paragraphs: vec!["본문 한 줄".to_string(), "둘째 줄".to_string()],
        ..Default::default()
    }
}

fn rich() -> RichDocument {
    RichDocument {
        page_frame: Default::default(),
        title: "미리보기".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun::plain("본문 한 줄")],
            ..RichParagraph::default()
        })],
        official_agency: None,
    }
}

fn hwp() -> Vec<u8> {
    UniversalFormatConverter::encode(&model(), DocumentFormat::Hwp).unwrap()
}

fn hwpx() -> Vec<u8> {
    UniversalFormatConverter::encode_rich(&rich(), DocumentFormat::Hwpx, None).unwrap()
}

fn docx() -> Vec<u8> {
    UniversalFormatConverter::encode_rich(&rich(), DocumentFormat::Docx, None).unwrap()
}

fn streams(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut all: Vec<_> = Ole2ContainerCodec::unpack(bytes)
        .unwrap()
        .into_iter()
        .map(|s| (s.name, s.data))
        .collect();
    all.sort();
    all
}

fn parts(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
        .unwrap()
        .into_iter()
        .map(|e| (e.path, e.data))
        .collect()
}

fn part(bytes: &[u8], path: &str) -> Option<Vec<u8>> {
    parts(bytes)
        .into_iter()
        .find(|(name, _)| name == path)
        .map(|(_, data)| data)
}

fn text(bytes: &[u8], path: &str) -> String {
    String::from_utf8(part(bytes, path).unwrap_or_else(|| panic!("missing {path}"))).unwrap()
}

fn without(mut all: Vec<(String, Vec<u8>)>, name: &str) -> Vec<(String, Vec<u8>)> {
    all.retain(|(n, _)| n != name);
    all.sort();
    all
}

#[test]
fn only_the_three_formats_with_a_preview_part_are_supported() {
    for format in [
        DocumentFormat::Hwp,
        DocumentFormat::Hwt,
        DocumentFormat::Hwpx,
        DocumentFormat::Hwtx,
        DocumentFormat::Docx,
    ] {
        assert!(supports_thumbnail(format), "{format:?}");
    }
    for format in [
        DocumentFormat::Pdf,
        DocumentFormat::Xlsx,
        DocumentFormat::Odt,
    ] {
        assert!(!supports_thumbnail(format), "{format:?}");
        let err = embed_thumbnail(format, b"x", &thumbnail()).unwrap_err();
        assert!(matches!(err, EasyOfficeError::UnsupportedExtension(_)));
    }
}

#[test]
fn hwp_gets_a_prv_image_stream_and_keeps_every_other_stream() {
    let original = hwp();
    let png = thumbnail();
    let saved = embed_thumbnail(DocumentFormat::Hwp, &original, &png).unwrap();
    let after = streams(&saved);
    let image = after
        .iter()
        .find(|(n, _)| n == "PrvImage")
        .expect("PrvImage");
    assert_eq!(image.1, png);
    assert_eq!(
        without(after, "PrvImage"),
        without(streams(&original), "PrvImage")
    );
    // The result is still a complete HWP container: the header stream is intact.
    assert!(streams(&saved)
        .iter()
        .any(|(n, d)| n == "FileHeader" && d.starts_with(b"HWP Document File")));
}

#[test]
fn an_existing_prv_image_is_replaced_not_duplicated() {
    let png = thumbnail();
    let first = embed_thumbnail(DocumentFormat::Hwp, &hwp(), &make_png(90, 128, 2, 3)).unwrap();
    let second = embed_thumbnail(DocumentFormat::Hwp, &first, &png).unwrap();
    let after = streams(&second);
    assert_eq!(after.iter().filter(|(n, _)| n == "PrvImage").count(), 1);
    assert_eq!(after.iter().find(|(n, _)| n == "PrvImage").unwrap().1, png);
}

#[test]
fn hwpx_gets_preview_prv_image_png_and_keeps_mimetype_first() {
    let original = hwpx();
    let png = thumbnail();
    let saved = embed_thumbnail(DocumentFormat::Hwpx, &original, &png).unwrap();
    assert_eq!(part(&saved, "Preview/PrvImage.png").unwrap(), png);
    assert_eq!(parts(&saved)[0].0, "mimetype");
    for (name, data) in parts(&original) {
        assert_eq!(
            part(&saved, &name).as_deref(),
            Some(data.as_slice()),
            "{name}"
        );
    }
}

#[test]
fn an_existing_hwpx_preview_is_replaced_and_other_entries_stay() {
    let png = thumbnail();
    let mut entries = parts(&hwpx());
    entries.push((
        "Preview/PrvImage.png".to_string(),
        vec![0x89, b'P', b'N', b'G', 1, 2, 3],
    ));
    entries.push(("Preview/PrvText.txt".to_string(), b"keep".to_vec()));
    let stale = ZipArchiveCodec::pack_compressed(&entries).unwrap();
    let saved = embed_thumbnail(DocumentFormat::Hwpx, &stale, &png).unwrap();
    assert_eq!(
        parts(&saved)
            .iter()
            .filter(|(n, _)| n == "Preview/PrvImage.png")
            .count(),
        1
    );
    assert_eq!(part(&saved, "Preview/PrvImage.png").unwrap(), png);
    assert_eq!(part(&saved, "Preview/PrvText.txt").unwrap(), b"keep");
}

#[test]
fn docx_gets_the_part_the_relationship_and_the_png_content_type() {
    let original = docx();
    let png = thumbnail();
    let saved = embed_thumbnail(DocumentFormat::Docx, &original, &png).unwrap();
    assert_eq!(part(&saved, "docProps/thumbnail.png").unwrap(), png);
    let rels = text(&saved, "_rels/.rels");
    assert!(rels.contains(
        "Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/thumbnail\""
    ));
    assert!(rels.contains("Target=\"docProps/thumbnail.png\""));
    // The original relationships are all still there.
    assert!(rels.contains("Target=\"word/document.xml\""));
    assert!(rels.contains("Target=\"docProps/core.xml\""));
    let types = text(&saved, "[Content_Types].xml");
    assert_eq!(types.matches("Extension=\"png\"").count(), 1);
    for (name, data) in parts(&original) {
        if name != "_rels/.rels" && name != "[Content_Types].xml" {
            assert_eq!(
                part(&saved, &name).as_deref(),
                Some(data.as_slice()),
                "{name}"
            );
        }
    }
    // The package still opens as a document.
    UniversalFormatConverter::decode_rich(&saved, Some("docx")).unwrap();
}

#[test]
fn docx_with_pictures_does_not_get_a_second_png_default() {
    let doc = RichDocument {
        blocks: vec![RichBlock::Image(
            RichImage::new(make_png(40, 30, 2, 3), "그림").unwrap(),
        )],
        ..rich()
    };
    let original = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None).unwrap();
    assert_eq!(
        text(&original, "[Content_Types].xml")
            .matches("Extension=\"png\"")
            .count(),
        1
    );
    let saved = embed_thumbnail(DocumentFormat::Docx, &original, &thumbnail()).unwrap();
    assert_eq!(
        text(&saved, "[Content_Types].xml")
            .matches("Extension=\"png\"")
            .count(),
        1
    );
    assert!(part(&saved, "word/media/image1.png").is_some());
}

#[test]
fn docx_that_points_at_a_png_thumbnail_gets_that_part_replaced() {
    let png = thumbnail();
    let once = embed_thumbnail(DocumentFormat::Docx, &docx(), &make_png(90, 128, 2, 3)).unwrap();
    let twice = embed_thumbnail(DocumentFormat::Docx, &once, &png).unwrap();
    assert_eq!(part(&twice, "docProps/thumbnail.png").unwrap(), png);
    assert_eq!(
        parts(&twice)
            .iter()
            .filter(|(n, _)| n.starts_with("docProps/thumbnail"))
            .count(),
        1
    );
    assert_eq!(
        text(&twice, "_rels/.rels")
            .matches("metadata/thumbnail")
            .count(),
        1
    );
}

#[test]
fn docx_with_a_non_png_thumbnail_is_refused_not_given_two() {
    let mut entries = parts(&docx());
    for (name, data) in &mut entries {
        if name == "_rels/.rels" {
            let rels = String::from_utf8(data.clone()).unwrap().replace(
                "</Relationships>",
                "<Relationship Id=\"rId9\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/thumbnail\" Target=\"docProps/thumbnail.jpeg\"/></Relationships>",
            );
            *data = rels.into_bytes();
        }
    }
    entries.push((
        "docProps/thumbnail.jpeg".to_string(),
        vec![0xFF, 0xD8, 0xFF, 0xD9],
    ));
    let package = ZipArchiveCodec::pack_compressed(&entries).unwrap();
    assert!(embed_thumbnail(DocumentFormat::Docx, &package, &thumbnail()).is_err());
}

#[test]
fn the_rich_writer_adds_the_picture_only_when_the_caller_gives_one() {
    let png = thumbnail();
    let doc = rich();
    let plain = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None).unwrap();
    let none = UniversalFormatConverter::encode_rich_with_thumbnail(
        &doc,
        DocumentFormat::Docx,
        None,
        None,
    )
    .unwrap();
    assert_eq!(none, plain);
    assert!(part(&none, "docProps/thumbnail.png").is_none());
    let with = UniversalFormatConverter::encode_rich_with_thumbnail(
        &doc,
        DocumentFormat::Docx,
        None,
        Some(&png),
    )
    .unwrap();
    assert_eq!(part(&with, "docProps/thumbnail.png").unwrap(), png);
    // A format without a preview part ignores the picture.
    let odt = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Odt, None).unwrap();
    let odt_with = UniversalFormatConverter::encode_rich_with_thumbnail(
        &doc,
        DocumentFormat::Odt,
        None,
        Some(&png),
    )
    .unwrap();
    assert_eq!(odt_with, odt);
}

#[test]
fn a_picture_that_is_not_a_valid_small_png_is_refused_for_every_format() {
    let mut bad_crc = thumbnail();
    let last = bad_crc.len() - 20;
    bad_crc[last] ^= 0xFF;
    let mut padded = thumbnail();
    let iend = padded.len() - 12;
    let filler = png_chunk(b"tEXt", &vec![b'a'; MAX_THUMBNAIL_PNG_BYTES]);
    padded.splice(iend..iend, filler);
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("not a png", b"GIF89a....".to_vec()),
        ("empty", Vec::new()),
        ("corrupt crc", bad_crc),
        ("too many bytes", padded),
        ("too wide", make_png(MAX_THUMBNAIL_SIDE_PX + 1, 10, 2, 3)),
        ("too tall", make_png(10, MAX_THUMBNAIL_SIDE_PX + 1, 2, 3)),
    ];
    for (label, png) in cases {
        for (format, document) in [
            (DocumentFormat::Hwp, hwp()),
            (DocumentFormat::Hwpx, hwpx()),
            (DocumentFormat::Docx, docx()),
        ] {
            assert!(
                embed_thumbnail(format, &document, &png).is_err(),
                "{label} accepted for {format:?}"
            );
        }
    }
    // The limits are inclusive.
    let edge = make_png(MAX_THUMBNAIL_SIDE_PX, MAX_THUMBNAIL_SIDE_PX, 2, 3);
    assert!(edge.len() <= MAX_THUMBNAIL_PNG_BYTES);
    assert!(embed_thumbnail(DocumentFormat::Docx, &docx(), &edge).is_ok());
}

#[test]
fn documents_over_the_rewrite_budget_and_broken_containers_are_refused() {
    let png = thumbnail();
    let huge = vec![0_u8; MAX_EMBED_DOCUMENT_BYTES + 1];
    for format in [
        DocumentFormat::Hwp,
        DocumentFormat::Hwpx,
        DocumentFormat::Docx,
    ] {
        assert!(matches!(
            embed_thumbnail(format, &huge, &png),
            Err(EasyOfficeError::PayloadTooLarge { .. })
        ));
        assert!(embed_thumbnail(format, b"not a container", &png).is_err());
        assert!(embed_thumbnail(format, &[], &png).is_err());
    }
    // A cut-off container is refused as well, never half-rewritten.
    let mut cut = docx();
    cut.truncate(cut.len() / 2);
    assert!(embed_thumbnail(DocumentFormat::Docx, &cut, &png).is_err());
    let mut cut_hwp = hwp();
    cut_hwp.truncate(cut_hwp.len() / 2);
    assert!(embed_thumbnail(DocumentFormat::Hwp, &cut_hwp, &png).is_err());
}

#[test]
fn hostile_docx_relationship_targets_and_hwpx_compression_bomb_fail_closed() {
    let png = thumbnail();
    for (label, rel_attr) in [
        ("dotdot traversal", "Target=\"../../evil.png\""),
        ("internal parent segment", "Target=\"docProps/../evil.png\""),
        ("external url", "Target=\"https://evil.example/thumb.png\""),
        (
            "external mode",
            "Target=\"docProps/thumbnail.png\" TargetMode=\"External\"",
        ),
    ] {
        let mut entries = parts(&docx());
        for (name, data) in &mut entries {
            if name == "_rels/.rels" {
                let rels = String::from_utf8(data.clone()).unwrap().replace(
                    "</Relationships>",
                    &format!(
                        "<Relationship Id=\"rId9\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/thumbnail\" {rel_attr}/></Relationships>"
                    ),
                );
                *data = rels.into_bytes();
            }
        }
        let package = ZipArchiveCodec::pack_compressed(&entries).unwrap();
        assert!(
            embed_thumbnail(DocumentFormat::Docx, &package, &png).is_err(),
            "{label} was accepted"
        );
    }

    // Build an HWPX entry whose deflate compression ratio exceeds 1024.0 (e.g. 12 MiB of zeros).
    let mut bomb_entries = parts(&hwpx());
    bomb_entries.push((
        "Contents/filler.xml".to_string(),
        vec![b'0'; 12 * 1024 * 1024],
    ));
    let bomb_hwpx = ZipArchiveCodec::pack_deflate(&bomb_entries).unwrap();
    assert!(matches!(
        embed_thumbnail(DocumentFormat::Hwpx, &bomb_hwpx, &png),
        Err(EasyOfficeError::DecompressionBombDetected { .. })
    ));
}

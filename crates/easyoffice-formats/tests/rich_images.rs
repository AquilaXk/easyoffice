//! Picture blocks: validation limits, writer output (media parts, references, byte identity),
//! DOCX round trip and the fallback text of formats without picture support.

use easyoffice_formats::{
    deflate_zlib, DecodedContent, DocumentFormat, RichBlock, RichDocument, RichImage,
    RichImageFormat, RichParagraph, RichRun, UniversalFormatConverter, ZipArchiveCodec,
    ZipSecurityPolicy, MAX_IMAGES_PER_DOCUMENT, MAX_IMAGE_BYTES,
};

mod image_fixture;
use image_fixture::{make_jpeg, make_png, png_chunk, PNG_SIGNATURE};

fn doc_with(images: Vec<RichImage>) -> RichDocument {
    let mut blocks = vec![RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun::plain("앞 문단")],
        ..RichParagraph::default()
    })];
    blocks.extend(images.into_iter().map(RichBlock::Image));
    blocks.push(RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun::plain("뒤 문단")],
        ..RichParagraph::default()
    }));
    RichDocument {
        page_frame: Default::default(),
        title: "그림".to_string(),
        blocks,
        official_agency: None,
    }
}

fn two_images() -> (Vec<u8>, Vec<u8>, RichDocument) {
    let png = make_png(40, 30, 2, 3);
    let jpeg = make_jpeg(64, 48);
    let doc = doc_with(vec![
        RichImage::new(png.clone(), "파란 사각형").unwrap(),
        RichImage::new(jpeg.clone(), "사진 <a&b>").unwrap(),
    ]);
    (png, jpeg, doc)
}

fn entry(bytes: &[u8], path: &str) -> Vec<u8> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    entries
        .into_iter()
        .find(|e| e.path == path)
        .unwrap_or_else(|| panic!("missing {path}"))
        .data
}

fn text(bytes: &[u8], path: &str) -> String {
    String::from_utf8(entry(bytes, path)).unwrap()
}

fn encode(doc: &RichDocument, format: DocumentFormat) -> Vec<u8> {
    UniversalFormatConverter::encode_rich(doc, format, None).unwrap()
}

#[test]
fn model_reads_pixel_size_and_default_display_size() {
    let img = RichImage::new(make_png(40, 30, 2, 3), "x").unwrap();
    assert_eq!(img.format(), RichImageFormat::Png);
    assert_eq!((img.width_px(), img.height_px()), (40, 30));
    assert_eq!(img.display_emu(), (40 * 9525, 30 * 9525));
    let wide = RichImage::new(make_jpeg(4000, 1000), "").unwrap();
    let (w, h) = wide.display_emu();
    assert_eq!(w, 5_731_510);
    assert_eq!(h, 1_432_877);
    assert_eq!(wide.fallback_text(), "[그림]");
}

#[test]
fn damaged_unsupported_and_oversized_pictures_are_rejected() {
    let png = make_png(8, 8, 2, 3);
    assert!(RichImage::new(b"GIF89a....".to_vec(), "").is_err());
    assert!(RichImage::new(Vec::new(), "").is_err());
    assert!(RichImage::new(png[..png.len() - 5].to_vec(), "").is_err());
    let mut flipped = png.clone();
    flipped[40] ^= 0x01;
    assert!(RichImage::new(flipped, "").is_err(), "CRC mismatch");
    let mut trailing = png.clone();
    trailing.push(0);
    assert!(RichImage::new(trailing, "").is_err(), "data after IEND");
    // A header claiming far more pixels than the limit, with a correct CRC.
    let mut huge = PNG_SIGNATURE.to_vec();
    let mut ihdr = 60_000u32.to_be_bytes().to_vec();
    ihdr.extend_from_slice(&60_000u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    huge.extend(png_chunk(b"IHDR", &ihdr));
    huge.extend(png_chunk(b"IDAT", &[0x78, 0x01]));
    huge.extend(png_chunk(b"IEND", &[]));
    assert!(RichImage::new(huge, "").is_err(), "pixel limit");
    let jpeg = make_jpeg(10, 10);
    assert!(RichImage::new(jpeg[..jpeg.len() - 2].to_vec(), "").is_err());
    let mut oversized = png;
    oversized.resize(MAX_IMAGE_BYTES + 1, 0);
    assert!(RichImage::new(oversized, "").is_err());
}

#[test]
fn too_many_pictures_fail_closed_in_every_picture_writer() {
    let img = RichImage::new(make_png(2, 2, 2, 3), "").unwrap();
    let doc = doc_with(vec![img; MAX_IMAGES_PER_DOCUMENT + 1]);
    for format in [
        DocumentFormat::Docx,
        DocumentFormat::Odt,
        DocumentFormat::Hwpx,
    ] {
        assert!(
            UniversalFormatConverter::encode_rich(&doc, format, None).is_err(),
            "{format:?}"
        );
    }
}

#[test]
fn docx_writer_stores_media_relationships_and_inline_drawings() {
    let (png, jpeg, doc) = two_images();
    let bytes = encode(&doc, DocumentFormat::Docx);
    assert_eq!(entry(&bytes, "word/media/image1.png"), png);
    assert_eq!(entry(&bytes, "word/media/image2.jpg"), jpeg);
    let types = text(&bytes, "[Content_Types].xml");
    assert!(types.contains("<Default Extension=\"png\" ContentType=\"image/png\"/>"));
    assert!(types.contains("<Default Extension=\"jpg\" ContentType=\"image/jpeg\"/>"));
    let rels = text(&bytes, "word/_rels/document.xml.rels");
    assert!(rels.contains("Id=\"rId2\""), "{rels}");
    assert!(rels.contains("Target=\"media/image1.png\""), "{rels}");
    assert!(rels.contains("Id=\"rId3\""), "{rels}");
    assert!(rels.contains("Target=\"media/image2.jpg\""), "{rels}");
    let xml = text(&bytes, "word/document.xml");
    assert!(xml.contains("<a:blip r:embed=\"rId2\"/>"), "{xml}");
    assert!(xml.contains("<a:blip r:embed=\"rId3\"/>"), "{xml}");
    assert!(xml.contains(&format!(
        "<wp:extent cx=\"{}\" cy=\"{}\"/>",
        40 * 9525,
        30 * 9525
    )));
    assert!(xml.contains("descr=\"사진 &lt;a&amp;b&gt;\""), "{xml}");
    let before = xml.find("앞 문단").unwrap();
    let drawing = xml.find("<w:drawing>").unwrap();
    let after = xml.find("뒤 문단").unwrap();
    assert!(before < drawing && drawing < after, "document order");
}

#[test]
fn docx_roundtrip_keeps_picture_bytes_alt_size_and_position() {
    let (png, jpeg, doc) = two_images();
    let bytes = encode(&doc, DocumentFormat::Docx);
    let decoded = UniversalFormatConverter::decode_rich(&bytes, Some("a.docx")).unwrap();
    let DecodedContent::Document(back) = decoded.content else {
        panic!("document expected");
    };
    assert_eq!(back.blocks, doc.blocks);
    let RichBlock::Image(first) = &back.blocks[1] else {
        panic!("image at index 1");
    };
    assert_eq!(first.bytes(), png.as_slice());
    assert_eq!(first.alt(), "파란 사각형");
    let RichBlock::Image(second) = &back.blocks[2] else {
        panic!("image at index 2");
    };
    assert_eq!(second.bytes(), jpeg.as_slice());
    assert_eq!(second.alt(), "사진 <a&b>");
}

#[test]
fn docx_reader_skips_a_damaged_media_part_with_a_warning() {
    let (_, _, doc) = two_images();
    let bytes = encode(&doc, DocumentFormat::Docx);
    let mut entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    for e in &mut entries {
        if e.path == "word/media/image1.png" {
            e.data[30] ^= 0xFF;
        }
    }
    let pairs: Vec<(String, Vec<u8>)> = entries.into_iter().map(|e| (e.path, e.data)).collect();
    let damaged = ZipArchiveCodec::pack_compressed(&pairs).unwrap();
    let decoded = UniversalFormatConverter::decode_rich(&damaged, Some("a.docx")).unwrap();
    let DecodedContent::Document(back) = decoded.content else {
        panic!("document expected");
    };
    let images = back
        .blocks
        .iter()
        .filter(|b| matches!(b, RichBlock::Image(_)))
        .count();
    assert_eq!(images, 1);
    assert!(decoded.warnings.iter().any(|w| w.contains("image1.png")));
}

#[test]
fn odt_writer_stores_pictures_manifest_and_frames() {
    let (png, jpeg, doc) = two_images();
    let bytes = encode(&doc, DocumentFormat::Odt);
    assert_eq!(entry(&bytes, "Pictures/image1.png"), png);
    assert_eq!(entry(&bytes, "Pictures/image2.jpg"), jpeg);
    let manifest = text(&bytes, "META-INF/manifest.xml");
    assert!(manifest
        .contains("manifest:full-path=\"Pictures/image1.png\" manifest:media-type=\"image/png\""));
    assert!(manifest
        .contains("manifest:full-path=\"Pictures/image2.jpg\" manifest:media-type=\"image/jpeg\""));
    let content = text(&bytes, "content.xml");
    assert!(
        content.contains("<draw:image xlink:href=\"Pictures/image1.png\""),
        "{content}"
    );
    assert!(content.contains("<draw:image xlink:href=\"Pictures/image2.jpg\""));
    assert!(content.contains("svg:width=\"30.0000pt\""), "{content}");
    assert!(content.contains("<svg:desc>사진 &lt;a&amp;b&gt;</svg:desc>"));
    // The writer's own output still opens.
    let decoded = UniversalFormatConverter::decode_rich(&bytes, Some("a.odt")).unwrap();
    assert!(matches!(decoded.content, DecodedContent::Document(_)));
}

#[test]
fn hwpx_writer_stores_bindata_manifest_items_and_pictures() {
    let (png, jpeg, doc) = two_images();
    let bytes = encode(&doc, DocumentFormat::Hwpx);
    assert_eq!(entry(&bytes, "BinData/image1.png"), png);
    assert_eq!(entry(&bytes, "BinData/image2.jpg"), jpeg);
    let hpf = text(&bytes, "Contents/content.hpf");
    assert!(hpf.contains("<opf:item id=\"image1\" href=\"BinData/image1.png\" media-type=\"image/png\" isEmbeded=\"1\"/>"), "{hpf}");
    assert!(hpf.contains("<opf:item id=\"image2\" href=\"BinData/image2.jpg\" media-type=\"image/jpeg\" isEmbeded=\"1\"/>"));
    let section = text(&bytes, "Contents/section0.xml");
    assert!(section.contains("binaryItemIDRef=\"image1\""), "{section}");
    assert!(section.contains("binaryItemIDRef=\"image2\""));
    assert!(section.contains("<hp:shapeComment>사진 &lt;a&amp;b&gt;</hp:shapeComment>"));
    let decoded = UniversalFormatConverter::decode_rich(&bytes, Some("a.hwpx")).unwrap();
    assert!(matches!(decoded.content, DecodedContent::Document(_)));
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn pdf_embeds_jpeg_as_dct_and_opaque_png_idat_unchanged() {
    let (png, jpeg, doc) = two_images();
    let pdf = encode(&doc, DocumentFormat::Pdf);
    assert!(contains(&pdf, b"/Filter /DCTDecode"));
    assert!(contains(&pdf, &jpeg), "JPEG bytes pass through");
    assert!(contains(&pdf, b"/Filter /FlateDecode /DecodeParms << /Predictor 15 /Colors 3 /BitsPerComponent 8 /Columns 40 >>"));
    // The PNG's IDAT payload (zlib stream) is stored as is.
    let idat_len = u32::from_be_bytes(png[8 + 25..8 + 29].try_into().unwrap()) as usize;
    let idat = &png[8 + 25 + 8..8 + 25 + 8 + idat_len];
    assert!(contains(&pdf, idat));
    assert!(contains(&pdf, b"/XObject << /Im1 "));
    assert!(contains(&pdf, b"/Type /Page "));
}

#[test]
fn pdf_splits_rgba_png_into_color_and_soft_mask_streams() {
    let doc = doc_with(vec![RichImage::new(make_png(16, 8, 6, 4), "투명").unwrap()]);
    let pdf = encode(&doc, DocumentFormat::Pdf);
    assert!(contains(&pdf, b"/SMask "));
    assert!(contains(
        &pdf,
        b"/ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode"
    ));
    assert!(contains(
        &pdf,
        b"/ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode"
    ));
}

#[test]
fn pdf_lowers_palette_pictures_to_alternative_text() {
    let mut ihdr = 4u32.to_be_bytes().to_vec();
    ihdr.extend_from_slice(&4u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 3, 0, 0, 0]);
    let mut raw = Vec::new();
    for _ in 0..4 {
        raw.extend_from_slice(&[0, 0, 0, 0, 0]);
    }
    let mut png = PNG_SIGNATURE.to_vec();
    png.extend(png_chunk(b"IHDR", &ihdr));
    png.extend(png_chunk(b"PLTE", &[1, 2, 3]));
    png.extend(png_chunk(b"IDAT", &deflate_zlib(&raw)));
    png.extend(png_chunk(b"IEND", &[]));
    let doc = doc_with(vec![RichImage::new(png, "ALTTEXT").unwrap()]);
    let pdf = encode(&doc, DocumentFormat::Pdf);
    assert!(!contains(&pdf, b"/Subtype /Image"));
    assert!(contains(&pdf, b"ALTTEXT"));
}

#[test]
fn formats_without_pictures_get_alternative_text_paragraphs() {
    let (_, _, doc) = two_images();
    let txt = String::from_utf8(encode(&doc, DocumentFormat::Txt)).unwrap();
    assert!(
        txt.contains("앞 문단\n파란 사각형\n사진 <a&b>\n뒤 문단"),
        "{txt}"
    );
    let md = String::from_utf8(encode(&doc, DocumentFormat::Markdown)).unwrap();
    assert!(md.contains("파란 사각형"), "{md}");
    let rtf = String::from_utf8(encode(&doc, DocumentFormat::Rtf)).unwrap();
    assert!(rtf.contains("파란 사각형") || rtf.contains("\\u"), "{rtf}");
    let no_alt = doc_with(vec![RichImage::new(make_jpeg(8, 8), "").unwrap()]);
    let txt = String::from_utf8(encode(&no_alt, DocumentFormat::Txt)).unwrap();
    assert!(txt.contains("[그림]"));
}

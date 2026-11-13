//! ODT and HWPX read-back: pictures (bytes, size, alt text), paragraph indents and line spacing,
//! super/subscripts, and rejection of external, escaping, damaged or too many pictures.

use easyoffice_formats::{
    DecodedContent, DocumentFormat, RichBlock, RichDocument, RichImage, RichParagraph, RichRun,
    RichScript, UniversalFormatConverter, ZipArchiveCodec, ZipSecurityPolicy,
    MAX_IMAGES_PER_DOCUMENT,
};

mod image_fixture;
use image_fixture::{make_jpeg, make_png};

fn run(text: &str, script: RichScript) -> RichRun {
    RichRun {
        text: text.to_string(),
        script,
        ..RichRun::default()
    }
}

fn sample(png: &[u8], jpeg: &[u8]) -> RichDocument {
    RichDocument {
        page_frame: Default::default(),
        title: "읽기".to_string(),
        blocks: vec![
            RichBlock::Paragraph(RichParagraph {
                runs: vec![
                    run("X", RichScript::Normal),
                    run("2", RichScript::Superscript),
                    run(" H", RichScript::Normal),
                    run("2", RichScript::Subscript),
                ],
                left_indent_twips: Some(720),
                first_line_indent_twips: Some(360),
                line_spacing_percent: Some(200),
                ..RichParagraph::default()
            }),
            RichBlock::Image(RichImage::new(png.to_vec(), "파란 <사각형> & \"별\"").unwrap()),
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("내어쓰기")],
                left_indent_twips: Some(720),
                first_line_indent_twips: Some(-360),
                ..RichParagraph::default()
            }),
            RichBlock::Image(RichImage::new(jpeg.to_vec(), "").unwrap()),
        ],
        official_agency: None,
    }
}

fn encode(doc: &RichDocument, format: DocumentFormat) -> Vec<u8> {
    UniversalFormatConverter::encode_rich(doc, format, None).unwrap()
}

fn decode(bytes: &[u8], name: &str) -> (Vec<RichBlock>, Vec<String>) {
    let decoded = UniversalFormatConverter::decode_rich(bytes, Some(name)).unwrap();
    match decoded.content {
        DecodedContent::Document(d) => (d.blocks, decoded.warnings),
        other => panic!("document expected: {other:?}"),
    }
}

fn repack(bytes: &[u8], edit: impl Fn(&str, Vec<u8>) -> Vec<u8>) -> Vec<u8> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let pairs: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            let data = edit(&e.path, e.data);
            (e.path, data)
        })
        .collect();
    ZipArchiveCodec::pack_compressed(&pairs).unwrap()
}

fn replace_in(
    path: &'static str,
    from: &'static str,
    to: String,
) -> impl Fn(&str, Vec<u8>) -> Vec<u8> {
    move |p, data| {
        if p == path {
            String::from_utf8(data)
                .unwrap()
                .replace(from, &to)
                .into_bytes()
        } else {
            data
        }
    }
}

fn images(blocks: &[RichBlock]) -> Vec<&RichImage> {
    blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Image(i) => Some(i),
            _ => None,
        })
        .collect()
}

#[test]
fn odt_roundtrip_keeps_pictures_sizes_alt_text_indents_spacing_and_scripts() {
    let (png, jpeg) = (make_png(40, 30, 2, 3), make_jpeg(64, 48));
    let doc = sample(&png, &jpeg);
    let (blocks, warnings) = decode(&encode(&doc, DocumentFormat::Odt), "a.odt");
    assert_eq!(blocks, doc.blocks, "{warnings:?}");
    let pics = images(&blocks);
    assert_eq!(pics[0].bytes(), png.as_slice());
    assert_eq!(pics[0].display_emu(), (40 * 9525, 30 * 9525));
    assert_eq!(pics[0].alt(), "파란 <사각형> & \"별\"");
    assert_eq!(pics[1].bytes(), jpeg.as_slice());
}

#[test]
fn hwpx_roundtrip_keeps_pictures_sizes_alt_text_indents_spacing_and_scripts() {
    let (png, jpeg) = (make_png(40, 30, 2, 3), make_jpeg(64, 48));
    let doc = sample(&png, &jpeg);
    let (blocks, warnings) = decode(&encode(&doc, DocumentFormat::Hwpx), "a.hwpx");
    assert_eq!(blocks[1..], doc.blocks[1..], "{warnings:?}");
    // HWPUNIT is 127 EMU, so a pixel size lands within one unit of the original.
    let (w, h) = images(&blocks)[0].display_emu();
    assert!(
        w.abs_diff(40 * 9525) < 127 && h.abs_diff(30 * 9525) < 127,
        "{w}x{h}"
    );
    assert_eq!(blocks[0], doc.blocks[0]);
}

#[test]
fn odt_rejects_external_escaping_damaged_and_missing_pictures_with_warnings() {
    let png = make_png(8, 8, 2, 3);
    let doc = sample(&png, &make_jpeg(8, 8));
    let bytes = encode(&doc, DocumentFormat::Odt);
    for (label, href) in [
        ("escape", "../outside.png"),
        ("absolute", "/etc/passwd"),
        ("external", "http://example.com/a.png"),
        ("file url", "file:///tmp/a.png"),
        ("missing", "Pictures/none.png"),
    ] {
        let edited = repack(
            &bytes,
            replace_in(
                "content.xml",
                "xlink:href=\"Pictures/image1.png\"",
                format!("xlink:href=\"{href}\""),
            ),
        );
        let (blocks, warnings) = decode(&edited, "a.odt");
        assert_eq!(
            images(&blocks).len(),
            1,
            "{label}: only the intact JPEG remains"
        );
        assert!(!warnings.is_empty(), "{label}");
    }
    let damaged = repack(&bytes, |p, mut data| {
        if p == "Pictures/image1.png" {
            data[20] ^= 0xFF;
        }
        data
    });
    let (blocks, warnings) = decode(&damaged, "a.odt");
    assert_eq!(images(&blocks).len(), 1);
    assert!(
        warnings.iter().any(|w| w.contains("image1.png")),
        "{warnings:?}"
    );
}

#[test]
fn hwpx_rejects_external_escaping_damaged_and_missing_pictures_with_warnings() {
    let png = make_png(8, 8, 2, 3);
    let doc = sample(&png, &make_jpeg(8, 8));
    let bytes = encode(&doc, DocumentFormat::Hwpx);
    for (label, href) in [
        ("escape", "../outside.png"),
        ("absolute", "/etc/passwd"),
        ("external", "http://example.com/a.png"),
        ("missing", "BinData/none.png"),
    ] {
        let edited = repack(
            &bytes,
            replace_in(
                "Contents/content.hpf",
                "href=\"BinData/image1.png\"",
                format!("href=\"{href}\""),
            ),
        );
        let (blocks, warnings) = decode(&edited, "a.hwpx");
        assert_eq!(images(&blocks).len(), 1, "{label}");
        assert!(!warnings.is_empty(), "{label}");
    }
    let damaged = repack(&bytes, |p, mut data| {
        if p == "BinData/image1.png" {
            data[20] ^= 0xFF;
        }
        data
    });
    let (blocks, warnings) = decode(&damaged, "a.hwpx");
    assert_eq!(images(&blocks).len(), 1);
    assert!(
        warnings.iter().any(|w| w.contains("image1.png")),
        "{warnings:?}"
    );
}

#[test]
fn readers_fail_closed_when_a_package_holds_too_many_pictures() {
    let img = RichImage::new(make_png(2, 2, 2, 3), "").unwrap();
    let doc = RichDocument {
        page_frame: Default::default(),
        title: "많은 그림".to_string(),
        blocks: vec![RichBlock::Image(img.clone()); MAX_IMAGES_PER_DOCUMENT],
        official_agency: None,
    };
    let odt = encode(&doc, DocumentFormat::Odt);
    let extra_odt = repack(&odt, |p, data| {
        if p != "content.xml" {
            return data;
        }
        let xml = String::from_utf8(data).unwrap();
        let start = xml.find("<text:p><draw:frame").unwrap();
        let end = xml[start..].find("</text:p>").unwrap() + start + "</text:p>".len();
        let one = xml[start..end].to_string();
        xml.replace(&one, &one.repeat(2)).into_bytes()
    });
    assert!(UniversalFormatConverter::decode_rich(&extra_odt, Some("a.odt")).is_err());

    let hwpx = encode(&doc, DocumentFormat::Hwpx);
    let extra_hwpx = repack(&hwpx, |p, data| {
        if p != "Contents/section0.xml" {
            return data;
        }
        let xml = String::from_utf8(data).unwrap();
        let start = xml.rfind("<hp:p ").unwrap();
        let end = xml[start..].find("</hp:p>").unwrap() + start + "</hp:p>".len();
        let one = xml[start..end].to_string();
        let mut out = xml;
        out.insert_str(end, &one);
        out.into_bytes()
    });
    assert!(UniversalFormatConverter::decode_rich(&extra_hwpx, Some("a.hwpx")).is_err());
    // Exactly at the limit still opens.
    assert_eq!(
        images(&decode(&odt, "a.odt").0).len(),
        MAX_IMAGES_PER_DOCUMENT
    );
}

//! PD-402: the crop of a slide picture (`a:srcRect`) survives a PPTX round trip, is drawn clipped in the PDF, and an
//! invalid crop (past 99%, or leaving under 1% of the picture, or negative padding) never reaches the model.

use easyoffice_formats::{
    deflate_zlib, encode_slide_pdf, ElementFx, IeeeCrc32, OoxmlCodec, RichImage, RichImageCrop,
    RichSlide, RichSlideDeck, SlideBox, SlideElement, SlideExt, SlideMasterLayout, ZipArchiveCodec,
    ZipSecurityPolicy,
};

fn png() -> Vec<u8> {
    let comp = deflate_zlib(&[0u8, 37, 99, 235, 255]);
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let chunk = |kind: &[u8; 4], data: &[u8], out: &mut Vec<u8>| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        out.extend_from_slice(&IeeeCrc32::update(IeeeCrc32::checksum(kind), data).to_be_bytes());
    };
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    chunk(b"IHDR", &ihdr, &mut out);
    chunk(b"IDAT", &comp, &mut out);
    chunk(b"IEND", &[], &mut out);
    out
}

fn deck(crop: Option<RichImageCrop>) -> RichSlideDeck {
    let mut fx = ElementFx::new("pic");
    fx.crop = crop;
    RichSlideDeck {
        slides: vec![RichSlide {
            layout: SlideMasterLayout::Blank,
            elements: vec![SlideElement::Image {
                id: "pic".into(),
                rect: SlideBox {
                    x: 100.0,
                    y: 80.0,
                    w: 200.0,
                    h: 100.0,
                },
                image: RichImage::new(png(), "").unwrap(),
                alt: "그림".into(),
            }],
            ext: SlideExt {
                fx: vec![fx],
                ..SlideExt::default()
            },
            ..RichSlide::default()
        }],
        ..RichSlideDeck::default()
    }
}

fn slide_xml(pptx: &[u8]) -> String {
    let entries = ZipArchiveCodec::unpack(pptx, &ZipSecurityPolicy::default()).unwrap();
    let e = entries
        .iter()
        .find(|e| e.path == "ppt/slides/slide1.xml")
        .unwrap();
    String::from_utf8(e.data.clone()).unwrap()
}

fn crop_of(deck: &RichSlideDeck) -> Option<RichImageCrop> {
    deck.slides[0].ext.fx_of("pic").and_then(|f| f.crop)
}

fn with_slide_xml(pptx: &[u8], f: impl Fn(String) -> String) -> Vec<u8> {
    let entries = ZipArchiveCodec::unpack(pptx, &ZipSecurityPolicy::default()).unwrap();
    let edited: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            if e.path == "ppt/slides/slide1.xml" {
                (e.path, f(String::from_utf8(e.data).unwrap()).into_bytes())
            } else {
                (e.path, e.data)
            }
        })
        .collect();
    ZipArchiveCodec::pack(&edited).unwrap()
}

#[test]
fn src_rect_round_trips_in_the_picture_blip_fill() {
    let crop = RichImageCrop {
        left: 10_000,
        top: 20_000,
        right: 30_000,
        bottom: 5_000,
    };
    let bytes = OoxmlCodec::pack_pptx_rich(&deck(Some(crop)), false).unwrap();
    let xml = slide_xml(&bytes);
    assert!(xml.contains("<a:srcRect l=\"10000\" t=\"20000\" r=\"30000\" b=\"5000\"/><a:stretch>"));
    assert_eq!(
        crop_of(&OoxmlCodec::unpack_pptx_rich(&bytes).unwrap()),
        Some(crop)
    );
    let none = OoxmlCodec::pack_pptx_rich(&deck(None), false).unwrap();
    assert!(!slide_xml(&none).contains("srcRect"));
}

#[test]
fn an_invalid_srcrect_in_a_file_leaves_the_picture_whole() {
    let crop = RichImageCrop {
        left: 10_000,
        ..RichImageCrop::default()
    };
    let bytes = OoxmlCodec::pack_pptx_rich(&deck(Some(crop)), false).unwrap();
    for hostile in [
        "<a:srcRect l=\"-5000\"/>",
        "<a:srcRect l=\"99999\"/>",
        "<a:srcRect l=\"60000\" r=\"60000\"/>",
        "<a:srcRect l=\"abc\"/>",
        "<a:srcRect l=\"4294967296\"/>",
        "<a:srcRect/>",
    ] {
        let edited = with_slide_xml(&bytes, |xml| {
            xml.replace("<a:srcRect l=\"10000\" t=\"0\" r=\"0\" b=\"0\"/>", hostile)
        });
        let back = OoxmlCodec::unpack_pptx_rich(&edited).unwrap();
        assert_eq!(crop_of(&back), None, "{hostile}");
    }
}

#[test]
fn the_pdf_clips_a_cropped_picture_to_its_frame() {
    let crop = RichImageCrop {
        left: 25_000,
        right: 25_000,
        ..RichImageCrop::default()
    };
    let pdf = encode_slide_pdf(&deck(Some(crop)), "자르기").unwrap();
    assert!(!pdf.is_empty());
    let text = String::from_utf8_lossy(&pdf);
    // The frame is clipped (`re W n`) and the picture is drawn twice as wide as the frame (400 pt over 200 pt).
    assert!(
        text.contains("re W n 400.00 0 0 100.00 "),
        "clip and full-size matrix"
    );
}

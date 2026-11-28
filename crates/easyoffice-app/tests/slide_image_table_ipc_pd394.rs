//! PD-394: the JSON of a slide deck carries each picture once, in an `images` table that slides and
//! elements point at with `imageRef`. A 30 MiB background shared by 200 slides is one base64 string in the
//! open result and in the save request, not 200 (which would be about 8 GB of JSON text).

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{
    deflate_zlib, IeeeCrc32, OoxmlCodec, RichImage, RichSlide, RichSlideDeck, SlideAspect,
    SlideBackground, SlideBox, SlideElement, SlideExt, SlideOrientation, StrictJsonValue,
};

const BIG: usize = 30 * 1024 * 1024;
const SAVE_BG: usize = 8 * 1024 * 1024;
const SLIDES: usize = 200;

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = IeeeCrc32::update(IeeeCrc32::checksum(kind), data);
    out.extend_from_slice(&crc.to_be_bytes());
    out
}

/// A valid 2x2 PNG of about `extra` more bytes (a padding chunk before its data).
fn padded_png(extra: usize) -> Vec<u8> {
    padded_png_filled(extra, b'a')
}

fn padded_png_filled(extra: usize, fill: u8) -> Vec<u8> {
    let raw: Vec<u8> = (0..2)
        .flat_map(|_| [0u8, 37, 99, 235, 255, 37, 99, 235, 255])
        .collect();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&2u32.to_be_bytes());
    ihdr.extend_from_slice(&2u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
    bytes.extend_from_slice(&png_chunk(b"tEXt", &vec![fill; extra]));
    bytes.extend_from_slice(&png_chunk(b"IDAT", &deflate_zlib(&raw)));
    bytes.extend_from_slice(&png_chunk(b"IEND", &[]));
    bytes
}

/// 200 slides that all use `img` as the background and `logo` as a picture element.
fn shared_picture_deck(img: &RichImage, logo: &RichImage) -> RichSlideDeck {
    let slides = (0..SLIDES)
        .map(|i| RichSlide {
            title: format!("쪽 {i}"),
            elements: vec![SlideElement::Image {
                id: format!("im{i}"),
                rect: SlideBox {
                    x: 10.0,
                    y: 10.0,
                    w: 100.0,
                    h: 60.0,
                },
                image: logo.clone(),
                alt: String::new(),
            }],
            ext: SlideExt {
                background: Some(SlideBackground::Picture(img.clone())),
                ..SlideExt::default()
            },
            ..RichSlide::default()
        })
        .collect();
    RichSlideDeck {
        title: "공유 그림".into(),
        is_slideshow: false,
        aspect: SlideAspect::Widescreen16x9,
        orientation: SlideOrientation::Landscape,
        slides,
        elements_rejected: 0,
        settings: Default::default(),
    }
}

fn field<'a>(obj: &'a [(String, StrictJsonValue)], key: &str) -> &'a StrictJsonValue {
    StrictJsonValue::get_field(obj, key).unwrap_or_else(|| panic!("missing JSON field '{key}'"))
}

fn count_of(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

#[test]
fn open_result_carries_a_shared_picture_once_and_slides_refer_to_it() {
    // The decode request is capped at 16 MiB, so the picture is 8 MiB here; the 30 MiB case is the unit
    // test of `slide_image_table`.
    let img = RichImage::new(padded_png(SAVE_BG), "").unwrap();
    let picture_b64_len = EasyOfficeIpcRouter::encode_base64(img.bytes()).len();
    let logo = RichImage::new(padded_png(16), "").unwrap();
    let pptx = OoxmlCodec::pack_pptx_rich(&shared_picture_deck(&img, &logo), false).unwrap();
    let b64 = EasyOfficeIpcRouter::encode_base64(&pptx);
    drop(pptx);
    let req = format!(r#"{{"action":"decode","fileName":"deck.pptx","sourceBase64":"{b64}"}}"#);
    drop(b64);
    let resp = EasyOfficeIpcRouter::dispatch("convert_document_format", &req).unwrap();

    assert!(
        resp.len() < picture_b64_len + picture_b64_len / 8,
        "open result is {} bytes for one {picture_b64_len}-byte picture",
        resp.len()
    );
    assert_eq!(
        count_of(&resp, "\"dataBase64\""),
        2,
        "the background and the logo are table entries, no inline copies"
    );
    assert_eq!(
        count_of(&resp, "\"imageRef\":\"img-1\""),
        SLIDES,
        "every background refers to the table"
    );
    assert_eq!(
        count_of(&resp, "\"imageRef\":\"img-2\""),
        SLIDES,
        "every logo element refers to the table"
    );

    let parsed = StrictJsonValue::parse(&resp).unwrap();
    let obj = parsed.as_object().unwrap();
    let images = field(obj, "images").as_array().unwrap();
    assert_eq!(images.len(), 2);
    assert_eq!(
        field(images[0].as_object().unwrap(), "id").as_str(),
        Some("img-1")
    );
}

#[test]
fn save_request_with_the_picture_table_builds_the_deck_and_reopens_to_one_picture() {
    // The save request is capped at 16 MiB, so the shared background is 8 MiB here: inline it would be
    // 200 copies, about 2 GB of JSON.
    let picture_b64 = EasyOfficeIpcRouter::encode_base64(&padded_png(SAVE_BG));
    let logo_b64 = EasyOfficeIpcRouter::encode_base64(&padded_png(16));
    let slides: Vec<String> = (0..SLIDES)
        .map(|i| {
            format!(
                r#"{{"title":"쪽 {i}","bullets":[],"notes":"","layout":"blank","transition":"none","background":{{"kind":"picture","imageRef":"img-1"}},"elements":[{{"kind":"image","id":"im{i}","x":10,"y":10,"w":100,"h":60,"imageRef":"img-2","alt":""}}]}}"#
            )
        })
        .collect();
    let req = format!(
        r#"{{"sourceFormat":"pptx","targetFormat":"pptx","title":"공유","aspect":"16:9","images":[{{"id":"img-1","mime":"image/png","dataBase64":"{picture_b64}"}},{{"id":"img-2","mime":"image/png","dataBase64":"{logo_b64}"}}],"slides":[{}]}}"#,
        slides.join(",")
    );
    assert!(
        req.len() < picture_b64.len() + picture_b64.len() / 8,
        "the request is one picture plus the slide skeletons"
    );
    let saved = EasyOfficeIpcRouter::dispatch("convert_document_format", &req).unwrap();
    let saved_val = StrictJsonValue::parse(&saved).unwrap();
    let pptx_b64 = field(saved_val.as_object().unwrap(), "convertedBase64")
        .as_str()
        .unwrap()
        .to_string();
    let reopened =
        OoxmlCodec::unpack_pptx_rich(&EasyOfficeIpcRouter::decode_base64(&pptx_b64).unwrap())
            .unwrap();
    assert_eq!(reopened.slides.len(), SLIDES);
    let ptr = |s: &RichSlide| match &s.ext.background {
        Some(SlideBackground::Picture(i)) => i.bytes().as_ptr(),
        _ => std::ptr::null(),
    };
    let first = ptr(&reopened.slides[0]);
    assert!(!first.is_null());
    assert!(reopened.slides.iter().all(|s| ptr(s) == first));
}

#[test]
fn picture_references_outside_the_table_and_repeated_table_ids_are_refused() {
    let png = EasyOfficeIpcRouter::encode_base64(&padded_png(16));
    let table = format!(r#""images":[{{"id":"img-1","mime":"image/png","dataBase64":"{png}"}}]"#);
    let one_slide = |bg: &str| {
        format!(
            r#"{{"sourceFormat":"pptx","targetFormat":"pptx","aspect":"16:9",{table},"slides":[{{"title":"","bullets":[],"notes":"","layout":"blank","transition":"none","background":{bg}}}]}}"#
        )
    };
    let ok = one_slide(r#"{"kind":"picture","imageRef":"img-1"}"#);
    assert!(EasyOfficeIpcRouter::dispatch("convert_document_format", &ok).is_ok());

    let dangling = one_slide(r#"{"kind":"picture","imageRef":"img-9"}"#);
    assert!(EasyOfficeIpcRouter::dispatch("convert_document_format", &dangling).is_err());

    let twice = ok.replace(
        &table,
        &format!(
            r#""images":[{{"id":"img-1","dataBase64":"{png}"}},{{"id":"img-1","dataBase64":"{png}"}}]"#
        ),
    );
    assert!(EasyOfficeIpcRouter::dispatch("convert_document_format", &twice).is_err());

    let inline = one_slide(&format!(r#"{{"kind":"picture","data":"{png}"}}"#))
        .replace(&format!("{table},"), "");
    assert!(
        EasyOfficeIpcRouter::dispatch("convert_document_format", &inline).is_ok(),
        "an inline picture (clipboard, Copilot) is still read"
    );
}

#[test]
fn distinct_table_pictures_over_the_deck_limit_are_refused() {
    let entries: Vec<String> = (0..5)
        .map(|i| {
            let bytes = padded_png_filled(BIG, b'a' + i as u8);
            format!(
                r#"{{"id":"img-{i}","dataBase64":"{}"}}"#,
                EasyOfficeIpcRouter::encode_base64(&bytes)
            )
        })
        .collect();
    let req = format!(
        r#"{{"sourceFormat":"pptx","targetFormat":"pptx","aspect":"16:9","images":[{}],"slides":[{{"title":"","bullets":[],"notes":"","layout":"blank","transition":"none"}}]}}"#,
        entries.join(",")
    );
    assert!(EasyOfficeIpcRouter::dispatch("convert_document_format", &req).is_err());
}

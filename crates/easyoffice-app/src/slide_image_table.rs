//! Pictures of a slide deck in JSON (`PD-394`): each distinct picture travels once, in a deck-level
//! `images` table of `{ id, mime, dataBase64 }`, and slides and elements point at it with `imageRef`. A
//! 30 MiB background shared by 200 slides is one base64 string in the open result and in the save request,
//! not 200. Inline `dataBase64`/`data` on an element or background is still read (clipboard and Copilot
//! pictures arrive that way).

use std::collections::HashMap;
use std::fmt::Write;

use easyoffice_formats::{
    base64_codec, EasyOfficeError, RichImage, RichSlideDeck, SlideBackground, SlideElement,
    MAX_ELEMENTS_PER_DECK, MAX_SLIDES_PER_DECK, MAX_SLIDE_TOTAL_IMAGE_BYTES,
};

use crate::convert_structured::{as_array, as_object, field, invalid, opt_str, Budget, JsonObject};

/// Most distinct pictures a deck can name: one per element and one background per slide.
const MAX_TABLE_IMAGES: usize = MAX_ELEMENTS_PER_DECK + MAX_SLIDES_PER_DECK;

/// The distinct pictures of a deck in the order they are first used; a picture is told apart by the buffer it
/// shares, so a deck read from a file (one buffer per picture) yields one entry per picture.
pub(crate) struct ImageTable<'a> {
    ids: HashMap<usize, usize>,
    list: Vec<&'a RichImage>,
}

impl<'a> ImageTable<'a> {
    pub(crate) fn from_deck(deck: &'a RichSlideDeck) -> Self {
        let mut table = Self {
            ids: HashMap::new(),
            list: Vec::new(),
        };
        for slide in &deck.slides {
            if let Some(SlideBackground::Picture(img)) = &slide.ext.background {
                table.add(img);
            }
            for el in &slide.elements {
                if let SlideElement::Image { image, .. } = el {
                    table.add(image);
                }
            }
        }
        table
    }

    fn add(&mut self, img: &'a RichImage) {
        let key = img.bytes().as_ptr() as usize;
        if !self.ids.contains_key(&key) {
            self.ids.insert(key, self.list.len() + 1);
            self.list.push(img);
        }
    }

    /// The id a picture is referred to by.
    pub(crate) fn id_of(&self, img: &RichImage) -> String {
        format!(
            "img-{}",
            self.ids
                .get(&(img.bytes().as_ptr() as usize))
                .copied()
                .unwrap_or(0)
        )
    }

    /// `"images":[...],` for the deck object (nothing when the deck has no picture).
    pub(crate) fn write(&self, out: &mut String) {
        if self.list.is_empty() {
            return;
        }
        out.push_str("\"images\":[");
        for (i, img) in self.list.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "{{\"id\":\"img-{}\",\"mime\":\"{}\",\"dataBase64\":\"{}\"}}",
                i + 1,
                img.format().mime(),
                base64_codec::encode(img.bytes())
            );
        }
        out.push_str("],");
    }
}

/// The decoded `images` table by id.
pub(crate) type ImageRefs = HashMap<String, RichImage>;

/// The pictures of the `images` table by id. Each is decoded once and counted once in `total_img_bytes`.
pub(crate) fn parse_image_table(
    deck: &JsonObject,
    budget: &mut Budget,
    total_img_bytes: &mut usize,
) -> Result<ImageRefs, EasyOfficeError> {
    let mut table = HashMap::new();
    let Some(v) = field(deck, "images") else {
        return Ok(table);
    };
    let entries = as_array(v, "images")?;
    if entries.len() > MAX_TABLE_IMAGES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "슬라이드 그림 표 항목 수가 상한({MAX_TABLE_IMAGES})을 초과했습니다."
        )));
    }
    budget.take(entries.len())?;
    for e in entries {
        let o = as_object(e, "images[]")?;
        let id = opt_str(o, "id")?
            .filter(|s| !s.is_empty())
            .ok_or_else(|| invalid("그림 표 항목에 'id'가 필요합니다."))?;
        let b64 = opt_str(o, "dataBase64")?
            .filter(|s| !s.is_empty())
            .ok_or_else(|| invalid("그림 표 항목에 'dataBase64'가 필요합니다."))?;
        let bytes = base64_codec::decode(b64)
            .ok_or_else(|| invalid("슬라이드 그림 Base64 디코딩에 실패했습니다."))?;
        *total_img_bytes = total_img_bytes.saturating_add(bytes.len());
        if *total_img_bytes > MAX_SLIDE_TOTAL_IMAGE_BYTES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "슬라이드 전체 이미지 크기가 상한(128 MiB)을 초과했습니다.".to_string(),
            ));
        }
        if table
            .insert(id.to_string(), RichImage::new(bytes, "")?)
            .is_some()
        {
            return Err(invalid(format!("그림 표 id가 겹칩니다: {id}")));
        }
    }
    Ok(table)
}

/// The picture the `imageRef` of an element or background names.
pub(crate) fn image_by_ref(
    obj: &JsonObject,
    table: &ImageRefs,
) -> Result<Option<RichImage>, EasyOfficeError> {
    let Some(id) = opt_str(obj, "imageRef")? else {
        return Ok(None);
    };
    table
        .get(id)
        .cloned()
        .map(Some)
        .ok_or_else(|| invalid(format!("그림 표에 없는 imageRef입니다: {id}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slide_deck_json::write_slide_deck_decode_fields;
    use easyoffice_formats::json_out::push_json_string;
    use easyoffice_formats::{
        deflate_zlib, IeeeCrc32, RichSlide, SlideAspect, SlideBox, SlideExt, SlideOrientation,
    };

    const BIG: usize = 30 * 1024 * 1024;
    const SLIDES: usize = 200;

    fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = (data.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let crc = IeeeCrc32::update(IeeeCrc32::checksum(kind), data);
        out.extend_from_slice(&crc.to_be_bytes());
        out
    }

    /// A valid 2x2 PNG with a `pad`-byte text chunk before its data.
    fn padded_png(pad: usize) -> RichImage {
        let raw: Vec<u8> = (0..2)
            .flat_map(|_| [0u8, 37, 99, 235, 255, 37, 99, 235, 255])
            .collect();
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&2u32.to_be_bytes());
        ihdr.extend_from_slice(&2u32.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
        bytes.extend_from_slice(&png_chunk(b"tEXt", &vec![b'a'; pad]));
        bytes.extend_from_slice(&png_chunk(b"IDAT", &deflate_zlib(&raw)));
        bytes.extend_from_slice(&png_chunk(b"IEND", &[]));
        RichImage::new(bytes, "").unwrap()
    }

    fn deck(bg: &RichImage, logo: &RichImage) -> RichSlideDeck {
        let slide = |i: usize| RichSlide {
            elements: vec![SlideElement::Image {
                id: format!("im{i}"),
                rect: SlideBox {
                    x: 0.0,
                    y: 0.0,
                    w: 50.0,
                    h: 50.0,
                },
                image: logo.clone(),
                alt: String::new(),
            }],
            ext: SlideExt {
                background: Some(SlideBackground::Picture(bg.clone())),
                ..SlideExt::default()
            },
            ..RichSlide::default()
        };
        RichSlideDeck {
            title: String::new(),
            is_slideshow: false,
            aspect: SlideAspect::Widescreen16x9,
            orientation: SlideOrientation::Landscape,
            slides: (0..SLIDES).map(slide).collect(),
            elements_rejected: 0,
            settings: Default::default(),
        }
    }

    /// The reported defect: a 30 MiB background on 200 slides wrote about 8 GB of base64.
    #[test]
    fn open_json_of_a_30_mib_background_on_200_slides_is_one_picture_in_size() {
        let (bg, logo) = (padded_png(BIG), padded_png(16));
        let picture_b64 = base64_codec::encode(bg.bytes()).len();
        let deck = deck(&bg, &logo);
        let mut out = String::new();
        write_slide_deck_decode_fields(&mut out, &deck, push_json_string);
        assert!(
            out.len() < picture_b64 + picture_b64 / 16,
            "{} bytes of JSON for one {picture_b64}-byte picture",
            out.len()
        );
        assert_eq!(out.matches("\"dataBase64\"").count(), 2);
        assert_eq!(out.matches("\"imageRef\":\"img-1\"").count(), SLIDES);
        assert_eq!(out.matches("\"imageRef\":\"img-2\"").count(), SLIDES);
    }

    #[test]
    fn pictures_with_equal_bytes_in_separate_buffers_get_separate_ids() {
        let (a, b) = (padded_png(16), padded_png(16));
        let mut deck = deck(&a, &b);
        deck.slides.truncate(1);
        let table = ImageTable::from_deck(&deck);
        assert_eq!(table.list.len(), 2);
        assert_ne!(table.id_of(&a), table.id_of(&b));
    }
}

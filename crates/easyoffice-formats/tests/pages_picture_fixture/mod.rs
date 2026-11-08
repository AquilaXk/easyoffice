//! Shared builders for synthetic Pages packages that carry pictures (`Data/` files, package
//! metadata, picture drawables, anchors) plus small readers over the decoded document.
#![allow(dead_code)]

use crate::image_fixture::{png_chunk, PNG_SIGNATURE};
use crate::pages_fixture::*;
use easyoffice_formats::{
    DecodedContent, DocumentFormat, EasyOfficeError, RichBlock, RichDocument, RichImage,
    UniversalFormatConverter,
};

/// Bytes of a deterministic, incompressible pseudo-random stream.
pub fn noise(len: usize) -> Vec<u8> {
    let mut x = 0x9E37_79B9_7F4A_7C15u64;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 24) as u8
        })
        .collect()
}

/// Structurally valid 1x1 PNG whose single IDAT chunk holds `idat_len` bytes (only the
/// structure is checked on import, so the payload need not inflate).
pub fn bulky_png(idat_len: usize) -> Vec<u8> {
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    let mut png = PNG_SIGNATURE.to_vec();
    png.extend(png_chunk(b"IHDR", &ihdr));
    png.extend(png_chunk(b"IDAT", &noise(idat_len)));
    png.extend(png_chunk(b"IEND", &[]));
    png
}

/// Package-metadata object (type 11006) mapping data identifiers to file names.
pub fn metadata(b: &mut DocBuilder, files: &[(u64, &str)]) -> u64 {
    let infos: Vec<Vec<u8>> = files
        .iter()
        .map(|(id, name)| f_bytes(4, &cat(&[f_varint(1, *id), f_str(4, name)])))
        .collect();
    b.add(11006, &cat(&infos))
}

pub struct Picture<'a> {
    pub data_id: Option<u64>,
    pub alt: Option<&'a str>,
    pub size_pt: Option<(f32, f32)>,
    pub masked: bool,
}

impl Default for Picture<'_> {
    fn default() -> Self {
        Self {
            data_id: Some(1),
            alt: None,
            size_pt: None,
            masked: false,
        }
    }
}

/// Picture drawable (type 3005).
pub fn picture(b: &mut DocBuilder, p: &Picture) -> u64 {
    let mut drawable = Vec::new();
    if let Some((w, h)) = p.size_pt {
        let size = cat(&[f_f32(1, w), f_f32(2, h)]);
        drawable.push(f_bytes(1, &f_bytes(2, &size)));
    }
    if let Some(alt) = p.alt {
        drawable.push(f_str(8, alt));
    }
    let mut fields = vec![f_bytes(1, &cat(&drawable))];
    if p.masked {
        fields.push(f_ref(5, 4_000_000));
    }
    if let Some(id) = p.data_id {
        fields.push(f_ref(11, id));
    }
    b.add(3005, &cat(&fields))
}

/// Body with `intro`, one anchor paragraph per attachment, then `outro`.
pub fn body_with(b: &mut DocBuilder, intro: &str, atts: &[u64], outro: &str) -> u64 {
    let mut paras = vec![intro.to_string()];
    paras.extend(atts.iter().map(|_| "\u{FFFC}".to_string()));
    paras.push(outro.to_string());
    let text = paras.join("\n");
    let mut pos = 0u64;
    let mut table = Vec::new();
    for (i, para) in paras.iter().enumerate() {
        if i >= 1 && i <= atts.len() {
            table.push((pos, Some(atts[i - 1])));
        }
        pos += utf16_len(para) + 1;
    }
    b.add(
        2001,
        &DocBuilder::storage_payload(&text, &[], &[], &[], &table),
    )
}

pub fn open(zip: &[u8]) -> Result<(RichDocument, Vec<String>), EasyOfficeError> {
    let doc = UniversalFormatConverter::decode_rich(zip, Some("t.pages"))?;
    assert_eq!(doc.format, DocumentFormat::Pages);
    let DecodedContent::Document(d) = doc.content else {
        panic!("document expected");
    };
    Ok((d, doc.warnings))
}

pub fn images(doc: &RichDocument) -> Vec<&RichImage> {
    doc.blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Image(i) => Some(i),
            _ => None,
        })
        .collect()
}

pub fn paragraph_texts(doc: &RichDocument) -> Vec<String> {
    doc.blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some(p.plain_text()),
            _ => None,
        })
        .collect()
}

pub fn warned(warnings: &[String], needle: &str) -> bool {
    warnings.iter().any(|w| w.contains(needle))
}

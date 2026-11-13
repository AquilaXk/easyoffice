//! Picture fragments for the container writers: DOCX `w:drawing` inline pictures, ODT
//! `draw:frame` pictures and HWPX `hp:pic`. Each fragment references the media part by name; the
//! caller stores the bytes unchanged (so a part is byte-identical to the model's picture).

use std::collections::HashMap;

use crate::bytes_hash::{hash_bytes, random_seed};
use crate::rich_image::{RichImage, EMU_PER_POINT};
use crate::{RichImageFormat, XmlSanitizer};

const EMU_PER_INCH: u64 = 914_400;
/// HWPX length unit: 1/7200 inch.
const EMU_PER_HWPUNIT: u64 = EMU_PER_INCH / 7_200;
const POINT_DECIMALS_SCALE: u64 = 10_000;

pub(crate) const DOCX_IMAGE_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
pub(crate) const NS_HWPX_CORE: &str = "http://www.owpml.org/owpml/2011/core";

/// Media parts collected while writing a document, numbered from 1 in block order. A picture
/// used again (same bytes) shares its part, so the package stores it once.
pub(crate) struct MediaParts {
    parts: Vec<(String, Vec<u8>, RichImageFormat)>,
    /// Part indexes by `(length, keyed hash)` of the bytes.
    by_hash: HashMap<(usize, u64), Vec<usize>>,
    seed: u64,
    references: usize,
}

/// What one picture reference points to.
pub(crate) struct MediaRef {
    /// 1-based count of references so far: unique per picture reference, for drawing ids and names.
    pub(crate) ordinal: usize,
    /// 1-based number of the part holding the bytes (shared by every reference to them).
    pub(crate) part: usize,
    pub(crate) path: String,
    /// Whether this reference created the part.
    pub(crate) first_use: bool,
}

impl MediaParts {
    pub(crate) fn new() -> Self {
        Self {
            parts: Vec::new(),
            by_hash: HashMap::new(),
            seed: random_seed(),
            references: 0,
        }
    }

    /// Registers `img` under `dir`: its bytes become a new part unless a part holds the same bytes
    /// (a hash match is confirmed by comparing the bytes).
    pub(crate) fn add(&mut self, dir: &str, img: &RichImage) -> MediaRef {
        self.references += 1;
        let ordinal = self.references;
        let bytes = img.bytes();
        let key = (bytes.len(), hash_bytes(bytes, self.seed));
        let known = self
            .by_hash
            .get(&key)
            .and_then(|candidates| candidates.iter().find(|&&at| self.parts[at].1 == bytes));
        if let Some(&at) = known {
            return MediaRef {
                ordinal,
                part: at + 1,
                path: self.parts[at].0.clone(),
                first_use: false,
            };
        }
        let part = self.parts.len() + 1;
        let path = format!("{dir}/image{part}.{}", img.format().extension());
        self.by_hash.entry(key).or_default().push(part - 1);
        self.parts
            .push((path.clone(), bytes.to_vec(), img.format()));
        MediaRef {
            ordinal,
            part,
            path,
            first_use: true,
        }
    }

    pub(crate) fn has_format(&self, format: RichImageFormat) -> bool {
        self.parts.iter().any(|(_, _, f)| *f == format)
    }

    /// `(path, media type)` of every part.
    pub(crate) fn listing(&self) -> Vec<(String, &'static str)> {
        self.parts
            .iter()
            .map(|(p, _, f)| (p.clone(), f.mime()))
            .collect()
    }

    /// ZIP entries `(path, bytes)` for the parts.
    pub(crate) fn into_entries(self) -> Vec<(String, Vec<u8>)> {
        self.parts.into_iter().map(|(p, b, _)| (p, b)).collect()
    }
}

/// DOCX `[Content_Types].xml` defaults for the picture extensions in `media`.
pub(crate) fn docx_content_type_defaults(media: &MediaParts) -> String {
    let mut out = String::new();
    for format in [RichImageFormat::Png, RichImageFormat::Jpeg] {
        if media.has_format(format) {
            out.push_str(&format!(
                "<Default Extension=\"{}\" ContentType=\"{}\"/>",
                format.extension(),
                format.mime()
            ));
        }
    }
    out
}

/// One `Relationship` element for a DOCX picture.
pub(crate) fn docx_image_relationship(rel_id: &str, target: &str) -> String {
    format!(
        "<Relationship Id=\"{rel_id}\" Type=\"{DOCX_IMAGE_REL_TYPE}\" Target=\"{}\"/>",
        XmlSanitizer::escape(target)
    )
}

/// A DOCX paragraph holding one inline picture (`number`, the reference ordinal, is the drawing id).
pub(crate) fn docx_image_paragraph(img: &RichImage, number: usize, rel_id: &str) -> String {
    let (cx, cy) = img.display_emu();
    let alt = XmlSanitizer::escape(img.alt());
    format!(
        concat!(
            "<w:p><w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\">",
            "<wp:extent cx=\"{cx}\" cy=\"{cy}\"/>",
            "<wp:docPr id=\"{n}\" name=\"Picture {n}\" descr=\"{alt}\"/>",
            "<wp:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect=\"1\"/></wp:cNvGraphicFramePr>",
            "<a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">",
            "<pic:pic><pic:nvPicPr><pic:cNvPr id=\"{n}\" name=\"image{n}.{ext}\" descr=\"{alt}\"/><pic:cNvPicPr/></pic:nvPicPr>",
            "<pic:blipFill><a:blip r:embed=\"{rid}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>",
            "<pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>",
            "<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>",
            "</a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"
        ),
        cx = cx,
        cy = cy,
        n = number,
        alt = alt,
        ext = img.format().extension(),
        rid = rel_id
    )
}

/// Namespace declarations the DOCX body needs for pictures.
pub(crate) const DOCX_PICTURE_NAMESPACES: &str = concat!(
    " xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"",
    " xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\"",
    " xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"",
    " xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\""
);

/// EMU as a point length with four decimals (`30.0000pt`), rounded, integer math only. A
/// 96 dpi pixel is 0.75 pt, so pixel-based sizes are written exactly.
fn emu_to_points_text(emu: u64) -> String {
    let scaled = emu
        .saturating_mul(POINT_DECIMALS_SCALE)
        .saturating_add(EMU_PER_POINT / 2)
        / EMU_PER_POINT;
    format!(
        "{}.{:04}pt",
        scaled / POINT_DECIMALS_SCALE,
        scaled % POINT_DECIMALS_SCALE
    )
}

/// An ODT paragraph holding one as-character picture frame.
pub(crate) fn odt_image_paragraph(img: &RichImage, number: usize, href: &str) -> String {
    let (cx, cy) = img.display_emu();
    format!(
        concat!(
            "<text:p><draw:frame draw:name=\"image{n}\" text:anchor-type=\"as-char\" svg:width=\"{w}\" svg:height=\"{h}\">",
            "<draw:image xlink:href=\"{href}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/>",
            "<svg:desc>{alt}</svg:desc></draw:frame></text:p>"
        ),
        n = number,
        w = emu_to_points_text(cx),
        h = emu_to_points_text(cy),
        href = XmlSanitizer::escape(href),
        alt = XmlSanitizer::escape(img.alt())
    )
}

/// Namespace declarations the ODT content needs for pictures.
pub(crate) const ODT_PICTURE_NAMESPACES: &str = concat!(
    "xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" ",
    "xmlns:xlink=\"http://www.w3.org/1999/xlink\" "
);

/// An HWPX paragraph body (run) holding one inline picture; `item_id` is the manifest id.
pub(crate) fn hwpx_picture_run(img: &RichImage, number: usize, item_id: &str) -> String {
    let (cx, cy) = img.display_emu();
    let w = (cx / EMU_PER_HWPUNIT).max(1);
    let h = (cy / EMU_PER_HWPUNIT).max(1);
    format!(
        concat!(
            "<hp:run charPrIDRef=\"0\"><hp:pic id=\"{n}\" zOrder=\"0\" numberingType=\"PICTURE\" textWrap=\"TOP_AND_BOTTOM\" textFlow=\"BOTH_SIDES\" lock=\"0\" dropcapstyle=\"None\" href=\"\" groupLevel=\"0\" instid=\"{n}\" reverse=\"0\" xmlns:hc=\"{core}\">",
            "<hp:offset x=\"0\" y=\"0\"/><hp:orgSz width=\"{w}\" height=\"{h}\"/><hp:curSz width=\"{w}\" height=\"{h}\"/>",
            "<hp:flip horizontal=\"0\" vertical=\"0\"/><hp:rotationInfo angle=\"0\" centerX=\"{cx2}\" centerY=\"{cy2}\" rotateimage=\"1\"/>",
            "<hp:renderingInfo><hc:transMatrix e1=\"1\" e2=\"0\" e3=\"0\" e4=\"0\" e5=\"1\" e6=\"0\"/>",
            "<hc:scaMatrix e1=\"1\" e2=\"0\" e3=\"0\" e4=\"0\" e5=\"1\" e6=\"0\"/>",
            "<hc:rotMatrix e1=\"1\" e2=\"0\" e3=\"0\" e4=\"0\" e5=\"1\" e6=\"0\"/></hp:renderingInfo>",
            "<hc:img binaryItemIDRef=\"{item}\" bright=\"0\" contrast=\"0\" effect=\"REAL_PIC\" alpha=\"0\"/>",
            "<hp:imgRect><hc:pt0 x=\"0\" y=\"0\"/><hc:pt1 x=\"{w}\" y=\"0\"/><hc:pt2 x=\"{w}\" y=\"{h}\"/><hc:pt3 x=\"0\" y=\"{h}\"/></hp:imgRect>",
            "<hp:imgClip left=\"0\" right=\"{w}\" top=\"0\" bottom=\"{h}\"/><hp:inMargin left=\"0\" right=\"0\" top=\"0\" bottom=\"0\"/>",
            "<hp:imgDim dimwidth=\"{w}\" dimheight=\"{h}\"/><hp:effects/>",
            "<hp:sz width=\"{w}\" widthRelTo=\"ABSOLUTE\" height=\"{h}\" heightRelTo=\"ABSOLUTE\" protect=\"0\"/>",
            "<hp:pos treatAsChar=\"1\" affectLSpacing=\"0\" flowWithText=\"1\" allowOverlap=\"0\" holdAnchorAndSO=\"0\" vertRelTo=\"PARA\" horzRelTo=\"COLUMN\" vertAlign=\"TOP\" horzAlign=\"LEFT\" vertOffset=\"0\" horzOffset=\"0\"/>",
            "<hp:outMargin left=\"0\" right=\"0\" top=\"0\" bottom=\"0\"/>",
            "<hp:shapeComment>{alt}</hp:shapeComment></hp:pic></hp:run>"
        ),
        n = number,
        core = NS_HWPX_CORE,
        w = w,
        h = h,
        cx2 = w / 2,
        cy2 = h / 2,
        item = item_id,
        alt = XmlSanitizer::escape(img.alt())
    )
}

/// Points for a display size, used by page-based writers.
pub(crate) fn emu_to_points(emu: u64) -> u64 {
    (emu / EMU_PER_POINT).max(1)
}

const DOCX_STYLES_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml";
const DOCX_NUMBERING_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml";

/// `[Content_Types].xml` of a DOCX package; `frame_types` are the overrides of its header and footer parts.
pub(crate) fn docx_content_types(media: &MediaParts, numbering: bool, frame_types: &str) -> String {
    format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">",
            "<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>",
            "<Default Extension=\"xml\" ContentType=\"application/xml\"/>{}",
            "<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>",
            "<Override PartName=\"/word/styles.xml\" ContentType=\"{}\"/>",
            "{}{}",
            "<Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/>",
            "</Types>"
        ),
        docx_content_type_defaults(media),
        DOCX_STYLES_CONTENT_TYPE,
        if numbering {
            format!(
                "<Override PartName=\"/word/numbering.xml\" ContentType=\"{DOCX_NUMBERING_CONTENT_TYPE}\"/>"
            )
        } else {
            String::new()
        },
        frame_types
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IeeeCrc32;

    fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = (data.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let crc = IeeeCrc32::update(IeeeCrc32::checksum(kind), data);
        out.extend_from_slice(&crc.to_be_bytes());
        out
    }

    /// A structurally valid PNG whose bytes depend on `width` and `fill` but not its length.
    fn png(width: u32, fill: u8) -> RichImage {
        let mut ihdr = width.to_be_bytes().to_vec();
        ihdr.extend_from_slice(&1u32.to_be_bytes());
        ihdr.extend_from_slice(&[8, 0, 0, 0, 0]);
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend(chunk(b"IHDR", &ihdr));
        bytes.extend(chunk(b"IDAT", &[fill; 16]));
        bytes.extend(chunk(b"IEND", &[]));
        RichImage::new(bytes, "").unwrap()
    }

    #[test]
    fn equal_bytes_share_a_part_and_every_reference_gets_its_own_ordinal() {
        let mut media = MediaParts::new();
        let a = png(2, 1);
        let first = media.add("m", &a);
        let again = media.add("m", &png(2, 1));
        let other = media.add("m", &png(2, 9));
        assert!(first.first_use && !again.first_use && other.first_use);
        assert_eq!((first.part, again.part, other.part), (1, 1, 2));
        assert_eq!(first.path, again.path);
        assert_eq!((first.ordinal, again.ordinal, other.ordinal), (1, 2, 3));
        assert_eq!(media.listing().len(), 2);
        assert_eq!(media.into_entries().len(), 2);
    }

    #[test]
    fn a_hash_match_with_different_bytes_is_not_shared() {
        let mut media = MediaParts::new();
        let (a, b) = (png(2, 1), png(3, 1));
        assert_eq!(a.bytes().len(), b.bytes().len());
        media.add("m", &a);
        // Plant the first part under the key of the second picture: a forced hash collision.
        let key = (b.bytes().len(), hash_bytes(b.bytes(), media.seed));
        media.by_hash.entry(key).or_default().push(0);
        let second = media.add("m", &b);
        assert!(second.first_use);
        assert_eq!(second.part, 2);
        assert_eq!(media.listing().len(), 2);
        // The planted candidate stays and the real one is found next time.
        let third = media.add("m", &b);
        assert!(!third.first_use);
        assert_eq!(third.part, 2);
    }
}

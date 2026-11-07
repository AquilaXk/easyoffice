//! PDF image XObjects for [`crate::RichImage`]: JPEG streams pass through as `DCTDecode`; PNG
//! `IDAT` data of opaque images passes through as `FlateDecode` with PNG predictors; PNG images
//! with an alpha channel are split into a color stream and a `/SMask` gray stream. Palette,
//! interlaced and 16-bit PNG files (and CMYK JPEG) are not embedded; the caller prints the
//! alternative text instead.

use crate::rich_image_probe::{
    png_idat_stream, probe_jpeg, probe_png, PngHeader, PNG_COLOR_PALETTE,
};
use crate::{deflate_zlib, inflate_zlib, RichImage, RichImageFormat};

/// Largest pixel count whose alpha channel is decoded and split (bounds decode memory).
const PDF_MAX_ALPHA_PIXELS: u64 = 16_000_000;
const PNG_PREDICTOR_OPTIMUM: u8 = 15;
const GRAY_COMPONENTS: u8 = 1;
const RGB_COMPONENTS: u8 = 3;
const PNG_FILTER_NONE: u8 = 0;
const PNG_FILTER_SUB: u8 = 1;
const PNG_FILTER_UP: u8 = 2;
const PNG_FILTER_AVERAGE: u8 = 3;
const PNG_FILTER_PAETH: u8 = 4;

/// An image XObject: dictionary entries (without `/Length`), stream bytes and optional soft mask.
pub(crate) struct PdfImageObject {
    pub dict: String,
    pub data: Vec<u8>,
    pub soft_mask: Option<Box<PdfImageObject>>,
}

fn image_dict(width: u32, height: u32, color_space: &str, bits: u8, filter: &str) -> String {
    format!(
        " /Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /{color_space} /BitsPerComponent {bits} /Filter /{filter}"
    )
}

/// Builds the XObject for `img`, or `None` when the picture must be replaced by its alt text.
pub(crate) fn pdf_image_object(img: &RichImage) -> Option<PdfImageObject> {
    match img.format() {
        RichImageFormat::Jpeg => jpeg_object(img.bytes()),
        RichImageFormat::Png => png_object(img.bytes()),
    }
}

fn jpeg_object(bytes: &[u8]) -> Option<PdfImageObject> {
    let header = probe_jpeg(bytes).ok()?;
    let color_space = match header.components {
        GRAY_COMPONENTS => "DeviceGray",
        RGB_COMPONENTS => "DeviceRGB",
        _ => return None,
    };
    Some(PdfImageObject {
        dict: image_dict(header.width, header.height, color_space, 8, "DCTDecode"),
        data: bytes.to_vec(),
        soft_mask: None,
    })
}

fn png_object(bytes: &[u8]) -> Option<PdfImageObject> {
    let header = probe_png(bytes).ok()?;
    if header.interlaced || header.color_type == PNG_COLOR_PALETTE || header.bit_depth > 8 {
        return None;
    }
    if header.has_alpha_channel() {
        png_alpha_object(bytes, &header)
    } else {
        Some(png_opaque_object(bytes, &header))
    }
}

fn png_opaque_object(bytes: &[u8], header: &PngHeader) -> PdfImageObject {
    let colors = header.channels();
    let color_space = if colors == 1 {
        "DeviceGray"
    } else {
        "DeviceRGB"
    };
    let mut dict = image_dict(
        header.width,
        header.height,
        color_space,
        header.bit_depth,
        "FlateDecode",
    );
    dict.push_str(&format!(
        " /DecodeParms << /Predictor {PNG_PREDICTOR_OPTIMUM} /Colors {colors} /BitsPerComponent {} /Columns {} >>",
        header.bit_depth, header.width
    ));
    PdfImageObject {
        dict,
        data: png_idat_stream(bytes),
        soft_mask: None,
    }
}

fn png_alpha_object(bytes: &[u8], header: &PngHeader) -> Option<PdfImageObject> {
    if u64::from(header.width) * u64::from(header.height) > PDF_MAX_ALPHA_PIXELS {
        return None;
    }
    let bytes_per_pixel = header.channels();
    let row_bytes = header.row_bytes()?;
    let stride = row_bytes.checked_add(1)?;
    let expected = stride.checked_mul(header.height as usize)?;
    let mut raw = inflate_zlib(&png_idat_stream(bytes), expected).ok()?;
    if raw.len() != expected {
        return None;
    }
    unfilter_scanlines(&mut raw, row_bytes, bytes_per_pixel)?;
    let (color, alpha) = split_alpha(&raw, stride, bytes_per_pixel);
    let color_space = if bytes_per_pixel == 2 {
        "DeviceGray"
    } else {
        "DeviceRGB"
    };
    Some(PdfImageObject {
        dict: image_dict(header.width, header.height, color_space, 8, "FlateDecode"),
        data: deflate_zlib(&color),
        soft_mask: Some(Box::new(PdfImageObject {
            dict: image_dict(header.width, header.height, "DeviceGray", 8, "FlateDecode"),
            data: deflate_zlib(&alpha),
            soft_mask: None,
        })),
    })
}

/// Separates unfiltered `[color..., alpha]` pixels into a color plane and an alpha plane.
fn split_alpha(raw: &[u8], stride: usize, bytes_per_pixel: usize) -> (Vec<u8>, Vec<u8>) {
    let rows = raw.len() / stride;
    let pixels_per_row = (stride - 1) / bytes_per_pixel;
    let mut color = Vec::with_capacity(rows * pixels_per_row * (bytes_per_pixel - 1));
    let mut alpha = Vec::with_capacity(rows * pixels_per_row);
    for row in raw.chunks_exact(stride) {
        for pixel in row[1..].chunks_exact(bytes_per_pixel) {
            color.extend_from_slice(&pixel[..bytes_per_pixel - 1]);
            alpha.push(pixel[bytes_per_pixel - 1]);
        }
    }
    (color, alpha)
}

/// Reverses the PNG scanline filters in place (8-bit samples; `bpp` bytes per pixel).
fn unfilter_scanlines(raw: &mut [u8], row_bytes: usize, bpp: usize) -> Option<()> {
    let stride = row_bytes + 1;
    let mut previous_start: Option<usize> = None;
    for row_start in (0..raw.len()).step_by(stride) {
        let (done, rest) = raw.split_at_mut(row_start);
        let filter = rest[0];
        let current = rest.get_mut(1..stride)?;
        let previous = previous_start.and_then(|s| done.get(s + 1..s + stride));
        unfilter_row(filter, current, previous, bpp)?;
        previous_start = Some(row_start);
    }
    Some(())
}

fn unfilter_row(filter: u8, cur: &mut [u8], prev: Option<&[u8]>, bpp: usize) -> Option<()> {
    match filter {
        PNG_FILTER_NONE => {}
        PNG_FILTER_SUB => (bpp..cur.len()).for_each(|i| cur[i] = cur[i].wrapping_add(cur[i - bpp])),
        PNG_FILTER_UP => {
            if let Some(p) = prev {
                cur.iter_mut()
                    .zip(p)
                    .for_each(|(c, u)| *c = c.wrapping_add(*u));
            }
        }
        PNG_FILTER_AVERAGE => unfilter_average(cur, prev, bpp),
        PNG_FILTER_PAETH => unfilter_paeth(cur, prev, bpp),
        _ => return None,
    }
    Some(())
}

fn unfilter_average(cur: &mut [u8], prev: Option<&[u8]>, bpp: usize) {
    for i in 0..cur.len() {
        let left = if i >= bpp { u16::from(cur[i - bpp]) } else { 0 };
        let up = prev.map_or(0, |p| u16::from(p[i]));
        cur[i] = cur[i].wrapping_add(((left + up) / 2) as u8);
    }
}

fn paeth_predictor(a: u8, b: u8, c: u8) -> u8 {
    let (ia, ib, ic) = (i16::from(a), i16::from(b), i16::from(c));
    let p = ia + ib - ic;
    let (pa, pb, pc) = ((p - ia).abs(), (p - ib).abs(), (p - ic).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

fn unfilter_paeth(cur: &mut [u8], prev: Option<&[u8]>, bpp: usize) {
    for i in 0..cur.len() {
        let left = if i >= bpp { cur[i - bpp] } else { 0 };
        let up = prev.map_or(0, |p| p[i]);
        let up_left = if i >= bpp {
            prev.map_or(0, |p| p[i - bpp])
        } else {
            0
        };
        cur[i] = cur[i].wrapping_add(paeth_predictor(left, up, up_left));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BPP: usize = 4;

    fn forward_filter(filter: u8, cur: &[u8], prev: Option<&[u8]>) -> Vec<u8> {
        (0..cur.len())
            .map(|i| {
                let left = if i >= BPP { cur[i - BPP] } else { 0 };
                let up = prev.map_or(0, |p| p[i]);
                let up_left = if i >= BPP {
                    prev.map_or(0, |p| p[i - BPP])
                } else {
                    0
                };
                let predicted = match filter {
                    PNG_FILTER_SUB => left,
                    PNG_FILTER_UP => up,
                    PNG_FILTER_AVERAGE => ((u16::from(left) + u16::from(up)) / 2) as u8,
                    PNG_FILTER_PAETH => paeth_predictor(left, up, up_left),
                    _ => 0,
                };
                cur[i].wrapping_sub(predicted)
            })
            .collect()
    }

    #[test]
    fn every_scanline_filter_is_reversed_exactly() {
        let rows: Vec<Vec<u8>> = (0..5u32)
            .map(|y| {
                (0..12u32)
                    .map(|x| (x * 31 + y * 57 + x * y) as u8)
                    .collect()
            })
            .collect();
        let mut filtered = Vec::new();
        for (y, row) in rows.iter().enumerate() {
            let filter = (y % 5) as u8;
            filtered.push(filter);
            let prev = y.checked_sub(1).map(|p| rows[p].as_slice());
            filtered.extend(forward_filter(filter, row, prev));
        }
        unfilter_scanlines(&mut filtered, 12, BPP).expect("valid filters");
        let restored: Vec<u8> = filtered.chunks(13).flat_map(|r| r[1..].to_vec()).collect();
        assert_eq!(restored, rows.concat());
    }

    #[test]
    fn unknown_filter_type_is_rejected() {
        let mut raw = vec![9u8, 1, 2, 3, 4];
        assert!(unfilter_scanlines(&mut raw, 4, BPP).is_none());
    }
}

//! Header probes for the two picture formats a [`crate::RichImage`] may carry. Both probes are
//! strict and fail closed: a signature, chunk/marker structure, checksum or dimension problem is
//! an error, and only the subset the writers can embed unchanged is accepted.

use crate::{EasyOfficeError, IeeeCrc32};

pub(crate) const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
const JPEG_SOI: [u8; 2] = [0xFF, 0xD8];
const JPEG_EOI: [u8; 2] = [0xFF, 0xD9];
const PNG_IHDR_LEN: usize = 13;
const PNG_CHUNK_OVERHEAD: usize = 12;
const PNG_COLOR_GRAY: u8 = 0;
const PNG_COLOR_RGB: u8 = 2;
pub(crate) const PNG_COLOR_PALETTE: u8 = 3;
const PNG_COLOR_GRAY_ALPHA: u8 = 4;
const PNG_COLOR_RGBA: u8 = 6;
const JPEG_SOF0: u8 = 0xC0;
const JPEG_SOF1: u8 = 0xC1;
const JPEG_SOF2: u8 = 0xC2;
const JPEG_JPG_EXTENSION: u8 = 0xC8;
const JPEG_DAC: u8 = 0xCC;
const JPEG_SOF3: u8 = 0xC3;
const JPEG_SOS: u8 = 0xDA;
const JPEG_RST_FIRST: u8 = 0xD0;
const JPEG_RST_LAST: u8 = 0xD7;
const JPEG_TEM: u8 = 0x01;
const JPEG_FILL: u8 = 0xFF;
const JPEG_SOF_BODY_LEN: usize = 6;

/// Picture container formats accepted by the document model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RichImageFormat {
    Png,
    Jpeg,
}

impl RichImageFormat {
    /// File extension used for the embedded part.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }

    /// IANA media type.
    pub fn mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
        }
    }
}

/// Decides the format from the leading magic bytes only.
pub(crate) fn sniff_format(bytes: &[u8]) -> Option<RichImageFormat> {
    if bytes.starts_with(&PNG_SIGNATURE) {
        Some(RichImageFormat::Png)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(RichImageFormat::Jpeg)
    } else {
        None
    }
}

/// Validated PNG header fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PngHeader {
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub color_type: u8,
    pub interlaced: bool,
}

impl PngHeader {
    /// Samples per pixel.
    pub(crate) fn channels(&self) -> usize {
        match self.color_type {
            PNG_COLOR_RGB => 3,
            PNG_COLOR_GRAY_ALPHA => 2,
            PNG_COLOR_RGBA => 4,
            _ => 1,
        }
    }

    pub(crate) fn has_alpha_channel(&self) -> bool {
        matches!(self.color_type, PNG_COLOR_GRAY_ALPHA | PNG_COLOR_RGBA)
    }

    /// Bytes of one unfiltered scanline (without the filter byte).
    pub(crate) fn row_bytes(&self) -> Option<usize> {
        let bits = (self.width as usize)
            .checked_mul(self.channels())?
            .checked_mul(usize::from(self.bit_depth))?;
        Some(bits.div_ceil(8))
    }
}

/// Validated JPEG frame header fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct JpegHeader {
    pub width: u32,
    pub height: u32,
    pub components: u8,
}

fn corrupt(detail: &str) -> EasyOfficeError {
    EasyOfficeError::CorruptedContainer(format!("그림 데이터가 손상되었습니다: {detail}"))
}

fn be_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let raw = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

fn be_u16(bytes: &[u8], at: usize) -> Option<u16> {
    let raw = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_be_bytes([raw[0], raw[1]]))
}

fn valid_png_depth(color_type: u8, depth: u8) -> bool {
    match color_type {
        PNG_COLOR_GRAY => matches!(depth, 1 | 2 | 4 | 8 | 16),
        PNG_COLOR_PALETTE => matches!(depth, 1 | 2 | 4 | 8),
        PNG_COLOR_RGB | PNG_COLOR_GRAY_ALPHA | PNG_COLOR_RGBA => matches!(depth, 8 | 16),
        _ => false,
    }
}

fn parse_ihdr(data: &[u8]) -> Result<PngHeader, EasyOfficeError> {
    if data.len() != PNG_IHDR_LEN {
        return Err(corrupt("PNG IHDR 길이"));
    }
    let width = be_u32(data, 0).ok_or_else(|| corrupt("PNG 너비"))?;
    let height = be_u32(data, 4).ok_or_else(|| corrupt("PNG 높이"))?;
    let (bit_depth, color_type) = (data[8], data[9]);
    let (compression, filter, interlace) = (data[10], data[11], data[12]);
    if !valid_png_depth(color_type, bit_depth) || compression != 0 || filter != 0 || interlace > 1 {
        return Err(corrupt("PNG IHDR 값"));
    }
    Ok(PngHeader {
        width,
        height,
        bit_depth,
        color_type,
        interlaced: interlace == 1,
    })
}

fn chunk_crc(kind: &[u8], data: &[u8]) -> u32 {
    IeeeCrc32::update(IeeeCrc32::checksum(kind), data)
}

/// Walks every chunk (length, CRC), requires IHDR first, PLTE for palette images, at least one
/// IDAT and a final IEND with nothing after it.
pub(crate) fn probe_png(bytes: &[u8]) -> Result<PngHeader, EasyOfficeError> {
    if !bytes.starts_with(&PNG_SIGNATURE) {
        return Err(corrupt("PNG 시그니처"));
    }
    let mut pos = PNG_SIGNATURE.len();
    let mut header: Option<PngHeader> = None;
    let (mut seen_plte, mut seen_idat) = (false, false);
    loop {
        let len = be_u32(bytes, pos).ok_or_else(|| corrupt("PNG 청크 절단"))? as usize;
        let end = pos
            .checked_add(PNG_CHUNK_OVERHEAD)
            .and_then(|v| v.checked_add(len))
            .ok_or_else(|| corrupt("PNG 청크 길이"))?;
        let chunk = bytes
            .get(pos..end)
            .ok_or_else(|| corrupt("PNG 청크 절단"))?;
        let (kind, data) = (&chunk[4..8], &chunk[8..8 + len]);
        let stored_crc = be_u32(chunk, 8 + len).ok_or_else(|| corrupt("PNG CRC"))?;
        if stored_crc != chunk_crc(kind, data) {
            return Err(corrupt("PNG CRC 불일치"));
        }
        match (kind, header) {
            (b"IHDR", None) => header = Some(parse_ihdr(data)?),
            (_, None) => return Err(corrupt("PNG IHDR 누락")),
            (b"IHDR", Some(_)) => return Err(corrupt("PNG IHDR 중복")),
            (b"PLTE", Some(_)) => seen_plte = true,
            (b"IDAT", Some(_)) => seen_idat = true,
            (b"IEND", Some(h)) => {
                let palette_ok = h.color_type != PNG_COLOR_PALETTE || seen_plte;
                if !seen_idat || !palette_ok || end != bytes.len() {
                    return Err(corrupt("PNG 구조"));
                }
                return Ok(h);
            }
            _ => {}
        }
        pos = end;
    }
}

/// Concatenated IDAT payloads (the zlib stream), after [`probe_png`] validated the structure.
pub(crate) fn png_idat_stream(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut pos = PNG_SIGNATURE.len();
    while let Some(len) = be_u32(bytes, pos).map(|l| l as usize) {
        let data_start = pos + 8;
        let Some(data) = bytes.get(data_start..data_start.saturating_add(len)) else {
            break;
        };
        if bytes.get(pos + 4..pos + 8) == Some(b"IDAT") {
            out.extend_from_slice(data);
        }
        pos = data_start + len + 4;
    }
    out
}

fn is_standalone_marker(marker: u8) -> bool {
    marker == JPEG_TEM || (JPEG_RST_FIRST..=JPEG_RST_LAST).contains(&marker)
}

fn parse_sof(body: &[u8]) -> Result<JpegHeader, EasyOfficeError> {
    if body.len() < JPEG_SOF_BODY_LEN || body[0] != 8 {
        return Err(corrupt("JPEG 프레임 헤더"));
    }
    let height = u32::from(u16::from_be_bytes([body[1], body[2]]));
    let width = u32::from(u16::from_be_bytes([body[3], body[4]]));
    let components = body[5];
    let expected = JPEG_SOF_BODY_LEN + usize::from(components) * 3;
    if width == 0 || height == 0 || components == 0 || body.len() != expected {
        return Err(corrupt("JPEG 프레임 크기"));
    }
    Ok(JpegHeader {
        width,
        height,
        components,
    })
}

/// Reads the next marker code (skipping `FF` fill bytes) and advances past it.
fn next_jpeg_marker(bytes: &[u8], pos: &mut usize) -> Result<u8, EasyOfficeError> {
    while bytes.get(*pos) == Some(&JPEG_FILL) && bytes.get(*pos + 1) == Some(&JPEG_FILL) {
        *pos += 1;
    }
    if bytes.get(*pos) != Some(&JPEG_FILL) {
        return Err(corrupt("JPEG 표지 위치"));
    }
    let marker = *bytes.get(*pos + 1).ok_or_else(|| corrupt("JPEG 절단"))?;
    *pos += 2;
    Ok(marker)
}

/// Scans markers up to the first scan; accepts baseline, extended and progressive Huffman
/// frames, requires an end-of-image marker so truncated files are rejected.
pub(crate) fn probe_jpeg(bytes: &[u8]) -> Result<JpegHeader, EasyOfficeError> {
    if !bytes.starts_with(&JPEG_SOI) {
        return Err(corrupt("JPEG 시그니처"));
    }
    let trimmed_len = bytes.iter().rposition(|b| *b != 0).map_or(0, |p| p + 1);
    if !bytes[..trimmed_len].ends_with(&JPEG_EOI) {
        return Err(corrupt("JPEG 끝 표지 누락"));
    }
    let mut pos = JPEG_SOI.len();
    let mut frame: Option<JpegHeader> = None;
    loop {
        let marker = next_jpeg_marker(bytes, &mut pos)?;
        if is_standalone_marker(marker) {
            continue;
        }
        let seg_len = usize::from(be_u16(bytes, pos).ok_or_else(|| corrupt("JPEG 절단"))?);
        let body = bytes
            .get(pos + 2..pos.saturating_add(seg_len))
            .filter(|_| seg_len >= 2)
            .ok_or_else(|| corrupt("JPEG 구간 길이"))?;
        match marker {
            JPEG_SOF0 | JPEG_SOF1 | JPEG_SOF2 if frame.is_none() => frame = Some(parse_sof(body)?),
            JPEG_SOF3 | 0xC5..=0xCF if marker != JPEG_JPG_EXTENSION && marker != JPEG_DAC => {
                return Err(EasyOfficeError::InvalidArgument(
                    "지원하지 않는 JPEG 방식(무손실·산술 부호화)입니다.".to_string(),
                ));
            }
            JPEG_SOS => return frame.ok_or_else(|| corrupt("JPEG 프레임 헤더 누락")),
            _ => {}
        }
        pos += seg_len;
    }
}

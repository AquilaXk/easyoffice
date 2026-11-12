//! The check every SVG passes before an SVG parser sees it.
//!
//! The SVG parsers behind the PDF export, the preview pictures and Quick Look read a document type
//! declaration and expand the entities it holds, with no bound on the result (a 51 KB declaration
//! of nested entities fills several GB). A page SVG and the SVG pictures a document carries come
//! from the file being opened, so none of them may reach a parser before this check: it inflates a
//! gzip picture within a byte budget (usvg's own reader inflates without one), requires UTF-8, and
//! refuses a declaration with [`XmlSanitizer::validate_safe_xml`].

use crate::deflate::inflate_raw;
use crate::{EasyOfficeError, XmlSanitizer};
use std::borrow::Cow;

const GZIP_MAGIC: [u8; 2] = [0x1F, 0x8B];
const GZIP_METHOD_DEFLATE: u8 = 8;
const GZIP_FIXED_HEADER: usize = 10;
const FLAG_HEADER_CRC: u8 = 0x02;
const FLAG_EXTRA: u8 = 0x04;
const FLAG_NAME: u8 = 0x08;
const FLAG_COMMENT: u8 = 0x10;
const FLAG_RESERVED: u8 = 0xE0;

fn corrupt(message: &str) -> EasyOfficeError {
    EasyOfficeError::CorruptedContainer(message.to_string())
}

/// Start of the deflate stream of a gzip member (RFC 1952 2.3), after the optional header fields.
fn gzip_payload_start(data: &[u8]) -> Result<usize, EasyOfficeError> {
    let bad = || corrupt("SVGZ 머리가 올바르지 않습니다.");
    if data.len() < GZIP_FIXED_HEADER
        || data[2] != GZIP_METHOD_DEFLATE
        || data[3] & FLAG_RESERVED != 0
    {
        return Err(bad());
    }
    let flags = data[3];
    let mut at = GZIP_FIXED_HEADER;
    if flags & FLAG_EXTRA != 0 {
        let len = usize::from(u16::from_le_bytes([
            *data.get(at).ok_or_else(bad)?,
            *data.get(at + 1).ok_or_else(bad)?,
        ]));
        at += 2 + len;
    }
    for flag in [FLAG_NAME, FLAG_COMMENT] {
        if flags & flag != 0 {
            let end = data
                .get(at..)
                .and_then(|rest| rest.iter().position(|&b| b == 0));
            at += end.ok_or_else(bad)? + 1;
        }
    }
    if flags & FLAG_HEADER_CRC != 0 {
        at += 2;
    }
    (at <= data.len()).then_some(at).ok_or_else(bad)
}

/// The text of an SVG picture, inflated when it is gzip, at most `max_bytes` long. Refuses data
/// that is not UTF-8, that has a document type or entity declaration, or whose text (the inflated
/// text for gzip) is longer than the budget: inflation stops at `max_bytes`, the compressed size
/// is not limited here.
pub fn checked_svg_text(data: &[u8], max_bytes: usize) -> Result<Cow<'_, str>, EasyOfficeError> {
    let plain: Cow<[u8]> = if data.starts_with(&GZIP_MAGIC) {
        let start = gzip_payload_start(data)?;
        Cow::Owned(inflate_raw(&data[start..], max_bytes)?)
    } else {
        Cow::Borrowed(data)
    };
    if plain.len() > max_bytes {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "SVG 그림이 허용 크기를 넘습니다.".to_string(),
        ));
    }
    let text = match plain {
        Cow::Borrowed(bytes) => Cow::Borrowed(
            std::str::from_utf8(bytes).map_err(|_| corrupt("SVG가 UTF-8이 아닙니다."))?,
        ),
        Cow::Owned(bytes) => {
            Cow::Owned(String::from_utf8(bytes).map_err(|_| corrupt("SVG가 UTF-8이 아닙니다."))?)
        }
    };
    XmlSanitizer::validate_safe_xml(&text)?;
    Ok(text)
}

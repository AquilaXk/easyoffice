//! Image samples and embedded font programs: streams the text path never reads, so they are
//! neither inflated nor charged to the decode budget. Only their declared size is checked.

use super::{
    dict_get, dict_has_non_null_key, dict_name_eq, resolve_int, resolve_value, PdfIndirectObject,
    PdfObjValue,
};
use crate::EasyOfficeError;
use std::collections::HashMap;

/// Declared pixel count above which an image is refused (about 32768 x 32768).
const MAX_PDF_IMAGE_PIXELS: u64 = 1 << 30;
/// Longest expansion a single DEFLATE stream can reach (RFC 1951 stored-to-fixed worst case).
const MAX_DEFLATE_EXPANSION: u64 = 1032;

/// Streams the text path never reads: image samples and embedded font programs.
/// Font programs carry `/Length1` (`FontFile`, `FontFile2`) or a `FontFile3` `/Subtype`.
pub(in crate::text_pdf_codecs) fn is_opaque_binary_stream(
    entries: &[(String, PdfObjValue)],
) -> bool {
    const FONT_PROGRAM_SUBTYPES: [&str; 3] = ["/Type1C", "/CIDFontType0C", "/OpenType"];
    dict_name_eq(entries, "/Subtype", "/Image")
        || dict_has_non_null_key(entries, "/Length1")
        || FONT_PROGRAM_SUBTYPES
            .iter()
            .any(|sub| dict_name_eq(entries, "/Subtype", sub))
}

/// Checks an opaque stream's declared size without decoding it: the image pixel count is
/// capped, and a lone `/FlateDecode` cannot be shorter than the declared size allows.
pub(in crate::text_pdf_codecs) fn validate_opaque_stream_declaration(
    entries: &[(String, PdfObjValue)],
    objects: &HashMap<(u32, u16), PdfIndirectObject>,
    raw_len: usize,
) -> Result<(), EasyOfficeError> {
    if !dict_name_eq(entries, "/Subtype", "/Image") {
        return Ok(());
    }
    let dim = |key: &str| {
        dict_get(entries, key)
            .and_then(|v| resolve_int(v, objects))
            .and_then(|n| u64::try_from(n).ok())
            .unwrap_or(0)
    };
    let pixels = dim("/Width").saturating_mul(dim("/Height"));
    if pixels > MAX_PDF_IMAGE_PIXELS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "PDF 이미지가 선언한 픽셀 수가 상한(2^30)을 초과했습니다.".to_string(),
        ));
    }
    let lone_flate = matches!(
        dict_get(entries, "/Filter").and_then(|f| resolve_value(f, objects)),
        Some(PdfObjValue::Name(n)) if n == "/FlateDecode" || n == "/Fl"
    );
    let min_decoded = pixels.div_ceil(8);
    let max_reachable = (raw_len as u64 + 1).saturating_mul(MAX_DEFLATE_EXPANSION);
    if lone_flate && min_decoded > max_reachable {
        return Err(EasyOfficeError::CorruptedContainer(
            "PDF 이미지 압축 데이터가 선언한 크기에 비해 지나치게 짧습니다.".to_string(),
        ));
    }
    Ok(())
}

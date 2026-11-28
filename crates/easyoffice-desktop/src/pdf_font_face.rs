//! The face the PDF export falls back to on a machine without a Korean font (PD-114), read from the
//! editor's own Noto Sans KR woff2 (PD-397) instead of a second copy of the font in the installer.
//!
//! The desktop shell registers the woff2 once, from the assets embedded in the executable, before
//! any window or command line mode starts ([`register`]). The font database then loads the decoded
//! face from memory ([`source`]), the same face it used to read from `pdf-fonts/NotoSansKR-Regular.ttf`:
//! the decoded font has the same glyphs, outlines and advances as that file (the test below compares
//! every character), so the PDF text, its search map and the embedded subset come out the same.
//!
//! Reuse (implementation principle): (1) `wuff` (MIT, pure Rust WOFF/WOFF2 decoder, brotli through the
//! `brotli-decompressor` already in the graph) is used as it is; (2) no crate of the graph read woff2;
//! (3) nothing is ported. The file comes from our own bundle, but the decoder is still bounded: an input
//! and an output size limit, checked on the header before the decoder allocates for the output, and a
//! parse of the result before it is registered.

use std::sync::{Arc, OnceLock};
use usvg::fontdb::{Database, Source};

/// Where the editor bundle keeps the face, as an embedded asset key.
pub const FACE_ASSET: &str = "hwp-studio/fonts/NotoSansKR-Regular.woff2";
/// The shipped file is 0.56 MB.
pub const MAX_WOFF2_BYTES: usize = 4 * 1024 * 1024;
/// The decoded font is 2.5 MB.
pub const MAX_FONT_BYTES: usize = 16 * 1024 * 1024;
/// `totalSfntSize` of the WOFF2 header (W3C WOFF2 5.1): signature, flavor, length, numTables, reserved.
const SFNT_SIZE_AT: usize = 16;
const SIGNATURE: &[u8; 4] = b"wOF2";

static BUNDLED: OnceLock<Arc<Vec<u8>>> = OnceLock::new();

/// The sfnt font inside a woff2 file, or why it is refused.
pub fn decode_woff2(woff2: &[u8]) -> Result<Vec<u8>, String> {
    if woff2.len() > MAX_WOFF2_BYTES {
        return Err(format!(
            "woff2 글꼴이 {} MB 상한을 넘습니다.",
            MAX_WOFF2_BYTES >> 20
        ));
    }
    let header = woff2
        .get(..SFNT_SIZE_AT + 4)
        .filter(|header| header.starts_with(SIGNATURE))
        .ok_or("woff2 글꼴 머리가 올바르지 않습니다.")?;
    let declared = u32::from_be_bytes([header[16], header[17], header[18], header[19]]);
    if usize::try_from(declared).map_or(true, |size| size > MAX_FONT_BYTES) {
        return Err(format!(
            "woff2 글꼴이 풀면 {} MB 상한을 넘습니다.",
            MAX_FONT_BYTES >> 20
        ));
    }
    let font = wuff::decompress_woff2(woff2)
        .map_err(|err| format!("woff2 글꼴을 풀지 못했습니다: {err:?}"))?;
    if font.len() > MAX_FONT_BYTES {
        return Err("woff2 글꼴이 풀면 상한을 넘습니다.".to_string());
    }
    Ok(font)
}

/// Decodes `woff2` and keeps it as the bundled face of every font database built afterwards. The
/// first registration wins; the font must parse and hold Latin letters and Hangul syllables.
pub fn register(woff2: &[u8]) -> Result<(), String> {
    let font = decode_woff2(woff2)?;
    let face = ttf_parser::Face::parse(&font, 0)
        .map_err(|err| format!("번들 글꼴을 읽지 못했습니다: {err}"))?;
    if ['A', '한'].iter().any(|c| face.glyph_index(*c).is_none()) {
        return Err("번들 글꼴에 영문 또는 한글 글자가 없습니다.".to_string());
    }
    let _ = BUNDLED.set(Arc::new(font));
    Ok(())
}

/// The registered face as a font database source (shared, not copied), if there is one.
pub(crate) fn source() -> Option<Source> {
    let data: Arc<dyn AsRef<[u8]> + Send + Sync> = BUNDLED.get()?.clone();
    Some(Source::Binary(data))
}

/// Whether `db` holds the registered face.
pub fn has_face(db: &Database) -> bool {
    let Some(bundled) = BUNDLED.get() else {
        return false;
    };
    db.faces().any(|face| match &face.source {
        Source::Binary(data) => (**data).as_ref().as_ptr() == bundled.as_ptr(),
        Source::File(_) | Source::SharedFile(..) => false,
    })
}

#[cfg(test)]
#[path = "pdf_font_face_tests.rs"]
mod tests;

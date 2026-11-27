//! The font variant of a run (PD-368): the PostScript name of the face the run is drawn in (`AppleSDGothicNeo-Light`),
//! next to `RichRun::font`, which holds the font name the word-processor formats write. Only the `.pages` writer uses
//! the face; the other formats keep the run's font name.

/// Longest face name a document keeps, in bytes (the `.pages` writer's font-name limit).
pub const MAX_FONT_FACE_BYTES: usize = 128;

/// A face name from `raw`: trimmed, at most [`MAX_FONT_FACE_BYTES`] bytes, no control characters. Anything else is no
/// face, so a hostile file or request never carries an oversized or unprintable name into a document.
pub fn font_face(raw: &str) -> Option<String> {
    let name = raw.trim();
    (!name.is_empty() && name.len() <= MAX_FONT_FACE_BYTES && !name.chars().any(char::is_control))
        .then(|| name.to_string())
}

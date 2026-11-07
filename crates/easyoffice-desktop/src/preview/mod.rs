//! Embedded thumbnail of a recent document for the start window (PD-106).
//!
//! Hangul files carry a small preview picture (`PrvImage` in the OLE2 container, `Preview/PrvImage.*`
//! in HWPX), OOXML files `docProps/thumbnail.*`, ODF files `Thumbnails/thumbnail.png`. The shell reads
//! only that picture with seeks (the directory and one stream, never the whole file), checks its magic
//! bytes (PNG, JPEG, GIF, BMP) and size, and hands the page the bytes in Base64 for a given opaque
//! recent id. The page never gets a path, and never gets a document's bytes: only this picture.

mod cache;
mod ole2;
mod zip;

use crate::AppWindows;
use cache::{FileStamp, PreviewCache};
use serde::Serialize;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use tauri::{AppHandle, Manager};

/// Largest picture the shell accepts (compressed and uncompressed alike).
pub const MAX_THUMBNAIL_BYTES: usize = 256 * 1024;
const RECENT_ID_LEN: usize = 16;
const OLE2_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const ZIP_MAGIC: [u8; 4] = *b"PK\x03\x04";
const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
const JPEG_MAGIC: [u8; 3] = [0xFF, 0xD8, 0xFF];
const GIF_MAGIC_87: &[u8] = b"GIF87a";
const GIF_MAGIC_89: &[u8] = b"GIF89a";
const BMP_MAGIC: [u8; 2] = *b"BM";

/// A picture found in a document. `mime` comes from the magic bytes, never from a file name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumbnail {
    pub mime: &'static str,
    pub bytes: Vec<u8>,
}

/// What the page receives: the type and the Base64 text of the picture.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct PreviewImage {
    mime: &'static str,
    base64: String,
}

/// The type for the magic bytes of PNG, JPEG, GIF or BMP; anything else (WMF, EMF, ...) is `None`.
pub fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&PNG_MAGIC) {
        Some("image/png")
    } else if bytes.starts_with(&JPEG_MAGIC) {
        Some("image/jpeg")
    } else if bytes.starts_with(GIF_MAGIC_87) || bytes.starts_with(GIF_MAGIC_89) {
        Some("image/gif")
    } else if bytes.starts_with(&BMP_MAGIC) {
        Some("image/bmp")
    } else {
        None
    }
}

/// Keeps the picture only when its bytes are a known image type within the size limit.
fn checked_thumbnail(bytes: Vec<u8>) -> Option<Thumbnail> {
    if bytes.is_empty() || bytes.len() > MAX_THUMBNAIL_BYTES {
        return None;
    }
    let mime = sniff_image(&bytes)?;
    Some(Thumbnail { mime, bytes })
}

/// Reads `buf.len()` bytes at `offset`; `None` when the source is shorter.
pub(crate) fn read_at<R: Read + Seek>(src: &mut R, offset: u64, buf: &mut [u8]) -> Option<()> {
    src.seek(SeekFrom::Start(offset)).ok()?;
    src.read_exact(buf).ok()
}

pub(crate) fn le_u16(bytes: &[u8], at: usize) -> Option<u16> {
    let end = at.checked_add(2)?;
    Some(u16::from_le_bytes(bytes.get(at..end)?.try_into().ok()?))
}

pub(crate) fn le_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let end = at.checked_add(4)?;
    Some(u32::from_le_bytes(bytes.get(at..end)?.try_into().ok()?))
}

/// The embedded picture of the document in `src`, chosen by the container's first bytes.
pub fn extract_thumbnail<R: Read + Seek>(src: &mut R) -> Option<Thumbnail> {
    let mut magic = [0_u8; 8];
    read_at(src, 0, &mut magic)?;
    let bytes = if magic == OLE2_MAGIC {
        ole2::read_prv_image(src)?
    } else if magic.starts_with(&ZIP_MAGIC) {
        zip::read_thumbnail(src)?
    } else {
        return None;
    };
    checked_thumbnail(bytes)
}

/// `true` for the 16 lower-case hex digits of an opaque recent id.
pub fn is_recent_id(id: &str) -> bool {
    id.len() == RECENT_ID_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Managed state: a small cache of the pictures found (or not found) per recent id.
pub struct PreviewState(Mutex<PreviewCache>);

impl PreviewState {
    pub fn new() -> Self {
        Self(Mutex::new(PreviewCache::default()))
    }
}

fn load_preview(state: &PreviewState, id: &str, path: &Path) -> Option<Arc<Thumbnail>> {
    let meta = std::fs::metadata(path).ok()?;
    let stamp = FileStamp::of(&meta);
    if let Some(hit) = state
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(id, &stamp)
    {
        return hit;
    }
    let found = File::open(path)
        .ok()
        .and_then(|mut file| extract_thumbnail(&mut file))
        .map(Arc::new);
    state
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .put(id, stamp, found.clone());
    found
}

fn to_view(thumbnail: &Thumbnail) -> PreviewImage {
    PreviewImage {
        mime: thumbnail.mime,
        base64: easyoffice_formats::base64_codec::encode(&thumbnail.bytes),
    }
}

/// The picture of a recent document (or `None` when it has none) for the start window's card.
#[tauri::command]
pub async fn get_recent_preview(
    app: AppHandle,
    id: String,
) -> Result<Option<PreviewImage>, String> {
    if !is_recent_id(&id) {
        return Err("올바르지 않은 최근 문서 번호입니다.".to_string());
    }
    let Some(path) = app.state::<AppWindows>().recent_path(&id) else {
        return Ok(None);
    };
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<PreviewState>();
        load_preview(&state, &id, &path).map(|found| to_view(&found))
    })
    .await
    .map_err(|err| format!("미리보기를 읽지 못했습니다: {err}"))
}

#[cfg(test)]
mod tests;

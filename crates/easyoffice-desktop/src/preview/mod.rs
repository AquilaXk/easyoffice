//! Embedded thumbnail of a recent document for the start window (PD-106).
//!
//! Hangul files carry a small preview picture (`PrvImage` in the OLE2 container, `Preview/PrvImage.*`
//! in HWPX), OOXML files `docProps/thumbnail.*`, ODF files `Thumbnails/thumbnail.png`. The shell reads
//! only that picture with seeks (the directory and one stream, never the whole file), checks its magic
//! bytes (PNG, JPEG, GIF, BMP) and size, and hands the page the bytes in Base64 for a given opaque
//! recent id. The page never gets a path, and never gets a document's bytes: only this picture.
//!
//! A file without such a picture gets its first page drawn instead. A Hangul file (PD-142): rhwp lays
//! the page out, `usvg`/`resvg` draw it as a PNG. A PDF (PD-179): the PDF worker process draws it.
//! Any other document: a card of its first paragraphs, cells or slide is drawn from the content the
//! engine reads (`card_source.rs`). Each runs on its own bounded worker (`bounded.rs`) and fails to
//! nothing, which the page shows as the skeleton card.

mod bounded;
mod cache;
mod card;
mod card_source;
mod ole2;
mod pdf_page;
mod render;
mod source;
mod zip;

use crate::pdf_fonts::shared_font_db;
use crate::shell_caller::ShellCaller;
use crate::AppWindows;
use bounded::{Gate, RenderError};
use cache::{FileStamp, PreviewCache};
use serde::Serialize;
use source::Source;
use std::fs::{File, Metadata};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tauri::{AppHandle, Manager};

/// Largest picture the shell accepts from inside a file (compressed and uncompressed alike).
pub const MAX_THUMBNAIL_BYTES: usize = 256 * 1024;
/// Largest picture the shell hands the page for a drawn first page (the page refuses more).
pub const MAX_RENDERED_PNG_BYTES: usize = 1024 * 1024;
/// Time one first-page render may take before the card stays empty.
const RENDER_DEADLINE: Duration = Duration::from_secs(10);
/// Time a request waits for a free render thread before it gives up.
const PERMIT_WAIT: Duration = Duration::from_secs(2);
/// Render threads alive at once, running or stuck past their deadline.
const MAX_RENDER_WORKERS: usize = 2;
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

/// Managed state: a small cache of the pictures found (or not found) per recent id, the limits of
/// the first-page renders and the folders of the bundled fonts they draw with.
pub struct PreviewState {
    cache: Mutex<PreviewCache>,
    gate: Arc<Gate>,
    limits: RenderLimits,
    font_dirs: Arc<[PathBuf]>,
}

#[derive(Clone, Copy)]
struct RenderLimits {
    deadline: Duration,
    permit_wait: Duration,
}

impl PreviewState {
    pub fn new() -> Self {
        Self::with_font_dirs(Vec::new())
    }

    pub fn with_font_dirs(font_dirs: Vec<PathBuf>) -> Self {
        Self::with_limits(
            font_dirs,
            MAX_RENDER_WORKERS,
            RenderLimits {
                deadline: RENDER_DEADLINE,
                permit_wait: PERMIT_WAIT,
            },
        )
    }

    fn with_limits(font_dirs: Vec<PathBuf>, workers: usize, limits: RenderLimits) -> Self {
        Self {
            cache: Mutex::new(PreviewCache::default()),
            gate: Gate::new(workers),
            limits,
            font_dirs: font_dirs.into(),
        }
    }

    fn cached(&self, id: &str, stamp: &FileStamp) -> Option<Option<Arc<Thumbnail>>> {
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id, stamp)
    }

    fn remember(&self, id: &str, stamp: FileStamp, found: Option<Arc<Thumbnail>>) {
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .put(id, stamp, found);
    }
}

/// The picture of a recent document: the one inside the file, else its first page.
pub(crate) fn load_preview(state: &PreviewState, id: &str, path: &Path) -> Option<Arc<Thumbnail>> {
    let font_dirs = Arc::clone(&state.font_dirs);
    let source = Source::of(path);
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    load_preview_with(state, id, path, move |document| match source {
        Some(Source::Hangul) => render::first_page_png(&document, &shared_font_db(&font_dirs)),
        Some(Source::Pdf) => pdf_page::first_page_png(document, &shared_font_db(&font_dirs)),
        Some(Source::Document | Source::Text) => {
            card_source::card_png(&document, &extension, &shared_font_db(&font_dirs))
        }
        None => Err(RenderError::Unreadable),
    })
}

/// [`load_preview`] with the first-page step given by the caller (a test hook; the real step runs
/// on the bounded worker either way). An answer is cached for the file's time and size, a miss
/// too, except when no render thread was free: that one is the shell's state, not the file's.
fn load_preview_with(
    state: &PreviewState,
    id: &str,
    path: &Path,
    render: impl FnOnce(Vec<u8>) -> Result<Vec<u8>, RenderError> + Send + 'static,
) -> Option<Arc<Thumbnail>> {
    let meta = std::fs::metadata(path).ok()?;
    let stamp = FileStamp::of(&meta);
    if let Some(hit) = state.cached(id, &stamp) {
        return hit;
    }
    let found = match File::open(path) {
        Err(_) => None,
        Ok(mut file) => match extract_thumbnail(&mut file) {
            Some(embedded) => Some(Arc::new(embedded)),
            None => match draw_first_page(state, file, path, &meta, render) {
                Ok(drawn) => Some(Arc::new(drawn)),
                Err(RenderError::Busy) => return None,
                Err(_) => None,
            },
        },
    };
    state.remember(id, stamp, found.clone());
    found
}

/// The part of `source` a drawing of this kind reads: the whole file within the kind's limit, or
/// for a text file its start cut at a line break. The file may have grown since its size was
/// read, so one byte past the limit is read and refused.
fn read_document<R: Read + Seek>(
    source: &mut R,
    size_hint: u64,
    kind: Source,
) -> Result<Vec<u8>, RenderError> {
    let limit = kind.read_bytes();
    let mut document = Vec::with_capacity(size_hint.min(limit as u64) as usize);
    source
        .seek(SeekFrom::Start(0))
        .and_then(|_| source.take(limit as u64 + 1).read_to_end(&mut document))
        .map_err(|_| RenderError::Unreadable)?;
    if document.len() <= limit {
        return Ok(document);
    }
    if kind != Source::Text {
        return Err(RenderError::TooLarge);
    }
    document.truncate(limit);
    Ok(source::whole_lines(document))
}

/// The first page of a document within the size limit of its kind, drawn by the bounded worker. The
/// file is read on the worker, after it holds a render permit: a request that finds none free reads
/// nothing, so a stream of them cannot each hold up to 50 MiB of file bytes.
fn draw_first_page<R: Read + Seek + Send + 'static>(
    state: &PreviewState,
    mut source: R,
    path: &Path,
    meta: &Metadata,
    render: impl FnOnce(Vec<u8>) -> Result<Vec<u8>, RenderError> + Send + 'static,
) -> Result<Thumbnail, RenderError> {
    let kind = Source::of(path)
        .filter(|kind| meta.len() <= kind.max_file_bytes())
        .ok_or(RenderError::Unreadable)?;
    let size = meta.len();
    let limits = state.limits;
    let png = bounded::run(
        &state.gate,
        limits.permit_wait,
        limits.deadline,
        move || render(read_document(&mut source, size, kind)?),
    )?;
    if png.len() > MAX_RENDERED_PNG_BYTES || sniff_image(&png) != Some("image/png") {
        return Err(RenderError::TooLarge);
    }
    Ok(Thumbnail {
        mime: "image/png",
        bytes: png,
    })
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
    _caller: ShellCaller,
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
mod card_tests;
#[cfg(test)]
mod render_tests;
#[cfg(test)]
mod tests;

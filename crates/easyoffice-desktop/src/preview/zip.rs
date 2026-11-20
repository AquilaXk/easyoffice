//! The thumbnail entry of a ZIP based document, found through the central directory with seeks.
//!
//! The `zip` crate reads the directory and locates the entry; the end record is read here first so
//! a directory over the shell's limits never reaches the crate, and the entry's raw bytes go through
//! the engine's budgeted inflater instead of the crate's own (output limit free) decoder.

use super::{le_u16, le_u32, read_at, MAX_THUMBNAIL_BYTES};
use std::io::{Read, Seek, SeekFrom};
use zip::read::{ArchiveOffset, Config};
use zip::{CompressionMethod, ZipArchive};

const EOCD_SIGNATURE: u32 = 0x0605_4B50;
const EOCD_FIXED_LEN: usize = 22;
/// The end record may be followed by a comment of up to 65535 bytes.
const EOCD_SEARCH_LEN: u64 = 65_535 + EOCD_FIXED_LEN as u64;
const MAX_ENTRIES: usize = 8_192;
const MAX_CENTRAL_DIR_BYTES: u32 = 4 * 1024 * 1024;
const ZIP64_MARKER_16: u16 = 0xFFFF;
const ZIP64_MARKER_32: u32 = 0xFFFF_FFFF;
/// Name stems of the thumbnail entries, best first: HWPX, OOXML, ODF, then Pages (`QuickLook/` in
/// the 2009 package, `preview` and the smaller `preview-web` since 2013; `preview-micro` is a
/// few hundred bytes wide and never shown).
const STEMS: [&str; 6] = [
    "Preview/PrvImage",
    "docProps/thumbnail",
    "Thumbnails/thumbnail",
    "QuickLook/Thumbnail",
    "preview",
    "preview-web",
];
const EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "gif", "bmp"];

fn strip_pages_dir_prefix(name: &str) -> &str {
    let Some((dir, rest)) = name.split_once('/') else {
        return name;
    };
    if dir.len() > 6
        && dir.as_bytes()[dir.len() - 6..].eq_ignore_ascii_case(b".pages")
        && !rest.is_empty()
    {
        rest
    } else {
        name
    }
}

/// Position of the entry's name in [`STEMS`] (0 is best), `None` when it is no thumbnail name.
fn thumbnail_rank(name: &[u8]) -> Option<usize> {
    let name = strip_pages_dir_prefix(std::str::from_utf8(name).ok()?);
    STEMS.iter().position(|stem| {
        let Some(rest) = name.get(stem.len()..) else {
            return false;
        };
        name.get(..stem.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(stem))
            && rest
                .strip_prefix('.')
                .is_some_and(|ext| EXTENSIONS.iter().any(|e| ext.eq_ignore_ascii_case(e)))
    })
}

/// Size and entry count of the central directory, from the end record near the end of the file.
/// `None` for a ZIP64 marker or a directory over the shell's limits.
fn central_directory_limits<R: Read + Seek>(src: &mut R) -> Option<(u32, usize)> {
    let len = src.seek(SeekFrom::End(0)).ok()?;
    let window = len.min(EOCD_SEARCH_LEN);
    let mut tail = vec![0_u8; usize::try_from(window).ok()?];
    read_at(src, len.checked_sub(window)?, &mut tail)?;
    let at = (0..=tail.len().checked_sub(EOCD_FIXED_LEN)?)
        .rev()
        .find(|&i| le_u32(&tail, i) == Some(EOCD_SIGNATURE))?;
    let entries = le_u16(&tail, at + 10)?;
    let size = le_u32(&tail, at + 12)?;
    let offset = le_u32(&tail, at + 16)?;
    if entries == ZIP64_MARKER_16 || size == ZIP64_MARKER_32 || offset == ZIP64_MARKER_32 {
        return None;
    }
    let entries = usize::from(entries);
    (size != 0 && size <= MAX_CENTRAL_DIR_BYTES && entries <= MAX_ENTRIES)
        .then_some((size, entries))
}

/// The archive in `src` when its end record is within the limits above.
fn open<R: Read + Seek>(src: &mut R) -> Option<ZipArchive<&mut R>> {
    central_directory_limits(src)?;
    let config = Config {
        archive_offset: ArchiveOffset::Known(0),
    };
    ZipArchive::with_config(config, src).ok()
}

/// What the central directory declares the parts of the package to inflate to, added up. `None`
/// when the directory cannot be read (damaged, ZIP64) or is over the shell's limits.
pub fn declared_total<R: Read + Seek>(src: &mut R) -> Option<u64> {
    let mut archive = open(src)?;
    let mut total = 0_u64;
    for index in 0..archive.len() {
        total = total.checked_add(archive.by_index_raw(index).ok()?.size())?;
    }
    Some(total)
}

pub fn read_thumbnail<R: Read + Seek>(src: &mut R) -> Option<Vec<u8>> {
    let mut archive = open(src)?;
    let limit = u64::try_from(MAX_THUMBNAIL_BYTES).ok()?;
    // The best-ranked entry that may be read at all (not encrypted, within the picture limit);
    // the first one in directory order wins a tie.
    let mut candidates: Vec<(usize, usize)> = archive
        .file_names()
        .enumerate()
        .filter_map(|(index, name)| Some((thumbnail_rank(name.as_bytes())?, index)))
        .collect();
    candidates.sort_by_key(|&(rank, index)| (rank, index));
    for (_, index) in candidates {
        let mut file = archive.by_index_raw(index).ok()?;
        if file.encrypted() || file.compressed_size() > limit || file.size() > limit {
            continue;
        }
        let method = file.compression();
        let mut data = Vec::with_capacity(usize::try_from(file.compressed_size()).ok()?);
        file.by_ref().take(limit + 1).read_to_end(&mut data).ok()?;
        return if method == CompressionMethod::STORE {
            Some(data)
        } else if method == CompressionMethod::DEFLATE {
            easyoffice_formats::inflate_raw(&data, MAX_THUMBNAIL_BYTES).ok()
        } else {
            None
        };
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_pages_dir_prefix_handles_multibyte_utf8_names_without_panic() {
        assert_eq!(
            strip_pages_dir_prefix("가나다A/Preview/PrvImage.png"),
            "가나다A/Preview/PrvImage.png"
        );
        assert_eq!(
            thumbnail_rank("가나다A/Preview/PrvImage.png".as_bytes()),
            None
        );
        assert_eq!(
            strip_pages_dir_prefix("한글문서.pages/QuickLook/Thumbnail.png"),
            "QuickLook/Thumbnail.png"
        );
        assert_eq!(
            thumbnail_rank("한글문서.pages/QuickLook/Thumbnail.png".as_bytes()),
            Some(3)
        );
    }
}

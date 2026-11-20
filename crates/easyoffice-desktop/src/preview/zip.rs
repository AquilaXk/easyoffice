//! The thumbnail entry of a ZIP based document, found through the central directory with seeks.
//!
//! The `zip` crate reads the directory and locates the entry, opened through the engine's
//! `open_guarded` so a directory over the shell's limits (or one that would make the crate fall back
//! to an older end record) never reaches it, and the entry's raw bytes go through the engine's
//! budgeted inflater instead of the crate's own (output limit free) decoder.

use super::MAX_THUMBNAIL_BYTES;
use easyoffice_formats::{open_guarded, GuardedReader, OpenLimits};
use std::io::{Read, Seek};
use zip::{CompressionMethod, ZipArchive};

const MAX_ENTRIES: usize = 8_192;
const MAX_CENTRAL_DIR_BYTES: u32 = 4 * 1024 * 1024;
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

/// The archive in `src` when its end record and directory pass the checks of `open_guarded`
/// (single end record, no ZIP64, within the shell's limits).
fn open<R: Read + Seek>(src: &mut R) -> Option<ZipArchive<GuardedReader<&mut R>>> {
    let limits = OpenLimits {
        max_entries: MAX_ENTRIES,
        max_directory_bytes: u64::from(MAX_CENTRAL_DIR_BYTES),
    };
    let opened = open_guarded(src, limits).ok()?;
    (opened.total_entries > 0).then_some(opened.archive)
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

    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use std::io::Cursor;

    /// Records the largest single allocation of the thread that asks for it.
    struct Tracking;

    thread_local! {
        static LARGEST: Cell<usize> = const { Cell::new(0) };
    }

    // SAFETY: every call is forwarded unchanged to the system allocator.
    unsafe impl GlobalAlloc for Tracking {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let _ = LARGEST.try_with(|largest| largest.set(largest.get().max(layout.size())));
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            let _ = LARGEST.try_with(|largest| largest.set(largest.get().max(new_size)));
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    #[global_allocator]
    static ALLOCATOR: Tracking = Tracking;

    const HUGE_COUNT: u64 = 200_000;

    fn eocd(entries: u16, size: u32, offset: u32) -> Vec<u8> {
        let mut out = vec![0x50, 0x4B, 0x05, 0x06, 0, 0, 0, 0];
        out.extend_from_slice(&entries.to_le_bytes());
        out.extend_from_slice(&entries.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    /// Padding, then (older to newer) a ZIP64 end record announcing [`HUGE_COUNT`] entries, its
    /// locator, an end record that points at them, and last the end record that really ends the
    /// file: one entry whose directory offset has no directory record behind it.
    fn hostile_archive() -> Vec<u8> {
        let mut out = vec![0u8; HUGE_COUNT as usize * 47];
        let eocd64_at = out.len() as u64;
        out.extend_from_slice(&[0x50, 0x4B, 0x06, 0x06]);
        out.extend_from_slice(&44u64.to_le_bytes());
        out.extend_from_slice(&[45, 0, 45, 0]);
        out.extend_from_slice(&[0; 8]);
        out.extend_from_slice(&HUGE_COUNT.to_le_bytes());
        out.extend_from_slice(&HUGE_COUNT.to_le_bytes());
        out.extend_from_slice(&0u64.to_le_bytes());
        out.extend_from_slice(&HUGE_COUNT.to_le_bytes());
        out.extend_from_slice(&[0x50, 0x4B, 0x06, 0x07]);
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&eocd64_at.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&eocd(0xFFFF, 0xFFFF_FFFF, 0xFFFF_FFFF));
        out.extend_from_slice(&eocd(1, 0, 0));
        out
    }

    #[test]
    fn an_older_zip64_end_record_is_never_tried() {
        let archive = hostile_archive();
        LARGEST.with(|largest| largest.set(0));
        let thumbnail = read_thumbnail(&mut Cursor::new(&archive));
        let total = declared_total(&mut Cursor::new(&archive));
        let largest = LARGEST.with(Cell::get);
        assert!(thumbnail.is_none() && total.is_none());
        assert!(largest < 8 * 1024 * 1024, "reserved {largest} bytes");
    }
}

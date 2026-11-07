//! The thumbnail entry of a ZIP based document, found through the central directory with seeks.

use super::{le_u16, le_u32, read_at, MAX_THUMBNAIL_BYTES};
use std::io::{Read, Seek, SeekFrom};

const EOCD_SIGNATURE: u32 = 0x0605_4B50;
const CENTRAL_SIGNATURE: u32 = 0x0201_4B50;
const LOCAL_SIGNATURE: u32 = 0x0403_4B50;
const EOCD_FIXED_LEN: usize = 22;
/// The end record may be followed by a comment of up to 65535 bytes.
const EOCD_SEARCH_LEN: u64 = 65_535 + EOCD_FIXED_LEN as u64;
const CENTRAL_FIXED_LEN: usize = 46;
const LOCAL_FIXED_LEN: usize = 30;
const MAX_ENTRIES: usize = 8_192;
const MAX_CENTRAL_DIR_BYTES: u32 = 4 * 1024 * 1024;
const ZIP64_MARKER_16: u16 = 0xFFFF;
const ZIP64_MARKER_32: u32 = 0xFFFF_FFFF;
const METHOD_STORED: u16 = 0;
const METHOD_DEFLATED: u16 = 8;
const FLAG_ENCRYPTED: u16 = 1;
/// Name stems of the thumbnail entries: HWPX, OOXML and ODF.
const STEMS: [&str; 3] = [
    "Preview/PrvImage",
    "docProps/thumbnail",
    "Thumbnails/thumbnail",
];
const EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "gif", "bmp"];

struct CentralEntry {
    flags: u16,
    method: u16,
    compressed: u32,
    uncompressed: u32,
    local_offset: u32,
}

fn is_thumbnail_name(name: &[u8]) -> bool {
    let Ok(name) = std::str::from_utf8(name) else {
        return false;
    };
    STEMS.iter().any(|stem| {
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

/// Offset and size of the central directory, from the end record near the end of the file.
fn find_central_directory<R: Read + Seek>(src: &mut R) -> Option<(u64, u32, usize)> {
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
    Some((u64::from(offset), size, usize::from(entries)))
}

/// The first thumbnail entry of the central directory, with its sizes and position.
fn find_entry(directory: &[u8], entries: usize) -> Option<CentralEntry> {
    let mut at = 0_usize;
    for _ in 0..entries.min(MAX_ENTRIES) {
        if le_u32(directory, at)? != CENTRAL_SIGNATURE {
            return None;
        }
        let name_len = usize::from(le_u16(directory, at + 28)?);
        let extra_len = usize::from(le_u16(directory, at + 30)?);
        let comment_len = usize::from(le_u16(directory, at + 32)?);
        let name_start = at.checked_add(CENTRAL_FIXED_LEN)?;
        let name = directory.get(name_start..name_start.checked_add(name_len)?)?;
        if is_thumbnail_name(name) {
            return Some(CentralEntry {
                flags: le_u16(directory, at + 8)?,
                method: le_u16(directory, at + 10)?,
                compressed: le_u32(directory, at + 20)?,
                uncompressed: le_u32(directory, at + 24)?,
                local_offset: le_u32(directory, at + 42)?,
            });
        }
        at = name_start
            .checked_add(name_len)?
            .checked_add(extra_len)?
            .checked_add(comment_len)?;
    }
    None
}

fn read_entry_data<R: Read + Seek>(src: &mut R, entry: &CentralEntry) -> Option<Vec<u8>> {
    let limit = u32::try_from(MAX_THUMBNAIL_BYTES).ok()?;
    if entry.flags & FLAG_ENCRYPTED != 0 || entry.compressed > limit || entry.uncompressed > limit {
        return None;
    }
    let mut local = [0_u8; LOCAL_FIXED_LEN];
    let local_offset = u64::from(entry.local_offset);
    read_at(src, local_offset, &mut local)?;
    if le_u32(&local, 0)? != LOCAL_SIGNATURE {
        return None;
    }
    let skip = u64::from(le_u16(&local, 26)?).checked_add(u64::from(le_u16(&local, 28)?))?;
    let data_at = local_offset
        .checked_add(LOCAL_FIXED_LEN as u64)?
        .checked_add(skip)?;
    let mut data = vec![0_u8; usize::try_from(entry.compressed).ok()?];
    read_at(src, data_at, &mut data)?;
    match entry.method {
        METHOD_STORED => Some(data),
        METHOD_DEFLATED => easyoffice_formats::inflate_raw(&data, MAX_THUMBNAIL_BYTES).ok(),
        _ => None,
    }
}

pub fn read_thumbnail<R: Read + Seek>(src: &mut R) -> Option<Vec<u8>> {
    let (offset, size, entries) = find_central_directory(src)?;
    if size == 0 || size > MAX_CENTRAL_DIR_BYTES {
        return None;
    }
    let mut directory = vec![0_u8; usize::try_from(size).ok()?];
    read_at(src, offset, &mut directory)?;
    let entry = find_entry(&directory, entries)?;
    read_entry_data(src, &entry)
}

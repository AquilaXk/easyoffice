//! Opening a ZIP so that the `zip` crate sees exactly the one end record checked here.
//!
//! When the crate cannot use an end record (an unreadable directory, no directory entry at the
//! offset) it silently falls back to an older one, and a ZIP64 end record there can announce any
//! entry count: the crate reserves memory for the count before it reads a byte of the directory.
//! So the end record, the directory bounds and the absence of any other end-record signature are
//! checked first, and the crate then opens a view of the file in which everything before the
//! directory reads as zeros, which leaves it no older record to fall back to. The mask is lifted
//! once the archive is open, for the entry data.

use crate::EasyOfficeError;
use std::cell::Cell;
use std::io::{self, Read, Seek, SeekFrom};
use std::rc::Rc;
use zip::read::{ArchiveOffset, Config};
use zip::ZipArchive;

const EOCD_SIG: [u8; 4] = [0x50, 0x4B, 0x05, 0x06];
const ZIP64_EOCD_SIG: [u8; 4] = [0x50, 0x4B, 0x06, 0x06];
const ZIP64_LOCATOR_SIG: [u8; 4] = [0x50, 0x4B, 0x06, 0x07];
const CENTRAL_SIG: [u8; 4] = [0x50, 0x4B, 0x01, 0x02];
/// Fixed part of the end record.
const EOCD_LEN: usize = 22;
/// The end record may be followed by a comment of up to 65535 bytes.
const MAX_COMMENT: usize = 65_535;
/// Signature (4) plus the locator's fields (16).
const ZIP64_LOCATOR_LEN: usize = 20;
/// Fixed part of a central directory record.
const CENTRAL_FIXED_LEN: u64 = 46;
const SCAN_CHUNK: usize = 64 * 1024;

/// What an archive may declare before the crate is allowed to read its directory.
#[derive(Debug, Clone, Copy)]
pub struct OpenLimits {
    pub max_entries: usize,
    pub max_directory_bytes: u64,
}

/// An opened archive with the numbers of the end record that was checked.
pub struct GuardedArchive<R> {
    pub archive: ZipArchive<GuardedReader<R>>,
    pub total_entries: usize,
    pub directory_start: u64,
    pub directory_end: u64,
}

/// The offset of the end record whose comment reaches exactly the end of `bytes`, searching the last
/// 64 KiB; `None` when there is none. This is stricter than the crate, which accepts data after
/// the comment.
pub(crate) fn find_end_record(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < EOCD_LEN {
        return None;
    }
    let min_offset = bytes.len().saturating_sub(EOCD_LEN + MAX_COMMENT);
    let mut search_end = bytes.len() - (EOCD_LEN - 1);
    while search_end > min_offset {
        let rel = bytes[min_offset..search_end]
            .iter()
            .rposition(|&b| b == 0x50)?;
        let pos = min_offset + rel;
        if bytes[pos..pos + 4] == EOCD_SIG {
            let comment_len = usize::from(u16::from_le_bytes([bytes[pos + 20], bytes[pos + 21]]));
            if pos + EOCD_LEN + comment_len == bytes.len() {
                return Some(pos);
            }
        }
        search_end = pos;
    }
    None
}

fn corrupt(what: &str) -> EasyOfficeError {
    EasyOfficeError::CorruptedContainer(what.to_string())
}

fn security(what: &str) -> EasyOfficeError {
    EasyOfficeError::SecurityViolation(what.to_string())
}

fn io_error(err: io::Error) -> EasyOfficeError {
    EasyOfficeError::CorruptedContainer(format!("PKZIP 아카이브를 읽을 수 없습니다: {err}"))
}

fn le16(bytes: &[u8], at: usize) -> usize {
    usize::from(u16::from_le_bytes([bytes[at], bytes[at + 1]]))
}

fn le32(bytes: &[u8], at: usize) -> u64 {
    u64::from(u32::from_le_bytes([
        bytes[at],
        bytes[at + 1],
        bytes[at + 2],
        bytes[at + 3],
    ]))
}

/// Checks the end record and the directory, then opens the archive through the masked view.
pub fn open_guarded<R: Read + Seek>(
    mut reader: R,
    limits: OpenLimits,
) -> Result<GuardedArchive<R>, EasyOfficeError> {
    let len = reader.seek(SeekFrom::End(0)).map_err(io_error)?;
    // The window holds the locator that may precede the last possible end record too.
    let window = len.min((EOCD_LEN + MAX_COMMENT + ZIP64_LOCATOR_LEN) as u64);
    let mut tail = vec![0_u8; window as usize];
    reader
        .seek(SeekFrom::Start(len - window))
        .map_err(io_error)?;
    reader.read_exact(&mut tail).map_err(io_error)?;
    let window_start = len - window;

    let eocd = find_end_record(&tail).ok_or_else(|| {
        corrupt("PKZIP End of Central Directory(0x06054B50) 레코드를 찾을 수 없습니다.")
    })?;
    let total_entries = le16(&tail, eocd + 10);
    if le16(&tail, eocd + 4) != 0
        || le16(&tail, eocd + 6) != 0
        || le16(&tail, eocd + 8) != total_entries
    {
        return Err(corrupt("다중 디스크 ZIP 아카이브는 지원하지 않습니다."));
    }
    let size = le32(&tail, eocd + 12);
    let start = le32(&tail, eocd + 16);
    let zip64_locator = eocd >= ZIP64_LOCATOR_LEN
        && tail[eocd - ZIP64_LOCATOR_LEN..].starts_with(&ZIP64_LOCATOR_SIG);
    if total_entries == usize::from(u16::MAX)
        || start == u64::from(u32::MAX)
        || size == u64::from(u32::MAX)
        || zip64_locator
    {
        return Err(corrupt("ZIP64 아카이브는 지원하지 않습니다."));
    }
    if total_entries > limits.max_entries {
        return Err(security(&format!(
            "ZIP 엔트리 개수({total_entries})가 허용 상한({})을 초과했습니다.",
            limits.max_entries
        )));
    }
    let eocd_at = window_start + eocd as u64;
    if start + size != eocd_at {
        return Err(corrupt(
            "PKZIP Central Directory 오프셋 또는 크기가 유효하지 않거나 End of Central Directory와 맞닿지 않습니다.",
        ));
    }
    if size > limits.max_directory_bytes {
        return Err(security("ZIP Central Directory가 허용 크기를 넘습니다."));
    }
    if size < total_entries as u64 * CENTRAL_FIXED_LEN {
        return Err(corrupt(
            "PKZIP Central Directory가 엔트리 수에 비해 너무 작습니다.",
        ));
    }

    // A second end record makes tools disagree on which directory is the archive's: one inside the
    // comment (the crate accepts it when data follows), or an earlier one whose comment also
    // reaches the end of the file (the strict search above takes the last).
    let in_comment = tail[eocd + EOCD_LEN..].windows(4).any(|w| w == EOCD_SIG);
    let earlier_fit = tail[..eocd]
        .windows(EOCD_LEN)
        .enumerate()
        .any(|(at, record)| {
            record.starts_with(&EOCD_SIG) && at + EOCD_LEN + le16(record, 20) == tail.len()
        });
    if in_comment || earlier_fit {
        return Err(security(
            "ZIP에 End of Central Directory 기록이 둘 이상 있습니다.",
        ));
    }

    check_directory(&mut reader, start, size)?;

    let mask = Rc::new(Cell::new(start));
    let view = GuardedReader {
        inner: reader,
        len,
        pos: 0,
        inner_pos: None,
        mask_below: Rc::clone(&mask),
    };
    let config = Config {
        archive_offset: ArchiveOffset::Known(0),
    };
    let archive = ZipArchive::with_config(config, view).map_err(|err| {
        EasyOfficeError::CorruptedContainer(format!("PKZIP 아카이브를 읽을 수 없습니다: {err}"))
    })?;
    mask.set(0);
    Ok(GuardedArchive {
        archive,
        total_entries,
        directory_start: start,
        directory_end: eocd_at,
    })
}

/// The directory starts with a directory record, and no ZIP64 or second end-record signature hides in
/// it (a name or an extra field could carry one for the crate to fall back to).
fn check_directory<R: Read + Seek>(
    reader: &mut R,
    start: u64,
    size: u64,
) -> Result<(), EasyOfficeError> {
    if size == 0 {
        return Ok(());
    }
    reader.seek(SeekFrom::Start(start)).map_err(io_error)?;
    let mut remaining = size;
    let mut carry: Vec<u8> = Vec::new();
    let mut chunk = vec![0_u8; SCAN_CHUNK.min(size as usize)];
    let mut first = true;
    while remaining > 0 {
        let take = remaining.min(chunk.len() as u64) as usize;
        reader.read_exact(&mut chunk[..take]).map_err(io_error)?;
        if first {
            first = false;
            if take < 4 || chunk[..4] != CENTRAL_SIG {
                return Err(corrupt(
                    "PKZIP Central Directory 첫 엔트리가 오프셋에 없습니다.",
                ));
            }
        }
        let mut window = std::mem::take(&mut carry);
        window.extend_from_slice(&chunk[..take]);
        let hidden = window
            .windows(4)
            .any(|w| w == EOCD_SIG || w == ZIP64_EOCD_SIG || w == ZIP64_LOCATOR_SIG);
        if hidden {
            return Err(security(
                "ZIP Central Directory 안에 다른 End of Central Directory 기록의 서명이 있습니다.",
            ));
        }
        carry = window[window.len().saturating_sub(3)..].to_vec();
        remaining -= take as u64;
    }
    Ok(())
}

/// A `Read + Seek` view of the file in which the bytes before `mask_below` read as zeros.
pub struct GuardedReader<R> {
    inner: R,
    len: u64,
    pos: u64,
    /// Where `inner` is positioned, when known, to skip seeks that change nothing.
    inner_pos: Option<u64>,
    mask_below: Rc<Cell<u64>>,
}

impl<R: Read + Seek> Read for GuardedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() || self.pos >= self.len {
            return Ok(0);
        }
        let mask = self.mask_below.get();
        if self.pos < mask {
            let zeros = ((mask - self.pos).min(buf.len() as u64)) as usize;
            buf[..zeros].fill(0);
            self.pos += zeros as u64;
            return Ok(zeros);
        }
        if self.inner_pos != Some(self.pos) {
            self.inner.seek(SeekFrom::Start(self.pos))?;
        }
        let read = self.inner.read(buf);
        self.inner_pos = read.as_ref().ok().map(|&n| self.pos + n as u64);
        if let Ok(n) = read {
            self.pos += n as u64;
        }
        read
    }
}

impl<R: Read + Seek> Seek for GuardedReader<R> {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let target = match to {
            SeekFrom::Start(offset) => Some(offset),
            SeekFrom::End(delta) => self.len.checked_add_signed(delta),
            SeekFrom::Current(delta) => self.pos.checked_add_signed(delta),
        }
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek before the start"))?;
        self.pos = target;
        Ok(target)
    }
}

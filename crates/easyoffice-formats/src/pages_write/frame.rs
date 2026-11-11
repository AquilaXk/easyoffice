//! IWA framing: records (`ArchiveInfo` + payload), the record stream and its Snappy chunks.
//!
//! A `.iwa` file is a sequence of chunks, each a 4-byte header (type 0, 24-bit little-endian
//! length) and one raw Snappy block that decompresses to at most 64 KiB. The concatenated
//! decompressed chunks are the record stream: per object a varint-length `ArchiveInfo`, then the
//! payload of each of its `MessageInfo`s. Object framing does not depend on chunk borders.
//!
//! The Snappy blocks are written as literals only. That is a valid block for every decoder, needs
//! no hash table, and keeps the output deterministic; the files are small (text and styles), and
//! the container around them is not compressed either.
//!
//! 참조 구현: iwork-rs (MIT, Copyright (c) 2026 Zebra Pig), `src/iwa.rs` 의 `serialize_stream`
//! (255-298행)·`compress`(112-124행)의 구조(64 KiB 단위 분할, ArchiveInfo/MessageInfo 필드 번호).
//! 압축은 가져오지 않고 리터럴 전용 블록을 직접 작성함.

use super::wire::{encode, msg, packed, varint, write_varint};
use crate::EasyOfficeError;

/// Largest decompressed Snappy block of an `.iwa` chunk.
pub(super) const BLOCK_BYTES: usize = 64 * 1024;

/// `MessageInfo.version` of every object the writer produces.
const MESSAGE_VERSION: [u64; 3] = [1, 0, 5];

/// One object as it goes into a stream.
pub(super) struct Record {
    pub id: u64,
    pub type_id: u32,
    pub payload: Vec<u8>,
    /// The objects the payload owns (`MessageInfo.object_references`), ascending.
    pub owned: Vec<u64>,
}

/// The record stream of `records`.
pub(super) fn stream<'a>(records: impl IntoIterator<Item = &'a Record>) -> Vec<u8> {
    let mut out = Vec::new();
    for record in records {
        let mut info = vec![
            varint(1, u64::from(record.type_id)),
            packed(2, &MESSAGE_VERSION),
            varint(3, record.payload.len() as u64),
        ];
        if !record.owned.is_empty() {
            info.push(packed(5, &record.owned));
        }
        let header = encode(&[varint(1, record.id), msg(2, info)]);
        write_varint(&mut out, header.len() as u64);
        out.extend_from_slice(&header);
        out.extend_from_slice(&record.payload);
    }
    out
}

/// A raw Snappy block holding `data` (at most [`BLOCK_BYTES`]) as one literal element.
fn literal_block(data: &[u8]) -> Vec<u8> {
    debug_assert!(!data.is_empty() && data.len() <= BLOCK_BYTES);
    let mut out = Vec::with_capacity(data.len() + 8);
    write_varint(&mut out, data.len() as u64);
    let n = data.len() - 1;
    if n < 60 {
        out.push((n as u8) << 2);
    } else if n < 1 << 8 {
        out.push(60 << 2);
        out.push(n as u8);
    } else {
        out.push(61 << 2);
        out.extend_from_slice(&(n as u16).to_le_bytes());
    }
    out.extend_from_slice(data);
    out
}

/// The `.iwa` file of a record stream: one chunk per 64 KiB of stream.
pub(super) fn chunks(stream: &[u8]) -> Result<Vec<u8>, EasyOfficeError> {
    let mut out = Vec::with_capacity(stream.len() + stream.len() / BLOCK_BYTES * 12 + 16);
    for piece in stream.chunks(BLOCK_BYTES) {
        let block = literal_block(piece);
        let len = u32::try_from(block.len())
            .ok()
            .filter(|len| *len < 1 << 24)
            .ok_or_else(|| {
                EasyOfficeError::CorruptedContainer("Pages 쓰기: 청크 길이 초과".into())
            })?;
        out.push(0);
        out.extend_from_slice(&len.to_le_bytes()[..3]);
        out.extend_from_slice(&block);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages_iwa::{decompress_iwa, snappy_decompress};

    #[test]
    fn literal_lengths_cover_the_three_tag_forms() {
        for len in [1usize, 60, 61, 255, 256, 257, BLOCK_BYTES] {
            let data: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
            assert_eq!(
                snappy_decompress(&literal_block(&data), BLOCK_BYTES).unwrap(),
                data,
                "{len}"
            );
        }
    }

    #[test]
    fn chunks_split_at_64_kib_and_decompress_to_the_stream() {
        let data: Vec<u8> = (0..BLOCK_BYTES * 2 + 5)
            .map(|i| (i * 7 % 256) as u8)
            .collect();
        let file = chunks(&data).unwrap();
        let mut budget = usize::MAX;
        assert_eq!(decompress_iwa(&file, &mut budget).unwrap(), data);
        assert_eq!(file[0], 0);
    }
}

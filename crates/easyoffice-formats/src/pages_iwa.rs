//! IWA container decoding for the Pages reader: chunk framing, a self-written Snappy-variant
//! block decompressor, and record splitting.
//!
//! Framing: a sequence of chunks, each a 4-byte header (byte 0 = chunk type, always 0; bytes 1..4 =
//! 24-bit little-endian payload length) followed by a raw Snappy block. There is no stream
//! identifier and no CRC. The concatenated decompressed chunks form a record stream: a varint
//! `ArchiveInfo` length, the `ArchiveInfo`, then one payload per `MessageInfo.length`.
//!
//! No `unsafe`; every index computation is checked and every size is capped.

use crate::pages_proto::{
    parse_archive_header, parse_archive_info, read_varint, ArchiveInfo, ArchiveObject, ObjectTable,
};
use crate::EasyOfficeError;

/// Maximum decompressed bytes of a single chunk.
pub const MAX_CHUNK_BYTES: usize = 1024 * 1024;
/// Maximum decompressed bytes of one `.iwa` file.
pub const MAX_IWA_FILE_BYTES: usize = 64 * 1024 * 1024;
/// Maximum decompressed bytes across all `.iwa` files of one document.
pub const MAX_DOCUMENT_BYTES: usize = 256 * 1024 * 1024;
/// Maximum records read from one `.iwa` file.
pub const MAX_RECORDS: usize = 1_000_000;

fn bad(detail: &str) -> EasyOfficeError {
    EasyOfficeError::CorruptedContainer(format!("Pages IWA: {detail}"))
}

fn budget(detail: &str) -> EasyOfficeError {
    EasyOfficeError::ResourceBudgetExceeded(format!("Pages IWA {detail}"))
}

/// Little-endian unsigned value of up to `size_of::<usize>()` bytes.
fn read_le_usize(raw: &[u8]) -> usize {
    match raw.len() {
        1 => usize::from(raw[0]),
        2 => usize::from(u16::from_le_bytes([raw[0], raw[1]])),
        3 => usize::from(u16::from_le_bytes([raw[0], raw[1]])) | (usize::from(raw[2]) << 16),
        4 => u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize,
        _ => raw
            .iter()
            .enumerate()
            .fold(0usize, |acc, (i, b)| acc | (usize::from(*b) << (8 * i))),
    }
}

/// Takes `count` bytes at `*pos`, advancing it; errors with `what` when truncated.
fn take_bytes<'a>(
    input: &'a [u8],
    pos: &mut usize,
    count: usize,
    what: &str,
) -> Result<&'a [u8], EasyOfficeError> {
    let end = pos
        .checked_add(count)
        .ok_or_else(|| bad("offset overflow"))?;
    let raw = input.get(*pos..end).ok_or_else(|| bad(what))?;
    *pos = end;
    Ok(raw)
}

/// Fails when appending `len` bytes would exceed `target_len`.
fn ensure_within_declared(
    out: &[u8],
    len: usize,
    target_len: usize,
) -> Result<(), EasyOfficeError> {
    if out.len().checked_add(len).is_none_or(|n| n > target_len) {
        return Err(bad("snappy output exceeds declared length"));
    }
    Ok(())
}

/// Copies a literal run introduced by `tag` (tag type 0).
fn snappy_literal(
    input: &[u8],
    pos: &mut usize,
    tag: u8,
    out: &mut Vec<u8>,
    target_len: usize,
) -> Result<(), EasyOfficeError> {
    let mut len = usize::from(tag >> 2);
    if len >= 60 {
        let raw = take_bytes(input, pos, len - 59, "literal length truncated")?;
        len = read_le_usize(raw);
    }
    len = len
        .checked_add(1)
        .ok_or_else(|| bad("literal length overflow"))?;
    let lit = take_bytes(input, pos, len, "literal truncated")?;
    ensure_within_declared(out, len, target_len)?;
    out.extend_from_slice(lit);
    Ok(())
}

/// Copies from earlier output within the current block (`base_len..out.len()`).
fn snappy_copy(
    input: &[u8],
    pos: &mut usize,
    tag: u8,
    out: &mut Vec<u8>,
    base_len: usize,
    target_len: usize,
) -> Result<(), EasyOfficeError> {
    let kind = tag & 3;
    let (len, offset_bytes) = match kind {
        1 => (4 + usize::from((tag >> 2) & 7), 1usize),
        2 => (1 + usize::from(tag >> 2), 2),
        _ => (1 + usize::from(tag >> 2), 4),
    };
    let raw = take_bytes(input, pos, offset_bytes, "copy offset truncated")?;
    let mut offset = read_le_usize(raw);
    if kind == 1 {
        offset |= usize::from(tag >> 5) << 8;
    }
    let block_len = out.len() - base_len;
    if offset == 0 || offset > block_len {
        return Err(bad("invalid copy offset"));
    }
    ensure_within_declared(out, len, target_len)?;
    if offset >= len {
        let start = out.len() - offset;
        out.extend_from_within(start..start + len);
    } else if offset == 1 {
        let byte = out[out.len() - 1];
        out.resize(out.len() + len, byte);
    } else {
        let base = out.len() - offset;
        let mut remaining = len;
        let mut avail = offset;
        while remaining > 0 {
            let take = remaining.min(avail);
            out.extend_from_within(base..base + take);
            remaining -= take;
            avail += take;
        }
    }
    Ok(())
}

/// Appends one decompressed raw Snappy block directly to `out`, returning the block's byte length.
pub fn snappy_decompress_into(
    input: &[u8],
    max_len: usize,
    out: &mut Vec<u8>,
) -> Result<usize, EasyOfficeError> {
    let mut pos = 0usize;
    let declared = usize::try_from(read_varint(input, &mut pos)?)
        .map_err(|_| bad("snappy length too large"))?;
    if declared > max_len {
        return Err(budget("snappy 선언 길이 한도 초과"));
    }
    let base_len = out.len();
    let target_len = base_len
        .checked_add(declared)
        .ok_or_else(|| bad("size overflow"))?;
    out.reserve(declared);
    while pos < input.len() {
        let tag = input[pos];
        pos += 1;
        if tag & 3 == 0 {
            snappy_literal(input, &mut pos, tag, out, target_len)?;
        } else {
            snappy_copy(input, &mut pos, tag, out, base_len, target_len)?;
        }
    }
    if out.len() != target_len {
        return Err(bad("snappy output shorter than declared length"));
    }
    Ok(declared)
}

/// Decompresses one raw Snappy block whose declared length may not exceed `max_len`.
pub fn snappy_decompress(input: &[u8], max_len: usize) -> Result<Vec<u8>, EasyOfficeError> {
    let mut out = Vec::new();
    snappy_decompress_into(input, max_len, &mut out)?;
    Ok(out)
}

/// Decompresses a whole `.iwa` file directly into `out` (clearing any previous bytes).
pub fn decompress_iwa_into(
    data: &[u8],
    document_budget: &mut usize,
    out: &mut Vec<u8>,
) -> Result<(), EasyOfficeError> {
    out.clear();
    let mut pos = 0usize;
    while pos < data.len() {
        let header = data
            .get(pos..pos.checked_add(4).ok_or_else(|| bad("offset overflow"))?)
            .ok_or_else(|| bad("chunk header truncated"))?;
        if header[0] != 0 {
            return Err(bad("unknown chunk type"));
        }
        let len =
            usize::from(header[1]) | usize::from(header[2]) << 8 | usize::from(header[3]) << 16;
        pos += 4;
        let end = pos.checked_add(len).ok_or_else(|| bad("offset overflow"))?;
        let body = data.get(pos..end).ok_or_else(|| bad("chunk truncated"))?;
        pos = end;
        let chunk_len = snappy_decompress_into(body, MAX_CHUNK_BYTES, out)?;
        if out.len() > MAX_IWA_FILE_BYTES {
            return Err(budget(".iwa 파일 크기 한도 초과"));
        }
        if chunk_len > *document_budget {
            return Err(budget("문서 전체 크기 한도 초과"));
        }
        *document_budget -= chunk_len;
    }
    Ok(())
}

/// Decompresses a whole `.iwa` file into its record stream. `document_budget` is the remaining
/// per-document decompressed byte allowance and is reduced by the produced size.
pub fn decompress_iwa(
    data: &[u8],
    document_budget: &mut usize,
) -> Result<Vec<u8>, EasyOfficeError> {
    let mut out = Vec::new();
    decompress_iwa_into(data, document_budget, &mut out)?;
    Ok(out)
}

/// One IWA record: header plus one payload per `MessageInfo`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IwaRecord {
    pub info: ArchiveInfo,
    pub payloads: Vec<Vec<u8>>,
}

/// Splits a decompressed record stream into records.
pub fn split_records(stream: &[u8]) -> Result<Vec<IwaRecord>, EasyOfficeError> {
    let mut records = Vec::new();
    let mut pos = 0usize;
    while pos < stream.len() {
        if records.len() >= MAX_RECORDS {
            return Err(budget("레코드 수 한도 초과"));
        }
        let info_len = usize::try_from(read_varint(stream, &mut pos)?)
            .map_err(|_| bad("ArchiveInfo length too large"))?;
        let end = pos
            .checked_add(info_len)
            .ok_or_else(|| bad("offset overflow"))?;
        let info = parse_archive_info(
            stream
                .get(pos..end)
                .ok_or_else(|| bad("ArchiveInfo truncated"))?,
        )?;
        pos = end;
        let mut payloads = Vec::with_capacity(info.messages.len());
        for m in &info.messages {
            let len = usize::try_from(m.length).map_err(|_| bad("payload length too large"))?;
            let end = pos.checked_add(len).ok_or_else(|| bad("offset overflow"))?;
            payloads.push(
                stream
                    .get(pos..end)
                    .ok_or_else(|| bad("record payload truncated"))?
                    .to_vec(),
            );
            pos = end;
        }
        records.push(IwaRecord { info, payloads });
    }
    Ok(records)
}

/// Adds records to an object table. A repeated identifier replaces the earlier object.
pub fn add_to_table(table: &mut ObjectTable, records: Vec<IwaRecord>) {
    for rec in records {
        let mut msgs = rec.info.messages.iter().zip(rec.payloads);
        let Some((first, payload)) = msgs.next() else {
            continue;
        };
        let extra = msgs.map(|(m, p)| (m.type_id, p)).collect();
        table.objects.insert(
            rec.info.identifier,
            ArchiveObject {
                type_id: first.type_id,
                payload,
                extra,
            },
        );
    }
}

/// Decodes a decompressed `.iwa` record stream directly into `table` without intermediate record or header `Vec` allocations.
pub fn decode_stream_into_table(
    stream: &[u8],
    table: &mut ObjectTable,
) -> Result<(), EasyOfficeError> {
    let mut pos = 0usize;
    let mut count = 0usize;
    while pos < stream.len() {
        if count >= MAX_RECORDS {
            return Err(budget("레코드 수 한도 초과"));
        }
        count += 1;
        let info_len = usize::try_from(read_varint(stream, &mut pos)?)
            .map_err(|_| bad("ArchiveInfo length too large"))?;
        let info_end = pos
            .checked_add(info_len)
            .ok_or_else(|| bad("offset overflow"))?;
        let info_slice = stream
            .get(pos..info_end)
            .ok_or_else(|| bad("ArchiveInfo truncated"))?;
        let header = parse_archive_header(info_slice)?;
        pos = info_end;
        if header.count == 0 {
            continue;
        }
        let (first_type, first_len) = header.infos[0];
        let first_end = pos
            .checked_add(first_len)
            .ok_or_else(|| bad("offset overflow"))?;
        let first_payload = stream
            .get(pos..first_end)
            .ok_or_else(|| bad("record payload truncated"))?
            .to_vec();
        pos = first_end;
        let mut extra = Vec::with_capacity(header.count.saturating_sub(1));
        for &(type_id, len) in &header.infos[1..header.count] {
            let end = pos.checked_add(len).ok_or_else(|| bad("offset overflow"))?;
            let payload = stream
                .get(pos..end)
                .ok_or_else(|| bad("record payload truncated"))?
                .to_vec();
            pos = end;
            extra.push((type_id, payload));
        }
        table.objects.insert(
            header.identifier,
            ArchiveObject {
                type_id: first_type,
                payload: first_payload,
                extra,
            },
        );
    }
    Ok(())
}

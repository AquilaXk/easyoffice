//! What a PDF declares, counted against the budgets before any page is copied (PD-161).
//!
//! `inspect` is the gate every PDF passes on its way into an edit: the size, the header, an
//! encryption marker, what the compressed streams inflate to, then (after the reader has parsed
//! the file) the object count, the page count, the depth of the page tree and the decoded size of
//! the page contents. Past any limit the file is refused with a fixed sentence (fail closed); the
//! reader's own reasons go to the log only.
//!
//! The content check is exact: `hayro-write` decodes each page's content into memory, so the size
//! is counted here first with a streaming inflater (`inflated_len`) that allocates nothing. The
//! stream scan before parsing is a best effort for everything else the reader decodes on its own
//! (object streams): it finds `stream ... endstream` spans by their keywords, which a crafted file
//! can hide from. It stops the common bomb, not a determined one (the remaining risk is in PD-161).

use crate::pdf_edit_budget::{
    Deadline, MAX_OBJECTS, MAX_PAGES, MAX_PAGE_CONTENT_BYTES, MAX_PAGE_TREE_DEPTH,
    MAX_PAGE_TREE_NODES, MAX_SOURCE_BYTES, MAX_STREAM_INFLATED_BYTES, MAX_TOTAL_CONTENT_BYTES,
    MAX_TOTAL_INFLATED_BYTES,
};
use flate2::{Decompress, FlushDecompress, Status};
use hayro_syntax::object::dict::keys::{CONTENTS, DECODE_PARMS, DP, KIDS, PAGES, PREDICTOR};
use hayro_syntax::object::{Array, Dict, Stream};
use hayro_syntax::page::Page;
use hayro_syntax::{Filter, LoadPdfError, Pdf};
use std::collections::HashSet;
use std::sync::Arc;

pub const MSG_ENCRYPTED: &str =
    "암호화된 PDF는 편집할 수 없습니다. 암호를 해제한 사본을 만든 뒤 다시 열어 주세요.";
pub const MSG_UNREADABLE: &str = "PDF 파일이 아니거나 읽을 수 없는 파일입니다.";
pub const MSG_CONTENT_FILTER: &str =
    "지원하지 않는 방식으로 압축된 쪽 내용이 있어 이 PDF는 편집할 수 없습니다.";
pub const MSG_CONTENT_BROKEN: &str =
    "쪽 내용을 읽을 수 없어(손상된 압축) 이 PDF는 편집할 수 없습니다.";
const MSG_EMPTY: &str = "쪽이 없는 PDF는 편집할 수 없습니다.";

/// How far into the file the `%PDF-` header may sit (the specification allows some leading bytes).
const HEADER_WINDOW: usize = 1024;
/// The encryption entry of the trailer. A file that has it is refused even when it opens with an
/// empty password, because the result would silently lose the protection it was given.
const ENCRYPT_KEY: &[u8] = b"/Encrypt";
/// Bytes before `stream` that are searched for the stream's filter.
const DICT_LOOKBACK: usize = 1024;
/// How often the checks look at the deadline, as masks (a power of two minus one): every 4096th
/// keyword span of the stream scan, every 1024th page tree node, every 64th page.
const SPANS_PER_CHECK_MASK: usize = 4095;
const NODES_PER_CHECK_MASK: usize = 1023;
const PAGES_PER_CHECK_MASK: usize = 63;
/// Output buffer of the counting inflater (its contents are thrown away).
const SCRATCH_BYTES: usize = 32 * 1024;

/// The numbers a file is counted against. [`Limits::DEFAULT`] is what the shell uses (the constants
/// of `pdf_edit_budget.rs`); tests pass small ones so a bomb does not have to be a real one.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub source_bytes: usize,
    pub objects: usize,
    pub pages: usize,
    pub tree_depth: usize,
    pub tree_nodes: usize,
    pub page_content: usize,
    pub total_content: usize,
    pub stream_inflated: u64,
    pub total_inflated: u64,
}

impl Limits {
    pub const DEFAULT: Self = Self {
        source_bytes: MAX_SOURCE_BYTES,
        objects: MAX_OBJECTS,
        pages: MAX_PAGES,
        tree_depth: MAX_PAGE_TREE_DEPTH,
        tree_nodes: MAX_PAGE_TREE_NODES,
        page_content: MAX_PAGE_CONTENT_BYTES,
        total_content: MAX_TOTAL_CONTENT_BYTES,
        stream_inflated: MAX_STREAM_INFLATED_BYTES,
        total_inflated: MAX_TOTAL_INFLATED_BYTES,
    };
}

/// What a file that passed the gate is.
#[derive(Debug, PartialEq, Eq)]
pub struct SourceInfo {
    pub pages: usize,
}

/// Parses `bytes` as an unencrypted PDF. The reader's own failure reasons go to the log.
pub fn open_pdf(bytes: &Arc<Vec<u8>>) -> Result<Pdf, String> {
    Pdf::new(Arc::clone(bytes)).map_err(|err| match err {
        LoadPdfError::Decryption(_) => MSG_ENCRYPTED.to_string(),
        LoadPdfError::Invalid => MSG_UNREADABLE.to_string(),
    })
}

/// Runs every check against the shell's limits; see the module comment for the order.
pub fn inspect(bytes: &Arc<Vec<u8>>, deadline: &Deadline) -> Result<SourceInfo, String> {
    inspect_within(bytes, deadline, &Limits::DEFAULT)
}

pub fn inspect_within(
    bytes: &Arc<Vec<u8>>,
    deadline: &Deadline,
    limits: &Limits,
) -> Result<SourceInfo, String> {
    if bytes.is_empty() || !has_header(bytes) {
        return Err(MSG_UNREADABLE.to_string());
    }
    if bytes.len() > limits.source_bytes {
        return Err(format!(
            "PDF가 허용 크기({}MiB)를 초과합니다.",
            limits.source_bytes / 1_048_576
        ));
    }
    if find(bytes, ENCRYPT_KEY, 0).is_some() {
        return Err(MSG_ENCRYPTED.to_string());
    }
    scan_streams(bytes, deadline, limits)?;
    deadline.check()?;
    let pdf = open_pdf(bytes)?;
    deadline.check()?;
    if pdf.len() > limits.objects {
        return Err(format!(
            "PDF의 개체 수가 한도({})를 초과합니다.",
            limits.objects
        ));
    }
    let pages = pdf.pages();
    if pages.is_empty() {
        return Err(MSG_EMPTY.to_string());
    }
    if pages.len() > limits.pages {
        return Err(format!(
            "PDF의 쪽 수가 한도({})를 초과합니다.",
            limits.pages
        ));
    }
    check_page_tree(&pdf, deadline, limits)?;
    let mut total = 0usize;
    for (n, page) in pages.iter().enumerate() {
        if n & PAGES_PER_CHECK_MASK == 0 {
            deadline.check()?;
        }
        total = total.saturating_add(page_content_bytes(page, limits)?);
        if total > limits.total_content {
            return Err(too_much_content());
        }
    }
    Ok(SourceInfo { pages: pages.len() })
}

fn too_much_content() -> String {
    "PDF 쪽 내용의 압축 해제 크기가 한도를 초과합니다.".to_string()
}

/// The decoded size of a page's content (all of its content streams), counted without decoding it
/// into memory. Only uncompressed and plain Flate streams are accepted: they are the ones that
/// can be counted exactly, and nearly every file uses nothing else for page contents.
pub fn page_content_bytes(page: &Page<'_>, limits: &Limits) -> Result<usize, String> {
    let raw = page.raw();
    let mut total = 0usize;
    if let Some(stream) = raw.get::<Stream<'_>>(CONTENTS) {
        total = stream_content_bytes(&stream, limits.page_content)?;
    } else if let Some(streams) = raw.get::<Array<'_>>(CONTENTS) {
        for stream in streams.iter::<Stream<'_>>() {
            // The reader puts one separator after each stream.
            total = total.saturating_add(
                stream_content_bytes(&stream, limits.page_content)?.saturating_add(1),
            );
            if total > limits.page_content {
                break;
            }
        }
    }
    if total > limits.page_content {
        return Err(too_much_content());
    }
    Ok(total)
}

fn stream_content_bytes(stream: &Stream<'_>, cap: usize) -> Result<usize, String> {
    let data = stream.raw_data();
    match stream.filters().as_slice() {
        [] => Ok(data.len()),
        [Filter::FlateDecode] if !has_predictor(stream.dict()) => {
            let n = inflated_len(&data, cap as u64).map_err(|fail| match fail {
                Inflate::TooLarge => too_much_content(),
                Inflate::Invalid => MSG_CONTENT_BROKEN.to_string(),
            })?;
            usize::try_from(n).map_err(|_| too_much_content())
        }
        _ => Err(MSG_CONTENT_FILTER.to_string()),
    }
}

/// Whether the stream's `DecodeParms` ask for a predictor (which rewrites the inflated bytes).
fn has_predictor(dict: &Dict<'_>) -> bool {
    let predicts = |params: &Dict<'_>| params.get::<i32>(PREDICTOR).is_some_and(|p| p > 1);
    [DECODE_PARMS, DP].into_iter().any(|key| {
        dict.get::<Dict<'_>>(key).is_some_and(|d| predicts(&d))
            || dict
                .get::<Array<'_>>(key)
                .is_some_and(|a| a.iter::<Dict<'_>>().any(|d| predicts(&d)))
    })
}

/// Follows `/Pages` and `/Kids` with an explicit stack: the depth and the node count are limited and
/// a node met twice (a cycle) is not followed again.
fn check_page_tree(pdf: &Pdf, deadline: &Deadline, limits: &Limits) -> Result<(), String> {
    let xref = pdf.xref();
    let Some(root) = xref.get::<Dict<'_>>(xref.root_id()) else {
        // A repaired file without a catalog: the reader found the pages by scanning, so there is
        // no tree to measure.
        return Ok(());
    };
    let Some(top) = root.get::<Dict<'_>>(PAGES) else {
        return Ok(());
    };
    let mut seen = HashSet::new();
    let mut stack = vec![(top, 1usize)];
    let mut nodes = 0usize;
    while let Some((node, depth)) = stack.pop() {
        if depth > limits.tree_depth {
            return Err(format!(
                "PDF의 쪽 트리가 너무 깊습니다(한도 {}단계).",
                limits.tree_depth
            ));
        }
        nodes += 1;
        if nodes > limits.tree_nodes {
            return Err(format!(
                "PDF의 쪽 트리 노드 수가 한도({})를 초과합니다.",
                limits.tree_nodes
            ));
        }
        if nodes & NODES_PER_CHECK_MASK == 0 {
            deadline.check()?;
        }
        if node.obj_id().is_some_and(|id| !seen.insert(id)) {
            continue;
        }
        if let Some(kids) = node.get::<Array<'_>>(KIDS) {
            stack.extend(kids.iter::<Dict<'_>>().map(|kid| (kid, depth + 1)));
        }
    }
    Ok(())
}

/// Why a stream could not be counted.
#[derive(Debug, PartialEq, Eq)]
pub enum Inflate {
    /// It inflates past the cap.
    TooLarge,
    /// It is not a complete Flate stream.
    Invalid,
}

/// Inflates `data` (zlib, or raw deflate when it has no zlib header) into a scratch buffer and
/// returns how many bytes it makes, stopping with [`Inflate::TooLarge`] once that passes `cap`.
pub fn inflated_len(data: &[u8], cap: u64) -> Result<u64, Inflate> {
    let zlib = data.len() >= 2
        && data[0] & 0x0f == 8
        && ((u16::from(data[0]) << 8) | u16::from(data[1])) % 31 == 0;
    let mut inflater = Decompress::new(zlib);
    let mut scratch = [0u8; SCRATCH_BYTES];
    loop {
        let (read, written) = (inflater.total_in(), inflater.total_out());
        let input = data.get(usize::try_from(read).map_err(|_| Inflate::Invalid)?..);
        let status = inflater
            .decompress(input.unwrap_or(&[]), &mut scratch, FlushDecompress::None)
            .map_err(|_| Inflate::Invalid)?;
        if inflater.total_out() > cap {
            return Err(Inflate::TooLarge);
        }
        match status {
            Status::StreamEnd => return Ok(inflater.total_out()),
            Status::Ok | Status::BufError => {
                if inflater.total_in() == read && inflater.total_out() == written {
                    return Err(Inflate::Invalid);
                }
            }
        }
    }
}

/// Best-effort bomb check over the whole file (see the module comment): every Flate stream found
/// by its keywords must inflate to at most `limits.stream_inflated`, and all of them together to
/// at most `limits.total_inflated`. A span that is not a complete Flate stream is left to
/// the reader.
fn scan_streams(bytes: &[u8], deadline: &Deadline, limits: &Limits) -> Result<(), String> {
    let too_big = || "PDF의 압축 해제 크기가 한도를 초과합니다.".to_string();
    let mut total = 0u64;
    let mut at = 0usize;
    let mut spans = 0usize;
    while let Some(found) = find(bytes, b"stream", at) {
        at = found + b"stream".len();
        spans += 1;
        if spans & SPANS_PER_CHECK_MASK == 0 {
            deadline.check()?;
        }
        if found >= 3 && &bytes[found - 3..found] == b"end" {
            continue;
        }
        let Some(start) = stream_data_start(bytes, at) else {
            continue;
        };
        let end = find(bytes, b"endstream", start).unwrap_or(bytes.len());
        let lookback = &bytes[found.saturating_sub(DICT_LOOKBACK)..found];
        if find(lookback, b"FlateDecode", 0).is_some() {
            deadline.check()?;
            match inflated_len(&bytes[start..end], limits.stream_inflated) {
                Ok(n) => total += n,
                Err(Inflate::TooLarge) => return Err(too_big()),
                Err(Inflate::Invalid) => {}
            }
            if total > limits.total_inflated {
                return Err(too_big());
            }
        }
        at = end;
    }
    Ok(())
}

/// Where the data of a stream starts: after the line break that follows the `stream` keyword.
fn stream_data_start(bytes: &[u8], after_keyword: usize) -> Option<usize> {
    match bytes.get(after_keyword)? {
        b'\n' => Some(after_keyword + 1),
        b'\r' if bytes.get(after_keyword + 1) == Some(&b'\n') => Some(after_keyword + 2),
        b'\r' => Some(after_keyword + 1),
        _ => None,
    }
}

fn has_header(bytes: &[u8]) -> bool {
    find(&bytes[..bytes.len().min(HEADER_WINDOW)], b"%PDF-", 0).is_some()
}

/// The first position of `needle` in `hay` at or after `from`.
fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    let tail = hay.get(from..)?;
    let first = *needle.first()?;
    let mut at = 0;
    while let Some(i) = tail[at..].iter().position(|&b| b == first) {
        let pos = at + i;
        if tail[pos..].starts_with(needle) {
            return Some(from + pos);
        }
        at = pos + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_locates_needles_after_an_offset() {
        assert_eq!(find(b"abcabc", b"bc", 0), Some(1));
        assert_eq!(find(b"abcabc", b"bc", 2), Some(4));
        assert_eq!(find(b"abc", b"bd", 0), None);
        assert_eq!(find(b"abc", b"abc", 4), None);
        assert_eq!(find(b"", b"a", 0), None);
    }

    #[test]
    fn stream_data_starts_after_the_line_break() {
        assert_eq!(stream_data_start(b"stream\nX", 6), Some(7));
        assert_eq!(stream_data_start(b"stream\r\nX", 6), Some(8));
        assert_eq!(stream_data_start(b"stream\rX", 6), Some(7));
        assert_eq!(stream_data_start(b"streamX", 6), None);
        assert_eq!(stream_data_start(b"stream", 6), None);
    }
}

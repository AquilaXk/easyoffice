//! What a PDF declares, counted against the budgets before any page is copied (PD-170).
//!
//! `inspect` is the gate every PDF passes on its way into an edit: the size, the header, what the
//! compressed streams inflate to (`pdf_edit_scan.rs`), then, after the reader has parsed the
//! file, whether it is encrypted, the object count, the page count, the depth of the page tree,
//! the entries of the resource dictionaries and the decoded size of the page contents. Past any
//! limit the file is refused with a fixed sentence (fail closed); the reader's own reasons go to
//! the log only.
//!
//! All of it runs in the worker process (`pdf_edit_worker.rs`). The counts here cannot cover
//! everything the reader may do with a crafted file; what is left is contained by the worker's
//! memory and time limits, which end the process.
//!
//! The content check is exact: `hayro-write` decodes each page's content into memory, so the size
//! is counted here first with a streaming inflater that allocates nothing.

use crate::pdf_edit_budget::{
    Deadline, MAX_CONTENT_STREAMS, MAX_OBJECTS, MAX_PAGES, MAX_PAGE_CONTENT_BYTES,
    MAX_PAGE_TREE_DEPTH, MAX_PAGE_TREE_NODES, MAX_RESOURCE_ENTRIES, MAX_SOURCE_BYTES,
    MAX_STREAM_INFLATED_BYTES, MAX_TOTAL_CONTENT_BYTES, MAX_TOTAL_INFLATED_BYTES,
};
use crate::pdf_edit_scan::{find, inflated_len, scan_streams, Inflate};
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
const MSG_TOO_MANY_STREAMS: &str = "쪽 하나의 내용 스트림 수가 한도를 초과합니다.";
const MSG_TOO_MANY_RESOURCES: &str = "쪽 리소스(글꼴·그림 등) 항목 수가 한도를 초과합니다.";

/// How far into the file the `%PDF-` header may sit (the specification allows some leading bytes).
const HEADER_WINDOW: usize = 1024;
/// A password nobody has. An encrypted file does not open with it, an unencrypted one ignores it:
/// that is how the reader's own reading of the trailer tells whether a file is encrypted. A file
/// that opens with the empty password is refused all the same, because the result would silently
/// lose the protection it was given.
const PROBE_PASSWORD: &str = "\u{1}easyoffice-encryption-probe\u{2}";
/// How often the checks look at the deadline, as masks (a power of two minus one): every 1024th
/// page tree node, every 64th page, every 1024th content stream.
const NODES_PER_CHECK_MASK: usize = 1023;
const PAGES_PER_CHECK_MASK: usize = 63;
const STREAMS_PER_CHECK_MASK: usize = 1023;

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
    pub resource_entries: usize,
    pub content_streams: usize,
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
        resource_entries: MAX_RESOURCE_ENTRIES,
        content_streams: MAX_CONTENT_STREAMS,
    };
}

/// What a file that passed the gate is.
#[derive(Debug, PartialEq, Eq)]
pub struct SourceInfo {
    pub pages: usize,
}

/// Parses `bytes` as an unencrypted PDF. The file is opened with a password nobody has: a file the
/// reader finds encrypted does not open (and is refused), an unencrypted one does, so the
/// answer comes from the reader's reading of the trailer and cannot be hidden by how the entry is
/// spelled. The reader's own failure reasons go to the log.
pub fn open_pdf(bytes: &Arc<Vec<u8>>) -> Result<Pdf, String> {
    Pdf::new_with_password(Arc::clone(bytes), PROBE_PASSWORD).map_err(|err| match err {
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
    scan_streams(
        bytes,
        deadline,
        limits.stream_inflated,
        limits.total_inflated,
    )?;
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
    let (mut total, mut entries) = (0usize, 0usize);
    for (n, page) in pages.iter().enumerate() {
        if n & PAGES_PER_CHECK_MASK == 0 {
            deadline.check()?;
        }
        entries = entries.saturating_add(resource_entries(
            page,
            limits.resource_entries - entries.min(limits.resource_entries),
        )?);
        total = total.saturating_add(page_content_bytes(page, limits, deadline)?);
        if total > limits.total_content {
            return Err(too_much_content());
        }
    }
    Ok(SourceInfo { pages: pages.len() })
}

/// How many entries the resource dictionaries of a page hold (fonts, pictures, colour spaces, ...).
/// The library writes every entry again for every page it copies, so a file whose pages share a
/// huge dictionary is as large as pages times entries. Counting stops past `room`.
pub fn resource_entries(page: &Page<'_>, room: usize) -> Result<usize, String> {
    let r = page.resources();
    let mut count = 0usize;
    for dict in [
        &r.ext_g_states,
        &r.shadings,
        &r.patterns,
        &r.x_objects,
        &r.color_spaces,
        &r.fonts,
        &r.properties,
    ] {
        count = count.saturating_add(dict.len());
        if count > room {
            return Err(MSG_TOO_MANY_RESOURCES.to_string());
        }
    }
    Ok(count)
}

fn too_much_content() -> String {
    "PDF 쪽 내용의 압축 해제 크기가 한도를 초과합니다.".to_string()
}

/// The decoded size of a page's content (all of its content streams), counted without decoding it
/// into memory. Only uncompressed and plain Flate streams are accepted: they are the ones that
/// can be counted exactly, and nearly every file uses nothing else for page contents.
pub fn page_content_bytes(
    page: &Page<'_>,
    limits: &Limits,
    deadline: &Deadline,
) -> Result<usize, String> {
    let raw = page.raw();
    let mut total = 0usize;
    if let Some(stream) = raw.get::<Stream<'_>>(CONTENTS) {
        total = stream_content_bytes(&stream, limits.page_content)?;
    } else if let Some(streams) = raw.get::<Array<'_>>(CONTENTS) {
        for (n, stream) in streams.iter::<Stream<'_>>().enumerate() {
            if n >= limits.content_streams {
                return Err(MSG_TOO_MANY_STREAMS.to_string());
            }
            if n & STREAMS_PER_CHECK_MASK == 0 {
                deadline.check()?;
            }
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

fn has_header(bytes: &[u8]) -> bool {
    find(&bytes[..bytes.len().min(HEADER_WINDOW)], b"%PDF-", 0).is_some()
}

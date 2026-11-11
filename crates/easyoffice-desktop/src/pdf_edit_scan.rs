//! A cheap look at the compressed streams of a file before the PDF reader parses it (PD-168).
//!
//! The reader decodes object streams and cross-reference streams on its own, without a limit, so
//! a small file can make it allocate a lot. The worker process is what finally contains that (it is
//! killed past a memory limit); this scan stops the plain cases early and with a clear sentence,
//! as a second line of defence. It does not parse the file. It finds the keyword `stream`, and
//! whatever follows is judged by what it is, not by what the dictionary claims:
//!
//! * data that starts like a zlib stream is inflated to its end, through the whole rest of the file
//!   (never cut at an `endstream`, which a crafted stream can contain) and counted;
//! * a dictionary whose `/Filter` says Flate has its data counted the same way, raw deflate too;
//! * a dictionary that names another filter that stacks on Flate (ASCII85, ASCIIHex, LZW,
//!   RunLength), spells a filter with a `#` escape or as an abbreviation, or gives `/Filter` as an
//!   indirect reference is refused: the data behind it cannot be counted here.

use crate::pdf_edit_budget::Deadline;
use flate2::{Decompress, FlushDecompress, Status};

pub const MSG_STREAM_TOO_BIG: &str = "PDF의 압축 해제 크기가 한도를 초과합니다.";
pub const MSG_STREAM_FILTER: &str =
    "지원하지 않는 방식으로 압축된 스트림이 있어 이 PDF는 편집할 수 없습니다.";
pub const MSG_STREAM_SHAPE: &str = "PDF의 스트림 구조가 비정상이어서 편집할 수 없습니다.";

/// Bytes before `stream` that are searched for the stream's dictionary.
const DICT_LOOKBACK: usize = 256 * 1024;
/// The dictionaries of all streams together may be read this many times the size of the file (plus
/// a constant), so a file of nothing but `stream` keywords cannot make the scan quadratic.
const WINDOW_BUDGET_FACTOR: usize = 8;
const WINDOW_BUDGET_EXTRA: usize = 16 * 1_048_576;
/// Output buffer of the counting inflater (its contents are thrown away).
const SCRATCH_BYTES: usize = 32 * 1024;
/// How often the scan looks at the deadline: every 4096th keyword, as a mask.
const SPANS_PER_CHECK_MASK: usize = 4095;

/// Why a stream could not be counted.
#[derive(Debug, PartialEq, Eq)]
pub enum Inflate {
    /// It inflates past the cap.
    TooLarge,
    /// It is not a complete Flate stream.
    Invalid,
}

fn looks_like_zlib(data: &[u8]) -> bool {
    data.len() >= 2
        && data[0] & 0x0f == 8
        && ((u16::from(data[0]) << 8) | u16::from(data[1])) % 31 == 0
}

/// Inflates `data` (zlib, or raw deflate when it has no zlib header) into a scratch buffer and
/// returns how many bytes it makes and how many bytes of `data` it used, stopping with
/// [`Inflate::TooLarge`] once the output passes `cap`.
pub fn inflate_span(data: &[u8], cap: u64) -> Result<(u64, usize), Inflate> {
    let mut inflater = Decompress::new(looks_like_zlib(data));
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
            Status::StreamEnd => {
                let used = usize::try_from(inflater.total_in()).map_err(|_| Inflate::Invalid)?;
                return Ok((inflater.total_out(), used));
            }
            Status::Ok | Status::BufError => {
                if inflater.total_in() == read && inflater.total_out() == written {
                    return Err(Inflate::Invalid);
                }
            }
        }
    }
}

/// What a stream inflates to (the data may run on past the end of the stream).
pub fn inflated_len(data: &[u8], cap: u64) -> Result<u64, Inflate> {
    inflate_span(data, cap).map(|(out, _)| out)
}

/// The scan: every stream that is Flate by its data or its dictionary must inflate to at most
/// `stream_cap`, and all of them together to at most `total_cap`.
pub fn scan_streams(
    bytes: &[u8],
    deadline: &Deadline,
    stream_cap: u64,
    total_cap: u64,
) -> Result<(), String> {
    let mut window_budget = bytes
        .len()
        .saturating_mul(WINDOW_BUDGET_FACTOR)
        .saturating_add(WINDOW_BUDGET_EXTRA);
    let (mut total, mut at, mut spans) = (0u64, 0usize, 0usize);
    while let Some(found) = find(bytes, b"stream", at) {
        at = found + b"stream".len();
        spans += 1;
        if spans & SPANS_PER_CHECK_MASK == 0 {
            deadline.check()?;
        }
        if found >= 3 && &bytes[found - 3..found] == b"end" {
            continue;
        }
        let start = stream_data_start(bytes, at);
        let window = dictionary_window(bytes, found);
        window_budget = window_budget
            .checked_sub(window.len())
            .ok_or_else(|| MSG_STREAM_SHAPE.to_string())?;
        let flate_by_dictionary = filters_of(window)?;
        let data = &bytes[start..];
        if !flate_by_dictionary && !looks_like_zlib(data) {
            at = start;
            continue;
        }
        deadline.check()?;
        match inflate_span(data, stream_cap) {
            Ok((out, used)) => {
                total = total.saturating_add(out);
                at = start + used.max(1);
            }
            Err(Inflate::TooLarge) => return Err(MSG_STREAM_TOO_BIG.to_string()),
            Err(Inflate::Invalid) => at = start,
        }
        if total > total_cap {
            return Err(MSG_STREAM_TOO_BIG.to_string());
        }
    }
    Ok(())
}

/// The dictionary text before a `stream` keyword: what follows the last `obj` before it.
fn dictionary_window(bytes: &[u8], stream_at: usize) -> &[u8] {
    let hay = &bytes[stream_at.saturating_sub(DICT_LOOKBACK)..stream_at];
    match hay.windows(3).rposition(|w| w == b"obj") {
        Some(i) => &hay[i + 3..],
        None => hay,
    }
}

const NAME_DELIMITERS: &[u8] = b"()<>[]{}/%";

fn is_name_end(b: u8) -> bool {
    b.is_ascii_whitespace() || b == 0 || NAME_DELIMITERS.contains(&b)
}

/// Reads the name that starts at `at` (the byte after its `/`): its raw bytes, the decoded name
/// (`#xx` escapes resolved) and the position after it.
fn read_name(window: &[u8], at: usize) -> (&[u8], Vec<u8>, usize) {
    let end = window[at..]
        .iter()
        .position(|&b| is_name_end(b))
        .map_or(window.len(), |n| at + n);
    let raw = &window[at..end];
    let mut decoded = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match (
            raw[i],
            raw.get(i + 1).copied().and_then(hex),
            raw.get(i + 2).copied().and_then(hex),
        ) {
            (b'#', Some(hi), Some(lo)) => {
                decoded.push((hi * 16 + lo) as u8);
                i += 3;
            }
            (b, _, _) => {
                decoded.push(b);
                i += 1;
            }
        }
    }
    (raw, decoded, end)
}

/// Judges the `/Filter` of a stream dictionary. `Ok(true)` when it names Flate (so the data is
/// counted whatever it looks like), `Ok(false)` when it names no filter or only filters that do not
/// inflate, and an error for the cases in the module comment.
fn filters_of(window: &[u8]) -> Result<bool, String> {
    let mut flate = false;
    let mut i = 0;
    while i < window.len() {
        if window[i] != b'/' {
            i += 1;
            continue;
        }
        let (_, key, mut j) = read_name(window, i + 1);
        i = j.max(i + 1);
        if key != b"Filter" {
            continue;
        }
        // The value: one name, or an array of names.
        while window.get(j).is_some_and(|b| b.is_ascii_whitespace()) {
            j += 1;
        }
        let array = window.get(j) == Some(&b'[');
        if array {
            j += 1;
        }
        loop {
            while window.get(j).is_some_and(|b| b.is_ascii_whitespace()) {
                j += 1;
            }
            match window.get(j) {
                Some(b'/') => {
                    let (raw, name, end) = read_name(window, j + 1);
                    flate |= judge_filter(raw, &name)?;
                    j = end;
                    if !array {
                        break;
                    }
                }
                Some(b) if b.is_ascii_digit() => return Err(MSG_STREAM_FILTER.to_string()),
                _ => break,
            }
        }
        i = i.max(j);
    }
    Ok(flate)
}

fn judge_filter(raw: &[u8], name: &[u8]) -> Result<bool, String> {
    match name {
        b"FlateDecode" if raw == name => Ok(true),
        b"FlateDecode" | b"Fl" | b"LZWDecode" | b"LZW" | b"ASCII85Decode" | b"A85"
        | b"ASCIIHexDecode" | b"AHx" | b"RunLengthDecode" | b"RL" | b"Crypt" => {
            Err(MSG_STREAM_FILTER.to_string())
        }
        _ if raw != name => Err(MSG_STREAM_FILTER.to_string()),
        _ => Ok(false),
    }
}

/// Where the data of a stream starts: after the line break that follows the `stream` keyword. The
/// reader is lenient about what sits between them (spaces, a missing line break), so this is too:
/// a stream is never skipped for being written oddly.
pub fn stream_data_start(bytes: &[u8], after_keyword: usize) -> usize {
    let mut at = after_keyword;
    while matches!(bytes.get(at), Some(b' ' | b'\t' | 0)) {
        at += 1;
    }
    match (bytes.get(at), bytes.get(at + 1)) {
        (Some(b'\r'), Some(b'\n')) => at + 2,
        (Some(b'\n' | b'\r'), _) => at + 1,
        _ => at,
    }
}

/// The first position of `needle` in `hay` at or after `from`.
pub fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
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
        assert_eq!(stream_data_start(b"stream\nX", 6), 7);
        assert_eq!(stream_data_start(b"stream\r\nX", 6), 8);
        assert_eq!(stream_data_start(b"stream\rX", 6), 7);
        assert_eq!(stream_data_start(b"stream  \r\nX", 6), 10);
        assert_eq!(stream_data_start(b"streamX", 6), 6);
        assert_eq!(stream_data_start(b"stream", 6), 6);
    }

    #[test]
    fn a_filter_is_judged_by_its_decoded_name_and_only_plain_flate_is_counted() {
        for (dict, expected) in [
            (b" << /Length 5 >> ".as_slice(), Ok(false)),
            (b" << /Filter /FlateDecode >> ", Ok(true)),
            (b" << /Filter [/FlateDecode] >> ", Ok(true)),
            (b" << /Filter /DCTDecode >> ", Ok(false)),
            (b" << /Filter /Fl#61teDecode >> ", Err(())),
            (b" << /Filter /Fl >> ", Err(())),
            (b" << /Filter [/ASCII85Decode /FlateDecode] >> ", Err(())),
            (b" << /Filter /LZWDecode >> ", Err(())),
            (b" << /Filter /RunLengthDecode >> ", Err(())),
            (b" << /Fil#74er /FlateDecode >> ", Ok(true)),
            (b" << /Filter 12 0 R >> ", Err(())),
            (b" << /Filter [5 0 R] >> ", Err(())),
            (b" << /Font#20Name /Fl >> ", Ok(false)),
            (b" << /Filter /Fl#6fo >> ", Err(())),
        ] {
            assert_eq!(
                filters_of(dict).map_err(|_| ()),
                expected,
                "{}",
                String::from_utf8_lossy(dict)
            );
        }
    }

    #[test]
    fn the_dictionary_is_what_follows_the_last_obj() {
        let bytes = b"1 0 obj << /Filter /LZWDecode >> endobj 2 0 obj << /Length 3 >> stream";
        let at = bytes.len() - b"stream".len();
        assert_eq!(dictionary_window(bytes, at), b" << /Length 3 >> ");
    }
}

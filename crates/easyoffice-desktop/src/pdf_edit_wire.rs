//! The byte protocol between the shell and its PDF worker process (PD-170).
//!
//! Everything that parses or rewrites a PDF that cannot be trusted runs in a child process of the
//! shell's own binary (`EasyOffice --pdf-edit-worker`, see `pdf_edit_worker.rs`). The child gets
//! bytes on its standard input and answers with bytes on its standard output: no window, no IPC,
//! no path, and it can be killed at any time. This file is the format of those bytes. It depends on
//! nothing but `std`, so the shell, the worker and the isolation tests share it.
//!
//! A frame is one tag byte, a little-endian `u32` length and that many payload bytes.
//!
//! * shell -> worker: [`REQ_INSPECT`] (the PDF), or [`REQ_BUILD`] (the plan, see [`BuildHead`])
//!   followed by one [`SOURCE`] frame per PDF; later [`OVERLAY_REPLY`] / [`OVERLAY_FAIL`].
//! * worker -> shell: [`NEED_OVERLAY`] (the stamp and watermark are made by the shell, which owns
//!   the fonts), then exactly one of [`RES_OK`] and [`RES_ERR`].

use std::io::{self, Read, Write};

/// The argument that turns the shell binary into a worker.
pub const WORKER_ARG: &str = "--pdf-edit-worker";

pub const REQ_INSPECT: u8 = 1;
pub const REQ_BUILD: u8 = 2;
pub const SOURCE: u8 = 3;
pub const OVERLAY_REPLY: u8 = 5;
pub const OVERLAY_FAIL: u8 = 6;
pub const RES_OK: u8 = 10;
pub const RES_ERR: u8 = 11;
pub const NEED_OVERLAY: u8 = 12;

/// Largest frame either side accepts (bytes): a source or a result is at most 256 MiB.
pub const MAX_FRAME_BYTES: usize = 288 * 1_048_576;

/// Exit codes of a worker that stopped itself (any other code or a signal is an abnormal end).
pub const EXIT_MEMORY: i32 = 3;
pub const EXIT_DEADLINE: i32 = 4;

/// One page of the result: page `index` of source `source`, turned `turn` degrees (0, 90, 180 or
/// 270) more than it already is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PagePlan {
    pub source: usize,
    pub index: usize,
    pub turn: u16,
}

/// Makes the PDF of an overlay for a page shown `width` x `height` points (with the stamp or not).
/// The shell makes them: the worker process has no fonts and no files.
pub type OverlayMaker<'a> = dyn FnMut(f32, f32, bool) -> Result<Vec<u8>, String> + 'a;

/// Writes one frame whose payload is `parts` one after the other (no copy of the parts).
pub fn write_frame<W: Write + ?Sized>(out: &mut W, tag: u8, parts: &[&[u8]]) -> io::Result<()> {
    let total: usize = parts.iter().map(|p| p.len()).sum();
    let len = u32::try_from(total)
        .ok()
        .filter(|_| total <= MAX_FRAME_BYTES)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "frame too large"))?;
    out.write_all(&[tag])?;
    out.write_all(&len.to_le_bytes())?;
    for part in parts {
        out.write_all(part)?;
    }
    out.flush()
}

/// Largest payload of each tag the shell accepts from a worker (0: the tag is not expected). A
/// worker that has been fed a crafted file is not trusted: it may ask for an overlay (nine
/// bytes), fail with a sentence, or answer; only the answer may be large.
pub fn from_worker_cap(tag: u8) -> usize {
    match tag {
        NEED_OVERLAY => 9,
        RES_ERR => 4096,
        RES_OK => MAX_FRAME_BYTES,
        _ => 0,
    }
}

/// Largest payload of each tag the worker accepts from the shell.
pub fn from_shell_cap(tag: u8) -> usize {
    match tag {
        REQ_INSPECT | SOURCE => MAX_FRAME_BYTES,
        // 5,000 pages of 10 bytes, two 100-character texts and the counts.
        REQ_BUILD => 128 * 1024,
        OVERLAY_REPLY => 16 * 1_048_576,
        OVERLAY_FAIL => 4096,
        _ => 0,
    }
}

/// Reads one frame whose payload may be at most `cap(tag)` bytes (checked on the header, before
/// anything is read). The payload buffer grows with what actually arrives, so a length that lies
/// costs nothing but the bytes that were sent.
pub fn read_frame<R: Read + ?Sized>(
    input: &mut R,
    cap: fn(u8) -> usize,
) -> io::Result<(u8, Vec<u8>)> {
    let mut head = [0u8; 5];
    input.read_exact(&mut head)?;
    let len = u32::from_le_bytes([head[1], head[2], head[3], head[4]]) as usize;
    let allowed = cap(head[0]).min(MAX_FRAME_BYTES);
    if allowed == 0 || len > allowed {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut payload = Vec::new();
    let got = input.take(len as u64).read_to_end(&mut payload)?;
    if got != len {
        return Err(io::ErrorKind::UnexpectedEof.into());
    }
    Ok((head[0], payload))
}

/// The plan of a build, sent before the sources.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildHead {
    pub sources: usize,
    pub pages: Vec<PagePlan>,
    pub watermark: Option<String>,
    pub stamp: Option<String>,
}

fn put_text(out: &mut Vec<u8>, text: &Option<String>) {
    match text {
        None => out.push(0),
        Some(text) => {
            out.push(1);
            out.extend_from_slice(&u32::try_from(text.len()).unwrap_or(0).to_le_bytes());
            out.extend_from_slice(text.as_bytes());
        }
    }
}

impl BuildHead {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(16 + self.pages.len() * 10);
        out.extend_from_slice(
            &u32::try_from(self.sources)
                .unwrap_or(u32::MAX)
                .to_le_bytes(),
        );
        out.extend_from_slice(
            &u32::try_from(self.pages.len())
                .unwrap_or(u32::MAX)
                .to_le_bytes(),
        );
        put_text(&mut out, &self.watermark);
        put_text(&mut out, &self.stamp);
        for page in &self.pages {
            out.extend_from_slice(&u32::try_from(page.source).unwrap_or(u32::MAX).to_le_bytes());
            out.extend_from_slice(&u32::try_from(page.index).unwrap_or(u32::MAX).to_le_bytes());
            out.extend_from_slice(&page.turn.to_le_bytes());
        }
        out
    }

    /// Reads a head; `max_sources` and `max_pages` are checked before anything is allocated for
    /// them. `None` when the bytes are not a head.
    pub fn decode(bytes: &[u8], max_sources: usize, max_pages: usize) -> Option<Self> {
        let mut at = Cursor { bytes, at: 0 };
        let sources = at.u32()? as usize;
        let count = at.u32()? as usize;
        if sources > max_sources || count > max_pages {
            return None;
        }
        let watermark = at.text()?;
        let stamp = at.text()?;
        let mut pages = Vec::with_capacity(count);
        for _ in 0..count {
            let (source, index) = (at.u32()? as usize, at.u32()? as usize);
            let turn = u16::from_le_bytes(at.take(2)?.try_into().ok()?);
            if source >= sources || turn % 90 != 0 || turn >= 360 {
                return None;
            }
            pages.push(PagePlan {
                source,
                index,
                turn,
            });
        }
        (at.at == bytes.len()).then_some(Self {
            sources,
            pages,
            watermark,
            stamp,
        })
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(n)?;
        let slice = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(slice)
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn text(&mut self) -> Option<Option<String>> {
        match self.take(1)?[0] {
            0 => Some(None),
            1 => {
                let len = self.u32()? as usize;
                String::from_utf8(self.take(len)?.to_vec()).ok().map(Some)
            }
            _ => None,
        }
    }
}

/// A request for the overlay of a page shown `width` x `height` points.
pub fn encode_overlay_need(width: f32, height: f32, stamp: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(9);
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.push(u8::from(stamp));
    out
}

pub fn decode_overlay_need(bytes: &[u8]) -> Option<(f32, f32, bool)> {
    let [a, b, c, d, e, f, g, h, s] = <[u8; 9]>::try_from(bytes).ok()?;
    let (w, h) = (
        f32::from_le_bytes([a, b, c, d]),
        f32::from_le_bytes([e, f, g, h]),
    );
    (w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 && s <= 1).then_some((w, h, s == 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip_and_a_lying_length_costs_only_what_was_sent() {
        let mut wire = Vec::new();
        write_frame(&mut wire, REQ_INSPECT, &[b"ab", b"cd"]).unwrap();
        write_frame(&mut wire, RES_OK, &[]).unwrap();
        let mut input = &wire[..];
        assert_eq!(
            read_frame(&mut input, from_shell_cap).unwrap(),
            (REQ_INSPECT, b"abcd".to_vec())
        );
        assert_eq!(
            read_frame(&mut input, from_worker_cap).unwrap(),
            (RES_OK, Vec::new())
        );
        assert!(
            read_frame(&mut input, from_worker_cap).is_err(),
            "nothing left"
        );
        // A header that promises 100 MiB and delivers 3 bytes is an error, not an allocation.
        let mut lie = vec![RES_OK];
        lie.extend_from_slice(&(100u32 << 20).to_le_bytes());
        lie.extend_from_slice(b"abc");
        assert!(read_frame(&mut &lie[..], from_worker_cap).is_err());
        let mut huge = vec![RES_OK];
        huge.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(read_frame(&mut &huge[..], from_worker_cap).is_err());
    }

    #[test]
    fn a_worker_cannot_send_more_than_its_tag_allows() {
        let frame = |tag: u8, len: usize| {
            let mut wire = Vec::new();
            write_frame(&mut wire, tag, &[&vec![7u8; len]]).unwrap();
            wire
        };
        // The right size passes, one byte more does not; unknown tags never do.
        assert!(read_frame(&mut &frame(NEED_OVERLAY, 9)[..], from_worker_cap).is_ok());
        assert!(read_frame(&mut &frame(NEED_OVERLAY, 10)[..], from_worker_cap).is_err());
        assert!(read_frame(&mut &frame(RES_ERR, 4096)[..], from_worker_cap).is_ok());
        assert!(read_frame(&mut &frame(RES_ERR, 4097)[..], from_worker_cap).is_err());
        assert!(read_frame(&mut &frame(RES_OK, 1 << 20)[..], from_worker_cap).is_ok());
        assert!(read_frame(&mut &frame(OVERLAY_REPLY, 1)[..], from_worker_cap).is_err());
        assert!(read_frame(&mut &frame(99, 0)[..], from_worker_cap).is_err());
        // The same for what the worker takes from the shell.
        assert!(read_frame(&mut &frame(REQ_BUILD, 128 * 1024)[..], from_shell_cap).is_ok());
        assert!(read_frame(&mut &frame(REQ_BUILD, 128 * 1024 + 1)[..], from_shell_cap).is_err());
        assert!(read_frame(&mut &frame(RES_OK, 1)[..], from_shell_cap).is_err());
    }

    #[test]
    fn a_build_head_round_trips_and_bad_heads_are_refused() {
        let head = BuildHead {
            sources: 2,
            pages: vec![
                PagePlan {
                    source: 0,
                    index: 3,
                    turn: 90,
                },
                PagePlan {
                    source: 1,
                    index: 0,
                    turn: 0,
                },
            ],
            watermark: Some("대외비".into()),
            stamp: None,
        };
        let bytes = head.encode();
        assert_eq!(BuildHead::decode(&bytes, 64, 5000), Some(head.clone()));
        assert_eq!(BuildHead::decode(&bytes, 1, 5000), None, "too many sources");
        assert_eq!(BuildHead::decode(&bytes, 64, 1), None, "too many pages");
        assert_eq!(
            BuildHead::decode(&bytes[..bytes.len() - 1], 64, 5000),
            None,
            "cut"
        );
        let mut extra = bytes.clone();
        extra.push(0);
        assert_eq!(BuildHead::decode(&extra, 64, 5000), None, "trailing bytes");
        let mut bad_source = head;
        bad_source.pages[1].source = 2;
        assert_eq!(BuildHead::decode(&bad_source.encode(), 64, 5000), None);
        let mut bad_turn = bytes.clone();
        let n = bad_turn.len();
        bad_turn[n - 2] = 45;
        assert_eq!(BuildHead::decode(&bad_turn, 64, 5000), None);
        assert_eq!(BuildHead::decode(&[], 64, 5000), None);
        assert_eq!(BuildHead::decode(&[0xff; 64], 64, 5000), None);
    }

    #[test]
    fn overlay_needs_are_checked() {
        let bytes = encode_overlay_need(595.0, 842.0, true);
        assert_eq!(decode_overlay_need(&bytes), Some((595.0, 842.0, true)));
        assert_eq!(
            decode_overlay_need(&encode_overlay_need(f32::NAN, 1.0, false)),
            None
        );
        assert_eq!(
            decode_overlay_need(&encode_overlay_need(0.0, 1.0, false)),
            None
        );
        assert_eq!(decode_overlay_need(&bytes[..8]), None);
    }
}

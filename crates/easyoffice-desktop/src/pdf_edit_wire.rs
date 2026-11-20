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
//! * shell -> worker: [`REQ_INSPECT`] (the PDF), [`REQ_RENDER`] (the PDF, answered with the PNG of
//!   its first page), or [`REQ_BUILD`] (the plan, see [`BuildHead`]) followed by one [`SOURCE`]
//!   frame per PDF; later [`OVERLAY_REPLY`] / [`OVERLAY_FAIL`].
//! * worker -> shell: [`NEED_OVERLAY`] (the stamp and watermark are made by the shell, which owns
//!   the fonts), [`WARNING`] (one sentence about something of the original that the result does
//!   not keep, sent before the answer of a build), then exactly one of [`RES_OK`] and [`RES_ERR`].

use std::io::{self, Read, Write};

/// The argument that turns the shell binary into a worker.
pub const WORKER_ARG: &str = "--pdf-edit-worker";

pub const REQ_INSPECT: u8 = 1;
pub const REQ_BUILD: u8 = 2;
/// The first page of the PDF as a PNG for the start window's card (PD-179). The payload is the
/// length of a font file (`u32`, little-endian, may be 0), the font that draws the text of a PDF
/// that does not embed its Korean font, and the PDF.
pub const REQ_RENDER: u8 = 4;

/// The largest font file the shell hands the worker (bytes).
pub const MAX_RENDER_FONT_BYTES: usize = 16 * 1_048_576;

/// Splits the payload of a [`REQ_RENDER`] request into the font file and the PDF.
pub fn split_render_request(payload: &[u8]) -> Option<(&[u8], &[u8])> {
    let len = u32::from_le_bytes(payload.get(..4)?.try_into().ok()?) as usize;
    if len > MAX_RENDER_FONT_BYTES {
        return None;
    }
    let rest = payload.get(4..)?;
    (len <= rest.len()).then(|| rest.split_at(len))
}
pub const SOURCE: u8 = 3;
pub const OVERLAY_REPLY: u8 = 5;
pub const OVERLAY_FAIL: u8 = 6;
pub const RES_OK: u8 = 10;
pub const RES_ERR: u8 = 11;
pub const NEED_OVERLAY: u8 = 12;
pub const WARNING: u8 = 13;

/// Most warnings a build reports; a worker that sends more is not trusted.
pub const MAX_WARNINGS: usize = 16;
/// Longest warning (bytes); the worker cuts its sentences at this length.
pub const MAX_WARNING_BYTES: usize = 1024;

/// What a build made: the PDF, and a sentence for each thing of the original it could not keep.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Built {
    pub bytes: Vec<u8>,
    pub warnings: Vec<String>,
}

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

/// Which rendered PDF XObject the worker asks the shell to draw (`NEED_OVERLAY` byte 9).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OverlayKind {
    /// Page watermark only.
    Watermark = 0,
    /// First-page watermark and approval stamp.
    FirstPage = 1,
    /// Value-only appearance for an existing applicant `/Tx` widget.
    ApplicantField = 2,
    /// Value-only appearance for an existing department `/Tx` widget.
    DepartmentField = 3,
    /// Framed `"신청인: …"` appearance for a studio-added applicant `/Tx` widget.
    ApplicantLabeled = 4,
    /// Framed `"부서: …"` appearance for a studio-added department `/Tx` widget.
    DepartmentLabeled = 5,
}

impl OverlayKind {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(Self::Watermark),
            1 => Some(Self::FirstPage),
            2 => Some(Self::ApplicantField),
            3 => Some(Self::DepartmentField),
            4 => Some(Self::ApplicantLabeled),
            5 => Some(Self::DepartmentLabeled),
            _ => None,
        }
    }
}

/// Highlight or underline markup phrase requested by the PDF studio (PD-245).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WireMarkup {
    pub text: String,
    pub underline: bool,
    pub color: [u8; 3],
}

/// Interactive form field edits requested by the PDF studio (PD-245).
/// `None` leaves any existing field untouched; `Some("")` clears a matched field; `Some(v)` sets `/V`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WireForm {
    pub applicant: Option<String>,
    pub department: Option<String>,
}

/// Makes the PDF of an overlay or widget appearance of `width` x `height` points.
/// The shell makes them: the worker process has no fonts and no files.
pub type OverlayMaker<'a> = dyn FnMut(f32, f32, OverlayKind) -> Result<Vec<u8>, String> + 'a;

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

/// Largest answer to a [`REQ_RENDER`] request (bytes): the card picture the shell accepts is at most
/// 1 MiB (`MAX_RENDERED_PNG_BYTES` in `preview/mod.rs`, which a test keeps within this), plus room
/// for what a PNG encoder adds.
pub const MAX_RENDER_RESPONSE_BYTES: usize = 1_048_576 + 16 * 1024;

/// [`from_worker_cap`] for a render request: only the answer or a sentence, and the answer is a
/// small picture, not an edit's result of up to 256 MiB.
pub fn from_worker_render_cap(tag: u8) -> usize {
    match tag {
        RES_ERR => 4096,
        RES_OK => MAX_RENDER_RESPONSE_BYTES,
        _ => 0,
    }
}

/// Largest payload of each tag the shell accepts from a worker (0: the tag is not expected). A
/// worker that has been fed a crafted file is not trusted: it may ask for an overlay (nine
/// bytes), warn with a sentence, fail with a sentence, or answer; only the answer may be large.
pub fn from_worker_cap(tag: u8) -> usize {
    match tag {
        NEED_OVERLAY => 9,
        WARNING => MAX_WARNING_BYTES,
        RES_ERR => 4096,
        RES_OK => MAX_FRAME_BYTES,
        _ => 0,
    }
}

/// Largest payload of each tag the worker accepts from the shell.
pub fn from_shell_cap(tag: u8) -> usize {
    match tag {
        REQ_INSPECT | REQ_RENDER | SOURCE => MAX_FRAME_BYTES,
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

/// Largest text field encoded in a [`BuildHead`] (bytes).
const MAX_HEAD_TEXT_BYTES: usize = 2048;

/// The plan of a build, sent before the sources.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildHead {
    pub sources: usize,
    pub pages: Vec<PagePlan>,
    pub watermark: Option<String>,
    pub stamp: Option<String>,
    pub markup: Option<WireMarkup>,
    pub form: WireForm,
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

fn put_markup(out: &mut Vec<u8>, markup: &Option<WireMarkup>) {
    match markup {
        None => out.push(0),
        Some(m) => {
            out.push(1);
            out.extend_from_slice(&u32::try_from(m.text.len()).unwrap_or(0).to_le_bytes());
            out.extend_from_slice(m.text.as_bytes());
            out.push(u8::from(m.underline));
            out.extend_from_slice(&m.color);
        }
    }
}

impl BuildHead {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(24 + self.pages.len() * 10);
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
        put_markup(&mut out, &self.markup);
        put_text(&mut out, &self.form.applicant);
        put_text(&mut out, &self.form.department);
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
        let markup = at.markup()?;
        let form = WireForm {
            applicant: at.text()?,
            department: at.text()?,
        };
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
            markup,
            form,
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
                if len > MAX_HEAD_TEXT_BYTES {
                    return None;
                }
                String::from_utf8(self.take(len)?.to_vec()).ok().map(Some)
            }
            _ => None,
        }
    }

    fn markup(&mut self) -> Option<Option<WireMarkup>> {
        match self.take(1)?[0] {
            0 => Some(None),
            1 => {
                let len = self.u32()? as usize;
                if len == 0 || len > MAX_HEAD_TEXT_BYTES {
                    return None;
                }
                let text = String::from_utf8(self.take(len)?.to_vec()).ok()?;
                let underline = match self.take(1)?[0] {
                    0 => false,
                    1 => true,
                    _ => return None,
                };
                let color: [u8; 3] = self.take(3)?.try_into().ok()?;
                Some(Some(WireMarkup {
                    text,
                    underline,
                    color,
                }))
            }
            _ => None,
        }
    }
}

/// A request for the overlay or widget appearance of `width` x `height` points.
pub fn encode_overlay_need(width: f32, height: f32, kind: OverlayKind) -> Vec<u8> {
    let mut out = Vec::with_capacity(9);
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.push(kind as u8);
    out
}

pub fn decode_overlay_need(bytes: &[u8]) -> Option<(f32, f32, OverlayKind)> {
    let [a, b, c, d, e, f, g, h, s] = <[u8; 9]>::try_from(bytes).ok()?;
    let (w, h) = (
        f32::from_le_bytes([a, b, c, d]),
        f32::from_le_bytes([e, f, g, h]),
    );
    let kind = OverlayKind::from_byte(s)?;
    (w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0).then_some((w, h, kind))
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
        assert!(read_frame(&mut &frame(WARNING, MAX_WARNING_BYTES)[..], from_worker_cap).is_ok());
        assert!(read_frame(
            &mut &frame(WARNING, MAX_WARNING_BYTES + 1)[..],
            from_worker_cap
        )
        .is_err());
        assert!(read_frame(&mut &frame(WARNING, 1)[..], from_shell_cap).is_err());
        assert!(read_frame(&mut &frame(RES_ERR, 4096)[..], from_worker_cap).is_ok());
        assert!(read_frame(&mut &frame(RES_ERR, 4097)[..], from_worker_cap).is_err());
        assert!(read_frame(&mut &frame(RES_OK, 1 << 20)[..], from_worker_cap).is_ok());
        assert!(read_frame(&mut &frame(OVERLAY_REPLY, 1)[..], from_worker_cap).is_err());
        assert!(read_frame(&mut &frame(99, 0)[..], from_worker_cap).is_err());
        // The answer to a render request is a card picture: far less than an edit's result.
        let render =
            |tag: u8, len: usize| read_frame(&mut &frame(tag, len)[..], from_worker_render_cap);
        assert!(render(RES_OK, MAX_RENDER_RESPONSE_BYTES).is_ok());
        assert!(render(RES_OK, MAX_RENDER_RESPONSE_BYTES + 1).is_err());
        assert!(render(RES_OK, 2 << 20).is_err());
        assert!(render(RES_ERR, 4096).is_ok());
        assert!(render(RES_ERR, 4097).is_err());
        assert!(
            render(NEED_OVERLAY, 9).is_err(),
            "a render never asks for an overlay"
        );
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
            markup: Some(WireMarkup {
                text: "제1조".into(),
                underline: true,
                color: [255, 235, 59],
            }),
            form: WireForm {
                applicant: Some("홍길동".into()),
                department: Some(String::new()),
            },
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
        let bytes = encode_overlay_need(595.0, 842.0, OverlayKind::FirstPage);
        assert_eq!(
            decode_overlay_need(&bytes),
            Some((595.0, 842.0, OverlayKind::FirstPage))
        );
        for kind in [
            OverlayKind::Watermark,
            OverlayKind::ApplicantField,
            OverlayKind::DepartmentField,
            OverlayKind::ApplicantLabeled,
            OverlayKind::DepartmentLabeled,
        ] {
            let encoded = encode_overlay_need(120.0, 20.0, kind);
            assert_eq!(decode_overlay_need(&encoded), Some((120.0, 20.0, kind)));
        }
        assert_eq!(
            decode_overlay_need(&encode_overlay_need(f32::NAN, 1.0, OverlayKind::Watermark)),
            None
        );
        assert_eq!(
            decode_overlay_need(&encode_overlay_need(0.0, 1.0, OverlayKind::Watermark)),
            None
        );
        let mut unknown = bytes.clone();
        unknown[8] = 6;
        assert_eq!(decode_overlay_need(&unknown), None);
        assert_eq!(decode_overlay_need(&bytes[..8]), None);
    }
}

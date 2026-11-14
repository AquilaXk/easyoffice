//! Hyperlinks in BIFF8 (PD-223): the `HLink` record (0x01B8, MS-XLS 2.4.140 with the `HyperlinkObject` and
//! `URLMoniker` of MS-OSHARED 2.3.7) and the `HLinkTooltip` record (0x0800, MS-XLS 2.4.141) that follows
//! it when the link has a screen tip. The model is `sheet_links`.
//!
//! 참조 구현: Apache POI `poi/src/main/java/org/apache/poi/hssf/record/HyperlinkRecord.java` lines 55-64 (the
//! flag bits and the 24-byte tail of a URL moniker), 309-405 (reading: range, class id, stream version,
//! flags, display name, frame name, the URL moniker with its length and tail, the location) and 410-455
//! (writing: the same order, a string counts its terminating zero); Apache-2.0, Copyright The Apache
//! Software Foundation, commit 47dcff769bb395034dddfd59c4609659b6dbc6a4. See `THIRD_PARTY_NOTICES.md`.
//! Differences: the reference reads any moniker and trusts every length; here only a URL moniker, a moniker
//! saved as a string and a location are read, every length is checked against the bytes of the record, every
//! address goes through the policy of `sheet_links` (a file moniker or a path is turned away and counted),
//! the ranges are charged to the document's allowance, and the screen tip record is read and written (the
//! reference has no support for it).
//!
//! A write puts one `HLink` per linked cell with no display name (the cell's own text is the display text,
//! MS-OSHARED 2.3.7.1 makes the field optional). An address outside the workbook is a URL moniker, the
//! location after a `#` of an address stays inside the URL; a location inside the workbook has no moniker.

use crate::sheet_links::{CellLink, LinkAllowance, LinkCollector, SheetLinks};
use crate::utf16_swar::decode_utf16le_swar;
use crate::{CellBox, EasyOfficeError, MAX_BIFF8_COL, MAX_BIFF8_RECORD_DATA_LEN, MAX_BIFF8_ROW};

/// Record type of `HLink`.
pub(crate) const HLINK: u16 = 0x01B8;
/// Record type of `HLinkTooltip` (a future record: its `rt` is also the first field of the data).
pub(crate) const HLINK_TOOLTIP: u16 = 0x0800;

/// `StdLink` class id `{79EAC9D0-BAF9-11CE-8C82-00AA004BA90B}` that opens every `HLink`.
const HLINK_CLSID: [u8; 16] = [
    0xD0, 0xC9, 0xEA, 0x79, 0xF9, 0xBA, 0xCE, 0x11, 0x8C, 0x82, 0x00, 0xAA, 0x00, 0x4B, 0xA9, 0x0B,
];
/// `URLMoniker` class id `{79EAC9E0-BAF9-11CE-8C82-00AA004BA90B}`.
const URL_MONIKER_CLSID: [u8; 16] = [
    0xE0, 0xC9, 0xEA, 0x79, 0xF9, 0xBA, 0xCE, 0x11, 0x8C, 0x82, 0x00, 0xAA, 0x00, 0x4B, 0xA9, 0x0B,
];
/// What follows the address of a URL moniker the spreadsheet program writes: the serial class id, the
/// serial version and the URI flags (24 bytes).
const URL_TAIL: [u8; 24] = [
    0x79, 0x58, 0x81, 0xF4, 0x3B, 0x1D, 0x7F, 0x48, 0xAF, 0x2C, 0x82, 0x5D, 0xC4, 0x85, 0x27, 0x63,
    0x00, 0x00, 0x00, 0x00, 0xA5, 0xAB, 0x00, 0x00,
];

const STREAM_VERSION: u32 = 2;
const HAS_MONIKER: u32 = 0x01;
const IS_ABSOLUTE: u32 = 0x02;
const HAS_LOCATION: u32 = 0x08;
const HAS_DISPLAY_NAME: u32 = 0x10;
const HAS_FRAME_NAME: u32 = 0x80;
const MONIKER_AS_STRING: u32 = 0x100;
/// Bytes of the range and the class id before the stream version.
const HEADER_LEN: usize = 8 + 16;

// ---- reading ----

/// Bytes of a record being read; every read is checked against the end.
struct Bytes<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Bytes<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(n)?;
        let part = self.data.get(self.at..end)?;
        self.at = end;
        Some(part)
    }

    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// A `HyperlinkString`: a count of characters (the terminating zero included) and the UTF-16LE text.
    fn string(&mut self) -> Option<String> {
        let chars = self.u32()? as usize;
        zero_terminated(self.take(chars.checked_mul(2)?)?)
    }
}

/// The text of UTF-16LE `bytes` up to the first zero (which must be there).
fn zero_terminated(bytes: &[u8]) -> Option<String> {
    if bytes.len() % 2 != 0 {
        return None;
    }
    let end = bytes
        .chunks_exact(2)
        .position(|unit| unit == [0, 0])
        .map(|at| at * 2)?;
    decode_utf16le_swar(&bytes[..end]).ok()
}

/// The range of the record's first eight bytes (`Ref8U`): rows then columns, first before last, inside the
/// BIFF8 grid (a column past 255 is no cell of the sheet).
fn read_area(bytes: &mut Bytes) -> Option<CellBox> {
    let (row0, row1) = (bytes.u16()?, bytes.u16()?);
    let (col0, col1) = (bytes.u16()?, bytes.u16()?);
    let in_grid = u32::from(row1) <= MAX_BIFF8_ROW && u32::from(col1) <= MAX_BIFF8_COL;
    (row0 <= row1 && col0 <= col1 && in_grid).then(|| CellBox {
        row0: row0.into(),
        col0: col0.into(),
        row1: row1.into(),
        col1: col1.into(),
    })
}

/// The address of an `HLink` record, or `None` when the record is malformed or links to a file or a path
/// the model has no place for.
fn read_target(bytes: &mut Bytes) -> Option<String> {
    if bytes.take(16)? != HLINK_CLSID || bytes.u32()? != STREAM_VERSION {
        return None;
    }
    let flags = bytes.u32()?;
    if flags & HAS_DISPLAY_NAME != 0 {
        bytes.string()?;
    }
    if flags & HAS_FRAME_NAME != 0 {
        bytes.string()?;
    }
    let mut address = None;
    if flags & HAS_MONIKER != 0 {
        address = Some(if flags & MONIKER_AS_STRING != 0 {
            bytes.string()?
        } else {
            read_url_moniker(bytes)?
        });
    }
    let location = if flags & HAS_LOCATION != 0 {
        Some(bytes.string()?)
    } else {
        None
    };
    match (address, location) {
        (Some(url), Some(place)) if !place.is_empty() => Some(format!("{url}#{place}")),
        (Some(url), _) => Some(url),
        (None, Some(place)) => Some(format!("#{place}")),
        (None, None) => None,
    }
}

/// The address of a URL moniker. Its length covers the zero-terminated address and, when the writer put one,
/// the 24 bytes of the tail; any other moniker (a file) is not an address of the model.
fn read_url_moniker(bytes: &mut Bytes) -> Option<String> {
    if bytes.take(16)? != URL_MONIKER_CLSID {
        return None;
    }
    let length = bytes.u32()? as usize;
    zero_terminated_prefix(bytes.take(length)?)
}

/// The address at the start of a moniker's bytes: UTF-16LE up to its first zero, followed by nothing or by
/// the 24 bytes of the tail.
fn zero_terminated_prefix(moniker: &[u8]) -> Option<String> {
    let end = moniker
        .chunks_exact(2)
        .position(|unit| unit == [0, 0])
        .map(|at| at * 2 + 2)?;
    let rest = moniker.len() - end;
    (rest == 0 || rest == URL_TAIL.len()).then_some(())?;
    zero_terminated(&moniker[..end])
}

/// The links of the worksheet substreams of one workbook while its records are walked. An `HLink` is held
/// until the next record, so that a following `HLinkTooltip` of the same range gives it its tip.
#[derive(Default)]
pub(crate) struct XlsLinkReader {
    allowance: LinkAllowance,
    /// The sheet being read.
    current: LinkCollector,
    /// A bound was reached on this sheet: the records that follow are counted and not read.
    full: bool,
    pending: Option<(CellBox, String)>,
    /// The collectors of the sheets that are done, in sheet order.
    done: Vec<LinkCollector>,
}

impl XlsLinkReader {
    /// Called before every record of a worksheet: an `HLink` waiting for a tip is kept as it is unless
    /// `opcode` is its tip.
    pub(crate) fn before(&mut self, opcode: u16) {
        if opcode != HLINK_TOOLTIP {
            self.flush("");
        }
    }

    fn flush(&mut self, tip: &str) {
        if let Some((area, target)) = self.pending.take() {
            if !self
                .current
                .add_area(&mut self.allowance, area, &target, tip)
            {
                self.full = true;
            }
        }
    }

    /// Reads an `HLink` record (`data` is its payload).
    pub(crate) fn hlink(&mut self, data: &[u8]) {
        if self.full {
            self.current.rejected = self.current.rejected.saturating_add(1);
            return;
        }
        let mut bytes = Bytes { data, at: 0 };
        let parsed = read_area(&mut bytes).zip(read_target(&mut bytes));
        match parsed {
            Some(link) => self.pending = Some(link),
            None => self.current.rejected = self.current.rejected.saturating_add(1),
        }
    }

    /// Reads an `HLinkTooltip` record: it gives its text to the link of the same range that waits.
    pub(crate) fn tooltip(&mut self, data: &[u8]) {
        let Some((area, text)) = read_tooltip(data) else {
            return;
        };
        if self
            .pending
            .as_ref()
            .is_some_and(|(waiting, _)| *waiting == area)
        {
            self.flush(&text);
        }
    }

    /// Ends the worksheet substream being read.
    pub(crate) fn end_sheet(&mut self) {
        self.flush("");
        self.full = false;
        self.done.push(std::mem::take(&mut self.current));
    }

    /// The collectors of the sheets in the order they ended.
    pub(crate) fn finish(mut self) -> Vec<LinkCollector> {
        self.flush("");
        self.done
    }
}

/// The range and the text of an `HLinkTooltip` payload: the record type again, the flags, `Ref8U` and the
/// UTF-16LE text up to its zero (or up to the end of the record when a writer left the zero out).
fn read_tooltip(data: &[u8]) -> Option<(CellBox, String)> {
    let mut bytes = Bytes { data, at: 0 };
    (bytes.u16()? == HLINK_TOOLTIP).then_some(())?;
    bytes.u16()?;
    let area = read_area(&mut bytes)?;
    Some((area, zero_terminated_or_rest(&data[bytes.at..])?))
}

fn zero_terminated_or_rest(bytes: &[u8]) -> Option<String> {
    let even = &bytes[..bytes.len() & !1];
    let end = even
        .chunks_exact(2)
        .position(|unit| unit == [0, 0])
        .map_or(even.len(), |at| at * 2);
    decode_utf16le_swar(&even[..end]).ok()
}

// ---- writing ----

fn push_string(out: &mut Vec<u8>, text: &str) {
    let units: Vec<u16> = text.encode_utf16().collect();
    out.extend_from_slice(&((units.len() + 1) as u32).to_le_bytes());
    for unit in units.iter().chain(std::iter::once(&0u16)) {
        out.extend_from_slice(&unit.to_le_bytes());
    }
}

fn push_record(stream: &mut Vec<u8>, opcode: u16, data: &[u8]) {
    stream.extend_from_slice(&opcode.to_le_bytes());
    stream.extend_from_slice(&(data.len() as u16).to_le_bytes());
    stream.extend_from_slice(data);
}

fn push_area(out: &mut Vec<u8>, row: u16, col: u16) {
    for unit in [row, row, col, col] {
        out.extend_from_slice(&unit.to_le_bytes());
    }
}

/// The payload of the `HLink` record of the cell `(row, col)`.
fn hlink_data(row: u16, col: u16, target: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + 64 + target.len() * 2);
    push_area(&mut out, row, col);
    out.extend_from_slice(&HLINK_CLSID);
    out.extend_from_slice(&STREAM_VERSION.to_le_bytes());
    match target.strip_prefix('#') {
        Some(location) => {
            out.extend_from_slice(&HAS_LOCATION.to_le_bytes());
            push_string(&mut out, location);
        }
        None => {
            out.extend_from_slice(&(HAS_MONIKER | IS_ABSOLUTE).to_le_bytes());
            out.extend_from_slice(&URL_MONIKER_CLSID);
            let units: Vec<u16> = target.encode_utf16().chain(std::iter::once(0)).collect();
            out.extend_from_slice(&((units.len() * 2 + URL_TAIL.len()) as u32).to_le_bytes());
            for unit in units {
                out.extend_from_slice(&unit.to_le_bytes());
            }
            out.extend_from_slice(&URL_TAIL);
        }
    }
    out
}

/// The payload of the `HLinkTooltip` record of the cell `(row, col)`.
fn tooltip_data(row: u16, col: u16, tip: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + tip.len() * 2 + 2);
    out.extend_from_slice(&HLINK_TOOLTIP.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    push_area(&mut out, row, col);
    for unit in tip.encode_utf16().chain(std::iter::once(0)) {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

/// Whether the records of `link` fit the 8,224 bytes a BIFF8 record holds (a long address of characters that take
/// two UTF-16 units each does not): the `HLink` and, with a screen tip, its `HLinkTooltip`.
pub(crate) fn fits(link: &CellLink) -> bool {
    let units = |text: &str| text.encode_utf16().count() + 1;
    let link_len = match link.target.strip_prefix('#') {
        Some(location) => HEADER_LEN + 4 + 4 + 4 + units(location) * 2,
        None => {
            HEADER_LEN
                + 4
                + 4
                + URL_MONIKER_CLSID.len()
                + 4
                + units(&link.target) * 2
                + URL_TAIL.len()
        }
    };
    let tip_len = if link.tip.is_empty() {
        0
    } else {
        12 + units(&link.tip) * 2
    };
    link_len.max(tip_len) <= MAX_BIFF8_RECORD_DATA_LEN
}

/// Appends the records of the links of one sheet in cell order. A link outside the BIFF8 grid is refused.
pub(crate) fn write_links(stream: &mut Vec<u8>, links: &SheetLinks) -> Result<(), EasyOfficeError> {
    for (&(row, col), link) in links {
        if row > MAX_BIFF8_ROW || col > MAX_BIFF8_COL {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "BIFF8 하이퍼링크 칸(row={row}, col={col})이 명세 상한(row<={MAX_BIFF8_ROW}, col<={MAX_BIFF8_COL})을 초과했습니다."
            )));
        }
        if !fits(link) {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "BIFF8 하이퍼링크 칸(row={row}, col={col})의 레코드가 {MAX_BIFF8_RECORD_DATA_LEN}바이트를 넘습니다."
            )));
        }
        let (row, col) = (row as u16, col as u16);
        push_record(stream, HLINK, &hlink_data(row, col, &link.target));
        if !link.tip.is_empty() {
            push_record(stream, HLINK_TOOLTIP, &tooltip_data(row, col, &link.tip));
        }
    }
    Ok(())
}

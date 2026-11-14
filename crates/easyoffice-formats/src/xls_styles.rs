//! Cell fill and font colours in BIFF8 (PD-227): the `FONT` (0x0031), `XF` (0x00E0) and `PALETTE`
//! (0x0092) records and the cell records that name an XF, MS-XLS 2.4.122, 2.4.353 and 2.4.192.
//!
//! 참조 구현: Apache POI `poi/src/main/java/org/apache/poi/hssf/record/PaletteRecord.java` lines 35-47
//! (the 56 palette entries from colour index 8 and their default colours) and 95-128 (an entry
//! replaces the colour of its index), `record/ExtendedFormatRecord.java` lines 106-156 (the bit
//! fields of an XF: the font index, the "not from the parent" attribute bits, the fill pattern in the
//! high 6 bits of the fifth long and the two fill colour indexes in the last short) and
//! `usermodel/HSSFPalette.java` lines 115-132 (`findSimilarColor`: the closest palette colour by the
//! sum of the channel differences); Apache-2.0, Copyright The Apache Software Foundation, commit
//! 47dcff769bb395034dddfd59c4609659b6dbc6a4. See `THIRD_PARTY_NOTICES.md`.
//!
//! Modifications: the reader keeps only what a cell colour needs (a solid fill's foreground and the
//! font colour), resolves colour indexes through the same default table as XLSX (`xlsx_color`, where
//! indexes 0..=7 repeat 8..=15) with the workbook's `PALETTE` over entries 8..=63, and treats the
//! automatic font colour (0x7FFF), the system text colour (0x40) and any pattern other than solid as
//! no colour. Every count is checked against the bytes of its record and against the bounds below.
//! The writer picks the palette itself: a colour that is a default entry keeps its index, the others
//! take free entries (the `PALETTE` record is written only then) and, past 56 entries, the closest
//! colour of the final palette (counted, so the save can say so).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::sheet_styling::xlsx_color::DEFAULT_INDEXED;
use crate::sheet_styling::{CellStyle, SheetStyling, MAX_STYLED_CELLS};
use crate::{MAX_BIFF8_COL, MAX_BIFF8_ROW};

pub(crate) const FONT: u16 = 0x0031;
pub(crate) const XF: u16 = 0x00E0;
pub(crate) const PALETTE: u16 = 0x0092;
const BLANK: u16 = 0x0201;
const MULBLANK: u16 = 0x00BE;
const NUMBER: u16 = 0x0203;
const LABEL: u16 = 0x0204;
const BOOLERR: u16 = 0x0205;
const RK: u16 = 0x027E;
const MULRK: u16 = 0x00BD;
const LABELSST: u16 = 0x00FD;
const FORMULA: u16 = 0x0006;

/// The automatic font colour, the system text colour and the system background colour.
const ICV_AUTO_FONT: u16 = 0x7FFF;
const ICV_SYSTEM_TEXT: u16 = 0x0040;
const ICV_SYSTEM_BACKGROUND: u16 = 0x0041;
/// First colour index of the 56 custom palette entries, and their count.
const FIRST_ENTRY: usize = 8;
const ENTRIES: usize = 56;
/// XF records a workbook may have, the default ones included (MS-XLS `XFCRC` `cxfs`: 16 to 4,050).
const MAX_XFS: u16 = 4_050;
/// A `FONT` or `XF` record a reader keeps (an XF index is 16 bits).
const MAX_RECORDS: usize = 65_536;
/// The fill pattern of a solid fill.
const SOLID: u32 = 1;

// ---- reading ----

/// What the XFs of a workbook say about colours, resolved once the first cell is read.
struct Resolved {
    /// Per XF: the fill and the font colour, `None` when it has neither.
    xfs: Vec<Option<(Option<u32>, Option<u32>)>>,
}

/// Reads the fonts, XFs and palette of the workbook globals and the XF of every cell record.
#[derive(Default)]
pub(crate) struct XlsStyleReader {
    palette: Vec<u32>,
    fonts: Vec<u16>,
    /// Per XF: the font index, the fill pattern and the foreground colour index.
    xfs: Vec<(u16, u32, u16)>,
    resolved: Option<Resolved>,
    cells: BTreeMap<(u32, u32), CellStyle>,
    sheets: Vec<BTreeMap<(u32, u32), CellStyle>>,
}

fn u16_at(body: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*body.get(at)?, *body.get(at + 1)?]))
}

fn u32_at(body: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *body.get(at)?,
        *body.get(at + 1)?,
        *body.get(at + 2)?,
        *body.get(at + 3)?,
    ]))
}

impl XlsStyleReader {
    /// Looks at one record: the globals' `FONT`, `XF` and `PALETTE`, a worksheet's cell records.
    pub(crate) fn record(&mut self, opcode: u16, body: &[u8], in_worksheet: bool) {
        if !in_worksheet {
            match opcode {
                FONT if self.fonts.len() < MAX_RECORDS => {
                    self.fonts.push(u16_at(body, 4).unwrap_or(ICV_AUTO_FONT));
                }
                // A short record still takes its number: the XFs after it keep theirs.
                XF if self.xfs.len() < MAX_RECORDS => {
                    self.xfs.push(
                        match (u16_at(body, 0), u32_at(body, 14), u16_at(body, 18)) {
                            (Some(font), Some(fill), Some(fore)) => (font, fill >> 26, fore & 0x7F),
                            _ => (0, 0, ICV_SYSTEM_TEXT),
                        },
                    );
                }
                PALETTE => self.palette = read_palette(body),
                _ => {}
            }
            return;
        }
        match opcode {
            NUMBER | LABEL | BOOLERR | RK | LABELSST | FORMULA | BLANK => {
                if let (Some(row), Some(col), Some(xf)) =
                    (u16_at(body, 0), u16_at(body, 2), u16_at(body, 4))
                {
                    self.cell(u32::from(row), u32::from(col), xf);
                }
            }
            MULRK | MULBLANK => self.run(opcode, body),
            _ => {}
        }
    }

    /// A `MULRK` (an XF and an RK value per cell) or `MULBLANK` (an XF per cell) record.
    fn run(&mut self, opcode: u16, body: &[u8]) {
        let (Some(row), Some(first)) = (u16_at(body, 0), u16_at(body, 2)) else {
            return;
        };
        let (stride, tail) = if opcode == MULRK { (6, 2) } else { (2, 2) };
        let count = body.len().saturating_sub(4 + tail) / stride;
        for i in 0..count {
            let Some(xf) = u16_at(body, 4 + i * stride) else {
                return;
            };
            self.cell(u32::from(row), u32::from(first) + i as u32, xf);
        }
    }

    fn resolve(&mut self) {
        let font_rgb = |reader: &Self, ifnt: u16| {
            // The font with index 4 does not exist; the indexes after it are one more than the record.
            let record = match ifnt {
                0..=3 => usize::from(ifnt),
                4 => return None,
                n => usize::from(n) - 1,
            };
            reader.fonts.get(record).and_then(|&icv| reader.rgb(icv))
        };
        let default_color = font_rgb(self, 0);
        let xfs = self
            .xfs
            .iter()
            .map(|&(ifnt, pattern, fore)| {
                let font = font_rgb(self, ifnt).filter(|rgb| Some(*rgb) != default_color);
                let fill = (pattern == SOLID).then(|| self.rgb(fore)).flatten();
                (font.is_some() || fill.is_some()).then_some((fill, font))
            })
            .collect();
        self.resolved = Some(Resolved { xfs });
    }

    /// The colour of colour index `icv`: the workbook's palette over the default one.
    fn rgb(&self, icv: u16) -> Option<u32> {
        match icv {
            ICV_AUTO_FONT | ICV_SYSTEM_TEXT => None,
            ICV_SYSTEM_BACKGROUND => Some(0xFFFFFF),
            _ => {
                let at = usize::from(icv);
                if at >= FIRST_ENTRY {
                    if let Some(&rgb) = self.palette.get(at - FIRST_ENTRY) {
                        return Some(rgb);
                    }
                }
                DEFAULT_INDEXED.get(at).copied()
            }
        }
    }

    fn cell(&mut self, row: u32, col: u32, xf: u16) {
        if self.resolved.is_none() {
            self.resolve();
        }
        let Some(Some((fill, font_color))) = self
            .resolved
            .as_ref()
            .and_then(|r| r.xfs.get(usize::from(xf)))
            .copied()
        else {
            return;
        };
        if self.cells.len() < MAX_STYLED_CELLS {
            self.cells.insert(
                (row, col),
                CellStyle {
                    fill,
                    font_color,
                    ..CellStyle::default()
                },
            );
        }
    }

    /// Closes the worksheet being read.
    pub(crate) fn end_sheet(&mut self) {
        self.sheets.push(std::mem::take(&mut self.cells));
    }

    /// The coloured cells of every worksheet, in file order.
    pub(crate) fn finish(self) -> Vec<BTreeMap<(u32, u32), CellStyle>> {
        self.sheets
    }
}

/// The entries of a `PALETTE` record: a count and four bytes (red, green, blue, 0) each; at most the 56
/// entries a workbook has.
fn read_palette(body: &[u8]) -> Vec<u32> {
    let count = usize::from(u16_at(body, 0).unwrap_or(0));
    (0..count.min(ENTRIES))
        .map_while(|i| {
            let at = 2 + i * 4;
            let rgb = body.get(at..at + 3)?;
            Some((u32::from(rgb[0]) << 16) | (u32::from(rgb[1]) << 8) | u32::from(rgb[2]))
        })
        .collect()
}

// ---- writing ----

/// The colours a save writes: the palette, the fonts and the XFs they need, and how cells find their XF.
pub(crate) struct XlsColorPlan {
    /// The 56 palette entries when a colour is not a default one.
    palette: Option<[u32; ENTRIES]>,
    /// The colour index of each font written after the default one (`FONT` record `n + 1`).
    fonts: Vec<u16>,
    /// Per XF written after the default one: its font record (0 is the default font) and fill index.
    xfs: Vec<(usize, Option<u16>)>,
    by_look: HashMap<(Option<u32>, Option<u32>), u16>,
    /// Distinct colours that took the closest palette colour, and cells whose colour does not fit the sheet
    /// size of the format.
    pub(crate) approximated: u32,
    pub(crate) dropped: u32,
}

/// The colour pair of a cell style, `None` when it has no colour.
fn look(style: &CellStyle) -> Option<(Option<u32>, Option<u32>)> {
    (style.fill.is_some() || style.font_color.is_some()).then_some((style.fill, style.font_color))
}

fn in_grid(row: u32, col: u32) -> bool {
    row <= MAX_BIFF8_ROW && col <= MAX_BIFF8_COL
}

fn distance(a: u32, b: u32) -> u32 {
    (0..3)
        .map(|k| ((a >> (8 * k)) & 0xFF).abs_diff((b >> (8 * k)) & 0xFF))
        .sum()
}

impl XlsColorPlan {
    /// The plan of the colours of `stylings` (one per sheet); `None` when no cell has one.
    pub(crate) fn plan(stylings: &[SheetStyling]) -> Option<Self> {
        let mut dropped = 0u32;
        let mut looks: BTreeSet<(Option<u32>, Option<u32>)> = BTreeSet::new();
        for styling in stylings {
            for (&(row, col), style) in &styling.cells {
                let Some(pair) = look(style) else { continue };
                if in_grid(row, col) {
                    looks.insert(pair);
                } else {
                    dropped = dropped.saturating_add(1);
                }
            }
        }
        if looks.is_empty() {
            return None;
        }
        let mut slots = [0u32; ENTRIES];
        slots.copy_from_slice(&DEFAULT_INDEXED[FIRST_ENTRY..]);
        let mut taken = [false; ENTRIES];
        let mut index_of: HashMap<u32, u16> = HashMap::new();
        let colors: BTreeSet<u32> = looks
            .iter()
            .flat_map(|&(fill, font)| [fill, font])
            .flatten()
            .collect();
        for &rgb in &colors {
            if let Some(at) = slots.iter().position(|&entry| entry == rgb) {
                taken[at] = true;
                index_of.insert(rgb, (at + FIRST_ENTRY) as u16);
            }
        }
        let mut custom = false;
        let mut approximated = 0u32;
        let rest: Vec<u32> = colors
            .iter()
            .copied()
            .filter(|rgb| !index_of.contains_key(rgb))
            .collect();
        for rgb in rest {
            let icv = match taken.iter().position(|used| !used) {
                Some(at) => {
                    taken[at] = true;
                    slots[at] = rgb;
                    custom = true;
                    at
                }
                None => {
                    approximated += 1;
                    (0..ENTRIES)
                        .min_by_key(|&at| distance(slots[at], rgb))
                        .unwrap_or(0)
                }
            };
            index_of.insert(rgb, (icv + FIRST_ENTRY) as u16);
        }
        // An XF is a pair of palette indexes, not of colours: colours that share an index (the closest-colour
        // fallback) share the XF, so at most 57 x 57 of them exist whatever the sheet holds.
        let mut fonts: Vec<u16> = Vec::new();
        let mut font_of: HashMap<u16, usize> = HashMap::new();
        let mut xfs: Vec<(usize, Option<u16>)> = Vec::new();
        let mut xf_of: HashMap<(Option<u16>, Option<u16>), u16> = HashMap::new();
        let mut by_look = HashMap::new();
        for &(fill, font) in &looks {
            let key = (
                fill.map(|rgb| index_of[&rgb]),
                font.map(|rgb| index_of[&rgb]),
            );
            let xf = match xf_of.get(&key) {
                Some(&xf) => xf,
                // The default XF is index 0; a table past the limit of the format keeps the default look.
                None => match u16::try_from(xfs.len() + 1).ok().filter(|&n| n < MAX_XFS) {
                    Some(xf) => {
                        let font_record = key.1.map_or(0, |icv| {
                            *font_of.entry(icv).or_insert_with(|| {
                                fonts.push(icv);
                                fonts.len()
                            })
                        });
                        xfs.push((font_record, key.0));
                        xf_of.insert(key, xf);
                        xf
                    }
                    None => {
                        approximated = approximated.saturating_add(1);
                        0
                    }
                },
            };
            by_look.insert((fill, font), xf);
        }
        Some(Self {
            palette: custom.then_some(slots),
            fonts,
            xfs,
            by_look,
            approximated,
            dropped,
        })
    }

    /// The XF of the cell `(row, col)` of `styling`: 0 (the default XF) when it has no colour.
    pub(crate) fn xf(&self, styling: Option<&SheetStyling>, row: u32, col: u32) -> u16 {
        styling
            .and_then(|st| st.cells.get(&(row, col)))
            .and_then(look)
            .and_then(|pair| self.by_look.get(&pair).copied())
            .filter(|_| in_grid(row, col))
            .unwrap_or(0)
    }

    /// The cells of `styling` that have a colour and fit the grid of the format, in row order.
    pub(crate) fn colored_cells(styling: &SheetStyling) -> Vec<(u32, u32)> {
        let mut cells: Vec<(u32, u32)> = styling
            .cells
            .iter()
            .filter(|(&(row, col), style)| in_grid(row, col) && look(style).is_some())
            .map(|(&at, _)| at)
            .collect();
        cells.sort_unstable();
        cells
    }

    /// The `FONT` records that follow the default font: one per font colour, the font index of record
    /// `n` being `n` before the missing index 4 and `n + 1` after it.
    pub(crate) fn font_records(&self, out: &mut Vec<u8>) {
        for &icv in &self.fonts {
            out.extend(font_record(icv));
        }
    }

    /// The `XF` records that follow the default one, then the `PALETTE` record when the palette changed.
    pub(crate) fn xf_and_palette_records(&self, out: &mut Vec<u8>) {
        for &(font_record, fill) in &self.xfs {
            let font_index = if font_record >= 4 {
                font_record + 1
            } else {
                font_record
            } as u16;
            let mut body = [0u8; 20];
            body[0..2].copy_from_slice(&font_index.to_le_bytes());
            // Locked, a cell XF whose parent is the default XF.
            body[4..6].copy_from_slice(&1u16.to_le_bytes());
            // The attribute bits "not from the parent": the font (0x0800) and the pattern (0x4000).
            let mut used = 0u16;
            if font_record != 0 {
                used |= 0x0800;
            }
            let (pattern, fore) = match fill {
                Some(icv) => {
                    used |= 0x4000;
                    (SOLID, icv)
                }
                None => (0, ICV_SYSTEM_TEXT),
            };
            body[8..10].copy_from_slice(&used.to_le_bytes());
            body[14..18].copy_from_slice(&(pattern << 26).to_le_bytes());
            body[18..20].copy_from_slice(&(fore | (ICV_SYSTEM_BACKGROUND << 7)).to_le_bytes());
            out.extend(XF.to_le_bytes());
            out.extend(20u16.to_le_bytes());
            out.extend(body);
        }
        if let Some(palette) = &self.palette {
            out.extend(PALETTE.to_le_bytes());
            out.extend((2 + ENTRIES as u16 * 4).to_le_bytes());
            out.extend((ENTRIES as u16).to_le_bytes());
            for rgb in palette {
                out.extend([(rgb >> 16) as u8, (rgb >> 8) as u8, *rgb as u8, 0]);
            }
        }
    }
}

/// A `FONT` record of the default font (10 point, regular) in the colour index `icv`
/// ([`ICV_AUTO_FONT`] is the automatic colour).
pub(crate) fn font_record(icv: u16) -> Vec<u8> {
    const NAME: &[u8] = b"Arial";
    let mut out = Vec::with_capacity(4 + 16 + NAME.len());
    out.extend(FONT.to_le_bytes());
    out.extend(((16 + NAME.len()) as u16).to_le_bytes());
    out.extend(200u16.to_le_bytes()); // dyHeight = 10pt
    out.extend(0u16.to_le_bytes()); // grbit
    out.extend(icv.to_le_bytes());
    out.extend(400u16.to_le_bytes()); // bls = normal
    out.extend(0u16.to_le_bytes()); // sss
    out.extend([0, 0, 0, 0]); // uls, bFamily, bCharSet, reserved
    out.push(NAME.len() as u8);
    out.push(0); // compressed 8-bit Latin-1
    out.extend(NAME);
    out
}

/// The default font's record.
pub(crate) fn default_font_record() -> Vec<u8> {
    font_record(ICV_AUTO_FONT)
}

/// The record that holds a styled cell without a value (`BLANK`, MS-XLS 2.4.20).
pub(crate) fn blank_record(out: &mut Vec<u8>, row: u16, col: u16, xf: u16) {
    out.extend(BLANK.to_le_bytes());
    out.extend(6u16.to_le_bytes());
    out.extend(row.to_le_bytes());
    out.extend(col.to_le_bytes());
    out.extend(xf.to_le_bytes());
}

/// The cell xf of a `FORMULA` record already built (its XF is the third short of the body).
pub(crate) fn set_formula_xf(record: &mut [u8], xf: u16) {
    if let Some(slot) = record.get_mut(8..10) {
        slot.copy_from_slice(&xf.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xf(font: u16, pattern: u32, fore: u16) -> Vec<u8> {
        let mut body = vec![0u8; 20];
        body[0..2].copy_from_slice(&font.to_le_bytes());
        body[14..18].copy_from_slice(&(pattern << 26).to_le_bytes());
        body[18..20].copy_from_slice(&fore.to_le_bytes());
        body
    }

    fn font(icv: u16) -> Vec<u8> {
        let mut body = vec![0u8; 16];
        body[4..6].copy_from_slice(&icv.to_le_bytes());
        body
    }

    fn reader(fonts: &[u16], xfs: &[Vec<u8>]) -> XlsStyleReader {
        let mut r = XlsStyleReader::default();
        for &icv in fonts {
            r.record(FONT, &font(icv), false);
        }
        for body in xfs {
            r.record(XF, body, false);
        }
        r
    }

    fn cell(row: u16, col: u16, xf: u16) -> Vec<u8> {
        [row, col, xf]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect()
    }

    #[test]
    fn font_index_four_does_not_exist_and_later_indexes_shift_by_one() {
        // Records 0..=4 are the fonts with indexes 0, 1, 2, 3 and 5; the default font is black (8).
        let mut r = reader(
            &[8, 8, 8, 8, 0x0A],
            &[xf(5, 0, 0x40), xf(4, 0, 0x40), xf(6, 0, 0x40)],
        );
        r.record(NUMBER, &cell(0, 0, 0), true);
        r.record(NUMBER, &cell(0, 1, 1), true);
        r.record(NUMBER, &cell(0, 2, 2), true);
        r.end_sheet();
        let cells = &r.finish()[0];
        assert_eq!(
            cells[&(0, 0)].font_color,
            Some(0xFF0000),
            "index 5 is the fifth record"
        );
        assert!(!cells.contains_key(&(0, 1)), "index 4 is no font");
        assert!(!cells.contains_key(&(0, 2)), "an index past the fonts");
    }

    #[test]
    fn a_solid_fill_reads_its_foreground_and_the_palette_overrides_it() {
        let mut r = reader(
            &[8],
            &[
                xf(0, 1, 0x0A),
                xf(0, 2, 0x0A),
                xf(0, 1, 0x41),
                xf(0, 1, 0x40),
            ],
        );
        let mut palette = vec![56, 0];
        for i in 0..56u8 {
            palette.extend([i, 0, 0, 0]);
        }
        r.record(PALETTE, &palette, false);
        for xf in 0..4u16 {
            r.record(BLANK, &cell(xf, 0, xf), true);
        }
        r.end_sheet();
        let cells = &r.finish()[0];
        assert_eq!(
            cells[&(0, 0)].fill,
            Some(0x020000),
            "colour 10 is the third entry now"
        );
        assert!(!cells.contains_key(&(1, 0)), "another pattern is no fill");
        assert_eq!(cells[&(2, 0)].fill, Some(0xFFFFFF), "the system background");
        assert!(
            !cells.contains_key(&(3, 0)),
            "the system text colour is automatic"
        );
    }

    #[test]
    fn runs_of_cells_read_every_xf_and_a_lying_run_stops_at_the_record() {
        let mut r = reader(&[8], &[xf(0, 1, 0x0A), xf(0, 1, 0x0B)]);
        // MULBLANK: row 3, first column 5, xfs 0 and 1, last column (ignored).
        let mut body = cell(3, 5, 0);
        body.truncate(4);
        body.extend([0, 0, 1, 0, 6, 0]);
        r.record(MULBLANK, &body, true);
        // MULRK: row 4, first column 0, two (xf, rk) pairs.
        let mut rk = vec![4, 0, 0, 0];
        rk.extend([1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0]);
        r.record(MULRK, &rk, true);
        // A run that claims 65,535 cells in a record of a few bytes.
        r.record(MULBLANK, &[9, 0, 0xFF, 0xFF, 0, 0], true);
        r.record(MULRK, &[9], true);
        r.end_sheet();
        let cells = &r.finish()[0];
        assert_eq!(cells[&(3, 5)].fill, Some(0xFF0000));
        assert_eq!(cells[&(3, 6)].fill, Some(0x00FF00));
        assert_eq!(cells.len(), 3, "{cells:?}");
    }

    #[test]
    fn short_and_oversized_records_are_ignored_and_bounded() {
        let mut r = XlsStyleReader::default();
        r.record(FONT, &[1, 2], false);
        r.record(XF, &[0; 19], false);
        r.record(PALETTE, &[0xFF, 0xFF, 1, 2, 3], false);
        r.record(PALETTE, &[], false);
        for _ in 0..(MAX_RECORDS + 100) {
            r.record(XF, &xf(0, 1, 2), false);
            r.record(FONT, &font(2), false);
        }
        assert_eq!(r.xfs.len(), MAX_RECORDS);
        assert_eq!(r.fonts.len(), MAX_RECORDS);
        assert!(r.palette.is_empty() || r.palette.len() <= ENTRIES);
        r.record(NUMBER, &[1, 2, 3], true);
        // The last XF a record can name is the last one kept.
        r.record(BLANK, &cell(0, 0, 65_535), true);
        r.end_sheet();
        assert_eq!(r.finish()[0].len(), 1);
    }
}

//! Cell fill and font colours in BIFF8 (PD-227): the `FONT` (0x0031), `XF` (0x00E0) and `PALETTE`
//! (0x0092) records and the cell records that name an XF, MS-XLS 2.4.122, 2.4.353 and 2.4.192. A
//! reader also takes the number format of each XF (PD-235): the format index of the `XF` and the
//! codes of the `FORMAT` records (0x041E, MS-XLS 2.4.126); a save does not write them.
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

use std::sync::Arc;

use crate::sheet_styling::sheet_numfmt::{
    builtin_code, NumFmtTable, CUSTOM_BASE, MAX_NUM_FMT_CODE_BYTES,
};
use crate::sheet_styling::xlsx_color::DEFAULT_INDEXED;
use crate::sheet_styling::{
    clean_font_name, CellStyle, FontTable, SheetStyling, MAX_FONT_NAME_CHARS, MAX_STYLED_CELLS,
};
use crate::xls_builtin_styles::DEFAULT_CELL_XF;
use crate::{MAX_BIFF8_COL, MAX_BIFF8_ROW};

pub(crate) const FONT: u16 = 0x0031;
pub(crate) const XF: u16 = 0x00E0;
pub(crate) const PALETTE: u16 = 0x0092;
const FORMAT: u16 = 0x041E;
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

#[derive(Clone, Copy)]
struct ResolvedXf {
    fill: Option<u32>,
    font_color: Option<u32>,
    num_fmt: u16,
    font: u16,
}

/// What the XFs of a workbook say about colours and number formats, resolved once the first cell is read.
struct Resolved {
    /// Per XF: the fill, the font colour, the number format id and the font family id, `None` when plain.
    xfs: Vec<Option<ResolvedXf>>,
}

/// What the cell records of a workbook say about formatting: the styled cells of each worksheet and the
/// number format and font family tables their ids point into (one table for the workbook, like XLSX).
pub(crate) struct XlsStyles {
    pub(crate) sheets: Vec<BTreeMap<(u32, u32), CellStyle>>,
    pub(crate) num_fmts: Arc<Vec<String>>,
    /// Formats of the file that are not read: an id the table does not define or a code over a bound.
    pub(crate) num_fmts_rejected: u32,
    pub(crate) fonts: Arc<Vec<String>>,
    pub(crate) fonts_rejected: u32,
    pub(crate) default_font: Option<crate::DefaultFont>,
    /// `DATEMODE` says the dates count from 1904 (PD-263).
    pub(crate) date1904: bool,
}

/// Reads the fonts, XFs and palette of the workbook globals and the XF of every cell record.
#[derive(Default)]
pub(crate) struct XlsStyleReader {
    palette: Vec<u32>,
    /// `(icv, name, dy_height)` for each `FONT` record in the globals substream.
    fonts: Vec<(u16, Option<String>, u16)>,
    /// Per XF: the font index, the fill pattern, the foreground colour index and the number format index.
    xfs: Vec<(u16, u32, u16, u16)>,
    /// The codes of the `FORMAT` records by format index.
    formats: HashMap<u16, String>,
    resolved: Option<Resolved>,
    num_fmts: NumFmtTable,
    font_table: FontTable,
    cells: BTreeMap<(u32, u32), CellStyle>,
    sheets: Vec<BTreeMap<(u32, u32), CellStyle>>,
    date1904: bool,
}

/// `DATEMODE` (MS-XLS 2.4.77): 1 when the workbook counts its dates from 1904-01-01.
const DATEMODE: u16 = 0x0022;

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
                    self.fonts.push((
                        u16_at(body, 4).unwrap_or(ICV_AUTO_FONT),
                        read_font_name(body),
                        u16_at(body, 0).unwrap_or(0),
                    ));
                }
                // A short record still takes its number: the XFs after it keep theirs.
                XF if self.xfs.len() < MAX_RECORDS => {
                    self.xfs.push(
                        match (
                            u16_at(body, 0),
                            u16_at(body, 2),
                            u32_at(body, 14),
                            u16_at(body, 18),
                        ) {
                            (Some(font), Some(format), Some(fill), Some(fore)) => {
                                (font, fill >> 26, fore & 0x7F, format)
                            }
                            _ => (0, 0, ICV_SYSTEM_TEXT, 0),
                        },
                    );
                }
                FORMAT if self.formats.len() < MAX_RECORDS => {
                    if let Some((index, code)) = read_format(body) {
                        self.formats.insert(index, code);
                    }
                }
                PALETTE => self.palette = read_palette(body),
                DATEMODE => self.date1904 = u16_at(body, 0) == Some(1),
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
        let font_record = |ifnt: u16| match ifnt {
            // The font with index 4 does not exist; the indexes after it are one more than the record.
            0..=3 => Some(usize::from(ifnt)),
            4 => None,
            n => Some(usize::from(n) - 1),
        };
        let font_rgb = |reader: &Self, ifnt: u16| {
            reader
                .fonts
                .get(font_record(ifnt)?)
                .and_then(|&(icv, _, _)| reader.rgb(icv))
        };
        let default_color = font_rgb(self, 0);
        let default_font = self
            .fonts
            .first()
            .and_then(|(_, n, _)| n.as_deref())
            .and_then(clean_font_name)
            .map(str::to_string);
        let mut font_ids = vec![u16::MAX; self.fonts.len()];
        if !font_ids.is_empty() {
            font_ids[0] = 0;
        }
        let raw = std::mem::take(&mut self.xfs);
        let mut xfs = Vec::with_capacity(raw.len());
        for &(ifnt, pattern, fore, format) in &raw {
            let font_color = font_rgb(self, ifnt).filter(|rgb| Some(*rgb) != default_color);
            let font = match font_record(ifnt) {
                Some(rec) if rec < font_ids.len() => {
                    if font_ids[rec] == u16::MAX {
                        font_ids[rec] = match self.fonts[rec].1.as_deref() {
                            None => 0,
                            Some(raw_name)
                                if default_font.as_deref().is_some_and(|d| {
                                    clean_font_name(raw_name)
                                        .is_some_and(|c| c.eq_ignore_ascii_case(d))
                                }) =>
                            {
                                0
                            }
                            Some(raw_name) => self.font_table.intern(raw_name),
                        };
                    }
                    font_ids[rec]
                }
                _ => 0,
            };
            let fill = (pattern == SOLID).then(|| self.rgb(fore)).flatten();
            let num_fmt = self.num_fmt_id(format);
            xfs.push(
                (font_color.is_some() || fill.is_some() || num_fmt != 0 || font != 0).then_some(
                    ResolvedXf {
                        fill,
                        font_color,
                        num_fmt,
                        font,
                    },
                ),
            );
        }
        self.xfs = raw;
        self.resolved = Some(Resolved { xfs });
    }

    /// The cell format id of the format index `format`: General for index 0, the id of a built-in code or of an
    /// entry of the workbook's table; an index that names no code is counted and reads as General.
    fn num_fmt_id(&mut self, format: u16) -> u16 {
        if format == 0 {
            return 0;
        }
        // A workbook FORMAT record overrides any built-in fallback (MS-XLS 2.4.126, e.g. 5..=8).
        let code = self.formats.get(&format).map(String::as_str).or_else(|| {
            if format < CUSTOM_BASE {
                builtin_code(format)
            } else {
                None
            }
        });
        match code {
            Some(code) => self.num_fmts.intern(code),
            None => {
                self.num_fmts.reject();
                0
            }
        }
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
        let Some(Some(ResolvedXf {
            fill,
            font_color,
            num_fmt,
            font,
        })) = self
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
                    num_fmt,
                    font,
                    ..CellStyle::default()
                },
            );
        }
    }

    /// Closes the worksheet being read.
    pub(crate) fn end_sheet(&mut self) {
        self.sheets.push(std::mem::take(&mut self.cells));
    }

    /// The styled cells of every worksheet, in file order, and the number format and font tables.
    pub(crate) fn finish(self) -> XlsStyles {
        let default_font = self.fonts.first().and_then(|(_, name, dy_height)| {
            if name.is_none() && *dy_height == 0 {
                return None;
            }
            let size_pt = (*dy_height > 0).then(|| f64::from(*dy_height) / 20.0);
            crate::DefaultFont::kept(name.as_deref(), size_pt)
        });
        let (num_fmts, num_fmts_rejected) = self.num_fmts.finish();
        let (fonts, fonts_rejected) = self.font_table.finish();
        XlsStyles {
            sheets: self.sheets,
            num_fmts,
            num_fmts_rejected,
            fonts,
            fonts_rejected,
            default_font,
            date1904: self.date1904,
        }
    }
}

/// Reads the `ShortXLUnicodeString` font name at offset 14 of a BIFF8 `FONT` record (MS-XLS 2.4.122).
fn read_font_name(body: &[u8]) -> Option<String> {
    let count = usize::from(*body.get(14)?);
    if count == 0 || count > MAX_FONT_NAME_CHARS {
        return None;
    }
    let wide = (*body.get(15)? & 0x01) != 0;
    let byte_len = if wide { count * 2 } else { count };
    let chars = body.get(16..16 + byte_len)?;
    let name = if wide {
        let mut units = [0u16; MAX_FONT_NAME_CHARS];
        for (slot, pair) in units[..count].iter_mut().zip(chars.chunks_exact(2)) {
            *slot = u16::from_le_bytes([pair[0], pair[1]]);
        }
        String::from_utf16(&units[..count]).ok()?
    } else if chars.is_ascii() {
        String::from_utf8(chars.to_vec()).ok()?
    } else {
        chars.iter().map(|&b| char::from(b)).collect()
    };
    let trimmed = name.trim_end_matches('\0').trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// The format index and code of a `FORMAT` record: the index, a character count, `XLUnicodeString` flags,
/// then the characters (MS-XLS 2.4.126 and 2.5.293). Truncated or overlong records fail closed (`None`).
fn read_format(body: &[u8]) -> Option<(u16, String)> {
    let index = u16_at(body, 0)?;
    let count = usize::from(u16_at(body, 2)?);
    if count > MAX_NUM_FMT_CODE_BYTES {
        return None;
    }
    let flags = *body.get(4)?;
    let wide = flags & 0x01 != 0;
    let head = 5 + usize::from(flags & 0x08 != 0) * 2 + usize::from(flags & 0x04 != 0) * 4;
    let byte_len = if wide { count * 2 } else { count };
    let chars = body.get(head..head + byte_len)?;
    let code = if wide {
        let mut units = [0u16; MAX_NUM_FMT_CODE_BYTES];
        for (slot, pair) in units[..count].iter_mut().zip(chars.chunks_exact(2)) {
            *slot = u16::from_le_bytes([pair[0], pair[1]]);
        }
        String::from_utf16(&units[..count]).ok()?
    } else if chars.is_ascii() {
        String::from_utf8(chars.to_vec()).ok()?
    } else {
        chars.iter().map(|&b| char::from(b)).collect()
    };
    Some((index, code))
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
                // Built-in style/default XFs occupy indexes 0..=DEFAULT_CELL_XF (15); custom XFs start at 16.
                None => match DEFAULT_CELL_XF
                    .checked_add(1)
                    .and_then(|base| {
                        u16::try_from(xfs.len())
                            .ok()
                            .and_then(|n| base.checked_add(n))
                    })
                    .filter(|&n| n < MAX_XFS)
                {
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
                        DEFAULT_CELL_XF
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

    /// The XF of the cell `(row, col)` of `styling`: `DEFAULT_CELL_XF` (15) when it has no colour.
    pub(crate) fn xf(&self, styling: Option<&SheetStyling>, row: u32, col: u32) -> u16 {
        styling
            .and_then(|st| st.cells.get(&(row, col)))
            .and_then(look)
            .and_then(|pair| self.by_look.get(&pair).copied())
            .filter(|_| in_grid(row, col))
            .unwrap_or(DEFAULT_CELL_XF)
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
    pub(crate) fn font_records(
        &self,
        out: &mut Vec<u8>,
        default_font: Option<&crate::DefaultFont>,
    ) {
        for &icv in &self.fonts {
            out.extend(font_record(icv, default_font));
        }
    }

    /// The `XF` records that follow the 16 built-in XFs, then the `PALETTE` record when the palette changed.
    pub(crate) fn xf_and_palette_records(&self, out: &mut Vec<u8>) {
        for &(font_record, fill) in &self.xfs {
            // Four built-in FONT records (0..=3) precede custom fonts and BIFF8 skips index 4,
            // so a 1-based custom `font_record` maps to BIFF8 font index `font_record + 4`.
            let font_index = if font_record == 0 { 0 } else { font_record + 4 } as u16;
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

/// A `FONT` record of the workbook's default font (`"맑은 고딕"` 11 pt when `None`) in `icv`
/// ([`ICV_AUTO_FONT`] is the automatic colour).
pub(crate) fn font_record(icv: u16, default_font: Option<&crate::DefaultFont>) -> Vec<u8> {
    let (name, size_pt) = match default_font {
        Some(f) => (f.name(), f.size_pt()),
        None => (crate::STANDARD_FONT_NAME, crate::STANDARD_FONT_SIZE_PT),
    };
    let dy_height = ((size_pt * 20.0).round() as u32).clamp(1, u32::from(u16::MAX)) as u16;
    let wide = !name.is_ascii();
    let charset: u8 = if wide { 129 } else { 0 }; // 129 = HANGUL_CHARSET
    let (cch, flags, name_bytes): (u8, u8, Vec<u8>) = if wide {
        let units: Vec<u16> = name.encode_utf16().take(u8::MAX as usize).collect();
        let mut bytes = Vec::with_capacity(units.len() * 2);
        for u in &units {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        (units.len() as u8, 1, bytes)
    } else {
        let ascii = &name.as_bytes()[..name.len().min(u8::MAX as usize)];
        (ascii.len() as u8, 0, ascii.to_vec())
    };
    let mut out = Vec::with_capacity(4 + 16 + name_bytes.len());
    out.extend(FONT.to_le_bytes());
    out.extend(((16 + name_bytes.len()) as u16).to_le_bytes());
    out.extend(dy_height.to_le_bytes());
    out.extend(0u16.to_le_bytes()); // grbit
    out.extend(icv.to_le_bytes());
    out.extend(400u16.to_le_bytes()); // bls = normal
    out.extend(0u16.to_le_bytes()); // sss
    out.extend([0, 2, charset, 0]); // uls, bFamily = Swiss (2), bCharSet, reserved
    out.push(cch);
    out.push(flags);
    out.extend(name_bytes);
    out
}

/// The default font's record.
pub(crate) fn default_font_record(default_font: Option<&crate::DefaultFont>) -> Vec<u8> {
    font_record(ICV_AUTO_FONT, default_font)
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
        let cells = &r.finish().sheets[0];
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
        let cells = &r.finish().sheets[0];
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
        let cells = &r.finish().sheets[0];
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
        assert_eq!(r.finish().sheets[0].len(), 1);
    }

    fn xf_with_format(format: u16) -> Vec<u8> {
        let mut body = xf(0, 0, 0x40);
        body[2..4].copy_from_slice(&format.to_le_bytes());
        body
    }

    fn format_record(index: u16, code: &str) -> Vec<u8> {
        let mut body = index.to_le_bytes().to_vec();
        body.extend((code.chars().count() as u16).to_le_bytes());
        if code.is_ascii() {
            body.push(0);
            body.extend(code.bytes());
        } else {
            body.push(1);
            body.extend(code.encode_utf16().flat_map(u16::to_le_bytes));
        }
        body
    }

    #[test]
    fn number_formats_come_from_built_in_ids_and_format_records() {
        let mut r = reader(
            &[8],
            &[xf_with_format(2), xf_with_format(164), xf_with_format(165)],
        );
        r.record(FORMAT, &format_record(164, "yyyy\\-mm\\-dd"), false);
        r.record(FORMAT, &format_record(165, "0\"원\""), false);
        for (col, xf) in [(0, 0), (1, 1), (2, 2)] {
            r.record(NUMBER, &cell(0, col, xf), true);
        }
        r.end_sheet();
        let styles = r.finish();
        let cells = &styles.sheets[0];
        assert_eq!(cells[&(0, 0)].num_fmt, 2, "built-in 0.00 keeps its id");
        let code = |col| {
            crate::sheet_styling::sheet_numfmt::resolve_code(
                cells[&(0, col)].num_fmt,
                &styles.num_fmts,
            )
        };
        assert_eq!(code(1), Some("yyyy\\-mm\\-dd"));
        assert_eq!(code(2), Some("0\"원\""));
        assert_eq!(styles.num_fmts_rejected, 0);
    }

    #[test]
    fn a_format_index_that_names_no_code_is_counted_and_reads_as_general() {
        // 23 is an undefined id below CUSTOM_BASE; 200 has no FORMAT record.
        let mut r = reader(&[8], &[xf_with_format(23), xf_with_format(200)]);
        r.record(NUMBER, &cell(0, 0, 0), true);
        r.record(NUMBER, &cell(0, 1, 1), true);
        r.end_sheet();
        let styles = r.finish();
        assert!(styles.sheets[0].is_empty());
        assert_eq!(styles.num_fmts_rejected, 2);
    }

    #[test]
    fn currency_format_records_under_custom_base_and_truncated_or_overlong_records_behave_safely() {
        let mut r = reader(
            &[8],
            &[xf_with_format(5), xf_with_format(166), xf_with_format(167)],
        );
        // Currency index 5 with rich/ext header flags (`0x0C`) before ASCII payload.
        let mut rich_ext = Vec::new();
        rich_ext.extend_from_slice(&5u16.to_le_bytes());
        rich_ext.extend_from_slice(&6u16.to_le_bytes());
        rich_ext.push(0x0C);
        rich_ext.extend_from_slice(&[0u8; 6]);
        rich_ext.extend_from_slice(b"$#,##0");
        r.record(FORMAT, &rich_ext, false);
        // Truncated wide record: claims 20 UTF-16 units, carries 2 bytes -> rejected.
        let mut truncated = Vec::new();
        truncated.extend_from_slice(&166u16.to_le_bytes());
        truncated.extend_from_slice(&20u16.to_le_bytes());
        truncated.extend_from_slice(&[1, b'0', 0]);
        r.record(FORMAT, &truncated, false);
        // Overlong record: claims 300 chars (> 255) -> rejected before allocation.
        let mut overlong = Vec::new();
        overlong.extend_from_slice(&167u16.to_le_bytes());
        overlong.extend_from_slice(&300u16.to_le_bytes());
        overlong.push(0);
        overlong.resize(305, b'0');
        r.record(FORMAT, &overlong, false);

        for (col, xf) in [(0, 0), (1, 1), (2, 2)] {
            r.record(NUMBER, &cell(0, col, xf), true);
        }
        r.end_sheet();
        let styles = r.finish();
        let cells = &styles.sheets[0];
        assert_eq!(
            crate::sheet_styling::sheet_numfmt::resolve_code(
                cells[&(0, 0)].num_fmt,
                &styles.num_fmts
            ),
            Some("$#,##0")
        );
        assert!(!cells.contains_key(&(0, 1)) && !cells.contains_key(&(0, 2)));
        assert_eq!(styles.num_fmts_rejected, 2);
    }

    #[test]
    fn font_reader_decodes_utf16le_family_and_rejects_truncated_records() {
        let mut r = XlsStyleReader::default();
        // Font 0: default "Arial"
        let mut f0 = vec![0u8; 16];
        f0[4..6].copy_from_slice(&ICV_AUTO_FONT.to_le_bytes());
        f0[14] = 5;
        f0[15] = 0;
        f0.extend_from_slice(b"Arial");
        r.record(FONT, &f0, false);

        // Font 1: UTF-16LE "맑은 고딕"
        let ko = "맑은 고딕";
        let units: Vec<u16> = ko.encode_utf16().collect();
        let mut f1 = vec![0u8; 16];
        f1[4..6].copy_from_slice(&ICV_AUTO_FONT.to_le_bytes());
        f1[14] = units.len() as u8;
        f1[15] = 1;
        for u in units {
            f1.extend_from_slice(&u.to_le_bytes());
        }
        r.record(FONT, &f1, false);

        // Font 2: truncated wide FONT record -> fails closed (`None` -> font 0)
        let mut f2 = vec![0u8; 16];
        f2[4..6].copy_from_slice(&ICV_AUTO_FONT.to_le_bytes());
        f2[14] = 10;
        f2[15] = 1;
        f2.extend_from_slice(&[0x41, 0x00]);
        r.record(FONT, &f2, false);

        r.record(XF, &xf(0, 0, ICV_SYSTEM_TEXT), false);
        r.record(XF, &xf(1, 0, ICV_SYSTEM_TEXT), false);
        r.record(XF, &xf(2, 0, ICV_SYSTEM_TEXT), false);
        r.record(NUMBER, &cell(0, 0, 1), true);
        r.record(NUMBER, &cell(0, 1, 2), true);
        r.end_sheet();

        let styles = r.finish();
        assert_eq!(styles.fonts.as_slice(), [ko]);
        assert_eq!(styles.sheets[0][&(0, 0)].font, 1);
        assert!(!styles.sheets[0].contains_key(&(0, 1)));
    }
}

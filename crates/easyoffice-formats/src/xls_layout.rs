//! BIFF8 worksheet layout records (PD-308): `DEFAULTROWHEIGHT` (`0x0225`), `DEFCOLWIDTH` (`0x0055`),
//! `STANDARDWIDTH` (`0x0099`), `COLINFO` (`0x007D`), and `ROW` (`0x0208`).

use std::collections::BTreeMap;

use crate::ole2_codec::read_u16_le;
use crate::sheet_styling::sheet_calc_options::{write_biff8_calc_records, Biff8CalcReader};
use crate::sheet_styling::sheet_row_height::{
    default_row_centipt_for_font, kept_default_row_height,
};
use crate::sheet_styling::xlsx_layout::normalize_runs;
use crate::sheet_styling::xlsx_styles::{px_to_xlsx_width, xlsx_width_to_px};
use crate::{
    CalcOptions, DefaultColWidth, DefaultFont, RowHeight, SheetStyling, MAX_BIFF8_COL,
    MAX_BIFF8_ROW, MAX_COL_WIDTH_CHARS, MIN_COL_WIDTH_CHARS,
};

pub(crate) const BIFF8_DEFCOLWIDTH: u16 = 0x0055;
pub(crate) const BIFF8_COLINFO: u16 = 0x007D;
pub(crate) const BIFF8_STANDARDWIDTH: u16 = 0x0099;
pub(crate) const BIFF8_ROW: u16 = 0x0208;
pub(crate) const BIFF8_DEFAULTROWHEIGHT: u16 = 0x0225;

const STANDARD_DEFCOLWIDTH: u16 = 8;
const MAX_COLINFO_WIDTH: u16 = (MAX_COL_WIDTH_CHARS as u16) * 256;
const MAX_COL_INFOS: usize = (MAX_BIFF8_COL as usize) + 1;
const BITSET_WORDS: usize = ((MAX_BIFF8_ROW as usize) + 1) / 64;

const ROW_FLAG_DYZERO: u16 = 0x0020;
const ROW_FLAG_UNSYNCED: u16 = 0x0040;
const COL_FLAG_HIDDEN: u16 = 0x0001;
const COL_FLAG_USER_SET: u16 = 0x0002;
const DEF_ROW_UNSYNCED: u16 = 0x0001;
const DEF_ROW_DYZERO: u16 = 0x0002;
const MAX_HEIGHT_TWIPS: u32 = 409 * 20;

fn centipt_to_twips(centipt: u32) -> u16 {
    ((f64::from(centipt) / 5.0).round() as u32).clamp(1, MAX_HEIGHT_TWIPS) as u16
}

fn default_row_twips(styling: Option<&SheetStyling>, wb_font: Option<&DefaultFont>) -> (u16, u16) {
    if let Some(h) = styling.and_then(|st| st.default_row_height) {
        let flags = if h.custom { DEF_ROW_UNSYNCED } else { 0 };
        (flags, centipt_to_twips(h.centipt))
    } else {
        let font = styling.and_then(|st| st.default_font.as_ref()).or(wb_font);
        (0, centipt_to_twips(default_row_centipt_for_font(font)))
    }
}

/// Emits calculation settings, `DEFAULTROWHEIGHT`, `DEFCOLWIDTH`, `COLINFO`, and `STANDARDWIDTH` before `DIMENSIONS`.
pub(crate) fn write_sheet_layout_globals(
    out: &mut Vec<u8>,
    styling: Option<&SheetStyling>,
    wb_font: Option<&DefaultFont>,
    wb_calc: &CalcOptions,
) {
    write_biff8_calc_records(out, wb_calc);
    let (flags, def_twips) = default_row_twips(styling, wb_font);
    out.extend_from_slice(&BIFF8_DEFAULTROWHEIGHT.to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&def_twips.to_le_bytes());

    let dcw = styling.and_then(|st| st.default_col_width);
    let base_chars = dcw
        .and_then(|w| w.base)
        .unwrap_or(u32::from(STANDARD_DEFCOLWIDTH))
        .min(MAX_BIFF8_COL) as u16;
    out.extend_from_slice(&BIFF8_DEFCOLWIDTH.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&base_chars.to_le_bytes());

    if let Some(st) = styling {
        write_col_infos(out, st);
    }

    if let Some(width) = dcw.and_then(|w| w.width) {
        let raw = ((width * 256.0).round() as u32).clamp(1, u32::from(MAX_COLINFO_WIDTH)) as u16;
        out.extend_from_slice(&BIFF8_STANDARDWIDTH.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&raw.to_le_bytes());
    }
}

fn write_col_infos(out: &mut Vec<u8>, styling: &SheetStyling) {
    if styling.columns.is_empty() && styling.hidden_cols.is_empty() {
        return;
    }
    let digit_px = styling.digit_px();
    let default_px = styling.default_col_px();
    let mut by_col: BTreeMap<u32, (Option<u32>, bool)> = styling
        .columns
        .iter()
        .take_while(|(&c, _)| c <= MAX_BIFF8_COL)
        .map(|(&c, &px)| (c, (Some(px), false)))
        .collect();
    for &(first, last) in &styling.hidden_cols {
        if first > MAX_BIFF8_COL {
            continue;
        }
        for col in first..=last.min(MAX_BIFF8_COL) {
            by_col.entry(col).or_insert((None, false)).1 = true;
        }
    }
    let mut run: Option<(u32, u32, Option<u32>, bool)> = None;
    for (col, (px, hidden)) in by_col {
        match run {
            Some((first, last, cur_px, cur_hidden))
                if col == last + 1 && px == cur_px && hidden == cur_hidden =>
            {
                run = Some((first, col, cur_px, cur_hidden));
            }
            Some((first, last, cur_px, cur_hidden)) => {
                push_col_info(out, first, last, cur_px, default_px, cur_hidden, digit_px);
                run = Some((col, col, px, hidden));
            }
            None => run = Some((col, col, px, hidden)),
        }
    }
    if let Some((first, last, px, hidden)) = run {
        push_col_info(out, first, last, px, default_px, hidden, digit_px);
    }
}

fn push_col_info(
    out: &mut Vec<u8>,
    first: u32,
    last: u32,
    px: Option<u32>,
    default_px: u32,
    hidden: bool,
    digit_px: u32,
) {
    let chars = px_to_xlsx_width(px.unwrap_or(default_px), digit_px);
    let coldx = ((chars * 256.0).round() as u32).clamp(1, u32::from(u16::MAX)) as u16;
    let mut grbit = 0u16;
    if hidden {
        grbit |= COL_FLAG_HIDDEN;
    }
    if px.is_some() {
        grbit |= COL_FLAG_USER_SET;
    }
    out.extend_from_slice(&BIFF8_COLINFO.to_le_bytes());
    out.extend_from_slice(&12u16.to_le_bytes());
    out.extend_from_slice(&(first as u16).to_le_bytes());
    out.extend_from_slice(&(last as u16).to_le_bytes());
    out.extend_from_slice(&coldx.to_le_bytes());
    out.extend_from_slice(&0x000Fu16.to_le_bytes()); // ixfe = 15
    out.extend_from_slice(&grbit.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
}

/// Emits sorted `ROW` (`0x0208`) records for explicit row heights and hidden rows within `0..=65535`.
pub(crate) fn write_row_records(
    out: &mut Vec<u8>,
    styling: Option<&SheetStyling>,
    wb_font: Option<&DefaultFont>,
) {
    let Some(st) = styling else {
        return;
    };
    if st.row_heights.is_empty() && st.hidden_rows.is_empty() {
        return;
    }
    let (_, def_twips) = default_row_twips(Some(st), wb_font);
    let mut h_iter = st
        .row_heights
        .iter()
        .take_while(|(&r, _)| r <= MAX_BIFF8_ROW)
        .peekable();
    let mut hid_runs = st.hidden_rows.clone();
    normalize_runs(&mut hid_runs);
    let mut hid_idx = 0usize;
    let mut hid_cur = hid_runs
        .first()
        .map(|&(s, _)| s)
        .filter(|&s| s <= MAX_BIFF8_ROW);

    while h_iter.peek().is_some() || hid_cur.is_some() {
        let h_row = h_iter.peek().map(|(&r, _)| r);
        let row = match (h_row, hid_cur) {
            (Some(a), Some(b)) => a.min(b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => break,
        };
        let height = if h_row == Some(row) {
            Some(*h_iter.next().expect("peeked row").1)
        } else {
            None
        };
        let hidden = if hid_cur == Some(row) {
            let end = hid_runs[hid_idx].1.min(MAX_BIFF8_ROW);
            if row < end {
                hid_cur = Some(row + 1);
            } else {
                hid_idx += 1;
                hid_cur = hid_runs
                    .get(hid_idx)
                    .map(|&(s, _)| s)
                    .filter(|&s| s <= MAX_BIFF8_ROW);
            }
            true
        } else {
            false
        };
        push_row_record(out, row as u16, height, hidden, def_twips);
    }
}

fn push_row_record(
    out: &mut Vec<u8>,
    row: u16,
    height: Option<RowHeight>,
    hidden: bool,
    def_twips: u16,
) {
    let (miy_rw, custom) = match height {
        Some(h) => (centipt_to_twips(h.centipt), h.custom),
        None => (def_twips, false),
    };
    let mut grbit = 0u16;
    if hidden {
        grbit |= ROW_FLAG_DYZERO;
    }
    if custom {
        grbit |= ROW_FLAG_UNSYNCED;
    }
    out.extend_from_slice(&BIFF8_ROW.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(&row.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // colMic
    out.extend_from_slice(&0u16.to_le_bytes()); // colMac
    out.extend_from_slice(&miy_rw.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // irwMac
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&grbit.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // ixfe
}

/// Raw layout state collected during one BIFF8 worksheet substream before Globals fonts settle.
pub(crate) struct RawXlsSheetLayout {
    default_row_height: Option<RowHeight>,
    def_col_width: Option<u32>,
    standard_width: Option<f64>,
    col_infos: Vec<(u32, u32, u16, bool, bool)>,
    rows: BTreeMap<u32, (u32, bool, bool)>,
    hidden_bits: Box<[u64; BITSET_WORDS]>,
    calc_options: Option<CalcOptions>,
}

impl Default for RawXlsSheetLayout {
    fn default() -> Self {
        Self {
            default_row_height: None,
            def_col_width: None,
            standard_width: None,
            col_infos: Vec::new(),
            rows: BTreeMap::new(),
            hidden_bits: Box::new([0u64; BITSET_WORDS]),
            calc_options: None,
        }
    }
}

impl RawXlsSheetLayout {
    fn hidden_runs(&self) -> Vec<(u32, u32)> {
        let mut runs = Vec::new();
        let mut run_start: Option<u32> = None;
        for (word_idx, &word) in self.hidden_bits.iter().enumerate() {
            if word == 0 {
                if let Some(start) = run_start.take() {
                    runs.push((start, (word_idx as u32) * 64 - 1));
                }
                continue;
            }
            if word == u64::MAX {
                run_start.get_or_insert((word_idx as u32) * 64);
                continue;
            }
            let base = (word_idx as u32) * 64;
            for bit in 0..64u32 {
                let set = ((word >> bit) & 1) != 0;
                match (set, run_start) {
                    (true, None) => run_start = Some(base + bit),
                    (false, Some(start)) => {
                        runs.push((start, base + bit - 1));
                        run_start = None;
                    }
                    _ => {}
                }
            }
        }
        if let Some(start) = run_start {
            runs.push((start, MAX_BIFF8_ROW));
        }
        runs
    }

    pub(crate) fn resolve(
        self,
        default_font: Option<&DefaultFont>,
        digit_px: u32,
        styling: &mut SheetStyling,
    ) {
        let raw_default = self.default_row_height;
        styling.default_row_height = kept_default_row_height(raw_default, default_font);
        let base = self
            .def_col_width
            .filter(|&b| b != u32::from(STANDARD_DEFCOLWIDTH));
        styling.default_col_width = match (self.standard_width, base) {
            (Some(w), b) => Some(DefaultColWidth {
                width: Some(w),
                base: b,
            }),
            (None, Some(b)) => Some(DefaultColWidth {
                width: None,
                base: Some(b),
            }),
            (None, None) => None,
        };
        for &(first, last, coldx, hidden, custom_width) in &self.col_infos {
            if hidden {
                styling.hidden_cols.push((first, last));
            }
            if custom_width {
                let px = xlsx_width_to_px(f64::from(coldx) / 256.0, digit_px);
                for col in first..=last {
                    styling.columns.insert(col, px);
                }
            }
        }
        normalize_runs(&mut styling.hidden_cols);
        styling.hidden_rows = self.hidden_runs();
        let raw_centipt = raw_default.map(|h| h.centipt);
        for (row, (centipt, custom, hidden)) in self.rows {
            if custom || (!hidden && Some(centipt) != raw_centipt) {
                styling
                    .row_heights
                    .insert(row, RowHeight { centipt, custom });
            }
        }
        if let Some(opts) = self.calc_options {
            styling.calc_options = opts;
        }
    }
}

/// Collects BIFF8 worksheet layout records across all sheets of one workbook.
#[derive(Default)]
pub(crate) struct XlsLayoutReader {
    current: RawXlsSheetLayout,
    sheets: Vec<RawXlsSheetLayout>,
    calc: Biff8CalcReader,
}

impl XlsLayoutReader {
    pub(crate) fn global_calc_record(&mut self, opcode: u16, body: &[u8]) {
        self.calc.global_record(opcode, body);
    }

    pub(crate) fn record(&mut self, opcode: u16, body: &[u8]) {
        if self.sheets.is_empty() {
            self.calc.sheet0_record(opcode, body);
        }
        match opcode {
            BIFF8_DEFAULTROWHEIGHT => {
                if let (Ok(flags), Ok(twips)) = (read_u16_le(body, 0), read_u16_le(body, 2)) {
                    if (flags & DEF_ROW_DYZERO) == 0 && twips > 0 {
                        let custom = (flags & DEF_ROW_UNSYNCED) != 0;
                        self.current.default_row_height =
                            RowHeight::from_points(f64::from(twips) / 20.0, custom);
                    }
                }
            }
            BIFF8_DEFCOLWIDTH => {
                if let Ok(chars) = read_u16_le(body, 0) {
                    if u32::from(chars) <= MAX_BIFF8_COL {
                        self.current.def_col_width = Some(u32::from(chars));
                    }
                }
            }
            BIFF8_STANDARDWIDTH => {
                if let Ok(raw) = read_u16_le(body, 0) {
                    let w = f64::from(raw) / 256.0;
                    if raw > 0 && (MIN_COL_WIDTH_CHARS..=MAX_COL_WIDTH_CHARS).contains(&w) {
                        self.current.standard_width = Some(w);
                    }
                }
            }
            BIFF8_COLINFO if self.current.col_infos.len() < MAX_COL_INFOS => {
                if let (Ok(first), Ok(last), Ok(coldx), Ok(grbit)) = (
                    read_u16_le(body, 0),
                    read_u16_le(body, 2),
                    read_u16_le(body, 4),
                    read_u16_le(body, 8),
                ) {
                    if body.len() >= 10
                        && first <= last
                        && u32::from(last) <= MAX_BIFF8_COL
                        && coldx > 0
                        && coldx <= MAX_COLINFO_WIDTH
                    {
                        let hidden = (grbit & COL_FLAG_HIDDEN) != 0;
                        let custom_width = (grbit & COL_FLAG_USER_SET) != 0 || !hidden;
                        self.current.col_infos.push((
                            u32::from(first),
                            u32::from(last),
                            coldx,
                            hidden,
                            custom_width,
                        ));
                    }
                }
            }
            BIFF8_ROW if body.len() >= 16 => {
                if let (Ok(rw), Ok(miy_rw), Ok(grbit)) = (
                    read_u16_le(body, 0),
                    read_u16_le(body, 6),
                    read_u16_le(body, 12),
                ) {
                    let row = u32::from(rw);
                    let hidden = (grbit & ROW_FLAG_DYZERO) != 0;
                    if hidden {
                        let idx = row as usize;
                        self.current.hidden_bits[idx >> 6] |= 1u64 << (idx & 63);
                    }
                    let custom = (grbit & ROW_FLAG_UNSYNCED) != 0;
                    let twips = miy_rw & 0x7FFF;
                    if (miy_rw & 0x8000) == 0 && twips > 0 {
                        if let Some(h) = RowHeight::from_points(f64::from(twips) / 20.0, custom) {
                            self.current.rows.insert(row, (h.centipt, custom, hidden));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn end_sheet(&mut self) {
        self.sheets.push(std::mem::take(&mut self.current));
    }

    pub(crate) fn finish(mut self) -> Vec<RawXlsSheetLayout> {
        let calc = self.calc.finish();
        if let Some(first) = self.sheets.first_mut() {
            first.calc_options = Some(calc);
        }
        self.sheets
    }
}

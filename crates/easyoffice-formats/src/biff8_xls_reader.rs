//! BIFF8 `.xls` record stream reader (`Workbook` / `Book` OLE2 substreams).
//!
//! 참조 구현 (reference implementation): calamine 0.38.0 (MIT, Copyright (c) 2016 Johann Tuffe),
//! https://github.com/tafia/calamine, `src/xls.rs` (`parse_bool_err`, `parse_number`, `parse_rk`,
//! `parse_mul_rk`, `parse_labelsst`, `parse_label`) and MS-XLS §2.4.24 (`BoolErr`: `bBoolErr` at
//! byte 6, `fError` at byte 7). Explicit cell value types (`CellValueKind`) are recorded in sparse
//! delta form on [`SheetStyling`] so numeric-looking `LABELSST` / `LABEL` strings stay text.

use crate::biff8_formula::{formula_text, FORMULA_FIXED_LEN};
use crate::biff8_xls_codec::LegacyXlsPayload;
use crate::cell_type::{biff8_error_str, pack_cell_pos, CellValueKind};
use crate::decode_budget::DecodeBudget;
use crate::ole2_codec::{
    decode_utf16le, read_u16_le, read_u32_le, read_u64_le, Ole2ContainerCodec,
};
use crate::pages_proto::FastU64Map;
use crate::sheet_links::xls_links::{self, XlsLinkReader};
use crate::sheet_links::LinkCollector;
use crate::sheet_styling::{SheetState, SheetStyling, MAX_STYLED_CELLS};
use crate::xls_boundsheet;
use crate::xls_sst_cursor::SstContinueCursor;
use crate::xls_styles::{XlsStyleReader, XlsStyles};
use crate::{DocumentFormat, EasyOfficeError, LegacyOle2Codec};

/// What the decoded-budget error names when a cell's text is refused.
const XLS_BUDGET_WHAT: &str = "XLS 셀 텍스트";

/// BIFF8 BOF substream type of a worksheet.
const XLS_SUBSTREAM_WORKSHEET: u16 = 0x0010;

/// Payload range of one BIFF8 record within the workbook stream.
#[derive(Clone, Copy)]
struct XlsRecord {
    start: usize,
    end: usize,
}

impl XlsRecord {
    #[inline]
    fn len(&self) -> usize {
        self.end - self.start
    }
}

/// Mutable state while walking the BIFF8 record stream.
#[derive(Default)]
struct XlsParseState {
    boundsheet_names: Vec<String>,
    /// The `hsState` of each `BOUNDSHEET`, in the order of the names.
    boundsheet_states: Vec<SheetState>,
    sst_strings: Vec<String>,
    sheets: Vec<LegacyXlsPayload>,
    sheet_cell_types: Vec<FastU64Map<CellValueKind>>,
    current_cells: Vec<(u32, u32, String)>,
    current_types: FastU64Map<CellValueKind>,
    in_worksheet: bool,
    last_formula_cell_idx: Option<usize>,
    /// Decoded bytes of the cell texts, over all worksheets of the workbook (PD-175).
    budget: DecodeBudget,
    /// The hyperlinks of the worksheets (PD-223).
    links: XlsLinkReader,
    /// The fill and font colours of the cells (PD-227).
    styles: XlsStyleReader,
}

impl XlsParseState {
    fn record_kind(&mut self, row: u32, col: u32, raw: &str, kind: CellValueKind) {
        if raw.is_empty() {
            return;
        }
        // Error literals outside MS-XLS §2.5.10 `BErr` (`#SPILL!`, `#CALC!`) have no `BoolErr`
        // encoding in BIFF8 and are written as `LABELSST`, so reading them back keeps `Error`.
        if kind == CellValueKind::Text
            && CellValueKind::infer(raw) == CellValueKind::Error
            && crate::cell_type::biff8_error_code(raw).is_none()
        {
            return;
        }
        let packed = pack_cell_pos(row, col);
        if kind == CellValueKind::infer(raw) {
            self.current_types.remove(&packed);
        } else if self.current_types.len() < MAX_STYLED_CELLS
            || self.current_types.contains_key(&packed)
        {
            self.current_types.insert(packed, kind);
        }
    }

    fn push_typed_cell(
        &mut self,
        row: u32,
        col: u32,
        text: String,
        kind: CellValueKind,
    ) -> Result<(), EasyOfficeError> {
        self.budget.charge_cell(text.len(), XLS_BUDGET_WHAT)?;
        self.record_kind(row, col, &text, kind);
        self.current_cells.push((row, col, text));
        self.last_formula_cell_idx = None;
        Ok(())
    }

    /// Closes the current worksheet substream (EOF record).
    fn end_substream(&mut self) {
        self.last_formula_cell_idx = None;
        if !self.in_worksheet {
            return;
        }
        let sheet_idx = self.sheets.len();
        let name = self
            .boundsheet_names
            .get(sheet_idx)
            .cloned()
            .unwrap_or_else(|| format!("Sheet{}", sheet_idx + 1));
        self.sheets.push(LegacyXlsPayload {
            sheet_name: name,
            cells: std::mem::take(&mut self.current_cells),
        });
        self.sheet_cell_types
            .push(std::mem::take(&mut self.current_types));
        self.links.end_sheet();
        self.styles.end_sheet();
        self.in_worksheet = false;
    }

    #[allow(clippy::type_complexity)]
    fn finish(
        mut self,
    ) -> (
        Vec<LegacyXlsPayload>,
        Vec<LinkCollector>,
        XlsStyles,
        Vec<SheetState>,
        Vec<FastU64Map<CellValueKind>>,
    ) {
        if self.sheets.is_empty()
            && (!self.current_cells.is_empty() || !self.boundsheet_names.is_empty())
        {
            let name = self
                .boundsheet_names
                .into_iter()
                .next()
                .unwrap_or_else(|| "Sheet1".to_string());
            self.sheets.push(LegacyXlsPayload {
                sheet_name: name,
                cells: self.current_cells,
            });
            self.sheet_cell_types.push(self.current_types);
            self.links.end_sheet();
            self.styles.end_sheet();
        }
        let mut states = self.boundsheet_states;
        states.resize(self.sheets.len(), SheetState::Visible);
        if states.iter().all(|&s| s != SheetState::Visible) && !states.is_empty() {
            states[0] = SheetState::Visible;
        }
        (
            self.sheets,
            self.links.finish(),
            self.styles.finish(),
            states,
            self.sheet_cell_types,
        )
    }
}

impl LegacyOle2Codec {
    fn parse_sst_chunks(chunks: &[&[u8]]) -> Result<Vec<String>, EasyOfficeError> {
        if chunks.is_empty() || chunks[0].len() < 8 {
            return Ok(Vec::new());
        }
        let unique_count = read_u32_le(chunks[0], 4)? as usize;
        let mut cursor = SstContinueCursor::new(chunks, 8);
        let mut sst_strings = Vec::with_capacity(unique_count.min(65_536));

        for _ in 0..unique_count.min(262_144) {
            let Some(char_count_u16) = cursor.read_u16() else {
                break;
            };
            let Some(flags) = cursor.read_u8() else {
                break;
            };
            let char_count = char_count_u16 as usize;
            let rich_runs = if (flags & 0x08) != 0 {
                let Some(r) = cursor.read_u16() else {
                    break;
                };
                r as usize
            } else {
                0
            };
            let ext_rst_len = if (flags & 0x04) != 0 {
                let Some(ext) = cursor.read_u32() else {
                    break;
                };
                ext as usize
            } else {
                0
            };
            let is_utf16 = (flags & 0x01) != 0;
            let s = cursor.read_biff8_chars(char_count, is_utf16)?;
            sst_strings.push(s);
            let skip_extra = rich_runs.saturating_mul(4).saturating_add(ext_rst_len);
            if !cursor.skip_raw_bytes(skip_extra) {
                break;
            }
        }

        Ok(sst_strings)
    }

    pub fn unpack_xls(bytes: &[u8]) -> Result<LegacyXlsPayload, EasyOfficeError> {
        let mut sheets = Self::unpack_xls_multi(bytes)?;
        if sheets.is_empty() {
            Ok(LegacyXlsPayload {
                sheet_name: "Sheet1".to_string(),
                cells: Vec::new(),
            })
        } else {
            Ok(sheets.remove(0))
        }
    }

    pub fn unpack_xls_multi(bytes: &[u8]) -> Result<Vec<LegacyXlsPayload>, EasyOfficeError> {
        Ok(Self::unpack_xls_walk(bytes)?.0)
    }

    /// The sheets of a workbook with the formatting an XLS carries next to them: the hyperlinks
    /// (PD-223), the fill and font colours of the cells (PD-227), number formats (PD-235), cell
    /// value types (PD-241), and which sheets are hidden (PD-209).
    pub fn unpack_xls_with_styling(
        bytes: &[u8],
    ) -> Result<(crate::OdsMultiUnpacked, Vec<(String, SheetStyling)>), EasyOfficeError> {
        let (payloads, collectors, styles, states, cell_types) = Self::unpack_xls_walk(bytes)?;
        let mut sheets = Vec::with_capacity(payloads.len());
        let mut stylings = Vec::with_capacity(payloads.len());
        let (num_fmts, rejected) = (styles.num_fmts, styles.num_fmts_rejected);
        let mut colors = styles.sheets.into_iter();
        let mut states = states.into_iter();
        let mut types = cell_types.into_iter();
        for (index, (payload, links)) in payloads.into_iter().zip(collectors).enumerate() {
            let mut styling = SheetStyling {
                cells: colors.next().unwrap_or_default(),
                cell_types: types.next().unwrap_or_default(),
                num_fmts: std::sync::Arc::clone(&num_fmts),
                num_fmts_rejected: if index == 0 { rejected } else { 0 },
                links: links.links,
                links_rejected: links.rejected,
                ..SheetStyling::default()
            };
            states.next().unwrap_or_default().apply(&mut styling);
            stylings.push((payload.sheet_name.clone(), styling));
            sheets.push((payload.sheet_name, payload.cells));
        }
        Ok((sheets, stylings))
    }

    #[allow(clippy::type_complexity)]
    fn unpack_xls_walk(
        bytes: &[u8],
    ) -> Result<
        (
            Vec<LegacyXlsPayload>,
            Vec<LinkCollector>,
            XlsStyles,
            Vec<SheetState>,
            Vec<FastU64Map<CellValueKind>>,
        ),
        EasyOfficeError,
    > {
        let streams = Ole2ContainerCodec::unpack(bytes)?;
        let wb = streams
            .iter()
            .find(|s| s.name == "Workbook" || s.name == "Book")
            .ok_or_else(|| {
                EasyOfficeError::CorruptedContainer(
                    "BIFF8 필수 스트림 'Workbook'이 누락되었습니다.".to_string(),
                )
            })?;
        let data = wb.data.as_slice();
        if data.len() < 4 || read_u16_le(data, 0)? != Self::BIFF8_BOF {
            return Err(EasyOfficeError::CorruptedContainer(
                "BIFF8 BOF(0x0809) 시그니처 불일치".to_string(),
            ));
        }

        let mut state = XlsParseState::default();
        let mut offset = 0usize;
        while offset + 4 <= data.len() {
            let opcode = read_u16_le(data, offset)?;
            let rec_len = read_u16_le(data, offset + 2)? as usize;
            let start = offset + 4;
            let end = start.checked_add(rec_len).ok_or_else(|| {
                EasyOfficeError::CorruptedContainer("BIFF8 레코드 길이 오버플로".to_string())
            })?;
            if end > data.len() {
                return Err(EasyOfficeError::CorruptedContainer(
                    "BIFF8 레코드가 스트림 경계를 초과합니다.".to_string(),
                ));
            }
            let rec = XlsRecord { start, end };
            offset = Self::apply_xls_record(&mut state, data, opcode, rec)?;
        }
        Ok(state.finish())
    }

    /// Applies one record to `state`; returns the offset of the next record.
    fn apply_xls_record(
        state: &mut XlsParseState,
        data: &[u8],
        opcode: u16,
        rec: XlsRecord,
    ) -> Result<usize, EasyOfficeError> {
        if state.in_worksheet {
            state.links.before(opcode);
        }
        state
            .styles
            .record(opcode, &data[rec.start..rec.end], state.in_worksheet);
        match opcode {
            Self::BIFF8_FILEPASS => {
                return Err(EasyOfficeError::EncryptedDocumentUnsupported(
                    DocumentFormat::Xls,
                ));
            }
            Self::BIFF8_BOF => Self::xls_bof(state, data, rec)?,
            Self::BIFF8_BOUNDSHEET => Self::xls_boundsheet(state, data, rec)?,
            Self::BIFF8_SST => return Self::xls_sst(state, data, rec),
            Self::BIFF8_LABELSST => Self::xls_labelsst(state, data, rec)?,
            Self::BIFF8_NUMBER => Self::xls_number(state, data, rec)?,
            Self::BIFF8_RK => Self::xls_rk(state, data, rec)?,
            Self::BIFF8_MULRK => Self::xls_mulrk(state, data, rec)?,
            Self::BIFF8_FORMULA => Self::xls_formula(state, data, rec)?,
            Self::BIFF8_STRING => Self::xls_string(state, data, rec)?,
            Self::BIFF8_LABEL => Self::xls_label(state, data, rec)?,
            Self::BIFF8_BOOLERR => Self::xls_boolerr(state, data, rec)?,
            xls_links::HLINK if state.in_worksheet => {
                state.links.hlink(&data[rec.start..rec.end]);
            }
            xls_links::HLINK_TOOLTIP if state.in_worksheet => {
                state.links.tooltip(&data[rec.start..rec.end]);
            }
            Self::BIFF8_EOF => state.end_substream(),
            _ => {}
        }
        Ok(rec.end)
    }

    fn xls_boolerr(
        state: &mut XlsParseState,
        data: &[u8],
        rec: XlsRecord,
    ) -> Result<(), EasyOfficeError> {
        if rec.len() < 8 {
            return Ok(());
        }
        let start = rec.start;
        let row = read_u16_le(data, start)? as u32;
        let col = read_u16_le(data, start + 2)? as u32;
        // MS-XLS §2.4.24 `BoolErr`: byte 6 is `bBoolErr`, byte 7 is `fError` (0 = Bool, 1 = Error).
        let b_val = data[start + 6];
        let f_err = data[start + 7];
        let (text, kind) = if f_err == 0 {
            (
                if b_val == 0 { "FALSE" } else { "TRUE" },
                CellValueKind::Boolean,
            )
        } else if f_err == 1 {
            (biff8_error_str(b_val), CellValueKind::Error)
        } else if b_val == 1 {
            (biff8_error_str(f_err), CellValueKind::Error)
        } else {
            ("TRUE", CellValueKind::Boolean)
        };
        state.push_typed_cell(row, col, text.to_string(), kind)
    }

    fn xls_bof(
        state: &mut XlsParseState,
        data: &[u8],
        rec: XlsRecord,
    ) -> Result<(), EasyOfficeError> {
        if rec.len() >= 4 && read_u16_le(data, rec.start + 2)? == XLS_SUBSTREAM_WORKSHEET {
            state.in_worksheet = true;
            state.current_cells.clear();
            state.current_types.clear();
            state.last_formula_cell_idx = None;
        }
        Ok(())
    }

    fn xls_boundsheet(
        state: &mut XlsParseState,
        data: &[u8],
        rec: XlsRecord,
    ) -> Result<(), EasyOfficeError> {
        let XlsRecord { start, end } = rec;
        if rec.len() < 8 || !xls_boundsheet::is_worksheet(&data[start..end]) {
            return Ok(());
        }
        let cch_u8 = data[start + 6] as usize;
        let grbit = data[start + 7];
        let is_utf16 = (grbit & 0x01) != 0;
        let std_byte_len = if is_utf16 { cch_u8 * 2 } else { cch_u8 };
        let parsed_name = if (grbit & 0xFE) == 0 && start + 8 + std_byte_len == end {
            Some(Self::decode_biff_text(&data[start + 8..end], is_utf16)?)
        } else {
            let byte_len = read_u16_le(data, start + 6)? as usize * 2;
            if start + 8 + byte_len <= end {
                Some(decode_utf16le(&data[start + 8..start + 8 + byte_len])?)
            } else {
                None
            }
        };
        if parsed_name.is_some() {
            state
                .boundsheet_states
                .push(xls_boundsheet::state_of(&data[start..end]));
        }
        state.boundsheet_names.extend(parsed_name);
        Ok(())
    }

    /// Decodes a BIFF string body as UTF-16LE or Latin-1.
    fn decode_biff_text(bytes: &[u8], is_utf16: bool) -> Result<String, EasyOfficeError> {
        if is_utf16 {
            decode_utf16le(bytes)
        } else {
            Ok(Self::decode_latin1_bytes(bytes))
        }
    }

    /// Parses the SST record together with its CONTINUE records; returns the next offset.
    fn xls_sst(
        state: &mut XlsParseState,
        data: &[u8],
        rec: XlsRecord,
    ) -> Result<usize, EasyOfficeError> {
        let mut sst_chunks: Vec<&[u8]> = vec![&data[rec.start..rec.end]];
        let mut next_off = rec.end;
        while next_off + 4 <= data.len() {
            if read_u16_le(data, next_off)? != Self::BIFF8_CONTINUE {
                break;
            }
            let next_len = read_u16_le(data, next_off + 2)? as usize;
            let c_start = next_off + 4;
            let c_end = c_start.checked_add(next_len).ok_or_else(|| {
                EasyOfficeError::CorruptedContainer("BIFF8 CONTINUE 레코드 오버플로".to_string())
            })?;
            if c_end > data.len() {
                return Err(EasyOfficeError::CorruptedContainer(
                    "BIFF8 CONTINUE 레코드 경계 초과".to_string(),
                ));
            }
            sst_chunks.push(&data[c_start..c_end]);
            next_off = c_end;
        }
        state.sst_strings = Self::parse_sst_chunks(&sst_chunks)?;
        Ok(next_off)
    }

    fn xls_labelsst(
        state: &mut XlsParseState,
        data: &[u8],
        rec: XlsRecord,
    ) -> Result<(), EasyOfficeError> {
        if rec.len() < 10 {
            return Ok(());
        }
        let start = rec.start;
        let row = read_u16_le(data, start)? as u32;
        let col = read_u16_le(data, start + 2)? as u32;
        let sst_idx = read_u32_le(data, start + 6)? as usize;
        let text = state.sst_strings.get(sst_idx).cloned().ok_or_else(|| {
            EasyOfficeError::CorruptedContainer(
                "BIFF8 LABELSST 인덱스가 SST 범위를 벗어났습니다.".to_string(),
            )
        })?;
        // One record can name a string of up to 64 Ki characters: pay for the copy before it.
        state.push_typed_cell(row, col, text, CellValueKind::Text)
    }

    fn xls_number(
        state: &mut XlsParseState,
        data: &[u8],
        rec: XlsRecord,
    ) -> Result<(), EasyOfficeError> {
        if rec.len() != 14 {
            return Ok(());
        }
        let start = rec.start;
        let row = read_u16_le(data, start)? as u32;
        let col = read_u16_le(data, start + 2)? as u32;
        let num = f64::from_bits(read_u64_le(data, start + 6)?);
        state.push_typed_cell(
            row,
            col,
            Self::format_biff_number(num),
            CellValueKind::Number,
        )
    }

    fn xls_rk(
        state: &mut XlsParseState,
        data: &[u8],
        rec: XlsRecord,
    ) -> Result<(), EasyOfficeError> {
        if rec.len() < 10 {
            return Ok(());
        }
        let start = rec.start;
        let row = read_u16_le(data, start)? as u32;
        let col = read_u16_le(data, start + 2)? as u32;
        let num = Self::decode_rk_number(read_u32_le(data, start + 6)?);
        state.push_typed_cell(
            row,
            col,
            Self::format_biff_number(num),
            CellValueKind::Number,
        )
    }

    fn xls_mulrk(
        state: &mut XlsParseState,
        data: &[u8],
        rec: XlsRecord,
    ) -> Result<(), EasyOfficeError> {
        if rec.len() < 12 {
            return Ok(());
        }
        let start = rec.start;
        let row = read_u16_le(data, start)? as u32;
        let first_col = read_u16_le(data, start + 2)? as u32;
        let count = (rec.len() - 6) / 6;
        for i in 0..count {
            let rk = read_u32_le(data, start + 4 + i * 6 + 2)?;
            let num = Self::decode_rk_number(rk);
            state.push_typed_cell(
                row,
                first_col + i as u32,
                Self::format_biff_number(num),
                CellValueKind::Number,
            )?;
        }
        Ok(())
    }

    /// Extracts the inline `=expr` text that older versions of this writer stored after a FORMULA
    /// record in place of tokens, when present.
    fn xls_formula_inline(data: &[u8], rec: XlsRecord) -> Result<Option<String>, EasyOfficeError> {
        let XlsRecord { start, end } = rec;
        if rec.len() <= 16 {
            return Ok(None);
        }
        let char_count = read_u16_le(data, start + 14)? as usize;
        let byte_len = char_count * 2;
        let is_legacy_app_inline_text = char_count == 0
            && (end - (start + 16)) % 2 == 0
            && end >= start + 18
            && data[start + 16] == b'='
            && data[start + 17] == 0;
        let has_counted_text = char_count > 0 && start + 16 + byte_len == end;
        if !(has_counted_text || is_legacy_app_inline_text) {
            return Ok(None);
        }
        let slice = if char_count > 0 {
            &data[start + 16..start + 16 + byte_len]
        } else {
            &data[start + 16..end]
        };
        let Ok(expr_raw) = decode_utf16le(slice) else {
            return Ok(None);
        };
        let expr = expr_raw.trim_matches('\0');
        if expr.is_empty() || expr.chars().any(char::is_control) {
            return Ok(None);
        }
        Ok(Some(if expr.starts_with('=') {
            expr.to_string()
        } else {
            format!("={expr}")
        }))
    }

    /// Cached result of a FORMULA record as `(display text, value kind)`.
    fn xls_formula_cached(
        data: &[u8],
        start: usize,
    ) -> Result<(String, CellValueKind), EasyOfficeError> {
        if data[start + 12] == 0xFF && data[start + 13] == 0xFF {
            return Ok(match data[start + 6] {
                1 => (
                    if data[start + 8] != 0 {
                        "TRUE".to_string()
                    } else {
                        "FALSE".to_string()
                    },
                    CellValueKind::Boolean,
                ),
                2 => (
                    biff8_error_str(data[start + 8]).to_string(),
                    CellValueKind::Error,
                ),
                _ => (String::new(), CellValueKind::Text),
            });
        }
        let num = f64::from_bits(read_u64_le(data, start + 6)?);
        Ok(if num.is_finite() {
            (Self::format_biff_number(num), CellValueKind::Number)
        } else {
            (String::new(), CellValueKind::Text)
        })
    }

    fn xls_formula(
        state: &mut XlsParseState,
        data: &[u8],
        rec: XlsRecord,
    ) -> Result<(), EasyOfficeError> {
        if rec.len() < 14 {
            return Ok(());
        }
        let start = rec.start;
        let row = read_u16_le(data, start)? as u32;
        let col = read_u16_le(data, start + 2)? as u32;
        let has_tokens = rec.len() >= FORMULA_FIXED_LEN
            && rec.len() == FORMULA_FIXED_LEN + read_u16_le(data, start + 20)? as usize;
        let expression = if has_tokens {
            formula_text(&data[start + FORMULA_FIXED_LEN..rec.end])
        } else {
            Self::xls_formula_inline(data, rec)?
        };
        let (text, kind) = match expression {
            Some(expression) => (expression, CellValueKind::Formula),
            None => Self::xls_formula_cached(data, start)?,
        };
        state.budget.charge_cell(text.len(), XLS_BUDGET_WHAT)?;
        state.record_kind(row, col, &text, kind);
        state.current_cells.push((row, col, text));
        state.last_formula_cell_idx = Some(state.current_cells.len() - 1);
        Ok(())
    }

    /// STRING record: the text result of the preceding FORMULA cell.
    fn xls_string(
        state: &mut XlsParseState,
        data: &[u8],
        rec: XlsRecord,
    ) -> Result<(), EasyOfficeError> {
        if rec.len() < 3 {
            return Ok(());
        }
        let start = rec.start;
        let cch = read_u16_le(data, start)? as usize;
        let is_utf16 = (data[start + 2] & 0x01) != 0;
        let byte_len = if is_utf16 { cch * 2 } else { cch };
        if start + 3 + byte_len > rec.end {
            return Ok(());
        }
        let s = Self::decode_biff_text(&data[start + 3..start + 3 + byte_len], is_utf16)?;
        let Some(idx) = state.last_formula_cell_idx.take() else {
            return Ok(());
        };
        let Some(&(row, col, ref cur)) = state.current_cells.get(idx) else {
            return Ok(());
        };
        if !cur.starts_with('=') {
            state.budget.charge(s.len(), XLS_BUDGET_WHAT)?;
            state.record_kind(row, col, &s, CellValueKind::Text);
            state.current_cells[idx].2 = s;
        }
        Ok(())
    }

    fn xls_label(
        state: &mut XlsParseState,
        data: &[u8],
        rec: XlsRecord,
    ) -> Result<(), EasyOfficeError> {
        if rec.len() < 8 {
            return Ok(());
        }
        let XlsRecord { start, end } = rec;
        let row = read_u16_le(data, start)? as u32;
        let col = read_u16_le(data, start + 2)? as u32;
        let cch = read_u16_le(data, start + 6)? as usize;
        if rec.len() >= 9 && (data[start + 8] & 0xFE) == 0 {
            let is_utf16 = (data[start + 8] & 0x01) != 0;
            let std_len = if is_utf16 { cch * 2 } else { cch };
            if start + 9 + std_len == end {
                let s = Self::decode_biff_text(&data[start + 9..end], is_utf16)?;
                state.push_typed_cell(row, col, s, CellValueKind::Text)?;
                return Ok(());
            }
        }
        let byte_len = cch * 2;
        if start + 8 + byte_len <= end {
            let s = decode_utf16le(&data[start + 8..start + 8 + byte_len])?;
            state.push_typed_cell(row, col, s, CellValueKind::Text)?;
        }
        Ok(())
    }
}

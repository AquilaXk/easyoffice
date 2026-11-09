use crate::ole2_codec::{
    decode_utf16le, encode_utf16le, read_u16_le, read_u32_le, read_u64_le, write_u32_le,
    Ole2ContainerCodec,
};
use crate::{DocumentFormat, EasyOfficeError, LegacyOle2Codec};

/// Legacy `.xls` BIFF8 spreadsheet payload model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyXlsPayload {
    pub sheet_name: String,
    pub cells: Vec<(u32, u32, String)>,
}

struct SstContinueCursor<'a> {
    chunks: &'a [&'a [u8]],
    chunk_idx: usize,
    pos: usize,
}

impl<'a> SstContinueCursor<'a> {
    fn new(chunks: &'a [&'a [u8]], initial_pos: usize) -> Self {
        Self {
            chunks,
            chunk_idx: 0,
            pos: initial_pos,
        }
    }

    fn advance_if_exhausted(&mut self) -> bool {
        while self.chunk_idx < self.chunks.len() && self.pos >= self.chunks[self.chunk_idx].len() {
            self.chunk_idx += 1;
            self.pos = 0;
        }
        self.chunk_idx < self.chunks.len()
    }

    fn read_raw_bytes(&mut self, mut n: usize) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(n);
        while n > 0 {
            if !self.advance_if_exhausted() {
                return None;
            }
            let chunk = self.chunks[self.chunk_idx];
            let avail = (chunk.len() - self.pos).min(n);
            out.extend_from_slice(&chunk[self.pos..self.pos + avail]);
            self.pos += avail;
            n -= avail;
        }
        Some(out)
    }

    fn skip_raw_bytes(&mut self, mut n: usize) -> bool {
        while n > 0 {
            if !self.advance_if_exhausted() {
                return false;
            }
            let chunk = self.chunks[self.chunk_idx];
            let avail = (chunk.len() - self.pos).min(n);
            self.pos += avail;
            n -= avail;
        }
        true
    }

    fn read_u8(&mut self) -> Option<u8> {
        if !self.advance_if_exhausted() {
            return None;
        }
        let b = self.chunks[self.chunk_idx][self.pos];
        self.pos += 1;
        Some(b)
    }

    fn read_u16(&mut self) -> Option<u16> {
        let b = self.read_raw_bytes(2)?;
        Some(u16::from_le_bytes([b[0], b[1]]))
    }

    fn read_u32(&mut self) -> Option<u32> {
        let b = self.read_raw_bytes(4)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read_biff8_chars(
        &mut self,
        mut rem_chars: usize,
        mut is_utf16: bool,
    ) -> Result<String, EasyOfficeError> {
        let mut out = String::with_capacity(rem_chars);
        let mut first_segment = true;
        while rem_chars > 0 {
            if !self.advance_if_exhausted() {
                return Err(EasyOfficeError::CorruptedContainer(
                    "BIFF8 SST 문자열 데이터가 조기 종료되었습니다.".to_string(),
                ));
            }
            if !first_segment && self.pos == 0 {
                let cont_grbit = self.read_u8().ok_or_else(|| {
                    EasyOfficeError::CorruptedContainer(
                        "BIFF8 SST CONTINUE 플래그 바이트 누락".to_string(),
                    )
                })?;
                is_utf16 = (cont_grbit & 0x01) != 0;
            }
            first_segment = false;
            let chunk = self.chunks[self.chunk_idx];
            let avail_bytes = chunk.len().saturating_sub(self.pos);
            if is_utf16 {
                let take_chars = (avail_bytes / 2).min(rem_chars);
                if take_chars == 0 {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "BIFF8 SST UTF-16LE 코드 유닛이 레코드 경계에서 홀수 바이트로 잘렸습니다."
                            .to_string(),
                    ));
                }
                let take_bytes = take_chars * 2;
                let s = decode_utf16le(&chunk[self.pos..self.pos + take_bytes])?;
                out.push_str(&s);
                self.pos += take_bytes;
                rem_chars -= take_chars;
            } else {
                let take_chars = avail_bytes.min(rem_chars);
                if take_chars == 0 {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "BIFF8 SST 8비트 문자열 세그먼트가 비어 있습니다.".to_string(),
                    ));
                }
                for &b in &chunk[self.pos..self.pos + take_chars] {
                    out.push(b as char);
                }
                self.pos += take_chars;
                rem_chars -= take_chars;
            }
        }
        Ok(out)
    }
}

impl LegacyOle2Codec {
    pub const BIFF8_BOF: u16 = 0x0809;
    pub const BIFF8_EOF: u16 = 0x000A;
    pub const BIFF8_FILEPASS: u16 = 0x002F;
    pub const BIFF8_FONT: u16 = 0x0031;
    pub const BIFF8_CONTINUE: u16 = 0x003C;
    pub const BIFF8_BOUNDSHEET: u16 = 0x0085;
    pub const BIFF8_MULRK: u16 = 0x00BD;
    pub const BIFF8_XF: u16 = 0x00E0;
    pub const BIFF8_SST: u16 = 0x00FC;
    pub const BIFF8_LABELSST: u16 = 0x00FD;
    pub const BIFF8_DIMENSIONS: u16 = 0x0200;
    pub const BIFF8_NUMBER: u16 = 0x0203;
    pub const BIFF8_LABEL: u16 = 0x0204;
    pub const BIFF8_BOOLERR: u16 = 0x0205;
    pub const BIFF8_STRING: u16 = 0x0207;
    pub const BIFF8_RK: u16 = 0x027E;
    pub const BIFF8_FORMULA: u16 = 0x0006;

    #[inline]
    pub(crate) fn parse_finite_f64(s: &str) -> Option<f64> {
        let first = *s.as_bytes().first()?;
        if !(first.is_ascii_digit() || matches!(first, b'-' | b'+' | b'.')) {
            return None;
        }
        s.parse::<f64>().ok().filter(|n| n.is_finite())
    }

    #[inline]
    fn biff_error_str(code: u8) -> &'static str {
        match code {
            0x00 => "#NULL!",
            0x07 => "#DIV/0!",
            0x0F => "#VALUE!",
            0x17 => "#REF!",
            0x1D => "#NAME?",
            0x24 => "#NUM!",
            0x2A => "#N/A",
            _ => "#ERROR!",
        }
    }

    #[inline]
    pub(crate) fn decode_latin1_bytes(slice: &[u8]) -> String {
        slice.iter().map(|&b| b as char).collect()
    }

    #[inline]
    pub(crate) fn format_biff_number(num: f64) -> String {
        if num.fract() == 0.0 && num.abs() < 1e15 {
            format!("{num:.0}")
        } else {
            format!("{num}")
        }
    }

    #[inline]
    pub(crate) fn decode_rk_number(rk: u32) -> f64 {
        let base = if (rk & 0x02) != 0 {
            ((rk as i32) >> 2) as f64
        } else {
            f64::from_bits(((rk & !3) as u64) << 32)
        };
        if (rk & 0x01) != 0 {
            base / 100.0
        } else {
            base
        }
    }

    fn emit_sst_with_continue(stream: &mut Vec<u8>, sst_strings: &[String]) {
        let max_rec = crate::MAX_BIFF8_RECORD_DATA_LEN;
        let mut current_opcode = Self::BIFF8_SST;
        let mut current_buf = Vec::with_capacity(max_rec.min(1024));
        let total_count = sst_strings.len() as u32;
        current_buf.extend_from_slice(&total_count.to_le_bytes());
        current_buf.extend_from_slice(&total_count.to_le_bytes());

        let flush_record = |out: &mut Vec<u8>, opcode: u16, buf: &mut Vec<u8>| {
            out.extend_from_slice(&opcode.to_le_bytes());
            out.extend_from_slice(&(buf.len() as u16).to_le_bytes());
            out.extend_from_slice(buf);
            buf.clear();
        };

        for s in sst_strings {
            let units: Vec<u16> = s.encode_utf16().collect();
            let char_count = units.len() as u16;
            if current_buf.len() + 5 > max_rec {
                flush_record(stream, current_opcode, &mut current_buf);
                current_opcode = Self::BIFF8_CONTINUE;
            }
            current_buf.extend_from_slice(&char_count.to_le_bytes());
            current_buf.push(0x01); // UTF-16LE
            let mut unit_idx = 0usize;
            while unit_idx < units.len() {
                let avail_bytes = max_rec.saturating_sub(current_buf.len());
                let avail_units = avail_bytes / 2;
                if avail_units == 0 {
                    flush_record(stream, current_opcode, &mut current_buf);
                    current_opcode = Self::BIFF8_CONTINUE;
                    current_buf.push(0x01); // continuation grbit = UTF-16LE
                    continue;
                }
                let take = avail_units.min(units.len() - unit_idx);
                for &u in &units[unit_idx..unit_idx + take] {
                    current_buf.extend_from_slice(&u.to_le_bytes());
                }
                unit_idx += take;
            }
        }

        if !current_buf.is_empty() {
            flush_record(stream, current_opcode, &mut current_buf);
        }
    }

    pub fn pack_xls(payload: &LegacyXlsPayload) -> Result<Vec<u8>, EasyOfficeError> {
        Self::pack_xls_multi(std::slice::from_ref(payload))
    }

    pub fn pack_xls_multi(sheets: &[LegacyXlsPayload]) -> Result<Vec<u8>, EasyOfficeError> {
        Self::pack_xls_multi_inner(sheets, true)
    }

    pub fn pack_xls_multi_values(sheets: &[LegacyXlsPayload]) -> Result<Vec<u8>, EasyOfficeError> {
        Self::pack_xls_multi_inner(sheets, false)
    }

    fn pack_xls_multi_inner(
        sheets: &[LegacyXlsPayload],
        allow_formulas: bool,
    ) -> Result<Vec<u8>, EasyOfficeError> {
        if sheets.is_empty() {
            return Err(EasyOfficeError::EmptyPayload);
        }
        for sheet in sheets {
            for &(row, col, ref val) in &sheet.cells {
                if row > crate::MAX_BIFF8_ROW || col > crate::MAX_BIFF8_COL {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                        "BIFF8 셀 좌표(row={row}, col={col})가 명세 상한(row<={}, col<={})을 초과했습니다.",
                        crate::MAX_BIFF8_ROW,
                        crate::MAX_BIFF8_COL
                    )));
                }
                if val.encode_utf16().count() > 32_765 {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "BIFF8 셀 문자열 길이가 32,765 UTF-16 코드 유닛 한도를 초과했습니다."
                            .to_string(),
                    ));
                }
            }
        }

        let mut stream = Vec::with_capacity(512);

        // 1. Workbook Globals Substream BOF (0x0809, len 16, substream 0x0005)
        stream.extend_from_slice(&Self::BIFF8_BOF.to_le_bytes());
        stream.extend_from_slice(&16u16.to_le_bytes());
        stream.extend_from_slice(&0x0600u16.to_le_bytes()); // BIFF8
        stream.extend_from_slice(&0x0005u16.to_le_bytes()); // Workbook Globals
        stream.extend_from_slice(&0x0DBBu16.to_le_bytes());
        stream.extend_from_slice(&0x07CCu16.to_le_bytes());
        stream.extend_from_slice(&0u32.to_le_bytes());
        stream.extend_from_slice(&0x0006u32.to_le_bytes());

        // Minimal FONT (0x0031) and XF (0x00E0) records for BIFF8 spec compliance
        let font_name = b"Arial";
        let font_rec_len = (16 + font_name.len()) as u16;
        stream.extend_from_slice(&Self::BIFF8_FONT.to_le_bytes());
        stream.extend_from_slice(&font_rec_len.to_le_bytes());
        stream.extend_from_slice(&200u16.to_le_bytes()); // dyHeight = 10pt
        stream.extend_from_slice(&0u16.to_le_bytes()); // grbit
        stream.extend_from_slice(&0x7FFFu16.to_le_bytes()); // icv
        stream.extend_from_slice(&400u16.to_le_bytes()); // bls = normal
        stream.extend_from_slice(&0u16.to_le_bytes()); // sss
        stream.push(0); // uls
        stream.push(0); // bFamily
        stream.push(0); // bCharSet
        stream.push(0); // reserved
        stream.push(font_name.len() as u8);
        stream.push(0); // compressed 8-bit Latin-1
        stream.extend_from_slice(font_name);

        stream.extend_from_slice(&Self::BIFF8_XF.to_le_bytes());
        stream.extend_from_slice(&20u16.to_le_bytes());
        stream.extend_from_slice(&[0u8; 20]);

        // 2. BOUNDSHEET records (0x0085)
        let mut lb_ply_pos_offsets = Vec::with_capacity(sheets.len());
        for sheet in sheets {
            let sheet_units: Vec<u16> = sheet.sheet_name.encode_utf16().take(255).collect();
            let mut sheet_name_utf16 = Vec::with_capacity(sheet_units.len() * 2);
            for u in &sheet_units {
                sheet_name_utf16.extend_from_slice(&u.to_le_bytes());
            }
            let bs_len = (8 + sheet_name_utf16.len()) as u16;
            stream.extend_from_slice(&Self::BIFF8_BOUNDSHEET.to_le_bytes());
            stream.extend_from_slice(&bs_len.to_le_bytes());
            lb_ply_pos_offsets.push(stream.len());
            stream.extend_from_slice(&0u32.to_le_bytes()); // patched to Worksheet BOF offset
            stream.push(0); // hsState = visible
            stream.push(0); // dt = worksheet
            stream.push(sheet_units.len() as u8); // 1-byte cch
            stream.push(0x01); // grbit = 1 (UTF-16LE)
            stream.extend_from_slice(&sheet_name_utf16);
        }

        let mut sst_strings: Vec<String> = Vec::new();
        let mut sst_index_map: std::collections::HashMap<&str, u32> =
            std::collections::HashMap::new();
        for sheet in sheets {
            for (_, _, val) in &sheet.cells {
                let is_formula = allow_formulas && val.starts_with('=');
                if !is_formula
                    && Self::parse_finite_f64(val).is_none()
                    && !sst_index_map.contains_key(val.as_str())
                {
                    let idx = sst_strings.len() as u32;
                    sst_index_map.insert(val.as_str(), idx);
                    sst_strings.push(val.clone());
                }
            }
        }

        if !sst_strings.is_empty() {
            Self::emit_sst_with_continue(&mut stream, &sst_strings);
        }

        // End of Workbook Globals substream
        stream.extend_from_slice(&Self::BIFF8_EOF.to_le_bytes());
        stream.extend_from_slice(&0u16.to_le_bytes());

        // 3. Worksheet Substreams
        for (sheet_idx, sheet) in sheets.iter().enumerate() {
            let worksheet_bof_pos = stream.len() as u32;
            write_u32_le(
                &mut stream,
                lb_ply_pos_offsets[sheet_idx],
                worksheet_bof_pos,
            );

            stream.extend_from_slice(&Self::BIFF8_BOF.to_le_bytes());
            stream.extend_from_slice(&16u16.to_le_bytes());
            stream.extend_from_slice(&0x0600u16.to_le_bytes()); // BIFF8
            stream.extend_from_slice(&0x0010u16.to_le_bytes()); // Worksheet substream
            stream.extend_from_slice(&0x0DBBu16.to_le_bytes());
            stream.extend_from_slice(&0x07CCu16.to_le_bytes());
            stream.extend_from_slice(&0u32.to_le_bytes());
            stream.extend_from_slice(&0x0006u32.to_le_bytes());

            let (max_r, max_c) = sheet
                .cells
                .iter()
                .fold((0u32, 0u32), |(mr, mc), &(r, c, _)| {
                    (mr.max(r + 1), mc.max(c + 1))
                });
            stream.extend_from_slice(&Self::BIFF8_DIMENSIONS.to_le_bytes());
            stream.extend_from_slice(&14u16.to_le_bytes());
            stream.extend_from_slice(&0u32.to_le_bytes()); // first_row
            stream.extend_from_slice(&max_r.to_le_bytes()); // last_row + 1
            stream.extend_from_slice(&0u16.to_le_bytes()); // first_col
            stream.extend_from_slice(&(max_c.min(u16::MAX as u32) as u16).to_le_bytes()); // last_col + 1
            stream.extend_from_slice(&0u16.to_le_bytes()); // reserved

            for (row, col, raw) in &sheet.cells {
                let r16 = *row as u16;
                let c16 = *col as u16;
                if let Some(expr_str) = raw.strip_prefix('=').filter(|_| allow_formulas) {
                    let expr_utf16 = encode_utf16le(expr_str);
                    let rec_len = (16 + expr_utf16.len()) as u16;
                    stream.extend_from_slice(&Self::BIFF8_FORMULA.to_le_bytes());
                    stream.extend_from_slice(&rec_len.to_le_bytes());
                    stream.extend_from_slice(&r16.to_le_bytes());
                    stream.extend_from_slice(&c16.to_le_bytes());
                    stream.extend_from_slice(&0u16.to_le_bytes()); // xf
                    stream.extend_from_slice(&0.0f64.to_bits().to_le_bytes());
                    stream
                        .extend_from_slice(&(expr_str.encode_utf16().count() as u16).to_le_bytes());
                    stream.extend_from_slice(&expr_utf16);
                } else if let Some(num) = Self::parse_finite_f64(raw) {
                    stream.extend_from_slice(&Self::BIFF8_NUMBER.to_le_bytes());
                    stream.extend_from_slice(&14u16.to_le_bytes());
                    stream.extend_from_slice(&r16.to_le_bytes());
                    stream.extend_from_slice(&c16.to_le_bytes());
                    stream.extend_from_slice(&0u16.to_le_bytes()); // xf
                    stream.extend_from_slice(&num.to_bits().to_le_bytes());
                } else {
                    let sst_idx = sst_index_map.get(raw.as_str()).copied().unwrap_or(0);
                    stream.extend_from_slice(&Self::BIFF8_LABELSST.to_le_bytes());
                    stream.extend_from_slice(&10u16.to_le_bytes());
                    stream.extend_from_slice(&r16.to_le_bytes());
                    stream.extend_from_slice(&c16.to_le_bytes());
                    stream.extend_from_slice(&0u16.to_le_bytes()); // xf
                    stream.extend_from_slice(&sst_idx.to_le_bytes());
                }
            }

            stream.extend_from_slice(&Self::BIFF8_EOF.to_le_bytes());
            stream.extend_from_slice(&0u16.to_le_bytes());
        }

        Ole2ContainerCodec::pack(&[("Workbook".to_string(), stream)])
    }

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
        let b6 = data[start + 6];
        let b7 = data[start + 7];
        let text = match (b6, b7) {
            (0, 0) => "FALSE",
            (0, _) => "TRUE",
            (1, code) => Self::biff_error_str(code),
            (code, 1) => Self::biff_error_str(code),
            _ => "TRUE",
        };
        state.push_cell(row, col, text.to_string());
        Ok(())
    }

    fn xls_bof(
        state: &mut XlsParseState,
        data: &[u8],
        rec: XlsRecord,
    ) -> Result<(), EasyOfficeError> {
        if rec.len() >= 4 && read_u16_le(data, rec.start + 2)? == XLS_SUBSTREAM_WORKSHEET {
            state.in_worksheet = true;
            state.current_cells.clear();
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
        if rec.len() < 8 {
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
        state.push_cell(row, col, text);
        Ok(())
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
        state.push_cell(row, col, Self::format_biff_number(num));
        Ok(())
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
        state.push_cell(row, col, Self::format_biff_number(num));
        Ok(())
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
            state
                .current_cells
                .push((row, first_col + i as u32, Self::format_biff_number(num)));
        }
        state.last_formula_cell_idx = None;
        Ok(())
    }

    /// Extracts the inline `=expr` text stored after a FORMULA record, when present.
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

    /// Cached result of a FORMULA record as display text.
    fn xls_formula_cached(data: &[u8], start: usize) -> Result<String, EasyOfficeError> {
        if data[start + 12] == 0xFF && data[start + 13] == 0xFF {
            return Ok(match data[start + 6] {
                1 => {
                    if data[start + 8] != 0 {
                        "TRUE".to_string()
                    } else {
                        "FALSE".to_string()
                    }
                }
                2 => Self::biff_error_str(data[start + 8]).to_string(),
                _ => String::new(),
            });
        }
        let num = f64::from_bits(read_u64_le(data, start + 6)?);
        Ok(if num.is_finite() {
            Self::format_biff_number(num)
        } else {
            String::new()
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
        let text = match Self::xls_formula_inline(data, rec)? {
            Some(inline) => inline,
            None => Self::xls_formula_cached(data, start)?,
        };
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
        if let Some(entry) = state
            .last_formula_cell_idx
            .take()
            .and_then(|idx| state.current_cells.get_mut(idx))
        {
            if !entry.2.starts_with('=') {
                entry.2 = s;
            }
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
                state.push_cell(row, col, s);
                return Ok(());
            }
        }
        let byte_len = cch * 2;
        if start + 8 + byte_len <= end {
            let s = decode_utf16le(&data[start + 8..start + 8 + byte_len])?;
            state.push_cell(row, col, s);
        }
        Ok(())
    }
}

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
    sst_strings: Vec<String>,
    sheets: Vec<LegacyXlsPayload>,
    current_cells: Vec<(u32, u32, String)>,
    in_worksheet: bool,
    last_formula_cell_idx: Option<usize>,
}

impl XlsParseState {
    fn push_cell(&mut self, row: u32, col: u32, text: String) {
        self.current_cells.push((row, col, text));
        self.last_formula_cell_idx = None;
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
        self.in_worksheet = false;
    }

    fn finish(mut self) -> Vec<LegacyXlsPayload> {
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
        }
        self.sheets
    }
}

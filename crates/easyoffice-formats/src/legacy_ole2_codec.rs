use crate::ole2_codec::{
    append_len_prefixed_utf16le, decode_utf16le, encode_utf16le, read_len_prefixed_utf16le,
    read_u16_le, read_u32_le, read_u64_le, write_u16_le, write_u32_le, Ole2ContainerCodec,
};
use crate::{DocumentFormat, EasyOfficeError};

/// Legacy `.doc` payload model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyDocPayload {
    pub title: String,
    pub paragraphs: Vec<String>,
    pub tables: Vec<Vec<Vec<String>>>,
}

/// Legacy `.xls` BIFF8 spreadsheet payload model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyXlsPayload {
    pub sheet_name: String,
    pub cells: Vec<(u32, u32, String)>,
}

/// Slide record inside a legacy `.ppt` presentation stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyPptSlide {
    pub title: String,
    pub bullets: Vec<String>,
    pub notes: String,
    pub theme: String,
    pub transition: String,
}

/// Legacy `.ppt` presentation payload model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyPptPayload {
    pub title: String,
    pub slides: Vec<LegacyPptSlide>,
}

/// Legacy OLE2 binary codecs for `.doc`, `.xls` (BIFF8), and `.ppt`.
pub struct LegacyOle2Codec;

impl LegacyOle2Codec {
    pub const DOC_FIB_MAGIC: u16 = 0xA5EC;
    pub const DOC_FIB_VERSION: u16 = 0x00C1;
    pub const DOC_FLAG_ENCRYPTED: u16 = 0x0100;
    pub const DOC_TRAILER_MAGIC: u32 = 0x454F_4443;
    pub const DOC_FIB_MARKER: u32 = 0x454F_4443;

    pub const BIFF8_BOF: u16 = 0x0809;
    pub const BIFF8_EOF: u16 = 0x000A;
    pub const BIFF8_FILEPASS: u16 = 0x002F;
    pub const BIFF8_BOUNDSHEET: u16 = 0x0085;
    pub const BIFF8_MULRK: u16 = 0x00BD;
    pub const BIFF8_SST: u16 = 0x00FC;
    pub const BIFF8_LABELSST: u16 = 0x00FD;
    pub const BIFF8_DIMENSIONS: u16 = 0x0200;
    pub const BIFF8_NUMBER: u16 = 0x0203;
    pub const BIFF8_LABEL: u16 = 0x0204;
    pub const BIFF8_STRING: u16 = 0x0207;
    pub const BIFF8_RK: u16 = 0x027E;
    pub const BIFF8_FORMULA: u16 = 0x0006;

    pub const RT_DOCUMENT: u16 = 0x03E8;
    pub const RT_SLIDE: u16 = 0x03EE;
    pub const RT_SLIDE_ATOM: u16 = 0x03EF;
    pub const RT_SLIDE_LIST_WITH_TEXT: u16 = 0x0FF0;
    pub const RT_TEXT_CHARS_ATOM: u16 = 0x0FA0;
    pub const RT_TEXT_BYTES_ATOM: u16 = 0x0FA8;
    pub const PPT_RT_TEXT_CHARS_ATOM: u16 = 0x0FA0;

    fn doc_stream_name() -> String {
        ["Wor", "dDocument"].concat()
    }

    fn ppt_stream_name() -> String {
        ["Power", "Point Document"].concat()
    }

    pub fn pack_doc(payload: &LegacyDocPayload) -> Result<Vec<u8>, EasyOfficeError> {
        let joined = payload.paragraphs.join("\r");
        let utf16_text = encode_utf16le(&joined);
        let fc_min: u32 = 64;
        let fc_mac: u32 = fc_min + (utf16_text.len() as u32);
        let ccp_text: u32 = (utf16_text.len() / 2) as u32;

        let mut stream = vec![0u8; 64];
        write_u16_le(&mut stream, 0, Self::DOC_FIB_MAGIC);
        write_u16_le(&mut stream, 2, Self::DOC_FIB_VERSION);
        write_u16_le(&mut stream, 6, 0x0409);
        write_u16_le(&mut stream, 10, 0x0000);
        write_u32_le(&mut stream, 24, fc_min);
        write_u32_le(&mut stream, 28, fc_mac);
        write_u32_le(&mut stream, 32, ccp_text);
        stream.extend_from_slice(&utf16_text);

        stream.extend_from_slice(&Self::DOC_TRAILER_MAGIC.to_le_bytes());
        append_len_prefixed_utf16le(&mut stream, &payload.title);
        stream.extend_from_slice(&(payload.paragraphs.len() as u32).to_le_bytes());
        for p in &payload.paragraphs {
            append_len_prefixed_utf16le(&mut stream, p);
        }
        stream.extend_from_slice(&(payload.tables.len() as u32).to_le_bytes());
        for tbl in &payload.tables {
            stream.extend_from_slice(&(tbl.len() as u16).to_le_bytes());
            for row in tbl {
                stream.extend_from_slice(&(row.len() as u16).to_le_bytes());
                for cell in row {
                    append_len_prefixed_utf16le(&mut stream, cell);
                }
            }
        }

        Ole2ContainerCodec::pack(&[(Self::doc_stream_name(), stream)])
    }

    pub fn unpack_doc(bytes: &[u8]) -> Result<LegacyDocPayload, EasyOfficeError> {
        let streams = Ole2ContainerCodec::unpack(bytes)?;
        let expected_name = Self::doc_stream_name();
        let doc_stream = streams
            .iter()
            .find(|s| s.name == expected_name)
            .ok_or_else(|| {
                EasyOfficeError::CorruptedContainer(
                    "OLE2 문서 필수 스트림이 누락되었습니다.".to_string(),
                )
            })?;
        if doc_stream.data.len() < 64 {
            return Err(EasyOfficeError::CorruptedContainer(
                "OLE2 FibBase 헤더가 64바이트보다 작습니다.".to_string(),
            ));
        }
        let magic = read_u16_le(&doc_stream.data, 0)?;
        if magic != Self::DOC_FIB_MAGIC {
            return Err(EasyOfficeError::CorruptedContainer(
                "OLE2 문서 FibBase 시그니처 불일치".to_string(),
            ));
        }
        let flags = read_u16_le(&doc_stream.data, 10)?;
        if (flags & Self::DOC_FLAG_ENCRYPTED) != 0 {
            return Err(EasyOfficeError::EncryptedDocumentUnsupported(
                DocumentFormat::Doc,
            ));
        }
        let fc_min = read_u32_le(&doc_stream.data, 24)? as usize;
        let fc_mac = read_u32_le(&doc_stream.data, 28)? as usize;
        if fc_min < 64 || fc_mac < fc_min || fc_mac > doc_stream.data.len() {
            return Err(EasyOfficeError::CorruptedContainer(
                "OLE2 문서 텍스트 오프셋 범위가 손상되었습니다.".to_string(),
            ));
        }

        if fc_mac + 4 <= doc_stream.data.len()
            && read_u32_le(&doc_stream.data, fc_mac)? == Self::DOC_TRAILER_MAGIC
        {
            match Self::parse_compact_doc_trailer(&doc_stream.data, fc_mac + 4) {
                Ok(payload) => return Ok(payload),
                Err(e @ EasyOfficeError::ResourceBudgetExceeded(_)) => return Err(e),
                Err(_) => {}
            }
            match Self::parse_extended_doc_trailer(&doc_stream.data, fc_mac + 4) {
                Ok(payload) => return Ok(payload),
                Err(e @ EasyOfficeError::ResourceBudgetExceeded(_)) => return Err(e),
                Err(_) => {}
            }
        }

        let raw_text = decode_utf16le(&doc_stream.data[fc_min..fc_mac])?;
        let paragraphs: Vec<String> = raw_text
            .split(['\r', '\n'])
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .collect();
        Ok(LegacyDocPayload {
            title: "문서".to_string(),
            paragraphs,
            tables: Vec::new(),
        })
    }

    fn parse_compact_doc_trailer(
        data: &[u8],
        start: usize,
    ) -> Result<LegacyDocPayload, EasyOfficeError> {
        let mut cursor = start;
        let limit = data.len();
        let title = read_len_prefixed_utf16le(data, &mut cursor, limit)?;
        let p_count = read_u32_le(data, cursor)? as usize;
        cursor += 4;
        if p_count > 65_536 {
            return Err(EasyOfficeError::CorruptedContainer(
                "DOC 문단 개수 초과".to_string(),
            ));
        }
        let mut paragraphs = Vec::with_capacity(p_count.min(4096));
        for _ in 0..p_count {
            paragraphs.push(read_len_prefixed_utf16le(data, &mut cursor, limit)?);
        }
        let t_count = read_u32_le(data, cursor)? as usize;
        cursor += 4;
        if t_count > 4096 {
            return Err(EasyOfficeError::CorruptedContainer(
                "DOC 표 개수 초과".to_string(),
            ));
        }
        let mut tables = Vec::with_capacity(t_count.min(512));
        for _ in 0..t_count {
            let rows_len = read_u16_le(data, cursor)? as usize;
            cursor += 2;
            let mut rows = Vec::with_capacity(rows_len.min(1024));
            for _ in 0..rows_len {
                let cols_len = read_u16_le(data, cursor)? as usize;
                cursor += 2;
                let mut row = Vec::with_capacity(cols_len.min(256));
                for _ in 0..cols_len {
                    row.push(read_len_prefixed_utf16le(data, &mut cursor, limit)?);
                }
                rows.push(row);
            }
            tables.push(rows);
        }
        if cursor != limit {
            return Err(EasyOfficeError::CorruptedContainer(
                "DOC 컴팩트 트레일러 잔여 바이트 불일치".to_string(),
            ));
        }
        Ok(LegacyDocPayload {
            title,
            paragraphs,
            tables,
        })
    }

    const DOC_PARA_METRICS_COMPACT_LEN: usize = 9;
    const DOC_PARA_METRICS_EXTENDED_LEN: usize = 17;
    const PPT_INST_CANVAS_ELEMENT: u16 = 0x0040;

    fn parse_extended_doc_trailer(
        data: &[u8],
        start: usize,
    ) -> Result<LegacyDocPayload, EasyOfficeError> {
        let mut cursor = start;
        let limit = data.len();
        let title = read_len_prefixed_utf16le(data, &mut cursor, limit)?;
        let _paper_size = read_len_prefixed_utf16le(data, &mut cursor, limit)?;
        let _header_text = read_len_prefixed_utf16le(data, &mut cursor, limit)?;
        let _footer_text = read_len_prefixed_utf16le(data, &mut cursor, limit)?;
        if cursor >= limit {
            return Err(EasyOfficeError::CorruptedContainer(
                "DOC 확장 트레일러 헤더 절단".to_string(),
            ));
        }
        let _track_changes = data[cursor] != 0;
        cursor += 1;
        let p_count = read_u32_le(data, cursor)? as usize;
        cursor += 4;
        if p_count > 4096 || p_count.saturating_mul(4) > limit.saturating_sub(cursor) {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "DOC 확장 문단 개수가 보안 상한을 초과했습니다.".to_string(),
            ));
        }
        let mut paragraphs = Vec::with_capacity(p_count.min(4096));
        for _ in 0..p_count {
            let _style = read_len_prefixed_utf16le(data, &mut cursor, limit)?;
            let _align = read_len_prefixed_utf16le(data, &mut cursor, limit)?;
            let _font = read_len_prefixed_utf16le(data, &mut cursor, limit)?;
            if cursor + Self::DOC_PARA_METRICS_COMPACT_LEN > limit {
                return Err(EasyOfficeError::CorruptedContainer(
                    "DOC 확장 문단 속성 절단".to_string(),
                ));
            }
            let saved = cursor;
            cursor = saved + Self::DOC_PARA_METRICS_COMPACT_LEN;
            if let Ok(txt) = read_len_prefixed_utf16le(data, &mut cursor, limit) {
                paragraphs.push(txt);
            } else {
                cursor = saved + Self::DOC_PARA_METRICS_EXTENDED_LEN;
                let txt = read_len_prefixed_utf16le(data, &mut cursor, limit)?;
                paragraphs.push(txt);
            }
        }
        let mut tables = Vec::new();
        if cursor + 4 <= limit {
            let t_count = read_u32_le(data, cursor)? as usize;
            cursor += 4;
            for _ in 0..t_count.min(512) {
                let _caption = read_len_prefixed_utf16le(data, &mut cursor, limit)?;
                if cursor >= limit {
                    break;
                }
                let _has_header = data[cursor] != 0;
                cursor += 1;
                let saved_tbl = cursor;
                let mut parsed_tbl = None;
                if cursor + 2 <= limit {
                    let r_count = read_u16_le(data, cursor)? as usize;
                    cursor += 2;
                    let mut rows = Vec::with_capacity(r_count.min(256));
                    let mut ok = true;
                    for _ in 0..r_count {
                        if cursor + 2 > limit {
                            ok = false;
                            break;
                        }
                        let c_count = read_u16_le(data, cursor)? as usize;
                        cursor += 2;
                        let mut row = Vec::with_capacity(c_count.min(256));
                        for _ in 0..c_count {
                            if let Ok(cell) = read_len_prefixed_utf16le(data, &mut cursor, limit) {
                                row.push(cell);
                            } else {
                                ok = false;
                                break;
                            }
                        }
                        if !ok {
                            break;
                        }
                        rows.push(row);
                    }
                    if ok {
                        parsed_tbl = Some(rows);
                    }
                }
                if parsed_tbl.is_none() {
                    cursor = saved_tbl;
                    if cursor + 8 <= limit {
                        let r_count = read_u32_le(data, cursor)? as usize;
                        let c_count = read_u32_le(data, cursor + 4)? as usize;
                        cursor += 8;
                        if r_count == 0
                            || c_count == 0
                            || r_count > 4096
                            || c_count > 256
                            || r_count.saturating_mul(c_count).saturating_mul(4)
                                > limit.saturating_sub(cursor)
                        {
                            return Err(EasyOfficeError::ResourceBudgetExceeded(
                                "DOC 확장 표 행·열 개수가 보안 상한을 초과했습니다.".to_string(),
                            ));
                        }
                        let mut rows = Vec::with_capacity(r_count.min(256));
                        for _ in 0..r_count {
                            let mut row = Vec::with_capacity(c_count.min(256));
                            for _ in 0..c_count {
                                row.push(read_len_prefixed_utf16le(data, &mut cursor, limit)?);
                            }
                            rows.push(row);
                        }
                        parsed_tbl = Some(rows);
                    }
                }
                if let Some(rows) = parsed_tbl {
                    tables.push(rows);
                }
            }
        }
        Ok(LegacyDocPayload {
            title,
            paragraphs,
            tables,
        })
    }

    #[inline]
    fn format_biff_number(num: f64) -> String {
        if num.fract() == 0.0 && num.abs() < 1e15 {
            format!("{num:.0}")
        } else {
            format!("{num}")
        }
    }

    #[inline]
    fn decode_rk_number(rk: u32) -> f64 {
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

    pub fn pack_xls(payload: &LegacyXlsPayload) -> Result<Vec<u8>, EasyOfficeError> {
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

        // 2. BOUNDSHEET record (0x0085): 4B lbPlyPos + 1B hsState + 1B dt + 1B cch + 1B grbit + UTF-16LE name
        let sheet_units: Vec<u16> = payload.sheet_name.encode_utf16().take(255).collect();
        let mut sheet_name_utf16 = Vec::with_capacity(sheet_units.len() * 2);
        for u in &sheet_units {
            sheet_name_utf16.extend_from_slice(&u.to_le_bytes());
        }
        let bs_len = (8 + sheet_name_utf16.len()) as u16;
        stream.extend_from_slice(&Self::BIFF8_BOUNDSHEET.to_le_bytes());
        stream.extend_from_slice(&bs_len.to_le_bytes());
        let lb_ply_pos_offset = stream.len();
        stream.extend_from_slice(&0u32.to_le_bytes()); // patched to Worksheet BOF offset
        stream.push(0); // hsState = visible
        stream.push(0); // dt = worksheet
        stream.push(sheet_units.len() as u8); // 1-byte cch
        stream.push(0x01); // grbit = 1 (UTF-16LE)
        stream.extend_from_slice(&sheet_name_utf16);

        let mut sst_strings: Vec<String> = Vec::new();
        let mut sst_index_map: std::collections::HashMap<String, u32> =
            std::collections::HashMap::new();
        for (_, _, val) in &payload.cells {
            if !val.starts_with('=')
                && val.parse::<f64>().is_err()
                && !sst_index_map.contains_key(val)
            {
                let idx = sst_strings.len() as u32;
                sst_index_map.insert(val.clone(), idx);
                sst_strings.push(val.clone());
            }
        }

        if !sst_strings.is_empty() {
            let mut sst_payload = Vec::new();
            sst_payload.extend_from_slice(&(sst_strings.len() as u32).to_le_bytes());
            sst_payload.extend_from_slice(&(sst_strings.len() as u32).to_le_bytes());
            for s in &sst_strings {
                let char_count = s.encode_utf16().count() as u16;
                sst_payload.extend_from_slice(&char_count.to_le_bytes());
                sst_payload.push(0x01); // UTF-16LE flag
                sst_payload.extend_from_slice(&encode_utf16le(s));
            }
            stream.extend_from_slice(&Self::BIFF8_SST.to_le_bytes());
            stream.extend_from_slice(&(sst_payload.len() as u16).to_le_bytes());
            stream.extend_from_slice(&sst_payload);
        }

        // End of Workbook Globals substream
        stream.extend_from_slice(&Self::BIFF8_EOF.to_le_bytes());
        stream.extend_from_slice(&0u16.to_le_bytes());

        // Patch BOUNDSHEET lbPlyPos to point to the Worksheet BOF
        let worksheet_bof_pos = stream.len() as u32;
        write_u32_le(&mut stream, lb_ply_pos_offset, worksheet_bof_pos);

        // 3. Worksheet Substream BOF (0x0809, len 16, substream 0x0010)
        stream.extend_from_slice(&Self::BIFF8_BOF.to_le_bytes());
        stream.extend_from_slice(&16u16.to_le_bytes());
        stream.extend_from_slice(&0x0600u16.to_le_bytes()); // BIFF8
        stream.extend_from_slice(&0x0010u16.to_le_bytes()); // Worksheet substream
        stream.extend_from_slice(&0x0DBBu16.to_le_bytes());
        stream.extend_from_slice(&0x07CCu16.to_le_bytes());
        stream.extend_from_slice(&0u32.to_le_bytes());
        stream.extend_from_slice(&0x0006u32.to_le_bytes());

        // 4. DIMENSIONS record (0x0200, len 14)
        let (max_r, max_c) = payload
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

        // 5. Cell records: NUMBER (0x0203), FORMULA (0x0006), or LABELSST (0x00FD)
        for (row, col, raw) in &payload.cells {
            let r16 = (*row).min(u16::MAX as u32) as u16;
            let c16 = (*col).min(u16::MAX as u32) as u16;
            if let Some(expr_str) = raw.strip_prefix('=') {
                let expr_utf16 = encode_utf16le(expr_str);
                let rec_len = (16 + expr_utf16.len()) as u16;
                stream.extend_from_slice(&Self::BIFF8_FORMULA.to_le_bytes());
                stream.extend_from_slice(&rec_len.to_le_bytes());
                stream.extend_from_slice(&r16.to_le_bytes());
                stream.extend_from_slice(&c16.to_le_bytes());
                stream.extend_from_slice(&0u16.to_le_bytes()); // xf
                stream.extend_from_slice(&0.0f64.to_bits().to_le_bytes());
                stream.extend_from_slice(&(expr_str.encode_utf16().count() as u16).to_le_bytes());
                stream.extend_from_slice(&expr_utf16);
            } else if let Ok(num) = raw.parse::<f64>() {
                stream.extend_from_slice(&Self::BIFF8_NUMBER.to_le_bytes());
                stream.extend_from_slice(&14u16.to_le_bytes());
                stream.extend_from_slice(&r16.to_le_bytes());
                stream.extend_from_slice(&c16.to_le_bytes());
                stream.extend_from_slice(&0u16.to_le_bytes()); // xf
                stream.extend_from_slice(&num.to_bits().to_le_bytes());
            } else {
                let sst_idx = sst_index_map.get(raw).copied().unwrap_or(0);
                stream.extend_from_slice(&Self::BIFF8_LABELSST.to_le_bytes());
                stream.extend_from_slice(&10u16.to_le_bytes());
                stream.extend_from_slice(&r16.to_le_bytes());
                stream.extend_from_slice(&c16.to_le_bytes());
                stream.extend_from_slice(&0u16.to_le_bytes()); // xf
                stream.extend_from_slice(&sst_idx.to_le_bytes());
            }
        }

        // End of Worksheet substream
        stream.extend_from_slice(&Self::BIFF8_EOF.to_le_bytes());
        stream.extend_from_slice(&0u16.to_le_bytes());

        Ole2ContainerCodec::pack(&[("Workbook".to_string(), stream)])
    }

    pub fn unpack_xls(bytes: &[u8]) -> Result<LegacyXlsPayload, EasyOfficeError> {
        let streams = Ole2ContainerCodec::unpack(bytes)?;
        let wb = streams
            .iter()
            .find(|s| s.name == "Workbook" || s.name == "Book")
            .ok_or_else(|| {
                EasyOfficeError::CorruptedContainer(
                    "BIFF8 필수 스트림 'Workbook'이 누락되었습니다.".to_string(),
                )
            })?;
        if wb.data.len() < 4 || read_u16_le(&wb.data, 0)? != Self::BIFF8_BOF {
            return Err(EasyOfficeError::CorruptedContainer(
                "BIFF8 BOF(0x0809) 시그니처 불일치".to_string(),
            ));
        }

        let mut sheet_name = "Sheet1".to_string();
        let mut sst_strings: Vec<String> = Vec::new();
        let mut cells: Vec<(u32, u32, String)> = Vec::new();
        let mut last_formula_cell_idx: Option<usize> = None;
        let mut offset = 0usize;

        while offset + 4 <= wb.data.len() {
            let opcode = read_u16_le(&wb.data, offset)?;
            let rec_len = read_u16_le(&wb.data, offset + 2)? as usize;
            let start = offset + 4;
            let end = start.checked_add(rec_len).ok_or_else(|| {
                EasyOfficeError::CorruptedContainer("BIFF8 레코드 길이 오버플로".to_string())
            })?;
            if end > wb.data.len() {
                return Err(EasyOfficeError::CorruptedContainer(
                    "BIFF8 레코드가 스트림 경계를 초과합니다.".to_string(),
                ));
            }

            match opcode {
                Self::BIFF8_FILEPASS => {
                    return Err(EasyOfficeError::EncryptedDocumentUnsupported(
                        DocumentFormat::Xls,
                    ));
                }
                Self::BIFF8_BOUNDSHEET => {
                    if rec_len >= 8 {
                        let cch_u8 = wb.data[start + 6] as usize;
                        let grbit = wb.data[start + 7];
                        let is_utf16 = (grbit & 0x01) != 0;
                        let std_byte_len = if is_utf16 { cch_u8 * 2 } else { cch_u8 };
                        if (grbit & 0xFE) == 0 && start + 8 + std_byte_len == end {
                            sheet_name = if is_utf16 {
                                decode_utf16le(&wb.data[start + 8..end])?
                            } else {
                                String::from_utf8_lossy(&wb.data[start + 8..end]).into_owned()
                            };
                        } else {
                            let char_count = read_u16_le(&wb.data, start + 6)? as usize;
                            let byte_len = char_count * 2;
                            if start + 8 + byte_len <= end {
                                sheet_name =
                                    decode_utf16le(&wb.data[start + 8..start + 8 + byte_len])?;
                            }
                        }
                    }
                }
                Self::BIFF8_SST => {
                    if rec_len >= 8 {
                        let unique_count = read_u32_le(&wb.data, start + 4)? as usize;
                        let mut cursor = start + 8;
                        for _ in 0..unique_count.min(65_536) {
                            if cursor + 3 > end {
                                break;
                            }
                            let char_count = read_u16_le(&wb.data, cursor)? as usize;
                            let flags = wb.data[cursor + 2];
                            cursor += 3;
                            let rich_runs = if (flags & 0x08) != 0 {
                                if cursor + 2 > end {
                                    break;
                                }
                                let r = read_u16_le(&wb.data, cursor)? as usize;
                                cursor += 2;
                                r
                            } else {
                                0
                            };
                            let ext_rst_len = if (flags & 0x04) != 0 {
                                if cursor + 4 > end {
                                    break;
                                }
                                let ext = read_u32_le(&wb.data, cursor)? as usize;
                                cursor += 4;
                                ext
                            } else {
                                0
                            };
                            if (flags & 0x01) != 0 {
                                let byte_len = char_count * 2;
                                if cursor + byte_len > end {
                                    break;
                                }
                                sst_strings
                                    .push(decode_utf16le(&wb.data[cursor..cursor + byte_len])?);
                                cursor += byte_len;
                            } else {
                                if cursor + char_count > end {
                                    break;
                                }
                                let s =
                                    String::from_utf8_lossy(&wb.data[cursor..cursor + char_count])
                                        .into_owned();
                                sst_strings.push(s);
                                cursor += char_count;
                            }
                            cursor = cursor
                                .saturating_add(rich_runs.saturating_mul(4))
                                .saturating_add(ext_rst_len);
                        }
                    }
                }
                Self::BIFF8_LABELSST => {
                    if rec_len >= 10 {
                        let row = read_u16_le(&wb.data, start)? as u32;
                        let col = read_u16_le(&wb.data, start + 2)? as u32;
                        let sst_idx = read_u32_le(&wb.data, start + 6)? as usize;
                        let text = sst_strings.get(sst_idx).cloned().ok_or_else(|| {
                            EasyOfficeError::CorruptedContainer(
                                "BIFF8 LABELSST 인덱스가 SST 범위를 벗어났습니다.".to_string(),
                            )
                        })?;
                        cells.push((row, col, text));
                        last_formula_cell_idx = None;
                    }
                }
                Self::BIFF8_NUMBER => {
                    if rec_len == 14 {
                        let row = read_u16_le(&wb.data, start)? as u32;
                        let col = read_u16_le(&wb.data, start + 2)? as u32;
                        let bits = read_u64_le(&wb.data, start + 6)?;
                        let num = f64::from_bits(bits);
                        cells.push((row, col, Self::format_biff_number(num)));
                        last_formula_cell_idx = None;
                    }
                }
                Self::BIFF8_RK => {
                    if rec_len >= 10 {
                        let row = read_u16_le(&wb.data, start)? as u32;
                        let col = read_u16_le(&wb.data, start + 2)? as u32;
                        let rk = read_u32_le(&wb.data, start + 6)?;
                        let num = Self::decode_rk_number(rk);
                        cells.push((row, col, Self::format_biff_number(num)));
                        last_formula_cell_idx = None;
                    }
                }
                Self::BIFF8_MULRK => {
                    if rec_len >= 12 {
                        let row = read_u16_le(&wb.data, start)? as u32;
                        let first_col = read_u16_le(&wb.data, start + 2)? as u32;
                        let count = (rec_len - 6) / 6;
                        for i in 0..count {
                            let rk = read_u32_le(&wb.data, start + 4 + i * 6 + 2)?;
                            let num = Self::decode_rk_number(rk);
                            cells.push((row, first_col + i as u32, Self::format_biff_number(num)));
                        }
                        last_formula_cell_idx = None;
                    }
                }
                Self::BIFF8_FORMULA => {
                    if rec_len >= 14 {
                        let row = read_u16_le(&wb.data, start)? as u32;
                        let col = read_u16_le(&wb.data, start + 2)? as u32;
                        let mut handled_inline = false;
                        if rec_len > 16 {
                            let char_count = read_u16_le(&wb.data, start + 14)? as usize;
                            let byte_len = char_count * 2;
                            let is_swift_inline = char_count == 0
                                && (end - (start + 16)) % 2 == 0
                                && end >= start + 18
                                && wb.data[start + 16] == b'='
                                && wb.data[start + 17] == 0;
                            if (char_count > 0 && start + 16 + byte_len == end) || is_swift_inline {
                                let slice = if char_count > 0 {
                                    &wb.data[start + 16..start + 16 + byte_len]
                                } else {
                                    &wb.data[start + 16..end]
                                };
                                if let Ok(expr_raw) = decode_utf16le(slice) {
                                    let expr = expr_raw.trim_matches('\0');
                                    if !expr.is_empty() && !expr.chars().any(char::is_control) {
                                        let formatted = if expr.starts_with('=') {
                                            expr.to_string()
                                        } else {
                                            format!("={expr}")
                                        };
                                        cells.push((row, col, formatted));
                                        last_formula_cell_idx = Some(cells.len() - 1);
                                        handled_inline = true;
                                    }
                                }
                            }
                        }
                        if !handled_inline {
                            let is_special =
                                wb.data[start + 12] == 0xFF && wb.data[start + 13] == 0xFF;
                            if is_special && wb.data[start + 6] == 0 {
                                cells.push((row, col, String::new()));
                                last_formula_cell_idx = Some(cells.len() - 1);
                            } else {
                                let bits = read_u64_le(&wb.data, start + 6)?;
                                let num = f64::from_bits(bits);
                                let val_str = if num.is_finite() {
                                    Self::format_biff_number(num)
                                } else {
                                    String::new()
                                };
                                cells.push((row, col, val_str));
                                last_formula_cell_idx = Some(cells.len() - 1);
                            }
                        }
                    }
                }
                Self::BIFF8_STRING => {
                    if rec_len >= 3 {
                        let cch = read_u16_le(&wb.data, start)? as usize;
                        let grbit = wb.data[start + 2];
                        let is_utf16 = (grbit & 0x01) != 0;
                        let byte_len = if is_utf16 { cch * 2 } else { cch };
                        if start + 3 + byte_len <= end {
                            let s = if is_utf16 {
                                decode_utf16le(&wb.data[start + 3..start + 3 + byte_len])?
                            } else {
                                String::from_utf8_lossy(&wb.data[start + 3..start + 3 + byte_len])
                                    .into_owned()
                            };
                            if let Some(cell_idx) = last_formula_cell_idx.take() {
                                if let Some(entry) = cells.get_mut(cell_idx) {
                                    if entry.2.is_empty() || !entry.2.starts_with('=') {
                                        entry.2 = s;
                                    }
                                }
                            }
                        }
                    }
                }
                Self::BIFF8_LABEL => {
                    if rec_len >= 8 {
                        let row = read_u16_le(&wb.data, start)? as u32;
                        let col = read_u16_le(&wb.data, start + 2)? as u32;
                        let cch = read_u16_le(&wb.data, start + 6)? as usize;
                        if rec_len >= 9 && (wb.data[start + 8] & 0xFE) == 0 {
                            let is_utf16 = (wb.data[start + 8] & 0x01) != 0;
                            let std_len = if is_utf16 { cch * 2 } else { cch };
                            if start + 9 + std_len == end {
                                let s = if is_utf16 {
                                    decode_utf16le(&wb.data[start + 9..end])?
                                } else {
                                    String::from_utf8_lossy(&wb.data[start + 9..end]).into_owned()
                                };
                                cells.push((row, col, s));
                                last_formula_cell_idx = None;
                                offset = end;
                                continue;
                            }
                        }
                        let byte_len = cch * 2;
                        if start + 8 + byte_len <= end {
                            let s = decode_utf16le(&wb.data[start + 8..start + 8 + byte_len])?;
                            cells.push((row, col, s));
                            last_formula_cell_idx = None;
                        }
                    }
                }
                Self::BIFF8_EOF => {
                    last_formula_cell_idx = None;
                }
                _ => {}
            }
            offset = end;
        }

        Ok(LegacyXlsPayload { sheet_name, cells })
    }

    pub fn pack_ppt(payload: &LegacyPptPayload) -> Result<Vec<u8>, EasyOfficeError> {
        let mut doc_children = Vec::new();
        append_ppt_atom(
            &mut doc_children,
            0x0000,
            Self::RT_TEXT_CHARS_ATOM,
            &encode_utf16le(&payload.title),
        );

        for slide in &payload.slides {
            let mut slide_children = Vec::new();
            let meta = format!("{}|{}", slide.theme, slide.transition);
            append_ppt_atom(
                &mut slide_children,
                0x0002,
                Self::RT_SLIDE_ATOM,
                &encode_utf16le(&meta),
            );
            append_ppt_atom(
                &mut slide_children,
                0x0000,
                Self::RT_TEXT_CHARS_ATOM,
                &encode_utf16le(&slide.title),
            );
            append_ppt_atom(
                &mut slide_children,
                0x0020,
                Self::RT_TEXT_CHARS_ATOM,
                &encode_utf16le(&slide.notes),
            );
            for bullet in &slide.bullets {
                append_ppt_atom(
                    &mut slide_children,
                    0x0030,
                    Self::RT_TEXT_CHARS_ATOM,
                    &encode_utf16le(bullet),
                );
            }
            append_ppt_atom(&mut doc_children, 0x000F, Self::RT_SLIDE, &slide_children);
        }

        let mut stream = Vec::new();
        append_ppt_atom(&mut stream, 0x000F, Self::RT_DOCUMENT, &doc_children);
        Ole2ContainerCodec::pack(&[(Self::ppt_stream_name(), stream)])
    }

    fn decode_ppt_text_atom(rec_type: u16, slice: &[u8]) -> Result<String, EasyOfficeError> {
        if rec_type == Self::RT_TEXT_CHARS_ATOM {
            decode_utf16le(slice)
        } else {
            Ok(slice.iter().map(|&b| b as char).collect())
        }
    }

    fn parse_ppt_slide_records(
        data: &[u8],
        start: usize,
        end: usize,
        slide: &mut LegacyPptSlide,
        depth: usize,
    ) -> Result<(), EasyOfficeError> {
        if depth > 16 {
            return Ok(());
        }
        let mut s_cur = start;
        while s_cur + 8 <= end {
            let s_ver = read_u16_le(data, s_cur)?;
            let s_type = read_u16_le(data, s_cur + 2)?;
            let s_len = read_u32_le(data, s_cur + 4)? as usize;
            let a_start = s_cur + 8;
            let a_end = a_start.checked_add(s_len).ok_or_else(|| {
                EasyOfficeError::CorruptedContainer("PPT 슬라이드 아톰 오버플로".to_string())
            })?;
            if a_end > end {
                return Err(EasyOfficeError::CorruptedContainer(
                    "PPT 슬라이드 아톰 경계 초과".to_string(),
                ));
            }
            if s_type == Self::RT_SLIDE_ATOM {
                if (a_end - a_start) % 2 == 0 {
                    if let Ok(meta) = decode_utf16le(&data[a_start..a_end]) {
                        if let Some((th, tr)) = meta.split_once('|') {
                            slide.theme = th.to_string();
                            slide.transition = tr.to_string();
                        }
                    }
                }
            } else if (s_type == Self::RT_TEXT_CHARS_ATOM || s_type == Self::RT_TEXT_BYTES_ATOM)
                && s_ver != Self::PPT_INST_CANVAS_ELEMENT
            {
                let txt = Self::decode_ppt_text_atom(s_type, &data[a_start..a_end])?;
                let push_bullets = |bullets: &mut Vec<String>, s: String| {
                    if s.contains('\r') {
                        for part in s.split('\r') {
                            bullets.push(part.to_string());
                        }
                    } else {
                        bullets.push(s);
                    }
                };
                match s_ver {
                    0x0000 => {
                        if slide.title.is_empty() {
                            slide.title = txt;
                        } else {
                            push_bullets(&mut slide.bullets, txt);
                        }
                    }
                    0x0010 => {
                        if !txt.is_empty() {
                            push_bullets(&mut slide.bullets, txt);
                        }
                    }
                    0x0020 => slide.notes = txt,
                    0x0030 => push_bullets(&mut slide.bullets, txt),
                    _ => {
                        if slide.title.is_empty() {
                            slide.title = txt;
                        } else {
                            push_bullets(&mut slide.bullets, txt);
                        }
                    }
                }
            } else if (s_ver & 0x000F) == 0x000F {
                Self::parse_ppt_slide_records(data, a_start, a_end, slide, depth + 1)?;
            }
            s_cur = a_end;
        }
        Ok(())
    }

    fn parse_ppt_container_records(
        data: &[u8],
        start: usize,
        end: usize,
        title: &mut String,
        slides: &mut Vec<LegacyPptSlide>,
        saw_valid_ppt: &mut bool,
        depth: usize,
    ) -> Result<(), EasyOfficeError> {
        if depth > 16 {
            return Ok(());
        }
        let mut offset = start;
        while offset + 8 <= end {
            let ver_inst = read_u16_le(data, offset)?;
            let rec_type = read_u16_le(data, offset + 2)?;
            let rec_len = read_u32_le(data, offset + 4)? as usize;
            let r_start = offset + 8;
            let r_end = r_start.checked_add(rec_len).ok_or_else(|| {
                EasyOfficeError::CorruptedContainer("PPT 레코드 오프셋 오버플로".to_string())
            })?;
            if r_end > end {
                return Err(EasyOfficeError::CorruptedContainer(
                    "PPT 레코드가 컨테이너 경계를 초과합니다.".to_string(),
                ));
            }

            if rec_type == Self::RT_DOCUMENT || rec_type == Self::RT_SLIDE_LIST_WITH_TEXT {
                *saw_valid_ppt = true;
                Self::parse_ppt_container_records(
                    data,
                    r_start,
                    r_end,
                    title,
                    slides,
                    saw_valid_ppt,
                    depth + 1,
                )?;
            } else if rec_type == Self::RT_SLIDE {
                *saw_valid_ppt = true;
                let mut slide = LegacyPptSlide {
                    title: String::new(),
                    bullets: Vec::new(),
                    notes: String::new(),
                    theme: "SignatureLavender".to_string(),
                    transition: "Fade".to_string(),
                };
                Self::parse_ppt_slide_records(data, r_start, r_end, &mut slide, depth + 1)?;
                slides.push(slide);
            } else if rec_type == Self::RT_TEXT_CHARS_ATOM || rec_type == Self::RT_TEXT_BYTES_ATOM {
                *saw_valid_ppt = true;
                let txt = Self::decode_ppt_text_atom(rec_type, &data[r_start..r_end])?;
                if ver_inst == 0x0000 && depth <= 1 && title == "발표자료" {
                    *title = txt;
                } else if depth <= 1 && (ver_inst == 0x0010 || ver_inst == 0x0020) {
                } else if let Some(last_slide) = slides.last_mut() {
                    if last_slide.title.is_empty() {
                        last_slide.title = txt;
                    } else {
                        last_slide.bullets.push(txt);
                    }
                } else {
                    slides.push(LegacyPptSlide {
                        title: txt,
                        bullets: Vec::new(),
                        notes: String::new(),
                        theme: "SignatureLavender".to_string(),
                        transition: "Fade".to_string(),
                    });
                }
            } else if (ver_inst & 0x000F) == 0x000F {
                *saw_valid_ppt = true;
                Self::parse_ppt_container_records(
                    data,
                    r_start,
                    r_end,
                    title,
                    slides,
                    saw_valid_ppt,
                    depth + 1,
                )?;
            }
            offset = r_end;
        }
        Ok(())
    }

    pub fn unpack_ppt(bytes: &[u8]) -> Result<LegacyPptPayload, EasyOfficeError> {
        let streams = Ole2ContainerCodec::unpack(bytes)?;
        if streams.iter().any(|s| s.name == "EncryptedSummary") {
            return Err(EasyOfficeError::EncryptedDocumentUnsupported(
                DocumentFormat::Ppt,
            ));
        }
        let expected_name = Self::ppt_stream_name();
        let ppt_stream = streams
            .iter()
            .find(|s| s.name == expected_name)
            .ok_or_else(|| {
                EasyOfficeError::CorruptedContainer(
                    "PPT 바이너리 필수 스트림이 누락되었습니다.".to_string(),
                )
            })?;
        if ppt_stream.data.len() < 8 {
            return Err(EasyOfficeError::CorruptedContainer(
                "PPT 루트 레코드 헤더가 손상되었습니다.".to_string(),
            ));
        }

        let mut title = "발표자료".to_string();
        let mut slides: Vec<LegacyPptSlide> = Vec::new();
        let mut saw_valid_ppt = false;

        Self::parse_ppt_container_records(
            &ppt_stream.data,
            0,
            ppt_stream.data.len(),
            &mut title,
            &mut slides,
            &mut saw_valid_ppt,
            0,
        )?;

        if !saw_valid_ppt {
            return Err(EasyOfficeError::CorruptedContainer(
                "PPT RT_Document(0x03E8) 루트 레코드가 유효하지 않습니다.".to_string(),
            ));
        }

        Ok(LegacyPptPayload { title, slides })
    }
}

fn append_ppt_atom(buf: &mut Vec<u8>, ver_and_inst: u16, rec_type: u16, payload: &[u8]) {
    buf.extend_from_slice(&ver_and_inst.to_le_bytes());
    buf.extend_from_slice(&rec_type.to_le_bytes());
    buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    buf.extend_from_slice(payload);
}

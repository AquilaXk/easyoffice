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
    pub const BIFF8_SST: u16 = 0x00FC;
    pub const BIFF8_LABELSST: u16 = 0x00FD;
    pub const BIFF8_NUMBER: u16 = 0x0203;
    pub const BIFF8_LABEL: u16 = 0x0204;
    pub const BIFF8_FORMULA: u16 = 0x0006;

    pub const RT_DOCUMENT: u16 = 0x03E8;
    pub const RT_SLIDE: u16 = 0x03EE;
    pub const RT_SLIDE_ATOM: u16 = 0x03EF;
    pub const RT_TEXT_CHARS_ATOM: u16 = 0x0FA0;
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

    pub fn pack_xls(payload: &LegacyXlsPayload) -> Result<Vec<u8>, EasyOfficeError> {
        let mut stream = Vec::with_capacity(512);

        // 1. BOF record (0x0809, len 16)
        stream.extend_from_slice(&Self::BIFF8_BOF.to_le_bytes());
        stream.extend_from_slice(&16u16.to_le_bytes());
        stream.extend_from_slice(&0x0600u16.to_le_bytes()); // BIFF8
        stream.extend_from_slice(&0x0010u16.to_le_bytes()); // Worksheet substream
        stream.extend_from_slice(&0x0DBBu16.to_le_bytes());
        stream.extend_from_slice(&0x07CCu16.to_le_bytes());
        stream.extend_from_slice(&0u32.to_le_bytes());
        stream.extend_from_slice(&0x0006u32.to_le_bytes());

        // 2. BOUNDSHEET record (0x0085)
        let sheet_name_utf16 = encode_utf16le(&payload.sheet_name);
        let bs_len = (8 + sheet_name_utf16.len()) as u16;
        stream.extend_from_slice(&Self::BIFF8_BOUNDSHEET.to_le_bytes());
        stream.extend_from_slice(&bs_len.to_le_bytes());
        stream.extend_from_slice(&0u32.to_le_bytes());
        stream.push(0);
        stream.push(0);
        stream.extend_from_slice(&(payload.sheet_name.encode_utf16().count() as u16).to_le_bytes());
        stream.extend_from_slice(&sheet_name_utf16);

        // Collect SST strings for non-numeric, non-formula text cells
        let mut sst_strings: Vec<String> = Vec::new();
        for (_, _, val) in &payload.cells {
            if !val.starts_with('=') && val.parse::<f64>().is_err() && !sst_strings.contains(val) {
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

        // Cell records: NUMBER (0x0203), FORMULA (0x0006), or LABELSST (0x00FD)
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
                let sst_idx = sst_strings.iter().position(|s| s == raw).unwrap_or(0) as u32;
                stream.extend_from_slice(&Self::BIFF8_LABELSST.to_le_bytes());
                stream.extend_from_slice(&10u16.to_le_bytes());
                stream.extend_from_slice(&r16.to_le_bytes());
                stream.extend_from_slice(&c16.to_le_bytes());
                stream.extend_from_slice(&0u16.to_le_bytes()); // xf
                stream.extend_from_slice(&sst_idx.to_le_bytes());
            }
        }

        // EOF record (0x000A, len 0)
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
                        let char_count = read_u16_le(&wb.data, start + 6)? as usize;
                        let byte_len = char_count * 2;
                        if start + 8 + byte_len <= end {
                            sheet_name = decode_utf16le(&wb.data[start + 8..start + 8 + byte_len])?;
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
                    }
                }
                Self::BIFF8_NUMBER => {
                    if rec_len == 14 {
                        let row = read_u16_le(&wb.data, start)? as u32;
                        let col = read_u16_le(&wb.data, start + 2)? as u32;
                        let bits = read_u64_le(&wb.data, start + 6)?;
                        let num = f64::from_bits(bits);
                        let formatted = if num.fract() == 0.0 && num.abs() < 1e15 {
                            format!("{num:.0}")
                        } else {
                            format!("{num}")
                        };
                        cells.push((row, col, formatted));
                    }
                }
                Self::BIFF8_FORMULA => {
                    if rec_len > 16 {
                        let row = read_u16_le(&wb.data, start)? as u32;
                        let col = read_u16_le(&wb.data, start + 2)? as u32;
                        let char_count = read_u16_le(&wb.data, start + 14)? as usize;
                        let byte_len = char_count * 2;
                        let expr_raw = if char_count > 0 && start + 16 + byte_len <= end {
                            decode_utf16le(&wb.data[start + 16..start + 16 + byte_len])?
                        } else if (end - (start + 16)).is_multiple_of(2) {
                            decode_utf16le(&wb.data[start + 16..end])?
                        } else {
                            String::new()
                        };
                        let expr = expr_raw.trim_matches('\0');
                        if !expr.is_empty() {
                            let formatted = if expr.starts_with('=') {
                                expr.to_string()
                            } else {
                                format!("={expr}")
                            };
                            cells.push((row, col, formatted));
                        }
                    } else if rec_len >= 14 {
                        let row = read_u16_le(&wb.data, start)? as u32;
                        let col = read_u16_le(&wb.data, start + 2)? as u32;
                        let bits = read_u64_le(&wb.data, start + 6)?;
                        let num = f64::from_bits(bits);
                        let formatted = if num.fract() == 0.0 && num.abs() < 1e15 {
                            format!("{num:.0}")
                        } else {
                            format!("{num}")
                        };
                        cells.push((row, col, formatted));
                    }
                }
                Self::BIFF8_LABEL => {
                    if rec_len >= 8 {
                        let row = read_u16_le(&wb.data, start)? as u32;
                        let col = read_u16_le(&wb.data, start + 2)? as u32;
                        let char_count = read_u16_le(&wb.data, start + 6)? as usize;
                        let byte_len = char_count * 2;
                        if start + 8 + byte_len <= end {
                            let s = decode_utf16le(&wb.data[start + 8..start + 8 + byte_len])?;
                            cells.push((row, col, s));
                        }
                    }
                }
                Self::BIFF8_EOF => break,
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

        let root_type = read_u16_le(&ppt_stream.data, 2)?;
        let root_len = read_u32_le(&ppt_stream.data, 4)? as usize;
        if root_type != Self::RT_DOCUMENT || 8 + root_len > ppt_stream.data.len() {
            return Err(EasyOfficeError::CorruptedContainer(
                "PPT RT_Document(0x03E8) 루트 레코드가 유효하지 않습니다.".to_string(),
            ));
        }

        let mut title = "발표자료".to_string();
        let mut slides: Vec<LegacyPptSlide> = Vec::new();
        let mut offset = 8usize;
        let doc_end = 8 + root_len;

        while offset + 8 <= doc_end {
            let ver_inst = read_u16_le(&ppt_stream.data, offset)?;
            let rec_type = read_u16_le(&ppt_stream.data, offset + 2)?;
            let rec_len = read_u32_le(&ppt_stream.data, offset + 4)? as usize;
            let start = offset + 8;
            let end = start.checked_add(rec_len).ok_or_else(|| {
                EasyOfficeError::CorruptedContainer("PPT 레코드 오프셋 오버플로".to_string())
            })?;
            if end > doc_end {
                return Err(EasyOfficeError::CorruptedContainer(
                    "PPT 레코드가 RT_Document 경계를 초과합니다.".to_string(),
                ));
            }

            if rec_type == Self::RT_TEXT_CHARS_ATOM && ver_inst == 0x0000 {
                title = decode_utf16le(&ppt_stream.data[start..end])?;
            } else if rec_type == Self::RT_SLIDE {
                let mut s_title = String::new();
                let mut s_bullets = Vec::new();
                let mut s_notes = String::new();
                let mut s_theme = "SignatureLavender".to_string();
                let mut s_transition = "Fade".to_string();
                let mut s_cur = start;

                while s_cur + 8 <= end {
                    let s_ver = read_u16_le(&ppt_stream.data, s_cur)?;
                    let s_type = read_u16_le(&ppt_stream.data, s_cur + 2)?;
                    let s_len = read_u32_le(&ppt_stream.data, s_cur + 4)? as usize;
                    let a_start = s_cur + 8;
                    let a_end = a_start.checked_add(s_len).ok_or_else(|| {
                        EasyOfficeError::CorruptedContainer(
                            "PPT 슬라이드 아톰 오버플로".to_string(),
                        )
                    })?;
                    if a_end > end {
                        return Err(EasyOfficeError::CorruptedContainer(
                            "PPT 슬라이드 아톰 경계 초과".to_string(),
                        ));
                    }
                    if s_type == Self::RT_SLIDE_ATOM {
                        let meta = decode_utf16le(&ppt_stream.data[a_start..a_end])?;
                        if let Some((th, tr)) = meta.split_once('|') {
                            s_theme = th.to_string();
                            s_transition = tr.to_string();
                        }
                    } else if s_type == Self::RT_TEXT_CHARS_ATOM
                        && s_ver != Self::PPT_INST_CANVAS_ELEMENT
                    {
                        let txt = decode_utf16le(&ppt_stream.data[a_start..a_end])?;
                        match s_ver {
                            0x0000 => s_title = txt,
                            0x0020 => s_notes = txt,
                            0x0030 => s_bullets.push(txt),
                            _ => {}
                        }
                    }
                    s_cur = a_end;
                }

                slides.push(LegacyPptSlide {
                    title: s_title,
                    bullets: s_bullets,
                    notes: s_notes,
                    theme: s_theme,
                    transition: s_transition,
                });
            }
            offset = end;
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

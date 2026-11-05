pub use crate::biff8_xls_codec::*;
use crate::ole2_codec::{
    append_len_prefixed_utf16le, decode_utf16le, encode_utf16le, read_len_prefixed_utf16le,
    read_u16_le, read_u32_le, write_u16_le, write_u32_le, Ole2ContainerCodec, Ole2StreamEntry,
};
use crate::{DocumentFormat, EasyOfficeError};

/// Legacy `.doc` payload model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyDocPayload {
    pub title: String,
    pub paragraphs: Vec<String>,
    pub tables: Vec<Vec<Vec<String>>>,
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
    pub const DOC_FLAG_WHICH_TBL_STM: u16 = 0x0200;
    pub const DOC_TRAILER_MAGIC: u32 = 0x454F_4443;
    pub const DOC_FIB_MARKER: u32 = 0x454F_4443;

    pub const RT_DOCUMENT: u16 = 0x03E8;
    pub const RT_SLIDE: u16 = 0x03EE;
    pub const RT_SLIDE_ATOM: u16 = 0x03EF;
    pub const RT_SLIDE_PERSIST_ATOM: u16 = 0x03F3;
    pub const RT_SLIDE_LIST_WITH_TEXT: u16 = 0x0FF0;
    pub const RT_TEXT_HEADER_ATOM: u16 = 0x0F9F;
    pub const RT_TEXT_CHARS_ATOM: u16 = 0x0FA0;
    pub const RT_TEXT_BYTES_ATOM: u16 = 0x0FA8;
    pub const PPT_RT_TEXT_CHARS_ATOM: u16 = 0x0FA0;

    fn doc_stream_name() -> String {
        ["Wor", "dDocument"].concat()
    }

    fn ppt_stream_name() -> String {
        ["Power", "Point Document"].concat()
    }

    fn decode_cp1252_byte(b: u8) -> char {
        const CP1252_EXT: [char; 32] = [
            '\u{20AC}', '\u{0081}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}',
            '\u{2021}', '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008D}',
            '\u{017D}', '\u{008F}', '\u{0090}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}',
            '\u{2022}', '\u{2013}', '\u{2014}', '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}',
            '\u{0153}', '\u{009D}', '\u{017E}', '\u{0178}',
        ];
        if (0x80..=0x9F).contains(&b) {
            CP1252_EXT[(b - 0x80) as usize]
        } else {
            b as char
        }
    }

    fn decode_cp1252_bytes(slice: &[u8]) -> String {
        slice.iter().map(|&b| Self::decode_cp1252_byte(b)).collect()
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

    fn try_extract_clx_piece_table_text(
        doc_data: &[u8],
        flags: u16,
        streams: &[Ole2StreamEntry],
    ) -> Result<Option<String>, EasyOfficeError> {
        let preferred_table = if (flags & Self::DOC_FLAG_WHICH_TBL_STM) != 0 {
            "1Table"
        } else {
            "0Table"
        };
        let table_entry = streams
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(preferred_table))
            .or_else(|| {
                streams.iter().find(|s| {
                    s.name.eq_ignore_ascii_case("1Table") || s.name.eq_ignore_ascii_case("0Table")
                })
            });
        let Some(table_stream) = table_entry else {
            return Ok(None);
        };
        if table_stream.data.len() < 5 {
            return Ok(None);
        }

        let clx_slice = if doc_data.len() >= 0x01AA {
            let fc_clx = read_u32_le(doc_data, 0x01A2)? as usize;
            let lcb_clx = read_u32_le(doc_data, 0x01A6)? as usize;
            if lcb_clx >= 5 && fc_clx.saturating_add(lcb_clx) <= table_stream.data.len() {
                &table_stream.data[fc_clx..fc_clx + lcb_clx]
            } else {
                table_stream.data.as_slice()
            }
        } else {
            table_stream.data.as_slice()
        };

        let mut pos = 0usize;
        while pos < clx_slice.len() && clx_slice[pos] == 0x01 {
            if pos + 3 > clx_slice.len() {
                return Ok(None);
            }
            let grpprl_len = read_u16_le(clx_slice, pos + 1)? as usize;
            pos = pos.saturating_add(3).saturating_add(grpprl_len);
        }
        if pos + 5 > clx_slice.len() || clx_slice[pos] != 0x02 {
            return Ok(None);
        }
        let lcb = read_u32_le(clx_slice, pos + 1)? as usize;
        let plc_start = pos + 5;
        let plc_end = plc_start.checked_add(lcb).ok_or_else(|| {
            EasyOfficeError::CorruptedContainer("DOC CLX PlcPcd 오버플로".to_string())
        })?;
        if lcb < 16 || (lcb - 4) % 12 != 0 || plc_end > clx_slice.len() {
            return Err(EasyOfficeError::CorruptedContainer(
                "DOC CLX PlcPcd 길이가 유효하지 않습니다.".to_string(),
            ));
        }
        let piece_count = (lcb - 4) / 12;
        if piece_count > 65_536 {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "DOC CLX 조각(piece) 개수가 보안 상한을 초과했습니다.".to_string(),
            ));
        }
        let plc = &clx_slice[plc_start..plc_end];
        let mut out = String::new();
        for i in 0..piece_count {
            let cp_start = read_u32_le(plc, i * 4)? as usize;
            let cp_end = read_u32_le(plc, (i + 1) * 4)? as usize;
            if cp_end < cp_start {
                return Err(EasyOfficeError::CorruptedContainer(
                    "DOC CLX CP 배열이 단조 증가하지 않습니다.".to_string(),
                ));
            }
            let char_count = cp_end - cp_start;
            let pcd_off = (piece_count + 1) * 4 + i * 8;
            let fc_raw = read_u32_le(plc, pcd_off + 2)?;
            let is_compressed = (fc_raw & 0x4000_0000) != 0;
            if is_compressed {
                let byte_offset = ((fc_raw & 0x3FFF_FFFF) as usize) / 2;
                let byte_end = byte_offset.checked_add(char_count).ok_or_else(|| {
                    EasyOfficeError::CorruptedContainer("DOC CLX 8비트 조각 오버플로".to_string())
                })?;
                if byte_end > doc_data.len() {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "DOC CLX 8비트 조각이 스트림 범위를 벗어났습니다.".to_string(),
                    ));
                }
                out.push_str(&Self::decode_cp1252_bytes(&doc_data[byte_offset..byte_end]));
            } else {
                let byte_offset = (fc_raw & 0x3FFF_FFFF) as usize;
                let byte_len = char_count.checked_mul(2).ok_or_else(|| {
                    EasyOfficeError::CorruptedContainer(
                        "DOC CLX UTF-16LE 조각 오버플로".to_string(),
                    )
                })?;
                let byte_end = byte_offset.checked_add(byte_len).ok_or_else(|| {
                    EasyOfficeError::CorruptedContainer(
                        "DOC CLX UTF-16LE 조각 오프셋 오버플로".to_string(),
                    )
                })?;
                if byte_end > doc_data.len() {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "DOC CLX UTF-16LE 조각이 스트림 범위를 벗어났습니다.".to_string(),
                    ));
                }
                out.push_str(&decode_utf16le(&doc_data[byte_offset..byte_end])?);
            }
        }
        Ok(Some(out))
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
        let ccp_text = read_u32_le(&doc_stream.data, 32)? as usize;

        if fc_min >= 64
            && fc_mac >= fc_min
            && fc_mac + 4 <= doc_stream.data.len()
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

        if let Some(clx_text) =
            Self::try_extract_clx_piece_table_text(&doc_stream.data, flags, &streams)?
        {
            let paragraphs: Vec<String> = clx_text
                .split(['\r', '\n'])
                .filter(|s| !s.is_empty())
                .map(ToString::to_string)
                .collect();
            return Ok(LegacyDocPayload {
                title: "문서".to_string(),
                paragraphs,
                tables: Vec::new(),
            });
        }

        if fc_min < 64 || fc_mac < fc_min || fc_mac > doc_stream.data.len() {
            return Err(EasyOfficeError::CorruptedContainer(
                "OLE2 문서 텍스트 오프셋 범위가 손상되었습니다.".to_string(),
            ));
        }

        let raw_slice = &doc_stream.data[fc_min..fc_mac];
        let raw_text = if (ccp_text > 0 && raw_slice.len() == ccp_text) || raw_slice.len() % 2 != 0
        {
            Self::decode_cp1252_bytes(raw_slice)
        } else {
            decode_utf16le(raw_slice)?
        };
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
            Ok(Self::decode_cp1252_bytes(slice))
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
        let mut current_text_type: Option<u32> = None;
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
            } else if s_type == Self::RT_TEXT_HEADER_ATOM && s_len >= 4 {
                current_text_type = Some(read_u32_le(data, a_start)?);
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
                if let Some(tt) = current_text_type.take() {
                    if tt == 0 && slide.title.is_empty() {
                        slide.title = txt;
                    } else if tt == 2 {
                        slide.notes = txt;
                    } else {
                        push_bullets(&mut slide.bullets, txt);
                    }
                } else {
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
        let mut pending_text_type: Option<u32> = None;
        let mut persist_created_empty = false;

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
                persist_created_empty = false;
            } else if rec_type == Self::RT_SLIDE_PERSIST_ATOM {
                *saw_valid_ppt = true;
                let reuse_last = slides.last().is_some_and(|s| {
                    s.title.is_empty() && s.bullets.is_empty() && s.notes.is_empty()
                });
                if !reuse_last {
                    slides.push(LegacyPptSlide {
                        title: String::new(),
                        bullets: Vec::new(),
                        notes: String::new(),
                        theme: "SignatureLavender".to_string(),
                        transition: "Fade".to_string(),
                    });
                }
                persist_created_empty = true;
            } else if rec_type == Self::RT_TEXT_HEADER_ATOM && rec_len >= 4 {
                *saw_valid_ppt = true;
                let tt = read_u32_le(data, r_start)?;
                pending_text_type = Some(tt);
                if tt == 0 && !persist_created_empty {
                    if let Some(last) = slides.last() {
                        if !last.title.is_empty() || !last.bullets.is_empty() {
                            slides.push(LegacyPptSlide {
                                title: String::new(),
                                bullets: Vec::new(),
                                notes: String::new(),
                                theme: "SignatureLavender".to_string(),
                                transition: "Fade".to_string(),
                            });
                        }
                    }
                }
            } else if rec_type == Self::RT_TEXT_CHARS_ATOM || rec_type == Self::RT_TEXT_BYTES_ATOM {
                *saw_valid_ppt = true;
                let txt = Self::decode_ppt_text_atom(rec_type, &data[r_start..r_end])?;
                let tt = pending_text_type.take();
                persist_created_empty = false;
                if ver_inst == 0x0000 && depth <= 1 && title == "발표자료" && tt.is_none() {
                    *title = txt;
                } else if depth <= 1 && (ver_inst == 0x0010 || ver_inst == 0x0020) && tt.is_none() {
                } else if let Some(last_slide) = slides.last_mut() {
                    if tt == Some(2) {
                        last_slide.notes = txt;
                    } else if last_slide.title.is_empty() && (tt == Some(0) || tt.is_none()) {
                        last_slide.title = txt;
                    } else if txt.contains('\r') {
                        for part in txt.split('\r') {
                            last_slide.bullets.push(part.to_string());
                        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FormatDetector, Hwp5Codec, Hwp5DocumentPayload, Hwp5ParagraphRecord};

    #[test]
    fn test_ole2_hwp5_and_legacy_codecs_roundtrip() {
        let hwp_payload = Hwp5DocumentPayload {
            version: (5, 0, 3, 0),
            is_template: false,
            agency_name: "행정안전부 디지털정부실".to_string(),
            document_number: "고시 제2026-42호".to_string(),
            paragraphs: vec![
                Hwp5ParagraphRecord {
                    text: "1. 표준 전자문서 호환성 검증".to_string(),
                    style_name: "기관표제".to_string(),
                    is_official_header: true,
                },
                Hwp5ParagraphRecord {
                    text: "가. 32비트 레코드 헤더 및 UTF-16LE 본문 보존".to_string(),
                    style_name: "본문".to_string(),
                    is_official_header: false,
                },
            ],
            table_cells: vec![vec!["항목".to_string(), "상태".to_string()]],
        };
        let hwp_bytes = Hwp5Codec::pack(&hwp_payload).unwrap();
        assert_eq!(
            FormatDetector::detect(&hwp_bytes, Some("test.hwp")).unwrap(),
            DocumentFormat::Hwp
        );
        let decoded_hwp = Hwp5Codec::unpack(&hwp_bytes).unwrap();
        assert_eq!(decoded_hwp, hwp_payload);

        let xls_payload = LegacyXlsPayload {
            sheet_name: "예산시트".to_string(),
            cells: vec![
                (0, 0, "항목".to_string()),
                (0, 1, "1500".to_string()),
                (0, 2, "=B1*2".to_string()),
            ],
        };
        let xls_bytes = LegacyOle2Codec::pack_xls(&xls_payload).unwrap();
        let decoded_xls = LegacyOle2Codec::unpack_xls(&xls_bytes).unwrap();
        assert_eq!(decoded_xls, xls_payload);
    }

    #[test]
    fn test_hwp5_para_text_control_chars_and_cfb_dir_cmp() {
        assert_eq!(
            Ole2ContainerCodec::cmp_cfb_dir_names("DocInfo", "docinfo"),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            Ole2ContainerCodec::cmp_cfb_dir_names("a", "BB"),
            std::cmp::Ordering::Less
        );

        const HWP5_CTRL_SECTION_COL: u16 = 0x0002;
        const HWP5_CTRL_DRAWING_TABLE: u16 = 0x000B;
        const HWP5_CTRL_INLINE_TAB: u16 = 0x0009;
        const HWP5_CTRL_HYPHEN: u16 = 0x0018;
        let mut payload = Vec::new();
        payload.extend_from_slice(&encode_utf16le("문단앞"));
        payload.extend_from_slice(&HWP5_CTRL_SECTION_COL.to_le_bytes());
        for i in 0..6u16 {
            payload.extend_from_slice(&(0xAC00 + i).to_le_bytes());
        }
        payload.extend_from_slice(&HWP5_CTRL_SECTION_COL.to_le_bytes());
        payload.extend_from_slice(&HWP5_CTRL_DRAWING_TABLE.to_le_bytes());
        for i in 0..6u16 {
            payload.extend_from_slice(&(0xAC00 + i).to_le_bytes());
        }
        payload.extend_from_slice(&HWP5_CTRL_DRAWING_TABLE.to_le_bytes());
        payload.extend_from_slice(&HWP5_CTRL_INLINE_TAB.to_le_bytes());
        payload.extend_from_slice(&[0u8; 12]);
        payload.extend_from_slice(&HWP5_CTRL_INLINE_TAB.to_le_bytes());
        payload.extend_from_slice(&HWP5_CTRL_HYPHEN.to_le_bytes());
        payload.extend_from_slice(&encode_utf16le("문단뒤"));
        payload.extend_from_slice(&0x000Au16.to_le_bytes());
        payload.extend_from_slice(&encode_utf16le("둘째줄"));
        payload.extend_from_slice(&0x000Du16.to_le_bytes());

        let decoded = Hwp5Codec::decode_hwp5_para_text(&payload).unwrap();
        assert_eq!(decoded, "문단앞\t\u{00AD}문단뒤\n둘째줄");
    }
}

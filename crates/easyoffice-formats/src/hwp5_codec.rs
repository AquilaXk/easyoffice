use crate::ole2_codec::{
    append_len_prefixed_utf16le, decode_utf16le, encode_utf16le, read_len_prefixed_utf16le,
    read_u16_le, read_u32_le, write_u32_le, Ole2ContainerCodec, Ole2StreamEntry,
};
use crate::{EasyOfficeError, Ole2SecurityGuard};

/// Paragraph record inside an HWP 5.0 `BodyText/Section0` stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hwp5ParagraphRecord {
    pub text: String,
    pub style_name: String,
    pub is_official_header: bool,
}

/// Decoded HWP 5.0 (`.hwp`) or HWT (`.hwt`) document payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hwp5DocumentPayload {
    pub version: (u8, u8, u8, u8),
    pub is_template: bool,
    pub agency_name: String,
    pub document_number: String,
    pub paragraphs: Vec<Hwp5ParagraphRecord>,
    pub table_cells: Vec<Vec<String>>,
}

pub type DocumentCodecPayload = Hwp5DocumentPayload;
pub type Hwp5TableGrid = Vec<Vec<String>>;

/// HWP 5.0 (`.hwp`) and HWT (`.hwt`) OLE2 container codec using 32-bit HWP record headers.
pub struct Hwp5Codec;

impl Hwp5Codec {
    pub const HWPTAG_DOCUMENT_PROPERTIES: u16 = 16;
    pub const HWPTAG_PARA_HEADER: u16 = 66;
    pub const HWPTAG_PARA_TEXT: u16 = 67;
    pub const HWPTAG_TABLE: u16 = 77;
    pub const HWPTAG_TABLE_RECORD: u16 = 77;
    pub const STD_HWPTAG_PARA_HEADER: u16 = 50;
    pub const STD_HWPTAG_PARA_TEXT: u16 = 51;
    pub const STD_HWPTAG_LIST_HEADER: u16 = 56;
    pub const ALT_HWPTAG_LIST_HEADER: u16 = 72;
    pub const STD_HWPTAG_TABLE: u16 = 61;
    pub const HWT_TEMPLATE_MARKER: &'static [u8; 12] = b"HWT_TEMPLATE";
    pub const MAX_HWP5_RECORDS: usize = 524_288;
    pub const MAX_TOTAL_HWP5_TABLE_CELLS: usize = 65_536;

    pub fn encode_record_header(tag_id: u16, level: u16, size: usize) -> Vec<u8> {
        let tag_bits = (tag_id as u32) & 0x03FF;
        let level_bits = ((level as u32) & 0x03FF) << 10;
        let mut out = Vec::with_capacity(8);
        if size < 0x0FFF {
            let word = tag_bits | level_bits | (((size as u32) & 0x0FFF) << 20);
            out.extend_from_slice(&word.to_le_bytes());
        } else {
            let word = tag_bits | level_bits | (0x0FFFu32 << 20);
            out.extend_from_slice(&word.to_le_bytes());
            out.extend_from_slice(&(size as u32).to_le_bytes());
        }
        out
    }

    pub fn decode_record_header(
        bytes: &[u8],
        offset: usize,
    ) -> Result<(u16, u16, usize, usize), EasyOfficeError> {
        let word = read_u32_le(bytes, offset)?;
        let tag_id = (word & 0x03FF) as u16;
        let level = ((word >> 10) & 0x03FF) as u16;
        let inline_size = ((word >> 20) & 0x0FFF) as usize;
        if inline_size == 0x0FFF {
            let ext_size = read_u32_le(bytes, offset + 4)? as usize;
            Ok((tag_id, level, ext_size, 8))
        } else {
            Ok((tag_id, level, inline_size, 4))
        }
    }

    pub fn pack(payload: &Hwp5DocumentPayload) -> Result<Vec<u8>, EasyOfficeError> {
        Self::pack_with_compression_and_tables(payload, &[], false)
    }

    pub fn pack_multi_tables(
        payload: &Hwp5DocumentPayload,
        extra_tables: &[Hwp5TableGrid],
    ) -> Result<Vec<u8>, EasyOfficeError> {
        Self::pack_with_compression_and_tables(payload, extra_tables, false)
    }

    pub fn pack_compressed(payload: &Hwp5DocumentPayload) -> Result<Vec<u8>, EasyOfficeError> {
        Self::pack_with_compression_and_tables(payload, &[], true)
    }

    fn validate_table_bounds(
        tbl: &[Vec<String>],
        total_cells: &mut usize,
    ) -> Result<(), EasyOfficeError> {
        if tbl.len() > crate::MAX_HWP_TABLE_ROWS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "HWP 5.0 표 행 개수({})가 보안 상한({})을 초과했습니다.",
                tbl.len(),
                crate::MAX_HWP_TABLE_ROWS
            )));
        }
        for row in tbl {
            if row.len() > crate::MAX_HWP_TABLE_COLS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "HWP 5.0 표 열 개수({})가 보안 상한({})을 초과했습니다.",
                    row.len(),
                    crate::MAX_HWP_TABLE_COLS
                )));
            }
            *total_cells = total_cells.saturating_add(row.len());
            if *total_cells > Self::MAX_TOTAL_HWP5_TABLE_CELLS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "HWP 5.0 누적 표 셀 개수가 보안 상한({})을 초과했습니다.",
                    Self::MAX_TOTAL_HWP5_TABLE_CELLS
                )));
            }
        }
        Ok(())
    }

    fn pack_with_compression_and_tables(
        payload: &Hwp5DocumentPayload,
        extra_tables: &[Hwp5TableGrid],
        compressed: bool,
    ) -> Result<Vec<u8>, EasyOfficeError> {
        let mut total_cells = 0usize;
        for tbl in std::iter::once(&payload.table_cells).chain(extra_tables.iter()) {
            Self::validate_table_bounds(tbl, &mut total_cells)?;
        }

        let mut file_header = vec![0u8; Ole2SecurityGuard::HWP5_FILE_HEADER_LEN];
        file_header[..17].copy_from_slice(b"HWP Document File");
        file_header[32] = payload.version.3;
        file_header[33] = payload.version.2;
        file_header[34] = payload.version.1;
        file_header[35] = payload.version.0;
        let flags = if compressed {
            Ole2SecurityGuard::HWP5_FLAG_COMPRESSED
        } else {
            0x0000_0000
        };
        write_u32_le(&mut file_header, 36, flags);
        if payload.is_template {
            file_header[40..52].copy_from_slice(Self::HWT_TEMPLATE_MARKER);
        }

        let mut doc_props = Vec::new();
        append_len_prefixed_utf16le(&mut doc_props, &payload.agency_name);
        append_len_prefixed_utf16le(&mut doc_props, &payload.document_number);
        let mut doc_info_stream =
            Self::encode_record_header(Self::HWPTAG_DOCUMENT_PROPERTIES, 0, doc_props.len());
        doc_info_stream.extend_from_slice(&doc_props);

        let mut section0 = Vec::new();
        for para in &payload.paragraphs {
            let mut p_header = Vec::new();
            append_len_prefixed_utf16le(&mut p_header, &para.style_name);
            p_header.push(if para.is_official_header { 1 } else { 0 });
            section0.extend_from_slice(&Self::encode_record_header(
                Self::HWPTAG_PARA_HEADER,
                0,
                p_header.len(),
            ));
            section0.extend_from_slice(&p_header);

            let text_utf16 = encode_utf16le(&para.text);
            section0.extend_from_slice(&Self::encode_record_header(
                Self::HWPTAG_PARA_TEXT,
                1,
                text_utf16.len(),
            ));
            section0.extend_from_slice(&text_utf16);
        }

        for tbl in std::iter::once(&payload.table_cells).chain(extra_tables.iter()) {
            if !tbl.is_empty() {
                let mut tbl_buf = Vec::new();
                tbl_buf.extend_from_slice(&(tbl.len() as u16).to_le_bytes());
                for row in tbl {
                    tbl_buf.extend_from_slice(&(row.len() as u16).to_le_bytes());
                    for cell in row {
                        append_len_prefixed_utf16le(&mut tbl_buf, cell);
                    }
                }
                section0.extend_from_slice(&Self::encode_record_header(
                    Self::HWPTAG_TABLE_RECORD,
                    0,
                    tbl_buf.len(),
                ));
                section0.extend_from_slice(&tbl_buf);
            }
        }

        let (final_doc_info, final_section0) = if compressed {
            (
                crate::deflate::deflate_raw(&doc_info_stream),
                crate::deflate::deflate_raw(&section0),
            )
        } else {
            (doc_info_stream, section0)
        };

        Ole2ContainerCodec::pack(&[
            ("FileHeader".to_string(), file_header),
            ("DocInfo".to_string(), final_doc_info),
            ("BodyText/Section0".to_string(), final_section0),
        ])
    }

    pub fn unpack(bytes: &[u8]) -> Result<Hwp5DocumentPayload, EasyOfficeError> {
        Ok(Self::unpack_multi_tables(bytes)?.0)
    }

    fn collect_ordered_section_streams(streams: &[Ole2StreamEntry]) -> Vec<&Ole2StreamEntry> {
        let mut body_sections: Vec<(usize, &Ole2StreamEntry)> = Vec::new();
        let mut direct_sections: Vec<(usize, &Ole2StreamEntry)> = Vec::new();

        for s in streams {
            if let Some(rest) = s.name.strip_prefix("BodyText/Section") {
                if let Ok(idx) = rest.parse::<usize>() {
                    body_sections.push((idx, s));
                }
            } else if let Some(rest) = s.name.strip_prefix("Section") {
                if let Ok(idx) = rest.parse::<usize>() {
                    direct_sections.push((idx, s));
                }
            }
        }

        let mut chosen = if !body_sections.is_empty() {
            body_sections
        } else {
            direct_sections
        };
        chosen.sort_by_key(|(idx, _)| *idx);
        chosen.into_iter().map(|(_, entry)| entry).collect()
    }

    pub fn unpack_multi_tables(
        bytes: &[u8],
    ) -> Result<(Hwp5DocumentPayload, Vec<Hwp5TableGrid>), EasyOfficeError> {
        let streams = Ole2ContainerCodec::unpack(bytes)?;
        let file_header = streams
            .iter()
            .find(|s| s.name == "FileHeader")
            .ok_or_else(|| {
                EasyOfficeError::CorruptedContainer(
                    "HWP 5.0 필수 스트림 'FileHeader'가 누락되었습니다.".to_string(),
                )
            })?;

        let raw_ver = Ole2SecurityGuard::validate_hwp5_file_header(&file_header.data)?;
        let version = (
            ((raw_ver >> 24) & 0xFF) as u8,
            ((raw_ver >> 16) & 0xFF) as u8,
            ((raw_ver >> 8) & 0xFF) as u8,
            (raw_ver & 0xFF) as u8,
        );
        let flags = read_u32_le(&file_header.data, 36)?;
        let is_compressed = (flags & Ole2SecurityGuard::HWP5_FLAG_COMPRESSED) != 0;
        let is_template =
            file_header.data.len() >= 52 && &file_header.data[40..52] == Self::HWT_TEMPLATE_MARKER;

        let mut total_records = 0usize;
        let mut total_table_cells = 0usize;
        let mut agency_name = String::new();
        let mut document_number = String::new();
        if let Some(doc_info) = streams.iter().find(|s| s.name == "DocInfo") {
            if !doc_info.data.is_empty() {
                let doc_info_bytes: std::borrow::Cow<'_, [u8]> = if is_compressed {
                    std::borrow::Cow::Owned(crate::deflate::inflate_auto(
                        &doc_info.data,
                        crate::ZipSecurityGuard::MAX_SINGLE_ENTRY_UNCOMPRESSED_BYTES,
                    )?)
                } else {
                    std::borrow::Cow::Borrowed(&doc_info.data)
                };

                let mut offset = 0usize;
                while offset < doc_info_bytes.len() {
                    total_records += 1;
                    if total_records > Self::MAX_HWP5_RECORDS {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                            "HWP 5.0 레코드 개수가 보안 상한({})을 초과했습니다.",
                            Self::MAX_HWP5_RECORDS
                        )));
                    }
                    let (tag_id, _level, size, hdr_len) =
                        Self::decode_record_header(&doc_info_bytes, offset)?;
                    let start = offset + hdr_len;
                    let end = start.checked_add(size).ok_or_else(|| {
                        EasyOfficeError::CorruptedContainer(
                            "HWP DocInfo 레코드 오버플로".to_string(),
                        )
                    })?;
                    if end > doc_info_bytes.len() {
                        return Err(EasyOfficeError::CorruptedContainer(
                            "HWP DocInfo 레코드 경계 초과".to_string(),
                        ));
                    }
                    if tag_id == Self::HWPTAG_DOCUMENT_PROPERTIES && agency_name.is_empty() {
                        Self::parse_doc_properties(
                            &doc_info_bytes,
                            start,
                            end,
                            &mut agency_name,
                            &mut document_number,
                        )?;
                    }
                    offset = end;
                }
            }
        }

        let section_entries = Self::collect_ordered_section_streams(&streams);
        if section_entries.is_empty() {
            return Err(EasyOfficeError::CorruptedContainer(
                "HWP 5.0 필수 스트림 'BodyText/Section0'가 누락되었습니다.".to_string(),
            ));
        }

        let mut paragraphs: Vec<Hwp5ParagraphRecord> = Vec::new();
        let mut table_cells: Vec<Vec<String>> = Vec::new();
        let mut all_tables: Vec<Vec<Vec<String>>> = Vec::new();

        for sec_entry in section_entries {
            let section_bytes: std::borrow::Cow<'_, [u8]> = if is_compressed {
                std::borrow::Cow::Owned(crate::deflate::inflate_auto(
                    &sec_entry.data,
                    crate::ZipSecurityGuard::MAX_SINGLE_ENTRY_UNCOMPRESSED_BYTES,
                )?)
            } else {
                std::borrow::Cow::Borrowed(&sec_entry.data)
            };

            Self::parse_section_records(
                &section_bytes,
                &mut total_records,
                &mut total_table_cells,
                &mut paragraphs,
                &mut table_cells,
                &mut all_tables,
            )?;
        }

        Ok((
            Hwp5DocumentPayload {
                version,
                is_template,
                agency_name,
                document_number,
                paragraphs,
                table_cells,
            },
            all_tables,
        ))
    }

    fn peek_next_tag_id(bytes: &[u8], offset: usize) -> Option<u16> {
        if offset + 4 > bytes.len() {
            return None;
        }
        let word = read_u32_le(bytes, offset).ok()?;
        Some((word & 0x03FF) as u16)
    }

    fn parse_section_records(
        section_bytes: &[u8],
        total_records: &mut usize,
        total_table_cells: &mut usize,
        paragraphs: &mut Vec<Hwp5ParagraphRecord>,
        table_cells: &mut Vec<Vec<String>>,
        all_tables: &mut Vec<Vec<Vec<String>>>,
    ) -> Result<(), EasyOfficeError> {
        let mut pending_meta: Option<(String, bool)> = None;
        let mut offset = 0usize;

        while offset < section_bytes.len() {
            *total_records += 1;
            if *total_records > Self::MAX_HWP5_RECORDS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "HWP 5.0 레코드 개수가 보안 상한({})을 초과했습니다.",
                    Self::MAX_HWP5_RECORDS
                )));
            }
            let (tag_id, level, size, hdr_len) = Self::decode_record_header(section_bytes, offset)?;
            let start = offset + hdr_len;
            let end = start.checked_add(size).ok_or_else(|| {
                EasyOfficeError::CorruptedContainer("HWP Section 레코드 오버플로".to_string())
            })?;
            if end > section_bytes.len() {
                return Err(EasyOfficeError::CorruptedContainer(
                    "HWP Section 레코드 경계 초과".to_string(),
                ));
            }

            match tag_id {
                Self::HWPTAG_PARA_HEADER | Self::STD_HWPTAG_PARA_HEADER => {
                    pending_meta = Some(Self::parse_para_header(section_bytes, start, end)?);
                    offset = end;
                }
                Self::HWPTAG_PARA_TEXT | Self::STD_HWPTAG_PARA_TEXT => {
                    let text = Self::decode_hwp5_para_text(&section_bytes[start..end])?;
                    let (style_name, is_official_header) = pending_meta
                        .take()
                        .unwrap_or_else(|| ("본문".to_string(), false));
                    paragraphs.push(Hwp5ParagraphRecord {
                        text,
                        style_name,
                        is_official_header,
                    });
                    offset = end;
                }
                Self::HWPTAG_TABLE_RECORD | Self::STD_HWPTAG_TABLE => {
                    let next_tag = Self::peek_next_tag_id(section_bytes, end);
                    if matches!(
                        next_tag,
                        Some(Self::STD_HWPTAG_LIST_HEADER | Self::ALT_HWPTAG_LIST_HEADER)
                    ) {
                        let (n_rows, n_cols) =
                            Self::extract_std_table_dims(&section_bytes[start..end])?;
                        offset = end;
                        let parsed_rows = Self::consume_std_table_cells(
                            section_bytes,
                            &mut offset,
                            total_records,
                            level,
                            n_rows,
                            n_cols,
                            total_table_cells,
                        )?;
                        if !parsed_rows.is_empty() {
                            table_cells.extend(parsed_rows.iter().cloned());
                            all_tables.push(parsed_rows);
                        }
                    } else {
                        let parsed_rows =
                            Self::parse_table_record(section_bytes, start, end, total_table_cells)?;
                        if !parsed_rows.is_empty() {
                            table_cells.extend(parsed_rows.iter().cloned());
                            all_tables.push(parsed_rows);
                        }
                        offset = end;
                    }
                }
                _ => {
                    offset = end;
                }
            }
        }
        Ok(())
    }

    fn extract_std_table_dims(slice: &[u8]) -> Result<(usize, usize), EasyOfficeError> {
        let (rows, cols) = if slice.len() >= 8 {
            let r4 = read_u16_le(slice, 4)? as usize;
            let c6 = read_u16_le(slice, 6)? as usize;
            if r4 > 0 && c6 > 0 {
                (r4, c6)
            } else {
                (
                    read_u16_le(slice, 0)? as usize,
                    read_u16_le(slice, 2)? as usize,
                )
            }
        } else if slice.len() >= 4 {
            (
                read_u16_le(slice, 0)? as usize,
                read_u16_le(slice, 2)? as usize,
            )
        } else {
            return Err(EasyOfficeError::CorruptedContainer(
                "HWP 5.0 HWPTAG_TABLE 헤더가 너무 짧습니다.".to_string(),
            ));
        };
        if rows > crate::MAX_HWP_TABLE_ROWS || cols > crate::MAX_HWP_TABLE_COLS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "HWP 5.0 표 행·열 개수가 보안 상한을 초과했습니다.".to_string(),
            ));
        }
        Ok((rows, cols))
    }

    fn consume_std_table_cells(
        section_bytes: &[u8],
        offset: &mut usize,
        total_records: &mut usize,
        table_level: u16,
        n_rows: usize,
        n_cols: usize,
        total_table_cells: &mut usize,
    ) -> Result<Vec<Vec<String>>, EasyOfficeError> {
        let expected_cells = n_rows.checked_mul(n_cols).ok_or_else(|| {
            EasyOfficeError::ResourceBudgetExceeded("HWP 5.0 표 셀 오버플로".to_string())
        })?;
        if total_table_cells.saturating_add(expected_cells) > Self::MAX_TOTAL_HWP5_TABLE_CELLS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "HWP 5.0 누적 표 셀 개수가 보안 상한({})을 초과했습니다.",
                Self::MAX_TOTAL_HWP5_TABLE_CELLS
            )));
        }

        let mut flat_cells: Vec<Vec<String>> = Vec::with_capacity(expected_cells);
        let mut expected_paras_in_cell: Option<usize> = None;
        let mut list_hdr_level: Option<u16> = None;

        while *offset < section_bytes.len() {
            let saved = *offset;
            let (tag_id, rec_level, size, hdr_len) =
                Self::decode_record_header(section_bytes, *offset)?;
            let start = *offset + hdr_len;
            let end = start.checked_add(size).ok_or_else(|| {
                EasyOfficeError::CorruptedContainer("HWP 표 셀 레코드 오버플로".to_string())
            })?;
            if end > section_bytes.len() {
                return Err(EasyOfficeError::CorruptedContainer(
                    "HWP 표 셀 레코드 경계 초과".to_string(),
                ));
            }

            if tag_id == Self::STD_HWPTAG_LIST_HEADER || tag_id == Self::ALT_HWPTAG_LIST_HEADER {
                if flat_cells.len() >= expected_cells && expected_cells > 0 {
                    *offset = saved;
                    break;
                }
                *total_records += 1;
                if *total_records > Self::MAX_HWP5_RECORDS {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "HWP 5.0 레코드 개수 초과".to_string(),
                    ));
                }
                if list_hdr_level.is_none() {
                    list_hdr_level = Some(rec_level);
                }
                expected_paras_in_cell = if size >= 2 {
                    let v = read_u16_le(section_bytes, start)? as usize;
                    if v > 0 {
                        Some(v)
                    } else {
                        None
                    }
                } else {
                    None
                };
                flat_cells.push(Vec::new());
                *offset = end;
                continue;
            }

            if flat_cells.len() >= expected_cells {
                let last_done = match (flat_cells.last(), expected_paras_in_cell) {
                    (Some(p), Some(exp)) => p.len() >= exp,
                    (Some(p), None) => !p.is_empty(),
                    _ => true,
                };
                if last_done
                    && (rec_level <= table_level
                        || list_hdr_level.is_some_and(|lh| {
                            rec_level <= lh
                                && matches!(
                                    tag_id,
                                    Self::HWPTAG_PARA_HEADER
                                        | Self::STD_HWPTAG_PARA_HEADER
                                        | Self::HWPTAG_PARA_TEXT
                                        | Self::STD_HWPTAG_PARA_TEXT
                                        | Self::HWPTAG_TABLE_RECORD
                                        | Self::STD_HWPTAG_TABLE
                                        | 55
                                        | 71
                                )
                        }))
                {
                    *offset = saved;
                    break;
                }
            }

            if rec_level < table_level {
                *offset = saved;
                break;
            }

            *total_records += 1;
            if *total_records > Self::MAX_HWP5_RECORDS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "HWP 5.0 레코드 개수 초과".to_string(),
                ));
            }
            if tag_id == Self::HWPTAG_PARA_TEXT || tag_id == Self::STD_HWPTAG_PARA_TEXT {
                let txt = Self::decode_hwp5_para_text(&section_bytes[start..end])?;
                if let Some(cell_paras) = flat_cells.last_mut() {
                    cell_paras.push(txt);
                }
            }
            *offset = end;
        }

        if n_rows == 0 || n_cols == 0 {
            return Ok(Vec::new());
        }
        let mut rows = Vec::with_capacity(n_rows);
        let mut iter = flat_cells.into_iter().map(|p| p.join("\n"));
        for _ in 0..n_rows {
            let mut row = Vec::with_capacity(n_cols);
            for _ in 0..n_cols {
                row.push(iter.next().unwrap_or_default());
            }
            rows.push(row);
        }
        *total_table_cells += expected_cells;
        Ok(rows)
    }

    pub(crate) fn decode_hwp5_para_text(bytes: &[u8]) -> Result<String, EasyOfficeError> {
        if bytes.len() % 2 != 0 {
            return Err(EasyOfficeError::CorruptedContainer(
                "HWP 5.0 문단 텍스트 페이로드 길이가 2바이트(WCHAR) 배수가 아닙니다.".to_string(),
            ));
        }
        let has_control = bytes
            .chunks_exact(2)
            .any(|c| u16::from_le_bytes([c[0], c[1]]) < 0x0020);
        if !has_control {
            return decode_utf16le(bytes);
        }

        let mut filtered = Vec::with_capacity(bytes.len());
        let mut cursor = 0usize;
        while cursor < bytes.len() {
            let code = u16::from_le_bytes([bytes[cursor], bytes[cursor + 1]]);
            if code < 0x0020 {
                match code {
                    0x0009 => {
                        filtered.extend_from_slice(&0x0009u16.to_le_bytes());
                        if cursor + 16 <= bytes.len()
                            && u16::from_le_bytes([bytes[cursor + 14], bytes[cursor + 15]])
                                == 0x0009
                            && bytes[cursor + 2..cursor + 14].iter().all(|&b| b == 0)
                        {
                            cursor += 16;
                        } else {
                            cursor += 2;
                        }
                    }
                    0x000A => {
                        filtered.extend_from_slice(&0x000Au16.to_le_bytes());
                        cursor += 2;
                    }
                    0x000D => {
                        if cursor + 2 < bytes.len() {
                            filtered.extend_from_slice(&0x000Au16.to_le_bytes());
                        }
                        cursor += 2;
                    }
                    0x0001..=0x0008 | 0x000B..=0x000C | 0x000E..=0x0017
                        if cursor + 16 <= bytes.len()
                            && u16::from_le_bytes([bytes[cursor + 14], bytes[cursor + 15]])
                                == code =>
                    {
                        cursor += 16;
                    }
                    _ => {
                        cursor += 2;
                    }
                }
            } else {
                filtered.push(bytes[cursor]);
                filtered.push(bytes[cursor + 1]);
                cursor += 2;
            }
        }
        decode_utf16le(&filtered)
    }

    fn is_printable_text(s: &str) -> bool {
        !s.chars()
            .any(|c| (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r')
    }

    fn parse_doc_properties(
        data: &[u8],
        start: usize,
        end: usize,
        agency_name: &mut String,
        document_number: &mut String,
    ) -> Result<(), EasyOfficeError> {
        let mut cursor = start;
        if let Ok(s1) = read_len_prefixed_utf16le(data, &mut cursor, end) {
            if Self::is_printable_text(&s1) {
                if let Ok(s2) = read_len_prefixed_utf16le(data, &mut cursor, end) {
                    if Self::is_printable_text(&s2) {
                        if cursor == end {
                            *agency_name = s1;
                            *document_number = s2;
                            return Ok(());
                        }
                        if let Ok(s3) = read_len_prefixed_utf16le(data, &mut cursor, end) {
                            if Self::is_printable_text(&s3)
                                && read_len_prefixed_utf16le(data, &mut cursor, end).is_ok()
                                && cursor < end
                            {
                                *agency_name = s1;
                                *document_number = if !s3.is_empty() { s3 } else { s2 };
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }
        let slice = &data[start..end];
        if slice.len() == 26 {
            return Ok(());
        }
        if slice.len() >= 4 {
            let n = read_u32_le(slice, 0)? as usize;
            if 4 + n == slice.len() && n % 2 == 0 {
                if let Ok(decoded) = decode_utf16le(&slice[4..]) {
                    if Self::is_printable_text(&decoded) {
                        *agency_name = decoded;
                        return Ok(());
                    }
                }
            }
        }
        if slice.len() % 2 == 0 && !slice.is_empty() {
            if let Ok(decoded) = decode_utf16le(slice) {
                if Self::is_printable_text(&decoded) {
                    *agency_name = decoded;
                }
            }
        }
        Ok(())
    }

    fn parse_para_header(
        data: &[u8],
        start: usize,
        end: usize,
    ) -> Result<(String, bool), EasyOfficeError> {
        if end - start == 4 {
            return Ok(("본문".to_string(), data[start + 1] != 0));
        }
        let mut cursor = start;
        if let Ok(style) = read_len_prefixed_utf16le(data, &mut cursor, end) {
            if Self::is_printable_text(&style) {
                if cursor + 1 == end {
                    return Ok((style, data[cursor] != 0));
                }
                if read_len_prefixed_utf16le(data, &mut cursor, end).is_ok()
                    && read_len_prefixed_utf16le(data, &mut cursor, end).is_ok()
                    && cursor + 9 <= end
                {
                    let flags_or_bold = data[cursor + 8];
                    let is_official = (flags_or_bold & 0x01) != 0
                        || style.contains("표제")
                        || style.contains("제목");
                    return Ok((style, is_official));
                }
            }
        }
        Ok(("본문".to_string(), false))
    }

    fn try_parse_u16_rows(
        data: &[u8],
        start: usize,
        end: usize,
        total_table_cells: &mut usize,
    ) -> Result<Option<Vec<Vec<String>>>, EasyOfficeError> {
        if start + 4 > end {
            return Ok(None);
        }
        let mut cur = start;
        let Ok(row_count_u16) = read_u16_le(data, cur) else {
            return Ok(None);
        };
        let Ok(first_col_u16) = read_u16_le(data, cur + 2) else {
            return Ok(None);
        };
        let row_count = row_count_u16 as usize;
        let first_col = first_col_u16 as usize;
        cur += 2;
        if row_count == 0 || first_col == 0 {
            return Ok(None);
        }
        if row_count > crate::MAX_HWP_TABLE_ROWS
            || first_col > crate::MAX_HWP_TABLE_COLS
            || row_count.saturating_mul(first_col) > Self::MAX_TOTAL_HWP5_TABLE_CELLS
        {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "HWP 5.0 표 행·열·셀 개수가 보안 상한을 초과했습니다.".to_string(),
            ));
        }
        if row_count.saturating_mul(6) > end.saturating_sub(cur) {
            return Ok(None);
        }
        let mut rows = Vec::with_capacity(row_count.min(256));
        let mut local_cells = 0usize;
        for _ in 0..row_count {
            let Ok(col_count_u16) = read_u16_le(data, cur) else {
                return Ok(None);
            };
            let col_count = col_count_u16 as usize;
            cur += 2;
            if col_count == 0 {
                return Ok(None);
            }
            if col_count > crate::MAX_HWP_TABLE_COLS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "HWP 5.0 표 열 개수가 보안 상한(256)을 초과했습니다.".to_string(),
                ));
            }
            if col_count.saturating_mul(4) > end.saturating_sub(cur) {
                return Ok(None);
            }
            local_cells = local_cells.saturating_add(col_count);
            if total_table_cells.saturating_add(local_cells) > Self::MAX_TOTAL_HWP5_TABLE_CELLS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "HWP 5.0 누적 표 셀 개수가 보안 상한({})을 초과했습니다.",
                    Self::MAX_TOTAL_HWP5_TABLE_CELLS
                )));
            }
            let mut row = Vec::with_capacity(col_count.min(256));
            for _ in 0..col_count {
                let Ok(cell) = read_len_prefixed_utf16le(data, &mut cur, end) else {
                    return Ok(None);
                };
                row.push(cell);
            }
            rows.push(row);
        }
        if cur == end {
            *total_table_cells += local_cells;
            Ok(Some(rows))
        } else {
            Ok(None)
        }
    }

    fn parse_table_record(
        data: &[u8],
        start: usize,
        end: usize,
        total_table_cells: &mut usize,
    ) -> Result<Vec<Vec<String>>, EasyOfficeError> {
        if let Some(rows) = Self::try_parse_u16_rows(data, start, end, total_table_cells)? {
            return Ok(rows);
        }
        let mut cur = start;
        if read_len_prefixed_utf16le(data, &mut cur, end).is_ok() && cur < end {
            let _has_header = data[cur] != 0;
            cur += 1;
            if let Some(rows) = Self::try_parse_u16_rows(data, cur, end, total_table_cells)? {
                return Ok(rows);
            }
            if cur + 8 <= end {
                let row_count = read_u32_le(data, cur)? as usize;
                let col_count = read_u32_le(data, cur + 4)? as usize;
                cur += 8;
                let table_cells = row_count.saturating_mul(col_count);
                if row_count == 0
                    || col_count == 0
                    || row_count > crate::MAX_HWP_TABLE_ROWS
                    || col_count > crate::MAX_HWP_TABLE_COLS
                    || table_cells.saturating_mul(4) > end.saturating_sub(cur)
                    || total_table_cells.saturating_add(table_cells)
                        > Self::MAX_TOTAL_HWP5_TABLE_CELLS
                {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "HWP 5.0 표 행·열 개수가 보안 상한을 초과했습니다.".to_string(),
                    ));
                }
                let mut rows = Vec::with_capacity(row_count.min(256));
                let mut ok = true;
                for _ in 0..row_count {
                    let mut row = Vec::with_capacity(col_count.min(256));
                    for _ in 0..col_count {
                        if let Ok(cell) = read_len_prefixed_utf16le(data, &mut cur, end) {
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
                if ok && cur == end {
                    *total_table_cells += table_cells;
                    return Ok(rows);
                }
            }
        }
        if start + 4 <= end && (end - (start + 4)) % 2 == 0 {
            let payload_slice = &data[start + 4..end];
            if payload_slice.len() > 65_536 {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "HWP 5.0 표 텍스트 페이로드가 보안 상한을 초과했습니다.".to_string(),
                ));
            }
            if payload_slice
                .chunks_exact(2)
                .any(|c| c[0] == 0 && c[1] == 0)
            {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "HWP 5.0 표 레코드 셀 한도 초과 또는 비정상 바이너리 구조입니다.".to_string(),
                ));
            }
            let txt = decode_utf16le(payload_slice)?;
            let mut rows = Vec::new();
            let mut local_cells = 0usize;
            for r in txt.split('\n') {
                if rows.len() >= crate::MAX_HWP_TABLE_ROWS {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "HWP 5.0 표 행 개수가 보안 상한(4096)을 초과했습니다.".to_string(),
                    ));
                }
                let mut row = Vec::new();
                for c in r.split('\t') {
                    if row.len() >= crate::MAX_HWP_TABLE_COLS {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(
                            "HWP 5.0 표 열 개수가 보안 상한(256)을 초과했습니다.".to_string(),
                        ));
                    }
                    local_cells += 1;
                    if total_table_cells.saturating_add(local_cells)
                        > Self::MAX_TOTAL_HWP5_TABLE_CELLS
                    {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                            "HWP 5.0 누적 표 셀 개수가 보안 상한({})을 초과했습니다.",
                            Self::MAX_TOTAL_HWP5_TABLE_CELLS
                        )));
                    }
                    row.push(c.to_string());
                }
                rows.push(row);
            }
            *total_table_cells += local_cells;
            return Ok(rows);
        }
        Err(EasyOfficeError::CorruptedContainer(
            "HWP 5.0 표 레코드(HWPTAG_TABLE) 구조가 손상되었습니다.".to_string(),
        ))
    }
}

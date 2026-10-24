use crate::{ContainerFamily, EasyOfficeError, FormatDetector, Ole2SecurityGuard};
use std::collections::HashSet;

/// Named stream inside an OLE2 Compound File Binary Format (CFBF) container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ole2StreamEntry {
    pub name: String,
    pub data: Vec<u8>,
}

impl Ole2StreamEntry {
    pub fn new(name: impl Into<String>, data: Vec<u8>) -> Self {
        Self {
            name: name.into(),
            data,
        }
    }
}

/// 512-byte sector-aligned OLE2 CFBF (`D0 CF 11 E0 A1 B1 1A E1`) container packer and unpacker.
pub struct Ole2ContainerCodec;

impl Ole2ContainerCodec {
    pub const SECTOR_SIZE: usize = 512;
    pub const DIR_ENTRY_SIZE: usize = 128;
    pub const ENTRIES_PER_SECTOR: usize = Self::SECTOR_SIZE / Self::DIR_ENTRY_SIZE;
    pub const FAT_ENTRIES_PER_SECTOR: usize = Self::SECTOR_SIZE / 4;
    pub const FAT_SECTOR_MARKER: u32 = 0xFFFF_FFFD;
    pub const END_OF_CHAIN: u32 = Ole2SecurityGuard::END_OF_CHAIN;
    pub const FREE_SECTOR: u32 = Ole2SecurityGuard::FREE_SECTOR;

    const OBJ_TYPE_STREAM: u8 = 2;
    const OBJ_TYPE_ROOT: u8 = 5;

    pub fn pack(streams: &[(String, Vec<u8>)]) -> Result<Vec<u8>, EasyOfficeError> {
        if streams.is_empty() {
            return Err(EasyOfficeError::EmptyPayload);
        }

        let total_dir_entries = 1 + streams.len();
        let dir_sector_count = total_dir_entries.div_ceil(Self::ENTRIES_PER_SECTOR).max(1);

        let mut next_sector_idx = dir_sector_count as u32;
        let mut stream_start_sectors: Vec<u32> = Vec::with_capacity(streams.len());
        let mut stream_sector_counts: Vec<usize> = Vec::with_capacity(streams.len());

        for (name, data) in streams {
            if name.trim().is_empty() {
                return Err(EasyOfficeError::InvalidArgument(
                    "OLE2 스트림 이름은 비어 있을 수 없습니다.".to_string(),
                ));
            }
            if data.is_empty() {
                stream_start_sectors.push(Self::END_OF_CHAIN);
                stream_sector_counts.push(0);
            } else {
                let count = data.len().div_ceil(Self::SECTOR_SIZE);
                stream_start_sectors.push(next_sector_idx);
                stream_sector_counts.push(count);
                next_sector_idx = next_sector_idx.checked_add(count as u32).ok_or_else(|| {
                    EasyOfficeError::CorruptedContainer("OLE2 섹터 인덱스 오버플로".to_string())
                })?;
            }
        }

        let content_sector_count = next_sector_idx as usize;
        let fat_sector_count = content_sector_count
            .div_ceil(Self::FAT_ENTRIES_PER_SECTOR)
            .max(1);
        if fat_sector_count > Ole2SecurityGuard::MAX_FAT_SECTORS {
            return Err(EasyOfficeError::SecurityViolation(
                "OLE2 DIFAT 109 섹터 한도를 초과했습니다.".to_string(),
            ));
        }

        let first_fat_sector_idx = content_sector_count;
        let mut fat = vec![Self::FREE_SECTOR; fat_sector_count * Self::FAT_ENTRIES_PER_SECTOR];

        for (sec, slot) in fat.iter_mut().enumerate().take(dir_sector_count) {
            *slot = if sec + 1 < dir_sector_count {
                (sec + 1) as u32
            } else {
                Self::END_OF_CHAIN
            };
        }

        for idx in 0..streams.len() {
            let count = stream_sector_counts[idx];
            if count == 0 {
                continue;
            }
            let start = stream_start_sectors[idx] as usize;
            for offset in 0..count {
                let cur = start + offset;
                fat[cur] = if offset + 1 < count {
                    (cur + 1) as u32
                } else {
                    Self::END_OF_CHAIN
                };
            }
        }

        for f_idx in 0..fat_sector_count {
            let slot = first_fat_sector_idx + f_idx;
            if slot < fat.len() {
                fat[slot] = Self::FAT_SECTOR_MARKER;
            }
        }

        let total_sectors = content_sector_count + fat_sector_count;
        let mut container = vec![0u8; Self::SECTOR_SIZE];
        container[..8].copy_from_slice(&FormatDetector::OLE2_MAGIC);
        write_u16_le(&mut container, 24, 0x003E); // minor version
        write_u16_le(&mut container, 26, 0x0003); // major version 3
        write_u16_le(&mut container, 28, 0xFFFE); // little-endian byte order
        write_u16_le(&mut container, 30, 0x0009); // sector_shift = 9 (512 bytes)
        write_u16_le(&mut container, 32, 0x0006); // mini_sector_shift = 6 (64 bytes)
        write_u32_le(&mut container, 44, fat_sector_count as u32);
        write_u32_le(&mut container, 48, 0); // first directory sector = 0
        write_u32_le(&mut container, 56, 0x0000_1000); // mini stream cutoff
        write_u32_le(&mut container, 60, Self::END_OF_CHAIN);
        write_u32_le(&mut container, 64, 0);
        write_u32_le(&mut container, 68, Self::END_OF_CHAIN);
        write_u32_le(&mut container, 72, 0);

        for slot in 0..Ole2SecurityGuard::MAX_FAT_SECTORS {
            let val = if slot < fat_sector_count {
                (first_fat_sector_idx + slot) as u32
            } else {
                Self::FREE_SECTOR
            };
            write_u32_le(&mut container, 76 + slot * 4, val);
        }

        let mut dir_bytes = vec![0u8; dir_sector_count * Self::SECTOR_SIZE];
        Self::write_dir_entry(
            &mut dir_bytes,
            0,
            "Root Entry",
            Self::OBJ_TYPE_ROOT,
            if streams.is_empty() {
                Self::FREE_SECTOR
            } else {
                1
            },
            Self::FREE_SECTOR,
            Self::END_OF_CHAIN,
            0,
        )?;

        for (s_idx, (name, data)) in streams.iter().enumerate() {
            let entry_idx = s_idx + 1;
            let right_sibling = if s_idx + 1 < streams.len() {
                (entry_idx + 1) as u32
            } else {
                Self::FREE_SECTOR
            };
            Self::write_dir_entry(
                &mut dir_bytes,
                entry_idx,
                name,
                Self::OBJ_TYPE_STREAM,
                Self::FREE_SECTOR,
                right_sibling,
                stream_start_sectors[s_idx],
                data.len() as u64,
            )?;
        }
        container.extend_from_slice(&dir_bytes);

        for (_, data) in streams {
            if !data.is_empty() {
                container.extend_from_slice(data);
                let rem = data.len() % Self::SECTOR_SIZE;
                if rem != 0 {
                    container.resize(container.len() + (Self::SECTOR_SIZE - rem), 0);
                }
            }
        }

        for &entry in &fat {
            container.extend_from_slice(&entry.to_le_bytes());
        }

        debug_assert_eq!(container.len(), (1 + total_sectors) * Self::SECTOR_SIZE);
        Ok(container)
    }

    pub fn unpack(bytes: &[u8]) -> Result<Vec<Ole2StreamEntry>, EasyOfficeError> {
        let fat = Ole2SecurityGuard::validate_header_and_fat(bytes)?;
        let first_dir_sector = read_u32_le(bytes, 48)?;
        let dir_chain = Ole2SecurityGuard::validate_sector_chain(bytes, &fat, first_dir_sector)?;

        let mut dir_bytes = Vec::with_capacity(dir_chain.len() * Self::SECTOR_SIZE);
        for &sec in &dir_chain {
            let start = (1 + sec as usize) * Self::SECTOR_SIZE;
            dir_bytes.extend_from_slice(&bytes[start..start + Self::SECTOR_SIZE]);
        }

        let entry_count = dir_bytes.len() / Self::DIR_ENTRY_SIZE;
        let mut streams: Vec<Ole2StreamEntry> = Vec::new();
        let mut claimed_sectors: HashSet<u32> = dir_chain.into_iter().collect();

        for idx in 0..entry_count {
            let base = idx * Self::DIR_ENTRY_SIZE;
            let obj_type = dir_bytes[base + 66];
            if obj_type != Self::OBJ_TYPE_STREAM {
                continue;
            }
            let name_byte_len = read_u16_le(&dir_bytes, base + 64)? as usize;
            if !(2..=64).contains(&name_byte_len) || !name_byte_len.is_multiple_of(2) {
                return Err(EasyOfficeError::CorruptedContainer(format!(
                    "OLE2 디렉터리 엔트리 이름 길이 오류: {name_byte_len}"
                )));
            }
            let name = decode_utf16le(&dir_bytes[base..base + name_byte_len - 2])?;
            let start_sector = read_u32_le(&dir_bytes, base + 116)?;
            let stream_size = read_u64_le(&dir_bytes, base + 120)? as usize;

            if stream_size > bytes.len() {
                return Err(EasyOfficeError::CorruptedContainer(format!(
                    "OLE2 스트림('{name}') 크기가 컨테이너 크기를 초과합니다."
                )));
            }

            if stream_size == 0 {
                streams.push(Ole2StreamEntry::new(name, Vec::new()));
            } else {
                let chain = Ole2SecurityGuard::validate_sector_chain(bytes, &fat, start_sector)?;
                let mut payload = Vec::with_capacity(chain.len() * Self::SECTOR_SIZE);
                for sec in chain {
                    if !claimed_sectors.insert(sec) {
                        return Err(EasyOfficeError::SecurityViolation(
                            "OLE2 스트림 간 섹터 공유 또는 중복 참조가 감지되었습니다.".to_string(),
                        ));
                    }
                    let offset = (1 + sec as usize) * Self::SECTOR_SIZE;
                    payload.extend_from_slice(&bytes[offset..offset + Self::SECTOR_SIZE]);
                }
                if payload.len() < stream_size {
                    return Err(EasyOfficeError::CorruptedContainer(format!(
                        "OLE2 스트림('{name}') 섹터 체인 길이가 부족합니다."
                    )));
                }
                payload.truncate(stream_size);
                streams.push(Ole2StreamEntry::new(name, payload));
            }
        }

        Ok(streams)
    }

    #[allow(clippy::too_many_arguments)]
    fn write_dir_entry(
        dir_bytes: &mut [u8],
        entry_idx: usize,
        name: &str,
        obj_type: u8,
        child_id: u32,
        right_sibling_id: u32,
        start_sector: u32,
        stream_size: u64,
    ) -> Result<(), EasyOfficeError> {
        let base = entry_idx * Self::DIR_ENTRY_SIZE;
        let units: Vec<u16> = name.encode_utf16().collect();
        if units.len() > 31 {
            return Err(EasyOfficeError::InvalidArgument(format!(
                "OLE2 디렉터리 이름 길이(31 UTF-16 코드 유닛) 초과: {name}"
            )));
        }
        for (i, &u) in units.iter().enumerate() {
            write_u16_le(dir_bytes, base + i * 2, u);
        }
        let name_byte_len = ((units.len() + 1) * 2) as u16;
        write_u16_le(dir_bytes, base + 64, name_byte_len);
        dir_bytes[base + 66] = obj_type;
        dir_bytes[base + 67] = 1; // black node color
        write_u32_le(dir_bytes, base + 68, Self::FREE_SECTOR); // left sibling
        write_u32_le(dir_bytes, base + 72, right_sibling_id);
        write_u32_le(dir_bytes, base + 76, child_id);
        write_u32_le(dir_bytes, base + 116, start_sector);
        write_u64_le(dir_bytes, base + 120, stream_size);
        Ok(())
    }
}

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

/// HWP 5.0 (`.hwp`) and HWT (`.hwt`) OLE2 container codec using 32-bit HWP record headers.
pub struct Hwp5Codec;

impl Hwp5Codec {
    pub const HWPTAG_DOCUMENT_PROPERTIES: u16 = 16;
    pub const HWPTAG_PARA_HEADER: u16 = 66;
    pub const HWPTAG_PARA_TEXT: u16 = 67;
    pub const HWPTAG_TABLE: u16 = 77;
    pub const HWPTAG_TABLE_RECORD: u16 = 77;
    pub const HWT_TEMPLATE_MARKER: &'static [u8; 12] = b"HWT_TEMPLATE";

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
        Self::pack_with_compression(payload, false)
    }

    pub fn pack_compressed(payload: &Hwp5DocumentPayload) -> Result<Vec<u8>, EasyOfficeError> {
        Self::pack_with_compression(payload, true)
    }

    fn pack_with_compression(
        payload: &Hwp5DocumentPayload,
        compressed: bool,
    ) -> Result<Vec<u8>, EasyOfficeError> {
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

        if !payload.table_cells.is_empty() {
            let mut tbl_buf = Vec::new();
            tbl_buf.extend_from_slice(&(payload.table_cells.len() as u16).to_le_bytes());
            for row in &payload.table_cells {
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
                    if tag_id == Self::HWPTAG_DOCUMENT_PROPERTIES {
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

        let section0 = streams
            .iter()
            .find(|s| s.name == "BodyText/Section0")
            .ok_or_else(|| {
                EasyOfficeError::CorruptedContainer(
                    "HWP 5.0 필수 스트림 'BodyText/Section0'가 누락되었습니다.".to_string(),
                )
            })?;

        let section0_bytes: std::borrow::Cow<'_, [u8]> = if is_compressed {
            std::borrow::Cow::Owned(crate::deflate::inflate_auto(
                &section0.data,
                crate::ZipSecurityGuard::MAX_SINGLE_ENTRY_UNCOMPRESSED_BYTES,
            )?)
        } else {
            std::borrow::Cow::Borrowed(&section0.data)
        };

        let mut paragraphs: Vec<Hwp5ParagraphRecord> = Vec::new();
        let mut table_cells: Vec<Vec<String>> = Vec::new();
        let mut pending_meta: Option<(String, bool)> = None;
        let mut offset = 0usize;

        while offset < section0_bytes.len() {
            let (tag_id, _level, size, hdr_len) =
                Self::decode_record_header(&section0_bytes, offset)?;
            let start = offset + hdr_len;
            let end = start.checked_add(size).ok_or_else(|| {
                EasyOfficeError::CorruptedContainer("HWP Section0 레코드 오버플로".to_string())
            })?;
            if end > section0_bytes.len() {
                return Err(EasyOfficeError::CorruptedContainer(
                    "HWP Section0 레코드 경계 초과".to_string(),
                ));
            }

            match tag_id {
                Self::HWPTAG_PARA_HEADER => {
                    pending_meta = Some(Self::parse_para_header(&section0_bytes, start, end)?);
                }
                Self::HWPTAG_PARA_TEXT => {
                    let text = decode_utf16le(&section0_bytes[start..end])?;
                    let (style_name, is_official_header) = pending_meta
                        .take()
                        .unwrap_or_else(|| ("본문".to_string(), false));
                    paragraphs.push(Hwp5ParagraphRecord {
                        text,
                        style_name,
                        is_official_header,
                    });
                }
                Self::HWPTAG_TABLE_RECORD => {
                    let parsed_rows = Self::parse_table_record(&section0_bytes, start, end)?;
                    table_cells.extend(parsed_rows);
                }
                _ => {}
            }
            offset = end;
        }

        Ok(Hwp5DocumentPayload {
            version,
            is_template,
            agency_name,
            document_number,
            paragraphs,
            table_cells,
        })
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
            if let Ok(s2) = read_len_prefixed_utf16le(data, &mut cursor, end) {
                if cursor == end {
                    *agency_name = s1;
                    *document_number = s2;
                    return Ok(());
                }
                if let Ok(s3) = read_len_prefixed_utf16le(data, &mut cursor, end) {
                    if read_len_prefixed_utf16le(data, &mut cursor, end).is_ok() && cursor < end {
                        *agency_name = s1;
                        *document_number = if !s3.is_empty() { s3 } else { s2 };
                        return Ok(());
                    }
                }
            }
        }
        let slice = &data[start..end];
        if slice.len() >= 4 {
            let n = read_u32_le(slice, 0)? as usize;
            if 4 + n == slice.len() && n.is_multiple_of(2) {
                *agency_name = decode_utf16le(&slice[4..])?;
                return Ok(());
            }
            if 4 + n * 2 <= slice.len() {
                *agency_name = decode_utf16le(&slice[4..4 + n * 2])?;
                return Ok(());
            }
        }
        if slice.len().is_multiple_of(2) {
            *agency_name = decode_utf16le(slice)?;
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
            if cursor + 1 == end {
                return Ok((style, data[cursor] != 0));
            }
            if read_len_prefixed_utf16le(data, &mut cursor, end).is_ok()
                && read_len_prefixed_utf16le(data, &mut cursor, end).is_ok()
                && cursor + 9 <= end
            {
                let flags_or_bold = data[cursor + 8];
                let is_official =
                    (flags_or_bold & 0x01) != 0 || style.contains("표제") || style.contains("제목");
                return Ok((style, is_official));
            }
        }
        Ok(("본문".to_string(), false))
    }

    fn try_parse_u16_rows(data: &[u8], start: usize, end: usize) -> Option<Vec<Vec<String>>> {
        if start + 2 > end {
            return None;
        }
        let mut cur = start;
        let row_count = read_u16_le(data, cur).ok()? as usize;
        cur += 2;
        let mut rows = Vec::with_capacity(row_count.min(256));
        for _ in 0..row_count {
            let col_count = read_u16_le(data, cur).ok()? as usize;
            cur += 2;
            let mut row = Vec::with_capacity(col_count.min(256));
            for _ in 0..col_count {
                row.push(read_len_prefixed_utf16le(data, &mut cur, end).ok()?);
            }
            rows.push(row);
        }
        (cur == end).then_some(rows)
    }

    fn parse_table_record(
        data: &[u8],
        start: usize,
        end: usize,
    ) -> Result<Vec<Vec<String>>, EasyOfficeError> {
        if let Some(rows) = Self::try_parse_u16_rows(data, start, end) {
            return Ok(rows);
        }
        let mut cur = start;
        if read_len_prefixed_utf16le(data, &mut cur, end).is_ok() && cur < end {
            let _has_header = data[cur] != 0;
            cur += 1;
            if let Some(rows) = Self::try_parse_u16_rows(data, cur, end) {
                return Ok(rows);
            }
            if cur + 8 <= end {
                let row_count = read_u32_le(data, cur)? as usize;
                let col_count = read_u32_le(data, cur + 4)? as usize;
                cur += 8;
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
                    return Ok(rows);
                }
            }
        }
        if start + 4 <= end && (end - (start + 4)).is_multiple_of(2) {
            let txt = decode_utf16le(&data[start + 4..end])?;
            return Ok(txt
                .split('\n')
                .map(|r| r.split('\t').map(ToString::to_string).collect())
                .collect());
        }
        Err(EasyOfficeError::CorruptedContainer(
            "HWP 5.0 표 레코드(HWPTAG_TABLE) 구조가 손상되었습니다.".to_string(),
        ))
    }
}

pub use crate::legacy_ole2_codec::*;

pub(crate) fn encode_utf16le(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 2);
    for u in text.encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out
}

pub(crate) fn decode_utf16le(bytes: &[u8]) -> Result<String, EasyOfficeError> {
    if !bytes.len().is_multiple_of(2) {
        return Err(EasyOfficeError::CorruptedContainer(
            "UTF-16LE 바이트 길이가 짝수가 아닙니다.".to_string(),
        ));
    }
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16(&units).map_err(|_| {
        EasyOfficeError::InvalidUtf8("유효하지 않은 UTF-16LE 시퀀스입니다.".to_string())
    })
}

pub(crate) fn append_len_prefixed_utf16le(buf: &mut Vec<u8>, text: &str) {
    let bytes = encode_utf16le(text);
    buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(&bytes);
}

pub(crate) fn read_len_prefixed_utf16le(
    bytes: &[u8],
    cursor: &mut usize,
    limit: usize,
) -> Result<String, EasyOfficeError> {
    if *cursor + 4 > limit {
        return Err(EasyOfficeError::CorruptedContainer(
            "UTF-16LE 길이 프리픽스 읽기 범위 초과".to_string(),
        ));
    }
    let byte_len = read_u32_le(bytes, *cursor)? as usize;
    *cursor += 4;
    if !byte_len.is_multiple_of(2) || *cursor + byte_len > limit {
        return Err(EasyOfficeError::CorruptedContainer(
            "UTF-16LE 문자열 페이로드 범위 초과".to_string(),
        ));
    }
    let s = decode_utf16le(&bytes[*cursor..*cursor + byte_len])?;
    *cursor += byte_len;
    Ok(s)
}

#[inline]
pub(crate) fn write_u16_le(buf: &mut [u8], offset: usize, val: u16) {
    buf[offset..offset + 2].copy_from_slice(&val.to_le_bytes());
}

#[inline]
pub(crate) fn write_u32_le(buf: &mut [u8], offset: usize, val: u32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

#[inline]
pub(crate) fn write_u64_le(buf: &mut [u8], offset: usize, val: u64) {
    buf[offset..offset + 8].copy_from_slice(&val.to_le_bytes());
}

#[inline]
pub(crate) fn read_u16_le(bytes: &[u8], offset: usize) -> Result<u16, EasyOfficeError> {
    if offset + 2 > bytes.len() {
        return Err(EasyOfficeError::CorruptedContainer(
            "OLE2 u16 읽기 범위 초과".to_string(),
        ));
    }
    Ok(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]))
}

#[inline]
pub(crate) fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32, EasyOfficeError> {
    if offset + 4 > bytes.len() {
        return Err(EasyOfficeError::CorruptedContainer(
            "OLE2 u32 읽기 범위 초과".to_string(),
        ));
    }
    Ok(u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]))
}

#[inline]
pub(crate) fn read_u64_le(bytes: &[u8], offset: usize) -> Result<u64, EasyOfficeError> {
    if offset + 8 > bytes.len() {
        return Err(EasyOfficeError::CorruptedContainer(
            "OLE2 u64 읽기 범위 초과".to_string(),
        ));
    }
    Ok(u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ]))
}

impl Ole2SecurityGuard {
    pub fn validate_header_and_fat(container_bytes: &[u8]) -> Result<Vec<u32>, EasyOfficeError> {
        if container_bytes.len() < Self::SECTOR_SIZE * 2
            || !container_bytes.len().is_multiple_of(Self::SECTOR_SIZE)
        {
            return Err(EasyOfficeError::CorruptedContainer(
                "OLE2 컨테이너 크기가 512바이트 섹터 배수가 아닙니다.".to_string(),
            ));
        }
        if !container_bytes.starts_with(&FormatDetector::OLE2_MAGIC) {
            return Err(EasyOfficeError::SignatureMismatch {
                expected: ContainerFamily::Ole2Cfbf,
                detail: "OLE2 CFBF 시그니처 불일치".to_string(),
            });
        }
        let sector_shift = read_u16_le(container_bytes, 30)?;
        let mini_shift = read_u16_le(container_bytes, 32)?;
        if sector_shift != 9 || mini_shift != 6 {
            return Err(EasyOfficeError::CorruptedContainer(format!(
                "유효하지 않은 OLE2 섹터 시프트({sector_shift}/{mini_shift})"
            )));
        }
        let fat_sector_count = read_u32_le(container_bytes, 44)? as usize;
        if fat_sector_count == 0 || fat_sector_count > Self::MAX_FAT_SECTORS {
            return Err(EasyOfficeError::SecurityViolation(format!(
                "OLE2 DIFAT 섹터 개수({fat_sector_count})가 허용 범위(1..={})를 벗어났습니다.",
                Self::MAX_FAT_SECTORS
            )));
        }

        let total_sectors = (container_bytes.len() / Self::SECTOR_SIZE) - 1;
        let mut fat = Vec::with_capacity(fat_sector_count * 128);
        for slot in 0..fat_sector_count {
            let fat_sec_idx = read_u32_le(container_bytes, 76 + slot * 4)? as usize;
            if fat_sec_idx >= total_sectors {
                return Err(EasyOfficeError::CorruptedContainer(
                    "OLE2 FAT 섹터 인덱스가 파일 범위를 벗어났습니다.".to_string(),
                ));
            }
            let sec_offset = (1 + fat_sec_idx) * Self::SECTOR_SIZE;
            for entry_idx in 0..128 {
                let val = read_u32_le(container_bytes, sec_offset + entry_idx * 4)?;
                fat.push(val);
            }
        }

        let first_dir_sector = read_u32_le(container_bytes, 48)?;
        Self::validate_sector_chain(container_bytes, &fat, first_dir_sector)?;
        Ok(fat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DocumentFormat;

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
}

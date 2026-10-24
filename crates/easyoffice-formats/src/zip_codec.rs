use crate::{
    ContainerFamily, EasyOfficeError, FormatDetector, ZipEntryDescriptor, ZipSecurityGuard,
    ZipSecurityPolicy,
};

/// IEEE 802.3 CRC-32 (`0xEDB88320` reflected polynomial) Slicing-by-8 checksum calculator.
pub struct IeeeCrc32;

impl IeeeCrc32 {
    pub const POLYNOMIAL: u32 = 0xEDB8_8320;

    pub const TABLES: [[u32; 256]; 8] = {
        let mut tables = [[0u32; 256]; 8];
        let mut i = 0usize;
        while i < 256 {
            let mut crc = i as u32;
            let mut bit = 0;
            while bit < 8 {
                if (crc & 1) != 0 {
                    crc = (crc >> 1) ^ 0xEDB8_8320;
                } else {
                    crc >>= 1;
                }
                bit += 1;
            }
            tables[0][i] = crc;
            i += 1;
        }
        let mut t = 1usize;
        while t < 8 {
            let mut j = 0usize;
            while j < 256 {
                let prev = tables[t - 1][j];
                tables[t][j] = (prev >> 8) ^ tables[0][(prev & 0xFF) as usize];
                j += 1;
            }
            t += 1;
        }
        tables
    };

    #[inline]
    pub fn checksum(bytes: &[u8]) -> u32 {
        let mut crc: u32 = 0xFFFF_FFFF;
        let mut offset = 0usize;
        while offset + 8 <= bytes.len() {
            let one = u32::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ]) ^ crc;
            let two = u32::from_le_bytes([
                bytes[offset + 4],
                bytes[offset + 5],
                bytes[offset + 6],
                bytes[offset + 7],
            ]);
            crc = Self::TABLES[7][(one & 0xFF) as usize]
                ^ Self::TABLES[6][((one >> 8) & 0xFF) as usize]
                ^ Self::TABLES[5][((one >> 16) & 0xFF) as usize]
                ^ Self::TABLES[4][(one >> 24) as usize]
                ^ Self::TABLES[3][(two & 0xFF) as usize]
                ^ Self::TABLES[2][((two >> 8) & 0xFF) as usize]
                ^ Self::TABLES[1][((two >> 16) & 0xFF) as usize]
                ^ Self::TABLES[0][(two >> 24) as usize];
            offset += 8;
        }
        while offset < bytes.len() {
            let idx = ((crc ^ (bytes[offset] as u32)) & 0xFF) as usize;
            crc = (crc >> 8) ^ Self::TABLES[0][idx];
            offset += 1;
        }
        !crc
    }

    #[inline]
    pub fn checksum_byte_by_byte_baseline(bytes: &[u8]) -> u32 {
        let mut crc: u32 = 0xFFFF_FFFF;
        for &b in bytes {
            let idx = ((crc ^ (b as u32)) & 0xFF) as usize;
            crc = (crc >> 8) ^ Self::TABLES[0][idx];
        }
        !crc
    }
}

/// Extracted entry from a PKZIP container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipArchiveEntry {
    pub path: String,
    pub data: Vec<u8>,
    pub crc32: u32,
}

pub type ZipEntry = ZipArchiveEntry;

impl ZipArchiveEntry {
    pub fn new(path: impl Into<String>, data: Vec<u8>) -> Self {
        let crc32 = IeeeCrc32::checksum(&data);
        Self {
            path: path.into(),
            data,
            crc32,
        }
    }
}

pub trait AsZipEntry {
    fn entry_path(&self) -> &str;
    fn entry_data(&self) -> &[u8];
}

impl AsZipEntry for ZipArchiveEntry {
    fn entry_path(&self) -> &str {
        &self.path
    }
    fn entry_data(&self) -> &[u8] {
        &self.data
    }
}

impl AsZipEntry for (String, Vec<u8>) {
    fn entry_path(&self) -> &str {
        &self.0
    }
    fn entry_data(&self) -> &[u8] {
        &self.1
    }
}

impl AsZipEntry for (&str, &[u8]) {
    fn entry_path(&self) -> &str {
        self.0
    }
    fn entry_data(&self) -> &[u8] {
        self.1
    }
}

impl AsZipEntry for (&str, Vec<u8>) {
    fn entry_path(&self) -> &str {
        self.0
    }
    fn entry_data(&self) -> &[u8] {
        &self.1
    }
}

/// Standard PKZIP (`0x04034b50` / `0x02014b50` / `0x06054b50`) container packer and unpacker.
pub struct ZipArchiveCodec;

impl ZipArchiveCodec {
    pub const LOCAL_FILE_HEADER_SIG: u32 = 0x0403_4B50;
    pub const CENTRAL_DIR_HEADER_SIG: u32 = 0x0201_4B50;
    pub const END_OF_CENTRAL_DIR_SIG: u32 = 0x0605_4B50;
    pub const VERSION_NEEDED: u16 = 20;
    pub const METHOD_STORE: u16 = 0;
    pub const METHOD_DEFLATE: u16 = 8;

    #[inline]
    pub fn crc32(bytes: &[u8]) -> u32 {
        IeeeCrc32::checksum(bytes)
    }

    pub fn pack(entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>, EasyOfficeError> {
        Self::pack_internal(entries, false)
    }

    pub fn pack_deflate<E: AsZipEntry>(entries: &[E]) -> Result<Vec<u8>, EasyOfficeError> {
        Self::pack_internal(entries, true)
    }

    fn pack_internal<E: AsZipEntry>(
        entries: &[E],
        use_deflate: bool,
    ) -> Result<Vec<u8>, EasyOfficeError> {
        if entries.is_empty() {
            return Err(EasyOfficeError::EmptyPayload);
        }
        if entries.len() > u16::MAX as usize {
            return Err(EasyOfficeError::InvalidArgument(
                "PKZIP 엔트리 개수가 65,535개를 초과했습니다.".to_string(),
            ));
        }

        let policy = ZipSecurityPolicy::default();
        let descriptors: Vec<ZipEntryDescriptor> = entries
            .iter()
            .map(|e| {
                let len = e.entry_data().len() as u64;
                ZipEntryDescriptor::new(e.entry_path().to_string(), len, len, false)
            })
            .collect();
        ZipSecurityGuard::validate_archive(&descriptors, &policy)?;

        let mut out: Vec<u8> = Vec::with_capacity(1024);
        let mut central_dir: Vec<u8> = Vec::with_capacity(entries.len() * 64);

        for entry in entries {
            let path = entry.entry_path();
            let data = entry.entry_data();
            ZipSecurityGuard::validate_entry_path(path)?;
            let path_bytes = path.as_bytes();
            if path_bytes.len() > u16::MAX as usize {
                return Err(EasyOfficeError::InvalidArgument(format!(
                    "ZIP 엔트리 경로가 너무 깁니다: {path}"
                )));
            }
            if data.len() > u32::MAX as usize {
                return Err(EasyOfficeError::PayloadTooLarge {
                    actual: data.len(),
                    max: u32::MAX as usize,
                });
            }

            let local_header_offset = out.len() as u32;
            let crc = Self::crc32(data);
            let uncomp_size_u32 = data.len() as u32;
            let name_len = path_bytes.len() as u16;

            let (method, compressed_payload): (u16, std::borrow::Cow<'_, [u8]>) =
                if use_deflate && !data.is_empty() {
                    let deflated = crate::deflate::deflate_raw(data);
                    if deflated.len() < data.len() {
                        (Self::METHOD_DEFLATE, std::borrow::Cow::Owned(deflated))
                    } else {
                        (Self::METHOD_STORE, std::borrow::Cow::Borrowed(data))
                    }
                } else {
                    (Self::METHOD_STORE, std::borrow::Cow::Borrowed(data))
                };
            let comp_size_u32 = compressed_payload.len() as u32;

            // Local File Header (30 bytes + file name)
            out.extend_from_slice(&Self::LOCAL_FILE_HEADER_SIG.to_le_bytes());
            out.extend_from_slice(&Self::VERSION_NEEDED.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // general purpose bit flag
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // mod time
            out.extend_from_slice(&0x0021u16.to_le_bytes()); // mod date
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&comp_size_u32.to_le_bytes());
            out.extend_from_slice(&uncomp_size_u32.to_le_bytes());
            out.extend_from_slice(&name_len.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // extra field length
            out.extend_from_slice(path_bytes);
            out.extend_from_slice(&compressed_payload);

            // Central Directory Header (46 bytes + file name)
            central_dir.extend_from_slice(&Self::CENTRAL_DIR_HEADER_SIG.to_le_bytes());
            central_dir.extend_from_slice(&Self::VERSION_NEEDED.to_le_bytes());
            central_dir.extend_from_slice(&Self::VERSION_NEEDED.to_le_bytes());
            central_dir.extend_from_slice(&0u16.to_le_bytes()); // flags
            central_dir.extend_from_slice(&method.to_le_bytes());
            central_dir.extend_from_slice(&0u16.to_le_bytes()); // mod time
            central_dir.extend_from_slice(&0x0021u16.to_le_bytes()); // mod date
            central_dir.extend_from_slice(&crc.to_le_bytes());
            central_dir.extend_from_slice(&comp_size_u32.to_le_bytes());
            central_dir.extend_from_slice(&uncomp_size_u32.to_le_bytes());
            central_dir.extend_from_slice(&name_len.to_le_bytes());
            central_dir.extend_from_slice(&0u16.to_le_bytes()); // extra len
            central_dir.extend_from_slice(&0u16.to_le_bytes()); // comment len
            central_dir.extend_from_slice(&0u16.to_le_bytes()); // disk number
            central_dir.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            central_dir.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            central_dir.extend_from_slice(&local_header_offset.to_le_bytes());
            central_dir.extend_from_slice(path_bytes);
        }

        let cd_offset = out.len() as u32;
        let cd_size = central_dir.len() as u32;
        let entry_count = entries.len() as u16;
        out.extend_from_slice(&central_dir);

        // End of Central Directory Record (22 bytes)
        out.extend_from_slice(&Self::END_OF_CENTRAL_DIR_SIG.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // disk number
        out.extend_from_slice(&0u16.to_le_bytes()); // cd start disk
        out.extend_from_slice(&entry_count.to_le_bytes());
        out.extend_from_slice(&entry_count.to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // comment length

        Ok(out)
    }

    pub fn unpack(
        bytes: &[u8],
        policy: &ZipSecurityPolicy,
    ) -> Result<Vec<ZipArchiveEntry>, EasyOfficeError> {
        if bytes.is_empty() {
            return Err(EasyOfficeError::EmptyPayload);
        }
        if bytes.len() < 30 || !bytes.starts_with(&FormatDetector::ZIP_MAGIC) {
            return Err(EasyOfficeError::SignatureMismatch {
                expected: ContainerFamily::ZipOoxml,
                detail: "PKZIP(0x04034B50) 시그니처가 없거나 아카이브가 손상되었습니다."
                    .to_string(),
            });
        }

        let eocd_offset = Self::find_eocd_offset(bytes).ok_or_else(|| {
            EasyOfficeError::CorruptedContainer(
                "PKZIP End of Central Directory(0x06054B50) 레코드를 찾을 수 없습니다.".to_string(),
            )
        })?;

        let total_entries = read_u16_le(bytes, eocd_offset + 10)? as usize;
        let cd_size = read_u32_le(bytes, eocd_offset + 12)? as usize;
        let cd_offset = read_u32_le(bytes, eocd_offset + 16)? as usize;

        if cd_offset
            .checked_add(cd_size)
            .is_none_or(|end| end > eocd_offset)
        {
            return Err(EasyOfficeError::CorruptedContainer(
                "PKZIP Central Directory 오프셋 또는 크기가 유효하지 않습니다.".to_string(),
            ));
        }

        let mut descriptors: Vec<ZipEntryDescriptor> = Vec::with_capacity(total_entries.min(4096));
        let mut raw_records: Vec<(String, u16, u32, usize, usize, usize)> =
            Vec::with_capacity(total_entries.min(4096));

        let mut cursor = cd_offset;
        let cd_end = cd_offset + cd_size;

        for _ in 0..total_entries {
            if cursor + 46 > cd_end {
                return Err(EasyOfficeError::CorruptedContainer(
                    "PKZIP Central Directory 헤더가 절단되었습니다.".to_string(),
                ));
            }
            let sig = read_u32_le(bytes, cursor)?;
            if sig != Self::CENTRAL_DIR_HEADER_SIG {
                return Err(EasyOfficeError::CorruptedContainer(
                    "PKZIP Central Directory 시그니처(0x02014B50) 불일치".to_string(),
                ));
            }

            let flags = read_u16_le(bytes, cursor + 8)?;
            let method = read_u16_le(bytes, cursor + 10)?;
            let crc32 = read_u32_le(bytes, cursor + 16)?;
            let compressed_size = read_u32_le(bytes, cursor + 20)? as usize;
            let uncompressed_size = read_u32_le(bytes, cursor + 24)? as usize;
            let name_len = read_u16_le(bytes, cursor + 28)? as usize;
            let extra_len = read_u16_le(bytes, cursor + 30)? as usize;
            let comment_len = read_u16_le(bytes, cursor + 32)? as usize;
            let local_offset = read_u32_le(bytes, cursor + 42)? as usize;

            let name_start = cursor + 46;
            let name_end = name_start.checked_add(name_len).ok_or_else(|| {
                EasyOfficeError::CorruptedContainer("ZIP 이름 길이 오버플로".to_string())
            })?;
            let next_cursor = name_end
                .checked_add(extra_len)
                .and_then(|v| v.checked_add(comment_len))
                .ok_or_else(|| {
                    EasyOfficeError::CorruptedContainer("ZIP 메타데이터 길이 오버플로".to_string())
                })?;

            if next_cursor > cd_end {
                return Err(EasyOfficeError::CorruptedContainer(
                    "PKZIP Central Directory 엔트리 경계 초과".to_string(),
                ));
            }

            let path = std::str::from_utf8(&bytes[name_start..name_end])
                .map_err(|_| {
                    EasyOfficeError::InvalidUtf8(
                        "ZIP 엔트리 경로가 유효한 UTF-8이 아닙니다.".to_string(),
                    )
                })?
                .to_string();

            let is_encrypted = (flags & 0x0001) != 0;
            descriptors.push(ZipEntryDescriptor::new(
                path.clone(),
                compressed_size as u64,
                uncompressed_size as u64,
                is_encrypted,
            ));
            raw_records.push((
                path,
                method,
                crc32,
                compressed_size,
                uncompressed_size,
                local_offset,
            ));

            cursor = next_cursor;
        }

        ZipSecurityGuard::validate_archive(&descriptors, policy)?;

        let mut extracted: Vec<ZipArchiveEntry> = Vec::with_capacity(raw_records.len());
        let mut total_uncompressed: u64 = 0;

        for (path, method, expected_crc, comp_size, uncomp_size, local_offset) in raw_records {
            if method != Self::METHOD_STORE && method != Self::METHOD_DEFLATE {
                return Err(EasyOfficeError::CorruptedContainer(format!(
                    "지원하지 않는 ZIP 압축 방식({method})입니다 (Store=0, Deflate=8 전용)."
                )));
            }
            if method == Self::METHOD_STORE && comp_size != uncomp_size {
                return Err(EasyOfficeError::CorruptedContainer(format!(
                    "Store 엔트리({path})의 압축 크기({comp_size})와 원본 크기({uncomp_size})가 일치하지 않습니다."
                )));
            }
            if local_offset + 30 > cd_offset {
                return Err(EasyOfficeError::CorruptedContainer(format!(
                    "ZIP 로컬 파일 헤더 오프셋({local_offset})이 유효하지 않습니다."
                )));
            }
            let local_sig = read_u32_le(bytes, local_offset)?;
            if local_sig != Self::LOCAL_FILE_HEADER_SIG {
                return Err(EasyOfficeError::CorruptedContainer(
                    "PKZIP 로컬 파일 헤더 시그니처(0x04034B50) 불일치".to_string(),
                ));
            }
            let local_name_len = read_u16_le(bytes, local_offset + 26)? as usize;
            let local_extra_len = read_u16_le(bytes, local_offset + 28)? as usize;
            let data_start = local_offset
                .checked_add(30)
                .and_then(|v| v.checked_add(local_name_len))
                .and_then(|v| v.checked_add(local_extra_len))
                .ok_or_else(|| {
                    EasyOfficeError::CorruptedContainer("ZIP 로컬 헤더 오프셋 오버플로".to_string())
                })?;
            let data_end = data_start.checked_add(comp_size).ok_or_else(|| {
                EasyOfficeError::CorruptedContainer("ZIP 데이터 크기 오버플로".to_string())
            })?;

            if data_end > cd_offset {
                return Err(EasyOfficeError::CorruptedContainer(format!(
                    "ZIP 엔트리({path}) 데이터가 Central Directory 경계를 침범합니다."
                )));
            }

            let compressed_slice = &bytes[data_start..data_end];
            let payload = if method == Self::METHOD_STORE {
                compressed_slice.to_vec()
            } else {
                let decompressed = crate::deflate::inflate_raw(
                    compressed_slice,
                    policy.max_single_entry_uncompressed_bytes as usize,
                )?;
                if decompressed.len() != uncomp_size {
                    return Err(EasyOfficeError::CorruptedContainer(format!(
                        "Deflate 엔트리({path}) 복원 크기 불일치 (expected: {uncomp_size}, actual: {})",
                        decompressed.len()
                    )));
                }
                decompressed
            };

            total_uncompressed = total_uncompressed.saturating_add(payload.len() as u64);
            if total_uncompressed > policy.max_total_uncompressed_bytes {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "ZIP 누적 복원 크기({total_uncompressed})가 한도({})를 초과했습니다.",
                    policy.max_total_uncompressed_bytes
                )));
            }

            let actual_crc = Self::crc32(&payload);
            if actual_crc != expected_crc {
                return Err(EasyOfficeError::CorruptedContainer(format!(
                    "ZIP 엔트리({path}) CRC-32 체크섬 불일치 (expected: {expected_crc:#010x}, actual: {actual_crc:#010x})"
                )));
            }

            extracted.push(ZipArchiveEntry {
                path,
                data: payload,
                crc32: actual_crc,
            });
        }

        Ok(extracted)
    }

    fn find_eocd_offset(bytes: &[u8]) -> Option<usize> {
        if bytes.len() < 22 {
            return None;
        }
        let min_offset = bytes.len().saturating_sub(22 + 65_535);
        let mut pos = bytes.len() - 22;
        loop {
            if bytes[pos..pos + 4] == Self::END_OF_CENTRAL_DIR_SIG.to_le_bytes() {
                let comment_len = u16::from_le_bytes([bytes[pos + 20], bytes[pos + 21]]) as usize;
                if pos + 22 + comment_len == bytes.len() {
                    return Some(pos);
                }
            }
            if pos <= min_offset || pos == 0 {
                break;
            }
            pos -= 1;
        }
        None
    }
}

#[inline]
fn read_u16_le(bytes: &[u8], offset: usize) -> Result<u16, EasyOfficeError> {
    if offset + 2 > bytes.len() {
        return Err(EasyOfficeError::CorruptedContainer(
            "ZIP u16 읽기 범위 초과".to_string(),
        ));
    }
    Ok(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]))
}

#[inline]
fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32, EasyOfficeError> {
    if offset + 4 > bytes.len() {
        return Err(EasyOfficeError::CorruptedContainer(
            "ZIP u32 읽기 범위 초과".to_string(),
        ));
    }
    Ok(u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ieee_crc32_standard_vector_and_zip_roundtrip() {
        // IEEE 802.3 standard test vector: "123456789" -> 0xCBF43926
        assert_eq!(IeeeCrc32::checksum(b"123456789"), 0xCBF4_3926);

        let entries = vec![
            (
                "[Content_Types].xml".to_string(),
                b"<Types/>".to_vec(),
            ),
            (
                "word/document.xml".to_string(),
                "<w:document><w:body><w:p><w:r><w:t>한글 및 English 본문</w:t></w:p></w:body></w:document>"
                    .as_bytes()
                    .to_vec(),
            ),
        ];

        let archive = ZipArchiveCodec::pack(&entries).unwrap();
        let policy = ZipSecurityPolicy::default();
        let unpacked = ZipArchiveCodec::unpack(&archive, &policy).unwrap();
        assert_eq!(unpacked.len(), 2);
        assert_eq!(unpacked[0].path, "[Content_Types].xml");
        assert_eq!(unpacked[0].data, b"<Types/>");
        assert_eq!(unpacked[1].path, "word/document.xml");

        // Tamper with a payload byte to verify CRC-32 Fail-Closed rejection
        let mut corrupted = archive.clone();
        let payload_idx = 30 + "[Content_Types].xml".len();
        corrupted[payload_idx] ^= 0xFF;
        assert!(matches!(
            ZipArchiveCodec::unpack(&corrupted, &policy),
            Err(EasyOfficeError::CorruptedContainer(_))
        ));

        // Zip-Slip path rejection on pack
        let bad_entries = vec![("../evil.xml".to_string(), b"bad".to_vec())];
        assert!(matches!(
            ZipArchiveCodec::pack(&bad_entries),
            Err(EasyOfficeError::SecurityViolation(_))
        ));
    }
}

use crate::zip_policy::ZIP_BUDGET_SLOTS;
use crate::IeeeCrc32;
use crate::{
    ContainerFamily, EasyOfficeError, FormatDetector, ZipEntryDescriptor, ZipSecurityGuard,
    ZipSecurityPolicy,
};
use std::io::{Cursor, Write};
use zip::read::{ArchiveOffset, Config};
use zip::result::ZipError;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

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
        Self::pack_internal(entries, false, false)
    }

    /// Packs with DEFLATE per entry, except the entries `must_store` names, which stay stored.
    pub fn pack_deflate<E: AsZipEntry>(entries: &[E]) -> Result<Vec<u8>, EasyOfficeError> {
        Self::pack_internal(entries, true, false)
    }

    /// Writer entry point for every ZIP-based output format: DEFLATE per entry with the
    /// stored-entry exceptions of [`Self::must_store`]. Entries whose compressed form is not
    /// smaller are also stored.
    pub fn pack_compressed<E: AsZipEntry>(entries: &[E]) -> Result<Vec<u8>, EasyOfficeError> {
        Self::pack_internal(entries, true, true)
    }

    /// Largest uncompressed/compressed ratio `pack_compressed` emits (below the default reader
    /// policy limit of 100 so a freshly packed archive always passes the bomb guard).
    const MAX_PACKED_RATIO: usize = 90;

    /// Entries that must never be compressed: the ODF/HWPX `mimetype` marker (readers sniff it at
    /// a fixed offset) and payloads that are already compressed images or archives.
    pub fn must_store(path: &str) -> bool {
        if path == "mimetype" {
            return true;
        }
        let Some((_, ext)) = path.rsplit_once('.') else {
            return false;
        };
        const STORED_EXTS: [&str; 10] = [
            "png", "jpg", "jpeg", "gif", "webp", "zip", "jar", "gz", "mp3", "mp4",
        ];
        STORED_EXTS
            .iter()
            .any(|candidate| ext.eq_ignore_ascii_case(candidate))
    }

    fn pack_internal<E: AsZipEntry>(
        entries: &[E],
        use_deflate: bool,
        ratio_guard: bool,
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
        for entry in entries {
            let (path, data) = (entry.entry_path(), entry.entry_data());
            ZipSecurityGuard::validate_entry_path(path)?;
            if path.len() > u16::MAX as usize {
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
        }

        // The crate only lays the archive out. DEFLATE is the engine's own (`deflate.rs`, one
        // backend whatever else is linked), so each entry is compressed here, kept only when it
        // shrank and passes the ratio limit, and written as the payload of a stored entry whose
        // method, size and CRC fields are then set to the real ones (same length, nothing moves).
        let packed: Vec<Option<Vec<u8>>> = entries
            .iter()
            .map(|e| {
                let data = e.entry_data();
                if !use_deflate || data.is_empty() || Self::must_store(e.entry_path()) {
                    return None;
                }
                let deflated = crate::deflate::deflate_raw(data);
                let within_ratio = !ratio_guard
                    || data.len() <= deflated.len().saturating_mul(Self::MAX_PACKED_RATIO);
                (deflated.len() < data.len() && within_ratio).then_some(deflated)
            })
            .collect();
        let mut archive = Self::write_archive(entries, &packed)?;
        if packed.iter().any(Option::is_some) {
            Self::mark_deflated(&mut archive, entries, &packed)?;
        }
        Ok(archive)
    }

    /// Writes the entries in order with the fixed 1980-01-01 time stamp, every one as a stored
    /// entry holding `packed` when present and the data otherwise.
    fn write_archive<E: AsZipEntry>(
        entries: &[E],
        packed: &[Option<Vec<u8>>],
    ) -> Result<Vec<u8>, EasyOfficeError> {
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .last_modified_time(DateTime::default());
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (entry, deflated) in entries.iter().zip(packed) {
            writer
                .start_file(entry.entry_path(), options)
                .map_err(zip_write_error)?;
            let payload = deflated.as_deref().unwrap_or_else(|| entry.entry_data());
            writer
                .write_all(payload)
                .map_err(|err| zip_write_error(err.into()))?;
        }
        writer
            .finish()
            .map(Cursor::into_inner)
            .map_err(zip_write_error)
    }

    /// Turns the stored entries that hold deflated payloads into deflate entries: needed version,
    /// method, CRC-32 and uncompressed size of the real data, in the local header and in the
    /// central directory entry. The payload and every offset stay as written.
    fn mark_deflated<E: AsZipEntry>(
        archive: &mut [u8],
        entries: &[E],
        packed: &[Option<Vec<u8>>],
    ) -> Result<(), EasyOfficeError> {
        let mut offsets = Vec::new();
        {
            let mut reader = ZipArchive::new(Cursor::new(&*archive)).map_err(zip_read_error)?;
            for index in (0..entries.len()).filter(|&i| packed[i].is_some()) {
                let file = reader.by_index_raw(index).map_err(zip_read_error)?;
                offsets.push((
                    index,
                    file.header_start() as usize,
                    file.central_header_start() as usize,
                ));
            }
        }
        for (index, local, central) in offsets {
            let data = entries[index].entry_data();
            let crc = Self::crc32(data).to_le_bytes();
            let size = (data.len() as u32).to_le_bytes();
            let version = Self::VERSION_NEEDED.to_le_bytes();
            let method = Self::METHOD_DEFLATE.to_le_bytes();
            // Local header: version 4, method 8, CRC-32 14, uncompressed size 22.
            // Central entry: version needed 6, method 10, CRC-32 16, uncompressed size 24.
            for (at, field) in [
                (local + 4, &version[..]),
                (local + 8, &method[..]),
                (local + 14, &crc[..]),
                (local + 22, &size[..]),
                (central + 6, &version[..]),
                (central + 10, &method[..]),
                (central + 16, &crc[..]),
                (central + 24, &size[..]),
            ] {
                archive[at..at + field.len()].copy_from_slice(field);
            }
        }
        Ok(())
    }

    pub fn unpack(
        bytes: &[u8],
        policy: &ZipSecurityPolicy,
    ) -> Result<Vec<ZipArchiveEntry>, EasyOfficeError> {
        let mut extracted = Vec::new();
        Self::unpack_each(bytes, policy, |entry| extracted.push(entry))?;
        Ok(extracted)
    }

    /// Runs every check of [`Self::unpack`] (structure, CRC, sizes, ratio, paths) and returns the
    /// entry paths, without keeping the payloads: at most one entry is held at a time, so a
    /// package of many large parts costs the memory of the largest one, not of their sum.
    pub fn verify(
        bytes: &[u8],
        policy: &ZipSecurityPolicy,
    ) -> Result<Vec<String>, EasyOfficeError> {
        let mut paths = Vec::new();
        Self::unpack_each(bytes, policy, |entry| paths.push(entry.path))?;
        Ok(paths)
    }

    /// Returns the raw Central Directory byte slice when a valid EOCD record is present.
    pub(crate) fn central_dir_slice(bytes: &[u8]) -> Option<&[u8]> {
        let eocd_offset = Self::find_eocd_offset(bytes)?;
        let cd_size = read_u32_le(bytes, eocd_offset + 12).ok()? as usize;
        let cd_offset = read_u32_le(bytes, eocd_offset + 16).ok()? as usize;
        let cd_end = cd_offset.checked_add(cd_size)?;
        (cd_end <= eocd_offset).then_some(&bytes[cd_offset..cd_end])
    }

    /// Evaluates `pred` on entries selected by `path_filter`, decompressing only matching paths
    /// and stopping as soon as `pred` returns `true`.
    pub(crate) fn any_selected_entry(
        bytes: &[u8],
        policy: &ZipSecurityPolicy,
        path_filter: impl FnMut(&str) -> bool,
        mut pred: impl FnMut(&ZipArchiveEntry) -> bool,
    ) -> bool {
        Self::unpack_each_selected(bytes, policy, path_filter, |entry| pred(&entry))
            .unwrap_or(false)
    }

    /// [`Self::unpack`] that hands each verified entry to `visit` in archive order instead of
    /// collecting them.
    pub fn unpack_each(
        bytes: &[u8],
        policy: &ZipSecurityPolicy,
        mut visit: impl FnMut(ZipArchiveEntry),
    ) -> Result<(), EasyOfficeError> {
        Self::unpack_each_selected(
            bytes,
            policy,
            |_| true,
            |entry| {
                visit(entry);
                false
            },
        )
        .map(|_| ())
    }

    /// Unpacks only the entries matching `path_filter`, avoiding decoding the rest.
    pub fn unpack_selected(
        bytes: &[u8],
        policy: &ZipSecurityPolicy,
        mut path_filter: impl FnMut(&str) -> bool,
    ) -> Result<Vec<ZipArchiveEntry>, EasyOfficeError> {
        let mut entries = Vec::new();
        Self::unpack_each_selected(bytes, policy, &mut path_filter, |entry| {
            entries.push(entry);
            false
        })?;
        Ok(entries)
    }

    fn unpack_each_selected(
        bytes: &[u8],
        policy: &ZipSecurityPolicy,
        mut path_filter: impl FnMut(&str) -> bool,
        mut visit: impl FnMut(ZipArchiveEntry) -> bool,
    ) -> Result<bool, EasyOfficeError> {
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

        // The crate looks for the last record signature and tolerates data after it; the
        // strict trailer match, the single-disk rule and the directory bounds stay here.
        let eocd_offset = Self::find_eocd_offset(bytes).ok_or_else(|| {
            EasyOfficeError::CorruptedContainer(
                "PKZIP End of Central Directory(0x06054B50) 레코드를 찾을 수 없습니다.".to_string(),
            )
        })?;
        let disk_num = read_u16_le(bytes, eocd_offset + 4)?;
        let cd_start_disk = read_u16_le(bytes, eocd_offset + 6)?;
        let disk_entries = read_u16_le(bytes, eocd_offset + 8)? as usize;
        let total_entries = read_u16_le(bytes, eocd_offset + 10)? as usize;
        if disk_num != 0 || cd_start_disk != 0 || disk_entries != total_entries {
            return Err(EasyOfficeError::CorruptedContainer(
                "다중 디스크 ZIP 아카이브는 지원하지 않습니다.".to_string(),
            ));
        }
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

        // Offset 0 is fixed: no data may precede the archive (the crate would detect and skip it).
        let mut archive = ZipArchive::with_config(
            Config {
                archive_offset: ArchiveOffset::Known(0),
            },
            Cursor::new(bytes),
        )
        .map_err(zip_read_error)?;
        // The crate keeps one entry per exact name, so a shorter list than the directory
        // announces means a path appears twice.
        if archive.len() != total_entries {
            return Err(EasyOfficeError::SecurityViolation(
                "중복되거나 파일/디렉터리가 충돌하는 ZIP 엔트리 경로가 감지되었습니다".to_string(),
            ));
        }

        let mut descriptors: Vec<ZipEntryDescriptor> = Vec::with_capacity(total_entries.min(4096));
        let mut records: Vec<CentralRecord> = Vec::with_capacity(total_entries.min(4096));
        for index in 0..archive.len() {
            let file = archive.by_index_raw(index).map_err(zip_read_error)?;
            let path = std::str::from_utf8(file.name_raw())
                .map_err(|_| {
                    EasyOfficeError::InvalidUtf8(
                        "ZIP 엔트리 경로가 유효한 UTF-8이 아닙니다.".to_string(),
                    )
                })?
                .to_string();
            let method = file.compression().to_u16();
            if method != Self::METHOD_STORE && method != Self::METHOD_DEFLATE {
                return Err(EasyOfficeError::CorruptedContainer(format!(
                    "지원하지 않는 ZIP 압축 방식({method})입니다 (Store=0, Deflate=8 전용)."
                )));
            }
            descriptors.push(ZipEntryDescriptor::new(
                path.clone(),
                file.compressed_size(),
                file.size(),
                file.encrypted(),
            ));
            records.push(CentralRecord {
                path,
                method,
                expected_crc: file.crc32(),
                comp_size: file.compressed_size() as usize,
                uncomp_size: file.size() as usize,
                local_offset: file.header_start() as usize,
                data_start: file.data_start().map_or(0, |start| start as usize),
            });
        }

        ZipSecurityGuard::validate_archive(&descriptors, policy)?;

        let mut totals = [0_u64; ZIP_BUDGET_SLOTS];
        let mut spans = Vec::with_capacity(records.len());
        for record in &records {
            let data_end = Self::check_local_header(bytes, record, cd_offset)?;
            spans.push((record.local_offset, data_end));
        }

        spans.sort_unstable_by_key(|&(start, _)| start);
        for pair in spans.windows(2) {
            if pair[0].1 > pair[1].0 {
                return Err(EasyOfficeError::SecurityViolation(
                    "ZIP 로컬 엔트리 구간 중첩(오버래핑 ZIP) 감지".to_string(),
                ));
            }
        }

        for record in records {
            if !path_filter(&record.path) {
                continue;
            }

            let slot = policy.slot(&record.path);
            let (max_single, max_total) = policy.limits(slot);
            let compressed_slice = &bytes[record.data_start..record.data_start + record.comp_size];
            let payload = if record.method == Self::METHOD_STORE {
                compressed_slice.to_vec()
            } else {
                // The crate's own entry reader inflates without an output limit, so the raw bytes
                // go through the budgeted inflater instead.
                let decompressed = crate::deflate::inflate_raw_sized(
                    compressed_slice,
                    max_single as usize,
                    record.uncomp_size,
                )?;
                if decompressed.len() != record.uncomp_size {
                    return Err(EasyOfficeError::CorruptedContainer(format!(
                        "Deflate 엔트리({}) 복원 크기 불일치 (expected: {}, actual: {})",
                        record.path,
                        record.uncomp_size,
                        decompressed.len()
                    )));
                }
                decompressed
            };

            totals[slot] = totals[slot].saturating_add(payload.len() as u64);
            if totals[slot] > max_total {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "ZIP 누적 복원 크기({})가 한도({max_total})를 초과했습니다.",
                    totals[slot]
                )));
            }

            let actual_crc = Self::crc32(&payload);
            if actual_crc != record.expected_crc {
                return Err(EasyOfficeError::CorruptedContainer(format!(
                    "ZIP 엔트리({}) CRC-32 체크섬 불일치 (expected: {:#010x}, actual: {:#010x})",
                    record.path, record.expected_crc, actual_crc
                )));
            }

            if visit(ZipArchiveEntry {
                path: record.path,
                data: payload,
                crc32: actual_crc,
            }) {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Cross-checks the local file header of `record` against its central directory entry (the
    /// crate reads the directory and ignores the local copy) and returns where its data ends.
    fn check_local_header(
        bytes: &[u8],
        record: &CentralRecord,
        cd_offset: usize,
    ) -> Result<usize, EasyOfficeError> {
        let (path, local_offset) = (&record.path, record.local_offset);
        if record.method == Self::METHOD_STORE && record.comp_size != record.uncomp_size {
            return Err(EasyOfficeError::CorruptedContainer(format!(
                "Store 엔트리({path})의 압축 크기({})와 원본 크기({})가 일치하지 않습니다.",
                record.comp_size, record.uncomp_size
            )));
        }
        if local_offset + 30 > cd_offset {
            return Err(EasyOfficeError::CorruptedContainer(format!(
                "ZIP 로컬 파일 헤더 오프셋({local_offset})이 유효하지 않습니다."
            )));
        }
        if read_u32_le(bytes, local_offset)? != Self::LOCAL_FILE_HEADER_SIG {
            return Err(EasyOfficeError::CorruptedContainer(
                "PKZIP 로컬 파일 헤더 시그니처(0x04034B50) 불일치".to_string(),
            ));
        }
        let local_flags = read_u16_le(bytes, local_offset + 6)?;
        if (local_flags & 0x0001) != 0 {
            return Err(EasyOfficeError::EncryptedDocumentUnsupported(
                crate::DocumentFormat::Docx,
            ));
        }
        if read_u16_le(bytes, local_offset + 8)? != record.method {
            return Err(EasyOfficeError::SecurityViolation(format!(
                "ZIP 로컬 파일 헤더와 Central Directory 압축 방식 불일치: {path}"
            )));
        }
        let metadata_matches = read_u32_le(bytes, local_offset + 14)? == record.expected_crc
            && read_u32_le(bytes, local_offset + 18)? as usize == record.comp_size
            && read_u32_le(bytes, local_offset + 22)? as usize == record.uncomp_size;
        if (local_flags & 0x0008) == 0 && !metadata_matches {
            return Err(EasyOfficeError::SecurityViolation(format!(
                "ZIP 로컬 파일 헤더와 Central Directory 크기/CRC 메타데이터 불일치: {path}"
            )));
        }
        let local_name_len = read_u16_le(bytes, local_offset + 26)? as usize;
        let name_start = local_offset + 30;
        let name_end = name_start + local_name_len;
        if name_end > cd_offset || &bytes[name_start..name_end] != path.as_bytes() {
            return Err(EasyOfficeError::SecurityViolation(format!(
                "ZIP 로컬 파일 헤더와 Central Directory 엔트리 경로 불일치: {path}"
            )));
        }
        let data_end = record.data_start.saturating_add(record.comp_size);
        if data_end > cd_offset {
            return Err(EasyOfficeError::CorruptedContainer(format!(
                "ZIP 엔트리({path}) 데이터가 Central Directory 경계를 침범합니다."
            )));
        }
        Ok(data_end)
    }

    fn find_eocd_offset(bytes: &[u8]) -> Option<usize> {
        if bytes.len() < 22 {
            return None;
        }
        let min_offset = bytes.len().saturating_sub(22 + 65_535);
        let sig = Self::END_OF_CENTRAL_DIR_SIG.to_le_bytes();
        let mut search_end = bytes.len() - 21;
        while search_end > min_offset {
            let rel = bytes[min_offset..search_end]
                .iter()
                .rposition(|&b| b == 0x50)?;
            let pos = min_offset + rel;
            if bytes[pos..pos + 4] == sig {
                let comment_len = u16::from_le_bytes([bytes[pos + 20], bytes[pos + 21]]) as usize;
                if pos + 22 + comment_len == bytes.len() {
                    return Some(pos);
                }
            }
            search_end = pos;
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

/// What the central directory says about one entry, with the offsets the crate resolved.
struct CentralRecord {
    path: String,
    method: u16,
    expected_crc: u32,
    comp_size: usize,
    uncomp_size: usize,
    local_offset: usize,
    data_start: usize,
}

fn zip_read_error(err: ZipError) -> EasyOfficeError {
    EasyOfficeError::CorruptedContainer(format!("PKZIP 아카이브를 읽을 수 없습니다: {err}"))
}

fn zip_write_error(err: ZipError) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(format!("PKZIP 아카이브를 쓸 수 없습니다: {err}"))
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

    #[test]
    fn verify_runs_the_checks_of_unpack_and_returns_only_the_paths() {
        let entries = vec![
            ("Contents/header.xml".to_string(), b"<head/>".to_vec()),
            (
                "BinData/image1.bmp".to_string(),
                (0..=250).cycle().take(4096).collect(),
            ),
        ];
        let archive = ZipArchiveCodec::pack_deflate(&entries).unwrap();
        let policy = ZipSecurityPolicy::default();
        assert_eq!(
            ZipArchiveCodec::verify(&archive, &policy).unwrap(),
            ["Contents/header.xml", "BinData/image1.bmp"]
        );
        // A flipped payload byte (CRC-32) and a truncated archive are refused as by `unpack`.
        let mut corrupted = archive.clone();
        corrupted[30 + "Contents/header.xml".len()] ^= 0xFF;
        assert!(ZipArchiveCodec::verify(&corrupted, &policy).is_err());
        assert!(ZipArchiveCodec::verify(&archive[..archive.len() - 10], &policy).is_err());
    }

    #[test]
    fn a_binary_budget_counts_non_xml_parts_apart_from_xml_parts() {
        let entries = vec![
            ("Contents/a.xml".to_string(), vec![b'a'; 3000]),
            ("Contents/b.XML".to_string(), vec![b'b'; 3000]),
            ("Contents/content.hpf".to_string(), vec![b'c'; 3000]),
            ("BinData/image1.bmp".to_string(), vec![1_u8; 20_000]),
            ("BinData/image2.bmp".to_string(), vec![2_u8; 20_000]),
        ];
        let archive = ZipArchiveCodec::pack_deflate(&entries).unwrap();
        let shared = ZipSecurityPolicy {
            max_single_entry_uncompressed_bytes: 20_000,
            max_total_uncompressed_bytes: 10_000,
            max_compression_ratio: f64::INFINITY,
            ..ZipSecurityPolicy::default()
        };
        // One budget for everything: 49 000 bytes against 10 000.
        assert!(ZipArchiveCodec::verify(&archive, &shared).is_err());
        let split = ZipSecurityPolicy {
            binary_budget: Some(crate::ZipBinaryBudget {
                max_single_entry_uncompressed_bytes: 20_000,
                max_total_uncompressed_bytes: 40_000,
            }),
            ..shared.clone()
        };
        // 9 000 bytes of text fit 10 000 and 40 000 bytes of pictures fit theirs.
        assert_eq!(ZipArchiveCodec::verify(&archive, &split).unwrap().len(), 5);
        // One byte less for pictures, or less for text, is refused by size alone, whichever
        // order the parts come in.
        let few_pictures = ZipSecurityPolicy {
            binary_budget: Some(crate::ZipBinaryBudget {
                max_single_entry_uncompressed_bytes: 20_000,
                max_total_uncompressed_bytes: 39_999,
            }),
            ..shared.clone()
        };
        assert!(ZipArchiveCodec::verify(&archive, &few_pictures).is_err());
        let little_text = ZipSecurityPolicy {
            max_total_uncompressed_bytes: 8_999,
            ..split
        };
        assert!(ZipArchiveCodec::verify(&archive, &little_text).is_err());
    }

    #[test]
    fn test_twenty_formats_and_magic_signature_detection() {
        use crate::{DocumentFormat, FormatDetector};
        assert_eq!(DocumentFormat::ALL.len(), 20);
        let det = |b: &[u8], h: Option<&str>| FormatDetector::detect(b, h).unwrap();
        let zip_sample = b"PK\x03\x04\x14\x00\x00\x00word/document.xml";
        assert_eq!(det(zip_sample, None), DocumentFormat::Docx);
        for &(sample, expected) in &[
            (&b"PK\x03\x04Contents/header.xml"[..], DocumentFormat::Hwpx),
            (b"PK\x03\x04oasis.opendocument.text", DocumentFormat::Odt),
            (
                b"PK\x03\x04oasis.opendocument.spreadsheet",
                DocumentFormat::Ods,
            ),
            (
                b"PK\x03\x04oasis.opendocument.presentation",
                DocumentFormat::Odp,
            ),
            (b"PK\x03\x04xl/workbook.xml", DocumentFormat::Xlsx),
            (b"PK\x03\x04ppt/presentation.xml", DocumentFormat::Pptx),
        ] {
            assert_eq!(det(sample, None), expected);
        }
        assert_eq!(det(zip_sample, Some("report.docx")), DocumentFormat::Docx);
        assert_eq!(det(zip_sample, Some("template.dotx")), DocumentFormat::Docx);
        assert_eq!(det(zip_sample, Some("official.hwpx")), DocumentFormat::Hwpx);
        assert_eq!(det(zip_sample, Some("template.hwtx")), DocumentFormat::Hwtx);
        assert_eq!(det(zip_sample, Some("show.ppsx")), DocumentFormat::Ppsx);

        let ole2_sample = b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1HWP Document File";
        assert_eq!(det(ole2_sample, Some("proposal.hwp")), DocumentFormat::Hwp);
        assert_eq!(det(ole2_sample, Some("template.hwt")), DocumentFormat::Hwt);
        let pdf_sample = b"%PDF-1.4\n1 0 obj\n<<>>\nendobj\n%%EOF";
        assert_eq!(det(pdf_sample, Some("contract.pdf")), DocumentFormat::Pdf);
        assert_eq!(
            det(b"{\\rtf1\\ansi\\deff0 Hello}", Some("memo.rtf")),
            DocumentFormat::Rtf
        );
        assert_eq!(
            FormatDetector::detect(b"", Some("empty.docx")),
            Err(EasyOfficeError::EmptyPayload)
        );
        assert!(matches!(
            FormatDetector::detect(b"NOT_A_ZIP_FILE", Some("spoofed.xlsx")),
            Err(EasyOfficeError::SignatureMismatch { .. })
        ));
        assert!(matches!(
            FormatDetector::detect(zip_sample, Some("spoofed.csv")),
            Err(EasyOfficeError::SignatureMismatch { .. })
        ));
    }

    #[test]
    fn test_zip_security_guard_blocks_traversal_and_decompression_bomb() {
        let policy = ZipSecurityPolicy::default();
        let chk = |p: &str, c: u64, u: u64| {
            ZipSecurityGuard::validate_archive_entries(
                &[ZipEntryDescriptor::new(p, c, u, false)],
                &policy,
            )
        };
        assert!(chk("[Content_Types].xml", 512, 2048).is_ok());
        assert!(chk("word/document.xml", 1024, 4096).is_ok());
        assert!(matches!(
            chk("../../etc/passwd", 100, 200),
            Err(EasyOfficeError::SecurityViolation(_))
        ));
        assert!(matches!(
            chk("C:\\Windows\\win.ini", 100, 200),
            Err(EasyOfficeError::SecurityViolation(_))
        ));
        assert!(matches!(
            chk("word/document.xml", 100, 20_000),
            Err(EasyOfficeError::DecompressionBombDetected { .. })
        ));
    }

    #[test]
    fn test_ole2_sector_cycle_and_hwp5_encryption_guard() {
        use crate::{DocumentFormat, FormatDetector, Ole2SecurityGuard};
        let mut container = vec![0u8; 512 * 4];
        container[..8].copy_from_slice(&FormatDetector::OLE2_MAGIC);
        container[44..48].copy_from_slice(&1u32.to_le_bytes());

        let cyclic_fat = vec![1u32, 0u32, Ole2SecurityGuard::END_OF_CHAIN];
        assert!(matches!(
            Ole2SecurityGuard::validate_sector_chain(&container, &cyclic_fat, 0),
            Err(EasyOfficeError::SecurityViolation(_))
        ));

        let valid_fat = vec![1u32, 2u32, Ole2SecurityGuard::END_OF_CHAIN];
        assert_eq!(
            Ole2SecurityGuard::validate_sector_chain(&container, &valid_fat, 0).unwrap(),
            vec![0, 1, 2]
        );

        let mut hwp_header = vec![0u8; 256];
        hwp_header[..17].copy_from_slice(b"HWP Document File");
        hwp_header[32..36].copy_from_slice(&0x0500_0300u32.to_le_bytes());
        hwp_header[36..40].copy_from_slice(&0x02u32.to_le_bytes());
        assert_eq!(
            Ole2SecurityGuard::validate_hwp5_file_header(&hwp_header),
            Err(EasyOfficeError::EncryptedDocumentUnsupported(
                DocumentFormat::Hwp
            ))
        );
    }

    #[test]
    fn test_xml_sanitizer_and_csv_commonmark_pdf_roundtrip() {
        use crate::{
            CommonMarkCodec, CsvCodec, PdfDocumentModel, PdfPageModel, PdfSerializer, XmlSanitizer,
        };
        assert!(XmlSanitizer::validate_safe_xml(
            "<w:document><w:body><w:p><w:t>안전한 문서</w:t></w:p></w:body></w:document>"
        )
        .is_ok());
        assert_eq!(
            XmlSanitizer::validate_safe_xml("<?xml version=\"1.0\"?><!DOCTYPE foo [<!ENTITY xxe SYSTEM \"file:///etc/passwd\">]><foo>&xxe;</foo>"),
            Err(EasyOfficeError::XxeOrExternalEntityDetected)
        );
        let escaped = XmlSanitizer::escape("A & B < C > \"D\" 'E'");
        assert_eq!(XmlSanitizer::unescape(&escaped), "A & B < C > \"D\" 'E'");
        let rows = vec![
            vec!["부서".to_string(), "메모, 특이사항".to_string()],
            vec!["연구개발".to_string(), "1행\n2행 \"강조\"".to_string()],
        ];
        assert_eq!(
            CsvCodec::parse(&CsvCodec::serialize(&rows, ','), ',').unwrap(),
            rows
        );
        let md_input =
            "# 제품 아키텍처\n\n본문 첫 문단입니다.\n\n> 핵심 설계 원칙\n\n- 항목 1\n- 항목 2";
        assert_eq!(
            CommonMarkCodec::render(&CommonMarkCodec::parse(md_input).unwrap()),
            md_input
        );
        let pdf_doc = PdfDocumentModel {
            title: "분기 보안 감사 보고서".to_string(),
            pages: vec![PdfPageModel {
                page_number: 1,
                rotation_degrees: 0,
                body_text: "ISO 32000-1 PDF 본문 텍스트\n2행 개행 보존".to_string(),
                approval_stamp: Some("APPROVED".to_string()),
            }],
            watermark_text: Some("CONFIDENTIAL".to_string()),
        };
        assert_eq!(
            PdfSerializer::parse(&PdfSerializer::serialize(&pdf_doc)).unwrap(),
            pdf_doc
        );
    }

    #[test]
    fn test_sec_05_overlapping_zip_entries_rejected() {
        let entries = vec![
            ("file1.txt".to_string(), b"Hello World".to_vec()),
            ("file2.txt".to_string(), b"Foo Bar Baz".to_vec()),
        ];
        let valid_zip = ZipArchiveCodec::pack(&entries).unwrap();
        let eocd = valid_zip
            .windows(4)
            .rposition(|w| w == [0x50, 0x4b, 0x05, 0x06])
            .unwrap();
        let cd_offset =
            u32::from_le_bytes(valid_zip[eocd + 16..eocd + 20].try_into().unwrap()) as usize;
        let mut evil = valid_zip.clone();
        let cd2 = evil[cd_offset + 4..]
            .windows(4)
            .position(|w| w == [0x50, 0x4b, 0x01, 0x02])
            .unwrap()
            + cd_offset
            + 4;
        evil[cd2 + 42..cd2 + 46].copy_from_slice(&0u32.to_le_bytes());
        let nlen = u16::from_le_bytes(evil[cd_offset + 28..cd_offset + 30].try_into().unwrap());
        evil[cd2 + 28..cd2 + 30].copy_from_slice(&nlen.to_le_bytes());
        let name1 = &valid_zip[cd_offset + 46..cd_offset + 46 + nlen as usize];
        evil[cd2 + 46..cd2 + 46 + nlen as usize].copy_from_slice(name1);
        let res = ZipArchiveCodec::unpack(&evil, &crate::ZipSecurityPolicy::default());
        assert!(
            matches!(res, Err(EasyOfficeError::SecurityViolation(_))),
            "중첩 ZIP은 SecurityViolation이어야 함"
        );
    }
}

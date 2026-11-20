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

        let end = Self::read_end_record(bytes, policy)?;
        let (total_entries, cd_offset, cd_end) = (end.total_entries, end.cd_offset, end.cd_end);

        // Offset 0 is fixed: no data may precede the archive (the crate would detect and skip it).
        let mut archive = ZipArchive::with_config(
            Config {
                archive_offset: ArchiveOffset::Known(0),
            },
            Cursor::new(bytes),
        )
        .map_err(zip_read_error)?;
        // The crate chooses its own end record (the last signature it can parse, even inside a
        // comment) and lets extra fields override what the directory says. Everything below
        // therefore holds the crate's view to the raw bytes of the directory checked here.
        if archive.central_directory_start() != cd_offset as u64 {
            return Err(EasyOfficeError::SecurityViolation(
                "ZIP Central Directory 위치가 End of Central Directory 기록과 다릅니다."
                    .to_string(),
            ));
        }
        // The crate keeps one entry per exact name, so a shorter list than the directory
        // announces means a path appears twice.
        if archive.len() != total_entries {
            return Err(EasyOfficeError::SecurityViolation(
                "중복되거나 파일/디렉터리가 충돌하는 ZIP 엔트리 경로가 감지되었습니다".to_string(),
            ));
        }

        let mut descriptors: Vec<ZipEntryDescriptor> = Vec::with_capacity(total_entries.min(4096));
        let mut records: Vec<CentralRecord> = Vec::with_capacity(total_entries.min(4096));
        let mut next_record = cd_offset;
        for index in 0..archive.len() {
            let file = archive.by_index_raw(index).map_err(zip_read_error)?;
            let method = method_id(file.compression()).ok_or_else(|| {
                EasyOfficeError::CorruptedContainer(format!(
                    "지원하지 않는 ZIP 압축 방식({:?})입니다 (Store=0, Deflate=8 전용).",
                    file.compression()
                ))
            })?;
            if file.central_header_start() as usize != next_record {
                return Err(EasyOfficeError::SecurityViolation(
                    "ZIP Central Directory 항목이 연속하지 않거나 크레이트가 읽은 위치와 다릅니다."
                        .to_string(),
                ));
            }
            next_record = Self::check_central_record(bytes, next_record, cd_end, &file)?;
            let path = std::str::from_utf8(file.name_raw())
                .map_err(|_| {
                    EasyOfficeError::InvalidUtf8(
                        "ZIP 엔트리 경로가 유효한 UTF-8이 아닙니다.".to_string(),
                    )
                })?
                .to_string();
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

        if next_record != cd_end {
            return Err(EasyOfficeError::CorruptedContainer(
                "PKZIP Central Directory 엔트리 총 크기 불일치".to_string(),
            ));
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

    /// The strict end record (see [`Self::find_eocd_offset`]) and every limit that must hold before
    /// the crate allocates for the directory: single disk, no ZIP64, the policy's entry count,
    /// directory bounds, and no second end record inside the comment.
    fn read_end_record(
        bytes: &[u8],
        policy: &ZipSecurityPolicy,
    ) -> Result<EndRecord, EasyOfficeError> {
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
        let zip64_locator = eocd_offset >= ZIP64_LOCATOR_LEN
            && bytes[eocd_offset - ZIP64_LOCATOR_LEN..].starts_with(&ZIP64_LOCATOR_SIG);
        if total_entries == usize::from(u16::MAX)
            || cd_offset == u32::MAX as usize
            || cd_size == u32::MAX as usize
            || zip64_locator
        {
            return Err(EasyOfficeError::CorruptedContainer(
                "ZIP64 아카이브는 지원하지 않습니다.".to_string(),
            ));
        }
        if total_entries > policy.max_entries {
            return Err(EasyOfficeError::SecurityViolation(format!(
                "ZIP 엔트리 개수({total_entries})가 허용 상한({})을 초과했습니다.",
                policy.max_entries
            )));
        }
        let cd_end = cd_offset
            .checked_add(cd_size)
            .filter(|&end| end <= eocd_offset)
            .ok_or_else(|| {
                EasyOfficeError::CorruptedContainer(
                    "PKZIP Central Directory 오프셋 또는 크기가 유효하지 않습니다.".to_string(),
                )
            })?;
        // A second end record makes tools disagree on which directory is the archive's: one
        // inside the comment (the crate accepts it when data follows), or an earlier record whose
        // comment also reaches the end of the file (the strict search above takes the last).
        let sig = Self::END_OF_CENTRAL_DIR_SIG.to_le_bytes();
        let in_comment = bytes[eocd_offset + 22..].windows(4).any(|w| w == sig);
        let window_start = bytes.len().saturating_sub(22 + 65_535);
        let earlier_fit = bytes[window_start..eocd_offset]
            .windows(22)
            .enumerate()
            .any(|(at, record)| {
                let comment_len = usize::from(u16::from_le_bytes([record[20], record[21]]));
                record.starts_with(&sig) && window_start + at + 22 + comment_len == bytes.len()
            });
        if in_comment || earlier_fit {
            return Err(EasyOfficeError::SecurityViolation(
                "ZIP에 End of Central Directory 기록이 둘 이상 있습니다.".to_string(),
            ));
        }
        Ok(EndRecord {
            total_entries,
            cd_offset,
            cd_end,
        })
    }

    /// Reads the raw central directory record at `start` and requires it to say what the crate
    /// reports for `file` (name, method, flags, CRC, 32-bit sizes, local offset), with none of the
    /// extra fields that make the crate replace those values (ZIP64, Unicode path, AES). Returns
    /// where the record ends.
    fn check_central_record<R: std::io::Read>(
        bytes: &[u8],
        start: usize,
        cd_end: usize,
        file: &zip::read::ZipFile<'_, R>,
    ) -> Result<usize, EasyOfficeError> {
        let corrupt = |what: &str| {
            EasyOfficeError::CorruptedContainer(format!("PKZIP Central Directory {what}"))
        };
        if start.checked_add(46).is_none_or(|end| end > cd_end) {
            return Err(corrupt("헤더가 절단되었습니다."));
        }
        if read_u32_le(bytes, start)? != Self::CENTRAL_DIR_HEADER_SIG {
            return Err(corrupt("시그니처(0x02014B50) 불일치"));
        }
        let name_len = read_u16_le(bytes, start + 28)? as usize;
        let extra_len = read_u16_le(bytes, start + 30)? as usize;
        let comment_len = read_u16_le(bytes, start + 32)? as usize;
        let name_end = start + 46 + name_len;
        let extra_end = name_end + extra_len;
        let end = extra_end + comment_len;
        if end > cd_end {
            return Err(corrupt("엔트리 경계 초과"));
        }
        let mut extra = &bytes[name_end..extra_end];
        while extra.len() >= 4 {
            let tag = u16::from_le_bytes([extra[0], extra[1]]);
            let len = usize::from(u16::from_le_bytes([extra[2], extra[3]]));
            if matches!(tag, EXTRA_ZIP64 | EXTRA_UNICODE_PATH | EXTRA_AES) {
                return Err(EasyOfficeError::SecurityViolation(format!(
                    "ZIP Central Directory 부가 필드(0x{tag:04X})는 크기·이름·압축 방식을 바꿀 수 있어 허용하지 않습니다."
                )));
            }
            extra = extra
                .get(4 + len..)
                .ok_or_else(|| corrupt("부가 필드 길이 초과"))?;
        }
        let agrees = read_u16_le(bytes, start + 8)? & 1 == u16::from(file.encrypted())
            && Some(read_u16_le(bytes, start + 10)?) == method_id(file.compression())
            && u64::from(read_u32_le(bytes, start + 16)?) == u64::from(file.crc32())
            && u64::from(read_u32_le(bytes, start + 20)?) == file.compressed_size()
            && u64::from(read_u32_le(bytes, start + 24)?) == file.size()
            && u64::from(read_u32_le(bytes, start + 42)?) == file.header_start()
            && &bytes[start + 46..name_end] == file.name_raw();
        if !agrees {
            return Err(EasyOfficeError::SecurityViolation(
                "ZIP Central Directory 원시 기록이 크레이트가 읽은 값과 다릅니다.".to_string(),
            ));
        }
        Ok(end)
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

/// The end record's numbers the reader keeps.
struct EndRecord {
    total_entries: usize,
    cd_offset: usize,
    cd_end: usize,
}

const ZIP64_LOCATOR_SIG: [u8; 4] = [0x50, 0x4B, 0x06, 0x07];
/// Signature (4) plus the locator's fields (16).
const ZIP64_LOCATOR_LEN: usize = 20;
const EXTRA_ZIP64: u16 = 0x0001;
const EXTRA_AES: u16 = 0x9901;
const EXTRA_UNICODE_PATH: u16 = 0x7075;

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

/// The method numbers this reader accepts. Without a compression feature the crate reports
/// DEFLATE as an unsupported method, which still compares equal to its constant.
fn method_id(method: CompressionMethod) -> Option<u16> {
    if method == CompressionMethod::STORE {
        Some(ZipArchiveCodec::METHOD_STORE)
    } else if method == CompressionMethod::DEFLATE {
        Some(ZipArchiveCodec::METHOD_DEFLATE)
    } else {
        None
    }
}

fn zip_read_error(err: ZipError) -> EasyOfficeError {
    EasyOfficeError::CorruptedContainer(format!("PKZIP 아카이브를 읽을 수 없습니다: {err}"))
}

fn zip_write_error(err: ZipError) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(format!("PKZIP 아카이브를 쓸 수 없습니다: {err}"))
}

#[cfg(test)]
mod tests;

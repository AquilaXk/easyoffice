//! Pages package (ZIP) handling: layout detection, legacy/encrypted refusal and loading every
//! `Index/**/*.iwa` file into one [`ObjectTable`].
//!
//! The ZIP is unpacked through [`ZipArchiveCodec::unpack`] with the default
//! [`ZipSecurityPolicy`] (entry count, size and ratio ceilings, path traversal). Package-directory
//! zips (all entries below a single `Name.pages/` folder) are handled by stripping that prefix.

use crate::encrypted_container::is_ole2_offcrypto_encrypted;
use crate::pages_iwa::{decode_stream_into_table, decompress_iwa_into, MAX_DOCUMENT_BYTES};
use crate::pages_proto::ObjectTable;
use crate::zip_path_tracker::ZipPathTracker;
use crate::{
    open_guarded, DocumentFormat, EasyOfficeError, FormatDetector, OpenLimits, XmlSanitizer,
    ZipArchiveCodec, ZipArchiveEntry, ZipBinaryBudget, ZipEntryDescriptor, ZipSecurityGuard,
    ZipSecurityPolicy,
};
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::collections::HashMap;
use std::io::Cursor;

/// Package directory that holds picture and media files.
const DATA_DIR: &str = "Data/";

/// Entry-name suffixes that mark an encrypted/password-protected package.
const ENCRYPTED_ENTRY_SUFFIXES: [&str; 2] = [".iwph", ".iwpv2"];
/// Keys inside `Metadata/Properties.plist` that mark encryption when true.
const ENCRYPTED_PLIST_KEYS: [&str; 2] = ["isEncrypted", "documentEncrypted"];
const MAX_PLIST_BYTES: usize = 8 * 1024 * 1024;
const MAX_PLIST_XML_EVENTS: usize = 65_536;
const MAX_PLIST_KEY_BYTES: usize = 128;
const MAX_BPLIST_OBJECTS: usize = 100_000;
const MAX_BPLIST_DICT_ENTRIES: usize = 16_384;
const MAX_BPLIST_DEPTH: usize = 8;

pub const LEGACY_MESSAGE: &str = "2009년 이전 형식(index.xml) Pages 문서는 아직 지원하지 않습니다. Pages에서 문서를 열어 최신 형식(.pages) 또는 DOCX로 다시 저장한 뒤 열어 주세요.";

/// Decoded package: all IWA objects plus how many `.iwa` files were read.
#[derive(Debug, Default)]
pub struct PagesPackage {
    pub objects: ObjectTable,
    pub iwa_files: usize,
    /// Files under `Data/` (pictures and media), keyed by package-relative path.
    pub data_files: HashMap<String, Vec<u8>>,
}

#[inline]
fn is_single_folder_prefix(prefix: &str) -> bool {
    prefix.is_empty() || (prefix.ends_with('/') && !prefix[..prefix.len() - 1].contains('/'))
}

#[inline]
fn ends_with_ignore_ascii_case(s: &str, suffix: &str) -> bool {
    s.len() >= suffix.len() && s[s.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
}

fn strip_package_prefix(entries: &[ZipArchiveEntry]) -> &str {
    let has_top_marker = entries.iter().any(|e| {
        e.path.starts_with("Index/")
            || e.path.starts_with("Metadata/")
            || matches!(e.path.as_str(), "index.xml" | "index.xml.gz" | "Index.zip")
            || (!e.path.contains('/')
                && ENCRYPTED_ENTRY_SUFFIXES
                    .iter()
                    .any(|suffix| ends_with_ignore_ascii_case(&e.path, suffix)))
    });
    if has_top_marker {
        return "";
    }
    for marker in ["Index/", "Metadata/"] {
        for e in entries {
            if let Some(idx) = e.path.find(marker) {
                let prefix = &e.path[..idx];
                if is_single_folder_prefix(prefix)
                    && !prefix.eq_ignore_ascii_case("Data/")
                    && !prefix.eq_ignore_ascii_case("QuickLook/")
                {
                    return prefix;
                }
            }
        }
    }
    for marker in ["index.xml", "index.xml.gz", "Index.zip"] {
        for e in entries {
            if let Some(prefix) = e.path.strip_suffix(marker) {
                if is_single_folder_prefix(prefix) {
                    return prefix;
                }
            }
        }
    }
    for e in entries {
        if ENCRYPTED_ENTRY_SUFFIXES
            .iter()
            .any(|suffix| ends_with_ignore_ascii_case(&e.path, suffix))
        {
            let prefix = match e.path.rfind('/') {
                Some(pos) => &e.path[..=pos],
                None => "",
            };
            if is_single_folder_prefix(prefix) {
                return prefix;
            }
        }
    }
    ""
}

fn is_xml_plist_encrypted(data: &[u8]) -> bool {
    let Ok(raw) = std::str::from_utf8(data) else {
        return false;
    };
    let trimmed = raw.trim_start_matches('\u{FEFF}').trim_start();
    if !(trimmed.starts_with("<?xml") || trimmed.starts_with("<plist")) {
        return false;
    }
    let mut reader = Reader::from_str(trimmed);
    let mut in_key = false;
    let mut key_too_long = false;
    let mut key_buf = String::new();
    let mut expect_bool_value = false;
    let mut events = 0usize;

    loop {
        if events >= MAX_PLIST_XML_EVENTS {
            return false;
        }
        events += 1;
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                if expect_bool_value {
                    if e.local_name().as_ref() == "true" {
                        return true;
                    }
                    expect_bool_value = false;
                }
                if e.local_name().as_ref() == "key" {
                    in_key = true;
                    key_too_long = false;
                    key_buf.clear();
                }
            }
            Ok(Event::Empty(e)) => {
                if expect_bool_value {
                    if e.local_name().as_ref() == "true" {
                        return true;
                    }
                    expect_bool_value = false;
                }
            }
            Ok(Event::Text(t)) => {
                let text: &str = t.as_ref();
                if in_key {
                    if !key_too_long && key_buf.len() + text.len() <= MAX_PLIST_KEY_BYTES {
                        key_buf.push_str(text);
                    } else {
                        key_too_long = true;
                    }
                } else if expect_bool_value && !text.trim().is_empty() {
                    expect_bool_value = false;
                }
            }
            Ok(Event::GeneralRef(g)) => {
                if in_key {
                    let name: &str = g.as_ref();
                    if !key_too_long && key_buf.len() + name.len() + 2 <= MAX_PLIST_KEY_BYTES {
                        key_buf.push('&');
                        key_buf.push_str(name);
                        key_buf.push(';');
                    } else {
                        key_too_long = true;
                    }
                } else if expect_bool_value {
                    expect_bool_value = false;
                }
            }
            Ok(Event::End(e)) => {
                if in_key && e.local_name().as_ref() == "key" {
                    in_key = false;
                    if !key_too_long {
                        let resolved = XmlSanitizer::unescape(key_buf.trim());
                        if ENCRYPTED_PLIST_KEYS.contains(&resolved.trim()) {
                            expect_bool_value = true;
                        }
                    }
                    key_too_long = false;
                } else if expect_bool_value {
                    expect_bool_value = false;
                }
            }
            Ok(Event::Comment(_) | Event::PI(_) | Event::Decl(_) | Event::DocType(_)) => {}
            Ok(Event::CData(_)) => {
                if expect_bool_value {
                    expect_bool_value = false;
                }
            }
            Ok(Event::Eof) | Err(_) => return false,
        }
    }
}

#[inline]
fn read_be_uint(slice: &[u8], size: usize) -> Option<u64> {
    if slice.len() < size {
        return None;
    }
    match size {
        1 => Some(u64::from(slice[0])),
        2 => Some(u64::from(u16::from_be_bytes([slice[0], slice[1]]))),
        4 => Some(u64::from(u32::from_be_bytes([
            slice[0], slice[1], slice[2], slice[3],
        ]))),
        8 => Some(u64::from_be_bytes([
            slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
        ])),
        _ => None,
    }
}

struct BplistView<'a> {
    data: &'a [u8],
    offset_int_size: usize,
    object_ref_size: usize,
    num_objects: u64,
    top_object: u64,
    table_start: usize,
}

impl<'a> BplistView<'a> {
    fn parse(data: &'a [u8]) -> Option<Self> {
        if data.len() < 8 + 32 || !data.starts_with(b"bplist00") {
            return None;
        }
        let trailer_start = data.len() - 32;
        let trailer = &data[trailer_start..];
        let offset_int_size = usize::from(trailer[6]);
        let object_ref_size = usize::from(trailer[7]);
        if !matches!(offset_int_size, 1 | 2 | 4 | 8) || !matches!(object_ref_size, 1 | 2 | 4 | 8) {
            return None;
        }
        let num_objects = u64::from_be_bytes(trailer[8..16].try_into().ok()?);
        let top_object = u64::from_be_bytes(trailer[16..24].try_into().ok()?);
        let offset_table_offset = u64::from_be_bytes(trailer[24..32].try_into().ok()?);

        if num_objects == 0 || num_objects > MAX_BPLIST_OBJECTS as u64 || top_object >= num_objects
        {
            return None;
        }
        let table_start = usize::try_from(offset_table_offset).ok()?;
        if table_start < 8 || table_start >= trailer_start {
            return None;
        }
        let table_bytes = (num_objects as usize).checked_mul(offset_int_size)?;
        if table_start.checked_add(table_bytes)? > trailer_start {
            return None;
        }
        Some(Self {
            data,
            offset_int_size,
            object_ref_size,
            num_objects,
            top_object,
            table_start,
        })
    }

    fn offset_of(&self, obj_idx: u64) -> Option<usize> {
        if obj_idx >= self.num_objects {
            return None;
        }
        let slot = self
            .table_start
            .checked_add((obj_idx as usize).checked_mul(self.offset_int_size)?)?;
        let slice = self.data.get(slot..slot + self.offset_int_size)?;
        let raw = read_be_uint(slice, self.offset_int_size)?;
        let off = usize::try_from(raw).ok()?;
        (off >= 8 && off < self.table_start).then_some(off)
    }

    fn read_count(&self, marker_pos: usize, low: u8) -> Option<(usize, usize)> {
        if low < 0x0F {
            return Some((usize::from(low), marker_pos + 1));
        }
        let int_marker = *self.data.get(marker_pos + 1)?;
        if (int_marker >> 4) != 0x1 {
            return None;
        }
        let pow = int_marker & 0x0F;
        if pow > 3 {
            return None;
        }
        let byte_len = 1usize << pow;
        let start = marker_pos + 2;
        let end = start.checked_add(byte_len)?;
        if end > self.table_start {
            return None;
        }
        let val = read_be_uint(&self.data[start..end], byte_len)?;
        let count = usize::try_from(val).ok()?;
        Some((count, end))
    }

    fn match_encrypted_key(&self, key_idx: u64) -> Option<bool> {
        let off = self.offset_of(key_idx)?;
        let marker = *self.data.get(off)?;
        let kind = marker >> 4;
        let low = marker & 0x0F;
        match kind {
            0x5 | 0x7 => {
                let (len, body_start) = self.read_count(off, low)?;
                if len > MAX_PLIST_BYTES {
                    return None;
                }
                let body_end = body_start.checked_add(len)?;
                if body_end > self.table_start {
                    return None;
                }
                let bytes = &self.data[body_start..body_end];
                Some(ENCRYPTED_PLIST_KEYS.iter().any(|k| bytes == k.as_bytes()))
            }
            0x6 => {
                let (unit_count, body_start) = self.read_count(off, low)?;
                let byte_len = unit_count.checked_mul(2)?;
                if byte_len > MAX_PLIST_BYTES {
                    return None;
                }
                let body_end = body_start.checked_add(byte_len)?;
                if body_end > self.table_start {
                    return None;
                }
                let bytes = &self.data[body_start..body_end];
                Some(ENCRYPTED_PLIST_KEYS.iter().any(|k| {
                    bytes.len() == k.len() * 2
                        && k.as_bytes()
                            .iter()
                            .enumerate()
                            .all(|(i, &b)| bytes[i * 2] == 0 && bytes[i * 2 + 1] == b)
                }))
            }
            _ => Some(false),
        }
    }

    fn check_root_dict(&self, depth: usize) -> Option<bool> {
        if depth > MAX_BPLIST_DEPTH {
            return None;
        }
        let root_off = self.offset_of(self.top_object)?;
        let marker = *self.data.get(root_off)?;
        if (marker >> 4) != 0xD {
            return Some(false);
        }
        let (count, body_start) = self.read_count(root_off, marker & 0x0F)?;
        if count > MAX_BPLIST_DICT_ENTRIES {
            return None;
        }
        let refs_bytes = count.checked_mul(2)?.checked_mul(self.object_ref_size)?;
        let refs_end = body_start.checked_add(refs_bytes)?;
        if refs_end > self.table_start {
            return None;
        }
        for i in 0..count {
            let key_slot = body_start + i * self.object_ref_size;
            let val_slot = body_start + (count + i) * self.object_ref_size;
            let key_idx = read_be_uint(&self.data[key_slot..], self.object_ref_size)?;
            let val_idx = read_be_uint(&self.data[val_slot..], self.object_ref_size)?;
            if key_idx == self.top_object || val_idx == self.top_object {
                return None;
            }
            if self.match_encrypted_key(key_idx)? {
                let val_off = self.offset_of(val_idx)?;
                let val_marker = *self.data.get(val_off)?;
                if val_marker == 0x09 {
                    return Some(true);
                }
            }
        }
        Some(false)
    }
}

fn is_binary_plist_encrypted(data: &[u8]) -> bool {
    BplistView::parse(data)
        .and_then(|view| view.check_root_dict(0))
        .unwrap_or(false)
}

fn is_properties_plist_encrypted(data: &[u8]) -> bool {
    if data.len() > MAX_PLIST_BYTES {
        return false;
    }
    if data.starts_with(b"bplist00") {
        is_binary_plist_encrypted(data)
    } else {
        is_xml_plist_encrypted(data)
    }
}

fn check_nested_index_zip_encryption(bytes: &[u8]) -> Result<(), EasyOfficeError> {
    if is_ole2_offcrypto_encrypted(bytes) {
        return Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pages,
        ));
    }
    if bytes.starts_with(&FormatDetector::ZIP_MAGIC) {
        if let Ok(mut opened) = open_guarded(
            Cursor::new(bytes),
            OpenLimits {
                max_entries: usize::from(u16::MAX),
                max_directory_bytes: 16 * 1024 * 1024,
            },
        ) {
            for index in 0..opened.archive.len() {
                if let Ok(file) = opened.archive.by_index_raw(index) {
                    if file.encrypted() {
                        return Err(EasyOfficeError::EncryptedDocumentUnsupported(
                            DocumentFormat::Pages,
                        ));
                    }
                    if std::str::from_utf8(file.name_raw()).is_ok_and(|name| {
                        ENCRYPTED_ENTRY_SUFFIXES
                            .iter()
                            .any(|suffix| ends_with_ignore_ascii_case(name, suffix))
                    }) {
                        return Err(EasyOfficeError::EncryptedDocumentUnsupported(
                            DocumentFormat::Pages,
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

fn remaining_zip_policy(
    policy: &ZipSecurityPolicy,
    outer_entries: &[ZipArchiveEntry],
) -> Result<(ZipSecurityPolicy, u64), EasyOfficeError> {
    let outer_bytes = outer_entries
        .iter()
        .try_fold(0u64, |acc, e| acc.checked_add(e.data.len() as u64))
        .ok_or_else(|| {
            EasyOfficeError::ResourceBudgetExceeded(
                "Pages 패키지 압축 해제 크기가 한도를 초과했습니다.".to_string(),
            )
        })?;
    let remaining_entries = policy.max_entries.saturating_sub(outer_entries.len());
    let remaining_bytes = policy
        .max_total_uncompressed_bytes
        .saturating_sub(outer_bytes);
    let inner_policy = ZipSecurityPolicy {
        max_entries: remaining_entries,
        max_single_entry_uncompressed_bytes: policy
            .max_single_entry_uncompressed_bytes
            .min(remaining_bytes),
        max_total_uncompressed_bytes: remaining_bytes,
        max_compression_ratio: policy.max_compression_ratio,
        binary_budget: policy.binary_budget.map(|b| ZipBinaryBudget {
            max_single_entry_uncompressed_bytes: b
                .max_single_entry_uncompressed_bytes
                .min(remaining_bytes),
            max_total_uncompressed_bytes: b.max_total_uncompressed_bytes.min(remaining_bytes),
        }),
    };
    Ok((inner_policy, remaining_bytes))
}

fn normalize_index_zip_path(raw: &str) -> Result<String, EasyOfficeError> {
    if raw.ends_with('/') {
        let trimmed = raw.trim_end_matches('/');
        if raw.len() - trimmed.len() != 1 {
            return Err(EasyOfficeError::SecurityViolation(format!(
                "Index.zip 내부의 비정상 경로: {raw}"
            )));
        }
        if raw == "Index/" {
            return Ok(raw.to_string());
        }
        if let Some(rest) = raw.strip_prefix("Index/") {
            if rest
                .split('/')
                .next()
                .is_some_and(|seg| seg.eq_ignore_ascii_case("index"))
            {
                return Err(EasyOfficeError::SecurityViolation(format!(
                    "Index.zip 내부의 중복 Index/ 경로: {raw}"
                )));
            }
            ZipSecurityGuard::validate_entry_path(raw)?;
            return Ok(raw.to_string());
        }
        if raw == "Tables/" || raw.starts_with("Tables/") {
            let normalized = format!("Index/{raw}");
            ZipSecurityGuard::validate_entry_path(&normalized)?;
            return Ok(normalized);
        }
        return Err(EasyOfficeError::SecurityViolation(format!(
            "Index.zip 내부의 비정상 경로: {raw}"
        )));
    }

    if let Some(rest) = raw.strip_prefix("Index/") {
        if rest.is_empty()
            || rest
                .split('/')
                .next()
                .is_some_and(|seg| seg.eq_ignore_ascii_case("index"))
        {
            return Err(EasyOfficeError::SecurityViolation(format!(
                "Index.zip 내부의 중복 Index/ 경로: {raw}"
            )));
        }
        ZipSecurityGuard::validate_entry_path(raw)?;
        return Ok(raw.to_string());
    }
    if raw
        .split('/')
        .next()
        .is_some_and(|seg| seg.eq_ignore_ascii_case("index"))
    {
        return Err(EasyOfficeError::SecurityViolation(format!(
            "Index.zip 내부의 비정상 경로: {raw}"
        )));
    }
    if let Some(rest) = raw.strip_prefix("Tables/") {
        if rest.is_empty() {
            return Err(EasyOfficeError::SecurityViolation(format!(
                "Index.zip 내부의 비정상 경로: {raw}"
            )));
        }
        let normalized = format!("Index/{raw}");
        ZipSecurityGuard::validate_entry_path(&normalized)?;
        return Ok(normalized);
    }
    if !raw.contains('/')
        && (ends_with_ignore_ascii_case(raw, ".iwa")
            || ENCRYPTED_ENTRY_SUFFIXES
                .iter()
                .any(|suffix| ends_with_ignore_ascii_case(raw, suffix)))
    {
        let normalized = format!("Index/{raw}");
        ZipSecurityGuard::validate_entry_path(&normalized)?;
        return Ok(normalized);
    }
    Err(EasyOfficeError::SecurityViolation(format!(
        "Index.zip 내부의 비정상 경로: {raw}"
    )))
}

fn unpack_and_merge_index_zip(
    entries: Vec<ZipArchiveEntry>,
    prefix: &str,
    policy: &ZipSecurityPolicy,
) -> Result<Vec<ZipArchiveEntry>, EasyOfficeError> {
    check_nested_index_zip_encryption(
        &entries
            .iter()
            .find(|e| e.path.strip_prefix(prefix).unwrap_or(&e.path) == "Index.zip")
            .ok_or_else(|| {
                EasyOfficeError::CorruptedContainer("Pages 패키지에 Index.zip이 없습니다.".into())
            })?
            .data,
    )?;

    let (inner_policy, remaining_bytes) = remaining_zip_policy(policy, &entries)?;
    let mut index_zip_bytes = None;
    let mut outer_normalized = Vec::with_capacity(entries.len());
    for entry in entries {
        let relative = entry.path.strip_prefix(prefix).unwrap_or(&entry.path);
        if relative == "Index.zip" {
            index_zip_bytes = Some(entry.data);
        } else {
            let owned_rel = relative.to_string();
            outer_normalized.push(ZipArchiveEntry {
                path: owned_rel,
                data: entry.data,
                crc32: entry.crc32,
            });
        }
    }
    let index_zip_bytes = index_zip_bytes.ok_or_else(|| {
        EasyOfficeError::CorruptedContainer("Pages 패키지에 Index.zip이 없습니다.".into())
    })?;

    let raw_inner = ZipArchiveCodec::unpack(&index_zip_bytes, &inner_policy).map_err(|err| {
        match err {
            EasyOfficeError::EncryptedDocumentUnsupported(_) => {
                EasyOfficeError::EncryptedDocumentUnsupported(DocumentFormat::Pages)
            }
            EasyOfficeError::SecurityViolation(ref msg) if msg.contains("엔트리 개수") => {
                EasyOfficeError::ResourceBudgetExceeded(msg.clone())
            }
            EasyOfficeError::DecompressionBombDetected { ratio, max_ratio }
                if ratio <= max_ratio =>
            {
                EasyOfficeError::ResourceBudgetExceeded(format!(
                    "중첩 Index.zip 압축 해제 크기가 잔여 예산({remaining_bytes} bytes)을 초과했습니다."
                ))
            }
            other => other,
        }
    })?;

    let mut inner_tracker = ZipPathTracker::with_capacity(raw_inner.len());
    let mut normalized_inner = Vec::with_capacity(raw_inner.len());
    for entry in raw_inner {
        let norm_path = normalize_index_zip_path(&entry.path)?;
        let desc = ZipEntryDescriptor::new(
            norm_path.clone(),
            entry.data.len() as u64,
            entry.data.len() as u64,
            false,
        );
        inner_tracker.check(&desc)?;
        if ENCRYPTED_ENTRY_SUFFIXES
            .iter()
            .any(|suffix| ends_with_ignore_ascii_case(&norm_path, suffix))
        {
            return Err(EasyOfficeError::EncryptedDocumentUnsupported(
                DocumentFormat::Pages,
            ));
        }
        normalized_inner.push(ZipArchiveEntry {
            path: norm_path,
            data: entry.data,
            crc32: entry.crc32,
        });
    }

    let mut merged_tracker =
        ZipPathTracker::with_capacity(outer_normalized.len() + normalized_inner.len());
    for entry in &outer_normalized {
        if entry.path.ends_with('/') {
            continue;
        }
        let desc = ZipEntryDescriptor::new(
            entry.path.clone(),
            entry.data.len() as u64,
            entry.data.len() as u64,
            false,
        );
        merged_tracker.check(&desc)?;
    }
    for entry in &normalized_inner {
        if entry.path.ends_with('/') {
            continue;
        }
        let desc = ZipEntryDescriptor::new(
            entry.path.clone(),
            entry.data.len() as u64,
            entry.data.len() as u64,
            false,
        );
        merged_tracker.check(&desc)?;
    }

    outer_normalized.extend(normalized_inner);
    Ok(outer_normalized)
}

/// Loads every IWA record of a Pages package under `ZipSecurityPolicy::default()`.
pub fn load_package(bytes: &[u8]) -> Result<PagesPackage, EasyOfficeError> {
    load_package_with_policy(bytes, &ZipSecurityPolicy::default())
}

/// Loads every IWA record of a Pages package under `policy`. Legacy XML and encrypted packages
/// are refused, and nested `Index.zip` archives share the remaining `policy` budget.
pub fn load_package_with_policy(
    bytes: &[u8],
    policy: &ZipSecurityPolicy,
) -> Result<PagesPackage, EasyOfficeError> {
    let entries = ZipArchiveCodec::unpack(bytes, policy).map_err(|err| match err {
        EasyOfficeError::EncryptedDocumentUnsupported(_) => {
            EasyOfficeError::EncryptedDocumentUnsupported(DocumentFormat::Pages)
        }
        other => other,
    })?;
    let prefix_owned = strip_package_prefix(&entries).to_string();

    let encrypted_entry = entries.iter().any(|e| {
        let relative = e.path.strip_prefix(&prefix_owned).unwrap_or(&e.path);
        ENCRYPTED_ENTRY_SUFFIXES
            .iter()
            .any(|suffix| ends_with_ignore_ascii_case(relative, suffix))
    });
    let encrypted_plist = entries.iter().any(|e| {
        e.path.strip_prefix(&prefix_owned).unwrap_or(&e.path) == "Metadata/Properties.plist"
            && is_properties_plist_encrypted(&e.data)
    });
    if encrypted_entry || encrypted_plist {
        return Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pages,
        ));
    }

    let has_document_iwa = entries
        .iter()
        .any(|e| e.path.strip_prefix(&prefix_owned).unwrap_or(&e.path) == "Index/Document.iwa");
    let has_index_zip = entries
        .iter()
        .any(|e| e.path.strip_prefix(&prefix_owned).unwrap_or(&e.path) == "Index.zip");

    let (entries, prefix_owned) = if !has_document_iwa && has_index_zip {
        (
            unpack_and_merge_index_zip(entries, &prefix_owned, policy)?,
            String::new(),
        )
    } else {
        (entries, prefix_owned)
    };

    let has_iwa = entries.iter().any(|e| {
        let r = e.path.strip_prefix(&prefix_owned).unwrap_or(&e.path);
        r.starts_with("Index/") && r.ends_with(".iwa")
    });
    if !has_iwa {
        if entries.iter().any(|e| {
            matches!(
                e.path.strip_prefix(&prefix_owned).unwrap_or(&e.path),
                "index.xml" | "index.xml.gz"
            )
        }) {
            return Err(EasyOfficeError::InvalidArgument(LEGACY_MESSAGE.to_string()));
        }
        return Err(EasyOfficeError::CorruptedContainer(
            "Pages 패키지에 Index/*.iwa 항목이 없습니다.".to_string(),
        ));
    }

    let mut iwa: Vec<(&str, &[u8])> = entries
        .iter()
        .filter_map(|e| {
            let r = e.path.strip_prefix(&prefix_owned).unwrap_or(&e.path);
            (r.starts_with("Index/") && r.ends_with(".iwa")).then_some((r, e.data.as_slice()))
        })
        .collect();
    iwa.sort_by(|a, b| a.0.cmp(b.0));
    let mut remaining = MAX_DOCUMENT_BYTES;
    let mut pkg = PagesPackage::default();
    let mut stream = Vec::new();
    for (_, data) in &iwa {
        decompress_iwa_into(data, &mut remaining, &mut stream)?;
        decode_stream_into_table(&stream, &mut pkg.objects)?;
        pkg.iwa_files += 1;
    }
    drop(iwa);
    for entry in entries {
        let relative = entry
            .path
            .strip_prefix(prefix_owned.as_str())
            .unwrap_or(&entry.path);
        if relative.starts_with(DATA_DIR) {
            pkg.data_files.insert(relative.to_string(), entry.data);
        }
    }
    Ok(pkg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bplist(entries: &[(&[u8], bool, bool)]) -> Vec<u8> {
        let count = 1 + entries.len() * 2;
        let mut body = Vec::new();
        let mut offsets = vec![8usize];
        if entries.len() < 15 {
            body.push(0xD0 | (entries.len() as u8));
        } else {
            body.push(0xDF);
            body.push(0x10);
            body.push(entries.len() as u8);
        }
        for i in 0..entries.len() {
            body.push((1 + i) as u8);
        }
        for i in 0..entries.len() {
            body.push((1 + entries.len() + i) as u8);
        }
        for (key, utf16be, _) in entries {
            offsets.push(8 + body.len());
            if *utf16be {
                let units = key.len();
                assert!(units < 15);
                body.push(0x60 | (units as u8));
                for &b in *key {
                    body.push(0);
                    body.push(b);
                }
            } else if key.len() < 15 {
                body.push(0x50 | (key.len() as u8));
                body.extend_from_slice(key);
            } else {
                body.push(0x5F);
                body.push(0x10);
                body.push(key.len() as u8);
                body.extend_from_slice(key);
            }
        }
        for (_, _, flag) in entries {
            offsets.push(8 + body.len());
            body.push(if *flag { 0x09 } else { 0x08 });
        }
        let table_at = 8 + body.len();
        let off_size = if table_at <= 0xFF { 1usize } else { 2usize };
        let mut out = Vec::from(&b"bplist00"[..]);
        out.extend_from_slice(&body);
        for off in offsets {
            if off_size == 1 {
                out.push(off as u8);
            } else {
                out.extend_from_slice(&(off as u16).to_be_bytes());
            }
        }
        out.extend_from_slice(&[0; 6]);
        out.push(off_size as u8);
        out.push(1);
        out.extend_from_slice(&(count as u64).to_be_bytes());
        out.extend_from_slice(&0u64.to_be_bytes());
        out.extend_from_slice(&(table_at as u64).to_be_bytes());
        out
    }

    #[test]
    fn strip_package_prefix_handles_all_single_folder_markers_and_rejects_nested() {
        for path in [
            "Doc.pages/Index/Document.iwa",
            "Doc.pages/Metadata/Properties.plist",
            "Doc.pages/index.xml",
            "Doc.pages/index.xml.gz",
            "Doc.pages/Index.zip",
            "Doc.pages/Secret.iwpv2",
            "Doc.pages/Secret.iwph",
        ] {
            let entries = vec![ZipArchiveEntry::new(path, vec![])];
            assert_eq!(strip_package_prefix(&entries), "Doc.pages/", "{path}");
        }
        let nested = vec![ZipArchiveEntry::new("a/b/Index/Document.iwa", vec![])];
        assert_eq!(strip_package_prefix(&nested), "");
    }

    #[test]
    fn xml_plist_ignores_comments_and_non_element_text() {
        let commented = br#"<?xml version="1.0"?><plist><dict><!-- <key>isEncrypted</key><true/> --><key>isEncrypted</key><false/></dict></plist>"#;
        assert!(!is_properties_plist_encrypted(commented));

        let stray_text = br#"<?xml version="1.0"?><plist><dict><key>isEncrypted</key>noise<true/></dict></plist>"#;
        assert!(!is_properties_plist_encrypted(stray_text));

        let second_true = br#"<plist><dict><key>isEncrypted</key><false/><key>documentEncrypted</key><true></true></dict></plist>"#;
        assert!(is_properties_plist_encrypted(second_true));
    }

    #[test]
    fn bplist_parser_handles_utf16_large_dicts_and_hostile_boundaries_safely() {
        assert!(!is_properties_plist_encrypted(b"bplist00"));

        let utf16_true = make_bplist(&[(b"isEncrypted", true, true)]);
        assert!(is_properties_plist_encrypted(&utf16_true));

        let utf16_false = make_bplist(&[(b"isEncrypted", true, false)]);
        assert!(!is_properties_plist_encrypted(&utf16_false));

        let mut many_false: Vec<(&[u8], bool, bool)> =
            (0..15).map(|_| (&b"otherKey"[..], false, false)).collect();
        many_false.push((b"documentEncrypted", false, true));
        let large_dict_true = make_bplist(&many_false);
        assert!(is_properties_plist_encrypted(&large_dict_true));

        let base = make_bplist(&[(b"isEncrypted", false, true)]);
        let mutators: [fn(&mut Vec<u8>); 9] = [
            |b| {
                let len = b.len();
                b[len - 26] = 3;
            },
            |b| {
                let len = b.len();
                b[len - 25] = 0;
            },
            |b| {
                let len = b.len();
                b[len - 24..len - 16].copy_from_slice(&0u64.to_be_bytes());
            },
            |b| {
                let len = b.len();
                b[len - 24..len - 16].copy_from_slice(&u64::MAX.to_be_bytes());
            },
            |b| {
                let len = b.len();
                b[len - 16..len - 8].copy_from_slice(&99u64.to_be_bytes());
            },
            |b| {
                let len = b.len();
                b[len - 8..len].copy_from_slice(&4u64.to_be_bytes());
            },
            |b| {
                let len = b.len();
                b[len - 8..len].copy_from_slice(&((len - 33) as u64).to_be_bytes());
            },
            |b| b[9] = 0,
            |b| b[8] = 0xA1,
        ];
        for mutator in mutators {
            let mut corrupt = base.clone();
            mutator(&mut corrupt);
            assert!(!is_properties_plist_encrypted(&corrupt));
        }
    }
}

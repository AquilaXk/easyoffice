//! Pages package (ZIP) handling: layout detection, legacy/encrypted refusal and loading every
//! `Index/**/*.iwa` file into one [`ObjectTable`].
//!
//! The ZIP is unpacked through [`ZipArchiveCodec::unpack`] with the default
//! [`ZipSecurityPolicy`] (entry count, size and ratio ceilings, path traversal). Package-directory
//! zips (all entries below a single `Name.pages/` folder) are handled by stripping that prefix.

use crate::pages_iwa::{decode_stream_into_table, decompress_iwa_into, MAX_DOCUMENT_BYTES};
use crate::pages_proto::ObjectTable;
use crate::{DocumentFormat, EasyOfficeError, ZipArchiveCodec, ZipArchiveEntry, ZipSecurityPolicy};
use std::collections::HashMap;

/// Package directory that holds picture and media files.
const DATA_DIR: &str = "Data/";

/// Entry-name suffixes that mark an encrypted/password-protected package. 검증 필요
const ENCRYPTED_ENTRY_SUFFIXES: [&str; 1] = [".iwph"];
/// Markers inside `Metadata/Properties.plist` that mark encryption. 검증 필요
const ENCRYPTED_PLIST_MARKERS: [&str; 2] = ["isEncrypted", "documentEncrypted"];

pub(crate) const LEGACY_MESSAGE: &str = "2009년 이전 형식(index.xml) Pages 문서는 아직 지원하지 않습니다. Pages에서 문서를 열어 최신 형식(.pages) 또는 DOCX로 다시 저장한 뒤 열어 주세요.";

/// Decoded package: all IWA objects plus how many `.iwa` files were read.
#[derive(Debug, Default)]
pub struct PagesPackage {
    pub objects: ObjectTable,
    pub iwa_files: usize,
    /// Files under `Data/` (pictures and media), keyed by package-relative path.
    pub data_files: HashMap<String, Vec<u8>>,
}

fn strip_package_prefix(entries: &[ZipArchiveEntry]) -> &str {
    for e in entries {
        if let Some(idx) = e.path.find("Index/") {
            let prefix = &e.path[..idx];
            if prefix.is_empty() || prefix.ends_with('/') {
                return prefix;
            }
        }
    }
    ""
}

/// Loads every IWA record of a Pages package. Legacy XML and encrypted packages are refused.
pub fn load_package(bytes: &[u8]) -> Result<PagesPackage, EasyOfficeError> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())?;
    let prefix = strip_package_prefix(&entries);

    let has_iwa = entries.iter().any(|e| {
        let r = e.path.strip_prefix(prefix).unwrap_or(&e.path);
        r.starts_with("Index/") && r.ends_with(".iwa")
    });
    if !has_iwa {
        if entries.iter().any(|e| {
            matches!(
                e.path.strip_prefix(prefix).unwrap_or(&e.path),
                "index.xml" | "index.xml.gz"
            )
        }) {
            return Err(EasyOfficeError::InvalidArgument(LEGACY_MESSAGE.to_string()));
        }
        return Err(EasyOfficeError::CorruptedContainer(
            "Pages 패키지에 Index/*.iwa 항목이 없습니다.".to_string(),
        ));
    }
    let encrypted_entry = entries
        .iter()
        .any(|e| ENCRYPTED_ENTRY_SUFFIXES.iter().any(|s| e.path.ends_with(s)));
    let encrypted_plist = entries.iter().any(|e| {
        e.path.strip_prefix(prefix).unwrap_or(&e.path) == "Metadata/Properties.plist"
            && ENCRYPTED_PLIST_MARKERS
                .iter()
                .any(|m| e.data.windows(m.len()).any(|w| w == m.as_bytes()))
    });
    if encrypted_entry || encrypted_plist {
        return Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pages,
        ));
    }

    let mut iwa: Vec<(&str, &[u8])> = entries
        .iter()
        .filter_map(|e| {
            let r = e.path.strip_prefix(prefix).unwrap_or(&e.path);
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
    let prefix_owned = prefix.to_string();
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

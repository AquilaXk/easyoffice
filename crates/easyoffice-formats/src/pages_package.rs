//! Pages package (ZIP) handling: layout detection, legacy/encrypted refusal and loading every
//! `Index/**/*.iwa` file into one [`ObjectTable`].
//!
//! The ZIP is unpacked through [`ZipArchiveCodec::unpack`] with the default
//! [`ZipSecurityPolicy`] (entry count, size and ratio ceilings, path traversal). Package-directory
//! zips (all entries below a single `Name.pages/` folder) are handled by stripping that prefix.

use crate::pages_iwa::{add_to_table, decompress_iwa, split_records, MAX_DOCUMENT_BYTES};
use crate::pages_proto::ObjectTable;
use crate::{DocumentFormat, EasyOfficeError, ZipArchiveCodec, ZipSecurityPolicy};
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

fn strip_package_prefix(paths: &[String]) -> String {
    for p in paths {
        if let Some(idx) = p.find("Index/") {
            let prefix = &p[..idx];
            if prefix.is_empty() || prefix.ends_with('/') {
                return prefix.to_string();
            }
        }
    }
    String::new()
}

/// Loads every IWA record of a Pages package. Legacy XML and encrypted packages are refused.
pub fn load_package(bytes: &[u8]) -> Result<PagesPackage, EasyOfficeError> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())?;
    let paths: Vec<String> = entries.iter().map(|e| e.path.clone()).collect();
    let prefix = strip_package_prefix(&paths);
    let rel = |p: &str| p.strip_prefix(prefix.as_str()).unwrap_or(p).to_string();

    let has_iwa = paths
        .iter()
        .any(|p| rel(p).starts_with("Index/") && p.ends_with(".iwa"));
    if !has_iwa {
        if paths
            .iter()
            .any(|p| matches!(rel(p).as_str(), "index.xml" | "index.xml.gz"))
        {
            return Err(EasyOfficeError::InvalidArgument(LEGACY_MESSAGE.to_string()));
        }
        return Err(EasyOfficeError::CorruptedContainer(
            "Pages 패키지에 Index/*.iwa 항목이 없습니다.".to_string(),
        ));
    }
    let encrypted_entry = paths
        .iter()
        .any(|p| ENCRYPTED_ENTRY_SUFFIXES.iter().any(|s| p.ends_with(s)));
    let encrypted_plist = entries.iter().any(|e| {
        rel(&e.path) == "Metadata/Properties.plist"
            && ENCRYPTED_PLIST_MARKERS
                .iter()
                .any(|m| e.data.windows(m.len()).any(|w| w == m.as_bytes()))
    });
    if encrypted_entry || encrypted_plist {
        return Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pages,
        ));
    }

    let mut iwa: Vec<(String, &[u8])> = entries
        .iter()
        .filter_map(|e| {
            let r = rel(&e.path);
            (r.starts_with("Index/") && r.ends_with(".iwa")).then_some((r, e.data.as_slice()))
        })
        .collect();
    iwa.sort_by(|a, b| a.0.cmp(&b.0));
    let mut remaining = MAX_DOCUMENT_BYTES;
    let mut pkg = PagesPackage::default();
    for (_, data) in &iwa {
        let stream = decompress_iwa(data, &mut remaining)?;
        add_to_table(&mut pkg.objects, split_records(&stream)?);
        pkg.iwa_files += 1;
    }
    drop(iwa);
    for entry in entries {
        let relative = entry
            .path
            .strip_prefix(prefix.as_str())
            .unwrap_or(&entry.path);
        if relative.starts_with(DATA_DIR) {
            pkg.data_files.insert(relative.to_string(), entry.data);
        }
    }
    Ok(pkg)
}

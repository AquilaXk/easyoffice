//! Encrypted container detection for `[MS-OFFCRYPTO]` OLE2 wrappers,
//! `ODF 1.3` / `KS X 6101` (`META-INF/manifest.xml`) packages, and ZIP XML part validation.

use crate::xml_stream::{unescape_cow, XmlEvent, XmlReader};
use crate::{
    DocumentFormat, EasyOfficeError, FormatDetector, HwpxCodec, OdfCodec, Ole2ContainerCodec,
    XmlSanitizer, ZipArchiveEntry,
};

/// Upper bound on XML events read from `META-INF/manifest.xml` before failing closed.
const MAX_MANIFEST_XML_EVENTS: usize = 262_144;

#[inline]
fn has_xml_entry_ext(path: &str) -> bool {
    let bytes = path.as_bytes();
    let tail_eq = |ext: &[u8]| {
        bytes.len() >= ext.len() && bytes[bytes.len() - ext.len()..].eq_ignore_ascii_case(ext)
    };
    tail_eq(b".xml") || tail_eq(b".rels") || tail_eq(b".hpf")
}

/// Returns `true` when `bytes` is a valid OLE2 CFBF container carrying an `[MS-OFFCRYPTO]`
/// `EncryptionInfo` or `EncryptedPackage` stream in its root storage (PD-361). A stream of those
/// names inside a nested storage belongs to an embedded object (a `.doc` can hold an encrypted
/// OOXML object) and does not encrypt the container. Fails closed to `false` on unpack errors or
/// oversized inputs under [`Ole2SecurityGuard`](crate::Ole2SecurityGuard) limits.
pub fn is_ole2_offcrypto_encrypted(bytes: &[u8]) -> bool {
    if bytes.len() > FormatDetector::MAX_PAYLOAD_BYTES
        || !bytes.starts_with(&FormatDetector::OLE2_MAGIC)
    {
        return false;
    }
    let Ok(streams) = Ole2ContainerCodec::unpack(bytes) else {
        return false;
    };
    // A name with a `/` lies in a nested storage.
    streams.iter().any(|entry| {
        entry.name.eq_ignore_ascii_case("EncryptionInfo")
            || entry.name.eq_ignore_ascii_case("EncryptedPackage")
    })
}

fn find_manifest_entry(entries: &[ZipArchiveEntry]) -> Option<&ZipArchiveEntry> {
    entries
        .iter()
        .find(|e| e.path == "META-INF/manifest.xml")
        .or_else(|| {
            entries.iter().find(|e| {
                e.path
                    .trim_start_matches('/')
                    .eq_ignore_ascii_case("META-INF/manifest.xml")
            })
        })
}

fn hwpx_or_hwtx(
    entries: &[ZipArchiveEntry],
    fallback_fmt: Option<DocumentFormat>,
) -> DocumentFormat {
    let template_meta = crate::hwpx_codec::HWPX_TEMPLATE_META_MARKER.as_bytes();
    let is_template = entries.iter().any(|e| {
        let path = e.path.trim_start_matches('/');
        (path == "Contents/header.xml"
            && FormatDetector::slice_contains(&e.data, b"isTemplate=\"1\""))
            || (path == "Contents/content.hpf"
                && FormatDetector::slice_contains(&e.data, template_meta))
    });
    if is_template || fallback_fmt == Some(DocumentFormat::Hwtx) {
        DocumentFormat::Hwtx
    } else {
        DocumentFormat::Hwpx
    }
}

fn has_hwpx_structure_entry(entries: &[ZipArchiveEntry]) -> bool {
    entries.iter().any(|e| {
        let path = e.path.trim_start_matches('/');
        path == "Contents/header.xml" || path == "Contents/content.hpf" || path == "version.xml"
    })
}

fn format_from_mime_str(
    raw_mime: &str,
    entries: &[ZipArchiveEntry],
    fallback_fmt: Option<DocumentFormat>,
) -> Option<DocumentFormat> {
    let mime = raw_mime.trim();
    if mime.eq_ignore_ascii_case(OdfCodec::MIMETYPE_ODT) {
        Some(DocumentFormat::Odt)
    } else if mime.eq_ignore_ascii_case(OdfCodec::MIMETYPE_ODS) {
        Some(DocumentFormat::Ods)
    } else if mime.eq_ignore_ascii_case(OdfCodec::MIMETYPE_ODP) {
        Some(DocumentFormat::Odp)
    } else if mime.eq_ignore_ascii_case(HwpxCodec::MIMETYPE_HWPX) {
        Some(hwpx_or_hwtx(entries, fallback_fmt))
    } else {
        None
    }
}

fn resolve_manifest_encrypted_format(
    entries: &[ZipArchiveEntry],
    root_manifest_mime: Option<&str>,
    first_manifest_mime: Option<&str>,
    fallback_fmt: Option<DocumentFormat>,
) -> DocumentFormat {
    let from_mimetype = entries
        .iter()
        .find(|e| e.path == "mimetype")
        .or_else(|| {
            entries.iter().find(|e| {
                e.path
                    .trim_start_matches('/')
                    .eq_ignore_ascii_case("mimetype")
            })
        })
        .and_then(|e| std::str::from_utf8(&e.data).ok())
        .and_then(|mime| format_from_mime_str(mime, entries, fallback_fmt));

    if let Some(fmt) = from_mimetype {
        return fmt;
    }

    let from_manifest = root_manifest_mime
        .and_then(|m| format_from_mime_str(m, entries, fallback_fmt))
        .or_else(|| {
            first_manifest_mime.and_then(|m| format_from_mime_str(m, entries, fallback_fmt))
        })
        .or_else(|| has_hwpx_structure_entry(entries).then(|| hwpx_or_hwtx(entries, fallback_fmt)));

    from_manifest
        .or(fallback_fmt)
        .unwrap_or(DocumentFormat::Odt)
}

/// Checks `META-INF/manifest.xml` inside `entries` for `<manifest:encryption-data` /
/// `<encryption-data` elements (`ODF 1.3` Part 2 §3.4 / `KS X 6101`). When encrypted, returns
/// `Err(EasyOfficeError::EncryptedDocumentUnsupported(fmt))` before any body XML UTF-8 checks run.
pub fn check_zip_manifest_encryption(
    entries: &[ZipArchiveEntry],
    fallback_fmt: Option<DocumentFormat>,
) -> Result<(), EasyOfficeError> {
    let Some(manifest) = find_manifest_entry(entries) else {
        return Ok(());
    };
    let xml = std::str::from_utf8(&manifest.data).map_err(|_| {
        EasyOfficeError::InvalidUtf8(format!("ZIP XML 엔트리 UTF-8 오류: {}", manifest.path))
    })?;
    XmlSanitizer::validate_safe_xml(xml)?;

    let mut has_encryption = false;
    let mut root_manifest_mime: Option<String> = None;
    let mut first_manifest_mime: Option<String> = None;
    let mut reader = XmlReader::with_budget(xml, MAX_MANIFEST_XML_EVENTS);
    loop {
        let ev = match reader.next_event() {
            Ok(Some(ev)) => ev,
            Ok(None) => break,
            Err(err) => {
                if has_encryption {
                    break;
                }
                return Err(err);
            }
        };
        if let XmlEvent::Start { name, attrs } = ev {
            let local = name.rsplit(':').next().unwrap_or(name);
            if local == "encryption-data" {
                has_encryption = true;
            } else if local == "file-entry" {
                let media = attrs
                    .iter_raw()
                    .find(|(k, _)| k.rsplit(':').next() == Some("media-type"))
                    .map(|(_, v)| unescape_cow(v));
                if let Some(media) = media {
                    let full_path = attrs
                        .iter_raw()
                        .find(|(k, _)| k.rsplit(':').next() == Some("full-path"))
                        .map(|(_, v)| unescape_cow(v));
                    if full_path.as_deref() == Some("/") {
                        if root_manifest_mime.is_none() {
                            root_manifest_mime = Some(media.into_owned());
                        }
                    } else if first_manifest_mime.is_none()
                        && format_from_mime_str(&media, entries, fallback_fmt).is_some()
                    {
                        first_manifest_mime = Some(media.into_owned());
                    }
                }
            }
        }
    }

    if has_encryption {
        let fmt = resolve_manifest_encrypted_format(
            entries,
            root_manifest_mime.as_deref(),
            first_manifest_mime.as_deref(),
            fallback_fmt,
        );
        return Err(EasyOfficeError::EncryptedDocumentUnsupported(fmt));
    }
    Ok(())
}

/// Validates `META-INF/manifest.xml` encryption first, then UTF-8 and XXE-checks every XML/RELS/HPF
/// entry once.
pub(crate) fn checked_zip_xml_entries_for(
    entries: &[ZipArchiveEntry],
    fallback_fmt: Option<DocumentFormat>,
) -> Result<Vec<(&str, &str)>, EasyOfficeError> {
    check_zip_manifest_encryption(entries, fallback_fmt)?;
    let mut parts = Vec::new();
    for entry in entries {
        if entry.path.ends_with('/') {
            continue;
        }
        if has_xml_entry_ext(&entry.path) {
            let xml = std::str::from_utf8(&entry.data).map_err(|_| {
                EasyOfficeError::InvalidUtf8(format!("ZIP XML 엔트리 UTF-8 오류: {}", entry.path))
            })?;
            XmlSanitizer::validate_safe_xml(xml)?;
            parts.push((entry.path.as_str(), xml));
        }
    }
    Ok(parts)
}

pub(crate) fn validate_zip_xml_entries_for(
    entries: &[ZipArchiveEntry],
    fallback_fmt: Option<DocumentFormat>,
) -> Result<(), EasyOfficeError> {
    checked_zip_xml_entries_for(entries, fallback_fmt).map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offcrypto_rejects_corrupt_truncated_or_non_ole2_inputs_without_panic() {
        assert!(!is_ole2_offcrypto_encrypted(b""));
        assert!(!is_ole2_offcrypto_encrypted(b"PK\x03\x04not-ole2"));
        assert!(!is_ole2_offcrypto_encrypted(&FormatDetector::OLE2_MAGIC));
    }

    #[test]
    fn offcrypto_detects_case_insensitive_root_stream_names_only() {
        let root = Ole2ContainerCodec::pack(&[
            ("encryptioninfo".to_string(), vec![1, 2, 3]),
            ("Other".to_string(), vec![4]),
        ])
        .unwrap();
        assert!(is_ole2_offcrypto_encrypted(&root));
        // PD-361: a nested storage holds an embedded object, not the encryption of the container
        let nested = Ole2ContainerCodec::pack(&[
            ("RootSub/encryptioninfo".to_string(), vec![1, 2, 3]),
            ("Other".to_string(), vec![4]),
        ])
        .unwrap();
        assert!(!is_ole2_offcrypto_encrypted(&nested));
    }

    #[test]
    fn manifest_check_handles_comments_unprefixed_tags_and_format_fallbacks() {
        // Commented-out encryption-data is ignored; fallback format resolves when mimetype is absent.
        let benign = vec![ZipArchiveEntry::new(
            "META-INF/manifest.xml",
            b"<manifest><!-- <manifest:encryption-data/> --></manifest>".to_vec(),
        )];
        assert_eq!(check_zip_manifest_encryption(&benign, None), Ok(()));

        let unprefixed = vec![ZipArchiveEntry::new(
            "META-INF/manifest.xml",
            b"<manifest><file-entry full-path=\"content.xml\"><encryption-data checksum=\"x\"/></file-entry></manifest>".to_vec(),
        )];
        assert_eq!(
            check_zip_manifest_encryption(&unprefixed, Some(DocumentFormat::Ods)),
            Err(EasyOfficeError::EncryptedDocumentUnsupported(
                DocumentFormat::Ods
            ))
        );
        assert_eq!(
            check_zip_manifest_encryption(&unprefixed, None),
            Err(EasyOfficeError::EncryptedDocumentUnsupported(
                DocumentFormat::Odt
            ))
        );

        let manifest_mime_odp = vec![ZipArchiveEntry::new(
            "/META-INF/MANIFEST.XML",
            b"<manifest><file-entry full-path=\"/\" media-type=\"application/vnd.oasis.opendocument.presentation\"/><encryption-data/></manifest>".to_vec(),
        )];
        assert_eq!(
            check_zip_manifest_encryption(&manifest_mime_odp, None),
            Err(EasyOfficeError::EncryptedDocumentUnsupported(
                DocumentFormat::Odp
            ))
        );

        let hwpx_by_structure = vec![
            ZipArchiveEntry::new(
                "META-INF/manifest.xml",
                b"<manifest><manifest:encryption-data/></manifest>".to_vec(),
            ),
            ZipArchiveEntry::new(
                "Contents/header.xml",
                b"<hh:head isTemplate=\"1\"/>".to_vec(),
            ),
        ];
        assert_eq!(
            check_zip_manifest_encryption(&hwpx_by_structure, None),
            Err(EasyOfficeError::EncryptedDocumentUnsupported(
                DocumentFormat::Hwtx
            ))
        );
    }
}

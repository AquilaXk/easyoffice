use easyoffice_formats::{
    ContainerFamily, DocumentFormat, EasyOfficeError, FormatDetector, Hwp5Codec,
    Ole2ContainerCodec, PdfSerializer, RtfCodec, StrictJsonValue, UniversalFormatConverter,
    XmlSanitizer, ZipArchiveCodec, ZipEntryDescriptor, ZipSecurityGuard, ZipSecurityPolicy,
};

use crate::{DocumentSecurityAuditor, EasyOfficeIpcRouter, PiiAuditResult};
use std::fmt::Write as _;

const MAX_SCRUB_BATCH_ITEMS: usize = 4_096;
const MAX_SCRUB_BATCH_BYTES: usize = 4 * 1_048_576;

fn parse_metadata_pairs(entries: &[(String, StrictJsonValue)]) -> Vec<(String, String)> {
    let mut meta_pairs = Vec::new();
    if let Some(StrictJsonValue::Object(meta_entries)) =
        StrictJsonValue::get_field(entries, "metadata")
    {
        for (k, v) in meta_entries {
            if let Some(s) = v.to_scalar_string() {
                meta_pairs.push((k.clone(), s));
            }
        }
    }
    meta_pairs
}

fn push_scrub_audit_json(out: &mut String, audit: &PiiAuditResult) {
    out.push_str("{\"scrubbedText\":\"");
    out.push_str(&EasyOfficeIpcRouter::escape_json_str(&audit.scrubbed_text));
    let _ = write!(
        out,
        "\",\"maskedResidentIds\":{},\"maskedPhoneNumbers\":{},\"maskedEmails\":{},\"maskedIdentifierNumbers\":{},\"totalMasked\":{},\"spans\":[",
        audit.masked_resident_ids,
        audit.masked_phone_numbers,
        audit.masked_emails,
        audit.masked_identifier_numbers,
        audit.total_masked_count(),
    );
    for (idx, sp) in audit.spans.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"start\":{},\"end\":{},\"replacement\":\"{}\"}}",
            sp.start_utf16,
            sp.end_utf16,
            EasyOfficeIpcRouter::escape_json_str(&sp.replacement)
        );
    }
    out.push_str("],\"strippedMetadataFields\":[");
    for (idx, k) in audit.stripped_metadata_fields.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(&EasyOfficeIpcRouter::escape_json_str(k));
        out.push('"');
    }
    out.push_str("]}");
}

impl EasyOfficeIpcRouter {
    pub(crate) fn handle_detect_format(payload: &str) -> Result<String, EasyOfficeError> {
        let trimmed = payload.strip_prefix('\u{FEFF}').unwrap_or(payload).trim();
        let (hint_owned, bytes) = if trimmed.starts_with('{') {
            let entries = Self::parse_rpc_json_object(trimmed)?;
            let hint_str = match Self::optional_json_str(&entries, "fileName")? {
                Some(s) => Some(s),
                None => Self::optional_json_str(&entries, "hint")?,
            }
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string);
            let raw_bytes = if let Some(b64) = Self::optional_json_str(&entries, "containerBase64")?
            {
                Self::decode_base64(b64.trim()).ok_or_else(|| {
                    EasyOfficeError::InvalidArgument(
                        "유효하지 않은 Base64 데이터입니다.".to_string(),
                    )
                })?
            } else if let Some(hex_raw) = match Self::optional_json_str(&entries, "hexPrefix")? {
                Some(h) => Some(h),
                None => Self::optional_json_str(&entries, "hex")?,
            } {
                let clean_hex = hex_raw
                    .trim()
                    .strip_prefix("HEX:")
                    .unwrap_or(hex_raw.trim());
                Self::decode_hex(clean_hex)?
            } else if let Some(txt) = match Self::optional_json_str(&entries, "text")? {
                Some(t) => Some(t),
                None => Self::optional_json_str(&entries, "data")?,
            } {
                txt.as_bytes().to_vec()
            } else {
                Vec::new()
            };
            (hint_str, raw_bytes)
        } else {
            let (hint_part, data_part) = payload.split_once('|').unwrap_or(("", payload));
            let raw_bytes = if let Some(hex_str) = data_part.strip_prefix("HEX:") {
                Self::decode_hex(hex_str)?
            } else {
                data_part.as_bytes().to_vec()
            };
            let hint_str = if hint_part.trim().is_empty() {
                None
            } else {
                Some(hint_part.trim().to_string())
            };
            (hint_str, raw_bytes)
        };

        let fmt = FormatDetector::detect(&bytes, hint_owned.as_deref())?;
        Ok(format!(
            "{{\"format\":\"{}\",\"containerFamily\":\"{}\",\"specification\":\"{}\",\"byteCount\":{}}}",
            Self::escape_json_str(fmt.extension()),
            Self::escape_json_str(fmt.container_family().as_str()),
            Self::escape_json_str(fmt.standard_specification()),
            bytes.len()
        ))
    }

    fn empty_audit_error() -> EasyOfficeError {
        EasyOfficeError::InvalidArgument("보안 감사 대상 엔트리가 비어 있습니다.".to_string())
    }

    fn invalid_audit_arg(message: &str) -> EasyOfficeError {
        EasyOfficeError::InvalidArgument(message.to_string())
    }

    pub(crate) fn handle_audit_security(payload: &str) -> Result<String, EasyOfficeError> {
        let policy = ZipSecurityPolicy::default();
        let trimmed = payload.strip_prefix('\u{FEFF}').unwrap_or(payload).trim();
        if trimmed.is_empty() {
            return Err(Self::empty_audit_error());
        }
        if trimmed.starts_with('{') {
            return Self::audit_json_payload(trimmed, &policy);
        }
        let mut parsed_entries: Vec<ZipEntryDescriptor> = Vec::new();
        for raw_seg in trimmed.split([',', ';']) {
            let seg = raw_seg.trim();
            if !seg.is_empty() {
                parsed_entries.push(Self::parse_text_audit_entry(seg)?);
            }
        }
        Self::verdict_for_entries(&parsed_entries, &policy)
    }

    /// Validates the parsed descriptors and builds the SAFE verdict JSON.
    fn verdict_for_entries(
        parsed_entries: &[ZipEntryDescriptor],
        policy: &ZipSecurityPolicy,
    ) -> Result<String, EasyOfficeError> {
        if parsed_entries.is_empty() {
            return Err(Self::empty_audit_error());
        }
        ZipSecurityGuard::validate_archive_entries(parsed_entries, policy)?;
        Ok(format!(
            "{{\"status\":\"SAFE\",\"verdict\":\"SAFE\",\"xxeGuard\":\"PASSED\",\"zipGuard\":\"PASSED\",\"checkedEntries\":{}}}",
            parsed_entries.len()
        ))
    }

    /// Parses one `name:compressed:uncompressed[:encrypted]` segment.
    fn parse_text_audit_entry(seg: &str) -> Result<ZipEntryDescriptor, EasyOfficeError> {
        let parts: Vec<&str> = seg.split(':').collect();
        if parts.len() != 3 && parts.len() != 4 {
            return Err(EasyOfficeError::InvalidArgument(format!(
                "유효하지 않은 보안 감사 엔트리 형식입니다: {seg}"
            )));
        }
        let name = parts[0].trim();
        if name.is_empty() {
            return Err(Self::invalid_audit_arg("ZIP 엔트리 이름이 비어 있습니다."));
        }
        let comp = parts[1]
            .trim()
            .parse::<u64>()
            .map_err(|_| Self::invalid_audit_arg("유효하지 않은 compressed_size"))?;
        let uncomp = parts[2]
            .trim()
            .parse::<u64>()
            .map_err(|_| Self::invalid_audit_arg("유효하지 않은 uncompressed_size"))?;
        let enc = match parts.get(3).map(|p| p.trim()) {
            None | Some("false" | "0") => false,
            Some("true" | "1") => true,
            Some(_) => return Err(Self::invalid_audit_arg("유효하지 않은 encrypted 불리언 값")),
        };
        Ok(ZipEntryDescriptor::new(name, comp, uncomp, enc))
    }

    fn audit_json_payload(
        trimmed: &str,
        policy: &ZipSecurityPolicy,
    ) -> Result<String, EasyOfficeError> {
        let entries = Self::parse_rpc_json_object(trimmed)?;
        if let Some(xml_str) = Self::optional_json_str(&entries, "xml")? {
            if !xml_str.trim().is_empty() {
                XmlSanitizer::validate_safe_xml(xml_str)?;
            }
        }
        if StrictJsonValue::get_field(&entries, "containerBase64").is_some() {
            let b64_raw = Self::optional_json_str(&entries, "containerBase64")?
                .unwrap_or("")
                .trim();
            if b64_raw.is_empty() {
                return Err(Self::empty_audit_error());
            }
            let bytes = Self::decode_base64(b64_raw).ok_or_else(|| {
                Self::invalid_audit_arg("유효하지 않은 Base64 컨테이너 데이터입니다.")
            })?;
            let file_name = Self::optional_json_str(&entries, "fileName")?
                .map(str::trim)
                .filter(|s| !s.is_empty());
            return Self::audit_real_container_bytes(&bytes, file_name, policy);
        }

        let mut parsed_entries: Vec<ZipEntryDescriptor> = Vec::new();
        if let Some(entries_val) = StrictJsonValue::get_field(&entries, "entries") {
            let items = entries_val.as_array().ok_or_else(|| {
                Self::invalid_audit_arg("JSON 'entries' 필드는 배열이어야 합니다.")
            })?;
            for item in items {
                let obj = item.as_object().ok_or_else(|| {
                    Self::invalid_audit_arg("JSON 'entries' 항목은 객체여야 합니다.")
                })?;
                parsed_entries.push(Self::parse_json_audit_entry(obj)?);
            }
        }
        Self::verdict_for_entries(&parsed_entries, policy)
    }

    /// Reads a size field by primary or alternate key.
    fn json_size_field(
        obj: &[(String, StrictJsonValue)],
        keys: [&str; 2],
        error: &str,
    ) -> Result<u64, EasyOfficeError> {
        StrictJsonValue::get_field(obj, keys[0])
            .or_else(|| StrictJsonValue::get_field(obj, keys[1]))
            .and_then(StrictJsonValue::as_u64)
            .ok_or_else(|| Self::invalid_audit_arg(error))
    }

    fn parse_json_audit_entry(
        obj: &[(String, StrictJsonValue)],
    ) -> Result<ZipEntryDescriptor, EasyOfficeError> {
        let name = match Self::optional_json_str(obj, "name")? {
            Some(n) => n,
            None => Self::optional_json_str(obj, "path")?.unwrap_or(""),
        }
        .trim();
        if name.is_empty() {
            return Err(Self::invalid_audit_arg(
                "ZIP 엔트리 이름('name')이 비어 있습니다.",
            ));
        }
        let compressed = Self::json_size_field(
            obj,
            ["compressed", "compressedSize"],
            "유효하지 않은 compressed_size",
        )?;
        let uncompressed = Self::json_size_field(
            obj,
            ["uncompressed", "uncompressedSize"],
            "유효하지 않은 uncompressed_size",
        )?;
        let encrypted = match StrictJsonValue::get_field(obj, "encrypted") {
            None | Some(StrictJsonValue::Null) => false,
            Some(StrictJsonValue::Bool(b)) => *b,
            Some(_) => return Err(Self::invalid_audit_arg("유효하지 않은 encrypted 불리언 값")),
        };
        Ok(ZipEntryDescriptor::new(
            name,
            compressed,
            uncompressed,
            encrypted,
        ))
    }

    fn audit_real_container_bytes(
        bytes: &[u8],
        file_name: Option<&str>,
        policy: &ZipSecurityPolicy,
    ) -> Result<String, EasyOfficeError> {
        if bytes.is_empty() {
            return Err(Self::empty_audit_error());
        }
        let fmt = FormatDetector::detect(bytes, file_name)?;
        let checked_entries = match fmt.container_family() {
            ContainerFamily::ZipOoxml
            | ContainerFamily::ZipOdf
            | ContainerFamily::ZipHwpx
            | ContainerFamily::ZipIwork => Self::audit_zip_container(bytes, policy)?,
            ContainerFamily::Ole2Cfbf => Self::audit_ole2_container(bytes, fmt)?,
            ContainerFamily::PdfIso32000 => Self::audit_pdf_container(bytes)?,
            ContainerFamily::PlainTextUtf8 => Self::audit_text_container(bytes, fmt)?,
        };

        Ok(format!(
            "{{\"status\":\"SAFE\",\"verdict\":\"SAFE\",\"format\":\"{}\",\"xxeGuard\":\"PASSED\",\"zipGuard\":\"PASSED\",\"checkedEntries\":{}}}",
            Self::escape_json_str(fmt.extension()),
            checked_entries
        ))
    }

    fn audit_zip_container(
        bytes: &[u8],
        policy: &ZipSecurityPolicy,
    ) -> Result<usize, EasyOfficeError> {
        let unpacked = ZipArchiveCodec::unpack(bytes, policy)?;
        if unpacked.is_empty() {
            return Err(Self::empty_audit_error());
        }
        for entry in &unpacked {
            let lower = entry.path.to_ascii_lowercase();
            if lower.ends_with(".xml") || lower.ends_with(".rels") || lower.ends_with(".hpf") {
                let xml_text = std::str::from_utf8(&entry.data).map_err(|_| {
                    EasyOfficeError::CorruptedContainer(format!(
                        "유효하지 않은 UTF-8 XML 엔트리: {}",
                        entry.path
                    ))
                })?;
                XmlSanitizer::validate_safe_xml(xml_text)?;
            }
        }
        Ok(unpacked.len())
    }

    fn audit_ole2_container(bytes: &[u8], fmt: DocumentFormat) -> Result<usize, EasyOfficeError> {
        let streams = Ole2ContainerCodec::unpack(bytes)?;
        if streams.is_empty() {
            return Err(Self::empty_audit_error());
        }
        let encrypted = streams.iter().any(|st| {
            st.name.eq_ignore_ascii_case("EncryptionInfo")
                || st.name.eq_ignore_ascii_case("EncryptedPackage")
                || st.name.eq_ignore_ascii_case("\u{0006}DataSpaces")
        });
        if encrypted {
            return Err(EasyOfficeError::SecurityViolation(
                "암호화된 OLE2 컨테이너는 보안 정책상 거부됩니다.".to_string(),
            ));
        }
        if matches!(fmt, DocumentFormat::Hwp | DocumentFormat::Hwt) {
            let _ = Hwp5Codec::unpack(bytes)?;
        } else {
            let _ = UniversalFormatConverter::decode(bytes, Some(fmt.extension()))?;
        }
        Ok(streams.len())
    }

    fn audit_pdf_container(bytes: &[u8]) -> Result<usize, EasyOfficeError> {
        let raw_lossy = String::from_utf8_lossy(bytes);
        let has_encrypt_key = [
            "/Encrypt ",
            "/Encrypt/",
            "/Encrypt<<",
            "/Encrypt\n",
            "/Encrypt\r",
        ]
        .iter()
        .any(|needle| raw_lossy.contains(needle));
        if has_encrypt_key {
            return Err(EasyOfficeError::SecurityViolation(
                "암호화된 PDF 문서는 보안 정책상 거부됩니다.".to_string(),
            ));
        }
        let pdf = PdfSerializer::parse(bytes)?;
        if pdf.pages.is_empty() {
            return Err(Self::empty_audit_error());
        }
        Ok(pdf.pages.len())
    }

    fn audit_text_container(bytes: &[u8], fmt: DocumentFormat) -> Result<usize, EasyOfficeError> {
        if bytes.contains(&0u8) {
            return Err(EasyOfficeError::SecurityViolation(
                "텍스트 컨테이너에 허용되지 않는 NUL 바이트가 포함되어 있습니다.".to_string(),
            ));
        }
        std::str::from_utf8(bytes).map_err(|_| {
            EasyOfficeError::CorruptedContainer(
                "유효하지 않은 UTF-8 텍스트 컨테이너입니다.".to_string(),
            )
        })?;
        if fmt == DocumentFormat::Rtf {
            let _ = RtfCodec::decode(bytes)?;
        }
        Ok(1)
    }

    pub(crate) fn handle_scrub_pii(payload: &str) -> Result<String, EasyOfficeError> {
        let trimmed = payload
            .strip_prefix('\u{FEFF}')
            .unwrap_or(payload)
            .trim_start();
        if trimmed.starts_with('{') {
            if let Ok(entries) = Self::parse_rpc_json_object(trimmed) {
                if let Some(texts_val) = StrictJsonValue::get_field(&entries, "texts") {
                    let items = texts_val.as_array().ok_or_else(|| {
                        EasyOfficeError::InvalidArgument(
                            "JSON 'texts' 필드는 문자열 배열이어야 합니다.".to_string(),
                        )
                    })?;
                    if items.len() > MAX_SCRUB_BATCH_ITEMS {
                        return Err(EasyOfficeError::InvalidArgument(format!(
                            "개인정보 마스킹 배치는 최대 {MAX_SCRUB_BATCH_ITEMS}개까지 지원합니다."
                        )));
                    }
                    let mut total_bytes = 0usize;
                    let mut inputs = Vec::with_capacity(items.len());
                    for item in items {
                        let text = item.as_str().ok_or_else(|| {
                            EasyOfficeError::InvalidArgument(
                                "JSON 'texts' 항목은 문자열이어야 합니다.".to_string(),
                            )
                        })?;
                        total_bytes = total_bytes.saturating_add(text.len());
                        if total_bytes > MAX_SCRUB_BATCH_BYTES {
                            return Err(EasyOfficeError::PayloadTooLarge {
                                actual: total_bytes,
                                max: MAX_SCRUB_BATCH_BYTES,
                            });
                        }
                        inputs.push(text);
                    }
                    let meta_pairs = parse_metadata_pairs(&entries);
                    let mut out = String::with_capacity(items.len().saturating_mul(160) + 48);
                    out.push_str("{\"results\":[");
                    let mut total_masked = 0usize;
                    for (idx, text) in inputs.iter().enumerate() {
                        if idx > 0 {
                            out.push(',');
                        }
                        let audit = DocumentSecurityAuditor::scan_and_scrub_pii_with_metadata(
                            text,
                            &meta_pairs,
                        );
                        total_masked = total_masked.saturating_add(audit.total_masked_count());
                        push_scrub_audit_json(&mut out, &audit);
                    }
                    let _ = write!(
                        out,
                        "],\"totalMasked\":{},\"count\":{}}}",
                        total_masked,
                        inputs.len()
                    );
                    return Ok(out);
                }
                if let Some(StrictJsonValue::Str(text)) =
                    StrictJsonValue::get_field(&entries, "text")
                {
                    let meta_pairs = parse_metadata_pairs(&entries);
                    let audit = DocumentSecurityAuditor::scan_and_scrub_pii_with_metadata(
                        text,
                        &meta_pairs,
                    );
                    let mut out = String::with_capacity(audit.scrubbed_text.len() + 192);
                    push_scrub_audit_json(&mut out, &audit);
                    return Ok(out);
                }
            }
        }
        let audit = DocumentSecurityAuditor::scan_and_scrub_pii(payload);
        let mut out = String::with_capacity(audit.scrubbed_text.len() + 192);
        push_scrub_audit_json(&mut out, &audit);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub_pii_batch_masks_items_within_bounds_and_rejects_bad_batches() {
        let out = EasyOfficeIpcRouter::handle_scrub_pii(
            r#"{"texts":["연락처 010-1234-5678","홍길동","a@b.kr"],"metadata":{"Author":"홍길동"}}"#,
        )
        .unwrap();
        let parsed = StrictJsonValue::parse(&out).unwrap();
        let entries = parsed.as_object().unwrap();
        let results = StrictJsonValue::get_field(entries, "results")
            .and_then(StrictJsonValue::as_array)
            .expect("results array");
        assert_eq!(results.len(), 3);
        let first = results[0].as_object().unwrap();
        assert_eq!(
            StrictJsonValue::get_str(first, "scrubbedText"),
            Some("연락처 010-****-****")
        );
        let third = results[2].as_object().unwrap();
        assert_eq!(
            StrictJsonValue::get_str(third, "scrubbedText"),
            Some("***@***")
        );
        assert_eq!(
            StrictJsonValue::get_field(entries, "totalMasked").and_then(StrictJsonValue::as_u64),
            Some(2)
        );

        let empty = EasyOfficeIpcRouter::handle_scrub_pii(r#"{"texts":[]}"#).unwrap();
        let empty_obj = StrictJsonValue::parse(&empty).unwrap();
        let empty_results = StrictJsonValue::get_field(empty_obj.as_object().unwrap(), "results")
            .and_then(StrictJsonValue::as_array)
            .unwrap();
        assert!(empty_results.is_empty());

        // Single text & plain string backward compatibility.
        let single = EasyOfficeIpcRouter::handle_scrub_pii(r#"{"text":"010-1234-5678"}"#).unwrap();
        assert!(single.contains("\"scrubbedText\":\"010-****-****\""));
        let plain = EasyOfficeIpcRouter::handle_scrub_pii("a@b.kr").unwrap();
        assert!(plain.contains("\"scrubbedText\":\"***@***\""));

        // Fail closed on non-array, non-string element, or item count above limit (4096).
        assert!(EasyOfficeIpcRouter::handle_scrub_pii(r#"{"texts":"bad"}"#).is_err());
        assert!(EasyOfficeIpcRouter::handle_scrub_pii(r#"{"texts":["ok",123]}"#).is_err());
        let too_many = format!("{{\"texts\":[{}]}}", vec!["\"x\""; 4_097].join(","));
        assert!(EasyOfficeIpcRouter::handle_scrub_pii(&too_many).is_err());
        let huge_item = "a".repeat(2 * 1_048_576 + 1);
        let too_many_bytes = format!("{{\"texts\":[\"{huge_item}\",\"{huge_item}\"]}}");
        assert!(EasyOfficeIpcRouter::handle_scrub_pii(&too_many_bytes).is_err());
    }
}

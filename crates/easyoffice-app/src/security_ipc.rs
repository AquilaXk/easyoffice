use easyoffice_formats::{
    ContainerFamily, DocumentFormat, EasyOfficeError, FormatDetector, Hwp5Codec,
    Ole2ContainerCodec, PdfSerializer, RtfCodec, StrictJsonValue, UniversalFormatConverter,
    XmlSanitizer, ZipArchiveCodec, ZipEntryDescriptor, ZipSecurityGuard, ZipSecurityPolicy,
};

use crate::{DocumentSecurityAuditor, EasyOfficeIpcRouter};

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

    pub(crate) fn handle_audit_security(payload: &str) -> Result<String, EasyOfficeError> {
        let policy = ZipSecurityPolicy::default();
        let trimmed = payload.strip_prefix('\u{FEFF}').unwrap_or(payload).trim();
        if trimmed.is_empty() {
            return Err(EasyOfficeError::InvalidArgument(
                "보안 감사 대상 엔트리가 비어 있습니다.".to_string(),
            ));
        }

        if trimmed.starts_with('{') {
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
                    return Err(EasyOfficeError::InvalidArgument(
                        "보안 감사 대상 엔트리가 비어 있습니다.".to_string(),
                    ));
                }
                let bytes = Self::decode_base64(b64_raw).ok_or_else(|| {
                    EasyOfficeError::InvalidArgument(
                        "유효하지 않은 Base64 컨테이너 데이터입니다.".to_string(),
                    )
                })?;
                let file_name = Self::optional_json_str(&entries, "fileName")?
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                return Self::audit_real_container_bytes(&bytes, file_name, &policy);
            }

            let mut parsed_entries: Vec<ZipEntryDescriptor> = Vec::new();
            if let Some(entries_val) = StrictJsonValue::get_field(&entries, "entries") {
                let items = entries_val.as_array().ok_or_else(|| {
                    EasyOfficeError::InvalidArgument(
                        "JSON 'entries' 필드는 배열이어야 합니다.".to_string(),
                    )
                })?;
                for item in items {
                    let obj = item.as_object().ok_or_else(|| {
                        EasyOfficeError::InvalidArgument(
                            "JSON 'entries' 항목은 객체여야 합니다.".to_string(),
                        )
                    })?;
                    let name = match Self::optional_json_str(obj, "name")? {
                        Some(n) => n,
                        None => Self::optional_json_str(obj, "path")?.unwrap_or(""),
                    }
                    .trim();
                    if name.is_empty() {
                        return Err(EasyOfficeError::InvalidArgument(
                            "ZIP 엔트리 이름('name')이 비어 있습니다.".to_string(),
                        ));
                    }
                    let compressed = StrictJsonValue::get_field(obj, "compressed")
                        .or_else(|| StrictJsonValue::get_field(obj, "compressedSize"))
                        .and_then(StrictJsonValue::as_u64)
                        .ok_or_else(|| {
                            EasyOfficeError::InvalidArgument(
                                "유효하지 않은 compressed_size".to_string(),
                            )
                        })?;
                    let uncompressed = StrictJsonValue::get_field(obj, "uncompressed")
                        .or_else(|| StrictJsonValue::get_field(obj, "uncompressedSize"))
                        .and_then(StrictJsonValue::as_u64)
                        .ok_or_else(|| {
                            EasyOfficeError::InvalidArgument(
                                "유효하지 않은 uncompressed_size".to_string(),
                            )
                        })?;
                    let encrypted = match StrictJsonValue::get_field(obj, "encrypted") {
                        None | Some(StrictJsonValue::Null) => false,
                        Some(StrictJsonValue::Bool(b)) => *b,
                        Some(_) => {
                            return Err(EasyOfficeError::InvalidArgument(
                                "유효하지 않은 encrypted 불리언 값".to_string(),
                            ))
                        }
                    };
                    parsed_entries.push(ZipEntryDescriptor::new(
                        name,
                        compressed,
                        uncompressed,
                        encrypted,
                    ));
                }
            }
            if parsed_entries.is_empty() {
                return Err(EasyOfficeError::InvalidArgument(
                    "보안 감사 대상 엔트리가 비어 있습니다.".to_string(),
                ));
            }
            ZipSecurityGuard::validate_archive_entries(&parsed_entries, &policy)?;
            return Ok(format!(
                "{{\"status\":\"SAFE\",\"verdict\":\"SAFE\",\"xxeGuard\":\"PASSED\",\"zipGuard\":\"PASSED\",\"checkedEntries\":{}}}",
                parsed_entries.len()
            ));
        }

        let mut parsed_entries: Vec<ZipEntryDescriptor> = Vec::new();
        for raw_seg in trimmed.split([',', ';']) {
            let seg = raw_seg.trim();
            if seg.is_empty() {
                continue;
            }
            let parts: Vec<&str> = seg.split(':').collect();
            if parts.len() != 3 && parts.len() != 4 {
                return Err(EasyOfficeError::InvalidArgument(format!(
                    "유효하지 않은 보안 감사 엔트리 형식입니다: {seg}"
                )));
            }
            let name = parts[0].trim();
            if name.is_empty() {
                return Err(EasyOfficeError::InvalidArgument(
                    "ZIP 엔트리 이름이 비어 있습니다.".to_string(),
                ));
            }
            let comp = parts[1].trim().parse::<u64>().map_err(|_| {
                EasyOfficeError::InvalidArgument("유효하지 않은 compressed_size".to_string())
            })?;
            let uncomp = parts[2].trim().parse::<u64>().map_err(|_| {
                EasyOfficeError::InvalidArgument("유효하지 않은 uncompressed_size".to_string())
            })?;
            let enc = if parts.len() == 4 {
                match parts[3].trim() {
                    "true" | "1" => true,
                    "false" | "0" => false,
                    _ => {
                        return Err(EasyOfficeError::InvalidArgument(
                            "유효하지 않은 encrypted 불리언 값".to_string(),
                        ))
                    }
                }
            } else {
                false
            };
            parsed_entries.push(ZipEntryDescriptor::new(name, comp, uncomp, enc));
        }
        if parsed_entries.is_empty() {
            return Err(EasyOfficeError::InvalidArgument(
                "보안 감사 대상 엔트리가 비어 있습니다.".to_string(),
            ));
        }
        ZipSecurityGuard::validate_archive_entries(&parsed_entries, &policy)?;
        Ok(format!(
            "{{\"status\":\"SAFE\",\"verdict\":\"SAFE\",\"xxeGuard\":\"PASSED\",\"zipGuard\":\"PASSED\",\"checkedEntries\":{}}}",
            parsed_entries.len()
        ))
    }

    fn audit_real_container_bytes(
        bytes: &[u8],
        file_name: Option<&str>,
        policy: &ZipSecurityPolicy,
    ) -> Result<String, EasyOfficeError> {
        if bytes.is_empty() {
            return Err(EasyOfficeError::InvalidArgument(
                "보안 감사 대상 엔트리가 비어 있습니다.".to_string(),
            ));
        }
        let fmt = FormatDetector::detect(bytes, file_name)?;
        let checked_entries = match fmt.container_family() {
            ContainerFamily::ZipOoxml
            | ContainerFamily::ZipOdf
            | ContainerFamily::ZipHwpx
            | ContainerFamily::ZipIwork => {
                let unpacked = ZipArchiveCodec::unpack(bytes, policy)?;
                if unpacked.is_empty() {
                    return Err(EasyOfficeError::InvalidArgument(
                        "보안 감사 대상 엔트리가 비어 있습니다.".to_string(),
                    ));
                }
                for entry in &unpacked {
                    let lower = entry.path.to_ascii_lowercase();
                    if lower.ends_with(".xml")
                        || lower.ends_with(".rels")
                        || lower.ends_with(".hpf")
                    {
                        let xml_text = std::str::from_utf8(&entry.data).map_err(|_| {
                            EasyOfficeError::CorruptedContainer(format!(
                                "유효하지 않은 UTF-8 XML 엔트리: {}",
                                entry.path
                            ))
                        })?;
                        XmlSanitizer::validate_safe_xml(xml_text)?;
                    }
                }
                unpacked.len()
            }
            ContainerFamily::Ole2Cfbf => {
                let streams = Ole2ContainerCodec::unpack(bytes)?;
                if streams.is_empty() {
                    return Err(EasyOfficeError::InvalidArgument(
                        "보안 감사 대상 엔트리가 비어 있습니다.".to_string(),
                    ));
                }
                for st in &streams {
                    if st.name.eq_ignore_ascii_case("EncryptionInfo")
                        || st.name.eq_ignore_ascii_case("EncryptedPackage")
                        || st.name.eq_ignore_ascii_case("\u{0006}DataSpaces")
                    {
                        return Err(EasyOfficeError::SecurityViolation(
                            "암호화된 OLE2 컨테이너는 보안 정책상 거부됩니다.".to_string(),
                        ));
                    }
                }
                if matches!(fmt, DocumentFormat::Hwp | DocumentFormat::Hwt) {
                    let _ = Hwp5Codec::unpack(bytes)?;
                } else {
                    let _ = UniversalFormatConverter::decode(bytes, Some(fmt.extension()))?;
                }
                streams.len()
            }
            ContainerFamily::PdfIso32000 => {
                let raw_lossy = String::from_utf8_lossy(bytes);
                if raw_lossy.contains("/Encrypt ")
                    || raw_lossy.contains("/Encrypt/")
                    || raw_lossy.contains("/Encrypt<<")
                    || raw_lossy.contains("/Encrypt\n")
                    || raw_lossy.contains("/Encrypt\r")
                {
                    return Err(EasyOfficeError::SecurityViolation(
                        "암호화된 PDF 문서는 보안 정책상 거부됩니다.".to_string(),
                    ));
                }
                let pdf = PdfSerializer::parse(bytes)?;
                if pdf.pages.is_empty() {
                    return Err(EasyOfficeError::InvalidArgument(
                        "보안 감사 대상 엔트리가 비어 있습니다.".to_string(),
                    ));
                }
                for page in &pdf.pages {
                    XmlSanitizer::validate_safe_xml(&page.body_text)?;
                }
                if let Some(ref wm) = pdf.watermark_text {
                    XmlSanitizer::validate_safe_xml(wm)?;
                }
                pdf.pages.len()
            }
            ContainerFamily::PlainTextUtf8 => {
                if bytes.contains(&0u8) {
                    return Err(EasyOfficeError::SecurityViolation(
                        "텍스트 컨테이너에 허용되지 않는 NUL 바이트가 포함되어 있습니다."
                            .to_string(),
                    ));
                }
                let text = std::str::from_utf8(bytes).map_err(|_| {
                    EasyOfficeError::CorruptedContainer(
                        "유효하지 않은 UTF-8 텍스트 컨테이너입니다.".to_string(),
                    )
                })?;
                XmlSanitizer::validate_safe_xml(text)?;
                if fmt == DocumentFormat::Rtf {
                    let _ = RtfCodec::decode(bytes)?;
                }
                1
            }
        };

        Ok(format!(
            "{{\"status\":\"SAFE\",\"verdict\":\"SAFE\",\"format\":\"{}\",\"xxeGuard\":\"PASSED\",\"zipGuard\":\"PASSED\",\"checkedEntries\":{}}}",
            Self::escape_json_str(fmt.extension()),
            checked_entries
        ))
    }

    pub(crate) fn handle_scrub_pii(payload: &str) -> Result<String, EasyOfficeError> {
        let trimmed = payload
            .strip_prefix('\u{FEFF}')
            .unwrap_or(payload)
            .trim_start();
        let audit = if trimmed.starts_with('{') {
            if let Ok(entries) = Self::parse_rpc_json_object(trimmed) {
                if let Some(StrictJsonValue::Str(text)) =
                    StrictJsonValue::get_field(&entries, "text")
                {
                    let mut meta_pairs: Vec<(String, String)> = Vec::new();
                    if let Some(StrictJsonValue::Object(meta_entries)) =
                        StrictJsonValue::get_field(&entries, "metadata")
                    {
                        for (k, v) in meta_entries {
                            if let Some(s) = v.to_scalar_string() {
                                meta_pairs.push((k.clone(), s));
                            }
                        }
                    }
                    DocumentSecurityAuditor::scan_and_scrub_pii_with_metadata(text, &meta_pairs)
                } else {
                    DocumentSecurityAuditor::scan_and_scrub_pii(payload)
                }
            } else {
                DocumentSecurityAuditor::scan_and_scrub_pii(payload)
            }
        } else {
            DocumentSecurityAuditor::scan_and_scrub_pii(payload)
        };

        let escaped = Self::escape_json_str(&audit.scrubbed_text);
        let spans_json = audit
            .spans
            .iter()
            .map(|sp| {
                format!(
                    "{{\"start\":{},\"end\":{},\"replacement\":\"{}\"}}",
                    sp.start_utf16,
                    sp.end_utf16,
                    Self::escape_json_str(&sp.replacement)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let stripped_json = audit
            .stripped_metadata_fields
            .iter()
            .map(|k| format!("\"{}\"", Self::escape_json_str(k)))
            .collect::<Vec<_>>()
            .join(",");
        Ok(format!(
            "{{\"scrubbedText\":\"{}\",\"maskedResidentIds\":{},\"maskedPhoneNumbers\":{},\"maskedEmails\":{},\"maskedIdentifierNumbers\":{},\"totalMasked\":{},\"spans\":[{}],\"strippedMetadataFields\":[{}]}}",
            escaped,
            audit.masked_resident_ids,
            audit.masked_phone_numbers,
            audit.masked_emails,
            audit.masked_identifier_numbers,
            audit.total_masked_count(),
            spans_json,
            stripped_json
        ))
    }
}

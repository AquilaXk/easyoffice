use std::collections::HashSet;
use std::fmt;

/// Container technology family for supported document formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContainerFamily {
    ZipOoxml,
    ZipOdf,
    ZipHwpx,
    Ole2Cfbf,
    PdfIso32000,
    PlainTextUtf8,
}

impl ContainerFamily {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ZipOoxml => "ZIP-OOXML (ECMA-376)",
            Self::ZipOdf => "ZIP-ODF (ISO/IEC 26300)",
            Self::ZipHwpx => "ZIP-HWPX (KS X 6101 OWPML)",
            Self::Ole2Cfbf => "OLE2-CFBF",
            Self::PdfIso32000 => "PDF (ISO 32000)",
            Self::PlainTextUtf8 => "UTF-8 Plain Text",
        }
    }
}

/// 20 standard document formats supported by EasyOffice across macOS and Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DocumentFormat {
    Docx,
    Doc,
    Odt,
    Rtf,
    Markdown,
    Txt,
    Hwp,
    Hwpx,
    Hwt,
    Hwtx,
    Xlsx,
    Xls,
    Ods,
    Csv,
    Tsv,
    Pptx,
    Ppt,
    Ppsx,
    Odp,
    Pdf,
}

impl DocumentFormat {
    pub const ALL: [DocumentFormat; 20] = [
        Self::Docx,
        Self::Doc,
        Self::Odt,
        Self::Rtf,
        Self::Markdown,
        Self::Txt,
        Self::Hwp,
        Self::Hwpx,
        Self::Hwt,
        Self::Hwtx,
        Self::Xlsx,
        Self::Xls,
        Self::Ods,
        Self::Csv,
        Self::Tsv,
        Self::Pptx,
        Self::Ppt,
        Self::Ppsx,
        Self::Odp,
        Self::Pdf,
    ];

    pub fn extension(&self) -> &'static str {
        match self {
            Self::Docx => "docx",
            Self::Doc => "doc",
            Self::Odt => "odt",
            Self::Rtf => "rtf",
            Self::Markdown => "md",
            Self::Txt => "txt",
            Self::Hwp => "hwp",
            Self::Hwpx => "hwpx",
            Self::Hwt => "hwt",
            Self::Hwtx => "hwtx",
            Self::Xlsx => "xlsx",
            Self::Xls => "xls",
            Self::Ods => "ods",
            Self::Csv => "csv",
            Self::Tsv => "tsv",
            Self::Pptx => "pptx",
            Self::Ppt => "ppt",
            Self::Ppsx => "ppsx",
            Self::Odp => "odp",
            Self::Pdf => "pdf",
        }
    }

    pub fn container_family(&self) -> ContainerFamily {
        match self {
            Self::Docx | Self::Xlsx | Self::Pptx | Self::Ppsx => ContainerFamily::ZipOoxml,
            Self::Odt | Self::Ods | Self::Odp => ContainerFamily::ZipOdf,
            Self::Hwpx | Self::Hwtx => ContainerFamily::ZipHwpx,
            Self::Doc | Self::Xls | Self::Ppt | Self::Hwp | Self::Hwt => ContainerFamily::Ole2Cfbf,
            Self::Pdf => ContainerFamily::PdfIso32000,
            Self::Csv | Self::Tsv | Self::Markdown | Self::Txt | Self::Rtf => {
                ContainerFamily::PlainTextUtf8
            }
        }
    }

    pub fn standard_specification(&self) -> &'static str {
        match self {
            Self::Docx => "OOXML (ECMA-376) WordprocessingML",
            Self::Doc => "OLE2 CFBF Word Binary",
            Self::Xlsx => "OOXML (ECMA-376) SpreadsheetML",
            Self::Xls => "OLE2 CFBF BIFF8 Spreadsheet",
            Self::Pptx | Self::Ppsx => "OOXML (ECMA-376) PresentationML",
            Self::Ppt => "OLE2 CFBF Presentation Binary",
            Self::Hwp | Self::Hwt => "HWP 5.0 Binary (OLE2 CFBF)",
            Self::Hwpx | Self::Hwtx => "HWPX (KS X 6101 OWPML)",
            Self::Pdf => "PDF (ISO 32000)",
            Self::Odt => "ODF (ISO/IEC 26300) Text",
            Self::Ods => "ODF (ISO/IEC 26300) Spreadsheet",
            Self::Odp => "ODF (ISO/IEC 26300) Presentation",
            Self::Csv => "RFC 4180 CSV",
            Self::Tsv => "RFC 4180 Tab-Separated Values",
            Self::Markdown => "CommonMark Markdown",
            Self::Txt => "UTF-8 Plain Text",
            Self::Rtf => "Rich Text Format 1.9.1",
        }
    }

    pub fn from_extension(input: &str) -> Option<Self> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return None;
        }
        let ext = trimmed
            .rsplit('.')
            .next()
            .unwrap_or(trimmed)
            .to_ascii_lowercase();
        match ext.as_str() {
            "docx" | "dotx" => Some(Self::Docx),
            "doc" => Some(Self::Doc),
            "xlsx" | "xltx" => Some(Self::Xlsx),
            "xls" => Some(Self::Xls),
            "pptx" | "potx" => Some(Self::Pptx),
            "ppsx" => Some(Self::Ppsx),
            "ppt" => Some(Self::Ppt),
            "hwp" => Some(Self::Hwp),
            "hwt" => Some(Self::Hwt),
            "hwpx" => Some(Self::Hwpx),
            "hwtx" => Some(Self::Hwtx),
            "pdf" => Some(Self::Pdf),
            "odt" => Some(Self::Odt),
            "ods" => Some(Self::Ods),
            "odp" => Some(Self::Odp),
            "csv" => Some(Self::Csv),
            "tsv" => Some(Self::Tsv),
            "md" | "markdown" => Some(Self::Markdown),
            "txt" => Some(Self::Txt),
            "rtf" => Some(Self::Rtf),
            _ => None,
        }
    }
}

/// Fail-Closed error type shared across format detection, security guards, and codecs.
#[derive(Debug, Clone, PartialEq)]
pub enum EasyOfficeError {
    EmptyPayload,
    PayloadTooLarge {
        actual: usize,
        max: usize,
    },
    UnsupportedExtension(String),
    SignatureMismatch {
        expected: ContainerFamily,
        detail: String,
    },
    InvalidUtf8(String),
    CorruptedContainer(String),
    SecurityViolation(String),
    DecompressionBombDetected {
        ratio: f64,
        max_ratio: f64,
    },
    EncryptedDocumentUnsupported(DocumentFormat),
    XxeOrExternalEntityDetected,
    FormulaError(String),
    InvalidArgument(String),
}

impl fmt::Display for EasyOfficeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPayload => write!(f, "파일 데이터가 비어 있어 열 수 없습니다 (Fail-Closed)."),
            Self::PayloadTooLarge { actual, max } => write!(
                f,
                "파일 크기({actual} bytes)가 최대 허용 한도({max} bytes)를 초과했습니다."
            ),
            Self::UnsupportedExtension(ext) => write!(f, "지원하지 않는 확장자입니다: {ext}"),
            Self::SignatureMismatch { expected, detail } => write!(
                f,
                "매직 바이트 시그니처가 기대된 컨테이너({})와 일치하지 않습니다: {detail}",
                expected.as_str()
            ),
            Self::InvalidUtf8(detail) => write!(f, "유효하지 않은 UTF-8 텍스트입니다: {detail}"),
            Self::CorruptedContainer(detail) => {
                write!(f, "컨테이너 무결성 검증 실패 (Fail-Closed): {detail}")
            }
            Self::SecurityViolation(detail) => {
                write!(f, "보안 정책 위반 감지 (Fail-Closed): {detail}")
            }
            Self::DecompressionBombDetected { ratio, max_ratio } => write!(
                f,
                "압축 해제 비율({ratio:.1}x)이 허용 상한({max_ratio:.1}x)을 초과하여 차단했습니다."
            ),
            Self::EncryptedDocumentUnsupported(fmt_kind) => write!(
                f,
                "암호화되거나 배포 제한된 문서({})는 복호화 키 없이 열 수 없습니다.",
                fmt_kind.extension().to_ascii_uppercase()
            ),
            Self::XxeOrExternalEntityDetected => write!(
                f,
                "XML 외부 엔티티(XXE) 또는 DTD 선언이 감지되어 파싱을 차단했습니다."
            ),
            Self::FormulaError(detail) => write!(f, "수식 오류: {detail}"),
            Self::InvalidArgument(detail) => write!(f, "잘못된 인자: {detail}"),
        }
    }
}

impl std::error::Error for EasyOfficeError {}

/// Magic-byte and extension Fail-Closed format detector.
pub struct FormatDetector;

impl FormatDetector {
    pub const MAX_PAYLOAD_BYTES: usize = 512 * 1024 * 1024;
    pub const ZIP_MAGIC: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];
    pub const OLE2_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    pub const PDF_MAGIC: [u8; 5] = *b"%PDF-";
    pub const RTF_MAGIC: [u8; 5] = *b"{\\rtf";

    pub fn detect(
        bytes: &[u8],
        extension_hint: Option<&str>,
    ) -> Result<DocumentFormat, EasyOfficeError> {
        if bytes.is_empty() {
            return Err(EasyOfficeError::EmptyPayload);
        }
        if bytes.len() > Self::MAX_PAYLOAD_BYTES {
            return Err(EasyOfficeError::PayloadTooLarge {
                actual: bytes.len(),
                max: Self::MAX_PAYLOAD_BYTES,
            });
        }

        if let Some(hint_raw) = extension_hint {
            let trimmed = hint_raw.trim();
            if !trimmed.is_empty() {
                let format = DocumentFormat::from_extension(trimmed)
                    .ok_or_else(|| EasyOfficeError::UnsupportedExtension(trimmed.to_string()))?;
                Self::verify_magic_for_format(bytes, format)?;
                return Ok(format);
            }
        }

        if bytes.starts_with(&Self::ZIP_MAGIC) {
            if Self::slice_contains(bytes, b"Contents/header.xml")
                || Self::slice_contains(bytes, b"version.xml")
            {
                return Ok(DocumentFormat::Hwpx);
            }
            if Self::slice_contains(bytes, b"oasis.opendocument.text") {
                return Ok(DocumentFormat::Odt);
            }
            if Self::slice_contains(bytes, b"oasis.opendocument.spreadsheet") {
                return Ok(DocumentFormat::Ods);
            }
            if Self::slice_contains(bytes, b"oasis.opendocument.presentation") {
                return Ok(DocumentFormat::Odp);
            }
            if Self::slice_contains(bytes, b"xl/workbook.xml") {
                return Ok(DocumentFormat::Xlsx);
            }
            if Self::slice_contains(bytes, b"ppt/presentation.xml") {
                return Ok(DocumentFormat::Pptx);
            }
            return Ok(DocumentFormat::Docx);
        }

        if bytes.starts_with(&Self::OLE2_MAGIC) {
            if Self::slice_contains(bytes, b"HWP Document File") {
                return Ok(DocumentFormat::Hwp);
            }
            return Ok(DocumentFormat::Doc);
        }

        if bytes.starts_with(&Self::PDF_MAGIC) {
            return Ok(DocumentFormat::Pdf);
        }

        if bytes.starts_with(&Self::RTF_MAGIC) {
            Self::validate_utf8_without_nul(bytes, "rtf")?;
            return Ok(DocumentFormat::Rtf);
        }

        Self::validate_utf8_without_nul(bytes, "plain-text")?;
        Ok(DocumentFormat::Txt)
    }

    fn verify_magic_for_format(
        bytes: &[u8],
        format: DocumentFormat,
    ) -> Result<(), EasyOfficeError> {
        let family = format.container_family();
        match family {
            ContainerFamily::ZipOoxml | ContainerFamily::ZipOdf | ContainerFamily::ZipHwpx => {
                if !bytes.starts_with(&Self::ZIP_MAGIC) {
                    return Err(EasyOfficeError::SignatureMismatch {
                        expected: family,
                        detail: format!("파일 확장자 .{}에 ZIP(PK\\x03\\x04) 헤더가 없습니다.", format.extension()),
                    });
                }
            }
            ContainerFamily::Ole2Cfbf => {
                if !bytes.starts_with(&Self::OLE2_MAGIC) {
                    return Err(EasyOfficeError::SignatureMismatch {
                        expected: family,
                        detail: format!("파일 확장자 .{}에 OLE2 CFBF 헤더가 없습니다.", format.extension()),
                    });
                }
            }
            ContainerFamily::PdfIso32000 => {
                if !bytes.starts_with(&Self::PDF_MAGIC) {
                    return Err(EasyOfficeError::SignatureMismatch {
                        expected: family,
                        detail: "PDF(%PDF-) 매직 시그니처가 없습니다.".to_string(),
                    });
                }
            }
            ContainerFamily::PlainTextUtf8 => {
                if format == DocumentFormat::Rtf {
                    if !bytes.starts_with(&Self::RTF_MAGIC) {
                        return Err(EasyOfficeError::SignatureMismatch {
                            expected: family,
                            detail: "RTF({\\rtf) 매직 시그니처가 없습니다.".to_string(),
                        });
                    }
                    Self::validate_utf8_without_nul(bytes, format.extension())?;
                } else {
                    if bytes.starts_with(&Self::ZIP_MAGIC)
                        || bytes.starts_with(&Self::OLE2_MAGIC)
                        || bytes.starts_with(&Self::PDF_MAGIC)
                    {
                        return Err(EasyOfficeError::SignatureMismatch {
                            expected: family,
                            detail: "텍스트 확장자에 바이너리 컨테이너 시그니처가 포함되어 있습니다.".to_string(),
                        });
                    }
                    Self::validate_utf8_without_nul(bytes, format.extension())?;
                }
            }
        }
        Ok(())
    }

    fn validate_utf8_without_nul(bytes: &[u8], label: &str) -> Result<(), EasyOfficeError> {
        if bytes.contains(&0x00) {
            return Err(EasyOfficeError::InvalidUtf8(format!(
                "{label} 스트림에 NUL(0x00) 바이트가 포함되어 있습니다."
            )));
        }
        std::str::from_utf8(bytes)
            .map(|_| ())
            .map_err(|_| EasyOfficeError::InvalidUtf8(format!("{label} 스트림이 유효한 UTF-8이 아닙니다.")))
    }

    fn slice_contains(haystack: &[u8], needle: &[u8]) -> bool {
        if needle.is_empty() || haystack.len() < needle.len() {
            return false;
        }
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }
}

/// Metadata descriptor for a single entry inside a ZIP-based office container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipEntryDescriptor {
    pub path: String,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub is_encrypted: bool,
}

impl ZipEntryDescriptor {
    pub fn new(
        path: impl Into<String>,
        compressed_size: u64,
        uncompressed_size: u64,
        is_encrypted: bool,
    ) -> Self {
        Self {
            path: path.into(),
            compressed_size,
            uncompressed_size,
            is_encrypted,
        }
    }
}

/// Fail-Closed security budget for ZIP containers (OOXML, ODF, HWPX).
#[derive(Debug, Clone, PartialEq)]
pub struct ZipSecurityPolicy {
    pub max_entries: usize,
    pub max_single_entry_uncompressed_bytes: u64,
    pub max_total_uncompressed_bytes: u64,
    pub max_compression_ratio: f64,
}

impl Default for ZipSecurityPolicy {
    fn default() -> Self {
        Self {
            max_entries: 4_096,
            max_single_entry_uncompressed_bytes: 128 * 1024 * 1024,
            max_total_uncompressed_bytes: 512 * 1024 * 1024,
            max_compression_ratio: 100.0,
        }
    }
}

/// Fail-Closed ZIP archive security guard defending against Zip-Slip and Decompression Bombs.
pub struct ZipSecurityGuard;

impl ZipSecurityGuard {
    pub fn validate_entry_path(path: &str) -> Result<(), EasyOfficeError> {
        if path.is_empty() {
            return Err(EasyOfficeError::SecurityViolation(
                "비어 있는 ZIP 엔트리 경로는 허용되지 않습니다.".to_string(),
            ));
        }
        if path.as_bytes().contains(&0)
            || path.starts_with('/')
            || path.starts_with('\\')
            || path.contains('\\')
        {
            return Err(EasyOfficeError::SecurityViolation(format!(
                "절대 경로, 역슬래시, 또는 NUL 바이트가 포함된 ZIP 경로: {path}"
            )));
        }
        let bytes = path.as_bytes();
        if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
            return Err(EasyOfficeError::SecurityViolation(format!(
                "Windows 드라이브 문자가 포함된 ZIP 경로: {path}"
            )));
        }
        for segment in path.split('/') {
            if segment.is_empty() || segment == "." || segment == ".." || segment.contains(':') {
                return Err(EasyOfficeError::SecurityViolation(format!(
                    "상위 디렉터리 순회(..) 또는 비정상 세그먼트가 포함된 ZIP 경로: {path}"
                )));
            }
        }
        Ok(())
    }

    pub fn validate_archive_entries(
        entries: &[ZipEntryDescriptor],
        policy: &ZipSecurityPolicy,
    ) -> Result<(), EasyOfficeError> {
        if entries.len() > policy.max_entries {
            return Err(EasyOfficeError::SecurityViolation(format!(
                "ZIP 엔트리 개수({})가 허용 상한({})을 초과했습니다.",
                entries.len(),
                policy.max_entries
            )));
        }

        let mut total_uncompressed: u64 = 0;
        for entry in entries {
            Self::validate_entry_path(&entry.path)?;

            if entry.is_encrypted {
                return Err(EasyOfficeError::EncryptedDocumentUnsupported(
                    DocumentFormat::Docx,
                ));
            }

            if entry.uncompressed_size > policy.max_single_entry_uncompressed_bytes {
                return Err(EasyOfficeError::DecompressionBombDetected {
                    ratio: entry.uncompressed_size as f64
                        / (entry.compressed_size.max(1) as f64),
                    max_ratio: policy.max_compression_ratio,
                });
            }

            total_uncompressed = total_uncompressed
                .checked_add(entry.uncompressed_size)
                .ok_or_else(|| EasyOfficeError::DecompressionBombDetected {
                    ratio: f64::INFINITY,
                    max_ratio: policy.max_compression_ratio,
                })?;

            if total_uncompressed > policy.max_total_uncompressed_bytes {
                return Err(EasyOfficeError::DecompressionBombDetected {
                    ratio: total_uncompressed as f64 / (entry.compressed_size.max(1) as f64),
                    max_ratio: policy.max_compression_ratio,
                });
            }

            if entry.compressed_size == 0 && entry.uncompressed_size > 0 {
                return Err(EasyOfficeError::DecompressionBombDetected {
                    ratio: f64::INFINITY,
                    max_ratio: policy.max_compression_ratio,
                });
            }

            if entry.compressed_size > 0 {
                let ratio = (entry.uncompressed_size as f64) / (entry.compressed_size as f64);
                if ratio > policy.max_compression_ratio {
                    return Err(EasyOfficeError::DecompressionBombDetected {
                        ratio,
                        max_ratio: policy.max_compression_ratio,
                    });
                }
            }
        }

        Ok(())
    }
}

/// Fail-Closed OLE2 Compound File Binary Format and HWP 5.0 header security guard.
pub struct Ole2SecurityGuard;

impl Ole2SecurityGuard {
    pub const SECTOR_SIZE: usize = 512;
    pub const MAX_FAT_SECTORS: usize = 109;
    pub const END_OF_CHAIN: u32 = 0xFFFF_FFFE;
    pub const FREE_SECTOR: u32 = 0xFFFF_FFFF;
    pub const HWP5_FILE_HEADER_LEN: usize = 256;
    pub const HWP5_SIGNATURE: &'static [u8] = b"HWP Document File";
    pub const HWP5_FLAG_ENCRYPTED: u32 = 0x02;
    pub const HWP5_FLAG_DISTRIBUTION: u32 = 0x04;

    pub fn validate_sector_chain(
        container_bytes: &[u8],
        fat: &[u32],
        start_sector: u32,
    ) -> Result<Vec<u32>, EasyOfficeError> {
        if container_bytes.len() < Self::SECTOR_SIZE * 2
            || container_bytes.len() % Self::SECTOR_SIZE != 0
        {
            return Err(EasyOfficeError::CorruptedContainer(
                "OLE2 컨테이너 크기가 512바이트 섹터 배수가 아닙니다.".to_string(),
            ));
        }
        if !container_bytes.starts_with(&FormatDetector::OLE2_MAGIC) {
            return Err(EasyOfficeError::SignatureMismatch {
                expected: ContainerFamily::Ole2Cfbf,
                detail: "OLE2 CFBF 헤더 시그니처 불일치".to_string(),
            });
        }

        let fat_sector_count = u32::from_le_bytes([
            container_bytes[44],
            container_bytes[45],
            container_bytes[46],
            container_bytes[47],
        ]) as usize;
        if fat_sector_count == 0 || fat_sector_count > Self::MAX_FAT_SECTORS {
            return Err(EasyOfficeError::SecurityViolation(format!(
                "OLE2 DIFAT 섹터 개수({fat_sector_count})가 허용 범위(1..={})를 벗어났습니다.",
                Self::MAX_FAT_SECTORS
            )));
        }

        let total_sectors = (container_bytes.len() / Self::SECTOR_SIZE) - 1;
        let mut visited: HashSet<u32> = HashSet::new();
        let mut chain: Vec<u32> = Vec::new();
        let mut current = start_sector;

        while current != Self::END_OF_CHAIN {
            if !visited.insert(current) {
                return Err(EasyOfficeError::SecurityViolation(format!(
                    "OLE2 FAT 섹터 체인 순환 루프가 감지되었습니다 (sector: {current})."
                )));
            }
            let idx = current as usize;
            if idx >= total_sectors || idx >= fat.len() {
                return Err(EasyOfficeError::CorruptedContainer(format!(
                    "OLE2 FAT 체인이 유효하지 않은 섹터({current})를 참조합니다."
                )));
            }
            chain.push(current);
            current = fat[idx];
        }

        Ok(chain)
    }

    pub fn validate_hwp5_file_header(header_bytes: &[u8]) -> Result<u32, EasyOfficeError> {
        if header_bytes.len() < Self::HWP5_FILE_HEADER_LEN {
            return Err(EasyOfficeError::CorruptedContainer(format!(
                "HWP 5.0 FileHeader 길이({})가 256바이트보다 작습니다.",
                header_bytes.len()
            )));
        }
        if !header_bytes.starts_with(Self::HWP5_SIGNATURE) {
            return Err(EasyOfficeError::SignatureMismatch {
                expected: ContainerFamily::Ole2Cfbf,
                detail: "HWP 5.0 'HWP Document File' 시그니처가 없습니다.".to_string(),
            });
        }
        let version = u32::from_le_bytes([
            header_bytes[32],
            header_bytes[33],
            header_bytes[34],
            header_bytes[35],
        ]);
        let flags = u32::from_le_bytes([
            header_bytes[36],
            header_bytes[37],
            header_bytes[38],
            header_bytes[39],
        ]);
        if (flags & Self::HWP5_FLAG_ENCRYPTED) != 0
            || (flags & Self::HWP5_FLAG_DISTRIBUTION) != 0
        {
            return Err(EasyOfficeError::EncryptedDocumentUnsupported(
                DocumentFormat::Hwp,
            ));
        }
        Ok(version)
    }
}

/// Zero-allocation XML sanitizer blocking XXE / DTD declarations and escaping XML entities.
pub struct XmlSanitizer;

impl XmlSanitizer {
    pub fn validate_safe_xml(xml: &str) -> Result<(), EasyOfficeError> {
        let bytes = xml.as_bytes();
        if bytes.len() < 8 {
            return Ok(());
        }
        let mut idx = 0;
        while idx + 8 <= bytes.len() {
            if bytes[idx] == b'<' && bytes[idx + 1] == b'!' {
                let mut kw = idx + 2;
                while kw < bytes.len() && bytes[kw].is_ascii_whitespace() {
                    kw += 1;
                }
                if kw + 6 <= bytes.len() && bytes[kw..kw + 6].eq_ignore_ascii_case(b"ENTITY") {
                    return Err(EasyOfficeError::XxeOrExternalEntityDetected);
                }
                if kw + 7 <= bytes.len() && bytes[kw..kw + 7].eq_ignore_ascii_case(b"DOCTYPE") {
                    return Err(EasyOfficeError::XxeOrExternalEntityDetected);
                }
            }
            idx += 1;
        }
        Ok(())
    }

    pub fn escape(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for ch in text.chars() {
            match ch {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                '\'' => out.push_str("&apos;"),
                _ => out.push(ch),
            }
        }
        out
    }

    pub fn unescape(text: &str) -> String {
        text.replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&")
    }
}

/// RFC 4180 state-machine CSV / TSV parser and serializer.
pub struct CsvCodec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CsvParseState {
    StartOfField,
    InUnquotedField,
    InQuotedField,
    QuoteInQuotedField,
}

impl CsvCodec {
    pub fn parse(input: &str, delimiter: char) -> Result<Vec<Vec<String>>, EasyOfficeError> {
        if input.is_empty() {
            return Ok(Vec::new());
        }
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut current_row: Vec<String> = Vec::new();
        let mut current_field = String::new();
        let mut state = CsvParseState::StartOfField;
        let mut chars = input.chars().peekable();

        while let Some(ch) = chars.next() {
            match state {
                CsvParseState::StartOfField => {
                    if ch == '"' {
                        current_field.clear();
                        state = CsvParseState::InQuotedField;
                    } else if ch == delimiter {
                        current_row.push(String::new());
                    } else if ch == '\r' || ch == '\n' {
                        if ch == '\r' && chars.peek() == Some(&'\n') {
                            chars.next();
                        }
                        current_row.push(String::new());
                        rows.push(std::mem::take(&mut current_row));
                    } else {
                        current_field.clear();
                        current_field.push(ch);
                        state = CsvParseState::InUnquotedField;
                    }
                }
                CsvParseState::InUnquotedField => {
                    if ch == '"' {
                        return Err(EasyOfficeError::CorruptedContainer(
                            "비따옴표 CSV 필드 내부에 예기치 않은 따옴표가 발견되었습니다.".to_string(),
                        ));
                    } else if ch == delimiter {
                        current_row.push(std::mem::take(&mut current_field));
                        state = CsvParseState::StartOfField;
                    } else if ch == '\r' || ch == '\n' {
                        if ch == '\r' && chars.peek() == Some(&'\n') {
                            chars.next();
                        }
                        current_row.push(std::mem::take(&mut current_field));
                        rows.push(std::mem::take(&mut current_row));
                        state = CsvParseState::StartOfField;
                    } else {
                        current_field.push(ch);
                    }
                }
                CsvParseState::InQuotedField => {
                    if ch == '"' {
                        state = CsvParseState::QuoteInQuotedField;
                    } else {
                        current_field.push(ch);
                    }
                }
                CsvParseState::QuoteInQuotedField => {
                    if ch == '"' {
                        current_field.push('"');
                        state = CsvParseState::InQuotedField;
                    } else if ch == delimiter {
                        current_row.push(std::mem::take(&mut current_field));
                        state = CsvParseState::StartOfField;
                    } else if ch == '\r' || ch == '\n' {
                        if ch == '\r' && chars.peek() == Some(&'\n') {
                            chars.next();
                        }
                        current_row.push(std::mem::take(&mut current_field));
                        rows.push(std::mem::take(&mut current_row));
                        state = CsvParseState::StartOfField;
                    } else {
                        return Err(EasyOfficeError::CorruptedContainer(
                            "따옴표 CSV 필드 닫힘 이후 잘못된 문자가 발견되었습니다.".to_string(),
                        ));
                    }
                }
            }
        }

        match state {
            CsvParseState::InQuotedField => {
                return Err(EasyOfficeError::CorruptedContainer(
                    "닫히지 않은 따옴표 CSV 필드가 있습니다.".to_string(),
                ));
            }
            CsvParseState::QuoteInQuotedField | CsvParseState::InUnquotedField => {
                current_row.push(current_field);
                rows.push(current_row);
            }
            CsvParseState::StartOfField => {
                if !current_row.is_empty() {
                    current_row.push(String::new());
                    rows.push(current_row);
                }
            }
        }

        Ok(rows)
    }

    pub fn serialize(rows: &[Vec<String>], delimiter: char) -> String {
        let delim_str = delimiter.to_string();
        rows.iter()
            .map(|row| {
                row.iter()
                    .map(|field| Self::serialize_field(field, delimiter))
                    .collect::<Vec<_>>()
                    .join(&delim_str)
            })
            .collect::<Vec<_>>()
            .join("\r\n")
    }

    fn serialize_field(field: &str, delimiter: char) -> String {
        if field.contains(delimiter)
            || field.contains('"')
            || field.contains('\n')
            || field.contains('\r')
        {
            let escaped = field.replace('"', "\"\"");
            format!("\"{escaped}\"")
        } else {
            field.to_string()
        }
    }
}

/// Block element inside a CommonMark Markdown document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkdownBlock {
    Heading { level: u8, text: String },
    Paragraph(String),
    BlockQuote(String),
    BulletList(Vec<String>),
    NumberedList(Vec<String>),
}

/// CommonMark Markdown parser and serializer.
pub struct CommonMarkCodec;

impl CommonMarkCodec {
    pub fn parse(markdown: &str) -> Result<Vec<MarkdownBlock>, EasyOfficeError> {
        if markdown.as_bytes().contains(&0) {
            return Err(EasyOfficeError::InvalidUtf8(
                "Markdown 스트림에 NUL 바이트가 포함되어 있습니다.".to_string(),
            ));
        }

        let mut blocks: Vec<MarkdownBlock> = Vec::new();
        let mut bullet_acc: Vec<String> = Vec::new();
        let mut numbered_acc: Vec<String> = Vec::new();

        let flush_lists = |blocks: &mut Vec<MarkdownBlock>,
                           bullet_acc: &mut Vec<String>,
                           numbered_acc: &mut Vec<String>| {
            if !bullet_acc.is_empty() {
                blocks.push(MarkdownBlock::BulletList(std::mem::take(bullet_acc)));
            }
            if !numbered_acc.is_empty() {
                blocks.push(MarkdownBlock::NumberedList(std::mem::take(numbered_acc)));
            }
        };

        for raw_line in markdown.lines() {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() {
                flush_lists(&mut blocks, &mut bullet_acc, &mut numbered_acc);
                continue;
            }

            if let Some(rest) = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
            {
                if !numbered_acc.is_empty() {
                    blocks.push(MarkdownBlock::NumberedList(std::mem::take(
                        &mut numbered_acc,
                    )));
                }
                bullet_acc.push(rest.trim().to_string());
                continue;
            }

            if let Some(dot_idx) = trimmed.find(". ") {
                let prefix = &trimmed[..dot_idx];
                if !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit()) {
                    if !bullet_acc.is_empty() {
                        blocks.push(MarkdownBlock::BulletList(std::mem::take(&mut bullet_acc)));
                    }
                    numbered_acc.push(trimmed[dot_idx + 2..].trim().to_string());
                    continue;
                }
            }

            flush_lists(&mut blocks, &mut bullet_acc, &mut numbered_acc);

            if let Some(h4) = trimmed.strip_prefix("#### ") {
                blocks.push(MarkdownBlock::Heading {
                    level: 4,
                    text: h4.trim().to_string(),
                });
            } else if let Some(h3) = trimmed.strip_prefix("### ") {
                blocks.push(MarkdownBlock::Heading {
                    level: 3,
                    text: h3.trim().to_string(),
                });
            } else if let Some(h2) = trimmed.strip_prefix("## ") {
                blocks.push(MarkdownBlock::Heading {
                    level: 2,
                    text: h2.trim().to_string(),
                });
            } else if let Some(h1) = trimmed.strip_prefix("# ") {
                blocks.push(MarkdownBlock::Heading {
                    level: 1,
                    text: h1.trim().to_string(),
                });
            } else if let Some(quote) = trimmed.strip_prefix("> ") {
                blocks.push(MarkdownBlock::BlockQuote(quote.trim().to_string()));
            } else {
                blocks.push(MarkdownBlock::Paragraph(trimmed.to_string()));
            }
        }

        flush_lists(&mut blocks, &mut bullet_acc, &mut numbered_acc);
        Ok(blocks)
    }

    pub fn render(blocks: &[MarkdownBlock]) -> String {
        let mut rendered: Vec<String> = Vec::with_capacity(blocks.len());
        for block in blocks {
            match block {
                MarkdownBlock::Heading { level, text } => {
                    let hashes = "#".repeat((*level).clamp(1, 6) as usize);
                    rendered.push(format!("{hashes} {text}"));
                }
                MarkdownBlock::Paragraph(text) => rendered.push(text.clone()),
                MarkdownBlock::BlockQuote(text) => rendered.push(format!("> {text}")),
                MarkdownBlock::BulletList(items) => {
                    let lines = items
                        .iter()
                        .map(|item| format!("- {item}"))
                        .collect::<Vec<_>>()
                        .join("\n");
                    rendered.push(lines);
                }
                MarkdownBlock::NumberedList(items) => {
                    let lines = items
                        .iter()
                        .enumerate()
                        .map(|(idx, item)| format!("{}. {item}", idx + 1))
                        .collect::<Vec<_>>()
                        .join("\n");
                    rendered.push(lines);
                }
            }
        }
        rendered.join("\n\n")
    }
}

/// Page model for ISO 32000-1 PDF serialization and parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfPageModel {
    pub page_number: usize,
    pub rotation_degrees: i32,
    pub body_text: String,
    pub approval_stamp: Option<String>,
}

/// Document model for ISO 32000-1 PDF serialization and parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfDocumentModel {
    pub title: String,
    pub pages: Vec<PdfPageModel>,
    pub watermark_text: Option<String>,
}

/// ISO 32000-1 (`%PDF-1.4`) binary serializer and parser with byte-accurate `xref` table.
pub struct PdfSerializer;

impl PdfSerializer {
    pub fn serialize(doc: &PdfDocumentModel) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::with_capacity(2048);
        let push_line = |buf: &mut Vec<u8>, line: &str| {
            buf.extend_from_slice(line.as_bytes());
            buf.push(b'\n');
        };

        push_line(&mut out, "%PDF-1.4");

        let effective_pages: Vec<PdfPageModel> = if doc.pages.is_empty() {
            vec![PdfPageModel {
                page_number: 1,
                rotation_degrees: 0,
                body_text: String::new(),
                approval_stamp: None,
            }]
        } else {
            doc.pages.clone()
        };
        let page_count = effective_pages.len();
        let page_obj_ids: Vec<usize> = (0..page_count).map(|i| 3 + i * 2).collect();
        let kids_refs = page_obj_ids
            .iter()
            .map(|id| format!("{id} 0 R"))
            .collect::<Vec<_>>()
            .join(" ");

        let total_objects = 3 + page_count * 2;
        let mut offsets: Vec<usize> = vec![0; total_objects];

        offsets[1] = out.len();
        push_line(&mut out, "1 0 obj");
        push_line(
            &mut out,
            &format!(
                "<< /Type /Catalog /Pages 2 0 R /Title ({}) >>",
                Self::escape_pdf_text(&doc.title)
            ),
        );
        push_line(&mut out, "endobj");

        offsets[2] = out.len();
        push_line(&mut out, "2 0 obj");
        push_line(
            &mut out,
            &format!("<< /Type /Pages /Count {page_count} /Kids [{kids_refs}] >>"),
        );
        push_line(&mut out, "endobj");

        for (idx, page) in effective_pages.iter().enumerate() {
            let page_obj = 3 + idx * 2;
            let content_obj = page_obj + 1;

            let mut stream_lines: Vec<String> = Vec::new();
            stream_lines.push(format!(
                "BT /F1 12 Tf 54 780 Td ({}) Tj ET",
                Self::escape_pdf_text(&page.body_text)
            ));
            if let Some(stamp) = &page.approval_stamp {
                stream_lines.push(format!(
                    "BT /F1 10 Tf 386 604 Td ([STAMP] {}) Tj ET",
                    Self::escape_pdf_text(stamp)
                ));
            }
            if let Some(wm) = &doc.watermark_text {
                stream_lines.push(format!(
                    "BT /F1 10 Tf 320 746 Td ([WM] {}) Tj ET",
                    Self::escape_pdf_text(wm)
                ));
            }
            let stream_body = stream_lines.join("\n");

            offsets[page_obj] = out.len();
            push_line(&mut out, &format!("{page_obj} 0 obj"));
            push_line(
                &mut out,
                &format!(
                    "<< /Type /Page /Parent 2 0 R /PageNumber {} /Rotate {} /MediaBox [0 0 595 842] /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >> /Contents {content_obj} 0 R >>",
                    page.page_number, page.rotation_degrees
                ),
            );
            push_line(&mut out, "endobj");

            offsets[content_obj] = out.len();
            push_line(&mut out, &format!("{content_obj} 0 obj"));
            push_line(&mut out, &format!("<< /Length {} >>", stream_body.len()));
            push_line(&mut out, "stream");
            push_line(&mut out, &stream_body);
            push_line(&mut out, "endstream");
            push_line(&mut out, "endobj");
        }

        let xref_offset = out.len();
        push_line(&mut out, "xref");
        push_line(&mut out, &format!("0 {total_objects}"));
        push_line(&mut out, "0000000000 65535 f ");
        for offset in offsets.iter().skip(1) {
            push_line(&mut out, &format!("{offset:010} 00000 n "));
        }
        push_line(&mut out, "trailer");
        push_line(
            &mut out,
            &format!("<< /Size {total_objects} /Root 1 0 R >>"),
        );
        push_line(&mut out, "startxref");
        push_line(&mut out, &xref_offset.to_string());
        push_line(&mut out, "%%EOF");

        out
    }

    pub fn parse(bytes: &[u8]) -> Result<PdfDocumentModel, EasyOfficeError> {
        let detected = FormatDetector::detect(bytes, Some("pdf"))?;
        if detected != DocumentFormat::Pdf {
            return Err(EasyOfficeError::SignatureMismatch {
                expected: ContainerFamily::PdfIso32000,
                detail: "유효한 PDF 시그니처가 아닙니다.".to_string(),
            });
        }
        let raw = String::from_utf8_lossy(bytes);
        if !raw.contains("xref") || !raw.contains("%%EOF") {
            return Err(EasyOfficeError::CorruptedContainer(
                "PDF xref 테이블 또는 %%EOF 트레일러가 누락되었습니다.".to_string(),
            ));
        }
        if raw.contains("/Encrypt") {
            return Err(EasyOfficeError::EncryptedDocumentUnsupported(
                DocumentFormat::Pdf,
            ));
        }

        let mut title = String::from("PDF 문서");
        let mut watermark_text: Option<String> = None;
        let mut pages: Vec<PdfPageModel> = Vec::new();
        let mut current_page_number: Option<usize> = None;
        let mut current_rotation = 0;
        let mut current_body = String::new();
        let mut current_stamp: Option<String> = None;
        let mut in_stream = false;

        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.contains("/Type /Catalog") {
                if let Some(t) = Self::extract_paren_token(trimmed, "/Title (") {
                    title = Self::unescape_pdf_text(&t);
                }
            }
            if trimmed.contains("/Type /Page") && !trimmed.contains("/Type /Pages") {
                if let Some(pn_idx) = trimmed.find("/PageNumber ") {
                    let after = &trimmed[pn_idx + 12..];
                    let num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
                    current_page_number = num_str.parse::<usize>().ok();
                }
                if let Some(rot_idx) = trimmed.find("/Rotate ") {
                    let after = &trimmed[rot_idx + 8..];
                    let num_str: String = after
                        .chars()
                        .take_while(|c| c.is_ascii_digit() || *c == '-')
                        .collect();
                    current_rotation = num_str.parse::<i32>().unwrap_or(0);
                }
            }
            if trimmed == "stream" {
                in_stream = true;
                current_body.clear();
                current_stamp = None;
                continue;
            }
            if trimmed == "endstream" {
                if in_stream {
                    pages.push(PdfPageModel {
                        page_number: current_page_number.take().unwrap_or(pages.len() + 1),
                        rotation_degrees: current_rotation,
                        body_text: current_body.clone(),
                        approval_stamp: current_stamp.take(),
                    });
                    current_rotation = 0;
                    in_stream = false;
                }
                continue;
            }
            if in_stream && trimmed.starts_with("BT ") {
                if let (Some(open), Some(close)) = (trimmed.find('('), trimmed.rfind(')')) {
                    if open < close {
                        let content = Self::unescape_pdf_text(&trimmed[open + 1..close]);
                        if let Some(wm) = content.strip_prefix("[WM] ") {
                            watermark_text = Some(wm.to_string());
                        } else if let Some(stamp) = content.strip_prefix("[STAMP] ") {
                            current_stamp = Some(stamp.to_string());
                        } else {
                            if !current_body.is_empty() {
                                current_body.push('\n');
                            }
                            current_body.push_str(&content);
                        }
                    }
                }
            }
        }

        if pages.is_empty() {
            return Err(EasyOfficeError::CorruptedContainer(
                "PDF 페이지 스트림을 찾을 수 없습니다.".to_string(),
            ));
        }

        Ok(PdfDocumentModel {
            title,
            pages,
            watermark_text,
        })
    }

    fn extract_paren_token(line: &str, prefix: &str) -> Option<String> {
        let start = line.find(prefix)? + prefix.len();
        let rest = &line[start..];
        let mut out = String::new();
        let mut chars = rest.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                out.push('\\');
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else if c == ')' {
                return Some(out);
            } else {
                out.push(c);
            }
        }
        None
    }

    pub fn escape_pdf_text(text: &str) -> String {
        let mut out = String::with_capacity(text.len() + 8);
        for ch in text.chars() {
            match ch {
                '\\' => out.push_str("\\\\"),
                '(' => out.push_str("\\("),
                ')' => out.push_str("\\)"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                _ => out.push(ch),
            }
        }
        out
    }

    pub fn unescape_pdf_text(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('r') => out.push('\r'),
                    Some(escaped) => out.push(escaped),
                    None => out.push('\\'),
                }
            } else {
                out.push(ch);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_twenty_formats_and_magic_signature_detection() {
        assert_eq!(DocumentFormat::ALL.len(), 20);

        let zip_sample = b"PK\x03\x04\x14\x00\x00\x00word/document.xml";
        assert_eq!(
            FormatDetector::detect(zip_sample, None).unwrap(),
            DocumentFormat::Docx
        );
        assert_eq!(
            FormatDetector::detect(b"PK\x03\x04Contents/header.xml", None).unwrap(),
            DocumentFormat::Hwpx
        );
        assert_eq!(
            FormatDetector::detect(b"PK\x03\x04oasis.opendocument.text", None).unwrap(),
            DocumentFormat::Odt
        );
        assert_eq!(
            FormatDetector::detect(b"PK\x03\x04oasis.opendocument.spreadsheet", None).unwrap(),
            DocumentFormat::Ods
        );
        assert_eq!(
            FormatDetector::detect(b"PK\x03\x04oasis.opendocument.presentation", None).unwrap(),
            DocumentFormat::Odp
        );
        assert_eq!(
            FormatDetector::detect(b"PK\x03\x04xl/workbook.xml", None).unwrap(),
            DocumentFormat::Xlsx
        );
        assert_eq!(
            FormatDetector::detect(b"PK\x03\x04ppt/presentation.xml", None).unwrap(),
            DocumentFormat::Pptx
        );
        assert_eq!(
            FormatDetector::detect(zip_sample, Some("report.docx")).unwrap(),
            DocumentFormat::Docx
        );
        assert_eq!(
            FormatDetector::detect(zip_sample, Some("template.dotx")).unwrap(),
            DocumentFormat::Docx
        );
        assert_eq!(
            FormatDetector::detect(zip_sample, Some("official.hwpx")).unwrap(),
            DocumentFormat::Hwpx
        );
        assert_eq!(
            FormatDetector::detect(zip_sample, Some("template.hwtx")).unwrap(),
            DocumentFormat::Hwtx
        );
        assert_eq!(
            FormatDetector::detect(zip_sample, Some("show.ppsx")).unwrap(),
            DocumentFormat::Ppsx
        );

        let ole2_sample = b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1HWP Document File";
        assert_eq!(
            FormatDetector::detect(ole2_sample, Some("proposal.hwp")).unwrap(),
            DocumentFormat::Hwp
        );
        assert_eq!(
            FormatDetector::detect(ole2_sample, Some("template.hwt")).unwrap(),
            DocumentFormat::Hwt
        );

        let pdf_sample = b"%PDF-1.4\n1 0 obj\n<<>>\nendobj\n%%EOF";
        assert_eq!(
            FormatDetector::detect(pdf_sample, Some("contract.pdf")).unwrap(),
            DocumentFormat::Pdf
        );

        let rtf_sample = b"{\\rtf1\\ansi\\deff0 Hello}";
        assert_eq!(
            FormatDetector::detect(rtf_sample, Some("memo.rtf")).unwrap(),
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
        let safe_entries = vec![
            ZipEntryDescriptor::new("[Content_Types].xml", 512, 2048, false),
            ZipEntryDescriptor::new("word/document.xml", 1024, 4096, false),
        ];
        assert!(ZipSecurityGuard::validate_archive_entries(&safe_entries, &policy).is_ok());

        let traversal_entries = vec![ZipEntryDescriptor::new("../../etc/passwd", 100, 200, false)];
        assert!(matches!(
            ZipSecurityGuard::validate_archive_entries(&traversal_entries, &policy),
            Err(EasyOfficeError::SecurityViolation(_))
        ));

        let win_drive_entries =
            vec![ZipEntryDescriptor::new("C:\\Windows\\win.ini", 100, 200, false)];
        assert!(matches!(
            ZipSecurityGuard::validate_archive_entries(&win_drive_entries, &policy),
            Err(EasyOfficeError::SecurityViolation(_))
        ));

        let bomb_entries = vec![ZipEntryDescriptor::new(
            "word/document.xml",
            100,
            20_000,
            false,
        )];
        assert!(matches!(
            ZipSecurityGuard::validate_archive_entries(&bomb_entries, &policy),
            Err(EasyOfficeError::DecompressionBombDetected { .. })
        ));
    }

    #[test]
    fn test_ole2_sector_cycle_and_hwp5_encryption_guard() {
        let mut container = vec![0u8; 512 * 4];
        container[..8].copy_from_slice(&FormatDetector::OLE2_MAGIC);
        container[44..48].copy_from_slice(&1u32.to_le_bytes());

        let cyclic_fat = vec![1u32, 0u32, Ole2SecurityGuard::END_OF_CHAIN];
        assert!(matches!(
            Ole2SecurityGuard::validate_sector_chain(&container, &cyclic_fat, 0),
            Err(EasyOfficeError::SecurityViolation(_))
        ));

        let valid_fat = vec![1u32, 2u32, Ole2SecurityGuard::END_OF_CHAIN];
        let chain = Ole2SecurityGuard::validate_sector_chain(&container, &valid_fat, 0).unwrap();
        assert_eq!(chain, vec![0, 1, 2]);

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
    fn test_xml_sanitizer_blocks_xxe_and_escapes_entities() {
        let safe_xml = "<w:document><w:body><w:p><w:t>안전한 문서</w:t></w:p></w:body></w:document>";
        assert!(XmlSanitizer::validate_safe_xml(safe_xml).is_ok());

        let xxe_xml = "<?xml version=\"1.0\"?><!DOCTYPE foo [<!ENTITY xxe SYSTEM \"file:///etc/passwd\">]><foo>&xxe;</foo>";
        assert_eq!(
            XmlSanitizer::validate_safe_xml(xxe_xml),
            Err(EasyOfficeError::XxeOrExternalEntityDetected)
        );

        let escaped = XmlSanitizer::escape("A & B < C > \"D\" 'E'");
        assert_eq!(escaped, "A &amp; B &lt; C &gt; &quot;D&quot; &apos;E&apos;");
        assert_eq!(XmlSanitizer::unescape(&escaped), "A & B < C > \"D\" 'E'");
    }

    #[test]
    fn test_csv_commonmark_and_pdf_codecs_roundtrip() {
        let rows = vec![
            vec!["부서".to_string(), "메모, 특이사항".to_string()],
            vec![
                "연구개발".to_string(),
                "1행\n2행 \"강조\"".to_string(),
            ],
        ];
        let csv_text = CsvCodec::serialize(&rows, ',');
        let parsed_rows = CsvCodec::parse(&csv_text, ',').unwrap();
        assert_eq!(parsed_rows, rows);

        let md_input = "# 제품 아키텍처\n\n본문 첫 문단입니다.\n\n> 핵심 설계 원칙\n\n- 항목 1\n- 항목 2";
        let md_blocks = CommonMarkCodec::parse(md_input).unwrap();
        let md_rendered = CommonMarkCodec::render(&md_blocks);
        assert_eq!(md_rendered, md_input);

        let pdf_doc = PdfDocumentModel {
            title: "분기 보안 감사 보고서".to_string(),
            pages: vec![
                PdfPageModel {
                    page_number: 1,
                    rotation_degrees: 0,
                    body_text: "ISO 32000-1 PDF 본문 텍스트\n2행 개행 보존".to_string(),
                    approval_stamp: Some("APPROVED".to_string()),
                },
                PdfPageModel {
                    page_number: 5,
                    rotation_degrees: 90,
                    body_text: "2페이지 부록".to_string(),
                    approval_stamp: None,
                },
            ],
            watermark_text: Some("CONFIDENTIAL".to_string()),
        };
        let pdf_bytes = PdfSerializer::serialize(&pdf_doc);
        let decoded_pdf = PdfSerializer::parse(&pdf_bytes).unwrap();
        assert_eq!(decoded_pdf, pdf_doc);
    }
}

use std::fmt;

pub mod biff8_xls_codec;
pub mod converter;
pub mod csv_markdown_codecs;
pub mod deflate;
pub mod hwp5_codec;
pub mod hwp_eqedit;
mod legacy_ole2_codec;
pub mod odf_hwpx_codecs;
pub mod office_codecs;
pub mod ole2_codec;
mod pdf_stream_parser;
pub mod rtf_xml_helpers;
pub mod strict_json;
pub mod text_pdf_codecs;
pub mod utf16_swar;
pub mod zip_codec;

pub use converter::*;
pub use csv_markdown_codecs::*;
pub use deflate::*;
pub use hwp_eqedit::*;
pub use office_codecs::*;
pub use ole2_codec::*;
pub use rtf_xml_helpers::{
    unescape_xml_five_pass_baseline, unescape_xml_single_pass, RtfCodec, RtfPayload,
};
pub use strict_json::StrictJsonValue;
pub use text_pdf_codecs::*;
pub use utf16_swar::{
    decode_utf16le_swar, decode_utf16le_vec_baseline, encode_utf16le_scalar_baseline,
    encode_utf16le_swar, encode_utf16le_swar_into, expand_ascii4_to_utf16le_u64, SectorBitset,
};
pub use zip_codec::*;

pub const MAX_BIFF8_ROW: u32 = 65_535;
pub const MAX_BIFF8_COL: u32 = 255;
pub const MAX_BIFF8_RECORD_DATA_LEN: usize = 8_224;
pub const MAX_HWP_TABLE_ROWS: usize = 4_096;
pub const MAX_HWP_TABLE_COLS: usize = 256;
pub const MAX_OLE2_DIR_ENTRIES: usize = 4_096;

pub type FormatError = EasyOfficeError;

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
            "docx" | "dotx" | "docm" | "dotm" => Some(Self::Docx),
            "doc" => Some(Self::Doc),
            "xlsx" | "xltx" | "xlsm" | "xltm" => Some(Self::Xlsx),
            "xls" => Some(Self::Xls),
            "pptx" | "potx" | "pptm" | "potm" => Some(Self::Pptx),
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
    ResourceBudgetExceeded(String),
    EncryptedDocumentUnsupported(DocumentFormat),
    XxeOrExternalEntityDetected,
    FormulaError(String),
    InvalidArgument(String),
}

impl fmt::Display for EasyOfficeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPayload => {
                write!(f, "파일 데이터가 비어 있어 열 수 없습니다 (Fail-Closed).")
            }
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
            Self::ResourceBudgetExceeded(detail) => {
                write!(f, "리소스 예산 한도 초과 (Fail-Closed): {detail}")
            }
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
                if Self::slice_contains(bytes, b"isTemplate=\"1\"") {
                    return Ok(DocumentFormat::Hwtx);
                }
                if let Ok(entries) = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()) {
                    if entries.iter().any(|e| {
                        e.path == "Contents/header.xml"
                            && Self::slice_contains(&e.data, b"isTemplate=\"1\"")
                    }) {
                        return Ok(DocumentFormat::Hwtx);
                    }
                }
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
            if Self::slice_contains(bytes, b"presentationml.slideshow") {
                return Ok(DocumentFormat::Ppsx);
            }
            if Self::slice_contains(bytes, b"ppt/presentation.xml") {
                if let Ok(entries) = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()) {
                    if entries.iter().any(|e| {
                        (e.path == "[Content_Types].xml"
                            && Self::slice_contains(&e.data, b"presentationml.slideshow"))
                            || (e.path == "ppt/presentation.xml"
                                && Self::slice_contains(&e.data, b"slideshow=\"1\""))
                    }) {
                        return Ok(DocumentFormat::Ppsx);
                    }
                }
                return Ok(DocumentFormat::Pptx);
            }
            return Ok(DocumentFormat::Docx);
        }

        if bytes.starts_with(&Self::OLE2_MAGIC) {
            if Self::slice_contains(bytes, b"HWT_TEMPLATE") {
                return Ok(DocumentFormat::Hwt);
            }
            if Self::slice_contains(bytes, b"HWP Document File") {
                return Ok(DocumentFormat::Hwp);
            }
            let xls_utf16 = [
                b'W', 0, b'o', 0, b'r', 0, b'k', 0, b'b', 0, b'o', 0, b'o', 0, b'k', 0,
            ];
            if Self::slice_contains(bytes, &xls_utf16) {
                return Ok(DocumentFormat::Xls);
            }
            let ppt_utf16 = [b'P', 0, b'o', 0, b'w', 0, b'e', 0, b'r', 0];
            if Self::slice_contains(bytes, &ppt_utf16) {
                return Ok(DocumentFormat::Ppt);
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
        if bytes.starts_with(b"# ") || bytes.starts_with(b"## ") {
            return Ok(DocumentFormat::Markdown);
        }
        if let Some(first_line_end) = bytes.iter().position(|&b| b == b'\n') {
            let first_line = &bytes[..first_line_end];
            if first_line.contains(&b'\t') {
                return Ok(DocumentFormat::Tsv);
            }
            if first_line.contains(&b',') && bytes.contains(&b'\r') {
                return Ok(DocumentFormat::Csv);
            }
        }
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
                        detail: format!(
                            "파일 확장자 .{}에 ZIP(PK\\x03\\x04) 헤더가 없습니다.",
                            format.extension()
                        ),
                    });
                }
            }
            ContainerFamily::Ole2Cfbf => {
                if !bytes.starts_with(&Self::OLE2_MAGIC) {
                    return Err(EasyOfficeError::SignatureMismatch {
                        expected: family,
                        detail: format!(
                            "파일 확장자 .{}에 OLE2 CFBF 헤더가 없습니다.",
                            format.extension()
                        ),
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
                            detail:
                                "텍스트 확장자에 바이너리 컨테이너 시그니처가 포함되어 있습니다."
                                    .to_string(),
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
        std::str::from_utf8(bytes).map(|_| ()).map_err(|_| {
            EasyOfficeError::InvalidUtf8(format!("{label} 스트림이 유효한 UTF-8이 아닙니다."))
        })
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
    pub const MAX_ENTRIES: usize = 4_096;
    pub const MAX_SINGLE_ENTRY_UNCOMPRESSED_BYTES: usize = 128 * 1024 * 1024;
    pub const MAX_TOTAL_UNCOMPRESSED_BYTES: usize = 512 * 1024 * 1024;
    pub const MAX_COMPRESSION_RATIO: u64 = 100;

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
        let trimmed = path.strip_suffix('/').unwrap_or(path);
        if trimmed.is_empty() {
            return Err(EasyOfficeError::SecurityViolation(format!(
                "유효하지 않은 ZIP 루트 디렉터리 경로: {path}"
            )));
        }
        for segment in trimmed.split('/') {
            if segment.is_empty() || segment == "." || segment == ".." || segment.contains(':') {
                return Err(EasyOfficeError::SecurityViolation(format!(
                    "상위 디렉터리 순회(..) 또는 비정상 세그먼트가 포함된 ZIP 경로: {path}"
                )));
            }
        }
        Ok(())
    }

    pub fn validate_archive(
        entries: &[ZipEntryDescriptor],
        policy: &ZipSecurityPolicy,
    ) -> Result<(), EasyOfficeError> {
        Self::validate_archive_entries(entries, policy)
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
        let mut seen_lower_paths = std::collections::HashSet::with_capacity(entries.len());
        let mut seen_files_lower = std::collections::HashSet::with_capacity(entries.len());
        let mut seen_dirs_lower = std::collections::HashSet::with_capacity(entries.len());

        for entry in entries {
            Self::validate_entry_path(&entry.path)?;
            let is_dir = entry.path.ends_with('/');
            let lower = entry.path.to_ascii_lowercase();
            let trimmed_lower = lower.strip_suffix('/').unwrap_or(&lower).to_string();

            if !seen_lower_paths.insert(trimmed_lower.clone()) {
                return Err(EasyOfficeError::SecurityViolation(format!(
                    "중복되거나 파일/디렉터리가 충돌하는 ZIP 엔트리 경로가 감지되었습니다: {}",
                    entry.path
                )));
            }
            for (idx, _) in trimmed_lower.match_indices('/') {
                let prefix = &trimmed_lower[..idx];
                if seen_files_lower.contains(prefix) {
                    return Err(EasyOfficeError::SecurityViolation(format!(
                        "ZIP 파일 경로와 디렉터리 프리픽스 충돌이 감지되었습니다: {}",
                        entry.path
                    )));
                }
                seen_dirs_lower.insert(prefix.to_string());
            }
            if is_dir {
                if seen_files_lower.contains(&trimmed_lower) {
                    return Err(EasyOfficeError::SecurityViolation(format!(
                        "ZIP 디렉터리와 기존 파일 경로 충돌: {}",
                        entry.path
                    )));
                }
                seen_dirs_lower.insert(trimmed_lower);
                if entry.compressed_size > 0 || entry.uncompressed_size > 0 {
                    return Err(EasyOfficeError::SecurityViolation(format!(
                        "ZIP 디렉터리 엔트리({})는 데이터를 포함할 수 없습니다.",
                        entry.path
                    )));
                }
            } else {
                if seen_dirs_lower.contains(&trimmed_lower) {
                    return Err(EasyOfficeError::SecurityViolation(format!(
                        "ZIP 파일과 기존 디렉터리 경로 충돌: {}",
                        entry.path
                    )));
                }
                seen_files_lower.insert(trimmed_lower);
            }

            if entry.is_encrypted {
                return Err(EasyOfficeError::EncryptedDocumentUnsupported(
                    DocumentFormat::Docx,
                ));
            }

            if entry.uncompressed_size > policy.max_single_entry_uncompressed_bytes {
                return Err(EasyOfficeError::DecompressionBombDetected {
                    ratio: entry.uncompressed_size as f64 / (entry.compressed_size.max(1) as f64),
                    max_ratio: policy.max_compression_ratio,
                });
            }

            total_uncompressed = total_uncompressed
                .checked_add(entry.uncompressed_size)
                .ok_or(EasyOfficeError::DecompressionBombDetected {
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
    pub const HWP5_FLAG_COMPRESSED: u32 = 0x01;
    pub const HWP5_FLAG_ENCRYPTED: u32 = 0x02;
    pub const HWP5_FLAG_DISTRIBUTION: u32 = 0x04;
}

/// Zero-allocation 64-bit SWAR XML sanitizer blocking XXE / DTD declarations and escaping XML entities.
pub struct XmlSanitizer;

impl XmlSanitizer {
    #[inline]
    pub const fn has_zero_byte(v: u64) -> bool {
        (v.wrapping_sub(0x0101_0101_0101_0101) & !v & 0x8080_8080_8080_8080) != 0
    }

    #[inline]
    pub fn find_byte_swar(bytes: &[u8], start: usize, target: u8) -> Option<usize> {
        const LO: u64 = 0x0101_0101_0101_0101;
        const HI: u64 = 0x8080_8080_8080_8080;
        let splat = (target as u64).wrapping_mul(LO);
        let mut idx = start;
        while idx + 8 <= bytes.len() {
            let word = u64::from_le_bytes(bytes[idx..idx + 8].try_into().unwrap());
            let diff = word ^ splat;
            let mask = diff.wrapping_sub(LO) & !diff & HI;
            if mask != 0 {
                return Some(idx + ((mask.trailing_zeros() >> 3) as usize));
            }
            idx += 8;
        }
        while idx < bytes.len() {
            if bytes[idx] == target {
                return Some(idx);
            }
            idx += 1;
        }
        None
    }

    #[inline]
    fn is_forbidden_xml_ctrl_byte(b: u8) -> bool {
        b < 0x20 && b != b'\t' && b != b'\n' && b != b'\r'
    }

    #[inline]
    fn xml_escape_mask(word: u64) -> u64 {
        const LO: u64 = 0x0101_0101_0101_0101;
        const HI: u64 = 0x8080_8080_8080_8080;
        let d_amp = word ^ (0x26u64 * LO);
        let d_lt = word ^ (0x3Cu64 * LO);
        let d_gt = word ^ (0x3Eu64 * LO);
        let d_quot = word ^ (0x22u64 * LO);
        let d_apos = word ^ (0x27u64 * LO);
        let d_ef = word ^ (0xEFu64 * LO);
        let m_amp = d_amp.wrapping_sub(LO) & !d_amp;
        let m_lt = d_lt.wrapping_sub(LO) & !d_lt;
        let m_gt = d_gt.wrapping_sub(LO) & !d_gt;
        let m_quot = d_quot.wrapping_sub(LO) & !d_quot;
        let m_apos = d_apos.wrapping_sub(LO) & !d_apos;
        let m_ef = d_ef.wrapping_sub(LO) & !d_ef;
        let mut m_ctrl = !(word | (word << 1) | (word << 2)) & HI;
        if m_ctrl != 0 {
            const MASK7F: u64 = 0x7F7F_7F7F_7F7F_7F7F;
            let zero_byte = |d: u64| !(((d & MASK7F).wrapping_add(MASK7F)) | d) & HI;
            let d_tab = word ^ (0x09u64 * LO);
            let d_lf = word ^ (0x0Au64 * LO);
            let d_cr = word ^ (0x0Du64 * LO);
            let m_ws = zero_byte(d_tab) | zero_byte(d_lf) | zero_byte(d_cr);
            m_ctrl &= !m_ws;
        }
        ((m_amp | m_lt | m_gt | m_quot | m_apos | m_ef) & HI) | m_ctrl
    }

    pub fn validate_safe_xml(xml: &str) -> Result<(), EasyOfficeError> {
        let bytes = xml.as_bytes();
        if bytes.len() < 8 {
            return Ok(());
        }
        let mut cursor = 0usize;
        while let Some(lt_idx) = Self::find_byte_swar(bytes, cursor, b'<') {
            if lt_idx + 8 > bytes.len() {
                break;
            }
            if bytes[lt_idx + 1] == b'!' {
                let mut kw = lt_idx + 2;
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
            cursor = lt_idx + 1;
        }
        Ok(())
    }

    pub fn validate_safe_xml_scalar_baseline(xml: &str) -> Result<(), EasyOfficeError> {
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

    #[inline]
    fn handle_escape_hit(
        text: &str,
        bytes: &[u8],
        hit: usize,
        last: &mut usize,
        out: &mut String,
    ) -> usize {
        match bytes[hit] {
            b'&' | b'<' | b'>' | b'"' | b'\'' => {
                if hit > *last {
                    out.push_str(&text[*last..hit]);
                }
                match bytes[hit] {
                    b'&' => out.push_str("&amp;"),
                    b'<' => out.push_str("&lt;"),
                    b'>' => out.push_str("&gt;"),
                    b'"' => out.push_str("&quot;"),
                    b'\'' => out.push_str("&apos;"),
                    _ => unreachable!(),
                }
                *last = hit + 1;
                hit + 1
            }
            b if Self::is_forbidden_xml_ctrl_byte(b) => {
                if hit > *last {
                    out.push_str(&text[*last..hit]);
                }
                *last = hit + 1;
                hit + 1
            }
            0xEF if hit + 3 <= bytes.len()
                && bytes[hit + 1] == 0xBF
                && (bytes[hit + 2] == 0xBE || bytes[hit + 2] == 0xBF) =>
            {
                if hit > *last {
                    out.push_str(&text[*last..hit]);
                }
                *last = hit + 3;
                hit + 3
            }
            _ => hit + 1,
        }
    }

    pub fn escape(text: &str) -> String {
        let bytes = text.as_bytes();
        let mut out = String::with_capacity(bytes.len() + 16);
        let mut last = 0usize;
        let mut idx = 0usize;

        while idx + 8 <= bytes.len() {
            let word = u64::from_le_bytes(bytes[idx..idx + 8].try_into().unwrap());
            let mask = Self::xml_escape_mask(word);
            if mask == 0 {
                idx += 8;
            } else {
                let hit = idx + ((mask.trailing_zeros() >> 3) as usize);
                idx = Self::handle_escape_hit(text, bytes, hit, &mut last, &mut out);
            }
        }

        while idx < bytes.len() {
            idx = Self::handle_escape_hit(text, bytes, idx, &mut last, &mut out);
        }

        if last < bytes.len() {
            out.push_str(&text[last..]);
        }
        out
    }

    pub fn escape_scalar_baseline(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for ch in text.chars() {
            match ch {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                '\'' => out.push_str("&apos;"),
                '\x00'..='\x08' | '\x0B'..='\x0C' | '\x0E'..='\x1F' | '\u{FFFE}' | '\u{FFFF}' => {}
                _ => out.push(ch),
            }
        }
        out
    }

    #[inline]
    pub fn unescape(text: &str) -> String {
        unescape_xml_single_pass(text)
    }
}

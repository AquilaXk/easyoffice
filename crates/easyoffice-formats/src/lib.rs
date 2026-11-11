use std::fmt;

pub mod base64_codec;
mod biff8_formula;
pub mod biff8_xls_codec;
pub mod converter;
pub(crate) mod cp949;
mod cp949_tables;
mod crc32;
pub mod csv_markdown_codecs;
mod decode_budget;
pub mod deflate;
pub mod embedded_thumbnail;
pub mod hwp5_codec;
pub mod hwp_eqedit;
pub mod hwpx_codec;
mod hwpx_parts;
mod legacy_ole2_codec;
pub mod odf_hwpx_codecs;
pub mod office_codecs;
pub mod ole2_codec;
mod ooxml_presentation;
mod ooxml_shared_parts;
mod output_compatibility;
pub mod pages_iwa;
pub mod pages_package;
pub mod pages_proto;
mod pages_schema;
mod pages_table;
mod pages_write;
mod pdf_highlight;
mod pdf_image;
mod pdf_stream_parser;
mod pdf_text_wrap;
mod pdf_writer;
pub mod rich_document;
mod rich_image;
mod rich_image_probe;
mod rich_image_writers;
mod rich_list_writers;
mod rich_read_hwpx;
mod rich_read_images;
mod rich_read_odt;
mod rich_read_ooxml;
mod rich_read_pages;
mod rich_read_pages_image;
mod rich_read_rtf;
mod rich_read_text;
pub mod rich_readers;
mod rich_rtf_table;
mod rich_rtf_writer;
mod rich_run_writers;
mod rich_table;
mod rich_table_writers;
mod rich_writers;
mod rich_xml;
mod rtf_escape;
pub mod rtf_xml_helpers;
mod sheet_styling;
pub mod strict_json;
mod text_decode;
pub mod text_pdf_codecs;
pub mod utf16_swar;
mod xls_sheet_names;
mod xml_stream;
pub mod zip_codec;
mod zip_path_tracker;
mod zip_policy;

pub use converter::*;
pub use crc32::IeeeCrc32;
pub use csv_markdown_codecs::*;
pub use deflate::*;
pub use embedded_thumbnail::{
    check_thumbnail_png, embed_thumbnail, supports_thumbnail, MAX_EMBED_DOCUMENT_BYTES,
    MAX_THUMBNAIL_PNG_BYTES, MAX_THUMBNAIL_SIDE_PX,
};
pub use hwp_eqedit::*;
pub use office_codecs::*;
pub use ole2_codec::*;
pub use output_compatibility::OutputCompatibility;
pub use pages_write::{pages_write_loss, PagesWriteLoss};
pub use pdf_highlight::{PdfHighlight, PdfHighlightKind, MAX_HIGHLIGHT_ANNOTATIONS};
pub use pdf_writer::PdfWriteOptions;
pub use rich_document::{
    evaluate_table_formulas, normalize_hex_color, RichAlignment, RichBlock, RichDocument,
    RichListKind, RichListNumber, RichNumberEnclosure, RichNumberFormat, RichParagraph, RichRun,
    RichScript,
};
pub use rich_image::{
    RichImage, IMAGE_PLACEHOLDER_TEXT, MAX_ALT_TEXT_BYTES, MAX_IMAGES_PER_DOCUMENT,
    MAX_IMAGE_BYTES, MAX_IMAGE_PIXELS, MAX_TOTAL_IMAGE_BYTES,
};
pub use rich_image_probe::RichImageFormat;
pub use rich_readers::{DecodedContent, DecodedDocument, DecodedSlide};
pub use rich_table::{CellMerge, CellSpan, RichTable, SpanMap, MAX_MERGE_SPAN};
pub use rtf_xml_helpers::{
    unescape_xml_five_pass_baseline, unescape_xml_single_pass, RtfCodec, RtfPayload,
};
pub use sheet_styling::*;
pub use strict_json::StrictJsonValue;
pub use text_pdf_codecs::*;
pub use utf16_swar::{
    decode_utf16le_swar, decode_utf16le_vec_baseline, encode_utf16le_scalar_baseline,
    encode_utf16le_swar, encode_utf16le_swar_into, expand_ascii4_to_utf16le_u64, SectorBitset,
};
pub use zip_codec::*;
pub use zip_policy::*;

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
    ZipIwork,
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
            Self::ZipIwork => "ZIP-IWA (Apple iWork)",
            Self::Ole2Cfbf => "OLE2-CFBF",
            Self::PdfIso32000 => "PDF (ISO 32000)",
            Self::PlainTextUtf8 => "UTF-8 Plain Text",
        }
    }
}

/// 20 standard read/write document formats supported by EasyOffice across macOS and Windows,
/// plus [DocumentFormat::Pages], which is read in full and written experimentally (listed in
/// [DocumentFormat::EXPERIMENTAL_WRITE], outside [DocumentFormat::ALL]).
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
    /// Apple Pages (read; written experimentally, body text and basic formats only).
    Pages,
}

impl DocumentFormat {
    /// Formats written experimentally (PD-162): a first, partial writer whose files were not
    /// opened in the app that owns the format. Not part of [`Self::ALL`].
    pub const EXPERIMENTAL_WRITE: [DocumentFormat; 1] = [Self::Pages];

    pub fn is_experimental_write(&self) -> bool {
        Self::EXPERIMENTAL_WRITE.contains(self)
    }

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
            Self::Pages => "pages",
        }
    }

    pub fn container_family(&self) -> ContainerFamily {
        match self {
            Self::Docx | Self::Xlsx | Self::Pptx | Self::Ppsx => ContainerFamily::ZipOoxml,
            Self::Odt | Self::Ods | Self::Odp => ContainerFamily::ZipOdf,
            Self::Hwpx | Self::Hwtx => ContainerFamily::ZipHwpx,
            Self::Pages => ContainerFamily::ZipIwork,
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
            Self::Pages => "Apple Pages (IWA, 쓰기는 실험적)",
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
            "pages" => Some(Self::Pages),
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

    /// Checks whether a header/entry-name marker appears in the Central Directory (or leading
    /// local header window) without scanning the entire compressed payload of large archives.
    #[inline]
    fn zip_header_contains(bytes: &[u8], cd_opt: Option<&[u8]>, needle: &[u8]) -> bool {
        if let Some(cd) = cd_opt {
            let head = &bytes[..bytes.len().min(4096)];
            Self::slice_contains(head, needle) || Self::slice_contains(cd, needle)
        } else {
            Self::slice_contains(bytes, needle)
        }
    }

    fn detect_hwpx_family(bytes: &[u8], cd_opt: Option<&[u8]>) -> DocumentFormat {
        let template_meta = hwpx_codec::HWPX_TEMPLATE_META_MARKER.as_bytes();
        let is_template = Self::zip_header_contains(bytes, cd_opt, b"isTemplate=\"1\"")
            || Self::zip_header_contains(bytes, cd_opt, template_meta)
            || ZipArchiveCodec::any_selected_entry(
                bytes,
                &ZipSecurityPolicy::default(),
                |path| path == "Contents/header.xml" || path == "Contents/content.hpf",
                |e| {
                    (e.path == "Contents/header.xml"
                        && Self::slice_contains(&e.data, b"isTemplate=\"1\""))
                        || (e.path == "Contents/content.hpf"
                            && Self::slice_contains(&e.data, template_meta))
                },
            );
        if is_template {
            DocumentFormat::Hwtx
        } else {
            DocumentFormat::Hwpx
        }
    }

    fn detect_zip_family(bytes: &[u8]) -> DocumentFormat {
        let cd_opt = ZipArchiveCodec::central_dir_slice(bytes);
        let has = |needle: &[u8]| Self::zip_header_contains(bytes, cd_opt, needle);
        if has(b"Contents/header.xml") || has(b"version.xml") {
            return Self::detect_hwpx_family(bytes, cd_opt);
        }
        if has(b"Index/Document.iwa")
            || (has(b"index.xml") && !has(b"[Content_Types].xml") && !has(b"mimetype"))
        {
            return DocumentFormat::Pages;
        }
        const MARKERS: [(&[u8], DocumentFormat); 6] = [
            (b"oasis.opendocument.text", DocumentFormat::Odt),
            (b"oasis.opendocument.spreadsheet", DocumentFormat::Ods),
            (b"oasis.opendocument.presentation", DocumentFormat::Odp),
            (b"xl/workbook.xml", DocumentFormat::Xlsx),
            (b"presentationml.slideshow", DocumentFormat::Ppsx),
            (b"ppt/presentation.xml", DocumentFormat::Pptx),
        ];
        let Some(found) = MARKERS
            .iter()
            .find(|(marker, _)| has(marker))
            .map(|(_, format)| *format)
        else {
            return DocumentFormat::Docx;
        };
        if found != DocumentFormat::Pptx {
            return found;
        }
        let is_slideshow = ZipArchiveCodec::any_selected_entry(
            bytes,
            &ZipSecurityPolicy::default(),
            |path| path == "[Content_Types].xml" || path == "ppt/presentation.xml",
            |e| {
                (e.path == "[Content_Types].xml"
                    && Self::slice_contains(&e.data, b"presentationml.slideshow"))
                    || (e.path == "ppt/presentation.xml"
                        && Self::slice_contains(&e.data, b"slideshow=\"1\""))
            },
        );
        if is_slideshow {
            DocumentFormat::Ppsx
        } else {
            DocumentFormat::Pptx
        }
    }

    fn detect_ole2_family(bytes: &[u8]) -> DocumentFormat {
        const XLS_UTF16: [u8; 16] = [
            b'W', 0, b'o', 0, b'r', 0, b'k', 0, b'b', 0, b'o', 0, b'o', 0, b'k', 0,
        ];
        const PPT_UTF16: [u8; 10] = [b'P', 0, b'o', 0, b'w', 0, b'e', 0, b'r', 0];
        let markers: [(&[u8], DocumentFormat); 4] = [
            (b"HWT_TEMPLATE", DocumentFormat::Hwt),
            (b"HWP Document File", DocumentFormat::Hwp),
            (&XLS_UTF16, DocumentFormat::Xls),
            (&PPT_UTF16, DocumentFormat::Ppt),
        ];
        markers
            .iter()
            .find(|(marker, _)| Self::slice_contains(bytes, marker))
            .map_or(DocumentFormat::Doc, |(_, format)| *format)
    }

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
            return Ok(Self::detect_zip_family(bytes));
        }
        if bytes.starts_with(&Self::OLE2_MAGIC) {
            return Ok(Self::detect_ole2_family(bytes));
        }

        if bytes.starts_with(&Self::PDF_MAGIC) {
            return Ok(DocumentFormat::Pdf);
        }

        if bytes.starts_with(&Self::RTF_MAGIC) {
            Self::validate_without_nul(bytes, "rtf")?;
            return Ok(DocumentFormat::Rtf);
        }

        Self::validate_text_without_nul(bytes, "plain-text")?;
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
            ContainerFamily::ZipOoxml
            | ContainerFamily::ZipOdf
            | ContainerFamily::ZipHwpx
            | ContainerFamily::ZipIwork => {
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
                    Self::validate_without_nul(bytes, format.extension())?;
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
                    Self::validate_text_without_nul(bytes, format.extension())?;
                }
            }
        }
        Ok(())
    }

    fn validate_without_nul(bytes: &[u8], label: &str) -> Result<(), EasyOfficeError> {
        if XmlSanitizer::find_byte_swar(bytes, 0, 0x00).is_some() {
            return Err(EasyOfficeError::InvalidUtf8(format!(
                "{label} 스트림에 NUL(0x00) 바이트가 포함되어 있습니다."
            )));
        }
        Ok(())
    }

    /// Text inputs may be UTF-8, UTF-16 with a BOM or CP949 (see `text_decode`); the decoded text
    /// must not contain NUL.
    fn validate_text_without_nul(bytes: &[u8], label: &str) -> Result<(), EasyOfficeError> {
        let text = crate::text_decode::decode_text(bytes, label)?;
        Self::validate_without_nul(text.as_bytes(), label)
    }

    fn slice_contains(haystack: &[u8], needle: &[u8]) -> bool {
        let Some(&first) = needle.first() else {
            return false;
        };
        let mut pos = 0;
        while let Some(hit) = XmlSanitizer::find_byte_swar(haystack, pos, first) {
            if haystack.get(hit..hit + needle.len()) == Some(needle) {
                return true;
            }
            pos = hit + 1;
        }
        false
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
    pub const HWP5_SIGNATURE: &[u8] = b"HWP Document File";
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
        let z = |b: u8| {
            let d = word ^ ((b as u64) * LO);
            d.wrapping_sub(LO) & !d
        };
        let mut m_ctrl = !(word | (word << 1) | (word << 2)) & HI;
        if m_ctrl != 0 {
            const MASK7F: u64 = 0x7F7F_7F7F_7F7F_7F7F;
            let zb = |b: u8| {
                let d = word ^ ((b as u64) * LO);
                !(((d & MASK7F).wrapping_add(MASK7F)) | d) & HI
            };
            m_ctrl &= !(zb(0x09) | zb(0x0A) | zb(0x0D));
        }
        ((z(0x26) | z(0x3C) | z(0x3E) | z(0x22) | z(0x27) | z(0xEF)) & HI) | m_ctrl
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

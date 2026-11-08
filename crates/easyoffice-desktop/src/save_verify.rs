//! Checks a document's bytes before they may replace a file (PD-109, #227).
//!
//! The page builds the bytes; the shell does not trust them. A save that would put a truncated or
//! mislabelled document where a good one used to be is refused before any file is touched
//! (Fail-Closed). The reference implementation (MIT, Copyright (c) 2025-2026 Edward Kim; see
//! THIRD_PARTY_NOTICES.md; `apps/desktop/src-tauri/src/state.rs` lines 189-215) re-parses the
//! bytes with its native Hangul core before committing. This shell has no such core
//! (the Hangul editor runs inside the page), so the check is the engine's own structure parsers:
//!
//! * every format: the signature for the file extension (`FormatDetector::detect` with the extension
//!   as hint; text formats must be UTF-8 without NUL);
//! * ZIP formats (OOXML, ODF, HWPX): the full central directory and local header walk with CRC,
//!   size, ratio and path checks of `ZipArchiveCodec::unpack`, plus the format's main part;
//! * OLE2 formats (HWP, DOC, XLS, PPT): the full container walk of `Ole2ContainerCodec::unpack`;
//!   HWP also needs a `FileHeader` stream that starts with the HWP signature;
//! * PDF: the header, a `startxref` and the `%%EOF` marker within the last kilobyte.
//!
//! Hangul documents are checked at container level only: decoding their records needs rhwp, so a
//! record-level fault in the page's HWP/HWPX output is not caught here.

use easyoffice_formats::{
    ContainerFamily, DocumentFormat, EasyOfficeError, FormatDetector, Ole2ContainerCodec,
    Ole2SecurityGuard, ZipArchiveCodec, ZipSecurityPolicy,
};

/// What the user is told when the bytes are refused: no internal terms, and the file is untouched.
pub const REFUSED_MESSAGE: &str = "문서 내용이 올바르지 않아 저장하지 않았습니다. 기존 파일은 그대로 두었습니다. 다시 저장해 보세요.";

/// The PDF end-of-file marker must sit in this many trailing bytes (ISO 32000 asks for 1024).
const PDF_TAIL_BYTES: usize = 1024;

/// Entry that identifies each ZIP format; a package missing it is some other kind of package.
/// HWPX only needs something under `Contents/` because the Hangul editor chooses its file names.
const ZIP_MAIN_PARTS: [(DocumentFormat, &str); 9] = [
    (DocumentFormat::Docx, "word/document.xml"),
    (DocumentFormat::Xlsx, "xl/workbook.xml"),
    (DocumentFormat::Pptx, "ppt/presentation.xml"),
    (DocumentFormat::Ppsx, "ppt/presentation.xml"),
    (DocumentFormat::Odt, "content.xml"),
    (DocumentFormat::Ods, "content.xml"),
    (DocumentFormat::Odp, "content.xml"),
    (DocumentFormat::Hwpx, "Contents/"),
    (DocumentFormat::Hwtx, "Contents/"),
];

const HWP_FILE_HEADER_STREAM: &str = "FileHeader";

/// Refuses bytes that are not a complete document of the format the extension names.
pub fn verify_document_bytes(extension: &str, bytes: &[u8]) -> Result<(), EasyOfficeError> {
    let format = DocumentFormat::from_extension(extension)
        .filter(|format| !format.is_read_only())
        .ok_or_else(|| EasyOfficeError::UnsupportedExtension(extension.to_string()))?;
    FormatDetector::detect(bytes, Some(extension))?;
    match format.container_family() {
        ContainerFamily::ZipOoxml | ContainerFamily::ZipOdf | ContainerFamily::ZipHwpx => {
            verify_zip(format, bytes)
        }
        ContainerFamily::Ole2Cfbf => verify_ole2(format, bytes),
        ContainerFamily::PdfIso32000 => verify_pdf_tail(bytes),
        ContainerFamily::PlainTextUtf8 => Ok(()),
        ContainerFamily::ZipIwork => {
            Err(EasyOfficeError::UnsupportedExtension(extension.to_string()))
        }
    }
}

fn corrupted(detail: &str) -> EasyOfficeError {
    EasyOfficeError::CorruptedContainer(detail.to_string())
}

fn verify_zip(format: DocumentFormat, bytes: &[u8]) -> Result<(), EasyOfficeError> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())?;
    let main_part = ZIP_MAIN_PARTS
        .iter()
        .find(|(candidate, _)| *candidate == format)
        .map(|(_, part)| *part)
        .ok_or_else(|| EasyOfficeError::UnsupportedExtension(format.extension().to_string()))?;
    let present = entries.iter().any(|entry| {
        if main_part.ends_with('/') {
            entry.path.starts_with(main_part)
        } else {
            entry.path == main_part
        }
    });
    if present {
        Ok(())
    } else {
        Err(corrupted("문서의 주 구성 요소가 없습니다."))
    }
}

fn verify_ole2(format: DocumentFormat, bytes: &[u8]) -> Result<(), EasyOfficeError> {
    let streams = Ole2ContainerCodec::unpack(bytes)?;
    if streams.is_empty() {
        return Err(corrupted("문서 스트림이 없습니다."));
    }
    if !matches!(format, DocumentFormat::Hwp | DocumentFormat::Hwt) {
        return Ok(());
    }
    let header_ok = streams
        .iter()
        .find(|stream| stream.name == HWP_FILE_HEADER_STREAM)
        .is_some_and(|stream| {
            stream.data.len() >= Ole2SecurityGuard::HWP5_FILE_HEADER_LEN
                && stream.data.starts_with(Ole2SecurityGuard::HWP5_SIGNATURE)
        });
    if header_ok {
        Ok(())
    } else {
        Err(corrupted("한글 문서 머리 정보가 없습니다."))
    }
}

fn verify_pdf_tail(bytes: &[u8]) -> Result<(), EasyOfficeError> {
    let tail = &bytes[bytes.len().saturating_sub(PDF_TAIL_BYTES)..];
    let has = |needle: &[u8]| tail.windows(needle.len()).any(|window| window == needle);
    if has(b"startxref") && has(b"%%EOF") {
        Ok(())
    } else {
        Err(corrupted("PDF 끝 표시가 없습니다."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use easyoffice_formats::{UniversalDocumentModel, UniversalFormatConverter};

    fn sample_model() -> UniversalDocumentModel {
        UniversalDocumentModel {
            title: "검증 문서".to_string(),
            paragraphs: vec!["첫 문단".to_string(), "둘째 문단".to_string()],
            tables: vec![vec![vec!["a".to_string(), "b".to_string()]]],
            sheet_cells: vec![(0, 0, "x".to_string()), (1, 1, "=1+1".to_string())],
            slides: vec![(
                "제목".to_string(),
                vec!["항목".to_string()],
                "노트".to_string(),
            )],
            official_agency: None,
        }
    }

    fn encoded(format: DocumentFormat) -> Vec<u8> {
        UniversalFormatConverter::encode(&sample_model(), format).unwrap()
    }

    #[test]
    fn every_format_the_engine_writes_passes() {
        for format in DocumentFormat::ALL {
            let bytes = encoded(format);
            verify_document_bytes(format.extension(), &bytes)
                .unwrap_or_else(|err| panic!(".{} refused: {err}", format.extension()));
        }
    }

    #[test]
    fn zip_variants_of_an_extension_pass() {
        let docx = encoded(DocumentFormat::Docx);
        for extension in ["DOCX", "docm", "dotx"] {
            assert!(
                verify_document_bytes(extension, &docx).is_ok(),
                "{extension}"
            );
        }
    }

    #[test]
    fn unknown_and_read_only_extensions_are_refused() {
        assert!(verify_document_bytes("xyz", b"text").is_err());
        assert!(verify_document_bytes("pages", &encoded(DocumentFormat::Docx)).is_err());
        assert!(verify_document_bytes("docx", b"").is_err());
    }

    #[test]
    fn a_package_of_another_kind_is_refused() {
        let xlsx = encoded(DocumentFormat::Xlsx);
        assert!(verify_document_bytes("docx", &xlsx).is_err());
        assert!(verify_document_bytes("odt", &xlsx).is_err());
        assert!(verify_document_bytes("hwpx", &xlsx).is_err());
        assert!(verify_document_bytes("hwp", &xlsx).is_err());
        assert!(verify_document_bytes("pdf", &xlsx).is_err());
        assert!(verify_document_bytes("txt", &xlsx).is_err());
    }

    #[test]
    fn truncated_containers_are_refused() {
        for format in [
            DocumentFormat::Docx,
            DocumentFormat::Hwpx,
            DocumentFormat::Odt,
            DocumentFormat::Hwp,
            DocumentFormat::Xls,
            DocumentFormat::Pdf,
        ] {
            let bytes = encoded(format);
            // A PDF may lack its final line break, so its last cut is further in.
            for cut in [bytes.len() - 16, bytes.len() / 2, 64, 8] {
                assert!(
                    verify_document_bytes(format.extension(), &bytes[..cut]).is_err(),
                    ".{} cut to {cut} of {}",
                    format.extension(),
                    bytes.len()
                );
            }
        }
    }

    #[test]
    fn a_false_signature_with_no_structure_is_refused() {
        assert!(verify_document_bytes("docx", b"PK\x03\x04 and then nothing useful").is_err());
        assert!(
            verify_document_bytes("hwp", &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1])
                .is_err()
        );
        assert!(verify_document_bytes("pdf", b"%PDF-1.7 no trailer").is_err());
        assert!(verify_document_bytes("rtf", b"plain text, not rich").is_err());
    }

    #[test]
    fn a_declared_size_far_beyond_the_data_is_refused() {
        let mut bytes = encoded(DocumentFormat::Docx);
        // Central directory entry: uncompressed size field (offset 24 of the 0x02014B50 record).
        let cd = bytes
            .windows(4)
            .position(|w| w == [0x50, 0x4B, 0x01, 0x02])
            .unwrap();
        bytes[cd + 24..cd + 28].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(verify_document_bytes("docx", &bytes).is_err());
    }

    #[test]
    fn a_declared_container_size_far_beyond_the_data_is_refused() {
        let mut bytes = encoded(DocumentFormat::Hwp);
        // OLE2 header: number of FAT sectors (offset 44) claims millions of sectors.
        bytes[44..48].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(verify_document_bytes("hwp", &bytes).is_err());
    }

    #[test]
    fn a_hangul_container_without_its_header_stream_is_refused() {
        let streams = [("BodyText/Section0".to_string(), vec![0_u8; 16])];
        let bytes = Ole2ContainerCodec::pack(&streams).unwrap();
        assert!(verify_document_bytes("hwp", &bytes).is_err());
        // The same container is a fine legacy document of another kind.
        assert!(verify_document_bytes("doc", &bytes).is_ok());
    }

    #[test]
    fn text_formats_need_valid_utf8_without_nul() {
        assert!(verify_document_bytes("txt", "한글 text".as_bytes()).is_ok());
        assert!(verify_document_bytes("csv", b"a,b\r\n1,2\r\n").is_ok());
        assert!(verify_document_bytes("txt", &[0xFF, 0xFE, 0x41]).is_err());
        assert!(verify_document_bytes("md", b"# title\0hidden").is_err());
    }
}

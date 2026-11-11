//! Checks a document's bytes before they may replace a file (PD-109).
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
//!   size, ratio and path checks of `ZipArchiveCodec::verify` (one entry in memory at a time),
//!   plus the format's main part;
//! * OLE2 formats (HWP, DOC, XLS, PPT): the full container walk of `Ole2ContainerCodec::unpack`;
//!   HWP also needs a `FileHeader` stream that starts with the HWP signature;
//! * PDF: the header, a `startxref` and the `%%EOF` marker within the last kilobyte.
//!
//! * Pages (experimental writer, PD-162): the ZIP walk and the engine's own Pages reader, which must
//!   open the bytes as a document. This is circular: the reader is the engine's, not the app's, so
//!   it shows that the next EasyOffice open accepts the file, not that Pages does.
//!
//! Hangul documents are checked at container level only: decoding their records needs rhwp, so a
//! record-level fault in the page's HWP/HWPX output is not caught here.

use easyoffice_formats::{
    ContainerFamily, DecodedContent, DocumentFormat, EasyOfficeError, FormatDetector,
    Ole2ContainerCodec, Ole2SecurityGuard, UniversalFormatConverter, ZipArchiveCodec,
    ZipSecurityPolicy,
};

/// What the user is told when the bytes are refused: no internal terms, and the file is untouched.
pub const REFUSED_MESSAGE: &str =
    "문서 형식을 확인하지 못해 저장하지 않았습니다. 기존 파일은 그대로 두었습니다.";

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
        .ok_or_else(|| EasyOfficeError::UnsupportedExtension(extension.to_string()))?;
    FormatDetector::detect(bytes, Some(extension))?;
    match format.container_family() {
        ContainerFamily::ZipOoxml | ContainerFamily::ZipOdf | ContainerFamily::ZipHwpx => {
            verify_zip(format, bytes)
        }
        ContainerFamily::Ole2Cfbf => verify_ole2(format, bytes),
        ContainerFamily::PdfIso32000 => verify_pdf_tail(bytes),
        ContainerFamily::PlainTextUtf8 => Ok(()),
        ContainerFamily::ZipIwork => verify_pages(bytes),
    }
}

/// A Pages package passes when its ZIP is sound and the engine's reader opens it as a document.
fn verify_pages(bytes: &[u8]) -> Result<(), EasyOfficeError> {
    ZipArchiveCodec::verify(bytes, &ZipSecurityPolicy::default())?;
    let read = UniversalFormatConverter::decode_rich(bytes, Some("pages"))?;
    match read.content {
        DecodedContent::Document(_) if read.format == DocumentFormat::Pages => Ok(()),
        _ => Err(corrupted("Pages 문서 본문을 읽지 못했습니다.")),
    }
}

fn corrupted(detail: &str) -> EasyOfficeError {
    EasyOfficeError::CorruptedContainer(detail.to_string())
}

/// The policy the next reader applies: the engine's default for every format the engine opens, the
/// Hangul editor's own for the Hangul formats. That one has no ratio rule: the next reader of a
/// saved HWPX is the editor's rhwp, which has none, and the page has already re-parsed these exact
/// bytes with it before they get here (`exportVerified` in `hwp-session.js`, on every save path). A
/// flat picture the editor embeds legitimately deflates past 1000:1, so any number would refuse
/// real documents. Its size limits are the reader's too (PD-112): text parts 256 MiB each and
/// 512 MiB together, pictures 512 MiB each and, since the reader has no total for them, a total
/// that only bounds this check's own work.
fn zip_policy(format: DocumentFormat) -> ZipSecurityPolicy {
    match format {
        DocumentFormat::Hwpx | DocumentFormat::Hwtx => ZipSecurityPolicy::hangul_editor(),
        _ => ZipSecurityPolicy::default(),
    }
}

fn verify_zip(format: DocumentFormat, bytes: &[u8]) -> Result<(), EasyOfficeError> {
    let paths = ZipArchiveCodec::verify(bytes, &zip_policy(format))?;
    let main_part = ZIP_MAIN_PARTS
        .iter()
        .find(|(candidate, _)| *candidate == format)
        .map(|(_, part)| *part)
        .ok_or_else(|| EasyOfficeError::UnsupportedExtension(format.extension().to_string()))?;
    let present = paths.iter().any(|path| {
        if main_part.ends_with('/') {
            path.starts_with(main_part)
        } else {
            path == main_part
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
    let mut has_xref = false;
    let mut has_eof = false;
    for i in 0..tail.len() {
        if !has_xref && tail[i..].starts_with(b"startxref") {
            has_xref = true;
        } else if !has_eof && tail[i..].starts_with(b"%%EOF") {
            has_eof = true;
        }
        if has_xref && has_eof {
            return Ok(());
        }
    }
    Err(corrupted("PDF 끝 표시가 없습니다."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use easyoffice_formats::{
        UniversalDocumentModel, UniversalFormatConverter, ZipArchiveEntry, ZipEntryDescriptor,
        ZipSecurityGuard, HANGUL_EDITOR_MAX_ENTRY_BYTES as HANGUL_PICTURE_LIMIT,
    };

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
    fn unknown_extensions_are_refused() {
        assert!(verify_document_bytes("xyz", b"text").is_err());
        assert!(verify_document_bytes("docx", b"").is_err());
    }

    fn pages_bytes() -> Vec<u8> {
        let doc = easyoffice_formats::RichDocument {
            title: "t".to_string(),
            blocks: vec![easyoffice_formats::RichBlock::Paragraph(
                easyoffice_formats::RichParagraph {
                    runs: vec![easyoffice_formats::RichRun::plain("본문")],
                    ..Default::default()
                },
            )],
            official_agency: None,
        };
        UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Pages, None).unwrap()
    }

    #[test]
    fn a_pages_package_the_engine_wrote_passes_and_anything_else_under_that_name_is_refused() {
        let pages = pages_bytes();
        verify_document_bytes("pages", &pages).unwrap();
        verify_document_bytes("PAGES", &pages).unwrap();
        assert!(verify_document_bytes("pages", &encoded(DocumentFormat::Docx)).is_err());
        assert!(verify_document_bytes("docx", &pages).is_err());
        for cut in [pages.len() - 16, pages.len() / 2, 64, 8] {
            assert!(
                verify_document_bytes("pages", &pages[..cut]).is_err(),
                "cut {cut}"
            );
        }
        // A sound ZIP that holds the marker part but no readable document.
        let hollow =
            ZipArchiveCodec::pack(&[("Index/Document.iwa".to_string(), vec![0, 1, 0, 0, 7])])
                .unwrap();
        assert!(verify_document_bytes("pages", &hollow).is_err());
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

    /// What the Hangul editor writes for a flat 3000x2000 24-bit picture: one entry of 18 MB that
    /// deflates by more than 1000:1 (1025.6 measured on the editor's own output).
    fn flat_picture_package(main_part: &str) -> Vec<u8> {
        let entries = vec![
            ZipArchiveEntry::new("mimetype", b"application/hwp+zip".to_vec()),
            ZipArchiveEntry::new(main_part, b"<head/>".to_vec()),
            ZipArchiveEntry::new("BinData/image1.bmp", vec![0xFF_u8; 18_000_054]),
        ];
        ZipArchiveCodec::pack_deflate(&entries).unwrap()
    }

    #[test]
    fn a_hangul_package_with_a_flat_picture_passes_without_a_ratio_rule() {
        let bytes = flat_picture_package("Contents/header.xml");
        let ratio = 18_000_054.0 / bytes.len() as f64;
        assert!(ratio > 1000.0, "fixture is a high-ratio package: {ratio}");
        // The engine's open policy would call it a bomb; the Hangul editor opens it.
        assert!(matches!(
            ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()),
            Err(EasyOfficeError::DecompressionBombDetected { .. })
        ));
        verify_document_bytes("hwpx", &bytes).unwrap();
        verify_document_bytes("hwtx", &bytes).unwrap();
    }

    #[test]
    fn formats_the_engine_opens_keep_the_open_policy() {
        // The same kind of package under an extension the engine opens is held to its policy...
        let bytes = flat_picture_package("word/document.xml");
        assert!(verify_document_bytes("docx", &bytes).is_err());
        // ...and the repetitive documents the engine itself writes stay within it, because its
        // writer stores an entry that would compress past the limit.
        let repetitive = UniversalDocumentModel {
            title: "반복".to_string(),
            paragraphs: vec!["같은 문장이 계속 반복됩니다. ".repeat(20); 20_000],
            sheet_cells: (0..20_000).map(|r| (r, 0, "same".to_string())).collect(),
            slides: vec![(
                "제목".to_string(),
                vec!["항목".to_string(); 5_000],
                String::new(),
            )],
            ..Default::default()
        };
        for format in [
            DocumentFormat::Docx,
            DocumentFormat::Odt,
            DocumentFormat::Xlsx,
            DocumentFormat::Ods,
            DocumentFormat::Pptx,
            DocumentFormat::Odp,
            DocumentFormat::Hwpx,
        ] {
            let bytes = UniversalFormatConverter::encode(&repetitive, format).unwrap();
            verify_document_bytes(format.extension(), &bytes)
                .unwrap_or_else(|err| panic!(".{} refused: {err}", format.extension()));
        }
    }

    #[test]
    fn the_size_limits_still_apply_to_hangul_packages() {
        let mut bytes = flat_picture_package("Contents/header.xml");
        // Declare the picture 600 MiB large in its central directory record: over the editor's limit.
        let mut at = bytes
            .windows(4)
            .position(|w| w == [0x50, 0x4B, 0x01, 0x02])
            .unwrap();
        while &bytes[at + 46..at + 46 + 18] != b"BinData/image1.bmp" {
            at += 46 + usize::from(u16::from_le_bytes([bytes[at + 28], bytes[at + 29]]));
        }
        bytes[at + 24..at + 28].copy_from_slice(&(600_u32 * 1_048_576).to_le_bytes());
        assert!(matches!(
            verify_document_bytes("hwpx", &bytes),
            Err(EasyOfficeError::DecompressionBombDetected { .. })
        ));
    }

    const MIB: u64 = 1_048_576;
    const PICTURE_NAMES: [&str; 10] = [
        "BinData/image0.bmp",
        "BinData/image1.bmp",
        "BinData/image2.bmp",
        "BinData/image3.bmp",
        "BinData/image4.bmp",
        "BinData/image5.bmp",
        "BinData/image6.bmp",
        "BinData/image7.bmp",
        "BinData/image8.bmp",
        "BinData/image9.bmp",
    ];

    /// Verdict of the Hangul policy on a package declared as these `(path, size)` parts (stored,
    /// so the compressed size is the size), without building the data.
    fn hangul_verdict(parts: &[(&str, u64)]) -> Result<(), EasyOfficeError> {
        let descriptors: Vec<_> = parts
            .iter()
            .map(|(path, size)| ZipEntryDescriptor::new(*path, *size, *size, false))
            .collect();
        ZipSecurityGuard::validate_archive(&descriptors, &zip_policy(DocumentFormat::Hwpx))
    }

    fn is_bomb(verdict: Result<(), EasyOfficeError>) -> bool {
        matches!(
            verdict,
            Err(EasyOfficeError::DecompressionBombDetected { .. })
        )
    }

    #[test]
    fn the_hangul_picture_limit_is_the_editors_not_the_engines() {
        let engine = 128 * MIB;
        let picture = |size: u64| hangul_verdict(&[("BinData/image1.bmp", size)]);
        // Over the engine's default but within what the editor opens: kept.
        assert!(picture(engine + 1).is_ok());
        assert!(picture(HANGUL_PICTURE_LIMIT).is_ok());
        let docx = vec![ZipEntryDescriptor::new(
            "BinData/image1.bmp",
            engine + 1,
            engine + 1,
            false,
        )];
        assert!(
            ZipSecurityGuard::validate_archive(&docx, &zip_policy(DocumentFormat::Docx)).is_err()
        );
        // Past the editor's limit: refused.
        assert!(is_bomb(picture(HANGUL_PICTURE_LIMIT + 1)));
    }

    #[test]
    fn hangul_text_parts_are_limited_like_the_editors_reader() {
        let xml = |size: u64| hangul_verdict(&[("Contents/section0.xml", size)]);
        assert!(xml(256 * MIB).is_ok());
        assert!(is_bomb(xml(256 * MIB + 1)));
        // 256 + 256 MiB of text is the editor's total; one byte more is refused.
        let section = |size| ("Contents/section0.xml", size);
        let header = |size| ("Contents/header.xml", size);
        assert!(hangul_verdict(&[section(256 * MIB), header(256 * MIB)]).is_ok());
        assert!(is_bomb(hangul_verdict(&[
            section(256 * MIB),
            header(256 * MIB),
            ("Contents/content.hpf", 1),
        ])));
        // The OPF package file counts as text whatever the case of its name.
        assert!(is_bomb(hangul_verdict(&[
            section(256 * MIB),
            ("Contents/content.HPF", 256 * MIB),
            ("version.xml", 1),
        ])));
    }

    #[test]
    fn hangul_pictures_beyond_the_old_total_pass_when_the_editor_opens_them() {
        // Ten 100 MiB pictures: 1000 MiB of picture data, almost twice the old 512 MiB total.
        // The editor inflates a picture only when a page needs it, so it opens this package.
        let mut parts = vec![("Contents/header.xml", 1_u64), ("Contents/section0.xml", 1)];
        parts.extend(PICTURE_NAMES.iter().map(|name| (*name, 100 * MIB)));
        assert!(hangul_verdict(&parts).is_ok());
        // Eight pictures of the editor's per-part limit are the verification's whole budget.
        let eight: Vec<_> = PICTURE_NAMES[..8]
            .iter()
            .map(|name| (*name, HANGUL_PICTURE_LIMIT))
            .collect();
        assert!(hangul_verdict(&eight).is_ok());
        let mut nine = eight;
        nine.push((PICTURE_NAMES[8], 1));
        assert!(is_bomb(hangul_verdict(&nine)));
    }

    #[test]
    fn pictures_and_text_have_separate_budgets() {
        let section = ("Contents/section0.xml", 256 * MIB);
        let header = ("Contents/header.xml", 256 * MIB);
        let picture = |index: usize| (PICTURE_NAMES[index], HANGUL_PICTURE_LIMIT);
        assert!(hangul_verdict(&[section, header, picture(0), picture(1), picture(2)]).is_ok());
    }

    #[test]
    fn a_picture_bomb_is_refused_by_its_declared_sizes_before_anything_is_inflated() {
        // A few KiB of data declared as many gigabytes of pictures: over the budget by size alone.
        let descriptors: Vec<_> = (0..9)
            .map(|index| {
                ZipEntryDescriptor::new(
                    format!("BinData/bomb{index}.bmp"),
                    500_000,
                    HANGUL_PICTURE_LIMIT,
                    false,
                )
            })
            .collect();
        assert!(is_bomb(ZipSecurityGuard::validate_archive(
            &descriptors,
            &zip_policy(DocumentFormat::Hwpx)
        )));
        // A real package: one picture whose data inflates past the size its header declares.
        let mut bytes = flat_picture_package("Contents/header.xml");
        let mut at = bytes
            .windows(4)
            .position(|w| w == [0x50, 0x4B, 0x01, 0x02])
            .unwrap();
        while &bytes[at + 46..at + 46 + 18] != b"BinData/image1.bmp" {
            at += 46 + usize::from(u16::from_le_bytes([bytes[at + 28], bytes[at + 29]]));
        }
        bytes[at + 24..at + 28].copy_from_slice(&1_000_u32.to_le_bytes());
        assert!(verify_document_bytes("hwpx", &bytes).is_err());
    }

    /// The stored or deflate stream of some data with its CRC-32 and size, to be written into a package any
    /// number of times: `ZipArchiveCodec::pack_deflate` refuses a sum over its own 512 MiB policy.
    struct Stream {
        method: u16,
        deflated: Vec<u8>,
        crc: u32,
        size: u32,
    }

    impl Stream {
        fn of(data: &[u8]) -> Self {
            // Take it from a one-entry package: CRC at 14, compressed size at 18, data at 31.
            let single = ZipArchiveCodec::pack_deflate(&[("p", data)]).unwrap();
            let word = |at: usize| u32::from_le_bytes(single[at..at + 4].try_into().unwrap());
            Self {
                method: u16::from_le_bytes([single[8], single[9]]),
                deflated: single[31..31 + word(18) as usize].to_vec(),
                crc: word(14),
                size: data.len() as u32,
            }
        }
    }

    fn package_of_streams(parts: &[(&str, &Stream)]) -> Vec<u8> {
        let (mut out, mut directory) = (Vec::new(), Vec::new());
        for (name, stream) in parts {
            let offset = out.len() as u32;
            let header = |signature: u32, central: bool| {
                let mut h = signature.to_le_bytes().to_vec();
                if central {
                    h.extend_from_slice(&20_u16.to_le_bytes());
                }
                h.extend_from_slice(&20_u16.to_le_bytes());
                h.extend_from_slice(&[0, 0]);
                h.extend_from_slice(&stream.method.to_le_bytes());
                h.extend_from_slice(&[0, 0, 0x21, 0]);
                h.extend_from_slice(&stream.crc.to_le_bytes());
                h.extend_from_slice(&(stream.deflated.len() as u32).to_le_bytes());
                h.extend_from_slice(&stream.size.to_le_bytes());
                h.extend_from_slice(&(name.len() as u16).to_le_bytes());
                h
            };
            out.extend(header(ZipArchiveCodec::LOCAL_FILE_HEADER_SIG, false));
            out.extend_from_slice(&[0, 0]);
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&stream.deflated);
            directory.extend(header(ZipArchiveCodec::CENTRAL_DIR_HEADER_SIG, true));
            directory.extend_from_slice(&[0; 12]);
            directory.extend_from_slice(&offset.to_le_bytes());
            directory.extend_from_slice(name.as_bytes());
        }
        let count = (parts.len() as u16).to_le_bytes();
        let offset = out.len() as u32;
        out.extend_from_slice(&directory);
        out.extend_from_slice(&ZipArchiveCodec::END_OF_CENTRAL_DIR_SIG.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&count);
        out.extend_from_slice(&count);
        out.extend_from_slice(&(directory.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out
    }

    #[test]
    fn a_real_package_with_more_picture_data_than_the_old_total_is_verified() {
        // Six flat 100 MiB pictures (600 MiB, over the old 512 MiB total) plus a text part.
        // Verification inflates one picture at a time and keeps none of them.
        let picture = Stream::of(&vec![0xAB_u8; 100 * MIB as usize]);
        let text = Stream::of(b"<head/>");
        let mut parts = vec![("Contents/header.xml", &text)];
        parts.extend(PICTURE_NAMES[..6].iter().map(|name| (*name, &picture)));
        let bytes = package_of_streams(&parts);
        assert!(bytes.len() < MIB as usize);
        verify_document_bytes("hwpx", &bytes).unwrap();
        // The old policy (one shared 512 MiB total) refused exactly this package.
        let old = ZipSecurityPolicy {
            max_single_entry_uncompressed_bytes: HANGUL_PICTURE_LIMIT,
            max_compression_ratio: f64::INFINITY,
            ..ZipSecurityPolicy::default()
        };
        assert!(ZipArchiveCodec::verify(&bytes, &old).is_err());
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

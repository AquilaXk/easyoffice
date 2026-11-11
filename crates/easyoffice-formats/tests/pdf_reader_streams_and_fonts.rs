//! PDF reader: opaque binary streams (images, font programs) must not consume the text decode
//! budget, and text decoding must follow the page's own font resources.
use easyoffice_formats::{deflate_zlib, EasyOfficeError, PdfSerializer};

const MIB: usize = 1024 * 1024;

fn stream_obj(num: u32, dict: &str, data: &[u8]) -> Vec<u8> {
    let mut out = format!("{num} 0 obj << {dict} /Length {} >>\nstream\n", data.len()).into_bytes();
    out.extend_from_slice(data);
    out.extend_from_slice(b"\nendstream\nendobj\n");
    out
}

/// One page (object 3) with content `content`, font resources `fonts`, then extra objects.
fn single_page_pdf(content: &str, fonts: &str, extra: &[Vec<u8>]) -> Vec<u8> {
    let mut raw = b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n".to_vec();
    raw.extend_from_slice(b"2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n");
    raw.extend_from_slice(
        format!("3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /Font << {fonts} >> >> >> endobj\n")
            .as_bytes(),
    );
    raw.extend_from_slice(&stream_obj(4, "", content.as_bytes()));
    for obj in extra {
        raw.extend_from_slice(obj);
    }
    raw.extend_from_slice(b"xref\n0 1\ntrailer << /Root 1 0 R /Size 20 >>\nstartxref\n0\n%%EOF\n");
    raw
}

fn cmap(pairs: &str) -> Vec<u8> {
    format!("begincmap\n1 begincodespacerange <00> <FF> endcodespacerange\nbeginbfchar\n{pairs}\nendbfchar\nendcmap\n")
        .into_bytes()
}

#[test]
fn flate_image_and_font_program_do_not_consume_the_text_budget() {
    let huge = deflate_zlib(&vec![0u8; 40 * MIB]);
    let image = stream_obj(
        10,
        "/Type /XObject /Subtype /Image /Width 6000 /Height 7000 /BitsPerComponent 8 /ColorSpace /DeviceGray /Filter /FlateDecode",
        &huge,
    );
    let font_program = stream_obj(11, "/Length1 40000000 /Filter /FlateDecode", &huge);
    let jpeg = stream_obj(
        12,
        "/Type /XObject /Subtype /Image /Width 8 /Height 8 /BitsPerComponent 8 /ColorSpace /DeviceRGB /Filter /DCTDecode",
        &[0xFF, 0xD8, 0xFF, 0xE0, 0, 0],
    );
    let pdf = single_page_pdf(
        "BT /F1 12 Tf 72 720 Td (scan text) Tj ET",
        "",
        &[image, font_program, jpeg],
    );
    let parsed = PdfSerializer::parse(&pdf).expect("images and font programs are opaque");
    assert_eq!(parsed.pages[0].body_text, "scan text");
}

#[test]
fn image_with_absurd_declared_size_is_rejected() {
    let image = stream_obj(
        10,
        "/Type /XObject /Subtype /Image /Width 65535 /Height 65535 /BitsPerComponent 16 /ColorSpace /DeviceRGB /Filter /FlateDecode",
        &deflate_zlib(&[0u8; 64]),
    );
    let pdf = single_page_pdf("BT (x) Tj ET", "", &[image]);
    assert!(matches!(
        PdfSerializer::parse(&pdf),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn flate_content_stream_still_hits_the_budget() {
    let content = deflate_zlib(&vec![b' '; 40 * MIB]);
    let mut raw = b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n".to_vec();
    raw.extend_from_slice(b"2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n");
    raw.extend_from_slice(b"3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R >> endobj\n");
    raw.extend_from_slice(&stream_obj(4, "/Filter /FlateDecode", &content));
    raw.extend_from_slice(b"xref\n0 1\ntrailer << /Root 1 0 R >>\nstartxref\n0\n%%EOF\n");
    assert!(matches!(
        PdfSerializer::parse(&raw),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn to_unicode_maps_are_scoped_to_their_font_resource() {
    let mut raw = b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n".to_vec();
    raw.extend_from_slice(b"2 0 obj << /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >> endobj\n");
    raw.extend_from_slice(b"3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /Font << /F1 10 0 R >> >> >> endobj\n");
    raw.extend_from_slice(&stream_obj(4, "", b"BT /F1 12 Tf 72 720 Td <01> Tj ET"));
    raw.extend_from_slice(b"5 0 obj << /Type /Page /Parent 2 0 R /Contents 6 0 R /Resources << /Font << /F1 11 0 R >> >> >> endobj\n");
    raw.extend_from_slice(&stream_obj(6, "", b"BT /F1 12 Tf 72 720 Td <01> Tj ET"));
    raw.extend_from_slice(b"10 0 obj << /Type /Font /Subtype /Type1 /ToUnicode 20 0 R >> endobj\n");
    raw.extend_from_slice(b"11 0 obj << /Type /Font /Subtype /Type1 /ToUnicode 21 0 R >> endobj\n");
    raw.extend_from_slice(&stream_obj(20, "", &cmap("<01> <0041>")));
    raw.extend_from_slice(&stream_obj(21, "", &cmap("<01> <0042>")));
    raw.extend_from_slice(b"xref\n0 1\ntrailer << /Root 1 0 R >>\nstartxref\n0\n%%EOF\n");
    let parsed = PdfSerializer::parse(&raw).unwrap();
    assert_eq!(parsed.pages[0].body_text, "A");
    assert_eq!(parsed.pages[1].body_text, "B");
}

#[test]
fn composite_font_is_two_byte_by_subtype_not_by_resource_name() {
    let fonts = "/F2 10 0 R /F7 11 0 R";
    let objs = [
        b"10 0 obj << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> endobj\n".to_vec(),
        b"11 0 obj << /Type /Font /Subtype /Type0 /Encoding /UniKS-UCS2-H >> endobj\n".to_vec(),
    ];
    let pdf = single_page_pdf(
        "BT /F7 12 Tf 72 720 Td <AC00> Tj ET\nBT /F2 12 Tf 72 700 Td <C3A9> Tj ET",
        fonts,
        &objs,
    );
    let parsed = PdfSerializer::parse(&pdf).unwrap();
    assert_eq!(parsed.pages[0].body_text, "가\né");
}

#[test]
fn flate_image_shorter_than_its_declared_size_allows_is_corrupted() {
    let image = stream_obj(
        10,
        "/Type /XObject /Subtype /Image /Width 20000 /Height 20000 /BitsPerComponent 8 /ColorSpace /DeviceGray /Filter /FlateDecode",
        &deflate_zlib(&[0u8; 64]),
    );
    let pdf = single_page_pdf("BT (x) Tj ET", "", &[image]);
    assert!(matches!(
        PdfSerializer::parse(&pdf),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));
}

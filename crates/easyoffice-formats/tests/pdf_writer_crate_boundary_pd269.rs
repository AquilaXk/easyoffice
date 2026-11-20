//! The engine's PDF writer builds its dictionaries and cross-reference table with a PDF writing
//! crate (PD-269, step 4). These tests hold what the crate does not promise: the same input writes
//! the same bytes on every run and every operating system, the table points at the objects it
//! lists, and the engine's own reader still accepts the result.

use easyoffice_formats::{
    encode_sheet_pdf, OdsUnpacked, PdfDocumentModel, PdfPageModel, PdfSerializer, SheetStyling,
};
use sha2::{Digest, Sha256};

fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn text_document() -> PdfDocumentModel {
    PdfDocumentModel {
        title: "예산 보고서 (2026)".to_string(),
        pages: (1..=3)
            .map(|n| PdfPageModel {
                page_number: n,
                rotation_degrees: if n == 2 { 90 } else { 0 },
                body_text: format!("{n}쪽 첫 줄\nSecond line of page {n}\n세 번째 줄"),
                approval_stamp: (n == 1).then(|| "승인".to_string()),
            })
            .collect(),
        watermark_text: Some("초안".to_string()),
    }
}

fn sheet() -> (Vec<OdsUnpacked>, Vec<SheetStyling>) {
    let cells = (0..60u32)
        .flat_map(|r| (0..8u32).map(move |c| (r, c, format!("{}", r * 17 + c))))
        .collect();
    (
        vec![("시트1".to_string(), cells)],
        vec![SheetStyling::default()],
    )
}

#[test]
fn the_same_input_writes_the_same_bytes_every_time() {
    let doc = text_document();
    let first = PdfSerializer::serialize(&doc);
    for _ in 0..4 {
        assert_eq!(PdfSerializer::serialize(&doc), first);
    }
    // A sheet is laid out through hash maps with a random seed per map; the pages must not depend on it.
    let (sheets, stylings) = sheet();
    let first = encode_sheet_pdf("시트", &sheets, &stylings, &[]).unwrap();
    for _ in 0..8 {
        assert_eq!(
            encode_sheet_pdf("시트", &sheets, &stylings, &[]).unwrap(),
            first
        );
    }
}

/// Pinned digests of two files. The crate writes numbers and the table with fixed rules (no locale,
/// no platform line ending), so every operating system must give the same digest; CI runs this
/// test on macOS, Windows and Linux.
#[test]
fn the_bytes_are_the_same_on_every_operating_system() {
    let text = PdfSerializer::serialize(&text_document());
    assert_eq!(
        hex_digest(&text),
        "dd3d2800ee81d044a16980ec1c35ddfc098e16a8c34f2b267ec2215bebe40a89",
        "text document, {} bytes",
        text.len()
    );
    let (sheets, stylings) = sheet();
    let laid = encode_sheet_pdf("시트", &sheets, &stylings, &[]).unwrap();
    assert_eq!(
        hex_digest(&laid),
        "f35d3279b4ca34e7383128a06cf4dfc3a7bff2751d724f4c36ae7a0c108f37d3",
        "sheet, {} bytes",
        laid.len()
    );
}

/// Offsets listed in the table of `pdf` as `(object number, offset)`.
fn table_entries(pdf: &[u8]) -> Vec<(usize, usize)> {
    let text = String::from_utf8_lossy(pdf);
    let table = text.rfind("\nxref\n").expect("a cross-reference table") + 6;
    let mut lines = text[table..].lines();
    let header: Vec<usize> = lines
        .next()
        .unwrap()
        .split(' ')
        .map(|n| n.parse().unwrap())
        .collect();
    assert_eq!(header[0], 0, "one subsection from object 0");
    lines
        .take(header[1])
        .enumerate()
        .filter(|(_, line)| line.ends_with(" n"))
        .map(|(number, line)| (number, line[..10].parse().unwrap()))
        .collect()
}

#[test]
fn the_table_points_at_every_object_and_the_start_marker_at_the_table() {
    let (sheets, stylings) = sheet();
    for pdf in [
        PdfSerializer::serialize(&text_document()),
        encode_sheet_pdf("시트", &sheets, &stylings, &[]).unwrap(),
    ] {
        let entries = table_entries(&pdf);
        assert!(entries.len() > 8, "objects: {}", entries.len());
        for (number, offset) in entries {
            let expected = format!("{number} 0 obj");
            assert!(
                pdf[offset..].starts_with(expected.as_bytes()),
                "object {number} is not at {offset}"
            );
        }
        let text = String::from_utf8_lossy(&pdf);
        let marker = text.rfind("startxref\n").unwrap() + 10;
        let table: usize = text[marker..].lines().next().unwrap().parse().unwrap();
        assert!(pdf[table..].starts_with(b"xref\n"));
        assert!(pdf.ends_with(b"%%EOF\n"));
    }
}

#[test]
fn the_engine_reads_back_what_the_crate_wrote() {
    let doc = text_document();
    let pdf = PdfSerializer::serialize(&doc);
    let parsed = PdfSerializer::parse(&pdf).unwrap();
    assert_eq!(parsed.title, doc.title);
    assert_eq!(parsed.watermark_text, doc.watermark_text);
    assert_eq!(parsed.pages.len(), doc.pages.len());
    for (read, written) in parsed.pages.iter().zip(&doc.pages) {
        assert_eq!(read.body_text, written.body_text);
        assert_eq!(read.rotation_degrees, written.rotation_degrees);
        assert_eq!(read.approval_stamp, written.approval_stamp);
    }
}

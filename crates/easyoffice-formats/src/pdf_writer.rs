//! ISO 32000 PDF writer for `PdfSerializer::serialize`.
//!
//! Korean text uses a predefined CJK font reference instead of an embedded font program: a Type0
//! font with the predefined `UniKS-UCS2-H` CMap over the `Adobe-Korea1` character collection and
//! the standard font name `HYGoThic-Medium`. Conforming viewers substitute an installed Korean
//! font for this well-known name, so no font binary ships with EasyOffice (see
//! `docs/policy-decisions.md`). A `ToUnicode` CMap keeps text extraction exact. Document
//! information strings use PDFDocEncoding for ASCII and UTF-16BE with a byte-order mark otherwise.

use crate::pdf_stream_parser::{encode_utf16be_hex_impl, escape_pdf_text};
use crate::{PdfDocumentModel, PdfPageModel, PdfSerializer};

const MAX_LINES_PER_PDF_PAGE: usize = 42;
/// Standard Korean font name resolved by viewers from the Adobe-Korea1 collection.
const KOREAN_CID_FONT: &str = "HYGoThic-Medium";
/// Predefined horizontal CMap mapping UCS-2 code units to Adobe-Korea1 CIDs (PDF 1.4 Table 5.15).
const KOREAN_CMAP: &str = "UniKS-UCS2-H";

/// Encodes a PDF text string (ISO 32000-1 §7.9.2.2): literal for ASCII, UTF-16BE+BOM otherwise.
pub(crate) fn pdf_text_string(text: &str) -> String {
    if text.is_ascii() {
        format!("({})", escape_pdf_text(text))
    } else {
        format!("<FEFF{}>", encode_utf16be_hex_impl(text))
    }
}

fn tj_operand(text: &str) -> (&'static str, String) {
    if text.is_ascii() {
        ("/F1", format!("({})", escape_pdf_text(text)))
    } else {
        ("/F2", format!("<{}>", encode_utf16be_hex_impl(text)))
    }
}

fn paginate(doc: &PdfDocumentModel) -> Vec<PdfPageModel> {
    let source_pages: Vec<PdfPageModel> = if doc.pages.is_empty() {
        vec![PdfPageModel {
            page_number: 1,
            rotation_degrees: 0,
            body_text: String::new(),
            approval_stamp: None,
        }]
    } else {
        doc.pages.clone()
    };
    let mut effective_pages: Vec<PdfPageModel> = Vec::with_capacity(source_pages.len());
    for page in source_pages {
        let lines: Vec<&str> = page.body_text.split('\n').collect();
        if lines.len() <= MAX_LINES_PER_PDF_PAGE {
            effective_pages.push(page);
        } else {
            for (c_idx, chunk) in lines.chunks(MAX_LINES_PER_PDF_PAGE).enumerate() {
                effective_pages.push(PdfPageModel {
                    page_number: effective_pages.len() + 1,
                    rotation_degrees: page.rotation_degrees,
                    body_text: chunk.join("\n"),
                    approval_stamp: if c_idx == 0 {
                        page.approval_stamp.clone()
                    } else {
                        None
                    },
                });
            }
        }
    }
    effective_pages
}

fn page_content_stream(page: &PdfPageModel, watermark: Option<&str>) -> String {
    let mut stream_lines: Vec<String> = Vec::new();
    for (l_idx, line_str) in page.body_text.split('\n').enumerate() {
        let y = 760 - (l_idx as i32) * 16;
        let (font_tag, operand) = tj_operand(line_str);
        stream_lines.push(format!("BT {font_tag} 12 Tf 54 {y} Td {operand} Tj ET"));
    }
    if let Some(stamp) = &page.approval_stamp {
        let (font_tag, operand) = tj_operand(&format!("[STAMP] {stamp}"));
        stream_lines.push(format!("BT {font_tag} 10 Tf 386 604 Td {operand} Tj ET"));
    }
    if let Some(wm) = watermark {
        let (font_tag, operand) = tj_operand(&format!("[WM] {wm}"));
        stream_lines.push(format!("BT {font_tag} 10 Tf 320 746 Td {operand} Tj ET"));
    }
    stream_lines.join("\n")
}

const TO_UNICODE_CMAP: &str = "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n1 beginbfrange\n<0000> <FFFF> <0000>\nendbfrange\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend";

struct PdfObjectWriter {
    out: Vec<u8>,
    offsets: Vec<usize>,
}

impl PdfObjectWriter {
    fn line(&mut self, line: &str) {
        self.out.extend_from_slice(line.as_bytes());
        self.out.push(b'\n');
    }

    fn object(&mut self, id: usize, body: &str) {
        self.offsets[id] = self.out.len();
        self.line(&format!("{id} 0 obj"));
        self.line(body);
        self.line("endobj");
    }

    fn stream(&mut self, id: usize, dict_extra: &str, data: &str) {
        self.offsets[id] = self.out.len();
        self.line(&format!("{id} 0 obj"));
        self.line(&format!("<<{dict_extra} /Length {} >>", data.len()));
        self.line("stream");
        self.line(data);
        self.line("endstream");
        self.line("endobj");
    }
}

impl PdfSerializer {
    pub fn serialize(doc: &PdfDocumentModel) -> Vec<u8> {
        let pages = paginate(doc);
        let page_count = pages.len();
        let kids_refs = (0..page_count)
            .map(|i| format!("{} 0 R", 3 + i * 2))
            .collect::<Vec<_>>()
            .join(" ");

        let latin_font_obj = 3 + page_count * 2;
        let korean_font_obj = latin_font_obj + 1;
        let cid_font_obj = latin_font_obj + 2;
        let descriptor_obj = latin_font_obj + 3;
        let cmap_obj = latin_font_obj + 4;
        let info_obj = latin_font_obj + 5;
        let total_objects = info_obj + 1;

        let mut w = PdfObjectWriter {
            out: Vec::with_capacity(4096),
            offsets: vec![0; total_objects],
        };
        w.line("%PDF-1.4");
        w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
        w.object(
            2,
            &format!("<< /Type /Pages /Count {page_count} /Kids [{kids_refs}] >>"),
        );

        for (idx, page) in pages.iter().enumerate() {
            let page_obj = 3 + idx * 2;
            let content_obj = page_obj + 1;
            w.object(
                page_obj,
                &format!(
                    "<< /Type /Page /Parent 2 0 R /PageNumber {} /Rotate {} /MediaBox [0 0 595 842] /Resources << /Font << /F1 {latin_font_obj} 0 R /F2 {korean_font_obj} 0 R >> >> /Contents {content_obj} 0 R >>",
                    page.page_number, page.rotation_degrees
                ),
            );
            let body = page_content_stream(page, doc.watermark_text.as_deref());
            w.stream(content_obj, "", &body);
        }

        w.object(
            latin_font_obj,
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
        );
        w.object(
            korean_font_obj,
            &format!(
                "<< /Type /Font /Subtype /Type0 /BaseFont /{KOREAN_CID_FONT}-{KOREAN_CMAP} /Encoding /{KOREAN_CMAP} /DescendantFonts [{cid_font_obj} 0 R] /ToUnicode {cmap_obj} 0 R >>"
            ),
        );
        w.object(
            cid_font_obj,
            &format!(
                "<< /Type /Font /Subtype /CIDFontType0 /BaseFont /{KOREAN_CID_FONT} /CIDSystemInfo << /Registry (Adobe) /Ordering (Korea1) /Supplement 1 >> /FontDescriptor {descriptor_obj} 0 R /DW 1000 /W [1 95 500] >>"
            ),
        );
        w.object(
            descriptor_obj,
            &format!(
                "<< /Type /FontDescriptor /FontName /{KOREAN_CID_FONT} /Flags 4 /FontBBox [-6 -145 1003 880] /ItalicAngle 0 /Ascent 880 /Descent -120 /CapHeight 880 /StemV 93 >>"
            ),
        );
        w.stream(
            cmap_obj,
            " /Type /CMap /CMapName /Adobe-Identity-UCS",
            TO_UNICODE_CMAP,
        );
        w.object(
            info_obj,
            &format!(
                "<< /Title {} /Producer (EasyOffice) >>",
                pdf_text_string(&doc.title)
            ),
        );

        let xref_offset = w.out.len();
        w.line("xref");
        w.line(&format!("0 {total_objects}"));
        w.line("0000000000 65535 f ");
        let offsets = std::mem::take(&mut w.offsets);
        for offset in offsets.iter().skip(1) {
            w.line(&format!("{offset:010} 00000 n "));
        }
        w.line("trailer");
        w.line(&format!(
            "<< /Size {total_objects} /Root 1 0 R /Info {info_obj} 0 R >>"
        ));
        w.line("startxref");
        w.line(&xref_offset.to_string());
        w.line("%%EOF");
        w.out
    }
}

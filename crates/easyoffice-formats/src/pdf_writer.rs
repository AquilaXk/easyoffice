//! ISO 32000 PDF writer for `PdfSerializer::serialize`.
//!
//! Korean text uses a predefined CJK font reference instead of an embedded font program: a Type0
//! font with the predefined `UniKS-UCS2-H` CMap over the `Adobe-Korea1` character collection and
//! the standard font name `HYGoThic-Medium`. Conforming viewers substitute an installed Korean
//! font for this well-known name, so no font binary ships with EasyOffice (see
//! `docs/policy-decisions.md`). A `ToUnicode` CMap keeps text extraction exact. Document
//! information strings use PDFDocEncoding for ASCII and UTF-16BE with a byte-order mark otherwise.

use crate::pdf_image::PdfImageObject;
use crate::pdf_stream_parser::{encode_utf16be_hex_impl, escape_pdf_text};
use crate::{PdfDocumentModel, PdfPageModel, PdfSerializer};

const MAX_LINES_PER_PDF_PAGE: usize = 42;
/// Page box (A4, points) and the margin kept free around a placed picture.
pub(crate) const PDF_PAGE_WIDTH_PT: u64 = 595;
pub(crate) const PDF_PAGE_HEIGHT_PT: u64 = 842;
pub(crate) const PDF_IMAGE_MARGIN_PT: u64 = 54;
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
    paginate_pages(source_pages)
}

/// Splits pages longer than [`MAX_LINES_PER_PDF_PAGE`] lines; the stamp stays on the first piece.
pub(crate) fn paginate_pages(source_pages: Vec<PdfPageModel>) -> Vec<PdfPageModel> {
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
        self.stream_bytes(id, dict_extra, data.as_bytes());
    }

    fn stream_bytes(&mut self, id: usize, dict_extra: &str, data: &[u8]) {
        self.offsets[id] = self.out.len();
        self.line(&format!("{id} 0 obj"));
        self.line(&format!("<<{dict_extra} /Length {} >>", data.len()));
        self.line("stream");
        self.out.extend_from_slice(data);
        self.out.push(b'\n');
        self.line("endstream");
        self.line("endobj");
    }
}

/// A page that shows one picture scaled to `width_pt` x `height_pt` at the top-left margin.
pub(crate) struct PdfImagePage {
    pub page_number: usize,
    pub image: PdfImageObject,
    pub width_pt: u64,
    pub height_pt: u64,
}

/// One output page: text lines or a picture.
pub(crate) enum PdfPageEntry {
    Text(PdfPageModel),
    Image(PdfImagePage),
}

/// Object numbers of one page: page, content stream, and optional image / soft mask.
struct PagePlan {
    page_obj: usize,
    content_obj: usize,
    image_obj: Option<usize>,
    mask_obj: Option<usize>,
}

/// Assigns object numbers in page order (first page object is 3); returns the next free number.
fn plan_objects(entries: &[PdfPageEntry]) -> (Vec<PagePlan>, usize) {
    let mut next = 3usize;
    let mut plans = Vec::with_capacity(entries.len());
    for entry in entries {
        let page_obj = next;
        let content_obj = next + 1;
        next += 2;
        let (mut image_obj, mut mask_obj) = (None, None);
        if let PdfPageEntry::Image(page) = entry {
            image_obj = Some(next);
            next += 1;
            if page.image.soft_mask.is_some() {
                mask_obj = Some(next);
                next += 1;
            }
        }
        plans.push(PagePlan {
            page_obj,
            content_obj,
            image_obj,
            mask_obj,
        });
    }
    (plans, next)
}

fn image_content_stream(page: &PdfImagePage) -> String {
    let y = PDF_PAGE_HEIGHT_PT - PDF_IMAGE_MARGIN_PT - page.height_pt;
    format!(
        "q {} 0 0 {} {PDF_IMAGE_MARGIN_PT} {y} cm /Im1 Do Q",
        page.width_pt, page.height_pt
    )
}

impl PdfSerializer {
    pub fn serialize(doc: &PdfDocumentModel) -> Vec<u8> {
        let entries: Vec<PdfPageEntry> =
            paginate(doc).into_iter().map(PdfPageEntry::Text).collect();
        Self::serialize_entries(&doc.title, doc.watermark_text.as_deref(), &entries)
    }

    /// Writes text pages and picture pages in the given order (page numbers are taken as is).
    pub(crate) fn serialize_entries(
        title: &str,
        watermark: Option<&str>,
        entries: &[PdfPageEntry],
    ) -> Vec<u8> {
        let page_count = entries.len();
        let (plans, latin_font_obj) = plan_objects(entries);
        let kids_refs = plans
            .iter()
            .map(|p| format!("{} 0 R", p.page_obj))
            .collect::<Vec<_>>()
            .join(" ");
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

        let fonts = format!("/Font << /F1 {latin_font_obj} 0 R /F2 {korean_font_obj} 0 R >>");
        for (entry, plan) in entries.iter().zip(&plans) {
            write_page(&mut w, entry, plan, &fonts, watermark);
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
                pdf_text_string(title)
            ),
        );
        write_trailer(&mut w, total_objects, info_obj);
        w.out
    }
}

fn write_page(
    w: &mut PdfObjectWriter,
    entry: &PdfPageEntry,
    plan: &PagePlan,
    fonts: &str,
    watermark: Option<&str>,
) {
    let (page_number, rotation, xobjects, body) = match (entry, plan.image_obj) {
        (PdfPageEntry::Text(page), _) => (
            page.page_number,
            page.rotation_degrees,
            String::new(),
            page_content_stream(page, watermark),
        ),
        (PdfPageEntry::Image(page), Some(image_obj)) => (
            page.page_number,
            0,
            format!(" /XObject << /Im1 {image_obj} 0 R >>"),
            image_content_stream(page),
        ),
        (PdfPageEntry::Image(_), None) => return,
    };
    w.object(
        plan.page_obj,
        &format!(
            "<< /Type /Page /Parent 2 0 R /PageNumber {page_number} /Rotate {rotation} /MediaBox [0 0 {PDF_PAGE_WIDTH_PT} {PDF_PAGE_HEIGHT_PT}] /Resources << {fonts}{xobjects} >> /Contents {} 0 R >>",
            plan.content_obj
        ),
    );
    w.stream(plan.content_obj, "", &body);
    if let (PdfPageEntry::Image(page), Some(image_obj)) = (entry, plan.image_obj) {
        write_image_objects(w, &page.image, image_obj, plan.mask_obj);
    }
}

fn write_image_objects(
    w: &mut PdfObjectWriter,
    image: &PdfImageObject,
    image_obj: usize,
    mask_obj: Option<usize>,
) {
    let mut dict = image.dict.clone();
    if let Some(mask) = mask_obj {
        dict.push_str(&format!(" /SMask {mask} 0 R"));
    }
    w.stream_bytes(image_obj, &dict, &image.data);
    if let (Some(mask), Some(soft)) = (mask_obj, image.soft_mask.as_deref()) {
        w.stream_bytes(mask, &soft.dict, &soft.data);
    }
}

fn write_trailer(w: &mut PdfObjectWriter, total_objects: usize, info_obj: usize) {
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
}

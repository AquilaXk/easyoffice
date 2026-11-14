//! ISO 32000 PDF writer for `PdfSerializer::serialize`.
//!
//! Korean text uses a predefined CJK font reference instead of an embedded font program: a Type0
//! font with the predefined `UniKS-UCS2-H` CMap over the `Adobe-Korea1` character collection and
//! the standard font name `HYGoThic-Medium`. Conforming viewers substitute an installed Korean
//! font for this well-known name, so no font binary ships with EasyOffice (see
//! `docs/policy-decisions.md`). A `ToUnicode` CMap keeps text extraction exact. Document
//! information strings use PDFDocEncoding for ASCII and UTF-16BE with a byte-order mark otherwise.

use crate::pdf_highlight::{write_annotation, HighlightPass, PageAnnotation, PdfHighlight};
use crate::pdf_image::PdfImageObject;
use crate::pdf_stream_parser::{encode_utf16be_hex_impl, escape_pdf_text};
use crate::pdf_text_wrap::wrap_spans;
use crate::{PdfDocumentModel, PdfPageModel, PdfSerializer};
use std::ops::Range;

const MAX_LINES_PER_PDF_PAGE: usize = 42;
/// Body text starts at the left margin in 12 pt; a line ends at the right margin, which is as wide.
pub(crate) const PDF_TEXT_FONT_PT: u64 = 12;
pub(crate) const PDF_TEXT_MARGIN_PT: u64 = 54;
/// Baseline of the first body line and the distance between lines, in points.
pub(crate) const PDF_BODY_FIRST_BASELINE_PT: i32 = 760;
pub(crate) const PDF_BODY_LINE_PITCH_PT: i32 = 16;
/// Width available to a text line in thousandths of the font size (the unit of glyph advances).
const PDF_TEXT_LINE_UNITS: u32 =
    ((PDF_PAGE_WIDTH_PT - 2 * PDF_TEXT_MARGIN_PT) * 1000 / PDF_TEXT_FONT_PT) as u32;
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

/// Whether the Latin font (Helvetica, WinAnsi) shows `c`: ASCII and the Latin-1 letters and signs (the middle dot among them).
fn in_latin_font(c: char) -> bool {
    matches!(c, ' '..='~' | '\u{A0}'..='\u{FF}')
}

/// Width of `text` in thousandths of the font size as [`text_ops`] shows it: the Latin font's widths for what it shows,
/// a full em for the rest. Latin-1 signs outside the ASCII table count 556, the width of most of them.
pub(crate) fn mixed_units(text: &str) -> u32 {
    text.chars()
        .map(|c| match c {
            ' '..='~' => crate::pdf_text_wrap::ascii_units(c),
            '\u{A0}'..='\u{FF}' => 556,
            _ => 1000,
        })
        .sum()
}

/// The operators that show `text` at (`x`, `y`) in `size` pt. A line the Korean font shows whole is one text object in one
/// font ([`tj_operand`]). A line with Latin-1 signs outside ASCII (the middle dot, accented letters), which the Korean
/// font's viewer substitute may not draw, is still one text object: the font changes at the runs, and each run
/// starts at the advance width of the runs before it, the signs and the ASCII beside them in the Latin font. A reader
/// takes the runs of one text object on one baseline as one line. No character is left out.
pub(crate) fn text_ops(x: f64, y: f64, size: f64, text: &str) -> String {
    if !text.chars().any(|c| matches!(c, '\u{A0}'..='\u{FF}')) {
        let (font, operand) = tj_operand(text);
        return format!("BT {font} {size:.2} Tf {x:.2} {y:.2} Td {operand} Tj ET");
    }
    let mut out = String::from("BT");
    // `Td` moves from the start of the previous run: the offset is the width of that run, rounded
    // against the sum already written so the error never adds up.
    let (mut width_before, mut written) = (0.0_f64, 0.0_f64);
    let mut rest = text;
    while let Some(first) = rest.chars().next() {
        let latin = in_latin_font(first);
        let end = rest
            .char_indices()
            .find(|&(_, c)| in_latin_font(c) != latin)
            .map_or(rest.len(), |(i, _)| i);
        let (run, tail) = rest.split_at(end);
        let target = (width_before * 100.0).round() / 100.0;
        let (dx, dy) = if rest.len() == text.len() {
            (x, y)
        } else {
            (target - written, 0.0)
        };
        written = target;
        if latin {
            let literal = run.chars().fold(String::new(), |mut acc, c| {
                match c {
                    '\\' | '(' | ')' => {
                        acc.push('\\');
                        acc.push(c);
                    }
                    ' '..='~' => acc.push(c),
                    _ => acc.push_str(&format!("\\{:03o}", c as u32)),
                }
                acc
            });
            out.push_str(&format!(
                " /F1 {size:.2} Tf {dx:.2} {dy:.2} Td ({literal}) Tj"
            ));
        } else {
            out.push_str(&format!(
                " /F2 {size:.2} Tf {dx:.2} {dy:.2} Td <{}> Tj",
                encode_utf16be_hex_impl(run)
            ));
        }
        width_before += f64::from(mixed_units(run)) * size / 1000.0;
        rest = tail;
    }
    out.push_str(" ET");
    out
}

pub(crate) fn tj_operand(text: &str) -> (&'static str, String) {
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

/// One output line of a laid-out page: the source line it comes from (by index in the page's
/// text split at newlines) and the byte range of that line it shows.
pub(crate) struct LaidLine {
    pub source: usize,
    pub range: Range<usize>,
}

/// A page with its lines broken at the text width, and where each line came from.
pub(crate) struct LaidPage {
    pub model: PdfPageModel,
    pub lines: Vec<LaidLine>,
}

/// Breaks every line of `page` at the text width and splits it into pages of at most
/// [`MAX_LINES_PER_PDF_PAGE`] lines (the stamp stays on the first). Pages are numbered from
/// `first_number`.
pub(crate) fn lay_out(page: &PdfPageModel, first_number: usize) -> Vec<LaidPage> {
    let source: Vec<&str> = page.body_text.split('\n').collect();
    let mut lines: Vec<LaidLine> = Vec::new();
    for (idx, text) in source.iter().enumerate() {
        lines.extend(
            wrap_spans(text, PDF_TEXT_LINE_UNITS)
                .into_iter()
                .map(|range| LaidLine { source: idx, range }),
        );
    }
    lines
        .chunks(MAX_LINES_PER_PDF_PAGE)
        .enumerate()
        .map(|(c_idx, chunk)| LaidPage {
            model: PdfPageModel {
                page_number: first_number + c_idx,
                rotation_degrees: page.rotation_degrees,
                body_text: chunk
                    .iter()
                    .map(|l| &source[l.source][l.range.clone()])
                    .collect::<Vec<_>>()
                    .join("\n"),
                approval_stamp: if c_idx == 0 {
                    page.approval_stamp.clone()
                } else {
                    None
                },
            },
            lines: chunk
                .iter()
                .map(|l| LaidLine {
                    source: l.source,
                    range: l.range.clone(),
                })
                .collect(),
        })
        .collect()
}

/// Breaks every line at the text width, then splits pages longer than [`MAX_LINES_PER_PDF_PAGE`]
/// lines. Pages are numbered in output order.
pub(crate) fn paginate_pages(source_pages: Vec<PdfPageModel>) -> Vec<PdfPageModel> {
    let mut effective_pages: Vec<PdfPageModel> = Vec::with_capacity(source_pages.len());
    for page in &source_pages {
        let first = effective_pages.len() + 1;
        effective_pages.extend(lay_out(page, first).into_iter().map(|p| p.model));
    }
    effective_pages
}

/// [`paginate_pages`] for one page as output entries, with the annotations of the highlight
/// search (`pass`) on the pages they fall on.
pub(crate) fn text_entries(
    page: &PdfPageModel,
    first_number: usize,
    pass: Option<&mut HighlightPass<'_>>,
) -> Vec<PdfPageEntry> {
    let laid = lay_out(page, first_number);
    let annotations = match pass {
        Some(pass) => pass.annotate(&page.body_text, &laid),
        None => vec![Vec::new(); laid.len()],
    };
    laid.into_iter()
        .zip(annotations)
        .map(|(p, a)| PdfPageEntry::Text(p.model, a))
        .collect()
}

/// The content stream of a text page. Lines are one font each where highlight annotations were placed by their widths
/// (`annotated`); otherwise they switch fonts run by run ([`text_ops`]).
fn page_content_stream(page: &PdfPageModel, watermark: Option<&str>, annotated: bool) -> String {
    let mut stream_lines: Vec<String> = Vec::new();
    for (l_idx, line_str) in page.body_text.split('\n').enumerate() {
        let y = PDF_BODY_FIRST_BASELINE_PT - (l_idx as i32) * PDF_BODY_LINE_PITCH_PT;
        if !annotated {
            stream_lines.push(text_ops(
                PDF_TEXT_MARGIN_PT as f64,
                f64::from(y),
                PDF_TEXT_FONT_PT as f64,
                line_str,
            ));
            continue;
        }
        let (font_tag, operand) = tj_operand(line_str);
        stream_lines.push(format!(
            "BT {font_tag} {PDF_TEXT_FONT_PT} Tf {PDF_TEXT_MARGIN_PT} {y} Td {operand} Tj ET"
        ));
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

pub(crate) struct PdfObjectWriter {
    out: Vec<u8>,
    offsets: Vec<usize>,
}

impl PdfObjectWriter {
    fn line(&mut self, line: &str) {
        self.out.extend_from_slice(line.as_bytes());
        self.out.push(b'\n');
    }

    pub(crate) fn object(&mut self, id: usize, body: &str) {
        self.offsets[id] = self.out.len();
        self.line(&format!("{id} 0 obj"));
        self.line(body);
        self.line("endobj");
    }

    pub(crate) fn stream(&mut self, id: usize, dict_extra: &str, data: &str) {
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

/// Page settings a PDF save can carry besides the text: rotation of every page, the watermark
/// shown on text pages, and the approval stamp on the first page. The default writes none.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PdfWriteOptions {
    /// Clockwise page rotation in degrees (a multiple of 90).
    pub rotation_degrees: u16,
    pub watermark: Option<String>,
    pub approval_stamp: Option<String>,
    /// A phrase marked with highlight or underline annotations wherever it occurs.
    pub highlight: Option<PdfHighlight>,
}

impl PdfWriteOptions {
    pub(crate) fn rotate(&self, entries: &mut [PdfPageEntry]) {
        for entry in entries {
            match entry {
                PdfPageEntry::Text(page, _) => {
                    page.rotation_degrees = i32::from(self.rotation_degrees)
                }
                PdfPageEntry::Image(page) => page.rotation_degrees = self.rotation_degrees,
                PdfPageEntry::Laid(page) => page.rotation_degrees = self.rotation_degrees,
            }
        }
    }
}

/// A page that shows one picture scaled to `width_pt` x `height_pt` at the top-left margin.
pub(crate) struct PdfImagePage {
    pub page_number: usize,
    pub rotation_degrees: u16,
    pub image: PdfImageObject,
    pub width_pt: u64,
    pub height_pt: u64,
}

/// A page of its own size whose content stream is already placed (`pdf_laid.rs`).
pub(crate) struct PdfLaidPage {
    pub page_number: usize,
    pub rotation_degrees: u16,
    pub width_pt: f64,
    pub height_pt: f64,
    pub content: String,
}

/// One output page: text lines, a picture or a laid-out page.
pub(crate) enum PdfPageEntry {
    Text(PdfPageModel, Vec<PageAnnotation>),
    Image(PdfImagePage),
    Laid(PdfLaidPage),
}

/// Object numbers of one page: page, content stream, and optional image / soft mask.
struct PagePlan {
    page_obj: usize,
    content_obj: usize,
    image_obj: Option<usize>,
    mask_obj: Option<usize>,
    /// Object numbers of the page's annotations and their appearance streams.
    annotations: Vec<(usize, usize)>,
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
            annotations: Vec::new(),
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
        let entries: Vec<PdfPageEntry> = paginate(doc)
            .into_iter()
            .map(|page| PdfPageEntry::Text(page, Vec::new()))
            .collect();
        Self::serialize_entries(&doc.title, doc.watermark_text.as_deref(), &entries)
    }

    /// Writes text pages and picture pages in the given order (page numbers are taken as is).
    pub(crate) fn serialize_entries(
        title: &str,
        watermark: Option<&str>,
        entries: &[PdfPageEntry],
    ) -> Vec<u8> {
        let page_count = entries.len();
        let (mut plans, latin_font_obj) = plan_objects(entries);
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
        // Annotation objects follow the document objects.
        let mut total_objects = info_obj + 1;
        for (entry, plan) in entries.iter().zip(&mut plans) {
            if let PdfPageEntry::Text(_, annotations) = entry {
                for _ in annotations {
                    plan.annotations.push((total_objects, total_objects + 1));
                    total_objects += 2;
                }
            }
        }

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
                "<< /Type /Font /Subtype /CIDFontType0 /BaseFont /{KOREAN_CID_FONT} /CIDSystemInfo << /Registry (Adobe) /Ordering (Korea1) /Supplement 1 >> /FontDescriptor {descriptor_obj} 0 R /DW 1000 /W {} >>",
                crate::pdf_text_wrap::cid_ascii_widths()
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
    let mut media = format!("0 0 {PDF_PAGE_WIDTH_PT} {PDF_PAGE_HEIGHT_PT}");
    let (page_number, rotation, xobjects, body) = match (entry, plan.image_obj) {
        (PdfPageEntry::Laid(page), _) => {
            media = format!("0 0 {:.2} {:.2}", page.width_pt, page.height_pt);
            let mut content = page.content.clone();
            if let Some(wm) = watermark {
                let (font_tag, operand) = tj_operand(&format!("[WM] {wm}"));
                content.push_str(&format!(
                    "\nBT {font_tag} 10 Tf 36 {:.2} Td {operand} Tj ET",
                    page.height_pt - 30.0
                ));
            }
            (
                page.page_number,
                i32::from(page.rotation_degrees),
                String::new(),
                content,
            )
        }
        (PdfPageEntry::Text(page, annotations), _) => (
            page.page_number,
            page.rotation_degrees,
            String::new(),
            page_content_stream(page, watermark, !annotations.is_empty()),
        ),
        (PdfPageEntry::Image(page), Some(image_obj)) => (
            page.page_number,
            i32::from(page.rotation_degrees),
            format!(" /XObject << /Im1 {image_obj} 0 R >>"),
            image_content_stream(page),
        ),
        (PdfPageEntry::Image(_), None) => return,
    };
    let annots = if plan.annotations.is_empty() {
        String::new()
    } else {
        let refs: Vec<String> = plan
            .annotations
            .iter()
            .map(|(annot, _)| format!("{annot} 0 R"))
            .collect();
        format!(" /Annots [{}]", refs.join(" "))
    };
    w.object(
        plan.page_obj,
        &format!(
            "<< /Type /Page /Parent 2 0 R /PageNumber {page_number} /Rotate {rotation} /MediaBox [{media}] /Resources << {fonts}{xobjects} >> /Contents {} 0 R{annots} >>",
            plan.content_obj
        ),
    );
    w.stream(plan.content_obj, "", &body);
    if let (PdfPageEntry::Image(page), Some(image_obj)) = (entry, plan.image_obj) {
        write_image_objects(w, &page.image, image_obj, plan.mask_obj);
    }
    if let PdfPageEntry::Text(_, annotations) = entry {
        for (annotation, (annot_id, ap_id)) in annotations.iter().zip(&plan.annotations) {
            write_annotation(w, *annot_id, *ap_id, plan.page_obj, annotation);
        }
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

#[cfg(test)]
mod tests {
    use super::{mixed_units, text_ops};

    #[test]
    fn a_mixed_line_is_one_text_object_whose_runs_start_where_the_runs_before_end() {
        let ops = text_ops(54.0, 700.0, 12.0, "가·나");
        assert_eq!(ops.matches("BT").count(), 1, "{ops}");
        assert_eq!(ops.matches("ET").count(), 1, "{ops}");
        let moves: Vec<(f64, f64)> = ops
            .split_whitespace()
            .collect::<Vec<_>>()
            .windows(3)
            .filter(|w| w[2] == "Td")
            .map(|w| (w[0].parse().unwrap(), w[1].parse().unwrap()))
            .collect();
        let advance = |run: &str| f64::from(mixed_units(run)) * 12.0 / 1000.0;
        assert_eq!(moves.len(), 3, "{ops}");
        assert_eq!(moves[0], (54.0, 700.0));
        assert!((moves[1].0 - advance("가")).abs() < 0.006 && moves[1].1 == 0.0);
        assert!((moves[2].0 - advance("·")).abs() < 0.006 && moves[2].1 == 0.0);
    }
}

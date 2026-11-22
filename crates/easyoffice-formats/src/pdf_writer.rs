//! ISO 32000 PDF writer for `PdfSerializer::serialize`.
//!
//! Korean text uses a predefined CJK font reference instead of an embedded font program: a Type0
//! font with the predefined `UniKS-UCS2-H` CMap over the `Adobe-Korea1` character collection and
//! the standard font name `HYGoThic-Medium`. Conforming viewers substitute an installed Korean
//! font for this well-known name, so no font binary ships with EasyOffice (see
//! `docs/policy-decisions.md`). A `ToUnicode` CMap keeps text extraction exact. Document
//! information strings use PDFDocEncoding for ASCII and UTF-16BE with a byte-order mark otherwise.
//!
//! The objects (catalog, page tree, pages, fonts, pictures, annotations, information dictionary) and
//! the cross-reference table are written by the PDF writing crate the engine links (PD-269, step 4). This file
//! keeps what is the engine's own: which objects a page needs and their numbers, the page
//! content (line breaking, mixed fonts, watermark and stamp markers) and the `/PageNumber` entry the
//! engine's reader looks for.

use crate::pdf_highlight::{write_annotation, HighlightPass, PageAnnotation, PdfHighlight};
use crate::pdf_image::PdfImageObject;
use crate::pdf_stream_parser::{encode_utf16be_hex_impl, escape_pdf_text};
use crate::pdf_text_wrap::wrap_spans;
use crate::{PdfDocumentModel, PdfPageModel, PdfSerializer};
use ::pdf_writer::types::{CidFontType, FontFlags, FunctionShadingType, SystemInfo};
use ::pdf_writer::writers::{FunctionShading, Resources};
use ::pdf_writer::{Filter, Finish, Name, Pdf, Rect, Ref, Str, TextStr};

use std::ops::Range;

const MAX_LINES_PER_PDF_PAGE: usize = 42;
/// Body text starts at the left margin in 12 pt; a line ends at the right margin, which is as wide.
pub(crate) const PDF_TEXT_FONT_PT: u64 = 12;
pub(crate) const PDF_TEXT_MARGIN_PT: u64 = 54;
/// Baseline of the first body line and the distance between lines, in points.
pub(crate) const PDF_BODY_FIRST_BASELINE_PT: i32 = 760;
pub(crate) const PDF_BODY_LINE_PITCH_PT: i32 = 16;
/// Width available to a text line in thousandths of the font size (the unit of glyph advances).
const PDF_TEXT_LINE_UNITS: u32 = ((PDF_PAGE_WIDTH_PT.saturating_sub(2 * PDF_TEXT_MARGIN_PT))
    .saturating_mul(1000)
    / PDF_TEXT_FONT_PT) as u32;
/// Page box (A4, points) and the margin kept free around a placed picture.
pub(crate) const PDF_PAGE_WIDTH_PT: u64 = 595;
pub(crate) const PDF_PAGE_HEIGHT_PT: u64 = 842;
pub(crate) const PDF_IMAGE_MARGIN_PT: u64 = 54;
/// Standard Korean font name resolved by viewers from the Adobe-Korea1 collection.
const KOREAN_CID_FONT: &str = "HYGoThic-Medium";
/// Predefined horizontal CMap mapping UCS-2 code units to Adobe-Korea1 CIDs (PDF 1.4 Table 5.15).
const KOREAN_CMAP: &str = "UniKS-UCS2-H";

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
                page_number: first_number.saturating_add(c_idx),
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
        let first = effective_pages.len().saturating_add(1);
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
        let y_off = i32::try_from(l_idx)
            .unwrap_or(i32::MAX)
            .saturating_mul(PDF_BODY_LINE_PITCH_PT);
        let y = PDF_BODY_FIRST_BASELINE_PT.saturating_sub(y_off);
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

/// A page of its own size whose content stream is already placed (`pdf_laid.rs`), with the pictures
/// it draws by resource name.
pub(crate) struct PdfLaidPage {
    pub page_number: usize,
    pub rotation_degrees: u16,
    pub width_pt: f64,
    pub height_pt: f64,
    pub content: String,
    pub xobjects: Vec<(String, PdfImageObject)>,
    /// The gradients the content draws by resource name, as the colors at the two ends of the unit axis (`0xRRGGBB`): each
    /// is a form of one unit square that content stretches over a box with `cm` and `Do` (PD-300).
    pub gradients: Vec<(String, u32, u32)>,
    /// The content stream is stored deflated (`FlateDecode`).
    pub deflate: bool,
}

/// One output page: text lines, a picture or a laid-out page.
pub(crate) enum PdfPageEntry {
    Text(PdfPageModel, Vec<PageAnnotation>),
    Image(PdfImagePage),
    Laid(PdfLaidPage),
}

/// Object numbers of one page: page, content stream, and its pictures and annotations.
struct PagePlan {
    page: Ref,
    content: Ref,
    /// The picture of a picture page and its soft mask.
    image: Option<Ref>,
    mask: Option<Ref>,
    /// The pictures of a laid page and their soft masks, in resource order.
    xobjects: Vec<(Ref, Option<Ref>)>,
    /// The forms of the gradients of a laid page and the functions they shade with, in resource order.
    gradients: Vec<(Ref, Ref)>,
    /// The page's annotations and their appearance streams.
    annotations: Vec<(Ref, Ref)>,
}

impl PagePlan {
    /// The `/XObject` resource of the page: `/Im1` of a picture page, the named pictures of a
    /// laid page. Nothing is written for a page without pictures.
    fn xobject_resources(&self, entry: &PdfPageEntry, resources: &mut Resources<'_>) {
        if let Some(image) = self.image {
            resources.x_objects().pair(Name(b"Im1"), image);
        }
        if let PdfPageEntry::Laid(page) = entry {
            if !page.xobjects.is_empty() || !page.gradients.is_empty() {
                let mut forms = resources.x_objects();
                for ((name, _), &(picture, _)) in page.xobjects.iter().zip(&self.xobjects) {
                    forms.pair(Name(name.as_bytes()), picture);
                }
                for ((name, ..), &(form, _)) in page.gradients.iter().zip(&self.gradients) {
                    forms.pair(Name(name.as_bytes()), form);
                }
            }
        }
    }
}

/// Assigns object numbers in page order (first page object is 3); returns the next free number.
fn plan_objects(entries: &[PdfPageEntry]) -> Option<(Vec<PagePlan>, usize)> {
    let mut next = 3usize;
    let mut take = || {
        let id = next;
        next = next.checked_add(1)?;
        object_ref_checked(id)
    };
    let mut plans = Vec::with_capacity(entries.len());
    for entry in entries {
        let page = take()?;
        let content = take()?;
        let (mut image, mut mask) = (None, None);
        let mut xobjects = Vec::new();
        let mut gradients = Vec::new();
        match entry {
            PdfPageEntry::Image(page) => {
                image = Some(take()?);
                if page.image.soft_mask.is_some() {
                    mask = Some(take()?);
                }
            }
            PdfPageEntry::Laid(page) => {
                for (_, picture) in &page.xobjects {
                    let picture_ref = take()?;
                    let mask_ref = if picture.soft_mask.is_some() {
                        Some(take()?)
                    } else {
                        None
                    };
                    xobjects.push((picture_ref, mask_ref));
                }
                for _ in &page.gradients {
                    gradients.push((take()?, take()?));
                }
            }
            PdfPageEntry::Text(..) => {}
        }
        plans.push(PagePlan {
            page,
            content,
            image,
            mask,
            xobjects,
            gradients,
            annotations: Vec::new(),
        });
    }
    Some((plans, next))
}

fn image_content_stream(page: &PdfImagePage) -> String {
    let y = PDF_PAGE_HEIGHT_PT
        .saturating_sub(PDF_IMAGE_MARGIN_PT)
        .saturating_sub(page.height_pt);
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
        let (mut plans, latin_font_obj) = plan_objects(entries).expect("PDF object count overflow");
        let fonts = FontObjects::at(latin_font_obj);
        // Annotation objects follow the document objects.
        let mut next_free = latin_font_obj
            .checked_add(FontObjects::COUNT)
            .expect("PDF object count overflow");
        for (entry, plan) in entries.iter().zip(&mut plans) {
            if let PdfPageEntry::Text(_, annotations) = entry {
                for _ in annotations {
                    plan.annotations
                        .push((object_ref(next_free), object_ref(next_free + 1)));
                    next_free = next_free.checked_add(2).expect("PDF object count overflow");
                }
            }
        }

        let mut pdf = Pdf::with_capacity(4096);
        pdf.set_version(1, 4);
        pdf.catalog(CATALOG_REF).pages(PAGES_REF);
        pdf.pages(PAGES_REF)
            .count(i32::try_from(entries.len()).expect("PDF object count overflow"))
            .kids(plans.iter().map(|plan| plan.page));
        for (entry, plan) in entries.iter().zip(&plans) {
            write_page(&mut pdf, entry, plan, &fonts, watermark);
        }
        fonts.write(&mut pdf);
        pdf.document_info(fonts.info)
            .title(TextStr(title))
            .producer(TextStr("EasyOffice"));
        // The crate ends the file at `%%EOF`; the file ends with a line break like every text
        // file, so line-based tools and the readers of this crate see a complete last line.
        let mut bytes = pdf.finish();
        bytes.push(b'\n');
        bytes
    }
}

/// The document-level objects every file has: catalog 1 and page tree 2 come first, then the
/// pages, then the fonts and the information dictionary.
const CATALOG_REF: Ref = Ref::new(1);
const PAGES_REF: Ref = Ref::new(2);

/// Object numbers are small counts; one past `i32::MAX` objects is not a file anyone can write.
fn object_ref(id: usize) -> Ref {
    object_ref_checked(id).expect("PDF object count overflow")
}

fn object_ref_checked(id: usize) -> Option<Ref> {
    i32::try_from(id).ok().map(Ref::new)
}

/// Object numbers of the two page fonts and what they point to, in the order they are written.
struct FontObjects {
    latin: Ref,
    korean: Ref,
    cid: Ref,
    descriptor: Ref,
    cmap: Ref,
    info: Ref,
}

impl FontObjects {
    /// Latin font, Korean font, its descendant, descriptor, `ToUnicode` map and information dictionary.
    const COUNT: usize = 6;

    fn at(first: usize) -> Self {
        let next = |offset: usize| {
            object_ref(
                first
                    .checked_add(offset)
                    .expect("PDF object count overflow"),
            )
        };
        Self {
            latin: next(0),
            korean: next(1),
            cid: next(2),
            descriptor: next(3),
            cmap: next(4),
            info: next(5),
        }
    }

    /// The `/Font` resource of a page.
    fn resources(&self, resources: &mut Resources<'_>) {
        let mut fonts = resources.fonts();
        fonts.pair(Name(b"F1"), self.latin);
        fonts.pair(Name(b"F2"), self.korean);
    }

    fn write(&self, pdf: &mut Pdf) {
        pdf.type1_font(self.latin)
            .base_font(Name(b"Helvetica"))
            .encoding_predefined(Name(b"WinAnsiEncoding"));
        let korean_name = format!("{KOREAN_CID_FONT}-{KOREAN_CMAP}");
        pdf.type0_font(self.korean)
            .base_font(Name(korean_name.as_bytes()))
            .encoding_predefined(Name(KOREAN_CMAP.as_bytes()))
            .descendant_font(self.cid)
            .to_unicode(self.cmap);
        let mut cid_font = pdf.cid_font(self.cid);
        cid_font
            .subtype(CidFontType::Type0)
            .base_font(Name(KOREAN_CID_FONT.as_bytes()))
            .system_info(SystemInfo {
                registry: Str(b"Adobe"),
                ordering: Str(b"Korea1"),
                supplement: 1,
            })
            .font_descriptor(self.descriptor)
            .default_width(1000.0);
        cid_font
            .widths()
            .consecutive(1, crate::pdf_text_wrap::cid_ascii_widths());
        cid_font.finish();
        pdf.font_descriptor(self.descriptor)
            .name(Name(KOREAN_CID_FONT.as_bytes()))
            .flags(FontFlags::SYMBOLIC)
            .bbox(Rect::new(-6.0, -145.0, 1003.0, 880.0))
            .italic_angle(0.0)
            .ascent(880.0)
            .descent(-120.0)
            .cap_height(880.0)
            .stem_v(93.0);
        pdf.cmap(self.cmap, TO_UNICODE_CMAP.as_bytes())
            .name(Name(b"Adobe-Identity-UCS"));
    }
}

/// A gradient form: a unit square shaded along its bottom edge from the color `from` to the color `to`, which content
/// stretches over a box (`w 0 0 h x y cm /Name Do`). The function is the straight blend between the two colors.
fn write_gradient(pdf: &mut Pdf, (form, function): (Ref, Ref), (from, to): (u32, u32)) {
    let rgb = |c: u32| [16, 8, 0].map(|shift| ((c >> shift) & 0xFF) as f32 / 255.0);
    pdf.exponential_function(function)
        .domain([0.0, 1.0])
        .c0(rgb(from))
        .c1(rgb(to))
        .n(1.0);
    let mut xobject = pdf.form_xobject(form, b"/Sh sh");
    xobject.bbox(Rect::new(0.0, 0.0, 1.0, 1.0));
    let mut resources = xobject.resources();
    let mut shadings = resources.shadings();
    let mut shading = shadings.insert(Name(b"Sh")).start::<FunctionShading>();
    shading
        .shading_type(FunctionShadingType::Axial)
        .coords([0.0, 0.0, 1.0, 0.0])
        .function(function)
        .extend([true, true]);
    shading.color_space().device_rgb();
}

fn write_page(
    pdf: &mut Pdf,
    entry: &PdfPageEntry,
    plan: &PagePlan,
    fonts: &FontObjects,
    watermark: Option<&str>,
) {
    let a4 = Rect::new(
        0.0,
        0.0,
        PDF_PAGE_WIDTH_PT as f32,
        PDF_PAGE_HEIGHT_PT as f32,
    );
    let (page_number, rotation, media_box, body) = match (entry, plan.image) {
        (PdfPageEntry::Laid(page), _) => {
            let mut content = page.content.clone();
            if let Some(wm) = watermark {
                let (font_tag, operand) = tj_operand(&format!("[WM] {wm}"));
                content.push_str(&format!(
                    "\nBT {font_tag} 10 Tf 36 {:.2} Td {operand} Tj ET",
                    page.height_pt - 30.0
                ));
            }
            let media_box = Rect::new(0.0, 0.0, page.width_pt as f32, page.height_pt as f32);
            (
                page.page_number,
                i32::from(page.rotation_degrees),
                media_box,
                content,
            )
        }
        (PdfPageEntry::Text(page, annotations), _) => (
            page.page_number,
            page.rotation_degrees,
            a4,
            page_content_stream(page, watermark, !annotations.is_empty()),
        ),
        (PdfPageEntry::Image(page), Some(_)) => (
            page.page_number,
            i32::from(page.rotation_degrees),
            a4,
            image_content_stream(page),
        ),
        (PdfPageEntry::Image(_), None) => return,
    };
    {
        let mut page = pdf.page(plan.page);
        page.parent(PAGES_REF).pair(
            Name(b"PageNumber"),
            i32::try_from(page_number).unwrap_or(i32::MAX),
        );
        page.rotate(rotation).media_box(media_box);
        let mut resources = page.resources();
        fonts.resources(&mut resources);
        plan.xobject_resources(entry, &mut resources);
        resources.finish();
        page.contents(plan.content);
        if !plan.annotations.is_empty() {
            page.annotations(plan.annotations.iter().map(|&(annot, _)| annot));
        }
    }
    if matches!(entry, PdfPageEntry::Laid(page) if page.deflate) {
        pdf.stream(plan.content, &crate::deflate_zlib(body.as_bytes()))
            .filter(Filter::FlateDecode);
    } else {
        pdf.stream(plan.content, body.as_bytes());
    }
    match entry {
        PdfPageEntry::Image(page) => {
            if let Some(image) = plan.image {
                page.image.write(pdf, image, plan.mask);
            }
        }
        PdfPageEntry::Laid(page) => {
            for ((_, image), &(image_ref, mask_ref)) in page.xobjects.iter().zip(&plan.xobjects) {
                image.write(pdf, image_ref, mask_ref);
            }
            for ((_, from, to), &(form, function)) in page.gradients.iter().zip(&plan.gradients) {
                write_gradient(pdf, (form, function), (*from, *to));
            }
        }
        PdfPageEntry::Text(_, annotations) => {
            for (annotation, &(annot_ref, ap_ref)) in annotations.iter().zip(&plan.annotations) {
                write_annotation(pdf, annot_ref, ap_ref, plan.page, annotation);
            }
        }
    }
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

    #[test]
    fn hostile_oversized_image_height_does_not_underflow() {
        let page = super::PdfImagePage {
            page_number: 1,
            rotation_degrees: 0,
            image: crate::pdf_image::PdfImageObject {
                width: 1,
                height: 1,
                gray: true,
                bits: 8,
                codec: crate::pdf_image::ImageCodec::Dct,
                data: Vec::new(),
                soft_mask: None,
            },
            width_pt: 500,
            height_pt: 2_000,
        };
        let s = super::image_content_stream(&page);
        assert!(s.contains(" 0 cm /Im1 Do Q"), "{s}");
    }
}

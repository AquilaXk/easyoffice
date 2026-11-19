//! Pages with their own size and placed text, for the engine's own PDF (the one written where no page renderer is
//! available): a document with its page setup, header, footer and page numbers, and a sheet with its page setup
//! (paper, orientation, margins, scale or fit, print area, repeated rows and columns, grid lines, headings, centring)
//! (PD-213). The text is the same two fonts as the plain text pages (`pdf_writer.rs`); a line's width is estimated from
//! the same glyph units as the line breaker, so a line is cut where its column ends and a number sits at the right.

use std::collections::HashMap;
use std::fmt::Write;

use crate::pdf_text_wrap::wrap_spans;
use crate::pdf_writer::{mixed_units, text_ops, PdfLaidPage, PdfPageEntry, PDF_TEXT_FONT_PT};
use crate::rich_document::{
    RichAlignment, RichBindingKind, RichDocument, RichField, RichParagraph,
};
use crate::rich_writers::render_plain_text;
use crate::sheet_page::{PageScale, SheetPage};
use crate::{EasyOfficeError, RichPaper, SheetStyling};

const TWIPS_PER_PT: f64 = 20.0;
const PT_PER_MM: f64 = 72.0 / 25.4;
const PT_PER_PX: f64 = 0.75;
const PT_PER_MIL: f64 = 0.072;
const BODY_LINE_PITCH_PT: f64 = 16.0;
const FRAME_FONT_PT: f64 = 10.0;
const FRAME_LINE_PT: f64 = 12.0;
/// The sizes the sheet window draws with (`sheet-grid.js`: `DEFAULT_COL_W`, `ROW_H`), in CSS px.
pub(crate) const DEFAULT_COL_PX: f64 = 96.0;
pub(crate) const ROW_PX: f64 = 24.0;
const HEADING_COL_PX: f64 = 36.0;
const HEADING_ROW_PX: f64 = 22.0;
const SHEET_FONT_PT: f64 = 10.0;
/// The width of every grid line, in points.
const GRID_WIDTH_PT: f64 = 0.5;
const CELL_PAD_PT: f64 = 4.0;
const DEFAULT_SHEET_MARGIN_MM: f64 = 10.0;
const WIDE_SHEET_COLUMNS: u32 = 8;
const MIN_SCALE: f64 = 0.1;
/// Points per CSS pixel (72 pt/in / 96 px/in).
pub(crate) const PDF_PT_PER_PX: f64 = PT_PER_PX;
/// A fill with a luminance (0..=255) up to this is dark: its automatic text is white (PD-227).
const DARK_FILL_LUMINANCE: u32 = 62;
/// Pages and printed cells a sheet may make (the page's print caps, `sheet-page-print.js`).
const MAX_SHEET_PAGES: usize = 5_000;
const MAX_SHEET_CELLS: u64 = 200_000;

/// How a number format changes the way a page draws one cell (PD-235): a number sits at the right, the colour of the
/// format's section replaces the cell's text colour, and `*x` repeats `x` over the width the text leaves free
/// (`fill` is the byte offset in the text and the character).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CellShown {
    pub numeric: bool,
    pub color: Option<u32>,
    pub fill: Option<(usize, char)>,
}

/// The cells of one sheet a number format changed, by zero-based row and column.
pub type SheetShown = HashMap<(u32, u32), CellShown>;

/// Most copies of a fill character one cell draws (a column cannot be wider in points than this many glyphs).
const MAX_FILL_REPEAT: usize = 512;

/// `text` with its fill character repeated over the `room` pt the text leaves free.
fn filled_text(text: &str, fill: Option<(usize, char)>, size: f64, room: f64) -> String {
    let Some((at, ch)) = fill.filter(|(at, _)| text.is_char_boundary(*at)) else {
        return text.to_string();
    };
    let glyph = width_pt(ch.encode_utf8(&mut [0u8; 4]), size);
    let free = room - width_pt(text, size);
    let copies = if glyph > 0.0 && free > 0.0 {
        ((free / glyph) as usize).min(MAX_FILL_REPEAT)
    } else {
        0
    };
    let mut out = String::with_capacity(text.len() + copies * ch.len_utf8());
    out.push_str(&text[..at]);
    out.extend(std::iter::repeat_n(ch, copies));
    out.push_str(&text[at..]);
    out
}

/// Text width in points at `size` pt.
pub(crate) fn width_pt(text: &str, size: f64) -> f64 {
    f64::from(mixed_units(text)) * size / 1000.0
}

/// The three operands of `rg` for `0xRRGGBB`.
pub(crate) fn rgb_ops(rgb: u32) -> String {
    let channel = |shift: u32| f64::from((rgb >> shift) & 0xFF) / 255.0;
    format!("{:.3} {:.3} {:.3}", channel(16), channel(8), channel(0))
}

/// The colour of text with no colour of its own on `fill`: white on a dark fill, none (black) otherwise.
fn auto_text_color(fill: Option<u32>) -> Option<u32> {
    let rgb = fill?;
    let (r, g, b) = ((rgb >> 16) & 0xFF, (rgb >> 8) & 0xFF, rgb & 0xFF);
    ((r * 76 + g * 151 + b * 29) / 256 <= DARK_FILL_LUMINANCE).then_some(0xFFFFFF)
}

/// One page being drawn: the content stream in PDF user space (origin bottom left).
pub(crate) struct Canvas {
    width: f64,
    height: f64,
    ops: Vec<String>,
}

impl Canvas {
    fn new(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            ops: Vec::new(),
        }
    }

    pub(crate) fn text(&mut self, x: f64, y: f64, size: f64, text: &str) {
        if text.is_empty() {
            return;
        }
        self.ops.push(text_ops(x, y, size, text));
    }

    /// Text in the colour `rgb` (the fill colour of the path state stays as it was after it).
    pub(crate) fn colored_text(&mut self, x: f64, y: f64, size: f64, text: &str, rgb: u32) {
        if text.is_empty() {
            return;
        }
        self.ops.push(format!(
            "q {} rg {} Q",
            rgb_ops(rgb),
            text_ops(x, y, size, text)
        ));
    }

    /// A filled box with the corner `(x, y)` at the bottom left.
    pub(crate) fn fill_rect(&mut self, x: f64, y: f64, w: f64, h: f64, rgb: u32) {
        self.ops.push(format!(
            "q {} rg {x:.2} {y:.2} {w:.2} {h:.2} re f Q",
            rgb_ops(rgb)
        ));
    }

    pub(crate) fn stroke_rect(&mut self, x: f64, y: f64, w: f64, h: f64, rgb: u32, width: f64) {
        self.ops.push(format!(
            "q {width:.2} w {} RG {x:.2} {y:.2} {w:.2} {h:.2} re S Q",
            rgb_ops(rgb)
        ));
    }

    pub(crate) fn stroke_line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, rgb: u32, width: f64) {
        self.ops.push(format!(
            "q {width:.2} w {} RG {x1:.2} {y1:.2} m {x2:.2} {y2:.2} l S Q",
            rgb_ops(rgb)
        ));
    }

    pub(crate) fn stroke_polyline(&mut self, pts: &[(f64, f64)], rgb: u32, width: f64) {
        if pts.len() < 2 {
            return;
        }
        let mut op = format!(
            "q {width:.2} w 1 J 1 j {} RG {:.2} {:.2} m",
            rgb_ops(rgb),
            pts[0].0,
            pts[0].1
        );
        for &(x, y) in &pts[1..] {
            let _ = write!(op, " {x:.2} {y:.2} l");
        }
        op.push_str(" S Q");
        self.ops.push(op);
    }

    pub(crate) fn fill_polygon(&mut self, pts: &[(f64, f64)], rgb: u32) {
        if pts.len() < 3 {
            return;
        }
        let mut op = format!("q {} rg {:.2} {:.2} m", rgb_ops(rgb), pts[0].0, pts[0].1);
        for &(x, y) in &pts[1..] {
            let _ = write!(op, " {x:.2} {y:.2} l");
        }
        op.push_str(" h f Q");
        self.ops.push(op);
    }

    pub(crate) fn push_raw_op(&mut self, op: String) {
        self.ops.push(op);
    }

    pub(crate) fn push_clip_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        self.ops
            .push(format!("q {x:.2} {y:.2} {w:.2} {h:.2} re W n"));
    }

    pub(crate) fn pop_clip(&mut self) {
        self.ops.push("Q".to_string());
    }

    fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        self.ops
            .push(format!("{x1:.2} {y1:.2} m {x2:.2} {y2:.2} l S"));
    }

    /// The grid lines of one page at one width: every row edge and column edge once, or, without grid lines, the box of the
    /// headings alone.
    fn grid(&mut self, xs: &[f64], ys: &[f64], all: bool, headings: bool) {
        let (Some(&left), Some(&right), Some(&top), Some(&bottom)) =
            (xs.first(), xs.last(), ys.first(), ys.last())
        else {
            return;
        };
        let (cols, rows): (&[f64], &[f64]) = if all {
            (xs, ys)
        } else if headings && xs.len() > 1 && ys.len() > 1 {
            (&xs[..2], &ys[..2])
        } else {
            return;
        };
        self.ops.push(format!("{GRID_WIDTH_PT} w"));
        for &y in rows {
            self.line(left, y, right, y);
        }
        for &x in cols {
            self.line(x, top, x, bottom);
        }
    }

    fn finish(self, rotation_degrees: u16) -> PdfPageEntry {
        PdfPageEntry::Laid(PdfLaidPage {
            page_number: 0,
            rotation_degrees,
            width_pt: self.width,
            height_pt: self.height,
            content: self.ops.join("\n"),
        })
    }
}

// ---- document ---------------------------------------------------------------------------------------

/// The text of a header or footer paragraph for page `number` of `total`.
fn frame_text(p: &RichParagraph, number: usize, total: usize, title: &str) -> String {
    let mut out = String::new();
    for run in &p.runs {
        if crate::rich_notes::is_deleted_run(run) {
            continue;
        }
        match run.field {
            Some(RichField::PageNumber) => {
                let _ = write!(out, "{number}");
            }
            Some(RichField::TotalPages) => {
                let _ = write!(out, "{total}");
            }
            Some(RichField::FileName) => out.push_str(title),
            None => out.push_str(&run.text),
        }
    }
    out
}

/// Whether the engine's document PDF has to lay the pages out itself: the document has a header, a footer or a page setup.
pub(crate) fn needs_laid_pages(doc: &RichDocument) -> bool {
    let f = &doc.page_frame;
    !f.is_empty() || f.margins.is_some() || f.paper.is_some()
}

/// The document's PDF laid out on its own paper when it has a header, a footer or a page setup (the caller has ruled out a
/// highlight and a stamp, which the plain text pages carry).
pub(crate) fn laid_document_pdf(
    doc: &RichDocument,
    options: &crate::PdfWriteOptions,
) -> Option<Vec<u8>> {
    needs_laid_pages(doc).then(|| {
        let entries = laid_document_pages(doc, options.rotation_degrees);
        crate::PdfSerializer::serialize_entries(
            &doc.display_title(),
            options.watermark.as_deref(),
            &entries,
        )
    })
}

/// The pages of a document on its own paper and margins with its header, footer and page numbers.
pub(crate) fn laid_document_pages(doc: &RichDocument, rotation_degrees: u16) -> Vec<PdfPageEntry> {
    let frame = &doc.page_frame;
    let paper = frame.paper.unwrap_or(RichPaper::A4);
    let (w, h) = (
        f64::from(paper.width) / TWIPS_PER_PT,
        f64::from(paper.height) / TWIPS_PER_PT,
    );
    let (top, bottom, header, footer, left, right) = frame
        .margins
        .map_or((1417, 1417, 709, 709, 1080, 1080), |m| {
            (m.top, m.bottom, m.header, m.footer, m.left, m.right)
        });
    let gutter = frame.binding.gutter;
    let (side_gutter, top_gutter) = match frame.binding.kind {
        RichBindingKind::Top => (0, gutter),
        _ => (gutter, 0),
    };
    let to_pt = |twips: u32| f64::from(twips) / TWIPS_PER_PT;
    let (body_left, body_right) = (to_pt(left.saturating_add(side_gutter)), w - to_pt(right));
    let (body_top, body_bottom) = (h - to_pt(top.saturating_add(top_gutter)), to_pt(bottom));
    let text_width = (body_right - body_left).max(1.0);
    let units =
        ((text_width / PDF_TEXT_FONT_PT as f64) * 1000.0).clamp(1.0, u32::MAX as f64) as u32;
    let text = render_plain_text(doc, " | ", false);
    let mut lines: Vec<&str> = Vec::new();
    for line in text.split('\n') {
        let spans = wrap_spans(line, units.max(1));
        lines.extend(spans.into_iter().map(|r| &line[r]));
    }
    let per_page = ((body_top - body_bottom) / BODY_LINE_PITCH_PT)
        .floor()
        .clamp(1.0, usize::MAX as f64) as usize;
    let chunks: Vec<&[&str]> = if lines.is_empty() {
        vec![&[][..]]
    } else {
        lines.chunks(per_page).collect()
    };
    let total = chunks.len();
    let title = doc.display_title();
    let mut pages = Vec::with_capacity(total);
    for (i, chunk) in chunks.iter().enumerate() {
        let mut canvas = Canvas::new(w, h);
        for (n, line) in chunk.iter().enumerate() {
            canvas.text(
                body_left,
                body_top - PDF_TEXT_FONT_PT as f64 - n as f64 * BODY_LINE_PITCH_PT,
                PDF_TEXT_FONT_PT as f64,
                line,
            );
        }
        let place = |canvas: &mut Canvas, p: &RichParagraph, y: f64| {
            let t = frame_text(p, i + 1, total, &title);
            let tw = width_pt(&t, FRAME_FONT_PT);
            let x = match p.alignment {
                Some(RichAlignment::Center) => body_left + (text_width - tw) / 2.0,
                Some(RichAlignment::Right) => body_right - tw,
                _ => body_left,
            };
            canvas.text(x, y, FRAME_FONT_PT, &t);
        };
        let (hdr, ftr) = if i == 0 && frame.first_page_different {
            (&frame.first_header, &frame.first_footer)
        } else if (i + 1) % 2 == 0 && frame.even_odd_different {
            (&frame.even_header, &frame.even_footer)
        } else {
            (&frame.header, &frame.footer)
        };
        for (n, p) in hdr.iter().enumerate() {
            place(
                &mut canvas,
                p,
                h - to_pt(header) - FRAME_FONT_PT - n as f64 * FRAME_LINE_PT,
            );
        }
        let last = ftr.len().saturating_sub(1);
        for (n, p) in ftr.iter().enumerate() {
            place(
                &mut canvas,
                p,
                to_pt(footer) + (last - n) as f64 * FRAME_LINE_PT,
            );
        }
        pages.push(canvas.finish(rotation_degrees));
    }
    for (i, page) in pages.iter_mut().enumerate() {
        if let PdfPageEntry::Laid(p) = page {
            p.page_number = i + 1;
        }
    }
    pages
}

// ---- sheet ------------------------------------------------------------------------------------------

/// A zero-based inclusive range of a sheet to print.
#[derive(Clone, Copy)]
struct Area {
    r1: u32,
    c1: u32,
    r2: u32,
    c2: u32,
}

/// Cuts `sizes` into groups of at most `budget`; a line wider than the budget has a group of its own. The groups after the
/// first carry the repeated lines `title` (indexes into `sizes`) when those lie before the group (the page's cut).
fn cut(sizes: &[f64], budget: f64, title: Option<(usize, usize)>) -> Vec<(usize, usize, bool)> {
    let mut prefix = Vec::with_capacity(sizes.len() + 1);
    prefix.push(0.0);
    for s in sizes {
        prefix.push(prefix.last().copied().unwrap_or(0.0) + s);
    }
    let title_size = title.map_or(0.0, |(a, b)| prefix[b + 1] - prefix[a]);
    let mut groups = Vec::new();
    let mut i = 0;
    while i < sizes.len() {
        let repeated = !groups.is_empty() && title.is_some_and(|(_, b)| i > b);
        let room = budget - if repeated { title_size } else { 0.0 };
        let mut j = i + 1;
        while j < sizes.len() && prefix[j + 1] - prefix[i] <= room {
            j += 1;
        }
        groups.push((i, j - 1, repeated));
        i = j;
    }
    groups
}

fn lines_of(group: (usize, usize, bool), title: Option<(usize, usize)>) -> Vec<usize> {
    let mut out = Vec::new();
    if let (true, Some((a, b))) = (group.2, title) {
        out.extend(a..=b);
    }
    out.extend(group.0..=group.1);
    out
}

fn is_number(text: &str) -> bool {
    let t = text.trim().replace(',', "");
    !t.is_empty() && t.parse::<f64>().is_ok()
}

/// `text` cut at the end so that it is at most `width` pt wide.
pub(crate) fn fit_text(text: &str, size: f64, width: f64) -> String {
    let mut out = String::new();
    for c in text.chars() {
        out.push(c);
        if width_pt(&out, size) > width {
            out.pop();
            break;
        }
    }
    out
}

fn col_name(col: u32) -> String {
    let mut buf = [0u8; 4];
    crate::rtf_xml_helpers::write_col_letters(&mut buf, col as usize).to_string()
}

fn in_runs(runs: &[(u32, u32)], i: u32) -> bool {
    runs.iter().any(|&(a, b)| a <= i && i <= b)
}

/// The PDF of a workbook as its page setups print it. `sheets` are the displayed cells of each sheet (zero-based row and
/// column); a sheet without a setup prints its used range on A4 with 10 mm margins, turned when it is wide.
pub(crate) fn encode_sheet_pdf(
    title: &str,
    sheets: &[crate::OdsUnpacked],
    stylings: &[SheetStyling],
    shown: &[SheetShown],
) -> Result<Vec<u8>, EasyOfficeError> {
    crate::sheet_styling::sheet_chart::validate_workbook_charts(stylings)?;
    let cell_index = crate::sheet_styling::sheet_chart::SheetCellIndex::build(sheets);
    let mut entries: Vec<PdfPageEntry> = Vec::new();
    let mut printed_cells = 0u64;
    for (index, (sheet_name, cells)) in sheets.iter().enumerate() {
        let styling = stylings.get(index);
        let page = styling.map(|s| s.page.clone()).unwrap_or_default();
        let by_cell: HashMap<(u32, u32), &str> = cells
            .iter()
            .map(|(r, c, t)| ((*r, *c), t.as_str()))
            .collect();
        let mut used = cells.iter().fold(None, |acc: Option<Area>, &(r, c, _)| {
            Some(acc.map_or(
                Area {
                    r1: r,
                    c1: c,
                    r2: r,
                    c2: c,
                },
                |a| Area {
                    r1: a.r1.min(r),
                    c1: a.c1.min(c),
                    r2: a.r2.max(r),
                    c2: a.c2.max(c),
                },
            ))
        });
        if page.print_area.is_empty() {
            if let Some(st) = styling {
                for ch in &st.charts {
                    let (fc, fr, _, _, tc, tr, _, _) = ch.anchor.effective_two_cell(&st.columns);
                    let next = used.map_or(
                        Area {
                            r1: fr,
                            c1: fc,
                            r2: tr,
                            c2: tc,
                        },
                        |a| Area {
                            r1: a.r1.min(fr),
                            c1: a.c1.min(fc),
                            r2: a.r2.max(tr),
                            c2: a.c2.max(tc),
                        },
                    );
                    let rows = u64::from(next.r2.saturating_sub(next.r1)).saturating_add(1);
                    let cols = u64::from(next.c2.saturating_sub(next.c1)).saturating_add(1);
                    if rows.saturating_mul(cols) <= MAX_SHEET_CELLS {
                        used = Some(next);
                    }
                }
            }
        }
        let areas: Vec<Area> = if page.print_area.is_empty() {
            used.into_iter().collect()
        } else {
            page.print_area
                .iter()
                .map(|b| Area {
                    r1: b.row0,
                    c1: b.col0,
                    r2: b.row1,
                    c2: b.col1,
                })
                .collect()
        };
        let wide = used.is_some_and(|u| {
            u.c2.checked_sub(u.c1)
                .and_then(|d| d.checked_add(1))
                .is_some_and(|w| w > WIDE_SHEET_COLUMNS)
        });
        let setup = if page.is_default() && wide {
            SheetPage {
                landscape: true,
                ..SheetPage::default()
            }
        } else {
            page.clone()
        };
        for area in areas {
            let (Some(rows), Some(cols)) = (
                area.r2.checked_sub(area.r1).and_then(|d| d.checked_add(1)),
                area.c2.checked_sub(area.c1).and_then(|d| d.checked_add(1)),
            ) else {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "인쇄 범위가 올바르지 않아 PDF로 저장할 수 없습니다.".to_string(),
                ));
            };
            let Some(next_cells) = u64::from(rows)
                .checked_mul(u64::from(cols))
                .and_then(|n| printed_cells.checked_add(n))
            else {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "인쇄 범위가 너무 커서 PDF로 저장할 수 없습니다.".to_string(),
                ));
            };
            printed_cells = next_cells;
            if printed_cells > MAX_SHEET_CELLS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "인쇄 범위가 너무 커서 PDF로 저장할 수 없습니다.".to_string(),
                ));
            }
            sheet_area_pages(
                &mut entries,
                area,
                &setup,
                sheet_name,
                sheets,
                &cell_index,
                (styling, shown.get(index)),
                &by_cell,
                page.is_default(),
            )?;
        }
    }
    if entries.is_empty() {
        entries.push(Canvas::new(595.0, 842.0).finish(0));
    }
    for (i, entry) in entries.iter_mut().enumerate() {
        if let PdfPageEntry::Laid(p) = entry {
            p.page_number = i + 1;
        }
    }
    Ok(crate::PdfSerializer::serialize_entries(
        title, None, &entries,
    ))
}

#[allow(clippy::too_many_arguments)]
fn sheet_area_pages(
    entries: &mut Vec<PdfPageEntry>,
    area: Area,
    page: &SheetPage,
    sheet_name: &str,
    sheets: &[crate::OdsUnpacked],
    cell_index: &crate::sheet_styling::sheet_chart::SheetCellIndex<'_>,
    (styling, shown): (Option<&SheetStyling>, Option<&SheetShown>),
    cells: &HashMap<(u32, u32), &str>,
    unset: bool,
) -> Result<(), EasyOfficeError> {
    let (pw, ph) = page.paper_mm();
    let (paper_w, paper_h) = if page.landscape { (ph, pw) } else { (pw, ph) };
    let (paper_w, paper_h) = (paper_w * PT_PER_MM, paper_h * PT_PER_MM);
    let (m_top, m_bottom, m_left, m_right) = match (page.margins, unset) {
        (Some(m), _) => (
            f64::from(m.top) * PT_PER_MIL,
            f64::from(m.bottom) * PT_PER_MIL,
            f64::from(m.left) * PT_PER_MIL,
            f64::from(m.right) * PT_PER_MIL,
        ),
        _ => {
            let d = DEFAULT_SHEET_MARGIN_MM * PT_PER_MM;
            (d, d, d, d)
        }
    };
    let box_w = paper_w - m_left - m_right;
    let box_h = paper_h - m_top - m_bottom;
    if !box_w.is_finite() || box_w <= 0.0 || !box_h.is_finite() || box_h <= 0.0 {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "PDF 여백이 용지 크기를 초과하여 저장할 수 없습니다.".to_string(),
        ));
    }
    let hidden_rows = styling.map_or(&[][..], |s| &s.hidden_rows[..]);
    let hidden_cols = styling.map_or(&[][..], |s| &s.hidden_cols[..]);
    let col_sizes: Vec<f64> = (area.c1..=area.c2)
        .map(|c| {
            if in_runs(hidden_cols, c) {
                0.0
            } else {
                styling
                    .and_then(|s| s.columns.get(&c))
                    .map_or(DEFAULT_COL_PX, |px| f64::from(*px))
                    * PT_PER_PX
            }
        })
        .collect();
    let row_sizes: Vec<f64> = (area.r1..=area.r2)
        .map(|r| {
            if styling.is_some_and(|s| s.row_hidden(r)) || in_runs(hidden_rows, r) {
                0.0
            } else {
                ROW_PX * PT_PER_PX
            }
        })
        .collect();
    let (head_w, head_h) = if page.headings {
        (HEADING_COL_PX * PT_PER_PX, HEADING_ROW_PX * PT_PER_PX)
    } else {
        (0.0, 0.0)
    };
    let total_w = col_sizes.iter().sum::<f64>() + head_w;
    let total_h = row_sizes.iter().sum::<f64>() + head_h;
    let scale = match page.scale {
        PageScale::Percent(0) => {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "PDF 인쇄 배율은 0보다 커야 합니다.".to_string(),
            ));
        }
        PageScale::Percent(p) => (f64::from(p) / 100.0).max(MIN_SCALE),
        PageScale::Fit { width, height } => {
            let mut fit: f64 = 1.0;
            if width > 0 && total_w > 0.0 {
                fit = fit.min(f64::from(width) * box_w / total_w);
            }
            if height > 0 && total_h > 0.0 {
                fit = fit.min(f64::from(height) * box_h / total_h);
            }
            fit.max(MIN_SCALE)
        }
    };
    let title = |run: Option<(u32, u32)>, first: u32, len: usize| {
        run.filter(|&(a, b)| b >= a && a >= first)
            .and_then(|(a, b)| {
                let end = b.checked_sub(first)? as usize;
                (end < len).then_some(((a - first) as usize, end))
            })
    };
    let title_rows = title(page.title_rows, area.r1, row_sizes.len());
    let title_cols = title(page.title_cols, area.c1, col_sizes.len());
    let avail_w = box_w / scale - head_w;
    let avail_h = box_h / scale - head_h;
    if !avail_w.is_finite() || avail_w <= 0.0 || !avail_h.is_finite() || avail_h <= 0.0 {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "PDF 인쇄 영역이 너무 좁아 저장할 수 없습니다.".to_string(),
        ));
    }
    let col_groups = cut(&col_sizes, avail_w, title_cols);
    let row_groups = cut(&row_sizes, avail_h, title_rows);
    let Some(total_pages) = col_groups
        .len()
        .checked_mul(row_groups.len())
        .and_then(|n| entries.len().checked_add(n))
    else {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "인쇄 범위가 너무 커서 PDF로 저장할 수 없습니다.".to_string(),
        ));
    };
    if total_pages > MAX_SHEET_PAGES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "인쇄 범위가 너무 커서 PDF로 저장할 수 없습니다.".to_string(),
        ));
    }
    for cg in &col_groups {
        let cols = lines_of(*cg, title_cols);
        for rg in &row_groups {
            let rows = lines_of(*rg, title_rows);
            let content_w = (cols.iter().map(|&i| col_sizes[i]).sum::<f64>() + head_w) * scale;
            let content_h = (rows.iter().map(|&i| row_sizes[i]).sum::<f64>() + head_h) * scale;
            let x0 = m_left
                + if page.center_horizontal {
                    (box_w - content_w) / 2.0
                } else {
                    0.0
                };
            let y_top = paper_h
                - m_top
                - if page.center_vertical {
                    (box_h - content_h) / 2.0
                } else {
                    0.0
                };
            let mut canvas = Canvas::new(paper_w, paper_h);
            let font = SHEET_FONT_PT * scale;
            let pad = CELL_PAD_PT * scale;
            let mut y = y_top;
            // The edges the cells share are drawn once: one line per row edge and per column edge, across the whole page.
            let mut xs = vec![x0];
            let mut ys = vec![y_top];
            let mut xs_done = false;
            if head_w > 0.0 {
                xs.push(x0 + head_w * scale);
            }
            if head_h > 0.0 {
                let h = head_h * scale;
                let mut x = x0 + head_w * scale;
                for &i in &cols {
                    let w = col_sizes[i] * scale;
                    if w <= 0.0 {
                        continue;
                    }
                    let name = col_name(area.c1 + i as u32);
                    canvas.text(
                        x + (w - width_pt(&name, font)) / 2.0,
                        y - h + (h - font) / 2.0 + 1.0,
                        font,
                        &name,
                    );
                    x += w;
                }
                y -= h;
                ys.push(y);
            }
            let mut page_cols: Vec<u32> = Vec::with_capacity(cols.len());
            let mut page_rows: Vec<u32> = Vec::with_capacity(rows.len());
            for &i in &rows {
                let h = row_sizes[i] * scale;
                if h <= 0.0 {
                    continue;
                }
                let r = area.r1 + i as u32;
                page_rows.push(r);
                let mut x = x0;
                if head_w > 0.0 {
                    let label = (r + 1).to_string();
                    canvas.text(
                        x + (head_w * scale - width_pt(&label, font)) / 2.0,
                        y - h + (h - font) / 2.0 + 1.0,
                        font,
                        &label,
                    );
                    x += head_w * scale;
                }
                for &k in &cols {
                    let w = col_sizes[k] * scale;
                    if w <= 0.0 {
                        continue;
                    }
                    let c = area.c1 + k as u32;
                    if !xs_done {
                        page_cols.push(c);
                    }
                    let at = (r, c);
                    let look = styling.and_then(|s| s.cells.get(&at));
                    let fill = look.and_then(|style| style.fill);
                    if let Some(rgb) = fill {
                        canvas.fill_rect(x, y - h, w, h, rgb);
                    }
                    if let Some(text) = cells.get(&at) {
                        let how_opt = shown.and_then(|m| m.get(&at)).copied();
                        let how = how_opt.unwrap_or_default();
                        let room = w - 2.0 * pad;
                        let drawn = fit_text(&filled_text(text, how.fill, font, room), font, room);
                        let tw = width_pt(&drawn, font);
                        let right_aligned = match how_opt {
                            Some(cell) => cell.numeric,
                            None => styling
                                .and_then(|s| s.explicit_cell_kind(at.0, at.1, text))
                                .map_or_else(
                                    || is_number(text),
                                    |k| k == crate::CellValueKind::Number,
                                ),
                        };
                        let tx = if right_aligned {
                            x + w - pad - tw
                        } else {
                            x + pad
                        };
                        let ty = y - h + (h - font) / 2.0 + 1.0;
                        match how
                            .color
                            .or_else(|| look.and_then(|style| style.font_color))
                            .or_else(|| auto_text_color(fill))
                        {
                            Some(rgb) => canvas.colored_text(tx, ty, font, &drawn, rgb),
                            None => canvas.text(tx, ty, font, &drawn),
                        }
                    }
                    x += w;
                    if !xs_done {
                        xs.push(x);
                    }
                }
                xs_done = true;
                y -= h;
                ys.push(y);
            }
            canvas.grid(&xs, &ys, page.grid_lines, page.headings);
            if let Some(st) = styling {
                if !st.charts.is_empty() {
                    let col_xs = if head_w > 0.0 && xs.len() > 1 {
                        &xs[1..]
                    } else {
                        &xs[..]
                    };
                    let row_ys = if head_h > 0.0 && ys.len() > 1 {
                        &ys[1..]
                    } else {
                        &ys[..]
                    };
                    crate::pdf_chart::draw_sheet_charts(
                        &mut canvas,
                        sheet_name,
                        sheets,
                        cell_index,
                        st,
                        &page_cols,
                        &page_rows,
                        col_xs,
                        row_ys,
                        scale,
                    );
                }
            }
            entries.push(canvas.finish(0));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheet_page::PageMargins;
    use crate::{CellBox, RichBinding, RichPageFrame, RichPageMargins};

    fn one_cell_sheet() -> Vec<crate::OdsUnpacked> {
        vec![("S1".to_string(), vec![(0, 0, "a".to_string())])]
    }

    #[test]
    fn hostile_zero_percent_scale_and_excess_margins_fail_closed() {
        let mut st = SheetStyling::default();
        st.page.scale = PageScale::Percent(0);
        assert!(encode_sheet_pdf("t", &one_cell_sheet(), &[st], &[]).is_err());

        let mut st_margin = SheetStyling::default();
        st_margin.page.margins = Some(PageMargins {
            left: 10_000,
            right: 10_000,
            top: 1_000,
            bottom: 1_000,
            header: 0,
            footer: 0,
        });
        assert!(encode_sheet_pdf("t", &one_cell_sheet(), &[st_margin], &[]).is_err());
    }

    #[test]
    fn hostile_inverted_or_max_print_area_and_title_fail_closed_without_panic() {
        let mut inverted = SheetStyling::default();
        inverted.page.print_area = vec![CellBox {
            row0: 5,
            col0: 0,
            row1: 2,
            col1: 0,
        }];
        assert!(encode_sheet_pdf("t", &one_cell_sheet(), &[inverted], &[]).is_err());

        let mut max_row = SheetStyling::default();
        max_row.page.print_area = vec![CellBox {
            row0: 0,
            col0: 0,
            row1: u32::MAX,
            col1: 0,
        }];
        assert!(encode_sheet_pdf("t", &one_cell_sheet(), &[max_row], &[]).is_err());

        let mut bad_title = SheetStyling::default();
        bad_title.page.print_area = vec![CellBox {
            row0: 5,
            col0: 0,
            row1: 10,
            col1: 2,
        }];
        bad_title.page.title_rows = Some((10, 2));
        let res = encode_sheet_pdf("t", &one_cell_sheet(), &[bad_title], &[]);
        assert!(res.is_ok());

        let doc = RichDocument {
            page_frame: RichPageFrame {
                margins: Some(RichPageMargins {
                    top: u32::MAX,
                    bottom: 0,
                    header: 0,
                    footer: 0,
                    left: u32::MAX,
                    right: 0,
                }),
                binding: RichBinding {
                    gutter: 100,
                    kind: RichBindingKind::Single,
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let pages = laid_document_pages(&doc, 0);
        assert_eq!(pages.len(), 1);
    }
}

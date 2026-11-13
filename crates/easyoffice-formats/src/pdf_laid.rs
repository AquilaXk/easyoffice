//! Pages with their own size and placed text, for the engine's own PDF (the one written where no page renderer is
//! available): a document with its page setup, header, footer and page numbers, and a sheet with its page setup
//! (paper, orientation, margins, scale or fit, print area, repeated rows and columns, grid lines, headings, centring)
//! (PD-213). The text is the same two fonts as the plain text pages (`pdf_writer.rs`); a line's width is estimated from
//! the same glyph units as the line breaker, so a line is cut where its column ends and a number sits at the right.

use std::collections::HashMap;

use crate::pdf_text_wrap::{text_units, wrap_spans};
use crate::pdf_writer::{tj_operand, PdfLaidPage, PdfPageEntry, PDF_TEXT_FONT_PT};
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
const DEFAULT_COL_PX: f64 = 96.0;
const ROW_PX: f64 = 24.0;
const HEADING_COL_PX: f64 = 36.0;
const HEADING_ROW_PX: f64 = 22.0;
const SHEET_FONT_PT: f64 = 10.0;
const CELL_PAD_PT: f64 = 4.0;
const DEFAULT_SHEET_MARGIN_MM: f64 = 10.0;
const WIDE_SHEET_COLUMNS: u32 = 8;
const MIN_SCALE: f64 = 0.1;
/// Pages and printed cells a sheet may make (the page's print caps, `sheet-page-print.js`).
const MAX_SHEET_PAGES: usize = 5_000;
const MAX_SHEET_CELLS: u64 = 200_000;

/// Text width in points at `size` pt.
fn width_pt(text: &str, size: f64) -> f64 {
    f64::from(text_units(text)) * size / 1000.0
}

/// One page being drawn: the content stream in PDF user space (origin bottom left).
struct Canvas {
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

    fn text(&mut self, x: f64, y: f64, size: f64, text: &str) {
        if text.is_empty() {
            return;
        }
        let (font, operand) = tj_operand(text);
        self.ops.push(format!(
            "BT {font} {size:.2} Tf {x:.2} {y:.2} Td {operand} Tj ET"
        ));
    }

    fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        self.ops
            .push(format!("{x1:.2} {y1:.2} m {x2:.2} {y2:.2} l S"));
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
    p.runs
        .iter()
        .map(|run| match run.field {
            Some(RichField::PageNumber) => number.to_string(),
            Some(RichField::TotalPages) => total.to_string(),
            Some(RichField::FileName) => title.to_string(),
            None => run.text.clone(),
        })
        .collect()
}

/// Whether the engine's document PDF has to lay the pages out itself: the document has a header, a footer or a page setup.
pub(crate) fn needs_laid_pages(doc: &RichDocument) -> bool {
    let f = &doc.page_frame;
    !f.is_empty() || f.margins.is_some() || f.paper.is_some()
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
    let (body_left, body_right) = (to_pt(left + side_gutter), w - to_pt(right));
    let (body_top, body_bottom) = (h - to_pt(top + top_gutter), to_pt(bottom));
    let text_width = (body_right - body_left).max(1.0);
    let units = (text_width / PDF_TEXT_FONT_PT as f64 * 1000.0) as u32;
    let text = render_plain_text(doc, " | ", false);
    let mut lines: Vec<&str> = Vec::new();
    for line in text.split('\n') {
        let spans = wrap_spans(line, units.max(1));
        lines.extend(spans.into_iter().map(|r| &line[r]));
    }
    let per_page = (((body_top - body_bottom) / BODY_LINE_PITCH_PT).floor() as usize).max(1);
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
        for (n, p) in frame.header.iter().enumerate() {
            place(
                &mut canvas,
                p,
                h - to_pt(header) - FRAME_FONT_PT - n as f64 * FRAME_LINE_PT,
            );
        }
        let last = frame.footer.len().saturating_sub(1);
        for (n, p) in frame.footer.iter().enumerate() {
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
fn fit_text(text: &str, size: f64, width: f64) -> String {
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
) -> Result<Vec<u8>, EasyOfficeError> {
    let mut entries: Vec<PdfPageEntry> = Vec::new();
    let mut printed_cells = 0u64;
    for (index, (_, cells)) in sheets.iter().enumerate() {
        let styling = stylings.get(index);
        let page = styling.map(|s| s.page.clone()).unwrap_or_default();
        let by_cell: HashMap<(u32, u32), &str> = cells
            .iter()
            .map(|(r, c, t)| ((*r, *c), t.as_str()))
            .collect();
        let used = cells.iter().fold(None, |acc: Option<Area>, &(r, c, _)| {
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
        let wide = used.is_some_and(|u| u.c2 - u.c1 + 1 > WIDE_SHEET_COLUMNS);
        let setup = if page.is_default() && wide {
            SheetPage {
                landscape: true,
                ..SheetPage::default()
            }
        } else {
            page.clone()
        };
        for area in areas {
            printed_cells += u64::from(area.r2 - area.r1 + 1) * u64::from(area.c2 - area.c1 + 1);
            if printed_cells > MAX_SHEET_CELLS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "인쇄 범위가 너무 커서 PDF로 저장할 수 없습니다.".to_string(),
                ));
            }
            sheet_area_pages(
                &mut entries,
                area,
                &setup,
                styling,
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

fn sheet_area_pages(
    entries: &mut Vec<PdfPageEntry>,
    area: Area,
    page: &SheetPage,
    styling: Option<&SheetStyling>,
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
        PageScale::Percent(p) => f64::from(p) / 100.0,
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
        run.filter(|&(a, b)| a >= first && ((b - first) as usize) < len)
            .map(|(a, b)| ((a - first) as usize, (b - first) as usize))
    };
    let title_rows = title(page.title_rows, area.r1, row_sizes.len());
    let title_cols = title(page.title_cols, area.c1, col_sizes.len());
    let col_groups = cut(&col_sizes, box_w / scale - head_w, title_cols);
    let row_groups = cut(&row_sizes, box_h / scale - head_h, title_rows);
    if entries.len() + col_groups.len() * row_groups.len() > MAX_SHEET_PAGES {
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
            let draw_grid = page.grid_lines || page.headings;
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
            }
            for &i in &rows {
                let h = row_sizes[i] * scale;
                if h <= 0.0 {
                    continue;
                }
                let r = area.r1 + i as u32;
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
                    if let Some(text) = cells.get(&(r, area.c1 + k as u32)) {
                        let shown = fit_text(text, font, w - 2.0 * pad);
                        let tw = width_pt(&shown, font);
                        let tx = if is_number(text) {
                            x + w - pad - tw
                        } else {
                            x + pad
                        };
                        canvas.text(tx, y - h + (h - font) / 2.0 + 1.0, font, &shown);
                    }
                    if page.grid_lines {
                        canvas.line(x, y - h, x + w, y - h);
                        canvas.line(x + w, y, x + w, y - h);
                    }
                    x += w;
                }
                if page.grid_lines {
                    canvas.line(x0 + head_w * scale, y, x, y);
                }
                y -= h;
            }
            if draw_grid {
                canvas.line(x0, y_top, x0 + content_w, y_top);
            }
            entries.push(canvas.finish(0));
        }
    }
    Ok(())
}

//! Laid-out document pages for the engine's own PDF when a document carries a header, footer, or page setup (PD-213).

use std::fmt::Write;

use super::{width_pt, Canvas, TWIPS_PER_PT};
use crate::pdf_text_wrap::wrap_spans;
use crate::pdf_writer::{PdfPageEntry, PDF_TEXT_FONT_PT};
use crate::rich_document::{
    RichAlignment, RichBindingKind, RichDocument, RichField, RichParagraph,
};
use crate::rich_writers::render_plain_text;
use crate::RichPaper;

const BODY_LINE_PITCH_PT: f64 = 16.0;
const FRAME_FONT_PT: f64 = 10.0;
const FRAME_LINE_PT: f64 = 12.0;

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
        .map_or((1701, 1440, 851, 992, 1440, 1440), |m| {
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

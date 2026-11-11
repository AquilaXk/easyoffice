//! Highlight and underline annotations for the text pages of the PDF writer.
//!
//! Text markup annotations follow ISO 32000-1 §12.5.6.10: `/Subtype /Highlight` or `/Underline`,
//! `/Rect`, `/QuadPoints` (one quadrilateral per marked line, listed top-left, top-right,
//! bottom-left, bottom-right as viewers write them), `/C`, `/Contents` and `/F 4`, plus an
//! appearance stream `/AP /N` so a viewer that does not draw its own shows the same marks.
//!
//! Reference implementations (structure only, no code copied): the annotation dictionary keys and
//! `AnnotationType::Highlight` / `Underline` of pdf-writer 0.15 (MIT OR Apache-2.0,
//! `src/annotations.rs` lines 172-287, `quad_points`), and the highlight appearance of the pdf.js
//! editor (Apache-2.0), a rectangle filled with the color under `/BM /Multiply` so the text stays
//! readable. The engine has no crate dependencies, so the writer is written here.
//!
//! The phrase is searched in the source lines (before the lines are broken), so an occurrence
//! that crosses a line or page break is found; each output line it touches gets its own quad,
//! placed from the same glyph advances the line breaking uses. Searching is linear in the text
//! and at most [`MAX_HIGHLIGHT_ANNOTATIONS`] annotations are written.

use crate::pdf_text_wrap::units_before;
use crate::pdf_writer::{
    pdf_text_string, LaidPage, PdfObjectWriter, PDF_BODY_FIRST_BASELINE_PT, PDF_BODY_LINE_PITCH_PT,
    PDF_TEXT_FONT_PT, PDF_TEXT_MARGIN_PT,
};

/// Most annotations written to one document; the rest of the occurrences are left out.
pub const MAX_HIGHLIGHT_ANNOTATIONS: usize = 10_000;
/// A text quad reaches this far under and over the baseline (the 12 pt font's descent and ascent).
const QUAD_BELOW_BASELINE_PT: f64 = 3.0;
const QUAD_ABOVE_BASELINE_PT: f64 = 10.0;
/// An underline is drawn this far over the bottom of its quad, 1 pt thick.
const UNDERLINE_OFFSET_PT: f64 = 1.5;
const UNDERLINE_WIDTH_PT: f64 = 1.0;
/// `/F` flag "print": the annotation is printed with the page.
const FLAG_PRINT: u8 = 4;

/// What the annotation looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfHighlightKind {
    Highlight,
    Underline,
}

/// A phrase to mark everywhere it occurs, and how.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfHighlight {
    pub text: String,
    pub kind: PdfHighlightKind,
    /// Color as red, green, blue bytes.
    pub color: [u8; 3],
}

/// One rectangle of an annotation: the lower-left and upper-right corner in page points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Quad {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

/// One annotation on a page: an occurrence of the phrase, or the part of it on that page.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PageAnnotation {
    kind: PdfHighlightKind,
    color: [u8; 3],
    contents: String,
    quads: Vec<Quad>,
}

/// The search over one document: the phrase, the annotations still allowed, and what was found.
pub(crate) struct HighlightPass<'a> {
    spec: &'a PdfHighlight,
    remaining: usize,
    found: usize,
    dropped: usize,
}

impl<'a> HighlightPass<'a> {
    pub(crate) fn new(spec: &'a PdfHighlight) -> Self {
        Self::with_limit(spec, MAX_HIGHLIGHT_ANNOTATIONS)
    }

    fn with_limit(spec: &'a PdfHighlight, limit: usize) -> Self {
        Self {
            spec,
            remaining: limit,
            found: 0,
            dropped: 0,
        }
    }

    /// Notes for the caller: nothing found, or occurrences left out over the limit.
    pub(crate) fn warnings(&self) -> Vec<String> {
        if self.found == 0 {
            return vec![
                "형광펜·밑줄 문구를 본문에서 찾지 못해 주석을 쓰지 않았습니다.".to_string(),
            ];
        }
        if self.dropped > 0 {
            return vec![format!(
                "형광펜·밑줄 주석은 문서당 {MAX_HIGHLIGHT_ANNOTATIONS}개까지만 쓰므로 {}곳은 표시하지 않았습니다.",
                self.dropped
            )];
        }
        Vec::new()
    }

    /// The annotations of each laid-out page of one source page (`source_body` is that page's
    /// text before its lines were broken), in the order of `laid`.
    pub(crate) fn annotate(
        &mut self,
        source_body: &str,
        laid: &[LaidPage],
    ) -> Vec<Vec<PageAnnotation>> {
        let mut per_page: Vec<Vec<PageAnnotation>> = vec![Vec::new(); laid.len()];
        let source: Vec<&str> = source_body.split('\n').collect();
        let mut pieces: Vec<Vec<Piece>> = vec![Vec::new(); source.len()];
        for (page_idx, page) in laid.iter().enumerate() {
            for (line_idx, line) in page.lines.iter().enumerate() {
                let rendered = &source[line.source][line.range.clone()];
                pieces[line.source].push(Piece {
                    page: page_idx,
                    line: line_idx,
                    start: line.range.start,
                    end: line.range.start + rendered.trim_end().len(),
                    rendered_start: line.range.start,
                    rendered_end: line.range.end,
                });
            }
        }
        for (text, line_pieces) in source.iter().zip(&pieces) {
            for (at, found) in text.match_indices(self.spec.text.as_str()) {
                self.found += 1;
                let covered = covered_pieces(line_pieces, at, at + found.len());
                let mut parts: Vec<(usize, Vec<Quad>)> = Vec::new();
                for piece in covered {
                    let from = at.max(piece.start);
                    let to = (at + found.len()).min(piece.end);
                    if from >= to {
                        continue;
                    }
                    let rendered = &text[piece.rendered_start..piece.rendered_end];
                    let quad = text_quad(
                        rendered,
                        from - piece.rendered_start,
                        to - piece.rendered_start,
                        piece.line,
                    );
                    match parts.last_mut() {
                        Some((page, quads)) if *page == piece.page => quads.push(quad),
                        _ => parts.push((piece.page, vec![quad])),
                    }
                }
                if parts.len() > self.remaining {
                    self.dropped += 1;
                    continue;
                }
                self.remaining -= parts.len();
                for (page, quads) in parts {
                    per_page[page].push(PageAnnotation {
                        kind: self.spec.kind,
                        color: self.spec.color,
                        contents: self.spec.text.clone(),
                        quads,
                    });
                }
            }
        }
        per_page
    }
}

/// An output line of a source line: where it is and which bytes of the source line it shows
/// (`start..end` without trailing spaces, `rendered_*` as drawn).
#[derive(Clone)]
struct Piece {
    page: usize,
    line: usize,
    start: usize,
    end: usize,
    rendered_start: usize,
    rendered_end: usize,
}

/// The pieces (in order, disjoint) that overlap the byte range `from..to` of their source line.
fn covered_pieces(pieces: &[Piece], from: usize, to: usize) -> &[Piece] {
    let first = pieces.partition_point(|p| p.end <= from);
    let last = first + pieces[first..].partition_point(|p| p.start < to);
    &pieces[first..last]
}

fn text_quad(rendered: &str, from: usize, to: usize, line: usize) -> Quad {
    let at = |offset: usize| {
        PDF_TEXT_MARGIN_PT as f64
            + PDF_TEXT_FONT_PT as f64 * f64::from(units_before(rendered, offset)) / 1000.0
    };
    let baseline =
        f64::from(PDF_BODY_FIRST_BASELINE_PT) - line as f64 * f64::from(PDF_BODY_LINE_PITCH_PT);
    Quad {
        x0: at(from),
        y0: baseline - QUAD_BELOW_BASELINE_PT,
        x1: at(to),
        y1: baseline + QUAD_ABOVE_BASELINE_PT,
    }
}

/// A number as PDF text: at most three decimals, no trailing zeros.
fn num(value: f64) -> String {
    let text = format!("{value:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn color_components(color: [u8; 3]) -> String {
    color
        .iter()
        .map(|c| num(f64::from(*c) / 255.0))
        .collect::<Vec<_>>()
        .join(" ")
}

fn bounding_box(quads: &[Quad]) -> Quad {
    quads.iter().fold(quads[0], |acc, q| Quad {
        x0: acc.x0.min(q.x0),
        y0: acc.y0.min(q.y0),
        x1: acc.x1.max(q.x1),
        y1: acc.y1.max(q.y1),
    })
}

/// Content of the appearance stream: every quad filled (highlight) or underlined.
fn appearance_content(a: &PageAnnotation) -> String {
    let color = color_components(a.color);
    match a.kind {
        PdfHighlightKind::Highlight => {
            let mut content = format!("/GS gs {color} rg");
            for q in &a.quads {
                content.push_str(&format!(
                    "\n{} {} {} {} re f",
                    num(q.x0),
                    num(q.y0),
                    num(q.x1 - q.x0),
                    num(q.y1 - q.y0)
                ));
            }
            content
        }
        PdfHighlightKind::Underline => {
            let mut content = format!("{color} RG {} w", num(UNDERLINE_WIDTH_PT));
            for q in &a.quads {
                let y = num(q.y0 + UNDERLINE_OFFSET_PT);
                content.push_str(&format!("\n{} {y} m {} {y} l S", num(q.x0), num(q.x1)));
            }
            content
        }
    }
}

/// Writes the annotation as object `annot_id` and its appearance stream as `ap_id`.
pub(crate) fn write_annotation(
    w: &mut PdfObjectWriter,
    annot_id: usize,
    ap_id: usize,
    page_obj: usize,
    a: &PageAnnotation,
) {
    let bbox = bounding_box(&a.quads);
    let rect = format!(
        "[{} {} {} {}]",
        num(bbox.x0),
        num(bbox.y0),
        num(bbox.x1),
        num(bbox.y1)
    );
    let quad_points = a
        .quads
        .iter()
        .flat_map(|q| [q.x0, q.y1, q.x1, q.y1, q.x0, q.y0, q.x1, q.y0])
        .map(num)
        .collect::<Vec<_>>()
        .join(" ");
    let subtype = match a.kind {
        PdfHighlightKind::Highlight => "Highlight",
        PdfHighlightKind::Underline => "Underline",
    };
    w.object(
        annot_id,
        &format!(
            "<< /Type /Annot /Subtype /{subtype} /Rect {rect} /QuadPoints [{quad_points}] /C [{}] /F {FLAG_PRINT} /Contents {} /P {page_obj} 0 R /AP << /N {ap_id} 0 R >> >>",
            color_components(a.color),
            pdf_text_string(&a.contents)
        ),
    );
    w.stream(
        ap_id,
        &format!(
            " /Type /XObject /Subtype /Form /BBox {rect} /Resources << /ExtGState << /GS << /Type /ExtGState /BM /Multiply >> >> >>"
        ),
        &appearance_content(a),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_short_and_exact_enough() {
        assert_eq!(num(54.0), "54");
        assert_eq!(num(84.672), "84.672");
        assert_eq!(num(0.1 + 0.2), "0.3");
        assert_eq!(num(-1.5), "-1.5");
    }

    #[test]
    fn colors_are_unit_components() {
        assert_eq!(color_components([255, 255, 0]), "1 1 0");
        assert_eq!(color_components([0, 0, 0]), "0 0 0");
        assert_eq!(color_components([128, 0, 255]), "0.502 0 1");
    }

    #[test]
    fn pieces_are_found_by_range_in_log_time() {
        let piece = |start, end| Piece {
            page: 0,
            line: 0,
            start,
            end,
            rendered_start: start,
            rendered_end: end,
        };
        let pieces = [piece(0, 10), piece(12, 20), piece(21, 30)];
        assert_eq!(covered_pieces(&pieces, 5, 8).len(), 1);
        assert_eq!(covered_pieces(&pieces, 8, 13).len(), 2);
        assert_eq!(
            covered_pieces(&pieces, 10, 12).len(),
            0,
            "a match inside a dropped space"
        );
        assert_eq!(covered_pieces(&pieces, 0, 30).len(), 3);
    }
}

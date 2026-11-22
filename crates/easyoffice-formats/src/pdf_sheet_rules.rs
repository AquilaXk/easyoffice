//! What conditional formatting adds to a sheet page of the engine's own PDF (PD-300): the fill and text color of the
//! rules, the font bits, the edges and the data bar. The rules are evaluated by the engine (`easyoffice-sheet`, the code
//! the grid and the print read) and arrive per cell in [`CellRule`], as the number format's answer arrives in
//! `CellShown`. Icon sets are not drawn: a cell that has one shows its value.
//!
//! The data bar geometry is the grid's (`barBackground` in `sheet-rules.js`): the axis is `start` percent of the cell
//! width, a positive bar grows right from it to `start + (100 - start) * pct / 100`, a negative bar left to
//! `start - start * |pct| / 100`, and a gradient bar fades from its color to a quarter of it at its far end. The PDF has
//! no gradient inside a content stream, so the fade is a few steps of colors mixed with the cell's own fill.

use crate::pdf_laid::Canvas;
use crate::pdf_writer::text_ops;
use crate::{
    BorderEdge, BorderLine, CellBorders, CfBar, FLAG_BOLD, FLAG_ITALIC, FLAG_STRIKE, FLAG_UNDERLINE,
};

/// The look the conditional formats give one cell of a PDF page. A part the rules do not set is none, 0 or the
/// default of its type, and a cell whose rule is all of those looks as it does without one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CellRule {
    /// Fill, `0xRRGGBB`; replaces the fill of the cell itself.
    pub bg: Option<u32>,
    /// Text color, `0xRRGGBB`; replaces the cell's own (a number format's color goes over it).
    pub fg: Option<u32>,
    /// `FLAG_*` bits, added to the cell's own.
    pub flags: u8,
    /// The sides the rules draw (a side with no edge is not drawn).
    pub border: CellBorders,
    pub bar: Option<CfBar>,
    /// The bar shows no value: the cell's text is not drawn.
    pub hide_value: bool,
}

const PAPER: u32 = 0xFF_FFFF;
const RGB_MASK: u32 = 0xFF_FFFF;
/// Steps a gradient bar is cut in, and how far each reaches into the next so no seam shows between them.
const GRADIENT_STEPS: usize = 8;
const STEP_OVERLAP_PT: f64 = 0.25;
/// What is left of a gradient bar's color at its far end.
const GRADIENT_END_ALPHA: f64 = 0.25;
/// A slant of the text of 12 degrees (the tangent), a bold text's outline and the lines of an underline or a
/// strike-through, as parts of the font size.
const ITALIC_SHEAR: f64 = 0.21;
const BOLD_OUTLINE_EM: f64 = 0.03;
const TEXT_LINE_EM: f64 = 0.06;
const UNDERLINE_DROP_EM: f64 = 0.12;
const STRIKE_RISE_EM: f64 = 0.3;
/// Widths of the lines of an edge in points; a dash is the same in points. Everything is drawn at the page's scale.
const THIN_PT: f64 = 0.75;
const THICK_PT: f64 = 1.5;
const MIN_LINE_PT: f64 = 0.25;
const DASH_PT: [f64; 2] = [3.0, 2.0];
const DOT_PT: [f64; 2] = [0.75, 1.5];
const DOUBLE_GAP_PT: f64 = 0.75;

fn channels(rgb: u32) -> [f64; 3] {
    [16, 8, 0].map(|shift| f64::from((rgb >> shift) & 0xFF))
}

/// `color` over `under` at `alpha`.
fn mixed(color: u32, under: u32, alpha: f64) -> u32 {
    let (c, u) = (channels(color), channels(under));
    (0..3).fold(0, |acc, i| {
        let value = (c[i] * alpha + u[i] * (1.0 - alpha))
            .round()
            .clamp(0.0, 255.0) as u32;
        (acc << 8) | value
    })
}

/// The bar over the cell `(x, y, w, h)` (`y` the bottom), drawn before the cell's text. `under` is the cell's fill.
pub(crate) fn draw_bar(
    canvas: &mut Canvas,
    (x, y, w, h): (f64, f64, f64, f64),
    bar: CfBar,
    under: Option<u32>,
) {
    let start = f64::from(bar.start.min(100));
    let mag = f64::from(bar.pct.unsigned_abs().min(100)) / 100.0;
    let positive = bar.pct >= 0;
    let (left, right) = if positive {
        (start, start + (100.0 - start) * mag)
    } else {
        (start - start * mag, start)
    };
    let (from, span) = (x + w * left / 100.0, w * (right - left) / 100.0);
    if span <= 0.0 {
        return;
    }
    let color = bar.color & RGB_MASK;
    if !bar.gradient {
        canvas.fill_rect(from, y, span, h, color);
        return;
    }
    let under = under.unwrap_or(PAPER);
    let step = span / GRADIENT_STEPS as f64;
    for i in 0..GRADIENT_STEPS {
        let along = (i as f64 + 0.5) / GRADIENT_STEPS as f64;
        let faded = if positive { along } else { 1.0 - along };
        let alpha = 1.0 - (1.0 - GRADIENT_END_ALPHA) * faded;
        let reach = if i + 1 < GRADIENT_STEPS {
            STEP_OVERLAP_PT
        } else {
            0.0
        };
        canvas.fill_rect(
            from + step * i as f64,
            y,
            step + reach,
            h,
            mixed(color, under, alpha),
        );
    }
}

/// The text of a cell at `(x, y)` (the baseline), `width` wide, in `color` (black when none) with the font bits `flags`.
/// Without bits it is the plain text of the page.
pub(crate) fn draw_text(
    canvas: &mut Canvas,
    (x, y): (f64, f64),
    size: f64,
    text: &str,
    width: f64,
    (color, flags): (Option<u32>, u8),
) {
    let bits = flags & (FLAG_BOLD | FLAG_ITALIC | FLAG_UNDERLINE | FLAG_STRIKE);
    if bits == 0 {
        match color {
            Some(rgb) => canvas.colored_text(x, y, size, text, rgb),
            None => canvas.text(x, y, size, text),
        }
        return;
    }
    if text.is_empty() {
        return;
    }
    let ink = color.unwrap_or(0) & RGB_MASK;
    let line_width = (size * TEXT_LINE_EM).max(MIN_LINE_PT);
    let mut state = String::new();
    if let Some(rgb) = color {
        let rgb = crate::pdf_laid::rgb_ops(rgb & RGB_MASK);
        state.push_str(&format!("{rgb} rg {rgb} RG "));
    }
    if bits & FLAG_ITALIC != 0 {
        // The slant is about the baseline: the shift cancels the one the shear gives at the height `y`.
        state.push_str(&format!(
            "1 0 {ITALIC_SHEAR} 1 {:.2} 0 cm ",
            -ITALIC_SHEAR * y
        ));
    }
    if bits & FLAG_BOLD != 0 {
        state.push_str(&format!("2 Tr {:.2} w ", size * BOLD_OUTLINE_EM));
    }
    canvas.push_raw_op(format!("q {state}{} Q", text_ops(x, y, size, text)));
    if bits & FLAG_UNDERLINE != 0 {
        let at = y - size * UNDERLINE_DROP_EM;
        canvas.stroke_line(x, at, x + width, at, ink, line_width);
    }
    if bits & FLAG_STRIKE != 0 {
        let at = y + size * STRIKE_RISE_EM;
        canvas.stroke_line(x, at, x + width, at, ink, line_width);
    }
}

/// The drawing operators of the edges of the cell `(x, y, w, h)` (`y` the bottom) at the page's `scale`, top, right,
/// bottom, left. They are meant to come after the grid lines, which would paint over them.
pub(crate) fn edge_ops(
    (x, y, w, h): (f64, f64, f64, f64),
    border: CellBorders,
    scale: f64,
) -> Vec<String> {
    let sides = [
        (border.top, (x, y + h), (x + w, y + h), (0.0, 1.0)),
        (border.right, (x + w, y), (x + w, y + h), (1.0, 0.0)),
        (border.bottom, (x, y), (x + w, y), (0.0, -1.0)),
        (border.left, (x, y), (x, y + h), (-1.0, 0.0)),
    ];
    let mut ops = Vec::new();
    for (edge, from, to, outward) in sides {
        push_edge(&mut ops, edge, (from, to), outward, scale);
    }
    ops
}

fn push_edge(
    ops: &mut Vec<String>,
    edge: BorderEdge,
    ((x1, y1), (x2, y2)): ((f64, f64), (f64, f64)),
    (out_x, out_y): (f64, f64),
    scale: f64,
) {
    let (width, dash, lines) = match edge.line() {
        BorderLine::None => return,
        BorderLine::Thin => (THIN_PT, None, 1),
        BorderLine::Thick => (THICK_PT, None, 1),
        BorderLine::Dashed => (THIN_PT, Some(DASH_PT), 1),
        BorderLine::Dotted => (THIN_PT, Some(DOT_PT), 1),
        BorderLine::Double => (THIN_PT, None, 2),
    };
    let unit = |pt: f64| (pt * scale).max(MIN_LINE_PT);
    let color = crate::pdf_laid::rgb_ops(edge.rgb());
    let dash = dash.map_or(String::new(), |[on, off]| {
        format!("[{:.2} {:.2}] 0 d ", unit(on), unit(off))
    });
    for i in 0..lines {
        // A double edge is two lines, one on either side of the cell's own edge.
        let shift = if lines == 2 {
            (f64::from(i) * 2.0 - 1.0) * unit(DOUBLE_GAP_PT)
        } else {
            0.0
        };
        let (dx, dy) = (out_x * shift, out_y * shift);
        ops.push(format!(
            "q {:.2} w {color} RG {dash}{:.2} {:.2} m {:.2} {:.2} l S Q",
            unit(width),
            x1 + dx,
            y1 + dy,
            x2 + dx,
            y2 + dy
        ));
    }
}

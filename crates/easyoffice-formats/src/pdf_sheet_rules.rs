//! What conditional formatting adds to a sheet page of the engine's own PDF (PD-300): the fill and text color of the
//! rules, the font bits, the edges and the data bar. The rules are evaluated by the engine (`easyoffice-sheet`, the code
//! the grid and the print read) and arrive per cell in [`CellRule`], as the number format's answer arrives in
//! `CellShown`. Icon sets are not drawn: a cell that has one shows its value.
//!
//! The data bar geometry is the grid's (`barBackground` in `sheet-rules.js`): the axis is `start` percent of the cell
//! width, a positive bar grows right from it to `start + (100 - start) * pct / 100`, a negative bar left to
//! `start - start * |pct| / 100`, and a gradient bar fades from its color to a quarter of it at its far end, mixed with
//! the cell's own fill. A gradient is a form of one unit square shaded once per pair of end colors (`write_gradient` in
//! `pdf_writer.rs`) that each bar stretches over its box.
//!
//! A sheet can have tens of thousands of cells that a rule paints, so the fills, bars and edges are not written cell by
//! cell. [`RuleLayer`] collects them for the page: the boxes of one color are one path (a run of neighbours in a row is
//! one box), the edges of one kind and color are one stroked path (a run along one line is one segment).

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;

use crate::pdf_laid::{rgb_ops, Canvas};
use crate::pdf_writer::text_ops;
use crate::{
    BorderEdge, BorderLine, CellBorders, CfBar, SheetStyling, FLAG_BOLD, FLAG_ITALIC, FLAG_STRIKE,
    FLAG_UNDERLINE,
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

/// How a number format changes the way a page draws one cell (PD-235): a number sits at the right, the colour of the
/// format's section replaces the cell's text colour, and `*x` repeats `x` over the width the text leaves free
/// (`fill` is the byte offset in the text and the character). `rule` is what the conditional formats give the cell (PD-300);
/// a cell that has one and no number format has the `numeric` of [`plain_cell_numeric`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CellShown {
    pub numeric: bool,
    pub color: Option<u32>,
    pub fill: Option<(usize, char)>,
    pub rule: Option<CellRule>,
}

/// The cells of one sheet a number format or a conditional format changed, by zero-based row and column.
pub type SheetShown = HashMap<(u32, u32), CellShown>;

/// Whether a cell with no number format sits at the right: its kind when the sheet says it, else whether the text reads as
/// a number.
pub fn plain_cell_numeric(styling: Option<&SheetStyling>, at: (u32, u32), text: &str) -> bool {
    styling
        .and_then(|s| s.explicit_cell_kind(at.0, at.1, text))
        .map_or_else(|| is_number(text), |k| k == crate::CellValueKind::Number)
}

fn is_number(text: &str) -> bool {
    let t = text.trim().replace(',', "");
    !t.is_empty() && t.parse::<f64>().is_ok()
}

const PAPER: u32 = 0xFF_FFFF;
const RGB_MASK: u32 = 0xFF_FFFF;
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
/// Boxes or segments one path holds (viewers cap the length of a path).
const PATH_PARTS: usize = 256;
/// Two coordinates this close are one (the operators carry two decimals).
const SAME_PT: f64 = 0.011;

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

/// A box `(x, y, w, h)` with `y` its bottom.
type Rect = (f64, f64, f64, f64);

/// How an edge is stroked: the line kinds with a width and a dash of their own (a double edge is two thin lines).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Stroke {
    Thin,
    Thick,
    Dashed,
    Dotted,
}

impl Stroke {
    fn width_and_dash(self) -> (f64, Option<[f64; 2]>) {
        match self {
            Self::Thin => (THIN_PT, None),
            Self::Thick => (THICK_PT, None),
            Self::Dashed => (THIN_PT, Some(DASH_PT)),
            Self::Dotted => (THIN_PT, Some(DOT_PT)),
        }
    }
}

/// A line along a row edge or a column edge: its fixed coordinate and the stretch it covers.
type Segment = (f64, f64, f64);

/// The operators a [`RuleLayer`] makes: the boxes and gradients, which go under the text of the page; the edges, which go
/// over the grid lines; and the gradient forms the first ones draw by name (`Gr1`, ...), as the colors at their two ends.
pub(crate) struct RuleOps {
    pub under: Vec<String>,
    pub over: Vec<String>,
    pub gradients: Vec<(String, u32, u32)>,
}

/// Boxes by color.
type Boxes = BTreeMap<u32, Vec<Rect>>;

/// Adds a box; one that continues the last box of its color in the same row makes it longer.
fn push_box(boxes: &mut Boxes, (x, y, w, h): Rect, rgb: u32) {
    let list = boxes.entry(rgb & RGB_MASK).or_default();
    if let Some(last) = list.last_mut() {
        if (last.1 - y).abs() < SAME_PT
            && (last.3 - h).abs() < SAME_PT
            && (last.0 + last.2 - x).abs() < SAME_PT
        {
            last.2 += w;
            return;
        }
    }
    list.push((x, y, w, h));
}

/// The fills, bars and edges of the cells with a rule on one page.
pub(crate) struct RuleLayer {
    scale: f64,
    fills: Boxes,
    /// The solid bars, which go over the fills.
    bars: Boxes,
    /// The gradient bars by the colors at their two ends, which go over the solid bars.
    gradients: BTreeMap<(u32, u32), Vec<Rect>>,
    /// By stroke, color and whether the lines are vertical.
    edges: BTreeMap<(Stroke, u32, bool), Vec<Segment>>,
}

impl RuleLayer {
    pub(crate) fn new(scale: f64) -> Self {
        Self {
            scale,
            fills: BTreeMap::new(),
            bars: BTreeMap::new(),
            gradients: BTreeMap::new(),
            edges: BTreeMap::new(),
        }
    }

    /// The fill of a cell.
    pub(crate) fn fill(&mut self, cell: Rect, rgb: u32) {
        push_box(&mut self.fills, cell, rgb);
    }

    /// The bar over the cell `cell`, drawn after the cell's fill. `under` is the cell's fill.
    pub(crate) fn bar(&mut self, cell: Rect, bar: CfBar, under: Option<u32>) {
        let (x, y, w, h) = cell;
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
        let bar_box = (from, y, span, h);
        if !bar.gradient {
            push_box(&mut self.bars, bar_box, color);
            return;
        }
        // The color fades to a quarter of itself at the far end, mixed with what is under the bar: the cell's fill.
        let faded = mixed(color, under.unwrap_or(PAPER), GRADIENT_END_ALPHA);
        let ends = if positive {
            (color, faded)
        } else {
            (faded, color)
        };
        self.gradients.entry(ends).or_default().push(bar_box);
    }

    /// The edges of the cell `(x, y, w, h)`: top, right, bottom, left.
    pub(crate) fn edges(&mut self, (x, y, w, h): Rect, border: CellBorders) {
        let sides = [
            (border.top, false, y + h, (x, x + w), 1.0),
            (border.right, true, x + w, (y, y + h), 1.0),
            (border.bottom, false, y, (x, x + w), -1.0),
            (border.left, true, x, (y, y + h), -1.0),
        ];
        for (edge, vertical, at, stretch, outward) in sides {
            self.edge(edge, vertical, at, stretch, outward);
        }
    }

    fn edge(
        &mut self,
        edge: BorderEdge,
        vertical: bool,
        at: f64,
        stretch: (f64, f64),
        outward: f64,
    ) {
        let (stroke, lines) = match edge.line() {
            BorderLine::None => return,
            BorderLine::Thin => (Stroke::Thin, 1),
            BorderLine::Thick => (Stroke::Thick, 1),
            BorderLine::Dashed => (Stroke::Dashed, 1),
            BorderLine::Dotted => (Stroke::Dotted, 1),
            BorderLine::Double => (Stroke::Thin, 2),
        };
        let gap = (DOUBLE_GAP_PT * self.scale).max(MIN_LINE_PT);
        let list = self
            .edges
            .entry((stroke, edge.rgb(), vertical))
            .or_default();
        for i in 0..lines {
            // A double edge is two lines, one on either side of the cell's own edge.
            let shift = if lines == 2 {
                (f64::from(i) * 2.0 - 1.0) * gap * outward
            } else {
                0.0
            };
            list.push((at + shift, stretch.0, stretch.1));
        }
    }

    pub(crate) fn into_ops(self) -> RuleOps {
        let mut under = Vec::new();
        for (rgb, boxes) in self.fills.iter().chain(&self.bars) {
            for chunk in boxes.chunks(PATH_PARTS) {
                let mut op = format!("q {} rg", rgb_ops(*rgb));
                for &(x, y, w, h) in chunk {
                    let _ = write!(op, " {x:.2} {y:.2} {w:.2} {h:.2} re");
                }
                op.push_str(" f Q");
                under.push(op);
            }
        }
        let mut gradients = Vec::with_capacity(self.gradients.len());
        for (i, ((from, to), boxes)) in self.gradients.iter().enumerate() {
            let name = format!("Gr{}", i + 1);
            for &(x, y, w, h) in boxes {
                under.push(format!("q {w:.2} 0 0 {h:.2} {x:.2} {y:.2} cm /{name} Do Q"));
            }
            gradients.push((name, *from, *to));
        }
        let unit = |pt: f64| (pt * self.scale).max(MIN_LINE_PT);
        let mut over = Vec::new();
        for ((stroke, rgb, vertical), mut lines) in self.edges {
            let (width, dash) = stroke.width_and_dash();
            let dash = dash.map_or(String::new(), |[on, off]| {
                format!("[{:.2} {:.2}] 0 d ", unit(on), unit(off))
            });
            lines.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
            let lines = joined(&lines);
            for chunk in lines.chunks(PATH_PARTS) {
                let mut op = format!("q {:.2} w {} RG {dash}", unit(width), rgb_ops(rgb));
                for &(at, from, to) in chunk {
                    let _ = if vertical {
                        write!(op, "{at:.2} {from:.2} m {at:.2} {to:.2} l ")
                    } else {
                        write!(op, "{from:.2} {at:.2} m {to:.2} {at:.2} l ")
                    };
                }
                op.push_str("S Q");
                over.push(op);
            }
        }
        RuleOps {
            under,
            over,
            gradients,
        }
    }
}

/// `lines` (sorted by line, then start) with the ones that meet or overlap on one line made one.
fn joined(lines: &[Segment]) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::with_capacity(lines.len());
    for &(at, from, to) in lines {
        match out.last_mut() {
            Some(last) if (last.0 - at).abs() < SAME_PT && from <= last.2 + SAME_PT => {
                last.2 = last.2.max(to);
            }
            _ => out.push((at, from, to)),
        }
    }
    out
}

/// Marks of an operator that draws a text under a graphics state of its own: `STYLED_START`, the state, `STYLED_TEXT`, the
/// text operators. [`merge_styled`] turns the operators of a page into `q <state> <text> Q`, with one `q` for a run of
/// texts of the same state. Neither character starts an operator of the page.
const STYLED_START: char = '\u{1}';
const STYLED_TEXT: char = '\u{2}';

/// `ops` with every run of neighbours marked as styled text of one state made one operator.
pub(crate) fn merge_styled(ops: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(ops.len());
    // The state of the run being collected: its operator is the last of `out`.
    let mut run: Option<String> = None;
    let close = |out: &mut Vec<String>, run: &mut Option<String>| {
        if run.take().is_some() {
            if let Some(last) = out.last_mut() {
                last.push_str(" Q");
            }
        }
    };
    for op in ops {
        let Some((state, text)) = op
            .strip_prefix(STYLED_START)
            .and_then(|rest| rest.split_once(STYLED_TEXT))
        else {
            close(&mut out, &mut run);
            out.push(op);
            continue;
        };
        if run.as_deref() == Some(state) {
            if let Some(last) = out.last_mut() {
                last.push('\n');
                last.push_str(text);
            }
        } else {
            close(&mut out, &mut run);
            run = Some(state.to_string());
            out.push(format!("q {state}{text}"));
        }
    }
    close(&mut out, &mut run);
    out
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
        let rgb = rgb_ops(rgb & RGB_MASK);
        // The outline of a bold text is stroked in the color of its fill.
        let stroke = if bits & FLAG_BOLD != 0 {
            format!(" {rgb} RG")
        } else {
            String::new()
        };
        state.push_str(&format!("{rgb} rg{stroke} "));
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
    canvas.push_raw_op(format!(
        "{STYLED_START}{state}{STYLED_TEXT}{}",
        text_ops(x, y, size, text)
    ));
    if bits & FLAG_UNDERLINE != 0 {
        let at = y - size * UNDERLINE_DROP_EM;
        canvas.stroke_line(x, at, x + width, at, ink, line_width);
    }
    if bits & FLAG_STRIKE != 0 {
        let at = y + size * STRIKE_RISE_EM;
        canvas.stroke_line(x, at, x + width, at, ink, line_width);
    }
}

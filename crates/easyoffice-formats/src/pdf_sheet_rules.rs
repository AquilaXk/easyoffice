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
    BorderEdge, BorderLine, CellBorders, CellStyle, CfBar, SheetStyling, FLAG_BOLD, FLAG_ITALIC,
    FLAG_STRIKE, FLAG_UNDERLINE, MAX_SIZE_PT, MIN_SIZE_PT, STYLE_BOLD, STYLE_ITALIC, STYLE_STRIKE,
    STYLE_UNDERLINE,
};

const _: () = {
    assert!(STYLE_BOLD == FLAG_BOLD);
    assert!(STYLE_ITALIC == FLAG_ITALIC);
    assert!(STYLE_UNDERLINE == FLAG_UNDERLINE);
    assert!(STYLE_STRIKE == FLAG_STRIKE);
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
const ASCENT_EM: f64 = 0.90;
const DESCENT_EM: f64 = 0.25;
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
            // The line is named in hundredths of a point, as it is written: the edge two cells share is one line though
            // its two ends were computed along different sums and differ in the last place.
            list.push((((at + shift) * 100.0).round() / 100.0, stretch.0, stretch.1));
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
            let lines = settled(&mut lines);
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

/// `lines` sorted by line, then start, with the ones that meet or overlap on one line made one.
fn settled(lines: &mut [Segment]) -> Vec<Segment> {
    lines.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    joined(lines)
}

/// `lines` (sorted by line, then start) with the ones that meet or overlap on one line made one. A line that lies apart
/// from the last one stays its own, whatever the order the sums put it in.
fn joined(lines: &[Segment]) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::with_capacity(lines.len());
    for &(at, from, to) in lines {
        match out.last_mut() {
            Some(last)
                if (last.0 - at).abs() < SAME_PT
                    && from <= last.2 + SAME_PT
                    && to >= last.1 - SAME_PT =>
            {
                last.1 = last.1.min(from);
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

/// Effective PDF font size in points for a sheet cell: the cell's own `size_pt` (clamped to
/// `MIN_SIZE_PT..=MAX_SIZE_PT` and multiplied by `scale`) when finite and positive, else `default_pt`.
pub(crate) fn cell_font_pt(look: Option<&CellStyle>, default_pt: f64, scale: f64) -> f64 {
    look.and_then(|style| style.size_pt)
        .filter(|size| size.is_finite() && *size > 0.0)
        .map_or(default_pt, |size| {
            size.clamp(MIN_SIZE_PT, MAX_SIZE_PT) * scale
        })
}

/// Whether a cell's glyphs, underline, strikethrough, bold outline, or italic shear extend outside
/// `cell_box` `(x, bottom, w, h)`.
fn cell_text_needs_clip(
    (bx, by, bw, bh): Rect,
    (tx, ty): (f64, f64),
    size: f64,
    width: f64,
    flags: u8,
) -> bool {
    if size > bh {
        return true;
    }
    let bits = flags & (FLAG_BOLD | FLAG_ITALIC | FLAG_UNDERLINE | FLAG_STRIKE);
    let line_half = (size * TEXT_LINE_EM).max(MIN_LINE_PT) * 0.5;
    let bold_half = if bits & FLAG_BOLD != 0 {
        size * BOLD_OUTLINE_EM * 0.5
    } else {
        0.0
    };
    if ty + size * ASCENT_EM + bold_half > by + bh || ty - size * DESCENT_EM - bold_half < by {
        return true;
    }
    if bits & FLAG_UNDERLINE != 0 && ty - size * UNDERLINE_DROP_EM - line_half < by {
        return true;
    }
    if bits & FLAG_STRIKE != 0 && ty + size * STRIKE_RISE_EM + line_half > by + bh {
        return true;
    }
    let (italic_left, italic_right) = if bits & FLAG_ITALIC != 0 {
        (
            ITALIC_SHEAR * size * DESCENT_EM,
            ITALIC_SHEAR * size * ASCENT_EM,
        )
    } else {
        (0.0, 0.0)
    };
    tx - bold_half - italic_left < bx || tx + width + bold_half + italic_right > bx + bw
}

/// Draws cell text, clipping to `cell_box` `(x, bottom, w, h)` whenever the text or its font
/// decorations extend past any edge of the cell.
pub(crate) fn draw_cell_text(
    canvas: &mut Canvas,
    cell_box: Rect,
    pos: (f64, f64),
    size: f64,
    text: &str,
    width: f64,
    look: (Option<u32>, u8),
) {
    if text.is_empty() {
        return;
    }
    let clipped = cell_text_needs_clip(cell_box, pos, size, width, look.1);
    if clipped {
        canvas.push_clip_rect(cell_box.0, cell_box.1, cell_box.2, cell_box.3);
    }
    draw_text(canvas, pos, size, text, width, look);
    if clipped {
        canvas.pop_clip();
    }
}

/// The text of a cell at `(x, y)` (the baseline), `width` wide, in `color` (black when none) with the font bits `flags`.
/// Without bits it is the plain text of the page.
fn draw_text(
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

#[cfg(test)]
mod tests {
    use super::*;

    fn edge_only(top: bool, bottom: bool) -> CellBorders {
        let thin = BorderEdge::new(BorderLine::Thin, 0xFF0000);
        CellBorders {
            top: if top { thin } else { BorderEdge::NONE },
            bottom: if bottom { thin } else { BorderEdge::NONE },
            ..CellBorders::NONE
        }
    }

    /// The thin red lines along rows the layer holds, sorted and joined as `into_ops` writes them.
    fn row_lines(layer: RuleLayer) -> Vec<Segment> {
        let mut lines = layer.edges.into_values().next().unwrap_or_default();
        settled(&mut lines)
    }

    fn covers(lines: &[Segment], x: f64) -> bool {
        lines.iter().any(|&(_, from, to)| from <= x && x <= to)
    }

    #[test]
    fn the_shared_edge_of_two_rows_is_one_line_though_the_floats_differ_in_the_last_place() {
        let (y, h) = (751.059_251_835_584_f64, 19.773_744_998_649_533_f64);
        let mut layer = RuleLayer::new(1.0);
        // The cell above ends at `y`; the cell below is laid out from `y - h` and its top is `(y - h) + h`.
        layer.edges((300.0, y, 100.0, h), edge_only(false, true));
        layer.edges((0.0, y - h, 100.0, h), edge_only(true, false));
        assert_ne!(y, (y - h) + h, "the two edges differ in the last place");
        let lines = row_lines(layer);
        assert!(
            covers(&lines, 50.0),
            "the lower cell's edge is kept: {lines:?}"
        );
        assert!(
            covers(&lines, 350.0),
            "the upper cell's edge is kept: {lines:?}"
        );
        assert!(
            !covers(&lines, 200.0),
            "the gap between them stays: {lines:?}"
        );
    }

    /// Integer hundredths of a point.
    fn hundredths(pt: f64) -> i64 {
        (pt * 100.0).round() as i64
    }

    /// `(line, from, to)` in hundredths as the union of touching stretches on each line.
    fn union(segments: impl Iterator<Item = (i64, i64, i64)>) -> Vec<(i64, i64, i64)> {
        let mut all: Vec<(i64, i64, i64)> = segments.collect();
        all.sort_unstable();
        let mut out: Vec<(i64, i64, i64)> = Vec::new();
        for (at, from, to) in all {
            match out.last_mut() {
                Some(last) if last.0 == at && from <= last.2 + 1 => last.2 = last.2.max(to),
                _ => out.push((at, from, to)),
            }
        }
        out
    }

    #[test]
    fn joined_edges_cover_what_the_cells_draw_one_by_one_for_any_row_heights() {
        // A fixed seed: the same rows and cells every run.
        let mut seed = 0x2545_F491_4F6C_DD1D_u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1_u64 << 53) as f64
        };
        let mut layer = RuleLayer::new(1.0);
        let mut direct = Vec::new();
        let mut mismatched = 0;
        let mut y = 800.0 + next();
        for _ in 0..120 {
            let h = 8.0 + next() * 32.0;
            let (mut x, bottom) = (28.0 + next(), y - h);
            for _ in 0..16 {
                let w = 24.0 + next() * 90.0;
                let (top_on, bottom_on) = (next() < 0.5, next() < 0.5);
                layer.edges((x, bottom, w, h), edge_only(top_on, bottom_on));
                for (on, at) in [(top_on, bottom + h), (bottom_on, bottom)] {
                    if on {
                        direct.push((hundredths(at), hundredths(x), hundredths(x + w)));
                    }
                }
                x += w;
            }
            if (y - h) + h != y {
                mismatched += 1;
            }
            y = bottom;
        }
        assert!(
            mismatched > 0,
            "the rows include shared edges that differ in the last place"
        );
        let mut lines = layer.edges.into_values().next().unwrap_or_default();
        let joined_lines = settled(&mut lines);
        let got = union(
            joined_lines
                .iter()
                .map(|&(at, from, to)| (hundredths(at), hundredths(from), hundredths(to))),
        );
        let want = union(direct.into_iter());
        let first_difference = got.iter().zip(&want).position(|(a, b)| a != b);
        assert!(
            got == want,
            "{} stretches, {} wanted; first difference at {first_difference:?}",
            got.len(),
            want.len()
        );
    }

    #[test]
    fn cell_font_pt_scales_valid_sizes_and_rejects_or_clamps_extremes() {
        assert_eq!(cell_font_pt(None, 10.0, 1.0), 10.0);
        let plain = CellStyle::default();
        assert_eq!(cell_font_pt(Some(&plain), 15.0, 1.5), 15.0);

        let sized = CellStyle {
            size_pt: Some(11.25),
            ..CellStyle::default()
        };
        assert!((cell_font_pt(Some(&sized), 10.0, 0.5) - 5.625).abs() < 1e-12);

        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0, -4.0] {
            let bad = CellStyle {
                size_pt: Some(invalid),
                ..CellStyle::default()
            };
            assert_eq!(cell_font_pt(Some(&bad), 10.0, 1.0), 10.0);
        }

        let tiny = CellStyle {
            size_pt: Some(0.2),
            ..CellStyle::default()
        };
        assert_eq!(cell_font_pt(Some(&tiny), 10.0, 2.0), MIN_SIZE_PT * 2.0);

        let huge = CellStyle {
            size_pt: Some(9_999.0),
            ..CellStyle::default()
        };
        assert_eq!(cell_font_pt(Some(&huge), 10.0, 0.5), MAX_SIZE_PT * 0.5);
    }

    #[test]
    fn cell_text_needs_clip_catches_underline_descender_and_italic_overflow_without_false_positives(
    ) {
        let box_std = (28.35, 800.0, 48.0, 16.5);
        let all = FLAG_BOLD | FLAG_ITALIC | FLAG_UNDERLINE | FLAG_STRIKE;
        assert!(!cell_text_needs_clip(
            box_std,
            (32.35, 804.25),
            10.0,
            40.0,
            all
        ));
        assert!(!cell_text_needs_clip(
            box_std,
            (32.35, 803.25),
            12.0,
            40.0,
            all
        ));
        assert!(cell_text_needs_clip(
            box_std,
            (32.35, 801.25),
            16.0,
            20.0,
            FLAG_UNDERLINE
        ));
        assert!(cell_text_needs_clip(
            box_std,
            (32.35, 802.25),
            14.0,
            20.0,
            0
        ));

        // 10% scale default cell fits cleanly; 16% of 10% scale clips descender.
        let box_tiny = (28.35, 800.0, 4.8, 1.65);
        assert!(!cell_text_needs_clip(
            box_tiny,
            (28.75, 800.425),
            1.0,
            3.0,
            all
        ));
        assert!(cell_text_needs_clip(
            box_tiny,
            (28.75, 800.125),
            1.6,
            2.0,
            0
        ));

        // Tall 60pt row with 36pt text: upright fits inside vertically and horizontally, while
        // 12-deg italic filling width shears past right padding and must clip.
        let box_tall = (28.35, 760.0, 90.0, 60.0);
        assert!(cell_text_needs_clip(
            box_tall,
            (32.35, 773.0),
            36.0,
            82.0,
            FLAG_ITALIC
        ));
        assert!(!cell_text_needs_clip(
            box_tall,
            (32.35, 773.0),
            36.0,
            60.0,
            0
        ));
    }
}

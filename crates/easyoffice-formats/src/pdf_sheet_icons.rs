//! PDF vector paths and cell placement for conditional formatting icon sets (`PD-342`).
//!
//! Translates the 24×24 SVG `SHAPES` and `ICON_SETS` of `sheet-rules.js` into PDF Form XObjects
//! on `[0 0 24 24]`. Each distinct `(shape, color)` pair used in a workbook is written once as a
//! shared Form XObject and stamped into matching cells via `cm` + `Do`, keeping large spreadsheets
//! compact and deterministic.

use std::fmt::Write;

use crate::pdf_laid::{rgb_ops, CELL_PAD_PT, PDF_PT_PER_PX};
use crate::{CfIcon, ICON_SETS};

const RED: u32 = 0xD93025;
const AMBER: u32 = 0xF9AB00;
const GREEN: u32 = 0x1E8E3E;
const GRAY: u32 = 0x80868B;
const DARK: u32 = 0x202124;
const BLUE: u32 = 0x1A73E8;
const PINK: u32 = 0xE8A0A0;

/// Grid box size in SVG and Form XObject coordinates.
pub(crate) const ICON_BOX_UNITS: f64 = 24.0;
/// CSS icon size (`14px`) and trailing margin (`4px`) converted to PDF points (`0.75 pt/px`).
const ICON_SIZE_PT: f64 = 14.0 * PDF_PT_PER_PX;
const ICON_GAP_PT: f64 = 4.0 * PDF_PT_PER_PX;
/// Icons smaller than this in PDF points are skipped rather than emitting degenerate transforms.
const MIN_ICON_PT: f64 = 0.1;
/// Stroke width in SVG `24×24` units for outlined shapes (`ring`, `pie-0..3`, `box-0..4`, `star-0..1`).
const OUTLINE_STROKE_UNITS: f64 = 1.8;
/// Cubic Bezier handle length for a 90° circle arc of radius 1 (`4 * (sqrt(2) - 1) / 3`).
const CIRCLE_KAPPA: f64 = 0.552_284_749_8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum IconShape {
    Up,
    Down,
    Right,
    UpRight,
    DownRight,
    Disc,
    Ring,
    Diamond,
    Flag,
    Cross,
    Check,
    Mark,
    Star(u8),
    Pie(u8),
    Box(u8),
    Bars4(u8),
    Bars5(u8),
}

/// Canonical deduplication key for a rendered icon Form XObject.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct IconKey {
    pub shape: IconShape,
    pub color: u32,
}

/// Resolved cell placement for a single icon in PDF points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct IconSlot {
    pub x: f64,
    pub y: f64,
    pub size: f64,
    /// Horizontal width on the left of the cell reserved for the icon and its gap (0 when hidden).
    pub text_inset: f64,
}

type IconEntry = (IconShape, u32);

const SET_3_ARROWS: [IconEntry; 3] = [
    (IconShape::Down, RED),
    (IconShape::Right, AMBER),
    (IconShape::Up, GREEN),
];
const SET_3_ARROWS_GRAY: [IconEntry; 3] = [
    (IconShape::Down, GRAY),
    (IconShape::Right, GRAY),
    (IconShape::Up, GRAY),
];
const SET_3_FLAGS: [IconEntry; 3] = [
    (IconShape::Flag, RED),
    (IconShape::Flag, AMBER),
    (IconShape::Flag, GREEN),
];
const SET_3_TRAFFIC_1: [IconEntry; 3] = [
    (IconShape::Disc, RED),
    (IconShape::Disc, AMBER),
    (IconShape::Disc, GREEN),
];
const SET_3_TRAFFIC_2: [IconEntry; 3] = [
    (IconShape::Ring, RED),
    (IconShape::Ring, AMBER),
    (IconShape::Ring, GREEN),
];
const SET_3_SIGNS: [IconEntry; 3] = [
    (IconShape::Diamond, RED),
    (IconShape::Up, AMBER),
    (IconShape::Disc, GREEN),
];
const SET_3_SYMBOLS: [IconEntry; 3] = [
    (IconShape::Cross, RED),
    (IconShape::Mark, AMBER),
    (IconShape::Check, GREEN),
];
const SET_3_STARS: [IconEntry; 3] = [
    (IconShape::Star(0), AMBER),
    (IconShape::Star(1), AMBER),
    (IconShape::Star(2), AMBER),
];
const SET_3_TRIANGLES: [IconEntry; 3] = [
    (IconShape::Down, RED),
    (IconShape::Diamond, AMBER),
    (IconShape::Up, GREEN),
];
const SET_4_ARROWS: [IconEntry; 4] = [
    (IconShape::Down, RED),
    (IconShape::DownRight, AMBER),
    (IconShape::UpRight, AMBER),
    (IconShape::Up, GREEN),
];
const SET_4_ARROWS_GRAY: [IconEntry; 4] = [
    (IconShape::Down, GRAY),
    (IconShape::DownRight, GRAY),
    (IconShape::UpRight, GRAY),
    (IconShape::Up, GRAY),
];
const SET_4_RED_TO_BLACK: [IconEntry; 4] = [
    (IconShape::Disc, DARK),
    (IconShape::Disc, GRAY),
    (IconShape::Disc, PINK),
    (IconShape::Disc, RED),
];
const SET_4_RATING: [IconEntry; 4] = [
    (IconShape::Bars4(1), GRAY),
    (IconShape::Bars4(2), GRAY),
    (IconShape::Bars4(3), GRAY),
    (IconShape::Bars4(4), GRAY),
];
const SET_4_TRAFFIC: [IconEntry; 4] = [
    (IconShape::Disc, DARK),
    (IconShape::Disc, RED),
    (IconShape::Disc, AMBER),
    (IconShape::Disc, GREEN),
];
const SET_5_ARROWS: [IconEntry; 5] = [
    (IconShape::Down, RED),
    (IconShape::DownRight, AMBER),
    (IconShape::Right, AMBER),
    (IconShape::UpRight, AMBER),
    (IconShape::Up, GREEN),
];
const SET_5_ARROWS_GRAY: [IconEntry; 5] = [
    (IconShape::Down, GRAY),
    (IconShape::DownRight, GRAY),
    (IconShape::Right, GRAY),
    (IconShape::UpRight, GRAY),
    (IconShape::Up, GRAY),
];
const SET_5_RATING: [IconEntry; 5] = [
    (IconShape::Bars5(1), GRAY),
    (IconShape::Bars5(2), GRAY),
    (IconShape::Bars5(3), GRAY),
    (IconShape::Bars5(4), GRAY),
    (IconShape::Bars5(5), GRAY),
];
const SET_5_QUARTERS: [IconEntry; 5] = [
    (IconShape::Pie(0), BLUE),
    (IconShape::Pie(1), BLUE),
    (IconShape::Pie(2), BLUE),
    (IconShape::Pie(3), BLUE),
    (IconShape::Pie(4), BLUE),
];
const SET_5_BOXES: [IconEntry; 5] = [
    (IconShape::Box(0), BLUE),
    (IconShape::Box(1), BLUE),
    (IconShape::Box(2), BLUE),
    (IconShape::Box(3), BLUE),
    (IconShape::Box(4), BLUE),
];

/// Matches `ICON_SETS` order in `sheet_rules.rs` (`0..20`).
const ICON_SPECS: [&[IconEntry]; 20] = [
    &SET_3_ARROWS,
    &SET_3_ARROWS_GRAY,
    &SET_3_FLAGS,
    &SET_3_TRAFFIC_1,
    &SET_3_TRAFFIC_2,
    &SET_3_SIGNS,
    &SET_3_SYMBOLS,
    &SET_3_SYMBOLS,
    &SET_3_STARS,
    &SET_3_TRIANGLES,
    &SET_4_ARROWS,
    &SET_4_ARROWS_GRAY,
    &SET_4_RED_TO_BLACK,
    &SET_4_RATING,
    &SET_4_TRAFFIC,
    &SET_5_ARROWS,
    &SET_5_ARROWS_GRAY,
    &SET_5_RATING,
    &SET_5_QUARTERS,
    &SET_5_BOXES,
];

const _: () = assert!(ICON_SPECS.len() == ICON_SETS.len());

/// Resolves a [`CfIcon`] to its canonical `(shape, color)` key, or `None` when out of bounds.
pub(crate) fn resolve_icon(icon: CfIcon) -> Option<IconKey> {
    let set = ICON_SPECS.get(usize::from(icon.set))?;
    let &(shape, color) = set.get(usize::from(icon.index))?;
    Some(IconKey { shape, color })
}

/// Computes the cell placement for an icon in `(x, y_bottom, w, h)`.
/// When `hide_value` is true, the icon is centered inside the cell and reserves no text inset.
/// Otherwise it sits at the left padding edge and shrinks cleanly inside narrow or short cells.
pub(crate) fn icon_slot(
    (x, y_bottom, w, h): (f64, f64, f64, f64),
    scale: f64,
    hide_value: bool,
) -> Option<IconSlot> {
    if !x.is_finite()
        || !y_bottom.is_finite()
        || !w.is_finite()
        || !h.is_finite()
        || !scale.is_finite()
        || w <= 0.0
        || h <= 0.0
        || scale <= 0.0
    {
        return None;
    }
    let nominal = ICON_SIZE_PT * scale;
    let pad = CELL_PAD_PT * scale;
    let inner_h = (h - 0.5 * scale).max(h * 0.85);
    if hide_value {
        let inner_w = (w - 0.5 * scale).max(w * 0.85);
        let size = nominal.min(inner_w).min(inner_h);
        if size <= MIN_ICON_PT {
            return None;
        }
        return Some(IconSlot {
            x: x + (w - size) * 0.5,
            y: y_bottom + (h - size) * 0.5,
            size,
            text_inset: 0.0,
        });
    }
    let pad_x = (w * 0.25).min(pad);
    let avail_w = (w - 2.0 * pad_x).max(0.0);
    let size = nominal.min(avail_w).min(inner_h);
    if size <= MIN_ICON_PT {
        return None;
    }
    let gap = (ICON_GAP_PT * scale).min((avail_w - size).max(0.0));
    Some(IconSlot {
        x: x + pad_x,
        y: y_bottom + (h - size) * 0.5,
        size,
        text_inset: ((pad_x + size + gap) - pad).max(0.0),
    })
}

/// Converts SVG `24×24` coordinates `(sx, sy)` (top-left origin) to PDF coordinates (bottom-left).
#[inline]
fn svg_to_pdf((sx, sy): (f64, f64)) -> (f64, f64) {
    (sx, ICON_BOX_UNITS - sy)
}

fn push_polygon_svg(out: &mut String, pts: &[(f64, f64)]) {
    let Some(&first) = pts.first() else {
        return;
    };
    let (fx, fy) = svg_to_pdf(first);
    let _ = write!(out, "{fx:.3} {fy:.3} m");
    for &pt in &pts[1..] {
        let (px, py) = svg_to_pdf(pt);
        let _ = write!(out, " {px:.3} {py:.3} l");
    }
    out.push_str(" h");
}

fn push_rotated_arrow(out: &mut String, deg: f64) {
    let rad = deg.to_radians();
    let (sin, cos) = rad.sin_cos();
    let rot = |(x, y): (f64, f64)| -> (f64, f64) {
        let (dx, dy) = (x - 12.0, y - 12.0);
        (12.0 + dx * cos - dy * sin, 12.0 + dx * sin + dy * cos)
    };
    let pts = [rot((5.0, 4.0)), rot((19.0, 12.0)), rot((5.0, 20.0))];
    push_polygon_svg(out, &pts);
}

/// Emits 1..=4 clockwise quarter-circle cubic Bezier segments centered at `(12, 12)` radius `r`,
/// starting at `(12, 12 + r)` (12 o'clock in PDF coordinates).
fn push_cw_quarters(out: &mut String, r: f64, quarters: u8) {
    let k = CIRCLE_KAPPA * r;
    let segs = [
        ((12.0 + k, 12.0 + r), (12.0 + r, 12.0 + k), (12.0 + r, 12.0)),
        ((12.0 + r, 12.0 - k), (12.0 + k, 12.0 - r), (12.0, 12.0 - r)),
        ((12.0 - k, 12.0 - r), (12.0 - r, 12.0 - k), (12.0 - r, 12.0)),
        ((12.0 - r, 12.0 + k), (12.0 - k, 12.0 + r), (12.0, 12.0 + r)),
    ];
    for &(c1, c2, end) in segs.iter().take(usize::from(quarters.min(4))) {
        let _ = write!(
            out,
            " {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} c",
            c1.0, c1.1, c2.0, c2.1, end.0, end.1
        );
    }
}

/// Emits a full circle centered at `(12, 12)` with radius `r`; `cw` selects clockwise vs CCW.
fn push_circle(out: &mut String, r: f64, cw: bool) {
    let _ = write!(out, "12.000 {:.3} m", 12.0 + r);
    if cw {
        push_cw_quarters(out, r, 4);
    } else {
        let k = CIRCLE_KAPPA * r;
        let ccw = [
            ((12.0 - k, 12.0 + r), (12.0 - r, 12.0 + k), (12.0 - r, 12.0)),
            ((12.0 - r, 12.0 - k), (12.0 - k, 12.0 - r), (12.0, 12.0 - r)),
            ((12.0 + k, 12.0 - r), (12.0 + r, 12.0 - k), (12.0 + r, 12.0)),
            ((12.0 + r, 12.0 + k), (12.0 + k, 12.0 + r), (12.0, 12.0 + r)),
        ];
        for &(c1, c2, end) in &ccw {
            let _ = write!(
                out,
                " {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} c",
                c1.0, c1.1, c2.0, c2.1, end.0, end.1
            );
        }
    }
    out.push_str(" h");
}

fn push_bars(out: &mut String, color_ops: &str, level: u8, total: u8) -> bool {
    let total_f = f64::from(total);
    let step = 18.0 / total_f;
    let bar_w = step * 0.7;
    let mut has_faint = false;
    let _ = write!(out, "{color_ops} rg");
    for i in 0..total {
        if i < level {
            let bx = 3.0 + f64::from(i) * step;
            let bh = 4.0 + (f64::from(i) * 14.0) / f64::from(total - 1);
            let _ = write!(out, " {bx:.3} 3.000 {bar_w:.3} {bh:.3} re");
        }
    }
    out.push_str(" f");
    if level < total {
        has_faint = true;
        let _ = write!(out, "\nq /A3 gs {color_ops} rg");
        for i in level..total {
            let bx = 3.0 + f64::from(i) * step;
            let bh = 4.0 + (f64::from(i) * 14.0) / f64::from(total - 1);
            let _ = write!(out, " {bx:.3} 3.000 {bar_w:.3} {bh:.3} re");
        }
        out.push_str(" f Q");
    }
    has_faint
}

const STAR_SVG: [(f64, f64); 10] = [
    (12.0, 3.0),
    (14.7, 8.6),
    (20.8, 9.5),
    (16.4, 13.8),
    (17.4, 19.9),
    (12.0, 17.0),
    (6.5, 19.9),
    (7.5, 13.8),
    (3.1, 9.5),
    (9.2, 8.6),
];

const STAR_LEFT_SVG: [(f64, f64); 6] = [
    (12.0, 3.0),
    (12.0, 17.0),
    (6.5, 19.9),
    (7.5, 13.8),
    (3.1, 9.5),
    (9.2, 8.6),
];

const CROSS_SVG: [(f64, f64); 12] = [
    (6.0, 8.0),
    (8.0, 6.0),
    (12.0, 10.0),
    (16.0, 6.0),
    (18.0, 8.0),
    (14.0, 12.0),
    (18.0, 16.0),
    (16.0, 18.0),
    (12.0, 14.0),
    (8.0, 18.0),
    (6.0, 16.0),
    (10.0, 12.0),
];

const CHECK_SVG: [(f64, f64); 6] = [
    (4.0, 13.0),
    (6.5, 10.5),
    (10.0, 14.0),
    (17.5, 6.5),
    (20.0, 9.0),
    (10.0, 19.0),
];

/// Builds the PDF Form XObject content stream on `[0 0 24 24]` for `key`.
/// Returns `(content_stream, uses_alpha_30_extgstate)`.
pub(crate) fn icon_form_content(key: IconKey) -> (String, bool) {
    let color = rgb_ops(key.color);
    let mut out = String::with_capacity(256);
    let mut uses_alpha = false;
    match key.shape {
        IconShape::Up => {
            let _ = write!(out, "{color} rg ");
            push_polygon_svg(&mut out, &[(12.0, 4.0), (21.0, 20.0), (3.0, 20.0)]);
            out.push_str(" f");
        }
        IconShape::Down => {
            let _ = write!(out, "{color} rg ");
            push_polygon_svg(&mut out, &[(12.0, 20.0), (3.0, 4.0), (21.0, 4.0)]);
            out.push_str(" f");
        }
        IconShape::Right => {
            let _ = write!(out, "{color} rg ");
            push_polygon_svg(&mut out, &[(5.0, 4.0), (19.0, 12.0), (5.0, 20.0)]);
            out.push_str(" f");
        }
        IconShape::UpRight => {
            let _ = write!(out, "{color} rg ");
            push_rotated_arrow(&mut out, -45.0);
            out.push_str(" f");
        }
        IconShape::DownRight => {
            let _ = write!(out, "{color} rg ");
            push_rotated_arrow(&mut out, 45.0);
            out.push_str(" f");
        }
        IconShape::Disc => {
            let _ = write!(out, "{color} rg ");
            push_circle(&mut out, 9.0, true);
            out.push_str(" f");
        }
        IconShape::Ring => {
            let _ = write!(out, "{color} rg ");
            push_circle(&mut out, 9.0, true);
            out.push(' ');
            push_circle(&mut out, 5.0, false);
            out.push_str(" f*");
        }
        IconShape::Diamond => {
            let _ = write!(out, "{color} rg ");
            push_polygon_svg(
                &mut out,
                &[(12.0, 3.0), (21.0, 12.0), (12.0, 21.0), (3.0, 12.0)],
            );
            out.push_str(" f");
        }
        IconShape::Flag => {
            let _ = write!(out, "{color} rg 5.000 3.000 2.000 18.000 re ");
            push_polygon_svg(
                &mut out,
                &[
                    (7.0, 4.0),
                    (19.0, 4.0),
                    (15.5, 8.0),
                    (19.0, 12.0),
                    (7.0, 12.0),
                ],
            );
            out.push_str(" f");
        }
        IconShape::Cross => {
            let _ = write!(out, "{color} rg ");
            push_polygon_svg(&mut out, &CROSS_SVG);
            out.push_str(" f");
        }
        IconShape::Check => {
            let _ = write!(out, "{color} rg ");
            push_polygon_svg(&mut out, &CHECK_SVG);
            out.push_str(" f");
        }
        IconShape::Mark => {
            let _ = write!(out, "{color} rg ");
            push_polygon_svg(
                &mut out,
                &[(10.5, 4.0), (13.5, 4.0), (13.0, 14.0), (11.0, 14.0)],
            );
            out.push_str(" 10.500 4.000 3.000 3.000 re f");
        }
        IconShape::Star(0) => {
            let _ = write!(out, "{OUTLINE_STROKE_UNITS:.1} w 1 J 1 j {color} RG ");
            push_polygon_svg(&mut out, &STAR_SVG);
            out.push_str(" S");
        }
        IconShape::Star(1) => {
            let _ = write!(out, "{OUTLINE_STROKE_UNITS:.1} w 1 J 1 j {color} RG ");
            push_polygon_svg(&mut out, &STAR_SVG);
            let _ = write!(out, " S\n{color} rg ");
            push_polygon_svg(&mut out, &STAR_LEFT_SVG);
            out.push_str(" f");
        }
        IconShape::Star(_) => {
            let _ = write!(out, "{color} rg ");
            push_polygon_svg(&mut out, &STAR_SVG);
            out.push_str(" f");
        }
        IconShape::Pie(q) if q >= 4 => {
            let _ = write!(out, "{color} rg ");
            push_circle(&mut out, 9.0, true);
            out.push_str(" f");
        }
        IconShape::Pie(q) => {
            let _ = write!(out, "{OUTLINE_STROKE_UNITS:.1} w 1 J 1 j {color} RG ");
            push_circle(&mut out, 9.0, true);
            out.push_str(" S");
            if q > 0 {
                let _ = write!(out, "\n{color} rg 12.000 12.000 m 12.000 21.000 l");
                push_cw_quarters(&mut out, 9.0, q);
                out.push_str(" h f");
            }
        }
        IconShape::Box(q) => {
            let q = q.min(4);
            let _ = write!(
                out,
                "{OUTLINE_STROKE_UNITS:.1} w 1 J 1 j {color} RG 4.000 4.000 16.000 16.000 re S"
            );
            if q > 0 {
                let fill_h = f64::from(q) * 4.0;
                let _ = write!(out, "\n{color} rg 4.000 4.000 16.000 {fill_h:.3} re f");
            }
        }
        IconShape::Bars4(level) => {
            uses_alpha = push_bars(&mut out, &color, level.clamp(1, 4), 4);
        }
        IconShape::Bars5(level) => {
            uses_alpha = push_bars(&mut out, &color, level.clamp(1, 5), 5);
        }
    }
    (out, uses_alpha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_twenty_icon_sets_match_icon_sets_table_and_reject_out_of_bounds() {
        for (set_idx, &(_name, count)) in ICON_SETS.iter().enumerate() {
            assert_eq!(ICON_SPECS[set_idx].len(), usize::from(count));
            for index in 0..count {
                let icon = CfIcon {
                    set: set_idx as u8,
                    index,
                };
                assert!(icon.is_valid());
                let key = resolve_icon(icon).expect("valid icon must resolve");
                let (stream, uses_alpha) = icon_form_content(key);
                assert!(!stream.is_empty());
                assert!(!stream.contains("NaN") && !stream.contains("inf"));
                assert_eq!(stream.contains("/A3 gs"), uses_alpha);
            }
            let oob = CfIcon {
                set: set_idx as u8,
                index: count,
            };
            assert!(!oob.is_valid());
            assert!(resolve_icon(oob).is_none());
        }
        assert!(resolve_icon(CfIcon { set: 20, index: 0 }).is_none());
        assert!(resolve_icon(CfIcon {
            set: u8::MAX,
            index: u8::MAX
        })
        .is_none());
    }

    #[test]
    fn ring_and_pie_and_bars_emit_expected_pdf_primitives() {
        let ring = resolve_icon(CfIcon { set: 4, index: 0 }).unwrap();
        let (ring_stream, ring_alpha) = icon_form_content(ring);
        assert!(ring_stream.ends_with(" f*"), "{ring_stream}");
        assert!(!ring_alpha);

        let pie2 = resolve_icon(CfIcon { set: 18, index: 2 }).unwrap();
        let (pie2_stream, _) = icon_form_content(pie2);
        assert!(pie2_stream.contains("12.000 12.000 m 12.000 21.000 l"));
        assert!(pie2_stream.contains("12.000 3.000 c h f"));

        let bars4_2 = resolve_icon(CfIcon { set: 13, index: 1 }).unwrap();
        let (b2_stream, b2_alpha) = icon_form_content(bars4_2);
        assert!(b2_alpha);
        assert!(b2_stream.contains("q /A3 gs ") && b2_stream.ends_with(" f Q"));

        let bars4_4 = resolve_icon(CfIcon { set: 13, index: 3 }).unwrap();
        let (b4_stream, b4_alpha) = icon_form_content(bars4_4);
        assert!(!b4_alpha);
        assert!(!b4_stream.contains("/A3 gs"));
    }

    #[test]
    fn icon_slot_stays_inside_cell_box_across_sizes_scales_and_hostile_bounds() {
        let std = (28.35, 800.0, 48.0, 16.5);
        let visible = icon_slot(std, 1.0, false).unwrap();
        assert!((visible.x - 32.35).abs() < 1e-9);
        assert!((visible.y - 803.0).abs() < 1e-9);
        assert!((visible.size - 10.5).abs() < 1e-9);
        assert!((visible.text_inset - 13.5).abs() < 1e-9);

        let hidden = icon_slot(std, 1.0, true).unwrap();
        assert!((hidden.x - (28.35 + (48.0 - 10.5) * 0.5)).abs() < 1e-9);
        assert!((hidden.y - 803.0).abs() < 1e-9);
        assert_eq!(hidden.text_inset, 0.0);

        for box_ in [
            (10.0, 200.0, 12.0, 16.5),
            (10.0, 200.0, 8.1, 16.5),
            (10.0, 200.0, 8.0, 16.5),
            (10.0, 200.0, 7.9, 16.5),
            (10.0, 200.0, 6.0, 8.0),
            (10.0, 200.0, 48.0, 7.0),
        ] {
            for hide in [false, true] {
                let slot = icon_slot(box_, 1.0, hide).expect("non-tiny cell fits an icon");
                assert!(slot.x >= box_.0 && slot.x + slot.size <= box_.0 + box_.2 + 1e-9);
                assert!(slot.y >= box_.1 && slot.y + slot.size <= box_.1 + box_.3 + 1e-9);
            }
        }

        // Visible-value icon size shrinks monotonically with cell width without jumping at 2*pad.
        let sizes: Vec<f64> = [48.0, 20.0, 16.0, 12.0, 8.1, 8.0, 7.9, 4.0]
            .into_iter()
            .map(|w| icon_slot((0.0, 0.0, w, 16.5), 1.0, false).unwrap().size)
            .collect();
        assert!(sizes.windows(2).all(|pair| pair[0] >= pair[1]), "{sizes:?}");
        // 12px at 10% scale (w=0.9pt) keeps positive monotonic size 0.45pt > MIN_ICON_PT.
        let tiny_scaled = icon_slot((0.0, 0.0, 0.9, 1.65), 0.1, false).unwrap();
        assert!((tiny_scaled.size - 0.45).abs() < 1e-9);
        assert!(icon_slot((0.0, 0.0, 0.15, 16.5), 1.0, false).is_none());

        assert!(icon_slot((f64::NAN, 0.0, 48.0, 16.5), 1.0, false).is_none());
        assert!(icon_slot((0.0, 0.0, 0.0, 16.5), 1.0, false).is_none());
        assert!(icon_slot((0.0, 0.0, 48.0, -5.0), 1.0, false).is_none());
        assert!(icon_slot((0.0, 0.0, 48.0, 16.5), 0.0, false).is_none());
    }
}

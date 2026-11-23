//! Multi-page pure vector PDF exporter for [`RichSlideDeck`].

use std::f64::consts::FRAC_PI_2;
use std::fmt::Write;

use crate::pdf_image::{pdf_image_object, PdfImageObject};
use crate::pdf_laid::{fit_text, rgb_ops, width_pt};
use crate::pdf_text_wrap::wrap_spans;
use crate::pdf_writer::{text_ops, PdfLaidPage, PdfPageEntry};
use crate::rich_document::normalize_hex_color;
use crate::rich_slide::{
    RichSlide, RichSlideDeck, SlideBox, SlideChartSeries, SlideElement, SlideMasterLayout,
    SlideShapeType, SlideTextAlign,
};
use crate::sheet_chart::{ChartKind, ChartLegendPos};
use crate::{EasyOfficeError, PdfSerializer};

const PALETTE: [u32; 6] = [0x2563EB, 0xDC2626, 0x16A34A, 0xD97706, 0x7C3AED, 0x0891B2];
const KAPPA: f64 = 0.552_284_75;

fn parse_rgb(hex: Option<&str>) -> Option<u32> {
    normalize_hex_color(hex?).and_then(|s| u32::from_str_radix(&s, 16).ok())
}

fn contrast_rgb(fill: Option<u32>, explicit: Option<u32>) -> u32 {
    if let Some(c) = explicit {
        return c;
    }
    if let Some(rgb) = fill {
        let (r, g, b) = ((rgb >> 16) & 0xFF, (rgb >> 8) & 0xFF, rgb & 0xFF);
        if (r * 76 + g * 151 + b * 29) / 256 <= 128 {
            return 0xFFFFFF;
        }
    }
    0x111827
}

#[derive(Clone, Copy)]
struct TextRun {
    pt: f64,
    rgb: u32,
    align: SlideTextAlign,
    bold: bool,
    underline: bool,
}

impl TextRun {
    const fn plain(pt: f64, rgb: u32, bold: bool) -> Self {
        Self {
            pt,
            rgb,
            align: SlideTextAlign::Left,
            bold,
            underline: false,
        }
    }
    const fn aligned(pt: f64, rgb: u32, align: SlideTextAlign, bold: bool) -> Self {
        Self {
            pt,
            rgb,
            align,
            bold,
            underline: false,
        }
    }
}

struct PdfCanvas {
    pw: f64,
    ph: f64,
    ops: Vec<String>,
    xobjects: Vec<(String, PdfImageObject)>,
}

impl PdfCanvas {
    fn new(pw: f64, ph: f64) -> Self {
        let mut ops = Vec::with_capacity(64);
        ops.push(format!("q 1 1 1 rg 0 0 {pw:.2} {ph:.2} re f Q"));
        Self {
            pw,
            ph,
            ops,
            xobjects: Vec::new(),
        }
    }

    #[inline]
    fn py(&self, top_y: f64, h: f64) -> f64 {
        self.ph - (top_y + h)
    }

    fn fill_rect(&mut self, x: f64, top_y: f64, w: f64, h: f64, rgb: u32) {
        let y = self.py(top_y, h);
        self.ops.push(format!(
            "q {} rg {x:.2} {y:.2} {w:.2} {h:.2} re f Q",
            rgb_ops(rgb)
        ));
    }

    fn stroke_rect(&mut self, x: f64, top_y: f64, w: f64, h: f64, rgb: u32, lw: f64) {
        let y = self.py(top_y, h);
        self.ops.push(format!(
            "q {lw:.2} w {} RG {x:.2} {y:.2} {w:.2} {h:.2} re S Q",
            rgb_ops(rgb)
        ));
    }

    fn paint_path(&mut self, path: &str, fill: Option<u32>, stroke: Option<u32>, lw: f64) {
        let (has_f, has_s) = (fill.is_some(), stroke.is_some() && lw > 0.0);
        if !has_f && !has_s {
            return;
        }
        let mut op = String::from("q ");
        if let Some(f) = fill {
            let _ = write!(op, "{} rg ", rgb_ops(f));
        }
        if let Some(s) = stroke.filter(|_| has_s) {
            let _ = write!(op, "{lw:.2} w {} RG ", rgb_ops(s));
        }
        op.push_str(path);
        op.push_str(match (has_f, has_s) {
            (true, true) => " B Q",
            (true, false) => " f Q",
            _ => " S Q",
        });
        self.ops.push(op);
    }

    fn text_line(&mut self, x: f64, base_y: f64, text: &str, st: TextRun) {
        if text.is_empty() {
            return;
        }
        let t = text_ops(x, base_y, st.pt, text);
        if st.bold {
            let sw = (st.pt * 0.025).clamp(0.2, 0.8);
            self.ops.push(format!(
                "q {sw:.2} w 2 Tr {} rg {} RG {t} 0 Tr Q",
                rgb_ops(st.rgb),
                rgb_ops(st.rgb)
            ));
        } else {
            self.ops.push(format!("q {} rg {t} Q", rgb_ops(st.rgb)));
        }
        if st.underline {
            let w = width_pt(text, st.pt);
            let uy = base_y - st.pt * 0.14;
            let uw = (st.pt * 0.06).max(0.5);
            self.ops.push(format!(
                "q {uw:.2} w {} RG {x:.2} {uy:.2} m {:.2} {uy:.2} l S Q",
                rgb_ops(st.rgb),
                x + w
            ));
        }
    }

    fn draw_text_box(
        &mut self,
        rect: SlideBox,
        text: &str,
        st: TextRun,
        pad_x: f64,
        v_center: bool,
    ) {
        if text.trim().is_empty() {
            return;
        }
        let inner_w = (rect.w - 2.0 * pad_x).max(4.0);
        let max_units = ((inner_w * 1000.0) / st.pt.max(1.0)).floor().max(500.0) as u32;
        let mut lines: Vec<&str> = Vec::new();
        for para in text.split('\n') {
            if para.is_empty() {
                lines.push("");
                continue;
            }
            for span in wrap_spans(para, max_units) {
                lines.push(&para[span]);
            }
        }
        let line_h = st.pt * 1.28;
        let max_lines = ((rect.h - 4.0).max(st.pt) / line_h).floor().max(1.0) as usize;
        let shown = lines.len().min(max_lines);
        let py = self.py(rect.y, rect.h);
        self.ops.push(format!(
            "q {:.2} {py:.2} {:.2} {:.2} re W n",
            rect.x, rect.w, rect.h
        ));
        let top_start = if v_center && (shown as f64 * line_h) < rect.h {
            rect.y + (rect.h - shown as f64 * line_h) * 0.5 + st.pt * 0.85
        } else {
            rect.y + 4.0 + st.pt * 0.85
        };
        for (i, line) in lines.iter().take(shown).enumerate() {
            let w = width_pt(line, st.pt);
            let lx = match st.align {
                SlideTextAlign::Left => rect.x + pad_x,
                SlideTextAlign::Center => rect.x + (rect.w - w) * 0.5,
                SlideTextAlign::Right => rect.x + rect.w - pad_x - w,
            };
            let base_y = self.ph - (top_start + i as f64 * line_h);
            self.text_line(lx, base_y, line, st);
        }
        self.ops.push("Q".to_string());
    }
}

fn shape_path(kind: SlideShapeType, x: f64, y: f64, w: f64, h: f64) -> String {
    match kind {
        SlideShapeType::Rect => format!("{x:.2} {y:.2} {w:.2} {h:.2} re"),
        SlideShapeType::RoundRect => {
            let r = (w.min(h) * 0.14).min(18.0);
            let c = r * KAPPA;
            let (xr, yt) = (x + w, y + h);
            format!(
                "{:.2} {y:.2} m {:.2} {y:.2} l {:.2} {y:.2} {xr:.2} {:.2} {xr:.2} {:.2} c {xr:.2} {:.2} l {xr:.2} {:.2} {:.2} {yt:.2} {:.2} {yt:.2} c {:.2} {yt:.2} l {:.2} {yt:.2} {x:.2} {:.2} {x:.2} {:.2} c {x:.2} {:.2} l {x:.2} {:.2} {:.2} {y:.2} {:.2} {y:.2} c h",
                x + r, xr - r, xr - r + c, y + r - c, y + r,
                yt - r, yt - r + c, xr - r + c, xr - r,
                x + r, x + r - c, yt - r + c, yt - r,
                y + r, y + r - c, x + r - c, x + r
            )
        }
        SlideShapeType::Ellipse => {
            let (rx, ry) = (w * 0.5, h * 0.5);
            let (cx, cy, kx, ky) = (x + rx, y + ry, rx * KAPPA, ry * KAPPA);
            let (xr, yt) = (x + w, y + h);
            format!(
                "{cx:.2} {yt:.2} m {:.2} {yt:.2} {xr:.2} {:.2} {xr:.2} {cy:.2} c {xr:.2} {:.2} {:.2} {y:.2} {cx:.2} {y:.2} c {:.2} {y:.2} {x:.2} {:.2} {x:.2} {cy:.2} c {x:.2} {:.2} {:.2} {yt:.2} {cx:.2} {yt:.2} c h",
                cx + kx, cy + ky, cy - ky, cx + kx, cx - kx, cy - ky, cy + ky, cx - kx
            )
        }
        SlideShapeType::RightArrow => {
            let hw = (w * 0.34).min(h * 0.72);
            let sx = x + (w - hw).max(0.0);
            let (cy, sh) = (y + h * 0.5, h * 0.24);
            let (yt, yb) = (y + h, y);
            format!(
                "{x:.2} {:.2} m {sx:.2} {:.2} l {sx:.2} {yt:.2} l {:.2} {cy:.2} l {sx:.2} {yb:.2} l {sx:.2} {:.2} l {x:.2} {:.2} l h",
                cy + sh, cy + sh, x + w, cy - sh, cy - sh
            )
        }
        SlideShapeType::Callout => {
            let tail = (h * 0.18).min(18.0);
            let yb = y + tail;
            let (xr, yt) = (x + w, y + h);
            format!(
                "{x:.2} {yt:.2} m {xr:.2} {yt:.2} l {xr:.2} {yb:.2} l {:.2} {yb:.2} l {:.2} {y:.2} l {:.2} {yb:.2} l {x:.2} {yb:.2} l h",
                x + w * 0.36, x + w * 0.20, x + w * 0.24
            )
        }
        SlideShapeType::Diamond => {
            let (cx, cy) = (x + w * 0.5, y + h * 0.5);
            format!(
                "{cx:.2} {:.2} m {:.2} {cy:.2} l {cx:.2} {y:.2} l {x:.2} {cy:.2} l h",
                y + h,
                x + w
            )
        }
    }
}

fn draw_table(
    cv: &mut PdfCanvas,
    r: SlideBox,
    rows: &[Vec<String>],
    header_row: bool,
    fill: Option<&str>,
) {
    if r.w <= 0.0 || r.h <= 0.0 {
        return;
    }
    let n_rows = rows.len().max(1);
    let n_cols = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let (cw, rh) = (r.w / n_cols as f64, r.h / n_rows as f64);
    let pt = (rh * 0.44).clamp(7.0, 12.0);
    let hdr_rgb = parse_rgb(fill).unwrap_or(0x1E3A8A);

    for (ri, row) in rows.iter().enumerate() {
        let cell_top = r.y + ri as f64 * rh;
        let bg = if (header_row && ri == 0) || (!header_row && fill.is_some()) {
            Some(hdr_rgb)
        } else if ri % 2 == 1 {
            Some(0xF8FAFC)
        } else {
            Some(0xFFFFFF)
        };
        let t_rgb = contrast_rgb(bg, None);
        for ci in 0..n_cols {
            let cx = r.x + ci as f64 * cw;
            if let Some(rgb) = bg {
                cv.fill_rect(cx, cell_top, cw, rh, rgb);
            }
            cv.stroke_rect(cx, cell_top, cw, rh, 0xCBD5E1, 0.5);
            let txt = row.get(ci).map_or("", String::as_str);
            let first = txt.split('\n').next().unwrap_or("");
            let fitted = fit_text(first, pt, (cw - 8.0).max(4.0));
            let base_y = cv.ph - (cell_top + (rh + pt * 0.72) * 0.5);
            cv.text_line(
                cx + 4.0,
                base_y,
                &fitted,
                TextRun::plain(pt, t_rgb, header_row && ri == 0),
            );
        }
    }
}

fn draw_pie(cv: &mut PdfCanvas, cx: f64, cy: f64, rad: f64, vals: &[f64]) {
    let total: f64 = vals
        .iter()
        .copied()
        .filter(|&v| v.is_finite() && v > 0.0)
        .sum();
    if total <= 0.0 || rad <= 2.0 {
        return;
    }
    let mut ang = FRAC_PI_2;
    for (idx, &v) in vals.iter().enumerate() {
        if !v.is_finite() || v <= 0.0 {
            continue;
        }
        let sweep = (v / total) * std::f64::consts::TAU;
        let segs = ((sweep / FRAC_PI_2).ceil() as usize).max(1);
        let step = sweep / segs as f64;
        let mut path = format!(
            "{cx:.2} {cy:.2} m {:.2} {:.2} l",
            cx + rad * ang.cos(),
            cy + rad * ang.sin()
        );
        for s in 0..segs {
            let (a0, a1) = (ang - s as f64 * step, ang - (s + 1) as f64 * step);
            let k = (4.0 / 3.0) * ((a1 - a0).abs() * 0.25).tan();
            let (c0, s0, c1, s1) = (a0.cos(), a0.sin(), a1.cos(), a1.sin());
            let (x1, y1) = (cx + rad * (c0 + k * s0), cy + rad * (s0 - k * c0));
            let (x2, y2) = (cx + rad * (c1 - k * s1), cy + rad * (s1 + k * c1));
            let (xe, ye) = (cx + rad * c1, cy + rad * s1);
            let _ = write!(path, " {x1:.2} {y1:.2} {x2:.2} {y2:.2} {xe:.2} {ye:.2} c");
        }
        path.push_str(" h");
        cv.paint_path(
            &path,
            Some(PALETTE[idx % PALETTE.len()]),
            Some(0xFFFFFF),
            0.8,
        );
        ang -= sweep;
    }
}

fn draw_chart(
    cv: &mut PdfCanvas,
    r: SlideBox,
    kind: ChartKind,
    title: &str,
    cats: &[String],
    series: &[SlideChartSeries],
    legend: ChartLegendPos,
) {
    if r.w <= 0.0 || r.h <= 0.0 {
        return;
    }
    cv.fill_rect(r.x, r.y, r.w, r.h, 0xFFFFFF);
    cv.stroke_rect(r.x, r.y, r.w, r.h, 0xCBD5E1, 0.75);
    let top_pad = if title.is_empty() {
        10.0
    } else {
        let fitted = fit_text(title, 11.0, (r.w - 16.0).max(10.0));
        let tw = width_pt(&fitted, 11.0);
        let bx = cv.ph - (r.y + 18.0);
        cv.text_line(
            r.x + (r.w - tw) * 0.5,
            bx,
            &fitted,
            TextRun::plain(11.0, 0x0F172A, true),
        );
        28.0
    };
    let leg_r = if legend == ChartLegendPos::Right && r.w > 180.0 {
        78.0
    } else {
        12.0
    };
    let leg_b = if legend == ChartLegendPos::Bottom && r.h > 130.0 {
        32.0
    } else {
        22.0
    };
    let (px_l, px_top, pw, ph_box) = (
        r.x + 36.0,
        r.y + top_pad
            + if legend == ChartLegendPos::Top {
                18.0
            } else {
                4.0
            },
        (r.w - 36.0
            - leg_r
            - if legend == ChartLegendPos::Left {
                72.0
            } else {
                0.0
            })
        .max(32.0),
        (r.h - top_pad
            - leg_b
            - if legend == ChartLegendPos::Top {
                18.0
            } else {
                4.0
            })
        .max(24.0),
    );
    let px = px_l
        + if legend == ChartLegendPos::Left && r.w > 180.0 {
            72.0
        } else {
            0.0
        };
    let py_bot = cv.py(px_top, ph_box);

    if kind == ChartKind::Pie {
        let vals = series.first().map(|s| s.values.as_slice()).unwrap_or(&[]);
        draw_pie(
            cv,
            px + pw * 0.5,
            py_bot + ph_box * 0.5,
            (pw.min(ph_box) * 0.42).max(4.0),
            vals,
        );
    } else {
        let mut max_v = series
            .iter()
            .flat_map(|s| s.values.iter().copied())
            .filter(|v| v.is_finite())
            .fold(0.0_f64, f64::max);
        if max_v <= 0.0 {
            max_v = 1.0;
        }
        for g in 0..=4 {
            let f = g as f64 / 4.0;
            let gy = py_bot + ph_box * f;
            cv.ops.push(format!(
                "q 0.4 w 0.89 0.91 0.94 RG {px:.2} {gy:.2} m {:.2} {gy:.2} l S Q",
                px + pw
            ));
            if kind != ChartKind::Bar {
                let lbl = format!("{:.0}", max_v * f);
                cv.text_line(
                    (px - width_pt(&lbl, 7.0) - 4.0).max(r.x + 2.0),
                    gy - 2.5,
                    &lbl,
                    TextRun::plain(7.0, 0x64748B, false),
                );
            }
        }
        cv.ops.push(format!("q 0.8 w 0.39 0.45 0.55 RG {px:.2} {:.2} m {px:.2} {py_bot:.2} l {:.2} {py_bot:.2} l S Q", py_bot + ph_box, px + pw));
        let n_cats = cats
            .len()
            .max(series.iter().map(|s| s.values.len()).max().unwrap_or(1))
            .max(1);
        let n_ser = series.len().max(1);

        if kind == ChartKind::Bar {
            let grp_h = ph_box / n_cats as f64;
            let bar_h = ((grp_h * 0.72) / n_ser as f64).max(2.0);
            for c in 0..n_cats {
                let top_c = px_top + c as f64 * grp_h + grp_h * 0.14;
                if let Some(cat) = cats.get(c) {
                    let lbl = fit_text(cat, 7.5, 32.0);
                    cv.text_line(
                        r.x + 4.0,
                        cv.ph - (top_c + grp_h * 0.45),
                        &lbl,
                        TextRun::plain(7.5, 0x475569, false),
                    );
                }
                for (si, ser) in series.iter().enumerate() {
                    let v = ser.values.get(c).copied().unwrap_or(0.0).max(0.0);
                    let bw = (v / max_v) * pw;
                    if bw > 0.3 {
                        cv.fill_rect(
                            px,
                            top_c + si as f64 * bar_h,
                            bw,
                            (bar_h - 1.0).max(1.0),
                            PALETTE[si % PALETTE.len()],
                        );
                    }
                }
            }
        } else {
            let grp_w = pw / n_cats as f64;
            for c in 0..n_cats {
                if let Some(cat) = cats.get(c) {
                    let lbl = fit_text(cat, 7.5, (grp_w - 2.0).max(6.0));
                    let cx = px + c as f64 * grp_w + (grp_w - width_pt(&lbl, 7.5)) * 0.5;
                    cv.text_line(
                        cx,
                        py_bot - 10.0,
                        &lbl,
                        TextRun::plain(7.5, 0x475569, false),
                    );
                }
            }
            if kind == ChartKind::Column {
                let bw = ((grp_w * 0.72) / n_ser as f64).max(2.0);
                for (si, ser) in series.iter().enumerate() {
                    for c in 0..n_cats {
                        let v = ser.values.get(c).copied().unwrap_or(0.0).max(0.0);
                        let bh = (v / max_v) * ph_box;
                        if bh > 0.3 {
                            let bx = px + c as f64 * grp_w + grp_w * 0.14 + si as f64 * bw;
                            cv.fill_rect(
                                bx,
                                px_top + (ph_box - bh),
                                (bw - 1.0).max(1.0),
                                bh,
                                PALETTE[si % PALETTE.len()],
                            );
                        }
                    }
                }
            } else {
                for (si, ser) in series.iter().enumerate() {
                    let col = PALETTE[si % PALETTE.len()];
                    let mut pts = Vec::with_capacity(ser.values.len());
                    for (c, &v) in ser.values.iter().enumerate() {
                        let x = px + (c as f64 + 0.5) * (pw / n_cats as f64);
                        let y = py_bot + (v.max(0.0) / max_v) * ph_box;
                        pts.push((x, y));
                    }
                    if kind == ChartKind::Line && pts.len() >= 2 {
                        let mut op = format!(
                            "q 1.6 w {} RG {:.2} {:.2} m",
                            rgb_ops(col),
                            pts[0].0,
                            pts[0].1
                        );
                        for &(x, y) in &pts[1..] {
                            let _ = write!(op, " {x:.2} {y:.2} l");
                        }
                        op.push_str(" S Q");
                        cv.ops.push(op);
                    }
                    for &(x, y) in &pts {
                        cv.ops.push(format!(
                            "q {} rg {:.2} {:.2} 4.0 4.0 re f Q",
                            rgb_ops(col),
                            x - 2.0,
                            y - 2.0
                        ));
                    }
                }
            }
        }
    }

    if legend != ChartLegendPos::None {
        let labels: Vec<String> = if kind == ChartKind::Pie && !cats.is_empty() {
            cats.to_vec()
        } else {
            series.iter().map(|s| s.name.clone()).collect()
        };
        let (mut lx, mut ly_top) = match legend {
            ChartLegendPos::Left => (r.x + 6.0, px_top),
            ChartLegendPos::Right => (px + pw + 8.0, px_top),
            _ => (px, r.y + r.h - 15.0),
        };
        for (i, lbl) in labels.iter().take(6).enumerate() {
            let short = fit_text(lbl, 7.5, 58.0);
            cv.fill_rect(lx, ly_top + 1.0, 7.0, 7.0, PALETTE[i % PALETTE.len()]);
            cv.text_line(
                lx + 10.0,
                cv.ph - (ly_top + 7.5),
                &short,
                TextRun::plain(7.5, 0x334155, false),
            );
            if matches!(legend, ChartLegendPos::Top | ChartLegendPos::Bottom) {
                lx += width_pt(&short, 7.5) + 18.0;
                if lx > r.x + r.w - 40.0 {
                    break;
                }
            } else {
                ly_top += 13.0;
            }
        }
    }
}

fn draw_placeholders(cv: &mut PdfCanvas, slide: &RichSlide) {
    let (pw, ph) = (cv.pw, cv.ph);
    let has_t = !slide.title.is_empty();
    let has_b = !slide.bullets.is_empty();
    if !has_t && !has_b {
        return;
    }

    let bullets_text = slide
        .bullets
        .iter()
        .map(|b| format!("• {b}"))
        .collect::<Vec<_>>()
        .join("\n");
    match slide.layout {
        SlideMasterLayout::TitleSlide => {
            if has_t {
                cv.draw_text_box(
                    SlideBox {
                        x: 40.0,
                        y: ph * 0.28,
                        w: pw - 80.0,
                        h: 90.0,
                    },
                    &slide.title,
                    TextRun::aligned(28.0, 0x0F172A, SlideTextAlign::Center, true),
                    6.0,
                    true,
                );
            }
            if has_b {
                cv.draw_text_box(
                    SlideBox {
                        x: 60.0,
                        y: ph * 0.50,
                        w: pw - 120.0,
                        h: ph * 0.36,
                    },
                    &slide.bullets.join("\n"),
                    TextRun::aligned(16.0, 0x475569, SlideTextAlign::Center, false),
                    6.0,
                    false,
                );
            }
        }
        SlideMasterLayout::SectionHeader => {
            if has_t {
                cv.draw_text_box(
                    SlideBox {
                        x: 48.0,
                        y: ph * 0.36,
                        w: pw - 96.0,
                        h: 72.0,
                    },
                    &slide.title,
                    TextRun::plain(26.0, 0x0F172A, true),
                    4.0,
                    true,
                );
                cv.fill_rect(48.0, ph * 0.51, 96.0, 3.0, 0x2563EB);
            }
            if has_b {
                cv.draw_text_box(
                    SlideBox {
                        x: 48.0,
                        y: ph * 0.55,
                        w: pw - 96.0,
                        h: ph * 0.32,
                    },
                    &bullets_text,
                    TextRun::plain(14.0, 0x334155, false),
                    4.0,
                    false,
                );
            }
        }
        SlideMasterLayout::TwoColumn => {
            if has_t {
                cv.draw_text_box(
                    SlideBox {
                        x: 36.0,
                        y: 28.0,
                        w: pw - 72.0,
                        h: 52.0,
                    },
                    &slide.title,
                    TextRun::plain(22.0, 0x0F172A, true),
                    4.0,
                    false,
                );
            }
            if has_b {
                let mid = slide.bullets.len().div_ceil(2);
                let col_w = (pw - 96.0) * 0.5;
                let left = slide.bullets[..mid]
                    .iter()
                    .map(|b| format!("• {b}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                let right = slide.bullets[mid..]
                    .iter()
                    .map(|b| format!("• {b}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                cv.draw_text_box(
                    SlideBox {
                        x: 36.0,
                        y: 94.0,
                        w: col_w,
                        h: ph - 128.0,
                    },
                    &left,
                    TextRun::plain(14.0, 0x1E293B, false),
                    4.0,
                    false,
                );
                cv.draw_text_box(
                    SlideBox {
                        x: 60.0 + col_w,
                        y: 94.0,
                        w: col_w,
                        h: ph - 128.0,
                    },
                    &right,
                    TextRun::plain(14.0, 0x1E293B, false),
                    4.0,
                    false,
                );
            }
        }
        SlideMasterLayout::TitleAndContent | SlideMasterLayout::Blank => {
            if has_t {
                cv.draw_text_box(
                    SlideBox {
                        x: 36.0,
                        y: 28.0,
                        w: pw - 72.0,
                        h: 52.0,
                    },
                    &slide.title,
                    TextRun::plain(22.0, 0x0F172A, true),
                    4.0,
                    false,
                );
            }
            if has_b {
                cv.draw_text_box(
                    SlideBox {
                        x: 40.0,
                        y: 94.0,
                        w: pw - 80.0,
                        h: ph - 128.0,
                    },
                    &bullets_text,
                    TextRun::plain(14.0, 0x1E293B, false),
                    4.0,
                    false,
                );
            }
        }
    }
}

fn render_element(cv: &mut PdfCanvas, el: &SlideElement) {
    match el {
        SlideElement::TextBox {
            rect,
            text,
            font_size_pt,
            bold,
            underline,
            align,
            color,
            fill,
            ..
        } => {
            if rect.w < 1.0 || rect.h < 1.0 {
                return;
            }
            let f = parse_rgb(fill.as_deref());
            if let Some(rgb) = f {
                cv.fill_rect(rect.x, rect.y, rect.w, rect.h, rgb);
            }
            let fg = contrast_rgb(f, parse_rgb(color.as_deref()));
            cv.draw_text_box(
                *rect,
                text,
                TextRun {
                    pt: *font_size_pt,
                    rgb: fg,
                    align: *align,
                    bold: *bold,
                    underline: *underline,
                },
                4.0,
                false,
            );
        }
        SlideElement::Shape {
            rect,
            shape_type,
            text,
            fill,
            stroke,
            stroke_width_pt,
            color,
            font_size_pt,
            bold,
            align,
            ..
        } => {
            let (f, s) = (parse_rgb(fill.as_deref()), parse_rgb(stroke.as_deref()));
            if rect.w <= 0.0 || rect.h <= 0.0 {
                let y_bot = cv.py(rect.y, rect.h);
                let (x2, y2) = (rect.x + rect.w, y_bot + rect.h);
                let path = format!("{:.2} {y_bot:.2} m {x2:.2} {y2:.2} l", rect.x);
                let col = s.or(f).unwrap_or(0x475569);
                cv.paint_path(&path, None, Some(col), stroke_width_pt.max(0.75));
            } else {
                let path = shape_path(*shape_type, rect.x, cv.py(rect.y, rect.h), rect.w, rect.h);
                cv.paint_path(&path, f, s, *stroke_width_pt);
            }
            if rect.w >= 1.0 && rect.h >= 1.0 {
                let fg = contrast_rgb(f, parse_rgb(color.as_deref()));
                cv.draw_text_box(
                    *rect,
                    text,
                    TextRun::aligned(*font_size_pt, fg, *align, *bold),
                    6.0,
                    true,
                );
            }
        }
        SlideElement::Image {
            rect, image, alt, ..
        } => {
            if rect.w <= 0.0 || rect.h <= 0.0 {
                return;
            }
            let py = cv.py(rect.y, rect.h);
            if let Some(obj) = pdf_image_object(image) {
                let xname = format!("Im{}", cv.xobjects.len() + 1);
                cv.ops.push(format!(
                    "q {:.2} 0 0 {:.2} {:.2} {py:.2} cm /{xname} Do Q",
                    rect.w, rect.h, rect.x
                ));
                cv.xobjects.push((xname, obj));
            } else {
                cv.fill_rect(rect.x, rect.y, rect.w, rect.h, 0xF8FAFC);
                cv.stroke_rect(rect.x, rect.y, rect.w, rect.h, 0x94A3B8, 0.8);
                let label = if alt.is_empty() {
                    "[이미지]".to_string()
                } else {
                    format!("[이미지: {alt}]")
                };
                cv.draw_text_box(
                    *rect,
                    &label,
                    TextRun::aligned(10.0, 0x475569, SlideTextAlign::Center, false),
                    4.0,
                    true,
                );
            }
        }
        SlideElement::Table {
            rect,
            rows,
            header_row,
            fill,
            ..
        } => {
            draw_table(cv, *rect, rows, *header_row, fill.as_deref());
        }
        SlideElement::Chart {
            rect,
            chart_type,
            title,
            categories,
            series,
            legend,
            ..
        } => {
            draw_chart(cv, *rect, *chart_type, title, categories, series, *legend);
        }
    }
}

/// Exports a [`RichSlideDeck`] as a multi-page pure vector PDF (one page per slide).
pub fn encode_slide_pdf(deck: &RichSlideDeck, title: &str) -> Result<Vec<u8>, EasyOfficeError> {
    deck.validate()?;
    let (pw, ph) = deck.aspect.dimensions_pt();
    let effective_title = if title.trim().is_empty() {
        if deck.title.trim().is_empty() {
            "발표자료"
        } else {
            deck.title.as_str()
        }
    } else {
        title
    };
    let slides_ref: Vec<RichSlide> = if deck.slides.is_empty() {
        vec![RichSlide {
            title: effective_title.to_string(),
            ..RichSlide::default()
        }]
    } else {
        deck.slides.clone()
    };
    let mut pages = Vec::with_capacity(slides_ref.len());
    for (idx, slide) in slides_ref.iter().enumerate() {
        let mut cv = PdfCanvas::new(pw, ph);
        draw_placeholders(&mut cv, slide);
        for el in &slide.elements {
            render_element(&mut cv, el);
        }
        pages.push(PdfPageEntry::Laid(PdfLaidPage {
            page_number: idx + 1,
            rotation_degrees: 0,
            width_pt: pw,
            height_pt: ph,
            content: cv.ops.join("\n"),
            xobjects: cv.xobjects,
            gradients: Vec::new(),
            deflate: false,
        }));
    }
    Ok(PdfSerializer::serialize_entries(
        effective_title,
        None,
        &pages,
    ))
}

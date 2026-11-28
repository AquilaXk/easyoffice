//! Slide extensions in the pure vector PDF: hidden slides are left out, backgrounds and footer fields
//! are painted, rotated and flipped elements are drawn under a transform, and text boxes and pictures
//! get their outline. Shadows, animations and paragraph formats are not drawn.

use std::time::{SystemTime, UNIX_EPOCH};

use super::{parse_rgb, render_element, PdfCanvas, TextRun};
use crate::pptx_master::footer_placeholder_specs;
use crate::rich_slide::{RichSlide, SlideBox, SlideElement, SlideTextAlign, EMU_PER_PT};
use crate::rich_slide_ext::{SlideBackground, SlideDate};

const FOOTER_PT: f64 = 12.0;
const FOOTER_RGB: u32 = 0x8B8B8B;
const GRADIENT_BANDS: usize = 48;

/// The slides a PDF carries with their positions in the deck (slide numbers count hidden slides). A deck
/// whose every slide is hidden keeps all of them.
pub(super) fn visible_slides(slides: &[RichSlide]) -> Vec<(usize, &RichSlide)> {
    let shown: Vec<(usize, &RichSlide)> = slides
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.ext.hidden)
        .collect();
    if shown.is_empty() {
        slides.iter().enumerate().collect()
    } else {
        shown
    }
}

fn blend(a: u32, b: u32, t: f64) -> u32 {
    let ch = |shift: u32| {
        let (x, y) = (
            f64::from((a >> shift) & 0xFF),
            f64::from((b >> shift) & 0xFF),
        );
        ((x + (y - x) * t).round() as u32) & 0xFF
    };
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

pub(super) fn draw_background(cv: &mut PdfCanvas, slide: &RichSlide) {
    let (pw, ph) = (cv.pw, cv.ph);
    match &slide.ext.background {
        None => {}
        Some(SlideBackground::Solid(c)) => {
            if let Some(rgb) = parse_rgb(Some(c)) {
                cv.fill_rect(0.0, 0.0, pw, ph, rgb);
            }
        }
        Some(SlideBackground::Gradient {
            from,
            to,
            angle_deg,
        }) => {
            let (Some(a), Some(b)) = (parse_rgb(Some(from)), parse_rgb(Some(to))) else {
                return;
            };
            let vertical = (45..135).contains(angle_deg) || (225..315).contains(angle_deg);
            let reversed = (135..315).contains(angle_deg);
            for i in 0..GRADIENT_BANDS {
                let t0 = i as f64 / GRADIENT_BANDS as f64;
                let t = if reversed { 1.0 - t0 } else { t0 };
                let rgb = blend(a, b, t);
                // Bands overlap by a point so no seam shows between them.
                if vertical {
                    cv.fill_rect(0.0, ph * t0, pw, ph / GRADIENT_BANDS as f64 + 1.0, rgb);
                } else {
                    cv.fill_rect(pw * t0, 0.0, pw / GRADIENT_BANDS as f64 + 1.0, ph, rgb);
                }
            }
        }
        Some(SlideBackground::Picture(image)) => {
            let full = SlideElement::Image {
                id: String::new(),
                rect: SlideBox {
                    x: 0.0,
                    y: 0.0,
                    w: pw,
                    h: ph,
                },
                image: image.clone(),
                alt: String::new(),
            };
            render_element(cv, &full);
        }
    }
}

/// Draws one element, turned and flipped about its center when its effects ask for it.
pub(super) fn draw_element(cv: &mut PdfCanvas, slide: &RichSlide, el: &SlideElement) {
    let Some(fx) = slide.ext.fx_of(el.id()) else {
        render_element(cv, el);
        return;
    };
    let r = el.rect();
    let (cx, cy) = (r.x + r.w / 2.0, cv.ph - (r.y + r.h / 2.0));
    let (sin, cos) = fx.rotation_deg.to_radians().sin_cos();
    let (sx, sy) = (
        if fx.flip_h { -1.0 } else { 1.0 },
        if fx.flip_v { -1.0 } else { 1.0 },
    );
    // PDF y runs up, so a clockwise turn on the page is a negative angle in the matrix.
    let (a, b, c, d) = (cos * sx, -sin * sx, sin * sy, cos * sy);
    let (e, f) = (cx - a * cx - c * cy, cy - b * cx - d * cy);
    cv.ops
        .push(format!("q {a:.5} {b:.5} {c:.5} {d:.5} {e:.3} {f:.3} cm"));
    render_element(cv, el);
    if let (Some(o), true) = (&fx.outline, !matches!(el, SlideElement::Shape { .. })) {
        if let Some(rgb) = parse_rgb(Some(&o.color)) {
            cv.stroke_rect(r.x, r.y, r.w, r.h, rgb, o.width_pt.max(0.25));
        }
    }
    cv.ops.push("Q".to_string());
}

/// Days since 1970-01-01 as `YYYY-MM-DD` (proleptic Gregorian, Howard Hinnant's civil-from-days).
fn iso_date(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

pub(super) fn draw_footer(cv: &mut PdfCanvas, slide: &RichSlide, number: usize, emu: (u64, u64)) {
    let f = &slide.ext.footer;
    if f.is_default() {
        return;
    }
    let specs = footer_placeholder_specs(emu.0, emu.1);
    let rect = |i: usize| {
        let e = specs[i].emu;
        SlideBox {
            x: e.x as f64 / EMU_PER_PT,
            y: e.y as f64 / EMU_PER_PT,
            w: e.cx as f64 / EMU_PER_PT,
            h: e.cy as f64 / EMU_PER_PT,
        }
    };
    let today = || {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        iso_date(i64::try_from(secs / 86_400).unwrap_or(0))
    };
    let date = match &f.date {
        SlideDate::None => None,
        SlideDate::Auto => Some(today()),
        SlideDate::Fixed(t) => Some(t.clone()),
    };
    let mut draw = |i: usize, text: &str, align: SlideTextAlign| {
        cv.draw_text_box(
            rect(i),
            text,
            TextRun::aligned(FOOTER_PT, FOOTER_RGB, align, false),
            4.0,
            true,
        );
    };
    if let Some(d) = date {
        draw(0, &d, SlideTextAlign::Left);
    }
    if let Some(t) = &f.text {
        draw(1, t, SlideTextAlign::Center);
    }
    if f.number {
        draw(2, &number.to_string(), SlideTextAlign::Right);
    }
}

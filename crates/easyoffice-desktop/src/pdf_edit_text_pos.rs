//! Phrase highlight and underline positioning on source PDF pages (PD-245).
//!
//! Reference implementations (three-step rule): (1) `hayro::hayro_interpret` (MIT OR Apache-2.0,
//! already in the shell dependency graph) interprets page content streams and nested `/Form`
//! XObjects (`Do`), resolves Type1/TrueType/OpenType/Type0 CID/Type3 fonts, all 14 PDF standard
//! fonts and the 61 predefined Adobe CJK CMaps, and decodes `/ToUnicode` maps into positioned
//! glyphs; (2) `pdf_edit_preserve::write_markup_annotation` writes ISO 32000-1 §12.5.6.10
//! `/Highlight` (`/BM /Multiply`) and `/Underline` annotations; (3) this module supplies a
//! zero-rasterization [`Device`] that collects glyph boxes in output page coordinates and matches
//! phrases in a single linear pass within [`MAX_PAGE_GLYPHS`] and [`MAX_ADDED_MARKUPS`].

use crate::pdf_edit_budget::{MAX_ADDED_MARKUPS, MAX_PAGE_GLYPHS, MAX_TOTAL_ANNOTS};
use crate::pdf_edit_preserve::{markup_key, write_markup_annotation, MarkupKey, OrientedQuad};
use crate::pdf_edit_wire::WireMarkup;
use hayro::hayro_interpret::font::{Glyph, GlyphRun};
use hayro::hayro_interpret::hayro_cmap::BfString;
use hayro::hayro_interpret::{
    interpret_page, BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image,
    ImageDrawProps, InterpreterCache, InterpreterSettings, RenderContent, SoftMask,
};
use hayro::kurbo::{Affine, BezPath, Rect as KRect};
use hayro_syntax::page::Page;
use hayro_write::pdf_writer::types::{AnnotationFlags, AnnotationType};
use hayro_write::pdf_writer::{Pdf, Ref};
use std::collections::HashSet;

const DESCENT_RATIO: f64 = 0.22;
const ASCENT_RATIO: f64 = 0.82;
/// Canonical hidden metadata slots written by our own text PDF writer (`1 pt`, `3 Tr`).
const HIDDEN_METADATA_MAX_EM: f64 = 2.0;

#[derive(Clone, Copy)]
struct CharSpan {
    ch: char,
    quad: Option<OrientedQuad>,
    line_id: u32,
}

#[derive(Clone, Copy)]
struct PrevGlyphEnd {
    x: f64,
    y: f64,
    u_adv: (f64, f64),
    u_up: (f64, f64),
    em: f64,
}

struct GlyphCollector {
    chars: Vec<CharSpan>,
    glyphs_seen: usize,
    overflow: bool,
    line_id: u32,
    prev_end: Option<PrevGlyphEnd>,
}

impl GlyphCollector {
    fn new() -> Self {
        Self {
            chars: Vec::new(),
            glyphs_seen: 0,
            overflow: false,
            line_id: 0,
            prev_end: None,
        }
    }

    fn last_was_space(&self) -> bool {
        self.chars.last().is_some_and(|c| c.ch == ' ')
    }
}

fn glyph_advance_units(glyph: &Glyph<'_>) -> f64 {
    let raw = match glyph {
        Glyph::Outline(g) => g.advance_width(),
        Glyph::Type3(g) => g.advance_width(),
    };
    f64::from(raw.unwrap_or(600.0).clamp(0.0, 4_000.0))
}

fn glyph_text(glyph: &Glyph<'_>) -> Option<BfString> {
    if let Some(u) = glyph.as_unicode() {
        return Some(u);
    }
    let code = glyph.char_code();
    (0x20..=0x7E)
        .contains(&code)
        .then(|| char::from_u32(code).map(BfString::Char))
        .flatten()
}

#[allow(clippy::cast_possible_truncation)]
fn make_quad(s0: (f64, f64), s1: (f64, f64), u_up: (f64, f64), em: f64) -> OrientedQuad {
    let asc = ASCENT_RATIO * em;
    let desc = DESCENT_RATIO * em;
    [
        (s0.0 + asc * u_up.0) as f32,
        (s0.1 + asc * u_up.1) as f32,
        (s1.0 + asc * u_up.0) as f32,
        (s1.1 + asc * u_up.1) as f32,
        (s0.0 - desc * u_up.0) as f32,
        (s0.1 - desc * u_up.1) as f32,
        (s1.0 - desc * u_up.0) as f32,
        (s1.1 - desc * u_up.1) as f32,
    ]
}

impl<'a> Device<'a> for GlyphCollector {
    fn draw_path(&mut self, _: &BezPath, _: DrawProps<'a>, _: &DrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_image(&mut self, _: Image<'a, '_>, _: ImageDrawProps<'a>) {}
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}

    fn draw_glyph_run(
        &mut self,
        glyph_run: &GlyphRun<'_, 'a>,
        _: DrawProps<'a>,
        draw_mode: &DrawMode,
    ) {
        if self.overflow {
            return;
        }
        let glyphs = glyph_run.glyphs();
        if glyphs.is_empty() {
            return;
        }
        self.glyphs_seen = self.glyphs_seen.saturating_add(glyphs.len());
        if self.glyphs_seen > MAX_PAGE_GLYPHS {
            self.overflow = true;
            return;
        }
        for (i, pos_glyph) in glyphs.iter().enumerate() {
            let [c0, c1, c2, c3, c4, c5] = pos_glyph.transform().as_coeffs();
            let up_vec = (1000.0 * c2, 1000.0 * c3);
            let em = up_vec.0.hypot(up_vec.1);
            if !em.is_finite() || em < 0.1 {
                continue;
            }
            if matches!(draw_mode, DrawMode::Invisible) && em < HIDDEN_METADATA_MAX_EM {
                continue;
            }
            let u_up = (up_vec.0 / em, up_vec.1 / em);
            let nx = c0.hypot(c1);
            let u_adv = if nx > 1e-9 {
                (c0 / nx, c1 / nx)
            } else {
                (u_up.1, -u_up.0)
            };
            let p0 = (c4, c5);
            let nominal_adv = glyph_advance_units(pos_glyph) * (em / 1000.0);
            let adv_len = if let Some(next_glyph) = glyphs.get(i + 1) {
                let nc = next_glyph.transform().as_coeffs();
                let (dx, dy) = (nc[4] - p0.0, nc[5] - p0.1);
                let along = dx * u_adv.0 + dy * u_adv.1;
                let across = (dx * u_up.0 + dy * u_up.1).abs();
                if across <= 0.25 * em && along > 0.0 && along <= nominal_adv * 1.6 + 0.5 * em {
                    along.min(nominal_adv.max(0.1 * em))
                } else {
                    nominal_adv
                }
            } else {
                nominal_adv
            };

            if let Some(prev) = self.prev_end {
                let (dx, dy) = (p0.0 - prev.x, p0.1 - prev.y);
                let across = (dx * u_up.0 + dy * u_up.1).abs();
                let along = dx * u_adv.0 + dy * u_adv.1;
                let same_dir = prev.u_adv.0 * u_adv.0 + prev.u_adv.1 * u_adv.1 > 0.92
                    && (em - prev.em).abs() < em;
                if !same_dir || across > 0.35 * em || along < -1.5 * em {
                    self.line_id = self.line_id.saturating_add(1);
                    if !self.last_was_space() {
                        self.chars.push(CharSpan {
                            ch: ' ',
                            quad: None,
                            line_id: self.line_id,
                        });
                    }
                } else if along >= 0.20 * em && along <= 6.0 * em && !self.last_was_space() {
                    let space_quad = make_quad((prev.x, prev.y), p0, prev.u_up, em);
                    self.chars.push(CharSpan {
                        ch: ' ',
                        quad: Some(space_quad),
                        line_id: self.line_id,
                    });
                }
            }

            let p1 = (p0.0 + adv_len * u_adv.0, p0.1 + adv_len * u_adv.1);
            match glyph_text(pos_glyph) {
                Some(BfString::Char(ch)) => {
                    let ch_norm = if ch.is_whitespace() { ' ' } else { ch };
                    if !(ch_norm == ' ' && self.last_was_space()) {
                        self.chars.push(CharSpan {
                            ch: ch_norm,
                            quad: Some(make_quad(p0, p1, u_up, em)),
                            line_id: self.line_id,
                        });
                    }
                }
                Some(BfString::String(s)) => {
                    let list: Vec<char> = s.chars().collect();
                    let n = list.len().max(1) as f64;
                    for (k, ch) in list.into_iter().enumerate() {
                        let t0 = k as f64 / n;
                        let t1 = (k + 1) as f64 / n;
                        let s0 = (p0.0 + (p1.0 - p0.0) * t0, p0.1 + (p1.1 - p0.1) * t0);
                        let s1 = (p0.0 + (p1.0 - p0.0) * t1, p0.1 + (p1.1 - p0.1) * t1);
                        let ch_norm = if ch.is_whitespace() { ' ' } else { ch };
                        if !(ch_norm == ' ' && self.last_was_space()) {
                            self.chars.push(CharSpan {
                                ch: ch_norm,
                                quad: Some(make_quad(s0, s1, u_up, em)),
                                line_id: self.line_id,
                            });
                        }
                    }
                }
                None => {}
            }
            self.prev_end = Some(PrevGlyphEnd {
                x: p1.0,
                y: p1.1,
                u_adv,
                u_up,
                em,
            });
        }
    }
}

fn normalize_phrase(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut in_ws = false;
    for ch in raw.chars() {
        if ch.is_whitespace() {
            if !out.is_empty() {
                in_ws = true;
            }
        } else {
            if in_ws {
                out.push(' ');
                in_ws = false;
            }
            out.push(ch);
        }
    }
    out
}

fn merge_line_quads(spans: &[CharSpan]) -> Vec<OrientedQuad> {
    let mut merged: Vec<(u32, OrientedQuad)> = Vec::new();
    for span in spans {
        let Some(q) = span.quad else {
            continue;
        };
        match merged.last_mut() {
            Some((line, prev)) if *line == span.line_id => {
                // Keep first glyph's left edge (tl, bl) and extend right edge to `q`'s (tr, br).
                prev[2] = q[2];
                prev[3] = q[3];
                prev[6] = q[6];
                prev[7] = q[7];
            }
            _ => merged.push((span.line_id, q)),
        }
    }
    merged.into_iter().map(|(_, q)| q).collect()
}

fn quads_bbox(quads: &[OrientedQuad]) -> [f32; 4] {
    let (mut x0, mut y0, mut x1, mut y1) = (
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    );
    for q in quads {
        for k in [0usize, 2, 4, 6] {
            x0 = x0.min(q[k]);
            y0 = y0.min(q[k + 1]);
            x1 = x1.max(q[k]);
            y1 = y1.max(q[k + 1]);
        }
    }
    [x0, y0, x1, y1]
}

/// Scans `src_page` for all occurrences of `markup.text` and appends `/Highlight` or `/Underline`
/// annotations to `out`, returning their indirect references.
#[allow(clippy::too_many_arguments)]
pub fn add_page_phrase_markups<'a>(
    out: &mut Pdf,
    next: &mut Ref,
    cache: &InterpreterCache<'a>,
    xref: &'a hayro_syntax::xref::XRef,
    src_page: &Page<'a>,
    page_size: (f32, f32),
    out_page_ref: Ref,
    markup: &WireMarkup,
    added_markups: &mut usize,
    total_annots: &mut usize,
    seen_markups: &mut HashSet<MarkupKey>,
) -> Result<Vec<Ref>, String> {
    let needle = normalize_phrase(&markup.text);
    if needle.is_empty() {
        return Ok(Vec::new());
    }
    let crop = src_page.intersected_crop_box();
    let initial = Affine::translate((-crop.x0, -crop.y0));
    let bbox = KRect::new(0.0, 0.0, f64::from(page_size.0), f64::from(page_size.1));
    let settings = InterpreterSettings {
        render_content: RenderContent::PAGE,
        ..Default::default()
    };
    let mut context = Context::new(initial, bbox, cache, xref, settings);
    let mut collector = GlyphCollector::new();
    interpret_page(src_page, &mut context, &mut collector);
    if collector.overflow {
        return Err(format!(
            "쪽 하나의 글자 수가 한도({MAX_PAGE_GLYPHS})를 초과해 주석을 달 수 없습니다."
        ));
    }
    if collector.chars.is_empty() {
        return Ok(Vec::new());
    }

    let mut text = String::with_capacity(collector.chars.len() * 2);
    let mut byte_to_idx = Vec::with_capacity(collector.chars.len() * 2 + 1);
    for (idx, span) in collector.chars.iter().enumerate() {
        let len = span.ch.len_utf8();
        text.push(span.ch);
        byte_to_idx.resize(byte_to_idx.len() + len, idx);
    }
    byte_to_idx.push(collector.chars.len());

    let (annot_type, kind_tag) = if markup.underline {
        (AnnotationType::Underline, 2u8)
    } else {
        (AnnotationType::Highlight, 1u8)
    };
    let color = [
        f32::from(markup.color[0]) / 255.0,
        f32::from(markup.color[1]) / 255.0,
        f32::from(markup.color[2]) / 255.0,
    ];

    let mut refs = Vec::new();
    let mut search_from = 0usize;
    while let Some(rel) = text[search_from..].find(&needle) {
        let start_byte = search_from + rel;
        let end_byte = start_byte + needle.len();
        search_from = end_byte;
        let c_start = byte_to_idx[start_byte];
        let c_end = byte_to_idx[end_byte - 1] + 1;
        let quads = merge_line_quads(&collector.chars[c_start..c_end]);
        if quads.is_empty() {
            continue;
        }
        let rect = quads_bbox(&quads);
        if !seen_markups.insert(markup_key(kind_tag, rect)) {
            continue;
        }
        *added_markups += 1;
        if *added_markups > MAX_ADDED_MARKUPS {
            return Err(format!(
                "형광펜·밑줄 주석 수가 한도({MAX_ADDED_MARKUPS}개)를 초과합니다."
            ));
        }
        *total_annots += 1;
        if *total_annots > MAX_TOTAL_ANNOTS {
            return Err(format!(
                "문서 전체의 주석 수가 한도({MAX_TOTAL_ANNOTS})를 초과합니다."
            ));
        }
        let annot_ref = write_markup_annotation(
            out,
            next,
            annot_type,
            rect,
            &quads,
            color,
            &markup.text,
            None,
            AnnotationFlags::PRINT,
            out_page_ref,
        );
        refs.push(annot_ref);
    }
    Ok(refs)
}

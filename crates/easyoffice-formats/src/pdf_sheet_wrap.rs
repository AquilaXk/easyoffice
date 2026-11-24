//! Wrapped-text line breaking, auto row-height expansion, and aligned multi-line cell
//! rendering for the engine's sheet PDF (`PD-343`).

use std::collections::HashMap;

use crate::pdf_laid::{width_pt, Area, Canvas, CELL_PAD_PT, PDF_PT_PER_PX, SHEET_FONT_PT};
use crate::pdf_sheet_icons::icon_slot;
use crate::pdf_sheet_rules::{
    cell_font_pt, cell_text_needs_clip, draw_text, effective_rule_icon, SheetShown,
};
use crate::pdf_writer::char_units;
use crate::sheet_row_height::MAX_ROW_HEIGHT_CENTIPT;
use crate::{CellBox, HAlign, SheetStyling, VAlign};

/// Line pitch multiplier for wrapped cell lines (`sheet-look.js` `LINE_HEIGHT_RATIO`).
pub(crate) const WRAP_LINE_HEIGHT_RATIO: f64 = 1.35;
/// Top and bottom cell padding for wrapped text (`3 CSS px * 0.75 pt/px`).
pub(crate) const WRAP_PAD_V_PT: f64 = 3.0 * PDF_PT_PER_PX;
/// Single-line auto height ratio (`sheet-row-fit.js` `SINGLE_LINE_RATIO`).
const SINGLE_LINE_RATIO: f64 = 1.275;
/// Tallest spreadsheet row in points (`409.0 pt`).
const MAX_ROW_PT: f64 = MAX_ROW_HEIGHT_CENTIPT as f64 / 100.0;
/// Fail-closed cap on wrapped lines per cell (`MAX_ROW_PT / (MIN_SIZE_PT * 1.35) = ~303`).
const MAX_WRAP_LINES: usize = 512;
/// Ascender/descender proportions matching `pdf_sheet_rules.rs`.
const ASCENT_EM: f64 = 0.90;
const DESCENT_EM: f64 = 0.25;
/// Longest paragraph checked via the whole-line fast path before incremental breaking.
const FAST_WRAP_CHECK_BYTES: usize = 512;

#[inline]
pub(crate) fn wrap_pad_v(cell_font: f64, scale: f64) -> f64 {
    (WRAP_PAD_V_PT * scale).max(0.15 * cell_font)
}

#[inline]
pub(crate) fn cell_tx(
    x: f64,
    w: f64,
    pad: f64,
    icon_inset: f64,
    tw: f64,
    h_align: HAlign,
    right_aligned: bool,
) -> f64 {
    match h_align {
        HAlign::Left => x + pad + icon_inset,
        HAlign::Right => x + w - pad - tw,
        HAlign::Center => {
            let room = (w - 2.0 * pad - icon_inset).max(0.0);
            x + pad + icon_inset + (room - tw) * 0.5
        }
        HAlign::General => {
            if right_aligned {
                x + w - pad - tw
            } else {
                x + pad + icon_inset
            }
        }
    }
}

struct Paragraphs<'a>(Option<&'a str>);

impl<'a> Iterator for Paragraphs<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let s = self.0?;
        let bytes = s.as_bytes();
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'\n' || b == b'\r' {
                let next_at = if b == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
                    i + 2
                } else {
                    i + 1
                };
                self.0 = Some(&s[next_at..]);
                return Some(&s[..i]);
            }
        }
        self.0 = None;
        Some(s)
    }
}

#[inline]
fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric()
        || matches!(
            b,
            b'_' | b'\''
                | b'.'
                | b','
                | b';'
                | b':'
                | b'!'
                | b'?'
                | b'%'
                | b'('
                | b')'
                | b'-'
                | b'+'
                | b'/'
        )
}

fn next_token(s: &str) -> (&str, bool) {
    let bytes = s.as_bytes();
    if matches!(bytes[0], b' ' | b'\t') {
        let end = bytes
            .iter()
            .position(|&b| !matches!(b, b' ' | b'\t'))
            .unwrap_or(bytes.len());
        return (&s[..end], true);
    }
    if is_word_byte(bytes[0]) || s.starts_with('’') {
        let mut i = 0;
        while i < bytes.len() {
            if is_word_byte(bytes[i]) {
                i += 1;
            } else if s[i..].starts_with('’') {
                i += '’'.len_utf8();
            } else {
                break;
            }
        }
        return (&s[..i], false);
    }
    let ch = s.chars().next().expect("non-empty token slice");
    (&s[..ch.len_utf8()], false)
}

#[inline]
fn fits_units(units: u32, font_pt: f64, room_pt: f64) -> bool {
    f64::from(units) * font_pt / 1000.0 <= room_pt
}

/// Adds `s`'s character units onto `current_units`, stopping early if it exceeds `room_pt`.
#[inline]
fn try_add_fits(s: &str, current_units: u32, font_pt: f64, room_pt: f64) -> Option<u32> {
    let mut units = current_units;
    for ch in s.chars() {
        units = units.saturating_add(char_units(ch));
        if !fits_units(units, font_pt, room_pt) {
            return None;
        }
    }
    Some(units)
}

fn wrap_paragraph<'a>(para: &'a str, room_pt: f64, font_pt: f64, out: &mut Vec<&'a str>) {
    if out.len() >= MAX_WRAP_LINES {
        return;
    }
    if para.is_empty() {
        out.push("");
        return;
    }
    if para.len() <= FAST_WRAP_CHECK_BYTES {
        let trimmed = para.trim_end_matches([' ', '\t']);
        if try_add_fits(trimmed, 0, font_pt, room_pt).is_some() {
            out.push(trimmed);
            return;
        }
    }
    let mut pos = 0usize;
    let mut line_start = 0usize;
    let mut line_end = 0usize;
    let mut line_units = 0u32;
    let mut pushed_any = false;

    while pos < para.len() && out.len() < MAX_WRAP_LINES {
        let (tok, is_space) = next_token(&para[pos..]);
        let tok_start = pos;
        let tok_end = pos + tok.len();
        pos = tok_end;

        if line_end > line_start {
            if let Some(next_units) = try_add_fits(tok, line_units, font_pt, room_pt) {
                line_end = tok_end;
                line_units = next_units;
                continue;
            }
            out.push(para[line_start..line_end].trim_end_matches([' ', '\t']));
            pushed_any = true;
            if out.len() >= MAX_WRAP_LINES {
                return;
            }
            line_start = tok_start;
            line_end = tok_start;
            line_units = 0;
            if is_space {
                line_start = tok_end;
                line_end = tok_end;
                continue;
            }
        }

        if is_space {
            if let Some(units) = try_add_fits(tok, 0, font_pt, room_pt) {
                line_end = tok_end;
                line_units = units;
            } else {
                line_start = tok_end;
                line_end = tok_end;
                line_units = 0;
            }
            continue;
        }

        if let Some(units) = try_add_fits(tok, 0, font_pt, room_pt) {
            line_end = tok_end;
            line_units = units;
        } else {
            for (ci, ch) in tok.char_indices() {
                let cu = char_units(ch);
                let abs_start = tok_start + ci;
                let abs_end = abs_start + ch.len_utf8();
                let next_units = line_units.saturating_add(cu);
                if line_end > line_start && !fits_units(next_units, font_pt, room_pt) {
                    out.push(&para[line_start..line_end]);
                    pushed_any = true;
                    if out.len() >= MAX_WRAP_LINES {
                        return;
                    }
                    line_start = abs_start;
                    line_end = abs_end;
                    line_units = cu;
                } else {
                    line_end = abs_end;
                    line_units = next_units;
                }
            }
        }
    }

    if out.len() < MAX_WRAP_LINES && (line_end > line_start || !pushed_any) {
        out.push(para[line_start..line_end].trim_end_matches([' ', '\t']));
    }
}

/// Breaks `text` into wrapped slices fitting `room_pt` at `font_pt`, bounded by [`MAX_WRAP_LINES`].
pub(crate) fn wrap_cell_lines(text: &str, room_pt: f64, font_pt: f64) -> Vec<&str> {
    if text.is_empty()
        || !room_pt.is_finite()
        || room_pt <= 0.0
        || !font_pt.is_finite()
        || font_pt <= 0.0
    {
        return Vec::new();
    }
    let mut out = Vec::new();
    for para in Paragraphs(Some(text)) {
        if out.len() >= MAX_WRAP_LINES {
            break;
        }
        wrap_paragraph(para, room_pt, font_pt, &mut out);
    }
    out
}

fn is_merged(r: u32, c: u32, merges: &[&CellBox]) -> bool {
    let limit = merges.partition_point(|m| m.row0 <= r);
    merges[..limit].iter().any(|m| m.contains(r, c))
}

/// Expands automatic rows in `row_sizes` (unscaled points) to fit wrapped cell content (`PD-343`).
/// Hidden rows, hidden columns, merged cells, and user-fixed (`custom == true`) rows keep their
/// existing heights.
pub(crate) fn expand_wrapped_row_heights(
    area: Area,
    row_sizes: &mut [f64],
    col_sizes: &[f64],
    styling: Option<&SheetStyling>,
    cells: &HashMap<(u32, u32), &str>,
    shown: Option<&SheetShown>,
) {
    let Some(st) = styling else {
        return;
    };
    if st.cells.is_empty() || cells.is_empty() {
        return;
    }
    let mut area_merges: Vec<&CellBox> = st
        .merges
        .iter()
        .filter(|m| {
            m.row0 <= area.r2 && m.row1 >= area.r1 && m.col0 <= area.c2 && m.col1 >= area.c1
        })
        .collect();
    if !area_merges.is_empty() {
        area_merges.sort_unstable_by_key(|m| (m.row0, m.col0));
    }

    for (&(r, c), style) in st.cells.range((area.r1, 0)..=(area.r2, u32::MAX)) {
        if !style.wrap || c < area.c1 || c > area.c2 {
            continue;
        }
        let r_idx = (r - area.r1) as usize;
        let c_idx = (c - area.c1) as usize;
        let Some(&cur_h) = row_sizes.get(r_idx) else {
            continue;
        };
        let Some(&w) = col_sizes.get(c_idx) else {
            continue;
        };
        if cur_h <= 0.0 || cur_h >= MAX_ROW_PT || w <= 0.0 {
            continue;
        }
        if st.row_heights.get(&r).is_some_and(|rh| rh.custom) {
            continue;
        }
        let Some(&text) = cells.get(&(r, c)) else {
            continue;
        };
        if text.is_empty() {
            continue;
        }
        if !area_merges.is_empty() && is_merged(r, c, &area_merges) {
            continue;
        }

        let rule = shown.and_then(|m| m.get(&(r, c))).and_then(|cs| cs.rule);
        let (icon_key, hidden_val) = effective_rule_icon(rule);
        if hidden_val {
            continue;
        }

        let icon_inset = icon_key
            .and_then(|_| icon_slot((0.0, 0.0, w, cur_h.max(22.0 * PDF_PT_PER_PX)), 1.0, false))
            .map_or(0.0, |slot| slot.text_inset);
        let room = (w - 2.0 * CELL_PAD_PT - icon_inset).max(0.0);
        if room <= 0.0 {
            continue;
        }
        let cell_font = cell_font_pt(Some(style), SHEET_FONT_PT, 1.0);
        let lines = wrap_cell_lines(text, room, cell_font);
        if lines.is_empty() {
            continue;
        }
        let needed_pt = if lines.len() <= 1 {
            (cell_font * SINGLE_LINE_RATIO).min(MAX_ROW_PT)
        } else {
            let line_h = cell_font * WRAP_LINE_HEIGHT_RATIO;
            let pad_v = wrap_pad_v(cell_font, 1.0);
            ((lines.len() as f64) * line_h + 2.0 * pad_v).min(MAX_ROW_PT)
        };
        if needed_pt > cur_h {
            row_sizes[r_idx] = needed_pt;
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_wrapped_cell(
    canvas: &mut Canvas,
    cell_box: (f64, f64, f64, f64),
    (pad, icon_inset, scale): (f64, f64, f64),
    cell_font: f64,
    wrap_metrics: (f64, f64),
    text: &str,
    (h_align, v_align, right_aligned): (HAlign, VAlign, bool),
    look: (Option<u32>, u8),
) {
    let (x, by, w, h) = cell_box;
    let y = by + h;
    let lines = wrap_cell_lines(text, wrap_metrics.0, wrap_metrics.1);
    if lines.is_empty() {
        return;
    }

    let n = lines.len();
    let line_h = cell_font * WRAP_LINE_HEIGHT_RATIO;
    let pad_v = wrap_pad_v(cell_font, scale);
    let span_baselines = (n.saturating_sub(1) as f64) * line_h;
    let min_span = span_baselines + cell_font * (ASCENT_EM + DESCENT_EM);
    let ty_0 = if h < min_span {
        y - pad_v.min(h * 0.15) - cell_font * ASCENT_EM
    } else if n == 1
        && v_align == VAlign::Bottom
        && cell_font <= SHEET_FONT_PT * scale + 1e-6
        && h <= line_h + 2.0 * pad_v
        && (h - cell_font) * 0.5 + scale >= cell_font * DESCENT_EM
    {
        by + (h - cell_font) / 2.0 + scale
    } else {
        let avail_pad = ((h - min_span) * 0.5).min(pad_v);
        let min_ty = by + span_baselines + cell_font * DESCENT_EM;
        let max_ty = y - cell_font * ASCENT_EM;
        match v_align {
            VAlign::Top => y - avail_pad - cell_font * ASCENT_EM,
            VAlign::Center => {
                let center = by + (h + span_baselines - cell_font) / 2.0 + scale;
                if center < min_ty || center > max_ty {
                    (min_ty + max_ty) * 0.5
                } else {
                    center
                }
            }
            VAlign::Bottom => min_ty + avail_pad,
        }
    };

    let mut placements: Vec<(f64, f64, &str, f64)> = Vec::with_capacity(n);
    let mut needs_clip = false;
    for (k, &line) in lines.iter().enumerate() {
        if line.is_empty() {
            continue;
        }
        let ty = ty_0 - (k as f64) * line_h;
        let lw = width_pt(line, cell_font);
        let raw_tx = cell_tx(x, w, pad, icon_inset, lw, h_align, right_aligned);
        let tx = if icon_inset > 0.0 {
            raw_tx.max(x + pad + icon_inset)
        } else {
            raw_tx
        };
        if cell_text_needs_clip(cell_box, (tx, ty), cell_font, lw, look.1) {
            needs_clip = true;
        }
        placements.push((tx, ty, line, lw));
    }
    if placements.is_empty() {
        return;
    }
    if needs_clip {
        canvas.push_clip_rect(x, by, w, h);
    }
    for (tx, ty, line, lw) in placements {
        if needs_clip && (ty + cell_font * ASCENT_EM < by || ty - cell_font * DESCENT_EM > y) {
            continue;
        }
        draw_text(canvas, (tx, ty), cell_font, line, lw, look);
    }
    if needs_clip {
        canvas.pop_clip();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_splits_english_words_and_korean_characters() {
        assert_eq!(wrap_cell_lines("", 40.0, 10.0), Vec::<&str>::new());
        assert_eq!(wrap_cell_lines("Hello", 40.0, 10.0), ["Hello"]);
        assert_eq!(
            wrap_cell_lines("Hello world", 40.0, 10.0),
            ["Hello", "world"]
        );
        assert_eq!(
            wrap_cell_lines("가나다라마", 25.0, 10.0),
            ["가나", "다라", "마"]
        );
        assert_eq!(
            wrap_cell_lines("앞 가나다라마", 25.0, 10.0),
            ["앞 가", "나다", "라마"]
        );
    }

    #[test]
    fn wrap_preserves_explicit_newlines_and_trailing_breaks() {
        assert_eq!(wrap_cell_lines("a\nb\nc", 100.0, 10.0), ["a", "b", "c"]);
        assert_eq!(wrap_cell_lines("a\r\nb\rc", 100.0, 10.0), ["a", "b", "c"]);
        assert_eq!(wrap_cell_lines("a\n", 100.0, 10.0), ["a", ""]);
        assert_eq!(wrap_cell_lines("a\n\nb", 100.0, 10.0), ["a", "", "b"]);
    }

    #[test]
    fn wrap_handles_narrow_columns_and_hostile_inputs_safely() {
        assert_eq!(wrap_cell_lines("가나", 1.0, 10.0), ["가", "나"]);
        assert!(wrap_cell_lines("abc", 0.0, 10.0).is_empty());
        assert!(wrap_cell_lines("abc", f64::NAN, 10.0).is_empty());
        assert!(wrap_cell_lines("abc", 40.0, 0.0).is_empty());

        let started = std::time::Instant::now();
        let huge = "x".repeat(1_000_000);
        let lines = wrap_cell_lines(&huge, 40.0, 10.0);
        assert_eq!(lines.len(), MAX_WRAP_LINES);
        assert!(started.elapsed().as_secs() < 2);

        let many_breaks = "\n".repeat(1_000_000);
        assert_eq!(
            wrap_cell_lines(&many_breaks, 40.0, 10.0).len(),
            MAX_WRAP_LINES
        );
    }
}

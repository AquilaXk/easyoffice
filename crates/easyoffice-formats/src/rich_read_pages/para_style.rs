//! Character and paragraph style resolution for the Pages reader (`rich_read_pages`).

use super::TWIPS_PER_POINT;
use crate::pages_proto::{ArchiveObject, Message};
use crate::pages_schema::{
    baseline, char_props_layout, compact_para, heading, para_props, style, ty, CharPropFields,
    ParaPropFields, COLOR_RGBA, COMPACT_PARA_FIELDS, FULL_PARA_FIELDS, FULL_PARA_FIELDS_ALT,
};
use crate::{RichAlignment, RichRun, RichScript};

const MAX_FONT_SIZE_PT: f32 = 1000.0;
const MAX_HEADING_LEVEL: u8 = 6;
const COLOR_CHANNEL_MAX: f32 = 255.0;
const MAX_FONT_NAME_BYTES: usize = 128;
/// Field of a text fill message that holds the color.
const FILL_COLOR: u32 = 1;
const MAX_INDENT_PT: f32 = 1000.0;
const MAX_LINE_SPACING_MULTIPLE: f32 = 10.0;
const SINGLE_SPACING_PERCENT: u16 = 100;
const MIN_LINE_SPACING_PERCENT: f64 = 50.0;
const MAX_LINE_SPACING_PERCENT: f64 = 1000.0;
const DEFAULT_LINE_HEIGHT_PT: f64 = 12.0;

pub(super) const FLAG_BOLD: u8 = 1 << 0;
pub(super) const FLAG_ITALIC: u8 = 1 << 1;
pub(super) const FLAG_UNDERLINE: u8 = 1 << 2;
pub(super) const FLAG_STRIKE: u8 = 1 << 3;
const HEX_UPPER: &[u8; 16] = b"0123456789ABCDEF";

/// Resolved format of a style chain (first definition along the chain wins).
/// `style_bits` packs 4 tri-state flags (`bold`, `italic`, `underline`, `strike`):
/// low nibble (bits 0..4) = known mask, high nibble (bits 4..8) = value mask.
#[derive(Default)]
pub(super) struct Fmt {
    pub style_bits: u8,
    pub size_half_pt: Option<u16>,
    pub color: Option<String>,
    pub highlight: Option<String>,
    pub align: Option<u64>,
    pub identifier: Option<String>,
    pub font: Option<String>,
    pub script: Option<RichScript>,
    pub left_indent_twips: Option<u32>,
    pub first_line_indent_twips: Option<i32>,
    pub line_spacing_percent: Option<u16>,
}

fn color_hex(color: &Message) -> Option<String> {
    let mut hex = [0u8; 6];
    for (i, field) in COLOR_RGBA[..3].iter().copied().enumerate() {
        let v = color.float(field)?;
        if !v.is_finite() {
            return None;
        }
        let byte = (v.clamp(0.0, 1.0) * COLOR_CHANNEL_MAX).round() as usize;
        hex[i * 2] = HEX_UPPER[byte >> 4];
        hex[i * 2 + 1] = HEX_UPPER[byte & 0x0F];
    }
    Some(String::from_utf8(hex.to_vec()).unwrap_or_default())
}

fn size_half_pt(props: &Message, field: u32) -> Option<u16> {
    props
        .float(field)
        .filter(|s| s.is_finite() && *s > 0.0 && *s < MAX_FONT_SIZE_PT)
        .map(|s| (s * 2.0).round() as u16)
}

fn script_of(value: u64) -> Option<RichScript> {
    match value {
        baseline::SUPERSCRIPT => Some(RichScript::Superscript),
        baseline::SUBSCRIPT => Some(RichScript::Subscript),
        _ => None,
    }
}

/// Points as twips (1/20 point); `None` for non-finite or out-of-range values.
fn points_to_twips(points: f32) -> Option<i32> {
    (points.is_finite() && points.abs() <= MAX_INDENT_PT)
        .then(|| (points * TWIPS_PER_POINT).round() as i32)
}

/// Line spacing as a percentage of single spacing. Multiple mode keeps 100% unset as the default;
/// point-based exact/at-least modes (`1 | 2`) convert relative to 12pt and clamp to `50..=1000`%.
fn line_spacing_percent(p: &Message, field: u32) -> Option<u16> {
    let spacing = p.sub(field).ok().flatten()?;
    let mode = spacing
        .varint(compact_para::SPACING_MODE)
        .unwrap_or(compact_para::SPACING_MULTIPLE);
    let amount = spacing.float(compact_para::SPACING_AMOUNT)?;
    if !amount.is_finite() || amount <= 0.0 {
        return None;
    }
    match mode {
        compact_para::SPACING_MULTIPLE => {
            if amount > MAX_LINE_SPACING_MULTIPLE {
                return None;
            }
            let percent = (amount * 100.0).round() as u16;
            (percent != SINGLE_SPACING_PERCENT).then_some(percent)
        }
        1 | 2 => Some(
            ((f64::from(amount) / DEFAULT_LINE_HEIGHT_PT * 100.0)
                .round()
                .clamp(MIN_LINE_SPACING_PERCENT, MAX_LINE_SPACING_PERCENT)) as u16,
        ),
        _ => None,
    }
}

pub(super) fn alignment_of(value: u64) -> Option<RichAlignment> {
    match value {
        para_props::ALIGN_LEFT => Some(RichAlignment::Left),
        para_props::ALIGN_RIGHT => Some(RichAlignment::Right),
        para_props::ALIGN_CENTER => Some(RichAlignment::Center),
        para_props::ALIGN_JUSTIFY => Some(RichAlignment::Justify),
        _ => None,
    }
}

impl Fmt {
    fn set_flag_if_unset(&mut self, bit: u8, p: &Message, field: u32) {
        if self.style_bits & bit == 0 {
            if let Some(v) = p.varint(field) {
                self.style_bits |= bit | (u8::from(v != 0) * (bit << 4));
            }
        }
    }

    /// Fills the properties that are still unset from a character property message.
    fn merge_props(&mut self, p: &Message, l: &CharPropFields) {
        self.set_flag_if_unset(FLAG_BOLD, p, l.bold);
        self.set_flag_if_unset(FLAG_ITALIC, p, l.italic);
        self.set_flag_if_unset(FLAG_UNDERLINE, p, l.underline);
        self.set_flag_if_unset(FLAG_STRIKE, p, l.strike);
        if self.size_half_pt.is_none() {
            self.size_half_pt = size_half_pt(p, l.size_pt);
        }
        if self.color.is_none() {
            // The fill is what Pages draws, so it wins over the plain color property.
            let fill = l
                .fill
                .and_then(|n| p.sub(n).ok().flatten())
                .and_then(|f| f.sub(FILL_COLOR).ok().flatten())
                .and_then(|c| color_hex(&c));
            self.color = fill.or_else(|| p.sub(l.color).ok().flatten().and_then(|c| color_hex(&c)));
        }
        if self.highlight.is_none() {
            self.highlight = [l.highlight, l.highlight_alt]
                .into_iter()
                .flatten()
                .find_map(|n| p.sub(n).ok().flatten().and_then(|c| color_hex(&c)));
        }
        if self.font.is_none() {
            self.font = [l.font, l.font_alt]
                .into_iter()
                .flatten()
                .find_map(|n| p.string(n).ok().flatten())
                .filter(|f| !f.is_empty() && f.len() <= MAX_FONT_NAME_BYTES);
        }
        if self.script.is_none() {
            self.script = l.script.and_then(|n| p.varint(n)).and_then(script_of);
        }
    }

    /// Indents and line spacing of a paragraph property message.
    fn merge_para_layout(&mut self, p: &Message, l: &ParaPropFields) {
        if self.left_indent_twips.is_none() {
            self.left_indent_twips = p
                .float(l.left_indent)
                .and_then(points_to_twips)
                .and_then(|t| u32::try_from(t).ok())
                .filter(|t| *t > 0);
        }
        if self.first_line_indent_twips.is_none() {
            self.first_line_indent_twips = p
                .float(l.first_line_indent)
                .and_then(points_to_twips)
                .filter(|t| *t != 0);
        }
        if self.line_spacing_percent.is_none() {
            self.line_spacing_percent = line_spacing_percent(p, l.line_spacing);
        }
    }

    /// Merges one style object (full or compact layout) into this format.
    pub(super) fn merge_object(&mut self, obj: &ArchiveObject) {
        let Ok(m) = Message::parse(&obj.payload) else {
            return;
        };
        if self.identifier.is_none() {
            self.identifier = style_identifier(&m);
        }
        if let Some((props, layout)) = char_props_layout(&m) {
            self.merge_props(&props, layout);
        }
        if obj.type_id == ty::PARA_STYLE {
            if self.align.is_none() {
                self.align = para_props_of(&m).and_then(|p| p.varint(para_props::ALIGNMENT));
            }
            if let Ok(Some(p)) = m.sub(style::COMPACT_PARA_PROPS) {
                self.merge_para_layout(&p, &COMPACT_PARA_FIELDS);
            }
            if let Ok(Some(p)) = m.sub(style::PARA_PROPS) {
                self.merge_para_layout(&p, &FULL_PARA_FIELDS);
                self.merge_para_layout(&p, &FULL_PARA_FIELDS_ALT);
            }
        }
    }
}

fn style_identifier(m: &Message) -> Option<String> {
    m.sub(style::HEADER)
        .ok()
        .flatten()
        .and_then(|h| h.string(style::HEADER_IDENTIFIER).ok().flatten())
        .filter(|s| !s.is_empty())
}

fn para_props_of<'a>(m: &Message<'a>) -> Option<Message<'a>> {
    match m.sub(style::PARA_PROPS) {
        Ok(Some(p)) => Some(p),
        _ => m.sub(style::COMPACT_PARA_PROPS).ok().flatten(),
    }
}

pub(super) fn heading_level(identifier: &str) -> u8 {
    if let Some(pos) = identifier.find(heading::HEADING_PREFIX) {
        let rest = &identifier[pos + heading::HEADING_PREFIX.len()..];
        if let Some(d) = rest.chars().next().and_then(|c| c.to_digit(10)) {
            return (d as u8).clamp(1, MAX_HEADING_LEVEL);
        }
    }
    u8::from(identifier.contains(heading::TITLE_MARKER))
}

pub(super) fn push_clean(cur: &mut String, ch: char) {
    match ch {
        '\u{2028}' => cur.push(' '),
        c if c.is_control() && c != '\t' => {}
        c => cur.push(c),
    }
}

fn pick<T>(c: Option<&Fmt>, p: Option<&Fmt>, field: impl Fn(&Fmt) -> Option<T>) -> Option<T> {
    c.and_then(&field).or_else(|| p.and_then(&field))
}

/// Run formatted by a character style layered over the paragraph style.
pub(super) fn styled_run(text: String, c: Option<&Fmt>, p: Option<&Fmt>) -> RichRun {
    let c_bits = c.map_or(0, |f| f.style_bits);
    let p_bits = p.map_or(0, |f| f.style_bits);
    let c_known = c_bits & 0x0F;
    let resolved = ((c_bits >> 4) & c_known) | ((p_bits >> 4) & !c_known & (p_bits & 0x0F));
    RichRun {
        field: None,
        link: None,
        text,
        bold: (resolved & FLAG_BOLD) != 0,
        italic: (resolved & FLAG_ITALIC) != 0,
        underline: (resolved & FLAG_UNDERLINE) != 0,
        strike: (resolved & FLAG_STRIKE) != 0,
        size_half_pt: pick(c, p, |f| f.size_half_pt),
        color: pick(c, p, |f| f.color.clone()),
        highlight: pick(c, p, |f| f.highlight.clone()),
        // The paragraph style's font is the template default, so only a character style counts.
        font: c.and_then(|f| f.font.clone()),
        script: pick(c, p, |f| f.script).unwrap_or_default(),
        ..RichRun::default()
    }
}

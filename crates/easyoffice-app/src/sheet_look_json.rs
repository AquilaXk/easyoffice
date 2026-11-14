//! Alignment, wrapping and borders of a cell in the save payload and the open payload (PD-207):
//! `{"ha":"center","va":"top","wrap":true,"bd":{"t":"thin:112233","r":"double:000000"}}`.
//!
//! `fill` and `fc` are the fill colour and the font colour of the cell as `rrggbb` (PD-227).
//!
//! `unlocked`, `locked` and `hideFormula` are the protection flags of the cell (PD-216): unlocked, kept locked
//! inside an unlocked row or column, and a formula the protected sheet does not show.
//!
//! `ha` is `left|center|right`, `va` is `top|middle|bottom`, `bd` maps the sides `t`, `r`, `b`, `l`
//! to `<line>:<rrggbb>` with a line of `thin|thick|dashed|dotted|double`. A value outside these is
//! an error: the payload is rejected as a whole and nothing is written.

use std::fmt::Write;

use easyoffice_formats::{
    parse_hex_rgb, BorderEdge, BorderLine, CellBorders, CellStyle, EasyOfficeError, HAlign,
    StrictJsonValue, VAlign,
};

use crate::convert_structured::{field, invalid, JsonObject};

fn text<'a>(obj: &'a JsonObject, key: &str) -> Result<Option<&'a str>, EasyOfficeError> {
    field(obj, key)
        .map(|v| {
            v.as_str()
                .ok_or_else(|| invalid(format!("구조화 저장: '{key}'는 문자열이어야 합니다.")))
        })
        .transpose()
}

fn parse_edge(value: &StrictJsonValue) -> Result<BorderEdge, EasyOfficeError> {
    let bad = || invalid("구조화 저장: 'bd'의 변은 '선:rrggbb' 형식이어야 합니다.");
    let (line, color) = value
        .as_str()
        .and_then(|v| v.split_once(':'))
        .ok_or_else(bad)?;
    let line = BorderLine::from_payload(line).ok_or_else(bad)?;
    let rgb = parse_hex_rgb(color).ok_or_else(bad)?;
    Ok(BorderEdge::new(line, rgb))
}

fn parse_borders(value: &StrictJsonValue) -> Result<CellBorders, EasyOfficeError> {
    let obj = value
        .as_object()
        .ok_or_else(|| invalid("구조화 저장: 'bd'는 객체여야 합니다."))?;
    for (key, _) in obj {
        if !matches!(key.as_str(), "t" | "r" | "b" | "l") {
            return Err(invalid(format!(
                "구조화 저장: 'bd'에 알 수 없는 변 '{key}'가 있습니다."
            )));
        }
    }
    let edge = |side| field(obj, side).map_or(Ok(BorderEdge::NONE), parse_edge);
    Ok(CellBorders {
        top: edge("t")?,
        right: edge("r")?,
        bottom: edge("b")?,
        left: edge("l")?,
    })
}

/// An optional `rrggbb` colour of the payload key `key`.
fn parse_color(obj: &JsonObject, key: &str) -> Result<Option<u32>, EasyOfficeError> {
    text(obj, key)?
        .map(|word| {
            parse_hex_rgb(word)
                .ok_or_else(|| invalid(format!("구조화 저장: '{key}'는 rrggbb 형식이어야 합니다.")))
        })
        .transpose()
}

/// Reads the look keys of a payload `style` object into `style`.
pub(crate) fn parse_look(obj: &JsonObject, style: &mut CellStyle) -> Result<(), EasyOfficeError> {
    style.fill = parse_color(obj, "fill")?;
    style.font_color = parse_color(obj, "fc")?;
    if let Some(word) = text(obj, "ha")? {
        style.h_align = HAlign::from_payload(word).ok_or_else(|| {
            invalid("구조화 저장: 'ha'는 left, center, right 중 하나여야 합니다.")
        })?;
    }
    if let Some(word) = text(obj, "va")? {
        style.v_align = VAlign::from_payload(word).ok_or_else(|| {
            invalid("구조화 저장: 'va'는 top, middle, bottom 중 하나여야 합니다.")
        })?;
    }
    if let Some(v) = field(obj, "wrap") {
        style.wrap = v
            .as_bool()
            .ok_or_else(|| invalid("구조화 저장: 'wrap'은 true/false여야 합니다."))?;
    }
    if let Some(v) = field(obj, "bd") {
        style.borders = parse_borders(v)?;
    }
    for (key, slot) in [
        ("unlocked", &mut style.unlocked),
        ("locked", &mut style.force_locked),
        ("hideFormula", &mut style.formula_hidden),
    ] {
        if let Some(v) = field(obj, key) {
            *slot = v
                .as_bool()
                .ok_or_else(|| invalid(format!("구조화 저장: '{key}'는 true/false여야 합니다.")))?;
        }
    }
    if style.unlocked && style.force_locked {
        return Err(invalid(
            "구조화 저장: 'unlocked'와 'locked'는 함께 쓸 수 없습니다.",
        ));
    }
    Ok(())
}

/// Appends the look keys of `style` to an object body that already holds `wrote` keys.
pub(crate) fn write_look(out: &mut String, style: &CellStyle, wrote: &mut bool) {
    let mut key = |out: &mut String, name: &str| {
        if *wrote {
            out.push(',');
        }
        *wrote = true;
        let _ = write!(out, "\"{name}\":");
    };
    if let Some(word) = style.h_align.payload() {
        key(out, "ha");
        let _ = write!(out, "\"{word}\"");
    }
    if let Some(word) = style.v_align.payload() {
        key(out, "va");
        let _ = write!(out, "\"{word}\"");
    }
    if style.wrap {
        key(out, "wrap");
        out.push_str("true");
    }
    for (rgb, name) in [(style.fill, "fill"), (style.font_color, "fc")] {
        if let Some(rgb) = rgb {
            key(out, name);
            let _ = write!(out, "\"{rgb:06x}\"");
        }
    }
    for (on, name) in [
        (style.unlocked, "unlocked"),
        (style.force_locked, "locked"),
        (style.formula_hidden, "hideFormula"),
    ] {
        if on {
            key(out, name);
            out.push_str("true");
        }
    }
    if !style.borders.is_none() {
        key(out, "bd");
        out.push('{');
        let mut first = true;
        let b = &style.borders;
        for (side, edge) in [("t", b.top), ("r", b.right), ("b", b.bottom), ("l", b.left)] {
            let Some(line) = edge.line().payload() else {
                continue;
            };
            if !first {
                out.push(',');
            }
            first = false;
            let _ = write!(out, "\"{side}\":\"{line}:{:06x}\"", edge.rgb());
        }
        out.push('}');
    }
}

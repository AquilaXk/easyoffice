//! ODF presentation transition of a page (PD-398): the effect as `smil:type` with `smil:subtype`/`smil:direction`
//! (ODF 1.3 §14.6.4 `style:drawing-page-properties`, SMIL 3 transition names) and the automatic advance as
//! `presentation:duration`. The effect names are the ones the office suites write for their own transitions
//! (`pushWipe`, `slideWipe`, `barWipe`, `barnDoorWipe`, `randomBarWipe`, `fade`, `zoom`).
//!
//! Reuse (구현 원칙 3단계): (1) no Rust ODF presentation crate exists; (2) the page style reader's `XmlEvent` attributes are used
//! as they are; (3) names and values follow the ODF specification and the files the reference office suite writes; nothing was
//! copied. A cover and a pull both read back through `slideWipe` (`smil:direction="reverse"` is the pull).

use crate::rich_slide::SlideTransition;
use crate::rich_slide_ext::{AnimDir, MAX_ADVANCE_MS};
use crate::xml_stream::{attr, Attrs};

fn from_side(dir: Option<AnimDir>) -> &'static str {
    match dir.unwrap_or(AnimDir::Left) {
        AnimDir::Left => "fromRight",
        AnimDir::Right => "fromLeft",
        AnimDir::Top => "fromBottom",
        AnimDir::Bottom => "fromTop",
    }
}

fn vertical(dir: Option<AnimDir>) -> bool {
    matches!(dir.unwrap_or(AnimDir::Left), AnimDir::Left | AnimDir::Right)
}

/// `presentation:duration` text of `ms` (`PT00H00M03.5S`).
fn duration_text(ms: u32) -> String {
    let (secs, rest) = (ms / 1000, ms % 1000);
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if rest == 0 {
        format!("PT{h:02}H{m:02}M{s:02}S")
    } else {
        format!("PT{h:02}H{m:02}M{s:02}.{rest:03}S")
    }
}

/// The attributes of `style:drawing-page-properties` that carry the transition and the advance time.
pub(crate) fn transition_attrs(
    transition: SlideTransition,
    dir: Option<AnimDir>,
    advance_after_ms: Option<u32>,
) -> String {
    let side = from_side(dir);
    let (kind, smil) = match transition {
        SlideTransition::None => ("none", String::new()),
        SlideTransition::Fade => ("fade", "smil:type=\"fade\"".to_string()),
        SlideTransition::Zoom => ("zoom", "smil:type=\"zoom\"".to_string()),
        SlideTransition::Push => (
            "pushWipe",
            format!("smil:type=\"pushWipe\" smil:subtype=\"{side}\""),
        ),
        SlideTransition::Cover => (
            "slideWipe",
            format!("smil:type=\"slideWipe\" smil:subtype=\"{side}\""),
        ),
        SlideTransition::Pull => (
            "slideWipe",
            format!("smil:type=\"slideWipe\" smil:subtype=\"{side}\" smil:direction=\"reverse\""),
        ),
        SlideTransition::Wipe => (
            "barWipe",
            format!(
                "smil:type=\"barWipe\" smil:subtype=\"{}\"",
                if vertical(dir) {
                    "leftToRight"
                } else {
                    "topToBottom"
                }
            ),
        ),
        SlideTransition::Split => (
            "barnDoorWipe",
            format!(
                "smil:type=\"barnDoorWipe\" smil:subtype=\"{}\"",
                if vertical(dir) {
                    "vertical"
                } else {
                    "horizontal"
                }
            ),
        ),
        SlideTransition::RandomBar => (
            "randomBarWipe",
            format!(
                "smil:type=\"randomBarWipe\" smil:subtype=\"{}\"",
                if vertical(dir) {
                    "vertical"
                } else {
                    "horizontal"
                }
            ),
        ),
    };
    let mut out = format!("presentation:transition-type=\"{kind}\"");
    if !smil.is_empty() {
        out.push(' ');
        out.push_str(&smil);
    }
    if let Some(ms) = advance_after_ms {
        out.push_str(&format!(
            " presentation:duration=\"{}\"",
            duration_text(ms.min(MAX_ADVANCE_MS))
        ));
    }
    out
}

/// A page's transition as read.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct OdpTransition {
    pub kind: SlideTransition,
    pub dir: Option<AnimDir>,
    pub advance_after_ms: Option<u32>,
}

/// `PT00H00M03.5S` as milliseconds; `None` for anything else or a value past [`MAX_ADVANCE_MS`].
fn parse_duration(text: &str) -> Option<u32> {
    let rest = text.trim().strip_prefix("PT")?;
    let (mut total, mut num) = (0f64, String::new());
    for c in rest.chars() {
        match c {
            '0'..='9' | '.' => num.push(c),
            'H' | 'M' | 'S' => {
                let v: f64 = num.parse().ok()?;
                total += v * match c {
                    'H' => 3_600_000.0,
                    'M' => 60_000.0,
                    _ => 1_000.0,
                };
                num.clear();
            }
            _ => return None,
        }
    }
    (num.is_empty() && total.is_finite() && (0.0..=f64::from(MAX_ADVANCE_MS)).contains(&total))
        .then(|| total.round() as u32)
}

impl OdpTransition {
    /// Reads `style:drawing-page-properties`. The effect name comes from `smil:type`, or from `presentation:transition-type`
    /// where the file has no `smil:type`.
    pub(crate) fn read(attrs: &Attrs<'_>) -> Self {
        let name = attr(attrs, "smil:type")
            .or_else(|| attr(attrs, "presentation:transition-type"))
            .map(|v| v.to_ascii_lowercase())
            .unwrap_or_default();
        let sub = attr(attrs, "smil:subtype").unwrap_or_default();
        let reverse = attr(attrs, "smil:direction").is_some_and(|v| v == "reverse");
        let side = match &*sub {
            "fromRight" => Some(AnimDir::Left),
            "fromLeft" => Some(AnimDir::Right),
            "fromBottom" => Some(AnimDir::Top),
            "fromTop" => Some(AnimDir::Bottom),
            _ => None,
        };
        // The old writer's `fromLeft` for a push is the file's own default: no side.
        let axis = match &*sub {
            "leftToRight" | "vertical" => Some(AnimDir::Left),
            "topToBottom" | "horizontal" => Some(AnimDir::Top),
            _ => None,
        };
        let (kind, dir) = match name.as_str() {
            "fade" => (SlideTransition::Fade, None),
            "zoom" => (SlideTransition::Zoom, None),
            "pushwipe" | "push" => (SlideTransition::Push, side),
            "slidewipe" if reverse => (SlideTransition::Pull, side),
            "slidewipe" => (SlideTransition::Cover, side),
            "barwipe" => (SlideTransition::Wipe, axis),
            "barndoorwipe" => (SlideTransition::Split, axis),
            "randombarwipe" => (SlideTransition::RandomBar, axis),
            _ => (SlideTransition::None, None),
        };
        Self {
            kind,
            dir,
            advance_after_ms: attr(attrs, "presentation:duration").and_then(|v| parse_duration(&v)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_round_trip_and_hostile_values_are_dropped() {
        assert_eq!(parse_duration(&duration_text(3500)), Some(3500));
        assert_eq!(parse_duration(&duration_text(0)), Some(0));
        assert_eq!(parse_duration("PT99999H00M00S"), None);
        assert_eq!(parse_duration("PT-1S"), None);
        assert_eq!(parse_duration("garbage"), None);
        assert_eq!(parse_duration("PT1e9S"), None);
    }

    #[test]
    fn a_push_names_its_side() {
        let xml = transition_attrs(SlideTransition::Push, Some(AnimDir::Left), Some(2000));
        assert!(xml.contains("smil:type=\"pushWipe\" smil:subtype=\"fromRight\""));
        assert!(xml.contains("presentation:duration=\"PT00H00M02S\""));
    }
}

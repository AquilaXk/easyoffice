//! `p:transition` of a PresentationML slide (PD-398): the effect (`p:fade`, `p:push`, `p:wipe`, `p:split`, `p:cover`,
//! `p:pull`, `p:zoom`, `p:randomBar`), the side it moves toward (`dir`) and the show timing (`advClick`, `advTm`).
//!
//! Reuse (구현 원칙 3단계): (1) no Rust PresentationML crate exists; (2) the slide readers' `XmlEvent` stream and
//! `xml_stream::attr` are used as they are; (3) element names, attributes and values follow the schema as PptxGenJS (MIT,
//! Copyright (c) 2015-present Brent Ely) writes slide transitions and as ECMA-376 Part 1 §19.5 defines them
//! (`ST_TransitionSideDirectionType`: `l`, `u`, `r`, `d`; `ST_Direction`: `horz`, `vert`; `ST_TransitionInOutDirectionType`:
//! `in`, `out`). Nothing was copied. Only the base schema is written: the extension transitions (`p14:*`) need an
//! `mc:AlternateContent` wrapper.

use std::fmt::Write;

use crate::rich_slide::SlideTransition;
use crate::rich_slide_ext::{AnimDir, MAX_ADVANCE_MS};
use crate::xml_stream::{attr, Attrs};

fn side(dir: Option<AnimDir>) -> &'static str {
    match dir.unwrap_or(AnimDir::Left) {
        AnimDir::Left => "l",
        AnimDir::Right => "r",
        AnimDir::Top => "u",
        AnimDir::Bottom => "d",
    }
}

/// The axis of the split and the bars: left and right run across (`vert`), top and bottom down (`horz`).
fn axis(dir: Option<AnimDir>) -> &'static str {
    match dir.unwrap_or(AnimDir::Left) {
        AnimDir::Left | AnimDir::Right => "vert",
        AnimDir::Top | AnimDir::Bottom => "horz",
    }
}

/// `<p:transition>` of a slide, or an empty string for a slide with no transition and no timing.
pub(crate) fn transition_xml(
    transition: SlideTransition,
    dir: Option<AnimDir>,
    advance_after_ms: Option<u32>,
    no_click_advance: bool,
) -> String {
    let child = match transition {
        SlideTransition::None => String::new(),
        SlideTransition::Fade => "<p:fade/>".to_string(),
        SlideTransition::Zoom => "<p:zoom dir=\"in\"/>".to_string(),
        SlideTransition::Push => format!("<p:push dir=\"{}\"/>", side(dir)),
        SlideTransition::Wipe => format!("<p:wipe dir=\"{}\"/>", side(dir)),
        SlideTransition::Cover => format!("<p:cover dir=\"{}\"/>", side(dir)),
        SlideTransition::Pull => format!("<p:pull dir=\"{}\"/>", side(dir)),
        SlideTransition::Split => format!("<p:split orient=\"{}\" dir=\"out\"/>", axis(dir)),
        SlideTransition::RandomBar => format!("<p:randomBar dir=\"{}\"/>", axis(dir)),
    };
    if child.is_empty() && advance_after_ms.is_none() && !no_click_advance {
        return String::new();
    }
    let mut out = String::from("<p:transition");
    if no_click_advance {
        out.push_str(" advClick=\"0\"");
    }
    if let Some(ms) = advance_after_ms {
        let _ = write!(out, " advTm=\"{}\"", ms.min(MAX_ADVANCE_MS));
    }
    let _ = write!(out, ">{child}</p:transition>");
    out
}

/// What the reader collects from one `p:transition` element.
#[derive(Default)]
pub(crate) struct TransitionRead {
    pub transition: SlideTransition,
    pub dir: Option<AnimDir>,
    pub advance_after_ms: Option<u32>,
    pub no_click_advance: bool,
}

impl TransitionRead {
    /// The `p:transition` start tag: the timing attributes. A value past [`MAX_ADVANCE_MS`] or not a number is dropped.
    pub(crate) fn on_start(&mut self, attrs: &Attrs<'_>) {
        self.no_click_advance = attr(attrs, "advClick").is_some_and(|v| v == "0" || v == "false");
        self.advance_after_ms = attr(attrs, "advTm")
            .and_then(|v| v.trim().parse::<u32>().ok())
            .filter(|ms| *ms <= MAX_ADVANCE_MS);
    }

    /// A child of `p:transition`: the effect and its side. An effect the editor does not offer changes nothing.
    pub(crate) fn on_child(&mut self, local: &str, attrs: &Attrs<'_>) {
        self.transition = match local {
            "fade" => SlideTransition::Fade,
            "zoom" => SlideTransition::Zoom,
            "push" => SlideTransition::Push,
            "wipe" => SlideTransition::Wipe,
            "cover" => SlideTransition::Cover,
            "pull" => SlideTransition::Pull,
            "split" => SlideTransition::Split,
            "randomBar" => SlideTransition::RandomBar,
            _ => return,
        };
        let dir = attr(attrs, "dir");
        self.dir = match self.transition {
            SlideTransition::Split => axis_dir(attr(attrs, "orient").as_deref()),
            SlideTransition::RandomBar => axis_dir(dir.as_deref()),
            t if t.has_side() => dir.as_deref().and_then(side_dir),
            _ => None,
        };
    }
}

fn side_dir(v: &str) -> Option<AnimDir> {
    match v {
        "l" => Some(AnimDir::Left),
        "r" => Some(AnimDir::Right),
        "u" => Some(AnimDir::Top),
        "d" => Some(AnimDir::Bottom),
        _ => None,
    }
}

/// The side that stands for an axis: `vert` is left, `horz` is top.
fn axis_dir(v: Option<&str>) -> Option<AnimDir> {
    match v {
        Some("vert") => Some(AnimDir::Left),
        Some("horz") => Some(AnimDir::Top),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_each_effect_with_its_side_and_timing() {
        assert_eq!(transition_xml(SlideTransition::None, None, None, false), "");
        assert_eq!(
            transition_xml(SlideTransition::Push, None, None, false),
            "<p:transition><p:push dir=\"l\"/></p:transition>"
        );
        assert_eq!(
            transition_xml(
                SlideTransition::Wipe,
                Some(AnimDir::Bottom),
                Some(3000),
                true
            ),
            "<p:transition advClick=\"0\" advTm=\"3000\"><p:wipe dir=\"d\"/></p:transition>"
        );
        assert_eq!(
            transition_xml(SlideTransition::Split, Some(AnimDir::Top), None, false),
            "<p:transition><p:split orient=\"horz\" dir=\"out\"/></p:transition>"
        );
        assert_eq!(
            transition_xml(SlideTransition::None, None, Some(500), false),
            "<p:transition advTm=\"500\"></p:transition>"
        );
        assert!(
            transition_xml(SlideTransition::Fade, None, Some(u32::MAX), false)
                .contains("advTm=\"86400000\"")
        );
    }
}

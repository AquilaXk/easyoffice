//! ODF presentation writers for the slide extensions of [`crate::SlideExt`]: hidden pages, backgrounds,
//! rotation transform, outline dash, shadow, list and paragraph styles, header/footer frames and the
//! animation timeline (`anim:par`). Frames carry no separate master placeholders: the footer fields sit
//! on the slide itself, the way the reference office suite writes slides it imported.

use std::collections::HashMap;
use std::fmt::Write;

use crate::pptx_master::footer_placeholder_specs;
use crate::rich_image_writers::MediaParts;
use crate::rich_slide::{SlideBox, SlideTextAlign, EMU_PER_PT};
use crate::rich_slide_ext::{
    hex6, AnimClass, AnimDir, AnimEffect, AnimStart, ElementFx, SlideAnim, SlideBackground,
    SlideBullet, SlideDash, SlideDate, SlideExt, SlideNumberStyle, SlideParaFmt, SlideShadow,
};
use crate::XmlSanitizer;

pub(crate) const LOEXT_NS: &str =
    "urn:org:documentfoundation:names:experimental:office:xmlns:loext:1.0";
pub(crate) const DASH_NAME: &str = "eo_Dash";
pub(crate) const DOT_NAME: &str = "eo_Dot";
const FOOTER_COLOR: &str = "8B8B8B";
const FOOTER_PT: f64 = 12.0;

/// Gradient and picture fills that pages refer to by name; they live in `styles.xml`.
#[derive(Default)]
pub(crate) struct OdpFills {
    xml: String,
    gradients: usize,
    images: usize,
}

impl OdpFills {
    /// Name of a new linear gradient; `angle_deg` runs clockwise from left to right (OOXML), ODF turns
    /// a top-to-bottom gradient counterclockwise.
    pub(crate) fn gradient(&mut self, from: &str, to: &str, angle_deg: u16) -> String {
        self.gradients += 1;
        let name = format!("eoGrad{}", self.gradients);
        let odf = (90 + 360 - i32::from(angle_deg) % 360) % 360;
        let (from, to) = (hex6(from), hex6(to));
        let _ = write!(
            self.xml,
            "<draw:gradient draw:name=\"{name}\" draw:style=\"linear\" draw:start-color=\"#{from}\" draw:end-color=\"#{to}\" draw:start-intensity=\"100%\" draw:end-intensity=\"100%\" draw:angle=\"{odf}deg\" draw:border=\"0%\"/>"
        );
        name
    }

    pub(crate) fn image(&mut self, href: &str) -> String {
        self.images += 1;
        let name = format!("eoBg{}", self.images);
        let _ = write!(
            self.xml,
            "<draw:fill-image draw:name=\"{name}\" xlink:href=\"{href}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/>"
        );
        name
    }

    /// Fragment for `office:styles`: the dash definitions every deck uses and the fills collected so far.
    pub(crate) fn styles_xml(&self) -> String {
        format!(
            "<draw:stroke-dash draw:name=\"{DASH_NAME}\" draw:style=\"rect\" draw:dots1=\"1\" draw:dots1-length=\"0.4cm\" draw:distance=\"0.2cm\"/><draw:stroke-dash draw:name=\"{DOT_NAME}\" draw:style=\"rect\" draw:dots1=\"1\" draw:dots1-length=\"0.1cm\" draw:distance=\"0.1cm\"/>{}",
            self.xml
        )
    }
}

/// `style:drawing-page-properties` attributes: visibility and background fill.
pub(crate) fn page_attrs(ext: &SlideExt, fills: &mut OdpFills, media: &mut MediaParts) -> String {
    let mut s = String::new();
    if ext.hidden {
        s.push_str(" presentation:visibility=\"hidden\"");
    }
    match &ext.background {
        None => {}
        Some(SlideBackground::Solid(c)) => {
            let c = hex6(c);
            let _ = write!(s, " draw:fill=\"solid\" draw:fill-color=\"#{c}\"");
        }
        Some(SlideBackground::Gradient {
            from,
            to,
            angle_deg,
        }) => {
            let name = fills.gradient(from, to, *angle_deg);
            let _ = write!(
                s,
                " draw:fill=\"gradient\" draw:fill-gradient-name=\"{name}\""
            );
        }
        Some(SlideBackground::Picture(img)) => {
            let path = media.add("Pictures", img).path;
            let name = fills.image(&path);
            let _ = write!(
                s,
                " draw:fill=\"bitmap\" draw:fill-image-name=\"{name}\" style:repeat=\"stretch\""
            );
        }
    }
    s
}

pub(crate) fn dash_attrs(dash: SlideDash) -> String {
    match dash {
        SlideDash::Solid => " draw:stroke=\"solid\"".to_string(),
        SlideDash::Dash => format!(" draw:stroke=\"dash\" draw:stroke-dash=\"{DASH_NAME}\""),
        SlideDash::Dot => format!(" draw:stroke=\"dash\" draw:stroke-dash=\"{DOT_NAME}\""),
    }
}

pub(crate) fn shadow_attrs(shadow: Option<&SlideShadow>) -> String {
    let Some(s) = shadow else {
        return String::new();
    };
    let rad = s.angle_deg.to_radians();
    format!(
        " draw:shadow=\"visible\" draw:shadow-offset-x=\"{:.3}pt\" draw:shadow-offset-y=\"{:.3}pt\" draw:shadow-color=\"#{}\" draw:shadow-opacity=\"{}%\" loext:shadow-blur=\"{:.3}pt\"",
        s.dist_pt * rad.cos(),
        s.dist_pt * rad.sin(),
        hex6(&s.color),
        s.alpha_pct,
        s.blur_pt
    )
}

/// `text:list-style` for a bulleted or numbered paragraph.
pub(crate) fn list_style_xml(name: &str, bullet: SlideBullet) -> Option<String> {
    let props = "<style:list-level-properties text:min-label-width=\"0.953cm\"/>";
    match bullet {
        SlideBullet::None => None,
        SlideBullet::Char(c) => Some(format!(
            "<text:list-style style:name=\"{name}\"><text:list-level-style-bullet text:level=\"1\" text:bullet-char=\"{}\">{props}</text:list-level-style-bullet></text:list-style>",
            XmlSanitizer::escape(&c.to_string())
        )),
        SlideBullet::Number(n) => {
            let (format, suffix) = number_format(n);
            Some(format!(
                "<text:list-style style:name=\"{name}\"><text:list-level-style-number text:level=\"1\" style:num-suffix=\"{suffix}\" style:num-format=\"{format}\">{props}</text:list-level-style-number></text:list-style>"
            ))
        }
    }
}

pub(crate) fn number_format(n: SlideNumberStyle) -> (&'static str, &'static str) {
    match n {
        SlideNumberStyle::ArabicPeriod => ("1", "."),
        SlideNumberStyle::ArabicParen => ("1", ")"),
        SlideNumberStyle::AlphaUpperPeriod => ("A", "."),
        SlideNumberStyle::AlphaLowerPeriod => ("a", "."),
        SlideNumberStyle::RomanUpperPeriod => ("I", "."),
    }
}

/// `style:paragraph-properties` attributes of a paragraph (alignment, line height, spacing).
pub(crate) fn para_attrs(align: SlideTextAlign, para: Option<&SlideParaFmt>) -> String {
    let odf_al = match (align, para.is_some_and(|p| p.justify)) {
        (_, true) => "justify",
        (SlideTextAlign::Left, _) => "start",
        (SlideTextAlign::Center, _) => "center",
        (SlideTextAlign::Right, _) => "end",
    };
    let mut s = format!(" fo:text-align=\"{odf_al}\"");
    if let Some(p) = para {
        if let Some(pct) = p.line_spacing_pct {
            let _ = write!(s, " fo:line-height=\"{pct}%\"");
        }
        if let Some(v) = p.space_before_pt {
            let _ = write!(s, " fo:margin-top=\"{v:.2}pt\"");
        }
        if let Some(v) = p.space_after_pt {
            let _ = write!(s, " fo:margin-bottom=\"{v:.2}pt\"");
        }
    }
    s
}

/// `draw:transform` for a rotated element, in the form the open-source office suite writes: a rotation (radians, counterclockwise
/// positive) followed by the translation of the rotated top-left corner.
pub(crate) fn transform_attr(r: SlideBox, fx: Option<&ElementFx>) -> Option<String> {
    let deg = fx.map_or(0.0, |f| f.rotation_deg);
    if deg == 0.0 {
        return None;
    }
    let rad = deg.to_radians();
    let (sin, cos) = rad.sin_cos();
    let (hw, hh) = (r.w / 2.0, r.h / 2.0);
    let (cx, cy) = (r.x + hw, r.y + hh);
    let (tx, ty) = (cx - (hw * cos - hh * sin), cy - (hw * sin + hh * cos));
    Some(format!(
        " draw:transform=\"rotate ({:.9}) translate ({tx:.3}pt {ty:.3}pt)\"",
        -rad
    ))
}

/// Position attributes: `svg:x`/`svg:y` for an upright element, the transform for a rotated one.
pub(crate) fn place_attrs(r: SlideBox, fx: Option<&ElementFx>) -> String {
    match transform_attr(r, fx) {
        Some(t) => format!("{t} svg:width=\"{:.3}pt\" svg:height=\"{:.3}pt\"", r.w, r.h),
        None => format!(
            "svg:x=\"{:.3}pt\" svg:y=\"{:.3}pt\" svg:width=\"{:.3}pt\" svg:height=\"{:.3}pt\"",
            r.x, r.y, r.w, r.h
        ),
    }
}

/// Footer frames, written after the page content; `text_style` and `para_style` style their text.
pub(crate) struct FooterFrame<'a> {
    pub class: &'static str,
    pub rect: SlideBox,
    pub align: SlideTextAlign,
    pub body: String,
    pub graphic: &'a str,
}

pub(crate) fn footer_frames<'a>(
    ext: &SlideExt,
    emu: (u64, u64),
    graphic: &'a str,
) -> Vec<FooterFrame<'a>> {
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
    let mut out = Vec::new();
    let f = &ext.footer;
    match &f.date {
        SlideDate::None => {}
        SlideDate::Auto => out.push(FooterFrame {
            class: "date-time",
            rect: rect(0),
            align: SlideTextAlign::Left,
            body: "<text:date text:fixed=\"false\">2026-01-01</text:date>".to_string(),
            graphic,
        }),
        SlideDate::Fixed(t) => out.push(FooterFrame {
            class: "date-time",
            rect: rect(0),
            align: SlideTextAlign::Left,
            body: XmlSanitizer::escape(t),
            graphic,
        }),
    }
    if let Some(t) = &f.text {
        out.push(FooterFrame {
            class: "footer",
            rect: rect(1),
            align: SlideTextAlign::Center,
            body: XmlSanitizer::escape(t),
            graphic,
        });
    }
    if f.number {
        out.push(FooterFrame {
            class: "page-number",
            rect: rect(2),
            align: SlideTextAlign::Right,
            body: "<text:page-number>&lt;number&gt;</text:page-number>".to_string(),
            graphic,
        });
    }
    out
}

pub(crate) const FOOTER_TEXT: (f64, &str) = (FOOTER_PT, FOOTER_COLOR);

fn clock(ms: u32) -> String {
    format!("{}s", f64::from(ms) / 1000.0)
}

fn preset_name(a: &SlideAnim) -> &'static str {
    match (a.class, a.effect) {
        (AnimClass::Entrance, AnimEffect::Appear) => "ooo-entrance-appear",
        (AnimClass::Entrance, AnimEffect::Fade) => "ooo-entrance-fade-in",
        (AnimClass::Entrance, AnimEffect::Fly) => "ooo-entrance-fly-in",
        (AnimClass::Entrance, AnimEffect::Wipe) => "ooo-entrance-wipe",
        (AnimClass::Entrance, AnimEffect::Zoom) => "ooo-entrance-zoom",
        (AnimClass::Exit, AnimEffect::Appear) => "ooo-exit-disappear",
        (AnimClass::Exit, AnimEffect::Fade) => "ooo-exit-fade-out",
        (AnimClass::Exit, AnimEffect::Fly) => "ooo-exit-fly-out",
        (AnimClass::Exit, AnimEffect::Wipe) => "ooo-exit-wipe",
        (AnimClass::Exit, AnimEffect::Zoom) => "ooo-exit-zoom",
        (_, AnimEffect::Spin) => "ooo-emphasis-spin",
        _ => "ooo-emphasis-grow-and-shrink",
    }
}

pub(crate) fn dir_attr(d: AnimDir) -> &'static str {
    match d {
        AnimDir::Left => "from-left",
        AnimDir::Right => "from-right",
        AnimDir::Top => "from-top",
        AnimDir::Bottom => "from-bottom",
    }
}

fn wipe_subtype(d: AnimDir) -> &'static str {
    match d {
        AnimDir::Left => "leftToRight",
        AnimDir::Right => "rightToLeft",
        AnimDir::Top => "topToBottom",
        AnimDir::Bottom => "bottomToTop",
    }
}

fn animate(out: &mut String, target: &str, dur: u32, attr: &str, values: &str) {
    let _ = write!(
        out,
        "<anim:animate smil:dur=\"{}\" smil:fill=\"hold\" smil:targetElement=\"{target}\" smil:attributeName=\"{attr}\" smil:values=\"{values}\" smil:keyTimes=\"0;1\"/>",
        clock(dur)
    );
}

fn set_visibility(out: &mut String, target: &str, shown: bool, begin_ms: u32) {
    let _ = write!(
        out,
        "<anim:set smil:begin=\"{}\" smil:dur=\"0.001s\" smil:fill=\"hold\" smil:targetElement=\"{target}\" smil:attributeName=\"visibility\" smil:to=\"{}\"/>",
        clock(begin_ms),
        if shown { "visible" } else { "hidden" }
    );
}

/// Behavior nodes of one effect on the shape with ODF id `t`.
fn behaviors(out: &mut String, t: &str, a: &SlideAnim) {
    let (dur, entering) = (a.duration_ms.max(1), a.class == AnimClass::Entrance);
    if entering {
        set_visibility(out, t, true, 0);
    }
    let filter = |out: &mut String, kind: &str, sub: &str| {
        let mode = if entering { "" } else { " smil:mode=\"out\"" };
        let _ = write!(
            out,
            "<anim:transitionFilter smil:dur=\"{}\" smil:targetElement=\"{t}\" smil:type=\"{kind}\" smil:subtype=\"{sub}\"{mode}/>",
            clock(dur)
        );
    };
    match a.effect {
        AnimEffect::Appear => {}
        AnimEffect::Fade => filter(out, "fade", "crossfade"),
        AnimEffect::Wipe => filter(out, "barWipe", wipe_subtype(a.direction)),
        AnimEffect::Fly => {
            let (x, y) = match a.direction {
                AnimDir::Left => ("0-width/2", "y"),
                AnimDir::Right => ("1+width/2", "y"),
                AnimDir::Top => ("x", "0-height/2"),
                AnimDir::Bottom => ("x", "1+height/2"),
            };
            let (xv, yv) = if entering {
                (format!("{x};x"), format!("{y};y"))
            } else {
                (format!("x;{x}"), format!("y;{y}"))
            };
            animate(out, t, dur, "x", &xv);
            animate(out, t, dur, "y", &yv);
        }
        AnimEffect::Zoom => {
            let (w, h) = if entering {
                ("0;width", "0;height")
            } else {
                ("width;0", "height;0")
            };
            animate(out, t, dur, "width", w);
            animate(out, t, dur, "height", h);
            filter(out, "fade", "crossfade");
        }
        AnimEffect::Spin => {
            let _ = write!(out, "<anim:animateTransform smil:dur=\"{}\" smil:fill=\"hold\" smil:targetElement=\"{t}\" smil:by=\"360\" smil:attributeName=\"transform\" svg:type=\"rotate\"/>", clock(dur));
        }
        AnimEffect::GrowShrink => {
            let _ = write!(out, "<anim:animateTransform smil:dur=\"{}\" smil:fill=\"hold\" smil:targetElement=\"{t}\" smil:by=\"1.5,1.5\" smil:attributeName=\"transform\" svg:type=\"scale\"/>", clock(dur));
        }
    }
    if a.class == AnimClass::Exit {
        set_visibility(out, t, false, dur.saturating_sub(1));
    }
}

/// `anim:par` timeline of a page; `ids` maps element and group ids to the ODF ids their frames carry.
pub(crate) fn push_timeline(out: &mut String, anims: &[SlideAnim], ids: &HashMap<String, String>) {
    let steps: Vec<(&SlideAnim, &String)> = anims
        .iter()
        .filter_map(|a| ids.get(&a.target).map(|i| (a, i)))
        .collect();
    if steps.is_empty() {
        return;
    }
    let mut clicks: Vec<Vec<Vec<(&SlideAnim, &String)>>> = Vec::new();
    for &step in &steps {
        match step.0.start {
            AnimStart::OnClick => clicks.push(vec![vec![step]]),
            AnimStart::AfterPrevious => match clicks.last_mut() {
                Some(c) => c.push(vec![step]),
                None => clicks.push(vec![vec![step]]),
            },
            AnimStart::WithPrevious => match clicks.last_mut().and_then(|c| c.last_mut()) {
                Some(g) => g.push(step),
                None => clicks.push(vec![vec![step]]),
            },
        }
    }
    out.push_str("<anim:par smil:dur=\"indefinite\" smil:restart=\"never\" presentation:node-type=\"timing-root\"><anim:seq smil:dur=\"indefinite\" presentation:node-type=\"main-sequence\">");
    for click in &clicks {
        out.push_str("<anim:par smil:begin=\"next\" smil:fill=\"hold\">");
        let mut start_ms = 0u32;
        for group in click {
            let _ = write!(
                out,
                "<anim:par smil:begin=\"{}\" smil:fill=\"hold\">",
                clock(start_ms)
            );
            for &(a, t) in group {
                let node = match a.start {
                    AnimStart::OnClick => "on-click",
                    AnimStart::WithPrevious => "with-previous",
                    AnimStart::AfterPrevious => "after-previous",
                };
                let class = match a.class {
                    AnimClass::Entrance => "entrance",
                    AnimClass::Emphasis => "emphasis",
                    AnimClass::Exit => "exit",
                };
                let sub = if matches!(a.effect, AnimEffect::Fly | AnimEffect::Wipe) {
                    format!(
                        " presentation:preset-sub-type=\"{}\"",
                        dir_attr(a.direction)
                    )
                } else {
                    String::new()
                };
                let _ = write!(out, "<anim:par smil:begin=\"{}\" smil:fill=\"hold\" presentation:node-type=\"{node}\" presentation:preset-class=\"{class}\" presentation:preset-id=\"{}\"{sub}>", clock(a.delay_ms), preset_name(a));
                behaviors(out, t, a);
                out.push_str("</anim:par>");
            }
            out.push_str("</anim:par>");
            let longest = group
                .iter()
                .map(|(a, _)| a.delay_ms.saturating_add(a.duration_ms))
                .max()
                .unwrap_or(0);
            start_ms = start_ms.saturating_add(longest);
        }
        out.push_str("</anim:par>");
    }
    out.push_str("</anim:seq></anim:par>");
}

//! ODF presentation readers for the slide extensions of [`crate::SlideExt`]: hidden pages and page
//! backgrounds, rotation transforms, outline dash, shadow, list and paragraph styles, header/footer
//! frames, groups (`draw:g`) and the animation timeline (`anim:par`). Gathered state goes through the
//! same accumulator and `finish_ext` the PPTX reader uses.

use std::collections::HashMap;

use crate::odp_ext_write::DOT_NAME;
use crate::pptx_ext_read::{ExtAccum, RawAnim};
use crate::rich_document::normalize_hex_color;
use crate::rich_slide::{parse_length_pt, SlideBox};
use crate::rich_slide_ext::{
    AnimClass, AnimDir, AnimEffect, AnimStart, SlideBullet, SlideDash, SlideDate, SlideNumberStyle,
    SlideParaFmt, SlideShadow, DEFAULT_ANIM_MS, MAX_ANIM_MS, MAX_LINE_SPACING_PCT,
    MAX_PARA_SPACE_PT, MAX_SHADOW_PT, MAX_SLIDE_ANIMS, MIN_LINE_SPACING_PCT,
};
use crate::xml_stream::{attr, Attrs, XmlEvent};
use crate::EasyOfficeError;

fn loc(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

fn attr_of(attrs: &Attrs<'_>, qualified: &str) -> Option<String> {
    attr(attrs, qualified)
        .or_else(|| attr(attrs, loc(qualified)))
        .map(|s| s.into_owned())
}

/// How a drawing-page style fills its page.
#[derive(Clone, Default)]
pub(crate) struct PageStyle {
    pub hidden: bool,
    fill: String,
    color: Option<String>,
    name: Option<String>,
}

#[derive(Clone, Default)]
pub(crate) struct GraphicFx {
    dash: Option<String>,
    pub shadow: Option<SlideShadow>,
    pub flip: (bool, bool),
}

#[derive(Clone, Copy, Default)]
pub(crate) struct ParaFx {
    pub justify: bool,
    pub line_pct: Option<u16>,
    pub before: Option<f64>,
    pub after: Option<f64>,
}

/// Named definitions and style properties the extensions need; filled from `styles.xml` and the
/// automatic styles of `content.xml`.
#[derive(Default)]
pub(crate) struct OdpExtStyles {
    gradients: HashMap<String, (String, String, u16)>,
    images: HashMap<String, String>,
    dashes: HashMap<String, SlideDash>,
    pages: HashMap<String, PageStyle>,
    graphics: HashMap<String, GraphicFx>,
    paras: HashMap<String, ParaFx>,
    lists: HashMap<String, SlideBullet>,
}

/// The files keep lengths and angles to a few decimals, so values read back are rounded to 0.01.
/// A value that overflows while it is rounded (a length of 1e308) reads as 0.
fn round2(v: f64) -> f64 {
    let r = (v * 100.0).round() / 100.0;
    if r.is_finite() {
        r
    } else {
        0.0
    }
}

fn color_attr(attrs: &Attrs<'_>, key: &str) -> Option<String> {
    attr_of(attrs, key).and_then(|v| normalize_hex_color(&v))
}

fn angle_deg(raw: &str) -> f64 {
    let t = raw.trim();
    t.strip_suffix("deg")
        .and_then(|n| n.trim().parse::<f64>().ok())
        .or_else(|| {
            t.parse::<f64>()
                .ok()
                .map(|v| if v.abs() > 360.0 { v / 10.0 } else { v })
        })
        .filter(|v| v.is_finite())
        .unwrap_or(0.0)
}

fn dash_kind(name: &str) -> SlideDash {
    if name == DOT_NAME || (name.contains("Dot") && !name.contains("Dash")) {
        SlideDash::Dot
    } else {
        SlideDash::Dash
    }
}

fn pct(raw: &str) -> Option<u16> {
    let v = raw.trim().strip_suffix('%')?.trim().parse::<f64>().ok()?;
    v.is_finite().then(|| {
        v.round().clamp(
            f64::from(MIN_LINE_SPACING_PCT),
            f64::from(MAX_LINE_SPACING_PCT),
        ) as u16
    })
}

fn number_style(format: &str, suffix: &str) -> SlideNumberStyle {
    match (format, suffix) {
        ("A", _) => SlideNumberStyle::AlphaUpperPeriod,
        ("a", _) => SlideNumberStyle::AlphaLowerPeriod,
        ("I", _) => SlideNumberStyle::RomanUpperPeriod,
        (_, ")") => SlideNumberStyle::ArabicParen,
        _ => SlideNumberStyle::ArabicPeriod,
    }
}

impl OdpExtStyles {
    pub(crate) fn page(&self, style: &str) -> PageStyle {
        self.pages.get(style).cloned().unwrap_or_default()
    }

    pub(crate) fn graphic(&self, style: Option<&str>) -> GraphicFx {
        style
            .and_then(|s| self.graphics.get(s))
            .cloned()
            .unwrap_or_default()
    }

    pub(crate) fn dash_of(&self, g: &GraphicFx) -> SlideDash {
        g.dash.as_deref().map_or(SlideDash::Solid, |n| {
            self.dashes.get(n).copied().unwrap_or_else(|| dash_kind(n))
        })
    }

    pub(crate) fn para(&self, style: Option<&str>) -> ParaFx {
        style
            .and_then(|s| self.paras.get(s))
            .copied()
            .unwrap_or_default()
    }

    pub(crate) fn bullet(&self, list_style: Option<&str>) -> SlideBullet {
        list_style
            .and_then(|s| self.lists.get(s))
            .copied()
            .unwrap_or_default()
    }

    /// Reads named fills and the extension-relevant properties of every style in `events`.
    pub(crate) fn collect(&mut self, events: &[XmlEvent<'_>]) {
        let (mut name, mut family, mut list_name) = (String::new(), String::new(), String::new());
        let mut list_seen = false;
        for ev in events {
            let XmlEvent::Start { name: tag, attrs } = ev else {
                continue;
            };
            let n = attr_of(attrs, "style:name")
                .or_else(|| attr_of(attrs, "draw:name"))
                .unwrap_or_default();
            match loc(tag) {
                "gradient" => {
                    let (from, to) = (
                        color_attr(attrs, "draw:start-color"),
                        color_attr(attrs, "draw:end-color"),
                    );
                    if let (Some(from), Some(to)) = (from, to) {
                        let odf = angle_deg(&attr_of(attrs, "draw:angle").unwrap_or_default());
                        let ooxml = (90.0 - odf).rem_euclid(360.0).round() as u16 % 360;
                        self.gradients.insert(n, (from, to, ooxml));
                    }
                }
                "fill-image" => {
                    if let Some(href) = attr_of(attrs, "xlink:href") {
                        self.images.insert(n, href);
                    }
                }
                "stroke-dash" => {
                    self.dashes.insert(n.clone(), dash_kind(&n));
                }
                "style" => {
                    name = attr_of(attrs, "style:name").unwrap_or_default();
                    family = attr_of(attrs, "style:family").unwrap_or_default();
                }
                "list-style" => {
                    list_name = attr_of(attrs, "style:name").unwrap_or_default();
                    list_seen = false;
                }
                "list-level-style-bullet" | "list-level-style-number"
                    if !list_seen && !list_name.is_empty() =>
                {
                    list_seen = true;
                    let bullet = if loc(tag) == "list-level-style-bullet" {
                        attr_of(attrs, "text:bullet-char")
                            .and_then(|c| c.chars().next())
                            .map(SlideBullet::Char)
                    } else {
                        let format = attr_of(attrs, "style:num-format").unwrap_or_default();
                        let suffix = attr_of(attrs, "style:num-suffix").unwrap_or_default();
                        Some(SlideBullet::Number(number_style(&format, &suffix)))
                    };
                    if let Some(b) = bullet {
                        self.lists.insert(list_name.clone(), b);
                    }
                }
                "drawing-page-properties" if family == "drawing-page" => {
                    let page = PageStyle {
                        hidden: attr_of(attrs, "presentation:visibility")
                            .is_some_and(|v| v == "hidden"),
                        fill: attr_of(attrs, "draw:fill").unwrap_or_default(),
                        color: color_attr(attrs, "draw:fill-color"),
                        name: attr_of(attrs, "draw:fill-gradient-name")
                            .or_else(|| attr_of(attrs, "draw:fill-image-name")),
                    };
                    self.pages.insert(name.clone(), page);
                }
                "graphic-properties" if family == "graphic" => {
                    self.graphics.insert(name.clone(), graphic_fx(attrs));
                }
                "paragraph-properties" if family == "paragraph" => {
                    let len = |k: &str| {
                        attr_of(attrs, k)
                            .and_then(|v| parse_length_pt(&v))
                            .filter(|v| *v > 0.0)
                            .map(|v| v.min(MAX_PARA_SPACE_PT))
                    };
                    self.paras.insert(
                        name.clone(),
                        ParaFx {
                            justify: attr_of(attrs, "fo:text-align")
                                .is_some_and(|v| v == "justify"),
                            line_pct: attr_of(attrs, "fo:line-height")
                                .and_then(|v| pct(&v))
                                .filter(|p| *p != 100),
                            before: len("fo:margin-top"),
                            after: len("fo:margin-bottom"),
                        },
                    );
                }
                _ => {}
            }
        }
    }

    /// Fill of a page style as a background.
    pub(crate) fn background(
        &self,
        page: &PageStyle,
        mut resolve_image: impl FnMut(&str) -> Result<crate::RichImage, EasyOfficeError>,
    ) -> Result<Option<crate::SlideBackground>, EasyOfficeError> {
        Ok(match page.fill.as_str() {
            "solid" => page.color.clone().map(crate::SlideBackground::Solid),
            "gradient" => {
                page.name
                    .as_ref()
                    .and_then(|n| self.gradients.get(n))
                    .map(|(from, to, angle)| crate::SlideBackground::Gradient {
                        from: from.clone(),
                        to: to.clone(),
                        angle_deg: *angle,
                    })
            }
            "bitmap" => match page.name.as_ref().and_then(|n| self.images.get(n)) {
                Some(href) => Some(crate::SlideBackground::Picture(resolve_image(href)?)),
                None => None,
            },
            _ => None,
        })
    }
}

fn graphic_fx(attrs: &Attrs<'_>) -> GraphicFx {
    let dash = if attr_of(attrs, "draw:stroke").is_some_and(|v| v == "dash") {
        attr_of(attrs, "draw:stroke-dash")
    } else {
        None
    };
    let mirror = attr_of(attrs, "style:mirror").unwrap_or_default();
    let shadow = (attr_of(attrs, "draw:shadow").as_deref() == Some("visible")).then(|| {
        let len = |k: &str| {
            attr_of(attrs, k)
                .and_then(|v| parse_length_pt(&v))
                .unwrap_or(0.0)
        };
        let (dx, dy) = (len("draw:shadow-offset-x"), len("draw:shadow-offset-y"));
        SlideShadow {
            color: color_attr(attrs, "draw:shadow-color").unwrap_or_else(|| "000000".to_string()),
            alpha_pct: attr_of(attrs, "draw:shadow-opacity")
                .and_then(|v| {
                    v.trim()
                        .strip_suffix('%')
                        .and_then(|n| n.parse::<f64>().ok())
                })
                .map_or(100, |v| v.clamp(0.0, 100.0).round() as u8),
            blur_pt: round2(len("loext:shadow-blur")).clamp(0.0, MAX_SHADOW_PT),
            dist_pt: round2(dx.hypot(dy)).min(MAX_SHADOW_PT),
            angle_deg: round2(dy.atan2(dx).to_degrees()).rem_euclid(360.0),
        }
    });
    GraphicFx {
        dash,
        shadow,
        flip: (mirror.contains("horizontal"), mirror.contains("vertical")),
    }
}

/// Rotation and translation of an element's `draw:transform`, in points; the angle is clockwise degrees.
pub(crate) struct OdfTransform {
    pub rotation_deg: f64,
    pub translate: Option<(f64, f64)>,
}

fn args_of<'a>(text: &'a str, func: &str) -> Option<&'a str> {
    let rest = &text[text.find(func)? + func.len()..];
    let open = rest.find('(')?;
    let close = rest[open..].find(')')? + open;
    Some(&rest[open + 1..close])
}

pub(crate) fn parse_transform(attrs: &Attrs<'_>) -> Option<OdfTransform> {
    let raw = attr_of(attrs, "draw:transform")?;
    let rotation_deg = args_of(&raw, "rotate")
        .and_then(|a| a.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .map_or(0.0, |rad| round2((-rad).to_degrees()).rem_euclid(360.0));
    let translate = args_of(&raw, "translate").and_then(|a| {
        let mut parts = a.split_whitespace();
        Some((
            parse_length_pt(parts.next()?)?,
            parse_length_pt(parts.next()?)?,
        ))
    });
    // A translation that overflows (1e308cm) is left out rather than refusing the file.
    let translate = translate.filter(|(x, y)| x.is_finite() && y.is_finite());
    Some(OdfTransform {
        rotation_deg,
        translate,
    })
}

/// Upright frame of an element from its raw box (`raw`, taken as written) and transform: a rotated
/// element written without `svg:x`/`svg:y` is placed through the translation of its rotated corner.
pub(crate) fn place_from_transform(raw: SlideBox, has_xy: bool, t: &OdfTransform) -> SlideBox {
    let (Some((tx, ty)), false) = (t.translate, has_xy) else {
        return raw;
    };
    let (sin, cos) = t.rotation_deg.to_radians().sin_cos();
    let (hw, hh) = (raw.w / 2.0, raw.h / 2.0);
    let (cx, cy) = (tx + hw * cos - hh * sin, ty + hw * sin + hh * cos);
    SlideBox {
        x: cx - hw,
        y: cy - hh,
        ..raw
    }
}

/// Bullet and paragraph format of a text body: the first list and the first paragraph decide.
pub(crate) fn scan_para(events: &[XmlEvent<'_>], styles: &OdpExtStyles) -> Option<SlideParaFmt> {
    let (mut list, mut para) = (None::<String>, None::<String>);
    for ev in events {
        if let XmlEvent::Start { name, attrs } = ev {
            match loc(name) {
                "list" if list.is_none() => list = attr_of(attrs, "text:style-name"),
                "p" | "h" if para.is_none() => para = attr_of(attrs, "text:style-name"),
                _ => {}
            }
        }
    }
    let p = styles.para(para.as_deref());
    let fmt = SlideParaFmt {
        justify: p.justify,
        bullet: styles.bullet(list.as_deref()),
        line_spacing_pct: p.line_pct,
        space_before_pt: p.before,
        space_after_pt: p.after,
    };
    (!fmt.is_default()).then_some(fmt)
}

/// Footer frame (`date-time`, `footer`, `page-number`) of a page.
pub(crate) fn apply_footer_frame(
    acc: &mut ExtAccum,
    class: &str,
    paras: &[String],
    events: &[XmlEvent<'_>],
) {
    let text = paras.join("\n");
    let auto_date = events.iter().any(|e| {
        matches!(e, XmlEvent::Start { name, attrs }
        if loc(name) == "date" && attr_of(attrs, "text:fixed").is_none_or(|v| v != "true"))
    });
    match class {
        "page-number" => acc.footer.number = true,
        "date-time" if auto_date => acc.footer.date = SlideDate::Auto,
        "date-time" if !text.is_empty() => {
            acc.footer.date = SlideDate::Fixed(crate::rich_slide_ext::clamp_footer_text(&text))
        }
        "footer" if !text.is_empty() => {
            acc.footer.text = Some(crate::rich_slide_ext::clamp_footer_text(&text))
        }
        _ => {}
    }
}

fn clock_ms(raw: &str) -> Option<u32> {
    let t = raw.trim();
    let ms = if let Some(n) = t.strip_suffix("ms") {
        n.trim().parse::<f64>().ok()?
    } else {
        t.strip_suffix('s')?.trim().parse::<f64>().ok()? * 1000.0
    };
    (ms.is_finite() && ms >= 0.0).then(|| (ms.round() as u64).min(u64::from(MAX_ANIM_MS)) as u32)
}

fn effect_of(preset: &str) -> Option<(AnimClass, AnimEffect)> {
    let (class, rest) = if let Some(r) = preset.strip_prefix("ooo-entrance-") {
        (AnimClass::Entrance, r)
    } else if let Some(r) = preset.strip_prefix("ooo-emphasis-") {
        (AnimClass::Emphasis, r)
    } else {
        (AnimClass::Exit, preset.strip_prefix("ooo-exit-")?)
    };
    let effect = match (class, rest) {
        (AnimClass::Entrance, "appear") | (AnimClass::Exit, "disappear") => AnimEffect::Appear,
        (AnimClass::Entrance, "fade-in") | (AnimClass::Exit, "fade-out") => AnimEffect::Fade,
        (AnimClass::Entrance, "fly-in") | (AnimClass::Exit, "fly-out") => AnimEffect::Fly,
        (AnimClass::Entrance | AnimClass::Exit, "wipe") => AnimEffect::Wipe,
        (AnimClass::Entrance | AnimClass::Exit, "zoom") => AnimEffect::Zoom,
        (AnimClass::Emphasis, "spin") => AnimEffect::Spin,
        (AnimClass::Emphasis, "grow-and-shrink") => AnimEffect::GrowShrink,
        _ => return None,
    };
    Some((class, effect))
}

/// Effects of the page's `anim:par` timeline that the editor can play; others are left out. More than
/// [`MAX_SLIDE_ANIMS`] effects refuse the page.
pub(crate) fn parse_timeline(
    events: &[XmlEvent<'_>],
    out: &mut Vec<RawAnim>,
) -> Result<(), EasyOfficeError> {
    let mut depth = 0usize;
    let mut cur: Option<(usize, RawAnim)> = None;
    for ev in events {
        match ev {
            XmlEvent::Start { name, attrs } => {
                depth += 1;
                if let Some((_, a)) = cur.as_mut() {
                    if let Some(t) =
                        attr_of(attrs, "smil:targetElement").filter(|_| a.spid.is_empty())
                    {
                        a.spid = t;
                    }
                    if let Some(d) = attr_of(attrs, "smil:dur")
                        .and_then(|v| clock_ms(&v))
                        .filter(|d| *d > 1)
                    {
                        a.duration_ms = a.duration_ms.max(d);
                    }
                } else if loc(name) == "par" {
                    let start = match attr_of(attrs, "presentation:node-type").as_deref() {
                        Some("on-click") => Some(AnimStart::OnClick),
                        Some("with-previous") => Some(AnimStart::WithPrevious),
                        Some("after-previous") => Some(AnimStart::AfterPrevious),
                        _ => None,
                    };
                    let class_effect =
                        attr_of(attrs, "presentation:preset-id").and_then(|p| effect_of(&p));
                    if let (Some(start), Some((class, effect))) = (start, class_effect) {
                        let direction =
                            match attr_of(attrs, "presentation:preset-sub-type").as_deref() {
                                Some("from-left") => AnimDir::Left,
                                Some("from-right") => AnimDir::Right,
                                Some("from-top") => AnimDir::Top,
                                _ => AnimDir::Bottom,
                            };
                        let delay = attr_of(attrs, "smil:begin")
                            .and_then(|v| clock_ms(&v))
                            .unwrap_or(0);
                        let raw = RawAnim {
                            spid: String::new(),
                            class,
                            effect,
                            start,
                            direction,
                            duration_ms: 0,
                            delay_ms: delay,
                        };
                        cur = Some((depth, raw));
                    }
                }
            }
            XmlEvent::End(_) => {
                if cur.as_ref().is_some_and(|(d, _)| *d == depth) {
                    if let Some((_, mut a)) = cur.take() {
                        if a.duration_ms == 0 {
                            a.duration_ms = DEFAULT_ANIM_MS;
                        }
                        if !a.spid.is_empty() {
                            if out.len() >= MAX_SLIDE_ANIMS {
                                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                                    "ODP 슬라이드 애니메이션 수가 상한({MAX_SLIDE_ANIMS})을 초과했습니다."
                                )));
                            }
                            out.push(a);
                        }
                    }
                }
                depth = depth.saturating_sub(1);
            }
            XmlEvent::Text(_) => {}
        }
    }
    Ok(())
}

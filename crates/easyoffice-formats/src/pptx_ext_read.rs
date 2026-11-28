//! PresentationML readers for the slide extensions of [`crate::SlideExt`]: background, rotation and
//! flip, outline dash, outer shadow, paragraph format, header/footer fields, groups and the animation
//! timeline. Every list is capped; a slide that exceeds a cap is refused (Fail-Closed).

use std::collections::{HashMap, HashSet};

use crate::pptx_rich_read::{loc, resolve_rel_entry, SlideCtx};
use crate::pptx_theme_color::{ColorSpecBuilder, PptxClrMap, PptxThemePalette};
use crate::rich_image::RichImage;
use crate::rich_slide::{emu_to_pt, SlideElement, MAX_SLIDE_STROKE_PT};
use crate::rich_slide_ext::{
    AnimClass, AnimDir, AnimEffect, AnimStart, ElementFx, SlideAnim, SlideBackground, SlideBullet,
    SlideDash, SlideDate, SlideExt, SlideFooter, SlideGroup, SlideNumberStyle, SlideOutline,
    SlideParaFmt, SlideShadow, DEFAULT_ANIM_MS, MAX_ANIM_MS, MAX_LINE_SPACING_PCT,
    MAX_PARA_SPACE_PT, MAX_SHADOW_PT, MAX_SLIDE_ANIMS, MAX_SLIDE_GROUPS, MIN_LINE_SPACING_PCT,
};
use crate::xml_stream::{attr, XmlEvent};
use crate::EasyOfficeError;

const ANGLE_UNIT: f64 = 60_000.0;

/// What a group member is: an element (by position in the slide) or another group.
#[derive(Clone, Copy)]
pub(crate) enum Member {
    El(usize),
    Group(usize),
}

pub(crate) struct PendingGroup {
    pub name: Option<String>,
    pub members: Vec<Member>,
}

/// One animation as read, still pointing at its target by the file's own shape id.
pub(crate) struct RawAnim {
    pub spid: String,
    pub class: AnimClass,
    pub effect: AnimEffect,
    pub start: AnimStart,
    pub direction: AnimDir,
    pub duration_ms: u32,
    pub delay_ms: u32,
}

/// Slide-extension state gathered while the shape tree is walked.
#[derive(Default)]
pub(crate) struct ExtAccum {
    pub fx: Vec<(usize, ElementFx)>,
    pub spids: HashMap<String, Member>,
    pub members: Vec<Member>,
    pub groups: Vec<PendingGroup>,
    pub footer: SlideFooter,
    pub background: Option<SlideBackground>,
    pub anims: Vec<RawAnim>,
}

/// What [`scan_fx`] found on one shape: its drawing id and its effects.
pub(crate) struct FxScan {
    pub spid: Option<String>,
    pub fx: ElementFx,
    /// Outline as written, for elements that keep it in the overlay rather than as a stroke.
    pub line: Option<SlideOutline>,
}

fn resolve(
    builder: ColorSpecBuilder,
    palette: &PptxThemePalette,
    clr_map: &PptxClrMap,
) -> Option<String> {
    builder
        .finish()
        .and_then(|s| s.resolve_hex(palette, clr_map, None))
}

/// Color of the element that starts at `events[at]`, with its alpha (0-100) when it has one.
fn color_at(
    events: &[XmlEvent<'_>],
    at: usize,
    palette: &PptxThemePalette,
    clr_map: &PptxClrMap,
) -> Option<(String, Option<u8>)> {
    let XmlEvent::Start { name, attrs } = events.get(at)? else {
        return None;
    };
    let mut builder = ColorSpecBuilder::from_start(loc(name), attrs)?;
    let (mut depth, mut alpha) = (1usize, None);
    for ev in &events[at + 1..] {
        match ev {
            XmlEvent::Start { name, attrs } => {
                depth += 1;
                let ln = loc(name);
                if ln == "alpha" {
                    alpha = attr(attrs, "val")
                        .and_then(|v| v.trim().parse::<f64>().ok())
                        .map(|v| (v / 1_000.0).clamp(0.0, 100.0).round() as u8);
                } else {
                    builder.push_transform(ln, attrs);
                }
            }
            XmlEvent::End(_) => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            XmlEvent::Text(_) => {}
        }
    }
    resolve(builder, palette, clr_map).map(|c| (c, alpha))
}

fn is_color(name: &str) -> bool {
    matches!(name, "srgbClr" | "schemeClr" | "sysClr" | "prstClr")
}

fn num<T: std::str::FromStr>(attrs: &crate::xml_stream::Attrs<'_>, key: &str) -> Option<T> {
    attr(attrs, key).and_then(|v| v.trim().parse().ok())
}

fn dash_of(val: &str) -> SlideDash {
    match val {
        "dot" | "sysDot" => SlideDash::Dot,
        "solid" => SlideDash::Solid,
        _ => SlideDash::Dash,
    }
}

fn bullet_of(name: &str, attrs: &crate::xml_stream::Attrs<'_>) -> Option<SlideBullet> {
    match name {
        "buNone" => Some(SlideBullet::None),
        "buChar" => attr(attrs, "char")
            .and_then(|c| c.chars().next())
            .map(SlideBullet::Char),
        "buAutoNum" => Some(SlideBullet::Number(
            attr(attrs, "type")
                .and_then(|t| SlideNumberStyle::parse(&t))
                .unwrap_or(SlideNumberStyle::ArabicPeriod),
        )),
        _ => None,
    }
}

/// Reads rotation, flips, outline, shadow and the first paragraph's format from one shape subtree.
pub(crate) fn scan_fx(
    events: &[XmlEvent<'_>],
    palette: &PptxThemePalette,
    clr_map: &PptxClrMap,
) -> FxScan {
    let mut out = FxScan {
        spid: None,
        fx: ElementFx::default(),
        line: None,
    };
    let mut stack: Vec<&str> = Vec::new();
    let (mut ppr_seen, mut ln_w, mut ln_color) = (false, None::<f64>, None::<String>);
    let mut para = SlideParaFmt::default();
    let mut shadow: Option<SlideShadow> = None;
    for (i, ev) in events.iter().enumerate() {
        match ev {
            XmlEvent::Start { name, attrs } => {
                let ln = loc(name);
                let (parent, grand) = (
                    stack.last().copied().unwrap_or(""),
                    stack
                        .len()
                        .checked_sub(2)
                        .and_then(|k| stack.get(k))
                        .copied()
                        .unwrap_or(""),
                );
                match (ln, parent) {
                    ("cNvPr", _) if out.spid.is_none() => {
                        out.spid = attr(attrs, "id").map(|s| s.into_owned())
                    }
                    ("xfrm", "spPr") => {
                        let rot = num::<f64>(attrs, "rot").unwrap_or(0.0) / ANGLE_UNIT;
                        out.fx.rotation_deg = rot.rem_euclid(360.0);
                        out.fx.flip_h =
                            attr(attrs, "flipH").is_some_and(|v| v == "1" || v == "true");
                        out.fx.flip_v =
                            attr(attrs, "flipV").is_some_and(|v| v == "1" || v == "true");
                    }
                    ("ln", "spPr") => {
                        ln_w = num::<f64>(attrs, "w")
                            .map(|w| emu_to_pt(w).clamp(0.0, MAX_SLIDE_STROKE_PT))
                    }
                    ("prstDash", "ln") => {
                        out.fx.dash = attr(attrs, "val").map_or(SlideDash::Solid, |v| dash_of(&v))
                    }
                    ("outerShdw", "effectLst") => {
                        shadow = Some(SlideShadow {
                            color: "000000".to_string(),
                            alpha_pct: 100,
                            blur_pt: emu_to_pt(num::<f64>(attrs, "blurRad").unwrap_or(0.0))
                                .clamp(0.0, MAX_SHADOW_PT),
                            dist_pt: emu_to_pt(num::<f64>(attrs, "dist").unwrap_or(0.0))
                                .clamp(0.0, MAX_SHADOW_PT),
                            angle_deg: (num::<f64>(attrs, "dir").unwrap_or(0.0) / ANGLE_UNIT)
                                .rem_euclid(360.0),
                        });
                    }
                    (c, "outerShdw") if is_color(c) => {
                        if let (Some(s), Some((hex, alpha))) =
                            (shadow.as_mut(), color_at(events, i, palette, clr_map))
                        {
                            s.color = hex;
                            s.alpha_pct = alpha.unwrap_or(100);
                        }
                    }
                    (c, "solidFill") if is_color(c) && grand == "ln" && ln_color.is_none() => {
                        ln_color = color_at(events, i, palette, clr_map).map(|c| c.0);
                    }
                    ("pPr", "p") if !ppr_seen && stack.contains(&"txBody") => {
                        ppr_seen = true;
                        para.justify = attr(attrs, "algn").is_some_and(|a| a == "just");
                    }
                    ("spcPct", "lnSpc") if ppr_seen => {
                        para.line_spacing_pct = num::<f64>(attrs, "val").map(|v| {
                            (v / 1_000.0).round().clamp(
                                f64::from(MIN_LINE_SPACING_PCT),
                                f64::from(MAX_LINE_SPACING_PCT),
                            ) as u16
                        });
                    }
                    ("spcPts", "spcBef" | "spcAft") if ppr_seen => {
                        let pts = num::<f64>(attrs, "val")
                            .map(|v| (v / 100.0).clamp(0.0, MAX_PARA_SPACE_PT));
                        if parent == "spcBef" {
                            para.space_before_pt = pts;
                        } else {
                            para.space_after_pt = pts;
                        }
                    }
                    (b, "pPr") if ppr_seen && para.bullet == SlideBullet::None => {
                        if let Some(bu) = bullet_of(b, attrs) {
                            para.bullet = bu;
                        }
                    }
                    _ => {}
                }
                stack.push(ln);
            }
            XmlEvent::End(_) => {
                stack.pop();
            }
            XmlEvent::Text(_) => {}
        }
    }
    out.fx.shadow = shadow;
    out.fx.para = (!para.is_default()).then_some(para);
    out.line = ln_color.map(|color| SlideOutline {
        color,
        width_pt: ln_w.unwrap_or(0.75),
    });
    out
}

/// Slide background from `p:bg` (`p:bgPr` fill or the theme reference `p:bgRef`).
pub(crate) fn parse_bg(
    events: &[XmlEvent<'_>],
    ctx: &mut SlideCtx<'_>,
) -> Result<Option<SlideBackground>, EasyOfficeError> {
    let (mut stops, mut angle, mut rid) = (Vec::<String>::new(), 0u16, None::<String>);
    let (mut solid, mut kind) = (None::<String>, "");
    let mut stack: Vec<&str> = Vec::new();
    for (i, ev) in events.iter().enumerate() {
        match ev {
            XmlEvent::Start { name, attrs } => {
                let ln = loc(name);
                let parent = stack.last().copied().unwrap_or("");
                match (ln, parent) {
                    ("solidFill" | "gradFill" | "blipFill" | "bgRef", _) if kind.is_empty() => {
                        kind = ln
                    }
                    ("lin", "gradFill") => {
                        angle = (num::<f64>(attrs, "ang").unwrap_or(0.0) / ANGLE_UNIT)
                            .rem_euclid(360.0) as u16
                    }
                    ("blip", "blipFill") => {
                        rid = attr(attrs, "r:embed")
                            .or_else(|| attr(attrs, "embed"))
                            .map(|s| s.into_owned())
                    }
                    (c, "solidFill" | "bgRef") if is_color(c) && solid.is_none() => {
                        solid = color_at(events, i, ctx.palette, ctx.clr_map).map(|c| c.0);
                    }
                    (c, "gs") if is_color(c) && kind == "gradFill" => {
                        if let Some((hex, _)) = color_at(events, i, ctx.palette, ctx.clr_map) {
                            stops.push(hex);
                        }
                    }
                    _ => {}
                }
                stack.push(ln);
            }
            XmlEvent::End(_) => {
                stack.pop();
            }
            XmlEvent::Text(_) => {}
        }
    }
    Ok(match kind {
        "solidFill" | "bgRef" => solid.map(SlideBackground::Solid),
        "gradFill" => match (stops.first(), stops.last()) {
            (Some(from), Some(to)) => Some(SlideBackground::Gradient {
                from: from.clone(),
                to: to.clone(),
                angle_deg: angle,
            }),
            _ => None,
        },
        "blipFill" => {
            let Some(rid) = rid else { return Ok(None) };
            let (path, entry) = resolve_rel_entry(&rid, "슬라이드 배경 그림", ctx)?;
            let img = match ctx.cache.get(path) {
                Some(c) => c.clone(),
                None => {
                    let built = RichImage::new(entry.data.clone(), "")?;
                    ctx.cache.insert(path.to_string(), built.clone());
                    built
                }
            };
            Some(SlideBackground::Picture(img))
        }
        _ => None,
    })
}

/// Footer field (`dt`, `ftr`, `sldNum` placeholder) of a slide.
pub(crate) fn apply_footer_ph(
    footer: &mut SlideFooter,
    ph_type: &str,
    text: &str,
    field: Option<&str>,
) {
    match ph_type {
        "sldNum" => footer.number = true,
        "dt" if field.is_some_and(|t| t.starts_with("datetime")) => footer.date = SlideDate::Auto,
        "dt" if !text.is_empty() => {
            footer.date = SlideDate::Fixed(crate::rich_slide_ext::clamp_footer_text(text))
        }
        "ftr" if !text.is_empty() => {
            footer.text = Some(crate::rich_slide_ext::clamp_footer_text(text))
        }
        _ => {}
    }
}

fn effect_of(preset: u32, class: AnimClass) -> Option<AnimEffect> {
    match (class, preset) {
        (AnimClass::Emphasis, 8) => Some(AnimEffect::Spin),
        (AnimClass::Emphasis, 6) => Some(AnimEffect::GrowShrink),
        (AnimClass::Emphasis, _) => None,
        (_, 1) => Some(AnimEffect::Appear),
        (_, 10) => Some(AnimEffect::Fade),
        (_, 2) => Some(AnimEffect::Fly),
        (_, 22) => Some(AnimEffect::Wipe),
        (_, 53) => Some(AnimEffect::Zoom),
        _ => None,
    }
}

fn dir_of(subtype: u32) -> AnimDir {
    match subtype {
        1 => AnimDir::Top,
        2 => AnimDir::Right,
        8 => AnimDir::Left,
        _ => AnimDir::Bottom,
    }
}

/// Effects of `p:timing` the editor can play (appear, fade, fly, wipe, zoom, spin, grow/shrink); other
/// presets are left out. More than [`MAX_SLIDE_ANIMS`] effects refuse the slide.
pub(crate) fn parse_timing(
    events: &[XmlEvent<'_>],
    out: &mut Vec<RawAnim>,
) -> Result<(), EasyOfficeError> {
    let mut stack: Vec<&str> = Vec::new();
    let mut cur: Option<(usize, RawAnim, bool)> = None;
    for ev in events {
        match ev {
            XmlEvent::Start { name, attrs } => {
                let ln = loc(name);
                if ln == "cTn" {
                    if let Some((_, a, _)) = cur.as_mut() {
                        let dur = attr(attrs, "dur").and_then(|v| v.trim().parse::<u32>().ok());
                        if let Some(d) = dur.filter(|&d| d > 1) {
                            a.duration_ms = a.duration_ms.max(d.min(MAX_ANIM_MS));
                        }
                    } else if let Some(class) = match attr(attrs, "presetClass").as_deref() {
                        Some("entr") => Some(AnimClass::Entrance),
                        Some("emph") => Some(AnimClass::Emphasis),
                        Some("exit") => Some(AnimClass::Exit),
                        _ => None,
                    } {
                        let node = attr(attrs, "nodeType");
                        let start = match node.as_deref() {
                            Some("clickEffect") => Some(AnimStart::OnClick),
                            Some("withEffect") => Some(AnimStart::WithPrevious),
                            Some("afterEffect") => Some(AnimStart::AfterPrevious),
                            _ => None,
                        };
                        let effect =
                            num::<u32>(attrs, "presetID").and_then(|p| effect_of(p, class));
                        if let (Some(start), Some(effect)) = (start, effect) {
                            let direction = dir_of(num::<u32>(attrs, "presetSubtype").unwrap_or(4));
                            let raw = RawAnim {
                                spid: String::new(),
                                class,
                                effect,
                                start,
                                direction,
                                duration_ms: 0,
                                delay_ms: 0,
                            };
                            cur = Some((stack.len(), raw, false));
                        }
                    }
                } else if let Some((depth, a, delayed)) = cur.as_mut() {
                    if ln == "cond"
                        && !*delayed
                        && stack.len() == *depth + 2
                        && stack.last() == Some(&"stCondLst")
                    {
                        a.delay_ms = num::<u32>(attrs, "delay").unwrap_or(0).min(MAX_ANIM_MS);
                        *delayed = true;
                    } else if ln == "spTgt" && a.spid.is_empty() {
                        a.spid = attr(attrs, "spid")
                            .map(|s| s.into_owned())
                            .unwrap_or_default();
                    }
                }
                stack.push(ln);
            }
            XmlEvent::End(_) => {
                stack.pop();
                if cur
                    .as_ref()
                    .is_some_and(|(depth, _, _)| stack.len() == *depth)
                {
                    if let Some((_, mut a, _)) = cur.take() {
                        if a.duration_ms == 0 {
                            a.duration_ms = DEFAULT_ANIM_MS;
                        }
                        if !a.spid.is_empty() {
                            if out.len() >= MAX_SLIDE_ANIMS {
                                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                                    "PPTX 슬라이드 애니메이션 수가 상한({MAX_SLIDE_ANIMS})을 초과했습니다."
                                )));
                            }
                            out.push(a);
                        }
                    }
                }
            }
            XmlEvent::Text(_) => {}
        }
    }
    Ok(())
}

/// Moves the elements from `first` on as one body turned and flipped about `(cx, cy)`.
pub(crate) fn transform_group(
    els: &mut [SlideElement],
    fxs: &mut Vec<(usize, ElementFx)>,
    first: usize,
    center: (f64, f64),
    (rot_deg, flip_h, flip_v): (f64, bool, bool),
) {
    if rot_deg == 0.0 && !flip_h && !flip_v {
        return;
    }
    let (sin, cos) = rot_deg.to_radians().sin_cos();
    for (i, el) in els.iter_mut().enumerate().skip(first) {
        let mut r = el.rect();
        let (mut dx, mut dy) = (r.x + r.w / 2.0 - center.0, r.y + r.h / 2.0 - center.1);
        if flip_h {
            dx = -dx;
        }
        if flip_v {
            dy = -dy;
        }
        let (nx, ny) = (
            center.0 + dx * cos - dy * sin,
            center.1 + dx * sin + dy * cos,
        );
        r.x = nx - r.w / 2.0;
        r.y = ny - r.h / 2.0;
        el.set_rect(r);
        let pos = fxs.iter().position(|(k, _)| *k == i).unwrap_or_else(|| {
            fxs.push((i, ElementFx::default()));
            fxs.len() - 1
        });
        let fx = &mut fxs[pos].1;
        // A single mirror negates the element's own turn; two mirrors make a half turn and keep it.
        let own = if flip_h ^ flip_v {
            -fx.rotation_deg
        } else {
            fx.rotation_deg
        };
        fx.flip_h ^= flip_h;
        fx.flip_v ^= flip_v;
        fx.rotation_deg = (own + rot_deg).rem_euclid(360.0);
    }
}

fn unique_name(base: &str, taken: &mut HashSet<String>) -> String {
    let mut name = base.to_string();
    let mut n = 1usize;
    while !taken.insert(name.clone()) {
        n += 1;
        name = format!("{base}-{n}");
    }
    name
}

/// Turns what the shape tree gathered into the slide's [`SlideExt`]: element ids are made unique, group
/// members and animation targets are resolved to ids, and entries that point at nothing are dropped.
pub(crate) fn finish_ext(
    mut acc: ExtAccum,
    els: &mut [SlideElement],
    hidden: bool,
) -> Result<SlideExt, EasyOfficeError> {
    let mut taken = HashSet::new();
    for el in els.iter_mut() {
        let id = unique_name(el.id(), &mut taken);
        el.set_id(id);
    }
    if acc.groups.len() > MAX_SLIDE_GROUPS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "PPTX 슬라이드 그룹 수가 상한({MAX_SLIDE_GROUPS})을 초과했습니다."
        )));
    }
    let group_ids: Vec<String> = acc
        .groups
        .iter()
        .enumerate()
        .map(|(n, g)| {
            unique_name(
                g.name
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or(&format!("grp-{}", n + 1)),
                &mut taken,
            )
        })
        .collect();
    let id_of = |m: Member| match m {
        Member::El(i) => els[i].id().to_string(),
        Member::Group(g) => group_ids[g].clone(),
    };
    let groups: Vec<SlideGroup> = acc
        .groups
        .iter()
        .zip(&group_ids)
        .map(|(g, id)| SlideGroup {
            id: id.clone(),
            members: g.members.iter().map(|&m| id_of(m)).collect(),
        })
        .collect();
    let mut ext = SlideExt {
        hidden,
        background: acc.background.take(),
        footer: acc.footer,
        groups,
        ..SlideExt::default()
    };
    acc.fx.sort_by_key(|(i, _)| *i);
    for (i, mut fx) in acc.fx {
        fx.id = els[i].id().to_string();
        if !fx.is_default() {
            ext.fx.push(fx);
        }
    }
    for raw in acc.anims {
        let Some(&m) = acc.spids.get(&raw.spid) else {
            continue;
        };
        ext.anims.push(SlideAnim {
            target: id_of(m),
            class: raw.class,
            effect: raw.effect,
            start: raw.start,
            duration_ms: raw.duration_ms,
            delay_ms: raw.delay_ms,
            direction: raw.direction,
        });
    }
    Ok(ext)
}

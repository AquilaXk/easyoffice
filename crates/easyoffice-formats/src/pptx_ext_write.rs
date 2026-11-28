// 참조 구현: gitbrent/PptxGenJS (MIT, Copyright (c) 2015-present Brent Ely, src/gen-xml.ts, src/gen-objects.ts):
// 단위 규약(회전 60000분의 1도, 그림자 흐림·거리 EMU, 각도 60000분의 1도, 투명도 1000분의 1%, 줄 간격 spcPct 1000분의 1%),
// `p:bg`/`a:outerShdw`/`a:buChar`/`a:buAutoNum` 요소 모양. 애니메이션 `p:timing` 골격은 ECMA-376 19.5절과
// 같은 모양으로 새로 썼다(참조 구현에는 애니메이션 쓰기가 없다). THIRD_PARTY_NOTICES.md 참조.

//! PresentationML writers for the slide extensions of [`crate::SlideExt`]: background, rotation and
//! flip, outline dash, outer shadow, paragraph format, header/footer fields and the animation timeline.

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use crate::pptx_master::footer_placeholder_specs;
use crate::rich_image_writers::MediaParts;
use crate::rich_slide::{pt_to_emu, SlideTextAlign};
use crate::rich_slide_ext::{
    AnimClass, AnimDir, AnimEffect, AnimStart, ElementFx, SlideAnim, SlideBackground, SlideBullet,
    SlideDash, SlideDate, SlideExt, SlideParaFmt,
};
use crate::XmlSanitizer;

/// Marker the reader uses to tell the generated date field from a typed date.
pub(crate) const DATE_FIELD_TYPE: &str = "datetime1";
const ANGLE_UNIT: f64 = 60_000.0;
const PARA_MARGIN_EMU: u32 = 342_900;

/// `a:xfrm` attributes for an element's rotation and flips.
pub(crate) fn xfrm_attrs(fx: Option<&ElementFx>) -> String {
    let Some(fx) = fx else {
        return String::new();
    };
    let mut s = String::new();
    if fx.rotation_deg != 0.0 {
        let _ = write!(
            s,
            " rot=\"{}\"",
            (fx.rotation_deg * ANGLE_UNIT).round() as i64
        );
    }
    if fx.flip_h {
        s.push_str(" flipH=\"1\"");
    }
    if fx.flip_v {
        s.push_str(" flipV=\"1\"");
    }
    s
}

/// `a:ln` with an optional width, a color (`None` means no line) and the dash pattern.
pub(crate) fn push_ln(
    out: &mut String,
    width_pt: Option<f64>,
    color: Option<&str>,
    dash: SlideDash,
) {
    out.push_str("<a:ln");
    if let Some(w) = width_pt {
        let _ = write!(out, " w=\"{}\"", pt_to_emu(w));
    }
    out.push('>');
    match color {
        Some(h) => {
            let _ = write!(out, "<a:solidFill><a:srgbClr val=\"{h}\"/></a:solidFill>");
            match dash {
                SlideDash::Solid => {}
                d => {
                    let _ = write!(out, "<a:prstDash val=\"{}\"/>", d.as_str());
                }
            }
        }
        None => out.push_str("<a:noFill/>"),
    }
    out.push_str("</a:ln>");
}

/// `a:effectLst` holding the outer shadow, when the element has one.
pub(crate) fn push_effects(out: &mut String, fx: Option<&ElementFx>) {
    let Some(s) = fx.and_then(|f| f.shadow.as_ref()) else {
        return;
    };
    let _ = write!(
        out,
        "<a:effectLst><a:outerShdw blurRad=\"{}\" dist=\"{}\" dir=\"{}\" algn=\"tl\" rotWithShape=\"0\"><a:srgbClr val=\"{}\"><a:alpha val=\"{}\"/></a:srgbClr></a:outerShdw></a:effectLst>",
        pt_to_emu(s.blur_pt),
        pt_to_emu(s.dist_pt),
        (s.angle_deg * ANGLE_UNIT).round() as i64,
        s.color,
        u32::from(s.alpha_pct) * 1_000
    );
}

fn push_spacing(out: &mut String, tag: &str, pts: Option<f64>) {
    if let Some(p) = pts {
        let _ = write!(
            out,
            "<a:{tag}><a:spcPts val=\"{}\"/></a:{tag}>",
            (p * 100.0).round() as u32
        );
    }
}

/// `a:pPr` of a paragraph: alignment plus the paragraph format overlay.
pub(crate) fn ppr_xml(align: SlideTextAlign, para: Option<&SlideParaFmt>) -> String {
    let Some(p) = para.filter(|p| !p.is_default()) else {
        return format!("<a:pPr algn=\"{}\"/>", align.ooxml_algn());
    };
    let algn = if p.justify {
        "just"
    } else {
        align.ooxml_algn()
    };
    let bulleted = p.bullet != SlideBullet::None;
    let mut s = String::from("<a:pPr");
    if bulleted {
        let _ = write!(
            s,
            " marL=\"{PARA_MARGIN_EMU}\" indent=\"-{PARA_MARGIN_EMU}\""
        );
    }
    let _ = write!(s, " algn=\"{algn}\">");
    if let Some(pct) = p.line_spacing_pct {
        let _ = write!(
            s,
            "<a:lnSpc><a:spcPct val=\"{}\"/></a:lnSpc>",
            u32::from(pct) * 1_000
        );
    }
    push_spacing(&mut s, "spcBef", p.space_before_pt);
    push_spacing(&mut s, "spcAft", p.space_after_pt);
    match p.bullet {
        SlideBullet::None => {}
        SlideBullet::Char(c) => {
            let _ = write!(
                s,
                "<a:buFont typeface=\"Arial\"/><a:buChar char=\"{}\"/>",
                XmlSanitizer::escape(&c.to_string())
            );
        }
        SlideBullet::Number(n) => {
            let _ = write!(
                s,
                "<a:buFont typeface=\"+mj-lt\"/><a:buAutoNum type=\"{}\"/>",
                n.as_str()
            );
        }
    }
    s.push_str("</a:pPr>");
    s
}

/// `p:bg` of a slide; a picture adds an image part and a relationship.
pub(crate) fn push_bg(
    out: &mut String,
    bg: &SlideBackground,
    media: &mut MediaParts,
    rels: &mut Vec<(String, &'static str, String)>,
    next_rel: &mut usize,
) {
    out.push_str("<p:bg><p:bgPr>");
    match bg {
        SlideBackground::Solid(c) => {
            let _ = write!(out, "<a:solidFill><a:srgbClr val=\"{c}\"/></a:solidFill>");
        }
        SlideBackground::Gradient {
            from,
            to,
            angle_deg,
        } => {
            let _ = write!(
                out,
                "<a:gradFill rotWithShape=\"1\"><a:gsLst><a:gs pos=\"0\"><a:srgbClr val=\"{from}\"/></a:gs><a:gs pos=\"100000\"><a:srgbClr val=\"{to}\"/></a:gs></a:gsLst><a:lin ang=\"{}\" scaled=\"0\"/></a:gradFill>",
                u32::from(*angle_deg) * 60_000
            );
        }
        SlideBackground::Picture(img) => {
            let mref = media.add("ppt/media", img);
            let rid = format!("rId{}", *next_rel);
            *next_rel += 1;
            rels.push((
                rid.clone(),
                "image",
                format!(
                    "../{}",
                    mref.path.strip_prefix("ppt/").unwrap_or(&mref.path)
                ),
            ));
            let _ = write!(
                out,
                "<a:blipFill dpi=\"0\" rotWithShape=\"1\"><a:blip r:embed=\"{rid}\"/><a:srcRect/><a:stretch><a:fillRect/></a:stretch></a:blipFill>"
            );
        }
    }
    out.push_str("<a:effectLst/></p:bgPr></p:bg>");
}

fn push_footer_sp(
    out: &mut String,
    sid: usize,
    idx: usize,
    algn: &str,
    body: &str,
    emu_w: u64,
    emu_h: u64,
) {
    let spec = &footer_placeholder_specs(emu_w, emu_h)[idx];
    let e = spec.emu;
    let _ = write!(
        out,
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{sid}\" name=\"{}\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr>{}</p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:pPr algn=\"{algn}\"/>{body}</a:p></p:txBody></p:sp>",
        spec.name, spec.ph_xml, e.x, e.y, e.cx, e.cy
    );
}

const FOOTER_RPR: &str =
    "<a:rPr lang=\"ko-KR\" sz=\"1200\" dirty=\"0\"><a:solidFill><a:srgbClr val=\"8B8B8B\"/></a:solidFill></a:rPr>";

/// Date, footer and slide-number placeholders; returns how many shape ids were used.
pub(crate) fn push_footer(
    out: &mut String,
    ext: &SlideExt,
    first_sid: usize,
    emu_w: u64,
    emu_h: u64,
) -> usize {
    let mut sid = first_sid;
    let f = &ext.footer;
    let run = |t: &str| {
        format!(
            "<a:r>{FOOTER_RPR}<a:t>{}</a:t></a:r>",
            XmlSanitizer::escape(t)
        )
    };
    match &f.date {
        SlideDate::None => {}
        SlideDate::Auto => {
            let body = format!(
                "<a:fld id=\"{{B1F0A1C2-3D4E-4F50-8A6B-7C8D9E0F1A2B}}\" type=\"{DATE_FIELD_TYPE}\">{FOOTER_RPR}<a:t>2026-01-01</a:t></a:fld>"
            );
            push_footer_sp(out, sid, 0, "l", &body, emu_w, emu_h);
            sid += 1;
        }
        SlideDate::Fixed(t) => {
            push_footer_sp(out, sid, 0, "l", &run(t), emu_w, emu_h);
            sid += 1;
        }
    }
    if let Some(t) = &f.text {
        push_footer_sp(out, sid, 1, "ctr", &run(t), emu_w, emu_h);
        sid += 1;
    }
    if f.number {
        let body = format!(
            "<a:fld id=\"{{C2A1B0D3-4E5F-4061-9B7C-8D9E0F1A2B3C}}\" type=\"slidenum\">{FOOTER_RPR}<a:t>‹#›</a:t></a:fld>"
        );
        push_footer_sp(out, sid, 2, "r", &body, emu_w, emu_h);
        sid += 1;
    }
    sid - first_sid
}

struct Ids(u32);

impl Ids {
    fn next(&mut self) -> u32 {
        self.0 += 1;
        self.0
    }
}

fn set_visibility(out: &mut String, ids: &mut Ids, spid: usize, shown: bool, delay_ms: u32) {
    let _ = write!(
        out,
        "<p:set><p:cBhvr><p:cTn id=\"{}\" dur=\"1\" fill=\"hold\"><p:stCondLst><p:cond delay=\"{delay_ms}\"/></p:stCondLst></p:cTn><p:tgtEl><p:spTgt spid=\"{spid}\"/></p:tgtEl><p:attrNameLst><p:attrName>style.visibility</p:attrName></p:attrNameLst></p:cBhvr><p:to><p:strVal val=\"{}\"/></p:to></p:set>",
        ids.next(),
        if shown { "visible" } else { "hidden" }
    );
}

fn anim_effect(
    out: &mut String,
    ids: &mut Ids,
    spid: usize,
    entering: bool,
    filter: &str,
    dur: u32,
) {
    let _ = write!(
        out,
        "<p:animEffect transition=\"{}\" filter=\"{filter}\"><p:cBhvr><p:cTn id=\"{}\" dur=\"{dur}\"/><p:tgtEl><p:spTgt spid=\"{spid}\"/></p:tgtEl></p:cBhvr></p:animEffect>",
        if entering { "in" } else { "out" },
        ids.next()
    );
}

fn anim_num(
    out: &mut String,
    ids: &mut Ids,
    spid: usize,
    attr: &str,
    (from, to): (&str, &str),
    dur: u32,
) {
    let _ = write!(
        out,
        "<p:anim calcmode=\"lin\" valueType=\"num\"><p:cBhvr additive=\"base\"><p:cTn id=\"{}\" dur=\"{dur}\" fill=\"hold\"/><p:tgtEl><p:spTgt spid=\"{spid}\"/></p:tgtEl><p:attrNameLst><p:attrName>{attr}</p:attrName></p:attrNameLst></p:cBhvr><p:tavLst><p:tav tm=\"0\"><p:val><p:strVal val=\"{from}\"/></p:val></p:tav><p:tav tm=\"100000\"><p:val><p:strVal val=\"{to}\"/></p:val></p:tav></p:tavLst></p:anim>",
        ids.next()
    );
}

/// Where a fly-in starts (and a fly-out ends): off the slide edge on the chosen side.
fn fly_offscreen(dir: AnimDir) -> (&'static str, &'static str) {
    match dir {
        AnimDir::Left => ("0-#ppt_w/2", "#ppt_y"),
        AnimDir::Right => ("1+#ppt_w/2", "#ppt_y"),
        AnimDir::Top => ("#ppt_x", "0-#ppt_h/2"),
        AnimDir::Bottom => ("#ppt_x", "1+#ppt_h/2"),
    }
}

fn wipe_filter(dir: AnimDir) -> &'static str {
    match dir {
        AnimDir::Left => "wipe(left)",
        AnimDir::Right => "wipe(right)",
        AnimDir::Top => "wipe(up)",
        AnimDir::Bottom => "wipe(down)",
    }
}

fn dir_subtype(dir: AnimDir) -> u8 {
    match dir {
        AnimDir::Top => 1,
        AnimDir::Right => 2,
        AnimDir::Bottom => 4,
        AnimDir::Left => 8,
    }
}

/// `(presetID, presetSubtype)` the presentation formats assign the effect.
pub(crate) fn preset_of(a: &SlideAnim) -> (u8, u8) {
    match a.effect {
        AnimEffect::Appear => (1, 0),
        AnimEffect::Fade => (10, 0),
        AnimEffect::Fly => (2, dir_subtype(a.direction)),
        AnimEffect::Wipe => (22, dir_subtype(a.direction)),
        AnimEffect::Zoom => (53, 16),
        AnimEffect::Spin => (8, 0),
        AnimEffect::GrowShrink => (6, 0),
    }
}

fn behaviors(out: &mut String, ids: &mut Ids, a: &SlideAnim, spid: usize) {
    let (dur, entering) = (a.duration_ms.max(1), a.class == AnimClass::Entrance);
    let tail_delay = dur.saturating_sub(1);
    if entering {
        set_visibility(out, ids, spid, true, 0);
    }
    match a.effect {
        AnimEffect::Appear => {}
        AnimEffect::Fade => anim_effect(out, ids, spid, entering, "fade", dur),
        AnimEffect::Wipe => anim_effect(out, ids, spid, entering, wipe_filter(a.direction), dur),
        AnimEffect::Fly => {
            let (sx, sy) = fly_offscreen(a.direction);
            let (from_x, to_x, from_y, to_y) = if entering {
                (sx, "#ppt_x", sy, "#ppt_y")
            } else {
                ("#ppt_x", sx, "#ppt_y", sy)
            };
            anim_num(out, ids, spid, "ppt_x", (from_x, to_x), dur);
            anim_num(out, ids, spid, "ppt_y", (from_y, to_y), dur);
        }
        AnimEffect::Zoom => {
            let (w, h) = if entering {
                (("0", "#ppt_w"), ("0", "#ppt_h"))
            } else {
                (("#ppt_w", "0"), ("#ppt_h", "0"))
            };
            anim_num(out, ids, spid, "ppt_w", w, dur);
            anim_num(out, ids, spid, "ppt_h", h, dur);
            anim_effect(out, ids, spid, entering, "fade", dur);
        }
        AnimEffect::Spin => {
            let _ = write!(
                out,
                "<p:animRot by=\"21600000\"><p:cBhvr><p:cTn id=\"{}\" dur=\"{dur}\" fill=\"hold\"/><p:tgtEl><p:spTgt spid=\"{spid}\"/></p:tgtEl><p:attrNameLst><p:attrName>r</p:attrName></p:attrNameLst></p:cBhvr></p:animRot>",
                ids.next()
            );
        }
        AnimEffect::GrowShrink => {
            let _ = write!(
                out,
                "<p:animScale><p:cBhvr><p:cTn id=\"{}\" dur=\"{dur}\" fill=\"hold\"/><p:tgtEl><p:spTgt spid=\"{spid}\"/></p:tgtEl></p:cBhvr><p:by x=\"150000\" y=\"150000\"/></p:animScale>",
                ids.next()
            );
        }
    }
    if a.class == AnimClass::Exit {
        set_visibility(out, ids, spid, false, tail_delay);
    }
}

fn effect_par(out: &mut String, ids: &mut Ids, a: &SlideAnim, spid: usize, text_shape: bool) {
    let (preset, subtype) = preset_of(a);
    let class = match a.class {
        AnimClass::Entrance => "entr",
        AnimClass::Emphasis => "emph",
        AnimClass::Exit => "exit",
    };
    let node = match a.start {
        AnimStart::OnClick => "clickEffect",
        AnimStart::WithPrevious => "withEffect",
        AnimStart::AfterPrevious => "afterEffect",
    };
    let grp = if text_shape { " grpId=\"0\"" } else { "" };
    let _ = write!(
        out,
        "<p:par><p:cTn id=\"{}\" presetID=\"{preset}\" presetClass=\"{class}\" presetSubtype=\"{subtype}\" fill=\"hold\"{grp} nodeType=\"{node}\"><p:stCondLst><p:cond delay=\"{}\"/></p:stCondLst><p:childTnLst>",
        ids.next(),
        a.delay_ms
    );
    behaviors(out, ids, a, spid);
    out.push_str("</p:childTnLst></p:cTn></p:par>");
}

type Step<'a> = (&'a SlideAnim, usize);

/// Click groups, each a list of after-previous groups of effects that start together.
fn click_groups<'a>(steps: &[Step<'a>]) -> Vec<Vec<Vec<Step<'a>>>> {
    let mut clicks: Vec<Vec<Vec<Step<'a>>>> = Vec::new();
    for &step in steps {
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
    clicks
}

/// `p:timing` of a slide. `ids` maps element and group ids to shape ids; `text_shapes` holds the shape
/// ids that go into `p:bldLst` (shapes and text boxes).
pub(crate) fn push_timing(
    out: &mut String,
    anims: &[SlideAnim],
    ids: &HashMap<String, usize>,
    text_shapes: &HashSet<usize>,
) {
    let steps: Vec<Step<'_>> = anims
        .iter()
        .filter_map(|a| ids.get(&a.target).map(|&s| (a, s)))
        .collect();
    if steps.is_empty() {
        return;
    }
    let mut n = Ids(2);
    out.push_str("<p:timing><p:tnLst><p:par><p:cTn id=\"1\" dur=\"indefinite\" restart=\"never\" nodeType=\"tmRoot\"><p:childTnLst><p:seq concurrent=\"1\" nextAc=\"seek\"><p:cTn id=\"2\" dur=\"indefinite\" nodeType=\"mainSeq\"><p:childTnLst>");
    for (ci, click) in click_groups(&steps).iter().enumerate() {
        let auto = ci == 0 && click[0][0].0.start != AnimStart::OnClick;
        let _ = write!(
            out,
            "<p:par><p:cTn id=\"{}\" fill=\"hold\"><p:stCondLst><p:cond delay=\"indefinite\"/>",
            n.next()
        );
        if auto {
            out.push_str("<p:cond evt=\"onBegin\" delay=\"0\"><p:tn val=\"2\"/></p:cond>");
        }
        out.push_str("</p:stCondLst><p:childTnLst>");
        let mut start_ms = 0u32;
        for group in click {
            let _ = write!(out, "<p:par><p:cTn id=\"{}\" fill=\"hold\"><p:stCondLst><p:cond delay=\"{start_ms}\"/></p:stCondLst><p:childTnLst>", n.next());
            for &(a, spid) in group {
                effect_par(out, &mut n, a, spid, text_shapes.contains(&spid));
            }
            out.push_str("</p:childTnLst></p:cTn></p:par>");
            let longest = group
                .iter()
                .map(|(a, _)| a.delay_ms.saturating_add(a.duration_ms))
                .max()
                .unwrap_or(0);
            start_ms = start_ms.saturating_add(longest);
        }
        out.push_str("</p:childTnLst></p:cTn></p:par>");
    }
    out.push_str("</p:childTnLst></p:cTn><p:prevCondLst><p:cond evt=\"onPrev\" delay=\"0\"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:prevCondLst><p:nextCondLst><p:cond evt=\"onNext\" delay=\"0\"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:nextCondLst></p:seq></p:childTnLst></p:cTn></p:par></p:tnLst>");
    let mut bld: Vec<usize> = steps
        .iter()
        .map(|s| s.1)
        .filter(|s| text_shapes.contains(s))
        .collect();
    bld.sort_unstable();
    bld.dedup();
    if !bld.is_empty() {
        out.push_str("<p:bldLst>");
        for spid in bld {
            let _ = write!(out, "<p:bldP spid=\"{spid}\" grpId=\"0\" animBg=\"1\"/>");
        }
        out.push_str("</p:bldLst>");
    }
    out.push_str("</p:timing>");
}

//! Lossless ECMA-376 PresentationML (`.pptx`/`.ppsx`) reader for [`RichSlideDeck`].

use std::collections::{HashMap, HashSet};

use crate::decode_budget::DecodeBudget;
use crate::office_codecs::validate_zip_xml_entries;
use crate::ooxml_presentation::{
    extract_notes_xml_text, parse_slide_chart_xml, parse_slide_size_aspect, read_ppt_rels,
    resolve_ppt, utf8_entry, SLIDESHOW_MAIN_CT,
};
use crate::pptx_deck_settings::read_deck_settings;
use crate::pptx_ext_read::{
    apply_footer_ph, finish_ext, parse_bg, parse_timing, scan_fx, transform_group, ExtAccum,
    Member, PendingGroup,
};
use crate::pptx_shape_read::{parse_cxn_sp, parse_sp, ParsedSp};
use crate::pptx_theme_cache::PptxThemeCache;
use crate::pptx_theme_color::{ColorSpecBuilder, PptxClrMap, PptxThemePalette};
use crate::pptx_transition::TransitionRead;
use crate::rich_slide::{
    emu_to_pt, normalize_table_rows, push_checked_slide_element, RichSlide, RichSlideDeck,
    SlideBox, SlideElement,
};
use crate::rich_slide_ext::{charge_background, ElementFx};
use crate::rich_slide_guides::read_guides;
use crate::rich_slide_table::{scan_spans, DRAWINGML};
use crate::rtf_xml_helpers::{extract_attr, extract_attr_from_fragment, extract_balanced_elements};
use crate::xml_stream::{attr, Attrs, XmlEvent, XmlReader};
use crate::{
    EasyOfficeError, OoxmlCodec, RichImage, XmlSanitizer, ZipArchiveCodec, ZipArchiveEntry,
    ZipSecurityPolicy,
};

const MAX_SLIDE_XML_EVENTS: usize = 512_000;
const MAX_GROUP_DEPTH: usize = 16;

pub(crate) use crate::xml_stream::local_name as loc;

fn parse_pt_pair(
    attrs: &Attrs<'_>,
    kx: &str,
    ky: &str,
) -> Result<Option<(f64, f64)>, EasyOfficeError> {
    let (Some(xs), Some(ys)) = (attr(attrs, kx), attr(attrs, ky)) else {
        return Ok(None);
    };
    let (Ok(x), Ok(y)) = (xs.trim().parse::<f64>(), ys.trim().parse::<f64>()) else {
        return Err(EasyOfficeError::InvalidArgument(format!(
            "PPTX 개체 좌표/크기({kx}/{ky})가 숫자가 아닙니다."
        )));
    };
    if !x.is_finite() || !y.is_finite() {
        return Err(EasyOfficeError::InvalidArgument(format!(
            "PPTX 개체 좌표/크기({kx}/{ky})에 유한하지 않은 값이 포함되어 있습니다."
        )));
    }
    Ok(Some((emu_to_pt(x), emu_to_pt(y))))
}

pub(crate) fn parse_cnvpr(attrs: &Attrs<'_>, id: &mut String, descr: Option<&mut String>) {
    if let Some(n) = attr(attrs, "name").filter(|s| !s.trim().is_empty()) {
        *id = n.into_owned();
    }
    if let Some(out_d) = descr.filter(|d| d.is_empty()) {
        if let Some(d) = attr(attrs, "descr") {
            *out_d = d.into_owned();
        }
    }
}

#[derive(Clone, Copy, Default)]
pub(crate) struct GroupFrame {
    gx: f64,
    gy: f64,
    gw: f64,
    gh: f64,
    ch_x: f64,
    ch_y: f64,
    ch_w: f64,
    ch_h: f64,
    rot_deg: f64,
    flip_h: bool,
    flip_v: bool,
}

impl GroupFrame {
    fn apply(self, c: SlideBox) -> SlideBox {
        let sx = if self.ch_w > 0.0 {
            self.gw / self.ch_w
        } else {
            1.0
        };
        let sy = if self.ch_h > 0.0 {
            self.gh / self.ch_h
        } else {
            1.0
        };
        SlideBox {
            x: if self.ch_w > 0.0 {
                self.gx + (c.x - self.ch_x) * sx
            } else {
                c.x - self.ch_x + self.gx
            },
            y: if self.ch_h > 0.0 {
                self.gy + (c.y - self.ch_y) * sy
            } else {
                c.y - self.ch_y + self.gy
            },
            w: c.w * sx,
            h: c.h * sy,
        }
    }
}

pub(crate) struct SlideCtx<'a> {
    pub(crate) sx: f64,
    pub(crate) sy: f64,
    pub(crate) rels: &'a HashMap<String, String>,
    pub(crate) map: &'a HashMap<&'a str, &'a ZipArchiveEntry>,
    pub(crate) cache: &'a mut HashMap<String, RichImage>,
    pub(crate) budget: &'a mut DecodeBudget,
    pub(crate) palette: &'a PptxThemePalette,
    pub(crate) clr_map: &'a PptxClrMap,
}

#[derive(Default)]
struct SlideAccum {
    t_parts: Vec<String>,
    bullets: Vec<String>,
    loose: Vec<String>,
    els: Vec<SlideElement>,
    total_el: usize,
    total_img_b: usize,
    x: ExtAccum,
}

pub(crate) fn resolve_rel_entry<'a>(
    rid: &str,
    kind: &str,
    ctx: &SlideCtx<'a>,
) -> Result<(&'a str, &'a ZipArchiveEntry), EasyOfficeError> {
    let path = ctx.rels.get(rid).ok_or_else(|| {
        EasyOfficeError::CorruptedContainer(format!("{kind} 관계 ID '{rid}'를 찾을 수 없습니다."))
    })?;
    let entry = *ctx.map.get(path.as_str()).ok_or_else(|| {
        EasyOfficeError::CorruptedContainer(format!("{kind} 파트 '{path}'가 누락되었습니다."))
    })?;
    Ok((entry.path.as_str(), entry))
}

pub(crate) fn parse_xfrm(
    events: &[XmlEvent<'_>],
    ignore_tc: bool,
    sx: f64,
    sy: f64,
    groups: &[GroupFrame],
) -> Result<Option<SlideBox>, EasyOfficeError> {
    let (mut off, mut ext, mut in_xfrm, mut in_tc) = (None, None, false, false);
    for ev in events {
        match ev {
            XmlEvent::Start { name, attrs } => match loc(name) {
                "tc" if ignore_tc => in_tc = true,
                "xfrm" if !in_tc => in_xfrm = true,
                "off" if in_xfrm && off.is_none() => off = parse_pt_pair(attrs, "x", "y")?,
                "ext" if in_xfrm && ext.is_none() => ext = parse_pt_pair(attrs, "cx", "cy")?,
                _ => {}
            },
            XmlEvent::End(name) => match loc(name) {
                "tc" if ignore_tc => in_tc = false,
                "xfrm" if in_xfrm => in_xfrm = false,
                _ => {}
            },
            _ => {}
        }
    }
    let Some(((x, y), (w, h))) = off.zip(ext) else {
        return Ok(None);
    };
    if w < 0.0 || h < 0.0 {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "슬라이드 개체 크기({w:.1}x{h:.1})는 음수일 수 없습니다."
        )));
    }
    let mut box_pt = SlideBox { x, y, w, h };
    for frame in groups.iter().rev() {
        box_pt = frame.apply(box_pt);
    }
    box_pt.validate()?;
    let scaled = box_pt.scaled(sx, sy);
    scaled.validate()?;
    Ok(Some(scaled))
}

fn parse_grp_frame(events: &[XmlEvent<'_>]) -> Result<GroupFrame, EasyOfficeError> {
    let (mut off, mut ext, mut ch_off, mut ch_ext, mut in_xfrm) = (None, None, None, None, false);
    let (mut rot_deg, mut flip_h, mut flip_v) = (0.0, false, false);
    for ev in events {
        match ev {
            XmlEvent::Start { name, attrs } => match loc(name) {
                "xfrm" => {
                    in_xfrm = true;
                    let turn = attr(attrs, "rot").and_then(|v| v.trim().parse::<f64>().ok());
                    rot_deg = turn
                        .filter(|v| v.is_finite())
                        .map_or(0.0, |v| (v / 60_000.0).rem_euclid(360.0));
                    flip_h = attr(attrs, "flipH").is_some_and(|v| v == "1" || v == "true");
                    flip_v = attr(attrs, "flipV").is_some_and(|v| v == "1" || v == "true");
                }
                "off" if in_xfrm && off.is_none() => off = parse_pt_pair(attrs, "x", "y")?,
                "ext" if in_xfrm && ext.is_none() => ext = parse_pt_pair(attrs, "cx", "cy")?,
                "chOff" if in_xfrm && ch_off.is_none() => ch_off = parse_pt_pair(attrs, "x", "y")?,
                "chExt" if in_xfrm && ch_ext.is_none() => {
                    ch_ext = parse_pt_pair(attrs, "cx", "cy")?
                }
                _ => {}
            },
            XmlEvent::End(name) if loc(name) == "xfrm" => in_xfrm = false,
            _ => {}
        }
    }
    let (gx, gy) = off.unwrap_or((0.0, 0.0));
    let (gw, gh) = ext.unwrap_or((0.0, 0.0));
    let (ch_x, ch_y) = ch_off.unwrap_or((0.0, 0.0));
    let (ch_w, ch_h) = ch_ext.unwrap_or((0.0, 0.0));
    if gw < 0.0 || gh < 0.0 || ch_w < 0.0 || ch_h < 0.0 {
        return Err(EasyOfficeError::InvalidArgument(
            "PPTX 그룹 크기(ext/chExt)는 음수일 수 없습니다.".to_string(),
        ));
    }
    Ok(GroupFrame {
        gx,
        gy,
        gw,
        gh,
        ch_x,
        ch_y,
        ch_w,
        ch_h,
        rot_deg,
        flip_h,
        flip_v,
    })
}

fn parse_pic(
    events: &[XmlEvent<'_>],
    fallback_id: &str,
    ctx: &mut SlideCtx<'_>,
    groups: &[GroupFrame],
) -> Result<Option<SlideElement>, EasyOfficeError> {
    let (mut id, mut alt, mut rid) = (fallback_id.to_string(), String::new(), None);
    for ev in events {
        if let XmlEvent::Start { name, attrs } = ev {
            match loc(name) {
                "cNvPr" => parse_cnvpr(attrs, &mut id, Some(&mut alt)),
                "blip" if rid.is_none() => {
                    rid = attr(attrs, "r:embed")
                        .or_else(|| attr(attrs, "embed"))
                        .map(|s| s.into_owned())
                }
                _ => {}
            }
        }
    }
    let (Some(rect), Some(rid)) = (parse_xfrm(events, false, ctx.sx, ctx.sy, groups)?, rid) else {
        return Ok(None);
    };
    let (path, entry) = resolve_rel_entry(&rid, "그림", ctx)?;
    let base = if let Some(c) = ctx.cache.get(path) {
        c.clone()
    } else {
        let built = RichImage::new(entry.data.clone(), "")?;
        ctx.cache.insert(path.to_string(), built.clone());
        built
    };
    Ok(Some(SlideElement::Image {
        id,
        rect,
        image: base.with_alt(&alt),
        alt,
    }))
}

fn parse_graphic_frame(
    events: &[XmlEvent<'_>],
    fallback_id: &str,
    ctx: &mut SlideCtx<'_>,
    groups: &[GroupFrame],
    loose: &mut Vec<String>,
) -> Result<Option<SlideElement>, EasyOfficeError> {
    let mut id = fallback_id.to_string();
    let (mut chart_rid, mut is_table, mut header_row, mut fill) = (None, false, false, None);
    let (mut rows, mut cur_row, mut cell_paras, mut cur_p) =
        (Vec::new(), Vec::new(), Vec::new(), String::new());
    let (mut d, mut tc_pr_d, mut solid_fill_d, mut color_d) = (0usize, None, None, None);
    let (mut in_tc, mut in_p, mut in_t) = (false, false, false);
    let mut pending_color = None::<ColorSpecBuilder>;

    for ev in events {
        match ev {
            XmlEvent::Start { name, attrs } => {
                d += 1;
                let ln = loc(name);
                if color_d.is_some_and(|cd| d == cd + 1) {
                    if let Some(b) = pending_color.as_mut() {
                        b.push_transform(ln, attrs);
                    }
                    continue;
                }
                match ln {
                    "cNvPr" => parse_cnvpr(attrs, &mut id, None),
                    "chart" if chart_rid.is_none() => {
                        chart_rid = attr(attrs, "r:id")
                            .or_else(|| attr(attrs, "id"))
                            .map(|s| s.into_owned())
                    }
                    "tbl" => is_table = true,
                    "tblPr" if is_table => {
                        header_row =
                            attr(attrs, "firstRow").is_some_and(|v| v == "1" || v == "true")
                    }
                    "tr" if is_table => cur_row.clear(),
                    "tc" if is_table => {
                        in_tc = true;
                        cell_paras.clear();
                    }
                    "tcPr" if in_tc && tc_pr_d.is_none() => tc_pr_d = Some(d),
                    "solidFill" if tc_pr_d.is_some_and(|td| d == td + 1) && fill.is_none() => {
                        solid_fill_d = Some(d);
                    }
                    "srgbClr" | "schemeClr" | "sysClr" | "prstClr"
                        if color_d.is_none()
                            && fill.is_none()
                            && solid_fill_d.is_some_and(|fd| d == fd + 1) =>
                    {
                        if let Some(b) = ColorSpecBuilder::from_start(ln, attrs) {
                            color_d = Some(d);
                            pending_color = Some(b);
                        }
                    }
                    "p" if in_tc => {
                        in_p = true;
                        cur_p.clear();
                    }
                    "t" if in_p => in_t = true,
                    _ => {}
                }
            }
            XmlEvent::Text(t) if in_t => cur_p.push_str(t),
            XmlEvent::End(name) => {
                let ln = loc(name);
                if color_d == Some(d) {
                    color_d = None;
                    if fill.is_none() {
                        if let Some(spec) = pending_color.take().and_then(ColorSpecBuilder::finish)
                        {
                            fill = spec.resolve_hex(ctx.palette, ctx.clr_map, None);
                        }
                    }
                }
                if solid_fill_d == Some(d) {
                    solid_fill_d = None;
                }
                if tc_pr_d == Some(d) {
                    tc_pr_d = None;
                }
                match ln {
                    "t" if in_t => in_t = false,
                    "p" if in_tc && in_p => {
                        in_p = false;
                        cell_paras.push(std::mem::take(&mut cur_p));
                    }
                    "tc" if is_table => {
                        in_tc = false;
                        cur_row.push(cell_paras.join("\n"));
                    }
                    "tr" if is_table => rows.push(std::mem::take(&mut cur_row)),
                    _ => {}
                }
                d = d.saturating_sub(1);
            }
            _ => {}
        }
    }
    let Some(rect) = parse_xfrm(events, true, ctx.sx, ctx.sy, groups)? else {
        if is_table {
            for r in rows {
                let cleaned: Vec<String> = r.into_iter().map(|c| c.replace('\n', " ")).collect();
                if cleaned.iter().any(|c| !c.is_empty()) {
                    loose.push(cleaned.join("\t"));
                }
            }
        }
        return Ok(None);
    };
    if is_table && !rows.is_empty() {
        return Ok(Some(SlideElement::Table {
            id,
            rect,
            rows: normalize_table_rows(rows)?,
            header_row,
            fill,
        }));
    }
    if let Some(rid) = chart_rid {
        let (path, entry) = resolve_rel_entry(&rid, "차트", ctx)?;
        let chart_xml = utf8_entry(entry, path)?;
        ctx.budget.charge(chart_xml.len(), "PPTX 차트 XML")?;
        let (chart_type, title, categories, series, legend) = parse_slide_chart_xml(chart_xml)?;
        return Ok(Some(SlideElement::Chart {
            id,
            rect,
            chart_type,
            title,
            categories,
            series,
            legend,
        }));
    }
    Ok(None)
}

fn dispatch_shape_subtree(
    kind: &str,
    sub: &[XmlEvent<'_>],
    ctx: &mut SlideCtx<'_>,
    groups: &[GroupFrame],
    acc: &mut SlideAccum,
) -> Result<(), EasyOfficeError> {
    let fid = format!("el-{}", acc.els.len() + 1);
    let maybe = match kind {
        "sp" => match parse_sp(sub, &fid, ctx, groups)? {
            ParsedSp::Ignore => None,
            ParsedSp::Placeholder {
                is_title,
                paragraphs,
            } => {
                let ne = paragraphs.into_iter().filter(|s| !s.is_empty());
                if is_title {
                    acc.t_parts.extend(ne);
                } else {
                    acc.bullets.extend(ne);
                }
                None
            }
            ParsedSp::LooseText(p) => {
                acc.loose.extend(p.into_iter().filter(|s| !s.is_empty()));
                None
            }
            ParsedSp::Footer {
                ph_type,
                text,
                field,
            } => {
                apply_footer_ph(&mut acc.x.footer, &ph_type, &text, field.as_deref());
                None
            }
            ParsedSp::Element(el) => Some(el),
        },
        "cxnSp" => parse_cxn_sp(sub, &fid, ctx, groups)?,
        "pic" => parse_pic(sub, &fid, ctx, groups)?,
        "graphicFrame" => parse_graphic_frame(sub, &fid, ctx, groups, &mut acc.loose)?,
        _ => None,
    };
    if let Some(el) = maybe {
        let kind = el_fx_kind(&el);
        push_checked_slide_element(el, &mut acc.els, &mut acc.total_el, &mut acc.total_img_b)?;
        let idx = acc.els.len() - 1;
        let scan = scan_fx(sub, ctx.palette, ctx.clr_map);
        let mut fx = scan.fx;
        match kind {
            FxKind::None => {
                fx = ElementFx::default();
                if let SlideElement::Table { rows, .. } = &acc.els[idx] {
                    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
                    fx.spans = scan_spans(sub, &DRAWINGML, rows.len(), cols);
                }
            }
            FxKind::Shape => {
                fx.outline = None;
                fx.crop = None;
            }
            FxKind::Overlay => {
                fx.outline = scan.line;
                fx.crop = None;
            }
            FxKind::Picture => {
                fx.outline = scan.line;
                fx.para = None;
            }
        }
        if !fx.is_default() {
            acc.x.fx.push((idx, fx));
        }
        if let Some(spid) = scan.spid {
            acc.x.spids.insert(spid, Member::El(idx));
        }
        acc.x.members.push(Member::El(idx));
    }
    Ok(())
}

/// Which effects an element kind keeps: its stroke is on the element for shapes, in the overlay for text
/// boxes and pictures; tables and charts take none.
enum FxKind {
    None,
    Shape,
    Overlay,
    Picture,
}

fn el_fx_kind(el: &SlideElement) -> FxKind {
    match el {
        SlideElement::Shape { .. } => FxKind::Shape,
        SlideElement::TextBox { .. } => FxKind::Overlay,
        SlideElement::Image { .. } => FxKind::Picture,
        SlideElement::Table { .. } | SlideElement::Chart { .. } => FxKind::None,
    }
}

fn walk_grp_sp(
    events: &[XmlEvent<'_>],
    depth: usize,
    groups: &mut Vec<GroupFrame>,
    ctx: &mut SlideCtx<'_>,
    acc: &mut SlideAccum,
) -> Result<(), EasyOfficeError> {
    if depth > MAX_GROUP_DEPTH {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "PPTX 그룹 중첩 깊이({depth})가 상한({MAX_GROUP_DEPTH})을 초과했습니다."
        )));
    }
    if events.len() < 2 {
        return Ok(());
    }
    let inner = &events[1..events.len() - 1];
    let mut children: Vec<(&str, usize, usize)> = Vec::new();
    let (mut rel_d, mut c_start, mut c_name) = (0usize, 0usize, "");
    let mut frame = None;

    for (idx, ev) in inner.iter().enumerate() {
        match ev {
            XmlEvent::Start { name, .. } => {
                if rel_d == 0 {
                    c_start = idx;
                    c_name = loc(name);
                }
                rel_d += 1;
            }
            XmlEvent::End(_) => {
                rel_d = rel_d.saturating_sub(1);
                if rel_d == 0 {
                    if c_name == "grpSpPr" && frame.is_none() {
                        frame = Some(parse_grp_frame(&inner[c_start..=idx])?);
                    } else if matches!(c_name, "sp" | "pic" | "graphicFrame" | "cxnSp" | "grpSp") {
                        children.push((c_name, c_start, idx));
                    }
                }
            }
            XmlEvent::Text(_) => {}
        }
    }

    let frame = frame.unwrap_or_default();
    let (first_el, mark) = (acc.els.len(), acc.x.members.len());
    groups.push(frame);
    let res = (|| -> Result<(), EasyOfficeError> {
        for (kind, s, e) in children {
            let sub = &inner[s..=e];
            if kind == "grpSp" {
                walk_grp_sp(sub, depth + 1, groups, ctx, acc)?;
            } else {
                dispatch_shape_subtree(kind, sub, ctx, groups, acc)?;
            }
        }
        Ok(())
    })();
    groups.pop();
    res?;
    let mut bounds = SlideBox {
        x: frame.gx,
        y: frame.gy,
        w: frame.gw,
        h: frame.gh,
    };
    for outer in groups.iter().rev() {
        bounds = outer.apply(bounds);
    }
    let bounds = bounds.scaled(ctx.sx, ctx.sy);
    transform_group(
        &mut acc.els,
        &mut acc.x.fx,
        first_el,
        (bounds.x + bounds.w / 2.0, bounds.y + bounds.h / 2.0),
        (frame.rot_deg, frame.flip_h, frame.flip_v),
    );
    let members: Vec<Member> = acc.x.members.drain(mark..).collect();
    if members.len() < 2 {
        acc.x.members.extend(members);
        return Ok(());
    }
    let (mut name, mut spid) = (None, None);
    if let Some(XmlEvent::Start { attrs, .. }) = events
        .iter()
        .find(|e| matches!(e, XmlEvent::Start { name, .. } if loc(name) == "cNvPr"))
    {
        name = attr(attrs, "name").map(|s| s.into_owned());
        spid = attr(attrs, "id").map(|s| s.into_owned());
    }
    acc.x.groups.push(PendingGroup { name, members });
    let member = Member::Group(acc.x.groups.len() - 1);
    if let Some(spid) = spid {
        acc.x.spids.insert(spid, member);
    }
    acc.x.members.push(member);
    Ok(())
}

/// Unpacks a `.pptx` or `.ppsx` OOXML package into a [`RichSlideDeck`].
pub fn unpack_pptx_rich(bytes: &[u8]) -> Result<RichSlideDeck, EasyOfficeError> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())?;
    validate_zip_xml_entries(&entries)?;
    let entry_map: HashMap<&str, &ZipArchiveEntry> =
        entries.iter().map(|e| (e.path.as_str(), e)).collect();
    let &pres_entry = entry_map.get("ppt/presentation.xml").ok_or_else(|| {
        EasyOfficeError::CorruptedContainer(
            "PPTX 필수 엔트리 'ppt/presentation.xml'이 누락되었습니다.".to_string(),
        )
    })?;
    let pres_xml = utf8_entry(pres_entry, "ppt/presentation.xml")?;
    let title = OoxmlCodec::pptx_title(&entry_map, pres_xml);
    let slideshow_ct = entry_map
        .get("[Content_Types].xml")
        .and_then(|e| std::str::from_utf8(&e.data).ok())
        .is_some_and(|ct| ct.contains(SLIDESHOW_MAIN_CT));
    let is_slideshow =
        slideshow_ct || extract_attr(pres_xml, "<p:presentation", "slideshow") == Some("1");
    let (aspect, orientation, sx, sy) = parse_slide_size_aspect(pres_xml)?;
    let mut budget = DecodeBudget::new();
    budget.charge(pres_xml.len(), "PPTX 프레젠테이션")?;

    let pres_rels = match entry_map.get("ppt/_rels/presentation.xml.rels") {
        Some(&e) => read_ppt_rels(utf8_entry(e, "ppt/_rels/presentation.xml.rels")?, "ppt")?,
        None => HashMap::new(),
    };
    let ordered_slides =
        OoxmlCodec::ordered_pptx_slides(pres_xml, &entries, &entry_map, &pres_rels)?;

    let mut theme_cache = PptxThemeCache::new(&entry_map, &mut budget)?;
    let mut media_cache = HashMap::new();
    let (mut total_el, mut total_img_b) = (0usize, 0usize);
    let mut seen_bg = HashSet::new();
    let mut slides = Vec::with_capacity(ordered_slides.len());

    for s_entry in ordered_slides {
        let s_xml = utf8_entry(s_entry, &s_entry.path)?;
        budget.charge(s_xml.len(), "PPTX 슬라이드 XML")?;
        let (dir, file) = s_entry.path.rsplit_once('/').unwrap_or(("", &s_entry.path));
        let rels_path = format!("{dir}/_rels/{file}.rels");
        let (mut slide_rels, mut layout_path, mut notes) = (HashMap::new(), None, String::new());
        if let Some(&re) = entry_map.get(rels_path.as_str()) {
            let r_xml = utf8_entry(re, &rels_path)?;
            for (attrs, _) in extract_balanced_elements(r_xml, "Relationship") {
                let (Some(id), Some(target)) = (
                    extract_attr_from_fragment(attrs, "Id"),
                    extract_attr_from_fragment(attrs, "Target"),
                ) else {
                    continue;
                };
                if extract_attr_from_fragment(attrs, "TargetMode") == Some("External") {
                    continue;
                }
                let rt = extract_attr_from_fragment(attrs, "Type").unwrap_or("");
                let tp = resolve_ppt(if target.starts_with("ppt/") { "" } else { dir }, target)?;
                if rt.ends_with("/slideLayout") && layout_path.is_none() {
                    layout_path = Some(tp.clone());
                } else if rt.ends_with("/notesSlide") && notes.is_empty() {
                    if let Some(&ne) = entry_map.get(tp.as_str()) {
                        let n_xml = utf8_entry(ne, &tp)?;
                        budget.charge(n_xml.len(), "PPTX 노트 XML")?;
                        if let Some(n) = extract_notes_xml_text(n_xml) {
                            notes = n;
                        }
                    }
                }
                slide_rels.insert(id.to_string(), tp);
            }
        }
        if notes.is_empty() {
            if let Some(an) = extract_attr(s_xml, "<p:sld", "notes") {
                notes = XmlSanitizer::unescape(an);
            }
        }
        budget.charge(notes.len(), "PPTX 슬라이드 노트")?;
        let (layout, palette, clr_map) =
            theme_cache.resolve_for_slide(layout_path.as_deref(), s_xml, &mut budget)?;

        let mut reader = XmlReader::with_budget(s_xml, MAX_SLIDE_XML_EVENTS);
        let (mut tr, mut in_tr, mut sp_d, mut d) =
            (TransitionRead::default(), false, None::<usize>, 0usize);
        let (mut in_bare_t, mut bare_t) = (false, String::new());
        let mut acc = SlideAccum {
            total_el,
            total_img_b,
            ..SlideAccum::default()
        };
        let mut groups = Vec::new();
        let mut ctx = SlideCtx {
            sx,
            sy,
            rels: &slide_rels,
            map: &entry_map,
            cache: &mut media_cache,
            budget: &mut budget,
            palette: &palette,
            clr_map: &clr_map,
        };

        while let Some(ev) = reader.next_event()? {
            match &ev {
                XmlEvent::Start { name, attrs } => {
                    d += 1;
                    let ln = loc(name);
                    if ln == "transition" {
                        in_tr = true;
                        tr.on_start(attrs);
                        continue;
                    }
                    if in_tr {
                        tr.on_child(ln, attrs);
                        continue;
                    }
                    if ln == "spTree" && sp_d.is_none() {
                        sp_d = Some(d);
                        continue;
                    }
                    if sp_d.is_none() && matches!(ln, "bg" | "timing") {
                        let is_bg = ln == "bg";
                        let sub = reader.collect_subtree(ev)?;
                        d -= 1;
                        if is_bg {
                            acc.x.background = parse_bg(&sub, &mut ctx)?;
                        } else {
                            parse_timing(&sub, &mut acc.x.anims)?;
                        }
                        continue;
                    }
                    if sp_d.is_none() && ln == "t" {
                        in_bare_t = true;
                        bare_t.clear();
                    }
                    if sp_d.is_some_and(|sd| d == sd + 1)
                        && matches!(ln, "sp" | "pic" | "graphicFrame" | "cxnSp" | "grpSp")
                    {
                        let kind = ln;
                        let sub = reader.collect_subtree(ev)?;
                        d -= 1;
                        if kind == "grpSp" {
                            walk_grp_sp(&sub, 1, &mut groups, &mut ctx, &mut acc)?;
                        } else {
                            dispatch_shape_subtree(kind, &sub, &mut ctx, &groups, &mut acc)?;
                        }
                    }
                }
                XmlEvent::Text(t) if in_bare_t => bare_t.push_str(t),
                XmlEvent::End(name) => {
                    let ln = loc(name);
                    if ln == "t" && in_bare_t {
                        in_bare_t = false;
                        if !bare_t.is_empty() {
                            acc.loose.push(std::mem::take(&mut bare_t));
                        }
                    }
                    if ln == "transition" {
                        in_tr = false;
                    }
                    if sp_d == Some(d) && ln == "spTree" {
                        sp_d = None;
                    }
                    d = d.saturating_sub(1);
                }
                _ => {}
            }
        }
        total_el = acc.total_el;
        total_img_b = acc.total_img_b;
        let mut s_title = acc.t_parts.join("\n");
        if s_title.is_empty() {
            if let Some(t) = extract_attr(s_xml, "<p:sld", "title") {
                s_title = XmlSanitizer::unescape(t);
            } else if acc.bullets.is_empty() && !acc.loose.is_empty() {
                s_title = acc.loose.remove(0);
            }
        }
        acc.bullets.extend(acc.loose);
        let hidden = matches!(extract_attr(s_xml, "<p:sld", "show"), Some("0" | "false"));
        let mut ext = finish_ext(std::mem::take(&mut acc.x), &mut acc.els, hidden)?;
        ext.transition_dir = tr.dir;
        ext.advance_after_ms = tr.advance_after_ms;
        ext.no_click_advance = tr.no_click_advance;
        charge_background(&ext, &mut seen_bg, &mut total_img_b)?;
        slides.push(RichSlide {
            title: s_title,
            bullets: acc.bullets,
            notes,
            layout,
            transition: tr.transition,
            elements: acc.els,
            ext,
        });
    }
    let text_of = |path: &str| {
        entry_map
            .get(path)
            .and_then(|e| std::str::from_utf8(&e.data).ok())
    };
    let mut settings = read_deck_settings(
        text_of("ppt/presProps.xml"),
        text_of("ppt/theme/theme1.xml"),
    )?;
    if let Some(view) = text_of("ppt/viewProps.xml") {
        budget.charge(view.len(), "PPTX 보기 속성")?;
        (settings.guides, settings.show_guides) = read_guides(view)?;
    }
    let deck = RichSlideDeck {
        title,
        is_slideshow,
        aspect,
        orientation,
        slides,
        elements_rejected: 0,
        settings,
    };
    deck.validate()?;
    Ok(deck)
}

//! Lossless ODF 1.3 Presentation (`.odp`) reader for [`RichSlideDeck`].

use std::collections::HashMap;

use crate::decode_budget::DecodeBudget;
use crate::odf_package::{
    extract_odp_rich_parts, parse_odp_aspect, parse_odp_chart_xml, parse_odp_layout,
};
use crate::odp_ext_read::{
    apply_footer_frame, parse_timeline, parse_transform, place_from_transform, scan_para,
    OdpExtStyles,
};
use crate::odp_transition::OdpTransition;
use crate::pptx_ext_read::{finish_ext, ExtAccum, Member, PendingGroup};
use crate::rich_document::normalize_hex_color;
use crate::rich_image::safe_package_path;
use crate::rich_slide::{
    normalize_table_rows, parse_length_pt, push_checked_slide_element, RichSlide, RichSlideDeck,
    SlideBox, SlideElement, SlidePlaceholderRole, SlideShapeType, SlideTextAlign,
    DEFAULT_SLIDE_FONT_PT, DEFAULT_SLIDE_STROKE_PT, MAX_SLIDES_PER_DECK, MAX_SLIDE_FONT_PT,
    MAX_SLIDE_STROKE_PT, MAX_SLIDE_TABLE_COLS, MAX_SLIDE_TABLE_ROWS, MIN_SLIDE_FONT_PT,
};
use crate::rich_slide_ext::{charge_background, ElementFx, SlideOutline};
use crate::rtf_xml_helpers::{extract_attr, extract_tag_bodies};
use crate::xml_stream::{attr, Attrs, XmlEvent, XmlReader};
use crate::{EasyOfficeError, RichImage, XmlSanitizer, ZipArchiveEntry};

const MAX_ODP_XML_EVENTS: usize = 512_000;
const MAX_GROUP_DEPTH: usize = 16;

use crate::xml_stream::local_name as loc;

#[derive(Clone, Default)]
struct OdfGraphicStyle {
    fill: Option<String>,
    stroke: Option<String>,
    stroke_width_pt: Option<f64>,
}

#[derive(Clone, Default)]
struct OdfTextStyle {
    font_size_pt: Option<f64>,
    bold: bool,
    italic: bool,
    underline: bool,
    color: Option<String>,
}

fn parse_odp_rect(
    attrs: &Attrs<'_>,
    sx: f64,
    sy: f64,
) -> Result<Option<SlideBox>, EasyOfficeError> {
    let (xs, ys) = (
        attr(attrs, "svg:x").or_else(|| attr(attrs, "x")),
        attr(attrs, "svg:y").or_else(|| attr(attrs, "y")),
    );
    let transform = parse_transform(attrs);
    let (Some(ws), Some(hs)) = (
        attr(attrs, "svg:width").or_else(|| attr(attrs, "width")),
        attr(attrs, "svg:height").or_else(|| attr(attrs, "height")),
    ) else {
        return Ok(None);
    };
    if (xs.is_none() || ys.is_none()) && transform.as_ref().is_none_or(|t| t.translate.is_none()) {
        return Ok(None);
    }
    let corner =
        |s: &Option<std::borrow::Cow<'_, str>>| s.as_deref().map_or(Some(0.0), parse_length_pt);
    let (Some(x), Some(y), Some(w), Some(h)) = (
        corner(&xs),
        corner(&ys),
        parse_length_pt(&ws),
        parse_length_pt(&hs),
    ) else {
        return Err(EasyOfficeError::InvalidArgument(
            "ODP 개체 좌표/크기 단위 형식이 올바르지 않습니다.".to_string(),
        ));
    };
    let mut raw = SlideBox { x, y, w, h };
    if let Some(t) = &transform {
        raw = place_from_transform(raw, xs.is_some() && ys.is_some(), t);
    }
    raw.validate()?;
    let scaled = raw.scaled(sx, sy);
    scaled.validate()?;
    Ok(Some(scaled))
}

#[derive(Default)]
struct OdpStyleMaps {
    transitions: HashMap<String, OdpTransition>,
    graphics: HashMap<String, OdfGraphicStyle>,
    paras: HashMap<String, SlideTextAlign>,
    texts: HashMap<String, OdfTextStyle>,
    x: OdpExtStyles,
}

fn collect_odp_styles(sub: &[XmlEvent<'_>], maps: &mut OdpStyleMaps) {
    let (mut cur_name, mut family) = (String::new(), String::new());
    let (mut tr, mut gs, mut al, mut ts) = (
        OdpTransition::default(),
        OdfGraphicStyle::default(),
        SlideTextAlign::Left,
        OdfTextStyle::default(),
    );
    for ev in sub {
        match ev {
            XmlEvent::Start { name, attrs } => match loc(name) {
                "style" => {
                    cur_name = attr(attrs, "style:name")
                        .or_else(|| attr(attrs, "name"))
                        .map(|s| s.into_owned())
                        .unwrap_or_default();
                    family = attr(attrs, "style:family")
                        .or_else(|| attr(attrs, "family"))
                        .map(|s| s.into_owned())
                        .unwrap_or_default();
                    tr = OdpTransition::default();
                    gs = OdfGraphicStyle::default();
                    al = SlideTextAlign::Left;
                    ts = OdfTextStyle::default();
                }
                "drawing-page-properties" if family == "drawing-page" => {
                    tr = OdpTransition::read(attrs);
                }
                "graphic-properties" if family == "graphic" => {
                    let fill_mode = attr(attrs, "draw:fill").unwrap_or_default();
                    if fill_mode != "none" {
                        gs.fill =
                            attr(attrs, "draw:fill-color").and_then(|v| normalize_hex_color(&v));
                    }
                    let stroke_mode = attr(attrs, "draw:stroke").unwrap_or_default();
                    if stroke_mode != "none" {
                        gs.stroke =
                            attr(attrs, "svg:stroke-color").and_then(|v| normalize_hex_color(&v));
                    }
                    gs.stroke_width_pt = attr(attrs, "svg:stroke-width")
                        .and_then(|s| parse_length_pt(&s))
                        .map(|w| w.clamp(0.0, MAX_SLIDE_STROKE_PT));
                }
                "paragraph-properties" if family == "paragraph" => {
                    if let Some(a) =
                        attr(attrs, "fo:text-align").and_then(|v| SlideTextAlign::parse(&v))
                    {
                        al = a;
                    }
                }
                "text-properties" if family == "text" => {
                    ts.font_size_pt = attr(attrs, "fo:font-size")
                        .and_then(|s| parse_length_pt(&s))
                        .map(|pt| pt.clamp(MIN_SLIDE_FONT_PT, MAX_SLIDE_FONT_PT));
                    ts.bold |=
                        attr(attrs, "fo:font-weight").is_some_and(|v| v == "bold" || v == "700");
                    ts.italic |= attr(attrs, "fo:font-style")
                        .is_some_and(|v| v == "italic" || v == "oblique");
                    ts.underline |=
                        attr(attrs, "style:text-underline-style").is_some_and(|v| v != "none");
                    ts.color = attr(attrs, "fo:color").and_then(|v| normalize_hex_color(&v));
                }
                _ => {}
            },
            XmlEvent::End(name) if loc(name) == "style" && !cur_name.is_empty() => {
                match family.as_str() {
                    "drawing-page" => {
                        maps.transitions.insert(cur_name.clone(), tr);
                    }
                    "graphic" => {
                        maps.graphics.insert(cur_name.clone(), gs.clone());
                    }
                    "paragraph" => {
                        maps.paras.insert(cur_name.clone(), al);
                    }
                    "text" => {
                        maps.texts.insert(cur_name.clone(), ts.clone());
                    }
                    _ => {}
                }
                cur_name.clear();
            }
            _ => {}
        }
    }
}

struct OdpPageCtx<'a> {
    sx: f64,
    sy: f64,
    styles: &'a OdpStyleMaps,
    entry_map: &'a HashMap<&'a str, &'a ZipArchiveEntry>,
    media_cache: &'a mut HashMap<String, RichImage>,
    budget: &'a mut DecodeBudget,
}

fn parse_text_runs(
    events: &[XmlEvent<'_>],
    styles: &OdpStyleMaps,
) -> (Vec<String>, Option<SlideTextAlign>, OdfTextStyle) {
    let (mut paras, mut cur, mut align, mut ts) =
        (Vec::new(), String::new(), None, OdfTextStyle::default());
    let mut in_p = false;
    for ev in events {
        match ev {
            XmlEvent::Start { name, attrs } => match loc(name) {
                "p" | "h" => {
                    in_p = true;
                    cur.clear();
                    if align.is_none() {
                        if let Some(pn) =
                            attr(attrs, "text:style-name").or_else(|| attr(attrs, "style-name"))
                        {
                            align = styles.paras.get(pn.as_ref()).copied();
                        }
                    }
                    if let Some(tn) =
                        attr(attrs, "text:style-name").or_else(|| attr(attrs, "style-name"))
                    {
                        if let Some(s) = styles.texts.get(tn.as_ref()) {
                            ts = s.clone();
                        }
                    }
                }
                "span" if in_p => {
                    if let Some(tn) =
                        attr(attrs, "text:style-name").or_else(|| attr(attrs, "style-name"))
                    {
                        if let Some(s) = styles.texts.get(tn.as_ref()) {
                            ts = s.clone();
                        }
                    }
                }
                "s" if in_p => cur.push(' '),
                "tab" if in_p => cur.push('\t'),
                "line-break" if in_p => cur.push('\n'),
                _ => {}
            },
            XmlEvent::Text(t) if in_p => cur.push_str(t),
            XmlEvent::End(name) if matches!(loc(name), "p" | "h") && in_p => {
                in_p = false;
                paras.push(std::mem::take(&mut cur));
            }
            _ => {}
        }
    }
    (paras, align, ts)
}

enum OdpItem {
    Placeholder {
        is_title: bool,
        paras: Vec<String>,
    },
    Loose(Vec<String>),
    /// Date, footer or page-number frame (`presentation:class`).
    Footer {
        class: String,
        paras: Vec<String>,
    },
    Element(SlideElement),
}

fn parse_odp_frame(
    events: &[XmlEvent<'_>],
    fallback_id: &str,
    ctx: &mut OdpPageCtx<'_>,
) -> Result<Option<OdpItem>, EasyOfficeError> {
    let Some(XmlEvent::Start { attrs, .. }) = events.first() else {
        return Ok(None);
    };
    let id = attr(attrs, "draw:name")
        .or_else(|| attr(attrs, "name"))
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.into_owned())
        .unwrap_or_else(|| fallback_id.to_string());
    let g_style = attr(attrs, "draw:style-name")
        .and_then(|n| ctx.styles.graphics.get(n.as_ref()))
        .cloned()
        .unwrap_or_default();
    let is_layout = attr(attrs, "draw:layer").as_deref() == Some("layout");
    let ph_class = attr(attrs, "presentation:class")
        .or_else(|| attr(attrs, "class"))
        .map(|s| s.into_owned());
    let rect_opt = parse_odp_rect(attrs, ctx.sx, ctx.sy)?;

    let (mut has_tb, mut img_href, mut obj_href, mut has_tbl, mut desc) =
        (false, None, None, false, String::new());
    let (mut in_desc, mut tbl_rows, mut cur_row, mut cur_cell_p, mut cur_p) =
        (false, Vec::new(), Vec::new(), Vec::new(), String::new());
    let (mut header_row, mut in_tc, mut in_p, mut cell_buf_open) = (false, false, false, false);
    for ev in events {
        match ev {
            XmlEvent::Start { name, attrs } => match loc(name) {
                "text-box" if !has_tbl => has_tb = true,
                "image" if img_href.is_none() => {
                    img_href = attr(attrs, "xlink:href")
                        .or_else(|| attr(attrs, "href"))
                        .map(|s| s.into_owned())
                }
                "object" if obj_href.is_none() => {
                    obj_href = attr(attrs, "xlink:href")
                        .or_else(|| attr(attrs, "href"))
                        .map(|s| s.into_owned())
                }
                "desc" => {
                    in_desc = true;
                    desc.clear();
                }
                "table" => {
                    has_tbl = true;
                    header_row =
                        attr(attrs, "table:use-first-row-styles").as_deref() == Some("true");
                }
                "table-row" if has_tbl => {
                    if tbl_rows.len() >= MAX_SLIDE_TABLE_ROWS {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(
                            "ODP 표 행 수 상한 초과".to_string(),
                        ));
                    }
                    cur_row.clear();
                }
                "table-cell" if has_tbl => {
                    if cur_row.len() >= MAX_SLIDE_TABLE_COLS {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(
                            "ODP 표 열 수 상한 초과".to_string(),
                        ));
                    }
                    in_tc = true;
                    cur_cell_p.clear();
                }
                "p" if in_tc => {
                    in_p = true;
                    cur_p.clear();
                    cell_buf_open = true;
                }
                _ => {}
            },
            XmlEvent::Text(t) => {
                if in_desc {
                    desc.push_str(t);
                } else if cell_buf_open && in_p {
                    cur_p.push_str(t);
                }
            }
            XmlEvent::End(name) => match loc(name) {
                "desc" => in_desc = false,
                "p" if in_tc && in_p => {
                    in_p = false;
                    cell_buf_open = false;
                    cur_cell_p.push(std::mem::take(&mut cur_p));
                }
                "table-cell" if has_tbl && in_tc => {
                    in_tc = false;
                    cur_row.push(cur_cell_p.join("\n"));
                }
                "table-row" if has_tbl => tbl_rows.push(std::mem::take(&mut cur_row)),
                _ => {}
            },
        }
    }

    if let Some(href) = img_href {
        let Some(rect) = rect_opt else {
            return Ok(None);
        };
        let path = safe_package_path(&href).ok_or_else(|| {
            EasyOfficeError::SecurityViolation(format!(
                "ODP 이미지 경로가 안전하지 않습니다: {href}"
            ))
        })?;
        let base = if let Some(c) = ctx.media_cache.get(&path) {
            c.clone()
        } else {
            let entry = ctx.entry_map.get(path.as_str()).ok_or_else(|| {
                EasyOfficeError::CorruptedContainer(format!(
                    "ODP 이미지 '{path}'가 누락되었습니다."
                ))
            })?;
            let img = RichImage::new(entry.data.clone(), "")?;
            ctx.media_cache.insert(path, img.clone());
            img
        };
        return Ok(Some(OdpItem::Element(SlideElement::Image {
            id,
            rect,
            image: base.with_alt(&desc),
            alt: desc,
        })));
    }
    if let Some(href) = obj_href {
        let Some(rect) = rect_opt else {
            return Ok(None);
        };
        let base_dir = safe_package_path(&href).ok_or_else(|| {
            EasyOfficeError::SecurityViolation(format!("ODP 차트 경로가 안전하지 않습니다: {href}"))
        })?;
        let c_path = format!("{}/content.xml", base_dir.trim_end_matches('/'));
        let entry = ctx.entry_map.get(c_path.as_str()).ok_or_else(|| {
            EasyOfficeError::CorruptedContainer(format!(
                "ODP 차트 파트 '{c_path}'가 누락되었습니다."
            ))
        })?;
        let chart_xml = std::str::from_utf8(&entry.data)
            .map_err(|_| EasyOfficeError::InvalidUtf8(format!("{c_path} UTF-8 오류")))?;
        ctx.budget.charge(chart_xml.len(), "ODP 차트 XML")?;
        let (chart_type, title, categories, series, legend) = parse_odp_chart_xml(chart_xml)?;
        return Ok(Some(OdpItem::Element(SlideElement::Chart {
            id,
            rect,
            chart_type,
            title,
            categories,
            series,
            legend,
        })));
    }
    if has_tbl && !tbl_rows.is_empty() {
        let Some(rect) = rect_opt else {
            return Ok(None);
        };
        return Ok(Some(OdpItem::Element(SlideElement::Table {
            id,
            rect,
            rows: normalize_table_rows(tbl_rows)?,
            header_row,
            fill: g_style.fill,
        })));
    }
    if has_tb {
        let (paras, align, ts) = parse_text_runs(events, ctx.styles);
        if let Some(class @ ("date-time" | "footer" | "page-number")) = ph_class.as_deref() {
            return Ok(Some(OdpItem::Footer {
                class: class.to_string(),
                paras,
            }));
        }
        if is_layout || rect_opt.is_none() {
            if is_layout {
                let is_title = matches!(ph_class.as_deref(), Some("title" | "ctrTitle"));
                return Ok(Some(OdpItem::Placeholder { is_title, paras }));
            }
            return Ok(Some(OdpItem::Loose(paras)));
        }
        let rect = rect_opt.unwrap();
        let role = ph_class.as_deref().and_then(|c| {
            if c == "outline" {
                Some(SlidePlaceholderRole::Body)
            } else {
                SlidePlaceholderRole::parse(c)
            }
        });
        return Ok(Some(OdpItem::Element(SlideElement::TextBox {
            id,
            rect,
            text: paras.join("\n"),
            font_size_pt: ts.font_size_pt.unwrap_or(DEFAULT_SLIDE_FONT_PT),
            bold: ts.bold,
            italic: ts.italic,
            underline: ts.underline,
            align: align.unwrap_or(SlideTextAlign::Left),
            color: ts.color,
            fill: g_style.fill,
            role,
        })));
    }
    Ok(None)
}

fn parse_odp_shape(
    tag: &str,
    events: &[XmlEvent<'_>],
    fallback_id: &str,
    ctx: &mut OdpPageCtx<'_>,
) -> Result<Option<SlideElement>, EasyOfficeError> {
    let Some(XmlEvent::Start { attrs, .. }) = events.first() else {
        return Ok(None);
    };
    let Some(rect) = parse_odp_rect(attrs, ctx.sx, ctx.sy)? else {
        return Ok(None);
    };
    let id = attr(attrs, "draw:name")
        .or_else(|| attr(attrs, "name"))
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.into_owned())
        .unwrap_or_else(|| fallback_id.to_string());
    let g = attr(attrs, "draw:style-name")
        .and_then(|n| ctx.styles.graphics.get(n.as_ref()))
        .cloned()
        .unwrap_or_default();
    let mut shape_type = if tag == "ellipse" {
        SlideShapeType::Ellipse
    } else {
        SlideShapeType::Rect
    };
    for ev in events {
        if let XmlEvent::Start { name, attrs } = ev {
            if loc(name) == "enhanced-geometry" {
                if let Some(st) = attr(attrs, "draw:type")
                    .or_else(|| attr(attrs, "type"))
                    .and_then(|s| SlideShapeType::parse(&s))
                {
                    shape_type = st;
                }
            }
        }
    }
    let (paras, align, ts) = parse_text_runs(events, ctx.styles);
    Ok(Some(SlideElement::Shape {
        id,
        rect,
        shape_type,
        text: paras.join("\n"),
        fill: g.fill,
        stroke: g.stroke,
        stroke_width_pt: g.stroke_width_pt.unwrap_or(DEFAULT_SLIDE_STROKE_PT),
        color: ts.color,
        font_size_pt: ts.font_size_pt.unwrap_or(16.0),
        bold: ts.bold,
        italic: ts.italic,
        underline: ts.underline,
        align: align.unwrap_or(SlideTextAlign::Center),
    }))
}

/// Elements, text and slide extensions gathered from one page.
#[derive(Default)]
struct PageAcc {
    title_parts: Vec<String>,
    bullets: Vec<String>,
    loose: Vec<String>,
    els: Vec<SlideElement>,
    notes: String,
    total_el: usize,
    total_img_b: usize,
    x: ExtAccum,
}

/// Effects an element keeps, read from its style, transform and geometry attributes (tables and charts
/// keep none), with the id animations point at.
fn element_fx(
    sub: &[XmlEvent<'_>],
    el: &SlideElement,
    ctx: &OdpPageCtx<'_>,
) -> (ElementFx, Option<String>) {
    let Some(XmlEvent::Start { attrs, .. }) = sub.first() else {
        return (ElementFx::default(), None);
    };
    let odf_id = attr(attrs, "draw:id")
        .or_else(|| attr(attrs, "xml:id"))
        .map(|s| s.into_owned());
    let style = attr(attrs, "draw:style-name");
    let g = ctx.styles.x.graphic(style.as_deref());
    let line = style
        .as_deref()
        .and_then(|n| ctx.styles.graphics.get(n))
        .and_then(|s| {
            s.stroke.clone().map(|color| SlideOutline {
                color,
                width_pt: s.stroke_width_pt.unwrap_or(0.75),
            })
        });
    let mut fx = ElementFx {
        rotation_deg: parse_transform(attrs).map_or(0.0, |t| t.rotation_deg),
        dash: ctx.styles.x.dash_of(&g),
        shadow: g.shadow.clone(),
        ..ElementFx::default()
    };
    match el {
        SlideElement::Shape { .. } => {
            let mirrored = |key: &str| {
                sub.iter().any(|e| matches!(e, XmlEvent::Start { name, attrs }
                    if loc(name) == "enhanced-geometry" && attr(attrs, key).is_some_and(|v| v == "true")))
            };
            fx.flip_h = mirrored("draw:mirror-horizontal");
            fx.flip_v = mirrored("draw:mirror-vertical");
            fx.para = scan_para(sub, &ctx.styles.x);
        }
        SlideElement::TextBox { .. } => {
            fx.outline = line;
            fx.para = scan_para(sub, &ctx.styles.x);
        }
        SlideElement::Image { .. } => {
            (fx.flip_h, fx.flip_v) = g.flip;
            fx.outline = line;
        }
        SlideElement::Table { .. } | SlideElement::Chart { .. } => {
            return (ElementFx::default(), odf_id)
        }
    }
    (fx, odf_id)
}

fn walk_children(
    inner: &[XmlEvent<'_>],
    ctx: &mut OdpPageCtx<'_>,
    acc: &mut PageAcc,
    depth: usize,
) -> Result<(), EasyOfficeError> {
    let mut i = 0usize;
    while i < inner.len() {
        let XmlEvent::Start { name, .. } = &inner[i] else {
            i += 1;
            continue;
        };
        let (tag, start) = (loc(name), i);
        let mut open = 1usize;
        i += 1;
        while i < inner.len() && open > 0 {
            match &inner[i] {
                XmlEvent::Start { .. } => open += 1,
                XmlEvent::End(_) => open -= 1,
                XmlEvent::Text(_) => {}
            }
            i += 1;
        }
        let sub = &inner[start..i];
        match tag {
            "notes" => {
                let (np, _, _) = parse_text_runs(sub, ctx.styles);
                let nt = np
                    .into_iter()
                    .filter(|s| !s.trim().is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
                if !nt.is_empty() && acc.notes.is_empty() {
                    acc.notes = nt;
                }
            }
            "par" => parse_timeline(sub, &mut acc.x.anims)?,
            "g" => walk_group(sub, ctx, acc, depth)?,
            _ => place_shape(tag, sub, ctx, acc)?,
        }
    }
    Ok(())
}

fn place_shape(
    tag: &str,
    sub: &[XmlEvent<'_>],
    ctx: &mut OdpPageCtx<'_>,
    acc: &mut PageAcc,
) -> Result<(), EasyOfficeError> {
    let fid = format!("el-{}", acc.els.len() + 1);
    let maybe = match tag {
        "frame" => match parse_odp_frame(sub, &fid, ctx)? {
            Some(OdpItem::Placeholder { is_title, paras }) => {
                let ne = paras.into_iter().filter(|s| !s.is_empty());
                if is_title {
                    acc.title_parts.extend(ne)
                } else {
                    acc.bullets.extend(ne)
                }
                None
            }
            Some(OdpItem::Loose(p)) => {
                acc.loose.extend(p.into_iter().filter(|s| !s.is_empty()));
                None
            }
            Some(OdpItem::Footer { class, paras }) => {
                apply_footer_frame(&mut acc.x, &class, &paras, sub);
                None
            }
            Some(OdpItem::Element(el)) => Some(el),
            None => None,
        },
        "custom-shape" | "rect" | "ellipse" => parse_odp_shape(tag, sub, &fid, ctx)?,
        _ => None,
    };
    if let Some(el) = maybe {
        let (fx, odf_id) = element_fx(sub, &el, ctx);
        push_checked_slide_element(el, &mut acc.els, &mut acc.total_el, &mut acc.total_img_b)?;
        let idx = acc.els.len() - 1;
        if !fx.is_default() {
            acc.x.fx.push((idx, fx));
        }
        if let Some(id) = odf_id {
            acc.x.spids.insert(id, Member::El(idx));
        }
        acc.x.members.push(Member::El(idx));
    }
    Ok(())
}

/// A `draw:g` group: its children are read in place and the group is kept when two or more survive.
fn walk_group(
    sub: &[XmlEvent<'_>],
    ctx: &mut OdpPageCtx<'_>,
    acc: &mut PageAcc,
    depth: usize,
) -> Result<(), EasyOfficeError> {
    if depth >= MAX_GROUP_DEPTH {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "ODP 그룹 중첩 깊이가 상한({MAX_GROUP_DEPTH})을 초과했습니다."
        )));
    }
    let mark = acc.x.members.len();
    if let Some(inner) = sub.get(1..sub.len().saturating_sub(1)) {
        walk_children(inner, ctx, acc, depth + 1)?;
    }
    let members: Vec<Member> = acc.x.members.drain(mark..).collect();
    if members.len() < 2 {
        acc.x.members.extend(members);
        return Ok(());
    }
    let Some(XmlEvent::Start { attrs, .. }) = sub.first() else {
        return Ok(());
    };
    let name = attr(attrs, "draw:name").map(|s| s.into_owned());
    let odf_id = attr(attrs, "draw:id")
        .or_else(|| attr(attrs, "xml:id"))
        .map(|s| s.into_owned());
    acc.x.groups.push(PendingGroup { name, members });
    let member = Member::Group(acc.x.groups.len() - 1);
    if let Some(id) = odf_id {
        acc.x.spids.insert(id, member);
    }
    acc.x.members.push(member);
    Ok(())
}

/// Unpacks an ODF 1.3 Presentation (`.odp`) package into a [`RichSlideDeck`].
pub fn unpack_odp_rich(bytes: &[u8]) -> Result<RichSlideDeck, EasyOfficeError> {
    let (entries, content_xml, meta_xml, styles_xml) = extract_odp_rich_parts(bytes)?;
    let entry_map: HashMap<&str, &ZipArchiveEntry> =
        entries.iter().map(|e| (e.path.as_str(), e)).collect();

    let title = extract_attr(&content_xml, "<office:document-content", "title")
        .map(XmlSanitizer::unescape)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            meta_xml
                .as_deref()
                .and_then(|m| extract_tag_bodies(m, "dc:title").into_iter().next())
                .map(XmlSanitizer::unescape)
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "발표자료".to_string());
    let (aspect, orientation, sx, sy) = parse_odp_aspect(styles_xml.as_deref());

    let mut budget = DecodeBudget::new();
    budget.charge(content_xml.len(), "ODP content.xml")?;

    let mut style_maps = OdpStyleMaps::default();
    if let Some(styles) = styles_xml.as_deref() {
        budget.charge(styles.len(), "ODP styles.xml")?;
        let mut sr = XmlReader::with_budget(styles, MAX_ODP_XML_EVENTS);
        while let Some(ev) = sr.next_event()? {
            if matches!(&ev, XmlEvent::Start { name, .. } if matches!(loc(name), "styles" | "automatic-styles"))
            {
                let sub = sr.collect_subtree(ev)?;
                style_maps.x.collect(&sub);
            }
        }
    }
    let mut reader = XmlReader::with_budget(&content_xml, MAX_ODP_XML_EVENTS);
    let (mut media_cache, mut total_el, mut total_img_b) = (HashMap::new(), 0usize, 0usize);
    let mut seen_bg = std::collections::HashSet::new();
    let mut slides = Vec::new();

    while let Some(ev) = reader.next_event()? {
        if let XmlEvent::Start { name, attrs } = &ev {
            let (tag_loc, attrs_copy) = (loc(name), *attrs);
            match tag_loc {
                "automatic-styles" => {
                    let sub = reader.collect_subtree(ev)?;
                    collect_odp_styles(&sub, &mut style_maps);
                    style_maps.x.collect(&sub);
                }
                "page" => {
                    if slides.len() >= MAX_SLIDES_PER_DECK {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                            "ODP 슬라이드 수 상한({MAX_SLIDES_PER_DECK}) 초과"
                        )));
                    }
                    let page_style = attr(&attrs_copy, "draw:style-name")
                        .map(|s| s.into_owned())
                        .unwrap_or_default();
                    let layout = parse_odp_layout(
                        attr(&attrs_copy, "presentation:presentation-page-layout-name").as_deref(),
                    );
                    let transition = style_maps
                        .transitions
                        .get(&page_style)
                        .copied()
                        .unwrap_or_default();
                    let notes = attr(&attrs_copy, "notes")
                        .map(|s| XmlSanitizer::unescape(&s))
                        .unwrap_or_default();
                    let attr_name = attr(&attrs_copy, "draw:name")
                        .map(|s| s.into_owned())
                        .unwrap_or_default();
                    let page_events = reader.collect_subtree(ev)?;
                    let mut ctx = OdpPageCtx {
                        sx,
                        sy,
                        styles: &style_maps,
                        entry_map: &entry_map,
                        media_cache: &mut media_cache,
                        budget: &mut budget,
                    };
                    let mut acc = PageAcc {
                        notes,
                        total_el,
                        total_img_b,
                        ..PageAcc::default()
                    };
                    if let Some(inner) = page_events.get(1..page_events.len().saturating_sub(1)) {
                        walk_children(inner, &mut ctx, &mut acc, 0)?;
                    }
                    (total_el, total_img_b) = (acc.total_el, acc.total_img_b);
                    ctx.budget.charge(acc.notes.len(), "ODP 발표자 노트")?;
                    let page = style_maps.x.page(&page_style);
                    // A background picture is decoded once however many pages use it; the cache hands out
                    // clones of one shared buffer, and the pages' total counts it once.
                    acc.x.background = style_maps.x.background(&page, |href| {
                        let path = safe_package_path(href).ok_or_else(|| {
                            EasyOfficeError::SecurityViolation(format!(
                                "ODP 배경 그림 경로가 안전하지 않습니다: {href}"
                            ))
                        })?;
                        if let Some(img) = media_cache.get(&path) {
                            return Ok(img.clone());
                        }
                        let entry = entry_map.get(path.as_str()).ok_or_else(|| {
                            EasyOfficeError::CorruptedContainer(format!(
                                "ODP 배경 그림 '{path}'가 누락되었습니다."
                            ))
                        })?;
                        let img = RichImage::new(entry.data.clone(), "")?;
                        media_cache.insert(path, img.clone());
                        Ok(img)
                    })?;
                    let s_title = if !acc.title_parts.is_empty() {
                        acc.title_parts.join("\n")
                    } else if acc.els.is_empty() {
                        attr_name
                    } else {
                        String::new()
                    };
                    acc.bullets.extend(acc.loose);
                    let mut ext = finish_ext(acc.x, &mut acc.els, page.hidden)?;
                    ext.transition_dir = transition.dir;
                    ext.advance_after_ms = transition.advance_after_ms;
                    charge_background(&ext, &mut seen_bg, &mut total_img_b)?;
                    slides.push(RichSlide {
                        title: s_title,
                        bullets: acc.bullets,
                        notes: acc.notes,
                        layout,
                        transition: transition.kind,
                        elements: acc.els,
                        ext,
                    });
                }
                _ => {}
            }
        }
    }
    let deck = RichSlideDeck {
        title,
        is_slideshow: false,
        aspect,
        orientation,
        slides,
        elements_rejected: 0,
        settings: Default::default(),
    };
    deck.validate()?;
    Ok(deck)
}

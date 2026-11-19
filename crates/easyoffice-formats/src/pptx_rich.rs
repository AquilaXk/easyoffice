// 참조 구현: jmcnamara/rust_xlsxwriter (MIT OR Apache-2.0, Copyright 2022-2026 John McNamara, src/chart.rs, src/drawing.rs; see THIRD_PARTY_NOTICES.md)

//! Lossless ECMA-376 PresentationML (`.pptx`/`.ppsx`) reader and writer for [`RichSlideDeck`].

use std::collections::HashMap;
use std::fmt::Write;

use crate::decode_budget::DecodeBudget;
use crate::office_codecs::validate_zip_xml_entries;
use crate::ooxml_presentation::{
    build_slide_chart_xml, extract_notes_xml_text, finish_pptx_package, notes_slide_xml,
    parse_slide_chart_xml, parse_slide_size_aspect, read_ppt_rels, rels_xml, resolve_ppt,
    utf8_entry, PML_NS, SLIDESHOW_MAIN_CT,
};
use crate::ooxml_shared_parts::PML_SP_TREE_HEADER;
use crate::rich_document::normalize_hex_color;
use crate::rich_image_writers::MediaParts;
use crate::rich_slide::{
    emu_to_pt, normalize_table_rows, pt_to_emu, push_checked_slide_element, RichSlide,
    RichSlideDeck, SlideBox, SlideElement, SlideMasterLayout, SlidePlaceholderRole, SlideShapeType,
    SlideTextAlign, SlideTransition, DEFAULT_SLIDE_FONT_PT, DEFAULT_SLIDE_STROKE_PT,
    MAX_SLIDE_FONT_PT, MAX_SLIDE_STROKE_PT, MIN_SLIDE_FONT_PT,
};
use crate::rtf_xml_helpers::{extract_attr, extract_attr_from_fragment, extract_balanced_elements};
use crate::xml_stream::{attr, Attrs, XmlEvent, XmlReader};
use crate::{
    EasyOfficeError, OoxmlCodec, RichImage, XmlSanitizer, ZipArchiveCodec, ZipArchiveEntry,
    ZipSecurityPolicy,
};

const MAX_SLIDE_XML_EVENTS: usize = 512_000;

#[inline]
fn loc(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

fn push_xfrm(out: &mut String, tag: &str, r: SlideBox) {
    let (x, y, cx, cy) = (
        pt_to_emu(r.x),
        pt_to_emu(r.y),
        pt_to_emu(r.w),
        pt_to_emu(r.h),
    );
    let _ = write!(
        out,
        "<{tag}><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></{tag}>"
    );
}

fn push_fill(out: &mut String, fill: Option<&str>) {
    match fill {
        Some(h) => {
            let _ = write!(out, "<a:solidFill><a:srgbClr val=\"{h}\"/></a:solidFill>");
        }
        None => out.push_str("<a:noFill/>"),
    }
}

fn push_paras(
    out: &mut String,
    text: &str,
    pt: f64,
    biu: (bool, bool, bool),
    align: SlideTextAlign,
    color: Option<&str>,
) {
    let sz = (pt * 100.0).round() as u32;
    let (b, i, algn) = (u8::from(biu.0), u8::from(biu.1), align.ooxml_algn());
    let u = if biu.2 { "sng" } else { "none" };
    let clr = color
        .map(|h| format!("<a:solidFill><a:srgbClr val=\"{h}\"/></a:solidFill>"))
        .unwrap_or_default();
    for line in text.split('\n') {
        let _ = write!(
            out,
            "<a:p><a:pPr algn=\"{algn}\"/><a:r><a:rPr lang=\"ko-KR\" sz=\"{sz}\" b=\"{b}\" i=\"{i}\" u=\"{u}\" dirty=\"0\">{clr}</a:rPr><a:t>{}</a:t></a:r></a:p>",
            XmlSanitizer::escape(line),
        );
    }
}

fn push_ph(
    out: &mut String,
    sid: usize,
    name: &str,
    ph: &str,
    emu: (u64, u64, u64, u64),
    lines: &[&str],
) {
    let (x, y, cx, cy) = emu;
    let _ = write!(out, "<p:sp><p:nvSpPr><p:cNvPr id=\"{sid}\" name=\"{name}\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr>{ph}</p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/>");
    for line in lines {
        let _ = write!(
            out,
            "<a:p><a:r><a:rPr lang=\"ko-KR\" dirty=\"0\"/><a:t>{}</a:t></a:r></a:p>",
            XmlSanitizer::escape(line)
        );
    }
    if lines.is_empty() {
        out.push_str("<a:p/>");
    }
    out.push_str("</p:txBody></p:sp>");
}

fn push_gf_head(out: &mut String, sid: usize, id: &str, rect: SlideBox, uri: &str) {
    let _ = write!(out, "<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id=\"{sid}\" name=\"{}\"/><p:cNvGraphicFramePr><a:graphicFrameLocks noGrp=\"1\"/></p:cNvGraphicFramePr><p:nvPr/></p:nvGraphicFramePr>", XmlSanitizer::escape(id));
    push_xfrm(out, "p:xfrm", rect);
    let _ = write!(out, "<a:graphic><a:graphicData uri=\"{uri}\">");
}

fn push_sp_open(out: &mut String, sid: usize, id: &str, extra: &str, tx_box: &str, rect: SlideBox) {
    let _ = write!(
        out,
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{sid}\" name=\"{}\"{extra}/><p:cNvSpPr{tx_box}/><p:nvPr/></p:nvSpPr><p:spPr>",
        XmlSanitizer::escape(id)
    );
    push_xfrm(out, "a:xfrm", rect);
}

fn push_element(
    out: &mut String,
    sid: usize,
    el: &SlideElement,
    media: &mut MediaParts,
    charts: &mut Vec<(usize, String)>,
    rels: &mut Vec<(String, &'static str, String)>,
    next_rel: &mut usize,
) {
    match el {
        SlideElement::TextBox {
            id,
            rect,
            text,
            font_size_pt,
            bold,
            italic,
            underline,
            align,
            color,
            fill,
            role,
        } => {
            let extra = role
                .map(|r| format!(" descr=\"eo-role:{}\"", r.as_str()))
                .unwrap_or_default();
            push_sp_open(out, sid, id, &extra, " txBox=\"1\"", *rect);
            out.push_str("<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom>");
            push_fill(out, fill.as_deref());
            out.push_str("<a:ln><a:noFill/></a:ln></p:spPr><p:txBody><a:bodyPr wrap=\"square\"/><a:lstStyle/>");
            push_paras(
                out,
                text,
                *font_size_pt,
                (*bold, *italic, *underline),
                *align,
                color.as_deref(),
            );
            out.push_str("</p:txBody></p:sp>");
        }
        SlideElement::Shape {
            id,
            rect,
            shape_type,
            text,
            fill,
            stroke,
            stroke_width_pt,
            color,
            font_size_pt,
            bold,
            align,
        } => {
            push_sp_open(out, sid, id, "", "", *rect);
            let _ = write!(
                out,
                "<a:prstGeom prst=\"{}\"><a:avLst/></a:prstGeom>",
                shape_type.ooxml_prst()
            );
            push_fill(out, fill.as_deref());
            let _ = write!(out, "<a:ln w=\"{}\">", pt_to_emu(*stroke_width_pt));
            push_fill(out, stroke.as_deref());
            out.push_str("</a:ln></p:spPr><p:txBody><a:bodyPr anchor=\"ctr\"/><a:lstStyle/>");
            push_paras(
                out,
                text,
                *font_size_pt,
                (*bold, false, false),
                *align,
                color.as_deref(),
            );
            out.push_str("</p:txBody></p:sp>");
        }
        SlideElement::Image {
            id,
            rect,
            image,
            alt,
        } => {
            let mref = media.add("ppt/media", image);
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
            let descr = if alt.is_empty() {
                image.alt()
            } else {
                alt.as_str()
            };
            let _ = write!(out, "<p:pic><p:nvPicPr><p:cNvPr id=\"{sid}\" name=\"{}\" descr=\"{}\"/><p:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed=\"{rid}\"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr>", XmlSanitizer::escape(id), XmlSanitizer::escape(descr));
            push_xfrm(out, "a:xfrm", *rect);
            out.push_str("<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr></p:pic>");
        }
        SlideElement::Table {
            id,
            rect,
            rows,
            header_row,
            fill,
        } => {
            let cols = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
            let (col_emu, row_emu) = (
                (pt_to_emu(rect.w) / cols as u64).max(1),
                (pt_to_emu(rect.h) / rows.len().max(1) as u64).max(1),
            );
            push_gf_head(
                out,
                sid,
                id,
                *rect,
                "http://schemas.openxmlformats.org/drawingml/2006/table",
            );
            let _ = write!(
                out,
                "<a:tbl><a:tblPr firstRow=\"{}\" bandRow=\"1\"/><a:tblGrid>",
                u8::from(*header_row)
            );
            for _ in 0..cols {
                let _ = write!(out, "<a:gridCol w=\"{col_emu}\"/>");
            }
            out.push_str("</a:tblGrid>");
            for (r_idx, row) in rows.iter().enumerate() {
                let _ = write!(out, "<a:tr h=\"{row_emu}\">");
                let cell_fill = fill.as_deref().filter(|_| !header_row || r_idx == 0);
                for c_idx in 0..cols {
                    let cell = row.get(c_idx).map_or("", String::as_str);
                    out.push_str("<a:tc><a:txBody><a:bodyPr/><a:lstStyle/>");
                    for line in cell.split('\n') {
                        let _ = write!(out, "<a:p><a:r><a:rPr lang=\"ko-KR\" dirty=\"0\"/><a:t>{}</a:t></a:r></a:p>", XmlSanitizer::escape(line));
                    }
                    match cell_fill {
                        Some(h) => {
                            let _ = write!(out, "</a:txBody><a:tcPr><a:solidFill><a:srgbClr val=\"{h}\"/></a:solidFill></a:tcPr></a:tc>");
                        }
                        None => out.push_str("</a:txBody><a:tcPr/></a:tc>"),
                    }
                }
                out.push_str("</a:tr>");
            }
            out.push_str("</a:tbl></a:graphicData></a:graphic></p:graphicFrame>");
        }
        SlideElement::Chart {
            id,
            rect,
            chart_type,
            title,
            categories,
            series,
            legend,
        } => {
            let num = charts.len() + 1;
            charts.push((
                num,
                build_slide_chart_xml(*chart_type, title, categories, series, *legend),
            ));
            let rid = format!("rId{}", *next_rel);
            *next_rel += 1;
            rels.push((rid.clone(), "chart", format!("../charts/chart{num}.xml")));
            push_gf_head(
                out,
                sid,
                id,
                *rect,
                "http://schemas.openxmlformats.org/drawingml/2006/chart",
            );
            let _ = write!(out, "<c:chart xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:id=\"{rid}\"/></a:graphicData></a:graphic></p:graphicFrame>");
        }
    }
}

/// Packs a [`RichSlideDeck`] into a `.pptx` or `.ppsx` OOXML package.
pub fn pack_pptx_rich(
    deck: &RichSlideDeck,
    is_slideshow: bool,
) -> Result<Vec<u8>, EasyOfficeError> {
    deck.validate()?;
    let slideshow = is_slideshow || deck.is_slideshow;
    let (emu_w, emu_h) = deck.aspect.dimensions_emu();
    let mut media = MediaParts::new();
    let mut charts: Vec<(usize, String)> = Vec::new();
    let mut slide_entries: Vec<(String, Vec<u8>)> = Vec::with_capacity(deck.slides.len() * 4);

    for (idx, slide) in deck.slides.iter().enumerate() {
        let n = idx + 1;
        let mut s_rels = vec![(
            "rId1".to_string(),
            "slideLayout",
            format!(
                "../slideLayouts/slideLayout{}.xml",
                slide.layout.layout_index()
            ),
        )];
        let (mut next_rel, mut sid) = (2usize, 2usize);
        let mut s_xml = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:sld {PML_NS}><p:cSld><p:spTree>{PML_SP_TREE_HEADER}");
        let (tw, bh) = (
            emu_w.saturating_sub(914_400).max(1),
            emu_h.saturating_sub(2_057_400).max(1),
        );

        if !slide.title.is_empty() || slide.elements.is_empty() {
            let lines: Vec<&str> = if slide.title.is_empty() {
                vec![]
            } else {
                slide.title.split('\n').collect()
            };
            push_ph(
                &mut s_xml,
                sid,
                "Title 1",
                "<p:ph type=\"title\"/>",
                (457_200, 274_638, tw, 1_143_000),
                &lines,
            );
            sid += 1;
        }
        if !slide.bullets.is_empty() || slide.elements.is_empty() {
            let lines: Vec<&str> = slide.bullets.iter().map(String::as_str).collect();
            push_ph(
                &mut s_xml,
                sid,
                "Content Placeholder 2",
                "<p:ph idx=\"1\"/>",
                (457_200, 1_600_200, tw, bh),
                &lines,
            );
            sid += 1;
        }
        for el in &slide.elements {
            push_element(
                &mut s_xml,
                sid,
                el,
                &mut media,
                &mut charts,
                &mut s_rels,
                &mut next_rel,
            );
            sid += 1;
        }
        s_xml.push_str("</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>");
        match slide.transition {
            SlideTransition::None => {}
            SlideTransition::Fade => s_xml.push_str("<p:transition><p:fade/></p:transition>"),
            SlideTransition::Push => {
                s_xml.push_str("<p:transition><p:push dir=\"l\"/></p:transition>")
            }
            SlideTransition::Zoom => {
                s_xml.push_str("<p:transition><p:zoom dir=\"in\"/></p:transition>")
            }
        }
        s_xml.push_str("</p:sld>");
        slide_entries.push((format!("ppt/slides/slide{n}.xml"), s_xml.into_bytes()));

        if !slide.notes.is_empty() {
            s_rels.push((
                format!("rId{next_rel}"),
                "notesSlide",
                format!("../notesSlides/notesSlide{n}.xml"),
            ));
            slide_entries.push((
                format!("ppt/notesSlides/notesSlide{n}.xml"),
                notes_slide_xml(&slide.notes).into_bytes(),
            ));
            slide_entries.push((
                format!("ppt/notesSlides/_rels/notesSlide{n}.xml.rels"),
                rels_xml(&[
                    (
                        "rId1",
                        "notesMaster",
                        "../notesMasters/notesMaster1.xml".into(),
                    ),
                    ("rId2", "slide", format!("../slides/slide{n}.xml")),
                ]),
            ));
        }
        slide_entries.push((
            format!("ppt/slides/_rels/slide{n}.xml.rels"),
            rels_xml(&s_rels),
        ));
    }
    finish_pptx_package(deck, slideshow, slide_entries, media, charts)
}

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
    Ok(Some((emu_to_pt(x), emu_to_pt(y))))
}

fn parse_cnvpr(attrs: &Attrs<'_>, id: &mut String, descr: Option<&mut String>) {
    if let Some(n) = attr(attrs, "name").filter(|s| !s.trim().is_empty()) {
        *id = n.into_owned();
    }
    if let Some(out_d) = descr.filter(|d| d.is_empty()) {
        if let Some(d) = attr(attrs, "descr") {
            *out_d = d.into_owned();
        }
    }
}

struct SlideCtx<'a> {
    sx: f64,
    sy: f64,
    rels: &'a HashMap<String, String>,
    map: &'a HashMap<&'a str, &'a ZipArchiveEntry>,
    cache: &'a mut HashMap<String, RichImage>,
    budget: &'a mut DecodeBudget,
}

fn resolve_rel_entry<'a>(
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

fn parse_xfrm(
    events: &[XmlEvent<'_>],
    ignore_tc: bool,
    sx: f64,
    sy: f64,
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
    let raw = SlideBox { x, y, w, h };
    raw.validate()?;
    let scaled = raw.scaled(sx, sy);
    scaled.validate()?;
    Ok(Some(scaled))
}

enum ParsedSp {
    Placeholder {
        is_title: bool,
        paragraphs: Vec<String>,
    },
    LooseText(Vec<String>),
    Element(SlideElement),
}

fn parse_sp(
    events: &[XmlEvent<'_>],
    sx: f64,
    sy: f64,
    fallback_id: &str,
) -> Result<ParsedSp, EasyOfficeError> {
    let mut id = fallback_id.to_string();
    let (mut descr, mut ph_type, mut fill, mut stroke, mut t_clr) = (None, None, None, None, None);
    let (mut tx_box, mut has_ph, mut bold, mut italic, mut underline) =
        (false, false, false, false, false);
    let (mut in_sp, mut in_ln, mut in_rpr, mut in_t, mut in_p) =
        (false, false, false, false, false);
    let (mut prst, mut stroke_w, mut font_pt, mut align) =
        (SlideShapeType::Rect, DEFAULT_SLIDE_STROKE_PT, None, None);
    let (mut paras, mut cur_p) = (Vec::new(), String::new());

    for ev in events {
        match ev {
            XmlEvent::Start { name, attrs } => match loc(name) {
                "cNvPr" => {
                    parse_cnvpr(attrs, &mut id, None);
                    descr = descr.or_else(|| attr(attrs, "descr").map(|s| s.into_owned()));
                }
                "cNvSpPr" if attr(attrs, "txBox").as_deref() == Some("1") => tx_box = true,
                "ph" => {
                    has_ph = true;
                    ph_type = ph_type.or_else(|| attr(attrs, "type").map(|s| s.into_owned()));
                }
                "spPr" => in_sp = true,
                "prstGeom" if in_sp => {
                    if let Some(p) = attr(attrs, "prst").and_then(|s| SlideShapeType::parse(&s)) {
                        prst = p;
                    }
                }
                "ln" if in_sp => {
                    in_ln = true;
                    if let Some(w) = attr(attrs, "w").and_then(|s| s.trim().parse::<f64>().ok()) {
                        stroke_w = emu_to_pt(w).clamp(0.0, MAX_SLIDE_STROKE_PT);
                    }
                }
                "noFill" if in_sp => {
                    if in_ln {
                        stroke = None
                    } else {
                        fill = None
                    }
                }
                "srgbClr" if in_sp && !in_rpr => {
                    if let Some(h) = attr(attrs, "val").and_then(|v| normalize_hex_color(&v)) {
                        if in_ln {
                            stroke = Some(h)
                        } else {
                            fill = Some(h)
                        }
                    }
                }
                "p" => {
                    in_p = true;
                    cur_p.clear();
                }
                "pPr" if in_p && align.is_none() => {
                    align = attr(attrs, "algn").and_then(|a| SlideTextAlign::parse(&a))
                }
                "rPr" | "defRPr" | "endParaRPr" => {
                    in_rpr = true;
                    if font_pt.is_none() {
                        font_pt = attr(attrs, "sz")
                            .and_then(|v| v.trim().parse::<f64>().ok())
                            .map(|sz| (sz / 100.0).clamp(MIN_SLIDE_FONT_PT, MAX_SLIDE_FONT_PT));
                    }
                    bold |= attr(attrs, "b").is_some_and(|v| v == "1" || v == "true");
                    italic |= attr(attrs, "i").is_some_and(|v| v == "1" || v == "true");
                    underline |= attr(attrs, "u").is_some_and(|v| v != "none");
                }
                "srgbClr" if in_rpr && t_clr.is_none() => {
                    t_clr = attr(attrs, "val").and_then(|v| normalize_hex_color(&v))
                }
                "t" if in_p => in_t = true,
                "br" if in_p => cur_p.push('\n'),
                _ => {}
            },
            XmlEvent::Text(t) if in_t => cur_p.push_str(t),
            XmlEvent::End(name) => match loc(name) {
                "spPr" => in_sp = false,
                "ln" => in_ln = false,
                "rPr" | "defRPr" | "endParaRPr" => in_rpr = false,
                "t" => in_t = false,
                "p" => {
                    in_p = false;
                    paras.push(std::mem::take(&mut cur_p));
                }
                _ => {}
            },
            _ => {}
        }
    }

    if has_ph && !tx_box {
        return Ok(ParsedSp::Placeholder {
            is_title: matches!(ph_type.as_deref(), Some("title" | "ctrTitle")),
            paragraphs: paras,
        });
    }
    let Some(rect) = parse_xfrm(events, false, sx, sy)? else {
        return Ok(ParsedSp::LooseText(paras));
    };
    let text = paras.join("\n");
    if tx_box {
        let role = descr
            .as_deref()
            .and_then(|d| d.strip_prefix("eo-role:"))
            .and_then(SlidePlaceholderRole::parse);
        Ok(ParsedSp::Element(SlideElement::TextBox {
            id,
            rect,
            text,
            font_size_pt: font_pt.unwrap_or(DEFAULT_SLIDE_FONT_PT),
            bold,
            italic,
            underline,
            align: align.unwrap_or(SlideTextAlign::Left),
            color: t_clr,
            fill,
            role,
        }))
    } else {
        Ok(ParsedSp::Element(SlideElement::Shape {
            id,
            rect,
            shape_type: prst,
            text,
            fill,
            stroke,
            stroke_width_pt: stroke_w,
            color: t_clr,
            font_size_pt: font_pt.unwrap_or(16.0),
            bold,
            align: align.unwrap_or(SlideTextAlign::Center),
        }))
    }
}

fn parse_pic(
    events: &[XmlEvent<'_>],
    fallback_id: &str,
    ctx: &mut SlideCtx<'_>,
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
    let (Some(rect), Some(rid)) = (parse_xfrm(events, false, ctx.sx, ctx.sy)?, rid) else {
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
    loose: &mut Vec<String>,
) -> Result<Option<SlideElement>, EasyOfficeError> {
    let mut id = fallback_id.to_string();
    let (mut chart_rid, mut is_table, mut header_row, mut fill) = (None, false, false, None);
    let (mut rows, mut cur_row, mut cell_paras, mut cur_p) =
        (Vec::new(), Vec::new(), Vec::new(), String::new());
    let (mut in_tc, mut in_tc_pr, mut in_p, mut in_t) = (false, false, false, false);

    for ev in events {
        match ev {
            XmlEvent::Start { name, attrs } => match loc(name) {
                "cNvPr" => parse_cnvpr(attrs, &mut id, None),
                "chart" if chart_rid.is_none() => {
                    chart_rid = attr(attrs, "r:id")
                        .or_else(|| attr(attrs, "id"))
                        .map(|s| s.into_owned())
                }
                "tbl" => is_table = true,
                "tblPr" if is_table => {
                    header_row = attr(attrs, "firstRow").is_some_and(|v| v == "1" || v == "true")
                }
                "tr" if is_table => cur_row.clear(),
                "tc" if is_table => {
                    in_tc = true;
                    cell_paras.clear();
                }
                "tcPr" if in_tc => in_tc_pr = true,
                "srgbClr" if in_tc_pr && fill.is_none() => {
                    fill = attr(attrs, "val").and_then(|v| normalize_hex_color(&v))
                }
                "p" if in_tc => {
                    in_p = true;
                    cur_p.clear();
                }
                "t" if in_p => in_t = true,
                _ => {}
            },
            XmlEvent::Text(t) if in_t => cur_p.push_str(t),
            XmlEvent::End(name) => match loc(name) {
                "tcPr" => in_tc_pr = false,
                "t" => in_t = false,
                "p" if in_tc => {
                    in_p = false;
                    cell_paras.push(std::mem::take(&mut cur_p));
                }
                "tc" if is_table => {
                    in_tc = false;
                    cur_row.push(cell_paras.join("\n"));
                }
                "tr" if is_table => rows.push(std::mem::take(&mut cur_row)),
                _ => {}
            },
            _ => {}
        }
    }
    let Some(rect) = parse_xfrm(events, true, ctx.sx, ctx.sy)? else {
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
    let (aspect, sx, sy) = parse_slide_size_aspect(pres_xml)?;
    let mut budget = DecodeBudget::new();
    budget.charge(pres_xml.len(), "PPTX 프레젠테이션")?;

    let pres_rels = match entry_map.get("ppt/_rels/presentation.xml.rels") {
        Some(&e) => read_ppt_rels(utf8_entry(e, "ppt/_rels/presentation.xml.rels")?, "ppt")?,
        None => HashMap::new(),
    };
    let ordered_slides =
        OoxmlCodec::ordered_pptx_slides(pres_xml, &entries, &entry_map, &pres_rels)?;

    let mut media_cache = HashMap::new();
    let (mut total_el, mut total_img_b) = (0usize, 0usize);
    let mut slides = Vec::with_capacity(ordered_slides.len());

    for s_entry in ordered_slides {
        let s_xml = utf8_entry(s_entry, &s_entry.path)?;
        budget.charge(s_xml.len(), "PPTX 슬라이드 XML")?;
        let (dir, file) = s_entry.path.rsplit_once('/').unwrap_or(("", &s_entry.path));
        let rels_path = format!("{dir}/_rels/{file}.rels");
        let (mut slide_rels, mut layout, mut notes) = (
            HashMap::new(),
            SlideMasterLayout::TitleAndContent,
            String::new(),
        );
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
                if rt.ends_with("/slideLayout") {
                    if let Some(&le) = entry_map.get(tp.as_str()) {
                        if let Some(lt) = extract_attr(utf8_entry(le, &tp)?, "<p:sldLayout", "type")
                        {
                            layout = SlideMasterLayout::from_ooxml_type(lt);
                        }
                    }
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

        let mut reader = XmlReader::with_budget(s_xml, MAX_SLIDE_XML_EVENTS);
        let (mut transition, mut in_tr, mut sp_d, mut d) =
            (SlideTransition::None, false, None::<usize>, 0usize);
        let (mut t_parts, mut bullets, mut loose, mut els) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let (mut in_bare_t, mut bare_t) = (false, String::new());
        let mut ctx = SlideCtx {
            sx,
            sy,
            rels: &slide_rels,
            map: &entry_map,
            cache: &mut media_cache,
            budget: &mut budget,
        };

        while let Some(ev) = reader.next_event()? {
            match &ev {
                XmlEvent::Start { name, .. } => {
                    d += 1;
                    let ln = loc(name);
                    if ln == "transition" {
                        in_tr = true;
                        continue;
                    }
                    if in_tr {
                        match ln {
                            "fade" => transition = SlideTransition::Fade,
                            "push" => transition = SlideTransition::Push,
                            "zoom" => transition = SlideTransition::Zoom,
                            _ => {}
                        }
                        continue;
                    }
                    if ln == "spTree" && sp_d.is_none() {
                        sp_d = Some(d);
                        continue;
                    }
                    if sp_d.is_none() && ln == "t" {
                        in_bare_t = true;
                        bare_t.clear();
                    }
                    if sp_d.is_some_and(|sd| d == sd + 1)
                        && matches!(ln, "sp" | "pic" | "graphicFrame")
                    {
                        let kind = ln;
                        let sub = reader.collect_subtree(ev)?;
                        d -= 1;
                        let fid = format!("el-{}", els.len() + 1);
                        let maybe = match kind {
                            "sp" => match parse_sp(&sub, sx, sy, &fid)? {
                                ParsedSp::Placeholder {
                                    is_title,
                                    paragraphs,
                                } => {
                                    let ne = paragraphs.into_iter().filter(|s| !s.is_empty());
                                    if is_title {
                                        t_parts.extend(ne)
                                    } else {
                                        bullets.extend(ne)
                                    }
                                    None
                                }
                                ParsedSp::LooseText(p) => {
                                    loose.extend(p.into_iter().filter(|s| !s.is_empty()));
                                    None
                                }
                                ParsedSp::Element(el) => Some(el),
                            },
                            "pic" => parse_pic(&sub, &fid, &mut ctx)?,
                            "graphicFrame" => {
                                parse_graphic_frame(&sub, &fid, &mut ctx, &mut loose)?
                            }
                            _ => None,
                        };
                        if let Some(el) = maybe {
                            push_checked_slide_element(
                                el,
                                &mut els,
                                &mut total_el,
                                &mut total_img_b,
                            )?;
                        }
                    }
                }
                XmlEvent::Text(t) if in_bare_t => bare_t.push_str(t),
                XmlEvent::End(name) => {
                    let ln = loc(name);
                    if ln == "t" && in_bare_t {
                        in_bare_t = false;
                        if !bare_t.is_empty() {
                            loose.push(std::mem::take(&mut bare_t));
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
        let mut s_title = t_parts.join("\n");
        if s_title.is_empty() {
            if let Some(t) = extract_attr(s_xml, "<p:sld", "title") {
                s_title = XmlSanitizer::unescape(t);
            } else if bullets.is_empty() && !loose.is_empty() {
                s_title = loose.remove(0);
            }
        }
        bullets.extend(loose);
        slides.push(RichSlide {
            title: s_title,
            bullets,
            notes,
            layout,
            transition,
            elements: els,
        });
    }
    let deck = RichSlideDeck {
        title,
        is_slideshow,
        aspect,
        slides,
        elements_rejected: 0,
    };
    deck.validate()?;
    Ok(deck)
}

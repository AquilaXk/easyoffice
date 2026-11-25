// 참조 구현: jmcnamara/rust_xlsxwriter (MIT OR Apache-2.0, Copyright 2022-2026 John McNamara, src/chart.rs, src/drawing.rs; see THIRD_PARTY_NOTICES.md)

//! Lossless ECMA-376 PresentationML (`.pptx`/`.ppsx`) writer and re-exported reader for [`RichSlideDeck`].

use std::fmt::Write;

use crate::ooxml_presentation::{
    build_slide_chart_xml, finish_pptx_package, notes_slide_xml, rels_xml, PML_NS,
};
use crate::ooxml_shared_parts::PML_SP_TREE_HEADER;
use crate::pptx_master::{
    slide_placeholder_specs, slide_placeholder_visibility, two_column_placeholder_specs,
    PlaceholderSpec,
};
pub use crate::pptx_rich_read::unpack_pptx_rich;
use crate::rich_image_writers::MediaParts;
use crate::rich_slide::{
    pt_to_emu, pt_to_signed_emu, RichSlideDeck, SlideBox, SlideElement, SlideMasterLayout,
    SlideTextAlign, SlideTransition,
};
use crate::{EasyOfficeError, XmlSanitizer};

fn push_xfrm(out: &mut String, tag: &str, r: SlideBox) {
    let (x, y, cx, cy) = (
        pt_to_signed_emu(r.x),
        pt_to_signed_emu(r.y),
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
            "<a:p><a:pPr algn=\"{algn}\"/><a:r><a:rPr lang=\"ko-KR\" sz=\"{sz}\" b=\"{b}\" i=\"{i}\" u=\"{u}\" dirty=\"0\">{clr}<a:latin typeface=\"+mn-lt\"/><a:ea typeface=\"+mn-ea\"/></a:rPr><a:t>{}</a:t></a:r></a:p>",
            XmlSanitizer::escape(line),
        );
    }
}

fn push_ph(out: &mut String, sid: usize, spec: &PlaceholderSpec, lines: &[&str]) {
    let emu = spec.emu;
    let _ = write!(
        out,
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{sid}\" name=\"{}\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr>{}</p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/>",
        spec.name, spec.ph_xml, emu.x, emu.y, emu.cx, emu.cy
    );
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
            italic,
            underline,
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
                (*bold, *italic, *underline),
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
        let (t_spec, b_spec) = slide_placeholder_specs(slide.layout, emu_w, emu_h);
        let (emit_title, emit_body) = slide_placeholder_visibility(slide);

        if emit_title {
            let lines: Vec<&str> = if slide.title.is_empty() {
                vec![]
            } else {
                slide.title.split('\n').collect()
            };
            push_ph(&mut s_xml, sid, &t_spec, &lines);
            sid += 1;
        }
        if emit_body {
            if slide.layout == SlideMasterLayout::TwoColumn {
                let (left_spec, right_spec) = two_column_placeholder_specs(emu_w, emu_h);
                let mid = slide.bullets.len().div_ceil(2);
                let left: Vec<&str> = slide.bullets[..mid].iter().map(String::as_str).collect();
                let right: Vec<&str> = slide.bullets[mid..].iter().map(String::as_str).collect();
                push_ph(&mut s_xml, sid, &left_spec, &left);
                sid += 1;
                push_ph(&mut s_xml, sid, &right_spec, &right);
                sid += 1;
            } else {
                let lines: Vec<&str> = slide.bullets.iter().map(String::as_str).collect();
                push_ph(&mut s_xml, sid, &b_spec, &lines);
                sid += 1;
            }
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

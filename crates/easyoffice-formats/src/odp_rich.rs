//! Lossless ODF 1.3 Presentation (`.odp`) reader and writer for [`RichSlideDeck`].

use std::collections::HashMap;
use std::fmt::Write;

use crate::decode_budget::DecodeBudget;
use crate::odf_package::{
    build_odp_chart_xml, extract_odp_rich_parts, finish_odp_package, parse_odp_aspect,
    parse_odp_chart_xml, parse_odp_layout,
};
use crate::rich_document::normalize_hex_color;
use crate::rich_image::safe_package_path;
use crate::rich_image_writers::MediaParts;
use crate::rich_slide::{
    normalize_table_rows, parse_length_pt, push_checked_slide_element, RichSlide, RichSlideDeck,
    SlideBox, SlideElement, SlidePlaceholderRole, SlideShapeType, SlideTextAlign, SlideTransition,
    DEFAULT_SLIDE_FONT_PT, DEFAULT_SLIDE_STROKE_PT, MAX_SLIDES_PER_DECK, MAX_SLIDE_FONT_PT,
    MAX_SLIDE_STROKE_PT, MAX_SLIDE_TABLE_COLS, MAX_SLIDE_TABLE_ROWS, MIN_SLIDE_FONT_PT,
};
use crate::rtf_xml_helpers::{extract_attr, extract_tag_bodies};
use crate::xml_stream::{attr, Attrs, XmlEvent, XmlReader};
use crate::{EasyOfficeError, RichImage, XmlSanitizer, ZipArchiveEntry};

const MAX_ODP_XML_EVENTS: usize = 512_000;
const ODP_CONTENT_NS: &str = concat!(
    "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
    "xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" ",
    "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" ",
    "xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" ",
    "xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" ",
    "xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" ",
    "xmlns:xlink=\"urn:oasis:names:tc:opendocument:xmlns:xlink\" ",
    "xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" ",
    "xmlns:presentation=\"urn:oasis:names:tc:opendocument:xmlns:presentation:1.0\" ",
    "xmlns:smil=\"urn:oasis:names:tc:opendocument:xmlns:smil-compatible:1.0\""
);

#[inline]
fn loc(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

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

struct StyleGen {
    auto_xml: String,
    counter: usize,
}

impl StyleGen {
    fn new() -> Self {
        Self {
            auto_xml: String::with_capacity(4_096),
            counter: 0,
        }
    }

    fn next_id(&mut self, prefix: &str) -> String {
        self.counter += 1;
        format!("{prefix}{}", self.counter)
    }

    fn add_graphic(
        &mut self,
        fill: Option<&str>,
        stroke: Option<&str>,
        stroke_w: Option<f64>,
    ) -> String {
        let id = self.next_id("gr");
        let _ = write!(
            self.auto_xml,
            "<style:style style:name=\"{id}\" style:family=\"graphic\"><style:graphic-properties"
        );
        match fill {
            Some(h) => {
                let _ = write!(
                    self.auto_xml,
                    " draw:fill=\"solid\" draw:fill-color=\"#{h}\""
                );
            }
            None => self.auto_xml.push_str(" draw:fill=\"none\""),
        }
        match stroke {
            Some(h) => {
                let _ = write!(
                    self.auto_xml,
                    " draw:stroke=\"solid\" svg:stroke-color=\"#{h}\""
                );
            }
            None => self.auto_xml.push_str(" draw:stroke=\"none\""),
        }
        if let Some(w) = stroke_w {
            let _ = write!(self.auto_xml, " svg:stroke-width=\"{w:.2}pt\"");
        }
        self.auto_xml.push_str("/></style:style>");
        id
    }

    fn add_para_text(
        &mut self,
        align: SlideTextAlign,
        font_pt: f64,
        bold: bool,
        italic: bool,
        underline: bool,
        color: Option<&str>,
    ) -> (String, String) {
        let pid = self.next_id("P");
        let odf_al = match align {
            SlideTextAlign::Left => "start",
            SlideTextAlign::Center => "center",
            SlideTextAlign::Right => "end",
        };
        let _ = write!(self.auto_xml, "<style:style style:name=\"{pid}\" style:family=\"paragraph\"><style:paragraph-properties fo:text-align=\"{odf_al}\"/></style:style>");

        let tid = self.next_id("T");
        let fw = if bold { "bold" } else { "normal" };
        let fs = if italic { "italic" } else { "normal" };
        let ul = if underline { "solid" } else { "none" };
        let clr = color
            .map(|h| format!(" fo:color=\"#{h}\""))
            .unwrap_or_default();
        let _ = write!(self.auto_xml, "<style:style style:name=\"{tid}\" style:family=\"text\"><style:text-properties fo:font-size=\"{font_pt:.2}pt\" fo:font-weight=\"{fw}\" fo:font-style=\"{fs}\" style:text-underline-style=\"{ul}\"{clr}/></style:style>");
        (pid, tid)
    }
}

fn push_styled_paras(out: &mut String, text: &str, pid: &str, tid: &str) {
    for line in text.split('\n') {
        let _ = write!(
            out,
            "<text:p text:style-name=\"{pid}\"><text:span text:style-name=\"{tid}\">{}</text:span></text:p>",
            XmlSanitizer::escape(line),
        );
    }
}

fn push_odp_element(
    body: &mut String,
    sg: &mut StyleGen,
    el: &SlideElement,
    media: &mut MediaParts,
    charts: &mut Vec<(usize, String)>,
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
            let gid = sg.add_graphic(fill.as_deref(), None, None);
            let (pid, tid) = sg.add_para_text(
                *align,
                *font_size_pt,
                *bold,
                *italic,
                *underline,
                color.as_deref(),
            );
            let role_attr = role
                .map(|r| format!(" presentation:class=\"{}\"", r.as_str()))
                .unwrap_or_default();
            let _ = write!(
                body,
                "<draw:frame draw:name=\"{}\" draw:style-name=\"{gid}\" presentation:user-transformed=\"true\"{role_attr} svg:x=\"{:.3}pt\" svg:y=\"{:.3}pt\" svg:width=\"{:.3}pt\" svg:height=\"{:.3}pt\"><draw:text-box>",
                XmlSanitizer::escape(id), rect.x, rect.y, rect.w, rect.h
            );
            push_styled_paras(body, text, &pid, &tid);
            body.push_str("</draw:text-box></draw:frame>");
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
            let gid = sg.add_graphic(fill.as_deref(), stroke.as_deref(), Some(*stroke_width_pt));
            let (pid, tid) =
                sg.add_para_text(*align, *font_size_pt, *bold, false, false, color.as_deref());
            let _ = write!(
                body,
                "<draw:custom-shape draw:name=\"{}\" draw:style-name=\"{gid}\" svg:x=\"{:.3}pt\" svg:y=\"{:.3}pt\" svg:width=\"{:.3}pt\" svg:height=\"{:.3}pt\"><draw:enhanced-geometry draw:type=\"{}\"/>",
                XmlSanitizer::escape(id), rect.x, rect.y, rect.w, rect.h, shape_type.odf_type()
            );
            push_styled_paras(body, text, &pid, &tid);
            body.push_str("</draw:custom-shape>");
        }
        SlideElement::Image {
            id,
            rect,
            image,
            alt,
        } => {
            let mref = media.add("Pictures", image);
            let desc = if alt.is_empty() {
                image.alt()
            } else {
                alt.as_str()
            };
            let _ = write!(
                body,
                "<draw:frame draw:name=\"{}\" svg:x=\"{:.3}pt\" svg:y=\"{:.3}pt\" svg:width=\"{:.3}pt\" svg:height=\"{:.3}pt\"><draw:image xlink:href=\"{}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/><svg:desc>{}</svg:desc></draw:frame>",
                XmlSanitizer::escape(id), rect.x, rect.y, rect.w, rect.h, mref.path, XmlSanitizer::escape(desc)
            );
        }
        SlideElement::Table {
            id,
            rect,
            rows,
            header_row,
            fill,
        } => {
            let gid = sg.add_graphic(fill.as_deref(), None, None);
            let cols = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
            let _ = write!(
                body,
                "<draw:frame draw:name=\"{}\" draw:style-name=\"{gid}\" svg:x=\"{:.3}pt\" svg:y=\"{:.3}pt\" svg:width=\"{:.3}pt\" svg:height=\"{:.3}pt\"><table:table table:use-first-row-styles=\"{header_row}\"><table:table-column table:number-columns-repeated=\"{cols}\"/>",
                XmlSanitizer::escape(id), rect.x, rect.y, rect.w, rect.h
            );
            for row in rows {
                body.push_str("<table:table-row>");
                for c in 0..cols {
                    let cell = row.get(c).map_or("", String::as_str);
                    body.push_str("<table:table-cell office:value-type=\"string\">");
                    for line in cell.split('\n') {
                        let _ = write!(body, "<text:p>{}</text:p>", XmlSanitizer::escape(line));
                    }
                    body.push_str("</table:table-cell>");
                }
                body.push_str("</table:table-row>");
            }
            body.push_str("</table:table></draw:frame>");
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
            let c_num = charts.len() + 1;
            charts.push((
                c_num,
                build_odp_chart_xml(*chart_type, title, categories, series, *legend),
            ));
            let _ = write!(
                body,
                "<draw:frame draw:name=\"{}\" svg:x=\"{:.3}pt\" svg:y=\"{:.3}pt\" svg:width=\"{:.3}pt\" svg:height=\"{:.3}pt\"><draw:object xlink:href=\"./Object {c_num}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/></draw:frame>",
                XmlSanitizer::escape(id), rect.x, rect.y, rect.w, rect.h
            );
        }
    }
}

/// Packs a [`RichSlideDeck`] into an ODF 1.3 Presentation (`.odp`) package.
pub fn pack_odp_rich(deck: &RichSlideDeck) -> Result<Vec<u8>, EasyOfficeError> {
    deck.validate()?;
    let mut sg = StyleGen::new();
    let mut media = MediaParts::new();
    let mut charts: Vec<(usize, String)> = Vec::new();
    let mut body = String::with_capacity(4_096);

    for (idx, slide) in deck.slides.iter().enumerate() {
        let dp = format!("dp{}", idx + 1);
        let tr_attrs = match slide.transition {
            SlideTransition::None => "presentation:transition-type=\"none\"",
            SlideTransition::Fade => "presentation:transition-type=\"fade\" smil:type=\"fade\"",
            SlideTransition::Push => "presentation:transition-type=\"slideWipe\" smil:type=\"slideWipe\" smil:direction=\"fromLeft\"",
            SlideTransition::Zoom => "presentation:transition-type=\"zoom\" smil:type=\"zoom\"",
        };
        let _ = write!(sg.auto_xml, "<style:style style:name=\"{dp}\" style:family=\"drawing-page\"><style:drawing-page-properties {tr_attrs}/></style:style>");
        let _ = write!(
            body,
            "<draw:page draw:name=\"{}\" draw:style-name=\"{dp}\" draw:master-page-name=\"Standard\" presentation:presentation-page-layout-name=\"eo_{}\">",
            XmlSanitizer::escape(&slide.title), slide.layout.as_str()
        );
        if !slide.title.is_empty() {
            body.push_str(
                "<draw:frame draw:layer=\"layout\" presentation:class=\"title\"><draw:text-box>",
            );
            for line in slide.title.split('\n') {
                let _ = write!(body, "<text:p>{}</text:p>", XmlSanitizer::escape(line));
            }
            body.push_str("</draw:text-box></draw:frame>");
        }
        if !slide.bullets.is_empty() || slide.elements.is_empty() {
            body.push_str("<draw:frame draw:layer=\"layout\"><draw:text-box>");
            for b in &slide.bullets {
                let _ = write!(body, "<text:p>{}</text:p>", XmlSanitizer::escape(b));
            }
            body.push_str("</draw:text-box></draw:frame>");
        }
        for el in &slide.elements {
            push_odp_element(&mut body, &mut sg, el, &mut media, &mut charts);
        }
        if !slide.notes.is_empty() {
            body.push_str("<presentation:notes><draw:frame><draw:text-box>");
            for line in slide.notes.split('\n') {
                let _ = write!(body, "<text:p>{}</text:p>", XmlSanitizer::escape(line));
            }
            body.push_str("</draw:text-box></draw:frame></presentation:notes>");
        }
        body.push_str("</draw:page>");
    }

    let content_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-content {ODP_CONTENT_NS} office:version=\"1.2\" title=\"{}\"><office:automatic-styles>{}</office:automatic-styles><office:body><office:presentation>{body}</office:presentation></office:body></office:document-content>",
        XmlSanitizer::escape(&deck.title), sg.auto_xml
    );
    finish_odp_package(deck, content_xml, media, charts)
}

fn parse_odp_rect(
    attrs: &Attrs<'_>,
    sx: f64,
    sy: f64,
) -> Result<Option<SlideBox>, EasyOfficeError> {
    let (Some(xs), Some(ys), Some(ws), Some(hs)) = (
        attr(attrs, "svg:x").or_else(|| attr(attrs, "x")),
        attr(attrs, "svg:y").or_else(|| attr(attrs, "y")),
        attr(attrs, "svg:width").or_else(|| attr(attrs, "width")),
        attr(attrs, "svg:height").or_else(|| attr(attrs, "height")),
    ) else {
        return Ok(None);
    };
    let (Some(x), Some(y), Some(w), Some(h)) = (
        parse_length_pt(&xs),
        parse_length_pt(&ys),
        parse_length_pt(&ws),
        parse_length_pt(&hs),
    ) else {
        return Err(EasyOfficeError::InvalidArgument(
            "ODP 개체 좌표/크기 단위 형식이 올바르지 않습니다.".to_string(),
        ));
    };
    let raw = SlideBox { x, y, w, h };
    raw.validate()?;
    let scaled = raw.scaled(sx, sy);
    scaled.validate()?;
    Ok(Some(scaled))
}

#[derive(Default)]
struct OdpStyleMaps {
    transitions: HashMap<String, SlideTransition>,
    graphics: HashMap<String, OdfGraphicStyle>,
    paras: HashMap<String, SlideTextAlign>,
    texts: HashMap<String, OdfTextStyle>,
}

fn collect_odp_styles(sub: &[XmlEvent<'_>], maps: &mut OdpStyleMaps) {
    let (mut cur_name, mut family) = (String::new(), String::new());
    let (mut tr, mut gs, mut al, mut ts) = (
        SlideTransition::None,
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
                    tr = SlideTransition::None;
                    gs = OdfGraphicStyle::default();
                    al = SlideTextAlign::Left;
                    ts = OdfTextStyle::default();
                }
                "drawing-page-properties" if family == "drawing-page" => {
                    let t = attr(attrs, "presentation:transition-type")
                        .or_else(|| attr(attrs, "smil:type"))
                        .map(|v| v.to_ascii_lowercase())
                        .unwrap_or_default();
                    tr = match t.as_str() {
                        "fade" => SlideTransition::Fade,
                        "slidewipe" | "push" => SlideTransition::Push,
                        "zoom" => SlideTransition::Zoom,
                        _ => SlideTransition::None,
                    };
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
    Placeholder { is_title: bool, paras: Vec<String> },
    Loose(Vec<String>),
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
        align: align.unwrap_or(SlideTextAlign::Center),
    }))
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
    let (aspect, sx, sy) = parse_odp_aspect(styles_xml.as_deref());

    let mut budget = DecodeBudget::new();
    budget.charge(content_xml.len(), "ODP content.xml")?;

    let mut reader = XmlReader::with_budget(&content_xml, MAX_ODP_XML_EVENTS);
    let mut style_maps = OdpStyleMaps::default();
    let (mut media_cache, mut total_el, mut total_img_b) = (HashMap::new(), 0usize, 0usize);
    let mut slides = Vec::new();

    while let Some(ev) = reader.next_event()? {
        if let XmlEvent::Start { name, attrs } = &ev {
            let (tag_loc, attrs_copy) = (loc(name), *attrs);
            match tag_loc {
                "automatic-styles" => {
                    let sub = reader.collect_subtree(ev)?;
                    collect_odp_styles(&sub, &mut style_maps);
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
                        .unwrap_or(SlideTransition::None);
                    let mut notes = attr(&attrs_copy, "notes")
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
                    let (mut title_parts, mut bullets, mut loose, mut els) =
                        (Vec::new(), Vec::new(), Vec::new(), Vec::new());

                    let mut i = 1usize;
                    while i + 1 < page_events.len() {
                        if let XmlEvent::Start { name: cn, .. } = &page_events[i] {
                            let tag = loc(cn);
                            let start = i;
                            let mut depth = 1usize;
                            i += 1;
                            while i < page_events.len() && depth > 0 {
                                match &page_events[i] {
                                    XmlEvent::Start { .. } => depth += 1,
                                    XmlEvent::End(_) => depth -= 1,
                                    _ => {}
                                }
                                i += 1;
                            }
                            let sub = &page_events[start..i];
                            if tag == "notes" {
                                let (np, _, _) = parse_text_runs(sub, ctx.styles);
                                let nt = np
                                    .into_iter()
                                    .filter(|s| !s.trim().is_empty())
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                if !nt.is_empty() && notes.is_empty() {
                                    notes = nt;
                                }
                                continue;
                            }
                            let fid = format!("el-{}", els.len() + 1);
                            let maybe = match tag {
                                "frame" => match parse_odp_frame(sub, &fid, &mut ctx)? {
                                    Some(OdpItem::Placeholder { is_title, paras }) => {
                                        let ne = paras.into_iter().filter(|s| !s.is_empty());
                                        if is_title {
                                            title_parts.extend(ne)
                                        } else {
                                            bullets.extend(ne)
                                        }
                                        None
                                    }
                                    Some(OdpItem::Loose(p)) => {
                                        loose.extend(p.into_iter().filter(|s| !s.is_empty()));
                                        None
                                    }
                                    Some(OdpItem::Element(el)) => Some(el),
                                    None => None,
                                },
                                "custom-shape" | "rect" | "ellipse" => {
                                    parse_odp_shape(tag, sub, &fid, &mut ctx)?
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
                        } else {
                            i += 1;
                        }
                    }
                    ctx.budget.charge(notes.len(), "ODP 발표자 노트")?;
                    let s_title = if !title_parts.is_empty() {
                        title_parts.join("\n")
                    } else if els.is_empty() {
                        attr_name
                    } else {
                        String::new()
                    };
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
                _ => {}
            }
        }
    }
    let deck = RichSlideDeck {
        title,
        is_slideshow: false,
        aspect,
        slides,
        elements_rejected: 0,
    };
    deck.validate()?;
    Ok(deck)
}

//! Lossless ODF 1.3 Presentation (`.odp`) writer and re-exported reader for [`RichSlideDeck`].

use std::fmt::Write;

use crate::odf_package::{build_odp_chart_xml, finish_odp_package};
pub use crate::odp_rich_read::unpack_odp_rich;
use crate::pptx_master::{odp_layout_boxes_pt, slide_placeholder_visibility};
use crate::rich_image_writers::MediaParts;
use crate::rich_slide::{
    RichSlideDeck, SlideElement, SlideMasterLayout, SlideTextAlign, SlideTransition,
};
use crate::{EasyOfficeError, XmlSanitizer};

const ODP_CONTENT_NS: &str = concat!(
    "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
    "xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" ",
    "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" ",
    "xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" ",
    "xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" ",
    "xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" ",
    "xmlns:xlink=\"http://www.w3.org/1999/xlink\" ",
    "xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" ",
    "xmlns:presentation=\"urn:oasis:names:tc:opendocument:xmlns:presentation:1.0\" ",
    "xmlns:smil=\"urn:oasis:names:tc:opendocument:xmlns:smil-compatible:1.0\""
);

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
        let _ = write!(
            self.auto_xml,
            "<style:style style:name=\"{tid}\" style:family=\"text\"><style:text-properties style:font-name=\"맑은 고딕\" style:font-name-asian=\"맑은 고딕\" fo:font-size=\"{font_pt:.2}pt\" style:font-size-asian=\"{font_pt:.2}pt\" fo:font-weight=\"{fw}\" style:font-weight-asian=\"{fw}\" fo:font-style=\"{fs}\" style:font-style-asian=\"{fs}\" style:text-underline-style=\"{ul}\"{clr}/></style:style>"
        );
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
            italic,
            underline,
            align,
        } => {
            let gid = sg.add_graphic(fill.as_deref(), stroke.as_deref(), Some(*stroke_width_pt));
            let (pid, tid) = sg.add_para_text(
                *align,
                *font_size_pt,
                *bold,
                *italic,
                *underline,
                color.as_deref(),
            );
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
        let (title_box, body_box, _) =
            odp_layout_boxes_pt(slide.layout, deck.aspect, deck.orientation);
        let (emit_title, emit_body) = slide_placeholder_visibility(slide);
        if emit_title {
            let _ = write!(
                body,
                "<draw:frame draw:layer=\"layout\" presentation:placeholder=\"true\" presentation:class=\"title\" presentation:style-name=\"Standard-title\" svg:x=\"{:.3}pt\" svg:y=\"{:.3}pt\" svg:width=\"{:.3}pt\" svg:height=\"{:.3}pt\"><draw:text-box>",
                title_box.x, title_box.y, title_box.w, title_box.h
            );
            for line in slide.title.split('\n') {
                let _ = write!(body, "<text:p>{}</text:p>", XmlSanitizer::escape(line));
            }
            body.push_str("</draw:text-box></draw:frame>");
        }
        if emit_body {
            let (body_class, body_style) = if slide.layout == SlideMasterLayout::TitleSlide {
                ("subtitle", "Standard-subtitle")
            } else {
                ("outline", "Standard-outline1")
            };
            let _ = write!(
                body,
                "<draw:frame draw:layer=\"layout\" presentation:placeholder=\"true\" presentation:class=\"{body_class}\" presentation:style-name=\"{body_style}\" svg:x=\"{:.3}pt\" svg:y=\"{:.3}pt\" svg:width=\"{:.3}pt\" svg:height=\"{:.3}pt\"><draw:text-box>",
                body_box.x, body_box.y, body_box.w, body_box.h
            );
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
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-content {ODP_CONTENT_NS} office:version=\"1.2\"><office:font-face-decls><style:font-face style:name=\"맑은 고딕\" svg:font-family=\"&apos;맑은 고딕&apos;\" style:font-family-generic=\"swiss\"/></office:font-face-decls><office:automatic-styles>{}</office:automatic-styles><office:body><office:presentation>{body}</office:presentation></office:body></office:document-content>",
        sg.auto_xml
    );
    finish_odp_package(deck, content_xml, media, charts)
}

//! Lossless ODF 1.3 Presentation (`.odp`) writer and re-exported reader for [`RichSlideDeck`].

use std::collections::HashMap;
use std::fmt::Write;

use crate::odf_package::{build_odp_chart_xml, finish_odp_package};
use crate::odp_ext_write::{
    dash_attrs, footer_frames, list_style_xml, para_attrs, place_attrs, push_timeline,
    shadow_attrs, OdpFills, FOOTER_TEXT, LOEXT_NS,
};
pub use crate::odp_rich_read::unpack_odp_rich;
use crate::odp_transition::transition_attrs;
use crate::pptx_master::{odp_layout_boxes_pt, slide_placeholder_visibility};
use crate::rich_image_writers::MediaParts;
use crate::rich_slide::{RichSlideDeck, SlideBox, SlideElement, SlideMasterLayout, SlideTextAlign};
use crate::rich_slide_ext::{hex6, ElementFx, SlideBullet, SlideExt, SlideNode, SlideParaFmt};
use crate::rich_slide_table::{cover_map, CellCover};
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
    "xmlns:smil=\"urn:oasis:names:tc:opendocument:xmlns:smil-compatible:1.0\" ",
    "xmlns:anim=\"urn:oasis:names:tc:opendocument:xmlns:animation:1.0\""
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

    /// Graphic style: fill, line (color and width) and the dash and shadow of the element's effects.
    /// `mirror` also writes its flips as `style:mirror` (pictures; shapes mirror their geometry).
    fn add_graphic(
        &mut self,
        fill: Option<&str>,
        line: Option<(Option<&str>, f64)>,
        fx: Option<&ElementFx>,
        mirror: bool,
    ) -> String {
        let id = self.next_id("gr");
        let _ = write!(
            self.auto_xml,
            "<style:style style:name=\"{id}\" style:family=\"graphic\"><style:graphic-properties"
        );
        match fill {
            Some(h) => {
                let h = hex6(h);
                let _ = write!(
                    self.auto_xml,
                    " draw:fill=\"solid\" draw:fill-color=\"#{h}\""
                );
            }
            None => self.auto_xml.push_str(" draw:fill=\"none\""),
        }
        match line {
            Some((Some(h), w)) => {
                let h = hex6(h);
                let dash = dash_attrs(fx.map_or(Default::default(), |f| f.dash));
                let _ = write!(
                    self.auto_xml,
                    "{dash} svg:stroke-color=\"#{h}\" svg:stroke-width=\"{w:.2}pt\""
                );
            }
            // A shape without a line keeps its width, so a later line comes back at the same weight.
            Some((None, w)) => {
                let _ = write!(
                    self.auto_xml,
                    " draw:stroke=\"none\" svg:stroke-width=\"{w:.2}pt\""
                );
            }
            None => self.auto_xml.push_str(" draw:stroke=\"none\""),
        }
        self.auto_xml
            .push_str(&shadow_attrs(fx.and_then(|f| f.shadow.as_ref())));
        if let Some(f) = fx.filter(|f| mirror && (f.flip_h || f.flip_v)) {
            let value = match (f.flip_h, f.flip_v) {
                (true, true) => "horizontal vertical",
                (true, false) => "horizontal",
                _ => "vertical",
            };
            let _ = write!(self.auto_xml, " style:mirror=\"{value}\"");
        }
        self.auto_xml.push_str("/></style:style>");
        id
    }

    fn add_list(&mut self, bullet: SlideBullet) -> Option<String> {
        let id = self.next_id("L");
        let xml = list_style_xml(&id, bullet)?;
        self.auto_xml.push_str(&xml);
        Some(id)
    }

    fn add_para_text(
        &mut self,
        align: SlideTextAlign,
        font_pt: f64,
        biu: (bool, bool, bool),
        color: Option<&str>,
        para: Option<&SlideParaFmt>,
    ) -> (String, String) {
        let pid = self.next_id("P");
        let attrs = para_attrs(align, para);
        let _ = write!(self.auto_xml, "<style:style style:name=\"{pid}\" style:family=\"paragraph\"><style:paragraph-properties{attrs}/></style:style>");

        let tid = self.next_id("T");
        let fw = if biu.0 { "bold" } else { "normal" };
        let fs = if biu.1 { "italic" } else { "normal" };
        let ul = if biu.2 { "solid" } else { "none" };
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

/// Paragraphs of a text, in a `text:list` when the paragraphs are bulleted or numbered.
fn push_styled_paras(out: &mut String, text: &str, (pid, tid): (&str, &str), list: Option<&str>) {
    if let Some(l) = list {
        let _ = write!(out, "<text:list text:style-name=\"{l}\">");
    }
    for line in text.split('\n') {
        let (open, close) = if list.is_some() {
            ("<text:list-item>", "</text:list-item>")
        } else {
            ("", "")
        };
        let _ = write!(
            out,
            "{open}<text:p text:style-name=\"{pid}\"><text:span text:style-name=\"{tid}\">{}</text:span></text:p>{close}",
            XmlSanitizer::escape(line),
        );
    }
    if list.is_some() {
        out.push_str("</text:list>");
    }
}

/// What the writer keeps while it walks the elements of one page.
struct PageEmit<'a> {
    body: String,
    sg: &'a mut StyleGen,
    media: &'a mut MediaParts,
    charts: &'a mut Vec<(usize, String)>,
    ext: &'a SlideExt,
    /// ODF ids of the elements and groups an animation points at.
    ids: &'a HashMap<String, String>,
}

impl PageEmit<'_> {
    fn id_attr(&self, id: &str) -> String {
        self.ids
            .get(id)
            .map(|x| format!(" xml:id=\"{x}\" draw:id=\"{x}\""))
            .unwrap_or_default()
    }

    fn texts(
        &mut self,
        text: &str,
        look: (SlideTextAlign, f64, (bool, bool, bool), Option<&str>),
        para: Option<&SlideParaFmt>,
    ) {
        let (align, pt, biu, color) = look;
        let (pid, tid) = self.sg.add_para_text(align, pt, biu, color, para);
        let list = para.and_then(|p| self.sg.add_list(p.bullet));
        push_styled_paras(&mut self.body, text, (&pid, &tid), list.as_deref());
    }

    fn element(&mut self, el: &SlideElement) {
        let fx = self.ext.fx_of(el.id());
        let para = fx.and_then(|f| f.para.as_ref());
        let idx = self.id_attr(el.id());
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
                let line = fx
                    .and_then(|f| f.outline.as_ref())
                    .map(|o| (Some(o.color.as_str()), o.width_pt));
                let gid = self.sg.add_graphic(fill.as_deref(), line, fx, false);
                let role_attr = role
                    .map(|r| format!(" presentation:class=\"{}\"", r.as_str()))
                    .unwrap_or_default();
                let _ = write!(
                    self.body,
                    "<draw:frame draw:name=\"{}\"{idx} draw:style-name=\"{gid}\" presentation:user-transformed=\"true\"{role_attr} {}><draw:text-box>",
                    XmlSanitizer::escape(id), place_attrs(*rect, fx)
                );
                self.texts(
                    text,
                    (
                        *align,
                        *font_size_pt,
                        (*bold, *italic, *underline),
                        color.as_deref(),
                    ),
                    para,
                );
                self.body.push_str("</draw:text-box></draw:frame>");
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
                let line = Some((stroke.as_deref(), *stroke_width_pt));
                let gid = self.sg.add_graphic(fill.as_deref(), line, fx, false);
                let mirror = fx.map_or(String::new(), |f| {
                    format!(
                        " draw:mirror-horizontal=\"{}\" draw:mirror-vertical=\"{}\"",
                        f.flip_h, f.flip_v
                    )
                });
                let _ = write!(
                    self.body,
                    "<draw:custom-shape draw:name=\"{}\"{idx} draw:style-name=\"{gid}\" {}><draw:enhanced-geometry svg:viewBox=\"0 0 21600 21600\" draw:type=\"{}\"{mirror}/>",
                    XmlSanitizer::escape(id), place_attrs(*rect, fx), shape_type.odf_type()
                );
                self.texts(
                    text,
                    (
                        *align,
                        *font_size_pt,
                        (*bold, *italic, *underline),
                        color.as_deref(),
                    ),
                    para,
                );
                self.body.push_str("</draw:custom-shape>");
            }
            SlideElement::Image {
                id,
                rect,
                image,
                alt,
            } => {
                let mref = self.media.add("Pictures", image);
                let desc = if alt.is_empty() {
                    image.alt()
                } else {
                    alt.as_str()
                };
                let line = fx
                    .and_then(|f| f.outline.as_ref())
                    .map(|o| (Some(o.color.as_str()), o.width_pt));
                let style = if fx.is_some_and(|f| {
                    f.outline.is_some() || f.shadow.is_some() || f.flip_h || f.flip_v
                }) {
                    format!(
                        " draw:style-name=\"{}\"",
                        self.sg.add_graphic(None, line, fx, true)
                    )
                } else {
                    String::new()
                };
                let _ = write!(
                    self.body,
                    "<draw:frame draw:name=\"{}\"{idx}{style} {}><draw:image xlink:href=\"{}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/><svg:desc>{}</svg:desc></draw:frame>",
                    XmlSanitizer::escape(id), place_attrs(*rect, fx), mref.path, XmlSanitizer::escape(desc)
                );
            }
            SlideElement::Table {
                id,
                rect,
                rows,
                header_row,
                fill,
            } => {
                let gid = self.sg.add_graphic(fill.as_deref(), None, None, false);
                let cols = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
                let _ = write!(
                    self.body,
                    "<draw:frame draw:name=\"{}\"{idx} draw:style-name=\"{gid}\" {}><table:table table:use-first-row-styles=\"{header_row}\"><table:table-column table:number-columns-repeated=\"{cols}\"/>",
                    XmlSanitizer::escape(id), place_attrs(*rect, None)
                );
                let cover = cover_map(fx.map_or(&[][..], |f| f.spans.as_slice()), rows.len(), cols);
                for (r, row) in rows.iter().enumerate() {
                    self.body.push_str("<table:table-row>");
                    for (c, &covered) in cover[r].iter().enumerate() {
                        let cell = row.get(c).map_or("", String::as_str);
                        match covered {
                            CellCover::Covered { .. } => {
                                self.body.push_str("<table:covered-table-cell/>");
                                continue;
                            }
                            CellCover::Origin(s) => {
                                let _ = write!(
                                    self.body,
                                    "<table:table-cell office:value-type=\"string\" table:number-columns-spanned=\"{}\" table:number-rows-spanned=\"{}\">",
                                    s.col_span, s.row_span
                                );
                            }
                            CellCover::Plain => self
                                .body
                                .push_str("<table:table-cell office:value-type=\"string\">"),
                        }
                        for line in cell.split('\n') {
                            let _ = write!(
                                self.body,
                                "<text:p>{}</text:p>",
                                XmlSanitizer::escape(line)
                            );
                        }
                        self.body.push_str("</table:table-cell>");
                    }
                    self.body.push_str("</table:table-row>");
                }
                self.body.push_str("</table:table></draw:frame>");
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
                let c_num = self.charts.len() + 1;
                self.charts.push((
                    c_num,
                    build_odp_chart_xml(*chart_type, title, categories, series, *legend),
                ));
                let _ = write!(
                    self.body,
                    "<draw:frame draw:name=\"{}\"{idx} {}><draw:object xlink:href=\"./Object {c_num}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/></draw:frame>",
                    XmlSanitizer::escape(id), place_attrs(*rect, None)
                );
            }
        }
    }

    fn node(&mut self, node: &SlideNode<'_>, elements: &[SlideElement]) {
        match node {
            SlideNode::Leaf(i) => self.element(&elements[*i]),
            SlideNode::Group { id, children } => {
                let idx = self.id_attr(id);
                let _ = write!(
                    self.body,
                    "<draw:g draw:name=\"{}\"{idx}>",
                    XmlSanitizer::escape(id)
                );
                for c in children {
                    self.node(c, elements);
                }
                self.body.push_str("</draw:g>");
            }
        }
    }

    fn footer(&mut self, emu: (u64, u64)) {
        let graphic = self.sg.add_graphic(None, None, None, false);
        for f in footer_frames(self.ext, emu, &graphic) {
            let (pt, color) = FOOTER_TEXT;
            let (pid, tid) =
                self.sg
                    .add_para_text(f.align, pt, (false, false, false), Some(color), None);
            let _ = write!(
                self.body,
                "<draw:frame draw:layer=\"layout\" draw:style-name=\"{}\" presentation:class=\"{}\" presentation:user-transformed=\"true\" {}><draw:text-box><text:p text:style-name=\"{pid}\"><text:span text:style-name=\"{tid}\">{}</text:span></text:p></draw:text-box></draw:frame>",
                f.graphic, f.class, place_attrs(f.rect, None), f.body
            );
        }
    }
}

/// ODF ids for every element or group an animation targets. `xml:id` is unique in the whole document,
/// so `used` counts the ids of all the pages written so far.
fn animation_ids(ext: &SlideExt, used: &mut usize) -> HashMap<String, String> {
    let mut ids = HashMap::new();
    for a in &ext.anims {
        ids.entry(a.target.clone()).or_insert_with(|| {
            *used += 1;
            format!("id{used}")
        });
    }
    ids
}

/// Packs a [`RichSlideDeck`] into an ODF 1.3 Presentation (`.odp`) package.
pub fn pack_odp_rich(deck: &RichSlideDeck) -> Result<Vec<u8>, EasyOfficeError> {
    deck.validate()?;
    let mut sg = StyleGen::new();
    let mut media = MediaParts::new();
    let mut fills = OdpFills::default();
    let mut charts: Vec<(usize, String)> = Vec::new();
    let mut body = String::with_capacity(4_096);
    let mut used_ids = 0usize;

    for (idx, slide) in deck.slides.iter().enumerate() {
        let dp = format!("dp{}", idx + 1);
        let tr_attrs = transition_attrs(
            slide.transition,
            slide.ext.transition_dir,
            slide.ext.advance_after_ms,
        );
        let page_attrs = crate::odp_ext_write::page_attrs(&slide.ext, &mut fills, &mut media);
        let _ = write!(sg.auto_xml, "<style:style style:name=\"{dp}\" style:family=\"drawing-page\"><style:drawing-page-properties {tr_attrs}{page_attrs}/></style:style>");
        let _ = write!(
            body,
            "<draw:page draw:name=\"{}\" draw:style-name=\"{dp}\" draw:master-page-name=\"Standard\" presentation:presentation-page-layout-name=\"eo_{}\">",
            XmlSanitizer::escape(&slide.title), slide.layout.as_str()
        );
        let (title_box, body_box, _) =
            odp_layout_boxes_pt(slide.layout, deck.aspect, deck.orientation);
        let (emit_title, emit_body) = slide_placeholder_visibility(slide);
        if emit_title {
            push_layout_frame(
                &mut body,
                "title",
                "Standard-title",
                title_box,
                &slide.title.split('\n').collect::<Vec<_>>(),
            );
        }
        if emit_body {
            let (class, style) = if slide.layout == SlideMasterLayout::TitleSlide {
                ("subtitle", "Standard-subtitle")
            } else {
                ("outline", "Standard-outline1")
            };
            push_layout_frame(
                &mut body,
                class,
                style,
                body_box,
                &slide.bullets.iter().map(String::as_str).collect::<Vec<_>>(),
            );
        }
        let ids = animation_ids(&slide.ext, &mut used_ids);
        let mut em = PageEmit {
            body,
            sg: &mut sg,
            media: &mut media,
            charts: &mut charts,
            ext: &slide.ext,
            ids: &ids,
        };
        for node in slide.ext.layout(&slide.elements) {
            em.node(&node, &slide.elements);
        }
        em.footer(deck.stage_dimensions_emu());
        body = em.body;
        push_timeline(&mut body, &slide.ext.anims, &ids);
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
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-content {ODP_CONTENT_NS} xmlns:loext=\"{LOEXT_NS}\" office:version=\"1.2\"><office:font-face-decls><style:font-face style:name=\"맑은 고딕\" svg:font-family=\"&apos;맑은 고딕&apos;\" style:font-family-generic=\"swiss\"/></office:font-face-decls><office:automatic-styles>{}</office:automatic-styles><office:body><office:presentation>{body}</office:presentation></office:body></office:document-content>",
        sg.auto_xml
    );
    finish_odp_package(deck, content_xml, media, charts, &fills.styles_xml())
}

/// Title or body placeholder frame of the page layout, with its lines of text.
fn push_layout_frame(body: &mut String, class: &str, style: &str, r: SlideBox, lines: &[&str]) {
    let _ = write!(
        body,
        "<draw:frame draw:layer=\"layout\" presentation:placeholder=\"true\" presentation:class=\"{class}\" presentation:style-name=\"{style}\" svg:x=\"{:.3}pt\" svg:y=\"{:.3}pt\" svg:width=\"{:.3}pt\" svg:height=\"{:.3}pt\"><draw:text-box>",
        r.x, r.y, r.w, r.h
    );
    for line in lines {
        let _ = write!(body, "<text:p>{}</text:p>", XmlSanitizer::escape(line));
    }
    body.push_str("</draw:text-box></draw:frame>");
}

//! Parsing of PPTX `<p:sp>` and `<p:cxnSp>` shapes, connectors, text runs, and `<p:style>` refs.

use crate::pptx_rich_read::{loc, parse_cnvpr, parse_xfrm, GroupFrame, SlideCtx};
use crate::pptx_theme_color::{ColorSpecBuilder, PptxClrMap, PptxThemePalette, StyleRefs};
use crate::rich_slide::{
    emu_to_pt, SlideElement, SlidePlaceholderRole, SlideShapeType, SlideTextAlign,
    DEFAULT_SLIDE_FONT_PT, DEFAULT_SLIDE_STROKE_PT, MAX_SLIDE_FONT_PT, MAX_SLIDE_STROKE_PT,
    MIN_SLIDE_FONT_PT,
};
use crate::xml_stream::{attr, Attrs, XmlEvent};
use crate::EasyOfficeError;

#[derive(Clone, Copy)]
enum FillTarget {
    ShapeFill,
    StrokeFill,
    TextRun,
    TextDef,
    StyleFill,
    StyleLn,
    StyleFont,
}

pub(crate) enum ParsedSp {
    Ignore,
    Placeholder {
        is_title: bool,
        paragraphs: Vec<String>,
    },
    LooseText(Vec<String>),
    Element(SlideElement),
}

fn is_chrome_ph(ph_type: Option<&str>) -> bool {
    matches!(ph_type, Some("dt" | "ftr" | "sldNum" | "hdr" | "sldImg"))
}

struct SpStyleScan {
    sp_pr_d: Option<usize>,
    ln_d: Option<usize>,
    style_d: Option<usize>,
    solid_fill_d: Option<usize>,
    ref_d: Option<usize>,
    color_d: Option<usize>,
    fill: Option<String>,
    stroke: Option<String>,
    stroke_w: f64,
    has_ln_w: bool,
    explicit_no_fill: bool,
    explicit_no_stroke: bool,
    has_arrow: bool,
    prst: SlideShapeType,
    style_refs: StyleRefs,
    fill_target: Option<FillTarget>,
    pending_color: Option<ColorSpecBuilder>,
}

impl Default for SpStyleScan {
    fn default() -> Self {
        Self {
            sp_pr_d: None,
            ln_d: None,
            style_d: None,
            solid_fill_d: None,
            ref_d: None,
            color_d: None,
            fill: None,
            stroke: None,
            stroke_w: DEFAULT_SLIDE_STROKE_PT,
            has_ln_w: false,
            explicit_no_fill: false,
            explicit_no_stroke: false,
            has_arrow: false,
            prst: SlideShapeType::Rect,
            style_refs: StyleRefs::default(),
            fill_target: None,
            pending_color: None,
        }
    }
}

impl SpStyleScan {
    fn handle_start(
        &mut self,
        d: usize,
        ln: &str,
        attrs: &Attrs<'_>,
        rpr_target: Option<FillTarget>,
    ) -> bool {
        if self.color_d.is_some_and(|cd| d == cd + 1) {
            if let Some(b) = self.pending_color.as_mut() {
                b.push_transform(ln, attrs);
            }
            return true;
        }
        match ln {
            "spPr" if self.sp_pr_d.is_none() => self.sp_pr_d = Some(d),
            "prstGeom" if self.sp_pr_d.is_some_and(|sd| d == sd + 1) => {
                if let Some(p) = attr(attrs, "prst").and_then(|s| SlideShapeType::parse(&s)) {
                    self.prst = p;
                }
            }
            "ln" if self.sp_pr_d.is_some_and(|sd| d == sd + 1) && self.ln_d.is_none() => {
                self.ln_d = Some(d);
                if let Some(w) = attr(attrs, "w").and_then(|s| s.trim().parse::<f64>().ok()) {
                    if w.is_finite() {
                        self.has_ln_w = true;
                        self.stroke_w = emu_to_pt(w).clamp(0.0, MAX_SLIDE_STROKE_PT);
                    }
                }
            }
            "headEnd" | "tailEnd" if self.ln_d.is_some_and(|ld| d == ld + 1) => {
                if attr(attrs, "type").is_some_and(|t| t != "none") {
                    self.has_arrow = true;
                }
            }
            "noFill" if self.ln_d.is_some_and(|ld| d == ld + 1) => {
                self.explicit_no_stroke = true;
                self.stroke = None;
            }
            "noFill" if self.sp_pr_d.is_some_and(|sd| d == sd + 1) => {
                self.explicit_no_fill = true;
                self.fill = None;
            }
            "solidFill" if self.solid_fill_d.is_none() => {
                let target = if self.ln_d.is_some_and(|ld| d == ld + 1) {
                    Some(FillTarget::StrokeFill)
                } else if self.sp_pr_d.is_some_and(|sd| d == sd + 1) {
                    Some(FillTarget::ShapeFill)
                } else {
                    rpr_target
                };
                if target.is_some() {
                    self.solid_fill_d = Some(d);
                    self.fill_target = target;
                }
            }
            "style" if self.style_d.is_none() => self.style_d = Some(d),
            "fillRef" if self.style_d.is_some_and(|sd| d == sd + 1) => {
                self.style_refs.fill_idx = attr(attrs, "idx").and_then(|v| v.trim().parse().ok());
                self.ref_d = Some(d);
                self.fill_target = Some(FillTarget::StyleFill);
            }
            "lnRef" if self.style_d.is_some_and(|sd| d == sd + 1) => {
                self.style_refs.ln_idx = attr(attrs, "idx").and_then(|v| v.trim().parse().ok());
                self.ref_d = Some(d);
                self.fill_target = Some(FillTarget::StyleLn);
            }
            "fontRef" if self.style_d.is_some_and(|sd| d == sd + 1) => {
                self.style_refs.font_active = attr(attrs, "idx").as_deref() != Some("none");
                self.ref_d = Some(d);
                self.fill_target = Some(FillTarget::StyleFont);
            }
            "srgbClr" | "schemeClr" | "sysClr" | "prstClr"
                if self.color_d.is_none()
                    && (self.solid_fill_d.is_some_and(|fd| d == fd + 1)
                        || self.ref_d.is_some_and(|rd| d == rd + 1)) =>
            {
                if let Some(b) = ColorSpecBuilder::from_start(ln, attrs) {
                    self.color_d = Some(d);
                    self.pending_color = Some(b);
                }
            }
            _ => return false,
        }
        true
    }

    fn handle_end(
        &mut self,
        d: usize,
        palette: &PptxThemePalette,
        clr_map: &PptxClrMap,
        run_t_clr: &mut Option<String>,
        def_t_clr: &mut Option<String>,
    ) {
        if self.color_d == Some(d) {
            self.color_d = None;
            if let Some(spec) = self.pending_color.take().and_then(ColorSpecBuilder::finish) {
                let hex = || spec.resolve_hex(palette, clr_map, None);
                match self.fill_target {
                    Some(FillTarget::ShapeFill) => self.fill = hex(),
                    Some(FillTarget::StrokeFill) => self.stroke = hex(),
                    Some(FillTarget::TextRun) => {
                        if run_t_clr.is_none() {
                            *run_t_clr = hex();
                        }
                    }
                    Some(FillTarget::TextDef) => {
                        if def_t_clr.is_none() {
                            *def_t_clr = hex();
                        }
                    }
                    Some(FillTarget::StyleFill) => self.style_refs.fill_color = Some(spec),
                    Some(FillTarget::StyleLn) => self.style_refs.ln_color = Some(spec),
                    Some(FillTarget::StyleFont) => self.style_refs.font_color = Some(spec),
                    None => {}
                }
            }
        }
        if self.solid_fill_d == Some(d) {
            self.solid_fill_d = None;
            self.fill_target = None;
        }
        if self.ref_d == Some(d) {
            self.ref_d = None;
            self.fill_target = None;
        }
        if self.ln_d == Some(d) {
            self.ln_d = None;
        }
        if self.sp_pr_d == Some(d) {
            self.sp_pr_d = None;
        }
        if self.style_d == Some(d) {
            self.style_d = None;
        }
    }

    fn resolve_stroke_fallback(&mut self, palette: &PptxThemePalette, clr_map: &PptxClrMap) {
        if self.stroke.is_none() && !self.explicit_no_stroke {
            let (lw, lc) = self.style_refs.resolve_line(palette, clr_map);
            self.stroke = lc;
            if !self.has_ln_w {
                if let Some(w) = lw {
                    self.stroke_w = w;
                }
            }
        }
    }
}

pub(crate) fn parse_sp(
    events: &[XmlEvent<'_>],
    fallback_id: &str,
    ctx: &SlideCtx<'_>,
    groups: &[GroupFrame],
) -> Result<ParsedSp, EasyOfficeError> {
    let mut id = fallback_id.to_string();
    let (mut descr, mut ph_type, mut ph_idx) = (None, None, None);
    let (mut tx_box, mut has_ph) = (false, false);
    let mut def_biu = (false, false, false);
    let mut run_biu: (Option<bool>, Option<bool>, Option<bool>) = (None, None, None);
    let (mut def_font_pt, mut run_font_pt, mut align) = (None, None, None);
    let (mut run_t_clr, mut def_t_clr) = (None, None);
    let (mut paras, mut cur_p) = (Vec::new(), String::new());
    let (mut d, mut end_para_d, mut rpr_d, mut rpr_is_def) = (0usize, None, None, false);
    let (mut in_p, mut in_t, mut saw_run) = (false, false, false);
    let mut st = SpStyleScan::default();

    for ev in events {
        match ev {
            XmlEvent::Start { name, attrs } => {
                d += 1;
                let ln = loc(name);
                if end_para_d.is_some() {
                    continue;
                }
                let rpr_target = rpr_d.filter(|&rd| d == rd + 1).map(|_| {
                    if rpr_is_def {
                        FillTarget::TextDef
                    } else {
                        FillTarget::TextRun
                    }
                });
                if st.handle_start(d, ln, attrs, rpr_target) {
                    continue;
                }
                match ln {
                    "cNvPr" => {
                        parse_cnvpr(attrs, &mut id, None);
                        descr = descr.or_else(|| attr(attrs, "descr").map(|s| s.into_owned()));
                    }
                    "cNvSpPr" if attr(attrs, "txBox").as_deref() == Some("1") => tx_box = true,
                    "ph" => {
                        has_ph = true;
                        ph_type = ph_type.or_else(|| attr(attrs, "type").map(|s| s.into_owned()));
                        ph_idx = ph_idx.or_else(|| attr(attrs, "idx").map(|s| s.into_owned()));
                    }
                    "p" => {
                        in_p = true;
                        saw_run = false;
                        cur_p.clear();
                    }
                    "r" | "fld" if in_p => saw_run = true,
                    "pPr" if in_p && align.is_none() => {
                        align = attr(attrs, "algn").and_then(|a| SlideTextAlign::parse(&a));
                    }
                    "endParaRPr" => end_para_d = Some(d),
                    "rPr" | "defRPr" if rpr_d.is_none() => {
                        let is_def = ln == "defRPr";
                        rpr_d = Some(d);
                        rpr_is_def = is_def;
                        let sz = attr(attrs, "sz")
                            .and_then(|v| v.trim().parse::<f64>().ok())
                            .filter(|v| v.is_finite())
                            .map(|v| (v / 100.0).clamp(MIN_SLIDE_FONT_PT, MAX_SLIDE_FONT_PT));
                        if is_def {
                            if def_font_pt.is_none() {
                                def_font_pt = sz;
                            }
                            def_biu.0 |= attr(attrs, "b").is_some_and(|v| v == "1" || v == "true");
                            def_biu.1 |= attr(attrs, "i").is_some_and(|v| v == "1" || v == "true");
                            def_biu.2 |= attr(attrs, "u").is_some_and(|v| v != "none");
                        } else {
                            if run_font_pt.is_none() {
                                run_font_pt = sz;
                            }
                            if let Some(v) = attr(attrs, "b") {
                                let b = v == "1" || v == "true";
                                run_biu.0 = Some(run_biu.0.unwrap_or(false) || b);
                            }
                            if let Some(v) = attr(attrs, "i") {
                                let i = v == "1" || v == "true";
                                run_biu.1 = Some(run_biu.1.unwrap_or(false) || i);
                            }
                            if let Some(v) = attr(attrs, "u") {
                                let u = v != "none";
                                run_biu.2 = Some(run_biu.2.unwrap_or(false) || u);
                            }
                        }
                    }
                    "t" if in_p => in_t = true,
                    "br" if in_p => cur_p.push('\n'),
                    _ => {}
                }
            }
            XmlEvent::Text(t) if in_t && end_para_d.is_none() => cur_p.push_str(t),
            XmlEvent::End(name) => {
                let ln = loc(name);
                if end_para_d == Some(d) {
                    end_para_d = None;
                } else if end_para_d.is_none() {
                    st.handle_end(d, ctx.palette, ctx.clr_map, &mut run_t_clr, &mut def_t_clr);
                    if rpr_d == Some(d) {
                        rpr_d = None;
                    }
                    if ln == "t" && in_t {
                        in_t = false;
                    }
                    if ln == "p" && in_p {
                        in_p = false;
                        let _ = saw_run;
                        paras.push(std::mem::take(&mut cur_p));
                    }
                }
                d = d.saturating_sub(1);
            }
            _ => {}
        }
    }

    if has_ph && !tx_box {
        let _ = &ph_idx;
        if is_chrome_ph(ph_type.as_deref()) {
            return Ok(ParsedSp::Ignore);
        }
        return Ok(ParsedSp::Placeholder {
            is_title: matches!(ph_type.as_deref(), Some("title" | "ctrTitle")),
            paragraphs: paras,
        });
    }
    let Some(rect) = parse_xfrm(events, false, ctx.sx, ctx.sy, groups)? else {
        return Ok(ParsedSp::LooseText(paras));
    };

    if st.fill.is_none() && !st.explicit_no_fill {
        st.fill = st.style_refs.resolve_fill(ctx.palette, ctx.clr_map);
    }
    let t_clr = run_t_clr
        .or(def_t_clr)
        .or_else(|| st.style_refs.resolve_font_color(ctx.palette, ctx.clr_map));
    let font_pt = run_font_pt.or(def_font_pt);
    let (bold, italic, underline) = (
        run_biu.0.unwrap_or(def_biu.0),
        run_biu.1.unwrap_or(def_biu.1),
        run_biu.2.unwrap_or(def_biu.2),
    );
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
            fill: st.fill,
            role,
        }))
    } else {
        st.resolve_stroke_fallback(ctx.palette, ctx.clr_map);
        Ok(ParsedSp::Element(SlideElement::Shape {
            id,
            rect,
            shape_type: st.prst,
            text,
            fill: st.fill,
            stroke: st.stroke,
            stroke_width_pt: st.stroke_w,
            color: t_clr,
            font_size_pt: font_pt.unwrap_or(16.0),
            bold,
            italic,
            underline,
            align: align.unwrap_or(SlideTextAlign::Center),
        }))
    }
}

pub(crate) fn parse_cxn_sp(
    events: &[XmlEvent<'_>],
    fallback_id: &str,
    ctx: &SlideCtx<'_>,
    groups: &[GroupFrame],
) -> Result<Option<SlideElement>, EasyOfficeError> {
    let mut id = fallback_id.to_string();
    let mut d = 0usize;
    let (mut unused_run, mut unused_def) = (None, None);
    let mut st = SpStyleScan::default();

    for ev in events {
        match ev {
            XmlEvent::Start { name, attrs } => {
                d += 1;
                let ln = loc(name);
                if ln == "cNvPr" {
                    parse_cnvpr(attrs, &mut id, None);
                } else {
                    st.handle_start(d, ln, attrs, None);
                }
            }
            XmlEvent::End(_) => {
                st.handle_end(
                    d,
                    ctx.palette,
                    ctx.clr_map,
                    &mut unused_run,
                    &mut unused_def,
                );
                d = d.saturating_sub(1);
            }
            XmlEvent::Text(_) => {}
        }
    }

    let Some(rect) = parse_xfrm(events, false, ctx.sx, ctx.sy, groups)? else {
        return Ok(None);
    };
    st.resolve_stroke_fallback(ctx.palette, ctx.clr_map);
    Ok(Some(SlideElement::Shape {
        id,
        rect,
        shape_type: if st.has_arrow {
            SlideShapeType::RightArrow
        } else {
            SlideShapeType::Rect
        },
        text: String::new(),
        fill: None,
        stroke: st.stroke,
        stroke_width_pt: st.stroke_w,
        color: None,
        font_size_pt: DEFAULT_SLIDE_FONT_PT,
        bold: false,
        italic: false,
        underline: false,
        align: SlideTextAlign::Center,
    }))
}

//! ECMA-376 PresentationML (`.pptx`/`.ppsx`) master, layout, placeholder geometry,
//! and Korean default text style helpers.

use std::fmt::Write;

use crate::ooxml_presentation::PML_NS;
use crate::ooxml_shared_parts::{PML_CLR_MAP, PML_SP_TREE_HEADER};
use crate::rich_slide::{RichSlide, SlideAspect, SlideBox, SlideMasterLayout, EMU_PER_PT};

const REF_16_9_W: f64 = 12_192_000.0;
const REF_16_9_H: f64 = 6_858_000.0;

#[derive(Clone, Copy)]
pub(crate) struct EmuBox {
    pub x: u64,
    pub y: u64,
    pub cx: u64,
    pub cy: u64,
}

impl EmuBox {
    const fn new(x: u64, y: u64, cx: u64, cy: u64) -> Self {
        Self { x, y, cx, cy }
    }

    fn scaled(self, emu_w: u64, emu_h: u64) -> Self {
        Self {
            x: scale_emu_x(self.x, emu_w),
            y: scale_emu_y(self.y, emu_h),
            cx: scale_emu_x(self.cx, emu_w),
            cy: scale_emu_y(self.cy, emu_h),
        }
    }

    fn to_pt(self) -> SlideBox {
        SlideBox {
            x: self.x as f64 / EMU_PER_PT,
            y: self.y as f64 / EMU_PER_PT,
            w: self.cx as f64 / EMU_PER_PT,
            h: self.cy as f64 / EMU_PER_PT,
        }
    }
}

pub(crate) struct PlaceholderSpec {
    pub name: &'static str,
    pub ph_xml: &'static str,
    pub emu: EmuBox,
}

#[inline]
fn scale_emu_x(val_16_9: u64, emu_w: u64) -> u64 {
    ((val_16_9 as f64) * (emu_w as f64 / REF_16_9_W))
        .round()
        .max(1.0) as u64
}

#[inline]
fn scale_emu_y(val_16_9: u64, emu_h: u64) -> u64 {
    ((val_16_9 as f64) * (emu_h as f64 / REF_16_9_H))
        .round()
        .max(1.0) as u64
}

/// Returns `(title_emu, body_emu)` placeholder rectangles scaled to `(emu_w, emu_h)`.
pub(crate) fn pptx_placeholder_boxes(
    layout: SlideMasterLayout,
    emu_w: u64,
    emu_h: u64,
) -> (EmuBox, EmuBox) {
    let (title_ref, body_ref) = match layout {
        SlideMasterLayout::TitleSlide => (
            EmuBox::new(1_524_000, 1_562_100, 9_144_000, 2_209_800),
            EmuBox::new(1_524_000, 3_962_400, 9_144_000, 1_371_600),
        ),
        SlideMasterLayout::SectionHeader => (
            EmuBox::new(838_200, 1_981_200, 10_515_600, 1_325_563),
            EmuBox::new(838_200, 3_429_000, 10_515_600, 1_600_200),
        ),
        SlideMasterLayout::TitleAndContent
        | SlideMasterLayout::TwoColumn
        | SlideMasterLayout::Blank => (
            EmuBox::new(838_200, 365_125, 10_515_600, 1_325_563),
            EmuBox::new(838_200, 1_825_625, 10_515_600, 4_381_500),
        ),
    };
    (
        title_ref.scaled(emu_w, emu_h),
        body_ref.scaled(emu_w, emu_h),
    )
}

pub(crate) fn slide_placeholder_specs(
    layout: SlideMasterLayout,
    emu_w: u64,
    emu_h: u64,
) -> (PlaceholderSpec, PlaceholderSpec) {
    let (title_emu, body_emu) = pptx_placeholder_boxes(layout, emu_w, emu_h);
    match layout {
        SlideMasterLayout::TitleSlide => (
            PlaceholderSpec {
                name: "Title 1",
                ph_xml: "<p:ph type=\"ctrTitle\"/>",
                emu: title_emu,
            },
            PlaceholderSpec {
                name: "Subtitle 2",
                ph_xml: "<p:ph type=\"subTitle\" idx=\"1\"/>",
                emu: body_emu,
            },
        ),
        SlideMasterLayout::SectionHeader => (
            PlaceholderSpec {
                name: "Title 1",
                ph_xml: "<p:ph type=\"title\"/>",
                emu: title_emu,
            },
            PlaceholderSpec {
                name: "Text Placeholder 2",
                ph_xml: "<p:ph type=\"body\" idx=\"1\"/>",
                emu: body_emu,
            },
        ),
        SlideMasterLayout::TitleAndContent
        | SlideMasterLayout::TwoColumn
        | SlideMasterLayout::Blank => (
            PlaceholderSpec {
                name: "Title 1",
                ph_xml: "<p:ph type=\"title\"/>",
                emu: title_emu,
            },
            PlaceholderSpec {
                name: "Content Placeholder 2",
                ph_xml: "<p:ph idx=\"1\"/>",
                emu: body_emu,
            },
        ),
    }
}

/// Returns `(left_col, right_col)` placeholder specs matching `build_slide_layout_xml` (`twoObj`).
pub(crate) fn two_column_placeholder_specs(
    emu_w: u64,
    emu_h: u64,
) -> (PlaceholderSpec, PlaceholderSpec) {
    (
        PlaceholderSpec {
            name: "Content Placeholder 2",
            ph_xml: "<p:ph idx=\"1\"/>",
            emu: EmuBox::new(838_200, 1_825_625, 5_167_313, 4_381_500).scaled(emu_w, emu_h),
        },
        PlaceholderSpec {
            name: "Content Placeholder 3",
            ph_xml: "<p:ph idx=\"2\"/>",
            emu: EmuBox::new(6_186_487, 1_825_625, 5_167_313, 4_381_500).scaled(emu_w, emu_h),
        },
    )
}

/// Returns `(emit_title, emit_body)` visibility flags for slide placeholders.
#[inline]
pub(crate) fn slide_placeholder_visibility(slide: &RichSlide) -> (bool, bool) {
    if slide.layout == SlideMasterLayout::Blank {
        (!slide.title.is_empty(), !slide.bullets.is_empty())
    } else {
        (
            !slide.title.is_empty() || slide.elements.is_empty(),
            !slide.bullets.is_empty() || slide.elements.is_empty(),
        )
    }
}

/// Returns `(title_pt, body_pt, right_col_opt_pt)` for ODP layout frames and styles.
pub(crate) fn odp_layout_boxes_pt(
    layout: SlideMasterLayout,
    aspect: SlideAspect,
) -> (SlideBox, SlideBox, Option<SlideBox>) {
    let (emu_w, emu_h) = aspect.dimensions_emu();
    let (t_emu, b_emu) = pptx_placeholder_boxes(layout, emu_w, emu_h);
    if layout == SlideMasterLayout::TwoColumn {
        let (left, right) = two_column_placeholder_specs(emu_w, emu_h);
        return (t_emu.to_pt(), left.emu.to_pt(), Some(right.emu.to_pt()));
    }
    (t_emu.to_pt(), b_emu.to_pt(), None)
}

pub(crate) fn presentation_default_text_style_xml() -> &'static str {
    concat!(
        "<p:defaultTextStyle><a:defPPr><a:defRPr lang=\"ko-KR\"/></a:defPPr>",
        "<a:lvl1pPr marL=\"0\" algn=\"l\" defTabSz=\"914400\" rtl=\"0\" eaLnBrk=\"1\" latinLnBrk=\"0\" hangingPunct=\"1\">",
        "<a:defRPr sz=\"1800\" kern=\"1200\"><a:solidFill><a:schemeClr val=\"tx1\"/></a:solidFill>",
        "<a:latin typeface=\"+mn-lt\"/><a:ea typeface=\"+mn-ea\"/><a:cs typeface=\"+mn-cs\"/></a:defRPr>",
        "</a:lvl1pPr></p:defaultTextStyle>"
    )
}

fn layout_placeholder_sp(
    id: usize,
    name: &str,
    ph: &str,
    emu: EmuBox,
    sz_hundredths: Option<u32>,
    align: Option<&str>,
    body_anchor: &str,
) -> String {
    let mut xml = format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{name}\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr>{ph}</p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm></p:spPr><p:txBody><a:bodyPr anchor=\"{body_anchor}\"/>",
        emu.x, emu.y, emu.cx, emu.cy
    );
    if sz_hundredths.is_some() || align.is_some() {
        xml.push_str("<a:lstStyle><a:lvl1pPr");
        if let Some(a) = align {
            let _ = write!(xml, " algn=\"{a}\"");
        }
        xml.push('>');
        if let Some(sz) = sz_hundredths {
            let _ = write!(xml, "<a:defRPr sz=\"{sz}\"/>");
        }
        xml.push_str("</a:lvl1pPr></a:lstStyle>");
    } else {
        xml.push_str("<a:lstStyle/>");
    }
    xml.push_str("<a:p><a:endParaRPr lang=\"ko-KR\"/></a:p></p:txBody></p:sp>");
    xml
}

pub(crate) fn build_slide_master_xml(emu_w: u64, emu_h: u64, layout_id_entries: &str) -> String {
    let (title_emu, body_emu) =
        pptx_placeholder_boxes(SlideMasterLayout::TitleAndContent, emu_w, emu_h);
    let master_title = layout_placeholder_sp(
        2,
        "Title Placeholder 1",
        "<p:ph type=\"title\"/>",
        title_emu,
        Some(4400),
        None,
        "ctr",
    );
    let master_body = layout_placeholder_sp(
        3,
        "Text Placeholder 2",
        "<p:ph type=\"body\" idx=\"1\"/>",
        body_emu,
        Some(2800),
        None,
        "t",
    );
    format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
            "<p:sldMaster {ns}><p:cSld>",
            "<p:bg><p:bgRef idx=\"1001\"><a:schemeClr val=\"bg1\"/></p:bgRef></p:bg>",
            "<p:spTree>{hdr}{mt}{mb}</p:spTree></p:cSld>",
            "{clr}<p:sldLayoutIdLst>{lids}</p:sldLayoutIdLst>",
            "<p:txStyles>",
            "<p:titleStyle><a:lvl1pPr algn=\"l\" defTabSz=\"914400\" rtl=\"0\" eaLnBrk=\"1\" latinLnBrk=\"0\" hangingPunct=\"1\">",
            "<a:defRPr lang=\"ko-KR\" altLang=\"en-US\" sz=\"4400\" kern=\"1200\"><a:solidFill><a:schemeClr val=\"tx1\"/></a:solidFill>",
            "<a:latin typeface=\"+mj-lt\"/><a:ea typeface=\"+mj-ea\"/><a:cs typeface=\"+mj-cs\"/></a:defRPr></a:lvl1pPr></p:titleStyle>",
            "<p:bodyStyle><a:lvl1pPr marL=\"342900\" indent=\"-342900\" algn=\"l\" defTabSz=\"914400\" rtl=\"0\" eaLnBrk=\"1\" latinLnBrk=\"0\" hangingPunct=\"1\">",
            "<a:buFont typeface=\"맑은 고딕\" pitchFamily=\"34\" charset=\"-127\"/><a:buChar char=\"•\"/>",
            "<a:defRPr lang=\"ko-KR\" altLang=\"en-US\" sz=\"2800\" kern=\"1200\"><a:solidFill><a:schemeClr val=\"tx1\"/></a:solidFill>",
            "<a:latin typeface=\"+mn-lt\"/><a:ea typeface=\"+mn-ea\"/><a:cs typeface=\"+mn-cs\"/></a:defRPr></a:lvl1pPr></p:bodyStyle>",
            "<p:otherStyle><a:defPPr><a:defRPr lang=\"ko-KR\"/></a:defPPr>",
            "<a:lvl1pPr marL=\"0\" algn=\"l\" defTabSz=\"914400\" rtl=\"0\" eaLnBrk=\"1\" latinLnBrk=\"0\" hangingPunct=\"1\">",
            "<a:defRPr lang=\"ko-KR\" altLang=\"en-US\" sz=\"1800\" kern=\"1200\"><a:solidFill><a:schemeClr val=\"tx1\"/></a:solidFill>",
            "<a:latin typeface=\"+mn-lt\"/><a:ea typeface=\"+mn-ea\"/><a:cs typeface=\"+mn-cs\"/></a:defRPr></a:lvl1pPr></p:otherStyle>",
            "</p:txStyles></p:sldMaster>"
        ),
        ns = PML_NS,
        hdr = PML_SP_TREE_HEADER,
        mt = master_title,
        mb = master_body,
        clr = PML_CLR_MAP,
        lids = layout_id_entries,
    )
}

pub(crate) fn build_slide_layout_xml(layout: SlideMasterLayout, emu_w: u64, emu_h: u64) -> String {
    let mut shapes = String::new();
    match layout {
        SlideMasterLayout::TitleSlide => {
            let (t_emu, b_emu) = pptx_placeholder_boxes(layout, emu_w, emu_h);
            shapes.push_str(&layout_placeholder_sp(
                2,
                "Title 1",
                "<p:ph type=\"ctrTitle\"/>",
                t_emu,
                Some(6000),
                Some("ctr"),
                "ctr",
            ));
            shapes.push_str(&layout_placeholder_sp(
                3,
                "Subtitle 2",
                "<p:ph type=\"subTitle\" idx=\"1\"/>",
                b_emu,
                Some(2400),
                Some("ctr"),
                "t",
            ));
        }
        SlideMasterLayout::TitleAndContent => {
            let (t_emu, b_emu) = pptx_placeholder_boxes(layout, emu_w, emu_h);
            shapes.push_str(&layout_placeholder_sp(
                2,
                "Title 1",
                "<p:ph type=\"title\"/>",
                t_emu,
                None,
                None,
                "ctr",
            ));
            shapes.push_str(&layout_placeholder_sp(
                3,
                "Content Placeholder 2",
                "<p:ph idx=\"1\"/>",
                b_emu,
                None,
                None,
                "t",
            ));
        }
        SlideMasterLayout::SectionHeader => {
            let (t_emu, b_emu) = pptx_placeholder_boxes(layout, emu_w, emu_h);
            shapes.push_str(&layout_placeholder_sp(
                2,
                "Title 1",
                "<p:ph type=\"title\"/>",
                t_emu,
                Some(4000),
                None,
                "b",
            ));
            shapes.push_str(&layout_placeholder_sp(
                3,
                "Text Placeholder 2",
                "<p:ph type=\"body\" idx=\"1\"/>",
                b_emu,
                Some(2000),
                None,
                "t",
            ));
        }
        SlideMasterLayout::TwoColumn => {
            let (t_emu, _) = pptx_placeholder_boxes(layout, emu_w, emu_h);
            let (left, right) = two_column_placeholder_specs(emu_w, emu_h);
            shapes.push_str(&layout_placeholder_sp(
                2,
                "Title 1",
                "<p:ph type=\"title\"/>",
                t_emu,
                None,
                None,
                "ctr",
            ));
            shapes.push_str(&layout_placeholder_sp(
                3,
                left.name,
                left.ph_xml,
                left.emu,
                None,
                None,
                "t",
            ));
            shapes.push_str(&layout_placeholder_sp(
                4,
                right.name,
                right.ph_xml,
                right.emu,
                None,
                None,
                "t",
            ));
        }
        SlideMasterLayout::Blank => {}
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:sldLayout {PML_NS} type=\"{}\" preserve=\"1\"><p:cSld name=\"{}\"><p:spTree>{PML_SP_TREE_HEADER}{shapes}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>",
        layout.ooxml_type(),
        layout.ooxml_name()
    )
}

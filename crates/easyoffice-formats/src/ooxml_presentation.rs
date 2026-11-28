//! ECMA-376 PresentationML (`.pptx`/`.ppsx`) writer and reader.
//!
//! The writer emits every part a conforming consumer requires: a DrawingML theme related from the
//! slide master (and notes master), `p:clrMap` on masters, `p:nvGrpSpPr`/`p:grpSpPr` on each shape
//! tree, `p:nvSpPr`/`p:spPr`/`p:txBody` on each shape, slide titles in a title placeholder shape,
//! speaker notes in real notes slides, and the presentation title in `docProps/core.xml`.

use crate::chart_kinds::{axes_xml, plot_groups, STOCK_SERIES_LOOK};
use crate::ooxml_shared_parts::{
    core_properties_xml, drawingml_theme_xml, CORE_PROPERTIES_CONTENT_TYPE,
    CORE_PROPERTIES_REL_TYPE, PML_CLR_MAP, PML_SP_TREE_HEADER,
};
use crate::pptx_deck_settings::{pres_props_xml, theme_xml, PRES_PROPS_CT, PRES_PROPS_PATH};
use crate::pptx_master::{
    build_slide_layout_xml, build_slide_master_xml, presentation_default_text_style_xml,
};
use crate::rich_image_writers::MediaParts;
use crate::rich_slide::{
    RichSlideDeck, SlideAspect, SlideChartSeries, SlideMasterLayout, SlideOrientation,
    MAX_SLIDE_CHART_POINTS, MAX_SLIDE_CHART_SERIES, MAX_SLIDE_SIZE_EMU, MIN_SLIDE_SIZE_EMU,
};
use crate::rich_slide_guides::{needs_view_props, view_props_xml, VIEW_PROPS_CT, VIEW_PROPS_PATH};
use crate::rtf_xml_helpers::{
    extract_attr, extract_attr_from_fragment, extract_balanced_elements, extract_tag_bodies,
};
use crate::sheet_chart::{ChartKind, ChartLegendPos};
use crate::xml_stream::{attr, XmlEvent, XmlReader};
use crate::{
    EasyOfficeError, OoxmlCodec, OoxmlPptxPayload, RichImageFormat, XmlSanitizer, ZipArchiveCodec,
    ZipArchiveEntry,
};
use std::collections::HashSet;
use std::fmt::Write;

pub(crate) const PML_NS: &str = concat!(
    "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" ",
    "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" ",
    "xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\""
);
const REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const REL_BASE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
pub(crate) const CT_PREFIX: &str = "application/vnd.openxmlformats-officedocument";
pub(crate) const SLIDESHOW_MAIN_CT: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.slideshow.main+xml";
pub(crate) const PRESENTATION_MAIN_CT: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml";
pub(crate) const MAX_PPTX_SLIDES: usize = 2_000;

pub(crate) fn rels_xml<A: AsRef<str>, B: AsRef<str>, C: AsRef<str>>(rels: &[(A, B, C)]) -> Vec<u8> {
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"{REL_NS}\">"
    );
    for (id, rel_type, target) in rels {
        let rt = rel_type.as_ref();
        let (sep, base) = if rt.starts_with("http") {
            ("", "")
        } else {
            ("/", REL_BASE)
        };
        let _ = write!(
            xml,
            "<Relationship Id=\"{}\" Type=\"{base}{sep}{rt}\" Target=\"{}\"/>",
            id.as_ref(),
            target.as_ref()
        );
    }
    xml.push_str("</Relationships>");
    xml.into_bytes()
}

pub(crate) fn notes_slide_xml(notes: &str) -> String {
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:notes {PML_NS}><p:cSld><p:spTree>{}<p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"Notes Placeholder 1\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"body\" idx=\"1\"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/>",
        crate::ooxml_shared_parts::PML_SP_TREE_HEADER
    );
    for line in notes.split('\n') {
        let _ = write!(
            xml,
            "<a:p><a:r><a:rPr lang=\"ko-KR\" dirty=\"0\"/><a:t>{}</a:t></a:r></a:p>",
            XmlSanitizer::escape(line)
        );
    }
    xml.push_str("</p:txBody></p:sp></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:notes>");
    xml
}

pub(crate) fn resolve_ppt(base: &str, target: &str) -> Result<String, EasyOfficeError> {
    if target.contains('\\') {
        return Err(EasyOfficeError::SecurityViolation(format!(
            "허용되지 않는 PPTX 경로 구분자입니다: {target}"
        )));
    }
    crate::sheet_styling::xlsx_parts::resolve_zip_target(base, target)
        .filter(|p| p.starts_with("ppt/"))
        .ok_or_else(|| {
            EasyOfficeError::SecurityViolation(format!(
                "PPTX 관계 대상 경로가 패키지 밖을 가리킵니다: {target}"
            ))
        })
}

pub(crate) fn read_ppt_rels(
    xml: &str,
    base: &str,
) -> Result<std::collections::HashMap<String, String>, EasyOfficeError> {
    let mut map = std::collections::HashMap::new();
    for (attrs, _) in extract_balanced_elements(xml, "Relationship") {
        let (Some(id), Some(target)) = (
            extract_attr_from_fragment(attrs, "Id"),
            extract_attr_from_fragment(attrs, "Target"),
        ) else {
            continue;
        };
        if extract_attr_from_fragment(attrs, "TargetMode") == Some("External") {
            continue;
        }
        let b = if target.starts_with("ppt/") { "" } else { base };
        map.insert(id.to_string(), resolve_ppt(b, target)?);
    }
    Ok(map)
}

pub(crate) fn parse_slide_size_aspect(
    pres_xml: &str,
) -> Result<(SlideAspect, SlideOrientation, f64, f64), EasyOfficeError> {
    for (attrs, _) in extract_balanced_elements(pres_xml, "p:sldSz") {
        let (Some(cxs), Some(cys)) = (
            extract_attr_from_fragment(attrs, "cx"),
            extract_attr_from_fragment(attrs, "cy"),
        ) else {
            continue;
        };
        let (Ok(cx), Ok(cy)) = (cxs.trim().parse::<u64>(), cys.trim().parse::<u64>()) else {
            return Err(EasyOfficeError::CorruptedContainer(
                "PPTX 슬라이드 크기(p:sldSz) 값이 올바르지 않습니다.".to_string(),
            ));
        };
        if cx > 0 && cy > 0 {
            // Fail-Closed: a size outside 1 inch to the coordinate cap is not scaled into a preset.
            if !(MIN_SLIDE_SIZE_EMU..=MAX_SLIDE_SIZE_EMU).contains(&cx.min(cy))
                || !(MIN_SLIDE_SIZE_EMU..=MAX_SLIDE_SIZE_EMU).contains(&cx.max(cy))
            {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "PPTX 슬라이드 크기(p:sldSz {cx}×{cy})가 허용 범위(최소 {MIN_SLIDE_SIZE_EMU}, 최대 {MAX_SLIDE_SIZE_EMU} EMU)를 벗어났습니다."
                )));
            }
            return Ok(SlideAspect::from_emu_oriented(cx, cy));
        }
    }
    Ok((
        SlideAspect::Standard4x3,
        SlideOrientation::Landscape,
        1.0,
        1.0,
    ))
}

pub(crate) fn utf8_entry<'a>(
    entry: &'a ZipArchiveEntry,
    label: &str,
) -> Result<&'a str, EasyOfficeError> {
    let xml = std::str::from_utf8(&entry.data)
        .map_err(|_| EasyOfficeError::InvalidUtf8(format!("{label} UTF-8 오류")))?;
    XmlSanitizer::validate_safe_xml(xml)?;
    Ok(xml)
}

fn shape_paragraph_texts(sp_body: &str) -> Vec<String> {
    extract_tag_bodies(sp_body, "a:p")
        .into_iter()
        .map(|p_inner| {
            extract_tag_bodies(p_inner, "a:t")
                .into_iter()
                .map(XmlSanitizer::unescape)
                .collect::<Vec<_>>()
                .join("")
        })
        .collect()
}

pub(crate) fn extract_notes_xml_text(notes_xml: &str) -> Option<String> {
    let mut non_empty = Vec::new();
    let mut saw_included = false;
    for sp in extract_tag_bodies(notes_xml, "p:sp") {
        let nv_pr = extract_tag_bodies(sp, "p:nvSpPr")
            .first()
            .copied()
            .unwrap_or(sp);
        let ph_elems = extract_balanced_elements(nv_pr, "p:ph");
        let include = if ph_elems.is_empty() && !nv_pr.contains("<p:ph") {
            true
        } else {
            ph_elems.iter().any(|(a, _)| {
                let ph_type = extract_attr_from_fragment(a, "type");
                !matches!(ph_type, Some("sldImg" | "sldNum" | "hdr" | "ftr" | "dt"))
                    && ph_type == Some("body")
            })
        };
        if include {
            saw_included = true;
            let text = shape_paragraph_texts(sp).join("\n");
            if !text.is_empty() {
                non_empty.push(text);
            }
        }
    }
    if !non_empty.is_empty() {
        Some(non_empty.join("\n"))
    } else if saw_included {
        Some(String::new())
    } else {
        None
    }
}

pub(crate) const AX_ID_CAT: &str = "64052224";
pub(crate) const AX_ID_VAL: &str = "64055552";
const MAX_CHART_XML_EVENTS: usize = 65_536;
const CHART_CT: &str = "application/vnd.openxmlformats-officedocument.drawingml.chart+xml";

pub(crate) fn finish_pptx_package(
    deck: &RichSlideDeck,
    slideshow: bool,
    slide_entries: Vec<(String, Vec<u8>)>,
    media: MediaParts,
    charts: Vec<(usize, String)>,
) -> Result<Vec<u8>, EasyOfficeError> {
    let (emu_w, emu_h) = deck.stage_dimensions_emu();
    let has_notes = deck.slides.iter().any(|s| !s.notes.is_empty());
    let main_ct = if slideshow {
        SLIDESHOW_MAIN_CT
    } else {
        PRESENTATION_MAIN_CT
    };
    let pml = format!("{CT_PREFIX}.presentationml");
    let mut ct = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/ppt/presentation.xml\" ContentType=\"{main_ct}\"/><Override PartName=\"/ppt/slideMasters/slideMaster1.xml\" ContentType=\"{pml}.slideMaster+xml\"/><Override PartName=\"/ppt/theme/theme1.xml\" ContentType=\"{CT_PREFIX}.theme+xml\"/><Override PartName=\"/docProps/core.xml\" ContentType=\"{CORE_PROPERTIES_CONTENT_TYPE}\"/>"
    );
    for fmt in [RichImageFormat::Png, RichImageFormat::Jpeg] {
        if media.has_format(fmt) {
            let _ = write!(
                ct,
                "<Default Extension=\"{}\" ContentType=\"{}\"/>",
                fmt.extension(),
                fmt.mime()
            );
        }
    }
    for layout in SlideMasterLayout::ALL {
        let _ = write!(ct, "<Override PartName=\"/ppt/slideLayouts/slideLayout{}.xml\" ContentType=\"{pml}.slideLayout+xml\"/>", layout.layout_index());
    }
    let _ = write!(
        ct,
        "<Override PartName=\"/{PRES_PROPS_PATH}\" ContentType=\"{PRES_PROPS_CT}\"/>"
    );
    let has_view = needs_view_props(&deck.settings);
    if has_view {
        let _ = write!(
            ct,
            "<Override PartName=\"/{VIEW_PROPS_PATH}\" ContentType=\"{VIEW_PROPS_CT}\"/>"
        );
    }
    if has_notes {
        let _ = write!(ct, "<Override PartName=\"/ppt/theme/theme2.xml\" ContentType=\"{CT_PREFIX}.theme+xml\"/><Override PartName=\"/ppt/notesMasters/notesMaster1.xml\" ContentType=\"{pml}.notesMaster+xml\"/>");
    }
    for (idx, slide) in deck.slides.iter().enumerate() {
        let n = idx + 1;
        let _ = write!(
            ct,
            "<Override PartName=\"/ppt/slides/slide{n}.xml\" ContentType=\"{pml}.slide+xml\"/>"
        );
        if !slide.notes.is_empty() {
            let _ = write!(ct, "<Override PartName=\"/ppt/notesSlides/notesSlide{n}.xml\" ContentType=\"{pml}.notesSlide+xml\"/>");
        }
    }
    for (c_num, _) in &charts {
        let _ = write!(
            ct,
            "<Override PartName=\"/ppt/charts/chart{c_num}.xml\" ContentType=\"{CHART_CT}\"/>"
        );
    }
    ct.push_str("</Types>");

    let notes_lst = if has_notes {
        "<p:notesMasterIdLst><p:notesMasterId r:id=\"rIdNotesMaster1\"/></p:notesMasterIdLst>"
    } else {
        ""
    };
    let mut pres_xml = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:presentation {PML_NS}><p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rIdMaster1\"/></p:sldMasterIdLst>{notes_lst}<p:sldIdLst>");
    let mut pres_rels: Vec<(String, &'static str, String)> =
        Vec::with_capacity(deck.slides.len() + 3);
    for idx in 0..deck.slides.len() {
        let (n, rid) = (idx + 1, format!("rId{}", idx + 1));
        let _ = write!(pres_xml, "<p:sldId id=\"{}\" r:id=\"{rid}\"/>", 255 + n);
        pres_rels.push((rid, "slide", format!("slides/slide{n}.xml")));
    }
    let sz_type = if deck.aspect == SlideAspect::Standard4x3
        && deck.orientation == SlideOrientation::Landscape
    {
        " type=\"screen4x3\""
    } else {
        ""
    };
    let default_tx = presentation_default_text_style_xml();
    let _ = write!(
        pres_xml,
        "</p:sldIdLst><p:sldSz cx=\"{emu_w}\" cy=\"{emu_h}\"{sz_type}/><p:notesSz cx=\"6858000\" cy=\"9144000\"/>{default_tx}</p:presentation>"
    );
    pres_rels.extend([
        (
            "rIdMaster1".into(),
            "slideMaster",
            "slideMasters/slideMaster1.xml".into(),
        ),
        ("rIdTheme1".into(), "theme", "theme/theme1.xml".into()),
        ("rIdPresProps1".into(), "presProps", "presProps.xml".into()),
    ]);
    if has_view {
        pres_rels.push(("rIdViewProps1".into(), "viewProps", "viewProps.xml".into()));
    }
    if has_notes {
        pres_rels.push((
            "rIdNotesMaster1".into(),
            "notesMaster",
            "notesMasters/notesMaster1.xml".into(),
        ));
    }

    let mut layout_id_entries = String::new();
    let mut master_rels: Vec<(String, &'static str, String)> = Vec::new();
    for layout in SlideMasterLayout::ALL {
        let k = layout.layout_index();
        let _ = write!(
            layout_id_entries,
            "<p:sldLayoutId id=\"{}\" r:id=\"rId{k}\"/>",
            2_147_483_648u64 + k as u64
        );
        master_rels.push((
            format!("rId{k}"),
            "slideLayout",
            format!("../slideLayouts/slideLayout{k}.xml"),
        ));
    }
    let master_xml = build_slide_master_xml(emu_w, emu_h, &layout_id_entries);
    master_rels.push(("rId6".into(), "theme", "../theme/theme1.xml".into()));

    let mut entries: Vec<(String, Vec<u8>)> = vec![
        ("[Content_Types].xml".into(), ct.into_bytes()),
        (
            "_rels/.rels".into(),
            rels_xml(&[
                ("rId1", "officeDocument", "ppt/presentation.xml"),
                ("rId2", CORE_PROPERTIES_REL_TYPE, "docProps/core.xml"),
            ]),
        ),
        (
            "docProps/core.xml".into(),
            core_properties_xml(&deck.title).into_bytes(),
        ),
        ("ppt/presentation.xml".into(), pres_xml.into_bytes()),
        (
            "ppt/_rels/presentation.xml.rels".into(),
            rels_xml(&pres_rels),
        ),
        (
            "ppt/slideMasters/slideMaster1.xml".into(),
            master_xml.into_bytes(),
        ),
        (
            "ppt/slideMasters/_rels/slideMaster1.xml.rels".into(),
            rels_xml(&master_rels),
        ),
        (
            "ppt/theme/theme1.xml".into(),
            theme_xml("EasyOffice", &deck.settings).into_bytes(),
        ),
        (
            PRES_PROPS_PATH.into(),
            pres_props_xml(&deck.settings).into_bytes(),
        ),
    ];
    if has_view {
        entries.push((
            VIEW_PROPS_PATH.into(),
            view_props_xml(&deck.settings).into_bytes(),
        ));
    }
    let layout_rels = rels_xml(&[("rId1", "slideMaster", "../slideMasters/slideMaster1.xml")]);
    for layout in SlideMasterLayout::ALL {
        let k = layout.layout_index();
        let l_xml = build_slide_layout_xml(layout, emu_w, emu_h);
        entries.push((
            format!("ppt/slideLayouts/slideLayout{k}.xml"),
            l_xml.into_bytes(),
        ));
        entries.push((
            format!("ppt/slideLayouts/_rels/slideLayout{k}.xml.rels"),
            layout_rels.clone(),
        ));
    }
    if has_notes {
        let nm = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:notesMaster {PML_NS}><p:cSld><p:spTree>{PML_SP_TREE_HEADER}</p:spTree></p:cSld>{PML_CLR_MAP}</p:notesMaster>");
        entries.push(("ppt/notesMasters/notesMaster1.xml".into(), nm.into_bytes()));
        entries.push((
            "ppt/notesMasters/_rels/notesMaster1.xml.rels".into(),
            rels_xml(&[("rId1", "theme", "../theme/theme2.xml")]),
        ));
        entries.push((
            "ppt/theme/theme2.xml".into(),
            drawingml_theme_xml("EasyOffice Notes").into_bytes(),
        ));
    }
    entries.extend(slide_entries);
    entries.extend(media.into_entries());
    for (c, x) in charts {
        entries.push((format!("ppt/charts/chart{c}.xml"), x.into_bytes()));
    }
    for (path, data) in &entries {
        if path.ends_with(".xml") || path.ends_with(".rels") {
            let xml = std::str::from_utf8(data)
                .map_err(|_| EasyOfficeError::InvalidUtf8(format!("{path} UTF-8 오류")))?;
            XmlSanitizer::validate_safe_xml(xml)?;
        }
    }
    ZipArchiveCodec::pack_compressed(&entries)
}

fn push_chart_pts(
    xml: &mut String,
    tag: &str,
    fref: &str,
    num: bool,
    vals: impl Iterator<Item = String>,
    len: usize,
) {
    let (rf, ch, fmt) = if num {
        (
            "c:numRef",
            "c:numCache",
            "<c:formatCode>General</c:formatCode>",
        )
    } else {
        ("c:strRef", "c:strCache", "")
    };
    let _ = write!(
        xml,
        "<{tag}><{rf}><c:f>{fref}</c:f><{ch}>{fmt}<c:ptCount val=\"{len}\"/>"
    );
    for (i, v) in vals.enumerate() {
        let _ = write!(xml, "<c:pt idx=\"{i}\"><c:v>{v}</c:v></c:pt>");
    }
    let _ = write!(xml, "</{ch}></{rf}></{tag}>");
}

pub(crate) fn build_slide_chart_xml(
    chart_type: ChartKind,
    title: &str,
    categories: &[String],
    series: &[SlideChartSeries],
    legend: ChartLegendPos,
) -> String {
    let mut xml = String::with_capacity(2_048);
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><c:lang val=\"ko-KR\"/><c:chart>");
    if title.is_empty() {
        xml.push_str("<c:autoTitleDeleted val=\"1\"/>");
    } else {
        let _ = write!(xml, "<c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr/></a:pPr><a:r><a:t>{}</a:t></a:r></a:p></c:rich></c:tx><c:overlay val=\"0\"/></c:title><c:autoTitleDeleted val=\"0\"/>", XmlSanitizer::escape(title));
    }
    xml.push_str("<c:plotArea><c:layout/>");
    let scatter = chart_type.has_numeric_x();
    let (cat_tag, val_tag) = if scatter {
        ("c:xVal", "c:yVal")
    } else {
        ("c:cat", "c:val")
    };
    let num_cats = scatter
        && !categories.is_empty()
        && categories
            .iter()
            .all(|c| c.trim().parse::<f64>().is_ok_and(f64::is_finite));
    let mut sers: Vec<String> = Vec::new();
    // A bubble chart reads its series in pairs: the Y values, then the sizes of the bubbles (PD-402).
    let step = if chart_type == ChartKind::Bubble {
        2
    } else {
        1
    };
    for (idx, pair) in series.chunks(step).enumerate() {
        let ser = &pair[0];
        let mut ser_xml = String::new();
        let _ = write!(
            ser_xml,
            "<c:ser><c:idx val=\"{idx}\"/><c:order val=\"{idx}\"/><c:tx><c:v>{}</c:v></c:tx>",
            XmlSanitizer::escape(&ser.name)
        );
        if chart_type == ChartKind::Stock {
            ser_xml.push_str(STOCK_SERIES_LOOK);
        }
        if !categories.is_empty() {
            push_chart_pts(
                &mut ser_xml,
                cat_tag,
                "Chart!$A$1",
                num_cats,
                categories.iter().map(|c| {
                    if num_cats {
                        c.trim().to_string()
                    } else {
                        XmlSanitizer::escape(c)
                    }
                }),
                categories.len(),
            );
        }
        push_chart_pts(
            &mut ser_xml,
            val_tag,
            "Chart!$B$1",
            true,
            ser.values.iter().map(|v| v.to_string()),
            ser.values.len(),
        );
        if let Some(sizes) = pair.get(1).filter(|s| !s.values.is_empty()) {
            push_chart_pts(
                &mut ser_xml,
                "c:bubbleSize",
                "Chart!$C$1",
                true,
                sizes.values.iter().map(|v| v.to_string()),
                sizes.values.len(),
            );
        }
        ser_xml.push_str("</c:ser>");
        sers.push(ser_xml);
    }
    for group in plot_groups(chart_type, series.len()) {
        let _ = write!(xml, "<{}>{}", group.tag, group.header);
        sers[group.series.clone()]
            .iter()
            .for_each(|s| xml.push_str(s));
        xml.push_str(group.tail);
        if group.has_axes {
            let _ = write!(
                xml,
                "<c:axId val=\"{AX_ID_CAT}\"/><c:axId val=\"{AX_ID_VAL}\"/>"
            );
        }
        let _ = write!(xml, "</{}>", group.tag);
    }
    xml.push_str(&axes_xml(chart_type));
    xml.push_str("</c:plotArea>");
    if let Some(pos) = legend.ooxml_val() {
        let _ = write!(
            xml,
            "<c:legend><c:legendPos val=\"{pos}\"/><c:overlay val=\"0\"/></c:legend>"
        );
    }
    xml.push_str("<c:plotVisOnly val=\"1\"/></c:chart></c:chartSpace>");
    xml
}

pub(crate) type DecodedSlideChart = (
    ChartKind,
    String,
    Vec<String>,
    Vec<SlideChartSeries>,
    ChartLegendPos,
);

pub(crate) fn parse_slide_chart_xml(xml: &str) -> Result<DecodedSlideChart, EasyOfficeError> {
    use crate::xml_stream::local_name as loc;
    fn over() -> EasyOfficeError {
        EasyOfficeError::ResourceBudgetExceeded(format!(
            "슬라이드 차트 포인트 수({MAX_SLIDE_CHART_POINTS}) 초과"
        ))
    }
    let mut reader = XmlReader::with_budget(xml, MAX_CHART_XML_EVENTS);
    let (mut cd, mut pd, mut td, mut ld, mut bd, mut sd, mut x_d, mut d) =
        (0usize, 0, 0, 0, 0, 0, 0, 0);
    let (mut slot, mut tk, mut pt_i, mut pt_n) = ("", "", 0usize, 0usize);
    let (mut kind, mut title, mut legend) = (None, String::new(), ChartLegendPos::None);
    let mut s_sizes: Vec<(usize, f64)> = Vec::new();
    let (mut cats, mut series, mut buf, mut s_name, mut s_cats, mut s_vals) = (
        Vec::new(),
        Vec::new(),
        String::new(),
        String::new(),
        Vec::new(),
        Vec::new(),
    );

    while let Some(ev) = reader.next_event()? {
        match ev {
            XmlEvent::Start { name, attrs } => {
                d += 1;
                match loc(name) {
                    "chart" if cd == 0 => cd = d,
                    "plotArea" if cd > 0 && pd == 0 => pd = d,
                    "title" if cd > 0 && pd == 0 && td == 0 => td = d,
                    "legend" if cd > 0 && pd == 0 && ld == 0 => {
                        ld = d;
                        legend = ChartLegendPos::Right;
                    }
                    "legendPos" if ld > 0 => {
                        if let Some(v) = attr(&attrs, "val") {
                            legend = ChartLegendPos::from_ooxml(&v);
                        }
                    }
                    "t" | "v" if td > 0 => {
                        tk = "title";
                        buf.clear();
                    }
                    "barChart" | "bar3DChart" if pd > 0 && kind.is_none() => {
                        kind = Some(ChartKind::Column);
                        bd = d;
                    }
                    "barDir" if bd > 0 && attr(&attrs, "val").as_deref() == Some("bar") => {
                        kind = Some(ChartKind::Bar)
                    }
                    "lineChart" | "line3DChart" if pd > 0 && kind.is_none() => {
                        kind = Some(ChartKind::Line)
                    }
                    // A line group after a bar group with one series is a combo (PD-402); with more bar series the lines are
                    // read as bars, which is what a model without a kind per series can hold.
                    "lineChart" | "line3DChart"
                        if pd > 0 && kind == Some(ChartKind::Column) && series.len() == 1 =>
                    {
                        kind = Some(ChartKind::Combo)
                    }
                    "pieChart" | "pie3DChart" if pd > 0 && kind.is_none() => {
                        kind = Some(ChartKind::Pie)
                    }
                    "doughnutChart" if pd > 0 && kind.is_none() => kind = Some(ChartKind::Doughnut),
                    "radarChart" if pd > 0 && kind.is_none() => kind = Some(ChartKind::Radar),
                    "bubbleChart" if pd > 0 && kind.is_none() => kind = Some(ChartKind::Bubble),
                    "stockChart" if pd > 0 && kind.is_none() => kind = Some(ChartKind::Stock),
                    "scatterChart" if pd > 0 && kind.is_none() => kind = Some(ChartKind::Scatter),
                    "ser" if pd > 0 && sd == 0 => {
                        if series.len() >= MAX_SLIDE_CHART_SERIES {
                            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                                "슬라이드 차트 계열 수({MAX_SLIDE_CHART_SERIES}) 초과"
                            )));
                        }
                        sd = d;
                        s_name.clear();
                        s_cats.clear();
                        s_vals.clear();
                        s_sizes.clear();
                        slot = "";
                        x_d = 0;
                    }
                    tag @ ("tx" | "cat" | "xVal" | "val" | "yVal" | "bubbleSize")
                        if sd > 0 && d == sd + 1 && slot.is_empty() =>
                    {
                        slot = match tag {
                            "tx" => "tx",
                            "cat" | "xVal" => "cat",
                            "bubbleSize" => "size",
                            _ => "val",
                        };
                        x_d = d;
                        pt_n = 0;
                    }
                    "ptCount"
                        if !slot.is_empty()
                            && attr(&attrs, "val")
                                .and_then(|s| s.trim().parse::<usize>().ok())
                                .unwrap_or(0)
                                > MAX_SLIDE_CHART_POINTS =>
                    {
                        return Err(over())
                    }
                    "pt" if !slot.is_empty() => {
                        pt_n += 1;
                        if pt_n > MAX_SLIDE_CHART_POINTS {
                            return Err(over());
                        }
                        pt_i = attr(&attrs, "idx")
                            .and_then(|s| s.trim().parse::<usize>().ok())
                            .unwrap_or(pt_n - 1);
                    }
                    "v" | "t" if !slot.is_empty() => {
                        tk = "slot";
                        buf.clear();
                    }
                    _ => {}
                }
            }
            XmlEvent::Text(t) if !tk.is_empty() => buf.push_str(&t),
            XmlEvent::End(name) => {
                let l = loc(name);
                if tk == "title" && matches!(l, "t" | "v") {
                    tk = "";
                    title.push_str(&buf);
                    buf.clear();
                } else if tk == "slot" && matches!(l, "v" | "t") {
                    tk = "";
                    let v = buf.trim().to_string();
                    buf.clear();
                    match slot {
                        "tx" if s_name.is_empty() => s_name = v,
                        "cat" if pt_i < MAX_SLIDE_CHART_POINTS => s_cats.push((pt_i, v)),
                        "val" | "size" if pt_i < MAX_SLIDE_CHART_POINTS => {
                            let n = v.parse::<f64>().ok().filter(|x| x.is_finite()).ok_or_else(
                                || EasyOfficeError::InvalidArgument(format!("차트 수치 오류: {v}")),
                            )?;
                            if slot == "size" {
                                s_sizes.push((pt_i, n));
                            } else {
                                s_vals.push((pt_i, n));
                            }
                        }
                        _ => {}
                    }
                }
                if d == x_d {
                    slot = "";
                    x_d = 0;
                }
                if d == sd {
                    if cats.is_empty() && !s_cats.is_empty() {
                        cats.resize(
                            s_cats.iter().map(|(i, _)| *i).max().unwrap_or(0) + 1,
                            String::new(),
                        );
                        for (i, c) in s_cats.drain(..) {
                            cats[i] = c;
                        }
                    }
                    let mut values = s_vals
                        .iter()
                        .map(|(i, _)| *i)
                        .max()
                        .map_or_else(Vec::new, |m| vec![0.0; m + 1]);
                    for (i, n) in s_vals.drain(..) {
                        values[i] = n;
                    }
                    let name = std::mem::take(&mut s_name);
                    // The sizes of a bubble series are the next series of the model (see `ChartKind`).
                    // A bubble series with no `bubbleSize` still gets its (empty) size series, so the pairs of the model
                    // never take the next series' values for sizes.
                    let sizes =
                        (!s_sizes.is_empty() || kind == Some(ChartKind::Bubble)).then(|| {
                            let mut sizes =
                                vec![0.0; s_sizes.iter().map(|(i, _)| *i + 1).max().unwrap_or(0)];
                            for (i, n) in s_sizes.drain(..) {
                                sizes[i] = n;
                            }
                            SlideChartSeries {
                                name: format!("{name} (크기)"),
                                values: sizes,
                            }
                        });
                    series.push(SlideChartSeries { name, values });
                    if let Some(sizes) = sizes {
                        if series.len() >= MAX_SLIDE_CHART_SERIES {
                            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                                "슬라이드 차트 계열 수({MAX_SLIDE_CHART_SERIES}) 초과"
                            )));
                        }
                        series.push(sizes);
                    }
                    sd = 0;
                }
                for dp in [&mut bd, &mut td, &mut ld, &mut pd, &mut cd] {
                    if *dp == d {
                        *dp = 0;
                    }
                }
                d = d.saturating_sub(1);
            }
            _ => {}
        }
    }
    let kind = kind.ok_or_else(|| {
        EasyOfficeError::CorruptedContainer("PPTX 차트 유형을 판별할 수 없습니다.".to_string())
    })?;
    Ok((kind, title, cats, series, legend))
}

impl OoxmlCodec {
    pub fn pack_pptx(payload: &OoxmlPptxPayload) -> Result<Vec<u8>, EasyOfficeError> {
        let deck = RichSlideDeck::from_legacy_slides(
            &payload.title,
            payload.is_slideshow,
            payload.slides.clone(),
        );
        crate::pptx_rich::pack_pptx_rich(&deck, payload.is_slideshow)
    }

    pub(crate) fn ordered_pptx_slides<'a>(
        pres_xml: &str,
        entries: &'a [ZipArchiveEntry],
        entry_map: &std::collections::HashMap<&str, &'a ZipArchiveEntry>,
        rel_targets: &std::collections::HashMap<String, String>,
    ) -> Result<Vec<&'a ZipArchiveEntry>, EasyOfficeError> {
        let (mut ordered, mut seen, mut cnt) = (Vec::new(), HashSet::new(), 0usize);
        for (attrs, _) in extract_balanced_elements(pres_xml, "p:sldId") {
            cnt += 1;
            if cnt > MAX_PPTX_SLIDES * 4 {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "PPTX 슬라이드 참조 수 한도를 초과했습니다.".to_string(),
                ));
            }
            let rid = extract_attr_from_fragment(attrs, "r:id")
                .or_else(|| extract_attr_from_fragment(attrs, "id"));
            if let Some(&entry) = rid
                .and_then(|r| rel_targets.get(r))
                .and_then(|p| entry_map.get(p.as_str()))
            {
                if seen.insert(entry.path.as_str()) {
                    if ordered.len() >= MAX_PPTX_SLIDES {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(
                            "PPTX 슬라이드 수 한도를 초과했습니다.".to_string(),
                        ));
                    }
                    ordered.push(entry);
                }
            }
        }
        if ordered.is_empty() {
            ordered = Self::pptx_slides_by_number(entries);
            if ordered.len() > MAX_PPTX_SLIDES {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "PPTX 슬라이드 수 한도를 초과했습니다.".to_string(),
                ));
            }
        }
        Ok(ordered)
    }

    /// Presentation title: core properties first, then the root attributes, then a default.
    pub(crate) fn pptx_title(
        entry_map: &std::collections::HashMap<&str, &ZipArchiveEntry>,
        pres_xml: &str,
    ) -> String {
        let core_title = entry_map
            .get("docProps/core.xml")
            .and_then(|e| std::str::from_utf8(&e.data).ok())
            .and_then(|xml| extract_tag_bodies(xml, "dc:title").into_iter().next())
            .map(XmlSanitizer::unescape)
            .filter(|s| !s.is_empty());
        core_title
            .or_else(|| {
                extract_attr(pres_xml, "<p:presentation", "title")
                    .or_else(|| extract_attr(pres_xml, "<p:presentation", "name"))
                    .map(XmlSanitizer::unescape)
            })
            .unwrap_or_else(|| "발표자료".to_string())
    }

    pub(crate) fn pptx_slides_by_number(entries: &[ZipArchiveEntry]) -> Vec<&ZipArchiveEntry> {
        let slide_num = |path: &str| {
            path.strip_prefix("ppt/slides/slide")
                .and_then(|s| s.strip_suffix(".xml"))
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(usize::MAX)
        };
        let mut slide_entries: Vec<_> = entries
            .iter()
            .filter(|e| e.path.starts_with("ppt/slides/slide") && e.path.ends_with(".xml"))
            .collect();
        slide_entries.sort_by(|a, b| {
            slide_num(&a.path)
                .cmp(&slide_num(&b.path))
                .then_with(|| a.path.cmp(&b.path))
        });
        slide_entries
    }

    pub fn unpack_pptx(bytes: &[u8]) -> Result<OoxmlPptxPayload, EasyOfficeError> {
        let deck = crate::pptx_rich::unpack_pptx_rich(bytes)?;
        let slides = deck.to_legacy_slides();
        Ok(OoxmlPptxPayload {
            title: deck.title,
            is_slideshow: deck.is_slideshow,
            slides,
        })
    }

    pub fn pack_pptx_rich(
        deck: &crate::rich_slide::RichSlideDeck,
        is_slideshow: bool,
    ) -> Result<Vec<u8>, EasyOfficeError> {
        crate::pptx_rich::pack_pptx_rich(deck, is_slideshow)
    }

    pub fn unpack_pptx_rich(
        bytes: &[u8],
    ) -> Result<crate::rich_slide::RichSlideDeck, EasyOfficeError> {
        crate::pptx_rich::unpack_pptx_rich(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ZipSecurityPolicy;

    #[test]
    fn pptx_duplicate_slide_targets_are_deduplicated_and_hostile_amplification_fails_closed() {
        // 1. Duplicate rId target referenced N times must yield only 1 slide, not N copies.
        let base = OoxmlCodec::pack_pptx(&OoxmlPptxPayload {
            title: "dedup".into(),
            is_slideshow: false,
            slides: vec![("첫 슬라이드".into(), vec!["내용".into()], "노트".into())],
        })
        .unwrap();
        let mut entries: Vec<(String, Vec<u8>)> =
            ZipArchiveCodec::unpack(&base, &ZipSecurityPolicy::default())
                .unwrap()
                .into_iter()
                .map(|e| (e.path, e.data))
                .collect();
        let repeated_ids = (1..=500)
            .map(|i| format!("<p:sldId id=\"{}\" r:id=\"rId2\"/>", 256 + i))
            .collect::<String>();
        let dup_rels = rels_xml(&[("rId2", "slide", "slides/slide1.xml")]);
        let mut found_rels = false;
        for (path, data) in &mut entries {
            if path == "ppt/presentation.xml" {
                *data = format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:presentation {PML_NS}><p:sldIdLst>{repeated_ids}</p:sldIdLst></p:presentation>"
                )
                .into_bytes();
            } else if path == "ppt/_rels/presentation.xml.rels" {
                *data = dup_rels.clone();
                found_rels = true;
            }
        }
        if !found_rels {
            entries.push(("ppt/_rels/presentation.xml.rels".into(), dup_rels));
        }
        let dup_bytes = ZipArchiveCodec::pack(&entries).unwrap();
        assert_eq!(OoxmlCodec::unpack_pptx(&dup_bytes).unwrap().slides.len(), 1);

        let build_pkg = |count: usize, with_notes: bool| -> Vec<u8> {
            let mut ids = String::new();
            let mut rels = Vec::with_capacity(count);
            let mut entries = vec![(
                "[Content_Types].xml".into(),
                b"<?xml version=\"1.0\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"></Types>".to_vec(),
            )];
            if with_notes {
                let txt = "A".repeat(300_000);
                entries.push(("ppt/notesSlides/notesSlide1.xml".into(), format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:notes {PML_NS}><p:cSld><p:spTree><p:sp><p:nvSpPr><p:nvPr><p:ph type=\"body\" idx=\"1\"/></p:nvPr></p:nvSpPr><p:txBody><a:p><a:r><a:t>{txt}</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:notes>"
                ).into_bytes()));
            }
            let note_rel = rels_xml(&[("rId1", "notesSlide", "../notesSlides/notesSlide1.xml")]);
            for i in 1..=count {
                let rid = format!("rId{i}");
                entries.push((format!("ppt/slides/slide{i}.xml"), b"<p:sld xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:cSld><p:spTree/></p:cSld></p:sld>".to_vec()));
                if with_notes {
                    entries.push((
                        format!("ppt/slides/_rels/slide{i}.xml.rels"),
                        note_rel.clone(),
                    ));
                }
                let _ = write!(ids, "<p:sldId id=\"{}\" r:id=\"{rid}\"/>", 256 + i);
                rels.push((rid, "slide", format!("slides/slide{i}.xml")));
            }
            entries.push(("ppt/_rels/presentation.xml.rels".into(), rels_xml(&rels)));
            entries.push((
                "ppt/presentation.xml".into(),
                format!("<p:presentation {PML_NS}><p:sldIdLst>{ids}</p:sldIdLst></p:presentation>")
                    .into_bytes(),
            ));
            ZipArchiveCodec::pack(&entries).unwrap()
        };

        // 2. Excess slide count (> MAX_PPTX_SLIDES) fails closed.
        let over_bytes = build_pkg(MAX_PPTX_SLIDES + 1, false);
        assert!(matches!(
            OoxmlCodec::unpack_pptx(&over_bytes),
            Err(EasyOfficeError::ResourceBudgetExceeded(_))
        ));

        // 3. Shared giant notesSlide referenced by many unique small slides trips DecodeBudget.
        let amp_bytes = build_pkg(950, true);
        assert!(matches!(
            OoxmlCodec::unpack_pptx(&amp_bytes),
            Err(EasyOfficeError::ResourceBudgetExceeded(_))
        ));
    }
}

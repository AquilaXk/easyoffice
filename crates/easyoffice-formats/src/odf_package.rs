//! ODF package extraction, manifest generation, and `.odp` presentation codec helpers.

use std::fmt::Write;

use crate::odf_hwpx_codecs::{OdfCodec, OdpUnpacked};
use crate::office_codecs::validate_zip_xml_entries_for;
use crate::pptx_master::odp_layout_boxes_pt;
use crate::rich_image_writers::MediaParts;
use crate::rich_slide::{
    parse_length_pt, RichSlideDeck, SlideAspect, SlideChartSeries, SlideMasterLayout,
    MAX_SLIDE_CHART_POINTS, MAX_SLIDE_CHART_SERIES,
};
use crate::rtf_xml_helpers::{
    extract_attr, extract_attr_from_fragment, extract_balanced_elements,
    extract_odf_blocks_in_order, extract_odf_inline_text, extract_tag_bodies, strip_tag_blocks,
};
use crate::sheet_chart::{ChartKind, ChartLegendPos};
use crate::sheet_styling::ods_view::read_views;
use crate::sheet_styling::SheetStyling;
use crate::xml_stream::{attr, XmlEvent, XmlReader};
use crate::{
    ContainerFamily, DocumentFormat, EasyOfficeError, XmlSanitizer, ZipArchiveCodec,
    ZipSecurityPolicy,
};

const MAX_ODP_CHART_EVENTS: usize = 65_536;
const ODF_NS_DECLS: &str = concat!(
    "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
    "xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" ",
    "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" ",
    "xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" ",
    "xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" ",
    "xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" ",
    "xmlns:xlink=\"http://www.w3.org/1999/xlink\" ",
    "xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" ",
    "xmlns:chart=\"urn:oasis:names:tc:opendocument:xmlns:chart:1.0\""
);

#[inline]
fn local_xml(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

pub(crate) fn build_odp_chart_xml(
    chart_type: ChartKind,
    title: &str,
    categories: &[String],
    series: &[SlideChartSeries],
    legend: ChartLegendPos,
) -> String {
    let class = match chart_type {
        ChartKind::Column | ChartKind::Bar => "chart:bar",
        ChartKind::Line => "chart:line",
        ChartKind::Pie => "chart:circle",
        ChartKind::Scatter => "chart:scatter",
    };
    let vertical = chart_type == ChartKind::Bar;
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-content {ODF_NS_DECLS} office:version=\"1.2\"><office:automatic-styles><style:style style:name=\"ch-plot\" style:family=\"chart\"><style:chart-properties chart:vertical=\"{vertical}\"/></style:style></office:automatic-styles><office:body><office:chart><chart:chart chart:class=\"{class}\">"
    );
    if !title.is_empty() {
        let _ = write!(
            xml,
            "<chart:title><text:p>{}</text:p></chart:title>",
            XmlSanitizer::escape(title)
        );
    }
    if let Some(pos) = match legend {
        ChartLegendPos::None => None,
        ChartLegendPos::Right => Some("end"),
        ChartLegendPos::Bottom => Some("bottom"),
        ChartLegendPos::Top => Some("top"),
        ChartLegendPos::Left => Some("start"),
    } {
        let _ = write!(xml, "<chart:legend chart:legend-position=\"{pos}\"/>");
    }
    xml.push_str("<chart:plot-area chart:style-name=\"ch-plot\">");
    for (s_idx, s) in series.iter().enumerate() {
        let col = char::from_u32(b'B' as u32 + s_idx as u32).unwrap_or('B');
        let n_rows = categories.len().max(s.values.len()).max(1) + 1;
        let _ = write!(xml, "<chart:series chart:values-cell-range-address=\"local-table.{col}2:local-table.{col}{n_rows}\" chart:label-cell-address=\"local-table.{col}1\"/>");
    }
    xml.push_str("</chart:plot-area><table:table table:name=\"local-table\"><table:table-header-rows><table:table-row><table:table-cell/>");
    for s in series {
        let _ = write!(
            xml,
            "<table:table-cell office:value-type=\"string\"><text:p>{}</text:p></table:table-cell>",
            XmlSanitizer::escape(&s.name)
        );
    }
    xml.push_str("</table:table-row></table:table-header-rows><table:table-rows>");
    let row_count = categories
        .len()
        .max(series.iter().map(|s| s.values.len()).max().unwrap_or(0));
    for r in 0..row_count {
        let cat = categories.get(r).map_or("", String::as_str);
        let _ = write!(xml, "<table:table-row><table:table-cell office:value-type=\"string\"><text:p>{}</text:p></table:table-cell>", XmlSanitizer::escape(cat));
        for s in series {
            let v = s.values.get(r).copied().unwrap_or(0.0);
            let _ = write!(xml, "<table:table-cell office:value-type=\"float\" office:value=\"{v}\"><text:p>{v}</text:p></table:table-cell>");
        }
        xml.push_str("</table:table-row>");
    }
    xml.push_str("</table:table-rows></table:table></chart:chart></office:chart></office:body></office:document-content>");
    xml
}

pub(crate) type ParsedSlideChart = (
    ChartKind,
    String,
    Vec<String>,
    Vec<SlideChartSeries>,
    ChartLegendPos,
);

pub(crate) fn parse_odp_chart_xml(xml: &str) -> Result<ParsedSlideChart, EasyOfficeError> {
    let mut reader = XmlReader::with_budget(xml, MAX_ODP_CHART_EVENTS);
    let (mut in_title, mut in_table, mut in_hdr, mut in_row, mut in_cell, mut in_p) =
        (false, false, false, false, false, false);
    let (mut kind_class, mut vertical) = (String::new(), false);
    let (mut title, mut legend) = (String::new(), ChartLegendPos::None);
    let (mut cell_val, mut cell_buf) = (None::<String>, String::new());
    let (mut cur_row, mut header_names, mut data_rows) = (Vec::new(), Vec::new(), Vec::new());

    while let Some(ev) = reader.next_event()? {
        match ev {
            XmlEvent::Start { name, attrs } => match local_xml(name) {
                "chart-properties" => {
                    if attr(&attrs, "chart:vertical").as_deref() == Some("true")
                        || attr(&attrs, "vertical").as_deref() == Some("true")
                    {
                        vertical = true;
                    }
                }
                "chart" if kind_class.is_empty() => {
                    if let Some(c) = attr(&attrs, "chart:class").or_else(|| attr(&attrs, "class")) {
                        kind_class = c.into_owned();
                    }
                }
                "title" => in_title = true,
                "legend" => {
                    let pos = attr(&attrs, "chart:legend-position")
                        .or_else(|| attr(&attrs, "legend-position"));
                    legend = match pos.as_deref() {
                        Some("start" | "left") => ChartLegendPos::Left,
                        Some("top") => ChartLegendPos::Top,
                        Some("bottom") => ChartLegendPos::Bottom,
                        _ => ChartLegendPos::Right,
                    };
                }
                "table" => in_table = true,
                "table-header-rows" if in_table => in_hdr = true,
                "table-row" if in_table => {
                    in_row = true;
                    cur_row.clear();
                }
                "table-cell" if in_row => {
                    in_cell = true;
                    cell_val = attr(&attrs, "office:value")
                        .or_else(|| attr(&attrs, "value"))
                        .map(|v| v.into_owned());
                    cell_buf.clear();
                }
                "p" if in_title || in_cell => in_p = true,
                _ => {}
            },
            XmlEvent::Text(t) if in_p => {
                if in_title {
                    title.push_str(&t);
                } else if in_cell {
                    cell_buf.push_str(&t);
                }
            }
            XmlEvent::End(name) => match local_xml(name) {
                "p" => in_p = false,
                "title" => in_title = false,
                "table-cell" if in_cell => {
                    in_cell = false;
                    let text = cell_val
                        .take()
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or_else(|| cell_buf.trim().to_string());
                    if cur_row.len() <= MAX_SLIDE_CHART_SERIES + 1 {
                        cur_row.push(text);
                    }
                }
                "table-row" if in_row => {
                    in_row = false;
                    let row = std::mem::take(&mut cur_row);
                    if in_hdr && header_names.is_empty() {
                        header_names = row.into_iter().skip(1).collect();
                    } else if !in_hdr || !header_names.is_empty() {
                        if data_rows.len() >= MAX_SLIDE_CHART_POINTS {
                            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                                "ODP 차트 포인트 수 상한({MAX_SLIDE_CHART_POINTS}) 초과"
                            )));
                        }
                        data_rows.push(row);
                    }
                }
                "table-header-rows" => in_hdr = false,
                "table" => in_table = false,
                _ => {}
            },
            _ => {}
        }
    }

    let chart_type = match kind_class.as_str() {
        c if c.ends_with("bar") => {
            if vertical {
                ChartKind::Bar
            } else {
                ChartKind::Column
            }
        }
        c if c.ends_with("line") | c.ends_with("area") => ChartKind::Line,
        c if c.ends_with("circle") | c.ends_with("ring") => ChartKind::Pie,
        c if c.ends_with("scatter") => ChartKind::Scatter,
        _ => {
            return Err(EasyOfficeError::CorruptedContainer(format!(
                "ODP 차트 유형을 판별할 수 없습니다: {kind_class}"
            )))
        }
    };

    let n_series = header_names.len().max(
        data_rows
            .iter()
            .map(|r| r.len().saturating_sub(1))
            .max()
            .unwrap_or(0),
    );
    if n_series > MAX_SLIDE_CHART_SERIES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "ODP 차트 계열 수 상한({MAX_SLIDE_CHART_SERIES}) 초과"
        )));
    }
    let mut categories = Vec::with_capacity(data_rows.len());
    let mut series_vals: Vec<Vec<f64>> = vec![Vec::with_capacity(data_rows.len()); n_series];
    for row in &data_rows {
        categories.push(row.first().cloned().unwrap_or_default());
        for (i, vals) in series_vals.iter_mut().enumerate() {
            let raw = row.get(i + 1).map_or("0", String::as_str).trim();
            let v = if raw.is_empty() {
                0.0
            } else {
                raw.parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite())
                    .ok_or_else(|| {
                        EasyOfficeError::InvalidArgument(format!(
                            "ODP 차트 수치가 올바르지 않습니다: {raw}"
                        ))
                    })?
            };
            vals.push(v);
        }
    }
    let series = series_vals
        .into_iter()
        .enumerate()
        .map(|(i, values)| SlideChartSeries {
            name: header_names
                .get(i)
                .cloned()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| format!("계열 {}", i + 1)),
            values,
        })
        .collect();
    Ok((chart_type, title, categories, series, legend))
}

pub(crate) fn finish_odp_package(
    deck: &RichSlideDeck,
    content_xml: String,
    media: MediaParts,
    charts: Vec<(usize, String)>,
) -> Result<Vec<u8>, EasyOfficeError> {
    let (w_cm, h_cm) = deck.aspect.dimensions_cm();
    let mut styles = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<office:document-styles ",
        "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
        "xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" ",
        "xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" ",
        "xmlns:presentation=\"urn:oasis:names:tc:opendocument:xmlns:presentation:1.0\" ",
        "xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" ",
        "xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" ",
        "office:version=\"1.2\">",
        "<office:font-face-decls>",
        "<style:font-face style:name=\"맑은 고딕\" svg:font-family=\"&apos;맑은 고딕&apos;\" style:font-family-generic=\"swiss\"/>",
        "</office:font-face-decls>",
        "<office:styles>",
        "<style:default-style style:family=\"graphic\">",
        "<style:graphic-properties svg:stroke-color=\"#808080\" draw:fill=\"solid\" draw:fill-color=\"#ffffff\"/>",
        "<style:paragraph-properties fo:text-align=\"start\" style:writing-mode=\"lr-tb\"/>",
        "<style:text-properties style:font-name=\"맑은 고딕\" style:font-name-asian=\"맑은 고딕\" fo:font-size=\"18pt\" style:font-size-asian=\"18pt\" fo:language=\"ko\" fo:country=\"KR\" style:language-asian=\"ko\" style:country-asian=\"KR\"/>",
        "</style:default-style>",
        "<style:default-style style:family=\"paragraph\">",
        "<style:paragraph-properties fo:text-align=\"start\" style:writing-mode=\"lr-tb\"/>",
        "<style:text-properties style:font-name=\"맑은 고딕\" style:font-name-asian=\"맑은 고딕\" fo:font-size=\"18pt\" style:font-size-asian=\"18pt\" fo:language=\"ko\" fo:country=\"KR\" style:language-asian=\"ko\" style:country-asian=\"KR\"/>",
        "</style:default-style>",
        "<style:style style:name=\"Standard-title\" style:family=\"presentation\"><style:text-properties style:font-name=\"맑은 고딕\" style:font-name-asian=\"맑은 고딕\" fo:font-size=\"44pt\" style:font-size-asian=\"44pt\"/></style:style>",
        "<style:style style:name=\"Standard-subtitle\" style:family=\"presentation\"><style:text-properties style:font-name=\"맑은 고딕\" style:font-name-asian=\"맑은 고딕\" fo:font-size=\"24pt\" style:font-size-asian=\"24pt\"/></style:style>",
        "<style:style style:name=\"Standard-outline1\" style:family=\"presentation\"><style:text-properties style:font-name=\"맑은 고딕\" style:font-name-asian=\"맑은 고딕\" fo:font-size=\"28pt\" style:font-size-asian=\"28pt\"/></style:style>"
    ).to_string();
    for layout in SlideMasterLayout::ALL {
        let name = layout.as_str();
        if layout == SlideMasterLayout::Blank {
            let _ = write!(
                styles,
                "<style:presentation-page-layout style:name=\"eo_{name}\"/>"
            );
            continue;
        }
        let (title_box, body_box, right_box) = odp_layout_boxes_pt(layout, deck.aspect);
        let body_obj = if layout == SlideMasterLayout::TitleSlide {
            "subtitle"
        } else {
            "outline"
        };
        let _ = write!(
            styles,
            "<style:presentation-page-layout style:name=\"eo_{name}\"><presentation:placeholder presentation:object=\"title\" svg:x=\"{:.3}pt\" svg:y=\"{:.3}pt\" svg:width=\"{:.3}pt\" svg:height=\"{:.3}pt\"/><presentation:placeholder presentation:object=\"{body_obj}\" svg:x=\"{:.3}pt\" svg:y=\"{:.3}pt\" svg:width=\"{:.3}pt\" svg:height=\"{:.3}pt\"/>",
            title_box.x, title_box.y, title_box.w, title_box.h,
            body_box.x, body_box.y, body_box.w, body_box.h
        );
        if let Some(rb) = right_box {
            let _ = write!(
                styles,
                "<presentation:placeholder presentation:object=\"outline\" svg:x=\"{:.3}pt\" svg:y=\"{:.3}pt\" svg:width=\"{:.3}pt\" svg:height=\"{:.3}pt\"/>",
                rb.x, rb.y, rb.w, rb.h
            );
        }
        styles.push_str("</style:presentation-page-layout>");
    }
    let _ = write!(
        styles,
        "</office:styles><office:automatic-styles><style:page-layout style:name=\"PM1\"><style:page-layout-properties fo:margin-top=\"0cm\" fo:margin-bottom=\"0cm\" fo:margin-left=\"0cm\" fo:margin-right=\"0cm\" fo:page-width=\"{w_cm:.3}cm\" fo:page-height=\"{h_cm:.3}cm\" style:print-orientation=\"landscape\"/></style:page-layout></office:automatic-styles><office:master-styles><style:master-page style:name=\"Standard\" style:page-layout-name=\"PM1\"/></office:master-styles></office:document-styles>"
    );
    let mut manifest_items: Vec<(String, &str)> = vec![
        ("styles.xml".into(), "text/xml"),
        ("meta.xml".into(), "text/xml"),
    ];
    manifest_items.extend(media.listing());
    for (c_num, _) in &charts {
        manifest_items.push((
            format!("Object {c_num}/"),
            "application/vnd.oasis.opendocument.chart",
        ));
        manifest_items.push((format!("Object {c_num}/content.xml"), "text/xml"));
    }
    let manifest = OdfCodec::build_manifest_with_media(OdfCodec::MIMETYPE_ODP, &manifest_items);
    let meta = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-meta xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" office:version=\"1.2\"><office:meta><dc:title>{}</dc:title></office:meta></office:document-meta>",
        XmlSanitizer::escape(&deck.title)
    );
    let mut entries: Vec<(String, Vec<u8>)> = vec![
        (
            "mimetype".into(),
            OdfCodec::MIMETYPE_ODP.as_bytes().to_vec(),
        ),
        ("META-INF/manifest.xml".into(), manifest.into_bytes()),
        ("content.xml".into(), content_xml.into_bytes()),
        ("styles.xml".into(), styles.into_bytes()),
        ("meta.xml".into(), meta.into_bytes()),
    ];
    entries.extend(media.into_entries());
    for (c_num, c_xml) in charts {
        entries.push((format!("Object {c_num}/content.xml"), c_xml.into_bytes()));
    }
    for (path, data) in &entries {
        if path.ends_with(".xml") {
            let xml = std::str::from_utf8(data)
                .map_err(|_| EasyOfficeError::InvalidUtf8(format!("{path} UTF-8 오류")))?;
            XmlSanitizer::validate_safe_xml(xml)?;
        }
    }
    ZipArchiveCodec::pack_compressed(&entries)
}

pub(crate) type ExtractedOdp = (
    Vec<crate::ZipArchiveEntry>,
    String,
    Option<String>,
    Option<String>,
);

pub(crate) fn extract_odp_rich_parts(bytes: &[u8]) -> Result<ExtractedOdp, EasyOfficeError> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())?;
    validate_zip_xml_entries_for(&entries, Some(DocumentFormat::Odp))?;
    if let Some(mime_e) = entries.iter().find(|e| e.path == "mimetype") {
        let actual = std::str::from_utf8(&mime_e.data).unwrap_or_default().trim();
        if actual != OdfCodec::MIMETYPE_ODP {
            return Err(EasyOfficeError::SignatureMismatch {
                expected: ContainerFamily::ZipOdf,
                detail: format!(
                    "ODF mimetype 불일치 (expected: {}, actual: {actual})",
                    OdfCodec::MIMETYPE_ODP
                ),
            });
        }
    }
    let read_opt_xml = |path: &str| -> Result<Option<String>, EasyOfficeError> {
        let Some(e) = entries.iter().find(|e| e.path == path) else {
            return Ok(None);
        };
        let xml = std::str::from_utf8(&e.data)
            .map_err(|_| EasyOfficeError::InvalidUtf8(format!("ODF {path} UTF-8 오류")))?;
        XmlSanitizer::validate_safe_xml(xml)?;
        Ok(Some(xml.to_string()))
    };
    let content_xml = read_opt_xml("content.xml")?.ok_or_else(|| {
        EasyOfficeError::CorruptedContainer(
            "ODF 필수 엔트리 'content.xml'이 누락되었습니다.".to_string(),
        )
    })?;
    let meta_xml = read_opt_xml("meta.xml")?;
    let styles_xml = read_opt_xml("styles.xml")?;
    Ok((entries, content_xml, meta_xml, styles_xml))
}

pub(crate) fn parse_odp_aspect(styles_xml: Option<&str>) -> (SlideAspect, f64, f64) {
    let Some(xml) = styles_xml else {
        return (SlideAspect::Widescreen16x9, 1.0, 1.0);
    };
    let mut r = XmlReader::with_budget(xml, 32_768);
    while let Ok(Some(ev)) = r.next_event() {
        if let XmlEvent::Start { name, attrs } = ev {
            if local_xml(name) == "page-layout-properties" {
                let w = attr(&attrs, "fo:page-width")
                    .or_else(|| attr(&attrs, "page-width"))
                    .and_then(|s| parse_length_pt(&s));
                let h = attr(&attrs, "fo:page-height")
                    .or_else(|| attr(&attrs, "page-height"))
                    .and_then(|s| parse_length_pt(&s));
                if let (Some(wp), Some(hp)) = (w, h) {
                    return SlideAspect::from_dimensions_pt(wp, hp);
                }
            }
        }
    }
    (SlideAspect::Widescreen16x9, 1.0, 1.0)
}

pub(crate) fn parse_odp_layout(raw: Option<&str>) -> SlideMasterLayout {
    let s = raw.unwrap_or("").trim();
    let stripped = s
        .strip_prefix("eo_")
        .or_else(|| s.strip_prefix("eo-"))
        .unwrap_or(s);
    if let Some(m) = SlideMasterLayout::parse(stripped) {
        return m;
    }
    let low = s.to_ascii_lowercase();
    if low == "al1t0"
        || low.contains("titleonly")
        || (low.contains("title") && !low.contains("content") && !low.contains("two"))
    {
        SlideMasterLayout::TitleSlide
    } else if low.contains("sec") || low.contains("header") {
        SlideMasterLayout::SectionHeader
    } else if low.contains("two") || low.contains("2obj") {
        SlideMasterLayout::TwoColumn
    } else if low == "al1t19" || low.contains("blank") {
        SlideMasterLayout::Blank
    } else {
        SlideMasterLayout::TitleAndContent
    }
}

impl OdfCodec {
    pub fn pack_odp(
        title: &str,
        slides: &[(String, Vec<String>, String)],
    ) -> Result<Vec<u8>, EasyOfficeError> {
        let deck = RichSlideDeck::from_legacy_slides(title, false, slides.to_vec());
        crate::odp_rich::pack_odp_rich(&deck)
    }

    pub fn pack_odp_rich(deck: &RichSlideDeck) -> Result<Vec<u8>, EasyOfficeError> {
        crate::odp_rich::pack_odp_rich(deck)
    }

    pub fn unpack_odp_rich(bytes: &[u8]) -> Result<RichSlideDeck, EasyOfficeError> {
        crate::odp_rich::unpack_odp_rich(bytes)
    }

    pub fn unpack_odp(bytes: &[u8]) -> Result<OdpUnpacked, EasyOfficeError> {
        let (content_xml, meta_xml_opt) =
            Self::extract_content_and_meta_xml(bytes, Self::MIMETYPE_ODP)?;
        let title = extract_attr(&content_xml, "<office:document-content", "title")
            .map(XmlSanitizer::unescape)
            .filter(|s| !s.is_empty())
            .or_else(|| {
                meta_xml_opt
                    .as_deref()
                    .and_then(|m| extract_tag_bodies(m, "dc:title").into_iter().next())
                    .map(XmlSanitizer::unescape)
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_else(|| "발표자료".to_string());

        let raw_slides = extract_balanced_elements(&content_xml, "draw:page");
        if raw_slides.len() > 4096 {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "ODP 슬라이드 개수가 상한(4,096)을 초과했습니다.".to_string(),
            ));
        }
        let mut budget = crate::decode_budget::DecodeBudget::new();
        budget.charge(title.len(), "ODP 제목")?;

        let mut slides = Vec::with_capacity(raw_slides.len());
        for (attrs, inner) in raw_slides {
            let slide = Self::parse_odp_slide(attrs, inner);
            budget.charge(slide.0.len(), "ODP 슬라이드 제목")?;
            for bullet in &slide.1 {
                budget.charge(bullet.len(), "ODP 슬라이드 본문")?;
            }
            budget.charge(slide.2.len(), "ODP 슬라이드 발표자 노트")?;
            slides.push(slide);
        }
        Ok((title, slides))
    }

    /// Speaker notes of a slide: the `notes` attribute, else the notes pages' paragraph text.
    fn odp_slide_notes(attrs: &str, inner: &str) -> String {
        let notes = extract_attr_from_fragment(attrs, "notes")
            .map(XmlSanitizer::unescape)
            .unwrap_or_default();
        let notes_blocks = extract_balanced_elements(inner, "presentation:notes");
        if !notes.is_empty() || notes_blocks.is_empty() {
            return notes;
        }
        let mut note_lines = Vec::new();
        for (_, n_inner) in &notes_blocks {
            let cleaned = strip_tag_blocks(n_inner, "text:page-number");
            for (_, _, p_body) in extract_odf_blocks_in_order(&cleaned, &["text:p", "text:h"]) {
                let txt = extract_odf_inline_text(p_body);
                if !txt.trim().is_empty() {
                    note_lines.push(txt);
                }
            }
        }
        note_lines.join("\n")
    }

    /// Text of the first non-empty title-class frame.
    fn odp_frame_title(without_page_num: &str) -> Option<String> {
        extract_balanced_elements(without_page_num, "draw:frame")
            .into_iter()
            .filter(|(f_attrs, _)| {
                extract_attr_from_fragment(f_attrs, "presentation:class") == Some("title")
            })
            .find_map(|(_, f_inner)| {
                let t_parts: Vec<String> =
                    extract_odf_blocks_in_order(f_inner, &["text:p", "text:h"])
                        .into_iter()
                        .map(|(_, _, b)| extract_odf_inline_text(b))
                        .filter(|s| !s.is_empty())
                        .collect();
                (!t_parts.is_empty()).then(|| t_parts.join(" "))
            })
    }

    fn parse_odp_slide(attrs: &str, inner: &str) -> (String, Vec<String>, String) {
        let notes = Self::odp_slide_notes(attrs, inner);
        let without_notes = strip_tag_blocks(inner, "presentation:notes");
        let without_page_num = strip_tag_blocks(&without_notes, "text:page-number");
        let frame_title = Self::odp_frame_title(&without_page_num);
        let attr_title = extract_attr_from_fragment(attrs, "draw:name")
            .or_else(|| extract_attr_from_fragment(attrs, "title"))
            .map(XmlSanitizer::unescape)
            .unwrap_or_default();

        let mut bullets: Vec<String> =
            extract_odf_blocks_in_order(&without_notes, &["text:p", "text:h"])
                .into_iter()
                .filter_map(|(_, _, b)| {
                    let had_page_num = b.contains("<text:page-number");
                    let cleaned = strip_tag_blocks(b, "text:page-number");
                    let txt = extract_odf_inline_text(&cleaned);
                    (!(had_page_num && txt.trim().is_empty())).then_some(txt)
                })
                .collect();

        let s_title = match frame_title {
            Some(ft) => {
                if bullets.first() == Some(&ft) {
                    bullets.remove(0);
                }
                ft
            }
            None => attr_title,
        };
        (s_title, bullets, notes)
    }

    pub(crate) fn build_manifest(mime: &str) -> String {
        Self::build_manifest_with_media(mime, &[])
    }

    /// Manifest listing `content.xml` plus embedded media parts `(path, media type)`.
    pub(crate) fn build_manifest_with_media(mime: &str, media: &[(String, &str)]) -> String {
        let mut xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\"><manifest:file-entry manifest:full-path=\"/\" manifest:media-type=\"{mime}\"/><manifest:file-entry manifest:full-path=\"content.xml\" manifest:media-type=\"text/xml\"/>"
        );
        for (path, media_type) in media {
            xml.push_str(&format!(
                "<manifest:file-entry manifest:full-path=\"{}\" manifest:media-type=\"{media_type}\"/>",
                XmlSanitizer::escape(path)
            ));
        }
        xml.push_str("</manifest:manifest>");
        xml
    }

    pub(crate) fn extract_content_and_meta_xml(
        bytes: &[u8],
        expected_mime: &str,
    ) -> Result<(String, Option<String>), EasyOfficeError> {
        Self::extract_parts(bytes, expected_mime).map(|(content, meta, _, _)| (content, meta))
    }

    /// `content.xml`, `meta.xml`, `settings.xml` and `styles.xml` (when the file has them) of an ODF package.
    #[allow(clippy::type_complexity)]
    pub(crate) fn extract_parts(
        bytes: &[u8],
        expected_mime: &str,
    ) -> Result<(String, Option<String>, Option<String>, Option<String>), EasyOfficeError> {
        let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())?;
        let fallback_fmt = match expected_mime {
            Self::MIMETYPE_ODT => Some(DocumentFormat::Odt),
            Self::MIMETYPE_ODS => Some(DocumentFormat::Ods),
            Self::MIMETYPE_ODP => Some(DocumentFormat::Odp),
            _ => None,
        };
        validate_zip_xml_entries_for(&entries, fallback_fmt)?;
        if let Some(mime_entry) = entries.iter().find(|e| e.path == "mimetype") {
            let actual_mime = std::str::from_utf8(&mime_entry.data).unwrap_or_default();
            if actual_mime.trim() != expected_mime {
                return Err(EasyOfficeError::SignatureMismatch {
                    expected: ContainerFamily::ZipOdf,
                    detail: format!(
                        "ODF mimetype 불일치 (expected: {expected_mime}, actual: {actual_mime})"
                    ),
                });
            }
        }
        let content_entry = entries
            .iter()
            .find(|e| e.path == "content.xml")
            .ok_or_else(|| {
                EasyOfficeError::CorruptedContainer(
                    "ODF 필수 엔트리 'content.xml'이 누락되었습니다.".to_string(),
                )
            })?;
        let xml = std::str::from_utf8(&content_entry.data)
            .map_err(|_| EasyOfficeError::InvalidUtf8("ODF content.xml UTF-8 오류".to_string()))?;
        XmlSanitizer::validate_safe_xml(xml)?;

        let meta_xml = if let Some(m_entry) = entries.iter().find(|e| e.path == "meta.xml") {
            let m_str = std::str::from_utf8(&m_entry.data)
                .map_err(|_| EasyOfficeError::InvalidUtf8("ODF meta.xml UTF-8 오류".to_string()))?;
            XmlSanitizer::validate_safe_xml(m_str)?;
            Some(m_str.to_string())
        } else {
            None
        };

        let settings_xml = match entries.iter().find(|e| e.path == "settings.xml") {
            Some(entry) => {
                let text = std::str::from_utf8(&entry.data).map_err(|_| {
                    EasyOfficeError::InvalidUtf8("ODF settings.xml UTF-8 오류".to_string())
                })?;
                XmlSanitizer::validate_safe_xml(text)?;
                Some(text.to_string())
            }
            None => None,
        };

        let styles_xml = match entries.iter().find(|e| e.path == "styles.xml") {
            Some(entry) => {
                let text = std::str::from_utf8(&entry.data).map_err(|_| {
                    EasyOfficeError::InvalidUtf8("ODF styles.xml UTF-8 오류".to_string())
                })?;
                XmlSanitizer::validate_safe_xml(text)?;
                Some(text.to_string())
            }
            None => None,
        };

        Ok((xml.to_string(), meta_xml, settings_xml, styles_xml))
    }

    /// A workbook shows at least one sheet: a file that hides them all opens on its first.
    pub(crate) fn show_one_sheet(stylings: &mut [(String, SheetStyling)]) {
        if !stylings.is_empty() && stylings.iter().all(|(_, s)| s.hidden) {
            crate::SheetState::Visible.apply(&mut stylings[0].1);
        }
    }

    /// Gives each table the frozen pane and the view `settings.xml` names for its sheet.
    pub(crate) fn apply_views(stylings: &mut [(String, SheetStyling)], settings: Option<&str>) {
        let Some(settings) = settings else { return };
        if let Some((_, first)) = stylings.first_mut() {
            crate::sheet_styling::sheet_calc_options::read_ods_config_settings(
                settings,
                &mut first.calc_options,
            );
        }
        let mut views = read_views(settings);
        for (name, styling) in stylings {
            if let Some(table) = views.remove(name.as_str()) {
                (styling.freeze, styling.view) = (table.freeze, table.view);
            }
        }
    }
}

// 참조 구현: jmcnamara/rust_xlsxwriter (MIT OR Apache-2.0, Copyright 2022-2026 John McNamara, src/drawing.rs, src/image.rs, src/content_types.rs; see THIRD_PARTY_NOTICES.md)

//! Unified OOXML DrawingML (`xl/drawings/drawing*.xml`, `xl/media/*`) reader & writer
//! combining worksheet charts and floating images under a single `<drawing>` part (`PD-256`).

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use crate::rich_image::{RichImage, MAX_ALT_TEXT_BYTES, MAX_IMAGE_BYTES};
use crate::rich_image_writers::{docx_content_type_defaults, MediaParts};
use crate::sheet_styling::sheet_chart::{
    col_left_px, validate_workbook_charts, ChartAnchor, SheetCellIndex, SheetChart,
    DEFAULT_CHART_HEIGHT_PX, DEFAULT_CHART_WIDTH_PX, EMU_PER_PX, MAX_CHARTS_PER_SHEET,
    MAX_CHART_HEIGHT_PX, MAX_CHART_TEXT_BYTES, MAX_CHART_WIDTH_PX, MAX_WORKBOOK_CHARTS,
    MIN_CHART_HEIGHT_PX, MIN_CHART_WIDTH_PX,
};
use crate::sheet_styling::sheet_media_notes::{
    effective_image_two_cell, sync_image_grid_from_px, sync_image_px_from_grid,
    validate_workbook_sheet_images, SheetFloatingImage, MAX_SHEET_IMAGES_PER_SHEET,
    MAX_SHEET_IMAGE_DIM_PX, MAX_SHEET_IMAGE_NAME_BYTES, MAX_WORKBOOK_SHEET_IMAGES,
    MAX_WORKBOOK_SHEET_IMAGE_BYTES,
};
use crate::sheet_styling::sheet_row_offsets::SheetGrid;
use crate::sheet_styling::xlsx_charts::{
    build_chart_xml, local_name, parse_chart_xml, CHART_CONTENT_TYPE, CHART_REL_TYPE,
};
use crate::sheet_styling::xlsx_parts::resolve_zip_target;
use crate::sheet_styling::SheetStyling;
use crate::xml_stream::{unescape_cow, XmlEvent, XmlReader};
use crate::{EasyOfficeError, OdsUnpacked, OoxmlCodec, XlsxFormulaCache, XmlSanitizer};

const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const DRAWING_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing";
const IMAGE_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const DRAWING_CONTENT_TYPE: &str = "application/vnd.openxmlformats-officedocument.drawing+xml";

const MAX_DRAWING_XML_EVENTS: usize = 65_536;
const MAX_DRAWING_ANCHORS: usize = 512;

#[derive(Default)]
pub(crate) struct SheetDrawingPart {
    pub(crate) drawing_xml: String,
    pub(crate) rels: String,
}

/// Generates unified `xl/drawings/drawing*.xml`, `xl/charts/chart*.xml`, `xl/media/*`,
/// relationships, and content types for both charts and floating images.
pub(crate) struct DrawingWriter<'a> {
    sheets: &'a [OdsUnpacked],
    caches: &'a [XlsxFormulaCache],
    cell_index: SheetCellIndex<'a>,
    media: MediaParts,
    next_drawing_id: usize,
    next_chart_id: usize,
    parts: Vec<(String, Vec<u8>)>,
    pub(crate) content_types: String,
}

impl<'a> DrawingWriter<'a> {
    pub(crate) fn new(
        sheets: &'a [OdsUnpacked],
        caches: &'a [XlsxFormulaCache],
        stylings: &[SheetStyling],
    ) -> Result<Self, EasyOfficeError> {
        validate_workbook_charts(stylings)?;
        validate_workbook_sheet_images(stylings)?;
        Ok(Self {
            sheets,
            caches,
            cell_index: SheetCellIndex::build(sheets),
            media: MediaParts::new(),
            next_drawing_id: 1,
            next_chart_id: 1,
            parts: Vec::new(),
            content_types: String::new(),
        })
    }

    pub(crate) fn default_content_types(&self) -> String {
        docx_content_type_defaults(&self.media)
    }

    pub(crate) fn into_parts(mut self) -> Vec<(String, Vec<u8>)> {
        self.parts.extend(self.media.into_entries());
        self.parts
    }

    pub(crate) fn add_sheet(
        &mut self,
        sheet_name: &str,
        styling: Option<&SheetStyling>,
    ) -> Result<SheetDrawingPart, EasyOfficeError> {
        let charts = styling.map_or(&[][..], |st| &st.charts[..]);
        let images = styling.map_or(&[][..], |st| &st.images[..]);
        if charts.is_empty() && images.is_empty() {
            return Ok(SheetDrawingPart::default());
        }
        let grid = match styling {
            Some(st) => SheetGrid::of(st),
            None => SheetGrid::plain(),
        };
        let d = self.next_drawing_id;
        self.next_drawing_id += 1;

        let mut drawing_xml =
            String::with_capacity(512 + (charts.len() + images.len()).saturating_mul(512));
        drawing_xml.push_str(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
             <xdr:wsDr xmlns:xdr=\"http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing\" \
             xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">",
        );
        let mut drawing_rels =
            String::with_capacity(256 + (charts.len() + images.len()).saturating_mul(160));
        drawing_rels.push_str(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
             <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
        );

        let mut r_num = 0usize;
        let mut shape_id = 0usize;
        for chart in charts {
            let c = self.next_chart_id;
            self.next_chart_id += 1;
            r_num += 1;
            shape_id += 1;
            let (fc, fr, fce, fre, tc, tr, tce, tre) = chart.anchor.effective_two_cell(&grid);
            write_two_cell_bounds(&mut drawing_xml, fc, fr, fce, fre, tc, tr, tce, tre);
            let _ = write!(
                drawing_xml,
                "<xdr:graphicFrame macro=\"\">\
                 <xdr:nvGraphicFramePr><xdr:cNvPr id=\"{shape_id}\" name=\"{}\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr>\
                 <xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm>\
                 <xdr:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\">\
                 <c:chart xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:r=\"{REL_NS}\" r:id=\"rId{r_num}\"/>\
                 </a:graphicData></xdr:graphic></xdr:graphicFrame><xdr:clientData/></xdr:twoCellAnchor>",
                XmlSanitizer::escape(&chart.id),
            );
            let _ = write!(
                drawing_rels,
                "<Relationship Id=\"rId{r_num}\" Type=\"{CHART_REL_TYPE}\" Target=\"../charts/chart{c}.xml\"/>"
            );
            let _ = write!(
                self.content_types,
                "<Override PartName=\"/xl/charts/chart{c}.xml\" ContentType=\"{CHART_CONTENT_TYPE}\"/>"
            );
            let chart_xml = build_chart_xml(
                chart,
                sheet_name,
                self.sheets,
                self.caches,
                &self.cell_index,
            );
            XmlSanitizer::validate_safe_xml(&chart_xml)?;
            self.parts
                .push((format!("xl/charts/chart{c}.xml"), chart_xml.into_bytes()));
        }

        let mut media_rids: HashMap<usize, usize> = HashMap::new();
        for img in images {
            shape_id += 1;
            let media_ref = self.media.add("xl/media", &img.image);
            let img_rid = if let Some(&existing) = media_rids.get(&media_ref.part) {
                existing
            } else {
                r_num += 1;
                media_rids.insert(media_ref.part, r_num);
                let rel_target = media_ref
                    .path
                    .strip_prefix("xl/")
                    .unwrap_or(&media_ref.path);
                let _ = write!(
                    drawing_rels,
                    "<Relationship Id=\"rId{r_num}\" Type=\"{IMAGE_REL_TYPE}\" Target=\"../{rel_target}\"/>"
                );
                r_num
            };
            let (fc, fr, fce, fre, tc, tr, tce, tre) = effective_image_two_cell(&img.anchor, &grid);
            let x_emu = col_left_px(fc, grid.columns)
                .saturating_mul(EMU_PER_PX)
                .saturating_add(u64::from(fce));
            let y_emu = grid
                .rows()
                .top(fr)
                .saturating_mul(EMU_PER_PX)
                .saturating_add(u64::from(fre));
            let x1_emu = col_left_px(tc, grid.columns)
                .saturating_mul(EMU_PER_PX)
                .saturating_add(u64::from(tce));
            let y1_emu = grid
                .rows()
                .top(tr)
                .saturating_mul(EMU_PER_PX)
                .saturating_add(u64::from(tre));
            let w_emu = x1_emu.saturating_sub(x_emu).max(EMU_PER_PX);
            let h_emu = y1_emu.saturating_sub(y_emu).max(EMU_PER_PX);
            let pic_name = if img.name.trim().is_empty() {
                img.id.as_str()
            } else {
                img.name.as_str()
            };
            let descr_attr = if img.alt.is_empty() {
                String::new()
            } else {
                format!(" descr=\"{}\"", XmlSanitizer::escape(&img.alt))
            };
            write_two_cell_bounds(&mut drawing_xml, fc, fr, fce, fre, tc, tr, tce, tre);
            let _ = write!(
                drawing_xml,
                "<xdr:pic><xdr:nvPicPr>\
                 <xdr:cNvPr id=\"{shape_id}\" name=\"{}\"{descr_attr}/>\
                 <xdr:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></xdr:cNvPicPr></xdr:nvPicPr>\
                 <xdr:blipFill><a:blip xmlns:r=\"{REL_NS}\" r:embed=\"rId{img_rid}\"/>\
                 <a:stretch><a:fillRect/></a:stretch></xdr:blipFill>\
                 <xdr:spPr><a:xfrm><a:off x=\"{x_emu}\" y=\"{y_emu}\"/><a:ext cx=\"{w_emu}\" cy=\"{h_emu}\"/></a:xfrm>\
                 <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic>\
                 <xdr:clientData/></xdr:twoCellAnchor>",
                XmlSanitizer::escape(pic_name),
            );
        }

        drawing_xml.push_str("</xdr:wsDr>");
        drawing_rels.push_str("</Relationships>");
        XmlSanitizer::validate_safe_xml(&drawing_xml)?;
        XmlSanitizer::validate_safe_xml(&drawing_rels)?;
        self.parts.push((
            format!("xl/drawings/drawing{d}.xml"),
            drawing_xml.into_bytes(),
        ));
        self.parts.push((
            format!("xl/drawings/_rels/drawing{d}.xml.rels"),
            drawing_rels.into_bytes(),
        ));
        let _ = write!(
            self.content_types,
            "<Override PartName=\"/xl/drawings/drawing{d}.xml\" ContentType=\"{DRAWING_CONTENT_TYPE}\"/>"
        );
        Ok(SheetDrawingPart {
            drawing_xml: format!("<drawing xmlns:r=\"{REL_NS}\" r:id=\"rIdD1\"/>"),
            rels: format!(
                "<Relationship Id=\"rIdD1\" Type=\"{DRAWING_REL_TYPE}\" Target=\"../drawings/drawing{d}.xml\"/>"
            ),
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn write_two_cell_bounds(
    out: &mut String,
    fc: u32,
    fr: u32,
    fce: u32,
    fre: u32,
    tc: u32,
    tr: u32,
    tce: u32,
    tre: u32,
) {
    let _ = write!(
        out,
        "<xdr:twoCellAnchor>\
         <xdr:from><xdr:col>{fc}</xdr:col><xdr:colOff>{fce}</xdr:colOff>\
         <xdr:row>{fr}</xdr:row><xdr:rowOff>{fre}</xdr:rowOff></xdr:from>\
         <xdr:to><xdr:col>{tc}</xdr:col><xdr:colOff>{tce}</xdr:colOff>\
         <xdr:row>{tr}</xdr:row><xdr:rowOff>{tre}</xdr:rowOff></xdr:to>"
    );
}

// ---- Reading ------------------------------------------------------------------------------------------

#[derive(Default)]
pub(crate) struct DrawingReadBudget {
    pub(crate) charts: usize,
    pub(crate) images: usize,
    pub(crate) image_bytes: usize,
}

fn rels_for_part_types(
    entries: &HashMap<&str, &[u8]>,
    part_path: &str,
) -> Result<HashMap<String, (String, String)>, EasyOfficeError> {
    let (dir, file) = part_path.rsplit_once('/').unwrap_or(("", part_path));
    let rels_path = if dir.is_empty() {
        format!("_rels/{file}.rels")
    } else {
        format!("{dir}/_rels/{file}.rels")
    };
    let mut out = HashMap::new();
    let Some(xml) = entries
        .get(rels_path.as_str())
        .and_then(|d| std::str::from_utf8(d).ok())
    else {
        return Ok(out);
    };
    let mut reader = XmlReader::with_budget(xml, MAX_DRAWING_XML_EVENTS);
    while let Some(ev) = reader.next_event()? {
        let XmlEvent::Start { name, attrs } = ev else {
            continue;
        };
        if local_name(name) != "Relationship" {
            continue;
        }
        let (mut rel_type, mut id, mut target, mut external) = (None, None, None, false);
        for (k, v) in attrs.iter_raw() {
            match k {
                "Type" => rel_type = Some(unescape_cow(v)),
                "Id" => id = Some(unescape_cow(v)),
                "Target" => target = Some(unescape_cow(v)),
                "TargetMode" => external = v.eq_ignore_ascii_case("External"),
                _ => {}
            }
        }
        if !external {
            if let (Some(ty), Some(id), Some(target)) = (rel_type, id, target) {
                out.insert(id.into_owned(), (ty.into_owned(), target.into_owned()));
            }
        }
    }
    Ok(out)
}

fn sheet_drawing_rids(sheet_xml: &str) -> Result<Vec<String>, EasyOfficeError> {
    let tail_start = sheet_xml
        .rfind("</sheetData>")
        .or_else(|| sheet_xml.rfind("<sheetData/>"))
        .unwrap_or(0);
    let tail = &sheet_xml[tail_start..];
    if !tail.contains("drawing") {
        return Ok(Vec::new());
    }
    let mut reader = XmlReader::with_budget(tail, MAX_DRAWING_XML_EVENTS);
    let mut ids = Vec::new();
    while let Some(ev) = reader.next_event()? {
        let XmlEvent::Start { name, attrs } = ev else {
            continue;
        };
        if local_name(name) != "drawing" {
            continue;
        }
        ids.push(
            attrs
                .get("r:id")
                .or_else(|| attrs.get("id"))
                .map_or_else(String::new, |v| v.into_owned()),
        );
        if ids.len() > MAX_CHARTS_PER_SHEET + MAX_SHEET_IMAGES_PER_SHEET {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "시트 드로잉 관계 수가 허용 상한을 초과했습니다.".to_string(),
            ));
        }
    }
    Ok(ids)
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum AnchorKind {
    #[default]
    TwoCell,
    OneCell,
    Absolute,
}

#[derive(Default)]
struct AnchorPoint {
    col: Option<u32>,
    row: Option<u32>,
    col_off: u32,
    row_off: u32,
}

#[derive(Default)]
struct DrawingAnchorRaw {
    kind: AnchorKind,
    from: AnchorPoint,
    to: AnchorPoint,
    pos_x: Option<u64>,
    pos_y: Option<u64>,
    ext_cx: Option<u64>,
    ext_cy: Option<u64>,
    name: Option<String>,
    descr: Option<String>,
    chart_rid: Option<String>,
    has_pic: bool,
    pic_rid: Option<String>,
}

fn parse_drawing_anchors(xml: &str) -> Result<Vec<DrawingAnchorRaw>, EasyOfficeError> {
    let mut reader = XmlReader::with_budget(xml, MAX_DRAWING_XML_EVENTS);
    let mut out = Vec::new();
    let mut cur: Option<DrawingAnchorRaw> = None;
    let mut loc_to: Option<bool> = None;
    let mut capture = false;
    let mut text = String::new();

    while let Some(event) = reader.next_event()? {
        match event {
            XmlEvent::Start { name, attrs } => match local_name(name) {
                tag @ ("twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor") => {
                    let kind = match tag {
                        "twoCellAnchor" => AnchorKind::TwoCell,
                        "oneCellAnchor" => AnchorKind::OneCell,
                        _ => AnchorKind::Absolute,
                    };
                    cur = Some(DrawingAnchorRaw {
                        kind,
                        ..Default::default()
                    });
                    loc_to = None;
                    capture = false;
                }
                "from" if cur.is_some() => loc_to = Some(false),
                "to" if cur.is_some() => loc_to = Some(true),
                "col" | "colOff" | "row" | "rowOff" if loc_to.is_some() => {
                    capture = true;
                    text.clear();
                }
                "pos" if cur.as_ref().is_some_and(|a| a.kind == AnchorKind::Absolute) => {
                    if let Some(a) = cur.as_mut() {
                        a.pos_x = attrs.get("x").and_then(|s| s.trim().parse().ok());
                        a.pos_y = attrs.get("y").and_then(|s| s.trim().parse().ok());
                    }
                }
                "ext"
                    if cur
                        .as_ref()
                        .is_some_and(|a| a.kind != AnchorKind::TwoCell && loc_to.is_none()) =>
                {
                    if let Some(a) = cur.as_mut().filter(|a| a.ext_cx.is_none()) {
                        a.ext_cx = attrs.get("cx").and_then(|s| s.trim().parse().ok());
                        a.ext_cy = attrs.get("cy").and_then(|s| s.trim().parse().ok());
                    }
                }
                "pic" if cur.is_some() => {
                    if let Some(a) = cur.as_mut() {
                        a.has_pic = true;
                    }
                }
                "blip" if cur.is_some() => {
                    if let Some(a) = cur.as_mut().filter(|a| a.pic_rid.is_none()) {
                        a.pic_rid = attrs
                            .get("r:embed")
                            .or_else(|| attrs.get("embed"))
                            .map(|s| s.into_owned())
                            .filter(|s| !s.is_empty());
                    }
                }
                "cNvPr" if cur.is_some() => {
                    if let Some(a) = cur.as_mut() {
                        if a.name.is_none() {
                            a.name = attrs
                                .get("name")
                                .map(|s| s.into_owned())
                                .filter(|s| !s.is_empty());
                        }
                        if a.descr.is_none() {
                            a.descr = attrs
                                .get("descr")
                                .map(|s| s.into_owned())
                                .filter(|s| !s.is_empty());
                        }
                    }
                }
                "chart" if cur.is_some() => {
                    if let Some(a) = cur.as_mut() {
                        a.chart_rid = Some(
                            attrs
                                .get("r:id")
                                .or_else(|| attrs.get("id"))
                                .map_or_else(String::new, |s| s.into_owned()),
                        );
                    }
                }
                _ => {}
            },
            XmlEvent::Text(t) if capture && text.len() + t.len() <= 64 => text.push_str(&t),
            XmlEvent::End(name) => match local_name(name) {
                "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor" => {
                    if let Some(anchor) = cur.take().filter(|a| a.chart_rid.is_some() || a.has_pic)
                    {
                        out.push(anchor);
                        if out.len() > MAX_DRAWING_ANCHORS {
                            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                                "시트 드로잉 요소 수가 허용 상한({MAX_DRAWING_ANCHORS})을 초과했습니다."
                            )));
                        }
                    }
                    loc_to = None;
                    capture = false;
                }
                "from" | "to" => {
                    loc_to = None;
                    capture = false;
                }
                tag @ ("col" | "colOff" | "row" | "rowOff") => {
                    capture = false;
                    if let (Some(a), Some(is_to), Ok(v)) =
                        (cur.as_mut(), loc_to, text.trim().parse::<u64>())
                    {
                        let pt = if is_to { &mut a.to } else { &mut a.from };
                        match tag {
                            "col" => pt.col = u32::try_from(v).ok(),
                            "row" => pt.row = u32::try_from(v).ok(),
                            "colOff" => pt.col_off = v.min(u64::from(u32::MAX)) as u32,
                            _ => pt.row_off = v.min(u64::from(u32::MAX)) as u32,
                        }
                    }
                    text.clear();
                }
                _ => {}
            },
            _ => {}
        }
    }
    Ok(out)
}

#[inline]
fn emu_u64_to_px(emu: u64) -> u32 {
    u32::try_from(emu.saturating_add(EMU_PER_PX / 2) / EMU_PER_PX).unwrap_or(u32::MAX)
}

fn build_anchor_from_raw(
    raw: &DrawingAnchorRaw,
    grid: &SheetGrid<'_>,
    is_image: bool,
) -> Option<ChartAnchor> {
    let (min_w, max_w, def_w, min_h, max_h, def_h) = if is_image {
        (
            1,
            MAX_SHEET_IMAGE_DIM_PX,
            240,
            1,
            MAX_SHEET_IMAGE_DIM_PX,
            180,
        )
    } else {
        (
            MIN_CHART_WIDTH_PX,
            MAX_CHART_WIDTH_PX,
            DEFAULT_CHART_WIDTH_PX,
            MIN_CHART_HEIGHT_PX,
            MAX_CHART_HEIGHT_PX,
            DEFAULT_CHART_HEIGHT_PX,
        )
    };
    let mut anchor = ChartAnchor::default();
    match raw.kind {
        AnchorKind::TwoCell => {
            let (fc, fr) = (raw.from.col?, raw.from.row?);
            let (tc, tr) = (raw.to.col?, raw.to.row?);
            if fc >= OoxmlCodec::MAX_XLSX_COLS
                || fr >= OoxmlCodec::MAX_XLSX_ROWS
                || tc >= OoxmlCodec::MAX_XLSX_COLS
                || tr >= OoxmlCodec::MAX_XLSX_ROWS
            {
                return None;
            }
            anchor.from_col = fc;
            anchor.from_row = fr;
            anchor.col_off_emu = raw.from.col_off;
            anchor.row_off_emu = raw.from.row_off;
            anchor.to_col = tc;
            anchor.to_row = tr;
            anchor.to_col_off_emu = raw.to.col_off;
            anchor.to_row_off_emu = raw.to.row_off;
            if is_image {
                sync_image_px_from_grid(&mut anchor, grid);
            } else {
                anchor.sync_px_from_grid(grid);
            }
        }
        AnchorKind::OneCell => {
            let (fc, fr) = (raw.from.col?, raw.from.row?);
            if fc >= OoxmlCodec::MAX_XLSX_COLS || fr >= OoxmlCodec::MAX_XLSX_ROWS {
                return None;
            }
            let w = raw.ext_cx.map_or(def_w, emu_u64_to_px).clamp(min_w, max_w);
            let h = raw.ext_cy.map_or(def_h, emu_u64_to_px).clamp(min_h, max_h);
            anchor.left_px = col_left_px(fc, grid.columns)
                .saturating_add(u64::from(emu_u64_to_px(u64::from(raw.from.col_off))))
                .min(u64::from(u32::MAX)) as u32;
            anchor.top_px = grid
                .rows()
                .top(fr)
                .saturating_add(u64::from(emu_u64_to_px(u64::from(raw.from.row_off))))
                .min(u64::from(u32::MAX)) as u32;
            anchor.width_px = w;
            anchor.height_px = h;
            if is_image {
                sync_image_grid_from_px(&mut anchor, grid);
            } else {
                anchor.sync_grid_from_px(grid);
            }
            anchor.from_col = fc;
            anchor.from_row = fr;
            anchor.col_off_emu = raw.from.col_off;
            anchor.row_off_emu = raw.from.row_off;
        }
        AnchorKind::Absolute => {
            anchor.left_px = raw.pos_x.map_or(0, emu_u64_to_px);
            anchor.top_px = raw.pos_y.map_or(0, emu_u64_to_px);
            anchor.width_px = raw.ext_cx.map_or(def_w, emu_u64_to_px).clamp(min_w, max_w);
            anchor.height_px = raw.ext_cy.map_or(def_h, emu_u64_to_px).clamp(min_h, max_h);
            if is_image {
                sync_image_grid_from_px(&mut anchor, grid);
            } else {
                anchor.sync_grid_from_px(grid);
            }
        }
    }
    Some(anchor)
}

fn resolve_rel_bytes<'a>(
    rels: &HashMap<String, (String, String)>,
    wanted_type: &str,
    dir: &str,
    rid: &str,
    entries: &HashMap<&str, &'a [u8]>,
) -> Option<(String, &'a [u8])> {
    let (ty, target) = rels.get(rid)?;
    if ty != wanted_type {
        return None;
    }
    let path = resolve_zip_target(dir, target).filter(|p| p.starts_with("xl/"))?;
    let bytes = entries.get(path.as_str()).copied()?;
    Some((path, bytes))
}

fn truncate_utf8(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut cut = max_bytes;
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    s[..cut].to_string()
}

/// Reads charts and floating images attached to worksheet `part_path`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn read_sheet_drawings(
    sheet_name: &str,
    sheet_xml: &str,
    part_path: &str,
    entries: &HashMap<&str, &[u8]>,
    grid: &SheetGrid<'_>,
    budget: &mut DrawingReadBudget,
    charts_out: &mut Vec<SheetChart>,
    images_out: &mut Vec<SheetFloatingImage>,
) -> Result<(u32, u32), EasyOfficeError> {
    let rids = sheet_drawing_rids(sheet_xml)?;
    if rids.is_empty() {
        return Ok((0, 0));
    }
    let sheet_rels = rels_for_part_types(entries, part_path)?;
    let sheet_dir = part_path.rsplit_once('/').map_or("", |(d, _)| d);
    let (mut charts_rejected, mut images_rejected) = (0u32, 0u32);
    let mut seen_drawings = HashSet::new();
    let (mut charts_in_sheet, mut images_in_sheet) = (0usize, 0usize);

    for rid in rids {
        let Some((drawing_path, drawing_bytes)) =
            resolve_rel_bytes(&sheet_rels, DRAWING_REL_TYPE, sheet_dir, &rid, entries)
        else {
            charts_rejected = charts_rejected.saturating_add(1);
            continue;
        };
        let Ok(drawing_xml) = std::str::from_utf8(drawing_bytes) else {
            charts_rejected = charts_rejected.saturating_add(1);
            continue;
        };
        if !seen_drawings.insert(drawing_path.clone()) {
            continue;
        }
        let anchors = parse_drawing_anchors(drawing_xml)?;
        let drawing_rels = rels_for_part_types(entries, &drawing_path)?;
        let drawing_dir = drawing_path.rsplit_once('/').map_or("", |(d, _)| d);

        for raw_anchor in anchors {
            if let Some(cid) = raw_anchor.chart_rid.as_deref() {
                charts_in_sheet += 1;
                if charts_in_sheet > MAX_CHARTS_PER_SHEET {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                        "시트 차트 수가 허용 상한({MAX_CHARTS_PER_SHEET})을 초과했습니다."
                    )));
                }
                if budget.charts >= MAX_WORKBOOK_CHARTS {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                        "통합 문서 전체 차트 수가 허용 상한({MAX_WORKBOOK_CHARTS})을 초과했습니다."
                    )));
                }
                budget.charts += 1;

                let parsed_anchor = build_anchor_from_raw(&raw_anchor, grid, false);
                let chart_xml =
                    resolve_rel_bytes(&drawing_rels, CHART_REL_TYPE, drawing_dir, cid, entries)
                        .and_then(|(_, data)| std::str::from_utf8(data).ok());
                let chart_id = raw_anchor
                    .name
                    .as_deref()
                    .filter(|s| !s.trim().is_empty() && s.len() <= MAX_CHART_TEXT_BYTES)
                    .map_or_else(
                        || format!("chart-{}", charts_out.len() + 1),
                        ToString::to_string,
                    );
                let parsed_chart = match (parsed_anchor, chart_xml) {
                    (Some(a), Some(xml)) => parse_chart_xml(xml, sheet_name, chart_id, a)?,
                    _ => None,
                };
                if let Some(chart) = parsed_chart {
                    charts_out.push(chart);
                } else {
                    charts_rejected = charts_rejected.saturating_add(1);
                }
            } else if raw_anchor.has_pic {
                images_in_sheet += 1;
                if images_in_sheet > MAX_SHEET_IMAGES_PER_SHEET {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                        "시트 그림 수가 허용 상한({MAX_SHEET_IMAGES_PER_SHEET})을 초과했습니다."
                    )));
                }
                if budget.images >= MAX_WORKBOOK_SHEET_IMAGES {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                        "통합 문서 전체 그림 수가 허용 상한({MAX_WORKBOOK_SHEET_IMAGES})을 초과했습니다."
                    )));
                }
                budget.images += 1;

                let Some(media_bytes) = raw_anchor
                    .pic_rid
                    .as_deref()
                    .and_then(|pid| {
                        resolve_rel_bytes(&drawing_rels, IMAGE_REL_TYPE, drawing_dir, pid, entries)
                    })
                    .map(|(_, data)| data)
                else {
                    images_rejected = images_rejected.saturating_add(1);
                    continue;
                };
                if media_bytes.len() > MAX_IMAGE_BYTES {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                        "그림 크기({}바이트)가 한도({MAX_IMAGE_BYTES}바이트)를 넘었습니다.",
                        media_bytes.len()
                    )));
                }
                let next_bytes = budget.image_bytes.saturating_add(media_bytes.len());
                if next_bytes > MAX_WORKBOOK_SHEET_IMAGE_BYTES {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                        "통합 문서 그림 총 크기({next_bytes}바이트)가 허용 상한({MAX_WORKBOOK_SHEET_IMAGE_BYTES}바이트)을 초과했습니다."
                    )));
                }
                let alt_raw = raw_anchor.descr.as_deref().unwrap_or("");
                let rich_img = match RichImage::new(
                    media_bytes.to_vec(),
                    truncate_utf8(alt_raw, MAX_ALT_TEXT_BYTES),
                ) {
                    Ok(img) => img,
                    Err(EasyOfficeError::ResourceBudgetExceeded(msg)) => {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(msg));
                    }
                    Err(_) => {
                        images_rejected = images_rejected.saturating_add(1);
                        continue;
                    }
                };
                budget.image_bytes = next_bytes;
                let Some(mut anchor) = build_anchor_from_raw(&raw_anchor, grid, true) else {
                    images_rejected = images_rejected.saturating_add(1);
                    continue;
                };
                if !anchor.has_explicit_grid() && raw_anchor.ext_cx.is_none() {
                    anchor.width_px = rich_img.width_px().clamp(1, MAX_SHEET_IMAGE_DIM_PX);
                    anchor.height_px = rich_img.height_px().clamp(1, MAX_SHEET_IMAGE_DIM_PX);
                    sync_image_grid_from_px(&mut anchor, grid);
                }
                let name = raw_anchor
                    .name
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| truncate_utf8(s, MAX_SHEET_IMAGE_NAME_BYTES))
                    .unwrap_or_else(|| format!("그림 {}", images_out.len() + 1));
                let alt = rich_img.alt().to_string();
                images_out.push(SheetFloatingImage {
                    id: format!("pic-{}", images_out.len() + 1),
                    name,
                    alt,
                    image: rich_img,
                    anchor,
                });
            }
        }
    }
    Ok((charts_rejected, images_rejected))
}

// 참조 구현: jmcnamara/rust_xlsxwriter (MIT OR Apache-2.0, Copyright 2022-2026 John McNamara, src/chart.rs, src/drawing.rs; see THIRD_PARTY_NOTICES.md)

//! Bidirectional OOXML SpreadsheetML DrawingML chart codec (`xl/drawings/drawing*.xml`,
//! `xl/charts/chart*.xml`) with fail-closed budgets and XML sanitization (`PD-254`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write;

use crate::sheet_styling::sheet_chart::{
    col_left_px, parse_sheet_and_box, row_top_px, validate_workbook_charts, ChartAnchor, ChartKind,
    ChartLegendPos, ChartSeriesRef, SheetCellIndex, SheetChart, DEFAULT_CHART_HEIGHT_PX,
    DEFAULT_CHART_WIDTH_PX, EMU_PER_PX, MAX_CHARTS_PER_SHEET, MAX_CHART_HEIGHT_PX,
    MAX_CHART_TEXT_BYTES, MAX_CHART_WIDTH_PX, MAX_POINTS_PER_SERIES, MAX_SERIES_PER_CHART,
    MAX_WORKBOOK_CHARTS, MIN_CHART_HEIGHT_PX, MIN_CHART_WIDTH_PX,
};
use crate::sheet_styling::SheetStyling;
use crate::xml_stream::{unescape_cow, XmlEvent, XmlReader};
use crate::{CellBox, EasyOfficeError, OdsUnpacked, OoxmlCodec, XlsxFormulaCache, XmlSanitizer};

const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const DRAWING_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing";
const CHART_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart";
const DRAWING_CONTENT_TYPE: &str = "application/vnd.openxmlformats-officedocument.drawing+xml";
const CHART_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.drawingml.chart+xml";

const AX_ID_CAT: &str = "64052224";
const AX_ID_VAL: &str = "64055552";
const MAX_DRAWING_XML_EVENTS: usize = 32_768;
const MAX_CHART_XML_EVENTS: usize = 65_536;

#[inline]
fn local_name(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

#[derive(Default)]
pub(crate) struct SheetChartPart {
    pub(crate) drawing_xml: String,
    pub(crate) rels: String,
}

/// Generates `xl/drawings/drawing*.xml`, `xl/charts/chart*.xml`, relationships, and content types.
pub(crate) struct ChartWriter<'a> {
    sheets: &'a [OdsUnpacked],
    caches: &'a [XlsxFormulaCache],
    cell_index: SheetCellIndex<'a>,
    next_drawing_id: usize,
    next_chart_id: usize,
    pub(crate) parts: Vec<(String, Vec<u8>)>,
    pub(crate) content_types: String,
}

impl<'a> ChartWriter<'a> {
    pub(crate) fn new(
        sheets: &'a [OdsUnpacked],
        caches: &'a [XlsxFormulaCache],
        stylings: &[SheetStyling],
    ) -> Result<Self, EasyOfficeError> {
        validate_workbook_charts(stylings)?;
        Ok(Self {
            sheets,
            caches,
            cell_index: SheetCellIndex::build(sheets),
            next_drawing_id: 1,
            next_chart_id: 1,
            parts: Vec::new(),
            content_types: String::new(),
        })
    }

    pub(crate) fn add_sheet(
        &mut self,
        sheet_name: &str,
        styling: Option<&SheetStyling>,
    ) -> Result<SheetChartPart, EasyOfficeError> {
        let Some(charts) = styling.map(|st| &st.charts).filter(|c| !c.is_empty()) else {
            return Ok(SheetChartPart::default());
        };
        let empty_cols = BTreeMap::new();
        let cols_ref = styling.map_or(&empty_cols, |st| &st.columns);
        let d = self.next_drawing_id;
        self.next_drawing_id += 1;

        let mut drawing_xml = String::with_capacity(512 + charts.len() * 512);
        drawing_xml.push_str(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
             <xdr:wsDr xmlns:xdr=\"http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing\" \
             xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">",
        );
        let mut drawing_rels = String::with_capacity(256 + charts.len() * 160);
        drawing_rels.push_str(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
             <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
        );

        for (idx, chart) in charts.iter().enumerate() {
            let c = self.next_chart_id;
            self.next_chart_id += 1;
            let r_num = idx + 1;
            let (fc, fr, fce, fre, tc, tr, tce, tre) = chart.anchor.effective_two_cell(cols_ref);
            let _ = write!(
                drawing_xml,
                "<xdr:twoCellAnchor>\
                 <xdr:from><xdr:col>{fc}</xdr:col><xdr:colOff>{fce}</xdr:colOff>\
                 <xdr:row>{fr}</xdr:row><xdr:rowOff>{fre}</xdr:rowOff></xdr:from>\
                 <xdr:to><xdr:col>{tc}</xdr:col><xdr:colOff>{tce}</xdr:colOff>\
                 <xdr:row>{tr}</xdr:row><xdr:rowOff>{tre}</xdr:rowOff></xdr:to>\
                 <xdr:graphicFrame macro=\"\">\
                 <xdr:nvGraphicFramePr><xdr:cNvPr id=\"{r_num}\" name=\"{}\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr>\
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
            let chart_xml = self.build_chart_xml(chart, sheet_name);
            XmlSanitizer::validate_safe_xml(&chart_xml)?;
            self.parts
                .push((format!("xl/charts/chart{c}.xml"), chart_xml.into_bytes()));
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
        Ok(SheetChartPart {
            drawing_xml: format!("<drawing xmlns:r=\"{REL_NS}\" r:id=\"rIdD1\"/>"),
            rels: format!(
                "<Relationship Id=\"rIdD1\" Type=\"{DRAWING_REL_TYPE}\" Target=\"../drawings/drawing{d}.xml\"/>"
            ),
        })
    }

    fn build_chart_xml(&self, chart: &SheetChart, sheet_name: &str) -> String {
        let mut xml = String::with_capacity(2_048);
        xml.push_str(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
             <c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" \
             xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" \
             xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">\
             <c:lang val=\"ko-KR\"/><c:chart>",
        );
        if chart.title.is_empty() {
            xml.push_str("<c:autoTitleDeleted val=\"1\"/>");
        } else {
            let _ = write!(
                xml,
                "<c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr/></a:pPr>\
                 <a:r><a:t>{}</a:t></a:r></a:p></c:rich></c:tx><c:overlay val=\"0\"/></c:title>\
                 <c:autoTitleDeleted val=\"0\"/>",
                XmlSanitizer::escape(&chart.title)
            );
        }

        xml.push_str("<c:plotArea><c:layout/>");
        let series_list = chart.resolved_series(sheet_name, self.sheets);
        if chart.kind == ChartKind::Pie {
            xml.push_str("<c:pieChart><c:varyColors val=\"1\"/>");
            self.push_series_list(&mut xml, &series_list, sheet_name, false);
            xml.push_str("<c:firstSliceAng val=\"0\"/></c:pieChart>");
        } else {
            let (tag, hdr, extra) = match chart.kind {
                ChartKind::Bar => (
                    "c:barChart",
                    "<c:barDir val=\"bar\"/><c:grouping val=\"clustered\"/>",
                    "",
                ),
                ChartKind::Column => (
                    "c:barChart",
                    "<c:barDir val=\"col\"/><c:grouping val=\"clustered\"/>",
                    "",
                ),
                ChartKind::Line => (
                    "c:lineChart",
                    "<c:grouping val=\"standard\"/>",
                    "<c:marker val=\"1\"/>",
                ),
                ChartKind::Scatter => {
                    ("c:scatterChart", "<c:scatterStyle val=\"lineMarker\"/>", "")
                }
                ChartKind::Pie => unreachable!(),
            };
            let is_scatter = chart.kind == ChartKind::Scatter;
            let (cat_pos, val_pos) = if chart.kind == ChartKind::Bar {
                ("l", "b")
            } else {
                ("b", "l")
            };
            let _ = write!(xml, "<{tag}>{hdr}");
            self.push_series_list(&mut xml, &series_list, sheet_name, is_scatter);
            let _ = write!(
                xml,
                "{extra}<c:axId val=\"{AX_ID_CAT}\"/><c:axId val=\"{AX_ID_VAL}\"/></{tag}>"
            );
            if is_scatter {
                push_val_ax(&mut xml, AX_ID_CAT, AX_ID_VAL, cat_pos, false);
            } else {
                push_cat_ax(&mut xml, cat_pos);
            }
            push_val_ax(&mut xml, AX_ID_VAL, AX_ID_CAT, val_pos, true);
        }
        xml.push_str("</c:plotArea>");
        if let Some(pos) = chart.legend.ooxml_val() {
            let _ = write!(
                xml,
                "<c:legend><c:legendPos val=\"{pos}\"/><c:overlay val=\"0\"/></c:legend>"
            );
        }
        xml.push_str("<c:plotVisOnly val=\"1\"/></c:chart></c:chartSpace>");
        xml
    }

    fn push_series_list(
        &self,
        xml: &mut String,
        series_list: &[ChartSeriesRef],
        sheet_name: &str,
        is_scatter: bool,
    ) {
        let (cat_tag, val_tag) = if is_scatter {
            ("c:xVal", "c:yVal")
        } else {
            ("c:cat", "c:val")
        };
        for (idx, ser) in series_list.iter().take(MAX_SERIES_PER_CHART).enumerate() {
            let resolved = self.cell_index.resolve_series(sheet_name, self.caches, ser);
            let _ = write!(xml, "<c:ser><c:idx val=\"{idx}\"/><c:order val=\"{idx}\"/>");
            let tx_name = if ser.name.is_empty() {
                resolved.name.as_str()
            } else {
                ser.name.as_str()
            };
            if let Some(nref) = ser.name_ref.as_deref() {
                let _ = write!(
                    xml,
                    "<c:tx><c:strRef><c:f>{}</c:f>",
                    XmlSanitizer::escape(nref)
                );
                if !tx_name.is_empty() {
                    push_text_cache(xml, &[tx_name], false);
                }
                xml.push_str("</c:strRef></c:tx>");
            } else {
                let _ = write!(
                    xml,
                    "<c:tx><c:v>{}</c:v></c:tx>",
                    XmlSanitizer::escape(tx_name)
                );
            }

            if let Some(cref) = ser.categories_ref.as_deref() {
                let ref_tag = if resolved.categories_numeric {
                    "c:numRef"
                } else {
                    "c:strRef"
                };
                let _ = write!(
                    xml,
                    "<{cat_tag}><{ref_tag}><c:f>{}</c:f>",
                    XmlSanitizer::escape(cref)
                );
                push_text_cache(xml, &resolved.categories, resolved.categories_numeric);
                let _ = write!(xml, "</{ref_tag}></{cat_tag}>");
            }

            let _ = write!(xml, "<{val_tag}>");
            if !ser.values_ref.is_empty() {
                let _ = write!(
                    xml,
                    "<c:numRef><c:f>{}</c:f>",
                    XmlSanitizer::escape(&ser.values_ref)
                );
                push_num_cache(xml, &resolved.values);
                xml.push_str("</c:numRef>");
            } else {
                xml.push_str(
                    "<c:numLit><c:formatCode>General</c:formatCode><c:ptCount val=\"0\"/></c:numLit>",
                );
            }
            let _ = write!(xml, "</{val_tag}></c:ser>");
        }
    }
}

fn push_cat_ax(xml: &mut String, ax_pos: &str) {
    let _ = write!(
        xml,
        "<c:catAx><c:axId val=\"{AX_ID_CAT}\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling>\
         <c:delete val=\"0\"/><c:axPos val=\"{ax_pos}\"/><c:crossAx val=\"{AX_ID_VAL}\"/></c:catAx>"
    );
}

fn push_val_ax(xml: &mut String, ax_id: &str, cross_ax: &str, ax_pos: &str, gridlines: bool) {
    let grid = if gridlines { "<c:majorGridlines/>" } else { "" };
    let _ = write!(
        xml,
        "<c:valAx><c:axId val=\"{ax_id}\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling>\
         <c:delete val=\"0\"/><c:axPos val=\"{ax_pos}\"/>{grid}<c:crossAx val=\"{cross_ax}\"/></c:valAx>"
    );
}

fn push_text_cache<S: AsRef<str>>(xml: &mut String, items: &[S], numeric: bool) {
    let (tag, hdr) = if numeric {
        ("c:numCache", "<c:formatCode>General</c:formatCode>")
    } else {
        ("c:strCache", "")
    };
    let _ = write!(xml, "<{tag}>{hdr}<c:ptCount val=\"{}\"/>", items.len());
    for (i, item) in items.iter().enumerate() {
        let s = item.as_ref();
        let val = if numeric { s.trim() } else { s };
        if !val.is_empty() {
            let _ = write!(
                xml,
                "<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>",
                XmlSanitizer::escape(val)
            );
        }
    }
    let _ = write!(xml, "</{tag}>");
}

fn push_num_cache(xml: &mut String, values: &[Option<f64>]) {
    let _ = write!(
        xml,
        "<c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"{}\"/>",
        values.len()
    );
    for (i, v) in values.iter().enumerate() {
        if let Some(n) = v.filter(|x| x.is_finite()) {
            let _ = write!(xml, "<c:pt idx=\"{i}\"><c:v>{n}</c:v></c:pt>");
        }
    }
    xml.push_str("</c:numCache>");
}

// ---- Reading ------------------------------------------------------------------------------------------

fn resolve_zip_target(dir: &str, target: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    let joined;
    let path = if let Some(absolute) = target.strip_prefix('/') {
        absolute
    } else {
        joined = if dir.is_empty() {
            target.to_string()
        } else {
            format!("{dir}/{target}")
        };
        &joined
    };
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

fn rels_for_part(
    entries: &HashMap<&str, &[u8]>,
    part_path: &str,
    wanted_type: &str,
) -> HashMap<String, String> {
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
        return out;
    };
    let mut reader = XmlReader::with_budget(xml, MAX_DRAWING_XML_EVENTS);
    while let Ok(Some(ev)) = reader.next_event() {
        let XmlEvent::Start { name, attrs } = ev else {
            continue;
        };
        if local_name(name) != "Relationship" {
            continue;
        }
        let (mut ty_ok, mut id, mut target) = (false, None, None);
        for (k, v) in attrs.iter_raw() {
            match k {
                "Type" => ty_ok = v == wanted_type,
                "Id" => id = Some(unescape_cow(v)),
                "Target" => target = Some(unescape_cow(v)),
                _ => {}
            }
        }
        if ty_ok {
            if let (Some(id), Some(target)) = (id, target) {
                out.insert(id.into_owned(), target.into_owned());
            }
        }
    }
    out
}

fn sheet_drawing_rids(sheet_xml: &str) -> Vec<String> {
    let tail_start = sheet_xml
        .rfind("</sheetData>")
        .or_else(|| sheet_xml.rfind("<sheetData/>"))
        .unwrap_or(0);
    let tail = &sheet_xml[tail_start..];
    if !tail.contains("drawing") {
        return Vec::new();
    }
    let mut reader = XmlReader::with_budget(tail, MAX_DRAWING_XML_EVENTS);
    let mut ids = Vec::new();
    while let Ok(Some(ev)) = reader.next_event() {
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
        if ids.len() > MAX_CHARTS_PER_SHEET {
            break;
        }
    }
    ids
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
    is_two_cell: bool,
    from: AnchorPoint,
    to: AnchorPoint,
    ext_cx: Option<u64>,
    ext_cy: Option<u64>,
    name: Option<String>,
    chart_rid: Option<String>,
}

fn parse_drawing_anchors(xml: &str) -> Option<Vec<DrawingAnchorRaw>> {
    let mut reader = XmlReader::with_budget(xml, MAX_DRAWING_XML_EVENTS);
    let mut out = Vec::new();
    let mut cur: Option<DrawingAnchorRaw> = None;
    let mut loc_to: Option<bool> = None;
    let mut capture = false;
    let mut text = String::new();

    while let Some(event) = reader.next_event().ok()? {
        match event {
            XmlEvent::Start { name, attrs } => match local_name(name) {
                tag @ ("twoCellAnchor" | "oneCellAnchor") => {
                    cur = Some(DrawingAnchorRaw {
                        is_two_cell: tag == "twoCellAnchor",
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
                "ext"
                    if cur
                        .as_ref()
                        .is_some_and(|a| !a.is_two_cell && loc_to.is_none()) =>
                {
                    if let Some(a) = cur.as_mut().filter(|a| a.ext_cx.is_none()) {
                        a.ext_cx = attrs.get("cx").and_then(|s| s.trim().parse().ok());
                        a.ext_cy = attrs.get("cy").and_then(|s| s.trim().parse().ok());
                    }
                }
                "cNvPr" if cur.is_some() => {
                    if let Some(a) = cur.as_mut().filter(|a| a.name.is_none()) {
                        a.name = attrs
                            .get("name")
                            .map(|s| s.into_owned())
                            .filter(|s| !s.is_empty());
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
                "twoCellAnchor" | "oneCellAnchor" => {
                    if let Some(anchor) = cur.take().filter(|a| a.chart_rid.is_some()) {
                        out.push(anchor);
                        if out.len() > MAX_CHARTS_PER_SHEET {
                            return Some(out);
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
    Some(out)
}

fn emu_u64_to_px(emu: u64) -> u32 {
    ((emu + EMU_PER_PX / 2) / EMU_PER_PX).min(u64::from(u32::MAX)) as u32
}

fn build_anchor_from_raw(
    raw: &DrawingAnchorRaw,
    columns: &BTreeMap<u32, u32>,
) -> Option<ChartAnchor> {
    let (fc, fr) = (raw.from.col?, raw.from.row?);
    if fc >= OoxmlCodec::MAX_XLSX_COLS || fr >= OoxmlCodec::MAX_XLSX_ROWS {
        return None;
    }
    let mut anchor = ChartAnchor {
        from_col: fc,
        from_row: fr,
        col_off_emu: raw.from.col_off,
        row_off_emu: raw.from.row_off,
        ..ChartAnchor::default()
    };
    if raw.is_two_cell {
        let (tc, tr) = (raw.to.col?, raw.to.row?);
        if tc >= OoxmlCodec::MAX_XLSX_COLS || tr >= OoxmlCodec::MAX_XLSX_ROWS {
            return None;
        }
        anchor.to_col = tc;
        anchor.to_row = tr;
        anchor.to_col_off_emu = raw.to.col_off;
        anchor.to_row_off_emu = raw.to.row_off;
        anchor.sync_px_from_grid(columns);
    } else {
        let w = raw
            .ext_cx
            .map_or(DEFAULT_CHART_WIDTH_PX, emu_u64_to_px)
            .clamp(MIN_CHART_WIDTH_PX, MAX_CHART_WIDTH_PX);
        let h = raw
            .ext_cy
            .map_or(DEFAULT_CHART_HEIGHT_PX, emu_u64_to_px)
            .clamp(MIN_CHART_HEIGHT_PX, MAX_CHART_HEIGHT_PX);
        let lx = col_left_px(fc, columns)
            .saturating_add(u64::from(emu_u64_to_px(u64::from(raw.from.col_off))))
            .min(u64::from(u32::MAX)) as u32;
        let ty = row_top_px(fr)
            .saturating_add(u64::from(emu_u64_to_px(u64::from(raw.from.row_off))))
            .min(u64::from(u32::MAX)) as u32;
        anchor.left_px = lx;
        anchor.top_px = ty;
        anchor.width_px = w;
        anchor.height_px = h;
        anchor.sync_grid_from_px(columns);
        anchor.from_col = fc;
        anchor.from_row = fr;
        anchor.col_off_emu = raw.from.col_off;
        anchor.row_off_emu = raw.from.row_off;
    }
    Some(anchor)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SerSlot {
    Tx,
    Cat,
    Val,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TextTarget {
    None,
    Title,
    Formula,
    Value,
}

fn parse_chart_xml(
    xml: &str,
    default_sheet: &str,
    chart_id: String,
    anchor: ChartAnchor,
) -> Option<SheetChart> {
    let mut reader = XmlReader::with_budget(xml, MAX_CHART_XML_EVENTS);
    let (mut chart_d, mut plot_d, mut title_d, mut leg_d, mut bar_d, mut ser_d, mut slot_d) =
        (0, 0, 0, 0, 0, 0, 0);
    let mut slot: Option<SerSlot> = None;
    let mut txt_target = TextTarget::None;

    let mut kind: Option<ChartKind> = None;
    let mut title = String::new();
    let mut legend = ChartLegendPos::None;
    let mut series: Vec<ChartSeriesRef> = Vec::new();
    let mut cur_ser = ChartSeriesRef::default();
    let mut text_buf = String::new();
    let (mut depth, mut pt_count) = (0usize, 0usize);

    while let Some(event) = reader.next_event().ok()? {
        match event {
            XmlEvent::Start { name, attrs } => {
                depth += 1;
                match local_name(name) {
                    "chart" if chart_d == 0 => chart_d = depth,
                    "plotArea" if chart_d > 0 && plot_d == 0 => plot_d = depth,
                    "title" if chart_d > 0 && plot_d == 0 && title_d == 0 => title_d = depth,
                    "legend" if chart_d > 0 && plot_d == 0 && leg_d == 0 => {
                        leg_d = depth;
                        legend = ChartLegendPos::Right;
                    }
                    "legendPos" if leg_d > 0 => {
                        if let Some(val) = attrs.get("val") {
                            legend = ChartLegendPos::from_ooxml(&val);
                        }
                    }
                    "t" | "v" if title_d > 0 => {
                        txt_target = TextTarget::Title;
                        text_buf.clear();
                    }
                    "barChart" | "bar3DChart" if plot_d > 0 && kind.is_none() => {
                        kind = Some(ChartKind::Column);
                        bar_d = depth;
                    }
                    "barDir" if bar_d > 0 && attrs.get("val").as_deref() == Some("bar") => {
                        kind = Some(ChartKind::Bar);
                    }
                    "lineChart" | "line3DChart" if plot_d > 0 && kind.is_none() => {
                        kind = Some(ChartKind::Line);
                    }
                    "pieChart" | "pie3DChart" | "doughnutChart" if plot_d > 0 && kind.is_none() => {
                        kind = Some(ChartKind::Pie);
                    }
                    "scatterChart" if plot_d > 0 && kind.is_none() => {
                        kind = Some(ChartKind::Scatter);
                    }
                    "ser" if plot_d > 0 && ser_d == 0 => {
                        if series.len() >= MAX_SERIES_PER_CHART {
                            return None;
                        }
                        ser_d = depth;
                        cur_ser = ChartSeriesRef::default();
                        slot = None;
                        slot_d = 0;
                    }
                    tag @ ("tx" | "cat" | "xVal" | "val" | "yVal")
                        if ser_d > 0 && depth == ser_d + 1 && slot.is_none() =>
                    {
                        slot = Some(match tag {
                            "tx" => SerSlot::Tx,
                            "cat" | "xVal" => SerSlot::Cat,
                            _ => SerSlot::Val,
                        });
                        slot_d = depth;
                        pt_count = 0;
                    }
                    "ptCount" if slot.is_some() => {
                        let count = attrs
                            .get("val")
                            .and_then(|s| s.trim().parse().ok())
                            .unwrap_or(0);
                        if count > MAX_POINTS_PER_SERIES {
                            return None;
                        }
                    }
                    "pt" if slot.is_some() => {
                        pt_count += 1;
                        if pt_count > MAX_POINTS_PER_SERIES {
                            return None;
                        }
                    }
                    "f" if slot.is_some() => {
                        txt_target = TextTarget::Formula;
                        text_buf.clear();
                    }
                    "v" if slot.is_some() => {
                        txt_target = TextTarget::Value;
                        text_buf.clear();
                    }
                    _ => {}
                }
            }
            XmlEvent::Text(t) if txt_target != TextTarget::None => {
                if text_buf.len() + t.len() > MAX_CHART_TEXT_BYTES * 2 {
                    return None;
                }
                text_buf.push_str(&t);
            }
            XmlEvent::End(name) => {
                let loc = local_name(name);
                match (txt_target, loc) {
                    (TextTarget::Title, "t" | "v") => {
                        txt_target = TextTarget::None;
                        if title.len() + text_buf.len() > MAX_CHART_TEXT_BYTES {
                            return None;
                        }
                        title.push_str(&text_buf);
                        text_buf.clear();
                    }
                    (TextTarget::Formula, "f") => {
                        txt_target = TextTarget::None;
                        let f = text_buf.trim().to_string();
                        text_buf.clear();
                        if f.len() > MAX_CHART_TEXT_BYTES {
                            return None;
                        }
                        match slot {
                            Some(SerSlot::Tx) => cur_ser.name_ref = (!f.is_empty()).then_some(f),
                            Some(SerSlot::Cat) => {
                                cur_ser.categories_ref = (!f.is_empty()).then_some(f);
                            }
                            Some(SerSlot::Val) => cur_ser.values_ref = f,
                            None => {}
                        }
                    }
                    (TextTarget::Value, "v") => {
                        txt_target = TextTarget::None;
                        if slot == Some(SerSlot::Tx) && cur_ser.name.is_empty() {
                            let val_text = text_buf.trim().to_string();
                            if val_text.len() > MAX_CHART_TEXT_BYTES {
                                return None;
                            }
                            cur_ser.name = val_text;
                        }
                        text_buf.clear();
                    }
                    _ => {}
                }

                if depth == slot_d {
                    slot = None;
                    slot_d = 0;
                }
                if depth == ser_d {
                    if !cur_ser.within_bounds() {
                        return None;
                    }
                    series.push(std::mem::take(&mut cur_ser));
                    ser_d = 0;
                }
                if depth == bar_d {
                    bar_d = 0;
                }
                if depth == title_d {
                    title_d = 0;
                }
                if depth == leg_d {
                    leg_d = 0;
                }
                if depth == plot_d {
                    plot_d = 0;
                }
                if depth == chart_d {
                    chart_d = 0;
                }
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }

    let kind = kind?;
    let (data_range, switch_row_col) = infer_data_range_and_orientation(default_sheet, &series);
    let chart = SheetChart {
        id: chart_id,
        kind,
        title,
        data_range,
        switch_row_col,
        legend,
        anchor,
        series,
    };
    chart.within_bounds().then_some(chart)
}

fn infer_data_range_and_orientation(
    default_sheet: &str,
    series: &[ChartSeriesRef],
) -> (String, bool) {
    let mut union: Option<CellBox> = None;
    let mut ref_sheet: Option<String> = None;
    let mut switch_row_col = false;

    for (idx, ser) in series.iter().enumerate() {
        for r in [
            ser.name_ref.as_deref(),
            ser.categories_ref.as_deref(),
            Some(ser.values_ref.as_str()),
        ]
        .into_iter()
        .flatten()
        {
            let Some((sh, b)) = parse_sheet_and_box(default_sheet, r) else {
                continue;
            };
            if ref_sheet
                .as_ref()
                .is_some_and(|first| !first.eq_ignore_ascii_case(&sh))
            {
                continue;
            }
            if ref_sheet.is_none() {
                ref_sheet = Some(sh);
            }
            if idx == 0 && r == ser.values_ref && b.row0 == b.row1 && b.col1 > b.col0 {
                switch_row_col = true;
            }
            union = Some(match union {
                None => b,
                Some(p) => CellBox {
                    row0: p.row0.min(b.row0),
                    col0: p.col0.min(b.col0),
                    row1: p.row1.max(b.row1),
                    col1: p.col1.max(b.col1),
                },
            });
        }
    }
    (
        union.map_or_else(String::new, |b| b.to_a1()),
        switch_row_col,
    )
}

fn resolve_rel_str<'a>(
    rels: &HashMap<String, String>,
    dir: &str,
    rid: &str,
    entries: &HashMap<&str, &'a [u8]>,
) -> Option<(String, &'a str)> {
    let target = rels.get(rid)?;
    let path = resolve_zip_target(dir, target)?;
    let xml = entries
        .get(path.as_str())
        .and_then(|d| std::str::from_utf8(d).ok())?;
    Some((path, xml))
}

/// Reads charts attached to worksheet `part_path` into `charts_out`.
pub(crate) fn read_sheet_charts(
    sheet_name: &str,
    sheet_xml: &str,
    part_path: &str,
    entries: &HashMap<&str, &[u8]>,
    columns: &BTreeMap<u32, u32>,
    workbook_total: &mut usize,
    charts_out: &mut Vec<SheetChart>,
) -> Result<u32, EasyOfficeError> {
    let rids = sheet_drawing_rids(sheet_xml);
    if rids.is_empty() {
        return Ok(0);
    }
    let sheet_rels = rels_for_part(entries, part_path, DRAWING_REL_TYPE);
    let sheet_dir = part_path.rsplit_once('/').map_or("", |(d, _)| d);
    let mut rejected = 0u32;
    let mut seen_drawings = HashSet::new();
    let mut seen_in_sheet = 0usize;

    for rid in rids {
        let Some((drawing_path, drawing_xml)) =
            resolve_rel_str(&sheet_rels, sheet_dir, &rid, entries)
        else {
            rejected = rejected.saturating_add(1);
            continue;
        };
        if !seen_drawings.insert(drawing_path.clone()) {
            continue;
        }
        let Some(anchors) = parse_drawing_anchors(drawing_xml) else {
            rejected = rejected.saturating_add(1);
            continue;
        };
        let chart_rels = rels_for_part(entries, &drawing_path, CHART_REL_TYPE);
        let drawing_dir = drawing_path.rsplit_once('/').map_or("", |(d, _)| d);

        for raw_anchor in anchors {
            seen_in_sheet += 1;
            if seen_in_sheet > MAX_CHARTS_PER_SHEET {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "시트 차트 수가 허용 상한({MAX_CHARTS_PER_SHEET})을 초과했습니다."
                )));
            }
            if *workbook_total >= MAX_WORKBOOK_CHARTS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "통합 문서 전체 차트 수가 허용 상한({MAX_WORKBOOK_CHARTS})을 초과했습니다."
                )));
            }
            *workbook_total += 1;

            let parsed_anchor = build_anchor_from_raw(&raw_anchor, columns);
            let chart_xml = raw_anchor
                .chart_rid
                .as_deref()
                .and_then(|cid| resolve_rel_str(&chart_rels, drawing_dir, cid, entries))
                .map(|(_, xml)| xml);
            let chart_id = raw_anchor
                .name
                .as_deref()
                .filter(|s| !s.trim().is_empty() && s.len() <= MAX_CHART_TEXT_BYTES)
                .map_or_else(
                    || format!("chart-{}", charts_out.len() + 1),
                    ToString::to_string,
                );

            if let Some(chart) = parsed_anchor
                .zip(chart_xml)
                .and_then(|(a, xml)| parse_chart_xml(xml, sheet_name, chart_id, a))
            {
                charts_out.push(chart);
            } else {
                rejected = rejected.saturating_add(1);
            }
        }
    }
    Ok(rejected)
}

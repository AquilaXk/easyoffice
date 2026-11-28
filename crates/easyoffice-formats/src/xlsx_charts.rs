// 참조 구현: jmcnamara/rust_xlsxwriter (MIT OR Apache-2.0, Copyright 2022-2026 John McNamara, src/chart.rs, src/drawing.rs; see THIRD_PARTY_NOTICES.md)

//! OOXML SpreadsheetML DrawingML chart part XML codec (`xl/charts/chart*.xml`)
//! with fail-closed budgets and XML sanitization (`PD-254`).

use std::fmt::Write;

use crate::chart_kinds::{axes_xml, plot_groups, STOCK_SERIES_LOOK};
use crate::sheet_styling::sheet_chart::{
    parse_sheet_and_box, ChartAnchor, ChartKind, ChartLegendPos, ChartSeriesRef, SheetCellIndex,
    SheetChart, MAX_CHART_TEXT_BYTES, MAX_POINTS_PER_SERIES, MAX_SERIES_PER_CHART,
};
use crate::xml_stream::{XmlEvent, XmlReader};
use crate::{CellBox, EasyOfficeError, OdsUnpacked, XlsxFormulaCache, XmlSanitizer};

pub(crate) const CHART_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart";
pub(crate) const CHART_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.drawingml.chart+xml";

const AX_ID_CAT: &str = "64052224";
const AX_ID_VAL: &str = "64055552";
const MAX_CHART_XML_EVENTS: usize = 65_536;

#[inline]
pub(crate) fn local_name(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

pub(crate) fn build_chart_xml(
    chart: &SheetChart,
    sheet_name: &str,
    sheets: &[OdsUnpacked],
    caches: &[XlsxFormulaCache],
    cell_index: &SheetCellIndex<'_>,
) -> String {
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
    let series_list = chart.resolved_series(sheet_name, sheets);
    let series_list = &series_list[..series_list.len().min(MAX_SERIES_PER_CHART)];
    for group in plot_groups(chart.kind, series_list.len()) {
        let _ = write!(xml, "<{}>{}", group.tag, group.header);
        push_series_list(
            &mut xml,
            series_list,
            sheet_name,
            caches,
            cell_index,
            (chart.kind, group.series.clone()),
        );
        xml.push_str(group.tail);
        if group.has_axes {
            let _ = write!(
                xml,
                "<c:axId val=\"{AX_ID_CAT}\"/><c:axId val=\"{AX_ID_VAL}\"/>"
            );
        }
        let _ = write!(xml, "</{}>", group.tag);
    }
    xml.push_str(&axes_xml(chart.kind));
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
    xml: &mut String,
    series_list: &[ChartSeriesRef],
    sheet_name: &str,
    caches: &[XlsxFormulaCache],
    cell_index: &SheetCellIndex<'_>,
    (kind, emitted): (ChartKind, std::ops::Range<usize>),
) {
    let (cat_tag, val_tag) = if kind.has_numeric_x() {
        ("c:xVal", "c:yVal")
    } else {
        ("c:cat", "c:val")
    };
    // A bubble chart reads its series in pairs: the Y values, then the sizes.
    let step = if kind == ChartKind::Bubble { 2 } else { 1 };
    for idx in emitted {
        let ser = &series_list[idx * step];
        let resolved = cell_index.resolve_series(sheet_name, caches, ser);
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
        if kind == ChartKind::Stock {
            xml.push_str(STOCK_SERIES_LOOK);
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
        let _ = write!(xml, "</{val_tag}>");
        if let Some(size_ser) = series_list.get(idx * step + 1).filter(|_| step == 2) {
            let sizes = cell_index.resolve_series(sheet_name, caches, size_ser);
            xml.push_str("<c:bubbleSize>");
            if size_ser.values_ref.is_empty() {
                xml.push_str("<c:numLit><c:formatCode>General</c:formatCode><c:ptCount val=\"0\"/></c:numLit>");
            } else {
                let _ = write!(
                    xml,
                    "<c:numRef><c:f>{}</c:f>",
                    XmlSanitizer::escape(&size_ser.values_ref)
                );
                push_num_cache(xml, &sizes.values);
                xml.push_str("</c:numRef>");
            }
            xml.push_str("</c:bubbleSize>");
        }
        xml.push_str("</c:ser>");
    }
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum SerSlot {
    Tx,
    Cat,
    Val,
    Size,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TextTarget {
    None,
    Title,
    Formula,
    Value,
}

pub(crate) fn parse_chart_xml(
    xml: &str,
    default_sheet: &str,
    chart_id: String,
    anchor: ChartAnchor,
) -> Result<Option<SheetChart>, EasyOfficeError> {
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
    let mut size_ref: Option<String> = None;
    let mut text_buf = String::new();
    let (mut depth, mut pt_count) = (0usize, 0usize);

    while let Some(event) = reader.next_event()? {
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
                    // A line group after a bar group of one series is a combo (PD-402).
                    "lineChart" | "line3DChart"
                        if plot_d > 0 && kind == Some(ChartKind::Column) && series.len() == 1 =>
                    {
                        kind = Some(ChartKind::Combo);
                    }
                    "pieChart" | "pie3DChart" if plot_d > 0 && kind.is_none() => {
                        kind = Some(ChartKind::Pie);
                    }
                    "doughnutChart" if plot_d > 0 && kind.is_none() => {
                        kind = Some(ChartKind::Doughnut);
                    }
                    "radarChart" if plot_d > 0 && kind.is_none() => {
                        kind = Some(ChartKind::Radar);
                    }
                    "bubbleChart" if plot_d > 0 && kind.is_none() => {
                        kind = Some(ChartKind::Bubble);
                    }
                    "stockChart" if plot_d > 0 && kind.is_none() => {
                        kind = Some(ChartKind::Stock);
                    }
                    "scatterChart" if plot_d > 0 && kind.is_none() => {
                        kind = Some(ChartKind::Scatter);
                    }
                    "ser" if plot_d > 0 && ser_d == 0 => {
                        if series.len() >= MAX_SERIES_PER_CHART {
                            return Ok(None);
                        }
                        ser_d = depth;
                        cur_ser = ChartSeriesRef::default();
                        slot = None;
                        slot_d = 0;
                    }
                    tag @ ("tx" | "cat" | "xVal" | "val" | "yVal" | "bubbleSize")
                        if ser_d > 0 && depth == ser_d + 1 && slot.is_none() =>
                    {
                        slot = Some(match tag {
                            "tx" => SerSlot::Tx,
                            "cat" | "xVal" => SerSlot::Cat,
                            "bubbleSize" => SerSlot::Size,
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
                            return Ok(None);
                        }
                    }
                    "pt" if slot.is_some() => {
                        pt_count += 1;
                        if pt_count > MAX_POINTS_PER_SERIES {
                            return Ok(None);
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
                    return Ok(None);
                }
                text_buf.push_str(&t);
            }
            XmlEvent::End(name) => {
                let loc = local_name(name);
                match (txt_target, loc) {
                    (TextTarget::Title, "t" | "v") => {
                        txt_target = TextTarget::None;
                        if title.len() + text_buf.len() > MAX_CHART_TEXT_BYTES {
                            return Ok(None);
                        }
                        title.push_str(&text_buf);
                        text_buf.clear();
                    }
                    (TextTarget::Formula, "f") => {
                        txt_target = TextTarget::None;
                        let f = text_buf.trim().to_string();
                        text_buf.clear();
                        if f.len() > MAX_CHART_TEXT_BYTES {
                            return Ok(None);
                        }
                        match slot {
                            Some(SerSlot::Tx) => cur_ser.name_ref = (!f.is_empty()).then_some(f),
                            Some(SerSlot::Cat) => {
                                cur_ser.categories_ref = (!f.is_empty()).then_some(f);
                            }
                            Some(SerSlot::Val) => cur_ser.values_ref = f,
                            Some(SerSlot::Size) => size_ref = (!f.is_empty()).then_some(f),
                            None => {}
                        }
                    }
                    (TextTarget::Value, "v") => {
                        txt_target = TextTarget::None;
                        if slot == Some(SerSlot::Tx) && cur_ser.name.is_empty() {
                            let val_text = text_buf.trim().to_string();
                            if val_text.len() > MAX_CHART_TEXT_BYTES {
                                return Ok(None);
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
                        return Ok(None);
                    }
                    // The sizes of a bubble series are the next series of the model (see `ChartKind`).
                    let sizes = size_ref.take().map(|values_ref| ChartSeriesRef {
                        name: format!("{} (크기)", cur_ser.name),
                        categories_ref: cur_ser.categories_ref.clone(),
                        values_ref,
                        ..ChartSeriesRef::default()
                    });
                    series.push(std::mem::take(&mut cur_ser));
                    if let Some(sizes) = sizes {
                        if series.len() >= MAX_SERIES_PER_CHART || !sizes.within_bounds() {
                            return Ok(None);
                        }
                        series.push(sizes);
                    }
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

    let Some(kind) = kind else {
        return Ok(None);
    };
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
    Ok(chart.within_bounds().then_some(chart))
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

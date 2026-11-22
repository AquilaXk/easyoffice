//! JSON parser, serializer, and row/column shift helpers for sheet charts (`PD-254`).

use std::fmt::Write;

use crate::EasyOfficeIpcRouter;
use easyoffice_formats::{
    ref_point_count, AnchorEdit, ChartAnchor, ChartKind, ChartLegendPos, ChartSeriesRef,
    EasyOfficeError, SheetChart, SheetGrid, StrictJsonValue, MAX_CHARTS_PER_SHEET,
    MAX_CHART_TEXT_BYTES, MAX_POINTS_PER_SERIES, MAX_SERIES_PER_CHART, MAX_SHEET_IMAGE_DIM_PX,
};
use easyoffice_sheet::axis::rewrite_formula_refs;
use easyoffice_sheet::{Axis, AxisEdit};

use crate::sheet_media_notes_json::shift_anchor_span;

const LAST_ROW: u32 = 1_048_575;
const LAST_COL: u32 = 16_383;
const MAX_POS_PX: u64 = 50_000_000;
/// The largest offset inside a cell a page may send, in EMU (the image anchors take as much).
const MAX_OFFSET_EMU: u64 = 0x7fff_ffff;

fn bad(what: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(format!("차트 JSON이 올바르지 않습니다: {what}"))
}

fn over(msg: &str) -> EasyOfficeError {
    EasyOfficeError::ResourceBudgetExceeded(msg.to_string())
}

fn text_opt(
    obj: &[(String, StrictJsonValue)],
    key: &str,
) -> Result<Option<String>, EasyOfficeError> {
    match StrictJsonValue::get_field(obj, key) {
        None | Some(StrictJsonValue::Null) => Ok(None),
        Some(StrictJsonValue::Str(s)) => {
            if s.len() > MAX_CHART_TEXT_BYTES {
                return Err(over(&format!(
                    "차트 '{key}' 길이가 허용 상한({MAX_CHART_TEXT_BYTES}B)을 초과했습니다."
                )));
            }
            Ok(Some(s.clone()))
        }
        Some(_) => Err(bad(key)),
    }
}

fn parse_anchor(
    v: Option<&StrictJsonValue>,
    grid: &SheetGrid<'_>,
) -> Result<ChartAnchor, EasyOfficeError> {
    let Some(val) = v.filter(|x| !matches!(x, StrictJsonValue::Null)) else {
        let mut a = ChartAnchor::default();
        a.sync_grid_from_px(grid);
        return Ok(a);
    };
    let obj = val.as_object().ok_or_else(|| bad("anchor"))?;
    let opt_u32 = |key: &str, lo: u64, hi: u64| -> Result<Option<u32>, EasyOfficeError> {
        match StrictJsonValue::get_field(obj, key) {
            None | Some(StrictJsonValue::Null) => Ok(None),
            Some(v) => v
                .as_u64()
                .filter(|n| (lo..=hi).contains(n))
                .map(|n| n as u32)
                .map(Some)
                .ok_or_else(|| bad(key)),
        }
    };

    let from_col = opt_u32("fromCol", 0, u64::from(LAST_COL))?;
    let from_row = opt_u32("fromRow", 0, u64::from(LAST_ROW))?;
    let to_col = opt_u32("toCol", 0, u64::from(LAST_COL))?;
    let to_row = opt_u32("toRow", 0, u64::from(LAST_ROW))?;
    let left = opt_u32("left", 0, MAX_POS_PX)?;
    let top = opt_u32("top", 0, MAX_POS_PX)?;
    // A chart read from a file may be smaller or larger than the page lets a user drag one to; its size comes back as it went.
    let width = opt_u32("width", 1, u64::from(MAX_SHEET_IMAGE_DIM_PX))?;
    let height = opt_u32("height", 1, u64::from(MAX_SHEET_IMAGE_DIM_PX))?;

    let offset = |key: &str| opt_u32(key, 0, MAX_OFFSET_EMU).map(Option::unwrap_or_default);
    let mut anchor = ChartAnchor {
        edit_as: crate::sheet_media_notes_json::parse_edit_as(obj),
        from_col: from_col.unwrap_or(0),
        from_row: from_row.unwrap_or(0),
        to_col: to_col.unwrap_or(0),
        to_row: to_row.unwrap_or(0),
        col_off_emu: offset("colOffEmu")?,
        row_off_emu: offset("rowOffEmu")?,
        to_col_off_emu: offset("toColOffEmu")?,
        to_row_off_emu: offset("toRowOffEmu")?,
        left_px: left.unwrap_or(0),
        top_px: top.unwrap_or(0),
        width_px: width.unwrap_or(420),
        height_px: height.unwrap_or(260),
    };

    if anchor.has_explicit_grid() {
        let mut derived = anchor;
        derived.sync_px_from_grid(grid);
        if left.is_none() {
            anchor.left_px = derived.left_px;
        }
        if top.is_none() {
            anchor.top_px = derived.top_px;
        }
        if width.is_none() {
            anchor.width_px = derived.width_px;
        }
        if height.is_none() {
            anchor.height_px = derived.height_px;
        }
    } else {
        anchor.sync_grid_from_px(grid);
    }

    Ok(anchor)
}

fn parse_series(v: Option<&StrictJsonValue>) -> Result<Vec<ChartSeriesRef>, EasyOfficeError> {
    let Some(val) = v.filter(|x| !matches!(x, StrictJsonValue::Null)) else {
        return Ok(Vec::new());
    };
    let items = val.as_array().ok_or_else(|| bad("series"))?;
    if items.len() > MAX_SERIES_PER_CHART {
        return Err(over(&format!(
            "차트 계열 수({})가 허용 상한({MAX_SERIES_PER_CHART})을 초과했습니다.",
            items.len()
        )));
    }
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let obj = item.as_object().ok_or_else(|| bad("series"))?;
        let name = text_opt(obj, "name")?.unwrap_or_default();
        let name_ref = text_opt(obj, "nameRef")?.filter(|s| !s.trim().is_empty());
        let categories_ref = text_opt(obj, "categoriesRef")?.filter(|s| !s.trim().is_empty());
        let values_ref = text_opt(obj, "valuesRef")?.unwrap_or_default();

        if ref_point_count(&values_ref).is_some_and(|n| n > MAX_POINTS_PER_SERIES as u64)
            || categories_ref
                .as_deref()
                .and_then(ref_point_count)
                .is_some_and(|n| n > MAX_POINTS_PER_SERIES as u64)
        {
            return Err(over(&format!(
                "차트 계열 포인트 수가 허용 상한({MAX_POINTS_PER_SERIES})을 초과했습니다."
            )));
        }

        out.push(ChartSeriesRef {
            name,
            name_ref,
            categories_ref,
            values_ref,
        });
    }
    Ok(out)
}

pub(crate) fn parse_charts(
    v: &StrictJsonValue,
    grid: &SheetGrid<'_>,
) -> Result<Vec<SheetChart>, EasyOfficeError> {
    let items = v.as_array().ok_or_else(|| bad("charts"))?;
    if items.len() > MAX_CHARTS_PER_SHEET {
        return Err(over(&format!(
            "시트 차트 수({})가 허용 상한({MAX_CHARTS_PER_SHEET})을 초과했습니다.",
            items.len()
        )));
    }
    let mut out = Vec::with_capacity(items.len());
    for (idx, item) in items.iter().enumerate() {
        let obj = item.as_object().ok_or_else(|| bad("charts"))?;
        let id = match text_opt(obj, "id")? {
            Some(s) if !s.trim().is_empty() => s,
            Some(_) => return Err(bad("id")),
            None => format!("chart-{}", idx + 1),
        };
        let kind = StrictJsonValue::get_field(obj, "chartType")
            .and_then(StrictJsonValue::as_str)
            .and_then(ChartKind::parse)
            .ok_or_else(|| bad("chartType"))?;
        let title = text_opt(obj, "title")?.unwrap_or_default();
        let data_range = text_opt(obj, "dataRange")?.unwrap_or_default();
        let switch_row_col = match StrictJsonValue::get_field(obj, "switchRowCol") {
            None | Some(StrictJsonValue::Null) => false,
            Some(StrictJsonValue::Bool(b)) => *b,
            Some(_) => return Err(bad("switchRowCol")),
        };
        let legend = match StrictJsonValue::get_field(obj, "legend") {
            None | Some(StrictJsonValue::Null) => ChartLegendPos::Right,
            Some(StrictJsonValue::Str(s)) => {
                ChartLegendPos::parse(s).ok_or_else(|| bad("legend"))?
            }
            Some(_) => return Err(bad("legend")),
        };
        let anchor = parse_anchor(StrictJsonValue::get_field(obj, "anchor"), grid)?;
        let series = parse_series(StrictJsonValue::get_field(obj, "series"))?;
        let chart = SheetChart {
            id,
            kind,
            title,
            data_range,
            switch_row_col,
            legend,
            anchor,
            series,
        };
        if !chart.within_bounds() {
            return Err(over(
                "차트 포인트 수 또는 크기 예산이 허용 상한을 초과했습니다.",
            ));
        }
        out.push(chart);
    }
    Ok(out)
}

fn write_opt_str(out: &mut String, val: Option<&str>) {
    match val {
        Some(s) => {
            out.push('"');
            EasyOfficeIpcRouter::escape_json_into(out, s);
            out.push('"');
        }
        None => out.push_str("null"),
    }
}

pub(crate) fn write_chart_json(out: &mut String, chart: &SheetChart, include_series_count: bool) {
    out.push_str("{\"id\":\"");
    EasyOfficeIpcRouter::escape_json_into(out, &chart.id);
    let _ = write!(
        out,
        "\",\"chartType\":\"{}\",\"title\":\"",
        chart.kind.as_str()
    );
    EasyOfficeIpcRouter::escape_json_into(out, &chart.title);
    out.push_str("\",\"dataRange\":\"");
    EasyOfficeIpcRouter::escape_json_into(out, &chart.data_range);
    let _ = write!(
        out,
        "\",\"switchRowCol\":{},\"legend\":\"{}\"",
        chart.switch_row_col,
        chart.legend.as_str()
    );
    if include_series_count {
        let _ = write!(out, ",\"seriesCount\":{}", chart.series.len());
    }
    let a = &chart.anchor;
    let _ = write!(
        out,
        ",\"anchor\":{{\"fromCol\":{},\"fromRow\":{},\"colOffEmu\":{},\"rowOffEmu\":{},\"toCol\":{},\"toRow\":{},\"toColOffEmu\":{},\"toRowOffEmu\":{},\"left\":{},\"top\":{},\"width\":{},\"height\":{}",
        a.from_col,
        a.from_row,
        a.col_off_emu,
        a.row_off_emu,
        a.to_col,
        a.to_row,
        a.to_col_off_emu,
        a.to_row_off_emu,
        a.left_px,
        a.top_px,
        a.width_px,
        a.height_px
    );
    crate::sheet_media_notes_json::write_edit_as(out, a.edit_as);
    out.push_str("},\"series\":[");
    for (i, s) in chart.series.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"name\":\"");
        EasyOfficeIpcRouter::escape_json_into(out, &s.name);
        out.push_str("\",\"nameRef\":");
        write_opt_str(out, s.name_ref.as_deref());
        out.push_str(",\"categoriesRef\":");
        write_opt_str(out, s.categories_ref.as_deref());
        out.push_str(",\"valuesRef\":\"");
        EasyOfficeIpcRouter::escape_json_into(out, &s.values_ref);
        out.push_str("\"}");
    }
    out.push_str("]}");
}

pub(crate) fn charts_json(charts: &[SheetChart]) -> String {
    let mut out = String::with_capacity(64 + charts.len() * 256);
    out.push('[');
    for (i, ch) in charts.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write_chart_json(&mut out, ch, false);
    }
    out.push(']');
    out
}

/// Moves the data range and the series references of `chart`, a chart of the sheet `own`, for an `edit` of `sheet`: the
/// rule of a formula. A reference without a sheet name points at `own`. True when a reference changed.
pub(crate) fn shift_chart_refs(
    chart: &mut SheetChart,
    own: &str,
    sheet: &str,
    edit: &AxisEdit,
) -> bool {
    let shift = |text: &mut String| {
        rewrite_formula_refs(text, own, sheet, edit, false).is_some_and(|moved| {
            *text = moved;
            true
        })
    };
    let mut changed = shift(&mut chart.data_range);
    for s in &mut chart.series {
        for text in [&mut s.name_ref, &mut s.categories_ref]
            .into_iter()
            .flatten()
        {
            changed |= shift(text);
        }
        changed |= shift(&mut s.values_ref);
    }
    changed
}

/// The sheets a request may name in `chartRefs` (a workbook holds 256) and the charts of all of them together.
const MAX_REF_SHEETS: usize = 256;
const MAX_REF_CHARTS: usize = 1_024;

/// One chart of `chartRefs`: its id and the references the edit may move (the chart is not anywhere else of the request).
fn parse_chart_refs_item(item: &StrictJsonValue) -> Result<SheetChart, EasyOfficeError> {
    let obj = item.as_object().ok_or_else(|| bad("chartRefs"))?;
    let id = text_opt(obj, "id")?
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| bad("id"))?;
    let switch_row_col = match StrictJsonValue::get_field(obj, "switchRowCol") {
        None | Some(StrictJsonValue::Null) => false,
        Some(StrictJsonValue::Bool(b)) => *b,
        Some(_) => return Err(bad("switchRowCol")),
    };
    let chart = SheetChart {
        id,
        data_range: text_opt(obj, "dataRange")?.unwrap_or_default(),
        switch_row_col,
        series: parse_series(StrictJsonValue::get_field(obj, "series"))?,
        ..SheetChart::default()
    };
    if !chart.within_bounds() {
        return Err(over(
            "차트 포인트 수 또는 크기 예산이 허용 상한을 초과했습니다.",
        ));
    }
    Ok(chart)
}

/// `chartRefs` of a row or column edit of the sheet `edited`: `{sheet: [chart, ...]}`, the references of the charts of the
/// other sheets. The sheets, the charts of a sheet and the charts of all of them are bounded before any is read, and the
/// edited sheet is not a member (its charts are `charts` and would move twice).
pub(crate) fn parse_chart_refs(
    v: &StrictJsonValue,
    edited: &str,
) -> Result<Vec<(String, Vec<SheetChart>)>, EasyOfficeError> {
    let sheets = v.as_object().ok_or_else(|| bad("chartRefs"))?;
    if sheets.len() > MAX_REF_SHEETS {
        return Err(over(&format!(
            "차트가 있는 시트 수({})가 허용 상한({MAX_REF_SHEETS})을 초과했습니다.",
            sheets.len()
        )));
    }
    let mut total = 0usize;
    let mut out = Vec::with_capacity(sheets.len());
    for (name, list) in sheets {
        let items = list.as_array().ok_or_else(|| bad("chartRefs"))?;
        total = total.saturating_add(items.len());
        if name.eq_ignore_ascii_case(edited) {
            return Err(bad("chartRefs"));
        }
        if items.len() > MAX_CHARTS_PER_SHEET || total > MAX_REF_CHARTS {
            return Err(over(&format!(
                "다른 시트의 차트 수가 허용 상한(시트당 {MAX_CHARTS_PER_SHEET}개, 전체 {MAX_REF_CHARTS}개)을 초과했습니다."
            )));
        }
        let charts = items
            .iter()
            .map(parse_chart_refs_item)
            .collect::<Result<_, _>>()?;
        out.push((name.clone(), charts));
    }
    Ok(out)
}

/// The sheets of `charts` with a chart that points at the cells of `edited` the `edit` moves, their charts moved. The sheets
/// whose references stay are left out.
pub(crate) fn shift_other_charts(
    charts: Vec<(String, Vec<SheetChart>)>,
    edited: &str,
    edit: &AxisEdit,
) -> Vec<(String, Vec<SheetChart>)> {
    charts
        .into_iter()
        .filter_map(|(name, mut list)| {
            let mut changed = false;
            for chart in &mut list {
                changed |= shift_chart_refs(chart, &name, edited, edit);
            }
            changed.then_some((name, list))
        })
        .collect()
}

/// `{sheet: [{id, dataRange, series: [{nameRef, categoriesRef, valuesRef}]}]}`, all the charts of each sheet in the order
/// they came, so the page puts each reference back on the chart of the same place.
pub(crate) fn chart_refs_json(sheets: &[(String, Vec<SheetChart>)]) -> String {
    let mut out = String::from("{");
    for (i, (name, charts)) in sheets.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('"');
        EasyOfficeIpcRouter::escape_json_into(&mut out, name);
        out.push_str("\":[");
        for (j, chart) in charts.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            out.push_str("{\"id\":\"");
            EasyOfficeIpcRouter::escape_json_into(&mut out, &chart.id);
            out.push_str("\",\"dataRange\":\"");
            EasyOfficeIpcRouter::escape_json_into(&mut out, &chart.data_range);
            out.push_str("\",\"series\":[");
            for (k, s) in chart.series.iter().enumerate() {
                if k > 0 {
                    out.push(',');
                }
                out.push_str("{\"nameRef\":");
                write_opt_str(&mut out, s.name_ref.as_deref());
                out.push_str(",\"categoriesRef\":");
                write_opt_str(&mut out, s.categories_ref.as_deref());
                out.push_str(",\"valuesRef\":\"");
                EasyOfficeIpcRouter::escape_json_into(&mut out, &s.values_ref);
                out.push_str("\"}");
            }
            out.push_str("]}");
        }
        out.push(']');
    }
    out.push('}');
    out
}

pub(crate) fn shift_charts(
    charts: &[SheetChart],
    sheet: &str,
    edit: &AxisEdit,
    grid: &SheetGrid<'_>,
) -> Vec<SheetChart> {
    charts
        .iter()
        .map(|ch| {
            let mut next = ch.clone();
            shift_chart_refs(&mut next, sheet, sheet, edit);
            // An absolute anchor stays where it is; the others move with their cells (a one-cell one keeps its size).
            if next.anchor.edit_as != AnchorEdit::Absolute {
                match edit.axis {
                    Axis::Row => {
                        let (fr, tr) =
                            shift_anchor_span(next.anchor.from_row, next.anchor.to_row, edit);
                        next.anchor.from_row = fr;
                        next.anchor.to_row = tr;
                    }
                    Axis::Col => {
                        let (fc, tc) =
                            shift_anchor_span(next.anchor.from_col, next.anchor.to_col, edit);
                        next.anchor.from_col = fc;
                        next.anchor.to_col = tc;
                    }
                }
            }
            match next.anchor.edit_as {
                AnchorEdit::TwoCell => next.anchor.sync_px_from_grid(grid),
                AnchorEdit::OneCell => next
                    .anchor
                    .start_on_cell(grid, ChartAnchor::sync_grid_from_px),
                AnchorEdit::Absolute => next.anchor.sync_grid_from_px(grid),
            }
            next
        })
        .collect()
}

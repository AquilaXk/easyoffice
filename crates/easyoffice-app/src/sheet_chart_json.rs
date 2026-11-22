//! JSON parser, serializer, and row/column shift helpers for sheet charts (`PD-254`).

use std::fmt::Write;

use crate::EasyOfficeIpcRouter;
use easyoffice_formats::{
    ref_point_count, ChartAnchor, ChartKind, ChartLegendPos, ChartSeriesRef, EasyOfficeError,
    SheetChart, SheetGrid, StrictJsonValue, MAX_CHARTS_PER_SHEET, MAX_CHART_HEIGHT_PX,
    MAX_CHART_TEXT_BYTES, MAX_CHART_WIDTH_PX, MAX_POINTS_PER_SERIES, MAX_SERIES_PER_CHART,
    MIN_CHART_HEIGHT_PX, MIN_CHART_WIDTH_PX,
};
use easyoffice_sheet::axis::rewrite_formula_refs;
use easyoffice_sheet::{Axis, AxisEdit, EditKind};

const LAST_ROW: u32 = 1_048_575;
const LAST_COL: u32 = 16_383;
const MAX_POS_PX: u64 = 50_000_000;

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
    let width = opt_u32(
        "width",
        u64::from(MIN_CHART_WIDTH_PX),
        u64::from(MAX_CHART_WIDTH_PX),
    )?;
    let height = opt_u32(
        "height",
        u64::from(MIN_CHART_HEIGHT_PX),
        u64::from(MAX_CHART_HEIGHT_PX),
    )?;

    let mut anchor = ChartAnchor {
        from_col: from_col.unwrap_or(0),
        from_row: from_row.unwrap_or(0),
        to_col: to_col.unwrap_or(0),
        to_row: to_row.unwrap_or(0),
        col_off_emu: 0,
        row_off_emu: 0,
        to_col_off_emu: 0,
        to_row_off_emu: 0,
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
        ",\"anchor\":{{\"fromCol\":{},\"fromRow\":{},\"toCol\":{},\"toRow\":{},\"left\":{},\"top\":{},\"width\":{},\"height\":{}}},\"series\":[",
        a.from_col, a.from_row, a.to_col, a.to_row, a.left_px, a.top_px, a.width_px, a.height_px
    );
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

fn shift_anchor_span(lo: u32, hi: u32, edit: &AxisEdit) -> (u32, u32) {
    let limit = match edit.axis {
        Axis::Row => LAST_ROW,
        Axis::Col => LAST_COL,
    };
    match edit.kind {
        EditKind::Insert => {
            if lo >= edit.at {
                (
                    lo.saturating_add(edit.count).min(limit),
                    hi.saturating_add(edit.count).min(limit),
                )
            } else if hi >= edit.at {
                (lo, hi.saturating_add(edit.count).min(limit))
            } else {
                (lo, hi)
            }
        }
        EditKind::Delete => {
            let del_end = edit.at.saturating_add(edit.count.saturating_sub(1));
            if hi < edit.at {
                (lo, hi)
            } else if lo > del_end {
                (lo - edit.count, hi - edit.count)
            } else if lo >= edit.at && hi <= del_end {
                (edit.at.min(limit), edit.at.min(limit))
            } else if lo >= edit.at {
                (edit.at.min(limit), hi - edit.count)
            } else if hi <= del_end {
                (lo, edit.at.saturating_sub(1).max(lo))
            } else {
                (lo, hi - edit.count)
            }
        }
    }
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
            if !next.data_range.is_empty() {
                if let Some(moved) =
                    rewrite_formula_refs(&next.data_range, sheet, sheet, edit, false)
                {
                    next.data_range = moved;
                }
            }
            for s in &mut next.series {
                if let Some(nr) = s.name_ref.as_deref() {
                    if let Some(moved) = rewrite_formula_refs(nr, sheet, sheet, edit, false) {
                        s.name_ref = Some(moved);
                    }
                }
                if let Some(cr) = s.categories_ref.as_deref() {
                    if let Some(moved) = rewrite_formula_refs(cr, sheet, sheet, edit, false) {
                        s.categories_ref = Some(moved);
                    }
                }
                if !s.values_ref.is_empty() {
                    if let Some(moved) =
                        rewrite_formula_refs(&s.values_ref, sheet, sheet, edit, false)
                    {
                        s.values_ref = moved;
                    }
                }
            }
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
            next.anchor.sync_px_from_grid(grid);
            next
        })
        .collect()
}

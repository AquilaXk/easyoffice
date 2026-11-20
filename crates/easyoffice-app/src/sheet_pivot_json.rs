//! JSON parser, serializer, and row/column structure shift helpers for sheet pivots (`PD-255`).

use std::collections::HashSet;
use std::fmt::Write;

use easyoffice_formats::{
    CellBox, EasyOfficeError, PivotAggKind, PivotFilterCriteria, PivotSlicer, PivotValueField,
    SheetPivotTable, StrictJsonValue, MAX_PIVOTS_PER_SHEET, MAX_PIVOT_AXIS_FIELDS,
    MAX_PIVOT_FILTER_VALUES, MAX_PIVOT_SLICERS, MAX_PIVOT_TEXT_BYTES, MAX_PIVOT_VALUE_FIELDS,
};
use easyoffice_sheet::{shift_boxes, Axis, AxisEdit};

const LAST_ROW: u32 = 1_048_575;
const LAST_COL: u32 = 16_383;
const MAX_PIVOT_FILTERS: usize = 64;

fn bad(what: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(format!("피벗 JSON이 올바르지 않습니다: {what}"))
}

fn over(msg: impl Into<String>) -> EasyOfficeError {
    EasyOfficeError::ResourceBudgetExceeded(msg.into())
}

fn check_text(key: &str, text: &str) -> Result<(), EasyOfficeError> {
    if text.len() > MAX_PIVOT_TEXT_BYTES {
        return Err(over(format!(
            "피벗 '{key}' 길이가 허용 상한({MAX_PIVOT_TEXT_BYTES}B)을 초과했습니다."
        )));
    }
    Ok(())
}

fn opt_text(
    obj: &[(String, StrictJsonValue)],
    key: &str,
) -> Result<Option<String>, EasyOfficeError> {
    match StrictJsonValue::get_field(obj, key) {
        None | Some(StrictJsonValue::Null) => Ok(None),
        Some(StrictJsonValue::Str(s)) => {
            check_text(key, s)?;
            Ok(Some(s.clone()))
        }
        Some(_) => Err(bad(key)),
    }
}

fn opt_bool(
    obj: &[(String, StrictJsonValue)],
    key: &str,
    default: bool,
) -> Result<bool, EasyOfficeError> {
    match StrictJsonValue::get_field(obj, key) {
        None | Some(StrictJsonValue::Null) => Ok(default),
        Some(StrictJsonValue::Bool(b)) => Ok(*b),
        Some(_) => Err(bad(key)),
    }
}

fn parse_u32_array(
    obj: &[(String, StrictJsonValue)],
    key: &str,
    max_len: usize,
) -> Result<Vec<u32>, EasyOfficeError> {
    let Some(val) =
        StrictJsonValue::get_field(obj, key).filter(|v| !matches!(v, StrictJsonValue::Null))
    else {
        return Ok(Vec::new());
    };
    let items = val.as_array().ok_or_else(|| bad(key))?;
    if items.len() > max_len {
        return Err(over(format!(
            "피벗 '{key}' 개수가 허용 상한({max_len})을 초과했습니다."
        )));
    }
    items
        .iter()
        .map(|v| {
            v.as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .filter(|&c| c <= LAST_COL)
                .ok_or_else(|| bad(key))
        })
        .collect()
}

fn parse_value_fields(
    obj: &[(String, StrictJsonValue)],
) -> Result<Vec<PivotValueField>, EasyOfficeError> {
    let Some(val) = StrictJsonValue::get_field(obj, "valueFields")
        .filter(|v| !matches!(v, StrictJsonValue::Null))
    else {
        return Ok(Vec::new());
    };
    let items = val.as_array().ok_or_else(|| bad("valueFields"))?;
    if items.len() > MAX_PIVOT_VALUE_FIELDS {
        return Err(over(format!(
            "피벗 값 필드 수가 허용 상한({MAX_PIVOT_VALUE_FIELDS})을 초과했습니다."
        )));
    }
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let fobj = item.as_object().ok_or_else(|| bad("valueFields"))?;
        let col = StrictJsonValue::get_field(fobj, "col")
            .and_then(StrictJsonValue::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .filter(|&c| c <= LAST_COL)
            .ok_or_else(|| bad("valueFields.col"))?;
        let agg = StrictJsonValue::get_field(fobj, "agg")
            .and_then(StrictJsonValue::as_str)
            .and_then(PivotAggKind::parse)
            .ok_or_else(|| bad("valueFields.agg"))?;
        let name = opt_text(fobj, "name")?.unwrap_or_default();
        out.push(PivotValueField { col, agg, name });
    }
    Ok(out)
}

fn parse_filters(
    obj: &[(String, StrictJsonValue)],
) -> Result<Vec<PivotFilterCriteria>, EasyOfficeError> {
    let Some(val) =
        StrictJsonValue::get_field(obj, "filters").filter(|v| !matches!(v, StrictJsonValue::Null))
    else {
        return Ok(Vec::new());
    };
    let items = val.as_array().ok_or_else(|| bad("filters"))?;
    if items.len() > MAX_PIVOT_FILTERS {
        return Err(over(format!(
            "피벗 필터 수가 허용 상한({MAX_PIVOT_FILTERS})을 초과했습니다."
        )));
    }
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let fobj = item.as_object().ok_or_else(|| bad("filters"))?;
        let col = StrictJsonValue::get_field(fobj, "col")
            .and_then(StrictJsonValue::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .filter(|&c| c <= LAST_COL)
            .ok_or_else(|| bad("filters.col"))?;
        let blank = opt_bool(fobj, "blank", false)?;
        let vals_arr = match StrictJsonValue::get_field(fobj, "values") {
            None | Some(StrictJsonValue::Null) => &[][..],
            Some(v) => v.as_array().ok_or_else(|| bad("filters.values"))?,
        };
        if vals_arr.len() > MAX_PIVOT_FILTER_VALUES {
            return Err(over(format!(
                "피벗 필터 항목 수가 허용 상한({MAX_PIVOT_FILTER_VALUES})을 초과했습니다."
            )));
        }
        let mut values = Vec::with_capacity(vals_arr.len());
        for v in vals_arr {
            let s = v.as_str().ok_or_else(|| bad("filters.values"))?;
            check_text("filters.values", s)?;
            values.push(s.to_string());
        }
        out.push(PivotFilterCriteria { col, values, blank });
    }
    Ok(out)
}

fn parse_slicers(obj: &[(String, StrictJsonValue)]) -> Result<Vec<PivotSlicer>, EasyOfficeError> {
    let Some(val) =
        StrictJsonValue::get_field(obj, "slicers").filter(|v| !matches!(v, StrictJsonValue::Null))
    else {
        return Ok(Vec::new());
    };
    let items = val.as_array().ok_or_else(|| bad("slicers"))?;
    if items.len() > MAX_PIVOT_SLICERS {
        return Err(over(format!(
            "피벗 슬라이서 수가 허용 상한({MAX_PIVOT_SLICERS})을 초과했습니다."
        )));
    }
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let sobj = item.as_object().ok_or_else(|| bad("slicers"))?;
        let u32_of = |key: &str| {
            StrictJsonValue::get_field(sobj, key)
                .and_then(StrictJsonValue::as_u64)
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(|| bad(key))
        };
        let caption = opt_text(sobj, "caption")?
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| bad("slicers.caption"))?;
        let slicer = PivotSlicer {
            col: u32_of("col")?,
            caption,
            left: u32_of("left")?,
            top: u32_of("top")?,
            width: u32_of("width")?,
            height: u32_of("height")?,
            multi_select: opt_bool(sobj, "multiSelect", false)?,
        };
        slicer.check_bounds(0)?;
        out.push(slicer);
    }
    Ok(out)
}

pub(crate) fn parse_pivots(v: &StrictJsonValue) -> Result<Vec<SheetPivotTable>, EasyOfficeError> {
    let items = v.as_array().ok_or_else(|| bad("pivots"))?;
    if items.len() > MAX_PIVOTS_PER_SHEET {
        return Err(over(format!(
            "시트 피벗 테이블 수({})가 허용 상한({MAX_PIVOTS_PER_SHEET})을 초과했습니다.",
            items.len()
        )));
    }
    let mut out = Vec::with_capacity(items.len());
    for (idx, item) in items.iter().enumerate() {
        let obj = item.as_object().ok_or_else(|| bad("pivots"))?;
        let name = match opt_text(obj, "name")? {
            Some(s) if !s.trim().is_empty() => s,
            Some(_) => return Err(bad("name")),
            None => format!("피벗 테이블{}", idx + 1),
        };
        let cache_id = match StrictJsonValue::get_field(obj, "cacheId") {
            None | Some(StrictJsonValue::Null) => 1,
            Some(v) => v
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .filter(|&n| n >= 1)
                .ok_or_else(|| bad("cacheId"))?,
        };
        let source_sheet = opt_text(obj, "sourceSheet")?.unwrap_or_default();
        let source_range = opt_text(obj, "sourceRange")?.unwrap_or_default();
        let source_table = opt_text(obj, "sourceTable")?;
        let has_header = opt_bool(obj, "hasHeader", true)?;
        let location = opt_text(obj, "location")?
            .as_deref()
            .and_then(CellBox::parse)
            .filter(|b| b.row1 <= LAST_ROW && b.col1 <= LAST_COL)
            .ok_or_else(|| bad("location"))?;

        let field_names = match StrictJsonValue::get_field(obj, "fieldNames")
            .filter(|v| !matches!(v, StrictJsonValue::Null))
        {
            None => Vec::new(),
            Some(v) => {
                let arr = v.as_array().ok_or_else(|| bad("fieldNames"))?;
                if arr.len() > (LAST_COL as usize + 1) {
                    return Err(over("피벗 필드 이름이 최대 열 수를 초과했습니다."));
                }
                let mut names = Vec::with_capacity(arr.len());
                for f in arr {
                    let s = f.as_str().ok_or_else(|| bad("fieldNames"))?;
                    check_text("fieldNames", s)?;
                    names.push(s.to_string());
                }
                names
            }
        };

        let pivot = SheetPivotTable {
            name,
            cache_id,
            source_sheet,
            source_range,
            source_table,
            has_header,
            location,
            field_names,
            row_fields: parse_u32_array(obj, "rowFields", MAX_PIVOT_AXIS_FIELDS)?,
            col_fields: parse_u32_array(obj, "colFields", MAX_PIVOT_AXIS_FIELDS)?,
            filter_fields: parse_u32_array(obj, "filterFields", MAX_PIVOT_AXIS_FIELDS)?,
            value_fields: parse_value_fields(obj)?,
            filters: parse_filters(obj)?,
            slicers: parse_slicers(obj)?,
        };
        pivot.check_budgets()?;
        out.push(pivot);
    }
    Ok(out)
}

use easyoffice_formats::json_out::push_json_string as push_json_str_into;

fn push_u32_array(out: &mut String, vals: &[u32]) {
    out.push('[');
    for (i, &v) in vals.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{v}");
    }
    out.push(']');
}

fn push_str_array(out: &mut String, vals: &[String]) {
    out.push('[');
    for (i, s) in vals.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_json_str_into(out, s);
    }
    out.push(']');
}

pub(crate) fn write_pivot_json(out: &mut String, pivot: &SheetPivotTable) {
    out.push_str("{\"name\":");
    push_json_str_into(out, &pivot.name);
    let _ = write!(out, ",\"cacheId\":{},\"sourceSheet\":", pivot.cache_id);
    push_json_str_into(out, &pivot.source_sheet);
    out.push_str(",\"sourceRange\":");
    push_json_str_into(out, &pivot.source_range);
    out.push_str(",\"sourceTable\":");
    match pivot.source_table.as_deref() {
        Some(t) => push_json_str_into(out, t),
        None => out.push_str("null"),
    }
    let _ = write!(out, ",\"hasHeader\":{},\"location\":", pivot.has_header);
    push_json_str_into(out, &pivot.location.to_a1());
    out.push_str(",\"fieldNames\":");
    push_str_array(out, &pivot.field_names);
    out.push_str(",\"rowFields\":");
    push_u32_array(out, &pivot.row_fields);
    out.push_str(",\"colFields\":");
    push_u32_array(out, &pivot.col_fields);
    out.push_str(",\"filterFields\":");
    push_u32_array(out, &pivot.filter_fields);
    out.push_str(",\"valueFields\":[");
    for (i, vf) in pivot.value_fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"col\":{},\"agg\":\"{}\",\"name\":",
            vf.col,
            vf.agg.as_str()
        );
        push_json_str_into(out, &vf.name);
        out.push('}');
    }
    out.push_str("],\"filters\":[");
    for (i, f) in pivot.filters.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{{\"col\":{},\"values\":", f.col);
        push_str_array(out, &f.values);
        let _ = write!(out, ",\"blank\":{}}}", f.blank);
    }
    out.push_str("],\"slicers\":[");
    for (i, s) in pivot.slicers.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{{\"col\":{},\"caption\":", s.col);
        push_json_str_into(out, &s.caption);
        let _ = write!(
            out,
            ",\"left\":{},\"top\":{},\"width\":{},\"height\":{},\"multiSelect\":{}}}",
            s.left, s.top, s.width, s.height, s.multi_select
        );
    }
    out.push_str("]}");
}

pub(crate) fn pivots_json(pivots: &[SheetPivotTable]) -> String {
    let mut out = String::with_capacity(64 + pivots.len() * 256);
    out.push('[');
    for (i, p) in pivots.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write_pivot_json(&mut out, p);
    }
    out.push(']');
    out
}

fn remap_source_columns(
    pivot: &mut SheetPivotTable,
    old_box: CellBox,
    new_box: CellBox,
    edit: &AxisEdit,
) {
    let map_rel = |rel: u32| -> Option<u32> {
        let abs = old_box.col0.checked_add(rel)?;
        let mapped = edit
            .map_index(abs)
            .filter(|c| (new_box.col0..=new_box.col1).contains(c))?;
        Some(mapped - new_box.col0)
    };
    if !pivot.field_names.is_empty() {
        let mut taken: HashSet<String> = HashSet::new();
        let mut names = Vec::with_capacity((new_box.col1 - new_box.col0 + 1) as usize);
        let mut inserted = Vec::new();
        for col in new_box.col0..=new_box.col1 {
            let old = edit
                .unmap_index(col)
                .filter(|c| (old_box.col0..=old_box.col1).contains(c));
            match old.and_then(|c| pivot.field_names.get((c - old_box.col0) as usize)) {
                Some(name) => {
                    taken.insert(name.to_ascii_lowercase());
                    names.push(name.clone());
                }
                None => {
                    inserted.push(names.len());
                    names.push(String::new());
                }
            }
        }
        let mut next = 1usize;
        for at in inserted {
            let name = loop {
                let cand = format!("Column{next}");
                next += 1;
                if taken.insert(cand.to_ascii_lowercase()) {
                    break cand;
                }
            };
            names[at] = name;
        }
        pivot.field_names = names;
    }
    pivot.row_fields = pivot
        .row_fields
        .iter()
        .filter_map(|&c| map_rel(c))
        .collect();
    pivot.col_fields = pivot
        .col_fields
        .iter()
        .filter_map(|&c| map_rel(c))
        .collect();
    pivot.filter_fields = pivot
        .filter_fields
        .iter()
        .filter_map(|&c| map_rel(c))
        .collect();
    pivot.value_fields = pivot
        .value_fields
        .iter()
        .filter_map(|vf| {
            Some(PivotValueField {
                col: map_rel(vf.col)?,
                ..vf.clone()
            })
        })
        .collect();
    pivot.filters = pivot
        .filters
        .iter()
        .filter_map(|f| {
            Some(PivotFilterCriteria {
                col: map_rel(f.col)?,
                ..f.clone()
            })
        })
        .collect();
    pivot.slicers = pivot
        .slicers
        .iter()
        .filter_map(|s| {
            Some(PivotSlicer {
                col: map_rel(s.col)?,
                ..s.clone()
            })
        })
        .collect();
}

pub(crate) fn shift_pivots(
    pivots: &[SheetPivotTable],
    sheet: &str,
    edit: &AxisEdit,
) -> Vec<SheetPivotTable> {
    pivots
        .iter()
        .filter_map(|p| {
            let mut next = p.clone();
            let moved_loc = shift_boxes(&[next.location], edit, false);
            next.location = *moved_loc.first()?;
            let same_sheet =
                next.source_sheet.is_empty() || next.source_sheet.eq_ignore_ascii_case(sheet);
            if same_sheet
                && !next.source_range.trim().is_empty()
                && !next.source_range.contains('!')
            {
                if let Some(old_box) = CellBox::parse(&next.source_range) {
                    let moved_src = shift_boxes(&[old_box], edit, false);
                    let new_box = *moved_src.first()?;
                    next.source_range = new_box.to_a1();
                    if edit.axis == Axis::Col {
                        remap_source_columns(&mut next, old_box, new_box, edit);
                        if next.row_fields.is_empty() && next.value_fields.is_empty() {
                            return None;
                        }
                    }
                }
            }
            Some(next)
        })
        .collect()
}

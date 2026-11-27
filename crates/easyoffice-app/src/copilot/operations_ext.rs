//! 슬라이드 개체·확장 시트·BASE·BOARD 코파일럿 연산 및 온디맨드 읽기 도구의 상한·형식 검증기.

use easyoffice_formats::StrictJsonValue;

use super::op_types::{
    BoardGraphEdge, BoardGraphNode, CopilotOperation, CopilotToolCall, SlideChartSeries,
    SlideElementSpec,
};
use super::prompt::doc_has_unread_blocks;
use super::types::{
    is_valid_a1_coord, is_valid_hex_color, is_valid_range_ref, CopilotModule, CopilotRunRequest,
    MAX_BASE_FIELDS, MAX_BASE_RECORDS, MAX_BASE_TEXT_BYTES, MAX_BOARD_TEXT_BYTES,
    MAX_DOC_PARAGRAPHS, MAX_LLM_TOOL_CALLS, MAX_SLIDE_ELEMENTS,
};

const MAX_SLIDE_TABLE_ROWS: usize = 64;
const MAX_SLIDE_TABLE_COLS: usize = 32;
const MAX_SLIDE_CHART_ITEMS: usize = 32;
const MAX_SLIDE_TEXT_BYTES: usize = 16_384;
const MAX_SLIDE_IMAGE_DATA_BYTES: usize = 256 * 1024;
const MAX_GRAPH_NODES: usize = 128;
const MAX_GRAPH_EDGES: usize = 256;

pub(super) fn opt_usize(obj: &[(String, StrictJsonValue)], key: &str) -> Option<usize> {
    StrictJsonValue::get_field(obj, key)
        .and_then(StrictJsonValue::as_u64)
        .and_then(|v| usize::try_from(v).ok())
}

pub(super) fn opt_hex(obj: &[(String, StrictJsonValue)], key: &str) -> Option<String> {
    StrictJsonValue::get_str(obj, key)
        .map(str::trim)
        .filter(|s| is_valid_hex_color(s))
        .map(ToString::to_string)
}

fn opt_clean_str(
    obj: &[(String, StrictJsonValue)],
    key: &str,
    max_bytes: usize,
) -> Option<Option<String>> {
    match StrictJsonValue::get_field(obj, key) {
        None | Some(StrictJsonValue::Null) => Some(None),
        Some(StrictJsonValue::Str(s)) => {
            let t = s.trim();
            if t.len() > max_bytes || t.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
                None
            } else {
                Some(Some(t.to_string()))
            }
        }
        Some(_) => None,
    }
}

pub(super) fn parse_slide_layout(obj: &[(String, StrictJsonValue)]) -> Option<Option<String>> {
    match StrictJsonValue::get_field(obj, "layout") {
        None | Some(StrictJsonValue::Null) => Some(None),
        Some(StrictJsonValue::Str(s)) => {
            let t = s.trim();
            matches!(
                t,
                "titleSlide" | "titleAndContent" | "sectionHeader" | "twoColumn" | "blank"
            )
            .then(|| Some(t.to_string()))
        }
        Some(_) => None,
    }
}

pub(super) fn parse_slide_transition(obj: &[(String, StrictJsonValue)]) -> Option<Option<String>> {
    match StrictJsonValue::get_field(obj, "transition") {
        None | Some(StrictJsonValue::Null) => Some(None),
        Some(StrictJsonValue::Str(s)) => {
            let t = s.trim();
            matches!(t, "none" | "fade" | "push" | "wipe" | "split" | "zoom")
                .then(|| Some(t.to_string()))
        }
        Some(_) => None,
    }
}

fn json_f64(v: &StrictJsonValue) -> Option<f64> {
    match v {
        StrictJsonValue::Number(s) => s.parse::<f64>().ok().filter(|n| n.is_finite()),
        _ => None,
    }
}

fn opt_coord_i32(
    obj: &[(String, StrictJsonValue)],
    key: &str,
    min: f64,
    max: f64,
) -> Option<Option<i32>> {
    match StrictJsonValue::get_field(obj, key) {
        None | Some(StrictJsonValue::Null) => Some(None),
        Some(v) => json_f64(v)
            .filter(|n| (min..=max).contains(n))
            .map(|n| Some(n.round() as i32)),
    }
}

fn opt_dim_u32(
    obj: &[(String, StrictJsonValue)],
    key: &str,
    min: f64,
    max: f64,
) -> Option<Option<u32>> {
    match StrictJsonValue::get_field(obj, key) {
        None | Some(StrictJsonValue::Null) => Some(None),
        Some(v) => json_f64(v)
            .filter(|n| (min..=max).contains(n))
            .map(|n| Some(n.round() as u32)),
    }
}

pub(super) fn parse_slide_element(obj: &[(String, StrictJsonValue)]) -> Option<SlideElementSpec> {
    let kind = StrictJsonValue::get_str(obj, "kind")?.trim();
    if !matches!(kind, "textbox" | "shape" | "table" | "chart" | "image") {
        return None;
    }
    let id = opt_clean_str(obj, "id", 128)?.filter(|s| !s.is_empty());
    let x = opt_coord_i32(obj, "x", 0.0, 1_920.0)?;
    let y = opt_coord_i32(obj, "y", 0.0, 1_920.0)?;
    let w = opt_dim_u32(obj, "w", 12.0, 1_920.0)?;
    let h = opt_dim_u32(obj, "h", 12.0, 1_920.0)?;
    let text = opt_clean_str(obj, "text", MAX_SLIDE_TEXT_BYTES)?;
    let role = match opt_clean_str(obj, "role", 32)? {
        None => None,
        Some(r) if matches!(r.as_str(), "title" | "subtitle" | "body" | "none" | "") => {
            (!r.is_empty()).then_some(r)
        }
        Some(_) => return None,
    };
    let font_size_pt = opt_dim_u32(obj, "fontSizePt", 6.0, 200.0)?;
    let bold = match StrictJsonValue::get_field(obj, "bold") {
        None | Some(StrictJsonValue::Null) => None,
        Some(v) => Some(v.as_bool()?),
    };
    let italic = match StrictJsonValue::get_field(obj, "italic") {
        None | Some(StrictJsonValue::Null) => None,
        Some(v) => Some(v.as_bool()?),
    };
    let color = match StrictJsonValue::get_field(obj, "color") {
        None | Some(StrictJsonValue::Null) => None,
        Some(v) => v
            .as_str()
            .map(str::trim)
            .filter(|s| is_valid_hex_color(s))
            .map(ToString::to_string)
            .map(Some)?,
    };
    let fill = match StrictJsonValue::get_field(obj, "fill") {
        None | Some(StrictJsonValue::Null) => None,
        Some(v) => {
            let s = v.as_str()?.trim();
            if s.is_empty() || s == "none" || is_valid_hex_color(s) {
                Some(s.to_string())
            } else {
                return None;
            }
        }
    };
    let align = match opt_clean_str(obj, "align", 16)? {
        None => None,
        Some(a) if matches!(a.as_str(), "left" | "center" | "right" | "justify") => Some(a),
        Some(_) => return None,
    };
    let shape_type = opt_clean_str(obj, "shapeType", 32)?.filter(|s| !s.is_empty());
    let stroke = match StrictJsonValue::get_field(obj, "stroke") {
        None | Some(StrictJsonValue::Null) => None,
        Some(v) => {
            let s = v.as_str()?.trim();
            if s.is_empty() || s == "none" || is_valid_hex_color(s) {
                Some(s.to_string())
            } else {
                return None;
            }
        }
    };

    let rows = match StrictJsonValue::get_field(obj, "rows") {
        None | Some(StrictJsonValue::Null) => None,
        Some(v) => {
            let raw_rows = v.as_array()?;
            if raw_rows.is_empty() || raw_rows.len() > MAX_SLIDE_TABLE_ROWS {
                return None;
            }
            let mut parsed_rows = Vec::with_capacity(raw_rows.len());
            for r in raw_rows {
                let cells = r.as_array()?;
                if cells.is_empty() || cells.len() > MAX_SLIDE_TABLE_COLS {
                    return None;
                }
                let mut row_out = Vec::with_capacity(cells.len());
                for c in cells {
                    let s = c.to_scalar_string()?;
                    if s.len() > 2_048 {
                        return None;
                    }
                    row_out.push(s);
                }
                parsed_rows.push(row_out);
            }
            Some(parsed_rows)
        }
    };
    let header_row = match StrictJsonValue::get_field(obj, "headerRow") {
        None | Some(StrictJsonValue::Null) => None,
        Some(v) => Some(v.as_bool()?),
    };
    let chart_type = match opt_clean_str(obj, "chartType", 32)? {
        None => None,
        Some(ct) if matches!(ct.as_str(), "column" | "bar" | "line" | "pie" | "scatter") => {
            Some(ct)
        }
        Some(_) => return None,
    };
    let title = opt_clean_str(obj, "title", 256)?;
    let categories = match StrictJsonValue::get_field(obj, "categories") {
        None | Some(StrictJsonValue::Null) => None,
        Some(v) => {
            let arr = v.as_array()?;
            if arr.len() > MAX_SLIDE_CHART_ITEMS {
                return None;
            }
            let mut cats = Vec::with_capacity(arr.len());
            for item in arr {
                let s = item.to_scalar_string()?;
                if s.len() > 256 {
                    return None;
                }
                cats.push(s);
            }
            Some(cats)
        }
    };
    let series = match StrictJsonValue::get_field(obj, "series") {
        None | Some(StrictJsonValue::Null) => None,
        Some(v) => {
            let arr = v.as_array()?;
            if arr.len() > MAX_SLIDE_CHART_ITEMS {
                return None;
            }
            let mut list = Vec::with_capacity(arr.len());
            for item in arr {
                let s_obj = item.as_object()?;
                let name = opt_clean_str(s_obj, "name", 128)?.unwrap_or_default();
                let vals = StrictJsonValue::get_field(s_obj, "values")?.as_array()?;
                if vals.is_empty() || vals.len() > MAX_SLIDE_CHART_ITEMS {
                    return None;
                }
                let mut formatted = Vec::with_capacity(vals.len());
                for n in vals {
                    let num = json_f64(n)?;
                    formatted.push(num.to_string());
                }
                list.push(SlideChartSeries {
                    name,
                    values: formatted,
                    color: opt_hex(s_obj, "color"),
                });
            }
            Some(list)
        }
    };
    let mime = opt_clean_str(obj, "mime", 64)?;
    let data = opt_clean_str(obj, "data", MAX_SLIDE_IMAGE_DATA_BYTES)?;
    let alt = opt_clean_str(obj, "alt", 256)?;

    Some(SlideElementSpec {
        id,
        kind: kind.to_string(),
        x,
        y,
        w,
        h,
        text,
        role,
        font_size_pt,
        bold,
        italic,
        color,
        fill,
        align,
        shape_type,
        stroke,
        rows,
        header_row,
        chart_type,
        title,
        categories,
        series,
        mime,
        data,
        alt,
    })
}

pub(super) fn parse_slide_elements_opt(
    obj: &[(String, StrictJsonValue)],
) -> Option<Option<Vec<SlideElementSpec>>> {
    match StrictJsonValue::get_field(obj, "elements") {
        None | Some(StrictJsonValue::Null) => Some(None),
        Some(v) => {
            let arr = v.as_array()?;
            if arr.len() > MAX_SLIDE_ELEMENTS {
                return None;
            }
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                out.push(parse_slide_element(item.as_object()?)?);
            }
            Some(Some(out))
        }
    }
}

pub(super) fn validate_extended_sheet_op(
    kind: &str,
    obj: &[(String, StrictJsonValue)],
    default_sheet: &str,
) -> Option<CopilotOperation> {
    let sheet = StrictJsonValue::get_str(obj, "sheet")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(default_sheet)
        .to_string();
    match kind {
        "sheet.table" => {
            let action = StrictJsonValue::get_str(obj, "action")
                .unwrap_or("create")
                .trim();
            if !matches!(action, "create" | "style" | "total_row" | "to_range") {
                return None;
            }
            let range = match StrictJsonValue::get_str(obj, "range") {
                Some(r) => {
                    let t = r.trim();
                    is_valid_range_ref(t).then(|| t.to_string())?
                }
                None if action == "create" => return None,
                None => String::new(),
            };
            let table_name = opt_clean_str(obj, "tableName", 64)?
                .or(opt_clean_str(obj, "name", 64)?)
                .filter(|s| !s.is_empty());
            let style = opt_clean_str(obj, "style", 32)?.filter(|s| !s.is_empty());
            let total_row =
                StrictJsonValue::get_field(obj, "totalRow").and_then(StrictJsonValue::as_bool);
            Some(CopilotOperation::SheetTable {
                sheet,
                action: action.to_string(),
                range: (!range.is_empty()).then_some(range),
                table_name,
                style,
                total_row,
            })
        }
        "sheet.pivot" => {
            let source_range = StrictJsonValue::get_str(obj, "sourceRange")
                .or_else(|| StrictJsonValue::get_str(obj, "range"))?
                .trim();
            if !is_valid_range_ref(source_range) {
                return None;
            }
            let dest_cell = StrictJsonValue::get_str(obj, "destCell")
                .unwrap_or("F1")
                .trim();
            if !is_valid_a1_coord(dest_cell) {
                return None;
            }
            let row_field = opt_clean_str(obj, "rowField", 128)??;
            let value_field = opt_clean_str(obj, "valueField", 128)??;
            if row_field.is_empty() || value_field.is_empty() {
                return None;
            }
            let col_field = opt_clean_str(obj, "colField", 128)?.filter(|s| !s.is_empty());
            let agg = StrictJsonValue::get_str(obj, "agg")
                .unwrap_or("sum")
                .trim()
                .to_ascii_lowercase();
            if !matches!(agg.as_str(), "sum" | "count" | "average" | "min" | "max") {
                return None;
            }
            let dest_sheet = opt_clean_str(obj, "destSheet", 64)?.filter(|s| !s.is_empty());
            Some(CopilotOperation::SheetPivot {
                sheet,
                source_range: source_range.to_string(),
                dest_sheet,
                dest_cell: dest_cell.to_string(),
                row_field,
                col_field,
                value_field,
                agg,
            })
        }
        "sheet.cond_format" => {
            let range = StrictJsonValue::get_str(obj, "range")?.trim();
            if !is_valid_range_ref(range) {
                return None;
            }
            let preset = StrictJsonValue::get_str(obj, "preset")?.trim();
            if !matches!(
                preset,
                "greaterThan"
                    | "lessThan"
                    | "between"
                    | "equal"
                    | "textContains"
                    | "duplicateValues"
                    | "top10"
                    | "aboveAverage"
                    | "belowAverage"
                    | "dataBar"
                    | "colorScale"
                    | "iconSet"
            ) {
                return None;
            }
            let value1 = StrictJsonValue::get_field(obj, "value1")
                .and_then(StrictJsonValue::to_scalar_string)
                .filter(|s| s.len() <= 128);
            let value2 = StrictJsonValue::get_field(obj, "value2")
                .and_then(StrictJsonValue::to_scalar_string)
                .filter(|s| s.len() <= 128);
            Some(CopilotOperation::SheetCondFormat {
                sheet,
                range: range.to_string(),
                preset: preset.to_string(),
                value1,
                value2,
                fill: opt_hex(obj, "fill"),
                color: opt_hex(obj, "color"),
            })
        }
        "sheet.sort_filter" => {
            let action = StrictJsonValue::get_str(obj, "action")?.trim();
            if !matches!(action, "sort" | "filter" | "clear_filter") {
                return None;
            }
            let range = StrictJsonValue::get_str(obj, "range")?.trim();
            if !is_valid_range_ref(range) {
                return None;
            }
            let col_offset = opt_usize(obj, "colOffset").filter(|c| *c < 64);
            let ascending =
                StrictJsonValue::get_field(obj, "ascending").and_then(StrictJsonValue::as_bool);
            let query = opt_clean_str(obj, "query", 256)?;
            Some(CopilotOperation::SheetSortFilter {
                sheet,
                action: action.to_string(),
                range: range.to_string(),
                col_offset,
                ascending,
                query,
            })
        }
        "sheet.structure" => {
            let action = StrictJsonValue::get_str(obj, "action")?.trim();
            if !matches!(
                action,
                "insert_rows"
                    | "delete_rows"
                    | "insert_cols"
                    | "delete_cols"
                    | "freeze"
                    | "unfreeze"
            ) {
                return None;
            }
            let index = opt_usize(obj, "index")
                .or_else(|| opt_usize(obj, "at"))
                .or_else(|| opt_usize(obj, "freezeRows"))
                .unwrap_or(0);
            if index > 10_000 {
                return None;
            }
            let count = opt_usize(obj, "count").unwrap_or(1);
            if count == 0 || count > 64 {
                return None;
            }
            Some(CopilotOperation::SheetStructure {
                sheet,
                action: action.to_string(),
                index,
                count,
            })
        }
        "sheet.note" => {
            let coord = StrictJsonValue::get_str(obj, "coord")
                .or_else(|| StrictJsonValue::get_str(obj, "cell"))?
                .trim();
            if !is_valid_a1_coord(coord) {
                return None;
            }
            let text = opt_clean_str(obj, "text", 2_048)??;
            Some(CopilotOperation::SheetNote {
                sheet,
                coord: coord.to_string(),
                text,
            })
        }
        _ => None,
    }
}

pub(super) fn validate_base_or_board_op(
    kind: &str,
    obj: &[(String, StrictJsonValue)],
) -> Option<CopilotOperation> {
    match kind {
        "base.add_field" => {
            let name = opt_clean_str(obj, "name", MAX_BASE_TEXT_BYTES)??;
            if name.is_empty() {
                return None;
            }
            let field_type = StrictJsonValue::get_str(obj, "fieldType")
                .or_else(|| StrictJsonValue::get_str(obj, "type"))
                .unwrap_or("text")
                .trim();
            if !matches!(
                field_type,
                "text" | "number" | "checkbox" | "select" | "date"
            ) {
                return None;
            }
            let mut options = Vec::new();
            if let Some(arr) =
                StrictJsonValue::get_field(obj, "options").and_then(StrictJsonValue::as_array)
            {
                if arr.len() > 32 {
                    return None;
                }
                for item in arr {
                    let opt = item.as_str()?.trim();
                    if !opt.is_empty() && opt.len() <= MAX_BASE_TEXT_BYTES {
                        options.push(opt.to_string());
                    }
                }
            }
            Some(CopilotOperation::BaseAddField {
                name,
                field_type: field_type.to_string(),
                options,
            })
        }
        "base.add_records" => {
            let arr = StrictJsonValue::get_field(obj, "records")?.as_array()?;
            if arr.is_empty() || arr.len() > MAX_BASE_RECORDS {
                return None;
            }
            let mut records = Vec::with_capacity(arr.len());
            for item in arr {
                let row_obj = item.as_object()?;
                if row_obj.is_empty() || row_obj.len() > MAX_BASE_FIELDS {
                    return None;
                }
                let mut pairs = Vec::with_capacity(row_obj.len());
                for (k, v) in row_obj {
                    let key = k.trim();
                    if key.is_empty() || key.len() > MAX_BASE_TEXT_BYTES {
                        return None;
                    }
                    let val = v.to_scalar_string()?;
                    if val.len() > MAX_BASE_TEXT_BYTES {
                        return None;
                    }
                    pairs.push((key.to_string(), val));
                }
                records.push(pairs);
            }
            Some(CopilotOperation::BaseAddRecords { records })
        }
        "base.update_records" => {
            let arr = StrictJsonValue::get_field(obj, "updates")?.as_array()?;
            if arr.is_empty() || arr.len() > MAX_BASE_RECORDS {
                return None;
            }
            let mut updates = Vec::with_capacity(arr.len());
            for item in arr {
                let u_obj = item.as_object()?;
                let row_idx = opt_usize(u_obj, "rowIndex").or_else(|| opt_usize(u_obj, "index"))?;
                if row_idx >= 10_000 {
                    return None;
                }
                let values_obj = StrictJsonValue::get_field(u_obj, "values")?.as_object()?;
                if values_obj.is_empty() || values_obj.len() > MAX_BASE_FIELDS {
                    return None;
                }
                let mut pairs = Vec::with_capacity(values_obj.len());
                for (k, v) in values_obj {
                    let key = k.trim();
                    if key.is_empty() || key.len() > MAX_BASE_TEXT_BYTES {
                        return None;
                    }
                    let val = v.to_scalar_string()?;
                    if val.len() > MAX_BASE_TEXT_BYTES {
                        return None;
                    }
                    pairs.push((key.to_string(), val));
                }
                updates.push((row_idx, pairs));
            }
            Some(CopilotOperation::BaseUpdateRecords { updates })
        }
        "base.set_view" => {
            let view = match opt_clean_str(obj, "view", 16)? {
                None => None,
                Some(v) if matches!(v.as_str(), "grid" | "kanban" | "calendar") => Some(v),
                Some(_) => return None,
            };
            let filter_field = opt_clean_str(obj, "filterField", MAX_BASE_TEXT_BYTES)?;
            let filter_op = match opt_clean_str(obj, "filterOp", 32)? {
                None => None,
                Some(op) if op == "not_empty" => Some("notEmpty".to_string()),
                Some(op)
                    if matches!(
                        op.as_str(),
                        "contains"
                            | "eq"
                            | "neq"
                            | "gt"
                            | "gte"
                            | "lt"
                            | "lte"
                            | "empty"
                            | "notEmpty"
                    ) =>
                {
                    Some(op)
                }
                Some(_) => return None,
            };
            let filter_value = opt_clean_str(obj, "filterValue", MAX_BASE_TEXT_BYTES)?;
            let sort_field = opt_clean_str(obj, "sortField", MAX_BASE_TEXT_BYTES)?;
            let sort_dir = match opt_clean_str(obj, "sortDir", 16)? {
                None => None,
                Some(d) if matches!(d.as_str(), "asc" | "desc") => Some(d),
                Some(_) => return None,
            };
            let group_field = opt_clean_str(obj, "groupField", MAX_BASE_TEXT_BYTES)?
                .or(opt_clean_str(obj, "groupBy", MAX_BASE_TEXT_BYTES)?);
            Some(CopilotOperation::BaseSetView {
                view,
                filter_field,
                filter_op,
                filter_value,
                sort_field,
                sort_dir,
                group_field,
            })
        }
        "board.add_graph" => {
            let direction = StrictJsonValue::get_str(obj, "direction")
                .unwrap_or("TB")
                .trim();
            if !matches!(direction, "TB" | "LR") {
                return None;
            }
            let raw_nodes = StrictJsonValue::get_field(obj, "nodes")?.as_array()?;
            if raw_nodes.is_empty() || raw_nodes.len() > MAX_GRAPH_NODES {
                return None;
            }
            let mut nodes = Vec::with_capacity(raw_nodes.len());
            for item in raw_nodes {
                let n_obj = item.as_object()?;
                let id = opt_clean_str(n_obj, "id", 128)??;
                if id.is_empty() {
                    return None;
                }
                let n_kind = StrictJsonValue::get_str(n_obj, "kind")
                    .unwrap_or("shape")
                    .trim();
                if !matches!(n_kind, "shape" | "text") {
                    return None;
                }
                let shape = match opt_clean_str(n_obj, "shape", 32)? {
                    None => None,
                    Some(s)
                        if matches!(s.as_str(), "rect" | "roundRect" | "diamond" | "ellipse") =>
                    {
                        Some(s)
                    }
                    Some(_) => return None,
                };
                let text = opt_clean_str(n_obj, "text", MAX_BOARD_TEXT_BYTES)?
                    .or(opt_clean_str(n_obj, "label", MAX_BOARD_TEXT_BYTES)?)
                    .unwrap_or_default();
                nodes.push(BoardGraphNode {
                    id,
                    kind: n_kind.to_string(),
                    shape,
                    text,
                    fill: opt_hex(n_obj, "fill"),
                });
            }
            let mut edges = Vec::new();
            if let Some(raw_edges) =
                StrictJsonValue::get_field(obj, "edges").and_then(StrictJsonValue::as_array)
            {
                if raw_edges.len() > MAX_GRAPH_EDGES {
                    return None;
                }
                for item in raw_edges {
                    let e_obj = item.as_object()?;
                    let from_id = opt_clean_str(e_obj, "from", 128)??;
                    let to_id = opt_clean_str(e_obj, "to", 128)??;
                    if from_id.is_empty()
                        || to_id.is_empty()
                        || !nodes.iter().any(|n| n.id == from_id)
                        || !nodes.iter().any(|n| n.id == to_id)
                    {
                        return None;
                    }
                    let style = StrictJsonValue::get_str(e_obj, "style")
                        .unwrap_or("orthogonal")
                        .trim();
                    if !matches!(style, "orthogonal" | "curved") {
                        return None;
                    }
                    edges.push(BoardGraphEdge {
                        from_id,
                        to_id,
                        label: opt_clean_str(e_obj, "label", 256)?.filter(|s| !s.is_empty()),
                        style: style.to_string(),
                    });
                }
            }
            Some(CopilotOperation::BoardAddGraph {
                direction: direction.to_string(),
                nodes,
                edges,
            })
        }
        _ => None,
    }
}

pub fn validate_tool_calls(
    req: &CopilotRunRequest,
    items: &[StrictJsonValue],
) -> Vec<CopilotToolCall> {
    if req.command.is_some() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for item in items.iter().take(MAX_LLM_TOOL_CALLS) {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let Some(name) = StrictJsonValue::get_str(obj, "name")
            .or_else(|| StrictJsonValue::get_str(obj, "tool"))
            .map(str::trim)
        else {
            continue;
        };
        let allow_doc_read = doc_has_unread_blocks(req);
        match (name, req.module) {
            (
                "read_doc_blocks" | "read_hwp_paragraphs",
                CopilotModule::Doc | CopilotModule::Hwp,
            ) if allow_doc_read => {
                let start = opt_usize(obj, "startIndex").unwrap_or(0);
                let end = opt_usize(obj, "endIndex").unwrap_or(start.saturating_add(19));
                if start <= end && start < MAX_DOC_PARAGRAPHS && end.saturating_sub(start) < 200 {
                    out.push(CopilotToolCall::ReadDocBlocks {
                        start_index: start,
                        end_index: end.min(MAX_DOC_PARAGRAPHS - 1),
                    });
                }
            }
            ("read_sheet_range", CopilotModule::Sheet) => {
                if let Some(range) = StrictJsonValue::get_str(obj, "range")
                    .map(str::trim)
                    .filter(|r| is_valid_range_ref(r))
                {
                    let sheet = opt_clean_str(obj, "sheet", 64)
                        .flatten()
                        .filter(|s| !s.is_empty());
                    out.push(CopilotToolCall::ReadSheetRange {
                        sheet,
                        range: range.to_string(),
                    });
                }
            }
            ("find_sheet_cells", CopilotModule::Sheet) => {
                if let Some(Some(query)) = opt_clean_str(obj, "query", 256) {
                    if !query.is_empty() {
                        let sheet = opt_clean_str(obj, "sheet", 64)
                            .flatten()
                            .filter(|s| !s.is_empty());
                        out.push(CopilotToolCall::FindSheetCells { sheet, query });
                    }
                }
            }
            ("search_pdf_pages", CopilotModule::Pdf) => {
                if let Some(Some(query)) = opt_clean_str(obj, "query", 256) {
                    if !query.is_empty() {
                        out.push(CopilotToolCall::SearchPdfPages { query });
                    }
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_request_with_unread_blocks() -> CopilotRunRequest {
        let raw = r#"{"action":"copilot_run","mode":"edit","module":"DOC","prompt":"점검","selection":{"range":"","paragraphIndex":0},"provider":{"kind":"openai","baseUrl":"https://93.184.216.34/v1","model":"gpt-4o"},"context":{"doc":{"paragraphs":["첫 문단"],"omittedParagraphs":5}}}"#;
        let obj = StrictJsonValue::parse(raw).unwrap();
        super::super::types::parse_run_request(obj.as_object().unwrap()).unwrap()
    }

    fn read_calls(args: &str) -> Vec<CopilotToolCall> {
        let items =
            StrictJsonValue::parse(&format!(r#"[{{"name":"read_doc_blocks",{args}}}]"#)).unwrap();
        validate_tool_calls(&doc_request_with_unread_blocks(), items.as_array().unwrap())
    }

    #[test]
    fn a_read_without_an_end_index_covers_twenty_paragraphs() {
        assert_eq!(
            read_calls(r#""startIndex":10"#),
            vec![CopilotToolCall::ReadDocBlocks {
                start_index: 10,
                end_index: 29
            }]
        );
    }

    #[test]
    fn a_start_index_at_the_integer_limit_is_refused_instead_of_overflowing() {
        assert!(read_calls(&format!(r#""startIndex":{}"#, u64::MAX)).is_empty());
        assert!(read_calls(&format!(
            r#""startIndex":{},"endIndex":{}"#,
            u64::MAX,
            u64::MAX
        ))
        .is_empty());
    }
}

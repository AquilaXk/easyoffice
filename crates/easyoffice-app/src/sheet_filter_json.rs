//! JSON form of a sheet's auto filter (PD-198), shared by the open answer, the save request, the
//! filter requests and the row/column edit of a sheet session.
//!
//! `{"range":"A1:D20","columns":[...],"hidden":[[first,last],...]}`: `range` includes the header
//! row, `hidden` lists the rows (1-based, inclusive) the filter hides. A column is
//! `{"col":2,"values":["a","b"],"blank":true}` or `{"col":3,"and":true,"conditions":[{"op":
//! "greaterThan","text":"5"},{"text":"a*"}]}` (`col` is 1-based; no `op` is the text match). A list
//! may also hold date groups, `"dates":[{"unit":"month","year":2024,"month":3}]` (the fields the
//! unit names), and a column may instead carry `"top":{"bottom":false,"percent":false,"count":10}`,
//! `"average":{"above":true}` or `"color":{"font":false,"rgb":"ffc7ce"}` (no `rgb`: no colour of its
//! own) (PD-226). Input over a bound is refused, not trimmed.

use crate::sheet_layout_json::{parse_runs_limited, runs_json};
use crate::EasyOfficeIpcRouter;
use easyoffice_formats::{
    CellBox, CustomCondition, CustomOp, DateGroup, DateUnit, EasyOfficeError, FilterColumn,
    FilterCriteria, SheetFilter, StrictJsonValue, MAX_FILTER_COLUMNS, MAX_FILTER_DATE_GROUPS,
    MAX_FILTER_ROWS, MAX_FILTER_TEXT_CHARS, MAX_FILTER_VALUES, MAX_TOP_ITEMS, MAX_TOP_PERCENT,
};
use std::fmt::Write;

const LAST_ROW: u32 = 1_048_575;

fn bad(what: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(format!("필터 JSON이 올바르지 않습니다: {what}"))
}

fn over(what: &str) -> EasyOfficeError {
    EasyOfficeError::ResourceBudgetExceeded(format!("필터 {what}이(가) 한도를 넘었습니다."))
}

fn text_of(v: &StrictJsonValue, what: &str) -> Result<String, EasyOfficeError> {
    let text = v.as_str().ok_or_else(|| bad(what))?;
    if text.chars().count() > MAX_FILTER_TEXT_CHARS {
        return Err(over("문자열"));
    }
    Ok(text.to_string())
}

fn parse_condition(v: &StrictJsonValue) -> Result<CustomCondition, EasyOfficeError> {
    let obj = v.as_object().ok_or_else(|| bad("conditions"))?;
    let op = match StrictJsonValue::get_field(obj, "op") {
        None | Some(StrictJsonValue::Null) => None,
        Some(name) => Some(
            name.as_str()
                .and_then(CustomOp::from_name)
                .ok_or_else(|| bad("op"))?,
        ),
    };
    let text = StrictJsonValue::get_field(obj, "text").ok_or_else(|| bad("text"))?;
    Ok(CustomCondition {
        op,
        text: text_of(text, "text")?,
    })
}

fn parse_date_group(v: &StrictJsonValue) -> Result<DateGroup, EasyOfficeError> {
    let obj = v.as_object().ok_or_else(|| bad("dates"))?;
    let unit = StrictJsonValue::get_field(obj, "unit")
        .and_then(StrictJsonValue::as_str)
        .and_then(DateUnit::from_name)
        .ok_or_else(|| bad("unit"))?;
    let field = |name: &str, needed: bool| -> Result<u16, EasyOfficeError> {
        match StrictJsonValue::get_field(obj, name) {
            Some(v) if needed => v
                .as_u64()
                .and_then(|n| u16::try_from(n).ok())
                .ok_or_else(|| bad(name)),
            None if !needed => Ok(0),
            _ => Err(bad(name)),
        }
    };
    let small = |name: &str, needed: bool| {
        field(name, needed).and_then(|n| u8::try_from(n).map_err(|_| bad(name)))
    };
    let group = DateGroup {
        unit,
        year: field("year", true)?,
        month: small("month", unit >= DateUnit::Month)?,
        day: small("day", unit >= DateUnit::Day)?,
        hour: small("hour", unit >= DateUnit::Hour)?,
        minute: small("minute", unit >= DateUnit::Minute)?,
        second: small("second", unit >= DateUnit::Second)?,
    };
    if group.is_valid() {
        Ok(group)
    } else {
        Err(bad("dates"))
    }
}

fn parse_top(v: &StrictJsonValue) -> Result<FilterCriteria, EasyOfficeError> {
    let obj = v.as_object().ok_or_else(|| bad("top"))?;
    let flag = |key: &str| {
        matches!(
            StrictJsonValue::get_field(obj, key),
            Some(StrictJsonValue::Bool(true))
        )
    };
    let percent = flag("percent");
    let most = if percent {
        MAX_TOP_PERCENT
    } else {
        MAX_TOP_ITEMS
    };
    let count = StrictJsonValue::get_field(obj, "count")
        .and_then(StrictJsonValue::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| (1..=most).contains(n))
        .ok_or_else(|| bad("count"))?;
    Ok(FilterCriteria::Top {
        bottom: flag("bottom"),
        percent,
        count,
    })
}

fn parse_color(v: &StrictJsonValue) -> Result<FilterCriteria, EasyOfficeError> {
    let obj = v.as_object().ok_or_else(|| bad("color"))?;
    let font = matches!(
        StrictJsonValue::get_field(obj, "font"),
        Some(StrictJsonValue::Bool(true))
    );
    let color = match StrictJsonValue::get_field(obj, "rgb") {
        None | Some(StrictJsonValue::Null) => None,
        Some(v) => Some(
            v.as_str()
                .filter(|t| t.len() == 6)
                .and_then(|t| u32::from_str_radix(t, 16).ok())
                .ok_or_else(|| bad("rgb"))?,
        ),
    };
    Ok(FilterCriteria::Color { font, color })
}

fn parse_column(v: &StrictJsonValue, range: &CellBox) -> Result<FilterColumn, EasyOfficeError> {
    let obj = v.as_object().ok_or_else(|| bad("columns"))?;
    let col = StrictJsonValue::get_field(obj, "col")
        .and_then(StrictJsonValue::as_u64)
        .filter(|c| (1..=16_384).contains(c))
        .map(|c| c as u32 - 1)
        .filter(|c| (range.col0..=range.col1).contains(c))
        .ok_or_else(|| bad("col"))?;
    let flag = |key: &str| {
        matches!(
            StrictJsonValue::get_field(obj, key),
            Some(StrictJsonValue::Bool(true))
        )
    };
    let list_of = |key: &str, most: usize| -> Result<&[StrictJsonValue], EasyOfficeError> {
        match StrictJsonValue::get_field(obj, key) {
            None => Ok(&[]),
            Some(list) => {
                let list = list.as_array().ok_or_else(|| bad(key))?;
                if list.len() > most {
                    return Err(over("값 수"));
                }
                Ok(list)
            }
        }
    };
    let criteria = if let Some(top) = StrictJsonValue::get_field(obj, "top") {
        parse_top(top)?
    } else if let Some(average) = StrictJsonValue::get_field(obj, "average") {
        let above = average
            .as_object()
            .and_then(|o| StrictJsonValue::get_field(o, "above"))
            .and_then(|v| match v {
                StrictJsonValue::Bool(b) => Some(*b),
                _ => None,
            })
            .ok_or_else(|| bad("average"))?;
        FilterCriteria::Average { above }
    } else if let Some(color) = StrictJsonValue::get_field(obj, "color") {
        parse_color(color)?
    } else if obj.iter().any(|(k, _)| k == "values" || k == "dates") {
        FilterCriteria::Values {
            blank: flag("blank"),
            values: list_of("values", MAX_FILTER_VALUES)?
                .iter()
                .map(|item| text_of(item, "values"))
                .collect::<Result<_, _>>()?,
            dates: list_of("dates", MAX_FILTER_DATE_GROUPS)?
                .iter()
                .map(parse_date_group)
                .collect::<Result<_, _>>()?,
        }
    } else {
        let list = StrictJsonValue::get_field(obj, "conditions")
            .and_then(StrictJsonValue::as_array)
            .ok_or_else(|| bad("conditions"))?;
        let mut it = list.iter();
        match (it.next(), it.next(), it.next()) {
            (Some(first), second, None) => FilterCriteria::Custom {
                and: flag("and"),
                first: parse_condition(first)?,
                second: second.map(parse_condition).transpose()?,
            },
            _ => return Err(bad("conditions")),
        }
    };
    Ok(FilterColumn { col, criteria })
}

/// The conditions of a filter's columns from `[{"col":..}, ...]`, sorted by column and inside `range`.
pub(crate) fn parse_columns(
    v: &StrictJsonValue,
    range: &CellBox,
) -> Result<Vec<FilterColumn>, EasyOfficeError> {
    let items = v.as_array().ok_or_else(|| bad("columns"))?;
    if items.len() > MAX_FILTER_COLUMNS {
        return Err(over("조건 열 수"));
    }
    let mut columns: Vec<FilterColumn> = Vec::with_capacity(items.len());
    for item in items {
        let column = parse_column(item, range)?;
        if columns.last().is_some_and(|last| last.col >= column.col) {
            return Err(bad("columns"));
        }
        columns.push(column);
    }
    Ok(columns)
}

/// A filter from its JSON object.
pub(crate) fn parse_filter(v: &StrictJsonValue) -> Result<SheetFilter, EasyOfficeError> {
    let obj = v.as_object().ok_or_else(|| bad("filter"))?;
    let range = StrictJsonValue::get_field(obj, "range")
        .and_then(StrictJsonValue::as_str)
        .filter(|t| t.contains(':') && t.bytes().any(|b| b.is_ascii_digit()))
        .and_then(CellBox::parse)
        .filter(|r| r.row1 > r.row0 && r.row1 <= LAST_ROW)
        .ok_or_else(|| bad("range"))?;
    if range.row1 - range.row0 >= MAX_FILTER_ROWS {
        return Err(over("범위 행 수"));
    }
    let mut filter = SheetFilter::new(range);
    if let Some(columns) = StrictJsonValue::get_field(obj, "columns") {
        filter.columns = parse_columns(columns, &range)?;
    }
    if let Some(hidden) = StrictJsonValue::get_field(obj, "hidden") {
        filter.hidden = parse_runs_limited(
            hidden,
            LAST_ROW,
            "filter.hidden",
            u64::from(MAX_FILTER_ROWS),
        )?;
        if filter
            .hidden
            .iter()
            .any(|&(a, b)| a <= range.row0 || b > range.row1)
        {
            return Err(bad("hidden"));
        }
    }
    Ok(filter)
}

fn push_text(out: &mut String, text: &str) {
    out.push('"');
    EasyOfficeIpcRouter::escape_json_into(out, text);
    out.push('"');
}

fn condition_json(out: &mut String, c: &CustomCondition) {
    out.push('{');
    if let Some(op) = c.op {
        let _ = write!(out, "\"op\":\"{}\",", op.name());
    }
    out.push_str("\"text\":");
    push_text(out, &c.text);
    out.push('}');
}

fn date_group_json(out: &mut String, g: &DateGroup) {
    let _ = write!(out, "{{\"unit\":\"{}\",\"year\":{}", g.unit.name(), g.year);
    for (unit, name, value) in [
        (DateUnit::Month, "month", g.month),
        (DateUnit::Day, "day", g.day),
        (DateUnit::Hour, "hour", g.hour),
        (DateUnit::Minute, "minute", g.minute),
        (DateUnit::Second, "second", g.second),
    ] {
        if g.unit >= unit {
            let _ = write!(out, ",\"{name}\":{value}");
        }
    }
    out.push('}');
}

/// `[{"col":..}, ...]` for the conditions of a filter's columns.
pub(crate) fn write_columns(out: &mut String, columns: &[FilterColumn]) {
    out.push('[');
    for (i, column) in columns.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{{\"col\":{}", column.col + 1);
        match &column.criteria {
            FilterCriteria::Values {
                blank,
                values,
                dates,
            } => {
                if *blank {
                    out.push_str(",\"blank\":true");
                }
                out.push_str(",\"values\":[");
                for (j, value) in values.iter().enumerate() {
                    if j > 0 {
                        out.push(',');
                    }
                    push_text(out, value);
                }
                out.push(']');
                if !dates.is_empty() {
                    out.push_str(",\"dates\":[");
                    for (j, group) in dates.iter().enumerate() {
                        if j > 0 {
                            out.push(',');
                        }
                        date_group_json(out, group);
                    }
                    out.push(']');
                }
            }
            FilterCriteria::Top {
                bottom,
                percent,
                count,
            } => {
                out.push_str(",\"top\":{");
                if *bottom {
                    out.push_str("\"bottom\":true,");
                }
                if *percent {
                    out.push_str("\"percent\":true,");
                }
                let _ = write!(out, "\"count\":{count}}}");
            }
            FilterCriteria::Average { above } => {
                let _ = write!(out, ",\"average\":{{\"above\":{above}}}");
            }
            FilterCriteria::Color { font, color } => {
                out.push_str(",\"color\":{");
                if *font {
                    out.push_str("\"font\":true,");
                }
                if let Some(rgb) = color {
                    let _ = write!(out, "\"rgb\":\"{rgb:06x}\",");
                }
                if out.ends_with(',') {
                    out.pop();
                }
                out.push('}');
            }
            FilterCriteria::Custom { and, first, second } => {
                if *and {
                    out.push_str(",\"and\":true");
                }
                out.push_str(",\"conditions\":[");
                condition_json(out, first);
                if let Some(second) = second {
                    out.push(',');
                    condition_json(out, second);
                }
                out.push(']');
            }
        }
        out.push('}');
    }
    out.push(']');
}

/// The JSON object of `filter`.
pub(crate) fn filter_json(filter: &SheetFilter) -> String {
    let mut out = String::with_capacity(128);
    let _ = write!(out, "{{\"range\":\"{}\",\"columns\":", filter.range.to_a1());
    write_columns(&mut out, &filter.columns);
    if !filter.hidden.is_empty() {
        let _ = write!(out, ",\"hidden\":{}", runs_json(&filter.hidden));
    }
    out.push('}');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<SheetFilter, EasyOfficeError> {
        parse_filter(&StrictJsonValue::parse(text).unwrap())
    }

    #[test]
    fn a_filter_round_trips_through_json() {
        let text = concat!(
            r#"{"range":"A1:D20","columns":["#,
            r#"{"col":1,"blank":true,"values":["영업","a\"b"]},"#,
            r#"{"col":3,"and":true,"conditions":[{"op":"greaterThan","text":"5"},{"text":"a*"}]}],"#,
            r#""hidden":[[3,5],[9,9]]}"#
        );
        let filter = parse(text).unwrap();
        assert_eq!(filter.range.to_a1(), "A1:D20");
        assert_eq!(filter.columns.len(), 2);
        assert_eq!(filter.hidden, vec![(2, 4), (8, 8)]);
        assert_eq!(filter_json(&filter), text);
        let plain = parse(r#"{"range":"B2:C9"}"#).unwrap();
        assert_eq!(filter_json(&plain), r#"{"range":"B2:C9","columns":[]}"#);
    }

    #[test]
    fn date_groups_top_average_and_colour_round_trip_through_json() {
        let text = concat!(
            r#"{"range":"A1:E20","columns":["#,
            r#"{"col":1,"blank":true,"values":["n/a"],"dates":[{"unit":"year","year":2023},"#,
            r#"{"unit":"day","year":2024,"month":2,"day":29},"#,
            r#"{"unit":"second","year":2024,"month":1,"day":2,"hour":3,"minute":4,"second":5}]},"#,
            r#"{"col":2,"dates":[{"unit":"month","year":2024,"month":12}]},"#,
            r#"{"col":3,"top":{"bottom":true,"percent":true,"count":25}},"#,
            r#"{"col":4,"average":{"above":false}},"#,
            r#"{"col":5,"color":{"font":true,"rgb":"9c0006"}}]}"#
        );
        let filter = parse(text).unwrap();
        // A list of dates alone has its `values` key written (empty).
        let again = filter_json(&filter);
        assert_eq!(parse(&again).unwrap(), filter);
        assert!(again.contains(
            r#"{"col":2,"values":[],"dates":[{"unit":"month","year":2024,"month":12}]}"#
        ));
        assert!(again.contains(r#"{"col":3,"top":{"bottom":true,"percent":true,"count":25}}"#));
        assert!(again.contains(r#"{"col":4,"average":{"above":false}}"#));
        assert!(again.contains(r#"{"col":5,"color":{"font":true,"rgb":"9c0006"}}"#));
        let plain = parse(r#"{"range":"A1:B9","columns":[{"col":1,"color":{}}]}"#).unwrap();
        assert!(filter_json(&plain).contains(r#"{"col":1,"color":{}}"#));
    }

    #[test]
    fn hostile_filters_are_refused() {
        let long = "x".repeat(MAX_FILTER_TEXT_CHARS + 1);
        let many = (0..=MAX_FILTER_VALUES)
            .map(|i| format!("\"{i}\""))
            .collect::<Vec<_>>()
            .join(",");
        for bad_text in [
            r#"{"range":"A1"}"#.to_string(),
            r#"{"range":"A1:C1"}"#.to_string(),
            r#"{"range":"A1:C300000"}"#.to_string(),
            r#"{"range":"A1:C9","columns":[{"col":4,"values":[]}]}"#.to_string(),
            r#"{"range":"A1:C9","columns":[{"col":2,"values":[1]}]}"#.to_string(),
            r#"{"range":"A1:C9","columns":[{"col":2},{"col":1}]}"#.to_string(),
            r#"{"range":"A1:C9","columns":[{"col":2,"conditions":[]}]}"#.to_string(),
            r#"{"range":"A1:C9","columns":[{"col":2,"conditions":[{"text":"a"},{"text":"b"},{"text":"c"}]}]}"#.to_string(),
            r#"{"range":"A1:C9","columns":[{"col":2,"conditions":[{"op":"near","text":"a"}]}]}"#.to_string(),
            format!(r#"{{"range":"A1:C9","columns":[{{"col":2,"conditions":[{{"text":"{long}"}}]}}]}}"#),
            format!(r#"{{"range":"A1:C9","columns":[{{"col":2,"values":[{many}]}}]}}"#),
            r#"{"range":"A5:C9","hidden":[[3,4]]}"#.to_string(),
            r#"{"range":"A1:C9","hidden":[[5,12]]}"#.to_string(),
        ] {
            assert!(parse(&bad_text).is_err(), "{}", &bad_text[..bad_text.len().min(120)]);
        }
    }
}

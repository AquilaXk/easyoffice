//! JSON form of a sheet's auto filter (PD-198), shared by the open answer, the save request, the
//! filter requests and the row/column edit of a sheet session.
//!
//! `{"range":"A1:D20","columns":[...],"hidden":[[first,last],...]}`: `range` includes the header
//! row, `hidden` lists the rows (1-based, inclusive) the filter hides. A column is
//! `{"col":2,"values":["a","b"],"blank":true}` or `{"col":3,"and":true,"conditions":[{"op":
//! "greaterThan","text":"5"},{"text":"a*"}]}` (`col` is 1-based; no `op` is the text match).
//! Input over a bound is refused, not trimmed.

use crate::sheet_layout_json::{parse_runs_limited, runs_json};
use crate::EasyOfficeIpcRouter;
use easyoffice_formats::{
    CellBox, CustomCondition, CustomOp, EasyOfficeError, FilterColumn, FilterCriteria, SheetFilter,
    StrictJsonValue, MAX_FILTER_COLUMNS, MAX_FILTER_ROWS, MAX_FILTER_TEXT_CHARS, MAX_FILTER_VALUES,
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
    let criteria = if let Some(list) = StrictJsonValue::get_field(obj, "values") {
        let list = list.as_array().ok_or_else(|| bad("values"))?;
        if list.len() > MAX_FILTER_VALUES {
            return Err(over("값 수"));
        }
        FilterCriteria::Values {
            blank: flag("blank"),
            values: list
                .iter()
                .map(|item| text_of(item, "values"))
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
        let columns = columns.as_array().ok_or_else(|| bad("columns"))?;
        if columns.len() > MAX_FILTER_COLUMNS {
            return Err(over("조건 열 수"));
        }
        for item in columns {
            let column = parse_column(item, &range)?;
            if filter
                .columns
                .last()
                .is_some_and(|last| last.col >= column.col)
            {
                return Err(bad("columns"));
            }
            filter.columns.push(column);
        }
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

/// The JSON object of `filter`.
pub(crate) fn filter_json(filter: &SheetFilter) -> String {
    let mut out = String::with_capacity(128);
    let _ = write!(
        out,
        "{{\"range\":\"{}\",\"columns\":[",
        filter.range.to_a1()
    );
    for (i, column) in filter.columns.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{{\"col\":{}", column.col + 1);
        match &column.criteria {
            FilterCriteria::Values { blank, values } => {
                if *blank {
                    out.push_str(",\"blank\":true");
                }
                out.push_str(",\"values\":[");
                for (j, value) in values.iter().enumerate() {
                    if j > 0 {
                        out.push(',');
                    }
                    push_text(&mut out, value);
                }
                out.push(']');
            }
            FilterCriteria::Custom { and, first, second } => {
                if *and {
                    out.push_str(",\"and\":true");
                }
                out.push_str(",\"conditions\":[");
                condition_json(&mut out, first);
                if let Some(second) = second {
                    out.push(',');
                    condition_json(&mut out, second);
                }
                out.push(']');
            }
        }
        out.push('}');
    }
    out.push(']');
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

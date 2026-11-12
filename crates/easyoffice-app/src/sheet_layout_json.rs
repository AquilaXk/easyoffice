//! JSON form of a sheet's merged ranges, hidden rows and hidden columns (and, from `sheet_view_json`, its view), shared by the open
//! answer, the save request and the row/column edit of a sheet session.
//!
//! `merges`: `["A1:B2", ...]`; `hiddenRows`, `hiddenCols`: `[[first, last], ...]`, 1-based and
//! inclusive. Input over a limit is refused, not trimmed.

use easyoffice_formats::{
    normalize_runs, run_total, CellBox, EasyOfficeError, SheetFilter, SheetStyling,
    StrictJsonValue, MAX_HIDDEN_ROWS, MAX_MERGES,
};
use std::fmt::Write;

const LAST_ROW: u32 = 1_048_575;
const LAST_COL: u32 = 16_383;

fn bad(what: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(format!("레이아웃 JSON이 올바르지 않습니다: {what}"))
}

pub(crate) fn parse_merges(v: &StrictJsonValue) -> Result<Vec<CellBox>, EasyOfficeError> {
    let items = v.as_array().ok_or_else(|| bad("merges"))?;
    if items.len() > MAX_MERGES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "병합 범위 수가 한도({MAX_MERGES})를 초과했습니다."
        )));
    }
    items
        .iter()
        .map(|item| {
            item.as_str()
                .filter(|t| t.bytes().any(|b| b.is_ascii_digit()) && t.contains(':'))
                .and_then(CellBox::parse)
                .filter(|b| b.area() > 1 && b.row1 <= LAST_ROW && b.col1 <= LAST_COL)
                .ok_or_else(|| bad("merges"))
        })
        .collect()
}

/// Runs from `[[first, last], ...]`, 1-based; `last_index` is the last valid zero-based index.
pub(crate) fn parse_runs(
    v: &StrictJsonValue,
    last_index: u32,
    what: &str,
) -> Result<Vec<(u32, u32)>, EasyOfficeError> {
    parse_runs_limited(
        v,
        last_index,
        what,
        MAX_HIDDEN_ROWS.max(u64::from(LAST_COL) + 1),
    )
}

/// [`parse_runs`] with at most `max_total` rows or columns over all runs.
pub(crate) fn parse_runs_limited(
    v: &StrictJsonValue,
    last_index: u32,
    what: &str,
    max_total: u64,
) -> Result<Vec<(u32, u32)>, EasyOfficeError> {
    let items = v.as_array().ok_or_else(|| bad(what))?;
    if items.len() > max_total as usize {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "숨김 구간 수가 한도({MAX_HIDDEN_ROWS})를 초과했습니다."
        )));
    }
    let mut runs = Vec::with_capacity(items.len());
    for item in items {
        let pair = item
            .as_array()
            .filter(|p| p.len() == 2)
            .ok_or_else(|| bad(what))?;
        let end = |i: usize| {
            pair[i]
                .as_u64()
                .filter(|n| (1..=u64::from(last_index) + 1).contains(n))
                .map(|n| n as u32 - 1)
        };
        match (end(0), end(1)) {
            (Some(a), Some(b)) if a <= b => runs.push((a, b)),
            _ => return Err(bad(what)),
        }
    }
    normalize_runs(&mut runs);
    if run_total(&runs) > max_total {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "숨김 행·열 수가 한도({max_total})를 초과했습니다."
        )));
    }
    Ok(runs)
}

pub(crate) fn parse_hidden_rows(v: &StrictJsonValue) -> Result<Vec<(u32, u32)>, EasyOfficeError> {
    let runs = parse_runs(v, LAST_ROW, "hiddenRows")?;
    if run_total(&runs) > MAX_HIDDEN_ROWS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "숨김 행 수가 한도({MAX_HIDDEN_ROWS})를 초과했습니다."
        )));
    }
    Ok(runs)
}

pub(crate) fn parse_hidden_cols(v: &StrictJsonValue) -> Result<Vec<(u32, u32)>, EasyOfficeError> {
    parse_runs(v, LAST_COL, "hiddenCols")
}

/// Reads the layout fields of a sheet object into `styling`.
pub(crate) fn parse_layout(
    obj: &[(String, StrictJsonValue)],
    styling: &mut SheetStyling,
) -> Result<(), EasyOfficeError> {
    if let Some(v) = StrictJsonValue::get_field(obj, "merges") {
        styling.merges = parse_merges(v)?;
    }
    if let Some(v) = StrictJsonValue::get_field(obj, "hiddenRows") {
        styling.hidden_rows = parse_hidden_rows(v)?;
    }
    if let Some(v) = StrictJsonValue::get_field(obj, "hiddenCols") {
        styling.hidden_cols = parse_hidden_cols(v)?;
    }
    if let Some(v) = StrictJsonValue::get_field(obj, "filter") {
        styling.filter = Some(crate::sheet_filter_json::parse_filter(v)?);
    }
    crate::sheet_view_json::parse_view(obj, styling)?;
    Ok(())
}

pub(crate) fn merges_json(merges: &[CellBox]) -> String {
    let items: Vec<String> = merges
        .iter()
        .map(|m| format!("\"{}\"", m.to_a1()))
        .collect();
    format!("[{}]", items.join(","))
}

pub(crate) fn runs_json(runs: &[(u32, u32)]) -> String {
    let mut out = String::from("[");
    for (i, (a, b)) in runs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "[{},{}]", a + 1, b + 1);
    }
    out.push(']');
    out
}

/// `,"merges":..,"hiddenRows":..,"hiddenCols":..,"filter":..` for the parts that are not empty.
pub(crate) fn write_layout(out: &mut String, styling: &SheetStyling) {
    write_layout_split(out, styling, &styling.hidden_rows, styling.filter.as_ref());
}

/// [`write_layout`] with the hidden rows and the filter a caller has already told apart.
pub(crate) fn write_layout_split(
    out: &mut String,
    styling: &SheetStyling,
    hidden_rows: &[(u32, u32)],
    filter: Option<&SheetFilter>,
) {
    if !styling.merges.is_empty() {
        let _ = write!(out, ",\"merges\":{}", merges_json(&styling.merges));
    }
    if !hidden_rows.is_empty() {
        let _ = write!(out, ",\"hiddenRows\":{}", runs_json(hidden_rows));
    }
    if !styling.hidden_cols.is_empty() {
        let _ = write!(out, ",\"hiddenCols\":{}", runs_json(&styling.hidden_cols));
    }
    if let Some(filter) = filter {
        let _ = write!(
            out,
            ",\"filter\":{}",
            crate::sheet_filter_json::filter_json(filter)
        );
    }
    crate::sheet_view_json::write_view(out, styling);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(text: &str) -> StrictJsonValue {
        StrictJsonValue::parse(text).unwrap()
    }

    #[test]
    fn layout_round_trips_through_json() {
        let mut styling = SheetStyling::default();
        let sheet = json(
            r#"{"merges":["A1:B2","D4:D6"],"hiddenRows":[[3,4],[5,5],[9,9]],"hiddenCols":[[2,2]]}"#,
        );
        parse_layout(sheet.as_object().unwrap(), &mut styling).unwrap();
        assert_eq!(styling.merges.len(), 2);
        assert_eq!(styling.hidden_rows, vec![(2, 4), (8, 8)]);
        let mut out = String::new();
        write_layout(&mut out, &styling);
        assert_eq!(
            out,
            r#","merges":["A1:B2","D4:D6"],"hiddenRows":[[3,5],[9,9]],"hiddenCols":[[2,2]]"#
        );
    }

    #[test]
    fn hostile_layout_is_refused() {
        for bad_text in [
            r#"{"merges":["A1"]}"#,
            r#"{"merges":["A:C"]}"#,
            r#"{"merges":[1]}"#,
            r#"{"hiddenRows":[[0,3]]}"#,
            r#"{"hiddenRows":[[5,3]]}"#,
            r#"{"hiddenRows":[[1,1048577]]}"#,
            r#"{"hiddenRows":[[1,200000]]}"#,
            r#"{"hiddenCols":[[1,16385]]}"#,
            r#"{"hiddenRows":[1,2]}"#,
        ] {
            let v = json(bad_text);
            assert!(
                parse_layout(v.as_object().unwrap(), &mut SheetStyling::default()).is_err(),
                "{bad_text}"
            );
        }
    }
}

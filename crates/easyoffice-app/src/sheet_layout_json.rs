//! JSON form of a sheet's merged ranges, hidden rows and hidden columns (and, from `sheet_view_json`, its view), shared by the open
//! answer, the save request and the row/column edit of a sheet session.
//!
//! `merges`: `["A1:B2", ...]`; `hiddenRows`, `hiddenCols`: `[[first, last], ...]`, 1-based and
//! inclusive. Input over a limit is refused, not trimmed.

use easyoffice_formats::{
    normalize_runs, normalize_spills, run_total, CellBox, EasyOfficeError, SheetFilter,
    SheetStyling, SheetTable, SpillRef, StrictJsonValue, MAX_HIDDEN_ROWS, MAX_MERGES,
    MAX_SPILL_REFS,
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

/// The `merges` field of a request without a session: `{"Sheet1": ["A1:B2", ...], ...}` (PD-222); none when absent.
pub(crate) fn merges_by_sheet(
    entries: &[(String, StrictJsonValue)],
) -> Result<Vec<(String, Vec<CellBox>)>, EasyOfficeError> {
    let Some(field) = StrictJsonValue::get_field(entries, "merges") else {
        return Ok(Vec::new());
    };
    let sheets = field.as_object().ok_or_else(|| bad("merges"))?;
    sheets
        .iter()
        .map(|(name, list)| Ok((name.clone(), parse_merges(list)?)))
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
    if let Some(v) = StrictJsonValue::get_field(obj, "tables") {
        styling.tables = crate::sheet_table_json::parse_tables(v)?;
    }
    if let Some(v) = StrictJsonValue::get_field(obj, "spills") {
        styling.spills = parse_spills(v)?;
    }
    crate::sheet_view_json::parse_view(obj, styling)?;
    crate::sheet_page_json::parse_page(obj, styling)?;
    crate::sheet_protect_json::parse_sheet_protect(obj, styling)?;
    Ok(())
}

/// `spills`: `["A1:A3", ...]`, the area of each array that spills, its first cell the formula's (PD-217).
pub(crate) fn parse_spills(v: &StrictJsonValue) -> Result<Vec<SpillRef>, EasyOfficeError> {
    let items = v.as_array().ok_or_else(|| bad("spills"))?;
    if items.len() > MAX_SPILL_REFS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "스필 영역 수가 한도({MAX_SPILL_REFS})를 초과했습니다."
        )));
    }
    let mut refs = Vec::with_capacity(items.len());
    for item in items {
        let area = item
            .as_str()
            .filter(|t| t.bytes().any(|b| b.is_ascii_digit()) && t.contains(':'))
            .and_then(CellBox::parse)
            .filter(|b| b.area() > 1 && b.row1 <= LAST_ROW && b.col1 <= LAST_COL)
            .ok_or_else(|| bad("spills"))?;
        refs.push(SpillRef {
            row: area.row0,
            col: area.col0,
            rows: area.row1 - area.row0 + 1,
            cols: area.col1 - area.col0 + 1,
        });
    }
    let kept = normalize_spills(&refs, LAST_ROW + 1, LAST_COL + 1);
    if kept.len() != refs.len() {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "스필 영역이 한도를 넘었습니다.".to_string(),
        ));
    }
    Ok(kept)
}

pub(crate) fn spills_json(spills: &[SpillRef]) -> String {
    let items: Vec<String> = spills.iter().map(|r| format!("\"{}\"", r.a1())).collect();
    format!("[{}]", items.join(","))
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
    write_layout_split(
        out,
        styling,
        &styling.hidden_rows,
        styling.filter.as_ref(),
        &styling.tables,
    );
}

/// [`write_layout`] with the hidden rows, the filter and the tables a caller has already told apart.
pub(crate) fn write_layout_split(
    out: &mut String,
    styling: &SheetStyling,
    hidden_rows: &[(u32, u32)],
    filter: Option<&SheetFilter>,
    tables: &[SheetTable],
) {
    if !styling.merges.is_empty() {
        let _ = write!(out, ",\"merges\":{}", merges_json(&styling.merges));
    }
    if !styling.spills.is_empty() {
        let _ = write!(out, ",\"spills\":{}", spills_json(&styling.spills));
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
    if !tables.is_empty() {
        let _ = write!(
            out,
            ",\"tables\":{}",
            crate::sheet_table_json::tables_json(tables)
        );
    }
    crate::sheet_view_json::write_view(out, styling);
    crate::sheet_page_json::write_page(out, styling);
    crate::sheet_protect_json::write_sheet_protect(out, styling);
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

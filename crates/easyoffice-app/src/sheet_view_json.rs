//! JSON form of a sheet's frozen pane, tab colour and row and column groups, shared by the open
//! answer, the save request and the row/column edit of a sheet session (PD-208).
//!
//! `freeze`: `{"rows": n, "cols": n}`, the counts of frozen rows and columns. `tabColor`: `rrggbb`.
//! `outline`: `{"rows": [[first, last, level], ...], "cols": [...], "foldedRows": [[first, last], ...],
//! "foldedCols": [...], "summaryAbove": bool, "summaryLeft": bool}`, 1-based and inclusive; a level
//! is 1 to 7 and a folded run lies inside a group. Input over a limit is refused, not trimmed.

use crate::sheet_layout_json::runs_json;
use easyoffice_formats::{
    normalize_runs, parse_hex_rgb, EasyOfficeError, Freeze, LevelRun, Outline, SheetStyling,
    StrictJsonValue, MAX_OUTLINE_LEVEL, MAX_OUTLINE_RUNS,
};
use std::fmt::Write;

const ROWS: u32 = 1_048_576;
const COLS: u32 = 16_384;

fn bad(what: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(format!("보기 설정 JSON이 올바르지 않습니다: {what}"))
}

fn count(v: Option<&StrictJsonValue>, what: &str) -> Result<u32, EasyOfficeError> {
    match v {
        None => Ok(0),
        Some(v) => v
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| bad(what)),
    }
}

pub(crate) fn parse_freeze(v: &StrictJsonValue) -> Result<Option<Freeze>, EasyOfficeError> {
    let obj = v.as_object().ok_or_else(|| bad("freeze"))?;
    let rows = count(StrictJsonValue::get_field(obj, "rows"), "freeze")?;
    let cols = count(StrictJsonValue::get_field(obj, "cols"), "freeze")?;
    if rows == 0 && cols == 0 {
        return Ok(None);
    }
    Freeze::new(rows, cols).map(Some).ok_or_else(|| {
        EasyOfficeError::ResourceBudgetExceeded("틀 고정 행·열 수가 한도를 초과했습니다.".into())
    })
}

/// `[[first, last, level], ...]` as sorted, disjoint zero-based runs; neighbours of one level join.
fn parse_levels(
    v: &StrictJsonValue,
    limit: u32,
    what: &str,
) -> Result<Vec<LevelRun>, EasyOfficeError> {
    let items = v.as_array().ok_or_else(|| bad(what))?;
    if items.len() > MAX_OUTLINE_RUNS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "그룹 구간 수가 한도({MAX_OUTLINE_RUNS})를 초과했습니다."
        )));
    }
    let mut runs: Vec<LevelRun> = Vec::with_capacity(items.len());
    for item in items {
        let triple = item
            .as_array()
            .filter(|t| t.len() == 3)
            .ok_or_else(|| bad(what))?;
        let index = |i: usize| {
            triple[i]
                .as_u64()
                .filter(|n| (1..=u64::from(limit)).contains(n))
                .map(|n| n as u32 - 1)
        };
        let level = triple[2]
            .as_u64()
            .filter(|l| (1..=u64::from(MAX_OUTLINE_LEVEL)).contains(l))
            .map(|l| l as u8);
        let (Some(first), Some(last), Some(level)) = (index(0), index(1), level) else {
            return Err(bad(what));
        };
        if first > last || runs.last().is_some_and(|prev| first <= prev.1) {
            return Err(bad(what));
        }
        match runs.last_mut() {
            Some(prev) if prev.1 + 1 == first && prev.2 == level => prev.1 = last,
            _ => runs.push((first, last, level)),
        }
    }
    Ok(runs)
}

fn parse_folded(
    v: &StrictJsonValue,
    limit: u32,
    what: &str,
) -> Result<Vec<(u32, u32)>, EasyOfficeError> {
    crate::sheet_layout_json::parse_runs_limited(v, limit - 1, what, u64::from(limit))
}

pub(crate) fn parse_outline(v: &StrictJsonValue) -> Result<Outline, EasyOfficeError> {
    let obj = v.as_object().ok_or_else(|| bad("outline"))?;
    let field = |key: &str| StrictJsonValue::get_field(obj, key);
    let flag = |key: &str| field(key).map_or(Ok(false), |v| v.as_bool().ok_or_else(|| bad(key)));
    let mut outline = Outline::default();
    if let Some(v) = field("rows") {
        outline.rows = parse_levels(v, ROWS, "outline.rows")?;
    }
    if let Some(v) = field("cols") {
        outline.cols = parse_levels(v, COLS, "outline.cols")?;
    }
    if let Some(v) = field("foldedRows") {
        outline.collapsed_rows = parse_folded(v, ROWS, "outline.foldedRows")?;
    }
    if let Some(v) = field("foldedCols") {
        outline.collapsed_cols = parse_folded(v, COLS, "outline.foldedCols")?;
    }
    outline.summary_above = flag("summaryAbove")?;
    outline.summary_left = flag("summaryLeft")?;
    normalize_runs(&mut outline.collapsed_rows);
    normalize_runs(&mut outline.collapsed_cols);
    if !outline.within_bounds(ROWS, COLS) {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "그룹 행·열 수가 한도를 초과했거나 접힌 행·열이 그룹 밖에 있습니다.".into(),
        ));
    }
    Ok(outline)
}

/// Reads `freeze`, `tabColor` and `outline` of a sheet object into `styling`.
pub(crate) fn parse_view(
    obj: &[(String, StrictJsonValue)],
    styling: &mut SheetStyling,
) -> Result<(), EasyOfficeError> {
    if let Some(v) = StrictJsonValue::get_field(obj, "freeze") {
        styling.freeze = parse_freeze(v)?;
    }
    if let Some(v) = StrictJsonValue::get_field(obj, "tabColor") {
        styling.tab_color = Some(
            v.as_str()
                .and_then(parse_hex_rgb)
                .ok_or_else(|| bad("tabColor"))?,
        );
    }
    if let Some(v) = StrictJsonValue::get_field(obj, "outline") {
        styling.outline = parse_outline(v)?;
    }
    Ok(())
}

fn levels_json(runs: &[LevelRun]) -> String {
    let mut out = String::from("[");
    for (i, (a, b, level)) in runs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "[{},{},{level}]", a + 1, b + 1);
    }
    out.push(']');
    out
}

/// The outline as one JSON object (an empty list is left out).
pub(crate) fn outline_json(outline: &Outline) -> String {
    let mut out = String::from("{");
    let mut sep = "";
    let mut put = |out: &mut String, key: &str, body: String| {
        let _ = write!(out, "{sep}\"{key}\":{body}");
        sep = ",";
    };
    if !outline.rows.is_empty() {
        put(&mut out, "rows", levels_json(&outline.rows));
    }
    if !outline.cols.is_empty() {
        put(&mut out, "cols", levels_json(&outline.cols));
    }
    if !outline.collapsed_rows.is_empty() {
        put(&mut out, "foldedRows", runs_json(&outline.collapsed_rows));
    }
    if !outline.collapsed_cols.is_empty() {
        put(&mut out, "foldedCols", runs_json(&outline.collapsed_cols));
    }
    if outline.summary_above {
        put(&mut out, "summaryAbove", "true".into());
    }
    if outline.summary_left {
        put(&mut out, "summaryLeft", "true".into());
    }
    out.push('}');
    out
}

/// `,"freeze":..,"tabColor":..,"outline":..` for the parts that are set.
pub(crate) fn write_view(out: &mut String, styling: &SheetStyling) {
    if let Some(f) = styling.freeze {
        let _ = write!(
            out,
            ",\"freeze\":{{\"rows\":{},\"cols\":{}}}",
            f.rows, f.cols
        );
    }
    if let Some(rgb) = styling.tab_color {
        let _ = write!(out, ",\"tabColor\":\"{rgb:06x}\"");
    }
    if !styling.outline.is_empty() {
        let _ = write!(out, ",\"outline\":{}", outline_json(&styling.outline));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(text: &str) -> StrictJsonValue {
        StrictJsonValue::parse(text).unwrap()
    }

    fn read(text: &str) -> Result<SheetStyling, EasyOfficeError> {
        let mut styling = SheetStyling::default();
        parse_view(json(text).as_object().unwrap(), &mut styling).map(|()| styling)
    }

    #[test]
    fn the_view_round_trips_through_json() {
        let text = r#"{"freeze":{"rows":2,"cols":1},"tabColor":"3366cc","outline":{"rows":[[3,4,1],[5,6,2],[7,9,1]],"cols":[[2,3,1]],"foldedRows":[[5,6]],"summaryAbove":true}}"#;
        let styling = read(text).unwrap();
        assert_eq!(styling.freeze, Freeze::new(2, 1));
        assert_eq!(styling.tab_color, Some(0x3366CC));
        assert_eq!(styling.outline.rows, vec![(2, 3, 1), (4, 5, 2), (6, 8, 1)]);
        let mut out = String::new();
        write_view(&mut out, &styling);
        let want = r#","freeze":{"rows":2,"cols":1},"tabColor":"3366cc","outline":{"rows":[[3,4,1],[5,6,2],[7,9,1]],"cols":[[2,3,1]],"foldedRows":[[5,6]],"summaryAbove":true}"#;
        assert_eq!(out, want);
        assert_eq!(read(&format!("{{{}}}", &out[1..])).unwrap(), styling);
    }

    #[test]
    fn hostile_views_are_refused() {
        for bad_text in [
            r#"{"freeze":{"rows":1001}}"#,
            r#"{"freeze":{"cols":101}}"#,
            r#"{"freeze":{"rows":-1}}"#,
            r#"{"freeze":{"rows":4294967296}}"#,
            r#"{"freeze":{"rows":1.5}}"#,
            r#"{"freeze":3}"#,
            r#"{"tabColor":"zzzzzz"}"#,
            r##"{"tabColor":"#3366cc"}"##,
            r#"{"tabColor":"33"}"#,
            r#"{"tabColor":7}"#,
            r#"{"outline":{"rows":[[1,3,8]]}}"#,
            r#"{"outline":{"rows":[[1,3,0]]}}"#,
            r#"{"outline":{"rows":[[0,3,1]]}}"#,
            r#"{"outline":{"rows":[[5,3,1]]}}"#,
            r#"{"outline":{"rows":[[1,1048577,1]]}}"#,
            r#"{"outline":{"rows":[[1,100000,1],[100001,100001,1]]}}"#,
            r#"{"outline":{"rows":[[3,4,1],[4,6,1]]}}"#,
            r#"{"outline":{"rows":[[3,4,1]],"foldedRows":[[3,5]]}}"#,
            r#"{"outline":{"rows":[[3,4,1]],"foldedRows":[[1,2]]}}"#,
            r#"{"outline":{"cols":[[1,16385,1]]}}"#,
            r#"{"outline":{"rows":[[1,2]]}}"#,
            r#"{"outline":{"summaryAbove":1}}"#,
            r#"{"outline":[]}"#,
        ] {
            assert!(read(bad_text).is_err(), "{bad_text}");
        }
    }

    #[test]
    fn an_empty_freeze_is_no_freeze() {
        assert_eq!(
            read(r#"{"freeze":{"rows":0,"cols":0}}"#).unwrap().freeze,
            None
        );
    }
}

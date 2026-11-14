//! JSON form of a sheet's frozen pane, tab colour and row and column groups, shared by the open
//! answer, the save request and the row/column edit of a sheet session (PD-208).
//!
//! `freeze`: `{"rows": n, "cols": n}`, the counts of frozen rows and columns. `tabColor`: `rrggbb`.
//! `hidden`: `true` for a sheet without a tab; `veryHidden`: `true` for one the sheet menu does not list
//! either, which is hidden as well (PD-209).
//! `outline`: `{"rows": [[first, last, level], ...], "cols": [...], "foldedRows": [[first, last], ...],
//! "foldedCols": [...], "summaryAbove": bool, "summaryLeft": bool}`, 1-based and inclusive; a level
//! is 1 to 7 and a folded run lies inside a group. Input over a limit is refused, not trimmed.
//! `view`: `{"active": [row, col], "range": [row1, col1, row2, col2], "zoom": percent, "top": [row, col]}`,
//! 1-based; the selected cell, the selected range that holds it, the zoom (10 to 400) and the first row and
//! column the scrolling part shows. Each key is left out at its default (A1, A1, 100, A1).

use crate::sheet_layout_json::runs_json;
use easyoffice_formats::{
    normalize_runs, parse_hex_rgb, EasyOfficeError, Freeze, LevelRun, Outline, SheetStyling,
    StrictJsonValue, ViewState, MAX_OUTLINE_LEVEL, MAX_OUTLINE_RUNS, MAX_ZOOM, MIN_ZOOM,
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

/// `[n, n, ...]` of exactly `N` 1-based numbers up to `limits` as zero-based.
fn cell_list<const N: usize>(
    v: &StrictJsonValue,
    limits: [u32; N],
    what: &str,
) -> Result<[u32; N], EasyOfficeError> {
    let items = v
        .as_array()
        .filter(|a| a.len() == N)
        .ok_or_else(|| bad(what))?;
    let mut out = [0; N];
    for (i, item) in items.iter().enumerate() {
        out[i] = item
            .as_u64()
            .filter(|n| (1..=u64::from(limits[i])).contains(n))
            .map(|n| n as u32 - 1)
            .ok_or_else(|| bad(what))?;
    }
    Ok(out)
}

pub(crate) fn parse_view_state(v: &StrictJsonValue) -> Result<ViewState, EasyOfficeError> {
    let obj = v.as_object().ok_or_else(|| bad("view"))?;
    let field = |key: &str| StrictJsonValue::get_field(obj, key);
    let mut view = ViewState::default();
    if let Some(v) = field("active") {
        let [row, col] = cell_list(v, [ROWS, COLS], "view.active")?;
        view = ViewState::selecting((row, col), (row, col), (row, col));
    }
    if let Some(v) = field("range") {
        let [r0, c0, r1, c1] = cell_list(v, [ROWS, COLS, ROWS, COLS], "view.range")?;
        let active = if field("active").is_some() {
            view.active
        } else {
            (r0.min(r1), c0.min(c1))
        };
        view = ViewState::selecting(active, (r0, c0), (r1, c1));
        // A range that does not hold the active cell is a lie, not a selection of that cell alone.
        if view.range != (r0.min(r1), c0.min(c1), r0.max(r1), c0.max(c1)) {
            return Err(bad("view.range"));
        }
    }
    if let Some(v) = field("zoom") {
        let percent = v
            .as_u64()
            .filter(|z| (u64::from(MIN_ZOOM)..=u64::from(MAX_ZOOM)).contains(z))
            .ok_or_else(|| bad("view.zoom"))?;
        view.zoom = ViewState::zoom_of(percent as u32);
    }
    if let Some(v) = field("top") {
        let [row, col] = cell_list(v, [ROWS, COLS], "view.top")?;
        view.top_left = (row, col);
    }
    Ok(view)
}

/// Reads `freeze`, `tabColor`, `hidden`, `veryHidden`, `outline` and `view` of a sheet object into `styling`.
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
    if let Some(v) = StrictJsonValue::get_field(obj, "hidden") {
        styling.hidden = v.as_bool().ok_or_else(|| bad("hidden"))?;
    }
    if let Some(v) = StrictJsonValue::get_field(obj, "veryHidden") {
        styling.very_hidden = v.as_bool().ok_or_else(|| bad("veryHidden"))?;
        styling.hidden |= styling.very_hidden;
    }
    if let Some(v) = StrictJsonValue::get_field(obj, "outline") {
        styling.outline = parse_outline(v)?;
    }
    if let Some(v) = StrictJsonValue::get_field(obj, "view") {
        styling.view = parse_view_state(v)?;
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

/// `,"freeze":..,"tabColor":..,"hidden":true,"veryHidden":true,"outline":..,"view":..` for the parts that are set.
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
    if styling.hidden {
        out.push_str(",\"hidden\":true");
    }
    if styling.very_hidden {
        out.push_str(",\"veryHidden\":true");
    }
    if !styling.outline.is_empty() {
        let _ = write!(out, ",\"outline\":{}", outline_json(&styling.outline));
    }
    if !styling.view.is_default() {
        let _ = write!(out, ",\"view\":{}", view_json(&styling.view));
    }
}

/// The view as one JSON object (a default key is left out).
pub(crate) fn view_json(view: &ViewState) -> String {
    let mut out = String::from("{");
    let mut sep = "";
    let (r0, c0, r1, c1) = view.range;
    if view.active != (0, 0) {
        let _ = write!(
            out,
            "\"active\":[{},{}]",
            view.active.0 + 1,
            view.active.1 + 1
        );
        sep = ",";
    }
    if (r0, c0, r1, c1) != (view.active.0, view.active.1, view.active.0, view.active.1) {
        let _ = write!(
            out,
            "{sep}\"range\":[{},{},{},{}]",
            r0 + 1,
            c0 + 1,
            r1 + 1,
            c1 + 1
        );
        sep = ",";
    }
    if let Some(zoom) = view.zoom {
        let _ = write!(out, "{sep}\"zoom\":{zoom}");
        sep = ",";
    }
    if view.top_left != (0, 0) {
        let _ = write!(
            out,
            "{sep}\"top\":[{},{}]",
            view.top_left.0 + 1,
            view.top_left.1 + 1
        );
    }
    out.push('}');
    out
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
        let text = r#"{"freeze":{"rows":2,"cols":1},"tabColor":"3366cc","hidden":true,"outline":{"rows":[[3,4,1],[5,6,2],[7,9,1]],"cols":[[2,3,1]],"foldedRows":[[5,6]],"summaryAbove":true}}"#;
        let styling = read(text).unwrap();
        assert_eq!(styling.freeze, Freeze::new(2, 1));
        assert_eq!(styling.tab_color, Some(0x3366CC));
        assert!(styling.hidden);
        assert_eq!(styling.outline.rows, vec![(2, 3, 1), (4, 5, 2), (6, 8, 1)]);
        let mut out = String::new();
        write_view(&mut out, &styling);
        let want = r#","freeze":{"rows":2,"cols":1},"tabColor":"3366cc","hidden":true,"outline":{"rows":[[3,4,1],[5,6,2],[7,9,1]],"cols":[[2,3,1]],"foldedRows":[[5,6]],"summaryAbove":true}"#;
        assert_eq!(out, want);
        assert_eq!(read(&format!("{{{}}}", &out[1..])).unwrap(), styling);
    }

    #[test]
    fn the_selection_zoom_and_scroll_round_trip_through_json() {
        for text in [
            r#"{"view":{"active":[12,6],"range":[12,6,14,7],"zoom":150,"top":[10,4]}}"#,
            r#"{"view":{"active":[3,3],"range":[2,2,5,5]}}"#,
            r#"{"view":{"zoom":10}}"#,
            r#"{"view":{"zoom":400,"top":[1048576,16384]}}"#,
            r#"{"view":{"range":[1,1,1048576,16384]}}"#,
        ] {
            let styling = read(text).unwrap();
            let mut out = String::new();
            write_view(&mut out, &styling);
            assert_eq!(format!("{{{}}}", &out[1..]), text);
        }
        // A range without a cell, or a zoom of 100, is the default view and writes nothing.
        let mut out = String::new();
        write_view(
            &mut out,
            &read(r#"{"view":{"zoom":100,"active":[1,1]}}"#).unwrap(),
        );
        assert_eq!(out, "");
        // Corners in any order are one range, and the active cell defaults to its first cell.
        let styling = read(r#"{"view":{"range":[5,6,2,3]}}"#).unwrap();
        assert_eq!(
            (styling.view.active, styling.view.range),
            ((1, 2), (1, 2, 4, 5))
        );
    }

    #[test]
    fn a_hostile_view_is_refused() {
        for text in [
            r#"{"view":[]}"#,
            r#"{"view":{"active":[0,1]}}"#,
            r#"{"view":{"active":[1048577,1]}}"#,
            r#"{"view":{"active":[1,16385]}}"#,
            r#"{"view":{"active":[1]}}"#,
            r#"{"view":{"active":[1,1,1]}}"#,
            r#"{"view":{"active":[-1,1]}}"#,
            r#"{"view":{"active":[1.5,1]}}"#,
            r#"{"view":{"active":["1",1]}}"#,
            r#"{"view":{"active":[9,9],"range":[1,1,2,2]}}"#,
            r#"{"view":{"range":[1,1,1048577,1]}}"#,
            r#"{"view":{"zoom":9}}"#,
            r#"{"view":{"zoom":401}}"#,
            r#"{"view":{"zoom":99999999999}}"#,
            r#"{"view":{"zoom":-100}}"#,
            r#"{"view":{"zoom":125.5}}"#,
            r#"{"view":{"zoom":"150"}}"#,
            r#"{"view":{"top":[0,0]}}"#,
            r#"{"view":{"top":[1,16385]}}"#,
        ] {
            assert!(read(text).is_err(), "{text}");
        }
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
            r#"{"hidden":"yes"}"#,
            r#"{"hidden":1}"#,
            r#"{"hidden":null}"#,
            r#"{"veryHidden":"yes"}"#,
            r#"{"veryHidden":0}"#,
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
    fn a_very_hidden_sheet_is_hidden_and_says_so() {
        let styling = read(r#"{"veryHidden":true}"#).unwrap();
        assert!(styling.hidden && styling.very_hidden);
        let mut out = String::new();
        write_view(&mut out, &styling);
        assert_eq!(out, r#","hidden":true,"veryHidden":true"#);
        assert!(
            !read(r#"{"hidden":true,"veryHidden":false}"#)
                .unwrap()
                .very_hidden
        );
    }

    #[test]
    fn an_empty_freeze_is_no_freeze() {
        assert_eq!(
            read(r#"{"freeze":{"rows":0,"cols":0}}"#).unwrap().freeze,
            None
        );
    }
}

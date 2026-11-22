//! JSON form of a sheet's merged ranges, hidden rows and hidden columns (and, from `sheet_view_json`, its view), shared by the open
//! answer, the save request and the row/column edit of a sheet session.
//!
//! `merges`: `["A1:B2", ...]`; `hiddenRows`, `hiddenCols`: `[[first, last], ...]`, 1-based and
//! inclusive. Input over a limit is refused, not trimmed.

use easyoffice_formats::{
    normalize_runs, normalize_spills, run_total, CellBox, EasyOfficeError, RowHeight, RowHeights,
    RowOffsets, SheetFilter, SheetGrid, SheetStyling, SheetTable, SpillRef, StrictJsonValue,
    MAX_HIDDEN_ROWS, MAX_MERGES, MAX_ROW_HEIGHTS, MAX_SPILL_REFS,
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

/// The rows a sheet hides for `SUBTOTAL` and `AGGREGATE` (PD-232): `{"hidden": [[first, last], ...], "filtered": [...]}`, 1-based
/// and inclusive, the rows a user (or a folded group) hid and the rows a filter hid; zero-based runs. Either may be absent.
pub(crate) type HiddenRowRuns = (Vec<(u32, u32)>, Vec<(u32, u32)>);

pub(crate) fn parse_engine_hidden_rows(
    v: &StrictJsonValue,
) -> Result<HiddenRowRuns, EasyOfficeError> {
    let obj = v.as_object().ok_or_else(|| bad("hiddenRows"))?;
    let runs = |key: &str| match StrictJsonValue::get_field(obj, key) {
        None | Some(StrictJsonValue::Null) => Ok(Vec::new()),
        Some(list) => parse_runs_limited(list, LAST_ROW, "hiddenRows", u64::from(LAST_ROW) + 1),
    };
    Ok((runs("hidden")?, runs("filtered")?))
}

/// The `hiddenRows` field of a request without a session: `{"Sheet1": {"hidden": [...], "filtered": [...]}, ...}`; none when absent.
pub(crate) fn hidden_rows_by_sheet(
    entries: &[(String, StrictJsonValue)],
) -> Result<Vec<(String, HiddenRowRuns)>, EasyOfficeError> {
    let Some(field) = StrictJsonValue::get_field(entries, "hiddenRows") else {
        return Ok(Vec::new());
    };
    let sheets = field.as_object().ok_or_else(|| bad("hiddenRows"))?;
    sheets
        .iter()
        .map(|(name, runs)| Ok((name.clone(), parse_engine_hidden_rows(runs)?)))
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

/// `rowHeights`: `[[row, points], ...]` or `[row, points, 0]` for a height that follows the content, rows 1-based
/// (PD-278). Input over a limit or with a height that is not a finite number of points in 0..=409 is refused.
pub(crate) fn parse_row_heights(v: &StrictJsonValue) -> Result<RowHeights, EasyOfficeError> {
    let items = v.as_array().ok_or_else(|| bad("rowHeights"))?;
    if items.len() > MAX_ROW_HEIGHTS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "행 높이 수가 한도({MAX_ROW_HEIGHTS})를 초과했습니다."
        )));
    }
    let mut heights = RowHeights::new();
    for item in items {
        let entry = item
            .as_array()
            .filter(|p| (2..=3).contains(&p.len()))
            .ok_or_else(|| bad("rowHeights"))?;
        let row = entry[0]
            .as_u64()
            .filter(|n| (1..=u64::from(LAST_ROW) + 1).contains(n))
            .map(|n| n as u32 - 1);
        let custom = entry.get(2).map_or(Some(true), |flag| match flag.as_u64() {
            Some(0) => Some(false),
            Some(1) => Some(true),
            _ => None,
        });
        let height = match (&entry[1], custom) {
            (StrictJsonValue::Number(points), Some(custom)) => RowHeight::parse(points, custom),
            _ => None,
        };
        match (row, height) {
            (Some(row), Some(height)) => heights.insert(row, height),
            _ => return Err(bad("rowHeights")),
        };
    }
    Ok(heights)
}

/// `defaultRowHeight`: `[points]` for a default a user set or `[points, 0]` for one that follows the font (PD-286). A
/// value that is not a finite number of points in 0..=409 is refused, as in `rowHeights`.
pub(crate) fn parse_default_row_height(v: &StrictJsonValue) -> Result<RowHeight, EasyOfficeError> {
    let entry = v
        .as_array()
        .filter(|p| (1..=2).contains(&p.len()))
        .ok_or_else(|| bad("defaultRowHeight"))?;
    let custom = entry.get(1).map_or(Some(true), |flag| match flag.as_u64() {
        Some(0) => Some(false),
        Some(1) => Some(true),
        _ => None,
    });
    match (&entry[0], custom) {
        (StrictJsonValue::Number(points), Some(custom)) => RowHeight::parse(points, custom),
        _ => None,
    }
    .ok_or_else(|| bad("defaultRowHeight"))
}

pub(crate) fn default_row_height_json(height: RowHeight) -> String {
    format!(
        "[{}{}]",
        height.points_text(),
        if height.custom { "" } else { ",0" }
    )
}

pub(crate) fn row_heights_json(heights: &RowHeights) -> String {
    let mut out = String::from("[");
    for (i, (row, height)) in heights.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "[{},{}", row + 1, height.points_text());
        out.push_str(if height.custom { "]" } else { ",0]" });
    }
    out.push(']');
    out
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
    if let Some(v) = StrictJsonValue::get_field(obj, "rowHeights") {
        styling.row_heights = parse_row_heights(v)?;
    }
    if let Some(v) = StrictJsonValue::get_field(obj, "defaultRowHeight") {
        styling.default_row_height = Some(parse_default_row_height(v)?);
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
    if let Some(v) =
        StrictJsonValue::get_field(obj, "pivots").filter(|v| !matches!(v, StrictJsonValue::Null))
    {
        styling.pivots = crate::sheet_pivot_json::parse_pivots(v)?;
    }
    if let Some(v) =
        StrictJsonValue::get_field(obj, "notes").filter(|v| !matches!(v, StrictJsonValue::Null))
    {
        styling.notes = crate::sheet_media_notes_json::parse_notes(v)?;
    }
    crate::sheet_view_json::parse_view(obj, styling)?;
    crate::sheet_page_json::parse_page(obj, styling)?;
    crate::sheet_protect_json::parse_sheet_protect(obj, styling)?;
    parse_floating_objects(obj, styling)
}

/// Reads `charts` and `images`, which sit on the rows of the sheet (heights, default height, hidden rows, folded groups,
/// filters), so it runs after the fields that give those (PD-299).
fn parse_floating_objects(
    obj: &[(String, StrictJsonValue)],
    styling: &mut SheetStyling,
) -> Result<(), EasyOfficeError> {
    let present = |key: &str| {
        StrictJsonValue::get_field(obj, key).filter(|v| !matches!(v, StrictJsonValue::Null))
    };
    let (charts, images) = (present("charts"), present("images"));
    if charts.is_none() && images.is_none() {
        return Ok(());
    }
    let grid = SheetGrid::new(&styling.columns, RowOffsets::of(styling));
    if let Some(v) = charts {
        styling.charts = crate::sheet_chart_json::parse_charts(v, &grid)?;
    }
    if let Some(v) = images {
        styling.images = crate::sheet_media_notes_json::parse_images(v, &grid)?;
    }
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
    if !styling.row_heights.is_empty() {
        let _ = write!(
            out,
            ",\"rowHeights\":{}",
            row_heights_json(&styling.row_heights)
        );
    }
    if let Some(height) = styling.default_row_height {
        let _ = write!(
            out,
            ",\"defaultRowHeight\":{}",
            default_row_height_json(height)
        );
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
    if !styling.charts.is_empty() {
        let _ = write!(
            out,
            ",\"charts\":{}",
            crate::sheet_chart_json::charts_json(&styling.charts)
        );
    }
    if !styling.pivots.is_empty() {
        let _ = write!(
            out,
            ",\"pivots\":{}",
            crate::sheet_pivot_json::pivots_json(&styling.pivots)
        );
    }
    if !styling.images.is_empty() {
        out.push_str(",\"images\":");
        crate::sheet_media_notes_json::write_images_json(out, &styling.images);
    }
    if !styling.notes.is_empty() {
        out.push_str(",\"notes\":");
        crate::sheet_media_notes_json::write_notes_json(out, &styling.notes);
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
    fn charts_are_placed_on_the_rows_of_the_sheet_whatever_the_key_order() {
        // 30 pt (40 px) rows with row 2 at 60 pt (80 px): the chart starts at 40 px and spans rows 2..=6 (80 + 4 * 40).
        let sheet = json(concat!(
            r#"{"charts":[{"id":"c","chartType":"column","dataRange":"A1:B3","anchor":"#,
            r#"{"fromCol":1,"fromRow":1,"toCol":5,"toRow":6}}],"rowHeights":[[2,60]],"defaultRowHeight":[30]}"#
        ));
        let mut styling = SheetStyling::default();
        parse_layout(sheet.as_object().unwrap(), &mut styling).unwrap();
        let anchor = styling.charts[0].anchor;
        assert_eq!((anchor.top_px, anchor.height_px), (40, 240));
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
    fn row_heights_round_trip_through_json_with_the_custom_flag() {
        let mut styling = SheetStyling::default();
        let sheet = json(r#"{"rowHeights":[[3,30],[5,14.4,1],[9,21.75,0]]}"#);
        parse_layout(sheet.as_object().unwrap(), &mut styling).unwrap();
        assert_eq!(styling.row_heights.len(), 3);
        assert!(styling.row_heights[&2].custom && !styling.row_heights[&8].custom);
        assert_eq!(styling.row_heights[&4].centipt, 1440);
        let mut out = String::new();
        write_layout(&mut out, &styling);
        assert_eq!(out, r#","rowHeights":[[3,30],[5,14.4],[9,21.75,0]]"#);
    }

    #[test]
    fn hostile_row_heights_are_refused() {
        let flood = format!("{{\"rowHeights\":[{}]}}", vec!["[1,20]"; 100_001].join(","));
        let mut texts = vec![
            r#"{"rowHeights":[[0,20]]}"#.to_string(),
            r#"{"rowHeights":[[1048577,20]]}"#.to_string(),
            r#"{"rowHeights":[[1,0]]}"#.to_string(),
            r#"{"rowHeights":[[1,-3]]}"#.to_string(),
            r#"{"rowHeights":[[1,409.01]]}"#.to_string(),
            r#"{"rowHeights":[[1,1e999]]}"#.to_string(),
            r#"{"rowHeights":[[1,"20"]]}"#.to_string(),
            r#"{"rowHeights":[[1,null]]}"#.to_string(),
            r#"{"rowHeights":[[1,20,2]]}"#.to_string(),
            r#"{"rowHeights":[[1,20,true]]}"#.to_string(),
            r#"{"rowHeights":[[1]]}"#.to_string(),
            r#"{"rowHeights":[1,20]}"#.to_string(),
            r#"{"rowHeights":{"1":20}}"#.to_string(),
        ];
        texts.push(flood);
        for text in texts {
            let v = json(&text);
            assert!(
                parse_layout(v.as_object().unwrap(), &mut SheetStyling::default()).is_err(),
                "{}",
                &text[..text.len().min(60)]
            );
        }
    }

    #[test]
    fn the_default_row_height_round_trips_with_the_custom_flag() {
        for (text, out) in [
            (
                r#"{"defaultRowHeight":[30]}"#,
                r#","defaultRowHeight":[30]"#,
            ),
            (
                r#"{"defaultRowHeight":[14.4,0]}"#,
                r#","defaultRowHeight":[14.4,0]"#,
            ),
            (
                r#"{"defaultRowHeight":[409,1]}"#,
                r#","defaultRowHeight":[409]"#,
            ),
        ] {
            let mut styling = SheetStyling::default();
            parse_layout(json(text).as_object().unwrap(), &mut styling).unwrap();
            let mut written = String::new();
            write_layout(&mut written, &styling);
            assert_eq!(written, out);
        }
    }

    #[test]
    fn hostile_default_row_heights_are_refused() {
        for text in [
            r#"{"defaultRowHeight":[0]}"#,
            r#"{"defaultRowHeight":[-1]}"#,
            r#"{"defaultRowHeight":[409.01]}"#,
            r#"{"defaultRowHeight":[1e999]}"#,
            r#"{"defaultRowHeight":["20"]}"#,
            r#"{"defaultRowHeight":[null]}"#,
            r#"{"defaultRowHeight":[20,2]}"#,
            r#"{"defaultRowHeight":[20,true]}"#,
            r#"{"defaultRowHeight":[]}"#,
            r#"{"defaultRowHeight":[20,1,0]}"#,
            r#"{"defaultRowHeight":20}"#,
            r#"{"defaultRowHeight":{"pt":20}}"#,
        ] {
            let v = json(text);
            assert!(
                parse_layout(v.as_object().unwrap(), &mut SheetStyling::default()).is_err(),
                "{text}"
            );
        }
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

//! Auto filter requests (PD-198): `evaluate_sheet` with `sessionAction:"filter"`. The command set
//! stays at 15.
//!
//! The page sends the filter (the JSON of `sheet_filter_json`) and, for every column the request
//! needs, the stored text of the rows below the header, with the number format code of the cell
//! ranges that have one: `data: [{col, values: [text...], fmt: [[first, last, code], ...]}]`
//! (`col` 1-based, `first`/`last` 0-based offsets into `values`). The engine judges the displayed
//! text, the same text a cell shows.
//!
//! - `op:"apply"` -> `{hidden: [[first, last], ...], count}`: the rows (1-based) the conditions hide.
//! - `op:"values"`, `col`, `query?` -> `{items: [[text, count], ...], blank, more}`: the distinct
//!   texts of that column among the rows the other columns leave visible.

use crate::sheet_filter_json::parse_filter;
use crate::sheet_layout_json::runs_json;
use crate::EasyOfficeIpcRouter;
use easyoffice_formats::{
    code_within_bounds, resolve_code, EasyOfficeError, SheetFilter, SheetStyling, StrictJsonValue,
    MAX_FILTER_VALUES,
};
use easyoffice_sheet::filter::{hidden_bits, split_hidden_rows, value_list, ColumnText, RowBits};
use easyoffice_sheet::format_cell_value;
use std::fmt::Write;

type Entries = [(String, StrictJsonValue)];

/// Cell texts one request may carry over all columns.
const MAX_REQUEST_CELLS: usize = 2_000_000;
/// A longer stored text is text no number format changes.
const MAX_FORMATTED_BYTES: usize = 512;
/// Characters of a search text.
const MAX_QUERY_CHARS: usize = 255;

fn invalid(what: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(format!("필터 요청이 올바르지 않습니다: {what}"))
}

struct Column {
    col: u32,
    texts: Vec<String>,
}

/// Applies the format runs of one data column to its texts.
fn apply_formats(texts: &mut [String], runs: &StrictJsonValue) -> Result<(), EasyOfficeError> {
    for run in runs.as_array().ok_or_else(|| invalid("fmt"))? {
        let parts = run
            .as_array()
            .filter(|p| p.len() == 3)
            .ok_or_else(|| invalid("fmt"))?;
        let offset = |i: usize| parts[i].as_u64().and_then(|n| usize::try_from(n).ok());
        let (Some(first), Some(last), Some(code)) = (offset(0), offset(1), parts[2].as_str())
        else {
            return Err(invalid("fmt"));
        };
        if first > last || last >= texts.len() {
            return Err(invalid("fmt"));
        }
        if !code_within_bounds(code) {
            continue;
        }
        for text in &mut texts[first..=last] {
            if text.len() > MAX_FORMATTED_BYTES {
                continue;
            }
            if let Some(shown) = format_cell_value(code, text) {
                *text = shown.text;
            }
        }
    }
    Ok(())
}

fn parse_data(entries: &Entries, filter: &SheetFilter) -> Result<Vec<Column>, EasyOfficeError> {
    let items = StrictJsonValue::get_field(entries, "data")
        .and_then(StrictJsonValue::as_array)
        .ok_or_else(|| invalid("data"))?;
    let rows = (filter.range.row1 - filter.range.row0) as usize;
    let mut total = 0usize;
    let mut columns = Vec::with_capacity(items.len());
    for item in items {
        let obj = item.as_object().ok_or_else(|| invalid("data"))?;
        let col = StrictJsonValue::get_field(obj, "col")
            .and_then(StrictJsonValue::as_u64)
            .filter(|c| (1..=16_384).contains(c))
            .map(|c| c as u32 - 1)
            .ok_or_else(|| invalid("col"))?;
        let values = StrictJsonValue::get_field(obj, "values")
            .and_then(StrictJsonValue::as_array)
            .filter(|v| v.len() == rows)
            .ok_or_else(|| invalid("values"))?;
        total += values.len();
        if total > MAX_REQUEST_CELLS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "필터 요청의 셀이 {MAX_REQUEST_CELLS}개를 넘습니다."
            )));
        }
        let mut texts = values
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| invalid("values"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(runs) = StrictJsonValue::get_field(obj, "fmt") {
            apply_formats(&mut texts, runs)?;
        }
        columns.push(Column { col, texts });
    }
    Ok(columns)
}

fn push_text(out: &mut String, text: &str) {
    out.push('"');
    EasyOfficeIpcRouter::escape_json_into(out, text);
    out.push('"');
}

impl EasyOfficeIpcRouter {
    pub(crate) fn sheet_filter(entries: &Entries) -> Result<String, EasyOfficeError> {
        let filter = parse_filter(
            StrictJsonValue::get_field(entries, "filter").ok_or_else(|| invalid("filter"))?,
        )?;
        let columns = parse_data(entries, &filter)?;
        let views: Vec<ColumnText<'_>> = columns
            .iter()
            .map(|c| ColumnText {
                col: c.col,
                texts: &c.texts,
            })
            .collect();
        match Self::optional_json_str(entries, "op")? {
            Some("apply") => {
                let bits = hidden_bits(&filter, &views, None)?;
                Ok(format!(
                    "{{\"hidden\":{},\"count\":{}}}",
                    runs_json(&bits.runs(filter.range.row0 + 1)),
                    bits.count()
                ))
            }
            Some("values") => values_answer(entries, &filter, &views),
            _ => Err(invalid("op")),
        }
    }
}

fn values_answer(
    entries: &Entries,
    filter: &SheetFilter,
    views: &[ColumnText<'_>],
) -> Result<String, EasyOfficeError> {
    let col = StrictJsonValue::get_field(entries, "col")
        .and_then(StrictJsonValue::as_u64)
        .filter(|c| (1..=16_384).contains(c))
        .map(|c| c as u32 - 1)
        .filter(|c| (filter.range.col0..=filter.range.col1).contains(c))
        .ok_or_else(|| invalid("col"))?;
    let query = EasyOfficeIpcRouter::optional_json_str(entries, "query")?.unwrap_or("");
    if query.chars().count() > MAX_QUERY_CHARS {
        return Err(invalid("query"));
    }
    let target = views
        .iter()
        .find(|v| v.col == col)
        .ok_or_else(|| invalid("열 값이 없습니다"))?;
    let excluded = hidden_bits(filter, views, Some(col))?;
    let list = value_list(target.texts, &excluded, query, MAX_FILTER_VALUES);
    let mut out = String::from("{\"items\":[");
    for (i, item) in list.items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('[');
        push_text(&mut out, &item.text);
        let _ = write!(out, ",{}]", item.count);
    }
    let _ = write!(out, "],\"blank\":{},\"more\":{}}}", list.blank, list.more);
    Ok(out)
}

/// The filter of an opened sheet with the hidden rows told apart (the file has one `hidden` flag
/// per row): the rows the filter's conditions also hide become the filter's, the rest stay the
/// user's. `None` when the sheet has no filter with conditions.
pub(crate) fn split_for_open(
    cells: &[(u32, u32, String)],
    styling: &SheetStyling,
) -> Option<(SheetFilter, Vec<(u32, u32)>)> {
    let mut filter = styling.filter.clone().filter(SheetFilter::has_criteria)?;
    let range = filter.range;
    let rows = (range.row1 - range.row0) as usize;
    let cols: Vec<u32> = filter.columns.iter().map(|c| c.col).collect();
    let mut texts: Vec<Vec<String>> = vec![vec![String::new(); rows]; cols.len()];
    let mut unread = RowBits::new(rows);
    for (row, col, raw) in cells {
        let (Some(at), true) = (
            cols.binary_search(col).ok(),
            *row > range.row0 && *row <= range.row1,
        ) else {
            continue;
        };
        let offset = (*row - range.row0 - 1) as usize;
        if raw.starts_with('=') {
            unread.set(offset);
            continue;
        }
        let shown = styling
            .cells
            .get(&(*row, *col))
            .and_then(|style| resolve_code(style.num_fmt, &styling.num_fmts))
            .filter(|_| raw.len() <= MAX_FORMATTED_BYTES)
            .and_then(|code| format_cell_value(code, raw));
        texts[at][offset] = shown.map_or_else(|| raw.clone(), |s| s.text);
    }
    let views: Vec<ColumnText<'_>> = cols
        .iter()
        .zip(&texts)
        .map(|(&col, texts)| ColumnText { col, texts })
        .collect();
    let mut hidden = styling.hidden_rows.clone();
    split_hidden_rows(&mut filter, &mut hidden, &views, &unread).ok()?;
    Some((filter, hidden))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(body: &str) -> Result<String, EasyOfficeError> {
        EasyOfficeIpcRouter::dispatch("evaluate_sheet", body)
    }

    const FILTER: &str = r#"{"range":"A1:B7","columns":[{"col":1,"values":["영업","개발"]},{"col":2,"conditions":[{"op":"greaterThan","text":"10"}]}]}"#;
    const DATA: &str = r#"[{"col":1,"values":["영업","영업","개발","","기획","영업"]},{"col":2,"values":["5","20","30","40","50","abc"]}]"#;

    #[test]
    fn apply_answers_the_hidden_rows_one_based() {
        let json = call(&format!(
            r#"{{"sessionAction":"filter","op":"apply","filter":{FILTER},"data":{DATA}}}"#
        ))
        .unwrap();
        // Data rows 1..6 are sheet rows 2..7: row 2 (5 <= 10), 5 (blank), 6 (기획), 7 ("abc").
        assert_eq!(json, r#"{"hidden":[[2,2],[5,7]],"count":4}"#);
    }

    #[test]
    fn values_leave_out_the_rows_other_columns_hide() {
        let json = call(&format!(
            r#"{{"sessionAction":"filter","op":"values","col":1,"filter":{FILTER},"data":{DATA}}}"#
        ))
        .unwrap();
        // Column B hides rows 2 (5) and 7 ("abc"): the rest are 영업, 개발, "", 기획.
        assert_eq!(
            json,
            r#"{"items":[["개발",1],["기획",1],["영업",1]],"blank":1,"more":false}"#
        );
        let searched = call(&format!(
            r#"{{"sessionAction":"filter","op":"values","col":1,"query":"영","filter":{FILTER},"data":{DATA}}}"#
        ))
        .unwrap();
        assert_eq!(searched, r#"{"items":[["영업",1]],"blank":0,"more":false}"#);
    }

    #[test]
    fn the_displayed_text_is_judged_not_the_stored_one() {
        let filter = r#"{"range":"A1:A3","columns":[{"col":1,"values":["2023-03-15"]}]}"#;
        let data = r#"[{"col":1,"values":["45000","45001"],"fmt":[[0,1,"yyyy-mm-dd"]]}]"#;
        let json = call(&format!(
            r#"{{"sessionAction":"filter","op":"apply","filter":{filter},"data":{data}}}"#
        ))
        .unwrap();
        assert_eq!(json, r#"{"hidden":[[3,3]],"count":1}"#);
    }

    #[test]
    fn opening_tells_the_filter_s_rows_from_the_user_s() {
        use easyoffice_formats::{CellBox, FilterColumn, FilterCriteria};
        let mut filter = SheetFilter::new(CellBox::parse("A1:A5").unwrap());
        filter.columns = vec![FilterColumn {
            col: 0,
            criteria: FilterCriteria::Values {
                blank: false,
                values: vec!["영업".into()],
            },
        }];
        let styling = SheetStyling {
            filter: Some(filter),
            hidden_rows: vec![(2, 3), (7, 7)],
            ..SheetStyling::default()
        };
        let cells: Vec<(u32, u32, String)> = ["부서", "영업", "개발", "영업", "개발"]
            .iter()
            .enumerate()
            .map(|(i, t)| (i as u32, 0, t.to_string()))
            .collect();
        let (filter, user) = split_for_open(&cells, &styling).unwrap();
        // Row 3 (index 2) is 개발 and hidden by the filter; row 4 is 영업 and the user's.
        assert_eq!(filter.hidden, vec![(2, 2)]);
        assert_eq!(user, vec![(3, 3), (7, 7)]);
        assert!(split_for_open(&cells, &SheetStyling::default()).is_none());
    }

    #[test]
    fn bad_requests_are_refused() {
        let short = r#"[{"col":1,"values":["a"]},{"col":2,"values":["1"]}]"#;
        for (op, data) in [
            ("apply", short),
            ("apply", r#"[{"col":1,"values":["a","b","c","d","e","f"]}]"#),
            ("nope", DATA),
            ("apply", r#"[{"col":1,"values":[1,2,3,4,5,6]}]"#),
            ("apply", r#"[{"col":0,"values":["a","b","c","d","e","f"]}]"#),
        ] {
            let body = format!(
                r#"{{"sessionAction":"filter","op":"{op}","filter":{FILTER},"data":{data}}}"#
            );
            assert!(call(&body).is_err(), "{op} {data}");
        }
        let too_many = format!(
            r#"{{"sessionAction":"filter","op":"values","col":9,"filter":{FILTER},"data":{DATA}}}"#
        );
        assert!(call(&too_many).is_err());
    }
}

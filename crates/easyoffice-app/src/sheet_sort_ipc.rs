//! Row sort request (PD-203): `evaluate_sheet` with `sessionAction:"sort"`. The command set stays
//! at 15.
//!
//! The page sends the key columns of the rows it may move: `keys: [{asc, values: [..]}]`, one
//! entry per key (first key first, at most 64), `values` holding one item per row (`null` for a
//! blank cell, a number, or a text) and the same number of rows in every key, plus
//! `caseSensitive`. The answer is `{order: [i, ...]}`: the row that goes to position `n` is the
//! one that was at `order[n]` (0-based, stable, blank keys last in both directions; the rules are
//! in `easyoffice_sheet::sort_keys`). Which rows may move (hidden and filtered rows stay) and how
//! the cells follow is the page's business.
//!
//! With `formulas: [[row, "=A2"], ...]` (`row` is the index of the row the formula sits in) the
//! page also sends `sheet`, `range: {r1, r2}` (the sorted rows, 1-based), `rows` (the 1-based sheet
//! row of every row index) and gets `formulas` back: every formula as it reads after its row moved
//! (the rule of PD-181: relative row references into the sorted rows follow, the rest stays).

use crate::EasyOfficeIpcRouter;
use easyoffice_formats::{EasyOfficeError, StrictJsonValue};
use easyoffice_sheet::{sort_order, sorted_formula, SortColumn, MAX_SORT_KEYS};
use std::fmt::Write;

/// Cells (rows times keys) one request may carry.
const MAX_REQUEST_CELLS: usize = 2_000_000;

fn invalid(what: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(format!("정렬 요청이 올바르지 않습니다: {what}"))
}

fn push_value(column: &mut SortColumn, value: &StrictJsonValue) -> Result<(), EasyOfficeError> {
    match value {
        StrictJsonValue::Null => column.push_empty(),
        StrictJsonValue::Number(text) => {
            let n = text
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite())
                .ok_or_else(|| invalid("values"))?;
            column.push_number(n);
        }
        StrictJsonValue::Str(text) => column.push_text(text),
        _ => return Err(invalid("values")),
    }
    Ok(())
}

impl EasyOfficeIpcRouter {
    pub(crate) fn sheet_sort(
        entries: &[(String, StrictJsonValue)],
    ) -> Result<String, EasyOfficeError> {
        let case_sensitive = match StrictJsonValue::get_field(entries, "caseSensitive") {
            None => false,
            Some(v) => v.as_bool().ok_or_else(|| invalid("caseSensitive"))?,
        };
        let keys = StrictJsonValue::get_field(entries, "keys")
            .and_then(StrictJsonValue::as_array)
            .filter(|k| (1..=MAX_SORT_KEYS).contains(&k.len()))
            .ok_or_else(|| invalid("keys"))?;
        let mut columns = Vec::with_capacity(keys.len());
        let mut rows = None;
        for key in keys {
            let obj = key.as_object().ok_or_else(|| invalid("keys"))?;
            let ascending = StrictJsonValue::get_field(obj, "asc")
                .and_then(StrictJsonValue::as_bool)
                .ok_or_else(|| invalid("asc"))?;
            let values = StrictJsonValue::get_field(obj, "values")
                .and_then(StrictJsonValue::as_array)
                .ok_or_else(|| invalid("values"))?;
            if *rows.get_or_insert(values.len()) != values.len() {
                return Err(invalid("values"));
            }
            if values.len().saturating_mul(keys.len()) > MAX_REQUEST_CELLS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "정렬 요청의 셀이 {MAX_REQUEST_CELLS}개를 넘습니다."
                )));
            }
            let mut column = SortColumn::new(ascending, case_sensitive, values.len());
            for value in values {
                push_value(&mut column, value)?;
            }
            columns.push(column);
        }
        let order = sort_order(&columns);
        let mut out = String::with_capacity(order.len() * 7 + 12);
        out.push_str("{\"order\":[");
        for (i, row) in order.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let _ = write!(out, "{row}");
        }
        out.push(']');
        if let Some(formulas) = StrictJsonValue::get_field(entries, "formulas") {
            out.push_str(",\"formulas\":[");
            push_formulas(&mut out, entries, formulas, &order)?;
            out.push(']');
        }
        out.push('}');
        Ok(out)
    }
}

/// Rows a request may name (1-based sheet rows).
const MAX_SHEET_ROW: u64 = 1_048_576;

fn row_field(obj: &[(String, StrictJsonValue)], name: &str) -> Result<u32, EasyOfficeError> {
    StrictJsonValue::get_field(obj, name)
        .and_then(StrictJsonValue::as_u64)
        .filter(|r| (1..=MAX_SHEET_ROW).contains(r))
        .map(|r| r as u32 - 1)
        .ok_or_else(|| invalid(name))
}

/// Writes the quoted formulas of the request after their rows moved, comma separated.
fn push_formulas(
    out: &mut String,
    entries: &[(String, StrictJsonValue)],
    formulas: &StrictJsonValue,
    order: &[u32],
) -> Result<(), EasyOfficeError> {
    let items = formulas.as_array().ok_or_else(|| invalid("formulas"))?;
    let sheet = EasyOfficeIpcRouter::optional_json_str(entries, "sheet")?.unwrap_or("");
    let range = StrictJsonValue::get_field(entries, "range")
        .and_then(StrictJsonValue::as_object)
        .ok_or_else(|| invalid("range"))?;
    let (first, last) = (row_field(range, "r1")?, row_field(range, "r2")?);
    let rows = StrictJsonValue::get_field(entries, "rows")
        .and_then(StrictJsonValue::as_array)
        .filter(|r| r.len() == order.len())
        .ok_or_else(|| invalid("rows"))?;
    let sheet_rows = rows
        .iter()
        .map(|r| {
            r.as_u64()
                .filter(|r| (1..=MAX_SHEET_ROW).contains(r))
                .map(|r| r as u32 - 1)
                .ok_or_else(|| invalid("rows"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    // Where each row index goes.
    let mut place = vec![0usize; order.len()];
    for (at, &row) in order.iter().enumerate() {
        place[row as usize] = at;
    }
    for (i, item) in items.iter().enumerate() {
        let pair = item
            .as_array()
            .filter(|p| p.len() == 2)
            .ok_or_else(|| invalid("formulas"))?;
        let row = pair[0]
            .as_u64()
            .and_then(|r| usize::try_from(r).ok())
            .filter(|&r| r < order.len())
            .ok_or_else(|| invalid("formulas"))?;
        let text = pair[1].as_str().ok_or_else(|| invalid("formulas"))?;
        if i > 0 {
            out.push(',');
        }
        push_text(
            out,
            &sorted_formula(
                text,
                sheet,
                first,
                last,
                sheet_rows[row],
                sheet_rows[place[row]],
            ),
        );
    }
    Ok(())
}

fn push_text(out: &mut String, text: &str) {
    out.push('"');
    EasyOfficeIpcRouter::escape_json_into(out, text);
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(body: &str) -> Result<String, EasyOfficeError> {
        EasyOfficeIpcRouter::dispatch("evaluate_sheet", body)
    }

    #[test]
    fn answers_the_stable_order_of_the_keys() {
        let json = call(
            r#"{"sessionAction":"sort","keys":[{"asc":true,"values":["b",null,10,"a",2,""]}]}"#,
        )
        .unwrap();
        assert_eq!(json, r#"{"order":[4,2,3,0,1,5]}"#);
        let two = call(
            r#"{"sessionAction":"sort","keys":[{"asc":true,"values":[1,1,0,1]},{"asc":false,"values":[5,9,7,9]}]}"#,
        )
        .unwrap();
        assert_eq!(two, r#"{"order":[2,1,3,0]}"#);
    }

    #[test]
    fn case_matters_only_when_asked() {
        let body = |flag: &str| {
            format!(
                r#"{{"sessionAction":"sort","caseSensitive":{flag},"keys":[{{"asc":true,"values":["A","a","B","b"]}}]}}"#
            )
        };
        assert_eq!(call(&body("false")).unwrap(), r#"{"order":[0,1,2,3]}"#);
        assert_eq!(call(&body("true")).unwrap(), r#"{"order":[1,0,3,2]}"#);
    }

    #[test]
    fn bad_requests_are_refused() {
        let many = format!(
            r#"{{"sessionAction":"sort","keys":[{}]}}"#,
            vec![r#"{"asc":true,"values":[1]}"#; 65].join(",")
        );
        for body in [
            r#"{"sessionAction":"sort"}"#.to_string(),
            r#"{"sessionAction":"sort","keys":[]}"#.to_string(),
            r#"{"sessionAction":"sort","keys":[{"values":[1]}]}"#.to_string(),
            r#"{"sessionAction":"sort","keys":[{"asc":true,"values":[1,2]},{"asc":true,"values":[1]}]}"#
                .to_string(),
            r#"{"sessionAction":"sort","keys":[{"asc":true,"values":[true]}]}"#.to_string(),
            r#"{"sessionAction":"sort","keys":[{"asc":true,"values":[[1]]}]}"#.to_string(),
            r#"{"sessionAction":"sort","caseSensitive":"yes","keys":[{"asc":true,"values":[1]}]}"#
                .to_string(),
            many,
        ] {
            assert!(call(&body).is_err(), "{body}");
        }
    }

    #[test]
    fn formulas_come_back_as_they_read_after_their_rows_moved() {
        // Rows 2..4 of the sheet sorted by their key: row 4 goes first, rows 2 and 3 follow.
        let json = call(
            r#"{"sessionAction":"sort","sheet":"S","range":{"r1":2,"r2":4},"rows":[2,3,4],
            "keys":[{"asc":true,"values":[5,9,1]}],
            "formulas":[[0,"=A2*2+A1+$A$2"],[2,"=SUM(A3:A4)"],[1,"text"]]}"#,
        )
        .unwrap();
        assert_eq!(
            json,
            r#"{"order":[2,0,1],"formulas":["=A3*2+A1+$A$2","=SUM(A1:A2)","text"]}"#
        );
    }

    #[test]
    fn hidden_rows_in_between_give_the_distance_the_row_travels() {
        // Rows 2 and 5 may move (3 and 4 are hidden): the formula of row 2 moves to row 5.
        let json = call(
            r#"{"sessionAction":"sort","sheet":"S","range":{"r1":2,"r2":6},"rows":[2,5],
            "keys":[{"asc":false,"values":[1,2]}],"formulas":[[0,"=A2+1"]]}"#,
        )
        .unwrap();
        assert_eq!(json, r#"{"order":[1,0],"formulas":["=A5+1"]}"#);
        let bad = call(
            r#"{"sessionAction":"sort","range":{"r1":2,"r2":6},"rows":[2],
            "keys":[{"asc":true,"values":[1,2]}],"formulas":[[0,"=A2"]]}"#,
        );
        assert!(bad.is_err());
    }

    #[test]
    fn the_answer_for_many_rows_is_a_permutation() {
        let values: Vec<String> = (0..5000).map(|i| ((i * 7919) % 1000).to_string()).collect();
        let body = format!(
            r#"{{"sessionAction":"sort","keys":[{{"asc":false,"values":[{}]}}]}}"#,
            values.join(",")
        );
        let json = call(&body).unwrap();
        let order: Vec<usize> = json
            .trim_start_matches("{\"order\":[")
            .trim_end_matches("]}")
            .split(',')
            .map(|n| n.parse().unwrap())
            .collect();
        let mut seen = order.clone();
        seen.sort_unstable();
        assert!(seen.iter().enumerate().all(|(i, &v)| i == v));
        let nums: Vec<i64> = values.iter().map(|v| v.parse().unwrap()).collect();
        assert!(order.windows(2).all(|w| nums[w[0]] >= nums[w[1]]));
    }
}

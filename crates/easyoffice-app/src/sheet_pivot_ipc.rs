//! Pivot tables over the sheet engine's `compute_pivot_grid` (PD-171).
//!
//! `evaluate_sheet` with `sessionAction:"pivot"` takes the cells the page sends (the same
//! `sheets`/`cells` the stateless evaluation takes) and answers with the grouped values; the page
//! writes them into a new sheet. The command set stays at 15.

use crate::EasyOfficeIpcRouter;
use easyoffice_formats::{EasyOfficeError, StrictJsonValue};
use easyoffice_sheet::{CellRange, PivotAggregation};
use std::fmt::Write;

/// Cells (rows x columns) of a pivot answer.
const MAX_PIVOT_CELLS: usize = 250_000;

fn invalid(msg: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(msg.to_string())
}

fn offset(
    entries: &[(String, StrictJsonValue)],
    key: &str,
) -> Result<Option<usize>, EasyOfficeError> {
    match StrictJsonValue::get_field(entries, key) {
        None | Some(StrictJsonValue::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| invalid("피벗 열 번호는 0 이상의 정수여야 합니다.")),
    }
}

fn aggregation(name: &str) -> Result<PivotAggregation, EasyOfficeError> {
    Ok(match name {
        "sum" => PivotAggregation::Sum,
        "average" => PivotAggregation::Average,
        "count" => PivotAggregation::Count,
        "min" => PivotAggregation::Min,
        "max" => PivotAggregation::Max,
        _ => {
            return Err(invalid(
                "피벗 집계는 sum, average, count, min, max 중 하나입니다.",
            ))
        }
    })
}

fn number(out: &mut String, n: f64) {
    if n.is_finite() {
        let _ = write!(out, "{n}");
    } else {
        out.push_str("null");
    }
}

fn strings(out: &mut String, items: &[String]) {
    out.push('[');
    for (i, s) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "\"{}\"", EasyOfficeIpcRouter::escape_json_str(s));
    }
    out.push(']');
}

fn numbers(out: &mut String, items: &[f64]) {
    out.push('[');
    for (i, n) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        number(out, *n);
    }
    out.push(']');
}

impl EasyOfficeIpcRouter {
    /// `{sheet, sheets?, cells, range:"A1:D90", header?, row, col?, value, agg}` ->
    /// `{rowKeys, columnKeys, matrix, rowTotals, columnTotals, grandTotal}`; `row`, `col` and
    /// `value` are column offsets inside `range`.
    pub(crate) fn sheet_pivot(
        entries: &[(String, StrictJsonValue)],
    ) -> Result<String, EasyOfficeError> {
        let default_sheet = Self::resolve_default_sheet_from_entries(entries)?;
        let wb = Self::build_workbook_from_json_entries(entries, &default_sheet)?;
        let sheet = wb
            .sheet(&default_sheet)
            .ok_or_else(|| invalid("피벗 대상 시트를 찾을 수 없습니다."))?;
        let range = CellRange::parse(Self::optional_json_str(entries, "range")?.unwrap_or(""))?;
        let (_, width) = range.dimensions();
        let row = offset(entries, "row")?.ok_or_else(|| invalid("피벗 행 열이 필요합니다."))?;
        let value = offset(entries, "value")?.ok_or_else(|| invalid("피벗 값 열이 필요합니다."))?;
        let col = offset(entries, "col")?;
        if [Some(row), Some(value), col]
            .iter()
            .flatten()
            .any(|&o| o >= width)
        {
            return Err(invalid("피벗 열이 선택한 범위를 벗어났습니다."));
        }
        let header = !matches!(
            StrictJsonValue::get_field(entries, "header"),
            Some(StrictJsonValue::Bool(false))
        );
        let agg = aggregation(Self::optional_json_str(entries, "agg")?.unwrap_or("sum"))?;
        let pivot = sheet.compute_pivot_grid(&range, header, row, col, value, agg);
        if pivot.row_keys.len().saturating_mul(pivot.column_keys.len()) > MAX_PIVOT_CELLS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "피벗 결과가 {MAX_PIVOT_CELLS}칸을 넘습니다. 행·열 항목을 줄여 주세요."
            )));
        }
        let mut out = String::from("{\"rowKeys\":");
        strings(&mut out, &pivot.row_keys);
        out.push_str(",\"columnKeys\":");
        strings(&mut out, &pivot.column_keys);
        out.push_str(",\"matrix\":[");
        for (i, line) in pivot.matrix.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            numbers(&mut out, line);
        }
        out.push_str("],\"rowTotals\":");
        numbers(&mut out, &pivot.row_totals);
        out.push_str(",\"columnTotals\":");
        numbers(&mut out, &pivot.column_totals);
        out.push_str(",\"grandTotal\":");
        number(&mut out, pivot.grand_total);
        out.push('}');
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pivot(extra: &str) -> Result<String, EasyOfficeError> {
        let cells = [
            ("A1", "부서"),
            ("B1", "분기"),
            ("C1", "매출"),
            ("A2", "영업"),
            ("B2", "1Q"),
            ("C2", "10"),
            ("A3", "영업"),
            ("B3", "2Q"),
            ("C3", "20"),
            ("A4", "개발"),
            ("B4", "1Q"),
            ("C4", "5"),
            ("A5", "영업"),
            ("B5", "1Q"),
            ("C5", "=C2+1"),
        ]
        .iter()
        .map(|(c, v)| format!("{{\"cell\":\"{c}\",\"value\":\"{v}\"}}"))
        .collect::<Vec<_>>()
        .join(",");
        EasyOfficeIpcRouter::dispatch(
            "evaluate_sheet",
            &format!("{{\"sessionAction\":\"pivot\",\"sheet\":\"S\",\"cells\":[{cells}],\"range\":\"A1:C5\",{extra}}}"),
        )
    }

    #[test]
    fn groups_rows_and_columns_with_totals() {
        let json = pivot("\"row\":0,\"col\":1,\"value\":2,\"agg\":\"sum\"").unwrap();
        assert!(json.contains("\"rowKeys\":[\"영업\",\"개발\"]"), "{json}");
        assert!(json.contains("\"columnKeys\":[\"1Q\",\"2Q\"]"), "{json}");
        assert!(json.contains("\"matrix\":[[21,20],[5,0]]"), "{json}");
        assert!(json.contains("\"rowTotals\":[41,5]"), "{json}");
        assert!(json.contains("\"columnTotals\":[26,20]"), "{json}");
        assert!(json.contains("\"grandTotal\":46"), "{json}");
    }

    #[test]
    fn one_dimension_and_other_aggregates() {
        let avg = pivot("\"row\":0,\"value\":2,\"agg\":\"average\"").unwrap();
        assert!(avg.contains("\"columnKeys\":[\"Total\"]"), "{avg}");
        assert!(
            avg.contains("\"matrix\":[[13.666666666666666],[5]]"),
            "{avg}"
        );
        let count = pivot("\"row\":1,\"value\":0,\"agg\":\"count\"").unwrap();
        assert!(count.contains("\"matrix\":[[3],[1]]"), "{count}");
    }

    #[test]
    fn bad_requests_are_refused() {
        for extra in [
            "\"row\":0,\"value\":9",
            "\"value\":2",
            "\"row\":0,\"value\":2,\"agg\":\"median\"",
            "\"row\":0,\"col\":7,\"value\":2",
        ] {
            assert!(pivot(extra).is_err(), "{extra}");
        }
    }
}

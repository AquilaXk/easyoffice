//! Pivot tables over the sheet engine's `compute_pivot_within` (PD-171).
//!
//! `evaluate_sheet` with `sessionAction:"pivot"` takes the cells the page sends (the same
//! `sheets`/`cells` the stateless evaluation takes) and answers with the grouped values; the page
//! writes them into a new sheet. The command set stays at 15.

use crate::{eval_limit, EasyOfficeIpcRouter};
use easyoffice_formats::{
    EasyOfficeError, StrictJsonValue, MAX_PIVOT_FILTER_VALUES, MAX_PIVOT_TEXT_BYTES,
};
use easyoffice_sheet::{
    CellRange, DynamicPivotSpec, DynamicPivotSummary, PivotAggregation, PivotError,
    PivotFieldFilter, PivotSpec, MAX_PIVOT_FIELDS,
};
use std::collections::BTreeMap;
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

fn map_pivot_err(err: PivotError) -> EasyOfficeError {
    match err {
        PivotError::NoSheet => invalid("피벗 대상 시트를 찾을 수 없습니다."),
        PivotError::InvalidSpec => invalid("피벗 필드 지정이 올바르지 않거나 범위를 벗어났습니다."),
        PivotError::TooLarge => EasyOfficeError::ResourceBudgetExceeded(format!(
            "피벗 결과가 {MAX_PIVOT_CELLS}칸을 넘습니다. 행·열 항목을 줄여 주세요."
        )),
        PivotError::TimedOut => EasyOfficeError::ResourceBudgetExceeded(
            "피벗 계산이 제한 시간을 넘었습니다. 범위를 줄여 주세요.".to_string(),
        ),
    }
}

fn is_dynamic_request(entries: &[(String, StrictJsonValue)]) -> bool {
    [
        "rowFields",
        "colFields",
        "filterFields",
        "valueFields",
        "filters",
    ]
    .iter()
    .any(|k| {
        StrictJsonValue::get_field(entries, k).is_some_and(|v| !matches!(v, StrictJsonValue::Null))
    })
}

fn parse_field_list(
    entries: &[(String, StrictJsonValue)],
    key: &str,
    width: usize,
) -> Result<Vec<usize>, EasyOfficeError> {
    let Some(val) =
        StrictJsonValue::get_field(entries, key).filter(|v| !matches!(v, StrictJsonValue::Null))
    else {
        return Ok(Vec::new());
    };
    let arr = val
        .as_array()
        .ok_or_else(|| invalid(&format!("{key}는 배열이어야 합니다.")))?;
    if arr.len() > MAX_PIVOT_FIELDS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "피벗 '{key}' 개수가 허용 상한({MAX_PIVOT_FIELDS})을 초과했습니다."
        )));
    }
    let mut out = Vec::with_capacity(arr.len());
    for v in arr {
        let col = v
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| invalid("피벗 열 번호는 0 이상의 정수여야 합니다."))?;
        if col >= width {
            return Err(invalid("피벗 열이 선택한 범위를 벗어났습니다."));
        }
        out.push(col);
    }
    Ok(out)
}

fn parse_dynamic_values(
    entries: &[(String, StrictJsonValue)],
    width: usize,
) -> Result<Vec<(usize, PivotAggregation)>, EasyOfficeError> {
    let Some(val) = StrictJsonValue::get_field(entries, "valueFields")
        .filter(|v| !matches!(v, StrictJsonValue::Null))
    else {
        let value = offset(entries, "value")?.ok_or_else(|| invalid("피벗 값 열이 필요합니다."))?;
        if value >= width {
            return Err(invalid("피벗 열이 선택한 범위를 벗어났습니다."));
        }
        let agg =
            aggregation(EasyOfficeIpcRouter::optional_json_str(entries, "agg")?.unwrap_or("sum"))?;
        return Ok(vec![(value, agg)]);
    };
    let arr = val
        .as_array()
        .ok_or_else(|| invalid("valueFields는 배열이어야 합니다."))?;
    if arr.len() > MAX_PIVOT_FIELDS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "피벗 값 필드 수가 허용 상한({MAX_PIVOT_FIELDS})을 초과했습니다."
        )));
    }
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let obj = item
            .as_object()
            .ok_or_else(|| invalid("valueFields 항목은 객체여야 합니다."))?;
        let col = StrictJsonValue::get_field(obj, "col")
            .and_then(StrictJsonValue::as_u64)
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| invalid("피벗 값 열 번호가 올바르지 않습니다."))?;
        if col >= width {
            return Err(invalid("피벗 열이 선택한 범위를 벗어났습니다."));
        }
        let agg_name = StrictJsonValue::get_field(obj, "agg")
            .and_then(StrictJsonValue::as_str)
            .unwrap_or("sum");
        let agg = aggregation(agg_name)?;
        out.push((col, agg));
    }
    Ok(out)
}

fn parse_dynamic_filters(
    entries: &[(String, StrictJsonValue)],
    width: usize,
) -> Result<Vec<PivotFieldFilter>, EasyOfficeError> {
    let Some(val) = StrictJsonValue::get_field(entries, "filters")
        .filter(|v| !matches!(v, StrictJsonValue::Null))
    else {
        return Ok(Vec::new());
    };
    let arr = val
        .as_array()
        .ok_or_else(|| invalid("filters는 배열이어야 합니다."))?;
    if arr.len() > 64 {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "피벗 필터 조건 수가 허용 상한(64)을 초과했습니다.".to_string(),
        ));
    }
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let obj = item
            .as_object()
            .ok_or_else(|| invalid("filters 항목은 객체여야 합니다."))?;
        let col_offset = StrictJsonValue::get_field(obj, "col")
            .or_else(|| StrictJsonValue::get_field(obj, "colOffset"))
            .and_then(StrictJsonValue::as_u64)
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| invalid("피벗 필터 열 번호가 올바르지 않습니다."))?;
        if col_offset >= width {
            return Err(invalid("피벗 필터 열이 선택한 범위를 벗어났습니다."));
        }
        let vals_arr = match StrictJsonValue::get_field(obj, "values")
            .or_else(|| StrictJsonValue::get_field(obj, "allowedValues"))
        {
            None | Some(StrictJsonValue::Null) => &[][..],
            Some(v) => v
                .as_array()
                .ok_or_else(|| invalid("피벗 필터 values는 배열이어야 합니다."))?,
        };
        if vals_arr.len() > MAX_PIVOT_FILTER_VALUES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "피벗 필터 항목 수가 허용 상한({MAX_PIVOT_FILTER_VALUES})을 초과했습니다."
            )));
        }
        let mut allowed_values = Vec::with_capacity(vals_arr.len());
        for v in vals_arr {
            let s = v
                .as_str()
                .ok_or_else(|| invalid("피벗 필터 값은 문자열이어야 합니다."))?;
            if s.len() > MAX_PIVOT_TEXT_BYTES {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "피벗 필터 값 길이가 허용 상한({MAX_PIVOT_TEXT_BYTES}B)을 초과했습니다."
                )));
            }
            allowed_values.push(s.to_string());
        }
        let allow_blank = match StrictJsonValue::get_field(obj, "blank")
            .or_else(|| StrictJsonValue::get_field(obj, "allowBlank"))
        {
            None | Some(StrictJsonValue::Null) => false,
            Some(StrictJsonValue::Bool(b)) => *b,
            Some(_) => return Err(invalid("피벗 필터 blank는 불리언이어야 합니다.")),
        };
        out.push(PivotFieldFilter {
            col_offset,
            allowed_values,
            allow_blank,
        });
    }
    Ok(out)
}

fn write_field_items_map(out: &mut String, map: &BTreeMap<usize, Vec<String>>) {
    out.push('{');
    for (i, (col, items)) in map.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "\"{col}\":");
        strings(out, items);
    }
    out.push('}');
}

fn write_field_blanks_map(out: &mut String, map: &BTreeMap<usize, bool>) {
    out.push('{');
    for (i, (col, &has_blank)) in map.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "\"{col}\":{has_blank}");
    }
    out.push('}');
}

fn format_dynamic_pivot_json(summary: &DynamicPivotSummary) -> String {
    let row_keys: Vec<String> = summary
        .row_tuples
        .iter()
        .map(|t| {
            if t.is_empty() {
                "Total".to_string()
            } else {
                t.join(" / ")
            }
        })
        .collect();
    let col_keys: Vec<String> = if summary.col_tuples.is_empty() {
        vec!["Total".to_string()]
    } else {
        summary
            .col_tuples
            .iter()
            .map(|t| {
                if t.is_empty() {
                    "Total".to_string()
                } else {
                    t.join(" / ")
                }
            })
            .collect()
    };
    let compat_row_totals: Vec<f64> = summary
        .row_totals
        .iter()
        .map(|rt| rt.first().copied().unwrap_or(0.0))
        .collect();
    let compat_col_totals: Vec<f64> = if summary.col_totals.is_empty() {
        vec![0.0]
    } else {
        summary
            .col_totals
            .iter()
            .map(|ct| ct.first().copied().unwrap_or(0.0))
            .collect()
    };
    let compat_grand = summary.grand_totals.first().copied().unwrap_or(0.0);

    let mut out = String::with_capacity(512);
    out.push_str("{\"rowKeys\":");
    strings(&mut out, &row_keys);
    out.push_str(",\"columnKeys\":");
    strings(&mut out, &col_keys);
    out.push_str(",\"matrix\":[");
    for (r, row) in summary.values_matrix.iter().enumerate() {
        if r > 0 {
            out.push(',');
        }
        let compat_row: Vec<f64> = row
            .iter()
            .map(|c| c.first().copied().unwrap_or(0.0))
            .collect();
        numbers(&mut out, &compat_row);
    }
    out.push_str("],\"rowTotals\":");
    numbers(&mut out, &compat_row_totals);
    out.push_str(",\"columnTotals\":");
    numbers(&mut out, &compat_col_totals);
    out.push_str(",\"grandTotal\":");
    number(&mut out, compat_grand);

    out.push_str(",\"rowTuples\":[");
    for (i, t) in summary.row_tuples.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        strings(&mut out, t);
    }
    out.push_str("],\"colTuples\":[");
    for (i, t) in summary.col_tuples.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        strings(&mut out, t);
    }
    out.push_str("],\"valuesMatrix\":[");
    for (r, row) in summary.values_matrix.iter().enumerate() {
        if r > 0 {
            out.push(',');
        }
        out.push('[');
        for (c, cell) in row.iter().enumerate() {
            if c > 0 {
                out.push(',');
            }
            numbers(&mut out, cell);
        }
        out.push(']');
    }
    out.push_str("],\"valueRowTotals\":[");
    for (r, rt) in summary.row_totals.iter().enumerate() {
        if r > 0 {
            out.push(',');
        }
        numbers(&mut out, rt);
    }
    out.push_str("],\"valueColumnTotals\":[");
    for (c, ct) in summary.col_totals.iter().enumerate() {
        if c > 0 {
            out.push(',');
        }
        numbers(&mut out, ct);
    }
    out.push_str("],\"valueGrandTotals\":");
    numbers(&mut out, &summary.grand_totals);
    out.push_str(",\"fieldItems\":");
    write_field_items_map(&mut out, &summary.field_items);
    out.push_str(",\"availableFieldItems\":");
    write_field_items_map(&mut out, &summary.available_field_items);
    out.push_str(",\"fieldBlanks\":");
    write_field_blanks_map(&mut out, &summary.field_blanks);
    out.push_str(",\"availableFieldBlanks\":");
    write_field_blanks_map(&mut out, &summary.available_field_blanks);
    out.push('}');
    out
}

impl EasyOfficeIpcRouter {
    /// `{sheet, sheets?, cells, range:"A1:D90", header?, row, col?, value, agg}` ->
    /// `{rowKeys, columnKeys, matrix, rowTotals, columnTotals, grandTotal}`; `row`, `col` and
    /// `value` are column offsets inside `range`. When multi-dimensional fields (`rowFields`,
    /// `colFields`, `filterFields`, `valueFields`, `filters`) are provided, runs
    /// `compute_dynamic_pivot_within` and includes extended fields alongside compatible ones.
    pub(crate) fn sheet_pivot(
        entries: &[(String, StrictJsonValue)],
    ) -> Result<String, EasyOfficeError> {
        let default_sheet = Self::resolve_default_sheet_from_entries(entries)?;
        let wb = Self::build_workbook_from_json_entries(entries, &default_sheet)?;
        let range = CellRange::parse(Self::optional_json_str(entries, "range")?.unwrap_or(""))?;
        let (_, width) = range.dimensions();
        let header = !matches!(
            StrictJsonValue::get_field(entries, "header"),
            Some(StrictJsonValue::Bool(false))
        );
        if is_dynamic_request(entries) {
            let spec = DynamicPivotSpec {
                range,
                header,
                row_fields: parse_field_list(entries, "rowFields", width)?,
                col_fields: parse_field_list(entries, "colFields", width)?,
                filter_fields: parse_field_list(entries, "filterFields", width)?,
                value_fields: parse_dynamic_values(entries, width)?,
                filters: parse_dynamic_filters(entries, width)?,
            };
            let summary = wb
                .compute_dynamic_pivot_within(
                    &default_sheet,
                    &spec,
                    MAX_PIVOT_CELLS,
                    eval_limit::eval_time(),
                )
                .map_err(map_pivot_err)?;
            return Ok(format_dynamic_pivot_json(&summary));
        }
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
        let agg = aggregation(Self::optional_json_str(entries, "agg")?.unwrap_or("sum"))?;
        let spec = PivotSpec {
            range,
            header,
            row,
            col,
            value,
            agg,
        };
        let pivot = wb
            .compute_pivot_within(
                &default_sheet,
                &spec,
                MAX_PIVOT_CELLS,
                eval_limit::eval_time(),
            )
            .map_err(map_pivot_err)?;
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

    fn request(sheets: &str, cells: &[(&str, &str, String)], extra: &str) -> String {
        let cells = cells
            .iter()
            .map(|(sheet, cell, value)| {
                format!("{{\"sheet\":\"{sheet}\",\"cell\":\"{cell}\",\"value\":\"{value}\"}}")
            })
            .collect::<Vec<_>>()
            .join(",");
        format!("{{\"sessionAction\":\"pivot\",\"sheet\":\"S\",\"sheets\":{sheets},\"cells\":[{cells}],{extra}}}")
    }

    #[test]
    fn values_come_from_the_whole_workbook() {
        let cells = [
            ("S", "A1", "부서".to_string()),
            ("S", "C1", "매출".to_string()),
            ("S", "A2", "영업".to_string()),
            ("S", "C2", "=T!A1*2".to_string()),
            ("S", "A3", "개발".to_string()),
            ("S", "C3", "=Rate+SUM(T!A1:A2)".to_string()),
            ("S", "A4", "배열".to_string()),
            ("S", "C4", "=SUM(SEQUENCE(3))".to_string()),
            ("T", "A1", "5".to_string()),
            ("T", "A2", "7".to_string()),
        ];
        let body = request(
            "[\"S\",\"T\"]",
            &cells,
            "\"range\":\"A1:C4\",\"row\":0,\"value\":2,\"agg\":\"sum\",\"names\":[{\"name\":\"Rate\",\"refersTo\":\"100\"}]",
        );
        let json = EasyOfficeIpcRouter::dispatch("evaluate_sheet", &body).unwrap();
        assert!(
            json.contains("\"rowKeys\":[\"영업\",\"개발\",\"배열\"]"),
            "{json}"
        );
        assert!(json.contains("\"matrix\":[[10],[112],[6]]"), "{json}");
    }

    #[test]
    fn an_answer_over_the_cell_limit_is_refused_while_the_groups_are_collected() {
        let mut cells = Vec::new();
        for n in 0..600u32 {
            cells.push(("S", format!("A{}", n + 1), format!("r{n}")));
            cells.push(("S", format!("B{}", n + 1), format!("c{n}")));
            cells.push(("S", format!("C{}", n + 1), "1".to_string()));
        }
        let cells: Vec<(&str, &str, String)> = cells
            .iter()
            .map(|(s, c, v)| (*s, c.as_str(), v.clone()))
            .collect();
        let body = request(
            "[\"S\"]",
            &cells,
            "\"range\":\"A1:C600\",\"header\":false,\"row\":0,\"col\":1,\"value\":2",
        );
        let err = EasyOfficeIpcRouter::dispatch("evaluate_sheet", &body).unwrap_err();
        assert!(
            matches!(err, EasyOfficeError::ResourceBudgetExceeded(_)),
            "{err:?}"
        );
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

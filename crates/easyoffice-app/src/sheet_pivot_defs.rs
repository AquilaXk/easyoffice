//! The pivot tables `GETPIVOTDATA` finds (PD-292): the `pivots` of a request, read by
//! `sheet_pivot_json::parse_pivot` (its bounds apply), as the definitions the engine registers.

use crate::sheet_pivot_json::parse_pivot;
use easyoffice_formats::{
    parse_sheet_and_box, CellBox, EasyOfficeError, PivotAggKind, SheetPivotTable, StrictJsonValue,
    MAX_PIVOTS_PER_SHEET,
};
use easyoffice_sheet::{
    CellRange, PackedCellCoordinate, PivotAggregation, PivotFieldFilter, PivotTableDef,
    PivotValueDef, MAX_REGISTERED_PIVOTS, MAX_REGISTERED_PIVOT_BYTES,
};

fn bad() -> EasyOfficeError {
    EasyOfficeError::InvalidArgument("피벗 JSON이 올바르지 않습니다: pivots".to_string())
}

fn range_of(b: CellBox) -> Option<CellRange> {
    let corner = |row, col| PackedCellCoordinate::new(row, col).ok();
    CellRange::new(corner(b.row0, b.col0)?, corner(b.row1, b.col1)?).ok()
}

const fn aggregation(kind: PivotAggKind) -> PivotAggregation {
    match kind {
        PivotAggKind::Sum => PivotAggregation::Sum,
        PivotAggKind::Average => PivotAggregation::Average,
        PivotAggKind::Count => PivotAggregation::Count,
        PivotAggKind::Min => PivotAggregation::Min,
        PivotAggKind::Max => PivotAggregation::Max,
    }
}

/// The definition of `pivot` on `sheet`. None when the table cannot answer: a source that is a table the range of
/// which is not known (a file that dropped the cache) leaves `GETPIVOTDATA` at `#REF!`, as a reference outside any
/// pivot table does.
fn pivot_def(sheet: &str, pivot: &SheetPivotTable) -> Option<PivotTableDef> {
    let held_in = if pivot.source_sheet.is_empty() {
        sheet
    } else {
        &pivot.source_sheet
    };
    let (source_sheet, source) = parse_sheet_and_box(held_in, &pivot.source_range)?;
    let columns = |fields: &[u32]| fields.iter().map(|&c| c as usize).collect();
    let def = PivotTableDef {
        location: range_of(pivot.location)?,
        source_sheet,
        source: range_of(source)?,
        has_header: pivot.has_header,
        row_fields: columns(&pivot.row_fields),
        col_fields: columns(&pivot.col_fields),
        values: pivot
            .value_fields
            .iter()
            .map(|v| PivotValueDef {
                column: v.col as usize,
                aggregation: aggregation(v.agg),
                caption: v.name.clone(),
            })
            .collect(),
        filters: pivot
            .filters
            .iter()
            .map(|f| PivotFieldFilter {
                col_offset: f.col as usize,
                allowed_values: f.values.clone(),
                allow_blank: f.blank,
            })
            .collect(),
    };
    // Each table gets its share of what a workbook may hold, so the tables of one request never outgrow it.
    (def.heap_bytes() <= MAX_REGISTERED_PIVOT_BYTES / MAX_REGISTERED_PIVOTS).then_some(def)
}

/// The pivot tables of `sheet` from a `pivots` array (null: none). A table whose own content is over a bound (a
/// text, a list, its bytes) is left out and answers `#REF!`, so it cannot stop the other formulas of the workbook;
/// the number of tables of a sheet is a bound of the request and refuses all of it.
pub(crate) fn pivot_defs(
    sheet: &str,
    value: &StrictJsonValue,
) -> Result<Vec<PivotTableDef>, EasyOfficeError> {
    if matches!(value, StrictJsonValue::Null) {
        return Ok(Vec::new());
    }
    let items = value.as_array().ok_or_else(bad)?;
    if items.len() > MAX_PIVOTS_PER_SHEET {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "시트 피벗 테이블 수({})가 허용 상한({MAX_PIVOTS_PER_SHEET})을 초과했습니다.",
            items.len()
        )));
    }
    let mut defs = Vec::with_capacity(items.len());
    for (idx, item) in items.iter().enumerate() {
        match parse_pivot(idx, item) {
            Ok(pivot) => defs.extend(pivot_def(sheet, &pivot)),
            Err(EasyOfficeError::ResourceBudgetExceeded(_)) => {}
            Err(err) => return Err(err),
        }
    }
    Ok(defs)
}

/// The `pivots` field of a request without a session: `{"Sheet1": [pivot, ...], ...}`; nothing when absent. The
/// pivot tables of all the sheets together stay within what a workbook may register, checked before any is read.
pub(crate) fn pivots_by_sheet(
    entries: &[(String, StrictJsonValue)],
) -> Result<Vec<(String, Vec<PivotTableDef>)>, EasyOfficeError> {
    let Some(field) = StrictJsonValue::get_field(entries, "pivots")
        .filter(|v| !matches!(v, StrictJsonValue::Null))
    else {
        return Ok(Vec::new());
    };
    let sheets = field.as_object().ok_or_else(bad)?;
    let mut total = 0usize;
    let mut out = Vec::new();
    for (name, list) in sheets {
        let count = match list {
            StrictJsonValue::Null => 0,
            other => other.as_array().ok_or_else(bad)?.len(),
        };
        total = total.saturating_add(count);
        if total > MAX_REGISTERED_PIVOTS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "통합 문서 피벗 테이블 수가 허용 상한({MAX_REGISTERED_PIVOTS})을 초과했습니다."
            )));
        }
        if count > 0 {
            out.push((name.clone(), pivot_defs(name, list)?));
        }
    }
    Ok(out)
}

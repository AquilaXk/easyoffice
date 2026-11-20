//! `GETPIVOTDATA(data_field, pivot_table, [field1, item1, ...])`: a value of a pivot table.
//!
//! 참조 구현: none (a reference engine lists the name and has no body). This follows the function
//! documentation: a cell of the pivot table, the value field named by its caption or its source
//! header, and the pairs of a field and an item that pick the cell; the answer is `#REF!` when the
//! reference is not in a pivot table, when a field is not one of the table's row or column fields
//! (a hidden field) or when no data matches.
//!
//! The answer is worked out from the pivot table's definition and its source rows, with the
//! pivot engine's own rules (`pivot.rs`: the aggregations, the error values that spoil an
//! aggregation, the case-folded keys of `normalize_pivot_key`; `pivot_dynamic.rs`: the filters of
//! a table), in one pass over the source rows read through the evaluation context, so a source
//! that depends on the formula's own cell is a cycle like any other. The table is not read
//! back from the cells it was laid out in, which a user may have edited, and the pivot engine is
//! not run again for each call. The definitions come from [`WorkbookEngine::set_pivot_tables`]; a
//! workbook that registers none answers `#REF!`, as a reference outside any pivot table does.

use crate::pivot::{normalize_pivot_key, pivot_error_of, pivot_input, PivotAccumulator};
use crate::pivot_dynamic::{is_cell_blank, normalize_filter_text};
use crate::{
    CellRange, EvalContext, EvaluatedCellValue as V, Expr, FormulaError, PackedCellCoordinate,
    PivotAggregation, PivotFieldFilter, SheetEngine, WorkbookEngine,
};
use easyoffice_formats::EasyOfficeError;
use std::collections::{HashMap, HashSet};

/// Pivot tables one workbook may register.
pub const MAX_REGISTERED_PIVOTS: usize = 128;
/// Field and item pairs of one call (a formula takes 255 arguments).
const MAX_PIVOT_PAIRS: usize = 126;

/// A value field of a pivot table: its source column (counted from the first column of the
/// source), what it adds up and the caption the table shows (`합계: 매출`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PivotValueDef {
    pub column: usize,
    pub aggregation: PivotAggregation,
    pub caption: String,
}

/// What `GETPIVOTDATA` needs to know of a pivot table: where it is, where its rows come from and
/// how they are grouped and filtered. Columns count from the first column of `source`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PivotTableDef {
    pub location: CellRange,
    pub source_sheet: String,
    pub source: CellRange,
    pub has_header: bool,
    pub row_fields: Vec<usize>,
    pub col_fields: Vec<usize>,
    pub values: Vec<PivotValueDef>,
    pub filters: Vec<PivotFieldFilter>,
}

impl WorkbookEngine {
    /// Replaces the pivot tables `GETPIVOTDATA` finds on `sheet`. Fails, changing nothing, when the
    /// sheet is unknown or the workbook would hold more than [`MAX_REGISTERED_PIVOTS`]. Set them
    /// before the first calculation: a result already calculated is not worked out again.
    pub fn set_pivot_tables(
        &mut self,
        sheet: &str,
        tables: Vec<PivotTableDef>,
    ) -> Result<(), EasyOfficeError> {
        let Some(name) = self
            .sheet_names()
            .into_iter()
            .find(|n| WorkbookEngine::is_same_sheet_identity(sheet, n))
        else {
            return Err(EasyOfficeError::InvalidArgument(format!(
                "피벗 테이블을 둘 시트가 없습니다: {sheet}"
            )));
        };
        let name = name.to_string();
        let mut all: Vec<(String, PivotTableDef)> = self
            .pivot_tables
            .iter()
            .filter(|(n, _)| !WorkbookEngine::is_same_sheet_identity(n, &name))
            .cloned()
            .collect();
        all.extend(tables.into_iter().map(|t| (name.clone(), t)));
        if all.len() > MAX_REGISTERED_PIVOTS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "피벗 테이블은 통합 문서에 {MAX_REGISTERED_PIVOTS}개까지 둘 수 있습니다."
            )));
        }
        self.pivot_tables = std::sync::Arc::new(all);
        Ok(())
    }

    /// The pivot table of `sheet` that covers `cell`.
    fn pivot_table_at(&self, sheet: &str, cell: PackedCellCoordinate) -> Option<&PivotTableDef> {
        self.pivot_tables
            .iter()
            .find(|(name, def)| {
                WorkbookEngine::is_same_sheet_identity(sheet, name) && def.location.contains(cell)
            })
            .map(|(_, def)| def)
    }
}

/// The rows of a source that pass a table's filters: per column the allowed values (case-folded)
/// and whether a blank passes.
struct FilterSet {
    by_column: HashMap<usize, (HashSet<String>, bool)>,
}

impl FilterSet {
    fn new(filters: &[PivotFieldFilter]) -> Self {
        let mut by_column: HashMap<usize, (HashSet<String>, bool)> = HashMap::new();
        for f in filters {
            let allowed: HashSet<String> = f
                .allowed_values
                .iter()
                .filter(|v| !v.is_empty())
                .map(|v| normalize_filter_text(v))
                .collect();
            by_column
                .entry(f.col_offset)
                .and_modify(|(existing, blank)| {
                    existing.retain(|k| allowed.contains(k));
                    *blank &= f.allow_blank;
                })
                .or_insert((allowed, f.allow_blank));
        }
        Self { by_column }
    }

    fn passes(&self, cell: impl Fn(usize) -> V) -> bool {
        self.by_column.iter().all(|(&column, (allowed, blank))| {
            let value = cell(column);
            if is_cell_blank(&value) {
                *blank
            } else {
                allowed.contains(&normalize_pivot_key(value).1)
            }
        })
    }
}

impl SheetEngine {
    fn pivot_text_arg(
        &self,
        arg: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<String, FormulaError> {
        match self.eval_arg_val(arg, ctx, depth)? {
            V::Text(text) => Ok(text),
            other => Ok(other.to_display_string()),
        }
    }

    /// `GETPIVOTDATA(data_field, pivot_table, [field1, item1], ...)`
    pub(crate) fn fn_getpivotdata(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<V, FormulaError> {
        if args.len() < 2 || !args.len().is_multiple_of(2) || args.len() - 2 > 2 * MAX_PIVOT_PAIRS {
            return Err(FormulaError::Value);
        }
        let data_field = self.pivot_text_arg(&args[0], ctx, depth)?;
        let (sheet, range, _) = self
            .extract_range_and_sheet_ctx(&args[1], ctx, depth)?
            .ok_or(FormulaError::Ref)?;
        let workbook = ctx.workbook.ok_or(FormulaError::Ref)?;
        let table = workbook
            .pivot_table_at(sheet.as_deref().unwrap_or(&self.name), range.start)
            .ok_or(FormulaError::Ref)?;
        let source_expr = Expr::sheet_range(table.source_sheet.as_str(), table.source);
        let source = match self.read_ref_grid(&source_expr, ctx, depth)? {
            Some(grid) => grid,
            None => return Err(FormulaError::Ref),
        };
        if let [V::Error(e)] = source.data.as_slice() {
            return Err(*e);
        }
        let header_name = |column: usize| {
            normalize_pivot_key(source.get(0, column).clone())
                .0
                .to_lowercase()
        };
        let named = |text: &str| -> Option<usize> {
            let wanted = text.trim().to_lowercase();
            table
                .has_header
                .then(|| (0..source.cols).find(|&column| header_name(column) == wanted))?
        };
        // the value field: its caption in the table, or the header of its source column
        let wanted = data_field.trim().to_lowercase();
        let value = table
            .values
            .iter()
            .find(|v| v.caption.to_lowercase() == wanted)
            .or_else(|| {
                table.has_header.then(|| {
                    table
                        .values
                        .iter()
                        .find(|v| v.column < source.cols && header_name(v.column) == wanted)
                })?
            })
            .ok_or(FormulaError::Ref)?;
        if value.column >= source.cols {
            return Err(FormulaError::Ref);
        }
        // the fields that pick the cell have to be shown on an axis of the table
        let mut picks: Vec<(usize, String)> = Vec::new();
        for pair in args[2..].chunks(2) {
            let field = self.pivot_text_arg(&pair[0], ctx, depth)?;
            let column = named(&field).ok_or(FormulaError::Ref)?;
            if !table.row_fields.contains(&column) && !table.col_fields.contains(&column) {
                return Err(FormulaError::Ref);
            }
            let item = self.eval_arg_val(&pair[1], ctx, depth)?;
            let key = if is_cell_blank(&item) {
                String::new()
            } else {
                normalize_pivot_key(item).1
            };
            picks.push((column, key));
        }
        let filters = FilterSet::new(&table.filters);
        let mut sum = PivotAccumulator::default();
        let mut matched = false;
        for row in usize::from(table.has_header)..source.rows {
            let cell = |column: usize| source.get(row, column).clone();
            if !filters.passes(cell) {
                continue;
            }
            let picked = picks.iter().all(|(column, key)| {
                let found = cell(*column);
                if is_cell_blank(&found) {
                    key.is_empty()
                } else {
                    normalize_pivot_key(found).1 == *key
                }
            });
            if !picked {
                continue;
            }
            matched = true;
            if let Some(input) = pivot_input(source.get(row, value.column), value.aggregation) {
                sum.add_input(input);
            }
        }
        if !matched {
            return Err(FormulaError::Ref);
        }
        let result = sum.result(value.aggregation);
        match pivot_error_of(result) {
            Some(e) => Err(e),
            None => Ok(V::Number(result)),
        }
    }
}

use crate::names::NameTable;
use crate::{
    CellRange, EvalContext, EvaluatedCellValue, FormulaError, PackedCellCoordinate, SheetEngine,
};
use easyoffice_formats::{DefinedName, EasyOfficeError, MAX_NAMES};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Multi-sheet workbook engine supporting cross-sheet references (`=Sheet2!A1`, `=SUM(Sheet2!A1:A3)`)
/// and cross-sheet cycle detection.
#[derive(Debug, Default)]
pub struct WorkbookEngine {
    sheets: Vec<SheetEngine>,
    names: Arc<NameTable>,
    /// The tables, which structured references are resolved against (PD-225).
    pub(crate) tables: Arc<crate::table_index::TableIndex>,
    /// Nodes the formulas that use names hold, over all sheets (see `names::MAX_WORKBOOK_NAME_NODES`).
    name_budget: Arc<AtomicUsize>,
}

impl Clone for WorkbookEngine {
    /// The copy counts its own formulas: its sheets get a counter of their own.
    fn clone(&self) -> Self {
        let mut sheets = self.sheets.clone();
        let budget = Arc::new(AtomicUsize::new(self.name_budget.load(Ordering::Relaxed)));
        for sheet in &mut sheets {
            sheet.name_budget = Arc::clone(&budget);
        }
        Self {
            sheets,
            names: Arc::clone(&self.names),
            tables: Arc::clone(&self.tables),
            name_budget: budget,
        }
    }
}

/// Maximum sheets one workbook holds (matches XLSX format cap and keeps sheet indices within `u16`).
pub const MAX_WORKBOOK_SHEETS: usize = 1_024;

/// Cells of a range a name's value shows in the 이름 관리 list.
pub const NAME_VALUE_CELLS: usize = 24;

/// A change of the defined names worked out but not applied: the new table and the formulas
/// (sheet index, cell) that use a name whose meaning changes and so compile again.
pub(crate) struct NamesPlan {
    pub(crate) table: Arc<NameTable>,
    pub(crate) affected: Vec<(usize, PackedCellCoordinate)>,
    /// The problems of the definitions, by position in the list given.
    pub(crate) issues: Vec<(usize, crate::NameIssue)>,
}

impl WorkbookEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// The defined names, in the order they were set (PD-214).
    pub fn names(&self) -> &[DefinedName] {
        self.names.definitions()
    }

    /// Estimated heap bytes the defined names cost: their definitions, the trees they expand to and the nodes the
    /// formulas that use them hold (64 bytes a node, 24 per byte of text).
    pub(crate) fn name_bytes(&self) -> usize {
        const NODE_BYTES: usize = 64;
        let nodes = self
            .names
            .physical_nodes()
            .saturating_add(self.name_budget.load(Ordering::Relaxed));
        nodes
            .saturating_mul(NODE_BYTES)
            .saturating_add(self.names.text_bytes().saturating_mul(24))
    }

    /// Nodes the formulas that use names hold, over all sheets, and nodes the expanded definitions hold.
    pub fn name_nodes(&self) -> (usize, usize) {
        (
            self.name_budget.load(Ordering::Relaxed),
            self.names.physical_nodes(),
        )
    }

    /// Replaces the defined names. Fails, changing nothing, when one is not valid: a name that breaks
    /// the syntax, is a function or sheet name, repeats another name of the same scope, a scope that
    /// is no sheet, a definition that does not read as a formula or leads back to itself, too many
    /// names or too deep a chain. The formulas that use a changed name are compiled again.
    pub fn set_names(&mut self, defs: &[DefinedName]) -> Result<(), EasyOfficeError> {
        let plan = self.plan_names(defs, true)?;
        self.commit_names(plan);
        Ok(())
    }

    /// [`set_names`](Self::set_names) for names read from a file: a definition that is not valid is
    /// left out (a bad name, a repeated one, an unknown scope) or kept reading as an error, and names
    /// past [`MAX_NAMES`] are dropped. Returns how many were dropped.
    pub fn set_names_lenient(&mut self, defs: &[DefinedName]) -> usize {
        let kept = defs.len().min(MAX_NAMES);
        let plan = self
            .plan_names(&defs[..kept], false)
            .expect("a lenient plan does not fail");
        self.commit_names(plan);
        defs.len() - kept
    }

    /// The first problem `candidate` has as a new or changed definition (`replacing` is the name and
    /// scope of the definition it replaces, if any), judged with the names the workbook has.
    pub fn check_name(
        &self,
        candidate: &DefinedName,
        replacing: Option<(&str, Option<&str>)>,
    ) -> Option<crate::NameIssue> {
        let same = |def: &DefinedName, (name, scope): (&str, Option<&str>)| {
            easyoffice_formats::fold_name(&def.name) == easyoffice_formats::fold_name(name)
                && def.scope.as_deref().map(str::to_lowercase) == scope.map(str::to_lowercase)
        };
        let mut defs: Vec<DefinedName> = self
            .names()
            .iter()
            .filter(|d| !replacing.is_some_and(|r| same(d, r)))
            .cloned()
            .collect();
        defs.push(candidate.clone());
        let at = defs.len() - 1;
        let sheet_names: Vec<&str> = self.sheets.iter().map(|s| s.name.as_str()).collect();
        let built = NameTable::build(&defs, &sheet_names, true, &self.tables);
        built
            .issues
            .iter()
            .find(|&&(i, _)| i == at)
            .map(|&(_, issue)| issue)
    }

    /// [`check_name`](Self::check_name) for many new definitions at once, judged together with the names the
    /// workbook has: a name that repeats an earlier candidate is a duplicate. One table is built for all.
    pub fn check_names(&self, candidates: &[DefinedName]) -> Vec<Option<crate::NameIssue>> {
        let mut defs: Vec<DefinedName> = self.names().to_vec();
        let first = defs.len();
        defs.extend_from_slice(candidates);
        let sheet_names: Vec<&str> = self.sheets.iter().map(|s| s.name.as_str()).collect();
        let issues = NameTable::build(&defs, &sheet_names, true, &self.tables).issues;
        (first..defs.len())
            .map(|i| {
                issues
                    .iter()
                    .find(|&&(j, _)| j == i)
                    .map(|&(_, issue)| issue)
            })
            .collect()
    }

    /// The value of a name as the 이름 관리 list shows it: a cell's value, a range as `{a,b;c,d}` (rows end with `;`, at
    /// most [`NAME_VALUE_CELLS`] cells, `…` for the rest), any other formula its result on the sheet the name
    /// belongs to (the first sheet for a workbook name).
    pub fn name_value_text(&self, def: &DefinedName) -> String {
        let Some(sheet) = def
            .scope
            .as_deref()
            .or_else(|| self.sheets.first().map(|s| s.name.as_str()))
        else {
            return String::new();
        };
        let folded = easyoffice_formats::fold_name(&def.name);
        if let Some(Ok(tree)) = self.names.resolved_expr(&folded, sheet) {
            if let crate::Expr::SheetRange { sheet: at, range } = &tree.expr {
                return self.range_text(at, range);
            }
        }
        self.evaluate_formula_on_sheet(sheet, &format!("={}", def.name))
            .to_display_string()
    }

    fn range_text(&self, sheet: &str, range: &CellRange) -> String {
        let (r0, r1) = (
            range.start.row().min(range.end.row()),
            range.start.row().max(range.end.row()),
        );
        let (c0, c1) = (
            range.start.col().min(range.end.col()),
            range.start.col().max(range.end.col()),
        );
        let mut out = String::from("{");
        let mut shown = 0usize;
        'rows: for r in r0..=r1 {
            for c in c0..=c1 {
                if shown == NAME_VALUE_CELLS {
                    out.push('…');
                    break 'rows;
                }
                if c > c0 {
                    out.push(',');
                }
                let Ok(coord) = PackedCellCoordinate::new(r, c) else {
                    break 'rows;
                };
                match self.evaluate_cell(sheet, &coord.to_a1()) {
                    EvaluatedCellValue::Text(t) => {
                        out.push('"');
                        out.push_str(&t.replace('"', "\"\""));
                        out.push('"');
                    }
                    other => out.push_str(&other.to_display_string()),
                }
                shown += 1;
            }
            if r < r1 && shown < NAME_VALUE_CELLS {
                out.push(';');
            }
        }
        out.push('}');
        out
    }

    pub(crate) fn plan_names(
        &self,
        defs: &[DefinedName],
        strict: bool,
    ) -> Result<NamesPlan, EasyOfficeError> {
        self.plan_names_with(defs, strict, &self.tables)
    }

    /// [`plan_names`](Self::plan_names) for the structured references of the definitions resolved against
    /// `tables` (the tables a change is about to install).
    pub(crate) fn plan_names_with(
        &self,
        defs: &[DefinedName],
        strict: bool,
        tables: &crate::table_index::TableIndex,
    ) -> Result<NamesPlan, EasyOfficeError> {
        let sheet_names: Vec<&str> = self.sheets.iter().map(|s| s.name.as_str()).collect();
        if strict && defs.len() > MAX_NAMES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "이름은 최대 {MAX_NAMES}개까지 정의할 수 있습니다."
            )));
        }
        let built = NameTable::build(defs, &sheet_names, strict, tables);
        if let (true, Some(&(i, issue))) = (strict, built.issues.first()) {
            return Err(crate::names::issue_error(defs, i, issue));
        }
        let table = Arc::new(built.table);
        let changed = self.names.changed_keys(&table);
        let mut affected = Vec::new();
        if !changed.is_empty() {
            for (si, sheet) in self.sheets.iter().enumerate() {
                affected.extend(
                    sheet
                        .name_uses
                        .iter()
                        .filter(|(_, u)| u.keys.iter().any(|k| changed.contains(k)))
                        .map(|(&coord, _)| (si, coord)),
                );
            }
            affected.sort_unstable();
        }
        Ok(NamesPlan {
            table,
            affected,
            issues: built.issues,
        })
    }

    pub(crate) fn commit_names(&mut self, plan: NamesPlan) {
        self.install_names(plan.table);
        for &(si, coord) in &plan.affected {
            self.sheets[si].recompile_cell(coord);
        }
    }

    /// Makes `table` the workbook's names, in every sheet; the formulas that use a changed name are not
    /// compiled again here.
    pub(crate) fn install_names(&mut self, table: Arc<NameTable>) {
        self.names = table;
        for sheet in &mut self.sheets {
            sheet.names = Arc::clone(&self.names);
        }
    }

    pub(crate) fn names_use_tables(&self) -> bool {
        self.names.uses_tables()
    }

    pub fn unquote_formula_sheet_token(s: &str) -> Option<String> {
        if s.len() < 2 || !s.starts_with('\'') || !s.ends_with('\'') {
            return None;
        }
        let inner = &s[1..s.len() - 1];
        let bytes = inner.as_bytes();
        let mut out = String::with_capacity(inner.len());
        let mut i = 0usize;
        while i < bytes.len() {
            if let Some(rel) = bytes[i..].iter().position(|&b| b == b'\'') {
                if rel > 0 {
                    out.push_str(&inner[i..i + rel]);
                    i += rel;
                }
                if bytes.get(i + 1) == Some(&b'\'') {
                    out.push('\'');
                    i += 2;
                } else {
                    return None;
                }
            } else {
                out.push_str(&inner[i..]);
                break;
            }
        }
        Some(out)
    }

    pub fn canonical_sheet_key(raw: &str) -> String {
        if raw.trim().is_empty() {
            String::new()
        } else {
            raw.to_ascii_uppercase()
        }
    }

    /// Same identity as comparing canonical_sheet_keys, without allocating.
    pub fn is_same_sheet_identity(a: &str, b: &str) -> bool {
        !a.trim().is_empty() && a.eq_ignore_ascii_case(b)
    }

    pub(crate) fn find_sheet_index(&self, name: &str) -> Option<usize> {
        if name.trim().is_empty() {
            return None;
        }
        // A name that equals a non-blank one up to ASCII case is non-blank too, so the sheets'
        // own names need no trimming here.
        self.sheets
            .iter()
            .position(|s| s.name.eq_ignore_ascii_case(name))
    }

    pub fn try_add_sheet(
        &mut self,
        name: impl Into<String>,
    ) -> Result<&mut SheetEngine, EasyOfficeError> {
        let raw = name.into();
        let norm = if raw.trim().is_empty() {
            "Sheet1"
        } else {
            raw.as_str()
        };
        if let Some(pos) = self.find_sheet_index(norm) {
            return Ok(&mut self.sheets[pos]);
        }
        if self.sheets.len() >= MAX_WORKBOOK_SHEETS || u16::try_from(self.sheets.len()).is_err() {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "워크북은 최대 {MAX_WORKBOOK_SHEETS}개 시트까지만 지원합니다."
            )));
        }
        let mut engine = SheetEngine::new(norm);
        let start_id = if engine.sheet_id == 0 {
            1
        } else {
            engine.sheet_id
        };
        let mut candidate = start_id;
        while self.sheets.iter().any(|s| s.sheet_id == candidate) {
            candidate = candidate.checked_add(1).unwrap_or(1);
            if candidate == start_id {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "워크북 시트 식별자 공간이 가득 찼습니다.".to_string(),
                ));
            }
        }
        engine.sheet_id = candidate;
        engine.has_workbook_assigned_id = true;
        engine.names = Arc::clone(&self.names);
        engine.tables = Arc::clone(&self.tables);
        engine.name_budget = Arc::clone(&self.name_budget);
        self.sheets.push(engine);
        if !self.names.is_empty() {
            // A name may be scoped to the sheet that is new, or be the new sheet's name.
            let defs = self.names.definitions().to_vec();
            self.set_names_lenient(&defs);
        }
        Ok(self.sheets.last_mut().unwrap())
    }

    pub fn add_sheet(&mut self, name: impl Into<String>) -> &mut SheetEngine {
        self.try_add_sheet(name).unwrap_or_else(|e| panic!("{e}"))
    }

    pub fn sheet(&self, name: &str) -> Option<&SheetEngine> {
        self.find_sheet_index(name).map(|pos| &self.sheets[pos])
    }

    pub fn sheet_mut(&mut self, name: &str) -> Option<&mut SheetEngine> {
        self.find_sheet_index(name)
            .map(move |pos| &mut self.sheets[pos])
    }

    pub(crate) fn sheets(&self) -> &[SheetEngine] {
        &self.sheets
    }

    pub(crate) fn sheets_mut(&mut self) -> &mut [SheetEngine] {
        &mut self.sheets
    }

    pub fn sheet_count(&self) -> usize {
        self.sheets.len()
    }

    pub fn sheet_names(&self) -> Vec<&str> {
        self.sheets.iter().map(|s| s.name.as_str()).collect()
    }

    pub fn set_cell(
        &mut self,
        sheet_name: &str,
        a1: &str,
        raw_value: impl Into<String>,
    ) -> Result<(), EasyOfficeError> {
        let sheet = self.sheet_mut(sheet_name).ok_or_else(|| {
            EasyOfficeError::InvalidArgument(format!("시트를 찾을 수 없습니다: {sheet_name}"))
        })?;
        sheet.set_cell(a1, raw_value)
    }

    pub fn evaluate_cell(&self, sheet_name: &str, a1: &str) -> EvaluatedCellValue {
        let Some(sheet) = self.sheet(sheet_name) else {
            return EvaluatedCellValue::Error(FormulaError::Ref);
        };
        let Ok(coord) = PackedCellCoordinate::parse_a1(a1) else {
            return EvaluatedCellValue::Error(FormulaError::Ref);
        };
        let mut ctx = EvalContext::with_workbook(self, sheet.resolved_sheet_id());
        sheet.pre_evaluate_dag_iterative(std::slice::from_ref(&coord), &mut ctx);
        ctx.reset_steps();
        sheet.eval_coord_ctx(coord, &mut ctx, 0)
    }

    pub fn evaluate_formula_on_sheet(&self, sheet_name: &str, formula: &str) -> EvaluatedCellValue {
        self.evaluate_formula_on_sheet_at(sheet_name, None, formula)
    }

    /// The address of a `HYPERLINK` formula of a cell of `sheet_name` (see `SheetEngine::hyperlink_address_ctx`):
    /// `None` when the formula is not a `HYPERLINK` call or the sheet is not in the workbook.
    pub fn hyperlink_address_at(
        &self,
        sheet_name: &str,
        anchor: Option<PackedCellCoordinate>,
        formula: &str,
    ) -> Option<Result<String, FormulaError>> {
        let sheet = self.sheet(sheet_name)?;
        let mut ctx = EvalContext::with_workbook(self, sheet.resolved_sheet_id());
        ctx.current_coord = anchor;
        sheet.hyperlink_address_ctx(formula, &mut ctx)
    }

    pub fn evaluate_formula_on_sheet_at(
        &self,
        sheet_name: &str,
        anchor: Option<PackedCellCoordinate>,
        formula: &str,
    ) -> EvaluatedCellValue {
        let Some(sheet) = self.sheet(sheet_name) else {
            return EvaluatedCellValue::Error(FormulaError::Ref);
        };
        let mut ctx = EvalContext::with_workbook(self, sheet.resolved_sheet_id());
        ctx.current_coord = anchor;
        sheet.eval_raw_string_ctx(formula, &mut ctx, 0)
    }
}

/// One `evaluate_targets` request: a cell address or `=` formula evaluated on `sheet`,
/// with an optional anchor cell for `ROW()`/`COLUMN()` in formulas.
#[derive(Debug, Clone, Copy)]
pub struct WorkbookEvalTarget<'a> {
    pub sheet: &'a str,
    pub expr: &'a str,
    pub anchor: Option<PackedCellCoordinate>,
}

/// Values for each `WorkbookEvalTarget` plus a circular-reference flag (cycles display `#NUM!`).
#[derive(Debug, Clone, PartialEq)]
pub struct WorkbookBatchEvaluation {
    pub values: Vec<EvaluatedCellValue>,
    pub circular: bool,
    /// The deadline passed: the targets the pass had not reached have no value (see `deadline`).
    pub timed_out: bool,
    /// Per target, whether the pass did not reach it (empty when every target has its value).
    pub unreached: Vec<bool>,
    /// The arrays of the workbook that spill, with their values (PD-217); empty without array formulas.
    pub spills: Vec<crate::SpillArea>,
}

impl WorkbookBatchEvaluation {
    /// The pass did not reach target `i`: its entry in `values` is empty, not a result.
    pub fn is_unreached(&self, i: usize) -> bool {
        self.unreached.get(i).copied().unwrap_or(false)
    }
}

impl SheetEngine {
    pub fn formatted_cell(&self, a1: &str) -> String {
        let Ok(coord) = PackedCellCoordinate::parse_a1(a1) else {
            return "#REF!".to_string();
        };
        let val = self.evaluate_cell(a1);
        if let EvaluatedCellValue::Number(n) = val {
            if let Some(Ok(crate::Expr::FuncCall { name, .. })) = self.compiled_asts.get(&coord) {
                if matches!(
                    name.as_str(),
                    "DATE" | "EDATE" | "EOMONTH" | "WORKDAY" | "TODAY" | "DATEVALUE" | "오늘"
                ) && (1.0..=2_958_465.999_999).contains(&n)
                {
                    return crate::coercion::format_text_pattern(n, "yyyy-mm-dd");
                }
            }
        }
        val.to_display_string()
    }

    /// Secant / Newton-Raphson solver up to 100 iterations finding the value of `variable_cell`
    /// that makes `target_cell` evaluate to `target_value` within `1e-6`.
    pub fn goal_seek(
        &mut self,
        target_cell: &str,
        target_value: f64,
        variable_cell: &str,
    ) -> Result<f64, EasyOfficeError> {
        let _target_coord = PackedCellCoordinate::parse_a1(target_cell)?;
        let var_coord = PackedCellCoordinate::parse_a1(variable_cell)?;
        if !target_value.is_finite() {
            return Err(EasyOfficeError::InvalidArgument(
                "목표값은 유한한 실수여야 합니다.".to_string(),
            ));
        }

        let original_raw = self.cells.get(&var_coord).cloned();
        let initial_guess = self.evaluate_cell(variable_cell).as_number().unwrap_or(0.0);

        let eval_at = |sheet: &mut SheetEngine, x: f64| -> Option<f64> {
            sheet.set_coord_number(var_coord, x);
            sheet
                .evaluate_cell(target_cell)
                .as_number()
                .map(|y| y - target_value)
        };

        let mut x0 = initial_guess;
        let Some(f0) = eval_at(self, x0) else {
            restore_cell(self, var_coord, original_raw);
            return Err(EasyOfficeError::FormulaError(
                "목표 셀이 수치로 평가되지 않습니다.".to_string(),
            ));
        };
        if f0.abs() < 1e-6 {
            return Ok(x0);
        }

        let mut x1 = if x0.abs() > 1e-6 { x0 * 1.05 } else { 1.0 };
        let Some(mut f1) = eval_at(self, x1) else {
            restore_cell(self, var_coord, original_raw);
            return Err(EasyOfficeError::FormulaError(
                "목표값 찾기 초기 구간 평가에 실패했습니다.".to_string(),
            ));
        };
        let mut f0 = f0;

        for _ in 0..100 {
            if f1.abs() < 1e-6 {
                let rounded = if (x1 - x1.round()).abs() < 1e-9 {
                    x1.round()
                } else {
                    x1
                };
                self.set_coord_number(var_coord, rounded);
                return Ok(rounded);
            }

            let diff = f1 - f0;
            let next_x = if diff.abs() > 1e-14 {
                x1 - f1 * (x1 - x0) / diff
            } else {
                let eps = 1e-5;
                let Some(f_eps) = eval_at(self, x1 + eps) else {
                    break;
                };
                let deriv = (f_eps - f1) / eps;
                if deriv.abs() < 1e-14 {
                    break;
                }
                x1 - f1 / deriv
            };

            if !next_x.is_finite() {
                break;
            }
            x0 = x1;
            f0 = f1;
            x1 = next_x;
            let Some(next_f) = eval_at(self, x1) else {
                break;
            };
            f1 = next_f;
        }

        if f1.abs() < 1e-6 {
            let rounded = if (x1 - x1.round()).abs() < 1e-9 {
                x1.round()
            } else {
                x1
            };
            self.set_coord_number(var_coord, rounded);
            return Ok(rounded);
        }

        restore_cell(self, var_coord, original_raw);
        Err(EasyOfficeError::FormulaError(
            "100회 반복 내에 목표값 찾기가 수렴하지 않았습니다.".to_string(),
        ))
    }
}

fn restore_cell(sheet: &mut SheetEngine, coord: PackedCellCoordinate, orig: Option<String>) {
    match orig {
        Some(v) => {
            sheet.cells.insert(coord, v);
        }
        None => {
            sheet.cells.remove(&coord);
        }
    }
    sheet.rebuild_compiled_asts();
}

/// Supported chart visualization types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartType {
    Bar,
    Line,
    Pie,
    Scatter,
    Area,
}

/// Normalized data point in `[0.0, 1.0]` coordinate space for SVG/Canvas chart rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartPoint {
    pub label: String,
    pub raw_value: f64,
    pub normalized_x: f64,
    pub normalized_y: f64,
    pub pie_share: f64,
}

/// Computed chart series model with normalized coordinates and SVG path geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartModel {
    pub chart_type: ChartType,
    pub points: Vec<ChartPoint>,
    pub min_value: f64,
    pub max_value: f64,
    pub total_value: f64,
    pub svg_path: String,
}

/// Builder computing normalized `[0.0, 1.0]` chart series and SVG/Canvas geometry.
pub struct ChartSeriesBuilder;

impl ChartSeriesBuilder {
    pub fn build(
        sheet: &SheetEngine,
        chart_type: ChartType,
        label_range: CellRange,
        value_range: CellRange,
    ) -> Result<ChartModel, EasyOfficeError> {
        let label_coords = label_range.expand()?;
        let value_coords = value_range.expand()?;
        if label_coords.is_empty() || label_coords.len() != value_coords.len() {
            return Err(EasyOfficeError::InvalidArgument(
                "차트 레이블 범위와 값 범위의 셀 개수가 일치해야 합니다.".to_string(),
            ));
        }

        let mut ctx = EvalContext::standalone_id(sheet.resolved_sheet_id());
        let mut labels = Vec::with_capacity(label_coords.len());
        let mut values = Vec::with_capacity(value_coords.len());
        for (&lc, &vc) in label_coords.iter().zip(value_coords.iter()) {
            labels.push(sheet.eval_coord_ctx(lc, &mut ctx, 0).to_display_string());
            let v = sheet
                .eval_coord_ctx(vc, &mut ctx, 0)
                .as_number()
                .unwrap_or(0.0);
            values.push(if v.is_finite() { v } else { 0.0 });
        }

        let min_value = values.iter().copied().fold(f64::INFINITY, f64::min);
        let max_value = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let max_abs = min_value.abs().max(max_value.abs());
        let scale = if max_abs > 1e150 && max_abs.is_finite() {
            1.0 / max_abs
        } else {
            1.0
        };
        let total_abs: f64 = values.iter().map(|v| v.abs() * scale).sum();
        let raw_total: f64 = values.iter().sum();
        let total_value = if raw_total.is_finite() {
            raw_total
        } else {
            raw_total.signum() * f64::MAX
        };
        let scaled_base_min = (min_value * scale).min(0.0);
        let scaled_span = ((max_value * scale) - scaled_base_min).max(1e-9);

        let n = values.len();
        let mut points = Vec::with_capacity(n);
        let mut svg_cmds = Vec::with_capacity(n + 2);

        for (i, (label, raw_value)) in labels.into_iter().zip(values).enumerate() {
            let normalized_x = if n <= 1 {
                0.5
            } else {
                (i as f64) / ((n - 1) as f64)
            };
            let raw_ny = ((raw_value * scale) - scaled_base_min) / scaled_span;
            let normalized_y = if raw_ny.is_finite() {
                raw_ny.clamp(0.0, 1.0)
            } else {
                0.5
            };
            let raw_pie = if total_abs > 1e-12 && total_abs.is_finite() {
                (raw_value.abs() * scale) / total_abs
            } else {
                0.0
            };
            let pie_share = if raw_pie.is_finite() {
                raw_pie.clamp(0.0, 1.0)
            } else {
                0.0
            };
            let cmd = if i == 0 { "M" } else { "L" };
            svg_cmds.push(format!("{cmd} {normalized_x:.4} {:.4}", 1.0 - normalized_y));
            points.push(ChartPoint {
                label,
                raw_value,
                normalized_x,
                normalized_y,
                pie_share,
            });
        }

        if chart_type == ChartType::Area && !points.is_empty() {
            svg_cmds.push("L 1.0000 1.0000 L 0.0000 1.0000 Z".to_string());
        }

        Ok(ChartModel {
            chart_type,
            points,
            min_value,
            max_value,
            total_value,
            svg_path: svg_cmds.join(" "),
        })
    }
}

#[cfg(test)]
#[path = "workbook_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "workbook_sort_tests.rs"]
mod sort_tests;

#[cfg(test)]
#[path = "workbook_scale_tests.rs"]
mod scale_tests;

use easyoffice_formats::EasyOfficeError;
use std::collections::HashMap;

pub mod functions;
mod functions_date_fin;
mod functions_lookup_text;
mod parser;
pub mod types;
pub mod workbook;

pub use functions::*;
pub use functions_date_fin::{wildcard_match, wildcard_match_alloc_baseline};
pub use parser::{compile_formula_expr, BinaryOp, Expr};
pub use types::*;
pub use workbook::*;

pub const MAX_CELL_TEXT_CHARS: usize = 32_767;
pub const MAX_EVAL_STEPS: usize = 65_536;

pub(crate) struct EvalContext<'a> {
    pub workbook: Option<&'a WorkbookEngine>,
    pub visiting: VisitingTracker,
    pub memo: HashMap<PackedWorkbookCoord, EvaluatedCellValue>,
    pub current_coord: Option<PackedCellCoordinate>,
    pub(crate) steps: usize,
}

impl<'a> EvalContext<'a> {
    #[inline]
    pub fn standalone_id(sheet_id: u16) -> Self {
        Self {
            workbook: None,
            visiting: VisitingTracker::new(sheet_id),
            memo: HashMap::new(),
            current_coord: None,
            steps: 0,
        }
    }

    #[inline]
    pub fn with_workbook(workbook: &'a WorkbookEngine, sheet_id: u16) -> Self {
        Self {
            workbook: Some(workbook),
            visiting: VisitingTracker::new(sheet_id),
            memo: HashMap::new(),
            current_coord: None,
            steps: 0,
        }
    }
}

/// Spreadsheet calculation engine with $O(V + E)$ cycle detection, pre-compiled ASTs, and DAG memoization.
#[derive(Debug, Clone, Default)]
pub struct SheetEngine {
    pub name: String,
    pub(crate) sheet_id: u16,
    pub(crate) cells: HashMap<PackedCellCoordinate, String>,
    pub(crate) compiled_asts: HashMap<PackedCellCoordinate, Result<Expr, FormulaError>>,
    pub(crate) dense_numbers: Vec<f64>,
    pub conditional_rules: Vec<ConditionalFormatRule>,
    pub validation_rule: Option<DataValidationRule>,
}

impl SheetEngine {
    pub const MAX_RECURSION_DEPTH: usize = 1024;
    pub const MAX_CELL_TEXT_CHARS: usize = MAX_CELL_TEXT_CHARS;
    pub const MAX_EVAL_STEPS: usize = MAX_EVAL_STEPS;
    const DENSE_ROWS: usize = 256;
    const DENSE_COLS: usize = 16;

    pub fn new(name: impl Into<String>) -> Self {
        let raw = name.into();
        let name_str = if raw.trim().is_empty() {
            "Sheet1".to_string()
        } else {
            raw
        };
        let sheet_id = compute_sheet_id(&name_str);
        Self {
            name: name_str,
            sheet_id,
            cells: HashMap::new(),
            compiled_asts: HashMap::new(),
            dense_numbers: Vec::new(),
            conditional_rules: Vec::new(),
            validation_rule: None,
        }
    }

    pub fn set_name(&mut self, name: impl Into<String>) {
        let raw = name.into();
        self.name = if raw.trim().is_empty() {
            "Sheet1".to_string()
        } else {
            raw
        };
        self.sheet_id = compute_sheet_id(&self.name);
    }

    #[inline]
    pub fn raw_cells(&self) -> &HashMap<PackedCellCoordinate, String> {
        &self.cells
    }

    #[inline]
    pub(crate) fn resolved_sheet_id(&self) -> u16 {
        if self.sheet_id != 0 {
            self.sheet_id
        } else {
            compute_sheet_id(&self.name)
        }
    }

    fn compile_cell_entry(raw: &str) -> Option<Result<Expr, FormulaError>> {
        let trimmed = raw.trim();
        if let Some(formula_body) = crate::parser::strip_formula_prefix(trimmed) {
            let expr_src = formula_body.trim();
            if expr_src.is_empty() {
                None
            } else {
                Some(compile_formula_expr(expr_src))
            }
        } else if let Ok(num) = trimmed.parse::<f64>() {
            if num.is_finite() {
                Some(Ok(Expr::Number(num)))
            } else {
                Some(Err(FormulaError::Num))
            }
        } else if trimmed.eq_ignore_ascii_case("TRUE") {
            Some(Ok(Expr::Boolean(true)))
        } else if trimmed.eq_ignore_ascii_case("FALSE") {
            Some(Ok(Expr::Boolean(false)))
        } else {
            None
        }
    }

    #[inline]
    fn update_dense_number(
        &mut self,
        coord: PackedCellCoordinate,
        compiled: Option<&Result<Expr, FormulaError>>,
    ) {
        let r = coord.row() as usize;
        let c = coord.col() as usize;
        if r < Self::DENSE_ROWS && c < Self::DENSE_COLS {
            let idx = (r << 4) | c;
            if let Some(Ok(Expr::Number(n))) = compiled {
                if self.dense_numbers.is_empty() {
                    self.dense_numbers = vec![f64::NAN; Self::DENSE_ROWS * Self::DENSE_COLS];
                }
                self.dense_numbers[idx] = *n;
            } else if !self.dense_numbers.is_empty() {
                self.dense_numbers[idx] = f64::NAN;
            }
        }
    }

    pub fn set_cell(
        &mut self,
        a1: &str,
        raw_value: impl Into<String>,
    ) -> Result<(), EasyOfficeError> {
        let coord = PackedCellCoordinate::parse_a1(a1)?;
        let val = raw_value.into();
        if val.is_empty() {
            self.cells.remove(&coord);
            self.compiled_asts.remove(&coord);
            self.update_dense_number(coord, None);
        } else {
            let compiled = Self::compile_cell_entry(&val);
            self.update_dense_number(coord, compiled.as_ref());
            if let Some(comp) = compiled {
                self.compiled_asts.insert(coord, comp);
            } else {
                self.compiled_asts.remove(&coord);
            }
            self.cells.insert(coord, val);
        }
        Ok(())
    }

    #[inline]
    pub(crate) fn set_coord_number(&mut self, coord: PackedCellCoordinate, num: f64) {
        let comp = Ok(Expr::Number(num));
        self.update_dense_number(coord, Some(&comp));
        self.compiled_asts.insert(coord, comp);
        self.cells.insert(coord, format!("{num}"));
    }

    pub(crate) fn rebuild_compiled_asts(&mut self) {
        if self.sheet_id == 0 {
            self.sheet_id = compute_sheet_id(&self.name);
        }
        self.compiled_asts.clear();
        self.dense_numbers.clear();
        let coords_and_compiled: Vec<(PackedCellCoordinate, Option<Result<Expr, FormulaError>>)> =
            self.cells
                .iter()
                .map(|(&coord, raw)| (coord, Self::compile_cell_entry(raw)))
                .collect();
        for (coord, compiled) in coords_and_compiled {
            self.update_dense_number(coord, compiled.as_ref());
            if let Some(comp) = compiled {
                self.compiled_asts.insert(coord, comp);
            }
        }
    }

    pub fn get_raw_cell(&self, a1: &str) -> Option<&str> {
        let coord = PackedCellCoordinate::parse_a1(a1).ok()?;
        self.cells.get(&coord).map(String::as_str)
    }

    pub fn evaluate_cell(&self, a1: &str) -> EvaluatedCellValue {
        let Ok(coord) = PackedCellCoordinate::parse_a1(a1) else {
            return EvaluatedCellValue::Error(FormulaError::Ref);
        };
        let mut ctx = EvalContext::standalone_id(self.resolved_sheet_id());
        self.eval_coord_ctx(coord, &mut ctx, 0)
    }

    /// Evaluates all populated cells in the worksheet in a single shared DAG memoization pass
    /// using pre-compiled ASTs and the inline `VisitingTracker` bitset.
    pub fn recalculate_all(&self) -> HashMap<PackedCellCoordinate, EvaluatedCellValue> {
        let mut ctx = EvalContext::standalone_id(self.resolved_sheet_id());
        let mut results = HashMap::with_capacity(self.cells.len());
        let mut coords: Vec<PackedCellCoordinate> = self.cells.keys().copied().collect();
        coords.sort_unstable();
        for coord in coords {
            ctx.steps = 0;
            let val = self.eval_coord_ctx(coord, &mut ctx, 0);
            results.insert(coord, val);
        }
        results
    }

    pub fn evaluate_formula_str(&self, formula: &str) -> EvaluatedCellValue {
        let mut ctx = EvalContext::standalone_id(self.resolved_sheet_id());
        self.eval_raw_string_ctx(formula, &mut ctx, 0)
    }

    #[inline]
    pub(crate) fn eval_sheet_coord(
        &self,
        sheet_opt: Option<&str>,
        coord: PackedCellCoordinate,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> EvaluatedCellValue {
        match sheet_opt {
            None => self.eval_coord_ctx(coord, ctx, depth),
            Some(target_name) => {
                if WorkbookEngine::is_same_sheet_identity(target_name, &self.name) {
                    return self.eval_coord_ctx(coord, ctx, depth);
                }
                let Some(wb) = ctx.workbook else {
                    return EvaluatedCellValue::Error(FormulaError::Ref);
                };
                let Some(target_sheet) = wb.sheet(target_name) else {
                    return EvaluatedCellValue::Error(FormulaError::Ref);
                };
                target_sheet.eval_coord_ctx(coord, ctx, depth)
            }
        }
    }

    #[inline]
    pub(crate) fn is_nested_subtotal_cell(
        &self,
        sheet_opt: Option<&str>,
        coord: PackedCellCoordinate,
        ctx: &EvalContext<'_>,
    ) -> bool {
        let target_sheet = match sheet_opt {
            None => self,
            Some(target_name)
                if WorkbookEngine::is_same_sheet_identity(target_name, &self.name) =>
            {
                self
            }
            Some(target_name) => {
                let Some(wb) = ctx.workbook else {
                    return false;
                };
                let Some(ts) = wb.sheet(target_name) else {
                    return false;
                };
                ts
            }
        };
        matches!(
            target_sheet.compiled_asts.get(&coord),
            Some(Ok(expr)) if expr_contains_subtotal(expr)
        )
    }

    #[inline]
    pub(crate) fn eval_coord_ctx(
        &self,
        coord: PackedCellCoordinate,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> EvaluatedCellValue {
        ctx.steps = ctx.steps.saturating_add(1);
        if ctx.steps > Self::MAX_EVAL_STEPS {
            return EvaluatedCellValue::Error(FormulaError::Value);
        }
        if depth > Self::MAX_RECURSION_DEPTH {
            return EvaluatedCellValue::Error(FormulaError::Ref);
        }
        let r = coord.row() as usize;
        let c = coord.col() as usize;
        if r < Self::DENSE_ROWS && c < Self::DENSE_COLS {
            if let Some(&n) = self.dense_numbers.get((r << 4) | c) {
                if !n.is_nan() {
                    return EvaluatedCellValue::Number(n);
                }
            }
        }
        let Some(compiled) = self.compiled_asts.get(&coord) else {
            return match self.cells.get(&coord) {
                None => EvaluatedCellValue::Empty,
                Some(raw) => {
                    if crate::parser::strip_formula_prefix(raw).is_some_and(|s| s.trim().is_empty())
                    {
                        EvaluatedCellValue::Empty
                    } else if raw.chars().count() > Self::MAX_CELL_TEXT_CHARS {
                        EvaluatedCellValue::Error(FormulaError::Value)
                    } else {
                        EvaluatedCellValue::Text(raw.clone())
                    }
                }
            };
        };
        match compiled {
            Ok(Expr::Number(n)) => return EvaluatedCellValue::Number(*n),
            Ok(Expr::Boolean(b)) => return EvaluatedCellValue::Boolean(*b),
            Err(e) => return EvaluatedCellValue::Error(*e),
            Ok(_) => {}
        }

        let sid = self.resolved_sheet_id();
        let key = PackedWorkbookCoord::new(sid, coord);
        if let Some(cached) = ctx.memo.get(&key) {
            return cached.clone();
        }
        if !ctx.visiting.insert(sid, coord) {
            return EvaluatedCellValue::Error(FormulaError::Cycle);
        }

        let prev_coord = ctx.current_coord;
        ctx.current_coord = Some(coord);
        let mut result = match compiled {
            Ok(expr) => self.eval_expr_ctx(expr, ctx, depth + 1),
            Err(e) => EvaluatedCellValue::Error(*e),
        };
        ctx.current_coord = prev_coord;
        if let EvaluatedCellValue::Text(ref s) = result {
            if s.chars().count() > Self::MAX_CELL_TEXT_CHARS {
                result = EvaluatedCellValue::Error(FormulaError::Value);
            }
        }

        ctx.visiting.remove(sid, coord);
        ctx.memo.insert(key, result.clone());
        result
    }

    pub(crate) fn eval_raw_string_ctx(
        &self,
        raw: &str,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> EvaluatedCellValue {
        if raw.is_empty() {
            return EvaluatedCellValue::Empty;
        }
        let trimmed = raw.trim();
        let mut res = if let Some(formula_body) = crate::parser::strip_formula_prefix(trimmed) {
            let expr_src = formula_body.trim();
            if expr_src.is_empty() {
                return EvaluatedCellValue::Empty;
            }
            match compile_formula_expr(expr_src) {
                Ok(expr) => self.eval_expr_ctx(&expr, ctx, depth),
                Err(e) => EvaluatedCellValue::Error(e),
            }
        } else if let Ok(num) = trimmed.parse::<f64>() {
            if num.is_finite() {
                EvaluatedCellValue::Number(num)
            } else {
                EvaluatedCellValue::Error(FormulaError::Num)
            }
        } else if trimmed.eq_ignore_ascii_case("TRUE") {
            EvaluatedCellValue::Boolean(true)
        } else if trimmed.eq_ignore_ascii_case("FALSE") {
            EvaluatedCellValue::Boolean(false)
        } else {
            EvaluatedCellValue::Text(raw.to_string())
        };
        if let EvaluatedCellValue::Text(ref s) = res {
            if s.chars().count() > Self::MAX_CELL_TEXT_CHARS {
                res = EvaluatedCellValue::Error(FormulaError::Value);
            }
        }
        res
    }

    pub(crate) fn eval_expr_ctx(
        &self,
        expr: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> EvaluatedCellValue {
        ctx.steps = ctx.steps.saturating_add(1);
        if ctx.steps > Self::MAX_EVAL_STEPS {
            return EvaluatedCellValue::Error(FormulaError::Value);
        }
        if depth > Self::MAX_RECURSION_DEPTH {
            return EvaluatedCellValue::Error(FormulaError::Ref);
        }
        match expr {
            Expr::Number(n) => EvaluatedCellValue::Number(*n),
            Expr::Text(s) => {
                if s.chars().count() > Self::MAX_CELL_TEXT_CHARS {
                    EvaluatedCellValue::Error(FormulaError::Value)
                } else {
                    EvaluatedCellValue::Text(s.clone())
                }
            }
            Expr::Boolean(b) => EvaluatedCellValue::Boolean(*b),
            Expr::ErrorLiteral(e) => EvaluatedCellValue::Error(*e),
            Expr::Cell(coord) => self.eval_coord_ctx(*coord, ctx, depth),
            Expr::SheetCell { sheet, coord } => {
                self.eval_sheet_coord(Some(sheet), *coord, ctx, depth)
            }
            Expr::Range(_) | Expr::SheetRange { .. } => {
                EvaluatedCellValue::Error(FormulaError::Value)
            }
            Expr::UnaryPlus(inner) => {
                let val = self.eval_expr_ctx(inner, ctx, depth);
                if let EvaluatedCellValue::Error(e) = val {
                    return EvaluatedCellValue::Error(e);
                }
                match val.as_number() {
                    Some(n) => EvaluatedCellValue::Number(n),
                    None => EvaluatedCellValue::Error(FormulaError::Value),
                }
            }
            Expr::UnaryMinus(inner) => {
                let val = self.eval_expr_ctx(inner, ctx, depth);
                if let EvaluatedCellValue::Error(e) = val {
                    return EvaluatedCellValue::Error(e);
                }
                match val.as_number() {
                    Some(n) => EvaluatedCellValue::Number(-n),
                    None => EvaluatedCellValue::Error(FormulaError::Value),
                }
            }
            Expr::PostfixPercent(inner) => {
                let val = self.eval_expr_ctx(inner, ctx, depth);
                if let EvaluatedCellValue::Error(e) = val {
                    return EvaluatedCellValue::Error(e);
                }
                match val.as_number() {
                    Some(n) => EvaluatedCellValue::Number(n / 100.0),
                    None => EvaluatedCellValue::Error(FormulaError::Value),
                }
            }
            Expr::Binary { .. } => {
                let mut chain: Vec<(BinaryOp, &Expr)> = Vec::new();
                let mut cur = expr;
                while let Expr::Binary { op, lhs, rhs } = cur {
                    chain.push((*op, rhs));
                    cur = lhs;
                }
                let mut l_val = self.eval_expr_ctx(cur, ctx, depth);
                if let EvaluatedCellValue::Error(e) = l_val {
                    return EvaluatedCellValue::Error(e);
                }
                while let Some((op, rhs)) = chain.pop() {
                    let r_val = self.eval_expr_ctx(rhs, ctx, depth);
                    if let EvaluatedCellValue::Error(e) = r_val {
                        return EvaluatedCellValue::Error(e);
                    }
                    l_val = Self::apply_binary_op(op, l_val, r_val);
                    if let EvaluatedCellValue::Error(e) = l_val {
                        return EvaluatedCellValue::Error(e);
                    }
                }
                l_val
            }
            Expr::FuncCall { name, args } => match name.as_str() {
                "SUM" | "AVERAGE" | "AVG" | "MIN" | "MAX" | "COUNT" | "COUNTA" | "COUNTBLANK"
                | "PRODUCT" => match self.eval_streaming_range_agg_ctx(name, args, ctx, depth) {
                    Ok(v) => v,
                    Err(e) => EvaluatedCellValue::Error(e),
                },
                _ => self.eval_func_ctx(name, args, ctx, depth),
            },
        }
    }

    pub(crate) fn apply_binary_op(
        op: BinaryOp,
        l_val: EvaluatedCellValue,
        r_val: EvaluatedCellValue,
    ) -> EvaluatedCellValue {
        match op {
            BinaryOp::Concat => {
                let ls = l_val.to_display_string();
                let rs = r_val.to_display_string();
                if ls.chars().count().saturating_add(rs.chars().count()) > Self::MAX_CELL_TEXT_CHARS
                {
                    EvaluatedCellValue::Error(FormulaError::Value)
                } else {
                    EvaluatedCellValue::Text(format!("{ls}{rs}"))
                }
            }
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Pow => {
                let ln = match l_val {
                    EvaluatedCellValue::Empty => 0.0,
                    _ => match l_val.as_number() {
                        Some(v) => v,
                        None => return EvaluatedCellValue::Error(FormulaError::Value),
                    },
                };
                let rn = match r_val {
                    EvaluatedCellValue::Empty => 0.0,
                    _ => match r_val.as_number() {
                        Some(v) => v,
                        None => return EvaluatedCellValue::Error(FormulaError::Value),
                    },
                };
                let res = match op {
                    BinaryOp::Add => ln + rn,
                    BinaryOp::Sub => ln - rn,
                    BinaryOp::Mul => ln * rn,
                    BinaryOp::Div => {
                        if rn == 0.0 {
                            return EvaluatedCellValue::Error(FormulaError::DivByZero);
                        }
                        ln / rn
                    }
                    BinaryOp::Pow => ln.powf(rn),
                    _ => unreachable!(),
                };
                if res.is_finite() {
                    EvaluatedCellValue::Number(res)
                } else {
                    EvaluatedCellValue::Error(FormulaError::Num)
                }
            }
            BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge => {
                let Some(ordering) = Self::compare_excel_values(&l_val, &r_val) else {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                };
                let flag = match op {
                    BinaryOp::Eq => ordering.is_eq(),
                    BinaryOp::Ne => ordering.is_ne(),
                    BinaryOp::Lt => ordering.is_lt(),
                    BinaryOp::Le => ordering.is_le(),
                    BinaryOp::Gt => ordering.is_gt(),
                    BinaryOp::Ge => ordering.is_ge(),
                    _ => unreachable!(),
                };
                EvaluatedCellValue::Boolean(flag)
            }
        }
    }

    pub(crate) fn compare_excel_values(
        l_val: &EvaluatedCellValue,
        r_val: &EvaluatedCellValue,
    ) -> Option<std::cmp::Ordering> {
        match (l_val, r_val) {
            (EvaluatedCellValue::Error(_), _) | (_, EvaluatedCellValue::Error(_)) => None,
            (EvaluatedCellValue::Empty, EvaluatedCellValue::Empty) => {
                Some(std::cmp::Ordering::Equal)
            }
            (EvaluatedCellValue::Empty, EvaluatedCellValue::Number(b)) => 0.0f64.partial_cmp(b),
            (EvaluatedCellValue::Number(a), EvaluatedCellValue::Empty) => a.partial_cmp(&0.0),
            (EvaluatedCellValue::Empty, EvaluatedCellValue::Text(b)) => Some("".cmp(b.as_str())),
            (EvaluatedCellValue::Text(a), EvaluatedCellValue::Empty) => Some(a.as_str().cmp("")),
            (EvaluatedCellValue::Empty, EvaluatedCellValue::Boolean(b)) => Some(false.cmp(b)),
            (EvaluatedCellValue::Boolean(a), EvaluatedCellValue::Empty) => Some(a.cmp(&false)),
            (EvaluatedCellValue::Number(a), EvaluatedCellValue::Number(b)) => a.partial_cmp(b),
            (EvaluatedCellValue::Text(a), EvaluatedCellValue::Text(b)) => {
                Some(a.to_lowercase().cmp(&b.to_lowercase()))
            }
            (EvaluatedCellValue::Boolean(a), EvaluatedCellValue::Boolean(b)) => Some(a.cmp(b)),
            (EvaluatedCellValue::Number(_), _) => Some(std::cmp::Ordering::Less),
            (_, EvaluatedCellValue::Number(_)) => Some(std::cmp::Ordering::Greater),
            (EvaluatedCellValue::Text(_), EvaluatedCellValue::Boolean(_)) => {
                Some(std::cmp::Ordering::Less)
            }
            (EvaluatedCellValue::Boolean(_), EvaluatedCellValue::Text(_)) => {
                Some(std::cmp::Ordering::Greater)
            }
        }
    }

    pub(crate) fn eval_streaming_range_agg_ctx(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        let mut acc0 = 0.0f64;
        let mut acc1 = 0.0f64;
        let mut acc2 = 0.0f64;
        let mut acc3 = 0.0f64;
        let mut p0 = 1.0f64;
        let mut p1 = 1.0f64;
        let mut p2 = 1.0f64;
        let mut p3 = 1.0f64;
        let mut num_count = 0usize;
        let mut item_count = 0usize;
        let mut min_val: Option<f64> = None;
        let mut max_val: Option<f64> = None;

        let mut fold_cell =
            |v: EvaluatedCellValue, lane: usize, from_range: bool| -> Result<(), FormulaError> {
                if let EvaluatedCellValue::Error(e) = v {
                    return Err(e);
                }
                let extract_num = |val: &EvaluatedCellValue| -> Result<Option<f64>, FormulaError> {
                    if from_range {
                        Ok(match val {
                            EvaluatedCellValue::Number(n) => Some(*n),
                            _ => None,
                        })
                    } else {
                        match val {
                            EvaluatedCellValue::Empty => Ok(None),
                            _ => val.as_number().map(Some).ok_or(FormulaError::Value),
                        }
                    }
                };
                match name {
                    "SUM" | "AVERAGE" | "AVG" => {
                        if let Some(n) = extract_num(&v)? {
                            num_count += 1;
                            match lane & 3 {
                                0 => acc0 += n,
                                1 => acc1 += n,
                                2 => acc2 += n,
                                _ => acc3 += n,
                            }
                        }
                    }
                    "PRODUCT" => {
                        if let Some(n) = extract_num(&v)? {
                            num_count += 1;
                            match lane & 3 {
                                0 => p0 *= n,
                                1 => p1 *= n,
                                2 => p2 *= n,
                                _ => p3 *= n,
                            }
                        }
                    }
                    "MIN" => {
                        if let Some(n) = extract_num(&v)? {
                            min_val = Some(min_val.map_or(n, |m| m.min(n)));
                        }
                    }
                    "MAX" => {
                        if let Some(n) = extract_num(&v)? {
                            max_val = Some(max_val.map_or(n, |m| m.max(n)));
                        }
                    }
                    "COUNT" => {
                        if matches!(v, EvaluatedCellValue::Number(_)) {
                            item_count += 1;
                        }
                    }
                    "COUNTA" => {
                        if !matches!(v, EvaluatedCellValue::Empty) {
                            item_count += 1;
                        }
                    }
                    "COUNTBLANK"
                        if matches!(v, EvaluatedCellValue::Empty)
                            || matches!(&v, EvaluatedCellValue::Text(s) if s.is_empty()) =>
                    {
                        item_count += 1;
                    }
                    _ => {}
                }
                Ok(())
            };

        for arg in args {
            match arg {
                Expr::Range(range) => {
                    if range.cell_count() > CellRange::MAX_EXPANDED_CELLS {
                        return Err(FormulaError::Ref);
                    }
                    let min_r = range.start.row().min(range.end.row());
                    let max_r = range.start.row().max(range.end.row());
                    let min_c = range.start.col().min(range.end.col());
                    let max_c = range.start.col().max(range.end.col());
                    for r in min_r..=max_r {
                        let mut c = min_c;
                        while c + 3 <= max_c {
                            let c0 =
                                PackedCellCoordinate::new(r, c).map_err(|_| FormulaError::Ref)?;
                            let c1 = PackedCellCoordinate::new(r, c + 1)
                                .map_err(|_| FormulaError::Ref)?;
                            let c2 = PackedCellCoordinate::new(r, c + 2)
                                .map_err(|_| FormulaError::Ref)?;
                            let c3 = PackedCellCoordinate::new(r, c + 3)
                                .map_err(|_| FormulaError::Ref)?;
                            fold_cell(self.eval_coord_ctx(c0, ctx, depth), 0, true)?;
                            fold_cell(self.eval_coord_ctx(c1, ctx, depth), 1, true)?;
                            fold_cell(self.eval_coord_ctx(c2, ctx, depth), 2, true)?;
                            fold_cell(self.eval_coord_ctx(c3, ctx, depth), 3, true)?;
                            c += 4;
                        }
                        while c <= max_c {
                            let coord =
                                PackedCellCoordinate::new(r, c).map_err(|_| FormulaError::Ref)?;
                            fold_cell(self.eval_coord_ctx(coord, ctx, depth), 0, true)?;
                            c += 1;
                        }
                    }
                }
                Expr::SheetRange { sheet, range } => {
                    if range.cell_count() > CellRange::MAX_EXPANDED_CELLS {
                        return Err(FormulaError::Ref);
                    }
                    let min_r = range.start.row().min(range.end.row());
                    let max_r = range.start.row().max(range.end.row());
                    let min_c = range.start.col().min(range.end.col());
                    let max_c = range.start.col().max(range.end.col());
                    for r in min_r..=max_r {
                        let mut c = min_c;
                        while c + 3 <= max_c {
                            let c0 =
                                PackedCellCoordinate::new(r, c).map_err(|_| FormulaError::Ref)?;
                            let c1 = PackedCellCoordinate::new(r, c + 1)
                                .map_err(|_| FormulaError::Ref)?;
                            let c2 = PackedCellCoordinate::new(r, c + 2)
                                .map_err(|_| FormulaError::Ref)?;
                            let c3 = PackedCellCoordinate::new(r, c + 3)
                                .map_err(|_| FormulaError::Ref)?;
                            fold_cell(self.eval_sheet_coord(Some(sheet), c0, ctx, depth), 0, true)?;
                            fold_cell(self.eval_sheet_coord(Some(sheet), c1, ctx, depth), 1, true)?;
                            fold_cell(self.eval_sheet_coord(Some(sheet), c2, ctx, depth), 2, true)?;
                            fold_cell(self.eval_sheet_coord(Some(sheet), c3, ctx, depth), 3, true)?;
                            c += 4;
                        }
                        while c <= max_c {
                            let coord =
                                PackedCellCoordinate::new(r, c).map_err(|_| FormulaError::Ref)?;
                            fold_cell(
                                self.eval_sheet_coord(Some(sheet), coord, ctx, depth),
                                0,
                                true,
                            )?;
                            c += 1;
                        }
                    }
                }
                Expr::Cell(_) | Expr::SheetCell { .. } => {
                    let v = self.eval_expr_ctx(arg, ctx, depth);
                    fold_cell(v, 0, true)?;
                }
                Expr::FuncCall { name: fn_name, .. }
                    if matches!(fn_name.as_str(), "INDIRECT" | "OFFSET" | "TRANSPOSE") =>
                {
                    if let Some(spec) = self.extract_streaming_range_ctx(arg, ctx, depth)? {
                        let (_, _, _, rows, cols, _) = spec;
                        for dr in 0..rows {
                            for dc in 0..cols {
                                let v = self
                                    .eval_streaming_cell_allow_error(&spec, dr, dc, ctx, depth)?;
                                fold_cell(v, (dr * cols + dc) & 3, true)?;
                            }
                        }
                    } else {
                        let v = self.eval_expr_ctx(arg, ctx, depth);
                        fold_cell(v, 0, false)?;
                    }
                }
                _ => {
                    let v = self.eval_expr_ctx(arg, ctx, depth);
                    fold_cell(v, 0, false)?;
                }
            }
        }

        let total_sum = (acc0 + acc1) + (acc2 + acc3);
        match name {
            "SUM" => Ok(EvaluatedCellValue::Number(total_sum)),
            "PRODUCT" => Ok(EvaluatedCellValue::Number(if num_count == 0 {
                0.0
            } else {
                (p0 * p1) * (p2 * p3)
            })),
            "AVERAGE" | "AVG" => {
                if num_count == 0 {
                    Err(FormulaError::DivByZero)
                } else {
                    Ok(EvaluatedCellValue::Number(total_sum / (num_count as f64)))
                }
            }
            "MIN" => Ok(EvaluatedCellValue::Number(min_val.unwrap_or(0.0))),
            "MAX" => Ok(EvaluatedCellValue::Number(max_val.unwrap_or(0.0))),
            "COUNT" | "COUNTA" | "COUNTBLANK" => Ok(EvaluatedCellValue::Number(item_count as f64)),
            _ => Err(FormulaError::Name),
        }
    }

    pub(crate) fn expand_args_ctx(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Vec<EvaluatedCellValue>, FormulaError> {
        let mut out = Vec::new();
        self.visit_args_streaming(args, ctx, depth, |v| {
            out.push(v);
            Ok(())
        })?;
        Ok(out)
    }

    pub(crate) fn values_match(a: &EvaluatedCellValue, b: &EvaluatedCellValue) -> bool {
        match (a, b) {
            (EvaluatedCellValue::Number(na), EvaluatedCellValue::Number(nb)) => na == nb,
            (EvaluatedCellValue::Boolean(ba), EvaluatedCellValue::Boolean(bb)) => ba == bb,
            (EvaluatedCellValue::Text(sa), EvaluatedCellValue::Text(sb)) => {
                if sa.is_ascii() && sb.is_ascii() {
                    sa.eq_ignore_ascii_case(sb)
                } else {
                    sa.to_lowercase() == sb.to_lowercase()
                }
            }
            (EvaluatedCellValue::Empty, EvaluatedCellValue::Empty) => true,
            (EvaluatedCellValue::Empty, EvaluatedCellValue::Text(s))
            | (EvaluatedCellValue::Text(s), EvaluatedCellValue::Empty) => s.is_empty(),
            _ => false,
        }
    }

    pub(crate) fn values_match_wildcard(
        cell: &EvaluatedCellValue,
        pattern: &EvaluatedCellValue,
    ) -> bool {
        if let (EvaluatedCellValue::Text(s), EvaluatedCellValue::Text(p)) = (cell, pattern) {
            wildcard_match(s, p)
        } else {
            Self::values_match(cell, pattern)
        }
    }
}

pub(crate) fn expr_contains_subtotal(expr: &Expr) -> bool {
    match expr {
        Expr::FuncCall { name, args } => {
            name.eq_ignore_ascii_case("SUBTOTAL") || args.iter().any(expr_contains_subtotal)
        }
        Expr::UnaryPlus(inner) | Expr::UnaryMinus(inner) | Expr::PostfixPercent(inner) => {
            expr_contains_subtotal(inner)
        }
        Expr::Binary { lhs, rhs, .. } => expr_contains_subtotal(lhs) || expr_contains_subtotal(rhs),
        Expr::Number(_)
        | Expr::Text(_)
        | Expr::Boolean(_)
        | Expr::ErrorLiteral(_)
        | Expr::Cell(_)
        | Expr::Range(_)
        | Expr::SheetCell { .. }
        | Expr::SheetRange { .. } => false,
    }
}

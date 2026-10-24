use crate::functions_date_fin::matches_criteria;
use crate::{
    CellRange, EvalContext, EvaluatedCellValue, Expr, FormulaError, PackedCellCoordinate,
    SheetEngine,
};

type StreamingRangeSpec<'a> = (Option<&'a str>, u32, u32, usize, usize, bool);

/// Canonical 122 built-in spreadsheet functions across 8 standard categories:
/// Math & Trig (27), Statistical (22), Logical (10), Lookup & Reference (12),
/// Text (21), Date & Time (10), Financial (11), and Information (9).
pub const CANONICAL_122_FUNCTIONS: [&str; 122] = [
    // Math & Trig (27)
    "SUM",
    "PRODUCT",
    "SUMPRODUCT",
    "SUMIF",
    "SUMIFS",
    "ABS",
    "SIGN",
    "ROUND",
    "ROUNDUP",
    "ROUNDDOWN",
    "INT",
    "TRUNC",
    "CEILING",
    "FLOOR",
    "MOD",
    "QUOTIENT",
    "SQRT",
    "POWER",
    "EXP",
    "LN",
    "LOG",
    "LOG10",
    "PI",
    "SIN",
    "COS",
    "TAN",
    "RAND",
    // Statistical (22)
    "AVERAGE",
    "AVERAGEA",
    "AVERAGEIF",
    "AVERAGEIFS",
    "MEDIAN",
    "MODE",
    "MIN",
    "MAX",
    "SMALL",
    "LARGE",
    "COUNT",
    "COUNTA",
    "COUNTBLANK",
    "COUNTIF",
    "COUNTIFS",
    "STDEV",
    "STDEVP",
    "VAR",
    "VARP",
    "RANK",
    "PERCENTILE",
    "QUARTILE",
    // Logical (10)
    "IF",
    "IFS",
    "AND",
    "OR",
    "NOT",
    "XOR",
    "IFERROR",
    "IFNA",
    "SWITCH",
    "TRUE",
    // Lookup & Reference (12)
    "VLOOKUP",
    "HLOOKUP",
    "XLOOKUP",
    "INDEX",
    "MATCH",
    "CHOOSE",
    "OFFSET",
    "ROW",
    "COLUMN",
    "ROWS",
    "COLUMNS",
    "TRANSPOSE",
    // Text (21)
    "LEN",
    "CONCAT",
    "CONCATENATE",
    "TEXTJOIN",
    "LEFT",
    "RIGHT",
    "MID",
    "TRIM",
    "UPPER",
    "LOWER",
    "PROPER",
    "SUBSTITUTE",
    "REPLACE",
    "FIND",
    "SEARCH",
    "EXACT",
    "REPT",
    "VALUE",
    "TEXT",
    "CHAR",
    "CODE",
    // Date & Time (10)
    "DATE",
    "YEAR",
    "MONTH",
    "DAY",
    "TIME",
    "HOUR",
    "MINUTE",
    "SECOND",
    "DAYS",
    "EDATE",
    // Financial (11)
    "PMT",
    "IPMT",
    "PPMT",
    "FV",
    "PV",
    "NPV",
    "IRR",
    "RATE",
    "NPER",
    "SLN",
    "DDB",
    // Information (9)
    "ISBLANK",
    "ISNUMBER",
    "ISTEXT",
    "ISLOGICAL",
    "ISERR",
    "ISERROR",
    "ISNA",
    "ISEVEN",
    "ISODD",
];

impl SheetEngine {
    /// Returns the slice of all 122 canonical built-in spreadsheet function names.
    pub fn supported_function_names() -> &'static [&'static str] {
        &CANONICAL_122_FUNCTIONS
    }

    pub(crate) fn eval_func_ctx(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> EvaluatedCellValue {
        match self.eval_func_inner(name, args, ctx, depth) {
            Ok(v) => v,
            Err(e) => EvaluatedCellValue::Error(e),
        }
    }

    pub(crate) fn eval_arg_num(
        &self,
        arg: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<f64, FormulaError> {
        let val = self.eval_expr_ctx(arg, ctx, depth + 1);
        if let EvaluatedCellValue::Error(e) = val {
            return Err(e);
        }
        val.as_number().ok_or(FormulaError::Value)
    }

    pub(crate) fn eval_arg_val(
        &self,
        arg: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        let val = self.eval_expr_ctx(arg, ctx, depth + 1);
        if let EvaluatedCellValue::Error(e) = val {
            return Err(e);
        }
        Ok(val)
    }

    pub(crate) fn extract_range_and_sheet<'b>(
        &self,
        expr: &'b Expr,
    ) -> Option<(Option<&'b str>, CellRange, bool)> {
        match expr {
            Expr::Cell(c) => CellRange::new(*c, *c).ok().map(|r| (None, r, false)),
            Expr::Range(r) => Some((None, *r, false)),
            Expr::SheetCell { sheet, coord } => CellRange::new(*coord, *coord)
                .ok()
                .map(|r| (Some(sheet.as_str()), r, false)),
            Expr::SheetRange { sheet, range } => Some((Some(sheet.as_str()), *range, false)),
            Expr::FuncCall { name, args } if name == "TRANSPOSE" && args.len() == 1 => self
                .extract_range_and_sheet(&args[0])
                .map(|(s, r, t)| (s, r, !t)),
            _ => None,
        }
    }

    fn extract_streaming_range<'b>(
        &self,
        expr: &'b Expr,
    ) -> Result<Option<StreamingRangeSpec<'b>>, FormulaError> {
        let Some((sheet_opt, range, transposed)) = self.extract_range_and_sheet(expr) else {
            return Ok(None);
        };
        if range.cell_count() > CellRange::MAX_EXPANDED_CELLS {
            return Err(FormulaError::Ref);
        }
        let min_r = range.start.row().min(range.end.row());
        let max_r = range.start.row().max(range.end.row());
        let min_c = range.start.col().min(range.end.col());
        let max_c = range.start.col().max(range.end.col());
        let phys_rows = (max_r - min_r + 1) as usize;
        let phys_cols = (max_c - min_c + 1) as usize;
        let (rows, cols) = if transposed {
            (phys_cols, phys_rows)
        } else {
            (phys_rows, phys_cols)
        };
        Ok(Some((sheet_opt, min_r, min_c, rows, cols, transposed)))
    }

    #[inline]
    fn eval_streaming_cell(
        &self,
        spec: StreamingRangeSpec<'_>,
        dr: usize,
        dc: usize,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        let (sheet_opt, min_r, min_c, _rows, _cols, transposed) = spec;
        let (r, c) = if transposed {
            (min_r + dc as u32, min_c + dr as u32)
        } else {
            (min_r + dr as u32, min_c + dc as u32)
        };
        let coord = PackedCellCoordinate::new(r, c).map_err(|_| FormulaError::Ref)?;
        let v = self.eval_sheet_coord(sheet_opt, coord, ctx, depth + 1);
        if let EvaluatedCellValue::Error(e) = v {
            return Err(e);
        }
        Ok(v)
    }

    pub(crate) fn eval_range_vec(
        &self,
        expr: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Vec<EvaluatedCellValue>, FormulaError> {
        if let Some((sheet_opt, range, transposed)) = self.extract_range_and_sheet(expr) {
            if range.cell_count() > CellRange::MAX_EXPANDED_CELLS {
                return Err(FormulaError::Ref);
            }
            let min_r = range.start.row().min(range.end.row());
            let max_r = range.start.row().max(range.end.row());
            let min_c = range.start.col().min(range.end.col());
            let max_c = range.start.col().max(range.end.col());
            let mut out = Vec::with_capacity(range.cell_count());
            if transposed {
                for c in min_c..=max_c {
                    for r in min_r..=max_r {
                        let coord =
                            PackedCellCoordinate::new(r, c).map_err(|_| FormulaError::Ref)?;
                        let v = self.eval_sheet_coord(sheet_opt, coord, ctx, depth + 1);
                        if let EvaluatedCellValue::Error(e) = v {
                            return Err(e);
                        }
                        out.push(v);
                    }
                }
            } else {
                for r in min_r..=max_r {
                    for c in min_c..=max_c {
                        let coord =
                            PackedCellCoordinate::new(r, c).map_err(|_| FormulaError::Ref)?;
                        let v = self.eval_sheet_coord(sheet_opt, coord, ctx, depth + 1);
                        if let EvaluatedCellValue::Error(e) = v {
                            return Err(e);
                        }
                        out.push(v);
                    }
                }
            }
            Ok(out)
        } else {
            Ok(vec![self.eval_arg_val(expr, ctx, depth)?])
        }
    }
}

#[derive(Clone, Copy)]
enum CritNumOp {
    Ge,
    Le,
    Ne,
    Gt,
    Lt,
    Eq,
}

enum CompiledCriteria<'a> {
    Number(f64),
    NumericCmp(CritNumOp, f64),
    Fallback(&'a EvaluatedCellValue),
}

impl<'a> CompiledCriteria<'a> {
    #[inline]
    fn compile(crit: &'a EvaluatedCellValue) -> Self {
        match crit {
            EvaluatedCellValue::Number(cn) => Self::Number(*cn),
            EvaluatedCellValue::Text(s) => {
                let trimmed = s.trim();
                for (op_str, op_kind) in [
                    (">=", CritNumOp::Ge),
                    ("<=", CritNumOp::Le),
                    ("<>", CritNumOp::Ne),
                    (">", CritNumOp::Gt),
                    ("<", CritNumOp::Lt),
                    ("=", CritNumOp::Eq),
                ] {
                    if let Some(rhs) = trimmed.strip_prefix(op_str) {
                        if let Ok(rhs_num) = rhs.trim().parse::<f64>() {
                            return Self::NumericCmp(op_kind, rhs_num);
                        }
                        return Self::Fallback(crit);
                    }
                }
                if let Ok(rhs_num) = trimmed.parse::<f64>() {
                    return Self::Number(rhs_num);
                }
                Self::Fallback(crit)
            }
            _ => Self::Fallback(crit),
        }
    }

    #[inline]
    fn matches(&self, cell: &EvaluatedCellValue) -> bool {
        match *self {
            Self::Number(cn) => cell.as_number().is_some_and(|v| (v - cn).abs() < 1e-9),
            Self::NumericCmp(op, rhs_num) => {
                let Some(lhs_num) = cell.as_number() else {
                    return matches!(op, CritNumOp::Ne);
                };
                match op {
                    CritNumOp::Ge => lhs_num >= rhs_num - 1e-9,
                    CritNumOp::Le => lhs_num <= rhs_num + 1e-9,
                    CritNumOp::Ne => (lhs_num - rhs_num).abs() >= 1e-9,
                    CritNumOp::Gt => lhs_num > rhs_num + 1e-9,
                    CritNumOp::Lt => lhs_num < rhs_num - 1e-9,
                    CritNumOp::Eq => (lhs_num - rhs_num).abs() < 1e-9,
                }
            }
            Self::Fallback(crit) => matches_criteria(cell, crit),
        }
    }
}

impl SheetEngine {
    fn eval_sumproduct_streaming(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if args.is_empty() {
            return Err(FormulaError::Value);
        }
        let Some(first_spec) = self.extract_streaming_range(&args[0])? else {
            let mut prod = 1.0f64;
            for arg in args {
                if self.extract_streaming_range(arg)?.is_some() {
                    return Err(FormulaError::Value);
                }
                prod *= self
                    .eval_arg_val(arg, ctx, depth)?
                    .as_number()
                    .unwrap_or(0.0);
            }
            return Ok(EvaluatedCellValue::Number(prod));
        };
        let (_, _, _, r_count, c_count, _) = first_spec;
        if args.len() == 2 {
            if let Some(second_spec) = self.extract_streaming_range(&args[1])? {
                if second_spec.3 != r_count || second_spec.4 != c_count {
                    return Err(FormulaError::Value);
                }
                let mut acc0 = 0.0f64;
                let mut acc1 = 0.0f64;
                let mut acc2 = 0.0f64;
                let mut acc3 = 0.0f64;
                let mut lane = 0usize;
                for dr in 0..r_count {
                    for dc in 0..c_count {
                        let a = self
                            .eval_streaming_cell(first_spec, dr, dc, ctx, depth)?
                            .as_number()
                            .unwrap_or(0.0);
                        let b = self
                            .eval_streaming_cell(second_spec, dr, dc, ctx, depth)?
                            .as_number()
                            .unwrap_or(0.0);
                        let p = a * b;
                        match lane & 3 {
                            0 => acc0 += p,
                            1 => acc1 += p,
                            2 => acc2 += p,
                            _ => acc3 += p,
                        }
                        lane += 1;
                    }
                }
                return Ok(EvaluatedCellValue::Number((acc0 + acc1) + (acc2 + acc3)));
            }
        }
        for arg in &args[1..] {
            let Some((_, _, _, r, c, _)) = self.extract_streaming_range(arg)? else {
                if r_count != 1 || c_count != 1 {
                    return Err(FormulaError::Value);
                }
                continue;
            };
            if r != r_count || c != c_count {
                return Err(FormulaError::Value);
            }
        }
        let mut sum = 0.0f64;
        for dr in 0..r_count {
            for dc in 0..c_count {
                let mut row_prod = 1.0f64;
                for arg in args {
                    let v = if let Some(sp) = self.extract_streaming_range(arg)? {
                        self.eval_streaming_cell(sp, dr, dc, ctx, depth)?
                    } else {
                        self.eval_arg_val(arg, ctx, depth)?
                    };
                    row_prod *= v.as_number().unwrap_or(0.0);
                }
                sum += row_prod;
            }
        }
        Ok(EvaluatedCellValue::Number(sum))
    }

    fn eval_single_criteria_streaming(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if name == "COUNTIF" {
            if args.len() != 2 {
                return Err(FormulaError::Value);
            }
            let Some(range_spec) = self.extract_streaming_range(&args[0])? else {
                let val = self.eval_arg_val(&args[0], ctx, depth)?;
                let crit = self.eval_arg_val(&args[1], ctx, depth)?;
                return Ok(EvaluatedCellValue::Number(
                    if matches_criteria(&val, &crit) {
                        1.0
                    } else {
                        0.0
                    },
                ));
            };
            let crit_val = self.eval_arg_val(&args[1], ctx, depth)?;
            let compiled_crit = CompiledCriteria::compile(&crit_val);
            let (_, _, _, r_count, c_count, _) = range_spec;
            let mut cnt = 0usize;
            for dr in 0..r_count {
                for dc in 0..c_count {
                    let val = self.eval_streaming_cell(range_spec, dr, dc, ctx, depth)?;
                    if compiled_crit.matches(&val) {
                        cnt += 1;
                    }
                }
            }
            return Ok(EvaluatedCellValue::Number(cnt as f64));
        }

        if args.len() < 2 || args.len() > 3 {
            return Err(FormulaError::Value);
        }
        let Some(range_spec) = self.extract_streaming_range(&args[0])? else {
            return Err(FormulaError::Value);
        };
        let crit_val = self.eval_arg_val(&args[1], ctx, depth)?;
        let compiled_crit = CompiledCriteria::compile(&crit_val);
        let (_, _, _, r_count, c_count, _) = range_spec;
        let target_spec = if args.len() == 3 {
            let Some(ts) = self.extract_streaming_range(&args[2])? else {
                return Err(FormulaError::Value);
            };
            if ts.3 != r_count || ts.4 != c_count {
                return Err(FormulaError::Value);
            }
            Some(ts)
        } else {
            None
        };

        let mut sum = 0.0f64;
        let mut count = 0usize;
        for dr in 0..r_count {
            for dc in 0..c_count {
                let val = self.eval_streaming_cell(range_spec, dr, dc, ctx, depth)?;
                if compiled_crit.matches(&val) {
                    let num_opt = if let Some(ts) = target_spec {
                        self.eval_streaming_cell(ts, dr, dc, ctx, depth)?
                            .as_number()
                    } else {
                        val.as_number()
                    };
                    if let Some(n) = num_opt {
                        sum += n;
                        count += 1;
                    }
                }
            }
        }

        if name == "AVERAGEIF" {
            if count == 0 {
                return Err(FormulaError::DivByZero);
            }
            Ok(EvaluatedCellValue::Number(sum / (count as f64)))
        } else {
            Ok(EvaluatedCellValue::Number(sum))
        }
    }

    fn eval_multi_criteria_streaming(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if name == "COUNTIFS" {
            if args.len() < 2 || !args.len().is_multiple_of(2) {
                return Err(FormulaError::Value);
            }
            if args.len() > 16 {
                let first_vals = self.eval_range_vec(&args[0], ctx, depth)?;
                let mask = self.eval_multi_criteria(args, first_vals.len(), ctx, depth)?;
                return Ok(EvaluatedCellValue::Number(
                    mask.into_iter().filter(|m| *m).count() as f64,
                ));
            }
            let Some(first_spec) = self.extract_streaming_range(&args[0])? else {
                return Err(FormulaError::Value);
            };
            let (_, _, _, r_count, c_count, _) = first_spec;
            let pair_count = args.len() / 2;
            let mut specs: [Option<StreamingRangeSpec<'_>>; 8] = [None; 8];
            let mut crit_vals: [EvaluatedCellValue; 8] =
                std::array::from_fn(|_| EvaluatedCellValue::Empty);
            for (idx, pair) in args.chunks(2).enumerate() {
                let Some(sp) = self.extract_streaming_range(&pair[0])? else {
                    return Err(FormulaError::Value);
                };
                if sp.3 != r_count || sp.4 != c_count {
                    return Err(FormulaError::Value);
                }
                specs[idx] = Some(sp);
                crit_vals[idx] = self.eval_arg_val(&pair[1], ctx, depth)?;
            }
            let compiled_crits: [CompiledCriteria<'_>; 8] =
                std::array::from_fn(|i| CompiledCriteria::compile(&crit_vals[i]));
            let mut cnt = 0usize;
            for dr in 0..r_count {
                'cell_loop: for dc in 0..c_count {
                    for idx in 0..pair_count {
                        let sp = specs[idx].unwrap();
                        let val = self.eval_streaming_cell(sp, dr, dc, ctx, depth)?;
                        if !compiled_crits[idx].matches(&val) {
                            continue 'cell_loop;
                        }
                    }
                    cnt += 1;
                }
            }
            return Ok(EvaluatedCellValue::Number(cnt as f64));
        }

        if args.len() < 3 || args.len().is_multiple_of(2) {
            return Err(FormulaError::Value);
        }
        let Some(target_spec) = self.extract_streaming_range(&args[0])? else {
            return Err(FormulaError::Value);
        };
        let (_, _, _, r_count, c_count, _) = target_spec;
        let crit_args = &args[1..];
        let pair_count = crit_args.len() / 2;
        if pair_count > 8 {
            let target_vals = self.eval_range_vec(&args[0], ctx, depth)?;
            let mask = self.eval_multi_criteria(crit_args, target_vals.len(), ctx, depth)?;
            let mut sum = 0.0f64;
            let mut count = 0usize;
            for (tv, m) in target_vals.into_iter().zip(mask) {
                if m {
                    if let Some(n) = tv.as_number() {
                        sum += n;
                        count += 1;
                    }
                }
            }
            return if name == "AVERAGEIFS" {
                if count == 0 {
                    Err(FormulaError::DivByZero)
                } else {
                    Ok(EvaluatedCellValue::Number(sum / (count as f64)))
                }
            } else {
                Ok(EvaluatedCellValue::Number(sum))
            };
        }
        let mut specs: [Option<StreamingRangeSpec<'_>>; 8] = [None; 8];
        let mut crit_vals: [EvaluatedCellValue; 8] =
            std::array::from_fn(|_| EvaluatedCellValue::Empty);
        for (idx, pair) in crit_args.chunks(2).enumerate() {
            let Some(sp) = self.extract_streaming_range(&pair[0])? else {
                return Err(FormulaError::Value);
            };
            if sp.3 != r_count || sp.4 != c_count {
                return Err(FormulaError::Value);
            }
            specs[idx] = Some(sp);
            crit_vals[idx] = self.eval_arg_val(&pair[1], ctx, depth)?;
        }
        let compiled_crits: [CompiledCriteria<'_>; 8] =
            std::array::from_fn(|i| CompiledCriteria::compile(&crit_vals[i]));

        let mut sum = 0.0f64;
        let mut count = 0usize;
        for dr in 0..r_count {
            'sumifs_cell: for dc in 0..c_count {
                for idx in 0..pair_count {
                    let sp = specs[idx].unwrap();
                    let val = self.eval_streaming_cell(sp, dr, dc, ctx, depth)?;
                    if !compiled_crits[idx].matches(&val) {
                        continue 'sumifs_cell;
                    }
                }
                let tv = self.eval_streaming_cell(target_spec, dr, dc, ctx, depth)?;
                if let Some(n) = tv.as_number() {
                    sum += n;
                    count += 1;
                }
            }
        }

        if name == "AVERAGEIFS" {
            if count == 0 {
                return Err(FormulaError::DivByZero);
            }
            Ok(EvaluatedCellValue::Number(sum / (count as f64)))
        } else {
            Ok(EvaluatedCellValue::Number(sum))
        }
    }
}

impl SheetEngine {
    fn eval_func_inner(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        match name {
            "SUM" | "PRODUCT" => self.eval_streaming_range_agg_ctx(name, args, ctx, depth),
            "SUMPRODUCT" => self.eval_sumproduct_streaming(args, ctx, depth),
            "SUMIF" => self.eval_single_criteria_streaming(name, args, ctx, depth),
            "SUMIFS" => self.eval_multi_criteria_streaming(name, args, ctx, depth),
            "ABS" => {
                if args.len() != 1 {
                    return Err(FormulaError::Value);
                }
                Ok(EvaluatedCellValue::Number(
                    self.eval_arg_num(&args[0], ctx, depth)?.abs(),
                ))
            }
            "SIGN" => {
                if args.len() != 1 {
                    return Err(FormulaError::Value);
                }
                let n = self.eval_arg_num(&args[0], ctx, depth)?;
                let s = if n > 0.0 {
                    1.0
                } else if n < 0.0 {
                    -1.0
                } else {
                    0.0
                };
                Ok(EvaluatedCellValue::Number(s))
            }
            "ROUND" | "ROUNDUP" | "ROUNDDOWN" | "TRUNC" => {
                if args.is_empty() || args.len() > 2 || (name != "TRUNC" && args.len() != 2) {
                    return Err(FormulaError::Value);
                }
                let num = self.eval_arg_num(&args[0], ctx, depth)?;
                let digits = if args.len() == 2 {
                    self.eval_arg_num(&args[1], ctx, depth)?
                } else {
                    0.0
                };
                let exp = digits.trunc().clamp(-300.0, 300.0) as i32;
                let factor = 10f64.powi(exp);
                if !factor.is_finite() || factor == 0.0 {
                    return Ok(EvaluatedCellValue::Number(num));
                }
                let scaled = num * factor;
                let rounded = match name {
                    "ROUND" => scaled.round(),
                    "ROUNDUP" => {
                        if scaled >= 0.0 {
                            scaled.ceil()
                        } else {
                            scaled.floor()
                        }
                    }
                    _ => scaled.trunc(),
                };
                Ok(EvaluatedCellValue::Number(rounded / factor))
            }
            "INT" => {
                if args.len() != 1 {
                    return Err(FormulaError::Value);
                }
                Ok(EvaluatedCellValue::Number(
                    self.eval_arg_num(&args[0], ctx, depth)?.floor(),
                ))
            }
            "CEILING" | "FLOOR" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(FormulaError::Value);
                }
                let num = self.eval_arg_num(&args[0], ctx, depth)?;
                let sig = if args.len() == 2 {
                    self.eval_arg_num(&args[1], ctx, depth)?
                } else if num < 0.0 {
                    -1.0
                } else {
                    1.0
                };
                if sig == 0.0 {
                    return if name == "FLOOR" && num != 0.0 {
                        Err(FormulaError::DivByZero)
                    } else {
                        Ok(EvaluatedCellValue::Number(0.0))
                    };
                }
                if (num > 0.0 && sig < 0.0) || (num < 0.0 && sig > 0.0 && name == "FLOOR") {
                    return Err(FormulaError::Num);
                }
                let q = num / sig;
                Ok(EvaluatedCellValue::Number(if name == "CEILING" {
                    q.ceil() * sig
                } else {
                    q.floor() * sig
                }))
            }
            "MOD" | "QUOTIENT" => {
                if args.len() != 2 {
                    return Err(FormulaError::Value);
                }
                let num = self.eval_arg_num(&args[0], ctx, depth)?;
                let div = self.eval_arg_num(&args[1], ctx, depth)?;
                if div.abs() < 1e-15 {
                    return Err(FormulaError::DivByZero);
                }
                Ok(EvaluatedCellValue::Number(if name == "MOD" {
                    num - div * (num / div).floor()
                } else {
                    (num / div).trunc()
                }))
            }
            "SQRT" => {
                if args.len() != 1 {
                    return Err(FormulaError::Value);
                }
                let n = self.eval_arg_num(&args[0], ctx, depth)?;
                if n < 0.0 {
                    return Err(FormulaError::Num);
                }
                Ok(EvaluatedCellValue::Number(n.sqrt()))
            }
            "POWER" => {
                if args.len() != 2 {
                    return Err(FormulaError::Value);
                }
                let res = self
                    .eval_arg_num(&args[0], ctx, depth)?
                    .powf(self.eval_arg_num(&args[1], ctx, depth)?);
                if !res.is_finite() {
                    return Err(FormulaError::Num);
                }
                Ok(EvaluatedCellValue::Number(res))
            }
            "EXP" | "LN" | "LOG10" | "SIN" | "COS" | "TAN" => {
                if args.len() != 1 {
                    return Err(FormulaError::Value);
                }
                let n = self.eval_arg_num(&args[0], ctx, depth)?;
                if (name == "LN" || name == "LOG10") && n <= 0.0 {
                    return Err(FormulaError::Num);
                }
                let res = match name {
                    "EXP" => n.exp(),
                    "LN" => n.ln(),
                    "LOG10" => n.log10(),
                    "SIN" => n.sin(),
                    "COS" => n.cos(),
                    _ => n.tan(),
                };
                if !res.is_finite() {
                    return Err(FormulaError::Num);
                }
                Ok(EvaluatedCellValue::Number(res))
            }
            "LOG" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(FormulaError::Value);
                }
                let n = self.eval_arg_num(&args[0], ctx, depth)?;
                let base = if args.len() == 2 {
                    self.eval_arg_num(&args[1], ctx, depth)?
                } else {
                    10.0
                };
                if n <= 0.0 || base <= 0.0 || (base - 1.0).abs() < 1e-15 {
                    return Err(FormulaError::Num);
                }
                Ok(EvaluatedCellValue::Number(n.log(base)))
            }
            "PI" | "RAND" => {
                if !args.is_empty() {
                    return Err(FormulaError::Value);
                }
                Ok(EvaluatedCellValue::Number(if name == "PI" {
                    std::f64::consts::PI
                } else {
                    0.5
                }))
            }

            // --- Statistical (22) ---
            "AVERAGE" | "AVG" | "MIN" | "MAX" | "COUNT" | "COUNTA" | "COUNTBLANK" => {
                self.eval_streaming_range_agg_ctx(name, args, ctx, depth)
            }
            "MEDIAN" | "MODE" | "STDEV" | "STDEVP" | "VAR" | "VARP" => {
                self.eval_stat_basic(name, args, ctx, depth)
            }
            "AVERAGEA" => {
                let vals = self.expand_args_ctx(args, ctx, depth)?;
                let mut sum = 0.0;
                let mut count = 0usize;
                for v in vals {
                    match v {
                        EvaluatedCellValue::Empty => {}
                        EvaluatedCellValue::Number(n) => {
                            sum += n;
                            count += 1;
                        }
                        EvaluatedCellValue::Boolean(b) => {
                            sum += if b { 1.0 } else { 0.0 };
                            count += 1;
                        }
                        EvaluatedCellValue::Text(_) => count += 1,
                        EvaluatedCellValue::Error(e) => return Err(e),
                    }
                }
                if count == 0 {
                    return Err(FormulaError::DivByZero);
                }
                Ok(EvaluatedCellValue::Number(sum / (count as f64)))
            }
            "AVERAGEIF" => self.eval_single_criteria_streaming(name, args, ctx, depth),
            "AVERAGEIFS" => self.eval_multi_criteria_streaming(name, args, ctx, depth),
            "SMALL" | "LARGE" | "RANK" | "PERCENTILE" | "QUARTILE" => {
                self.eval_stat_order(name, args, ctx, depth)
            }
            "COUNTIF" => self.eval_single_criteria_streaming(name, args, ctx, depth),
            "COUNTIFS" => self.eval_multi_criteria_streaming(name, args, ctx, depth),

            // --- Logical (10 + FALSE) ---
            "IF" | "IFS" | "AND" | "OR" | "XOR" | "NOT" | "IFERROR" | "IFNA" | "SWITCH"
            | "TRUE" | "FALSE" => self.eval_logical_func(name, args, ctx, depth),

            // --- Lookup & Reference (12 + INDIRECT) ---
            "VLOOKUP" | "HLOOKUP" | "XLOOKUP" | "INDEX" | "MATCH" | "CHOOSE" | "OFFSET" | "ROW"
            | "COLUMN" | "ROWS" | "COLUMNS" | "TRANSPOSE" | "INDIRECT" => {
                self.eval_lookup_func(name, args, ctx, depth)
            }

            // --- Text (21) ---
            "LEN" | "TRIM" | "UPPER" | "LOWER" | "PROPER" | "VALUE" | "CHAR" | "CODE"
            | "CONCAT" | "CONCATENATE" | "TEXTJOIN" | "LEFT" | "RIGHT" | "MID" | "SUBSTITUTE"
            | "REPLACE" | "FIND" | "SEARCH" | "EXACT" | "REPT" | "TEXT" => {
                self.eval_text_func(name, args, ctx, depth)
            }

            // --- Date & Time (10) ---
            "DATE" | "YEAR" | "MONTH" | "DAY" | "TIME" | "HOUR" | "MINUTE" | "SECOND" | "DAYS"
            | "EDATE" => self.eval_date_func(name, args, ctx, depth),

            // --- Financial (11 + DB) ---
            "PMT" | "IPMT" | "PPMT" | "FV" | "PV" | "NPV" | "IRR" | "RATE" | "NPER" | "SLN"
            | "DB" | "DDB" => self.eval_financial_func(name, args, ctx, depth),

            // --- Information (9 + TYPE) ---
            "ISBLANK" | "ISNUMBER" | "ISTEXT" | "ISLOGICAL" | "ISERR" | "ISERROR" | "ISNA"
            | "ISEVEN" | "ISODD" | "TYPE" => self.eval_info_func(name, args, ctx, depth),

            _ => Err(FormulaError::Name),
        }
    }

    fn eval_stat_basic(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        let vals = self.expand_args_ctx(args, ctx, depth)?;
        let mut nums: Vec<f64> = vals
            .iter()
            .filter_map(EvaluatedCellValue::as_number)
            .collect();
        match name {
            "AVERAGE" => {
                if nums.is_empty() {
                    return Err(FormulaError::DivByZero);
                }
                Ok(EvaluatedCellValue::Number(
                    nums.iter().sum::<f64>() / (nums.len() as f64),
                ))
            }
            "MEDIAN" => {
                if nums.is_empty() {
                    return Err(FormulaError::Num);
                }
                nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let mid = nums.len() / 2;
                let med = if nums.len() % 2 == 1 {
                    nums[mid]
                } else {
                    (nums[mid - 1] + nums[mid]) / 2.0
                };
                Ok(EvaluatedCellValue::Number(med))
            }
            "MODE" => {
                if nums.is_empty() {
                    return Err(FormulaError::Na);
                }
                let mut best_val = nums[0];
                let mut best_cnt = 0usize;
                for &x in &nums {
                    let cnt = nums.iter().filter(|&&y| (x - y).abs() < 1e-12).count();
                    if cnt > best_cnt {
                        best_cnt = cnt;
                        best_val = x;
                    }
                }
                if best_cnt < 2 {
                    return Err(FormulaError::Na);
                }
                Ok(EvaluatedCellValue::Number(best_val))
            }
            "MIN" => Ok(EvaluatedCellValue::Number(
                nums.into_iter().reduce(f64::min).unwrap_or(0.0),
            )),
            "MAX" => Ok(EvaluatedCellValue::Number(
                nums.into_iter().reduce(f64::max).unwrap_or(0.0),
            )),
            _ => {
                let is_sample = name == "STDEV" || name == "VAR";
                if nums.is_empty() || (is_sample && nums.len() < 2) {
                    return Err(FormulaError::DivByZero);
                }
                let mean = nums.iter().sum::<f64>() / (nums.len() as f64);
                let denom = if is_sample {
                    (nums.len() - 1) as f64
                } else {
                    nums.len() as f64
                };
                let var = nums.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / denom;
                Ok(EvaluatedCellValue::Number(if name.starts_with("STDEV") {
                    var.sqrt()
                } else {
                    var
                }))
            }
        }
    }

    fn eval_stat_order(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        match name {
            "SMALL" | "LARGE" => {
                if args.len() != 2 {
                    return Err(FormulaError::Value);
                }
                let vals = self.eval_range_vec(&args[0], ctx, depth)?;
                let k = self.eval_arg_num(&args[1], ctx, depth)?.trunc() as isize;
                let mut nums: Vec<f64> = vals
                    .iter()
                    .filter_map(EvaluatedCellValue::as_number)
                    .collect();
                if k < 1 || (k as usize) > nums.len() {
                    return Err(FormulaError::Num);
                }
                nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let idx = if name == "SMALL" {
                    (k as usize) - 1
                } else {
                    nums.len() - (k as usize)
                };
                Ok(EvaluatedCellValue::Number(nums[idx]))
            }
            "RANK" => {
                if args.len() < 2 || args.len() > 3 {
                    return Err(FormulaError::Value);
                }
                let target = self.eval_arg_num(&args[0], ctx, depth)?;
                let vals = self.eval_range_vec(&args[1], ctx, depth)?;
                let order = if args.len() == 3 {
                    self.eval_arg_num(&args[2], ctx, depth)?.trunc() as i32
                } else {
                    0
                };
                let nums: Vec<f64> = vals
                    .iter()
                    .filter_map(EvaluatedCellValue::as_number)
                    .collect();
                if !nums.iter().any(|&x| (x - target).abs() < 1e-12) {
                    return Err(FormulaError::Na);
                }
                let better = if order == 0 {
                    nums.iter().filter(|&&x| x > target + 1e-12).count()
                } else {
                    nums.iter().filter(|&&x| x < target - 1e-12).count()
                };
                Ok(EvaluatedCellValue::Number((better + 1) as f64))
            }
            _ => {
                if args.len() != 2 {
                    return Err(FormulaError::Value);
                }
                let vals = self.eval_range_vec(&args[0], ctx, depth)?;
                let param = self.eval_arg_num(&args[1], ctx, depth)?;
                let p = if name == "QUARTILE" {
                    let q = param.trunc() as i32;
                    if !(0..=4).contains(&q) {
                        return Err(FormulaError::Num);
                    }
                    (q as f64) * 0.25
                } else if (0.0..=1.0).contains(&param) {
                    param
                } else {
                    return Err(FormulaError::Num);
                };
                let mut nums: Vec<f64> = vals
                    .iter()
                    .filter_map(EvaluatedCellValue::as_number)
                    .collect();
                if nums.is_empty() {
                    return Err(FormulaError::Num);
                }
                nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let pos = p * ((nums.len() - 1) as f64);
                let lo = pos.floor() as usize;
                let hi = pos.ceil() as usize;
                Ok(EvaluatedCellValue::Number(
                    nums[lo] + (pos - lo as f64) * (nums[hi] - nums[lo]),
                ))
            }
        }
    }
}

#[path = "range_refs.rs"]
pub(crate) mod range_refs;

use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};

/// Canonical 122 built-in spreadsheet functions across 8 standard categories:
/// Math & Trig (27), Statistical (22), Logical (10), Lookup & Reference (12),
/// Text (21), Date & Time (10), Financial (11), and Information (9).
pub const CANONICAL_122_FUNCTIONS: [&str; 122] = [
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

/// The numeric values among `vals`.
fn numeric_values(vals: &[EvaluatedCellValue]) -> Vec<f64> {
    vals.iter()
        .filter_map(|v| match v {
            EvaluatedCellValue::Number(n) => Some(*n),
            _ => None,
        })
        .collect()
}

/// MODE: most frequent value (ties go to the earliest); `#N/A` when nothing repeats.
fn mode_of(nums: &[f64]) -> Result<EvaluatedCellValue, FormulaError> {
    if nums.is_empty() {
        return Err(FormulaError::Na);
    }
    let mut counts: std::collections::HashMap<u64, (usize, usize, f64)> =
        std::collections::HashMap::with_capacity(nums.len());
    for (idx, &x) in nums.iter().enumerate() {
        let key = if x == 0.0 {
            0.0f64.to_bits()
        } else {
            x.to_bits()
        };
        counts.entry(key).or_insert((0, idx, x)).0 += 1;
    }
    let mut best_val = nums[0];
    let mut best_cnt = 0usize;
    let mut best_idx = usize::MAX;
    for &(cnt, first_idx, val) in counts.values() {
        if cnt > best_cnt || (cnt == best_cnt && first_idx < best_idx) {
            best_cnt = cnt;
            best_idx = first_idx;
            best_val = val;
        }
    }
    if best_cnt < 2 {
        return Err(FormulaError::Na);
    }
    Ok(EvaluatedCellValue::Number(best_val))
}

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
        let val = self.eval_expr_ctx(arg, ctx, depth);
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
        let val = self.eval_expr_ctx(arg, ctx, depth);
        if let EvaluatedCellValue::Error(e) = val {
            return Err(e);
        }
        Ok(val)
    }

    fn eval_func_inner(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        match name {
            "SUM" | "PRODUCT" => self.eval_streaming_range_agg_ctx(name, args, ctx, depth),
            "SUBTOTAL" => self.eval_subtotal(args, ctx, depth),
            "SUMPRODUCT" => self.eval_sumproduct_streaming(args, ctx, depth),
            "SUMIF" => self.eval_single_criteria_streaming(name, args, ctx, depth),
            "SUMIFS" | "MINIFS" | "MAXIFS" => {
                self.eval_multi_criteria_streaming(name, args, ctx, depth)
            }
            "ABS" | "SIGN" | "ROUND" | "ROUNDUP" | "ROUNDDOWN" | "TRUNC" | "MROUND" | "FACT"
            | "RANDBETWEEN" | "INT" | "CEILING" | "FLOOR" | "MOD" | "QUOTIENT" | "SQRT"
            | "POWER" | "EXP" | "LN" | "LOG10" | "SIN" | "COS" | "TAN" | "LOG" | "PI" | "RAND"
            | "NA" | "N" | "T" | "AVERAGEA" => self.eval_math_func(name, args, ctx, depth),
            "AVERAGE" | "AVG" | "MIN" | "MAX" | "COUNT" | "COUNTA" | "COUNTBLANK" => {
                self.eval_streaming_range_agg_ctx(name, args, ctx, depth)
            }
            "STDEV" | "STDEVP" | "VAR" | "VARP" | "STDEV.S" | "STDEV.P" | "VAR.S" | "VAR.P" => {
                self.eval_welford_variance_streaming(name, args, ctx, depth)
            }
            "MEDIAN" | "MODE" | "MODE.SNGL" => self.eval_stat_basic(name, args, ctx, depth),
            "AVERAGEIF" => self.eval_single_criteria_streaming(name, args, ctx, depth),
            "AVERAGEIFS" => self.eval_multi_criteria_streaming(name, args, ctx, depth),
            "SMALL" | "LARGE" | "RANK" | "PERCENTILE" | "QUARTILE" | "PERCENTILE.INC"
            | "QUARTILE.INC" => self.eval_stat_order(name, args, ctx, depth),
            "COUNTIF" => self.eval_single_criteria_streaming(name, args, ctx, depth),
            "COUNTIFS" => self.eval_multi_criteria_streaming(name, args, ctx, depth),
            "IF" | "IFS" | "AND" | "OR" | "XOR" | "NOT" | "IFERROR" | "IFNA" | "SWITCH"
            | "TRUE" | "FALSE" => self.eval_logical_func(name, args, ctx, depth),
            "VLOOKUP" | "HLOOKUP" | "XLOOKUP" | "INDEX" | "MATCH" | "CHOOSE" | "OFFSET" | "ROW"
            | "COLUMN" | "ROWS" | "COLUMNS" | "TRANSPOSE" | "INDIRECT" => {
                self.eval_lookup_func(name, args, ctx, depth)
            }
            "LEN" | "TRIM" | "UPPER" | "LOWER" | "PROPER" | "VALUE" | "CHAR" | "CODE"
            | "CONCAT" | "CONCATENATE" | "TEXTJOIN" | "LEFT" | "RIGHT" | "MID" | "SUBSTITUTE"
            | "REPLACE" | "FIND" | "SEARCH" | "EXACT" | "REPT" | "TEXT" | "FIXED" | "CLEAN" => {
                self.eval_text_func(name, args, ctx, depth)
            }
            "DATE" | "YEAR" | "MONTH" | "DAY" | "TIME" | "HOUR" | "MINUTE" | "SECOND" | "DAYS"
            | "EDATE" | "EOMONTH" | "DATEDIF" | "TODAY" | "NOW" | "WEEKDAY" | "DATEVALUE"
            | "WORKDAY" | "NETWORKDAYS" => self.eval_date_func(name, args, ctx, depth),
            "PMT" | "IPMT" | "PPMT" | "FV" | "PV" | "NPV" | "IRR" | "RATE" | "NPER" | "SLN"
            | "DB" | "DDB" | "SYD" | "EFFECT" | "NOMINAL" => {
                self.eval_financial_func(name, args, ctx, depth)
            }
            "ISBLANK" | "ISNUMBER" | "ISTEXT" | "ISLOGICAL" | "ISERR" | "ISERROR" | "ISNA"
            | "ISEVEN" | "ISODD" | "TYPE" => self.eval_info_func(name, args, ctx, depth),
            _ => Err(FormulaError::Name),
        }
    }

    fn eval_welford_variance_streaming(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        let mut count = 0usize;
        let mut mean = 0.0f64;
        let mut m2 = 0.0f64;
        self.visit_args_streaming(args, ctx, depth, |v| {
            if let EvaluatedCellValue::Number(x) = v {
                count += 1;
                let delta = x - mean;
                mean += delta / (count as f64);
                let delta2 = x - mean;
                m2 += delta * delta2;
            }
            Ok(())
        })?;
        let is_sample = matches!(name, "STDEV" | "VAR" | "STDEV.S" | "VAR.S");
        if count == 0 || (is_sample && count < 2) {
            return Err(FormulaError::DivByZero);
        }
        let denom = if is_sample {
            (count - 1) as f64
        } else {
            count as f64
        };
        let var = m2 / denom;
        Ok(EvaluatedCellValue::Number(if name.starts_with("STDEV") {
            var.sqrt()
        } else {
            var
        }))
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
            .filter_map(|v| match v {
                EvaluatedCellValue::Number(n) => Some(*n),
                _ => None,
            })
            .collect();
        match name {
            "MEDIAN" => {
                if nums.is_empty() {
                    return Err(FormulaError::Num);
                }
                nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let mid = nums.len() / 2;
                let med = if (nums.len() & 1) == 1 {
                    nums[mid]
                } else {
                    (nums[mid - 1] + nums[mid]) / 2.0
                };
                Ok(EvaluatedCellValue::Number(med))
            }
            "MODE" | "MODE.SNGL" => mode_of(&nums),
            _ => Err(FormulaError::Name),
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
            "SMALL" | "LARGE" => self.stat_small_large(name == "SMALL", args, ctx, depth),
            "RANK" => self.stat_rank(args, ctx, depth),
            _ => self.stat_percentile(name, args, ctx, depth),
        }
    }

    fn stat_small_large(
        &self,
        is_small: bool,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if args.len() != 2 {
            return Err(FormulaError::Value);
        }
        let vals = self.eval_range_vec(&args[0], ctx, depth)?;
        let k = self.eval_arg_num(&args[1], ctx, depth)?.trunc() as isize;
        let mut nums = numeric_values(&vals);
        if k < 1 || (k as usize) > nums.len() {
            return Err(FormulaError::Num);
        }
        nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let idx = if is_small {
            (k as usize) - 1
        } else {
            nums.len() - (k as usize)
        };
        Ok(EvaluatedCellValue::Number(nums[idx]))
    }

    fn stat_rank(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if args.len() < 2 || args.len() > 3 {
            return Err(FormulaError::Value);
        }
        let target = self.eval_arg_num(&args[0], ctx, depth)?;
        let order = match args.get(2) {
            Some(arg) => self.eval_arg_num(arg, ctx, depth)?.trunc() as i32,
            None => 0,
        };
        let mut found = false;
        let mut better_count = 0usize;
        self.visit_args_streaming(std::slice::from_ref(&args[1]), ctx, depth, |v| {
            if let EvaluatedCellValue::Number(x) = v {
                if x == target {
                    found = true;
                } else if (order == 0 && x > target) || (order != 0 && x < target) {
                    better_count += 1;
                }
            }
            Ok(())
        })?;
        if !found {
            return Err(FormulaError::Na);
        }
        Ok(EvaluatedCellValue::Number((better_count + 1) as f64))
    }

    fn stat_percentile(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if args.len() != 2 {
            return Err(FormulaError::Value);
        }
        let vals = self.eval_range_vec(&args[0], ctx, depth)?;
        let param = self.eval_arg_num(&args[1], ctx, depth)?;
        let p = if name == "QUARTILE" || name == "QUARTILE.INC" {
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
        let mut nums = numeric_values(&vals);
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

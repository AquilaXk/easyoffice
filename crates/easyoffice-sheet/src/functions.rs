#[path = "range_refs.rs"]
mod range_refs;

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
                    "ROUND" => {
                        if exp > 0 {
                            let nudge = scaled.abs() * f64::EPSILON * 2.0;
                            (scaled + nudge.copysign(scaled)).round()
                        } else {
                            scaled.round()
                        }
                    }
                    "ROUNDUP" => {
                        if num == 0.0 {
                            0.0
                        } else {
                            let abs_s = scaled.abs();
                            let snapped = if exp > 0
                                && (abs_s - abs_s.round()).abs() <= abs_s * f64::EPSILON * 4.0
                            {
                                abs_s.round()
                            } else {
                                abs_s.ceil()
                            };
                            snapped.copysign(num)
                        }
                    }
                    _ => {
                        if num == 0.0 {
                            0.0
                        } else {
                            let abs_s = scaled.abs();
                            let snapped = if exp > 0
                                && (abs_s.round() - abs_s).abs() <= abs_s * f64::EPSILON * 4.0
                            {
                                abs_s.round()
                            } else {
                                abs_s.floor()
                            };
                            snapped.copysign(num)
                        }
                    }
                };
                let res = rounded / factor;
                Ok(EvaluatedCellValue::Number(if res == 0.0 {
                    0.0
                } else {
                    res
                }))
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
                let q_adj = if (q - q.round()).abs() <= q.abs() * f64::EPSILON * 4.0 {
                    q.round()
                } else {
                    q
                };
                Ok(EvaluatedCellValue::Number(if name == "CEILING" {
                    q_adj.ceil() * sig
                } else {
                    q_adj.floor() * sig
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
            "AVERAGE" | "AVG" | "MIN" | "MAX" | "COUNT" | "COUNTA" | "COUNTBLANK" => {
                self.eval_streaming_range_agg_ctx(name, args, ctx, depth)
            }
            "STDEV" | "STDEVP" | "VAR" | "VARP" => {
                self.eval_welford_variance_streaming(name, args, ctx, depth)
            }
            "MEDIAN" | "MODE" => self.eval_stat_basic(name, args, ctx, depth),
            "AVERAGEA" => {
                let mut sum = 0.0;
                let mut count = 0usize;
                self.visit_args_streaming(args, ctx, depth, |v| {
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
                    Ok(())
                })?;
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
            "IF" | "IFS" | "AND" | "OR" | "XOR" | "NOT" | "IFERROR" | "IFNA" | "SWITCH"
            | "TRUE" | "FALSE" => self.eval_logical_func(name, args, ctx, depth),
            "VLOOKUP" | "HLOOKUP" | "XLOOKUP" | "INDEX" | "MATCH" | "CHOOSE" | "OFFSET" | "ROW"
            | "COLUMN" | "ROWS" | "COLUMNS" | "TRANSPOSE" | "INDIRECT" => {
                self.eval_lookup_func(name, args, ctx, depth)
            }
            "LEN" | "TRIM" | "UPPER" | "LOWER" | "PROPER" | "VALUE" | "CHAR" | "CODE"
            | "CONCAT" | "CONCATENATE" | "TEXTJOIN" | "LEFT" | "RIGHT" | "MID" | "SUBSTITUTE"
            | "REPLACE" | "FIND" | "SEARCH" | "EXACT" | "REPT" | "TEXT" => {
                self.eval_text_func(name, args, ctx, depth)
            }
            "DATE" | "YEAR" | "MONTH" | "DAY" | "TIME" | "HOUR" | "MINUTE" | "SECOND" | "DAYS"
            | "EDATE" => self.eval_date_func(name, args, ctx, depth),
            "PMT" | "IPMT" | "PPMT" | "FV" | "PV" | "NPV" | "IRR" | "RATE" | "NPER" | "SLN"
            | "DB" | "DDB" => self.eval_financial_func(name, args, ctx, depth),
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
        let is_sample = name == "STDEV" || name == "VAR";
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
            "MODE" => {
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
                    let entry = counts.entry(key).or_insert((0, idx, x));
                    entry.0 += 1;
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
            "SMALL" | "LARGE" => {
                if args.len() != 2 {
                    return Err(FormulaError::Value);
                }
                let vals = self.eval_range_vec(&args[0], ctx, depth)?;
                let k = self.eval_arg_num(&args[1], ctx, depth)?.trunc() as isize;
                let mut nums: Vec<f64> = vals
                    .iter()
                    .filter_map(|v| match v {
                        EvaluatedCellValue::Number(n) => Some(*n),
                        _ => None,
                    })
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
                let order = if args.len() == 3 {
                    self.eval_arg_num(&args[2], ctx, depth)?.trunc() as i32
                } else {
                    0
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
                    .filter_map(|v| match v {
                        EvaluatedCellValue::Number(n) => Some(*n),
                        _ => None,
                    })
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

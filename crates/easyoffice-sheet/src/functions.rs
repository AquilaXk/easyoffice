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

#[inline]
fn cmp_f64(a: &f64, b: &f64) -> std::cmp::Ordering {
    a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
}

/// Selects the `lo`-th and `hi`-th ($lo \le hi \le lo + 1$) smallest numbers in $O(N)$ time.
fn select_pair(nums: &mut [f64], lo: usize, hi: usize) -> (f64, f64) {
    let (_, &mut lo_val, upper) = nums.select_nth_unstable_by(lo, cmp_f64);
    let hi_val = if hi == lo {
        lo_val
    } else {
        *upper.iter().min_by(|a, b| cmp_f64(a, b)).unwrap_or(&lo_val)
    };
    (lo_val, hi_val)
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

fn stat_skew_kurt(is_kurt: bool, nums: &[f64]) -> Result<EvaluatedCellValue, FormulaError> {
    let n = nums.len();
    if n < if is_kurt { 4 } else { 3 } {
        return Err(FormulaError::DivByZero);
    }
    let nf = n as f64;
    let mean = nums.iter().sum::<f64>() / nf;
    let m2: f64 = nums.iter().map(|&x| (x - mean).powi(2)).sum();
    let s = (m2 / (nf - 1.0)).sqrt();
    if s == 0.0 {
        return Err(FormulaError::DivByZero);
    }
    if is_kurt {
        let m4: f64 = nums.iter().map(|&x| ((x - mean) / s).powi(4)).sum();
        let c1 = (nf * (nf + 1.0)) / ((nf - 1.0) * (nf - 2.0) * (nf - 3.0));
        let c2 = (3.0 * (nf - 1.0).powi(2)) / ((nf - 2.0) * (nf - 3.0));
        Ok(EvaluatedCellValue::Number(c1 * m4 - c2))
    } else {
        let m3: f64 = nums.iter().map(|&x| ((x - mean) / s).powi(3)).sum();
        Ok(EvaluatedCellValue::Number(
            (nf / ((nf - 1.0) * (nf - 2.0))) * m3,
        ))
    }
}

fn stat_skew_p(nums: &[f64]) -> Result<EvaluatedCellValue, FormulaError> {
    let n = nums.len();
    if n < 3 {
        return Err(FormulaError::DivByZero);
    }
    let nf = n as f64;
    let mean = nums.iter().sum::<f64>() / nf;
    let m2 = nums.iter().map(|&x| (x - mean).powi(2)).sum::<f64>() / nf;
    if m2 == 0.0 {
        return Err(FormulaError::DivByZero);
    }
    let m3 = nums.iter().map(|&x| (x - mean).powi(3)).sum::<f64>() / nf;
    Ok(EvaluatedCellValue::Number(m3 / (m2 * m2.sqrt())))
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
            | "NA" | "N" | "T" | "AVERAGEA" | "SUMSQ" | "GCD" | "LCM" | "COMBIN" | "COMBINA"
            | "PERMUT" | "PERMUTATIONA" | "EVEN" | "ODD" | "FACTDOUBLE" | "SQRTPI" | "DEGREES"
            | "RADIANS" | "ASIN" | "ACOS" | "ATAN" | "ATAN2" | "SINH" | "COSH" | "TANH"
            | "BASE" | "DECIMAL" | "ROMAN" | "ARABIC" => {
                self.eval_math_func(name, args, ctx, depth)
            }
            "AVERAGE" | "AVG" | "MIN" | "MAX" | "COUNT" | "COUNTA" | "COUNTBLANK" => {
                self.eval_streaming_range_agg_ctx(name, args, ctx, depth)
            }
            "STDEV" | "STDEVP" | "VAR" | "VARP" | "STDEV.S" | "STDEV.P" | "VAR.S" | "VAR.P"
            | "DEVSQ" => self.eval_welford_variance_streaming(name, args, ctx, depth),
            "MEDIAN" | "MODE" | "MODE.SNGL" | "AVEDEV" | "GEOMEAN" | "HARMEAN" | "SKEW"
            | "SKEW.P" | "KURT" => self.eval_stat_basic(name, args, ctx, depth),
            "CORREL" | "PEARSON" | "COVARIANCE.P" | "COVARIANCE.S" | "SLOPE" | "INTERCEPT"
            | "RSQ" | "STEYX" | "FORECAST" | "FORECAST.LINEAR" => {
                self.eval_stat_regression(name, args, ctx, depth)
            }
            "AVERAGEIF" => self.eval_single_criteria_streaming(name, args, ctx, depth),
            "AVERAGEIFS" => self.eval_multi_criteria_streaming(name, args, ctx, depth),
            "SMALL" | "LARGE" | "RANK" | "RANK.EQ" | "RANK.AVG" | "PERCENTILE" | "QUARTILE"
            | "PERCENTILE.INC" | "QUARTILE.INC" | "PERCENTILE.EXC" | "QUARTILE.EXC"
            | "PERCENTRANK" | "PERCENTRANK.INC" | "PERCENTRANK.EXC" | "TRIMMEAN" => {
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
            | "REPLACE" | "FIND" | "SEARCH" | "EXACT" | "REPT" | "TEXT" | "FIXED" | "DOLLAR"
            | "CLEAN" | "TEXTBEFORE" | "TEXTAFTER" | "NUMBERVALUE" | "VALUETOTEXT" | "UNICODE"
            | "UNICHAR" => self.eval_text_func(name, args, ctx, depth),
            "DATE" | "YEAR" | "MONTH" | "DAY" | "TIME" | "HOUR" | "MINUTE" | "SECOND" | "DAYS"
            | "DAYS360" | "EDATE" | "EOMONTH" | "DATEDIF" | "TODAY" | "NOW" | "WEEKDAY"
            | "DATEVALUE" | "WORKDAY" | "WORKDAY.INTL" | "NETWORKDAYS" | "NETWORKDAYS.INTL"
            | "TIMEVALUE" | "WEEKNUM" | "ISOWEEKNUM" | "YEARFRAC" => {
                self.eval_date_func(name, args, ctx, depth)
            }
            "PMT" | "IPMT" | "PPMT" | "CUMIPMT" | "CUMPRINC" | "FV" | "PV" | "NPV" | "XNPV"
            | "IRR" | "XIRR" | "MIRR" | "RATE" | "NPER" | "SLN" | "DB" | "DDB" | "SYD"
            | "EFFECT" | "NOMINAL" | "PDURATION" | "RRI" | "DOLLARDE" | "DOLLARFR" => {
                self.eval_financial_func(name, args, ctx, depth)
            }
            "BITAND" | "BITOR" | "BITXOR" | "BITLSHIFT" | "BITRSHIFT" | "HEX2DEC" | "DEC2HEX"
            | "BIN2DEC" | "DEC2BIN" | "OCT2DEC" | "DEC2OCT" | "BIN2HEX" | "BIN2OCT" | "OCT2BIN"
            | "OCT2HEX" | "HEX2BIN" | "HEX2OCT" | "MULTINOMIAL" | "SERIESSUM" | "SUMX2MY2"
            | "SUMX2PY2" | "SUMXMY2" | "STANDARDIZE" | "FISHER" | "FISHERINV" => {
                self.eval_eng_func(name, args, ctx, depth)
            }
            "ISBLANK" | "ISNUMBER" | "ISTEXT" | "ISLOGICAL" | "ISERR" | "ISERROR" | "ISNA"
            | "ISEVEN" | "ISODD" | "TYPE" | "ERROR.TYPE" => {
                self.eval_info_func(name, args, ctx, depth)
            }
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
        if name == "DEVSQ" {
            return if count == 0 {
                Err(FormulaError::Num)
            } else {
                Ok(EvaluatedCellValue::Number(m2))
            };
        }
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
        let mut nums = numeric_values(&vals);
        match name {
            "MEDIAN" => {
                if nums.is_empty() {
                    return Err(FormulaError::Num);
                }
                let mid = nums.len() / 2;
                let med = if (nums.len() & 1) == 1 {
                    let (_, &mut m, _) = nums.select_nth_unstable_by(mid, cmp_f64);
                    m
                } else {
                    let (a, b) = select_pair(&mut nums, mid - 1, mid);
                    (a + b) * 0.5
                };
                Ok(EvaluatedCellValue::Number(med))
            }
            "MODE" | "MODE.SNGL" => mode_of(&nums),
            "AVEDEV" => {
                if nums.is_empty() {
                    return Err(FormulaError::Num);
                }
                let nf = nums.len() as f64;
                let mean = nums.iter().sum::<f64>() / nf;
                let sum_dev: f64 = nums.iter().map(|&x| (x - mean).abs()).sum();
                Ok(EvaluatedCellValue::Number(sum_dev / nf))
            }
            "GEOMEAN" => {
                if nums.is_empty() || nums.iter().any(|&x| x <= 0.0) {
                    return Err(FormulaError::Num);
                }
                let log_sum: f64 = nums.iter().map(|&x| x.ln()).sum();
                Ok(EvaluatedCellValue::Number(
                    (log_sum / (nums.len() as f64)).exp(),
                ))
            }
            "HARMEAN" => {
                if nums.is_empty() || nums.iter().any(|&x| x <= 0.0) {
                    return Err(FormulaError::Num);
                }
                let inv_sum: f64 = nums.iter().map(|&x| 1.0 / x).sum();
                Ok(EvaluatedCellValue::Number((nums.len() as f64) / inv_sum))
            }
            "SKEW" | "KURT" => stat_skew_kurt(name == "KURT", &nums),
            "SKEW.P" => stat_skew_p(&nums),
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
            "RANK" | "RANK.EQ" | "RANK.AVG" => self.stat_rank(name == "RANK.AVG", args, ctx, depth),
            "TRIMMEAN" => self.stat_trimmean(args, ctx, depth),
            "PERCENTRANK" | "PERCENTRANK.INC" | "PERCENTRANK.EXC" => {
                self.stat_percentrank(name == "PERCENTRANK.EXC", args, ctx, depth)
            }
            _ => self.stat_percentile(name, args, ctx, depth),
        }
    }

    fn stat_trimmean(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if args.len() != 2 {
            return Err(FormulaError::Value);
        }
        let vals = self.eval_range_vec(&args[0], ctx, depth)?;
        let pct = self.eval_arg_num(&args[1], ctx, depth)?;
        let mut nums = numeric_values(&vals);
        if nums.is_empty() || !(0.0..1.0).contains(&pct) {
            return Err(FormulaError::Num);
        }
        let g = ((nums.len() as f64 * pct).floor() as usize) / 2;
        if g * 2 >= nums.len() {
            return Err(FormulaError::Num);
        }
        nums.sort_unstable_by(cmp_f64);
        let slice = &nums[g..nums.len() - g];
        Ok(EvaluatedCellValue::Number(
            slice.iter().sum::<f64>() / (slice.len() as f64),
        ))
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
        let idx = if is_small {
            (k as usize) - 1
        } else {
            nums.len() - (k as usize)
        };
        let (_, &mut val, _) = nums.select_nth_unstable_by(idx, cmp_f64);
        Ok(EvaluatedCellValue::Number(val))
    }

    fn stat_rank(
        &self,
        is_avg: bool,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if !(2..=3).contains(&args.len()) {
            return Err(FormulaError::Value);
        }
        let target = self.eval_arg_num(&args[0], ctx, depth)?;
        let order = match args.get(2) {
            Some(arg) => self.eval_arg_num(arg, ctx, depth)?.trunc() as i32,
            None => 0,
        };
        let mut equal_count = 0usize;
        let mut better_count = 0usize;
        self.visit_args_streaming(std::slice::from_ref(&args[1]), ctx, depth, |v| {
            if let EvaluatedCellValue::Number(x) = v {
                if x == target {
                    equal_count += 1;
                } else if (order == 0 && x > target) || (order != 0 && x < target) {
                    better_count += 1;
                }
            }
            Ok(())
        })?;
        if equal_count == 0 {
            return Err(FormulaError::Na);
        }
        let rank = if is_avg {
            (better_count as f64) + ((equal_count + 1) as f64) * 0.5
        } else {
            (better_count + 1) as f64
        };
        Ok(EvaluatedCellValue::Number(rank))
    }

    fn stat_percentrank(
        &self,
        is_exc: bool,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if !(2..=3).contains(&args.len()) {
            return Err(FormulaError::Value);
        }
        let vals = self.eval_range_vec(&args[0], ctx, depth)?;
        for v in &vals {
            if let EvaluatedCellValue::Error(e) = v {
                return Err(*e);
            }
        }
        let x = self.eval_arg_num(&args[1], ctx, depth)?;
        let sig = match args.get(2) {
            Some(arg) if !matches!(arg, Expr::Missing) => {
                let s = self.eval_arg_num(arg, ctx, depth)?.trunc();
                if !s.is_finite() || s < 1.0 {
                    return Err(FormulaError::Num);
                }
                (s as i32).min(15)
            }
            _ => 3,
        };
        let mut nums = numeric_values(&vals);
        if nums.is_empty() {
            return Err(FormulaError::Num);
        }
        nums.sort_unstable_by(cmp_f64);
        let n = nums.len();
        if x < nums[0] || x > nums[n - 1] {
            return Err(FormulaError::Na);
        }
        let rank_at = |idx: usize| -> f64 {
            if is_exc {
                ((idx + 1) as f64) / ((n + 1) as f64)
            } else if n == 1 {
                1.0
            } else {
                (idx as f64) / ((n - 1) as f64)
            }
        };
        let raw_rank = if let Some(first_idx) = nums.iter().position(|&v| v == x) {
            rank_at(first_idx)
        } else {
            let hi = nums.partition_point(|&v| v < x);
            let lo = hi - 1;
            let r_lo = rank_at(lo);
            let r_hi = rank_at(hi);
            let frac = (x - nums[lo]) / (nums[hi] - nums[lo]);
            r_lo + frac * (r_hi - r_lo)
        };
        let factor = 10.0_f64.powi(sig);
        let truncated = ((raw_rank * factor) + 1e-12).trunc() / factor;
        Ok(EvaluatedCellValue::Number(truncated))
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
        let mut nums = numeric_values(&vals);
        if nums.is_empty() {
            return Err(FormulaError::Num);
        }
        let is_exc = name.ends_with(".EXC");
        let p = if name.starts_with("QUARTILE") {
            let q = param.trunc() as i32;
            let valid = if is_exc { 1..=3 } else { 0..=4 };
            if !valid.contains(&q) {
                return Err(FormulaError::Num);
            }
            (q as f64) * 0.25
        } else if is_exc && (param <= 0.0 || param >= 1.0)
            || !is_exc && !(0.0..=1.0).contains(&param)
        {
            return Err(FormulaError::Num);
        } else {
            param
        };
        let pos = if is_exc {
            let raw = p * ((nums.len() + 1) as f64);
            if raw < 1.0 || raw > nums.len() as f64 {
                return Err(FormulaError::Num);
            }
            raw - 1.0
        } else {
            p * ((nums.len() - 1) as f64)
        };
        let lo = pos.floor() as usize;
        let hi = pos.ceil() as usize;
        let (v_lo, v_hi) = select_pair(&mut nums, lo, hi);
        Ok(EvaluatedCellValue::Number(
            v_lo + (pos - lo as f64) * (v_hi - v_lo),
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::{EvaluatedCellValue, FormulaError, SheetEngine};

    #[test]
    fn statistical_math_text_and_date_reference_upgrades() {
        let mut e = SheetEngine::new("Sheet1");
        for (idx, v) in [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0].iter().enumerate() {
            e.set_cell(&format!("A{}", idx + 1), v.to_string()).unwrap();
        }
        assert_eq!(
            e.evaluate_formula_str("=AVEDEV(A1:A8)"),
            EvaluatedCellValue::Number(1.5)
        );
        assert_eq!(
            e.evaluate_formula_str("=DEVSQ(A1:A8)"),
            EvaluatedCellValue::Number(32.0)
        );
        assert_eq!(
            e.evaluate_formula_str("=ROUND(GEOMEAN(A1:A8),4)"),
            EvaluatedCellValue::Number(4.6032)
        );
        assert_eq!(
            e.evaluate_formula_str("=ROUND(HARMEAN(A1:A8),4)"),
            EvaluatedCellValue::Number(4.2018)
        );
        assert_eq!(
            e.evaluate_formula_str("=TRIMMEAN(A1:A8,0.25)"),
            EvaluatedCellValue::Number(4.833333333333333)
        );
        assert_eq!(
            e.evaluate_formula_str("=ROUND(SKEW(A1:A8),4)"),
            EvaluatedCellValue::Number(0.8185)
        );
        assert_eq!(
            e.evaluate_formula_str("=ROUND(KURT(A1:A8),4)"),
            EvaluatedCellValue::Number(0.9406)
        );
        assert_eq!(
            e.evaluate_formula_str("=PERCENTILE.EXC(A1:A8,0.5)"),
            EvaluatedCellValue::Number(4.5)
        );
        assert_eq!(
            e.evaluate_formula_str("=QUARTILE.EXC(A1:A8,2)"),
            EvaluatedCellValue::Number(4.5)
        );
        assert_eq!(
            e.evaluate_formula_str("=RANK.EQ(4,A1:A8)"),
            EvaluatedCellValue::Number(5.0)
        );
        assert_eq!(
            e.evaluate_formula_str("=RANK.AVG(4,A1:A8)"),
            EvaluatedCellValue::Number(6.0)
        );

        assert_eq!(
            e.evaluate_formula_str("=SUMSQ(3,4)"),
            EvaluatedCellValue::Number(25.0)
        );
        assert_eq!(
            e.evaluate_formula_str("=GCD(24,36)"),
            EvaluatedCellValue::Number(12.0)
        );
        assert_eq!(
            e.evaluate_formula_str("=LCM(4,6)"),
            EvaluatedCellValue::Number(12.0)
        );
        assert_eq!(
            e.evaluate_formula_str("=COMBIN(5,2)"),
            EvaluatedCellValue::Number(10.0)
        );
        assert_eq!(
            e.evaluate_formula_str("=PERMUT(5,2)"),
            EvaluatedCellValue::Number(20.0)
        );
        assert_eq!(
            e.evaluate_formula_str("=EVEN(3)"),
            EvaluatedCellValue::Number(4.0)
        );
        assert_eq!(
            e.evaluate_formula_str("=ODD(-2)"),
            EvaluatedCellValue::Number(-3.0)
        );
        assert_eq!(
            e.evaluate_formula_str("=FACTDOUBLE(6)"),
            EvaluatedCellValue::Number(48.0)
        );
        assert_eq!(
            e.evaluate_formula_str("=DEGREES(RADIANS(180))"),
            EvaluatedCellValue::Number(180.0)
        );
        assert_eq!(
            e.evaluate_formula_str("=BASE(255,16,4)"),
            EvaluatedCellValue::Text("00FF".into())
        );
        assert_eq!(
            e.evaluate_formula_str("=DECIMAL(\"FF\",16)"),
            EvaluatedCellValue::Number(255.0)
        );
        assert_eq!(
            e.evaluate_formula_str("=ROMAN(1999)"),
            EvaluatedCellValue::Text("MCMXCIX".into())
        );
        assert_eq!(
            e.evaluate_formula_str("=ARABIC(\"MCMXCIX\")"),
            EvaluatedCellValue::Number(1999.0)
        );
        assert_eq!(
            e.evaluate_formula_str(
                "=DEGREES(ATAN2(1,1)-ASIN(0)-ACOS(1)-ATAN(0)-SINH(0)-COSH(0)+1)"
            ),
            EvaluatedCellValue::Number(45.0)
        );
        assert_eq!(
            e.evaluate_formula_str("=ROUND(SQRTPI(1)+TANH(0),4)"),
            EvaluatedCellValue::Number(1.7725)
        );

        assert_eq!(
            e.evaluate_formula_str("=TEXTBEFORE(\"a-b-c\",\"-\",2)"),
            EvaluatedCellValue::Text("a-b".into())
        );
        assert_eq!(
            e.evaluate_formula_str("=TEXTAFTER(\"a-b-c\",\"-\",-1)"),
            EvaluatedCellValue::Text("c".into())
        );
        assert_eq!(
            e.evaluate_formula_str("=NUMBERVALUE(\"1.234,5%\", \",\", \".\")"),
            EvaluatedCellValue::Number(12.345)
        );
        assert_eq!(
            e.evaluate_formula_str("=VALUETOTEXT(\"hi\",1)"),
            EvaluatedCellValue::Text("\"hi\"".into())
        );
        assert_eq!(
            e.evaluate_formula_str("=UNICODE(UNICHAR(9731))"),
            EvaluatedCellValue::Number(9731.0)
        );
        assert_eq!(
            e.evaluate_formula_str("=ERROR.TYPE(NA())"),
            EvaluatedCellValue::Number(7.0)
        );
        assert_eq!(
            e.evaluate_formula_str("=TIMEVALUE(\"06:00 PM\")"),
            EvaluatedCellValue::Number(0.75)
        );
        assert_eq!(
            e.evaluate_formula_str("=WEEKNUM(DATE(2024,1,7),1)"),
            EvaluatedCellValue::Number(2.0)
        );
        assert_eq!(
            e.evaluate_formula_str("=ISOWEEKNUM(DATE(2024,1,4))"),
            EvaluatedCellValue::Number(1.0)
        );
        assert_eq!(
            e.evaluate_formula_str("=YEARFRAC(DATE(2024,1,1),DATE(2024,7,1),0)"),
            EvaluatedCellValue::Number(0.5)
        );
        assert_eq!(
            e.evaluate_formula_str("=ERROR.TYPE(123)"),
            EvaluatedCellValue::Error(FormulaError::Na)
        );
    }
}

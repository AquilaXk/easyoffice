use crate::range_refs::StreamingRangeSpec;
use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};

const ZERO_RATE_EPSILON: f64 = 1e-12;
const DERIVATIVE_EPSILON: f64 = 1e-15;
const SOLVER_MAX_ITERATIONS: usize = 100;
const RATE_FLOOR: f64 = -0.999_999;
const DEFAULT_RATE_GUESS: f64 = 0.1;
const BRACKET_LOW: f64 = -0.99;
const BRACKET_HIGH: f64 = 10.0;
const IRR_NEWTON_TOLERANCE: f64 = 1e-10;
const IRR_BISECT_TOLERANCE: f64 = 1e-11;
const IRR_BISECT_WIDTH: f64 = 1e-12;
const RATE_RELATIVE_TOLERANCE: f64 = 1e-9;
const RATE_CHECK_TOLERANCE: f64 = 1e-6;
const RATE_DIFF_STEP: f64 = 1e-7;
const RATE_STEP_TOLERANCE: f64 = 1e-12;
const RATE_MIN_SCALE: f64 = 1e-6;
const MAX_DEPRECIATION_PERIODS: usize = 10_000;
const MONTHS_PER_YEAR: usize = 12;
const DB_RATE_SCALE: f64 = 1000.0;
const DDB_DEFAULT_FACTOR: f64 = 2.0;

fn check_arity(args: &[Expr], min: usize, max: usize) -> Result<(), FormulaError> {
    if args.len() < min || args.len() > max {
        Err(FormulaError::Value)
    } else {
        Ok(())
    }
}

#[inline]
fn type_adjust(rate: f64, t: f64) -> f64 {
    if t != 0.0 {
        1.0 + rate
    } else {
        1.0
    }
}

fn num(v: f64) -> Result<EvaluatedCellValue, FormulaError> {
    Ok(EvaluatedCellValue::Number(v))
}

fn calc_pmt(rate: f64, nper: f64, pv: f64, fv: f64, t: f64) -> Result<f64, FormulaError> {
    if nper.abs() < ZERO_RATE_EPSILON {
        return Err(FormulaError::DivByZero);
    }
    if rate.abs() < ZERO_RATE_EPSILON {
        return Ok((-pv - fv) / nper);
    }
    let factor = (1.0 + rate).powf(nper);
    let denom = type_adjust(rate, t) * (factor - 1.0);
    if denom.abs() < DERIVATIVE_EPSILON {
        return Err(FormulaError::DivByZero);
    }
    Ok(-rate * (pv * factor + fv) / denom)
}

fn calc_fv(rate: f64, nper: f64, pmt: f64, pv: f64, t: f64) -> Result<f64, FormulaError> {
    if rate.abs() < ZERO_RATE_EPSILON {
        return Ok(-pv - pmt * nper);
    }
    let factor = (1.0 + rate).powf(nper);
    Ok(-pv * factor - pmt * type_adjust(rate, t) * (factor - 1.0) / rate)
}

/// Net present value of `cfs` (scaled) at `rate`, with the first flow at period 0.
fn irr_npv(cfs: &[f64], rate: f64) -> f64 {
    cfs.iter()
        .enumerate()
        .map(|(t, &cf)| cf / (1.0 + rate).powi(t as i32))
        .sum()
}

fn irr_newton(cfs: &[f64], mut r: f64) -> Option<f64> {
    for _ in 0..SOLVER_MAX_ITERATIONS {
        if r <= RATE_FLOOR || !r.is_finite() {
            return None;
        }
        let mut f = 0.0;
        let mut df = 0.0;
        for (t, &cf) in cfs.iter().enumerate() {
            f += cf / (1.0 + r).powi(t as i32);
            if t > 0 {
                df -= (t as f64) * cf / (1.0 + r).powi((t + 1) as i32);
            }
        }
        if f.abs() < IRR_NEWTON_TOLERANCE {
            return Some(r);
        }
        if df.abs() < DERIVATIVE_EPSILON {
            return None;
        }
        r -= f / df;
    }
    None
}

fn irr_bisect(cfs: &[f64]) -> Result<f64, FormulaError> {
    let mut lo = BRACKET_LOW;
    let mut hi = BRACKET_HIGH;
    let mut f_lo = irr_npv(cfs, lo);
    let f_hi = irr_npv(cfs, hi);
    if !(f_lo.is_finite() && f_hi.is_finite() && f_lo * f_hi <= 0.0) {
        return Err(FormulaError::Num);
    }
    for _ in 0..SOLVER_MAX_ITERATIONS {
        let mid = 0.5 * (lo + hi);
        let f_mid = irr_npv(cfs, mid);
        if f_mid.abs() < IRR_BISECT_TOLERANCE || (hi - lo).abs() < IRR_BISECT_WIDTH {
            return Ok(mid);
        }
        if f_lo * f_mid <= 0.0 {
            hi = mid;
        } else {
            lo = mid;
            f_lo = f_mid;
        }
    }
    Ok(0.5 * (lo + hi))
}

/// Annuity parameters shared by the RATE solver.
#[derive(Clone, Copy)]
struct RateProblem {
    nper: f64,
    pmt: f64,
    pv: f64,
    fv: f64,
    t: f64,
    f_tol: f64,
    scale: f64,
}

impl RateProblem {
    fn residual(&self, r: f64) -> Result<f64, FormulaError> {
        Ok(calc_fv(r, self.nper, self.pmt, self.pv, self.t)? - self.fv)
    }

    fn newton(&self, mut r: f64) -> Option<f64> {
        for _ in 0..SOLVER_MAX_ITERATIONS {
            if r <= RATE_FLOOR || !r.is_finite() {
                return None;
            }
            let f = self.residual(r).ok()?;
            if f.abs() <= self.f_tol {
                return Some(r);
            }
            let eps = RATE_DIFF_STEP * r.abs().max(1.0);
            let df = (self.residual(r + eps).ok()? - f) / eps;
            if !df.is_finite() || df.abs() < DERIVATIVE_EPSILON {
                return None;
            }
            let step = f / df;
            r -= step;
            if step.abs() <= RATE_STEP_TOLERANCE * r.abs().max(RATE_MIN_SCALE)
                && self.is_converged(r)
            {
                return Some(r);
            }
        }
        None
    }

    fn is_converged(&self, r: f64) -> bool {
        self.residual(r)
            .is_ok_and(|f| f.abs() <= RATE_CHECK_TOLERANCE * self.scale)
    }

    fn bisect(&self) -> Result<f64, FormulaError> {
        let mut lo = BRACKET_LOW;
        let mut hi = BRACKET_HIGH;
        let (Ok(r_lo), Ok(r_hi)) = (self.residual(lo), self.residual(hi)) else {
            return Err(FormulaError::Num);
        };
        let mut f_lo = r_lo;
        if !(f_lo.is_finite() && r_hi.is_finite() && f_lo * r_hi <= 0.0) {
            return Err(FormulaError::Num);
        }
        for _ in 0..SOLVER_MAX_ITERATIONS {
            let mid = 0.5 * (lo + hi);
            let Ok(f_mid) = self.residual(mid) else {
                break;
            };
            if f_mid.abs() <= self.f_tol
                || (hi - lo).abs() <= RATE_STEP_TOLERANCE * mid.abs().max(RATE_MIN_SCALE)
            {
                return Ok(mid);
            }
            if f_lo * f_mid <= 0.0 {
                hi = mid;
            } else {
                lo = mid;
                f_lo = f_mid;
            }
        }
        Ok(0.5 * (lo + hi))
    }
}

fn declining_balance(cost: f64, salvage: f64, life: f64, period: usize, month: usize) -> f64 {
    let rate = ((1.0 - (salvage / cost).powf(1.0 / life)) * DB_RATE_SCALE).round() / DB_RATE_SCALE;
    let months = MONTHS_PER_YEAR as f64;
    let mut total_dep = 0.0;
    let mut dep = 0.0;
    for p in 1..=period {
        dep = if p == 1 {
            cost * rate * (month as f64) / months
        } else if p == (life.trunc() as usize) + 1 && month < MONTHS_PER_YEAR {
            (cost - total_dep) * rate * ((MONTHS_PER_YEAR - month) as f64) / months
        } else {
            (cost - total_dep) * rate
        };
        total_dep += dep;
    }
    dep
}

fn double_declining_balance(cost: f64, salvage: f64, life: f64, period: usize, factor: f64) -> f64 {
    let mut book = cost;
    let mut dep = 0.0;
    for _ in 1..=period {
        dep = (book * (factor / life)).min((book - salvage).max(0.0));
        book -= dep;
    }
    dep
}

/// Running state of the NPV accumulation.
struct NpvAccumulator {
    rate: f64,
    npv: f64,
    t: i32,
}

impl NpvAccumulator {
    #[inline]
    fn add(&mut self, n: f64) {
        self.t += 1;
        self.npv += n / (1.0 + self.rate).powi(self.t);
    }
}

impl SheetEngine {
    pub(crate) fn eval_financial_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        match name {
            "PMT" => self.fin_pmt(args, ctx, depth),
            "IPMT" | "PPMT" => self.fin_ipmt_ppmt(name == "IPMT", args, ctx, depth),
            "FV" => self.fin_fv(args, ctx, depth),
            "PV" => self.fin_pv(args, ctx, depth),
            "NPV" => self.fin_npv(args, ctx, depth),
            "IRR" => self.fin_irr(args, ctx, depth),
            "RATE" => self.fin_rate(args, ctx, depth),
            "NPER" => self.fin_nper(args, ctx, depth),
            "SLN" => self.fin_sln(args, ctx, depth),
            "DB" => self.fin_db(args, ctx, depth),
            "DDB" => self.fin_ddb(args, ctx, depth),
            "SYD" => self.fin_syd(args, ctx, depth),
            "EFFECT" => self.fin_effect(args, ctx, depth),
            "NOMINAL" => self.fin_nominal(args, ctx, depth),
            _ => Err(FormulaError::Name),
        }
    }

    /// Evaluates the optional argument at `idx`, or returns `default` when absent.
    fn opt_arg_num(
        &self,
        args: &[Expr],
        idx: usize,
        default: f64,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<f64, FormulaError> {
        match args.get(idx) {
            Some(arg) => self.eval_arg_num(arg, ctx, depth),
            None => Ok(default),
        }
    }

    /// Evaluates the first `N` arguments in order.
    fn arg_nums<const N: usize>(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<[f64; N], FormulaError> {
        let mut out = [0.0; N];
        for (slot, arg) in out.iter_mut().zip(args) {
            *slot = self.eval_arg_num(arg, ctx, depth)?;
        }
        Ok(out)
    }

    fn fin_pmt(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 3, 5)?;
        let [rate, nper, pv] = self.arg_nums(args, ctx, depth)?;
        let fv = self.opt_arg_num(args, 3, 0.0, ctx, depth)?;
        let t = self.opt_arg_num(args, 4, 0.0, ctx, depth)?;
        num(calc_pmt(rate, nper, pv, fv, t)?)
    }

    fn fin_ipmt_ppmt(
        &self,
        is_interest: bool,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 4, 6)?;
        let [rate, per, nper, pv] = self.arg_nums(args, ctx, depth)?;
        let fv = self.opt_arg_num(args, 4, 0.0, ctx, depth)?;
        let t = self.opt_arg_num(args, 5, 0.0, ctx, depth)?;
        if per < 1.0 || per > nper {
            return Err(FormulaError::Num);
        }
        let pmt = calc_pmt(rate, nper, pv, fv, t)?;
        let ipmt = if (per - 1.0).abs() < 1e-9 && t != 0.0 {
            0.0
        } else {
            calc_fv(rate, per - 1.0, pmt, pv, t)? * rate / type_adjust(rate, t)
        };
        num(if is_interest { ipmt } else { pmt - ipmt })
    }

    fn fin_fv(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 3, 5)?;
        let [rate, nper, pmt] = self.arg_nums(args, ctx, depth)?;
        let pv = self.opt_arg_num(args, 3, 0.0, ctx, depth)?;
        let t = self.opt_arg_num(args, 4, 0.0, ctx, depth)?;
        num(calc_fv(rate, nper, pmt, pv, t)?)
    }

    fn fin_pv(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 3, 5)?;
        let [rate, nper, pmt] = self.arg_nums(args, ctx, depth)?;
        let fv = self.opt_arg_num(args, 3, 0.0, ctx, depth)?;
        let t = self.opt_arg_num(args, 4, 0.0, ctx, depth)?;
        if rate.abs() < ZERO_RATE_EPSILON {
            return num(-fv - pmt * nper);
        }
        let factor = (1.0 + rate).powf(nper);
        num((-fv - pmt * type_adjust(rate, t) * (factor - 1.0) / rate) / factor)
    }

    fn npv_range(
        &self,
        spec: &StreamingRangeSpec<'_>,
        acc: &mut NpvAccumulator,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<(), FormulaError> {
        let (_, _, _, rows, cols, _) = *spec;
        for dr in 0..rows {
            for dc in 0..cols {
                if let EvaluatedCellValue::Number(n) =
                    self.eval_streaming_cell(spec, dr, dc, ctx, depth)?
                {
                    acc.add(n);
                }
            }
        }
        Ok(())
    }

    fn fin_npv(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 2, usize::MAX)?;
        let rate = self.eval_arg_num(&args[0], ctx, depth)?;
        if rate <= -1.0 {
            return Err(FormulaError::DivByZero);
        }
        let mut acc = NpvAccumulator {
            rate,
            npv: 0.0,
            t: 0,
        };
        for arg in &args[1..] {
            if let Some(spec) = self.extract_streaming_range_ctx(arg, ctx, depth)? {
                self.npv_range(&spec, &mut acc, ctx, depth)?;
                continue;
            }
            let v = self.eval_arg_val(arg, ctx, depth)?;
            if !matches!(v, EvaluatedCellValue::Empty) {
                acc.add(v.as_number().ok_or(FormulaError::Value)?);
            }
        }
        num(acc.npv)
    }

    fn fin_irr(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 1, 2)?;
        let vals = self.eval_range_vec(&args[0], ctx, depth)?;
        let cfs: Vec<f64> = vals
            .iter()
            .filter_map(EvaluatedCellValue::as_number)
            .collect();
        let max_cf = cfs.iter().map(|v| v.abs()).fold(0.0_f64, f64::max);
        if cfs.len() < 2
            || max_cf == 0.0
            || !cfs.iter().any(|&v| v > 0.0)
            || !cfs.iter().any(|&v| v < 0.0)
        {
            return Err(FormulaError::Num);
        }
        let scaled: Vec<f64> = cfs.iter().map(|&v| v / max_cf).collect();
        let guess = self.opt_arg_num(args, 1, DEFAULT_RATE_GUESS, ctx, depth)?;
        match irr_newton(&scaled, guess) {
            Some(r) => num(r),
            None => num(irr_bisect(&scaled)?),
        }
    }

    fn fin_rate(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 3, 6)?;
        let [nper, pmt, pv] = self.arg_nums(args, ctx, depth)?;
        let fv = self.opt_arg_num(args, 3, 0.0, ctx, depth)?;
        let t = self.opt_arg_num(args, 4, 0.0, ctx, depth)?;
        let guess = self.opt_arg_num(args, 5, DEFAULT_RATE_GUESS, ctx, depth)?;
        if nper <= 0.0 {
            return Err(FormulaError::Num);
        }
        let scale = pv.abs().max(fv.abs()).max(pmt.abs() * nper).max(1.0);
        let problem = RateProblem {
            nper,
            pmt,
            pv,
            fv,
            t,
            f_tol: RATE_RELATIVE_TOLERANCE * scale,
            scale,
        };
        match problem.newton(guess) {
            Some(r) => num(r),
            None => num(problem.bisect()?),
        }
    }

    fn fin_nper(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 3, 5)?;
        let [rate, pmt, pv] = self.arg_nums(args, ctx, depth)?;
        let fv = self.opt_arg_num(args, 3, 0.0, ctx, depth)?;
        let t = self.opt_arg_num(args, 4, 0.0, ctx, depth)?;
        if rate.abs() < ZERO_RATE_EPSILON {
            if pmt.abs() < ZERO_RATE_EPSILON {
                return Err(FormulaError::DivByZero);
            }
            return num((-pv - fv) / pmt);
        }
        let pmt_adj = pmt * type_adjust(rate, t);
        let numerator = pmt_adj - fv * rate;
        let den = pv * rate + pmt_adj;
        if den == 0.0 || numerator / den <= 0.0 || 1.0 + rate <= 0.0 {
            return Err(FormulaError::Num);
        }
        num((numerator / den).ln() / (1.0 + rate).ln())
    }

    fn fin_sln(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 3, 3)?;
        let [cost, salvage, life] = self.arg_nums(args, ctx, depth)?;
        if life.abs() < DERIVATIVE_EPSILON {
            return Err(FormulaError::DivByZero);
        }
        num((cost - salvage) / life)
    }

    fn fin_db(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 4, 5)?;
        let [cost, salvage, life, period] = self.arg_nums(args, ctx, depth)?;
        let period = period.trunc() as usize;
        let month = self
            .opt_arg_num(args, 4, MONTHS_PER_YEAR as f64, ctx, depth)?
            .trunc() as usize;
        if cost <= 0.0
            || salvage < 0.0
            || life <= 0.0
            || !(1..=MAX_DEPRECIATION_PERIODS).contains(&period)
            || !(1..=MONTHS_PER_YEAR).contains(&month)
        {
            return Err(FormulaError::Num);
        }
        num(declining_balance(cost, salvage, life, period, month))
    }

    fn fin_ddb(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 4, 5)?;
        let [cost, salvage, life, period] = self.arg_nums(args, ctx, depth)?;
        let period = period.trunc() as usize;
        let factor = self.opt_arg_num(args, 4, DDB_DEFAULT_FACTOR, ctx, depth)?;
        if cost < 0.0
            || salvage < 0.0
            || life <= 0.0
            || !(1..=MAX_DEPRECIATION_PERIODS).contains(&period)
            || factor <= 0.0
        {
            return Err(FormulaError::Num);
        }
        num(double_declining_balance(
            cost, salvage, life, period, factor,
        ))
    }

    fn fin_syd(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 4, 4)?;
        let [cost, salvage, life, per] = self.arg_nums(args, ctx, depth)?;
        if life <= 0.0 || per <= 0.0 || per > life {
            return Err(FormulaError::Num);
        }
        num((cost - salvage) * (life - per + 1.0) * 2.0 / (life * (life + 1.0)))
    }

    fn fin_effect(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 2, 2)?;
        let [nominal, npery] = self.arg_nums(args, ctx, depth)?;
        let npery = npery.trunc();
        if nominal <= 0.0 || npery < 1.0 {
            return Err(FormulaError::Num);
        }
        num((1.0 + nominal / npery).powf(npery) - 1.0)
    }

    fn fin_nominal(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 2, 2)?;
        let [effect, npery] = self.arg_nums(args, ctx, depth)?;
        let npery = npery.trunc();
        if effect <= 0.0 || npery < 1.0 {
            return Err(FormulaError::Num);
        }
        num(((effect + 1.0).powf(1.0 / npery) - 1.0) * npery)
    }
}

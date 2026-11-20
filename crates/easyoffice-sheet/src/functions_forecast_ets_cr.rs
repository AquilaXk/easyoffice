//! Clean-room numerics of `FORECAST.ETS`, `FORECAST.ETS.CONFINT`, `FORECAST.ETS.SEASONALITY` and
//! `FORECAST.ETS.STAT` (additive Holt-Winters, ETS(A,A,A) and Holt linear trend, ETS(A,A,N)).
//!
//! License: Apache-2.0. Written from public specifications and statistical literature recorded in
//! `docs/forecast-ets-cleanroom.md` (Hyndman, Koehler, Snyder and Grose 2002; Hyndman, Koehler,
//! Ord and Snyder 2008; Hyndman and Athanasopoulos, *Forecasting: Principles and Practice*, 3rd
//! ed., sections 2.8, 2.9, 5.8, 8.1, 8.3, 8.5 to 8.7 and 9.1). Parameter optimization and
//! spectral autocorrelation live in `functions_forecast_ets_opt`; see `THIRD_PARTY_NOTICES.md`.
//!
//! Every input is bounded before allocation (`MAX_POINTS`, `MAX_SEASONALITY`, `MAX_EVALS`,
//! `MAX_STEPS_AHEAD`) and every loop charges the caller's step and byte budget: over any limit the
//! calculation fails closed with an error, never a partial value.

use crate::functions_dist::dist_value;
use crate::functions_forecast_ets_opt::{detect_period_bounded, optimize_bounded};

#[cfg(test)]
use crate::functions_forecast_ets_opt::{autocorr_direct, autocorr_fft};

/// Longest seasonal period (hours of a year), section 1 of the specification.
pub const MAX_SEASONALITY: usize = 8_760;
/// Maximum points of one call, before and after completing missing slots (section 6).
pub const MAX_POINTS: usize = 32_768;
/// Evaluations of the fit loss in one smoothing-parameter search.
pub const MAX_EVALS: usize = 1_500;
/// Farthest target in steps past the last point.
pub const MAX_STEPS_AHEAD: f64 = 1e9;
/// Points left after aggregation below which no model is fitted.
const MIN_POINTS: usize = 3;
/// Share of missing points that is still accepted, in tenths of the grid (3 of 10 is 30%).
const MISSING_TENTHS: usize = 3;
/// Relative distance from an integer at which a timeline ratio still counts as that integer.
const STEP_TOLERANCE: f64 = 1e-6;
/// Seasons used for the seasonal initial state.
const INIT_SEASONS: usize = 4;
/// Inner-loop point operations charged as one evaluation step.
const OPS_PER_STEP: usize = 32;
/// Binary exponent magnitude above which values are scaled by a power of two so `SSE` cannot
/// overflow `f64::MAX` or underflow to zero.
const SCALE_EXP_THRESHOLD: i32 = 450;

/// Charges `(steps, extra_bytes)` against the caller's evaluation context; returns `false` when
/// the step ceiling, memory ceiling, or evaluation deadline has been exceeded.
pub type BudgetFn<'a> = &'a mut dyn FnMut(usize, usize) -> bool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EtsError {
    /// `values` and `timeline` differ in size (`#N/A`).
    SizeMismatch,
    /// A value or the timeline is NaN or infinite, or the target is (`#VALUE!`).
    NonFinite,
    /// The timeline repeats a time and no aggregation was given (`#VALUE!`).
    DuplicateTimeline,
    /// Step, memory, or point-count budget was exceeded (`#VALUE!`).
    BudgetExceeded,
    /// An argument is outside its set (`#NUM!`).
    InvalidArgument,
    /// No constant step fits the timeline (`#NUM!`).
    NoConstantStep,
    /// More points than `MAX_POINTS` (`#NUM!` in pure calls, `#VALUE!` in sheet budget mapping).
    TooManyPoints,
    /// More than 30% of the completed timeline is missing (`#NUM!`).
    TooManyMissing,
    /// Too few points for the model or the period (`#NUM!`).
    TooFewPoints,
    /// The target is before the end of the timeline (`#NUM!`).
    TargetBeforeEnd,
    /// The target is more than `MAX_STEPS_AHEAD` steps away (`#NUM!`).
    TargetTooFar,
    /// The scale of MASE is zero (`#NUM!`).
    ZeroScale,
    /// The computed value is not finite (`#NUM!`).
    NonFiniteResult,
}

#[cfg(test)]
impl EtsError {
    /// The sheet error this stands for.
    pub fn code(self) -> &'static str {
        match self {
            Self::SizeMismatch => "#N/A",
            Self::NonFinite | Self::DuplicateTimeline | Self::BudgetExceeded => "#VALUE!",
            _ => "#NUM!",
        }
    }
}

type Fallible<T> = Result<T, EtsError>;

/// The arguments the four functions share. `seasonality` is 1 for detection, 0 for none, or the
/// period; `data_completion` is 0 or 1; `aggregation` is absent or 0 to 7 (section 5.8).
#[derive(Debug, Clone, Copy)]
pub struct EtsArgs<'a> {
    pub values: &'a [f64],
    pub timeline: &'a [f64],
    pub seasonality: f64,
    pub data_completion: f64,
    pub aggregation: Option<f64>,
}

#[cfg(test)]
impl<'a> EtsArgs<'a> {
    /// The two required arguments with every optional one at its default.
    pub fn new(values: &'a [f64], timeline: &'a [f64]) -> Self {
        Self {
            values,
            timeline,
            seasonality: 1.0,
            data_completion: 1.0,
            aggregation: None,
        }
    }
}

#[inline]
pub(crate) fn charge_ops(budget: BudgetFn, ops: usize) -> Fallible<()> {
    if budget(ops.div_ceil(OPS_PER_STEP).max(1), 0) {
        Ok(())
    } else {
        Err(EtsError::BudgetExceeded)
    }
}

#[inline]
pub(crate) fn charge_bytes(budget: BudgetFn, bytes: usize) -> Fallible<()> {
    if budget(1, bytes) {
        Ok(())
    } else {
        Err(EtsError::BudgetExceeded)
    }
}

/// Exact IEEE-754 power-of-two factor `2^-shift` / `2^+shift` (zero mantissa rounding).
#[derive(Clone, Copy, Debug)]
struct Scale {
    down: f64,
    up: f64,
}

impl Scale {
    fn for_slice(values: &[f64]) -> Self {
        let max_abs = values.iter().fold(0.0_f64, |m, &v| m.max(v.abs()));
        if max_abs == 0.0 || !max_abs.is_finite() {
            return Self { down: 1.0, up: 1.0 };
        }
        let biased = ((max_abs.to_bits() >> 52) & 0x7ff) as i32;
        let exp = if biased == 0 { -1022 } else { biased - 1023 };
        if exp.abs() <= SCALE_EXP_THRESHOLD {
            return Self { down: 1.0, up: 1.0 };
        }
        let shift = exp.clamp(-1020, 1020);
        Self {
            down: f64::from_bits(((1023 - shift) as u64) << 52),
            up: f64::from_bits(((1023 + shift) as u64) << 52),
        }
    }

    #[inline]
    fn active(self) -> bool {
        self.down != 1.0
    }
}

#[derive(Clone, Copy)]
enum Season {
    Detect,
    Off,
    Period(usize),
}

#[derive(Clone, Copy)]
enum Aggregate {
    Average,
    Count,
    Max,
    Median,
    Min,
    Sum,
}

impl Aggregate {
    /// Section 5.8: 0 and 1 AVERAGE, 2 COUNT, 3 COUNTA (the same on numbers), 4 MAX, 5 MEDIAN,
    /// 6 MIN, 7 SUM.
    fn from_code(code: f64) -> Fallible<Self> {
        if !code.is_finite() || code.fract() != 0.0 {
            return Err(EtsError::InvalidArgument);
        }
        Ok(match code as i64 {
            0 | 1 => Self::Average,
            2 | 3 => Self::Count,
            4 => Self::Max,
            5 => Self::Median,
            6 => Self::Min,
            7 => Self::Sum,
            _ => return Err(EtsError::InvalidArgument),
        })
    }

    fn apply(self, group: &mut [f64]) -> f64 {
        match self {
            Self::Average => group.iter().sum::<f64>() / group.len() as f64,
            Self::Count => group.len() as f64,
            Self::Max => group.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            Self::Min => group.iter().copied().fold(f64::INFINITY, f64::min),
            Self::Sum => group.iter().sum(),
            Self::Median => {
                group.sort_unstable_by(f64::total_cmp);
                let mid = group.len() / 2;
                if group.len() % 2 == 1 {
                    group[mid]
                } else {
                    (group[mid - 1] + group[mid]) / 2.0
                }
            }
        }
    }
}

struct Options {
    season: Season,
    fill_with_zero: bool,
    aggregate: Option<Aggregate>,
}

fn parse_options(args: &EtsArgs, season: f64) -> Fallible<Options> {
    if !season.is_finite()
        || season < 0.0
        || season.fract() != 0.0
        || season > MAX_SEASONALITY as f64
    {
        return Err(EtsError::InvalidArgument);
    }
    let season = match season as usize {
        0 => Season::Off,
        1 => Season::Detect,
        m => Season::Period(m),
    };
    let fill_with_zero = match args.data_completion {
        0.0 => true,
        1.0 => false,
        _ => return Err(EtsError::InvalidArgument),
    };
    let aggregate = args.aggregation.map(Aggregate::from_code).transpose()?;
    Ok(Options {
        season,
        fill_with_zero,
        aggregate,
    })
}

/// The series on a regular grid: point `i` is at `first + i * step`.
struct Series {
    y: Vec<f64>,
    last: f64,
    step: f64,
    scale: Scale,
}

/// Sort by time, aggregate repeated times, find the step, count and fill the missing points
/// (sections 5.8, 5.10, 5.11, 7).
fn build_series(args: &EtsArgs, options: &Options, budget: BudgetFn) -> Fallible<Series> {
    let n = args.values.len();
    if n != args.timeline.len() {
        return Err(EtsError::SizeMismatch);
    }
    if n > MAX_POINTS {
        return Err(EtsError::TooManyPoints);
    }
    if args
        .values
        .iter()
        .chain(args.timeline)
        .any(|v| !v.is_finite())
    {
        return Err(EtsError::NonFinite);
    }
    charge_ops(budget, n.saturating_mul(2))?;
    charge_bytes(budget, n.saturating_mul(24))?;
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_unstable_by(|&a, &b| args.timeline[a].total_cmp(&args.timeline[b]));
    let (times, values) = aggregate_repeats(args, &order, options.aggregate)?;
    if times.len() < MIN_POINTS {
        return Err(EtsError::TooFewPoints);
    }
    let first = times[0];
    let last = times[times.len() - 1];
    let step = times
        .windows(2)
        .map(|w| w[1] - w[0])
        .fold(f64::INFINITY, f64::min);
    if !(step.is_finite() && step > 0.0 && (last - first).is_finite()) {
        return Err(EtsError::NoConstantStep);
    }
    let slots = grid_slots(&times, first, step)?;
    let length = slots[slots.len() - 1] + 1;
    charge_bytes(budget, length.saturating_mul(8))?;
    let mut y = fill_grid(&slots, &values, length, options.fill_with_zero);
    let scale = Scale::for_slice(&y);
    if scale.active() {
        for v in &mut y {
            *v *= scale.down;
        }
    }
    Ok(Series {
        y,
        last,
        step,
        scale,
    })
}

fn aggregate_repeats(
    args: &EtsArgs,
    order: &[usize],
    aggregate: Option<Aggregate>,
) -> Fallible<(Vec<f64>, Vec<f64>)> {
    let mut times = Vec::with_capacity(order.len());
    let mut values = Vec::with_capacity(order.len());
    let mut group = Vec::new();
    let mut i = 0;
    while i < order.len() {
        let time = args.timeline[order[i]];
        let mut j = i + 1;
        while j < order.len() && args.timeline[order[j]] == time {
            j += 1;
        }
        match aggregate {
            None if j - i > 1 => return Err(EtsError::DuplicateTimeline),
            None => values.push(args.values[order[i]]),
            // A time seen once is aggregated too: COUNT of one value is 1.
            Some(aggregate) => {
                group.clear();
                group.extend(order[i..j].iter().map(|&k| args.values[k]));
                values.push(aggregate.apply(&mut group));
            }
        }
        times.push(time);
        i = j;
    }
    Ok((times, values))
}

fn grid_slots(times: &[f64], first: f64, step: f64) -> Fallible<Vec<usize>> {
    const MAX_EXACT_INT: f64 = (1_u64 << 52) as f64;
    let mut raw_slots = Vec::with_capacity(times.len());
    for &t in times {
        let ratio = (t - first) / step;
        if !ratio.is_finite() {
            return Err(EtsError::NoConstantStep);
        }
        if ratio > MAX_EXACT_INT {
            return Err(EtsError::TooManyMissing);
        }
        let slot = ratio.round();
        if (ratio - slot).abs() > STEP_TOLERANCE {
            return Err(EtsError::NoConstantStep);
        }
        raw_slots.push(slot as u64);
    }
    let total_len = raw_slots[raw_slots.len() - 1] + 1;
    if (total_len - times.len() as u64).saturating_mul(10)
        > total_len.saturating_mul(MISSING_TENTHS as u64)
    {
        return Err(EtsError::TooManyMissing);
    }
    if total_len > MAX_POINTS as u64 {
        return Err(EtsError::TooManyPoints);
    }
    Ok(raw_slots.into_iter().map(|s| s as usize).collect())
}

fn fill_grid(slots: &[usize], values: &[f64], length: usize, zero: bool) -> Vec<f64> {
    let mut y = vec![0.0; length];
    for (i, (&slot, &value)) in slots.iter().zip(values).enumerate() {
        y[slot] = value;
        if zero || i == 0 || slot == slots[i - 1] + 1 {
            continue;
        }
        let (from, to) = (slots[i - 1], slot);
        let span = (to - from) as f64;
        for (k, cell) in y[from + 1..to].iter_mut().enumerate() {
            let w = (k + 1) as f64 / span;
            *cell = values[i - 1] + (value - values[i - 1]) * w;
        }
    }
    y
}

/// The smoothing coefficients in the Holt-Winters parameterization (section 2.2): each is in
/// `[0, 1]`; the state-space trend and seasonal gains are `alpha * beta` and `gamma * (1 - alpha)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Params {
    pub alpha: f64,
    pub beta: f64,
    pub gamma: f64,
}

impl Params {
    #[inline]
    pub(crate) fn to_coords(self) -> [f64; 3] {
        [self.alpha, self.beta, self.gamma]
    }

    #[inline]
    pub(crate) fn from_coords(c: [f64; 3], seasonal: bool) -> Self {
        Self {
            alpha: c[0],
            beta: c[1],
            gamma: if seasonal { c[2] } else { 0.0 },
        }
    }
}

/// Initial states: level and trend before the first point, and the seasonal values of the period
/// before it (`seasonal[i]` is the value for the slot `i` of the first period).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Init {
    pub level: f64,
    pub trend: f64,
    pub seasonal: Vec<f64>,
}

struct Pass {
    sse: f64,
    level: f64,
    trend: f64,
    next_slot: usize,
}

/// One pass of the error-correction recursion (section 2.1). `ring` holds the seasonal values and
/// is empty without a season. `on_fit` sees every one-step prediction.
fn run(
    y: &[f64],
    init: &Init,
    params: Params,
    ring: &mut Vec<f64>,
    mut on_fit: impl FnMut(f64),
) -> Pass {
    ring.clone_from(&init.seasonal);
    let (mut level, mut trend) = (init.level, init.trend);
    let alpha = params.alpha;
    let beta = alpha * params.beta;
    let gamma = params.gamma * (1.0 - alpha);
    let mut sse = 0.0;
    if ring.is_empty() {
        for &obs in y {
            let fit = level + trend;
            let error = obs - fit;
            on_fit(fit);
            sse += error * error;
            level += trend + alpha * error;
            trend += beta * error;
        }
        return Pass {
            sse,
            level,
            trend,
            next_slot: 0,
        };
    }
    let period = ring.len();
    let mut slot = 0;
    for &obs in y {
        let season = ring[slot];
        let fit = level + trend + season;
        let error = obs - fit;
        on_fit(fit);
        sse += error * error;
        level += trend + alpha * error;
        trend += beta * error;
        ring[slot] = season + gamma * error;
        slot += 1;
        if slot == period {
            slot = 0;
        }
    }
    Pass {
        sse,
        level,
        trend,
        next_slot: slot,
    }
}

fn fit_line(xs: &[f64], ys: &[f64]) -> (f64, f64) {
    let n = xs.len() as f64;
    let (mx, my) = (xs.iter().sum::<f64>() / n, ys.iter().sum::<f64>() / n);
    let (mut sxy, mut sxx) = (0.0, 0.0);
    for (&x, &y) in xs.iter().zip(ys) {
        sxy += (x - mx) * (y - my);
        sxx += (x - mx) * (x - mx);
    }
    let slope = if sxx == 0.0 { 0.0 } else { sxy / sxx };
    (my - slope * mx, slope)
}

/// Allocation-free OLS slope of `ys` over regular `x = 0, 1, ..., n - 1`.
fn uniform_slope(ys: &[f64]) -> f64 {
    let n = ys.len();
    if n < 2 {
        return 0.0;
    }
    let nf = n as f64;
    let mx = (nf - 1.0) * 0.5;
    let sxx = nf * (nf * nf - 1.0) / 12.0;
    let sxy: f64 = ys
        .iter()
        .enumerate()
        .map(|(i, &y)| (i as f64 - mx) * y)
        .sum();
    sxy / sxx
}

/// Initial states from the data (sections 5.2 and 9.5). `period` is 1 without a season.
pub(crate) fn initial_states(y: &[f64], period: usize) -> Init {
    if period <= 1 {
        let trend = uniform_slope(y);
        return Init {
            level: y[0] - trend,
            trend,
            seasonal: Vec::new(),
        };
    }
    let seasons = (y.len() / period).min(INIT_SEASONS);
    let mut means = [0.0; INIT_SEASONS];
    let mut centers = [0.0; INIT_SEASONS];
    for j in 0..seasons {
        means[j] = y[j * period..(j + 1) * period].iter().sum::<f64>() / period as f64;
        centers[j] = (j * period) as f64 + (period + 1) as f64 * 0.5;
    }
    let (mut level, trend) = fit_line(&centers[..seasons], &means[..seasons]);
    let mut seasonal: Vec<f64> = (0..period)
        .map(|i| {
            (0..seasons)
                .map(|j| {
                    let t = j * period + i;
                    y[t] - (level + trend * (t + 1) as f64)
                })
                .sum::<f64>()
                / seasons as f64
        })
        .collect();
    let mean = seasonal.iter().sum::<f64>() / period as f64;
    seasonal.iter_mut().for_each(|s| *s -= mean);
    level += mean;
    Init {
        level,
        trend,
        seasonal,
    }
}

#[cfg(test)]
pub(crate) fn optimize(y: &[f64], init: &Init) -> Params {
    let mut unlimited = |_: usize, _: usize| true;
    optimize_bounded(
        y,
        init,
        |ys, i, p, ring| run(ys, i, p, ring, |_| {}).sse,
        &mut unlimited,
    )
    .unwrap_or(Params {
        alpha: 0.5,
        beta: 0.0,
        gamma: 0.0,
    })
}

#[cfg(test)]
pub(crate) fn detect_period(y: &[f64]) -> usize {
    let mut unlimited = |_: usize, _: usize| true;
    detect_period_bounded(y, &mut unlimited).unwrap_or(1)
}

fn resolve_period(season: Season, y: &[f64], budget: BudgetFn) -> Fallible<usize> {
    match season {
        Season::Off => Ok(1),
        Season::Detect => detect_period_bounded(y, budget),
        Season::Period(m) if y.len() >= 2 * m => Ok(m),
        Season::Period(_) => Err(EtsError::TooFewPoints),
    }
}

pub(crate) struct Model {
    series: Series,
    period: usize,
    params: Params,
    pass: Pass,
    ring: Vec<f64>,
    fitted: Vec<f64>,
}

#[derive(Clone, Copy)]
struct ErrorStats {
    mae: f64,
    smape: f64,
    mase_scale: f64,
}

#[cfg(test)]
pub(crate) fn fit_model(args: &EtsArgs, season: f64) -> Fallible<Model> {
    let mut unlimited = |_: usize, _: usize| true;
    fit_model_bounded(args, season, &mut unlimited)
}

pub(crate) fn fit_model_bounded(args: &EtsArgs, season: f64, budget: BudgetFn) -> Fallible<Model> {
    let options = parse_options(args, season)?;
    let series = build_series(args, &options, budget)?;
    let period = resolve_period(options.season, &series.y, budget)?;
    let init = initial_states(&series.y, period);
    let params = optimize_bounded(
        &series.y,
        &init,
        |ys, i, p, ring| run(ys, i, p, ring, |_| {}).sse,
        budget,
    )?;
    charge_bytes(budget, series.y.len().saturating_mul(8))?;
    let mut ring = Vec::new();
    let mut fitted = Vec::with_capacity(series.y.len());
    let pass = run(&series.y, &init, params, &mut ring, |f| fitted.push(f));
    if !(pass.sse.is_finite() && pass.level.is_finite() && pass.trend.is_finite()) {
        return Err(EtsError::NonFiniteResult);
    }
    Ok(Model {
        series,
        period,
        params,
        pass,
        ring,
        fitted,
    })
}

impl Model {
    fn len(&self) -> usize {
        self.series.y.len()
    }

    fn seasonal(&self) -> bool {
        self.period > 1
    }

    pub(crate) fn steps_to(&self, target: f64) -> Fallible<f64> {
        if !target.is_finite() {
            return Err(EtsError::NonFinite);
        }
        let h = (target - self.series.last) / self.series.step;
        if h < -STEP_TOLERANCE {
            return Err(EtsError::TargetBeforeEnd);
        }
        if h > MAX_STEPS_AHEAD {
            return Err(EtsError::TargetTooFar);
        }
        Ok(h.max(0.0))
    }

    /// Prediction `h` whole steps past the last point; step 0 is the last historical value.
    pub(crate) fn predict_steps(&self, h: usize) -> f64 {
        let raw = if h == 0 {
            self.series.y[self.len() - 1]
        } else {
            let season = if self.seasonal() {
                self.ring[(self.pass.next_slot + h - 1) % self.period]
            } else {
                0.0
            };
            self.pass.level + h as f64 * self.pass.trend + season
        };
        raw * self.series.scale.up
    }

    /// Standard deviation of the `h`-step error, whole `h` (section 5.7).
    pub(crate) fn error_sd_steps(&self, h: usize) -> f64 {
        let sigma2 = self.pass.sse / self.len() as f64;
        let raw = if h <= 1 {
            sigma2.sqrt()
        } else {
            let Params { alpha, beta, gamma } = self.params;
            let (a, b, g) = (alpha, alpha * beta, gamma * (1.0 - alpha));
            let h = h as f64;
            let mut factor =
                1.0 + (h - 1.0) * (a * a + a * b * h + b * b * h * (2.0 * h - 1.0) / 6.0);
            if self.seasonal() {
                let m = self.period as f64;
                let k = ((h - 1.0) / m).floor();
                factor += g * k * (2.0 * a + g + b * m * (k + 1.0));
            }
            (sigma2 * factor).sqrt()
        };
        raw * self.series.scale.up
    }

    pub(crate) fn interpolate(&self, h: f64, at: impl Fn(&Self, usize) -> f64) -> f64 {
        let lo = h.floor();
        let frac = h - lo;
        let lo_value = at(self, lo as usize);
        if frac <= STEP_TOLERANCE {
            lo_value
        } else {
            lo_value + (at(self, lo as usize + 1) - lo_value) * frac
        }
    }

    fn error_stats(&self) -> ErrorStats {
        let n = self.len() as f64;
        let (mut abs_sum, mut smape_sum) = (0.0, 0.0);
        for (&y, &f) in self.series.y.iter().zip(&self.fitted) {
            let abs_e = (y - f).abs();
            abs_sum += abs_e;
            let denom = y.abs() + f.abs();
            if denom != 0.0 {
                smape_sum += 2.0 * abs_e / denom;
            }
        }
        let lag = self.period.max(1);
        let y = &self.series.y;
        let mase_scale = y[lag..]
            .iter()
            .zip(y)
            .map(|(a, b)| (a - b).abs())
            .sum::<f64>()
            / (y.len() - lag) as f64;
        ErrorStats {
            mae: abs_sum / n,
            smape: smape_sum / n,
            mase_scale,
        }
    }

    fn stat_value(&self, code: u8, stats: &mut Option<ErrorStats>) -> Fallible<f64> {
        let get_stats = |s: &mut Option<ErrorStats>| *s.get_or_insert_with(|| self.error_stats());
        finite(match code {
            1 => self.params.alpha,
            2 => self.params.alpha * self.params.beta,
            3 => self.params.gamma * (1.0 - self.params.alpha),
            4 => {
                let st = get_stats(stats);
                if st.mase_scale == 0.0 {
                    return Err(EtsError::ZeroScale);
                }
                st.mae / st.mase_scale
            }
            5 => get_stats(stats).smape,
            6 => get_stats(stats).mae * self.series.scale.up,
            7 => (self.pass.sse / self.len() as f64).sqrt() * self.series.scale.up,
            _ => self.series.step,
        })
    }
}

fn finite(value: f64) -> Fallible<f64> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(EtsError::NonFiniteResult)
    }
}

pub fn forecast_ets_many(targets: &[f64], args: &EtsArgs, budget: BudgetFn) -> Fallible<Vec<f64>> {
    let model = fit_model_bounded(args, args.seasonality, budget)?;
    charge_ops(budget, targets.len())?;
    targets
        .iter()
        .map(|&t| {
            let h = model.steps_to(t)?;
            finite(model.interpolate(h, Model::predict_steps))
        })
        .collect()
}

pub fn forecast_ets_confint_many(
    targets: &[f64],
    args: &EtsArgs,
    confidence_level: f64,
    budget: BudgetFn,
) -> Fallible<Vec<f64>> {
    if !(confidence_level > 0.0 && confidence_level < 1.0) {
        return Err(EtsError::InvalidArgument);
    }
    let model = fit_model_bounded(args, args.seasonality, budget)?;
    let z = dist_value("NORM.S.INV", &[(1.0 + confidence_level) / 2.0])
        .map_err(|_| EtsError::NonFiniteResult)?;
    charge_ops(budget, targets.len())?;
    targets
        .iter()
        .map(|&t| {
            let h = model.steps_to(t)?;
            if h <= STEP_TOLERANCE {
                return Err(EtsError::TargetBeforeEnd);
            }
            finite(z * model.interpolate(h, Model::error_sd_steps))
        })
        .collect()
}

pub fn forecast_ets_seasonality_bounded(args: &EtsArgs, budget: BudgetFn) -> Fallible<usize> {
    let options = parse_options(args, 1.0)?;
    let series = build_series(args, &options, budget)?;
    let period = detect_period_bounded(&series.y, budget)?;
    Ok(if period <= 1 { 0 } else { period })
}

pub fn forecast_ets_stat_many(
    statistic_types: &[f64],
    args: &EtsArgs,
    budget: BudgetFn,
) -> Fallible<Vec<f64>> {
    for &s in statistic_types {
        if !(s.fract() == 0.0 && (1.0..=8.0).contains(&s)) {
            return Err(EtsError::InvalidArgument);
        }
    }
    let model = fit_model_bounded(args, args.seasonality, budget)?;
    let mut cached_stats = None;
    statistic_types
        .iter()
        .map(|&s| model.stat_value(s as u8, &mut cached_stats))
        .collect()
}

#[cfg(test)]
pub fn forecast_ets_cr(target_date: f64, args: &EtsArgs) -> Fallible<f64> {
    let mut unlimited = |_: usize, _: usize| true;
    Ok(forecast_ets_many(&[target_date], args, &mut unlimited)?[0])
}

#[cfg(test)]
pub fn forecast_ets_confint_cr(
    target_date: f64,
    args: &EtsArgs,
    confidence_level: f64,
) -> Fallible<f64> {
    let mut unlimited = |_: usize, _: usize| true;
    Ok(forecast_ets_confint_many(&[target_date], args, confidence_level, &mut unlimited)?[0])
}

#[cfg(test)]
pub fn forecast_ets_seasonality_cr(args: &EtsArgs) -> Fallible<usize> {
    let mut unlimited = |_: usize, _: usize| true;
    forecast_ets_seasonality_bounded(args, &mut unlimited)
}

#[cfg(test)]
pub fn forecast_ets_stat_cr(args: &EtsArgs, statistic_type: f64) -> Fallible<f64> {
    let mut unlimited = |_: usize, _: usize| true;
    Ok(forecast_ets_stat_many(&[statistic_type], args, &mut unlimited)?[0])
}

#[cfg(test)]
#[path = "functions_forecast_ets_cr_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "functions_forecast_ets_cr_excel_tests.rs"]
mod excel_tests;

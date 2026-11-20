//! Clean-room numerics of `FORECAST.ETS`, `FORECAST.ETS.CONFINT`, `FORECAST.ETS.SEASONALITY` and
//! `FORECAST.ETS.STAT` (additive Holt-Winters, ETS(A,A,A)).
//!
//! License: Apache-2.0. Written from public material only; every rule and every choice is recorded
//! in `docs/forecast-ets-cleanroom.md` (section numbers below point there). No source of another
//! implementation of these functions was read. The statistical formulas come from the state-space
//! form of exponential smoothing (Hyndman and Athanasopoulos, *Forecasting: Principles and
//! Practice*, 3rd ed., sections 8.3, 8.5 to 8.7).
//!
//! The functions are pure and take plain numbers. Registering them in the function table, turning
//! cells into numbers and date timelines into step indexes belong to the caller.
//!
//! Every input is bounded before anything is allocated (`MAX_POINTS`, `MAX_SEASONALITY`,
//! `MAX_EVALS`, `MAX_STEPS_AHEAD`): over a limit the result is an error, never a partial value.

#![allow(dead_code)] // not registered yet; the tests call the pure functions directly

use crate::functions_dist::dist_value;

/// Longest seasonal period (hours of a year), section 1 of the specification.
pub const MAX_SEASONALITY: usize = 8_760;
/// Points of one call, before and after the timeline is completed (section 6).
pub const MAX_POINTS: usize = 50_000;
/// Evaluations of the fit loss in one smoothing-parameter search.
pub const MAX_EVALS: usize = 3_000;
/// Farthest target in steps past the last point.
pub const MAX_STEPS_AHEAD: f64 = 1e9;
/// Points left after aggregation below which no model is fitted.
const MIN_POINTS: usize = 3;
/// Share of missing points that is still accepted, in tenths of the grid (3 of 10 is 30%).
const MISSING_TENTHS: usize = 3;
/// Relative distance from an integer at which a timeline ratio still counts as that integer.
const STEP_TOLERANCE: f64 = 1e-6;
/// Longest start of the non-seasonal initial line.
const INIT_LINE_POINTS: usize = 10;
/// Seasons used for the seasonal initial state.
const INIT_SEASONS: usize = 4;
/// Values tried for each smoothing coefficient in the first search stage.
const GRID: [f64; 6] = [0.02, 0.1, 0.25, 0.5, 0.75, 0.95];
/// Starting points the coordinate search is run from.
const STARTS: usize = 3;
/// First step of the coordinate search and the step at which it stops.
const START_STEP: f64 = 0.1;
const MIN_STEP: f64 = 1e-4;
/// A candidate period must reach this share of the highest autocorrelation peak.
const PEAK_SHARE: f64 = 0.9;
/// Difference variance (relative to the mean square of the data) at or below which the data is a line.
const FLAT_VARIANCE: f64 = 1e-20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EtsError {
    /// `values` and `timeline` differ in size (`#N/A`).
    SizeMismatch,
    /// A value or the timeline is NaN or infinite, or the target is (`#VALUE!`).
    NonFinite,
    /// The timeline repeats a time and no aggregation was given (`#VALUE!`).
    DuplicateTimeline,
    /// An argument is outside its set (`#NUM!`).
    InvalidArgument,
    /// No constant step fits the timeline (`#NUM!`).
    NoConstantStep,
    /// More points than `MAX_POINTS` (`#NUM!`).
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

impl EtsError {
    /// The sheet error this stands for.
    pub fn code(self) -> &'static str {
        match self {
            Self::SizeMismatch => "#N/A",
            Self::NonFinite | Self::DuplicateTimeline => "#VALUE!",
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
        if code.fract() != 0.0 {
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

/// The checked optional arguments.
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
}

/// Sort by time, aggregate repeated times, find the step, count and fill the missing points
/// (sections 5.8, 5.10, 5.11, 7).
fn build_series(args: &EtsArgs, options: &Options) -> Fallible<Series> {
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
    if length > MAX_POINTS {
        return Err(EtsError::TooManyPoints);
    }
    if (length - times.len()) * 10 > length * MISSING_TENTHS {
        return Err(EtsError::TooManyMissing);
    }
    Ok(Series {
        y: fill_grid(&slots, &values, length, options.fill_with_zero),
        last,
        step,
    })
}

/// Unique times in order with their values; repeated times are aggregated, or are an error when no
/// aggregation was given.
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

/// Grid slot of every time. A time that is not a whole number of steps from the first has no
/// constant step.
fn grid_slots(times: &[f64], first: f64, step: f64) -> Fallible<Vec<usize>> {
    let mut slots = Vec::with_capacity(times.len());
    for &t in times {
        let ratio = (t - first) / step;
        if !ratio.is_finite() {
            return Err(EtsError::NoConstantStep);
        }
        if ratio > MAX_POINTS as f64 {
            return Err(EtsError::TooManyPoints);
        }
        let slot = ratio.round();
        if (ratio - slot).abs() > STEP_TOLERANCE {
            return Err(EtsError::NoConstantStep);
        }
        slots.push(slot as usize);
    }
    Ok(slots)
}

/// The grid with its missing points set to zero or to the straight line between the neighbors.
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

/// Initial states: level and trend before the first point, and the seasonal values of the period
/// before it (`seasonal[i]` is the value for the slot `i` of the first period).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Init {
    pub level: f64,
    pub trend: f64,
    pub seasonal: Vec<f64>,
}

/// The last state and error totals of one pass over the series.
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
    let beta = params.alpha * params.beta;
    let gamma = params.gamma * (1.0 - params.alpha);
    let period = ring.len();
    let mut slot = 0;
    let mut sse = 0.0;
    for &obs in y {
        let season = if period == 0 { 0.0 } else { ring[slot] };
        let fit = level + trend + season;
        let error = obs - fit;
        on_fit(fit);
        sse += error * error;
        level += trend + params.alpha * error;
        trend += beta * error;
        if period != 0 {
            ring[slot] = season + gamma * error;
            slot += 1;
            if slot == period {
                slot = 0;
            }
        }
    }
    Pass {
        sse,
        level,
        trend,
        next_slot: slot,
    }
}

/// Least squares line `intercept + slope * x`.
fn fit_line(xs: &[f64], ys: &[f64]) -> (f64, f64) {
    let n = xs.len() as f64;
    let (mx, my) = (xs.iter().sum::<f64>() / n, ys.iter().sum::<f64>() / n);
    let (mut sxy, mut sxx) = (0.0, 0.0);
    for (&x, &y) in xs.iter().zip(ys) {
        sxy += (x - mx) * (y - my);
        sxx += (x - mx) * (x - mx);
    }
    let slope = sxy / sxx;
    (my - slope * mx, slope)
}

/// Initial states from the data (section 5.2). `period` is 1 without a season.
pub(crate) fn initial_states(y: &[f64], period: usize) -> Init {
    if period <= 1 {
        let k = y.len().min(INIT_LINE_POINTS);
        let xs: Vec<f64> = (1..=k).map(|t| t as f64).collect();
        let (level, trend) = fit_line(&xs, &y[..k]);
        return Init {
            level,
            trend,
            seasonal: Vec::new(),
        };
    }
    let seasons = (y.len() / period).min(INIT_SEASONS);
    let means: Vec<f64> = (0..seasons)
        .map(|j| y[j * period..(j + 1) * period].iter().sum::<f64>() / period as f64)
        .collect();
    let centers: Vec<f64> = (0..seasons)
        .map(|j| (j * period) as f64 + (period + 1) as f64 / 2.0)
        .collect();
    let (mut level, trend) = fit_line(&centers, &means);
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

/// Smoothing coefficients that minimize the SSE of the one-step errors (section 3.3): a grid, then
/// a coordinate search from the best grid points.
pub(crate) fn optimize(y: &[f64], init: &Init) -> Params {
    let seasonal = !init.seasonal.is_empty();
    let mut ring = Vec::with_capacity(init.seasonal.len());
    let mut evals = 0usize;
    let mut loss = |p: Params, evals: &mut usize| {
        *evals += 1;
        let sse = run(y, init, p, &mut ring, |_| {}).sse;
        if sse.is_finite() {
            sse
        } else {
            f64::INFINITY
        }
    };
    let gammas: &[f64] = if seasonal { &GRID } else { &[0.0] };
    let mut best: Vec<(f64, Params)> = Vec::with_capacity(STARTS + 1);
    for &alpha in &GRID {
        for &beta in &GRID {
            for &gamma in gammas {
                let p = Params { alpha, beta, gamma };
                let value = loss(p, &mut evals);
                // Keep the best few; an earlier point wins a tie.
                let at = best
                    .iter()
                    .position(|&(v, _)| value < v)
                    .unwrap_or(best.len());
                if at < STARTS {
                    best.insert(at, (value, p));
                    best.truncate(STARTS);
                }
            }
        }
    }
    let budget = (MAX_EVALS - evals) / best.len();
    let mut winner = best[0];
    for &(start_value, start) in &best {
        let found = coordinate_search(start, start_value, seasonal, budget, &mut |p, e| loss(p, e));
        if found.0 < winner.0 {
            winner = found;
        }
    }
    winner.1
}

/// Compass search in `[0, 1]^k`: move to the best neighbor one step away, halve the step when no
/// neighbor is better, stop at `MIN_STEP` or when the budget is used.
fn coordinate_search(
    start: Params,
    start_value: f64,
    seasonal: bool,
    budget: usize,
    loss: &mut dyn FnMut(Params, &mut usize) -> f64,
) -> (f64, Params) {
    let dims = if seasonal { 3 } else { 2 };
    let mut at = start;
    let mut value = start_value;
    let mut step = START_STEP;
    let mut evals = 0usize;
    while step >= MIN_STEP && evals + 2 * dims <= budget {
        let mut best: Option<(f64, Params)> = None;
        for dim in 0..dims {
            for sign in [-1.0, 1.0] {
                let mut p = at;
                let coord = match dim {
                    0 => &mut p.alpha,
                    1 => &mut p.beta,
                    _ => &mut p.gamma,
                };
                *coord = (*coord + sign * step).clamp(0.0, 1.0);
                if p == at {
                    continue;
                }
                let v = loss(p, &mut evals);
                if v < best.map_or(value * (1.0 - 1e-12), |b| b.0) {
                    best = Some((v, p));
                }
            }
        }
        match best {
            Some((v, p)) => (value, at) = (v, p),
            None => step /= 2.0,
        }
    }
    (value, at)
}

/// Normalized autocorrelation peaks of the first differences and the period they imply
/// (section 4). 1 means no season.
pub(crate) fn detect_period(y: &[f64]) -> usize {
    let n = y.len();
    if n < 6 {
        return 1;
    }
    let mut d: Vec<f64> = y.windows(2).map(|w| w[1] - w[0]).collect();
    let mean = d.iter().sum::<f64>() / d.len() as f64;
    d.iter_mut().for_each(|v| *v -= mean);
    let c0: f64 = d.iter().map(|v| v * v).sum();
    let mean_square = y.iter().map(|v| v * v).sum::<f64>() / n as f64;
    if c0.is_nan() || c0 <= FLAT_VARIANCE * mean_square * d.len() as f64 {
        return 1;
    }
    let max_lag = MAX_SEASONALITY.min(n / 2);
    let r: Vec<f64> = (0..=max_lag + 1)
        .map(|k| match k {
            0 => 1.0,
            k if k > max_lag => f64::NEG_INFINITY,
            k => d[k..].iter().zip(&d).map(|(a, b)| a * b).sum::<f64>() / c0,
        })
        .collect();
    let bound = 1.96 / (d.len() as f64).sqrt();
    let peaks: Vec<usize> = (2..=max_lag)
        .filter(|&k| r[k] > bound && r[k] >= r[k - 1] && r[k] > r[k + 1])
        .collect();
    let top = peaks
        .iter()
        .map(|&k| r[k])
        .fold(f64::NEG_INFINITY, f64::max);
    peaks
        .into_iter()
        .find(|&k| r[k] >= PEAK_SHARE * top)
        .unwrap_or(1)
}

/// The period of the model for the requested seasonality.
fn resolve_period(season: Season, y: &[f64]) -> Fallible<usize> {
    match season {
        Season::Off => Ok(1),
        Season::Detect => Ok(detect_period(y)),
        Season::Period(m) if y.len() >= 2 * m => Ok(m),
        Season::Period(_) => Err(EtsError::TooFewPoints),
    }
}

/// A fitted model: coefficients, the last state and what the one-step errors need.
struct Model {
    series: Series,
    period: usize,
    params: Params,
    pass: Pass,
    ring: Vec<f64>,
    fitted: Vec<f64>,
}

fn fit_model(args: &EtsArgs, season: f64) -> Fallible<Model> {
    let options = parse_options(args, season)?;
    let series = build_series(args, &options)?;
    let period = resolve_period(options.season, &series.y)?;
    let init = initial_states(&series.y, period);
    let params = optimize(&series.y, &init);
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

    /// Steps from the last point to `target`; the target may not be before it.
    fn steps_to(&self, target: f64) -> Fallible<f64> {
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

    /// Prediction `h` whole steps past the last point; step 0 is the one-step prediction of the
    /// last point (section 5.9).
    fn predict_steps(&self, h: usize) -> f64 {
        if h == 0 {
            return self.fitted[self.len() - 1];
        }
        let season = if self.seasonal() {
            self.ring[(self.pass.next_slot + h - 1) % self.period]
        } else {
            0.0
        };
        self.pass.level + h as f64 * self.pass.trend + season
    }

    /// Standard deviation of the `h`-step error, whole `h` (section 5.7).
    fn error_sd_steps(&self, h: usize) -> f64 {
        let sigma2 = self.pass.sse / self.len() as f64;
        if h <= 1 {
            return sigma2.sqrt();
        }
        let Params { alpha, beta, gamma } = self.params;
        let (a, b, g) = (alpha, alpha * beta, gamma * (1.0 - alpha));
        let h = h as f64;
        let mut factor = 1.0 + (h - 1.0) * (a * a + a * b * h + b * b * h * (2.0 * h - 1.0) / 6.0);
        if self.seasonal() {
            let m = self.period as f64;
            let k = ((h - 1.0) / m).floor();
            factor += g * k * (2.0 * a + g + b * m * (k + 1.0));
        }
        (sigma2 * factor).sqrt()
    }

    /// `at` at a possibly fractional step count: the straight line between the two whole steps.
    fn interpolate(&self, h: f64, at: impl Fn(&Self, usize) -> f64) -> f64 {
        let lo = h.floor();
        let frac = h - lo;
        let lo_value = at(self, lo as usize);
        if frac <= STEP_TOLERANCE {
            lo_value
        } else {
            lo_value + (at(self, lo as usize + 1) - lo_value) * frac
        }
    }
}

fn finite(value: f64) -> Fallible<f64> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(EtsError::NonFiniteResult)
    }
}

/// `FORECAST.ETS`: the predicted value at `target_date`.
pub fn forecast_ets_cr(target_date: f64, args: &EtsArgs) -> Fallible<f64> {
    let model = fit_model(args, args.seasonality)?;
    let h = model.steps_to(target_date)?;
    finite(model.interpolate(h, Model::predict_steps))
}

/// `FORECAST.ETS.CONFINT`: the half width of the interval around the forecast.
pub fn forecast_ets_confint_cr(
    target_date: f64,
    args: &EtsArgs,
    confidence_level: f64,
) -> Fallible<f64> {
    if !(confidence_level > 0.0 && confidence_level < 1.0) {
        return Err(EtsError::InvalidArgument);
    }
    let model = fit_model(args, args.seasonality)?;
    let h = model.steps_to(target_date)?;
    let z = dist_value("NORM.S.INV", &[(1.0 + confidence_level) / 2.0])
        .map_err(|_| EtsError::NonFiniteResult)?;
    finite(z * model.interpolate(h, Model::error_sd_steps))
}

/// `FORECAST.ETS.SEASONALITY`: the period the detection finds (1 for none). The `seasonality`
/// field of `args` is ignored.
pub fn forecast_ets_seasonality_cr(args: &EtsArgs) -> Fallible<usize> {
    let options = parse_options(args, 1.0)?;
    let series = build_series(args, &options)?;
    Ok(detect_period(&series.y))
}

/// `FORECAST.ETS.STAT`: statistic 1 to 8 (alpha, beta, gamma, MASE, SMAPE, MAE, RMSE, step).
pub fn forecast_ets_stat_cr(args: &EtsArgs, statistic_type: f64) -> Fallible<f64> {
    if !(statistic_type.fract() == 0.0 && (1.0..=8.0).contains(&statistic_type)) {
        return Err(EtsError::InvalidArgument);
    }
    let model = fit_model(args, args.seasonality)?;
    let n = model.len() as f64;
    let errors = || {
        model
            .series
            .y
            .iter()
            .zip(&model.fitted)
            .map(|(y, f)| (y, f, y - f))
    };
    finite(match statistic_type as u8 {
        1 => model.params.alpha,
        2 => model.params.beta,
        3 => model.params.gamma,
        4 => {
            let lag = model.period.max(1);
            let y = &model.series.y;
            let scale = y[lag..]
                .iter()
                .zip(y)
                .map(|(a, b)| (a - b).abs())
                .sum::<f64>()
                / (y.len() - lag) as f64;
            if scale == 0.0 {
                return Err(EtsError::ZeroScale);
            }
            errors().map(|(_, _, e)| e.abs()).sum::<f64>() / n / scale
        }
        5 => {
            errors()
                .map(|(y, f, e)| {
                    let denominator = y.abs() + f.abs();
                    if denominator == 0.0 {
                        0.0
                    } else {
                        200.0 * e.abs() / denominator
                    }
                })
                .sum::<f64>()
                / n
        }
        6 => errors().map(|(_, _, e)| e.abs()).sum::<f64>() / n,
        7 => (model.pass.sse / n).sqrt(),
        _ => model.series.step,
    })
}

#[cfg(test)]
#[path = "functions_forecast_ets_cr_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "functions_forecast_ets_cr_excel_tests.rs"]
mod excel_tests;

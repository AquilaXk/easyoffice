//! Sheet-engine adapter for `FORECAST.ETS`, `FORECAST.ETS.CONFINT`, `FORECAST.ETS.SEASONALITY` and
//! `FORECAST.ETS.STAT`: reads dynamic array arguments, normalizes calendar-month date timelines,
//! connects evaluation step and memory budgets, and maps errors to sheet values.
//!
//! All exponential smoothing numerics live in `functions_forecast_ets_cr` (Apache-2.0 clean-room,
//! `docs/forecast-ets-cleanroom.md`).

use crate::array::{Env, Grid, Val};
use crate::array_eval::ValResult;
use crate::functions_date_fin::{civil_from_serial, days_in_month, serial_from_civil};
use crate::functions_forecast_ets_cr::{
    forecast_ets_confint_many, forecast_ets_many, forecast_ets_seasonality_bounded,
    forecast_ets_stat_many, BudgetFn, EtsArgs, EtsError, MAX_SEASONALITY,
};
use crate::{EvalContext, EvaluatedCellValue as V, Expr, FormulaError as E, SheetEngine};
use std::sync::Arc;

const DEFAULT_CONFIDENCE: f64 = 0.95;
/// Serial bounds within which `civil_from_serial` and `serial_from_civil` represent calendar dates
/// without integer overflow.
const MIN_DATE_SERIAL: f64 = -700_000.0;
const MAX_DATE_SERIAL: f64 = 3_000_000.0;
const MAX_TARGET_SERIAL: f64 = 1e9;
const TOD_TOLERANCE: f64 = 1e-9;

#[derive(Clone, Copy)]
enum Kind {
    Forecast,
    Confint,
    Seasonality,
    Stat,
}

/// Positions of the arguments of one function (`None`: it has no such argument).
struct Layout {
    kind: Kind,
    min_args: usize,
    max_args: usize,
    target: Option<usize>,
    values: usize,
    timeline: usize,
    stat_type: Option<usize>,
    confidence: Option<usize>,
    seasonality: Option<usize>,
    completion: usize,
    aggregation: usize,
}

fn layout(name: &str) -> Option<Layout> {
    Some(match name {
        "FORECAST.ETS" => Layout {
            kind: Kind::Forecast,
            min_args: 3,
            max_args: 6,
            target: Some(0),
            values: 1,
            timeline: 2,
            stat_type: None,
            confidence: None,
            seasonality: Some(3),
            completion: 4,
            aggregation: 5,
        },
        "FORECAST.ETS.CONFINT" => Layout {
            kind: Kind::Confint,
            min_args: 3,
            max_args: 7,
            target: Some(0),
            values: 1,
            timeline: 2,
            stat_type: None,
            confidence: Some(3),
            seasonality: Some(4),
            completion: 5,
            aggregation: 6,
        },
        "FORECAST.ETS.SEASONALITY" => Layout {
            kind: Kind::Seasonality,
            min_args: 2,
            max_args: 4,
            target: None,
            values: 0,
            timeline: 1,
            stat_type: None,
            confidence: None,
            seasonality: None,
            completion: 2,
            aggregation: 3,
        },
        "FORECAST.ETS.STAT" => Layout {
            kind: Kind::Stat,
            min_args: 3,
            max_args: 6,
            target: None,
            values: 0,
            timeline: 1,
            stat_type: Some(2),
            confidence: None,
            seasonality: Some(3),
            completion: 4,
            aggregation: 5,
        },
        _ => return None,
    })
}

/// A matrix of numbers: the shape and the numbers row by row.
struct Numbers {
    rows: usize,
    cols: usize,
    data: Vec<f64>,
}

/// The numbers of a series grid: an error is that error, a text, blank or boolean is `#VALUE!`, a
/// number that is not finite is `#NUM!`.
fn numbers(grid: &Grid) -> Result<Numbers, E> {
    let data = grid
        .data
        .iter()
        .map(|cell| match cell {
            V::Number(n) if n.is_finite() => Ok(*n),
            V::Number(_) => Err(E::Num),
            V::Error(e) => Err(*e),
            _ => Err(E::Value),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Numbers {
        rows: grid.rows,
        cols: grid.cols,
        data,
    })
}

/// `x` when it is a whole number from `lo` to `hi`, else `#NUM!`.
fn whole(x: f64, lo: f64, hi: f64) -> Result<f64, E> {
    if x.is_finite() && x.fract() == 0.0 && (lo..=hi).contains(&x) {
        Ok(x)
    } else {
        Err(E::Num)
    }
}

fn error_value(error: EtsError) -> E {
    match error {
        EtsError::SizeMismatch => E::Na,
        EtsError::NonFinite
        | EtsError::DuplicateTimeline
        | EtsError::BudgetExceeded
        | EtsError::TooManyPoints => E::Value,
        _ => E::Num,
    }
}

/// Maps a variable-length calendar timeline (monthly, quarterly, semi-annual or annual date
/// serials sharing one day-of-month or end-of-month anchor) onto continuous month coordinates
/// `year * 12 + (month - 1)`. Returns `None` when the timeline is not such a calendar series.
fn calendar_month_timeline(
    timeline: &[f64],
    targets: Option<&[f64]>,
) -> Option<(Vec<f64>, Vec<f64>)> {
    let first = *timeline.first()?;
    if !(MIN_DATE_SERIAL..=MAX_DATE_SERIAL).contains(&first) {
        return None;
    }
    let tod = first - first.floor();
    let mut ymd = Vec::with_capacity(timeline.len());
    let mut all_eom = true;
    let mut max_day = 1_i64;
    for &t in timeline {
        if !(MIN_DATE_SERIAL..=MAX_DATE_SERIAL).contains(&t) {
            return None;
        }
        let day_serial = t.floor();
        if ((t - day_serial) - tod).abs() > TOD_TOLERANCE {
            return None;
        }
        let (y, m, d) = civil_from_serial(day_serial as i64);
        all_eom &= d == days_in_month(y, m);
        max_day = max_day.max(d);
        ymd.push((y, m, d));
    }
    let anchor_day = if all_eom {
        31
    } else if ymd
        .iter()
        .all(|&(y, m, d)| d == max_day.min(days_in_month(y, m)))
    {
        max_day
    } else {
        return None;
    };
    let month_timeline: Vec<f64> = ymd
        .iter()
        .map(|&(y, m, _)| (y * 12 + (m - 1)) as f64)
        .collect();
    let last_serial = timeline.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let last_month = month_timeline
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);

    let anchor_serial = |k: i64| -> f64 {
        let y = k.div_euclid(12);
        let m = k.rem_euclid(12) + 1;
        let d = anchor_day.min(days_in_month(y, m));
        serial_from_civil(y, m, d) as f64 + tod
    };
    let mapped_targets = match targets {
        None => Vec::new(),
        Some(ts) => ts
            .iter()
            .map(|&target| {
                if !target.is_finite() {
                    return f64::NAN;
                }
                if target > MAX_TARGET_SERIAL {
                    return last_month + (target - last_serial) / 30.436_875;
                }
                if target < MIN_DATE_SERIAL {
                    return last_month - 1.0;
                }
                let (ty, tm, _) = civil_from_serial(target.floor() as i64);
                let mut k = ty * 12 + (tm - 1);
                while anchor_serial(k) > target {
                    k -= 1;
                }
                while anchor_serial(k + 1) <= target {
                    k += 1;
                }
                let s0 = anchor_serial(k);
                let s1 = anchor_serial(k + 1);
                k as f64 + (target - s0) / (s1 - s0)
            })
            .collect(),
    };
    Some((month_timeline, mapped_targets))
}

fn eval_kind(
    kind: Kind,
    args: &EtsArgs<'_>,
    selector: &[f64],
    confidence: f64,
    budget: BudgetFn<'_>,
) -> Result<Vec<f64>, EtsError> {
    match kind {
        Kind::Forecast => forecast_ets_many(selector, args, budget),
        Kind::Confint => forecast_ets_confint_many(selector, args, confidence, budget),
        Kind::Seasonality => Ok(vec![forecast_ets_seasonality_bounded(args, budget)? as f64]),
        Kind::Stat => forecast_ets_stat_many(selector, args, budget),
    }
}

impl SheetEngine {
    fn numbers_arg(
        &self,
        arg: &Expr,
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Arc<Grid>, E> {
        self.eval_val(arg, env, ctx, depth + 1)?.into_grid()
    }

    /// `FORECAST.ETS`, `FORECAST.ETS.CONFINT`, `FORECAST.ETS.SEASONALITY` and `FORECAST.ETS.STAT`.
    /// An array of target dates (or statistic types) returns an array of the same shape.
    pub(crate) fn fn_forecast_ets(
        &self,
        name: &str,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        let lay = layout(name).ok_or(E::Name)?;
        if !(lay.min_args..=lay.max_args).contains(&args.len()) {
            return Err(E::Value);
        }
        let selector = match lay.target.or(lay.stat_type) {
            Some(i) => Some(self.numbers_arg(&args[i], env, ctx, depth)?),
            None => None,
        };
        let values = self.numbers_arg(&args[lay.values], env, ctx, depth)?;
        let timeline = self.numbers_arg(&args[lay.timeline], env, ctx, depth)?;
        let confidence = match lay.confidence {
            Some(i) => self.arg_number(args, i, DEFAULT_CONFIDENCE, env, ctx, depth)?,
            None => DEFAULT_CONFIDENCE,
        };
        let seasonality = match lay.seasonality {
            Some(i) => self.arg_number(args, i, 1.0, env, ctx, depth)?,
            None => 1.0,
        };
        let completion = self.arg_number(args, lay.completion, 1.0, env, ctx, depth)?;
        let aggregation = self.arg_number(args, lay.aggregation, 1.0, env, ctx, depth)?;

        if (values.rows, values.cols) != (timeline.rows, timeline.cols) {
            return Err(E::Na);
        }
        let values = numbers(&values)?;
        let timeline = numbers(&timeline)?;
        let selector = selector.as_deref().map(numbers).transpose()?;
        let seasonality = whole(seasonality, 0.0, MAX_SEASONALITY as f64)?;
        let completion = whole(completion, 0.0, 1.0)?;
        let aggregation = whole(aggregation, 0.0, 7.0)?;
        if matches!(lay.kind, Kind::Confint) && !(confidence > 0.0 && confidence < 1.0) {
            return Err(E::Num);
        }
        if let Some(selector) = &selector {
            if selector.data.is_empty() {
                return Err(E::Num);
            }
            match lay.kind {
                Kind::Stat => {
                    for &t in &selector.data {
                        whole(t, 1.0, 8.0)?;
                    }
                }
                _ => {
                    let end = timeline.data.iter().copied().fold(f64::MIN, f64::max);
                    if selector.data.iter().any(|&t| t < end) {
                        return Err(E::Num);
                    }
                }
            }
        }

        let sel_slice = selector.as_ref().map_or(&[][..], |s| &s.data);
        let ets_args = EtsArgs {
            values: &values.data,
            timeline: &timeline.data,
            seasonality,
            data_completion: completion,
            aggregation: Some(aggregation),
        };
        let mut budget = |steps: usize, bytes: usize| -> bool {
            (steps == 0 || ctx.charge_steps(steps).is_ok())
                && (bytes == 0 || ctx.charge_array_extra_bytes(bytes).is_ok())
        };
        let result = match eval_kind(lay.kind, &ets_args, sel_slice, confidence, &mut budget) {
            Err(EtsError::NoConstantStep) => {
                let target_input =
                    matches!(lay.kind, Kind::Forecast | Kind::Confint).then_some(sel_slice);
                let (month_tl, month_targets) =
                    calendar_month_timeline(&timeline.data, target_input).ok_or(E::Num)?;
                let month_args = EtsArgs {
                    timeline: &month_tl,
                    ..ets_args
                };
                let mapped_sel = if target_input.is_some() {
                    &month_targets
                } else {
                    sel_slice
                };
                eval_kind(lay.kind, &month_args, mapped_sel, confidence, &mut budget)
                    .map_err(error_value)?
            }
            other => other.map_err(error_value)?,
        };

        let cells: Vec<V> = result
            .into_iter()
            .map(|x| {
                if x.is_finite() {
                    V::Number(x)
                } else {
                    V::Error(E::Num)
                }
            })
            .collect();
        match selector {
            Some(shape) if cells.len() > 1 => Ok(Val::grid(Grid::from_rows_checked(
                ctx, shape.rows, shape.cols, cells,
            )?)),
            _ => Ok(Val::scalar(
                cells.into_iter().next().unwrap_or(V::Error(E::Value)),
            )),
        }
    }
}

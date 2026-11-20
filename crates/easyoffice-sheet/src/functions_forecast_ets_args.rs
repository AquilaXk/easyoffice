//! `FORECAST.ETS`, `FORECAST.ETS.CONFINT`, `FORECAST.ETS.SEASONALITY` and `FORECAST.ETS.STAT` (PD-272):
//! the arguments and errors of the function documentation, over the calculation in
//! `functions_forecast_ets.rs`.
//!
//! That file is the only one with code under the MPL-2.0 (the calculation, ported from a spreadsheet
//! program; to be replaced by a clean-room one). This file is this engine's own code and only reads
//! the arguments, applies the documented rules, and turns the numbers and errors of the calculation
//! into values.
//!
//! Rules from the function documentation: the target date must not be before the end of the
//! timeline (`#NUM!`), the timeline needs a constant step (`#NUM!`), the seasonality is 0 (none), 1
//! (detect, the default) or a whole number up to 8,760 (`#NUM!` otherwise), `data_completion` is 0 or
//! 1, `aggregation` is 1 to 7 (the documentation names 0 as the default average; 0 is the average
//! too), the confidence level is between 0 and 1 without the ends (default 0.95), a values range of
//! another size than the timeline is `#N/A`, and `statistic_type` is 1 to 8 (alpha, beta, gamma,
//! MASE, SMAPE, MAE, RMSE, step size). The documentation gives no error for an argument outside its
//! list (`aggregation`, `data_completion`, `statistic_type`) or for a series too short for its
//! seasonality: this engine answers `#NUM!`. A text, blank or boolean in the series is `#VALUE!`
//! and an error there is that error. The budget limits (series, prediction steps, work) answer
//! `#VALUE!`. The prediction interval of a seasonal series is simulated; the random numbers come
//! from a generator with a fixed seed, so a cell has the same value in every calculation.

use crate::array::{Env, Grid, Val};
use crate::array_eval::ValResult;
use crate::functions_date_fin::civil_from_serial;
use crate::functions_dist::dist_value;
use crate::functions_forecast_ets::{
    Budget, FormulaError as EtsError, ScETSForecastCalculation, ScETSType, ScInterpreterContext,
};
use crate::{EvalContext, EvaluatedCellValue as V, Expr, FormulaError as E, SheetEngine};
use std::sync::Arc;

/// Longest seasonal period of the documentation (one year of hours).
const MAX_SEASONALITY: f64 = 8760.0;
const DEFAULT_CONFIDENCE: f64 = 0.95;
const RNG_SEED: u64 = 0x2545_F491_4F6C_DD1D;

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
    if x.fract() == 0.0 && (lo..=hi).contains(&x) {
        Ok(x)
    } else {
        Err(E::Num)
    }
}

fn gaussinv(p: f64) -> f64 {
    dist_value("NORM.S.INV", &[p]).unwrap_or(f64::NAN)
}

/// The error value of an error of the calculation.
fn error_value(error: EtsError) -> E {
    match error {
        EtsError::NoValue | EtsError::IllegalFPOperation | EtsError::IllegalArgument => E::Num,
        EtsError::DivisionByZero => E::DivByZero,
        EtsError::NoError | EtsError::UnknownState | EtsError::Aborted => E::Value,
    }
}

/// What the calculation takes once the arguments are read and checked.
struct EtsRequest<'a> {
    e_ets_type: ScETSType,
    /// The timeline.
    mat_x: &'a [f64],
    /// The values.
    mat_y: &'a [f64],
    /// The target dates, or the statistic types of the `EtsStat*` kinds; unused for `EtsSeason`.
    t_mat: &'a [f64],
    /// Seasonal period: 0 none, 1 detect, more is that length.
    n_smpl_in_prd: usize,
    b_data_completion: bool,
    /// 1 AVERAGE, 2 COUNT, 3 COUNTA, 4 MAX, 5 MEDIAN, 6 MIN, 7 SUM.
    n_aggregation: i32,
    f_pi_level: f64,
}

/// The second half of `ScInterpreter::ScForecast_Ets`: runs the calculation the request names and
/// gives the numbers of the result matrix (one for `EtsSeason`).
fn sc_forecast_ets(
    req: &EtsRequest<'_>,
    context: ScInterpreterContext<'_>,
) -> Result<Vec<f64>, EtsError> {
    use ScETSType::{EtsAdd, EtsMult, EtsPIAdd, EtsPIMult, EtsSeason, EtsStatAdd, EtsStatMult};
    let e_ets_type = req.e_ets_type;
    if matches!(e_ets_type, EtsPIAdd | EtsPIMult) && !(0.0..=1.0).contains(&req.f_pi_level) {
        return Err(EtsError::IllegalArgument);
    }
    let is_stat = matches!(e_ets_type, EtsStatAdd | EtsStatMult);
    if is_stat && req.t_mat.iter().any(|&t| !(1..=9).contains(&(t as i32))) {
        return Err(EtsError::IllegalArgument);
    }
    let needs_target = e_ets_type != EtsSeason;
    if needs_target && req.t_mat.is_empty() {
        return Err(EtsError::IllegalArgument);
    }

    let mut a_ets_calc = ScETSForecastCalculation::new(req.mat_x.len(), context);
    if !a_ets_calc.preprocess_data_range(
        req.mat_x,
        req.mat_y,
        req.n_smpl_in_prd,
        req.b_data_completion,
        req.n_aggregation,
        (needs_target && !is_stat).then_some(req.t_mat),
        e_ets_type,
    ) {
        return Err(a_ets_calc.get_error());
    }

    let result = match e_ets_type {
        EtsAdd | EtsMult => a_ets_calc.get_forecast_range(req.t_mat),
        EtsPIAdd | EtsPIMult if a_ets_calc.b_eds() => {
            a_ets_calc.get_eds_prediction_intervals(req.t_mat, req.f_pi_level)
        }
        EtsPIAdd | EtsPIMult => a_ets_calc.get_ets_prediction_intervals(req.t_mat, req.f_pi_level),
        EtsStatAdd | EtsStatMult => a_ets_calc.get_statistic_value(req.t_mat),
        EtsSeason => vec![a_ets_calc.get_samples_in_period()],
    };
    match a_ets_calc.get_error() {
        EtsError::NoError => Ok(result),
        error => Err(error),
    }
}

/// The percentile of a list with linear interpolation (`ScInterpreter::GetPercentile`).
fn percentile(list: &mut [f64], p: f64) -> f64 {
    if list.len() == 1 {
        return list[0];
    }
    list.sort_by(f64::total_cmp);
    let position = p * (list.len() - 1) as f64;
    // a position one rounding step below a whole number is that number
    let nearest = position.round();
    let floor = if (position - nearest).abs() <= 5e-15 * nearest.abs() {
        nearest
    } else {
        position.floor()
    };
    let index = (floor as usize).min(list.len() - 1);
    let diff = position - floor;
    if diff <= 0.0 || index + 1 >= list.len() {
        list[index]
    } else {
        list[index] + diff * (list[index + 1] - list[index])
    }
}

/// A generator of numbers in [0, 1) with a fixed seed (SplitMix64).
struct FixedRng(u64);

impl FixedRng {
    fn next_f64(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
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

    /// The four functions: `FORECAST.ETS(target_date, values, timeline, [seasonality],
    /// [data_completion], [aggregation])`, `FORECAST.ETS.CONFINT(target_date, values, timeline,
    /// [confidence_level], [seasonality], [data_completion], [aggregation])`,
    /// `FORECAST.ETS.SEASONALITY(values, timeline, [data_completion], [aggregation])` and
    /// `FORECAST.ETS.STAT(values, timeline, statistic_type, [seasonality], [data_completion],
    /// [aggregation])`. An array of target dates (statistic types) gives an array of the same shape.
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

        // a values range of another size than the timeline
        if (values.rows, values.cols) != (timeline.rows, timeline.cols) {
            return Err(E::Na);
        }
        let values = numbers(&values)?;
        let timeline = numbers(&timeline)?;
        let selector = selector.as_deref().map(numbers).transpose()?;
        let seasonality = whole(seasonality, 0.0, MAX_SEASONALITY)?;
        let completion = whole(completion, 0.0, 1.0)?;
        let aggregation = whole(aggregation, 0.0, 7.0)?;
        if matches!(lay.kind, Kind::Confint) && !(confidence > 0.0 && confidence < 1.0) {
            return Err(E::Num);
        }
        if let Some(selector) = &selector {
            match lay.kind {
                Kind::Stat => {
                    for &t in &selector.data {
                        whole(t, 1.0, 8.0)?;
                    }
                }
                _ => {
                    // the target date is not before the end of the timeline
                    let end = timeline.data.iter().copied().fold(f64::MIN, f64::max);
                    if selector.data.iter().any(|&t| t < end) {
                        return Err(E::Num);
                    }
                }
            }
        }

        let request = EtsRequest {
            e_ets_type: match lay.kind {
                Kind::Forecast => ScETSType::EtsAdd,
                Kind::Confint => ScETSType::EtsPIAdd,
                Kind::Seasonality => ScETSType::EtsSeason,
                Kind::Stat => ScETSType::EtsStatAdd,
            },
            mat_x: &timeline.data,
            mat_y: &values.data,
            t_mat: selector.as_ref().map_or(&[], |s| &s.data),
            n_smpl_in_prd: seasonality as usize,
            b_data_completion: completion != 0.0,
            // the documented default 0 is the average, which is 1 for the calculation
            n_aggregation: if aggregation == 0.0 {
                1
            } else {
                aggregation as i32
            },
            f_pi_level: confidence,
        };
        let mut rng = FixedRng(RNG_SEED);
        let mut rand = || rng.next_f64();
        let mut budget = |charge: Budget| match charge {
            Budget::Steps(n) => ctx.charge_steps(n).is_ok(),
            Budget::Bytes(n) => ctx.charge_array_extra_bytes(n).is_ok(),
        };
        let context = ScInterpreterContext {
            civil_from_serial,
            gaussinv,
            percentile,
            rand: &mut rand,
            budget: &mut budget,
        };
        let result = sc_forecast_ets(&request, context).map_err(error_value)?;

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

/// The test vectors of the original (`tests/data/forecast_ets_vectors.txt`) through the calculation, and
/// the limits of the calculation. The values are the original program's: where the function
/// documentation gives another error, the integration tests (`tests/sheet_forecast_ets.rs`) hold
/// the documented one and the cases here only check that an error comes.
#[cfg(test)]
mod tests {
    use super::*;

    const VECTORS: &str = include_str!("../tests/data/forecast_ets_vectors.txt");

    struct Case {
        name: String,
        kinds: Vec<ScETSType>,
        select: Vec<f64>,
        values: Vec<f64>,
        timeline: Vec<f64>,
        params: Vec<f64>,
        expect: Vec<String>,
    }

    fn numbers_of(line: &str, key: &str) -> Vec<f64> {
        let list = line.strip_prefix(key).unwrap_or_default();
        list.split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect()
    }

    fn cases() -> Vec<Case> {
        let mut out: Vec<Case> = Vec::new();
        for line in VECTORS.lines().filter(|l| !l.starts_with('#')) {
            let word = line.split_whitespace().next().unwrap_or("");
            match word {
                "case" => {
                    let mut parts = line.split_whitespace().skip(1);
                    let name = parts.next().unwrap().to_string();
                    let kinds = match parts.next().unwrap() {
                        "add" => vec![ScETSType::EtsAdd],
                        "mult" => vec![ScETSType::EtsMult],
                        "both" => vec![ScETSType::EtsAdd, ScETSType::EtsMult],
                        _ => vec![ScETSType::EtsStatMult],
                    };
                    out.push(Case {
                        name,
                        kinds,
                        select: vec![],
                        values: vec![],
                        timeline: vec![],
                        params: vec![],
                        expect: vec![],
                    });
                }
                "select" => out.last_mut().unwrap().select = numbers_of(line, "select"),
                "values" => out.last_mut().unwrap().values = numbers_of(line, "values"),
                "timeline" => out.last_mut().unwrap().timeline = numbers_of(line, "timeline"),
                "params" => out.last_mut().unwrap().params = numbers_of(line, "params"),
                "expect" => {
                    out.last_mut().unwrap().expect =
                        line.split_whitespace().skip(1).map(String::from).collect();
                }
                _ => {}
            }
        }
        out
    }

    /// Runs a calculation with a budget that never ends (`limit` steps) and a fixed generator.
    fn run(
        kind: ScETSType,
        (select, values, timeline): (&[f64], &[f64], &[f64]),
        params: &[f64],
        limit: usize,
    ) -> Result<Vec<f64>, EtsError> {
        let mut rng = FixedRng(RNG_SEED);
        let mut rand = || rng.next_f64();
        let mut steps = 0usize;
        let mut budget = |charge: Budget| match charge {
            Budget::Steps(n) => {
                steps += n;
                steps <= limit
            }
            Budget::Bytes(n) => n <= 128 * 1024 * 1024,
        };
        let context = ScInterpreterContext {
            civil_from_serial,
            gaussinv,
            percentile,
            rand: &mut rand,
            budget: &mut budget,
        };
        let request = EtsRequest {
            e_ets_type: kind,
            mat_x: timeline,
            mat_y: values,
            t_mat: select,
            n_smpl_in_prd: params.first().map_or(1, |&p| p as usize),
            b_data_completion: params.get(1).is_none_or(|&p| p != 0.0),
            n_aggregation: params.get(2).map_or(1, |&p| p as i32),
            f_pi_level: 0.95,
        };
        sc_forecast_ets(&request, context)
    }

    const NO_LIMIT: usize = usize::MAX;

    #[test]
    fn the_original_test_vectors_give_the_original_values() {
        let cases = cases();
        assert_eq!(cases.len(), 21);
        for case in &cases {
            for &kind in &case.kinds {
                let got = run(
                    kind,
                    (&case.select, &case.values, &case.timeline),
                    &case.params,
                    NO_LIMIT,
                );
                if case.expect[0].starts_with('#') {
                    assert!(got.is_err(), "{} {kind:?}: {got:?}", case.name);
                    continue;
                }
                let got = got.unwrap_or_else(|e| panic!("{} {kind:?}: {e:?}", case.name));
                assert_eq!(got.len(), case.expect.len(), "{} {kind:?}", case.name);
                for (i, (g, e)) in got.iter().zip(&case.expect).enumerate() {
                    let e: f64 = e.parse().unwrap();
                    // the original compares 9 significant digits
                    assert!(
                        (g - e).abs() <= 1e-9 * e.abs().max(1e-3),
                        "{} {kind:?} #{i}: {g} against {e}",
                        case.name
                    );
                }
            }
        }
    }
}

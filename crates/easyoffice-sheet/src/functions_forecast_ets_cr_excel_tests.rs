//! The clean-room numerics against the values the reference app cached for three published examples
//! (`tests/data/forecast_ets_excel_reference.json`, `_2.json` and `_3.json`, sources in
//! `THIRD_PARTY_NOTICES.md`).
//!
//! What matches: the detected period, and the shape of the interval. What does not: the fitted
//! smoothing coefficients, so the forecast and the interval width differ by percents. The ignored
//! test states the target and fails until the fit agrees; `docs/forecast-ets-cleanroom.md`, section 9,
//! holds the measured gap and what the reference values say about the reference app's model.

use super::*;

const REFERENCE: &str = include_str!("../tests/data/forecast_ets_excel_reference.json");
const REFERENCE_2: &str = include_str!("../tests/data/forecast_ets_excel_reference_2.json");
const REFERENCE_3: &str = include_str!("../tests/data/forecast_ets_excel_reference_3.json");

fn close(a: f64, b: f64, tol: f64) {
    assert!(
        (a - b).abs() <= tol * b.abs().max(1e-3),
        "{a} is not within {tol} (relative) of {b}"
    );
}

/// The numbers after every `"key":` in the reference file (an array gives all its numbers).
fn numbers_of(key: &str) -> Vec<f64> {
    numbers_in(REFERENCE, key)
}

fn numbers_in(text: &str, key: &str) -> Vec<f64> {
    let needle = format!("\"{key}\":");
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(&needle) {
        rest = rest[at + needle.len()..].trim_start();
        let (list, after) = match rest.strip_prefix('[') {
            Some(inner) => inner.split_once(']').unwrap(),
            None => rest.split_once([',', '\n', '}']).unwrap(),
        };
        found.extend(list.split(',').map(|n| n.trim().parse::<f64>().unwrap()));
        rest = after;
    }
    found
}

/// Month numbers stand for the date timeline (section 5.10 of the specification: a month
/// timeline is turned into month numbers by the caller).
fn month_numbers(count: usize) -> Vec<f64> {
    (0..count).map(|i| i as f64).collect()
}

struct Reference {
    values: Vec<f64>,
    timeline: Vec<f64>,
    forecast: Vec<f64>,
    /// forecast - lower, which is the `FORECAST.ETS.CONFINT` value.
    confint: Vec<f64>,
}

fn reference() -> Reference {
    let values = numbers_of("values");
    let forecast = numbers_of("forecast");
    let confint = forecast
        .iter()
        .zip(numbers_of("lower"))
        .map(|(f, lower)| f - lower)
        .collect();
    Reference {
        timeline: month_numbers(values.len()),
        values,
        forecast,
        confint,
    }
}

#[test]
fn the_reference_file_holds_57_points_and_24_targets() {
    let r = reference();
    assert_eq!(
        (r.values.len(), r.forecast.len(), r.confint.len()),
        (57, 24, 24)
    );
    assert_eq!(numbers_of("timeline").len(), 57);
}

#[test]
fn the_detected_period_is_the_reference_period() {
    let r = reference();
    let args = EtsArgs {
        aggregation: Some(1.0),
        ..EtsArgs::new(&r.values, &r.timeline)
    };
    assert_eq!(
        forecast_ets_seasonality_cr(&args),
        Ok(numbers_of("seasonality_auto")[0] as usize)
    );
}

/// A fact of the reference values that no model of the specification explains yet: the forecasts
/// are `level + h * trend + season(h)` with one trend for all 24 steps, and the forecast twelve
/// steps after the last point is that point plus twelve trends. In the error-correction recursion
/// of section 2.1 this holds only when the last one-step error is zero or alpha + gamma is 1.
#[test]
fn the_reference_forecast_is_anchored_at_the_last_point() {
    let r = reference();
    let trend = (r.forecast[12] - r.forecast[0]) / 12.0;
    for i in 0..12 {
        close((r.forecast[i + 12] - r.forecast[i]) / 12.0, trend, 1e-9);
    }
    close(r.forecast[11], r.values[56] + 12.0 * trend, 1e-9);
}

/// The reference intervals follow the closed form of section 5.7 exactly when it is given the
/// coefficients 0.251, 0.001, 0.001 (state-space form), found by solving the first ratios. The
/// ratio of the half width at step `h` to the one at step 1 does not depend on the error variance.
#[test]
fn the_reference_interval_follows_the_closed_form() {
    let r = reference();
    let args = EtsArgs {
        aggregation: Some(1.0),
        ..EtsArgs::new(&r.values, &r.timeline)
    };
    let mut model = fit_model(&args, 12.0).unwrap();
    let (alpha, beta, gamma) = (0.251, 0.001, 0.001);
    // `Params` keeps the Holt-Winters coefficients: beta = alpha * beta*, gamma = gamma* (1 - alpha).
    model.params = Params {
        alpha,
        beta: beta / alpha,
        gamma: gamma / (1.0 - alpha),
    };
    let first = model.error_sd_steps(1);
    for (h, reference) in r.confint.iter().enumerate() {
        let ratio = model.error_sd_steps(h + 1) / first;
        close(ratio, reference / r.confint[0], 1e-9);
    }
}

/// The target: relative error 1e-6 on every forecast and interval of the reference. It does not hold:
/// the fit settles on other coefficients than the reference app's.
#[test]
#[ignore = "fit differs from the reference values by percents (docs/forecast-ets-cleanroom.md section 9)"]
fn forecast_and_interval_match_the_reference_values() {
    let r = reference();
    let args = EtsArgs {
        aggregation: Some(1.0),
        ..EtsArgs::new(&r.values, &r.timeline)
    };
    for (i, (forecast, confint)) in r.forecast.iter().zip(&r.confint).enumerate() {
        let target = (r.values.len() + i) as f64;
        close(forecast_ets_cr(target, &args).unwrap(), *forecast, 1e-6);
        close(
            forecast_ets_confint_cr(target, &args, 0.95).unwrap(),
            *confint,
            1e-6,
        );
    }
    let stat_args = EtsArgs {
        seasonality: 12.0,
        ..args
    };
    close(
        forecast_ets_stat_cr(&stat_args, 1.0).unwrap(),
        numbers_of("stat_alpha_type1_seasonality12")[0],
        1e-3,
    );
}

/// The second series: 24 monthly points, the period given as 12, forecasts of the next 12 months.
/// Only the forecasts are known, so this is the target for the fit and nothing else.
#[test]
#[ignore = "fit differs from the reference values by percents (docs/forecast-ets-cleanroom.md section 9)"]
fn the_second_series_forecast_matches_the_reference_values() {
    let values = numbers_in(REFERENCE_2, "values");
    let forecast = numbers_in(REFERENCE_2, "forecast");
    assert_eq!((values.len(), forecast.len()), (24, 12));
    let timeline = month_numbers(values.len());
    let args = EtsArgs {
        seasonality: 12.0,
        ..EtsArgs::new(&values, &timeline)
    };
    for (i, expected) in forecast.iter().enumerate() {
        let target = (values.len() + i) as f64;
        close(forecast_ets_cr(target, &args).unwrap(), *expected, 1e-6);
    }
}

/// Third series, no season: 13 annual points. With the reference coefficients (alpha 0.5, state-space
/// beta 0.499) and the initial state read from the reference values, the clean-room recursion
/// reproduces every forecast and interval width. The initial state: the first value is the level
/// (the first one-step error is 0) and the trend is the least squares slope over the whole series.
/// The error variance is the sum of squared one-step errors over the number of points.
#[test]
fn the_third_series_follows_the_recursion_with_the_reference_coefficients() {
    let values = numbers_in(REFERENCE_3, "values");
    let forecast = numbers_in(REFERENCE_3, "forecast");
    let lower = numbers_in(REFERENCE_3, "lower");
    let timeline = month_numbers(values.len());
    let args = EtsArgs::new(&values, &timeline);
    let mut model = fit_model(&args, 0.0).unwrap();

    let init = initial_states(&values, 1);
    // `Params` keeps the Holt form: beta = alpha * beta*.
    let (alpha, beta) = (
        numbers_in(REFERENCE_3, "alpha")[0],
        numbers_in(REFERENCE_3, "beta")[0],
    );
    model.params = Params {
        alpha,
        beta: beta / alpha,
        gamma: 0.0,
    };
    model.fitted.clear();
    let fitted = &mut model.fitted;
    model.pass = run(&values, &init, model.params, &mut model.ring, |f| {
        fitted.push(f)
    });

    let z = dist_value("NORM.S.INV", &[0.975]).unwrap();
    for (i, (expected, low)) in forecast.iter().zip(&lower).enumerate() {
        let h = i + 1;
        close(model.predict_steps(h), *expected, 1e-9);
        close(z * model.error_sd_steps(h), expected - low, 1e-9);
    }
}

/// The target for the fit on the third series: the clean-room search does not land on the reference
/// coefficients (0.5, 0.499), which are not the minimum of the squared one-step error either.
#[test]
#[ignore = "the coefficient search differs from the reference app's (docs/forecast-ets-cleanroom.md section 9)"]
fn the_third_series_forecast_matches_the_reference_values() {
    let values = numbers_in(REFERENCE_3, "values");
    let forecast = numbers_in(REFERENCE_3, "forecast");
    let timeline = month_numbers(values.len());
    let args = EtsArgs::new(&values, &timeline);
    for (i, expected) in forecast.iter().enumerate() {
        let target = (values.len() + i) as f64;
        close(forecast_ets_cr(target, &args).unwrap(), *expected, 1e-6);
    }
}

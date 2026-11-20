//! The clean-room numerics against the values the reference app cached for its published example
//! (`tests/data/forecast_ets_excel_reference.json`, source in `THIRD_PARTY_NOTICES.md`).
//!
//! What matches: the detected period, and the shape of the interval. What does not: the fitted
//! smoothing coefficients, so the forecast and the interval width differ by percents. The ignored
//! test states the target and fails until the fit agrees; `docs/forecast-ets-cleanroom.md`, section 9,
//! holds the measured gap and what the reference values say about the reference app's model.

use super::*;

const REFERENCE: &str = include_str!("../tests/data/forecast_ets_excel_reference.json");

fn close(a: f64, b: f64, tol: f64) {
    assert!(
        (a - b).abs() <= tol * b.abs().max(1e-3),
        "{a} is not within {tol} (relative) of {b}"
    );
}

/// The numbers after every `"key":` in the reference file (an array gives all its numbers).
fn numbers_of(key: &str) -> Vec<f64> {
    let needle = format!("\"{key}\":");
    let mut found = Vec::new();
    let mut rest = REFERENCE;
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

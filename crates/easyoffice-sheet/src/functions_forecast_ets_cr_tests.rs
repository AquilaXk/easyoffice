//! Tests of the clean-room `FORECAST.ETS` numerics.
//!
//! Data: the monthly international airline passengers 1949 to 1960 (Box, Jenkins and Reinsel), a
//! public textbook data set. The reference numbers marked "BSD library" come from a BSD-licensed
//! statistics library run with fixed initial states and fixed coefficients (`smooth`); only the
//! resulting numbers are used here, no code. See `docs/forecast-ets-cleanroom.md`, section 8.

use super::*;

const AIR: [f64; 144] = [
    112., 118., 132., 129., 121., 135., 148., 148., 136., 119., 104., 118., //
    115., 126., 141., 135., 125., 149., 170., 170., 158., 133., 114., 140., //
    145., 150., 178., 163., 172., 178., 199., 199., 184., 162., 146., 166., //
    171., 180., 193., 181., 183., 218., 230., 242., 209., 191., 172., 194., //
    196., 196., 236., 235., 229., 243., 264., 272., 237., 211., 180., 201., //
    204., 188., 235., 227., 234., 264., 302., 293., 259., 229., 203., 229., //
    242., 233., 267., 269., 270., 315., 364., 347., 312., 274., 237., 278., //
    284., 277., 317., 313., 318., 374., 413., 405., 355., 306., 271., 306., //
    315., 301., 356., 348., 355., 422., 465., 467., 404., 347., 305., 336., //
    340., 318., 362., 348., 363., 435., 491., 505., 404., 359., 310., 337., //
    360., 342., 406., 396., 420., 472., 548., 559., 463., 407., 362., 405., //
    417., 391., 419., 461., 472., 535., 622., 606., 508., 461., 390., 432.,
];

fn months() -> Vec<f64> {
    (1..=144).map(f64::from).collect()
}

fn close(a: f64, b: f64, tol: f64) {
    assert!(
        (a - b).abs() <= tol * b.abs().max(1.0),
        "{a} is not within {tol} of {b}"
    );
}

/// Initial states of the seasonal model of `AIR` (BSD library check of the same procedure).
const AIR_INIT_LEVEL: f64 = 109.06875000000002;
const AIR_INIT_TREND: f64 = 2.012499999999999;
const AIR_INIT_SEASONAL: [f64; 12] = [
    -11.556250000000004,
    -5.81875,
    9.668749999999994,
    -1.343750000000005,
    -5.106250000000005,
    12.631250000000007,
    27.36875,
    28.356249999999996,
    8.343749999999995,
    -14.168750000000001,
    -33.43125,
    -14.943749999999985,
];

fn air_init() -> Init {
    Init {
        level: AIR_INIT_LEVEL,
        trend: AIR_INIT_TREND,
        seasonal: AIR_INIT_SEASONAL.to_vec(),
    }
}

fn pass_of(init: &Init, p: Params) -> (Pass, Vec<f64>, Vec<f64>) {
    let mut ring = Vec::new();
    let mut fitted = Vec::new();
    let pass = run(&AIR, init, p, &mut ring, |f| fitted.push(f));
    (pass, ring, fitted)
}

fn air_model(init: &Init, params: Params, period: usize) -> Model {
    let (pass, ring, fitted) = pass_of(init, params);
    Model {
        series: Series {
            y: AIR.to_vec(),
            last: 144.0,
            step: 1.0,
        },
        period,
        params,
        pass,
        ring,
        fitted,
    }
}

#[test]
fn heuristic_initial_states_match_an_independent_computation() {
    let init = initial_states(&AIR, 12);
    close(init.level, AIR_INIT_LEVEL, 1e-12);
    close(init.trend, AIR_INIT_TREND, 1e-12);
    for (a, b) in init.seasonal.iter().zip(AIR_INIT_SEASONAL) {
        close(*a, b, 1e-12);
    }
    close(init.seasonal.iter().sum::<f64>(), 0.0, 1e-12);
    let line = initial_states(&AIR, 1);
    close(line.level, 118.46666666666665, 1e-12);
    close(line.trend, 2.0606060606060583, 1e-12);
    assert!(line.seasonal.is_empty());
}

#[test]
fn seasonal_recursion_matches_the_bsd_library() {
    // BSD library, `ETSModel(error="add", trend="add", seasonal="add").smooth([a, a*b, g*(1-a)])`.
    let cases = [
        ((0.3, 0.1, 0.4), 54467.78258842022),
        ((0.6, 0.0, 0.9), 44799.846495842154),
        ((0.1, 0.5, 0.2), 85569.17770997543),
    ];
    for ((alpha, beta, gamma), sse) in cases {
        let (pass, _, _) = pass_of(&air_init(), Params { alpha, beta, gamma });
        close(pass.sse, sse, 1e-11);
    }
    let (pass, _, _) = pass_of(
        &air_init(),
        Params {
            alpha: 0.3,
            beta: 0.1,
            gamma: 0.4,
        },
    );
    close(pass.level, 498.152962925517, 1e-11);
}

#[test]
fn seasonal_forecast_matches_the_bsd_library() {
    let model = air_model(
        &air_init(),
        Params {
            alpha: 0.3,
            beta: 0.1,
            gamma: 0.4,
        },
        12,
    );
    for (h, expected) in [
        (1, 466.81203478084535),
        (2, 454.8245820236794),
        (13, 511.6595082260419),
        (14, 499.67205546887595),
    ] {
        close(model.predict_steps(h), expected, 1e-11);
    }
}

#[test]
fn trend_only_recursion_and_forecast_match_the_bsd_library() {
    let init = initial_states(&AIR, 1);
    let params = Params {
        alpha: 0.5,
        beta: 0.2,
        gamma: 0.0,
    };
    let model = air_model(&init, params, 1);
    close(model.pass.sse, 319678.01480620395, 1e-11);
    close(model.predict_steps(3), 402.49557108540733, 1e-11);
}

#[test]
fn error_sd_matches_the_bsd_library_ratios_and_the_direct_sum() {
    let params = Params {
        alpha: 0.3,
        beta: 0.1,
        gamma: 0.4,
    };
    let model = air_model(&air_init(), params, 12);
    let sigma = (model.pass.sse / 144.0).sqrt();
    close(sigma, 19.44861152, 1e-8);
    // BSD library `get_prediction(...).var_pred_mean` relative to the first step.
    for (h, ratio) in [
        (2, 1.0530432089900206),
        (13, 2.125323504786977),
        (14, 2.2345245579317314),
    ] {
        close(model.error_sd_steps(h) / sigma, ratio, 1e-11);
    }
    // The closed form equals the sum of squared impulse responses of the recursion.
    let (a, b, g) = (0.3, 0.3 * 0.1, 0.4 * 0.7);
    for h in 1..=60usize {
        let sum: f64 = (1..h)
            .map(|j| {
                let c = a + b * j as f64 + if j % 12 == 0 { g } else { 0.0 };
                c * c
            })
            .sum();
        close(model.error_sd_steps(h) / sigma, (1.0 + sum).sqrt(), 1e-12);
    }
}

#[test]
fn trend_only_error_sd_has_no_seasonal_term() {
    let params = Params {
        alpha: 0.4,
        beta: 0.3,
        gamma: 0.0,
    };
    let model = air_model(&initial_states(&AIR, 1), params, 1);
    let sigma = (model.pass.sse / 144.0).sqrt();
    let (a, b) = (0.4, 0.4 * 0.3);
    for h in 2..30usize {
        let sum: f64 = (1..h).map(|j| (a + b * j as f64).powi(2)).sum();
        close(model.error_sd_steps(h) / sigma, (1.0 + sum).sqrt(), 1e-12);
    }
}

#[test]
fn detection_finds_twelve_months_in_the_airline_series() {
    assert_eq!(detect_period(&AIR), 12);
    let timeline = months();
    let args = EtsArgs::new(&AIR, &timeline);
    assert_eq!(forecast_ets_seasonality_cr(&args), Ok(12));
}

#[test]
fn detection_finds_a_synthetic_period_and_none_in_a_line() {
    let wave: Vec<f64> = (0..70)
        .map(|t| 10.0 + 0.5 * f64::from(t) + [0.0, 3.0, -2.0, 5.0, 1.0, -4.0, 2.0][t as usize % 7])
        .collect();
    assert_eq!(detect_period(&wave), 7);
    let line: Vec<f64> = (0..50).map(|t| 2.0 + 3.0 * f64::from(t)).collect();
    assert_eq!(detect_period(&line), 1);
    assert_eq!(detect_period(&[5.0; 40]), 1);
    assert_eq!(detect_period(&[1.0, 2.0, 1.0]), 1);
}

#[test]
fn search_is_at_least_as_good_as_a_fine_grid() {
    let init = air_init();
    let found = optimize(&AIR, &init);
    let mut ring = Vec::new();
    let loss = |p: Params, ring: &mut Vec<f64>| run(&AIR, &init, p, ring, |_| {}).sse;
    let found_sse = loss(found, &mut ring);
    let mut grid_best = f64::INFINITY;
    for i in 0..=10 {
        for j in 0..=10 {
            for k in 0..=10 {
                let p = Params {
                    alpha: f64::from(i) / 10.0,
                    beta: f64::from(j) / 10.0,
                    gamma: f64::from(k) / 10.0,
                };
                grid_best = grid_best.min(loss(p, &mut ring));
            }
        }
    }
    assert!(found_sse <= grid_best, "{found_sse} > {grid_best}");
    for c in [found.alpha, found.beta, found.gamma] {
        assert!((0.0..=1.0).contains(&c));
    }
}

#[test]
fn search_reaches_the_optimum_found_by_an_independent_simplex_search() {
    // SSE of the best coefficients a simplex search (BSD scientific library, five starts, tight
    // tolerances) found for the same initial states.
    let seasonal = optimize(&AIR, &air_init());
    let (pass, _, _) = pass_of(&air_init(), seasonal);
    close(pass.sse, 22354.4283562043, 1e-6);
    let line = initial_states(&AIR, 1);
    let trend = optimize(&AIR, &line);
    let mut ring = Vec::new();
    close(
        run(&AIR, &line, trend, &mut ring, |_| {}).sse,
        161865.11842056934,
        1e-6,
    );
}

#[test]
fn airline_forecast_is_close_to_the_series_pattern() {
    let timeline = months();
    let args = EtsArgs::new(&AIR, &timeline);
    let december = forecast_ets_cr(156.0, &args).unwrap();
    let july = forecast_ets_cr(151.0, &args).unwrap();
    // The last year peaks at 622 in July and ends at 432 in December.
    assert!((560.0..720.0).contains(&july), "{july}");
    assert!((400.0..520.0).contains(&december), "{december}");
    // Explicit period 12 and detection give the same model.
    let explicit = EtsArgs {
        seasonality: 12.0,
        ..args
    };
    assert_eq!(forecast_ets_cr(151.0, &explicit).unwrap(), july);
    // No season: a straight line through the level, far from the July peak.
    let linear = EtsArgs {
        seasonality: 0.0,
        ..args
    };
    let flat = forecast_ets_cr(151.0, &linear).unwrap();
    assert!((flat - july).abs() > 20.0);
}

#[test]
fn confidence_interval_grows_with_level_and_horizon() {
    let timeline = months();
    let args = EtsArgs::new(&AIR, &timeline);
    let near = forecast_ets_confint_cr(145.0, &args, 0.95).unwrap();
    let far = forecast_ets_confint_cr(168.0, &args, 0.95).unwrap();
    let low = forecast_ets_confint_cr(145.0, &args, 0.5).unwrap();
    assert!(near > 0.0 && far > near && low < near);
    let model = fit_model(&args, 1.0).unwrap();
    // The 95% radius at one step is 1.959964 sigma.
    close(near, 1.959963984540054 * model.error_sd_steps(1), 1e-9);
}

#[test]
fn a_line_is_forecast_exactly() {
    let values: Vec<f64> = (1..=20).map(|t| 2.0 + 3.0 * f64::from(t)).collect();
    let timeline: Vec<f64> = (1..=20).map(f64::from).collect();
    let args = EtsArgs::new(&values, &timeline);
    close(forecast_ets_cr(25.0, &args).unwrap(), 77.0, 1e-9);
    // Fractional target: halfway between the forecasts of 25 and 26.
    close(forecast_ets_cr(25.5, &args).unwrap(), 78.5, 1e-9);
    close(
        forecast_ets_confint_cr(25.0, &args, 0.9).unwrap(),
        0.0,
        1e-9,
    );
    assert_eq!(forecast_ets_seasonality_cr(&args), Ok(1));
}

#[test]
fn the_last_time_returns_the_one_step_fit() {
    let timeline = months();
    let args = EtsArgs::new(&AIR, &timeline);
    let model = fit_model(&args, 1.0).unwrap();
    assert_eq!(forecast_ets_cr(144.0, &args).unwrap(), model.fitted[143]);
}

#[test]
fn statistics_are_consistent_with_the_fit() {
    let timeline = months();
    let args = EtsArgs::new(&AIR, &timeline);
    let stat = |i: f64| forecast_ets_stat_cr(&args, i).unwrap();
    let model = fit_model(&args, 1.0).unwrap();
    close(stat(1.0), model.params.alpha, 0.0);
    close(stat(2.0), model.params.alpha * model.params.beta, 1e-15);
    close(
        stat(3.0),
        model.params.gamma * (1.0 - model.params.alpha),
        1e-15,
    );
    let (mase, smape, mae, rmse, step) = (stat(4.0), stat(5.0), stat(6.0), stat(7.0), stat(8.0));
    close(rmse * rmse * 144.0, model.pass.sse, 1e-12);
    assert!(mae <= rmse && mae > 0.0);
    assert_eq!(step, 1.0);
    // Direct definitions on the fitted values.
    let mut abs = 0.0;
    let mut sym = 0.0;
    for (y, f) in AIR.iter().zip(&model.fitted) {
        abs += (y - f).abs();
        sym += 2.0 * (y - f).abs() / (y + f);
    }
    close(mae, abs / 144.0, 1e-12);
    close(smape, sym / 144.0, 1e-12);
    let scale: f64 = (12..144).map(|t| (AIR[t] - AIR[t - 12]).abs()).sum::<f64>() / 132.0;
    close(mase, abs / 144.0 / scale, 1e-12);
    // The seasonal naive scale makes the error of this fit well under one.
    assert!(mase < 1.0, "{mase}");
}

#[test]
fn statistics_without_season_use_the_naive_scale_and_zero_gamma() {
    let timeline = months();
    let args = EtsArgs {
        seasonality: 0.0,
        ..EtsArgs::new(&AIR, &timeline)
    };
    assert_eq!(forecast_ets_stat_cr(&args, 3.0), Ok(0.0));
    let model = fit_model(&args, 0.0).unwrap();
    let abs: f64 = AIR
        .iter()
        .zip(&model.fitted)
        .map(|(y, f)| (y - f).abs())
        .sum();
    let scale: f64 = (1..144).map(|t| (AIR[t] - AIR[t - 1]).abs()).sum::<f64>() / 143.0;
    close(
        forecast_ets_stat_cr(&args, 4.0).unwrap(),
        abs / 144.0 / scale,
        1e-12,
    );
}

#[test]
fn a_constant_series_has_no_scale() {
    let values = [5.0; 12];
    let timeline: Vec<f64> = (1..=12).map(f64::from).collect();
    let args = EtsArgs::new(&values, &timeline);
    close(forecast_ets_cr(20.0, &args).unwrap(), 5.0, 1e-12);
    assert_eq!(forecast_ets_stat_cr(&args, 4.0), Err(EtsError::ZeroScale));
    assert_eq!(forecast_ets_seasonality_cr(&args), Ok(1));
}

#[test]
fn timeline_order_and_step_do_not_change_the_result() {
    let timeline = months();
    let base = forecast_ets_cr(150.0, &EtsArgs::new(&AIR, &timeline)).unwrap();
    // Reversed order.
    let values: Vec<f64> = AIR.iter().rev().copied().collect();
    let reversed: Vec<f64> = timeline.iter().rev().copied().collect();
    assert_eq!(
        forecast_ets_cr(150.0, &EtsArgs::new(&values, &reversed)).unwrap(),
        base
    );
    // Step 7 starting at 100: the same grid, so the same value at the matching target.
    let weekly: Vec<f64> = timeline.iter().map(|t| 100.0 + 7.0 * (t - 1.0)).collect();
    let shifted = forecast_ets_cr(100.0 + 7.0 * 149.0, &EtsArgs::new(&AIR, &weekly)).unwrap();
    close(shifted, base, 1e-9);
    assert_eq!(
        forecast_ets_stat_cr(&EtsArgs::new(&AIR, &weekly), 8.0),
        Ok(7.0)
    );
}

#[test]
fn missing_points_are_filled_and_counted() {
    // Drop every 7th month (about 14% missing).
    let kept: Vec<usize> = (0..144).filter(|t| t % 7 != 3).collect();
    let values: Vec<f64> = kept.iter().map(|&t| AIR[t]).collect();
    let timeline: Vec<f64> = kept.iter().map(|&t| (t + 1) as f64).collect();
    let args = EtsArgs::new(&values, &timeline);
    let filled = forecast_ets_cr(150.0, &args).unwrap();
    let zeros = forecast_ets_cr(
        150.0,
        &EtsArgs {
            data_completion: 0.0,
            ..args
        },
    )
    .unwrap();
    assert!(filled.is_finite() && (filled - zeros).abs() > 1e-6);
    let options = parse_options(&args, 1.0).unwrap();
    let series = build_series(&args, &options).unwrap();
    // The grid runs from month 1 to month 143 (month 144 is dropped too).
    assert_eq!(series.y.len(), 143);
    // The filled point is the straight line between its neighbors.
    close(series.y[3], (AIR[2] + AIR[4]) / 2.0, 1e-12);
}

#[test]
fn fill_grid_interpolates_long_gaps_and_zero_fills_on_request() {
    let slots = [0, 4, 5];
    let values = [10.0, 20.0, 30.0];
    assert_eq!(
        fill_grid(&slots, &values, 6, false),
        [10.0, 12.5, 15.0, 17.5, 20.0, 30.0]
    );
    assert_eq!(
        fill_grid(&slots, &values, 6, true),
        [10.0, 0.0, 0.0, 0.0, 20.0, 30.0]
    );
}

#[test]
fn more_than_thirty_percent_missing_is_rejected() {
    let timeline: Vec<f64> = (0..10).map(|t| f64::from(t) * 2.0).chain([1.0]).collect();
    // Times 0, 2, ..., 18 and 1: step 1 over 19 slots, 8 missing (42%).
    let values: Vec<f64> = (0..11).map(f64::from).collect();
    let args = EtsArgs::new(&values, &timeline);
    assert_eq!(forecast_ets_cr(30.0, &args), Err(EtsError::TooManyMissing));
    // Exactly 30%: 7 of 10 present.
    let timeline = [0.0, 1.0, 2.0, 4.0, 5.0, 6.0, 8.0, 9.0, 10.0, 12.0];
    let values: Vec<f64> = timeline.iter().map(|t| 3.0 + t).collect();
    let args = EtsArgs::new(&values, &timeline);
    assert!(forecast_ets_cr(13.0, &args).is_ok());
}

#[test]
fn repeated_times_follow_the_aggregation_argument() {
    let timeline = [1.0, 2.0, 2.0, 3.0, 4.0, 4.0, 4.0, 5.0];
    let values = [1.0, 2.0, 6.0, 4.0, 1.0, 5.0, 9.0, 8.0];
    let args = EtsArgs::new(&values, &timeline);
    assert_eq!(
        forecast_ets_cr(6.0, &args),
        Err(EtsError::DuplicateTimeline)
    );
    let grid = |code: f64| {
        let args = EtsArgs {
            aggregation: Some(code),
            ..args
        };
        let options = parse_options(&args, 1.0).unwrap();
        build_series(&args, &options).unwrap().y
    };
    assert_eq!(grid(0.0), [1.0, 4.0, 4.0, 5.0, 8.0]);
    assert_eq!(grid(1.0), grid(0.0));
    assert_eq!(grid(2.0), [1.0, 2.0, 1.0, 3.0, 1.0]);
    assert_eq!(grid(3.0), grid(2.0));
    assert_eq!(grid(4.0), [1.0, 6.0, 4.0, 9.0, 8.0]);
    assert_eq!(grid(5.0), [1.0, 4.0, 4.0, 5.0, 8.0]);
    assert_eq!(grid(6.0), [1.0, 2.0, 4.0, 1.0, 8.0]);
    assert_eq!(grid(7.0), [1.0, 8.0, 4.0, 15.0, 8.0]);
}

#[test]
fn documented_errors() {
    let timeline = months();
    let args = EtsArgs::new(&AIR, &timeline);
    // Sizes differ: #N/A.
    let short = &timeline[..100];
    let mismatch = EtsArgs::new(&AIR, short);
    assert_eq!(
        forecast_ets_cr(150.0, &mismatch),
        Err(EtsError::SizeMismatch)
    );
    assert_eq!(EtsError::SizeMismatch.code(), "#N/A");
    // Target before the end: #NUM!.
    assert_eq!(
        forecast_ets_cr(143.0, &args),
        Err(EtsError::TargetBeforeEnd)
    );
    assert_eq!(EtsError::TargetBeforeEnd.code(), "#NUM!");
    // Duplicates in the timeline: #VALUE!.
    let mut dup = timeline.clone();
    dup[10] = dup[9];
    assert_eq!(
        forecast_ets_cr(150.0, &EtsArgs::new(&AIR, &dup)),
        Err(EtsError::DuplicateTimeline)
    );
    assert_eq!(EtsError::DuplicateTimeline.code(), "#VALUE!");
    // No constant step: #NUM!.
    let mut bumpy = timeline.clone();
    bumpy[50] += 0.3;
    assert_eq!(
        forecast_ets_cr(150.0, &EtsArgs::new(&AIR, &bumpy)),
        Err(EtsError::NoConstantStep)
    );
    // Seasonality 0.5, -1 and 8,761: #NUM!.
    for bad in [0.5, -1.0, 8761.0, f64::NAN, f64::INFINITY] {
        let a = EtsArgs {
            seasonality: bad,
            ..args
        };
        assert_eq!(
            forecast_ets_cr(150.0, &a),
            Err(EtsError::InvalidArgument),
            "{bad}"
        );
    }
    // Confidence level outside (0, 1).
    for bad in [0.0, 1.0, -0.1, 1.5, f64::NAN] {
        assert_eq!(
            forecast_ets_confint_cr(150.0, &args, bad),
            Err(EtsError::InvalidArgument),
            "{bad}"
        );
    }
    // Statistic type outside 1 to 8.
    for bad in [0.0, 9.0, 2.5, -1.0, f64::NAN] {
        assert_eq!(
            forecast_ets_stat_cr(&args, bad),
            Err(EtsError::InvalidArgument),
            "{bad}"
        );
    }
    // Data completion and aggregation sets.
    let a = EtsArgs {
        data_completion: 2.0,
        ..args
    };
    assert_eq!(forecast_ets_cr(150.0, &a), Err(EtsError::InvalidArgument));
    for bad in [8.0, -1.0, 1.5, f64::NAN] {
        let a = EtsArgs {
            aggregation: Some(bad),
            ..args
        };
        assert_eq!(
            forecast_ets_cr(150.0, &a),
            Err(EtsError::InvalidArgument),
            "{bad}"
        );
    }
}

#[test]
fn a_period_needs_two_seasons() {
    let timeline = months();
    let a = EtsArgs {
        seasonality: 73.0,
        ..EtsArgs::new(&AIR, &timeline)
    };
    assert_eq!(forecast_ets_cr(150.0, &a), Err(EtsError::TooFewPoints));
    let a = EtsArgs {
        seasonality: 72.0,
        ..EtsArgs::new(&AIR, &timeline)
    };
    assert!(forecast_ets_cr(150.0, &a).is_ok());
    let tiny = EtsArgs::new(&AIR[..2], &timeline[..2]);
    assert_eq!(forecast_ets_cr(5.0, &tiny), Err(EtsError::TooFewPoints));
    assert_eq!(
        forecast_ets_cr(5.0, &EtsArgs::new(&[], &[])),
        Err(EtsError::TooFewPoints)
    );
}

#[test]
fn hostile_inputs_fail_closed() {
    let timeline = months();
    let args = EtsArgs::new(&AIR, &timeline);
    // Non-finite values, times and targets.
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut values = AIR.to_vec();
        values[5] = bad;
        assert_eq!(
            forecast_ets_cr(150.0, &EtsArgs::new(&values, &timeline)),
            Err(EtsError::NonFinite)
        );
        let mut times = timeline.clone();
        times[5] = bad;
        assert_eq!(
            forecast_ets_cr(150.0, &EtsArgs::new(&AIR, &times)),
            Err(EtsError::NonFinite)
        );
        assert_eq!(forecast_ets_cr(bad, &args), Err(EtsError::NonFinite));
        assert_eq!(
            forecast_ets_seasonality_cr(&EtsArgs::new(&values, &timeline)),
            Err(EtsError::NonFinite)
        );
    }
    // A target a billion steps away is refused; a finite one nearby is not.
    assert_eq!(forecast_ets_cr(1e12, &args), Err(EtsError::TargetTooFar));
    assert!(forecast_ets_cr(144.0 + 1e6, &args).is_ok());
    // Magnitudes that overflow the recursion are an error, not a NaN.
    let big: Vec<f64> = (0..30).map(|t| 1e300 * f64::from(t % 3 + 1)).collect();
    let times: Vec<f64> = (0..30).map(f64::from).collect();
    let result = forecast_ets_cr(40.0, &EtsArgs::new(&big, &times));
    assert!(result.is_err() || result.unwrap().is_finite());
}

#[test]
fn a_sparse_or_huge_timeline_is_refused_before_any_allocation() {
    let values = [1.0, 2.0, 3.0, 4.0];
    // A far point makes more than 30% of the grid missing, or too long a grid.
    for far in [1e6, 1e12, 1e300, f64::MAX] {
        let timeline = [0.0, 1.0, 2.0, far];
        let err = forecast_ets_cr(far + 1.0, &EtsArgs::new(&values, &timeline)).unwrap_err();
        assert!(
            matches!(
                err,
                EtsError::TooManyPoints | EtsError::NoConstantStep | EtsError::TooManyMissing
            ),
            "{far}: {err:?}"
        );
    }
    // The span itself overflows.
    let timeline = [-f64::MAX, 0.0, f64::MAX, 1.0];
    assert!(forecast_ets_cr(f64::MAX, &EtsArgs::new(&values, &timeline)).is_err());
    // Tiny steps are fine when regular.
    let timeline: Vec<f64> = (0..10).map(|t| f64::from(t) * 1e-300).collect();
    let values: Vec<f64> = (0..10).map(|t| f64::from(t) * 2.0).collect();
    close(
        forecast_ets_cr(10.0e-300, &EtsArgs::new(&values, &timeline)).unwrap(),
        20.0,
        1e-9,
    );
}

#[test]
fn too_many_points_are_refused() {
    let values = vec![1.0; MAX_POINTS + 1];
    let timeline: Vec<f64> = (0..=MAX_POINTS).map(|t| t as f64).collect();
    let args = EtsArgs::new(&values, &timeline);
    assert_eq!(forecast_ets_cr(1e6, &args), Err(EtsError::TooManyPoints));
    assert_eq!(
        forecast_ets_seasonality_cr(&args),
        Err(EtsError::TooManyPoints)
    );
}

#[test]
fn a_long_series_with_the_longest_period_stays_within_its_budget() {
    // 20,000 points with the longest period: 2 * 8,760 <= 20,000, so it is allowed and bounded.
    let n = 20_000usize;
    let values: Vec<f64> = (0..n)
        .map(|t| 100.0 + 0.01 * t as f64 + 10.0 * ((t % MAX_SEASONALITY) as f64 / 700.0).sin())
        .collect();
    let timeline: Vec<f64> = (0..n).map(|t| t as f64).collect();
    let args = EtsArgs {
        seasonality: MAX_SEASONALITY as f64,
        ..EtsArgs::new(&values, &timeline)
    };
    let started = std::time::Instant::now();
    assert!(forecast_ets_cr(n as f64 + 5.0, &args).is_ok());
    assert!(started.elapsed().as_secs() < 60, "{:?}", started.elapsed());
    // Detection on the same length is bounded by the lag limit too.
    let auto = EtsArgs::new(&values, &timeline);
    assert!(forecast_ets_seasonality_cr(&auto).is_ok());
}

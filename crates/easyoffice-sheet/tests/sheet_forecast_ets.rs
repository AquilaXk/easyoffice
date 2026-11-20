//! Integration tests for `FORECAST.ETS`, `FORECAST.ETS.CONFINT`, `FORECAST.ETS.SEASONALITY` and
//! `FORECAST.ETS.STAT` through the sheet engine: documented argument rules, calendar date
//! timelines, dynamic array spilling, closed-form intervals, and untrusted-input budgets.
#![allow(clippy::unwrap_used)]

mod dynamic_support;
use dynamic_support::{new_empty_model, Model};
use std::time::{Duration, Instant};

const REFERENCE_1: &str = include_str!("data/forecast_ets_excel_reference.json");
const REFERENCE_3: &str = include_str!("data/forecast_ets_excel_reference_3.json");

/// First 36 months (1949-1951) of the Box, Jenkins and Reinsel international airline passengers
/// textbook series.
const AIR_36: [f64; 36] = [
    112., 118., 132., 129., 121., 135., 148., 148., 136., 119., 104., 118., //
    115., 126., 141., 135., 125., 149., 170., 170., 158., 133., 114., 140., //
    145., 150., 178., 163., 172., 178., 199., 199., 184., 162., 146., 166.,
];

fn numbers_in(json: &str, key: &str) -> Vec<f64> {
    let needle = format!("\"{key}\":");
    let mut found = Vec::new();
    let mut rest = json;
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

/// A sheet with the timeline in column A and the values in column B, from row 1.
fn series(timeline: &[f64], values: &[f64]) -> Model {
    let mut model = new_empty_model();
    for (i, (t, v)) in timeline.iter().zip(values).enumerate() {
        model._set(&format!("A{}", i + 1), &t.to_string());
        model._set(&format!("B{}", i + 1), &v.to_string());
    }
    model
}

/// The text of `H1` for `formula` on the series.
fn text(model: &mut Model, formula: &str) -> String {
    model._set("H1", formula);
    model.evaluate();
    model._get_text("H1")
}

fn number(model: &mut Model, formula: &str) -> f64 {
    let t = text(model, formula);
    t.parse().unwrap_or_else(|_| panic!("{formula} gave {t}"))
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9 * b.abs().max(1e-3), "{a} is not {b}");
}

fn seasonal() -> Model {
    let timeline: Vec<f64> = (1..=36).map(f64::from).collect();
    let mut model = series(&timeline, &AIR_36);
    for (i, t) in [37, 38, 39].iter().enumerate() {
        model._set(&format!("D{}", i + 1), &t.to_string());
    }
    model
}

#[test]
fn forecast_ets_spills_for_target_arrays_and_anchors_at_the_last_point() {
    let mut model = seasonal();
    let step1 = number(&mut model, "=FORECAST.ETS(37,B1:B36,A1:A36,12)");
    let step2 = number(&mut model, "=FORECAST.ETS(38,B1:B36,A1:A36,12)");
    let step3 = number(&mut model, "=FORECAST.ETS(39,B1:B36,A1:A36,12)");
    // January..March of year 4 continue the upward seasonal pattern (145, 150, 178 in year 3).
    assert!((155.0..185.0).contains(&step1), "{step1}");
    assert!((160.0..195.0).contains(&step2), "{step2}");
    assert!(step3 > step2 && step3 < 220.0, "{step3}");
    // Target on the end of the timeline returns the last historical value.
    close(
        number(&mut model, "=FORECAST.ETS(36,B1:B36,A1:A36,12)"),
        166.0,
    );
    // Column array of target dates spills vertically.
    model._set("H1", "=FORECAST.ETS(D1:D3,B1:B36,A1:A36,12)");
    model.evaluate();
    for (i, want) in [step1, step2, step3].into_iter().enumerate() {
        close(
            model._get_text(&format!("H{}", i + 1)).parse().unwrap(),
            want,
        );
    }
    assert_eq!(model._get_text("H4"), "");
    // Row array of target dates spills horizontally.
    model._set("H1", "=FORECAST.ETS(TRANSPOSE(D1:D3),B1:B36,A1:A36,12)");
    model.evaluate();
    close(model._get_text("J1").parse().unwrap(), step3);
    // Default seasonality detects period 12 and matches explicit 12,1,1 and 12,1,0.
    close(number(&mut model, "=FORECAST.ETS(37,B1:B36,A1:A36)"), step1);
    close(
        number(&mut model, "=FORECAST.ETS(37,B1:B36,A1:A36,12,1,1)"),
        step1,
    );
    close(
        number(&mut model, "=FORECAST.ETS(37,B1:B36,A1:A36,12,1,0)"),
        step1,
    );
}

#[test]
fn forecast_ets_of_a_short_series_unsorted_timeline_and_array_constants() {
    let mut model = series(&[1.0, 2.0, 3.0, 4.0, 5.0], &[2.0, 4.0, 7.0, 8.0, 11.0]);
    let sorted_forecast = number(&mut model, "=FORECAST.ETS(6,B1:B5,A1:A5)");
    assert!((12.5..14.0).contains(&sorted_forecast), "{sorted_forecast}");
    assert_eq!(
        text(&mut model, "=FORECAST.ETS.SEASONALITY(B1:B5,A1:A5)"),
        "0"
    );
    // Unsorted timeline gives the exact same forecast after internal sorting.
    let mut unsorted = series(&[3.0, 1.0, 2.0, 5.0, 4.0], &[7.0, 2.0, 4.0, 11.0, 8.0]);
    close(
        number(&mut unsorted, "=FORECAST.ETS(6,B1:B5,A1:A5)"),
        sorted_forecast,
    );
    // Exact straight lines through array constants.
    let mut model = new_empty_model();
    close(number(&mut model, "=FORECAST.ETS(8,{1;2;3},{4;5;6})"), 5.0);
    close(
        number(&mut model, "=FORECAST.ETS(50,{-1;-2;-3;-4},{10;20;30;40})"),
        -5.0,
    );
}

#[test]
fn a_target_date_before_the_end_of_the_timeline_is_num() {
    let mut model = seasonal();
    assert_eq!(
        text(&mut model, "=FORECAST.ETS(35,B1:B36,A1:A36,12)"),
        "#NUM!"
    );
    assert_eq!(
        text(&mut model, "=FORECAST.ETS(0,B1:B36,A1:A36,12)"),
        "#NUM!"
    );
    assert_eq!(
        text(&mut model, "=FORECAST.ETS(-30,B1:B36,A1:A36,12)"),
        "#NUM!"
    );
    assert_eq!(
        text(&mut model, "=FORECAST.ETS({40;10},B1:B36,A1:A36,12)"),
        "#NUM!"
    );
    assert_eq!(
        text(&mut model, "=FORECAST.ETS.CONFINT(35,B1:B36,A1:A36,0.9,12)"),
        "#NUM!"
    );
}

#[test]
fn the_documented_errors_of_the_arguments() {
    let mut model = seasonal();
    let cases = [
        ("=FORECAST.ETS(41,{6;7;9;15;21},{20;28;31;38;40})", "#NUM!"),
        ("=FORECAST.ETS(37,B1:B36,A1:A35,12)", "#N/A"),
        ("=FORECAST.ETS(37,B1:B35,A1:A36,12)", "#N/A"),
        ("=FORECAST.ETS(37,B1:B36,TRANSPOSE(A1:A36),12)", "#N/A"),
        ("=FORECAST.ETS(37,B1:B36,A1:A36,8761)", "#NUM!"),
        ("=FORECAST.ETS(37,B1:B36,A1:A36,-1)", "#NUM!"),
        ("=FORECAST.ETS(37,B1:B36,A1:A36,1.5)", "#NUM!"),
        ("=FORECAST.ETS(37,B1:B36,A1:A36,\"x\")", "#VALUE!"),
        ("=FORECAST.ETS(37,B1:B36,A1:A36,19)", "#NUM!"),
        ("=FORECAST.ETS(37,B1:B36,A1:A36,12,2)", "#NUM!"),
        ("=FORECAST.ETS(37,B1:B36,A1:A36,12,1,8)", "#NUM!"),
        ("=FORECAST.ETS(37,B1:B36,A1:A36,12,1,-1)", "#NUM!"),
        ("=FORECAST.ETS(37,{1;\"a\";3;4},{1;2;3;4})", "#VALUE!"),
        ("=FORECAST.ETS(37,{1;2;3;4},{1;TRUE;3;4})", "#VALUE!"),
        ("=FORECAST.ETS(37,{1;#DIV/0!;3;4},{1;2;3;4})", "#DIV/0!"),
        ("=FORECAST.ETS(#N/A,B1:B36,A1:A36,12)", "#N/A"),
        ("=FORECAST.ETS(\"x\",B1:B36,A1:A36,12)", "#VALUE!"),
        ("=FORECAST.ETS(37,B1:B40,A1:A40,12)", "#VALUE!"),
        ("=FORECAST.ETS(37,B1:B36)", "#VALUE!"),
        ("=FORECAST.ETS(37,B1:B36,A1:A36,12,1,1,1)", "#VALUE!"),
        ("=FORECAST.ETS(3,{1},{1})", "#NUM!"),
        ("=FORECAST.ETS(3,{1;2},{1;2})", "#NUM!"),
    ];
    for (formula, want) in cases {
        assert_eq!(text(&mut model, formula), want, "{formula}");
    }
}

#[test]
fn equal_timeline_values_are_combined_by_the_aggregation() {
    let timeline = [1.0, 2.0, 2.0, 3.0, 4.0, 5.0];
    let mut model = series(&timeline, &[2.0, 3.0, 5.0, 7.0, 8.0, 11.0]);
    let expected_avg = number(&mut model, "=FORECAST.ETS(6,{2;4;7;8;11},{1;2;3;4;5},0)");
    for aggregation in ["", ",1", ",0"] {
        let formula = format!("=FORECAST.ETS(6,B1:B6,A1:A6,0,1{aggregation})");
        close(number(&mut model, &formula), expected_avg);
    }
    for (aggregation, combined) in [
        (7, "{2;8;7;8;11}"),
        (4, "{2;5;7;8;11}"),
        (6, "{2;3;7;8;11}"),
        (2, "{1;2;1;1;1}"),
        (3, "{1;2;1;1;1}"),
        (5, "{2;4;7;8;11}"),
    ] {
        let got = number(
            &mut model,
            &format!("=FORECAST.ETS(6,B1:B6,A1:A6,0,1,{aggregation})"),
        );
        let want = number(
            &mut model,
            &format!("=FORECAST.ETS(6,{combined},{{1;2;3;4;5}},0)"),
        );
        close(got, want);
    }
    let min = number(&mut model, "=FORECAST.ETS(6,B1:B6,A1:A6,0,1,6)");
    assert!((min - expected_avg).abs() > 1e-3, "{min}");
}

#[test]
fn a_gap_in_the_timeline_is_filled_up_to_thirty_percent() {
    let timeline = [1.0, 2.0, 3.0, 4.0, 6.0, 7.0, 8.0, 9.0, 10.0];
    let values = [1.0, 2.0, 3.0, 4.0, 6.0, 7.0, 8.0, 9.0, 10.0];
    let mut model = series(&timeline, &values);
    close(number(&mut model, "=FORECAST.ETS(11,B1:B9,A1:A9,0)"), 11.0);
    let zero_filled = number(&mut model, "=FORECAST.ETS(11,B1:B9,A1:A9,0,0)");
    assert!(
        zero_filled.is_finite() && (zero_filled - 11.0).abs() > 1e-2,
        "{zero_filled}"
    );
    let mut sparse = series(&[1.0, 2.0, 3.0, 4.0, 10.0], &[1.0, 2.0, 3.0, 4.0, 10.0]);
    assert_eq!(
        text(&mut sparse, "=FORECAST.ETS(11,B1:B5,A1:A5,0)"),
        "#NUM!"
    );
}

#[test]
fn calendar_monthly_eom_and_annual_date_timelines_are_normalized() {
    // First-of-month dates across 2001..2003 match an integer month timeline 1..36.
    let mut by_date = new_empty_model();
    for (i, v) in AIR_36.iter().enumerate() {
        let (year, month) = (2001 + i / 12, i % 12 + 1);
        by_date._set(&format!("A{}", i + 1), &format!("=DATE({year},{month},1)"));
        by_date._set(&format!("B{}", i + 1), &v.to_string());
    }
    let mut by_index = seasonal();
    close(
        number(
            &mut by_date,
            "=FORECAST.ETS(DATE(2004,1,1),B1:B36,A1:A36,12)",
        ),
        number(&mut by_index, "=FORECAST.ETS(37,B1:B36,A1:A36,12)"),
    );
    close(
        number(&mut by_date, "=FORECAST.ETS(DATE(2004,2,1),B1:B36,A1:A36)"),
        number(&mut by_index, "=FORECAST.ETS(38,B1:B36,A1:A36)"),
    );

    // End-of-month dates across a leap year (2024-01-31, 2024-02-29, ..., 2024-06-30).
    let mut eom = new_empty_model();
    for i in 0..6 {
        eom._set(
            &format!("A{}", i + 1),
            &format!("=EOMONTH(DATE(2024,1,1),{i})"),
        );
        eom._set(&format!("B{}", i + 1), &(10 + i * 5).to_string());
    }
    close(
        number(&mut eom, "=FORECAST.ETS(DATE(2024,7,31),B1:B6,A1:A6,0)"),
        40.0,
    );

    // Published 57-month reference series through real date serials detects period 12.
    let tl1 = numbers_in(REFERENCE_1, "timeline");
    let v1 = numbers_in(REFERENCE_1, "values");
    let mut ref1 = series(&tl1, &v1);
    assert_eq!(
        text(&mut ref1, "=FORECAST.ETS.SEASONALITY(B1:B57,A1:A57,1,1)"),
        "12"
    );

    // Published 13-year annual reference series (365 and 366 day steps) evaluates cleanly.
    let tl3 = numbers_in(REFERENCE_3, "timeline");
    let v3 = numbers_in(REFERENCE_3, "values");
    let targets3 = numbers_in(REFERENCE_3, "target");
    let expected3 = numbers_in(REFERENCE_3, "forecast");
    let mut ref3 = series(&tl3, &v3);
    let got_first = number(
        &mut ref3,
        &format!("=FORECAST.ETS({},B1:B13,A1:A13,0,1)", targets3[0]),
    );
    assert!((got_first - expected3[0]).abs() / expected3[0] < 0.02);
}

#[test]
fn seasonality_returns_period_or_zero_for_non_seasonal_series() {
    let mut model = seasonal();
    assert_eq!(
        text(&mut model, "=FORECAST.ETS.SEASONALITY(B1:B36,A1:A36)"),
        "12"
    );
    assert_eq!(
        text(&mut model, "=FORECAST.ETS.SEASONALITY(B1:B36,A1:A36,1,1)"),
        "12"
    );
    let mut line = series(&[1.0, 2.0, 3.0, 4.0, 5.0], &[2.0, 4.0, 6.0, 8.0, 10.0]);
    assert_eq!(
        text(&mut line, "=FORECAST.ETS.SEASONALITY(B1:B5,A1:A5)"),
        "0"
    );
    assert_eq!(
        text(&mut line, "=FORECAST.ETS.SEASONALITY(B1:B5,A1:A4)"),
        "#N/A"
    );
    assert_eq!(
        text(
            &mut line,
            "=FORECAST.ETS.SEASONALITY({6;7;9;15;21},{20;28;31;38;40})"
        ),
        "#NUM!"
    );
    assert_eq!(
        text(&mut line, "=FORECAST.ETS.SEASONALITY(B1:B5)"),
        "#VALUE!"
    );
    assert_eq!(
        text(&mut line, "=FORECAST.ETS.SEASONALITY(B1:B5,A1:A5,2)"),
        "#NUM!"
    );
    assert_eq!(
        text(&mut line, "=FORECAST.ETS.SEASONALITY(B1:B5,A1:A5,1,8)"),
        "#NUM!"
    );
}

#[test]
fn stat_gives_the_statistics_in_the_documented_order() {
    let mut model = seasonal();
    close(
        number(&mut model, "=FORECAST.ETS.STAT(B1:B36,A1:A36,8,12)"),
        1.0,
    );
    model._set(
        "H1",
        "=FORECAST.ETS.STAT(B1:B36,A1:A36,{1;2;3;4;5;6;7;8},12)",
    );
    model.evaluate();
    let stats: Vec<f64> = (1..=8)
        .map(|i| model._get_text(&format!("H{i}")).parse().unwrap())
        .collect();
    assert!(
        stats[..3].iter().all(|w| (0.0..=1.0).contains(w)),
        "{stats:?}"
    );
    assert!(stats[3..7].iter().all(|e| *e >= 0.0), "{stats:?}");
    assert_eq!(stats[7], 1.0);
    assert!(stats[6] >= stats[5], "{stats:?}");
    model._set("H1", "=FORECAST.ETS.STAT(B1:B36,A1:A36,{1,8},12)");
    model.evaluate();
    close(model._get_text("H1").parse().unwrap(), stats[0]);
    assert_eq!(model._get_text("I1"), "1");
    for (formula, want) in [
        ("=FORECAST.ETS.STAT(B1:B36,A1:A36,0,12)", "#NUM!"),
        ("=FORECAST.ETS.STAT(B1:B36,A1:A36,9,12)", "#NUM!"),
        ("=FORECAST.ETS.STAT(B1:B36,A1:A36,1.5,12)", "#NUM!"),
        ("=FORECAST.ETS.STAT(B1:B36,A1:A35,1,12)", "#N/A"),
        ("=FORECAST.ETS.STAT(B1:B36,A1:A36)", "#VALUE!"),
        ("=FORECAST.ETS.STAT(B1:B36,A1:A36,1,8761)", "#NUM!"),
    ] {
        assert_eq!(text(&mut model, formula), want, "{formula}");
    }
}

#[test]
fn confint_follows_the_closed_form_for_non_seasonal_and_seasonal_series() {
    let timeline: Vec<f64> = (1..=24).map(f64::from).collect();
    let values: Vec<f64> = timeline
        .iter()
        .map(|t| 10.0 + 2.0 * t + (t * 1.7).sin() * 3.0)
        .collect();
    let mut model = series(&timeline, &values);
    let rmse = number(&mut model, "=FORECAST.ETS.STAT(B1:B24,A1:A24,7,0)");
    let z = number(&mut model, "=NORM.S.INV(0.975)");
    close(
        number(&mut model, "=FORECAST.ETS.CONFINT(25,B1:B24,A1:A24,0.95,0)"),
        z * rmse,
    );
    let far = number(&mut model, "=FORECAST.ETS.CONFINT(30,B1:B24,A1:A24,0.95,0)");
    assert!(far > z * rmse, "{far}");
    close(
        number(&mut model, "=FORECAST.ETS.CONFINT(30,B1:B24,A1:A24,,0)"),
        far,
    );
    assert!(number(&mut model, "=FORECAST.ETS.CONFINT(30,B1:B24,A1:A24,0.8,0)") < far);
    let between = number(
        &mut model,
        "=FORECAST.ETS.CONFINT(25.5,B1:B24,A1:A24,0.95,0)",
    );
    assert!(
        between > z * rmse
            && between < number(&mut model, "=FORECAST.ETS.CONFINT(26,B1:B24,A1:A24,0.95,0)")
    );
    model._set("H1", "=FORECAST.ETS.CONFINT({25;30},B1:B24,A1:A24,0.95,0)");
    model.evaluate();
    close(model._get_text("H2").parse().unwrap(), far);

    // Seasonal closed-form interval: exact z * RMSE at step 1, wider at higher confidence,
    // deterministic across recalculations.
    let mut seas = seasonal();
    let seas_rmse = number(&mut seas, "=FORECAST.ETS.STAT(B1:B36,A1:A36,7,12)");
    let z90 = number(&mut seas, "=NORM.S.INV(0.95)");
    let ci90 = number(&mut seas, "=FORECAST.ETS.CONFINT(37,B1:B36,A1:A36,0.9,12)");
    close(ci90, z90 * seas_rmse);
    assert!(number(&mut seas, "=FORECAST.ETS.CONFINT(37,B1:B36,A1:A36,0.99,12)") > ci90);
    seas._set("H1", "=FORECAST.ETS.CONFINT(D1:D3,B1:B36,A1:A36,0.9,12)");
    seas.evaluate();
    let first: Vec<String> = (1..=3).map(|i| seas._get_text(&format!("H{i}"))).collect();
    seas._set("B1", "112");
    seas.evaluate();
    let second: Vec<String> = (1..=3).map(|i| seas._get_text(&format!("H{i}"))).collect();
    assert_eq!(first, second);
}

#[test]
fn the_documented_errors_of_confint() {
    let mut model = seasonal();
    let cases = [
        ("=FORECAST.ETS.CONFINT(37,B1:B36,A1:A36,0)", "#NUM!"),
        ("=FORECAST.ETS.CONFINT(37,B1:B36,A1:A36,1)", "#NUM!"),
        ("=FORECAST.ETS.CONFINT(37,B1:B36,A1:A36,-0.5)", "#NUM!"),
        ("=FORECAST.ETS.CONFINT(37,B1:B36,A1:A36,1.5)", "#NUM!"),
        ("=FORECAST.ETS.CONFINT(37,B1:B36,A1:A36,0.9,8761)", "#NUM!"),
        ("=FORECAST.ETS.CONFINT(37,B1:B36,A1:A35,0.9,12)", "#N/A"),
        ("=FORECAST.ETS.CONFINT(36,B1:B36,A1:A36,0.9,12)", "#NUM!"),
        ("=FORECAST.ETS.CONFINT(37,B1:B36)", "#VALUE!"),
        (
            "=FORECAST.ETS.CONFINT(37,B1:B36,A1:A36,0.9,12,1,1,1)",
            "#VALUE!",
        ),
    ];
    for (formula, want) in cases {
        assert_eq!(text(&mut model, formula), want, "{formula}");
    }
}

fn limited(model: &mut Model, formula: &str, want: &str, limit: Duration) {
    let start = Instant::now();
    let got = text(model, formula);
    assert!(
        start.elapsed() < limit,
        "{formula} took {:?}",
        start.elapsed()
    );
    assert!(got.starts_with(want), "{formula} gave {got}");
}

#[test]
fn point_cap_and_cell_step_budget_fail_closed() {
    let mut model = new_empty_model();
    // 40,000 points exceed MAX_POINTS (32,768) and fail closed with #VALUE!.
    limited(
        &mut model,
        "=FORECAST.ETS(40001,SEQUENCE(40000)*2,SEQUENCE(40000),1)",
        "#VALUE!",
        Duration::from_secs(5),
    );
    // FFT seasonality detection on 30,000 points completes well within budget and finds period 7.
    assert_eq!(
        text(
            &mut model,
            "=FORECAST.ETS.SEASONALITY(SEQUENCE(30000)*2+MOD(SEQUENCE(30000),7),SEQUENCE(30000))"
        ),
        "7"
    );
    // Chaining enough heavy 20,000-point fits in one formula exceeds MAX_EVAL_STEPS and aborts
    // with #VALUE! instead of running unbounded.
    let term = "FORECAST.ETS(20001,SEQUENCE(20000)+SIN(SEQUENCE(20000)),SEQUENCE(20000),8760)";
    let chained = format!("={}", vec![term; 12].join("+"));
    limited(&mut model, &chained, "#VALUE!", Duration::from_secs(15));
}

#[test]
fn the_longest_seasonal_period_of_the_documentation_works() {
    let mut model = new_empty_model();
    let formula = "=FORECAST.ETS(17521,SEQUENCE(17520)+10*SIN(SEQUENCE(17520)*2*PI()/8760),SEQUENCE(17520),8760)";
    let start = Instant::now();
    let got = text(&mut model, formula);
    assert!(
        start.elapsed() < Duration::from_secs(30),
        "{:?}",
        start.elapsed()
    );
    assert!(got.parse::<f64>().is_ok(), "{got}");
    let short = "=FORECAST.ETS(17520,SEQUENCE(17519)+10*SIN(SEQUENCE(17519)*2*PI()/8760),SEQUENCE(17519),8760)";
    assert_eq!(text(&mut model, short), "#NUM!");
}

#[test]
fn infinite_and_extreme_numbers_are_errors_not_hangs() {
    let mut model = seasonal();
    let limit = Duration::from_secs(5);
    limited(
        &mut model,
        "=FORECAST.ETS(37,B1:B36*1E308*1E308,A1:A36,12)",
        "#",
        limit,
    );
    limited(
        &mut model,
        "=FORECAST.ETS(1E308*10,B1:B36,A1:A36,12)",
        "#",
        limit,
    );
    limited(
        &mut model,
        "=FORECAST.ETS(1E308,B1:B36,A1:A36,12)",
        "#NUM!",
        limit,
    );
    limited(
        &mut model,
        "=FORECAST.ETS(1E308,B1:B36,A1:A36,0)",
        "#NUM!",
        limit,
    );
    limited(
        &mut model,
        "=FORECAST.ETS(1E308,B1:B36*1E307,A1:A36,0)",
        "#",
        limit,
    );
    limited(
        &mut model,
        "=FORECAST.ETS(37,B1:B36*1E306,A1:A36,12)",
        "#",
        limit,
    );
    limited(
        &mut model,
        "=FORECAST.ETS(1E300,B1:B36,A1:A36*1E299,12)",
        "#",
        limit,
    );
    limited(
        &mut model,
        "=FORECAST.ETS.CONFINT(1E308,B1:B36,A1:A36,0.9,12)",
        "#NUM!",
        limit,
    );
    limited(
        &mut model,
        "=FORECAST.ETS.CONFINT(1E308,B1:B36,A1:A36,0.9,0)",
        "#NUM!",
        limit,
    );
    // Valid extreme magnitudes (1e250 and 1e-250) compute exact scaled forecasts.
    let base = number(&mut model, "=FORECAST.ETS(37,B1:B36,A1:A36,12)");
    let big = number(&mut model, "=FORECAST.ETS(37,B1:B36*1E250,A1:A36,12)");
    let tiny = number(&mut model, "=FORECAST.ETS(37,B1:B36*1E-250,A1:A36,12)");
    close(big / 1e250, base);
    close(tiny / 1e-250, base);
}

#[test]
fn many_equal_dates_huge_gaps_and_batch_targets_are_cheap() {
    let mut model = new_empty_model();
    let limit = Duration::from_secs(5);
    limited(
        &mut model,
        "=FORECAST.ETS(2,SEQUENCE(30000),SEQUENCE(30000)*0+1)",
        "#NUM!",
        limit,
    );
    limited(
        &mut model,
        "=FORECAST.ETS(3,SEQUENCE(30000),MOD(SEQUENCE(30000),2)+1,0,1,7)",
        "#NUM!",
        limit,
    );
    limited(
        &mut model,
        "=FORECAST.ETS(2000000,{1;2;3;4;1000000},{1;2;3;4;1000000},0)",
        "#NUM!",
        limit,
    );
    // 100,000 target dates share a single fitted model instead of refitting per target.
    let batch_sum = number(
        &mut model,
        "=SUM(FORECAST.ETS(SEQUENCE(100000)+10,{2;4;6;8;10},{1;2;3;4;5},0))",
    );
    // Forecast at t = 10 + k (k = 1..100000) for y = 2t is 2*(10 + k); sum = 20*100000 + 100000*100001.
    close(batch_sum, 2_000_000.0 + 10_000_100_000.0);
}

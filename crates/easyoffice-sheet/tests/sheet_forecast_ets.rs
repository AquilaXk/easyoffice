//! `FORECAST.ETS`, `FORECAST.ETS.CONFINT`, `FORECAST.ETS.SEASONALITY` and `FORECAST.ETS.STAT` (PD-272):
//! the arguments and errors of the function documentation, and the limits on untrusted input.
//!
//! The series and the values of the seasonal cases are the test vectors of the calculation's original
//! (`tests/data/forecast_ets_vectors.txt`), which come from the original program, not from the
//! reference app: a value that is not the reference app's would show only against its output
//! (`TODO.md`). Where the original's error is another one than the documented error, the documented
//! one is tested (the original gives `#VALUE!` for a timeline without a constant step and for a
//! series too short for its seasonality; the documentation says `#NUM!`).
#![allow(clippy::unwrap_used)]

mod dynamic_support;
use dynamic_support::{new_empty_model, Model};
use std::time::{Duration, Instant};

const VECTORS: &str = include_str!("data/forecast_ets_vectors.txt");

/// The numbers after `key` in the case `name` of the vectors file.
fn vector(name: &str, key: &str) -> Vec<f64> {
    let case = VECTORS
        .split("\ncase ")
        .find(|c| c.starts_with(&format!("{name} ")))
        .unwrap();
    let line = case.lines().find(|l| l.starts_with(key)).unwrap();
    line[key.len()..]
        .split_whitespace()
        .map(|n| n.parse().unwrap())
        .collect()
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

/// The seasonal series of 36 months and its forecasts of the original (the first value is the
/// last point of the series).
fn seasonal() -> (Model, Vec<f64>) {
    let (timeline, values) = (vector("add-59", "timeline"), vector("add-59", "values"));
    let mut model = series(&timeline, &values);
    for (i, t) in [37, 38, 39].iter().enumerate() {
        model._set(&format!("D{}", i + 1), &t.to_string());
    }
    (model, vector("add-59", "expect"))
}

#[test]
fn forecast_ets_gives_the_forecast_of_the_original_and_spills_for_an_array_of_dates() {
    let (mut model, expect) = seasonal();
    close(
        number(&mut model, "=FORECAST.ETS(37,B1:B36,A1:A36,12)"),
        expect[1],
    );
    // the end of the timeline is not before it: the last value
    close(
        number(&mut model, "=FORECAST.ETS(36,B1:B36,A1:A36,12)"),
        166.0,
    );
    // an array of dates gives an array of the same shape
    model._set("H1", "=FORECAST.ETS(D1:D3,B1:B36,A1:A36,12)");
    model.evaluate();
    for (i, want) in expect[1..4].iter().enumerate() {
        close(
            model._get_text(&format!("H{}", i + 1)).parse().unwrap(),
            *want,
        );
    }
    assert_eq!(model._get_text("H4"), "");
    // a row of dates gives a row
    model._set("H1", "=FORECAST.ETS(TRANSPOSE(D1:D3),B1:B36,A1:A36,12)");
    model.evaluate();
    close(model._get_text("J1").parse().unwrap(), expect[3]);
    // default seasonality (detected) and the other defaults: the same series gives a period of 12
    close(
        number(&mut model, "=FORECAST.ETS(37,B1:B36,A1:A36)"),
        expect[1],
    );
    close(
        number(&mut model, "=FORECAST.ETS(37,B1:B36,A1:A36,12,1,1)"),
        expect[1],
    );
    close(
        number(&mut model, "=FORECAST.ETS(37,B1:B36,A1:A36,12,1,0)"),
        expect[1],
    );
}

#[test]
fn forecast_ets_of_a_short_series_and_of_array_constants() {
    let mut model = series(&[1.0, 2.0, 3.0, 4.0, 5.0], &[2.0, 4.0, 7.0, 8.0, 11.0]);
    // the detected period of this series is 2 (`FORECAST.ETS.SEASONALITY`), 0 is the trend alone
    close(number(&mut model, "=FORECAST.ETS(6,B1:B5,A1:A5)"), 12.5);
    close(number(&mut model, "=FORECAST.ETS(6,B1:B5,A1:A5,0)"), 13.25);
    assert_eq!(
        text(&mut model, "=FORECAST.ETS.SEASONALITY(B1:B5,A1:A5)"),
        "2"
    );
    // an unsorted timeline is sorted by the function
    let mut model = series(&[3.0, 1.0, 2.0, 5.0, 4.0], &[7.0, 2.0, 4.0, 11.0, 8.0]);
    close(number(&mut model, "=FORECAST.ETS(6,B1:B5,A1:A5)"), 12.5);
    // array constants
    let mut model = new_empty_model();
    close(number(&mut model, "=FORECAST.ETS(8,{1;2;3},{4;5;6})"), 5.0);
    close(
        number(&mut model, "=FORECAST.ETS(50,{-1;-2;-3;-4},{10;20;30;40})"),
        -5.0,
    );
}

#[test]
fn a_target_date_before_the_end_of_the_timeline_is_num() {
    let (mut model, _) = seasonal();
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
    // one early date in an array fails the whole call
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
    let (mut model, _) = seasonal();
    let cases = [
        // a timeline with no constant step (the original gives #VALUE!)
        ("=FORECAST.ETS(41,{6;7;9;15;21},{20;28;31;38;40})", "#NUM!"),
        // values and timeline of another size
        ("=FORECAST.ETS(37,B1:B36,A1:A35,12)", "#N/A"),
        ("=FORECAST.ETS(37,B1:B35,A1:A36,12)", "#N/A"),
        ("=FORECAST.ETS(37,B1:B36,TRANSPOSE(A1:A36),12)", "#N/A"),
        // seasonality: whole number from 0 to 8,760
        ("=FORECAST.ETS(37,B1:B36,A1:A36,8761)", "#NUM!"),
        ("=FORECAST.ETS(37,B1:B36,A1:A36,-1)", "#NUM!"),
        ("=FORECAST.ETS(37,B1:B36,A1:A36,1.5)", "#NUM!"),
        ("=FORECAST.ETS(37,B1:B36,A1:A36,\"x\")", "#VALUE!"),
        // a seasonal period of more than half of the series (the original gives #VALUE!)
        ("=FORECAST.ETS(37,B1:B36,A1:A36,19)", "#NUM!"),
        // data_completion is 0 or 1, aggregation 0 to 7
        ("=FORECAST.ETS(37,B1:B36,A1:A36,12,2)", "#NUM!"),
        ("=FORECAST.ETS(37,B1:B36,A1:A36,12,1,8)", "#NUM!"),
        ("=FORECAST.ETS(37,B1:B36,A1:A36,12,1,-1)", "#NUM!"),
        // a text, a blank or a boolean in the series, an error in any argument
        ("=FORECAST.ETS(37,{1;\"a\";3;4},{1;2;3;4})", "#VALUE!"),
        ("=FORECAST.ETS(37,{1;2;3;4},{1;TRUE;3;4})", "#VALUE!"),
        ("=FORECAST.ETS(37,{1;#DIV/0!;3;4},{1;2;3;4})", "#DIV/0!"),
        ("=FORECAST.ETS(#N/A,B1:B36,A1:A36,12)", "#N/A"),
        ("=FORECAST.ETS(\"x\",B1:B36,A1:A36,12)", "#VALUE!"),
        // blank cells in the series
        ("=FORECAST.ETS(37,B1:B40,A1:A40,12)", "#VALUE!"),
        // too few arguments, too many
        ("=FORECAST.ETS(37,B1:B36)", "#VALUE!"),
        ("=FORECAST.ETS(37,B1:B36,A1:A36,12,1,1,1)", "#VALUE!"),
        // a series of one point or two points that cannot be a seasonal one
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
    // the average (the default, and the documented 0) of 3 and 5 at 2 gives the series 2 4 7 8 11
    for aggregation in ["", ",1", ",0"] {
        let formula = format!("=FORECAST.ETS(6,B1:B6,A1:A6,0,1{aggregation})");
        close(number(&mut model, &formula), 13.25);
    }
    // each method gives what the series of the combined values gives
    for (aggregation, combined) in [
        (7, "{2;8;7;8;11}"),
        (4, "{2;5;7;8;11}"),
        (6, "{2;3;7;8;11}"),
        (2, "{2;2;7;8;11}"),
        (3, "{2;2;7;8;11}"),
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
    // the minimum and the sum are not the average
    let min = number(&mut model, "=FORECAST.ETS(6,B1:B6,A1:A6,0,1,6)");
    assert!((min - 13.25).abs() > 1e-3, "{min}");
}

#[test]
fn a_gap_in_the_timeline_is_filled_up_to_thirty_percent() {
    // 1..10 without 5: 1 of 9 points, 11 percent
    let timeline = [1.0, 2.0, 3.0, 4.0, 6.0, 7.0, 8.0, 9.0, 10.0];
    let values = [1.0, 2.0, 3.0, 4.0, 6.0, 7.0, 8.0, 9.0, 10.0];
    let mut model = series(&timeline, &values);
    close(number(&mut model, "=FORECAST.ETS(11,B1:B9,A1:A9,0)"), 11.0);
    close(
        number(&mut model, "=FORECAST.ETS(11,B1:B9,A1:A9,0,0)"),
        11.0 - 0.0,
    );
    // 1, 2, 3, 4 and 10: 5 of 5 points are missing
    let mut model = series(&[1.0, 2.0, 3.0, 4.0, 10.0], &[1.0, 2.0, 3.0, 4.0, 10.0]);
    assert_eq!(text(&mut model, "=FORECAST.ETS(11,B1:B5,A1:A5,0)"), "#NUM!");
}

#[test]
fn a_monthly_timeline_of_dates_is_read_in_months() {
    // the first of every month of 2001 to 2003, 36 points of the seasonal series
    let values = vector("add-59", "values");
    let mut model = new_empty_model();
    for (i, v) in values.iter().enumerate() {
        let (year, month) = (2001 + i / 12, i % 12 + 1);
        model._set(&format!("A{}", i + 1), &format!("=DATE({year},{month},1)"));
        model._set(&format!("B{}", i + 1), &v.to_string());
    }
    let expect = vector("add-59", "expect");
    close(
        number(&mut model, "=FORECAST.ETS(DATE(2004,1,1),B1:B36,A1:A36,12)"),
        expect[1],
    );
    close(
        number(&mut model, "=FORECAST.ETS(DATE(2004,2,1),B1:B36,A1:A36,12)"),
        expect[2],
    );
}

#[test]
fn seasonality_is_the_detected_period() {
    let (mut model, _) = seasonal();
    assert_eq!(
        text(&mut model, "=FORECAST.ETS.SEASONALITY(B1:B36,A1:A36)"),
        "12"
    );
    assert_eq!(
        text(&mut model, "=FORECAST.ETS.SEASONALITY(B1:B36,A1:A36,1,1)"),
        "12"
    );
    // no seasonality in a straight line
    let mut model = series(&[1.0, 2.0, 3.0, 4.0, 5.0], &[2.0, 4.0, 6.0, 8.0, 10.0]);
    assert_eq!(
        text(&mut model, "=FORECAST.ETS.SEASONALITY(B1:B5,A1:A5)"),
        "1"
    );
    // errors
    assert_eq!(
        text(&mut model, "=FORECAST.ETS.SEASONALITY(B1:B5,A1:A4)"),
        "#N/A"
    );
    assert_eq!(
        text(
            &mut model,
            "=FORECAST.ETS.SEASONALITY({6;7;9;15;21},{20;28;31;38;40})"
        ),
        "#NUM!"
    );
    assert_eq!(
        text(&mut model, "=FORECAST.ETS.SEASONALITY(B1:B5)"),
        "#VALUE!"
    );
    assert_eq!(
        text(&mut model, "=FORECAST.ETS.SEASONALITY(B1:B5,A1:A5,2)"),
        "#NUM!"
    );
    assert_eq!(
        text(&mut model, "=FORECAST.ETS.SEASONALITY(B1:B5,A1:A5,1,8)"),
        "#NUM!"
    );
}

#[test]
fn stat_gives_the_statistics_in_the_documented_order() {
    let (mut model, _) = seasonal();
    // step size (8) and the whole list as an array
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
    // alpha, beta and gamma are weights; the errors are not negative; the step is 1
    assert!(
        stats[..3].iter().all(|w| (0.0..=1.0).contains(w)),
        "{stats:?}"
    );
    assert!(stats[3..7].iter().all(|e| *e >= 0.0), "{stats:?}");
    assert_eq!(stats[7], 1.0);
    // RMSE is not less than MAE
    assert!(stats[6] >= stats[5], "{stats:?}");
    // a row of types gives a row
    model._set("H1", "=FORECAST.ETS.STAT(B1:B36,A1:A36,{1,8},12)");
    model.evaluate();
    close(model._get_text("H1").parse().unwrap(), stats[0]);
    assert_eq!(model._get_text("I1"), "1");
    // a type outside 1 to 8 (the documentation names no error: #NUM!)
    assert_eq!(
        text(&mut model, "=FORECAST.ETS.STAT(B1:B36,A1:A36,0,12)"),
        "#NUM!"
    );
    assert_eq!(
        text(&mut model, "=FORECAST.ETS.STAT(B1:B36,A1:A36,9,12)"),
        "#NUM!"
    );
    assert_eq!(
        text(&mut model, "=FORECAST.ETS.STAT(B1:B36,A1:A36,1.5,12)"),
        "#NUM!"
    );
    assert_eq!(
        text(&mut model, "=FORECAST.ETS.STAT(B1:B36,A1:A35,1,12)"),
        "#N/A"
    );
    assert_eq!(
        text(&mut model, "=FORECAST.ETS.STAT(B1:B36,A1:A36)"),
        "#VALUE!"
    );
    assert_eq!(
        text(&mut model, "=FORECAST.ETS.STAT(B1:B36,A1:A36,1,8761)"),
        "#NUM!"
    );
}

#[test]
fn confint_without_seasonality_is_the_closed_form_interval() {
    let timeline: Vec<f64> = (1..=24).map(f64::from).collect();
    let values: Vec<f64> = timeline
        .iter()
        .map(|t| 10.0 + 2.0 * t + (t * 1.7).sin() * 3.0)
        .collect();
    let mut model = series(&timeline, &values);
    let rmse = number(&mut model, "=FORECAST.ETS.STAT(B1:B24,A1:A24,7,0)");
    let z = number(&mut model, "=NORM.S.INV(0.975)");
    // the interval grows with the distance and is z * rmse at the first step
    close(
        number(&mut model, "=FORECAST.ETS.CONFINT(25,B1:B24,A1:A24,0.95,0)"),
        z * rmse,
    );
    let far = number(&mut model, "=FORECAST.ETS.CONFINT(30,B1:B24,A1:A24,0.95,0)");
    assert!(far > z * rmse * 2.0, "{far}");
    // the default level is 95 percent, a lower level is a narrower interval
    close(
        number(&mut model, "=FORECAST.ETS.CONFINT(30,B1:B24,A1:A24,,0)"),
        far,
    );
    assert!(number(&mut model, "=FORECAST.ETS.CONFINT(30,B1:B24,A1:A24,0.8,0)") < far);
    // dates between two steps are interpolated
    let between = number(
        &mut model,
        "=FORECAST.ETS.CONFINT(25.5,B1:B24,A1:A24,0.95,0)",
    );
    assert!(
        between > z * rmse
            && between < number(&mut model, "=FORECAST.ETS.CONFINT(26,B1:B24,A1:A24,0.95,0)")
    );
    // an array of dates
    model._set("H1", "=FORECAST.ETS.CONFINT({25;30},B1:B24,A1:A24,0.95,0)");
    model.evaluate();
    close(model._get_text("H2").parse().unwrap(), far);
}

#[test]
fn confint_of_a_seasonal_series_is_simulated_and_the_same_every_time() {
    let (mut model, _) = seasonal();
    let first = number(&mut model, "=FORECAST.ETS.CONFINT(37,B1:B36,A1:A36,0.9,12)");
    let again = number(&mut model, "=FORECAST.ETS.CONFINT(37,B1:B36,A1:A36,0.9,12)");
    assert!(first > 0.0, "{first}");
    assert_eq!(first, again);
    // a higher level is a wider interval
    assert!(
        number(
            &mut model,
            "=FORECAST.ETS.CONFINT(37,B1:B36,A1:A36,0.99,12)"
        ) > first
    );
    // detected seasonality
    assert!(number(&mut model, "=FORECAST.ETS.CONFINT(40,B1:B36,A1:A36)") > 0.0);
}

#[test]
fn the_documented_errors_of_confint() {
    let (mut model, _) = seasonal();
    let cases = [
        ("=FORECAST.ETS.CONFINT(37,B1:B36,A1:A36,0)", "#NUM!"),
        ("=FORECAST.ETS.CONFINT(37,B1:B36,A1:A36,1)", "#NUM!"),
        ("=FORECAST.ETS.CONFINT(37,B1:B36,A1:A36,-0.5)", "#NUM!"),
        ("=FORECAST.ETS.CONFINT(37,B1:B36,A1:A36,1.5)", "#NUM!"),
        ("=FORECAST.ETS.CONFINT(37,B1:B36,A1:A36,0.9,8761)", "#NUM!"),
        ("=FORECAST.ETS.CONFINT(37,B1:B36,A1:A35,0.9,12)", "#N/A"),
        // a target on the end of the series has no interval
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

/// A formula that has to finish within `limit` and give `want` (a prefix of the text).
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
fn a_very_long_series_is_refused_before_any_work() {
    let mut model = new_empty_model();
    // 40,000 points: more than the calculation takes
    limited(
        &mut model,
        "=FORECAST.ETS(40001,SEQUENCE(40000)*2,SEQUENCE(40000),1)",
        "#VALUE!",
        Duration::from_secs(5),
    );
    // detecting the seasonality of a long series costs more than the budget of one cell
    limited(
        &mut model,
        "=FORECAST.ETS(30001,SEQUENCE(30000)*2+MOD(SEQUENCE(30000),7),SEQUENCE(30000))",
        "#VALUE!",
        Duration::from_secs(8),
    );
}

#[test]
fn the_longest_seasonal_period_of_the_documentation_works() {
    // a year of hourly values twice: seasonality 8,760
    let mut model = new_empty_model();
    let formula = "=FORECAST.ETS(17521,SEQUENCE(17520)+10*SIN(SEQUENCE(17520)*2*PI()/8760),SEQUENCE(17520),8760)";
    let start = Instant::now();
    let got = text(&mut model, formula);
    assert!(
        start.elapsed() < Duration::from_secs(60),
        "{:?}",
        start.elapsed()
    );
    assert!(got.parse::<f64>().is_ok(), "{got}");
    // one point short of two periods
    let formula = "=FORECAST.ETS(17520,SEQUENCE(17519)+10*SIN(SEQUENCE(17519)*2*PI()/8760),SEQUENCE(17519),8760)";
    assert_eq!(text(&mut model, formula), "#NUM!");
}

#[test]
fn infinite_and_extreme_numbers_are_errors_not_hangs() {
    let (mut model, _) = seasonal();
    let limit = Duration::from_secs(5);
    // a number that does not fit is an error of the cell before it reaches the function
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
    // a date at the end of the number range
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
        "#NUM!",
        limit,
    );
    // values at the end of the number range
    limited(
        &mut model,
        "=FORECAST.ETS(37,B1:B36*1E306,A1:A36,12)",
        "#",
        limit,
    );
    // a timeline of huge numbers
    limited(
        &mut model,
        "=FORECAST.ETS(1E300,B1:B36,A1:A36*1E299,12)",
        "#",
        limit,
    );
    // the interval to a date far away: simulated for a seasonal series, which is refused;
    // the closed form for a series without seasonality, which is a number or #NUM!
    limited(
        &mut model,
        "=FORECAST.ETS.CONFINT(1E9,B1:B36,A1:A36,0.9,12)",
        "#VALUE!",
        limit,
    );
    limited(
        &mut model,
        "=FORECAST.ETS.CONFINT(1E308,B1:B36,A1:A36,0.9,0)",
        "#NUM!",
        limit,
    );
    // a statistic of an extreme series is a number or #NUM!
    limited(
        &mut model,
        "=FORECAST.ETS.STAT(B1:B36*1E306,A1:A36,4,12)",
        "",
        limit,
    );
}

#[test]
fn many_equal_dates_and_huge_gaps_are_cheap() {
    let mut model = new_empty_model();
    let limit = Duration::from_secs(5);
    // 30,000 values at one date are one point
    limited(
        &mut model,
        "=FORECAST.ETS(2,SEQUENCE(30000),SEQUENCE(30000)*0+1)",
        "#NUM!",
        limit,
    );
    // two dates, 30,000 values each: two points
    limited(
        &mut model,
        "=FORECAST.ETS(3,SEQUENCE(30000),MOD(SEQUENCE(30000),2)+1,0,1,7)",
        "",
        limit,
    );
    // a gap of a million steps is over the 30 percent that may be missing
    limited(
        &mut model,
        "=FORECAST.ETS(2000000,{1;2;3;4;1000000},{1;2;3;4;1000000},0)",
        "#NUM!",
        limit,
    );
    // many target dates
    limited(
        &mut model,
        "=SUM(FORECAST.ETS(SEQUENCE(100000)+10,{1;2;3;4;5},{1;2;3;4;5},0))",
        "",
        limit,
    );
}

#[test]
fn a_recalculation_gives_the_same_interval() {
    let (mut model, _) = seasonal();
    model._set("H1", "=FORECAST.ETS.CONFINT(D1:D3,B1:B36,A1:A36,0.9,12)");
    model.evaluate();
    let first: Vec<String> = (1..=3).map(|i| model._get_text(&format!("H{i}"))).collect();
    model._set("B1", "112");
    model.evaluate();
    let second: Vec<String> = (1..=3).map(|i| model._get_text(&format!("H{i}"))).collect();
    assert_eq!(first, second);
    assert!(first.iter().all(|t| t.parse::<f64>().is_ok()), "{first:?}");
}

//! Measurement only: the engine's time for the auto filter requests over 100,000 data rows
//! (`cargo test --release -p easyoffice-app --test filter_bench -- --ignored --nocapture`). The
//! request is JSON through the router, as the page sends it; the time covers parsing it, judging
//! the rows and writing the answer.

use easyoffice_app::EasyOfficeIpcRouter;
use std::fmt::Write;
use std::time::Instant;

const ROWS: usize = 100_000;

fn texts(f: impl Fn(usize) -> String) -> String {
    let mut out = String::from("[");
    for i in 0..ROWS {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "\"{}\"", f(i));
    }
    out.push(']');
    out
}

fn median_ms(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}

fn time(label: &str, body: &str) {
    let mut samples = Vec::new();
    let mut size = 0;
    for _ in 0..7 {
        let start = Instant::now();
        let answer = EasyOfficeIpcRouter::dispatch("evaluate_sheet", body).unwrap();
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
        size = answer.len();
    }
    println!(
        "{label:<46} {:>8.1} ms  (request {:>5} KB, answer {:>5} KB)",
        median_ms(samples),
        body.len() / 1024,
        size / 1024
    );
}

#[test]
#[ignore = "measurement"]
fn filter_requests_over_100000_rows() {
    let dept = texts(|i| format!("부서{}", i % 120));
    let amount = texts(|i| format!("{}", (i * 7919) % 100_003));
    let unique = texts(|i| format!("항목-{i}"));
    let dates = texts(|i| format!("{}", 40_000 + i % 5_000));
    let range = format!("A1:D{}", ROWS + 1);
    let fmt = format!(r#","fmt":[[0,{},"yyyy-mm-dd"]]"#, ROWS - 1);
    let column = |col: u32, values: &str, extra: &str| {
        format!(r#"{{"col":{col},"values":{values}{extra}}}"#)
    };
    let (a, b, c, d) = (
        column(1, &dept, ""),
        column(2, &amount, ""),
        column(3, &unique, ""),
        column(4, &dates, &fmt),
    );
    // Only the columns a request needs travel, as the page sends them.
    let request = |op: &str, columns: &str, data: &[&String], extra: &str| {
        format!(
            r#"{{"sessionAction":"filter","op":"{op}","filter":{{"range":"{range}","columns":[{columns}]}},{extra}"data":[{}]}}"#,
            data.iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    let selected: Vec<String> = (0..40).map(|i| format!("\"부서{i}\"")).collect();
    let values_col = format!(r#"{{"col":1,"values":[{}]}}"#, selected.join(","));
    let greater = r#"{"col":2,"conditions":[{"op":"greaterThan","text":"50000"}]}"#;
    let wildcard = r#"{"col":3,"conditions":[{"text":"*9?7*"}]}"#;
    let both = format!("{values_col},{greater}");
    time(
        "apply, values (40 of 120)",
        &request("apply", &values_col, &[&a], ""),
    );
    time(
        "apply, numbers above 50000",
        &request("apply", greater, &[&b], ""),
    );
    time(
        "apply, wildcard *9?7* over 100,000 texts",
        &request("apply", wildcard, &[&c], ""),
    );
    time(
        "apply, values + number (two columns)",
        &request("apply", &both, &[&a, &b], ""),
    );
    time(
        "values, 120 distinct",
        &request("values", &values_col, &[&a], r#""col":1,"#),
    );
    time(
        "values, 100,000 distinct (cap 10,000)",
        &request("values", greater, &[&b, &c], r#""col":3,"#),
    );
    time(
        "values, date format over 5,000 distinct",
        &request("values", greater, &[&b, &d], r#""col":4,"#),
    );
    time(
        "values, search in 100,000 distinct",
        &request("values", greater, &[&b, &c], r#""col":3,"query":"99","#),
    );
    time("apply, none (empty filter)", &request("apply", "", &[], ""));
    // The criteria of PD-226. The date column's dates are 5,000 distinct days of 2009 to 2022; the colour
    // column has 50,000 alternating one-row runs.
    let top = r#"{"col":2,"top":{"count":10}}"#;
    let percent = r#"{"col":2,"top":{"bottom":true,"percent":true,"count":25}}"#;
    let average = r#"{"col":2,"average":{"above":true}}"#;
    let year =
        r#"{"col":4,"dates":[{"unit":"year","year":2015},{"unit":"month","year":2012,"month":6}]}"#;
    let runs: Vec<String> = (0..ROWS / 2)
        .map(|i| format!("[{},{},\"ffc7ce\"]", 2 * i, 2 * i))
        .collect();
    let colored = column(1, &dept, &format!(r#","fill":[{}]"#, runs.join(",")));
    let color = r#"{"col":1,"color":{"rgb":"ffc7ce"}}"#;
    time("apply, top 10 numbers", &request("apply", top, &[&b], ""));
    time(
        "apply, bottom 25 percent",
        &request("apply", percent, &[&b], ""),
    );
    time(
        "apply, above average",
        &request("apply", average, &[&b], ""),
    );
    time(
        "apply, date groups (year, month)",
        &request("apply", year, &[&d], ""),
    );
    time(
        "apply, fill colour (50,000 runs)",
        &request("apply", color, &[&colored], ""),
    );
    time(
        "apply, top 10 after a value list",
        &request("apply", &format!("{values_col},{top}"), &[&a, &b], ""),
    );
    time(
        "values, date tree over 5,000 days",
        &request("values", year, &[&d], r#""col":4,"#),
    );
}

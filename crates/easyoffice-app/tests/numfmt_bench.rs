//! Measurement only: save and open of a 100,000-cell sheet with and without number formats, and
//! the engine's display text for them (`cargo test --release -p easyoffice-app --test numfmt_bench
//! -- --ignored --nocapture`). It talks to the engine through JSON only, so the same file runs on a
//! checkout that has no number formats (the formats are then ignored and the sheet is plain).

use easyoffice_app::EasyOfficeIpcRouter;
use std::fmt::Write;
use std::time::Instant;

const ROWS: usize = 5_000;
const COLS: usize = 20;
const CODES: [&str; 3] = ["#,##0.00", "0.0%", "\"₩\"#,##0_);[Red](\"₩\"#,##0)"];

fn col_name(c: usize) -> char {
    (b'A' + c as u8) as char
}

/// Save payload of the sheet; every cell carries one of the codes when `formatted`.
fn payload(formatted: bool) -> String {
    let mut out = String::from(
        r#"{"sourceFormat":"xlsx","targetFormat":"xlsx","title":"b","sheets":[{"name":"S""#,
    );
    if formatted {
        out.push_str(r#","numFmts":["#);
        let codes: Vec<String> = CODES
            .iter()
            .map(|c| format!("\"{}\"", c.replace('\\', "\\\\").replace('"', "\\\"")))
            .collect();
        out.push_str(&codes.join(","));
        out.push(']');
    }
    out.push_str(r#","cells":["#);
    let mut first = true;
    for r in 1..=ROWS {
        for c in 0..COLS {
            if !first {
                out.push(',');
            }
            first = false;
            let value = (r * 131 + c * 7) % 5003;
            let _ = write!(out, r#"{{"ref":"{}{r}","raw":"{value}.25""#, col_name(c));
            if formatted {
                let _ = write!(out, r#","style":{{"nf":{}}}"#, c % CODES.len() + 1);
            }
            out.push('}');
        }
    }
    out.push_str("]}]}");
    out
}

fn field<'a>(json: &'a str, key: &str) -> &'a str {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    &json[start..end]
}

fn median_ms(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}

fn run(runs: usize, mut f: impl FnMut()) -> f64 {
    median_ms(
        (0..runs)
            .map(|_| {
                let start = Instant::now();
                f();
                start.elapsed().as_secs_f64() * 1000.0
            })
            .collect(),
    )
}

#[test]
#[ignore]
fn save_open_and_display_of_100k_cells() {
    for formatted in [false, true] {
        let body = payload(formatted);
        let mut xlsx = String::new();
        let save_ms = run(5, || {
            let json = EasyOfficeIpcRouter::dispatch("convert_document_format", &body).unwrap();
            xlsx = field(&json, "convertedBase64").to_string();
        });
        let bytes = xlsx.len() * 3 / 4;
        let open_body =
            format!(r#"{{"action":"decode","fileName":"b.xlsx","sourceBase64":"{xlsx}"}}"#);
        let mut opened = 0usize;
        let open_ms = run(5, || {
            opened = EasyOfficeIpcRouter::dispatch("convert_document_format", &open_body)
                .unwrap()
                .len();
        });
        println!(
            "{:<10} save {save_ms:>8.1} ms  open {open_ms:>8.1} ms  xlsx {bytes:>9} B  decode answer {opened:>9} B",
            if formatted { "formatted" } else { "plain" }
        );
    }
    // Display text: one screen of cells (2,000) and the whole sheet (100,000) under one code.
    for (name, count) in [("window 2,000", 2_000usize), ("all 100,000", 50_000)] {
        let values: Vec<String> = (0..count).map(|i| format!("\"{}.25\"", i % 5003)).collect();
        let request = format!(
            r##"{{"sessionAction":"format","items":[{{"code":"#,##0.00","values":[{}]}}]}}"##,
            values.join(",")
        );
        if EasyOfficeIpcRouter::dispatch("evaluate_sheet", &request).is_err() {
            println!("display {name:<12} not available (no format action)");
            continue;
        }
        let ms = run(5, || {
            EasyOfficeIpcRouter::dispatch("evaluate_sheet", &request).unwrap();
        });
        println!("display {name:<12} {ms:>8.1} ms ({count} values)");
    }
}

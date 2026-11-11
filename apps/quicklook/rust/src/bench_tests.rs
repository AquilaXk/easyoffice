//! Measurement harness of the Quick Look paths for `docs/performance.md`, not a check. One
//! `#[ignore]` test reads `EASYOFFICE_BENCH_CASE` (`ql_pdf`, `ql_png`, `ql_embedded`) and
//! `EASYOFFICE_BENCH_INPUT` (a document) and prints one `BENCH ...` line, so
//! `scripts/bench-run.mjs` can run it in a fresh process N times under `/usr/bin/time -l`.
//! `EASYOFFICE_BENCH_REPEAT` repeats the timed part in one process (for a sampling profiler).

use crate::{fonts, render};
use std::path::Path;
use std::time::Instant;

/// The longer side Finder asks for in a large thumbnail.
const THUMBNAIL_SIDE_PX: u32 = 512;

fn ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

fn run(case: &str, input: &Path) -> String {
    let name = input.file_name().unwrap().to_string_lossy().to_string();
    let bytes = std::fs::read(input).expect("read");
    let started = Instant::now();
    let mut phases: Vec<(&str, f64)> = Vec::new();
    let (out_bytes, pages) = match case {
        "ql_pdf" | "ql_png" => {
            let t = Instant::now();
            fonts::shared();
            phases.push(("fonts", ms(t)));
            let t = Instant::now();
            let (core, count) = render::open(&bytes).expect("open");
            phases.push(("open", ms(t)));
            if case == "ql_pdf" {
                let t = Instant::now();
                let (pdf, _) = render::pdf(&core, count).expect("pdf");
                phases.push(("pdf", ms(t)));
                (pdf.len(), count.min(render::MAX_PREVIEW_PAGES) as usize)
            } else {
                let t = Instant::now();
                let page = render::page(&core, 0).expect("page");
                phases.push(("svg0_parse", ms(t)));
                let t = Instant::now();
                let png = render::png(&page, THUMBNAIL_SIDE_PX).expect("png");
                phases.push(("png", ms(t)));
                (png.len(), count as usize)
            }
        }
        "ql_embedded" => {
            let found = rhwp::parser::extract_thumbnail_only(&bytes).expect("embedded picture");
            (found.data.len(), 0)
        }
        other => panic!("unknown case {other}"),
    };
    let phases: Vec<String> = phases.iter().map(|(n, v)| format!("{n}={v:.1}")).collect();
    format!(
        "BENCH tool=eo case={case} input={name} total_ms={:.1} out_bytes={out_bytes} pages={pages} phases={}",
        ms(started),
        phases.join(",")
    )
}

#[test]
#[ignore = "measurement, run by scripts/bench-run.mjs"]
fn bench_case() {
    let case = std::env::var("EASYOFFICE_BENCH_CASE").expect("EASYOFFICE_BENCH_CASE");
    let input = std::env::var("EASYOFFICE_BENCH_INPUT").expect("EASYOFFICE_BENCH_INPUT");
    let repeat: usize = std::env::var("EASYOFFICE_BENCH_REPEAT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    for _ in 0..repeat {
        println!("{}", run(&case, Path::new(&input)));
    }
}

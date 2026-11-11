//! Measurement only: renders 100,000 cells per format code (`cargo test -p easyoffice-sheet
//! --release --test format_render_bench -- --ignored --nocapture`).

use easyoffice_sheet::format_number_with_code;
use std::time::Instant;

const CELLS: usize = 100_000;
const CODES: [&str; 4] = ["#,##0.00", "0.0%", "yyyy-mm-dd", "#,##0;[Red]-#,##0"];

#[test]
#[ignore]
fn render_100k_cells_per_format() {
    for code in CODES {
        let start = Instant::now();
        let mut total = 0usize;
        for i in 0..CELLS {
            let v = 40_000.0 + (i as f64) * 0.37;
            total += format_number_with_code(v, code).map_or(0, |s| s.len());
        }
        println!(
            "{code:<22} {:>8.2} ms  ({total} bytes)",
            start.elapsed().as_secs_f64() * 1000.0
        );
    }
}

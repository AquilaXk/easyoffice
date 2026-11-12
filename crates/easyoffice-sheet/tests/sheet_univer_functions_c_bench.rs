//! Measurement only (`cargo test -p easyoffice-sheet --release --test
//! sheet_univer_functions_c_bench -- --ignored --nocapture --test-threads=1`): the byte text
//! functions over 100,000 cells of long mixed text and `AGGREGATE` over large ranges (PD-205).

use easyoffice_sheet::{EvaluatedCellValue, PackedCellCoordinate, SheetEngine};
use std::time::Instant;

const CELLS: u32 = 100_000;

fn a1(row: u32, col: u32) -> String {
    PackedCellCoordinate::new(row, col).unwrap().to_a1()
}

/// Mixed text of about 200 characters: Hangul, Latin, digits, kana, Han and full-width forms.
fn mixed_text(n: u32) -> String {
    let pieces = [
        "한국어 문장 ",
        "Hello World ",
        "0123456789 ",
        "カタカナひらがな ",
        "漢字表記 ",
        "ＡＢＣ１２３ ",
    ];
    let mut out = String::new();
    for i in 0..24 {
        out.push_str(pieces[((n + i) % 6) as usize]);
    }
    out
}

/// Best of three full recalculations of column B (`{r}` is the row number).
fn per_cell(label: &str, template: &str) {
    let mut s = SheetEngine::new("S");
    for r in 0..CELLS {
        let text = mixed_text(r).replace('"', "");
        s.set_cell(&a1(r, 0), format!("=\"{text}\"")).unwrap();
        s.set_cell(&a1(r, 1), template.replace("{r}", &(r + 1).to_string()))
            .unwrap();
    }
    let mut best = f64::INFINITY;
    let mut sample = EvaluatedCellValue::Empty;
    for _ in 0..3 {
        let start = Instant::now();
        let all = s.recalculate_all();
        best = best.min(start.elapsed().as_secs_f64());
        sample = all
            .get(&PackedCellCoordinate::new(7, 1).unwrap())
            .cloned()
            .unwrap_or(EvaluatedCellValue::Empty);
    }
    let shown = format!("{sample:?}");
    let shown: String = shown.chars().take(40).collect();
    println!(
        "{label:<22} {CELLS} cells {:>8.1} ms ({:>7.0} ns/cell)  e.g. {shown}",
        best * 1000.0,
        best * 1e9 / f64::from(CELLS)
    );
}

#[test]
#[ignore]
fn byte_text_functions_over_100k_cells() {
    per_cell("LEN (baseline)", "=LEN(A{r})");
    per_cell("LENB", "=LENB(A{r})");
    per_cell("LEFTB(,100)", "=LEFTB(A{r},100)");
    per_cell("MIDB(,51,100)", "=MIDB(A{r},51,100)");
    per_cell("RIGHTB(,101)", "=RIGHTB(A{r},101)");
    per_cell("FINDB(\"World\")", "=FINDB(\"World\",A{r})");
    per_cell("SEARCHB(\"w?rld\")", "=SEARCHB(\"w?rld\",A{r})");
    per_cell("REPLACEB(,11,20,\"x\")", "=REPLACEB(A{r},11,20,\"x\")");
    per_cell("ASC", "=ASC(A{r})");
    per_cell("DBCS", "=DBCS(A{r})");
}

fn timed(s: &SheetEngine, formula: &str) {
    let mut best = f64::INFINITY;
    let mut value = EvaluatedCellValue::Empty;
    for _ in 0..5 {
        let start = Instant::now();
        value = s.evaluate_formula_str(formula);
        best = best.min(start.elapsed().as_secs_f64());
    }
    println!("{formula:<40} {:>8.2} ms  {value:?}", best * 1000.0);
}

#[test]
#[ignore]
fn aggregate_over_large_ranges() {
    let mut s = SheetEngine::new("S");
    for r in 0..CELLS {
        for c in 0..4u32 {
            let x = f64::from(r * 4 + c);
            let raw = if (r + c) % 97 == 0 {
                "=\"t\"".to_string()
            } else {
                ((x * 0.618).sin() * 1000.0).round().to_string()
            };
            s.set_cell(&a1(r, c), raw).unwrap();
        }
    }
    let range = "A1:D100000";
    for formula in [
        format!("=SUM({range})"),
        format!("=AGGREGATE(9,0,{range})"),
        format!("=AGGREGATE(9,4,{range})"),
        format!("=AGGREGATE(1,6,{range})"),
        format!("=AGGREGATE(7,0,{range})"),
        format!("=MEDIAN({range})"),
        format!("=AGGREGATE(12,0,{range})"),
        format!("=AGGREGATE(13,0,{range})"),
        format!("=LARGE({range},10)"),
        format!("=AGGREGATE(14,6,{range},10)"),
        format!("=PERCENTILE.INC({range},0.9)"),
        format!("=AGGREGATE(16,0,{range},0.9)"),
        "=AGGREGATE(9,0,A:D)".to_string(),
    ] {
        timed(&s, &formula);
    }
}

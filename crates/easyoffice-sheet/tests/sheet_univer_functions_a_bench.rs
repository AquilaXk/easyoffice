//! Measurement only (`cargo test -p easyoffice-sheet --release --test
//! sheet_univer_functions_a_bench -- --ignored --nocapture --test-threads=1`): `DSUM` over 100,000
//! records, `MMULT` of two 200 x 200 matrices, and the cost of resolving a function name.

use easyoffice_sheet::{EvaluatedCellValue, PackedCellCoordinate, SheetEngine};
use std::time::Instant;

fn a1(row: u32, col: u32) -> String {
    PackedCellCoordinate::new(row, col).unwrap().to_a1()
}

fn timed<R>(label: &str, f: impl FnOnce() -> R) -> R {
    let start = Instant::now();
    let out = f();
    println!(
        "{label:<48} {:>9.3} ms",
        start.elapsed().as_secs_f64() * 1000.0
    );
    out
}

#[test]
#[ignore]
fn dsum_over_100k_records() {
    const ROWS: u32 = 100_000;
    let mut s = SheetEngine::new("S");
    for c in ["Tree", "Height", "Yield"].iter().enumerate() {
        s.set_cell(&a1(0, c.0 as u32), *c.1).unwrap();
    }
    let trees = ["Maple", "Pear", "Cherry", "Plum"];
    for r in 1..=ROWS {
        s.set_cell(&a1(r, 0), trees[(r % 4) as usize]).unwrap();
        s.set_cell(&a1(r, 1), ((r * 7) % 23).to_string()).unwrap();
        s.set_cell(&a1(r, 2), (r % 10).to_string()).unwrap();
    }
    s.set_cell("E1", "Tree").unwrap();
    s.set_cell("F1", "Height").unwrap();
    s.set_cell("E2", "Maple").unwrap();
    s.set_cell("F2", ">10").unwrap();
    let range = format!("A1:C{}", ROWS + 1);
    for name in ["DSUM", "DAVERAGE", "DCOUNT", "DSTDEV"] {
        let formula = format!("={name}({range},\"Yield\",E1:F2)");
        let v = timed(&format!("{name} {ROWS} records, 2 criteria"), || {
            s.evaluate_formula_str(&formula)
        });
        println!("  -> {v:?}");
    }
    let v = timed("DSUM, 4 alternative criteria rows", || {
        s.set_cell("E3", "Pear").unwrap();
        s.set_cell("E4", "Cherry").unwrap();
        s.set_cell("E5", "Plum").unwrap();
        s.evaluate_formula_str(&format!("=DSUM({range},\"Yield\",E1:F5)"))
    });
    println!("  -> {v:?}");
}

#[test]
#[ignore]
fn mmult_200x200() {
    const N: u32 = 200;
    let mut s = SheetEngine::new("S");
    for r in 0..N {
        for c in 0..N {
            s.set_cell(
                &a1(r, c),
                (((r * 31 + c * 17) % 19) as f64 - 9.0).to_string(),
            )
            .unwrap();
            s.set_cell(
                &a1(r, c + N),
                (((r * 13 + c * 7) % 23) as f64 - 11.0).to_string(),
            )
            .unwrap();
        }
    }
    // a separate, well-conditioned block (determinant near 1e260) for the inversion and determinant
    for r in 0..N {
        for c in 0..N {
            let dominant = if r == c { 20.0 } else { 0.0 };
            s.set_cell(
                &a1(r, c + 2 * N),
                (((r * 29 + c * 11) % 17) as f64 - 8.0 + dominant).to_string(),
            )
            .unwrap();
        }
    }
    let (a, b) = (
        format!("A1:{}", a1(N - 1, N - 1)),
        format!("{}:{}", a1(0, N), a1(N - 1, 2 * N - 1)),
    );
    let d = format!("{}:{}", a1(0, 2 * N), a1(N - 1, 3 * N - 1));
    let v = timed("INDEX(MMULT 200x200, 200, 200)", || {
        s.evaluate_formula_str(&format!("=INDEX(MMULT({a},{b}),200,200)"))
    });
    println!("  -> {v:?}");
    let v = timed("SUM(MMULT 200x200)", || {
        s.evaluate_formula_str(&format!("=SUM(MMULT({a},{b}))"))
    });
    println!("  -> {v:?}");
    let v = timed("MMULT 200x200 top-left (scalar cell)", || {
        s.evaluate_formula_str(&format!("=MMULT({a},{b})"))
    });
    println!("  -> {v:?}");
    let v = timed("MDETERM 200x200", || {
        s.evaluate_formula_str(&format!("=MDETERM({d})"))
    });
    println!("  -> {v:?}");
    let v = timed("INDEX(MINVERSE 200x200, 1, 1)", || {
        s.evaluate_formula_str(&format!("=INDEX(MINVERSE({d}),1,1)"))
    });
    println!("  -> {v:?}");
}

/// 200,000 formula cells that call one cheap function each: the difference between the names is
/// the cost of finding the implementation.
#[test]
#[ignore]
fn function_name_resolution() {
    const CELLS: u32 = 200_000;
    // `ZZZ()`, `ZZZZZZ()` and `ZZZZZZZZZZ()` match no arm: the walk over the whole table
    for name in [
        "PI()",
        "NA()",
        "ISODD(1)",
        "UNICODE(\"a\")",
        "NORMSINV(0.5)",
        "MUNIT(1)",
        "AREAS(A1)",
        "ZZZ()",
        "ZZZZZZ()",
        "ZZZZZZZZZZ()",
    ] {
        let mut s = SheetEngine::new("S");
        for r in 0..CELLS {
            s.set_cell(&a1(r, 3), format!("={name}")).unwrap();
        }
        let best = (0..5)
            .map(|_| {
                let start = Instant::now();
                let all = s.recalculate_all();
                assert_eq!(all.len(), CELLS as usize);
                let unknown = name.starts_with("ZZZ");
                let is_name_error = matches!(
                    all.values().next(),
                    Some(EvaluatedCellValue::Error(
                        easyoffice_sheet::FormulaError::Name
                    ))
                );
                assert_eq!(is_name_error, unknown, "{name}");
                start.elapsed().as_secs_f64()
            })
            .fold(f64::INFINITY, f64::min);
        println!(
            "{name:<16} {CELLS} cells {:>9.3} ms  ({:>6.1} ns/cell)",
            best * 1000.0,
            best * 1e9 / f64::from(CELLS)
        );
    }
}

/// The straightforward port of the reference kernel (rows of vectors, one dot product per cell,
/// the second operand read down a column) on the same 200 x 200 data: the "before" of the flat
/// `i-k-j` kernel. Only the kernel is timed, so it is a lower bound for the formula-level numbers.
#[test]
#[ignore]
fn mmult_200x200_reference_kernel_for_comparison() {
    const N: usize = 200;
    let a: Vec<Vec<f64>> = (0..N)
        .map(|r| {
            (0..N)
                .map(|c| ((r * 31 + c * 17) % 19) as f64 - 9.0)
                .collect()
        })
        .collect();
    let b: Vec<Vec<f64>> = (0..N)
        .map(|r| {
            (0..N)
                .map(|c| ((r * 13 + c * 7) % 23) as f64 - 11.0)
                .collect()
        })
        .collect();
    let best = (0..5)
        .map(|_| {
            let start = Instant::now();
            let out: Vec<Vec<f64>> = a
                .iter()
                .map(|row| {
                    (0..N)
                        .map(|c| {
                            row.iter()
                                .enumerate()
                                .fold(0.0, |sum, (k, x)| sum + x * b[k][c])
                        })
                        .collect()
                })
                .collect();
            assert_eq!(out[N - 1][N - 1], 6.0);
            start.elapsed().as_secs_f64()
        })
        .fold(f64::INFINITY, f64::min);
    println!(
        "reference kernel 200x200 (kernel only)           {:>9.3} ms",
        best * 1000.0
    );
}

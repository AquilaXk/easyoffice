//! Measurement only (`cargo test -p easyoffice-sheet --release --test
//! sheet_univer_functions_b_bench -- --ignored --nocapture --test-threads=1`): 100,000 formula cells
//! of representative bundle B functions (PD-201), the array functions over 100,000 rows, and the cost
//! of resolving a function name.

use easyoffice_sheet::{EvaluatedCellValue, FormulaError, PackedCellCoordinate, SheetEngine};
use std::time::Instant;

const CELLS: u32 = 100_000;

fn a1(row: u32, col: u32) -> String {
    PackedCellCoordinate::new(row, col).unwrap().to_a1()
}

/// Best of three full recalculations of the formula cells in column G (`{r}` is the row number).
fn per_cell(label: &str, template: &str) {
    let mut s = SheetEngine::new("S");
    for r in 0..CELLS {
        let n = r + 1;
        s.set_cell(&a1(r, 0), (39_000 + n % 3_000).to_string())
            .unwrap(); // settlement
        s.set_cell(&a1(r, 1), (43_000 + n % 3_000).to_string())
            .unwrap(); // maturity
        s.set_cell(&a1(r, 2), (0.01 + f64::from(n % 50) / 1000.0).to_string())
            .unwrap();
        s.set_cell(&a1(r, 3), (90 + n % 20).to_string()).unwrap();
        s.set_cell(&a1(r, 4), (n % 40).to_string()).unwrap();
        s.set_cell(&a1(r, 5), "\"3+4i\"".replace('"', "")).unwrap();
        s.set_cell(&a1(r, 6), template.replace("{r}", &n.to_string()))
            .unwrap();
    }
    let mut best = f64::INFINITY;
    let mut sample = EvaluatedCellValue::Empty;
    for _ in 0..3 {
        let start = Instant::now();
        let all = s.recalculate_all();
        best = best.min(start.elapsed().as_secs_f64());
        assert_eq!(
            all.len(),
            CELLS as usize * 7 - CELLS as usize * 6 + CELLS as usize * 6
        );
        sample = all
            .get(&PackedCellCoordinate::new(7, 6).unwrap())
            .cloned()
            .unwrap_or(EvaluatedCellValue::Empty);
    }
    println!(
        "{label:<44} {CELLS} cells {:>9.1} ms ({:>8.0} ns/cell)  e.g. {sample:?}",
        best * 1000.0,
        best * 1e9 / f64::from(CELLS)
    );
}

#[test]
#[ignore]
fn bundle_b_hundred_thousand_cells() {
    per_cell("PRICE", "=PRICE(A{r},B{r},C{r},0.06,100,2,1)");
    per_cell("YIELD", "=YIELD(A{r},B{r},C{r},D{r},100,2,1)");
    per_cell("ACCRINT", "=ACCRINT(A{r}-400,A{r}+100,A{r},C{r},1000,2,1)");
    per_cell("COUPNUM", "=COUPNUM(A{r},B{r},2,1)");
    per_cell("DURATION", "=DURATION(A{r},B{r},C{r},0.06,2,1)");
    per_cell(
        "ODDFPRICE",
        "=ODDFPRICE(DATE(2008,11,11),DATE(2021,3,1),DATE(2008,10,15),DATE(2009,3,1),C{r},0.06,100,2,1)",
    );
    per_cell(
        "ODDFYIELD",
        "=ODDFYIELD(DATE(2008,11,11),DATE(2021,3,1),DATE(2008,10,15),DATE(2009,3,1),C{r},D{r},100,2,1)",
    );
    per_cell("VDB (life 40)", "=VDB(10000,500,40,E{r}/2,E{r}/2+3)");
    per_cell("BESSELJ(x,3)", "=BESSELJ(C{r}*400,3)");
    per_cell("BESSELK(x,5)", "=BESSELK(C{r}*40,5)");
    per_cell("CONVERT km -> mi", "=CONVERT(E{r},\"km\",\"mi\")");
    per_cell("IMSINH", "=IMSINH(F{r})");
    per_cell("IMLOG", "=IMLOG(F{r},2)");
    per_cell("MAXA of 4 cells", "=MAXA(A{r}:D{r})");
    per_cell("STDEVA of 5 cells", "=STDEVA(A{r}:E{r})");
}

#[test]
#[ignore]
fn bundle_b_array_functions_over_100k_rows() {
    let mut s = SheetEngine::new("S");
    for r in 0..CELLS {
        let x = f64::from(r);
        s.set_cell(&a1(r, 0), format!("{}", 3.0 + 0.5 * x + (x * 0.37).sin()))
            .unwrap();
        s.set_cell(&a1(r, 1), x.to_string()).unwrap();
        s.set_cell(&a1(r, 2), format!("{}", x.rem_euclid(97.0) + 1.0))
            .unwrap();
    }
    for r in 0..50 {
        s.set_cell(&a1(r, 4), (r * 2).to_string()).unwrap();
    }
    let last = CELLS;
    for (label, formula) in [
        (
            "LINEST y~x (100k rows)",
            format!("=INDEX(LINEST(A1:A{last},B1:B{last},TRUE,TRUE),1,1)"),
        ),
        (
            "LINEST y~x,x2 (100k rows)",
            format!("=INDEX(LINEST(A1:A{last},B1:C{last}),1,1)"),
        ),
        (
            "LOGEST y~x (100k rows)",
            format!("=INDEX(LOGEST(C1:C{last},B1:B{last}),1,1)"),
        ),
        (
            "TREND (100k rows)",
            format!("=SUM(TREND(A1:A{last},B1:B{last}))"),
        ),
        (
            "FREQUENCY 100k values, 50 bins",
            format!("=SUM(FREQUENCY(C1:C{last},E1:E50))"),
        ),
        (
            "FREQUENCY whole column, 50 bins",
            "=SUM(FREQUENCY(C1:C1048576,E1:E50))".to_string(),
        ),
        (
            "MODE.MULT 100k values",
            format!("=INDEX(MODE.MULT(C1:C{last}),1,1)"),
        ),
        ("STDEVA 100k values", format!("=STDEVA(A1:A{last})")),
        ("MAXA 100k values", format!("=MAXA(A1:A{last})")),
    ] {
        let start = Instant::now();
        let v = s.evaluate_formula_str(&formula);
        println!(
            "{label:<36} {:>9.3} ms  -> {v:?}",
            start.elapsed().as_secs_f64() * 1000.0
        );
        assert!(!matches!(v, EvaluatedCellValue::Error(FormulaError::Name)));
    }
}

/// 200,000 formula cells that call one cheap function each: the difference between the names is the
/// cost of finding the implementation. The names run from the first dispatch arm to the last and
/// past the end (`ZZZ`), and bundle B names are among them.
#[test]
#[ignore]
fn function_name_resolution() {
    const N: u32 = 200_000;
    for name in [
        "ABS(1)",
        "PI()",
        "ISODD(1)",
        "NORMSINV(0.5)",
        "MUNIT(1)",
        "COUPNUM(40000,41000,1)",
        "BESSELJ(1,0)",
        "CONVERT(1,\"m\",\"m\")",
        "IMSINH(1)",
        "MAXA(1)",
        "ENCODEURL(\"a\")",
        "ZZZ()",
        "ZZZZZZ()",
        "ZZZZZZZZZZ()",
    ] {
        let mut s = SheetEngine::new("S");
        for r in 0..N {
            s.set_cell(&a1(r, 3), format!("={name}")).unwrap();
        }
        let best = (0..5)
            .map(|_| {
                let start = Instant::now();
                let all = s.recalculate_all();
                assert_eq!(all.len(), N as usize);
                start.elapsed().as_secs_f64()
            })
            .fold(f64::INFINITY, f64::min);
        println!(
            "{name:<26} {N} cells {:>9.3} ms  ({:>6.1} ns/cell)",
            best * 1000.0,
            best * 1e9 / f64::from(N)
        );
    }
}

/// The counting kernel of `FREQUENCY` alone, 100,000 values and 50 unsorted bins: the reference's
/// scan over the intervals for every value against the sorted edges and one binary search.
#[test]
#[ignore]
fn frequency_kernel_scan_against_binary_search() {
    let values: Vec<f64> = (0..CELLS).map(|r| f64::from(r % 97) + 1.0).collect();
    let bins: Vec<f64> = (0..50).map(|b| f64::from((b * 37) % 100)).collect();
    let mut intervals: Vec<(f64, f64)> = vec![(f64::NEG_INFINITY, 0.0); bins.len() + 1];
    let mut order: Vec<usize> = (0..bins.len()).collect();
    order.sort_by(|&a, &b| bins[a].total_cmp(&bins[b]));
    for (rank, &index) in order.iter().enumerate() {
        let start = if rank == 0 {
            f64::NEG_INFINITY
        } else {
            bins[order[rank - 1]]
        };
        intervals[index] = (start, bins[index]);
    }
    intervals[bins.len()] = (bins[order[order.len() - 1]], f64::INFINITY);
    let mut scan_counts = vec![0u32; intervals.len()];
    let start = Instant::now();
    for _ in 0..20 {
        scan_counts.fill(0);
        for &v in &values {
            let at = intervals
                .iter()
                .position(|&(lo, hi)| v > lo && v <= hi)
                .unwrap();
            scan_counts[at] += 1;
        }
    }
    let scan = start.elapsed().as_secs_f64() / 20.0;
    let mut edges: Vec<(f64, usize)> = bins.iter().copied().zip(0..).collect();
    edges.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut sorted_counts = vec![0u32; edges.len() + 1];
    let start = Instant::now();
    for _ in 0..20 {
        sorted_counts.fill(0);
        for &v in &values {
            let at = edges.partition_point(|&(edge, _)| edge < v);
            sorted_counts[edges.get(at).map_or(edges.len(), |&(_, i)| i)] += 1;
        }
    }
    let search = start.elapsed().as_secs_f64() / 20.0;
    assert_eq!(scan_counts, sorted_counts);
    println!(
        "FREQUENCY kernel, 100k values, 50 bins: interval scan {:.3} ms, binary search {:.3} ms",
        scan * 1000.0,
        search * 1000.0
    );
}

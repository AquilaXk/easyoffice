//! Wall-clock deadline of a recalculation: the repro workbooks (a big range read hundreds of times per
//! formula cell, and a chain of defined names that doubles the reads) and the measurement harness
//! (`cargo test -p easyoffice-sheet --release --lib -- --ignored --nocapture deadline_bench`).

use super::*;
use crate::{SheetEngine, WorkbookEvalTarget};
use easyoffice_formats::DefinedName;
use std::time::{Duration, Instant};

fn at(a1: &str) -> PackedCellCoordinate {
    PackedCellCoordinate::parse_a1(a1).unwrap()
}

/// Rows of the block and terms of a heavy formula in the tests: a cell stays under `MAX_EVAL_STEPS`,
/// so its value is a number and tells a finished cell from a cut-off one.
const ROWS: u32 = 100;
const TERMS: usize = 100;
/// The repro (a 4.2 KB formula reading `A1:Z1000` 300 times) ends the cell itself at
/// `MAX_EVAL_STEPS` with `#VALUE!`; the harness uses it.
const REPRO_ROWS: u32 = 1000;
const REPRO_TERMS: usize = 300;

/// A workbook whose `A1:Z{rows}` holds numbers and whose column `AB` holds `heavy` formulas that
/// each read the whole block `terms` times.
fn heavy_workbook(heavy: u32) -> WorkbookEngine {
    heavy_workbook_of(heavy, TERMS, ROWS)
}

fn heavy_workbook_of(heavy: u32, terms: usize, rows: u32) -> WorkbookEngine {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");
    for r in 1..=rows {
        for c in 0..26u8 {
            let col = (b'A' + c) as char;
            wb.set_cell(
                "Sheet1",
                &format!("{col}{r}"),
                (r + u32::from(c)).to_string(),
            )
            .unwrap();
        }
    }
    let formula = format!("={}0", format!("SUM(A1:Z{rows})+").repeat(terms));
    for r in 1..=heavy {
        wb.set_cell("Sheet1", &format!("AB{r}"), formula.clone())
            .unwrap();
    }
    wb
}

/// `nm_0 = SUM(Sheet1!$A$1:$Z$1000)`, `nm_k = nm_{k-1}+nm_{k-1}`: 2^levels reads of the block per use.
fn doubling_names(levels: u32, rows: u32) -> Vec<DefinedName> {
    let mut defs = vec![DefinedName {
        name: "nm_0".to_string(),
        refers_to: format!("SUM(Sheet1!$A$1:$Z${rows})"),
        ..DefinedName::default()
    }];
    for k in 1..=levels {
        defs.push(DefinedName {
            name: format!("nm_{k}"),
            refers_to: format!("nm_{0}+nm_{0}", k - 1),
            ..DefinedName::default()
        });
    }
    defs
}

/// Heavy formula cells of the repro workbook.
const MANY: u32 = 400;

fn session_with(wb: WorkbookEngine, max_time: Duration) -> RecalcSession {
    let budget = Budget {
        max_time,
        ..Budget::default()
    };
    RecalcSession::with_budget(wb, budget).unwrap()
}

/// How long one heavy cell takes here, so the limits and bounds below follow the machine (a debug
/// build or a slow runner scales both sides).
fn one_cell() -> Duration {
    let mut session = session_with(heavy_workbook(1), MAX_EVAL_TIME);
    let start = Instant::now();
    session.recalculate();
    start.elapsed().max(Duration::from_millis(1))
}

fn value(s: &RecalcSession, a1: &str) -> String {
    s.cell_value("Sheet1", at(a1)).to_display_string()
}

/// The values of every `AB` cell by the stateless path with no practical limit.
fn full_values(wb: &WorkbookEngine, heavy: u32) -> Vec<String> {
    let addrs: Vec<String> = (1..=heavy).map(|r| format!("AB{r}")).collect();
    let targets: Vec<WorkbookEvalTarget<'_>> = addrs
        .iter()
        .map(|a| WorkbookEvalTarget {
            sheet: "Sheet1",
            expr: a,
            anchor: None,
        })
        .collect();
    let batch = wb.evaluate_targets_within(&targets, Duration::from_secs(3600));
    assert!(!batch.timed_out);
    batch.values.iter().map(|v| v.to_display_string()).collect()
}

#[test]
fn a_recalculation_that_would_take_long_stops_at_the_deadline() {
    let one = one_cell();
    let mut session = session_with(heavy_workbook(MANY), one * 8);
    let start = Instant::now();
    let out = session.recalculate();
    let took = start.elapsed();
    assert!(out.timed_out);
    assert!(
        took < one * 100,
        "{took:?} against {one:?} a cell: stopped late"
    );
    assert!(
        out.recomputed > 0 && out.recomputed < MANY as usize,
        "{}",
        out.recomputed
    );
    assert_eq!(out.pending + out.recomputed, MANY as usize);
    assert_eq!(
        out.changed.len(),
        out.recomputed,
        "a cut-off cell is not reported: the caller keeps what it shows"
    );
}

#[test]
fn the_cells_a_stopped_recalculation_finished_hold_their_real_value() {
    let one = one_cell();
    let wb = heavy_workbook(MANY);
    let full = full_values(&heavy_workbook(2), 1)[0].clone();
    let mut session = session_with(wb, one * 8);
    let out = session.recalculate();
    assert!(out.timed_out);
    assert!(!out.changed.is_empty());
    for c in &out.changed {
        assert_eq!(c.value.to_display_string(), full);
    }
}

#[test]
fn the_next_call_carries_on_and_ends_with_the_full_values() {
    let heavy = 24;
    let mut wb = heavy_workbook(heavy);
    wb.set_cell("Sheet1", "AC1", "=AB1+AB2").unwrap();
    let expected = full_values(&wb, heavy);
    let mut session = session_with(wb, Duration::ZERO);
    let first = session.recalculate();
    assert!(first.timed_out);
    assert_eq!((first.recomputed, first.pending), (0, heavy as usize + 1));
    assert!(first.changed.is_empty(), "nothing is shown as an error");
    session.budget.max_time = MAX_EVAL_TIME;
    let second = session.recalculate();
    assert!(!second.timed_out);
    assert_eq!(second.recomputed, heavy as usize + 1, "all that was left");
    assert_eq!(second.changed.len(), heavy as usize + 1);
    let shown: Vec<String> = (1..=heavy)
        .map(|r| value(&session, &format!("AB{r}")))
        .collect();
    assert_eq!(shown, expected);
    assert_eq!(
        value(&session, "AC1"),
        format!("{}", {
            let n: f64 = expected[0].parse().unwrap();
            n * 2.0
        })
    );
    assert_eq!(session.recalculate().recomputed, 0, "nothing is left over");
}

/// Column `B` holds formulas, so every read of the block by a heavy cell goes through the cell
/// evaluator (the deadline can pass inside a cell); the heavy cells turn an aborted read into `-1`.
fn guarded_workbook(heavy: u32) -> WorkbookEngine {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");
    for r in 1..=2000u32 {
        wb.set_cell("Sheet1", &format!("A{r}"), r.to_string())
            .unwrap();
        wb.set_cell("Sheet1", &format!("B{r}"), format!("=A{r}*2"))
            .unwrap();
    }
    let sum = format!("={}0", "SUM(B1:B2000)+".repeat(20));
    for r in 1..=heavy {
        wb.set_cell(
            "Sheet1",
            &format!("C{r}"),
            format!("=IFERROR({},-1)", &sum[1..]),
        )
        .unwrap();
    }
    wb
}

#[test]
fn a_cell_cut_off_in_the_middle_never_keeps_a_value_it_made_up() {
    let heavy = 60;
    let wb = guarded_workbook(heavy);
    let mut full = session_with(wb.clone(), MAX_EVAL_TIME);
    let expected_once = full.recalculate();
    assert!(!expected_once.timed_out);
    let start = Instant::now();
    let mut one = session_with(guarded_workbook(1), MAX_EVAL_TIME);
    one.recalculate();
    let one = start.elapsed();
    let mut session = session_with(wb, one);
    let mut passes = 0;
    while session.recalculate().timed_out {
        passes += 1;
        assert!(passes < 500, "no progress");
    }
    assert!(passes > 0, "the limit never cut a pass");
    for r in 1..=heavy {
        let a1 = format!("C{r}");
        assert_eq!(
            session.cell_value("Sheet1", at(&a1)),
            full.cell_value("Sheet1", at(&a1)),
            "{a1}"
        );
    }
}

#[test]
fn rules_read_the_cut_off_cells_as_the_page_shows_them_and_follow_when_they_are_computed() {
    use easyoffice_formats::{parse_sqref, CfKind, CfRule, CfStyle, Cmp, SheetRules};
    let mut session = session_with(heavy_workbook(6), MAX_EVAL_TIME);
    assert!(!session.recalculate().timed_out);
    session.set_cell("Sheet1", at("A1"), "1000000000").unwrap();
    session.budget.max_time = Duration::ZERO;
    assert!(session.recalculate().timed_out);
    let rules = SheetRules {
        cf: vec![CfRule {
            ranges: parse_sqref("AB1:AB6"),
            priority: 1,
            stop_if_true: false,
            kind: CfKind::CellIs {
                cmp: Cmp::Greater,
                a: "10000000000".to_string(),
                b: String::new(),
            },
            style: CfStyle {
                bg: Some(0xFF_C7_CE),
                fg: None,
                flags: 0,
            },
        }],
        ..SheetRules::default()
    };
    let visuals = session.set_rules("Sheet1", rules).unwrap();
    assert!(
        visuals.is_empty(),
        "the shown values are the old, small ones"
    );
    session.budget.max_time = MAX_EVAL_TIME;
    let out = session.recalculate();
    assert!(!out.timed_out);
    let updates = session.rule_updates(&[], &out);
    let marked: usize = updates.changed.iter().map(|(_, v)| v.len()).sum();
    assert_eq!(
        marked, 6,
        "the rule follows once the cells have their new values"
    );
}

#[test]
fn an_edit_after_a_stopped_pass_is_computed_in_full() {
    let mut wb = heavy_workbook(8);
    wb.set_cell("Sheet1", "AC1", "=AB1+1").unwrap();
    let mut session = session_with(wb, Duration::ZERO);
    assert!(session.recalculate().timed_out);
    session.budget.max_time = MAX_EVAL_TIME;
    session.set_cell("Sheet1", at("A1"), "1000").unwrap();
    let out = session.recalculate();
    assert!(!out.timed_out);
    let mut check = heavy_workbook(8);
    check.set_cell("Sheet1", "A1", "1000").unwrap();
    check.set_cell("Sheet1", "AC1", "=AB1+1").unwrap();
    let expected = full_values(&check, 8);
    for (i, want) in expected.iter().enumerate() {
        assert_eq!(&value(&session, &format!("AB{}", i + 1)), want);
    }
}

#[test]
fn a_constant_formula_edited_during_a_stopped_pass_still_reads_its_value() {
    let mut session = session_with(heavy_workbook(4), Duration::ZERO);
    session.set_cell("Sheet1", at("AD1"), "=5").unwrap();
    let out = session.recalculate();
    assert!(out.timed_out);
    let five = out.changed.iter().find(|c| c.coord == at("AD1")).unwrap();
    assert_eq!(five.value, EvaluatedCellValue::Number(5.0));
}

#[test]
fn within_the_deadline_the_values_equal_the_unlimited_pass() {
    let mut wb = heavy_workbook(3);
    for r in 1..=50u32 {
        wb.set_cell("Sheet1", &format!("AD{r}"), format!("=A{r}*2+AD{}", r - 1))
            .unwrap();
    }
    let expected = full_values(&wb, 3);
    let mut session = session_with(wb, MAX_EVAL_TIME);
    let out = session.recalculate();
    assert!(!out.timed_out);
    assert_eq!(out.recomputed, 53);
    let shown: Vec<String> = (1..=3)
        .map(|r| value(&session, &format!("AB{r}")))
        .collect();
    assert_eq!(shown, expected);
}

#[test]
fn a_chain_of_names_that_doubles_the_reads_stops_at_the_deadline() {
    let one = one_cell();
    let mut wb = heavy_workbook(0);
    wb.set_names(&doubling_names(10, ROWS)).unwrap();
    for r in 1..=MANY {
        wb.set_cell("Sheet1", &format!("AB{r}"), "=nm_10").unwrap();
    }
    let mut session = session_with(wb, one * 8);
    let start = Instant::now();
    let out = session.recalculate();
    assert!(out.timed_out);
    assert!(start.elapsed() < one * 100);
    assert!(out.recomputed < MANY as usize);
    assert!(out.changed.len() == out.recomputed);
}

#[test]
fn the_stateless_pass_stops_and_keeps_what_it_finished() {
    let one = one_cell();
    let wb = heavy_workbook(MANY);
    let mut addrs: Vec<String> = (1..=MANY).map(|r| format!("AB{r}")).collect();
    addrs.insert(0, "A1".to_string());
    let targets: Vec<WorkbookEvalTarget<'_>> = addrs
        .iter()
        .map(|a| WorkbookEvalTarget {
            sheet: "Sheet1",
            expr: a,
            anchor: None,
        })
        .collect();
    let start = Instant::now();
    let batch = wb.evaluate_targets_within(&targets, one * 8);
    assert!(batch.timed_out);
    assert!(start.elapsed() < one * 100);
    assert_eq!(
        batch.values[0].to_display_string(),
        "1",
        "a plain cell reads as itself"
    );
    assert!(!batch.is_unreached(0));
    let done = (1..=MANY as usize)
        .filter(|&i| !batch.is_unreached(i))
        .count();
    assert!(done > 0 && done < MANY as usize, "{done}");
    assert!(batch.values[1..]
        .iter()
        .enumerate()
        .all(|(i, v)| batch.is_unreached(i + 1) || v.to_display_string() != "#VALUE!"));
}

#[test]
fn a_target_after_the_deadline_has_no_value_unless_it_is_a_plain_cell() {
    let wb = heavy_workbook(2);
    let targets = [
        WorkbookEvalTarget {
            sheet: "Sheet1",
            expr: "=SUM(A1:A3)",
            anchor: None,
        },
        WorkbookEvalTarget {
            sheet: "Sheet1",
            expr: "B2",
            anchor: None,
        },
        WorkbookEvalTarget {
            sheet: "Sheet1",
            expr: "AB1",
            anchor: None,
        },
    ];
    let batch = wb.evaluate_targets_within(&targets, Duration::ZERO);
    assert!(batch.timed_out);
    let shown: Vec<String> = batch.values.iter().map(|v| v.to_display_string()).collect();
    assert_eq!(shown, ["", "3", ""]);
    assert_eq!(
        (0..3).map(|i| batch.is_unreached(i)).collect::<Vec<_>>(),
        [true, false, true]
    );
}

#[test]
fn a_stopped_full_recalculation_leaves_out_what_it_did_not_reach_and_no_plan_behind() {
    let mut sheet = SheetEngine::new("S");
    sheet.set_cell("A1", "2").unwrap();
    sheet.set_cell("B1", "=A1*3").unwrap();
    sheet.set_cell("C1", "=B1+1").unwrap();
    let (cut, timed_out) = sheet.recalculate_all_within(Some(Duration::ZERO));
    assert!(timed_out);
    assert_eq!(cut.get(&at("A1")), Some(&EvaluatedCellValue::Number(2.0)));
    assert!(!cut.contains_key(&at("B1")) && !cut.contains_key(&at("C1")));
    let all = sheet.recalculate_all();
    assert_eq!(all[&at("C1")], EvaluatedCellValue::Number(7.0));
    let (whole, timed_out) = sheet.recalculate_all_within(Some(MAX_EVAL_TIME));
    assert!(!timed_out);
    assert_eq!(whole[&at("B1")], EvaluatedCellValue::Number(6.0));
}

#[test]
fn a_cut_off_formula_keeps_the_value_it_had_and_is_reported_when_it_changes() {
    let wb = heavy_workbook(5);
    let mut session = session_with(wb, MAX_EVAL_TIME);
    assert!(!session.recalculate().timed_out);
    let before = value(&session, "AB1");
    session.set_cell("Sheet1", at("A1"), "100000").unwrap();
    session.budget.max_time = Duration::ZERO;
    let cut = session.recalculate();
    assert!(cut.timed_out);
    assert_eq!((cut.changed.len(), cut.pending), (0, 5));
    assert_eq!(value(&session, "AB1"), before, "the shown value stays");
    session.budget.max_time = MAX_EVAL_TIME;
    let done = session.recalculate();
    assert!(!done.timed_out);
    assert_eq!(done.changed.len(), 5, "reported once it has its new value");
    assert_ne!(value(&session, "AB1"), before);
}

#[test]
fn a_circular_workbook_still_finishes_in_several_short_passes() {
    let one = one_cell();
    let mut wb = guarded_workbook(60);
    wb.set_cell("Sheet1", "E1", "=F1+1").unwrap();
    wb.set_cell("Sheet1", "F1", "=E1+1").unwrap();
    let mut session = session_with(wb, one * 2);
    let mut passes = 0;
    while session.recalculate().timed_out {
        passes += 1;
        assert!(passes < 500, "no progress");
    }
    assert!(passes > 0);
    assert!(session.recalculate().circular);
}

#[test]
#[ignore = "measurement harness"]
fn deadline_bench() {
    for heavy in [10u32, 100] {
        let mut session =
            RecalcSession::new(heavy_workbook_of(heavy, REPRO_TERMS, REPRO_ROWS)).unwrap();
        let t = Instant::now();
        let out = session.recalculate();
        println!(
            "heavy cells {heavy}: first recalc {:.0} ms, recomputed {}",
            t.elapsed().as_secs_f64() * 1e3,
            out.recomputed
        );
    }
    let wb = heavy_workbook_of(100, REPRO_TERMS, REPRO_ROWS);
    let addrs: Vec<String> = (1..=100).map(|r| format!("AB{r}")).collect();
    let targets: Vec<crate::WorkbookEvalTarget<'_>> = addrs
        .iter()
        .map(|a| crate::WorkbookEvalTarget {
            sheet: "Sheet1",
            expr: a,
            anchor: None,
        })
        .collect();
    let t = Instant::now();
    let batch = wb.evaluate_targets(&targets);
    println!(
        "stateless 100 heavy cells: {:.0} ms, {} values",
        t.elapsed().as_secs_f64() * 1e3,
        batch.values.len()
    );
    let mut wb = heavy_workbook_of(0, REPRO_TERMS, REPRO_ROWS);
    wb.set_names(&doubling_names(10, REPRO_ROWS)).unwrap();
    for r in 1..=20u32 {
        wb.set_cell("Sheet1", &format!("AB{r}"), "=nm_10").unwrap();
    }
    let mut session = RecalcSession::new(wb).unwrap();
    let t = Instant::now();
    let out = session.recalculate();
    println!(
        "name chain, 20 uses: first recalc {:.0} ms, recomputed {}",
        t.elapsed().as_secs_f64() * 1e3,
        out.recomputed
    );
}

/// The repro at the size of the report (`cargo test -p easyoffice-sheet --release --lib --
/// --ignored --nocapture deadline_repro_bench`): what the stateless pass (`BENCH_HEAVY` formula
/// cells, 10,000 by default) and a session recalculation (`BENCH_SESSION` cells, 4,000 by default,
/// the most 4.2 KB formulas a session's memory budget takes) cost, with `BENCH_NAMES` (2,000)
/// cells of a name chain, and whether they stopped.
#[test]
#[ignore = "measurement harness"]
fn deadline_repro_bench() {
    let env = |key: &str, default: u32| {
        std::env::var(key)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let (heavy, in_session) = (env("BENCH_HEAVY", 10_000), env("BENCH_SESSION", 4_000));
    let named = env("BENCH_NAMES", 2_000);
    if heavy > 0 {
        let wb = heavy_workbook_of(heavy, REPRO_TERMS, REPRO_ROWS);
        let addrs: Vec<String> = (1..=heavy).map(|r| format!("AB{r}")).collect();
        let targets: Vec<crate::WorkbookEvalTarget<'_>> = addrs
            .iter()
            .map(|a| crate::WorkbookEvalTarget {
                sheet: "Sheet1",
                expr: a,
                anchor: None,
            })
            .collect();
        let t = Instant::now();
        let batch = wb.evaluate_targets(&targets);
        let timed = batch.timed_out;
        println!(
            "stateless, {heavy} repro cells: {:.0} ms, timed out {timed}",
            t.elapsed().as_secs_f64() * 1e3
        );
    }
    let wb = heavy_workbook_of(in_session, REPRO_TERMS, REPRO_ROWS);
    let mut session = RecalcSession::new(wb).unwrap();
    let t = Instant::now();
    let out = session.recalculate();
    let timed = out.timed_out;
    println!(
        "session, {in_session} repro cells: {:.0} ms, recomputed {}, timed out {timed}",
        t.elapsed().as_secs_f64() * 1e3,
        out.recomputed
    );
    let mut passes = 1;
    while session.recalculate().timed_out {
        passes += 1;
    }
    println!(
        "  carried on until nothing was pending: {passes} passes, {:.0} ms in all",
        t.elapsed().as_secs_f64() * 1e3
    );
    let mut wb = heavy_workbook_of(0, REPRO_TERMS, REPRO_ROWS);
    wb.set_names(&doubling_names(10, REPRO_ROWS)).unwrap();
    for r in 1..=named {
        wb.set_cell("Sheet1", &format!("AB{r}"), "=nm_10").unwrap();
    }
    let mut session = RecalcSession::new(wb).unwrap();
    let t = Instant::now();
    let out = session.recalculate();
    let timed = out.timed_out;
    println!(
        "session, {named} cells of a 10-level name chain: {:.0} ms, recomputed {}, timed out {timed}",
        t.elapsed().as_secs_f64() * 1e3,
        out.recomputed
    );
}

/// The sheet chain of the save-time tests (100 sheets of 400 rows, `S{k}!A{r} = S{k-1}!A{r}+1`, 39,600
/// formulas) through the stateless pass the save runs: the cost of the deadline checks on a document
/// that is nowhere near the limit.
#[test]
#[ignore = "measurement harness"]
fn chain_bench() {
    let mut wb = WorkbookEngine::new();
    for k in 0..100u32 {
        wb.add_sheet(format!("S{k}"));
        for r in 1..=400u32 {
            let raw = if k == 0 {
                r.to_string()
            } else {
                format!("=S{}!A{r}+1", k - 1)
            };
            wb.set_cell(&format!("S{k}"), &format!("A{r}"), raw)
                .unwrap();
        }
    }
    let names: Vec<String> = (1..100u32).map(|k| format!("S{k}")).collect();
    let addrs: Vec<String> = (1..=400u32).map(|r| format!("A{r}")).collect();
    let targets: Vec<crate::WorkbookEvalTarget<'_>> = names
        .iter()
        .flat_map(|n| {
            addrs.iter().map(move |a| crate::WorkbookEvalTarget {
                sheet: n,
                expr: a,
                anchor: None,
            })
        })
        .collect();
    let best = (0..5)
        .map(|_| {
            let t = Instant::now();
            let batch = wb.evaluate_targets(&targets);
            assert_eq!(batch.values.len(), targets.len());
            t.elapsed().as_secs_f64() * 1e3
        })
        .fold(f64::INFINITY, f64::min);
    println!("chain, {} formulas: best of 5 {best:.1} ms", targets.len());
}

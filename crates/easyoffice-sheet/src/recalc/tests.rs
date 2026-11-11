use super::*;
use crate::dag::{CellIndex, FormulaIndex};
use crate::{FastMap, WorkbookEvalTarget};
use std::collections::HashMap;

fn at(a1: &str) -> PackedCellCoordinate {
    PackedCellCoordinate::parse_a1(a1).unwrap()
}

fn workbook(sheets: &[&str], cells: &[(&str, &str, &str)]) -> WorkbookEngine {
    let mut wb = WorkbookEngine::new();
    for name in sheets {
        wb.add_sheet(*name);
    }
    for &(sheet, a1, raw) in cells {
        wb.set_cell(sheet, a1, raw).unwrap();
    }
    wb
}

fn session(sheets: &[&str], cells: &[(&str, &str, &str)]) -> RecalcSession {
    let mut s = RecalcSession::new(workbook(sheets, cells)).unwrap();
    s.recalculate();
    s
}

fn num(n: f64) -> EvaluatedCellValue {
    EvaluatedCellValue::Number(n)
}

fn changed_map(out: &RecalcOutcome) -> HashMap<(usize, String), EvaluatedCellValue> {
    out.changed
        .iter()
        .map(|c| ((c.sheet, c.coord.to_a1()), c.value.clone()))
        .collect()
}

/// Formula cells (`=` prefix) of the workbook, sorted by sheet then row-major.
fn formula_cells(wb: &WorkbookEngine) -> Vec<(usize, PackedCellCoordinate)> {
    let mut out = Vec::new();
    for (idx, sheet) in wb.sheets().iter().enumerate() {
        for (&coord, raw) in sheet.raw_cells() {
            if raw.trim_start().starts_with('=') {
                out.push((idx, coord));
            }
        }
    }
    out.sort_unstable();
    out
}

/// Full recalculation of a copy of the workbook: the oracle every incremental result must equal.
fn full_values(
    wb: &WorkbookEngine,
) -> (
    HashMap<(usize, PackedCellCoordinate), EvaluatedCellValue>,
    bool,
) {
    let cells = formula_cells(wb);
    let addrs: Vec<String> = cells.iter().map(|&(_, c)| c.to_a1()).collect();
    let targets: Vec<WorkbookEvalTarget<'_>> = cells
        .iter()
        .zip(&addrs)
        .map(|(&(idx, _), a1)| WorkbookEvalTarget {
            sheet: wb.sheets()[idx].name.as_str(),
            expr: a1.as_str(),
            anchor: None,
        })
        .collect();
    let batch = wb.clone().evaluate_targets(&targets);
    (
        cells.into_iter().zip(batch.values).collect(),
        batch.circular,
    )
}

fn assert_matches_full(s: &RecalcSession, context: &str) {
    let (want, circular) = full_values(s.workbook());
    for (&(idx, coord), value) in &want {
        let name = s.sheet_name(idx).unwrap();
        assert_eq!(
            &s.cell_value(name, coord),
            value,
            "{context}: {name}!{}",
            coord.to_a1()
        );
    }
    let _ = circular;
}

#[test]
fn an_edit_recomputes_only_the_cells_that_read_it() {
    let mut s = session(
        &["S"],
        &[
            ("S", "A1", "1"),
            ("S", "B1", "=A1+1"),
            ("S", "C1", "=B1*2"),
            ("S", "A2", "10"),
            ("S", "B2", "=A2+1"),
        ],
    );
    assert_eq!(s.cell_value("S", at("C1")), num(4.0));
    s.set_cell("S", at("A1"), "5").unwrap();
    let out = s.recalculate();
    assert_eq!(out.recomputed, 2, "B1 and C1 only");
    let changed = changed_map(&out);
    assert_eq!(changed[&(0, "B1".into())], num(6.0));
    assert_eq!(changed[&(0, "C1".into())], num(12.0));
    assert_eq!(s.cell_value("S", at("B2")), num(11.0));
    assert!(!out.circular);
}

#[test]
fn an_unchanged_edit_costs_nothing_and_a_new_value_is_reported_once() {
    let mut s = session(&["S"], &[("S", "A1", "1"), ("S", "B1", "=A1+1")]);
    s.set_cell("S", at("A1"), "1").unwrap();
    let out = s.recalculate();
    assert_eq!((out.recomputed, out.changed.len()), (0, 0));
    s.set_cell("S", at("A1"), "2").unwrap();
    assert_eq!(s.recalculate().changed.len(), 1);
    assert_eq!(
        s.recalculate().recomputed,
        0,
        "nothing staged, nothing dirty"
    );
}

#[test]
fn range_dependents_follow_literal_and_empty_cells_inside_the_range() {
    let mut s = session(
        &["S"],
        &[
            ("S", "A1", "1"),
            ("S", "A3", "3"),
            ("S", "B1", "=SUM(A1:A100)"),
        ],
    );
    assert_eq!(s.cell_value("S", at("B1")), num(4.0));
    s.set_cell("S", at("A50"), "10").unwrap();
    s.recalculate();
    assert_eq!(s.cell_value("S", at("B1")), num(14.0));
    s.set_cell("S", at("A101"), "99").unwrap();
    assert_eq!(s.recalculate().recomputed, 0, "outside the range");
    s.set_cell("S", at("A1"), "").unwrap();
    s.recalculate();
    assert_eq!(s.cell_value("S", at("B1")), num(13.0));
}

#[test]
fn whole_column_and_wide_ranges_are_indexed_without_expansion() {
    let mut s = session(
        &["S"],
        &[
            ("S", "A1", "1"),
            ("S", "C5", "=SUM(A:A)"),
            ("S", "C6", "=SUM(A1:Z1)"),
            ("S", "C7", "=SUM(1:1)"),
        ],
    );
    s.set_cell("S", at("A9000"), "5").unwrap();
    let out = s.recalculate();
    assert_eq!(out.recomputed, 1, "only the SUM over column A");
    assert_eq!(s.cell_value("S", at("C5")), num(6.0));
    s.set_cell("S", at("Q1"), "7").unwrap();
    s.recalculate();
    assert_eq!(s.cell_value("S", at("C6")), num(8.0));
    assert_eq!(s.cell_value("S", at("C7")), num(8.0));
}

#[test]
fn cross_sheet_references_and_ranges_propagate() {
    let mut s = session(
        &["One", "Two"],
        &[
            ("Two", "A1", "2"),
            ("Two", "A2", "3"),
            ("One", "A1", "=Two!A1*10"),
            ("One", "A2", "=SUM(Two!A1:A5)"),
            ("Two", "B1", "=One!A1+1"),
        ],
    );
    assert_eq!(s.cell_value("Two", at("B1")), num(21.0));
    s.set_cell("Two", at("A3"), "5").unwrap();
    s.recalculate();
    assert_eq!(s.cell_value("One", at("A2")), num(10.0));
    s.set_cell("Two", at("A1"), "4").unwrap();
    s.recalculate();
    assert_eq!(s.cell_value("One", at("A1")), num(40.0));
    assert_eq!(s.cell_value("Two", at("B1")), num(41.0));
    assert_matches_full(&s, "cross sheet");
}

#[test]
fn a_cycle_is_reported_and_clears_when_it_is_broken() {
    let mut s = session(&["S"], &[("S", "A1", "=B1+1"), ("S", "B1", "2")]);
    s.set_cell("S", at("B1"), "=A1+1").unwrap();
    let out = s.recalculate();
    assert!(out.circular);
    assert_eq!(
        s.cell_value("S", at("A1")),
        EvaluatedCellValue::Error(FormulaError::Cycle)
    );
    s.set_cell("S", at("C9"), "1").unwrap();
    assert!(s.recalculate().circular, "an unrelated edit keeps the flag");
    s.set_cell("S", at("B1"), "7").unwrap();
    let out = s.recalculate();
    assert!(!out.circular);
    assert_eq!(s.cell_value("S", at("A1")), num(8.0));
}

#[test]
fn volatile_formulas_are_recomputed_on_every_pass_and_nothing_else() {
    let mut s = session(
        &["S"],
        &[
            ("S", "A1", "1"),
            ("S", "B1", "=A1+1"),
            ("S", "C1", "=RAND()"),
            ("S", "D1", "=C1*0+5"),
        ],
    );
    let first = s.cell_value("S", at("C1"));
    let out = s.recalculate();
    assert_eq!(out.recomputed, 2, "RAND and its dependent");
    assert_ne!(s.cell_value("S", at("C1")), first);
    assert!(out.changed.iter().any(|c| c.coord == at("C1")));
    assert_eq!(s.cell_value("S", at("B1")), num(2.0));
}

#[test]
fn offset_and_indirect_read_current_values() {
    let mut s = session(
        &["S"],
        &[
            ("S", "A1", "4"),
            ("S", "A2", "=A1*2"),
            ("S", "B1", "=INDIRECT(\"A2\")+1"),
            ("S", "B2", "=OFFSET(A1,1,0)+1"),
        ],
    );
    assert_eq!(s.cell_value("S", at("B1")), num(9.0));
    s.set_cell("S", at("A1"), "10").unwrap();
    s.recalculate();
    assert_eq!(s.cell_value("S", at("B1")), num(21.0));
    assert_eq!(s.cell_value("S", at("B2")), num(21.0));
}

#[test]
fn constant_formula_text_is_reported_when_edited() {
    let mut s = session(&["S"], &[]);
    s.set_cell("S", at("A1"), "=5").unwrap();
    s.set_cell("S", at("A2"), "=1+").unwrap();
    s.set_cell("S", at("A3"), "plain").unwrap();
    let changed = changed_map(&s.recalculate());
    assert_eq!(changed[&(0, "A1".into())], num(5.0));
    assert!(matches!(
        changed[&(0, "A2".into())],
        EvaluatedCellValue::Error(_)
    ));
    assert!(!changed.contains_key(&(0, "A3".into())));
}

#[test]
fn a_formula_replaced_by_a_literal_leaves_no_stale_value_or_edge() {
    let mut s = session(
        &["S"],
        &[
            ("S", "A1", "=1+1"),
            ("S", "B1", "=A1+1"),
            ("S", "C1", "=SUM(A1:A9)"),
        ],
    );
    s.set_cell("S", at("A1"), "7").unwrap();
    s.recalculate();
    assert_eq!(s.cell_value("S", at("B1")), num(8.0));
    assert_eq!(s.cell_value("S", at("C1")), num(7.0));
    assert_eq!(s.cell_value("S", at("A1")), num(7.0));
    s.set_cell("S", at("B1"), "").unwrap();
    s.set_cell("S", at("C1"), "").unwrap();
    assert_eq!(s.graph_size(), (0, 0), "no vertices and no range entries");
    s.set_cell("S", at("A1"), "8").unwrap();
    assert_eq!(s.recalculate().recomputed, 0);
}

#[test]
fn shifting_rows_and_rewriting_references_matches_a_full_recalculation() {
    // Insert a row at 3: rows below move down by one and their references are rewritten.
    let mut cells = vec![];
    for r in 1..=6 {
        cells.push(("S".to_string(), format!("A{r}"), r.to_string()));
        cells.push(("S".to_string(), format!("B{r}"), format!("=A{r}*2")));
    }
    cells.push(("S".to_string(), "C1".into(), "=SUM(B1:B6)".into()));
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("S");
    for (s, a, v) in &cells {
        wb.set_cell(s, a, v).unwrap();
    }
    let mut session = RecalcSession::new(wb).unwrap();
    session.recalculate();
    let mut changes = Vec::new();
    for r in (3..=6).rev() {
        changes.push(CellChange {
            sheet: "S".into(),
            coord: at(&format!("A{}", r + 1)),
            raw: r.to_string(),
        });
        changes.push(CellChange {
            sheet: "S".into(),
            coord: at(&format!("B{}", r + 1)),
            raw: format!("=A{}*2", r + 1),
        });
    }
    changes.push(CellChange {
        sheet: "S".into(),
        coord: at("A3"),
        raw: "100".into(),
    });
    changes.push(CellChange {
        sheet: "S".into(),
        coord: at("B3"),
        raw: "=A3*2".into(),
    });
    changes.push(CellChange {
        sheet: "S".into(),
        coord: at("C1"),
        raw: "=SUM(B1:B7)".into(),
    });
    session.apply(&changes).unwrap();
    session.recalculate();
    assert_eq!(
        session.cell_value("S", at("C1")),
        num(2.0 * (1.0 + 2.0 + 100.0 + 3.0 + 4.0 + 5.0 + 6.0))
    );
    assert_matches_full(&session, "row insert");
    // Delete the inserted row again.
    let mut back = Vec::new();
    for r in 3..=6 {
        back.push(CellChange {
            sheet: "S".into(),
            coord: at(&format!("A{r}")),
            raw: r.to_string(),
        });
        back.push(CellChange {
            sheet: "S".into(),
            coord: at(&format!("B{r}")),
            raw: format!("=A{r}*2"),
        });
    }
    for col in ["A", "B"] {
        back.push(CellChange {
            sheet: "S".into(),
            coord: at(&format!("{col}7")),
            raw: String::new(),
        });
    }
    back.push(CellChange {
        sheet: "S".into(),
        coord: at("C1"),
        raw: "=SUM(B1:B6)".into(),
    });
    session.apply(&back).unwrap();
    session.recalculate();
    assert_eq!(session.cell_value("S", at("C1")), num(42.0));
    assert_matches_full(&session, "row delete");
}

#[test]
fn the_footprint_counts_cells_text_and_edges_through_edits() {
    let mut s = session(&["S"], &[("S", "A1", "12"), ("S", "B1", "=A1+1")]);
    let f = s.footprint();
    assert_eq!((f.cells, f.raw_bytes, f.edges), (2, 2 + 5, 1));
    assert!(f.graph_bytes > 0 && f.ast_bytes == 5 * AST_BYTES_PER_FORMULA_BYTE);
    assert_eq!(f.total_bytes(), f.raw_bytes + f.graph_bytes + f.ast_bytes);
    s.set_cell("S", at("A1"), "1234").unwrap();
    s.set_cell("S", at("C1"), "abc").unwrap();
    let f = s.footprint();
    assert_eq!((f.cells, f.raw_bytes, f.edges), (3, 4 + 5 + 3, 1));
    s.set_cell("S", at("B1"), "").unwrap();
    let f = s.footprint();
    assert_eq!((f.cells, f.raw_bytes, f.edges), (2, 4 + 3, 0));
    assert_eq!(
        (f.graph_bytes, f.ast_bytes),
        (0, 0),
        "an emptied graph holds nothing"
    );
}

/// `=Z{b*40+1}+...+Z{b*40+40}` for the 33 formulas of bundle `b`: every read cell has 33
/// dependents (enough to leave any small-set representation) and every bundle reads other cells.
pub(super) fn bundled_formula(i: u32) -> String {
    let first = i / 33 * 40;
    let terms: Vec<String> = (1..=40).map(|k| format!("Z{}", first + k)).collect();
    format!("={}", terms.join("+"))
}

#[test]
fn crowded_dependents_cost_memory_by_edges_not_by_vertex_ids() {
    const FORMULAS: u32 = 30_000;
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("S");
    for i in 0..FORMULAS {
        wb.set_cell("S", &format!("A{}", i + 1), bundled_formula(i))
            .unwrap();
    }
    let mut s = RecalcSession::new(wb).unwrap();
    s.recalculate();
    let f = s.footprint();
    assert_eq!(f.edges, FORMULAS as usize * 40);
    // A bit set per read cell sized by the highest vertex id was 296 MB here; edges cost 4 bytes.
    assert!(f.graph_bytes < 64 << 20, "{} bytes", f.graph_bytes);
    s.set_cell("S", at("Z7"), "5").unwrap();
    assert_eq!(
        s.recalculate().recomputed,
        33,
        "only the bundle that reads Z7"
    );
}

#[test]
fn a_graph_over_budget_is_refused_and_stays_refused() {
    let budget = Budget {
        max_edges: 100,
        max_graph_bytes: usize::MAX,
        max_bytes: usize::MAX,
    };
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("S");
    let mut s = RecalcSession::with_budget(wb, budget).unwrap();
    // 40 edges each: the third formula passes 100.
    s.set_cell("S", at("A1"), &bundled_formula(0)).unwrap();
    s.set_cell("S", at("A2"), &bundled_formula(40)).unwrap();
    assert!(matches!(
        s.set_cell("S", at("A3"), &bundled_formula(80)),
        Err(RecalcError::OverBudget)
    ));
    assert!(matches!(
        s.set_cell("S", at("B1"), "1"),
        Err(RecalcError::OverBudget)
    ));
    let change = CellChange {
        sheet: "S".into(),
        coord: at("B2"),
        raw: "2".into(),
    };
    assert!(matches!(s.apply(&[change]), Err(RecalcError::OverBudget)));

    let mut big = WorkbookEngine::new();
    big.add_sheet("S");
    for i in 0..3 {
        big.set_cell("S", &format!("A{}", i + 1), bundled_formula(i * 33))
            .unwrap();
    }
    assert!(matches!(
        RecalcSession::with_budget(big, budget),
        Err(RecalcError::OverBudget)
    ));

    let total_only = Budget {
        max_edges: usize::MAX,
        max_graph_bytes: usize::MAX,
        max_bytes: 5_000,
    };
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("S");
    let mut s = RecalcSession::with_budget(wb, total_only).unwrap();
    s.set_cell("S", at("A1"), &"x".repeat(4_000)).unwrap();
    assert!(
        matches!(
            s.set_cell("S", at("A2"), &"y".repeat(2_000)),
            Err(RecalcError::OverBudget)
        ),
        "text counts toward max_bytes"
    );

    let bytes_only = Budget {
        max_edges: usize::MAX,
        max_graph_bytes: 1_000,
        max_bytes: usize::MAX,
    };
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("S");
    let mut s = RecalcSession::with_budget(wb, bytes_only).unwrap();
    assert!(matches!(
        s.set_cell("S", at("A1"), &bundled_formula(0)),
        Err(RecalcError::OverBudget)
    ));
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const SHEETS: [&str; 3] = ["Sheet1", "Sheet2", "Sheet3"];

fn rand_cell(rng: &mut Rng) -> String {
    format!(
        "{}{}",
        (b'A' + rng.below(5) as u8) as char,
        rng.below(7) + 1
    )
}

fn rand_raw(rng: &mut Rng) -> String {
    let sheet = SHEETS[rng.below(3) as usize];
    let (a, b, c) = (rand_cell(rng), rand_cell(rng), rand_cell(rng));
    match rng.below(16) {
        0..=3 => rng.below(20).to_string(),
        4 => format!("t{}", rng.below(5)),
        5 => String::new(),
        6 => format!("={a}+{b}"),
        7 => format!("=SUM({a}:{b})"),
        8 => format!("=SUM({sheet}!{a}:{b})"),
        9 => format!("={sheet}!{a}*2"),
        10 => format!("=IF({a}>5,{b},{c})"),
        11 => format!("=COUNT({a}:{b})+1"),
        12 => format!("=MAX({a},{sheet}!{b})"),
        13 => format!("={a}&\"x\""),
        14 => format!("=SUM({}:{})", &a[..1], &a[..1]),
        _ => format!("=AVERAGE({a}:{b})"),
    }
}

/// A reference a formula at `(sheet, row)` may read without ever closing a cycle: any cell of an
/// earlier sheet, or a cell above it on its own sheet. `None` when there is nothing to point at.
fn rand_dag_ref(rng: &mut Rng, sheet: usize, row: u64) -> Option<(usize, String)> {
    let same_sheet_rows = row - 1;
    let earlier_sheets = sheet as u64;
    if same_sheet_rows == 0 && earlier_sheets == 0 {
        return None;
    }
    let col = (b'A' + rng.below(5) as u8) as char;
    if earlier_sheets > 0 && (same_sheet_rows == 0 || rng.below(3) == 0) {
        Some((
            rng.below(earlier_sheets) as usize,
            format!("{col}{}", rng.below(7) + 1),
        ))
    } else {
        Some((sheet, format!("{col}{}", rng.below(same_sheet_rows) + 1)))
    }
}

fn rand_dag_change(rng: &mut Rng) -> CellChange {
    let sheet = rng.below(3) as usize;
    let row = rng.below(7) + 1;
    let col = (b'A' + rng.below(5) as u8) as char;
    let refs = (rand_dag_ref(rng, sheet, row), rand_dag_ref(rng, sheet, row));
    let q = |r: &(usize, String)| format!("{}!{}", SHEETS[r.0], r.1);
    let raw = match (refs, rng.below(9)) {
        (_, 0..=2) => rng.below(20).to_string(),
        (_, 3) => String::new(),
        ((Some(a), Some(b)), 4) => format!("={}+{}", q(&a), q(&b)),
        ((Some(a), Some(b)), 5) if a.0 == b.0 => {
            format!("=SUM({}!{}:{})", SHEETS[a.0], a.1, b.1)
        }
        ((Some(a), _), 6) => format!("={}*2", q(&a)),
        ((Some(a), Some(b)), 7) => format!("=IF({}>5,{},1)", q(&a), q(&b)),
        ((Some(a), Some(b)), 8) if a.0 == b.0 => {
            format!("=COUNT({}!{}:{})", SHEETS[a.0], a.1, b.1)
        }
        _ => rng.below(20).to_string(),
    };
    CellChange {
        sheet: SHEETS[sheet].to_string(),
        coord: at(&format!("{col}{row}")),
        raw,
    }
}

fn assert_indexes_in_step(wb: &WorkbookEngine) {
    for sheet in wb.sheets() {
        let fresh = CellIndex::build(sheet);
        assert_eq!(sheet.cell_index().sorted_coords, fresh.sorted_coords);
        let mut by_col: Vec<_> = sheet.cell_index().by_col.clone().into_iter().collect();
        let mut want: Vec<_> = fresh.by_col.into_iter().collect();
        by_col.sort();
        want.sort();
        assert_eq!(by_col, want);
        let (mut got, mut exp) = (Vec::new(), Vec::new());
        sheet
            .formula_index()
            .collect(0, u32::MAX, 0, u32::MAX, &mut got);
        FormulaIndex::build(sheet).collect(0, u32::MAX, 0, u32::MAX, &mut exp);
        got.sort_unstable();
        exp.sort_unstable();
        assert_eq!(got, exp);
    }
}

fn random_edit_run(seeds: std::ops::RangeInclusive<u64>, acyclic: bool) -> (usize, usize, usize) {
    let (mut circular_steps, mut partial_steps, mut steps) = (0, 0, 0);
    for seed in seeds {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut wb = WorkbookEngine::new();
        for name in SHEETS {
            wb.add_sheet(name);
        }
        for _ in 0..25 {
            if acyclic {
                let c = rand_dag_change(&mut rng);
                wb.set_cell(&c.sheet, &c.coord.to_a1(), c.raw).unwrap();
            } else {
                let sheet = SHEETS[rng.below(3) as usize];
                let raw = rand_raw(&mut rng);
                wb.set_cell(sheet, &rand_cell(&mut rng), raw).unwrap();
            }
        }
        let mut s = RecalcSession::new(wb).unwrap();
        let mut shadow: FastMap<(usize, PackedCellCoordinate), EvaluatedCellValue> =
            FastMap::default();
        for step in 0..70 {
            let batch = if rng.below(8) == 0 {
                90
            } else {
                1 + rng.below(3)
            };
            let mut changes = Vec::new();
            for _ in 0..batch {
                changes.push(if acyclic {
                    rand_dag_change(&mut rng)
                } else {
                    CellChange {
                        sheet: SHEETS[rng.below(3) as usize].to_string(),
                        coord: at(&rand_cell(&mut rng)),
                        raw: rand_raw(&mut rng),
                    }
                });
            }
            s.apply(&changes).unwrap();
            let out = s.recalculate();
            steps += 1;
            circular_steps += usize::from(out.circular);
            partial_steps += usize::from(out.recomputed < s.graph_size().0);
            for c in out.changed {
                shadow.insert((c.sheet, c.coord), c.value);
            }
            let (want, circular) = full_values(s.workbook());
            assert_eq!(
                out.circular, circular,
                "seed {seed} step {step}: circular flag"
            );
            for (&(idx, coord), value) in &want {
                let name = s.sheet_name(idx).unwrap();
                assert_eq!(
                    &s.cell_value(name, coord),
                    value,
                    "seed {seed} step {step}: {name}!{}",
                    coord.to_a1()
                );
                assert_eq!(
                    shadow.get(&(idx, coord)),
                    Some(value),
                    "seed {seed} step {step}: reported value of {name}!{}",
                    coord.to_a1()
                );
            }
            assert_indexes_in_step(s.workbook());
        }
    }
    (steps, circular_steps, partial_steps)
}

#[test]
fn random_acyclic_edits_match_a_full_recalculation_incrementally() {
    let (steps, circular, partial) = random_edit_run(1..=40, true);
    assert_eq!(circular, 0, "the generator never closes a cycle");
    assert!(
        partial > steps * 8 / 10,
        "only {partial}/{steps} partial passes"
    );
}

#[test]
fn random_cyclic_edits_match_a_full_recalculation() {
    let (steps, circular, _) = random_edit_run(1..=40, false);
    assert!(
        circular > steps / 4,
        "only {circular}/{steps} circular steps"
    );
}

#[test]
#[ignore = "long soak: more seeds"]
fn random_edits_soak() {
    random_edit_run(1_000..=1_600, true);
    random_edit_run(2_000..=2_600, false);
}

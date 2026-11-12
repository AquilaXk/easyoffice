//! Sheet-session cases of the measurement harness (`perf_bench.rs`, `docs/performance.md`): the
//! calls the page makes for a big sheet, through the same IPC entry (`evaluate_sheet`). The
//! requests are built outside the timed part, so each phase is the engine's cost.
//!
//! `EASYOFFICE_BENCH_CASE=sheet`, input: the row count. Ten columns per row: two category
//! texts, six numbers, a per-row formula (`=C+D`) and a per-row `SUM` over the numbers, plus one
//! `SUM` over the whole formula column in the last row.

use crate::EasyOfficeIpcRouter;
use easyoffice_sheet::{SortKey, WorkbookEngine};
use std::time::Instant;

const CHUNK_CHANGES: usize = 20_000;
const CATEGORIES: [usize; 2] = [50, 12];
const NUMBER_COLUMNS: u32 = 6;

fn ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

fn call(body: &str) -> String {
    EasyOfficeIpcRouter::dispatch("evaluate_sheet", body).expect("evaluate_sheet")
}

/// `(a1, raw)` of every cell, row by row.
fn sheet_cells(rows: u32) -> Vec<(String, String)> {
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state >> 16
    };
    let mut cells = Vec::with_capacity(rows as usize * 10 + 1);
    for r in 1..=rows {
        cells.push((
            format!("A{r}"),
            format!("부서{}", next() % CATEGORIES[0] as u64),
        ));
        cells.push((
            format!("B{r}"),
            format!("분기{}", next() % CATEGORIES[1] as u64),
        ));
        for col in 0..NUMBER_COLUMNS {
            let name = (b'C' + col as u8) as char;
            cells.push((format!("{name}{r}"), (next() % 100_000).to_string()));
        }
        cells.push((format!("I{r}"), format!("=C{r}+D{r}")));
        cells.push((format!("J{r}"), format!("=SUM(C{r}:H{r})")));
    }
    cells.push((format!("K{rows}"), format!("=SUM(I1:I{rows})")));
    cells
}

fn changes_json(cells: &[(String, String)]) -> String {
    let items: Vec<String> = cells
        .iter()
        .map(|(a1, raw)| format!("{{\"sheet\":\"S\",\"cell\":\"{a1}\",\"value\":\"{raw}\"}}"))
        .collect();
    items.join(",")
}

fn session_id(opened: &str) -> String {
    let at = opened.find("\"session\":\"").expect("session") + 11;
    opened[at..at + opened[at..].find('"').expect("end")].to_string()
}

const RULES: &str = r##"{"cf":[
  {"r":"C1:H$N","p":1,"t":"cellIs","op":"greaterThan","a":"90000","bg":"#FFC7CE","fg":"#9C0006"},
  {"r":"C1:H$N","p":2,"t":"scale","stops":[{"k":"num","v":"0","c":"#F8696B"},{"k":"num","v":"50000","c":"#FFEB84"},{"k":"num","v":"100000","c":"#63BE7B"}]},
  {"r":"C1:H$N","p":3,"t":"bar","min":{"k":"min","v":""},"max":{"k":"max","v":""}}
]}"##;

pub(crate) fn run(rows: u32, phases: &mut Vec<(&'static str, f64)>) -> usize {
    let cells = sheet_cells(rows);
    let chunks: Vec<String> = cells.chunks(CHUNK_CHANGES).map(changes_json).collect();
    let t = Instant::now();
    let opened = call("{\"sessionAction\":\"open\",\"sheets\":[\"S\"]}");
    phases.push(("open", ms(t)));
    let id = session_id(&opened);
    let t = Instant::now();
    for chunk in &chunks {
        call(&format!(
            "{{\"sessionAction\":\"apply\",\"session\":\"{id}\",\"recalc\":false,\"changes\":[{chunk}]}}"
        ));
    }
    phases.push(("load", ms(t)));
    let edit = |cell: &str, value: &str| {
        format!(
            "{{\"sessionAction\":\"apply\",\"session\":\"{id}\",\"changes\":[{{\"sheet\":\"S\",\"cell\":\"{cell}\",\"value\":\"{value}\"}}]}}"
        )
    };
    let t = Instant::now();
    let first = call(&edit("A1", "부서0"));
    phases.push(("recalc_all", ms(t)));
    let mut out_bytes = first.len();
    let t = Instant::now();
    out_bytes += call(&edit("C1", "123")).len();
    phases.push(("edit_leaf", ms(t)));
    let t = Instant::now();
    out_bytes += call(&edit("A2", "수정")).len();
    phases.push(("edit_text", ms(t)));
    let rules = RULES.replace("$N", &rows.to_string());
    let t = Instant::now();
    out_bytes += call(&format!(
        "{{\"sessionAction\":\"rules\",\"session\":\"{id}\",\"sheet\":\"S\",\"rules\":{rules}}}"
    ))
    .len();
    phases.push(("rules", ms(t)));
    let t = Instant::now();
    out_bytes += call(&edit("C2", "99999")).len();
    phases.push(("edit_with_rules", ms(t)));
    call(&format!(
        "{{\"sessionAction\":\"close\",\"session\":\"{id}\"}}"
    ));

    let pivot_cells: Vec<(String, String)> = cells
        .iter()
        .filter(|(a1, _)| matches!(a1.as_bytes()[0], b'A' | b'B' | b'C'))
        .cloned()
        .collect();
    let mut pivot = String::from("{\"sessionAction\":\"pivot\",\"sheet\":\"S\",\"cells\":[");
    pivot.push_str(&pivot_cells_json(&pivot_cells));
    pivot.push_str(&format!(
        "],\"range\":\"A1:C{rows}\",\"header\":false,\"row\":0,\"col\":1,\"value\":2,\"agg\":\"sum\"}}"
    ));
    let t = Instant::now();
    out_bytes += call(&pivot).len();
    phases.push(("pivot", ms(t)));

    let mut engine = WorkbookEngine::new();
    engine.add_sheet("S");
    for (a1, raw) in &cells {
        engine.set_cell("S", a1, raw.as_str()).expect("set");
    }
    let sheet = engine.sheet_mut("S").expect("sheet");
    let t = Instant::now();
    sheet
        .sort_rows(0, rows - 1, &[SortKey::asc(2), SortKey::desc(3)], false)
        .expect("sort");
    phases.push(("sort", ms(t)));
    out_bytes
}

fn pivot_cells_json(cells: &[(String, String)]) -> String {
    let items: Vec<String> = cells
        .iter()
        .map(|(a1, raw)| format!("{{\"cell\":\"{a1}\",\"value\":\"{raw}\"}}"))
        .collect();
    items.join(",")
}

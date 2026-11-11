use super::*;

fn dispatch(command: &str, payload: &str) -> String {
    EasyOfficeIpcRouter::dispatch(command, payload).unwrap()
}

fn open(sheets: &str) -> String {
    let res = dispatch(
        "evaluate_sheet",
        &format!("{{\"sessionAction\":\"open\",\"sheets\":{sheets}}}"),
    );
    let start = res.find("\"session\":\"").unwrap() + 11;
    res[start..start + res[start..].find('"').unwrap()].to_string()
}

fn apply(id: &str, changes: &str) -> String {
    dispatch(
        "evaluate_sheet",
        &format!("{{\"sessionAction\":\"apply\",\"session\":\"{id}\",\"changes\":[{changes}]}}"),
    )
}

fn close(id: &str) -> String {
    dispatch(
        "evaluate_sheet",
        &format!("{{\"sessionAction\":\"close\",\"session\":\"{id}\"}}"),
    )
}

fn change(sheet: &str, cell: &str, value: &str) -> String {
    format!(
        "{{\"sheet\":\"{sheet}\",\"cell\":\"{cell}\",\"value\":\"{}\"}}",
        EasyOfficeIpcRouter::escape_json_str(value)
    )
}

#[test]
fn a_session_returns_only_the_formula_cells_an_edit_changed() {
    let id = open("[\"Sheet1\",\"Sheet2\"]");
    let first = apply(
        &id,
        &[
            change("Sheet1", "A1", "2"),
            change("Sheet1", "B1", "=A1*10"),
            change("Sheet2", "A1", "=Sheet1!B1+1"),
            change("Sheet1", "A2", "5"),
            change("Sheet1", "B2", "=A2+1"),
        ]
        .join(","),
    );
    assert!(first.contains("{\"sheet\":\"Sheet1\",\"cell\":\"B1\",\"value\":\"20\"}"));
    assert!(first.contains("{\"sheet\":\"Sheet2\",\"cell\":\"A1\",\"value\":\"21\"}"));
    assert!(first.contains("\"circular\":false"));

    let second = apply(&id, &change("Sheet1", "A1", "3"));
    assert!(second.contains("\"cell\":\"B1\",\"value\":\"30\""));
    assert!(second.contains("\"cell\":\"A1\",\"value\":\"31\""));
    assert!(
        !second.contains("\"cell\":\"B2\""),
        "B2 does not read A1: {second}"
    );
    assert!(second.contains("\"recomputed\":2"));

    let same = apply(&id, &change("Sheet1", "A1", "3"));
    assert!(same.contains("\"changed\":[]") && same.contains("\"recomputed\":0"));
    assert_eq!(close(&id), "{\"closed\":true}");
}

#[test]
fn session_values_equal_the_stateless_evaluation() {
    let cells = [
        ("Sheet1", "A1", "4"),
        ("Sheet1", "A2", "6"),
        ("Sheet1", "A3", "=SUM(A1:A2)*Sheet2!B1"),
        ("Sheet1", "B1", "=A3&\"x\""),
        ("Sheet2", "B1", "=IF(Sheet1!A1>3,2,3)"),
        ("Sheet2", "B2", "=B2+1"),
    ];
    let id = open("[\"Sheet1\",\"Sheet2\"]");
    let changes: Vec<String> = cells.iter().map(|&(s, c, v)| change(s, c, v)).collect();
    let session = apply(&id, &changes.join(","));
    let targets: Vec<String> = cells
        .iter()
        .filter(|c| c.2.starts_with('='))
        .map(|&(s, c, _)| format!("{{\"sheet\":\"{s}\",\"cell\":\"{c}\"}}"))
        .collect();
    let stateless = dispatch(
        "evaluate_sheet",
        &format!(
            "{{\"sheet\":\"Sheet1\",\"sheets\":[\"Sheet1\",\"Sheet2\"],\"targets\":[{}],\"cells\":[{}]}}",
            targets.join(","),
            changes.join(",")
        ),
    );
    for &(sheet, cell, raw) in &cells {
        if !raw.starts_with('=') {
            continue;
        }
        let want = stateless
            .split(&format!("\"{sheet}!{cell}\":\""))
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .unwrap();
        assert!(
            session.contains(&format!(
                "{{\"sheet\":\"{sheet}\",\"cell\":\"{cell}\",\"value\":\"{want}\"}}"
            )),
            "{sheet}!{cell}: want {want} in {session}"
        );
    }
    assert!(session.contains("\"circular\":true") && stateless.contains("\"circular\":true"));
}

#[test]
fn staged_chunks_recalculate_once_at_the_end() {
    let id = open("[\"S\"]");
    let staged = dispatch(
        "evaluate_sheet",
        &format!(
            "{{\"sessionAction\":\"apply\",\"session\":\"{id}\",\"recalc\":false,\"changes\":[{}]}}",
            [change("S", "A1", "7"), change("S", "B1", "=A1+1")].join(",")
        ),
    );
    assert_eq!(staged, format!("{{\"session\":\"{id}\",\"staged\":2}}"));
    let done = apply(&id, &change("S", "C1", "=B1*2"));
    assert!(done.contains("\"cell\":\"B1\",\"value\":\"8\""));
    assert!(done.contains("\"cell\":\"C1\",\"value\":\"16\""));
}

#[test]
fn unknown_closed_and_owner_closed_sessions_answer_expired_not_an_error() {
    let expired = "{\"expired\":true}";
    assert_eq!(apply("ss-nope", &change("S", "A1", "1")), expired);
    let id = open("[\"S\"]");
    close(&id);
    assert_eq!(apply(&id, &change("S", "A1", "1")), expired);

    let owned = EasyOfficeIpcRouter::dispatch_for_owner(
        "evaluate_sheet",
        "{\"sessionAction\":\"open\",\"sheets\":[\"S\"]}",
        "win-owner-test",
    )
    .unwrap();
    let id = owned.split('"').nth(3).unwrap().to_string();
    let payload = format!(
        "{{\"sessionAction\":\"apply\",\"session\":\"{id}\",\"changes\":[{}]}}",
        change("S", "A1", "1")
    );
    let as_owner = |owner: &str| {
        EasyOfficeIpcRouter::dispatch_for_owner("evaluate_sheet", &payload, owner).unwrap()
    };
    assert!(as_owner("win-owner-test").contains("\"changed\":[]"));
    assert_eq!(
        as_owner("another-window"),
        expired,
        "not that window's session"
    );
    assert_eq!(close_sheet_sessions_of("win-owner-test"), 1);
    assert_eq!(as_owner("win-owner-test"), expired);
    assert_eq!(close_sheet_sessions_of("win-owner-test"), 0);
}

#[test]
fn malformed_requests_are_rejected_and_leave_the_session_intact() {
    let id = open("[\"S\"]");
    apply(&id, &change("S", "A1", "1"));
    for bad in [
        "{\"sessionAction\":\"apply\",\"changes\":[]}".to_string(),
        "{\"sessionAction\":\"launch\"}".to_string(),
        format!("{{\"sessionAction\":\"apply\",\"session\":\"{id}\",\"changes\":{{}}}}"),
        format!("{{\"sessionAction\":\"apply\",\"session\":\"{id}\",\"changes\":[1]}}"),
        format!("{{\"sessionAction\":\"apply\",\"session\":\"{id}\",\"changes\":[{}]}}", change("Nope", "A1", "1")),
        format!("{{\"sessionAction\":\"apply\",\"session\":\"{id}\",\"changes\":[{}]}}", change("S", "ZZZZZ99999999", "1")),
        format!("{{\"sessionAction\":\"apply\",\"session\":\"{id}\",\"changes\":[{{\"sheet\":\"S\",\"cell\":\"A1\",\"value\":[1]}}]}}"),
    ] {
        assert!(
            EasyOfficeIpcRouter::dispatch("evaluate_sheet", &bad).is_err(),
            "{bad}"
        );
    }
    for bad_open in [
        "{\"sessionAction\":\"open\",\"sheets\":[\" \"]}",
        "{\"sessionAction\":\"open\"}",
    ] {
        assert!(EasyOfficeIpcRouter::dispatch("evaluate_sheet", bad_open).is_err());
    }
    let res = apply(&id, &change("S", "B1", "=A1+1"));
    assert!(res.contains("\"cell\":\"B1\",\"value\":\"2\""), "{res}");
}

fn limits() -> Limits {
    Limits {
        max_sessions: 2,
        max_sheets: 3,
        max_sheet_name_bytes: 8,
        max_session_cells: 5,
        max_session_edges: 50,
        max_session_bytes: 64,
        max_total_cells: 6,
        max_total_bytes: 100,
        max_changes: 4,
        idle_ttl: Duration::from_secs(60),
    }
}

fn put(raw: &str, n: u32) -> Vec<CellChange> {
    (0..n)
        .map(|i| CellChange {
            sheet: "S".into(),
            coord: PackedCellCoordinate::new(i, 0).unwrap(),
            raw: raw.into(),
        })
        .collect()
}

fn alive(r: Result<Applied, EasyOfficeError>) -> bool {
    matches!(r.unwrap(), Applied::Done(_))
}

fn gone(r: Result<Applied, EasyOfficeError>) -> bool {
    matches!(r.unwrap(), Applied::Expired)
}

fn refused(r: Result<Applied, EasyOfficeError>) -> bool {
    matches!(r.unwrap(), Applied::Refused)
}

fn sec(t: Instant, s: u64) -> Instant {
    t + Duration::from_secs(s)
}

#[test]
fn the_registry_evicts_the_least_recently_used_session_over_the_count_limit() {
    let reg = Registry::new(limits());
    let t = Instant::now();
    let sheets = vec!["S".to_string()];
    let a = reg.open(&sheets, "", t).unwrap();
    let b = reg.open(&sheets, "", sec(t, 1)).unwrap();
    assert!(alive(reg.apply(&a, "", &put("1", 1), true, sec(t, 2))));
    let c = reg.open(&sheets, "", sec(t, 3)).unwrap();
    assert_eq!(reg.len(), 2);
    assert!(
        gone(reg.apply(&b, "", &put("1", 1), true, sec(t, 4))),
        "b was the least recently used"
    );
    assert!(alive(reg.apply(&a, "", &put("1", 1), true, sec(t, 4))));
    assert!(alive(reg.apply(&c, "", &put("1", 1), true, sec(t, 4))));
}

#[test]
fn an_idle_session_expires() {
    let reg = Registry::new(limits());
    let t = Instant::now();
    let id = reg.open(&["S".to_string()], "", t).unwrap();
    assert!(alive(reg.apply(&id, "", &put("1", 1), true, sec(t, 59))));
    assert!(gone(reg.apply(
        &id,
        "",
        &put("2", 1),
        true,
        sec(t, 59 + 61)
    )));
    assert_eq!(reg.len(), 0);
}

#[test]
fn a_session_belongs_to_the_owner_that_opened_it() {
    let reg = Registry::new(limits());
    let t = Instant::now();
    let id = reg.open(&["S".to_string()], "win-a", t).unwrap();
    assert!(id.starts_with("ss") && id.len() == 2 + 32);
    assert_ne!(id, reg.open(&["S".to_string()], "win-a", t).unwrap());
    assert!(gone(reg.apply(&id, "win-b", &put("1", 1), true, t)));
    assert!(!reg.close(&id, "win-b"), "another window cannot close it");
    assert!(alive(reg.apply(&id, "win-a", &put("1", 1), true, t)));
    assert!(reg.close(&id, "win-a"));
}

#[test]
fn open_bounds_the_sheet_count_and_name_length_before_building_a_workbook() {
    let reg = Registry::new(limits());
    let t = Instant::now();
    let four: Vec<String> = (0..4).map(|i| format!("S{i}")).collect();
    assert!(reg.open(&four, "", t).is_err(), "4 sheets > 3");
    assert!(reg.open(&["x".repeat(9)], "", t).is_err());
    assert!(reg.open(&four[..3], "", t).is_ok());
    // The shipped limit stays far below the 65,535 sheet ids the workbook can hand out.
    assert!(LIMITS.max_sheets < usize::from(u16::MAX) / 8);
    let many: Vec<String> = (0..=LIMITS.max_sheets).map(|i| format!("S{i}")).collect();
    assert!(Registry::new(LIMITS).open(&many, "", t).is_err());
}

#[test]
fn size_limits_drop_a_session_that_would_grow_past_them() {
    let reg = Registry::new(limits());
    let t = Instant::now();
    let sheets = vec!["S".to_string()];
    let id = reg.open(&sheets, "", t).unwrap();
    assert!(
        reg.apply(&id, "", &put("1", 5), true, t).is_err(),
        "more changes than max_changes is a malformed request"
    );
    assert!(alive(reg.apply(&id, "", &put("1", 3), true, t)));
    assert!(
        refused(reg.apply(&id, "", &put("2", 3), true, t)),
        "3 + 3 cells > 5"
    );
    assert!(
        gone(reg.apply(&id, "", &put("9", 1), true, t)),
        "and the session is gone"
    );
    let id = reg.open(&sheets, "", t).unwrap();
    assert!(
        refused(reg.apply(&id, "", &put(&"x".repeat(40), 2), true, t)),
        "bytes over max_session_bytes"
    );
}

#[test]
fn a_dependency_graph_past_the_edge_budget_is_refused_and_dropped() {
    let reg = Registry::new(Limits {
        max_session_bytes: 1 << 20,
        max_total_bytes: 1 << 20,
        ..limits()
    });
    let t = Instant::now();
    let id = reg.open(&["S".to_string()], "", t).unwrap();
    // 40 reads per formula: the second formula passes max_session_edges = 50.
    let reads: Vec<String> = (1..=40).map(|k| format!("Z{k}")).collect();
    let formula = |row: u32| CellChange {
        sheet: "S".into(),
        coord: PackedCellCoordinate::new(row, 0).unwrap(),
        raw: format!("={}", reads.join("+")),
    };
    assert!(alive(reg.apply(&id, "", &[formula(0)], true, t)));
    assert!(refused(reg.apply(&id, "", &[formula(1)], true, t)));
    assert!(gone(reg.apply(&id, "", &[], true, t)));
    assert_eq!(reg.len(), 0);
}

#[test]
fn total_memory_pressure_evicts_older_sessions_first() {
    let reg = Registry::new(limits());
    let t = Instant::now();
    let sheets = vec!["S".to_string()];
    let a = reg.open(&sheets, "", t).unwrap();
    let b = reg.open(&sheets, "", sec(t, 1)).unwrap();
    let raw = "y".repeat(30);
    assert!(alive(reg.apply(&a, "", &put(&raw, 2), true, sec(t, 2))));
    // b needs 60 bytes while a holds 60: 120 > 100 evicts a.
    assert!(alive(reg.apply(&b, "", &put(&raw, 2), true, sec(t, 3))));
    assert!(gone(reg.apply(&a, "", &put("1", 1), true, sec(t, 4))));
}

#[test]
fn total_cell_pressure_evicts_older_sessions_first() {
    let reg = Registry::new(limits());
    let t = Instant::now();
    let sheets = vec!["S".to_string()];
    let a = reg.open(&sheets, "", t).unwrap();
    let b = reg.open(&sheets, "", sec(t, 1)).unwrap();
    assert!(alive(reg.apply(&a, "", &put("1", 4), true, sec(t, 2))));
    assert!(
        alive(reg.apply(&b, "", &put("1", 3), true, sec(t, 3))),
        "4 + 3 cells > 6 evicts a"
    );
    assert!(gone(reg.apply(&a, "", &put("1", 1), true, sec(t, 4))));
}

#[test]
fn a_poisoned_session_is_dropped_and_reported_as_expired() {
    let reg = Registry::new(limits());
    let t = Instant::now();
    let id = reg.open(&["S".to_string()], "", t).unwrap();
    let handle = lock(&reg.inner).sessions[&id].session.clone();
    let poisoner = std::thread::spawn(move || {
        let _guard = handle.lock().unwrap();
        panic!("engine panic while the session was locked");
    });
    assert!(poisoner.join().is_err());
    assert!(gone(reg.apply(&id, "", &put("1", 1), true, t)));
    assert_eq!(reg.len(), 0, "the poisoned session is removed");
}

#[test]
fn the_crowded_dependents_pattern_is_bounded_by_the_shipped_limits() {
    // 33 formulas per group read the same 40 cells, groups read other cells: a bit set per read
    // cell sized by the highest vertex id used 296 MB at 30,000 formulas.
    let t = Instant::now();
    let reg = Registry::new(LIMITS);
    let id = reg.open(&["S".to_string()], "", t).unwrap();
    let changes: Vec<CellChange> = (0..30_000u32)
        .map(|i| {
            let first = i / 33 * 40;
            let terms: Vec<String> = (1..=40).map(|k| format!("Z{}", first + k)).collect();
            CellChange {
                sheet: "S".into(),
                coord: PackedCellCoordinate::new(i, 0).unwrap(),
                raw: format!("={}", terms.join("+")),
            }
        })
        .collect();
    assert!(alive(reg.apply(&id, "", &changes, true, t)));
    let handle = lock(&reg.inner).sessions[&id].session.clone();
    let fp = handle.lock().unwrap().footprint();
    assert_eq!(fp.edges, 30_000 * 40);
    assert!(fp.total_bytes() < LIMITS.max_session_bytes / 2, "{fp:?}");
}

#[test]
fn bootstrap_advertises_sheet_sessions() {
    assert!(dispatch("get_workspace_bootstrap", "").contains("\"sheetSessions\":true"));
}

/// Measurement harness (not run by default):
/// `cargo test -p easyoffice-app --release --lib -- --ignored --nocapture ipc_recalc_bench`
///
/// Per edit of one cell: the stateless `evaluate_sheet` (every cell and every formula target in the
/// request, the workbook rebuilt and recomputed) against the session `apply` action (the one cell).
#[test]
#[ignore = "measurement harness"]
fn ipc_recalc_bench() {
    for rows in [3_334u32, 33_334] {
        let mut cells: Vec<(String, String)> = Vec::new();
        for r in 1..=rows {
            let prev = if r == 1 {
                "0".to_string()
            } else {
                format!("D{}", r - 1)
            };
            cells.push((format!("A{r}"), r.to_string()));
            cells.push((format!("B{r}"), format!("=A{r}*2")));
            cells.push((format!("C{r}"), format!("=B{r}+A{r}")));
            cells.push((format!("D{r}"), format!("={prev}+C{r}")));
        }
        cells.push(("F1".into(), format!("=SUM(C1:C{rows})")));
        let cell_json: Vec<String> = cells.iter().map(|(c, v)| change("Sheet1", c, v)).collect();
        let targets: Vec<String> = cells
            .iter()
            .filter(|(_, v)| v.starts_with('='))
            .map(|(c, _)| format!("{{\"sheet\":\"Sheet1\",\"cell\":\"{c}\"}}"))
            .collect();
        let formulas = targets.len();
        let stateless = format!(
            "{{\"sheet\":\"Sheet1\",\"sheets\":[\"Sheet1\"],\"targets\":[{}],\"cells\":[{}]}}",
            targets.join(","),
            cell_json.join(",")
        );
        let t = Instant::now();
        let response = dispatch("evaluate_sheet", &stateless);
        let stateless_ms = t.elapsed().as_secs_f64() * 1e3;
        println!(
            "formulas={formulas} cells={}: stateless edit = request {} B + response {} B, {stateless_ms:.1} ms (10 MiB limit = 10485760 B)",
            cells.len(),
            stateless.len(),
            response.len()
        );

        let id = open("[\"Sheet1\"]");
        let first_payload = format!(
            "{{\"sessionAction\":\"apply\",\"session\":\"{id}\",\"changes\":[{}]}}",
            cell_json.join(",")
        );
        let t = Instant::now();
        let first = dispatch("evaluate_sheet", &first_payload);
        println!(
            "  session open (once): request {} B + response {} B, {:.1} ms",
            first_payload.len(),
            first.len(),
            t.elapsed().as_secs_f64() * 1e3
        );
        for (label, row) in [("tail", rows), ("middle", rows / 2), ("head", 1)] {
            let mut best = (f64::MAX, 0, 0);
            for round in 0..5 {
                let payload = format!(
                    "{{\"sessionAction\":\"apply\",\"session\":\"{id}\",\"changes\":[{}]}}",
                    change("Sheet1", &format!("A{row}"), &(1_000 + round).to_string())
                );
                let t = Instant::now();
                let res = dispatch("evaluate_sheet", &payload);
                let ms = t.elapsed().as_secs_f64() * 1e3;
                if ms < best.0 {
                    best = (ms, payload.len(), res.len());
                }
            }
            println!(
                "  session {label} edit A{row}: request {} B + response {} B, {:.2} ms",
                best.1, best.2, best.0
            );
        }
        close(&id);
    }
}

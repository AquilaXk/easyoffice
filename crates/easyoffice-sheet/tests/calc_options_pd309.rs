//! PD-309: `INFO("RECALC")` reflection and bounded iterative circular calculation in both
//! stateless `WorkbookEngine` evaluation and stateful `RecalcSession`.
#![allow(clippy::unwrap_used)]

use std::time::Duration;

use easyoffice_sheet::{
    enter_calc_options, CellChange, EvaluatedCellValue, PackedCellCoordinate, RecalcSession,
    WorkbookEngine, WorkbookEvalTarget,
};

fn num(v: &EvaluatedCellValue) -> f64 {
    match v {
        EvaluatedCellValue::Number(n) => *n,
        other => panic!("expected Number, got {other:?}"),
    }
}

#[test]
fn info_recalc_follows_calc_mode_and_invalidates_recalc_session() {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("S1");
    wb.set_cell("S1", "A1", r#"=INFO("RECALC")"#).unwrap();

    let res_auto = wb.evaluate_targets_within(
        &[WorkbookEvalTarget {
            sheet: "S1",
            expr: "A1",
            anchor: None,
        }],
        Duration::from_secs(5),
    );
    assert_eq!(
        res_auto.values[0],
        EvaluatedCellValue::Text("Automatic".to_string())
    );

    {
        let _manual = enter_calc_options(true, None);
        let res_manual = wb.evaluate_targets_within(
            &[WorkbookEvalTarget {
                sheet: "S1",
                expr: "A1",
                anchor: None,
            }],
            Duration::from_secs(5),
        );
        assert_eq!(
            res_manual.values[0],
            EvaluatedCellValue::Text("Manual".to_string())
        );
    }

    // RecalcSession notices calculation mode switch without cell edits.
    let mut session = RecalcSession::new(wb).unwrap();
    let first = session.recalculate();
    assert_eq!(first.changed.len(), 1);
    assert_eq!(
        first.changed[0].value,
        EvaluatedCellValue::Text("Automatic".to_string())
    );
    {
        let _manual = enter_calc_options(true, None);
        let second = session.recalculate();
        assert_eq!(second.changed.len(), 1);
        assert_eq!(
            second.changed[0].value,
            EvaluatedCellValue::Text("Manual".to_string())
        );
    }
    let third = session.recalculate();
    assert_eq!(third.changed.len(), 1);
    assert_eq!(
        third.changed[0].value,
        EvaluatedCellValue::Text("Automatic".to_string())
    );
}

#[test]
fn circular_reference_converges_when_iteration_enabled_and_honors_max_iters() {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("S1");
    wb.set_cell("S1", "A1", "=B1+1").unwrap();
    wb.set_cell("S1", "B1", "=A1/2").unwrap();
    let targets = [
        WorkbookEvalTarget {
            sheet: "S1",
            expr: "A1",
            anchor: None,
        },
        WorkbookEvalTarget {
            sheet: "S1",
            expr: "B1",
            anchor: None,
        },
    ];

    // Iteration disabled -> #NUM! error and circular flag true.
    let no_iter = wb.evaluate_targets_within(&targets, Duration::from_secs(5));
    assert!(no_iter.circular);
    assert!(matches!(no_iter.values[0], EvaluatedCellValue::Error(_)));

    // Iteration enabled -> converges to A1 = 2.0, B1 = 1.0, circular = false.
    {
        let _iter = enter_calc_options(false, Some((100, 0.0001)));
        let converged = wb.evaluate_targets_within(&targets, Duration::from_secs(5));
        assert!(!converged.circular);
        assert!(
            (num(&converged.values[0]) - 2.0).abs() < 1e-3,
            "{:?}",
            converged.values
        );
        assert!(
            (num(&converged.values[1]) - 1.0).abs() < 1e-3,
            "{:?}",
            converged.values
        );
    }

    // Iteration with count = 1 stops after 1 Gauss-Seidel pass without error.
    {
        let _one = enter_calc_options(false, Some((1, 0.0)));
        let one_pass = wb.evaluate_targets_within(&targets, Duration::from_secs(5));
        assert!(!one_pass.circular);
        assert!(
            (num(&one_pass.values[0]) - 1.0).abs() < 1e-9,
            "{:?}",
            one_pass.values
        );
        assert!(
            (num(&one_pass.values[1]) - 0.5).abs() < 1e-9,
            "{:?}",
            one_pass.values
        );
    }

    // RecalcSession tracks iterative mode and self-accumulation counter.
    let mut acc_wb = WorkbookEngine::new();
    acc_wb.add_sheet("S1");
    acc_wb.set_cell("S1", "A1", "=B1+1").unwrap();
    acc_wb.set_cell("S1", "B1", "=A1/2").unwrap();
    let mut session = RecalcSession::new(acc_wb).unwrap();
    let base = session.recalculate();
    assert!(base.circular);
    {
        let _iter = enter_calc_options(false, Some((100, 0.0001)));
        let converged = session.recalculate();
        assert!(!converged.circular);
        let a1 = PackedCellCoordinate::parse_a1("A1").unwrap();
        let b1 = PackedCellCoordinate::parse_a1("B1").unwrap();
        let a_val = converged.changed.iter().find(|c| c.coord == a1).unwrap();
        let b_val = converged.changed.iter().find(|c| c.coord == b1).unwrap();
        assert!((num(&a_val.value) - 2.0).abs() < 1e-3);
        assert!((num(&b_val.value) - 1.0).abs() < 1e-3);

        // Editing B1 breaks and restores convergence on next pass cleanly.
        session
            .apply(&[CellChange {
                sheet: "S1".to_string(),
                coord: b1,
                raw: "10".to_string(),
            }])
            .unwrap();
        let after_edit = session.recalculate();
        assert!(!after_edit.circular);
        let a_after = after_edit.changed.iter().find(|c| c.coord == a1).unwrap();
        assert!((num(&a_after.value) - 11.0).abs() < 1e-9);
    }
}

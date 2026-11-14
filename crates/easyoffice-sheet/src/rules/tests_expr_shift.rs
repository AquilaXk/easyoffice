use super::cf::truthy;
use super::expr_shift::RuleExprTemplate;
use super::shift::shift_formula;
use crate::{EvalContext, PackedCellCoordinate, WorkbookEngine};
use easyoffice_formats::{parse_sqref, CfKind, CfRule, CfStyle, DefinedName, SheetRules};

fn rule_expr(ranges: &str, expr: &str) -> CfRule {
    CfRule {
        ranges: parse_sqref(ranges),
        priority: 1,
        stop_if_true: false,
        kind: CfKind::Expression(expr.to_string()),
        style: CfStyle {
            bg: Some(0xFF_00_00),
            fg: None,
            flags: 0,
        },
    }
}

#[test]
fn cf_expression_cumulative_step_budget_stops_exhausted_rules() {
    let mut wb = WorkbookEngine::new();
    let _ = wb.add_sheet("Sheet1");
    for row in 1..=4_000 {
        wb.set_cell("Sheet1", &format!("A{row}"), "1").unwrap();
    }
    let mut expr = String::from("A1");
    for _ in 0..600 {
        expr.push_str("+1");
    }
    expr.push_str(">0");
    let rules = SheetRules {
        cf: vec![rule_expr("A1:A4000", &expr)],
        ..SheetRules::default()
    };
    let visuals = wb.evaluate_rules("Sheet1", rules);
    assert!(
        visuals.len() < 4_000,
        "cumulative step budget must cut off rule evaluation instead of resetting steps per cell (matched {})",
        visuals.len()
    );
}

#[test]
fn cf_expression_ast_shift_matches_legacy_text_shift_for_all_edge_cases() {
    let mut wb = WorkbookEngine::new();
    let _ = wb.add_sheet("Sheet1");
    let _ = wb.add_sheet("Sheet2");
    let _ = wb.add_sheet("Sheet Two");

    wb.set_names(&[
        DefinedName {
            name: "GlobalThresh".to_string(),
            refers_to: "=15".to_string(),
            scope: None,
            ..DefinedName::default()
        },
        DefinedName {
            name: "SheetRange".to_string(),
            refers_to: "=Sheet1!$D$1:$D$3".to_string(),
            scope: Some("Sheet1".to_string()),
            ..DefinedName::default()
        },
        DefinedName {
            name: "ComplexShared".to_string(),
            refers_to: "=IF(Sheet2!$A$1>0,Sheet2!$A$1*2,5)".to_string(),
            scope: None,
            ..DefinedName::default()
        },
    ])
    .unwrap();

    for r in 1..=6 {
        wb.set_cell("Sheet1", &format!("A{r}"), (r * 5).to_string())
            .unwrap();
        wb.set_cell("Sheet1", &format!("B{r}"), "10").unwrap();
        wb.set_cell("Sheet1", &format!("C{r}"), (r % 2 * 20).to_string())
            .unwrap();
        wb.set_cell("Sheet1", &format!("D{r}"), (r * 3).to_string())
            .unwrap();
        wb.set_cell("Sheet2", &format!("A{r}"), (r * 4).to_string())
            .unwrap();
        wb.set_cell("Sheet Two", &format!("A{r}"), (r * 2).to_string())
            .unwrap();
    }
    wb.set_cell("Sheet1", "E1", "A1").unwrap();
    wb.set_cell("Sheet1", "F1", "=10").unwrap();
    wb.set_cell("Sheet1", "F2", "=1/0").unwrap();

    let formulas = [
        "A1>B1",
        "$A1>B$2",
        "A$1+$B1>20",
        "$A$2>10",
        "SUM(A1:B2)>GlobalThresh",
        "SUM($B$2:A1)>15",
        "SUM(SheetRange)+A1>GlobalThresh",
        "ComplexShared+A1>20",
        "Sheet2!A1>$B$1",
        "'Sheet Two'!A$1>2",
        "MissingSheet!A1>0",
        "SUM(A:A)>50",
        "SUM(1:2)>20",
        "LET(x,A1,x*2>$B$1)",
        "SUM(A1:A2*B1:B2)>200",
        "E1=\"A1\"",
        "IF(ISERROR(F1),FALSE,F1>0)",
        "A1/0",
        "UnknownName>0",
        "(((",
        "TRUE",
        "",
        "=A1>0",
    ];

    let sheet = wb.sheet("Sheet1").unwrap();
    let sid = sheet.resolved_sheet_id();

    for formula in formulas {
        let rules = SheetRules {
            cf: vec![rule_expr("B2:C4 A5:B6", formula)],
            ..SheetRules::default()
        };
        let actual_visuals = wb.evaluate_rules("Sheet1", rules);
        let mut actual_hits: Vec<PackedCellCoordinate> =
            actual_visuals.iter().map(|v| v.coord).collect();
        actual_hits.sort_unstable();

        // Oracle: exact anchor-based legacy string shift + eval_raw_string_ctx.
        let boxes = parse_sqref("B2:C4 A5:B6");
        let (ar, ac) = boxes.iter().map(|b| (b.row0, b.col0)).min().unwrap();
        let mut coords = Vec::new();
        for b in &boxes {
            for r in b.row0..=b.row1 {
                for c in b.col0..=b.col1 {
                    coords.push(PackedCellCoordinate::new(r, c).unwrap());
                }
            }
        }
        coords.sort_unstable();
        coords.dedup();

        let template = RuleExprTemplate::compile(sheet, formula);
        let mut expected_hits = Vec::new();
        for coord in coords {
            let dr = i64::from(coord.row()) - i64::from(ar);
            let dc = i64::from(coord.col()) - i64::from(ac);
            let mut ctx = EvalContext::with_workbook(&wb, sid);
            let oracle = shift_formula(formula, dr, dc).is_some_and(|shifted| {
                truthy(&sheet.eval_raw_string_ctx(&format!("={shifted}"), &mut ctx, 0))
            });
            // Also verify shift_expr_offset directly matches when shifted AST exists.
            if let Some(shifted_ast) = template.shift_expr_offset(dr, dc) {
                let mut ctx2 = EvalContext::with_workbook(&wb, sid);
                let direct = truthy(&sheet.eval_expr_ctx(&shifted_ast, &mut ctx2, 0));
                if !crate::array_class::needs_array(&shifted_ast) {
                    assert_eq!(direct, oracle, "formula `{formula}` at offset ({dr},{dc})");
                }
            }
            if oracle {
                expected_hits.push(coord);
            }
        }
        assert_eq!(
            actual_hits, expected_hits,
            "AST shift rule hits diverged from legacy text-shift oracle for formula `{formula}`"
        );
    }
}

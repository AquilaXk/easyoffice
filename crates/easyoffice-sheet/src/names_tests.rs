use crate::axis::{Axis, AxisBudget, AxisEdit, EditKind};
use crate::names::{MAX_FORMULA_NODES, MAX_NAME_DEPTH};
use crate::{EvaluatedCellValue as V, FormulaError, NameIssue, WorkbookEngine};
use easyoffice_formats::{DefinedName, MAX_NAMES};

fn def(name: &str, scope: Option<&str>, refers_to: &str) -> DefinedName {
    DefinedName {
        name: name.to_string(),
        scope: scope.map(str::to_string),
        refers_to: refers_to.to_string(),
        ..DefinedName::default()
    }
}

fn book() -> WorkbookEngine {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");
    wb.add_sheet("Data");
    for (i, v) in ["10", "20", "30"].iter().enumerate() {
        wb.set_cell("Sheet1", &format!("A{}", i + 1), *v).unwrap();
        wb.set_cell("Data", &format!("A{}", i + 1), (i + 1).to_string())
            .unwrap();
    }
    wb
}

fn value(wb: &WorkbookEngine, sheet: &str, cell: &str) -> V {
    wb.evaluate_cell(sheet, cell)
}

#[test]
fn a_name_stands_for_a_range_a_constant_or_a_formula() {
    let mut wb = book();
    wb.set_names(&[
        def("Prices", None, "Sheet1!$A$1:$A$3"),
        def("Margin", None, "0.5"),
        def("Greeting", None, "\"hi\""),
        def("Double", None, "Sheet1!$A$1*2"),
        def("Total", None, "SUM(Prices)"),
    ])
    .unwrap();
    for (cell, formula) in [
        ("C1", "=SUM(Prices)"),
        ("C2", "=Margin*Sheet1!A1"),
        ("C3", "=Greeting&\"!\""),
        ("C4", "=Double+1"),
        ("C5", "=Total/Margin"),
        ("C6", "=MAX(Prices)-MIN(PRICES)"),
        ("C7", "=INDEX(Prices,2)"),
        ("C8", "=VLOOKUP(20,Sheet1!A1:A3,1,FALSE)+COUNT(Prices)"),
    ] {
        wb.set_cell("Sheet1", cell, formula).unwrap();
    }
    assert_eq!(value(&wb, "Sheet1", "C1"), V::Number(60.0));
    assert_eq!(value(&wb, "Sheet1", "C2"), V::Number(5.0));
    assert_eq!(value(&wb, "Sheet1", "C3"), V::Text("hi!".into()));
    assert_eq!(value(&wb, "Sheet1", "C4"), V::Number(21.0));
    assert_eq!(value(&wb, "Sheet1", "C5"), V::Number(120.0));
    assert_eq!(value(&wb, "Sheet1", "C6"), V::Number(20.0));
    assert_eq!(value(&wb, "Sheet1", "C7"), V::Number(20.0));
    assert_eq!(value(&wb, "Sheet1", "C8"), V::Number(23.0));
}

#[test]
fn a_sheet_name_beats_a_workbook_name_and_can_be_asked_for_by_its_sheet() {
    let mut wb = book();
    wb.set_names(&[
        def("Pick", None, "Sheet1!$A$1"),
        def("Pick", Some("Data"), "Data!$A$3"),
        def("Only", Some("Data"), "Data!$A$2"),
    ])
    .unwrap();
    wb.set_cell("Sheet1", "C1", "=Pick").unwrap();
    wb.set_cell("Data", "C1", "=Pick").unwrap();
    wb.set_cell("Sheet1", "C2", "=Data!Pick").unwrap();
    wb.set_cell("Sheet1", "C3", "=Only").unwrap();
    wb.set_cell("Sheet1", "C4", "=Data!Only").unwrap();
    assert_eq!(value(&wb, "Sheet1", "C1"), V::Number(10.0));
    assert_eq!(value(&wb, "Data", "C1"), V::Number(3.0));
    assert_eq!(value(&wb, "Sheet1", "C2"), V::Number(3.0));
    assert_eq!(value(&wb, "Sheet1", "C3"), V::Error(FormulaError::Name));
    assert_eq!(value(&wb, "Sheet1", "C4"), V::Number(2.0));
}

#[test]
fn an_unknown_name_reads_name_error_until_it_is_defined() {
    let mut wb = book();
    wb.set_cell("Sheet1", "C1", "=Later+1").unwrap();
    assert_eq!(value(&wb, "Sheet1", "C1"), V::Error(FormulaError::Name));
    wb.set_names(&[def("later", None, "Sheet1!$A$2")]).unwrap();
    assert_eq!(value(&wb, "Sheet1", "C1"), V::Number(21.0));
    wb.set_names(&[]).unwrap();
    assert_eq!(value(&wb, "Sheet1", "C1"), V::Error(FormulaError::Name));
}

#[test]
fn a_definition_that_changes_recompiles_only_the_formulas_that_use_it() {
    let mut wb = book();
    wb.set_names(&[def("X", None, "Sheet1!$A$1"), def("Y", None, "X+1")])
        .unwrap();
    wb.set_cell("Sheet1", "C1", "=Y").unwrap();
    wb.set_cell("Sheet1", "C2", "=Sheet1!A2").unwrap();
    let plan = wb
        .plan_names(
            &[def("X", None, "Sheet1!$A$3"), def("Y", None, "X+1")],
            true,
        )
        .unwrap();
    assert_eq!(plan.affected.len(), 1);
    wb.commit_names(plan);
    assert_eq!(value(&wb, "Sheet1", "C1"), V::Number(31.0));
}

#[test]
fn definitions_that_lead_back_to_themselves_are_refused_or_read_as_errors() {
    let mut wb = book();
    let cyclic = [
        def("A_", None, "B_+1"),
        def("B_", None, "A_+1"),
        def("S_", None, "S_"),
    ];
    assert!(wb.set_names(&cyclic).is_err());
    assert_eq!(wb.names().len(), 0);
    assert_eq!(wb.set_names_lenient(&cyclic), 0);
    wb.set_cell("Sheet1", "C1", "=A_").unwrap();
    wb.set_cell("Sheet1", "C2", "=S_").unwrap();
    wb.set_cell("Sheet1", "C3", "=B_+1").unwrap();
    for cell in ["C1", "C2", "C3"] {
        assert_eq!(
            value(&wb, "Sheet1", cell),
            V::Error(FormulaError::Name),
            "{cell}"
        );
    }
    let built = crate::names::NameTable::build(
        &cyclic,
        &["Sheet1"],
        false,
        &crate::table_index::TableIndex::default(),
    );
    assert!(built
        .issues
        .iter()
        .all(|&(_, issue)| issue == NameIssue::Cycle));
    assert_eq!(built.issues.len(), 3);
}

#[test]
fn a_chain_deeper_than_the_limit_and_an_exponential_chain_read_value_error() {
    let chain = |n: usize| -> Vec<DefinedName> {
        let mut defs = vec![def("chain_0", None, "1")];
        for i in 1..n {
            defs.push(def(
                &format!("chain_{i}"),
                None,
                &format!("chain_{}+1", i - 1),
            ));
        }
        defs
    };
    let mut wb = book();
    assert!(wb.set_names(&chain(MAX_NAME_DEPTH)).is_ok());
    wb.set_cell("Sheet1", "C1", format!("=chain_{}", MAX_NAME_DEPTH - 1))
        .unwrap();
    assert_eq!(value(&wb, "Sheet1", "C1"), V::Number(MAX_NAME_DEPTH as f64));
    let deeper = chain(MAX_NAME_DEPTH + 1);
    assert!(wb.set_names(&deeper).is_err());
    wb.set_names_lenient(&deeper);
    wb.set_cell("Sheet1", "C2", format!("=chain_{MAX_NAME_DEPTH}"))
        .unwrap();
    assert_eq!(value(&wb, "Sheet1", "C2"), V::Error(FormulaError::Value));
    assert_eq!(value(&wb, "Sheet1", "C1"), V::Number(MAX_NAME_DEPTH as f64));

    let mut blowup = vec![def("ex_0", None, "1")];
    for i in 1..40 {
        blowup.push(def(
            &format!("ex_{i}"),
            None,
            &format!("ex_{}+ex_{}", i - 1, i - 1),
        ));
    }
    assert!(wb.set_names(&blowup).is_err());
    wb.set_names_lenient(&blowup);
    wb.set_cell("Sheet1", "C3", "=ex_39").unwrap();
    wb.set_cell("Sheet1", "C4", "=ex_5").unwrap();
    assert_eq!(value(&wb, "Sheet1", "C3"), V::Error(FormulaError::Value));
    assert_eq!(value(&wb, "Sheet1", "C4"), V::Number(32.0));
}

#[test]
fn a_formula_that_expands_past_the_node_limit_reads_value_error() {
    let mut wb = book();
    let long = vec!["A1"; 1_300].join("+");
    wb.set_names(&[def("Big", None, &long)]).unwrap();
    let uses = ["Big"; MAX_FORMULA_NODES / 2_600 + 1].join("+");
    wb.set_cell("Sheet1", "C1", format!("={uses}")).unwrap();
    assert_eq!(value(&wb, "Sheet1", "C1"), V::Error(FormulaError::Value));
    wb.set_cell("Sheet1", "C2", "=Big").unwrap();
    assert_eq!(value(&wb, "Sheet1", "C2"), V::Number(13_000.0));
}

#[test]
fn names_are_checked_like_the_spreadsheet_programs_check_them() {
    let mut wb = book();
    let bad = [
        def("A1", None, "1"),
        def("R1C1", None, "1"),
        def("two words", None, "1"),
        def("SUM", None, "1"),
        def("sheet1", None, "1"),
        def("Fine", Some("Missing"), "1"),
        def("Fine", None, "SUM("),
        def("Fine", None, ""),
    ];
    for one in &bad {
        assert!(wb.set_names(std::slice::from_ref(one)).is_err(), "{one:?}");
    }
    let twice = [def("Dup", None, "1"), def("DUP", None, "2")];
    assert!(wb.set_names(&twice).is_err());
    let scoped = [def("Dup", None, "1"), def("DUP", Some("Data"), "2")];
    assert!(wb.set_names(&scoped).is_ok());
    let comment = DefinedName {
        comment: "x".repeat(256),
        ..def("Note", None, "1")
    };
    assert!(wb.set_names(&[comment]).is_err());
    let many: Vec<DefinedName> = (0..=MAX_NAMES)
        .map(|i| def(&format!("n_{i}"), None, "1"))
        .collect();
    assert!(wb.set_names(&many).is_err());
    assert_eq!(wb.set_names_lenient(&many), 1);
    assert_eq!(wb.names().len(), MAX_NAMES);
    // A file may name a thing like a function or a sheet; only a typed name is refused for it.
    wb.set_names_lenient(&[def("Sum", None, "1"), def("Data", None, "2")]);
    wb.set_cell("Sheet1", "H1", "=SUM(1,2)+Sum+Data").unwrap();
    assert_eq!(value(&wb, "Sheet1", "H1"), V::Number(6.0));
    assert_eq!(
        wb.check_name(&def("SUM", None, "1"), None),
        Some(NameIssue::Reserved)
    );
    assert_eq!(
        wb.check_name(&def("Sum", None, "5"), Some(("sum", None))),
        Some(NameIssue::Reserved)
    );
    assert_eq!(
        wb.check_name(&def("Fresh", None, "Sheet1!$A$1"), None),
        None
    );
    wb.set_names_lenient(&[def("Dup_x", None, "1")]);
    assert_eq!(
        wb.check_name(&def("DUP_X", None, "5"), None),
        Some(NameIssue::Duplicate)
    );
    assert_eq!(
        wb.check_name(&def("DUP_X", None, "5"), Some(("dup_x", None))),
        None
    );
    assert_eq!(wb.check_name(&def("DUP_X", Some("Data"), "5"), None), None);
}

#[test]
fn inserting_rows_moves_the_names_and_the_formulas_that_use_them() {
    let mut wb = book();
    wb.set_names(&[
        def("Prices", None, "Sheet1!$A$1:$A$3"),
        def("Last", Some("Sheet1"), "$A$3"),
    ])
    .unwrap();
    wb.set_cell("Sheet1", "C1", "=SUM(Prices)").unwrap();
    wb.set_cell("Sheet1", "C2", "=Last").unwrap();
    let edit = AxisEdit::new(Axis::Row, EditKind::Insert, 1, 1).unwrap();
    let report = wb
        .apply_axis_edit("Sheet1", edit, AxisBudget::default())
        .unwrap();
    assert!(report.names_changed);
    assert_eq!(wb.names()[0].refers_to, "Sheet1!$A$1:$A$4");
    assert_eq!(wb.names()[1].refers_to, "$A$4");
    assert_eq!(value(&wb, "Sheet1", "C1"), V::Number(60.0));
    assert_eq!(value(&wb, "Sheet1", "C2"), V::Empty);
    assert_eq!(value(&wb, "Sheet1", "C3"), V::Number(30.0));
    let del = AxisEdit::new(Axis::Row, EditKind::Delete, 0, 4).unwrap();
    wb.apply_axis_edit("Sheet1", del, AxisBudget::default())
        .unwrap();
    assert_eq!(wb.names()[0].refers_to, "#REF!");
    wb.set_cell("Sheet1", "D1", "=SUM(Prices)").unwrap();
    assert_eq!(value(&wb, "Sheet1", "D1"), V::Error(FormulaError::Ref));
}

#[test]
fn a_renamed_or_deleted_sheet_changes_the_names_that_point_at_it() {
    use crate::SheetChange;
    let defs = [
        def("Prices", None, "Data!$A$1:$A$3"),
        def("Local", Some("Data"), "Data!$A$1"),
        def("Other", None, "Sheet1!$A$1"),
    ];
    let renamed = crate::follow_sheet_change_names(
        &defs,
        SheetChange::Rename {
            from: "data",
            to: "My Data",
        },
    )
    .unwrap();
    assert_eq!(renamed[0].refers_to, "'My Data'!$A$1:$A$3");
    assert_eq!(renamed[1].scope.as_deref(), Some("My Data"));
    assert_eq!(renamed[2], defs[2]);
    let deleted =
        crate::follow_sheet_change_names(&defs, SheetChange::Delete { name: "Data" }).unwrap();
    assert_eq!(deleted.len(), 2);
    assert_eq!(deleted[0].refers_to, "#REF!");
    assert!(crate::follow_sheet_change_names(
        &defs[2..],
        SheetChange::Rename {
            from: "Data",
            to: "X"
        }
    )
    .is_none());
}

#[test]
fn sorting_keeps_the_formulas_that_use_names_right() {
    let mut wb = book();
    wb.set_names(&[def("Margin", None, "2")]).unwrap();
    for (row, v) in ["3", "1", "2"].iter().enumerate() {
        wb.set_cell("Sheet1", &format!("E{}", row + 1), *v).unwrap();
        wb.set_cell(
            "Sheet1",
            &format!("F{}", row + 1),
            format!("=E{}*Margin", row + 1),
        )
        .unwrap();
    }
    let sheet = wb.sheet_mut("Sheet1").unwrap();
    sheet
        .sort_rows(0, 2, &[crate::SortKey::asc(4)], false)
        .unwrap();
    assert_eq!(value(&wb, "Sheet1", "F1"), V::Number(2.0));
    assert_eq!(value(&wb, "Sheet1", "F3"), V::Number(6.0));
}

fn doubling(top: usize) -> Vec<DefinedName> {
    let mut defs = vec![def("nm_0", None, "Sheet1!$A$1")];
    for k in 1..=top {
        defs.push(def(
            &format!("nm_{k}"),
            None,
            &format!("nm_{}+nm_{}", k - 1, k - 1),
        ));
    }
    defs
}

#[test]
fn thousands_of_formulas_that_use_a_doubling_chain_share_one_tree_each() {
    let mut wb = book();
    wb.set_names_lenient(&doubling(11));
    assert!(wb.names().len() == 12);
    for i in 0..9_988u32 {
        wb.set_cell("Sheet1", &format!("X{}", i + 1), "=nm_11")
            .unwrap();
    }
    let (in_formulas, in_table) = wb.name_nodes();
    // A formula holds the pointer to the shared tree, not a copy of its 4,095 nodes; the table holds each
    // definition's top layer.
    assert!(in_formulas <= 9_988, "{in_formulas}");
    assert!(in_table <= 12 * 3 + 1, "{in_table}");
    assert_eq!(value(&wb, "Sheet1", "X1"), V::Number(20_480.0));
    assert_eq!(value(&wb, "Sheet1", "X9988"), V::Number(20_480.0));
    // One more doubling is past the bound of a definition.
    wb.set_names_lenient(&doubling(12));
    wb.set_cell("Sheet1", "Y1", "=nm_12").unwrap();
    assert_eq!(value(&wb, "Sheet1", "Y1"), V::Error(FormulaError::Value));
    assert!(wb.set_names(&doubling(12)).is_err());
}

#[test]
fn the_formulas_of_a_workbook_that_copy_a_definition_stop_at_the_bound() {
    let mut wb = book();
    wb.set_names(&[def("Big", None, &vec!["A1"; 500].join("+"))])
        .unwrap();
    let copies = crate::names::MAX_WORKBOOK_NAME_NODES / 999 + 50;
    for i in 0..copies {
        wb.set_cell("Sheet1", &format!("X{}", i + 1), "=SUM(Big)")
            .unwrap();
    }
    let (in_formulas, _) = wb.name_nodes();
    assert!(
        in_formulas <= crate::names::MAX_WORKBOOK_NAME_NODES,
        "{in_formulas}"
    );
    assert_eq!(value(&wb, "Sheet1", "X1"), V::Number(5_000.0));
    assert_eq!(
        value(&wb, "Sheet1", &format!("X{copies}")),
        V::Error(FormulaError::Value)
    );
    // A cell that is overwritten gives its nodes back.
    wb.set_cell("Sheet1", "X1", "1").unwrap();
    let (after, _) = wb.name_nodes();
    assert_eq!(after, in_formulas - 1_000);
    let clone = wb.clone();
    wb.set_cell("Sheet1", "X2", "1").unwrap();
    assert_eq!(
        clone.name_nodes().0,
        after,
        "a copy of the workbook counts for itself"
    );
}

#[test]
fn the_definitions_of_a_workbook_stop_at_the_table_bound() {
    let defs: Vec<DefinedName> = (0..500)
        .map(|i| def(&format!("big_{i}"), None, &vec!["A1"; 1_300].join("+")))
        .collect();
    let mut wb = book();
    assert!(wb.set_names(&defs).is_err());
    wb.set_names_lenient(&defs);
    let (_, table) = wb.name_nodes();
    assert!(table <= crate::names::MAX_TABLE_NODES, "{table}");
    wb.set_cell("Sheet1", "X1", "=big_0").unwrap();
    wb.set_cell("Sheet1", "X2", "=big_499").unwrap();
    assert_eq!(value(&wb, "Sheet1", "X1"), V::Number(13_000.0));
    assert_eq!(value(&wb, "Sheet1", "X2"), V::Error(FormulaError::Value));
}

const SHAPE_FUNCTIONS: [&str; 24] = [
    "SUM({n})",
    "ROWS({n})",
    "COLUMNS({n})",
    "COUNT({n})",
    "COUNTA({n})",
    "COUNTBLANK({n})",
    "AVERAGE({n})",
    "MIN({n})",
    "MAX({n})",
    "SMALL({n},2)",
    "LARGE({n},1)",
    "STDEV({n})",
    "PRODUCT({n})",
    "SUMSQ({n})",
    "COUNTIF({n},2)",
    "SUMIF({n},\">1\")",
    "MATCH(2,{n},0)",
    "VLOOKUP(2,{n},1,FALSE)",
    "ROW({n})",
    "COLUMN({n})",
    "RANK(2,{n})",
    "INDEX({n},1)",
    "SUMPRODUCT({n},{n})",
    "MEDIAN({n})",
];

/// What a name stands for must not change how a function reads its argument: for every definition that is a
/// reference or a call that yields one, an alias of it (one step and two steps) gives what the same function
/// gives with the definition written out.
#[test]
fn an_alias_of_a_name_is_read_by_the_functions_like_the_name_and_like_the_definition_itself() {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");
    for r in 1..=5u32 {
        wb.set_cell("Sheet1", &format!("A{r}"), r.to_string())
            .unwrap();
        wb.set_cell("Sheet1", &format!("B{r}"), (r * 10).to_string())
            .unwrap();
    }
    let bases = [
        ("off", "OFFSET(Sheet1!$A$1,0,0,3,1)"),
        ("tr", "TRANSPOSE(Sheet1!$A$1:$B$2)"),
        ("mm", "MMULT(Sheet1!$A$1:$B$2,Sheet1!$A$1:$B$2)"),
        ("rng", "Sheet1!$A$1:$B$3"),
    ];
    let mut defs = Vec::new();
    for (base, text) in bases {
        defs.push(def(&format!("{base}_0"), None, text));
        defs.push(def(&format!("{base}_1"), None, &format!("{base}_0")));
        defs.push(def(&format!("{base}_2"), None, &format!("{base}_1")));
    }
    wb.set_names(&defs).unwrap();
    let mut checked = 0;
    for (base, text) in bases {
        for template in SHAPE_FUNCTIONS {
            let oracle_cell = format!("Z{}", 1 + checked);
            wb.set_cell(
                "Sheet1",
                &oracle_cell,
                format!("={}", template.replace("{n}", text)),
            )
            .unwrap();
            let want = value(&wb, "Sheet1", &oracle_cell);
            for depth in 0..=2 {
                let cell = format!("Y{}", 1 + checked * 3 + depth);
                let formula = template.replace("{n}", &format!("{base}_{depth}"));
                wb.set_cell("Sheet1", &cell, format!("={formula}")).unwrap();
                assert_eq!(
                    value(&wb, "Sheet1", &cell),
                    want,
                    "{formula} against {}",
                    template.replace("{n}", text)
                );
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 4 * 24);
    // The values the review found wrong, spelled out.
    wb.set_cell("Sheet1", "W1", "=SUM(off_2)").unwrap();
    wb.set_cell("Sheet1", "W2", "=SUM(tr_2)").unwrap();
    wb.set_cell("Sheet1", "W3", "=ROWS(tr_2)").unwrap();
    wb.set_cell("Sheet1", "W4", "=SUM(mm_2)").unwrap();
    assert_eq!(value(&wb, "Sheet1", "W1"), V::Number(6.0));
    assert_eq!(value(&wb, "Sheet1", "W2"), V::Number(33.0));
    assert_eq!(value(&wb, "Sheet1", "W3"), V::Number(2.0));
    assert_eq!(
        value(&wb, "Sheet1", "W4"),
        V::Number(693.0),
        "the sum of [[1,10],[2,20]] squared"
    );
}

#[test]
fn recompiling_existing_cell_does_not_falsely_exhaust_name_budget() {
    let mut wb = book();
    for r in 1..=5u32 {
        wb.set_cell("Sheet1", &format!("A{r}"), "10").unwrap();
    }
    wb.set_names(&[def("Big", None, &vec!["A1"; 500].join("+"))])
        .unwrap();
    // 2_097 * 1_000 = 2_097_000 <= MAX_WORKBOOK_NAME_NODES (2_097_152), leaving 152 nodes free.
    let fill = crate::names::MAX_WORKBOOK_NAME_NODES / 1_000;
    for i in 1..=fill {
        wb.set_cell("Sheet1", &format!("X{i}"), "=SUM(Big)")
            .unwrap();
    }
    let used_before = wb.name_nodes().0;
    assert_eq!(used_before, fill * 1_000);
    assert_eq!(value(&wb, "Sheet1", "X1"), V::Number(5_000.0));

    // Repeatedly overwriting / recompiling X1 must reuse X1's old budget instead of double-counting.
    for _ in 0..10 {
        wb.set_cell("Sheet1", "X1", "=SUM(Big)").unwrap();
        assert_eq!(value(&wb, "Sheet1", "X1"), V::Number(5_000.0));
        assert_eq!(wb.name_nodes().0, used_before);
    }

    // Recompiling via set_names on Big must also keep X1 valid.
    wb.set_names(&[def("Big", None, &vec!["A2"; 500].join("+"))])
        .unwrap();
    assert_eq!(value(&wb, "Sheet1", "X1"), V::Number(5_000.0));
    assert_eq!(wb.name_nodes().0, used_before);

    // A genuinely new concurrent cell exceeding the budget must still be blocked fail-closed.
    wb.set_cell("Sheet1", "Z1", "=SUM(Big)").unwrap();
    assert_eq!(value(&wb, "Sheet1", "Z1"), V::Error(FormulaError::Value));
}

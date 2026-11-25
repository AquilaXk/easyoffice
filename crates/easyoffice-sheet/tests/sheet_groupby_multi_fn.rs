//! Multi-aggregation function vectors (`HSTACK`/`VSTACK`), eta-reduced built-ins, omitted axes
//! in `PIVOTBY`, and `[MS-XLSX]` `_xleta.` round-tripping in `easyoffice-sheet`.
#![allow(clippy::unwrap_used)]

mod dynamic_support;
use dynamic_support::{new_empty_model, DefinedNameEntry};
use easyoffice_formats::{
    from_file_formula, to_file_formula, DefinedName, OoxmlCodec, SheetStyling,
};
use easyoffice_sheet::{PackedCellCoordinate, RecalcSession, WorkbookEngine};

const SALES: [(&str, &str, &str, &str); 6] = [
    ("Region", "Product", "Sales", "Units"),
    ("East", "Apple", "10", "1"),
    ("East", "Pear", "20", "2"),
    ("West", "Apple", "5", "3"),
    ("West", "Apple", "7", "1"),
    ("West", "Pear", "1", "3"),
];

fn setup_sales() -> dynamic_support::Model {
    let mut model = new_empty_model();
    for (i, (a, b, c, d)) in SALES.iter().enumerate() {
        model._set(&format!("A{}", i + 1), a);
        model._set(&format!("B{}", i + 1), b);
        model._set(&format!("C{}", i + 1), c);
        model._set(&format!("D{}", i + 1), d);
    }
    model
}

fn eval_grid(formula: &str, rows: usize, cols: usize) -> Vec<Vec<String>> {
    let mut model = setup_sales();
    model._set("H1", formula);
    model.evaluate();
    read_block(&model, 'H', 1, rows, cols)
}

fn read_block(
    model: &dynamic_support::Model,
    start_col: char,
    start_row: usize,
    rows: usize,
    cols: usize,
) -> Vec<Vec<String>> {
    (0..rows)
        .map(|r| {
            (0..cols)
                .map(|c| {
                    let col = (start_col as u8 + c as u8) as char;
                    model._get_text(&format!("{col}{}", start_row + r))
                })
                .collect()
        })
        .collect()
}

fn rows(text: &[&[&str]]) -> Vec<Vec<String>> {
    text.iter()
        .map(|r| r.iter().map(|c| c.to_string()).collect())
        .collect()
}

#[test]
fn groupby_horizontal_function_vectors_single_and_multi_value_cols() {
    assert_eq!(
        eval_grid(
            "=GROUPBY(A1:A6, C1:C6, HSTACK(COUNT, SUM, PERCENTOF), 3)",
            4,
            4
        ),
        rows(&[
            &["Region", "COUNT", "SUM", "PERCENTOF"],
            &["East", "2", "30", "0.697674418604651"],
            &["West", "3", "13", "0.302325581395349"],
            &["Total", "5", "43", "1"],
        ])
    );

    // headers = 1 hides row_fields header cell (blank), keeping 1 function header row so DROP(, 1) drops cleanly
    assert_eq!(
        eval_grid("=GROUPBY(A1:A6, C1:C6, HSTACK(COUNT, SUM), 1)", 4, 3),
        rows(&[
            &["", "COUNT", "SUM"],
            &["East", "2", "30"],
            &["West", "3", "13"],
            &["Total", "5", "43"],
        ])
    );
    assert_eq!(
        eval_grid(
            "=DROP(GROUPBY(A1:A6, C1:C6, HSTACK(COUNT, SUM), 1), 1)",
            3,
            3
        ),
        rows(&[
            &["East", "2", "30"],
            &["West", "3", "13"],
            &["Total", "5", "43"],
        ])
    );

    // Custom header labels via 2 x F VSTACK(HSTACK(...), {...})
    assert_eq!(
        eval_grid(
            "=GROUPBY(A1:A6, C1:C6, VSTACK(HSTACK(COUNT, SUM, PERCENTOF), {\"Count\", \"Cost\", \"%\"}), 3)",
            4,
            4
        ),
        rows(&[
            &["Region", "Count", "Cost", "%"],
            &["East", "2", "30", "0.697674418604651"],
            &["West", "3", "13", "0.302325581395349"],
            &["Total", "5", "43", "1"],
        ])
    );

    // Multiple value columns (m = 2) output function header row + value header row
    assert_eq!(
        eval_grid("=GROUPBY(A1:A6, C1:D6, HSTACK(SUM, AVERAGE), 3, 0)", 4, 5),
        rows(&[
            &["", "SUM", "SUM", "AVERAGE", "AVERAGE"],
            &["Region", "Sales", "Units", "Sales", "Units"],
            &["East", "30", "3", "15", "1.5"],
            &["West", "13", "7", "4.33333333333333", "2.33333333333333"],
        ])
    );
    assert_eq!(
        eval_grid("=GROUPBY(A1:A6, C1:D6, HSTACK(SUM, AVERAGE), 1, 0)", 4, 5),
        rows(&[
            &["", "SUM", "SUM", "AVERAGE", "AVERAGE"],
            &["", "Sales", "Units", "Sales", "Units"],
            &["East", "30", "3", "15", "1.5"],
            &["West", "13", "7", "4.33333333333333", "2.33333333333333"],
        ])
    );
}

#[test]
fn groupby_custom_single_lambda_label_pairs_and_explicit_lambda_labels() {
    assert_eq!(
        eval_grid(
            "=GROUPBY(A1:A6, C1:C6, HSTACK(SUM, \"TotalCost\"), 3, 0)",
            3,
            2
        ),
        rows(&[&["Region", "TotalCost"], &["East", "30"], &["West", "13"],])
    );
    assert_eq!(
        eval_grid(
            "=GROUPBY(A1:A6, C1:C6, VSTACK(SUM, \"TotalCost\"), 3, 0)",
            3,
            2
        ),
        rows(&[&["Region", "TotalCost"], &["East", "30"], &["West", "13"],])
    );
    // Explicit LAMBDA defaults to "LAMBDA" and receives [group, total] when 2 params
    assert_eq!(
        eval_grid(
            "=GROUPBY(A1:A6, C1:C6, HSTACK(SUM, LAMBDA(g, t, SUM(g)/COUNT(t))), 3, 0)",
            3,
            3
        ),
        rows(&[
            &["Region", "SUM", "LAMBDA"],
            &["East", "30", "6"],
            &["West", "13", "2.6"],
        ])
    );
}

#[test]
fn groupby_vertical_function_vectors_normal_table_and_let() {
    // F x 1 vertical with Grand total (labels on sub_row == 0 only)
    assert_eq!(
        eval_grid("=GROUPBY(A1:A6, C1:C6, VSTACK(SUM, COUNT), 3)", 7, 3),
        rows(&[
            &["Region", "Function", "Sales"],
            &["East", "SUM", "30"],
            &["", "COUNT", "2"],
            &["West", "SUM", "13"],
            &["", "COUNT", "3"],
            &["Total", "SUM", "43"],
            &["", "COUNT", "5"],
        ])
    );

    // Table mode (field_relationship = 1): Leaf repeats row labels on all F sub-rows; Grand stays sub_row == 0
    assert_eq!(
        eval_grid(
            "=GROUPBY(A1:B6, C1:C6, HSTACK(VSTACK(SUM, AVERAGE), {\"Total\"; \"Mean\"}), 3, 1, , , 1)",
            10,
            4
        ),
        rows(&[
            &["Region", "Product", "Function", "Sales"],
            &["East", "Apple", "Total", "10"],
            &["East", "Apple", "Mean", "10"],
            &["East", "Pear", "Total", "20"],
            &["East", "Pear", "Mean", "20"],
            &["West", "Apple", "Total", "12"],
            &["West", "Apple", "Mean", "6"],
            &["West", "Pear", "Total", "1"],
            &["West", "Pear", "Mean", "1"],
            &["Total", "", "Total", "43"],
        ])
    );
    assert_eq!(
        eval_grid(
            "=GROUPBY(A1:B6, C1:C6, HSTACK(VSTACK(SUM, AVERAGE), {\"Total\"; \"Mean\"}), 3, 1, , , 1)",
            11,
            4
        )[10],
        vec!["", "", "Mean", "8.6"]
    );

    // Stored in LET
    assert_eq!(
        eval_grid(
            "=LET(fns, HSTACK(SUM, AVERAGE), GROUPBY(A1:A6, C1:C6, fns, 3, 0))",
            3,
            3
        ),
        rows(&[
            &["Region", "SUM", "AVERAGE"],
            &["East", "30", "15"],
            &["West", "13", "4.33333333333333"],
        ])
    );

    // Vertical with 2 row fields, subtotals (total_depth = 2) and negative total_depth = -1
    assert_eq!(
        eval_grid("=GROUPBY(A1:B6, C1:C6, VSTACK(SUM, COUNT), 3, 2)", 15, 4)[11..15].to_vec(),
        rows(&[
            &["West Total", "", "SUM", "13"],
            &["", "", "COUNT", "3"],
            &["Total", "", "SUM", "43"],
            &["", "", "COUNT", "5"],
        ])
    );
    assert_eq!(
        eval_grid("=GROUPBY(A1:A6, C1:C6, VSTACK(SUM, COUNT), 3, -1)", 3, 3),
        rows(&[
            &["Region", "Function", "Sales"],
            &["Total", "SUM", "43"],
            &["", "COUNT", "5"],
        ])
    );

    // Short-side label vector pads missing label cell with #N/A cleanly
    assert_eq!(
        eval_grid(
            "=GROUPBY(A1:A6, C1:C6, VSTACK(HSTACK(COUNT, SUM), {\"Count\"}), 3, 0)",
            3,
            3
        ),
        rows(&[
            &["Region", "Count", "#N/A"],
            &["East", "2", "30"],
            &["West", "3", "13"],
        ])
    );
}

#[test]
fn pivotby_multi_functions_sorts_relative_to_and_omitted_axes() {
    // PIVOTBY horizontal with sort_order spanning kr + m * F and default relative_to = 0 (col total)
    // col_sort = 3 (ascending PERCENTOF: Pear 21/43 < Apple 22/43)
    assert_eq!(
        eval_grid(
            "=PIVOTBY(A1:A6, B1:B6, C1:C6, HSTACK(SUM, PERCENTOF), 3, 0, -2, 0, 3)",
            4,
            5
        ),
        rows(&[
            &["Product", "Pear", "Pear", "Apple", "Apple"],
            &["Region", "SUM", "PERCENTOF", "SUM", "PERCENTOF"],
            &["East", "20", "0.952380952380952", "10", "0.454545454545455"],
            &["West", "1", "0.0476190476190476", "12", "0.545454545454545"],
        ])
    );

    // PIVOTBY with relative_to = 2 (grand total) and descending col_sort = -3 (Apple > Pear)
    assert_eq!(
        eval_grid(
            "=PIVOTBY(A1:A6, B1:B6, C1:C6, HSTACK(SUM, PERCENTOF), 3, 0, -2, 0, -3, , 2)",
            4,
            5
        ),
        rows(&[
            &["Product", "Apple", "Apple", "Pear", "Pear"],
            &["Region", "SUM", "PERCENTOF", "SUM", "PERCENTOF"],
            &["East", "10", "0.232558139534884", "20", "0.465116279069767"],
            &["West", "12", "0.27906976744186", "1", "0.0232558139534884"],
        ])
    );

    // PIVOTBY with m = 2 and HSTACK(SUM, COUNT): col header row + fn label row + value field row (top = 3)
    assert_eq!(
        eval_grid(
            "=PIVOTBY(A1:A6, B1:B6, C1:D6, HSTACK(SUM, COUNT), 3, 0, , 0)",
            5,
            9
        ),
        rows(&[
            &["Product", "Apple", "Apple", "Apple", "Apple", "Pear", "Pear", "Pear", "Pear"],
            &["", "SUM", "SUM", "COUNT", "COUNT", "SUM", "SUM", "COUNT", "COUNT"],
            &["Region", "Sales", "Units", "Sales", "Units", "Sales", "Units", "Sales", "Units"],
            &["East", "10", "1", "1", "1", "20", "2", "1", "1"],
            &["West", "12", "4", "2", "2", "1", "3", "1", "1"],
        ])
    );

    // PIVOTBY with VSTACK(SUM, AVERAGE): adds Function col at kr
    assert_eq!(
        eval_grid(
            "=PIVOTBY(A1:A6, B1:B6, C1:C6, VSTACK(SUM, COUNT), 3, 0, , 0)",
            6,
            4
        ),
        rows(&[
            &["", "Product", "Apple", "Pear"],
            &["Region", "Function", "Sales", "Sales"],
            &["East", "SUM", "10", "20"],
            &["", "COUNT", "1", "1"],
            &["West", "SUM", "12", "1"],
            &["", "COUNT", "2", "1"],
        ])
    );

    // PIVOTBY with relative_to = 1 (row total)
    assert_eq!(
        eval_grid(
            "=PIVOTBY(A1:A6, B1:B6, C1:C6, HSTACK(SUM, PERCENTOF), 3, 0, , 0, , , 1)",
            4,
            5
        ),
        rows(&[
            &["Product", "Apple", "Apple", "Pear", "Pear"],
            &["Region", "SUM", "PERCENTOF", "SUM", "PERCENTOF"],
            &["East", "10", "0.333333333333333", "20", "0.666666666666667"],
            &["West", "12", "0.923076923076923", "1", "0.0769230769230769"],
        ])
    );

    // PIVOTBY omitted col_fields
    assert_eq!(
        eval_grid("=PIVOTBY(A1:A6, , C1:C6, HSTACK(SUM, AVERAGE), 3)", 4, 3),
        rows(&[
            &["Region", "SUM", "AVERAGE"],
            &["East", "30", "15"],
            &["West", "13", "4.33333333333333"],
            &["Total", "43", "8.6"],
        ])
    );

    // PIVOTBY omitted row_fields (single & horizontal & vertical)
    assert_eq!(
        eval_grid("=PIVOTBY(, B1:B6, C1:C6, SUM, 3)", 3, 3),
        rows(&[
            &["Apple", "Pear", "Total"],
            &["Sales", "Sales", "Sales"],
            &["22", "21", "43"],
        ])
    );
    assert_eq!(
        eval_grid("=PIVOTBY(, B1:B6, C1:C6, HSTACK(SUM, COUNT), 3)", 3, 6),
        rows(&[
            &["Apple", "Apple", "Pear", "Pear", "Total", "Total"],
            &["SUM", "COUNT", "SUM", "COUNT", "SUM", "COUNT"],
            &["22", "3", "21", "2", "43", "5"],
        ])
    );
    assert_eq!(
        eval_grid("=PIVOTBY(, B1:B6, C1:C6, VSTACK(SUM, COUNT), 3)", 4, 4),
        rows(&[
            &["Product", "Apple", "Pear", "Total"],
            &["Function", "Sales", "Sales", "Sales"],
            &["SUM", "22", "21", "43"],
            &["COUNT", "3", "2", "5"],
        ])
    );

    // Both axes omitted in PIVOTBY or omitted row_fields in GROUPBY -> #VALUE!
    assert_eq!(eval_grid("=PIVOTBY(, , C1:C6, SUM)", 1, 1)[0][0], "#VALUE!");
    assert_eq!(eval_grid("=GROUPBY(, C1:C6, SUM)", 1, 1)[0][0], "#VALUE!");
}

#[test]
fn eta_reduced_builtins_work_in_lambda_combinators() {
    let mut model = new_empty_model();
    model._set("A1", "1");
    model._set("B1", "2");
    model._set("A2", "3");
    model._set("B2", "4");

    model._set("D1", "=BYROW(A1:B2, SUM)");
    model._set("E1", "=BYCOL(A1:B2, AVERAGE)");
    model._set("G1", "=MAP({1,2}, {10,20}, SUM)");
    model._set("G3", "=REDUCE(0, {1,2,3,4}, SUM)");
    model._set("H3", "=REDUCE({5,2,9}, MIN)");
    model._set("I1", "=SCAN(0, {1,2,3}, SUM)");
    model._set("I3", "=SCAN({2,5,3}, MAX)");
    model._set("L1", "=MAKEARRAY(2, 2, MAX)");
    model.evaluate();

    assert_eq!(read_block(&model, 'D', 1, 2, 1), rows(&[&["3"], &["7"]]));
    assert_eq!(read_block(&model, 'E', 1, 1, 2), rows(&[&["2", "3"]]));
    assert_eq!(read_block(&model, 'G', 1, 1, 2), rows(&[&["11", "22"]]));
    assert_eq!(model._get_text("G3"), "10");
    assert_eq!(model._get_text("H3"), "2");
    assert_eq!(read_block(&model, 'I', 1, 1, 3), rows(&[&["1", "3", "6"]]));
    assert_eq!(read_block(&model, 'I', 3, 1, 3), rows(&[&["2", "5", "5"]]));
    assert_eq!(
        read_block(&model, 'L', 1, 2, 2),
        rows(&[&["1", "2"], &["2", "2"]])
    );
}

#[test]
fn defined_names_and_let_bindings_take_precedence_over_eta_reduction() {
    let mut model = setup_sales();
    // Lenient defined name overriding SUM -> COUNT
    model
        .session()
        .set_names_lenient(&[
            DefinedName {
                name: "SUM".to_string(),
                scope: None,
                refers_to: "=LAMBDA(x, COUNT(x))".to_string(),
                comment: String::new(),
                hidden: false,
            },
            DefinedName {
                name: "AVERAGE".to_string(),
                scope: None,
                refers_to: "=42".to_string(),
                comment: String::new(),
                hidden: false,
            },
            DefinedName {
                name: "MYFNS".to_string(),
                scope: None,
                refers_to: "=HSTACK(COUNT, MAX)".to_string(),
                comment: String::new(),
                hidden: false,
            },
        ])
        .unwrap();
    model._set("H1", "=GROUPBY(A1:A6, C1:C6, SUM, 3, 0)");
    model._set("K1", "=GROUPBY(A1:A6, C1:C6, HSTACK(SUM, COUNT), 3, 0)");
    model._set("O1", "=GROUPBY(A1:A6, C1:C6, AVERAGE, 3, 0)");
    model._set("P1", "=GROUPBY(A1:A6, C1:C6, MYFNS, 3, 0)");
    model._set("T1", "=MAP({1,2}, SUM)");
    model.evaluate();

    // SUM resolved to LAMBDA(x, COUNT(x)) -> label "LAMBDA" and counts 2, 3
    assert_eq!(
        read_block(&model, 'H', 1, 3, 2),
        rows(&[&["Region", "Sales"], &["East", "2"], &["West", "3"]])
    );
    assert_eq!(
        read_block(&model, 'K', 1, 3, 3),
        rows(&[
            &["Region", "LAMBDA", "COUNT"],
            &["East", "2", "2"],
            &["West", "3", "3"],
        ])
    );
    assert_eq!(model._get_text("O1"), "#VALUE!");
    assert_eq!(
        read_block(&model, 'P', 1, 3, 3),
        rows(&[
            &["Region", "COUNT", "MAX"],
            &["East", "2", "20"],
            &["West", "3", "7"],
        ])
    );
    // MAP({1,2}, SUM) uses the defined name SUM=LAMBDA(x, COUNT(x)), returning {1,1}
    assert_eq!(read_block(&model, 'T', 1, 1, 2), rows(&[&["1", "1"]]));

    // LET binding shadows eta built-in names
    let mut let_model = setup_sales();
    let_model
        .new_defined_name(DefinedNameEntry::new("BONUS", "=10"))
        .unwrap();
    let_model._set("H1", "=LET(SUM, 10, GROUPBY(A1:A6, C1:C6, SUM))");
    let_model._set(
        "I1",
        "=LET(SUM, LAMBDA(x, AVERAGE(x)), GROUPBY(A1:A6, C1:C6, SUM, 3, 0))",
    );
    let_model.evaluate();
    assert_eq!(let_model._get_text("H1"), "#VALUE!");
    assert_eq!(
        read_block(&let_model, 'I', 1, 3, 2),
        rows(&[
            &["Region", "Sales"],
            &["East", "15"],
            &["West", "4.33333333333333"],
        ])
    );
}

#[test]
fn invalid_func_vectors_top_level_calc_and_hostile_budget_fail_closed() {
    // Top-level HSTACK(SUM, AVERAGE) -> #CALC!
    assert_eq!(eval_grid("=HSTACK(SUM, AVERAGE)", 1, 1)[0][0], "#CALC!");
    assert_eq!(eval_grid("=VSTACK(SUM, \"Total\")", 1, 1)[0][0], "#CALC!");

    // Invalid shapes, non-lambda slots, >64 functions, out-of-range sort, unknown name, error propagation
    assert_eq!(
        eval_grid("=GROUPBY(A1:A6, C1:C6, HSTACK(1, SUM))", 1, 1)[0][0],
        "#VALUE!"
    );
    assert_eq!(
        eval_grid("=GROUPBY(A1:A6, C1:C6, VSTACK(SUM, 1, AVERAGE))", 1, 1)[0][0],
        "#VALUE!"
    );
    assert_eq!(
        eval_grid(
            "=GROUPBY(A1:A6, C1:C6, VSTACK(HSTACK(SUM, COUNT), HSTACK(AVERAGE, MAX)))",
            1,
            1
        )[0][0],
        "#VALUE!"
    );
    assert_eq!(
        eval_grid("=GROUPBY(A1:A6, C1:C6, 1)", 1, 1)[0][0],
        "#VALUE!"
    );
    assert_eq!(
        eval_grid("=GROUPBY(A1:A6, C1:C6, UNKNOWN_FUNC)", 1, 1)[0][0],
        "#NAME?"
    );
    assert_eq!(
        eval_grid("=GROUPBY(A1:A6, C1:C6, HSTACK(UNKNOWN_FUNC, SUM))", 1, 1)[0][0],
        "#NAME?"
    );
    assert_eq!(
        eval_grid("=GROUPBY(A1:A6, C1:C6, HSTACK(1/0, SUM))", 1, 1)[0][0],
        "#DIV/0!"
    );
    assert_eq!(
        eval_grid("=GROUPBY(A1:A6, C1:C6, HSTACK(SUM, COUNT), 3, 0, 4)", 1, 1)[0][0],
        "#VALUE!"
    );

    // More than MAX_VALUE_COLUMNS (65 > 64) rejected with #VALUE!
    let list_65 = vec!["SUM"; 65].join(", ");
    assert_eq!(
        eval_grid(&format!("=GROUPBY(A1:A6, C1:C6, HSTACK({list_65}))"), 1, 1)[0][0],
        "#VALUE!"
    );

    // Hostile giant HSTACK with large array exceeds MAX_ARRAY_CELLS -> #SPILL!
    assert_eq!(
        eval_grid("=HSTACK(SUM, MAKEARRAY(500000, 10, LAMBDA(r, c, r)))", 1, 1)[0][0],
        "#SPILL!"
    );
}

#[test]
fn xlsx_xleta_roundtrip_evaluates_identically_after_reopen() {
    let f1 = "=GROUPBY(A1:A6, C1:C6, VSTACK(HSTACK(COUNT, SUM, PERCENTOF), {\"Count\", \"Cost\", \"%\"}), 3)";
    let f2 = "=PIVOTBY(, B1:B6, C1:C6, HSTACK(SUM, AVERAGE), 3)";
    let f3 = "=BYROW(C2:D6, SUM)";
    let file_f1 = format!("={}", to_file_formula(&f1[1..]));
    assert!(file_f1.contains("_xleta.COUNT") && file_f1.contains("_xleta.PERCENTOF"));

    let mut cells: Vec<(u32, u32, String)> = SALES
        .iter()
        .enumerate()
        .flat_map(|(r, (a, b, c, d))| {
            let r = r as u32;
            [
                (r, 0, (*a).to_string()),
                (r, 1, (*b).to_string()),
                (r, 2, (*c).to_string()),
                (r, 3, (*d).to_string()),
            ]
        })
        .collect();
    cells.push((0, 7, f1.to_string()));
    cells.push((5, 7, f2.to_string()));
    cells.push((9, 7, f3.to_string()));
    let packed = OoxmlCodec::pack_xlsx_multi_styled(
        &[("Sheet1".to_string(), cells)],
        &[],
        &[SheetStyling::default()],
    )
    .unwrap();
    let (unpacked, _) = OoxmlCodec::unpack_xlsx_with_styling(&packed).unwrap();

    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");
    let mut session = RecalcSession::new(wb).unwrap();
    for (r, c, val) in &unpacked[0].1 {
        let coord = PackedCellCoordinate::new(*r, *c).unwrap();
        session.set_cell("Sheet1", coord, val).unwrap();
    }
    // Also set an external formula written directly with `_xleta._xlfn.PERCENTOF`
    let ext = format!(
        "={}",
        from_file_formula(
            "_xlfn.GROUPBY(A1:A6, C1:C6, _xlfn.HSTACK(_xleta.SUM, _xleta._xlfn.PERCENTOF), 3)"
        )
    );
    session
        .set_cell("Sheet1", PackedCellCoordinate::new(15, 7).unwrap(), &ext)
        .unwrap();
    let _ = session.recalculate();

    let at = |r: u32, c: u32| {
        session
            .cell_value("Sheet1", PackedCellCoordinate::new(r, c).unwrap())
            .to_display_string()
    };
    assert_eq!(at(0, 8), "Count");
    assert_eq!(at(0, 9), "Cost");
    assert_eq!(at(0, 10), "%");
    assert_eq!(at(1, 8), "2");
    assert_eq!(at(1, 9), "30");
    assert_eq!(at(3, 10), "1");
    assert_eq!(at(5, 7), "Apple");
    assert_eq!(at(6, 7), "SUM");
    assert_eq!(at(7, 7), "22");
    assert_eq!(at(9, 7), "11");
    assert_eq!(at(15, 9), "PERCENTOF");
    assert_eq!(at(18, 9), "1");
}

#[test]
fn canonical_functions_match_xleta_roundtrip_table() {
    for &fn_name in &easyoffice_sheet::CANONICAL_FUNCTIONS {
        if fn_name == "TRUE" || fn_name == "FALSE" {
            continue;
        }
        let page = format!("BYROW(A1:A3, {fn_name})");
        let file = to_file_formula(&page);
        assert_eq!(
            file,
            format!("_xlfn.BYROW(A1:A3, _xleta.{fn_name})"),
            "{fn_name}"
        );
        assert_eq!(from_file_formula(&file), page, "{fn_name}");
    }
}

#[test]
fn let_and_if_eta_builtins_adapt_arity_and_top_level_lifted_branches_calc() {
    let mut model = new_empty_model();
    model._set("A1", "1");
    model._set("B1", "2");
    model._set("A2", "3");
    model._set("B2", "4");

    model._set("D1", "=LET(f, SUM, MAP({1,2}, {10,20}, f))");
    model._set("D2", "=LET(f, MAX, REDUCE(0, {1,2,3}, f))");
    model._set("D3", "=LET(f, SUM, SCAN(0, {1,2,3}, f))");
    model._set("D4", "=LET(f, MAX, MAKEARRAY(2, 2, f))");
    model._set("D6", "=MAP({1,2}, {10,20}, IF(TRUE, SUM, AVERAGE))");
    model._set("D7", "=LET(f, SUM, f(1, 2, 3))");
    model._set("D8", "=LET(f, PERCENTOF, MAP({1,2}, {10,20}, f))");

    model._set("G1", "=BYROW(A1:B2, PERCENTOF)");
    model._set("H1", "=LET(f, PERCENTOF, BYROW(A1:B2, f))");
    model._set("I1", "=IF(TRUE, SUM, AVERAGE)");
    model._set("I2", "=CHOOSE(1, SUM, AVERAGE)");
    model._set("I3", "=IFS(TRUE, SUM)");
    model._set("I4", "=SWITCH(1, 1, SUM)");
    model._set("I5", "=SUM");
    model._set("I6", "=SUM+1");
    model.evaluate();

    assert_eq!(read_block(&model, 'D', 1, 1, 2), rows(&[&["11", "22"]]));
    assert_eq!(model._get_text("D2"), "3");
    assert_eq!(read_block(&model, 'D', 3, 1, 3), rows(&[&["1", "3", "6"]]));
    assert_eq!(
        read_block(&model, 'D', 4, 2, 2),
        rows(&[&["1", "2"], &["2", "2"]])
    );
    assert_eq!(read_block(&model, 'D', 6, 1, 2), rows(&[&["11", "22"]]));
    assert_eq!(model._get_text("D7"), "6");
    assert_eq!(read_block(&model, 'D', 8, 1, 2), rows(&[&["0.1", "0.1"]]));

    assert_eq!(
        read_block(&model, 'G', 1, 2, 1),
        rows(&[&["#VALUE!"], &["#VALUE!"]])
    );
    assert_eq!(
        read_block(&model, 'H', 1, 2, 1),
        rows(&[&["#VALUE!"], &["#VALUE!"]])
    );
    for cell in ["I1", "I2", "I3", "I4", "I5"] {
        assert_eq!(model._get_text(cell), "#CALC!", "{cell}");
    }
    assert_eq!(model._get_text("I6"), "#VALUE!");
}

#[test]
fn groupby_pivotby_omitted_values_multi_val_cols_sorts_and_optional_total_lambda() {
    // Omitted required values argument on a 1-row axis must fail with #VALUE!
    assert_eq!(eval_grid("=GROUPBY(A1, , SUM)", 1, 1)[0][0], "#VALUE!");
    assert_eq!(eval_grid("=PIVOTBY(A1, B1, , SUM)", 1, 1)[0][0], "#VALUE!");
    assert_eq!(
        eval_grid("=DROP(HSTACK(SUM, AVERAGE), 1)", 1, 1)[0][0],
        "#VALUE!"
    );

    // Omitted row_fields with multi-value columns (m = 2) and horizontal HSTACK(SUM, COUNT)
    assert_eq!(
        eval_grid("=PIVOTBY(, B1:B6, C1:D6, HSTACK(SUM, COUNT), 3)", 4, 12),
        rows(&[
            &[
                "Apple", "Apple", "Apple", "Apple", "Pear", "Pear", "Pear", "Pear", "Total",
                "Total", "Total", "Total"
            ],
            &[
                "SUM", "SUM", "COUNT", "COUNT", "SUM", "SUM", "COUNT", "COUNT", "SUM", "SUM",
                "COUNT", "COUNT"
            ],
            &[
                "Sales", "Units", "Sales", "Units", "Sales", "Units", "Sales", "Units", "Sales",
                "Units", "Sales", "Units"
            ],
            &["22", "5", "3", "3", "21", "5", "2", "2", "43", "10", "5", "5"],
        ])
    );

    // Omitted col_fields with multi-value columns (m = 2) and vertical VSTACK(SUM, COUNT)
    assert_eq!(
        eval_grid("=PIVOTBY(A1:A6, , C1:D6, VSTACK(SUM, COUNT), 3, 0)", 5, 4),
        rows(&[
            &["Region", "Function", "Sales", "Units"],
            &["East", "SUM", "30", "3"],
            &["", "COUNT", "2", "2"],
            &["West", "SUM", "13", "7"],
            &["", "COUNT", "3", "3"],
        ])
    );

    // Omitted row_fields with col_sort = -2 (SUM descending: Apple 22 > Pear 21)
    assert_eq!(
        eval_grid(
            "=PIVOTBY(, B1:B6, C1:C6, HSTACK(SUM, COUNT), 3, , , 0, -2)",
            3,
            4
        ),
        rows(&[
            &["Apple", "Apple", "Pear", "Pear"],
            &["SUM", "COUNT", "SUM", "COUNT"],
            &["22", "3", "21", "2"],
        ])
    );

    // row_sort = -5 on m = 2, F = 2 targets (f = 1 COUNT, j = 1 Units) descending: West (3) > East (2)
    assert_eq!(
        eval_grid(
            "=PIVOTBY(A1:A6, B1:B6, C1:D6, HSTACK(SUM, COUNT), 3, 0, -5, 0)",
            5,
            9
        )[3][0],
        "West"
    );

    // Per-function wants_total with optional 2nd parameter [t] in LAMBDA(g, [t], ...)
    assert_eq!(
        eval_grid(
            "=GROUPBY(A1:A6, C1:C6, HSTACK(SUM, LAMBDA(g, [t], IF(ISOMITTED(t), -1, SUM(g)/SUM(t)))), 3, 0)",
            3,
            3
        ),
        rows(&[
            &["Region", "SUM", "LAMBDA"],
            &["East", "30", "0.697674418604651"],
            &["West", "13", "0.302325581395349"],
        ])
    );
}

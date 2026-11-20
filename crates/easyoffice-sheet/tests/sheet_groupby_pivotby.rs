//! `GROUPBY` and `PIVOTBY` (PD-271): the documented arguments and their limits.
//!
//! No reference engine has a body for either function, so the cases follow the function
//! documentation (`field_headers`, `total_depth`, `sort_order`, `filter_array`,
//! `field_relationship`, `relative_to`). The labels of total rows (`Total`, `<item> Total`) and the
//! generated header names are this engine's choice and are fixed here.
#![allow(clippy::unwrap_used)]

mod dynamic_support;
use dynamic_support::new_empty_model;

const SALES: [(&str, &str, &str); 6] = [
    ("Region", "Product", "Sales"),
    ("East", "Apple", "10"),
    ("East", "Pear", "20"),
    ("West", "Apple", "5"),
    ("West", "Apple", "7"),
    ("West", "Pear", "1"),
];

/// The text of the `rows` by `cols` cells the formula in `H1` fills, on the sales table in A1:C6.
fn grid(formula: &str, rows: usize, cols: usize) -> Vec<Vec<String>> {
    let mut model = new_empty_model();
    for (i, (a, b, c)) in SALES.iter().enumerate() {
        model._set(&format!("A{}", i + 1), a);
        model._set(&format!("B{}", i + 1), b);
        model._set(&format!("C{}", i + 1), c);
    }
    model._set("H1", formula);
    model.evaluate();
    (0..rows)
        .map(|r| {
            (0..cols)
                .map(|c| model._get_text(&format!("{}{}", (b'H' + c as u8) as char, r + 1)))
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
fn groupby_sums_by_one_field_with_a_grand_total() {
    assert_eq!(
        grid("=GROUPBY(A2:A6,C2:C6,SUM)", 4, 2),
        rows(&[
            &["East", "30"],
            &["West", "13"],
            &["Total", "43"],
            &["", ""]
        ])
    );
}

#[test]
fn groupby_detects_and_shows_headers() {
    // a text first value over a number second value: headers, shown
    assert_eq!(
        grid("=GROUPBY(A1:A6,C1:C6,SUM)", 4, 2),
        rows(&[
            &["Region", "Sales"],
            &["East", "30"],
            &["West", "13"],
            &["Total", "43"]
        ])
    );
    // present but not shown (1), absent but generated (2), present and shown (3), none (0)
    assert_eq!(
        grid("=GROUPBY(A1:A6,C1:C6,SUM,1)", 3, 2),
        rows(&[&["East", "30"], &["West", "13"], &["Total", "43"]])
    );
    assert_eq!(
        grid("=GROUPBY(A2:A6,C2:C6,SUM,2)", 2, 2),
        rows(&[&["Group", "Value"], &["East", "30"]])
    );
    assert_eq!(
        grid("=GROUPBY(A1:A6,C1:C6,SUM,3)", 2, 2),
        rows(&[&["Region", "Sales"], &["East", "30"]])
    );
    // with 0 the first row is data: "Region" is a group of its own
    assert_eq!(
        grid("=GROUPBY(A1:A6,C1:C6,COUNTA,0,0)", 3, 2),
        rows(&[&["East", "2"], &["Region", "1"], &["West", "3"]])
    );
}

#[test]
fn groupby_totals_follow_total_depth() {
    assert_eq!(
        grid("=GROUPBY(A2:A6,C2:C6,SUM,,0)", 3, 2),
        rows(&[&["East", "30"], &["West", "13"], &["", ""]])
    );
    assert_eq!(
        grid("=GROUPBY(A2:A6,C2:C6,SUM,,-1)", 3, 2),
        rows(&[&["Total", "43"], &["East", "30"], &["West", "13"]])
    );
    // subtotals need two group columns
    assert_eq!(
        grid("=GROUPBY(A2:A6,C2:C6,SUM,,2)", 1, 1),
        rows(&[&["#VALUE!"]])
    );
}

#[test]
fn groupby_makes_subtotals_for_two_group_columns() {
    assert_eq!(
        grid("=GROUPBY(A2:B6,C2:C6,SUM)", 7, 3),
        rows(&[
            &["East", "Apple", "10"],
            &["East", "Pear", "20"],
            &["East Total", "", "30"],
            &["West", "Apple", "12"],
            &["West", "Pear", "1"],
            &["West Total", "", "13"],
            &["Total", "", "43"],
        ])
    );
    // totals first
    assert_eq!(
        grid("=GROUPBY(A2:B6,C2:C6,SUM,,-2)", 4, 3),
        rows(&[
            &["Total", "", "43"],
            &["East Total", "", "30"],
            &["East", "Apple", "10"],
            &["East", "Pear", "20"],
        ])
    );
    // the grand total only
    assert_eq!(
        grid("=GROUPBY(A2:B6,C2:C6,SUM,,1)", 5, 3)[4],
        ["Total", "", "43"]
    );
}

#[test]
fn groupby_as_a_table_has_one_row_per_combination() {
    assert_eq!(
        grid("=GROUPBY(A2:B6,C2:C6,SUM,,1,,,1)", 5, 3),
        rows(&[
            &["East", "Apple", "10"],
            &["East", "Pear", "20"],
            &["West", "Apple", "12"],
            &["West", "Pear", "1"],
            &["Total", "", "43"],
        ])
    );
    // a table has no subtotals
    assert_eq!(
        grid("=GROUPBY(A2:B6,C2:C6,SUM,,2,,,1)", 1, 1),
        rows(&[&["#VALUE!"]])
    );
}

#[test]
fn groupby_sorts_by_group_columns_and_value_columns() {
    // value column (column 2 after one group column), ascending then descending
    assert_eq!(
        grid("=GROUPBY(A2:A6,C2:C6,SUM,,0,2)", 2, 2),
        rows(&[&["West", "13"], &["East", "30"]])
    );
    assert_eq!(
        grid("=GROUPBY(B2:B6,C2:C6,SUM,,0,-2)", 2, 2),
        rows(&[&["Apple", "22"], &["Pear", "21"]])
    );
    // the group column descending
    assert_eq!(
        grid("=GROUPBY(A2:A6,C2:C6,SUM,,0,-1)", 2, 2),
        rows(&[&["West", "13"], &["East", "30"]])
    );
    // a list sorts the levels in turn
    assert_eq!(
        grid("=GROUPBY(A2:B6,C2:C6,SUM,,0,{1,-2})", 4, 3),
        rows(&[
            &["East", "Pear", "20"],
            &["East", "Apple", "10"],
            &["West", "Pear", "1"],
            &["West", "Apple", "12"],
        ])
    );
    assert_eq!(
        grid("=GROUPBY(A2:A6,C2:C6,SUM,,0,3)", 1, 1),
        rows(&[&["#VALUE!"]])
    );
    assert_eq!(
        grid("=GROUPBY(A2:A6,C2:C6,SUM,,0,0)", 1, 1),
        rows(&[&["#VALUE!"]])
    );
}

#[test]
fn groupby_filters_rows_and_takes_functions() {
    assert_eq!(
        grid(
            "=GROUPBY(A2:A6,C2:C6,SUM,,1,,{TRUE;FALSE;TRUE;TRUE;FALSE})",
            3,
            2
        ),
        rows(&[&["East", "10"], &["West", "12"], &["Total", "22"]])
    );
    assert_eq!(
        grid("=GROUPBY(A2:A6,C2:C6,AVERAGE,,0)", 2, 2),
        rows(&[&["East", "15"], &["West", "4.33333333333333"]])
    );
    assert_eq!(
        grid("=GROUPBY(A2:A6,C2:C6,COUNT,,0)", 2, 2),
        rows(&[&["East", "2"], &["West", "3"]])
    );
    assert_eq!(
        grid("=GROUPBY(A2:A6,C2:C6,LAMBDA(x,MAX(x)-MIN(x)),,0)", 2, 2),
        rows(&[&["East", "10"], &["West", "6"]])
    );
    // PERCENTOF: the share of the column's total
    assert_eq!(
        grid("=GROUPBY(A2:A6,C2:C6,PERCENTOF,,0)", 2, 2),
        rows(&[
            &["East", "0.697674418604651"],
            &["West", "0.302325581395349"]
        ])
    );
    // several value columns
    assert_eq!(
        grid("=GROUPBY(A2:A6,C2:C6*{1,2},SUM,,0)", 2, 3),
        rows(&[&["East", "30", "60"], &["West", "13", "26"]])
    );
}

#[test]
fn groupby_checks_its_arguments() {
    for formula in [
        // the value column is shorter
        "=GROUPBY(A2:A6,C2:C5,SUM)",
        // the filter is shorter
        "=GROUPBY(A2:A6,C2:C6,SUM,,,,{TRUE;FALSE})",
        // field_headers, field_relationship out of range
        "=GROUPBY(A2:A6,C2:C6,SUM,4)",
        "=GROUPBY(A2:A6,C2:C6,SUM,,,,,2)",
        // a filter that is no Boolean
        "=GROUPBY(A2:A6,C2:C6,SUM,,,,{\"a\";TRUE;TRUE;TRUE;TRUE})",
        // not enough or too many arguments
        "=GROUPBY(A2:A6,C2:C6)",
        "=GROUPBY(A2:A6,C2:C6,SUM,,,,,,1)",
        // a value that is no function
        "=GROUPBY(A2:A6,C2:C6,5)",
    ] {
        assert_eq!(grid(formula, 1, 1), rows(&[&["#VALUE!"]]), "{formula}");
    }
    assert_eq!(
        grid("=GROUPBY(A2:A6,C2:C6,NOSUCHFUNCTION)", 1, 1),
        rows(&[&["#NAME?"]])
    );
    // every row filtered out
    assert_eq!(
        grid(
            "=GROUPBY(A2:A6,C2:C6,SUM,,,,{FALSE;FALSE;FALSE;FALSE;FALSE})",
            1,
            1
        ),
        rows(&[&["#CALC!"]])
    );
    assert_eq!(grid("=GROUPBY(#N/A,C2:C6,SUM)", 1, 1), rows(&[&["#N/A"]]));
}

#[test]
fn pivotby_crosses_two_fields() {
    assert_eq!(
        grid("=PIVOTBY(A2:A6,B2:B6,C2:C6,SUM)", 4, 4),
        rows(&[
            &["", "Apple", "Pear", "Total"],
            &["East", "10", "20", "30"],
            &["West", "12", "1", "13"],
            &["Total", "22", "21", "43"],
        ])
    );
    // headers: the field names and the value name are shown
    assert_eq!(
        grid("=PIVOTBY(A1:A6,B1:B6,C1:C6,SUM)", 5, 4),
        rows(&[
            &["Product", "Apple", "Pear", "Total"],
            &["Region", "Sales", "Sales", "Sales"],
            &["East", "10", "20", "30"],
            &["West", "12", "1", "13"],
            &["Total", "22", "21", "43"],
        ])
    );
    // no totals, sorted by the column's field descending and by the row total ascending
    assert_eq!(
        grid("=PIVOTBY(A2:A6,B2:B6,C2:C6,SUM,,0,2,0,-1)", 3, 3),
        rows(&[
            &["", "Pear", "Apple"],
            &["West", "1", "12"],
            &["East", "20", "10"],
        ])
    );
    // a missing combination is blank
    assert_eq!(
        grid(
            "=PIVOTBY(A2:A6,B2:B6,C2:C6,SUM,,0,,0,,{TRUE;TRUE;TRUE;FALSE;FALSE})",
            3,
            3
        ),
        rows(&[
            &["", "Apple", "Pear"],
            &["East", "10", "20"],
            &["West", "5", ""],
        ])
    );
}

#[test]
fn pivotby_shows_shares_relative_to_a_total() {
    // 0 column totals, 1 row totals, 2 the grand total
    assert_eq!(
        grid("=PIVOTBY(A2:A6,B2:B6,C2:C6,PERCENTOF,,0,,0,,,0)", 3, 3),
        rows(&[
            &["", "Apple", "Pear"],
            &["East", "0.454545454545455", "0.952380952380952"],
            &["West", "0.545454545454545", "0.0476190476190476"],
        ])
    );
    assert_eq!(
        grid("=PIVOTBY(A2:A6,B2:B6,C2:C6,PERCENTOF,,0,,0,,,1)", 3, 3)[1],
        ["East", "0.333333333333333", "0.666666666666667"]
    );
    assert_eq!(
        grid("=PIVOTBY(A2:A6,B2:B6,C2:C6,PERCENTOF,,0,,0,,,2)", 3, 3)[1],
        ["East", "0.232558139534884", "0.465116279069767"]
    );
    // a function of two arguments of its own
    assert_eq!(
        grid(
            "=PIVOTBY(A2:A6,B2:B6,C2:C6,LAMBDA(s,t,SUM(s)-SUM(t)),,0,,0,,,1)",
            3,
            3
        )[1],
        ["East", "-20", "-10"]
    );
    assert_eq!(
        grid("=PIVOTBY(A2:A6,B2:B6,C2:C6,PERCENTOF,,0,,0,,,5)", 1, 1),
        rows(&[&["#VALUE!"]])
    );
}

#[test]
fn groupby_and_pivotby_fail_closed_on_hostile_sizes() {
    let mut model = new_empty_model();
    // 2,000 by 2,000 groups: 4 million cells are past the array budget
    model._set(
        "A1",
        "=PIVOTBY(SEQUENCE(2000),SEQUENCE(2000),SEQUENCE(2000),SUM,,0,,0)",
    );
    // more than 16 group columns
    model._set("A2", "=GROUPBY(SEQUENCE(4,17),SEQUENCE(4),SUM)");
    // many groups are fine
    model._set(
        "A3",
        "=ROWS(GROUPBY(SEQUENCE(20000),SEQUENCE(20000),SUM,,0))",
    );
    model.evaluate();
    assert_eq!(model._get_text("A1"), "#SPILL!");
    assert_eq!(model._get_text("A2"), "#VALUE!");
    assert_eq!(model._get_text("A3"), "20000");
}

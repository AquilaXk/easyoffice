//! Implicit intersection operator `@` and `_xlfn.SINGLE` function tests (PD-322).

mod dynamic_support;
use dynamic_support::{new_empty_model, DefinedNameEntry};

#[test]
fn implicit_single_intersects_1d_ranges_cross_sheet_and_1x1() {
    let mut m = new_empty_model();
    for r in 1..=5 {
        m._set(&format!("A{r}"), &(r * 10).to_string());
        m._set(&format!("Sheet2!A{r}"), &(r * 100).to_string());
    }
    m._set("F1", "77");
    m._set("G1", "88");
    m._set("H1", "99");

    // 1-col intersection on rows 2, 5, and out-of-range row 7
    m._set("B2", "=@A1:A5");
    m._set("B5", "=SINGLE(A1:A5)");
    m._set("B7", "=@A1:A5");
    // 1x1 range ignores caller position
    m._set("B8", "=@$A$3:$A$3");
    // 1-row intersection on col G (row 2) and out-of-range col J (row 2)
    m._set("G2", "=@F1:H1");
    m._set("J2", "=@F1:H1");
    // 2D range (caller D3 is outside A1:C5 so DAG has no self-cycle) -> #VALUE!
    m._set("D3", "=@A1:C5");
    // Cross-sheet 1-col intersection and missing sheet -> #REF!
    m._set("C4", "=@Sheet2!A1:A5");
    m._set("C5", "=@MissingSheet!A1:A5");
    // Invalid arity -> #VALUE!
    m._set("C6", "=SINGLE()");
    m._set("C7", "=SINGLE(A1, A2)");
    m.evaluate();

    assert_eq!(m._get_text("B2"), "20");
    assert_eq!(m._get_text("B5"), "50");
    assert_eq!(m._get_text("B7"), "#VALUE!");
    assert_eq!(m._get_text("B8"), "30");
    assert_eq!(m._get_text("G2"), "88");
    assert_eq!(m._get_text("J2"), "#VALUE!");
    assert_eq!(m._get_text("D3"), "#VALUE!");
    assert_eq!(m._get_text("C4"), "400");
    assert_eq!(m._get_text("C5"), "#REF!");
    assert_eq!(m._get_text("C6"), "#VALUE!");
    assert_eq!(m._get_text("C7"), "#VALUE!");
}

#[test]
fn implicit_single_suppresses_spill_and_takes_top_left_of_arrays() {
    let mut m = new_empty_model();
    m._set("A1", "10");
    m._set("A2", "20");
    m._set("A3", "30");

    // Array generators collapse to their top-left scalar without spilling
    m._set("B1", "=@SEQUENCE(5)");
    m._set("C1", "=@{10,20;30,40}");
    m._set("D1", "=@FILTER(A1:A3, A1:A3>=20)");
    m._set("E1", "=@FILTER(A1:A3, A1:A3>999)");
    // Scalar expression remains scalar
    m._set("F1", "=@(A1+5)");
    // Spill reference `F3#` intersects onto row 5 of `G5`
    m._set("F3", "=SEQUENCE(4)*10");
    m._set("G5", "=@F3#");
    m.evaluate();

    assert_eq!(m._get_text("B1"), "1");
    assert_eq!(m._get_text("B2"), "", "=@SEQUENCE(5) must not spill");
    assert_eq!(m._get_text("C1"), "10");
    assert_eq!(m._get_text("C2"), "");
    assert_eq!(m._get_text("D1"), "20");
    assert_eq!(m._get_text("E1"), "#CALC!");
    assert_eq!(m._get_text("F1"), "15");
    assert_eq!(m._get_text("G5"), "30");
}

#[test]
fn implicit_single_works_with_ref_functions_agg_names_and_let() {
    let mut m = new_empty_model();
    for r in 1..=5 {
        m._set(&format!("A{r}"), &(r * 2).to_string());
    }
    m.new_defined_name(DefinedNameEntry::new("MyCol", "=Sheet1!$A$1:$A$5"))
        .unwrap();

    m._set("B3", "=COLUMNS(@A1:C1)");
    m._set("D3", "=ROWS(@A1:A5)");
    m._set("E3", "=ROW(@A1:A5)");
    m._set("F3", "=ISREF(@A1:A5)");
    m._set("G3", "=ISREF(@SEQUENCE(5))");
    m._set("H3", "=SUM(@A1:A5)");
    m._set("I3", "=@MyCol");
    m._set("J3", "=LET(x, A1:A5, @x)");
    m._set("K3", "=LET(x, SEQUENCE(5), @x)");
    m.evaluate();

    assert_eq!(m._get_text("B3"), "1");
    assert_eq!(m._get_text("D3"), "1");
    assert_eq!(m._get_text("E3"), "3");
    assert_eq!(m._get_text("F3"), "TRUE");
    assert_eq!(m._get_text("G3"), "FALSE");
    assert_eq!(m._get_text("H3"), "6");
    assert_eq!(m._get_text("I3"), "6");
    assert_eq!(m._get_text("J3"), "6");
    assert_eq!(m._get_text("K3"), "1");
}

#[test]
fn hostile_deep_at_chain_stops_at_depth_limit() {
    let mut m = new_empty_model();
    let deep = format!("={}1", "@".repeat(128));
    m._set("A1", &deep);
    m.evaluate();
    assert_eq!(m._get_text("A1"), "#VALUE!");
}

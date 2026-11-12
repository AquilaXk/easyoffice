//! Formula function bundle A (PD-195): `LOOKUP`, `HYPERLINK`, `ISFORMULA`, `FORMULATEXT`,
//! `ADDRESS`, `AREAS`, the database functions and the matrix functions.
//!
//! The cases are the ones of the reference implementation's unit tests (same data, same expected
//! values), written as sheet formulas. Cases whose result differs from the reference are marked
//! `DIFFERS`.

use easyoffice_sheet::{
    EvaluatedCellValue as V, FormulaError as E, PackedCellCoordinate, SheetEngine,
};

fn num(n: f64) -> V {
    V::Number(n)
}

fn err(e: E) -> V {
    V::Error(e)
}

fn text(s: &str) -> V {
    V::Text(s.to_string())
}

fn close(v: &V, expected: f64, tolerance: f64) {
    let V::Number(n) = v else {
        panic!("expected a number near {expected}, got {v:?}");
    };
    assert!(
        (n - expected).abs() <= tolerance * expected.abs().max(1.0),
        "{n} is not within {tolerance} of {expected}"
    );
}

/// Writes `rows` (cells as formula-bar text, `""` clears a cell) with the top-left cell at `(row, col)`.
fn fill(s: &mut SheetEngine, row: u32, col: u32, rows: &[&[&str]]) {
    for (r, line) in rows.iter().enumerate() {
        for (c, raw) in line.iter().enumerate() {
            let at = PackedCellCoordinate::new(row + r as u32, col + c as u32).unwrap();
            s.set_cell(&at.to_a1(), *raw).unwrap();
        }
    }
}

fn eval(s: &SheetEngine, formula: &str) -> V {
    s.evaluate_formula_str(formula)
}

// ---------------------------------------------------------------- lookup and reference

#[test]
fn lookup_vector_form_finds_the_last_key_not_above_the_value() {
    let mut s = SheetEngine::new("S");
    fill(
        &mut s,
        0,
        0,
        &[
            &["1", "11"],
            &["2", "22"],
            &["3", "33"],
            &["4", "44"],
            &["5", "55"],
            &["6", "66"],
            &["7", "77"],
            &["8", "88"],
        ],
    );
    assert_eq!(eval(&s, "=LOOKUP(8,A1:A8,B1:B8)"), num(88.0));
    assert_eq!(eval(&s, "=LOOKUP(11,A1:A8,B1:B8)"), num(88.0));
    assert_eq!(eval(&s, "=LOOKUP(0,A1:A8,B1:B8)"), err(E::Na));
    assert_eq!(eval(&s, "=LOOKUP(\"999\",A1:A8,B1:B8)"), err(E::Na));
    assert_eq!(eval(&s, "=LOOKUP(4.5,A1:A8,B1:B8)"), num(44.0));
    // no result vector: the key itself
    assert_eq!(eval(&s, "=LOOKUP(4.5,A1:A8)"), num(4.0));
    // the vectors have to have the same shape
    assert_eq!(eval(&s, "=LOOKUP(3,A1:A8,B1:B7)"), err(E::Ref));
    assert_eq!(eval(&s, "=LOOKUP(3,A1:A8,5)"), err(E::Ref));
    assert_eq!(eval(&s, "=LOOKUP(3,5)"), err(E::Value));
    assert_eq!(eval(&s, "=LOOKUP(#N/A,A1:A8)"), err(E::Na));
}

#[test]
fn lookup_array_form_searches_the_first_row_or_column() {
    let mut s = SheetEngine::new("S");
    fill(
        &mut s,
        0,
        0,
        &[
            &["1", "First"],
            &["2", "Second"],
            &["3", "Third"],
            &["4", "Fourth"],
            &["5", "Fifth"],
            &["6", "Sixth"],
            &["7", "Seventh"],
            &["8", "Eighth"],
        ],
    );
    assert_eq!(eval(&s, "=LOOKUP(2,A1:B8)"), text("Second"));
    // wider than tall: the first row is searched, the last row answers
    fill(&mut s, 0, 3, &[&["1", "3", "5"], &["a", "b", "c"]]);
    assert_eq!(eval(&s, "=LOOKUP(4,D1:F2)"), text("b"));
    // square: the first column is searched, the last column answers (the reference's matching array)
    fill(&mut s, 9, 0, &[&["1", "3"], &["4", "6"], &["8", "7"]]);
    assert_eq!(eval(&s, "=LOOKUP(4,A10:B12)"), num(6.0));
}

#[test]
fn lookup_handles_text_keys_and_unsorted_keys_without_panicking() {
    let mut s = SheetEngine::new("S");
    fill(
        &mut s,
        0,
        0,
        &[&["apple", "1"], &["banana", "2"], &["cherry", "3"]],
    );
    assert_eq!(eval(&s, "=LOOKUP(\"BANANA\",A1:A3,B1:B3)"), num(2.0));
    assert_eq!(eval(&s, "=LOOKUP(\"blueberry\",A1:A3,B1:B3)"), num(2.0));
    // numeric lookup over text keys: no comparable key
    assert_eq!(eval(&s, "=LOOKUP(5,A1:A3,B1:B3)"), err(E::Na));
    fill(&mut s, 0, 3, &[&["5", "x"], &["", "y"], &["2", "z"]]);
    assert_eq!(eval(&s, "=LOOKUP(3,D1:D3,E1:E3)"), text("z"));
}

#[test]
fn hyperlink_returns_the_name_for_allowed_targets_only() {
    let s = SheetEngine::new("S");
    assert_eq!(
        eval(&s, "=HYPERLINK(\"https://example.com/a\",\"Open\")"),
        text("Open")
    );
    assert_eq!(
        eval(&s, "=HYPERLINK(\"https://example.com/a\")"),
        text("https://example.com/a")
    );
    assert_eq!(
        eval(&s, "=HYPERLINK(\"mailto:me@example.com\",\"Mail\")"),
        text("Mail")
    );
    // DIFFERS: the reference accepts any target; here the schemes are the document editor's (PD-192)
    assert_eq!(
        eval(&s, "=HYPERLINK(\"javascript:alert(1)\",\"x\")"),
        err(E::Value)
    );
    assert_eq!(
        eval(&s, "=HYPERLINK(\"file:///etc/passwd\",\"x\")"),
        err(E::Value)
    );
    assert_eq!(
        eval(&s, "=HYPERLINK(\"data:text/html,x\",\"x\")"),
        err(E::Value)
    );
    assert_eq!(eval(&s, "=HYPERLINK(\"https://\",\"x\")"), err(E::Value));
    assert_eq!(eval(&s, "=HYPERLINK(\"\",\"x\")"), err(E::Value));
    assert_eq!(eval(&s, "=HYPERLINK(\"https://a b\",\"x\")"), err(E::Value));
    // DIFFERS: a number name stays a number (the reference returns its text)
    assert_eq!(eval(&s, "=HYPERLINK(\"https://example.com\",5)"), num(5.0));
    assert_eq!(eval(&s, "=HYPERLINK(#N/A,\"x\")"), err(E::Na));
    assert_eq!(
        eval(&s, "=HYPERLINK(\"https://example.com\",#REF!)"),
        err(E::Ref)
    );
}

#[test]
fn hyperlink_accepts_jumps_inside_the_workbook() {
    let s = SheetEngine::new("S");
    for target in [
        "#시트!A1",
        "#Sheet1!B2:C3",
        "#'시트 이름'!A1:B2",
        "#'It''s'!A1",
        "#정의된이름",
        "#Sales_2024.total",
        "#A1",
        "#Data!Totals",
    ] {
        assert_eq!(
            eval(&s, &format!("=HYPERLINK(\"{target}\",\"go\")")),
            text("go"),
            "{target}"
        );
    }
    assert_eq!(eval(&s, "=HYPERLINK(\"#Sheet1!A1\")"), text("#Sheet1!A1"));
    for target in [
        "#",
        "#!A1",
        "#Sheet1!",
        "#Sheet1!A1 B2",
        "#'시트!A1",
        "#'시트'A1",
        "#Sheet 1!A1",
        "#1abc",
        "#bad name",
        "#Sheet1!!A1",
        "#Sheet1!A1:",
        "Sheet1!A1",
        "A1",
    ] {
        assert_eq!(
            eval(&s, &format!("=HYPERLINK(\"{target}\",\"go\")")),
            err(E::Value),
            "{target}"
        );
    }
    // control characters and over-long targets
    assert_eq!(
        eval(&s, "=HYPERLINK(\"#Sheet1!A\"&CHAR(10)&\"1\",\"go\")"),
        err(E::Value)
    );
    assert_eq!(
        eval(&s, "=HYPERLINK(\"#'a\"&CHAR(9)&\"b'!A1\",\"go\")"),
        err(E::Value)
    );
    let long = format!("=HYPERLINK(\"#{}\",\"go\")", "n".repeat(2_048));
    assert_eq!(eval(&s, &long), err(E::Value));
    let fits = format!("=HYPERLINK(\"#{}\",\"go\")", "n".repeat(2_047));
    assert_eq!(eval(&s, &fits), text("go"));
    // outside links keep the document editor's schemes
    assert_eq!(
        eval(&s, "=HYPERLINK(\"ftp://example.com\",\"x\")"),
        err(E::Value)
    );
}

#[test]
fn isformula_and_formulatext_read_the_cell_source() {
    let mut s = SheetEngine::new("S");
    fill(
        &mut s,
        0,
        0,
        &[&["1", "2", "=SUM(A1:B1)"], &["", "'=text", "=A1+1"]],
    );
    assert_eq!(eval(&s, "=ISFORMULA(C1)"), V::Boolean(true));
    assert_eq!(eval(&s, "=ISFORMULA(A1)"), V::Boolean(false));
    assert_eq!(eval(&s, "=ISFORMULA(A2)"), V::Boolean(false));
    assert_eq!(eval(&s, "=ISFORMULA(B2)"), V::Boolean(false));
    assert_eq!(eval(&s, "=FORMULATEXT(C1)"), text("=SUM(A1:B1)"));
    assert_eq!(eval(&s, "=FORMULATEXT(C2)"), text("=A1+1"));
    assert_eq!(eval(&s, "=FORMULATEXT(A1)"), err(E::Na));
    // a range gives its first cell
    assert_eq!(eval(&s, "=FORMULATEXT(C1:D2)"), text("=SUM(A1:B1)"));
    assert_eq!(eval(&s, "=ISFORMULA(A1:C1)"), V::Boolean(false));
    // not a reference
    assert_eq!(eval(&s, "=FORMULATEXT(\"test\")"), err(E::Na));
    assert_eq!(eval(&s, "=ISFORMULA(\"test\")"), err(E::Na));
    assert_eq!(eval(&s, "=FORMULATEXT(#NAME?)"), err(E::Na));
    assert_eq!(eval(&s, "=ISFORMULA(#NAME?)"), err(E::Name));
    assert_eq!(eval(&s, "=ISFORMULA(Nope!A1)"), err(E::Ref));
}

#[test]
fn isformula_reads_other_sheets_of_a_workbook() {
    use easyoffice_sheet::WorkbookEngine;
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Data");
    wb.add_sheet("Main");
    wb.set_cell("Data", "A1", "=1+1").unwrap();
    wb.set_cell("Data", "A2", "5").unwrap();
    wb.set_cell("Main", "A1", "=ISFORMULA(Data!A1)").unwrap();
    wb.set_cell("Main", "A2", "=ISFORMULA(Data!A2)").unwrap();
    wb.set_cell("Main", "A3", "=FORMULATEXT(Data!A1)").unwrap();
    assert_eq!(wb.evaluate_cell("Main", "A1"), V::Boolean(true));
    assert_eq!(wb.evaluate_cell("Main", "A2"), V::Boolean(false));
    assert_eq!(wb.evaluate_cell("Main", "A3"), text("=1+1"));
}

#[test]
fn address_builds_a1_and_r1c1_text() {
    let s = SheetEngine::new("S");
    assert_eq!(eval(&s, "=ADDRESS(2,3)"), text("$C$2"));
    assert_eq!(eval(&s, "=ADDRESS(2,3,2)"), text("C$2"));
    assert_eq!(eval(&s, "=ADDRESS(2,3,3)"), text("$C2"));
    assert_eq!(eval(&s, "=ADDRESS(2,3,4)"), text("C2"));
    assert_eq!(eval(&s, "=ADDRESS(2,3,1,FALSE)"), text("R2C3"));
    assert_eq!(
        eval(&s, "=ADDRESS(2,3,1,FALSE,\"[Book1]Sheet1\")"),
        text("'[Book1]Sheet1'!R2C3")
    );
    assert_eq!(
        eval(&s, "=ADDRESS(1,1,1,TRUE,\"Sheet'1\")"),
        text("'Sheet''1'!$A$1")
    );
    assert_eq!(
        eval(&s, "=ADDRESS(2,3,1,FALSE,\"MY SHEET\")"),
        text("'MY SHEET'!R2C3")
    );
    // omitted absolute type and style
    assert_eq!(
        eval(&s, "=ADDRESS(10,1,,,\"Krycí List\")"),
        text("'Krycí List'!$A$10")
    );
    assert_eq!(
        eval(&s, "=ADDRESS(10,28,4,TRUE,\"Sheet4\")"),
        text("Sheet4!AB10")
    );
    assert_eq!(eval(&s, "=ADDRESS(1048576,16384)"), text("$XFD$1048576"));
    assert_eq!(eval(&s, "=ADDRESS(2.9,3.9)"), text("$C$2"));
    // absolute type out of 1..4
    assert_eq!(
        eval(&s, "=ADDRESS(2,3,0,FALSE,\"MY SHEET\")"),
        err(E::Value)
    );
    assert_eq!(
        eval(&s, "=ADDRESS(2,3,5,FALSE,\"MY SHEET\")"),
        err(E::Value)
    );
    // outside the sheet, and errors in any argument
    assert_eq!(eval(&s, "=ADDRESS(0,1)"), err(E::Value));
    assert_eq!(eval(&s, "=ADDRESS(1,16385)"), err(E::Value));
    assert_eq!(eval(&s, "=ADDRESS(1048577,1)"), err(E::Value));
    assert_eq!(eval(&s, "=ADDRESS(#VALUE!,#VALUE!)"), err(E::Value));
    assert_eq!(eval(&s, "=ADDRESS(1,#N/A)"), err(E::Na));
    assert_eq!(eval(&s, "=ADDRESS(\"a\",1)"), err(E::Value));
}

#[test]
fn address_relative_r1c1_counts_from_the_formula_cell() {
    let mut s = SheetEngine::new("S");
    s.set_cell("B3", "=ADDRESS(5,5,4,FALSE)").unwrap();
    s.set_cell("B4", "=ADDRESS(5,5,2,FALSE)").unwrap();
    s.set_cell("B5", "=ADDRESS(5,5,3,FALSE)").unwrap();
    s.set_cell("E5", "=ADDRESS(5,5,4,FALSE)").unwrap();
    assert_eq!(s.evaluate_cell("B3"), text("R[2]C[3]"));
    assert_eq!(s.evaluate_cell("B4"), text("R5C[3]"));
    assert_eq!(s.evaluate_cell("B5"), text("RC5"));
    // the formula cell itself is offset 0, written without brackets
    assert_eq!(s.evaluate_cell("E5"), text("RC"));
    // DIFFERS: the reference counts from the sheet origin (`R2C[3]` for row 2, column 3, absolute row)
    assert_eq!(eval(&s, "=ADDRESS(2,3,2,FALSE)"), text("R2C[2]"));
}

#[test]
fn areas_counts_a_reference_as_one() {
    let mut s = SheetEngine::new("S");
    s.set_cell("A1", "1").unwrap();
    assert_eq!(eval(&s, "=AREAS(A1)"), num(1.0));
    assert_eq!(eval(&s, "=AREAS(A1:C9)"), num(1.0));
    assert_eq!(eval(&s, "=AREAS(\"A1\")"), err(E::Value));
    assert_eq!(eval(&s, "=AREAS(5)"), err(E::Value));
}

// ---------------------------------------------------------------- database

const DB: &[&[&str]] = &[
    &["Tree", "Height", "Age", "Yield", "Profit"],
    &["Maple", "18", "20", "14", "105"],
    &["Pear", "1.2", "1.2", "10", "96"],
    &["Cherry", "1.3", "14", "9", "105"],
    &["Maple", "14", "15", "10", "75"],
    &["Pear", "9", "8", "8", "76.8"],
    &["Maple", "8", "9", "6", "45"],
];

fn db_sheet() -> SheetEngine {
    let mut s = SheetEngine::new("S");
    fill(&mut s, 0, 0, DB);
    // criteria: Maple with Height > 10
    fill(&mut s, 0, 6, &[&["Tree", "Height"], &["Maple", ">10"]]);
    s
}

fn db_call(s: &SheetEngine, name: &str, field: &str, criteria: &str) -> V {
    eval(s, &format!("={name}(A1:E7,{field},{criteria})"))
}

#[test]
fn database_functions_evaluate_as_written_formulas() {
    let s = db_sheet();
    assert_eq!(eval(&s, "=DSUM(A1:E7,\"Yield\",G1:H2)"), num(24.0));
    assert_eq!(eval(&s, "=DAVERAGE(A1:E7,\"Yield\",G1:H2)"), num(12.0));
    assert_eq!(eval(&s, "=DCOUNT(A1:E7,\"Yield\",G1:H2)"), num(2.0));
    assert_eq!(eval(&s, "=DCOUNTA(A1:E7,\"Yield\",G1:H2)"), num(2.0));
    assert_eq!(eval(&s, "=DGET(A1:E7,\"Yield\",G1:H2)"), err(E::Num));
    assert_eq!(eval(&s, "=DMAX(A1:E7,\"Yield\",G1:H2)"), num(14.0));
    assert_eq!(eval(&s, "=DMIN(A1:E7,\"Yield\",G1:H2)"), num(10.0));
    assert_eq!(eval(&s, "=DPRODUCT(A1:E7,\"Yield\",G1:H2)"), num(140.0));
    close(
        &eval(&s, "=DSTDEV(A1:E7,\"Yield\",G1:H2)"),
        8.0_f64.sqrt(),
        1e-12,
    );
    assert_eq!(eval(&s, "=DSTDEVP(A1:E7,\"Yield\",G1:H2)"), num(2.0));
    assert_eq!(eval(&s, "=DVAR(A1:E7,\"Yield\",G1:H2)"), num(8.0));
    assert_eq!(eval(&s, "=DVARP(A1:E7,\"Yield\",G1:H2)"), num(4.0));
}

#[test]
fn database_functions_match_the_reference_on_its_sample_table() {
    let s = db_sheet();
    let stdev = 8.0_f64.sqrt();
    let rows = [
        ("DSUM", num(24.0)),
        ("DAVERAGE", num(12.0)),
        ("DCOUNT", num(2.0)),
        ("DCOUNTA", num(2.0)),
        ("DMAX", num(14.0)),
        ("DMIN", num(10.0)),
        ("DPRODUCT", num(140.0)),
        ("DSTDEV", num(stdev)),
        ("DSTDEVP", num(2.0)),
        ("DVAR", num(8.0)),
        ("DVARP", num(4.0)),
    ];
    for (name, expected) in rows {
        let got = db_call(&s, name, "\"Yield\"", "G1:H2");
        match (&got, &expected) {
            (V::Number(a), V::Number(b)) => assert!((a - b).abs() < 1e-12, "{name}: {a} vs {b}"),
            _ => assert_eq!(got, expected, "{name}"),
        }
        // the field by position and by lower-case header give the same value
        assert_eq!(db_call(&s, name, "4", "G1:H2"), got, "{name} by position");
        assert_eq!(
            db_call(&s, name, "\"yield\"", "G1:H2"),
            got,
            "{name} by lower-case header"
        );
    }
    // DGET: the one record above 15
    let mut t = db_sheet();
    t.set_cell("H2", ">15").unwrap();
    assert_eq!(db_call(&t, "DGET", "\"Yield\"", "G1:H2"), num(14.0));
    // two records: #NUM!
    assert_eq!(db_call(&s, "DGET", "\"Yield\"", "G1:H2"), err(E::Num));
}

#[test]
fn database_field_argument_rules() {
    let s = db_sheet();
    // (function, field TRUE = column 1 of text, expected)
    let by_true = [
        ("DSUM", num(0.0)),
        ("DAVERAGE", err(E::DivByZero)),
        ("DCOUNT", num(0.0)),
        ("DCOUNTA", num(2.0)),
        ("DGET", err(E::Num)),
        ("DMAX", num(0.0)),
        ("DMIN", num(0.0)),
        ("DPRODUCT", num(0.0)),
        ("DSTDEV", err(E::DivByZero)),
        ("DSTDEVP", err(E::DivByZero)),
        ("DVAR", err(E::DivByZero)),
        ("DVARP", err(E::DivByZero)),
    ];
    for (name, expected) in by_true {
        assert_eq!(db_call(&s, name, "TRUE", "G1:H2"), expected, "{name} TRUE");
        assert_eq!(
            db_call(&s, name, "Z9", "G1:H2"),
            err(E::Value),
            "{name} blank field"
        );
        assert_eq!(
            db_call(&s, name, "\"nothing\"", "G1:H2"),
            err(E::Value),
            "{name} unknown header"
        );
        assert_eq!(
            db_call(&s, name, "0", "G1:H2"),
            err(E::Value),
            "{name} position 0"
        );
        assert_eq!(
            db_call(&s, name, "6", "G1:H2"),
            err(E::Value),
            "{name} position 6"
        );
        assert_eq!(
            db_call(&s, name, "#NAME?", "G1:H2"),
            err(E::Name),
            "{name} error field"
        );
        assert_eq!(
            db_call(&s, name, "A1:B1", "G1:H2"),
            err(E::Value),
            "{name} range field"
        );
        assert_eq!(
            db_call(&s, name, "\"4\"", "G1:H2"),
            db_call(&s, name, "4", "G1:H2"),
            "{name} numeric text"
        );
    }
}

#[test]
fn database_criteria_rules() {
    let s = db_sheet();
    // blank criteria rows match every record
    let mut all = db_sheet();
    fill(&mut all, 0, 9, &[&["", ""], &["", ""]]);
    let all_rows = [
        ("DSUM", num(57.0)),
        ("DAVERAGE", num(9.5)),
        ("DCOUNT", num(6.0)),
        ("DCOUNTA", num(6.0)),
        ("DGET", err(E::Num)),
        ("DMAX", num(14.0)),
        ("DMIN", num(6.0)),
        ("DPRODUCT", num(604_800.0)),
        ("DVAR", num(7.1)),
    ];
    for (name, expected) in all_rows {
        let got = db_call(&all, name, "\"Yield\"", "J1:K2");
        match (&got, &expected) {
            (V::Number(a), V::Number(b)) => assert!((a - b).abs() < 1e-9, "{name}: {a} vs {b}"),
            _ => assert_eq!(got, expected, "{name}"),
        }
    }
    close(
        &db_call(&all, "DSTDEVP", "\"Yield\"", "J1:K2"),
        2.43241991989,
        1e-11,
    );
    close(
        &db_call(&all, "DVARP", "\"Yield\"", "J1:K2"),
        5.91666666667,
        1e-11,
    );
    // header row only
    assert_eq!(db_call(&s, "DSUM", "\"Yield\"", "G1:H1"), err(E::Value));
    // a header that is no field: a logical criterion TRUE is ignored, FALSE rejects, so only
    // the second row (a blank row with another column) decides
    let mut t = db_sheet();
    fill(&mut t, 0, 9, &[&["", "Height"], &["TRUE", "FALSE"]]);
    assert_eq!(db_call(&t, "DSUM", "\"Yield\"", "J1:K2"), num(0.0));
    assert_eq!(db_call(&t, "DGET", "\"Yield\"", "J1:K2"), err(E::Value));
    fill(
        &mut t,
        0,
        9,
        &[&["", "Height"], &["TRUE", ""], &["", "FALSE"]],
    );
    assert_eq!(db_call(&t, "DSUM", "\"Yield\"", "J1:K3"), num(57.0));
    // text and numeric criteria under an unknown header reject everything
    fill(&mut t, 0, 9, &[&["Nope", "Height"], &["x", ""]]);
    assert_eq!(db_call(&t, "DSUM", "\"Yield\"", "J1:K2"), num(0.0));
    // rows are alternatives, cells of a row are conjunctions
    fill(
        &mut t,
        0,
        9,
        &[&["Tree", "Height"], &["Cherry", ""], &["Pear", ">5"]],
    );
    assert_eq!(db_call(&t, "DSUM", "\"Yield\"", "J1:K3"), num(17.0));
    // operators, wildcards and case
    fill(&mut t, 0, 9, &[&["Tree"], &["m*"]]);
    assert_eq!(db_call(&t, "DCOUNT", "\"Yield\"", "J1:J2"), num(3.0));
    fill(&mut t, 0, 9, &[&["Tree"], &["<>maple"]]);
    assert_eq!(db_call(&t, "DCOUNT", "\"Yield\"", "J1:J2"), num(3.0));
    fill(&mut t, 0, 9, &[&["Height"], &[">=9"]]);
    assert_eq!(db_call(&t, "DCOUNT", "\"Yield\"", "J1:J2"), num(3.0));
}

#[test]
fn database_value_cells_follow_the_reference() {
    // a database with one record only: #VALUE! (the header counts as a row, so two rows are needed)
    let mut one = SheetEngine::new("S");
    fill(&mut one, 0, 0, &[DB[0]]);
    fill(&mut one, 0, 6, &[&["Tree", "Height"], &["Maple", ">10"]]);
    assert_eq!(eval(&one, "=DSUM(A1:E1,\"Yield\",G1:H2)"), err(E::Value));
    // not a range
    assert_eq!(eval(&one, "=DSUM(5,\"Yield\",G1:H2)"), err(E::Value));
    assert_eq!(eval(&one, "=DSUM(A1:E2,\"Yield\",5)"), err(E::Value));

    // blank height, logical yield, error profit (record 1); the other Maple record still counts
    let variants: [(&str, [&str; 5]); 3] = [
        ("blank-or-logical", ["Maple", "", "TRUE", "FALSE", "#NAME?"]),
        ("text yield", ["Maple", "18", "20", "test", "105"]),
        ("blank yield", ["Maple", "18", "20", "", "105"]),
    ];
    // expected [DSUM, DAVERAGE, DCOUNT, DCOUNTA, DGET, DMAX, DMIN, DPRODUCT, DSTDEV, DSTDEVP, DVAR, DVARP] per variant
    let expected: [[V; 12]; 3] = [
        [
            num(10.0),
            num(10.0),
            num(1.0),
            num(1.0),
            num(10.0),
            num(10.0),
            num(10.0),
            num(10.0),
            err(E::DivByZero),
            num(0.0),
            err(E::DivByZero),
            num(0.0),
        ],
        [
            num(10.0),
            num(10.0),
            num(1.0),
            num(2.0),
            err(E::Num),
            num(10.0),
            num(10.0),
            num(10.0),
            err(E::DivByZero),
            num(0.0),
            err(E::DivByZero),
            num(0.0),
        ],
        [
            num(10.0),
            num(10.0),
            num(1.0),
            num(1.0),
            num(10.0),
            num(10.0),
            num(10.0),
            num(10.0),
            err(E::DivByZero),
            num(0.0),
            err(E::DivByZero),
            num(0.0),
        ],
    ];
    let names = [
        "DSUM", "DAVERAGE", "DCOUNT", "DCOUNTA", "DGET", "DMAX", "DMIN", "DPRODUCT", "DSTDEV",
        "DSTDEVP", "DVAR", "DVARP",
    ];
    for ((label, first), want) in variants.iter().zip(&expected) {
        let mut s = db_sheet();
        fill(&mut s, 1, 0, &[first]);
        for (name, expected) in names.iter().zip(want) {
            assert_eq!(
                &db_call(&s, name, "\"Yield\"", "G1:H2"),
                expected,
                "{label} {name}"
            );
        }
    }
}

#[test]
fn database_numbers_may_be_numeric_text_and_criteria_headers_are_case_blind() {
    let mut s = SheetEngine::new("S");
    fill(
        &mut s,
        0,
        0,
        &[&["Name", "Amount"], &["a", "=\"5\""], &["a", "7"]],
    );
    fill(&mut s, 0, 3, &[&["NAME"], &["A"]]);
    assert_eq!(eval(&s, "=DSUM(A1:B3,\"amount\",D1:D2)"), num(12.0));
    // an error cell in the criteria header is text that matches no field
    fill(&mut s, 0, 3, &[&["#N/A"], &["a"]]);
    assert_eq!(eval(&s, "=DSUM(A1:B3,\"amount\",D1:D2)"), num(0.0));
}

#[test]
fn database_functions_fail_closed_on_oversized_input() {
    let s = db_sheet();
    // a criteria block over 4,096 cells
    assert_eq!(eval(&s, "=DSUM(A1:E7,\"Yield\",A1:CP64)"), err(E::Value));
    // arity
    assert_eq!(eval(&s, "=DSUM(A1:E7,\"Yield\")"), err(E::Value));
    // the whole sheet as database is fine while a record costs one read (blank records fail the first term)
    let mut big = db_sheet();
    fill(&mut big, 0, 9, &[&["Tree", "Height"], &["Maple", ">10"]]);
    assert_eq!(
        eval(&big, "=DSUM(A1:XFD1048576,\"Yield\",J1:K2)"),
        num(24.0)
    );
    // 2,000 alternative criteria rows cost 2,000 reads per record: the evaluation budget ends the walk
    for r in 2..=2001 {
        big.set_cell(&format!("M{r}"), "x").unwrap();
    }
    big.set_cell("M1", "Tree").unwrap();
    assert_eq!(
        eval(&big, "=DSUM(A1:XFD1048576,\"Yield\",M1:M2001)"),
        err(E::Value)
    );
    // a field far outside the database
    assert_eq!(eval(&s, "=DSUM(A1:E7,100000,G1:H2)"), err(E::Value));
}

#[test]
fn database_follows_a_transposed_and_cross_sheet_database() {
    use easyoffice_sheet::WorkbookEngine;
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Data");
    wb.add_sheet("Calc");
    for (r, row) in DB.iter().enumerate() {
        for (c, raw) in row.iter().enumerate() {
            wb.set_cell(
                "Data",
                &PackedCellCoordinate::new(r as u32, c as u32)
                    .unwrap()
                    .to_a1(),
                *raw,
            )
            .unwrap();
        }
    }
    wb.set_cell("Calc", "A1", "Tree").unwrap();
    wb.set_cell("Calc", "B1", "Height").unwrap();
    wb.set_cell("Calc", "A2", "Maple").unwrap();
    wb.set_cell("Calc", "B2", ">10").unwrap();
    wb.set_cell("Calc", "D1", "=DSUM(Data!A1:E7,\"Yield\",A1:B2)")
        .unwrap();
    wb.set_cell("Calc", "D2", "=DSUM(TRANSPOSE(Data!A1:E7),\"Yield\",A1:B2)")
        .unwrap();
    assert_eq!(wb.evaluate_cell("Calc", "D1"), num(24.0));
    // the transposed block has 5 rows of 7 columns: its header row is the Tree column, no "Yield" header
    assert_eq!(wb.evaluate_cell("Calc", "D2"), err(E::Value));
}

// ---------------------------------------------------------------- matrix

fn grid(rows: &[&[&str]]) -> SheetEngine {
    let mut s = SheetEngine::new("S");
    fill(&mut s, 0, 0, rows);
    s
}

#[test]
fn mmult_multiplies_and_checks_shapes() {
    let mut s = grid(&[&["1", "2"], &["4", "5"]]);
    fill(&mut s, 0, 4, &[&["1", "2", "3"], &["4", "5", "6"]]);
    let expected = [[9.0, 12.0, 15.0], [24.0, 33.0, 42.0]];
    for (r, row) in expected.iter().enumerate() {
        for (c, want) in row.iter().enumerate() {
            let formula = format!("=INDEX(MMULT(A1:B2,E1:G2),{},{})", r + 1, c + 1);
            assert_eq!(eval(&s, &formula), num(*want), "{formula}");
        }
    }
    // a matrix function in a scalar cell gives its top-left element
    assert_eq!(eval(&s, "=MMULT(A1:B2,E1:G2)"), num(9.0));
    assert_eq!(eval(&s, "=SUM(MMULT(A1:B2,E1:G2))"), num(135.0));
    assert_eq!(eval(&s, "=MAX(MMULT(A1:B2,E1:G2))"), num(42.0));
    assert_eq!(
        eval(&s, "=SUMPRODUCT(MMULT(A1:B2,E1:G2),MMULT(A1:B2,E1:G2))"),
        num(81.0 + 144.0 + 225.0 + 576.0 + 1089.0 + 1764.0)
    );
    assert_eq!(eval(&s, "=MEDIAN(MMULT(A1:B2,E1:G2))"), num(19.5));
    // column count of the first is not the row count of the second
    fill(&mut s, 5, 0, &[&["1", "2"], &["3", "4"], &["5", "6"]]);
    assert_eq!(eval(&s, "=MMULT(A1:B2,A6:B8)"), err(E::Value));
    // operands: single values, text, logical, blank, errors
    assert_eq!(eval(&s, "=MMULT(3,4)"), num(12.0));
    assert_eq!(eval(&s, "=MMULT(\"test\",1)"), err(E::Value));
    assert_eq!(eval(&s, "=MMULT(\"3\",1)"), num(3.0));
    assert_eq!(eval(&s, "=MMULT(TRUE,1)"), err(E::Value));
    assert_eq!(eval(&s, "=MMULT(Z9,1)"), err(E::Value));
    assert_eq!(eval(&s, "=MMULT(#NAME?,1)"), err(E::Name));
    assert_eq!(eval(&s, "=MMULT(1,#NAME?)"), err(E::Name));
    assert_eq!(eval(&s, "=MMULT(A1:B2)"), err(E::Value));
    // composition
    assert_eq!(
        eval(&s, "=INDEX(MMULT(TRANSPOSE(A1:B2),A1:B2),1,1)"),
        num(17.0)
    );
    assert_eq!(eval(&s, "=MDETERM(MMULT(A1:B2,A1:B2))"), num(9.0));
}

#[test]
fn mdeterm_matches_the_reference() {
    let mut s = grid(&[&["1", "2", "3"], &["4", "5", "6"], &["7", "8", "9"]]);
    assert_eq!(eval(&s, "=MDETERM(A1:C3)"), num(0.0));
    fill(
        &mut s,
        0,
        5,
        &[&["1.5", "2", "3"], &["2", "5", "6"], &["7", "8", "9"]],
    );
    close(&eval(&s, "=MDETERM(F1:H3)"), -13.5, 1e-12);
    assert_eq!(eval(&s, "=MDETERM(1)"), num(1.0));
    assert_eq!(eval(&s, "=MDETERM(A1:B2)"), num(-3.0));
    assert_eq!(eval(&s, "=MDETERM(A1:C2)"), err(E::Value));
    assert_eq!(eval(&s, "=MDETERM(\"test\")"), err(E::Value));
    assert_eq!(eval(&s, "=MDETERM(TRUE)"), err(E::Value));
    assert_eq!(eval(&s, "=MDETERM(Z9)"), err(E::Value));
    assert_eq!(eval(&s, "=MDETERM(#NAME?)"), err(E::Name));
    // mixed cells: the first bad cell in reading order decides
    fill(
        &mut s,
        10,
        0,
        &[
            &["1", " ", "1.23", "TRUE", "FALSE", ""],
            &["0", "100", "2.34", "test", "-3.4", "#NAME?"],
        ],
    );
    assert_eq!(eval(&s, "=MDETERM(A11:F12)"), err(E::Value));
    assert_eq!(eval(&s, "=MDETERM(A1:C3,A1:C3)"), err(E::Value));
}

#[test]
fn mdeterm_of_an_identity_and_of_a_permutation() {
    let mut s = SheetEngine::new("S");
    for i in 0..30u32 {
        let at = PackedCellCoordinate::new(i, i).unwrap();
        s.set_cell(&at.to_a1(), "1").unwrap();
        // zeros everywhere else
        for j in 0..30u32 {
            if i != j {
                s.set_cell(&PackedCellCoordinate::new(i, j).unwrap().to_a1(), "0")
                    .unwrap();
            }
        }
    }
    assert_eq!(eval(&s, "=MDETERM(A1:AD30)"), num(1.0));
    assert_eq!(eval(&s, "=INDEX(MINVERSE(A1:AD30),30,30)"), num(1.0));
    assert_eq!(eval(&s, "=INDEX(MINVERSE(A1:AD30),1,30)"), num(0.0));
    // swapping two rows flips the sign
    let mut p = SheetEngine::new("S");
    fill(
        &mut p,
        0,
        0,
        &[&["0", "1", "0"], &["1", "0", "0"], &["0", "0", "1"]],
    );
    assert_eq!(eval(&p, "=MDETERM(A1:C3)"), num(-1.0));
}

#[test]
fn minverse_matches_the_reference() {
    let mut s = grid(&[&["1", "2", "3"], &["4", "5", "6"], &["7", "8", "9"]]);
    assert_eq!(eval(&s, "=MINVERSE(A1:C3)"), err(E::Num));
    fill(
        &mut s,
        0,
        5,
        &[&["1.5", "2", "3"], &["2", "5", "6"], &["7", "8", "9"]],
    );
    let want = [
        [0.222222222222, -0.444444444444, 0.222222222222],
        [-1.77777777778, 0.555555555556, 0.222222222222],
        [1.40740740741, -0.148148148148, -0.259259259259],
    ];
    for (r, row) in want.iter().enumerate() {
        for (c, x) in row.iter().enumerate() {
            let got = eval(&s, &format!("=INDEX(MINVERSE(F1:H3),{},{})", r + 1, c + 1));
            close(&got, *x, 1e-11);
        }
    }
    assert_eq!(eval(&s, "=INDEX(MINVERSE(1),1,1)"), num(1.0));
    assert_eq!(eval(&s, "=MINVERSE(0)"), err(E::Num));
    assert_eq!(eval(&s, "=MINVERSE(A1:C2)"), err(E::Value));
    assert_eq!(eval(&s, "=MINVERSE(\"test\")"), err(E::Value));
    assert_eq!(eval(&s, "=MINVERSE(TRUE)"), err(E::Value));
    assert_eq!(eval(&s, "=MINVERSE(Z9)"), err(E::Value));
    assert_eq!(eval(&s, "=MINVERSE(#NAME?)"), err(E::Name));
    // the product with the inverse is the identity
    for (r, c, want) in [
        (1, 1, 1.0),
        (2, 2, 1.0),
        (3, 3, 1.0),
        (1, 2, 0.0),
        (3, 1, 0.0),
    ] {
        let got = eval(&s, &format!("=INDEX(MMULT(F1:H3,MINVERSE(F1:H3)),{r},{c})"));
        let V::Number(n) = got else { panic!("{got:?}") };
        assert!((n - want).abs() < 1e-12, "({r},{c}) = {n}");
    }
}

#[test]
fn minverse_keeps_precision_on_a_nearly_singular_8x8() {
    let m: [[i32; 8]; 8] = [
        [0, 90, 161, 235, 307, 380, 452, 525],
        [-90, 0, 71, 145, 217, 290, 362, 435],
        [-161, -71, 0, 74, 146, 219, 291, 364],
        [-235, -145, -74, 0, 72, 145, 217, 290],
        [-307, -217, -146, -72, 0, 73, 145, 218],
        [-380, -290, -219, -145, -73, 0, 72, 145],
        [-452, -362, -291, -217, -145, -72, 0, 73],
        [-525, -435, -364, -290, -218, -145, -73, 0],
    ];
    let mut s = SheetEngine::new("S");
    for (r, row) in m.iter().enumerate() {
        for (c, x) in row.iter().enumerate() {
            s.set_cell(
                &PackedCellCoordinate::new(r as u32, c as u32)
                    .unwrap()
                    .to_a1(),
                x.to_string(),
            )
            .unwrap();
        }
    }
    let first_row = [
        20_312_366_176_490.105,
        3_127_499_741_229.562,
        -30_395_388_110_074.42,
        32_643_278_549_083.152,
        -51_212_808_262_633.19,
        4_691_249_611_844.312_5,
        781_874_935_307.398_1,
        20_051_927_358_753.35,
    ];
    for (c, want) in first_row.iter().enumerate() {
        let got = eval(&s, &format!("=INDEX(MINVERSE(A1:H8),1,{})", c + 1));
        close(&got, *want, 1e-12);
    }
}

#[test]
fn munit_builds_an_identity() {
    let s = SheetEngine::new("S");
    assert_eq!(eval(&s, "=INDEX(MUNIT(3),2,2)"), num(1.0));
    assert_eq!(eval(&s, "=INDEX(MUNIT(3),1,2)"), num(0.0));
    assert_eq!(eval(&s, "=SUM(MUNIT(3))"), num(3.0));
    assert_eq!(eval(&s, "=MUNIT(3)"), num(1.0));
    assert_eq!(eval(&s, "=INDEX(MUNIT(\"1.5\"),1,1)"), num(1.0));
    assert_eq!(eval(&s, "=INDEX(MUNIT(TRUE),1,1)"), num(1.0));
    assert_eq!(eval(&s, "=SUM(MUNIT(1.9))"), num(1.0));
    assert_eq!(eval(&s, "=MUNIT(\"test\")"), err(E::Value));
    assert_eq!(eval(&s, "=MUNIT(Z9)"), err(E::Value));
    assert_eq!(eval(&s, "=MUNIT(0)"), err(E::Value));
    assert_eq!(eval(&s, "=MUNIT(-2)"), err(E::Value));
    assert_eq!(eval(&s, "=MUNIT(#NAME?)"), err(E::Name));
    assert_eq!(eval(&s, "=INDEX(MUNIT(3),4,1)"), err(E::Ref));
}

#[test]
fn matrix_functions_fail_closed_on_oversized_input() {
    let s = SheetEngine::new("S");
    // 513 x 513 identity exceeds the cell limit, 257 x 257 the inversion limit
    assert_eq!(eval(&s, "=MUNIT(513)"), err(E::Value));
    assert_eq!(eval(&s, "=SUM(MUNIT(512))"), num(512.0));
    assert_eq!(eval(&s, "=MDETERM(MUNIT(257))"), err(E::Value));
    assert_eq!(eval(&s, "=MINVERSE(MUNIT(257))"), err(E::Value));
    assert_eq!(eval(&s, "=MDETERM(MUNIT(256))"), num(1.0));
    // 512x512 times 512x512 is 134M multiply-adds, over the 64M limit
    assert_eq!(eval(&s, "=MMULT(MUNIT(512),MUNIT(512))"), err(E::Value));
    assert_eq!(eval(&s, "=MMULT(MUNIT(400),MUNIT(400))"), num(1.0));
    // a huge range is refused before any cell is read
    assert_eq!(eval(&s, "=MDETERM(A1:XFD1048576)"), err(E::Value));
    assert_eq!(eval(&s, "=MMULT(A1:Z100000,A1:Z100000)"), err(E::Value));
}

#[test]
fn matrix_functions_feed_dependent_cells() {
    let mut s = grid(&[&["2", "0"], &["0", "4"]]);
    s.set_cell("D1", "=INDEX(MINVERSE(A1:B2),2,2)").unwrap();
    s.set_cell("D2", "=D1*8").unwrap();
    assert_eq!(s.evaluate_cell("D2"), num(2.0));
    s.set_cell("B2", "8").unwrap();
    assert_eq!(s.evaluate_cell("D2"), num(1.0));
}

// ---------------------------------------------------------------- registry

#[test]
fn the_registry_lists_every_function_and_each_one_is_implemented() {
    let names = SheetEngine::supported_function_names();
    assert_eq!(names.len(), 144 + 59);
    for name in names {
        // a registered name never evaluates to #NAME?
        let got = eval(&SheetEngine::new("S"), &format!("={name}()"));
        assert_ne!(got, err(E::Name), "{name} has no implementation");
    }
    for name in [
        "LOOKUP",
        "HYPERLINK",
        "ISFORMULA",
        "FORMULATEXT",
        "ADDRESS",
        "AREAS",
        "DAVERAGE",
        "DCOUNT",
        "DCOUNTA",
        "DGET",
        "DMAX",
        "DMIN",
        "DPRODUCT",
        "DSTDEV",
        "DSTDEVP",
        "DSUM",
        "DVAR",
        "DVARP",
        "MMULT",
        "MDETERM",
        "MINVERSE",
        "MUNIT",
    ] {
        assert!(names.contains(&name), "{name} is not registered");
    }
    let mut sorted: Vec<&str> = names.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), names.len(), "duplicate names in the registry");
}

//! Function parity bundle (PD-270): the functions that were missing (`TEXTSPLIT`, `ARRAYTOTEXT`,
//! `REGEXTEST`, `REGEXEXTRACT`, `REGEXREPLACE`, `TRIMRANGE`, `ISREF`, `SHEET`, `SHEETS`, `CELL`,
//! `INFO`, `PERCENTOF`, `ISO.CEILING`, `AMORDEGRC`) and the accuracy fixes of existing functions.
//! The values are the ones of the functions' documented examples, or of the reference engine's
//! tests (`DIFFERS` marks a case where the documented behaviour is followed instead).
//! Hostile inputs (many delimiters, huge patterns, long results) have to fail closed.
#![allow(clippy::unwrap_used)]

mod dynamic_support;
use dynamic_support::new_empty_model;

fn eval(formula: &str) -> String {
    let mut model = new_empty_model();
    model._set("A1", formula);
    model.evaluate();
    model._get_text("A1")
}

#[test]
fn textsplit_cuts_by_columns_and_rows() {
    let mut model = new_empty_model();
    model._set("A1", "=TEXTSPLIT(\"a,b;c,d\",\",\",\";\")");
    model._set("A5", "=TEXTSPLIT(\"a1b2c\",{\"1\",\"2\"})");
    model._set("A6", "=TEXTSPLIT(\"a,,b\",\",\",,TRUE)");
    model._set("A7", "=TEXTSPLIT(\"aXbxc\",\"x\",,,1)");
    model._set("A8", "=TEXTSPLIT(\"a,b;c\",\",\",\";\",FALSE,0,\"-\")");
    model.evaluate();
    assert_eq!(model._get_text("A1"), "a");
    assert_eq!(model._get_text("B1"), "b");
    assert_eq!(model._get_text("A2"), "c");
    assert_eq!(model._get_text("B2"), "d");
    assert_eq!(model._get_text("C5"), "c");
    assert_eq!(model._get_text("B6"), "b");
    assert_eq!(model._get_text("C6"), "");
    assert_eq!(model._get_text("C7"), "c");
    assert_eq!(model._get_text("B9"), "-");
}

#[test]
fn textsplit_rejects_empty_and_missing_delimiters() {
    assert_eq!(eval("=TEXTSPLIT(\"a b\",\"\")"), "#VALUE!");
    assert_eq!(eval("=TEXTSPLIT(\"a b\",,)"), "#VALUE!");
    assert_eq!(eval("=TEXTSPLIT(\"a b\")"), "#VALUE!");
    assert_eq!(eval("=TEXTSPLIT(\"a b\",#N/A)"), "#N/A");
}

#[test]
fn textsplit_fails_closed_on_hostile_input() {
    // 20,000 characters against 300 delimiters is past the work budget.
    let mut model = new_empty_model();
    model._set("A1", "=REPT(\"ab\",10000)");
    model._set("B1", "=TEXTSPLIT(A1,SEQUENCE(300)&\"x\")");
    // 20,001 columns are past the sheet width, so the array cannot be placed.
    model._set("C1", "=TEXTSPLIT(REPT(\",\",20000),\",\")");
    model.evaluate();
    assert_eq!(model._get_text("B1"), "#VALUE!");
    assert_eq!(model._get_text("C1"), "#SPILL!");
}

#[test]
fn arraytotext_joins_and_quotes() {
    assert_eq!(
        eval("=ARRAYTOTEXT({1,\"hello\";2,\"world\"})"),
        "1, hello, 2, world"
    );
    assert_eq!(
        eval("=ARRAYTOTEXT({1,\"hello\";2,\"world\"},1)"),
        "{1,\"hello\";2,\"world\"}"
    );
    assert_eq!(eval("=ARRAYTOTEXT(\"a\"\"b\",1)"), "\"a\"\"b\"");
    assert_eq!(eval("=ARRAYTOTEXT({1,2},2)"), "#VALUE!");
    assert_eq!(eval("=ARRAYTOTEXT({1,#N/A},1)"), "{1,#N/A}");
    assert_eq!(eval("=ARRAYTOTEXT(#DIV/0!)"), "#DIV/0!");
}

#[test]
fn arraytotext_stops_at_the_cell_text_size() {
    assert_eq!(
        eval("=ARRAYTOTEXT(SEQUENCE(5000,1,10000))"),
        "#VALUE!",
        "5,000 numbers of 5 digits and a separator are 35,000 characters"
    );
}

#[test]
fn trimrange_cuts_blank_edges() {
    let mut model = new_empty_model();
    model._set("B2", "1");
    model._set("C3", "2");
    model._set("E1", "=TRIMRANGE(A1:D4)");
    model._set("E6", "=TRIMRANGE(A1:D4,1,0)");
    model._set("E12", "=TRIMRANGE(A10:B11)");
    model._set("E14", "=TRIMRANGE(A1:D4,4)");
    model.evaluate();
    assert_eq!(model._get_text("E1"), "1");
    assert_eq!(model._get_text("F2"), "2");
    // a blank place of the result array reads 0
    assert_eq!(model._get_text("F1"), "0");
    // trimming only the leading blanks keeps the trailing column and row
    assert_eq!(model._get_text("H6"), "0");
    assert_eq!(model._get_text("H9"), "");
    assert_eq!(model._get_text("E12"), "#REF!");
    assert_eq!(model._get_text("E14"), "#VALUE!");
}

#[test]
fn isref_sheet_and_sheets_read_references() {
    let mut model = new_empty_model();
    model._set("A1", "5");
    model._set("B1", "=ISREF(A1)");
    model._set("B2", "=ISREF(5)");
    model._set("B3", "=ISREF(OFFSET(A1,1,1))");
    model._set("B4", "=ISREF(INDIRECT(\"A1\"))");
    model._set("B5", "=ISREF(INDIRECT(\"nonsense\"))");
    model._set("B6", "=ISREF(FILTER({1,2,3},{1,0,1}))");
    model._set("C1", "=SHEET()");
    model._set("C2", "=SHEET(Sheet2!A1)");
    model._set("C3", "=SHEET(\"Sheet2\")");
    model._set("C4", "=SHEET(\"missing\")");
    model._set("C5", "=SHEETS()");
    model._set("C6", "=SHEETS(A1:B2)");
    model.evaluate();
    let texts: Vec<String> = (1..=6).map(|r| model._get_text(&format!("B{r}"))).collect();
    assert_eq!(texts, ["TRUE", "FALSE", "TRUE", "TRUE", "FALSE", "FALSE"]);
    let texts: Vec<String> = (1..=6).map(|r| model._get_text(&format!("C{r}"))).collect();
    assert_eq!(texts, ["1", "2", "2", "#N/A", "2", "1"]);
}

#[test]
fn cell_and_info_report_a_cell_and_the_workbook() {
    let mut model = new_empty_model();
    model._set("B3", "hello");
    model._set("C1", "=CELL(\"address\",B3)");
    model._set("C2", "=CELL(\"row\",B3)");
    model._set("C3", "=CELL(\"col\",B3)");
    model._set("C4", "=CELL(\"type\",B3)");
    model._set("C5", "=CELL(\"type\",Z9)");
    model._set("C6", "=CELL(\"contents\",B3)");
    model._set("C7", "=CELL(\"nonsense\",B3)");
    model._set("D1", "=INFO(\"recalc\")");
    model._set("D2", "=INFO(\"numfile\")");
    model._set("D3", "=INFO(\"nonsense\")");
    model._set("D4", "=INFO(\"release\")");
    model.evaluate();
    let texts: Vec<String> = (1..=7).map(|r| model._get_text(&format!("C{r}"))).collect();
    assert_eq!(texts, ["$B$3", "3", "2", "l", "b", "hello", "#VALUE!"]);
    let texts: Vec<String> = (1..=4).map(|r| model._get_text(&format!("D{r}"))).collect();
    assert_eq!(texts, ["Automatic", "2", "#VALUE!", "16.0"]);
}

#[test]
fn percentof_iso_ceiling_and_amordegrc() {
    assert_eq!(eval("=PERCENTOF(2,10)"), "0.2");
    assert_eq!(eval("=PERCENTOF({1,2},{1,2,3,4})"), "0.3");
    assert_eq!(eval("=PERCENTOF(5,0)"), "#DIV/0!");
    // the documented examples of ISO.CEILING
    assert_eq!(eval("=ISO.CEILING(4.3)"), "5");
    assert_eq!(eval("=ISO.CEILING(-4.3)"), "-4");
    assert_eq!(eval("=ISO.CEILING(4.3,-2)"), "6");
    assert_eq!(eval("=ISO.CEILING(-4.3,-2)"), "-4");
    // the documented example of AMORDEGRC
    assert_eq!(
        eval("=AMORDEGRC(2400,DATE(2008,8,19),DATE(2008,12,31),300,1,0.15,1)"),
        "776"
    );
    assert_eq!(
        eval("=AMORDEGRC(2400,DATE(2008,8,19),DATE(2008,12,31),300,1,0.4,1)"),
        "#NUM!",
        "an asset life between 0 and 3 years is not allowed"
    );
    assert_eq!(
        eval("=AMORDEGRC(2400,DATE(2008,12,31),DATE(2008,8,19),300,1,0.15,1)"),
        "#NUM!"
    );
}

#[test]
fn array_arguments_lift_over_scalar_functions() {
    let mut model = new_empty_model();
    model._set("A1", "13");
    model._set("A2", "25");
    model._set("B1", "=BITAND(A1:A2,12)");
    model._set("D1", "=UNICHAR({72,105})");
    model._set("D3", "=WEEKDAY({44561,44562},2)");
    model._set("D5", "=NORM.S.DIST({0,1},TRUE)");
    model.evaluate();
    assert_eq!(model._get_text("B1"), "12");
    assert_eq!(model._get_text("B2"), "8");
    assert_eq!(model._get_text("D1"), "H");
    assert_eq!(model._get_text("E1"), "i");
    assert_eq!(model._get_text("D3"), "5");
    assert_eq!(model._get_text("E3"), "6");
    assert_eq!(model._get_text("D5"), "0.5");
    assert_eq!(model._get_text("E5"), "0.841344746068543");
}

#[test]
fn address_writes_the_relative_number_in_brackets() {
    // DIFFERS: the documented example `ADDRESS(2,3,2,FALSE)` is `R2C[3]` (the reference engine and this
    // engine used to count from the formula cell).
    assert_eq!(eval("=ADDRESS(2,3,2,FALSE)"), "R2C[3]");
    assert_eq!(eval("=ADDRESS(2,3,3,FALSE)"), "R[2]C3");
    assert_eq!(eval("=ADDRESS(2,3,1,FALSE)"), "R2C3");
}

#[test]
fn type_of_an_array_is_64() {
    assert_eq!(eval("=TYPE({1,2,3})"), "64");
    assert_eq!(eval("=TYPE(1)"), "1");
    assert_eq!(eval("=TYPE(\"a\")"), "2");
    assert_eq!(eval("=TYPE(#N/A)"), "16");
}

#[test]
fn lookup_index_arguments_follow_the_documentation() {
    let mut model = new_empty_model();
    for (cell, value) in [("A1", "1"), ("B1", "2"), ("A2", "3"), ("B2", "4")] {
        model._set(cell, value);
    }
    model._set("D1", "=VLOOKUP(1,A1:B2,0,FALSE)");
    model._set("D2", "=VLOOKUP(1,A1:B2,3,FALSE)");
    model._set("D3", "=HLOOKUP(1,A1:B2,0,FALSE)");
    model._set("D4", "=SUM(INDEX(A1:B2,0,2))");
    model._set("D5", "=SUM(INDEX(A1:B2,2,0))");
    model._set("D6", "=SUM(INDEX(A1:B2,0,0))");
    model._set("D7", "=INDEX(A1:B2,-1,1)");
    model._set("D8", "=INDEX(A1:B2,3,1)");
    model._set("E1", "=INDEX(A1:B2,0,2)");
    model._set("G1", "=INDEX({1,2;3,4},2)");
    model._set("G3", "=SUM(INDEX({1,2;3,4},0,2))");
    model._set("G4", "=INDEX({1,2,3},2)");
    model.evaluate();
    let texts: Vec<String> = (1..=8).map(|r| model._get_text(&format!("D{r}"))).collect();
    assert_eq!(
        texts,
        ["#VALUE!", "#REF!", "#VALUE!", "6", "7", "10", "#VALUE!", "#REF!"]
    );
    assert_eq!(model._get_text("E1"), "2");
    assert_eq!(model._get_text("E2"), "4");
    assert_eq!(model._get_text("G1"), "3");
    assert_eq!(model._get_text("H1"), "4");
    assert_eq!(model._get_text("G3"), "6");
    assert_eq!(model._get_text("G4"), "2");
}

#[test]
fn lookup_reads_a_result_vector_that_lies_the_other_way() {
    let mut model = new_empty_model();
    for (cell, value) in [("A1", "10"), ("A2", "20"), ("A3", "30")] {
        model._set(cell, value);
    }
    for (cell, value) in [("C1", "a"), ("D1", "b"), ("E1", "c")] {
        model._set(cell, value);
    }
    model._set("G1", "=LOOKUP(30,A1:A3,C1:E1)");
    model._set("G2", "=LOOKUP(20,A1:A3,C1:E1)");
    model._set("G3", "=LOOKUP(20,A1:A3,C1:D1)");
    model.evaluate();
    assert_eq!(model._get_text("G1"), "c");
    assert_eq!(model._get_text("G2"), "b");
    assert_eq!(model._get_text("G3"), "#REF!");
}

#[test]
fn weekday_return_types_11_to_17_and_the_date_functions() {
    // 44561 is Friday 31 December 2021
    let types: Vec<String> = (11..=17)
        .map(|t| eval(&format!("=WEEKDAY(44561,{t})")))
        .collect();
    assert_eq!(types, ["5", "4", "3", "2", "1", "7", "6"]);
    assert_eq!(eval("=WEEKDAY(44561,10)"), "#NUM!");
    let mut model = new_empty_model();
    model._set("B1", "foo");
    model._set("A1", "=NETWORKDAYS(44561,44570,B1)");
    model._set("A2", "=NETWORKDAYS.INTL(44561,44570,\"111110\")");
    model._set("A3", "=NETWORKDAYS.INTL(44561,44570,C1)");
    model._set("A4", "=NETWORKDAYS.INTL(44561,44570,\"0000011\")");
    model._set("A5", "=WORKDAY.INTL(44561,2,C1)");
    model.evaluate();
    assert_eq!(model._get_text("A1"), "#VALUE!");
    assert_eq!(model._get_text("A2"), "#VALUE!");
    assert_eq!(model._get_text("A3"), "#NUM!");
    assert_eq!(model._get_text("A4"), "6");
    assert_eq!(model._get_text("A5"), "#NUM!");
}

#[test]
fn base_decimal_char_and_code_limits() {
    assert_eq!(eval("=BASE(-0.5,2)"), "#NUM!");
    assert_eq!(eval("=BASE(7,2)"), "111");
    assert_eq!(eval("=DECIMAL(\"\",2)"), "0");
    assert_eq!(eval("=DECIMAL(REPT(\"0\",255)&\"1\",2)"), "#VALUE!");
    assert_eq!(eval("=DECIMAL(\"FF\",16)"), "255");
    assert_eq!(eval("=CHAR(300)"), "#VALUE!");
    assert_eq!(eval("=CHAR(65)"), "A");
    assert_eq!(eval("=UNICHAR(300)"), "Ĭ");
    assert_eq!(eval("=CODE(CHAR(128))"), "128");
    assert_eq!(eval("=CHAR(128)"), "€");
    assert_eq!(eval("=CODE(\"€\")"), "128");
    assert_eq!(eval("=UNICODE(\"€\")"), "8364");
}

#[test]
fn iferror_with_a_shorter_value_array_takes_the_fallback_for_the_missing_places() {
    let mut model = new_empty_model();
    model._set("A1", "=IFERROR({1,#DIV/0!},{10,20,30})");
    model._set("A3", "=IFNA({1,#N/A},{10,20,30})");
    model.evaluate();
    assert_eq!(model._get_text("A1"), "1");
    assert_eq!(model._get_text("B1"), "20");
    assert_eq!(model._get_text("C1"), "30");
    assert_eq!(model._get_text("C3"), "30");
}

#[test]
fn accrintm_has_an_optional_par_and_needs_issue_before_settlement() {
    // the documented example: 4/1/2008 to 6/15/2008, rate 0.1, par 1000, actual/365
    assert_eq!(
        eval("=ROUND(ACCRINTM(DATE(2008,4,1),DATE(2008,6,15),0.1,1000,3),8)"),
        "20.54794521"
    );
    assert_eq!(
        eval("=ROUND(ACCRINTM(DATE(2008,4,1),DATE(2008,6,15),0.1),4)"),
        "20.5556"
    );
    assert_eq!(
        eval("=ACCRINTM(DATE(2008,4,1),DATE(2008,4,1),0.1,1000,3)"),
        "#NUM!"
    );
}

#[test]
fn indirect_reads_r1c1_text() {
    let mut model = new_empty_model();
    model._set("A1", "1");
    model._set("A2", "2");
    model._set("C2", "20");
    model._set("D5", "=INDIRECT(\"R2C3\",FALSE)");
    model._set("D6", "=INDIRECT(\"R[-4]C[-1]\",FALSE)");
    model._set("D7", "=SUM(INDIRECT(\"R1C1:R2C1\",FALSE))");
    model._set("D8", "=INDIRECT(\"R0C1\",FALSE)");
    model._set("D9", "=INDIRECT(\"R2C3\",1)");
    model.evaluate();
    assert_eq!(model._get_text("D5"), "20");
    assert_eq!(model._get_text("D6"), "20");
    assert_eq!(model._get_text("D7"), "3");
    assert_eq!(model._get_text("D8"), "#REF!");
    assert_eq!(model._get_text("D9"), "#REF!");
}

#[test]
fn an_empty_cell_in_an_array_reads_as_blank_for_one_value_functions() {
    let mut model = new_empty_model();
    model._set("A1", "1");
    model._set("A3", "3");
    model._set("B1", "=SUM(ABS(A1:A3))");
    model._set("C1", "=NOT(A1:A3)");
    model._set("D1", "=LEN(A1:A3)");
    model._set("E1", "=ISBLANK(A1:A3)");
    model._set("F1", "=ISNUMBER(A1:A3)");
    model._set("G1", "=ROUND(A1:A3,1)");
    model.evaluate();
    assert_eq!(model._get_text("B1"), "4");
    assert_eq!(model._get_text("C2"), "TRUE");
    assert_eq!(model._get_text("D2"), "0");
    assert_eq!(model._get_text("E2"), "TRUE");
    assert_eq!(model._get_text("F2"), "FALSE");
    assert_eq!(model._get_text("G2"), "0");
}

#[test]
fn regextest_and_regexextract() {
    assert_eq!(eval("=REGEXTEST(\"abc123\",\"^[a-z0-9]+$\")"), "TRUE");
    assert_eq!(eval("=REGEXTEST(\"Hello\",\"hello\")"), "FALSE");
    assert_eq!(eval("=REGEXTEST(\"Hello\",\"hello\",1)"), "TRUE");
    assert_eq!(eval("=REGEXTEST(\"text\",\"[\")"), "#VALUE!");
    assert_eq!(eval("=REGEXEXTRACT(\"foo 42 bar 7\",\"[0-9]+\")"), "42");
    assert_eq!(eval("=REGEXEXTRACT(\"nothing\",\"[0-9]+\")"), "#N/A");
    assert_eq!(
        eval("=REGEXEXTRACT(\"2024-01-15\",\"(\\d+)-(\\d+)\",2)"),
        "2024"
    );
    // DIFFERS: mode 0 gives the whole first match, not the first group.
    assert_eq!(
        eval("=REGEXEXTRACT(\"2024-01-15\",\"(\\d+)-\\d+\")"),
        "2024-01"
    );
    let mut model = new_empty_model();
    model._set("A1", "=REGEXEXTRACT(\"LonelyPlanet\",\"[A-Z][a-z]+\",1)");
    model.evaluate();
    assert_eq!(model._get_text("A1"), "Lonely");
    assert_eq!(model._get_text("B1"), "Planet");
}

#[test]
fn regexreplace_replaces_all_or_one_match() {
    assert_eq!(eval("=REGEXREPLACE(\"aabbcc\",\"[a-c]\",\"x\")"), "xxxxxx");
    assert_eq!(
        eval("=REGEXREPLACE(\"a1b2c3\",\"[0-9]\",\"#\",2)"),
        "a1b#c3"
    );
    assert_eq!(
        eval("=REGEXREPLACE(\"a1b2c3\",\"[0-9]\",\"#\",-1)"),
        "a1b2c#"
    );
    assert_eq!(
        eval("=REGEXREPLACE(\"a1b2c3\",\"[0-9]\",\"#\",9)"),
        "a1b2c3"
    );
    assert_eq!(
        eval("=REGEXREPLACE(\"john smith\",\"(\\w+) (\\w+)\",\"$2 $1\")"),
        "smith john"
    );
    assert_eq!(eval("=REGEXREPLACE(\"text\",\"[\",\"x\")"), "#VALUE!");
}

#[test]
fn regex_functions_fail_closed_on_hostile_patterns() {
    let mut model = new_empty_model();
    // A catastrophic-backtracking pattern runs in linear time in this engine.
    model._set("A1", "=REGEXTEST(REPT(\"a\",5000)&\"b\",\"^(a+)+$\")");
    // A pattern past the length limit, a counted repeat past the program size limit and a deep
    // nesting are `#VALUE!`.
    model._set("A2", "=REGEXTEST(\"a\",REPT(\"a\",5000))");
    model._set(
        "A3",
        "=REGEXTEST(\"a\",\"(((a{1000}){1000}){1000}){1000}\")",
    );
    model._set(
        "A4",
        "=REGEXTEST(\"a\",REPT(\"(\",100)&\"a\"&REPT(\")\",100))",
    );
    // A replacement that grows the text past the cell limit is `#VALUE!`.
    model._set(
        "A5",
        "=REGEXREPLACE(REPT(\"a\",20000),\"a\",REPT(\"b\",100))",
    );
    model.evaluate();
    assert_eq!(model._get_text("A1"), "FALSE");
    assert_eq!(model._get_text("A2"), "#VALUE!");
    assert_eq!(model._get_text("A3"), "#VALUE!");
    assert_eq!(model._get_text("A4"), "#VALUE!");
    assert_eq!(model._get_text("A5"), "#VALUE!");
}

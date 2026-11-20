//! `WEBSERVICE`, `RTD` and `FILTERXML` (PD-271): names that are known and not served. The results are the ones
//! the function documentation names: `#VALUE!` for `WEBSERVICE` ("unable to return the data") and
//! `#N/A` for `RTD` (no real-time data server). Neither reaches out.
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
fn webservice_and_rtd_give_their_documented_errors() {
    assert_eq!(
        eval("=WEBSERVICE(\"https://example.invalid/x\")"),
        "#VALUE!"
    );
    assert_eq!(eval("=RTD(\"prog.id\",\"\",\"topic\")"), "#N/A");
    assert_eq!(eval("=RTD(\"prog.id\",,\"a\",\"b\")"), "#N/A");
    // the arguments are read first, and the count is checked
    assert_eq!(eval("=WEBSERVICE(#REF!)"), "#REF!");
    assert_eq!(eval("=RTD(\"p\",\"\",#DIV/0!)"), "#DIV/0!");
    assert_eq!(eval("=WEBSERVICE()"), "#VALUE!");
    assert_eq!(eval("=WEBSERVICE(\"a\",\"b\")"), "#VALUE!");
    assert_eq!(eval("=RTD(\"p\",\"\")"), "#VALUE!");
}

#[test]
fn filterxml_is_known_and_gives_the_documented_value_error() {
    assert_eq!(eval("=FILTERXML(\"<a><b>1</b></a>\",\"//b\")"), "#VALUE!");
    assert_eq!(eval("=FILTERXML(\"<a>\",\"//b\")"), "#VALUE!");
    assert_eq!(eval("=FILTERXML(#N/A,\"//b\")"), "#N/A");
    assert_eq!(eval("=FILTERXML(\"<a/>\")"), "#VALUE!");
}

#[test]
fn the_other_service_functions_stay_unknown() {
    for formula in [
        "=STOCKHISTORY(\"X\",DATE(2026,1,1))",
        "=DETECTLANGUAGE(\"x\")",
        "=TRANSLATE(\"x\",\"en\")",
        "=CUBEVALUE(\"c\")",
        "=IMAGE(\"https://example.invalid/a.png\")",
    ] {
        assert_eq!(eval(formula), "#NAME?", "{formula}");
    }
}

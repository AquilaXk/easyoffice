//! `WEBSERVICE`, `RTD`, `IMAGE`, `CALL`, `REGISTER.ID` and `STOCKHISTORY` (PD-271, PD-318):
//! names that are known and not served offline or under workbook security policy.
#![allow(clippy::unwrap_used)]

mod dynamic_support;
use dynamic_support::new_empty_model;
use easyoffice_sheet::{pivot_error_of, pivot_error_value, FormulaError};

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
fn blocked_and_connect_functions_give_documented_errors_and_check_args() {
    // IMAGE(source, [alt_text], [sizing], [height], [width]) -> #BLOCKED!
    assert_eq!(
        eval("=IMAGE(\"https://example.invalid/a.png\")"),
        "#BLOCKED!"
    );
    assert_eq!(
        eval("=IMAGE(\"https://example.invalid/a.png\",\"그림\",,120,120)"),
        "#BLOCKED!"
    );
    assert_eq!(
        eval("=IMAGE(\"https://example.invalid/a.png\",\"그림\",3,120,120)"),
        "#BLOCKED!"
    );
    assert_eq!(eval("=IMAGE()"), "#VALUE!");
    assert_eq!(eval("=IMAGE(,\"alt\")"), "#VALUE!");
    assert_eq!(eval("=IMAGE(\"a\",\"b\",0,1,2,3)"), "#VALUE!");
    assert_eq!(eval("=IMAGE(#REF!)"), "#REF!");

    // CALL(register_id_or_module, ...) -> #BLOCKED!
    assert_eq!(eval("=CALL(1)"), "#BLOCKED!");
    assert_eq!(
        eval("=CALL(\"Kernel32\",\"GetTickCount\",\"J\")"),
        "#BLOCKED!"
    );
    assert_eq!(eval("=CALL()"), "#VALUE!");
    assert_eq!(eval("=CALL(,1)"), "#VALUE!");
    assert_eq!(eval("=CALL(#DIV/0!)"), "#DIV/0!");

    // REGISTER.ID(module_text, procedure, [type_text]) -> #BLOCKED!
    assert_eq!(
        eval("=REGISTER.ID(\"Kernel32\",\"GetTickCount\")"),
        "#BLOCKED!"
    );
    assert_eq!(
        eval("=REGISTER.ID(\"Kernel32\",\"GetTickCount\",\"J\")"),
        "#BLOCKED!"
    );
    assert_eq!(eval("=REGISTER.ID(\"Kernel32\")"), "#VALUE!");
    assert_eq!(eval("=REGISTER.ID(,\"Proc\")"), "#VALUE!");
    assert_eq!(eval("=REGISTER.ID(\"m\",\"p\",\"t\",1)"), "#VALUE!");
    assert_eq!(eval("=REGISTER.ID(\"m\",#N/A)"), "#N/A");

    // STOCKHISTORY(stock, start_date, [end_date], [interval], [headers], [property0..5]) -> #CONNECT!
    assert_eq!(eval("=STOCKHISTORY(\"MSFT\",DATE(2026,1,1))"), "#CONNECT!");
    assert_eq!(
        eval("=STOCKHISTORY(\"MSFT\",DATE(2026,1,1),DATE(2026,1,31),,1,0,1)"),
        "#CONNECT!"
    );
    assert_eq!(eval("=STOCKHISTORY(\"MSFT\")"), "#VALUE!");
    assert_eq!(eval("=STOCKHISTORY(,DATE(2026,1,1))"), "#VALUE!");
    assert_eq!(
        eval("=STOCKHISTORY(\"X\",1,2,0,1,0,1,2,3,4,5,6)"),
        "#VALUE!"
    );
    assert_eq!(eval("=STOCKHISTORY(\"X\",#NAME?)"), "#NAME?");
}

#[test]
fn blocked_and_connect_literals_info_logical_array_and_pivot_work() {
    assert_eq!(eval("=#CONNECT!"), "#CONNECT!");
    assert_eq!(eval("=#BLOCKED!"), "#BLOCKED!");
    assert_eq!(eval("=ERROR.TYPE(#CONNECT!)"), "10");
    assert_eq!(eval("=ERROR.TYPE(#BLOCKED!)"), "11");
    assert_eq!(
        eval("=ERROR.TYPE(IMAGE(\"https://example.invalid/a.png\"))"),
        "11"
    );
    assert_eq!(
        eval("=ERROR.TYPE(STOCKHISTORY(\"X\",DATE(2026,1,1)))"),
        "10"
    );

    assert_eq!(eval("=ISERROR(#CONNECT!)"), "TRUE");
    assert_eq!(eval("=ISERROR(#BLOCKED!)"), "TRUE");
    assert_eq!(eval("=ISERR(#CONNECT!)"), "TRUE");
    assert_eq!(eval("=ISERR(#BLOCKED!)"), "TRUE");
    assert_eq!(eval("=ISNA(#CONNECT!)"), "FALSE");
    assert_eq!(eval("=ISNA(#BLOCKED!)"), "FALSE");

    assert_eq!(
        eval("=IFERROR(IMAGE(\"https://example.invalid/a.png\"),\"차단\")"),
        "차단"
    );
    assert_eq!(
        eval("=IFERROR(STOCKHISTORY(\"X\",DATE(2026,1,1)),\"오프라인\")"),
        "오프라인"
    );
    assert_eq!(eval("=INDEX({#CONNECT!,#BLOCKED!},1,2)"), "#BLOCKED!");
    assert_eq!(eval("=TYPE(#BLOCKED!)"), "16");
    assert_eq!(eval("=TYPE(#CONNECT!)"), "16");

    assert_eq!(
        FormulaError::from_code("#CONNECT!"),
        Some(FormulaError::Connect)
    );
    assert_eq!(
        FormulaError::from_code("#blocked!"),
        Some(FormulaError::Blocked)
    );
    assert_eq!(
        pivot_error_of(pivot_error_value(FormulaError::Connect)),
        Some(FormulaError::Connect)
    );
    assert_eq!(
        pivot_error_of(pivot_error_value(FormulaError::Blocked)),
        Some(FormulaError::Blocked)
    );
}

#[test]
fn the_other_service_functions_stay_unknown() {
    for formula in [
        "=DETECTLANGUAGE(\"x\")",
        "=TRANSLATE(\"x\",\"en\")",
        "=CUBEVALUE(\"c\")",
        "=CUBEMEMBER(\"c\",\"m\")",
    ] {
        assert_eq!(eval(formula), "#NAME?", "{formula}");
    }
}

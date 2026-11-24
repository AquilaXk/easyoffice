//! PD-335: DATEVALUE/TIMEVALUE whitespace & `24:00:00` rollover, Korean/ISO `DATEDIF` & numeric
//! coercion, and standard `#NUM!` representation for circular references (`FormulaError::Cycle`).
#![allow(clippy::unwrap_used)]

use easyoffice_sheet::{
    enter_date_system, EvaluatedCellValue, FormulaError, PackedCellCoordinate, SheetEngine,
};

fn eval(engine: &SheetEngine, formula: &str) -> EvaluatedCellValue {
    engine.evaluate_formula_str(formula)
}

#[test]
fn datevalue_spaces_iso_t_korean_2digit_and_24h_rollover() {
    let engine = SheetEngine::new("DateParsePd335");
    let jan1 = eval(&engine, "=DATE(2026,1,1)");
    let jan2 = eval(&engine, "=DATE(2026,1,2)");

    for expr in [
        "=DATEVALUE(\"2026 - 01 - 01\")",
        "=DATEVALUE(\"2026 / 01 / 01\")",
        "=DATEVALUE(\"2026 . 01 . 01.\")",
        "=DATEVALUE(\"2026-\"&REPT(\" \",32000)&\"01-01\")",
        "=DATEVALUE(\"2026-01-01T12:00:00\")",
        "=DATEVALUE(\"2026-01-01 23:59:59\")",
        "=DATEVALUE(\"26년 1월 1일\")",
    ] {
        assert_eq!(eval(&engine, expr), jan1, "{expr}");
    }

    for expr in [
        "=DATEVALUE(\"2026-01-01 24:00:00\")",
        "=DATEVALUE(\"2026-01-01 25:00:00\")",
        "=DATEVALUE(\"2026-01-01T24:00:00\")",
        "=DATEVALUE(\"2026-01-01 23:60:00\")",
        "=DATEVALUE(\"2026-01-01 23:59:60\")",
        "=DATEVALUE(\"2026년 1월 1일 24:00:00\")",
        "=DATEVALUE(\"26년 1월 1일 24:00:00\")",
    ] {
        assert_eq!(eval(&engine, expr), jan2, "{expr}");
    }

    assert_eq!(
        eval(&engine, "=DATEVALUE(\"99년 12월 31일\")"),
        eval(&engine, "=DATE(1999,12,31)")
    );
    assert_eq!(
        eval(&engine, "=DATEVALUE(\"29년 1월 1일\")"),
        eval(&engine, "=DATE(2029,1,1)")
    );
    assert_eq!(
        eval(&engine, "=DATEVALUE(\"30년 1월 1일\")"),
        eval(&engine, "=DATE(1930,1,1)")
    );

    for bad in [
        "=DATEVALUE(\"9999-12-31 24:00:00\")",
        "=DATEVALUE(\"2026-01-01T\")",
        "=DATEVALUE(\"2026-01-01T 12:00\")",
        "=DATEVALUE(\"2026-01-01 foo\")",
        "=DATEVALUE(\"5년 1월 1일\")",
        "=DATEVALUE(\"123년 1월 1일\")",
        "=DATEVALUE(\"01/02/2023\")",
        "=DATEVALUE(REPT(\"2\",140000))",
    ] {
        assert_eq!(
            eval(&engine, bad),
            EvaluatedCellValue::Error(FormulaError::Value),
            "{bad}"
        );
    }

    let _guard = enter_date_system(true);
    assert_eq!(
        eval(&engine, "=DATEVALUE(\"1900-01-01\")"),
        EvaluatedCellValue::Error(FormulaError::Value)
    );
    assert_eq!(
        eval(&engine, "=DATEVALUE(\"1904-01-01 24:00:00\")"),
        EvaluatedCellValue::Number(1.0)
    );
}

#[test]
fn timevalue_rollover_boundary_ampm_and_rejections() {
    let engine = SheetEngine::new("TimeParsePd335");
    let exact = [
        ("=TIMEVALUE(\"24:00:00\")", 0.0),
        ("=TIMEVALUE(\"23:60:00\")", 0.0),
        ("=TIMEVALUE(\"23:59:60\")", 0.0),
        (
            "=TIMEVALUE(\"4:35:00.5\")",
            (4.0 * 3600.0 + 35.0 * 60.0 + 0.5) / 86_400.0,
        ),
        ("=TIMEVALUE(\"2 PM\")", 14.0 / 24.0),
        ("=TIMEVALUE(\"12 AM\")", 0.0),
        ("=TIMEVALUE(\"2 PM.\")", 14.0 / 24.0),
        ("=TIMEVALUE(\"9 오전\")", 9.0 / 24.0),
        ("=TIMEVALUE(\"오후 2\")", 14.0 / 24.0),
        (
            "=TIMEVALUE(\"2023-12-31T23:59:59\")",
            (23.0 * 3600.0 + 59.0 * 60.0 + 59.0) / 86_400.0,
        ),
        (
            "=TIMEVALUE(\"4\"&REPT(\" \",32000)&\":35\")",
            (4.0 * 3600.0 + 35.0 * 60.0) / 86_400.0,
        ),
    ];
    for (expr, want) in exact {
        let EvaluatedCellValue::Number(got) = eval(&engine, expr) else {
            panic!("{expr} failed");
        };
        assert!((got - want).abs() < 1e-12, "{expr}: got {got}, want {want}");
    }

    for bad in [
        "=TIMEVALUE(\"25:00\")",
        "=TIMEVALUE(\"14:70\")",
        "=TIMEVALUE(\"2PM\")",
        "=TIMEVALUE(\"2 p.m.\")",
        "=TIMEVALUE(\"24시 0분\")",
        "=TIMEVALUE(\"9시 60분\")",
        "=TIMEVALUE(REPT(\"1\",140000))",
    ] {
        assert_eq!(
            eval(&engine, bad),
            EvaluatedCellValue::Error(FormulaError::Value),
            "{bad}"
        );
    }
}

#[test]
fn datedif_value_and_numeric_coercion_accept_korean_dates_and_rollover() {
    let mut engine = SheetEngine::new("DatedifCoercionPd335");
    engine.set_cell("A1", "2026-03-01").unwrap();
    engine.set_cell("A2", "2026-03-16").unwrap();
    engine.set_cell("B1", "-1000").unwrap();
    engine.set_cell("B2", "1100").unwrap();
    let _ = engine.recalculate_all();

    assert_eq!(
        eval(
            &engine,
            "=DATEDIF(\"2026년 3월 15일\", \"2026년 3월 20일\", \"D\")"
        ),
        EvaluatedCellValue::Number(5.0)
    );
    assert_eq!(
        eval(
            &engine,
            "=DATEDIF(\"26년 3월 15일\", \"2027년 3월 15일\", \"Y\")"
        ),
        EvaluatedCellValue::Number(1.0)
    );
    assert_eq!(
        eval(&engine, "=DATEDIF(\"2026. 3. 15.\", \"2026.06.15\", \"M\")"),
        EvaluatedCellValue::Number(3.0)
    );
    assert_eq!(
        eval(
            &engine,
            "=DATEDIF(\"2026 - 01 - 01\", \"2026-01-01 24:00:00\", \"D\")"
        ),
        EvaluatedCellValue::Number(1.0)
    );
    assert_eq!(
        eval(&engine, "=DATEDIF(\"10\", \"25\", \"D\")"),
        EvaluatedCellValue::Number(15.0)
    );
    assert_eq!(
        eval(&engine, "=VALUE(\"2026년 3월 15일 오후 2시 15분 30초\")"),
        EvaluatedCellValue::Number(
            eval(&engine, "=DATE(2026,3,15)+TIME(14,15,30)")
                .as_number()
                .unwrap()
        )
    );
    assert_eq!(
        eval(&engine, "=\"12:00\"+0"),
        EvaluatedCellValue::Number(0.5)
    );
    assert_eq!(
        eval(&engine, "=DAYS(\"2026.03.20\", \"2026년 3월 15일\")"),
        EvaluatedCellValue::Number(5.0)
    );
    assert_eq!(
        eval(&engine, "=YEARFRAC(\"2025년 1월 1일\", \"2026년 1월 1일\")"),
        EvaluatedCellValue::Number(1.0)
    );
    assert_eq!(
        eval(&engine, "=EDATE(\"2026년 1월 15일\", 1)"),
        eval(&engine, "=DATE(2026,2,15)")
    );
    assert_eq!(
        eval(&engine, "=DATEDIF(\"01/02/2023\", \"2026-01-01\", \"D\")"),
        EvaluatedCellValue::Error(FormulaError::Value)
    );

    let EvaluatedCellValue::Number(xnpv_korean) = eval(
        &engine,
        "=XNPV(0.1, B1:B2, {\"2026년 3월 1일\"; \"2026.03.16\"})",
    ) else {
        panic!("XNPV with Korean dates failed");
    };
    let EvaluatedCellValue::Number(xnpv_iso) = eval(&engine, "=XNPV(0.1, B1:B2, A1:A2)") else {
        panic!("XNPV with ISO dates failed");
    };
    assert!((xnpv_korean - xnpv_iso).abs() < 1e-12);
}

#[test]
fn circular_reference_evaluates_to_standard_num_not_circ_literal() {
    let mut engine = SheetEngine::new("CyclePd335");

    engine.set_cell("A1", "=A1+1").unwrap();
    let results = engine.recalculate_all();
    let a1 = PackedCellCoordinate::parse_a1("A1").unwrap();
    let direct = results.get(&a1).unwrap();
    assert_eq!(direct, &EvaluatedCellValue::Error(FormulaError::Cycle));
    assert_eq!(direct.to_display_string(), "#NUM!");
    assert_ne!(direct.to_display_string(), "#CIRC!");

    engine.set_cell("A1", "=B6").unwrap();
    engine.set_cell("A2", "=A1+1").unwrap();
    engine.set_cell("A3", "=A2+1").unwrap();
    engine.set_cell("A4", "=A3+5").unwrap();
    engine.set_cell("B6", "=A4*7").unwrap();
    let chain_results = engine.recalculate_all();

    for name in ["A1", "A2", "A3", "A4", "B6"] {
        let coord = PackedCellCoordinate::parse_a1(name).unwrap();
        let val = chain_results.get(&coord).unwrap();
        assert_eq!(
            val,
            &EvaluatedCellValue::Error(FormulaError::Cycle),
            "{name}"
        );
        assert_eq!(val.to_display_string(), "#NUM!", "{name}");
        assert_ne!(val.to_display_string(), "#CIRC!", "{name}");
    }
}

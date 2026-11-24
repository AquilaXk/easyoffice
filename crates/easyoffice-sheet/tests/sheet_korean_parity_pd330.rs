//! PD-330: Korean `[DBNum]` zero omission and date/time tokens, `date1904` serial 0,
//! `EOMONTH` 1900-02, local `TODAY`/`NOW`, Korean `DATEVALUE`/`TIMEVALUE`, and bond boundary roots.
#![allow(clippy::unwrap_used)]

use easyoffice_sheet::{
    enter_date_system, format_number_with_code, EvaluatedCellValue, FormulaError, SheetEngine,
};

fn fmt(num: f64, code: &str) -> String {
    format_number_with_code(num, code).unwrap_or_else(|e| format!("ERR {e:?}"))
}

fn eval(engine: &SheetEngine, formula: &str) -> EvaluatedCellValue {
    engine.evaluate_formula_str(formula)
}

#[test]
fn dbnum_korean_zero_omission_and_hanja_preservation() {
    assert_eq!(fmt(2026.0, "[DBNum4]0"), "이천이십육");
    assert_eq!(fmt(105.0, "[DBNum4]0"), "일백오");
    assert_eq!(fmt(10001.0, "[DBNum4]0"), "일만일");
    assert_eq!(fmt(0.0, "[DBNum4]0"), "영");
    assert_eq!(fmt(2026.0, "[$-412][DBNum1]0"), "二千二十六");
    assert_eq!(fmt(10001.0, "[$-412][DBNum1]0"), "一萬一");
    assert_eq!(fmt(2026.0, "[$-412][DBNum2]General"), "貳阡貳拾六");
    // Without [$-412], Hanja DBNum1 / DBNum2 keeps the existing '零' insertion.
    assert_eq!(fmt(10001.0, "[DBNum1]0"), "一萬零一");
    assert_eq!(fmt(100000001.0, "[DBNum1]General"), "一億零一");
    assert_eq!(fmt(2026.0, "[DBNum1]0"), "二千零二十六");
    assert_eq!(fmt(10001.0, "[DBNum2]0"), "壹萬零壹");
}

#[test]
fn dbnum_applies_to_date_and_time_numeric_tokens() {
    // 46096 = 2026-03-15 (Sunday); 46096.37853009259 = 09:05:05; 46096.59403935185 = 14:15:25
    let serial_2026_03_15 = 46_096.0;
    let time_09_05_03 = serial_2026_03_15 + (9.0 * 3600.0 + 5.0 * 60.0 + 3.0) / 86_400.0;
    let time_14_15_25 = serial_2026_03_15 + (14.0 * 3600.0 + 15.0 * 60.0 + 25.0) / 86_400.0;
    // 36526 = 2000-01-01 (2-digit year = 0), 38353 = 2005-01-01 (2-digit year = 5)
    let serial_2000_01_01 = 36_526.0;
    let serial_2005_01_01 = 38_353.0;

    assert_eq!(
        fmt(serial_2026_03_15, "[DBNum4]yyyy\"년\" m\"월\" d\"일\""),
        "이천이십육년 삼월 일십오일"
    );
    assert_eq!(
        fmt(
            serial_2026_03_15,
            "[$-412][DBNum1]yyyy\"년\" m\"월\" d\"일\""
        ),
        "二千二十六년 三월 一十五일"
    );
    assert_eq!(
        fmt(serial_2026_03_15, "[DBNum1]yyyy\"년\" m\"월\" d\"일\""),
        "二千零二十六년 三월 一十五일"
    );
    assert_eq!(
        fmt(serial_2005_01_01, "[DBNum4]yy\"년\" m\"월\" d\"일\""),
        "오년 일월 일일"
    );
    assert_eq!(
        fmt(serial_2000_01_01, "[DBNum4]yy\"년\" m\"월\" d\"일\""),
        "영년 일월 일일"
    );
    assert_eq!(fmt(serial_2000_01_01, "[DBNum1]yy"), "零");
    assert_eq!(
        fmt(time_09_05_03, "[DBNum3]yyyy-mm-dd hh:mm:ss"),
        "２０２６-０３-１５ ０９:０５:０３"
    );
    assert_eq!(fmt(serial_2005_01_01, "[DBNum3]yy-mm-dd"), "０５-０１-０１");
    assert_eq!(
        fmt(
            time_14_15_25,
            "[$-412][DBNum4]AM/PM h\"시\" mm\"분\" ss\"초\""
        ),
        "오후 이시 일십오분 이십오초"
    );
    assert_eq!(
        fmt(
            serial_2026_03_15,
            "[DBNum4]yyyy\"년\" m\"월\" d\"일\" (aaa)"
        ),
        "이천이십육년 삼월 일십오일 (일)"
    );
    assert_eq!(fmt(serial_2026_03_15, "[$-412][DBNum4]dddd"), "일요일");
    assert_eq!(fmt(1.5, "[DBNum4][h]\"시간\" m\"분\""), "삼십육시간 영분");
    assert_eq!(fmt(1.5, "[DBNum3][hh]:mm:ss"), "３６:００:００");
}

#[test]
fn date1904_serial_zero_and_eomonth_1900_feb() {
    let engine = SheetEngine::new("Dates");
    assert_eq!(
        eval(&engine, "=YEAR(0)"),
        EvaluatedCellValue::Number(1900.0)
    );
    assert_eq!(eval(&engine, "=MONTH(0)"), EvaluatedCellValue::Number(1.0));
    assert_eq!(eval(&engine, "=DAY(0)"), EvaluatedCellValue::Number(0.0));

    assert_eq!(
        eval(&engine, "=EOMONTH(DATE(1900,2,1),0)"),
        EvaluatedCellValue::Number(59.0)
    );
    assert_eq!(
        eval(&engine, "=EDATE(DATE(1900,1,31),1)"),
        EvaluatedCellValue::Number(59.0)
    );
    assert_eq!(
        eval(&engine, "=DATEVALUE(\"1900-02-29\")"),
        EvaluatedCellValue::Number(60.0)
    );

    let _guard = enter_date_system(true);
    assert_eq!(
        eval(&engine, "=YEAR(0)"),
        EvaluatedCellValue::Number(1904.0)
    );
    assert_eq!(eval(&engine, "=MONTH(0)"), EvaluatedCellValue::Number(1.0));
    assert_eq!(eval(&engine, "=DAY(0)"), EvaluatedCellValue::Number(1.0));
}

#[test]
fn today_and_now_follow_local_time_and_date_system() {
    let engine = SheetEngine::new("Clock");
    let EvaluatedCellValue::Number(today_1900) = eval(&engine, "=TODAY()") else {
        panic!("TODAY() must return number");
    };
    let EvaluatedCellValue::Number(now_1900) = eval(&engine, "=NOW()") else {
        panic!("NOW() must return number");
    };
    assert!(today_1900 > 44_000.0 && today_1900 < 80_000.0);
    assert!(now_1900 >= today_1900 && now_1900 < today_1900 + 1.0);

    let _guard = enter_date_system(true);
    let EvaluatedCellValue::Number(today_1904) = eval(&engine, "=TODAY()") else {
        panic!("TODAY() in 1904 must return number");
    };
    assert!((today_1900 - today_1904 - 1462.0).abs() <= 1.0);
}

#[test]
fn korean_datevalue_and_timevalue_formats() {
    let engine = SheetEngine::new("LocaleParse");
    let want_2026_03_15 = eval(&engine, "=DATE(2026,3,15)");
    for expr in [
        "=DATEVALUE(\"2026-03-15\")",
        "=DATEVALUE(\"2026/03/15\")",
        "=DATEVALUE(\"2026 - 3 - 15\")",
        "=DATEVALUE(\"2026년 3월 15일\")",
        "=DATEVALUE(\"2026년3월15일\")",
        "=DATEVALUE(\"2026. 3. 15.\")",
        "=DATEVALUE(\"2026.03.15\")",
        "=DATEVALUE(\"2026. 3. 15\")",
    ] {
        assert_eq!(eval(&engine, expr), want_2026_03_15, "{expr}");
    }
    for bad in [
        "=DATEVALUE(\"2026년 2월 29일\")",
        "=DATEVALUE(\"2026.13.01\")",
        "=DATEVALUE(\"2026.03.15..\")",
    ] {
        assert_eq!(
            eval(&engine, bad),
            EvaluatedCellValue::Error(FormulaError::Value),
            "{bad}"
        );
    }

    let time_cases = [
        (
            "=TIMEVALUE(\"9시 30분\")",
            (9.0 * 3600.0 + 30.0 * 60.0) / 86_400.0,
        ),
        (
            "=TIMEVALUE(\"오후 2시 15분 30초\")",
            (14.0 * 3600.0 + 15.0 * 60.0 + 30.0) / 86_400.0,
        ),
        (
            "=TIMEVALUE(\"14시 30분 5초\")",
            (14.0 * 3600.0 + 30.0 * 60.0 + 5.0) / 86_400.0,
        ),
        ("=TIMEVALUE(\"오전 12시 0분\")", 0.0),
        ("=TIMEVALUE(\"오후 12시 0분\")", 0.5),
        (
            "=TIMEVALUE(\"9:30 오전\")",
            (9.0 * 3600.0 + 30.0 * 60.0) / 86_400.0,
        ),
        (
            "=TIMEVALUE(\"2:15 오후\")",
            (14.0 * 3600.0 + 15.0 * 60.0) / 86_400.0,
        ),
        (
            "=TIMEVALUE(\"2026년 3월 15일 오후 2시 15분 30초\")",
            (14.0 * 3600.0 + 15.0 * 60.0 + 30.0) / 86_400.0,
        ),
    ];
    for (expr, want) in time_cases {
        let EvaluatedCellValue::Number(got) = eval(&engine, expr) else {
            panic!("{expr} failed");
        };
        assert!((got - want).abs() < 1e-12, "{expr}: got {got}, want {want}");
    }
    for bad in [
        "=TIMEVALUE(\"오후 13시 0분\")",
        "=TIMEVALUE(\"오전 0시 30분\")",
        "=TIMEVALUE(\"24시 0분\")",
        "=TIMEVALUE(\"9시 60분\")",
        "=TIMEVALUE(\"2026년 3월 15일\")",
    ] {
        assert_eq!(
            eval(&engine, bad),
            EvaluatedCellValue::Error(FormulaError::Value),
            "{bad}"
        );
    }
}

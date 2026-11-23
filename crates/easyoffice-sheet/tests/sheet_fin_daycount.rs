//! Precision, 1900-epoch boundary, round-trip yield solver, and fail-closed budget tests for
//! financial day-count and coupon schedule parity (PD-323).

use easyoffice_sheet::{EvaluatedCellValue, FormulaError, SheetEngine};

fn eval_num(formula: &str) -> f64 {
    let s = SheetEngine::new("S");
    match s.evaluate_formula_str(formula) {
        EvaluatedCellValue::Number(v) => v,
        other => panic!("{formula} expected number, got {other:?}"),
    }
}

fn eval_err(formula: &str) -> FormulaError {
    let s = SheetEngine::new("S");
    match s.evaluate_formula_str(formula) {
        EvaluatedCellValue::Error(e) => e,
        other => panic!("{formula} expected error, got {other:?}"),
    }
}

#[test]
fn oddfprice_long_first_period_matches_reference_to_machine_precision() {
    let cases = [
        (
            "=ODDFPRICE(DATE(2020,3,1),DATE(2030,3,1),DATE(2020,1,15),DATE(2021,3,1),0.075,0.07,120,1,2)",
            113.508_466_506_910,
        ),
        (
            "=ODDFPRICE(DATE(2020,3,1),DATE(2030,3,1),DATE(2020,1,15),DATE(2021,3,1),0.075,0,120,1,2)",
            195.0,
        ),
        (
            "=ODDFPRICE(DATE(2020,3,1),DATE(2030,3,1),DATE(2020,1,15),DATE(2021,3,1),0,0.1,100,1,2)",
            38.503_326_318_607_29,
        ),
        (
            "=ODDFPRICE(DATE(2020,3,1),DATE(2030,3,1),DATE(2020,1,15),DATE(2021,3,1),0.075,0.05,120,2,2)",
            131.483_767_046_541_66,
        ),
        (
            "=ODDFPRICE(DATE(2020,3,1),DATE(2030,3,1),DATE(2020,1,15),DATE(2021,3,1),0.075,0.05,120,2,1)",
            131.556_968_693_401_27,
        ),
    ];
    for (formula, expected) in cases {
        let got = eval_num(formula);
        assert!(
            (got - expected).abs() < 1e-11,
            "{formula}: got {got:.15}, expected {expected:.15}"
        );
    }
}

#[test]
fn oddfyield_roundtrips_short_long_zero_rate_and_extreme_redemptions() {
    // Short odd first period across all 5 day-count bases
    for basis in 0..=4 {
        let pr = eval_num(&format!(
            "=ODDFPRICE(DATE(2008,11,11),DATE(2021,3,1),DATE(2008,10,15),DATE(2009,3,1),0.0785,0.0625,100,2,{basis})"
        ));
        let yld = eval_num(&format!(
            "=ODDFYIELD(DATE(2008,11,11),DATE(2021,3,1),DATE(2008,10,15),DATE(2009,3,1),0.0785,{pr:.15},100,2,{basis})"
        ));
        assert!(
            (yld - 0.0625).abs() < 1e-7,
            "short basis={basis} roundtrip got {yld}"
        );
    }

    // Long multi-period odd first period across all 5 bases, zero coupon rate, and extreme redemptions
    let long_cases = [
        (
            "2020,3,1",
            "2030,3,1",
            "2020,1,15",
            "2021,9,1",
            0.075,
            0.055,
            100.0,
            2,
        ),
        (
            "2020,3,1",
            "2030,3,1",
            "2020,1,15",
            "2021,3,1",
            0.0,
            0.1,
            100.0,
            1,
        ),
        (
            "2020,3,1",
            "2030,3,1",
            "2020,1,15",
            "2021,3,1",
            0.075,
            0.07,
            10.0,
            1,
        ),
        (
            "2020,3,1",
            "2030,3,1",
            "2020,1,15",
            "2021,3,1",
            0.075,
            0.05,
            300.0,
            2,
        ),
    ];
    for &(settl, mat, issue, first, rate, yld_in, redemp, freq) in &long_cases {
        for basis in 0..=4 {
            let pr = eval_num(&format!(
                "=ODDFPRICE(DATE({settl}),DATE({mat}),DATE({issue}),DATE({first}),{rate},{yld_in},{redemp},{freq},{basis})"
            ));
            let yld = eval_num(&format!(
                "=ODDFYIELD(DATE({settl}),DATE({mat}),DATE({issue}),DATE({first}),{rate},{pr:.15},{redemp},{freq},{basis})"
            ));
            assert!(
                (yld - yld_in).abs() < 1e-7,
                "long freq={freq} basis={basis} redemp={redemp}: got {yld}, expected {yld_in}"
            );
        }
    }
}

#[test]
fn epoch_1900_boundary_coupdays_and_accrint_stub_rules() {
    // COUPDAYS steps before serial 0 in actual/actual when settlement > 0
    assert!((eval_num("=COUPDAYS(30,220,2,1)") - 184.0).abs() < 1e-12);
    // Serial 59 (Feb 28, 1900) is NOT month-end in the 1900 system because serial 60 is Feb 29, 1900
    assert!((eval_num("=COUPDAYS(30,425,2,1)") - 184.0).abs() < 1e-12);
    // Actual/360 stub ending Aug 31, 1900 divides by 360/freq = 180 rather than 30/360 length 181
    // Issue=61 (1900-03-01), first=425 (1901-02-28 -> reg_pcd=1900-08-31=245, not month-end because 59 wasn't),
    // wait: settlement=300 (1900-10-26) > 245:
    // reg_pcd = 1900-08-31 (244), days0 = 300 - 244 = 56, coup_days0 = 180;
    // stub 1900-02-28 (59) .. 1900-08-31 (244), issue=61 -> 244 - 61 = 183, denom = 180;
    // total = 1000 * 0.1 / 2 * (56/180 + 183/180) = 50 * 239 / 180.
    let expected = 1000.0 * 0.1 / 2.0 * ((300.0 - 244.0) / 180.0 + (244.0 - 61.0) / 180.0);
    let got = eval_num("=ACCRINT(61,425,300,0.1,1000,2,2)");
    assert!(
        (got - expected).abs() < 1e-11,
        "ACCRINT(61,425,300,0.1,1000,2,2): got {got}, expected {expected}"
    );
}

#[test]
fn oddlprice_rejects_settlement_before_last_interest_per_reference_spec() {
    // maturity > settlement > last_interest is required by the reference app specification.
    assert_eq!(
        eval_err("=ODDLPRICE(DATE(2023,3,1),DATE(2025,8,15),DATE(2024,10,15),0.06,0.05,100,2,1)"),
        FormulaError::Num
    );
    assert_eq!(
        eval_err("=ODDLYIELD(DATE(2023,3,1),DATE(2025,8,15),DATE(2024,10,15),0.06,101,100,2,1)"),
        FormulaError::Num
    );
}

#[test]
fn financial_coupon_walks_fail_closed_when_exceeding_budget() {
    let over_budget = [
        "=COUPDAYS(DATE(1900,1,1),DATE(9999,12,31),4,1)",
        "=COUPPCD(DATE(1900,1,1),DATE(9999,12,31),4,0)",
        "=COUPNCD(DATE(1900,1,1),DATE(9999,12,31),4,0)",
        "=COUPNUM(DATE(1900,1,1),DATE(9999,12,31),4,0)",
        "=ACCRINT(DATE(1900,1,2),DATE(9999,12,31),DATE(9999,6,1),0.05,1000,4,0)",
        "=ODDFPRICE(DATE(1900,2,1),DATE(9999,12,31),DATE(1900,1,15),DATE(1901,12,31),0.05,0.05,100,4,0)",
        "=ODDFYIELD(DATE(1900,2,1),DATE(9999,12,31),DATE(1900,1,15),DATE(1901,12,31),0.05,100,100,4,0)",
    ];
    for formula in over_budget {
        assert_eq!(
            eval_err(formula),
            FormulaError::Num,
            "{formula} must fail closed with #NUM!"
        );
    }
}

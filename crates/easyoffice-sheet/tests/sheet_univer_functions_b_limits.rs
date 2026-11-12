//! Formula function bundle B (PD-201): limits, hostile input and the array-result paths.

use easyoffice_sheet::{
    EvaluatedCellValue as V, FormulaError as E, PackedCellCoordinate, SheetEngine,
};

fn eval(s: &SheetEngine, formula: &str) -> V {
    s.evaluate_formula_str(formula)
}

fn put(s: &mut SheetEngine, row: u32, col: u32, raw: impl Into<String>) {
    let at = PackedCellCoordinate::new(row, col).unwrap();
    s.set_cell(&at.to_a1(), raw).unwrap();
}

fn number(v: &V) -> f64 {
    let V::Number(n) = v else {
        panic!("expected a number, got {v:?}");
    };
    *n
}

/// The reference's check that a degree-6 polynomial fit keeps its coefficients (the normal
/// equations of this data are close to singular; the QR solve is what keeps them stable).
#[test]
fn linest_keeps_high_order_polynomial_coefficients() {
    let xs: [f64; 11] = [
        155.0, 178.0, 195.0, 205.0, 189.0, 179.0, 142.0, 130.0, 152.0, 166.0, 194.0,
    ];
    let ys = [
        975097.0, 1076088.07, 985880.56, 1264998.15, 1487692.02, 879096.99, 1013152.71, 480886.67,
        651359.67, 472135.74, 720614.91,
    ];
    let expected = [
        0.003030803284094725,
        -3.014286876228756,
        1242.0301184586228,
        -271378.1017228177,
        33159765.83632094,
        -2148345015.0620275,
        57656015344.78786,
    ];
    let mut s = SheetEngine::new("S");
    for (r, (&x, &y)) in xs.iter().zip(&ys).enumerate() {
        put(&mut s, r as u32, 0, y.to_string());
        for k in 1..=6 {
            put(&mut s, r as u32, k, x.powi(k as i32).to_string());
        }
    }
    for (j, want) in expected.iter().enumerate() {
        let got = number(&eval(
            &s,
            &format!("=INDEX(LINEST(A1:A11,B1:G11),1,{})", j + 1),
        ));
        let relative = (got - want).abs() / want.abs().max(1.0);
        assert!(relative < 1e-8, "coefficient {j}: {got} against {want}");
    }
}

#[test]
fn linest_statistics_hold_errors_in_the_unused_cells() {
    let mut s = SheetEngine::new("S");
    for (r, (x, y)) in [(1, 2), (2, 4), (3, 5), (4, 4), (5, 5)].iter().enumerate() {
        put(&mut s, r as u32, 0, y.to_string());
        put(&mut s, r as u32, 1, x.to_string());
    }
    // two predictors need columns: one predictor gives a 5 x 2 result, every cell a number
    assert_eq!(
        number(&eval(&s, "=INDEX(LINEST(A1:A5,B1:B5,TRUE,TRUE),1,1)")),
        0.6
    );
    assert_eq!(
        eval(&s, "=INDEX(LINEST(A1:A5,B1:B5,TRUE,TRUE),5,3)"),
        V::Error(E::Ref)
    );
    // a regression without a constant has no standard error for it: #N/A in that cell
    assert_eq!(
        eval(&s, "=INDEX(LINEST(A1:A5,B1:B5,FALSE,TRUE),2,2)"),
        V::Error(E::Na)
    );
    // a result with an error cell poisons the aggregates the way a range with one would
    assert_eq!(
        eval(&s, "=SUM(LINEST(A1:A5,B1:B5,FALSE,TRUE))"),
        V::Error(E::Na)
    );
}

#[test]
fn matrix_results_feed_aggregates_and_index() {
    let mut s = SheetEngine::new("S");
    for (r, v) in [79, 85, 78, 85, 50, 81, 95, 88, 97].iter().enumerate() {
        put(&mut s, r as u32, 0, v.to_string());
    }
    for (r, v) in [70, 79, 89].iter().enumerate() {
        put(&mut s, r as u32, 1, v.to_string());
    }
    assert_eq!(
        eval(&s, "=INDEX(FREQUENCY(A1:A9,B1:B3),3,1)"),
        V::Number(4.0)
    );
    // the counts add up to the data
    assert_eq!(eval(&s, "=SUM(FREQUENCY(A1:A9,B1:B3))"), V::Number(9.0));
    // the first cell of the result shows in the cell
    assert_eq!(eval(&s, "=FREQUENCY(A1:A9,B1:B3)"), V::Number(1.0));
    assert_eq!(eval(&s, "=INDEX(MODE.MULT(A1:A9),1,1)"), V::Number(85.0));
    // TREND without x fits against 1, 2, 3, ...: its first cell is the line's value at 1
    for (r, v) in [1, 2, 3].iter().enumerate() {
        put(&mut s, r as u32, 3, v.to_string());
    }
    let line = number(&eval(&s, "=FORECAST(1,A1:A3,D1:D3)"));
    assert!((number(&eval(&s, "=TREND(A1:A3)")) - line).abs() < 1e-9);
}

#[test]
fn frequency_streams_a_whole_column() {
    let mut s = SheetEngine::new("S");
    for (r, v) in [1, 5, 9].iter().enumerate() {
        put(&mut s, r as u32, 0, v.to_string());
    }
    put(&mut s, 0, 2, "4");
    assert_eq!(eval(&s, "=SUM(FREQUENCY(A1:A1048576,C1))"), V::Number(3.0));
    assert_eq!(
        eval(&s, "=INDEX(FREQUENCY(A1:A1048576,C1),2,1)"),
        V::Number(2.0)
    );
    assert_eq!(eval(&s, "=MODE.MULT(A1:A1048576)"), V::Error(E::Na));
}

#[test]
fn operand_and_predictor_limits_are_errors_not_work() {
    let s = SheetEngine::new("S");
    // the whole sheet is refused before any cell is read
    assert_eq!(eval(&s, "=LINEST(A1:XFD1048576)"), V::Error(E::Value));
    assert_eq!(eval(&s, "=TREND(A1:XFD1048576)"), V::Error(E::Value));
    assert_eq!(eval(&s, "=FREQUENCY(A1,A1:XFD1048576)"), V::Error(E::Value));
    // 257 predictors (and the constant) are over the limit of 256
    let mut s = SheetEngine::new("S");
    let (predictors, observations) = (257u32, 260u32);
    for c in 0..observations {
        put(&mut s, 0, c, (c + 1).to_string());
        for r in 0..predictors {
            put(&mut s, r + 1, c, ((r + 1) * (c % 7 + 1) + c).to_string());
        }
    }
    let formula = format!(
        "=LINEST(A1:{}1,A2:{}{})",
        col(observations - 1),
        col(observations - 1),
        predictors + 1
    );
    assert_eq!(eval(&s, &formula), V::Error(E::Value));
}

fn col(c: u32) -> String {
    PackedCellCoordinate::new(0, c)
        .unwrap()
        .to_a1()
        .trim_end_matches('1')
        .to_string()
}

#[test]
fn bessel_order_and_coupon_walks_are_bounded() {
    let s = SheetEngine::new("S");
    assert_eq!(eval(&s, "=BESSELJ(1,100001)"), V::Error(E::Num));
    assert_eq!(eval(&s, "=BESSELI(1,1000000000)"), V::Error(E::Num));
    assert!(matches!(
        eval(&s, "=BESSELJ(1,100000)"),
        V::Number(_) | V::Error(E::Num)
    ));
    // 8,000 years of quarterly coupons are refused; a 100-year bond is not
    assert_eq!(
        eval(&s, "=COUPNUM(\"1900-01-02\",\"9999-12-31\",4)"),
        V::Error(E::Num)
    );
    assert_eq!(
        eval(
            &s,
            "=PRICE(\"1900-01-02\",\"9999-12-31\",0.05,0.05,100,4,0)"
        ),
        V::Error(E::Num)
    );
    assert_eq!(
        eval(&s, "=COUPNUM(\"2000-01-01\",\"2100-12-31\",4)"),
        V::Number(404.0)
    );
    assert_eq!(eval(&s, "=ODDFYIELD(\"1900-01-02\",\"9999-12-31\",\"1900-01-01\",\"1900-06-30\",0.05,95,100,2,0)"), V::Error(E::Num));
    // a yield that cannot be found is #NUM!, not a hang
    assert_eq!(
        eval(&s, "=ODDFYIELD(\"2008-11-11\",\"2021-03-01\",\"2008-10-15\",\"2009-03-01\",0.0575,1000000000000,100,2,0)"),
        V::Error(E::Num)
    );
}

#[test]
fn vdb_walks_a_bounded_number_of_periods() {
    let s = SheetEngine::new("S");
    assert_eq!(eval(&s, "=VDB(1000,0,20000,0,15000)"), V::Error(E::Num));
    assert!(matches!(
        eval(&s, "=VDB(1000,0,10000,0,9999)"),
        V::Number(_)
    ));
}

#[test]
fn convert_finds_prefixed_and_alternate_symbols_in_one_lookup() {
    let s = SheetEngine::new("S");
    assert_eq!(eval(&s, "=CONVERT(1,\"km\",\"m\")"), V::Number(1000.0));
    assert_eq!(eval(&s, "=CONVERT(1,\"KiB\",\"byte\")"), V::Error(E::Na));
    assert_eq!(
        eval(&s, "=CONVERT(1,\"kibyte\",\"byte\")"),
        V::Number(1024.0)
    );
    assert_eq!(eval(&s, "=CONVERT(1,\"m\",\"kg\")"), V::Error(E::Na));
    assert_eq!(eval(&s, "=CONVERT(100,\"C\",\"F\")"), V::Number(212.0));
    assert_eq!(eval(&s, "=CONVERT(1,\"da\",\"m\")"), V::Error(E::Na));
}

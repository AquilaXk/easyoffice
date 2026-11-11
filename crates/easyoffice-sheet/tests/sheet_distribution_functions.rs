//! Accuracy (known table values, relative error) and error values of the distribution
//! functions, plus `XMATCH`.

use easyoffice_sheet::{EvaluatedCellValue, FormulaError, SheetEngine};

const REL_TOL: f64 = 1e-11;

fn eval(formula: &str) -> EvaluatedCellValue {
    SheetEngine::new("Sheet1").evaluate_formula_str(formula)
}

fn assert_close(formula: &str, want: f64) {
    let EvaluatedCellValue::Number(got) = eval(formula) else {
        panic!("{formula}: not a number: {:?}", eval(formula));
    };
    let err = if want == 0.0 {
        got.abs()
    } else {
        ((got - want) / want).abs()
    };
    assert!(
        err <= REL_TOL,
        "{formula}: got {got}, want {want}, rel err {err:e}"
    );
}

fn assert_error(formula: &str, want: FormulaError) {
    assert_eq!(eval(formula), EvaluatedCellValue::Error(want), "{formula}");
}

#[test]
fn normal_distribution() {
    for (f, want) in [
        ("=NORM.S.INV(0.975)", 1.959963984540054),
        ("=NORMSINV(0.001)", -3.090232306167813),
        ("=NORM.S.INV(1E-10)", -6.361340902404056),
        ("=NORM.S.INV(1E-7)", -5.199337582192817),
        ("=NORM.INV(0.9,10,2)", 12.563103131089201),
        ("=NORMINV(0.5,3,7)", 3.0),
        ("=NORM.DIST(1,0,1,TRUE)", 0.8413447460685429),
        ("=NORMDIST(0,0,1,FALSE)", 0.3989422804014327),
        ("=NORM.DIST(52,50,4,FALSE)", 0.08801633169107487),
    ] {
        assert_close(f, want);
    }
    assert_error("=NORM.INV(1,0,1)", FormulaError::Num);
    assert_error("=NORM.INV(0,0,1)", FormulaError::Num);
    assert_error("=NORM.INV(0.5,0,0)", FormulaError::Num);
    assert_error("=NORM.DIST(1,0,-1,TRUE)", FormulaError::Num);
    assert_error("=NORM.S.INV(\"a\")", FormulaError::Value);
}

#[test]
fn students_t() {
    for (f, want) in [
        ("=T.INV.2T(0.05,10)", 2.228138851986274),
        ("=TINV(0.05,30)", 2.042272456301238),
        ("=T.INV.2T(0.05,1)", 12.706204736174698),
        ("=T.INV(0.95,5)", 2.0150483733330242),
        ("=T.INV(0.025,10)", -2.228138851986274),
        ("=T.DIST(1,1,TRUE)", 0.75),
        ("=T.DIST(1,2,TRUE)", 0.7886751345948129),
        ("=T.DIST(-1,2,TRUE)", 0.2113248654051871),
        ("=T.DIST(0,1,FALSE)", std::f64::consts::FRAC_1_PI),
        ("=T.DIST(0,5,FALSE)", 0.3796066898224944),
        ("=T.DIST.RT(2.228138851986274,10)", 0.025),
        ("=T.DIST.2T(2.228138851986274,10)", 0.05),
        ("=TDIST(2.228138851986274,10,1)", 0.025),
        ("=TDIST(2.228138851986274,10,2)", 0.05),
    ] {
        assert_close(f, want);
    }
    assert_error("=T.DIST(1,0,TRUE)", FormulaError::Num);
    assert_error("=T.INV(0,5)", FormulaError::Num);
    assert_error("=T.INV(1,5)", FormulaError::Num);
    assert_error("=T.DIST.2T(-1,5)", FormulaError::Num);
    assert_error("=TDIST(1,5,3)", FormulaError::Num);
    assert_error("=T.DIST(\"x\",5,TRUE)", FormulaError::Value);
    assert_error("=T.DIST(1,5)", FormulaError::Value);
}

#[test]
fn chi_squared() {
    for (f, want) in [
        ("=CHISQ.INV.RT(0.05,10)", 18.307038053275146),
        ("=CHIINV(0.05,1)", 3.841458820694124),
        ("=CHISQ.INV(0.95,2)", 5.991464547107979),
        ("=CHISQ.DIST(3.841458820694124,1,TRUE)", 0.95),
        ("=CHISQ.DIST.RT(10,5)", 0.07523524614651217),
        ("=CHIDIST(10,5)", 0.07523524614651217),
        ("=CHISQ.DIST(2,2,TRUE)", 0.6321205588285577),
        ("=CHISQ.DIST(1,2,FALSE)", 0.30326532985631666),
        ("=CHISQ.DIST(0,2,FALSE)", 0.5),
        ("=CHISQ.INV(0,4)", 0.0),
    ] {
        assert_close(f, want);
    }
    assert_error("=CHISQ.DIST(-1,2,TRUE)", FormulaError::Num);
    assert_error("=CHISQ.INV(1,2)", FormulaError::Num);
    assert_error("=CHISQ.INV.RT(0,2)", FormulaError::Num);
    assert_error("=CHISQ.INV.RT(0.5,0)", FormulaError::Num);
}

#[test]
fn gamma_family() {
    for (f, want) in [
        ("=GAMMA.DIST(2,3,1,TRUE)", 0.3233235838169366),
        ("=GAMMADIST(2,3,1,FALSE)", 0.2706705664732254),
        ("=GAMMA.INV(0.3233235838169366,3,1)", 2.0),
        ("=GAMMAINV(0.5,1,2)", 1.3862943611198906),
        ("=GAMMALN(10)", 12.801827480081469),
        ("=GAMMALN.PRECISE(0.5)", 0.5723649429247001),
        ("=GAMMALN(0.25)", 1.2880225246980772),
        ("=GAMMALN(100)", 359.134_205_369_575_4),
        ("=GAMMA(5)", 24.0),
        ("=GAMMA(0.5)", 1.772453850905516),
        ("=GAMMA(-1.5)", 2.3632718012073513),
    ] {
        assert_close(f, want);
    }
    assert_error("=GAMMA(0)", FormulaError::Num);
    assert_error("=GAMMA(-2)", FormulaError::Num);
    assert_error("=GAMMALN(0)", FormulaError::Num);
    assert_error("=GAMMA.DIST(1,0,1,TRUE)", FormulaError::Num);
    assert_error("=GAMMA.INV(1,2,2)", FormulaError::Num);
}

#[test]
fn beta_family() {
    for (f, want) in [
        ("=BETA.DIST(0.5,2,3,TRUE)", 0.6875),
        ("=BETA.DIST(0.5,2,3,FALSE)", 1.5),
        ("=BETA.DIST(2,8,10,TRUE,1,3)", 0.685470581054688),
        ("=BETA.DIST(2,8,10,FALSE,1,3)", 1.4837646484375),
        ("=BETADIST(2,8,10,1,3)", 0.685470581054688),
        ("=BETA.INV(0.685470581054688,8,10,1,3)", 2.0),
        ("=BETAINV(0.6875,2,3)", 0.5),
        ("=BETA.INV(1,2,3)", 1.0),
    ] {
        assert_close(f, want);
    }
    assert_error("=BETA.DIST(0.5,0,3,TRUE)", FormulaError::Num);
    assert_error("=BETA.DIST(2,2,3,TRUE)", FormulaError::Num);
    assert_error("=BETA.DIST(0.5,2,3,TRUE,1,1)", FormulaError::Num);
    assert_error("=BETA.INV(0,2,3)", FormulaError::Num);
}

#[test]
fn binomial() {
    for (f, want) in [
        ("=BINOM.DIST(6,10,0.5,FALSE)", 0.205078125),
        ("=BINOMDIST(6,10,0.5,TRUE)", 0.828125),
        ("=BINOM.DIST(0,5,0,FALSE)", 1.0),
        ("=BINOM.DIST(5,5,1,TRUE)", 1.0),
        ("=BINOM.DIST(2,5,0.3,TRUE)", 0.83692),
        ("=BINOM.INV(6,0.5,0.75)", 4.0),
        ("=CRITBINOM(10,0.5,0.5)", 5.0),
        ("=BINOM.DIST(60,100,0.5,FALSE)", 0.010843866711637987),
    ] {
        assert_close(f, want);
    }
    assert_error("=BINOM.DIST(11,10,0.5,TRUE)", FormulaError::Num);
    assert_error("=BINOM.DIST(1,10,1.5,TRUE)", FormulaError::Num);
    assert_error("=BINOM.DIST(-1,10,0.5,TRUE)", FormulaError::Num);
    assert_error("=BINOM.INV(10,0.5,1.5)", FormulaError::Num);
}

#[test]
fn xmatch_modes() {
    let mut sheet = SheetEngine::new("Sheet1");
    for (cell, value) in [("A1", "10"), ("A2", "20"), ("A3", "30"), ("A4", "20")] {
        sheet.set_cell(cell, value).unwrap();
    }
    sheet.set_cell("B1", "apple").unwrap();
    sheet.set_cell("B2", "banana").unwrap();
    sheet.set_cell("B3", "Cherry").unwrap();
    let num = |n: f64| EvaluatedCellValue::Number(n);
    for (formula, want) in [
        ("=XMATCH(20,A1:A4)", num(2.0)),
        ("=XMATCH(20,A1:A4,0,-1)", num(4.0)),
        ("=XMATCH(25,A1:A4,-1)", num(2.0)),
        ("=XMATCH(25,A1:A3,1)", num(3.0)),
        ("=XMATCH(25,A1:A3,1,2)", num(3.0)),
        ("=XMATCH(\"b*\",B1:B3,2)", num(2.0)),
        ("=XMATCH(\"cherry\",B1:B3)", num(3.0)),
    ] {
        assert_eq!(sheet.evaluate_formula_str(formula), want, "{formula}");
    }
    for (formula, want) in [
        ("=XMATCH(99,A1:A4)", FormulaError::Na),
        ("=XMATCH(5,A1:A4,-1)", FormulaError::Na),
        ("=XMATCH(20,A1:A4,3)", FormulaError::Value),
        ("=XMATCH(20,A1:A4,0,5)", FormulaError::Value),
    ] {
        assert_eq!(
            sheet.evaluate_formula_str(formula),
            EvaluatedCellValue::Error(want),
            "{formula}"
        );
    }
}

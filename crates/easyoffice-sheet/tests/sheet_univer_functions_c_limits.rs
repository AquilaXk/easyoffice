//! Formula function bundle C (PD-205): the byte width table, the split rule, the full-width
//! conversions, `AGGREGATE` options, and the limits and hostile inputs of the new functions.

use easyoffice_sheet::{EvaluatedCellValue as V, FormulaError as E, SheetEngine};

fn eval(s: &SheetEngine, formula: &str) -> V {
    s.evaluate_formula_str(formula)
}

fn text(s: &SheetEngine, formula: &str) -> String {
    match eval(s, formula) {
        V::Text(t) => t,
        other => panic!("{formula}: {other:?}"),
    }
}

fn number(s: &SheetEngine, formula: &str) -> f64 {
    match eval(s, formula) {
        V::Number(n) => n,
        other => panic!("{formula}: {other:?}"),
    }
}

/// The 2-byte ranges of PD-205, written out again here so a change of the table fails a test.
const WIDE: &[(u32, u32)] = &[
    (0x1100, 0x11FF),
    (0x2E80, 0x2FDF),
    (0x3000, 0x303E),
    (0x3040, 0x30FF),
    (0x3130, 0x318F),
    (0x31F0, 0x31FF),
    (0x3200, 0x33FF),
    (0x3400, 0x4DBF),
    (0x4E00, 0x9FFF),
    (0xA960, 0xA97F),
    (0xAC00, 0xD7A3),
    (0xD7B0, 0xD7FF),
    (0xF900, 0xFAFF),
    (0xFF01, 0xFF60),
    (0xFFE0, 0xFFE6),
    (0x2_0000, 0x2_FA1F),
    (0x3_0000, 0x3_134F),
];

#[test]
fn every_code_point_has_the_width_of_the_table() {
    let s = SheetEngine::new("S");
    let mut checked = 0u32;
    // the whole BMP, then the planes that hold the ideograph extensions
    for cp in (0x20u32..0x1_0000).chain(0x1_FFF0..0x3_1360) {
        if char::from_u32(cp).is_none() {
            continue;
        }
        let want = if WIDE.iter().any(|&(lo, hi)| (lo..=hi).contains(&cp)) {
            2.0
        } else {
            1.0
        };
        assert_eq!(
            number(&s, &format!("=LENB(UNICHAR({cp}))")),
            want,
            "U+{cp:04X}"
        );
        checked += 1;
    }
    assert!(checked > 60_000);
}

#[test]
fn a_boundary_inside_a_two_byte_character_gives_a_space() {
    let s = SheetEngine::new("S");
    for (formula, want) in [
        ("=LEFTB(\"한글\",1)", " "),
        ("=LEFTB(\"한글\",2)", "한"),
        ("=LEFTB(\"한글\",3)", "한 "),
        ("=LEFTB(\"a한\",2)", "a "),
        ("=LEFTB(\"한글\",99)", "한글"),
        ("=LEFTB(\"한글\",0)", ""),
        ("=LEFTB(\"abc\")", "a"),
        ("=RIGHTB(\"한글\",1)", " "),
        ("=RIGHTB(\"한글\",3)", " 글"),
        ("=RIGHTB(\"한글\",4)", "한글"),
        ("=RIGHTB(\"한글\",9)", "한글"),
        ("=RIGHTB(\"ab한\",2)", "한"),
        ("=MIDB(\"한글한글\",2,3)", " 글"),
        ("=MIDB(\"한글한글\",2,1)", " "),
        ("=MIDB(\"한글한글\",3,4)", "글한"),
        ("=MIDB(\"abc\",10,2)", ""),
        ("=MIDB(\"abc\",2,0)", ""),
        ("=REPLACEB(\"한글한글\",3,2,\"x\")", "한x한글"),
        ("=REPLACEB(\"한글한글\",2,2,\"x\")", " x 한글"),
        ("=REPLACEB(\"abc\",9,1,\"Z\")", "abcZ"),
        ("=REPLACEB(\"abc\",2,99,\"Z\")", "aZ"),
    ] {
        assert_eq!(text(&s, formula), want, "{formula}");
    }
    for formula in [
        "=LEFTB(\"a\",-1)",
        "=RIGHTB(\"a\",-1)",
        "=MIDB(\"a\",0,1)",
        "=MIDB(\"a\",1,-1)",
        "=REPLACEB(\"a\",0,1,\"b\")",
        "=FINDB(\"a\",\"abc\",0)",
        "=FINDB(\"a\",\"abc\",4)",
        "=FINDB(\"z\",\"abc\")",
        "=SEARCHB(\"z\",\"abc\")",
    ] {
        assert_eq!(eval(&s, formula), V::Error(E::Value), "{formula}");
    }
}

#[test]
fn byte_positions_of_find_and_search() {
    let s = SheetEngine::new("S");
    for (formula, want) in [
        ("=FINDB(\"글\",\"한글한글\")", 3.0),
        ("=FINDB(\"글\",\"한글한글\",4)", 7.0),
        ("=FINDB(\"글\",\"한글한글\",3)", 3.0),
        ("=FINDB(\"c\",\"한글abc\")", 7.0),
        ("=FINDB(\"\",\"abc\",2)", 2.0),
        ("=SEARCHB(\"G\",\"한글abGc\")", 7.0),
        ("=SEARCHB(\"?c\",\"한글abc\")", 6.0),
        ("=SEARCHB(\"한*c\",\"가한글abc\")", 3.0),
        ("=FINDB(\"G\",\"abg\")+0", f64::NAN),
    ] {
        if want.is_nan() {
            assert_eq!(eval(&s, formula), V::Error(E::Value), "{formula}");
        } else {
            assert_eq!(number(&s, formula), want, "{formula}");
        }
    }
}

#[test]
fn full_width_and_half_width_conversions() {
    let s = SheetEngine::new("S");
    for (formula, want) in [
        ("=ASC(\"ＡＢＣ　１２３\")", "ABC 123"),
        ("=ASC(\"￠￡￢￣￤￥￦\")", "¢£¬¯¦¥₩"),
        ("=ASC(\"カタカナ・ー「」。、\")", "ｶﾀｶﾅ･ｰ｢｣｡､"),
        ("=ASC(\"ガギグゲゴ\")", "ｶﾞｷﾞｸﾞｹﾞｺﾞ"),
        ("=ASC(\"パピプペポヴ\")", "ﾊﾟﾋﾟﾌﾟﾍﾟﾎﾟｳﾞ"),
        ("=ASC(\"ァィゥェォャュョッ\")", "ｧｨｩｪｫｬｭｮｯ"),
        ("=ASC(\"한글漢字ひらがな\")", "한글漢字ひらがな"),
        ("=DBCS(\"ABC 123\")", "ＡＢＣ　１２３"),
        ("=DBCS(\"¢£¬¯¦¥₩\")", "￠￡￢￣￤￥￦"),
        ("=DBCS(\"ｶﾀｶﾅ･ｰ｢｣｡､\")", "カタカナ・ー「」。、"),
        ("=DBCS(\"ｶﾞｷﾞｸﾞｹﾞｺﾞ\")", "ガギグゲゴ"),
        ("=DBCS(\"ﾊﾟﾋﾟﾌﾟﾍﾟﾎﾟｳﾞ\")", "パピプペポヴ"),
        ("=DBCS(\"ﾊﾞﾋﾞﾌﾞﾍﾞﾎﾞﾂﾞﾁﾞ\")", "バビブベボヅヂ"),
        ("=DBCS(\"ｱﾞ\")", "ア゛"),
        ("=DBCS(\"한글\")", "한글"),
    ] {
        assert_eq!(text(&s, formula), want, "{formula}");
    }
}

#[test]
fn conversions_are_inverse_over_their_tables() {
    let s = SheetEngine::new("S");
    // full-width forms: ASCII block, symbols, katakana and its marks
    let full = (0xFF01u32..=0xFF5E)
        .chain(0xFFE0..=0xFFE6)
        .chain(0x30A1..=0x30FC)
        .chain([0x3000, 0x3001, 0x3002, 0x300C, 0x300D, 0x309B, 0x309C]);
    for cp in full {
        assert_eq!(
            text(&s, &format!("=DBCS(ASC(UNICHAR({cp})))")),
            char::from_u32(cp).unwrap().to_string(),
            "U+{cp:04X}"
        );
    }
    // half-width forms
    let half = (0x20u32..=0x7E)
        .chain([0xA2, 0xA3, 0xAC, 0xAF, 0xA6, 0xA5, 0x20A9])
        .chain(0xFF61..=0xFF9F);
    for cp in half {
        assert_eq!(
            text(&s, &format!("=ASC(DBCS(UNICHAR({cp})))")),
            char::from_u32(cp).unwrap().to_string(),
            "U+{cp:04X}"
        );
    }
}

#[test]
fn hostile_text_stays_inside_the_cell_text_limit() {
    let mut s = SheetEngine::new("S");
    s.set_cell("A1", "=REPT(\"ガ\",20000)").unwrap();
    s.set_cell("A2", "=REPT(\"한\",32000)").unwrap();
    // a voiced katakana becomes two characters
    assert_eq!(eval(&s, "=ASC(A1)"), V::Error(E::Value));
    assert_eq!(number(&s, "=LEN(ASC(REPT(\"ガ\",10000)))"), 20_000.0);
    assert_eq!(number(&s, "=LENB(A2)"), 64_000.0);
    assert_eq!(text(&s, "=LEFTB(A2,3)"), "한 ");
    assert_eq!(number(&s, "=LEN(RIGHTB(A2,63999))"), 32_000.0);
    assert_eq!(number(&s, "=LEN(MIDB(A2,2,1E15))"), 32_000.0);
    assert_eq!(
        eval(&s, "=REPLACEB(A2,1,1,REPT(\"x\",2000))"),
        V::Error(E::Value)
    );
    assert_eq!(number(&s, "=LEN(LEFTB(A2,1E300))"), 32_000.0);
    assert_eq!(number(&s, "=FINDB(\"한\",A2,63999)"), 63_999.0);
    assert_eq!(number(&s, "=SEARCHB(\"*한\",A2)"), 1.0);
}

fn aggregate_sheet() -> SheetEngine {
    let mut s = SheetEngine::new("S");
    for (addr, raw) in [
        ("A1", "1"),
        ("A2", "2"),
        ("A3", "=SUBTOTAL(9,A1:A2)"),
        ("A4", "4"),
        ("B1", "10"),
        ("B2", "=#DIV/0!"),
        ("B3", "30"),
        ("B4", "=\"x\""),
        ("C1", "=AGGREGATE(9,0,A1:A2)"),
        ("C2", "5"),
    ] {
        s.set_cell(addr, raw).unwrap();
    }
    s
}

#[test]
fn aggregate_options_skip_nested_results_and_errors() {
    let s = aggregate_sheet();
    for (formula, want) in [
        // options 0 to 3 skip the nested SUBTOTAL cell, 4 to 7 keep it
        ("=AGGREGATE(9,0,A1:A4)", 7.0),
        ("=AGGREGATE(9,4,A1:A4)", 10.0),
        ("=AGGREGATE(2,0,A1:A4)", 3.0),
        ("=AGGREGATE(2,4,A1:A4)", 4.0),
        // the AGGREGATE cell is skipped like a SUBTOTAL cell
        ("=AGGREGATE(9,0,C1:C2)", 5.0),
        ("=AGGREGATE(9,4,C1:C2)", 8.0),
        ("=SUBTOTAL(9,C1:C2)", 5.0),
        // options 2, 3, 6 and 7 skip errors
        ("=AGGREGATE(9,2,B1:B4)", 40.0),
        ("=AGGREGATE(9,6,B1:B4)", 40.0),
        ("=AGGREGATE(14,2,B1:B4,1)", 30.0),
        ("=AGGREGATE(15,6,B1:B4,1)", 10.0),
        ("=AGGREGATE(2,0,B1:B4)", 2.0),
        ("=AGGREGATE(3,0,B1:B4)", 4.0),
    ] {
        assert_eq!(number(&s, formula), want, "{formula}");
    }
    for (formula, want) in [
        ("=AGGREGATE(9,0,B1:B4)", E::DivByZero),
        ("=AGGREGATE(14,0,B1:B4,1)", E::DivByZero),
        ("=AGGREGATE(0,0,A1:A4)", E::Value),
        ("=AGGREGATE(20,0,A1:A4)", E::Value),
        ("=AGGREGATE(9,8,A1:A4)", E::Value),
        ("=AGGREGATE(9,-1,A1:A4)", E::Value),
        ("=AGGREGATE(9,0)", E::Value),
        ("=AGGREGATE(14,0,A1:A4)", E::Value),
        ("=AGGREGATE(14,0,A1:A4,A1:A2)", E::Value),
        ("=AGGREGATE(14,0,A1:A4,0)", E::Num),
        ("=AGGREGATE(14,0,A1:A4,9)", E::Num),
        ("=AGGREGATE(16,0,A1:A4,1.5)", E::Num),
        ("=AGGREGATE(17,0,A1:A4,5)", E::Num),
        ("=AGGREGATE(19,0,A1:A4,4)", E::Num),
        ("=AGGREGATE(18,0,A1:A4,0.99)", E::Num),
        ("=AGGREGATE(12,0,D1:D2)", E::Num),
    ] {
        assert_eq!(eval(&s, formula), V::Error(want), "{formula}");
    }
}

#[test]
fn aggregate_order_statistics_match_the_plain_functions() {
    let mut s = SheetEngine::new("S");
    for (i, x) in [7.0, 3.0, 9.0, 1.0, 5.0, 8.0, 2.0].iter().enumerate() {
        s.set_cell(&format!("A{}", i + 1), x.to_string()).unwrap();
    }
    for (code, plain, k) in [
        (14, "LARGE", "3"),
        (15, "SMALL", "3"),
        (16, "PERCENTILE.INC", "0.3"),
        (17, "QUARTILE.INC", "1"),
        (18, "PERCENTILE.EXC", "0.3"),
        (19, "QUARTILE.EXC", "3"),
    ] {
        assert_eq!(
            eval(&s, &format!("=AGGREGATE({code},0,A1:A7,{k})")),
            eval(&s, &format!("={plain}(A1:A7,{k})")),
            "{plain}"
        );
    }
    assert_eq!(number(&s, "=AGGREGATE(12,0,A1:A7)"), 5.0);
    assert_eq!(eval(&s, "=AGGREGATE(13,0,A1:A7)"), V::Error(E::Na));
}

#[test]
fn distribution_limits_and_hostile_arguments() {
    let s = SheetEngine::new("S");
    let near = |formula: &str, want: f64, tol: f64| {
        let got = number(&s, formula);
        assert!((got - want).abs() <= tol, "{formula}: {got}");
    };
    // large populations use the sum from the mode, not a term-by-term walk from the start
    near("=HYPGEOM.DIST(5000,10000,50000,100000,TRUE)", 0.5, 0.01);
    near("=HYPGEOM.DIST(0,10000,50000,100000,TRUE)", 0.0, 1e-300);
    near("=HYPGEOM.DIST(10000,10000,100000,100000,TRUE)", 1.0, 1e-12);
    assert_eq!(
        eval(
            &s,
            "=HYPGEOM.DIST(3000000000,5000000000,5000000000,10000000000,TRUE)"
        ),
        V::Error(E::Num)
    );
    assert_eq!(
        eval(&s, "=HYPGEOM.DIST(1,4,8,10000000001,TRUE)"),
        V::Error(E::Num)
    );
    // a wide range uses the cumulative form
    near("=BINOM.DIST.RANGE(1000000,0.5,400000,600000)", 1.0, 1e-12);
    near(
        "=BINOM.DIST.RANGE(10000000000,0.5,0,10000000000)",
        1.0,
        1e-12,
    );
    near("=BINOM.DIST.RANGE(10,0.5,5)", 0.24609375, 1e-12);
    for formula in [
        "=BINOM.DIST.RANGE(10000000001,0.5,1,2)",
        "=F.DIST(1,10000000001,2,TRUE)",
        "=F.DIST(-1,2,2,TRUE)",
        "=F.INV(1,2,2)",
        "=F.INV.RT(0,2,2)",
        "=F.INV(1.5,2,2)",
        "=WEIBULL.DIST(0,0.5,1,FALSE)",
        "=EXPON.DIST(1,0,TRUE)",
        "=LOGNORM.DIST(0,0,1,TRUE)",
        "=POISSON.DIST(-1,1,TRUE)",
        "=NEGBINOM.DIST(1,0,0.5,TRUE)",
        "=CONFIDENCE.NORM(0,1,10)",
        "=CONFIDENCE.NORM(0.05,1,0)",
        "=T.TEST(A1:A2,A1:A2,3,1)",
    ] {
        assert_eq!(eval(&s, formula), V::Error(E::Num), "{formula}");
    }
    assert_eq!(eval(&s, "=CONFIDENCE.T(0.05,1,1)"), V::Error(E::DivByZero));
    near("=F.INV.RT(1,6,4)", 0.0, 0.0);
    near("=F.INV(0,6,4)", 0.0, 0.0);
    near("=POISSON.DIST(1000000000,1000000000,TRUE)", 0.5, 2e-3);
    // a huge mean with x = 0 underflows to 0
    near("=POISSON.DIST(0,1000000,FALSE)", 0.0, 0.0);
    // legacy names evaluate like the new names
    for (legacy, modern) in [
        ("=FDIST(2.5,6,4)", "=F.DIST.RT(2.5,6,4)"),
        ("=FINV(0.1,6,4)", "=F.INV.RT(0.1,6,4)"),
        ("=EXPONDIST(1,2,TRUE)", "=EXPON.DIST(1,2,TRUE)"),
        ("=WEIBULL(1,2,3,TRUE)", "=WEIBULL.DIST(1,2,3,TRUE)"),
        ("=POISSON(3,2,FALSE)", "=POISSON.DIST(3,2,FALSE)"),
        ("=LOGINV(0.3,1,2)", "=LOGNORM.INV(0.3,1,2)"),
        ("=CONFIDENCE(0.05,2,30)", "=CONFIDENCE.NORM(0.05,2,30)"),
        ("=LOGNORMDIST(2,1,1)", "=LOGNORM.DIST(2,1,1,TRUE)"),
        ("=NEGBINOMDIST(3,2,0.4)", "=NEGBINOM.DIST(3,2,0.4,FALSE)"),
        ("=HYPGEOMDIST(1,4,8,20)", "=HYPGEOM.DIST(1,4,8,20,FALSE)"),
    ] {
        assert_eq!(eval(&s, legacy), eval(&s, modern), "{legacy}");
    }
}

#[test]
fn significance_tests_and_weighted_average_on_ranges() {
    let mut s = SheetEngine::new("S");
    for (addr, raw) in [
        ("A1", "1"),
        ("A2", "2"),
        ("A3", "3"),
        ("A4", "4"),
        ("B1", "2"),
        ("B2", "4"),
        ("B3", "5"),
        ("B4", "9"),
        ("C1", "1"),
        ("C2", "1"),
        ("C3", "2"),
        ("C4", "0"),
    ] {
        s.set_cell(addr, raw).unwrap();
    }
    assert_eq!(number(&s, "=AVERAGE.WEIGHTED(A1:A4,C1:C4)"), 2.25);
    assert_eq!(
        number(&s, "=AVERAGE.WEIGHTED(A1:A4,C1:C4,B1:B2,C1:C2)"),
        2.5
    );
    assert_eq!(
        eval(&s, "=AVERAGE.WEIGHTED(A1:A4,C1:C3)"),
        V::Error(E::Value)
    );
    assert_eq!(eval(&s, "=AVERAGE.WEIGHTED(A1:A4)"), V::Error(E::Value));
    assert_eq!(
        eval(&s, "=AVERAGE.WEIGHTED(A1:A4,A1:A4,C4:C4)"),
        V::Error(E::Na)
    );
    // two samples, tails 2, type 2: the P value of a two-sample t test
    let p = number(&s, "=T.TEST(A1:A4,B1:B4,2,2)");
    assert!(p > 0.0 && p < 1.0, "{p}");
    assert_eq!(number(&s, "=TTEST(A1:A4,B1:B4,2,2)"), p);
    assert!(number(&s, "=T.TEST(A1:A4,B1:B4,1,2)") * 2.0 - p < 1e-12);
    let f = number(&s, "=F.TEST(A1:A4,B1:B4)");
    assert!(f > 0.0 && f <= 1.0, "{f}");
    let chi = number(&s, "=CHISQ.TEST(A1:A4,B1:B4)");
    assert!(chi > 0.0 && chi < 1.0, "{chi}");
    assert_eq!(eval(&s, "=CHISQ.TEST(A1:A4,A1:A3)"), V::Error(E::Na));
    assert_eq!(eval(&s, "=CHISQ.TEST(A1:A4,C1:C4)"), V::Error(E::DivByZero));
    assert_eq!(eval(&s, "=MARGINOFERROR(A1:A4,1)"), V::Error(E::Num));
    let m = number(&s, "=MARGINOFERROR(A1:A4,0.95)");
    let c = number(&s, "=CONFIDENCE.T(0.05,STDEV.S(A1:A4),4)");
    assert!((m - c).abs() < 1e-9, "{m} {c}");
}

#[test]
fn trig_and_rounding_edges() {
    let s = SheetEngine::new("S");
    for (formula, want) in [
        ("=ROUNDBANK(2.5,0)", 2.0),
        ("=ROUNDBANK(3.5,0)", 4.0),
        ("=ROUNDBANK(-2.5,0)", -2.0),
        ("=ROUNDBANK(0.125,2)", 0.12),
        ("=ROUNDBANK(1234,-2)", 1200.0),
        ("=ROUNDBANK(1.5,17)", 1.5),
        ("=ROUNDBANK(1.5,-17)", 0.0),
        ("=CEILING.MATH(-5.5,2)", -4.0),
        ("=CEILING.MATH(-5.5,2,1)", -6.0),
        ("=FLOOR.MATH(-5.5,2)", -6.0),
        ("=FLOOR.MATH(-5.5,2,1)", -4.0),
        ("=CEILING.PRECISE(4.3,-2)", 6.0),
        ("=FLOOR.PRECISE(-3.2,-1)", -4.0),
        ("=CEILING.MATH(0.3,0.1)", 0.3),
        ("=SECH(1000)", 0.0),
        ("=CSCH(1000)", 0.0),
    ] {
        let got = number(&s, formula);
        assert!((got - want).abs() < 1e-12, "{formula}: {got}");
    }
    for (formula, want) in [
        ("=COT(0)", E::DivByZero),
        ("=CSC(0)", E::DivByZero),
        ("=COTH(0)", E::DivByZero),
        ("=CSCH(0)", E::DivByZero),
        ("=COT(134217728)", E::Num),
        ("=SEC(134217728)", E::Num),
        ("=ACOTH(1)", E::Num),
        ("=ACOSH(0.5)", E::Num),
        ("=ATANH(1)", E::Num),
        ("=CEILING.MATH(1,2,3,4)", E::Value),
    ] {
        assert_eq!(eval(&s, formula), V::Error(want), "{formula}");
    }
}

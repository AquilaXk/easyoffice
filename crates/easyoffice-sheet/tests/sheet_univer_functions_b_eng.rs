//! Formula function bundle B, engineering (PD-201): Bessel functions, unit conversion and complex-number functions.
//!
//! A complex result whose imaginary part is 0 is text here like every other `IM*` function (the reference
//! returns a number), and complex parts are compared with a relative tolerance of 1e-12 (the reference
//! rounds in decimal arithmetic).
//!
//! Generated from the reference implementation's unit tests: same data, same expected values.
//! A case marked `DIFFERS` keeps the reference value in its comment; see PD-201.

// the expected values are the reference's literals, some of them close to a named constant
#![allow(clippy::approx_constant)]

use easyoffice_sheet::functions_complex::parse_complex;
use easyoffice_sheet::{
    EvaluatedCellValue as V, FormulaError as E, PackedCellCoordinate, SheetEngine,
};

enum Exp {
    Num(f64),
    NumText(f64),
    Complex(&'static str),
    Close(f64, f64),
    Err(E),
}

struct Case {
    id: &'static str,
    cells: &'static [(&'static str, &'static str)],
    formula: &'static str,
    expect: Exp,
}

fn same_complex(got: &str, want: &str) -> bool {
    let (Ok((gr, gi, gs)), Ok((wr, wi, ws))) = (parse_complex(got), parse_complex(want)) else {
        return false;
    };
    let near = |g: f64, w: f64| (g - w).abs() <= 1e-12 * g.abs().max(w.abs()).max(1e-300);
    gs == ws && near(gr, wr) && near(gi, wi)
}

fn matches(got: &V, want: &Exp) -> bool {
    match (got, want) {
        (V::Number(g), Exp::Num(w)) => (g - w).abs() <= 1e-9 * w.abs().max(1.0),
        (V::Number(g), Exp::Close(w, tol)) => (g - w).abs() <= *tol,
        (V::Number(g), Exp::NumText(w)) => (g - w).abs() <= 1e-12 * w.abs().max(1.0),
        (V::Text(g), Exp::NumText(w)) => g
            .parse::<f64>()
            .is_ok_and(|g| (g - w).abs() <= 1e-12 * w.abs().max(1.0)),
        (V::Text(g), Exp::Complex(w)) => same_complex(g, w),
        (V::Error(g), Exp::Err(w)) => g == w,
        _ => false,
    }
}

fn run(cases: &[Case]) -> Vec<String> {
    let mut failures = Vec::new();
    for case in cases {
        let mut s = SheetEngine::new("S");
        for (addr, raw) in case.cells {
            let at = PackedCellCoordinate::parse_a1(addr).unwrap();
            s.set_cell(&at.to_a1(), *raw).unwrap();
        }
        let got = s.evaluate_formula_str(case.formula);
        if !matches(&got, &case.expect) {
            failures.push(format!("{} {}: got {got:?}", case.id, case.formula));
        }
    }
    failures
}

// BESSELI: 7 cases (3 not ported: 2 array-broadcast, 1904 date system or an error type the engine lacks)
const BESSELI: &[Case] = &[
    Case {
        id: "besseli#0",
        cells: &[],
        formula: "=BESSELI(1.5,1)",
        expect: Exp::Num(0.981666428475),
    },
    Case {
        id: "besseli#1",
        cells: &[],
        formula: "=BESSELI(1.5,-1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "besseli#2",
        cells: &[],
        formula: "=BESSELI(\"-0.5\",1)",
        expect: Exp::Num(-0.257894303289),
    },
    Case {
        id: "besseli#3",
        cells: &[],
        formula: "=BESSELI(\"test\",1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "besseli#4",
        cells: &[],
        formula: "=BESSELI(TRUE,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "besseli#5",
        cells: &[],
        formula: "=BESSELI(ZZ999,1)",
        expect: Exp::Err(E::Na),
    },
    Case {
        id: "besseli#6",
        cells: &[],
        formula: "=BESSELI(#NAME?,1)",
        expect: Exp::Err(E::Name),
    },
];

#[test]
fn besseli_matches_the_reference_cases() {
    let failures = run(BESSELI);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// BESSELJ: 8 cases (2 not ported: 2 array-broadcast, 1904 date system or an error type the engine lacks)
const BESSELJ: &[Case] = &[
    Case {
        id: "besselj#0",
        cells: &[],
        formula: "=BESSELJ(1.5,1)",
        expect: Exp::Num(0.557936507891),
    },
    Case {
        id: "besselj#1",
        cells: &[],
        formula: "=BESSELJ(1.5,-1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "besselj#2",
        cells: &[],
        formula: "=BESSELJ(2,222)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "besselj#3",
        cells: &[],
        formula: "=BESSELJ(\"-0.5\",1)",
        expect: Exp::Num(-0.24226845768),
    },
    Case {
        id: "besselj#4",
        cells: &[],
        formula: "=BESSELJ(\"test\",1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "besselj#5",
        cells: &[],
        formula: "=BESSELJ(TRUE,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "besselj#6",
        cells: &[],
        formula: "=BESSELJ(ZZ999,1)",
        expect: Exp::Err(E::Na),
    },
    Case {
        id: "besselj#7",
        cells: &[],
        formula: "=BESSELJ(#NAME?,1)",
        expect: Exp::Err(E::Name),
    },
];

#[test]
fn besselj_matches_the_reference_cases() {
    let failures = run(BESSELJ);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// BESSELK: 7 cases (2 not ported: 2 array-broadcast, 1904 date system or an error type the engine lacks)
const BESSELK: &[Case] = &[
    Case {
        id: "besselk#0",
        cells: &[],
        formula: "=BESSELK(1.5,1)",
        expect: Exp::Num(0.277387803632),
    },
    Case {
        id: "besselk#1",
        cells: &[],
        formula: "=BESSELK(1.5,-1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "besselk#2",
        cells: &[],
        formula: "=BESSELK(\"-0.5\",1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "besselk#3",
        cells: &[],
        formula: "=BESSELK(\"test\",1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "besselk#4",
        cells: &[],
        formula: "=BESSELK(TRUE,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "besselk#5",
        cells: &[],
        formula: "=BESSELK(ZZ999,1)",
        expect: Exp::Err(E::Na),
    },
    Case {
        id: "besselk#6",
        cells: &[],
        formula: "=BESSELK(#NAME?,1)",
        expect: Exp::Err(E::Name),
    },
];

#[test]
fn besselk_matches_the_reference_cases() {
    let failures = run(BESSELK);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// BESSELY: 7 cases (2 not ported: 2 array-broadcast, 1904 date system or an error type the engine lacks)
const BESSELY: &[Case] = &[
    Case {
        id: "bessely#0",
        cells: &[],
        formula: "=BESSELY(1.5,1)",
        expect: Exp::Num(-0.412308626896),
    },
    Case {
        id: "bessely#1",
        cells: &[],
        formula: "=BESSELY(1.5,-1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "bessely#2",
        cells: &[],
        formula: "=BESSELY(\"-0.5\",1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "bessely#3",
        cells: &[],
        formula: "=BESSELY(\"test\",1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "bessely#4",
        cells: &[],
        formula: "=BESSELY(TRUE,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "bessely#5",
        cells: &[],
        formula: "=BESSELY(ZZ999,1)",
        expect: Exp::Err(E::Na),
    },
    Case {
        id: "bessely#6",
        cells: &[],
        formula: "=BESSELY(#NAME?,1)",
        expect: Exp::Err(E::Name),
    },
];

#[test]
fn bessely_matches_the_reference_cases() {
    let failures = run(BESSELY);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// CONVERT: 36 cases (1 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const CONVERT: &[Case] = &[
    Case {
        id: "convert#0",
        cells: &[],
        formula: "=CONVERT(1,\"lbm\",\"kg\")",
        expect: Exp::Num(0.45359237),
    },
    Case {
        id: "convert#1",
        cells: &[],
        formula: "=CONVERT(\"-0.5\",\"lbm\",\"kg\")",
        expect: Exp::Num(-0.226796185),
    },
    Case {
        id: "convert#2",
        cells: &[],
        formula: "=CONVERT(\"test\",\"lbm\",\"kg\")",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "convert#3",
        cells: &[],
        formula: "=CONVERT(TRUE,\"lbm\",\"kg\")",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "convert#4",
        cells: &[],
        formula: "=CONVERT(ZZ999,\"lbm\",\"kg\")",
        expect: Exp::Num(0.0),
    },
    Case {
        id: "convert#5",
        cells: &[],
        formula: "=CONVERT(#NAME?,\"lbm\",\"kg\")",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "convert#6",
        cells: &[],
        formula: "=CONVERT(2.5,\"ft\",\"sec\")",
        expect: Exp::Err(E::Na),
    },
    Case {
        id: "convert#7",
        cells: &[],
        formula: "=CONVERT(1,\"KiB\",\"bytes\")",
        expect: Exp::Err(E::Na),
    },
    Case {
        id: "convert#8",
        cells: &[],
        formula: "=CONVERT(1,\"km\",\"m\")",
        expect: Exp::Num(1000.0),
    },
    Case {
        id: "convert#9",
        cells: &[],
        formula: "=CONVERT(0,\"C\",\"F\")",
        expect: Exp::Num(32.0),
    },
    Case {
        id: "convert#10",
        cells: &[],
        formula: "=CONVERT(0,\"C\",\"K\")",
        expect: Exp::Num(273.15),
    },
    Case {
        id: "convert#11",
        cells: &[],
        formula: "=CONVERT(0,\"C\",\"Rank\")",
        expect: Exp::Close(491.67, 5e-13),
    },
    Case {
        id: "convert#12",
        cells: &[],
        formula: "=CONVERT(0,\"C\",\"Reau\")",
        expect: Exp::Num(0.0),
    },
    Case {
        id: "convert#13",
        cells: &[],
        formula: "=CONVERT(32,\"F\",\"C\")",
        expect: Exp::Num(0.0),
    },
    Case {
        id: "convert#14",
        cells: &[],
        formula: "=CONVERT(4,\"F\",\"C\")",
        expect: Exp::Close(-15.555555555555555, 5e-13),
    },
    Case {
        id: "convert#15",
        cells: &[],
        formula: "=CONVERT(32,\"F\",\"K\")",
        expect: Exp::Num(273.15),
    },
    Case {
        id: "convert#16",
        cells: &[],
        formula: "=CONVERT(32,\"F\",\"Rank\")",
        expect: Exp::Close(491.67, 5e-13),
    },
    Case {
        id: "convert#17",
        cells: &[],
        formula: "=CONVERT(32,\"F\",\"Reau\")",
        expect: Exp::Num(0.0),
    },
    Case {
        id: "convert#18",
        cells: &[],
        formula: "=CONVERT(273.15,\"K\",\"C\")",
        expect: Exp::Num(0.0),
    },
    Case {
        id: "convert#19",
        cells: &[],
        formula: "=CONVERT(273.15,\"K\",\"F\")",
        expect: Exp::Num(32.0),
    },
    Case {
        id: "convert#20",
        cells: &[],
        formula: "=CONVERT(273.15,\"K\",\"Rank\")",
        expect: Exp::Close(491.67, 5e-13),
    },
    Case {
        id: "convert#21",
        cells: &[],
        formula: "=CONVERT(273.15,\"K\",\"Reau\")",
        expect: Exp::Num(0.0),
    },
    Case {
        id: "convert#22",
        cells: &[],
        formula: "=CONVERT(491.67,\"Rank\",\"C\")",
        expect: Exp::Num(0.0),
    },
    Case {
        id: "convert#23",
        cells: &[],
        formula: "=CONVERT(491.67,\"Rank\",\"F\")",
        expect: Exp::Num(32.0),
    },
    Case {
        id: "convert#24",
        cells: &[],
        formula: "=CONVERT(491.67,\"Rank\",\"K\")",
        expect: Exp::Num(273.15),
    },
    Case {
        id: "convert#25",
        cells: &[],
        formula: "=CONVERT(491.67,\"Rank\",\"Reau\")",
        expect: Exp::Num(0.0),
    },
    Case {
        id: "convert#26",
        cells: &[],
        formula: "=CONVERT(0,\"Reau\",\"C\")",
        expect: Exp::Num(0.0),
    },
    Case {
        id: "convert#27",
        cells: &[],
        formula: "=CONVERT(0,\"Reau\",\"F\")",
        expect: Exp::Num(32.0),
    },
    Case {
        id: "convert#28",
        cells: &[],
        formula: "=CONVERT(0,\"Reau\",\"K\")",
        expect: Exp::Num(273.15),
    },
    Case {
        id: "convert#29",
        cells: &[],
        formula: "=CONVERT(0,\"Reau\",\"Rank\")",
        expect: Exp::Num(491.67),
    },
    Case {
        id: "convert#30",
        cells: &[],
        formula: "=CONVERT(1,\"unknown\",\"m\")",
        expect: Exp::Err(E::Na),
    },
    Case {
        id: "convert#31",
        cells: &[],
        formula: "=CONVERT(1,\"m\",\"unknown\")",
        expect: Exp::Err(E::Na),
    },
    Case {
        id: "convert#32",
        cells: &[],
        formula: "=CONVERT(1,\"inch\",\"cm\")",
        expect: Exp::Err(E::Na),
    },
    Case {
        id: "convert#33",
        cells: &[],
        formula: "=CONVERT(1e+21,\"m\",\"km\")",
        expect: Exp::Num(1000000000000000000.0),
    },
    Case {
        id: "convert#34",
        cells: &[],
        formula: "=CONVERT(1e-21,\"m\",\"km\")",
        expect: Exp::Num(1e-24),
    },
    Case {
        id: "convert#35",
        cells: &[],
        formula: "=CONVERT(1,\"mph\",\"km/h\")",
        expect: Exp::Num(1.609344),
    },
];

#[test]
fn convert_matches_the_reference_cases() {
    let failures = run(CONVERT);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// IMCOSH: 6 cases (2 not ported: 2 array-broadcast, 1904 date system or an error type the engine lacks)
const IMCOSH: &[Case] = &[
    Case {
        id: "imcosh#0",
        cells: &[],
        formula: "=IMCOSH(\"5+12i\")",
        expect: Exp::Complex("62.6223588399767-39.8154332247249i"),
    },
    Case {
        id: "imcosh#1",
        cells: &[],
        formula: "=IMCOSH(25698432)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "imcosh#2",
        cells: &[],
        formula: "=IMCOSH(\"1.5\")",
        expect: Exp::NumText(2.35240961524325),
    },
    Case {
        id: "imcosh#3",
        cells: &[],
        formula: "=IMCOSH(\"test\")",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "imcosh#4",
        cells: &[],
        formula: "=IMCOSH(TRUE)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "imcosh#5",
        cells: &[],
        formula: "=IMCOSH(#NAME?)",
        expect: Exp::Err(E::Name),
    },
];

#[test]
fn imcosh_matches_the_reference_cases() {
    let failures = run(IMCOSH);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// IMCOT: 6 cases (2 not ported: 2 array-broadcast, 1904 date system or an error type the engine lacks)
const IMCOT: &[Case] = &[
    Case {
        id: "imcot#0",
        cells: &[],
        formula: "=IMCOT(\"5+12i\")",
        expect: Exp::Complex("-4.10750577681088E-11-0.999999999936648i"),
    },
    Case {
        id: "imcot#1",
        cells: &[],
        formula: "=IMCOT(25698432)",
        expect: Exp::NumText(-9.06359967954937),
    },
    Case {
        id: "imcot#2",
        cells: &[],
        formula: "=IMCOT(\"1.5\")",
        expect: Exp::NumText(0.0709148443026525),
    },
    Case {
        id: "imcot#3",
        cells: &[],
        formula: "=IMCOT(\"test\")",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "imcot#4",
        cells: &[],
        formula: "=IMCOT(TRUE)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "imcot#5",
        cells: &[],
        formula: "=IMCOT(#NAME?)",
        expect: Exp::Err(E::Name),
    },
];

#[test]
fn imcot_matches_the_reference_cases() {
    let failures = run(IMCOT);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// IMCOTH: 6 cases (2 not ported: 2 array-broadcast, 1904 date system or an error type the engine lacks)
const IMCOTH: &[Case] = &[
    Case {
        id: "imcoth#0",
        cells: &[],
        formula: "=IMCOTH(\"5+12i\")",
        expect: Exp::Complex("1.00003851275523+0.0000822295549932979i"),
    },
    Case {
        id: "imcoth#1",
        cells: &[],
        formula: "=IMCOTH(25698432)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "imcoth#2",
        cells: &[],
        formula: "=IMCOTH(\"1.5\")",
        expect: Exp::NumText(1.10479139298251),
    },
    Case {
        id: "imcoth#3",
        cells: &[],
        formula: "=IMCOTH(\"test\")",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "imcoth#4",
        cells: &[],
        formula: "=IMCOTH(TRUE)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "imcoth#5",
        cells: &[],
        formula: "=IMCOTH(#NAME?)",
        expect: Exp::Err(E::Name),
    },
];

#[test]
fn imcoth_matches_the_reference_cases() {
    let failures = run(IMCOTH);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// IMCSC: 6 cases (2 not ported: 2 array-broadcast, 1904 date system or an error type the engine lacks)
const IMCSC: &[Case] = &[
    Case {
        id: "imcsc#0",
        cells: &[],
        formula: "=IMCSC(\"5+12i\")",
        expect: Exp::Complex("-0.0000117836687482814-3.48576140783804E-06i"),
    },
    Case {
        id: "imcsc#1",
        cells: &[],
        formula: "=IMCSC(25698432)",
        expect: Exp::NumText(9.11859852998954),
    },
    Case {
        id: "imcsc#2",
        cells: &[],
        formula: "=IMCSC(\"1.5\")",
        expect: Exp::NumText(1.00251130424672),
    },
    Case {
        id: "imcsc#3",
        cells: &[],
        formula: "=IMCSC(\"test\")",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "imcsc#4",
        cells: &[],
        formula: "=IMCSC(TRUE)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "imcsc#5",
        cells: &[],
        formula: "=IMCSC(#NAME?)",
        expect: Exp::Err(E::Name),
    },
];

#[test]
fn imcsc_matches_the_reference_cases() {
    let failures = run(IMCSC);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// IMCSCH: 6 cases (2 not ported: 2 array-broadcast, 1904 date system or an error type the engine lacks)
const IMCSCH: &[Case] = &[
    Case {
        id: "imcsch#0",
        cells: &[],
        formula: "=IMCSCH(\"5+12i\")",
        expect: Exp::Complex("0.0113716081825818+0.00723140654863397i"),
    },
    Case {
        id: "imcsch#1",
        cells: &[],
        formula: "=IMCSCH(25698432)",
        expect: Exp::NumText(0.0),
    },
    Case {
        id: "imcsch#2",
        cells: &[],
        formula: "=IMCSCH(\"1.5\")",
        expect: Exp::NumText(0.469642440595225),
    },
    Case {
        id: "imcsch#3",
        cells: &[],
        formula: "=IMCSCH(\"test\")",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "imcsch#4",
        cells: &[],
        formula: "=IMCSCH(TRUE)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "imcsch#5",
        cells: &[],
        formula: "=IMCSCH(#NAME?)",
        expect: Exp::Err(E::Name),
    },
];

#[test]
fn imcsch_matches_the_reference_cases() {
    let failures = run(IMCSCH);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// IMLOG: 10 cases (4 not ported: 4 array-broadcast, 1904 date system or an error type the engine lacks)
const IMLOG: &[Case] = &[
    Case {
        id: "imlog#0",
        cells: &[],
        formula: "=IMLOG(\"5+12i\")",
        expect: Exp::Complex("1.11394335230684+0.510732572130908i"),
    },
    Case {
        id: "imlog#1",
        cells: &[],
        formula: "=IMLOG(\"5-12i\")",
        expect: Exp::Complex("1.11394335230684-0.510732572130908i"),
    },
    Case {
        id: "imlog#2",
        cells: &[],
        formula: "=IMLOG(\"5+12i\",0)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "imlog#3",
        cells: &[],
        formula: "=IMLOG(\"5+12i\",\"test\")",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "imlog#4",
        cells: &[],
        formula: "=IMLOG(\"5+12i\",1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "imlog#5",
        cells: &[],
        formula: "=IMLOG(25698432)",
        expect: Exp::NumText(7.40990662548997),
    },
    Case {
        id: "imlog#6",
        cells: &[],
        formula: "=IMLOG(\"1.5\")",
        expect: Exp::NumText(0.176091259055681),
    },
    Case {
        id: "imlog#7",
        cells: &[],
        formula: "=IMLOG(\"test\")",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "imlog#8",
        cells: &[],
        formula: "=IMLOG(TRUE)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "imlog#9",
        cells: &[],
        formula: "=IMLOG(#NAME?)",
        expect: Exp::Err(E::Name),
    },
];

#[test]
fn imlog_matches_the_reference_cases() {
    let failures = run(IMLOG);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// IMSEC: 6 cases (2 not ported: 2 array-broadcast, 1904 date system or an error type the engine lacks)
const IMSEC: &[Case] = &[
    Case {
        id: "imsec#0",
        cells: &[],
        formula: "=IMSEC(\"5+12i\")",
        expect: Exp::Complex("3.48576140854289E-06-0.0000117836687488848i"),
    },
    Case {
        id: "imsec#1",
        cells: &[],
        formula: "=IMSEC(25698432)",
        expect: Exp::NumText(-1.00606810234175),
    },
    Case {
        id: "imsec#2",
        cells: &[],
        formula: "=IMSEC(\"1.5\")",
        expect: Exp::NumText(14.1368329029699),
    },
    Case {
        id: "imsec#3",
        cells: &[],
        formula: "=IMSEC(\"test\")",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "imsec#4",
        cells: &[],
        formula: "=IMSEC(TRUE)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "imsec#5",
        cells: &[],
        formula: "=IMSEC(#NAME?)",
        expect: Exp::Err(E::Name),
    },
];

#[test]
fn imsec_matches_the_reference_cases() {
    let failures = run(IMSEC);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// IMSECH: 6 cases (2 not ported: 2 array-broadcast, 1904 date system or an error type the engine lacks)
const IMSECH: &[Case] = &[
    Case {
        id: "imsech#0",
        cells: &[],
        formula: "=IMSECH(\"5+12i\")",
        expect: Exp::Complex("0.0113717647601419+0.00723019299882505i"),
    },
    Case {
        id: "imsech#1",
        cells: &[],
        formula: "=IMSECH(25698432)",
        expect: Exp::NumText(0.0),
    },
    Case {
        id: "imsech#2",
        cells: &[],
        formula: "=IMSECH(\"1.5\")",
        expect: Exp::NumText(0.425096034942281),
    },
    Case {
        id: "imsech#3",
        cells: &[],
        formula: "=IMSECH(\"test\")",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "imsech#4",
        cells: &[],
        formula: "=IMSECH(TRUE)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "imsech#5",
        cells: &[],
        formula: "=IMSECH(#NAME?)",
        expect: Exp::Err(E::Name),
    },
];

#[test]
fn imsech_matches_the_reference_cases() {
    let failures = run(IMSECH);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// IMSINH: 6 cases (2 not ported: 2 array-broadcast, 1904 date system or an error type the engine lacks)
const IMSINH: &[Case] = &[
    Case {
        id: "imsinh#0",
        cells: &[],
        formula: "=IMSINH(\"5+12i\")",
        expect: Exp::Complex("62.6166729967278-39.8190486246075i"),
    },
    Case {
        id: "imsinh#1",
        cells: &[],
        formula: "=IMSINH(25698432)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "imsinh#2",
        cells: &[],
        formula: "=IMSINH(\"1.5\")",
        expect: Exp::NumText(2.12927945509482),
    },
    Case {
        id: "imsinh#3",
        cells: &[],
        formula: "=IMSINH(\"test\")",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "imsinh#4",
        cells: &[],
        formula: "=IMSINH(TRUE)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "imsinh#5",
        cells: &[],
        formula: "=IMSINH(#NAME?)",
        expect: Exp::Err(E::Name),
    },
];

#[test]
fn imsinh_matches_the_reference_cases() {
    let failures = run(IMSINH);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// IMTAN: 6 cases (2 not ported: 2 array-broadcast, 1904 date system or an error type the engine lacks)
const IMTAN: &[Case] = &[
    Case {
        id: "imtan#0",
        cells: &[],
        formula: "=IMTAN(\"5+12i\")",
        expect: Exp::Complex("-4.10750577733132E-11+1.00000000006335i"),
    },
    Case {
        id: "imtan#1",
        cells: &[],
        formula: "=IMTAN(25698432)",
        expect: Exp::NumText(-0.110331439533494),
    },
    Case {
        id: "imtan#2",
        cells: &[],
        formula: "=IMTAN(\"1.5\")",
        expect: Exp::NumText(14.1014199471717),
    },
    Case {
        id: "imtan#3",
        cells: &[],
        formula: "=IMTAN(\"test\")",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "imtan#4",
        cells: &[],
        formula: "=IMTAN(TRUE)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "imtan#5",
        cells: &[],
        formula: "=IMTAN(#NAME?)",
        expect: Exp::Err(E::Name),
    },
];

#[test]
fn imtan_matches_the_reference_cases() {
    let failures = run(IMTAN);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// IMTANH: 6 cases (2 not ported: 2 array-broadcast, 1904 date system or an error type the engine lacks)
const IMTANH: &[Case] = &[
    Case {
        id: "imtanh#0",
        cells: &[],
        formula: "=IMTANH(\"5+12i\")",
        expect: Exp::Complex("0.99996148196703-0.000082223221029802i"),
    },
    Case {
        id: "imtanh#1",
        cells: &[],
        formula: "=IMTANH(25698432)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "imtanh#2",
        cells: &[],
        formula: "=IMTANH(\"1.5\")",
        expect: Exp::NumText(0.905148253644866),
    },
    Case {
        id: "imtanh#3",
        cells: &[],
        formula: "=IMTANH(\"test\")",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "imtanh#4",
        cells: &[],
        formula: "=IMTANH(TRUE)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "imtanh#5",
        cells: &[],
        formula: "=IMTANH(#NAME?)",
        expect: Exp::Err(E::Name),
    },
];

#[test]
fn imtanh_matches_the_reference_cases() {
    let failures = run(IMTANH);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

//! Formula function bundle B, text and information (PD-201): `ISNONTEXT` and `ENCODEURL`.
//!
//! Generated from the reference implementation's unit tests: same data, same expected values.
//! A case marked `DIFFERS` keeps the reference value in its comment; see PD-201.

// the expected values are the reference's literals, some of them close to a named constant
#![allow(clippy::approx_constant)]

use easyoffice_sheet::{
    EvaluatedCellValue as V, FormulaError as E, PackedCellCoordinate, SheetEngine,
};

enum Exp {
    Text(&'static str),
    Bool(bool),
    Err(E),
}

struct Case {
    id: &'static str,
    cells: &'static [(&'static str, &'static str)],
    formula: &'static str,
    expect: Exp,
}

fn matches(got: &V, want: &Exp) -> bool {
    match (got, want) {
        (V::Text(g), Exp::Text(w)) => g == w,
        (V::Boolean(g), Exp::Bool(w)) => g == w,
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

// ISNONTEXT: 1 cases
const ISNONTEXT: &[Case] = &[Case {
    id: "isnontext#0",
    cells: &[],
    formula: "=ISNONTEXT(\"test\")",
    expect: Exp::Bool(false),
}];

#[test]
fn isnontext_matches_the_reference_cases() {
    let failures = run(ISNONTEXT);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// ENCODEURL: 5 cases (2 not ported: 2 array-broadcast, 1904 date system or an error type the engine lacks)
const ENCODEURL: &[Case] = &[
    Case {
        id: "encodeurl#0",
        cells: &[],
        formula: "=ENCODEURL(\"https://example.test/\")",
        expect: Exp::Text("https%3A%2F%2Fexample.test%2F"),
    },
    Case {
        id: "encodeurl#1",
        cells: &[],
        formula: "=ENCODEURL(1)",
        expect: Exp::Text("1"),
    },
    Case {
        id: "encodeurl#2",
        cells: &[],
        formula: "=ENCODEURL(ZZ999)",
        expect: Exp::Text(""),
    },
    Case {
        id: "encodeurl#3",
        cells: &[],
        formula: "=ENCODEURL(TRUE)",
        expect: Exp::Text("TRUE"),
    },
    Case {
        id: "encodeurl#4",
        cells: &[],
        formula: "=ENCODEURL(#NAME?)",
        expect: Exp::Err(E::Name),
    },
];

#[test]
fn encodeurl_matches_the_reference_cases() {
    let failures = run(ENCODEURL);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

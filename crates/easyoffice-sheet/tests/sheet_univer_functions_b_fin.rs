//! Formula function bundle B, financial (PD-201): bond, security and depreciation functions.
//!
//! Dates are whole serial numbers here (the fraction is dropped), only the 1900 date system exists, and
//! the reference cases that pass arrays to scalar parameters (element-wise broadcast) are left out.
//!
//! Generated from the reference implementation's unit tests: same data, same expected values.
//! A case marked `DIFFERS` keeps the reference value in its comment; see PD-201.

// the expected values are the reference's literals, some of them close to a named constant
#![allow(clippy::approx_constant)]

use easyoffice_sheet::{
    EvaluatedCellValue as V, FormulaError as E, PackedCellCoordinate, SheetEngine,
};

enum Exp {
    Num(f64),
    Close(f64, f64),
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
        (V::Number(g), Exp::Num(w)) => (g - w).abs() <= 1e-9 * w.abs().max(1.0),
        (V::Number(g), Exp::Close(w, tol)) => (g - w).abs() <= *tol,
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

// ACCRINT: 19 cases (2 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const ACCRINT: &[Case] = &[
    Case {
        id: "accrint#0",
        cells: &[],
        formula: "=ACCRINT(\"2008/2/29\",\"2008/8/31\",\"2008/5/1\",0.1,1000,2)",
        expect: Exp::Num(16.9444444444),
    },
    Case {
        id: "accrint#1",
        cells: &[],
        formula: "=ACCRINT(\"2008/2/29\",\"2008/8/31\",\"2008/5/1\",0.1,1000,2,1)",
        expect: Exp::Num(16.847826087),
    },
    Case {
        id: "accrint#2",
        cells: &[],
        formula: "=ACCRINT(\"2008/5/1\",\"2008/8/31\",\"2008/2/29\",0.1,1000,2)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "accrint#3",
        cells: &[],
        formula: "=ACCRINT(\"1900/1/29\",\"2008/8/31\",\"2008/5/1\",0.1,1000,2)",
        expect: Exp::Num(0.0),
    },
    Case {
        id: "accrint#4",
        cells: &[],
        formula: "=ACCRINT(\"2008/2/29\",\"2008/8/31\",\"2008/11/1\",0.1,1000,2)",
        expect: Exp::Num(66.9444444444),
    },
    Case {
        id: "accrint#5",
        cells: &[],
        formula: "=ACCRINT(\"2008/2/29\",\"2008/8/31\",\"2008/11/1\",0.1,1000,2,3)",
        expect: Exp::Num(67.9452054795),
    },
    Case {
        id: "accrint#6",
        cells: &[],
        formula: "=ACCRINT(\"2008/2/29\",\"2008/8/31\",\"2008/11/1\",0.1,1000,2,1)",
        expect: Exp::Num(68.2065217391),
    },
    Case {
        id: "accrint#7",
        cells: &[],
        formula: "=ACCRINT(1,\"2008/8/31\",#NAME?,0.1,1000,2)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "accrint#8",
        cells: &[],
        formula: "=ACCRINT(FALSE,\"2008/8/31\",43832.233,0.1,1000,2)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "accrint#9",
        cells: &[],
        formula: "=ACCRINT(FALSE,\"2008/8/31\",43832.233,0.1,1000,2,TRUE)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "accrint#10",
        cells: &[],
        formula: "=ACCRINT(\"test\",\"2008/8/31\",\"2008/5/1\",0.1,1000,2)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "accrint#11",
        cells: &[],
        formula: "=ACCRINT(\"2008/2/29\",\"test\",\"2008/5/1\",0.1,1000,2)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "accrint#12",
        cells: &[],
        formula: "=ACCRINT(\"2008/2/29\",\"2008/8/31\",\"test\",0.1,1000,2)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "accrint#13",
        cells: &[],
        formula: "=ACCRINT(\"2008/2/29\",\"2008/8/31\",\"2008/5/1\",\"test\",1000,2)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "accrint#14",
        cells: &[],
        formula: "=ACCRINT(\"2008/2/29\",\"2008/8/31\",\"2008/5/1\",0,1000,2)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "accrint#15",
        cells: &[],
        formula: "=ACCRINT(\"2008/2/29\",\"2008/8/31\",\"2008/5/1\",0.1,0,2)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "accrint#16",
        cells: &[],
        formula: "=ACCRINT(\"2008/2/29\",\"2008/8/31\",\"2008/5/1\",0.1,1000,2,-1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "accrint#17",
        cells: &[],
        formula: "=ACCRINT(0,182,183,0.1,100,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "accrint#18",
        cells: &[],
        formula: "=ACCRINT(31,366,60,0.1,100,2,1)",
        expect: Exp::Num(0.0),
    },
];

#[test]
fn accrint_matches_the_reference_cases() {
    let failures = run(ACCRINT);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// ACCRINTM: 14 cases (2 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const ACCRINTM: &[Case] = &[
    Case {
        id: "accrintm#0",
        cells: &[],
        formula: "=ACCRINTM(\"2012/2/2\",\"2021/3/11\",0.1,1000)",
        expect: Exp::Num(910.833333333333),
    },
    Case {
        id: "accrintm#1",
        cells: &[],
        formula: "=ACCRINTM(\"2012/2/2\",\"2021/3/11\",0.1,1000,1)",
        expect: Exp::Num(910.210785655626),
    },
    Case {
        id: "accrintm#2",
        cells: &[],
        formula: "=ACCRINTM(\"2021/3/11\",\"2012/2/2\",0.1,1000)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "accrintm#3",
        cells: &[],
        formula: "=ACCRINTM(\"2012/2/2\",\"2012/2/2\",0.1,1000)",
        expect: Exp::Num(0.0),
    },
    Case {
        id: "accrintm#4",
        cells: &[],
        formula: "=ACCRINTM(1,#NAME?,0.1,1000)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "accrintm#5",
        cells: &[],
        formula: "=ACCRINTM(FALSE,43832.233,0.1,1000)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "accrintm#6",
        cells: &[],
        formula: "=ACCRINTM(FALSE,43832.233,0.1,1000,TRUE)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "accrintm#7",
        cells: &[],
        formula: "=ACCRINTM(\"test\",\"2021-12-31\",0.1,1000)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "accrintm#8",
        cells: &[],
        formula: "=ACCRINTM(\"2012/2/2\",\"test\",0.1,1000)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "accrintm#9",
        cells: &[],
        formula: "=ACCRINTM(\"2012/2/2\",\"2021-12-31\",\"test\",1000)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "accrintm#10",
        cells: &[],
        formula: "=ACCRINTM(\"2012/2/2\",\"2021/3/11\",0,1000)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "accrintm#11",
        cells: &[],
        formula: "=ACCRINTM(\"2012/2/2\",\"2021/3/11\",0.1,0)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "accrintm#12",
        cells: &[],
        formula: "=ACCRINTM(\"2012/2/2\",\"2021/3/11\",0.1,1000,-1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "accrintm#13",
        cells: &[],
        formula: "=ACCRINTM(0,366,0.1,100,1)",
        expect: Exp::Close(10.027397260273972, 5e-13),
    },
];

#[test]
fn accrintm_matches_the_reference_cases() {
    let failures = run(ACCRINTM);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// AMORLINC: 19 cases (2 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const AMORLINC: &[Case] = &[
    Case {
        id: "amorlinc#0",
        cells: &[],
        formula: "=AMORLINC(2400,\"2008/8/19\",\"2008/12/31\",300,1,0.15,0)",
        expect: Exp::Num(360.0),
    },
    Case {
        id: "amorlinc#1",
        cells: &[],
        formula: "=AMORLINC(2400,\"2008/8/19\",\"2009/12/31\",2100,1,0.6,0)",
        expect: Exp::Num(0.0),
    },
    Case {
        id: "amorlinc#2",
        cells: &[],
        formula: "=AMORLINC(\"test\",\"2008/8/19\",\"2008/12/31\",300,1,0.15,0)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "amorlinc#3",
        cells: &[],
        formula: "=AMORLINC(2400,\"test\",\"2008/12/31\",300,1,0.15,0)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "amorlinc#4",
        cells: &[],
        formula: "=AMORLINC(2400,\"2008/8/19\",\"test\",300,1,0.15,0)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "amorlinc#5",
        cells: &[],
        formula: "=AMORLINC(0,\"2008/8/19\",\"2008/12/31\",300,1,0.15,0)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "amorlinc#6",
        cells: &[],
        formula: "=AMORLINC(2400,\"2008/8/19\",\"2008/12/31\",-1,1,0.15,0)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "amorlinc#7",
        cells: &[],
        formula: "=AMORLINC(100,\"2008/8/19\",\"2008/12/31\",200,1,0.15,0)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "amorlinc#8",
        cells: &[],
        formula: "=AMORLINC(2400,\"2008/12/31\",\"2008/8/19\",300,1,0.15,0)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "amorlinc#9",
        cells: &[],
        formula: "=AMORLINC(2400,\"2008/8/19\",\"2008/12/31\",300,-1,0.15,0)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "amorlinc#10",
        cells: &[],
        formula: "=AMORLINC(2400,\"2008/8/19\",\"2008/12/31\",300,0,0.15,0)",
        expect: Exp::Num(132.0),
    },
    Case {
        id: "amorlinc#11",
        cells: &[],
        formula: "=AMORLINC(2400,\"2008/8/19\",\"2008/12/31\",300,6,0.15,0)",
        expect: Exp::Num(168.0),
    },
    Case {
        id: "amorlinc#12",
        cells: &[],
        formula: "=AMORLINC(2400,\"2008/8/19\",\"2008/12/31\",300,7,0.15,0)",
        expect: Exp::Num(0.0),
    },
    Case {
        id: "amorlinc#13",
        cells: &[],
        formula: "=AMORLINC(2400,\"2008/8/19\",\"2008/12/31\",300,1,0,0)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "amorlinc#14",
        cells: &[],
        formula: "=AMORLINC(2400,\"2008/8/19\",\"2008/12/31\",300,0,0.15,2)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "amorlinc#15",
        cells: &[],
        formula: "=AMORLINC(2400,\"2008/8/19\",\"2008/12/31\",300,0,0.15,1)",
        expect: Exp::Num(131.803278688525),
    },
    Case {
        id: "amorlinc#16",
        cells: &[],
        formula: "=AMORLINC(2400,\"2008/8/19\",\"2008/12/31\",300,0,0.15,3)",
        expect: Exp::Num(132.164383561644),
    },
    Case {
        id: "amorlinc#17",
        cells: &[],
        formula: "=AMORLINC(2400,\"2008/8/19\",\"2008/12/31\",300,0,0.15,4)",
        expect: Exp::Num(131.0),
    },
    Case {
        id: "amorlinc#18",
        cells: &[],
        formula: "=AMORLINC(100,0,366,0,1,0.1,1)",
        expect: Exp::Num(10.0),
    },
];

#[test]
fn amorlinc_matches_the_reference_cases() {
    let failures = run(AMORLINC);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// COUPDAYBS: 16 cases (2 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const COUPDAYBS: &[Case] = &[
    Case {
        id: "coupdaybs#0",
        cells: &[],
        formula: "=COUPDAYBS(\"2011-1-25\",\"2011-11-15\",2,1)",
        expect: Exp::Num(71.0),
    },
    Case {
        id: "coupdaybs#1",
        cells: &[],
        formula: "=COUPDAYBS(0,365,2,1)",
        expect: Exp::Num(0.0),
    },
    Case {
        id: "coupdaybs#2",
        cells: &[],
        formula: "=COUPDAYBS(\"2012-1-25\",\"2011-11-15\",2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "coupdaybs#3",
        cells: &[],
        formula: "=COUPDAYBS(\"2011-1-25\",\"2011-11-15\",3,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "coupdaybs#4",
        cells: &[],
        formula: "=COUPDAYBS(\"2011-1-25\",\"2011-11-15\",1,1)",
        expect: Exp::Num(71.0),
    },
    Case {
        id: "coupdaybs#5",
        cells: &[],
        formula: "=COUPDAYBS(\"2011-1-25\",\"2011-11-15\",4,1)",
        expect: Exp::Num(71.0),
    },
    Case {
        id: "coupdaybs#6",
        cells: &[],
        formula: "=COUPDAYBS(\"2011-1-25\",\"2011-11-15\",2,5)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "coupdaybs#7",
        cells: &[],
        formula: "=COUPDAYBS(\"2011-1-25\",\"2011-11-15\",2,0)",
        expect: Exp::Num(70.0),
    },
    Case {
        id: "coupdaybs#8",
        cells: &[],
        formula: "=COUPDAYBS(\"2011-1-25\",\"2011-11-15\",2,2)",
        expect: Exp::Num(71.0),
    },
    Case {
        id: "coupdaybs#9",
        cells: &[],
        formula: "=COUPDAYBS(\"2011-1-25\",\"2011-11-15\",2,3)",
        expect: Exp::Num(71.0),
    },
    Case {
        id: "coupdaybs#10",
        cells: &[],
        formula: "=COUPDAYBS(\"2011-1-25\",\"2011-11-15\",2,4)",
        expect: Exp::Num(70.0),
    },
    Case {
        id: "coupdaybs#11",
        cells: &[],
        formula: "=COUPDAYBS(#NAME?,\"2011-11-15\",2,1)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "coupdaybs#12",
        cells: &[],
        formula: "=COUPDAYBS(\"2011-1-25\",\"2011-11-15\",TRUE,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "coupdaybs#13",
        cells: &[],
        formula: "=COUPDAYBS(\"test\",\"2011-11-15\",2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "coupdaybs#14",
        cells: &[],
        formula: "=COUPDAYBS(\"2011-1-25\",\"test\",2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "coupdaybs#15",
        cells: &[],
        formula: "=COUPDAYBS(\"2011-1-25\",\"2011-11-15\",\"test\",1)",
        expect: Exp::Err(E::Value),
    },
];

#[test]
fn coupdaybs_matches_the_reference_cases() {
    let failures = run(COUPDAYBS);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// COUPDAYS: 16 cases (2 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const COUPDAYS: &[Case] = &[
    Case {
        id: "coupdays#0",
        cells: &[],
        formula: "=COUPDAYS(\"2011-1-25\",\"2011-11-15\",2,1)",
        expect: Exp::Num(181.0),
    },
    Case {
        id: "coupdays#1",
        cells: &[],
        formula: "=COUPDAYS(0,365,2,1)",
        expect: Exp::Num(184.0),
    },
    Case {
        id: "coupdays#2",
        cells: &[],
        formula: "=COUPDAYS(\"2012-1-25\",\"2011-11-15\",2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "coupdays#3",
        cells: &[],
        formula: "=COUPDAYS(\"2011-1-25\",\"2011-11-15\",3,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "coupdays#4",
        cells: &[],
        formula: "=COUPDAYS(\"2011-1-25\",\"2011-11-15\",1,1)",
        expect: Exp::Num(365.0),
    },
    Case {
        id: "coupdays#5",
        cells: &[],
        formula: "=COUPDAYS(\"2011-1-25\",\"2011-11-15\",4,1)",
        expect: Exp::Num(92.0),
    },
    Case {
        id: "coupdays#6",
        cells: &[],
        formula: "=COUPDAYS(\"2011-1-25\",\"2011-11-15\",2,5)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "coupdays#7",
        cells: &[],
        formula: "=COUPDAYS(\"2011-1-25\",\"2011-11-15\",2,0)",
        expect: Exp::Num(180.0),
    },
    Case {
        id: "coupdays#8",
        cells: &[],
        formula: "=COUPDAYS(\"2011-1-25\",\"2011-11-15\",2,2)",
        expect: Exp::Num(180.0),
    },
    Case {
        id: "coupdays#9",
        cells: &[],
        formula: "=COUPDAYS(\"2011-1-25\",\"2011-11-15\",2,3)",
        expect: Exp::Num(182.5),
    },
    Case {
        id: "coupdays#10",
        cells: &[],
        formula: "=COUPDAYS(\"2011-1-25\",\"2011-11-15\",2,4)",
        expect: Exp::Num(180.0),
    },
    Case {
        id: "coupdays#11",
        cells: &[],
        formula: "=COUPDAYS(#NAME?,\"2011-11-15\",2,1)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "coupdays#12",
        cells: &[],
        formula: "=COUPDAYS(\"2011-1-25\",\"2011-11-15\",TRUE,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "coupdays#13",
        cells: &[],
        formula: "=COUPDAYS(\"test\",\"2011-11-15\",2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "coupdays#14",
        cells: &[],
        formula: "=COUPDAYS(\"2011-1-25\",\"test\",2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "coupdays#15",
        cells: &[],
        formula: "=COUPDAYS(\"2011-1-25\",\"2011-11-15\",\"test\",1)",
        expect: Exp::Err(E::Value),
    },
];

#[test]
fn coupdays_matches_the_reference_cases() {
    let failures = run(COUPDAYS);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// COUPDAYSNC: 17 cases (2 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const COUPDAYSNC: &[Case] = &[
    Case {
        id: "coupdaysnc#0",
        cells: &[],
        formula: "=COUPDAYSNC(\"2011-1-25\",\"2011-11-15\",2,1)",
        expect: Exp::Num(110.0),
    },
    Case {
        id: "coupdaysnc#1",
        cells: &[],
        formula: "=COUPDAYSNC(0,365,2,1)",
        expect: Exp::Num(182.0),
    },
    Case {
        id: "coupdaysnc#2",
        cells: &[],
        formula: "=COUPDAYSNC(\"2012-1-25\",\"2011-11-15\",2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "coupdaysnc#3",
        cells: &[],
        formula: "=COUPDAYSNC(\"2011-11-25\",\"2014-3-15\",2,1)",
        expect: Exp::Num(111.0),
    },
    Case {
        id: "coupdaysnc#4",
        cells: &[],
        formula: "=COUPDAYSNC(\"2011-1-25\",\"2011-11-15\",3,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "coupdaysnc#5",
        cells: &[],
        formula: "=COUPDAYSNC(\"2011-1-25\",\"2011-11-15\",1,1)",
        expect: Exp::Num(294.0),
    },
    Case {
        id: "coupdaysnc#6",
        cells: &[],
        formula: "=COUPDAYSNC(\"2011-1-25\",\"2011-11-15\",4,1)",
        expect: Exp::Num(21.0),
    },
    Case {
        id: "coupdaysnc#7",
        cells: &[],
        formula: "=COUPDAYSNC(\"2011-1-25\",\"2011-11-15\",2,5)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "coupdaysnc#8",
        cells: &[],
        formula: "=COUPDAYSNC(\"2011-1-25\",\"2011-11-15\",2,0)",
        expect: Exp::Num(110.0),
    },
    Case {
        id: "coupdaysnc#9",
        cells: &[],
        formula: "=COUPDAYSNC(\"2011-1-25\",\"2011-11-15\",2,2)",
        expect: Exp::Num(110.0),
    },
    Case {
        id: "coupdaysnc#10",
        cells: &[],
        formula: "=COUPDAYSNC(\"2011-1-25\",\"2011-11-15\",2,3)",
        expect: Exp::Num(110.0),
    },
    Case {
        id: "coupdaysnc#11",
        cells: &[],
        formula: "=COUPDAYSNC(\"2011-1-25\",\"2011-11-15\",2,4)",
        expect: Exp::Num(110.0),
    },
    Case {
        id: "coupdaysnc#12",
        cells: &[],
        formula: "=COUPDAYSNC(#NAME?,\"2011-11-15\",2,1)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "coupdaysnc#13",
        cells: &[],
        formula: "=COUPDAYSNC(\"2011-1-25\",\"2011-11-15\",TRUE,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "coupdaysnc#14",
        cells: &[],
        formula: "=COUPDAYSNC(\"test\",\"2011-11-15\",2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "coupdaysnc#15",
        cells: &[],
        formula: "=COUPDAYSNC(\"2011-1-25\",\"test\",2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "coupdaysnc#16",
        cells: &[],
        formula: "=COUPDAYSNC(\"2011-1-25\",\"2011-11-15\",\"test\",1)",
        expect: Exp::Err(E::Value),
    },
];

#[test]
fn coupdaysnc_matches_the_reference_cases() {
    let failures = run(COUPDAYSNC);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// COUPNCD: 16 cases (2 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const COUPNCD: &[Case] = &[
    Case {
        id: "coupncd#0",
        cells: &[],
        formula: "=COUPNCD(\"2011-1-25\",\"2011-11-15\",2,1)",
        expect: Exp::Num(40678.0),
    },
    Case {
        id: "coupncd#1",
        cells: &[],
        formula: "=COUPNCD(0,365,2,1)",
        expect: Exp::Num(182.0),
    },
    Case {
        id: "coupncd#2",
        cells: &[],
        formula: "=COUPNCD(\"2012-1-25\",\"2011-11-15\",2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "coupncd#3",
        cells: &[],
        formula: "=COUPNCD(\"2011-1-25\",\"2011-11-15\",3,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "coupncd#4",
        cells: &[],
        formula: "=COUPNCD(\"2011-1-25\",\"2011-11-15\",1,1)",
        expect: Exp::Num(40862.0),
    },
    Case {
        id: "coupncd#5",
        cells: &[],
        formula: "=COUPNCD(\"2011-1-25\",\"2011-11-15\",4,1)",
        expect: Exp::Num(40589.0),
    },
    Case {
        id: "coupncd#6",
        cells: &[],
        formula: "=COUPNCD(\"2011-1-25\",\"2011-11-15\",2,5)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "coupncd#7",
        cells: &[],
        formula: "=COUPNCD(\"2011-1-25\",\"2011-11-15\",2,0)",
        expect: Exp::Num(40678.0),
    },
    Case {
        id: "coupncd#8",
        cells: &[],
        formula: "=COUPNCD(\"2011-1-25\",\"2011-11-15\",2,2)",
        expect: Exp::Num(40678.0),
    },
    Case {
        id: "coupncd#9",
        cells: &[],
        formula: "=COUPNCD(\"2011-1-25\",\"2011-11-15\",2,3)",
        expect: Exp::Num(40678.0),
    },
    Case {
        id: "coupncd#10",
        cells: &[],
        formula: "=COUPNCD(\"2011-1-25\",\"2011-11-15\",2,4)",
        expect: Exp::Num(40678.0),
    },
    Case {
        id: "coupncd#11",
        cells: &[],
        formula: "=COUPNCD(#NAME?,\"2011-11-15\",2,1)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "coupncd#12",
        cells: &[],
        formula: "=COUPNCD(\"2011-1-25\",\"2011-11-15\",TRUE,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "coupncd#13",
        cells: &[],
        formula: "=COUPNCD(\"test\",\"2011-11-15\",2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "coupncd#14",
        cells: &[],
        formula: "=COUPNCD(\"2011-1-25\",\"test\",2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "coupncd#15",
        cells: &[],
        formula: "=COUPNCD(\"2011-1-25\",\"2011-11-15\",\"test\",1)",
        expect: Exp::Err(E::Value),
    },
];

#[test]
fn coupncd_matches_the_reference_cases() {
    let failures = run(COUPNCD);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// COUPNUM: 18 cases (3 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const COUPNUM: &[Case] = &[
    Case {
        id: "coupnum#0",
        cells: &[],
        formula: "=COUPNUM(\"2011-1-25\",\"2011-11-15\",2,1)",
        expect: Exp::Num(2.0),
    },
    Case {
        id: "coupnum#1",
        cells: &[],
        formula: "=COUPNUM(365,730,2,1)",
        expect: Exp::Num(2.0),
    },
    Case {
        id: "coupnum#2",
        cells: &[],
        formula: "=COUPNUM(0,365,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "coupnum#3",
        cells: &[],
        formula: "=COUPNUM(\"2012-1-25\",\"2011-11-15\",2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "coupnum#4",
        cells: &[],
        formula: "=COUPNUM(1,\"2011-11-15\",2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "coupnum#5",
        cells: &[],
        formula: "=COUPNUM(\"2011-1-25\",\"2011-11-15\",3,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "coupnum#6",
        cells: &[],
        formula: "=COUPNUM(\"2011-1-25\",\"2011-11-15\",1,1)",
        expect: Exp::Num(1.0),
    },
    Case {
        id: "coupnum#7",
        cells: &[],
        formula: "=COUPNUM(\"2011-1-25\",\"2011-11-15\",4,1)",
        expect: Exp::Num(4.0),
    },
    Case {
        id: "coupnum#8",
        cells: &[],
        formula: "=COUPNUM(\"2011-1-25\",\"2011-11-15\",2,5)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "coupnum#9",
        cells: &[],
        formula: "=COUPNUM(\"2011-1-25\",\"2011-11-15\",2,0)",
        expect: Exp::Num(2.0),
    },
    Case {
        id: "coupnum#10",
        cells: &[],
        formula: "=COUPNUM(\"2011-1-25\",\"2011-11-15\",2,2)",
        expect: Exp::Num(2.0),
    },
    Case {
        id: "coupnum#11",
        cells: &[],
        formula: "=COUPNUM(\"2011-1-25\",\"2011-11-15\",2,3)",
        expect: Exp::Num(2.0),
    },
    Case {
        id: "coupnum#12",
        cells: &[],
        formula: "=COUPNUM(\"2011-1-25\",\"2011-11-15\",2,4)",
        expect: Exp::Num(2.0),
    },
    Case {
        id: "coupnum#13",
        cells: &[],
        formula: "=COUPNUM(#NAME?,\"2011-11-15\",2,1)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "coupnum#14",
        cells: &[],
        formula: "=COUPNUM(\"2011-1-25\",\"2011-11-15\",TRUE,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "coupnum#15",
        cells: &[],
        formula: "=COUPNUM(\"test\",\"2011-11-15\",2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "coupnum#16",
        cells: &[],
        formula: "=COUPNUM(\"2011-1-25\",\"test\",2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "coupnum#17",
        cells: &[],
        formula: "=COUPNUM(\"2011-1-25\",\"2011-11-15\",\"test\",1)",
        expect: Exp::Err(E::Value),
    },
];

#[test]
fn coupnum_matches_the_reference_cases() {
    let failures = run(COUPNUM);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// COUPPCD: 17 cases (4 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const COUPPCD: &[Case] = &[
    Case {
        id: "couppcd#0",
        cells: &[],
        formula: "=COUPPCD(\"2011-1-25\",\"2011-11-15\",2,1)",
        expect: Exp::Num(40497.0),
    },
    Case {
        id: "couppcd#1",
        cells: &[],
        formula: "=COUPPCD(0,365,2,1)",
        expect: Exp::Num(0.0),
    },
    Case {
        id: "couppcd#2",
        cells: &[],
        formula: "=COUPPCD(\"2012-1-25\",\"2011-11-15\",2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "couppcd#3",
        cells: &[],
        formula: "=COUPPCD(1,\"2011-11-15\",2,1)",
        expect: Exp::Num(0.0),
    },
    Case {
        id: "couppcd#4",
        cells: &[],
        formula: "=COUPPCD(\"2011-1-25\",\"2011-11-15\",3,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "couppcd#5",
        cells: &[],
        formula: "=COUPPCD(\"2011-1-25\",\"2011-11-15\",1,1)",
        expect: Exp::Num(40497.0),
    },
    Case {
        id: "couppcd#6",
        cells: &[],
        formula: "=COUPPCD(\"2011-1-25\",\"2011-11-15\",4,1)",
        expect: Exp::Num(40497.0),
    },
    Case {
        id: "couppcd#7",
        cells: &[],
        formula: "=COUPPCD(\"2011-1-25\",\"2011-11-15\",2,5)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "couppcd#8",
        cells: &[],
        formula: "=COUPPCD(\"2011-1-25\",\"2011-11-15\",2,0)",
        expect: Exp::Num(40497.0),
    },
    Case {
        id: "couppcd#9",
        cells: &[],
        formula: "=COUPPCD(\"2011-1-25\",\"2011-11-15\",2,2)",
        expect: Exp::Num(40497.0),
    },
    Case {
        id: "couppcd#10",
        cells: &[],
        formula: "=COUPPCD(\"2011-1-25\",\"2011-11-15\",2,3)",
        expect: Exp::Num(40497.0),
    },
    Case {
        id: "couppcd#11",
        cells: &[],
        formula: "=COUPPCD(\"2011-1-25\",\"2011-11-15\",2,4)",
        expect: Exp::Num(40497.0),
    },
    Case {
        id: "couppcd#12",
        cells: &[],
        formula: "=COUPPCD(#NAME?,\"2011-11-15\",2,1)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "couppcd#13",
        cells: &[],
        formula: "=COUPPCD(\"2011-1-25\",\"2011-11-15\",TRUE,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "couppcd#14",
        cells: &[],
        formula: "=COUPPCD(\"test\",\"2011-11-15\",2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "couppcd#15",
        cells: &[],
        formula: "=COUPPCD(\"2011-1-25\",\"test\",2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "couppcd#16",
        cells: &[],
        formula: "=COUPPCD(\"2011-1-25\",\"2011-11-15\",\"test\",1)",
        expect: Exp::Err(E::Value),
    },
];

#[test]
fn couppcd_matches_the_reference_cases() {
    let failures = run(COUPPCD);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// DISC: 13 cases (2 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const DISC: &[Case] = &[
    Case {
        id: "disc#0",
        cells: &[],
        formula: "=DISC(\"2018-7-1\",\"2048-1-1\",97.975,100,1)",
        expect: Exp::Num(0.000686384169121),
    },
    Case {
        id: "disc#1",
        cells: &[],
        formula: "=DISC(\"2048-1-1\",\"2018-7-1\",97.975,100,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "disc#2",
        cells: &[],
        formula: "=DISC(\"2018-7-1\",\"2048-1-1\",97.975,100,5)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "disc#3",
        cells: &[],
        formula: "=DISC(\"2018-7-1\",\"2048-1-1\",97.975,100,0)",
        expect: Exp::Num(0.000686440677966),
    },
    Case {
        id: "disc#4",
        cells: &[],
        formula: "=DISC(\"2018-7-1\",\"2048-1-1\",97.975,100,2)",
        expect: Exp::Num(0.000676503340757),
    },
    Case {
        id: "disc#5",
        cells: &[],
        formula: "=DISC(\"2018-7-1\",\"2048-1-1\",97.975,100,3)",
        expect: Exp::Num(0.00068589922049),
    },
    Case {
        id: "disc#6",
        cells: &[],
        formula: "=DISC(\"2018-7-1\",\"2048-1-1\",97.975,100,4)",
        expect: Exp::Num(0.000686440677966),
    },
    Case {
        id: "disc#7",
        cells: &[],
        formula: "=DISC(#NAME?,\"2048-1-1\",97.975,100,1)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "disc#8",
        cells: &[],
        formula: "=DISC(\"2018-7-1\",\"2048-1-1\",TRUE,100,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "disc#9",
        cells: &[],
        formula: "=DISC(\"test\",\"2048-1-1\",97.975,100,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "disc#10",
        cells: &[],
        formula: "=DISC(\"2018-7-1\",\"test\",97.975,100,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "disc#11",
        cells: &[],
        formula: "=DISC(\"2018-7-1\",\"2048-1-1\",97.975,100,\"test\")",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "disc#12",
        cells: &[],
        formula: "=DISC(0,366,0.1,100,1)",
        expect: Exp::Close(0.9962704918032788, 5e-13),
    },
];

#[test]
fn disc_matches_the_reference_cases() {
    let failures = run(DISC);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// DURATION: 21 cases (3 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const DURATION: &[Case] = &[
    Case {
        id: "duration#0",
        cells: &[],
        formula: "=DURATION(\"2018-7-1\",\"2048-1-1\",0.08,0.09,2,1)",
        expect: Exp::Num(10.9191452816),
    },
    Case {
        id: "duration#1",
        cells: &[],
        formula: "=DURATION(\"2048-1-1\",\"2018-7-1\",0.08,0.09,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "duration#2",
        cells: &[],
        formula: "=DURATION(0,\"2048-1-1\",0.08,0.09,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "duration#3",
        cells: &[],
        formula: "=DURATION(1,182,0.08,0.09,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "duration#4",
        cells: &[],
        formula: "=DURATION(\"2018-7-1\",\"2048-1-1\",-0.1,0.09,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "duration#5",
        cells: &[],
        formula: "=DURATION(\"2018-7-1\",\"2048-1-1\",0.08,-0.09,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "duration#6",
        cells: &[],
        formula: "=DURATION(\"2018-7-1\",\"2048-1-1\",0.08,0.09,3,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "duration#7",
        cells: &[],
        formula: "=DURATION(\"2018-7-1\",\"2048-1-1\",0.08,0.09,1,1)",
        expect: Exp::Num(10.8778775299),
    },
    Case {
        id: "duration#8",
        cells: &[],
        formula: "=DURATION(\"2018-7-1\",\"2048-1-1\",0.08,0.09,4,1)",
        expect: Exp::Num(10.7086433694),
    },
    Case {
        id: "duration#9",
        cells: &[],
        formula: "=DURATION(\"2018-7-1\",\"2048-1-1\",0.08,0.09,2,5)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "duration#10",
        cells: &[],
        formula: "=DURATION(\"2018-7-1\",\"2048-1-1\",0.08,0.09,2,0)",
        expect: Exp::Num(10.9191452816),
    },
    Case {
        id: "duration#11",
        cells: &[],
        formula: "=DURATION(\"2018-7-1\",\"2048-1-1\",0.08,0.09,2,2)",
        expect: Exp::Num(10.9191452816),
    },
    Case {
        id: "duration#12",
        cells: &[],
        formula: "=DURATION(\"2018-7-1\",\"2048-1-1\",0.08,0.09,2,3)",
        expect: Exp::Num(10.9191452816),
    },
    Case {
        id: "duration#13",
        cells: &[],
        formula: "=DURATION(\"2018-7-1\",\"2048-1-1\",0.08,0.09,2,4)",
        expect: Exp::Num(10.9191452816),
    },
    Case {
        id: "duration#14",
        cells: &[],
        formula: "=DURATION(\"2018-7-1\",\"2048-1-1\",0.08,0.09,2,ZZ999)",
        expect: Exp::Num(10.9191452816),
    },
    Case {
        id: "duration#15",
        cells: &[],
        formula: "=DURATION(#NAME?,\"2048-1-1\",0.08,0.09,2,1)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "duration#16",
        cells: &[],
        formula: "=DURATION(\"2018-7-1\",\"2048-1-1\",0.08,0.09,TRUE,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "duration#17",
        cells: &[],
        formula: "=DURATION(\"test\",\"2048-1-1\",0.08,0.09,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "duration#18",
        cells: &[],
        formula: "=DURATION(\"2018-7-1\",\"test\",0.08,0.09,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "duration#19",
        cells: &[],
        formula: "=DURATION(\"2018-7-1\",\"2048-1-1\",\"test\",0.09,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "duration#20",
        cells: &[],
        formula: "=DURATION(0,366,0.08,0.09,2,1)",
        expect: Exp::Err(E::Num),
    },
];

#[test]
fn duration_matches_the_reference_cases() {
    let failures = run(DURATION);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// ISPMT: 6 cases (1 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const ISPMT: &[Case] = &[
    Case {
        id: "ispmt#0",
        cells: &[],
        formula: "=ISPMT(0.008333333333333333,1,36,80000)",
        expect: Exp::Num(-648.148148148148),
    },
    Case {
        id: "ispmt#1",
        cells: &[],
        formula: "=ISPMT(0.008333333333333333,1,0,80000)",
        expect: Exp::Err(E::DivByZero),
    },
    Case {
        id: "ispmt#2",
        cells: &[],
        formula: "=ISPMT(#NAME?,1,36,80000)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "ispmt#3",
        cells: &[],
        formula: "=ISPMT(TRUE,1,36,80000)",
        expect: Exp::Num(-77777.7777777778),
    },
    Case {
        id: "ispmt#4",
        cells: &[],
        formula: "=ISPMT(ZZ999,1,36,80000)",
        expect: Exp::Num(0.0),
    },
    Case {
        id: "ispmt#5",
        cells: &[],
        formula: "=ISPMT(\"test\",1,36,80000)",
        expect: Exp::Err(E::Value),
    },
];

#[test]
fn ispmt_matches_the_reference_cases() {
    let failures = run(ISPMT);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// MDURATION: 19 cases (2 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const MDURATION: &[Case] = &[
    Case {
        id: "mduration#0",
        cells: &[],
        formula: "=MDURATION(\"2018-7-1\",\"2048-1-1\",0.08,0.09,2,1)",
        expect: Exp::Num(10.4489428532),
    },
    Case {
        id: "mduration#1",
        cells: &[],
        formula: "=MDURATION(\"2048-1-1\",\"2018-7-1\",0.08,0.09,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "mduration#2",
        cells: &[],
        formula: "=MDURATION(\"2018-7-1\",\"2048-1-1\",-0.1,0.09,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "mduration#3",
        cells: &[],
        formula: "=MDURATION(\"2018-7-1\",\"2048-1-1\",0.08,-0.09,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "mduration#4",
        cells: &[],
        formula: "=MDURATION(\"2018-7-1\",\"2048-1-1\",0.08,0.09,3,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "mduration#5",
        cells: &[],
        formula: "=MDURATION(\"2018-7-1\",\"2048-1-1\",0.08,0.09,1,1)",
        expect: Exp::Num(9.97970415591),
    },
    Case {
        id: "mduration#6",
        cells: &[],
        formula: "=MDURATION(\"2018-7-1\",\"2048-1-1\",0.08,0.09,4,1)",
        expect: Exp::Num(10.4730008502),
    },
    Case {
        id: "mduration#7",
        cells: &[],
        formula: "=MDURATION(\"2018-7-1\",\"2048-1-1\",0.08,0.09,2,5)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "mduration#8",
        cells: &[],
        formula: "=MDURATION(\"2018-7-1\",\"2048-1-1\",0.08,0.09,2,0)",
        expect: Exp::Num(10.4489428532),
    },
    Case {
        id: "mduration#9",
        cells: &[],
        formula: "=MDURATION(\"2018-7-1\",\"2048-1-1\",0.08,0.09,2,2)",
        expect: Exp::Num(10.4489428532),
    },
    Case {
        id: "mduration#10",
        cells: &[],
        formula: "=MDURATION(\"2018-7-1\",\"2048-1-1\",0.08,0.09,2,3)",
        expect: Exp::Num(10.4489428532),
    },
    Case {
        id: "mduration#11",
        cells: &[],
        formula: "=MDURATION(\"2018-7-1\",\"2048-1-1\",0.08,0.09,2,4)",
        expect: Exp::Num(10.4489428532),
    },
    Case {
        id: "mduration#12",
        cells: &[],
        formula: "=MDURATION(\"2018-7-1\",\"2048-1-1\",0.08,0.09,2,ZZ999)",
        expect: Exp::Num(10.4489428532),
    },
    Case {
        id: "mduration#13",
        cells: &[],
        formula: "=MDURATION(#NAME?,\"2048-1-1\",0.08,0.09,2,1)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "mduration#14",
        cells: &[],
        formula: "=MDURATION(\"2018-7-1\",\"2048-1-1\",0.08,0.09,TRUE,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "mduration#15",
        cells: &[],
        formula: "=MDURATION(\"test\",\"2048-1-1\",0.08,0.09,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "mduration#16",
        cells: &[],
        formula: "=MDURATION(\"2018-7-1\",\"test\",0.08,0.09,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "mduration#17",
        cells: &[],
        formula: "=MDURATION(\"2018-7-1\",\"2048-1-1\",\"test\",0.09,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "mduration#18",
        cells: &[],
        formula: "=MDURATION(0,366,0.08,0.09,2,1)",
        expect: Exp::Err(E::Num),
    },
];

#[test]
fn mduration_matches_the_reference_cases() {
    let failures = run(MDURATION);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// ODDFPRICE: 22 cases (2 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const ODDFPRICE: &[Case] = &[
    Case {
        id: "oddfprice#0",
        cells: &[],
        formula: "=ODDFPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,0.0625,100,2,1)",
        expect: Exp::Num(113.597717474),
    },
    Case {
        id: "oddfprice#1",
        cells: &[],
        formula: "=ODDFPRICE(31,366,0,0.08,0.09,100,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "oddfprice#2",
        cells: &[],
        formula: "=ODDFPRICE(\"2018-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,0.0625,100,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "oddfprice#3",
        cells: &[],
        formula: "=ODDFPRICE(\"2018-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-2\",0.0785,0.0625,100,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "oddfprice#4",
        cells: &[],
        formula: "=ODDFPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",-0.0785,0.0625,100,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "oddfprice#5",
        cells: &[],
        formula: "=ODDFPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,-0.0625,100,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "oddfprice#6",
        cells: &[],
        formula: "=ODDFPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,0.0625,100,3,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "oddfprice#7",
        cells: &[],
        formula: "=ODDFPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,0.0625,100,1,1)",
        expect: Exp::Num(113.494585545507),
    },
    Case {
        id: "oddfprice#8",
        cells: &[],
        formula: "=ODDFPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,0.0625,100,4,1)",
        expect: Exp::Num(113.650021611),
    },
    Case {
        id: "oddfprice#9",
        cells: &[],
        formula: "=ODDFPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,0.0625,100,2,5)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "oddfprice#10",
        cells: &[],
        formula: "=ODDFPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,0.0625,100,2,0)",
        expect: Exp::Num(113.599205828238),
    },
    Case {
        id: "oddfprice#11",
        cells: &[],
        formula: "=ODDFPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,0.0625,100,2,2)",
        expect: Exp::Num(113.598799608325),
    },
    Case {
        id: "oddfprice#12",
        cells: &[],
        formula: "=ODDFPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,0.0625,100,2,3)",
        expect: Exp::Num(113.596112595205),
    },
    Case {
        id: "oddfprice#13",
        cells: &[],
        formula: "=ODDFPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,0.0625,100,2,4)",
        expect: Exp::Num(113.599205828238),
    },
    Case {
        id: "oddfprice#14",
        cells: &[],
        formula: "=ODDFPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,0.0625,100,2,ZZ999)",
        expect: Exp::Num(113.599205828238),
    },
    Case {
        id: "oddfprice#15",
        cells: &[],
        formula: "=ODDFPRICE(#NAME?,\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,0.0625,100,2,1)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "oddfprice#16",
        cells: &[],
        formula: "=ODDFPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,0.0625,100,TRUE,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "oddfprice#17",
        cells: &[],
        formula: "=ODDFPRICE(\"test\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,0.0625,100,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "oddfprice#18",
        cells: &[],
        formula: "=ODDFPRICE(\"2008-11-11\",\"test\",\"2008-10-15\",\"2009-3-1\",0.0785,0.0625,100,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "oddfprice#19",
        cells: &[],
        formula: "=ODDFPRICE(\"2008-11-11\",\"2021-3-1\",\"test\",\"2009-3-1\",0.0785,0.0625,100,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "oddfprice#20",
        cells: &[],
        formula: "=ODDFPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"test\",0.0785,0.0625,100,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "oddfprice#21",
        cells: &[],
        formula: "=ODDFPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",\"test\",0.0625,100,2,1)",
        expect: Exp::Err(E::Value),
    },
];

#[test]
fn oddfprice_matches_the_reference_cases() {
    let failures = run(ODDFPRICE);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// ODDFYIELD: 21 cases (1 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const ODDFYIELD: &[Case] = &[
    Case {
        id: "oddfyield#0",
        cells: &[],
        formula: "=ODDFYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,84.5,100,2,1)",
        expect: Exp::Num(0.100766449804),
    },
    Case {
        id: "oddfyield#1",
        cells: &[],
        formula: "=ODDFYIELD(\"2018-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,84.5,100,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "oddfyield#2",
        cells: &[],
        formula: "=ODDFYIELD(\"2018-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-2\",0.0785,84.5,100,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "oddfyield#3",
        cells: &[],
        formula: "=ODDFYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",-0.0785,84.5,100,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "oddfyield#4",
        cells: &[],
        formula: "=ODDFYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,-84.5,100,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "oddfyield#5",
        cells: &[],
        formula: "=ODDFYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,84.5,100,3,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "oddfyield#6",
        cells: &[],
        formula: "=ODDFYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,84.5,100,1,1)",
        expect: Exp::Num(0.101169886094),
    },
    Case {
        id: "oddfyield#7",
        cells: &[],
        formula: "=ODDFYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,84.5,100,4,1)",
        expect: Exp::Num(0.100563371156),
    },
    Case {
        id: "oddfyield#8",
        cells: &[],
        formula: "=ODDFYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,84.5,100,2,5)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "oddfyield#9",
        cells: &[],
        formula: "=ODDFYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,84.5,100,2,0)",
        expect: Exp::Num(0.100765302483),
    },
    Case {
        id: "oddfyield#10",
        cells: &[],
        formula: "=ODDFYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,84.5,100,2,2)",
        expect: Exp::Num(0.100764261201),
    },
    Case {
        id: "oddfyield#11",
        cells: &[],
        formula: "=ODDFYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,84.5,100,2,3)",
        expect: Exp::Num(0.100769679523),
    },
    Case {
        id: "oddfyield#12",
        cells: &[],
        formula: "=ODDFYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,84.5,100,2,4)",
        expect: Exp::Num(0.100765302483),
    },
    Case {
        id: "oddfyield#13",
        cells: &[],
        formula: "=ODDFYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,84.5,100,2,ZZ999)",
        expect: Exp::Num(0.100765302483),
    },
    Case {
        id: "oddfyield#14",
        cells: &[],
        formula: "=ODDFYIELD(#NAME?,\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,84.5,100,2,1)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "oddfyield#15",
        cells: &[],
        formula: "=ODDFYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,84.5,100,TRUE,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "oddfyield#16",
        cells: &[],
        formula: "=ODDFYIELD(\"test\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",0.0785,84.5,100,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "oddfyield#17",
        cells: &[],
        formula: "=ODDFYIELD(\"2008-11-11\",\"test\",\"2008-10-15\",\"2009-3-1\",0.0785,84.5,100,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "oddfyield#18",
        cells: &[],
        formula: "=ODDFYIELD(\"2008-11-11\",\"2021-3-1\",\"test\",\"2009-3-1\",0.0785,84.5,100,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "oddfyield#19",
        cells: &[],
        formula: "=ODDFYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"test\",0.0785,84.5,100,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "oddfyield#20",
        cells: &[],
        formula: "=ODDFYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"2009-3-1\",\"test\",84.5,100,2,1)",
        expect: Exp::Err(E::Value),
    },
];

#[test]
fn oddfyield_matches_the_reference_cases() {
    let failures = run(ODDFYIELD);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// ODDLPRICE: 21 cases (2 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const ODDLPRICE: &[Case] = &[
    Case {
        id: "oddlprice#0",
        cells: &[],
        formula: "=ODDLPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,0.0625,100,2,1)",
        expect: Exp::Num(110.8745242842),
    },
    Case {
        id: "oddlprice#1",
        cells: &[],
        formula: "=ODDLPRICE(\"2008-11-11\",\"2021-3-1\",\"2009-10-15\",0.0785,0.0625,100,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "oddlprice#2",
        cells: &[],
        formula: "=ODDLPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",-0.0785,0.0625,100,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "oddlprice#3",
        cells: &[],
        formula: "=ODDLPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,-0.0625,100,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "oddlprice#4",
        cells: &[],
        formula: "=ODDLPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,0.0625,100,3,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "oddlprice#5",
        cells: &[],
        formula: "=ODDLPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,0.0625,100,1,1)",
        expect: Exp::Num(110.87480393587),
    },
    Case {
        id: "oddlprice#6",
        cells: &[],
        formula: "=ODDLPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,0.0625,100,4,1)",
        expect: Exp::Num(110.876991784),
    },
    Case {
        id: "oddlprice#7",
        cells: &[],
        formula: "=ODDLPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,0.0625,100,2,5)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "oddlprice#8",
        cells: &[],
        formula: "=ODDLPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,0.0625,100,2,0)",
        expect: Exp::Num(110.882869098244),
    },
    Case {
        id: "oddlprice#9",
        cells: &[],
        formula: "=ODDLPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,0.0625,100,2,2)",
        expect: Exp::Num(110.876163657771),
    },
    Case {
        id: "oddlprice#10",
        cells: &[],
        formula: "=ODDLPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,0.0625,100,2,3)",
        expect: Exp::Num(110.87411927),
    },
    Case {
        id: "oddlprice#11",
        cells: &[],
        formula: "=ODDLPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,0.0625,100,2,4)",
        expect: Exp::Num(110.882869098244),
    },
    Case {
        id: "oddlprice#12",
        cells: &[],
        formula: "=ODDLPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,0.0625,100,2,ZZ999)",
        expect: Exp::Num(110.882869098244),
    },
    Case {
        id: "oddlprice#13",
        cells: &[],
        formula: "=ODDLPRICE(#NAME?,\"2021-3-1\",\"2008-10-15\",0.0785,0.0625,100,2,1)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "oddlprice#14",
        cells: &[],
        formula: "=ODDLPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,0.0625,100,TRUE,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "oddlprice#15",
        cells: &[],
        formula: "=ODDLPRICE(\"test\",\"2021-3-1\",\"2008-10-15\",0.0785,0.0625,100,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "oddlprice#16",
        cells: &[],
        formula: "=ODDLPRICE(\"2008-11-11\",\"test\",\"2008-10-15\",0.0785,0.0625,100,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "oddlprice#17",
        cells: &[],
        formula: "=ODDLPRICE(\"2008-11-11\",\"2021-3-1\",\"test\",0.0785,0.0625,100,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "oddlprice#18",
        cells: &[],
        formula: "=ODDLPRICE(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"test\",0.0625,100,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "oddlprice#19",
        cells: &[],
        formula: "=ODDLPRICE(31,366,0,0.08,0.09,100,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "oddlprice#20",
        cells: &[],
        formula: "=ODDLPRICE(60,366,31,0.08,0.09,100,2,1)",
        expect: Exp::Err(E::Num),
    },
];

#[test]
fn oddlprice_matches_the_reference_cases() {
    let failures = run(ODDLPRICE);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// ODDLYIELD: 21 cases (2 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const ODDLYIELD: &[Case] = &[
    Case {
        id: "oddlyield#0",
        cells: &[],
        formula: "=ODDLYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,84.5,100,2,1)",
        expect: Exp::Num(0.107072088907),
    },
    Case {
        id: "oddlyield#1",
        cells: &[],
        formula: "=ODDLYIELD(\"2008-11-11\",\"2021-3-1\",\"2018-10-15\",0.0785,84.5,100,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "oddlyield#2",
        cells: &[],
        formula: "=ODDLYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",-0.0785,84.5,100,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "oddlyield#3",
        cells: &[],
        formula: "=ODDLYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,-84.5,100,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "oddlyield#4",
        cells: &[],
        formula: "=ODDLYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,84.5,100,3,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "oddlyield#5",
        cells: &[],
        formula: "=ODDLYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,84.5,100,1,1)",
        expect: Exp::Num(0.107075093237),
    },
    Case {
        id: "oddlyield#6",
        cells: &[],
        formula: "=ODDLYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,84.5,100,4,1)",
        expect: Exp::Num(0.107080737455),
    },
    Case {
        id: "oddlyield#7",
        cells: &[],
        formula: "=ODDLYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,84.5,100,2,5)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "oddlyield#8",
        cells: &[],
        formula: "=ODDLYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,84.5,100,2,0)",
        expect: Exp::Num(0.107087350309),
    },
    Case {
        id: "oddlyield#9",
        cells: &[],
        formula: "=ODDLYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,84.5,100,2,2)",
        expect: Exp::Num(0.107053227022),
    },
    Case {
        id: "oddlyield#10",
        cells: &[],
        formula: "=ODDLYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,84.5,100,2,3)",
        expect: Exp::Num(0.107076742612),
    },
    Case {
        id: "oddlyield#11",
        cells: &[],
        formula: "=ODDLYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,84.5,100,2,4)",
        expect: Exp::Num(0.107087350309),
    },
    Case {
        id: "oddlyield#12",
        cells: &[],
        formula: "=ODDLYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,84.5,100,2,ZZ999)",
        expect: Exp::Num(0.107087350309),
    },
    Case {
        id: "oddlyield#13",
        cells: &[],
        formula: "=ODDLYIELD(#NAME?,\"2021-3-1\",\"2008-10-15\",0.0785,84.5,100,2,1)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "oddlyield#14",
        cells: &[],
        formula: "=ODDLYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,84.5,100,TRUE,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "oddlyield#15",
        cells: &[],
        formula: "=ODDLYIELD(\"test\",\"2021-3-1\",\"2008-10-15\",0.0785,84.5,100,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "oddlyield#16",
        cells: &[],
        formula: "=ODDLYIELD(\"2008-11-11\",\"test\",\"2008-10-15\",0.0785,84.5,100,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "oddlyield#17",
        cells: &[],
        formula: "=ODDLYIELD(\"2008-11-11\",\"2021-3-1\",\"test\",0.0785,84.5,100,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "oddlyield#18",
        cells: &[],
        formula: "=ODDLYIELD(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"test\",84.5,100,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "oddlyield#19",
        cells: &[],
        formula: "=ODDLYIELD(31,366,0,0.08,84.5,100,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "oddlyield#20",
        cells: &[],
        formula: "=ODDLYIELD(60,366,31,0.08,90,100,2,1)",
        expect: Exp::Err(E::Num),
    },
];

#[test]
fn oddlyield_matches_the_reference_cases() {
    let failures = run(ODDLYIELD);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// PRICE: 19 cases (2 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const PRICE: &[Case] = &[
    Case {
        id: "price#0",
        cells: &[],
        formula: "=PRICE(\"2008-11-11\",\"2021-3-1\",0.0785,0.0625,100,2,1)",
        expect: Exp::Num(113.580039836105),
    },
    Case {
        id: "price#1",
        cells: &[],
        formula: "=PRICE(\"2028-11-11\",\"2021-3-1\",0.0785,0.0625,100,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "price#2",
        cells: &[],
        formula: "=PRICE(\"2008-11-11\",\"2021-3-1\",-0.0785,0.0625,100,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "price#3",
        cells: &[],
        formula: "=PRICE(\"2008-11-11\",\"2021-3-1\",0.0785,-0.0625,100,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "price#4",
        cells: &[],
        formula: "=PRICE(\"2008-11-11\",\"2021-3-1\",0.0785,0.0625,100,3,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "price#5",
        cells: &[],
        formula: "=PRICE(\"2008-11-11\",\"2021-3-1\",0.0785,0.0625,100,1,1)",
        expect: Exp::Num(113.405808810527),
    },
    Case {
        id: "price#6",
        cells: &[],
        formula: "=PRICE(\"2008-11-11\",\"2021-3-1\",0.0785,0.0625,100,4,1)",
        expect: Exp::Num(113.662334504),
    },
    Case {
        id: "price#7",
        cells: &[],
        formula: "=PRICE(\"2008-11-11\",\"2021-3-1\",0.0785,0.0625,100,2,5)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "price#8",
        cells: &[],
        formula: "=PRICE(\"2008-11-11\",\"2021-3-1\",0.0785,0.0625,100,2,0)",
        expect: Exp::Num(113.581332152),
    },
    Case {
        id: "price#9",
        cells: &[],
        formula: "=PRICE(\"2008-11-11\",\"2021-3-1\",0.0785,0.0625,100,2,2)",
        expect: Exp::Num(113.57920636475),
    },
    Case {
        id: "price#10",
        cells: &[],
        formula: "=PRICE(\"2008-11-11\",\"2021-3-1\",0.0785,0.0625,100,2,3)",
        expect: Exp::Num(113.581273866497),
    },
    Case {
        id: "price#11",
        cells: &[],
        formula: "=PRICE(\"2008-11-11\",\"2021-3-1\",0.0785,0.0625,100,2,4)",
        expect: Exp::Num(113.581332152),
    },
    Case {
        id: "price#12",
        cells: &[],
        formula: "=PRICE(\"2008-11-11\",\"2021-3-1\",0.0785,0.0625,100,2,ZZ999)",
        expect: Exp::Num(113.581332152),
    },
    Case {
        id: "price#13",
        cells: &[],
        formula: "=PRICE(#NAME?,\"2021-3-1\",0.0785,0.0625,100,2,1)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "price#14",
        cells: &[],
        formula: "=PRICE(\"2008-11-11\",\"2021-3-1\",0.0785,0.0625,100,TRUE,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "price#15",
        cells: &[],
        formula: "=PRICE(\"test\",\"2021-3-1\",0.0785,0.0625,100,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "price#16",
        cells: &[],
        formula: "=PRICE(\"2008-11-11\",\"test\",0.0785,0.0625,100,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "price#17",
        cells: &[],
        formula: "=PRICE(\"2008-11-11\",\"2021-3-1\",\"test\",0.0625,100,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "price#18",
        cells: &[],
        formula: "=PRICE(0,366,0.08,0.09,100,2,1)",
        expect: Exp::Err(E::Num),
    },
];

#[test]
fn price_matches_the_reference_cases() {
    let failures = run(PRICE);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// PRICEDISC: 16 cases (2 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const PRICEDISC: &[Case] = &[
    Case {
        id: "pricedisc#0",
        cells: &[],
        formula: "=PRICEDISC(\"2008-11-11\",\"2021-3-1\",0.0625,100,1)",
        expect: Exp::Num(23.1252444271),
    },
    Case {
        id: "pricedisc#1",
        cells: &[],
        formula: "=PRICEDISC(\"2028-11-11\",\"2021-3-1\",0.0625,100,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "pricedisc#2",
        cells: &[],
        formula: "=PRICEDISC(\"2008-11-11\",\"2021-3-1\",-0.0625,100,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "pricedisc#3",
        cells: &[],
        formula: "=PRICEDISC(\"2008-11-11\",\"2021-3-1\",0.0625,-100,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "pricedisc#4",
        cells: &[],
        formula: "=PRICEDISC(\"2008-11-11\",\"2021-3-1\",0.0625,100,5)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "pricedisc#5",
        cells: &[],
        formula: "=PRICEDISC(\"2008-11-11\",\"2021-3-1\",0.0625,100,0)",
        expect: Exp::Num(23.0902777778),
    },
    Case {
        id: "pricedisc#6",
        cells: &[],
        formula: "=PRICEDISC(\"2008-11-11\",\"2021-3-1\",0.0625,100,2)",
        expect: Exp::Num(21.9965277778),
    },
    Case {
        id: "pricedisc#7",
        cells: &[],
        formula: "=PRICEDISC(\"2008-11-11\",\"2021-3-1\",0.0625,100,3)",
        expect: Exp::Num(23.0650684932),
    },
    Case {
        id: "pricedisc#8",
        cells: &[],
        formula: "=PRICEDISC(\"2008-11-11\",\"2021-3-1\",0.0625,100,4)",
        expect: Exp::Num(23.0902777778),
    },
    Case {
        id: "pricedisc#9",
        cells: &[],
        formula: "=PRICEDISC(\"2008-11-11\",\"2021-3-1\",0.0625,100,ZZ999)",
        expect: Exp::Num(23.0902777778),
    },
    Case {
        id: "pricedisc#10",
        cells: &[],
        formula: "=PRICEDISC(#NAME?,\"2021-3-1\",0.0625,100,1)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "pricedisc#11",
        cells: &[],
        formula: "=PRICEDISC(\"2008-11-11\",\"2021-3-1\",0.0625,100,TRUE)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "pricedisc#12",
        cells: &[],
        formula: "=PRICEDISC(\"test\",\"2021-3-1\",0.0625,100,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "pricedisc#13",
        cells: &[],
        formula: "=PRICEDISC(\"2008-11-11\",\"test\",0.0625,100,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "pricedisc#14",
        cells: &[],
        formula: "=PRICEDISC(\"2008-11-11\",\"2021-3-1\",\"test\",100,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "pricedisc#15",
        cells: &[],
        formula: "=PRICEDISC(0,366,0.1,100,1)",
        expect: Exp::Close(89.97260273972603, 5e-13),
    },
];

#[test]
fn pricedisc_matches_the_reference_cases() {
    let failures = run(PRICEDISC);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// PRICEMAT: 18 cases (3 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const PRICEMAT: &[Case] = &[
    Case {
        id: "pricemat#0",
        cells: &[],
        formula: "=PRICEMAT(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,0.0625,1)",
        expect: Exp::Num(110.862780081706),
    },
    Case {
        id: "pricemat#1",
        cells: &[],
        formula: "=PRICEMAT(1,366,0,0.06,0.08,0)",
        expect: Exp::Close(98.1469135802469, 5e-11),
    },
    Case {
        id: "pricemat#2",
        cells: &[],
        formula: "=PRICEMAT(\"2018-11-11\",\"2021-3-1\",\"2028-10-15\",0.0785,0.0625,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "pricemat#3",
        cells: &[],
        formula: "=PRICEMAT(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",-0.0785,0.0625,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "pricemat#4",
        cells: &[],
        formula: "=PRICEMAT(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,-0.0625,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "pricemat#5",
        cells: &[],
        formula: "=PRICEMAT(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,0.0625,5)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "pricemat#6",
        cells: &[],
        formula: "=PRICEMAT(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,0.0625,0)",
        expect: Exp::Num(110.882869098244),
    },
    Case {
        id: "pricemat#7",
        cells: &[],
        formula: "=PRICEMAT(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,0.0625,2)",
        expect: Exp::Num(110.96026004584),
    },
    Case {
        id: "pricemat#8",
        cells: &[],
        formula: "=PRICEMAT(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,0.0625,3)",
        expect: Exp::Num(110.878910539315),
    },
    Case {
        id: "pricemat#9",
        cells: &[],
        formula: "=PRICEMAT(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,0.0625,4)",
        expect: Exp::Num(110.882869098244),
    },
    Case {
        id: "pricemat#10",
        cells: &[],
        formula: "=PRICEMAT(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,0.0625,ZZ999)",
        expect: Exp::Num(110.882869098244),
    },
    Case {
        id: "pricemat#11",
        cells: &[],
        formula: "=PRICEMAT(#NAME?,\"2021-3-1\",\"2008-10-15\",0.0785,0.0625,1)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "pricemat#12",
        cells: &[],
        formula: "=PRICEMAT(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,0.0625,TRUE)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "pricemat#13",
        cells: &[],
        formula: "=PRICEMAT(\"test\",\"2021-3-1\",\"2008-10-15\",0.0785,0.0625,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "pricemat#14",
        cells: &[],
        formula: "=PRICEMAT(\"2008-11-11\",\"test\",\"2008-10-15\",0.0785,0.0625,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "pricemat#15",
        cells: &[],
        formula: "=PRICEMAT(\"2008-11-11\",\"2021-3-1\",\"test\",0.0785,0.0625,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "pricemat#16",
        cells: &[],
        formula: "=PRICEMAT(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"test\",0.0625,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "pricemat#17",
        cells: &[],
        formula: "=PRICEMAT(0,366,0,100,0.1,1)",
        expect: Exp::Err(E::Num),
    },
];

#[test]
fn pricemat_matches_the_reference_cases() {
    let failures = run(PRICEMAT);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// RECEIVED: 17 cases (2 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const RECEIVED: &[Case] = &[
    Case {
        id: "received#0",
        cells: &[],
        formula: "=RECEIVED(\"2008-2-15\",\"2008-3-15\",10000000,0.0575,2)",
        expect: Exp::Num(10046534.9919418),
    },
    Case {
        id: "received#1",
        cells: &[],
        formula: "=RECEIVED(\"1990-2-15\",\"2008-3-15\",10000000,0.0575,2)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "received#2",
        cells: &[],
        formula: "=RECEIVED(\"2028-11-11\",\"2021-3-1\",10000000,0.0575,2)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "received#3",
        cells: &[],
        formula: "=RECEIVED(\"2008-2-15\",\"2008-3-15\",-10000000,0.0575,2)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "received#4",
        cells: &[],
        formula: "=RECEIVED(\"2008-2-15\",\"2008-3-15\",10000000,-0.0575,2)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "received#5",
        cells: &[],
        formula: "=RECEIVED(\"2008-2-15\",\"2008-3-15\",10000000,0.0575,5)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "received#6",
        cells: &[],
        formula: "=RECEIVED(\"2008-2-15\",\"2008-3-15\",10000000,0.0575,0)",
        expect: Exp::Num(10048147.3728281),
    },
    Case {
        id: "received#7",
        cells: &[],
        formula: "=RECEIVED(\"2008-2-15\",\"2008-3-15\",10000000,0.0575,1)",
        expect: Exp::Num(10045768.6316757),
    },
    Case {
        id: "received#8",
        cells: &[],
        formula: "=RECEIVED(\"2008-2-15\",\"2008-3-15\",10000000,0.0575,3)",
        expect: Exp::Num(10045894.6006757),
    },
    Case {
        id: "received#9",
        cells: &[],
        formula: "=RECEIVED(\"2008-2-15\",\"2008-3-15\",10000000,0.0575,4)",
        expect: Exp::Num(10048147.3728281),
    },
    Case {
        id: "received#10",
        cells: &[],
        formula: "=RECEIVED(\"2008-2-15\",\"2008-3-15\",10000000,0.0575,ZZ999)",
        expect: Exp::Num(10048147.3728281),
    },
    Case {
        id: "received#11",
        cells: &[],
        formula: "=RECEIVED(#NAME?,\"2008-3-15\",10000000,0.0575,2)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "received#12",
        cells: &[],
        formula: "=RECEIVED(\"2008-2-15\",\"2008-3-15\",10000000,0.0575,TRUE)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "received#13",
        cells: &[],
        formula: "=RECEIVED(\"test\",\"2008-3-15\",10000000,0.0575,2)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "received#14",
        cells: &[],
        formula: "=RECEIVED(\"2008-2-15\",\"test\",10000000,0.0575,2)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "received#15",
        cells: &[],
        formula: "=RECEIVED(\"2008-2-15\",\"2008-3-15\",10000000,\"test\",2)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "received#16",
        cells: &[],
        formula: "=RECEIVED(0,366,100,0.1,1)",
        expect: Exp::Close(111.14494518879415, 5e-13),
    },
];

#[test]
fn received_matches_the_reference_cases() {
    let failures = run(RECEIVED);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// TBILLEQ: 14 cases (2 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const TBILLEQ: &[Case] = &[
    Case {
        id: "tbilleq#0",
        cells: &[],
        formula: "=TBILLEQ(\"2008-3-31\",\"2008-6-1\",0.0914)",
        expect: Exp::Num(0.0941514935659),
    },
    Case {
        id: "tbilleq#1",
        cells: &[],
        formula: "=TBILLEQ(\"2008-3-31\",\"2008-11-1\",0.0914)",
        expect: Exp::Num(0.0973043585198),
    },
    Case {
        id: "tbilleq#2",
        cells: &[],
        formula: "=TBILLEQ(0,366,0.0914)",
        expect: Exp::Close(0.0999453758814, 5e-11),
    },
    Case {
        id: "tbilleq#3",
        cells: &[],
        formula: "=TBILLEQ(\"2018-3-31\",\"2008-6-1\",0.0914)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "tbilleq#4",
        cells: &[],
        formula: "=TBILLEQ(\"2008-3-31\",\"2018-6-1\",0.0914)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "tbilleq#5",
        cells: &[],
        formula: "=TBILLEQ(60,366,0.0914)",
        expect: Exp::Close(0.09851681907154934, 5e-13),
    },
    Case {
        id: "tbilleq#6",
        cells: &[],
        formula: "=TBILLEQ(\"2008-3-31\",\"2008-6-1\",-0.0914)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "tbilleq#7",
        cells: &[],
        formula: "=TBILLEQ(\"2008-3-31\",\"2008-6-1\",11)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "tbilleq#8",
        cells: &[],
        formula: "=TBILLEQ(\"2008-3-31\",\"2008-12-1\",11)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "tbilleq#9",
        cells: &[],
        formula: "=TBILLEQ(#NAME?,\"2008-6-1\",0.0914)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "tbilleq#10",
        cells: &[],
        formula: "=TBILLEQ(\"2008-3-31\",\"2008-6-1\",TRUE)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "tbilleq#11",
        cells: &[],
        formula: "=TBILLEQ(\"test\",\"2008-6-1\",0.0914)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "tbilleq#12",
        cells: &[],
        formula: "=TBILLEQ(\"2008-3-31\",\"test\",0.0914)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "tbilleq#13",
        cells: &[],
        formula: "=TBILLEQ(\"2008-3-31\",\"2008-6-1\",\"test\")",
        expect: Exp::Err(E::Value),
    },
];

#[test]
fn tbilleq_matches_the_reference_cases() {
    let failures = run(TBILLEQ);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// TBILLPRICE: 12 cases (2 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const TBILLPRICE: &[Case] = &[
    Case {
        id: "tbillprice#0",
        cells: &[],
        formula: "=TBILLPRICE(\"2008-3-31\",\"2008-6-1\",0.0914)",
        expect: Exp::Num(98.4258888889),
    },
    Case {
        id: "tbillprice#1",
        cells: &[],
        formula: "=TBILLPRICE(0,366,0.0914)",
        expect: Exp::Close(90.7076666667, 5e-11),
    },
    Case {
        id: "tbillprice#2",
        cells: &[],
        formula: "=TBILLPRICE(\"2018-3-31\",\"2008-6-1\",0.0914)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "tbillprice#3",
        cells: &[],
        formula: "=TBILLPRICE(\"2008-3-31\",\"2018-6-1\",0.0914)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "tbillprice#4",
        cells: &[],
        formula: "=TBILLPRICE(60,366,0.0914)",
        expect: Exp::Close(92.231, 5e-13),
    },
    Case {
        id: "tbillprice#5",
        cells: &[],
        formula: "=TBILLPRICE(\"2008-3-31\",\"2008-6-1\",-0.0914)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "tbillprice#6",
        cells: &[],
        formula: "=TBILLPRICE(\"2008-3-31\",\"2008-6-1\",11)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "tbillprice#7",
        cells: &[],
        formula: "=TBILLPRICE(#NAME?,\"2008-6-1\",0.0914)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "tbillprice#8",
        cells: &[],
        formula: "=TBILLPRICE(\"2008-3-31\",\"2008-6-1\",TRUE)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "tbillprice#9",
        cells: &[],
        formula: "=TBILLPRICE(\"test\",\"2008-6-1\",0.0914)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "tbillprice#10",
        cells: &[],
        formula: "=TBILLPRICE(\"2008-3-31\",\"test\",0.0914)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "tbillprice#11",
        cells: &[],
        formula: "=TBILLPRICE(\"2008-3-31\",\"2008-6-1\",\"test\")",
        expect: Exp::Err(E::Value),
    },
];

#[test]
fn tbillprice_matches_the_reference_cases() {
    let failures = run(TBILLPRICE);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// TBILLYIELD: 11 cases (2 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const TBILLYIELD: &[Case] = &[
    Case {
        id: "tbillyield#0",
        cells: &[],
        formula: "=TBILLYIELD(\"2008-3-31\",\"2008-6-1\",98.45)",
        expect: Exp::Num(0.0914169629253),
    },
    Case {
        id: "tbillyield#1",
        cells: &[],
        formula: "=TBILLYIELD(0,366,98.45)",
        expect: Exp::Close(0.0154859336103, 5e-11),
    },
    Case {
        id: "tbillyield#2",
        cells: &[],
        formula: "=TBILLYIELD(\"2018-3-31\",\"2008-6-1\",98.45)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "tbillyield#3",
        cells: &[],
        formula: "=TBILLYIELD(\"2008-3-31\",\"2018-6-1\",98.45)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "tbillyield#4",
        cells: &[],
        formula: "=TBILLYIELD(60,366,90)",
        expect: Exp::Close(0.13071895424836602, 5e-13),
    },
    Case {
        id: "tbillyield#5",
        cells: &[],
        formula: "=TBILLYIELD(\"2008-3-31\",\"2008-6-1\",-98.45)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "tbillyield#6",
        cells: &[],
        formula: "=TBILLYIELD(#NAME?,\"2008-6-1\",98.45)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "tbillyield#7",
        cells: &[],
        formula: "=TBILLYIELD(\"2008-3-31\",\"2008-6-1\",TRUE)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "tbillyield#8",
        cells: &[],
        formula: "=TBILLYIELD(\"test\",\"2008-6-1\",98.45)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "tbillyield#9",
        cells: &[],
        formula: "=TBILLYIELD(\"2008-3-31\",\"test\",98.45)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "tbillyield#10",
        cells: &[],
        formula: "=TBILLYIELD(\"2008-3-31\",\"2008-6-1\",\"test\")",
        expect: Exp::Err(E::Value),
    },
];

#[test]
fn tbillyield_matches_the_reference_cases() {
    let failures = run(TBILLYIELD);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// VDB: 11 cases (1 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const VDB: &[Case] = &[
    Case {
        id: "vdb#0",
        cells: &[],
        formula: "=VDB(24000,3000,10,1,2,2,0)",
        expect: Exp::Num(3840.0),
    },
    Case {
        id: "vdb#1",
        cells: &[],
        formula: "=VDB(\"test\",3000,10,1,2,2,0)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "vdb#2",
        cells: &[],
        formula: "=VDB(-24000,3000,10,1,2,2,0)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "vdb#3",
        cells: &[],
        formula: "=VDB(-24000,-3000,10,1,2,2,0)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "vdb#4",
        cells: &[],
        formula: "=VDB(2400,3000,10,1,2,2,0)",
        expect: Exp::Num(0.0),
    },
    Case {
        id: "vdb#5",
        cells: &[],
        formula: "=VDB(2400,3000,10,0.1,2,2,0)",
        expect: Exp::Num(-600.0),
    },
    Case {
        id: "vdb#6",
        cells: &[],
        formula: "=VDB(24000,3000,0,0,0,2,0)",
        expect: Exp::Err(E::DivByZero),
    },
    Case {
        id: "vdb#7",
        cells: &[],
        formula: "=VDB(24000,3000,10,22,12,2,0)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "vdb#8",
        cells: &[],
        formula: "=VDB(24000,3000,10,6.1,6.2,2,0)",
        expect: Exp::Num(123.3125376),
    },
    Case {
        id: "vdb#9",
        cells: &[],
        formula: "=VDB(24000,3000,10,1,2,2,1)",
        expect: Exp::Num(3840.0),
    },
    Case {
        id: "vdb#10",
        cells: &[],
        formula: "=VDB(24000,3000,10,1,2,ZZ999,ZZ999)",
        expect: Exp::Num(3840.0),
    },
];

#[test]
fn vdb_matches_the_reference_cases() {
    let failures = run(VDB);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// YIELD: 21 cases (2 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const YIELD: &[Case] = &[
    Case {
        id: "yield#0",
        cells: &[],
        formula: "=YIELD(\"2008-11-11\",\"2021-3-1\",0.0785,98.45,100,2,1)",
        expect: Exp::Num(0.0804842002961),
    },
    Case {
        id: "yield#1",
        cells: &[],
        formula: "=YIELD(\"2008-11-11\",\"2009-3-1\",0.0785,98.45,100,2,1)",
        expect: Exp::Num(0.129522508296),
    },
    Case {
        id: "yield#2",
        cells: &[],
        formula: "=YIELD(\"2028-11-11\",\"2021-3-1\",0.0785,98.45,100,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "yield#3",
        cells: &[],
        formula: "=YIELD(\"2008-11-11\",\"2021-3-1\",-0.0785,98.45,100,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "yield#4",
        cells: &[],
        formula: "=YIELD(\"2008-11-11\",\"2021-3-1\",0.0785,0,100,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "yield#5",
        cells: &[],
        formula: "=YIELD(\"2008-11-11\",\"2021-3-1\",0.0785,98.45,0,2,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "yield#6",
        cells: &[],
        formula: "=YIELD(\"2008-11-11\",\"2021-3-1\",0.0785,98.45,100,3,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "yield#7",
        cells: &[],
        formula: "=YIELD(\"2008-11-11\",\"2021-3-1\",0.0785,98.45,100,1,1)",
        expect: Exp::Num(0.0804467519494),
    },
    Case {
        id: "yield#8",
        cells: &[],
        formula: "=YIELD(\"2008-11-11\",\"2021-3-1\",0.0785,98.45,100,4,1)",
        expect: Exp::Num(0.0804921326093),
    },
    Case {
        id: "yield#9",
        cells: &[],
        formula: "=YIELD(\"2008-11-11\",\"2021-3-1\",0.0785,98.45,100,2,5)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "yield#10",
        cells: &[],
        formula: "=YIELD(\"2008-11-11\",\"2021-3-1\",0.0785,98.45,100,2,0)",
        expect: Exp::Num(0.0804841133636),
    },
    Case {
        id: "yield#11",
        cells: &[],
        formula: "=YIELD(\"2008-11-11\",\"2021-3-1\",0.0785,98.45,100,2,2)",
        expect: Exp::Num(0.0804842576475),
    },
    Case {
        id: "yield#12",
        cells: &[],
        formula: "=YIELD(\"2008-11-11\",\"2021-3-1\",0.0785,98.45,100,2,3)",
        expect: Exp::Num(0.0804841172323),
    },
    Case {
        id: "yield#13",
        cells: &[],
        formula: "=YIELD(\"2008-11-11\",\"2021-3-1\",0.0785,98.45,100,2,4)",
        expect: Exp::Num(0.0804841133636),
    },
    Case {
        id: "yield#14",
        cells: &[],
        formula: "=YIELD(\"2008-11-11\",\"2021-3-1\",0.0785,98.45,100,2,ZZ999)",
        expect: Exp::Num(0.0804841133636),
    },
    Case {
        id: "yield#15",
        cells: &[],
        formula: "=YIELD(#NAME?,\"2021-3-1\",0.0785,98.45,100,2,1)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "yield#16",
        cells: &[],
        formula: "=YIELD(\"2008-11-11\",\"2021-3-1\",0.0785,98.45,100,TRUE,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "yield#17",
        cells: &[],
        formula: "=YIELD(\"test\",\"2021-3-1\",0.0785,98.45,100,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "yield#18",
        cells: &[],
        formula: "=YIELD(\"2008-11-11\",\"test\",0.0785,98.45,100,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "yield#19",
        cells: &[],
        formula: "=YIELD(\"2008-11-11\",\"2021-3-1\",\"test\",98.45,100,2,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "yield#20",
        cells: &[],
        formula: "=YIELD(0,366,0.08,90,100,2,1)",
        expect: Exp::Err(E::Num),
    },
];

#[test]
fn yield_matches_the_reference_cases() {
    let failures = run(YIELD);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// YIELDDISC: 16 cases (2 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const YIELDDISC: &[Case] = &[
    Case {
        id: "yielddisc#0",
        cells: &[],
        formula: "=YIELDDISC(\"2008-11-11\",\"2021-3-1\",98.45,100,1)",
        expect: Exp::Num(0.00128000671242),
    },
    Case {
        id: "yielddisc#1",
        cells: &[],
        formula: "=YIELDDISC(\"2028-11-11\",\"2021-3-1\",98.45,100,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "yielddisc#2",
        cells: &[],
        formula: "=YIELDDISC(\"2008-11-11\",\"2021-3-1\",0,100,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "yielddisc#3",
        cells: &[],
        formula: "=YIELDDISC(\"2008-11-11\",\"2021-3-1\",98.45,0,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "yielddisc#4",
        cells: &[],
        formula: "=YIELDDISC(\"2008-11-11\",\"2021-3-1\",98.45,100,5)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "yielddisc#5",
        cells: &[],
        formula: "=YIELDDISC(\"2008-11-11\",\"2021-3-1\",98.45,100,0)",
        expect: Exp::Num(0.00127942476329),
    },
    Case {
        id: "yielddisc#6",
        cells: &[],
        formula: "=YIELDDISC(\"2008-11-11\",\"2021-3-1\",98.45,100,2)",
        expect: Exp::Num(0.00126148491016),
    },
    Case {
        id: "yielddisc#7",
        cells: &[],
        formula: "=YIELDDISC(\"2008-11-11\",\"2021-3-1\",98.45,100,3)",
        expect: Exp::Num(0.00127900553392),
    },
    Case {
        id: "yielddisc#8",
        cells: &[],
        formula: "=YIELDDISC(\"2008-11-11\",\"2021-3-1\",98.45,100,4)",
        expect: Exp::Num(0.00127942476329),
    },
    Case {
        id: "yielddisc#9",
        cells: &[],
        formula: "=YIELDDISC(\"2008-11-11\",\"2021-3-1\",98.45,100,ZZ999)",
        expect: Exp::Num(0.00127942476329),
    },
    Case {
        id: "yielddisc#10",
        cells: &[],
        formula: "=YIELDDISC(#NAME?,\"2021-3-1\",98.45,100,1)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "yielddisc#11",
        cells: &[],
        formula: "=YIELDDISC(\"2008-11-11\",\"2021-3-1\",98.45,100,TRUE)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "yielddisc#12",
        cells: &[],
        formula: "=YIELDDISC(\"test\",\"2021-3-1\",98.45,100,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "yielddisc#13",
        cells: &[],
        formula: "=YIELDDISC(\"2008-11-11\",\"test\",98.45,100,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "yielddisc#14",
        cells: &[],
        formula: "=YIELDDISC(\"2008-11-11\",\"2021-3-1\",\"test\",100,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "yielddisc#15",
        cells: &[],
        formula: "=YIELDDISC(0,366,90,100,1)",
        expect: Exp::Close(0.11080752884031576, 5e-13),
    },
];

#[test]
fn yielddisc_matches_the_reference_cases() {
    let failures = run(YIELDDISC);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// YIELDMAT: 18 cases (3 not ported: 1 array-broadcast, 1904 date system or an error type the engine lacks)
const YIELDMAT: &[Case] = &[
    Case {
        id: "yieldmat#0",
        cells: &[],
        formula: "=YIELDMAT(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,98.45,1)",
        expect: Exp::Num(0.080544639989),
    },
    Case {
        id: "yieldmat#1",
        cells: &[],
        formula: "=YIELDMAT(1,366,0,0.06,99,0)",
        expect: Exp::Close(0.0706951691634, 5e-11),
    },
    Case {
        id: "yieldmat#2",
        cells: &[],
        formula: "=YIELDMAT(\"2018-11-11\",\"2021-3-1\",\"2028-10-15\",0.0785,98.45,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "yieldmat#3",
        cells: &[],
        formula: "=YIELDMAT(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",-0.0785,98.45,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "yieldmat#4",
        cells: &[],
        formula: "=YIELDMAT(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,0,1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "yieldmat#5",
        cells: &[],
        formula: "=YIELDMAT(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,98.45,5)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "yieldmat#6",
        cells: &[],
        formula: "=YIELDMAT(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,98.45,0)",
        expect: Exp::Num(0.0805514592749),
    },
    Case {
        id: "yieldmat#7",
        cells: &[],
        formula: "=YIELDMAT(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,98.45,2)",
        expect: Exp::Num(0.0805158908953),
    },
    Case {
        id: "yieldmat#8",
        cells: &[],
        formula: "=YIELDMAT(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,98.45,3)",
        expect: Exp::Num(0.0805398660055),
    },
    Case {
        id: "yieldmat#9",
        cells: &[],
        formula: "=YIELDMAT(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,98.45,4)",
        expect: Exp::Num(0.0805514592749),
    },
    Case {
        id: "yieldmat#10",
        cells: &[],
        formula: "=YIELDMAT(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,98.45,ZZ999)",
        expect: Exp::Num(0.0805514592749),
    },
    Case {
        id: "yieldmat#11",
        cells: &[],
        formula: "=YIELDMAT(#NAME?,\"2021-3-1\",\"2008-10-15\",0.0785,98.45,1)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "yieldmat#12",
        cells: &[],
        formula: "=YIELDMAT(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",0.0785,98.45,TRUE)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "yieldmat#13",
        cells: &[],
        formula: "=YIELDMAT(\"test\",\"2021-3-1\",\"2008-10-15\",0.0785,98.45,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "yieldmat#14",
        cells: &[],
        formula: "=YIELDMAT(\"2008-11-11\",\"test\",\"2008-10-15\",0.0785,98.45,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "yieldmat#15",
        cells: &[],
        formula: "=YIELDMAT(\"2008-11-11\",\"2021-3-1\",\"test\",0.0785,98.45,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "yieldmat#16",
        cells: &[],
        formula: "=YIELDMAT(\"2008-11-11\",\"2021-3-1\",\"2008-10-15\",\"test\",98.45,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "yieldmat#17",
        cells: &[],
        formula: "=YIELDMAT(0,366,0,0.06,99,0)",
        expect: Exp::Err(E::Num),
    },
];

#[test]
fn yieldmat_matches_the_reference_cases() {
    let failures = run(YIELDMAT);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

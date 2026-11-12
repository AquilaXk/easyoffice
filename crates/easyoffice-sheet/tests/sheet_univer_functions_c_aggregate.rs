//! Formula function bundle C, `AGGREGATE` (PD-205).
//!
//! Generated from the reference implementation's unit tests: same data, same expected values.
//! A case marked `DIFFERS` keeps the reference value in its comment; see PD-205.

// the expected values are the reference's literals, some of them close to a named constant
#![allow(clippy::approx_constant)]

use easyoffice_sheet::{
    EvaluatedCellValue as V, FormulaError as E, PackedCellCoordinate, SheetEngine,
};

#[allow(dead_code)] // one matcher shared by every bundle file, each uses some of the variants
enum Exp {
    Num(f64),
    Close(f64, f64),
    Text(&'static str),
    Bool(bool),
    Err(E),
    Empty,
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
        (V::Text(g), Exp::Text(w)) => g == w,
        (V::Boolean(g), Exp::Bool(w)) => g == w,
        (V::Error(g), Exp::Err(w)) => g == w,
        (V::Empty, Exp::Empty) => true,
        (V::Text(g), Exp::Empty) => g.is_empty(),
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

const CELLS: &[(&str, &str)] = &[
    ("A1", "1"),
    ("B1", "2"),
    ("C1", "=#NAME?"),
    ("A2", "3"),
    ("B2", "4"),
    ("C2", "=\"B2\""),
    ("D2", "=\"R2C2\""),
    ("A3", "1"),
    ("B3", "=\" \""),
    ("C3", "1.23"),
    ("D3", "=TRUE"),
    ("E3", "=FALSE"),
    ("A4", "0"),
    ("B4", "100"),
    ("C4", "2.34"),
    ("D4", "=\"test\""),
    ("E4", "-3"),
];

// AGGREGATE: reference cases on the shared data (row 4 is hidden in the reference's last block)
const AGGREGATE: &[Case] = &[
    Case {
        id: "aggregate#0",
        cells: CELLS,
        formula: "=AGGREGATE(1,,1)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "aggregate#1",
        cells: CELLS,
        formula: "=AGGREGATE(2,,\"test\")",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "aggregate#2",
        cells: CELLS,
        formula: "=AGGREGATE(3,,\"1\")",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "aggregate#3",
        cells: CELLS,
        formula: "=AGGREGATE(4,,TRUE)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "aggregate#4",
        cells: CELLS,
        formula: "=AGGREGATE(5,,FALSE)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "aggregate#5",
        cells: CELLS,
        formula: "=AGGREGATE(7,,C1)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "aggregate#6",
        cells: CELLS,
        formula: "=AGGREGATE(1,,D1)",
        expect: Exp::Err(E::DivByZero),
    },
    Case {
        id: "aggregate#7",
        cells: CELLS,
        formula: "=AGGREGATE(2,,D1)",
        expect: Exp::Num(0.0),
    },
    Case {
        id: "aggregate#8",
        cells: CELLS,
        formula: "=AGGREGATE(3,,D1)",
        expect: Exp::Num(0.0),
    },
    Case {
        id: "aggregate#9",
        cells: CELLS,
        formula: "=AGGREGATE(4,,D1)",
        expect: Exp::Num(0.0),
    },
    Case {
        id: "aggregate#10",
        cells: CELLS,
        formula: "=AGGREGATE(5,,D1)",
        expect: Exp::Num(0.0),
    },
    Case {
        id: "aggregate#11",
        cells: CELLS,
        formula: "=AGGREGATE(6,,D1)",
        expect: Exp::Num(0.0),
    },
    Case {
        id: "aggregate#12",
        cells: CELLS,
        formula: "=AGGREGATE(7,,D1)",
        expect: Exp::Err(E::DivByZero),
    },
    Case {
        id: "aggregate#13",
        cells: CELLS,
        formula: "=AGGREGATE(8,,D1)",
        expect: Exp::Err(E::DivByZero),
    },
    Case {
        id: "aggregate#14",
        cells: CELLS,
        formula: "=AGGREGATE(9,,D1)",
        expect: Exp::Num(0.0),
    },
    Case {
        id: "aggregate#15",
        cells: CELLS,
        formula: "=AGGREGATE(10,,D1)",
        expect: Exp::Err(E::DivByZero),
    },
    Case {
        id: "aggregate#16",
        cells: CELLS,
        formula: "=AGGREGATE(11,,D1)",
        expect: Exp::Err(E::DivByZero),
    },
    Case {
        id: "aggregate#17",
        cells: CELLS,
        formula: "=AGGREGATE(12,,D1)",
        expect: Exp::Err(E::Num),
    },
    Case {
        id: "aggregate#18",
        cells: CELLS,
        formula: "=AGGREGATE(13,,D1)",
        expect: Exp::Err(E::Na),
    },
    Case {
        id: "aggregate#19",
        cells: CELLS,
        formula: "=AGGREGATE(9,,A1:B2,C1:C2)",
        expect: Exp::Err(E::Name),
    },
    Case {
        id: "aggregate#20",
        cells: CELLS,
        formula: "=AGGREGATE(1,,A1:B2,A3:F4)",
        expect: Exp::Num(11.157),
    },
    Case {
        id: "aggregate#21",
        cells: CELLS,
        formula: "=AGGREGATE(2,,A1:B2,A3:F4)",
        expect: Exp::Num(10.0),
    },
    Case {
        id: "aggregate#22",
        cells: CELLS,
        formula: "=AGGREGATE(2,,B1:C1)",
        expect: Exp::Num(1.0),
    },
    Case {
        id: "aggregate#23",
        cells: CELLS,
        formula: "=AGGREGATE(3,,A1:B2,A3:F4)",
        expect: Exp::Num(14.0),
    },
    Case {
        id: "aggregate#24",
        cells: CELLS,
        formula: "=AGGREGATE(3,,B1:C1)",
        expect: Exp::Num(2.0),
    },
    Case {
        id: "aggregate#25",
        cells: CELLS,
        formula: "=AGGREGATE(4,,A1:B2,A3:F4)",
        expect: Exp::Num(100.0),
    },
    Case {
        id: "aggregate#26",
        cells: CELLS,
        formula: "=AGGREGATE(5,,A1:B2,A3:F4)",
        expect: Exp::Num(-3.0),
    },
    Case {
        id: "aggregate#27",
        cells: CELLS,
        formula: "=AGGREGATE(6,,A1:B2,A3:F4)",
        expect: Exp::Num(0.0),
    },
    Case {
        id: "aggregate#28",
        cells: CELLS,
        formula: "=AGGREGATE(7,,A1:B2,A3:F4)",
        expect: Exp::Num(31.273350405),
    },
    Case {
        id: "aggregate#29",
        cells: CELLS,
        formula: "=AGGREGATE(8,,A1:B2,A3:F4)",
        expect: Exp::Num(29.6685052033),
    },
    Case {
        id: "aggregate#30",
        cells: CELLS,
        formula: "=AGGREGATE(9,,A1:B2,A3:F4)",
        expect: Exp::Num(111.57),
    },
    Case {
        id: "aggregate#31",
        cells: CELLS,
        formula: "=AGGREGATE(10,,A1:B2,A3:F4)",
        expect: Exp::Close(978.0224456, 5e-8),
    },
    Case {
        id: "aggregate#32",
        cells: CELLS,
        formula: "=AGGREGATE(11,,A1:B2,A3:F4)",
        expect: Exp::Num(880.220201),
    },
    Case {
        id: "aggregate#33",
        cells: CELLS,
        formula: "=AGGREGATE(12,,A1:B2,A3:F4)",
        expect: Exp::Num(1.615),
    },
    Case {
        id: "aggregate#34",
        cells: CELLS,
        formula: "=AGGREGATE(13,,A1:B2,A3:F4)",
        expect: Exp::Num(1.0),
    },
    Case {
        id: "aggregate#35",
        cells: CELLS,
        formula: "=AGGREGATE(14,,A1:B2,2,3)",
        expect: Exp::Err(E::Value),
    },
    Case {
        id: "aggregate#36",
        cells: CELLS,
        formula: "=AGGREGATE(14,,A1:B2,2)",
        expect: Exp::Num(3.0),
    },
    Case {
        id: "aggregate#37",
        cells: CELLS,
        formula: "=AGGREGATE(15,,A1:B2,2)",
        expect: Exp::Num(2.0),
    },
    Case {
        id: "aggregate#38",
        cells: CELLS,
        formula: "=AGGREGATE(16,,A1:B2,0.5)",
        expect: Exp::Num(2.5),
    },
    Case {
        id: "aggregate#39",
        cells: CELLS,
        formula: "=AGGREGATE(17,,A1:B2,2)",
        expect: Exp::Num(2.5),
    },
    Case {
        id: "aggregate#40",
        cells: CELLS,
        formula: "=AGGREGATE(18,,A1:B2,0.5)",
        expect: Exp::Num(2.5),
    },
    Case {
        id: "aggregate#41",
        cells: CELLS,
        formula: "=AGGREGATE(19,,A1:B2,2)",
        expect: Exp::Num(2.5),
    },
    // DIFFERS (hidden row): reference Exp::Num(1.115). The formula engine does not know the rows the editor hides, so option 5 skips none (PD-205).
    Case {
        id: "aggregate#42",
        cells: CELLS,
        formula: "=AGGREGATE(1,5,A3:F4)",
        expect: Exp::Close(16.928333333333335, 1e-9),
    },
    // DIFFERS (hidden row): reference Exp::Num(2.0). The formula engine does not know the rows the editor hides, so option 5 skips none (PD-205).
    Case {
        id: "aggregate#43",
        cells: CELLS,
        formula: "=AGGREGATE(2,5,A3:F4)",
        expect: Exp::Close(6.0, 1e-9),
    },
    // DIFFERS (hidden row): reference Exp::Num(5.0). The formula engine does not know the rows the editor hides, so option 5 skips none (PD-205).
    Case {
        id: "aggregate#44",
        cells: CELLS,
        formula: "=AGGREGATE(3,5,A3:F4)",
        expect: Exp::Close(10.0, 1e-9),
    },
    // DIFFERS (hidden row): reference Exp::Num(1.23). The formula engine does not know the rows the editor hides, so option 5 skips none (PD-205).
    Case {
        id: "aggregate#45",
        cells: CELLS,
        formula: "=AGGREGATE(4,5,A3:F4)",
        expect: Exp::Close(100.0, 1e-9),
    },
    // DIFFERS (hidden row): reference Exp::Num(1.0). The formula engine does not know the rows the editor hides, so option 5 skips none (PD-205).
    Case {
        id: "aggregate#46",
        cells: CELLS,
        formula: "=AGGREGATE(5,5,A3:F4)",
        expect: Exp::Close(-3.0, 1e-9),
    },
    // DIFFERS (hidden row): reference Exp::Num(1.23). The formula engine does not know the rows the editor hides, so option 5 skips none (PD-205).
    Case {
        id: "aggregate#47",
        cells: CELLS,
        formula: "=AGGREGATE(6,5,A3:F4)",
        expect: Exp::Close(0.0, 1e-9),
    },
    // DIFFERS (hidden row): reference Exp::Close(0.16263456, 5e-8). The formula engine does not know the rows the editor hides, so option 5 skips none (PD-205).
    Case {
        id: "aggregate#48",
        cells: CELLS,
        formula: "=AGGREGATE(7,5,A3:F4)",
        expect: Exp::Close(40.73715179865508, 1e-9),
    },
    // DIFFERS (hidden row): reference Exp::Num(0.115). The formula engine does not know the rows the editor hides, so option 5 skips none (PD-205).
    Case {
        id: "aggregate#49",
        cells: CELLS,
        formula: "=AGGREGATE(8,5,A3:F4)",
        expect: Exp::Close(37.18776161439256, 1e-9),
    },
    // DIFFERS (hidden row): reference Exp::Num(2.23). The formula engine does not know the rows the editor hides, so option 5 skips none (PD-205).
    Case {
        id: "aggregate#50",
        cells: CELLS,
        formula: "=AGGREGATE(9,5,A3:F4)",
        expect: Exp::Close(101.57000000000001, 1e-9),
    },
    // DIFFERS (hidden row): reference Exp::Num(0.02645). The formula engine does not know the rows the editor hides, so option 5 skips none (PD-205).
    Case {
        id: "aggregate#51",
        cells: CELLS,
        formula: "=AGGREGATE(10,5,A3:F4)",
        expect: Exp::Close(1659.5155366666663, 1e-9),
    },
    // DIFFERS (hidden row): reference Exp::Num(0.013225). The formula engine does not know the rows the editor hides, so option 5 skips none (PD-205).
    Case {
        id: "aggregate#52",
        cells: CELLS,
        formula: "=AGGREGATE(11,5,A3:F4)",
        expect: Exp::Close(1382.9296138888885, 1e-9),
    },
    Case {
        id: "aggregate#53",
        cells: CELLS,
        formula: "=AGGREGATE(12,5,A3:F4)",
        expect: Exp::Num(1.115),
    },
    Case {
        id: "aggregate#54",
        cells: CELLS,
        formula: "=AGGREGATE(13,5,A3:F4)",
        expect: Exp::Err(E::Na),
    },
];

#[test]
fn aggregate_matches_the_reference_cases() {
    let failures = run(AGGREGATE);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

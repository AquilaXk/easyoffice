//! A reference into another workbook (`[Book2.xlsx]Sheet2!A1`, `'[Book2.xlsx]Sheet2:Sheet6'!A1`)
//! names sheets of that workbook, so a rename, deletion or move of a sheet of this workbook with
//! the same name and a row or column edit leave it as written.
#![allow(clippy::unwrap_used)]

use easyoffice_sheet::axis::{
    rewrite_formula_refs, rewrite_sheet_names, Axis, AxisEdit, EditKind, SheetChange,
};

const TABS: [&str; 7] = [
    "Sheet1", "Sheet2", "Sheet3", "Sheet4", "Sheet5", "Sheet6", "Sheet7",
];

/// Each way an outside reference can be written, around a reference to this workbook's `Sheet2`.
const OUTSIDE: [&str; 4] = [
    "[Book2.xlsx]Sheet2!A1",
    "'[Book2.xlsx]Sheet2'!A1",
    "SUM([Book2.xlsx]Sheet2:Sheet6!A1)",
    "SUM('[Book2.xlsx]Sheet2:Sheet6'!A1)",
];

#[test]
fn a_rename_leaves_the_references_into_another_workbook() {
    for outside in OUTSIDE {
        for (from, to) in [("Sheet2", "Renamed"), ("Sheet6", "Last")] {
            let rename = SheetChange::Rename { from, to };
            assert_eq!(rewrite_sheet_names(&format!("={outside}"), rename), None);
            let both = format!("={outside}+{from}!A1");
            assert_eq!(
                rewrite_sheet_names(&both, rename).as_deref(),
                Some(format!("={outside}+{to}!A1").as_str()),
                "{both}"
            );
        }
    }
}

#[test]
fn a_deletion_leaves_the_references_into_another_workbook() {
    for outside in OUTSIDE {
        for name in ["Sheet2", "Sheet6"] {
            let delete = SheetChange::Delete {
                name,
                tab_order: &TABS,
            };
            assert_eq!(rewrite_sheet_names(&format!("={outside}"), delete), None);
            let both = format!("={outside}+{name}!A1");
            assert_eq!(
                rewrite_sheet_names(&both, delete).as_deref(),
                Some(format!("={outside}+#REF!").as_str()),
                "{both}"
            );
        }
    }
}

#[test]
fn a_move_leaves_the_references_into_another_workbook() {
    // Sheet2 after Sheet6, and Sheet6 before Sheet2.
    let moves = [
        (
            "Sheet2",
            [
                "Sheet1", "Sheet3", "Sheet4", "Sheet5", "Sheet6", "Sheet2", "Sheet7",
            ],
        ),
        (
            "Sheet6",
            [
                "Sheet1", "Sheet6", "Sheet2", "Sheet3", "Sheet4", "Sheet5", "Sheet7",
            ],
        ),
    ];
    for outside in OUTSIDE {
        for (name, order) in &moves {
            let moved = [name.to_string()];
            let new_order: Vec<String> = order.iter().map(ToString::to_string).collect();
            let change = SheetChange::Move {
                moved: &moved,
                tab_order: &TABS,
                new_order: &new_order,
            };
            assert_eq!(rewrite_sheet_names(&format!("={outside}"), change), None);
            // The same span of this workbook beside it is rewritten.
            let both = format!("={outside}+SUM(Sheet2:Sheet6!A1)");
            let rewritten = rewrite_sheet_names(&both, change).unwrap();
            assert!(
                rewritten.starts_with(&format!("={outside}+")),
                "{rewritten}"
            );
            assert_ne!(rewritten, both);
        }
    }
}

#[test]
fn a_row_edit_leaves_the_references_into_another_workbook() {
    let edit = AxisEdit::new(Axis::Row, EditKind::Insert, 0, 1).unwrap();
    for outside in ["[Book2.xlsx]Sheet1!A5", "'[Book2.xlsx]Sheet1'!A5"] {
        let both = format!("={outside}+Sheet1!A5");
        assert_eq!(
            rewrite_formula_refs(&both, "Sheet1", "Sheet1", &edit, false).as_deref(),
            Some(format!("={outside}+Sheet1!A6").as_str()),
            "{both}"
        );
    }
}

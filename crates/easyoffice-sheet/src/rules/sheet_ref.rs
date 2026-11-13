//! Rules follow a sheet that is renamed or deleted: the formulas in their operands name the sheet
//! the way any formula does (see `axis::sheet_ref`). The ranges a rule covers belong to the sheet
//! that holds the rule, so they go with it and need no rewrite here.

use super::axis_shift::{cf_formulas, dv_formulas};
use crate::axis::{rewrite_sheet_names, SheetChange};
use easyoffice_formats::SheetRules;

fn follow(text: &mut String, change: SheetChange<'_>) {
    if let Some(next) = rewrite_sheet_names(text, change) {
        *text = next;
    }
}

/// Rewrites the sheet names in the operand formulas of `rules`; returns whether anything changed.
pub fn follow_sheet_change(rules: &mut SheetRules, change: SheetChange<'_>) -> bool {
    let before = rules.clone();
    rules
        .cf
        .iter_mut()
        .for_each(|r| cf_formulas(r, |text, _| follow(text, change)));
    rules
        .dv
        .iter_mut()
        .for_each(|r| dv_formulas(r, |text| follow(text, change)));
    *rules != before
}

#[cfg(test)]
mod tests {
    use super::*;
    use easyoffice_formats::{CellBox, CfKind, CfRule, CfStyle, Cmp, DataValidation, DvKind};

    fn range() -> CellBox {
        CellBox {
            row0: 0,
            col0: 0,
            row1: 4,
            col1: 0,
        }
    }

    fn cell_is(a: &str) -> CfRule {
        CfRule {
            ranges: vec![range()],
            priority: 1,
            stop_if_true: false,
            kind: CfKind::CellIs {
                cmp: Cmp::Greater,
                a: a.to_string(),
                b: String::new(),
            },
            style: CfStyle::default(),
        }
    }

    #[test]
    fn operand_formulas_follow_a_rename_and_a_deletion() {
        let mut list = DataValidation::new(vec![range()], DvKind::List);
        list.a = "=Old!$A$1:$A$9".to_string();
        let mut items = DataValidation::new(vec![range()], DvKind::List);
        items.a = "Old!A1,b".to_string();
        let mut rules = SheetRules {
            cf: vec![cell_is("=Old!B2*2"), cell_is("7")],
            dv: vec![list, items],
            ..SheetRules::default()
        };
        let rename = SheetChange::Rename {
            from: "Old",
            to: "새 이름",
        };
        assert!(follow_sheet_change(&mut rules, rename));
        assert!(matches!(&rules.cf[0].kind, CfKind::CellIs { a, .. } if a == "='새 이름'!B2*2"));
        assert!(matches!(&rules.cf[1].kind, CfKind::CellIs { a, .. } if a == "7"));
        assert_eq!(rules.dv[0].a, "='새 이름'!$A$1:$A$9");
        // A list's items are not references.
        assert_eq!(rules.dv[1].a, "Old!A1,b");
        assert!(!follow_sheet_change(&mut rules, rename));

        let gone = SheetChange::Delete { name: "새 이름" };
        assert!(follow_sheet_change(&mut rules, gone));
        assert!(matches!(&rules.cf[0].kind, CfKind::CellIs { a, .. } if a == "=#REF!*2"));
        assert_eq!(rules.dv[0].a, "=#REF!");
    }
}

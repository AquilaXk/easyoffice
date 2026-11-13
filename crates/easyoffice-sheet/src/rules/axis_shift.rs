//! Rules follow a row or column edit: the ranges they cover move, shrink, grow or vanish like
//! merged ranges do, and the formulas in their operands follow like any formula.
//!
//! 참조 구현: the span rule is Univer's (`packages/sheets/src/services/ref-range/util.ts`, see the
//! header of `axis/mod.rs`; Apache-2.0, Copyright 2023-present DreamNum Co., Ltd., commit
//! 5a06a7842c8e13fa0646d0733dc3662cf4b330b9). Applying it to the ranges of a rule, removing a rule
//! whose ranges all disappear and keeping the references of a first-row formula alive are this
//! repository's own.

use crate::axis::{rewrite_formula_refs, shift_boxes, Axis, AxisEdit, EditKind};
use easyoffice_formats::{CfKind, CfRule, CfValue, DataValidation, DvKind, SheetRules, ValueKind};

struct Follow<'a> {
    own: &'a str,
    target: &'a str,
    edit: &'a AxisEdit,
}

impl Follow<'_> {
    fn text(&self, text: &mut String, collapse: bool) {
        if let Some(next) = rewrite_formula_refs(text, self.own, self.target, self.edit, collapse) {
            *text = next;
        }
    }
}

/// Calls `f` with every formula text in the operands of a conditional formatting rule; the flag is
/// true for an expression rule's own formula (the others are values and thresholds).
pub(super) fn cf_formulas(rule: &mut CfRule, mut f: impl FnMut(&mut String, bool)) {
    let mut value = |v: &mut CfValue| {
        if v.kind == ValueKind::Formula {
            f(&mut v.text, false);
        }
    };
    match &mut rule.kind {
        CfKind::CellIs { a, b, .. } => {
            f(a, false);
            f(b, false);
        }
        CfKind::Expression(formula) => f(formula, true),
        CfKind::ColorScale(stops) => stops.iter_mut().for_each(|(v, _)| value(v)),
        CfKind::DataBar { min, max, .. } => {
            value(min);
            value(max);
        }
        CfKind::IconSet { thresholds, .. } => thresholds.iter_mut().for_each(value),
        _ => {}
    }
}

/// Calls `f` with the formula texts of a data validation's operands. A list is either its items
/// (`a,b,c`) or `=range`; items, and a check box's two values, are not references.
pub(super) fn dv_formulas(rule: &mut DataValidation, mut f: impl FnMut(&mut String)) {
    let literal =
        rule.kind == DvKind::Checkbox || (rule.kind.is_list() && !rule.a.starts_with('='));
    if literal {
        return;
    }
    f(&mut rule.a);
    f(&mut rule.b);
}

/// True when the rule's first range starts in the rows or columns the edit deletes: the cell its
/// formula is written for goes away, and the references in the formula keep pointing at whatever
/// takes that cell's place.
fn anchor_deleted(rule: &CfRule, edit: &AxisEdit) -> bool {
    let Some(first) = rule.ranges.first() else {
        return false;
    };
    let start = match edit.axis {
        Axis::Row => first.row0,
        Axis::Col => first.col0,
    };
    edit.kind == EditKind::Delete && (edit.at..edit.at + edit.count).contains(&start)
}

fn shift_cf(rule: &mut CfRule, own_is_target: bool, f: &Follow<'_>) {
    let collapse = own_is_target && anchor_deleted(rule, f.edit);
    if own_is_target {
        rule.ranges = shift_boxes(&rule.ranges, f.edit, false);
    }
    cf_formulas(rule, |text, expression| {
        f.text(text, expression && collapse)
    });
}

fn shift_dv(rule: &mut DataValidation, own_is_target: bool, f: &Follow<'_>) {
    if own_is_target {
        rule.ranges = shift_boxes(&rule.ranges, f.edit, false);
    }
    dv_formulas(rule, |text| f.text(text, false));
}

/// Moves the rules of the sheet `own` (`own_is_target`: it is the edited sheet `target`, so its
/// ranges move too) for `edit`. A rule that loses every range is removed. Returns whether
/// anything changed.
pub fn shift_rules(rules: &mut SheetRules, own: &str, target: &str, edit: &AxisEdit) -> bool {
    let before = rules.clone();
    let own_is_target = own.eq_ignore_ascii_case(target);
    let f = Follow { own, target, edit };
    rules
        .cf
        .iter_mut()
        .for_each(|r| shift_cf(r, own_is_target, &f));
    rules
        .dv
        .iter_mut()
        .for_each(|r| shift_dv(r, own_is_target, &f));
    if own_is_target {
        rules.cf.retain(|r| !r.ranges.is_empty());
        rules.dv.retain(|r| !r.ranges.is_empty());
    }
    *rules != before
}

#[cfg(test)]
mod tests {
    use super::*;
    use easyoffice_formats::{CellBox, CfStyle, Cmp};

    fn cell_is(range: CellBox, a: &str) -> CfRule {
        CfRule {
            ranges: vec![range],
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

    fn rows(r0: u32, r1: u32) -> CellBox {
        CellBox {
            row0: r0,
            col0: 0,
            row1: r1,
            col1: 1,
        }
    }

    fn edit(kind: EditKind, at: u32, count: u32) -> AxisEdit {
        AxisEdit::new(Axis::Row, kind, at, count).unwrap()
    }

    #[test]
    fn ranges_and_operands_follow_an_insertion() {
        let mut rules = SheetRules {
            cf: vec![cell_is(rows(1, 5), "=$A$9"), cell_is(rows(10, 12), "5")],
            ..SheetRules::default()
        };
        assert!(shift_rules(
            &mut rules,
            "S",
            "S",
            &edit(EditKind::Insert, 3, 2)
        ));
        assert_eq!(
            (rules.cf[0].ranges[0].row0, rules.cf[0].ranges[0].row1),
            (1, 7)
        );
        assert_eq!(
            (rules.cf[1].ranges[0].row0, rules.cf[1].ranges[0].row1),
            (12, 14)
        );
        assert!(matches!(&rules.cf[0].kind, CfKind::CellIs { a, .. } if a == "=$A$11"));
        assert!(!shift_rules(
            &mut rules,
            "S",
            "S",
            &edit(EditKind::Insert, 100, 1)
        ));
    }

    #[test]
    fn a_rule_with_no_range_left_is_removed_and_other_sheets_keep_theirs() {
        let mut rules = SheetRules {
            cf: vec![cell_is(rows(2, 3), "=S!A5+T!A5")],
            ..SheetRules::default()
        };
        let mut other = rules.clone();
        assert!(shift_rules(
            &mut other,
            "T",
            "S",
            &edit(EditKind::Delete, 0, 1)
        ));
        // T's rule covers T's cells: the range stays; only its reference to S moves.
        assert_eq!(other.cf[0].ranges, rules.cf[0].ranges);
        assert!(matches!(&other.cf[0].kind, CfKind::CellIs { a, .. } if a == "=S!A4+T!A5"));
        assert!(shift_rules(
            &mut rules,
            "S",
            "S",
            &edit(EditKind::Delete, 2, 2)
        ));
        assert!(rules.cf.is_empty());
    }

    #[test]
    fn an_expression_rule_survives_deleting_its_first_row() {
        let mut rules = SheetRules {
            cf: vec![CfRule {
                ranges: vec![rows(1, 9)],
                priority: 1,
                stop_if_true: false,
                kind: CfKind::Expression("=$A2>5".to_string()),
                style: CfStyle::default(),
            }],
            ..SheetRules::default()
        };
        shift_rules(&mut rules, "S", "S", &edit(EditKind::Delete, 1, 1));
        assert_eq!(
            (rules.cf[0].ranges[0].row0, rules.cf[0].ranges[0].row1),
            (1, 8)
        );
        assert!(matches!(&rules.cf[0].kind, CfKind::Expression(f) if f == "=$A2>5"));
    }

    #[test]
    fn list_items_are_not_references() {
        let mut dv = DataValidation::new(vec![rows(1, 3)], DvKind::List);
        dv.a = "A5,B6".to_string();
        let mut rules = SheetRules {
            dv: vec![dv.clone()],
            ..SheetRules::default()
        };
        shift_rules(&mut rules, "S", "S", &edit(EditKind::Insert, 0, 1));
        assert_eq!(rules.dv[0].a, "A5,B6");
        assert_eq!(rules.dv[0].ranges[0].row0, 2);
        let mut ranged = dv;
        ranged.a = "=$A$5:$A$9".to_string();
        let mut rules = SheetRules {
            dv: vec![ranged],
            ..SheetRules::default()
        };
        shift_rules(&mut rules, "S", "S", &edit(EditKind::Insert, 0, 1));
        assert_eq!(rules.dv[0].a, "=$A$6:$A$10");
    }
}

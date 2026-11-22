//! The conditional formats of a sheet on its PDF page (PD-300). The engine evaluates the rules (the evaluation the grid and
//! the print read: `WorkbookEngine::evaluate_rules_checked`) and the answer goes to the page as `CellRule`s beside the
//! number formats' `CellShown`, as the page's own `fx` goes to the print. The PDF draws; it evaluates nothing.

use std::collections::HashMap;

use easyoffice_formats::{
    plain_cell_numeric, CellRule, CellShown, SheetRules, SheetShown, SheetStyling,
};
use easyoffice_sheet::{CellVisual, WorkbookEngine};

/// The cells the PDF of a sheet can print: the print areas, else the box over the cells it displays. A rule answer
/// outside it is not kept, so a rule over a whole column does not make a map of a million cells.
fn printed_box(
    styling: &SheetStyling,
    displays: &[(u32, u32, String)],
) -> Option<(u32, u32, u32, u32)> {
    let merge = |a: (u32, u32, u32, u32), b: (u32, u32, u32, u32)| {
        (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))
    };
    let area = &styling.page.print_area;
    if area.is_empty() {
        displays.iter().map(|&(r, c, _)| (r, c, r, c)).reduce(merge)
    } else {
        area.iter()
            .map(|b| (b.row0, b.col0, b.row1, b.col1))
            .reduce(merge)
    }
}

/// What the rules give the cell, as the PDF draws it. A bar that hides the value hides it only when it is drawn and no
/// icon is: the PDF draws no icon sets, so a cell with one shows its value.
fn rule_of(visual: &CellVisual) -> CellRule {
    CellRule {
        bg: visual.bg,
        fg: visual.fg,
        flags: visual.flags,
        border: visual.border,
        bar: visual.bar,
        hide_value: visual.hide_value && visual.bar.is_some() && visual.icon.is_none(),
    }
}

/// Adds what the rules of sheet `name` give its cells to `shown`: the fill, text color, font bits, edges and data bar of
/// each cell inside the printed box. A rule that the time limit or the budget cut short is said in `warnings`, and
/// what it answered is drawn.
pub(crate) fn add_rule_looks(
    wb: &WorkbookEngine,
    name: &str,
    (styling, displays): (&SheetStyling, &[(u32, u32, String)]),
    shown: &mut SheetShown,
    warnings: &mut Vec<String>,
) {
    if styling.rules.cf.is_empty() {
        return;
    }
    let Some((row0, col0, row1, col1)) = printed_box(styling, displays) else {
        return;
    };
    let rules = SheetRules {
        cf: styling.rules.cf.clone(),
        today: styling.rules.today,
        ..SheetRules::default()
    };
    let (visuals, complete) = wb.evaluate_rules_checked(name, rules);
    if !complete {
        warnings.push(format!(
            "{name} 시트의 조건부 서식을 시간 안에 다 계산하지 못해 PDF에 일부만 그렸습니다."
        ));
    }
    let texts: HashMap<(u32, u32), &str> = displays
        .iter()
        .map(|(r, c, text)| ((*r, *c), text.as_str()))
        .collect();
    for visual in &visuals {
        let at = (visual.coord.row(), visual.coord.col());
        if at.0 < row0 || at.0 > row1 || at.1 < col0 || at.1 > col1 {
            continue;
        }
        let rule = rule_of(visual);
        shown
            .entry(at)
            .or_insert_with(|| CellShown {
                numeric: texts
                    .get(&at)
                    .is_some_and(|text| plain_cell_numeric(Some(styling), at, text)),
                ..CellShown::default()
            })
            .rule = Some(rule);
    }
}

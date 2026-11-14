//! Conditional formatting and data validation of a sheet (PD-171).
//!
//! [`RuleRuntime`] holds the rules of one sheet and, per rule, the cells it currently matches. A
//! full evaluation fills them; an update after an edit re-evaluates only what the edit can have
//! changed: the edited cells for rules that read nothing but the cell itself, the whole rule for
//! rules that read range statistics (the statistics are shared and recomputed once per range
//! list), and the whole rule for formula operands, which may read any cell. The result is the list
//! of cells whose merged appearance differs from the one last reported.

mod axis_shift;
mod cf;
pub(crate) mod data;
pub(crate) mod expr_shift;
mod pack;
mod period;
mod sheet_ref;
#[cfg(test)]
mod shift;
mod validate;

use crate::{CellMap, EvalContext, PackedCellCoordinate, WorkbookEngine};
use cf::{apply_plan, locality, make_plan, truthy, Hit, Locality, Plan};
use data::{prepare_formulas, StatsCache, BLANK_SCAN_CELLS};
pub(crate) use data::{Cells, SheetCells};
use easyoffice_formats::{CellBox, CfKind, CfRule, SheetRules};

pub use axis_shift::shift_rules;
pub use pack::{decode_visuals, encode_visuals, PackedVisuals};
pub use sheet_ref::follow_sheet_change;
pub use validate::{validate_input, Verdict};

pub use easyoffice_formats::DataValidation;

/// A data bar: `pct` is -100..=100 (negative grows left of the axis), `start` the axis position
/// in percent of the cell width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bar {
    pub pct: i8,
    pub start: u8,
    pub color: u32,
    pub gradient: bool,
}

/// An icon of an icon set (`easyoffice_formats::ICON_SETS` order); `index` 0 is the lowest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Icon {
    pub set: u8,
    pub index: u8,
}

/// The merged appearance conditional formatting gives one cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellVisual {
    pub coord: PackedCellCoordinate,
    pub bg: Option<u32>,
    pub fg: Option<u32>,
    pub flags: u8,
    pub bar: Option<Bar>,
    pub icon: Option<Icon>,
    /// The cell's value is hidden behind a bar or an icon.
    pub hide_value: bool,
}

impl CellVisual {
    fn blank(coord: PackedCellCoordinate) -> Self {
        Self {
            coord,
            bg: None,
            fg: None,
            flags: 0,
            bar: None,
            icon: None,
            hide_value: false,
        }
    }

    /// Nothing to show: the cell looks as it does without rules.
    pub fn is_none(&self) -> bool {
        self.bg.is_none()
            && self.fg.is_none()
            && self.flags == 0
            && self.bar.is_none()
            && self.icon.is_none()
    }
}

struct RuleState {
    locality: Locality,
    needs_blanks: bool,
    /// True when the last evaluation stopped early on deadline or cumulative step budget.
    incomplete: bool,
    /// Plan of a rule that reads nothing but the cell (built at the first evaluation).
    plan: Option<Plan>,
    hits: CellMap<Hit>,
}

/// Cells all rules of one sheet may match at once (each match costs one map entry).
pub const MAX_RULE_HITS: usize = 4_000_000;

pub struct RuleRuntime {
    rules: Vec<CfRule>,
    /// The serial of the day period rules count from.
    today: i64,
    states: Vec<RuleState>,
    emitted: CellMap<CellVisual>,
    stats: StatsCache,
    pub validations: Vec<DataValidation>,
    max_hits: usize,
    /// The rules matched more than `max_hits` cells; the runtime holds nothing and does nothing.
    over_budget: bool,
}

fn needs_blanks(kind: &CfKind) -> bool {
    matches!(
        kind,
        CfKind::CellIs { .. }
            | CfKind::Text { .. }
            | CfKind::Blanks { .. }
            | CfKind::Errors { .. }
            | CfKind::Expression(_)
    )
}

fn shows_value(kind: &CfKind) -> bool {
    match kind {
        CfKind::DataBar { show_value, .. } | CfKind::IconSet { show_value, .. } => *show_value,
        _ => true,
    }
}

fn expression_anchor(rule: &CfRule) -> (u32, u32) {
    rule.ranges
        .iter()
        .map(|b| (b.row0, b.col0))
        .min()
        .unwrap_or((0, 0))
}

impl RuleRuntime {
    pub fn new(rules: SheetRules) -> Self {
        Self::with_max_hits(rules, MAX_RULE_HITS)
    }

    pub fn with_max_hits(rules: SheetRules, max_hits: usize) -> Self {
        let today = rules.today.unwrap_or_else(period::utc_today);
        let mut cf = rules.cf;
        cf.sort_by_key(|r| r.priority);
        let states = cf
            .iter()
            .map(|r| RuleState {
                locality: locality(&r.kind),
                needs_blanks: needs_blanks(&r.kind),
                incomplete: false,
                plan: None,
                hits: CellMap::default(),
            })
            .collect();
        Self {
            rules: cf,
            today,
            states,
            emitted: CellMap::default(),
            stats: StatsCache::default(),
            validations: rules.dv,
            max_hits,
            over_budget: false,
        }
    }

    /// Estimated heap bytes of the matched cells of every rule, the merged appearance and the
    /// cached range values (string payloads of cached values are not counted). Map slots are
    /// counted at their capacity, one control byte each, like the graph estimate counts its own.
    pub fn heap_bytes_estimate(&self) -> usize {
        fn map_bytes<V>(map: &CellMap<V>) -> usize {
            map.capacity() * (std::mem::size_of::<(PackedCellCoordinate, V)>() + 1)
        }
        self.states
            .iter()
            .map(|state| map_bytes(&state.hits))
            .sum::<usize>()
            + map_bytes(&self.emitted)
            + self.stats.heap_bytes_estimate()
    }

    /// True once the rules matched more cells than the budget allows.
    pub fn is_over_budget(&self) -> bool {
        self.over_budget
    }

    fn enforce_budget(&mut self) {
        let total: usize = self.states.iter().map(|s| s.hits.len()).sum();
        if total > self.max_hits {
            self.over_budget = true;
            for s in &mut self.states {
                s.hits = CellMap::default();
            }
            self.emitted.clear();
            self.stats.clear();
        }
    }

    pub fn has_formatting(&self) -> bool {
        !self.rules.is_empty()
    }

    fn compute_whole(&mut self, i: usize, cells: &mut dyn Cells) {
        self.states[i].incomplete = false;
        if cells.budget_exhausted() {
            self.states[i].incomplete = true;
            return;
        }
        let rule = &self.rules[i];
        let (data, complete) = self
            .stats
            .get(cells, &rule.ranges, self.states[i].needs_blanks);
        if !complete || cells.budget_exhausted() {
            self.states[i].incomplete = true;
            return;
        }
        let mut hits = CellMap::default();
        if let CfKind::Expression(formula) = &rule.kind {
            let (ar, ac) = expression_anchor(rule);
            let template = cells.compile_rule_expr(formula);
            let mut scratch = template.init_scratch();
            for (coord, _) in &data.cells {
                if cells.budget_exhausted() {
                    self.states[i].incomplete = true;
                    break;
                }
                let dr = i64::from(coord.row()) - i64::from(ar);
                let dc = i64::from(coord.col()) - i64::from(ac);
                let matched = template.eval_in_place(cells, &mut scratch, dr, dc);
                if cells.budget_exhausted() {
                    self.states[i].incomplete = true;
                    break;
                }
                if matched {
                    hits.insert(*coord, Hit::Style);
                }
            }
        } else if let Some(plan) = make_plan(rule, &data, cells, self.today) {
            if cells.budget_exhausted() {
                self.states[i].incomplete = true;
                return;
            }
            for (idx, (coord, value)) in data.cells.iter().enumerate() {
                if (idx & 1023) == 1023 {
                    cells.tick_rule_steps(1024);
                    if cells.budget_exhausted() {
                        self.states[i].incomplete = true;
                        break;
                    }
                }
                if let Some(hit) = apply_plan(&plan, value) {
                    hits.insert(*coord, hit);
                }
            }
            if self.states[i].locality == Locality::Cell {
                self.states[i].plan = Some(plan);
            }
        } else if cells.budget_exhausted() {
            self.states[i].incomplete = true;
            return;
        }
        self.states[i].hits = hits;
        self.enforce_budget();
    }

    /// Re-judges one edited cell for a rule that reads nothing but the cell.
    fn update_cell(&mut self, i: usize, coord: PackedCellCoordinate, cells: &mut dyn Cells) {
        let rule = &self.rules[i];
        let state = &mut self.states[i];
        let Some(box_) = rule
            .ranges
            .iter()
            .find(|b| b.contains(coord.row(), coord.col()))
        else {
            return;
        };
        let value = cells.value(coord);
        let scanned = state.needs_blanks && box_.area() <= BLANK_SCAN_CELLS;
        let hit = match (
            &state.plan,
            matches!(value, crate::EvaluatedCellValue::Empty) && !scanned,
        ) {
            (Some(plan), false) => apply_plan(plan, &value),
            _ => None,
        };
        match hit {
            Some(h) => {
                state.hits.insert(coord, h);
            }
            None => {
                state.hits.remove(&coord);
            }
        }
    }

    fn merge(&self, coord: PackedCellCoordinate) -> CellVisual {
        let mut v = CellVisual::blank(coord);
        for (rule, state) in self.rules.iter().zip(&self.states) {
            let Some(hit) = state.hits.get(&coord) else {
                continue;
            };
            match hit {
                Hit::Style => {
                    v.bg = v.bg.or(rule.style.bg);
                    v.fg = v.fg.or(rule.style.fg);
                    v.flags |= rule.style.flags;
                }
                Hit::Color(c) => v.bg = v.bg.or(Some(*c)),
                Hit::Bar(b) if v.bar.is_none() => {
                    v.bar = Some(*b);
                    v.hide_value |= !shows_value(&rule.kind);
                }
                Hit::Icon(i) if v.icon.is_none() => {
                    v.icon = Some(*i);
                    v.hide_value |= !shows_value(&rule.kind);
                }
                _ => {}
            }
            if rule.stop_if_true {
                break;
            }
        }
        v
    }

    /// Every cell the rules currently format.
    pub(crate) fn evaluate(&mut self, cells: &mut dyn Cells) -> Vec<CellVisual> {
        self.stats.clear();
        for i in 0..self.rules.len() {
            if cells.budget_exhausted() {
                for state in &mut self.states[i..] {
                    state.incomplete = true;
                }
                break;
            }
            self.compute_whole(i, cells);
            if self.over_budget {
                return Vec::new();
            }
            if cells.budget_exhausted() {
                for state in &mut self.states[i + 1..] {
                    state.incomplete = true;
                }
                break;
            }
        }
        let mut coords: Vec<PackedCellCoordinate> = self
            .states
            .iter()
            .flat_map(|s| s.hits.keys().copied())
            .collect();
        coords.sort_unstable();
        coords.dedup();
        self.emitted.clear();
        let out: Vec<CellVisual> = coords
            .into_iter()
            .map(|c| self.merge(c))
            .filter(|v| !v.is_none())
            .collect();
        self.emitted = out.iter().map(|v| (v.coord, *v)).collect();
        out
    }

    /// Cells whose appearance changed after `touched` cells (edited, or recalculated to a new
    /// value) changed in this sheet; `any_changed` is true when a cell of any sheet changed. A
    /// cell that lost its formatting comes back as a visual with [`CellVisual::is_none`].
    pub(crate) fn update(
        &mut self,
        cells: &mut dyn Cells,
        touched: &[PackedCellCoordinate],
        any_changed: bool,
    ) -> Vec<CellVisual> {
        let any_incomplete = self.states.iter().any(|s| s.incomplete);
        if self.rules.is_empty()
            || self.over_budget
            || (touched.is_empty() && !any_changed && !any_incomplete)
        {
            return Vec::new();
        }
        self.stats.invalidate(touched);
        let mut dirty: Vec<PackedCellCoordinate> = Vec::new();
        for i in 0..self.rules.len() {
            let inside: Vec<PackedCellCoordinate> = touched
                .iter()
                .copied()
                .filter(|c| {
                    self.rules[i]
                        .ranges
                        .iter()
                        .any(|b| b.contains(c.row(), c.col()))
                })
                .collect();
            let affected = match self.states[i].locality {
                Locality::Sheet => any_changed || !touched.is_empty(),
                Locality::Cell | Locality::Range => !inside.is_empty(),
            };
            if cells.budget_exhausted() {
                if self.states[i].incomplete || affected {
                    self.states[i].incomplete = true;
                }
                for j in (i + 1)..self.rules.len() {
                    let rule_j_affected = match self.states[j].locality {
                        Locality::Sheet => any_changed || !touched.is_empty(),
                        Locality::Cell | Locality::Range => touched.iter().any(|c| {
                            self.rules[j]
                                .ranges
                                .iter()
                                .any(|b| b.contains(c.row(), c.col()))
                        }),
                    };
                    if rule_j_affected {
                        self.states[j].incomplete = true;
                    }
                }
                break;
            }
            if self.states[i].incomplete || self.states[i].locality != Locality::Cell {
                if self.states[i].incomplete || affected {
                    let before = std::mem::take(&mut self.states[i].hits);
                    self.compute_whole(i, cells);
                    if self.over_budget {
                        return Vec::new();
                    }
                    let after = &self.states[i].hits;
                    dirty.extend(
                        before
                            .iter()
                            .filter(|(k, h)| after.get(k) != Some(h))
                            .map(|(k, _)| *k),
                    );
                    dirty.extend(
                        after
                            .iter()
                            .filter(|(k, h)| before.get(k) != Some(h))
                            .map(|(k, _)| *k),
                    );
                }
            } else {
                for c in inside {
                    if cells.budget_exhausted() {
                        self.states[i].incomplete = true;
                        break;
                    }
                    self.update_cell(i, c, cells);
                    dirty.push(c);
                    if cells.budget_exhausted() {
                        self.states[i].incomplete = true;
                        break;
                    }
                }
                self.enforce_budget();
                if self.over_budget {
                    return Vec::new();
                }
            }
        }
        dirty.sort_unstable();
        dirty.dedup();
        let mut out = Vec::new();
        for coord in dirty {
            let now = self.merge(coord);
            let was = self.emitted.get(&coord);
            if now.is_none() {
                if was.is_some() {
                    self.emitted.remove(&coord);
                    out.push(now);
                }
            } else if was != Some(&now) {
                self.emitted.insert(coord, now);
                out.push(now);
            }
        }
        out
    }

    /// Validation rule that covers `coord`.
    pub fn validation_at(&self, coord: PackedCellCoordinate) -> Option<&DataValidation> {
        self.validations.iter().find(|dv| {
            dv.ranges
                .iter()
                .any(|b| b.contains(coord.row(), coord.col()))
        })
    }
}

impl WorkbookEngine {
    /// Appearance every conditional formatting rule of `sheet` gives its cells, evaluated on a
    /// fresh pass over the workbook (the stateless path).
    pub fn evaluate_rules(&self, sheet: &str, rules: SheetRules) -> Vec<CellVisual> {
        let Some(engine) = self.sheet(sheet) else {
            return Vec::new();
        };
        let mut runtime = RuleRuntime::new(rules);
        let boxes: Vec<CellBox> = runtime
            .rules
            .iter()
            .flat_map(|r| r.ranges.iter().copied())
            .collect();
        let mut ctx = EvalContext::with_workbook(self, engine.resolved_sheet_id())
            .with_time_limit(crate::MAX_EVAL_TIME);
        prepare_formulas(engine, &boxes, &mut ctx);
        ctx.reset_steps();
        let mut cells = SheetCells {
            sheet: engine,
            ctx: &mut ctx,
        };
        runtime.evaluate(&mut cells)
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_expr_shift;
#[cfg(test)]
mod tests_validate;

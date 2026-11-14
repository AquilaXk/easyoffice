//! Conditional formatting rule evaluation.
//!
//! Reference implementation: the rule semantics (comparison operators, top/bottom ranks, average,
//! duplicate and unique values, colour scale interpolation, data bar geometry, icon set
//! thresholds) are ported function by function from the Univer sheets conditional formatting
//! package (`packages/sheets-conditional-formatting/src/models/calculate-unit-v2/*.ts`, commit
//! a57b187e4540003489555ffca5774c2735006c19), Apache-2.0, Copyright 2023-present DreamNum Co.,
//! Ltd. See `THIRD_PARTY_NOTICES.md`.
//!
//! Differences from the original: text rules ignore case (as the file formats define them), the
//! maximum of an all-negative range is its real maximum (the original starts from 0), equality
//! uses a relative tolerance, icon sets follow the file format (`>=` thresholds, lowest icon
//! first), and the average rule supports standard deviations.
//!
//! A rule is turned into a [`Plan`] (everything that depends on the range as a whole: the rank
//! cut-off, the average, the duplicate counts, the resolved colour stops) once, and a plan is
//! then applied to cell values one at a time.

use super::data::{Cells, RangeData};
use super::period::period_days;
use super::{Bar, Icon};
use crate::{EvaluatedCellValue, FormulaError};
use easyoffice_formats::{CfKind, CfRule, CfValue, Cmp, TextOp, ValueKind, ICON_SETS};
use std::collections::HashMap;

/// What a matching cell shows beyond the rule's style.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Hit {
    Style,
    Color(u32),
    Bar(Bar),
    Icon(Icon),
}

/// How much of the sheet a rule's result depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Locality {
    /// Only the cell's own value and constants.
    Cell,
    /// Statistics of the rule's ranges.
    Range,
    /// Cells anywhere (formula operands, expressions).
    Sheet,
}

fn is_literal(text: &str) -> bool {
    let t = text.trim();
    t.parse::<f64>().is_ok() || (t.len() >= 2 && t.starts_with('"') && t.ends_with('"'))
}

fn all_numbers(values: &[&CfValue]) -> bool {
    values.iter().all(|v| v.kind == ValueKind::Num)
}

fn any_formula(values: &[&CfValue]) -> bool {
    values.iter().any(|v| v.kind == ValueKind::Formula)
}

pub(crate) fn locality(kind: &CfKind) -> Locality {
    let graded = |values: &[&CfValue]| {
        if any_formula(values) {
            Locality::Sheet
        } else if all_numbers(values) {
            Locality::Cell
        } else {
            Locality::Range
        }
    };
    match kind {
        CfKind::CellIs { cmp, a, b } => {
            if is_literal(a) && (!cmp.takes_two() || is_literal(b)) {
                Locality::Cell
            } else {
                Locality::Sheet
            }
        }
        CfKind::Text { .. }
        | CfKind::Blanks { .. }
        | CfKind::Errors { .. }
        | CfKind::TimePeriod(_) => Locality::Cell,
        CfKind::Top { .. } | CfKind::Average { .. } | CfKind::Duplicates { .. } => Locality::Range,
        CfKind::Expression(_) => Locality::Sheet,
        CfKind::ColorScale(stops) => graded(&stops.iter().map(|(v, _)| v).collect::<Vec<_>>()),
        CfKind::DataBar { min, max, .. } => graded(&[min, max]),
        CfKind::IconSet { thresholds, .. } => graded(&thresholds.iter().collect::<Vec<_>>()),
    }
}

/// A comparison operand of a "cell value" rule.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Operand {
    Num(f64),
    Text(String),
}

fn operand(text: &str, cells: &mut dyn Cells) -> Operand {
    let expr = text.trim().trim_start_matches('=');
    if let Ok(n) = expr.parse::<f64>() {
        return Operand::Num(n);
    }
    match cells.eval_formula(expr) {
        EvaluatedCellValue::Number(n) => Operand::Num(n),
        EvaluatedCellValue::Boolean(b) => Operand::Num(f64::from(u8::from(b))),
        EvaluatedCellValue::Empty => Operand::Num(0.0),
        EvaluatedCellValue::Text(t) => Operand::Text(t),
        EvaluatedCellValue::Error(_) => Operand::Text(expr.to_string()),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub(crate) enum DupKey {
    Num(u64),
    Bool(bool),
    Error(FormulaError),
    Text(String),
}

fn dup_key(value: &EvaluatedCellValue) -> Option<DupKey> {
    Some(match value {
        EvaluatedCellValue::Empty => return None,
        EvaluatedCellValue::Number(n) => DupKey::Num(if *n == 0.0 { 0.0 } else { *n }.to_bits()),
        EvaluatedCellValue::Boolean(b) => DupKey::Bool(*b),
        EvaluatedCellValue::Error(e) => DupKey::Error(*e),
        EvaluatedCellValue::Text(t) if t.trim().is_empty() => return None,
        EvaluatedCellValue::Text(t) => DupKey::Text(fold(t)),
    })
}

fn fold(text: &str) -> String {
    if text.is_ascii() {
        text.to_ascii_lowercase()
    } else {
        text.to_lowercase()
    }
}

/// Everything a rule needs to judge a cell value.
pub(crate) enum Plan {
    CellIs {
        cmp: Cmp,
        a: Operand,
        b: Operand,
    },
    Text {
        op: TextOp,
        needle: String,
    },
    Blanks(bool),
    Errors(bool),
    /// First and last serial day of a period.
    Period(i64, i64),
    Rank {
        cut: f64,
        bottom: bool,
    },
    Average {
        line: f64,
        above: bool,
        equal: bool,
    },
    Duplicates {
        counts: HashMap<DupKey, u32>,
        unique: bool,
    },
    Scale(Vec<(f64, u32)>),
    Bar(BarPlan),
    Icons {
        set: u8,
        /// `(threshold, gte)` per icon, lowest first.
        thresholds: Vec<(f64, bool)>,
        reverse: bool,
    },
}

pub(crate) struct BarPlan {
    min: f64,
    max: f64,
    start: u8,
    positive: u32,
    negative: u32,
    gradient: bool,
}

fn parse_percent(v: &CfValue) -> f64 {
    v.text
        .trim()
        .parse::<f64>()
        .unwrap_or(0.0)
        .clamp(0.0, 100.0)
}

/// Port of `getValueByType`.
fn resolve(v: &CfValue, data: &RangeData, cells: &mut dyn Cells) -> Option<f64> {
    match v.kind {
        ValueKind::Num => Some(
            v.text
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite())
                .unwrap_or(0.0),
        ),
        ValueKind::Min => (data.count() > 0).then_some(data.min),
        ValueKind::Max => (data.count() > 0).then_some(data.max),
        ValueKind::Percent => {
            (data.count() > 0).then(|| data.min + (data.max - data.min) * parse_percent(v) / 100.0)
        }
        ValueKind::Percentile => data.percentile(parse_percent(v)),
        ValueKind::Formula => cells
            .eval_formula(v.text.trim().trim_start_matches('='))
            .as_number()
            .filter(|n| n.is_finite()),
    }
}

/// Port of the rank pre-computation: the value a cell must reach to be among the top (or bottom)
/// `n` values or percent.
fn rank_cut(n: u32, percent: bool, bottom: bool, data: &RangeData) -> Option<f64> {
    let ascending = data.sorted();
    let len = ascending.len();
    if len == 0 {
        return None;
    }
    // Descending, as the original sorts; `at(i)` is the i-th largest.
    let at = |i: usize| ascending[len - 1 - i];
    if percent {
        let threshold = f64::from(n.min(100)) / 100.0;
        let target = (threshold * len as f64).floor() as usize;
        let safe = target.saturating_sub(1).min(len - 1);
        // Bottom percent reads the reversed list, so the index counts from the smallest.
        return Some(if bottom { ascending[safe] } else { at(safe) });
    }
    let n = n as usize;
    let rank = n.min(len).saturating_sub(1);
    Some(if bottom { ascending[rank] } else { at(rank) })
}

/// Port of the data bar pre-computation: the range and where the zero axis sits (percent).
fn bar_plan(
    min: &CfValue,
    max: &CfValue,
    positive: u32,
    negative: u32,
    gradient: bool,
    data: &RangeData,
    cells: &mut dyn Cells,
) -> Option<BarPlan> {
    let lo = resolve(min, data, cells)?;
    let hi = resolve(max, data, cells)?;
    let (max, min) = (hi.max(lo), hi.min(lo));
    let start = if min < 0.0 && max <= 0.0 {
        100.0
    } else if min < 0.0 && max > 0.0 {
        min.abs() / (max.abs() + min.abs()) * 100.0
    } else if min >= 0.0 && max > 0.0 {
        0.0
    } else {
        50.0
    };
    Some(BarPlan {
        min,
        max,
        start: start.round().clamp(0.0, 100.0) as u8,
        positive,
        negative,
        gradient,
    })
}

/// Builds the plan of a rule over `data`.
pub(crate) fn make_plan(
    rule: &CfRule,
    data: &RangeData,
    cells: &mut dyn Cells,
    today: i64,
) -> Option<Plan> {
    Some(match &rule.kind {
        CfKind::CellIs { cmp, a, b } => Plan::CellIs {
            cmp: *cmp,
            a: operand(a, cells),
            b: operand(b, cells),
        },
        CfKind::Text { op, text } => Plan::Text {
            op: *op,
            needle: fold(text),
        },
        CfKind::Blanks { not } => Plan::Blanks(*not),
        CfKind::Errors { not } => Plan::Errors(*not),
        CfKind::TimePeriod(period) => {
            let (first, last) = period_days(*period, today);
            Plan::Period(first, last)
        }
        CfKind::Top { n, percent, bottom } => Plan::Rank {
            cut: rank_cut(*n, *percent, *bottom, data)?,
            bottom: *bottom,
        },
        CfKind::Average {
            above,
            equal,
            std_dev,
        } => {
            let mean = data.average()?;
            let spread = if *std_dev > 0 {
                data.std_dev()? * f64::from(*std_dev)
            } else {
                0.0
            };
            Plan::Average {
                line: if *above { mean + spread } else { mean - spread },
                above: *above,
                equal: *equal,
            }
        }
        CfKind::Duplicates { unique } => {
            let mut counts: HashMap<DupKey, u32> = HashMap::with_capacity(data.cells.len());
            for (_, v) in &data.cells {
                if let Some(key) = dup_key(v) {
                    *counts.entry(key).or_insert(0) += 1;
                }
            }
            Plan::Duplicates {
                counts,
                unique: *unique,
            }
        }
        CfKind::Expression(_) => return None,
        CfKind::ColorScale(stops) => {
            let mut values = Vec::with_capacity(stops.len());
            for (v, _) in stops {
                values.push(resolve(v, data, cells)?);
            }
            values.sort_unstable_by(f64::total_cmp);
            Plan::Scale(
                values
                    .into_iter()
                    .zip(stops.iter().map(|(_, color)| *color))
                    .collect(),
            )
        }
        CfKind::DataBar {
            min,
            max,
            positive,
            negative,
            gradient,
            ..
        } => Plan::Bar(bar_plan(
            min, max, *positive, *negative, *gradient, data, cells,
        )?),
        CfKind::IconSet {
            set,
            thresholds,
            reverse,
            ..
        } => {
            let mut resolved = Vec::with_capacity(thresholds.len());
            for t in thresholds {
                resolved.push((resolve(t, data, cells)?, t.gte));
            }
            Plan::Icons {
                set: *set,
                thresholds: resolved,
                reverse: *reverse,
            }
        }
    })
}

/// `a == b` within a relative tolerance (the original compares with `Number.EPSILON`).
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-12 * a.abs().max(b.abs()).max(1.0)
}

/// Port of `compareWithNumber`.
fn compare_number(cmp: Cmp, v: f64, a: f64, b: f64) -> bool {
    match cmp {
        Cmp::Between => v >= a.min(b) && v <= a.max(b),
        Cmp::NotBetween => !(v >= a.min(b) && v <= a.max(b)),
        Cmp::Equal => near(v, a),
        Cmp::NotEqual => !near(v, a),
        Cmp::Greater => v > a,
        Cmp::GreaterEqual => v >= a,
        Cmp::Less => v < a,
        Cmp::LessEqual => v <= a,
    }
}

fn compare_text(cmp: Cmp, v: &str, a: &str, b: &str) -> bool {
    let (v, a, b) = (fold(v), fold(a), fold(b));
    match cmp {
        Cmp::Between => v >= a.clone().min(b.clone()) && v <= a.max(b),
        Cmp::NotBetween => !(v >= a.clone().min(b.clone()) && v <= a.max(b)),
        Cmp::Equal => v == a,
        Cmp::NotEqual => v != a,
        Cmp::Greater => v > a,
        Cmp::GreaterEqual => v >= a,
        Cmp::Less => v < a,
        Cmp::LessEqual => v <= a,
    }
}

fn cell_is(cmp: Cmp, a: &Operand, b: &Operand, value: &EvaluatedCellValue) -> bool {
    // A value that is not a number sorts above every number (as spreadsheet programs do), which the original encodes
    // as "true for these operators".
    let above_numbers = || {
        matches!(
            cmp,
            Cmp::NotEqual | Cmp::NotBetween | Cmp::Greater | Cmp::GreaterEqual
        )
    };
    let num = |o: &Operand| {
        if let Operand::Num(n) = o {
            Some(*n)
        } else {
            None
        }
    };
    match value {
        EvaluatedCellValue::Error(_) => false,
        EvaluatedCellValue::Boolean(_) => above_numbers(),
        EvaluatedCellValue::Text(t) => match (a, b) {
            (Operand::Text(x), Operand::Text(y)) => compare_text(cmp, t, x, y),
            (Operand::Text(x), _) if !cmp.takes_two() => compare_text(cmp, t, x, ""),
            _ => above_numbers(),
        },
        other => {
            let v = other.as_number().unwrap_or(0.0);
            match (num(a), num(b)) {
                (Some(x), Some(y)) => compare_number(cmp, v, x, y),
                (Some(x), None) if !cmp.takes_two() => compare_number(cmp, v, x, 0.0),
                // A text operand: numbers sort below text.
                _ => matches!(cmp, Cmp::NotEqual | Cmp::Less | Cmp::LessEqual),
            }
        }
    }
}

fn text_of(value: &EvaluatedCellValue) -> String {
    match value {
        EvaluatedCellValue::Empty => String::new(),
        other => other.to_display_string(),
    }
}

fn text_rule(op: TextOp, needle: &str, value: &EvaluatedCellValue) -> bool {
    let hay = fold(&text_of(value));
    match op {
        TextOp::Contains => hay.contains(needle),
        TextOp::NotContains => !hay.contains(needle),
        TextOp::Begins => hay.starts_with(needle),
        TextOp::Ends => hay.ends_with(needle),
    }
}

fn lerp_color(a: (f64, u32), b: (f64, u32), v: f64) -> u32 {
    let t = (v - a.0) / (b.0 - a.0);
    let mut out = 0u32;
    for shift in [16u32, 8, 0] {
        let lo = f64::from((a.1 >> shift) & 0xFF);
        let hi = f64::from((b.1 >> shift) & 0xFF);
        out |= ((lo + (hi - lo) * t).round().clamp(0.0, 255.0) as u32) << shift;
    }
    out
}

/// Port of `getColorScaleFromValue`.
fn scale_color(stops: &[(f64, u32)], v: f64) -> u32 {
    match stops.iter().position(|s| s.0 >= v) {
        Some(0) => stops[0].1,
        Some(i) => lerp_color(stops[i - 1], stops[i], v),
        None => stops[stops.len() - 1].1,
    }
}

/// Port of the data bar `getCellResult`.
fn bar_hit(plan: &BarPlan, v: f64) -> Option<Hit> {
    let BarPlan {
        min,
        max,
        start,
        positive,
        negative,
        gradient,
    } = *plan;
    if v < min || min == max || max < min {
        return None;
    }
    let safe = |p: f64| p.clamp(0.0, 100.0);
    let bar = |pct: f64, color: u32| {
        let pct = pct.round() as i8;
        (pct != 0).then_some(Hit::Bar(Bar {
            pct,
            start,
            color,
            gradient,
        }))
    };
    if v == 0.0 {
        return None;
    }
    if min < 0.0 && max <= 0.0 {
        bar(-safe((max - v) / (max - min) * 100.0), negative)
    } else if min < 0.0 && max > 0.0 {
        if v > 0.0 {
            bar(safe((v / max).min(1.0) * 100.0), positive)
        } else {
            bar(-safe((v.abs() / min.abs()).min(1.0) * 100.0), negative)
        }
    } else if min >= 0.0 && max > 0.0 {
        bar(safe((1.0 - (max - v) / (max - min)) * 100.0), positive)
    } else {
        None
    }
}

fn icon_hit(set: u8, thresholds: &[(f64, bool)], reverse: bool, v: f64) -> Hit {
    let count = thresholds.len();
    // Highest icon whose threshold the value meets; the lowest icon always applies.
    let index = (1..count)
        .rev()
        .find(|&i| {
            let (t, gte) = thresholds[i];
            if gte {
                v >= t
            } else {
                v > t
            }
        })
        .unwrap_or(0);
    let shown = if reverse { count - 1 - index } else { index };
    debug_assert!(count == usize::from(ICON_SETS[usize::from(set)].1));
    Hit::Icon(Icon {
        set,
        index: shown as u8,
    })
}

/// Judges one cell value against a plan.
pub(crate) fn apply_plan(plan: &Plan, value: &EvaluatedCellValue) -> Option<Hit> {
    match plan {
        Plan::CellIs { cmp, a, b } => cell_is(*cmp, a, b, value).then_some(Hit::Style),
        Plan::Text { op, needle } => text_rule(*op, needle, value).then_some(Hit::Style),
        Plan::Blanks(not) => {
            let blank = match value {
                EvaluatedCellValue::Empty => true,
                EvaluatedCellValue::Text(t) => t.trim().is_empty(),
                _ => false,
            };
            (blank != *not).then_some(Hit::Style)
        }
        Plan::Errors(not) => {
            (matches!(value, EvaluatedCellValue::Error(_)) != *not).then_some(Hit::Style)
        }
        Plan::Period(first, last) => {
            // Only numbers are dates; the time of day does not matter.
            let EvaluatedCellValue::Number(v) = value else {
                return None;
            };
            let day = v.floor();
            (day >= *first as f64 && day <= *last as f64).then_some(Hit::Style)
        }
        Plan::Rank { cut, bottom } => {
            let EvaluatedCellValue::Number(v) = value else {
                return None;
            };
            (if *bottom { v <= cut } else { v >= cut }).then_some(Hit::Style)
        }
        Plan::Average { line, above, equal } => {
            let EvaluatedCellValue::Number(v) = value else {
                return None;
            };
            let hit = match (above, equal) {
                (true, true) => v >= line,
                (true, false) => v > line,
                (false, true) => v <= line,
                (false, false) => v < line,
            };
            hit.then_some(Hit::Style)
        }
        Plan::Duplicates { counts, unique } => {
            let count = *counts.get(&dup_key(value)?)?;
            (if *unique { count == 1 } else { count > 1 }).then_some(Hit::Style)
        }
        Plan::Scale(stops) => {
            let EvaluatedCellValue::Number(v) = value else {
                return None;
            };
            Some(Hit::Color(scale_color(stops, *v)))
        }
        Plan::Bar(bar) => {
            let EvaluatedCellValue::Number(v) = value else {
                return None;
            };
            bar_hit(bar, *v)
        }
        Plan::Icons {
            set,
            thresholds,
            reverse,
        } => {
            let EvaluatedCellValue::Number(v) = value else {
                return None;
            };
            Some(icon_hit(*set, thresholds, *reverse, *v))
        }
    }
}

/// Truth of an expression rule's result (a number other than zero counts, as in the file formats).
pub(crate) fn truthy(value: &EvaluatedCellValue) -> bool {
    match value {
        EvaluatedCellValue::Boolean(b) => *b,
        EvaluatedCellValue::Number(n) => *n != 0.0,
        _ => false,
    }
}

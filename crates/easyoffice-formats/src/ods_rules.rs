//! OpenDocument conditional formatting (`calcext:conditional-formats`, the spreadsheet extension
//! namespace) and data validation (`table:content-validations`) for [`SheetRules`] (PD-171).
//!
//! A validation is attached to cells (`table:content-validation-name`), so a validated range is
//! written as cells: every populated cell inside it, plus empty cells while the range is small
//! ([`BLANK_CELL_CAP`] per sheet). Reading folds the cells back into rectangles. Conditions the
//! reader does not recognise are named in [`SheetRules::unsupported`].

use std::collections::HashMap;
use std::fmt::Write;

use crate::sheet_styling::sheet_rules::{
    CellBox, CfKind, CfRule, CfStyle, CfValue, Cmp, DataValidation, DvKind, SheetRules, TextOp,
    ValueKind, FLAG_BOLD, FLAG_ITALIC, FLAG_STRIKE, FLAG_UNDERLINE, ICON_SETS,
};
use crate::sheet_styling::SheetStyling;
use crate::xlsx_rules::period_formula;
use crate::{OdfCodec, XmlSanitizer};

/// Empty cells per sheet written only to carry a validation.
pub(crate) const BLANK_CELL_CAP: u64 = 20_000;

pub(crate) const CALCEXT_NS: &str =
    "urn:org:documentfoundation:names:experimental:calc:xmlns:calcext:1.0";

/// Names of the rule styles and validations of a workbook.
pub(crate) struct OdsRuleTable {
    styles: Vec<CfStyle>,
    style_index: HashMap<CfStyle, usize>,
    /// `(sheet, index into that sheet's dv list)` in the order of the `eoval<N>` names.
    validations: Vec<(usize, usize)>,
}

fn quote_sheet(name: &str) -> String {
    if name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        name.to_string()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

fn address(sheet: &str, b: &CellBox) -> String {
    let s = quote_sheet(sheet);
    let a = CellBox::cell(b.row0, b.col0).to_a1();
    let z = CellBox::cell(b.row1, b.col1).to_a1();
    format!("{s}.{a}:{s}.{z}")
}

fn operand(text: &str) -> String {
    let t = text.trim().trim_start_matches('=');
    if t.parse::<f64>().is_ok() || t.starts_with('"') {
        t.to_string()
    } else {
        OdfCodec::to_open_formula_syntax(t)
    }
}

fn value_type(k: ValueKind) -> &'static str {
    match k {
        ValueKind::Num => "number",
        ValueKind::Min => "minimum",
        ValueKind::Max => "maximum",
        ValueKind::Percent => "percent",
        ValueKind::Percentile => "percentile",
        ValueKind::Formula => "formula",
    }
}

fn entry_xml(tag: &str, v: &CfValue, color: Option<u32>) -> String {
    let mut xml = format!("<calcext:{tag} calcext:type=\"{}\"", value_type(v.kind));
    if !matches!(v.kind, ValueKind::Min | ValueKind::Max) {
        let _ = write!(xml, " calcext:value=\"{}\"", XmlSanitizer::escape(&v.text));
    }
    if let Some(c) = color {
        let _ = write!(xml, " calcext:color=\"#{c:06x}\"");
    }
    xml.push_str("/>");
    xml
}

fn condition_value(kind: &CfKind, anchor: &str) -> Option<String> {
    let text = |t: &str| format!("\"{}\"", t.replace('"', "\"\""));
    Some(match kind {
        CfKind::CellIs { cmp, a, b } => match cmp {
            Cmp::Between => format!("cell-content-is-between({},{})", operand(a), operand(b)),
            Cmp::NotBetween => {
                format!("cell-content-is-not-between({},{})", operand(a), operand(b))
            }
            other => {
                let op = match other {
                    Cmp::Equal => "=",
                    Cmp::NotEqual => "!=",
                    Cmp::Greater => ">",
                    Cmp::GreaterEqual => ">=",
                    Cmp::Less => "<",
                    _ => "<=",
                };
                format!("cell-content(){op}{}", operand(a))
            }
        },
        CfKind::Text { op, text: t } => {
            let name = match op {
                TextOp::Contains => "contains-text",
                TextOp::NotContains => "not-contains-text",
                TextOp::Begins => "begins-with",
                TextOp::Ends => "ends-with",
            };
            format!("{name}({})", text(t))
        }
        CfKind::Blanks { not } => {
            let cmp = if *not { ">" } else { "=" };
            format!("formula-is(LEN(TRIM({anchor})){cmp}0)")
        }
        CfKind::Errors { not } => (if *not { "is-no-error" } else { "is-error" }).to_string(),
        CfKind::Top { n, percent, bottom } => {
            let side = if *bottom { "bottom" } else { "top" };
            let unit = if *percent { "percent" } else { "elements" };
            format!("{side}-{unit}({n})")
        }
        CfKind::Average { above, equal, .. } => match (above, equal) {
            (true, false) => "above-average",
            (false, false) => "below-average",
            (true, true) => "above-equal-average",
            (false, true) => "below-equal-average",
        }
        .to_string(),
        CfKind::Duplicates { unique } => (if *unique { "unique" } else { "duplicate" }).to_string(),
        CfKind::Expression(f) => format!("formula-is({})", OdfCodec::to_open_formula_syntax(f)),
        // No period rule in ODF: written as the formula rule that judges the same days.
        CfKind::TimePeriod(period) => format!(
            "formula-is({})",
            OdfCodec::to_open_formula_syntax(&period_formula(*period, anchor))
        ),
        _ => return None,
    })
}

fn rule_body(rule: &CfRule, styles: &OdsRuleTable, sheet_name: &str) -> String {
    let anchor = CellBox::cell(
        rule.ranges.first().map_or(0, |b| b.row0),
        rule.ranges.first().map_or(0, |b| b.col0),
    )
    .to_a1();
    match &rule.kind {
        CfKind::ColorScale(stops) => {
            let entries: String = stops.iter().map(|(v, c)| entry_xml("color-scale-entry", v, Some(*c))).collect();
            format!("<calcext:color-scale>{entries}</calcext:color-scale>")
        }
        CfKind::DataBar {
            min,
            max,
            positive,
            negative,
            gradient,
            show_value,
        } => format!(
            "<calcext:data-bar calcext:positive-color=\"#{positive:06x}\" calcext:negative-color=\"#{negative:06x}\" calcext:gradient=\"{gradient}\" calcext:show-value=\"{show_value}\" calcext:axis-position=\"automatic\">{}{}</calcext:data-bar>",
            entry_xml("formatting-entry", min, None),
            entry_xml("formatting-entry", max, None)
        ),
        CfKind::IconSet {
            set,
            thresholds,
            show_value,
            ..
        } => {
            let entries: String = thresholds.iter().map(|v| entry_xml("formatting-entry", v, None)).collect();
            format!(
                "<calcext:icon-set calcext:icon-set-type=\"{}\" calcext:show-value=\"{show_value}\">{entries}</calcext:icon-set>",
                ICON_SETS[usize::from(*set)].0
            )
        }
        kind => {
            let Some(value) = condition_value(kind, &anchor) else {
                return String::new();
            };
            let style = styles
                .style_index
                .get(&rule.style)
                .map_or(String::new(), |i| format!(" calcext:apply-style-name=\"eocf{i}\""));
            format!(
                "<calcext:condition{style} calcext:value=\"{}\" calcext:base-cell-address=\"{}\"/>",
                XmlSanitizer::escape(&value),
                XmlSanitizer::escape(&format!("{}.{anchor}", quote_sheet(sheet_name)))
            )
        }
    }
}

impl OdsRuleTable {
    pub(crate) fn build(stylings: &[SheetStyling]) -> Self {
        let mut table = Self {
            styles: Vec::new(),
            style_index: HashMap::new(),
            validations: Vec::new(),
        };
        for (sheet, st) in stylings.iter().enumerate() {
            for rule in st.rules.cf.iter().filter(|r| !r.style.is_plain()) {
                table.style_index.entry(rule.style).or_insert_with(|| {
                    table.styles.push(rule.style);
                    table.styles.len()
                });
            }
            for (i, dv) in st.rules.dv.iter().enumerate() {
                if !dv.ranges.is_empty() {
                    table.validations.push((sheet, i));
                }
            }
        }
        table
    }

    /// `<style:style>` entries to add to the automatic styles.
    pub(crate) fn styles_xml(&self) -> String {
        let mut xml = String::new();
        for (i, s) in self.styles.iter().enumerate() {
            let mut cell = String::new();
            if let Some(bg) = s.bg {
                let _ = write!(cell, " fo:background-color=\"#{bg:06x}\"");
            }
            let mut text = String::new();
            if let Some(fg) = s.fg {
                let _ = write!(text, " fo:color=\"#{fg:06x}\"");
            }
            if s.flags & FLAG_BOLD != 0 {
                text.push_str(" fo:font-weight=\"bold\"");
            }
            if s.flags & FLAG_ITALIC != 0 {
                text.push_str(" fo:font-style=\"italic\"");
            }
            if s.flags & FLAG_UNDERLINE != 0 {
                text.push_str(" style:text-underline-style=\"solid\" style:text-underline-width=\"auto\" style:text-underline-color=\"font-color\"");
            }
            if s.flags & FLAG_STRIKE != 0 {
                text.push_str(" style:text-line-through-style=\"solid\"");
            }
            let _ = write!(
                xml,
                "<style:style style:name=\"eocf{}\" style:family=\"table-cell\"><style:table-cell-properties{cell}/><style:text-properties{text}/></style:style>",
                i + 1
            );
        }
        xml
    }

    fn name_of(&self, sheet: usize, dv_index: usize) -> Option<usize> {
        self.validations
            .iter()
            .position(|&(s, i)| s == sheet && i == dv_index)
    }

    /// `<table:content-validations>` for the whole workbook.
    pub(crate) fn validations_xml(
        &self,
        stylings: &[SheetStyling],
        sheet_names: &[&str],
    ) -> String {
        if self.validations.is_empty() {
            return String::new();
        }
        let mut xml = String::from("<table:content-validations>");
        for (n, &(sheet, i)) in self.validations.iter().enumerate() {
            let dv = &stylings[sheet].rules.dv[i];
            let b = dv.ranges[0];
            let base = format!(
                "{}.{}",
                quote_sheet(sheet_names.get(sheet).copied().unwrap_or("Sheet1")),
                CellBox::cell(b.row0, b.col0).to_a1()
            );
            let display = if dv.kind == DvKind::List && !dv.show_dropdown {
                "none"
            } else {
                "unsorted"
            };
            let _ = write!(
                xml,
                "<table:content-validation table:name=\"eoval{n}\" table:condition=\"{}\" table:allow-empty-cell=\"{}\" table:display-list=\"{display}\" table:base-cell-address=\"{}\">",
                XmlSanitizer::escape(&validation_condition(dv)),
                dv.allow_blank,
                XmlSanitizer::escape(&base)
            );
            let message = |tag: &str, kind: &str, title: &str, text: &str, show: bool| {
                format!(
                    "<table:{tag}{kind} table:title=\"{}\" table:display=\"{show}\"><text:p>{}</text:p></table:{tag}>",
                    XmlSanitizer::escape(title),
                    XmlSanitizer::escape(text)
                )
            };
            xml.push_str(&message(
                "help-message",
                "",
                &dv.prompt_title,
                &dv.prompt,
                dv.show_prompt,
            ));
            let kind = format!(" table:message-type=\"{}\"", dv.alert.name());
            xml.push_str(&message(
                "error-message",
                &kind,
                &dv.error_title,
                &dv.error,
                dv.show_error,
            ));
            xml.push_str("</table:content-validation>");
        }
        xml.push_str("</table:content-validations>");
        xml
    }

    /// Attribute naming the validation of a cell (empty when none applies).
    pub(crate) fn cell_attr(
        &self,
        sheet: usize,
        stylings: &[SheetStyling],
        row: u32,
        col: u32,
    ) -> String {
        let Some(st) = stylings.get(sheet) else {
            return String::new();
        };
        st.rules
            .dv
            .iter()
            .enumerate()
            .find(|(_, dv)| dv.ranges.iter().any(|b| b.contains(row, col)))
            .and_then(|(i, _)| self.name_of(sheet, i))
            .map_or(String::new(), |n| {
                format!(" table:content-validation-name=\"eoval{n}\"")
            })
    }

    /// Empty cells that must exist to carry a validation: ranges small enough to spell out.
    pub(crate) fn blank_cells(&self, rules: &SheetRules) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        let mut budget = BLANK_CELL_CAP;
        for dv in &rules.dv {
            for b in &dv.ranges {
                if b.area() > budget {
                    continue;
                }
                budget -= b.area();
                for r in b.row0..=b.row1 {
                    for c in b.col0..=b.col1 {
                        out.push((r, c));
                    }
                }
            }
        }
        out
    }

    /// `<calcext:conditional-formats>` of one table (empty when it has no formatting rule).
    pub(crate) fn conditional_xml(&self, sheet_name: &str, rules: &SheetRules) -> String {
        let mut ordered: Vec<&CfRule> = rules.cf.iter().filter(|r| !r.ranges.is_empty()).collect();
        ordered.sort_by_key(|r| r.priority);
        let mut xml = String::new();
        for rule in ordered {
            let body = rule_body(rule, self, sheet_name);
            if body.is_empty() {
                continue;
            }
            let targets: Vec<String> = rule.ranges.iter().map(|b| address(sheet_name, b)).collect();
            let _ = write!(
                xml,
                "<calcext:conditional-format calcext:target-range-address=\"{}\">{body}</calcext:conditional-format>",
                XmlSanitizer::escape(&targets.join(" "))
            );
        }
        if xml.is_empty() {
            xml
        } else {
            format!("<calcext:conditional-formats>{xml}</calcext:conditional-formats>")
        }
    }
}

fn validation_condition(dv: &DataValidation) -> String {
    let between = |name: &str| {
        format!(
            "of:cell-content-is-{name}({};{})",
            operand(&dv.a),
            operand(&dv.b)
        )
    };
    let compare = || -> String {
        match dv.cmp {
            Cmp::Between => between("between"),
            Cmp::NotBetween => between("not-between"),
            other => {
                let name = match other {
                    Cmp::Equal => "equal-to",
                    Cmp::NotEqual => "not-equal-to",
                    Cmp::Greater => "greater-than",
                    Cmp::GreaterEqual => "greater-than-or-equal-to",
                    Cmp::Less => "less-than",
                    _ => "less-than-or-equal-to",
                };
                format!("of:cell-content-is-{name}({})", operand(&dv.a))
            }
        }
    };
    match dv.kind {
        DvKind::Whole => format!("of:cell-content-is-whole-number() and {}", compare()),
        DvKind::Decimal => format!("of:cell-content-is-decimal-number() and {}", compare()),
        DvKind::Date => format!("of:cell-content-is-date() and {}", compare()),
        DvKind::Time => format!("of:cell-content-is-time() and {}", compare()),
        DvKind::TextLength => match dv.cmp {
            Cmp::Between => format!(
                "of:cell-content-text-length-is-between({};{})",
                operand(&dv.a),
                operand(&dv.b)
            ),
            Cmp::NotBetween => format!(
                "of:cell-content-text-length-is-not-between({};{})",
                operand(&dv.a),
                operand(&dv.b)
            ),
            other => {
                let op = match other {
                    Cmp::Equal => "=",
                    Cmp::NotEqual => "!=",
                    Cmp::Greater => ">",
                    Cmp::GreaterEqual => ">=",
                    Cmp::Less => "<",
                    _ => "<=",
                };
                format!("of:cell-content-text-length(){op}{}", operand(&dv.a))
            }
        },
        DvKind::Checkbox => {
            let (on, off) = dv.checkbox_values();
            let quoted = |t: &str| format!("\"{}\"", t.replace('"', "\"\""));
            format!("of:cell-content-is-in-list({};{})", quoted(on), quoted(off))
        }
        DvKind::List | DvKind::ListMultiple => match dv.a.strip_prefix('=') {
            Some(reference) => format!("of:cell-content-is-in-list([{}])", range_to_ods(reference)),
            None => {
                let items: Vec<String> =
                    dv.a.split(',')
                        .map(|i| format!("\"{}\"", i.trim().replace('"', "\"\"")))
                        .collect();
                format!("of:cell-content-is-in-list({})", items.join(";"))
            }
        },
        DvKind::Custom => format!(
            "of:is-true-formula({})",
            OdfCodec::to_open_formula_syntax(&dv.a)
        ),
    }
}

/// `Sheet1!$A$1:$A$5` as the `$Sheet1.$A$1:.$A$5` of an ODF cell range.
fn range_to_ods(reference: &str) -> String {
    match reference.rsplit_once('!') {
        Some((sheet, cells)) => {
            let (a, z) = cells.split_once(':').unwrap_or((cells, cells));
            format!("${}.{a}:.{z}", sheet.trim_matches('\''))
        }
        None => {
            let (a, z) = reference.split_once(':').unwrap_or((reference, reference));
            format!(".{a}:.{z}")
        }
    }
}

pub(crate) fn range_from_ods(range: &str) -> String {
    let inner = range.trim().trim_matches(|c| c == '[' || c == ']');
    let (head, tail) = inner.split_once(':').unwrap_or((inner, inner));
    let cell = |s: &str| s.rsplit_once('.').map_or(s, |(_, c)| c).to_string();
    let sheet = head
        .rsplit_once('.')
        .map(|(s, _)| s.trim_start_matches('$').trim_matches('\''));
    let cells = format!("{}:{}", cell(head), cell(tail));
    match sheet.filter(|s| !s.is_empty()) {
        Some(s) => format!("{s}!{cells}"),
        None => cells,
    }
}

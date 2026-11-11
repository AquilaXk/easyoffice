//! Reading side of [`crate::ods_rules`]: conditional formatting and data validation back from
//! `content.xml` (PD-165).

use std::collections::{BTreeSet, HashMap};

use crate::ods_rules::range_from_ods;
use crate::rtf_xml_helpers::{
    extract_attr_from_fragment, extract_tag_bodies, extract_tags_or_self_closing_with_attrs,
    extract_tags_with_attrs,
};
use crate::sheet_rules::{
    icon_set_index, CellBox, CfKind, CfRule, CfStyle, CfValue, Cmp, DataValidation, DvAlert,
    DvKind, SheetRules, TextOp, ValueKind, FLAG_BOLD, FLAG_ITALIC, FLAG_STRIKE, FLAG_UNDERLINE,
    ICON_SETS, MAX_RANGES_PER_RULE, MAX_RULES_PER_SHEET, MAX_RULE_STOPS, MAX_RULE_TEXT_CHARS,
};
use crate::sheet_styling::ods_grid::{OdsCell, OdsSink};
use crate::xml_stream::Attrs;
use crate::{EasyOfficeError, OdfCodec, XmlSanitizer};

/// Cells per sheet the reader attributes to validations.
const MAX_VALIDATED_CELLS: usize = 200_000;

fn attr(attrs: &str, name: &str) -> Option<String> {
    extract_attr_from_fragment(attrs, name).map(XmlSanitizer::unescape)
}

fn color_of(attrs: &str, name: &str) -> Option<u32> {
    let c = attr(attrs, name)?;
    u32::from_str_radix(c.strip_prefix('#')?, 16).ok()
}

/// Colours and flags of the cell styles declared in `content.xml` (automatic and common styles).
pub(crate) fn parse_rule_styles(content_xml: &str) -> HashMap<String, CfStyle> {
    let mut out = HashMap::new();
    for (attrs, body) in extract_tags_with_attrs(content_xml, "style:style") {
        if extract_attr_from_fragment(attrs, "style:family") != Some("table-cell") {
            continue;
        }
        let Some(name) = attr(attrs, "style:name") else {
            continue;
        };
        let mut style = CfStyle::default();
        if let Some((a, _)) =
            extract_tags_or_self_closing_with_attrs(body, "style:table-cell-properties").first()
        {
            style.bg = color_of(a, "fo:background-color");
        }
        if let Some((a, _)) =
            extract_tags_or_self_closing_with_attrs(body, "style:text-properties").first()
        {
            style.fg = color_of(a, "fo:color");
            let on = |n: &str| extract_attr_from_fragment(a, n).is_some_and(|v| v != "none");
            if extract_attr_from_fragment(a, "fo:font-weight")
                .is_some_and(|w| w == "bold" || w.parse::<u32>().is_ok_and(|n| n >= 600))
            {
                style.flags |= FLAG_BOLD;
            }
            if extract_attr_from_fragment(a, "fo:font-style").is_some_and(|s| s == "italic") {
                style.flags |= FLAG_ITALIC;
            }
            if on("style:text-underline-style") {
                style.flags |= FLAG_UNDERLINE;
            }
            if on("style:text-line-through-style") {
                style.flags |= FLAG_STRIKE;
            }
        }
        out.insert(name, style);
    }
    out
}

fn split_args(args: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (mut cur, mut quoted, mut depth) = (String::new(), false, 0usize);
    for ch in args.chars() {
        match ch {
            '"' => {
                quoted = !quoted;
                cur.push(ch);
            }
            '(' | '[' if !quoted => {
                depth += 1;
                cur.push(ch);
            }
            ')' | ']' if !quoted => {
                depth = depth.saturating_sub(1);
                cur.push(ch);
            }
            ',' | ';' if !quoted && depth == 0 => out.push(std::mem::take(&mut cur)),
            _ => cur.push(ch),
        }
    }
    out.push(cur);
    out.into_iter().map(|s| s.trim().to_string()).collect()
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    match t.strip_prefix('"').and_then(|x| x.strip_suffix('"')) {
        Some(inner) => inner.replace("\"\"", "\""),
        None => t.to_string(),
    }
}

fn formula_text(f: &str) -> String {
    OdfCodec::normalize_ods_formula(f)
        .trim_start_matches('=')
        .to_string()
}

fn bounded(s: String) -> String {
    s.chars().take(MAX_RULE_TEXT_CHARS).collect()
}

fn parse_condition(value: &str) -> Option<CfKind> {
    let v = value.trim();
    if let Some(rest) = v.strip_prefix("cell-content()") {
        let (cmp, tail) = [
            (">=", Cmp::GreaterEqual),
            ("<=", Cmp::LessEqual),
            ("!=", Cmp::NotEqual),
            (">", Cmp::Greater),
            ("<", Cmp::Less),
            ("=", Cmp::Equal),
        ]
        .into_iter()
        .find_map(|(op, c)| rest.strip_prefix(op).map(|t| (c, t)))?;
        return Some(CfKind::CellIs {
            cmp,
            a: bounded(formula_text(tail)),
            b: String::new(),
        });
    }
    let (name, args) = match v.split_once('(') {
        Some((n, a)) => (n, a.strip_suffix(')')?),
        None => (v, ""),
    };
    let args = split_args(args);
    let num = || args.first()?.parse::<u32>().ok();
    Some(match name {
        "cell-content-is-between" | "cell-content-is-not-between" if args.len() == 2 => {
            CfKind::CellIs {
                cmp: if name.contains("not") {
                    Cmp::NotBetween
                } else {
                    Cmp::Between
                },
                a: bounded(formula_text(&args[0])),
                b: bounded(formula_text(&args[1])),
            }
        }
        "formula-is" | "is-true-formula" => {
            CfKind::Expression(bounded(formula_text(&split_args_whole(v)?)))
        }
        "contains-text" | "not-contains-text" | "begins-with" | "ends-with" => CfKind::Text {
            op: match name {
                "contains-text" => TextOp::Contains,
                "not-contains-text" => TextOp::NotContains,
                "begins-with" => TextOp::Begins,
                _ => TextOp::Ends,
            },
            text: bounded(unquote(args.first()?)),
        },
        "duplicate" => CfKind::Duplicates { unique: false },
        "unique" => CfKind::Duplicates { unique: true },
        "above-average" | "below-average" | "above-equal-average" | "below-equal-average" => {
            CfKind::Average {
                above: name.starts_with("above"),
                equal: name.contains("equal"),
                std_dev: 0,
            }
        }
        "top-elements" | "bottom-elements" | "top-percent" | "bottom-percent" => CfKind::Top {
            n: num()?,
            percent: name.ends_with("percent"),
            bottom: name.starts_with("bottom"),
        },
        "is-error" => CfKind::Errors { not: false },
        "is-no-error" => CfKind::Errors { not: true },
        _ => return None,
    })
}

/// The text between the outer parentheses of `name(...)`.
fn split_args_whole(v: &str) -> Option<String> {
    let (_, rest) = v.split_once('(')?;
    Some(rest.strip_suffix(')')?.to_string())
}

fn parse_entry(attrs: &str) -> Option<CfValue> {
    let kind = match attr(attrs, "calcext:type")?.as_str() {
        "number" | "value" => ValueKind::Num,
        "minimum" | "auto-minimum" => ValueKind::Min,
        "maximum" | "auto-maximum" => ValueKind::Max,
        "percent" => ValueKind::Percent,
        "percentile" => ValueKind::Percentile,
        "formula" => ValueKind::Formula,
        _ => return None,
    };
    Some(CfValue::new(
        kind,
        bounded(attr(attrs, "calcext:value").unwrap_or_default()),
    ))
}

fn parse_table_ranges(attrs: &str) -> Vec<CellBox> {
    attr(attrs, "calcext:target-range-address")
        .unwrap_or_default()
        .split_whitespace()
        .filter_map(|r| {
            let (a, z) = r.split_once(':').unwrap_or((r, r));
            let cell = |s: &str| s.rsplit_once('.').map_or(s, |(_, c)| c).to_string();
            CellBox::parse(&format!("{}:{}", cell(a), cell(z)))
        })
        .take(MAX_RANGES_PER_RULE)
        .collect()
}

fn parse_conditional_formats(
    table_xml: &str,
    styles: &HashMap<String, CfStyle>,
    rules: &mut SheetRules,
) {
    let mut priority = 0u32;
    for (block_attrs, block) in extract_tags_with_attrs(table_xml, "calcext:conditional-format") {
        let ranges = parse_table_ranges(block_attrs);
        if ranges.is_empty() {
            continue;
        }
        let mut found: Vec<(CfKind, CfStyle, String)> = Vec::new();
        for (a, _) in extract_tags_or_self_closing_with_attrs(block, "calcext:condition") {
            let value = attr(a, "calcext:value").unwrap_or_default();
            let style = attr(a, "calcext:apply-style-name")
                .and_then(|n| styles.get(&n).copied())
                .unwrap_or_default();
            match parse_condition(&value) {
                Some(kind) => found.push((kind, style, String::new())),
                None => rules.unsupported.push(format!(
                    "조건부 서식 {}",
                    value.split('(').next().unwrap_or("조건")
                )),
            }
        }
        for (_, body) in extract_tags_with_attrs(block, "calcext:color-scale") {
            let stops: Vec<(CfValue, u32)> =
                extract_tags_or_self_closing_with_attrs(body, "calcext:color-scale-entry")
                    .into_iter()
                    .filter_map(|(a, _)| Some((parse_entry(a)?, color_of(a, "calcext:color")?)))
                    .take(MAX_RULE_STOPS)
                    .collect();
            if (2..=3).contains(&stops.len()) {
                found.push((CfKind::ColorScale(stops), CfStyle::default(), String::new()));
            } else {
                rules.unsupported.push("조건부 서식 colorScale".to_string());
            }
        }
        for (a, body) in extract_tags_with_attrs(block, "calcext:data-bar") {
            let entries: Vec<CfValue> =
                extract_tags_or_self_closing_with_attrs(body, "calcext:formatting-entry")
                    .into_iter()
                    .filter_map(|(e, _)| parse_entry(e))
                    .collect();
            if let [min, max] = entries.as_slice() {
                found.push((
                    CfKind::DataBar {
                        min: min.clone(),
                        max: max.clone(),
                        positive: color_of(a, "calcext:positive-color").unwrap_or(0x63_8E_C6),
                        negative: color_of(a, "calcext:negative-color").unwrap_or(0xFF_00_00),
                        gradient: attr(a, "calcext:gradient").as_deref() != Some("false"),
                        show_value: attr(a, "calcext:show-value").as_deref() != Some("false"),
                    },
                    CfStyle::default(),
                    String::new(),
                ));
            } else {
                rules.unsupported.push("조건부 서식 dataBar".to_string());
            }
        }
        for (a, body) in extract_tags_with_attrs(block, "calcext:icon-set") {
            let name = attr(a, "calcext:icon-set-type").unwrap_or_default();
            let name = if name == "5Ratings" {
                "5Rating".to_string()
            } else {
                name
            };
            let thresholds: Vec<CfValue> =
                extract_tags_or_self_closing_with_attrs(body, "calcext:formatting-entry")
                    .into_iter()
                    .filter_map(|(e, _)| parse_entry(e))
                    .collect();
            match icon_set_index(&name) {
                Some(set) if thresholds.len() == usize::from(ICON_SETS[usize::from(set)].1) => {
                    found.push((
                        CfKind::IconSet {
                            set,
                            thresholds,
                            reverse: false,
                            show_value: attr(a, "calcext:show-value").as_deref() != Some("false"),
                        },
                        CfStyle::default(),
                        String::new(),
                    ))
                }
                _ => rules.unsupported.push("조건부 서식 iconSet".to_string()),
            }
        }
        for (kind, style, _) in found {
            if rules.cf.len() >= MAX_RULES_PER_SHEET {
                return;
            }
            priority += 1;
            rules.cf.push(CfRule {
                ranges: ranges.clone(),
                priority,
                stop_if_true: false,
                kind,
                style,
            });
        }
    }
}

struct ValidationSpec {
    kind: DvKind,
    cmp: Cmp,
    a: String,
    b: String,
    allow_blank: bool,
    show_dropdown: bool,
    alert: DvAlert,
    show_error: bool,
    error_title: String,
    error: String,
    show_prompt: bool,
    prompt_title: String,
    prompt: String,
}

fn parse_validation_condition(condition: &str) -> Option<(DvKind, Cmp, String, String)> {
    let cond = condition.trim().trim_start_matches("of:");
    let (kind_part, rest) = match cond.split_once(" and ") {
        Some((k, r)) => (
            Some(k.trim().trim_start_matches("of:")),
            r.trim().trim_start_matches("of:"),
        ),
        None => (None, cond),
    };
    let base_kind = match kind_part {
        Some("cell-content-is-whole-number()") => Some(DvKind::Whole),
        Some("cell-content-is-decimal-number()") => Some(DvKind::Decimal),
        Some("cell-content-is-date()") => Some(DvKind::Date),
        Some("cell-content-is-time()") => Some(DvKind::Time),
        Some(_) => return None,
        None => None,
    };
    let (name, args) = match rest.split_once('(') {
        Some((n, a)) => (n, a.strip_suffix(')')?),
        None => (rest, ""),
    };
    let parts = split_args(args);
    let first = parts.first().cloned().unwrap_or_default();
    let second = parts.get(1).cloned().unwrap_or_default();
    let cmp_of = |n: &str| match n {
        "between" => Some(Cmp::Between),
        "not-between" => Some(Cmp::NotBetween),
        "equal-to" => Some(Cmp::Equal),
        "not-equal-to" => Some(Cmp::NotEqual),
        "greater-than" => Some(Cmp::Greater),
        "greater-than-or-equal-to" => Some(Cmp::GreaterEqual),
        "less-than" => Some(Cmp::Less),
        "less-than-or-equal-to" => Some(Cmp::LessEqual),
        _ => None,
    };
    if let (Some(kind), Some(cmp)) = (
        base_kind,
        name.strip_prefix("cell-content-is-").and_then(cmp_of),
    ) {
        return Some((kind, cmp, formula_text(&first), formula_text(&second)));
    }
    if let Some(n) = name.strip_prefix("cell-content-text-length-is-") {
        return Some((
            DvKind::TextLength,
            cmp_of(n)?,
            formula_text(&first),
            formula_text(&second),
        ));
    }
    if name.starts_with("cell-content-text-length()") {
        return None;
    }
    if name == "cell-content-is-in-list" {
        let a = if first.starts_with('[') {
            format!("={}", range_from_ods(&first))
        } else {
            parts
                .iter()
                .map(|p| unquote(p))
                .collect::<Vec<_>>()
                .join(",")
        };
        return Some((DvKind::List, Cmp::Between, a, String::new()));
    }
    if name == "is-true-formula" {
        return Some((
            DvKind::Custom,
            Cmp::Between,
            formula_text(&split_args_whole(rest)?),
            String::new(),
        ));
    }
    None
}

fn parse_text_length_operator(condition: &str) -> Option<(DvKind, Cmp, String, String)> {
    let rest = condition
        .trim()
        .trim_start_matches("of:")
        .strip_prefix("cell-content-text-length()")?;
    let (cmp, tail) = [
        (">=", Cmp::GreaterEqual),
        ("<=", Cmp::LessEqual),
        ("!=", Cmp::NotEqual),
        (">", Cmp::Greater),
        ("<", Cmp::Less),
        ("=", Cmp::Equal),
    ]
    .into_iter()
    .find_map(|(op, c)| rest.strip_prefix(op).map(|t| (c, t)))?;
    Some((DvKind::TextLength, cmp, formula_text(tail), String::new()))
}

fn parse_validations(
    content_xml: &str,
    unsupported: &mut Vec<String>,
) -> HashMap<String, ValidationSpec> {
    let mut out = HashMap::new();
    for (attrs, body) in extract_tags_with_attrs(content_xml, "table:content-validation") {
        let (Some(name), Some(condition)) =
            (attr(attrs, "table:name"), attr(attrs, "table:condition"))
        else {
            continue;
        };
        let Some((kind, cmp, a, b)) = parse_validation_condition(&condition)
            .or_else(|| parse_text_length_operator(&condition))
        else {
            unsupported.push(format!(
                "데이터 유효성 {}",
                condition.split('(').next().unwrap_or("조건")
            ));
            continue;
        };
        let message = |tag: &str| {
            extract_tags_with_attrs(body, tag)
                .into_iter()
                .next()
                .map(|(a, inner)| {
                    let text = extract_tag_bodies(inner, "text:p")
                        .into_iter()
                        .map(XmlSanitizer::unescape)
                        .collect::<Vec<_>>()
                        .join("\n");
                    (
                        bounded(attr(a, "table:title").unwrap_or_default()),
                        bounded(text),
                        attr(a, "table:display").as_deref() != Some("false"),
                        attr(a, "table:message-type"),
                    )
                })
        };
        let help = message("table:help-message");
        let error = message("table:error-message");
        out.insert(
            name,
            ValidationSpec {
                kind,
                cmp,
                a: bounded(a),
                b: bounded(b),
                allow_blank: attr(attrs, "table:allow-empty-cell").as_deref() != Some("false"),
                show_dropdown: attr(attrs, "table:display-list").as_deref() != Some("none"),
                alert: error
                    .as_ref()
                    .and_then(|e| e.3.as_deref())
                    .and_then(DvAlert::from_name)
                    .unwrap_or(DvAlert::Stop),
                show_error: error.as_ref().is_some_and(|e| e.2),
                error_title: error.as_ref().map(|e| e.0.clone()).unwrap_or_default(),
                error: error.as_ref().map(|e| e.1.clone()).unwrap_or_default(),
                show_prompt: help.as_ref().is_some_and(|h| h.2),
                prompt_title: help.as_ref().map(|h| h.0.clone()).unwrap_or_default(),
                prompt: help.as_ref().map(|h| h.1.clone()).unwrap_or_default(),
            },
        );
    }
    out
}

/// Folds cells into rectangles: horizontal runs first, then identical runs of adjacent rows.
fn boxes_of(cells: &BTreeSet<(u32, u32)>) -> Vec<CellBox> {
    let mut runs: Vec<(u32, u32, u32)> = Vec::new();
    for &(r, c) in cells {
        match runs.last_mut() {
            Some((row, _, end)) if *row == r && *end + 1 == c => *end = c,
            _ => runs.push((r, c, c)),
        }
    }
    runs.sort_by_key(|&(r, c0, c1)| (c0, c1, r));
    let mut out: Vec<CellBox> = Vec::new();
    for (r, c0, c1) in runs {
        match out.last_mut() {
            Some(b) if b.col0 == c0 && b.col1 == c1 && b.row1 + 1 == r => b.row1 = r,
            _ => out.push(CellBox {
                row0: r,
                col0: c0,
                row1: r,
                col1: c1,
            }),
        }
    }
    out
}

/// Cells that name a validation, per table, collected while the grid is walked.
#[derive(Default)]
pub(crate) struct ValidationSink {
    tables: Vec<HashMap<String, BTreeSet<(u32, u32)>>>,
    current: HashMap<String, BTreeSet<(u32, u32)>>,
    row: u32,
    total: usize,
}

impl ValidationSink {
    pub(crate) fn into_tables(self) -> Vec<HashMap<String, BTreeSet<(u32, u32)>>> {
        self.tables
    }
}

impl OdsSink for ValidationSink {
    fn table(&mut self, _attrs: &Attrs<'_>) {
        self.current = HashMap::new();
        self.row = 0;
        self.total = 0;
    }

    fn row(&mut self, repeat: u32, cells: &[OdsCell<'_>]) -> Result<(), EasyOfficeError> {
        let mut col = 0u32;
        for cell in cells {
            if let Some(name) = cell.validation {
                let block = repeat as usize * cell.col_repeat as usize;
                if block <= 100_000 && self.total + block <= MAX_VALIDATED_CELLS {
                    self.total += block;
                    let set = self
                        .current
                        .entry(XmlSanitizer::unescape(name))
                        .or_default();
                    for r in self.row..self.row.saturating_add(repeat).min(OdfCodec::MAX_ROWS + 1) {
                        for c in col..col
                            .saturating_add(cell.col_repeat)
                            .min(OdfCodec::MAX_COLS + 1)
                        {
                            set.insert((r, c));
                        }
                    }
                }
            }
            col = col.saturating_add(cell.col_repeat);
        }
        self.row = self.row.saturating_add(repeat);
        Ok(())
    }

    fn table_end(&mut self) {
        self.tables.push(std::mem::take(&mut self.current));
    }
}

fn fold_cell_validations(
    cells: &HashMap<String, BTreeSet<(u32, u32)>>,
    specs: &HashMap<String, ValidationSpec>,
    rules: &mut SheetRules,
) {
    let mut names: Vec<String> = cells.keys().cloned().collect();
    names.sort();
    for name in names {
        let spec = &specs[&name];
        let boxes = boxes_of(&cells[&name]);
        for chunk in boxes.chunks(MAX_RANGES_PER_RULE) {
            if rules.len() >= MAX_RULES_PER_SHEET {
                return;
            }
            let mut dv = DataValidation::new(chunk.to_vec(), spec.kind);
            dv.cmp = spec.cmp;
            dv.a = spec.a.clone();
            dv.b = spec.b.clone();
            dv.allow_blank = spec.allow_blank;
            dv.show_dropdown = spec.show_dropdown;
            dv.alert = spec.alert;
            dv.show_error = spec.show_error;
            dv.error_title = spec.error_title.clone();
            dv.error = spec.error.clone();
            dv.show_prompt = spec.show_prompt;
            dv.prompt_title = spec.prompt_title.clone();
            dv.prompt = spec.prompt.clone();
            rules.dv.push(dv);
        }
    }
}

/// Workbook-level pieces of the reader, parsed once per `content.xml`.
pub(crate) struct OdsRuleReader {
    has_rules: bool,
    styles: HashMap<String, CfStyle>,
    validations: HashMap<String, ValidationSpec>,
    unsupported_validations: Vec<String>,
}

impl OdsRuleReader {
    pub(crate) fn new(content_xml: &str) -> Self {
        let has_rules = content_xml.contains("calcext:conditional-format")
            || content_xml.contains("table:content-validation");
        let mut unsupported_validations = Vec::new();
        Self {
            has_rules,
            styles: if has_rules {
                parse_rule_styles(content_xml)
            } else {
                HashMap::new()
            },
            validations: if has_rules {
                parse_validations(content_xml, &mut unsupported_validations)
            } else {
                HashMap::new()
            },
            unsupported_validations,
        }
    }

    /// False when the file names no conditional format and no validation at all.
    pub(crate) fn has_rules(&self) -> bool {
        self.has_rules
    }

    /// Rules of one table. Validations that the reader could not parse are listed on the first
    /// table that has any rule at all (they belong to the workbook, not to a table).
    pub(crate) fn table_rules(
        &mut self,
        table_xml: &str,
        validated: &HashMap<String, BTreeSet<(u32, u32)>>,
    ) -> SheetRules {
        let mut rules = SheetRules::default();
        if table_xml.contains("calcext:conditional-format") {
            parse_conditional_formats(table_xml, &self.styles, &mut rules);
        }
        if !validated.is_empty() {
            fold_cell_validations(validated, &self.validations, &mut rules);
            rules.unsupported.append(&mut self.unsupported_validations);
        }
        rules.cf.sort_by_key(|r| r.priority);
        rules
    }
}

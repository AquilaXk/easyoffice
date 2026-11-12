//! SpreadsheetML conditional formatting (`<conditionalFormatting>`, differential formats in
//! `styles.xml`) and data validation (`<dataValidations>`) for [`SheetRules`] (PD-171).
//!
//! Element order inside `<worksheet>` follows ECMA-376 `CT_Worksheet`: `sheetData`, then
//! `conditionalFormatting`, then `dataValidations`. Rules whose type the model cannot hold
//! (extended icon sets, extended validations) are named in
//! [`SheetRules::unsupported`] by the reader; theme and indexed colours are not resolved.

use std::collections::HashMap;
use std::fmt::Write;

use crate::rtf_xml_helpers::{
    extract_attr_from_fragment, extract_tag_bodies, extract_tags_or_self_closing_with_attrs,
};
use crate::sheet_styling::sheet_rules::{
    format_sqref, icon_set_index, parse_sqref, CellBox, CfKind, CfRule, CfStyle, CfValue, Cmp,
    DataValidation, DvAlert, DvKind, SheetRules, TextOp, TimePeriod, ValueKind, FLAG_BOLD,
    FLAG_ITALIC, FLAG_STRIKE, FLAG_UNDERLINE, ICON_SETS, MAX_RULES_PER_SHEET, MAX_RULE_STOPS,
    MAX_RULE_TEXT_CHARS,
};
use crate::XmlSanitizer;

/// Differential formats of a workbook, deduplicated in first-use order.
#[derive(Default)]
pub(crate) struct DxfTable {
    styles: Vec<CfStyle>,
    index: HashMap<CfStyle, usize>,
}

impl DxfTable {
    pub(crate) fn build(all: impl IntoIterator<Item = CfStyle>) -> Self {
        let mut table = Self::default();
        for style in all.into_iter().filter(|s| !s.is_plain()) {
            table.index.entry(style).or_insert_with(|| {
                table.styles.push(style);
                table.styles.len() - 1
            });
        }
        table
    }

    pub(crate) fn id(&self, style: &CfStyle) -> Option<usize> {
        self.index.get(style).copied()
    }

    /// `<dxfs>` element (empty when no rule formats by style).
    pub(crate) fn to_xml(&self) -> String {
        if self.styles.is_empty() {
            return String::new();
        }
        let mut xml = format!("<dxfs count=\"{}\">", self.styles.len());
        for s in &self.styles {
            xml.push_str("<dxf>");
            if s.fg.is_some() || s.flags != 0 {
                xml.push_str("<font>");
                for (flag, tag) in [
                    (FLAG_BOLD, "<b/>"),
                    (FLAG_ITALIC, "<i/>"),
                    (FLAG_STRIKE, "<strike/>"),
                    (FLAG_UNDERLINE, "<u/>"),
                ] {
                    if s.flags & flag != 0 {
                        xml.push_str(tag);
                    }
                }
                if let Some(fg) = s.fg {
                    let _ = write!(xml, "<color rgb=\"FF{fg:06X}\"/>");
                }
                xml.push_str("</font>");
            }
            if let Some(bg) = s.bg {
                let _ = write!(
                    xml,
                    "<fill><patternFill><bgColor rgb=\"FF{bg:06X}\"/></patternFill></fill>"
                );
            }
            xml.push_str("</dxf>");
        }
        xml.push_str("</dxfs>");
        xml
    }
}

fn attr(attrs: &str, name: &str) -> Option<String> {
    extract_attr_from_fragment(attrs, name).map(XmlSanitizer::unescape)
}

fn flag_attr(attrs: &str, name: &str, default: bool) -> bool {
    match extract_attr_from_fragment(attrs, name) {
        None => default,
        Some("0" | "false") => false,
        Some(_) => true,
    }
}

fn rgb_of(attrs: &str) -> Option<u32> {
    let rgb = extract_attr_from_fragment(attrs, "rgb")?;
    u32::from_str_radix(rgb.get(rgb.len().checked_sub(6)?..)?, 16).ok()
}

fn bounded(text: String) -> String {
    if text.chars().count() > MAX_RULE_TEXT_CHARS {
        text.chars().take(MAX_RULE_TEXT_CHARS).collect()
    } else {
        text
    }
}

fn first_formula_texts(body: &str) -> Vec<String> {
    extract_tag_bodies(body, "formula")
        .into_iter()
        .map(|f| bounded(XmlSanitizer::unescape(f)))
        .collect()
}

/// Styles of the `<dxf>` entries of `styles.xml`, in file order.
pub(crate) fn parse_dxf_styles(styles_xml: &str) -> Vec<CfStyle> {
    let Some(dxfs) = extract_tag_bodies(styles_xml, "dxfs").into_iter().next() else {
        return Vec::new();
    };
    extract_tag_bodies(dxfs, "dxf")
        .into_iter()
        .map(|dxf| {
            let mut style = CfStyle::default();
            if let Some(font) = extract_tag_bodies(dxf, "font").into_iter().next() {
                for (tag, flag) in [
                    ("b", FLAG_BOLD),
                    ("i", FLAG_ITALIC),
                    ("strike", FLAG_STRIKE),
                    ("u", FLAG_UNDERLINE),
                ] {
                    let on = extract_tags_or_self_closing_with_attrs(font, tag)
                        .first()
                        .is_some_and(|(a, _)| {
                            flag_attr(a, "val", true)
                                && extract_attr_from_fragment(a, "val") != Some("none")
                        });
                    if on {
                        style.flags |= flag;
                    }
                }
                style.fg = extract_tags_or_self_closing_with_attrs(font, "color")
                    .first()
                    .and_then(|(a, _)| rgb_of(a));
            }
            if let Some(fill) = extract_tag_bodies(dxf, "fill").into_iter().next() {
                style.bg = ["bgColor", "fgColor"].iter().find_map(|tag| {
                    extract_tags_or_self_closing_with_attrs(fill, tag)
                        .first()
                        .and_then(|(a, _)| rgb_of(a))
                });
            }
            style
        })
        .collect()
}

fn value_xml(v: &CfValue, with_gte: bool) -> String {
    let mut xml = format!("<cfvo type=\"{}\"", v.kind.name());
    if !matches!(v.kind, ValueKind::Min | ValueKind::Max) {
        let _ = write!(xml, " val=\"{}\"", XmlSanitizer::escape(&v.text));
    }
    if with_gte && !v.gte {
        xml.push_str(" gte=\"0\"");
    }
    xml.push_str("/>");
    xml
}

fn text_formula(op: TextOp, anchor: &str, text: &str) -> String {
    let quoted = format!("\"{}\"", text.replace('"', "\"\""));
    match op {
        TextOp::Contains => format!("NOT(ISERROR(SEARCH({quoted},{anchor})))"),
        TextOp::NotContains => format!("ISERROR(SEARCH({quoted},{anchor}))"),
        TextOp::Begins => format!("LEFT({anchor},LEN({quoted}))={quoted}"),
        TextOp::Ends => format!("RIGHT({anchor},LEN({quoted}))={quoted}"),
    }
}

/// The formula spreadsheet programs write for a period rule on the cell `a`; they judge by
/// `timePeriod` and keep the formula for readers that do not.
pub(crate) fn period_formula(period: TimePeriod, a: &str) -> String {
    let month = |step: &str| {
        format!(
            "AND(MONTH({a})=MONTH(EDATE(TODAY(),{step})),YEAR({a})=YEAR(EDATE(TODAY(),{step})))"
        )
    };
    match period {
        TimePeriod::Today => format!("FLOOR({a},1)=TODAY()"),
        TimePeriod::Yesterday => format!("FLOOR({a},1)=TODAY()-1"),
        TimePeriod::Tomorrow => format!("FLOOR({a},1)=TODAY()+1"),
        TimePeriod::Last7Days => format!("AND(TODAY()-FLOOR({a},1)<=6,FLOOR({a},1)<=TODAY())"),
        TimePeriod::ThisMonth => format!("AND(MONTH({a})=MONTH(TODAY()),YEAR({a})=YEAR(TODAY()))"),
        TimePeriod::LastMonth => month("0-1"),
        TimePeriod::NextMonth => month("0+1"),
        TimePeriod::ThisWeek => format!(
            "AND(TODAY()-ROUNDDOWN({a},0)<=WEEKDAY(TODAY())-1,ROUNDDOWN({a},0)-TODAY()<=7-WEEKDAY(TODAY()))"
        ),
        TimePeriod::LastWeek => format!(
            "AND(TODAY()-ROUNDDOWN({a},0)>=(WEEKDAY(TODAY())),TODAY()-ROUNDDOWN({a},0)<(WEEKDAY(TODAY())+7))"
        ),
        TimePeriod::NextWeek => format!(
            "AND(ROUNDDOWN({a},0)-TODAY()>(7-WEEKDAY(TODAY())),ROUNDDOWN({a},0)-TODAY()<(15-WEEKDAY(TODAY())))"
        ),
    }
}

fn rule_xml(rule: &CfRule, priority: usize, dxf: &DxfTable) -> String {
    // The top-left cell of the first range: the cell the generated formulas are written for.
    let anchor = rule.ranges.first().map_or_else(
        || "A1".to_string(),
        |b| CellBox::cell(b.row0, b.col0).to_a1(),
    );
    let dxf_attr = dxf
        .id(&rule.style)
        .map_or(String::new(), |id| format!(" dxfId=\"{id}\""));
    let mut head = String::new();
    let mut body = String::new();
    let mut uses_dxf = true;
    match &rule.kind {
        CfKind::CellIs { cmp, a, b } => {
            let _ = write!(head, "type=\"cellIs\" operator=\"{}\"", cmp.name());
            let _ = write!(body, "<formula>{}</formula>", XmlSanitizer::escape(a));
            if cmp.takes_two() {
                let _ = write!(body, "<formula>{}</formula>", XmlSanitizer::escape(b));
            }
        }
        CfKind::TimePeriod(period) => {
            let _ = write!(head, "type=\"timePeriod\" timePeriod=\"{}\"", period.name());
            let _ = write!(
                body,
                "<formula>{}</formula>",
                XmlSanitizer::escape(&period_formula(*period, &anchor))
            );
        }
        CfKind::Text { op, text } => {
            let (ty, operator) = match op {
                TextOp::Contains => ("containsText", "containsText"),
                TextOp::NotContains => ("notContainsText", "notContains"),
                TextOp::Begins => ("beginsWith", "beginsWith"),
                TextOp::Ends => ("endsWith", "endsWith"),
            };
            let _ = write!(
                head,
                "type=\"{ty}\" operator=\"{operator}\" text=\"{}\"",
                XmlSanitizer::escape(text)
            );
            let _ = write!(
                body,
                "<formula>{}</formula>",
                XmlSanitizer::escape(&text_formula(*op, &anchor, text))
            );
        }
        CfKind::Blanks { not } => {
            head.push_str(if *not {
                "type=\"notContainsBlanks\""
            } else {
                "type=\"containsBlanks\""
            });
            let cmp = if *not { ">" } else { "=" };
            let _ = write!(body, "<formula>LEN(TRIM({anchor})){cmp}0</formula>");
        }
        CfKind::Errors { not } => {
            head.push_str(if *not {
                "type=\"notContainsErrors\""
            } else {
                "type=\"containsErrors\""
            });
            let inner = format!("ISERROR({anchor})");
            let f = if *not { format!("NOT({inner})") } else { inner };
            let _ = write!(body, "<formula>{f}</formula>");
        }
        CfKind::Top { n, percent, bottom } => {
            let _ = write!(head, "type=\"top10\" rank=\"{n}\"");
            if *percent {
                head.push_str(" percent=\"1\"");
            }
            if *bottom {
                head.push_str(" bottom=\"1\"");
            }
        }
        CfKind::Average {
            above,
            equal,
            std_dev,
        } => {
            head.push_str("type=\"aboveAverage\"");
            if !above {
                head.push_str(" aboveAverage=\"0\"");
            }
            if *equal {
                head.push_str(" equalAverage=\"1\"");
            }
            if *std_dev > 0 {
                let _ = write!(head, " stdDev=\"{std_dev}\"");
            }
        }
        CfKind::Duplicates { unique } => {
            head.push_str(if *unique {
                "type=\"uniqueValues\""
            } else {
                "type=\"duplicateValues\""
            });
        }
        CfKind::Expression(f) => {
            head.push_str("type=\"expression\"");
            let _ = write!(body, "<formula>{}</formula>", XmlSanitizer::escape(f));
        }
        CfKind::ColorScale(stops) => {
            uses_dxf = false;
            head.push_str("type=\"colorScale\"");
            body.push_str("<colorScale>");
            for (v, _) in stops {
                body.push_str(&value_xml(v, false));
            }
            for (_, c) in stops {
                let _ = write!(body, "<color rgb=\"FF{c:06X}\"/>");
            }
            body.push_str("</colorScale>");
        }
        CfKind::DataBar {
            min,
            max,
            positive,
            show_value,
            ..
        } => {
            uses_dxf = false;
            head.push_str("type=\"dataBar\"");
            let show = if *show_value { "" } else { " showValue=\"0\"" };
            let _ = write!(
                body,
                "<dataBar{show}>{}{}<color rgb=\"FF{positive:06X}\"/></dataBar>",
                value_xml(min, false),
                value_xml(max, false)
            );
        }
        CfKind::IconSet {
            set,
            thresholds,
            reverse,
            show_value,
        } => {
            uses_dxf = false;
            head.push_str("type=\"iconSet\"");
            let name = ICON_SETS.get(usize::from(*set)).map_or("3Arrows", |s| s.0);
            let mut attrs = format!(" iconSet=\"{name}\"");
            if !show_value {
                attrs.push_str(" showValue=\"0\"");
            }
            if *reverse {
                attrs.push_str(" reverse=\"1\"");
            }
            body.push_str(&format!("<iconSet{attrs}>"));
            for t in thresholds {
                body.push_str(&value_xml(t, true));
            }
            body.push_str("</iconSet>");
        }
    }
    let dxf_attr = if uses_dxf { dxf_attr } else { String::new() };
    let stop = if rule.stop_if_true {
        " stopIfTrue=\"1\""
    } else {
        ""
    };
    format!("<cfRule {head}{dxf_attr} priority=\"{priority}\"{stop}>{body}</cfRule>")
}

fn truncated(text: &str, max: usize) -> String {
    XmlSanitizer::escape(&text.chars().take(max).collect::<String>())
}

fn validation_xml(dv: &DataValidation) -> String {
    let mut attrs = format!("type=\"{}\"", dv.kind.file_kind().name());
    if dv.alert != DvAlert::Stop {
        let _ = write!(attrs, " errorStyle=\"{}\"", dv.alert.name());
    }
    if dv.cmp != Cmp::Between && !dv.kind.is_list() && dv.kind != DvKind::Custom {
        let _ = write!(attrs, " operator=\"{}\"", dv.cmp.name());
    }
    if dv.allow_blank {
        attrs.push_str(" allowBlank=\"1\"");
    }
    // `showDropDown="1"` HIDES the in-cell arrow.
    if dv.kind.file_kind() == DvKind::List && !dv.show_dropdown {
        attrs.push_str(" showDropDown=\"1\"");
    }
    if dv.show_prompt {
        attrs.push_str(" showInputMessage=\"1\"");
    }
    if dv.show_error {
        attrs.push_str(" showErrorMessage=\"1\"");
    }
    for (name, text, max) in [
        ("errorTitle", &dv.error_title, 32),
        ("error", &dv.error, 255),
        ("promptTitle", &dv.prompt_title, 32),
        ("prompt", &dv.prompt, 255),
    ] {
        if !text.is_empty() {
            let _ = write!(attrs, " {name}=\"{}\"", truncated(text, max));
        }
    }
    let _ = write!(attrs, " sqref=\"{}\"", format_sqref(&dv.ranges));
    let first = match (dv.kind, dv.a.strip_prefix('=')) {
        (DvKind::Checkbox, _) => {
            let (on, off) = dv.checkbox_values();
            format!("\"{}\"", format!("{on},{off}").replace('"', "\"\""))
        }
        (kind, Some(reference)) if kind.is_list() => reference.to_string(),
        (kind, None) if kind.is_list() => format!("\"{}\"", dv.a.replace('"', "\"\"")),
        _ => dv.a.clone(),
    };
    let mut body = format!("<formula1>{}</formula1>", XmlSanitizer::escape(&first));
    if dv.cmp.takes_two()
        && !dv.kind.is_list()
        && !matches!(dv.kind, DvKind::Custom | DvKind::Checkbox)
    {
        let _ = write!(body, "<formula2>{}</formula2>", XmlSanitizer::escape(&dv.b));
    }
    format!("<dataValidation {attrs}>{body}</dataValidation>")
}

/// Namespace and extension id of the part that tells the check box and the multiple list from a
/// plain list: the file formats have no such validation, so it is written as the list of its
/// values and this extension (ignored by other programs) names the rules that are more.
const KIND_NS: &str = "urn:easyoffice:sheet-rules";
const KIND_EXT_URI: &str = "{4D0B5E1C-7A52-4C77-9F0A-2E6B1D93A8C4}";

fn kind_ext_xml(dvs: &[&DataValidation]) -> String {
    let marks: String = dvs
        .iter()
        .filter(|dv| matches!(dv.kind, DvKind::Checkbox | DvKind::ListMultiple))
        .map(|dv| {
            let (on, off) = dv.checkbox_values();
            let values = if dv.kind == DvKind::Checkbox {
                format!(
                    " on=\"{}\" off=\"{}\"",
                    truncated(on, 255),
                    truncated(off, 255)
                )
            } else {
                String::new()
            };
            format!(
                "<eo:dv sqref=\"{}\" kind=\"{}\"{values}/>",
                format_sqref(&dv.ranges),
                dv.kind.name()
            )
        })
        .collect();
    if marks.is_empty() {
        return String::new();
    }
    format!("<extLst><ext uri=\"{KIND_EXT_URI}\" xmlns:eo=\"{KIND_NS}\">{marks}</ext></extLst>")
}

/// `<conditionalFormatting>` blocks and `<dataValidations>` of one sheet (empty when none).
pub(crate) fn rules_xml(rules: &SheetRules, dxf: &DxfTable) -> String {
    let mut out = String::new();
    let mut ordered: Vec<&CfRule> = rules.cf.iter().collect();
    ordered.sort_by_key(|r| r.priority);
    for (i, rule) in ordered.into_iter().enumerate() {
        if rule.ranges.is_empty() {
            continue;
        }
        let _ = write!(
            out,
            "<conditionalFormatting sqref=\"{}\">{}</conditionalFormatting>",
            format_sqref(&rule.ranges),
            rule_xml(rule, i + 1, dxf)
        );
    }
    let written: Vec<&DataValidation> =
        rules.dv.iter().filter(|dv| !dv.ranges.is_empty()).collect();
    let validations: Vec<String> = written.iter().map(|dv| validation_xml(dv)).collect();
    if !validations.is_empty() {
        let _ = write!(
            out,
            "<dataValidations count=\"{}\">{}</dataValidations>",
            validations.len(),
            validations.concat()
        );
        out.push_str(&kind_ext_xml(&written));
    }
    out
}

fn parse_value(attrs: &str) -> Option<CfValue> {
    let kind = ValueKind::from_name(extract_attr_from_fragment(attrs, "type")?)?;
    let mut value = CfValue::new(kind, bounded(attr(attrs, "val").unwrap_or_default()));
    value.gte = flag_attr(attrs, "gte", true);
    Some(value)
}

fn parse_values(body: &str) -> Vec<CfValue> {
    extract_tags_or_self_closing_with_attrs(body, "cfvo")
        .into_iter()
        .filter_map(|(a, _)| parse_value(a))
        .take(MAX_RULE_STOPS)
        .collect()
}

fn parse_colors(body: &str) -> Vec<u32> {
    extract_tags_or_self_closing_with_attrs(body, "color")
        .into_iter()
        .filter_map(|(a, _)| rgb_of(a))
        .collect()
}

fn parse_kind(attrs: &str, body: &str) -> Result<CfKind, String> {
    let ty = attr(attrs, "type").unwrap_or_default();
    let formulas = first_formula_texts(body);
    let first = formulas.first().cloned().unwrap_or_default();
    let unsupported = |why: &str| Err(why.to_string());
    Ok(match ty.as_str() {
        "cellIs" => {
            let Some(cmp) = attr(attrs, "operator").and_then(|o| Cmp::from_name(&o)) else {
                return unsupported("cellIs");
            };
            CfKind::CellIs {
                cmp,
                a: first,
                b: formulas.get(1).cloned().unwrap_or_default(),
            }
        }
        "containsText" | "notContainsText" | "beginsWith" | "endsWith" => {
            let op = match ty.as_str() {
                "containsText" => TextOp::Contains,
                "notContainsText" => TextOp::NotContains,
                "beginsWith" => TextOp::Begins,
                _ => TextOp::Ends,
            };
            CfKind::Text {
                op,
                text: bounded(attr(attrs, "text").unwrap_or_default()),
            }
        }
        "timePeriod" => match attr(attrs, "timePeriod").and_then(|p| TimePeriod::from_name(&p)) {
            Some(period) => CfKind::TimePeriod(period),
            None => return unsupported("timePeriod"),
        },
        "containsBlanks" => CfKind::Blanks { not: false },
        "notContainsBlanks" => CfKind::Blanks { not: true },
        "containsErrors" => CfKind::Errors { not: false },
        "notContainsErrors" => CfKind::Errors { not: true },
        "top10" => CfKind::Top {
            n: attr(attrs, "rank")
                .and_then(|r| r.parse().ok())
                .unwrap_or(10),
            percent: flag_attr(attrs, "percent", false),
            bottom: flag_attr(attrs, "bottom", false),
        },
        "aboveAverage" => CfKind::Average {
            above: flag_attr(attrs, "aboveAverage", true),
            equal: flag_attr(attrs, "equalAverage", false),
            std_dev: attr(attrs, "stdDev")
                .and_then(|s| s.parse::<u8>().ok())
                .unwrap_or(0),
        },
        "duplicateValues" => CfKind::Duplicates { unique: false },
        "uniqueValues" => CfKind::Duplicates { unique: true },
        "expression" => CfKind::Expression(first),
        "colorScale" => {
            let stops = parse_values(body);
            let colors = parse_colors(body);
            if !(2..=3).contains(&stops.len()) || colors.len() != stops.len() {
                return unsupported("colorScale");
            }
            CfKind::ColorScale(stops.into_iter().zip(colors).collect())
        }
        "dataBar" => {
            let mut stops = parse_values(body).into_iter();
            let (Some(min), Some(max)) = (stops.next(), stops.next()) else {
                return unsupported("dataBar");
            };
            let bar_attrs = extract_tags_or_self_closing_with_attrs(body, "dataBar")
                .first()
                .map(|(a, _)| *a)
                .unwrap_or_default();
            CfKind::DataBar {
                min,
                max,
                positive: parse_colors(body).first().copied().unwrap_or(0x63_8E_C6),
                negative: 0xFF_00_00,
                gradient: true,
                show_value: flag_attr(bar_attrs, "showValue", true),
            }
        }
        "iconSet" => {
            let set_attrs = extract_tags_or_self_closing_with_attrs(body, "iconSet")
                .first()
                .map(|(a, _)| *a)
                .unwrap_or_default();
            let set = attr(set_attrs, "iconSet").unwrap_or_else(|| "3TrafficLights1".to_string());
            let Some(set) = icon_set_index(&set) else {
                return unsupported("iconSet");
            };
            let thresholds = parse_values(body);
            if thresholds.len() != usize::from(ICON_SETS[usize::from(set)].1) {
                return unsupported("iconSet");
            }
            CfKind::IconSet {
                set,
                thresholds,
                reverse: flag_attr(set_attrs, "reverse", false),
                show_value: flag_attr(set_attrs, "showValue", true),
            }
        }
        other => return unsupported(if other.is_empty() { "unknown" } else { other }),
    })
}

fn parse_conditional_formatting(sheet_xml: &str, dxf: &[CfStyle], rules: &mut SheetRules) {
    for (block_attrs, block) in
        extract_tags_or_self_closing_with_attrs(sheet_xml, "conditionalFormatting")
    {
        let ranges = parse_sqref(&attr(block_attrs, "sqref").unwrap_or_default());
        for (attrs, body) in extract_tags_or_self_closing_with_attrs(block, "cfRule") {
            if rules.cf.len() >= MAX_RULES_PER_SHEET {
                return;
            }
            match parse_kind(attrs, body) {
                Ok(kind) if !ranges.is_empty() => {
                    let style = attr(attrs, "dxfId")
                        .and_then(|i| i.parse::<usize>().ok())
                        .and_then(|i| dxf.get(i))
                        .copied()
                        .unwrap_or_default();
                    rules.cf.push(CfRule {
                        ranges: ranges.clone(),
                        priority: attr(attrs, "priority")
                            .and_then(|p| p.parse().ok())
                            .unwrap_or(u32::MAX),
                        stop_if_true: flag_attr(attrs, "stopIfTrue", false),
                        kind,
                        style,
                    });
                }
                Ok(_) => {}
                Err(name) => rules.unsupported.push(format!("조건부 서식 {name}")),
            }
        }
    }
    // Icon sets outside the 2007 schema (stars, triangles, boxes, custom icons) live only here.
    let extended = sheet_xml.matches("<x14:cfRule").count();
    for _ in 0..extended.saturating_sub(sheet_xml.matches("<x14:cfRule type=\"dataBar\"").count()) {
        rules
            .unsupported
            .push("조건부 서식 확장 아이콘".to_string());
    }
}

fn first_formula(body: &str, tag: &str) -> String {
    extract_tag_bodies(body, tag)
        .into_iter()
        .next()
        .map(|f| bounded(XmlSanitizer::unescape(f)))
        .unwrap_or_default()
}

/// Turns the lists the kind extension names back into check boxes and multiple lists.
fn restore_kinds(sheet_xml: &str, rules: &mut SheetRules) {
    if !sheet_xml.contains(KIND_NS) {
        return;
    }
    for (attrs, _) in extract_tags_or_self_closing_with_attrs(sheet_xml, "eo:dv") {
        let (Some(sqref), Some(kind)) = (
            attr(attrs, "sqref"),
            attr(attrs, "kind").and_then(|k| DvKind::from_name(&k)),
        ) else {
            continue;
        };
        let boxes = parse_sqref(&sqref);
        let Some(dv) = rules
            .dv
            .iter_mut()
            .find(|dv| dv.kind == DvKind::List && dv.ranges == boxes)
        else {
            continue;
        };
        match kind {
            DvKind::Checkbox => {
                dv.kind = kind;
                dv.a = bounded(attr(attrs, "on").unwrap_or_default());
                dv.b = bounded(attr(attrs, "off").unwrap_or_default());
            }
            DvKind::ListMultiple => dv.kind = kind,
            _ => {}
        }
    }
}

fn parse_validations(sheet_xml: &str, rules: &mut SheetRules) {
    for (attrs, body) in extract_tags_or_self_closing_with_attrs(sheet_xml, "dataValidation") {
        if rules.len() >= MAX_RULES_PER_SHEET {
            return;
        }
        let ty = attr(attrs, "type").unwrap_or_default();
        let Some(kind) = DvKind::from_name(&ty).filter(|k| k.file_kind() == *k) else {
            rules.unsupported.push(format!(
                "데이터 유효성 {}",
                if ty.is_empty() { "none" } else { &ty }
            ));
            continue;
        };
        let ranges = parse_sqref(&attr(attrs, "sqref").unwrap_or_default());
        if ranges.is_empty() {
            continue;
        }
        let mut dv = DataValidation::new(ranges, kind);
        dv.cmp = attr(attrs, "operator")
            .and_then(|o| Cmp::from_name(&o))
            .unwrap_or(Cmp::Between);
        let f1 = first_formula(body, "formula1");
        dv.a = if kind.is_list() {
            match f1.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                Some(items) => items.replace("\"\"", "\""),
                None => format!("={f1}"),
            }
        } else {
            f1
        };
        dv.b = first_formula(body, "formula2");
        dv.allow_blank = flag_attr(attrs, "allowBlank", false);
        dv.show_dropdown = !flag_attr(attrs, "showDropDown", false);
        dv.alert = attr(attrs, "errorStyle")
            .and_then(|s| DvAlert::from_name(&s))
            .unwrap_or(DvAlert::Stop);
        dv.show_error = flag_attr(attrs, "showErrorMessage", false);
        dv.show_prompt = flag_attr(attrs, "showInputMessage", false);
        dv.error_title = bounded(attr(attrs, "errorTitle").unwrap_or_default());
        dv.error = bounded(attr(attrs, "error").unwrap_or_default());
        dv.prompt_title = bounded(attr(attrs, "promptTitle").unwrap_or_default());
        dv.prompt = bounded(attr(attrs, "prompt").unwrap_or_default());
        rules.dv.push(dv);
    }
    restore_kinds(sheet_xml, rules);
    for _ in 0..sheet_xml.matches("<x14:dataValidation ").count() {
        rules.unsupported.push("데이터 유효성 확장".to_string());
    }
}

/// Rules of one worksheet; `dxf` is [`parse_dxf_styles`] of the workbook.
pub(crate) fn parse_worksheet_rules(sheet_xml: &str, dxf: &[CfStyle]) -> SheetRules {
    let mut rules = SheetRules::default();
    if sheet_xml.contains("<conditionalFormatting") || sheet_xml.contains("x14:cfRule") {
        parse_conditional_formatting(sheet_xml, dxf, &mut rules);
    }
    if sheet_xml.contains("<dataValidation") || sheet_xml.contains("x14:dataValidation") {
        parse_validations(sheet_xml, &mut rules);
    }
    rules.cf.sort_by_key(|r| r.priority);
    rules
}

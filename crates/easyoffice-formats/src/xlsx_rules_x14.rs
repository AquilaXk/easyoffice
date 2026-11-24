//! MS-XLSX `x14:conditionalFormattings` / `x14:dataBar` extension reading and writing for
//! [`SheetRules`] (PD-347).
//!
//! Base ECMA-376 `<dataBar>` stores only positive color, bounds, and `showValue`. Extended
//! settings (negative fill, solid/gradient, bar direction, border colors, axis placement, axis
//! color) live under `<ext uri="{78C0D932-649D-46B8-A469-E5A362C6BD93}">` linked by `<x14:id>`
//! inside `<cfRule><extLst><ext uri="{B025F937-C7B1-4748-B12E-C04C6255231F}">`. All untrusted
//! counts and identifier lengths are bounded cleanly.

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use crate::rtf_xml_helpers::{
    extract_attr_from_fragment, extract_tag_bodies, extract_tags_or_self_closing_with_attrs_limit,
};
use crate::sheet_styling::sheet_rules::{
    format_sqref, parse_sqref, CellBox, CfBarAxis, CfBarDirection, CfKind, CfRule, CfStyle,
    CfValue, SheetRules, ValueKind, DEFAULT_BAR_AXIS_COLOR, DEFAULT_BAR_NEGATIVE,
    DEFAULT_BAR_POSITIVE, MAX_RULES_PER_SHEET, MAX_RULE_STOPS, MAX_RULE_TEXT_CHARS,
};
use crate::XmlSanitizer;

const X14_NS: &str = concat!(
    "http://schemas.micro",
    "soft.com/office/spreadsheetml/2009/9/ac"
);
const XM_NS: &str = concat!(
    "http://schemas.micro",
    "soft.com/office/ex",
    "cel/2006/main"
);
const X14_CF_EXT_URI: &str = "{78C0D932-649D-46B8-A469-E5A362C6BD93}";
const CF_RULE_ID_EXT_URI: &str = "{B025F937-C7B1-4748-B12E-C04C6255231F}";
const MAX_X14_BLOCKS: usize = MAX_RULES_PER_SHEET * 2;
const MAX_X14_RULES: usize = MAX_RULES_PER_SHEET * 4;
const MAX_X14_ID_LEN: usize = 64;

#[derive(Debug, Clone)]
pub(crate) struct X14DataBar {
    pub positive_fill: Option<u32>,
    pub negative: Option<u32>,
    pub negative_same: bool,
    pub gradient: bool,
    pub show_value: Option<bool>,
    pub direction: CfBarDirection,
    pub border: bool,
    pub border_positive: Option<u32>,
    pub border_negative: Option<u32>,
    pub negative_border_same: bool,
    pub axis_position: CfBarAxis,
    pub axis_color: u32,
    pub min: Option<CfValue>,
    pub max: Option<CfValue>,
    pub priority: u32,
    pub ranges: Vec<CellBox>,
}

pub(crate) struct X14ParseResult {
    by_id: HashMap<String, Result<X14DataBar, String>>,
    order: Vec<Option<String>>,
    standalone: Vec<Result<CfRule, String>>,
    consumed: HashSet<String>,
}

fn attr(attrs: &str, name: &str) -> Option<String> {
    extract_attr_from_fragment(attrs, name).map(XmlSanitizer::unescape)
}

fn flag_attr(attrs: &str, name: &str, default: bool) -> Result<bool, String> {
    match extract_attr_from_fragment(attrs, name) {
        None => Ok(default),
        Some("0" | "false") => Ok(false),
        Some("1" | "true") => Ok(true),
        Some(_) => Err("dataBar".to_string()),
    }
}

fn opt_flag_attr(attrs: &str, name: &str) -> Result<Option<bool>, String> {
    match extract_attr_from_fragment(attrs, name) {
        None => Ok(None),
        Some("0" | "false") => Ok(Some(false)),
        Some("1" | "true") => Ok(Some(true)),
        Some(_) => Err("dataBar".to_string()),
    }
}

fn rgb_of_tag(body: &str, tag: &str) -> Result<Option<u32>, String> {
    let Some((attrs, _)) = extract_tags_or_self_closing_with_attrs_limit(body, tag, 1)
        .into_iter()
        .next()
    else {
        return Ok(None);
    };
    let Some(rgb) = extract_attr_from_fragment(attrs, "rgb") else {
        return Ok(None);
    };
    if !(rgb.len() == 6 || rgb.len() == 8) || !rgb.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("dataBar".to_string());
    }
    let hex = &rgb[rgb.len() - 6..];
    u32::from_str_radix(hex, 16)
        .map(Some)
        .map_err(|_| "dataBar".to_string())
}

fn valid_x14_id(raw: &str) -> Option<String> {
    let id = raw.trim();
    if id.is_empty() || id.len() > MAX_X14_ID_LEN {
        return None;
    }
    if id.bytes().any(|b| {
        !b.is_ascii()
            || b.is_ascii_control()
            || b.is_ascii_whitespace()
            || matches!(b, b'<' | b'>' | b'"' | b'\'' | b'&')
    }) {
        return None;
    }
    Some(id.to_string())
}

fn bounded(text: String) -> String {
    if text.chars().count() > MAX_RULE_TEXT_CHARS {
        text.chars().take(MAX_RULE_TEXT_CHARS).collect()
    } else {
        text
    }
}

fn x14_cfvo_xml(v: &CfValue) -> String {
    let kind = v.kind.name();
    if v.text.is_empty() {
        format!("<x14:cfvo type=\"{kind}\"/>")
    } else {
        let body = crate::xlsx_formula_text::to_file_formula(v.text.trim_start_matches('='));
        let escaped =
            XmlSanitizer::escape(&body.chars().take(MAX_RULE_TEXT_CHARS).collect::<String>());
        format!("<x14:cfvo type=\"{kind}\"><xm:f>{escaped}</xm:f></x14:cfvo>")
    }
}

fn parse_x14_cfvo(body: &str) -> Vec<CfValue> {
    extract_tags_or_self_closing_with_attrs_limit(body, "x14:cfvo", MAX_RULE_STOPS)
        .into_iter()
        .filter_map(|(attrs, inner)| {
            let kind = ValueKind::from_name(extract_attr_from_fragment(attrs, "type")?)?;
            let text = extract_tag_bodies(inner, "xm:f")
                .into_iter()
                .next()
                .map(|raw| {
                    bounded(crate::xlsx_formula_text::from_file_formula(
                        &XmlSanitizer::unescape(raw),
                    ))
                })
                .unwrap_or_default();
            Some(CfValue::new(kind, text))
        })
        .collect()
}

pub(crate) fn x14_rule_id(ordinal: usize) -> String {
    format!("{{E0347CF0-0000-4000-8000-{ordinal:012X}}}")
}

pub(crate) fn cf_rule_id_ext_xml(ordinal: usize) -> String {
    format!(
        "<extLst><ext uri=\"{CF_RULE_ID_EXT_URI}\" xmlns:x14=\"{X14_NS}\"><x14:id>{}</x14:id></ext></extLst>",
        x14_rule_id(ordinal)
    )
}

pub(crate) fn extract_rule_ext_id(cf_rule_body: &str) -> Option<String> {
    for (attrs, ext_body) in extract_tags_or_self_closing_with_attrs_limit(cf_rule_body, "ext", 8) {
        if extract_attr_from_fragment(attrs, "uri") == Some(CF_RULE_ID_EXT_URI) {
            if let Some(raw) = extract_tag_bodies(ext_body, "x14:id").into_iter().next() {
                if let Some(id) = valid_x14_id(&XmlSanitizer::unescape(raw)) {
                    return Some(id);
                }
            }
        }
    }
    extract_tag_bodies(cf_rule_body, "x14:id")
        .into_iter()
        .next()
        .and_then(|raw| valid_x14_id(&XmlSanitizer::unescape(raw)))
}

pub(crate) fn cf_ext_inner_xml(rules: &SheetRules) -> String {
    let mut ordered: Vec<&CfRule> = rules.cf.iter().collect();
    ordered.sort_by_key(|r| r.priority);
    let mut blocks = String::new();
    for (i, rule) in ordered.into_iter().enumerate() {
        if rule.ranges.is_empty() {
            continue;
        }
        let CfKind::DataBar {
            min,
            max,
            positive,
            negative,
            gradient,
            show_value,
            direction,
            border,
            border_positive,
            border_negative,
            axis_position,
            axis_color,
        } = &rule.kind
        else {
            continue;
        };
        let id = x14_rule_id(i + 1);
        let sqref = format_sqref(&rule.ranges);
        let mut attrs = String::from(" minLength=\"0\" maxLength=\"100\"");
        if *border {
            attrs.push_str(" border=\"1\"");
        }
        if !gradient {
            attrs.push_str(" gradient=\"0\"");
        }
        if *direction != CfBarDirection::Context {
            let _ = write!(attrs, " direction=\"{}\"", direction.name());
        }
        if !show_value {
            attrs.push_str(" showValue=\"0\"");
        }
        attrs.push_str(" negativeBarBorderColorSameAsPositive=\"0\"");
        if *axis_position != CfBarAxis::Automatic {
            let _ = write!(attrs, " axisPosition=\"{}\"", axis_position.name());
        }
        let _ = write!(
            blocks,
            "<x14:conditionalFormatting><x14:cfRule type=\"dataBar\" id=\"{id}\"><x14:dataBar{attrs}>{}{}<x14:fillColor rgb=\"FF{positive:06X}\"/><x14:borderColor rgb=\"FF{border_positive:06X}\"/><x14:negativeFillColor rgb=\"FF{negative:06X}\"/><x14:negativeBorderColor rgb=\"FF{border_negative:06X}\"/><x14:axisColor rgb=\"FF{axis_color:06X}\"/></x14:dataBar></x14:cfRule><xm:sqref>{sqref}</xm:sqref></x14:conditionalFormatting>",
            x14_cfvo_xml(min),
            x14_cfvo_xml(max),
        );
    }
    if blocks.is_empty() {
        return String::new();
    }
    format!(
        "<ext uri=\"{X14_CF_EXT_URI}\" xmlns:x14=\"{X14_NS}\" xmlns:xm=\"{XM_NS}\"><x14:conditionalFormattings>{blocks}</x14:conditionalFormattings></ext>"
    )
}

fn parse_one_x14_bar(
    rule_attrs: &str,
    rule_body: &str,
    ranges: Vec<CellBox>,
) -> Result<X14DataBar, String> {
    let Some((bar_attrs, bar_body)) =
        extract_tags_or_self_closing_with_attrs_limit(rule_body, "x14:dataBar", 1)
            .into_iter()
            .next()
    else {
        return Err("dataBar".to_string());
    };
    let direction = match attr(bar_attrs, "direction") {
        None => CfBarDirection::Context,
        Some(token) => CfBarDirection::from_name(&token).ok_or_else(|| "dataBar".to_string())?,
    };
    let axis_position = match attr(bar_attrs, "axisPosition") {
        None => CfBarAxis::Automatic,
        Some(token) => CfBarAxis::from_name(&token).ok_or_else(|| "dataBar".to_string())?,
    };
    let mut stops = parse_x14_cfvo(bar_body).into_iter();
    let min = stops.next();
    let max = stops.next();
    let negative_same = flag_attr(bar_attrs, "negativeBarColorSameAsPositive", false)?;
    // MS-XLSX 2.6.136 default for `negativeBarBorderColorSameAsPositive` is true.
    let negative_border_same = flag_attr(bar_attrs, "negativeBarBorderColorSameAsPositive", true)?;
    let show_value = opt_flag_attr(bar_attrs, "showValue")?;
    Ok(X14DataBar {
        positive_fill: rgb_of_tag(bar_body, "x14:fillColor")?,
        negative: rgb_of_tag(bar_body, "x14:negativeFillColor")?,
        negative_same,
        gradient: flag_attr(bar_attrs, "gradient", true)?,
        show_value,
        direction,
        border: flag_attr(bar_attrs, "border", false)?,
        border_positive: rgb_of_tag(bar_body, "x14:borderColor")?,
        border_negative: rgb_of_tag(bar_body, "x14:negativeBorderColor")?,
        negative_border_same,
        axis_position,
        axis_color: rgb_of_tag(bar_body, "x14:axisColor")?.unwrap_or(DEFAULT_BAR_AXIS_COLOR),
        min,
        max,
        priority: attr(rule_attrs, "priority")
            .and_then(|p| p.parse().ok())
            .unwrap_or(u32::MAX),
        ranges,
    })
}

pub(crate) fn count_unsupported_x14_rules(sheet_xml: &str) -> usize {
    if !sheet_xml.contains("<x14:cfRule") {
        return 0;
    }
    extract_tags_or_self_closing_with_attrs_limit(sheet_xml, "x14:cfRule", MAX_X14_RULES)
        .into_iter()
        .filter(|(attrs, _)| extract_attr_from_fragment(attrs, "type") != Some("dataBar"))
        .take(MAX_RULES_PER_SHEET)
        .count()
}

pub(crate) fn parse_x14_data_bars(sheet_xml: &str) -> X14ParseResult {
    let mut out = X14ParseResult {
        by_id: HashMap::new(),
        order: Vec::new(),
        standalone: Vec::new(),
        consumed: HashSet::new(),
    };
    if !sheet_xml.contains("<x14:cfRule") {
        return out;
    }
    let mut seen_rules = 0usize;
    for (_, block_body) in extract_tags_or_self_closing_with_attrs_limit(
        sheet_xml,
        "x14:conditionalFormatting",
        MAX_X14_BLOCKS,
    ) {
        if seen_rules >= MAX_X14_RULES {
            break;
        }
        let sqref_text = extract_tag_bodies(block_body, "xm:sqref")
            .into_iter()
            .next()
            .map(XmlSanitizer::unescape)
            .unwrap_or_default();
        let ranges = parse_sqref(&sqref_text);
        let rem = MAX_X14_RULES.saturating_sub(seen_rules);
        for (rule_attrs, rule_body) in
            extract_tags_or_self_closing_with_attrs_limit(block_body, "x14:cfRule", rem)
        {
            seen_rules += 1;
            if extract_attr_from_fragment(rule_attrs, "type") != Some("dataBar") {
                continue;
            }
            let raw_id = extract_attr_from_fragment(rule_attrs, "id").map(XmlSanitizer::unescape);
            let parsed = parse_one_x14_bar(rule_attrs, rule_body, ranges.clone());
            if let Some(raw) = raw_id {
                let Some(id) = valid_x14_id(&raw) else {
                    out.standalone.push(Err("조건부 서식 dataBar".to_string()));
                    continue;
                };
                if out.by_id.contains_key(&id) {
                    out.standalone.push(Err("조건부 서식 dataBar".to_string()));
                    continue;
                }
                out.order.push(Some(id.clone()));
                out.by_id.insert(id, parsed);
            } else {
                match parsed {
                    Ok(ext) => match standalone_rule(&ext) {
                        Some(rule) => out.standalone.push(Ok(rule)),
                        None => out.standalone.push(Err("조건부 서식 dataBar".to_string())),
                    },
                    Err(e) => out.standalone.push(Err(format!("조건부 서식 {e}"))),
                }
            }
        }
    }
    out
}

fn resolve_extended(base_pos: u32, ext: &X14DataBar) -> (u32, u32, u32, u32) {
    let positive = ext.positive_fill.unwrap_or(base_pos);
    let negative = if ext.negative_same {
        positive
    } else {
        ext.negative.unwrap_or(DEFAULT_BAR_NEGATIVE)
    };
    let border_positive = ext.border_positive.unwrap_or(positive);
    let border_negative = if ext.negative_border_same {
        border_positive
    } else {
        ext.border_negative.unwrap_or(negative)
    };
    (positive, negative, border_positive, border_negative)
}

fn standalone_rule(ext: &X14DataBar) -> Option<CfRule> {
    if ext.ranges.is_empty() {
        return None;
    }
    let min = ext.min.clone()?;
    let max = ext.max.clone()?;
    let (positive, negative, border_positive, border_negative) =
        resolve_extended(DEFAULT_BAR_POSITIVE, ext);
    Some(CfRule {
        ranges: ext.ranges.clone(),
        priority: ext.priority,
        stop_if_true: false,
        kind: CfKind::DataBar {
            min,
            max,
            positive,
            negative,
            gradient: ext.gradient,
            show_value: ext.show_value.unwrap_or(true),
            direction: ext.direction,
            border: ext.border,
            border_positive,
            border_negative,
            axis_position: ext.axis_position,
            axis_color: ext.axis_color,
        },
        style: CfStyle::default(),
    })
}

impl X14ParseResult {
    pub(crate) fn apply_to_bar(
        &mut self,
        cf_rule_body: &str,
        kind: &mut CfKind,
    ) -> Result<(), String> {
        let Some(id) = extract_rule_ext_id(cf_rule_body) else {
            return Ok(());
        };
        let Some(entry) = self.by_id.get(&id) else {
            return Ok(());
        };
        self.consumed.insert(id);
        let ext = entry.as_ref().map_err(|e| e.clone())?;
        if let CfKind::DataBar {
            positive,
            negative,
            gradient,
            show_value,
            direction,
            border,
            border_positive,
            border_negative,
            axis_position,
            axis_color,
            ..
        } = kind
        {
            let (pos, neg, bpos, bneg) = resolve_extended(*positive, ext);
            *positive = pos;
            *negative = neg;
            *gradient = ext.gradient;
            if let Some(show) = ext.show_value {
                *show_value = show;
            }
            *direction = ext.direction;
            *border = ext.border;
            *border_positive = bpos;
            *border_negative = bneg;
            *axis_position = ext.axis_position;
            *axis_color = ext.axis_color;
        }
        Ok(())
    }

    pub(crate) fn take_standalone(&mut self) -> Vec<Result<CfRule, String>> {
        let mut out = Vec::new();
        for id in self.order.iter().flatten() {
            if self.consumed.contains(id) {
                continue;
            }
            if let Some(entry) = self.by_id.get(id) {
                match entry {
                    Ok(ext) => match standalone_rule(ext) {
                        Some(rule) => out.push(Ok(rule)),
                        None => out.push(Err("조건부 서식 dataBar".to_string())),
                    },
                    Err(e) => out.push(Err(format!("조건부 서식 {e}"))),
                }
            }
        }
        out.append(&mut self.standalone);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheet_styling::sheet_rules::xlsx_rules::parse_worksheet_rules;

    #[test]
    fn hostile_excess_unsupported_rules_and_x14_ids_are_bounded() {
        let mut xml = String::from("<worksheet><conditionalFormatting sqref=\"A1\">");
        for _ in 0..400 {
            xml.push_str(r#"<cfRule type="unknownVendorRule" priority="1"/>"#);
        }
        xml.push_str("</conditionalFormatting>");
        for _ in 0..400 {
            xml.push_str("<x14:cfRule type=\"iconSet\"/>");
        }
        xml.push_str("<dataValidations>");
        for _ in 0..400 {
            xml.push_str(r#"<dataValidation type="customBad" sqref="A1"/>"#);
        }
        xml.push_str("</dataValidations>");
        for _ in 0..400 {
            xml.push_str("<x14:dataValidation sqref=\"A1\"/>");
        }
        let oversized_id = "A".repeat(200);
        xml.push_str(&format!(
            "<extLst><ext uri=\"{X14_CF_EXT_URI}\"><x14:conditionalFormattings><x14:conditionalFormatting><x14:cfRule type=\"dataBar\" id=\"{oversized_id}\"><x14:dataBar><x14:cfvo type=\"min\"/><x14:cfvo type=\"max\"/></x14:dataBar></x14:cfRule><xm:sqref>A1</xm:sqref></x14:conditionalFormatting></x14:conditionalFormattings></ext></extLst>"
        ));
        xml.push_str("</worksheet>");
        let rules = parse_worksheet_rules(&xml, &[]);
        assert!(rules.unsupported.len() <= MAX_RULES_PER_SHEET);
        assert!(
            rules.cf.is_empty(),
            "oversized x14 id must not be promoted to a standalone rule"
        );
    }

    #[test]
    fn x14_reordered_attrs_duplicates_and_malformed_attrs_fail_closed() {
        let valid_reordered = concat!(
            r#"<worksheet>"#,
            r#"<extLst><ext uri="{78C0D932-649D-46B8-A469-E5A362C6BD93}"><x14:conditionalFormattings>"#,
            r#"<x14:conditionalFormatting><x14:cfRule id="{AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA}" type="dataBar"><x14:dataBar border="1" axisPosition="midpoint"><x14:cfvo type="min"/><x14:cfvo type="max"/><x14:fillColor rgb="FF112233"/><x14:axisColor rgb="FF445566"/></x14:dataBar></x14:cfRule><xm:sqref>A1:A5</xm:sqref></x14:conditionalFormatting>"#,
            r#"</x14:conditionalFormattings></ext></extLst></worksheet>"#,
        );
        let parsed = parse_worksheet_rules(valid_reordered, &[]);
        assert_eq!(parsed.cf.len(), 1);
        assert!(
            parsed.unsupported.is_empty(),
            "reordered x14 dataBar attributes must not emit iconSet warning: {:?}",
            parsed.unsupported
        );

        let oversized_id = "A".repeat(200);
        let hostile = format!(
            concat!(
                r#"<worksheet>"#,
                r#"<extLst><ext uri="{{78C0D932-649D-46B8-A469-E5A362C6BD93}}"><x14:conditionalFormattings>"#,
                r#"<x14:conditionalFormatting><x14:cfRule type="dataBar" id="{{BBBBBBBB-BBBB-BBBB-BBBB-BBBBBBBBBBBB}}"><x14:dataBar><x14:cfvo type="min"/><x14:cfvo type="max"/></x14:dataBar></x14:cfRule><xm:sqref>B1:B2</xm:sqref></x14:conditionalFormatting>"#,
                r#"<x14:conditionalFormatting><x14:cfRule type="dataBar" id="{{BBBBBBBB-BBBB-BBBB-BBBB-BBBBBBBBBBBB}}"><x14:dataBar><x14:cfvo type="min"/><x14:cfvo type="max"/></x14:dataBar></x14:cfRule><xm:sqref>C1:C2</xm:sqref></x14:conditionalFormatting>"#,
                r#"<x14:conditionalFormatting><x14:cfRule type="dataBar" id="bad id"><x14:dataBar><x14:cfvo type="min"/><x14:cfvo type="max"/></x14:dataBar></x14:cfRule><xm:sqref>D1:D2</xm:sqref></x14:conditionalFormatting>"#,
                r#"<x14:conditionalFormatting><x14:cfRule type="dataBar" id="{}"><x14:dataBar><x14:cfvo type="min"/><x14:cfvo type="max"/></x14:dataBar></x14:cfRule><xm:sqref>G1:G2</xm:sqref></x14:conditionalFormatting>"#,
                r#"<x14:conditionalFormatting><x14:cfRule type="dataBar" id="{{CCCCCCCC-CCCC-CCCC-CCCC-CCCCCCCCCCCC}}"><x14:dataBar border="bogus"><x14:cfvo type="min"/><x14:cfvo type="max"/></x14:dataBar></x14:cfRule><xm:sqref>E1:E2</xm:sqref></x14:conditionalFormatting>"#,
                r#"<x14:conditionalFormatting><x14:cfRule type="dataBar" id="{{DDDDDDDD-DDDD-DDDD-DDDD-DDDDDDDDDDDD}}"><x14:dataBar><x14:cfvo type="min"/><x14:cfvo type="max"/><x14:axisColor rgb="+12345"/></x14:dataBar></x14:cfRule><xm:sqref>F1:F2</xm:sqref></x14:conditionalFormatting>"#,
                r#"</x14:conditionalFormattings></ext></extLst></worksheet>"#,
            ),
            oversized_id
        );
        let rejected = parse_worksheet_rules(&hostile, &[]);
        assert_eq!(rejected.cf.len(), 1);
        assert_eq!(rejected.cf[0].ranges[0].col0, 1);
        assert!(
            rejected.unsupported.len() >= 5,
            "{:?}",
            rejected.unsupported
        );
        assert!(rejected.unsupported.iter().all(|u| u.contains("dataBar")));
    }
}

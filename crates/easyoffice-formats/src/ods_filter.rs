//! The conditions of an auto filter in ODS: the `table:filter` element of a database range
//! (OASIS ODF 1.3 9.5.2 `table:filter`, 9.5.3 `table:filter-and`, 9.5.4 `table:filter-or`, 9.5.5
//! `table:filter-condition`, 9.5.6 `table:filter-set-item`, 19.688 `table:operator`). The model is
//! `sheet_filter`; the sheet's own filter and a table's filter share this codec (PD-226).
//!
//! A column's value list is a condition with one `table:filter-set-item` per value (an empty item for
//! the empty cells), its custom conditions are `table:filter-condition` elements (`=`, `!=`, `<`,
//! `<=`, `>`, `>=`, `begins`, `contains`, `ends` with their `!` forms, `empty`, `!empty`, and `match` /
//! `!match` with a regular expression for a pattern that has other wildcards), joined by
//! `table:filter-or` when the column's two conditions are alternatives, and its top and bottom filter
//! is `top values`, `bottom values`, `top percent` or `bottom percent`. The columns are joined by one
//! `table:filter-and`.
//!
//! The format has no colour filter, date group or above/below-average condition: such a column is not
//! written, and the rows it hid stay hidden through the rows' `table:visibility="filter"`. A filter
//! the model cannot hold (columns joined by OR, a condition with an operator or a regular expression
//! outside what the model says, a nested group beyond two levels) loses its conditions on reading; the
//! range, and the rows the file marks as filtered, stay.

use crate::sheet_filter::{
    CustomCondition, CustomOp, FilterColumn, FilterCriteria, MAX_FILTER_COLUMNS,
    MAX_FILTER_TEXT_CHARS, MAX_FILTER_VALUES, MAX_TOP_ITEMS, MAX_TOP_PERCENT,
};
use crate::xml_stream::{Attrs, XmlEvent, XmlReader};
use crate::{CellBox, XmlSanitizer};
use std::fmt::Write;

/// Elements of a filter condition tree one file may list before the filter is dropped.
const MAX_NODES: usize = 100_000;
/// Elements a condition tree may nest: the schema's `filter-and`, `filter-or`, `filter-condition` and set
/// item need four, so a deeper tree is not one this model holds and is refused before it is built (a
/// tree of tens of thousands of levels would overflow the stack when it is dropped).
const MAX_DEPTH: usize = 8;

// ---- wildcard patterns <-> literal text and regular expressions ------------------------------------------

/// The text a wildcard pattern stands for when it has no wildcard (`~` escapes `*`, `?` and `~`).
fn literal_of(pattern: &str) -> Option<String> {
    let mut out = String::with_capacity(pattern.len());
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        match c {
            '*' | '?' => return None,
            '~' => out.push(chars.next().unwrap_or('~')),
            c => out.push(c),
        }
    }
    Some(out)
}

/// The pattern that matches exactly `text`.
fn escape_pattern(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '*' | '?' | '~') {
            out.push('~');
        }
        out.push(c);
    }
    out
}

const REGEX_META: &str = "\\.+*?()|[]{}^$";

/// The anchored regular expression that matches what `pattern` matches.
fn pattern_to_regex(pattern: &str) -> String {
    let mut out = String::from("^");
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        match c {
            '*' => out.push_str(".*"),
            '?' => out.push('.'),
            c => {
                let lit = if c == '~' {
                    chars.next().unwrap_or('~')
                } else {
                    c
                };
                if REGEX_META.contains(lit) {
                    out.push('\\');
                }
                out.push(lit);
            }
        }
    }
    out.push('$');
    out
}

/// The wildcard pattern a regular expression of the shapes `pattern_to_regex` writes stands for:
/// `.*`, `.`, escaped and plain characters, with or without the anchors. `None` for any other regular
/// expression.
fn regex_to_pattern(regex: &str) -> Option<String> {
    let body = regex.strip_prefix('^').unwrap_or(regex);
    let body = match body.strip_suffix('$') {
        Some(rest) if !rest.ends_with('\\') => rest,
        _ => body,
    };
    let mut out = String::new();
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '.' if chars.peek() == Some(&'*') => {
                chars.next();
                out.push('*');
            }
            '.' => out.push('?'),
            '\\' => {
                let lit = chars.next()?;
                out.push_str(&escape_pattern(&lit.to_string()));
            }
            c if REGEX_META.contains(c) => return None,
            c => out.push_str(&escape_pattern(&c.to_string())),
        }
    }
    Some(out)
}

// ---- writing --------------------------------------------------------------------------------------------

/// One `table:filter-condition` before it is written.
struct Cond {
    operator: &'static str,
    value: String,
    number: bool,
}

fn text_cond(operator: &'static str, value: impl Into<String>) -> Cond {
    Cond {
        operator,
        value: value.into(),
        number: false,
    }
}

/// The ODS condition of a pattern, with `not` for the negated operators.
fn pattern_cond(pattern: &str, not: bool) -> Cond {
    let pick = |plain: &'static str, negated: &'static str| if not { negated } else { plain };
    if pattern.is_empty() || (not && pattern == " ") {
        return text_cond(pick("empty", "!empty"), "");
    }
    if let Some(inner) = pattern
        .strip_prefix('*')
        .and_then(|p| p.strip_suffix('*'))
        .filter(|_| pattern.len() >= 2)
    {
        if let Some(lit) = literal_of(inner) {
            return text_cond(pick("contains", "!contains"), lit);
        }
    }
    if let Some(lit) = pattern.strip_suffix('*').and_then(literal_of) {
        return text_cond(pick("begins", "!begins"), lit);
    }
    if let Some(lit) = pattern.strip_prefix('*').and_then(literal_of) {
        return text_cond(pick("ends", "!ends"), lit);
    }
    match literal_of(pattern) {
        Some(lit) => text_cond(pick("=", "!="), lit),
        None => text_cond(pick("match", "!match"), pattern_to_regex(pattern)),
    }
}

fn condition_of(c: &CustomCondition) -> Cond {
    let number = |operator: &'static str| Cond {
        operator,
        value: c.text.clone(),
        number: true,
    };
    match c.op {
        None => pattern_cond(&c.text, false),
        Some(CustomOp::Equal) => number("="),
        Some(CustomOp::NotEqual) if crate::sheet_filter::plain_number(&c.text) => number("!="),
        Some(CustomOp::NotEqual) => pattern_cond(&c.text, true),
        Some(CustomOp::GreaterThan) => number(">"),
        Some(CustomOp::GreaterThanOrEqual) => number(">="),
        Some(CustomOp::LessThan) => number("<"),
        Some(CustomOp::LessThanOrEqual) => number("<="),
    }
}

fn push_condition(out: &mut String, field: u32, cond: &Cond) {
    let _ = write!(
        out,
        "<table:filter-condition table:field-number=\"{field}\" table:value=\"{}\" table:operator=\"{}\" table:case-sensitive=\"false\" table:data-type=\"{}\"/>",
        XmlSanitizer::escape(&cond.value),
        XmlSanitizer::escape(cond.operator),
        if cond.number { "number" } else { "text" }
    );
}

/// The element of one column, `None` for a criterion the format cannot hold.
fn column_xml(field: u32, criteria: &FilterCriteria) -> Option<String> {
    let mut out = String::new();
    match criteria {
        FilterCriteria::Values { blank, values, .. } => {
            if values.is_empty() && !blank {
                return None;
            }
            let _ = write!(
                out,
                "<table:filter-condition table:field-number=\"{field}\" table:value=\"\" table:operator=\"=\" table:case-sensitive=\"false\" table:data-type=\"text\">"
            );
            if *blank {
                out.push_str("<table:filter-set-item table:value=\"\"/>");
            }
            for value in values {
                let _ = write!(
                    out,
                    "<table:filter-set-item table:value=\"{}\"/>",
                    XmlSanitizer::escape(value)
                );
            }
            out.push_str("</table:filter-condition>");
        }
        FilterCriteria::Custom { and, first, second } => match second {
            None => push_condition(&mut out, field, &condition_of(first)),
            // An AND is the parent's own list (a `table:filter-and` may not hold another one); an OR is a group.
            Some(second) => {
                if !*and {
                    out.push_str("<table:filter-or>");
                }
                push_condition(&mut out, field, &condition_of(first));
                push_condition(&mut out, field, &condition_of(second));
                if !*and {
                    out.push_str("</table:filter-or>");
                }
            }
        },
        FilterCriteria::Top {
            bottom,
            percent,
            count,
        } => {
            let operator = match (bottom, percent) {
                (false, false) => "top values",
                (false, true) => "top percent",
                (true, false) => "bottom values",
                (true, true) => "bottom percent",
            };
            let cond = Cond {
                operator,
                value: count.to_string(),
                number: true,
            };
            push_condition(&mut out, field, &cond);
        }
        FilterCriteria::Average { .. } | FilterCriteria::Color { .. } => return None,
    }
    Some(out)
}

/// `<table:filter>` for the conditions of `columns` over `range`; empty when no column can be written.
pub(crate) fn filter_xml(range: &CellBox, columns: &[FilterColumn]) -> String {
    let parts: Vec<String> = columns
        .iter()
        .filter_map(|c| column_xml(c.col - range.col0, &c.criteria))
        .collect();
    if parts.is_empty() {
        return String::new();
    }
    format!(
        "<table:filter table:display-duplicates=\"true\"><table:filter-and>{}</table:filter-and></table:filter>",
        parts.concat()
    )
}

// ---- reading --------------------------------------------------------------------------------------------

enum Node {
    Cond {
        field: u32,
        value: String,
        operator: String,
        number: bool,
        items: Vec<String>,
    },
    And(Vec<Node>),
    Or(Vec<Node>),
}

fn condition_node(attrs: &Attrs<'_>) -> Option<Node> {
    Some(Node::Cond {
        field: attrs.get("table:field-number")?.trim().parse().ok()?,
        value: attrs
            .get("table:value")
            .map(|v| v.into_owned())
            .unwrap_or_default(),
        operator: attrs.get("table:operator")?.into_owned(),
        number: attrs.get("table:data-type").as_deref() == Some("number"),
        items: Vec::new(),
    })
}

/// The condition tree inside `filter_inner` (the content of a `table:filter`), `None` when it is
/// malformed or over the node budget.
fn parse_tree(filter_inner: &str) -> Option<Node> {
    let wrapped = format!("<r>{filter_inner}</r>");
    let mut reader = XmlReader::unbounded(&wrapped);
    // The groups being read, and the finished root.
    let mut stack: Vec<Node> = Vec::new();
    let mut root: Option<Node> = None;
    let mut nodes = 0usize;
    while let Ok(Some(event)) = reader.next_event() {
        match event {
            XmlEvent::Start { name, attrs } => {
                nodes += 1;
                if nodes > MAX_NODES || stack.len() >= MAX_DEPTH {
                    return None;
                }
                match name {
                    "table:filter-and" => stack.push(Node::And(Vec::new())),
                    "table:filter-or" => stack.push(Node::Or(Vec::new())),
                    "table:filter-condition" => stack.push(condition_node(&attrs)?),
                    "table:filter-set-item" => {
                        if let Some(Node::Cond { items, .. }) = stack.last_mut() {
                            if items.len() <= MAX_FILTER_VALUES {
                                items.push(attrs.get("table:value")?.into_owned());
                            }
                        }
                        // A set item closes at once; keep the stack shape for its End.
                        stack.push(Node::And(Vec::new()));
                    }
                    _ => {}
                }
            }
            XmlEvent::End(name) => {
                if !matches!(
                    name,
                    "table:filter-and"
                        | "table:filter-or"
                        | "table:filter-condition"
                        | "table:filter-set-item"
                ) {
                    continue;
                }
                let done = stack.pop()?;
                if name == "table:filter-set-item" {
                    continue;
                }
                match stack.last_mut() {
                    Some(Node::And(children) | Node::Or(children)) => children.push(done),
                    Some(Node::Cond { .. }) => return None,
                    None => root = Some(done),
                }
            }
            XmlEvent::Text(_) => {}
        }
    }
    root
}

/// The criteria of one condition, `None` for one the model cannot hold.
fn criteria_of(node: &Node) -> Option<FilterCriteria> {
    let Node::Cond {
        value,
        operator,
        number,
        items,
        ..
    } = node
    else {
        return None;
    };
    if !items.is_empty() {
        return Some(FilterCriteria::Values {
            blank: items.iter().any(String::is_empty),
            values: items.iter().filter(|v| !v.is_empty()).cloned().collect(),
            dates: Vec::new(),
        });
    }
    let custom = |op: Option<CustomOp>, text: String| {
        (text.chars().count() <= MAX_FILTER_TEXT_CHARS).then_some(FilterCriteria::Custom {
            and: false,
            first: CustomCondition { op, text },
            second: None,
        })
    };
    let not = |text: String| custom(Some(CustomOp::NotEqual), text);
    let numeric = |op: CustomOp| {
        crate::sheet_filter::plain_number(value)
            .then(|| custom(Some(op), value.trim().to_string()))
            .flatten()
    };
    match operator.as_str() {
        "=" if *number => numeric(CustomOp::Equal),
        "!=" if *number => numeric(CustomOp::NotEqual),
        "=" if value.is_empty() => custom(None, String::new()),
        "=" => custom(None, escape_pattern(value)),
        "!=" if value.is_empty() => not(" ".into()),
        "!=" => not(escape_pattern(value)),
        "<" => numeric(CustomOp::LessThan),
        "<=" => numeric(CustomOp::LessThanOrEqual),
        ">" => numeric(CustomOp::GreaterThan),
        ">=" => numeric(CustomOp::GreaterThanOrEqual),
        "contains" => custom(None, format!("*{}*", escape_pattern(value))),
        "!contains" => not(format!("*{}*", escape_pattern(value))),
        "begins" => custom(None, format!("{}*", escape_pattern(value))),
        "!begins" => not(format!("{}*", escape_pattern(value))),
        "ends" => custom(None, format!("*{}", escape_pattern(value))),
        "!ends" => not(format!("*{}", escape_pattern(value))),
        "empty" => custom(None, String::new()),
        "!empty" => not(" ".into()),
        "match" => custom(None, regex_to_pattern(value)?),
        "!match" => not(regex_to_pattern(value)?),
        "top values" | "bottom values" | "top percent" | "bottom percent" => {
            let percent = operator.ends_with("percent");
            let most = if percent {
                MAX_TOP_PERCENT
            } else {
                MAX_TOP_ITEMS
            };
            let count = value
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|v| v.fract() == 0.0 && (1.0..=f64::from(most)).contains(v))?;
            Some(FilterCriteria::Top {
                bottom: operator.starts_with("bottom"),
                percent,
                count: count as u32,
            })
        }
        _ => None,
    }
}

fn field_of(node: &Node) -> Option<u32> {
    match node {
        Node::Cond { field, .. } => Some(*field),
        _ => None,
    }
}

/// One column's criteria from its element: a condition, or two conditions of one field in an
/// AND or OR group.
fn column_of(node: &Node) -> Option<(u32, FilterCriteria)> {
    match node {
        Node::Cond { field, .. } => Some((*field, criteria_of(node)?)),
        Node::And(parts) | Node::Or(parts) => {
            let and = matches!(node, Node::And(_));
            let [a, b] = parts.as_slice() else {
                return None;
            };
            let field = field_of(a).filter(|f| Some(*f) == field_of(b))?;
            let (
                FilterCriteria::Custom { first, .. },
                FilterCriteria::Custom { first: second, .. },
            ) = (criteria_of(a)?, criteria_of(b)?)
            else {
                return None;
            };
            Some((
                field,
                FilterCriteria::Custom {
                    and,
                    first,
                    second: Some(second),
                },
            ))
        }
    }
}

/// The conditions of the `table:filter` whose content is `filter_inner`, over `range`: a column per
/// field, sorted, within the range. Empty when the tree is not one the model holds; a column whose
/// condition the model cannot hold is left out.
pub(crate) fn parse_filter(filter_inner: &str, range: &CellBox) -> Vec<FilterColumn> {
    let Some(root) = parse_tree(filter_inner) else {
        return Vec::new();
    };
    // Columns are joined by AND; a lone condition or OR group is one column.
    let parts: Vec<&Node> = match &root {
        Node::And(parts)
            if parts
                .iter()
                .all(|p| field_of(p).is_some() || matches!(p, Node::Or(_))) =>
        {
            parts.iter().collect()
        }
        other => vec![other],
    };
    let mut columns: Vec<FilterColumn> = Vec::new();
    // Two plain conditions of one field in an AND are that column's pair.
    let mut pending: Option<(u32, FilterCriteria)> = None;
    let flush = |columns: &mut Vec<FilterColumn>, entry: Option<(u32, FilterCriteria)>| {
        let Some((field, criteria)) = entry else {
            return;
        };
        let Some(col) = range.col0.checked_add(field).filter(|c| *c <= range.col1) else {
            return;
        };
        if columns.len() < MAX_FILTER_COLUMNS && !columns.iter().any(|c| c.col == col) {
            columns.push(FilterColumn { col, criteria });
        }
    };
    for part in parts {
        let Some(entry) = column_of(part) else {
            continue;
        };
        match (pending.take(), entry) {
            (
                Some((
                    pf,
                    FilterCriteria::Custom {
                        first,
                        second: None,
                        ..
                    },
                )),
                (
                    f,
                    FilterCriteria::Custom {
                        first: next,
                        second: None,
                        ..
                    },
                ),
            ) if pf == f && matches!(part, Node::Cond { .. }) => {
                flush(
                    &mut columns,
                    Some((
                        f,
                        FilterCriteria::Custom {
                            and: true,
                            first,
                            second: Some(next),
                        },
                    )),
                );
            }
            (previous, (f, criteria)) => {
                flush(&mut columns, previous);
                pending = Some((f, criteria));
            }
        }
    }
    flush(&mut columns, pending);
    columns.sort_by_key(|c| c.col);
    columns
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range() -> CellBox {
        CellBox::parse("B2:F20").unwrap()
    }

    fn custom(op: Option<CustomOp>, text: &str) -> CustomCondition {
        CustomCondition {
            op,
            text: text.into(),
        }
    }

    fn one(op: Option<CustomOp>, text: &str) -> FilterCriteria {
        FilterCriteria::Custom {
            and: false,
            first: custom(op, text),
            second: None,
        }
    }

    fn round(columns: Vec<FilterColumn>) -> Vec<FilterColumn> {
        let xml = filter_xml(&range(), &columns);
        XmlSanitizer::validate_safe_xml(&format!("<r xmlns:table=\"t\">{xml}</r>")).unwrap();
        let inner = xml
            .strip_prefix("<table:filter table:display-duplicates=\"true\">")
            .and_then(|x| x.strip_suffix("</table:filter>"))
            .unwrap_or("");
        parse_filter(inner, &range())
    }

    fn on(col: u32, criteria: FilterCriteria) -> FilterColumn {
        FilterColumn { col, criteria }
    }

    #[test]
    fn every_condition_the_format_holds_round_trips() {
        use CustomOp::*;
        let cases = vec![
            one(None, "plain"),
            one(None, ""),
            one(None, "*in*"),
            one(None, "pre*"),
            one(None, "*post"),
            one(None, "a~*b"),
            one(None, "a*b?c"),
            one(None, "x.y*(z)"),
            one(Some(NotEqual), "plain"),
            one(Some(NotEqual), " "),
            one(Some(NotEqual), "*in*"),
            one(Some(NotEqual), "pre*"),
            one(Some(NotEqual), "*post"),
            one(Some(NotEqual), "a*b?c"),
            one(Some(NotEqual), "5"),
            one(Some(Equal), "5.5"),
            one(Some(GreaterThan), "10"),
            one(Some(GreaterThanOrEqual), "-3"),
            one(Some(LessThan), "1e3"),
            one(Some(LessThanOrEqual), "0"),
            FilterCriteria::Custom {
                and: true,
                first: custom(Some(GreaterThan), "1"),
                second: Some(custom(Some(LessThan), "9")),
            },
            FilterCriteria::Custom {
                and: false,
                first: custom(None, "a*"),
                second: Some(custom(None, "*b")),
            },
            FilterCriteria::Values {
                blank: true,
                values: vec!["a&b".into(), "<x>".into(), "한글".into()],
                dates: Vec::new(),
            },
            FilterCriteria::Top {
                bottom: false,
                percent: false,
                count: 10,
            },
            FilterCriteria::Top {
                bottom: true,
                percent: true,
                count: 25,
            },
        ];
        for (i, criteria) in cases.into_iter().enumerate() {
            let columns = vec![on(1 + (i as u32 % 5), criteria)];
            assert_eq!(round(columns.clone()), columns, "case {i}");
        }
    }

    #[test]
    fn several_columns_are_joined_by_one_and_and_unwritable_ones_are_left_out() {
        let columns = vec![
            on(1, one(None, "a*")),
            on(
                3,
                FilterCriteria::Custom {
                    and: false,
                    first: custom(Some(CustomOp::LessThan), "1"),
                    second: Some(custom(Some(CustomOp::GreaterThan), "9")),
                },
            ),
            on(4, FilterCriteria::Average { above: true }),
            on(
                5,
                FilterCriteria::Color {
                    font: false,
                    color: Some(1),
                },
            ),
            on(
                2,
                FilterCriteria::Values {
                    blank: false,
                    values: Vec::new(),
                    dates: Vec::new(),
                },
            ),
        ];
        let mut sorted = columns.clone();
        sorted.sort_by_key(|c| c.col);
        let xml = filter_xml(&range(), &sorted);
        assert_eq!(xml.matches("<table:filter-and>").count(), 1);
        assert!(xml.contains("table:field-number=\"0\"") && xml.contains("<table:filter-or>"));
        let back = round(sorted);
        assert_eq!(back, vec![columns[0].clone(), columns[1].clone()]);
        assert_eq!(
            filter_xml(&range(), &[on(4, FilterCriteria::Average { above: true })]),
            ""
        );
    }

    #[test]
    fn a_file_written_by_another_program_is_read_by_the_schema() {
        let cond = |field, op: &str, value: &str, ty: &str| {
            format!("<table:filter-condition table:field-number=\"{field}\" table:value=\"{value}\" table:operator=\"{op}\" table:data-type=\"{ty}\"/>")
        };
        // A lone condition, an AND of two conditions on one field, and a set of values.
        let lone = parse_filter(&cond(1, ">=", "5", "number"), &range());
        assert_eq!(
            lone,
            vec![on(2, one(Some(CustomOp::GreaterThanOrEqual), "5"))]
        );
        let pair = parse_filter(
            &format!(
                "<table:filter-and>{}{}</table:filter-and>",
                cond(0, ">", "1", "number"),
                cond(0, "<", "9", "number")
            ),
            &range(),
        );
        assert_eq!(
            pair,
            vec![on(
                1,
                FilterCriteria::Custom {
                    and: true,
                    first: custom(Some(CustomOp::GreaterThan), "1"),
                    second: Some(custom(Some(CustomOp::LessThan), "9")),
                }
            )]
        );
        let set = parse_filter(
            "<table:filter-condition table:field-number=\"0\" table:value=\"x\" table:operator=\"=\" table:data-type=\"text\"><table:filter-set-item table:value=\"x\"/><table:filter-set-item table:value=\"\"/></table:filter-condition>",
            &range(),
        );
        assert_eq!(
            set,
            vec![on(
                1,
                FilterCriteria::Values {
                    blank: true,
                    values: vec!["x".into()],
                    dates: Vec::new()
                }
            )]
        );
        // A regular expression the model has no wildcard for, an operator it does not know, columns joined by
        // OR and a field outside the range leave no condition.
        for bad in [
            cond(0, "match", "^a(b|c)$", "text"),
            cond(0, "between", "1", "number"),
            cond(0, "top values", "0", "number"),
            cond(0, ">", "abc", "number"),
            cond(7, "=", "x", "text"),
            format!(
                "<table:filter-or>{}{}</table:filter-or>",
                cond(0, "=", "a", "text"),
                cond(1, "=", "b", "text")
            ),
            "<table:filter-and><table:filter-condition/></table:filter-and>".to_string(),
            "<table:filter-and>".to_string(),
            String::new(),
        ] {
            assert!(parse_filter(&bad, &range()).is_empty(), "{bad}");
        }
    }

    #[test]
    fn hostile_trees_are_bounded() {
        let one = "<table:filter-condition table:field-number=\"0\" table:value=\"a\" table:operator=\"=\" table:data-type=\"text\"/>";
        // A flood of conditions on distinct fields keeps at most the columns the range has.
        let many: String = (0..5000)
            .map(|i| one.replace("\"0\"", &format!("\"{i}\"")))
            .collect();
        let columns = parse_filter(
            &format!("<table:filter-and>{many}</table:filter-and>"),
            &range(),
        );
        assert_eq!(columns.len(), 5);
        // A tree over the node budget is refused whole.
        let flood = one.repeat(MAX_NODES + 1);
        assert!(parse_filter(
            &format!("<table:filter-and>{flood}</table:filter-and>"),
            &range()
        )
        .is_empty());
        // Deep nesting is refused before it is built: 100,000 levels hold no column and do not overflow the
        // stack, tested on a thread with a stack far smaller than a release build's.
        let deep = format!(
            "{}{}{}",
            "<table:filter-and>".repeat(100_000),
            one,
            "</table:filter-and>".repeat(100_000)
        );
        let found = std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(move || parse_filter(&deep, &range()).len())
            .unwrap()
            .join()
            .unwrap();
        assert_eq!(found, 0);
        let shallow = format!(
            "{}{}{}",
            "<table:filter-and>".repeat(3),
            one,
            "</table:filter-and>".repeat(3)
        );
        assert!(parse_filter(&shallow, &range()).len() <= 1);
        // A list of set items is cut at the bound.
        let items = "<table:filter-set-item table:value=\"v\"/>".repeat(MAX_FILTER_VALUES + 500);
        let set = format!("<table:filter-condition table:field-number=\"0\" table:value=\"\" table:operator=\"=\" table:data-type=\"text\">{items}</table:filter-condition>");
        let columns = parse_filter(&set, &range());
        let FilterCriteria::Values { values, .. } = &columns[0].criteria else {
            panic!("a value list");
        };
        assert_eq!(values.len(), MAX_FILTER_VALUES + 1);
    }

    #[test]
    fn regular_expressions_and_wildcards_agree() {
        assert_eq!(pattern_to_regex("a*b?c.d"), "^a.*b.c\\.d$");
        assert_eq!(regex_to_pattern("^a.*b.c\\.d$").as_deref(), Some("a*b?c.d"));
        assert_eq!(regex_to_pattern("abc").as_deref(), Some("abc"));
        assert_eq!(regex_to_pattern("a\\*b").as_deref(), Some("a~*b"));
        assert_eq!(regex_to_pattern("a+b"), None);
        assert_eq!(regex_to_pattern("[ab]"), None);
        assert_eq!(literal_of("a~*b~~"), Some("a*b~".into()));
        assert_eq!(literal_of("a*b"), None);
    }
}

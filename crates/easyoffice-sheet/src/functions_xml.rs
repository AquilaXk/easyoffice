//! `FILTERXML(xml, xpath)`: the values an XPath 1.0 query finds in an XML text.
//!
//! 참조 구현: none (the reference engines list the name and give an error), so the query runs on
//! crates: `sxd-document` (the XML parser and document model, MIT) and
//! `sxd-xpath` (XPath 1.0, MIT OR Apache-2.0), see `THIRD_PARTY_NOTICES.md`. The code here is the
//! adapter and the limits. Both text arguments come from a formula, so they are untrusted:
//! - the XML goes through the engine's own `XmlSanitizer::validate_safe_xml` first (a `DOCTYPE` or
//!   `ENTITY` declaration is refused), and a text of more than [`MAX_XML_DEPTH`] levels or
//!   [`MAX_XML_NODES`] nodes is refused before any query runs;
//! - `sxd-xpath` has no work budget (a query with nested scans such as
//!   `//i[count(//i[count(//i)>1])>1]` runs for minutes on 4,000 nodes), so a query of more than
//!   [`MAX_XPATH_CHARS`] characters, with more than [`MAX_XPATH_SCANS`] scanning steps, with a
//!   predicate inside a predicate, or with more than one scanning step inside a predicate is refused
//!   (the cost is then at most nodes squared);
//! - the answer has at most as many values as the document has nodes, and goes through the array
//!   budget.
//!
//! The function documentation says an XML text that is not valid is `#VALUE!`. It gives no other
//! rule. A query that finds nothing, a query that is not valid XPath and a refused input are
//! `#VALUE!` as well. A node's value is its XPath string value, one value per row; a number found
//! is a number, as when the cell is read back.

use crate::array::{Env, Grid, Val};
use crate::array_eval::ValResult;
use crate::coercion::parse_strict_number;
use crate::{EvalContext, EvaluatedCellValue as V, Expr, FormulaError, SheetEngine};
use easyoffice_formats::XmlSanitizer;
use sxd_document::dom::{ChildOfElement, ChildOfRoot, Document};
use sxd_document::parser;
use sxd_xpath::{evaluate_xpath, Value};

/// Deepest element nesting of an XML text.
const MAX_XML_DEPTH: usize = 128;
/// Elements, attributes and text nodes of an XML text.
const MAX_XML_NODES: usize = 2_000;
/// Longest query, in characters.
const MAX_XPATH_CHARS: usize = 256;
/// Scanning steps (`//`, `descendant::`, `following::`, ...) of one query.
const MAX_XPATH_SCANS: usize = 3;
/// Axes that visit more than the children of one node.
const SCANNING_AXES: [&str; 6] = [
    "descendant",
    "descendant-or-self",
    "following",
    "following-sibling",
    "preceding",
    "preceding-sibling",
];

/// Whether the query is cheap enough to run: see the limits above. Quoted text is skipped.
fn xpath_within_limits(xpath: &str) -> bool {
    if xpath.chars().count() > MAX_XPATH_CHARS {
        return false;
    }
    let bytes = xpath.as_bytes();
    let (mut scans, mut scans_in_predicate, mut predicate_depth) = (0usize, 0usize, 0usize);
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let mut scan = false;
        match b {
            b'\'' | b'"' => match bytes[i + 1..].iter().position(|&q| q == b) {
                Some(end) => i += end + 1,
                None => return false,
            },
            b'[' => {
                predicate_depth += 1;
                if predicate_depth > 1 {
                    return false;
                }
            }
            b']' => predicate_depth = predicate_depth.saturating_sub(1),
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                scan = true;
                i += 1;
            }
            b':' if bytes.get(i + 1) == Some(&b':') => {
                let start = bytes[..i]
                    .iter()
                    .rposition(|c| !(c.is_ascii_alphabetic() || *c == b'-'))
                    .map_or(0, |p| p + 1);
                scan = SCANNING_AXES.contains(&&xpath[start..i]);
                i += 1;
            }
            _ => {}
        }
        if scan {
            scans += 1;
            if predicate_depth > 0 {
                scans_in_predicate += 1;
            }
        }
        i += 1;
    }
    scans <= MAX_XPATH_SCANS && scans_in_predicate <= 1
}

/// Whether the document is small enough: its depth and node count, walked without recursion.
fn document_within_limits(doc: &Document<'_>) -> bool {
    let mut nodes = 0usize;
    let mut stack = Vec::new();
    for child in doc.root().children() {
        if let ChildOfRoot::Element(element) = child {
            stack.push((element, 1usize));
        }
    }
    while let Some((element, depth)) = stack.pop() {
        nodes += 1 + element.attributes().len();
        if depth > MAX_XML_DEPTH || nodes > MAX_XML_NODES {
            return false;
        }
        for child in element.children() {
            match child {
                ChildOfElement::Element(child) => stack.push((child, depth + 1)),
                ChildOfElement::Text(_) => nodes += 1,
                _ => {}
            }
        }
    }
    true
}

/// A found text as the cell it would be: a number when it reads as one.
fn found_value(text: String) -> V {
    match parse_strict_number(&text) {
        Some(number) => V::Number(number),
        None => V::Text(text),
    }
}

/// The values `xpath` finds in `xml`; an empty list when nothing is found.
fn filter_xml(xml: &str, xpath: &str) -> Result<Vec<V>, FormulaError> {
    if XmlSanitizer::validate_safe_xml(xml).is_err() || !xpath_within_limits(xpath) {
        return Err(FormulaError::Value);
    }
    let package = parser::parse(xml).map_err(|_| FormulaError::Value)?;
    let doc = package.as_document();
    if !document_within_limits(&doc) {
        return Err(FormulaError::Value);
    }
    match evaluate_xpath(&doc, xpath).map_err(|_| FormulaError::Value)? {
        Value::Nodeset(nodes) => Ok(nodes
            .document_order()
            .into_iter()
            .map(|node| found_value(node.string_value()))
            .collect()),
        Value::String(text) => Ok(vec![found_value(text)]),
        Value::Number(number) => Ok(vec![V::Number(number)]),
        Value::Boolean(flag) => Ok(vec![V::Boolean(flag)]),
    }
}

impl SheetEngine {
    /// `FILTERXML(xml, xpath)`: one value, or the values found as a column.
    pub(crate) fn fn_filterxml(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.len() != 2 {
            return Err(FormulaError::Value);
        }
        let mut text = |i: usize| match self.arg_single(args, i, None, env, ctx, depth)? {
            V::Error(e) => Err(e),
            v => Ok(v.to_display_string()),
        };
        let xml = text(0)?;
        let xpath = text(1)?;
        let found = filter_xml(&xml, &xpath)?;
        if found.is_empty() {
            return Err(FormulaError::Value);
        }
        let rows = found.len();
        Ok(Val::grid(Grid::from_rows_checked(ctx, rows, 1, found)?))
    }
}

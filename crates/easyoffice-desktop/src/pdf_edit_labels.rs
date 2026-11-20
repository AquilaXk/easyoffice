//! Page labels (`/Catalog /PageLabels`) of the sources in the result (PD-266).
//!
//! A label is what a viewer shows for a page instead of its place: "iv", "A-5". The file stores
//! them as ranges: a number tree whose keys are page indexes and whose values say how the pages from
//! that index on are numbered (style, prefix, first number). Take pages out, reorder them or join
//! files, and the ranges point at the wrong pages, so they cannot be copied. They are written
//! again from the pages the result has: the label of each page is worked out from its own file,
//! and a new range starts wherever a page does not continue the numbering of the one before it.
//! A file without labels is numbered in decimal by the place its pages had, which is what a viewer
//! shows for it, so a join does not renumber its pages.
//!
//! Library choice (the three steps of the implementation rule): (1) `hayro-syntax` offers no
//! reader for the tree; (2) the JavaScript PDF viewers read it for display only; (3) written
//! against ISO 32000-1 §12.4.2 (page labels) with the same limits as the other trees of the shell
//! (depth, nodes, repeated nodes), nothing was copied.

use crate::pdf_edit_budget::{MAX_LABEL_PREFIX_BYTES, MAX_LABEL_RANGES};
use crate::pdf_edit_tree::{self as tree, Walk};
use crate::pdf_edit_value::Value;
use crate::pdf_edit_wire::PagePlan;
use hayro_syntax::object::{Array, Dict, Name, Object};
use hayro_syntax::Pdf as ReadPdf;
use std::collections::BTreeMap;

const WHAT: &str = "페이지 레이블";
const TREE: &str = "페이지 레이블 트리";
/// The numbering styles of ISO 32000-1 Table 159: decimal, upper and lower case roman and letters.
const STYLES: [&[u8]; 5] = [b"D", b"R", b"r", b"A", b"a"];

/// How the pages from one page index on are labelled.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Range {
    style: Option<Vec<u8>>,
    prefix: Option<Vec<u8>>,
    start: i64,
}

/// The ranges of one file, by the page index they start at.
#[derive(Debug, Default)]
pub struct Labels(BTreeMap<usize, Range>);

/// The label of one page: style, prefix and the number it shows.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Label {
    style: Option<Vec<u8>>,
    prefix: Option<Vec<u8>>,
    number: i64,
}

impl Labels {
    /// The label of page `index`; a page before the first range, or any page of a file without
    /// labels, is numbered in decimal by its place.
    fn of(labels: Option<&Self>, index: usize) -> Label {
        match labels.and_then(|labels| labels.0.range(..=index).next_back()) {
            Some((&first, range)) => Label {
                style: range.style.clone(),
                prefix: range.prefix.clone(),
                number: range.start + i64::try_from(index - first).unwrap_or(0),
            },
            None => Label {
                style: Some(b"D".to_vec()),
                prefix: None,
                number: i64::try_from(index).unwrap_or(0) + 1,
            },
        }
    }
}

/// The page labels of `pdf`, if it has any.
pub fn read(pdf: &ReadPdf) -> Result<Option<Labels>, String> {
    let xref = pdf.xref();
    let Some(catalog) = xref.get::<Dict<'_>>(xref.root_id()) else {
        return Ok(None);
    };
    let Some((id, root)) = tree::child(pdf, &catalog, b"PageLabels") else {
        return Ok(None);
    };
    let mut ranges = BTreeMap::new();
    let mut walk = Walk::default();
    walk.enter(id, 1).map_err(|why| why.sentence(TREE))?;
    visit(pdf, &root, 1, &mut walk, &mut ranges)?;
    Ok(Some(Labels(ranges)))
}

fn visit(
    pdf: &ReadPdf,
    node: &Dict<'_>,
    depth: usize,
    walk: &mut Walk,
    ranges: &mut BTreeMap<usize, Range>,
) -> Result<(), String> {
    if let Some(nums) = node.get::<Array<'_>>(b"Nums") {
        let mut items = nums.iter::<Object<'_>>();
        while let (Some(key), Some(label)) = (items.next(), items.next()) {
            if ranges.len() >= MAX_LABEL_RANGES {
                return Err(format!(
                    "{WHAT} 범위 수가 한도({MAX_LABEL_RANGES})를 초과합니다."
                ));
            }
            if let (Object::Number(at), Object::Dict(label)) = (key, label) {
                let range = read_range(&label)?;
                if let Ok(at) = usize::try_from(at.as_i64()) {
                    ranges.insert(at, range);
                }
            }
        }
    }
    for (id, kid) in tree::kids(pdf, node) {
        walk.enter(id, depth + 1)
            .map_err(|why| why.sentence(TREE))?;
        visit(pdf, &kid, depth + 1, walk, ranges)?;
    }
    Ok(())
}

fn read_range(label: &Dict<'_>) -> Result<Range, String> {
    let style = label
        .get::<Name<'_>>(b"S")
        .map(|name| name.as_ref().to_vec())
        .filter(|name| STYLES.contains(&name.as_slice()));
    let prefix = label
        .get::<hayro_syntax::object::String<'_>>(b"P")
        .map(|text| text.as_bytes().to_vec())
        .filter(|text| !text.is_empty());
    if prefix
        .as_ref()
        .is_some_and(|text| text.len() > MAX_LABEL_PREFIX_BYTES)
    {
        return Err(format!(
            "{WHAT}의 앞붙임 글자가 허용 길이({MAX_LABEL_PREFIX_BYTES}바이트)를 초과합니다."
        ));
    }
    let start = label.get::<i32>(b"St").map_or(1, |n| i64::from(n.max(1)));
    Ok(Range {
        style,
        prefix,
        start,
    })
}

/// The `/PageLabels` of the result, or `None` when no source of its pages has labels.
pub fn combine(labels: &BTreeMap<usize, Labels>, pages: &[PagePlan]) -> Option<Value> {
    if labels.is_empty() {
        return None;
    }
    let mut nums = Vec::new();
    let mut previous: Option<Label> = None;
    for (n, plan) in pages.iter().enumerate() {
        let label = Labels::of(labels.get(&plan.source), plan.index);
        let continues = previous.as_ref().is_some_and(|before| {
            before.style == label.style
                && before.prefix == label.prefix
                && before.number + 1 == label.number
        });
        if !continues {
            nums.push(Value::Int(i32::try_from(n).unwrap_or(i32::MAX)));
            nums.push(range_value(&label));
        }
        previous = Some(label);
    }
    Some(Value::Dict(vec![(b"Nums".to_vec(), Value::Array(nums))]))
}

fn range_value(label: &Label) -> Value {
    let mut entries = Vec::new();
    if let Some(style) = &label.style {
        entries.push((b"S".to_vec(), Value::Name(style.clone())));
    }
    if let Some(prefix) = &label.prefix {
        entries.push((b"P".to_vec(), Value::Str(prefix.clone())));
    }
    if label.number != 1 {
        let number = i32::try_from(label.number).unwrap_or(i32::MAX);
        entries.push((b"St".to_vec(), Value::Int(number)));
    }
    Value::Dict(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(ranges: &[(usize, &str, i64)]) -> Labels {
        Labels(
            ranges
                .iter()
                .map(|&(at, style, start)| {
                    (
                        at,
                        Range {
                            style: Some(style.as_bytes().to_vec()),
                            prefix: None,
                            start,
                        },
                    )
                })
                .collect(),
        )
    }

    fn plan(pages: &[(usize, usize)]) -> Vec<PagePlan> {
        pages
            .iter()
            .map(|&(source, index)| PagePlan {
                source,
                index,
                turn: 0,
            })
            .collect()
    }

    #[test]
    fn a_range_starts_wherever_the_numbering_does_not_continue() {
        let source = BTreeMap::from([(0, labels(&[(0, "r", 1), (2, "D", 1)]))]);
        // Pages 3, 4, 1 of the file: "1", "2" continue each other, "i" starts over.
        let out = combine(&source, &plan(&[(0, 2), (0, 3), (0, 0)])).unwrap();
        let nums = out.get(b"Nums").and_then(Value::as_array).unwrap();
        assert_eq!(nums.len(), 4, "two ranges");
        assert_eq!(nums[0], Value::Int(0));
        assert_eq!(nums[2], Value::Int(2));
        assert_eq!(nums[1].get(b"S"), Some(&Value::Name(b"D".to_vec())));
        assert_eq!(nums[3].get(b"S"), Some(&Value::Name(b"r".to_vec())));
        assert_eq!(nums[1].get(b"St"), None, "numbers start at 1");
    }

    #[test]
    fn no_labels_in_any_source_means_none() {
        assert_eq!(combine(&BTreeMap::new(), &plan(&[(0, 0)])), None);
    }

    #[test]
    fn a_source_without_labels_is_numbered_by_its_place() {
        let source = BTreeMap::from([(1, labels(&[(0, "a", 1)]))]);
        let out = combine(&source, &plan(&[(0, 4), (0, 5), (1, 0)])).unwrap();
        let nums = out.get(b"Nums").and_then(Value::as_array).unwrap();
        assert_eq!(nums[1].get(b"St"), Some(&Value::Int(5)));
        assert_eq!(nums[1].get(b"S"), Some(&Value::Name(b"D".to_vec())));
    }
}

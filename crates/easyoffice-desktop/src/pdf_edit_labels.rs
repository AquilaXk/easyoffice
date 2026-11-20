// 참조 구현: qpdf (https://github.com/qpdf/qpdf, 커밋 4eba9589) `libqpdf/QPDFPageLabelDocumentHelper.cc`
// 의 `validate`(32-41줄: `/PageLabels`를 값이 사전인 번호 트리로 읽음), `getLabelForPage`(49-74줄),
// `getLabelsForPageRange`(76-123줄)와, 같은 방식으로 쓰는 `libqpdf/QPDFJob.cc` `handlePageSpecs`
// (2582-2650줄: 결과의 쪽마다 한 번씩 `getLabelsForPageRange(쪽, 쪽, 결과 쪽 번호)`를 부르고 모은
// 항목을 `/PageLabels /Nums`로 씀)를 옮겼다. Apache-2.0, Copyright (c) 2005-2021 Jay Berkenbilt,
// 2022-2026 Jay Berkenbilt and Manfred Holger. 바꾼 점: (1) 레이블이 없는 쪽에 만드는 레이블에 `/S /D`를
// 더했다(원본은 `/St`만 둬서 번호 없이 빈 레이블이 된다). (2) 번호가 범위를 넘으면 예외 대신 거부한다.
// (3) 트리는 `pdf_edit_tree.rs`(같은 출처)로 읽고 범위 수와 앞붙임 길이에 상한을 뒀다. 출처는
// THIRD_PARTY_NOTICES.md에 있다.
//
//! Page labels (`/Catalog /PageLabels`) of the sources in the result (PD-266).
//!
//! A label is what a viewer shows for a page instead of its place: "iv", "A-5". The file stores
//! them as ranges: a number tree whose keys are page indexes and whose values say how the pages from
//! that index on are numbered (style, prefix, first number). Take pages out, reorder them or join
//! files, and the ranges point at the wrong pages, so they cannot be copied. They are written
//! again from the pages the result has: for each page its label is worked out from its own file
//! and an entry is added unless the label just continues the numbering of the entry before it.

use crate::pdf_edit_budget::{MAX_LABEL_PREFIX_BYTES, MAX_LABEL_RANGES};
use crate::pdf_edit_tree::{self as tree, Entries, Key, KeyKind};
use crate::pdf_edit_value::Value;
use crate::pdf_edit_wire::PagePlan;
use hayro_syntax::object::{Dict, Name, Object};
use hayro_syntax::Pdf as ReadPdf;
use std::collections::BTreeMap;

const WHAT: &str = "페이지 레이블";
const TREE: &str = "페이지 레이블 트리";
/// The numbering styles of ISO 32000-1 Table 159: decimal, upper and lower case roman and letters.
const STYLES: [&[u8]; 5] = [b"D", b"R", b"r", b"A", b"a"];
const MSG_RANGE: &str = "쪽 레이블 번호가 허용 범위를 벗어납니다.";

/// How pages are labelled from one page index on, or the label of one page: style, prefix and the
/// number the page (or the first page of the range) shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    style: Option<Vec<u8>>,
    prefix: Option<Vec<u8>>,
    start: i64,
}

/// The ranges of one file, by the page index they start at.
#[derive(Debug, Default)]
pub struct Labels(BTreeMap<usize, Label>);

impl Labels {
    /// `getLabelForPage`: the label of page `index`, or `None` before the first range (and in a
    /// file without labels).
    fn label_for_page(&self, index: usize) -> Result<Option<Label>, String> {
        let Some((&first, range)) = self.0.range(..=index).next_back() else {
            return Ok(None);
        };
        let offset = i64::try_from(index - first).map_err(|_| MSG_RANGE.to_string())?;
        Ok(Some(Label {
            style: range.style.clone(),
            prefix: range.prefix.clone(),
            start: range.start.checked_add(offset).ok_or(MSG_RANGE)?,
        }))
    }

    /// `getLabelsForPageRange`: adds the entries for the pages `start..=end` of this file, which
    /// are the pages from `new_start` on in the result.
    fn labels_for_page_range(
        &self,
        (start, end): (usize, usize),
        new_start: i64,
        new_labels: &mut Vec<(i64, Label)>,
    ) -> Result<(), String> {
        // A suitable label for the first page: its own, or the one it would have in a new file in
        // which it has none.
        let label = match self.label_for_page(start)? {
            Some(label) => label,
            None => Label {
                style: Some(b"D".to_vec()),
                prefix: None,
                start: new_start.checked_add(1).ok_or(MSG_RANGE)?,
            },
        };
        // The entry is redundant if it has the style and prefix of the one before it and the number
        // it would get by counting on.
        let redundant = new_labels.last().is_some_and(|(last_index, last)| {
            label.style == last.style
                && label.prefix == last.prefix
                && label.start - last.start == new_start - last_index
        });
        if !redundant {
            new_labels.push((new_start, label));
        }
        let shift = new_start - i64::try_from(start).map_err(|_| MSG_RANGE.to_string())?;
        for index in start + 1..=end {
            if self.0.contains_key(&index) {
                if let Some(label) = self.label_for_page(index)? {
                    let at = i64::try_from(index).map_err(|_| MSG_RANGE.to_string())? + shift;
                    new_labels.push((at, label));
                }
            }
        }
        Ok(())
    }
}

fn is_dict(object: &Object<'_>) -> bool {
    matches!(object, Object::Dict(_))
}

/// The page labels of `pdf`, if it has any.
pub fn read(pdf: &ReadPdf) -> Result<Option<Labels>, String> {
    let xref = pdf.xref();
    let Some(root) = xref
        .get::<Dict<'_>>(xref.root_id())
        .and_then(|catalog| tree::child(pdf, &catalog, b"PageLabels"))
    else {
        return Ok(None);
    };
    let mut ranges = BTreeMap::new();
    for entry in Entries::new(pdf, root, KeyKind::Number, is_dict, MAX_LABEL_RANGES) {
        let entry = entry.map_err(|why| why.sentence(TREE))?;
        if ranges.len() >= MAX_LABEL_RANGES {
            return Err(format!(
                "{WHAT} 범위 수가 한도({MAX_LABEL_RANGES})를 초과합니다."
            ));
        }
        let (Key::Number(at), Some(label)) = (entry.key, dict_of(pdf, &entry.value)) else {
            continue;
        };
        if let Ok(at) = usize::try_from(at) {
            ranges.insert(at, read_range(&label)?);
        }
    }
    Ok(Some(Labels(ranges)))
}

fn dict_of<'a>(
    pdf: &'a ReadPdf,
    value: &hayro_syntax::object::MaybeRef<Object<'a>>,
) -> Option<Dict<'a>> {
    match value {
        hayro_syntax::object::MaybeRef::NotRef(Object::Dict(dict)) => Some(dict.clone()),
        hayro_syntax::object::MaybeRef::Ref(r) => {
            match pdf.xref().get::<Object<'a>>((*r).into())? {
                Object::Dict(dict) => Some(dict),
                _ => None,
            }
        }
        hayro_syntax::object::MaybeRef::NotRef(_) => None,
    }
}

fn read_range(label: &Dict<'_>) -> Result<Label, String> {
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
    // `/St` is 1 when it is missing (and must not be less than 1).
    let start = label.get::<i32>(b"St").map_or(1, |n| i64::from(n.max(1)));
    Ok(Label {
        style,
        prefix,
        start,
    })
}

/// The `/PageLabels` of the result (`handlePageSpecs`: a call for each page of the result with
/// the labels of the file it comes from), or `None` when no source of its pages has labels.
pub fn combine(
    labels: &BTreeMap<usize, Labels>,
    pages: &[PagePlan],
) -> Result<Option<Value>, String> {
    if labels.is_empty() {
        return Ok(None);
    }
    let none = Labels::default();
    let mut new_labels = Vec::new();
    for (place, plan) in pages.iter().enumerate() {
        let source = labels.get(&plan.source).unwrap_or(&none);
        let at = i64::try_from(place).map_err(|_| MSG_RANGE.to_string())?;
        source.labels_for_page_range((plan.index, plan.index), at, &mut new_labels)?;
    }
    let mut nums = Vec::with_capacity(new_labels.len() * 2);
    for (at, label) in &new_labels {
        nums.push(Value::Int(
            i32::try_from(*at).map_err(|_| MSG_RANGE.to_string())?,
        ));
        nums.push(label_value(label)?);
    }
    Ok(Some(Value::Dict(vec![(
        b"Nums".to_vec(),
        Value::Array(nums),
    )])))
}

/// The label dictionary: `/S` and `/P` when there are some, `/St` always.
fn label_value(label: &Label) -> Result<Value, String> {
    let mut entries = Vec::new();
    if let Some(style) = &label.style {
        entries.push((b"S".to_vec(), Value::Name(style.clone())));
    }
    if let Some(prefix) = &label.prefix {
        entries.push((b"P".to_vec(), Value::Str(prefix.clone())));
    }
    let start = i32::try_from(label.start).map_err(|_| MSG_RANGE.to_string())?;
    entries.push((b"St".to_vec(), Value::Int(start)));
    Ok(Value::Dict(entries))
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
                        Label {
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

    fn entry(nums: &Value, n: usize) -> (&Value, &Value) {
        let items = nums.get(b"Nums").and_then(Value::as_array).unwrap();
        (&items[2 * n], &items[2 * n + 1])
    }

    #[test]
    fn a_label_counts_on_from_the_range_it_is_in() {
        let source = labels(&[(0, "r", 1), (2, "D", 10)]);
        let label = source.label_for_page(3).unwrap().unwrap();
        assert_eq!((label.style, label.start), (Some(b"D".to_vec()), 11));
        assert_eq!(labels(&[(2, "D", 1)]).label_for_page(1), Ok(None));
        let huge = labels(&[(0, "D", i64::MAX)]);
        assert_eq!(huge.label_for_page(1), Err(MSG_RANGE.to_string()));
    }

    #[test]
    fn an_entry_is_added_unless_it_continues_the_one_before() {
        let source = BTreeMap::from([(0, labels(&[(0, "r", 1), (2, "D", 1)]))]);
        // Pages 3, 4, 1 of the file: "1", "2" continue each other, "i" starts over.
        let out = combine(&source, &plan(&[(0, 2), (0, 3), (0, 0)]))
            .unwrap()
            .unwrap();
        let items = out.get(b"Nums").and_then(Value::as_array).unwrap();
        assert_eq!(items.len(), 4, "two entries");
        let ((first, first_label), (second, second_label)) = (entry(&out, 0), entry(&out, 1));
        assert_eq!((first, second), (&Value::Int(0), &Value::Int(2)));
        assert_eq!(first_label.get(b"S"), Some(&Value::Name(b"D".to_vec())));
        assert_eq!(second_label.get(b"S"), Some(&Value::Name(b"r".to_vec())));
        assert_eq!(
            first_label.get(b"St"),
            Some(&Value::Int(1)),
            "`/St` is always written"
        );
    }

    #[test]
    fn an_explicit_entry_inside_a_page_range_is_copied() {
        let source = labels(&[(0, "D", 1), (2, "r", 1)]);
        let mut out = Vec::new();
        source.labels_for_page_range((0, 3), 5, &mut out).unwrap();
        let at: Vec<i64> = out.iter().map(|(at, _)| *at).collect();
        assert_eq!(
            at,
            [5, 7],
            "the first page and the page with an explicit entry"
        );
    }

    #[test]
    fn no_labels_in_any_source_means_none() {
        assert_eq!(combine(&BTreeMap::new(), &plan(&[(0, 0)])), Ok(None));
    }

    #[test]
    fn a_source_without_labels_is_numbered_in_decimal_by_its_place_in_the_result() {
        let source = BTreeMap::from([(1, labels(&[(0, "a", 1)]))]);
        let out = combine(&source, &plan(&[(0, 4), (0, 5), (1, 0)]))
            .unwrap()
            .unwrap();
        let (at, label) = entry(&out, 0);
        assert_eq!(at, &Value::Int(0));
        assert_eq!(label.get(b"St"), Some(&Value::Int(1)));
        assert_eq!(label.get(b"S"), Some(&Value::Name(b"D".to_vec())));
        let items = out.get(b"Nums").and_then(Value::as_array).unwrap();
        assert_eq!(items.len(), 4, "the third page starts the lettered range");
    }
}

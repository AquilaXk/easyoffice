// 참조 구현: (1) qpdf (https://github.com/qpdf/qpdf, 커밋 4eba9589) `libqpdf/QPDFEmbeddedFileDocumentHelper.cc`의
// `validate`(62-78줄: `/Names /EmbeddedFiles`를 값이 사전인 이름 트리로 읽음)와 `getEmbeddedFiles`
// (119-129줄: 이름과 파일 명세의 목록)와 `libqpdf/NNTree.cc` `compareKeys`(561-575줄: 이름은 해독한 글자의 바이트 순서로
// 비교), Apache-2.0, Copyright (c) 2005-2021 Jay Berkenbilt, 2022-2026
// Jay Berkenbilt and Manfred Holger. (2) pdfcpu (https://github.com/pdfcpu/pdfcpu, 커밋 a21a372d)
// `pkg/pdfcpu/model/name_tree_keys.go`의 `uniqueNameTreeEntry`(232-252줄)와 `entry.renamed`(254-264줄):
// 이름이 겹치면 뒤에 `\x01`(UTF-16BE 이름은 `\x00\x01`)을 붙여 겹치지 않을 때까지 되풀이한다. Apache-2.0,
// Copyright 2026 The pdfcpu Authors. 바꾼 점: 이름 트리는 `pdf_edit_tree.rs`로 읽고 개수에 상한을 뒀으며,
// 이름은 문자열 바이트 그대로 유지하고 정렬만 해독한 글자로 한다(원본 둘과 같다). 출처는
// THIRD_PARTY_NOTICES.md에 있다.
//
//! Attached files (`/Catalog /Names /EmbeddedFiles`) of the sources in the result (PD-266).
//!
//! An attachment is a file specification that points at an embedded file stream, listed by name
//! in a name tree. The file specifications and their streams are copied byte for byte with the
//! pages' own objects (`pdf_edit_carry.rs`); the tree is written again as one flat list, sorted by
//! name as the format requires, so the attachments of several joined files fit in one tree. A name
//! that two files share is kept for both by adding a control character to the later one (the name
//! a viewer shows is the `/UF` of the file specification, which is not changed).

use crate::pdf_edit_budget::MAX_ATTACHMENTS;
use crate::pdf_edit_preserve::decode_pdf_text;
use crate::pdf_edit_tree::{self as tree, Entries, Key, KeyKind};
use crate::pdf_edit_value::{Budget, Value};
use hayro_syntax::object::{Dict, ObjRef, Object};
use hayro_syntax::Pdf as ReadPdf;
use hayro_write::pdf_writer::Ref;
use std::collections::{HashMap, HashSet};

const WHAT: &str = "첨부 파일";
const TREE: &str = "첨부 파일 이름 트리";
/// Longest attachment name used for sorting and for telling names apart (characters).
const MAX_NAME_CHARS: usize = 256;

/// One attachment: its name in the tree (the bytes as the file has them, and the text they stand
/// for) and its file specification (a reference or a direct dictionary).
#[derive(Debug)]
pub struct Attachment {
    raw: Vec<u8>,
    name: String,
    spec: Value,
}

impl Attachment {
    /// The same with the references swapped for those of the result.
    pub fn translate(&self, map: &dyn Fn(ObjRef) -> Option<Ref>) -> Self {
        Self {
            raw: self.raw.clone(),
            name: self.name.clone(),
            spec: self.spec.translate(map),
        }
    }

    /// The file specification, whose references are the objects to copy.
    pub fn spec(&self) -> &Value {
        &self.spec
    }

    /// `entry.renamed` applied `times` times to the name this one was read with: a control
    /// character after it for each time, so that it differs.
    fn renamed(&mut self, original: &Self, times: usize) {
        self.name = original.name.clone() + &"\u{1}".repeat(times);
        self.raw = original.raw.clone();
        for _ in 0..times {
            if original.raw.starts_with(&[0xFE, 0xFF]) {
                self.raw.extend_from_slice(&[0x00, 0x01]);
            } else {
                self.raw.push(0x01);
            }
        }
    }
}

/// The sentence for more attachments in all the sources of a result than a source may list.
pub fn too_many_in_all() -> String {
    format!("{WHAT}의 합계가 한도({MAX_ATTACHMENTS})를 초과합니다.")
}

fn is_dict(object: &Object<'_>) -> bool {
    matches!(object, Object::Dict(_))
}

/// The attachments of `pdf`.
pub fn read(pdf: &ReadPdf, budget: &mut Budget) -> Result<Vec<Attachment>, String> {
    let xref = pdf.xref();
    let Some(root) = xref
        .get::<Dict<'_>>(xref.root_id())
        .and_then(|catalog| tree::child(pdf, &catalog, b"Names"))
        .and_then(|(_, names)| tree::child(pdf, &names, b"EmbeddedFiles"))
    else {
        return Ok(Vec::new());
    };
    let mut found = Vec::new();
    for entry in Entries::new(pdf, root, KeyKind::Text, is_dict, MAX_ATTACHMENTS) {
        let entry = entry.map_err(|why| why.sentence(TREE))?;
        if found.len() >= MAX_ATTACHMENTS {
            return Err(format!(
                "{WHAT}의 수가 한도({MAX_ATTACHMENTS})를 초과합니다."
            ));
        }
        let Key::Text(raw) = entry.key else {
            continue;
        };
        let spec = Value::read(&entry.value, budget)
            .map_err(|refused| refused.sentence("첨부 파일 정보"))?;
        found.push(Attachment {
            name: decode_pdf_text(&raw, false, MAX_NAME_CHARS),
            raw,
            spec,
        });
    }
    Ok(found)
}

/// The `/Names` dictionary `<< /EmbeddedFiles << /Names [...] >> >>` for the attachments of all the
/// sources (their references already swapped for the result's), or `None` when there are none.
/// Sorted by the text of the name (`compareKeys`); a name that is taken is renamed until it is not
/// (`uniqueNameTreeEntry`).
pub fn combine(lists: Vec<Vec<Attachment>>) -> Option<Value> {
    let mut taken: HashSet<String> = HashSet::new();
    // How many times each name was renamed so far, so that a name shared by many files does not
    // start from the beginning for each of them.
    let mut renames: HashMap<String, usize> = HashMap::new();
    let mut all: Vec<Attachment> = Vec::new();
    for mut attachment in lists.into_iter().flatten() {
        if taken.contains(&attachment.name) {
            let original = Attachment {
                raw: attachment.raw.clone(),
                name: attachment.name.clone(),
                spec: Value::Null,
            };
            let times = renames.entry(original.name.clone()).or_insert(0);
            loop {
                *times += 1;
                attachment.renamed(&original, *times);
                if !taken.contains(&attachment.name) {
                    break;
                }
            }
        }
        taken.insert(attachment.name.clone());
        all.push(attachment);
    }
    if all.is_empty() {
        return None;
    }
    all.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.raw.cmp(&b.raw)));
    let pairs = all
        .into_iter()
        .flat_map(|attachment| [Value::Str(attachment.raw), attachment.spec])
        .collect();
    let tree = Value::Dict(vec![(b"Names".to_vec(), Value::Array(pairs))]);
    Some(Value::Dict(vec![(b"EmbeddedFiles".to_vec(), tree)]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attachment(name: &str) -> Attachment {
        Attachment {
            raw: name.as_bytes().to_vec(),
            name: name.to_string(),
            spec: Value::Ref(ObjRef::new(7, 0)),
        }
    }

    fn keys(joined: &Value) -> Vec<Vec<u8>> {
        joined
            .get(b"EmbeddedFiles")
            .and_then(|tree| tree.get(b"Names"))
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .step_by(2)
            .map(|key| match key {
                Value::Str(bytes) => bytes.clone(),
                other => panic!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn names_are_sorted_and_a_shared_name_is_renamed() {
        let joined = combine(vec![
            vec![attachment("b"), attachment("a")],
            vec![attachment("a")],
        ])
        .unwrap();
        assert_eq!(
            keys(&joined),
            [b"a".to_vec(), b"a\x01".to_vec(), b"b".to_vec()]
        );
        assert!(combine(vec![Vec::new()]).is_none());
    }

    #[test]
    fn a_renamed_name_that_is_taken_too_is_renamed_again() {
        // Three files with the same name, and a fourth that already has the name the second gets.
        let joined = combine(vec![
            vec![attachment("a")],
            vec![attachment("a")],
            vec![attachment("a\u{1}")],
            vec![attachment("a")],
        ])
        .unwrap();
        assert_eq!(
            keys(&joined),
            [
                b"a".to_vec(),
                b"a\x01".to_vec(),
                b"a\x01\x01".to_vec(),
                b"a\x01\x01\x01".to_vec()
            ]
        );
    }

    #[test]
    fn many_files_with_one_name_are_renamed_without_a_search_for_each_name() {
        let lists: Vec<Vec<Attachment>> = (0..MAX_ATTACHMENTS)
            .map(|_| vec![attachment("a")])
            .collect();
        let started = std::time::Instant::now();
        let joined = combine(lists).unwrap();
        let all = keys(&joined);
        assert_eq!(all.len(), MAX_ATTACHMENTS);
        assert_eq!(all.last().map(Vec::len), Some(MAX_ATTACHMENTS));
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    #[test]
    fn a_utf16_name_is_renamed_in_its_own_encoding() {
        let mut korean = attachment("x");
        korean.raw = vec![0xFE, 0xFF, 0xD5, 0x5C];
        korean.name = "한".to_string();
        let mut same = attachment("x");
        same.raw = korean.raw.clone();
        same.name = korean.name.clone();
        let joined = combine(vec![vec![korean], vec![same]]).unwrap();
        assert_eq!(
            keys(&joined),
            [
                vec![0xFE, 0xFF, 0xD5, 0x5C],
                vec![0xFE, 0xFF, 0xD5, 0x5C, 0x00, 0x01]
            ]
        );
    }
}

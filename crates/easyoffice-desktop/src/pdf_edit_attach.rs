//! Attached files (`/Catalog /Names /EmbeddedFiles`) of the sources in the result (PD-266).
//!
//! An attachment is a file specification that points at an embedded file stream, listed by name
//! in a name tree. The file specifications and their streams are copied byte for byte with the
//! pages' own objects (`pdf_edit_carry.rs`); the tree is written again as one flat list, sorted by
//! name as the format requires, so the attachments of several joined files fit in one tree. A name
//! that two files share is kept for both: the later one gets " (2)", " (3)" after it (the name a
//! viewer shows is the `/UF` of the file specification, which is not changed).
//!
//! Library choice (the three steps of the implementation rule): (1) `hayro-write` copies the
//! objects but no reader or writer of name trees is in the dependency graph; (2) the JavaScript
//! PDF viewers read the tree for display only; (3) written against ISO 32000-1 §7.9.6 (name trees)
//! and §7.11.4 (embedded file streams) with the limits of the shell's other trees. Nothing was
//! copied.

use crate::pdf_edit_budget::MAX_ATTACHMENTS;
use crate::pdf_edit_preserve::decode_pdf_text;
use crate::pdf_edit_tree::{self as tree, Walk};
use crate::pdf_edit_value::{Budget, Value};
use hayro_syntax::object::{Array, Dict, MaybeRef, ObjRef, Object};
use hayro_syntax::Pdf as ReadPdf;
use hayro_write::pdf_writer::Ref;
use std::collections::HashSet;

const WHAT: &str = "첨부 파일";
const TREE: &str = "첨부 파일 이름 트리";
/// Longest attachment name kept (characters).
const MAX_NAME_CHARS: usize = 256;

/// One attachment: its name in the tree and its file specification (a reference or a direct
/// dictionary).
#[derive(Debug)]
pub struct Attachment {
    name: String,
    spec: Value,
}

impl Attachment {
    /// The same with the references swapped for those of the result.
    pub fn translate(&self, map: &dyn Fn(ObjRef) -> Option<Ref>) -> Self {
        Self {
            name: self.name.clone(),
            spec: self.spec.translate(map),
        }
    }

    /// The file specification, whose references are the objects to copy.
    pub fn spec(&self) -> &Value {
        &self.spec
    }
}

/// The attachments of `pdf`.
pub fn read(pdf: &ReadPdf, budget: &mut Budget) -> Result<Vec<Attachment>, String> {
    let xref = pdf.xref();
    let Some((id, root)) = xref
        .get::<Dict<'_>>(xref.root_id())
        .and_then(|catalog| tree::child(pdf, &catalog, b"Names"))
        .and_then(|(_, names)| tree::child(pdf, &names, b"EmbeddedFiles"))
    else {
        return Ok(Vec::new());
    };
    let mut found = Vec::new();
    let mut walk = Walk::default();
    walk.enter(id, 1).map_err(|why| why.sentence(TREE))?;
    visit(pdf, &root, (1, &mut walk, budget), &mut found)?;
    Ok(found)
}

fn visit(
    pdf: &ReadPdf,
    node: &Dict<'_>,
    (depth, walk, budget): (usize, &mut Walk, &mut Budget),
    found: &mut Vec<Attachment>,
) -> Result<(), String> {
    if let Some(names) = node.get::<Array<'_>>(b"Names") {
        let mut items = names.raw_iter();
        while let (Some(key), Some(spec)) = (items.next(), items.next()) {
            if found.len() >= MAX_ATTACHMENTS {
                return Err(format!(
                    "{WHAT}의 수가 한도({MAX_ATTACHMENTS})를 초과합니다."
                ));
            }
            let Some(name) = name_of(pdf, &key) else {
                continue;
            };
            let spec =
                Value::read(&spec, budget).map_err(|refused| refused.sentence("첨부 파일 정보"))?;
            if matches!(spec, Value::Ref(_) | Value::Dict(_)) {
                found.push(Attachment { name, spec });
            }
        }
    }
    for (id, kid) in tree::kids(pdf, node) {
        walk.enter(id, depth + 1)
            .map_err(|why| why.sentence(TREE))?;
        visit(pdf, &kid, (depth + 1, &mut *walk, &mut *budget), found)?;
    }
    Ok(())
}

/// The text of a key, which may be a reference to a string.
fn name_of(pdf: &ReadPdf, key: &MaybeRef<Object<'_>>) -> Option<String> {
    let object = match key {
        MaybeRef::NotRef(object) => object.clone(),
        MaybeRef::Ref(r) => pdf.xref().get::<Object<'_>>((*r).into())?,
    };
    match object {
        Object::String(text) => Some(decode_pdf_text(text.as_bytes(), false, MAX_NAME_CHARS)),
        _ => None,
    }
}

/// A text string as the format writes it: plain bytes when every character is printable ASCII,
/// UTF-16BE with a byte order mark otherwise.
fn text_string(text: &str) -> Vec<u8> {
    if text.chars().all(|c| (' '..='~').contains(&c)) {
        return text.as_bytes().to_vec();
    }
    let mut out = vec![0xFE, 0xFF];
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_be_bytes());
    }
    out
}

/// The `/Names` dictionary `<< /EmbeddedFiles << /Names [...] >> >>` for the attachments of all the sources (their
/// references already swapped for the result's), or `None` when there are none.
pub fn combine(lists: Vec<Vec<Attachment>>) -> Option<Value> {
    let mut taken: HashSet<Vec<u8>> = HashSet::new();
    let mut entries: Vec<(Vec<u8>, Value)> = Vec::new();
    for attachment in lists.into_iter().flatten() {
        let mut key = text_string(&attachment.name);
        let mut copy = 2;
        while taken.contains(&key) {
            key = text_string(&format!("{} ({copy})", attachment.name));
            copy += 1;
        }
        taken.insert(key.clone());
        entries.push((key, attachment.spec));
    }
    if entries.is_empty() {
        return None;
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let pairs = entries
        .into_iter()
        .flat_map(|(key, spec)| [Value::Str(key), spec])
        .collect();
    let tree = Value::Dict(vec![(b"Names".to_vec(), Value::Array(pairs))]);
    Some(Value::Dict(vec![(b"EmbeddedFiles".to_vec(), tree)]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attachment(name: &str) -> Attachment {
        Attachment {
            name: name.to_string(),
            spec: Value::Ref(ObjRef::new(7, 0)),
        }
    }

    #[test]
    fn names_are_sorted_and_a_shared_name_is_numbered() {
        let joined = combine(vec![
            vec![attachment("b"), attachment("a")],
            vec![attachment("a")],
        ])
        .unwrap();
        let pairs = joined
            .get(b"EmbeddedFiles")
            .and_then(|tree| tree.get(b"Names"))
            .and_then(Value::as_array)
            .unwrap();
        let keys: Vec<&Value> = pairs.iter().step_by(2).collect();
        assert_eq!(
            keys,
            [
                &Value::Str(b"a".to_vec()),
                &Value::Str(b"a (2)".to_vec()),
                &Value::Str(b"b".to_vec())
            ]
        );
        assert_eq!(combine(vec![Vec::new()]), None);
    }

    #[test]
    fn a_name_with_other_than_ascii_is_a_utf16_text_string() {
        assert_eq!(text_string("a b"), b"a b");
        assert_eq!(text_string("한"), [0xFE, 0xFF, 0xD5, 0x5C]);
    }
}

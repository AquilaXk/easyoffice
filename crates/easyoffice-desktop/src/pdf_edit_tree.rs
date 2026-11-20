// 참조 구현: qpdf (https://github.com/qpdf/qpdf, 커밋 4eba9589) `libqpdf/NNTree.cc`에서 읽기 쪽 함수를
// 옮겼다: `NNTreeIterator::getNextKid`(76-90줄), `increment`(91-128줄, 앞으로만), `deepen`(474-543줄),
// `NNTreeImpl::begin`(545-551줄). Apache-2.0, Copyright (c) 2005-2021 Jay
// Berkenbilt, 2022-2026 Jay Berkenbilt and Manfred Holger. 바꾼 점: `deepen`은 첫 kid가 노드가 아니면 포기하지 않고 노드인 첫 kid로 내려간다. 반복자를 Rust `Iterator`로,
// 경로(`path`)를 `Vec`으로 썼고, qpdf가 경고하고 지나가는 순환·중첩·노드 수 문제를 거부로 바꿨으며
// (qpdf에는 노드 수·깊이 상한이 없다), 한 노드가 가진 kid와 항목 수에도 상한을 두었다. 삽입·삭제·
// 분할·복구와 뒤로 걷기는 쓰지 않아 옮기지 않았다. 출처는 THIRD_PARTY_NOTICES.md에 있다.
//
//! The walk over the entries of a name tree or a number tree with the limits of the shell (PD-266).
//!
//! The trees of the catalog (`/PageLabels`, `/Names /EmbeddedFiles`) are `/Kids` of nodes that are
//! dictionaries, usually behind references. A crafted file can make a node its own kid, nest
//! nodes without end or list a million kids. The walk keeps the path from the root to the node it
//! is in, refuses a node that is already on that path (a loop), and counts every node it enters
//! and the depth of the path. A kid that is not a node and an entry that has the wrong type of key
//! or value are skipped, as the original does.

use crate::pdf_edit_budget::{MAX_NAME_TREE_NODES, MAX_OUTLINE_DEPTH};
use hayro_syntax::object::{Array, Dict, MaybeRef, Object, ObjectIdentifier};
use hayro_syntax::Pdf as ReadPdf;

/// Why a tree is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    Deep,
    Nodes,
    Cycle,
}

impl Refused {
    /// The sentence for the person; `what` names the tree ("페이지 레이블 트리").
    pub fn sentence(self, what: &str) -> String {
        match self {
            Self::Deep => format!("{what}가 허용 깊이({MAX_OUTLINE_DEPTH}단계)를 초과합니다."),
            Self::Nodes => format!("{what}의 노드 수가 한도({MAX_NAME_TREE_NODES})를 초과합니다."),
            Self::Cycle => format!("{what}에 순환 참조가 있습니다."),
        }
    }
}

/// What the keys of the tree are: strings (a name tree) or integers (a number tree).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyKind {
    Text,
    Number,
}

impl KeyKind {
    /// The array with the keys and values: `/Names` or `/Nums`.
    fn items_key(self) -> &'static [u8] {
        match self {
            Self::Text => b"Names",
            Self::Number => b"Nums",
        }
    }
}

/// A key of an entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    Text(Vec<u8>),
    Number(i64),
}

/// An entry: its key and the value as the tree holds it (a reference stays a reference).
pub struct Entry<'a> {
    pub key: Key,
    pub value: MaybeRef<Object<'a>>,
}

/// A node of the tree and the reference it was reached through (`None` for a direct dictionary).
pub type Reached<'a> = (Option<ObjectIdentifier>, Dict<'a>);

/// The dictionary under `key` of `parent`, with the reference it is behind.
pub fn child<'a>(pdf: &'a ReadPdf, parent: &Dict<'a>, key: &[u8]) -> Option<Reached<'a>> {
    resolve(pdf, &parent.get_raw::<Object<'a>>(key)?)
}

/// A copy of a raw item (`MaybeRef` is not `Clone`).
fn copy_raw<'a>(raw: &MaybeRef<Object<'a>>) -> MaybeRef<Object<'a>> {
    match raw {
        MaybeRef::Ref(r) => MaybeRef::Ref(*r),
        MaybeRef::NotRef(object) => MaybeRef::NotRef(object.clone()),
    }
}

fn resolve<'a>(pdf: &'a ReadPdf, raw: &MaybeRef<Object<'a>>) -> Option<Reached<'a>> {
    match raw {
        MaybeRef::Ref(r) => {
            let id = ObjectIdentifier::from(*r);
            match pdf.xref().get::<Object<'a>>(id)? {
                Object::Dict(dict) => Some((Some(id), dict)),
                _ => None,
            }
        }
        MaybeRef::NotRef(Object::Dict(dict)) => Some((None, dict.clone())),
        MaybeRef::NotRef(_) => None,
    }
}

/// One step of the path from the root to the node the walk is in: the node, its kids and the
/// number of the kid the walk went into.
struct PathElement<'a> {
    id: Option<ObjectIdentifier>,
    kids: Vec<MaybeRef<Object<'a>>>,
    kid_number: usize,
}

/// The entries of a tree in order of the tree. After an error the walk ends.
pub struct Entries<'a> {
    pdf: &'a ReadPdf,
    kind: KeyKind,
    /// Whether a value (resolved) is acceptable; entries with another value are skipped.
    value_valid: fn(&Object<'_>) -> bool,
    /// Most values (and keys) one node's array may hold.
    max_items: usize,
    root: Option<Reached<'a>>,
    path: Vec<PathElement<'a>>,
    /// The array of keys and values of the node the walk is in; `item_number` is the key's place.
    items: Vec<MaybeRef<Object<'a>>>,
    item_number: usize,
    nodes: usize,
    started: bool,
    failed: bool,
}

impl<'a> Entries<'a> {
    /// The entries of the tree under `root`. `max_items` is the most entries one node may hold.
    pub fn new(
        pdf: &'a ReadPdf,
        root: Reached<'a>,
        kind: KeyKind,
        value_valid: fn(&Object<'_>) -> bool,
        max_items: usize,
    ) -> Self {
        Self {
            pdf,
            kind,
            value_valid,
            max_items,
            root: Some(root),
            path: Vec::new(),
            items: Vec::new(),
            item_number: 0,
            nodes: 0,
            started: false,
            failed: false,
        }
    }

    /// Counts a node the walk enters, as the walk of the original does for the loop check (the
    /// nodes on the path), and refuses past the limits.
    fn enter(&mut self, id: Option<ObjectIdentifier>) -> Result<(), Refused> {
        self.nodes += 1;
        if self.nodes > MAX_NAME_TREE_NODES {
            return Err(Refused::Nodes);
        }
        if self.path.len() >= MAX_OUTLINE_DEPTH {
            return Err(Refused::Deep);
        }
        let on_path = |id: ObjectIdentifier| self.path.iter().any(|element| element.id == Some(id));
        match id {
            Some(id) if on_path(id) => Err(Refused::Cycle),
            _ => Ok(()),
        }
    }

    /// `NNTreeIterator::deepen` going to the first kid: from `node` down through first kids until a
    /// node with entries. `Ok(false)` leaves the path as it was (a node that has neither entries
    /// nor kids, or a first kid that is no node).
    fn deepen(&mut self, mut node: Reached<'a>) -> Result<bool, Refused> {
        let old_depth = self.path.len();
        loop {
            if let Err(refused) = self.enter(node.0) {
                self.path.truncate(old_depth);
                return Err(refused);
            }
            let items = self.read_items(&node.1);
            if items.len() > 1 {
                self.items = items;
                self.item_number = 0;
                return Ok(true);
            }
            let kids = self.read_kids(&node.1)?;
            // The first kid that is a dictionary (the original takes the first kid and gives up
            // when it is not one, which hides every entry behind it).
            let Some((kid_number, next)) = kids
                .iter()
                .enumerate()
                .filter_map(|(n, kid)| resolve(self.pdf, kid).map(|next| (n, next)))
                .find(|(_, next)| self.is_node(&next.1))
            else {
                // Neither a non-empty items array nor a kid that is a node: nothing to see here.
                self.path.truncate(old_depth);
                return Ok(false);
            };
            self.path.push(PathElement {
                id: node.0,
                kids,
                kid_number,
            });
            node = next;
        }
    }

    /// A dictionary is a node of the tree when it has `/Kids` or the items array.
    fn is_node(&self, dict: &Dict<'a>) -> bool {
        dict.contains_key(b"Kids") || dict.contains_key(self.kind.items_key())
    }

    /// The kids of `node`, at most the limit of nodes (more is refused).
    fn read_kids(&self, node: &Dict<'a>) -> Result<Vec<MaybeRef<Object<'a>>>, Refused> {
        let Some(kids) = node.get::<Array<'a>>(b"Kids") else {
            return Ok(Vec::new());
        };
        let kids: Vec<_> = kids.raw_iter().take(MAX_NAME_TREE_NODES + 1).collect();
        if kids.len() > MAX_NAME_TREE_NODES {
            return Err(Refused::Nodes);
        }
        Ok(kids)
    }

    fn read_items(&self, node: &Dict<'a>) -> Vec<MaybeRef<Object<'a>>> {
        node.get::<Array<'a>>(self.kind.items_key())
            .map(|items| items.raw_iter().take(self.max_items * 2 + 2).collect())
            .unwrap_or_default()
    }

    /// `NNTreeIterator::getNextKid` going forward: the next kid of the element that is a node (it
    /// has `/Kids` or the items array); the ones that are not are skipped.
    fn next_kid(
        pdf: &'a ReadPdf,
        kind: KeyKind,
        element: &mut PathElement<'a>,
    ) -> Option<Reached<'a>> {
        loop {
            element.kid_number += 1;
            let kid = resolve(pdf, element.kids.get(element.kid_number)?);
            if let Some(kid) = kid {
                if kid.1.contains_key(b"Kids") || kid.1.contains_key(kind.items_key()) {
                    return Some(kid);
                }
            }
        }
    }

    /// `NNTreeIterator::increment` going forward: the next entry's place, climbing to the next kid
    /// of a parent when the node's entries are used up. `Ok(false)` when the tree is at its end.
    fn increment(&mut self) -> Result<bool, Refused> {
        self.item_number += 2;
        while self.item_number + 1 >= self.items.len() {
            self.items.clear();
            loop {
                let (pdf, kind) = (self.pdf, self.kind);
                let Some(element) = self.path.last_mut() else {
                    return Ok(false);
                };
                match Self::next_kid(pdf, kind, element) {
                    Some(kid) => {
                        if self.deepen(kid)? {
                            break;
                        }
                    }
                    None => {
                        self.path.pop();
                    }
                }
            }
        }
        Ok(true)
    }

    /// The key at `place` if it is of the tree's kind.
    fn key_at(&self, place: usize) -> Option<Key> {
        let object = self.resolve_raw(self.items.get(place)?)?;
        match (self.kind, object) {
            (KeyKind::Text, Object::String(text)) => Some(Key::Text(text.as_bytes().to_vec())),
            (KeyKind::Number, Object::Number(number)) => Some(Key::Number(number.as_i64())),
            _ => None,
        }
    }

    fn resolve_raw(&self, raw: &MaybeRef<Object<'a>>) -> Option<Object<'a>> {
        match raw {
            MaybeRef::NotRef(object) => Some(object.clone()),
            MaybeRef::Ref(r) => self.pdf.xref().get::<Object<'a>>((*r).into()),
        }
    }

    fn step(&mut self) -> Result<Option<Entry<'a>>, Refused> {
        loop {
            if !self.started {
                self.started = true;
                let Some(root) = self.root.take() else {
                    return Ok(None);
                };
                // An empty root is a tree without entries, not a fault (`allow_empty`).
                if !self.deepen(root)? {
                    return Ok(None);
                }
            } else if !self.increment()? {
                return Ok(None);
            }
            let (Some(key), Some(value)) = (
                self.key_at(self.item_number),
                self.items.get(self.item_number + 1).map(copy_raw),
            ) else {
                continue;
            };
            if self
                .resolve_raw(&value)
                .is_some_and(|resolved| (self.value_valid)(&resolved))
            {
                return Ok(Some(Entry { key, value }));
            }
        }
    }
}

impl<'a> Iterator for Entries<'a> {
    type Item = Result<Entry<'a>, Refused>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.failed {
            return None;
        }
        match self.step() {
            Ok(entry) => entry.map(Ok),
            Err(refused) => {
                self.failed = true;
                Some(Err(refused))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf_edit_fixtures::hand_written_with_xref;
    use std::sync::Arc;

    fn is_dict(object: &Object<'_>) -> bool {
        matches!(object, Object::Dict(_))
    }

    /// The numbers of a `/Nums` tree whose root is object 5 of a file with `objects` from 5 on.
    fn numbers(objects: &[&str]) -> Result<Vec<i64>, Refused> {
        let mut all = vec![
            (1, "<< /Type /Catalog /Pages 2 0 R /T 5 0 R >>".to_string()),
            (2, "<< /Type /Pages /Kids [] /Count 0 >>".to_string()),
            (3, "null".to_string()),
            (4, "null".to_string()),
        ];
        all.extend((5u32..).zip(objects.iter().map(|o| (*o).to_string())));
        let pdf = ReadPdf::new(Arc::new(hand_written_with_xref(&all, ""))).unwrap();
        let catalog = pdf.xref().get::<Dict<'_>>(pdf.xref().root_id()).unwrap();
        let root = child(&pdf, &catalog, b"T").unwrap();
        Entries::new(&pdf, root, KeyKind::Number, is_dict, 64)
            .map(|entry| {
                entry.map(|entry| match entry.key {
                    Key::Number(n) => n,
                    Key::Text(_) => -1,
                })
            })
            .collect()
    }

    #[test]
    fn entries_come_in_the_order_of_the_tree_across_kids() {
        let tree = [
            "<< /Kids [6 0 R 7 0 R] >>",
            "<< /Nums [1 << >> 2 << >>] >>",
            "<< /Kids [8 0 R] >>",
            "<< /Nums [5 << >> 7 << >>] >>",
        ];
        assert_eq!(numbers(&tree), Ok(vec![1, 2, 5, 7]));
    }

    #[test]
    fn nodes_and_entries_that_are_wrong_are_skipped() {
        // A kid that is no dictionary, a node without entries, a key of the wrong type and a value
        // that is no dictionary.
        let tree = [
            "<< /Kids [99 0 R 6 0 R 7 0 R 8 0 R] >>",
            "<< /Foo 1 >>",
            "<< /Nums [(x) << >> 3 5 4 << >>] >>",
            "<< /Nums [9 << >>] >>",
        ];
        assert_eq!(numbers(&tree), Ok(vec![4, 9]));
        assert_eq!(numbers(&["<< /Nums [] >>"]), Ok(vec![]));
    }

    #[test]
    fn a_loop_nesting_and_a_width_past_the_limits_are_refused() {
        assert_eq!(numbers(&["<< /Kids [5 0 R] >>"]), Err(Refused::Cycle));
        let a_chain: Vec<String> = (0..=MAX_OUTLINE_DEPTH)
            .map(|n| format!("<< /Kids [{} 0 R] >>", 6 + n))
            .collect();
        let refs: Vec<&str> = a_chain.iter().map(String::as_str).collect();
        assert_eq!(numbers(&refs), Err(Refused::Deep));
        let wide = format!("<< /Kids [{}] >>", "6 0 R ".repeat(MAX_NAME_TREE_NODES + 1));
        assert_eq!(
            numbers(&[&wide, "<< /Nums [1 << >>] >>"]),
            Err(Refused::Nodes)
        );
        // A shared kid is not a loop, but every visit counts: a fan of the same node ends in the limit.
        let fan = format!("<< /Kids [{}] >>", "6 0 R ".repeat(MAX_NAME_TREE_NODES));
        assert_eq!(
            numbers(&[&fan, "<< /Nums [1 << >>] >>"]),
            Err(Refused::Nodes)
        );
        assert!(Refused::Cycle
            .sentence("첨부 파일 이름 트리")
            .contains("순환"));
    }
}

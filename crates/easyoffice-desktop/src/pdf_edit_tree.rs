//! The walk over the nodes of a name tree or a number tree with the limits of the shell (PD-266).
//!
//! The trees of the catalog (`/PageLabels`, `/Names /EmbeddedFiles`) are `/Kids` of nodes that are
//! dictionaries, usually behind references. A crafted file can make a node its own kid, nest
//! nodes without end or list a million kids. [`Walk::enter`] counts every node and refuses past
//! the depth and node limits and a node that was entered before, by the reference it was reached
//! through: the object number of a dictionary is not relied on, only the reference that led to it.

use crate::pdf_edit_budget::{MAX_NAME_TREE_NODES, MAX_OUTLINE_DEPTH};
use hayro_syntax::object::{Array, Dict, MaybeRef, Object, ObjectIdentifier};
use hayro_syntax::Pdf as ReadPdf;
use std::collections::HashSet;

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

/// A node that was reached: where it came from, so that it is not entered twice.
pub type Reached<'a> = (Option<ObjectIdentifier>, Dict<'a>);

/// The nodes entered so far.
#[derive(Debug, Default)]
pub struct Walk {
    seen: HashSet<ObjectIdentifier>,
    nodes: usize,
}

impl Walk {
    /// Counts a node at `depth` (the root is 1) that was reached through the reference `id`.
    pub fn enter(&mut self, id: Option<ObjectIdentifier>, depth: usize) -> Result<(), Refused> {
        if depth > MAX_OUTLINE_DEPTH {
            return Err(Refused::Deep);
        }
        self.nodes += 1;
        if self.nodes > MAX_NAME_TREE_NODES {
            return Err(Refused::Nodes);
        }
        match id {
            Some(id) if !self.seen.insert(id) => Err(Refused::Cycle),
            _ => Ok(()),
        }
    }
}

/// The dictionary under `key` of `parent`, with the reference it is behind.
pub fn child<'a>(pdf: &'a ReadPdf, parent: &Dict<'a>, key: &[u8]) -> Option<Reached<'a>> {
    resolve(pdf, parent.get_raw::<Object<'a>>(key)?)
}

/// The kids of `node` that are dictionaries, in order; at most one more than a tree may have
/// nodes, so that a list of a million kids is not read to the end.
pub fn kids<'a>(pdf: &'a ReadPdf, node: &Dict<'a>) -> Vec<Reached<'a>> {
    node.get::<Array<'a>>(b"Kids")
        .map(|kids| {
            kids.raw_iter()
                .take(MAX_NAME_TREE_NODES + 1)
                .filter_map(|kid| resolve(pdf, kid))
                .collect()
        })
        .unwrap_or_default()
}

fn resolve<'a>(pdf: &'a ReadPdf, raw: MaybeRef<Object<'a>>) -> Option<Reached<'a>> {
    match raw {
        MaybeRef::Ref(r) => {
            let id = ObjectIdentifier::from(r);
            match pdf.xref().get::<Object<'a>>(id)? {
                Object::Dict(dict) => Some((Some(id), dict)),
                _ => None,
            }
        }
        MaybeRef::NotRef(Object::Dict(dict)) => Some((None, dict)),
        MaybeRef::NotRef(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u32) -> Option<ObjectIdentifier> {
        Some(ObjectIdentifier::new(n, 0))
    }

    #[test]
    fn a_node_is_entered_once_and_the_limits_hold() {
        let mut walk = Walk::default();
        assert_eq!(walk.enter(id(1), 1), Ok(()));
        assert_eq!(walk.enter(id(1), 2), Err(Refused::Cycle));
        assert_eq!(
            walk.enter(None, 2),
            Ok(()),
            "a direct dictionary has no reference"
        );
        assert_eq!(walk.enter(id(2), MAX_OUTLINE_DEPTH + 1), Err(Refused::Deep));
        let mut many = Walk::default();
        for n in 0..MAX_NAME_TREE_NODES {
            assert_eq!(many.enter(None, 1), Ok(()), "{n}");
        }
        assert_eq!(many.enter(None, 1), Err(Refused::Nodes));
        assert!(Refused::Cycle
            .sentence("첨부 파일 이름 트리")
            .contains("순환"));
    }
}

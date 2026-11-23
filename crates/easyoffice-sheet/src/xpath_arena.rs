//! Pre-order XML arena, word-packed node bitset, XPath value coercion, and bounded work budget.
//!
//! 참조 구현: none (built cleanly from W3C XPath 1.0 specification, wrapping workspace
//! `roxmltree` with `allow_dtd: false`, structural depth/node limits, and `WorkBudget`).

use crate::{EvalContext, FormulaError};
use std::collections::{HashMap, HashSet};

pub(super) const MAX_XML_BYTES: usize = 2 * 1024 * 1024;
const MAX_XML_DEPTH: usize = 128;
pub(super) const MAX_XML_NODES: usize = 16_384;
const MAX_XPATH_OPS: usize = 200_000;
const NS_XML_URI: &str = "http://www.w3.org/XML/1998/namespace";
pub(super) const NONE_ID: u32 = u32::MAX;

pub(super) struct WorkBudget {
    ops: usize,
    charged_blocks: usize,
}

impl WorkBudget {
    pub(super) fn new() -> Self {
        Self {
            ops: 0,
            charged_blocks: 0,
        }
    }

    #[inline]
    pub(super) fn charge(
        &mut self,
        n: usize,
        ctx: &mut EvalContext<'_>,
    ) -> Result<(), FormulaError> {
        self.ops = self.ops.saturating_add(n);
        if self.ops > MAX_XPATH_OPS {
            return Err(FormulaError::Value);
        }
        let blocks = self.ops >> 6;
        if blocks > self.charged_blocks {
            let delta = blocks - self.charged_blocks;
            self.charged_blocks = blocks;
            ctx.charge_steps(delta)?;
        }
        Ok(())
    }
}

/// Word-packed node bitset kept strictly in pre-order (document order) `0..total`.
#[derive(Debug, Clone)]
pub(super) struct NodeSet {
    words: Vec<u64>,
    total: usize,
}

impl NodeSet {
    pub(super) fn empty(total: usize) -> Self {
        Self {
            words: vec![0; (total + 63) >> 6],
            total,
        }
    }

    pub(super) fn singleton(total: usize, id: u32) -> Self {
        let mut s = Self::empty(total);
        s.insert(id);
        s
    }

    #[inline]
    pub(super) fn insert(&mut self, id: u32) {
        let idx = id as usize;
        if idx < self.total {
            self.words[idx >> 6] |= 1u64 << (idx & 63);
        }
    }

    #[inline]
    pub(super) fn is_empty(&self) -> bool {
        self.words.iter().all(|&w| w == 0)
    }

    #[inline]
    pub(super) fn len(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }

    #[inline]
    pub(super) fn first(&self) -> Option<u32> {
        for (wi, &w) in self.words.iter().enumerate() {
            if w != 0 {
                return Some(((wi << 6) + w.trailing_zeros() as usize) as u32);
            }
        }
        None
    }

    pub(super) fn union_with(&mut self, other: &Self) {
        for (dst, &src) in self.words.iter_mut().zip(&other.words) {
            *dst |= src;
        }
    }

    pub(super) fn iter(&self) -> NodeSetIter<'_> {
        NodeSetIter {
            words: &self.words,
            word_idx: 0,
            cur_word: self.words.first().copied().unwrap_or(0),
        }
    }
}

pub(super) struct NodeSetIter<'a> {
    words: &'a [u64],
    word_idx: usize,
    cur_word: u64,
}

impl Iterator for NodeSetIter<'_> {
    type Item = u32;

    #[inline]
    fn next(&mut self) -> Option<u32> {
        while self.cur_word == 0 {
            self.word_idx += 1;
            self.cur_word = *self.words.get(self.word_idx)?;
        }
        let bit = self.cur_word.trailing_zeros() as usize;
        self.cur_word &= self.cur_word - 1;
        Some(((self.word_idx << 6) + bit) as u32)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NodeKind {
    Root,
    Element,
    Attribute,
    Text,
    Comment,
    Pi,
}

pub(super) struct ArenaNode<'doc> {
    pub(super) kind: NodeKind,
    pub(super) parent: Option<u32>,
    pub(super) first_child: u32,
    pub(super) next_sibling: u32,
    pub(super) prev_sibling: u32,
    pub(super) attr_start: u32,
    pub(super) attr_end: u32,
    pub(super) subtree_end: u32,
    pub(super) local: &'doc str,
    pub(super) prefix: &'doc str,
    pub(super) ns_uri: Option<&'doc str>,
    pub(super) leaf_text: &'doc str,
}

pub(super) struct XmlArena<'doc> {
    pub(super) nodes: Vec<ArenaNode<'doc>>,
    elem_text_cache: Vec<Option<Box<str>>>,
    pub(super) ns_prefixes: HashMap<&'doc str, Option<&'doc str>>,
}

impl<'doc> XmlArena<'doc> {
    pub(super) fn from_doc(doc: &'doc roxmltree::Document<'doc>) -> Result<Self, FormulaError> {
        enum Frame<'a, 'd> {
            Enter(roxmltree::Node<'a, 'd>, Option<u32>, usize),
            Finish(u32),
        }

        let mut ns_prefixes: HashMap<&'doc str, Option<&'doc str>> = HashMap::new();
        ns_prefixes.insert("xml", Some(NS_XML_URI));
        let mut root_bound: HashSet<&'doc str> = HashSet::new();
        for ns in doc.root_element().namespaces() {
            if let Some(p) = ns.name() {
                ns_prefixes.insert(p, Some(ns.uri()));
                root_bound.insert(p);
            }
        }

        let mut nodes: Vec<ArenaNode<'doc>> = Vec::new();
        let mut last_child: Vec<u32> = Vec::new();
        let mut stack = vec![Frame::Enter(doc.root(), None, 0)];

        while let Some(frame) = stack.pop() {
            match frame {
                Frame::Finish(id) => {
                    nodes[id as usize].subtree_end = (nodes.len() - 1) as u32;
                }
                Frame::Enter(node, parent, depth) => {
                    let kind = match node.node_type() {
                        roxmltree::NodeType::Root => NodeKind::Root,
                        roxmltree::NodeType::Element => NodeKind::Element,
                        roxmltree::NodeType::Text => NodeKind::Text,
                        roxmltree::NodeType::Comment => NodeKind::Comment,
                        roxmltree::NodeType::PI => NodeKind::Pi,
                    };
                    if kind == NodeKind::Element && depth > MAX_XML_DEPTH {
                        return Err(FormulaError::Value);
                    }
                    if nodes.len() >= MAX_XML_NODES {
                        return Err(FormulaError::Value);
                    }
                    let id = nodes.len() as u32;
                    let (local, prefix, ns_uri, leaf_text) = match kind {
                        NodeKind::Root => ("", "", None, ""),
                        NodeKind::Element => {
                            let tag = node.tag_name();
                            let uri = tag.namespace();
                            let pfx = uri.and_then(|u| node.lookup_prefix(u)).unwrap_or("");
                            for ns in node.namespaces() {
                                if let Some(p) = ns.name() {
                                    if p != "xml" && !root_bound.contains(p) {
                                        ns_prefixes
                                            .entry(p)
                                            .and_modify(|slot| {
                                                if *slot != Some(ns.uri()) {
                                                    *slot = None;
                                                }
                                            })
                                            .or_insert(Some(ns.uri()));
                                    }
                                }
                            }
                            (tag.name(), pfx, uri, "")
                        }
                        NodeKind::Text | NodeKind::Comment => {
                            ("", "", None, node.text().unwrap_or(""))
                        }
                        NodeKind::Pi => {
                            let pi = node.pi().unwrap_or(roxmltree::PI {
                                target: "",
                                value: None,
                            });
                            (pi.target, "", None, pi.value.unwrap_or(""))
                        }
                        NodeKind::Attribute => unreachable!(),
                    };

                    let prev_sib = match parent {
                        Some(p) => {
                            let lc = last_child[p as usize];
                            if lc == NONE_ID {
                                nodes[p as usize].first_child = id;
                            } else {
                                nodes[lc as usize].next_sibling = id;
                            }
                            last_child[p as usize] = id;
                            lc
                        }
                        None => NONE_ID,
                    };

                    nodes.push(ArenaNode {
                        kind,
                        parent,
                        first_child: NONE_ID,
                        next_sibling: NONE_ID,
                        prev_sibling: prev_sib,
                        attr_start: 0,
                        attr_end: 0,
                        subtree_end: id,
                        local,
                        prefix,
                        ns_uri,
                        leaf_text,
                    });
                    last_child.push(NONE_ID);

                    if kind == NodeKind::Element {
                        let attr_start = nodes.len() as u32;
                        for attr in node.attributes() {
                            if attr.name() == "xmlns" || attr.name().starts_with("xmlns:") {
                                continue;
                            }
                            if nodes.len() >= MAX_XML_NODES {
                                return Err(FormulaError::Value);
                            }
                            let attr_id = nodes.len() as u32;
                            let attr_ns = attr.namespace();
                            let attr_pfx =
                                attr_ns.and_then(|u| node.lookup_prefix(u)).unwrap_or("");
                            nodes.push(ArenaNode {
                                kind: NodeKind::Attribute,
                                parent: Some(id),
                                first_child: NONE_ID,
                                next_sibling: NONE_ID,
                                prev_sibling: NONE_ID,
                                attr_start: 0,
                                attr_end: 0,
                                subtree_end: attr_id,
                                local: attr.name(),
                                prefix: attr_pfx,
                                ns_uri: attr_ns,
                                leaf_text: attr.value(),
                            });
                            last_child.push(NONE_ID);
                        }
                        let attr_end = nodes.len() as u32;
                        nodes[id as usize].attr_start = attr_start;
                        nodes[id as usize].attr_end = attr_end;
                    }

                    stack.push(Frame::Finish(id));
                    let next_depth = if kind == NodeKind::Root || kind == NodeKind::Element {
                        depth + 1
                    } else {
                        depth
                    };
                    let children: Vec<_> = node.children().collect();
                    for ch in children.into_iter().rev() {
                        stack.push(Frame::Enter(ch, Some(id), next_depth));
                    }
                }
            }
        }

        let elem_text_cache = vec![None; nodes.len()];
        Ok(Self {
            nodes,
            elem_text_cache,
            ns_prefixes,
        })
    }

    pub(super) fn string_value(
        &mut self,
        id: u32,
        budget: &mut WorkBudget,
        ctx: &mut EvalContext<'_>,
    ) -> Result<&str, FormulaError> {
        let node = &self.nodes[id as usize];
        match node.kind {
            NodeKind::Text | NodeKind::Attribute | NodeKind::Comment | NodeKind::Pi => {
                budget.charge(1, ctx)?;
                Ok(self.nodes[id as usize].leaf_text)
            }
            NodeKind::Root | NodeKind::Element => {
                let idx = id as usize;
                if self.elem_text_cache[idx].is_some() {
                    budget.charge(1, ctx)?;
                    return Ok(self.elem_text_cache[idx].as_deref().unwrap_or(""));
                }
                let end = self.nodes[idx].subtree_end as usize;
                let span = (end - idx).max(1);
                budget.charge(span, ctx)?;
                let mut acc = String::new();
                for sub in (idx + 1)..=end {
                    if self.nodes[sub].kind == NodeKind::Text {
                        acc.push_str(self.nodes[sub].leaf_text);
                        if acc.len() > MAX_XML_BYTES {
                            return Err(FormulaError::Value);
                        }
                    }
                }
                self.elem_text_cache[idx] = Some(acc.into_boxed_str());
                Ok(self.elem_text_cache[idx].as_deref().unwrap_or(""))
            }
        }
    }

    #[inline]
    pub(super) fn node_count(&self) -> usize {
        self.nodes.len()
    }
}

#[derive(Debug, Clone)]
pub(super) enum XPathVal {
    NodeSet(NodeSet),
    String(String),
    Number(f64),
    Boolean(bool),
}

impl XPathVal {
    pub(super) fn to_bool(
        &self,
        arena: &mut XmlArena<'_>,
        budget: &mut WorkBudget,
        ctx: &mut EvalContext<'_>,
    ) -> Result<bool, FormulaError> {
        Ok(match self {
            Self::Boolean(b) => *b,
            Self::Number(n) => !n.is_nan() && *n != 0.0,
            Self::String(s) => !s.is_empty(),
            Self::NodeSet(ns) => {
                let _ = (arena, budget, ctx);
                !ns.is_empty()
            }
        })
    }

    pub(super) fn into_string(
        self,
        arena: &mut XmlArena<'_>,
        budget: &mut WorkBudget,
        ctx: &mut EvalContext<'_>,
    ) -> Result<String, FormulaError> {
        Ok(match self {
            Self::String(s) => s,
            Self::Boolean(true) => "true".to_string(),
            Self::Boolean(false) => "false".to_string(),
            Self::Number(n) => xpath_number_to_string(n),
            Self::NodeSet(ns) => match ns.first() {
                Some(first) => arena.string_value(first, budget, ctx)?.to_string(),
                None => String::new(),
            },
        })
    }

    pub(super) fn into_number(
        self,
        arena: &mut XmlArena<'_>,
        budget: &mut WorkBudget,
        ctx: &mut EvalContext<'_>,
    ) -> Result<f64, FormulaError> {
        Ok(match self {
            Self::Number(n) => n,
            Self::Boolean(b) => {
                if b {
                    1.0
                } else {
                    0.0
                }
            }
            Self::String(s) => xpath_to_number(&s),
            Self::NodeSet(_) => xpath_to_number(&self.into_string(arena, budget, ctx)?),
        })
    }
}

/// W3C XPath 1.0 Section 4.4 string-to-number conversion (no `+` sign or scientific notation).
pub(super) fn xpath_to_number(s: &str) -> f64 {
    let t = s.trim_matches(|c| matches!(c, ' ' | '\t' | '\r' | '\n'));
    if t.is_empty() {
        return f64::NAN;
    }
    let body = t.strip_prefix('-').unwrap_or(t);
    if body.is_empty() {
        return f64::NAN;
    }
    let mut dot_seen = false;
    let mut digit_seen = false;
    for b in body.bytes() {
        if b.is_ascii_digit() {
            digit_seen = true;
        } else if b == b'.' && !dot_seen {
            dot_seen = true;
        } else {
            return f64::NAN;
        }
    }
    if !digit_seen {
        return f64::NAN;
    }
    t.parse::<f64>().unwrap_or(f64::NAN)
}

fn xpath_number_to_string(n: f64) -> String {
    if n.is_nan() {
        "NaN".to_string()
    } else if n == 0.0 {
        "0".to_string()
    } else if n.is_infinite() {
        if n.is_sign_positive() {
            "Infinity".to_string()
        } else {
            "-Infinity".to_string()
        }
    } else {
        format!("{n}")
    }
}

#[inline]
pub(super) fn xpath_round(n: f64) -> f64 {
    if !n.is_finite() || n == 0.0 {
        n
    } else if (-0.5..0.0).contains(&n) {
        -0.0
    } else {
        (n + 0.5).floor()
    }
}

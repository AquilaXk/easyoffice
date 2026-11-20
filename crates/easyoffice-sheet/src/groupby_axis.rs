//! The groups of one axis of `GROUPBY` and `PIVOTBY`: a tree of the distinct values of the group
//! columns, its order, and the rows (header cells and totals) the axis shows.
//!
//! 참조 구현: none. The reference engines list the two functions without a body, so this follows the
//! function documentation (`field_relationship`, `total_depth`, `sort_order`) and the engine's own
//! `SORT` key order. The labels of subtotal and grand total rows (`<item> Total`, `Total`) are
//! this engine's choice: the documentation shows its output only as pictures.

use crate::functions_spill::{cell_key, sort_key_cmp, CellKey};
use crate::{EvaluatedCellValue as V, FormulaError};
use std::cmp::Ordering;
use std::collections::HashMap;

/// A group: how many group columns lead to it, the first data row that made it (its values are read
/// from that row, never copied), and the data rows (positions in the kept rows, ascending) it
/// holds. Node 0 is the group of every row.
pub(crate) struct Node {
    /// Group columns that lead to the group: its depth, or all columns for a group of a table.
    pub(crate) level: usize,
    pub(crate) first_row: u32,
    pub(crate) parent: usize,
    pub(crate) children: Vec<usize>,
    pub(crate) rows: Vec<u32>,
}

pub(crate) struct Tree<'a> {
    pub(crate) nodes: Vec<Node>,
    /// The value of group column `level` in kept row `row`.
    key: Box<dyn Fn(usize, usize) -> V + 'a>,
    table: bool,
    /// The node of the deepest group each kept row is in.
    pub(crate) leaf_of_row: Vec<usize>,
    pub(crate) fields: usize,
}

/// How a group column is ordered: its 1-based number among the group columns and then the value
/// columns, and the direction.
#[derive(Clone, Copy)]
pub(crate) struct SortKey {
    pub(crate) column: usize,
    pub(crate) ascending: bool,
}

/// The totals an axis shows (`total_depth`): the grand total, the number of group levels with
/// subtotals, and whether they come before the rows they sum.
#[derive(Clone, Copy)]
pub(crate) struct Totals {
    pub(crate) grand: bool,
    pub(crate) subtotal_levels: usize,
    pub(crate) on_top: bool,
}

impl Totals {
    /// `total_depth` of an axis of `fields` group columns: missing is the grand total and every
    /// subtotal, 0 none, 1 the grand total, 2 and more the subtotals of that many levels less one
    /// (so they need that many columns, and a table has none).
    pub(crate) fn new(
        depth: Option<i64>,
        fields: usize,
        table: bool,
    ) -> Result<Self, FormulaError> {
        let Some(depth) = depth else {
            let levels = if table { 0 } else { fields.saturating_sub(1) };
            return Ok(Self {
                grand: true,
                subtotal_levels: levels,
                on_top: false,
            });
        };
        let wanted = usize::try_from(depth.unsigned_abs()).map_err(|_| FormulaError::Value)?;
        let levels = wanted.saturating_sub(1);
        if levels > fields.saturating_sub(1) || (table && levels > 0) {
            return Err(FormulaError::Value);
        }
        Ok(Self {
            grand: wanted > 0,
            subtotal_levels: levels,
            on_top: depth < 0,
        })
    }
}

/// What a shown row (or column) of an axis stands for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Leaf,
    /// The group of a group column's value, at this 0-based level.
    Subtotal(usize),
    Grand,
}

pub(crate) struct Entry {
    pub(crate) node: usize,
    pub(crate) kind: Kind,
}

impl<'a> Tree<'a> {
    /// The tree of `fields` group columns over `rows` kept rows; `key(level, row)` is a group
    /// value. In a table every distinct combination is one group under the root.
    pub(crate) fn build(
        fields: usize,
        rows: usize,
        table: bool,
        key: impl Fn(usize, usize) -> V + 'a,
    ) -> Self {
        let root = Node {
            level: 0,
            first_row: 0,
            parent: 0,
            children: Vec::new(),
            rows: (0..rows as u32).collect(),
        };
        let mut tree = Self {
            nodes: vec![root],
            table,
            key: Box::new(key),
            leaf_of_row: Vec::with_capacity(rows),
            fields,
        };
        let mut index: HashMap<(usize, Vec<CellKey>), usize> = HashMap::new();
        for row in 0..rows {
            let mut node = 0;
            // A table has one step to a group that holds the whole combination.
            let steps = if table { 1 } else { fields };
            for step in 0..steps {
                let columns = if table { 0..fields } else { step..step + 1 };
                let probe = (
                    node,
                    columns
                        .clone()
                        .map(|l| cell_key(&(tree.key)(l, row)))
                        .collect(),
                );
                node = match index.get(&probe) {
                    Some(&child) => child,
                    None => {
                        let child = tree.nodes.len();
                        tree.nodes.push(Node {
                            level: columns.end,
                            first_row: row as u32,
                            parent: node,
                            children: Vec::new(),
                            rows: Vec::new(),
                        });
                        tree.nodes[node].children.push(child);
                        index.insert(probe, child);
                        child
                    }
                };
                tree.nodes[node].rows.push(row as u32);
            }
            tree.leaf_of_row.push(node);
        }
        tree
    }

    /// The value of group column `l` of group `node`, read from the row that made the group (or
    /// the row that made its ancestor at that level).
    pub(crate) fn value(&self, node: usize, l: usize) -> V {
        let mut at = node;
        if !self.table {
            for _ in 0..self.nodes[node].level - (l + 1) {
                at = self.nodes[at].parent;
            }
        }
        (self.key)(l, self.nodes[at].first_row as usize)
    }

    /// The values of the group columns that lead to `node`.
    pub(crate) fn path(&self, node: usize) -> Vec<V> {
        (0..self.nodes[node].level)
            .map(|l| self.value(node, l))
            .collect()
    }

    /// Orders the children of every group by `sort`: a group column orders its own level, a value
    /// column orders every level by what `aggregate(node, value_column)` gives for the group; the
    /// default is the group value, ascending. In a table the groups are ordered by all keys in turn.
    pub(crate) fn sort(
        &mut self,
        sort: &[SortKey],
        table: bool,
        aggregate: &mut dyn FnMut(&Node, usize) -> Result<V, FormulaError>,
    ) -> Result<(), FormulaError> {
        let fields = self.fields;
        for parent in 0..self.nodes.len() {
            if self.nodes[parent].children.len() < 2 {
                continue;
            }
            let level = self.nodes[parent].level;
            let children = self.nodes[parent].children.clone();
            let mut keyed: Vec<(usize, Vec<(V, bool)>)> = Vec::with_capacity(children.len());
            for &child in &children {
                let mut keys = Vec::new();
                for entry in sort {
                    let mut push = |value: V| keys.push((value, entry.ascending));
                    if entry.column <= fields {
                        if table || entry.column == level + 1 {
                            push(self.value(child, entry.column - 1));
                        }
                    } else {
                        push(aggregate(&self.nodes[child], entry.column - fields - 1)?);
                    }
                }
                let own = self.nodes[child].level;
                let default_from = if table { 0 } else { own - 1 };
                for l in default_from..own {
                    keys.push((self.value(child, l), true));
                }
                keyed.push((child, keys));
            }
            keyed.sort_by(|a, b| {
                a.1.iter()
                    .zip(&b.1)
                    .map(|((x, asc), (y, _))| sort_key_cmp(x, y, *asc))
                    .find(|o| *o != Ordering::Equal)
                    .unwrap_or(Ordering::Equal)
            });
            self.nodes[parent].children = keyed.into_iter().map(|(child, _)| child).collect();
        }
        Ok(())
    }

    /// The shown rows of the axis in order: the groups at the deepest level, the subtotals above
    /// them and the grand total, as `totals` says.
    pub(crate) fn entries(&self, totals: Totals, table: bool) -> Vec<Entry> {
        let mut out = Vec::new();
        if totals.grand && totals.on_top {
            out.push(Entry {
                node: 0,
                kind: Kind::Grand,
            });
        }
        self.visit(0, 0, totals, table, &mut out);
        if totals.grand && !totals.on_top {
            out.push(Entry {
                node: 0,
                kind: Kind::Grand,
            });
        }
        out
    }

    fn visit(
        &self,
        parent: usize,
        level: usize,
        totals: Totals,
        table: bool,
        out: &mut Vec<Entry>,
    ) {
        for &child in &self.nodes[parent].children {
            if table || level + 1 == self.fields {
                out.push(Entry {
                    node: child,
                    kind: Kind::Leaf,
                });
                continue;
            }
            let subtotal = level < totals.subtotal_levels;
            let entry = || Entry {
                node: child,
                kind: Kind::Subtotal(level),
            };
            if subtotal && totals.on_top {
                out.push(entry());
            }
            self.visit(child, level + 1, totals, table, out);
            if subtotal && !totals.on_top {
                out.push(entry());
            }
        }
    }

    /// The header cells of a shown row: the group values of a leaf, the parent values and
    /// `<item> Total` for a subtotal, `Total` for the grand total, the rest blank.
    pub(crate) fn labels(&self, entry: &Entry) -> Vec<V> {
        let mut labels = match entry.kind {
            Kind::Leaf => self.path(entry.node),
            Kind::Subtotal(level) => {
                let mut path = self.path(entry.node);
                let item = path[level].to_display_string();
                path[level] = V::Text(format!("{item} Total"));
                path
            }
            Kind::Grand => vec![V::Text("Total".to_string())],
        };
        labels.resize(self.fields.max(1), V::Text(String::new()));
        labels
    }
}

/// The values of the sorted lists `a` and `b` that are in both.
pub(crate) fn intersect(a: &[u32], b: &[u32]) -> Vec<u32> {
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            Ordering::Less => i += 1,
            Ordering::Greater => j += 1,
            Ordering::Equal => {
                out.push(a[i]);
                i += 1;
                j += 1;
            }
        }
    }
    out
}

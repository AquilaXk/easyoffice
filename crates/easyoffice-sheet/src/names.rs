//! Defined names in formulas (PD-214): a name stands for a range, a constant or a formula, and a
//! formula that uses it reads exactly what the definition says.
//!
//! 참조 구현: Univer `packages/engine-formula/src/services/defined-names.service.ts` (the lookup
//! prefers the name of the sheet that holds the formula, then the workbook's; names compare without
//! case through one lower-cased index) and IronCalc `base/src/model.rs` (`ParsedDefinedName`: a
//! reference, a range or any other formula, local names before global ones, a definition that does
//! not parse reads as an error), Apache-2.0 or MIT. Both resolve a name while they evaluate. This
//! engine resolves it when a formula is compiled: every definition is expanded once into a syntax
//! tree whose names are already replaced (the expansion is a Kahn walk, so a cycle or a chain that is
//! too deep is found without recursion), and a formula that uses a name gets a copy of that tree.
//! The dependency graph, the evaluator and the functions that look at their arguments' shape then
//! see plain references, and nothing looks a name up while cells are evaluated.
//!
//! Bounds (a hostile file or request cannot make the table or one formula unbounded):
//! [`MAX_NAMES`] names, [`MAX_NAME_DEPTH`] names chained, [`MAX_NAME_NODES`] syntax nodes in one
//! expanded definition, [`MAX_FORMULA_NODES`] in one expanded formula. A name that is cyclic reads
//! `#NAME?`; one too deep or too large reads `#VALUE!`; an unknown name reads `#NAME?`.

use crate::parser::parse_formula_in;
use crate::table_index::{TableCtx, TableIndex};
use crate::SharedExpr;
use crate::{Expr, FormulaError, PackedCellCoordinate};
use easyoffice_formats::{
    defined_name_problem, fold_name, DefinedName, EasyOfficeError, MAX_NAMES,
    MAX_NAME_COMMENT_CHARS, MAX_REFERS_TO_CHARS,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Names chained through each other (`a = b + 1`, `b = c + 1`, ...).
pub const MAX_NAME_DEPTH: usize = 64;
/// Syntax nodes of one definition after its names are expanded.
pub const MAX_NAME_NODES: usize = 4_096;
/// Syntax nodes of one formula after its names are expanded (a formula may have 8,192 tokens).
pub const MAX_FORMULA_NODES: usize = 8_192;
/// Nodes the definitions of a workbook hold in memory, parsed and expanded (a tree a formula shares is counted once).
pub const MAX_TABLE_NODES: usize = 1_048_576;
/// Nodes the formulas of a workbook hold in memory because they use names: the copies of a definition's top layer
/// and the pointers to the shared trees. A formula past it reads `#VALUE!`.
pub const MAX_WORKBOOK_NAME_NODES: usize = 2_097_152;

/// Hash of a folded name: what a formula remembers about the names it uses (collisions only make
/// a formula compile again when it did not have to).
pub(crate) type NameKey = u64;

pub(crate) fn name_key(folded: &str) -> NameKey {
    folded.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn same_sheet(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b) || a.to_lowercase() == b.to_lowercase()
}

/// One definition, expanded. `expr` is `Err` when the definition cannot be read or its names cannot
/// be resolved.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Resolved {
    scope: Option<Box<str>>,
    expr: Result<Arc<SharedExpr>, FormulaError>,
    /// Nodes the tree holds in memory (a shared tree inside it counts one).
    phys: usize,
}

/// An expanded definition while the table is built.
struct Done {
    tree: Arc<SharedExpr>,
    phys: usize,
}

/// Why a definition is not usable as written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameIssue {
    /// The name breaks the syntax of a name.
    BadName,
    /// The name is the name of a built-in function or of a sheet.
    Reserved,
    /// Another definition has the same name and scope.
    Duplicate,
    /// The scope is not a sheet of the workbook.
    UnknownScope,
    /// `refers_to` is empty, too long or does not read as a formula.
    BadFormula,
    /// The comment is too long.
    BadComment,
    /// The definition leads back to itself through other names.
    Cycle,
    /// Too many names are chained, or the expansion is too large.
    TooLarge,
}

/// The names of a workbook, expanded and indexed by folded name.
#[derive(Debug, Clone, Default)]
pub struct NameTable {
    defs: Vec<DefinedName>,
    index: HashMap<Box<str>, Vec<Resolved>>,
    /// A definition has a structured reference, so a change of the tables builds the table again (PD-225).
    uses_tables: bool,
    /// Nodes the expanded definitions hold in memory.
    phys_total: usize,
}

/// What [`NameTable::build`] found next to the table.
pub(crate) struct Built {
    pub table: NameTable,
    /// The first problem of each definition that has one, by position in the list given.
    pub issues: Vec<(usize, NameIssue)>,
}

struct Parsed {
    folded: String,
    expr: Result<Expr, FormulaError>,
    deps: Vec<usize>,
}

pub(crate) fn is_function(name: &str) -> bool {
    static FUNCTIONS: std::sync::OnceLock<HashSet<&'static str>> = std::sync::OnceLock::new();
    FUNCTIONS
        .get_or_init(|| {
            crate::SheetEngine::supported_function_names()
                .iter()
                .copied()
                .collect()
        })
        .contains(name.to_ascii_uppercase().as_str())
}

/// Visits every node under `root`, parents first, without recursion; `visit` gets the node and whether it is
/// an argument of a function call, and answers whether to go down into the node's children (it may have
/// replaced the node). A shared tree is not entered.
pub(crate) fn walk_mut(root: &mut Expr, mut visit: impl FnMut(&mut Expr, bool) -> bool) {
    let mut stack: Vec<(&mut Expr, bool)> = vec![(root, false)];
    while let Some((expr, in_arg)) = stack.pop() {
        if !visit(&mut *expr, in_arg) {
            continue;
        }
        match expr {
            Expr::UnaryPlus(inner) | Expr::UnaryMinus(inner) | Expr::PostfixPercent(inner) => {
                stack.push((&mut **inner, false));
            }
            Expr::Binary { lhs, rhs, .. } => {
                stack.push((&mut **lhs, false));
                stack.push((&mut **rhs, false));
            }
            Expr::FuncCall { args, .. } => stack.extend(args.iter_mut().map(|a| (a, true))),
            Expr::Call { callee, args } => {
                stack.extend(args.iter_mut().map(|a| (a, true)));
                stack.push((&mut **callee, false));
            }
            _ => {}
        }
    }
}

/// Visits every node under `root` without changing it. A shared tree is visited as one node.
pub(crate) fn walk(root: &Expr, mut visit: impl FnMut(&Expr)) {
    let mut stack: Vec<&Expr> = vec![root];
    while let Some(expr) = stack.pop() {
        visit(expr);
        match expr {
            Expr::UnaryPlus(inner) | Expr::UnaryMinus(inner) | Expr::PostfixPercent(inner) => {
                stack.push(inner);
            }
            Expr::Binary { lhs, rhs, .. } => {
                stack.push(lhs);
                stack.push(rhs);
            }
            Expr::FuncCall { args, .. } => stack.extend(args.iter()),
            Expr::Call { callee, args } => {
                stack.push(callee);
                stack.extend(args.iter());
            }
            _ => {}
        }
    }
}

/// Nodes `root` stands for, every shared tree counted in full: what evaluating it takes.
pub(crate) fn logical_count(root: &Expr) -> usize {
    let mut n = 0usize;
    walk(root, |node| {
        n = n.saturating_add(match node {
            Expr::Shared(shared) => shared.nodes,
            _ => 1,
        });
    });
    n
}

/// Nodes `root` holds in memory: a shared tree counts one.
pub(crate) fn physical_count(root: &Expr) -> usize {
    let mut n = 0usize;
    walk(root, |_| n += 1);
    n
}

/// A tree that is a reference or a constant: small enough to copy where a name stands, and what the functions that
/// look at the shape of an argument have to see.
fn is_leaf(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Cell(_)
            | Expr::Range(_)
            | Expr::SheetCell { .. }
            | Expr::SheetRange { .. }
            | Expr::Number(_)
            | Expr::Boolean(_)
            | Expr::ErrorLiteral(_)
            | Expr::Missing
    )
}

/// What a name is replaced by.
enum Repl<'a> {
    Tree(&'a Arc<SharedExpr>, usize),
    /// A reference that is no name's definition: the data of a table that has the name.
    Leaf(Expr),
    Error(FormulaError),
}

/// Replaces the names in `expr` by the trees they stand for and returns the nodes the result stands for and holds.
/// A reference or constant, or any tree where a function reads an argument, is copied (its inner shared trees are
/// not); a tree in an operand or a whole formula is shared. The size is checked before each replacement, so an
/// exponential chain stops at `max_nodes` instead of growing. `Err` past the bound.
fn replace_names<'a>(
    expr: &mut Expr,
    max_nodes: usize,
    mut resolve: impl FnMut(Option<&str>, &str) -> Repl<'a>,
) -> Result<(usize, usize), FormulaError> {
    let mut logical = logical_count(expr);
    let mut physical = physical_count(expr);
    let mut too_large = false;
    walk_mut(expr, |node, in_arg| {
        if too_large {
            return false;
        }
        let Expr::Name { scope, name } = node else {
            return true;
        };
        let replacement = match resolve(scope.as_deref(), name) {
            Repl::Error(e) => Expr::ErrorLiteral(e),
            Repl::Leaf(expr) => expr,
            Repl::Tree(tree, phys) => {
                logical = logical.saturating_add(tree.nodes - 1);
                if logical > max_nodes {
                    too_large = true;
                    return false;
                }
                if in_arg || is_leaf(&tree.expr) {
                    physical = physical.saturating_add(phys - 1);
                    tree.expr.clone()
                } else {
                    Expr::Shared(Arc::clone(tree))
                }
            }
        };
        *node = replacement;
        false
    });
    if too_large {
        Err(FormulaError::Value)
    } else {
        Ok((logical, physical))
    }
}

/// `refers_to` without a leading `=` and surrounding blanks.
fn formula_text(refers_to: &str) -> &str {
    let t = refers_to.trim();
    t.strip_prefix('=').map_or(t, str::trim_start)
}

impl NameTable {
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Whether a definition has a structured reference (a change of the tables builds the names again).
    pub(crate) fn uses_tables(&self) -> bool {
        self.uses_tables
    }

    /// The definitions in the order they were given.
    pub fn definitions(&self) -> &[DefinedName] {
        &self.defs
    }

    /// Characters of the definitions' text: what the table costs to hold.
    pub(crate) fn text_bytes(&self) -> usize {
        self.defs
            .iter()
            .map(|d| d.name.len() + d.refers_to.len())
            .sum()
    }

    /// The expanded definition of `folded` for a formula on `sheet`: that sheet's own name first, then
    /// the workbook's (`Sheet1!Name` asks for the names of `Sheet1`).
    fn lookup(&self, folded: &str, sheet: &str) -> Option<&Resolved> {
        let entries = self.index.get(folded)?;
        entries
            .iter()
            .find(|r| r.scope.as_deref().is_some_and(|s| same_sheet(s, sheet)))
            .or_else(|| entries.iter().find(|r| r.scope.is_none()))
    }

    /// The expanded definition of `folded` for a formula on `sheet` (see [`Self::lookup`]).
    pub(crate) fn resolved_expr(
        &self,
        folded: &str,
        sheet: &str,
    ) -> Option<&Result<Arc<SharedExpr>, FormulaError>> {
        self.lookup(folded, sheet).map(|r| &r.expr)
    }

    /// Nodes the expanded definitions hold in memory.
    pub(crate) fn physical_nodes(&self) -> usize {
        self.phys_total
    }

    /// Builds the table of `defs` for a workbook with `sheets`. Every definition with a problem is
    /// listed in `issues`; those with a bad name, a duplicate or an unknown scope are left out of
    /// the table, the others stay in it reading as errors. `strict` also refuses a name that is
    /// a function's or a sheet's (a name entered by a user); a file may have one, and a formula tells
    /// it from the function by its parenthesis.
    pub(crate) fn build(
        defs: &[DefinedName],
        sheets: &[&str],
        strict: bool,
        tables: &TableIndex,
    ) -> Built {
        let mut issues: Vec<(usize, NameIssue)> = Vec::new();
        let mut parsed: Vec<Option<Parsed>> = Vec::with_capacity(defs.len());
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::with_capacity(defs.len());
        let first_sheet = sheets.first().copied();
        let mut parsed_nodes = 0usize;
        let mut uses_tables = false;
        for (i, def) in defs.iter().enumerate() {
            let folded = fold_name(&def.name);
            let mut issue = Self::check_header(def, sheets, strict);
            if issue.is_none()
                && by_name.get(&folded).is_some_and(|ids| {
                    ids.iter()
                        .any(|&j| scope_eq(defs[j].scope.as_deref(), def.scope.as_deref()))
                })
            {
                issue = Some(NameIssue::Duplicate);
            }
            if let Some(
                fatal @ (NameIssue::BadName
                | NameIssue::Reserved
                | NameIssue::Duplicate
                | NameIssue::UnknownScope),
            ) = issue
            {
                issues.push((i, fatal));
                parsed.push(None);
                continue;
            }
            by_name.entry(folded.clone()).or_default().push(i);
            let owner = def.scope.as_deref().or(first_sheet).unwrap_or_default();
            let mut expr = parse_names_expr(&def.refers_to, owner, tables, &mut uses_tables);
            if expr.is_err() {
                issue = issue.or(Some(NameIssue::BadFormula));
            }
            if let Ok(e) = &expr {
                parsed_nodes = parsed_nodes.saturating_add(physical_count(e));
                if parsed_nodes > MAX_TABLE_NODES {
                    expr = Err(FormulaError::Value);
                    issue = Some(NameIssue::TooLarge);
                }
            }
            if let Some(issue) = issue {
                issues.push((i, issue));
            }
            if let (Ok(e), Some(owner)) = (expr.as_mut(), def.scope.as_deref().or(first_sheet)) {
                qualify_references(e, owner);
            }
            parsed.push(Some(Parsed {
                folded,
                expr,
                deps: Vec::new(),
            }));
        }
        let mut table = NameTable {
            defs: defs.to_vec(),
            index: HashMap::new(),
            uses_tables,
            phys_total: 0,
        };
        let mut resolved = expand_all(&mut parsed, &by_name, defs, &mut issues);
        for (i, entry) in parsed.into_iter().enumerate() {
            let (Some(entry), Some(done)) = (entry, resolved[i].take()) else {
                continue;
            };
            let (expr, phys) = match done {
                Ok(done) => (Ok(done.tree), done.phys),
                Err(e) => (Err(e), 1),
            };
            table.phys_total += phys;
            table
                .index
                .entry(entry.folded.into_boxed_str())
                .or_default()
                .push(Resolved {
                    scope: defs[i].scope.as_deref().map(Box::from),
                    expr,
                    phys,
                });
        }
        issues.sort_by_key(|&(i, _)| i);
        issues.dedup_by_key(|&mut (i, _)| i);
        Built { table, issues }
    }

    fn check_header(def: &DefinedName, sheets: &[&str], strict: bool) -> Option<NameIssue> {
        if defined_name_problem(&def.name).is_some() {
            return Some(NameIssue::BadName);
        }
        if strict && (is_function(&def.name) || sheets.iter().any(|s| same_sheet(s, &def.name))) {
            return Some(NameIssue::Reserved);
        }
        if let Some(scope) = def.scope.as_deref() {
            if !sheets.iter().any(|s| same_sheet(s, scope)) {
                return Some(NameIssue::UnknownScope);
            }
        }
        if def.refers_to.chars().count() > MAX_REFERS_TO_CHARS
            || formula_text(&def.refers_to).is_empty()
        {
            return Some(NameIssue::BadFormula);
        }
        (def.comment.chars().count() > MAX_NAME_COMMENT_CHARS).then_some(NameIssue::BadComment)
    }

    /// The names whose meaning differs between `self` and `next` (added, removed or redefined,
    /// including those that use a changed name).
    pub(crate) fn changed_keys(&self, next: &NameTable) -> HashSet<NameKey> {
        let mut changed = HashSet::new();
        for (name, entries) in &next.index {
            if self.index.get(name) != Some(entries) {
                changed.insert(name_key(name));
            }
        }
        for name in self.index.keys() {
            if !next.index.contains_key(name) {
                changed.insert(name_key(name));
            }
        }
        changed
    }

    /// Replaces the names in `expr` (a formula on `sheet`) by their definitions (see [`replace_names`]). Records the
    /// key of every name met in `uses`. Returns the nodes the formula stands for and the nodes it holds; `Err` when
    /// it would stand for more than [`MAX_FORMULA_NODES`].
    pub(crate) fn expand(
        &self,
        expr: &mut Expr,
        sheet: &str,
        tables: &TableIndex,
        coord: Option<PackedCellCoordinate>,
        uses: &mut Vec<NameKey>,
    ) -> Result<(usize, usize), FormulaError> {
        let ctx = TableCtx {
            index: tables,
            sheet,
            coord,
        };
        let mut folded = String::new();
        // `Square(3)` where `Square` is a defined name: a call of the value the name stands for.
        walk_mut(expr, |node, _| {
            if let Expr::FuncCall { name, args } = node {
                if !is_function(name) && self.lookup(&fold_name(name), sheet).is_some() {
                    let callee = Expr::Name {
                        scope: None,
                        name: std::mem::take(name),
                    };
                    *node = Expr::Call {
                        callee: Box::new(callee),
                        args: std::mem::take(args),
                    };
                }
            }
            true
        });
        let sizes = replace_names(expr, MAX_FORMULA_NODES, |scope, name| {
            folded.clear();
            if name.is_ascii() {
                folded.push_str(&name.to_ascii_lowercase());
            } else {
                folded.push_str(&name.to_lowercase());
            }
            uses.push(name_key(&folded));
            match self.lookup(&folded, scope.unwrap_or(sheet)) {
                Some(entry) => match &entry.expr {
                    Ok(tree) => Repl::Tree(tree, entry.phys),
                    Err(e) => Repl::Error(*e),
                },
                // No name: a table of the name is its data (`SUM(Table1)`).
                None => match tables.data_of(&folded, &ctx) {
                    Some(data) => Repl::Leaf(data),
                    None => Repl::Error(FormulaError::Name),
                },
            }
        });
        uses.sort_unstable();
        uses.dedup();
        sizes
    }
}

fn scope_eq(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => same_sheet(a, b),
        _ => false,
    }
}

/// A definition parsed; its structured references are resolved for the sheet `owner` (they have no cell, so
/// `#This Row` reads `#VALUE!`). `uses_tables` is set when it has one.
fn parse_names_expr(
    refers_to: &str,
    owner: &str,
    tables: &TableIndex,
    uses_tables: &mut bool,
) -> Result<Expr, FormulaError> {
    let ctx = TableCtx {
        index: tables,
        sheet: owner,
        coord: None,
    };
    let parsed = parse_formula_in(formula_text(refers_to), Some(&ctx))?;
    *uses_tables |= !parsed.table_keys.is_empty();
    Ok(parsed.expr)
}

/// A reference without a sheet in a definition points at the sheet the definition belongs to.
fn qualify_references(expr: &mut Expr, owner: &str) {
    walk_mut(expr, |node, _| {
        match node {
            Expr::Cell(coord) => {
                *node = Expr::SheetCell {
                    sheet: owner.to_string(),
                    coord: *coord,
                };
            }
            Expr::Range(range) => {
                *node = Expr::SheetRange {
                    sheet: owner.to_string(),
                    range: *range,
                };
            }
            _ => {}
        }
        true
    });
}

/// Expands every parsed definition in dependency order (Kahn). The result of each is its tree, or
/// the error it reads as; `None` for a definition left out of the table.
fn expand_all(
    parsed: &mut [Option<Parsed>],
    by_name: &HashMap<String, Vec<usize>>,
    defs: &[DefinedName],
    issues: &mut Vec<(usize, NameIssue)>,
) -> Vec<Option<Result<Done, FormulaError>>> {
    let n = parsed.len();
    let pick = |folded: &str, scope: Option<&str>| -> Option<usize> {
        let ids = by_name.get(folded)?;
        ids.iter()
            .copied()
            .find(|&j| {
                scope.is_some_and(|s| defs[j].scope.as_deref().is_some_and(|d| same_sheet(d, s)))
            })
            .or_else(|| ids.iter().copied().find(|&j| defs[j].scope.is_none()))
    };
    let mut reverse: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut pending = vec![0usize; n];
    for i in 0..n {
        let Some(entry) = parsed[i].as_mut() else {
            continue;
        };
        let Ok(expr) = &entry.expr else {
            continue;
        };
        let mut deps = Vec::new();
        let scope = defs[i].scope.as_deref();
        walk(expr, |node| {
            if let Expr::Name { scope: q, name } = node {
                let folded = fold_name(name);
                if let Some(j) = pick(&folded, q.as_deref().or(scope)) {
                    deps.push(j);
                }
            }
        });
        deps.sort_unstable();
        deps.dedup();
        pending[i] = deps.len();
        for &j in &deps {
            reverse[j].push(i);
        }
        entry.deps = deps;
    }
    let mut out: Vec<Option<Result<Done, FormulaError>>> = (0..n).map(|_| None).collect();
    let mut table_nodes = 0usize;
    let mut depth = vec![0usize; n];
    let mut ready: Vec<usize> = (0..n)
        .filter(|&i| parsed[i].is_some() && pending[i] == 0)
        .collect();
    while let Some(i) = ready.pop() {
        let entry = parsed[i].as_mut().expect("ready entries are parsed");
        let level = 1 + entry.deps.iter().map(|&j| depth[j]).max().unwrap_or(0);
        depth[i] = level;
        let (result, issue) = match std::mem::replace(&mut entry.expr, Err(FormulaError::Name)) {
            Err(e) => (Err(e), None),
            Ok(_) if level > MAX_NAME_DEPTH => {
                (Err(FormulaError::Value), Some(NameIssue::TooLarge))
            }
            Ok(mut expr) => {
                let scope = defs[i].scope.as_deref();
                let sizes = replace_names(&mut expr, MAX_NAME_NODES, |q, name| {
                    match pick(&fold_name(name), q.or(scope)).and_then(|j| out[j].as_ref()) {
                        Some(Ok(done)) => Repl::Tree(&done.tree, done.phys),
                        Some(Err(e)) => Repl::Error(*e),
                        None => Repl::Error(FormulaError::Name),
                    }
                });
                // A definition that is only another name (`b = a`) comes out as that name's shared tree: it is stored
                // as the tree's top layer, so the top of a stored definition is always the real call or reference.
                let sizes = sizes.map(|(logical, phys)| {
                    let mut phys = phys;
                    while let Expr::Shared(inner) = &expr {
                        expr = inner.expr.clone();
                        phys = physical_count(&expr);
                    }
                    (logical, phys)
                });
                match sizes {
                    Ok((logical, phys)) if table_nodes.saturating_add(phys) <= MAX_TABLE_NODES => {
                        table_nodes += phys;
                        let tree = Arc::new(SharedExpr {
                            expr,
                            nodes: logical,
                        });
                        (Ok(Done { tree, phys }), None)
                    }
                    Ok(_) | Err(_) => (Err(FormulaError::Value), Some(NameIssue::TooLarge)),
                }
            }
        };
        if let Some(issue) = issue {
            issues.push((i, issue));
        }
        out[i] = Some(result);
        for &k in &reverse[i] {
            pending[k] -= 1;
            if pending[k] == 0 {
                ready.push(k);
            }
        }
    }
    for i in 0..n {
        if parsed[i].is_some() && out[i].is_none() {
            issues.push((i, NameIssue::Cycle));
            out[i] = Some(Err(FormulaError::Name));
        }
    }
    out
}

/// The error a request to change the names answers for the definition at `index` with `issue`.
pub(crate) fn issue_error(defs: &[DefinedName], index: usize, issue: NameIssue) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(format!(
        "이름 정의가 올바르지 않습니다({}번째 {:?}): {}",
        index + 1,
        issue,
        defs[index].name
    ))
}

/// Checks `defs` the way a request to change the names is checked: the first problem is an error.
pub fn validate_names(defs: &[DefinedName], sheets: &[&str]) -> Result<(), EasyOfficeError> {
    if defs.len() > MAX_NAMES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "이름은 최대 {MAX_NAMES}개까지 정의할 수 있습니다."
        )));
    }
    match NameTable::build(defs, sheets, true, &TableIndex::default())
        .issues
        .first()
    {
        None => Ok(()),
        Some(&(i, issue)) => Err(issue_error(defs, i, issue)),
    }
}

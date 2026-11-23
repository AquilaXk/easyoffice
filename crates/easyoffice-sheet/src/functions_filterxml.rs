//! `FILTERXML(xml, xpath)`: bounded cleanroom W3C XPath 1.0 evaluator (PD-320).
//!
//! 참조 구현: none (built cleanly from W3C XPath 1.0 specification and Excel `FILTERXML`
//! documentation). Parses XML with the workspace `roxmltree` crate (`MIT OR Apache-2.0`,
//! `allow_dtd: false`) after running `XmlSanitizer::validate_safe_xml`, indexes nodes into a
//! flat pre-order `XmlArena`, and charges every axis visit, predicate evaluation, and function
//! call against a fixed runtime `WorkBudget` (`MAX_XPATH_OPS`) plus `EvalContext::charge_steps`
//! so pathological nested predicates/scans fail closed with `#VALUE!`.

#[path = "xpath_arena.rs"]
mod arena;
#[path = "functions_filterxml_core.rs"]
mod core_fns;

use crate::array::{Env, Grid, Val};
use crate::array_eval::ValResult;
use crate::coercion::parse_strict_number;
use crate::xpath_ast::{parse_xpath, Axis, BinOp, NodeTest, Step, XExpr, MAX_XPATH_DEPTH};
use crate::{EvalContext, EvaluatedCellValue as V, Expr, FormulaError, SheetEngine};
use arena::{
    xpath_round, xpath_to_number, NodeKind, NodeSet, WorkBudget, XPathVal, XmlArena, MAX_XML_BYTES,
    MAX_XML_NODES, NONE_ID,
};
use easyoffice_formats::XmlSanitizer;
use std::collections::HashSet;

const MAX_CELL_TEXT_CHARS: usize = 32_767;

fn validate_qname_prefixes(expr: &XExpr, arena: &XmlArena<'_>) -> Result<(), FormulaError> {
    fn check_test(test: &NodeTest, arena: &XmlArena<'_>) -> Result<(), FormulaError> {
        match test {
            NodeTest::QName(p, _) | NodeTest::PrefixStar(p) => {
                match arena.ns_prefixes.get(p.as_str()) {
                    Some(Some(_)) => Ok(()),
                    _ => Err(FormulaError::Value),
                }
            }
            _ => Ok(()),
        }
    }
    fn check_step(step: &Step, arena: &XmlArena<'_>) -> Result<(), FormulaError> {
        check_test(&step.test, arena)?;
        for pred in &step.predicates {
            validate_qname_prefixes(pred, arena)?;
        }
        Ok(())
    }
    match expr {
        XExpr::LocPath { steps, .. } => {
            for s in steps {
                check_step(s, arena)?;
            }
        }
        XExpr::Filter {
            base,
            predicates,
            tail,
        } => {
            validate_qname_prefixes(base, arena)?;
            for p in predicates {
                validate_qname_prefixes(p, arena)?;
            }
            for s in tail {
                check_step(s, arena)?;
            }
        }
        XExpr::Call { args, .. } => {
            for a in args {
                validate_qname_prefixes(a, arena)?;
            }
        }
        XExpr::UnaryNeg(inner) => validate_qname_prefixes(inner, arena)?,
        XExpr::Binary { left, right, .. } => {
            validate_qname_prefixes(left, arena)?;
            validate_qname_prefixes(right, arena)?;
        }
        XExpr::Literal(_) | XExpr::Number(_) => {}
    }
    Ok(())
}

fn matches_test(arena: &XmlArena<'_>, id: u32, axis: Axis, test: &NodeTest) -> bool {
    let n = &arena.nodes[id as usize];
    match test {
        NodeTest::Star => {
            if axis == Axis::Attribute {
                n.kind == NodeKind::Attribute
            } else {
                n.kind == NodeKind::Element
            }
        }
        NodeTest::Name(local) => {
            let principal = if axis == Axis::Attribute {
                NodeKind::Attribute
            } else {
                NodeKind::Element
            };
            n.kind == principal && n.ns_uri.is_none() && n.local == local
        }
        NodeTest::QName(pfx, local) => {
            let principal = if axis == Axis::Attribute {
                NodeKind::Attribute
            } else {
                NodeKind::Element
            };
            let uri = arena.ns_prefixes.get(pfx.as_str()).and_then(|o| *o);
            n.kind == principal && uri.is_some() && n.ns_uri == uri && n.local == local
        }
        NodeTest::PrefixStar(pfx) => {
            let principal = if axis == Axis::Attribute {
                NodeKind::Attribute
            } else {
                NodeKind::Element
            };
            let uri = arena.ns_prefixes.get(pfx.as_str()).and_then(|o| *o);
            n.kind == principal && uri.is_some() && n.ns_uri == uri
        }
        NodeTest::AnyNode => true,
        NodeTest::Text => n.kind == NodeKind::Text,
        NodeTest::Comment => n.kind == NodeKind::Comment,
        NodeTest::Pi(target) => {
            n.kind == NodeKind::Pi
                && match target {
                    Some(t) => n.local == t,
                    None => true,
                }
        }
    }
}

fn collect_axis_candidates(
    arena: &XmlArena<'_>,
    ctx_node: u32,
    axis: Axis,
    test: &NodeTest,
    budget: &mut WorkBudget,
    ctx: &mut EvalContext<'_>,
) -> Result<Vec<u32>, FormulaError> {
    let mut out = Vec::new();
    let total = arena.nodes.len() as u32;
    let node = &arena.nodes[ctx_node as usize];
    match axis {
        Axis::SelfNode => {
            budget.charge(1, ctx)?;
            if matches_test(arena, ctx_node, axis, test) {
                out.push(ctx_node);
            }
        }
        Axis::Parent => {
            budget.charge(1, ctx)?;
            if let Some(p) = node.parent {
                if matches_test(arena, p, axis, test) {
                    out.push(p);
                }
            }
        }
        Axis::Child => {
            let mut cur = node.first_child;
            while cur != NONE_ID {
                budget.charge(1, ctx)?;
                if matches_test(arena, cur, axis, test) {
                    out.push(cur);
                }
                cur = arena.nodes[cur as usize].next_sibling;
            }
        }
        Axis::Attribute => {
            if node.kind == NodeKind::Element {
                for id in node.attr_start..node.attr_end {
                    budget.charge(1, ctx)?;
                    if matches_test(arena, id, axis, test) {
                        out.push(id);
                    }
                }
            }
        }
        Axis::Ancestor | Axis::AncestorOrSelf => {
            if axis == Axis::AncestorOrSelf {
                budget.charge(1, ctx)?;
                if matches_test(arena, ctx_node, axis, test) {
                    out.push(ctx_node);
                }
            }
            let mut cur = node.parent;
            while let Some(p) = cur {
                budget.charge(1, ctx)?;
                if matches_test(arena, p, axis, test) {
                    out.push(p);
                }
                cur = arena.nodes[p as usize].parent;
            }
        }
        Axis::Descendant | Axis::DescendantOrSelf => {
            if axis == Axis::DescendantOrSelf {
                budget.charge(1, ctx)?;
                if matches_test(arena, ctx_node, axis, test) {
                    out.push(ctx_node);
                }
            }
            if matches!(node.kind, NodeKind::Root | NodeKind::Element) {
                for id in (ctx_node + 1)..=node.subtree_end {
                    budget.charge(1, ctx)?;
                    if arena.nodes[id as usize].kind != NodeKind::Attribute
                        && matches_test(arena, id, axis, test)
                    {
                        out.push(id);
                    }
                }
            }
        }
        Axis::FollowingSibling => {
            if node.kind != NodeKind::Attribute {
                let mut cur = node.next_sibling;
                while cur != NONE_ID {
                    budget.charge(1, ctx)?;
                    if matches_test(arena, cur, axis, test) {
                        out.push(cur);
                    }
                    cur = arena.nodes[cur as usize].next_sibling;
                }
            }
        }
        Axis::PrecedingSibling => {
            if node.kind != NodeKind::Attribute {
                let mut cur = node.prev_sibling;
                while cur != NONE_ID {
                    budget.charge(1, ctx)?;
                    if matches_test(arena, cur, axis, test) {
                        out.push(cur);
                    }
                    cur = arena.nodes[cur as usize].prev_sibling;
                }
            }
        }
        Axis::Following => {
            let start = node.subtree_end + 1;
            for id in start..total {
                budget.charge(1, ctx)?;
                if arena.nodes[id as usize].kind != NodeKind::Attribute
                    && matches_test(arena, id, axis, test)
                {
                    out.push(id);
                }
            }
        }
        Axis::Preceding => {
            let mut ancestors = Vec::new();
            let mut p = node.parent;
            while let Some(pid) = p {
                ancestors.push(pid);
                p = arena.nodes[pid as usize].parent;
            }
            let mut id = ctx_node;
            while id > 1 {
                id -= 1;
                budget.charge(1, ctx)?;
                if arena.nodes[id as usize].kind != NodeKind::Attribute
                    && !ancestors.contains(&id)
                    && matches_test(arena, id, axis, test)
                {
                    out.push(id);
                }
            }
        }
    }
    Ok(out)
}

fn filter_candidates(
    mut list: Vec<u32>,
    predicates: &[XExpr],
    arena: &mut XmlArena<'_>,
    budget: &mut WorkBudget,
    ctx: &mut EvalContext<'_>,
    depth: usize,
) -> Result<Vec<u32>, FormulaError> {
    for pred in predicates {
        let size = list.len();
        if size == 0 {
            break;
        }
        let mut next = Vec::with_capacity(size);
        for (idx, &id) in list.iter().enumerate() {
            let val = eval_xexpr(pred, arena, id, idx + 1, size, budget, ctx, depth + 1)?;
            let keep = match val {
                XPathVal::Number(n) => (idx + 1) as f64 == n,
                other => other.to_bool(arena, budget, ctx)?,
            };
            if keep {
                next.push(id);
            }
        }
        list = next;
    }
    Ok(list)
}

fn eval_steps(
    initial: NodeSet,
    steps: &[Step],
    arena: &mut XmlArena<'_>,
    budget: &mut WorkBudget,
    ctx: &mut EvalContext<'_>,
    depth: usize,
) -> Result<NodeSet, FormulaError> {
    let mut current = initial;
    for step in steps {
        let mut next = NodeSet::empty(arena.node_count());
        for ctx_node in current.iter() {
            let cands =
                collect_axis_candidates(arena, ctx_node, step.axis, &step.test, budget, ctx)?;
            let kept = filter_candidates(cands, &step.predicates, arena, budget, ctx, depth)?;
            for id in kept {
                next.insert(id);
            }
        }
        current = next;
        if current.is_empty() {
            break;
        }
    }
    Ok(current)
}

#[inline]
fn cmp_f64_rel(op: BinOp, a: f64, b: f64) -> bool {
    match op {
        BinOp::Lt => a < b,
        BinOp::Le => a <= b,
        BinOp::Gt => a > b,
        BinOp::Ge => a >= b,
        _ => false,
    }
}

fn eval_eq(
    op: BinOp,
    lhs: XPathVal,
    rhs: XPathVal,
    arena: &mut XmlArena<'_>,
    budget: &mut WorkBudget,
    ctx: &mut EvalContext<'_>,
) -> Result<bool, FormulaError> {
    let ne = op == BinOp::Ne;
    match (lhs, rhs) {
        (XPathVal::NodeSet(l), XPathVal::NodeSet(r)) => {
            if l.is_empty() || r.is_empty() {
                return Ok(false);
            }
            if !ne {
                let mut set: HashSet<String> = HashSet::new();
                for id in l.iter() {
                    set.insert(arena.string_value(id, budget, ctx)?.to_string());
                }
                for id in r.iter() {
                    if set.contains(arena.string_value(id, budget, ctx)?) {
                        return Ok(true);
                    }
                }
                Ok(false)
            } else {
                let mut first: Option<String> = None;
                let mut multi = false;
                for id in l.iter() {
                    let s = arena.string_value(id, budget, ctx)?;
                    match &first {
                        None => first = Some(s.to_string()),
                        Some(prev) if prev != s => {
                            multi = true;
                            break;
                        }
                        _ => {}
                    }
                }
                if multi {
                    return Ok(true);
                }
                let Some(s0) = first else { return Ok(false) };
                for id in r.iter() {
                    if arena.string_value(id, budget, ctx)? != s0 {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
        (XPathVal::NodeSet(ns), XPathVal::Number(num))
        | (XPathVal::Number(num), XPathVal::NodeSet(ns)) => {
            for id in ns.iter() {
                let lv = xpath_to_number(arena.string_value(id, budget, ctx)?);
                if if ne { lv != num } else { lv == num } {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        (XPathVal::NodeSet(ns), XPathVal::String(s))
        | (XPathVal::String(s), XPathVal::NodeSet(ns)) => {
            for id in ns.iter() {
                let sv = arena.string_value(id, budget, ctx)?;
                if if ne { sv != s } else { sv == s } {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        (XPathVal::NodeSet(ns), XPathVal::Boolean(b))
        | (XPathVal::Boolean(b), XPathVal::NodeSet(ns)) => Ok((ns.is_empty() != b) ^ ne),
        (XPathVal::Boolean(a), other) | (other, XPathVal::Boolean(a)) => {
            let b = other.to_bool(arena, budget, ctx)?;
            Ok((a == b) ^ ne)
        }
        (XPathVal::Number(a), other) | (other, XPathVal::Number(a)) => {
            let b = other.into_number(arena, budget, ctx)?;
            Ok(if ne { a != b } else { a == b })
        }
        (XPathVal::String(a), XPathVal::String(b)) => Ok((a == b) ^ ne),
    }
}

fn nodeset_min_max(
    ns: &NodeSet,
    arena: &mut XmlArena<'_>,
    budget: &mut WorkBudget,
    ctx: &mut EvalContext<'_>,
) -> Result<Option<(f64, f64)>, FormulaError> {
    let mut bounds: Option<(f64, f64)> = None;
    for id in ns.iter() {
        let n = xpath_to_number(arena.string_value(id, budget, ctx)?);
        if !n.is_nan() {
            bounds = Some(match bounds {
                None => (n, n),
                Some((mn, mx)) => (mn.min(n), mx.max(n)),
            });
        }
    }
    Ok(bounds)
}

fn eval_rel(
    op: BinOp,
    lhs: XPathVal,
    rhs: XPathVal,
    arena: &mut XmlArena<'_>,
    budget: &mut WorkBudget,
    ctx: &mut EvalContext<'_>,
) -> Result<bool, FormulaError> {
    match (lhs, rhs) {
        (XPathVal::NodeSet(l), XPathVal::NodeSet(r)) => {
            let Some((l_min, l_max)) = nodeset_min_max(&l, arena, budget, ctx)? else {
                return Ok(false);
            };
            let Some((r_min, r_max)) = nodeset_min_max(&r, arena, budget, ctx)? else {
                return Ok(false);
            };
            Ok(match op {
                BinOp::Lt => l_min < r_max,
                BinOp::Le => l_min <= r_max,
                BinOp::Gt => l_max > r_min,
                BinOp::Ge => l_max >= r_min,
                _ => false,
            })
        }
        (XPathVal::NodeSet(ns), other) => {
            let b = other.into_number(arena, budget, ctx)?;
            if b.is_nan() {
                return Ok(false);
            }
            for id in ns.iter() {
                let a = xpath_to_number(arena.string_value(id, budget, ctx)?);
                if cmp_f64_rel(op, a, b) {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        (other, XPathVal::NodeSet(ns)) => {
            let a = other.into_number(arena, budget, ctx)?;
            if a.is_nan() {
                return Ok(false);
            }
            for id in ns.iter() {
                let b = xpath_to_number(arena.string_value(id, budget, ctx)?);
                if cmp_f64_rel(op, a, b) {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        (l, r) => {
            let a = l.into_number(arena, budget, ctx)?;
            let b = r.into_number(arena, budget, ctx)?;
            Ok(cmp_f64_rel(op, a, b))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn eval_xexpr(
    expr: &XExpr,
    arena: &mut XmlArena<'_>,
    ctx_node: u32,
    ctx_pos: usize,
    ctx_size: usize,
    budget: &mut WorkBudget,
    ctx: &mut EvalContext<'_>,
    depth: usize,
) -> Result<XPathVal, FormulaError> {
    if depth > MAX_XPATH_DEPTH {
        return Err(FormulaError::Value);
    }
    budget.charge(1, ctx)?;
    match expr {
        XExpr::Literal(s) => Ok(XPathVal::String(s.clone())),
        XExpr::Number(n) => Ok(XPathVal::Number(*n)),
        XExpr::UnaryNeg(inner) => {
            let n = eval_xexpr(
                inner,
                arena,
                ctx_node,
                ctx_pos,
                ctx_size,
                budget,
                ctx,
                depth + 1,
            )?
            .into_number(arena, budget, ctx)?;
            Ok(XPathVal::Number(-n))
        }
        XExpr::LocPath { absolute, steps } => {
            let seed = if *absolute { 0 } else { ctx_node };
            let start = NodeSet::singleton(arena.node_count(), seed);
            Ok(XPathVal::NodeSet(eval_steps(
                start, steps, arena, budget, ctx, depth,
            )?))
        }
        XExpr::Filter {
            base,
            predicates,
            tail,
        } => {
            let XPathVal::NodeSet(ns) = eval_xexpr(
                base,
                arena,
                ctx_node,
                ctx_pos,
                ctx_size,
                budget,
                ctx,
                depth + 1,
            )?
            else {
                return Err(FormulaError::Value);
            };
            let list: Vec<u32> = ns.iter().collect();
            let kept = filter_candidates(list, predicates, arena, budget, ctx, depth)?;
            let mut filtered = NodeSet::empty(arena.node_count());
            for id in kept {
                filtered.insert(id);
            }
            if tail.is_empty() {
                Ok(XPathVal::NodeSet(filtered))
            } else {
                Ok(XPathVal::NodeSet(eval_steps(
                    filtered, tail, arena, budget, ctx, depth,
                )?))
            }
        }
        XExpr::Call { name, args } => core_fns::eval_core_fn(
            name, args, arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth,
        ),
        XExpr::Binary { op, left, right } => match op {
            BinOp::Or => {
                let lb = eval_xexpr(
                    left,
                    arena,
                    ctx_node,
                    ctx_pos,
                    ctx_size,
                    budget,
                    ctx,
                    depth + 1,
                )?
                .to_bool(arena, budget, ctx)?;
                if lb {
                    return Ok(XPathVal::Boolean(true));
                }
                let rb = eval_xexpr(
                    right,
                    arena,
                    ctx_node,
                    ctx_pos,
                    ctx_size,
                    budget,
                    ctx,
                    depth + 1,
                )?
                .to_bool(arena, budget, ctx)?;
                Ok(XPathVal::Boolean(rb))
            }
            BinOp::And => {
                let lb = eval_xexpr(
                    left,
                    arena,
                    ctx_node,
                    ctx_pos,
                    ctx_size,
                    budget,
                    ctx,
                    depth + 1,
                )?
                .to_bool(arena, budget, ctx)?;
                if !lb {
                    return Ok(XPathVal::Boolean(false));
                }
                let rb = eval_xexpr(
                    right,
                    arena,
                    ctx_node,
                    ctx_pos,
                    ctx_size,
                    budget,
                    ctx,
                    depth + 1,
                )?
                .to_bool(arena, budget, ctx)?;
                Ok(XPathVal::Boolean(rb))
            }
            BinOp::Union => {
                let XPathVal::NodeSet(mut l) = eval_xexpr(
                    left,
                    arena,
                    ctx_node,
                    ctx_pos,
                    ctx_size,
                    budget,
                    ctx,
                    depth + 1,
                )?
                else {
                    return Err(FormulaError::Value);
                };
                let XPathVal::NodeSet(r) = eval_xexpr(
                    right,
                    arena,
                    ctx_node,
                    ctx_pos,
                    ctx_size,
                    budget,
                    ctx,
                    depth + 1,
                )?
                else {
                    return Err(FormulaError::Value);
                };
                l.union_with(&r);
                Ok(XPathVal::NodeSet(l))
            }
            BinOp::Eq | BinOp::Ne => {
                let l = eval_xexpr(
                    left,
                    arena,
                    ctx_node,
                    ctx_pos,
                    ctx_size,
                    budget,
                    ctx,
                    depth + 1,
                )?;
                let r = eval_xexpr(
                    right,
                    arena,
                    ctx_node,
                    ctx_pos,
                    ctx_size,
                    budget,
                    ctx,
                    depth + 1,
                )?;
                Ok(XPathVal::Boolean(eval_eq(*op, l, r, arena, budget, ctx)?))
            }
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                let l = eval_xexpr(
                    left,
                    arena,
                    ctx_node,
                    ctx_pos,
                    ctx_size,
                    budget,
                    ctx,
                    depth + 1,
                )?;
                let r = eval_xexpr(
                    right,
                    arena,
                    ctx_node,
                    ctx_pos,
                    ctx_size,
                    budget,
                    ctx,
                    depth + 1,
                )?;
                Ok(XPathVal::Boolean(eval_rel(*op, l, r, arena, budget, ctx)?))
            }
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                let a = eval_xexpr(
                    left,
                    arena,
                    ctx_node,
                    ctx_pos,
                    ctx_size,
                    budget,
                    ctx,
                    depth + 1,
                )?
                .into_number(arena, budget, ctx)?;
                let b = eval_xexpr(
                    right,
                    arena,
                    ctx_node,
                    ctx_pos,
                    ctx_size,
                    budget,
                    ctx,
                    depth + 1,
                )?
                .into_number(arena, budget, ctx)?;
                Ok(XPathVal::Number(match op {
                    BinOp::Add => a + b,
                    BinOp::Sub => a - b,
                    BinOp::Mul => a * b,
                    BinOp::Div => a / b,
                    BinOp::Mod => a % b,
                    _ => unreachable!(),
                }))
            }
        },
    }
}

#[inline]
fn found_value(text: String) -> Result<V, FormulaError> {
    if text.chars().count() > MAX_CELL_TEXT_CHARS {
        return Err(FormulaError::Value);
    }
    Ok(match parse_strict_number(&text) {
        Some(n) => V::Number(n),
        None => V::Text(text),
    })
}

fn filter_xml(xml: &str, xpath: &str, ctx: &mut EvalContext<'_>) -> Result<Vec<V>, FormulaError> {
    if xml.len() > MAX_XML_BYTES
        || xml.trim().is_empty()
        || XmlSanitizer::validate_safe_xml(xml).is_err()
    {
        return Err(FormulaError::Value);
    }
    let ast = parse_xpath(xpath)?;
    let doc = roxmltree::Document::parse_with_options(
        xml,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: MAX_XML_NODES as u32,
            entity_resolver: None,
        },
    )
    .map_err(|_| FormulaError::Value)?;
    let mut arena = XmlArena::from_doc(&doc)?;
    validate_qname_prefixes(&ast, &arena)?;
    let mut budget = WorkBudget::new();
    let val = eval_xexpr(&ast, &mut arena, 0, 1, 1, &mut budget, ctx, 0)?;
    match val {
        XPathVal::NodeSet(ns) => {
            let mut out = Vec::with_capacity(ns.len());
            for id in ns.iter() {
                let s = arena.string_value(id, &mut budget, ctx)?.to_string();
                out.push(found_value(s)?);
            }
            Ok(out)
        }
        XPathVal::String(s) => Ok(vec![found_value(s)?]),
        XPathVal::Number(n) => {
            if !n.is_finite() {
                return Err(FormulaError::Value);
            }
            Ok(vec![V::Number(if n == 0.0 { 0.0 } else { n })])
        }
        XPathVal::Boolean(b) => Ok(vec![V::Boolean(b)]),
    }
}

impl SheetEngine {
    /// `FILTERXML(xml, xpath)`: evaluates an XPath 1.0 query over untrusted XML text and returns
    /// matching nodes as a vertical dynamic-array column or a scalar XPath result.
    pub(crate) fn fn_filterxml(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.len() != 2 || matches!(args[0], Expr::Missing) || matches!(args[1], Expr::Missing) {
            return Err(FormulaError::Value);
        }
        let xml = match self.arg_single(args, 0, None, env, ctx, depth)? {
            V::Error(e) => return Err(e),
            v => v.to_display_string(),
        };
        let xpath = match self.arg_single(args, 1, None, env, ctx, depth)? {
            V::Error(e) => return Err(e),
            v => v.to_display_string(),
        };
        let found = filter_xml(&xml, &xpath, ctx)?;
        if found.is_empty() {
            return Err(FormulaError::Value);
        }
        let rows = found.len();
        Ok(Val::grid(Grid::from_rows_checked(ctx, rows, 1, found)?))
    }

    /// Scalar entry point (`functions.rs`): evaluates `FILTERXML` and takes the upper-left value.
    pub(crate) fn fn_filterxml_scalar(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<V, FormulaError> {
        let grid = self
            .fn_filterxml(args, &Env::default(), ctx, depth)?
            .into_grid()?;
        Ok(grid.get(0, 0).clone())
    }
}

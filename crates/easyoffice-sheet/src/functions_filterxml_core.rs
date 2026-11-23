//! W3C XPath 1.0 core function library (27 functions) for `FILTERXML`.

use super::{
    eval_xexpr, xpath_round, NodeKind, NodeSet, WorkBudget, XPathVal, XmlArena, MAX_XML_BYTES,
};
use crate::xpath_ast::XExpr;
use crate::{EvalContext, FormulaError};

#[allow(clippy::too_many_arguments)]
fn eval_arg(
    expr: &XExpr,
    arena: &mut XmlArena<'_>,
    ctx_node: u32,
    ctx_pos: usize,
    ctx_size: usize,
    budget: &mut WorkBudget,
    ctx: &mut EvalContext<'_>,
    depth: usize,
) -> Result<XPathVal, FormulaError> {
    eval_xexpr(
        expr,
        arena,
        ctx_node,
        ctx_pos,
        ctx_size,
        budget,
        ctx,
        depth + 1,
    )
}

#[allow(clippy::too_many_arguments)]
fn first_node_from_optional_arg(
    name_check_arity: usize,
    args: &[XExpr],
    arena: &mut XmlArena<'_>,
    ctx_node: u32,
    ctx_pos: usize,
    ctx_size: usize,
    budget: &mut WorkBudget,
    ctx: &mut EvalContext<'_>,
    depth: usize,
) -> Result<Option<u32>, FormulaError> {
    if args.len() > name_check_arity {
        return Err(FormulaError::Value);
    }
    if args.is_empty() {
        Ok(Some(ctx_node))
    } else {
        match eval_arg(
            &args[0], arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth,
        )? {
            XPathVal::NodeSet(ns) => Ok(ns.first()),
            _ => Err(FormulaError::Value),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn optional_string_arg(
    args: &[XExpr],
    arena: &mut XmlArena<'_>,
    ctx_node: u32,
    ctx_pos: usize,
    ctx_size: usize,
    budget: &mut WorkBudget,
    ctx: &mut EvalContext<'_>,
    depth: usize,
) -> Result<String, FormulaError> {
    if args.len() > 1 {
        return Err(FormulaError::Value);
    }
    if args.is_empty() {
        Ok(arena.string_value(ctx_node, budget, ctx)?.to_string())
    } else {
        eval_arg(
            &args[0], arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth,
        )?
        .into_string(arena, budget, ctx)
    }
}

fn check_exact(args: &[XExpr], n: usize) -> Result<(), FormulaError> {
    if args.len() == n {
        Ok(())
    } else {
        Err(FormulaError::Value)
    }
}

fn push_id_keys(text: &str, keys: &mut Vec<String>) {
    for tok in text.split([' ', '\t', '\r', '\n']) {
        if !tok.is_empty() {
            keys.push(tok.to_string());
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn node_id_match(
    args: &[XExpr],
    arena: &mut XmlArena<'_>,
    ctx_node: u32,
    ctx_pos: usize,
    ctx_size: usize,
    budget: &mut WorkBudget,
    ctx: &mut EvalContext<'_>,
    depth: usize,
) -> Result<XPathVal, FormulaError> {
    check_exact(args, 1)?;
    let val = eval_arg(
        &args[0], arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth,
    )?;
    let mut keys = Vec::new();
    match val {
        XPathVal::NodeSet(ns) => {
            for id in ns.iter() {
                push_id_keys(arena.string_value(id, budget, ctx)?, &mut keys);
            }
        }
        other => {
            push_id_keys(&other.into_string(arena, budget, ctx)?, &mut keys);
        }
    }
    let mut out = NodeSet::empty(arena.node_count());
    if keys.is_empty() {
        return Ok(XPathVal::NodeSet(out));
    }
    for (id, node) in arena.nodes.iter().enumerate() {
        if node.kind == NodeKind::Element {
            for aid in node.attr_start..node.attr_end {
                budget.charge(1, ctx)?;
                let a = &arena.nodes[aid as usize];
                if a.prefix == "xml" && a.local == "id" && keys.iter().any(|k| k == a.leaf_text) {
                    out.insert(id as u32);
                    break;
                }
            }
        }
    }
    Ok(XPathVal::NodeSet(out))
}

fn fn_substring(s: &str, start: f64, len: Option<f64>) -> String {
    let r_start = xpath_round(start);
    let r_end = len.map(|l| r_start + xpath_round(l));
    if r_start.is_nan() || matches!(r_end, Some(e) if e.is_nan()) {
        return String::new();
    }
    let mut out = String::new();
    for (idx, ch) in s.chars().enumerate() {
        let p = (idx + 1) as f64;
        if p >= r_start && r_end.is_none_or(|e| p < e) {
            out.push(ch);
        }
    }
    out
}

fn fn_normalize_space(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for ch in s.chars() {
        if matches!(ch, ' ' | '\t' | '\r' | '\n') {
            if !out.is_empty() {
                in_ws = true;
            }
        } else {
            if in_ws {
                out.push(' ');
                in_ws = false;
            }
            out.push(ch);
        }
    }
    out
}

fn fn_translate(s: &str, from: &str, to: &str) -> String {
    let from_chars: Vec<char> = from.chars().collect();
    let to_chars: Vec<char> = to.chars().collect();
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match from_chars.iter().position(|&f| f == ch) {
            Some(pos) => {
                if let Some(&rep) = to_chars.get(pos) {
                    out.push(rep);
                }
            }
            None => out.push(ch),
        }
    }
    out
}

fn fn_lang(arena: &XmlArena<'_>, ctx_node: u32, target: &str) -> bool {
    let target_lc = target.to_ascii_lowercase();
    let mut cur = match arena.nodes[ctx_node as usize].kind {
        NodeKind::Element => Some(ctx_node),
        _ => arena.nodes[ctx_node as usize].parent,
    };
    while let Some(id) = cur {
        let n = &arena.nodes[id as usize];
        if n.kind == NodeKind::Element {
            for aid in n.attr_start..n.attr_end {
                let a = &arena.nodes[aid as usize];
                if a.prefix == "xml" && a.local == "lang" {
                    let val_lc = a.leaf_text.to_ascii_lowercase();
                    return val_lc == target_lc
                        || (val_lc.starts_with(&target_lc)
                            && val_lc.as_bytes().get(target_lc.len()) == Some(&b'-'));
                }
            }
        }
        cur = n.parent;
    }
    false
}

#[allow(clippy::too_many_arguments)]
pub(super) fn eval_core_fn(
    name: &str,
    args: &[XExpr],
    arena: &mut XmlArena<'_>,
    ctx_node: u32,
    ctx_pos: usize,
    ctx_size: usize,
    budget: &mut WorkBudget,
    ctx: &mut EvalContext<'_>,
    depth: usize,
) -> Result<XPathVal, FormulaError> {
    match name {
        "last" => {
            check_exact(args, 0)?;
            Ok(XPathVal::Number(ctx_size as f64))
        }
        "position" => {
            check_exact(args, 0)?;
            Ok(XPathVal::Number(ctx_pos as f64))
        }
        "count" => {
            check_exact(args, 1)?;
            let XPathVal::NodeSet(ns) = eval_arg(
                &args[0], arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth,
            )?
            else {
                return Err(FormulaError::Value);
            };
            Ok(XPathVal::Number(ns.len() as f64))
        }
        "id" => node_id_match(args, arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth),
        "local-name" => {
            let opt = first_node_from_optional_arg(
                1, args, arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth,
            )?;
            let text = opt
                .map(|id| match arena.nodes[id as usize].kind {
                    NodeKind::Element | NodeKind::Attribute | NodeKind::Pi => {
                        arena.nodes[id as usize].local
                    }
                    _ => "",
                })
                .unwrap_or("");
            Ok(XPathVal::String(text.to_string()))
        }
        "namespace-uri" => {
            let opt = first_node_from_optional_arg(
                1, args, arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth,
            )?;
            let uri = opt
                .and_then(|id| match arena.nodes[id as usize].kind {
                    NodeKind::Element | NodeKind::Attribute => arena.nodes[id as usize].ns_uri,
                    _ => None,
                })
                .unwrap_or("");
            Ok(XPathVal::String(uri.to_string()))
        }
        "name" => {
            let opt = first_node_from_optional_arg(
                1, args, arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth,
            )?;
            let qname = opt
                .map(|id| {
                    let n = &arena.nodes[id as usize];
                    match n.kind {
                        NodeKind::Element | NodeKind::Attribute => {
                            if n.prefix.is_empty() {
                                n.local.to_string()
                            } else {
                                format!("{}:{}", n.prefix, n.local)
                            }
                        }
                        NodeKind::Pi => n.local.to_string(),
                        _ => String::new(),
                    }
                })
                .unwrap_or_default();
            Ok(XPathVal::String(qname))
        }
        "string" => Ok(XPathVal::String(optional_string_arg(
            args, arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth,
        )?)),
        "concat" => {
            if args.len() < 2 {
                return Err(FormulaError::Value);
            }
            let mut out = String::new();
            for a in args {
                let s = eval_arg(a, arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth)?
                    .into_string(arena, budget, ctx)?;
                if out.len().saturating_add(s.len()) > MAX_XML_BYTES {
                    return Err(FormulaError::Value);
                }
                out.push_str(&s);
            }
            Ok(XPathVal::String(out))
        }
        "starts-with" | "contains" => {
            check_exact(args, 2)?;
            let a = eval_arg(
                &args[0], arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth,
            )?
            .into_string(arena, budget, ctx)?;
            let b = eval_arg(
                &args[1], arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth,
            )?
            .into_string(arena, budget, ctx)?;
            Ok(XPathVal::Boolean(if name == "starts-with" {
                a.starts_with(&b)
            } else {
                a.contains(&b)
            }))
        }
        "substring-before" | "substring-after" => {
            check_exact(args, 2)?;
            let a = eval_arg(
                &args[0], arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth,
            )?
            .into_string(arena, budget, ctx)?;
            let b = eval_arg(
                &args[1], arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth,
            )?
            .into_string(arena, budget, ctx)?;
            let out = if b.is_empty() {
                if name == "substring-after" {
                    a
                } else {
                    String::new()
                }
            } else if let Some(pos) = a.find(&b) {
                if name == "substring-before" {
                    a[..pos].to_string()
                } else {
                    a[pos + b.len()..].to_string()
                }
            } else {
                String::new()
            };
            Ok(XPathVal::String(out))
        }
        "substring" => {
            if !(2..=3).contains(&args.len()) {
                return Err(FormulaError::Value);
            }
            let s = eval_arg(
                &args[0], arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth,
            )?
            .into_string(arena, budget, ctx)?;
            let start = eval_arg(
                &args[1], arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth,
            )?
            .into_number(arena, budget, ctx)?;
            let len = if args.len() == 3 {
                Some(
                    eval_arg(
                        &args[2], arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth,
                    )?
                    .into_number(arena, budget, ctx)?,
                )
            } else {
                None
            };
            Ok(XPathVal::String(fn_substring(&s, start, len)))
        }
        "string-length" => {
            let s =
                optional_string_arg(args, arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth)?;
            Ok(XPathVal::Number(s.chars().count() as f64))
        }
        "normalize-space" => {
            let s =
                optional_string_arg(args, arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth)?;
            Ok(XPathVal::String(fn_normalize_space(&s)))
        }
        "translate" => {
            check_exact(args, 3)?;
            let s = eval_arg(
                &args[0], arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth,
            )?
            .into_string(arena, budget, ctx)?;
            let from = eval_arg(
                &args[1], arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth,
            )?
            .into_string(arena, budget, ctx)?;
            let to = eval_arg(
                &args[2], arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth,
            )?
            .into_string(arena, budget, ctx)?;
            budget.charge(s.len(), ctx)?;
            Ok(XPathVal::String(fn_translate(&s, &from, &to)))
        }
        "boolean" => {
            check_exact(args, 1)?;
            let v = eval_arg(
                &args[0], arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth,
            )?;
            Ok(XPathVal::Boolean(v.to_bool(arena, budget, ctx)?))
        }
        "not" => {
            check_exact(args, 1)?;
            let v = eval_arg(
                &args[0], arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth,
            )?;
            Ok(XPathVal::Boolean(!v.to_bool(arena, budget, ctx)?))
        }
        "true" => {
            check_exact(args, 0)?;
            Ok(XPathVal::Boolean(true))
        }
        "false" => {
            check_exact(args, 0)?;
            Ok(XPathVal::Boolean(false))
        }
        "lang" => {
            check_exact(args, 1)?;
            let s = eval_arg(
                &args[0], arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth,
            )?
            .into_string(arena, budget, ctx)?;
            Ok(XPathVal::Boolean(fn_lang(arena, ctx_node, &s)))
        }
        "number" => {
            let val = if args.is_empty() {
                XPathVal::String(arena.string_value(ctx_node, budget, ctx)?.to_string())
            } else if args.len() == 1 {
                eval_arg(
                    &args[0], arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth,
                )?
            } else {
                return Err(FormulaError::Value);
            };
            Ok(XPathVal::Number(val.into_number(arena, budget, ctx)?))
        }
        "sum" => {
            check_exact(args, 1)?;
            let XPathVal::NodeSet(ns) = eval_arg(
                &args[0], arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth,
            )?
            else {
                return Err(FormulaError::Value);
            };
            let mut total = 0.0f64;
            for id in ns.iter() {
                let s = arena.string_value(id, budget, ctx)?;
                total += super::xpath_to_number(s);
            }
            Ok(XPathVal::Number(total))
        }
        "floor" | "ceiling" | "round" => {
            check_exact(args, 1)?;
            let n = eval_arg(
                &args[0], arena, ctx_node, ctx_pos, ctx_size, budget, ctx, depth,
            )?
            .into_number(arena, budget, ctx)?;
            Ok(XPathVal::Number(match name {
                "floor" => n.floor(),
                "ceiling" => n.ceil(),
                _ => xpath_round(n),
            }))
        }
        _ => Err(FormulaError::Value),
    }
}

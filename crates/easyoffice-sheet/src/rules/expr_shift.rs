//! Precompiled conditional formatting formula templates with AST offset shifting (Q5).
//!
//! Instead of calling [`super::shift::shift_formula`] and re-lexing/re-parsing formula text for
//! every cell of a conditional formatting range, `CfKind::Expression` parses and expands defined
//! names once at anchor `(ar, ac)` while tracking AST paths to non-`$` relative references, then
//! shifts only those integer offsets per cell.

use super::data::Cells;
use crate::parser::{parse_formula_with_refs, RefEndpointSpec, RefSpec};
use crate::table_index::TableCtx;
use crate::{CellRange, Expr, FormulaError, PackedCellCoordinate, SheetEngine};
use std::sync::atomic::Ordering::Relaxed;

#[derive(Clone, Copy, Debug)]
enum ChildStep {
    Unary,
    Left,
    Right,
    Arg(usize),
    Callee,
}

#[derive(Clone, Debug)]
enum PatchKind {
    Cell(RefEndpointSpec),
    Range {
        start: RefEndpointSpec,
        end: RefEndpointSpec,
    },
}

#[derive(Clone, Debug)]
struct Patch {
    path: Vec<ChildStep>,
    kind: PatchKind,
}

#[derive(Clone, Copy, Debug)]
enum ShiftedRef {
    Cell(PackedCellCoordinate),
    Range(CellRange),
}

#[derive(Clone, Debug)]
enum InnerTemplate {
    Empty,
    Invalid,
    Fixed {
        expr: Expr,
        needs_array: bool,
    },
    Shifted {
        base: Expr,
        patches: Vec<Patch>,
        needs_array: bool,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct RuleExprTemplate(InnerTemplate);

#[inline]
fn pack_unchecked(row: u32, col: u32) -> PackedCellCoordinate {
    PackedCellCoordinate((u64::from(row) << 32) | u64::from(col))
}

#[inline]
fn shift_endpoint(spec: RefEndpointSpec, dr: i64, dc: i64) -> Option<PackedCellCoordinate> {
    let row = if spec.shift_row {
        i64::from(spec.coord.row()).checked_add(dr)?
    } else {
        i64::from(spec.coord.row())
    };
    let col = if spec.shift_col {
        i64::from(spec.coord.col()).checked_add(dc)?
    } else {
        i64::from(spec.coord.col())
    };
    if !(0..i64::from(PackedCellCoordinate::MAX_ROWS)).contains(&row)
        || !(0..i64::from(PackedCellCoordinate::MAX_COLS)).contains(&col)
    {
        return None;
    }
    Some(pack_unchecked(row as u32, col as u32))
}

fn collect_patches(
    expr: &Expr,
    specs: &[RefSpec],
    spec_idx: &mut usize,
    path: &mut Vec<ChildStep>,
    patches: &mut Vec<Patch>,
) -> bool {
    match expr {
        Expr::Cell(_)
        | Expr::Range(_)
        | Expr::SheetCell { .. }
        | Expr::SheetRange { .. }
        | Expr::SpillRef { .. } => {
            let Some(spec) = specs.get(*spec_idx).copied() else {
                return false;
            };
            *spec_idx += 1;
            match spec {
                RefSpec::Fixed => {}
                RefSpec::Cell(ep) => {
                    if ep.shift_row || ep.shift_col {
                        patches.push(Patch {
                            path: path.clone(),
                            kind: PatchKind::Cell(ep),
                        });
                    }
                }
                RefSpec::Range { start, end } => {
                    patches.push(Patch {
                        path: path.clone(),
                        kind: PatchKind::Range { start, end },
                    });
                }
            }
            true
        }
        Expr::UnaryPlus(inner) | Expr::UnaryMinus(inner) | Expr::PostfixPercent(inner) => {
            path.push(ChildStep::Unary);
            let ok = collect_patches(inner, specs, spec_idx, path, patches);
            path.pop();
            ok
        }
        Expr::Binary { lhs, rhs, .. } => {
            path.push(ChildStep::Left);
            let ok_l = collect_patches(lhs, specs, spec_idx, path, patches);
            path.pop();
            if !ok_l {
                return false;
            }
            path.push(ChildStep::Right);
            let ok_r = collect_patches(rhs, specs, spec_idx, path, patches);
            path.pop();
            ok_r
        }
        Expr::FuncCall { args, .. } => {
            for (idx, arg) in args.iter().enumerate() {
                path.push(ChildStep::Arg(idx));
                let ok = collect_patches(arg, specs, spec_idx, path, patches);
                path.pop();
                if !ok {
                    return false;
                }
            }
            true
        }
        Expr::Call { callee, args } => {
            path.push(ChildStep::Callee);
            let ok_c = collect_patches(callee, specs, spec_idx, path, patches);
            path.pop();
            if !ok_c {
                return false;
            }
            for (idx, arg) in args.iter().enumerate() {
                path.push(ChildStep::Arg(idx));
                let ok = collect_patches(arg, specs, spec_idx, path, patches);
                path.pop();
                if !ok {
                    return false;
                }
            }
            true
        }
        Expr::Number(_)
        | Expr::Text(_)
        | Expr::Boolean(_)
        | Expr::ErrorLiteral(_)
        | Expr::Missing
        | Expr::Name { .. }
        | Expr::Var(_)
        | Expr::Array(_)
        | Expr::Shared(_) => true,
    }
}

fn node_at_path_mut<'a>(mut expr: &'a mut Expr, path: &[ChildStep]) -> Option<&'a mut Expr> {
    for &step in path {
        expr = match (expr, step) {
            (
                Expr::UnaryPlus(inner) | Expr::UnaryMinus(inner) | Expr::PostfixPercent(inner),
                ChildStep::Unary,
            ) => inner,
            (Expr::Binary { lhs, .. }, ChildStep::Left) => lhs,
            (Expr::Binary { rhs, .. }, ChildStep::Right) => rhs,
            (Expr::FuncCall { args, .. } | Expr::Call { args, .. }, ChildStep::Arg(idx)) => {
                args.get_mut(idx)?
            }
            (Expr::Call { callee, .. }, ChildStep::Callee) => callee,
            _ => return None,
        };
    }
    Some(expr)
}

#[inline]
fn apply_shift_ref(node: &mut Expr, shifted: ShiftedRef) -> bool {
    match (node, shifted) {
        (
            Expr::Cell(c) | Expr::SheetCell { coord: c, .. } | Expr::SpillRef { coord: c, .. },
            ShiftedRef::Cell(new_c),
        ) => {
            *c = new_c;
            true
        }
        (Expr::Range(r) | Expr::SheetRange { range: r, .. }, ShiftedRef::Range(new_r)) => {
            *r = new_r;
            true
        }
        _ => false,
    }
}

fn compute_shifted(patches: &[Patch], dr: i64, dc: i64, out: &mut Vec<ShiftedRef>) -> bool {
    out.clear();
    out.reserve(patches.len());
    for patch in patches {
        let value = match patch.kind {
            PatchKind::Cell(ep) => match shift_endpoint(ep, dr, dc) {
                Some(c) => ShiftedRef::Cell(c),
                None => return false,
            },
            PatchKind::Range { start, end } => {
                let (Some(c0), Some(c1)) =
                    (shift_endpoint(start, dr, dc), shift_endpoint(end, dr, dc))
                else {
                    return false;
                };
                ShiftedRef::Range(CellRange { start: c0, end: c1 })
            }
        };
        out.push(value);
    }
    true
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RuleEvalScratch {
    expr: Option<Expr>,
    refs: Vec<ShiftedRef>,
}

impl RuleExprTemplate {
    /// Parses and compiles a conditional formatting expression once for the anchor cell.
    pub(crate) fn compile(sheet: &SheetEngine, formula: &str) -> Self {
        let expr_src = formula.trim();
        if expr_src.is_empty() {
            return Self(InnerTemplate::Empty);
        }
        let table_ctx = TableCtx {
            index: &sheet.tables,
            sheet: &sheet.name,
            coord: None,
        };
        let (mut parsed, specs) = match parse_formula_with_refs(expr_src, Some(&table_ctx)) {
            Ok(res) => res,
            Err(_) => return Self(InnerTemplate::Invalid),
        };

        let mut patches = Vec::new();
        let mut path = Vec::new();
        let mut spec_idx = 0;
        let ok = collect_patches(&parsed.expr, &specs, &mut spec_idx, &mut path, &mut patches);
        if !ok || spec_idx != specs.len() {
            return Self(InnerTemplate::Invalid);
        }

        if parsed.saw_name {
            let mut keys = parsed.table_keys;
            let held = match sheet.names.expand(
                &mut parsed.expr,
                &sheet.name,
                &sheet.tables,
                None,
                &mut keys,
            ) {
                Ok((_, held)) => held,
                Err(_) => return Self(InnerTemplate::Invalid),
            };
            let in_use = sheet.name_budget.load(Relaxed);
            if in_use.saturating_add(held) > crate::names::MAX_WORKBOOK_NAME_NODES {
                return Self(InnerTemplate::Invalid);
            }
        }

        let needs_array = crate::array_class::needs_array(&parsed.expr);
        if patches.is_empty() {
            Self(InnerTemplate::Fixed {
                expr: parsed.expr,
                needs_array,
            })
        } else {
            Self(InnerTemplate::Shifted {
                base: parsed.expr,
                patches,
                needs_array,
            })
        }
    }

    /// Shifts all relative AST references by `(dr, dc)` without mutating `base` or re-parsing text.
    /// Returns `None` when any shifted endpoint leaves the sheet bounds.
    #[cfg(test)]
    pub(crate) fn shift_expr_offset(&self, dr: i64, dc: i64) -> Option<Expr> {
        let InnerTemplate::Shifted { base, patches, .. } = &self.0 else {
            return None;
        };
        if dr == 0 && dc == 0 {
            return Some(base.clone());
        }
        let mut scratch = base.clone();
        let mut buf = Vec::with_capacity(patches.len());
        self.apply_offset_in_place(&mut scratch, dr, dc, &mut buf)
            .then_some(scratch)
    }

    fn apply_offset_in_place(
        &self,
        dst: &mut Expr,
        dr: i64,
        dc: i64,
        buf: &mut Vec<ShiftedRef>,
    ) -> bool {
        let InnerTemplate::Shifted { patches, .. } = &self.0 else {
            return false;
        };
        if !compute_shifted(patches, dr, dc, buf) {
            return false;
        }
        for (patch, &value) in patches.iter().zip(buf.iter()) {
            let Some(node) = node_at_path_mut(dst, &patch.path) else {
                return false;
            };
            if !apply_shift_ref(node, value) {
                return false;
            }
        }
        true
    }

    /// Creates a reusable working AST buffer for [`Self::eval_in_place`].
    pub(crate) fn init_scratch(&self) -> RuleEvalScratch {
        match &self.0 {
            InnerTemplate::Shifted { base, patches, .. } => RuleEvalScratch {
                expr: Some(base.clone()),
                refs: Vec::with_capacity(patches.len()),
            },
            InnerTemplate::Empty | InnerTemplate::Invalid | InnerTemplate::Fixed { .. } => {
                RuleEvalScratch::default()
            }
        }
    }

    /// Evaluates this rule expression at cell offset `(dr, dc)` from the anchor cell, reusing
    /// `scratch` so shifted evaluations avoid per-cell AST cloning.
    pub(crate) fn eval_in_place(
        &self,
        cells: &mut dyn Cells,
        scratch: &mut RuleEvalScratch,
        dr: i64,
        dc: i64,
    ) -> bool {
        match &self.0 {
            InnerTemplate::Empty | InnerTemplate::Invalid => false,
            InnerTemplate::Fixed { expr, needs_array } => {
                let v = cells.eval_compiled_expr(expr, *needs_array);
                super::truthy(&v)
            }
            InnerTemplate::Shifted {
                base, needs_array, ..
            } => {
                if dr == 0 && dc == 0 {
                    let v = cells.eval_compiled_expr(base, *needs_array);
                    return super::truthy(&v);
                }
                let dst = scratch.expr.get_or_insert_with(|| base.clone());
                if !self.apply_offset_in_place(dst, dr, dc, &mut scratch.refs) {
                    return false;
                }
                let v = cells.eval_compiled_expr(dst, *needs_array);
                super::truthy(&v)
            }
        }
    }
}

/// Evaluates an already-compiled AST expression with the same sanitization rules as
/// [`SheetEngine::eval_raw_string_ctx`].
pub(crate) fn eval_compiled_sanitized(
    sheet: &SheetEngine,
    expr: &Expr,
    needs_array: bool,
    ctx: &mut crate::EvalContext<'_>,
) -> crate::EvaluatedCellValue {
    let mut res = if needs_array {
        sheet.eval_array_formula(expr, None, ctx, 0)
    } else {
        sheet.eval_expr_ctx(expr, ctx, 0)
    };
    res = crate::coercion::sanitize_evaluated_value(res);
    if let crate::EvaluatedCellValue::Text(ref s) = res {
        if s.len() > SheetEngine::MAX_CELL_TEXT_CHARS
            && s.chars().count() > SheetEngine::MAX_CELL_TEXT_CHARS
        {
            res = crate::EvaluatedCellValue::Error(FormulaError::Value);
        }
    }
    res
}

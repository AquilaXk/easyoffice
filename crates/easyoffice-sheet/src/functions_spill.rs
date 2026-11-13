//! `FILTER`, `SORT`, `SORTBY` and `UNIQUE` over arrays (PD-217).
//!
//! 참조 구현: IronCalc `base/src/functions/spill_functions.rs` (기준 커밋
//! 6dc9e8bae4b8d066276f399de7b3e1cf991c220b; `fn_sort` 192-303, `fn_sortby` 305-420, `fn_unique`
//! 422-530, `fn_filter` 532-656, 정렬 키 비교 19-44, 같음 판정 키 60-117). MIT OR Apache-2.0,
//! Copyright 2023 EqualTo GmbH, 2023-present IronCalc contributors, 2026-present IronCalc GmbH.
//! The bodies are ported onto this engine's grid and error types. Where the reference differs from
//! the spreadsheet program's documented behavior this follows the program (see PD-217): `FILTER`'s
//! include array must be as long as the rows (or the columns) of the array, errors sort after
//! booleans, and a text in the include array is `#VALUE!`.

use crate::array::{Env, Grid, Val};
use crate::array_eval::{single, ValResult};
use crate::{EvalContext, EvaluatedCellValue as V, Expr, FormulaError, SheetEngine};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

/// Order of the kinds of value in a sort: numbers, texts, booleans, errors; blanks follow them all
/// in either direction.
fn kind_rank(v: &V) -> u8 {
    match v {
        V::Number(_) => 0,
        V::Text(_) => 1,
        V::Boolean(_) => 2,
        V::Error(_) => 3,
        V::Empty => 4,
    }
}

/// Compares two sort keys; blanks are last whatever the direction.
fn sort_key_cmp(a: &V, b: &V, ascending: bool) -> Ordering {
    let (ra, rb) = (kind_rank(a), kind_rank(b));
    if ra == 4 || rb == 4 {
        return ra.cmp(&rb);
    }
    let ord = if ra != rb {
        ra.cmp(&rb)
    } else {
        match (a, b) {
            (V::Error(x), V::Error(y)) => x.code().cmp(y.code()),
            _ => SheetEngine::compare_excel_values(a, b).unwrap_or(Ordering::Equal),
        }
    };
    if ascending {
        ord
    } else {
        ord.reverse()
    }
}

/// A value as a hash key: texts without case, numbers by their bits (zero without sign).
#[derive(Hash, PartialEq, Eq)]
enum CellKey {
    Number(u64),
    Boolean(bool),
    Text(String),
    Error(&'static str),
    Empty,
}

fn cell_key(v: &V) -> CellKey {
    match v {
        V::Number(n) => CellKey::Number(if *n == 0.0 { 0 } else { n.to_bits() }),
        V::Boolean(b) => CellKey::Boolean(*b),
        V::Text(s) => CellKey::Text(s.to_lowercase()),
        V::Error(e) => CellKey::Error(e.code()),
        V::Empty => CellKey::Empty,
    }
}

fn row_key(grid: &Grid, r: usize) -> Vec<CellKey> {
    (0..grid.cols).map(|c| cell_key(grid.get(r, c))).collect()
}

fn col_key(grid: &Grid, c: usize) -> Vec<CellKey> {
    (0..grid.rows).map(|r| cell_key(grid.get(r, c))).collect()
}

/// `1` or `-1`; anything else is `#VALUE!`.
fn sort_order(v: V) -> Result<bool, FormulaError> {
    match v.as_number() {
        Some(1.0) => Ok(true),
        Some(-1.0) => Ok(false),
        _ => Err(FormulaError::Value),
    }
}

fn truthy(v: &V) -> bool {
    match v {
        V::Boolean(b) => *b,
        V::Number(n) => *n != 0.0,
        _ => false,
    }
}

impl SheetEngine {
    /// Argument `i` as a single value (`default` when it is absent or left empty).
    pub(crate) fn arg_single(
        &self,
        args: &[Expr],
        i: usize,
        default: Option<V>,
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<V, FormulaError> {
        match args.get(i) {
            Some(Expr::Missing) | None => default.ok_or(FormulaError::Value),
            Some(arg) => single(self.eval_val(arg, env, ctx, depth + 1)?),
        }
    }

    pub(crate) fn arg_number(
        &self,
        args: &[Expr],
        i: usize,
        default: f64,
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<f64, FormulaError> {
        let v = self.arg_single(args, i, Some(V::Number(default)), env, ctx, depth)?;
        match v {
            V::Empty => Ok(default),
            other => other.as_number().ok_or(FormulaError::Value),
        }
    }

    pub(crate) fn arg_bool(
        &self,
        args: &[Expr],
        i: usize,
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<bool, FormulaError> {
        match self.arg_single(args, i, Some(V::Boolean(false)), env, ctx, depth)? {
            V::Empty => Ok(false),
            other => other.as_bool().ok_or(FormulaError::Value),
        }
    }

    fn arg_grid(
        &self,
        args: &[Expr],
        i: usize,
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<std::sync::Arc<Grid>, FormulaError> {
        let grid = self.eval_val(&args[i], env, ctx, depth + 1)?.into_grid()?;
        match grid.data.as_slice() {
            [V::Error(e)] => Err(*e),
            _ => Ok(grid),
        }
    }

    /// `SORT(array, [sort_index], [sort_order], [by_col])`.
    pub(crate) fn fn_sort(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.is_empty() || args.len() > 4 {
            return Err(FormulaError::Value);
        }
        let data = self.arg_grid(args, 0, env, ctx, depth)?;
        let index = self.arg_number(args, 1, 1.0, env, ctx, depth)?.trunc();
        let ascending = match args.get(2) {
            Some(Expr::Missing) | None => true,
            Some(_) => sort_order(self.arg_single(args, 2, None, env, ctx, depth)?)?,
        };
        let by_col = self.arg_bool(args, 3, env, ctx, depth)?;
        let (lines, width) = if by_col {
            (data.cols, data.rows)
        } else {
            (data.rows, data.cols)
        };
        if index < 1.0 || index > width as f64 {
            return Err(FormulaError::Value);
        }
        let key = index as usize - 1;
        let key_at = |line: usize| {
            if by_col {
                data.get(key, line)
            } else {
                data.get(line, key)
            }
        };
        let mut order: Vec<usize> = (0..lines).collect();
        order.sort_by(|&a, &b| sort_key_cmp(key_at(a), key_at(b), ascending));
        Ok(Val::grid(if by_col {
            data.take_cols(&order)
        } else {
            data.take_rows(&order)
        }))
    }

    /// `SORTBY(array, by_array1, [sort_order1], [by_array2, sort_order2], ...)`.
    pub(crate) fn fn_sortby(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        let n = args.len();
        if n < 2 || (n > 3 && n % 2 == 0) {
            return Err(FormulaError::Value);
        }
        let data = self.arg_grid(args, 0, env, ctx, depth)?;
        let mut keys: Vec<(std::sync::Arc<Grid>, bool)> = Vec::new();
        let mut by_col = false;
        let mut i = 1;
        while i < n {
            let key = self.arg_grid(args, i, env, ctx, depth)?;
            let ascending = match args.get(i + 1) {
                Some(Expr::Missing) | None => true,
                Some(_) => sort_order(self.arg_single(args, i + 1, None, env, ctx, depth)?)?,
            };
            if keys.is_empty() {
                by_col = !(key.cols == 1 && key.rows == data.rows);
            }
            let fits = if by_col {
                key.rows == 1 && key.cols == data.cols
            } else {
                key.cols == 1 && key.rows == data.rows
            };
            if !fits {
                return Err(FormulaError::Value);
            }
            keys.push((key, ascending));
            i += 2;
        }
        let lines = if by_col { data.cols } else { data.rows };
        let mut order: Vec<usize> = (0..lines).collect();
        order.sort_by(|&a, &b| {
            for (key, ascending) in &keys {
                let (x, y) = if by_col {
                    (key.get(0, a), key.get(0, b))
                } else {
                    (key.get(a, 0), key.get(b, 0))
                };
                let ord = sort_key_cmp(x, y, *ascending);
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            Ordering::Equal
        });
        Ok(Val::grid(if by_col {
            data.take_cols(&order)
        } else {
            data.take_rows(&order)
        }))
    }

    /// `UNIQUE(array, [by_col], [exactly_once])`.
    pub(crate) fn fn_unique(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.is_empty() || args.len() > 3 {
            return Err(FormulaError::Value);
        }
        let data = self.arg_grid(args, 0, env, ctx, depth)?;
        let by_col = self.arg_bool(args, 1, env, ctx, depth)?;
        let exactly_once = self.arg_bool(args, 2, env, ctx, depth)?;
        let lines = if by_col { data.cols } else { data.rows };
        let key_of = |i: usize| {
            if by_col {
                col_key(&data, i)
            } else {
                row_key(&data, i)
            }
        };
        let keys: Vec<Vec<CellKey>> = (0..lines).map(key_of).collect();
        let keep: Vec<usize> = if exactly_once {
            let mut counts: HashMap<&Vec<CellKey>, usize> = HashMap::with_capacity(lines);
            for key in &keys {
                *counts.entry(key).or_insert(0) += 1;
            }
            (0..lines).filter(|&i| counts[&keys[i]] == 1).collect()
        } else {
            let mut seen: HashSet<&Vec<CellKey>> = HashSet::with_capacity(lines);
            (0..lines).filter(|&i| seen.insert(&keys[i])).collect()
        };
        if keep.is_empty() {
            return Err(FormulaError::Calc);
        }
        Ok(Val::grid(if by_col {
            data.take_cols(&keep)
        } else {
            data.take_rows(&keep)
        }))
    }

    /// `FILTER(array, include, [if_empty])`.
    pub(crate) fn fn_filter(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        if args.len() < 2 || args.len() > 3 {
            return Err(FormulaError::Value);
        }
        let data = self.arg_grid(args, 0, env, ctx, depth)?;
        let include = self.eval_val(&args[1], env, ctx, depth + 1)?.into_grid()?;
        for v in &include.data {
            match v {
                V::Text(_) => return Err(FormulaError::Value),
                V::Error(e) => return Err(*e),
                _ => {}
            }
        }
        let by_rows = include.cols == 1 && include.rows == data.rows;
        let by_cols = include.rows == 1 && include.cols == data.cols;
        if !by_rows && !by_cols {
            return Err(FormulaError::Value);
        }
        let keep: Vec<usize> = include
            .data
            .iter()
            .enumerate()
            .filter(|(_, v)| truthy(v))
            .map(|(i, _)| i)
            .collect();
        if keep.is_empty() {
            return match args.get(2) {
                None => Err(FormulaError::Calc),
                Some(Expr::Missing) => Err(FormulaError::Calc),
                Some(fallback) => self.eval_val(fallback, env, ctx, depth + 1),
            };
        }
        Ok(Val::grid(if by_rows {
            data.take_rows(&keep)
        } else {
            data.take_cols(&keep)
        }))
    }
}

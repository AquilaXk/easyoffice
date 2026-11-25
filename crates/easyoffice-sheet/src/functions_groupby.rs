//! `GROUPBY` and `PIVOTBY`: values summed up by the distinct values of group columns.
//!
//! 참조 구현: none. The reference engines list the names (one of them as a stub) and have no body,
//! so this follows the function documentation: the arguments `field_headers` (missing is detected:
//! a text first value over a number second value; 0 none, 1 present but not shown, 2 absent but
//! shown, 3 present and shown), `total_depth` (missing the grand total and every subtotal, 0 none,
//! 1 the grand total, 2 and more the subtotals of that many levels less one, a negative number
//! puts the totals first), `sort_order` (a column number among the group columns and then the value
//! columns, negative for descending, a list of them when sorting by group columns), `filter_array`
//! (a column of `TRUE` and `FALSE` as long as the group columns), `field_relationship` (0
//! hierarchy, 1 table with no subtotals) and `relative_to` (0 column total, 1 row total, 2 grand
//! total, 3 parent column total, 4 parent row total, for a function with two arguments such as
//! `PERCENTOF`). A function is an eta-reduced name (`SUM`, `AVERAGE`, ...), a `LAMBDA`, or a
//! `HSTACK`/`VSTACK` vector of functions (optionally paired with custom header labels, see
//! `groupby_funcs.rs`).
//!
//! The pass over the rows is single: each row is put in the cell of every shown row group and
//! column group it belongs to, so the cost is the rows times the levels of the two axes and the
//! number of cells is checked against the array budget before anything is allocated. The
//! documentation shows the output only as pictures, so the generated header names (`Group`,
//! `Column`, `Value`, `Function`), the layout of the field header row and the labels of the total
//! rows are this engine's choice (`groupby_axis.rs`). Each function runs once per cell and value
//! column, sharing grouped inputs and cached totals across multi-function vectors.

use crate::array::{Env, Grid, Val};
use crate::array_eval::ValResult;
use crate::groupby_axis::{intersect, Entry, Kind, Node, SortKey, Totals, Tree};
use crate::groupby_funcs::{
    parse_func_plan, AggFunc, FuncOrientation, FuncPlan, OutputShape, OutputSpec, MAX_VALUE_COLUMNS,
};
use crate::{EvalContext, EvaluatedCellValue as V, Expr, FormulaError, SheetEngine};
use std::collections::hash_map::Entry as MapEntry;
use std::collections::HashMap;
use std::sync::Arc;

/// Group columns on one axis.
const MAX_GROUP_FIELDS: usize = 16;
/// Rows put into cells, over all rows: the rows times the levels of both axes.
const MAX_BUCKET_PUSHES: usize = 8_000_000;
const NO_ENTRY: usize = usize::MAX;
/// Bytes one group costs before it exists: the group, its row list entries and its index entry.
const GROUP_NODE_BYTES: usize = 160;
/// Values a function reads for one evaluation step.
const STEP_VALUES: usize = 16;

/// The positions of the arguments of `GROUPBY` and of `PIVOTBY`.
struct Layout {
    rows: usize,
    cols: Option<usize>,
    values: usize,
    function: usize,
    headers: usize,
    row_totals: usize,
    row_sort: usize,
    col_totals: Option<usize>,
    col_sort: Option<usize>,
    filter: usize,
    /// `field_relationship` of `GROUPBY`, `relative_to` of `PIVOTBY`.
    last: usize,
    max_args: usize,
}

static GROUPBY: Layout = Layout {
    rows: 0,
    cols: None,
    values: 1,
    function: 2,
    headers: 3,
    row_totals: 4,
    row_sort: 5,
    col_totals: None,
    col_sort: None,
    filter: 6,
    last: 7,
    max_args: 8,
};

static PIVOTBY: Layout = Layout {
    rows: 0,
    cols: Some(1),
    values: 2,
    function: 3,
    headers: 4,
    row_totals: 5,
    row_sort: 6,
    col_totals: Some(7),
    col_sort: Some(8),
    filter: 9,
    last: 10,
    max_args: 11,
};

/// The one value a function gave for a group; an array is `#CALC!`.
fn single(val: Val) -> V {
    match val {
        Val::Grid(g) if g.is_scalar() => g.top_left(),
        _ => V::Error(FormulaError::Calc),
    }
}

fn present(args: &[Expr], i: usize) -> Option<&Expr> {
    args.get(i).filter(|a| !matches!(a, Expr::Missing))
}

impl SheetEngine {
    /// `GROUPBY(row_fields, values, function, [field_headers], [total_depth], [sort_order],
    /// [filter_array], [field_relationship])`
    pub(crate) fn fn_groupby(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        self.group_rows(&GROUPBY, 3, args, env, ctx, depth)
    }

    /// `PIVOTBY(row_fields, col_fields, values, function, [field_headers], [row_total_depth],
    /// [row_sort_order], [col_total_depth], [col_sort_order], [filter_array], [relative_to])`
    pub(crate) fn fn_pivotby(
        &self,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        self.group_rows(&PIVOTBY, 4, args, env, ctx, depth)
    }

    fn grid_arg(
        &self,
        arg: &Expr,
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Arc<Grid>, FormulaError> {
        let grid = self.eval_val(arg, env, ctx, depth + 1)?.into_grid()?;
        match grid.data.as_slice() {
            [V::Error(e)] => Err(*e),
            _ => Ok(grid),
        }
    }

    fn whole_number_arg(
        &self,
        args: &[Expr],
        i: usize,
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Option<i64>, FormulaError> {
        let Some(arg) = present(args, i) else {
            return Ok(None);
        };
        let grid = self.grid_arg(arg, env, ctx, depth)?;
        match grid.data.as_slice() {
            [V::Empty] => Ok(Some(0)),
            [v] => v
                .as_number()
                .map(|n| Some(n.trunc() as i64))
                .ok_or(FormulaError::Value),
            _ => Err(FormulaError::Value),
        }
    }

    /// A `sort_order` argument: the numbers it lists, each a column of the group columns and then
    /// the value columns (`columns` of them).
    fn sort_arg(
        &self,
        args: &[Expr],
        i: Option<usize>,
        columns: usize,
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Vec<SortKey>, FormulaError> {
        let Some(arg) = i.and_then(|i| present(args, i)) else {
            return Ok(Vec::new());
        };
        let grid = self.grid_arg(arg, env, ctx, depth)?;
        grid.data
            .iter()
            .map(|v| {
                let n = match v {
                    V::Empty => 0,
                    other => other.as_number().ok_or(FormulaError::Value)?.trunc() as i64,
                };
                let column = n.unsigned_abs() as usize;
                if column == 0 || column > columns {
                    return Err(FormulaError::Value);
                }
                Ok(SortKey {
                    column,
                    ascending: n > 0,
                })
            })
            .collect()
    }

    fn func_plan_arg(
        &self,
        arg: &Expr,
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<FuncPlan, FormulaError> {
        parse_func_plan(self.eval_val(arg, env, ctx, depth + 1)?)
    }

    /// `group` (and `total`, for a function of two arguments) through the aggregator.
    fn aggregate(
        &self,
        aggregator: &AggFunc,
        group: &Arc<Grid>,
        total: Option<Arc<Grid>>,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<V, FormulaError> {
        // the function reads every value it is given: that work is paid for in steps
        let work = group.data.len() + total.as_ref().map_or(0, |t| t.data.len());
        ctx.charge_steps(1 + work / STEP_VALUES)?;
        let f = Val::Lambda(Arc::clone(&aggregator.closure));
        let mut call_args = Vec::with_capacity(1 + usize::from(total.is_some()));
        call_args.push(Val::Grid(Arc::clone(group)));
        if let Some(t) = total {
            call_args.push(Val::Grid(t));
        }
        Ok(single(self.apply_val(&f, call_args, ctx, depth + 1)?))
    }

    /// The arguments of a call, checked, and the rows that take part.
    fn read_request(
        &self,
        layout: &'static Layout,
        min_args: usize,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Request, FormulaError> {
        if args.len() < min_args || args.len() > layout.max_args {
            return Err(FormulaError::Value);
        }
        let row_fields = match present(args, layout.rows) {
            Some(arg) => Some(self.grid_arg(arg, env, ctx, depth)?),
            None => None,
        };
        let col_fields = match layout.cols.and_then(|i| present(args, i)) {
            Some(arg) => Some(self.grid_arg(arg, env, ctx, depth)?),
            None => None,
        };
        if row_fields.is_none() && col_fields.is_none() {
            return Err(FormulaError::Value);
        }
        let values_arg = present(args, layout.values).ok_or(FormulaError::Value)?;
        let values = self.grid_arg(values_arg, env, ctx, depth)?;
        let plan = self.func_plan_arg(&args[layout.function], env, ctx, depth)?;
        let headers = self.whole_number_arg(args, layout.headers, env, ctx, depth)?;
        let row_totals = self.whole_number_arg(args, layout.row_totals, env, ctx, depth)?;
        let col_totals = match layout.col_totals {
            Some(i) => self.whole_number_arg(args, i, env, ctx, depth)?,
            None => None,
        };
        let last = self.whole_number_arg(args, layout.last, env, ctx, depth)?;
        let (table, relative_to) = match (layout.cols, last.unwrap_or(0)) {
            (None, n @ 0..=1) => (n == 1, 0),
            (Some(_), n @ 0..=4) => (false, n),
            _ => return Err(FormulaError::Value),
        };
        let n = values.rows;
        let kr = row_fields.as_ref().map_or(0, |g| g.cols);
        let kc = col_fields.as_ref().map_or(0, |g| g.cols);
        let m = values.cols;
        if kr > MAX_GROUP_FIELDS || kc > MAX_GROUP_FIELDS || m > MAX_VALUE_COLUMNS {
            return Err(FormulaError::Value);
        }
        if row_fields.as_ref().is_some_and(|g| g.rows != n)
            || col_fields.as_ref().is_some_and(|g| g.rows != n)
        {
            return Err(FormulaError::Value);
        }
        let fn_cols = plan.fn_cols();
        let row_sort = self.sort_arg(
            args,
            Some(layout.row_sort),
            kr + m * fn_cols,
            env,
            ctx,
            depth,
        )?;
        let col_sort = self.sort_arg(args, layout.col_sort, kc + m * fn_cols, env, ctx, depth)?;
        let filter = match present(args, layout.filter) {
            Some(arg) => Some(self.grid_arg(arg, env, ctx, depth)?),
            None => None,
        };
        if filter.as_ref().is_some_and(|g| g.rows != n || g.cols != 1) {
            return Err(FormulaError::Value);
        }

        // headers: present in the data, shown in the answer
        let auto = n >= 2
            && matches!(values.get(0, 0), V::Text(_))
            && matches!(values.get(1, 0), V::Number(_));
        let (has_headers, show_headers) = match headers {
            None => (auto, auto),
            Some(0) => (false, false),
            Some(1) => (true, false),
            Some(2) => (false, true),
            Some(3) => (true, true),
            Some(_) => return Err(FormulaError::Value),
        };
        let mut kept: Vec<usize> = Vec::with_capacity(n);
        for r in usize::from(has_headers)..n {
            let keep = match filter.as_ref().map(|g| g.get(r, 0)) {
                None => true,
                Some(V::Boolean(b)) => *b,
                Some(V::Number(x)) => *x != 0.0,
                Some(V::Error(e)) => return Err(*e),
                Some(_) => return Err(FormulaError::Value),
            };
            if keep {
                kept.push(r);
            }
        }
        if kept.is_empty() {
            return Err(FormulaError::Calc);
        }
        if kept.len().saturating_mul((kr + 1).saturating_mul(kc + 1)) > MAX_BUCKET_PUSHES {
            return Err(FormulaError::Spill);
        }
        Ok(Request {
            row_fields,
            col_fields,
            values,
            plan,
            table,
            relative_to,
            row_totals,
            col_totals,
            row_sort,
            col_sort,
            has_headers,
            show_headers,
            kept,
        })
    }

    fn group_rows(
        &self,
        layout: &'static Layout,
        min_args: usize,
        args: &[Expr],
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> ValResult {
        let req = self.read_request(layout, min_args, args, env, ctx, depth)?;
        let groups = self.build_groups(&req, ctx, depth)?;
        let (row_count, col_count) = (groups.row_entries.len(), groups.col_entries.len());
        let kr = req.row_fields.as_ref().map_or(0, |g| g.cols);
        let (kc, m) = (groups.col_tree.fields, req.values.cols);
        let shape = OutputShape::compute(
            &req.plan,
            OutputSpec {
                kr,
                kc,
                m,
                has_col_fields: req.col_fields.is_some(),
                show_headers: req.show_headers,
                row_count,
                col_count,
            },
        )?;
        ctx.charge_array_cells(shape.out_rows, shape.out_cols)?;
        // a cell with nothing in it is empty text: an empty value would read 0 where it spills
        let mut out = vec![V::Text(String::new()); shape.out_rows * shape.out_cols];
        req.write_headers(&groups, &shape, &mut out);
        self.write_values(&req, &groups, &shape, &mut out, ctx, depth)?;
        Ok(Val::grid(Grid::from_rows_with_links_precharged(
            ctx,
            shape.out_rows,
            shape.out_cols,
            out,
            None,
        )?))
    }

    /// The row groups and column groups, in order, and the rows in the cell of every pair.
    fn build_groups<'r>(
        &self,
        req: &'r Request,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Groups<'r>, FormulaError> {
        let rows = req.kept.len();
        let kr = req.row_fields.as_ref().map_or(0, |g| g.cols);
        let kc = req.col_fields.as_ref().map_or(0, |g| g.cols);
        // the groups, and the lower-cased copy of each key text the index holds, are paid for before
        // they are made: every row may start a group on every level
        ctx.charge_array_extra_bytes(
            rows.saturating_mul(kr + kc)
                .saturating_mul(GROUP_NODE_BYTES),
        )?;
        ctx.charge_array_extra_bytes(req.key_text_bytes())?;
        let mut row_tree = Tree::build(kr, rows, req.table, |l, i| {
            req.row_fields
                .as_ref()
                .map_or(V::Empty, |g| g.get(req.kept[i], l).clone())
        });
        let mut col_tree = Tree::build(kc, rows, false, |l, i| {
            req.col_fields
                .as_ref()
                .map_or(V::Empty, |g| g.get(req.kept[i], l).clone())
        });
        let all_rows: Vec<u32> = (0..rows as u32).collect();
        // the column of every row, made once per value column for a function that compares with it
        let mut all_columns: Vec<Option<Arc<Grid>>> = vec![None; req.values.cols];
        for (tree, sort, table) in [
            (&mut row_tree, &req.row_sort, req.table),
            (&mut col_tree, &req.col_sort, false),
        ] {
            let mut aggregate = |node: &Node, idx: usize| {
                let (f, j) = req.plan.sort_func_col(idx, req.values.cols);
                let func = &req.plan.funcs[f];
                let total = if func.wants_total {
                    if all_columns[j].is_none() {
                        all_columns[j] = Some(req.column_grid(&all_rows, j, ctx)?);
                    }
                    all_columns[j].clone()
                } else {
                    None
                };
                let group = Arc::new(Grid::from_rows(
                    node.rows.len(),
                    1,
                    req.column(&node.rows, j),
                ));
                self.aggregate(func, &group, total, ctx, depth)
            };
            tree.sort(sort, table, &mut aggregate)?;
        }
        let row_entries = if req.row_fields.is_some() {
            row_tree.entries(Totals::new(req.row_totals, kr, req.table)?, req.table)
        } else {
            let _ = Totals::new(req.row_totals, 0, req.table)?;
            vec![Entry {
                node: 0,
                kind: Kind::Grand,
            }]
        };
        let col_entries = if req.col_fields.is_some() {
            col_tree.entries(Totals::new(req.col_totals, kc, false)?, false)
        } else {
            let _ = Totals::new(req.col_totals, 0, false)?;
            vec![Entry {
                node: 0,
                kind: Kind::Grand,
            }]
        };
        let (row_count, col_count) = (row_entries.len(), col_entries.len());
        ctx.charge_array_cells(row_count, col_count)?;
        // each row goes into the cell of every shown row group and column group it is in
        let row_entry_of = entry_of(&row_entries, row_tree.nodes.len());
        let col_entry_of = entry_of(&col_entries, col_tree.nodes.len());
        let mut cells: Vec<Vec<u32>> = vec![Vec::new(); row_count * col_count];
        let (mut row_chain, mut col_chain) = (Vec::new(), Vec::new());
        for i in 0..rows {
            shown_chain(
                &row_tree,
                &row_entry_of,
                row_tree.leaf_of_row[i],
                &mut row_chain,
            );
            shown_chain(
                &col_tree,
                &col_entry_of,
                col_tree.leaf_of_row[i],
                &mut col_chain,
            );
            for &ri in &row_chain {
                for &ci in &col_chain {
                    cells[ri * col_count + ci].push(i as u32);
                }
            }
        }
        Ok(Groups {
            row_tree,
            col_tree,
            row_entries,
            col_entries,
            cells,
            all_rows,
        })
    }

    /// The row labels and the value of every cell that has rows in it.
    fn write_values(
        &self,
        req: &Request,
        groups: &Groups<'_>,
        shape: &OutputShape,
        out: &mut [V],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<(), FormulaError> {
        let kr = req.row_fields.as_ref().map_or(0, |g| g.cols);
        let m = req.values.cols;
        let f_len = req.plan.funcs.len();
        let col_count = groups.col_entries.len();
        let mut ticks = 0usize;
        // the columns of the totals, each made once: (relative_to, row group, column group, value column)
        let mut totals: HashMap<(i64, usize, usize, usize), Arc<Grid>> = HashMap::new();
        if req.plan.orientation == FuncOrientation::Vertical {
            for (ri, re) in groups.row_entries.iter().enumerate() {
                let row_labels = (kr > 0).then(|| groups.row_tree.labels(re));
                let repeat_labels = req.table && re.kind == Kind::Leaf;
                for f in 0..f_len {
                    let line = (shape.top + ri * f_len + f) * shape.out_cols;
                    if let Some(labels) = &row_labels {
                        if f == 0 || repeat_labels {
                            for (l, label) in labels.iter().take(kr).enumerate() {
                                out[line + l] = label.clone();
                            }
                        }
                    }
                    out[line + kr] = req.plan.funcs[f].header_value();
                }
                for (ci, ce) in groups.col_entries.iter().enumerate() {
                    let members = &groups.cells[ri * col_count + ci];
                    if members.is_empty() {
                        continue;
                    }
                    for j in 0..m {
                        let group =
                            Arc::new(Grid::from_rows(members.len(), 1, req.column(members, j)));
                        let mut total_cache: Option<Arc<Grid>> = None;
                        for f in 0..f_len {
                            let func = &req.plan.funcs[f];
                            let total = if func.wants_total {
                                if total_cache.is_none() {
                                    total_cache = Some(req.cached_total(
                                        groups,
                                        re.node,
                                        ce.node,
                                        j,
                                        &mut totals,
                                        ctx,
                                    )?);
                                }
                                total_cache.clone()
                            } else {
                                None
                            };
                            let line = (shape.top + ri * f_len + f) * shape.out_cols;
                            out[line + shape.prefix_cols + ci * m + j] =
                                self.aggregate(func, &group, total, ctx, depth)?;
                            ticks += 1;
                            if ticks & 255 == 0 {
                                ctx.charge_steps(1)?;
                                ctx.poll_deadline();
                                if ctx.expired() {
                                    return Err(FormulaError::Value);
                                }
                            }
                        }
                    }
                }
            }
            return Ok(());
        }

        for (ri, re) in groups.row_entries.iter().enumerate() {
            let line = (shape.top + ri) * shape.out_cols;
            if kr > 0 {
                for (l, label) in groups.row_tree.labels(re).into_iter().take(kr).enumerate() {
                    out[line + l] = label;
                }
            }
            for (ci, ce) in groups.col_entries.iter().enumerate() {
                let members = &groups.cells[ri * col_count + ci];
                if members.is_empty() {
                    continue;
                }
                for j in 0..m {
                    let group = Arc::new(Grid::from_rows(members.len(), 1, req.column(members, j)));
                    let mut total_cache: Option<Arc<Grid>> = None;
                    for f in 0..f_len {
                        let func = &req.plan.funcs[f];
                        let total = if func.wants_total {
                            if total_cache.is_none() {
                                total_cache = Some(req.cached_total(
                                    groups,
                                    re.node,
                                    ce.node,
                                    j,
                                    &mut totals,
                                    ctx,
                                )?);
                            }
                            total_cache.clone()
                        } else {
                            None
                        };
                        out[line + kr + ci * (m * f_len) + f * m + j] =
                            self.aggregate(func, &group, total, ctx, depth)?;
                        ticks += 1;
                        if ticks & 255 == 0 {
                            ctx.charge_steps(1)?;
                            ctx.poll_deadline();
                            if ctx.expired() {
                                return Err(FormulaError::Value);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

/// The index of the shown row of each group that has one, `NO_ENTRY` for the others.
fn entry_of(entries: &[Entry], nodes: usize) -> Vec<usize> {
    let mut map = vec![NO_ENTRY; nodes];
    for (i, e) in entries.iter().enumerate() {
        map[e.node] = i;
    }
    map
}

/// The shown rows of the groups from the group of a data row up to the group of every row.
fn shown_chain(tree: &Tree<'_>, entry_of: &[usize], leaf: usize, out: &mut Vec<usize>) {
    out.clear();
    let mut node = leaf;
    loop {
        if entry_of[node] != NO_ENTRY {
            out.push(entry_of[node]);
        }
        if node == 0 {
            break;
        }
        node = tree.nodes[node].parent;
    }
}

/// The arguments of a call, read and checked.
struct Request {
    row_fields: Option<Arc<Grid>>,
    col_fields: Option<Arc<Grid>>,
    values: Arc<Grid>,
    plan: FuncPlan,
    table: bool,
    relative_to: i64,
    row_totals: Option<i64>,
    col_totals: Option<i64>,
    row_sort: Vec<SortKey>,
    col_sort: Vec<SortKey>,
    /// The first row of the data is a header row.
    has_headers: bool,
    show_headers: bool,
    /// The rows of the data that take part (past the header row, `filter_array` true).
    kept: Vec<usize>,
}

impl Request {
    fn cached_total(
        &self,
        groups: &Groups<'_>,
        row_node: usize,
        col_node: usize,
        j: usize,
        totals: &mut HashMap<(i64, usize, usize, usize), Arc<Grid>>,
        ctx: &mut EvalContext<'_>,
    ) -> Result<Arc<Grid>, FormulaError> {
        let (rt, ct) = (&groups.row_tree.nodes, &groups.col_tree.nodes);
        let key = match self.relative_to {
            0 => (0, col_node, 0, j),
            1 => (1, row_node, 0, j),
            2 => (2, 0, 0, j),
            3 => (3, rt[row_node].parent, col_node, j),
            _ => (4, row_node, ct[col_node].parent, j),
        };
        Ok(match totals.entry(key) {
            MapEntry::Occupied(done) => Arc::clone(done.get()),
            MapEntry::Vacant(slot) => {
                let set = match key.0 {
                    0 => ct[key.1].rows.clone(),
                    1 => rt[key.1].rows.clone(),
                    2 => groups.all_rows.clone(),
                    _ => intersect(&rt[key.1].rows, &ct[key.2].rows),
                };
                Arc::clone(slot.insert(self.column_grid(&set, j, ctx)?))
            }
        })
    }

    /// The values of value column `j` in the kept rows `rows` (positions in `kept`).
    fn column(&self, rows: &[u32], j: usize) -> Vec<V> {
        rows.iter()
            .map(|&i| self.values.get(self.kept[i as usize], j).clone())
            .collect()
    }

    /// The same values as a column grid, paid for in the array budget.
    fn column_grid(
        &self,
        rows: &[u32],
        j: usize,
        ctx: &mut EvalContext<'_>,
    ) -> Result<Arc<Grid>, FormulaError> {
        ctx.charge_array_cells(rows.len(), 1)?;
        Ok(Arc::new(Grid::from_rows(
            rows.len(),
            1,
            self.column(rows, j),
        )))
    }

    /// Bytes of the texts of the group columns in the kept rows.
    fn key_text_bytes(&self) -> usize {
        self.kept
            .iter()
            .map(|&r| {
                let line = |g: &Grid| (0..g.cols).map(|c| g.get(r, c).heap_bytes()).sum::<usize>();
                self.row_fields.as_ref().map_or(0, |g| line(g))
                    + self.col_fields.as_ref().map_or(0, |g| line(g))
            })
            .sum()
    }

    /// The name of field `l` of `grid`: its header, or a generated one.
    fn field_name(&self, grid: &Grid, l: usize, generated: &str) -> V {
        if self.has_headers {
            grid.get(0, l).clone()
        } else if grid.cols > 1 {
            V::Text(format!("{generated}{}", l + 1))
        } else {
            V::Text(generated.to_string())
        }
    }

    /// The column labels above the values and the field names.
    fn write_headers(&self, groups: &Groups<'_>, shape: &OutputShape, out: &mut [V]) {
        let kr = self.row_fields.as_ref().map_or(0, |g| g.cols);
        let kc = groups.col_tree.fields;
        let m = self.values.cols;
        let width = shape.out_cols;
        let fn_cols = self.plan.fn_cols();
        let col_count = groups.col_entries.len();
        let prefix = shape.prefix_cols;
        if let Some(cols) = &self.col_fields {
            let block = m * fn_cols;
            for (ci, entry) in groups.col_entries.iter().enumerate() {
                for (l, label) in groups
                    .col_tree
                    .labels(entry)
                    .into_iter()
                    .take(kc)
                    .enumerate()
                {
                    for k in 0..block {
                        out[l * width + prefix + ci * block + k] = label.clone();
                    }
                }
            }
            if self.show_headers && prefix > 0 {
                for l in 0..cols.cols {
                    out[l * width + prefix - 1] = self.field_name(cols, l, "Column");
                }
            }
        }
        if shape.show_fn_row {
            let fn_row = if shape.show_field_row {
                kc
            } else {
                shape.top - 1
            };
            let line = fn_row * width;
            if !shape.show_field_row && self.show_headers {
                if let Some(rows) = &self.row_fields {
                    for l in 0..kr {
                        out[line + l] = self.field_name(rows, l, "Group");
                    }
                }
            }
            for ci in 0..col_count {
                for (f, func) in self.plan.funcs.iter().enumerate() {
                    let lbl = func.header_value();
                    for j in 0..m {
                        out[line + prefix + ci * (m * fn_cols) + f * m + j] = lbl.clone();
                    }
                }
            }
        }
        if shape.show_field_row && shape.top > 0 {
            let line = (shape.top - 1) * width;
            if self.show_headers {
                if let Some(rows) = &self.row_fields {
                    for l in 0..kr {
                        out[line + l] = self.field_name(rows, l, "Group");
                    }
                }
                if self.plan.orientation == FuncOrientation::Vertical {
                    out[line + kr] = V::Text("Function".to_string());
                }
            }
            for ci in 0..col_count {
                for f in 0..fn_cols {
                    for j in 0..m {
                        out[line + prefix + ci * (m * fn_cols) + f * m + j] =
                            self.field_name(&self.values, j, "Value");
                    }
                }
            }
        }
    }
}

/// The groups of both axes.
struct Groups<'r> {
    row_tree: Tree<'r>,
    col_tree: Tree<'r>,
    row_entries: Vec<Entry>,
    col_entries: Vec<Entry>,
    /// The kept rows (positions) of every pair of shown row and column, row major.
    cells: Vec<Vec<u32>>,
    all_rows: Vec<u32>,
}

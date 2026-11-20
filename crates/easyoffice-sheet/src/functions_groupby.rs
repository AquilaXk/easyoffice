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
//! `PERCENTOF`). A function is an eta-reduced name (`SUM`, `AVERAGE`, ...) or a `LAMBDA`. Not
//! implemented: a list of functions that makes one result column per function (`#VALUE!`).
//!
//! The pass over the rows is single: each row is put in the cell of every shown row group and
//! column group it belongs to, so the cost is the rows times the levels of the two axes and the
//! number of cells is checked against the array budget before anything is allocated. The
//! documentation shows the output only as pictures, so the generated header names (`Group`,
//! `Column`, `Value`), the layout of the field header row and the labels of the total rows are this
//! engine's choice (`groupby_axis.rs`). The group function runs once per cell and value column.

use crate::array::{Closure, Env, Grid, Val};
use crate::array_eval::ValResult;
use crate::groupby_axis::{intersect, Entry, Kind, Node, SortKey, Totals, Tree};
use crate::{EvalContext, EvaluatedCellValue as V, Expr, FormulaError, SheetEngine};
use std::collections::HashMap;
use std::sync::Arc;

/// Group columns on one axis.
const MAX_GROUP_FIELDS: usize = 16;
/// Value columns.
const MAX_VALUE_COLUMNS: usize = 64;
/// Rows put into cells, over all rows: the rows times the levels of both axes.
const MAX_BUCKET_PUSHES: usize = 8_000_000;
const NO_ENTRY: usize = usize::MAX;
/// Bytes one group costs before it exists: the group, its row list entries and its index entry.
const GROUP_NODE_BYTES: usize = 160;
/// Values a function reads for one evaluation step.
const STEP_VALUES: usize = 16;

/// What turns the values of a group into one value: a `LAMBDA` (a function named without its
/// arguments, `SUM`, was made one when the formula was read, see `parser_array.rs`).
type Aggregator = Arc<Closure>;

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

    fn aggregator(
        &self,
        arg: &Expr,
        env: &Env,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Aggregator, FormulaError> {
        match self.eval_val(arg, env, ctx, depth + 1)? {
            Val::Lambda(closure) => Ok(closure),
            Val::Grid(g) => match g.data.as_slice() {
                [V::Error(e)] => Err(*e),
                _ => Err(FormulaError::Value),
            },
            _ => Err(FormulaError::Value),
        }
    }

    /// `group` (and `total`, for a function of two arguments) through the aggregator.
    fn aggregate(
        &self,
        aggregator: &Aggregator,
        group: Vec<V>,
        total: Option<Arc<Grid>>,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<V, FormulaError> {
        // the function reads every value it is given: that work is paid for in steps
        let work = group.len() + total.as_ref().map_or(0, |t| t.data.len());
        ctx.charge_steps(1 + work / STEP_VALUES)?;
        let mut grids = vec![Arc::new(Grid::from_rows(group.len(), 1, group))];
        grids.extend(total);
        let f = Val::Lambda(Arc::clone(aggregator));
        let call_args = grids.into_iter().map(Val::Grid).collect();
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
        let row_fields = self.grid_arg(&args[layout.rows], env, ctx, depth)?;
        let col_fields = match layout.cols {
            Some(i) => Some(self.grid_arg(&args[i], env, ctx, depth)?),
            None => None,
        };
        let values = self.grid_arg(&args[layout.values], env, ctx, depth)?;
        let aggregator = self.aggregator(&args[layout.function], env, ctx, depth)?;
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
        let (n, kr, m) = (row_fields.rows, row_fields.cols, values.cols);
        let kc = col_fields.as_ref().map_or(0, |g| g.cols);
        if kr > MAX_GROUP_FIELDS || kc > MAX_GROUP_FIELDS || m > MAX_VALUE_COLUMNS {
            return Err(FormulaError::Value);
        }
        if values.rows != n || col_fields.as_ref().is_some_and(|g| g.rows != n) {
            return Err(FormulaError::Value);
        }
        let row_sort = self.sort_arg(args, Some(layout.row_sort), kr + m, env, ctx, depth)?;
        let col_sort = self.sort_arg(args, layout.col_sort, kc + m, env, ctx, depth)?;
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
            aggregator,
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
        let (kr, kc, m) = (req.row_fields.cols, groups.col_tree.fields, req.values.cols);
        // the answer: the header rows, then one row per shown row group
        let show_field_row = req.show_headers || (req.col_fields.is_some() && m > 1);
        let top = if req.col_fields.is_some() {
            kc + usize::from(show_field_row)
        } else {
            usize::from(req.show_headers)
        };
        let (out_rows, out_cols) = (top + row_count, kr + col_count * m);
        ctx.charge_array_cells(out_rows, out_cols)?;
        // a cell with nothing in it is empty text: an empty value would read 0 where it spills
        let mut out = vec![V::Text(String::new()); out_rows * out_cols];
        let shape = Shape {
            top,
            width: out_cols,
            show_field_row,
        };
        req.write_headers(&groups, &shape, &mut out);
        self.write_values(&req, &groups, &shape, &mut out, ctx, depth)?;
        Ok(Val::grid(Grid::from_rows_checked(
            ctx, out_rows, out_cols, out,
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
        let (kr, kc) = (
            req.row_fields.cols,
            req.col_fields.as_ref().map_or(0, |g| g.cols),
        );
        // the groups, and the lower-cased copy of each key text the index holds, are paid for before
        // they are made: every row may start a group on every level
        ctx.charge_array_extra_bytes(
            rows.saturating_mul(kr + kc)
                .saturating_mul(GROUP_NODE_BYTES),
        )?;
        ctx.charge_array_extra_bytes(req.key_text_bytes())?;
        let mut row_tree = Tree::build(kr, rows, req.table, |l, i| {
            req.row_fields.get(req.kept[i], l).clone()
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
            let mut aggregate = |node: &Node, j: usize| {
                let total = if req.wants_total() {
                    if all_columns[j].is_none() {
                        all_columns[j] = Some(req.column_grid(&all_rows, j, ctx)?);
                    }
                    all_columns[j].clone()
                } else {
                    None
                };
                self.aggregate(
                    &req.aggregator,
                    req.column(&node.rows, j),
                    total,
                    ctx,
                    depth,
                )
            };
            tree.sort(sort, table, &mut aggregate)?;
        }
        let row_entries = row_tree.entries(Totals::new(req.row_totals, kr, req.table)?, req.table);
        let col_entries = if req.col_fields.is_some() {
            col_tree.entries(Totals::new(req.col_totals, kc, false)?, false)
        } else {
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
        shape: &Shape,
        out: &mut [V],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<(), FormulaError> {
        let (kr, m) = (req.row_fields.cols, req.values.cols);
        let col_count = groups.col_entries.len();
        let wants_total = req.wants_total();
        let (rt, ct) = (&groups.row_tree.nodes, &groups.col_tree.nodes);
        let mut ticks = 0usize;
        // the columns of the totals, each made once: (relative_to, row group, column group, value column)
        let mut totals: HashMap<(i64, usize, usize, usize), Arc<Grid>> = HashMap::new();
        for (ri, re) in groups.row_entries.iter().enumerate() {
            let line = (shape.top + ri) * shape.width;
            for (l, label) in groups.row_tree.labels(re).into_iter().enumerate() {
                out[line + l] = label;
            }
            for (ci, ce) in groups.col_entries.iter().enumerate() {
                let members = &groups.cells[ri * col_count + ci];
                if members.is_empty() {
                    continue;
                }
                for j in 0..m {
                    let total = if wants_total {
                        let key = match req.relative_to {
                            0 => (0, ce.node, 0, j),
                            1 => (1, re.node, 0, j),
                            2 => (2, 0, 0, j),
                            // the parent row group with this column group, or this row group
                            // with the parent column group
                            3 => (3, rt[re.node].parent, ce.node, j),
                            _ => (4, re.node, ct[ce.node].parent, j),
                        };
                        if !totals.contains_key(&key) {
                            let set = match key.0 {
                                0 => ct[key.1].rows.clone(),
                                1 => rt[key.1].rows.clone(),
                                2 => groups.all_rows.clone(),
                                _ => intersect(&rt[key.1].rows, &ct[key.2].rows),
                            };
                            totals.insert(key, req.column_grid(&set, j, ctx)?);
                        }
                        totals.get(&key).cloned()
                    } else {
                        None
                    };
                    let group = req.column(members, j);
                    out[line + kr + ci * m + j] =
                        self.aggregate(&req.aggregator, group, total, ctx, depth)?;
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
    row_fields: Arc<Grid>,
    col_fields: Option<Arc<Grid>>,
    values: Arc<Grid>,
    aggregator: Aggregator,
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
    /// A function of two arguments gets the group and a total to compare it with.
    fn wants_total(&self) -> bool {
        self.aggregator.params.len() == 2
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
                line(&self.row_fields) + self.col_fields.as_ref().map_or(0, |g| line(g))
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
    fn write_headers(&self, groups: &Groups<'_>, shape: &Shape, out: &mut [V]) {
        let (kr, m) = (self.row_fields.cols, self.values.cols);
        let width = shape.width;
        if let Some(cols) = &self.col_fields {
            for (ci, entry) in groups.col_entries.iter().enumerate() {
                for (l, label) in groups.col_tree.labels(entry).into_iter().enumerate() {
                    for j in 0..m {
                        out[l * width + kr + ci * m + j] = label.clone();
                    }
                }
            }
            if self.show_headers {
                for l in 0..cols.cols {
                    out[l * width + kr.saturating_sub(1)] = self.field_name(cols, l, "Column");
                }
            }
        }
        if shape.show_field_row && shape.top > 0 {
            let line = (shape.top - 1) * width;
            if self.show_headers {
                for l in 0..kr {
                    out[line + l] = self.field_name(&self.row_fields, l, "Group");
                }
            }
            for ci in 0..groups.col_entries.len() {
                for j in 0..m {
                    out[line + kr + ci * m + j] = self.field_name(&self.values, j, "Value");
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

/// Where things go in the answer.
struct Shape {
    /// Header rows above the first row group.
    top: usize,
    width: usize,
    /// The last header row names the fields and the values.
    show_field_row: bool,
}

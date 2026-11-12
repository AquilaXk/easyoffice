//! Database functions: `DAVERAGE`, `DCOUNT`, `DCOUNTA`, `DGET`, `DMAX`, `DMIN`, `DPRODUCT`,
//! `DSTDEV`, `DSTDEVP`, `DSUM`, `DVAR`, `DVARP`.
//!
//! Reference implementation: the database / field / criteria checks (`checkDatabase`, `checkField`,
//! `checkCriteria`), the criteria matching rule (AND across a criteria row, OR across rows; an empty
//! criteria cell is skipped; a criteria column whose header is not a database field never matches a
//! text or zero criterion and is ignored otherwise, which is how calculated `TRUE`/`FALSE`
//! criteria work) and the per-function aggregation and error rules are ported from the Univer formula
//! engine (`packages/engine-formula/src/basics/database.ts` and
//! `src/functions/database/<name>/index.ts`, commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9),
//! Apache-2.0, Copyright 2023-present DreamNum Co., Ltd. See `THIRD_PARTY_NOTICES.md`.
//!
//! Differences from the original: criteria are compiled once into the shared `COUNTIF` matcher
//! ([`CompiledCriteria`]) instead of being re-parsed for every record, records are read one at a time
//! (only the criteria columns and the field column are touched) and the variance family accumulates
//! in one pass (Welford) instead of keeping the values. The criteria block is bounded
//! (`MAX_CRITERIA_CELLS`); the walk over the records is bounded by the evaluation step budget.

use crate::criteria::CompiledCriteria;
use crate::range_refs::StreamingRangeSpec;
use crate::{
    EvalContext, EvaluatedCellValue, Expr, FormulaError, PackedCellCoordinate, SheetEngine,
};

type CellResult = Result<EvaluatedCellValue, FormulaError>;

/// Cells of the criteria block (header row included).
const MAX_CRITERIA_CELLS: usize = 4_096;

#[derive(Clone, Copy, PartialEq, Eq)]
enum DbOp {
    Sum,
    Average,
    Count,
    CountA,
    Get,
    Max,
    Min,
    Product,
    StdevSample,
    StdevPopulation,
    VarSample,
    VarPopulation,
}

fn db_op(name: &str) -> Option<DbOp> {
    Some(match name {
        "DSUM" => DbOp::Sum,
        "DAVERAGE" => DbOp::Average,
        "DCOUNT" => DbOp::Count,
        "DCOUNTA" => DbOp::CountA,
        "DGET" => DbOp::Get,
        "DMAX" => DbOp::Max,
        "DMIN" => DbOp::Min,
        "DPRODUCT" => DbOp::Product,
        "DSTDEV" => DbOp::StdevSample,
        "DSTDEVP" => DbOp::StdevPopulation,
        "DVAR" => DbOp::VarSample,
        "DVARP" => DbOp::VarPopulation,
        _ => return None,
    })
}

/// A rectangular block of the sheet (the database or the criteria range).
struct Block<'a> {
    spec: StreamingRangeSpec<'a>,
    rows: usize,
    cols: usize,
}

impl<'a> Block<'a> {
    fn new(spec: StreamingRangeSpec<'a>) -> Self {
        Self {
            rows: spec.3,
            cols: spec.4,
            spec,
        }
    }

    /// Hands the cell `(dr, dc)` to `f` without copying it.
    fn with_cell<R>(
        &self,
        engine: &SheetEngine,
        (dr, dc): (usize, usize),
        ctx: &mut EvalContext<'_>,
        depth: usize,
        f: impl FnOnce(&EvaluatedCellValue) -> R,
    ) -> Result<R, FormulaError> {
        if ctx.steps > SheetEngine::MAX_EVAL_STEPS {
            return Err(FormulaError::Value);
        }
        let (ref sheet, min_r, min_c, _, _, transposed) = self.spec;
        let (r, c) = if transposed {
            (min_r + dc as u32, min_c + dr as u32)
        } else {
            (min_r + dr as u32, min_c + dc as u32)
        };
        let coord = PackedCellCoordinate::new(r, c).map_err(|_| FormulaError::Ref)?;
        Ok(engine.with_sheet_coord_value(sheet.as_deref(), coord, ctx, depth, f))
    }

    fn cell(
        &self,
        engine: &SheetEngine,
        at: (usize, usize),
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        self.with_cell(engine, at, ctx, depth, EvaluatedCellValue::clone)
    }
}

/// One criterion cell: the matcher, and whether the criterion is ignored when its header names no
/// database field.
struct Term {
    crit: CompiledCriteria,
    /// Text and zero criteria reject every record when the header is unknown; other numbers pass.
    rejects_unknown_header: bool,
}

/// Normalizes a criteria cell: blank cells are skipped, logical values count as 1 and 0, numeric
/// text as numbers.
fn term_of(v: EvaluatedCellValue) -> Option<Term> {
    let v = match v {
        EvaluatedCellValue::Empty => return None,
        EvaluatedCellValue::Boolean(b) => EvaluatedCellValue::Number(f64::from(u8::from(b))),
        EvaluatedCellValue::Text(s) => match crate::coercion::parse_strict_number(&s) {
            Some(n) => EvaluatedCellValue::Number(n),
            None => EvaluatedCellValue::Text(s),
        },
        other => other,
    };
    let rejects_unknown_header = match v {
        EvaluatedCellValue::Number(n) => n == 0.0,
        _ => true,
    };
    Some(Term {
        crit: CompiledCriteria::compile(&v),
        rejects_unknown_header,
    })
}

/// Lower-cased header text of every database column (`None` for blank headers).
fn header_names(headers: Vec<EvaluatedCellValue>) -> Vec<Option<String>> {
    headers
        .into_iter()
        .map(|h| match h {
            EvaluatedCellValue::Empty => None,
            other => Some(other.to_display_string().to_lowercase()),
        })
        .collect()
}

fn header_index(names: &[Option<String>], wanted: &str) -> Option<usize> {
    let wanted = wanted.to_lowercase();
    names
        .iter()
        .position(|n| n.as_deref() == Some(wanted.as_str()))
}

/// The criteria rows as alternatives of `(database column, term)` conjunctions.
struct Criteria {
    /// Some criteria row has no active term, so every record matches.
    match_all: bool,
    rows: Vec<Vec<(usize, Term)>>,
}

impl Criteria {
    fn matches(
        &self,
        engine: &SheetEngine,
        db: &Block<'_>,
        record: usize,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<bool, FormulaError> {
        if self.match_all {
            return Ok(true);
        }
        'rows: for row in &self.rows {
            for (col, term) in row {
                let hit = db.with_cell(engine, (record, *col), ctx, depth, |v| {
                    !matches!(v, EvaluatedCellValue::Empty) && term.crit.matches(v)
                })?;
                if !hit {
                    continue 'rows;
                }
            }
            return Ok(true);
        }
        Ok(false)
    }
}

/// Running totals of the field values of the matching records.
#[derive(Default)]
struct Totals {
    sum: f64,
    product: f64,
    min: f64,
    max: f64,
    numbers: usize,
    non_blank: usize,
    mean: f64,
    m2: f64,
    /// DGET: the field value of the first match and the number of matches.
    first: Option<EvaluatedCellValue>,
}

impl Totals {
    fn new() -> Self {
        Self {
            product: 1.0,
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
            ..Self::default()
        }
    }

    fn add_number(&mut self, x: f64) {
        self.numbers += 1;
        self.sum += x;
        self.product *= x;
        self.min = self.min.min(x);
        self.max = self.max.max(x);
        let delta = x - self.mean;
        self.mean += delta / self.numbers as f64;
        self.m2 += delta * (x - self.mean);
    }

    fn finish(&self, op: DbOp) -> CellResult {
        let n = self.numbers as f64;
        let number = |x: f64| Ok(EvaluatedCellValue::Number(x));
        match op {
            DbOp::Sum => number(self.sum),
            DbOp::Count => number(n),
            DbOp::CountA => number(self.non_blank as f64),
            DbOp::Average if self.numbers == 0 => Err(FormulaError::DivByZero),
            DbOp::Average => number(self.sum / n),
            DbOp::Max | DbOp::Min | DbOp::Product if self.numbers == 0 => number(0.0),
            DbOp::Max => number(self.max),
            DbOp::Min => number(self.min),
            DbOp::Product => number(self.product),
            DbOp::StdevSample | DbOp::VarSample if self.numbers < 2 => Err(FormulaError::DivByZero),
            DbOp::StdevPopulation | DbOp::VarPopulation if self.numbers == 0 => {
                Err(FormulaError::DivByZero)
            }
            DbOp::VarSample => number(self.m2 / (n - 1.0)),
            DbOp::StdevSample => number((self.m2 / (n - 1.0)).sqrt()),
            DbOp::VarPopulation => number(self.m2 / n),
            DbOp::StdevPopulation => number((self.m2 / n).sqrt()),
            DbOp::Get => match (self.non_blank, &self.first) {
                (1, Some(v)) => Ok(v.clone()),
                (0, _) => Err(FormulaError::Value),
                _ => Err(FormulaError::Num),
            },
        }
    }
}

/// The numeric value of a database cell: numbers and numeric text.
fn field_number(v: &EvaluatedCellValue) -> Option<f64> {
    match v {
        EvaluatedCellValue::Number(n) => Some(*n),
        EvaluatedCellValue::Text(s) => crate::coercion::parse_strict_number(s),
        _ => None,
    }
}

impl SheetEngine {
    pub(crate) fn eval_db_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        let op = db_op(name).ok_or(FormulaError::Name)?;
        if args.len() != 3 {
            return Err(FormulaError::Value);
        }
        let db = self.db_block(&args[0], 2, usize::MAX, ctx, depth)?;
        let headers = header_names(
            (0..db.cols)
                .map(|c| db.cell(self, (0, c), ctx, depth))
                .collect::<Result<_, _>>()?,
        );
        let field = self.db_field(&args[1], &headers, ctx, depth)?;
        let crit_block = self.db_block(&args[2], 2, MAX_CRITERIA_CELLS, ctx, depth)?;
        let criteria = self.db_criteria(&crit_block, &headers, ctx, depth)?;

        let mut totals = Totals::new();
        for record in 1..db.rows {
            if !criteria.matches(self, &db, record, ctx, depth)? {
                continue;
            }
            db.with_cell(self, (record, field), ctx, depth, |v| {
                if !matches!(v, EvaluatedCellValue::Empty) {
                    totals.non_blank += 1;
                    if op == DbOp::Get && totals.first.is_none() {
                        totals.first = Some(v.clone());
                    }
                }
                if let Some(x) = field_number(v) {
                    totals.add_number(x);
                }
            })?;
        }
        totals.finish(op)
    }

    /// A range argument of at least `min_rows` rows and at most `max_cells` cells.
    fn db_block<'b>(
        &self,
        expr: &'b Expr,
        min_rows: usize,
        max_cells: usize,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Block<'b>, FormulaError> {
        let spec = self
            .extract_streaming_range_ctx(expr, ctx, depth)?
            .ok_or(FormulaError::Value)?;
        let block = Block::new(spec);
        if block.rows < min_rows || block.rows.saturating_mul(block.cols) > max_cells {
            return Err(FormulaError::Value);
        }
        Ok(block)
    }

    /// The 0-based database column named by the field argument (a 1-based position or a header).
    fn db_field(
        &self,
        arg: &Expr,
        headers: &[Option<String>],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<usize, FormulaError> {
        let by_position = |n: f64| {
            let pos = n.floor();
            if pos >= 1.0 && pos <= headers.len() as f64 {
                Ok(pos as usize - 1)
            } else {
                Err(FormulaError::Value)
            }
        };
        match self.eval_arg_val(arg, ctx, depth)? {
            EvaluatedCellValue::Number(n) => by_position(n),
            EvaluatedCellValue::Boolean(b) => by_position(f64::from(u8::from(b))),
            EvaluatedCellValue::Text(s) => match crate::coercion::parse_strict_number(&s) {
                Some(n) => by_position(n),
                None => header_index(headers, &s).ok_or(FormulaError::Value),
            },
            _ => Err(FormulaError::Value),
        }
    }

    fn db_criteria(
        &self,
        block: &Block<'_>,
        headers: &[Option<String>],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Criteria, FormulaError> {
        let mut titles = Vec::with_capacity(block.cols);
        for c in 0..block.cols {
            titles.push(match block.cell(self, (0, c), ctx, depth)? {
                EvaluatedCellValue::Empty => None,
                title => header_index(headers, &title.to_display_string()),
            });
        }
        let mut criteria = Criteria {
            match_all: false,
            rows: Vec::new(),
        };
        'rows: for r in 1..block.rows {
            let mut row = Vec::new();
            for (c, title) in titles.iter().enumerate() {
                let Some(term) = term_of(block.cell(self, (r, c), ctx, depth)?) else {
                    continue;
                };
                match title {
                    Some(col) => row.push((*col, term)),
                    None if term.rejects_unknown_header => continue 'rows,
                    None => {}
                }
            }
            if row.is_empty() {
                criteria.match_all = true;
                break;
            }
            criteria.rows.push(row);
        }
        Ok(criteria)
    }
}

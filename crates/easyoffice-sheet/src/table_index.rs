//! The tables of a workbook as formulas see them (PD-225): a structured reference such as `Table1[Q1]` is
//! resolved when a formula is compiled, into the cells it stands for in the table's present range, so the
//! dependency graph, the evaluator and the functions that look at the shape of an argument see plain
//! references. A formula remembers the tables it names (a hash of each name, in the formula's
//! [`NameUse`](crate::compile::NameUse)), so a change of one table compiles only the formulas that name it
//! again, as a change of a defined name does (PD-214).
//!
//! 참조 구현: IronCalc `base/src/expressions/parser/mod.rs` lines 1189-1370 resolves a structured reference to
//! a range while it parses, from the table list and the formula's cell (the first table of the name, the
//! cells of the sheet of the table, `#This Row` as the formula's row), MIT OR Apache-2.0, commit
//! 01b39c1ad26252f7b6c810440e8433a932daee07; Univer `table-reference-object.ts` resolves at calculation time
//! with the current row. Here the same decision is taken at compile time with one hash lookup for the table
//! and one for the column (`syntax and rows: easyoffice_formats::structured_ref`).
//!
//! Modifications: a table is found by its name without case in one hash lookup and a column by a hash of its
//! folded name; a reference inside a table that does not name it finds the table by the formula's cell; the
//! tables of the workbook are bounded (the reader's counts) and a name that two sheets share belongs to the
//! first sheet.

use crate::names::{name_key, NameKey};
use crate::{CellRange, Expr, FormulaError, PackedCellCoordinate};
use easyoffice_formats::{
    fold_name, resolve, tables_within_bounds, CellBox, EasyOfficeError, RefError, SheetTable,
    StructuredRef, MAX_TABLES, MAX_TABLE_COLUMNS_TOTAL,
};
use std::collections::{HashMap, HashSet};

/// What a formula remembers of a reference that names no table (inside one, `[@Q1]`): any change of the
/// tables compiles it again.
pub(crate) const IMPLICIT_TABLE_KEY: NameKey = 0;

#[derive(Debug, Clone)]
struct Entry {
    sheet: String,
    table: SheetTable,
    /// Folded column name to position in the table (the first of equal names).
    columns: HashMap<Box<str>, u32>,
}

impl Entry {
    fn new(sheet: String, table: SheetTable) -> Self {
        let mut columns = HashMap::with_capacity(table.columns.len());
        for (i, name) in table.columns.iter().enumerate() {
            columns.entry(fold_name(name).into()).or_insert(i as u32);
        }
        Self {
            sheet,
            table,
            columns,
        }
    }

    /// The parts of the table a formula can see.
    fn same_shape(&self, other: &Entry) -> bool {
        let (a, b) = (&self.table, &other.table);
        self.sheet.eq_ignore_ascii_case(&other.sheet)
            && a.range == b.range
            && a.header == b.header
            && a.totals == b.totals
            && a.columns == b.columns
    }
}

/// The tables of the workbook, all sheets together.
#[derive(Debug, Clone, Default)]
pub struct TableIndex {
    entries: Vec<Entry>,
    by_name: HashMap<Box<str>, usize>,
}

/// Where a formula stands: the sheet and cell it is in (a definition of a name has no cell).
pub(crate) struct TableCtx<'a> {
    pub index: &'a TableIndex,
    pub sheet: &'a str,
    pub coord: Option<PackedCellCoordinate>,
}

fn expr_of(cells: CellBox, sheet: Option<&str>) -> Result<Expr, FormulaError> {
    let at =
        |row: u32, col: u32| PackedCellCoordinate::new(row, col).map_err(|_| FormulaError::Ref);
    let start = at(cells.row0, cells.col0)?;
    let end = at(cells.row1, cells.col1)?;
    let single = start == end;
    Ok(match (sheet, single) {
        (None, true) => Expr::Cell(start),
        (None, false) => Expr::Range(CellRange { start, end }),
        (Some(sheet), true) => Expr::SheetCell {
            sheet: sheet.to_string(),
            coord: start,
        },
        (Some(sheet), false) => Expr::SheetRange {
            sheet: sheet.to_string(),
            range: CellRange { start, end },
        },
    })
}

impl TableIndex {
    /// The index of `tables`, given with the sheet each is on, in the order of the workbook's sheets.
    pub(crate) fn new(tables: Vec<(String, SheetTable)>) -> Self {
        let entries: Vec<Entry> = tables
            .into_iter()
            .map(|(sheet, table)| Entry::new(sheet, table))
            .collect();
        let mut by_name = HashMap::with_capacity(entries.len());
        for (i, e) in entries.iter().enumerate() {
            by_name.entry(fold_name(&e.table.name).into()).or_insert(i);
        }
        Self { entries, by_name }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The tables on `sheet`.
    pub fn tables_of(&self, sheet: &str) -> Vec<SheetTable> {
        self.entries
            .iter()
            .filter(|e| e.sheet.eq_ignore_ascii_case(sheet))
            .map(|e| e.table.clone())
            .collect()
    }

    /// Every table with its sheet, in the order the index holds them.
    pub(crate) fn all(&self) -> impl Iterator<Item = (&str, &SheetTable)> {
        self.entries.iter().map(|e| (e.sheet.as_str(), &e.table))
    }

    /// This index with the tables of `sheet` replaced by `tables`; the other sheets' tables keep their order and
    /// come first, in the order of `sheet_order` (the names of the workbook's sheets).
    pub(crate) fn with_sheet(
        &self,
        sheet: &str,
        tables: Vec<SheetTable>,
        sheet_order: &[&str],
    ) -> Result<Self, EasyOfficeError> {
        let mut all: Vec<(String, SheetTable)> = self
            .entries
            .iter()
            .filter(|e| !e.sheet.eq_ignore_ascii_case(sheet))
            .map(|e| (e.sheet.clone(), e.table.clone()))
            .collect();
        all.extend(tables.into_iter().map(|t| (sheet.to_string(), t)));
        let position = |name: &str| {
            sheet_order
                .iter()
                .position(|s| s.eq_ignore_ascii_case(name))
                .unwrap_or(usize::MAX)
        };
        all.sort_by_key(|(name, _)| position(name));
        if all.len() > MAX_TABLES
            || all.iter().map(|(_, t)| t.columns.len()).sum::<usize>() > MAX_TABLE_COLUMNS_TOTAL
        {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "표 개수·열 개수가 한도({MAX_TABLES}개)를 넘었습니다."
            )));
        }
        Ok(Self::new(all))
    }

    /// Whether the tables of one sheet are within the bounds.
    pub(crate) fn within_bounds(tables: &[SheetTable]) -> bool {
        tables_within_bounds(tables)
    }

    /// The hash of each table name whose table is not the same in `other` (a table that is not in one of them
    /// counts), and [`IMPLICIT_TABLE_KEY`] when there is any.
    pub(crate) fn changed_keys(&self, other: &TableIndex) -> HashSet<NameKey> {
        let mut changed = HashSet::new();
        let mut note = |name: &str| {
            changed.insert(name_key(&fold_name(name)));
        };
        for (folded, &i) in &self.by_name {
            match other.by_name.get(folded) {
                Some(&j) if self.entries[i].same_shape(&other.entries[j]) => {}
                _ => note(&self.entries[i].table.name),
            }
        }
        for (folded, &j) in &other.by_name {
            if !self.by_name.contains_key(folded) {
                note(&other.entries[j].table.name);
            }
        }
        if !changed.is_empty() {
            changed.insert(IMPLICIT_TABLE_KEY);
        }
        changed
    }

    fn containing(&self, sheet: &str, coord: PackedCellCoordinate) -> Option<&Entry> {
        let (row, col) = (coord.row(), coord.col());
        self.entries.iter().find(|e| {
            let r = e.table.range;
            e.sheet.eq_ignore_ascii_case(sheet)
                && (r.row0..=r.row1).contains(&row)
                && (r.col0..=r.col1).contains(&col)
        })
    }

    /// The cells the reference `sref` of `table` (the table the formula's cell is in when `None`) stands for,
    /// in a formula at `ctx`. A table or column that is not there reads `#REF!`
    /// and `#This Row` outside the data rows `#VALUE!`.
    pub(crate) fn resolve(
        &self,
        ctx: &TableCtx<'_>,
        table: Option<&str>,
        sref: &StructuredRef,
    ) -> Expr {
        let entry = match (table, ctx.coord) {
            (Some(name), _) => self
                .by_name
                .get(fold_name(name).as_str())
                .map(|&i| &self.entries[i]),
            (None, Some(coord)) => self.containing(ctx.sheet, coord),
            (None, None) => None,
        };
        let Some(entry) = entry else {
            // A table that is not there (deleted, or not told yet) reads as a reference to nothing, as it does
            // when the spreadsheet program deletes the table a formula names.
            return Expr::ErrorLiteral(FormulaError::Ref);
        };
        let same_sheet = entry.sheet.eq_ignore_ascii_case(ctx.sheet);
        let row = ctx
            .coord
            .filter(|_| same_sheet)
            .map(PackedCellCoordinate::row);
        let cells = resolve(
            sref,
            &entry.table,
            |name| entry.columns.get(fold_name(name).as_str()).copied(),
            row,
        );
        match cells {
            Ok(cells) => expr_of(cells, (!same_sheet).then_some(entry.sheet.as_str()))
                .unwrap_or_else(Expr::ErrorLiteral),
            Err(RefError::Ref) => Expr::ErrorLiteral(FormulaError::Ref),
            Err(RefError::Value) => Expr::ErrorLiteral(FormulaError::Value),
        }
    }

    /// The data area of the table named `folded` (a bare table name in a formula), when there is one.
    pub(crate) fn data_of(&self, folded: &str, ctx: &TableCtx<'_>) -> Option<Expr> {
        let &i = self.by_name.get(folded)?;
        let sref = StructuredRef {
            items: 0,
            columns: easyoffice_formats::Columns::All,
            at: false,
        };
        Some(self.resolve(ctx, Some(&self.entries[i].table.name), &sref))
    }
}

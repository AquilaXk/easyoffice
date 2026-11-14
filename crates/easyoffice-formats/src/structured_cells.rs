//! The structured references (PD-225) of the cells a writer is about to write: an XLSX file holds them in its
//! own spelling (`Table1[[#This Row],[Q1]]`, the table's name always there), a format that has no tables (ODS)
//! holds the cells they stand for, so a saved sheet reads back with the same values. Only text that has a `[`
//! is looked at, so a sheet without such a reference costs one byte search per formula.

use crate::sheet_table::SheetTable;
use crate::structured_ref::{
    may_hold_structured_ref, same_name, to_file_refs, to_range_refs, TableLookup,
};
use crate::{DefinedName, SheetStyling};
use std::borrow::Cow;
use std::collections::HashMap;

type Cells = [(u32, u32, String)];

/// `cells` with every formula's text mapped by `rewrite` (`None` keeps the text); borrowed when none changes.
fn map_formulas<'a>(
    cells: &'a Cells,
    mut rewrite: impl FnMut(u32, u32, &str) -> Option<String>,
) -> Cow<'a, Cells> {
    let mut out: Option<Vec<(u32, u32, String)>> = None;
    for (i, (row, col, raw)) in cells.iter().enumerate() {
        let Some(body) = raw.strip_prefix('=').filter(|b| may_hold_structured_ref(b)) else {
            continue;
        };
        if let Some(new) = rewrite(*row, *col, body) {
            out.get_or_insert_with(|| cells.to_vec())[i].2 = format!("={new}");
        }
    }
    out.map_or(Cow::Borrowed(cells), Cow::Owned)
}

fn table_at(tables: &[SheetTable], row: u32, col: u32) -> Option<&SheetTable> {
    tables.iter().find(|t| {
        let r = t.range;
        (r.row0..=r.row1).contains(&row) && (r.col0..=r.col1).contains(&col)
    })
}

/// The cells of one sheet for an XLSX file; `tables` are the tables of that sheet.
pub(crate) fn file_formulas<'a>(cells: &'a Cells, tables: &[SheetTable]) -> Cow<'a, Cells> {
    map_formulas(cells, |row, col, body| {
        to_file_refs(body, table_at(tables, row, col).map(|t| t.name.as_str()))
    })
}

/// Where each table of a workbook is: by its folded name, the sheet's position and the table.
struct Tables<'a> {
    by_name: HashMap<String, (usize, &'a SheetTable)>,
}

impl<'a> Tables<'a> {
    fn new(stylings: &'a [SheetStyling]) -> Self {
        let mut by_name = HashMap::new();
        for (sheet, styling) in stylings.iter().enumerate() {
            for table in &styling.tables {
                by_name
                    .entry(crate::fold_name(&table.name))
                    .or_insert((sheet, table));
            }
        }
        Self { by_name }
    }
}

/// `formula` (without `=`) with its structured references written as ranges; `sheet` is the position of the
/// sheet that holds it.
fn ranges_of(
    formula: &str,
    (sheet, row, col): (usize, u32, u32),
    sheet_names: &[&str],
    stylings: &[SheetStyling],
    tables: &Tables<'_>,
) -> Option<String> {
    let name_of = |i: usize| sheet_names.get(i).copied().unwrap_or_default();
    let find = |name: &str| {
        tables
            .by_name
            .get(crate::fold_name(name).as_str())
            .map(|&(i, t)| (name_of(i), t))
    };
    let own = stylings
        .get(sheet)
        .and_then(|st| table_at(&st.tables, row, col))
        .map(|t| (name_of(sheet), t));
    to_range_refs(
        formula,
        name_of(sheet),
        row,
        &TableLookup { find: &find, own },
    )
}

/// The cells of sheet `sheet` for a format that has no tables: each structured reference is the range it
/// stands for.
pub(crate) fn range_formulas<'a>(
    cells: &'a Cells,
    sheet: usize,
    sheet_names: &[&str],
    stylings: &[SheetStyling],
) -> Cow<'a, Cells> {
    if !cells
        .iter()
        .any(|(_, _, raw)| raw.starts_with('=') && may_hold_structured_ref(raw))
    {
        return Cow::Borrowed(cells);
    }
    let tables = Tables::new(stylings);
    map_formulas(cells, |row, col, body| {
        ranges_of(body, (sheet, row, col), sheet_names, stylings, &tables)
    })
}

/// The defined names for a format that has no tables: a definition is read on the sheet it is scoped to (the
/// first sheet for a workbook's name) and has no row, so `#This Row` in it is an error.
pub(crate) fn range_names<'a>(
    names: &'a [DefinedName],
    sheet_names: &[&str],
    stylings: &[SheetStyling],
) -> Cow<'a, [DefinedName]> {
    if !names.iter().any(|d| may_hold_structured_ref(&d.refers_to)) {
        return Cow::Borrowed(names);
    }
    let tables = Tables::new(stylings);
    Cow::Owned(
        names
            .iter()
            .map(|def| {
                let sheet = def
                    .scope
                    .as_deref()
                    .and_then(|s| sheet_names.iter().position(|n| same_name(n, s)))
                    .unwrap_or(0);
                let text = def.refers_to.trim_start_matches('=');
                let own = ranges_of(
                    text,
                    (sheet, u32::MAX, u32::MAX),
                    sheet_names,
                    stylings,
                    &tables,
                );
                DefinedName {
                    refers_to: own.unwrap_or_else(|| def.refers_to.clone()),
                    ..def.clone()
                }
            })
            .collect(),
    )
}

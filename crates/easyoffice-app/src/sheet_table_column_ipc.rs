//! `evaluate_sheet` with `sessionAction` `tableColumn`: the formulas that name a table are written anew after an
//! edit of the table, as the spreadsheet program does (PD-225, PD-231). No session is needed: the page sends the
//! formulas that have a bracket (or the table's name) and the tables, and applies what comes back.
//!
//! `{table, from, to, tables:{"Sheet1":[table, ...]}, formulas:[{sheet, cell, text}], names?:[...]}` ->
//! `{rewrites:[{sheet, cell, text}], names:[...]|null}`: the formulas (text with its `=`) whose text changed,
//! and the whole list of defined names when one of them changed. `op` says the edit: absent or `column` (a
//! column of `table` got the name `to`; `from` is the old one), `rename` (the table `table` is named `to`) and
//! `range` (the table `table` becomes a plain range: its references are the cells they stand for). A reference
//! that does not name the table (`[@Q1]` in a cell of the table does, one outside any table does not) stays as written.

use crate::sheet_names_json::{names_field, names_json};
use crate::sheet_table_json::tables_by_sheet;
use crate::EasyOfficeIpcRouter;
use easyoffice_formats::{
    convert_table_refs, may_hold_structured_ref, rename_column_refs, rename_table_refs,
    DefinedName, EasyOfficeError, SheetTable, StrictJsonValue, TableLookup, MAX_TABLE_TEXT_CHARS,
};
use easyoffice_sheet::PackedCellCoordinate;
use std::collections::HashMap;

type Entries = [(String, StrictJsonValue)];

/// Formulas one request may carry.
const MAX_FORMULAS: usize = 1_000_000;

fn bad(what: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(format!("표 열 이름 요청이 올바르지 않습니다: {what}"))
}

fn text_field<'a>(entries: &'a Entries, key: &str) -> Result<&'a str, EasyOfficeError> {
    StrictJsonValue::get_field(entries, key)
        .and_then(StrictJsonValue::as_str)
        .filter(|t| !t.is_empty() && t.chars().count() <= MAX_TABLE_TEXT_CHARS)
        .ok_or_else(|| bad(key))
}

/// The table of `tables` that has the cell, if any.
fn table_of(tables: &[SheetTable], coord: PackedCellCoordinate) -> Option<&SheetTable> {
    tables.iter().find(|t| {
        let r = t.range;
        (r.row0..=r.row1).contains(&coord.row()) && (r.col0..=r.col1).contains(&coord.col())
    })
}

/// What an edit of a table does to a formula's text.
enum Edit<'a> {
    Column {
        table: &'a str,
        from: &'a str,
        to: &'a str,
    },
    Rename {
        from: &'a str,
        to: &'a str,
    },
    Range {
        table: &'a SheetTable,
        sheet: &'a str,
    },
}

impl Edit<'_> {
    /// Whether a formula body may need the edit (a cheap test before a scan).
    fn may_apply(&self, body: &str) -> bool {
        match self {
            Self::Column { .. } => may_hold_structured_ref(body),
            Self::Rename { from, .. } => {
                may_hold_structured_ref(body) || body.to_lowercase().contains(&from.to_lowercase())
            }
            Self::Range { table, .. } => {
                may_hold_structured_ref(body)
                    || body.to_lowercase().contains(&table.name.to_lowercase())
            }
        }
    }

    /// The body written anew for a cell of `sheet` at the zero-based `row` that is in the table `own`, or the
    /// definition of a name (no sheet, no row, no table).
    fn rewrite(
        &self,
        body: &str,
        sheet: &str,
        row: u32,
        own: Option<&SheetTable>,
        own_sheet: &str,
    ) -> Option<String> {
        match self {
            Self::Column { table, from, to } => {
                rename_column_refs(body, own.map(|t| t.name.as_str()), table, from, to)
            }
            Self::Rename { from, to } => rename_table_refs(body, from, to),
            Self::Range {
                table,
                sheet: table_sheet,
            } => {
                let find = |name: &str| {
                    name.eq_ignore_ascii_case(&table.name)
                        .then_some((*table_sheet, *table))
                };
                let own = own
                    .filter(|t| t.name.eq_ignore_ascii_case(&table.name))
                    .map(|t| (own_sheet, t));
                let lookup = TableLookup { find: &find, own };
                convert_table_refs(body, sheet, row, &lookup, table, table_sheet)
            }
        }
    }
}

impl EasyOfficeIpcRouter {
    /// See the module header.
    pub(crate) fn table_column_rewrites(entries: &Entries) -> Result<String, EasyOfficeError> {
        // The tables of each sheet by its name without case: one lookup per formula.
        let by_sheet: Vec<(String, Vec<SheetTable>)> = tables_by_sheet(entries)?
            .into_iter()
            .map(|(name, field)| (name, field.tables.unwrap_or_default()))
            .collect();
        let sheets: HashMap<String, &Vec<SheetTable>> = by_sheet
            .iter()
            .map(|(name, tables)| (name.to_lowercase(), tables))
            .collect();
        let tables_on = |sheet: &str| {
            sheets
                .get(&sheet.to_lowercase())
                .map_or(&[][..], |tables| &tables[..])
        };
        let table = text_field(entries, "table")?;
        let op = StrictJsonValue::get_str(entries, "op").unwrap_or("column");
        let edit = match op {
            "column" => Edit::Column {
                table,
                from: text_field(entries, "from")?,
                to: text_field(entries, "to")?,
            },
            "rename" => Edit::Rename {
                from: table,
                to: text_field(entries, "to")?,
            },
            "range" => {
                let (sheet, found) = by_sheet
                    .iter()
                    .find_map(|(sheet, tables)| {
                        tables
                            .iter()
                            .find(|t| t.name.eq_ignore_ascii_case(table))
                            .map(|t| (sheet.as_str(), t))
                    })
                    .ok_or_else(|| bad("table"))?;
                Edit::Range {
                    table: found,
                    sheet,
                }
            }
            _ => return Err(bad("op")),
        };
        let formulas = StrictJsonValue::get_field(entries, "formulas")
            .and_then(StrictJsonValue::as_array)
            .ok_or_else(|| bad("formulas"))?;
        if formulas.len() > MAX_FORMULAS {
            return Err(bad("formulas"));
        }
        let mut out = String::from("{\"rewrites\":[");
        let mut first = true;
        for item in formulas {
            let fields = item.as_object().ok_or_else(|| bad("formulas"))?;
            let field = |key: &str| {
                StrictJsonValue::get_field(fields, key)
                    .and_then(StrictJsonValue::as_str)
                    .ok_or_else(|| bad(key))
            };
            let (sheet, cell, formula) = (field("sheet")?, field("cell")?, field("text")?);
            let Some(body) = formula.strip_prefix('=').filter(|b| edit.may_apply(b)) else {
                continue;
            };
            let coord = PackedCellCoordinate::parse_a1(cell)?;
            let own = table_of(tables_on(sheet), coord);
            let Some(new) = edit.rewrite(body, sheet, coord.row(), own, sheet) else {
                continue;
            };
            if !first {
                out.push(',');
            }
            first = false;
            out.push_str("{\"sheet\":\"");
            EasyOfficeIpcRouter::escape_json_into(&mut out, sheet);
            out.push_str("\",\"cell\":\"");
            EasyOfficeIpcRouter::escape_json_into(&mut out, cell);
            out.push_str("\",\"text\":\"=");
            EasyOfficeIpcRouter::escape_json_into(&mut out, &new);
            out.push_str("\"}");
        }
        out.push_str("],\"names\":");
        let mut names = names_field(entries)?;
        let renamed = rewrite_names(&mut names, &edit);
        out.push_str(&if renamed {
            names_json(&names)
        } else {
            "null".to_string()
        });
        out.push('}');
        Ok(out)
    }
}

/// Writes the edit into the definitions of `names` (they sit in no table and have no row); whether one changed.
fn rewrite_names(names: &mut [DefinedName], edit: &Edit<'_>) -> bool {
    let mut changed = false;
    for def in names {
        let body = def.refers_to.trim_start_matches('=');
        if !edit.may_apply(body) {
            continue;
        }
        if let Some(new) = edit.rewrite(body, "", u32::MAX, None, "") {
            def.refers_to = new;
            changed = true;
        }
    }
    changed
}

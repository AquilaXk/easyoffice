//! `evaluate_sheet` with `sessionAction` `tableColumn`: a column of a table got a new name, so the formulas
//! that name it are written with the new name, as the spreadsheet program does (PD-225). No session is needed:
//! the page sends the formulas that have a bracket and the tables, and applies what comes back.
//!
//! `{table, from, to, tables:{"Sheet1":[table, ...]}, formulas:[{sheet, cell, text}], names?:[...]}` ->
//! `{rewrites:[{sheet, cell, text}], names:[...]|null}`: the formulas (text with its `=`) whose text changed,
//! and the whole list of defined names when one of them changed. A reference that does not name the table
//! (`[@Q1]` in a cell of the table does, one outside any table does not) stays as written.

use crate::sheet_names_json::{names_field, names_json};
use crate::sheet_table_json::tables_by_sheet;
use crate::EasyOfficeIpcRouter;
use easyoffice_formats::{
    may_hold_structured_ref, rename_column_refs, DefinedName, EasyOfficeError, SheetTable,
    StrictJsonValue, MAX_TABLE_TEXT_CHARS,
};
use easyoffice_sheet::PackedCellCoordinate;

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

/// The name of the table of `tables` that has the cell, if any.
fn table_of(tables: &[SheetTable], coord: PackedCellCoordinate) -> Option<&str> {
    tables
        .iter()
        .find(|t| {
            let r = t.range;
            (r.row0..=r.row1).contains(&coord.row()) && (r.col0..=r.col1).contains(&coord.col())
        })
        .map(|t| t.name.as_str())
}

impl EasyOfficeIpcRouter {
    /// See the module header.
    pub(crate) fn table_column_rewrites(entries: &Entries) -> Result<String, EasyOfficeError> {
        let (table, from, to) = (
            text_field(entries, "table")?,
            text_field(entries, "from")?,
            text_field(entries, "to")?,
        );
        let sheets: Vec<(String, Vec<SheetTable>)> = tables_by_sheet(entries)?
            .into_iter()
            .map(|(name, field)| (name, field.tables.unwrap_or_default()))
            .collect();
        let tables_on = |sheet: &str| {
            sheets
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(sheet))
                .map_or(&[][..], |(_, tables)| &tables[..])
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
            let Some(body) = formula
                .strip_prefix('=')
                .filter(|b| may_hold_structured_ref(b))
            else {
                continue;
            };
            let coord = PackedCellCoordinate::parse_a1(cell)?;
            let own = table_of(tables_on(sheet), coord);
            let Some(new) = rename_column_refs(body, own, table, from, to) else {
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
        let renamed = rename_in_names(&mut names, table, from, to);
        out.push_str(&if renamed {
            names_json(&names)
        } else {
            "null".to_string()
        });
        out.push('}');
        Ok(out)
    }
}

/// Renames the column in the definitions of `names` (they sit in no table); whether one changed.
fn rename_in_names(names: &mut [DefinedName], table: &str, from: &str, to: &str) -> bool {
    let mut changed = false;
    for def in names {
        if let Some(new) = rename_column_refs(&def.refers_to, None, table, from, to) {
            def.refers_to = new;
            changed = true;
        }
    }
    changed
}

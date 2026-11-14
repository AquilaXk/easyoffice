//! `evaluate_sheet` with `sessionAction` `merges` and `tables`: what an array may not be placed in or cross, and
//! the tables structured references are resolved against (PD-222, PD-224, PD-225).

use crate::sheet_session::{
    recalc_answer, registry, session_id, Applied, Registry, EXPIRED, REFUSED,
};
use crate::sheet_table_json::TablesField;
use crate::EasyOfficeIpcRouter;
use easyoffice_formats::{CellBox, EasyOfficeError, StrictJsonValue};
use easyoffice_sheet::RecalcError;
use std::time::Instant;

/// What `set_blockers` is told: the merged ranges of a sheet, or its tables.
pub(crate) enum Blockers {
    Merges(Vec<CellBox>),
    Tables(TablesField),
}

impl Registry {
    /// Tells the session the merged ranges of `sheet` (the tables of it with `Blockers::Tables`) and recalculates
    /// the arrays they concern (PD-222, PD-224). The tables of a request that sends them whole also give the
    /// engine what structured references need, and the formulas that name a table that changed are calculated
    /// again (PD-225).
    pub(crate) fn set_blockers(
        &self,
        id: &str,
        owner: &str,
        sheet: &str,
        blockers: Blockers,
        now: Instant,
    ) -> Result<Applied, EasyOfficeError> {
        let Some(handle) = self.checkout(id, owner, now) else {
            return Ok(Applied::Expired);
        };
        let Ok(mut session) = handle.lock() else {
            self.forget(id);
            return Ok(Applied::Expired);
        };
        let set = match blockers {
            Blockers::Merges(boxes) => session.set_sheet_merges(sheet, boxes),
            Blockers::Tables(TablesField {
                tables: Some(tables),
                ..
            }) => session.set_sheet_table_defs(sheet, tables),
            Blockers::Tables(TablesField {
                boxes,
                tables: None,
            }) => session.set_sheet_tables(sheet, boxes),
        };
        match set {
            Ok(()) => {}
            Err(RecalcError::Invalid(err)) => return Err(err),
            Err(RecalcError::OverBudget) => return Ok(Applied::Refused),
        }
        let outcome = session.recalculate();
        let json = recalc_answer(id, &mut session, &[], &outcome);
        Ok(Applied::Done(json))
    }
}

impl EasyOfficeIpcRouter {
    /// `merges` `{session, sheet, merges}`: `merges` is `["A1:B2", ...]`; the arrays do not cross them (PD-222).
    /// `tables` `{session, sheet, tables}`: `tables` is `["A1:C5", ...]`, the ranges of the sheet's tables, which
    /// an array may not be placed in or cross (PD-224), or the tables themselves (`sheet_table_json`), which also
    /// give the structured references their table names and columns (PD-225).
    pub(crate) fn sheet_session_blockers(
        entries: &[(String, StrictJsonValue)],
        owner: &str,
        tables: bool,
    ) -> Result<String, EasyOfficeError> {
        let id = session_id(entries)?;
        let sheet = Self::optional_json_str(entries, "sheet")?.unwrap_or("");
        let blockers =
            match StrictJsonValue::get_field(entries, if tables { "tables" } else { "merges" }) {
                None | Some(StrictJsonValue::Null) if tables => Blockers::Tables(TablesField {
                    boxes: Vec::new(),
                    tables: Some(Vec::new()),
                }),
                None | Some(StrictJsonValue::Null) => Blockers::Merges(Vec::new()),
                Some(v) if tables => {
                    Blockers::Tables(crate::sheet_table_json::parse_tables_field(v)?)
                }
                Some(v) => Blockers::Merges(crate::sheet_layout_json::parse_merges(v)?),
            };
        match registry().set_blockers(id, owner, sheet, blockers, Instant::now())? {
            Applied::Done(json) => Ok(json),
            Applied::Expired => Ok(EXPIRED.to_string()),
            Applied::Refused => Ok(REFUSED.to_string()),
        }
    }
}

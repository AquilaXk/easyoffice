//! `evaluate_sheet` with `sessionAction` `sheetRefs`: the formulas a sheet rename or deletion
//! rewrites (PD-209).
//!
//! Request: `{session, op: "rename"|"delete", sheet, newName, rules}` (`newName` for a rename) and
//! the rules `{sheet: {cf, dv}}` of every sheet, whose operand formulas may name the sheet too.
//! The engine checks the name (`sheet_name_problem`, no other sheet has it) and that a deletion
//! leaves a sheet. It does not change the session: the page applies the answer and its sheet set
//! changes, so it opens a new session.
//!
//! Answer: `formulas` (`[{sheet, cell, value}]`, the new text of every formula that named the
//! sheet, the sheet under the name it has before the change) and `rules` (the rules whose operand
//! formulas changed, by sheet name before the change). `{"refused":"structure"}` means the change
//! rewrites too many formulas and nothing changed; `{"expired":true}` and `{"refused":"limit"}`
//! are as for `apply`.

use crate::sheet_rules_json::{parse_rules, rules_json};
use crate::sheet_session::{session_id, Applied, EXPIRED, REFUSED};
use crate::EasyOfficeIpcRouter;
use easyoffice_formats::{EasyOfficeError, SheetRules, StrictJsonValue};
use easyoffice_sheet::{follow_sheet_change, SheetChange};
use std::fmt::Write;
use std::time::Instant;

type Entries = [(String, StrictJsonValue)];

/// What the request asks for, with the names owned by the parsed request.
pub(crate) enum SheetOp {
    Rename { from: String, to: String },
    Delete { name: String },
}

impl SheetOp {
    pub(crate) fn change(&self) -> SheetChange<'_> {
        match self {
            Self::Rename { from, to } => SheetChange::Rename { from, to },
            Self::Delete { name } => SheetChange::Delete { name },
        }
    }

    pub(crate) fn sheet(&self) -> &str {
        match self {
            Self::Rename { from, .. } => from,
            Self::Delete { name } => name,
        }
    }
}

fn invalid(what: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(format!("시트 요청이 올바르지 않습니다: {what}"))
}

fn parse_op(entries: &Entries) -> Result<SheetOp, EasyOfficeError> {
    let sheet = EasyOfficeIpcRouter::optional_json_str(entries, "sheet")?
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid("sheet"))?
        .to_string();
    match EasyOfficeIpcRouter::optional_json_str(entries, "op")? {
        Some("rename") => {
            let to = EasyOfficeIpcRouter::optional_json_str(entries, "newName")?
                .ok_or_else(|| invalid("newName"))?
                .to_string();
            Ok(SheetOp::Rename { from: sheet, to })
        }
        Some("delete") => Ok(SheetOp::Delete { name: sheet }),
        _ => Err(invalid("op")),
    }
}

/// The rules of the request's sheets whose operand formulas the change rewrites, as
/// `,"rules":{sheet: rules}`.
fn followed_rules_json(
    entries: &Entries,
    op: &SheetOp,
    out: &mut String,
) -> Result<(), EasyOfficeError> {
    let Some(rules) = StrictJsonValue::get_field(entries, "rules") else {
        return Ok(());
    };
    let rules = rules.as_object().ok_or_else(|| invalid("rules"))?;
    let mut changed: Vec<(&str, SheetRules)> = Vec::new();
    for (name, value) in rules {
        let mut parsed = parse_rules(value)?;
        if follow_sheet_change(&mut parsed, op.change()) {
            changed.push((name, parsed));
        }
    }
    out.push_str(",\"rules\":{");
    for (i, (name, followed)) in changed.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "\"{}\":{}",
            EasyOfficeIpcRouter::escape_json_str(name),
            rules_json(followed)
        );
    }
    out.push('}');
    Ok(())
}

impl EasyOfficeIpcRouter {
    pub(crate) fn sheet_session_sheet_refs(
        entries: &Entries,
        owner: &str,
    ) -> Result<String, EasyOfficeError> {
        let id = session_id(entries)?;
        let op = parse_op(entries)?;
        // The page's rules are read first, so a bad request fails before the session is asked.
        let mut rules = String::new();
        followed_rules_json(entries, &op, &mut rules)?;
        match crate::sheet_session::registry().sheet_refs(id, owner, &op, Instant::now())? {
            Applied::Done(mut json) => {
                if !json.contains("\"refused\"") {
                    json.pop();
                    json.push_str(&rules);
                    json.push('}');
                }
                Ok(json)
            }
            Applied::Expired => Ok(EXPIRED.to_string()),
            Applied::Refused => Ok(REFUSED.to_string()),
        }
    }
}

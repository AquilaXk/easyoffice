//! `evaluate_sheet` with `sessionAction` `names` and `nameCheck`: the defined names of a workbook (PD-214).
//!
//! - `names` `{session, names:[{name, scope?, refersTo, comment?, hidden?}]}` replaces the names of the
//!   session, compiles the formulas that use a changed name again and answers like `apply`: the formula
//!   cells whose value changed, `circular`, `cf`, plus `nameIssues` (`[{index, problem}]`) for the names
//!   that cannot be used (`name`, `reserved`, `duplicate`, `scope`, `formula`, `comment`, `cycle`, `size`).
//!   The page sends the whole list; the engine answers `{"refused":"limit"}` when the names make the
//!   session outgrow its memory bound.
//! - `nameValues` `{session}` -> `{values:[text]}`: the value of every name of the session, in the order of the list
//!   the page sent, as the 이름 관리 list shows it (a cell's value, a range as `{a,b;c,d}`).
//! - `nameCheck` `{sheets:[names], names:[...], candidate:{...}, replace?:{name, scope?}}` -> `{problem}`
//!   (`null` when the candidate can be defined): the first rule a new or changed name breaks, judged with
//!   the names the page holds (no session needed). `problem` also carries `detail` for a name that breaks
//!   the syntax. With `candidates` (up to 10,000) instead of `candidate` it answers `{problems:[null|code]}`, one
//!   per candidate, judged together (the 선택 영역에서 이름 만들기 dialog).

use crate::sheet_names_json::{issue_code, issues_json, names_field, names_json, parse_name};
use crate::sheet_session::{recalc_answer, session_id, Applied, Registry, EXPIRED, REFUSED};
use crate::EasyOfficeIpcRouter;
use easyoffice_formats::{
    defined_name_problem, DefinedName, EasyOfficeError, NameProblem, StrictJsonValue,
};
use easyoffice_sheet::{RecalcError, WorkbookEngine};
use std::time::Instant;

type Entries = [(String, StrictJsonValue)];

fn detail(problem: NameProblem) -> &'static str {
    match problem {
        NameProblem::Empty => "empty",
        NameProblem::TooLong => "tooLong",
        NameProblem::BadStart(_) => "badStart",
        NameProblem::BadChar(_) => "badChar",
        NameProblem::CellAddress => "cellAddress",
        NameProblem::RowColumnAddress => "rowColumnAddress",
        NameProblem::Boolean => "boolean",
    }
}

impl Registry {
    /// Replaces the names of the session and recomputes what uses a changed name (see the module header).
    pub(crate) fn set_names(
        &self,
        id: &str,
        owner: &str,
        defs: &[DefinedName],
        now: Instant,
    ) -> Result<Applied, EasyOfficeError> {
        let Some(handle) = self.checkout(id, owner, now) else {
            return Ok(Applied::Expired);
        };
        let Ok(mut session) = handle.lock() else {
            self.forget(id);
            return Ok(Applied::Expired);
        };
        let issues = match session.set_names_lenient(defs) {
            Ok(issues) => issues,
            Err(RecalcError::Invalid(err)) => return Err(err),
            Err(RecalcError::OverBudget) => {
                drop(session);
                self.forget(id);
                return Ok(Applied::Refused);
            }
        };
        let outcome = session.recalculate();
        let mut json = recalc_answer(id, &mut session, &[], &outcome);
        let after = session.footprint();
        drop(session);
        if !self.fits(id, after) {
            self.forget(id);
            return Ok(Applied::Refused);
        }
        self.record_footprint(id, after);
        if !issues.is_empty() {
            json.pop();
            json.push_str(",\"nameIssues\":");
            json.push_str(&issues_json(&issues));
            json.push('}');
        }
        Ok(Applied::Done(json))
    }
}

impl Registry {
    /// The value text of each name of the session (see the module header).
    pub(crate) fn name_values(
        &self,
        id: &str,
        owner: &str,
        now: Instant,
    ) -> Result<Applied, EasyOfficeError> {
        let Some(handle) = self.checkout(id, owner, now) else {
            return Ok(Applied::Expired);
        };
        let Ok(session) = handle.lock() else {
            self.forget(id);
            return Ok(Applied::Expired);
        };
        let wb = session.workbook();
        let mut out = format!("{{\"session\":\"{id}\",\"values\":[");
        for (i, def) in wb.names().iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push('"');
            EasyOfficeIpcRouter::escape_json_into(&mut out, &wb.name_value_text(def));
            out.push('"');
        }
        out.push_str("]}");
        Ok(Applied::Done(out))
    }
}

impl EasyOfficeIpcRouter {
    pub(crate) fn sheet_session_name_values(
        entries: &Entries,
        owner: &str,
    ) -> Result<String, EasyOfficeError> {
        match crate::sheet_session::registry().name_values(
            session_id(entries)?,
            owner,
            Instant::now(),
        )? {
            Applied::Done(json) => Ok(json),
            Applied::Expired => Ok(EXPIRED.to_string()),
            Applied::Refused => Ok(REFUSED.to_string()),
        }
    }

    pub(crate) fn sheet_session_names(
        entries: &Entries,
        owner: &str,
    ) -> Result<String, EasyOfficeError> {
        let id = session_id(entries)?;
        let defs = names_field(entries)?;
        match crate::sheet_session::registry().set_names(id, owner, &defs, Instant::now())? {
            Applied::Done(json) => Ok(json),
            Applied::Expired => Ok(EXPIRED.to_string()),
            Applied::Refused => Ok(REFUSED.to_string()),
        }
    }

    pub(crate) fn sheet_name_check(entries: &Entries) -> Result<String, EasyOfficeError> {
        if let Some(list) = StrictJsonValue::get_field(entries, "candidates") {
            return Self::sheet_name_check_many(entries, list);
        }
        let candidate = parse_name(StrictJsonValue::get_field(entries, "candidate").ok_or_else(
            || EasyOfficeError::InvalidArgument("JSON 'candidate' 필드가 필요합니다.".to_string()),
        )?)?;
        let replace = match StrictJsonValue::get_field(entries, "replace") {
            Some(v) if !matches!(v, StrictJsonValue::Null) => Some(parse_name(v)?),
            _ => None,
        };
        let wb = Self::name_check_workbook(entries)?;
        let issue = wb.check_name(
            &candidate,
            replace
                .as_ref()
                .map(|r| (r.name.as_str(), r.scope.as_deref())),
        );
        Ok(match issue {
            None => "{\"problem\":null}".to_string(),
            Some(issue) => {
                let syntax = defined_name_problem(&candidate.name)
                    .map_or(String::new(), |p| format!(",\"detail\":\"{}\"", detail(p)));
                format!("{{\"problem\":\"{}\"{syntax}}}", issue_code(issue))
            }
        })
    }

    fn sheet_name_check_many(
        entries: &Entries,
        list: &StrictJsonValue,
    ) -> Result<String, EasyOfficeError> {
        let candidates = crate::sheet_names_json::parse_names(list)?;
        let wb = Self::name_check_workbook(entries)?;
        let mut out = String::from("{\"problems\":[");
        for (i, issue) in wb.check_names(&candidates).into_iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            match issue {
                None => out.push_str("null"),
                Some(issue) => out.push_str(&format!("\"{}\"", issue_code(issue))),
            }
        }
        out.push_str("]}");
        Ok(out)
    }

    /// A workbook of the request's sheets and names, to judge new names against.
    fn name_check_workbook(entries: &Entries) -> Result<WorkbookEngine, EasyOfficeError> {
        let mut wb = WorkbookEngine::new();
        for sheet in StrictJsonValue::get_field(entries, "sheets")
            .and_then(StrictJsonValue::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(StrictJsonValue::as_str)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
        {
            wb.add_sheet(sheet);
        }
        wb.set_names_lenient(&names_field(entries)?);
        Ok(wb)
    }

    /// The names of the session as `{"names":[...]}` fields for an answer.
    pub(crate) fn names_answer_field(names: &[DefinedName]) -> String {
        format!(",\"names\":{}", names_json(names))
    }
}

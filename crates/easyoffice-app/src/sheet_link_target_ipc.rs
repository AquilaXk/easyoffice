//! The address a `HYPERLINK` formula cell opens (PD-223): `evaluate_sheet` with `sessionAction:"linkTarget"`.
//! The command set stays at 15.
//!
//! `{sheet, cell, formula}` plus either `session` (the workbook the session holds) or the cells of a stateless
//! `evaluate_sheet` request (`cells`, `sheets`, `names`, ...). The engine evaluates the first argument of the
//! call on the sheet of the cell, under the rules of the function itself (an address the editors would open, or a
//! `#` jump inside the workbook), and answers `{"target":"https://..."}`, `{"error":"#VALUE!"}` when the function
//! gives an error, or `{"none":true}` when the formula is not a `HYPERLINK` call. The page checks the address
//! with its own policy again, and the shell once more when it opens it.

use crate::sheet_session::{registry, session_id, Applied, EXPIRED, REFUSED};
use crate::EasyOfficeIpcRouter;
use easyoffice_formats::{EasyOfficeError, StrictJsonValue};
use easyoffice_sheet::{EvaluatedCellValue, FormulaError, PackedCellCoordinate, WorkbookEngine};
use std::time::Instant;

type Entries = [(String, StrictJsonValue)];

/// Characters of the formula a request may carry (the longest formula a cell holds).
const MAX_FORMULA_CHARS: usize = 8_192;

fn answer(found: Option<Result<String, FormulaError>>) -> String {
    match found {
        None => "{\"none\":true}".to_string(),
        Some(Err(error)) => format!(
            "{{\"error\":\"{}\"}}",
            EvaluatedCellValue::Error(error).to_display_string()
        ),
        Some(Ok(target)) => {
            let mut out = String::from("{\"target\":\"");
            EasyOfficeIpcRouter::escape_json_into(&mut out, &target);
            out.push_str("\"}");
            out
        }
    }
}

impl EasyOfficeIpcRouter {
    pub(crate) fn sheet_link_target(
        entries: &Entries,
        owner: &str,
    ) -> Result<String, EasyOfficeError> {
        let sheet = Self::optional_json_str(entries, "sheet")?.unwrap_or("");
        let formula = Self::optional_json_str(entries, "formula")?.unwrap_or("");
        if formula.chars().count() > MAX_FORMULA_CHARS {
            return Err(EasyOfficeError::InvalidArgument(
                "하이퍼링크 수식이 너무 깁니다.".to_string(),
            ));
        }
        let cell = PackedCellCoordinate::parse_a1(
            Self::optional_json_str(entries, "cell")?
                .unwrap_or("")
                .trim(),
        )?;
        if StrictJsonValue::get_field(entries, "session").is_some() {
            return match registry().link_target(
                session_id(entries)?,
                owner,
                sheet,
                cell,
                formula,
                Instant::now(),
            )? {
                Applied::Done(json) => Ok(json),
                Applied::Expired => Ok(EXPIRED.to_string()),
                Applied::Refused => Ok(REFUSED.to_string()),
            };
        }
        let default_sheet = Self::resolve_default_sheet_from_entries(entries)?;
        let wb = Self::build_workbook_from_json_entries(entries, &default_sheet)?;
        Ok(answer(wb.hyperlink_address_at(
            Self::json_sheet_name_or_default(sheet, &default_sheet),
            Some(cell),
            formula,
        )))
    }
}

impl crate::sheet_session::Registry {
    pub(crate) fn link_target(
        &self,
        id: &str,
        owner: &str,
        sheet: &str,
        cell: PackedCellCoordinate,
        formula: &str,
        now: Instant,
    ) -> Result<Applied, EasyOfficeError> {
        let Some(handle) = self.checkout(id, owner, now) else {
            return Ok(Applied::Expired);
        };
        let Ok(session) = handle.lock() else {
            self.forget(id);
            return Ok(Applied::Expired);
        };
        let wb: &WorkbookEngine = session.workbook();
        Ok(Applied::Done(answer(wb.hyperlink_address_at(
            sheet,
            Some(cell),
            formula,
        ))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(body: &str) -> Result<String, EasyOfficeError> {
        EasyOfficeIpcRouter::dispatch("evaluate_sheet", body)
    }

    fn open(cells: &[(&str, &str)]) -> String {
        let res = call("{\"sessionAction\":\"open\",\"sheets\":[\"S\",\"두 번째\"]}").unwrap();
        let start = res.find("\"session\":\"").unwrap() + 11;
        let id = res[start..start + res[start..].find('"').unwrap()].to_string();
        let changes: Vec<String> = cells
            .iter()
            .map(|(cell, value)| {
                format!(
                    "{{\"sheet\":\"S\",\"cell\":\"{cell}\",\"value\":\"{}\"}}",
                    EasyOfficeIpcRouter::escape_json_str(value)
                )
            })
            .collect();
        call(&format!(
            "{{\"sessionAction\":\"apply\",\"session\":\"{id}\",\"changes\":[{}]}}",
            changes.join(",")
        ))
        .unwrap();
        id
    }

    fn in_session(id: &str, formula: &str) -> String {
        call(&format!(
            "{{\"sessionAction\":\"linkTarget\",\"session\":\"{id}\",\"sheet\":\"S\",\"cell\":\"C1\",\"formula\":\"{}\"}}",
            EasyOfficeIpcRouter::escape_json_str(formula)
        ))
        .unwrap()
    }

    fn stateless(formula: &str) -> String {
        call(&format!(
            "{{\"sessionAction\":\"linkTarget\",\"sheet\":\"S\",\"sheets\":[\"S\"],\"cell\":\"C1\",\"cells\":[{{\"sheet\":\"S\",\"cell\":\"A1\",\"value\":\"https://example.com/a\"}},{{\"sheet\":\"S\",\"cell\":\"A2\",\"value\":\"=\\\"https://x.y/\\\"&\\\"b\\\"\"}}],\"formula\":\"{}\"}}",
            EasyOfficeIpcRouter::escape_json_str(formula)
        ))
        .unwrap()
    }

    #[test]
    fn a_session_answers_the_evaluated_address() {
        let id = open(&[
            ("A1", "https://example.com/a"),
            ("A2", "=\"https://x.y/\"&\"b\""),
        ]);
        assert_eq!(
            in_session(&id, "=HYPERLINK(A1,\"열기\")"),
            "{\"target\":\"https://example.com/a\"}"
        );
        assert_eq!(
            in_session(&id, "=HYPERLINK(A2)"),
            "{\"target\":\"https://x.y/b\"}"
        );
        assert_eq!(
            in_session(&id, "=HYPERLINK(\"#S!B2\")"),
            "{\"target\":\"#S!B2\"}"
        );
        assert_eq!(in_session(&id, "=1+1"), "{\"none\":true}");
        assert_eq!(
            in_session(&id, "=HYPERLINK(\"javascript:alert(1)\")"),
            "{\"error\":\"#VALUE!\"}"
        );
        assert_eq!(
            in_session(&id, "=HYPERLINK(1/0)"),
            "{\"error\":\"#DIV/0!\"}"
        );
    }

    #[test]
    fn a_session_another_window_opened_or_closed_does_not_answer() {
        let id = open(&[]);
        let other = EasyOfficeIpcRouter::dispatch_for_owner(
            "evaluate_sheet",
            &format!(
                "{{\"sessionAction\":\"linkTarget\",\"session\":\"{id}\",\"sheet\":\"S\",\"cell\":\"C1\",\"formula\":\"=HYPERLINK(\\\"https://a.b/\\\")\"}}"
            ),
            "another-window",
        )
        .unwrap();
        assert_eq!(other, EXPIRED);
        call(&format!(
            "{{\"sessionAction\":\"close\",\"session\":\"{id}\"}}"
        ))
        .unwrap();
        assert_eq!(in_session(&id, "=HYPERLINK(\"https://a.b/\")"), EXPIRED);
    }

    #[test]
    fn without_a_session_the_request_carries_the_cells() {
        assert_eq!(
            stateless("=HYPERLINK(A1)"),
            "{\"target\":\"https://example.com/a\"}"
        );
        assert_eq!(
            stateless("=HYPERLINK(A2,\"x\")"),
            "{\"target\":\"https://x.y/b\"}"
        );
        assert_eq!(stateless("=A1"), "{\"none\":true}");
        assert_eq!(
            stateless("=HYPERLINK(\"file:///etc/passwd\")"),
            "{\"error\":\"#VALUE!\"}"
        );
    }

    #[test]
    fn a_bad_request_is_refused() {
        let long = format!("={}", "A".repeat(MAX_FORMULA_CHARS + 1));
        for body in [
            format!(
                "{{\"sessionAction\":\"linkTarget\",\"sheet\":\"S\",\"cell\":\"C1\",\"formula\":\"{long}\"}}"
            ),
            "{\"sessionAction\":\"linkTarget\",\"sheet\":\"S\",\"cell\":\"not a cell\",\"formula\":\"=1\"}"
                .to_string(),
            "{\"sessionAction\":\"linkTarget\",\"session\":\"\",\"sheet\":\"S\",\"cell\":\"C1\",\"formula\":\"=1\"}"
                .to_string(),
        ] {
            assert!(call(&body).is_err(), "{body}");
        }
    }
}

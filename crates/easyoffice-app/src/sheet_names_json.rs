//! JSON form of the defined names (PD-214) exchanged with the page: the names a file carries when it is
//! opened, the names the page sends to a sheet session and when it saves.
//!
//! `[{"name","scope","refersTo","comment","hidden"}]`: `scope` is the sheet's name (absent for the whole
//! workbook), `refersTo` a formula without the leading `=`, `comment` and `hidden` are optional. Every
//! limit of `easyoffice_formats::sheet_names` is enforced on input; an input over a limit is refused, not
//! trimmed.

use easyoffice_formats::json_out::{push_json, Items};
use easyoffice_formats::{
    defined_name_problem, fold_name, DefinedName, EasyOfficeError, StrictJsonValue, MAX_NAMES,
    MAX_NAME_CHARS, MAX_NAME_COMMENT_CHARS, MAX_REFERS_TO_CHARS,
};
use easyoffice_sheet::NameIssue;
use serde::Serialize;

type Fields = [(String, StrictJsonValue)];

fn bad(what: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(format!("이름 JSON이 올바르지 않습니다: {what}"))
}

fn text<'a>(f: &'a Fields, key: &str, max: usize) -> Result<Option<&'a str>, EasyOfficeError> {
    match StrictJsonValue::get_field(f, key) {
        None | Some(StrictJsonValue::Null) => Ok(None),
        Some(StrictJsonValue::Str(s)) if s.chars().count() <= max => Ok(Some(s)),
        Some(_) => Err(bad(key)),
    }
}

/// One name object.
pub(crate) fn parse_name(v: &StrictJsonValue) -> Result<DefinedName, EasyOfficeError> {
    let f = v.as_object().ok_or_else(|| bad("name"))?;
    Ok(DefinedName {
        name: text(f, "name", MAX_NAME_CHARS)?.unwrap_or("").to_string(),
        scope: text(f, "scope", 1_024)?
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        refers_to: text(f, "refersTo", MAX_REFERS_TO_CHARS)?
            .unwrap_or("")
            .to_string(),
        comment: text(f, "comment", MAX_NAME_COMMENT_CHARS)?
            .unwrap_or("")
            .to_string(),
        hidden: StrictJsonValue::get_field(f, "hidden")
            .and_then(StrictJsonValue::as_bool)
            .unwrap_or(false),
    })
}

/// The list of names a session is given (`null` or absent is an empty list). A name that breaks the rules is kept and
/// reported by the engine (`nameIssues`).
pub(crate) fn parse_names(v: &StrictJsonValue) -> Result<Vec<DefinedName>, EasyOfficeError> {
    if matches!(v, StrictJsonValue::Null) {
        return Ok(Vec::new());
    }
    let items = v.as_array().ok_or_else(|| bad("names"))?;
    if items.len() > MAX_NAMES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "이름은 최대 {MAX_NAMES}개까지 정의할 수 있습니다."
        )));
    }
    items.iter().map(parse_name).collect()
}

/// A list of new names to be judged by the engine (`nameCheck`): the rules are the engine's to apply.
pub(crate) fn parse_candidates(v: &StrictJsonValue) -> Result<Vec<DefinedName>, EasyOfficeError> {
    parse_names(v)
}

/// The names of a save: every name must follow the rules of a name and no two may be the same (the same name
/// without regard to case, in the same scope), or the save is refused.
pub(crate) fn parse_names_for_save(
    v: &StrictJsonValue,
) -> Result<Vec<DefinedName>, EasyOfficeError> {
    let names = parse_names(v)?;
    let mut seen = std::collections::HashSet::with_capacity(names.len());
    for def in &names {
        if defined_name_problem(&def.name).is_some() {
            return Err(bad(&format!("이름 규칙에 맞지 않음: {}", def.name)));
        }
        if !seen.insert((
            fold_name(&def.name),
            def.scope.as_deref().map(str::to_lowercase),
        )) {
            return Err(bad(&format!("이름이 겹침: {}", def.name)));
        }
    }
    Ok(names)
}

/// The `names` field of a request, empty when it has none.
pub(crate) fn names_field(entries: &Fields) -> Result<Vec<DefinedName>, EasyOfficeError> {
    StrictJsonValue::get_field(entries, "names").map_or_else(|| Ok(Vec::new()), parse_names)
}

/// One definition as the page reads it; `scope`, `comment` and `hidden` are left out at their defaults.
#[derive(Serialize)]
struct NameOut<'a> {
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<&'a str>,
    #[serde(rename = "refersTo")]
    refers_to: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    comment: &'a str,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    hidden: bool,
}

impl<'a> NameOut<'a> {
    fn new(def: &'a DefinedName) -> Self {
        Self {
            name: &def.name,
            scope: def.scope.as_deref(),
            refers_to: &def.refers_to,
            comment: &def.comment,
            hidden: def.hidden,
        }
    }
}

/// `[...]`.
pub(crate) fn names_json(names: &[DefinedName]) -> String {
    let mut out = String::with_capacity(2 + names.len() * 48);
    push_json(&mut out, &Items(names.iter().map(NameOut::new)));
    out
}

/// The code the page shows a message for.
pub(crate) fn issue_code(issue: NameIssue) -> &'static str {
    match issue {
        NameIssue::BadName => "name",
        NameIssue::Reserved => "reserved",
        NameIssue::Duplicate => "duplicate",
        NameIssue::UnknownScope => "scope",
        NameIssue::BadFormula => "formula",
        NameIssue::BadComment => "comment",
        NameIssue::Cycle => "cycle",
        NameIssue::TooLarge => "size",
    }
}

#[derive(Serialize)]
struct IssueOut {
    index: usize,
    problem: &'static str,
}

/// `[{"index","problem"}]` for the definitions with a problem.
pub(crate) fn issues_json(issues: &[(usize, NameIssue)]) -> String {
    let mut out = String::new();
    let items = issues.iter().map(|&(index, issue)| IssueOut {
        index,
        problem: issue_code(issue),
    });
    push_json(&mut out, &Items(items));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Result<Vec<DefinedName>, EasyOfficeError> {
        let value = StrictJsonValue::parse(json).unwrap();
        parse_names(&value)
    }

    #[test]
    fn names_round_trip_through_json() {
        let names = vec![
            DefinedName {
                name: "Prices".into(),
                scope: None,
                refers_to: "Sheet1!$A$1:$A$3".into(),
                ..DefinedName::default()
            },
            DefinedName {
                name: "세율".into(),
                scope: Some("내 \"시트\"".into()),
                refers_to: "0.1".into(),
                comment: "a\nb".into(),
                hidden: true,
            },
        ];
        assert_eq!(parse(&names_json(&names)).unwrap(), names);
    }

    #[test]
    fn the_answer_keeps_its_key_order_and_leaves_defaults_out() {
        let names = vec![
            DefinedName {
                name: "Prices".into(),
                refers_to: "Sheet1!$A$1:$A$3".into(),
                ..DefinedName::default()
            },
            DefinedName {
                name: "세율".into(),
                scope: Some("내 \"시트\"".into()),
                refers_to: "0.1".into(),
                comment: "a\nb".into(),
                hidden: true,
            },
        ];
        assert_eq!(
            names_json(&names),
            r#"[{"name":"Prices","refersTo":"Sheet1!$A$1:$A$3"},{"name":"세율","scope":"내 \"시트\"","refersTo":"0.1","comment":"a\nb","hidden":true}]"#
        );
        assert_eq!(names_json(&[]), "[]");
        assert_eq!(
            issues_json(&[(0, NameIssue::BadName), (3, NameIssue::Cycle)]),
            r#"[{"index":0,"problem":"name"},{"index":3,"problem":"cycle"}]"#
        );
    }

    #[test]
    fn a_list_or_a_field_over_its_limit_is_refused() {
        let many = format!(
            "[{}]",
            vec!["{\"name\":\"a\",\"refersTo\":\"1\"}"; MAX_NAMES + 1].join(",")
        );
        assert!(matches!(
            parse(&many),
            Err(EasyOfficeError::ResourceBudgetExceeded(_))
        ));
        let long = format!(
            "[{{\"name\":\"{}\",\"refersTo\":\"1\"}}]",
            "a".repeat(MAX_NAME_CHARS + 1)
        );
        assert!(parse(&long).is_err());
        assert!(parse("[1]").is_err());
        assert!(parse("{}").is_err());
        assert_eq!(parse("null").unwrap(), Vec::new());
    }
}

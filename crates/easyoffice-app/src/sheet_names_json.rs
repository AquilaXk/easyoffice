//! JSON form of the defined names (PD-214) exchanged with the page: the names a file carries when it is
//! opened, the names the page sends to a sheet session and when it saves.
//!
//! `[{"name","scope","refersTo","comment","hidden"}]`: `scope` is the sheet's name (absent for the whole
//! workbook), `refersTo` a formula without the leading `=`, `comment` and `hidden` are optional. Every
//! limit of `easyoffice_formats::sheet_names` is enforced on input; an input over a limit is refused, not
//! trimmed.

use crate::EasyOfficeIpcRouter;
use easyoffice_formats::{
    DefinedName, EasyOfficeError, StrictJsonValue, MAX_NAMES, MAX_NAME_CHARS,
    MAX_NAME_COMMENT_CHARS, MAX_REFERS_TO_CHARS,
};
use easyoffice_sheet::NameIssue;
use std::fmt::Write;

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

/// The list of names (`null` or absent is an empty list).
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

/// The `names` field of a request, empty when it has none.
pub(crate) fn names_field(entries: &Fields) -> Result<Vec<DefinedName>, EasyOfficeError> {
    StrictJsonValue::get_field(entries, "names").map_or_else(|| Ok(Vec::new()), parse_names)
}

pub(crate) fn write_name(out: &mut String, def: &DefinedName) {
    out.push_str("{\"name\":\"");
    EasyOfficeIpcRouter::escape_json_into(out, &def.name);
    if let Some(scope) = &def.scope {
        out.push_str("\",\"scope\":\"");
        EasyOfficeIpcRouter::escape_json_into(out, scope);
    }
    out.push_str("\",\"refersTo\":\"");
    EasyOfficeIpcRouter::escape_json_into(out, &def.refers_to);
    out.push('"');
    if !def.comment.is_empty() {
        out.push_str(",\"comment\":\"");
        EasyOfficeIpcRouter::escape_json_into(out, &def.comment);
        out.push('"');
    }
    if def.hidden {
        out.push_str(",\"hidden\":true");
    }
    out.push('}');
}

/// `[...]`.
pub(crate) fn names_json(names: &[DefinedName]) -> String {
    let mut out = String::with_capacity(2 + names.len() * 48);
    out.push('[');
    for (i, def) in names.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write_name(&mut out, def);
    }
    out.push(']');
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

/// `[{"index","problem"}]` for the definitions with a problem.
pub(crate) fn issues_json(issues: &[(usize, NameIssue)]) -> String {
    let mut out = String::from("[");
    for (i, &(index, issue)) in issues.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"index\":{index},\"problem\":\"{}\"}}",
            issue_code(issue)
        );
    }
    out.push(']');
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

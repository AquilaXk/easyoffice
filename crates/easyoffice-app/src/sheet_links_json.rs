//! JSON form of the cell hyperlinks (PD-218) exchanged with the page: the links a file carries when it
//! is opened, the links the page sends when it saves, and the in-workbook targets a row or column edit
//! or a sheet rename moves.
//!
//! A sheet's `links` are `[{"ref":"B2","target":"https://...","tip":"..."}]` (`tip` absent when
//! empty). Every address goes through the policy of `easyoffice_formats::sheet_links`; a request that
//! carries an address the policy refuses, more links than a sheet may hold or a tip over its limit is
//! refused, not trimmed. `linkTargets` of a session request is `{sheet: ["#Sheet1!B5", ...]}`, the
//! answer's `{sheet: [[old, new], ...]}` for the targets the edit changed.

use crate::EasyOfficeIpcRouter;
use easyoffice_formats::{
    link_target, CellBox, CellLink, EasyOfficeError, SheetLinks, StrictJsonValue,
    MAX_LINKS_PER_SHEET, MAX_LINK_TIP_CHARS,
};
use easyoffice_sheet::{
    follow_sheet_change_links, shift_link_targets, AxisEdit, PackedCellCoordinate, SheetChange,
};
use std::fmt::Write;

type Entries = [(String, StrictJsonValue)];

/// Sheets one request may name.
const MAX_REQUEST_SHEETS: usize = 1_024;

fn bad(what: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(format!("하이퍼링크 JSON이 올바르지 않습니다: {what}"))
}

fn over_budget() -> EasyOfficeError {
    EasyOfficeError::ResourceBudgetExceeded(format!(
        "시트 하나의 하이퍼링크는 최대 {MAX_LINKS_PER_SHEET}개까지입니다."
    ))
}

/// The links of one sheet of a save request (`null` or absent is none).
pub(crate) fn parse_links(v: &StrictJsonValue) -> Result<SheetLinks, EasyOfficeError> {
    if matches!(v, StrictJsonValue::Null) {
        return Ok(SheetLinks::new());
    }
    let items = v.as_array().ok_or_else(|| bad("links"))?;
    if items.len() > MAX_LINKS_PER_SHEET {
        return Err(over_budget());
    }
    let mut links = SheetLinks::new();
    for item in items {
        let obj = item.as_object().ok_or_else(|| bad("link"))?;
        let text = |key: &str| match StrictJsonValue::get_field(obj, key) {
            None | Some(StrictJsonValue::Null) => Ok(""),
            Some(StrictJsonValue::Str(s)) => Ok(s.as_str()),
            Some(_) => Err(bad(key)),
        };
        let at = PackedCellCoordinate::parse_a1(text("ref")?.trim()).map_err(|_| bad("ref"))?;
        let tip = text("tip")?;
        if tip.chars().count() > MAX_LINK_TIP_CHARS {
            return Err(bad("tip"));
        }
        let link = CellLink::new(text("target")?, tip)
            .filter(|link| link.tip == tip)
            .ok_or_else(|| bad("target"))?;
        links.insert((at.row(), at.col()), link);
    }
    Ok(links)
}

/// `[{"ref","target","tip"}, ...]` for the links inside the first `max_rows` rows and `max_cols`
/// columns (the editor's grid; `None` is no limit).
pub(crate) fn links_json(
    links: &SheetLinks,
    max_rows: Option<u32>,
    max_cols: Option<u32>,
) -> String {
    let mut out = String::with_capacity(links.len() * 64 + 2);
    out.push('[');
    let inside = |&&(row, col): &&(u32, u32)| {
        max_rows.is_none_or(|m| row < m) && max_cols.is_none_or(|m| col < m)
    };
    let mut first = true;
    for ((&(row, col), link), _) in links
        .iter()
        .map(|(at, link)| ((at, link), at))
        .filter(|(_, at)| inside(at))
    {
        if !std::mem::take(&mut first) {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"ref\":\"{}\",\"target\":\"",
            CellBox::cell(row, col).to_a1()
        );
        EasyOfficeIpcRouter::escape_json_into(&mut out, &link.target);
        out.push('"');
        if !link.tip.is_empty() {
            out.push_str(",\"tip\":\"");
            EasyOfficeIpcRouter::escape_json_into(&mut out, &link.tip);
            out.push('"');
        }
        out.push('}');
    }
    out.push(']');
    out
}

/// The `linkTargets` of a request as `(sheet, targets)`; a target the policy refuses is left out
/// (it has no location to move).
fn parse_link_targets(entries: &Entries) -> Result<Vec<(&str, Vec<&str>)>, EasyOfficeError> {
    let Some(v) = StrictJsonValue::get_field(entries, "linkTargets") else {
        return Ok(Vec::new());
    };
    let sheets = v.as_object().ok_or_else(|| bad("linkTargets"))?;
    if sheets.len() > MAX_REQUEST_SHEETS {
        return Err(over_budget());
    }
    sheets
        .iter()
        .map(|(name, list)| {
            let items = list.as_array().ok_or_else(|| bad("linkTargets"))?;
            if items.len() > MAX_LINKS_PER_SHEET {
                return Err(over_budget());
            }
            let targets = items
                .iter()
                .map(|t| t.as_str().ok_or_else(|| bad("linkTargets")))
                .filter(|t| {
                    t.as_ref()
                        .map_or(true, |t| t.starts_with('#') && link_target(t).is_some())
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok((name.as_str(), targets))
        })
        .collect()
}

/// `,"linkTargets":{sheet: [[old, new], ...]}` for the sheets with a changed target; nothing when the
/// request has no `linkTargets`.
fn write_changed_targets(out: &mut String, changed: &[(&str, Vec<(String, String)>)]) {
    out.push_str(",\"linkTargets\":{");
    let mut first = true;
    for (sheet, pairs) in changed.iter().filter(|(_, pairs)| !pairs.is_empty()) {
        if !std::mem::take(&mut first) {
            out.push(',');
        }
        let _ = write!(out, "\"{}\":[", EasyOfficeIpcRouter::escape_json_str(sheet));
        for (i, (old, new)) in pairs.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str("[\"");
            EasyOfficeIpcRouter::escape_json_into(out, old);
            out.push_str("\",\"");
            EasyOfficeIpcRouter::escape_json_into(out, new);
            out.push_str("\"]");
        }
        out.push(']');
    }
    out.push('}');
}

/// The targets of the request's links that `edit` of `sheet` moves, as the answer's `linkTargets`.
pub(crate) fn moved_link_targets_json(
    entries: &Entries,
    sheet: &str,
    edit: &AxisEdit,
    out: &mut String,
) -> Result<(), EasyOfficeError> {
    let requested = parse_link_targets(entries)?;
    if requested.is_empty() {
        return Ok(());
    }
    let changed: Vec<_> = requested
        .iter()
        .map(|(own, targets)| {
            (
                *own,
                shift_link_targets(targets.iter().copied(), own, sheet, edit),
            )
        })
        .collect();
    write_changed_targets(out, &changed);
    Ok(())
}

/// The targets of the request's links that `change` of a sheet rewrites, as the answer's `linkTargets`.
pub(crate) fn followed_link_targets_json(
    entries: &Entries,
    change: SheetChange<'_>,
    out: &mut String,
) -> Result<(), EasyOfficeError> {
    let requested = parse_link_targets(entries)?;
    if requested.is_empty() {
        return Ok(());
    }
    let changed: Vec<_> = requested
        .iter()
        .map(|(own, targets)| {
            (
                *own,
                follow_sheet_change_links(targets.iter().copied(), change),
            )
        })
        .collect();
    write_changed_targets(out, &changed);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Result<SheetLinks, EasyOfficeError> {
        parse_links(&StrictJsonValue::parse(json).unwrap())
    }

    #[test]
    fn links_round_trip_through_json() {
        let links = parse(
            r##"[{"ref":"B2","target":"https://example.com/?a=1&b=\"2\"","tip":"팁"},{"ref":"A1","target":"#'시트 2'!C3"}]"##,
        )
        .unwrap();
        assert_eq!(links.len(), 2);
        assert_eq!(links[&(1, 1)].tip, "팁");
        let json = links_json(&links, None, None);
        assert_eq!(parse(&json).unwrap(), links);
        assert_eq!(links_json(&SheetLinks::new(), None, None), "[]");
        assert_eq!(
            links_json(&links, Some(1), None),
            r##"[{"ref":"A1","target":"#'시트 2'!C3"}]"##
        );
        assert_eq!(
            links_json(&links, None, Some(1)),
            r##"[{"ref":"A1","target":"#'시트 2'!C3"}]"##
        );
    }

    #[test]
    fn a_request_with_an_unsafe_address_a_long_tip_or_too_many_links_is_refused() {
        for bad in [
            r#"[{"ref":"A1","target":"javascript:alert(1)"}]"#,
            r#"[{"ref":"A1","target":"file:///etc/passwd"}]"#,
            r#"[{"ref":"A1","target":"data:text/html,x"}]"#,
            r#"[{"ref":"A1","target":""}]"#,
            r#"[{"ref":"A1"}]"#,
            r#"[{"ref":"not a ref","target":"https://a.b"}]"#,
            r#"[{"ref":"A1","target":"https://a.b","tip":1}]"#,
            r#"{"ref":"A1"}"#,
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
        let long_tip = format!(
            r#"[{{"ref":"A1","target":"https://a.b","tip":"{}"}}]"#,
            "가".repeat(256)
        );
        assert!(parse(&long_tip).is_err());
        let long_url = format!(
            r#"[{{"ref":"A1","target":"https://a.b/{}"}}]"#,
            "x".repeat(2_100)
        );
        assert!(parse(&long_url).is_err());
        let many = format!(
            "[{}]",
            (0..=MAX_LINKS_PER_SHEET)
                .map(|i| format!(r#"{{"ref":"A{}","target":"https://a.b"}}"#, i % 1000 + 1))
                .collect::<Vec<_>>()
                .join(",")
        );
        assert!(matches!(
            parse(&many),
            Err(EasyOfficeError::ResourceBudgetExceeded(_))
        ));
    }

    #[test]
    fn a_row_edit_answers_the_changed_targets_per_sheet() {
        let request = StrictJsonValue::parse(
            r##"{"linkTargets":{"Sheet1":["#Sheet1!B5","https://a.b","#Total"],"Sheet2":["#Sheet1!B5","#B5"]}}"##,
        )
        .unwrap();
        let entries = request.as_object().unwrap();
        let edit = AxisEdit::new(
            easyoffice_sheet::Axis::Row,
            easyoffice_sheet::EditKind::Insert,
            0,
            1,
        )
        .unwrap();
        let mut out = String::new();
        moved_link_targets_json(entries, "Sheet1", &edit, &mut out).unwrap();
        assert_eq!(
            out,
            r##","linkTargets":{"Sheet1":[["#Sheet1!B5","#Sheet1!B6"]],"Sheet2":[["#Sheet1!B5","#Sheet1!B6"]]}"##
        );
    }

    #[test]
    fn a_rename_answers_the_changed_targets() {
        let request =
            StrictJsonValue::parse(r##"{"linkTargets":{"Sheet1":["#Old!B5","#Sheet1!A1"]}}"##)
                .unwrap();
        let mut out = String::new();
        followed_link_targets_json(
            request.as_object().unwrap(),
            SheetChange::Rename {
                from: "Old",
                to: "New",
            },
            &mut out,
        )
        .unwrap();
        assert_eq!(
            out,
            r##","linkTargets":{"Sheet1":[["#Old!B5","#New!B5"]]}"##
        );
    }
}

//! `evaluate_sheet` with `sessionAction` `structure`: inserts or deletes rows or columns of a sheet
//! in a session (PD-194).
//!
//! Request: `{session, sheet, axis: "row"|"col", op: "insert"|"delete", at, count}` (`at` is the
//! 1-based row or column the insertion goes before, or the first one deleted) plus the position
//! bound parts of the page's model, which the engine moves with the cells so the page keeps no
//! second copy of the rule: `merges`, `hiddenRows`, `hiddenCols` (see `sheet_layout_json`),
//! `widths` `[[column, px], ...]` of the edited sheet, `filter` (see `sheet_filter_json`) and `rules`
//! `{sheet: {cf, dv}}` of every sheet, and the row and column groups `outline` (`sheet_view_json`).
//!
//! Answer: the session's `formulas` (`[{sheet, cell, value}]`, the text of every formula the edit
//! rewrote, by new position), the `changed` values, `circular`, and the moved `merges`,
//! `hiddenRows`, `hiddenCols`, `filter` (`null` when the edit removed it; `filterReapply: true` when
//! it dropped a column's condition, so the page evaluates the others again), `widths` and the
//! `outline` and `rules` that changed. `{"refused":"structure"}`
//! means the edit changes too many formulas and nothing changed; `{"refused":"protected"}` means the
//! request's `protect` (the sheet's protection, `sheet_protect_json`) locks that edit (PD-216), and the
//! allowed ranges and `unlockedRows`, `unlockedCols` of the request move with the cells; `{"expired":true}` and
//! `{"refused":"limit"}` are as for `apply`.

use crate::sheet_filter_json::{filter_json, parse_filter};
use crate::sheet_layout_json::{
    merges_json, parse_hidden_cols, parse_hidden_rows, parse_merges, runs_json,
};
use crate::sheet_page_json::moved_page_json;
use crate::sheet_protect_json::{moved_protect_json, moved_runs_json, parse_protection};
use crate::sheet_rules_json::{parse_rules, rules_json};
use crate::sheet_session::{session_id, write_changed, Applied, EXPIRED, REFUSED};
use crate::sheet_view_json::{outline_json, parse_outline};
use crate::EasyOfficeIpcRouter;
use easyoffice_formats::{
    CellBox, EasyOfficeError, SheetRules, StrictJsonValue, DELETE_COLUMNS, DELETE_ROWS,
    INSERT_COLUMNS, INSERT_ROWS, MAX_COLUMN_PX, MIN_COLUMN_PX,
};
use easyoffice_sheet::{
    shift_boxes, shift_filter, shift_outline, shift_rules, shift_runs, shift_widths, Axis,
    AxisEdit, AxisEditReport, EditKind, RecalcOutcome, RecalcSession,
};
use std::collections::BTreeMap;
use std::fmt::Write;
use std::time::Instant;

type Entries = [(String, StrictJsonValue)];

fn invalid(what: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(format!("행·열 편집 요청이 올바르지 않습니다: {what}"))
}

fn parse_edit(entries: &Entries) -> Result<AxisEdit, EasyOfficeError> {
    let axis = match EasyOfficeIpcRouter::optional_json_str(entries, "axis")? {
        Some("row") => Axis::Row,
        Some("col") => Axis::Col,
        _ => return Err(invalid("axis")),
    };
    let kind = match EasyOfficeIpcRouter::optional_json_str(entries, "op")? {
        Some("insert") => EditKind::Insert,
        Some("delete") => EditKind::Delete,
        _ => return Err(invalid("op")),
    };
    let number = |key: &str| {
        StrictJsonValue::get_field(entries, key)
            .and_then(StrictJsonValue::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| invalid(key))
    };
    let at = number("at")?.checked_sub(1).ok_or_else(|| invalid("at"))?;
    AxisEdit::new(axis, kind, at, number("count")?)
}

/// The answer for an edit the sheet's protection locks.
const REFUSED_PROTECTED: &str = "{\"refused\":\"protected\"}";

/// The protection flag an edit needs (PD-216).
fn edit_flag(edit: &AxisEdit) -> u16 {
    match (edit.kind, edit.axis) {
        (EditKind::Insert, Axis::Row) => INSERT_ROWS,
        (EditKind::Insert, Axis::Col) => INSERT_COLUMNS,
        (EditKind::Delete, Axis::Row) => DELETE_ROWS,
        (EditKind::Delete, Axis::Col) => DELETE_COLUMNS,
    }
}

/// `[[column, px], ...]`, 1-based columns.
fn parse_widths(v: &StrictJsonValue) -> Result<BTreeMap<u32, u32>, EasyOfficeError> {
    let items = v
        .as_array()
        .filter(|a| a.len() <= 16_384)
        .ok_or_else(|| invalid("widths"))?;
    let mut widths = BTreeMap::new();
    for item in items {
        let pair = item
            .as_array()
            .filter(|p| p.len() == 2)
            .ok_or_else(|| invalid("widths"))?;
        let col = pair[0].as_u64().filter(|c| (1..=16_384).contains(c));
        let px = pair[1]
            .as_u64()
            .filter(|p| (u64::from(MIN_COLUMN_PX)..=u64::from(MAX_COLUMN_PX)).contains(p));
        let (Some(col), Some(px)) = (col, px) else {
            return Err(invalid("widths"));
        };
        widths.insert(col as u32 - 1, px as u32);
    }
    Ok(widths)
}

/// The layout the request carries, moved for `edit`, as the answer's JSON fields.
fn moved_layout_json(
    entries: &Entries,
    edit: &AxisEdit,
    out: &mut String,
) -> Result<(), EasyOfficeError> {
    let field = |key: &str| StrictJsonValue::get_field(entries, key);
    if let Some(v) = field("merges") {
        let moved: Vec<CellBox> = shift_boxes(&parse_merges(v)?, edit, true);
        let _ = write!(out, ",\"merges\":{}", merges_json(&moved));
    }
    if let Some(v) = field("hiddenRows") {
        let rows = parse_hidden_rows(v)?;
        let moved = if edit.axis == Axis::Row {
            shift_runs(&rows, edit)
        } else {
            rows
        };
        let _ = write!(out, ",\"hiddenRows\":{}", runs_json(&moved));
    }
    if let Some(v) = field("hiddenCols") {
        let cols = parse_hidden_cols(v)?;
        let moved = if edit.axis == Axis::Col {
            shift_runs(&cols, edit)
        } else {
            cols
        };
        let _ = write!(out, ",\"hiddenCols\":{}", runs_json(&moved));
    }
    if let Some(v) = field("page") {
        let _ = write!(out, ",\"page\":{}", moved_page_json(v, edit)?);
    }
    if let Some(v) = field("protect").filter(|v| !matches!(v, StrictJsonValue::Null)) {
        let _ = write!(out, ",\"protect\":{}", moved_protect_json(v, edit)?);
    }
    if let Some(v) = field("unlockedRows") {
        let moved = moved_runs_json(v, edit, Axis::Row, "unlockedRows")?;
        let _ = write!(out, ",\"unlockedRows\":{moved}");
    }
    if let Some(v) = field("unlockedCols") {
        let moved = moved_runs_json(v, edit, Axis::Col, "unlockedCols")?;
        let _ = write!(out, ",\"unlockedCols\":{moved}");
    }
    if let Some(v) = field("outline") {
        let moved = shift_outline(&parse_outline(v)?, edit);
        let _ = write!(out, ",\"outline\":{}", outline_json(&moved));
    }
    // `null` is the page saying the sheet has no filter.
    if let Some(v) = field("filter").filter(|v| !matches!(v, StrictJsonValue::Null)) {
        let shifted = shift_filter(&parse_filter(v)?, edit);
        match &shifted.filter {
            Some(moved) => {
                let _ = write!(out, ",\"filter\":{}", filter_json(moved));
            }
            None => out.push_str(",\"filter\":null"),
        }
        if shifted.reapply {
            out.push_str(",\"filterReapply\":true");
        }
    }
    if let Some(v) = field("widths") {
        let moved = shift_widths(&parse_widths(v)?, edit);
        out.push_str(",\"widths\":[");
        for (i, (col, px)) in moved.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let _ = write!(out, "[{},{px}]", col + 1);
        }
        out.push(']');
    }
    Ok(())
}

/// The rules of the request's sheets that the edit changes, as `,"rules":{sheet: rules}`.
fn moved_rules_json(
    entries: &Entries,
    sheet: &str,
    edit: &AxisEdit,
    out: &mut String,
) -> Result<(), EasyOfficeError> {
    let Some(rules) = StrictJsonValue::get_field(entries, "rules") else {
        return Ok(());
    };
    let rules = rules.as_object().ok_or_else(|| invalid("rules"))?;
    let mut changed: Vec<(&str, SheetRules)> = Vec::new();
    for (name, value) in rules {
        let mut parsed = parse_rules(value)?;
        if shift_rules(&mut parsed, name, sheet, edit) {
            changed.push((name, parsed));
        }
    }
    out.push_str(",\"rules\":{");
    for (i, (name, moved)) in changed.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "\"{}\":{}",
            EasyOfficeIpcRouter::escape_json_str(name),
            rules_json(moved)
        );
    }
    out.push('}');
    Ok(())
}

/// The session part of the answer, without the closing brace.
pub(crate) fn session_answer(
    id: &str,
    session: &RecalcSession,
    report: &AxisEditReport,
    outcome: &RecalcOutcome,
) -> String {
    let mut out = String::with_capacity(64 + report.rewritten.len() * 40);
    let _ = write!(out, "{{\"session\":\"{id}\",\"formulas\":[");
    for (i, &(sheet, coord)) in report.rewritten.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let name = session.sheet_name(sheet).unwrap_or_default();
        let text = session
            .workbook()
            .sheet(name)
            .and_then(|s| s.raw_cells().get(&coord))
            .map_or("", String::as_str);
        out.push_str("{\"sheet\":\"");
        EasyOfficeIpcRouter::escape_json_into(&mut out, name);
        out.push_str("\",\"cell\":\"");
        out.push_str(&coord.to_a1());
        out.push_str("\",\"value\":\"");
        EasyOfficeIpcRouter::escape_json_into(&mut out, text);
        out.push_str("\"}");
    }
    out.push_str("],\"changed\":");
    write_changed(&mut out, session, &outcome.changed);
    let _ = write!(
        out,
        ",\"circular\":{},\"removed\":{}",
        outcome.circular, report.removed_cells
    );
    out.push_str(&crate::sheet_session::timed_out_field(outcome));
    if report.names_changed {
        out.push_str(&EasyOfficeIpcRouter::names_answer_field(
            session.workbook().names(),
        ));
    }
    out.push('}');
    out
}

impl EasyOfficeIpcRouter {
    pub(crate) fn sheet_session_structure(
        entries: &Entries,
        owner: &str,
    ) -> Result<String, EasyOfficeError> {
        let id = session_id(entries)?;
        let sheet = Self::optional_json_str(entries, "sheet")?
            .filter(|s| !s.is_empty())
            .ok_or_else(|| invalid("sheet"))?;
        let edit = parse_edit(entries)?;
        if let Some(v) = StrictJsonValue::get_field(entries, "protect")
            .filter(|v| !matches!(v, StrictJsonValue::Null))
        {
            if !parse_protection(v)?.allows(edit_flag(&edit)) {
                return Ok(REFUSED_PROTECTED.to_string());
            }
        }
        // The page's own parts move first, so a bad request fails before the session changes.
        let mut moved = String::new();
        moved_layout_json(entries, &edit, &mut moved)?;
        moved_rules_json(entries, sheet, &edit, &mut moved)?;
        match crate::sheet_session::registry().structure(id, owner, sheet, edit, Instant::now())? {
            Applied::Done(mut json) => {
                if !json.contains("\"refused\"") {
                    json.pop();
                    json.push_str(&moved);
                    json.push('}');
                }
                Ok(json)
            }
            Applied::Expired => Ok(EXPIRED.to_string()),
            Applied::Refused => Ok(REFUSED.to_string()),
        }
    }
}

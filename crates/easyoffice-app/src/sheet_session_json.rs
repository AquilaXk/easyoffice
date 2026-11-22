//! JSON serialization and zero-copy RPC parsing helpers for sheet session actions.

use std::borrow::Cow;
use std::fmt::Write as _;
use std::time::Instant;

use crate::sheet_rules_json::visuals_json;
use crate::sheet_session::{applied_json, registry, REFUSED};
use crate::EasyOfficeIpcRouter;
use easyoffice_formats::{
    parse_strict_json_borrowed, CellValueKind, EasyOfficeError, StrictJsonBorrow,
};
use easyoffice_sheet::{
    ChangedCell, EvaluatedCellValue, PackedCellCoordinate, RecalcOutcome, RecalcSession,
    TypedCellChange,
};

pub(crate) fn parse_cell_kind_tag(
    tag: Option<&str>,
) -> Result<Option<CellValueKind>, EasyOfficeError> {
    tag.map(|t| {
        CellValueKind::from_tag(t).ok_or_else(|| {
            EasyOfficeError::InvalidArgument(format!("알 수 없는 셀 타입 태그입니다: '{t}'"))
        })
    })
    .transpose()
}

fn borrow_opt_str<'a, 'b>(
    entries: &'b [(Cow<'a, str>, StrictJsonBorrow<'a>)],
    key: &str,
) -> Result<Option<&'b str>, EasyOfficeError> {
    match StrictJsonBorrow::get_field(entries, key) {
        None | Some(StrictJsonBorrow::Null) => Ok(None),
        Some(StrictJsonBorrow::Str(s)) => Ok(Some(s.as_ref())),
        Some(_) => Err(EasyOfficeError::InvalidArgument(format!(
            "JSON 필드 '{key}'는 문자열이어야 합니다."
        ))),
    }
}

fn borrow_session_id<'a, 'b>(
    entries: &'b [(Cow<'a, str>, StrictJsonBorrow<'a>)],
) -> Result<&'b str, EasyOfficeError> {
    borrow_opt_str(entries, "session")?
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            EasyOfficeError::InvalidArgument("JSON 'session' 필드가 필요합니다.".to_string())
        })
}

fn parse_borrowed_changes<'a>(
    entries: Vec<(Cow<'a, str>, StrictJsonBorrow<'a>)>,
) -> Result<(String, bool, Vec<TypedCellChange>), EasyOfficeError> {
    let mut session = None;
    let mut recalc = true;
    let mut changes_val = None;
    for (k, v) in entries {
        match k.as_ref() {
            "session" => match v {
                StrictJsonBorrow::Str(s) if !s.is_empty() => session = Some(s.into_owned()),
                StrictJsonBorrow::Null | StrictJsonBorrow::Str(_) => {}
                _ => {
                    return Err(EasyOfficeError::InvalidArgument(
                        "JSON 필드 'session'는 문자열이어야 합니다.".to_string(),
                    ))
                }
            },
            "recalc" => {
                if matches!(v, StrictJsonBorrow::Bool(false)) {
                    recalc = false;
                }
            }
            "changes" => changes_val = Some(v),
            _ => {}
        }
    }
    let id = session.ok_or_else(|| {
        EasyOfficeError::InvalidArgument("JSON 'session' 필드가 필요합니다.".to_string())
    })?;
    let Some(items_val) = changes_val else {
        return Ok((id, recalc, Vec::new()));
    };
    let StrictJsonBorrow::Array(items) = items_val else {
        return Err(EasyOfficeError::InvalidArgument(
            "JSON 'changes' 필드는 배열이어야 합니다.".to_string(),
        ));
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let StrictJsonBorrow::Object(fields) = item else {
            return Err(EasyOfficeError::InvalidArgument(
                "JSON 'changes' 항목은 객체여야 합니다.".to_string(),
            ));
        };
        let mut sheet: Option<Cow<'a, str>> = None;
        let mut cell: Option<Cow<'a, str>> = None;
        let mut raw = String::new();
        let mut kind: Option<CellValueKind> = None;
        for (fk, fv) in fields {
            match fk.as_ref() {
                "sheet" => match fv {
                    StrictJsonBorrow::Null => {}
                    StrictJsonBorrow::Str(s) => sheet = Some(s),
                    _ => {
                        return Err(EasyOfficeError::InvalidArgument(
                            "JSON 필드 'sheet'는 문자열이어야 합니다.".to_string(),
                        ))
                    }
                },
                "cell" => match fv {
                    StrictJsonBorrow::Null => {}
                    StrictJsonBorrow::Str(s) => cell = Some(s),
                    _ => {
                        return Err(EasyOfficeError::InvalidArgument(
                            "JSON 필드 'cell'는 문자열이어야 합니다.".to_string(),
                        ))
                    }
                },
                "value" => {
                    raw = fv.into_scalar_string().ok_or_else(|| {
                        EasyOfficeError::InvalidArgument(
                            "JSON 셀 'value'는 문자열·숫자·불리언·null 스칼라여야 합니다."
                                .to_string(),
                        )
                    })?;
                }
                "t" => match fv {
                    StrictJsonBorrow::Null => {}
                    StrictJsonBorrow::Str(s) => kind = parse_cell_kind_tag(Some(s.as_ref()))?,
                    _ => {
                        return Err(EasyOfficeError::InvalidArgument(
                            "JSON 필드 't'는 문자열이어야 합니다.".to_string(),
                        ))
                    }
                },
                _ => {}
            }
        }
        let coord = PackedCellCoordinate::parse_a1(cell.as_deref().unwrap_or("").trim())?;
        let sheet_owned = match sheet {
            Some(s) if !s.is_empty() => s.into_owned(),
            _ => String::new(),
        };
        out.push(TypedCellChange {
            sheet: sheet_owned,
            coord,
            raw,
            kind,
        });
    }
    Ok((id, recalc, out))
}

/// Zero-copy fast path for high-volume stateful sheet RPC actions (`open`, `apply`, `close`).
pub(crate) fn try_fast_sheet_session_borrowed(
    payload: &str,
    owner: &str,
) -> Result<Option<String>, EasyOfficeError> {
    if !payload.contains("\"sessionAction\"") {
        return Ok(None);
    }
    let root = parse_strict_json_borrowed(payload)?;
    let StrictJsonBorrow::Object(entries) = root else {
        return Err(EasyOfficeError::InvalidArgument(
            "RPC JSON 페이로드 루트는 객체여야 합니다.".to_string(),
        ));
    };
    let Some(action) = borrow_opt_str(&entries, "sessionAction")? else {
        return Ok(None);
    };
    // The request calculates in the workbook's date system (PD-264).
    let _date_system = easyoffice_sheet::enter_date_system(matches!(
        StrictJsonBorrow::get_field(&entries, "date1904"),
        Some(StrictJsonBorrow::Bool(true))
    ));
    let _calc_options = crate::sheet_calc_options_json::enter_from_borrowed(&entries)?;
    match action {
        "open" => {
            let items = StrictJsonBorrow::get_field(&entries, "sheets")
                .and_then(StrictJsonBorrow::as_array)
                .ok_or_else(|| {
                    EasyOfficeError::InvalidArgument(
                        "JSON 'sheets' 필드는 문자열 배열이어야 합니다.".to_string(),
                    )
                })?;
            let sheets = items
                .iter()
                .map(|item| {
                    item.as_str()
                        .filter(|n| !n.trim().is_empty())
                        .map(ToString::to_string)
                        .ok_or_else(|| {
                            EasyOfficeError::InvalidArgument(
                                "JSON 'sheets' 항목은 비어 있지 않은 문자열이어야 합니다."
                                    .to_string(),
                            )
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            match registry().open(&sheets, owner, Instant::now()) {
                Ok(id) => Ok(Some(format!("{{\"session\":\"{id}\"}}"))),
                Err(EasyOfficeError::ResourceBudgetExceeded(_)) => Ok(Some(REFUSED.to_string())),
                Err(err) => Err(err),
            }
        }
        "apply" => {
            let (id, recalc, changes) = parse_borrowed_changes(entries)?;
            match registry().apply_typed(&id, owner, &changes, recalc, Instant::now()) {
                Ok(applied) => Ok(Some(applied_json(applied))),
                Err(EasyOfficeError::ResourceBudgetExceeded(_)) => Ok(Some(REFUSED.to_string())),
                Err(err) => Err(err),
            }
        }
        "close" => {
            let id = borrow_session_id(&entries)?;
            let closed = registry().close(id, owner);
            Ok(Some(format!("{{\"closed\":{closed}}}")))
        }
        _ => Ok(None),
    }
}

/// `[{"sheet","cell","value"}, ...]` for the formula cells of `changed`.
pub(crate) fn write_changed(out: &mut String, session: &RecalcSession, changed: &[ChangedCell]) {
    out.push('[');
    if !changed.is_empty() {
        let escaped_sheets: Vec<String> = session
            .sheet_names()
            .map(EasyOfficeIpcRouter::escape_json_str)
            .collect();
        for (i, c) in changed.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str("{\"sheet\":\"");
            if let Some(sh) = escaped_sheets.get(c.sheet) {
                out.push_str(sh);
            }
            out.push_str("\",\"cell\":\"");
            c.coord.write_a1(out);
            out.push_str("\",\"value\":\"");
            match &c.value {
                EvaluatedCellValue::Empty => {}
                EvaluatedCellValue::Boolean(b) => out.push_str(if *b { "TRUE" } else { "FALSE" }),
                EvaluatedCellValue::Error(e) => out.push_str(e.code()),
                EvaluatedCellValue::Text(s) => EasyOfficeIpcRouter::escape_json_into(out, s),
                EvaluatedCellValue::Number(_) => {
                    let num_str = c.value.to_display_string();
                    out.push_str(&num_str);
                }
            }
            if c.hyperlink {
                out.push_str("\",\"link\":true}");
            } else {
                out.push_str("\"}");
            }
        }
    }
    out.push(']');
}

/// `,"spills":[{"sheet","cell","rows","cols","blocked"}, ...]` for the arrays whose area changed in the
/// recalculation (`rows` is 0 when the formula no longer spills); nothing when none did (PD-217).
pub(crate) fn spills_field(session: &RecalcSession, outcome: &RecalcOutcome) -> String {
    if outcome.spills.is_empty() {
        return String::new();
    }
    let escaped_sheets: Vec<String> = session
        .sheet_names()
        .map(EasyOfficeIpcRouter::escape_json_str)
        .collect();
    let mut out = String::with_capacity(16 + outcome.spills.len() * 56);
    out.push_str(",\"spills\":[");
    for (i, spill) in outcome.spills.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"sheet\":\"");
        if let Some(sh) = escaped_sheets.get(spill.sheet) {
            out.push_str(sh);
        }
        out.push_str("\",\"cell\":\"");
        spill.anchor.write_a1(&mut out);
        let _ = write!(
            out,
            "\",\"rows\":{},\"cols\":{},\"blocked\":{}}}",
            spill.rows, spill.cols, spill.blocked
        );
    }
    out.push(']');
    out
}

/// `,"timedOut":true,"pending":n` when the recalculation ran out of time: `pending` formulas keep the
/// value the page shows and are computed by the next `apply` (the page sends one without changes).
pub(crate) fn timed_out_field(outcome: &RecalcOutcome) -> String {
    if outcome.timed_out {
        format!(",\"timedOut\":true,\"pending\":{}", outcome.pending)
    } else {
        String::new()
    }
}

/// The answer to an `apply` that recalculated: the changed formula cells, `circular`, the cells whose
/// conditional formatting changed and the sheets that lost their rules.
pub(crate) fn recalc_answer(
    id: &str,
    session: &mut RecalcSession,
    changes: &[TypedCellChange],
    outcome: &RecalcOutcome,
) -> String {
    let mut out = String::with_capacity(64 + outcome.changed.len() * 40);
    out.push_str(&format!("{{\"session\":\"{id}\",\"changed\":"));
    write_changed(&mut out, session, &outcome.changed);
    out.push_str(&format!(
        ",\"circular\":{},\"recomputed\":{}",
        outcome.circular, outcome.recomputed
    ));
    out.push_str(&timed_out_field(outcome));
    out.push_str(&spills_field(session, outcome));
    let rule_updates = session.rule_updates_typed(changes, outcome);
    if !rule_updates.changed.is_empty() {
        out.push_str(",\"cf\":[");
        for (i, (sheet, visuals)) in rule_updates.changed.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let name = EasyOfficeIpcRouter::escape_json_str(
                session.sheet_name(*sheet).unwrap_or_default(),
            );
            let packed = visuals_json(visuals);
            out.push_str(&format!("{{\"sheet\":\"{name}\",{}", &packed[1..]));
        }
        out.push(']');
    }
    if !rule_updates.dropped.is_empty() {
        let names: Vec<String> = rule_updates
            .dropped
            .iter()
            .map(|&i| {
                format!(
                    "\"{}\"",
                    EasyOfficeIpcRouter::escape_json_str(session.sheet_name(i).unwrap_or_default())
                )
            })
            .collect();
        out.push_str(&format!(",\"cfDropped\":[{}]", names.join(",")));
    }
    out.push('}');
    out
}

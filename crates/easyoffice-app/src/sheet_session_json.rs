//! JSON serialization helpers for sheet session recalculation answers.

use crate::sheet_rules_json::visuals_json;
use crate::EasyOfficeIpcRouter;
use easyoffice_sheet::{CellChange, ChangedCell, RecalcOutcome, RecalcSession};

/// `[{"sheet","cell","value"}, ...]` for the formula cells of `changed`.
pub(crate) fn write_changed(out: &mut String, session: &RecalcSession, changed: &[ChangedCell]) {
    out.push('[');
    for (i, c) in changed.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let sheet =
            EasyOfficeIpcRouter::escape_json_str(session.sheet_name(c.sheet).unwrap_or_default());
        let val = EasyOfficeIpcRouter::escape_json_str(&c.value.to_display_string());
        out.push_str(&format!(
            "{{\"sheet\":\"{sheet}\",\"cell\":\"{}\",\"value\":\"{val}\"}}",
            c.coord.to_a1()
        ));
    }
    out.push(']');
}

/// `,"spills":[{"sheet","cell","rows","cols","blocked"}, ...]` for the arrays whose area changed in the
/// recalculation (`rows` is 0 when the formula no longer spills); nothing when none did (PD-217).
pub(crate) fn spills_field(session: &RecalcSession, outcome: &RecalcOutcome) -> String {
    if outcome.spills.is_empty() {
        return String::new();
    }
    let mut out = String::from(",\"spills\":[");
    for (i, spill) in outcome.spills.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let sheet = EasyOfficeIpcRouter::escape_json_str(
            session.sheet_name(spill.sheet).unwrap_or_default(),
        );
        out.push_str(&format!(
            "{{\"sheet\":\"{sheet}\",\"cell\":\"{}\",\"rows\":{},\"cols\":{},\"blocked\":{}}}",
            spill.anchor.to_a1(),
            spill.rows,
            spill.cols,
            spill.blocked
        ));
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
    changes: &[CellChange],
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
    let rule_updates = session.rule_updates(changes, outcome);
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

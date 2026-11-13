//! The time one evaluation request may take (PD-215). The sheet engine stops a pass at the limit and
//! reports the cells it did not reach as having no value (never as an error). An answer the page
//! shows leaves those cells out and says `timedOut`; a file keeps its formulas without cached values
//! where a format can (XLSX, ODS and the formula records of XLS recalculate on open) and refuses the
//! save where the format would have to hold the values (CSV, PDF, XLS values, documents).

use crate::EasyOfficeIpcRouter;
use easyoffice_formats::EasyOfficeError;
use easyoffice_sheet::{
    EvaluatedCellValue, WorkbookBatchEvaluation, WorkbookEngine, WorkbookEvalTarget, MAX_EVAL_TIME,
};
use std::time::Duration;

/// The longest a save or conversion calculates the formulas it writes values for. Nobody is typing
/// into the window meanwhile and the alternative is a refused save, so it is longer than
/// [`MAX_EVAL_TIME`].
pub(crate) const MAX_SAVE_EVAL_TIME: Duration = Duration::from_secs(60);

#[cfg(test)]
thread_local! {
    static TEST_LIMIT: std::cell::Cell<Option<Duration>> = const { std::cell::Cell::new(None) };
}

/// The limit of one request that evaluates a workbook for the page.
pub(crate) fn eval_time() -> Duration {
    #[cfg(test)]
    if let Some(limit) = TEST_LIMIT.with(std::cell::Cell::get) {
        return limit;
    }
    MAX_EVAL_TIME
}

/// The limit of one save or conversion.
pub(crate) fn save_eval_time() -> Duration {
    #[cfg(test)]
    if let Some(limit) = TEST_LIMIT.with(std::cell::Cell::get) {
        return limit;
    }
    MAX_SAVE_EVAL_TIME
}

/// Sets `limit` as the limit of every request on this test thread until the guard is dropped.
#[cfg(test)]
pub(crate) struct LimitGuard;

#[cfg(test)]
impl LimitGuard {
    pub(crate) fn set(limit: Duration) -> Self {
        TEST_LIMIT.with(|cell| cell.set(Some(limit)));
        Self
    }
}

#[cfg(test)]
impl Drop for LimitGuard {
    fn drop(&mut self) {
        TEST_LIMIT.with(|cell| cell.set(None));
    }
}

/// Evaluates `requests` (`(sheet, cell or formula)`) for an answer the page shows.
pub(crate) fn evaluate(
    wb: &WorkbookEngine,
    requests: &[(&str, &str)],
    anchor: Option<easyoffice_sheet::PackedCellCoordinate>,
) -> WorkbookBatchEvaluation {
    let targets: Vec<WorkbookEvalTarget<'_>> = requests
        .iter()
        .map(|&(sheet, expr)| WorkbookEvalTarget {
            sheet,
            expr,
            anchor,
        })
        .collect();
    wb.evaluate_targets_within(&targets, eval_time())
}

/// The stateless `evaluate_sheet` answer: a target the pass did not reach is `null` (the page keeps
/// the value it shows) and left out of `values`; `timedOut` is added when any was not reached.
pub(crate) fn answer(requests: &[(&str, &str)], batch: &WorkbookBatchEvaluation) -> String {
    let esc = EasyOfficeIpcRouter::escape_json_str;
    let shown =
        |i: usize| (!batch.is_unreached(i)).then(|| esc(&batch.values[i].to_display_string()));
    let mut values = Vec::with_capacity(requests.len());
    let mut target_values = Vec::with_capacity(requests.len());
    for (i, &(sheet, expr)) in requests.iter().enumerate() {
        match shown(i) {
            Some(v) => {
                values.push(format!("\"{}!{}\":\"{v}\"", esc(sheet), esc(expr)));
                target_values.push(format!("\"{v}\""));
            }
            None => target_values.push("null".to_string()),
        }
    }
    let first = shown(0).map_or("null".to_string(), |v| format!("\"{v}\""));
    with_timed_out(
        format!(
            "{{\"query\":\"{}\",\"value\":{first},\"values\":{{{}}},\"targetValues\":[{}],\"circular\":{}{}}}",
            esc(requests[0].1),
            values.join(","),
            target_values.join(","),
            batch.circular,
            spills_field(batch)
        ),
        batch.timed_out,
    )
}

/// `,"spills":[{"sheet","cell","rows","cols","blocked","values"}, ...]`: the arrays that spill with the
/// values of their area, row by row (PD-217); nothing for a workbook without one.
fn spills_field(batch: &WorkbookBatchEvaluation) -> String {
    if batch.spills.is_empty() {
        return String::new();
    }
    let esc = EasyOfficeIpcRouter::escape_json_str;
    let areas: Vec<String> = batch
        .spills
        .iter()
        .map(|a| {
            let values: Vec<String> = a
                .values
                .iter()
                .map(|v| format!("\"{}\"", esc(&v.to_display_string())))
                .collect();
            format!(
                "{{\"sheet\":\"{}\",\"cell\":\"{}\",\"rows\":{},\"cols\":{},\"blocked\":{},\"values\":[{}]}}",
                esc(&a.sheet),
                a.anchor.to_a1(),
                a.rows,
                a.cols,
                a.blocked,
                values.join(",")
            )
        })
        .collect();
    format!(",\"spills\":[{}]", areas.join(","))
}

/// Evaluates `targets` for a file. With `complete` the save is refused when the pass did not reach
/// every target; without it an unreached target is `None` and its formula is written with no value.
pub(crate) fn evaluate_for_file(
    wb: &WorkbookEngine,
    targets: &[WorkbookEvalTarget<'_>],
    complete: bool,
) -> Result<Vec<Option<EvaluatedCellValue>>, EasyOfficeError> {
    let batch = wb.evaluate_targets_within(targets, save_eval_time());
    if complete && batch.timed_out {
        return Err(refused());
    }
    let WorkbookBatchEvaluation {
        values, unreached, ..
    } = batch;
    Ok(values
        .into_iter()
        .enumerate()
        .map(|(i, v)| (!unreached.get(i).copied().unwrap_or(false)).then_some(v))
        .collect())
}

pub(crate) fn refused() -> EasyOfficeError {
    EasyOfficeError::ResourceBudgetExceeded(
        "수식 계산이 제한 시간을 넘어 저장할 수 없습니다.".into(),
    )
}

/// `json` (an object) with `"timedOut":true` added when the pass ran out of time.
pub(crate) fn with_timed_out(mut json: String, timed_out: bool) -> String {
    if timed_out && json.ends_with('}') {
        json.pop();
        json.push_str(",\"timedOut\":true}");
    }
    json
}

//! The time one evaluation request may take (PD-215). The sheet engine stops a pass at the limit and
//! leaves the cells it did not reach as `#VALUE!`; what a request does with that depends on whether
//! the values are shown (the answer says `timedOut`) or written into a file (the request is refused).

use easyoffice_formats::EasyOfficeError;
use easyoffice_sheet::{
    WorkbookBatchEvaluation, WorkbookEngine, WorkbookEvalTarget, MAX_EVAL_TIME,
};
use std::time::Duration;

/// The limit of one request that evaluates a workbook without a session.
#[cfg(not(test))]
pub(crate) fn eval_time() -> Duration {
    MAX_EVAL_TIME
}

#[cfg(test)]
thread_local! {
    static TEST_LIMIT: std::cell::Cell<Option<Duration>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(crate) fn eval_time() -> Duration {
    TEST_LIMIT
        .with(std::cell::Cell::get)
        .unwrap_or(MAX_EVAL_TIME)
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

/// Evaluates `targets` for an answer the page shows.
pub(crate) fn evaluate(
    wb: &WorkbookEngine,
    targets: &[WorkbookEvalTarget<'_>],
) -> WorkbookBatchEvaluation {
    wb.evaluate_targets_within(targets, eval_time())
}

/// Evaluates `targets` for a file: a pass that ran out of time refuses the save instead of
/// writing `#VALUE!` where a value belongs.
pub(crate) fn evaluate_for_file(
    wb: &WorkbookEngine,
    targets: &[WorkbookEvalTarget<'_>],
) -> Result<Vec<easyoffice_sheet::EvaluatedCellValue>, EasyOfficeError> {
    let batch = evaluate(wb, targets);
    if batch.timed_out {
        return Err(refused());
    }
    Ok(batch.values)
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

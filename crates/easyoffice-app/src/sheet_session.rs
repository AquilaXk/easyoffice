//! Stateful sheet sessions: `evaluate_sheet` with `sessionAction` `open` -> `apply` -> `close`.
//!
//! The stateless `evaluate_sheet` command rebuilds the whole workbook and recomputes every formula
//! on every call, so each edit ships every cell. A session keeps the workbook and its dependency
//! graph in the process (`easyoffice_sheet::RecalcSession`); the page sends only the cells that
//! changed and receives only the formula cells whose value changed (PD-164).
//!
//! The registry is bounded: a session count, sheets per session, cells, memory (raw text plus the
//! dependency graph and compiled formulas, per session and over all sessions) and an idle
//! lifetime. When a session is gone (idle expiry, eviction by a newer session, its window closed,
//! a poisoned lock) the commands answer `{"expired":true}` instead of an error, and the page
//! reopens a session and resends its cells. A session that outgrows its graph budget is dropped
//! and answers `{"refused":"limit"}`: the page evaluates that workbook without a session. A
//! session id is only valid for the window (owner) that opened it.

use crate::EasyOfficeIpcRouter;
use easyoffice_formats::{EasyOfficeError, StrictJsonValue};
use easyoffice_sheet::{
    Budget, CellChange, PackedCellCoordinate, RecalcError, RecalcSession, WorkbookEngine,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

/// Bounds of the process-wide registry.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Limits {
    /// Open sessions; a newer one evicts the least recently used.
    pub max_sessions: usize,
    /// Sheets in one session (the engine addresses sheets with 16 bits and a workbook with many
    /// sheets only grows its sheet-id collision search).
    pub max_sheets: usize,
    /// Bytes of one sheet name.
    pub max_sheet_name_bytes: usize,
    /// Populated cells in one session.
    pub max_session_cells: usize,
    /// Reverse edges of one session's dependency graph.
    pub max_session_edges: usize,
    /// Text, graph and compiled formulas of one session (`Footprint::total_bytes`).
    pub max_session_bytes: usize,
    /// Populated cells over all sessions; older sessions are evicted to make room.
    pub max_total_cells: usize,
    /// `max_session_bytes` summed over all sessions; older sessions are evicted to make room.
    pub max_total_bytes: usize,
    /// Cell edits in one `apply`.
    pub max_changes: usize,
    /// A session unused this long is dropped.
    pub idle_ttl: Duration,
}

pub(crate) const LIMITS: Limits = Limits {
    max_sessions: 16,
    max_sheets: 1_024,
    max_sheet_name_bytes: 1_024,
    max_session_cells: 2_000_000,
    max_session_edges: 8_000_000,
    max_session_bytes: 512 << 20,
    max_total_cells: 4_000_000,
    max_total_bytes: 1 << 30,
    max_changes: 250_000,
    idle_ttl: Duration::from_secs(30 * 60),
};

impl Limits {
    fn budget(&self) -> Budget {
        Budget {
            max_edges: self.max_session_edges,
            max_graph_bytes: self.max_session_bytes,
            max_bytes: self.max_session_bytes,
        }
    }
}

struct Entry {
    session: Arc<Mutex<RecalcSession>>,
    owner: String,
    last_used: Instant,
    /// Cells and bytes the session held after its last call (counted against the totals).
    cells: usize,
    bytes: usize,
}

/// Answer to `apply`.
pub(crate) enum Applied {
    Done(String),
    /// No such session for this owner (expired, evicted, closed, or another window's).
    Expired,
    /// The session outgrew its budget and was dropped.
    Refused,
}

pub(crate) struct Registry {
    limits: Limits,
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    sessions: HashMap<String, Entry>,
    counter: u64,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Registry {
    pub(crate) fn new(limits: Limits) -> Self {
        Self {
            limits,
            inner: Mutex::new(Inner::default()),
        }
    }

    fn drop_idle(&self, inner: &mut Inner, now: Instant) {
        let ttl = self.limits.idle_ttl;
        inner
            .sessions
            .retain(|_, e| now.saturating_duration_since(e.last_used) < ttl);
    }

    /// Removes the least recently used session; false when there is none.
    fn evict_oldest(inner: &mut Inner, except: Option<&str>) -> bool {
        let oldest = inner
            .sessions
            .iter()
            .filter(|(id, _)| Some(id.as_str()) != except)
            .min_by_key(|(_, e)| e.last_used)
            .map(|(id, _)| id.clone());
        oldest.is_some_and(|id| inner.sessions.remove(&id).is_some())
    }

    pub(crate) fn open(
        &self,
        sheets: &[String],
        owner: &str,
        now: Instant,
    ) -> Result<String, EasyOfficeError> {
        if sheets.len() > self.limits.max_sheets
            || sheets
                .iter()
                .any(|name| name.len() > self.limits.max_sheet_name_bytes)
        {
            return Err(EasyOfficeError::InvalidArgument(format!(
                "시트 세션은 시트 {}개, 시트 이름 {}바이트까지 지원합니다.",
                self.limits.max_sheets, self.limits.max_sheet_name_bytes
            )));
        }
        let mut wb = WorkbookEngine::new();
        for name in sheets {
            wb.add_sheet(name);
        }
        let session = RecalcSession::with_budget(wb, self.limits.budget())
            .map_err(|err| EasyOfficeError::InvalidArgument(err.to_string()))?;
        let mut inner = lock(&self.inner);
        self.drop_idle(&mut inner, now);
        while inner.sessions.len() >= self.limits.max_sessions {
            if !Self::evict_oldest(&mut inner, None) {
                break;
            }
        }
        inner.counter += 1;
        let mut id = fresh_id(inner.counter);
        while inner.sessions.contains_key(&id) {
            inner.counter += 1;
            id = fresh_id(inner.counter);
        }
        inner.sessions.insert(
            id.clone(),
            Entry {
                session: Arc::new(Mutex::new(session)),
                owner: owner.to_string(),
                last_used: now,
                cells: 0,
                bytes: 0,
            },
        );
        Ok(id)
    }

    /// Stages `changes` and, when `recalc`, recomputes. A session another owner opened does not
    /// exist for `owner`.
    pub(crate) fn apply(
        &self,
        id: &str,
        owner: &str,
        changes: &[CellChange],
        recalc: bool,
        now: Instant,
    ) -> Result<Applied, EasyOfficeError> {
        if changes.len() > self.limits.max_changes {
            return Err(EasyOfficeError::InvalidArgument(format!(
                "한 번에 보낼 수 있는 셀 변경은 최대 {}개입니다.",
                self.limits.max_changes
            )));
        }
        let incoming: usize = changes.iter().map(|c| c.raw.len()).sum();
        let handle = {
            let mut inner = lock(&self.inner);
            self.drop_idle(&mut inner, now);
            let Some(entry) = inner.sessions.get_mut(id).filter(|e| e.owner == owner) else {
                return Ok(Applied::Expired);
            };
            entry.last_used = now;
            entry.session.clone()
        };
        // A panic inside the engine poisons the lock; the session's state can no longer be trusted.
        let Ok(mut session) = handle.lock() else {
            lock(&self.inner).sessions.remove(id);
            return Ok(Applied::Expired);
        };
        let before = session.footprint();
        // Upper bounds before anything is staged: every change adds a cell and its text.
        if before.cells.saturating_add(changes.len()) > self.limits.max_session_cells
            || before.total_bytes().saturating_add(incoming) > self.limits.max_session_bytes
            || !self.reserve_total(
                id,
                before.total_bytes().saturating_add(incoming),
                before.cells.saturating_add(changes.len()),
            )
        {
            drop(session);
            lock(&self.inner).sessions.remove(id);
            return Ok(Applied::Refused);
        }
        match session.apply(changes) {
            Ok(()) => {}
            Err(RecalcError::Invalid(err)) => return Err(err),
            Err(RecalcError::OverBudget) => {
                drop(session);
                lock(&self.inner).sessions.remove(id);
                return Ok(Applied::Refused);
            }
        }
        let json = if recalc {
            let outcome = session.recalculate();
            let mut out = String::with_capacity(64 + outcome.changed.len() * 40);
            out.push_str("{\"session\":\"");
            out.push_str(id);
            out.push_str("\",\"changed\":[");
            for (i, c) in outcome.changed.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str("{\"sheet\":\"");
                out.push_str(&EasyOfficeIpcRouter::escape_json_str(
                    session.sheet_name(c.sheet).unwrap_or_default(),
                ));
                out.push_str("\",\"cell\":\"");
                out.push_str(&c.coord.to_a1());
                out.push_str("\",\"value\":\"");
                out.push_str(&EasyOfficeIpcRouter::escape_json_str(
                    &c.value.to_display_string(),
                ));
                out.push_str("\"}");
            }
            out.push_str(&format!(
                "],\"circular\":{},\"recomputed\":{}}}",
                outcome.circular, outcome.recomputed
            ));
            out
        } else {
            format!("{{\"session\":\"{id}\",\"staged\":{}}}", changes.len())
        };
        let after = session.footprint();
        drop(session);
        if let Some(entry) = lock(&self.inner).sessions.get_mut(id) {
            entry.cells = after.cells;
            entry.bytes = after.total_bytes();
        }
        Ok(Applied::Done(json))
    }

    /// Makes room for a session of `projected_bytes` and `projected_cells` under the all-session bounds by evicting the
    /// least recently used other sessions; false when even that is not enough.
    fn reserve_total(&self, id: &str, projected_bytes: usize, projected_cells: usize) -> bool {
        let mut inner = lock(&self.inner);
        loop {
            let (cells, bytes) = inner
                .sessions
                .iter()
                .filter(|(other, _)| other.as_str() != id)
                .fold((0usize, 0usize), |(c, b), (_, e)| {
                    (c + e.cells, b + e.bytes)
                });
            if bytes.saturating_add(projected_bytes) <= self.limits.max_total_bytes
                && cells.saturating_add(projected_cells) <= self.limits.max_total_cells
            {
                return true;
            }
            if !Self::evict_oldest(&mut inner, Some(id)) {
                return false;
            }
        }
    }

    pub(crate) fn close(&self, id: &str, owner: &str) -> bool {
        let mut inner = lock(&self.inner);
        if inner.sessions.get(id).is_some_and(|e| e.owner == owner) {
            inner.sessions.remove(id);
            true
        } else {
            false
        }
    }

    pub(crate) fn close_owner(&self, owner: &str) -> usize {
        let mut inner = lock(&self.inner);
        let before = inner.sessions.len();
        inner.sessions.retain(|_, e| e.owner != owner);
        before - inner.sessions.len()
    }

    /// Test hook: the number of open sessions.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        lock(&self.inner).sessions.len()
    }
}

/// An unguessable id: 128 bits from the standard library's randomly keyed hasher (seeded from the
/// operating system), which needs no dependency. The owner check, not secrecy, is what keeps one
/// window out of another's session.
fn fresh_id(counter: u64) -> String {
    use std::hash::{BuildHasher, RandomState};
    let hi = RandomState::new().hash_one(counter);
    let lo = RandomState::new().hash_one(hi ^ counter.rotate_left(32));
    format!("ss{hi:016x}{lo:016x}")
}

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| Registry::new(LIMITS))
}

/// Closes every session opened by `owner` (a window label); returns how many.
pub fn close_sheet_sessions_of(owner: &str) -> usize {
    registry().close_owner(owner)
}

const EXPIRED: &str = "{\"expired\":true}";
const REFUSED: &str = "{\"refused\":\"limit\"}";

fn session_id(entries: &[(String, StrictJsonValue)]) -> Result<&str, EasyOfficeError> {
    EasyOfficeIpcRouter::optional_json_str(entries, "session")?
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            EasyOfficeError::InvalidArgument("JSON 'session' 필드가 필요합니다.".to_string())
        })
}

fn parse_changes(
    entries: &[(String, StrictJsonValue)],
) -> Result<Vec<CellChange>, EasyOfficeError> {
    let Some(items) = StrictJsonValue::get_field(entries, "changes") else {
        return Ok(Vec::new());
    };
    let items = items.as_array().ok_or_else(|| {
        EasyOfficeError::InvalidArgument("JSON 'changes' 필드는 배열이어야 합니다.".to_string())
    })?;
    items
        .iter()
        .map(|item| {
            let fields = item.as_object().ok_or_else(|| {
                EasyOfficeError::InvalidArgument(
                    "JSON 'changes' 항목은 객체여야 합니다.".to_string(),
                )
            })?;
            let sheet = EasyOfficeIpcRouter::optional_json_str(fields, "sheet")?.unwrap_or("");
            let cell = EasyOfficeIpcRouter::optional_json_str(fields, "cell")?.unwrap_or("");
            let raw = match StrictJsonValue::get_field(fields, "value") {
                None => String::new(),
                Some(v) => v.to_scalar_string().ok_or_else(|| {
                    EasyOfficeError::InvalidArgument(
                        "JSON 셀 'value'는 문자열·숫자·불리언·null 스칼라여야 합니다.".to_string(),
                    )
                })?,
            };
            Ok(CellChange {
                sheet: sheet.to_string(),
                coord: PackedCellCoordinate::parse_a1(cell.trim())?,
                raw,
            })
        })
        .collect()
}

impl EasyOfficeIpcRouter {
    /// Handles an `evaluate_sheet` payload that carries `sessionAction` (the command set stays at
    /// the 15 commands of the IPC contract, as for the ribbon history in PD-069):
    /// - `open` `{sheets:[names]}` -> `{session}`; the sheet set is fixed for the session.
    /// - `apply` `{session, changes:[{sheet,cell,value}], recalc?}` -> the formula cells whose value
    ///   changed; `recalc:false` only stages the edits (chunked loading of a big sheet).
    /// - `close` `{session}` -> `{closed}`.
    pub(crate) fn handle_sheet_session_action(
        action: &str,
        entries: &[(String, StrictJsonValue)],
        owner: &str,
    ) -> Result<String, EasyOfficeError> {
        match action {
            "open" => Self::sheet_session_open(entries, owner),
            "apply" => Self::sheet_session_apply(entries, owner),
            "close" => {
                let closed = registry().close(session_id(entries)?, owner);
                Ok(format!("{{\"closed\":{closed}}}"))
            }
            other => Err(EasyOfficeError::InvalidArgument(format!(
                "알 수 없는 시트 세션 동작입니다: {other}"
            ))),
        }
    }

    fn sheet_session_open(
        entries: &[(String, StrictJsonValue)],
        owner: &str,
    ) -> Result<String, EasyOfficeError> {
        let items = StrictJsonValue::get_field(entries, "sheets")
            .and_then(StrictJsonValue::as_array)
            .ok_or_else(|| {
                EasyOfficeError::InvalidArgument(
                    "JSON 'sheets' 필드는 문자열 배열이어야 합니다.".to_string(),
                )
            })?;
        let mut sheets = Vec::with_capacity(items.len());
        for item in items {
            let name = item
                .as_str()
                .filter(|n| !n.trim().is_empty())
                .ok_or_else(|| {
                    EasyOfficeError::InvalidArgument(
                        "JSON 'sheets' 항목은 비어 있지 않은 문자열이어야 합니다.".to_string(),
                    )
                })?;
            sheets.push(name.to_string());
        }
        let id = registry().open(&sheets, owner, Instant::now())?;
        Ok(format!("{{\"session\":\"{id}\"}}"))
    }

    fn sheet_session_apply(
        entries: &[(String, StrictJsonValue)],
        owner: &str,
    ) -> Result<String, EasyOfficeError> {
        let id = session_id(entries)?;
        let changes = parse_changes(entries)?;
        let recalc = !matches!(
            StrictJsonValue::get_field(entries, "recalc"),
            Some(StrictJsonValue::Bool(false))
        );
        match registry().apply(id, owner, &changes, recalc, Instant::now())? {
            Applied::Done(json) => Ok(json),
            Applied::Expired => Ok(EXPIRED.to_string()),
            Applied::Refused => Ok(REFUSED.to_string()),
        }
    }
}

#[cfg(test)]
#[path = "sheet_session_tests.rs"]
mod tests;

//! Stateful sheet sessions: `evaluate_sheet` with `sessionAction` `open` -> `apply` -> `close`.
//!
//! The stateless `evaluate_sheet` command rebuilds the whole workbook and recomputes every formula
//! on every call, so each edit ships every cell. A session keeps the workbook and its dependency
//! graph in the process (`easyoffice_sheet::RecalcSession`); the page sends only the cells that
//! changed and receives only the formula cells whose value changed (PD-163).
//!
//! The registry is bounded: a session count, per-session and total memory (cells and raw text
//! bytes) and an idle lifetime. When a session is gone (idle expiry, eviction by a newer session,
//! its window closed) the commands answer `{"expired":true}` instead of an error, and the page
//! reopens a session and resends its cells.

use crate::EasyOfficeIpcRouter;
use easyoffice_formats::{EasyOfficeError, StrictJsonValue};
use easyoffice_sheet::{CellChange, PackedCellCoordinate, RecalcSession, WorkbookEngine};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

/// Bounds of the process-wide registry.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Limits {
    /// Open sessions; a newer one evicts the least recently used.
    pub max_sessions: usize,
    /// Populated cells in one session.
    pub max_session_cells: usize,
    /// Raw cell text bytes in one session.
    pub max_session_bytes: usize,
    /// Raw cell text bytes over all sessions; older sessions are evicted to make room.
    pub max_total_bytes: usize,
    /// Cell edits in one `sheet_session_apply`.
    pub max_changes: usize,
    /// A session unused this long is dropped.
    pub idle_ttl: Duration,
}

pub(crate) const LIMITS: Limits = Limits {
    max_sessions: 16,
    max_session_cells: 2_000_000,
    max_session_bytes: 256 << 20,
    max_total_bytes: 512 << 20,
    max_changes: 250_000,
    idle_ttl: Duration::from_secs(30 * 60),
};

struct Entry {
    session: Arc<Mutex<RecalcSession>>,
    owner: String,
    last_used: Instant,
    /// Raw text bytes the session held after its last call (counted against `max_total_bytes`).
    bytes: usize,
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
        let mut wb = WorkbookEngine::new();
        for name in sheets {
            wb.add_sheet(name);
        }
        let session = RecalcSession::new(wb)?;
        let mut inner = lock(&self.inner);
        self.drop_idle(&mut inner, now);
        while inner.sessions.len() >= self.limits.max_sessions {
            if !Self::evict_oldest(&mut inner, None) {
                break;
            }
        }
        inner.counter += 1;
        let id = format!("ss{:x}-{:x}", inner.counter, process_nonce());
        inner.sessions.insert(
            id.clone(),
            Entry {
                session: Arc::new(Mutex::new(session)),
                owner: owner.to_string(),
                last_used: now,
                bytes: 0,
            },
        );
        Ok(id)
    }

    /// Stages `changes` and, when `recalc`, recomputes; returns the response JSON, or `Ok(None)`
    /// when the session no longer exists.
    pub(crate) fn apply(
        &self,
        id: &str,
        changes: &[CellChange],
        recalc: bool,
        now: Instant,
    ) -> Result<Option<String>, EasyOfficeError> {
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
            let Some(entry) = inner.sessions.get_mut(id) else {
                return Ok(None);
            };
            entry.last_used = now;
            entry.session.clone()
        };
        let mut session = lock(&handle);
        let (cells, bytes) = session.footprint();
        // Upper bound before anything is staged: every change adds a cell and its text.
        if cells + changes.len() > self.limits.max_session_cells
            || bytes + incoming > self.limits.max_session_bytes
        {
            return Err(EasyOfficeError::InvalidArgument(
                "시트 세션이 허용 크기(셀 수 또는 텍스트 양)를 넘습니다.".to_string(),
            ));
        }
        self.reserve_total(id, bytes + incoming)?;
        session.apply(changes)?;
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
        let after = session.footprint().1;
        drop(session);
        if let Some(entry) = lock(&self.inner).sessions.get_mut(id) {
            entry.bytes = after;
        }
        Ok(Some(json))
    }

    /// Makes room for `projected` bytes in session `id` under the all-session bound by evicting
    /// the least recently used other sessions.
    fn reserve_total(&self, id: &str, projected: usize) -> Result<(), EasyOfficeError> {
        let mut inner = lock(&self.inner);
        loop {
            let others: usize = inner
                .sessions
                .iter()
                .filter(|(other, _)| other.as_str() != id)
                .map(|(_, e)| e.bytes)
                .sum();
            if others + projected <= self.limits.max_total_bytes {
                return Ok(());
            }
            if !Self::evict_oldest(&mut inner, Some(id)) {
                return Err(EasyOfficeError::InvalidArgument(
                    "시트 세션의 전체 메모리 상한을 넘습니다.".to_string(),
                ));
            }
        }
    }

    pub(crate) fn close(&self, id: &str) -> bool {
        lock(&self.inner).sessions.remove(id).is_some()
    }

    pub(crate) fn close_owner(&self, owner: &str) -> usize {
        let mut inner = lock(&self.inner);
        let before = inner.sessions.len();
        inner.sessions.retain(|_, e| e.owner != owner);
        before - inner.sessions.len()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        lock(&self.inner).sessions.len()
    }
}

/// Not secret (every window of the process is equally trusted); it only keeps ids from repeating
/// across restarts of a long-lived page.
fn process_nonce() -> u64 {
    static NONCE: OnceLock<u64> = OnceLock::new();
    *NONCE.get_or_init(|| {
        use std::hash::{BuildHasher, RandomState};
        RandomState::new().hash_one(0x5E55_1011_u64) & 0xFFFF_FFFF
    })
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
            "apply" => Self::sheet_session_apply(entries),
            "close" => {
                let closed = registry().close(session_id(entries)?);
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
    ) -> Result<String, EasyOfficeError> {
        let id = session_id(entries)?;
        let changes = parse_changes(entries)?;
        let recalc = !matches!(
            StrictJsonValue::get_field(entries, "recalc"),
            Some(StrictJsonValue::Bool(false))
        );
        match registry().apply(id, &changes, recalc, Instant::now())? {
            Some(json) => Ok(json),
            None => Ok(EXPIRED.to_string()),
        }
    }
}

#[cfg(test)]
#[path = "sheet_session_tests.rs"]
mod tests;

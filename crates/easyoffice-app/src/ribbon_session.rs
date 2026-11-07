//! Ribbon formatting state and undo/redo history, isolated per client session and per module.
//!
//! Every WebUI page load sends a random `sessionId` with its ribbon IPC payloads; within a session
//! each office module (DOC/HWP/SHEET/SLIDE/PDF) owns its own `RibbonWorkspaceState` and
//! `UndoRedoStack`, so a bold toggle in DOC never leaks into SHEET and Ctrl+Z in one browser window
//! never reverts another window's command. Payloads without `sessionId` use a shared default
//! session for backward compatibility. The session map is bounded and evicts the least recently
//! used session so memory cannot grow without limit.

use crate::{EasyOfficeIpcRouter, RibbonCommandRouter, RibbonWorkspaceState, UndoRedoStack};
use easyoffice_formats::{EasyOfficeError, StrictJsonValue};
use easyoffice_layout::{OfficeModuleKind, RibbonLayoutStyle};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub(crate) const DEFAULT_RIBBON_SESSION_ID: &str = "default";
/// Upper bound of concurrently retained client sessions (LRU eviction beyond this).
pub(crate) const MAX_RIBBON_SESSIONS: usize = 32;
/// Upper bound of a client-provided session identifier length.
pub(crate) const MAX_RIBBON_SESSION_ID_LEN: usize = 64;

const MODULE_ORDER: [OfficeModuleKind; 5] = [
    OfficeModuleKind::Document,
    OfficeModuleKind::Hwp,
    OfficeModuleKind::Sheet,
    OfficeModuleKind::Slide,
    OfficeModuleKind::Pdf,
];

fn module_slot_index(module: OfficeModuleKind) -> usize {
    match module {
        OfficeModuleKind::Document => 0,
        OfficeModuleKind::Hwp => 1,
        OfficeModuleKind::Sheet => 2,
        OfficeModuleKind::Slide => 3,
        OfficeModuleKind::Pdf => 4,
    }
}

fn parse_module_key(raw: &str) -> Result<OfficeModuleKind, EasyOfficeError> {
    match raw.trim().to_ascii_uppercase().as_str() {
        "DOC" | "DOCUMENT" | "CONV" => Ok(OfficeModuleKind::Document),
        "HWP" => Ok(OfficeModuleKind::Hwp),
        "SHEET" => Ok(OfficeModuleKind::Sheet),
        "SLIDE" => Ok(OfficeModuleKind::Slide),
        "PDF" => Ok(OfficeModuleKind::Pdf),
        _ => Err(EasyOfficeError::InvalidArgument(format!(
            "지원하지 않는 리본 모듈입니다: {}",
            EasyOfficeIpcRouter::escape_json_str(raw.trim())
        ))),
    }
}

fn validate_session_id(raw: &str) -> Result<String, EasyOfficeError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(DEFAULT_RIBBON_SESSION_ID.to_string());
    }
    let well_formed = trimmed.len() <= MAX_RIBBON_SESSION_ID_LEN
        && trimmed
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if !well_formed {
        return Err(EasyOfficeError::InvalidArgument(format!(
            "리본 세션 식별자는 영문·숫자·-·_ 조합 {MAX_RIBBON_SESSION_ID_LEN}자 이하여야 합니다."
        )));
    }
    Ok(trimmed.to_string())
}

/// Extracts and validates the optional `sessionId` field (missing → default session).
pub(crate) fn session_id_from_entries(
    entries: &[(String, StrictJsonValue)],
) -> Result<String, EasyOfficeError> {
    validate_session_id(EasyOfficeIpcRouter::optional_json_str(entries, "sessionId")?.unwrap_or(""))
}

#[derive(Default)]
struct ModuleRibbonSlot {
    state: RibbonWorkspaceState,
    history: UndoRedoStack<RibbonWorkspaceState>,
}

struct ClientRibbonSession {
    ribbon_style: RibbonLayoutStyle,
    last_module: OfficeModuleKind,
    slots: [ModuleRibbonSlot; 5],
    last_used: u64,
}

impl ClientRibbonSession {
    fn new(tick: u64) -> Self {
        let slots = MODULE_ORDER.map(|module| ModuleRibbonSlot {
            state: RibbonWorkspaceState {
                active_module: module,
                ..RibbonWorkspaceState::default()
            },
            history: UndoRedoStack::new(),
        });
        Self {
            ribbon_style: RibbonLayoutStyle::StandardOffice,
            last_module: OfficeModuleKind::Document,
            slots,
            last_used: tick,
        }
    }

    fn slot_mut(&mut self, module: OfficeModuleKind) -> &mut ModuleRibbonSlot {
        let style = self.ribbon_style;
        let slot = &mut self.slots[module_slot_index(module)];
        slot.state.ribbon_style = style;
        slot
    }
}

/// Bounded LRU map of client ribbon sessions.
#[derive(Default)]
pub(crate) struct RibbonSessionRegistry {
    sessions: HashMap<String, ClientRibbonSession>,
    tick: u64,
}

impl RibbonSessionRegistry {
    fn session_mut(&mut self, session_id: &str) -> &mut ClientRibbonSession {
        self.tick += 1;
        let tick = self.tick;
        if !self.sessions.contains_key(session_id) && self.sessions.len() >= MAX_RIBBON_SESSIONS {
            let lru_key = self
                .sessions
                .iter()
                .min_by_key(|(_, s)| s.last_used)
                .map(|(k, _)| k.clone());
            if let Some(key) = lru_key {
                self.sessions.remove(&key);
            }
        }
        let session = self
            .sessions
            .entry(session_id.to_string())
            .or_insert_with(|| ClientRibbonSession::new(tick));
        session.last_used = tick;
        session
    }

    #[cfg(test)]
    pub(crate) fn session_count(&self) -> usize {
        self.sessions.len()
    }

    #[cfg(test)]
    pub(crate) fn contains_session(&self, session_id: &str) -> bool {
        self.sessions.contains_key(session_id)
    }
}

static RIBBON_SESSIONS: OnceLock<Mutex<RibbonSessionRegistry>> = OnceLock::new();

fn with_registry<R>(f: impl FnOnce(&mut RibbonSessionRegistry) -> R) -> R {
    let mut guard = RIBBON_SESSIONS
        .get_or_init(|| Mutex::new(RibbonSessionRegistry::default()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    f(&mut guard)
}

pub(crate) fn set_session_ribbon_style(session_id: &str, style: RibbonLayoutStyle) {
    with_registry(|reg| reg.session_mut(session_id).ribbon_style = style);
}

impl EasyOfficeIpcRouter {
    pub(crate) fn handle_execute_ribbon_command(payload: &str) -> Result<String, EasyOfficeError> {
        let trimmed = payload.strip_prefix('\u{FEFF}').unwrap_or(payload).trim();
        let (cmd_id, session_id) = if trimmed.starts_with('{') {
            let entries = Self::parse_rpc_json_object(trimmed)?;
            let id = match Self::optional_json_str(&entries, "commandId")? {
                Some(id) => id.trim().to_string(),
                None => Self::optional_json_str(&entries, "id")?
                    .unwrap_or("")
                    .trim()
                    .to_string(),
            };
            (id, session_id_from_entries(&entries)?)
        } else {
            (trimmed.to_string(), DEFAULT_RIBBON_SESSION_ID.to_string())
        };
        let module = RibbonCommandRouter::module_for_command(&cmd_id)?;
        with_registry(|reg| {
            let session = reg.session_mut(&session_id);
            session.last_module = module;
            let slot = session.slot_mut(module);
            let snapshot = slot.state.clone();
            let summary = RibbonCommandRouter::execute_command(&cmd_id, &mut slot.state)?;
            slot.history.push_state(snapshot);
            Ok(Self::format_ribbon_state_json(
                &slot.state,
                &summary,
                slot.history.can_undo(),
                slot.history.can_redo(),
            ))
        })
    }

    /// Undo (`is_undo`) or redo on the given module's stack of the given client session.
    /// Missing `module` targets the session's most recently executed module.
    pub(crate) fn handle_ribbon_history_step(
        payload: &str,
        is_undo: bool,
    ) -> Result<String, EasyOfficeError> {
        let trimmed = payload.strip_prefix('\u{FEFF}').unwrap_or(payload).trim();
        let (session_id, module) = if trimmed.starts_with('{') {
            let entries = Self::parse_rpc_json_object(trimmed)?;
            let module = match Self::optional_json_str(&entries, "module")? {
                Some(raw) if !raw.trim().is_empty() => Some(parse_module_key(raw)?),
                _ => None,
            };
            (session_id_from_entries(&entries)?, module)
        } else if trimmed.is_empty() {
            (DEFAULT_RIBBON_SESSION_ID.to_string(), None)
        } else {
            (
                DEFAULT_RIBBON_SESSION_ID.to_string(),
                Some(parse_module_key(trimmed)?),
            )
        };
        with_registry(|reg| {
            let session = reg.session_mut(&session_id);
            let module = module.unwrap_or(session.last_module);
            let style = session.ribbon_style;
            let slot = session.slot_mut(module);
            let current = slot.state.clone();
            let restored = if is_undo {
                slot.history.undo(current)
            } else {
                slot.history.redo(current)
            };
            if let Some(state) = restored {
                slot.state = state;
                slot.state.ribbon_style = style;
            }
            let summary = if is_undo {
                "실행 취소 완료"
            } else {
                "다시 실행 완료"
            };
            Ok(Self::format_ribbon_state_json(
                &slot.state,
                summary,
                slot.history.can_undo(),
                slot.history.can_redo(),
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ribbon_session_registry_is_bounded_and_evicts_least_recently_used() {
        let mut reg = RibbonSessionRegistry::default();
        reg.session_mut("keep-alive");
        for i in 0..MAX_RIBBON_SESSIONS - 1 {
            reg.session_mut(&format!("client-{i}"));
        }
        assert_eq!(reg.session_count(), MAX_RIBBON_SESSIONS);
        reg.session_mut("keep-alive");
        reg.session_mut("overflow-client");
        assert_eq!(reg.session_count(), MAX_RIBBON_SESSIONS);
        assert!(reg.contains_session("keep-alive"));
        assert!(reg.contains_session("overflow-client"));
        assert!(!reg.contains_session("client-0"));
    }

    fn exec(session: &str, cmd: &str) -> String {
        EasyOfficeIpcRouter::dispatch(
            "execute_ribbon_command",
            &format!(r#"{{"commandId":"{cmd}","sessionId":"{session}"}}"#),
        )
        .unwrap()
    }

    fn history(command: &str, session: &str, module: Option<&str>) -> String {
        let payload = match module {
            Some(m) => format!(r#"{{"sessionId":"{session}","module":"{m}"}}"#),
            None => format!(r#"{{"sessionId":"{session}"}}"#),
        };
        EasyOfficeIpcRouter::dispatch(command, &payload).unwrap()
    }

    #[test]
    fn test_issue_106_ribbon_state_does_not_leak_across_modules_and_undo_targets_module() {
        let s = "issue106-repro";
        let first = exec(s, "document-home-bold");
        assert!(first.contains("\"activeModule\":\"DOC\""), "{first}");
        assert!(first.contains("\"isBold\":true"), "{first}");

        let second = exec(s, "sheet-home-italic");
        assert!(second.contains("\"activeModule\":\"SHEET\""), "{second}");
        assert!(second.contains("\"isItalic\":true"), "{second}");
        assert!(
            second.contains("\"isBold\":false"),
            "DOC bold leaked into SHEET: {second}"
        );

        // Undo without an explicit module targets the session's most recently executed module.
        let undone = history("undo_ribbon_command", s, None);
        assert!(undone.contains("\"activeModule\":\"SHEET\""), "{undone}");
        assert!(undone.contains("\"isItalic\":false"), "{undone}");
        assert!(
            !undone.contains("document-home-bold"),
            "undo hit DOC: {undone}"
        );
        assert!(undone.contains("\"canUndo\":false"), "{undone}");

        let doc_undone = history("undo_ribbon_command", s, Some("DOC"));
        assert!(
            doc_undone.contains("\"activeModule\":\"DOC\""),
            "{doc_undone}"
        );
        assert!(doc_undone.contains("\"isBold\":false"), "{doc_undone}");

        let doc_redone = history("redo_ribbon_command", s, Some("DOC"));
        assert!(doc_redone.contains("\"isBold\":true"), "{doc_redone}");
        let sheet_redone = history("redo_ribbon_command", s, Some("SHEET"));
        assert!(
            sheet_redone.contains("\"activeModule\":\"SHEET\""),
            "{sheet_redone}"
        );
        assert!(sheet_redone.contains("\"isItalic\":true"), "{sheet_redone}");
        assert!(sheet_redone.contains("\"isBold\":false"), "{sheet_redone}");
    }

    #[test]
    fn test_issue_106_two_client_sessions_do_not_share_ribbon_stacks() {
        let a = "issue106-window-a";
        let b = "issue106-window-b";
        let a_bold = exec(a, "document-home-bold");
        assert!(a_bold.contains("\"isBold\":true"), "{a_bold}");

        let b_italic = exec(b, "document-home-italic");
        assert!(
            b_italic.contains("\"isBold\":false"),
            "window A leaked: {b_italic}"
        );
        assert!(b_italic.contains("\"isItalic\":true"), "{b_italic}");

        // Ctrl+Z in window B must only revert B's own command.
        let b_undo = history("undo_ribbon_command", b, Some("DOC"));
        assert!(b_undo.contains("\"isItalic\":false"), "{b_undo}");
        assert!(b_undo.contains("\"isBold\":false"), "{b_undo}");
        assert!(b_undo.contains("\"canUndo\":false"), "{b_undo}");

        // Window A's stack is untouched: its own bold is still undoable.
        let a_undo = history("undo_ribbon_command", a, Some("DOC"));
        assert!(a_undo.contains("\"isBold\":false"), "{a_undo}");
        assert!(a_undo.contains("\"isItalic\":false"), "{a_undo}");
        assert!(a_undo.contains("\"canRedo\":true"), "{a_undo}");

        // Ribbon style is per client session as well.
        EasyOfficeIpcRouter::dispatch(
            "switch_ribbon_style",
            &format!(r#"{{"style":"hangul","sessionId":"{a}"}}"#),
        )
        .unwrap();
        assert!(exec(a, "hwp-home-bold").contains("\"ribbonStyle\":\"hangul\""));
        assert!(exec(b, "hwp-home-bold").contains("\"ribbonStyle\":\"standard\""));
    }

    #[test]
    fn test_ribbon_session_rejects_malformed_session_and_module_ids() {
        let bad_session = EasyOfficeIpcRouter::dispatch(
            "execute_ribbon_command",
            r#"{"commandId":"document-home-bold","sessionId":"bad id\"<>"}"#,
        );
        assert!(bad_session.is_err());
        let bad_module = EasyOfficeIpcRouter::dispatch(
            "undo_ribbon_command",
            r#"{"sessionId":"issue106-bad-module","module":"NOPE"}"#,
        );
        assert!(bad_module.is_err());
    }
}

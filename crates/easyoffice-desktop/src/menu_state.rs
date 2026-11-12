//! What the native menu bar (macOS only, PD-121) shows for each window (PD-102): the marks of the
//! theme and update-check items and the items grayed out for the window's module.
//!
//! The menu bar is one set of items shared by all windows, so the state belongs to the window that
//! has the focus: every window reports its own state to Rust ([`report_menu_state`], when its
//! theme, module or editor changes) and Rust applies the focused window's state to the bar when a
//! window reports and whenever the focus moves ([`refresh`]).
//!
//! The page owns the module rules (`studioCommandApplies`, the same rule that grays out the page's own
//! menu items), so it sends the ids of the items its module cannot use; nothing is duplicated here.
//! Every id is checked against the item table.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Manager, WebviewWindow};

use crate::native_menu::{self, KindMenus, MenuKind, MenuView, ITEMS};
use crate::shell_caller::ShellCaller;
use crate::update_check::UpdateService;
use crate::windows::is_window_label;

/// The menu state one window reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowMenu {
    kind: MenuKind,
    theme: &'static str,
    disabled_mask: u128,
}

impl Default for WindowMenu {
    fn default() -> Self {
        Self {
            kind: MenuKind::Doc,
            theme: "system",
            disabled_mask: 0,
        }
    }
}

impl WindowMenu {
    /// Checks a report: the kind and the theme must be known and every id an item of the table.
    pub fn parse(kind: &str, theme: &str, disabled: Vec<String>) -> Result<Self, String> {
        let kind =
            MenuKind::parse(kind).ok_or_else(|| "알 수 없는 메뉴 종류입니다.".to_string())?;
        if disabled.len() > ITEMS.len() {
            return Err("메뉴 항목 목록이 너무 깁니다.".to_string());
        }
        let theme = ["system", "light", "dark"]
            .into_iter()
            .find(|mode| *mode == theme)
            .ok_or_else(|| "알 수 없는 테마입니다.".to_string())?;
        let mut disabled_mask = 0u128;
        for id in &disabled {
            let Some(index) = native_menu::item_index(id.as_str()) else {
                return Err(format!("알 수 없는 메뉴 항목입니다: {id}"));
            };
            disabled_mask |= 1u128 << index;
        }
        Ok(Self {
            kind,
            theme,
            disabled_mask,
        })
    }
}

/// The reported state of every open window, by label.
#[derive(Default)]
pub struct MenuStates(Mutex<HashMap<String, WindowMenu>>);

impl MenuStates {
    fn states(&self) -> std::sync::MutexGuard<'_, HashMap<String, WindowMenu>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn get(&self, label: &str) -> WindowMenu {
        self.states().get(label).copied().unwrap_or_default()
    }

    pub fn forget(&self, label: &str) {
        self.states().remove(label);
    }
}

pub fn forget(app: &AppHandle, label: &str) {
    app.state::<MenuStates>().forget(label);
}

/// Shows the focused window's bar and state. With no focused window the bar on screen stays.
pub fn refresh(app: &AppHandle) -> Result<(), String> {
    let Some(kinds) = app.try_state::<KindMenus>() else {
        return Ok(());
    };
    let focused = app
        .webview_windows()
        .into_values()
        .find(|window| window.is_focused().unwrap_or(false));
    let reported = focused
        .as_ref()
        .map(|window| app.state::<MenuStates>().get(window.label()));
    let state = reported.unwrap_or_default();
    let kind = reported.map_or_else(|| kinds.shown(), |state| state.kind);
    let menu = kinds
        .show(app, kind)
        .map_err(|err| format!("메뉴 막대를 바꾸지 못했습니다: {err}"))?;
    let update_check = app
        .try_state::<Arc<UpdateService>>()
        .map(|service| service.view())
        .filter(|view| view.supported)
        .map(|view| view.enabled);
    native_menu::sync(
        &menu,
        &MenuView {
            theme: state.theme,
            disabled_mask: state.disabled_mask,
            update_check,
        },
    )
    .map_err(|err| format!("메뉴를 갱신하지 못했습니다: {err}"))
}

/// Shows the focused window's state on the bar; a failure is only logged (the bar is a convenience
/// and never stops the window's own work).
pub fn refresh_logged(app: &AppHandle) {
    if let Err(error) = refresh(app) {
        eprintln!("[menu] {error}");
    }
}

/// A window reports its menu kind, its theme and the items its module cannot use.
#[tauri::command]
pub fn report_menu_state(
    _caller: ShellCaller,
    app: AppHandle,
    window: WebviewWindow,
    kind: String,
    theme: String,
    disabled: Vec<String>,
) -> Result<(), crate::IpcError> {
    if !is_window_label(window.label()) {
        return Ok(());
    }
    let state = WindowMenu::parse(&kind, &theme, disabled).map_err(crate::IpcError::new)?;
    app.state::<MenuStates>()
        .states()
        .insert(window.label().to_string(), state);
    refresh(&app).map_err(crate::IpcError::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    impl WindowMenu {
        pub fn is_enabled(&self, index: usize) -> bool {
            (self.disabled_mask & (1u128 << index)) == 0
        }

        pub fn is_disabled(&self, id: &str) -> bool {
            native_menu::item_index(id).is_some_and(|index| !self.is_enabled(index))
        }
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|id| (*id).to_string()).collect()
    }

    #[test]
    fn a_report_is_accepted_only_with_a_known_theme_and_known_items() {
        let ok = WindowMenu::parse("SHEET", "dark", ids(&["table:sort", "edit:undo"])).unwrap();
        assert_eq!(ok.theme, "dark");
        assert_eq!(ok.kind, MenuKind::Sheet);
        assert!(ok.is_disabled("table:sort"));
        assert!(!ok.is_disabled("file:save"));
        assert!(WindowMenu::parse("DOC", "purple", Vec::new()).is_err());
        assert!(WindowMenu::parse("DOC", "light", ids(&["app:quit"])).is_err());
        assert!(WindowMenu::parse("DOC", "light", ids(&["nope"])).is_err());
        let too_many = vec!["table:sort".to_string(); ITEMS.len() + 1];
        assert!(WindowMenu::parse("DOC", "light", too_many).is_err());
        for bad in ["", "doc", "START", "HWP_EMBED"] {
            assert!(
                WindowMenu::parse(bad, "light", Vec::new()).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_window_that_has_not_reported_shows_the_document_bar_enabled_with_the_system_theme() {
        let states = MenuStates::default();
        assert_eq!(states.get("doc-1"), WindowMenu::default());
        assert_eq!(WindowMenu::default().kind, MenuKind::Doc);
        states.states().insert(
            "doc-1".to_string(),
            WindowMenu::parse("PDF", "dark", Vec::new()).unwrap(),
        );
        assert_eq!(states.get("doc-1").kind, MenuKind::Pdf);
        assert_eq!(states.get("doc-1").theme, "dark");
        states.forget("doc-1");
        assert_eq!(states.get("doc-1").theme, "system");
    }
}

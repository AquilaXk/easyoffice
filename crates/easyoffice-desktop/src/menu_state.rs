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

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Manager, WebviewWindow};

use crate::native_menu::{self, MenuView, ITEMS};
use crate::shell_caller::ShellCaller;
use crate::update_check::UpdateService;
use crate::windows::is_window_label;

/// The menu state one window reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowMenu {
    theme: &'static str,
    disabled: HashSet<String>,
}

impl Default for WindowMenu {
    fn default() -> Self {
        Self {
            theme: "system",
            disabled: HashSet::new(),
        }
    }
}

impl WindowMenu {
    /// Checks a report: the theme must be a known mode and every id an item of the table.
    pub fn parse(theme: &str, disabled: Vec<String>) -> Result<Self, String> {
        if disabled.len() > ITEMS.len() {
            return Err("메뉴 항목 목록이 너무 깁니다.".to_string());
        }
        let theme = ["system", "light", "dark"]
            .into_iter()
            .find(|mode| *mode == theme)
            .ok_or_else(|| "알 수 없는 테마입니다.".to_string())?;
        if let Some(unknown) = disabled
            .iter()
            .find(|id| !ITEMS.iter().any(|spec| spec.id == id.as_str()))
        {
            return Err(format!("알 수 없는 메뉴 항목입니다: {unknown}"));
        }
        Ok(Self {
            theme,
            disabled: disabled.into_iter().collect(),
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
        self.states().get(label).cloned().unwrap_or_default()
    }

    pub fn forget(&self, label: &str) {
        self.states().remove(label);
    }
}

pub fn forget(app: &AppHandle, label: &str) {
    app.state::<MenuStates>().forget(label);
}

/// Shows the focused window's state on the bar.
pub fn refresh(app: &AppHandle) -> Result<(), String> {
    let Some(menu) = app.menu() else {
        return Ok(());
    };
    let focused = app
        .webview_windows()
        .into_values()
        .find(|window| window.is_focused().unwrap_or(false));
    let state = focused
        .as_ref()
        .map(|window| app.state::<MenuStates>().get(window.label()))
        .unwrap_or_default();
    let update_check = app
        .try_state::<Arc<UpdateService>>()
        .map(|service| service.view())
        .filter(|view| view.supported)
        .map(|view| view.enabled);
    native_menu::sync(
        &menu,
        &MenuView {
            theme: state.theme,
            disabled: &state.disabled,
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

/// A window reports its theme and the items its module cannot use.
#[tauri::command]
pub fn report_menu_state(
    _caller: ShellCaller,
    app: AppHandle,
    window: WebviewWindow,
    theme: String,
    disabled: Vec<String>,
) -> Result<(), crate::IpcError> {
    if !is_window_label(window.label()) {
        return Ok(());
    }
    let state = WindowMenu::parse(&theme, disabled).map_err(crate::IpcError::new)?;
    app.state::<MenuStates>()
        .states()
        .insert(window.label().to_string(), state);
    refresh(&app).map_err(crate::IpcError::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|id| (*id).to_string()).collect()
    }

    #[test]
    fn a_report_is_accepted_only_with_a_known_theme_and_known_items() {
        let ok = WindowMenu::parse("dark", ids(&["table:sort", "edit:undo"])).unwrap();
        assert_eq!(ok.theme, "dark");
        assert!(ok.disabled.contains("table:sort"));
        assert!(WindowMenu::parse("purple", Vec::new()).is_err());
        assert!(WindowMenu::parse("light", ids(&["app:quit"])).is_err());
        assert!(WindowMenu::parse("light", ids(&["nope"])).is_err());
        let too_many = vec!["table:sort".to_string(); ITEMS.len() + 1];
        assert!(WindowMenu::parse("light", too_many).is_err());
    }

    #[test]
    fn a_window_that_has_not_reported_shows_everything_enabled_with_the_system_theme() {
        let states = MenuStates::default();
        assert_eq!(states.get("doc-1"), WindowMenu::default());
        states.states().insert(
            "doc-1".to_string(),
            WindowMenu::parse("dark", Vec::new()).unwrap(),
        );
        assert_eq!(states.get("doc-1").theme, "dark");
        states.forget("doc-1");
        assert_eq!(states.get("doc-1").theme, "system");
    }
}

//! Native macOS menu bar (PD-102).
//!
//! macOS shows one application menu bar, so the app installs one: 앱 / 파일 / 편집 / 보기 / 윈도우.
//! Windows and Linux keep the menu bar the page draws. Adapted from hop
//! `apps/desktop/src-tauri/src/menu.rs` (MIT, see THIRD_PARTY_NOTICES.md): the same menu layout and
//! the same "menu id -> the focused window only" delivery, with EasyOffice's differences:
//!
//! * One static table ([`ITEMS`]) holds every custom item: its id, label, accelerator and the
//!   page action it runs. The page keeps the matching list (`NATIVE_MENU_ACTIONS` in
//!   `shell-chrome.js`); a test compares both lists, so they cannot drift apart.
//! * The page action names are the ones the keyboard shortcuts already use, so a menu item and its
//!   shortcut run the same handler (dirty prompts and the Hangul editor's shell commands included).
//! * The page is called by evaluating a fixed script with the item's static id; no event
//!   permission is added to the capability, and the id never comes from the page.
//! * Quit uses the system item, which ends in `RunEvent::ExitRequested`; the existing handler then
//!   closes every window through its own save prompt. Undo, copy, paste and the like are system
//!   items as well.
//!
//! The menu is built by plain, platform independent functions (so the Linux build checks them with
//! clippy); only the installation is limited to macOS.

// Only the installation is macOS-specific; the rest is built and tested on every platform, so the
// non-macOS builds see it as unused outside the tests.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use tauri::menu::{Menu, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::{AppHandle, Runtime};

/// One custom menu item.
pub struct MenuItemSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub accelerator: Option<&'static str>,
    /// Page action: a name of the page's shortcut action table, optionally `name:argument`.
    pub action: &'static str,
}

const fn item(
    id: &'static str,
    label: &'static str,
    accelerator: Option<&'static str>,
    action: &'static str,
) -> MenuItemSpec {
    MenuItemSpec {
        id,
        label,
        accelerator,
        action,
    }
}

/// Every custom item of the menu bar, in menu order.
pub const ITEMS: &[MenuItemSpec] = &[
    item("app:about", "EasyOffice 정보", None, "about"),
    item("file:new", "새로 만들기", Some("CmdOrCtrl+N"), "file-new"),
    item("file:open", "열기…", Some("CmdOrCtrl+O"), "file-open"),
    item(
        "file:open-recent",
        "최근 문서…",
        Some("CmdOrCtrl+Alt+O"),
        "recent-dialog",
    ),
    item("file:save", "저장", Some("CmdOrCtrl+S"), "studio-save"),
    item(
        "file:save-as",
        "다른 이름으로 저장…",
        Some("CmdOrCtrl+Shift+S"),
        "file-save-as",
    ),
    item("file:close", "닫기", Some("CmdOrCtrl+W"), "window-close"),
    item("edit:find", "찾기", Some("CmdOrCtrl+F"), "find"),
    item("view:zoom-in", "확대", Some("CmdOrCtrl+="), "zoom-in"),
    item("view:zoom-out", "축소", Some("CmdOrCtrl+-"), "zoom-out"),
    item(
        "view:zoom-100",
        "실제 크기",
        Some("CmdOrCtrl+0"),
        "ribbon:view-zoom",
    ),
];

/// Longest accepted id (a defensive bound for the generated script).
const MAX_ID_LEN: usize = 32;
const SCRIPT_PREFIX: &str = "window.__EASYOFFICE_NATIVE_MENU__&&window.__EASYOFFICE_NATIVE_MENU__(";

/// The page action of a menu id, if the id is one of ours.
pub fn action_for(id: &str) -> Option<&'static str> {
    ITEMS
        .iter()
        .find(|spec| spec.id == id)
        .map(|spec| spec.action)
}

/// The script that runs a menu item in the page. Only ids of [`ITEMS`] produce one, and the id is
/// written as a JSON string, so nothing but the static table ever reaches the page.
pub fn dispatch_script(id: &str) -> Option<String> {
    action_for(id)?;
    let quoted = serde_json::to_string(id).ok()?;
    (id.len() <= MAX_ID_LEN).then(|| format!("{SCRIPT_PREFIX}{quoted});"))
}

fn custom<R: Runtime>(
    app: &AppHandle<R>,
    spec: &MenuItemSpec,
) -> tauri::Result<tauri::menu::MenuItem<R>> {
    let builder = MenuItemBuilder::with_id(spec.id, spec.label);
    match spec.accelerator {
        Some(accelerator) => builder.accelerator(accelerator).build(app),
        None => builder.build(app),
    }
}

fn spec(id: &str) -> &'static MenuItemSpec {
    ITEMS
        .iter()
        .find(|spec| spec.id == id)
        .expect("menu id is in the table")
}

/// Builds the whole menu bar.
pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let app_menu = SubmenuBuilder::new(app, "EasyOffice")
        .item(&custom(app, spec("app:about"))?)
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;
    let file_menu = SubmenuBuilder::new(app, "파일")
        .item(&custom(app, spec("file:new"))?)
        .item(&custom(app, spec("file:open"))?)
        .item(&custom(app, spec("file:open-recent"))?)
        .separator()
        .item(&custom(app, spec("file:save"))?)
        .item(&custom(app, spec("file:save-as"))?)
        .separator()
        .item(&custom(app, spec("file:close"))?)
        .build()?;
    let edit_menu = SubmenuBuilder::new(app, "편집")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .separator()
        .item(&custom(app, spec("edit:find"))?)
        .build()?;
    let view_menu = SubmenuBuilder::new(app, "보기")
        .item(&custom(app, spec("view:zoom-in"))?)
        .item(&custom(app, spec("view:zoom-out"))?)
        .item(&custom(app, spec("view:zoom-100"))?)
        .build()?;
    let window_menu = SubmenuBuilder::new(app, "윈도우")
        .minimize()
        .item(&PredefinedMenuItem::close_window(app, None)?)
        .build()?;
    Menu::with_items(
        app,
        &[&app_menu, &file_menu, &edit_menu, &view_menu, &window_menu],
    )
}

/// Installs the menu bar and routes custom items to the focused window.
#[cfg(target_os = "macos")]
pub fn install(app: &mut tauri::App) -> tauri::Result<()> {
    let menu = build(app.handle())?;
    app.set_menu(menu)?;
    app.on_menu_event(|app, event| run_item(app, event.id().as_ref()));
    Ok(())
}

/// Runs a custom item in the window that has the focus; with no focused window nothing happens
/// (a menu click never reaches a window the user is not looking at).
#[cfg(target_os = "macos")]
fn run_item(app: &AppHandle, id: &str) {
    use tauri::Manager;
    let Some(script) = dispatch_script(id) else {
        return;
    };
    let focused = app
        .webview_windows()
        .into_values()
        .find(|window| window.is_focused().unwrap_or(false));
    if let Some(window) = focused {
        let _ = window.eval(&script);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODIFIERS: &[&str] = &["CmdOrCtrl", "Alt", "Shift"];

    #[test]
    fn ids_are_unique_and_shaped_like_group_colon_name() {
        let mut seen = std::collections::HashSet::new();
        for spec in ITEMS {
            assert!(seen.insert(spec.id), "duplicate id {}", spec.id);
            let (group, name) = spec.id.split_once(':').expect("group:name");
            assert!(
                ["app", "file", "edit", "view"].contains(&group),
                "{}",
                spec.id
            );
            assert!(
                !name.is_empty()
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            );
            assert!(spec.id.len() <= MAX_ID_LEN);
        }
    }

    #[test]
    fn accelerators_are_command_based_and_do_not_collide() {
        let mut seen = std::collections::HashSet::new();
        for spec in ITEMS {
            let Some(accelerator) = spec.accelerator else {
                continue;
            };
            let mut parts: Vec<&str> = accelerator.split('+').collect();
            let key = parts.pop().unwrap();
            assert!(parts.contains(&"CmdOrCtrl"), "{accelerator} uses Cmd");
            assert!(parts.iter().all(|p| MODIFIERS.contains(p)), "{accelerator}");
            assert!(key.len() == 1, "{accelerator} ends in one key");
            assert!(
                seen.insert(accelerator),
                "duplicate accelerator {accelerator}"
            );
        }
    }

    #[test]
    fn page_actions_are_plain_names_with_an_optional_argument() {
        for spec in ITEMS {
            let (name, argument) = spec.action.split_once(':').map_or((spec.action, ""), |p| p);
            assert!(
                name.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
                "{}",
                spec.action
            );
            assert!(
                argument
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'-'),
                "{}",
                spec.action
            );
        }
    }

    #[test]
    fn only_table_ids_produce_a_script_and_the_id_is_quoted() {
        assert_eq!(
            dispatch_script("file:save").as_deref(),
            Some(
                r#"window.__EASYOFFICE_NATIVE_MENU__&&window.__EASYOFFICE_NATIVE_MENU__("file:save");"#
            )
        );
        for bad in [
            "",
            "file:save\");alert(1);(\"",
            "file:unknown",
            "app:quit",
            "FILE:SAVE",
        ] {
            assert_eq!(dispatch_script(bad), None, "{bad}");
        }
        assert_eq!(action_for("file:close"), Some("window-close"));
        assert_eq!(action_for("nope"), None);
    }

    #[test]
    fn labels_are_korean_menu_texts_without_markup() {
        for spec in ITEMS {
            assert!(!spec.label.trim().is_empty());
            assert!(!spec.label.contains(['<', '>', '&']), "{}", spec.label);
        }
    }
}

//! Native menu bar (PD-102, PD-121): macOS only.
//!
//! The reference app installs a native menu only on macOS and draws the page's own menu row on every
//! OS (`apps/desktop/src-tauri/src/lib.rs:8-9,67-70`, `apps/studio-host/src/main.ts:154`). So does
//! EasyOffice: Windows and Linux have no native bar (the page's menu row is their menu), and macOS has
//! this bar beside the page's row. Every item of the page's menu row is in [`ITEMS`] (a test compares
//! both). Adapted from the reference app `apps/desktop/src-tauri/src/menu.rs` (MIT, Copyright (c)
//! 2025-2026 Edward Kim, see THIRD_PARTY_NOTICES.md): the same menu layout, the same "menu id -> the
//! focused window only" delivery and the same quit item (`app:quit`, [`QUIT_ID`]), with EasyOffice's
//! differences:
//!
//! * The bar follows the focused window's kind (PD-190): `layout.rs` lists the menus of a
//!   document, spreadsheet, presentation, PDF and Hangul editor window, `build.rs` builds one bar per
//!   kind ([`KindMenus`]) and `menu_state.rs` puts the focused window's bar on screen.
//! * One static table ([`ITEMS`]) holds every custom item: its id, label, accelerator and the
//!   page action it runs. The page keeps the matching list (`NATIVE_MENU_ACTIONS` in
//!   `shell-chrome.js`); a test compares both lists, so they cannot drift apart.
//! * The page action names are the ones the keyboard shortcuts already use, so a menu item and its
//!   shortcut run the same handler (dirty prompts and the Hangul editor's shell commands included).
//!   Items that the page menu also has run that page item (`studio-cmd:<command>`), so the module
//!   rules (an item that does not apply in the window's module does nothing) stay the page's.
//! * The page is called by evaluating a fixed script with the item's static id; no event
//!   permission is added to the capability, and the id never comes from the page.
//! * Labels are Korean. Only items whose keys the page does not already handle for the focused
//!   editor carry an accelerator: the editing keys (bold and the like) stay with the editor.
//! * The application menu (about, services, hide, 끝내기) and the 윈도우 menu hold system items with
//!   Korean labels (nothing is left to the system language). The reference app's edit menu: undo,
//!   redo, cut and copy are the table's items with their ⌘ keys ([`MACOS_EDIT_KEYS`]), so they
//!   reach the page (and in the Hangul editor its own undo stack through `automation.execute`,
//!   `EMBED_EDITOR_COMMANDS`); only paste and select all are system items. 닫기 is the 윈도우
//!   menu's system item and the only ⌘W. 끝내기 ([`QUIT_ID`], ⌘Q) is a custom item at the end of the
//!   application menu, handled in Rust by `app_quit.rs`, so it is not in the table and has no page
//!   action; the Dock's 종료 reaches the same queue through `RunEvent::ExitRequested` (`main.rs`).
//! * The theme choices and the update-check switch are check items; the 'enabled' and 'checked'
//!   state of the bar follows the focused window ([`sync`], fed by `menu_state.rs`); the disabled
//!   ids come from the page, which owns the module rules (`studioCommandApplies`).

use std::{collections::HashMap, sync::Mutex};

use tauri::menu::{CheckMenuItem, CheckMenuItemBuilder, Menu, MenuItemBuilder, MenuItemKind};
use tauri::{AppHandle, Manager, Runtime, Wry};

mod build;
mod items;
mod layout;
pub use items::ITEMS;
pub use layout::MenuKind;

/// One custom menu item.
pub struct MenuItemSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub accelerator: Option<&'static str>,
    /// Page action: a name of the page's shortcut action table, optionally `name:argument`.
    pub action: &'static str,
}

/// 끝내기: the end of the application menu. It runs in Rust
/// (`app_quit.rs`), not in the page, so it is not in [`ITEMS`]; id and accelerator are the
/// reference app's.
pub const QUIT_ID: &str = "app:quit";
const QUIT_LABEL: &str = "EasyOffice 끝내기";
const QUIT_ACCELERATOR: &str = "CmdOrCtrl+Q";

/// The keys the edit menu gives the table's undo, redo, cut and copy items (the reference app's).
/// A key bound to a menu item never reaches the page, so the web view runs copy and the like only
/// through a menu key (the page's own menu row, on the other OSes, leaves the keys to the editor).
const MACOS_EDIT_KEYS: [(&str, &str); 4] = [
    ("edit:undo", "CmdOrCtrl+Z"),
    ("edit:redo", "CmdOrCtrl+Shift+Z"),
    ("edit:cut", "CmdOrCtrl+X"),
    ("edit:copy", "CmdOrCtrl+C"),
];

/// The key an item of the table gets.
fn accelerator_of(spec: &MenuItemSpec) -> Option<&'static str> {
    spec.accelerator.or_else(|| {
        MACOS_EDIT_KEYS
            .iter()
            .find(|(id, _)| *id == spec.id)
            .map(|(_, keys)| *keys)
    })
}

/// The key an item gets for a specific window kind (HWP leaves CmdOrCtrl+K for two-key chords and the
/// replace key to the editor's own Ctrl+F2; SHEET adds standard spreadsheet workbook/formula/table accelerators).
fn accelerator_for(kind: MenuKind, spec: &MenuItemSpec) -> Option<&'static str> {
    if kind == MenuKind::Hwp && matches!(spec.id, "insert:link" | "edit:replace") {
        return None;
    }
    if kind == MenuKind::Sheet {
        return match spec.id {
            "sheet:note-insert" => Some("Shift+F2"),
            "sheet:name-paste" => Some("F3"),
            "sheet:calc-now" => Some("F9"),
            "sheet:calc-sheet" => Some("Shift+F9"),
            "table:insert" => Some("CmdOrCtrl+T"),
            "table:autosum" => Some("Alt+="),
            _ => accelerator_of(spec),
        };
    }
    accelerator_of(spec)
}

/// Settings the user switches on and off or picks one of: check items whose mark [`sync`] sets.
pub const CHECK_IDS: [&str; 5] = [
    "view:theme-system",
    "view:theme-light",
    "view:theme-dark",
    TOOLBAR_LABELS_ID,
    UPDATE_CHECK_ID,
];
const TOOLBAR_LABELS_ID: &str = "view:toolbar-labels";
const UPDATE_CHECK_ID: &str = "help:update-check";
const THEME_ITEMS: [(&str, &str); 3] = [
    ("system", "view:theme-system"),
    ("light", "view:theme-light"),
    ("dark", "view:theme-dark"),
];

/// The id of the theme item for a theme mode (`system`, `light` or `dark`).
pub fn theme_item_id(mode: &str) -> Option<&'static str> {
    THEME_ITEMS
        .iter()
        .find(|(name, _)| *name == mode)
        .map(|(_, id)| *id)
}

fn is_check_item(id: &str) -> bool {
    CHECK_IDS.contains(&id)
}

/// Longest accepted id (a defensive bound for the generated script).
const MAX_ID_LEN: usize = 32;
const SCRIPT_PREFIX: &str = "window.__EASYOFFICE_NATIVE_MENU__&&window.__EASYOFFICE_NATIVE_MENU__(";

const _: () = assert!(ITEMS.len() <= ItemMask::CAPACITY);

/// A set of indexes of [`ITEMS`] (the items a window cannot use).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ItemMask([u128; 3]);

impl ItemMask {
    /// How many indexes the set can hold.
    pub const CAPACITY: usize = 3 * u128::BITS as usize;

    pub fn insert(&mut self, index: usize) {
        self.0[index / u128::BITS as usize] |= 1u128 << (index % u128::BITS as usize);
    }

    pub fn contains(&self, index: usize) -> bool {
        (self.0[index / u128::BITS as usize] & (1u128 << (index % u128::BITS as usize))) != 0
    }
}

/// Index of a menu id in [`ITEMS`], if the id is one of ours.
pub fn item_index(id: &str) -> Option<usize> {
    ITEMS.iter().position(|spec| spec.id == id)
}

/// The page action of a menu id, if the id is one of ours.
pub fn action_for(id: &str) -> Option<&'static str> {
    item_index(id).map(|index| ITEMS[index].action)
}

/// The script that runs a menu item in the page. Only ids of [`ITEMS`] produce one, and the id is
/// written as a JSON string, so nothing but the static table ever reaches the page.
pub fn dispatch_script(id: &str) -> Option<String> {
    action_for(id)?;
    let quoted = serde_json::to_string(id).ok()?;
    (id.len() <= MAX_ID_LEN).then(|| format!("{SCRIPT_PREFIX}{quoted});"))
}

/// Per-window Korean label for a menu item (`HWP` preserves Hangul terms while other Office
/// windows use standard Office Korean terminology, `PD-354`).
pub fn label_for(kind: MenuKind, spec: &MenuItemSpec) -> &'static str {
    if kind == MenuKind::Hwp {
        match spec.id {
            "edit:undo" => return "되돌리기",
            "edit:cut" => return "오려 두기",
            "edit:copy" => return "복사하기",
            "edit:paste" => return "붙이기",
            "edit:replace" => return "찾아 바꾸기",
            "view:ctrl-marks" => return "조판 부호",
            "view:para-marks" => return "문단 부호",
            "view:doc-grid" => return "격자 설정",
            "format:bold" => return "진하게",
            "format:italic" => return "기울임",
            "format:indent-in" => return "한 수준 증가",
            "format:indent-out" => return "한 수준 감소",
            "view:zoom-100" => return "실제 크기",
            _ => {}
        }
    } else if kind == MenuKind::Pages {
        match spec.id {
            "file:print" => return "프린트…",
            "edit:redo" => return "실행 복귀",
            "edit:cut" => return "오려두기",
            "edit:replace" => return "찾기 및 대치",
            "view:para-marks" => return "보이지 않는 요소 보기",
            "insert:image" => return "이미지",
            "insert:link" => return "링크",
            "format:bold" => return "볼드체",
            "format:italic" => return "이탤릭체",
            "format:underline" => return "밑줄체",
            "format:para-numbering" => return "번호 매기기",
            "format:para-bullet" => return "글머리 기호",
            "format:indent-in" => return "들여쓰기 단계 증가",
            "format:indent-out" => return "들여쓰기 단계 감소",
            "page:columns" => return "단",
            "page:break" => return "페이지 나누기",
            "table:insert" => return "표 삽입",
            "table:row-above" => return "위에 행 추가",
            "table:add-row" => return "아래에 행 추가",
            "table:col-left" => return "앞에 열 추가",
            "table:col-right" => return "뒤에 열 추가",
            "table:row-delete" => return "행 삭제",
            "table:col-delete" => return "열 삭제",
            "table:split" => return "셀 병합 해제",
            "table:merge" => return "셀 병합",
            "table:col-equal" => return "같은 너비로 열 조절",
            "table:border-bg" => return "테두리 및 음영",
            "table:cell-props" => return "표 속성",
            _ => {}
        }
    } else if kind == MenuKind::Doc {
        match spec.id {
            "file:doc-paper" | "page:doc-paper" => return "페이지 설정",
            "edit:replace" => return "바꾸기",
            "view:zoom-fit-page" => return "한 페이지",
            "view:zoom-fit-width" => return "페이지 너비",
            "view:ctrl-marks" => return "모든 서식 표시",
            "view:para-marks" => return "단락 표시",
            "tool:review-stats" => return "단어 수",
            "insert:symbol" => return "기호",
            "format:para-numbering" => return "번호 매기기",
            "format:para-bullet" => return "글머리 기호",
            "page:columns" => return "단",
            "page:break" => return "페이지 나누기",
            "table:insert" => return "표 삽입",
            "table:row-above" => return "위에 삽입",
            "table:add-row" => return "아래에 삽입",
            "table:col-left" => return "왼쪽에 삽입",
            "table:col-right" => return "오른쪽에 삽입",
            "table:row-delete" => return "행 삭제",
            "table:col-delete" => return "열 삭제",
            "table:split" => return "셀 분할",
            "table:merge" => return "셀 병합",
            "table:col-equal" => return "열 너비를 같게",
            "table:border-bg" => return "테두리 및 음영",
            "table:cell-props" => return "표 속성",
            _ => {}
        }
    } else if matches!(kind, MenuKind::Sheet | MenuKind::Slide) {
        match spec.id {
            "insert:symbol" => return "기호",
            "table:insert" => return "표 삽입",
            _ => {}
        }
    }
    spec.label
}

fn custom<R: Runtime>(
    app: &AppHandle<R>,
    kind: MenuKind,
    spec: &MenuItemSpec,
) -> tauri::Result<tauri::menu::MenuItem<R>> {
    let builder = MenuItemBuilder::with_id(spec.id, label_for(kind, spec));
    match accelerator_for(kind, spec) {
        Some(accelerator) => builder.accelerator(accelerator).build(app),
        None => builder.build(app),
    }
}

fn check<R: Runtime>(
    app: &AppHandle<R>,
    kind: MenuKind,
    spec: &MenuItemSpec,
) -> tauri::Result<CheckMenuItem<R>> {
    CheckMenuItemBuilder::with_id(spec.id, label_for(kind, spec))
        .checked(false)
        .build(app)
}

fn quit_item<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<tauri::menu::MenuItem<R>> {
    MenuItemBuilder::with_id(QUIT_ID, QUIT_LABEL)
        .accelerator(QUIT_ACCELERATOR)
        .build(app)
}

fn spec(id: &str) -> &'static MenuItemSpec {
    ITEMS
        .iter()
        .find(|spec| spec.id == id)
        .expect("menu id is in the table")
}

/// Table items the bar does not install as custom items: paste is the system item (it reaches the
/// page's paste handler and text fields), and 닫기 is the 윈도우 menu's system item, the one ⌘W.
/// They stay in the table because the page's menu row has them.
const MACOS_SYSTEM_IDS: &[&str] = &["edit:paste", "file:close"];

/// The custom ids the menu bar holds.
pub fn installed_ids() -> impl Iterator<Item = &'static str> {
    ITEMS
        .iter()
        .map(|spec| spec.id)
        .filter(|id| !MACOS_SYSTEM_IDS.contains(id))
}

/// What the menu bar shows for the focused window.
pub struct MenuView<'a> {
    /// Theme mode of the window: `system`, `light` or `dark`.
    pub theme: &'a str,
    /// Holds `i` when `ITEMS[i]` cannot be used in the window's module.
    pub disabled_mask: ItemMask,
    /// The update-check switch: `None` when the app does not check at all (it is then disabled).
    pub update_check: Option<bool>,
    /// Whether `#icon-toolbar` / DOM toolbars show `.tb-label` text under buttons (`PD-105`, `PD-354`).
    pub toolbar_labels: bool,
}

/// `(checked, enabled)` of a check item, `None` for an id that is not one.
fn check_state(id: &str, view: &MenuView) -> Option<(bool, bool)> {
    if id == UPDATE_CHECK_ID {
        return Some((view.update_check == Some(true), view.update_check.is_some()));
    }
    if id == TOOLBAR_LABELS_ID {
        let enabled = item_index(id).is_none_or(|index| !view.disabled_mask.contains(index));
        return Some((view.toolbar_labels, enabled));
    }
    let theme = theme_item_id(view.theme);
    is_check_item(id).then(|| {
        let enabled = item_index(id).is_none_or(|index| !view.disabled_mask.contains(index));
        (theme == Some(id), enabled)
    })
}

/// Visits every item of the bar, submenus included.
pub fn for_each_item<R: Runtime>(
    menu: &Menu<R>,
    visit: &mut impl FnMut(&MenuItemKind<R>),
) -> tauri::Result<()> {
    fn walk<R: Runtime>(
        items: Vec<MenuItemKind<R>>,
        visit: &mut impl FnMut(&MenuItemKind<R>),
    ) -> tauri::Result<()> {
        for item in items {
            visit(&item);
            if let Some(submenu) = item.as_submenu() {
                walk(submenu.items()?, visit)?;
            }
        }
        Ok(())
    }
    walk(menu.items()?, visit)
}

/// Shows `view` on the bar: grays out the unusable items and sets the marks of the check items.
pub fn sync<R: Runtime>(menu: &Menu<R>, view: &MenuView) -> tauri::Result<()> {
    let mut failed = None;
    for_each_item(menu, &mut |item| {
        let id = item.id().as_ref();
        let result = match item {
            MenuItemKind::Check(check) => check_state(id, view)
                .map_or(Ok(()), |(checked, enabled)| {
                    check.set_checked(checked).and(check.set_enabled(enabled))
                }),
            MenuItemKind::MenuItem(plain) => match item_index(id) {
                Some(index) => plain.set_enabled(!view.disabled_mask.contains(index)),
                None => Ok(()),
            },
            _ => Ok(()),
        };
        failed = failed.take().or(result.err());
    })?;
    failed.map_or(Ok(()), Err)
}

/// The menu bars, one per kind of window, built once. The bar on screen is the one of the window
/// that has the focus (`menu_state.rs` puts it there).
pub struct KindMenus {
    menus: HashMap<MenuKind, Menu<Wry>>,
    shown: Mutex<MenuKind>,
}

impl KindMenus {
    fn new(app: &AppHandle) -> tauri::Result<Self> {
        let mut menus = HashMap::new();
        for kind in MenuKind::ALL {
            menus.insert(kind, build::build(app, kind)?);
        }
        Ok(Self {
            menus,
            shown: Mutex::new(MenuKind::Doc),
        })
    }

    fn menu(&self, kind: MenuKind) -> Menu<Wry> {
        self.menus[&kind].clone()
    }

    fn shown_guard(&self) -> std::sync::MutexGuard<'_, MenuKind> {
        self.shown
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The kind whose bar is on screen.
    pub fn shown(&self) -> MenuKind {
        *self.shown_guard()
    }

    /// Every bar (the smoke run checks all of them).
    pub fn all(&self) -> impl Iterator<Item = &Menu<Wry>> {
        self.menus.values()
    }

    /// Puts the bar of `kind` on screen unless it is already there, and returns it.
    pub fn show(&self, app: &AppHandle, kind: MenuKind) -> tauri::Result<Menu<Wry>> {
        let menu = self.menu(kind);
        let mut shown = self.shown_guard();
        if *shown != kind {
            app.set_menu(menu.clone())?;
            *shown = kind;
        }
        Ok(menu)
    }
}

/// Installs the menu bars and routes custom items to the focused window.
pub fn install(app: &mut tauri::App) -> tauri::Result<()> {
    let kinds = KindMenus::new(app.handle())?;
    app.set_menu(kinds.menu(kinds.shown()))?;
    app.manage(kinds);
    app.on_menu_event(|app, event| run_item(app, event.id().as_ref()));
    Ok(())
}

/// Runs a custom item in the window that has the focus; with no focused window nothing happens
/// (a menu click never reaches a window the user is not looking at).
fn run_item(app: &AppHandle, id: &str) {
    if id == QUIT_ID {
        crate::app_quit::request_app_quit(app);
        return;
    }
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
    // The system flips a check item when it is clicked; the bar shows the state again.
    crate::menu_state::refresh_logged(app);
}

#[cfg(test)]
mod tests;

//! Turns the menus of `layout.rs` into Tauri menus: table ids become custom items (check items for
//! the themes and the update switch), tokens become the system items and submenus, and the quit id
//! becomes the custom 끝내기 item.

use tauri::menu::{Menu, PredefinedMenuItem, Submenu, SubmenuBuilder};
use tauri::{AppHandle, Runtime};

use super::layout::{
    self, MenuKind, MenuSpec, CLOSE_WINDOW, HIDE, HIDE_OTHERS, MINIMIZE, NEW_DOCS, NEW_DOC_IDS,
    PASTE, SELECT_ALL, SEPARATOR, SERVICES, SHOW_ALL, THEME, THEME_IDS,
};
use super::{check, custom, is_check_item, quit_item, spec, QUIT_ID};

fn nested<R: Runtime>(
    app: &AppHandle<R>,
    title: &str,
    ids: &[&'static str],
) -> tauri::Result<Submenu<R>> {
    let mut builder = SubmenuBuilder::new(app, title);
    for id in ids {
        builder = if is_check_item(id) {
            builder.item(&check(app, spec(id))?)
        } else {
            builder.item(&custom(app, spec(id))?)
        };
    }
    builder.build()
}

fn submenu<R: Runtime>(app: &AppHandle<R>, menu: &MenuSpec) -> tauri::Result<Submenu<R>> {
    let mut builder = SubmenuBuilder::new(app, menu.title);
    for line in menu.lines {
        builder = match *line {
            SEPARATOR => builder.separator(),
            NEW_DOCS => builder.item(&nested(app, "새 문서", &NEW_DOC_IDS)?),
            THEME => builder.item(&nested(app, "테마", &THEME_IDS)?),
            // Paste and select all are system items: paste reaches the page's paste handler and
            // the text fields. Every system item carries its Korean text (the system language
            // would show otherwise).
            PASTE => builder.item(&PredefinedMenuItem::paste(
                app,
                Some(spec("edit:paste").label),
            )?),
            SELECT_ALL => builder.item(&PredefinedMenuItem::select_all(app, Some("모두 선택"))?),
            SERVICES => builder.item(&PredefinedMenuItem::services(app, Some("서비스"))?),
            HIDE => builder.item(&PredefinedMenuItem::hide(app, Some("EasyOffice 가리기"))?),
            HIDE_OTHERS => {
                builder.item(&PredefinedMenuItem::hide_others(app, Some("기타 가리기"))?)
            }
            SHOW_ALL => builder.item(&PredefinedMenuItem::show_all(app, Some("모두 보기"))?),
            MINIMIZE => builder.item(&PredefinedMenuItem::minimize(app, Some("최소화"))?),
            CLOSE_WINDOW => builder.item(&PredefinedMenuItem::close_window(app, Some("닫기"))?),
            // 끝내기 is the custom item that runs in Rust, not the system one that would end the
            // process at once.
            QUIT_ID => builder.item(&quit_item(app)?),
            id if is_check_item(id) => builder.item(&check(app, spec(id))?),
            id => builder.item(&custom(app, spec(id))?),
        };
    }
    builder.build()
}

/// Builds the whole menu bar of a kind of window.
pub fn build<R: Runtime>(app: &AppHandle<R>, kind: MenuKind) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    for spec in layout::layout(kind) {
        menu.append(&submenu(app, spec)?)?;
    }
    Ok(menu)
}

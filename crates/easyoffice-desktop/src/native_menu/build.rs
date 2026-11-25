//! Turns the menus of `layout.rs` into Tauri menus: table ids become custom items (check items for
//! the themes and the update switch), tokens become the system items and submenus, and the quit id
//! becomes the custom 끝내기 item.

use tauri::menu::{Menu, PredefinedMenuItem, Submenu, SubmenuBuilder};
use tauri::{AppHandle, Runtime};

use super::layout::{
    self, MenuKind, MenuSpec, ALIGN, ALIGN_LINES, BORDER, BORDER_LINES, CALC_OPTIONS,
    CALC_OPTIONS_LINES, CF, CF_LINES, CLOSE_WINDOW, EXPORT_BASE, EXPORT_BASE_LINES, EXPORT_BOARD,
    EXPORT_BOARD_LINES, EXPORT_DOC, EXPORT_DOC_LINES, EXPORT_PDF, EXPORT_PDF_LINES, EXPORT_SHEET,
    EXPORT_SHEET_LINES, EXPORT_SLIDE, EXPORT_SLIDE_LINES, HIDE, HIDE_OTHERS, MINIMIZE, NEW_DOCS,
    NEW_DOC_IDS, NUMFMT, NUMFMT_LINES, PASTE, SELECT_ALL, SEPARATOR, SERVICES, SHOW_ALL, THEME,
    THEME_IDS, USE_IN_FORMULA, USE_IN_FORMULA_LINES,
};
use super::{check, custom, is_check_item, label_for, quit_item, spec, QUIT_ID};

fn nested<R: Runtime>(
    app: &AppHandle<R>,
    kind: MenuKind,
    title: &str,
    lines: &[&'static str],
) -> tauri::Result<Submenu<R>> {
    let mut builder = SubmenuBuilder::new(app, title);
    for id in lines {
        builder = if *id == SEPARATOR {
            builder.separator()
        } else if let Some(sub) = layout::cf_submenu(id) {
            builder.item(&nested(app, kind, sub.title, sub.lines)?)
        } else if is_check_item(id) {
            builder.item(&check(app, kind, spec(id))?)
        } else {
            builder.item(&custom(app, kind, spec(id))?)
        };
    }
    builder.build()
}

fn submenu<R: Runtime>(
    app: &AppHandle<R>,
    kind: MenuKind,
    menu: &MenuSpec,
) -> tauri::Result<Submenu<R>> {
    let mut builder = SubmenuBuilder::new(app, menu.title);
    for line in menu.lines {
        builder = match *line {
            SEPARATOR => builder.separator(),
            NEW_DOCS => builder.item(&nested(app, kind, "새 문서", &NEW_DOC_IDS)?),
            EXPORT_DOC => builder.item(&nested(app, kind, "내보내기", &EXPORT_DOC_LINES)?),
            EXPORT_SHEET => builder.item(&nested(app, kind, "내보내기", &EXPORT_SHEET_LINES)?),
            EXPORT_SLIDE => builder.item(&nested(app, kind, "내보내기", &EXPORT_SLIDE_LINES)?),
            EXPORT_BASE => builder.item(&nested(app, kind, "내보내기", &EXPORT_BASE_LINES)?),
            EXPORT_BOARD => builder.item(&nested(app, kind, "내보내기", &EXPORT_BOARD_LINES)?),
            EXPORT_PDF => builder.item(&nested(app, kind, "내보내기", &EXPORT_PDF_LINES)?),
            THEME => builder.item(&nested(app, kind, "테마", &THEME_IDS)?),
            NUMFMT => builder.item(&nested(app, kind, "표시 형식", &NUMFMT_LINES)?),
            ALIGN => builder.item(&nested(app, kind, "맞춤", &ALIGN_LINES)?),
            BORDER => builder.item(&nested(app, kind, "테두리", &BORDER_LINES)?),
            CF => builder.item(&nested(app, kind, "조건부 서식", &CF_LINES)?),
            USE_IN_FORMULA => {
                builder.item(&nested(app, kind, "수식에서 사용", &USE_IN_FORMULA_LINES)?)
            }
            CALC_OPTIONS => builder.item(&nested(app, kind, "계산 옵션", &CALC_OPTIONS_LINES)?),
            // Paste and select all are system items: paste reaches the page's paste handler and
            // the text fields. Every system item carries its Korean text (the system language
            // would show otherwise).
            PASTE => builder.item(&PredefinedMenuItem::paste(
                app,
                Some(label_for(kind, spec("edit:paste"))),
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
            id if is_check_item(id) => builder.item(&check(app, kind, spec(id))?),
            id => builder.item(&custom(app, kind, spec(id))?),
        };
    }
    builder.build()
}

/// Builds the whole menu bar of a kind of window.
pub fn build<R: Runtime>(app: &AppHandle<R>, kind: MenuKind) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    for spec in layout::layout(kind) {
        menu.append(&submenu(app, kind, spec)?)?;
    }
    Ok(menu)
}

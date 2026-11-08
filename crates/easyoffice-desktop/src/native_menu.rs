//! Native menu bar (PD-102), installed on every OS.
//!
//! The desktop shell draws one menu bar and the page hides its own (`data-native-menu`), so every
//! item of the page's menu bar is in [`ITEMS`] (a test compares both). Adapted from the reference app
//! `apps/desktop/src-tauri/src/menu.rs` (MIT, Copyright (c) 2025-2026 Edward Kim, see
//! THIRD_PARTY_NOTICES.md): the same menu layout, the same "menu id -> the focused window only"
//! delivery and the same quit item (`app:quit`, [`QUIT_ID`]), with EasyOffice's differences:
//!
//! * One static table ([`ITEMS`]) holds every custom item: its id, label, accelerator and the
//!   page action it runs. The page keeps the matching list (`NATIVE_MENU_ACTIONS` in
//!   `shell-chrome.js`); a test compares both lists, so they cannot drift apart.
//! * The page action names are the ones the keyboard shortcuts already use, so a menu item and its
//!   shortcut run the same handler (dirty prompts and the Hangul editor's shell commands included).
//!   Items that the page menu also has run that page item (`studio-cmd:<command>`), so the module
//!   rules (an item that does not apply in the window's module does nothing) stay the page's.
//! * The page is called by evaluating a fixed script with the item's static id; no event
//!   permission is added to the capability, and the id never comes from the page.
//! * Labels are Korean on every OS. Accelerators follow the OS (`CmdOrCtrl`). Only items whose
//!   keys the page does not already handle for the focused editor carry one: the editing keys
//!   (bold, undo, copy and the like) stay with the editor.
//! * macOS: the application menu (about, services, hide, 끝내기) and the 윈도우 menu hold system
//!   items with Korean labels (nothing is left to the system language). The reference app's edit
//!   menu: undo, redo, cut and copy are the table's items with their ⌘ keys ([`MACOS_EDIT_KEYS`]), so
//!   they reach the page (and in the Hangul editor its own undo stack through `automation.execute`,
//!   `EMBED_EDITOR_COMMANDS`); only paste and select all are system items. 닫기 is the 윈도우
//!   menu's system item and the only ⌘W (the 파일 menu has none on macOS). 끝내기 ([`QUIT_ID`], ⌘Q)
//!   is a custom item on every OS (macOS: application menu, Windows and Linux: end of the 파일
//!   menu), handled in Rust by `app_quit.rs`, so it is not in the table and has no page action;
//!   the Dock's 종료 reaches the same queue through `RunEvent::ExitRequested` (`main.rs`).
//!   Windows and Linux have no system undo/redo (and no application menu), so their edit menu uses
//!   the table's items without keys.
//! * The theme choices and the update-check switch are check items; the 'enabled' and 'checked'
//!   state of the bar follows the focused window ([`sync`], fed by `menu_state.rs`).
//!   The disabled ids come from the page, which owns the module rules (`studioCommandApplies`).

use std::collections::HashSet;

use tauri::menu::{
    CheckMenuItem, CheckMenuItemBuilder, Menu, MenuItemBuilder, MenuItemKind, PredefinedMenuItem,
    Submenu, SubmenuBuilder,
};
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
    item("file:new-doc", "문서 (DOCX)", None, "window-new:doc"),
    item("file:new-hwp", "한글 문서 (HWP)", None, "window-new:hwp"),
    item(
        "file:new-sheet",
        "스프레드시트 (XLSX)",
        None,
        "window-new:sheet",
    ),
    item(
        "file:new-slide",
        "프레젠테이션 (PPTX)",
        None,
        "window-new:slide",
    ),
    item("file:new-pdf", "PDF 스튜디오", None, "window-new:pdf"),
    item("file:new-conv", "포맷 변환기", None, "window-new:conv"),
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
    item("edit:undo", "되돌리기", None, "studio-cmd:studio-undo"),
    item("edit:redo", "다시 실행", None, "studio-cmd:studio-redo"),
    item("edit:cut", "오려 두기", None, "studio-cmd:clip-cut"),
    item("edit:copy", "복사하기", None, "studio-cmd:clip-copy"),
    item("edit:paste", "붙이기", None, "studio-cmd:clip-paste"),
    item("edit:find", "찾기", Some("CmdOrCtrl+F"), "find"),
    item(
        "view:zoom-in",
        "확대",
        Some("CmdOrCtrl+="),
        "studio-cmd:zoom-in",
    ),
    item(
        "view:zoom-out",
        "축소",
        Some("CmdOrCtrl+-"),
        "studio-cmd:zoom-out",
    ),
    item(
        "view:zoom-fit-page",
        "쪽 맞춤",
        None,
        "studio-cmd:zoom-fit-page",
    ),
    item(
        "view:zoom-fit-width",
        "폭 맞춤",
        None,
        "studio-cmd:zoom-fit-width",
    ),
    item(
        "view:zoom-100",
        "100% 실제 배율",
        Some("CmdOrCtrl+0"),
        "studio-cmd:view-zoom",
    ),
    item(
        "view:split",
        "좌우 스플릿 뷰",
        None,
        "studio-cmd:view-split",
    ),
    item(
        "view:outline",
        "문서 개요…",
        None,
        "studio-cmd:view-outline",
    ),
    item("view:theme-system", "시스템 설정", None, "theme:system"),
    item("view:theme-light", "밝게", None, "theme:light"),
    item("view:theme-dark", "어둡게", None, "theme:dark"),
    item(
        "view:theme-toggle",
        "밝게/어둡게 전환",
        Some("CmdOrCtrl+Alt+D"),
        "studio-cmd:view-theme",
    ),
    item(
        "view:ctrl-marks",
        "조판 부호",
        None,
        "studio-cmd:ctrl-marks",
    ),
    item(
        "view:para-marks",
        "문단 부호",
        None,
        "studio-cmd:para-marks",
    ),
    item("view:doc-grid", "격자 설정", None, "studio-cmd:doc-grid"),
    item("insert:shape", "도형", None, "studio-cmd:insert-shape"),
    item("insert:chart", "차트 개체", None, "studio-cmd:insert-chart"),
    item("insert:footnote", "각주", None, "studio-cmd:mod-footnote"),
    item("insert:symbol", "문자표", None, "studio-cmd:mod-symbol"),
    item("insert:link", "하이퍼링크", None, "studio-cmd:insert-link"),
    item(
        "insert:toc",
        "자동 목차 생성",
        None,
        "studio-cmd:doc-mod-toc",
    ),
    item(
        "insert:mailmerge",
        "메일 머지 치환",
        None,
        "studio-cmd:doc-mod-mailmerge",
    ),
    item(
        "insert:field",
        "누름틀 필드 삽입",
        None,
        "studio-cmd:hwp-mod-field",
    ),
    item("format:bold", "진하게", None, "studio-cmd:home-bold"),
    item("format:italic", "기울임", None, "studio-cmd:home-italic"),
    item(
        "format:underline",
        "밑줄",
        None,
        "studio-cmd:home-underline",
    ),
    item(
        "format:char-shape",
        "글자 모양",
        None,
        "studio-cmd:home-style",
    ),
    item(
        "format:para-shape",
        "문단 모양",
        None,
        "studio-cmd:para-shape",
    ),
    item(
        "format:para-numbering",
        "문단 번호 모양",
        None,
        "studio-cmd:para-numbering",
    ),
    item(
        "format:para-bullet",
        "글머리표 모양",
        None,
        "studio-cmd:para-bullet",
    ),
    item(
        "format:indent-in",
        "한 수준 증가",
        None,
        "studio-cmd:indent-in",
    ),
    item(
        "format:indent-out",
        "한 수준 감소",
        None,
        "studio-cmd:indent-out",
    ),
    item("page:paper", "편집 용지", None, "studio-cmd:layout-paper"),
    item(
        "page:orientation",
        "용지 방향 전환",
        None,
        "studio-cmd:layout-orientation",
    ),
    item(
        "page:margins",
        "여백 전환",
        None,
        "studio-cmd:layout-margins",
    ),
    item(
        "page:header",
        "머리말/꼬리말",
        None,
        "studio-cmd:mod-header",
    ),
    item(
        "page:columns",
        "다단 설정",
        None,
        "studio-cmd:layout-columns",
    ),
    item(
        "page:hwp-header",
        "행정 공문서 두문·결재란",
        None,
        "studio-cmd:hwp-mod-header",
    ),
    item(
        "page:manuscript",
        "200자 원고지 매수 검산",
        None,
        "studio-cmd:hwp-mod-manuscript",
    ),
    item(
        "page:slide-master",
        "마스터 레이아웃",
        None,
        "studio-cmd:slide-mod-master",
    ),
    item(
        "page:slide-snap",
        "스마트 정렬 가이드",
        None,
        "studio-cmd:slide-mod-snap",
    ),
    item(
        "page:slide-transition",
        "화면 전환 효과",
        None,
        "studio-cmd:slide-mod-transition",
    ),
    item(
        "page:slide-presenter",
        "발표자 도구 시작",
        None,
        "studio-cmd:slide-mod-presenter",
    ),
    item("table:insert", "표 만들기", None, "studio-cmd:insert-table"),
    item(
        "table:add-row",
        "아래쪽에 줄 추가하기",
        None,
        "studio-cmd:table-add-row",
    ),
    item(
        "table:cell-select",
        "셀 선택",
        Some("CmdOrCtrl+Alt+T"),
        "hwp-editor",
    ),
    item("table:merge", "셀 합치기", None, "studio-cmd:table-merge"),
    item("table:cell-split", "셀 나누기", None, "hwp-editor"),
    item("table:sum", "블록 합계", None, "studio-cmd:table-sum"),
    item(
        "table:doc-formula",
        "표 셀 수식 계산",
        None,
        "studio-cmd:doc-mod-tableformula",
    ),
    item(
        "table:hwp-border",
        "이중선·대각선 테두리",
        None,
        "studio-cmd:hwp-mod-border",
    ),
    item(
        "table:autosum",
        "자동 합계 (SUM)",
        None,
        "studio-cmd:data-autosum",
    ),
    item(
        "table:fn-browser",
        "함수 마법사",
        None,
        "studio-cmd:data-fn-browser",
    ),
    item(
        "table:sort",
        "오름차순·내림차순 정렬",
        None,
        "studio-cmd:data-sort",
    ),
    item(
        "table:validation",
        "데이터 유효성 검사",
        None,
        "studio-cmd:data-validation",
    ),
    item(
        "table:sheet-pivot",
        "피벗 테이블 요약",
        None,
        "studio-cmd:sheet-mod-pivot",
    ),
    item(
        "table:sheet-chart",
        "차트 시각화",
        None,
        "studio-cmd:sheet-mod-chart",
    ),
    item(
        "table:sheet-condfmt",
        "조건부 서식 규칙",
        None,
        "studio-cmd:sheet-mod-condfmt",
    ),
    item(
        "table:sheet-freeze",
        "행·열 틀 고정",
        None,
        "studio-cmd:sheet-mod-freeze",
    ),
    item(
        "tool:diagnose",
        "문장 진단 및 요약",
        None,
        "studio-cmd:smart-diagnose",
    ),
    item(
        "tool:scrub-pii",
        "개인정보 스캔/마스킹",
        None,
        "studio-cmd:scrub-pii",
    ),
    item(
        "tool:security-audit",
        "문서 보안 검사",
        None,
        "studio-cmd:security-audit",
    ),
    item(
        "tool:bench-run",
        "문서 엔진 성능 진단",
        None,
        "studio-cmd:bench-run",
    ),
    item(
        "tool:review-track",
        "변경 내용 추적",
        None,
        "studio-cmd:review-track",
    ),
    item(
        "tool:review-comment",
        "검토 메모 스레드",
        None,
        "studio-cmd:review-comment",
    ),
    item(
        "tool:review-verify",
        "문서 구조 보안 검사",
        None,
        "studio-cmd:review-verify",
    ),
    item(
        "tool:review-stats",
        "문서 통계·글자 수",
        None,
        "studio-cmd:review-stats",
    ),
    item(
        "tool:pdf-highlight",
        "형광펜·밑줄 주석",
        None,
        "studio-cmd:pdf-mod-highlight",
    ),
    item(
        "tool:pdf-rotate",
        "페이지 90° 회전",
        None,
        "studio-cmd:pdf-mod-rotate",
    ),
    item(
        "tool:pdf-splitmerge",
        "페이지 분할·병합",
        None,
        "studio-cmd:pdf-mod-splitmerge",
    ),
    item(
        "tool:pdf-tooffice",
        "편집 문서 변환",
        None,
        "studio-cmd:pdf-mod-tooffice",
    ),
    item(
        "tool:palette",
        "명령 검색·환경 설정",
        None,
        "studio-cmd:palette",
    ),
    item(
        "help:update-check",
        "업데이트 확인 알림 켜기/끄기",
        None,
        "update-check",
    ),
];

/// 끝내기: the application menu on macOS, the end of the 파일 menu elsewhere. It runs in Rust
/// (`app_quit.rs`), not in the page, so it is not in [`ITEMS`]; id and accelerator are the
/// reference app's.
pub const QUIT_ID: &str = "app:quit";
const QUIT_LABEL: &str = if cfg!(target_os = "macos") {
    "EasyOffice 끝내기"
} else {
    "끝내기"
};
const QUIT_ACCELERATOR: &str = "CmdOrCtrl+Q";

/// The keys the macOS edit menu gives the table's undo, redo, cut and copy items (the reference
/// app's). A key bound to a menu item never reaches the page, so only macOS binds them: its web
/// view runs copy and the like only through a menu key. Elsewhere the editor takes the keys.
const MACOS_EDIT_KEYS: [(&str, &str); 4] = [
    ("edit:undo", "CmdOrCtrl+Z"),
    ("edit:redo", "CmdOrCtrl+Shift+Z"),
    ("edit:cut", "CmdOrCtrl+X"),
    ("edit:copy", "CmdOrCtrl+C"),
];

/// The key an item of the table gets on this OS.
fn accelerator_of(spec: &MenuItemSpec) -> Option<&'static str> {
    spec.accelerator.or_else(|| {
        MACOS_EDIT_KEYS
            .iter()
            .find(|(id, _)| cfg!(target_os = "macos") && *id == spec.id)
            .map(|(_, keys)| *keys)
    })
}

/// Settings the user switches on and off or picks one of: check items whose mark [`sync`] sets.
pub const CHECK_IDS: [&str; 4] = [
    "view:theme-system",
    "view:theme-light",
    "view:theme-dark",
    UPDATE_CHECK_ID,
];
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
    match accelerator_of(spec) {
        Some(accelerator) => builder.accelerator(accelerator).build(app),
        None => builder.build(app),
    }
}

fn check<R: Runtime>(app: &AppHandle<R>, spec: &MenuItemSpec) -> tauri::Result<CheckMenuItem<R>> {
    CheckMenuItemBuilder::with_id(spec.id, spec.label)
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

/// Table items macOS does not install as custom items: paste is the system item (it reaches the
/// page's paste handler and text fields), and 닫기 is the 윈도우 menu's system item, the one ⌘W.
const MACOS_SYSTEM_IDS: &[&str] = &["edit:paste", "file:close"];

/// The custom ids the menu bar of this OS holds.
pub fn installed_ids() -> impl Iterator<Item = &'static str> {
    ITEMS
        .iter()
        .map(|spec| spec.id)
        .filter(|id| !(cfg!(target_os = "macos") && MACOS_SYSTEM_IDS.contains(id)))
}

const SEPARATOR: &str = "-";

/// One line of a submenu: a table item, a separator or a nested submenu.
enum Entry<R: Runtime> {
    Item(&'static str),
    Separator,
    Sub(Submenu<R>),
}

fn entries<R: Runtime>(ids: &[&'static str]) -> Vec<Entry<R>> {
    ids.iter()
        .map(|id| {
            if *id == SEPARATOR {
                Entry::Separator
            } else {
                Entry::Item(id)
            }
        })
        .collect()
}

fn submenu<R: Runtime>(
    app: &AppHandle<R>,
    title: &str,
    lines: Vec<Entry<R>>,
) -> tauri::Result<Submenu<R>> {
    let mut builder = SubmenuBuilder::new(app, title);
    for line in lines {
        builder = match line {
            Entry::Separator => builder.separator(),
            Entry::Item(QUIT_ID) => builder.item(&quit_item(app)?),
            Entry::Item(id) if is_check_item(id) => builder.item(&check(app, spec(id))?),
            Entry::Item(id) => builder.item(&custom(app, spec(id))?),
            Entry::Sub(sub) => builder.item(&sub),
        };
    }
    builder.build()
}

fn app_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Submenu<R>> {
    SubmenuBuilder::new(app, "EasyOffice")
        .item(&custom(app, spec("app:about"))?)
        .separator()
        .item(&PredefinedMenuItem::services(app, Some("서비스"))?)
        .separator()
        .item(&PredefinedMenuItem::hide(app, Some("EasyOffice 가리기"))?)
        .item(&PredefinedMenuItem::hide_others(app, Some("기타 가리기"))?)
        .item(&PredefinedMenuItem::show_all(app, Some("모두 보기"))?)
        .separator()
        .item(&quit_item(app)?)
        .build()
}

fn file_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Submenu<R>> {
    let kinds = submenu(
        app,
        "새 문서",
        entries(&[
            "file:new-doc",
            "file:new-hwp",
            "file:new-sheet",
            "file:new-slide",
            "file:new-pdf",
            "file:new-conv",
        ]),
    )?;
    let mut lines = entries(&["file:new"]);
    lines.push(Entry::Sub(kinds));
    lines.extend(entries(&[
        "file:open",
        "file:open-recent",
        SEPARATOR,
        "file:save",
        "file:save-as",
    ]));
    // macOS: 닫기 is the 윈도우 menu's (the only ⌘W) and 끝내기 is in the application menu.
    if !cfg!(target_os = "macos") {
        lines.extend(entries(&[SEPARATOR, "file:close", SEPARATOR, QUIT_ID]));
    }
    submenu(app, "파일", lines)
}

fn edit_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Submenu<R>> {
    if !cfg!(target_os = "macos") {
        let ids = ["edit:undo", "edit:redo", SEPARATOR, "edit:cut", "edit:copy"];
        let mut lines = entries(&ids);
        lines.extend(entries(&["edit:paste", SEPARATOR, "edit:find"]));
        return submenu(app, "편집", lines);
    }
    SubmenuBuilder::new(app, "편집")
        .item(&custom(app, spec("edit:undo"))?)
        .item(&custom(app, spec("edit:redo"))?)
        .separator()
        .item(&custom(app, spec("edit:cut"))?)
        .item(&custom(app, spec("edit:copy"))?)
        .item(&PredefinedMenuItem::paste(
            app,
            Some(spec("edit:paste").label),
        )?)
        .item(&PredefinedMenuItem::select_all(app, Some("모두 선택"))?)
        .separator()
        .item(&custom(app, spec("edit:find"))?)
        .build()
}

fn view_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Submenu<R>> {
    let theme = submenu(
        app,
        "테마",
        entries(&["view:theme-system", "view:theme-light", "view:theme-dark"]),
    )?;
    let mut lines = entries(&[
        "view:zoom-in",
        "view:zoom-out",
        "view:zoom-fit-page",
        "view:zoom-fit-width",
        "view:zoom-100",
        SEPARATOR,
        "view:split",
        "view:outline",
        SEPARATOR,
    ]);
    lines.push(Entry::Sub(theme));
    lines.extend(entries(&[
        "view:theme-toggle",
        SEPARATOR,
        "view:ctrl-marks",
        "view:para-marks",
        "view:doc-grid",
    ]));
    submenu(app, "보기", lines)
}

fn insert_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Submenu<R>> {
    let ids = [
        "insert:shape",
        "insert:chart",
        "insert:footnote",
        "insert:symbol",
        "insert:link",
        "insert:toc",
        "insert:mailmerge",
        "insert:field",
    ];
    submenu(app, "입력", entries(&ids))
}

fn format_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Submenu<R>> {
    let ids = [
        "format:bold",
        "format:italic",
        "format:underline",
        SEPARATOR,
        "format:char-shape",
        "format:para-shape",
        "format:para-numbering",
        "format:para-bullet",
        "format:indent-in",
        "format:indent-out",
    ];
    submenu(app, "서식", entries(&ids))
}

fn page_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Submenu<R>> {
    let ids = [
        "page:paper",
        "page:orientation",
        "page:margins",
        "page:header",
        "page:columns",
        SEPARATOR,
        "page:hwp-header",
        "page:manuscript",
        SEPARATOR,
        "page:slide-master",
        "page:slide-snap",
        "page:slide-transition",
        "page:slide-presenter",
    ];
    submenu(app, "쪽", entries(&ids))
}

fn table_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Submenu<R>> {
    let ids = [
        "table:insert",
        "table:add-row",
        SEPARATOR,
        "table:cell-select",
        "table:merge",
        "table:cell-split",
        SEPARATOR,
        "table:sum",
        "table:doc-formula",
        "table:hwp-border",
        SEPARATOR,
        "table:autosum",
        "table:fn-browser",
        "table:sort",
        "table:validation",
        "table:sheet-pivot",
        "table:sheet-chart",
        "table:sheet-condfmt",
        "table:sheet-freeze",
    ];
    submenu(app, "표", entries(&ids))
}

fn tool_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Submenu<R>> {
    let ids = [
        "tool:diagnose",
        "tool:scrub-pii",
        "tool:security-audit",
        "tool:bench-run",
        SEPARATOR,
        "tool:review-track",
        "tool:review-comment",
        "tool:review-verify",
        "tool:review-stats",
        SEPARATOR,
        "tool:pdf-highlight",
        "tool:pdf-rotate",
        "tool:pdf-splitmerge",
        "tool:pdf-tooffice",
        SEPARATOR,
        "tool:palette",
    ];
    submenu(app, "도구", entries(&ids))
}

fn window_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Submenu<R>> {
    SubmenuBuilder::new(app, "윈도우")
        .item(&PredefinedMenuItem::minimize(app, Some("최소화"))?)
        .item(&PredefinedMenuItem::close_window(app, Some("닫기"))?)
        .build()
}

/// 도움말: the About item lives in the application menu on macOS and here elsewhere.
fn help_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Submenu<R>> {
    let about = if cfg!(target_os = "macos") {
        Vec::new()
    } else {
        entries(&["app:about", SEPARATOR])
    };
    let mut lines = about;
    lines.extend(entries(&["help:update-check"]));
    submenu(app, "도움말", lines)
}

/// Builds the whole menu bar.
pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    if cfg!(target_os = "macos") {
        menu.append(&app_menu(app)?)?;
    }
    menu.append(&file_menu(app)?)?;
    menu.append(&edit_menu(app)?)?;
    menu.append(&view_menu(app)?)?;
    menu.append(&insert_menu(app)?)?;
    menu.append(&format_menu(app)?)?;
    menu.append(&page_menu(app)?)?;
    menu.append(&table_menu(app)?)?;
    menu.append(&tool_menu(app)?)?;
    if cfg!(target_os = "macos") {
        menu.append(&window_menu(app)?)?;
    }
    menu.append(&help_menu(app)?)?;
    Ok(menu)
}

/// What the menu bar shows for the focused window.
pub struct MenuView<'a> {
    /// Theme mode of the window: `system`, `light` or `dark`.
    pub theme: &'a str,
    /// Ids of the table items the window's module cannot use.
    pub disabled: &'a HashSet<String>,
    /// The update-check switch: `None` when the app does not check at all (it is then disabled).
    pub update_check: Option<bool>,
}

/// `(checked, enabled)` of a check item, `None` for an id that is not one.
fn check_state(id: &str, view: &MenuView) -> Option<(bool, bool)> {
    if id == UPDATE_CHECK_ID {
        return Some((view.update_check == Some(true), view.update_check.is_some()));
    }
    let theme = theme_item_id(view.theme);
    is_check_item(id).then(|| (theme == Some(id), !view.disabled.contains(id)))
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
            MenuItemKind::MenuItem(plain) if ITEMS.iter().any(|spec| spec.id == id) => {
                plain.set_enabled(!view.disabled.contains(id))
            }
            _ => Ok(()),
        };
        failed = failed.take().or(result.err());
    })?;
    failed.map_or(Ok(()), Err)
}

/// Installs the menu bar and routes custom items to the focused window.
pub fn install(app: &mut tauri::App) -> tauri::Result<()> {
    let menu = build(app.handle())?;
    app.set_menu(menu)?;
    app.on_menu_event(|app, event| run_item(app, event.id().as_ref()));
    Ok(())
}

/// Runs a custom item in the window that has the focus; with no focused window nothing happens
/// (a menu click never reaches a window the user is not looking at).
fn run_item(app: &AppHandle, id: &str) {
    use tauri::Manager;
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
    if let Err(error) = crate::menu_state::refresh(app) {
        eprintln!("[menu] {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODIFIERS: &[&str] = &["CmdOrCtrl", "Alt", "Shift"];
    const GROUPS: &[&str] = &[
        "app", "file", "edit", "view", "insert", "format", "page", "table", "tool", "help",
    ];

    #[test]
    fn ids_are_unique_and_shaped_like_group_colon_name() {
        let mut seen = std::collections::HashSet::new();
        for spec in ITEMS {
            assert!(seen.insert(spec.id), "duplicate id {}", spec.id);
            let (group, name) = spec.id.split_once(':').expect("group:name");
            assert!(GROUPS.contains(&group), "{}", spec.id);
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
            let (name, argument) = spec.action.split_once(':').unwrap_or((spec.action, ""));
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
            QUIT_ID,
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

    #[test]
    fn the_menu_functions_use_exactly_the_ids_of_the_table() {
        let source = include_str!("native_menu.rs");
        let menus =
            &source[source.find("const SEPARATOR").unwrap()..source.find("#[cfg(test)]").unwrap()];
        let used: std::collections::BTreeSet<&str> = menus
            .split('"')
            .skip(1)
            .step_by(2)
            .filter(|text| {
                text.split_once(':')
                    .is_some_and(|(group, _)| GROUPS.contains(&group))
            })
            .collect();
        let table: std::collections::BTreeSet<&str> = ITEMS.iter().map(|spec| spec.id).collect();
        assert_eq!(used, table);
    }

    #[test]
    fn the_page_menu_commands_are_reached_through_the_page_item_actions() {
        let studio = ITEMS
            .iter()
            .filter(|spec| spec.action.starts_with("studio-cmd:"))
            .count();
        assert!(studio >= 60, "{studio} page menu commands");
        for spec in ITEMS.iter().filter(|spec| spec.id.starts_with("file:new-")) {
            assert!(spec.action.starts_with("window-new:"), "{}", spec.id);
        }
    }

    #[test]
    fn the_table_menu_has_the_reference_cell_items_in_order_and_the_editor_only_ones_say_so() {
        let source = include_str!("native_menu.rs");
        let table =
            &source[source.find("fn table_menu").unwrap()..source.find("fn tool_menu").unwrap()];
        let cells = [
            "\"table:cell-select\"",
            "\"table:merge\"",
            "\"table:cell-split\"",
        ];
        let at: Vec<usize> = cells.iter().map(|id| table.find(id).unwrap()).collect();
        assert!(
            at.windows(2).all(|pair| pair[0] < pair[1]),
            "select, merge, split"
        );
        let editor_only: Vec<&str> = ITEMS
            .iter()
            .filter(|spec| spec.action == "hwp-editor")
            .map(|spec| spec.id)
            .collect();
        assert_eq!(editor_only, ["table:cell-select", "table:cell-split"]);
        assert_eq!(
            spec("table:cell-select").accelerator,
            Some("CmdOrCtrl+Alt+T")
        );
        assert_eq!(spec("view:zoom-100").accelerator, Some("CmdOrCtrl+0"));
    }

    #[test]
    fn only_macos_leaves_paste_and_close_to_system_items() {
        let ids: Vec<&str> = installed_ids().collect();
        assert_eq!(MACOS_SYSTEM_IDS, ["edit:paste", "file:close"]);
        for id in MACOS_SYSTEM_IDS {
            assert!(ITEMS.iter().any(|spec| spec.id == *id), "{id}");
            assert_eq!(ids.contains(id), !cfg!(target_os = "macos"), "{id}");
        }
        for id in [
            "edit:undo",
            "edit:redo",
            "edit:cut",
            "edit:copy",
            "edit:find",
        ] {
            assert!(ids.contains(&id), "{id} is a custom item on every OS");
        }
        assert!(ids.contains(&"app:about"));
    }

    #[test]
    fn editing_keys_stay_with_the_focused_editor_except_the_macos_edit_menu_keys() {
        for spec in ITEMS
            .iter()
            .filter(|spec| spec.id.starts_with("format:") || spec.id.starts_with("edit:"))
            .filter(|spec| spec.id != "edit:find")
        {
            assert_eq!(spec.accelerator, None, "{}", spec.id);
            let bound = accelerator_of(spec);
            let macos_key = MACOS_EDIT_KEYS.iter().find(|(id, _)| *id == spec.id);
            assert_eq!(
                bound,
                macos_key
                    .filter(|_| cfg!(target_os = "macos"))
                    .map(|(_, keys)| *keys)
            );
        }
    }

    #[test]
    fn the_macos_edit_keys_are_the_reference_keys_of_four_table_items() {
        assert_eq!(
            MACOS_EDIT_KEYS,
            [
                ("edit:undo", "CmdOrCtrl+Z"),
                ("edit:redo", "CmdOrCtrl+Shift+Z"),
                ("edit:cut", "CmdOrCtrl+X"),
                ("edit:copy", "CmdOrCtrl+C"),
            ]
        );
        let taken: Vec<&str> = ITEMS.iter().filter_map(|spec| spec.accelerator).collect();
        for (id, keys) in MACOS_EDIT_KEYS {
            assert!(ITEMS.iter().any(|spec| spec.id == id), "{id}");
            assert!(!taken.contains(&keys), "{keys} is bound twice");
            assert!(!MACOS_SYSTEM_IDS.contains(&id), "{id} is a custom item");
        }
    }

    #[test]
    fn quit_runs_in_rust_with_the_reference_id_and_accelerator_and_is_not_a_table_item() {
        assert!(ITEMS.iter().all(|spec| spec.id != QUIT_ID));
        assert_eq!(dispatch_script(QUIT_ID), None);
        assert_eq!(action_for(QUIT_ID), None);
        assert_eq!(QUIT_ID, "app:quit");
        assert!(QUIT_ID.len() <= MAX_ID_LEN);
        assert_eq!(QUIT_ACCELERATOR, "CmdOrCtrl+Q");
        assert!(ITEMS
            .iter()
            .all(|spec| spec.accelerator != Some(QUIT_ACCELERATOR)));
        assert!(MACOS_EDIT_KEYS
            .iter()
            .all(|(_, keys)| *keys != QUIT_ACCELERATOR));
        assert_eq!(
            QUIT_LABEL,
            if cfg!(target_os = "macos") {
                "EasyOffice 끝내기"
            } else {
                "끝내기"
            }
        );
    }

    /// `run_item` hands the quit item to the queue on every OS (it once skipped macOS, where the
    /// id then fell through to `dispatch_script`, which has none, and ⌘Q did nothing).
    #[test]
    fn run_item_sends_the_quit_item_to_the_queue_on_every_os() {
        let source = include_str!("native_menu.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        let run_item = &production[production.find("fn run_item").unwrap()..];
        let quit = &run_item[..run_item.find("dispatch_script(id)").unwrap()];
        assert!(quit.contains("if id == QUIT_ID {"));
        assert!(quit.contains("crate::app_quit::request_app_quit(app);"));
        assert!(!quit.contains("cfg"), "no OS is skipped");
    }

    /// The macOS layout (source check, the menu itself needs a running app): the application menu
    /// ends in the custom quit item, not the system one that would end the process at once; the
    /// system items carry Korean labels; ⌘W belongs to the 윈도우 menu's 닫기 alone.
    #[test]
    fn the_macos_menus_have_the_custom_quit_korean_system_labels_and_one_cmd_w() {
        let source = include_str!("native_menu.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        let section = |from: &str, to: &str| {
            let start = production.find(from).unwrap();
            &production[start..start + production[start..].find(to).unwrap()]
        };
        let app = section("fn app_menu", "fn file_menu");
        assert!(app.contains(".item(&quit_item(app)?)"));
        assert!(!app.contains(".quit()") && !app.contains("PredefinedMenuItem::quit"));
        let file = section("fn file_menu", "fn edit_menu");
        let on_macos = &file[..file.find("if !cfg!(target_os = \"macos\")").unwrap()];
        assert!(!on_macos.contains("file:close") && !on_macos.contains("QUIT_ID"));
        let window = section("fn window_menu", "fn help_menu");
        assert!(window.contains("PredefinedMenuItem::close_window(app, Some(\"닫기\"))"));
        let edit = section("fn edit_menu", "fn view_menu");
        assert!(edit.contains("PredefinedMenuItem::paste") && edit.contains("select_all"));
        for item in ["undo", "redo", "cut", "copy"] {
            assert!(
                !edit.contains(&format!(".{item}()")),
                "{item} is the table's item"
            );
        }
        // No system item is built without its Korean text (the system language would show).
        for menu in [app, window, edit] {
            for call in menu.split("PredefinedMenuItem::").skip(1) {
                let args = &call[..call.find(")?").unwrap()];
                assert!(args.contains("Some("), "{args}");
            }
            for bare in [
                ".services()",
                ".hide()",
                ".hide_others()",
                ".show_all()",
                ".minimize()",
                ".select_all()",
            ] {
                assert!(!menu.contains(bare), "{bare}");
            }
        }
    }

    #[test]
    fn the_check_items_are_the_three_themes_and_the_update_switch_of_the_table() {
        for id in CHECK_IDS {
            assert!(ITEMS.iter().any(|spec| spec.id == id), "{id}");
        }
        for (mode, id) in THEME_ITEMS {
            assert_eq!(theme_item_id(mode), Some(id));
            assert!(CHECK_IDS.contains(&id));
            assert_eq!(action_for(id).unwrap(), format!("theme:{mode}"));
        }
        assert_eq!(theme_item_id("purple"), None);
        assert_eq!(CHECK_IDS.len(), THEME_ITEMS.len() + 1);
        assert_eq!(action_for("help:update-check"), Some("update-check"));
    }

    #[test]
    fn exactly_the_current_theme_is_checked_and_the_update_switch_follows_the_app() {
        let none = HashSet::new();
        let view = |theme, update_check| MenuView {
            theme,
            disabled: &none,
            update_check,
        };
        for mode in ["system", "light", "dark"] {
            let checked: Vec<&str> = CHECK_IDS
                .into_iter()
                .filter(|id| check_state(id, &view(mode, None)).is_some_and(|(on, _)| on))
                .collect();
            assert_eq!(checked, vec![theme_item_id(mode).unwrap()], "{mode}");
        }
        assert_eq!(
            check_state("help:update-check", &view("dark", Some(true))),
            Some((true, true))
        );
        assert_eq!(
            check_state("help:update-check", &view("dark", Some(false))),
            Some((false, true))
        );
        assert_eq!(
            check_state("help:update-check", &view("dark", None)),
            Some((false, false)),
            "an app that does not check offers no switch"
        );
        assert_eq!(check_state("file:save", &view("dark", None)), None);
    }
}

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
mod layout;
pub use layout::MenuKind;

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

/// A spreadsheet menu item that runs the page menu item `data-sheet-act="<act>"` (the long ones are written with
/// braces so the formatter keeps each on one line).
macro_rules! sa {
    ($id:literal, $label:literal, $act:literal) => {
        item($id, $label, None, concat!("sheet-act:", $act))
    };
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
    item(
        "file:export-pdf",
        "PDF로 내보내기…",
        Some("CmdOrCtrl+E"),
        "hwp-export-pdf",
    ),
    item("file:print", "인쇄…", Some("CmdOrCtrl+P"), "print"),
    item("file:close", "닫기", Some("CmdOrCtrl+W"), "window-close"),
    item("edit:undo", "되돌리기", None, "studio-cmd:studio-undo"),
    item("edit:redo", "다시 실행", None, "studio-cmd:studio-redo"),
    item("edit:cut", "오려 두기", None, "studio-cmd:clip-cut"),
    item("edit:copy", "복사하기", None, "studio-cmd:clip-copy"),
    item("edit:paste", "붙이기", None, "studio-cmd:clip-paste"),
    item(
        "edit:paste-special",
        "골라 붙이기…",
        None,
        "sheet-paste:special",
    ),
    item("edit:find", "찾기", Some("CmdOrCtrl+F"), "find"),
    item("edit:replace", "찾아 바꾸기", None, "replace"),
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
    item("view:zoom-dialog", "확대/축소…", None, "hwp-editor"),
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
        "view:toolbar-labels",
        "도구 상자 글자 표시",
        None,
        "hwp-editor",
    ),
    item("view:form-mode", "양식 모드", None, "hwp-editor"),
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
        "insert:toc-update",
        "목차 업데이트",
        None,
        "studio-cmd:doc-mod-toc-update",
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
    item("format:style", "스타일 적용", None, "studio-cmd:home-style"),
    item("format:justify", "양쪽 정렬", None, "studio-cmd:para-shape"),
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
    item(
        "format:numfmt-general",
        "일반",
        None,
        "sheet-numfmt:general",
    ),
    item("format:numfmt-number", "숫자", None, "sheet-numfmt:number"),
    item(
        "format:numfmt-currency",
        "통화",
        None,
        "sheet-numfmt:currency",
    ),
    item(
        "format:numfmt-accounting",
        "회계",
        None,
        "sheet-numfmt:accounting",
    ),
    item(
        "format:numfmt-percent",
        "백분율",
        None,
        "sheet-numfmt:percent",
    ),
    item("format:numfmt-date", "날짜", None, "sheet-numfmt:date"),
    item("format:numfmt-time", "시간", None, "sheet-numfmt:time"),
    item("format:numfmt-text", "텍스트", None, "sheet-numfmt:text"),
    item(
        "format:numfmt-decimal-up",
        "자릿수 늘림",
        None,
        "sheet-numfmt:decimal-up",
    ),
    item(
        "format:numfmt-decimal-down",
        "자릿수 줄임",
        None,
        "sheet-numfmt:decimal-down",
    ),
    item(
        "format:align-left",
        "왼쪽 정렬",
        None,
        "sheet-look:align-left",
    ),
    item(
        "format:align-center",
        "가운데 정렬",
        None,
        "sheet-look:align-center",
    ),
    item(
        "format:align-right",
        "오른쪽 정렬",
        None,
        "sheet-look:align-right",
    ),
    item(
        "format:valign-top",
        "위쪽 맞춤",
        None,
        "sheet-look:valign-top",
    ),
    item(
        "format:valign-middle",
        "가운데 맞춤",
        None,
        "sheet-look:valign-middle",
    ),
    item(
        "format:valign-bottom",
        "아래쪽 맞춤",
        None,
        "sheet-look:valign-bottom",
    ),
    item("format:wrap", "줄 바꿈", None, "sheet-look:wrap"),
    item("format:border-all", "모두", None, "sheet-look:border-all"),
    item(
        "format:border-outer",
        "바깥쪽",
        None,
        "sheet-look:border-outer",
    ),
    item(
        "format:border-inner",
        "안쪽",
        None,
        "sheet-look:border-inner",
    ),
    item("format:border-top", "위쪽", None, "sheet-look:border-top"),
    item(
        "format:border-bottom",
        "아래쪽",
        None,
        "sheet-look:border-bottom",
    ),
    item("format:border-left", "왼쪽", None, "sheet-look:border-left"),
    item(
        "format:border-right",
        "오른쪽",
        None,
        "sheet-look:border-right",
    ),
    item("format:border-none", "없음", None, "sheet-look:border-none"),
    item(
        "format:border-line-thin",
        "가는 실선",
        None,
        "sheet-look:line-thin",
    ),
    item(
        "format:border-line-thick",
        "굵은 실선",
        None,
        "sheet-look:line-thick",
    ),
    item(
        "format:border-line-dashed",
        "파선",
        None,
        "sheet-look:line-dashed",
    ),
    item(
        "format:border-line-dotted",
        "점선",
        None,
        "sheet-look:line-dotted",
    ),
    item(
        "format:border-line-double",
        "이중 실선",
        None,
        "sheet-look:line-double",
    ),
    item(
        "format:border-color",
        "테두리 색…",
        None,
        "sheet-look:dialog-border",
    ),
    item(
        "format:cell-format",
        "셀 서식…",
        None,
        "sheet-numfmt:dialog",
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
        "page:break",
        "쪽 나누기",
        Some("CmdOrCtrl+Enter"),
        "studio-cmd:doc-mod-page-break",
    ),
    item("page:header", "머리말", None, "studio-cmd:mod-header"),
    item("page:footer", "꼬리말", None, "studio-cmd:mod-footer"),
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
        "table:row-above",
        "위쪽에 줄 추가하기",
        None,
        "studio-cmd:table-row-above",
    ),
    item(
        "table:add-row",
        "아래쪽에 줄 추가하기",
        None,
        "studio-cmd:table-add-row",
    ),
    item(
        "table:col-left",
        "왼쪽에 칸 추가하기",
        None,
        "studio-cmd:table-col-left",
    ),
    item(
        "table:col-right",
        "오른쪽에 칸 추가하기",
        None,
        "studio-cmd:table-col-right",
    ),
    item(
        "table:row-delete",
        "줄 지우기",
        None,
        "studio-cmd:table-row-delete",
    ),
    item(
        "table:col-delete",
        "칸 지우기",
        None,
        "studio-cmd:table-col-delete",
    ),
    item(
        "table:cell-select",
        "셀 선택",
        Some("CmdOrCtrl+Alt+T"),
        "hwp-editor",
    ),
    item("table:merge", "셀 합치기", None, "studio-cmd:table-merge"),
    item("table:cell-split", "셀 나누기", None, "hwp-editor"),
    item("table:split", "셀 나누기", None, "studio-cmd:table-split"),
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
        "선택 행/열 고정",
        None,
        "studio-cmd:sheet-mod-freeze",
    ),
    sa!("sheet:freeze-first-row", "첫 행 고정", "freeze-first-row"),
    sa!("sheet:freeze-first-col", "첫 열 고정", "freeze-first-col"),
    sa!("sheet:freeze-clear", "틀 고정 해제", "freeze-clear"),
    sa!("sheet:tab-insert", "시트 삽입", "tab-insert"),
    sa!("sheet:tab-delete", "시트 삭제", "tab-delete"),
    sa!("sheet:tab-rename", "시트 이름 바꾸기", "tab-rename"),
    sa!("sheet:tab-copy", "시트 복사", "tab-copy"),
    sa!("sheet:tab-left", "시트 왼쪽으로 이동", "tab-left"),
    sa!("sheet:tab-right", "시트 오른쪽으로 이동", "tab-right"),
    sa!("sheet:tab-hide", "시트 숨기기", "tab-hide"),
    sa!("sheet:tab-unhide", "시트 숨기기 취소…", "tab-unhide"),
    sa!("sheet:name-define", "이름 정의…", "name-define"),
    sa!("sheet:name-manage", "이름 관리…", "name-manage"),
    sa!("sheet:protect", "시트 보호", "sheet-protect"),
    sa!(
        "sheet:protect-workbook",
        "통합 문서 보호",
        "workbook-protect"
    ),
    sa!("sheet:protect-ranges", "범위 편집 허용", "range-allow"),
    sa! { "sheet:name-create", "선택 영역에서 이름 만들기…", "name-create" },
    sa!("sheet:page-setup", "쪽 설정…", "page-setup"),
    sa!("sheet:print-area-set", "인쇄 영역 설정", "print-area-set"),
    sa! { "sheet:print-area-clear", "인쇄 영역 해제", "print-area-clear" },
    sa!("sheet:group", "그룹 묶기", "group"),
    sa!("sheet:ungroup", "그룹 풀기", "ungroup"),
    sa!("sheet:hide-detail", "하위 수준 숨기기", "hide-detail"),
    sa!("sheet:show-detail", "하위 수준 표시", "show-detail"),
    sa!("sheet:outline-clear", "윤곽 지우기", "outline-clear"),
    sa!("sheet:row-insert-above", "행 위에 삽입", "row-insert-above"),
    sa! { "sheet:row-insert-below", "행 아래에 삽입", "row-insert-below" },
    sa!("sheet:col-insert-left", "열 왼쪽에 삽입", "col-insert-left"),
    sa! { "sheet:col-insert-right", "열 오른쪽에 삽입", "col-insert-right" },
    sa!("sheet:row-delete", "행 삭제", "row-delete"),
    sa!("sheet:col-delete", "열 삭제", "col-delete"),
    sa!("sheet:hide", "숨기기", "hide"),
    sa!("sheet:unhide", "숨기기 취소", "unhide"),
    sa!("sheet:merge", "셀 병합", "merge"),
    sa!("sheet:unmerge", "병합 해제", "unmerge"),
    sa!("sheet:sort", "정렬…", "sort-dialog"),
    sa!("sheet:filter", "필터", "filter-toggle"),
    sa!("sheet:filter-reapply", "다시 적용", "filter-reapply"),
    sa!("sheet:filter-clear", "필터 지우기", "filter-clear"),
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

const _: () = assert!(ITEMS.len() <= ItemMask::CAPACITY);

/// A set of indexes of [`ITEMS`] (the items a window cannot use).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ItemMask([u128; 2]);

impl ItemMask {
    /// How many indexes the set can hold.
    pub const CAPACITY: usize = 2 * u128::BITS as usize;

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
}

/// `(checked, enabled)` of a check item, `None` for an id that is not one.
fn check_state(id: &str, view: &MenuView) -> Option<(bool, bool)> {
    if id == UPDATE_CHECK_ID {
        return Some((view.update_check == Some(true), view.update_check.is_some()));
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

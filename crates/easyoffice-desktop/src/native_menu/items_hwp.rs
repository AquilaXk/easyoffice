//! The items of the Hangul editor window's menu bar that only the editor can run (PD-396).
//!
//! Each one is a command the editor's own menu row already has (`rhwp-studio`, MIT); the native bar names it
//! with its official 한글 menu text and sends it over the embed port (`automation.execute`, the page's
//! `EMBED_EDITOR_COMMANDS`). Nothing here is new behavior: the bar is the same commands in the official order.

use super::items::item;
use super::MenuItemSpec;

/// A command of the editor: no page action of its own, so it is grayed out outside the Hangul editor window.
const fn editor(id: &'static str, label: &'static str) -> MenuItemSpec {
    item(id, label, None, "hwp-editor")
}

/// The text the Hangul editor window shows for an item of the shared table: the official 한글 menu text
/// (the help pages of the menus), where the shared text is another program's word.
pub fn label(id: &str) -> Option<&'static str> {
    Some(match id {
        "edit:undo" => "되돌리기",
        "edit:cut" => "오려 두기",
        "edit:copy" => "복사하기",
        "edit:paste" => "붙이기",
        "edit:replace" => "찾아 바꾸기",
        "edit:goto" => "찾아가기",
        "view:zoom-100" => "실제 크기",
        "view:zoom-dialog" => "화면 확대/축소",
        "view:ctrl-marks" => "조판 부호",
        "view:para-marks" => "문단 부호",
        "view:doc-grid" => "격자 보기",
        "format:bold" => "진하게",
        "format:italic" => "기울임",
        "format:style" => "스타일",
        "format:para-numbering" => "문단 번호 적용/해제",
        "format:para-bullet" => "글머리표 적용/해제",
        "format:indent-in" => "한 수준 증가",
        "format:indent-out" => "한 수준 감소",
        "table:border-bg" => "각 셀마다 적용",
        "tool:scrub-pii" => "개인 정보 보호",
        "tool:review-comment" => "메모 넣기",
        "tool:palette" => "명령 검색",
        _ => return None,
    })
}

pub const HWP_ITEMS: &[MenuItemSpec] = &[
    editor("edit:format-copy", "모양 복사"),
    editor("edit:delete", "지우기"),
    editor("edit:find-again", "다시 찾기"),
    editor("view:border-transparent", "투명 선"),
    editor("view:clip", "잘림 보기"),
    editor("view:grid-settings", "격자 설정"),
    editor("view:toolbox-basic", "기본"),
    editor("view:toolbox-format", "서식"),
    editor("insert:textbox", "글상자"),
    editor("insert:equation", "수식"),
    editor("insert:bookmark", "책갈피"),
    editor("insert:endnote-shape", "미주 모양"),
    editor("insert:caption-top", "위"),
    editor("insert:caption-lt", "왼쪽 위"),
    editor("insert:caption-lm", "왼쪽 가운데"),
    editor("insert:caption-lb", "왼쪽 아래"),
    editor("insert:caption-rt", "오른쪽 위"),
    editor("insert:caption-rm", "오른쪽 가운데"),
    editor("insert:caption-rb", "오른쪽 아래"),
    editor("insert:caption-bottom", "아래"),
    editor("insert:caption-none", "캡션 없음"),
    editor("insert:rotate-cw", "오른쪽 90° 회전"),
    editor("insert:rotate-ccw", "왼쪽 90° 회전"),
    editor("insert:flip-horz", "좌우 대칭"),
    editor("insert:flip-vert", "상하 대칭"),
    editor("insert:picture-delete", "개체 지우기"),
    editor("format:hwp-object-props", "개체 속성"),
    editor("format:hwp-font-up", "글자 크기 크게"),
    editor("format:hwp-font-down", "글자 크기 작게"),
    editor("format:hwp-align-left", "왼쪽 정렬"),
    editor("format:hwp-align-center", "가운데 정렬"),
    editor("format:hwp-align-right", "오른쪽 정렬"),
    editor("format:hwp-align-distribute", "배분 정렬"),
    editor("format:hwp-line-up", "줄 간격 늘림"),
    editor("format:hwp-line-down", "줄 간격 줄임"),
    editor("page:page-border", "쪽 테두리/배경"),
    editor("page:new-number", "새 번호로 시작"),
    editor("page:hide-current", "현재 쪽만 감추기"),
    editor("page:column-break", "단 나누기"),
    editor("page:section-settings", "구역 설정"),
    editor("table:insert-row-col", "줄/칸 추가하기"),
    editor("table:delete-row-col", "줄/칸 지우기"),
    editor("table:border-one", "하나의 셀처럼 적용"),
    editor("table:row-equal", "셀 높이를 같게"),
    editor("table:transpose-copy", "행/열 바꿈 복사"),
    editor("table:transpose-paste", "행/열 바꿈 붙여넣기"),
    editor("table:block-formula", "블록 계산식"),
    editor("table:block-avg", "블록 평균"),
    editor("table:block-product", "블록 곱"),
    editor("table:thousand-sep", "1,000 단위 구분 쉼표"),
    editor("table:decimal-add", "자릿점 넣기"),
    editor("table:decimal-remove", "자릿점 빼기"),
    editor("file:set-password", "문서 암호 설정…"),
    editor("tool:options", "환경 설정"),
    editor("tool:review-history", "문서 이력 관리"),
];

//! The items of the Pages document window's menu bar that no other window has, and the text and keys the window
//! gives the shared items (PD-399).
//!
//! The names are the ones of the reference Pages app's Korean help (프린트, 실행 복귀, 오려두기, 찾기 및 대치,
//! 보이지 않는 요소 보기, 이미지, 좌우 정렬, 주석, 단어 수 보기, 서체 윈도우, 하이라이트, 북마크). Each new item runs a
//! command the document window already has: the date and time, the bookmark, the watermark and the picture layout
//! commands come from the word processor (`doc-word-*.js`), the highlight and the capitals from the run model.

use super::items::item;
use super::MenuItemSpec;

macro_rules! sc {
    ($id:literal, $label:literal, $cmd:literal) => {
        item($id, $label, None, concat!("studio-cmd:", $cmd))
    };
}

pub const PAGES_ITEMS: &[MenuItemSpec] = &[
    sc!("insert:datetime", "날짜 및 시간…", "insert-datetime"),
    sc!("insert:pages-bookmark", "북마크…", "insert-bookmark"),
    sc!("format:highlight", "하이라이트", "home-highlight"),
    sc!("format:case-none", "없음", "text-case-none"),
    sc!("format:case-all", "모두 대문자", "text-case-all"),
    sc!("format:case-small", "작은 대문자", "text-case-small"),
    sc!("arrange:wrap-inline", "줄 안", "image-wrap-inline"),
    sc!("arrange:wrap-square", "정사각형", "image-wrap-square"),
    sc!(
        "arrange:wrap-top-bottom",
        "위/아래",
        "image-wrap-top-bottom"
    ),
    sc!("arrange:wrap-behind", "텍스트 뒤", "image-wrap-behind"),
    sc!("arrange:wrap-front", "텍스트 앞", "image-wrap-front"),
    sc!("arrange:crop", "자르기", "image-crop"),
    sc!(
        "arrange:rotate-right",
        "오른쪽으로 90° 회전",
        "image-rotate-right"
    ),
    sc!(
        "arrange:rotate-left",
        "왼쪽으로 90° 회전",
        "image-rotate-left"
    ),
    sc!("arrange:watermark", "워터마크…", "layout-watermark"),
];

/// The text the Pages window shows for an item of the shared table.
pub fn label(id: &str) -> Option<&'static str> {
    Some(match id {
        "file:print" => "프린트…",
        "file:print-preview" => "프린트 미리 보기",
        "edit:redo" => "실행 복귀",
        "edit:cut" => "오려두기",
        "edit:replace" => "찾기 및 대치",
        "view:para-marks" => "보이지 않는 요소 보기",
        "tool:review-stats" => "단어 수 보기",
        "tool:review-comment" => "주석 추가",
        "insert:image" => "이미지",
        "insert:link" => "링크",
        "format:bold" => "볼드체",
        "format:italic" => "이탤릭체",
        "format:underline" => "밑줄체",
        "format:font-dialog" => "서체 윈도우",
        "format:para-numbering" => "번호 매기기",
        "format:para-bullet" => "글머리 기호",
        "format:indent-in" => "들여쓰기 단계 증가",
        "format:indent-out" => "들여쓰기 단계 감소",
        "page:columns" => "단",
        "page:break" => "페이지 나누기",
        "table:insert" => "표 삽입",
        "table:row-above" => "위에 행 추가",
        "table:add-row" => "아래에 행 추가",
        "table:col-left" => "앞에 열 추가",
        "table:col-right" => "뒤에 열 추가",
        "table:row-delete" => "행 삭제",
        "table:col-delete" => "열 삭제",
        "table:split" => "셀 병합 해제",
        "table:merge" => "셀 병합",
        "table:col-equal" => "같은 너비로 열 조절",
        "table:border-bg" => "테두리 및 음영",
        "table:cell-props" => "표 속성",
        _ => return None,
    })
}

/// The key the Pages window gives an item (the reference app's keys; the others keep the table's).
pub fn accelerator(id: &str) -> Option<&'static str> {
    Some(match id {
        "format:font-dialog" => "CmdOrCtrl+T",
        "format:highlight" => "CmdOrCtrl+Shift+H",
        "insert:pages-bookmark" => "CmdOrCtrl+Alt+B",
        "tool:review-stats" => "CmdOrCtrl+Shift+W",
        "view:para-marks" => "CmdOrCtrl+Shift+I",
        _ => return None,
    })
}

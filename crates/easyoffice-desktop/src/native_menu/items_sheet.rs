//! The items of the spreadsheet window's menu bar that came with PD-400 (`items.rs` is at its line ceiling): the 지우기
//! list, 서식 복사 and 서식 붙여넣기, 수식 표시. Each runs the page menu item `data-sheet-act="<act>"` of the same name, as
//! the `sa!` items of `items.rs` do.

use super::items::item;
use super::MenuItemSpec;

macro_rules! sa {
    ($id:literal, $label:literal, $act:literal) => {
        item($id, $label, None, concat!("sheet-act:", $act))
    };
}

pub const SHEET_ITEMS: &[MenuItemSpec] = &[
    sa!("sheet:clear-all", "모두 지우기", "clear-all"),
    sa!("sheet:clear-formats", "서식 지우기", "clear-formats"),
    sa!("sheet:clear-contents", "내용 지우기", "clear-contents"),
    sa!("sheet:clear-notes", "메모 지우기", "clear-notes"),
    sa!("sheet:clear-links", "하이퍼링크 지우기", "clear-links"),
    sa!("sheet:format-painter", "서식 복사", "format-painter"),
    sa!("sheet:format-paste", "서식 붙여넣기", "format-paste"),
    sa!("sheet:view-formulas", "수식 표시", "view-formulas"),
    sa!("sheet:adv-filter", "고급 필터…", "adv-filter"),
    sa!("sheet:goal-seek", "목표값 찾기…", "goal-seek"),
    sa!("sheet:cell-styles", "셀 스타일…", "cell-styles"),
];

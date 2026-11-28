//! The Pages document window's bar (PD-190, PD-399).

use super::*;

/// The words of other products that the Pages bar must not show: the reference app's names are 프린트, 실행 복귀, 오려두기,
/// 찾기 및 대치, 보이지 않는 요소 보기, 이미지, 좌우 정렬, 주석, 단어 수 보기, 북마크.
const OTHER_PRODUCTS: [&str; 14] = [
    "인쇄 미리",
    "인쇄…",
    "다시 실행",
    "잘라내기",
    "바꾸기",
    "서식 기호",
    "편집 기호",
    "문서 통계",
    "새 메모",
    "양쪽 맞춤",
    "표 만들기",
    "책갈피",
    "하이퍼링크",
    "성능 진단",
];

/// Every id of the Pages bar with its submenus opened up.
fn bar_ids() -> Vec<&'static str> {
    layout::table_ids(MenuKind::Pages)
}

#[test]
fn the_bar_follows_the_reference_menu_order_with_the_table_menu_under_format() {
    assert_eq!(
        titles_of(MenuKind::Pages),
        [
            "EasyOffice",
            "파일",
            "편집",
            "삽입",
            "포맷",
            "정렬",
            "보기",
            "도구",
            "윈도우",
            "도움말"
        ]
    );
    let format = lines_of(MenuKind::Pages, "포맷");
    assert_eq!(format.last().copied(), Some("@pages-table"));
    let table = layout::pages_submenu("@pages-table").expect("the 표 submenu");
    assert_eq!(table.title, "표");
    assert!(
        !table.lines.contains(&"table:insert"),
        "표 삽입 is one item, in 삽입"
    );
    assert!(lines_of(MenuKind::Pages, "삽입").contains(&"table:insert"));
    assert!(!lines_of(MenuKind::Pages, "도구").contains(&"tool:bench-run"));
    assert!(
        lines_of(MenuKind::Pages, "보기").contains(&"tool:review-stats"),
        "단어 수 보기 is in 보기"
    );
}

#[test]
fn the_new_items_run_the_commands_the_document_window_already_has() {
    let action = |id: &str| {
        ITEMS
            .iter()
            .find(|spec| spec.id == id)
            .map(|spec| spec.action)
    };
    for (id, cmd) in [
        ("insert:datetime", "insert-datetime"),
        ("insert:pages-bookmark", "insert-bookmark"),
        ("arrange:watermark", "layout-watermark"),
        ("format:highlight", "home-highlight"),
        ("format:case-all", "text-case-all"),
        ("format:case-small", "text-case-small"),
        ("format:case-none", "text-case-none"),
        ("format:case-title", "text-case-title"),
        ("arrange:wrap-front", "image-wrap-front"),
        ("page:break", "doc-mod-page-break"),
        ("format:font-dialog", "font-dialog"),
    ] {
        assert_eq!(
            action(id),
            Some(format!("studio-cmd:{cmd}").as_str()),
            "{id}"
        );
    }
}

#[test]
fn the_bar_uses_the_reference_names_and_keys() {
    let spec = |id: &str| ITEMS.iter().find(|spec| spec.id == id).expect(id);
    for (id, label) in [
        ("file:print", "프린트…"),
        ("file:print-preview", "프린트 미리 보기"),
        ("edit:redo", "실행 복귀"),
        ("edit:cut", "오려두기"),
        ("edit:replace", "찾기 및 대치"),
        ("view:para-marks", "보이지 않는 요소 보기"),
        ("tool:review-stats", "단어 수 보기"),
        ("tool:review-comment", "주석 추가"),
        ("insert:image", "이미지"),
        ("insert:pages-bookmark", "북마크…"),
        ("insert:datetime", "날짜 및 시간…"),
        ("format:font-dialog", "서체 윈도우"),
        ("format:highlight", "하이라이트"),
        ("format:pages-justify", "좌우 정렬"),
        ("page:break", "페이지 나누기"),
        ("table:insert", "표 삽입"),
    ] {
        assert_eq!(label_for(MenuKind::Pages, spec(id)), label, "{id}");
    }
    for (id, keys) in [
        ("format:font-dialog", "CmdOrCtrl+T"),
        ("format:highlight", "CmdOrCtrl+Shift+H"),
        ("insert:pages-bookmark", "CmdOrCtrl+Alt+B"),
        ("tool:review-stats", "CmdOrCtrl+Shift+W"),
        ("view:para-marks", "CmdOrCtrl+Shift+I"),
        ("page:break", "CmdOrCtrl+Enter"),
        ("edit:replace", "Ctrl+H"),
        ("edit:redo", "CmdOrCtrl+Shift+Z"),
    ] {
        assert_eq!(
            accelerator_for(MenuKind::Pages, spec(id)),
            Some(keys),
            "{id}"
        );
    }
    // The other windows keep their own names and keys.
    assert_eq!(
        label_for(MenuKind::Doc, spec("tool:review-comment")),
        "새 메모"
    );
    assert_eq!(
        accelerator_for(MenuKind::Doc, spec("format:font-dialog")),
        None
    );
    assert_eq!(
        accelerator_for(MenuKind::Sheet, spec("table:insert")),
        Some("CmdOrCtrl+T")
    );
}

#[test]
fn no_label_of_the_bar_is_a_word_of_another_product_and_no_key_repeats() {
    let mut keys = std::collections::HashSet::new();
    for id in bar_ids() {
        let spec = ITEMS.iter().find(|spec| spec.id == id).expect(id);
        let label = label_for(MenuKind::Pages, spec);
        for word in OTHER_PRODUCTS {
            assert!(!label.contains(word), "{id}: {label}");
        }
        if let Some(accelerator) = accelerator_for(MenuKind::Pages, spec) {
            assert!(keys.insert(accelerator), "{accelerator} repeats ({id})");
        }
    }
    // The reference app's comment key is bound to 주석 추가 and the copilot has no key in this window (PD-400); the other
    // windows keep Shift+K for the copilot.
    assert_eq!(
        accelerator_for(MenuKind::Pages, spec("tool:review-comment")),
        Some("CmdOrCtrl+Shift+K")
    );
    assert_eq!(accelerator_for(MenuKind::Pages, spec("tool:copilot")), None);
    assert_eq!(
        accelerator_for(MenuKind::Doc, spec("tool:copilot")),
        Some("CmdOrCtrl+Shift+K")
    );
}

#[test]
fn the_item_mask_holds_every_item_with_room_to_grow() {
    assert!(ITEMS.len() <= ItemMask::CAPACITY);
    assert!(
        ItemMask::CAPACITY - ITEMS.len() >= 64,
        "room for the next windows' items"
    );
    let mut mask = ItemMask::default();
    let last = ITEMS.len() - 1;
    mask.insert(last);
    assert!(mask.contains(last));
    assert!(!mask.contains(last - 1));
}

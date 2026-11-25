use super::*;

#[test]
fn the_spreadsheet_bar_has_the_number_format_and_row_column_items_in_the_page_order() {
    let s = layout::SEPARATOR;
    assert_eq!(
        lines_of(MenuKind::Sheet, "서식"),
        [
            "format:bold",
            "format:italic",
            "format:underline",
            s,
            "format:style",
            s,
            layout::NUMFMT,
            layout::ALIGN,
            "format:wrap",
            layout::BORDER,
            layout::CF,
            "format:cell-format",
        ]
    );
    let labels: Vec<&str> = layout::NUMFMT_LINES
        .iter()
        .map(|id| if *id == s { s } else { spec(id).label })
        .collect();
    assert_eq!(
        labels,
        [
            "일반",
            "숫자",
            "통화",
            "회계",
            "백분율",
            "날짜",
            "시간",
            "텍스트",
            s,
            "자릿수 늘림",
            "자릿수 줄임",
        ]
    );
    let align: Vec<&str> = layout::ALIGN_LINES
        .iter()
        .map(|id| if *id == s { s } else { spec(id).label })
        .collect();
    assert_eq!(
        align,
        [
            "왼쪽 맞춤",
            "가운데 맞춤",
            "오른쪽 맞춤",
            s,
            "위쪽 맞춤",
            "가운데 맞춤",
            "아래쪽 맞춤",
        ]
    );
    let border: Vec<&str> = layout::BORDER_LINES
        .iter()
        .map(|id| if *id == s { s } else { spec(id).label })
        .collect();
    assert_eq!(
        border,
        [
            "모두",
            "바깥쪽",
            "안쪽",
            "위쪽",
            "아래쪽",
            "왼쪽",
            "오른쪽",
            "없음",
            s,
            "가는 실선",
            "굵은 실선",
            "파선",
            "점선",
            "이중 실선",
            s,
            "테두리 색…",
        ]
    );
    let sheet: Vec<&str> = lines_of(MenuKind::Sheet, "시트")
        .iter()
        .map(|id| if *id == s { s } else { spec(id).label })
        .collect();
    assert_eq!(
        sheet,
        [
            "첫 행 고정",
            "첫 열 고정",
            "틀 고정",
            "틀 고정 해제",
            s,
            "행 위에 삽입",
            "행 아래에 삽입",
            "열 왼쪽에 삽입",
            "열 오른쪽에 삽입",
            "행 삭제",
            "열 삭제",
            s,
            "숨기기",
            "숨기기 취소",
            s,
            "셀 병합",
            "병합 해제",
            s,
            "시트 삽입",
            "시트 삭제",
            "시트 이름 바꾸기",
            "시트 복사",
            "시트 이동/복사…",
            "시트 왼쪽으로 이동",
            "시트 오른쪽으로 이동",
            "시트 숨기기",
            "시트 숨기기 취소…",
        ]
    );
    for kind in [
        MenuKind::Doc,
        MenuKind::Hwp,
        MenuKind::Slide,
        MenuKind::Pdf,
        MenuKind::Base,
        MenuKind::Board,
    ] {
        let ids = layout::table_ids(kind);
        assert!(
            ids.iter().all(|id| !id.starts_with("sheet:")
                && !id.starts_with("format:numfmt-")
                && !id.starts_with("format:align-")
                && !id.starts_with("format:valign-")
                && !id.starts_with("format:border-")
                && !id.starts_with("format:cf-")
                && *id != "format:wrap"),
            "{kind:?}"
        );
    }
}

#[test]
fn the_conditional_formatting_menu_has_the_submenus_and_items_of_the_page() {
    let s = layout::SEPARATOR;
    let top: Vec<&str> = layout::CF_LINES
        .iter()
        .map(|line| match layout::cf_submenu(line) {
            Some(sub) => sub.title,
            None if *line == s => s,
            None => spec(line).label,
        })
        .collect();
    assert_eq!(
        top,
        [
            "셀 강조 규칙",
            "상위/하위 규칙",
            "데이터 막대",
            "색조",
            "아이콘 집합",
            s,
            "새 규칙…",
            "규칙 지우기",
            "규칙 관리…",
        ]
    );
    let counts: Vec<usize> = layout::CF_SUBMENUS
        .iter()
        .map(|sub| sub.lines.iter().filter(|line| **line != s).count())
        .collect();
    assert_eq!(counts, [8, 7, 13, 13, 21, 4]);
    // The id of an item is `format:cf-` and the argument of its page action, so the two cannot drift apart.
    let ids = layout::table_ids(MenuKind::Sheet);
    let cf: Vec<&str> = ids
        .iter()
        .copied()
        .filter(|id| id.starts_with("format:cf-"))
        .collect();
    assert_eq!(cf.len(), 8 + 7 + 13 + 13 + 21 + 4 + 2);
    for id in cf {
        let argument = &id["format:cf-".len()..];
        assert_eq!(
            action_for(id),
            Some(format!("sheet-cf:{argument}").as_str())
        );
    }
}

#[test]
fn the_sheet_items_run_the_page_menu_items_through_a_quoted_static_id() {
    for id in [
        "format:numfmt-decimal-down",
        "format:cell-format",
        "format:align-left",
        "format:border-line-double",
        "sheet:row-insert-above",
        "sheet:unmerge",
    ] {
        let action = action_for(id).unwrap();
        assert!(
            action.starts_with("sheet-numfmt:")
                || action.starts_with("sheet-act:")
                || action.starts_with("sheet-look:")
                || action.starts_with("sheet-cf:"),
            "{id}"
        );
        let script = dispatch_script(id).unwrap();
        assert_eq!(script, format!("{SCRIPT_PREFIX}\"{id}\");"));
    }
    assert!(ITEMS.len() <= ItemMask::CAPACITY);
}

#[test]
fn paste_special_follows_the_system_paste_in_the_spreadsheet_edit_menu_only() {
    let lines = lines_of(MenuKind::Sheet, "편집");
    let paste = lines.iter().position(|id| *id == layout::PASTE).unwrap();
    assert_eq!(lines[paste + 1], "edit:paste-special");
    let item = spec("edit:paste-special");
    assert_eq!(item.label, "선택하여 붙여넣기…");
    assert_eq!(
        action_for("edit:paste-special"),
        Some("sheet-paste:special")
    );
    assert_eq!(
        dispatch_script("edit:paste-special").unwrap(),
        format!("{SCRIPT_PREFIX}\"edit:paste-special\");")
    );
    for kind in [
        MenuKind::Doc,
        MenuKind::Hwp,
        MenuKind::Slide,
        MenuKind::Pdf,
        MenuKind::Base,
        MenuKind::Board,
    ] {
        assert!(
            !lines_of(kind, "편집").contains(&"edit:paste-special"),
            "{kind:?}"
        );
    }
}

#[test]
fn base_and_board_isolate_workspace_menus_without_doc_leaks() {
    for kind in [MenuKind::Base, MenuKind::Board] {
        let view = lines_of(kind, "보기");
        assert_eq!(
            view,
            [
                "view:zoom-in",
                "view:zoom-out",
                "view:zoom-100",
                layout::SEPARATOR,
                layout::THEME,
                "view:theme-toggle",
                layout::SEPARATOR,
                "view:toolbar-labels",
            ],
            "{kind:?} view"
        );
        let tool = lines_of(kind, "도구");
        assert_eq!(tool, ["tool:copilot", layout::SEPARATOR, "tool:palette"]);
    }
    assert!(lines_of(MenuKind::Base, "편집").contains(&"edit:find"));
    assert!(lines_of(MenuKind::Base, "편집").contains(&"edit:replace"));
    assert!(!lines_of(MenuKind::Board, "편집").contains(&"edit:find"));
    assert!(!lines_of(MenuKind::Board, "편집").contains(&"edit:replace"));
}

#[test]
fn official_korean_labels_separate_doc_and_hwp_and_match_menu_terms() {
    assert_eq!(spec("insert:chart").label, "차트");
    assert_eq!(spec("insert:mailmerge").label, "편지 병합");
    assert_eq!(spec("page:orientation").label, "용지 방향");
    assert_eq!(spec("page:margins").label, "여백");
    assert_eq!(spec("page:doc-paper").label, "용지 설정");
    assert_eq!(spec("page:doc-header").label, "머리글");
    assert_eq!(spec("page:doc-footer").label, "바닥글");
    assert_eq!(spec("page:paper").label, "편집 용지");
    assert_eq!(spec("page:header").label, "머리말");
    assert_eq!(spec("page:footer").label, "꼬리말");
    let doc_page = lines_of(MenuKind::Doc, "쪽");
    assert!(doc_page.contains(&"page:doc-paper"));
    assert!(doc_page.contains(&"page:doc-header"));
    assert!(doc_page.contains(&"page:doc-footer"));
    assert!(!doc_page.contains(&"page:paper"));
    assert!(!doc_page.contains(&"page:header"));
    assert!(!doc_page.contains(&"page:footer"));
    let hwp_page = lines_of(MenuKind::Hwp, "쪽");
    assert!(hwp_page.contains(&"page:paper"));
    assert!(hwp_page.contains(&"page:header"));
    assert!(hwp_page.contains(&"page:footer"));
    assert!(!hwp_page.contains(&"page:doc-paper"));
    assert!(!hwp_page.contains(&"page:doc-header"));
    assert!(!hwp_page.contains(&"page:doc-footer"));
}

#[test]
fn per_kind_export_submenus_keep_hwp_isolated_and_cover_supported_formats() {
    assert!(!lines_of(MenuKind::Hwp, "파일")
        .iter()
        .any(|line| line.starts_with("@export")));
    for (kind, token, required) in [
        (
            MenuKind::Doc,
            layout::EXPORT_DOC,
            &[
                "file:exp-docx",
                "file:exp-odt",
                "file:exp-rtf",
                "file:exp-pages",
                "file:exp-md",
                "file:exp-txt",
                "file:exp-pdf",
                "file:exp-dlg",
            ][..],
        ),
        (
            MenuKind::Sheet,
            layout::EXPORT_SHEET,
            &[
                "file:exp-xlsx",
                "file:exp-ods",
                "file:exp-csv",
                "file:exp-tsv",
                "file:exp-pdf",
                "file:exp-dlg",
            ][..],
        ),
        (
            MenuKind::Slide,
            layout::EXPORT_SLIDE,
            &[
                "file:exp-pptx",
                "file:exp-odp",
                "file:exp-pdf",
                "file:exp-dlg",
            ][..],
        ),
        (
            MenuKind::Base,
            layout::EXPORT_BASE,
            &[
                "file:exp-base",
                "file:exp-xlsx",
                "file:exp-csv",
                "file:exp-tsv",
                "file:exp-dlg",
            ][..],
        ),
        (
            MenuKind::Board,
            layout::EXPORT_BOARD,
            &["file:exp-board", "file:exp-pdf", "file:exp-dlg"][..],
        ),
        (
            MenuKind::Pdf,
            layout::EXPORT_PDF,
            &[
                "file:exp-pdf",
                "file:exp-docx",
                "file:exp-txt",
                "file:exp-dlg",
            ][..],
        ),
    ] {
        let file = lines_of(kind, "파일");
        assert!(file.contains(&token), "{kind:?} missing {token}");
        let ids = layout::table_ids(kind);
        for item in required {
            assert!(ids.contains(item), "{kind:?} missing {item}");
        }
    }
}

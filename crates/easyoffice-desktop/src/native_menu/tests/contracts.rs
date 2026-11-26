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
            s,
            "format:row-height",
            "format:row-autofit",
            "format:col-width",
            "format:col-autofit",
            s,
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
            "가로 가운데 맞춤",
            "오른쪽 맞춤",
            s,
            "위쪽 맞춤",
            "세로 가운데 맞춤",
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
        "format:row-height",
        "format:row-autofit",
        "format:col-width",
        "format:col-autofit",
        "format:cell-format",
        "format:align-left",
        "format:border-line-double",
        "sheet:row-insert-above",
        "sheet:unmerge",
        "sheet:remove-duplicates",
        "sheet:text-to-columns",
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
    assert!(lines_of(MenuKind::Board, "편집").contains(&"edit:find"));
    assert!(!lines_of(MenuKind::Board, "편집").contains(&"edit:replace"));
}

#[test]
fn slice5_native_menu_parity_covers_doc_slide_base_and_cross_module_find() {
    let doc_fmt = lines_of(MenuKind::Doc, "서식");
    for id in ["format:strike", "format:superscript", "format:subscript"] {
        assert!(doc_fmt.contains(&id), "DOC서식 missing {id}");
        assert!(
            lines_of(MenuKind::Hwp, "서식").contains(&id),
            "HWP keeps {id}"
        );
        assert_eq!(spec(id).accelerator, None);
    }
    let slide_ids = layout::table_ids(MenuKind::Slide);
    for id in [
        "slide:bring-forward",
        "slide:send-backward",
        "slide:align-left",
        "slide:align-center",
        "slide:align-right",
        "slide:align-top",
        "slide:align-middle",
        "slide:align-bottom",
        "slide:distribute-h",
        "slide:distribute-v",
    ] {
        assert!(slide_ids.contains(&id), "SLIDE missing {id}");
    }
    assert!(lines_of(MenuKind::Slide, "서식").contains(&layout::SLIDE_ALIGN));
    assert!(lines_of(MenuKind::Base, "파일").contains(&"file:base-import"));
    for kind in [MenuKind::Slide, MenuKind::Pdf, MenuKind::Board] {
        let edit = lines_of(kind, "편집");
        assert!(edit.contains(&"edit:find"), "{kind:?} missing edit:find");
        assert!(
            !edit.contains(&"edit:replace"),
            "{kind:?} must omit edit:replace"
        );
    }
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
    let doc_page = lines_of(MenuKind::Doc, "레이아웃");
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

#[test]
fn label_for_separates_hwp_and_office_terms_and_all_menus_stay_synced() {
    for (id, office_label, hwp_label) in [
        ("edit:undo", "실행 취소", "되돌리기"),
        ("edit:cut", "잘라내기", "오려 두기"),
        ("edit:copy", "복사", "복사하기"),
        ("edit:paste", "붙여넣기", "붙이기"),
        ("edit:replace", "찾기 및 바꾸기", "찾아 바꾸기"),
        ("format:bold", "굵게", "진하게"),
        ("format:italic", "기울임꼴", "기울임"),
        ("view:zoom-100", "100%", "실제 크기"),
    ] {
        let s = spec(id);
        assert_eq!(s.label, office_label, "{id}");
        assert_eq!(label_for(MenuKind::Doc, s), office_label, "{id} doc");
        assert_eq!(label_for(MenuKind::Sheet, s), office_label, "{id} sheet");
        assert_eq!(label_for(MenuKind::Hwp, s), hwp_label, "{id} hwp");
    }
    assert_eq!(spec("view:toolbar-labels").label, "도구 상자 라벨 표시");
    assert_eq!(spec("help:update-check").label, "업데이트 확인 알림");
    assert!(lines_of(MenuKind::Doc, "파일").contains(&"file:doc-paper"));
    assert!(lines_of(MenuKind::Hwp, "파일").contains(&"file:save-as-hwpx"));
    for slide_id in ["slide:insert-textbox", "slide:insert-picture"] {
        assert!(
            lines_of(MenuKind::Slide, "삽입").contains(&slide_id),
            "{slide_id}"
        );
    }
    for slide_id in [
        "slide:bring-front",
        "slide:send-back",
        "slide:delete-element",
    ] {
        assert!(
            lines_of(MenuKind::Slide, "서식").contains(&slide_id),
            "{slide_id}"
        );
    }
    for sheet_id in [
        "sheet:insert-picture",
        "sheet:note-insert",
        "sheet:note-toggle-all",
    ] {
        assert!(
            lines_of(MenuKind::Sheet, "삽입").contains(&sheet_id),
            "{sheet_id}"
        );
    }
    assert!(!lines_of(MenuKind::Sheet, "삽입").contains(&"table:sheet-chart"));
    for kind in MenuKind::ALL {
        assert!(
            lines_of(kind, "보기").contains(&"view:toolbar-labels"),
            "{kind:?}"
        );
        assert!(
            !layout::table_ids(kind).contains(&"tool:security-audit"),
            "{kind:?}"
        );
    }
    assert_eq!(accelerator_for(MenuKind::Hwp, spec("insert:link")), None);
    for kind in [MenuKind::Doc, MenuKind::Sheet, MenuKind::Slide] {
        assert_eq!(
            accelerator_for(kind, spec("insert:link")),
            Some("CmdOrCtrl+K"),
            "{kind:?}"
        );
    }
    for (id, acc) in [
        ("sheet:note-insert", "Shift+F2"),
        ("sheet:name-paste", "F3"),
        ("sheet:calc-now", "F9"),
        ("sheet:calc-sheet", "Shift+F9"),
        ("table:insert", "CmdOrCtrl+T"),
        ("table:autosum", "Alt+="),
    ] {
        assert_eq!(
            accelerator_for(MenuKind::Sheet, spec(id)),
            Some(acc),
            "{id}"
        );
    }
    assert_eq!(
        label_for(MenuKind::Doc, spec("table:border-bg")),
        "테두리 및 음영"
    );
    assert_eq!(
        label_for(MenuKind::Doc, spec("table:cell-props")),
        "표 속성"
    );
    assert_eq!(
        label_for(MenuKind::Doc, spec("table:col-equal")),
        "열 너비를 같게"
    );
    assert_eq!(label_for(MenuKind::Pages, spec("insert:link")), "링크");
    assert_eq!(label_for(MenuKind::Sheet, spec("insert:symbol")), "기호");
    assert_eq!(label_for(MenuKind::Slide, spec("insert:symbol")), "기호");
}

#[test]
fn exactly_the_current_theme_is_checked_and_switches_follow_view_state() {
    let view = |theme, toolbar_labels, update_check| MenuView {
        theme,
        disabled_mask: ItemMask::default(),
        update_check,
        toolbar_labels,
    };
    for mode in ["system", "light", "dark"] {
        let checked: Vec<&str> = CHECK_IDS
            .into_iter()
            .filter(|id| *id != "view:toolbar-labels")
            .filter(|id| check_state(id, &view(mode, true, None)).is_some_and(|(on, _)| on))
            .collect();
        assert_eq!(checked, vec![theme_item_id(mode).unwrap()], "{mode}");
    }
    assert_eq!(
        check_state("view:toolbar-labels", &view("dark", true, None)),
        Some((true, true))
    );
    assert_eq!(
        check_state("view:toolbar-labels", &view("dark", false, None)),
        Some((false, true))
    );
    assert_eq!(
        check_state("help:update-check", &view("dark", true, Some(true))),
        Some((true, true))
    );
    assert_eq!(
        check_state("help:update-check", &view("dark", true, Some(false))),
        Some((false, true))
    );
    assert_eq!(
        check_state("help:update-check", &view("dark", true, None)),
        Some((false, false)),
        "an app that does not check offers no switch"
    );
    assert_eq!(check_state("file:save", &view("dark", true, None)), None);
}

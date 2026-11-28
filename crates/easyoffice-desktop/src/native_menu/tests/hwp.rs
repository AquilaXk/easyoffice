//! The Hangul editor window's bar (PD-190, PD-396).

use super::*;

/// The ids of one menu of the Hangul bar in order, its submenus opened up.
fn hwp_ids(title: &str) -> Vec<&'static str> {
    let mut ids = Vec::new();
    for line in lines_of(MenuKind::Hwp, title) {
        match layout::hwp_submenu(line) {
            Some(sub) => ids.extend(
                sub.lines
                    .iter()
                    .copied()
                    .filter(|id| *id != layout::SEPARATOR),
            ),
            None if *line != layout::SEPARATOR => ids.push(*line),
            None => {}
        }
    }
    ids
}

#[test]
fn the_hangul_window_keeps_the_full_bar_with_the_reference_cell_items_in_order() {
    let table = lines_of(MenuKind::Hwp, "표");
    let at = |id: &str| table.iter().position(|line| *line == id).unwrap();
    assert!(
        at("table:cell-select") < at("table:merge") && at("table:merge") < at("table:cell-split"),
        "select, merge, split"
    );
    let editor_only: Vec<&str> = ITEMS
        .iter()
        .filter(|spec| spec.action == "hwp-editor")
        .map(|spec| spec.id)
        .collect();
    // The shared table's editor items come first, then the ones of `items_hwp.rs`.
    assert_eq!(
        editor_only[..7],
        [
            "view:zoom-dialog",
            "view:form-mode",
            "format:hwp-char-shape",
            "format:hwp-para-shape",
            "table:cell-select",
            "table:cell-split",
            "table:hwp-split",
        ]
    );
    assert_eq!(editor_only.len(), 7 + items_hwp::HWP_ITEMS.len());
    assert_eq!(spec("view:toolbar-labels").action, "view-toolbar-labels");
    let bar_ids = layout::table_ids(MenuKind::Hwp);
    for id in &editor_only {
        assert!(bar_ids.contains(id), "{id}");
    }
    // The official Ctrl+Alt+T is 새 탭, so 셀 선택 has no key of its own.
    assert_eq!(spec("table:cell-select").accelerator, None);
    assert_eq!(spec("view:zoom-100").accelerator, Some("CmdOrCtrl+0"));
}

/// The bar names the official menus in the official order: the table menu is the last one, and 보안 and 검토
/// come before 도구 (the help pages of the menus).
#[test]
fn the_hangul_bar_follows_the_official_menu_order_and_names() {
    let page = hwp_ids("쪽");
    let at = |id: &str| page.iter().position(|line| *line == id).unwrap();
    let in_order = |ids: &[&str]| ids.windows(2).all(|pair| at(pair[0]) < at(pair[1]));
    assert!(in_order(&[
        "page:paper",
        "page:page-border",
        "page:header",
        "page:footer"
    ]));
    assert!(in_order(&[
        "page:new-number",
        "page:break",
        "page:column-break",
        "page:columns",
        "page:section-settings",
        "page:manuscript",
    ]));
    let header_footer =
        layout::hwp_submenu("@hwp-header-footer").expect("the 머리말/꼬리말 submenu");
    assert_eq!(header_footer.title, "머리말/꼬리말");
    assert_eq!(hwp_ids("보안"), ["tool:scrub-pii", "tool:review-verify"]);
    let review = hwp_ids("검토");
    assert!(review.contains(&"tool:review-track") && review.contains(&"tool:review-history"));
    let tool = hwp_ids("도구");
    assert!(
        !tool.contains(&"tool:bench-run"),
        "only in the command search"
    );
    assert!(tool.contains(&"tool:options") && tool.contains(&"view:theme-toggle"));
    assert!(
        !hwp_ids("보기").contains(&"view:theme-toggle"),
        "no theme in 보기"
    );
    for (id, label) in [
        ("view:zoom-dialog", "화면 확대/축소"),
        ("view:doc-grid", "격자 보기"),
        ("table:border-bg", "각 셀마다 적용"),
        ("edit:goto", "찾아가기"),
        ("tool:scrub-pii", "개인 정보 보호"),
    ] {
        assert_eq!(label_for(MenuKind::Hwp, spec(id)), label, "{id}");
    }
    assert_eq!(accelerator_for(MenuKind::Hwp, spec("edit:goto")), None);
}

/// The reference app's file menu ends in a separator, `Export PDF...` (⌘E) and `Print...` (⌘P)
/// (`menu.rs` 27-32, 88-89). Both are shell commands of the Hangul editor window: the page
/// actions are the ones its keyboard shortcuts and its own 파일 menu already run. Only that
/// window's bar has the export; the other windows' bars have no item that is grayed out there.
/// The official 한글 file menu names the export 'PDF로 저장하기' and lists it before 편집 용지, then 인쇄;
/// the reference keys stay. 미리 보기 and 문서 정보 are not in the bar: the editor has no such command and
/// the page's preview pane does not read the editor's pages yet.
#[test]
fn the_hangul_file_menu_has_pdf_export_and_print_in_the_official_order() {
    let export = spec("file:export-pdf");
    assert_eq!(
        (export.label, export.accelerator, export.action),
        ("PDF로 저장하기", Some("CmdOrCtrl+E"), "hwp-export-pdf")
    );
    let print = spec("file:print");
    assert_eq!(
        (print.label, print.accelerator, print.action),
        ("인쇄…", Some("CmdOrCtrl+P"), "print")
    );
    let file = lines_of(MenuKind::Hwp, "파일");
    assert_eq!(
        file[file.len() - 6..],
        [
            "file:save-as",
            "file:save-as-hwpx",
            "file:export-pdf",
            "-",
            "file:page-setup",
            "file:print",
        ],
        "save as, save as hwpx, PDF, a separator, page setup, print"
    );
}

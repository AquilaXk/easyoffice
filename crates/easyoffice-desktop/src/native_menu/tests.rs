use super::*;

const MODIFIERS: &[&str] = &["CmdOrCtrl", "Alt", "Shift"];
const GROUPS: &[&str] = &[
    "app", "file", "edit", "view", "insert", "format", "page", "sheet", "table", "tool", "help",
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
        assert!(
            key.len() == 1 || key == "Enter",
            "{accelerator} ends in one key"
        );
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

fn menus_of(kind: MenuKind) -> &'static [&'static layout::MenuSpec] {
    layout::layout(kind)
}

fn lines_of(kind: MenuKind, title: &str) -> &'static [&'static str] {
    menus_of(kind)
        .iter()
        .find(|menu| menu.title == title)
        .unwrap_or_else(|| panic!("{kind:?} has no {title} menu"))
        .lines
}

fn titles_of(kind: MenuKind) -> Vec<&'static str> {
    menus_of(kind).iter().map(|menu| menu.title).collect()
}

#[test]
fn the_bars_of_all_kinds_use_exactly_the_ids_of_the_table() {
    let used: std::collections::BTreeSet<&str> = MenuKind::ALL
        .into_iter()
        .flat_map(layout::table_ids)
        .collect();
    // 닫기 is the 윈도우 menu's system item and paste is the system item: both stay in the table
    // because the page's menu row has them; every other table item is on some kind's bar.
    let table: std::collections::BTreeSet<&str> = ITEMS
        .iter()
        .map(|spec| spec.id)
        .filter(|id| !MACOS_SYSTEM_IDS.contains(id))
        .collect();
    assert_eq!(used, table);
}

#[test]
fn a_bar_holds_each_id_once_and_every_line_is_known() {
    for kind in MenuKind::ALL {
        let ids = layout::table_ids(kind);
        let unique: std::collections::HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(unique.len(), ids.len(), "{kind:?} repeats an id");
        for menu in menus_of(kind) {
            for line in menu.lines {
                let token = *line == layout::SEPARATOR || line.starts_with('@') || *line == QUIT_ID;
                assert!(token || item_index(line).is_some(), "{kind:?}: {line}");
            }
        }
    }
}

#[test]
fn no_menu_starts_or_ends_with_a_separator_or_repeats_one() {
    for kind in MenuKind::ALL {
        for menu in menus_of(kind) {
            let lines = menu.lines;
            assert_ne!(
                lines.first(),
                Some(&layout::SEPARATOR),
                "{kind:?} {}",
                menu.title
            );
            assert_ne!(
                lines.last(),
                Some(&layout::SEPARATOR),
                "{kind:?} {}",
                menu.title
            );
            assert!(
                lines.windows(2).all(|pair| pair != [layout::SEPARATOR; 2]),
                "{kind:?} {}",
                menu.title
            );
        }
    }
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
fn each_kind_has_the_menus_of_its_window_in_order() {
    let bar = |first: &[&'static str], last: &[&'static str]| {
        let mut titles = vec!["EasyOffice"];
        titles.extend_from_slice(first);
        titles.extend_from_slice(last);
        titles
    };
    let tail = ["윈도우", "도움말"];
    assert_eq!(
        titles_of(MenuKind::Doc),
        bar(
            &["파일", "편집", "보기", "입력", "서식", "쪽", "표", "도구"],
            &tail
        )
    );
    assert_eq!(
        titles_of(MenuKind::Sheet),
        bar(
            &[
                "파일",
                "편집",
                "보기",
                "삽입",
                "서식",
                "시트",
                "수식",
                "데이터",
                "도구"
            ],
            &tail
        )
    );
    assert_eq!(
        titles_of(MenuKind::Slide),
        bar(
            &[
                "파일",
                "편집",
                "보기",
                "삽입",
                "서식",
                "슬라이드",
                "슬라이드 쇼",
                "도구"
            ],
            &tail
        )
    );
    assert_eq!(
        titles_of(MenuKind::Pdf),
        bar(&["파일", "편집", "보기", "주석", "쪽", "도구"], &tail)
    );
    assert_eq!(
        titles_of(MenuKind::Hwp),
        bar(
            &["파일", "편집", "보기", "입력", "서식", "쪽", "표", "도구"],
            &tail
        )
    );
}

#[test]
fn a_kind_does_not_hold_the_items_of_other_kinds() {
    let not_in = |kind: MenuKind, forbidden: &[&str]| {
        let ids = layout::table_ids(kind);
        for id in forbidden {
            assert!(!ids.contains(id), "{kind:?} holds {id}");
        }
    };
    let editor_only: Vec<&str> = ITEMS
        .iter()
        .filter(|spec| spec.action == "hwp-editor")
        .map(|spec| spec.id)
        .collect();
    let paragraph = [
        "format:justify",
        "format:para-numbering",
        "format:para-bullet",
        "format:indent-in",
        "format:indent-out",
        "page:columns",
        "page:header",
        "page:footer",
        "view:ctrl-marks",
        "view:para-marks",
        "view:doc-grid",
    ];
    let sheet_only = [
        "table:sheet-pivot",
        "table:sheet-chart",
        "table:sheet-condfmt",
        "table:sheet-freeze",
    ];
    let slide_only = [
        "page:slide-master",
        "page:slide-snap",
        "page:slide-transition",
        "page:slide-presenter",
    ];
    let pdf_only = [
        "tool:pdf-highlight",
        "tool:pdf-rotate",
        "tool:pdf-splitmerge",
        "tool:pdf-tooffice",
    ];
    let hwp_only = [
        "insert:field",
        "page:hwp-header",
        "page:manuscript",
        "table:hwp-border",
        "file:export-pdf",
    ];
    for kind in [
        MenuKind::Doc,
        MenuKind::Sheet,
        MenuKind::Slide,
        MenuKind::Pdf,
    ] {
        not_in(kind, &editor_only);
        not_in(kind, &hwp_only);
    }
    not_in(MenuKind::Doc, &sheet_only);
    not_in(MenuKind::Doc, &slide_only);
    not_in(MenuKind::Doc, &pdf_only);
    not_in(MenuKind::Doc, &["table:cell-select", "table:cell-split"]);
    not_in(MenuKind::Hwp, &["table:split"]);
    for kind in [MenuKind::Sheet, MenuKind::Slide, MenuKind::Pdf] {
        not_in(kind, &paragraph);
        not_in(
            kind,
            &[
                "insert:toc",
                "insert:toc-update",
                "page:break",
                "insert:mailmerge",
                "insert:footnote",
                "table:doc-formula",
                "table:row-above",
                "table:add-row",
                "table:col-left",
                "table:col-right",
                "table:row-delete",
                "table:col-delete",
                "table:split",
                "table:merge",
                "table:sum",
            ],
        );
    }
    not_in(MenuKind::Sheet, &slide_only);
    not_in(MenuKind::Sheet, &pdf_only);
    not_in(MenuKind::Slide, &sheet_only);
    not_in(MenuKind::Slide, &pdf_only);
    not_in(MenuKind::Pdf, &sheet_only);
    not_in(MenuKind::Pdf, &slide_only);
    // Nothing prints or changes the paper in a PDF window, and a PDF has no formatting menu.
    not_in(
        MenuKind::Pdf,
        &[
            "file:print",
            "page:paper",
            "page:orientation",
            "page:margins",
        ],
    );
    for kind in [MenuKind::Sheet, MenuKind::Slide] {
        assert!(!titles_of(kind).contains(&"쪽"), "{kind:?}");
    }
    for kind in [MenuKind::Sheet, MenuKind::Slide, MenuKind::Pdf] {
        assert!(
            !titles_of(kind).contains(&"표") && !titles_of(kind).contains(&"입력"),
            "{kind:?}"
        );
    }
    assert!(!titles_of(MenuKind::Pdf).contains(&"서식"));
}

#[test]
fn only_the_spreadsheet_tool_menu_has_the_protection_items_and_they_run_the_page_commands() {
    let tools = lines_of(MenuKind::Sheet, "도구");
    for (id, act) in [
        ("sheet:protect", "sheet-act:sheet-protect"),
        ("sheet:protect-workbook", "sheet-act:workbook-protect"),
        ("sheet:protect-ranges", "sheet-act:range-allow"),
    ] {
        assert!(tools.contains(&id), "{id}");
        assert_eq!(action_for(id), Some(act), "{id}");
        for kind in [MenuKind::Doc, MenuKind::Hwp, MenuKind::Slide, MenuKind::Pdf] {
            assert!(!layout::table_ids(kind).contains(&id), "{kind:?} {id}");
        }
    }
    // The three sit together between the review items and the command search, as on the page.
    let at = |id: &str| tools.iter().position(|line| *line == id).unwrap();
    assert!(at("tool:review-stats") < at("sheet:protect"));
    assert!(at("sheet:protect") + 1 == at("sheet:protect-workbook"));
    assert!(at("sheet:protect-workbook") + 1 == at("sheet:protect-ranges"));
    assert!(at("sheet:protect-ranges") < at("tool:palette"));
}

#[test]
fn the_spreadsheet_keeps_sort_filter_style_data_tools_in_data_and_the_frame_in_sheet() {
    let data = lines_of(MenuKind::Sheet, "데이터");
    for id in [
        "table:sort",
        "table:validation",
        "table:sheet-pivot",
        "table:sheet-condfmt",
    ] {
        assert!(data.contains(&id), "{id}");
    }
    // The auto filter items sit between the sort and validation group and the pivot group, as
    // on the page.
    let at = |id: &str| data.iter().position(|line| *line == id).unwrap();
    // The sort dialog follows the quick sort, as on the page.
    assert!(at("table:sort") + 1 == at("sheet:sort"));
    assert_eq!(action_for("sheet:sort"), Some("sheet-act:sort-dialog"));
    assert!(at("table:validation") < at("sheet:filter"));
    assert!(at("sheet:filter") + 1 == at("sheet:filter-reapply"));
    assert!(at("sheet:filter-reapply") + 1 == at("sheet:filter-clear"));
    assert!(at("sheet:filter-clear") < at("table:sheet-pivot"));
    for id in ["sheet:filter", "sheet:filter-reapply", "sheet:filter-clear"] {
        assert!(
            action_for(id).unwrap().starts_with("sheet-act:filter-"),
            "{id}"
        );
        for kind in [MenuKind::Doc, MenuKind::Hwp, MenuKind::Slide, MenuKind::Pdf] {
            assert!(!layout::table_ids(kind).contains(&id), "{kind:?} {id}");
        }
    }
    assert!(lines_of(MenuKind::Sheet, "시트").contains(&"table:sheet-freeze"));
    let insert = lines_of(MenuKind::Sheet, "삽입");
    assert!(insert.contains(&"table:fn-browser") && insert.contains(&"table:autosum"));
    let pdf_annotations = lines_of(MenuKind::Pdf, "주석");
    assert!(pdf_annotations.contains(&"tool:pdf-highlight"));
    let pdf_pages = lines_of(MenuKind::Pdf, "쪽");
    assert_eq!(pdf_pages, ["tool:pdf-rotate", "tool:pdf-splitmerge"]);
    let show = lines_of(MenuKind::Slide, "슬라이드 쇼");
    assert_eq!(show, ["page:slide-transition", "page:slide-presenter"]);
}

#[test]
fn every_kind_keeps_the_save_items_the_standard_edit_group_and_the_command_search() {
    for kind in MenuKind::ALL {
        let ids = layout::table_ids(kind);
        for id in [
            "file:new",
            "file:open",
            "file:open-recent",
            "file:save",
            "file:save-as",
            "edit:undo",
            "edit:redo",
            "edit:cut",
            "edit:copy",
            "tool:palette",
            "help:update-check",
            "view:zoom-in",
            "view:theme-toggle",
        ] {
            assert!(ids.contains(&id), "{kind:?} lacks {id}");
        }
        assert!(lines_of(kind, "편집").contains(&layout::PASTE), "{kind:?}");
        assert!(
            lines_of(kind, "편집").contains(&layout::SELECT_ALL),
            "{kind:?}"
        );
        assert_eq!(menus_of(kind)[0].lines.last(), Some(&QUIT_ID), "{kind:?}");
    }
}

#[test]
fn the_page_kind_names_are_the_rust_kinds() {
    for (name, kind) in [
        ("DOC", MenuKind::Doc),
        ("HWP", MenuKind::Hwp),
        ("SHEET", MenuKind::Sheet),
        ("SLIDE", MenuKind::Slide),
        ("PDF", MenuKind::Pdf),
    ] {
        assert_eq!(MenuKind::parse(name), Some(kind));
        assert!(MenuKind::ALL.contains(&kind));
    }
    assert_eq!(MenuKind::ALL.len(), 5);
    for bad in ["", "doc", "START", "CONV", "HWP_EMBED"] {
        assert_eq!(MenuKind::parse(bad), None, "{bad}");
    }
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
    assert_eq!(
        editor_only,
        [
            "view:zoom-dialog",
            "view:toolbar-labels",
            "view:form-mode",
            "table:cell-select",
            "table:cell-split",
        ]
    );
    let hwp_ids = layout::table_ids(MenuKind::Hwp);
    for id in &editor_only {
        assert!(hwp_ids.contains(id), "{id}");
    }
    assert_eq!(
        spec("table:cell-select").accelerator,
        Some("CmdOrCtrl+Alt+T")
    );
    assert_eq!(spec("view:zoom-100").accelerator, Some("CmdOrCtrl+0"));
}

#[test]
fn paste_and_close_are_system_items_that_stay_in_the_table_for_the_page_menu() {
    let ids: Vec<&str> = installed_ids().collect();
    assert_eq!(MACOS_SYSTEM_IDS, ["edit:paste", "file:close"]);
    for id in MACOS_SYSTEM_IDS {
        assert!(ITEMS.iter().any(|spec| spec.id == *id), "{id}");
        assert!(!ids.contains(id), "{id} is a system item");
    }
    for id in [
        "edit:undo",
        "edit:redo",
        "edit:cut",
        "edit:copy",
        "edit:find",
        "edit:replace",
    ] {
        assert!(ids.contains(&id), "{id} is a custom item");
    }
    assert!(ids.contains(&"app:about"));
}

#[test]
fn editing_keys_stay_with_the_focused_editor_except_the_edit_menu_keys() {
    for spec in ITEMS
        .iter()
        .filter(|spec| spec.id.starts_with("format:") || spec.id.starts_with("edit:"))
        .filter(|spec| spec.id != "edit:find")
    {
        assert_eq!(spec.accelerator, None, "{}", spec.id);
        let bound = accelerator_of(spec);
        let macos_key = MACOS_EDIT_KEYS.iter().find(|(id, _)| *id == spec.id);
        assert_eq!(bound, macos_key.map(|(_, keys)| *keys));
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
    assert_eq!(QUIT_LABEL, "EasyOffice 끝내기");
}

/// The reference app's file menu ends in a separator, `Export PDF...` (⌘E) and `Print...` (⌘P)
/// (`menu.rs` 27-32, 88-89). Both are shell commands of the Hangul editor window: the page
/// actions are the ones its keyboard shortcuts and its own 파일 menu already run. Only that
/// window's bar has the export; the other windows' bars have no item that is grayed out there.
#[test]
fn the_hangul_file_menu_ends_with_pdf_export_and_print_with_the_reference_keys() {
    let export = spec("file:export-pdf");
    assert_eq!(
        (export.label, export.accelerator, export.action),
        ("PDF로 내보내기…", Some("CmdOrCtrl+E"), "hwp-export-pdf")
    );
    let print = spec("file:print");
    assert_eq!(
        (print.label, print.accelerator, print.action),
        ("인쇄…", Some("CmdOrCtrl+P"), "print")
    );
    let file = lines_of(MenuKind::Hwp, "파일");
    assert_eq!(
        file[file.len() - 4..],
        ["file:save-as", "-", "file:export-pdf", "file:print"],
        "save as, a separator, export, print"
    );
}

/// The reference app's file menu is new documents, a separator, open, recent, save, save as, a
/// separator, export, print; its view menu is zoom in, zoom out, actual size, a separator, fit
/// page, fit width (`menu.rs` 79-114). The lines before the first item and after the last one are
/// ours. Every kind's bar keeps that head.
#[test]
fn the_file_and_view_menus_keep_the_reference_order_and_separators() {
    for kind in MenuKind::ALL {
        let file = lines_of(kind, "파일");
        assert_eq!(
            file[..7],
            [
                "file:new",
                layout::NEW_DOCS,
                "-",
                "file:open",
                "file:open-recent",
                "file:save",
                "file:save-as"
            ],
            "{kind:?}"
        );
        assert_eq!(
            lines_of(kind, "보기")[..6],
            [
                "view:zoom-in",
                "view:zoom-out",
                "view:zoom-100",
                "-",
                "view:zoom-fit-page",
                "view:zoom-fit-width"
            ],
            "{kind:?}"
        );
    }
}

/// `run_item` hands the quit item to the queue (it once skipped macOS, where the id then fell
/// through to `dispatch_script`, which has none, and ⌘Q did nothing).
#[test]
fn run_item_sends_the_quit_item_to_the_queue() {
    let source = include_str!("../native_menu.rs");
    let production = source.split("#[cfg(test)]").next().unwrap();
    let run_item = &production[production.find("fn run_item").unwrap()..];
    let quit = &run_item[..run_item.find("dispatch_script(id)").unwrap()];
    assert!(quit.contains("if id == QUIT_ID {"));
    assert!(quit.contains("crate::app_quit::request_app_quit(app);"));
    assert!(!quit.contains("cfg"), "no condition is skipped");
}

/// The macOS layout: the application menu ends in the custom quit item, not the system one that
/// would end the process at once; the system items carry Korean labels; ⌘W belongs to the 윈도우
/// menu's 닫기 alone (the builder is checked as source, the menu itself needs a running app).
#[test]
fn the_macos_menus_have_the_custom_quit_korean_system_labels_and_one_cmd_w() {
    let build = include_str!("build.rs");
    assert!(build.contains("QUIT_ID => builder.item(&quit_item(app)?)"));
    assert!(!build.contains(".quit()") && !build.contains("PredefinedMenuItem::quit"));
    assert!(build.contains("PredefinedMenuItem::close_window(app, Some(\"닫기\"))"));
    // No system item is built without its Korean text (the system language would show).
    for call in build
        .split("PredefinedMenuItem::")
        .skip(1)
        .filter(|c| !c.starts_with("{"))
    {
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
        assert!(!build.contains(bare), "{bare}");
    }
    for kind in MenuKind::ALL {
        let bar = menus_of(kind);
        assert_eq!(bar[0].title, "EasyOffice");
        assert_eq!(bar[0].lines.last(), Some(&QUIT_ID), "{kind:?}");
        assert_eq!(
            lines_of(kind, "윈도우"),
            [layout::MINIMIZE, layout::CLOSE_WINDOW]
        );
        let ids = layout::table_ids(kind);
        assert!(
            !ids.contains(&"file:close"),
            "{kind:?}: 닫기 is the 윈도우 menu's"
        );
        for undo_redo_cut_copy in ["edit:undo", "edit:redo", "edit:cut", "edit:copy"] {
            assert!(
                ids.contains(&undo_redo_cut_copy),
                "{kind:?}: the table's item, not a system one"
            );
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
    let view = |theme, update_check| MenuView {
        theme,
        disabled_mask: ItemMask::default(),
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
            "왼쪽 정렬",
            "가운데 정렬",
            "오른쪽 정렬",
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
            "선택 행/열 고정",
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
            "시트 왼쪽으로 이동",
            "시트 오른쪽으로 이동",
            "시트 숨기기",
            "시트 숨기기 취소…",
        ]
    );
    for kind in [MenuKind::Doc, MenuKind::Hwp, MenuKind::Slide, MenuKind::Pdf] {
        let ids = layout::table_ids(kind);
        assert!(
            ids.iter().all(|id| !id.starts_with("sheet:")
                && !id.starts_with("format:numfmt-")
                && !id.starts_with("format:align-")
                && !id.starts_with("format:valign-")
                && !id.starts_with("format:border-")
                && *id != "format:wrap"),
            "{kind:?}"
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
                || action.starts_with("sheet-look:"),
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
    assert_eq!(item.label, "골라 붙이기…");
    assert_eq!(
        action_for("edit:paste-special"),
        Some("sheet-paste:special")
    );
    assert_eq!(
        dispatch_script("edit:paste-special").unwrap(),
        format!("{SCRIPT_PREFIX}\"edit:paste-special\");")
    );
    for kind in [MenuKind::Doc, MenuKind::Hwp, MenuKind::Slide, MenuKind::Pdf] {
        assert!(
            !lines_of(kind, "편집").contains(&"edit:paste-special"),
            "{kind:?}"
        );
    }
}

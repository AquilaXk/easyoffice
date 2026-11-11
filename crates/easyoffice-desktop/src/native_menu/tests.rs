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
    let source = include_str!("../native_menu.rs");
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
    // 닫기 is the 윈도우 menu's system item; every other table item is built by a menu function.
    let table: std::collections::BTreeSet<&str> = ITEMS
        .iter()
        .map(|spec| spec.id)
        .filter(|id| *id != "file:close")
        .collect();
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
    let source = include_str!("../native_menu.rs");
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
/// actions are the ones its keyboard shortcuts and its own 파일 menu already run, and the page
/// grays them out in every other window (`shell-chrome.js`, `EMBED_ONLY_ACTIONS`).
#[test]
fn the_file_menu_ends_with_pdf_export_and_print_with_the_reference_keys() {
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
    let source = include_str!("../native_menu.rs");
    let production = source.split("#[cfg(test)]").next().unwrap();
    let start = production.find("fn file_menu").unwrap();
    let file = &production[start..start + production[start..].find("fn edit_menu").unwrap()];
    let at = |needle: &str| file.find(needle).unwrap();
    assert!(
        at("\"file:save-as\"") < at("SEPARATOR,\n        \"file:export-pdf\"")
            && at("\"file:export-pdf\"") < at("\"file:print\""),
        "save as, a separator, export, print"
    );
    assert!(file
        .trim_end()
        .ends_with("submenu(app, \"파일\", lines)\n}"));
}

/// The reference app's file menu is new documents, a separator, open, recent, save, save as, a
/// separator, export, print; its view menu is zoom in, zoom out, actual size, a separator, fit
/// page, fit width (`menu.rs` 79-114). The lines before the first item and after the last one are ours.
#[test]
fn the_file_and_view_menus_keep_the_reference_order_and_separators() {
    let source = include_str!("../native_menu.rs");
    let production = source.split("#[cfg(test)]").next().unwrap();
    let section = |from: &str, to: &str| {
        let start = production.find(from).unwrap();
        &production[start..start + production[start..].find(to).unwrap()]
    };
    let ids_in = |text: &str| -> Vec<String> {
        text.split('"')
            .skip(1)
            .step_by(2)
            .filter(|piece| piece.contains(':') || *piece == "-")
            .map(str::to_string)
            .collect()
    };
    let file = section("fn file_menu", "fn edit_menu");
    let lines = file.split("lines.push(Entry::Sub(kinds));").nth(1).unwrap();
    let lines = lines.replace("SEPARATOR", "\"-\"");
    assert_eq!(
        ids_in(&lines),
        [
            "-",
            "file:open",
            "file:open-recent",
            "file:save",
            "file:save-as",
            "-",
            "file:export-pdf",
            "file:print"
        ]
    );
    let view =
        section("fn view_menu", "lines.push(Entry::Sub(theme))").replace("SEPARATOR", "\"-\"");
    let view_ids = ids_in(view.split("let mut lines").nth(1).unwrap());
    assert_eq!(
        view_ids[..6],
        [
            "view:zoom-in",
            "view:zoom-out",
            "view:zoom-100",
            "-",
            "view:zoom-fit-page",
            "view:zoom-fit-width"
        ]
    );
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

/// The macOS layout (source check, the menu itself needs a running app): the application menu
/// ends in the custom quit item, not the system one that would end the process at once; the
/// system items carry Korean labels; ⌘W belongs to the 윈도우 menu's 닫기 alone.
#[test]
fn the_macos_menus_have_the_custom_quit_korean_system_labels_and_one_cmd_w() {
    let source = include_str!("../native_menu.rs");
    let production = source.split("#[cfg(test)]").next().unwrap();
    let section = |from: &str, to: &str| {
        let start = production.find(from).unwrap();
        &production[start..start + production[start..].find(to).unwrap()]
    };
    let app = section("fn app_menu", "fn file_menu");
    assert!(app.contains(".item(&quit_item(app)?)"));
    assert!(!app.contains(".quit()") && !app.contains("PredefinedMenuItem::quit"));
    let file = section("fn file_menu", "fn edit_menu");
    assert!(!file.contains("file:close") && !file.contains("QUIT_ID"));
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
    let view = |theme, update_check| MenuView {
        theme,
        disabled_mask: 0,
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

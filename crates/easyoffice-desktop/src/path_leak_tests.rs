//! A structural guard for PD-088: no text that goes back to the page may carry a path. Paths and the
//! system's own error text belong in `eprintln!` only. The sources are read as text, comments and
//! `eprintln!` calls removed, and the test module cut off.

/// The shell sources whose errors reach the page (commands and the code under them).
const SOURCES: [(&str, &str); 9] = [
    ("main.rs", include_str!("main.rs")),
    ("pdf_job.rs", include_str!("pdf_job.rs")),
    ("window_commands.rs", include_str!("window_commands.rs")),
    ("save_file.rs", include_str!("save_file.rs")),
    ("save_error.rs", include_str!("save_error.rs")),
    ("font_catalog.rs", include_str!("font_catalog.rs")),
    ("confirm_dialog.rs", include_str!("confirm_dialog.rs")),
    ("open_feedback.rs", include_str!("open_feedback.rs")),
    ("menu_state.rs", include_str!("menu_state.rs")),
];

/// The code before the first test module, without line comments.
fn production_code(source: &str) -> String {
    let end = source.find("#[cfg(test)]").unwrap_or(source.len());
    source[..end]
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The text of each call `marker(...)`, up to the matching parenthesis.
fn call_arguments<'a>(code: &'a str, marker: &str) -> Vec<&'a str> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = code[from..].find(marker) {
        let start = from + at + marker.len();
        let mut depth = 1;
        let mut end = start;
        for (offset, ch) in code[start..].char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
            if depth == 0 {
                end = start + offset;
                break;
            }
        }
        found.push(&code[start..end]);
        from = end.max(start);
    }
    found
}

fn without_logging(code: &str) -> String {
    let mut text = code.to_string();
    for call in call_arguments(code, "eprintln!(") {
        text = text.replace(call, "");
    }
    text
}

#[test]
fn no_path_is_formatted_outside_the_log() {
    for (name, source) in SOURCES {
        let code = without_logging(&production_code(source));
        assert!(
            !code.contains(".display()"),
            "{name} formats a path outside eprintln!"
        );
    }
}

#[test]
fn error_constructors_never_take_a_path() {
    for (name, source) in SOURCES {
        let code = production_code(source);
        for marker in ["IpcError::new(", "WindowError::new("] {
            for args in call_arguments(&code, marker) {
                for banned in ["display()", "to_string_lossy", "Path", "path"] {
                    assert!(
                        !args.contains(banned),
                        "{name}: {marker}{args}) has {banned}"
                    );
                }
            }
        }
    }
}

#[test]
fn the_guard_sees_a_leak() {
    let leaky = "Err(IpcError::new(format!(\"x {}\", path.display())))";
    assert_eq!(call_arguments(leaky, "IpcError::new(").len(), 1);
    assert!(without_logging(leaky).contains(".display()"));
    let logged = "eprintln!(\"save: {}\", path.display());";
    assert!(!without_logging(logged).contains(".display()"));
}

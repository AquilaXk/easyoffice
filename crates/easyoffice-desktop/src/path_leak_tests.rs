//! A structural guard for PD-088: no text that goes back to the page may carry a path. Paths and the
//! system's own error text belong in `eprintln!` only. The sources are read as text, comments and
//! `eprintln!` calls removed, and the test module cut off.

/// Files that never answer the page: test code, the offline smoke run and the benchmark harness.
fn is_page_facing(name: &str) -> bool {
    let tests = name.ends_with("_tests.rs") || name.contains("/tests/") || name.contains("_tests/");
    let harness =
        name.starts_with("smoke") || name == "perf_bench.rs" || name.ends_with("_fixtures.rs");
    !(tests || harness)
}

fn collect_sources(
    dir: &std::path::Path,
    base: &std::path::Path,
    found: &mut Vec<(String, String)>,
) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .expect("read the shell source directory")
        .map(|entry| entry.expect("read a directory entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_sources(&path, base, found);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let name = path
                .strip_prefix(base)
                .expect("under src")
                .to_string_lossy()
                .replace('\\', "/");
            if is_page_facing(&name) {
                let text = std::fs::read_to_string(&path).expect("read a shell source");
                found.push((name, text));
            }
        }
    }
}

/// Every shell source that can answer the page, so a new file is covered without a list to update.
fn sources() -> Vec<(String, String)> {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    collect_sources(&src, &src, &mut found);
    assert!(
        found.len() > 20,
        "the source scan found only {} files",
        found.len()
    );
    found
}

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
    for (name, source) in sources() {
        let code = without_logging(&production_code(&source));
        assert!(
            !code.contains(".display()"),
            "{name} formats a path outside eprintln!"
        );
    }
}

#[test]
fn error_constructors_never_take_a_path() {
    for (name, source) in sources() {
        let code = production_code(&source);
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

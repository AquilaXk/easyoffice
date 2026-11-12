use super::*;

#[test]
fn document_kinds_exclude_the_start_screen_and_unknown_names() {
    assert_eq!(parse_document_kind("sheet"), Ok(WindowKind::Sheet));
    assert!(parse_document_kind("START").is_err());
    assert!(parse_document_kind("../../x").is_err());
}

#[test]
fn windows_cascade_from_the_work_area_centre_and_wrap() {
    let work = (1920.0, 1000.0);
    let size = (1280.0, 760.0);
    assert_eq!(cascade_origin(work, size, 0), (320.0, 84.0));
    assert_eq!(cascade_origin(work, size, 1), (348.0, 112.0));
    assert_eq!(cascade_origin(work, size, 8), (320.0, 84.0));
    assert_eq!(cascade_origin((1000.0, 700.0), size, 0), (24.0, 24.0));
}

#[test]
fn size_is_limited_to_85_percent_of_the_work_area_but_keeps_the_minimum() {
    let min = (960.0, 720.0);
    assert_eq!(
        clamp_size((2560.0, 1400.0), (1280.0, 860.0), min),
        (1280.0, 860.0)
    );
    // (1366 - 48) * 0.85 = 1120.3 and (768 - 48 - 72) * 0.85 = 550.8, floored; the height is then raised to the minimum.
    assert_eq!(
        clamp_size((1366.0, 768.0), (1280.0, 860.0), min),
        (1120.0, 720.0)
    );
    assert_eq!(
        clamp_size((900.0, 600.0), (1280.0, 860.0), min),
        (960.0, 720.0)
    );
}

#[test]
fn open_outcomes_and_errors_serialize_without_paths() {
    assert_eq!(
        serde_json::to_string(&OpenOutcome::OPENED).unwrap(),
        r#"{"status":"opened"}"#
    );
    assert_eq!(
        serde_json::to_string(&OpenOutcome::FOCUSED).unwrap(),
        r#"{"status":"focused"}"#
    );
    assert_eq!(
        serde_json::to_string(&OpenOutcome::CANCELLED).unwrap(),
        r#"{"status":"cancelled"}"#
    );
    let err = serde_json::to_string(&WindowError::new("bad")).unwrap();
    assert_eq!(err, r#"{"status":"error","message":"bad"}"#);
}

/// Structural guard: no window command takes or returns a file system path.
#[test]
fn window_command_signatures_never_carry_a_path() {
    let source = include_str!("../window_commands.rs");
    let production = source.split("#[cfg(test)]").next().unwrap();
    let mut checked = 0;
    for block in production.split("#[tauri::command]").skip(1) {
        let signature = block.split('{').next().unwrap();
        let lowered = signature.to_lowercase();
        assert!(
            !lowered.contains("path"),
            "command signature mentions a path: {signature}"
        );
        assert!(
            !lowered.contains("pathbuf"),
            "command signature uses PathBuf: {signature}"
        );
        checked += 1;
    }
    assert_eq!(checked, 11, "every window command is covered");
}

fn scratch_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "easyoffice-desktop-launch-{tag}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn second_instance_args_are_routed_to_the_open_queue() {
    let dir = scratch_dir("second");
    let a = dir.join("a.docx");
    let b = dir.join("b.hwpx");
    let exe = dir.join("notes.exe");
    fs::write(&a, b"x").unwrap();
    fs::write(&b, b"x").unwrap();
    fs::write(&exe, b"x").unwrap();
    let argv = vec![
        "C:\\Program Files\\EasyOffice\\EasyOffice.exe".to_string(),
        "--flag".to_string(),
        a.display().to_string(),
        dir.join("missing.docx").display().to_string(),
        exe.display().to_string(),
        b.display().to_string(),
    ];
    let files = second_instance_files(argv);
    assert_eq!(
        files,
        vec![a.clone(), exe.clone(), b.clone()],
        "only existing files, in order; the unsupported one is reported when opened"
    );
    let mut queued = Vec::new();
    let handled = route_launch_files(
        &files,
        |path| {
            queued.push(path.to_path_buf());
            Ok(OpenOutcome::OPENED)
        },
        |_| {},
    );
    assert!(handled);
    assert_eq!(queued, vec![a, exe, b]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn finder_open_urls_keep_only_existing_local_files() {
    let dir = std::env::temp_dir().join(format!("eo-urls-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let doc = dir.join("보고서 1.docx");
    fs::write(&doc, b"x").unwrap();
    let exe = dir.join("tool.exe");
    fs::write(&exe, b"x").unwrap();
    let urls = vec![
        tauri::Url::from_file_path(&doc).unwrap(),
        tauri::Url::from_file_path(&exe).unwrap(),
        tauri::Url::from_file_path(dir.join("missing.xlsx")).unwrap(),
        tauri::Url::parse("https://example.com/a.docx").unwrap(),
    ];
    assert_eq!(file_url_paths(&urls), vec![doc, exe]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn launch_routing_reports_whether_anything_was_handled() {
    let none: Vec<PathBuf> = Vec::new();
    assert!(!route_launch_files(
        &none,
        |_| Ok(OpenOutcome::OPENED),
        |_| {}
    ));
    let files = vec![PathBuf::from("a.docx")];
    let mut failures = Vec::new();
    assert!(!route_launch_files(
        &files,
        |_| Err("nope".into()),
        |message| failures.push(message.to_string())
    ));
    assert_eq!(failures, vec!["nope".to_string()], "a failure is reported");
    assert!(route_launch_files(
        &files,
        |_| Ok(OpenOutcome::FOCUSED),
        |_| {}
    ));
}

#[test]
fn labels_are_unique_and_pass_the_capability_pattern() {
    let state = AppWindows::new(None);
    let a = state.next_label();
    let b = state.next_label();
    assert_ne!(a, b);
    assert!(crate::windows::is_window_label(&a));
    assert!(
        a.starts_with("doc-"),
        "matches the doc-* capability window pattern"
    );
}

/// A window opened for a file that its page could not show: nothing of the file is left to
/// save to, and the file can be opened again in a window of its own.
#[test]
fn an_abandoned_open_lets_go_of_the_file_and_its_bytes() {
    let dir = scratch_dir("abandon");
    let file = dir.join("a.docx");
    fs::write(&file, b"x").unwrap();
    let canonical = fs::canonicalize(&file).unwrap();
    let state = AppWindows::new(None);
    locked(&state.registry).register("doc-1-a", WindowKind::Doc);
    locked(&state.registry)
        .claim_path("doc-1-a", path_key(&canonical))
        .unwrap();
    let fingerprint = Fingerprint::of(&canonical).unwrap();
    locked(&state.files).track_opened("doc-1-a", canonical.clone(), fingerprint);
    locked(&state.pending).push(
        "doc-1-a",
        PendingOpen {
            name: "a.docx".into(),
            bytes: vec![1],
        },
    );
    state.abandon_open("doc-1-a");
    assert!(locked(&state.files).get("doc-1-a").is_none());
    assert!(locked(&state.pending).take("doc-1-a").is_none());
    assert_eq!(
        locked(&state.registry).find_by_path(&path_key(&canonical)),
        None
    );
    assert!(
        locked(&state.registry).remove("doc-1-a"),
        "the window stays"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn app_state_shares_the_registry_and_queue_by_label() {
    let state = AppWindows::new(None);
    locked(&state.registry).register("doc-1-a", WindowKind::Doc);
    locked(&state.pending).push(
        "doc-1-a",
        PendingOpen {
            name: "a.docx".into(),
            bytes: vec![1],
        },
    );
    assert_eq!(
        locked(&state.pending).take("doc-1-a").unwrap().name,
        "a.docx"
    );
    assert!(locked(&state.pending).take("doc-1-a").is_none());
    assert!(locked(&state.registry).remove("doc-1-a"));
}

/// What the shell reads and hands over is bounded by what the page opens of that kind (10 MiB
/// through the engine, 64 MiB for the Hangul editor), so no larger file is read, encoded and
/// copied only to be refused there.
#[test]
fn the_open_size_limit_is_the_pages_limit_of_the_kind() {
    let mib = 1_048_576;
    assert_eq!(WindowKind::Doc.max_open_bytes(), 10 * mib);
    assert_eq!(WindowKind::Hwp.max_open_bytes(), 64 * mib);
    for kind in [WindowKind::Sheet, WindowKind::Slide, WindowKind::Pdf] {
        assert_eq!(kind.max_open_bytes(), 10 * mib, "{kind:?}");
    }
    assert!(check_open_size(WindowKind::Doc, 10 * mib, "a.docx").is_ok());
    let over = check_open_size(WindowKind::Doc, 10 * mib + 1, "a.docx").unwrap_err();
    assert!(over.contains("a.docx") && over.contains("10MiB"), "{over}");
    assert!(check_open_size(WindowKind::Hwp, 64 * mib, "a.hwp").is_ok());
    let over = check_open_size(WindowKind::Hwp, 64 * mib + 1, "a.hwp").unwrap_err();
    assert!(over.contains("64MiB"), "{over}");
    assert!(
        check_open_size(WindowKind::Pdf, 0, "a.pdf").is_err(),
        "empty"
    );
}

/// A file opened through a symbolic link is one document under two names: the registry,
/// tracking and saving use the real file, while the title, the page's file type and the recent
/// list keep the name the user opened it by.
#[cfg(unix)]
#[test]
fn a_file_opened_through_a_link_keeps_the_links_name() {
    let dir = scratch_dir("open-link");
    let real = dir.join("data").join("a1b2c3.docx");
    fs::create_dir_all(real.parent().unwrap()).unwrap();
    fs::write(&real, b"x").unwrap();
    let link = dir.join("분기 보고서.docx");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let target = OpenTarget::of(&link).unwrap();
    assert_eq!(target.canonical, fs::canonicalize(&real).unwrap());
    assert_eq!(display_name(&target.shown), "분기 보고서.docx");
    assert_eq!(WindowKind::for_path(&target.shown), Some(WindowKind::Doc));
    // A plain file is shown as the disk names it (the case of the name on a case-insensitive disk).
    let plain = OpenTarget::of(&real).unwrap();
    assert_eq!(plain.shown, plain.canonical);
    assert!(OpenTarget::of(&dir.join("missing.docx")).is_err());
    let _ = fs::remove_dir_all(&dir);
}

/// The size is checked before the read, and the file may grow in between: the read itself is
/// bounded, so no more than the limit (plus the one byte that shows the overrun) is held.
#[test]
fn a_file_that_grew_after_its_size_was_checked_is_not_read_whole() {
    let grown = std::io::repeat(7).take(1_000_000);
    let err = read_within(grown, 4096, 16).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    let exact = read_within(std::io::repeat(7).take(4096), 4096, 4096).unwrap();
    assert_eq!(exact.len(), 4096);
    assert!(read_within(std::io::empty(), 4096, 0).unwrap().is_empty());
}

/// Save As can finish before the page gives up on the open (a password question is still up): the
/// window then belongs to the file it was saved to, which the failed open must not take away.
#[test]
fn an_abandoned_open_leaves_a_file_the_window_was_saved_to_meanwhile() {
    let dir = scratch_dir("abandon-saved");
    let opened = dir.join("a.docx");
    let saved = dir.join("b.docx");
    fs::write(&opened, b"x").unwrap();
    fs::write(&saved, b"y").unwrap();
    let state = AppWindows::new(None);
    locked(&state.registry).register("doc-1-a", WindowKind::Doc);
    let (opened_key, saved_key) = (path_key(&opened), path_key(&saved));
    locked(&state.registry)
        .claim_path("doc-1-a", opened_key.clone())
        .unwrap();
    locked(&state.files).track_opened("doc-1-a", opened, Fingerprint::of(&saved).unwrap());
    // What `note_saved_path` does for a save to `b.docx`.
    locked(&state.registry)
        .claim_path("doc-1-a", saved_key.clone())
        .unwrap();
    locked(&state.files).track("doc-1-a", saved.clone(), Fingerprint::of(&saved).unwrap());
    state.abandon_open("doc-1-a");
    assert_eq!(locked(&state.files).get("doc-1-a").unwrap().path, saved);
    assert_eq!(
        locked(&state.registry).find_by_path(&saved_key),
        Some("doc-1-a")
    );
    // A save to a path another window owns leaves the window untracked but still claiming the file
    // it was opened for: that one is let go.
    locked(&state.files).untrack("doc-1-a");
    locked(&state.registry).release_path("doc-1-a");
    locked(&state.registry)
        .claim_path("doc-1-a", opened_key.clone())
        .unwrap();
    state.abandon_open("doc-1-a");
    assert_eq!(locked(&state.registry).find_by_path(&opened_key), None);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn persisting_writes_the_geometry_the_moves_recorded_and_needs_nothing_else() {
    let dir = std::env::temp_dir().join(format!("eo-geometry-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let state = AppWindows::new(Some(dir.clone()));
    state.persist_geometry();
    assert_eq!(WindowGeometry::load(&dir), None, "nothing recorded yet");
    let moved = WindowGeometry::from_logical((1200.0, 800.0), (140.0, 90.0), false).unwrap();
    *locked(&state.geometry) = Some(moved);
    state.persist_geometry();
    assert_eq!(WindowGeometry::load(&dir), Some(moved));
    let _ = std::fs::remove_dir_all(&dir);
    AppWindows::new(None).persist_geometry();
}

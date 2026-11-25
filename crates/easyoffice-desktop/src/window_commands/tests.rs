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
    locked(&state.files).track_opened("doc-1-a", canonical.clone(), canonical.clone(), fingerprint);
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

/// A link and the file behind it may differ in their extension: the document is saved in place in
/// the format of the name it was opened by, into the file behind the link, and the link stays.
#[cfg(unix)]
#[test]
fn a_link_with_another_extension_than_its_target_still_saves_in_place() {
    let dir = scratch_dir("open-link-ext");
    let real = dir.join("data").join("a1b2c3.bin");
    fs::create_dir_all(real.parent().unwrap()).unwrap();
    fs::write(&real, b"old").unwrap();
    let link = dir.join("분기 보고서.docx");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let OpenTarget { canonical, shown } = OpenTarget::of(&link).unwrap();
    let state = AppWindows::new(None);
    let fingerprint = Fingerprint::of(&canonical).unwrap();
    locked(&state.files).track_opened("doc-1-a", canonical, shown, fingerprint);
    let tracked = locked(&state.files).get("doc-1-a").unwrap();
    assert!(tracked.extension_matches("docx"));
    crate::save_file::save_in_place(&tracked.path, &tracked.fingerprint, b"new", |_| false)
        .unwrap();
    assert_eq!(fs::read(&real).unwrap(), b"new");
    assert!(fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(display_name(&tracked.shown), "분기 보고서.docx");
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn symlinks_to_dotfiles_and_credential_dirs_are_refused() {
    let dir = scratch_dir("open-link-sensitive");
    let targets = [
        dir.join(".zshrc"),
        dir.join(".env"),
        dir.join(".bashrc"),
        dir.join(".ssh").join("id_rsa"),
        dir.join(".gnupg").join("secring.gpg"),
        dir.join(".aws").join("credentials"),
        dir.join(".kube").join("config"),
    ];
    for (idx, real) in targets.iter().enumerate() {
        fs::create_dir_all(real.parent().unwrap()).unwrap();
        fs::write(real, b"secret").unwrap();
        let link = dir.join(format!("문서_{idx}.docx"));
        std::os::unix::fs::symlink(real, &link).unwrap();
        let Err(err) = OpenTarget::of(&link) else {
            panic!("expected PermissionDenied for {real:?}");
        };
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied, "{real:?}");
    }
    // Directory symlink into a credential folder is also refused.
    let dir_link = dir.join("linked_dir");
    std::os::unix::fs::symlink(dir.join(".ssh"), &dir_link).unwrap();
    let via_dir = dir_link.join("id_rsa");
    let Err(err) = OpenTarget::of(&via_dir) else {
        panic!("expected PermissionDenied for {via_dir:?}");
    };
    assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
    // Ancestor directory link into a non-sensitive folder still refuses dotfiles and credential files.
    let home_sim = dir.join("home_sim");
    fs::create_dir_all(&home_sim).unwrap();
    fs::write(home_sim.join(".env"), b"SECRET=1").unwrap();
    fs::write(home_sim.join(".npmrc"), b"//registry/:_authToken=x").unwrap();
    let home_link = dir.join("home_link");
    std::os::unix::fs::symlink(&home_sim, &home_link).unwrap();
    for name in [".env", ".npmrc"] {
        let Err(err) = OpenTarget::of(&home_link.join(name)) else {
            panic!("expected PermissionDenied for ancestor link to {name}");
        };
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
    }
    // PD-177: a symlink to a non-hidden document store file outside sensitive dirs still opens.
    let allowed = dir.join("store").join("a1b2c3.bin");
    fs::create_dir_all(allowed.parent().unwrap()).unwrap();
    fs::write(&allowed, b"content").unwrap();
    let ok_link = dir.join("보고서.docx");
    std::os::unix::fs::symlink(&allowed, &ok_link).unwrap();
    let target = OpenTarget::of(&ok_link).unwrap();
    assert_eq!(target.canonical, fs::canonicalize(&allowed).unwrap());
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
    locked(&state.files).track_opened(
        "doc-1-a",
        opened.clone(),
        opened,
        Fingerprint::of(&saved).unwrap(),
    );
    // What `note_saved_path` does for a save to `b.docx`.
    locked(&state.registry)
        .claim_path("doc-1-a", saved_key.clone())
        .unwrap();
    locked(&state.files).track(
        "doc-1-a",
        saved.clone(),
        saved.clone(),
        Fingerprint::of(&saved).unwrap(),
    );
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

#[test]
fn opening_more_than_batch_limit_is_refused() {
    let files: Vec<PathBuf> = (0..MAX_OPEN_BATCH + 1)
        .map(|i| PathBuf::from(format!("/tmp/doc_{i}.docx")))
        .collect();
    let mut failed_msg = String::new();
    let handled = route_launch_files(
        &files,
        |_| Ok(OpenOutcome::OPENED),
        |msg| failed_msg = msg.to_string(),
    );
    assert!(!handled);
    assert!(failed_msg.contains("한 번에 열 수 있는 파일 한도"));
}

#[test]
fn window_limit_bounds_concurrent_windows() {
    let mut registry = WindowRegistry::default();
    for i in 0..MAX_OPEN_WINDOWS {
        assert!(check_window_limit(&registry).is_ok());
        registry.register(&format!("win-{i}"), WindowKind::Doc);
    }
    let err = check_window_limit(&registry).expect_err("33rd window must be refused");
    assert!(err.contains("동시에 열 수 있는 문서 창 한도"));
    registry.remove("win-0");
    assert!(check_window_limit(&registry).is_ok());
}

#[cfg(windows)]
fn make_ntfs_junction(junction: &Path, target: &Path) {
    let out = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(junction)
        .arg(target)
        .output()
        .expect("cmd mklink /J executed");
    assert!(
        out.status.success(),
        "mklink /J failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[cfg(windows)]
#[test]
fn windows_symlinks_and_ntfs_junctions_open_save_and_refuse_sensitive_targets() {
    use std::os::windows::fs::{symlink_dir, symlink_file, MetadataExt};
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;

    let dir = scratch_dir("win-link-junction");
    let real = dir.join("data").join("a1b2c3.bin");
    fs::create_dir_all(real.parent().unwrap()).unwrap();
    fs::write(&real, b"old-win").unwrap();

    // 1. File symbolic link with a different extension opens as DOCX and saves in-place to target.
    let link = dir.join("분기 보고서.docx");
    symlink_file(&real, &link).unwrap();
    let OpenTarget { canonical, shown } = OpenTarget::of(&link).unwrap();
    assert_eq!(canonical, fs::canonicalize(&real).unwrap());
    assert_eq!(display_name(&shown), "분기 보고서.docx");
    assert!(!shown.to_string_lossy().starts_with(r"\\?\"));
    assert_eq!(WindowKind::for_path(&shown), Some(WindowKind::Doc));

    let state = AppWindows::new(None);
    let fingerprint = Fingerprint::of(&canonical).unwrap();
    locked(&state.files).track_opened("doc-win-1", canonical, shown, fingerprint);
    let tracked = locked(&state.files).get("doc-win-1").unwrap();
    assert!(tracked.extension_matches("docx"));
    crate::save_file::save_in_place(&tracked.path, &tracked.fingerprint, b"new-win", |_| false)
        .unwrap();
    assert_eq!(fs::read(&real).unwrap(), b"new-win");
    assert!(fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());

    // 2. NTFS directory junction (`mklink /J`) deduplicates path_key, opens cleanly, and saves in-place.
    let real_docs = dir.join("real_docs");
    fs::create_dir_all(&real_docs).unwrap();
    let real_doc = real_docs.join("기획안.docx");
    fs::write(&real_doc, b"junc-old").unwrap();
    let junction_docs = dir.join("junction_docs");
    make_ntfs_junction(&junction_docs, &real_docs);

    let via_junc = OpenTarget::of(&junction_docs.join("기획안.docx")).unwrap();
    let direct = OpenTarget::of(&real_doc).unwrap();
    assert_eq!(path_key(&via_junc.canonical), path_key(&direct.canonical));
    assert!(!via_junc.shown.to_string_lossy().starts_with(r"\\?\"));
    let junc_fp = Fingerprint::of(&via_junc.canonical).unwrap();
    crate::save_file::save_in_place(&via_junc.canonical, &junc_fp, b"junc-new", |_| false).unwrap();
    assert_eq!(fs::read(&real_doc).unwrap(), b"junc-new");
    assert_ne!(
        fs::symlink_metadata(&junction_docs)
            .unwrap()
            .file_attributes()
            & FILE_ATTRIBUTE_REPARSE_POINT,
        0
    );

    // 3. Symlinks and NTFS junctions into dotfiles, `.ssh`, or Windows Startup are refused.
    let ssh_dir = dir.join(".ssh");
    fs::create_dir_all(&ssh_dir).unwrap();
    fs::write(ssh_dir.join("id_rsa"), b"secret").unwrap();
    let startup_dir = dir.join("Start Menu").join("Programs").join("Startup");
    fs::create_dir_all(&startup_dir).unwrap();
    fs::write(startup_dir.join("autorun.bat"), b"rem").unwrap();
    let home_sim = dir.join("home_sim");
    fs::create_dir_all(&home_sim).unwrap();
    fs::write(home_sim.join(".env"), b"SECRET=1").unwrap();
    fs::write(home_sim.join(".npmrc"), b"token").unwrap();

    let bad_file_link = dir.join("비밀키.docx");
    symlink_file(ssh_dir.join("id_rsa"), &bad_file_link).unwrap();
    assert_eq!(
        OpenTarget::of(&bad_file_link).err().unwrap().kind(),
        std::io::ErrorKind::PermissionDenied
    );

    let dir_sym_home = dir.join("sym_home");
    symlink_dir(&home_sim, &dir_sym_home).unwrap();
    assert_eq!(
        OpenTarget::of(&dir_sym_home.join(".env"))
            .err()
            .unwrap()
            .kind(),
        std::io::ErrorKind::PermissionDenied
    );

    let junc_ssh = dir.join("junc_ssh");
    make_ntfs_junction(&junc_ssh, &ssh_dir);
    assert_eq!(
        OpenTarget::of(&junc_ssh.join("id_rsa"))
            .err()
            .unwrap()
            .kind(),
        std::io::ErrorKind::PermissionDenied
    );

    let junc_startup = dir.join("junc_startup");
    make_ntfs_junction(&junc_startup, &startup_dir);
    assert_eq!(
        OpenTarget::of(&junc_startup.join("autorun.bat"))
            .err()
            .unwrap()
            .kind(),
        std::io::ErrorKind::PermissionDenied
    );

    let junc_home = dir.join("junc_home");
    make_ntfs_junction(&junc_home, &home_sim);
    assert_eq!(
        OpenTarget::of(&junc_home.join(".npmrc"))
            .err()
            .unwrap()
            .kind(),
        std::io::ErrorKind::PermissionDenied
    );

    // 4. Dangling NTFS junction fails closed.
    let gone_dir = dir.join("gone_target");
    fs::create_dir_all(&gone_dir).unwrap();
    let dangling_junc = dir.join("dangling_junc");
    make_ntfs_junction(&dangling_junc, &gone_dir);
    fs::remove_dir_all(&gone_dir).unwrap();
    assert!(OpenTarget::of(&dangling_junc.join("doc.docx")).is_err());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn launch_files_and_urls_accept_pages_bundle_directories_and_read_open_target_succeeds() {
    let dir =
        std::env::temp_dir().join(format!("easyoffice-open-pages-dir-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let plain_dir = dir.join("plain_folder");
    let bundle_dir = dir.join("Report.PAGES");
    fs::create_dir_all(&plain_dir).unwrap();
    fs::create_dir_all(bundle_dir.join("Index")).unwrap();
    fs::write(
        bundle_dir.join("Index").join("Document.iwa"),
        b"iwa-payload",
    )
    .unwrap();

    let picked = launch_files(
        [
            plain_dir.to_string_lossy().into_owned(),
            bundle_dir.to_string_lossy().into_owned(),
        ]
        .into_iter(),
    );
    assert_eq!(picked, vec![bundle_dir.clone()]);

    let urls = vec![
        tauri::Url::from_file_path(&plain_dir).unwrap(),
        tauri::Url::from_file_path(&bundle_dir).unwrap(),
    ];
    assert_eq!(file_url_paths(&urls), vec![bundle_dir.clone()]);

    let target = OpenTarget::of(&bundle_dir).expect("safe pages dir");
    let kind = WindowKind::for_path(&target.shown).unwrap();
    assert_eq!(kind, WindowKind::Doc);
    let (bytes, meta) = read_open_target(&target.canonical, kind, "Report.PAGES").expect("opens");
    assert!(!bytes.is_empty());
    let fp = Fingerprint::from_metadata_and_bytes(&meta, &bytes);
    assert_eq!(fp, Fingerprint::of(&target.canonical).unwrap());

    // Recent list keeps `.pages/` directory entries and drops ordinary folders.
    let mut recents = crate::recent::RecentList::default();
    recents.add(plain_dir);
    recents.add(bundle_dir.clone());
    let views = recents.views();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].name, "Report.PAGES");

    // Empty bundle is rejected on open without leaking full directory path.
    let empty_bundle = dir.join("empty.pages");
    fs::create_dir_all(&empty_bundle).unwrap();
    let err = read_open_target(&empty_bundle, WindowKind::Doc, "empty.pages").unwrap_err();
    assert!(!err.contains(&dir.to_string_lossy().into_owned()), "{err}");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn export_only_dialog_save_preserves_original_tracked_file_and_duplicate_open_lock() {
    let dir = scratch_dir("export-only-track");
    let orig = dir.join("원본.docx");
    let other_win = dir.join("다른창.docx");
    let export_same = dir.join("배포본.docx");
    let export_pdf = dir.join("배포본.pdf");
    let export_pages = dir.join("배포본.pages");
    let save_as_target = dir.join("새이름.docx");
    for p in [
        &orig,
        &other_win,
        &export_same,
        &export_pdf,
        &export_pages,
        &save_as_target,
    ] {
        fs::write(p, p.to_string_lossy().as_bytes()).unwrap();
    }

    let orig_canon = fs::canonicalize(&orig).unwrap();
    let other_canon = fs::canonicalize(&other_win).unwrap();
    let orig_fp = Fingerprint::of(&orig_canon).unwrap();
    let other_fp = Fingerprint::of(&other_canon).unwrap();

    let state = AppWindows::new(None);
    locked(&state.registry).register("doc-1-a", WindowKind::Doc);
    locked(&state.registry)
        .claim_path("doc-1-a", path_key(&orig_canon))
        .unwrap();
    locked(&state.files).track_opened("doc-1-a", orig_canon.clone(), orig_canon.clone(), orig_fp);

    locked(&state.registry).register("doc-2-b", WindowKind::Doc);
    locked(&state.registry)
        .claim_path("doc-2-b", path_key(&other_canon))
        .unwrap();
    locked(&state.files).track_opened(
        "doc-2-b",
        other_canon.clone(),
        other_canon.clone(),
        other_fp,
    );

    // 1. Exporting same-extension copy (`배포본.docx`), `.pdf`, `.pages`, or even overwriting another
    // window's path (`다른창.docx`) with `track_saved = false` keeps `원본.docx` tracked & locked.
    for target in [&export_same, &export_pdf, &export_pages, &other_win] {
        let fp = Fingerprint::of(target).unwrap();
        assert_eq!(
            state.finish_dialog_save("doc-1-a", target, Some(fp), false),
            None
        );
        let tracked = locked(&state.files).get("doc-1-a").unwrap();
        assert_eq!(tracked.path, orig_canon);
        assert_eq!(tracked.fingerprint, orig_fp);
        assert_eq!(
            locked(&state.registry).find_by_path(&path_key(&orig_canon)),
            Some("doc-1-a")
        );
        assert_eq!(
            locked(&state.registry).find_by_path(&path_key(&other_canon)),
            Some("doc-2-b")
        );
    }
    assert_eq!(
        locked(&state.registry).find_by_path(&path_key(&fs::canonicalize(&export_same).unwrap())),
        None
    );

    // 2. Normal Save As (`track_saved = true`) updates tracking & transfers duplicate-open claim.
    let save_as_fp = Fingerprint::of(&save_as_target).unwrap();
    let shown = state
        .finish_dialog_save("doc-1-a", &save_as_target, Some(save_as_fp), true)
        .unwrap();
    let save_as_canon = fs::canonicalize(&save_as_target).unwrap();
    assert_eq!(shown, save_as_canon);
    assert_eq!(
        locked(&state.files).get("doc-1-a").unwrap().path,
        save_as_canon
    );
    assert_eq!(
        locked(&state.registry).find_by_path(&path_key(&orig_canon)),
        None
    );
    assert_eq!(
        locked(&state.registry).find_by_path(&path_key(&save_as_canon)),
        Some("doc-1-a")
    );

    let _ = fs::remove_dir_all(&dir);
}

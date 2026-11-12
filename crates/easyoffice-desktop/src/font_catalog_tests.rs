use super::*;
use crate::pdf_fonts::tests::bundled_font_dir;
use std::fs;
use std::time::Instant;

/// A catalog over hand-made files, so the read rules do not depend on an installed font.
fn catalog_of(files: Vec<PathBuf>) -> Catalog {
    Catalog {
        files,
        faces: Vec::new(),
    }
}

/// A file that starts with `head` and is `len` bytes long (sparse when large).
fn file_with(dir: &Path, name: &str, head: &[u8], len: u64) -> PathBuf {
    let path = dir.join(name);
    let mut bytes = head.to_vec();
    bytes.resize(len.max(head.len() as u64) as usize, 0);
    fs::write(&path, bytes).unwrap();
    path
}

fn listed(fonts: &SystemFonts, label: &str, files: Vec<PathBuf>) -> String {
    fonts.list(label, || catalog_of(files)).unwrap().revision
}

#[test]
fn a_listed_file_is_read_whole() {
    let dir = tempfile::tempdir().unwrap();
    let path = file_with(dir.path(), "a.ttf", &[0, 1, 0, 0], 4096);
    let fonts = SystemFonts::default();
    let revision = listed(&fonts, "main", vec![path.clone()]);
    assert_eq!(
        fonts.read("main", &revision, 0).unwrap(),
        fs::read(path).unwrap()
    );
}

#[test]
fn every_font_signature_is_accepted_and_nothing_else() {
    let dir = tempfile::tempdir().unwrap();
    let mut files = Vec::new();
    for (i, head) in [&[0u8, 1, 0, 0][..], b"true", b"OTTO", b"ttcf"]
        .iter()
        .enumerate()
    {
        files.push(file_with(dir.path(), &format!("ok{i}.ttf"), head, 64));
    }
    for (i, head) in [&b"wOFF"[..], b"%PDF", b"\0\x02\0", b"MZ\x90\0", b"<svg"]
        .iter()
        .enumerate()
    {
        files.push(file_with(dir.path(), &format!("bad{i}.ttf"), head, 64));
    }
    let fonts = SystemFonts::default();
    let revision = listed(&fonts, "main", files);
    for file in 0..4 {
        assert!(fonts.read("main", &revision, file).is_ok(), "{file}");
    }
    for file in 4..9 {
        assert_eq!(
            fonts.read("main", &revision, file),
            Err(MSG_NOT_A_FONT.to_string()),
            "{file}"
        );
    }
}

#[test]
fn an_empty_or_oversized_file_is_refused_before_it_is_read() {
    let dir = tempfile::tempdir().unwrap();
    let empty = dir.path().join("empty.ttf");
    fs::write(&empty, b"").unwrap();
    let huge = file_with(dir.path(), "huge.ttf", b"OTTO", 4);
    File::options()
        .write(true)
        .open(&huge)
        .unwrap()
        .set_len(MAX_FONT_BYTES + 1)
        .unwrap();
    let fonts = SystemFonts::with_budget(u64::MAX);
    let revision = listed(&fonts, "main", vec![empty, huge]);
    assert_eq!(
        fonts.read("main", &revision, 0),
        Err(MSG_TOO_LARGE.to_string())
    );
    assert_eq!(
        fonts.read("main", &revision, 1),
        Err(MSG_TOO_LARGE.to_string())
    );
}

#[test]
fn a_file_that_grows_past_the_bound_while_it_is_read_is_refused() {
    let mut endless = std::io::repeat(0);
    assert_eq!(read_font(&mut endless), Err(MSG_TOO_LARGE.to_string()));
    let mut exact = std::io::repeat(0).take(MAX_FONT_BYTES);
    assert_eq!(
        read_font(&mut exact),
        Err(MSG_NOT_A_FONT.to_string()),
        "the bound itself is read; the signature decides"
    );
}

#[test]
fn only_numbers_of_the_windows_current_list_work() {
    let dir = tempfile::tempdir().unwrap();
    let path = file_with(dir.path(), "a.ttf", b"OTTO", 64);
    let fonts = SystemFonts::default();
    let main = listed(&fonts, "main", vec![path.clone()]);
    let other = listed(&fonts, "doc-1", vec![path]);
    assert_ne!(main, other);
    assert_eq!(main.len(), REVISION_BYTES * 2);
    assert!(main.bytes().all(|b| b.is_ascii_hexdigit()));

    assert!(fonts.read("main", &main, 0).is_ok());
    assert_eq!(fonts.read("main", &other, 0), Err(MSG_NO_LIST.to_string()));
    assert_eq!(
        fonts.read("doc-1", &main, 0),
        Err(MSG_NO_LIST.to_string()),
        "another window's list"
    );
    assert_eq!(fonts.read("main", &main, 1), Err(MSG_NO_LIST.to_string()));
    assert_eq!(
        fonts.read("main", &main, u32::MAX),
        Err(MSG_NO_LIST.to_string())
    );
    assert_eq!(fonts.read("main", "", 0), Err(MSG_NO_LIST.to_string()));
    assert_eq!(
        fonts.read("unknown", &main, 0),
        Err(MSG_NO_LIST.to_string()),
        "a window without a list"
    );

    let newer = fonts.list("main", || unreachable!("built once")).unwrap();
    assert_ne!(newer.revision, main);
    assert_eq!(
        fonts.read("main", &main, 0),
        Err(MSG_NO_LIST.to_string()),
        "an older list"
    );
    assert!(fonts.read("main", &newer.revision, 0).is_ok());

    fonts.forget("main");
    assert_eq!(
        fonts.read("main", &newer.revision, 0),
        Err(MSG_NO_LIST.to_string()),
        "a closed window"
    );
    assert!(fonts.read("doc-1", &other, 0).is_ok(), "other windows stay");
}

#[test]
fn a_window_is_served_a_bounded_amount_of_distinct_fonts() {
    let dir = tempfile::tempdir().unwrap();
    let files: Vec<PathBuf> = (0..3)
        .map(|i| file_with(dir.path(), &format!("f{i}.ttf"), b"OTTO", 100))
        .collect();
    let fonts = SystemFonts::with_budget(250);
    let revision = listed(&fonts, "main", files.clone());
    assert!(fonts.read("main", &revision, 0).is_ok());
    assert!(fonts.read("main", &revision, 1).is_ok());
    assert_eq!(
        fonts.read("main", &revision, 2),
        Err(MSG_BUDGET.to_string()),
        "300 > 250"
    );
    assert!(
        fonts.read("main", &revision, 0).is_ok(),
        "a font already served costs nothing again"
    );
    assert!(fonts.read("main", &revision, 1).is_ok());
    // The budget belongs to the window.
    let other = listed(&fonts, "doc-1", files.clone());
    assert!(fonts.read("doc-1", &other, 2).is_ok());
}

/// A window asking for a new list (the page reloads, or loops) must not start a new budget: the
/// limit is for the window's life, and what it was served stays served.
#[test]
fn a_new_list_does_not_give_the_window_a_new_budget() {
    let dir = tempfile::tempdir().unwrap();
    let files: Vec<PathBuf> = (0..3)
        .map(|i| file_with(dir.path(), &format!("f{i}.ttf"), b"OTTO", 100))
        .collect();
    let fonts = SystemFonts::with_budget(250);
    let first = listed(&fonts, "main", files.clone());
    assert!(fonts.read("main", &first, 0).is_ok());
    assert!(fonts.read("main", &first, 1).is_ok());
    let again = fonts.list("main", || unreachable!()).unwrap().revision;
    assert_ne!(first, again);
    assert_eq!(
        fonts.read("main", &first, 0),
        Err(MSG_NO_LIST.to_string()),
        "the old list's numbers stop working"
    );
    assert_eq!(
        fonts.read("main", &again, 2),
        Err(MSG_BUDGET.to_string()),
        "200 already served, 300 > 250"
    );
    assert!(
        fonts.read("main", &again, 0).is_ok(),
        "a font served before the new list costs nothing again"
    );
}

/// Only a regular file is read: opening a named pipe would block the worker until some program
/// writes to it.
#[cfg(unix)]
#[test]
fn a_file_that_is_not_a_regular_file_is_refused_without_being_opened() {
    let dir = tempfile::tempdir().unwrap();
    let pipe = dir.path().join("pipe.ttf");
    let made = std::process::Command::new("mkfifo").arg(&pipe).status();
    if !made.is_ok_and(|status| status.success()) {
        return;
    }
    let fonts = Arc::new(SystemFonts::default());
    let revision = listed(&fonts, "main", vec![pipe.clone()]);
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = Arc::clone(&fonts);
    std::thread::spawn(move || {
        let _ = tx.send(reader.read("main", &revision, 0));
    });
    let answer = rx.recv_timeout(std::time::Duration::from_secs(5));
    if answer.is_err() {
        // A reader stuck in `open` (a failing run): a writer lets it go, so the test ends.
        let _ = fs::OpenOptions::new().write(true).open(&pipe);
    }
    assert_eq!(answer, Ok(Err(MSG_UNREADABLE.to_string())));
}

#[test]
fn a_refused_read_gives_its_budget_back() {
    let dir = tempfile::tempdir().unwrap();
    let bad = file_with(dir.path(), "bad.ttf", b"%PDF", 200);
    let good = file_with(dir.path(), "good.ttf", b"OTTO", 200);
    let fonts = SystemFonts::with_budget(250);
    let revision = listed(&fonts, "main", vec![bad, good]);
    for _ in 0..3 {
        assert_eq!(
            fonts.read("main", &revision, 0),
            Err(MSG_NOT_A_FONT.to_string())
        );
    }
    assert!(fonts.read("main", &revision, 1).is_ok());
}

/// A window asks for a new list while one of its reads is in flight (the page reloads). The read
/// held a reservation made under the old revision; it must still be given back when the read
/// fails and must still count as served when it succeeds.
#[test]
fn a_new_list_during_a_read_neither_leaks_nor_double_charges() {
    let dir = tempfile::tempdir().unwrap();
    let files: Vec<PathBuf> = (0..2)
        .map(|i| file_with(dir.path(), &format!("f{i}.ttf"), b"OTTO", 100))
        .collect();
    let fonts = SystemFonts::with_budget(250);
    let old = listed(&fonts, "main", files);
    let bytes_of = |fonts: &SystemFonts| fonts.lock().get("main").unwrap().bytes;

    let failing = fonts.charge("main", &old, 0, 100).unwrap();
    assert!(failing.is_some());
    let new = fonts.list("main", || unreachable!()).unwrap().revision;
    fonts.settle("main", failing, 0, 100, false);
    assert_eq!(bytes_of(&fonts), 0, "a failed read gives its bytes back");

    let good = fonts.charge("main", &new, 0, 100).unwrap();
    fonts.list("main", || unreachable!()).unwrap();
    fonts.settle("main", good, 0, 100, true);
    assert_eq!(bytes_of(&fonts), 100);
    assert!(fonts.lock().get("main").unwrap().served.contains(&0));

    let latest = fonts.list("main", || unreachable!()).unwrap().revision;
    assert!(
        fonts.charge("main", &latest, 0, 100).unwrap().is_none(),
        "served once"
    );
    assert!(fonts.read("main", &latest, 1).is_ok());
    assert_eq!(
        bytes_of(&fonts),
        200,
        "100 + 100, file 0 was not charged twice"
    );
}

/// A read that ends after its window closed and a new window took the same label settles nothing
/// in the new session.
#[test]
fn a_read_that_outlives_its_window_does_not_touch_the_next_window_of_the_same_label() {
    let dir = tempfile::tempdir().unwrap();
    let files = vec![file_with(dir.path(), "f0.ttf", b"OTTO", 100)];
    let fonts = SystemFonts::with_budget(250);
    let revision = listed(&fonts, "main", files);
    let stale = fonts.charge("main", &revision, 0, 100).unwrap();
    fonts.forget("main");
    let next = fonts.list("main", || unreachable!()).unwrap().revision;
    let fresh = fonts.charge("main", &next, 0, 100).unwrap();
    assert!(fresh.is_some());
    fonts.settle("main", stale, 0, 100, false);
    let sessions = fonts.lock();
    assert_eq!(sessions.get("main").unwrap().bytes, 100);
}

#[test]
fn a_file_that_vanished_is_refused_without_naming_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = file_with(dir.path(), "gone.ttf", b"OTTO", 64);
    let fonts = SystemFonts::default();
    let revision = listed(&fonts, "main", vec![path.clone()]);
    fs::remove_file(&path).unwrap();
    let error = fonts.read("main", &revision, 0).unwrap_err();
    assert_eq!(error, MSG_UNREADABLE);
    assert!(!error.contains(dir.path().to_str().unwrap()));
}

#[test]
fn names_put_english_first_and_stay_within_the_editor_bounds() {
    let families = vec![
        ("맑은 고딕".to_string(), Language::Korean_Korea),
        (
            "  Malgun Gothic ".to_string(),
            Language::English_UnitedStates,
        ),
        ("Malgun Gothic".to_string(), Language::English_UnitedKingdom),
        (String::new(), Language::English_UnitedStates),
        ("x".repeat(MAX_NAME_CHARS + 1), Language::Korean_Korea),
    ];
    assert_eq!(family_names(&families), vec!["Malgun Gothic", "맑은 고딕"]);
    assert_eq!(bounded(&"x".repeat(MAX_NAME_CHARS)).len(), MAX_NAME_CHARS);
}

#[test]
fn styles_read_as_weight_then_slant() {
    assert_eq!(style_name(400, "normal"), "Regular");
    assert_eq!(style_name(400, "italic"), "Italic");
    assert_eq!(style_name(700, "normal"), "Bold");
    assert_eq!(style_name(700, "italic"), "Bold Italic");
    assert_eq!(style_name(300, "oblique"), "Light Oblique");
    assert_eq!(style_name(450, "normal"), "450");
    assert_eq!(style_name(450, "italic"), "450 italic");
}

/// A database over a copy of the bundled face (and of it again, renamed) in a temporary folder.
fn database_of_copies() -> Option<(tempfile::TempDir, Database)> {
    let source = bundled_font_dir()?.join("NotoSansKR-Regular.ttf");
    let dir = tempfile::tempdir().unwrap();
    fs::copy(&source, dir.path().join("one.ttf")).unwrap();
    fs::copy(&source, dir.path().join("two.TTF")).unwrap();
    fs::write(dir.path().join("notes.txt"), b"not a font").unwrap();
    let mut db = Database::new();
    db.load_fonts_dir(dir.path());
    Some((dir, db))
}

#[test]
fn the_list_has_names_and_numbers_but_no_path() {
    let Some((dir, db)) = database_of_copies() else {
        return;
    };
    let catalog = Catalog::from_database(&db, &[]);
    assert_eq!(
        catalog.faces.len(),
        1,
        "the same family, weight and slant once"
    );
    let face = &catalog.faces[0];
    assert_eq!(face.family, "Noto Sans KR");
    assert_eq!(face.full_name, "Noto Sans KR");
    assert_eq!(face.style, "Regular");
    assert_eq!(
        (face.weight, face.slant, face.face_index),
        (400, "normal", 0)
    );
    assert_eq!(catalog.files.len(), 1);

    let fonts = SystemFonts::default();
    let list = fonts.list("main", || catalog).unwrap();
    let json = serde_json::to_string(&list).unwrap();
    let root = dir.path().to_str().unwrap();
    assert!(!json.contains(root) && !json.contains("one.ttf") && !json.contains(".ttf"));
    assert!(json.contains(r#""faceIndex":0"#) && json.contains(r#""postscriptName""#));
    let bytes = fonts
        .read("main", &list.revision, list.faces[0].file)
        .unwrap();
    assert!(bytes.starts_with(&[0, 1, 0, 0]) || bytes.starts_with(b"OTTO"));
}

#[test]
fn a_skipped_folder_is_left_out_of_the_list() {
    let Some((dir, db)) = database_of_copies() else {
        return;
    };
    let skipped = Catalog::from_database(&db, &[dir.path().to_path_buf()]);
    assert_eq!(skipped.faces.len(), 0);
    assert!(skipped.files.is_empty());
}

/// Time to build the list from the real database and to read a font, on this machine:
/// `cargo test --release -p easyoffice-desktop system_fonts_are_measured -- --ignored --nocapture`.
#[test]
#[ignore = "measurement"]
fn system_fonts_are_measured() {
    let dirs: Vec<PathBuf> = bundled_font_dir().into_iter().collect();
    let started = Instant::now();
    let db = pdf_export::shared_font_db(&dirs);
    let scanned = started.elapsed();
    let started = Instant::now();
    let catalog = Catalog::from_database(&db, &dirs);
    let built = started.elapsed();
    let fonts = SystemFonts::default();
    let started = Instant::now();
    let list = fonts.list("main", || catalog).unwrap();
    let json = serde_json::to_string(&list).unwrap();
    let listed = started.elapsed();
    let sizes: Vec<u64> = fonts
        .catalog
        .get()
        .unwrap()
        .files
        .iter()
        .map(|path| fs::metadata(path).map_or(0, |meta| meta.len()))
        .collect();
    let largest = sizes
        .iter()
        .copied()
        .enumerate()
        .max_by_key(|(_, size)| *size);
    println!(
        "database {scanned:?}; catalog {built:?} ({} faces, {} files); list+json {listed:?} ({} bytes); largest file {:?}",
        list.faces.len(),
        sizes.len(),
        json.len(),
        largest.map(|(_, size)| size),
    );
    for (file, _) in sizes.iter().enumerate().take(3) {
        let started = Instant::now();
        let read = fonts.read("main", &list.revision, file as u32);
        println!(
            "read file {file}: {:?} in {:?}",
            read.map(|b| b.len()),
            started.elapsed()
        );
    }
    if let Some((file, _)) = largest {
        let started = Instant::now();
        let read = fonts.read("main", &list.revision, file as u32);
        println!(
            "read largest: {:?} in {:?}",
            read.map(|b| b.len()),
            started.elapsed()
        );
    }
}

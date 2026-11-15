use super::*;
use easyoffice_formats::{
    base64_codec, DocumentFormat, UniversalDocumentModel, UniversalFormatConverter,
};
use std::collections::HashSet;

fn scratch_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "easyoffice-desktop-save-{tag}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A complete document of `format`, written by the engine.
fn document(format: DocumentFormat) -> Vec<u8> {
    let model = UniversalDocumentModel {
        title: "저장 시험".to_string(),
        paragraphs: vec!["본문".to_string()],
        ..Default::default()
    };
    UniversalFormatConverter::encode(&model, format).unwrap()
}

fn saved_bytes(result: InPlaceResult) -> usize {
    match result {
        InPlaceResult::Saved(written) => written.bytes,
        InPlaceResult::Declined => panic!("declined"),
    }
}

fn assert_no_leftovers(dir: &Path) {
    let left: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(TEMP_SUFFIX))
        .collect();
    assert!(left.is_empty(), "temporary files left: {left:?}");
}

#[test]
fn parse_keeps_only_the_file_name_and_decodes_bytes() {
    let docx = document(DocumentFormat::Docx);
    let req =
        SaveRequest::parse("..\\..\\Windows/보고서.DOCX", &base64_codec::encode(&docx)).unwrap();
    assert_eq!(req.file_name(), "보고서.DOCX");
    assert_eq!(req.extension(), "docx");
    assert_eq!(req.filter_label(), "DOCX 문서");
    assert_eq!(req.bytes(), docx.as_slice());
}

#[test]
fn shell_made_bytes_get_the_same_name_rules_and_limits() {
    let pdf = document(DocumentFormat::Pdf);
    let req = SaveRequest::from_bytes("C:\\x\\보고서.PDF", pdf).unwrap();
    assert_eq!((req.file_name(), req.extension()), ("보고서.PDF", "pdf"));
    assert!(SaveRequest::from_bytes("noext", vec![1]).is_err());
    assert!(SaveRequest::from_bytes("a.pdf", Vec::new()).is_err());
}

#[test]
fn parse_rejects_bad_names_and_payloads() {
    assert!(SaveRequest::parse("noext", "UEsDBA==").is_err());
    assert!(SaveRequest::parse(".docx", "UEsDBA==").is_err());
    assert!(SaveRequest::parse("a.do cx", "UEsDBA==").is_err());
    assert!(SaveRequest::parse("a.docx", "***").is_err());
    assert!(SaveRequest::parse("a.docx", "").is_err());
}

#[test]
fn damaged_documents_never_become_a_request() {
    let docx = document(DocumentFormat::Docx);
    let truncated = base64_codec::encode(&docx[..docx.len() / 2]);
    let header_only = base64_codec::encode(b"PK\x03\x04");
    for payload in [truncated, header_only] {
        let err = SaveRequest::parse("a.docx", &payload).unwrap_err();
        assert_eq!(err, REFUSED_MESSAGE);
    }
    let err = SaveRequest::from_bytes("a.pdf", b"%PDF-1.7 cut off".to_vec()).unwrap_err();
    assert_eq!(err, REFUSED_MESSAGE);
    assert!(!REFUSED_MESSAGE.contains("Fail-Closed"));
}

#[test]
fn write_replaces_existing_file_atomically() {
    let dir = scratch_dir("write");
    let target = dir.join("문서.hwpx");
    fs::write(&target, b"old").unwrap();
    let written = write_document_bytes(&target, b"new-bytes").unwrap();
    assert_eq!(written.bytes, 9);
    assert_eq!(fs::read(&target).unwrap(), b"new-bytes");
    assert_no_leftovers(&dir);
    let _ = fs::remove_dir_all(&dir);
}

/// Renaming the temporary file over a symbolic link would replace the link with a regular file
/// and leave the file it pointed at unchanged: the write goes to the target instead.
#[cfg(unix)]
#[test]
fn a_write_through_a_symbolic_link_updates_the_target_and_keeps_the_link() {
    let dir = scratch_dir("symlink");
    let real = dir.join("real").join("원본.docx");
    fs::create_dir_all(real.parent().unwrap()).unwrap();
    fs::write(&real, b"old").unwrap();
    let link = dir.join("바로가기.docx");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let written = write_document_bytes(&link, b"new-bytes").unwrap();
    assert_eq!(written.bytes, 9);
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink(),
        "the link is still a link"
    );
    assert_eq!(
        fs::read(&real).unwrap(),
        b"new-bytes",
        "the target is saved"
    );
    assert_eq!(written.fingerprint, Some(Fingerprint::of(&real).unwrap()));
    assert_no_leftovers(&dir);
    assert_no_leftovers(real.parent().unwrap());
    // A link that points nowhere is not written through (nothing is created behind it).
    let dangling = dir.join("끊김.docx");
    std::os::unix::fs::symlink(dir.join("nowhere.docx"), &dangling).unwrap();
    assert!(write_document_bytes(&dangling, b"x").is_err());
    assert!(!dir.join("nowhere.docx").exists());
    assert!(fs::symlink_metadata(&dangling)
        .unwrap()
        .file_type()
        .is_symlink());
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn a_write_through_a_link_to_sensitive_or_dotfile_target_is_refused() {
    let dir = scratch_dir("symlink-sensitive");
    for (idx, rel) in [
        ".zshrc",
        ".env",
        ".ssh/id_rsa",
        ".gnupg/secring.gpg",
        ".aws/credentials",
        ".kube/config",
    ]
    .iter()
    .enumerate()
    {
        let real = dir.join(rel);
        fs::create_dir_all(real.parent().unwrap()).unwrap();
        fs::write(&real, b"untouched").unwrap();
        let link = dir.join(format!("계약서_{idx}.docx"));
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let err = write_document_bytes(&link, b"overwrite").unwrap_err();
        assert_eq!(err.to_string(), "쓰기 권한이 없습니다.", "{rel}");
        assert_eq!(fs::read(&real).unwrap(), b"untouched");
    }
    // Directory symlink into a credential folder is also refused.
    let dir_link = dir.join("linked_dir");
    std::os::unix::fs::symlink(dir.join(".ssh"), &dir_link).unwrap();
    let via_dir = dir_link.join("authorized_keys");
    assert_eq!(
        write_document_bytes(&via_dir, b"overwrite")
            .unwrap_err()
            .to_string(),
        "쓰기 권한이 없습니다."
    );
    assert!(!dir.join(".ssh").join("authorized_keys").exists());
    // PD-177: a link with a different extension outside sensitive paths remains writable.
    let allowed = dir.join("store").join("a1b2c3.bin");
    fs::create_dir_all(allowed.parent().unwrap()).unwrap();
    fs::write(&allowed, b"old").unwrap();
    let ok_link = dir.join("보고서.docx");
    std::os::unix::fs::symlink(&allowed, &ok_link).unwrap();
    write_document_bytes(&ok_link, b"new").unwrap();
    assert_eq!(fs::read(&allowed).unwrap(), b"new");
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn a_write_through_ancestor_symlink_to_sensitive_or_dotfile_is_refused() {
    let dir = scratch_dir("ancestor-symlink");
    let home_sim = dir.join("simulated_home");
    fs::create_dir_all(&home_sim).unwrap();
    let zshrc = home_sim.join(".zshrc");
    fs::write(&zshrc, b"original zshrc").unwrap();

    let link_parent = dir.join("link_to_home");
    std::os::unix::fs::symlink(&home_sim, &link_parent).unwrap();

    let target = link_parent.join(".zshrc");
    let err = write_document_bytes(&target, b"malicious content").unwrap_err();
    assert_eq!(err.to_string(), "쓰기 권한이 없습니다.");
    assert_eq!(fs::read(&zshrc).unwrap(), b"original zshrc");

    // Normal non-dotfile file through the directory symlink should still succeed
    let normal_target = link_parent.join("normal.docx");
    assert!(write_document_bytes(&normal_target, b"safe doc").is_ok());
    assert_eq!(fs::read(home_sim.join("normal.docx")).unwrap(), b"safe doc");

    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn write_claimed_file_writes_the_claimed_descriptor_without_reopening_the_path() {
    let dir = scratch_dir("claimed-fd");
    let target = dir.join("split_1.pdf");
    let victim = dir.join("victim.txt");
    fs::write(&victim, b"untouched").unwrap();
    let mut claimed = claim_new_file(&target).unwrap();
    // Second claim at the same path fails atomically.
    assert_eq!(
        claim_new_file(&target).unwrap_err().kind(),
        std::io::ErrorKind::AlreadyExists
    );
    // Simulate a path swap after claim: remove target and drop a symlink at the same path.
    fs::remove_file(&target).unwrap();
    std::os::unix::fs::symlink(&victim, &target).unwrap();
    assert_eq!(
        claim_new_file(&target).unwrap_err().kind(),
        std::io::ErrorKind::AlreadyExists
    );
    // Writing to the claimed descriptor never touches the symlink or its victim.
    write_claimed_file(&mut claimed, b"payload").unwrap();
    assert_eq!(fs::read(&victim).unwrap(), b"untouched");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_fingerprint_of_a_write_matches_the_file_on_disk() {
    let dir = scratch_dir("fingerprint");
    let target = dir.join("a.docx");
    let written = write_document_bytes(&target, b"content").unwrap();
    assert_eq!(written.fingerprint, Some(Fingerprint::of(&target).unwrap()));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn write_failure_is_reported_and_leaves_no_temp_file() {
    let dir = scratch_dir("fail");
    let target = dir.join("missing-dir").join("a.pdf");
    let err = write_document_bytes(&target, b"x").unwrap_err();
    assert_eq!(err.to_string(), "파일을 저장하지 못했습니다 (a.pdf)");
    assert!(!err.to_string().contains("missing-dir"), "no folder: {err}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_rename_keeps_the_original_and_removes_the_temp_file() {
    let dir = scratch_dir("rename-fail");
    let target = dir.join("a.docx");
    fs::create_dir_all(target.join("inner")).unwrap();
    fs::write(target.join("inner").join("keep.txt"), b"keep").unwrap();
    assert!(write_document_bytes(&target, b"new").is_err());
    assert_eq!(
        fs::read(target.join("inner").join("keep.txt")).unwrap(),
        b"keep"
    );
    assert_no_leftovers(&dir);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn temporary_names_are_unique_and_live_next_to_the_target() {
    let dir = scratch_dir("names");
    let target = dir.join("a.docx");
    let mut names = HashSet::new();
    let mut held = Vec::new();
    for _ in 0..200 {
        let (tmp, file) = create_unique_sibling(&target).unwrap();
        assert_eq!(tmp.parent(), target.parent());
        assert!(tmp.to_string_lossy().ends_with(TEMP_SUFFIX));
        assert!(names.insert(tmp.clone()), "name reused: {tmp:?}");
        held.push((tmp, file));
    }
    for (tmp, file) in held {
        drop(file);
        fs::remove_file(tmp).unwrap();
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn concurrent_saves_into_one_folder_do_not_collide() {
    let dir = scratch_dir("concurrent");
    let shared = dir.join("shared.docx");
    let handles: Vec<_> = (0..8_u8)
        .map(|worker| {
            let shared = shared.clone();
            let own = dir.join(format!("own-{worker}.docx"));
            std::thread::spawn(move || {
                let payload = vec![worker; 4096];
                for _ in 0..25 {
                    let raced = write_document_bytes(&shared, &payload);
                    assert!(raced.is_ok() || cfg!(windows), "{raced:?}");
                    write_document_bytes(&own, &payload).unwrap();
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    let bytes = fs::read(&shared).unwrap();
    assert_eq!(bytes.len(), 4096);
    assert!(bytes.iter().all(|b| *b == bytes[0]));
    for worker in 0..8_u8 {
        let own = fs::read(dir.join(format!("own-{worker}.docx"))).unwrap();
        assert_eq!(own, vec![worker; 4096]);
    }
    assert_no_leftovers(&dir);
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn the_temporary_file_is_born_with_the_targets_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch_dir("born");
    let target = dir.join("a.docx");
    fs::write(&target, b"old").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    let (tmp, file) = create_unique_sibling(&target).unwrap();
    let mode = file.metadata().unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    drop(file);
    fs::remove_file(tmp).unwrap();
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn a_replaced_file_keeps_its_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch_dir("mode");
    let target = dir.join("a.docx");
    fs::write(&target, b"old").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
    write_document_bytes(&target, b"new").unwrap();
    let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o640);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn in_place_save_writes_unchanged_files_without_asking() {
    let dir = scratch_dir("inplace");
    let target = dir.join("a.docx");
    fs::write(&target, b"old").unwrap();
    let seen = Fingerprint::of(&target).unwrap();
    let result = save_in_place(&target, &seen, b"new!", |_| panic!("must not ask")).unwrap();
    assert_eq!(saved_bytes(result), 4);
    assert_eq!(fs::read(&target).unwrap(), b"new!");
    assert_no_leftovers(&dir);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn external_change_asks_and_respects_the_answer() {
    let dir = scratch_dir("conflict");
    let target = dir.join("a.hwp");
    fs::write(&target, b"old").unwrap();
    let seen = Fingerprint::of(&target).unwrap();
    fs::write(&target, b"edited elsewhere").unwrap();
    let declined = save_in_place(&target, &seen, b"mine", |_| false).unwrap();
    assert_eq!(declined, InPlaceResult::Declined);
    assert_eq!(fs::read(&target).unwrap(), b"edited elsewhere");
    let mut asked = None;
    let accepted = save_in_place(&target, &seen, b"mine", |state| {
        asked = Some(state);
        true
    })
    .unwrap();
    assert_eq!(asked, Some(DiskState::Changed));
    assert_eq!(saved_bytes(accepted), 4);
    assert_eq!(fs::read(&target).unwrap(), b"mine");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_file_that_changes_again_while_the_question_is_open_is_asked_about_again() {
    let dir = scratch_dir("conflict-again");
    let target = dir.join("a.hwp");
    fs::write(&target, b"old").unwrap();
    let seen = Fingerprint::of(&target).unwrap();
    fs::write(&target, b"edited elsewhere").unwrap();
    let mut asked = 0;
    let result = save_in_place(&target, &seen, b"mine", |_| {
        asked += 1;
        if asked == 1 {
            fs::write(&target, b"edited again while the dialog was up").unwrap();
            return true;
        }
        false
    })
    .unwrap();
    assert_eq!(asked, 2, "the second change is asked about too");
    assert_eq!(result, InPlaceResult::Declined);
    assert_eq!(
        fs::read(&target).unwrap(),
        b"edited again while the dialog was up"
    );
    fs::write(&target, b"edited elsewhere").unwrap();
    let mut asked = 0;
    let accepted = save_in_place(&target, &seen, b"mine", |_| {
        asked += 1;
        true
    })
    .unwrap();
    assert_eq!((asked, saved_bytes(accepted)), (1, 4));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_file_that_keeps_changing_is_asked_about_a_few_times_and_then_left_alone() {
    let dir = scratch_dir("conflict-churn");
    let target = dir.join("a.hwp");
    fs::write(&target, b"old").unwrap();
    let seen = Fingerprint::of(&target).unwrap();
    fs::write(&target, b"0").unwrap();
    let mut asked = 0;
    let result = save_in_place(&target, &seen, b"mine", |_| {
        asked += 1;
        fs::write(&target, asked.to_string()).unwrap();
        true
    })
    .unwrap();
    assert_eq!(result, InPlaceResult::Declined);
    assert_eq!(asked, MAX_CONFIRMATIONS);
    assert_ne!(fs::read(&target).unwrap(), b"mine");
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn a_link_that_is_retargeted_while_the_question_is_open_is_asked_about_again() {
    let dir = scratch_dir("conflict-retarget");
    let (first, second) = (dir.join("first.docx"), dir.join("second.docx"));
    let link = dir.join("link.docx");
    fs::write(&first, b"old").unwrap();
    fs::write(&second, b"old").unwrap();
    std::os::unix::fs::symlink(&first, &link).unwrap();
    let seen = Fingerprint::of(&first).unwrap();
    fs::write(&first, b"edited elsewhere").unwrap();
    let mut asked = 0;
    let result = save_in_place(&link, &seen, b"mine", |_| {
        asked += 1;
        if asked == 1 {
            fs::remove_file(&link).unwrap();
            std::os::unix::fs::symlink(&second, &link).unwrap();
            fs::write(&second, b"edited elsewhere").unwrap();
            return true;
        }
        false
    })
    .unwrap();
    assert_eq!((asked, result), (2, InPlaceResult::Declined));
    assert_eq!(fs::read(&second).unwrap(), b"edited elsewhere");
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn a_tracked_real_path_replaced_by_a_link_while_the_question_is_open_is_asked_about_again() {
    let dir = scratch_dir("conflict-replaced");
    let (tracked, other) = (dir.join("a.docx"), dir.join("other.docx"));
    fs::write(&tracked, b"old").unwrap();
    let tracked = fs::canonicalize(&tracked).unwrap();
    let seen = Fingerprint::of(&tracked).unwrap();
    fs::write(&tracked, b"edited elsewhere").unwrap();
    fs::write(&other, b"edited elsewhere").unwrap();
    let mut asked = 0;
    let result = save_in_place(&tracked, &seen, b"mine", |_| {
        asked += 1;
        if asked == 1 {
            fs::remove_file(&tracked).unwrap();
            std::os::unix::fs::symlink(&other, &tracked).unwrap();
            return true;
        }
        false
    })
    .unwrap();
    assert_eq!((asked, result), (2, InPlaceResult::Declined));
    assert_eq!(fs::read(&other).unwrap(), b"edited elsewhere");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_deleted_or_moved_original_asks_before_it_is_recreated() {
    let dir = scratch_dir("gone");
    let target = dir.join("a.pdf");
    fs::write(&target, b"old").unwrap();
    let seen = Fingerprint::of(&target).unwrap();
    fs::remove_file(&target).unwrap();

    let declined = save_in_place(&target, &seen, b"back", |state| {
        assert_eq!(state, DiskState::Missing);
        false
    })
    .unwrap();
    assert_eq!(declined, InPlaceResult::Declined);
    assert!(!target.exists(), "declining recreates nothing");
    assert_no_leftovers(&dir);

    let mut asked = 0;
    let accepted = save_in_place(&target, &seen, b"back", |state| {
        assert_eq!(state, DiskState::Missing);
        asked += 1;
        true
    })
    .unwrap();
    assert_eq!(asked, 1);
    assert_eq!(saved_bytes(accepted), 4);
    assert_eq!(fs::read(&target).unwrap(), b"back");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn outcomes_serialize_for_the_webview() {
    let saved =
        serde_json::to_string(&SaveOutcome::saved(Path::new("/home/me/docs/a.pdf"), 3)).unwrap();
    assert_eq!(saved, r#"{"status":"saved","name":"a.pdf","bytes":3}"#);
    let cancelled = serde_json::to_string(&SaveOutcome::cancelled()).unwrap();
    assert_eq!(cancelled, r#"{"status":"cancelled","name":null,"bytes":0}"#);
    for path in ["/home/me/docs/보고서.hwpx", "relative/dir/b.docx", "c.txt"] {
        let json = serde_json::to_string(&SaveOutcome::saved(Path::new(path), 1)).unwrap();
        assert!(
            !json.contains('/') && !json.contains('\\'),
            "no directory leaks: {json}"
        );
    }
}

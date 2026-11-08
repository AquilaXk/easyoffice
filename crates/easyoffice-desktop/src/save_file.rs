//! Validation and atomic writing for the desktop "save document" command.
//!
//! A [`SaveRequest`] only exists for bytes that passed [`verify_document_bytes`], so no write path
//! sees an unchecked document (PD-109). The write itself follows a reference implementation (MIT,
//! Copyright (c) 2025-2026 Edward Kim; see THIRD_PARTY_NOTICES.md), `apps/desktop/src-tauri/src/
//! state.rs` lines 621-640: a temporary file next to the target, `sync_all`, then rename. The
//! temporary file is created by hand instead of with `tempfile::NamedTempFile`, which creates files
//! with mode 0600 on Unix and would tighten the permissions of every document it replaces; here a
//! new file keeps the process's usual mode and a replaced file keeps the mode it had.

use crate::save_verify::{verify_document_bytes, REFUSED_MESSAGE};
use crate::tracked_files::{disk_state, DiskState, Fingerprint};
use easyoffice_app::EasyOfficeIpcRouter;
use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Upper bound for one saved document (decoded bytes).
pub const MAX_SAVE_BYTES: usize = 256 * 1_048_576;

/// Suffix of the temporary file written next to the target (a leftover after a crash is easy to
/// recognise and safe to delete).
const TEMP_SUFFIX: &str = ".easyoffice-saving";

/// Attempts at a free temporary name; a name only repeats if another process left one behind.
const MAX_TEMP_NAME_ATTEMPTS: u32 = 16;

/// A save request from the WebView: a proposed file name (never a path) plus the document bytes.
#[derive(Debug)]
pub struct SaveRequest {
    file_name: String,
    extension: String,
    bytes: Vec<u8>,
}

/// Validates a proposed file name and returns `(file name, lower-case extension)`.
pub(crate) fn split_file_name(file_name: &str) -> Result<(String, String), String> {
    // Keep only the final component so a proposed name can never point at another directory.
    let base = file_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    let Some((stem, ext)) = base.rsplit_once('.') else {
        return Err("저장할 파일 이름에 확장자가 없습니다.".to_string());
    };
    let ext_ok =
        !ext.is_empty() && ext.len() <= 8 && ext.chars().all(|c| c.is_ascii_alphanumeric());
    if stem.trim().is_empty() || !ext_ok {
        return Err(format!("저장할 수 없는 파일 이름입니다: {base}"));
    }
    let ext = ext.to_ascii_lowercase();
    Ok((base, ext))
}

impl SaveRequest {
    /// A request for bytes the shell produced itself (for example a searchable PDF).
    pub fn from_bytes(file_name: &str, bytes: Vec<u8>) -> Result<Self, String> {
        let (file_name, extension) = split_file_name(file_name)?;
        if bytes.is_empty() {
            return Err("저장할 문서 데이터가 비어 있습니다.".to_string());
        }
        if bytes.len() > MAX_SAVE_BYTES {
            return Err("저장할 문서가 허용 크기(256MiB)를 초과합니다.".to_string());
        }
        Self::verified(file_name, extension, bytes)
    }

    pub fn parse(file_name: &str, base64: &str) -> Result<Self, String> {
        let (base, extension) = split_file_name(file_name)?;
        if base64.len() / 4 * 3 > MAX_SAVE_BYTES + 3 {
            return Err("저장할 문서가 허용 크기(256MiB)를 초과합니다.".to_string());
        }
        let bytes = EasyOfficeIpcRouter::decode_base64(base64.trim())
            .ok_or_else(|| "저장할 문서 데이터가 올바른 Base64가 아닙니다.".to_string())?;
        if bytes.is_empty() {
            return Err("저장할 문서 데이터가 비어 있습니다.".to_string());
        }
        if bytes.len() > MAX_SAVE_BYTES {
            return Err("저장할 문서가 허용 크기(256MiB)를 초과합니다.".to_string());
        }
        Self::verified(base, extension, bytes)
    }

    /// The one way to build a request: the bytes must be a complete document of the extension's
    /// format. The engine's reason goes to the log, the user gets a fixed sentence.
    fn verified(file_name: String, extension: String, bytes: Vec<u8>) -> Result<Self, String> {
        verify_document_bytes(&extension, &bytes).map_err(|err| {
            eprintln!("save: refused .{extension} bytes: {err}");
            REFUSED_MESSAGE.to_string()
        })?;
        Ok(Self {
            file_name,
            extension,
            bytes,
        })
    }

    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    pub fn extension(&self) -> &str {
        &self.extension
    }

    pub fn filter_label(&self) -> String {
        format!("{} 문서", self.extension.to_ascii_uppercase())
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SaveOutcome {
    status: &'static str,
    name: Option<String>,
    bytes: usize,
}

impl SaveOutcome {
    /// A save reports the file name only, never a path: the page never learns the directory
    /// (PD-088). Rust keeps the path in the window's tracked file.
    pub fn saved(path: &Path, bytes: usize) -> Self {
        Self {
            status: "saved",
            name: path.file_name().map(|n| n.to_string_lossy().into_owned()),
            bytes,
        }
    }

    pub fn cancelled() -> Self {
        Self {
            status: "cancelled",
            name: None,
            bytes: 0,
        }
    }
}

/// What a successful write left behind: the byte count and, when the file could be inspected
/// right after the rename, its fingerprint (metadata of the file on disk, hash of the bytes written,
/// so nothing is read back and a change by another program in between still shows up later).
#[derive(Debug, PartialEq, Eq)]
pub struct Written {
    pub bytes: usize,
    pub fingerprint: Option<Fingerprint>,
}

/// Writes through a uniquely named sibling temporary file and renames it into place, so a failed
/// write never leaves a truncated document where an existing one used to be, and two saves into the
/// same folder never share a temporary file. `fs::rename` replaces an existing file on Windows too
/// (`MoveFileExW` with `MOVEFILE_REPLACE_EXISTING`).
pub fn write_document_bytes(path: &Path, bytes: &[u8]) -> Result<Written, String> {
    let failed =
        |err: std::io::Error| format!("파일을 저장하지 못했습니다 ({}): {err}", path.display());
    let (tmp_path, tmp_file) = create_unique_sibling(path).map_err(failed)?;
    if let Err(err) = write_then_rename(&tmp_path, tmp_file, path, bytes) {
        let _ = fs::remove_file(&tmp_path);
        return Err(failed(err));
    }
    let fingerprint = fs::metadata(path)
        .ok()
        .map(|meta| Fingerprint::from_metadata_and_bytes(&meta, bytes));
    Ok(Written {
        bytes: bytes.len(),
        fingerprint,
    })
}

/// Options that create the temporary file with the permissions the target already has, so the
/// document is never readable by more people than before, not even while it is being written. The
/// process umask can only narrow the mode here; `keep_mode_of` restores the exact bits afterwards.
/// A new target gets the usual mode (0666 minus the umask on Unix).
fn sibling_open_options(target: &Path) -> fs::OpenOptions {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    if let Ok(meta) = fs::metadata(target) {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode(meta.permissions().mode());
    }
    #[cfg(not(unix))]
    let _ = target;
    options
}

/// Creates a new, empty file next to `path` under a name nobody else can be using: the process id,
/// a per-process counter and the clock make a repeat unlikely, and `create_new` makes it harmless
/// (the open fails instead of reusing the file, and the next name is tried).
fn create_unique_sibling(path: &Path) -> std::io::Result<(PathBuf, fs::File)> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    path.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "저장 위치가 파일이 아닙니다",
        )
    })?;
    let mut last_err = None;
    for _ in 0..MAX_TEMP_NAME_ATTEMPTS {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let name = format!(
            ".{}-{}-{nanos:x}{TEMP_SUFFIX}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let candidate = path.with_file_name(name);
        match sibling_open_options(path).open(&candidate) {
            Ok(file) => return Ok((candidate, file)),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => last_err = Some(err),
            Err(err) => return Err(err),
        }
    }
    Err(last_err.unwrap_or_else(|| std::io::ErrorKind::AlreadyExists.into()))
}

/// Result of an in-place save.
#[derive(Debug, PartialEq, Eq)]
pub enum InPlaceResult {
    Saved(Written),
    /// The file changed on disk and the user chose not to overwrite it.
    Declined,
}

/// Saves over the file the document came from. If the file no longer matches the fingerprint taken
/// when it was opened, `confirm_overwrite` decides (the shell shows a native dialog); the write itself
/// is the same temp-file + fsync + rename as every other save.
pub fn save_in_place(
    path: &Path,
    expected: &Fingerprint,
    bytes: &[u8],
    confirm_overwrite: impl FnOnce() -> bool,
) -> Result<InPlaceResult, String> {
    if disk_state(path, expected) == DiskState::Changed && !confirm_overwrite() {
        return Ok(InPlaceResult::Declined);
    }
    write_document_bytes(path, bytes).map(InPlaceResult::Saved)
}

fn write_then_rename(
    tmp_path: &Path,
    mut file: fs::File,
    path: &Path,
    bytes: &[u8],
) -> std::io::Result<()> {
    file.write_all(bytes)?;
    file.sync_all()?;
    keep_mode_of(&file, path);
    drop(file);
    fs::rename(tmp_path, path)
}

/// A replaced document keeps its permission bits (best effort: some volumes refuse `chmod`, and the
/// save is worth more than the mode). Only Unix has modes to keep; on Windows a read-only target
/// stays unwritable for the rename anyway.
#[cfg(unix)]
fn keep_mode_of(temp: &fs::File, target: &Path) {
    if let Ok(meta) = fs::metadata(target) {
        let _ = temp.set_permissions(meta.permissions());
    }
}

#[cfg(not(unix))]
fn keep_mode_of(_temp: &fs::File, _target: &Path) {}

#[cfg(test)]
mod tests {
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
        let req = SaveRequest::parse("..\\..\\Windows/보고서.DOCX", &base64_codec::encode(&docx))
            .unwrap();
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
        assert!(err.contains("파일을 저장하지 못했습니다"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_rename_keeps_the_original_and_removes_the_temp_file() {
        let dir = scratch_dir("rename-fail");
        // A non-empty directory cannot be replaced by a file: the rename fails after the temp
        // file was written, which is the late failure that must not damage anything.
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
                        // Windows may refuse a rename onto a file another rename is replacing;
                        // what must hold everywhere is that nothing is torn or left behind.
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
        // The shared file is exactly one worker's complete payload, never a mix.
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
        // Checked before anything is written or changed: no wider mode at any moment.
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
        let result = save_in_place(&target, &seen, b"new!", || panic!("must not ask")).unwrap();
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
        let declined = save_in_place(&target, &seen, b"mine", || false).unwrap();
        assert_eq!(declined, InPlaceResult::Declined);
        assert_eq!(fs::read(&target).unwrap(), b"edited elsewhere");
        let mut asked = false;
        let accepted = save_in_place(&target, &seen, b"mine", || {
            asked = true;
            true
        })
        .unwrap();
        assert!(asked);
        assert_eq!(saved_bytes(accepted), 4);
        assert_eq!(fs::read(&target).unwrap(), b"mine");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_deleted_original_is_recreated_without_asking() {
        let dir = scratch_dir("gone");
        let target = dir.join("a.pdf");
        fs::write(&target, b"old").unwrap();
        let seen = Fingerprint::of(&target).unwrap();
        fs::remove_file(&target).unwrap();
        let result = save_in_place(&target, &seen, b"back", || panic!("must not ask")).unwrap();
        assert_eq!(saved_bytes(result), 4);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn outcomes_serialize_for_the_webview() {
        let saved = serde_json::to_string(&SaveOutcome::saved(Path::new("/home/me/docs/a.pdf"), 3))
            .unwrap();
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
}

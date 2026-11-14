//! Validation and atomic writing for the desktop "save document" command.
//!
//! A [`SaveRequest`] only exists for bytes that passed [`verify_document_bytes`], so no write path
//! sees an unchecked document (PD-109). The write itself follows a reference implementation (MIT,
//! Copyright (c) 2025-2026 Edward Kim; see THIRD_PARTY_NOTICES.md), `apps/desktop/src-tauri/src/
//! state.rs` lines 621-640: a temporary file next to the target, `sync_all`, then rename. The
//! temporary file is created by hand instead of with `tempfile::NamedTempFile`, which creates files
//! with mode 0600 on Unix and would tighten the permissions of every document it replaces; here a
//! new file keeps the process's usual mode and a replaced file keeps the mode it had.

use crate::save_error::SaveError;
use crate::save_verify::{verify_document_bytes, REFUSED_MESSAGE};
use crate::tracked_files::{disk_view, DiskState, Fingerprint};
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
/// How many times the user is asked about a file that keeps changing under the question.
const MAX_CONFIRMATIONS: usize = 3;

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

    /// Puts the first page, drawn from `svg`, into the document as its preview picture (PD-132). The
    /// save never depends on it: when the picture cannot be made, the result does not pass the
    /// check, or any step panics (the SVG, font and container code all handle untrusted input), the
    /// document stays exactly as it was and the reason goes to the log. `font_dirs` are the folders
    /// of the shared font database, which is built inside the guarded step.
    pub fn with_thumbnail(self, svg: &str, font_dirs: &[PathBuf]) -> Self {
        self.with_thumbnail_using(|extension, bytes| {
            let fonts = crate::pdf_export::shared_font_db(font_dirs);
            crate::save_thumbnail::with_first_page(extension, bytes, svg, &fonts)
        })
    }

    /// [`Self::with_thumbnail`] with the step that makes the new bytes given by the caller (a test
    /// hook: the guard around it is what keeps a panic from failing the save).
    pub(crate) fn with_thumbnail_using(
        self,
        make: impl FnOnce(&str, &[u8]) -> Result<Vec<u8>, String>,
    ) -> Self {
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            make(&self.extension, &self.bytes)
        }));
        match attempt {
            Ok(Ok(bytes)) => Self { bytes, ..self },
            Ok(Err(reason)) => {
                eprintln!("save: no preview picture in .{}: {reason}", self.extension);
                self
            }
            Err(_) => {
                eprintln!(
                    "save: no preview picture in .{}: the step panicked",
                    self.extension
                );
                self
            }
        }
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
/// (`MoveFileExW` with `MOVEFILE_REPLACE_EXISTING`). A symbolic link as `path` is saved through to
/// its target.
pub fn write_document_bytes(path: &Path, bytes: &[u8]) -> Result<Written, SaveError> {
    let failed = |err: std::io::Error| SaveError::from_io(path, &err);
    let path = &resolve_link(path).map_err(failed)?;
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

const SENSITIVE_CREDENTIAL_DIRS: &[&[u8]] = &[b".ssh", b".gnupg", b".aws", b".kube"];

/// Rejects symlink targets pointing at hidden dotfiles (`.zshrc`, `.env`, …) or inside sensitive
/// credential directories (`.ssh`, `.gnupg`, `.aws`, `.kube`) while allowing normal non-hidden
/// targets regardless of extension (PD-177).
pub(crate) fn ensure_safe_symlink_target(canonical: &Path) -> std::io::Result<()> {
    let is_dotfile = canonical
        .file_name()
        .is_some_and(|name| name.as_encoded_bytes().starts_with(b"."));
    let in_sensitive_dir = canonical.components().any(|comp| match comp {
        std::path::Component::Normal(part) => SENSITIVE_CREDENTIAL_DIRS
            .iter()
            .any(|dir| part.as_encoded_bytes().eq_ignore_ascii_case(dir)),
        _ => false,
    });
    if is_dotfile || in_sensitive_dir {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "숨김 파일이나 자격증명 디렉터리를 가리키는 심볼릭 링크는 허용되지 않습니다.",
        ));
    }
    Ok(())
}

/// The file a save really replaces. A symbolic link is followed: renaming the temporary file over
/// the link would swap the link for a regular file and leave its target unchanged. A link that
/// points nowhere is an error, so nothing is created behind it.
fn resolve_link(path: &Path) -> std::io::Result<PathBuf> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            let canonical = fs::canonicalize(path)?;
            ensure_safe_symlink_target(&canonical)?;
            Ok(canonical)
        }
        _ => Ok(path.to_path_buf()),
    }
}

/// Creates `path` exclusively (`create_new(true)`) and returns the open file handle so callers write
/// directly to the claimed descriptor without reopening by path.
pub(crate) fn claim_new_file(path: &Path) -> std::io::Result<fs::File> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
}

/// Writes `bytes` to an already claimed file handle and flushes file contents to disk.
pub(crate) fn write_claimed_file(file: &mut fs::File, bytes: &[u8]) -> std::io::Result<()> {
    file.write_all(bytes)?;
    file.sync_all()
}

/// Flushes directory entries on Unix after rename/create so crash recovery keeps the link target.
#[cfg(unix)]
pub(crate) fn sync_parent_dir(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
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
    /// The file changed or disappeared on disk and the user chose not to write over it.
    Declined,
}

/// Saves over the file the document came from. If the file no longer matches the fingerprint taken
/// when it was opened, or is gone (deleted or moved), `confirm_overwrite` decides with what was found
/// (the shell shows a native dialog, as the reference app does); the write itself is the same
/// temp-file + fsync + rename as every other save. The question can stay open for a long time, so
/// the disk is looked at again once it is answered: a file that turned into something else meanwhile
/// (changed once more, or a link pointed at another file) is asked about again, and one that keeps
/// changing is left alone after [`MAX_CONFIRMATIONS`] questions.
pub fn save_in_place(
    path: &Path,
    expected: &Fingerprint,
    bytes: &[u8],
    mut confirm_overwrite: impl FnMut(DiskState) -> bool,
) -> Result<InPlaceResult, SaveError> {
    let mut asked_about = disk_view(path, expected);
    let mut questions = 0;
    while asked_about.state != DiskState::Unchanged {
        if questions == MAX_CONFIRMATIONS || !confirm_overwrite(asked_about.state) {
            return Ok(InPlaceResult::Declined);
        }
        questions += 1;
        let after = disk_view(path, expected);
        if after == asked_about {
            break;
        }
        asked_about = after;
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
    fs::rename(tmp_path, path)?;
    #[cfg(unix)]
    sync_parent_dir(path)?;
    Ok(())
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

    /// The question stays open for as long as the user takes: what the file turns into meanwhile is
    /// not what was shown, so the answer is only good for the file it was given about.
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
        // An answer about a file that stayed as it was is enough.
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

    /// The path the question was about is a link that is pointed somewhere else while it is up.
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

    /// The window tracks the real file (no link in its path): what can change under the question is
    /// that the name now leads somewhere else, a link to a file of the same content.
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

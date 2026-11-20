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

    #[allow(dead_code)]
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

    pub fn parse_owned(file_name: &str, base64: String) -> Result<Self, String> {
        let (base, extension) = split_file_name(file_name)?;
        if base64.len() / 4 * 3 > MAX_SAVE_BYTES + 3 {
            return Err("저장할 문서가 허용 크기(256MiB)를 초과합니다.".to_string());
        }
        let bytes = EasyOfficeIpcRouter::decode_base64(base64.trim())
            .ok_or_else(|| "저장할 문서 데이터가 올바른 Base64가 아닙니다.".to_string())?;
        drop(base64);
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

    /// Draws and embeds a first-page preview picture when the caller supplied no SVG (non-Hangul
    /// saves and converted files, PD-239). Skips immediately via central-directory/OLE2 seek when
    /// the format has no preview slot or the document already holds a non-placeholder thumbnail.
    pub fn with_drawn_thumbnail(self, font_dirs: &[PathBuf]) -> Self {
        let supported = easyoffice_formats::DocumentFormat::from_extension(&self.extension)
            .is_some_and(easyoffice_formats::supports_thumbnail);
        if !supported || crate::save_thumbnail::has_non_placeholder_thumbnail(&self.bytes) {
            return self;
        }
        self.with_thumbnail_using(|extension, bytes| {
            let fonts = crate::pdf_export::shared_font_db(font_dirs);
            crate::save_thumbnail::with_drawn_first_page(extension, bytes, &fonts)
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
        match self.extension.as_str() {
            "eobase" => "데이터베이스 (BASE)".to_string(),
            "eoboard" => "보드 (BOARD)".to_string(),
            _ => format!("{} 문서", self.extension.to_ascii_uppercase()),
        }
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

pub(crate) use crate::sensitive_path::{
    ensure_safe_symlink_target, in_sensitive_credential_dir, sensitive_symlink_error,
};

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
        _ => {
            ensure_safe_symlink_target(path)?;
            if let Ok(canon_existing) = fs::canonicalize(path) {
                ensure_safe_symlink_target(&canon_existing)?;
            }
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                if let Ok(canon_parent) = fs::canonicalize(parent) {
                    if in_sensitive_credential_dir(&canon_parent) {
                        return Err(sensitive_symlink_error());
                    }
                    if let Some(file_name) = path.file_name() {
                        ensure_safe_symlink_target(&canon_parent.join(file_name))?;
                    }
                }
            }
            Ok(path.to_path_buf())
        }
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

/// Flushes directory entries on Unix after rename/create so crash recovery keeps the link target
/// (best effort: network, FUSE, or exFAT volumes may refuse `fsync` on a directory).
#[cfg(unix)]
pub(crate) fn sync_parent_dir(path: &Path) {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        let _ = fs::File::open(parent).and_then(|dir| dir.sync_all());
    }
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
    sync_parent_dir(path);
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
#[path = "save_file_tests.rs"]
mod tests;

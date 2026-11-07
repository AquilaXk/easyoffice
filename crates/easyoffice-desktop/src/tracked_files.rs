//! The window label <-> file path map used by in-place saving (#133). Like the registry in
//! `windows.rs`, it lives in Rust only: the WebView never sees a path, only whether a save worked.
//! Each entry also keeps the size and modification time observed when the document was opened (or
//! last written), so a save can notice that another program changed the file in the meantime.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Size and modification time of a file at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fingerprint {
    len: u64,
    modified_nanos: Option<u128>,
}

impl Fingerprint {
    pub fn from_metadata(meta: &fs::Metadata) -> Self {
        let modified_nanos = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_nanos());
        Self {
            len: meta.len(),
            modified_nanos,
        }
    }

    pub fn of(path: &Path) -> std::io::Result<Self> {
        fs::metadata(path).map(|meta| Self::from_metadata(&meta))
    }
}

/// How the file on disk compares with what the window last saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskState {
    Unchanged,
    /// Another program changed (or replaced) the file after it was opened.
    Changed,
    /// The file is gone; saving simply recreates it.
    Missing,
}

pub fn disk_state(path: &Path, expected: &Fingerprint) -> DiskState {
    match Fingerprint::of(path) {
        Ok(now) if now == *expected => DiskState::Unchanged,
        Ok(_) => DiskState::Changed,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => DiskState::Missing,
        // Unreadable metadata (permissions, sharing violation): be careful and ask.
        Err(_) => DiskState::Changed,
    }
}

#[derive(Debug, Clone)]
pub struct TrackedFile {
    pub path: PathBuf,
    pub fingerprint: Fingerprint,
}

impl TrackedFile {
    /// In-place saving keeps the format: a different extension goes through "save as" instead.
    pub fn extension_matches(&self, extension: &str) -> bool {
        self.path
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|e| e.eq_ignore_ascii_case(extension))
    }
}

#[derive(Debug, Default)]
pub struct TrackedFiles {
    by_label: HashMap<String, TrackedFile>,
}

impl TrackedFiles {
    pub fn track(&mut self, label: &str, path: PathBuf, fingerprint: Fingerprint) {
        self.by_label
            .insert(label.to_string(), TrackedFile { path, fingerprint });
    }

    pub fn get(&self, label: &str) -> Option<TrackedFile> {
        self.by_label.get(label).cloned()
    }

    pub fn untrack(&mut self, label: &str) {
        self.by_label.remove(label);
    }

    pub fn set_fingerprint(&mut self, label: &str, fingerprint: Fingerprint) {
        if let Some(entry) = self.by_label.get_mut(label) {
            entry.fingerprint = fingerprint;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("easyoffice-tracked-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn detects_changed_and_missing_files() {
        let dir = scratch("state");
        let file = dir.join("a.docx");
        fs::write(&file, b"one").unwrap();
        let seen = Fingerprint::of(&file).unwrap();
        assert_eq!(disk_state(&file, &seen), DiskState::Unchanged);
        fs::write(&file, b"three bytes more").unwrap();
        assert_eq!(disk_state(&file, &seen), DiskState::Changed);
        fs::remove_file(&file).unwrap();
        assert_eq!(disk_state(&file, &seen), DiskState::Missing);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn map_tracks_untracks_and_updates_per_window() {
        let dir = scratch("map");
        let file = dir.join("b.hwpx");
        fs::write(&file, b"x").unwrap();
        let fp = Fingerprint::of(&file).unwrap();
        let mut files = TrackedFiles::default();
        files.track("doc-1", file.clone(), fp);
        let got = files.get("doc-1").unwrap();
        assert!(got.extension_matches("HWPX"));
        assert!(!got.extension_matches("hwp"));
        fs::write(&file, b"longer now").unwrap();
        files.set_fingerprint("doc-1", Fingerprint::of(&file).unwrap());
        assert_eq!(
            disk_state(&file, &files.get("doc-1").unwrap().fingerprint),
            DiskState::Unchanged
        );
        files.untrack("doc-1");
        assert!(files.get("doc-1").is_none());
        let _ = fs::remove_dir_all(&dir);
    }
}

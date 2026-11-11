//! The window label <-> file path map used by in-place saving. Like the registry in
//! `windows.rs`, it lives in Rust only: the WebView never sees a path, only whether a save worked.
//! Each entry also keeps a fingerprint of the file as it was opened (or last written): size,
//! modification time and a hash of the content, so a save can notice that another program changed
//! the file in the meantime (PD-109).
//!
//! The content hash follows a reference implementation (MIT, Copyright (c) 2025-2026 Edward Kim;
//! see THIRD_PARTY_NOTICES.md): `apps/desktop/src-tauri/src/state.rs` lines 643-690
//! (`file_fingerprint`, `hash_reader`), a streamed hash in a fixed buffer next to the length and
//! the modification time. EasyOffice differences: a
//! 64-bit SipHash-1-3 from the standard library instead of 32-bit FNV-1a (about 6 times faster on
//! 64 MiB and without FNV's weak spots), a read cap, and a fingerprint built from bytes the caller
//! already holds so the open and save paths do not read the file a second time.

use crate::save_file::MAX_SAVE_BYTES;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fs;
use std::hash::Hasher;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Size of the stack buffer the file is streamed through.
const HASH_BUFFER_BYTES: usize = 64 * 1024;

/// Size, modification time and content hash of a file at one moment. The hash only lives in this
/// process (it is never stored), so the standard hasher's unspecified algorithm is not a problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fingerprint {
    len: u64,
    modified_nanos: Option<u128>,
    content_hash: u64,
}

impl Fingerprint {
    /// For bytes the caller already holds: the fingerprint of `meta` (taken before the bytes were
    /// read, so a change in between shows up as a difference) with the hash of `bytes`.
    pub fn from_metadata_and_bytes(meta: &fs::Metadata, bytes: &[u8]) -> Self {
        let mut hasher = DefaultHasher::new();
        hasher.write(bytes);
        Self::from_parts(meta, hasher.finish())
    }

    /// Reads the file once: metadata from the opened handle, then the content through a fixed
    /// buffer. Files above the save limit hash only their first bytes; the length already differs.
    pub fn of(path: &Path) -> std::io::Result<Self> {
        let file = fs::File::open(path)?;
        let meta = file.metadata()?;
        let content_hash = hash_reader(file.take(MAX_SAVE_BYTES as u64 + 1))?;
        Ok(Self::from_parts(&meta, content_hash))
    }

    fn from_parts(meta: &fs::Metadata, content_hash: u64) -> Self {
        let modified_nanos = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_nanos());
        Self {
            len: meta.len(),
            modified_nanos,
            content_hash,
        }
    }
}

/// Streams `reader` through a stack buffer into a 64-bit hash, without allocating.
fn hash_reader(mut reader: impl Read) -> std::io::Result<u64> {
    let mut hasher = DefaultHasher::new();
    let mut buffer = [0_u8; HASH_BUFFER_BYTES];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(hasher.finish()),
            Ok(read) => hasher.write(&buffer[..read]),
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
            Err(err) => return Err(err),
        }
    }
}

/// How the file on disk compares with what the window last saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskState {
    Unchanged,
    /// Another program changed (or replaced) the file after it was opened.
    Changed,
    /// The file is gone (deleted or moved); saving recreates it, after asking.
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
    /// The window was opened for this file and nothing was saved to another file since.
    pub from_open: bool,
}

fn canonical_ext(ext: &str) -> &str {
    if ext.eq_ignore_ascii_case("markdown") {
        "md"
    } else {
        ext
    }
}

/// Whether the last extension of `path` is `extension` (`markdown` counts as `md`).
pub fn path_has_extension(path: &Path, extension: &str) -> bool {
    let want = canonical_ext(extension);
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|e| canonical_ext(e).eq_ignore_ascii_case(want))
}

impl TrackedFile {
    /// In-place saving keeps the format: a different extension goes through "save as" instead.
    pub fn extension_matches(&self, extension: &str) -> bool {
        path_has_extension(&self.path, extension)
    }
}

#[derive(Debug, Default)]
pub struct TrackedFiles {
    by_label: HashMap<String, TrackedFile>,
}

impl TrackedFiles {
    /// The file the window was saved to.
    pub fn track(&mut self, label: &str, path: PathBuf, fingerprint: Fingerprint) {
        self.insert(label, path, fingerprint, false);
    }

    /// The file the window was opened for.
    pub fn track_opened(&mut self, label: &str, path: PathBuf, fingerprint: Fingerprint) {
        self.insert(label, path, fingerprint, true);
    }

    fn insert(&mut self, label: &str, path: PathBuf, fingerprint: Fingerprint, from_open: bool) {
        self.by_label.insert(
            label.to_string(),
            TrackedFile {
                path,
                fingerprint,
                from_open,
            },
        );
    }

    /// The window was saved to a file of its own (after it was opened, or from the start).
    pub fn is_saved_file(&self, label: &str) -> bool {
        self.by_label
            .get(label)
            .is_some_and(|tracked| !tracked.from_open)
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
    fn same_length_and_same_time_but_different_content_is_a_change() {
        let dir = scratch("samestamp");
        let file = dir.join("a.docx");
        fs::write(&file, b"abcdef").unwrap();
        let seen = Fingerprint::of(&file).unwrap();
        let stamp = fs::metadata(&file).unwrap().modified().unwrap();
        fs::write(&file, b"abcdeX").unwrap();
        fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(stamp)
            .unwrap();
        // Length and modification time alone cannot tell these apart.
        let now = fs::metadata(&file).unwrap();
        assert_eq!(now.len(), 6);
        assert_eq!(now.modified().unwrap(), stamp);
        assert_eq!(disk_state(&file, &seen), DiskState::Changed);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unchanged_content_with_the_same_stamp_is_unchanged() {
        let dir = scratch("same");
        let file = dir.join("a.docx");
        fs::write(&file, b"abcdef").unwrap();
        let seen = Fingerprint::of(&file).unwrap();
        assert_eq!(Fingerprint::of(&file).unwrap(), seen);
        assert_eq!(disk_state(&file, &seen), DiskState::Unchanged);
        let _ = fs::remove_dir_all(&dir);
    }

    /// A reader that hands out at most `step` bytes per call, like a slow disk or a pipe.
    struct Trickle<'a> {
        data: &'a [u8],
        step: usize,
    }

    impl Read for Trickle<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = self.step.min(self.data.len()).min(buf.len());
            buf[..n].copy_from_slice(&self.data[..n]);
            self.data = &self.data[n..];
            Ok(n)
        }
    }

    fn hash_of_slice(bytes: &[u8]) -> u64 {
        let mut hasher = DefaultHasher::new();
        hasher.write(bytes);
        hasher.finish()
    }

    #[test]
    fn the_streamed_hash_equals_the_hash_of_the_whole_slice_at_every_buffer_edge() {
        let sizes = [
            0,
            1,
            7,
            8,
            9,
            HASH_BUFFER_BYTES - 1,
            HASH_BUFFER_BYTES,
            HASH_BUFFER_BYTES + 1,
            2 * HASH_BUFFER_BYTES,
            2 * HASH_BUFFER_BYTES + 3,
        ];
        for size in sizes {
            let data: Vec<u8> = (0..size).map(|i| (i * 31 + 7) as u8).collect();
            let whole = hash_of_slice(&data);
            assert_eq!(hash_reader(&data[..]).unwrap(), whole, "size {size}");
            // Short reads must not change the result.
            for step in [1, 5, 4096] {
                let trickle = Trickle { data: &data, step };
                assert_eq!(
                    hash_reader(trickle).unwrap(),
                    whole,
                    "size {size} step {step}"
                );
            }
        }
    }

    #[test]
    fn one_changed_byte_anywhere_changes_the_hash() {
        let size = 2 * HASH_BUFFER_BYTES + 5;
        let base: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
        let original = hash_of_slice(&base);
        for at in [0, 1, HASH_BUFFER_BYTES - 1, HASH_BUFFER_BYTES, size - 1] {
            let mut edited = base.clone();
            edited[at] ^= 0x01;
            assert_ne!(hash_of_slice(&edited), original, "byte {at}");
        }
        assert_ne!(hash_of_slice(&base[..size - 1]), original);
    }

    #[test]
    fn an_empty_file_has_a_stable_fingerprint_and_the_file_and_byte_paths_agree() {
        let dir = scratch("empty");
        for (name, bytes) in [
            ("empty", Vec::new()),
            ("edge", vec![9_u8; HASH_BUFFER_BYTES]),
            ("over", vec![9_u8; HASH_BUFFER_BYTES + 1]),
        ] {
            let file = dir.join(name);
            fs::write(&file, &bytes).unwrap();
            let meta = fs::metadata(&file).unwrap();
            assert_eq!(
                Fingerprint::of(&file).unwrap(),
                Fingerprint::from_metadata_and_bytes(&meta, &bytes),
                "{name}"
            );
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_path_matches_the_format_it_is_saved_as_by_its_extension() {
        let has = |name: &str, ext: &str| path_has_extension(Path::new(name), ext);
        assert!(has("/a/보고서.docx", "docx"));
        assert!(has("/a/보고서.DOCX", "docx"));
        assert!(has("/a/notes.markdown", "md"));
        assert!(has("/a/notes.md", "MARKDOWN"));
        assert!(!has("/a/보고서.pdf", "docx"), "another format");
        assert!(!has("/a/보고서", "docx"), "no extension");
        assert!(
            !has("/a/보고서.docx.txt", "docx"),
            "the last extension counts"
        );
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
        let md_file = TrackedFile {
            path: dir.join("notes.markdown"),
            fingerprint: fp,
            from_open: true,
        };
        assert!(md_file.extension_matches("md"));
        assert!(md_file.extension_matches("MARKDOWN"));
        let _ = fs::remove_dir_all(&dir);
    }
}

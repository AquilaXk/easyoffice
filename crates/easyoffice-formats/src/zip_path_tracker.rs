//! Duplicate and file / directory collision detection for ZIP entry paths.

use crate::{EasyOfficeError, ZipEntryDescriptor};

fn zip_conflict(message: &str, path: &str) -> EasyOfficeError {
    EasyOfficeError::SecurityViolation(format!("{message}: {path}"))
}

/// Tracks lower-cased entry paths to reject duplicates and file / directory collisions.
pub(crate) struct ZipPathTracker {
    seen_paths: std::collections::HashSet<String>,
    seen_files: std::collections::HashSet<String>,
    seen_dirs: std::collections::HashSet<String>,
}

impl ZipPathTracker {
    pub(crate) fn with_capacity(n: usize) -> Self {
        Self {
            seen_paths: std::collections::HashSet::with_capacity(n),
            seen_files: std::collections::HashSet::with_capacity(n),
            seen_dirs: std::collections::HashSet::with_capacity(n),
        }
    }

    pub(crate) fn check(&mut self, entry: &ZipEntryDescriptor) -> Result<(), EasyOfficeError> {
        let is_dir = entry.path.ends_with('/');
        let lower = entry.path.to_ascii_lowercase();
        let trimmed_lower = lower.strip_suffix('/').unwrap_or(&lower).to_string();

        if !self.seen_paths.insert(trimmed_lower.clone()) {
            return Err(zip_conflict(
                "중복되거나 파일/디렉터리가 충돌하는 ZIP 엔트리 경로가 감지되었습니다",
                &entry.path,
            ));
        }
        for (idx, _) in trimmed_lower.match_indices('/') {
            let prefix = &trimmed_lower[..idx];
            if self.seen_files.contains(prefix) {
                return Err(zip_conflict(
                    "ZIP 파일 경로와 디렉터리 프리픽스 충돌이 감지되었습니다",
                    &entry.path,
                ));
            }
            self.seen_dirs.insert(prefix.to_string());
        }
        if is_dir {
            if self.seen_files.contains(&trimmed_lower) {
                return Err(zip_conflict(
                    "ZIP 디렉터리와 기존 파일 경로 충돌",
                    &entry.path,
                ));
            }
            self.seen_dirs.insert(trimmed_lower);
            if entry.compressed_size > 0 || entry.uncompressed_size > 0 {
                return Err(EasyOfficeError::SecurityViolation(format!(
                    "ZIP 디렉터리 엔트리({})는 데이터를 포함할 수 없습니다.",
                    entry.path
                )));
            }
        } else {
            if self.seen_dirs.contains(&trimmed_lower) {
                return Err(zip_conflict(
                    "ZIP 파일과 기존 디렉터리 경로 충돌",
                    &entry.path,
                ));
            }
            self.seen_files.insert(trimmed_lower);
        }
        Ok(())
    }
}

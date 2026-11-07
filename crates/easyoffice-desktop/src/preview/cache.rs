//! Cache of found pictures, one entry per recent id, valid while the file's time and size match.

use super::Thumbnail;
use std::fs::Metadata;
use std::sync::Arc;
use std::time::SystemTime;

/// Most entries kept: the recent list never holds more.
pub const MAX_CACHED: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStamp {
    modified: Option<SystemTime>,
    len: u64,
}

impl FileStamp {
    pub fn of(meta: &Metadata) -> Self {
        Self {
            modified: meta.modified().ok(),
            len: meta.len(),
        }
    }

    #[cfg(test)]
    pub fn new(modified: Option<SystemTime>, len: u64) -> Self {
        Self { modified, len }
    }
}

struct Entry {
    id: String,
    stamp: FileStamp,
    found: Option<Arc<Thumbnail>>,
}

/// Oldest entry first; a miss is remembered too, so a file without a picture is read once.
#[derive(Default)]
pub struct PreviewCache {
    entries: Vec<Entry>,
}

impl PreviewCache {
    /// `Some(found)` when the id is cached for this stamp (`found` may be `None`: no picture).
    pub fn get(&self, id: &str, stamp: &FileStamp) -> Option<Option<Arc<Thumbnail>>> {
        self.entries
            .iter()
            .find(|e| e.id == id && e.stamp == *stamp)
            .map(|e| e.found.clone())
    }

    pub fn put(&mut self, id: &str, stamp: FileStamp, found: Option<Arc<Thumbnail>>) {
        self.entries.retain(|e| e.id != id);
        if self.entries.len() >= MAX_CACHED {
            self.entries.remove(0);
        }
        self.entries.push(Entry {
            id: id.to_string(),
            stamp,
            found,
        });
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

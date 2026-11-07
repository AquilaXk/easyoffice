//! Recent documents of the desktop shell (PD-088). The list is stored as plain paths in the app's
//! data directory and stays in Rust: the WebView only receives an opaque id, the display name and
//! the document kind, and asks to open an entry by that id.

use crate::windows::{display_name, path_key, WindowKind};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

pub const MAX_RECENT: usize = 10;
const FILE_NAME: &str = "recent-documents.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentEntry {
    pub path: PathBuf,
}

/// What the page sees of an entry.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct RecentView {
    pub id: String,
    pub name: String,
    pub kind: &'static str,
}

/// Opaque, stable id of a path (FNV-1a over the comparison key); it cannot be turned back into a path.
pub fn recent_id(path: &Path) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in path_key(path).bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[derive(Debug, Default)]
pub struct RecentList {
    entries: Vec<RecentEntry>,
}

impl RecentList {
    /// Moves (or inserts) a document to the front; the list never exceeds `MAX_RECENT`.
    pub fn add(&mut self, path: PathBuf) {
        let key = path_key(&path);
        self.entries.retain(|e| path_key(&e.path) != key);
        self.entries.insert(0, RecentEntry { path });
        self.entries.truncate(MAX_RECENT);
    }

    pub fn path_for(&self, id: &str) -> Option<PathBuf> {
        self.entries
            .iter()
            .find(|e| recent_id(&e.path) == id)
            .map(|e| e.path.clone())
    }

    /// Entries whose file still exists and has a supported kind, newest first.
    pub fn views(&self) -> Vec<RecentView> {
        self.entries
            .iter()
            .filter(|e| e.path.is_file())
            .filter_map(|e| {
                let kind = WindowKind::for_path(&e.path)?;
                Some(RecentView {
                    id: recent_id(&e.path),
                    name: display_name(&e.path),
                    kind: kind.param(),
                })
            })
            .collect()
    }

    pub fn load(dir: &Path) -> Self {
        let Ok(text) = fs::read_to_string(dir.join(FILE_NAME)) else {
            return Self::default();
        };
        let Ok(paths) = serde_json::from_str::<Vec<String>>(&text) else {
            return Self::default();
        };
        let mut list = Self::default();
        for raw in paths.into_iter().rev() {
            list.add(PathBuf::from(raw));
        }
        list
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        fs::create_dir_all(dir)?;
        let paths: Vec<String> = self
            .entries
            .iter()
            .map(|e| e.path.to_string_lossy().into_owned())
            .collect();
        let json = serde_json::to_string(&paths).map_err(std::io::Error::other)?;
        fs::write(dir.join(FILE_NAME), json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("easyoffice-recent-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn ids_are_stable_opaque_and_path_free() {
        let a = recent_id(Path::new("C:/문서/a.docx"));
        assert_eq!(a, recent_id(Path::new("C:/문서/a.docx")));
        assert_ne!(a, recent_id(Path::new("C:/문서/b.docx")));
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn list_is_newest_first_deduplicated_and_capped() {
        let mut list = RecentList::default();
        for n in 0..15 {
            list.add(PathBuf::from(format!("C:/d/{n}.docx")));
        }
        assert_eq!(list.entries.len(), MAX_RECENT);
        assert_eq!(list.entries[0].path, PathBuf::from("C:/d/14.docx"));
        list.add(PathBuf::from("C:/d/10.docx"));
        assert_eq!(list.entries[0].path, PathBuf::from("C:/d/10.docx"));
        assert_eq!(list.entries.len(), MAX_RECENT);
        assert_eq!(
            list.entries
                .iter()
                .filter(|e| e.path.ends_with("10.docx"))
                .count(),
            1
        );
    }

    #[test]
    fn views_expose_id_name_and_kind_only_for_existing_supported_files() {
        let dir = scratch("views");
        let doc = dir.join("개요.hwpx");
        let sheet = dir.join("예산.xlsx");
        fs::write(&doc, b"x").unwrap();
        fs::write(&sheet, b"x").unwrap();
        let mut list = RecentList::default();
        list.add(doc.clone());
        list.add(dir.join("사라진.docx"));
        list.add(dir.join("메모.exe"));
        list.add(sheet.clone());
        let views = list.views();
        let json = serde_json::to_string(&views).unwrap();
        assert_eq!(
            views.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(),
            ["예산.xlsx", "개요.hwpx"]
        );
        assert_eq!(views[0].kind, "SHEET");
        assert_eq!(views[1].kind, "HWP");
        assert!(
            !json.contains("easyoffice-recent"),
            "directories never leave Rust: {json}"
        );
        assert_eq!(list.path_for(&views[1].id), Some(doc));
        assert_eq!(list.path_for("0000000000000000"), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_round_trips_through_the_data_directory() {
        let dir = scratch("roundtrip");
        let mut list = RecentList::default();
        list.add(PathBuf::from("C:/d/one.docx"));
        list.add(PathBuf::from("C:/d/two.pdf"));
        list.save(&dir).unwrap();
        let loaded = RecentList::load(&dir);
        assert_eq!(loaded.entries, list.entries);
        fs::write(dir.join(FILE_NAME), "not json").unwrap();
        assert!(
            RecentList::load(&dir).entries.is_empty(),
            "a damaged file starts an empty list"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

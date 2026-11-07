//! Window model of the desktop shell (PD-088): one window per document. This module holds the
//! pure state and rules (kinds, launch URLs, titles, the path registry, the pending-open queue and
//! the close decision); `window_commands.rs` wires them to Tauri. File system paths live only here
//! and in `recent.rs`: nothing in this module is ever serialized to the WebView with a path in it.

use std::collections::HashMap;
use std::path::Path;

/// What a window hosts. `Start` is a regular-size window showing the start screen (PD-098); the
/// others are document windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowKind {
    Start,
    Doc,
    Hwp,
    Sheet,
    Slide,
    Pdf,
    Conv,
}

impl WindowKind {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "START" => Some(Self::Start),
            "DOC" => Some(Self::Doc),
            "HWP" => Some(Self::Hwp),
            "SHEET" => Some(Self::Sheet),
            "SLIDE" => Some(Self::Slide),
            "PDF" => Some(Self::Pdf),
            "CONV" => Some(Self::Conv),
            _ => None,
        }
    }

    /// The `module` launch parameter the WebUI reads.
    pub fn param(self) -> &'static str {
        match self {
            Self::Start => "START",
            Self::Doc => "DOC",
            Self::Hwp => "HWP",
            Self::Sheet => "SHEET",
            Self::Slide => "SLIDE",
            Self::Pdf => "PDF",
            Self::Conv => "CONV",
        }
    }

    /// Document kind a file opens in, by extension (the same families the WebUI file open accepts).
    pub fn for_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "docx" | "doc" | "odt" | "rtf" | "md" | "markdown" | "txt" => Some(Self::Doc),
            "hwp" | "hwpx" | "hwt" | "hwtx" => Some(Self::Hwp),
            "xlsx" | "xls" | "ods" | "csv" | "tsv" => Some(Self::Sheet),
            "pptx" | "ppt" | "ppsx" | "odp" => Some(Self::Slide),
            "pdf" => Some(Self::Pdf),
            _ => None,
        }
    }

    pub fn for_path(path: &Path) -> Option<Self> {
        Self::for_extension(path.extension()?.to_str()?)
    }

    /// Inner size in logical pixels: every window, the start window included, is a workspace.
    pub fn inner_size(self) -> (f64, f64) {
        (1280.0, 860.0)
    }

    pub fn min_inner_size(self) -> (f64, f64) {
        (1024.0, 680.0)
    }
}

/// Label of the first window (the start window, or the document window in `--smoke`).
pub const PRIMARY_LABEL: &str = "main";
/// Dynamic windows are labelled `doc-<counter>-<hex millis>`; the capability file grants `doc-*`.
pub const LABEL_PREFIX: &str = "doc-";

pub fn new_label(counter: u64, millis: u128) -> String {
    format!("{LABEL_PREFIX}{counter}-{millis:x}")
}

pub fn is_window_label(label: &str) -> bool {
    label == PRIMARY_LABEL
        || (label.starts_with(LABEL_PREFIX)
            && label.len() <= 64
            && label
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
}

/// Launch URL inside the embedded app origin. `win` doubles as the key of the page's own
/// recovery records, so it is the window label and nothing derived from a path.
pub fn window_url(kind: WindowKind, label: &str) -> String {
    format!("index.html?module={}&win={}", kind.param(), label)
}

/// "파일 이름 — EasyOffice", with a leading mark while the document has unsaved changes.
pub fn window_title(name: &str, dirty: bool) -> String {
    let shown = name.trim();
    let shown = if shown.is_empty() {
        "제목 없음"
    } else {
        shown
    };
    format!("{}{shown} — EasyOffice", if dirty { "● " } else { "" })
}

/// File name for display (never a directory), trimmed to a sane length.
pub fn display_name(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.chars().take(200).collect()
}

/// Comparison key of a canonical path. Windows file names are case-insensitive.
pub fn path_key(path: &Path) -> String {
    let raw = path.to_string_lossy().into_owned();
    if cfg!(windows) {
        raw.to_lowercase()
    } else {
        raw
    }
}

#[derive(Debug)]
struct WindowEntry {
    kind: WindowKind,
    name: String,
    dirty: bool,
    path_key: Option<String>,
    close_prompted: bool,
}

/// What to do when the user asks to close a window.
#[derive(Debug, PartialEq, Eq)]
pub enum CloseAction {
    /// Nothing unsaved (or the page did not answer the earlier request): close now.
    CloseNow,
    /// Ask the page to run its 저장 / 저장 안 함 / 취소 flow.
    AskPage,
}

/// Open windows, their reported state and the canonical path each document came from.
#[derive(Debug, Default)]
pub struct WindowRegistry {
    windows: HashMap<String, WindowEntry>,
    by_path: HashMap<String, String>,
}

impl WindowRegistry {
    pub fn register(&mut self, label: &str, kind: WindowKind) {
        self.windows.insert(
            label.to_string(),
            WindowEntry {
                kind,
                name: String::new(),
                dirty: false,
                path_key: None,
                close_prompted: false,
            },
        );
    }

    /// The window already showing the document at `key`, if any.
    pub fn find_by_path(&self, key: &str) -> Option<&str> {
        self.by_path.get(key).map(String::as_str)
    }

    /// Records that `label` shows the document at `key`. Fails with the label that already does.
    pub fn claim_path(&mut self, label: &str, key: String) -> Result<(), String> {
        if let Some(owner) = self.by_path.get(&key) {
            if owner != label {
                return Err(owner.clone());
            }
        }
        let Some(entry) = self.windows.get_mut(label) else {
            return Err(String::new());
        };
        if let Some(old) = entry.path_key.replace(key.clone()) {
            self.by_path.remove(&old);
        }
        self.by_path.insert(key, label.to_string());
        Ok(())
    }

    /// The page reports its document name and unsaved state; a report ends any pending close prompt.
    pub fn set_state(&mut self, label: &str, name: &str, dirty: bool) -> Option<String> {
        let entry = self.windows.get_mut(label)?;
        entry.name = name.chars().take(200).collect();
        entry.dirty = dirty;
        entry.close_prompted = false;
        Some(window_title(&entry.name, dirty))
    }

    #[cfg(test)]
    pub fn is_dirty(&self, label: &str) -> bool {
        self.windows.get(label).is_some_and(|w| w.dirty)
    }

    #[cfg(test)]
    pub fn kind(&self, label: &str) -> Option<WindowKind> {
        self.windows.get(label).map(|w| w.kind)
    }

    /// A second request while the page is still asked (it never answered) closes the window, so a
    /// hung page can never make a window unclosable.
    pub fn begin_close(&mut self, label: &str) -> CloseAction {
        let Some(entry) = self.windows.get_mut(label) else {
            return CloseAction::CloseNow;
        };
        if !entry.dirty || entry.close_prompted {
            return CloseAction::CloseNow;
        }
        entry.close_prompted = true;
        CloseAction::AskPage
    }

    pub fn remove(&mut self, label: &str) -> bool {
        let Some(entry) = self.windows.remove(label) else {
            return false;
        };
        if let Some(key) = entry.path_key {
            self.by_path.remove(&key);
        }
        true
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.windows.len()
    }

    pub fn document_window_count(&self) -> usize {
        self.windows
            .values()
            .filter(|w| w.kind != WindowKind::Start)
            .count()
    }
}

/// Bytes and display name queued for a window that is about to load its document. The path is not
/// part of this type: it is consumed in Rust (registry, recents) and never reaches the page.
#[derive(Debug, PartialEq, Eq)]
pub struct PendingOpen {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Queue keyed by window label; a window takes its entry exactly once.
#[derive(Debug, Default)]
pub struct PendingOpenQueue {
    by_label: HashMap<String, PendingOpen>,
}

impl PendingOpenQueue {
    pub fn push(&mut self, label: &str, open: PendingOpen) {
        self.by_label.insert(label.to_string(), open);
    }

    pub fn take(&mut self, label: &str) -> Option<PendingOpen> {
        self.by_label.remove(label)
    }

    pub fn discard(&mut self, label: &str) {
        self.by_label.remove(label);
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.by_label.len()
    }
}

/// The shape the page receives for a pending open: a display name and the bytes. Nothing else.
#[derive(Debug, serde::Serialize, PartialEq, Eq)]
pub struct PendingOpenView {
    pub name: String,
    pub base64: String,
}

impl PendingOpenView {
    pub fn from_pending(open: &PendingOpen) -> Self {
        Self {
            name: open.name.clone(),
            base64: easyoffice_app::EasyOfficeIpcRouter::encode_base64(&open.bytes),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn kinds_parse_and_map_extensions() {
        assert_eq!(WindowKind::parse("hwp"), Some(WindowKind::Hwp));
        assert_eq!(WindowKind::parse(" START "), Some(WindowKind::Start));
        // PD-098: the start window is a regular workspace window, not a compact one.
        for kind in [
            WindowKind::Start,
            WindowKind::Doc,
            WindowKind::Hwp,
            WindowKind::Conv,
        ] {
            assert_eq!(kind.inner_size(), (1280.0, 860.0));
            assert_eq!(kind.min_inner_size(), (1024.0, 680.0));
        }
        assert_eq!(WindowKind::parse("evil"), None);
        assert_eq!(WindowKind::for_extension("DOCX"), Some(WindowKind::Doc));
        assert_eq!(WindowKind::for_extension("hwpx"), Some(WindowKind::Hwp));
        assert_eq!(WindowKind::for_extension("csv"), Some(WindowKind::Sheet));
        assert_eq!(WindowKind::for_extension("ppsx"), Some(WindowKind::Slide));
        assert_eq!(WindowKind::for_extension("pdf"), Some(WindowKind::Pdf));
        assert_eq!(WindowKind::for_extension("exe"), None);
        assert_eq!(
            WindowKind::for_path(&PathBuf::from("C:/문서/보고서.HWP")),
            Some(WindowKind::Hwp)
        );
        assert_eq!(WindowKind::for_path(&PathBuf::from("no-extension")), None);
    }

    #[test]
    fn launch_urls_carry_only_kind_and_label() {
        let label = new_label(3, 0xabc);
        assert_eq!(label, "doc-3-abc");
        assert!(is_window_label(&label));
        assert!(is_window_label("main"));
        assert!(!is_window_label("doc-../x"));
        assert!(!is_window_label("other"));
        assert_eq!(
            window_url(WindowKind::Sheet, &label),
            "index.html?module=SHEET&win=doc-3-abc"
        );
    }

    #[test]
    fn titles_show_the_file_name_and_the_unsaved_mark() {
        assert_eq!(
            window_title("보고서.docx", false),
            "보고서.docx — EasyOffice"
        );
        assert_eq!(
            window_title("보고서.docx", true),
            "● 보고서.docx — EasyOffice"
        );
        assert_eq!(window_title("  ", false), "제목 없음 — EasyOffice");
    }

    #[test]
    fn display_name_is_a_file_name_never_a_directory() {
        assert_eq!(
            display_name(&PathBuf::from("C:/비밀/폴더/예산.xlsx")),
            "예산.xlsx"
        );
        assert_eq!(display_name(&PathBuf::from("/")), "");
    }

    #[test]
    fn reopening_a_file_focuses_the_window_that_has_it() {
        let mut reg = WindowRegistry::default();
        reg.register("doc-1-a", WindowKind::Doc);
        reg.register("doc-2-b", WindowKind::Doc);
        let key = path_key(&PathBuf::from("C:/문서/보고서.docx"));
        assert_eq!(reg.find_by_path(&key), None);
        reg.claim_path("doc-1-a", key.clone()).unwrap();
        assert_eq!(reg.find_by_path(&key), Some("doc-1-a"));
        assert_eq!(
            reg.claim_path("doc-2-b", key.clone()),
            Err("doc-1-a".to_string())
        );
        assert!(
            reg.claim_path("doc-1-a", key.clone()).is_ok(),
            "same window may claim again"
        );
        assert!(reg.remove("doc-1-a"));
        assert_eq!(reg.find_by_path(&key), None, "closing frees the path");
        assert!(reg.claim_path("doc-2-b", key).is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn path_keys_ignore_case_on_windows() {
        assert_eq!(
            path_key(&PathBuf::from("C:/Docs/A.DOCX")),
            path_key(&PathBuf::from("c:/docs/a.docx"))
        );
    }

    #[test]
    fn saving_under_a_new_name_moves_the_path_claim() {
        let mut reg = WindowRegistry::default();
        reg.register("doc-1-a", WindowKind::Doc);
        reg.claim_path("doc-1-a", "old".into()).unwrap();
        reg.claim_path("doc-1-a", "new".into()).unwrap();
        assert_eq!(reg.find_by_path("old"), None);
        assert_eq!(reg.find_by_path("new"), Some("doc-1-a"));
    }

    #[test]
    fn state_reports_set_the_title_and_drive_the_close_decision() {
        let mut reg = WindowRegistry::default();
        reg.register("doc-1-a", WindowKind::Doc);
        assert_eq!(
            reg.begin_close("doc-1-a"),
            CloseAction::CloseNow,
            "clean windows close"
        );
        assert_eq!(
            reg.set_state("doc-1-a", "계획.hwpx", true).as_deref(),
            Some("● 계획.hwpx — EasyOffice")
        );
        assert!(reg.is_dirty("doc-1-a"));
        assert_eq!(reg.begin_close("doc-1-a"), CloseAction::AskPage);
        assert_eq!(
            reg.begin_close("doc-1-a"),
            CloseAction::CloseNow,
            "a page that never answers cannot trap the window"
        );
        reg.set_state("doc-1-a", "계획.hwpx", true);
        assert_eq!(
            reg.begin_close("doc-1-a"),
            CloseAction::AskPage,
            "a new report re-arms the prompt"
        );
        reg.set_state("doc-1-a", "계획.hwpx", false);
        assert_eq!(reg.begin_close("doc-1-a"), CloseAction::CloseNow);
        assert_eq!(reg.set_state("missing", "x", false), None);
        assert_eq!(reg.begin_close("missing"), CloseAction::CloseNow);
    }

    #[test]
    fn registry_counts_document_windows_separately_from_the_start_window() {
        let mut reg = WindowRegistry::default();
        reg.register(PRIMARY_LABEL, WindowKind::Start);
        assert_eq!((reg.len(), reg.document_window_count()), (1, 0));
        reg.register("doc-1-a", WindowKind::Pdf);
        assert_eq!((reg.len(), reg.document_window_count()), (2, 1));
        assert_eq!(reg.kind("doc-1-a"), Some(WindowKind::Pdf));
    }

    #[test]
    fn pending_open_is_keyed_by_window_and_taken_once() {
        let mut queue = PendingOpenQueue::default();
        queue.push(
            "doc-1-a",
            PendingOpen {
                name: "a.docx".into(),
                bytes: vec![1, 2, 3],
            },
        );
        queue.push(
            "doc-2-b",
            PendingOpen {
                name: "b.hwp".into(),
                bytes: vec![9],
            },
        );
        assert_eq!(queue.take("doc-3-c"), None, "other windows get nothing");
        let first = queue.take("doc-1-a").unwrap();
        assert_eq!(
            (first.name.as_str(), first.bytes.as_slice()),
            ("a.docx", &[1u8, 2, 3][..])
        );
        assert_eq!(
            queue.take("doc-1-a"),
            None,
            "an entry is taken exactly once"
        );
        queue.discard("doc-2-b");
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn the_page_only_ever_receives_a_name_and_bytes() {
        let secret_dir = "C:/비밀 폴더/하위";
        let path = PathBuf::from(format!("{secret_dir}/계약서.pdf"));
        let open = PendingOpen {
            name: display_name(&path),
            bytes: b"%PDF-1.7".to_vec(),
        };
        let view = PendingOpenView::from_pending(&open);
        let json = serde_json::to_string(&view).unwrap();
        assert_eq!(json, r#"{"name":"계약서.pdf","base64":"JVBERi0xLjc="}"#);
        assert!(
            !json.contains("비밀") && !json.contains("C:"),
            "no directory or drive leaks: {json}"
        );
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let mut keys: Vec<_> = value.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["base64", "name"]);
    }
}

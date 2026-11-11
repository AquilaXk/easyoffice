//! The fonts installed on the machine, for the Hangul editor (PD-128).
//!
//! The editor draws a document in the fonts it names (Batang, Malgun Gothic, ...) when the
//! machine has them. The reference app does this with two commands, `list_local_fonts` and
//! `read_local_font` (`apps/desktop/src-tauri/src/font_catalog.rs`, `src-tauri/src/commands.rs`
//! 339-349; MIT, Copyright (c) 2025-2026 Edward Kim, see THIRD_PARTY_NOTICES.md): the list holds
//! family, style and weight, the read takes a path and hands back the file. Two things differ here.
//!
//! * The list comes from the font database the PDF export already builds once per process
//!   (`pdf_fonts::shared_font_db`, fontdb; it scans the same system and per-user folders the
//!   reference app adds by hand). Nothing is scanned a second time.
//! * The page never sees a path (PD-088 3). Every font file has a number that is valid only
//!   together with the `revision` of the list the window got, and a new list replaces the old
//!   revision. A number from another window, from an older list or out of range is refused.
//!
//! The reference app puts no bound on the file it reads. Here a read is bounded four ways, and any
//! of them refuses (fail closed): a file larger than [`MAX_FONT_BYTES`], a file that does not
//! start with a TrueType, OpenType or collection signature, a window that has been served more than
//! [`MAX_SESSION_BYTES`] of distinct fonts, and a list longer than [`MAX_FACES`] (the longest
//! catalog the editor accepts). The first two equal what the editor does with the bytes
//! (`host-font-provider.ts`: 64 MiB, one face of an SFNT file).

use crate::shell_caller::ShellCaller;
use crate::{pdf_export, IpcError};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use tauri::{AppHandle, State, WebviewWindow};
use usvg::fontdb::{Database, Language, Source, Stretch, Style};

/// Largest font file a read hands out (the editor drops anything above this).
pub const MAX_FONT_BYTES: u64 = 64 * 1024 * 1024;
/// Most faces in the list (the editor refuses a longer catalog).
pub const MAX_FACES: usize = 25_000;
/// Most distinct font bytes one window may be served over its life (a new list does not renew it). A document names a handful of
/// fonts; this leaves room for a few large CJK collections and stops a loop that asks for all.
pub const MAX_SESSION_BYTES: u64 = 512 * 1024 * 1024;
/// Longest family or style text the editor accepts.
const MAX_NAME_CHARS: usize = 1024;
/// Most alias names kept for a face (the editor accepts 32).
const MAX_ALIASES: usize = 32;
/// Bytes of a random list revision.
const REVISION_BYTES: usize = 8;

const MSG_NO_LIST: &str = "글꼴 목록이 없거나 오래되어 읽기를 거부했습니다.";
const MSG_UNREADABLE: &str = "시스템 글꼴을 읽을 수 없습니다.";
const MSG_TOO_LARGE: &str = "글꼴 파일이 허용 크기를 넘어 읽기를 거부했습니다.";
const MSG_NOT_A_FONT: &str = "글꼴 형식이 아니어서 읽기를 거부했습니다.";
const MSG_BUDGET: &str = "이 창에 보낼 수 있는 글꼴 용량을 넘어 읽기를 거부했습니다.";

/// One face as the editor's host-font contract describes it, plus where its bytes are.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FontFaceInfo {
    /// Number of the font file in the catalog; several faces of a collection share one.
    pub file: u32,
    /// Face inside the file (0 unless the file is a collection).
    pub face_index: u32,
    pub family: String,
    pub full_name: String,
    pub postscript_name: String,
    pub style: String,
    pub weight: u16,
    pub slant: &'static str,
    /// The other names of the family, e.g. the localized one.
    pub aliases: Vec<String>,
}

/// What `list_system_fonts` answers: names and numbers only.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SystemFontList {
    /// Random per list; a read must quote it.
    pub revision: String,
    pub faces: Vec<FontFaceInfo>,
}

/// The installed fonts: faces, and the files they live in (never leaves this module).
pub struct Catalog {
    files: Vec<PathBuf>,
    faces: Vec<FontFaceInfo>,
}

impl Catalog {
    /// Lists the faces of `db` that come from a font file outside `skip_dirs` (the bundled font the
    /// editor already ships). A name the editor would refuse, a hidden system face (a name that
    /// starts with a dot), a non-normal width and a second face with the same family, weight and
    /// slant are left out: a document names a family, and two candidates would leave it unresolved.
    pub fn from_database(db: &Database, skip_dirs: &[PathBuf]) -> Self {
        let mut files: Vec<PathBuf> = Vec::new();
        let mut file_numbers: HashMap<&Path, u32> = HashMap::new();
        let mut faces: Vec<FontFaceInfo> = Vec::new();
        let mut seen: HashSet<(String, u16, &'static str)> = HashSet::new();
        for face in db.faces() {
            if faces.len() >= MAX_FACES {
                break;
            }
            let (Source::File(path) | Source::SharedFile(path, _)) = &face.source else {
                continue;
            };
            if face.stretch != Stretch::Normal
                || skip_dirs.iter().any(|dir| path.starts_with(dir))
                || !has_font_extension(path)
            {
                continue;
            }
            let names = family_names(&face.families);
            let Some(family) = names.first().filter(|name| !name.starts_with('.')) else {
                continue;
            };
            let weight = face.weight.0.clamp(1, 1000);
            let slant = slant_name(face.style);
            if !seen.insert((family.to_lowercase(), weight, slant)) {
                continue;
            }
            let file = *file_numbers.entry(path.as_path()).or_insert_with(|| {
                files.push(path.clone());
                u32::try_from(files.len() - 1).unwrap_or(u32::MAX)
            });
            let style = style_name(weight, slant);
            let full_name = if style == "Regular" {
                family.clone()
            } else {
                format!("{family} {style}")
            };
            faces.push(FontFaceInfo {
                file,
                face_index: face.index,
                family: family.clone(),
                full_name,
                postscript_name: bounded(&face.post_script_name),
                style,
                weight,
                slant,
                aliases: names.iter().skip(1).take(MAX_ALIASES).cloned().collect(),
            });
        }
        Self { files, faces }
    }
}

fn has_font_extension(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };
    let [b0, b1, b2] = *ext.as_bytes() else {
        return false;
    };
    matches!(
        &[b0 | 0x20, b1 | 0x20, b2 | 0x20],
        b"ttf" | b"otf" | b"ttc" | b"otc"
    )
}

/// The family names of a face, English (United States) first, trimmed, without empty or oversized
/// ones and without repeats.
fn family_names(families: &[(String, Language)]) -> Vec<String> {
    let english = families
        .iter()
        .filter(|(_, language)| *language == Language::English_UnitedStates);
    let others = families
        .iter()
        .filter(|(_, language)| *language != Language::English_UnitedStates);
    let mut names: Vec<String> = Vec::new();
    for (name, _) in english.chain(others) {
        let name = bounded(name);
        if !name.is_empty() && !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

fn bounded(text: &str) -> String {
    let text = text.trim();
    if text.len() <= MAX_NAME_CHARS || text.chars().count() <= MAX_NAME_CHARS {
        text.to_string()
    } else {
        String::new()
    }
}

fn slant_name(style: Style) -> &'static str {
    match style {
        Style::Normal => "normal",
        Style::Italic => "italic",
        Style::Oblique => "oblique",
    }
}

/// "Regular", "Bold", "Bold Italic" ...: the weight word, then the slant.
fn style_name(weight: u16, slant: &str) -> String {
    let word = match weight {
        100 => "Thin",
        200 => "ExtraLight",
        300 => "Light",
        400 => "Regular",
        500 => "Medium",
        600 => "SemiBold",
        700 => "Bold",
        800 => "ExtraBold",
        900 => "Black",
        other if slant == "normal" => return other.to_string(),
        other => return format!("{other} {slant}"),
    };
    match slant {
        "italic" if weight == 400 => "Italic".to_string(),
        "oblique" if weight == 400 => "Oblique".to_string(),
        "italic" => format!("{word} Italic"),
        "oblique" => format!("{word} Oblique"),
        _ => word.to_string(),
    }
}

/// What a window has been told and served.
struct Session {
    revision: String,
    /// Files whose bytes this window already received (not charged twice).
    served: HashSet<u32>,
    bytes: u64,
}

/// The catalog (built once for the process) and the list each window holds.
pub struct SystemFonts {
    catalog: OnceLock<Arc<Catalog>>,
    sessions: Mutex<HashMap<String, Session>>,
    /// Most distinct font bytes one window is served.
    budget: u64,
}

impl Default for SystemFonts {
    fn default() -> Self {
        Self::with_budget(MAX_SESSION_BYTES)
    }
}

impl SystemFonts {
    fn with_budget(budget: u64) -> Self {
        Self {
            catalog: OnceLock::new(),
            sessions: Mutex::new(HashMap::new()),
            budget,
        }
    }

    /// The window's new list. `build` runs only the first time (the font folders are scanned once
    /// per process); every call replaces the window's revision, so older numbers stop working. What
    /// the window was served stays counted (file numbers belong to the process's one catalog), so
    /// asking for a list again does not start a new budget.
    pub fn list(
        &self,
        label: &str,
        build: impl FnOnce() -> Catalog,
    ) -> Result<SystemFontList, String> {
        let catalog = self.catalog.get_or_init(|| Arc::new(build()));
        let revision = new_revision()?;
        self.lock()
            .entry(label.to_string())
            .or_insert_with(|| Session {
                revision: String::new(),
                served: HashSet::new(),
                bytes: 0,
            })
            .revision
            .clone_from(&revision);
        Ok(SystemFontList {
            revision,
            faces: catalog.faces.clone(),
        })
    }

    /// The bytes of font file `file` of the list `revision` that `label` holds.
    pub fn read(&self, label: &str, revision: &str, file: u32) -> Result<Vec<u8>, String> {
        let path = self.path_of(label, revision, file)?;
        // Checked before opening: opening a named pipe blocks until a program writes to it.
        if !std::fs::metadata(&path).is_ok_and(|meta| meta.is_file()) {
            return Err(MSG_UNREADABLE.to_string());
        }
        let mut handle = File::open(&path).map_err(|_| MSG_UNREADABLE.to_string())?;
        let meta = handle.metadata().map_err(|_| MSG_UNREADABLE.to_string())?;
        if !meta.is_file() {
            return Err(MSG_UNREADABLE.to_string());
        }
        let size = meta.len();
        if size == 0 || size > MAX_FONT_BYTES {
            return Err(MSG_TOO_LARGE.to_string());
        }
        let charged = self.charge(label, revision, file, size)?;
        let result = read_font(&mut handle);
        self.settle(label, revision, file, size, charged, result.is_ok());
        result
    }

    /// Drops the window's list (the window is gone).
    pub fn forget(&self, label: &str) {
        self.lock().remove(label);
    }

    fn path_of(&self, label: &str, revision: &str, file: u32) -> Result<PathBuf, String> {
        let current = self
            .lock()
            .get(label)
            .is_some_and(|session| session.revision == revision);
        let catalog = self.catalog.get().filter(|_| current);
        catalog
            .and_then(|catalog| catalog.files.get(file as usize))
            .cloned()
            .ok_or_else(|| MSG_NO_LIST.to_string())
    }

    /// Reserves `size` bytes of the window's budget unless the file was served already. Returns
    /// whether something was reserved.
    fn charge(&self, label: &str, revision: &str, file: u32, size: u64) -> Result<bool, String> {
        let mut sessions = self.lock();
        let session = sessions
            .get_mut(label)
            .filter(|session| session.revision == revision)
            .ok_or_else(|| MSG_NO_LIST.to_string())?;
        if session.served.contains(&file) {
            return Ok(false);
        }
        if session.bytes.saturating_add(size) > self.budget {
            return Err(MSG_BUDGET.to_string());
        }
        session.bytes += size;
        Ok(true)
    }

    /// Ends a read: a failed one gives its reservation back; a good one marks the file served (and
    /// gives back a reservation that a parallel read of the same file made too).
    fn settle(&self, label: &str, revision: &str, file: u32, size: u64, charged: bool, ok: bool) {
        let mut sessions = self.lock();
        let Some(session) = sessions
            .get_mut(label)
            .filter(|session| session.revision == revision)
        else {
            return;
        };
        let first = ok && session.served.insert(file);
        if charged && !first {
            session.bytes = session.bytes.saturating_sub(size);
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Session>> {
        self.sessions.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn is_sfnt(head: &[u8; 4]) -> bool {
    matches!(
        u32::from_be_bytes(*head),
        0x0001_0000 | 0x7472_7565 | 0x4F54_544F | 0x7474_6366
    )
}

/// Reads at most [`MAX_FONT_BYTES`] and accepts only a font signature. Reading one byte more than
/// the bound catches a file that grew after it was measured.
fn read_font(source: &mut impl Read) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    source
        .take(MAX_FONT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| MSG_UNREADABLE.to_string())?;
    if bytes.len() as u64 > MAX_FONT_BYTES {
        return Err(MSG_TOO_LARGE.to_string());
    }
    match bytes.first_chunk::<4>() {
        Some(head) if is_sfnt(head) => Ok(bytes),
        _ => Err(MSG_NOT_A_FONT.to_string()),
    }
}

fn new_revision() -> Result<String, String> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut bytes = [0u8; REVISION_BYTES];
    getrandom::fill(&mut bytes).map_err(|_| MSG_UNREADABLE.to_string())?;
    let mut hex = [0u8; REVISION_BYTES * 2];
    for (i, &byte) in bytes.iter().enumerate() {
        hex[i * 2] = HEX[usize::from(byte >> 4)];
        hex[i * 2 + 1] = HEX[usize::from(byte & 0x0f)];
    }
    Ok(std::str::from_utf8(&hex).unwrap_or_default().to_string())
}

fn worker_failed(err: tauri::Error) -> IpcError {
    IpcError::new(format!("글꼴을 불러오지 못했습니다: {err}"))
}

/// The window's font list: names, styles and numbers, no paths. The first call scans the font
/// folders on a worker (the PDF export's database), later ones reuse the catalog.
#[tauri::command]
pub async fn list_system_fonts(
    _caller: ShellCaller,
    app: AppHandle,
    window: WebviewWindow,
    fonts: State<'_, Arc<SystemFonts>>,
) -> Result<SystemFontList, IpcError> {
    let fonts = Arc::clone(&fonts);
    let label = window.label().to_string();
    let dirs = crate::pdf_job::pdf_font_dirs(&app);
    tauri::async_runtime::spawn_blocking(move || {
        fonts.list(&label, || {
            Catalog::from_database(&pdf_export::shared_font_db(&dirs), &dirs)
        })
    })
    .await
    .map_err(worker_failed)?
    .map_err(IpcError::new)
}

/// The bytes of one font file of the window's list (binary, not a number array).
#[tauri::command]
pub async fn read_system_font(
    _caller: ShellCaller,
    window: WebviewWindow,
    fonts: State<'_, Arc<SystemFonts>>,
    revision: String,
    file: u32,
) -> Result<tauri::ipc::Response, IpcError> {
    let fonts = Arc::clone(&fonts);
    let label = window.label().to_string();
    tauri::async_runtime::spawn_blocking(move || fonts.read(&label, &revision, file))
        .await
        .map_err(worker_failed)?
        .map(tauri::ipc::Response::new)
        .map_err(IpcError::new)
}

#[cfg(test)]
#[path = "font_catalog_tests.rs"]
mod tests;

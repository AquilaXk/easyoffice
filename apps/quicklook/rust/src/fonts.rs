//! The font database of one extension process.
//!
//! Page SVGs name document fonts that a Mac rarely has. The same table the PDF export uses
//! (`pdf_font_fallbacks.rs`, included below, one source of truth) appends installed Korean faces to
//! those names. The bundled Noto Sans KR of the containing app (`Contents/Resources/pdf-fonts`,
//! PD-114) is loaded first and the generic families resolve to it, so Korean text never turns into
//! blanks on a Mac whose system lacks the named face.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use usvg::fontdb::Database;

#[path = "../../../../crates/easyoffice-desktop/src/pdf_font_fallbacks.rs"]
#[allow(dead_code)]
pub mod fallbacks;

const BUNDLED_FAMILY: &str = "Noto Sans KR";
const BUNDLED_DIR: &str = "pdf-fonts";
/// `<app>/Contents/PlugIns/<name>.appex/Contents/MacOS/<exe>`: the app's `Contents` is five levels up.
const APP_CONTENTS_LEVELS_UP: usize = 5;

static FONTS: OnceLock<Arc<Database>> = OnceLock::new();

/// Built once per process: the bundled face, then the system fonts.
pub fn shared() -> Arc<Database> {
    FONTS.get_or_init(|| Arc::new(build())).clone()
}

fn build() -> Database {
    let mut db = Database::new();
    if let Some(dir) = bundled_dir() {
        db.load_fonts_dir(dir);
    }
    db.load_system_fonts();
    db.set_serif_family(BUNDLED_FAMILY);
    db.set_sans_serif_family(BUNDLED_FAMILY);
    db.set_monospace_family(BUNDLED_FAMILY);
    with_mapped_faces(db)
}

/// Whether a font path resides in an immutable system or app bundle directory.
/// Fonts in user-writable directories (~/Library/Fonts etc.) are excluded from mmap to prevent SIGBUS.
fn is_immutable_font_path(path: &std::path::Path) -> bool {
    if path.starts_with("/System/") || path.starts_with("/Library/Fonts") {
        return true;
    }
    if let Some(bundled) = bundled_dir() {
        if path.starts_with(&bundled) {
            return true;
        }
    }
    false
}

/// `db` with immutable system/app file faces mapped once for the life of the process. A face loaded from a file is
/// otherwise opened, mapped and unmapped on every access, and text layout accesses it per text run
/// (the PD-114 export fix; here it was 63% of a first page PNG and 35% of a preview PDF).
fn with_mapped_faces(mut db: Database) -> Database {
    let ids: Vec<_> = db
        .faces()
        .filter_map(|face| match &face.source {
            usvg::fontdb::Source::File(path) | usvg::fontdb::Source::SharedFile(path, _) => {
                is_immutable_font_path(path).then_some(face.id)
            }
            usvg::fontdb::Source::Binary(_) => None,
        })
        .collect();
    for id in ids {
        // SAFETY: mapped only for immutable system/app-bundle font paths.
        let _ = unsafe { db.make_shared_face_data(id) };
    }
    db
}

/// The font folder of the app that contains this extension, if the process runs inside one.
fn bundled_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let contents = exe.ancestors().nth(APP_CONTENTS_LEVELS_UP)?;
    let dir = contents.join("Resources").join(BUNDLED_DIR);
    dir.is_dir().then_some(dir)
}

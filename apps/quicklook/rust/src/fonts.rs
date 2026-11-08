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
    db
}

/// The font folder of the app that contains this extension, if the process runs inside one.
fn bundled_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let contents = exe.ancestors().nth(APP_CONTENTS_LEVELS_UP)?;
    let dir = contents.join("Resources").join(BUNDLED_DIR);
    dir.is_dir().then_some(dir)
}

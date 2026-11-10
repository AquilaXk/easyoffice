//! Fonts for the searchable PDF export (PD-101, PD-114).
//!
//! * [`shared_font_db`] builds the font database once per process: the bundled Noto Sans KR
//!   Regular (OFL) first, then the system fonts. The bundled face is what keeps Korean text from
//!   disappearing on a machine without any Korean font (a Linux container, a CI runner); the
//!   generic `serif`/`sans-serif`/`monospace` families map to it, as in the reference app.
//! * [`fallback_selector`] picks the face that draws a character the requested face lacks. It is
//!   ported from the rhwp PDF renderer (`src/renderer/pdf.rs`, `PdfFontFallbackIndex`, rhwp
//!   0.8.7 at `1a76570`, lines 569-753; MIT, Copyright (c) 2025-2026 Edward Kim, see
//!   THIRD_PARTY_NOTICES.md), which the reference app calls for its PDF export. usvg's stock
//!   selector parses every installed face again for every character; this one remembers which
//!   faces were probed and what they answered, as two bits per (character, face) pair.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use usvg::fontdb::{Database, Source, ID};

/// Family the generic `serif`/`sans-serif`/`monospace` names resolve to (the bundled face).
const FALLBACK_FAMILY: &str = "Noto Sans KR";

/// Most distinct characters the fallback index remembers. A document with more distinct fallback
/// characters is unusual enough that the stock selector is safer than an index that keeps growing.
pub const MAX_INDEXED_CHARS: usize = 4_096;
/// Most (character, face) probes the index may spend over one export, so its size and the time it
/// holds the lock stay bounded whatever the installed font inventory is.
pub const MAX_FACE_PROBES: usize = 262_144;

static FONT_DB: OnceLock<Arc<Database>> = OnceLock::new();

/// Font database shared by every export: the given directories (the bundled font) first, then the
/// system fonts. The first call decides the directories; later calls reuse the database, so the
/// font files are scanned once per process.
pub fn shared_font_db(font_dirs: &[PathBuf]) -> Arc<Database> {
    FONT_DB
        .get_or_init(|| Arc::new(build_font_db(font_dirs)))
        .clone()
}

fn build_font_db(font_dirs: &[PathBuf]) -> Database {
    let mut db = Database::new();
    for dir in font_dirs {
        db.load_fonts_dir(dir);
    }
    db.load_system_fonts();
    set_generic_families(&mut db);
    with_mapped_faces(&db)
}

/// Whether the database holds a face loaded from one of `dirs` (the bundled font folder).
pub fn has_face_from(db: &Database, dirs: &[PathBuf]) -> bool {
    db.faces().any(|face| match &face.source {
        Source::File(path) | Source::SharedFile(path, _) => {
            dirs.iter().any(|dir| path.starts_with(dir))
        }
        Source::Binary(_) => false,
    })
}

/// A copy of `db` whose faces keep their font files mapped into memory for the life of the copy.
///
/// A face loaded from a file is opened, mapped and unmapped again on every access, and text
/// conversion accesses it for every text run: on a 300-page report that was 25 million page
/// reclaims and 97 s of system time out of 108 s (PD-114). The mapping is read-only and lives
/// only as long as the returned database, that is, one export.
pub fn with_mapped_faces(db: &Database) -> Database {
    let mut mapped = db.clone();
    let ids: Vec<ID> = mapped.faces().map(|face| face.id).collect();
    for id in ids {
        // SAFETY: fontdb's own access maps the same file the same way for each call; the only
        // difference is the span. A font file that is truncated by another process during an export
        // would fault in either case. A face that cannot be mapped keeps reading its file per call.
        let _ = unsafe { mapped.make_shared_face_data(id) };
    }
    mapped
}

/// Whether the face has a glyph for `c` (the lookup of usvg's stock selector, which keeps its own
/// private).
fn has_char(db: &Database, id: ID, c: char) -> bool {
    db.with_face_data(id, |data, face_index| {
        ttf_parser::Face::parse(data, face_index)
            .ok()
            .and_then(|face| face.glyph_index(c))
            .is_some()
    })
    .unwrap_or(false)
}

/// Points the generic families at [`FALLBACK_FAMILY`].
pub fn set_generic_families(db: &mut Database) {
    db.set_serif_family(FALLBACK_FAMILY);
    db.set_sans_serif_family(FALLBACK_FAMILY);
    db.set_monospace_family(FALLBACK_FAMILY);
}

/// Which faces were probed for one character and which of them have it: two bits per face
/// (bit 0 = probed, bit 1 = covered) packed 32 faces per `u64` word.
#[derive(Default)]
struct Coverage {
    bits: Vec<u64>,
}

impl Coverage {
    const fn locate(face: usize) -> (usize, usize) {
        (face >> 5, (face & 31) << 1)
    }

    fn get(&self, face: usize) -> Option<bool> {
        let (word, shift) = Self::locate(face);
        let pair = (self.bits.get(word).copied().unwrap_or(0) >> shift) & 0b11;
        (pair & 1 != 0).then_some(pair & 2 != 0)
    }

    fn set(&mut self, face: usize, covered: bool) {
        let (word, shift) = Self::locate(face);
        if self.bits.len() <= word {
            self.bits.resize(word + 1, 0);
        }
        let pair = 1u64 | (u64::from(covered) << 1);
        self.bits[word] = (self.bits[word] & !(0b11u64 << shift)) | (pair << shift);
    }
}

/// Answers of the index. `Stock` means the index gave up (a bound was reached) and the stock
/// selector must answer.
enum Lookup {
    Face(Option<ID>),
    Stock,
}

struct FallbackIndex {
    by_char: HashMap<char, Coverage>,
    probes: usize,
    max_chars: usize,
    max_probes: usize,
    disabled: bool,
}

impl FallbackIndex {
    fn new(max_chars: usize, max_probes: usize) -> Self {
        Self {
            by_char: HashMap::new(),
            probes: 0,
            max_chars,
            max_probes,
            disabled: false,
        }
    }

    fn disable(&mut self) -> Lookup {
        self.by_char = HashMap::new();
        self.disabled = true;
        Lookup::Stock
    }

    /// The first face (database order) that is not in `used`, is compatible with the first used
    /// face and has `c`. The checks and their order are those of usvg's stock selector, so the
    /// chosen face is the same one; only the glyph lookup is remembered. The database must not
    /// change during the index's life (the export builds one index per run over one database).
    fn select(&mut self, c: char, used: &[ID], db: &Database) -> Lookup {
        if self.disabled {
            return Lookup::Stock;
        }
        if !self.by_char.contains_key(&c) && self.by_char.len() >= self.max_chars {
            return self.disable();
        }
        let Some(base) = used.first().and_then(|&id| db.face(id)) else {
            return Lookup::Face(None);
        };
        let coverage = self.by_char.entry(c).or_default();
        for (index, face) in db.faces().enumerate() {
            if used.contains(&face.id) {
                continue;
            }
            if base.style != face.style
                && base.weight != face.weight
                && base.stretch != face.stretch
            {
                continue;
            }
            let has = match coverage.get(index) {
                Some(known) => known,
                None => {
                    if self.probes >= self.max_probes {
                        return self.disable();
                    }
                    self.probes += 1;
                    let has = has_char(db, face.id, c);
                    coverage.set(index, has);
                    has
                }
            };
            if has {
                return Lookup::Face(Some(face.id));
            }
        }
        Lookup::Face(None)
    }
}

/// usvg fallback selector backed by a [`FallbackIndex`] with the given bounds. One selector per
/// export; the lock is only held for the lookup.
pub fn fallback_selector(
    max_chars: usize,
    max_probes: usize,
) -> usvg::FallbackSelectionFn<'static> {
    let index = Mutex::new(FallbackIndex::new(max_chars, max_probes));
    let stock = usvg::FontResolver::default_fallback_selector();
    Box::new(move |c, used, db| {
        let lookup = match index.lock() {
            Ok(mut index) => index.select(c, used, db),
            Err(_) => Lookup::Stock,
        };
        match lookup {
            Lookup::Face(id) => id,
            Lookup::Stock => stock(c, used, db),
        }
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::path::Path;

    /// The bundled face, when the rhwp checkout that holds it is present. The CI jobs that build the
    /// desktop shell check it out (`tauri.conf.json` ships it as `pdf-fonts/`), so there a missing
    /// file fails the test; elsewhere (`CI` unset) the tests that need it are skipped.
    pub(crate) fn bundled_font_dir() -> Option<PathBuf> {
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../third_party/rhwp/ttfs/opensource");
        let found = dir.join("NotoSansKR-Regular.ttf").is_file();
        assert!(
            found || std::env::var_os("CI").is_none(),
            "CI must have the bundled font {}: run scripts/checkout-rhwp.mjs",
            dir.display()
        );
        found.then_some(dir)
    }

    #[test]
    fn coverage_bits_remember_probed_and_covered_faces_apart() {
        let mut coverage = Coverage::default();
        assert_eq!(coverage.get(0), None);
        coverage.set(3, true);
        coverage.set(130, false);
        assert_eq!(coverage.get(3), Some(true));
        assert_eq!(coverage.get(130), Some(false));
        assert_eq!(coverage.get(4), None, "a neighbour is not probed");
        assert_eq!(coverage.get(1_000_000), None, "beyond the words is unknown");
    }

    #[test]
    fn a_face_from_the_bundled_folder_is_recognised_and_other_folders_are_not() {
        let Some(dir) = bundled_font_dir() else {
            return;
        };
        let Some(db) = bundled_only_db() else { return };
        assert!(has_face_from(&db, &[dir]));
        assert!(!has_face_from(&db, &[PathBuf::from("/nonexistent")]));
        assert!(!has_face_from(&db, &[]));
        assert!(!has_face_from(
            &Database::new(),
            &[bundled_font_dir().expect("dir")]
        ));
    }

    #[test]
    fn the_shared_font_database_is_built_once() {
        let first = shared_font_db(&[]);
        let second = shared_font_db(&[PathBuf::from("/nonexistent")]);
        assert!(Arc::ptr_eq(&first, &second));
    }

    /// A database with only the bundled face: what a machine without Korean fonts has.
    pub(crate) fn bundled_only_db() -> Option<Database> {
        let mut db = Database::new();
        db.load_fonts_dir(bundled_font_dir()?);
        set_generic_families(&mut db);
        Some(db)
    }

    #[test]
    fn the_bundled_face_covers_hangul_and_the_generic_families_use_it() {
        let Some(db) = bundled_only_db() else { return };
        let id = db
            .query(&usvg::fontdb::Query {
                families: &[usvg::fontdb::Family::Name(FALLBACK_FAMILY)],
                ..usvg::fontdb::Query::default()
            })
            .expect("bundled face");
        for c in ['한', '글', 'A', '1'] {
            assert!(has_char(&db, id, c), "{c}");
        }
        for family in [
            usvg::fontdb::Family::Serif,
            usvg::fontdb::Family::SansSerif,
            usvg::fontdb::Family::Monospace,
        ] {
            let query = usvg::fontdb::Query {
                families: &[family],
                ..usvg::fontdb::Query::default()
            };
            assert_eq!(db.query(&query), Some(id));
        }
    }

    #[test]
    fn the_index_picks_what_the_stock_selector_picks() {
        let Some(mut db) = bundled_only_db() else {
            return;
        };
        // A second, unrelated face first in the database would not cover Hangul; the index must skip
        // it exactly as the stock selector does. Loading the same file twice gives two faces.
        db.load_fonts_dir(bundled_font_dir().expect("dir"));
        let used = [db.faces().next().expect("face").id];
        let stock = usvg::FontResolver::default_fallback_selector();
        let mut db = Arc::new(db);
        let select = fallback_selector(MAX_INDEXED_CHARS, MAX_FACE_PROBES);
        for c in ['한', 'A', '가', '\u{10FFFF}'] {
            assert_eq!(select(c, &used, &mut db), stock(c, &used, &mut db), "{c:?}");
        }
    }

    #[test]
    fn an_exhausted_budget_hands_over_to_the_stock_selector() {
        let Some(db) = bundled_only_db() else { return };
        let used = [db.faces().next().expect("face").id];
        let mut db = Arc::new(db);
        let stock = usvg::FontResolver::default_fallback_selector();
        // No character slots at all: every lookup is the stock selector's.
        let none = fallback_selector(0, MAX_FACE_PROBES);
        // No probes at all: the first uncached lookup gives up, and so does every later one.
        let no_probes = fallback_selector(MAX_INDEXED_CHARS, 0);
        for c in ['한', 'A'] {
            assert_eq!(none(c, &used, &mut db), stock(c, &used, &mut db));
            assert_eq!(no_probes(c, &used, &mut db), stock(c, &used, &mut db));
        }
    }

    #[test]
    fn the_character_bound_is_enforced_for_hostile_text() {
        let mut index = FallbackIndex::new(8, usize::MAX);
        let mut db = Database::new();
        let Some(dir) = bundled_font_dir() else {
            return;
        };
        db.load_fonts_dir(dir);
        let used = [db.faces().next().expect("face").id];
        for code in 0xAC00..0xAC00 + 64 {
            let c = char::from_u32(code).expect("hangul");
            let _ = index.select(c, &used, &db);
        }
        assert!(index.disabled);
        assert!(index.by_char.is_empty(), "the index lets go of its memory");
    }
}

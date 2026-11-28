use super::*;
use crate::pdf_export::{verify_exported_pdf, PdfAssembler, PdfCheck};
use crate::pdf_fonts::set_generic_families;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use ttf_parser::{Face, OutlineBuilder};

fn woff2_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts/NotoSansKR-Regular.woff2")
}

fn shipped_woff2() -> Vec<u8> {
    std::fs::read(woff2_path()).expect("assets/fonts/NotoSansKR-Regular.woff2 is tracked")
}

/// The TTF this face replaced (the rhwp checkout's file), when the checkout is present.
fn old_ttf() -> Option<Vec<u8>> {
    let dir = crate::pdf_fonts::tests::bundled_font_dir()?;
    std::fs::read(dir.join("NotoSansKR-Regular.ttf")).ok()
}

/// A hash of a glyph's outline commands and coordinates.
#[derive(Default)]
struct OutlineHash(u64);

impl OutlineHash {
    fn mix(&mut self, tag: u8, values: &[f32]) {
        self.0 = (self.0 ^ u64::from(tag)).wrapping_mul(0x0100_0000_01b3);
        for v in values {
            self.0 = (self.0 ^ u64::from(v.to_bits())).wrapping_mul(0x0100_0000_01b3);
        }
    }
}

impl OutlineBuilder for OutlineHash {
    fn move_to(&mut self, x: f32, y: f32) {
        self.mix(1, &[x, y]);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.mix(2, &[x, y]);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.mix(3, &[x1, y1, x, y]);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.mix(4, &[x1, y1, x2, y2, x, y]);
    }
    fn close(&mut self) {
        self.mix(5, &[]);
    }
}

fn code_points(face: &Face) -> BTreeSet<u32> {
    let mut out = BTreeSet::new();
    for subtable in face.tables().cmap.expect("cmap").subtables {
        if subtable.is_unicode() {
            subtable.codepoints(|c| {
                out.insert(c);
            });
        }
    }
    out
}

/// A database with only the face of `font`, as a machine without Korean fonts has it.
fn db_of(font: Vec<u8>) -> Arc<Database> {
    let mut db = Database::new();
    db.load_font_data(font);
    set_generic_families(&mut db);
    Arc::new(db)
}

const KOREAN_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="320" height="120"><text x="16" y="64" font-family="함초롬바탕, serif" font-size="24">한글 검색 Search 123</text></svg>"#;

fn pdf_of(db: &Arc<Database>) -> Vec<u8> {
    let mut assembler = PdfAssembler::new(db, true);
    assembler
        .add_pages(&[KOREAN_SVG.to_string()])
        .expect("page");
    assembler.finish().expect("pdf")
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn the_shipped_woff2_decodes_in_bounds_and_to_a_font_with_hangul() {
    let font = decode_woff2(&shipped_woff2()).expect("decode");
    assert!(font.len() <= MAX_FONT_BYTES);
    let face = Face::parse(&font, 0).expect("font");
    assert_eq!(face.units_per_em(), 1000);
    assert!(face.glyph_index('한').is_some() && face.glyph_index('A').is_some());
}

/// Every character of the old TTF has the same glyph (outline and advance) in the decoded woff2, and the
/// decoded font has no character the TTF lacks: a PDF drawn from either has the same text shapes.
#[test]
fn the_decoded_face_has_the_characters_outlines_and_advances_of_the_old_ttf() {
    let Some(ttf) = old_ttf() else { return };
    let font = decode_woff2(&shipped_woff2()).expect("decode");
    let (old, new) = (
        Face::parse(&ttf, 0).unwrap(),
        Face::parse(&font, 0).unwrap(),
    );
    assert_eq!(
        (
            old.units_per_em(),
            old.ascender(),
            old.descender(),
            old.line_gap()
        ),
        (
            new.units_per_em(),
            new.ascender(),
            new.descender(),
            new.line_gap()
        )
    );
    assert_eq!(old.number_of_glyphs(), new.number_of_glyphs());
    let (old_points, new_points) = (code_points(&old), code_points(&new));
    assert_eq!(old_points, new_points);
    assert!(old_points.len() > 11_000, "{}", old_points.len());
    for point in old_points {
        let ch = char::from_u32(point).expect("scalar value");
        let (a, b) = (old.glyph_index(ch).unwrap(), new.glyph_index(ch).unwrap());
        assert_eq!(old.glyph_hor_advance(a), new.glyph_hor_advance(b), "{ch:?}");
        let (mut ha, mut hb) = (OutlineHash::default(), OutlineHash::default());
        old.outline_glyph(a, &mut ha);
        new.outline_glyph(b, &mut hb);
        assert_eq!(ha.0, hb.0, "outline of {ch:?}");
    }
}

/// The searchable PDF of a Korean page, drawn with only the decoded woff2 face, is what it was with only the TTF:
/// the same font name, the same text map (the syllables a viewer searches and copies) and a valid check.
#[test]
fn the_searchable_pdf_from_the_woff2_face_matches_the_one_from_the_old_ttf() {
    let Some(ttf) = old_ttf() else { return };
    let from_woff2 = pdf_of(&db_of(decode_woff2(&shipped_woff2()).expect("decode")));
    let from_ttf = pdf_of(&db_of(ttf));
    for pdf in [&from_woff2, &from_ttf] {
        assert_eq!(
            verify_exported_pdf(pdf, 1, true),
            Ok(PdfCheck {
                pages: 1,
                warning: None
            })
        );
        assert!(contains(pdf, b"NotoSansKR"));
        assert!(contains(pdf, b"<D55C>") && contains(pdf, b"<AE00>"));
    }
    assert_eq!(text_map(&from_woff2), text_map(&from_ttf));
    let (a, b) = (from_woff2.len() as f64, from_ttf.len() as f64);
    assert!((a - b).abs() / b < 0.02, "pdf sizes {a} vs {b}");
}

/// The `bfchar` lines of the first text map of a PDF, in file order.
fn text_map(pdf: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(pdf)
        .lines()
        .filter(|line| {
            line.starts_with('<') && line.matches('<').count() == 2 && line.contains("> <")
        })
        .map(str::to_string)
        .collect()
}

/// The key the shell asks the executable's embedded assets for (`main.rs`) is the editor bundle's copy of the
/// tracked font file: the same bytes, so what the export decodes is what these tests compared with the old TTF.
#[test]
fn the_assets_embedded_in_the_executable_hold_the_face_under_the_key_the_shell_uses() {
    let context: tauri::Context<tauri::Wry> = tauri::generate_context!();
    let key = tauri::utils::assets::AssetKey::from(FACE_ASSET);
    let woff2 = context.assets().get(&key).expect("embedded asset");
    assert_eq!(&*woff2, shipped_woff2().as_slice());
}

#[test]
fn a_hostile_woff2_is_refused_without_a_panic() {
    let good = shipped_woff2();
    assert!(decode_woff2(&[]).is_err());
    assert!(decode_woff2(b"wOF2").is_err(), "a header cut short");
    assert!(decode_woff2(&good[..good.len() / 2]).is_err(), "a cut file");
    assert!(
        decode_woff2(&vec![0u8; good.len()]).is_err(),
        "no signature"
    );
    let mut garbage = good.clone();
    for byte in garbage.iter_mut().skip(48).step_by(7) {
        *byte ^= 0x5a;
    }
    assert!(decode_woff2(&garbage).is_err(), "damaged data");
    // A header that declares a 4 GiB font is refused on the header, before the decoder allocates for it.
    let mut huge = good.clone();
    huge[SFNT_SIZE_AT..SFNT_SIZE_AT + 4].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(decode_woff2(&huge).unwrap_err().contains("상한"));
    let mut just_over = good.clone();
    just_over[SFNT_SIZE_AT..SFNT_SIZE_AT + 4]
        .copy_from_slice(&u32::try_from(MAX_FONT_BYTES + 1).unwrap().to_be_bytes());
    assert!(decode_woff2(&just_over).unwrap_err().contains("상한"));
    // An input past the size limit is refused before it is read.
    let mut big = good;
    big.resize(MAX_WOFF2_BYTES + 1, 0);
    assert!(decode_woff2(&big).unwrap_err().contains("상한"));
}

#[test]
fn the_registered_face_is_shared_by_the_database_and_recognised_in_it() {
    // A font that is not Noto Sans KR is refused: no Hangul syllable, so it is no fallback.
    assert!(register(b"wOF2 not a font").is_err());
    register(&shipped_woff2()).expect("register");
    // The first registration stays; a second call is harmless.
    register(&shipped_woff2()).expect("again");
    let mut db = Database::new();
    assert!(!has_face(&db));
    db.load_font_source(source().expect("registered"));
    assert!(has_face(&db));
    assert_eq!(db.len(), 1);
    set_generic_families(&mut db);
    let id = db
        .query(&usvg::fontdb::Query {
            families: &[usvg::fontdb::Family::Name("Noto Sans KR")],
            ..usvg::fontdb::Query::default()
        })
        .expect("face");
    assert!(matches!(
        db.face(id).expect("face").source,
        Source::Binary(_)
    ));
}

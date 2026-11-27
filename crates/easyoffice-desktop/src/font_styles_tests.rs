use super::*;
use crate::pdf_fonts::tests::bundled_only_db;

fn record(id: u16, locale: Locale, text: &str) -> NameRecord {
    NameRecord {
        id,
        locale,
        text: text.to_string(),
    }
}

fn style(face: &str, weight: u16, width: u16, italic: bool) -> FontStyle {
    FontStyle {
        face: face.to_string(),
        full_name: face.to_string(),
        style: face.to_string(),
        weight,
        width,
        italic,
    }
}

const STYLE_IDS: [u16; 2] = [name_id::TYPOGRAPHIC_SUBFAMILY, name_id::SUBFAMILY];
const KOREAN_FIRST: [Locale; 3] = [Locale::Korean, Locale::English, Locale::Other];

#[test]
fn the_style_name_is_korean_when_the_font_has_one_and_else_english() {
    let both = [
        record(2, Locale::English, "Light"),
        record(2, Locale::Korean, "가는체"),
    ];
    assert_eq!(
        pick_name(&both, &STYLE_IDS, &KOREAN_FIRST),
        Some("가는체".to_string())
    );
    let english_only = [record(2, Locale::English, "Light")];
    assert_eq!(
        pick_name(&english_only, &STYLE_IDS, &KOREAN_FIRST),
        Some("Light".to_string())
    );
    let other_only = [record(2, Locale::Other, "Leicht")];
    assert_eq!(
        pick_name(&other_only, &STYLE_IDS, &KOREAN_FIRST),
        Some("Leicht".to_string()),
        "a font with neither Korean nor English keeps what it has"
    );
    assert_eq!(pick_name(&[], &STYLE_IDS, &KOREAN_FIRST), None);
}

#[test]
fn the_typographic_subfamily_wins_over_the_subfamily_whatever_the_language() {
    let records = [
        record(2, Locale::Korean, "굵게"),
        record(17, Locale::English, "Condensed Bold"),
    ];
    assert_eq!(
        pick_name(&records, &STYLE_IDS, &KOREAN_FIRST),
        Some("Condensed Bold".to_string())
    );
}

#[test]
fn a_face_without_a_subfamily_takes_the_end_of_its_postscript_name() {
    assert_eq!(style_from_postscript("AppleSDGothicNeo-Light"), "Light");
    assert_eq!(style_from_postscript("Plain"), FALLBACK_STYLE);
    assert_eq!(style_from_postscript("Odd-"), FALLBACK_STYLE);
    assert_eq!(
        style_from_postscript(&format!("A-{}", "x".repeat(MAX_NAME_BYTES + 1))),
        FALLBACK_STYLE,
        "an over-long tail is not a style name"
    );
}

#[test]
fn a_mac_roman_record_is_read_only_when_it_is_plain_ascii() {
    use ttf_parser::name::Name;
    use ttf_parser::PlatformId;
    let name = |platform_id, name: &'static [u8]| Name {
        platform_id,
        encoding_id: 0,
        language_id: 0,
        name_id: name_id::FULL_NAME,
        name,
    };
    assert_eq!(
        record_text(&name(PlatformId::Macintosh, b"Helvetica Neue Light")),
        Some("Helvetica Neue Light".to_string())
    );
    assert_eq!(
        record_text(&name(PlatformId::Macintosh, b"Caf\x8e")),
        None,
        "the rest of the Roman table is not decoded"
    );
    assert_eq!(
        record_text(&name(PlatformId::Unicode, &[0xAC, 0x00, 0x00, 0x41])),
        Some("\u{AC00}A".to_string())
    );
}

#[test]
fn the_popup_lists_normal_widths_first_light_to_heavy_without_repeats() {
    let ordered = arranged(vec![
        style("B-Italic", 400, 100, true),
        style("Narrow-Regular", 400, 75, false),
        style("B-Bold", 700, 100, false),
        style("B-Light", 300, 100, false),
        style("B-Bold", 700, 100, false),
        style("B-Regular", 400, 100, false),
    ]);
    let faces: Vec<_> = ordered.iter().map(|s| s.face.as_str()).collect();
    assert_eq!(
        faces,
        [
            "B-Light",
            "B-Regular",
            "B-Italic",
            "B-Bold",
            "Narrow-Regular"
        ]
    );
}

#[test]
fn a_family_lists_at_most_the_limit() {
    let many: Vec<_> = (0..MAX_STYLES_PER_FAMILY * 2)
        .map(|i| style(&format!("F-{i:03}"), 400, 100, false))
        .collect();
    assert_eq!(arranged(many).len(), MAX_STYLES_PER_FAMILY);
}

#[test]
fn widths_are_css_percentages() {
    assert_eq!(width_percent(Stretch::Normal), 100);
    assert_eq!(width_percent(Stretch::Condensed), 75);
    assert_eq!(width_percent(Stretch::UltraCondensed), 50);
    assert_eq!(width_percent(Stretch::UltraExpanded), 200);
}

#[test]
fn a_name_that_cannot_be_a_font_answers_nothing() {
    let Some(db) = bundled_only_db() else { return };
    let long = "a".repeat(MAX_NAME_BYTES + 1);
    for bad in ["", "   ", "Noto\u{0}Sans", "Noto\nSans", long.as_str()] {
        assert_eq!(styles_of(&db, bad), FontStyleGroup::default(), "{bad:?}");
    }
    assert_eq!(
        styles_of(&Database::new(), "Noto Sans KR"),
        FontStyleGroup::default()
    );
    assert_eq!(styles_of(&db, "No Such Family"), FontStyleGroup::default());
}

#[test]
fn a_family_postscript_or_full_name_finds_the_variants_of_the_family() {
    let Some(db) = bundled_only_db() else { return };
    let group = styles_of(&db, "noto sans kr");
    assert_eq!(group.family, "Noto Sans KR");
    assert!(!group.styles.is_empty());
    assert!(group.styles.len() <= MAX_STYLES_PER_FAMILY);
    for variant in &group.styles {
        assert!(!variant.face.is_empty() && !variant.style.is_empty());
        assert!(variant.full_name.starts_with("Noto Sans KR"), "{variant:?}");
        assert!((1..=1000).contains(&variant.weight));
    }
    let regular = group
        .styles
        .iter()
        .find(|s| s.weight == 400 && !s.italic)
        .expect("the bundled face is the regular weight");
    assert_eq!(styles_of(&db, &regular.face), group, "by PostScript name");
    assert_eq!(styles_of(&db, &regular.full_name), group, "by full name");
    assert_eq!(
        styles_of(&db, &format!("Noto Sans KR {}", regular.style)),
        group,
        "by family and style"
    );
}

/// What the installed fonts of this machine list, to read by eye:
/// `cargo test -p easyoffice-desktop --bin EasyOffice system_font_styles -- --ignored --nocapture`.
#[test]
#[ignore = "reads the machine's fonts"]
fn system_font_styles_are_listed() {
    let mut db = Database::new();
    db.load_system_fonts();
    let started = std::time::Instant::now();
    for name in [
        "Apple SD Gothic Neo",
        "Helvetica Neue",
        "맑은 고딕",
        "Arial",
    ] {
        let group = styles_of(&db, name);
        eprintln!("{name} -> {} ({} styles)", group.family, group.styles.len());
        for s in &group.styles {
            eprintln!(
                "  {:<28} {:<22} {} w{} {}%{}",
                s.face,
                s.style,
                s.full_name,
                s.weight,
                s.width,
                if s.italic { " italic" } else { "" }
            );
        }
    }
    eprintln!("four lookups: {:?}", started.elapsed());
}

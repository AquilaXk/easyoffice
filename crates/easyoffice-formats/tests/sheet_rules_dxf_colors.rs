//! The fill and the text colour of a conditional format's `dxf` resolve theme and `indexed` colours, as the border colour
//! does, and a colour a hostile file makes up is no colour (PD-288).

use easyoffice_formats::{
    parse_sqref, CellBorders, CfKind, CfRule, CfStyle, Cmp, OoxmlCodec, SheetRules, SheetStyling,
    ZipArchiveCodec, ZipSecurityPolicy, FLAG_BOLD,
};

const FILL: &str = "<bgColor rgb=\"FFFFC7CE\"/>";
const TEXT: &str = "<color rgb=\"FF9C0006\"/>";

fn sheet(theme: Option<[u32; 12]>) -> SheetStyling {
    SheetStyling {
        rules: SheetRules {
            cf: vec![CfRule {
                ranges: parse_sqref("A1:A3"),
                priority: 1,
                stop_if_true: false,
                kind: CfKind::CellIs {
                    cmp: Cmp::Greater,
                    a: "1".into(),
                    b: String::new(),
                },
                style: CfStyle {
                    bg: Some(0xFFC7CE),
                    fg: Some(0x9C0006),
                    flags: FLAG_BOLD,
                    border: CellBorders::NONE,
                },
            }],
            ..SheetRules::default()
        },
        workbook_theme: theme,
        ..SheetStyling::default()
    }
}

fn file(theme: Option<[u32; 12]>) -> Vec<u8> {
    let cells = vec![("Sheet1".to_string(), vec![(0, 0, "5".to_string())])];
    OoxmlCodec::pack_xlsx_multi_styled(&cells, &[], &[sheet(theme)]).unwrap()
}

/// `bytes` with the text of `xl/styles.xml` rewritten by `edit`.
fn with_styles(bytes: &[u8], edit: impl FnOnce(String) -> String) -> Vec<u8> {
    let mut entries: Vec<(String, Vec<u8>)> =
        ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
            .unwrap()
            .into_iter()
            .map(|e| (e.path, e.data))
            .collect();
    let slot = entries
        .iter_mut()
        .find(|(p, _)| p == "xl/styles.xml")
        .unwrap();
    slot.1 = edit(String::from_utf8(slot.1.clone()).unwrap()).into_bytes();
    ZipArchiveCodec::pack_compressed(&entries).unwrap()
}

/// The style the rule has after the fill element `fill` and the text colour element `text` replace the written ones.
fn style_with(bytes: &[u8], fill: &str, text: &str) -> CfStyle {
    let patched = with_styles(bytes, |xml| {
        assert!(xml.contains(FILL) && xml.contains(TEXT), "{xml}");
        xml.replacen(FILL, fill, 1).replacen(TEXT, text, 1)
    });
    let rules = OoxmlCodec::unpack_xlsx_with_styling(&patched).unwrap().1[0]
        .1
        .rules
        .clone();
    assert_eq!(rules.cf.len(), 1);
    rules.cf[0].style
}

#[test]
fn rgb_colours_read_as_before() {
    let style = style_with(&file(None), FILL, TEXT);
    assert_eq!((style.bg, style.fg), (Some(0xFFC7CE), Some(0x9C0006)));
    assert_eq!(style.flags, FLAG_BOLD);
}

// The tints are the "lighter" steps of the colour menu of a spreadsheet program: orange (accent 2) lighter 40% is F4B183 there,
// and blue (accent 1) lighter 80% is D9E1F2 there and DAE3F3 here: the float HLS shift of `apply_tint` (the one the cell fills
// use) is one to two steps from the program's integer HLS in a channel.
#[test]
fn theme_colours_with_a_tint_resolve_through_the_office_theme() {
    let bytes = file(None);
    let style = style_with(
        &bytes,
        "<bgColor theme=\"5\" tint=\"0.39997558519241921\"/>",
        "<color theme=\"1\"/>",
    );
    assert_eq!((style.bg, style.fg), (Some(0xF4B183), Some(0x000000)));
    let style = style_with(
        &bytes,
        "<bgColor theme=\"4\" tint=\"0.79998168889431442\"/>",
        "<color theme=\"0\"/>",
    );
    assert_eq!((style.bg, style.fg), (Some(0xDAE3F3), Some(0xFFFFFF)));
}

#[test]
fn a_theme_the_workbook_carries_replaces_the_office_one() {
    let mut theme = [0u32; 12];
    theme[5] = 0x112233;
    theme[1] = 0x445566;
    let style = style_with(
        &file(Some(theme)),
        "<bgColor theme=\"5\"/>",
        "<color theme=\"1\"/>",
    );
    assert_eq!((style.bg, style.fg), (Some(0x112233), Some(0x445566)));
}

#[test]
fn indexed_colours_resolve_through_the_default_palette_and_the_workbook_override() {
    let bytes = file(None);
    let style = style_with(
        &bytes,
        "<bgColor indexed=\"10\"/>",
        "<color indexed=\"22\"/>",
    );
    assert_eq!((style.bg, style.fg), (Some(0xFF0000), Some(0xC0C0C0)));
    let overridden = with_styles(&bytes, |xml| {
        xml.replacen(
            "</styleSheet>",
            "<colors><indexedColors><rgbColor rgb=\"FF010203\"/></indexedColors></colors></styleSheet>",
            1,
        )
    });
    let style = style_with(
        &overridden,
        "<bgColor indexed=\"0\"/>",
        "<color indexed=\"1\"/>",
    );
    assert_eq!((style.bg, style.fg), (Some(0x010203), Some(0xFFFFFF)));
}

#[test]
fn an_automatic_colour_is_no_colour_and_a_fill_falls_back_to_its_other_colour() {
    let bytes = file(None);
    let style = style_with(&bytes, "<bgColor auto=\"1\"/>", "<color auto=\"1\"/>");
    assert_eq!((style.bg, style.fg), (None, None));
    // The system foreground is automatic too, so the fill takes `fgColor`.
    let style = style_with(
        &bytes,
        "<bgColor indexed=\"64\"/><fgColor theme=\"4\"/>",
        TEXT,
    );
    assert_eq!(style.bg, Some(0x4472C4));
    // `rgb` wins over `theme`, that over `indexed`.
    let style = style_with(
        &bytes,
        "<bgColor rgb=\"FF00FF00\" theme=\"4\" indexed=\"10\"/>",
        "<color theme=\"4\" indexed=\"10\"/>",
    );
    assert_eq!((style.bg, style.fg), (Some(0x00FF00), Some(0x4472C4)));
}

#[test]
fn colours_a_hostile_file_makes_up_are_no_colour() {
    let bytes = file(None);
    let huge = "9".repeat(300_000);
    let wrong = [
        "theme=\"12\"".to_string(),
        "theme=\"999999999999\"".to_string(),
        "theme=\"-1\"".to_string(),
        "theme=\"4\" tint=\"2\"".to_string(),
        "theme=\"4\" tint=\"-1.5\"".to_string(),
        "theme=\"4\" tint=\"NaN\"".to_string(),
        "theme=\"4\" tint=\"1e400\"".to_string(),
        "theme=\"4\" tint=\"abc\"".to_string(),
        "indexed=\"66\"".to_string(),
        "indexed=\"99999999999\"".to_string(),
        "indexed=\"-3\"".to_string(),
        "indexed=\"x\"".to_string(),
        "rgb=\"ZZZZZZZZ\"".to_string(),
        "rgb=\"FFF\"".to_string(),
        "rgb=\"\"".to_string(),
        format!("rgb=\"{huge}\""),
        format!("theme=\"{huge}\""),
        format!("indexed=\"{huge}\""),
        format!("theme=\"4\" tint=\"{huge}\""),
    ];
    for attrs in wrong {
        let style = style_with(
            &bytes,
            &format!("<bgColor {attrs}/>"),
            &format!("<color {attrs}/>"),
        );
        assert_eq!((style.bg, style.fg), (None, None), "{:.50}", attrs);
        assert_eq!(style.flags, FLAG_BOLD, "the rest of the format stays");
    }
}

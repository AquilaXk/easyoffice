//! The border of a conditional format survives XLSX and ODS writing and reading, and a border value a hostile
//! file makes up is bounded or ignored (PD-284).

use easyoffice_formats::{
    parse_sqref, BorderEdge, BorderLine, CellBorders, CfKind, CfRule, CfStyle, Cmp, DocumentFormat,
    OdfCodec, OoxmlCodec, SheetRules, SheetStyling, ZipArchiveCodec, ZipSecurityPolicy, FLAG_BOLD,
    MAX_RULES_PER_SHEET,
};

fn edge(line: BorderLine, rgb: u32) -> BorderEdge {
    BorderEdge::new(line, rgb)
}

fn outline(line: BorderLine, rgb: u32) -> CellBorders {
    CellBorders {
        top: edge(line, rgb),
        right: edge(line, rgb),
        bottom: edge(line, rgb),
        left: edge(line, rgb),
    }
}

fn rule(ranges: &str, priority: u32, style: CfStyle) -> CfRule {
    CfRule {
        ranges: parse_sqref(ranges),
        priority,
        stop_if_true: false,
        kind: CfKind::CellIs {
            cmp: Cmp::Greater,
            a: "1".into(),
            b: String::new(),
        },
        style,
    }
}

/// A border alone, a border with colours and bold, a partial border of mixed lines, one of each line kind.
fn sample_rules() -> SheetRules {
    let mixed = CellBorders {
        top: edge(BorderLine::Double, 0x0000FF),
        right: edge(BorderLine::Dashed, 0x00FF00),
        bottom: BorderEdge::NONE,
        left: edge(BorderLine::Thick, 0x000000),
    };
    let dotted = CellBorders {
        bottom: edge(BorderLine::Dotted, 0x123456),
        ..CellBorders::NONE
    };
    let styles = [
        CfStyle {
            border: outline(BorderLine::Thin, 0xFF0000),
            ..CfStyle::default()
        },
        CfStyle {
            bg: Some(0xFFC7CE),
            fg: Some(0x9C0006),
            flags: FLAG_BOLD,
            border: mixed,
        },
        CfStyle {
            border: dotted,
            ..CfStyle::default()
        },
    ];
    SheetRules {
        cf: styles
            .into_iter()
            .enumerate()
            .map(|(i, s)| rule(&format!("A{n}:A{n}", n = i + 1), i as u32 + 1, s))
            .collect(),
        ..SheetRules::default()
    }
}

fn styling(rules: SheetRules) -> SheetStyling {
    SheetStyling {
        rules,
        ..SheetStyling::default()
    }
}

type SheetCells = Vec<(String, Vec<(u32, u32, String)>)>;

fn cells() -> SheetCells {
    vec![("Sheet1".to_string(), vec![(0, 0, "5".to_string())])]
}

fn entries_of(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
        .unwrap()
        .into_iter()
        .map(|e| (e.path, e.data))
        .collect()
}

fn text_of(entries: &[(String, Vec<u8>)], path: &str) -> String {
    let (_, data) = entries.iter().find(|(p, _)| p == path).unwrap();
    String::from_utf8(data.clone()).unwrap()
}

/// `bytes` with the text of the part `path` rewritten by `edit`.
fn with_part(bytes: &[u8], path: &str, edit: impl FnOnce(String) -> String) -> Vec<u8> {
    let mut entries = entries_of(bytes);
    let slot = entries.iter_mut().find(|(p, _)| p == path).unwrap();
    slot.1 = edit(String::from_utf8(slot.1.clone()).unwrap()).into_bytes();
    ZipArchiveCodec::pack_compressed(&entries).unwrap()
}

fn read_xlsx(bytes: &[u8]) -> SheetRules {
    OoxmlCodec::unpack_xlsx_with_styling(bytes).unwrap().1[0]
        .1
        .rules
        .clone()
}

fn read_ods(bytes: &[u8]) -> SheetRules {
    easyoffice_formats::read_sheet_styling(bytes, DocumentFormat::Ods).unwrap()[0]
        .1
        .rules
        .clone()
}

fn xlsx_with(rules: SheetRules) -> Vec<u8> {
    OoxmlCodec::pack_xlsx_multi_styled(&cells(), &[], &[styling(rules)]).unwrap()
}

fn ods_with(rules: SheetRules) -> Vec<u8> {
    OdfCodec::pack_ods_multi_styled(&cells(), &[styling(rules)]).unwrap()
}

#[test]
fn xlsx_borders_round_trip_through_the_dxf_list() {
    let rules = sample_rules();
    let bytes = xlsx_with(rules.clone());
    let styles = text_of(&entries_of(&bytes), "xl/styles.xml");
    assert!(
        styles.contains("<dxf><border><left style=\"thin\"><color rgb=\"FFFF0000\"/></left>"),
        "{styles}"
    );
    // The fill follows the font and comes before the border (the order of `CT_Dxf`).
    assert!(styles.contains("</fill><border><left style=\"medium\""));
    let got = read_xlsx(&bytes);
    assert_eq!(got.cf, rules.cf);
    assert!(got.unsupported.is_empty());
}

#[test]
fn ods_borders_round_trip_through_the_rule_styles() {
    let rules = sample_rules();
    let bytes = ods_with(rules.clone());
    let content = text_of(&entries_of(&bytes), "content.xml");
    assert!(
        content.contains("fo:border-top=\"0.74pt solid #ff0000\""),
        "{content}"
    );
    assert!(content.contains("fo:border-top=\"1.5pt double #0000ff\""));
    assert!(content.contains("style:border-line-width-top=\"0.5pt 0.5pt 0.5pt\""));
    let got = read_ods(&bytes);
    assert_eq!(got.cf, rules.cf);
    assert!(got.unsupported.is_empty());
}

#[test]
fn a_rule_whose_only_format_is_a_border_is_written_with_a_format() {
    let rules = SheetRules {
        cf: vec![rule(
            "A1:A3",
            1,
            CfStyle {
                border: outline(BorderLine::Thin, 0xFF0000),
                ..CfStyle::default()
            },
        )],
        ..SheetRules::default()
    };
    let xlsx = xlsx_with(rules.clone());
    assert!(text_of(&entries_of(&xlsx), "xl/styles.xml").contains("<dxfs count=\"1\">"));
    assert!(text_of(&entries_of(&xlsx), "xl/worksheets/sheet1.xml").contains("dxfId=\"0\""));
    let ods = ods_with(rules);
    assert!(
        text_of(&entries_of(&ods), "content.xml").contains("calcext:apply-style-name=\"eocf1\"")
    );
}

#[test]
fn xlsx_dxf_borders_over_a_bound_or_made_up_are_ignored() {
    let rules = SheetRules {
        cf: vec![rule(
            "A1",
            1,
            CfStyle {
                bg: Some(0xFFC7CE),
                ..CfStyle::default()
            },
        )],
        ..SheetRules::default()
    };
    let bytes = xlsx_with(rules);
    let huge = "q".repeat(300_000);
    let hostile = [
        // A style name and a colour the format does not have, an oversized style, an oversized colour attribute.
        "<border><left style=\"wavy\"><color rgb=\"FFFF0000\"/></left></border>".to_string(),
        format!("<border><left style=\"thin{huge}\"><color rgb=\"FFFF0000\"/></left></border>"),
        format!("<border><left style=\"thin\"><color rgb=\"FFFF0000\" x=\"{huge}\"/></left></border>"),
        // Sides that a rule has no use for.
        "<border><diagonal style=\"thin\"/><vertical style=\"thin\"/><horizontal style=\"thin\"/></border>"
            .to_string(),
    ];
    for border in hostile {
        let patched = with_part(&bytes, "xl/styles.xml", |xml| {
            xml.replacen("</fill></dxf>", &format!("</fill>{border}</dxf>"), 1)
        });
        let got = read_xlsx(&patched);
        assert_eq!(got.cf.len(), 1);
        let style = got.cf[0].style;
        assert_eq!(style.bg, Some(0xFFC7CE));
        // An oversized colour is the automatic one on a side that has its line; nothing else gets in.
        let drawn = [style.border.top, style.border.right, style.border.bottom]
            .into_iter()
            .all(BorderEdge::is_none);
        assert!(drawn, "{:?}", style.border);
        assert!(
            style.border.left.is_none() || style.border.left == edge(BorderLine::Thin, 0),
            "{:?}",
            style.border.left
        );
    }
}

#[test]
fn xlsx_dxf_list_with_thousands_of_borders_reads_up_to_the_rule_bound() {
    let rules = SheetRules {
        cf: vec![rule(
            "A1",
            1,
            CfStyle {
                bg: Some(0xFFC7CE),
                ..CfStyle::default()
            },
        )],
        ..SheetRules::default()
    };
    let bytes = xlsx_with(rules);
    let one = "<dxf><border><left style=\"thin\"><color rgb=\"FF000000\"/></left><right style=\"thin\"><color rgb=\"FF000000\"/></right></border></dxf>";
    let patched = with_part(&bytes, "xl/styles.xml", |xml| {
        xml.replacen("</dxfs>", &format!("{}</dxfs>", one.repeat(20_000)), 1)
    });
    let got = read_xlsx(&patched);
    assert_eq!(got.cf.len(), 1);
    assert!(got.cf.len() <= MAX_RULES_PER_SHEET);
}

#[test]
fn ods_border_values_over_a_bound_or_made_up_are_ignored() {
    let rules = SheetRules {
        cf: vec![rule(
            "A1",
            1,
            CfStyle {
                bg: Some(0xFFC7CE),
                border: outline(BorderLine::Thin, 0xFF0000),
                ..CfStyle::default()
            },
        )],
        ..SheetRules::default()
    };
    let bytes = ods_with(rules);
    let huge = "9".repeat(300_000);
    let cases: [(String, BorderEdge); 5] = [
        ("0.74pt wavy #ff0000".to_string(), BorderEdge::NONE),
        (format!("{huge}pt solid #ff0000"), BorderEdge::NONE),
        (format!("0.74pt solid #ff0000 {huge}"), BorderEdge::NONE),
        ("0.74pt solid #ff00".to_string(), edge(BorderLine::Thin, 0)),
        ("none".to_string(), BorderEdge::NONE),
    ];
    for (value, want) in cases {
        let patched = with_part(&bytes, "content.xml", |xml| {
            xml.replace(
                "fo:border-top=\"0.74pt solid #ff0000\"",
                &format!("fo:border-top=\"{value}\""),
            )
        });
        let got = read_ods(&patched);
        let style = got.cf[0].style;
        assert_eq!(style.bg, Some(0xFFC7CE));
        assert_eq!(style.border.top, want, "{value:.40}");
        assert_eq!(style.border.left, edge(BorderLine::Thin, 0xFF0000));
    }
}

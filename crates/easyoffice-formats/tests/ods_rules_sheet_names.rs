//! A sheet name with XML metacharacters stays well-formed in an ODS that carries rules (external review M-15).

use easyoffice_formats::{
    parse_sqref, CfKind, CfRule, CfStyle, Cmp, DocumentFormat, OdfCodec, SheetRules, SheetStyling,
    ZipArchiveCodec, ZipSecurityPolicy, FLAG_BOLD,
};

const NAMES: [&str; 4] = ["A&B", "큰\"따옴표\"", "<태그>", "It's & \"both\" <1>"];

fn styled_with_rule() -> SheetStyling {
    let style = CfStyle {
        bg: Some(0xFFC7CE),
        fg: Some(0x9C0006),
        flags: FLAG_BOLD,
    };
    let cf = CfRule {
        ranges: parse_sqref("A1:A3"),
        priority: 1,
        stop_if_true: false,
        kind: CfKind::CellIs {
            cmp: Cmp::Greater,
            a: "1".into(),
            b: String::new(),
        },
        style,
    };
    SheetStyling {
        rules: SheetRules {
            cf: vec![cf],
            ..SheetRules::default()
        },
        ..SheetStyling::default()
    }
}

#[test]
fn a_sheet_name_with_markup_characters_keeps_the_content_well_formed() {
    for name in NAMES {
        let sheets = vec![(name.to_string(), vec![(0, 0, "5".to_string())])];
        let bytes = OdfCodec::pack_ods_multi_styled(&sheets, &[styled_with_rule()])
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
        let content = entries.iter().find(|e| e.path == "content.xml").unwrap();
        let xml = String::from_utf8(content.data.clone()).unwrap();
        let base =
            easyoffice_formats::XmlSanitizer::escape(&format!("'{}'.A1", name.replace('\'', "''")));
        assert!(
            xml.contains(&format!("calcext:base-cell-address=\"{base}\"")),
            "{name}: the base cell address is not escaped"
        );
        easyoffice_formats::XmlSanitizer::validate_safe_xml(&xml)
            .unwrap_or_else(|e| panic!("{name}: content.xml is not well-formed: {e:?}"));
        let back = easyoffice_formats::read_sheet_styling(&bytes, DocumentFormat::Ods)
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_eq!(back[0].0, name);
        assert_eq!(back[0].1.rules.cf.len(), 1, "{name}");
    }
}

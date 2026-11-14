//! A table's custom style (`tableStyles` with its differential formats) survives an XLSX round trip and a
//! hostile `tableStyles` is read to a bound (PD-231).

use easyoffice_formats::{
    CellBox, OoxmlCodec, SheetStyling, SheetTable, StyleElement, StyleFormat, TableEdge, TableLine,
    TableStyleDef, ZipArchiveCodec, ZipSecurityPolicy, MAX_TABLE_STYLES,
};

fn cells() -> Vec<(u32, u32, String)> {
    ["Region", "Q1"]
        .iter()
        .enumerate()
        .map(|(i, n)| (0, i as u32, (*n).to_string()))
        .collect()
}

fn custom() -> TableStyleDef {
    TableStyleDef {
        elements: vec![
            (
                StyleElement::Header,
                StyleFormat {
                    fill: Some(0x1F4E78),
                    color: Some(0xFFFFFF),
                    bold: true,
                    stripe: 1,
                    ..StyleFormat::default()
                },
            ),
            (
                StyleElement::RowStripe1,
                StyleFormat {
                    fill: Some(0xDDEBF7),
                    stripe: 2,
                    edges: [
                        None,
                        Some(TableEdge {
                            line: TableLine::Medium,
                            color: 0x1F4E78,
                        }),
                        None,
                        None,
                        None,
                        None,
                    ],
                    ..StyleFormat::default()
                },
            ),
        ],
    }
}

fn table(name: &str, style: &str, range: &str) -> SheetTable {
    let mut t = SheetTable::new(name, CellBox::parse(range).unwrap());
    t.columns = vec!["Region".into(), "Q1".into()];
    t.style = Some(style.into());
    t.style_def = Some(custom());
    t
}

fn pack(tables: Vec<SheetTable>) -> Vec<u8> {
    let styling = SheetStyling {
        tables,
        ..SheetStyling::default()
    };
    OoxmlCodec::pack_xlsx_multi_styled(&[("S".to_string(), cells())], &[], &[styling]).unwrap()
}

fn part(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    String::from_utf8(
        entries
            .iter()
            .find(|e| e.path == path)
            .unwrap()
            .data
            .clone(),
    )
    .unwrap()
}

#[test]
fn a_custom_style_is_written_once_for_two_tables_and_read_back() {
    let bytes = pack(vec![
        table("A", "Mine", "A1:B4"),
        table("B", "Mine", "D1:E4"),
    ]);
    let styles = part(&bytes, "xl/styles.xml");
    assert_eq!(styles.matches("<tableStyle ").count(), 1, "{styles}");
    assert!(styles.contains("name=\"Mine\"") && styles.contains("size=\"2\""));
    assert!(styles.find("</dxfs>").unwrap() < styles.find("<tableStyles").unwrap());
    let back = OoxmlCodec::unpack_xlsx_with_styling(&bytes).unwrap().1;
    let tables = &back[0].1.tables;
    assert_eq!(tables.len(), 2);
    for t in tables {
        assert_eq!(t.style.as_deref(), Some("Mine"));
        let def = t.style_def.as_ref().expect("the definition is read");
        let header = def
            .elements
            .iter()
            .find(|(e, _)| *e == StyleElement::Header)
            .unwrap();
        assert_eq!(
            (header.1.fill, header.1.color, header.1.bold),
            (Some(0x1F4E78), Some(0xFFFFFF), true)
        );
        let stripe = def
            .elements
            .iter()
            .find(|(e, _)| *e == StyleElement::RowStripe1)
            .unwrap();
        assert_eq!(stripe.1.stripe, 2);
        assert_eq!(stripe.1.edges[1].map(|e| e.line), Some(TableLine::Medium));
    }
}

#[test]
fn a_built_in_style_writes_no_definition() {
    let mut t = table("A", "TableStyleMedium2", "A1:B4");
    t.style_def = None;
    let styles = part(&pack(vec![t]), "xl/styles.xml");
    assert!(!styles.contains("<tableStyles"), "{styles}");
}

#[test]
fn a_flood_of_table_styles_in_a_file_is_cut_at_the_bound() {
    let bytes = pack(vec![table("A", "Mine", "A1:B4")]);
    let flood = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let parts: Vec<(String, Vec<u8>)> = flood
        .into_iter()
        .map(|e| {
            let mut text = String::from_utf8_lossy(&e.data).into_owned();
            if e.path == "xl/styles.xml" {
                let many: String = (0..MAX_TABLE_STYLES * 3)
                    .map(|n| format!("<tableStyle name=\"F{n}\"><tableStyleElement type=\"wholeTable\" dxfId=\"0\"/></tableStyle>"))
                    .collect();
                text = text.replace("</tableStyles>", &format!("{many}</tableStyles>"));
            }
            (e.path, text.into_bytes())
        })
        .collect();
    let bytes = ZipArchiveCodec::pack(&parts).unwrap();
    let back = OoxmlCodec::unpack_xlsx_with_styling(&bytes).unwrap().1;
    assert!(
        back[0].1.tables[0].style_def.is_some(),
        "the table's own style is first in the file"
    );
}

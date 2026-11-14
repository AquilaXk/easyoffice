//! The look a cell with a hyperlink is saved with (PD-223): the spreadsheet program's "Hyperlink" cell style in
//! XLSX (`cellStyles` with `builtinId` 8, its font in blue with an underline) and its named text styles in ODS.
//! Reading takes the style back as nothing but the link.

use easyoffice_formats::{
    CellLink, CellStyle, OdfCodec, OdsUnpacked, OoxmlCodec, SheetLinks, SheetStyling,
    ZipArchiveCodec, ZipSecurityPolicy,
};

fn entry(bytes: &[u8], path: &str) -> Option<String> {
    ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
        .unwrap()
        .into_iter()
        .find(|e| e.path == path)
        .map(|e| String::from_utf8(e.data).unwrap())
}

fn sheets() -> Vec<OdsUnpacked> {
    vec![(
        "Sheet1".to_string(),
        vec![
            (0, 0, "링크".to_string()),
            (1, 0, "굵은 링크".to_string()),
            (2, 0, "그냥 글자".to_string()),
            (3, 0, "밑줄 글자".to_string()),
        ],
    )]
}

fn styling() -> Vec<SheetStyling> {
    let mut st = SheetStyling::default();
    for (row, link) in [
        (0, "https://example.com/"),
        (1, "#Sheet1!A1"),
        (4, "mailto:a@b.c"),
    ] {
        st.links.insert((row, 0), CellLink::new(link, "").unwrap());
    }
    st.cells.insert(
        (1, 0),
        CellStyle {
            bold: true,
            ..CellStyle::default()
        },
    );
    st.cells.insert(
        (3, 0),
        CellStyle {
            underline: true,
            ..CellStyle::default()
        },
    );
    vec![st]
}

fn cell_tag<'a>(sheet: &'a str, at: &str) -> &'a str {
    let start = sheet.find(&format!("<c r=\"{at}\"")).unwrap();
    &sheet[start..start + sheet[start..].find('>').unwrap()]
}

#[test]
fn xlsx_writes_the_hyperlink_cell_style_for_every_linked_cell() {
    let bytes = OoxmlCodec::pack_xlsx_multi_styled(&sheets(), &[], &styling()).unwrap();
    let styles = entry(&bytes, "xl/styles.xml").unwrap();
    assert!(styles.contains("<cellStyleXfs count=\"2\">"), "{styles}");
    assert!(styles.contains("<cellStyle name=\"Hyperlink\" xfId=\"1\" builtinId=\"8\"/>"));
    assert!(styles.contains("<u/><sz val=\"11\"/><color rgb=\"FF0563C1\"/>"));
    // Bold and underline stay on the font of a bold link and the link adds its colour.
    assert!(styles.contains("<b/><u/><sz val=\"11\"/><color rgb=\"FF0563C1\"/>"));

    let sheet = entry(&bytes, "xl/worksheets/sheet1.xml").unwrap();
    let xf_of = |at: &str| {
        let tag = cell_tag(&sheet, at);
        let s = tag
            .split("s=\"")
            .nth(1)
            .map(|t| t.split('"').next().unwrap());
        s.map_or(0, |s| s.parse::<usize>().unwrap())
    };
    let cell_xfs = &styles[styles.find("<cellXfs").unwrap()..styles.find("</cellXfs>").unwrap()];
    let xfs: Vec<&str> = cell_xfs.split("<xf ").skip(1).collect();
    for at in ["A1", "A2", "A5"] {
        assert!(
            xfs[xf_of(at)].contains("xfId=\"1\""),
            "{at}: {}",
            xfs[xf_of(at)]
        );
    }
    for at in ["A3", "A4"] {
        assert!(!xfs[xf_of(at)].contains("xfId=\"1\""), "{at}");
    }
    // A blank linked cell is written with the look too.
    assert!(
        cell_tag(&sheet, "A5").contains(" s=\""),
        "{}",
        cell_tag(&sheet, "A5")
    );
}

#[test]
fn a_workbook_without_links_has_no_hyperlink_style() {
    let plain = vec![SheetStyling::default()];
    let bytes = OoxmlCodec::pack_xlsx_multi_styled(&sheets(), &[], &plain).unwrap();
    let styles = entry(&bytes, "xl/styles.xml").unwrap();
    assert!(styles.contains("<cellStyleXfs count=\"1\">") && !styles.contains("Hyperlink"));
}

#[test]
fn xlsx_reads_the_style_back_as_the_link_alone() {
    let bytes = OoxmlCodec::pack_xlsx_multi_styled(&sheets(), &[], &styling()).unwrap();
    let back = &OoxmlCodec::unpack_xlsx_styling(&bytes).unwrap()[0].1;
    assert_eq!(back.links, styling()[0].links);
    // The plain link cells and the blank one carry no format of their own; the bold link is only bold; an
    // underline the user chose on a cell without a link stays.
    assert_eq!(back.cells.get(&(0, 0)), None);
    assert_eq!(back.cells.get(&(4, 0)), None);
    let bold = back.cells[&(1, 0)];
    assert!(bold.bold && !bold.underline);
    assert!(back.cells[&(3, 0)].underline);
    // The next save writes the same look.
    let again = OoxmlCodec::pack_xlsx_multi_styled(&sheets(), &[], &[back.clone()]).unwrap();
    assert_eq!(
        entry(&again, "xl/styles.xml"),
        entry(&bytes, "xl/styles.xml")
    );
    assert_eq!(
        entry(&again, "xl/worksheets/sheet1.xml"),
        entry(&bytes, "xl/worksheets/sheet1.xml")
    );
}

#[test]
fn a_file_the_spreadsheet_program_made_reads_the_same_way() {
    let bytes = OoxmlCodec::pack_xlsx_multi_styled(&sheets(), &[], &styling()).unwrap();
    let mut entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    for e in &mut entries {
        if e.path == "xl/styles.xml" {
            // Excel names the style by its builtin id and may call it anything; the font has a theme colour.
            let text = String::from_utf8(e.data.clone()).unwrap();
            e.data = text
                .replace("name=\"Hyperlink\"", "name=\"하이퍼링크\"")
                .replace("<color rgb=\"FF0563C1\"/>", "<color theme=\"10\"/>")
                .into_bytes();
        }
    }
    let foreign = ZipArchiveCodec::pack_compressed(&entries).unwrap();
    let back = &OoxmlCodec::unpack_xlsx_styling(&foreign).unwrap()[0].1;
    assert_eq!(back.cells.get(&(0, 0)), None);
    assert!(!back.cells[&(1, 0)].underline);
}

#[test]
fn ods_names_the_link_text_styles_in_the_common_styles() {
    let bytes = OdfCodec::pack_ods_multi_styled(&sheets(), &styling()).unwrap();
    let styles = entry(&bytes, "styles.xml").unwrap();
    assert!(
        styles.contains("style:name=\"Internet_20_link\"")
            && styles.contains("fo:color=\"#0563c1\"")
    );
    assert!(
        styles.find("<office:styles>").unwrap() < styles.find("<office:automatic-styles>").unwrap()
    );
    let manifest = entry(&bytes, "META-INF/manifest.xml").unwrap();
    assert!(manifest.contains("styles.xml"));
    let content = entry(&bytes, "content.xml").unwrap();
    assert!(content.contains("text:style-name=\"Internet_20_link\" text:visited-style-name=\"Visited_20_Internet_20_Link\""));
    let (_, back) = OdfCodec::unpack_ods_with_styling(&bytes).unwrap();
    assert_eq!(back[0].1.links, styling()[0].links);
    let no_links = OdfCodec::pack_ods_multi_styled(&sheets(), &[SheetStyling::default()]).unwrap();
    assert!(entry(&no_links, "styles.xml").is_none());
}

#[test]
fn a_link_with_no_cells_to_carry_it_is_not_needed() {
    let mut st = SheetStyling::default();
    st.links
        .insert((0, 0), CellLink::new("https://a.b/", "").unwrap());
    let bytes = OoxmlCodec::pack_xlsx_multi_styled(&sheets(), &[], &[st]).unwrap();
    let back: SheetLinks = OoxmlCodec::unpack_xlsx_styling(&bytes).unwrap()[0]
        .1
        .links
        .clone();
    assert_eq!(back.len(), 1);
}

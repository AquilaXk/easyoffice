//! Fill and font colours of cells survive XLSX/ODS save -> reopen, are shared across cells, resolve
//! theme, tint and palette numbers, and stay bounded on hostile stylesheets (PD-227).

use easyoffice_formats::{
    read_sheet_styling, CellStyle, DocumentFormat, OdfCodec, OdsUnpacked, OoxmlCodec, SheetStyling,
    ZipArchiveCodec, ZipSecurityPolicy,
};

fn sheet() -> Vec<OdsUnpacked> {
    vec![(
        "Sheet1".to_string(),
        vec![(0, 0, "가".to_string()), (1, 1, "나".to_string())],
    )]
}

fn pack(format: DocumentFormat, styling: &SheetStyling) -> Vec<u8> {
    let styles = std::slice::from_ref(styling);
    match format {
        DocumentFormat::Xlsx => OoxmlCodec::pack_xlsx_multi_styled(&sheet(), &[], styles).unwrap(),
        _ => OdfCodec::pack_ods_multi_styled(&sheet(), styles).unwrap(),
    }
}

fn part(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let entry = entries.iter().find(|e| e.path == path).unwrap();
    String::from_utf8(entry.data.clone()).unwrap()
}

/// The package with `path` replaced by `text` (added when the package has no such part).
fn swap(bytes: &[u8], path: &str, text: &str) -> Vec<u8> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let mut seen = false;
    let mut parts: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            if e.path == path {
                seen = true;
                (e.path, text.as_bytes().to_vec())
            } else {
                (e.path, e.data)
            }
        })
        .collect();
    if !seen {
        parts.push((path.to_string(), text.as_bytes().to_vec()));
    }
    ZipArchiveCodec::pack_compressed(&parts).unwrap()
}

fn with(cells: Vec<((u32, u32), CellStyle)>) -> SheetStyling {
    SheetStyling {
        cells: cells.into_iter().collect(),
        ..Default::default()
    }
}

fn read(bytes: &[u8], format: DocumentFormat) -> SheetStyling {
    read_sheet_styling(bytes, format).unwrap().remove(0).1
}

fn round_trip(format: DocumentFormat, styling: &SheetStyling) -> SheetStyling {
    read(&pack(format, styling), format)
}

fn colored(fill: Option<u32>, font_color: Option<u32>) -> CellStyle {
    CellStyle {
        fill,
        font_color,
        ..Default::default()
    }
}

/// A stylesheet with the fonts, fills and cell formats given (the fonts and fills as inner XML).
fn stylesheet(fonts: &str, fills: &str, xfs: &str, extra: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?><styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">{extra}<fonts>{fonts}</fonts><fills>{fills}</fills><borders><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs><cellXfs>{xfs}</cellXfs></styleSheet>"
    )
}

/// The styling of a workbook whose stylesheet is `styles` and whose cell in row `i` (zero-based) uses
/// cell format `i`, for `count` formats.
fn foreign(styles: &str, count: usize) -> SheetStyling {
    let bytes = pack(
        DocumentFormat::Xlsx,
        &with(vec![((0, 0), colored(Some(0x112233), None))]),
    );
    let mut rows = String::new();
    for i in 0..count {
        rows.push_str(&format!(
            "<row r=\"{}\"><c r=\"A{}\" s=\"{i}\" t=\"inlineStr\"><is><t>x</t></is></c></row>",
            i + 1,
            i + 1
        ));
    }
    let sheet = format!(
        "<?xml version=\"1.0\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>{rows}</sheetData></worksheet>"
    );
    let bytes = swap(&bytes, "xl/styles.xml", styles);
    read(
        &swap(&bytes, "xl/worksheets/sheet1.xml", &sheet),
        DocumentFormat::Xlsx,
    )
}

const NO_FILLS: &str = "<fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill>";

#[test]
fn fill_and_font_colours_round_trip_through_xlsx() {
    let styling = with(vec![
        ((0, 0), colored(Some(0xFFEB9C), None)),
        ((0, 1), colored(None, Some(0x9C5700))),
        ((1, 1), colored(Some(0x00B050), Some(0xFFFFFF))),
        (
            (2, 2),
            CellStyle {
                bold: true,
                fill: Some(0x000000),
                font_color: Some(0x000000),
                ..Default::default()
            },
        ),
    ]);
    let back = round_trip(DocumentFormat::Xlsx, &styling);
    assert_eq!(back.cells, styling.cells);
}

#[test]
fn the_writer_keeps_the_two_leading_fills_and_shares_equal_colours() {
    let mut cells = Vec::new();
    for row in 0..1000 {
        let style = if row % 2 == 0 {
            colored(Some(0xFF0000), Some(0x0000FF))
        } else {
            colored(Some(0xFF0000), None)
        };
        cells.push(((row, 0), style));
    }
    let bytes = pack(DocumentFormat::Xlsx, &with(cells));
    let styles = part(&bytes, "xl/styles.xml");
    assert!(styles.contains("<fills count=\"3\">"), "{styles}");
    assert!(styles.contains("<fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill><fill><patternFill patternType=\"solid\"><fgColor rgb=\"FFFF0000\"/>"));
    assert!(
        styles.contains("<cellXfs count=\"3\">"),
        "two looks, one default"
    );
    assert!(
        styles.contains("<fonts count=\"2\">"),
        "the default font and one coloured font"
    );
    assert!(styles.contains("<color rgb=\"FF0000FF\"/>"));
    assert!(styles.contains("fillId=\"2\""));
    assert!(styles.contains("applyFill=\"1\""));
}

#[test]
fn a_file_with_duplicate_formats_does_not_grow_when_saved() {
    let mut xfs = String::new();
    for _ in 0..500 {
        xfs.push_str("<xf numFmtId=\"0\" fontId=\"1\" fillId=\"2\" borderId=\"0\" xfId=\"0\"/>");
        xfs.push_str("<xf numFmtId=\"0\" fontId=\"2\" fillId=\"3\" borderId=\"0\" xfId=\"0\"/>");
    }
    let fonts = "<font><sz val=\"11\"/><color theme=\"1\"/></font><font><sz val=\"11\"/><color rgb=\"FFFF0000\"/></font><font><sz val=\"11\"/><color rgb=\"FFFF0000\"/></font>";
    let fills = format!("{NO_FILLS}<fill><patternFill patternType=\"solid\"><fgColor rgb=\"FF00FF00\"/></patternFill></fill><fill><patternFill patternType=\"solid\"><fgColor rgb=\"FF00FF00\"/></patternFill></fill>");
    let styles = stylesheet(fonts, &fills, &xfs, "");
    let bytes = pack(
        DocumentFormat::Xlsx,
        &with(vec![((0, 0), colored(Some(1), None))]),
    );
    let opened = read(
        &swap(&bytes, "xl/styles.xml", &styles),
        DocumentFormat::Xlsx,
    );
    let saved = pack(DocumentFormat::Xlsx, &opened);
    let out = part(&saved, "xl/styles.xml");
    assert!(out.contains("<cellXfs count=\"2\">"), "{out}");
    assert!(out.contains("<fills count=\"3\">"));
    assert!(out.contains("<fonts count=\"2\">"));
}

#[test]
fn theme_tint_indexed_and_custom_palette_colours_read_as_rgb() {
    let fonts = concat!(
        "<font><sz val=\"11\"/><color theme=\"1\"/></font>",
        "<font><color theme=\"4\"/></font>",
        "<font><color indexed=\"10\"/></font>",
        "<font><color indexed=\"12\"/></font>",
        "<font><color auto=\"1\"/></font>",
        "<font><color rgb=\"FF000000\"/></font>"
    );
    let fills = format!(
        "{NO_FILLS}{}",
        concat!(
            "<fill><patternFill patternType=\"solid\"><fgColor theme=\"5\" tint=\"0.5\"/><bgColor indexed=\"64\"/></patternFill></fill>",
            "<fill><patternFill patternType=\"solid\"><fgColor indexed=\"13\"/></patternFill></fill>",
            "<fill><patternFill patternType=\"solid\"><bgColor rgb=\"FF010203\"/></patternFill></fill>",
            "<fill><patternFill patternType=\"darkGray\"><fgColor rgb=\"FF111111\"/></patternFill></fill>",
            "<fill><gradientFill><stop position=\"0\"><color rgb=\"FF222222\"/></stop></gradientFill></fill>",
        )
    );
    let xfs = concat!(
        "<xf fontId=\"0\" fillId=\"0\"/>",
        "<xf fontId=\"1\" fillId=\"2\"/>",
        "<xf fontId=\"2\" fillId=\"3\"/>",
        "<xf fontId=\"3\" fillId=\"4\"/>",
        "<xf fontId=\"4\" fillId=\"5\"/>",
        "<xf fontId=\"5\" fillId=\"6\"/>",
        "<xf fontId=\"1\" fillId=\"99\"/>"
    );
    let mut palette = String::from("<colors><indexedColors>");
    for at in 0..14 {
        palette.push_str(if at == 13 {
            "<rgbColor rgb=\"FFABCDEF\"/>"
        } else if at == 10 {
            "<rgbColor rgb=\"FFFF0000\"/>"
        } else {
            "<rgbColor rgb=\"FF000000\"/>"
        });
    }
    palette.push_str("</indexedColors></colors>");
    let styles = stylesheet(fonts, &fills, xfs, "")
        .replace("</styleSheet>", &format!("{palette}</styleSheet>"));
    let styling = foreign(&styles, 7);
    let at = |xf: u32| styling.cells.get(&(xf, 0)).copied().unwrap_or_default();
    assert_eq!(
        at(0),
        CellStyle::default(),
        "the default font's colour is no colour"
    );
    assert_eq!(at(1).font_color, Some(0x4472C4), "theme accent 1");
    let light = at(1).fill.expect("theme accent 2 lightened");
    assert!(
        light > 0xED7D31 && (light >> 16) & 0xFF >= 0xED,
        "{light:06X}"
    );
    assert_eq!(
        at(2).font_color,
        Some(0xFF0000),
        "the workbook's palette says entry 10"
    );
    assert_eq!(
        at(2).fill,
        Some(0xABCDEF),
        "the workbook's palette overrides entry 13"
    );
    assert_eq!(
        at(3).font_color,
        None,
        "an override that makes it black is the default colour"
    );
    assert_eq!(
        at(3).fill,
        Some(0x010203),
        "a solid fill with only a background colour"
    );
    assert_eq!(
        at(4),
        CellStyle::default(),
        "auto and a pattern other than solid"
    );
    assert_eq!(at(5), CellStyle::default(), "explicit black and a gradient");
    assert_eq!(at(6).fill, None, "a fill id past the list");
    assert_eq!(at(6).font_color, Some(0x4472C4));
}

#[test]
fn a_custom_theme_colours_fonts_and_fills() {
    let theme = "<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><a:themeElements><a:clrScheme name=\"x\"><a:dk1><a:srgbClr val=\"101010\"/></a:dk1><a:lt1><a:srgbClr val=\"F0F0F0\"/></a:lt1><a:dk2><a:srgbClr val=\"222222\"/></a:dk2><a:lt2><a:srgbClr val=\"EEEEEE\"/></a:lt2><a:accent1><a:srgbClr val=\"A1B2C3\"/></a:accent1><a:accent2><a:srgbClr val=\"111111\"/></a:accent2><a:accent3><a:srgbClr val=\"222222\"/></a:accent3><a:accent4><a:srgbClr val=\"333333\"/></a:accent4><a:accent5><a:srgbClr val=\"444444\"/></a:accent5><a:accent6><a:srgbClr val=\"555555\"/></a:accent6><a:hlink><a:srgbClr val=\"123456\"/></a:hlink><a:folHlink><a:srgbClr val=\"654321\"/></a:folHlink></a:clrScheme></a:themeElements></a:theme>";
    let fonts =
        "<font><sz val=\"11\"/><color theme=\"1\"/></font><font><color theme=\"4\"/></font>";
    let fills = format!("{NO_FILLS}<fill><patternFill patternType=\"solid\"><fgColor theme=\"0\"/></patternFill></fill>");
    let styles = stylesheet(
        fonts,
        &fills,
        "<xf fontId=\"0\" fillId=\"0\"/><xf fontId=\"1\" fillId=\"2\"/>",
        "",
    );
    let bytes = pack(
        DocumentFormat::Xlsx,
        &with(vec![((0, 0), colored(Some(1), None))]),
    );
    let bytes = swap(&bytes, "xl/styles.xml", &styles);
    let bytes = swap(&bytes, "xl/theme/theme1.xml", theme);
    let sheet = "<?xml version=\"1.0\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData><row r=\"1\"><c r=\"A1\" s=\"1\" t=\"inlineStr\"><is><t>x</t></is></c></row></sheetData></worksheet>";
    let styling = read(
        &swap(&bytes, "xl/worksheets/sheet1.xml", sheet),
        DocumentFormat::Xlsx,
    );
    let style = styling.cells[&(0, 0)];
    assert_eq!(
        style.font_color,
        Some(0xA1B2C3),
        "theme 4 is accent 1 of the file's own theme"
    );
    assert_eq!(
        style.fill,
        Some(0xF0F0F0),
        "theme 0 is the light colour slot of the file"
    );
}

#[test]
fn a_link_cell_keeps_a_colour_of_its_own_and_loses_the_link_colour() {
    use easyoffice_formats::CellLink;
    let mut styling = with(vec![
        ((0, 0), colored(Some(0xFFFF00), Some(0xC00000))),
        ((1, 0), colored(Some(0xFFFF00), None)),
    ]);
    for row in 0..3 {
        styling
            .links
            .insert((row, 0), CellLink::new("https://example.com/", "").unwrap());
    }
    let sheets = vec![(
        "Sheet1".to_string(),
        vec![
            (0, 0, "a".to_string()),
            (1, 0, "b".to_string()),
            (2, 0, "c".to_string()),
        ],
    )];
    let bytes =
        OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], std::slice::from_ref(&styling)).unwrap();
    let styles = part(&bytes, "xl/styles.xml");
    assert!(
        styles.contains("<u/><sz val=\"11\"/><color rgb=\"FFC00000\"/>"),
        "{styles}"
    );
    let back = &OoxmlCodec::unpack_xlsx_styling(&bytes).unwrap()[0].1;
    assert_eq!(back.cells[&(0, 0)].font_color, Some(0xC00000));
    assert_eq!(back.cells[&(0, 0)].fill, Some(0xFFFF00));
    assert_eq!(
        back.cells[&(1, 0)].font_color,
        None,
        "the hyperlink colour comes from the style"
    );
    assert_eq!(back.cells[&(1, 0)].fill, Some(0xFFFF00));
    assert_eq!(back.cells.get(&(2, 0)), None);
}

#[test]
fn hostile_stylesheets_stay_bounded_and_never_panic() {
    // More fills, fonts and cell formats than a reader keeps: the ones past the limit read as nothing.
    let count = 65_536 + 10;
    let mut fonts = String::from("<font><sz val=\"11\"/></font>");
    let mut fills = String::from(NO_FILLS);
    let mut xfs = String::new();
    for _ in 0..count {
        fonts.push_str("<font><color rgb=\"FF010203\"/></font>");
        fills.push_str("<fill><patternFill patternType=\"solid\"><fgColor rgb=\"FF040506\"/></patternFill></fill>");
    }
    xfs.push_str("<xf fontId=\"1\" fillId=\"2\"/>");
    xfs.push_str(&format!(
        "<xf fontId=\"{}\" fillId=\"{}\"/>",
        count - 1,
        count - 1
    ));
    xfs.push_str(&format!(
        "<xf fontId=\"{}\" fillId=\"{}\"/>",
        count + 5,
        count + 5
    ));
    let styling = foreign(&stylesheet(&fonts, &fills, &xfs, ""), 3);
    assert_eq!(styling.cells[&(0, 0)].fill, Some(0x040506));
    assert_eq!(styling.cells[&(0, 0)].font_color, Some(0x010203));
    assert_eq!(
        styling.cells.get(&(1, 0)),
        None,
        "an entry past the limit is nothing"
    );
    assert_eq!(styling.cells.get(&(2, 0)), None);

    // Impossible numbers read as no colour.
    let fonts = "<font><sz val=\"11\"/></font><font><color theme=\"999999999999\"/></font><font><color indexed=\"4000000000\"/></font><font><color rgb=\"ééééé\"/></font><font><color theme=\"2\" tint=\"-7\"/></font>";
    let fills = format!("{NO_FILLS}<fill><patternFill patternType=\"solid\"><fgColor theme=\"-3\"/><bgColor indexed=\"-1\"/></patternFill></fill>");
    let xfs = "<xf fontId=\"1\" fillId=\"2\"/><xf fontId=\"2\" fillId=\"0\"/><xf fontId=\"3\" fillId=\"0\"/><xf fontId=\"4\" fillId=\"0\"/>";
    let styling = foreign(&stylesheet(fonts, &fills, xfs, ""), 4);
    assert!(styling.cells.is_empty(), "{:?}", styling.cells);

    // A multi-byte `rgb` of border length cannot split a character.
    let borders = "<borders><border><left style=\"thin\"><color rgb=\"aéxxxxx\"/></left><right/><top/><bottom/><diagonal/></border></borders>";
    let styles = stylesheet(
        "<font><sz val=\"11\"/></font>",
        NO_FILLS,
        "<xf fontId=\"0\" fillId=\"0\" borderId=\"0\"/>",
        "",
    )
    .replace(
        "<borders><border><left/><right/><top/><bottom/><diagonal/></border></borders>",
        borders,
    );
    let styling = foreign(&styles, 1);
    assert_eq!(styling.cells[&(0, 0)].borders.left.rgb(), 0);
}

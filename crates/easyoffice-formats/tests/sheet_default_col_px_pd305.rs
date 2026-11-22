//! The width of a column that has none of its own is one rule for the page, the positions of pictures and charts and the
//! PDF (PD-305): `defaultColWidth` first, then `baseColWidth` (8 when missing) with the padding rounded up to 8 pixels, in
//! digits of the workbook's default font (7 px for Calibri 11, so a sheet that says nothing is 64 px; PD-306 makes the
//! standard font Malgun Gothic 11 pt, 8 px, 72 px, which `sheet_korean_defaults_pd306` covers). The rules below are read with
//! the Calibri 11 digit unless a test says otherwise. The widths a file writes down never change.

use std::collections::BTreeMap;

use easyoffice_formats::{
    encode_sheet_pdf, read_sheet_styling, CellStyle, ChartAnchor, ChartKind, ChartLegendPos,
    ChartSeriesRef, DefaultColWidth, DefaultFont, DocumentFormat, OdfCodec, OdsUnpacked,
    OoxmlCodec, SheetChart, SheetGrid, SheetStyling, ZipArchiveCodec, ZipSecurityPolicy,
    DEFAULT_COL_PX, MAX_DEFAULT_COL_PX,
};

fn cell() -> OdsUnpacked {
    ("Sheet1".to_string(), vec![(0, 0, "x".to_string())])
}

const SHEET_HEAD: &str = r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#;
const SHEET_TAIL: &str = r#"<sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>x</t></is></c></row></sheetData></worksheet>"#;

/// A file whose sheet and styles are the ones given (`styles` replaces `xl/styles.xml` when it is not empty).
fn xlsx_with(styles: &str, sheet: &str) -> Vec<u8> {
    let base = OoxmlCodec::pack_xlsx_multi(&[cell()]).unwrap();
    let entries = ZipArchiveCodec::unpack(&base, &ZipSecurityPolicy::default()).unwrap();
    let replaced: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| match e.path.as_str() {
            "xl/worksheets/sheet1.xml" => (e.path, sheet.as_bytes().to_vec()),
            "xl/styles.xml" if !styles.is_empty() => (e.path, styles.as_bytes().to_vec()),
            _ => (e.path, e.data),
        })
        .collect();
    ZipArchiveCodec::pack_compressed(&replaced).unwrap()
}

fn format_pr(attrs: &str) -> String {
    format!(r#"{SHEET_HEAD}<sheetFormatPr {attrs}/>{SHEET_TAIL}"#)
}

/// A stylesheet whose default font (font 0) is `<sz val="{size}"/><name val="{name}"/>`.
fn styles_of(size: &str, name: &str) -> String {
    format!(
        concat!(
            r#"<?xml version="1.0"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#,
            r#"<fonts count="1"><font><sz val="{}"/><name val="{}"/></font></fonts>"#,
            r#"<cellXfs count="1"><xf fontId="0"/></cellXfs></styleSheet>"#
        ),
        size, name
    )
}

fn styling_of(bytes: &[u8]) -> SheetStyling {
    read_sheet_styling(bytes, DocumentFormat::Xlsx)
        .unwrap()
        .remove(0)
        .1
}

/// The default font of a Calibri 11 pt workbook, a 7 px digit.
fn calibri() -> Option<DefaultFont> {
    DefaultFont::kept(Some("Calibri"), Some(11.0))
}

fn saved(styling: SheetStyling) -> Vec<u8> {
    OoxmlCodec::pack_xlsx_multi_styled(&[cell()], &[], &[styling]).unwrap()
}

/// The sheet of a workbook whose default font is Calibri 11 pt (a 7 px digit).
fn calibri_xlsx(sheet: &str) -> Vec<u8> {
    xlsx_with(&styles_of("11", "Calibri"), sheet)
}

fn px_of(attrs: &str) -> u32 {
    styling_of(&calibri_xlsx(&format_pr(attrs))).default_col_px()
}

#[test]
fn a_new_sheet_and_a_sheet_that_says_nothing_are_the_default_of_the_digit() {
    assert_eq!(DEFAULT_COL_PX, 72);
    assert_eq!(SheetStyling::default().default_col_px(), 72);
    // A new workbook is written with the standard font: 8 px digit, 72 px, and the reader keeps no font for it.
    let new = styling_of(&OoxmlCodec::pack_xlsx_multi(&[cell()]).unwrap());
    assert_eq!((new.default_col_px(), new.default_col_width), (72, None));
    assert_eq!(new.default_font, None);
    let bare = xlsx_with("", &format!("{SHEET_HEAD}{SHEET_TAIL}"));
    assert_eq!(styling_of(&bare).default_col_px(), 72);
    // Calibri 11 pt, the first spreadsheet program's default: 7 px digit, 64 px.
    assert_eq!(px_of(r#"defaultRowHeight="15""#), 64);
}

#[test]
fn the_width_in_the_file_decides_in_the_order_of_the_rule() {
    // `defaultColWidth` first: the spec's own example is 8.7109375 characters of a 7 px digit, 61 px, not rounded up.
    assert_eq!(px_of(r#"defaultColWidth="8.7109375""#), 61);
    assert_eq!(
        px_of(r#"defaultColWidth="20.7109375" baseColWidth="10""#),
        145
    );
    assert_eq!(px_of(r#"defaultColWidth="9.140625""#), 64);
    // Then `baseColWidth`: the characters and the 5 px of padding, up to the next multiple of 8.
    assert_eq!(px_of(r#"baseColWidth="10""#), 80);
    assert_eq!(px_of(r#"baseColWidth="8""#), 64);
    assert_eq!(px_of(r#"baseColWidth="0""#), 8);
    // A value that is not a width does not count and the next rule decides.
    assert_eq!(px_of(r#"defaultColWidth="x" baseColWidth="10""#), 80);
    assert_eq!(px_of(r#"defaultColWidth="x" baseColWidth="y""#), 64);
}

#[test]
fn hostile_values_stay_in_range_and_cost_nothing() {
    for attrs in [
        r#"defaultColWidth="1e308""#,
        r#"defaultColWidth="NaN""#,
        r#"defaultColWidth="inf""#,
        r#"defaultColWidth="-1e308""#,
        r#"defaultColWidth="0""#,
        r#"defaultColWidth="255.0001""#,
        r#"baseColWidth="99999999999999999999""#,
        r#"baseColWidth="-1""#,
        r#"baseColWidth="256""#,
        r#"baseColWidth="4294967295""#,
    ] {
        assert_eq!(px_of(attrs), 64, "{attrs}");
    }
    // The widest width a sheet may say is 255 characters (of a 7 px digit: 1,785 px, and 1,792 px for the base with padding).
    assert_eq!(px_of(r#"defaultColWidth="255""#), 1_785);
    assert_eq!(px_of(r#"baseColWidth="255""#), 1_792);
    // Wider digits are held at the widest column a sheet may have.
    let wide = styling_of(&xlsx_with(
        &styles_of("12", "Arial"),
        &format_pr(r#"defaultColWidth="255""#),
    ));
    assert_eq!(wide.default_col_px(), MAX_DEFAULT_COL_PX);
}

#[test]
fn the_digit_of_the_default_font_is_the_unit_of_every_width_in_characters() {
    // Arial 12 pt: a 9 px digit. 8 characters and the padding are 77 px, up to 80; a width of 10 characters is 90 px.
    let styles = styles_of("12", "Arial");
    let sheet = format!(
        r#"{SHEET_HEAD}<cols><col min="2" max="2" width="10" customWidth="1"/></cols>{SHEET_TAIL}"#
    );
    let read = styling_of(&xlsx_with(&styles, &sheet));
    assert_eq!(read.default_col_px(), 80);
    assert_eq!(read.columns, BTreeMap::from([(1, 90)]));
    // A save writes the file's own default font again (PD-306), and the pixels stay what the file showed.
    let again = styling_of(&saved(read.clone()));
    assert_eq!(again.default_col_px(), 80);
    assert_eq!(again.columns, read.columns);
    assert_eq!(again.default_font, read.default_font);
    assert_eq!(again.default_font.as_ref().map(|f| f.name()), Some("Arial"));
    // A file that gives the width is read in its own digits, too: 20 characters of 9 px are 180 px.
    let given = format!(
        r#"{SHEET_HEAD}<sheetFormatPr defaultColWidth="20" baseColWidth="3"/>{SHEET_TAIL}"#
    );
    let read = styling_of(&xlsx_with(&styles, &given));
    assert_eq!(read.default_col_px(), 180);
    let again = styling_of(&saved(read.clone()));
    assert_eq!(again.default_col_px(), 180);
    assert_eq!(again.default_col_width, read.default_col_width);
    // The digit of Calibri 11 keeps the file's values as they are.
    let calibri = styling_of(&xlsx_with(&styles_of("11", "Calibri"), &given));
    assert_eq!(
        calibri.default_col_width,
        Some(DefaultColWidth {
            width: Some(20.0),
            base: Some(3)
        })
    );
}

/// The default font of the Korean edition (Malgun Gothic 11 pt): 8.38 characters of an 8 px digit, 72 px; a width in characters
/// of the file is in digits of that width.
#[test]
fn the_korean_default_font_has_an_8_px_digit() {
    let bare = format!("{SHEET_HEAD}{SHEET_TAIL}");
    for name in ["Malgun Gothic", "맑은 고딕"] {
        let read = styling_of(&xlsx_with(&styles_of("11", name), &bare));
        assert_eq!(read.default_col_px(), 72, "{name}");
    }
    let sheet = format!(
        r#"{SHEET_HEAD}<cols><col min="1" max="1" width="10" customWidth="1"/></cols>{SHEET_TAIL}"#
    );
    let read = styling_of(&xlsx_with(&styles_of("11", "맑은 고딕"), &sheet));
    assert_eq!(read.columns, BTreeMap::from([(0, 80)]));
    let again = styling_of(&saved(read.clone()));
    assert_eq!((again.default_col_px(), again.columns), (72, read.columns));
    assert_eq!(read.default_font, None); // the standard font is not kept: the writer says it
                                         // The face the table has no digit for (Aptos) is read as Calibri: no source gives its width.
    let aptos = styling_of(&xlsx_with(&styles_of("11", "Aptos Narrow"), &bare));
    assert_eq!(aptos.default_col_px(), 64);
}

/// The unit is the digit of the Normal style's font (`cellStyle` `builtinId` 0, its `cellStyleXfs` entry's `fontId`), which
/// is font 0 only when the file says so or says nothing that can be followed.
#[test]
fn the_normal_style_decides_the_font_of_the_unit() {
    let styles = |cell_styles: &str, style_xfs: &str| {
        format!(
            concat!(
                r#"<?xml version="1.0"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#,
                r#"<fonts count="2"><font><sz val="11"/><name val="Calibri"/></font>"#,
                r#"<font><sz val="12"/><name val="Arial"/></font></fonts>"#,
                r#"<cellStyleXfs count="2">{}</cellStyleXfs>"#,
                r#"<cellXfs count="1"><xf fontId="0"/></cellXfs><cellStyles count="1">{}</cellStyles></styleSheet>"#
            ),
            style_xfs, cell_styles
        )
    };
    let two = r#"<xf fontId="0"/><xf fontId="1"/>"#;
    let bare = format!("{SHEET_HEAD}{SHEET_TAIL}");
    let px = |cell_styles: &str, xfs: &str| {
        styling_of(&xlsx_with(&styles(cell_styles, xfs), &bare)).default_col_px()
    };
    // Normal is the second entry: Arial 12 pt, a 9 px digit, 80 px.
    assert_eq!(
        px(r#"<cellStyle name="Normal" xfId="1" builtinId="0"/>"#, two),
        80
    );
    // Another style that names the second entry is not Normal; font 0 is Calibri 11, 64 px.
    assert_eq!(
        px(
            r#"<cellStyle name="Hyperlink" xfId="1" builtinId="8"/>"#,
            two
        ),
        64
    );
    for (cell_styles, xfs) in [
        (r#"<cellStyle name="Normal" xfId="0" builtinId="0"/>"#, two),
        (
            r#"<cellStyle name="Normal" xfId="999" builtinId="0"/>"#,
            two,
        ),
        (r#"<cellStyle name="Normal" xfId="x" builtinId="0"/>"#, two),
        (
            r#"<cellStyle name="Normal" xfId="1" builtinId="0"/>"#,
            r#"<xf fontId="99"/><xf fontId="99"/>"#,
        ),
        (
            r#"<cellStyle name="Normal" xfId="1" builtinId="0"/>"#,
            r#"<xf fontId="-1"/><xf/>"#,
        ),
        (
            r#"<cellStyle name="Normal" builtinId="0"/>"#,
            r#"<xf fontId="0"/>"#,
        ),
        ("", two),
    ] {
        assert_eq!(px(cell_styles, xfs), 64, "{cell_styles} {xfs}");
    }
}

#[test]
fn a_default_font_that_makes_no_sense_is_the_calibri_digit() {
    for (size, name) in [
        ("NaN", "Calibri"),
        ("inf", "Arial"),
        ("-11", "Arial"),
        ("0", "Arial"),
        ("1e308", "Arial"),
        ("410", "Arial"),
        ("11", "A Face Nobody Has"),
        ("11", ""),
    ] {
        let sheet = format!(r#"{SHEET_HEAD}{SHEET_TAIL}"#);
        let px = styling_of(&xlsx_with(&styles_of(size, name), &sheet)).default_col_px();
        // A size that cannot be is 11 pt; a face that is not known is Calibri; both stay in range.
        let wanted = if name == "Arial" { 72 } else { 64 };
        assert_eq!(px, wanted, "{size} {name}");
    }
    // The largest size a font may have: the digit is capped, so the width is bounded.
    let big = styling_of(&xlsx_with(
        &styles_of("409", "Verdana"),
        &format!("{SHEET_HEAD}{SHEET_TAIL}"),
    ));
    assert!((8..=MAX_DEFAULT_COL_PX).contains(&big.default_col_px()));
}

#[test]
fn the_default_width_places_pictures_and_charts() {
    let mut styling = SheetStyling {
        default_col_width: Some(DefaultColWidth {
            width: Some(20.7109375),
            base: None,
        }),
        columns: BTreeMap::from([(1, 300)]),
        default_font: calibri(),
        ..SheetStyling::default()
    };
    assert_eq!(styling.default_col_px(), 145);
    let mut anchor = ChartAnchor {
        from_col: 3,
        to_col: 5,
        from_row: 1,
        to_row: 3,
        ..ChartAnchor::default()
    };
    // Column 1 is 300 px wide, columns 0, 2 and 3 are 145: column 3 starts at 145 + 300 + 145.
    anchor.sync_px_from_grid(&SheetGrid::of(&styling));
    assert_eq!((anchor.left_px, anchor.width_px), (590, 290));
    // And back: 590 px is the left edge of column 3 and the 290 px after it are two default columns.
    let mut by_px = ChartAnchor {
        left_px: 590,
        top_px: 24,
        width_px: 290,
        height_px: 48,
        ..ChartAnchor::default()
    };
    by_px.sync_grid_from_px(&SheetGrid::of(&styling));
    assert_eq!((by_px.from_col, by_px.col_off_emu, by_px.to_col), (3, 0, 5));
    // The sheet that says nothing puts the same pixels on other columns: 590 px lies in column 5 (556..620) and 880 px in
    // column 10 (876..940).
    styling.default_col_width = None;
    by_px.sync_grid_from_px(&SheetGrid::of(&styling));
    assert_eq!((by_px.from_col, by_px.to_col), (5, 10));
}

/// A chart on columns 3..5 stays on its cells through a save and a read, on the columns of the sheet's own width: the
/// pixels are those of the default (145 px) and not another default's.
#[test]
fn a_chart_keeps_its_cells_on_the_columns_of_the_sheet_through_a_file() {
    let chart = SheetChart {
        id: "c".to_string(),
        kind: ChartKind::Column,
        title: "차트".to_string(),
        data_range: "A1:B3".to_string(),
        switch_row_col: false,
        legend: ChartLegendPos::Right,
        anchor: ChartAnchor {
            from_col: 3,
            to_col: 5,
            from_row: 1,
            to_row: 6,
            ..ChartAnchor::default()
        },
        series: vec![ChartSeriesRef {
            name: "값".to_string(),
            name_ref: None,
            categories_ref: Some("Sheet1!$A$2:$A$3".to_string()),
            values_ref: "Sheet1!$B$2:$B$3".to_string(),
        }],
    };
    // Column 1 is 300 px wide, so column 3 starts after two default columns and 300 px; the chart spans two default columns.
    let widths = |width, base| Some(DefaultColWidth { width, base });
    for (default_col_width, default_px) in [
        (None, 64),
        (widths(Some(20.7109375), None), 145),
        (widths(None, Some(10)), 80),
    ] {
        let mut styling = SheetStyling {
            default_col_width,
            columns: BTreeMap::from([(1, 300)]),
            charts: vec![chart.clone()],
            default_font: calibri(),
            ..SheetStyling::default()
        };
        assert_eq!(styling.default_col_px(), default_px);
        let mut placed = styling.charts[0].anchor;
        placed.sync_px_from_grid(&SheetGrid::of(&styling));
        styling.charts[0].anchor = placed;
        let (left, width) = (2 * default_px + 300, 2 * default_px);
        assert_eq!((placed.left_px, placed.width_px), (left, width));
        let anchor = styling_of(&saved(styling)).charts[0].anchor;
        assert_eq!((anchor.from_col, anchor.to_col), (3, 5), "{default_px}");
        assert_eq!(
            (anchor.left_px, anchor.width_px),
            (left, width),
            "{default_px}"
        );
    }
}

/// `(x, width)` in points of the fill rectangle of the cell with the red channel `red`.
fn fill_of(pdf: &str, red: u32) -> (f64, f64) {
    let red = f64::from(red) / 255.0;
    let at = pdf
        .find(&format!("q {red:.3} 0.000 0.000 rg "))
        .unwrap_or_else(|| panic!("no fill of {red}"));
    let ops: Vec<&str> = pdf[at..].split_whitespace().skip(5).take(4).collect();
    (ops[0].parse().unwrap(), ops[2].parse().unwrap())
}

#[test]
fn the_pdf_draws_the_default_column_at_the_width_of_the_sheet() {
    let cells = vec![(0, 0, "a".to_string()), (0, 1, "b".to_string())];
    let with = |default_col_width| {
        let mut styling = SheetStyling {
            default_col_width,
            columns: BTreeMap::from([(1, 200)]),
            default_font: calibri(),
            ..SheetStyling::default()
        };
        for (col, fill) in [(0, 0x10_0000), (1, 0x20_0000)] {
            styling.cells.insert(
                (0, col),
                CellStyle {
                    fill: Some(fill),
                    ..CellStyle::default()
                },
            );
        }
        let sheets = vec![("S".to_string(), cells.clone())];
        let pdf = encode_sheet_pdf("열 너비", &sheets, &[styling], &[]).unwrap();
        String::from_utf8_lossy(&pdf).into_owned()
    };
    let plain = with(None);
    let (_, a_w) = fill_of(&plain, 0x10);
    let (_, b_w) = fill_of(&plain, 0x20);
    assert!((a_w - 64.0 * 0.75).abs() < 0.011 && (b_w - 200.0 * 0.75).abs() < 0.011);
    let wide = with(Some(DefaultColWidth {
        width: Some(20.7109375),
        base: None,
    }));
    let (_, a_w) = fill_of(&wide, 0x10);
    assert!((a_w - 145.0 * 0.75).abs() < 0.011, "{a_w}");
}

#[test]
fn an_ods_sheet_has_no_default_of_its_own_so_its_unstyled_columns_follow_the_same_rule() {
    let styling = SheetStyling {
        columns: BTreeMap::from([(1, 150)]),
        ..SheetStyling::default()
    };
    let ods = OdfCodec::pack_ods_multi_styled(&[cell()], std::slice::from_ref(&styling)).unwrap();
    let back = read_sheet_styling(&ods, DocumentFormat::Ods)
        .unwrap()
        .remove(0)
        .1;
    assert_eq!(back.columns, BTreeMap::from([(1, 150)]));
    assert_eq!(back.default_col_width, None);
    // The standard font has no ODS counterpart here: a sheet without a default is 72 px (PD-306).
    assert_eq!(
        (back.default_col_px(), back.col_px(0), back.col_px(1)),
        (72, 72, 150)
    );
    assert_eq!(back.col_px(2), 72);
}

#[test]
fn the_widths_written_down_do_not_change_with_the_default() {
    let sheet = format!(
        r#"{SHEET_HEAD}<sheetFormatPr baseColWidth="10" defaultColWidth="20"/><cols><col min="1" max="1" width="9.140625" customWidth="1"/><col min="3" max="3" width="13.7109375" customWidth="1"/></cols>{SHEET_TAIL}"#
    );
    let first = styling_of(&calibri_xlsx(&sheet));
    assert_eq!(first.default_col_px(), 140);
    assert_eq!(first.columns, BTreeMap::from([(0, 64), (2, 96)]));
    let second = styling_of(&saved(first.clone()));
    assert_eq!(second.columns, first.columns);
    assert_eq!(second.default_col_width, first.default_col_width);
}

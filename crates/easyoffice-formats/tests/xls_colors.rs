//! Fill and font colours in an XLS (PD-227): `FONT`, `XF` and `PALETTE` records written for the colours of
//! the cells, read back, bounded on hostile files.

use easyoffice_formats::{
    CellStyle, LegacyOle2Codec, LegacyXlsPayload, SheetLinks, SheetStyling, MAX_BIFF8_COL,
    MAX_BIFF8_ROW,
};

fn payload(cells: Vec<(u32, u32, &str)>) -> LegacyXlsPayload {
    LegacyXlsPayload {
        sheet_name: "S1".to_string(),
        cells: cells
            .into_iter()
            .map(|(r, c, v)| (r, c, v.to_string()))
            .collect(),
    }
}

fn colored(fill: Option<u32>, font_color: Option<u32>) -> CellStyle {
    CellStyle {
        fill,
        font_color,
        ..Default::default()
    }
}

fn styling(cells: Vec<((u32, u32), CellStyle)>) -> SheetStyling {
    SheetStyling {
        cells: cells.into_iter().collect(),
        ..Default::default()
    }
}

fn pack(cells: Vec<(u32, u32, &str)>, st: SheetStyling) -> Vec<u8> {
    LegacyOle2Codec::pack_xls_multi_values_styled(
        &[payload(cells)],
        &[SheetLinks::default()],
        &[st],
    )
    .unwrap()
}

fn read(bytes: &[u8]) -> SheetStyling {
    LegacyOle2Codec::unpack_xls_with_styling(bytes)
        .unwrap()
        .1
        .remove(0)
        .1
}

#[test]
fn colours_of_valued_and_blank_cells_round_trip() {
    let st = styling(vec![
        ((0, 0), colored(Some(0xFFFF00), None)),
        ((0, 1), colored(None, Some(0xFF0000))),
        ((1, 0), colored(Some(0x123456), Some(0xABCDEF))),
        ((2, 2), colored(Some(0x00B050), None)),
        (
            (3, 0),
            CellStyle {
                bold: true,
                ..Default::default()
            },
        ),
    ]);
    let bytes = pack(
        vec![(0, 0, "가"), (0, 1, "12"), (1, 0, "다"), (4, 4, "끝")],
        st,
    );
    let back = read(&bytes);
    let want = styling(vec![
        ((0, 0), colored(Some(0xFFFF00), None)),
        ((0, 1), colored(None, Some(0xFF0000))),
        ((1, 0), colored(Some(0x123456), Some(0xABCDEF))),
        ((2, 2), colored(Some(0x00B050), None)),
    ]);
    assert_eq!(
        back.cells, want.cells,
        "a blank coloured cell is kept; other formatting is not carried"
    );
    // The values are untouched, and the blank cell added none.
    let sheets = LegacyOle2Codec::unpack_xls_multi(&bytes).unwrap();
    assert_eq!(sheets[0].cells.len(), 4);
}

#[test]
fn a_workbook_with_no_colour_is_written_as_before() {
    let plain = pack(
        vec![(0, 0, "a")],
        styling(vec![(
            (0, 0),
            CellStyle {
                bold: true,
                ..Default::default()
            },
        )]),
    );
    let none = LegacyOle2Codec::pack_xls_multi_values(&[payload(vec![(0, 0, "a")])]).unwrap();
    assert_eq!(plain, none);
}

#[test]
fn default_palette_colours_need_no_palette_record() {
    let default_only = styling(vec![((0, 0), colored(Some(0xFF0000), Some(0x0000FF)))]);
    let custom = styling(vec![((0, 0), colored(Some(0x123456), Some(0x0000FF)))]);
    let a = pack(vec![(0, 0, "x")], default_only);
    let b = pack(vec![(0, 0, "x")], custom);
    let has_palette = |bytes: &[u8]| {
        let streams = easyoffice_formats::Ole2ContainerCodec::unpack(bytes).unwrap();
        let wb = &streams.iter().find(|s| s.name == "Workbook").unwrap().data;
        wb.windows(4).any(|w| w == [0x92, 0x00, 0xE2, 0x00])
    };
    assert!(!has_palette(&a));
    assert!(has_palette(&b));
    assert_eq!(
        read(&a).cells[&(0, 0)],
        colored(Some(0xFF0000), Some(0x0000FF))
    );
    assert_eq!(
        read(&b).cells[&(0, 0)],
        colored(Some(0x123456), Some(0x0000FF))
    );
}

#[test]
fn more_than_the_palette_holds_takes_the_closest_colour_and_is_counted() {
    // 100 colours that are not in the default palette: the 56 entries cannot hold them.
    let cells: Vec<((u32, u32), CellStyle)> = (0..100u32)
        .map(|i| ((i, 0), colored(Some(0x010101 * (i + 1) + 0x000300), None)))
        .collect();
    let st = styling(cells.clone());
    let (approximated, dropped) = LegacyOle2Codec::xls_color_losses(std::slice::from_ref(&st));
    assert!(approximated >= 100 - 56, "{approximated}");
    assert_eq!(dropped, 0);
    let bytes = pack(vec![(0, 0, "x")], st);
    let back = read(&bytes);
    assert_eq!(back.cells.len(), 100);
    // Every colour is one of the 56 palette colours (distance to the asked one stays small for a grey ramp).
    let distinct: std::collections::BTreeSet<u32> =
        back.cells.values().filter_map(|c| c.fill).collect();
    assert!(distinct.len() <= 56, "{}", distinct.len());
}

#[test]
fn cells_outside_the_format_grid_are_dropped_and_counted() {
    let st = styling(vec![
        ((MAX_BIFF8_ROW + 1, 0), colored(Some(0xFF0000), None)),
        ((0, MAX_BIFF8_COL + 1), colored(Some(0xFF0000), None)),
        (
            (MAX_BIFF8_ROW, MAX_BIFF8_COL),
            colored(Some(0x00FF00), None),
        ),
    ]);
    assert_eq!(
        LegacyOle2Codec::xls_color_losses(std::slice::from_ref(&st)),
        (0, 2)
    );
    let back = read(&pack(vec![(0, 0, "x")], st));
    assert_eq!(back.cells.len(), 1);
    assert!(back.cells.contains_key(&(MAX_BIFF8_ROW, MAX_BIFF8_COL)));
}

/// Writes a coloured workbook to `$EO_XLS_COLOR_SAMPLE` for a reader that is not this engine (an office
/// suite converting it); run with `--ignored`.
#[test]
#[ignore = "writes a sample file for an independent reader"]
fn write_a_sample_for_an_independent_reader() {
    let Some(path) = std::env::var_os("EO_XLS_COLOR_SAMPLE") else {
        return;
    };
    let st = styling(vec![
        ((0, 0), colored(Some(0xFFFF00), None)),
        ((0, 1), colored(None, Some(0xFF0000))),
        ((1, 0), colored(Some(0x123456), Some(0xFFFFFF))),
        ((2, 2), colored(Some(0x00B050), None)),
    ]);
    std::fs::write(
        path,
        pack(vec![(0, 0, "가"), (0, 1, "12"), (1, 0, "다")], st),
    )
    .unwrap();
}

/// Prints the colours our reader finds in the file named by `$EO_XLS_COLOR_READ` (a file another program
/// wrote); run with `--ignored --nocapture`.
#[test]
#[ignore = "reads a file an independent writer made"]
fn read_a_file_an_independent_writer_made() {
    let Some(path) = std::env::var_os("EO_XLS_COLOR_READ") else {
        return;
    };
    let bytes = std::fs::read(path).unwrap();
    for (at, style) in read(&bytes).cells {
        println!("{at:?} fill={:?} font={:?}", style.fill, style.font_color);
    }
}

/// The `(opcode, body)` records of the first (globals) substream of a written workbook.
fn globals(bytes: &[u8]) -> Vec<(u16, Vec<u8>)> {
    let streams = easyoffice_formats::Ole2ContainerCodec::unpack(bytes).unwrap();
    let wb = &streams.iter().find(|s| s.name == "Workbook").unwrap().data;
    let (mut at, mut out, mut eofs) = (0usize, Vec::new(), 0);
    while at + 4 <= wb.len() && eofs < 1 {
        let op = u16::from_le_bytes([wb[at], wb[at + 1]]);
        let len = usize::from(u16::from_le_bytes([wb[at + 2], wb[at + 3]]));
        out.push((op, wb[at + 4..at + 4 + len].to_vec()));
        at += 4 + len;
        eofs += usize::from(op == 0x000A);
    }
    out
}

fn palette_of(records: &[(u16, Vec<u8>)]) -> Vec<u32> {
    match records.iter().find(|r| r.0 == 0x0092) {
        Some((_, body)) => body[2..]
            .chunks(4)
            .map(|c| (u32::from(c[0]) << 16) | (u32::from(c[1]) << 8) | u32::from(c[2]))
            .collect(),
        None => Vec::new(),
    }
}

fn manhattan(a: u32, b: u32) -> u32 {
    (0..3)
        .map(|k| ((a >> (8 * k)) & 0xFF).abs_diff((b >> (8 * k)) & 0xFF))
        .sum()
}

/// A sheet whose cells have `pairs` distinct (fill, font) colour pairs, or `fills` distinct fills: the XFs stay
/// under the limit of the format, and every colour reads back as the closest palette colour (PD-227).
fn many_colours(cells: Vec<((u32, u32), CellStyle)>) {
    let originals = cells.clone();
    let bytes = pack(vec![(0, 0, "x")], styling(cells));
    let records = globals(&bytes);
    let xfs = records.iter().filter(|r| r.0 == 0x00E0).count();
    assert!(xfs <= 4_050, "{xfs} XFs");
    let palette = palette_of(&records);
    assert_eq!(palette.len(), 56, "so many colours change the palette");
    let back = read(&bytes);
    assert_eq!(back.cells.len(), originals.len());
    for (at, style) in originals {
        let got = back.cells[&at];
        for (want, read) in [(style.fill, got.fill), (style.font_color, got.font_color)] {
            let Some(want) = want else {
                assert_eq!(read, None, "{at:?}");
                continue;
            };
            let read = read.unwrap_or_else(|| panic!("{at:?} lost its colour"));
            assert!(
                palette.contains(&read),
                "{at:?}: {read:06X} is not a palette colour"
            );
            let closest = palette.iter().map(|&c| manhattan(c, want)).min().unwrap();
            assert_eq!(
                manhattan(read, want),
                closest,
                "{at:?}: {want:06X} read as {read:06X}"
            );
        }
    }
}

#[test]
fn five_thousand_colour_pairs_share_xfs_and_read_back_as_the_closest_palette_colour() {
    let cells = (0..5_000u32)
        .map(|i| {
            let fill = 0x010203 * (i % 97 + 1) + (i / 97) * 0x000101;
            let font = 0x0A0B0C + 0x030201 * (i % 89);
            ((i % 60_000, i / 60_000), colored(Some(fill), Some(font)))
        })
        .collect();
    many_colours(cells);
}

#[test]
fn seventy_thousand_fills_never_wrap_an_xf_number() {
    // 65,536 rows x 256 columns hold them: distinct fills spread over the whole grid of the format.
    let cells = (0..70_000u32)
        .map(|i| {
            (
                (i % 65_536, i / 65_536),
                colored(Some((i.wrapping_mul(2_654_435_761)) >> 8 & 0xFFFFFF), None),
            )
        })
        .collect();
    many_colours(cells);
}

#[test]
fn a_short_xf_record_keeps_the_numbers_of_the_xfs_after_it() {
    let st = styling(vec![
        ((0, 0), colored(Some(0xFF0000), None)),
        ((0, 1), colored(Some(0x00FF00), None)),
    ]);
    let bytes = pack(vec![(0, 0, "a"), (0, 1, "b")], st);
    let streams = easyoffice_formats::Ole2ContainerCodec::unpack(&bytes).unwrap();
    let mut wb = streams
        .iter()
        .find(|s| s.name == "Workbook")
        .unwrap()
        .data
        .clone();
    // Insert a short XF record before the written ones: every XF number after it moves by one, so the
    // cells (which name the XF by number) now point at the XF before the one they were written for.
    let at = (0..wb.len() - 4)
        .find(|&i| wb[i..i + 4] == [0xE0, 0x00, 20, 0x00])
        .unwrap();
    let mut spliced = wb[..at].to_vec();
    spliced.extend([0xE0, 0x00, 4, 0x00, 1, 2, 3, 4]);
    spliced.extend(&wb[at..]);
    wb = spliced;
    let repacked =
        easyoffice_formats::Ole2ContainerCodec::pack(&[("Workbook".to_string(), wb)]).unwrap();
    let back = read(&repacked);
    // Written: default XF 0, green 1, red 2 (the colours are in order). With the short record first they
    // are 1, 2 and 3 and the cells, which name the XF by number, point one XF earlier: A1 (red, XF 2) now
    // reads the green XF and B1 (green, XF 1) the default one. Had the short record been skipped without
    // taking a number, A1 and B1 would have read their own colours.
    assert_eq!(back.cells.get(&(0, 0)).and_then(|c| c.fill), Some(0x00FF00));
    assert_eq!(back.cells.get(&(0, 1)).and_then(|c| c.fill), None);
}

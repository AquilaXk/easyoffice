//! Pictures and charts sit on the rows the sheet has: a two-cell anchor is as tall in pixels as the rows it spans and a
//! picture that has pixels only lands on the row that holds them. The rows are the height of each row, the default row
//! height a user set and the hidden rows (PD-299); a saved file read again keeps every picture on its cells.

use easyoffice_formats::{
    deflate_zlib, effective_image_two_cell, AnchorEdit, ChartAnchor, ChartKind, ChartLegendPos,
    ChartSeriesRef, IeeeCrc32, OdsUnpacked, OoxmlCodec, RichImage, RowHeight, RowOffsets,
    SheetChart, SheetFloatingImage, SheetGrid, SheetStyling, ZipArchiveCodec, ZipSecurityPolicy,
};

fn png() -> Vec<u8> {
    fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = (data.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let crc = IeeeCrc32::update(IeeeCrc32::checksum(kind), data);
        out.extend_from_slice(&crc.to_be_bytes());
        out
    }
    let raw = [0u8, 1, 2, 3, 1, 2, 3, 0, 1, 2, 3, 1, 2, 3];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&2u32.to_be_bytes());
    ihdr.extend_from_slice(&2u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    out.extend(chunk(b"IHDR", &ihdr));
    out.extend(chunk(b"IDAT", &deflate_zlib(&raw)));
    out.extend(chunk(b"IEND", &[]));
    out
}

fn custom(points: f64) -> RowHeight {
    RowHeight::from_points(points, true).unwrap()
}

/// Rows of 40 px (30 pt default), row 1 of 80 px (60 pt), row 4 of 20 px (15 pt):
/// tops 0, 40, 120, 160, 200, 220, 260 and 40 px each below.
fn tall_rows() -> SheetStyling {
    let mut styling = SheetStyling {
        default_row_height: Some(custom(30.0)),
        ..SheetStyling::default()
    };
    styling.row_heights.insert(1, custom(60.0));
    styling.row_heights.insert(4, custom(15.0));
    styling
}

fn picture(anchor: ChartAnchor) -> SheetFloatingImage {
    SheetFloatingImage {
        id: "pic".to_string(),
        name: "그림".to_string(),
        alt: String::new(),
        image: RichImage::new(png(), "").unwrap(),
        anchor,
    }
}

fn chart(anchor: ChartAnchor) -> SheetChart {
    SheetChart {
        id: "chart".to_string(),
        kind: ChartKind::Column,
        title: "차트".to_string(),
        data_range: "A1:B3".to_string(),
        switch_row_col: false,
        legend: ChartLegendPos::Right,
        anchor,
        series: vec![ChartSeriesRef {
            name: "값".to_string(),
            name_ref: None,
            categories_ref: Some("S!$A$2:$A$3".to_string()),
            values_ref: "S!$B$2:$B$3".to_string(),
        }],
    }
}

fn sheets() -> Vec<OdsUnpacked> {
    let cells = vec![
        (0, 0, "k".to_string()),
        (0, 1, "v".to_string()),
        (1, 0, "a".to_string()),
        (1, 1, "1".to_string()),
        (2, 0, "b".to_string()),
        (2, 1, "2".to_string()),
    ];
    vec![("S".to_string(), cells)]
}

fn round_trip(styling: SheetStyling) -> SheetStyling {
    let xlsx = OoxmlCodec::pack_xlsx_multi_styled(&sheets(), &[], &[styling]).unwrap();
    let (_, mut read) = OoxmlCodec::unpack_xlsx_with_styling(&xlsx).unwrap();
    read.remove(0).1
}

/// Rows 1..=5 are `top(1)=40 ..= top(5)=220` tall: 80 + 40 + 40 + 20 + 40 below the first row is `220 - 40 = 180`.
#[test]
fn a_two_cell_picture_is_as_tall_as_the_rows_it_spans_after_a_round_trip() {
    let mut styling = tall_rows();
    styling.images.push(picture(ChartAnchor {
        from_col: 1,
        from_row: 1,
        to_col: 3,
        to_row: 5,
        ..ChartAnchor::default()
    }));
    let read = round_trip(styling);
    let anchor = read.images[0].anchor;
    assert_eq!((anchor.from_row, anchor.to_row), (1, 5));
    assert_eq!((anchor.row_off_emu, anchor.to_row_off_emu), (0, 0));
    assert_eq!((anchor.top_px, anchor.height_px), (40, 180));
}

#[test]
fn a_picture_with_pixels_only_lands_on_the_row_that_holds_them() {
    let mut styling = tall_rows();
    // 160 px is the top of row 3 and 260 px the top of row 6.
    styling.images.push(picture(ChartAnchor {
        left_px: 80,
        top_px: 160,
        width_px: 100,
        height_px: 100,
        ..ChartAnchor::default()
    }));
    let read = round_trip(styling);
    let anchor = read.images[0].anchor;
    assert_eq!((anchor.from_row, anchor.row_off_emu), (3, 0));
    assert_eq!((anchor.to_row, anchor.to_row_off_emu), (6, 0));
    assert_eq!((anchor.top_px, anchor.height_px), (160, 100));
}

#[test]
fn a_picture_inside_a_row_keeps_its_offset_in_that_row() {
    let mut styling = tall_rows();
    // 60 px is 20 px below the top of row 1, which is 80 px tall.
    styling.images.push(picture(ChartAnchor {
        top_px: 60,
        width_px: 64,
        height_px: 40,
        ..ChartAnchor::default()
    }));
    let anchor = round_trip(styling).images[0].anchor;
    assert_eq!((anchor.from_row, anchor.row_off_emu), (1, 20 * 9_525));
    assert_eq!((anchor.to_row, anchor.to_row_off_emu), (1, 60 * 9_525));
}

#[test]
fn hidden_rows_take_no_height_in_the_position_of_a_picture() {
    let mut styling = tall_rows();
    styling.hidden_rows = vec![(2, 3)];
    // Rows 2 and 3 are hidden: row 4 starts at 120 and row 5 at 140.
    styling.images.push(picture(ChartAnchor {
        from_col: 1,
        from_row: 1,
        to_col: 3,
        to_row: 5,
        ..ChartAnchor::default()
    }));
    let anchor = round_trip(styling).images[0].anchor;
    assert_eq!((anchor.from_row, anchor.to_row), (1, 5));
    assert_eq!((anchor.top_px, anchor.height_px), (40, 100));
}

#[test]
fn a_chart_is_placed_by_the_same_rows() {
    let mut styling = tall_rows();
    styling.charts.push(chart(ChartAnchor {
        from_col: 1,
        from_row: 1,
        to_col: 5,
        to_row: 6,
        ..ChartAnchor::default()
    }));
    let anchor = round_trip(styling).charts[0].anchor;
    assert_eq!((anchor.from_row, anchor.to_row), (1, 6));
    // Rows 1..=5 are 80 + 40 + 40 + 20 + 40.
    assert_eq!((anchor.top_px, anchor.height_px), (40, 220));
}

/// The anchor stays on its cells when the rows above change: the picture moves with them.
#[test]
fn a_taller_row_above_moves_the_picture_down_and_keeps_its_cell() {
    let anchor = ChartAnchor {
        from_col: 1,
        from_row: 3,
        to_col: 3,
        to_row: 5,
        ..ChartAnchor::default()
    };
    let top_with = |styling: &SheetStyling| {
        let mut anchor = anchor;
        anchor.sync_px_from_grid(&SheetGrid::of(styling));
        (anchor.from_row, anchor.top_px)
    };
    let plain = SheetStyling::default();
    let mut taller = SheetStyling::default();
    taller.row_heights.insert(0, custom(75.0));
    assert_eq!(top_with(&plain), (3, 72));
    assert_eq!(top_with(&taller), (3, 72 + 100 - 24));
}

/// 96 px tall and 160 px wide on plain rows, which the taller rows of `tall_rows` do not keep when the cells decide.
fn sized_anchor(edit_as: AnchorEdit) -> ChartAnchor {
    ChartAnchor {
        edit_as,
        from_col: 1,
        from_row: 1,
        to_col: 3,
        to_row: 5,
        left_px: 80,
        top_px: 24,
        width_px: 160,
        height_px: 96,
        ..ChartAnchor::default()
    }
}

/// `editAs`: a two-cell anchor says its own cells (its size follows them), a one-cell anchor starts on its cell and keeps
/// its size, an absolute anchor keeps its place whatever the rows are.
#[test]
fn the_cells_an_anchor_writes_follow_its_edit_kind() {
    let styling = tall_rows();
    let grid = SheetGrid::of(&styling);
    let cells = |edit_as| effective_image_two_cell(&sized_anchor(edit_as), &grid);
    assert_eq!(cells(AnchorEdit::TwoCell), (1, 1, 0, 0, 3, 5, 0, 0));
    // Row 1 starts at 40 px, 96 px below is 136 px: row 2 (120..160) and 16 px into it; column 3 starts at 240 px.
    assert_eq!(
        cells(AnchorEdit::OneCell),
        (1, 1, 0, 0, 3, 2, 0, 16 * 9_525)
    );
    let mut absolute = sized_anchor(AnchorEdit::Absolute);
    (absolute.left_px, absolute.top_px) = (100, 100);
    // 100 px down is row 1 (40..120) and 60 px into it; 100 px right is column 1 and 20 px into it. The end, 160 x 96 px
    // further, is column 3 (starts at 240) 20 px in and row 3 (160..200) 36 px in.
    assert_eq!(
        effective_image_two_cell(&absolute, &grid),
        (1, 1, 20 * 9_525, 60 * 9_525, 3, 3, 20 * 9_525, 36 * 9_525)
    );
}

#[test]
fn the_edit_kind_survives_a_round_trip_and_the_size_follows_its_rule() {
    let mut styling = tall_rows();
    styling
        .images
        .push(picture(sized_anchor(AnchorEdit::OneCell)));
    let mut absolute = sized_anchor(AnchorEdit::Absolute);
    (absolute.left_px, absolute.top_px) = (100, 100);
    styling.images.push(picture(absolute));
    styling
        .images
        .push(picture(sized_anchor(AnchorEdit::TwoCell)));
    let read = round_trip(styling);
    let kinds: Vec<_> = read.images.iter().map(|i| i.anchor.edit_as).collect();
    assert_eq!(
        kinds,
        [
            AnchorEdit::OneCell,
            AnchorEdit::Absolute,
            AnchorEdit::TwoCell
        ]
    );
    let one = read.images[0].anchor;
    assert_eq!((one.from_row, one.to_row), (1, 2));
    assert_eq!((one.top_px, one.height_px), (40, 96), "its size is kept");
    let abs = read.images[1].anchor;
    assert_eq!((abs.left_px, abs.top_px), (100, 100), "its place is kept");
    let two = read.images[2].anchor;
    assert_eq!(
        (two.top_px, two.height_px),
        (40, 180),
        "its size is its cells"
    );
}

/// A value of `editAs` that is not one of the three is the default, and so is no attribute at all.
#[test]
fn an_edit_kind_that_is_not_one_of_the_three_is_two_cell() {
    for (text, want) in [
        ("twoCell", AnchorEdit::TwoCell),
        ("oneCell", AnchorEdit::OneCell),
        ("absolute", AnchorEdit::Absolute),
        (" oneCell ", AnchorEdit::OneCell),
        ("", AnchorEdit::TwoCell),
        ("onecell", AnchorEdit::TwoCell),
        ("ONECELL", AnchorEdit::TwoCell),
        ("move", AnchorEdit::TwoCell),
        ("oneCell\u{0}", AnchorEdit::TwoCell),
    ] {
        assert_eq!(AnchorEdit::parse(text), want, "{text:?}");
    }
    let mut styling = tall_rows();
    styling
        .images
        .push(picture(sized_anchor(AnchorEdit::OneCell)));
    let xlsx = OoxmlCodec::pack_xlsx_multi_styled(&sheets(), &[], &[styling]).unwrap();
    let entries = ZipArchiveCodec::unpack(&xlsx, &ZipSecurityPolicy::default()).unwrap();
    let drawing = |entries: &[easyoffice_formats::ZipEntry]| {
        let part = entries
            .iter()
            .find(|e| e.path.starts_with("xl/drawings/drawing"))
            .unwrap();
        String::from_utf8(part.data.clone()).unwrap()
    };
    assert!(drawing(&entries).contains(r#"<xdr:twoCellAnchor editAs="oneCell">"#));
    for hostile in [
        "bogus",
        "",
        "oneCell oneCell",
        "&lt;x&gt;",
        "twoCell\" a=\"b",
    ] {
        let edited: Vec<(String, Vec<u8>)> = entries
            .iter()
            .map(|e| {
                let data = if e.path.starts_with("xl/drawings/drawing") {
                    String::from_utf8(e.data.clone())
                        .unwrap()
                        .replace("editAs=\"oneCell\"", &format!("editAs=\"{hostile}\""))
                        .into_bytes()
                } else {
                    e.data.clone()
                };
                (e.path.clone(), data)
            })
            .collect();
        let pkg = ZipArchiveCodec::pack_compressed(&edited).unwrap();
        let styling = OoxmlCodec::unpack_xlsx_with_styling(&pkg).map(|(_, s)| s);
        // Either the file reads with the default or a value that is not XML is refused; never another kind.
        if let Ok(stylings) = styling {
            assert_eq!(
                stylings[0].1.images[0].anchor.edit_as,
                AnchorEdit::TwoCell,
                "{hostile:?}"
            );
        }
    }
}

/// A user's default row height and hidden rows are the same rule the page and the PDF use (`SheetStyling::shown_row_px`).
#[test]
fn row_offsets_add_up_the_heights_of_the_rows_before() {
    let mut styling = tall_rows();
    styling.hidden_rows = vec![(6, 8)];
    let rows = RowOffsets::of(&styling);
    let mut top = 0u64;
    for row in 0..20u32 {
        assert_eq!(rows.top(row), top, "top of row {row}");
        top += styling.shown_row_px(row, 24.0).round() as u64;
    }
}

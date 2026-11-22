//! Pictures and charts sit on the rows the sheet has: a two-cell anchor is as tall in pixels as the rows it spans and a
//! picture that has pixels only lands on the row that holds them. The rows are the height of each row, the default row
//! height a user set and the hidden rows (PD-299); a saved file read again keeps every picture on its cells.

use easyoffice_formats::{
    deflate_zlib, ChartAnchor, ChartKind, ChartLegendPos, ChartSeriesRef, IeeeCrc32, OdsUnpacked,
    OoxmlCodec, RichImage, RowHeight, RowOffsets, SheetChart, SheetFloatingImage, SheetGrid,
    SheetStyling,
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

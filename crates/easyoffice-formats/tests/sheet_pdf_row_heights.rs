//! The PDF of a sheet draws each row at its height: the height of the row (`row_heights`), else the default row height a
//! user set, else the grid's 24 px; a floating image is placed by the rows it spans (PD-287).

use easyoffice_formats::{
    deflate_zlib, encode_sheet_pdf, CellStyle, ChartAnchor, IeeeCrc32, OdsUnpacked, RichImage,
    RowHeight, SheetFloatingImage, SheetStyling,
};

const ROWS: u32 = 5;
const PT_PER_PX: f64 = 0.75;

fn height(points: &str, custom: bool) -> RowHeight {
    RowHeight::parse(points, custom).unwrap()
}

/// A sheet of `ROWS` rows whose cells each have a fill of their own (`0x000001 * (row + 1)` in the red channel steps),
/// so the rectangle of each row can be found in the page.
fn sheet(styling: SheetStyling) -> (Vec<OdsUnpacked>, SheetStyling) {
    let cells = (0..ROWS).map(|r| (r, 0, format!("r{r}"))).collect();
    let mut styling = styling;
    for r in 0..ROWS {
        styling.cells.insert(
            (r, 0),
            CellStyle {
                fill: Some(0x10_0000 * (r + 1)),
                ..CellStyle::default()
            },
        );
    }
    (vec![("S".to_string(), cells)], styling)
}

fn pdf_text(styling: SheetStyling) -> String {
    let (sheets, styling) = sheet(styling);
    let pdf = encode_sheet_pdf("행 높이", &sheets, &[styling], &[]).unwrap();
    String::from_utf8_lossy(&pdf).into_owned()
}

/// `(y, height)` in points of the fill rectangle of each row, from the page's drawing operators.
fn row_rects(pdf: &str) -> Vec<(f64, f64)> {
    (0..ROWS)
        .map(|r| {
            let red = f64::from(0x10 * (r + 1)) / 255.0;
            let at = pdf
                .find(&format!("q {red:.3} 0.000 0.000 rg "))
                .unwrap_or_else(|| panic!("no fill for row {r}"));
            let ops: Vec<&str> = pdf[at..].split_whitespace().skip(5).take(4).collect();
            let n = |i: usize| ops[i].parse::<f64>().unwrap();
            assert_eq!(&pdf[at..][..1], "q");
            (n(1), n(3))
        })
        .collect()
}

fn assert_heights(pdf: &str, px: [f64; ROWS as usize]) {
    let rects = row_rects(pdf);
    for (r, (&(y, h), want)) in rects.iter().zip(px).enumerate() {
        assert!(
            (h - want * PT_PER_PX).abs() < 0.011,
            "row {r}: {h} pt, {want} px"
        );
        if r > 0 {
            let (above_y, _) = rects[r - 1];
            assert!(
                (above_y - (y + h)).abs() < 0.011,
                "row {r} starts where row {} ends",
                r - 1
            );
        }
    }
}

#[test]
fn rows_with_no_height_of_their_own_are_24_px() {
    assert_heights(&pdf_text(SheetStyling::default()), [24.0; 5]);
}

#[test]
fn a_row_height_moves_the_rows_below_it() {
    let styling = SheetStyling {
        row_heights: [(1, height("60", true)), (3, height("15", false))].into(),
        ..SheetStyling::default()
    };
    assert_heights(&pdf_text(styling), [24.0, 80.0, 24.0, 20.0, 24.0]);
}

#[test]
fn the_default_row_height_a_user_set_is_the_height_of_the_other_rows() {
    let styling = SheetStyling {
        default_row_height: Some(height("30", true)),
        row_heights: [(1, height("60", true)), (3, height("15", true))].into(),
        ..SheetStyling::default()
    };
    assert_heights(&pdf_text(styling), [40.0, 80.0, 40.0, 20.0, 40.0]);
}

#[test]
fn a_default_that_follows_the_font_leaves_the_grid_row_height() {
    let styling = SheetStyling {
        default_row_height: Some(height("14.4", false)),
        ..SheetStyling::default()
    };
    assert_heights(&pdf_text(styling), [24.0; 5]);
}

#[test]
fn a_hidden_row_has_no_height_whatever_it_is_set_to() {
    let styling = SheetStyling {
        hidden_rows: vec![(1, 1)],
        row_heights: [(1, height("60", true))].into(),
        ..SheetStyling::default()
    };
    let pdf = pdf_text(styling);
    let rects = row_rects_present(&pdf);
    assert_eq!(rects.len(), 4, "the hidden row is not drawn");
}

fn row_rects_present(pdf: &str) -> Vec<u32> {
    (0..ROWS)
        .filter(|r| {
            pdf.contains(&format!(
                "q {:.3} 0.000 0.000 rg ",
                f64::from(0x10 * (r + 1)) / 255.0
            ))
        })
        .collect()
}

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

/// `(x, y, width, height)` of the image drawn as `/Im1`.
fn image_box(pdf: &str) -> (f64, f64, f64, f64) {
    let end = pdf.find("/Im1 Do").expect("the image is drawn");
    let line = &pdf[..end];
    let start = line.rfind("q ").unwrap();
    let t: Vec<f64> = line[start..]
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect();
    // `q w 0 0 h x y cm`
    (t[4], t[5], t[0], t[3])
}

fn with_image(mut styling: SheetStyling, from_row: u32, to_row: u32) -> SheetStyling {
    styling.images.push(SheetFloatingImage {
        id: "pic".to_string(),
        name: "그림".to_string(),
        alt: String::new(),
        image: RichImage::new(png(), "").unwrap(),
        anchor: ChartAnchor {
            from_col: 2,
            from_row,
            to_col: 4,
            to_row,
            width_px: 160,
            height_px: 96,
            ..ChartAnchor::default()
        },
    });
    let last = styling.images.last_mut().unwrap();
    last.anchor.to_row = to_row;
    styling
}

#[test]
fn an_image_is_placed_and_sized_by_the_rows_it_spans() {
    let plain = pdf_text(with_image(SheetStyling::default(), 1, 4));
    let (_, y_plain, _, h_plain) = image_box(&plain);
    assert!(
        (h_plain - 3.0 * 24.0 * PT_PER_PX).abs() < 0.011,
        "{h_plain}"
    );

    let styling = SheetStyling {
        default_row_height: Some(height("30", true)),
        row_heights: [(1, height("60", true)), (2, height("15", true))].into(),
        ..SheetStyling::default()
    };
    let tall = pdf_text(with_image(styling, 1, 4));
    let (_, y_tall, _, h_tall) = image_box(&tall);
    // Rows 1, 2 and 3: 80 px, 20 px and 40 px.
    assert!((h_tall - 140.0 * PT_PER_PX).abs() < 0.011, "{h_tall}");
    // The top of the image is the bottom of row 0, which is 40 px high instead of 24.
    let top_plain = y_plain + h_plain;
    let top_tall = y_tall + h_tall;
    assert!(
        ((top_plain - top_tall) - 16.0 * PT_PER_PX).abs() < 0.011,
        "{top_plain} {top_tall}"
    );
}

#[test]
fn heights_outside_the_bounds_are_refused() {
    let wrong = |centipt| RowHeight {
        centipt,
        custom: true,
    };
    for styling in [
        SheetStyling {
            default_row_height: Some(wrong(0)),
            ..SheetStyling::default()
        },
        SheetStyling {
            default_row_height: Some(wrong(40_901)),
            ..SheetStyling::default()
        },
        SheetStyling {
            default_row_height: Some(wrong(u32::MAX)),
            ..SheetStyling::default()
        },
        SheetStyling {
            row_heights: [(1, wrong(u32::MAX))].into(),
            ..SheetStyling::default()
        },
        SheetStyling {
            row_heights: [(2_000_000, height("20", true))].into(),
            ..SheetStyling::default()
        },
    ] {
        let (sheets, styling) = sheet(styling);
        assert!(encode_sheet_pdf("행 높이", &sheets, &[styling], &[]).is_err());
    }
}

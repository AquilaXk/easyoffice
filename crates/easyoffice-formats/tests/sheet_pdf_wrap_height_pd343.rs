//! Integration tests for engine sheet PDF wrapped-text line breaking and auto row height (`PD-343`).

use std::collections::BTreeMap;

use easyoffice_formats::{
    deflate_zlib, encode_sheet_pdf, AnchorEdit, CellBox, CellRule, CellShown, CellStyle, CfIcon,
    ChartAnchor, ChartKind, ChartLegendPos, ChartSeriesRef, DefaultFont, HAlign, IeeeCrc32,
    OdsUnpacked, PageScale, RichImage, RowHeight, SheetChart, SheetFloatingImage, SheetShown,
    SheetStyling, VAlign, FLAG_BOLD, FLAG_UNDERLINE,
};

const PT_PER_PX: f64 = 0.75;

fn base_styling() -> SheetStyling {
    SheetStyling {
        default_font: DefaultFont::kept(Some("Calibri"), Some(11.0)),
        ..SheetStyling::default()
    }
}

fn rgb(value: u32) -> String {
    let c = |shift: u32| f64::from((value >> shift) & 0xFF) / 255.0;
    format!("{:.3} {:.3} {:.3}", c(16), c(8), c(0))
}

fn fill_rects(pdf: &str, color: u32) -> Vec<(f64, f64, f64, f64)> {
    let color = rgb(color);
    pdf.lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("q ")?.strip_suffix(" f Q")?;
            let (c, path) = rest.split_once(" rg ")?;
            if c != color {
                return None;
            }
            let words: Vec<&str> = path.split(' ').collect();
            if !words.len().is_multiple_of(5) || words.chunks(5).any(|ch| ch[4] != "re") {
                return None;
            }
            Some(
                words
                    .chunks(5)
                    .map(|ch| {
                        let n = |i: usize| ch[i].parse::<f64>().unwrap();
                        (n(0), n(1), n(2), n(3))
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .flatten()
        .collect()
}

fn td_positions(pdf: &str) -> Vec<(f64, f64, String)> {
    pdf.lines()
        .filter_map(|line| {
            let td_idx = line.find(" Td ")?;
            let words: Vec<&str> = line[..td_idx].split(' ').collect();
            let y: f64 = words.last()?.parse().ok()?;
            let x: f64 = words.get(words.len().checked_sub(2)?)?.parse().ok()?;
            let body = if let Some(s) = line.find('(') {
                let e = line.rfind(") Tj")?;
                line[s + 1..e].to_string()
            } else {
                let s = line.find('<')?;
                let e = line.rfind("> Tj")?;
                line[s..=e].to_string()
            };
            Some((x, y, body))
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

fn image_box(pdf: &str) -> (f64, f64, f64, f64) {
    let end = pdf.find("/Im1 Do").expect("image drawn");
    let line = &pdf[..end];
    let start = line.rfind("q ").unwrap();
    let t: Vec<f64> = line[start..]
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect();
    (t[4], t[5], t[0], t[3])
}

#[test]
fn wrapped_cell_breaks_lines_expands_row_height_and_shifts_following_rows() {
    let mut styling = base_styling();
    styling.columns = BTreeMap::from([(0, 64)]);
    for r in 0..3u32 {
        styling.cells.insert(
            (r, 0),
            CellStyle {
                wrap: r == 0,
                fill: Some(0x10_0000 * (r + 1)),
                ..CellStyle::default()
            },
        );
    }
    let sheets: Vec<OdsUnpacked> = vec![(
        "Wrap".to_string(),
        vec![
            (0, 0, "Alpha Beta Gamma".to_string()),
            (1, 0, "Second".to_string()),
            (2, 0, "Third".to_string()),
        ],
    )];
    let bytes = encode_sheet_pdf("wrap", &sheets, &[styling], &[]).unwrap();
    let pdf = String::from_utf8_lossy(&bytes);

    let r0 = fill_rects(&pdf, 0x10_0000)[0];
    let r1 = fill_rects(&pdf, 0x20_0000)[0];
    let r2 = fill_rects(&pdf, 0x30_0000)[0];

    // Row 0 wraps into 3 lines (`Alpha`, `Beta`, `Gamma`): 3 * 13.5 + 4.5 = 45.0 pt.
    assert!((r0.3 - 45.0).abs() < 0.02, "row 0 height={}", r0.3);
    assert!((r1.3 - 16.5).abs() < 0.02, "row 1 height={}", r1.3);
    assert!((r0.1 - (r1.1 + r1.3)).abs() < 0.02);
    assert!((r1.1 - (r2.1 + r2.3)).abs() < 0.02);

    let tds = td_positions(&pdf);
    assert_eq!(tds.len(), 5, "{tds:?}");
    assert_eq!(tds[0].2, "Alpha");
    assert_eq!(tds[1].2, "Beta");
    assert_eq!(tds[2].2, "Gamma");
    assert!(tds[0].1 > tds[1].1 && tds[1].1 > tds[2].1);
    assert!(tds[2].1 > r0.1 && tds[0].1 + 9.0 <= r0.1 + r0.3);
}

#[test]
fn single_line_wrap_keeps_base_row_height_while_large_wrapped_font_expands() {
    let mut styling = base_styling();
    styling.columns = BTreeMap::from([(0, 120)]);
    styling.cells.insert(
        (0, 0),
        CellStyle {
            wrap: true,
            fill: Some(0x11_0000),
            ..CellStyle::default()
        },
    );
    styling.cells.insert(
        (1, 0),
        CellStyle {
            wrap: true,
            size_pt: Some(20.0),
            fill: Some(0x22_0000),
            ..CellStyle::default()
        },
    );
    let sheets: Vec<OdsUnpacked> = vec![(
        "FontWrap".to_string(),
        vec![(0, 0, "Short".to_string()), (1, 0, "Big".to_string())],
    )];
    let bytes = encode_sheet_pdf("font-wrap", &sheets, &[styling], &[]).unwrap();
    let pdf = String::from_utf8_lossy(&bytes);

    let r0 = fill_rects(&pdf, 0x11_0000)[0];
    let r1 = fill_rects(&pdf, 0x22_0000)[0];
    assert!(
        (r0.3 - 16.5).abs() < 0.02,
        "10pt single-line wrap stays 16.5pt: {}",
        r0.3
    );
    // 20pt * 1.275 = 25.5 pt > 16.5 pt.
    assert!(
        (r1.3 - 25.5).abs() < 0.02,
        "20pt single-line wrap grows to 25.5pt: {}",
        r1.3
    );
}

#[test]
fn custom_row_height_never_expands_and_clips_wrapped_overflow() {
    let mut styling = base_styling();
    styling.columns = BTreeMap::from([(0, 64)]);
    styling.row_heights = [
        (0, RowHeight::parse("16.5", true).unwrap()),
        (1, RowHeight::parse("15", false).unwrap()),
    ]
    .into();
    styling.cells.insert(
        (0, 0),
        CellStyle {
            wrap: true,
            fill: Some(0x10_1010),
            ..CellStyle::default()
        },
    );
    styling.cells.insert(
        (1, 0),
        CellStyle {
            wrap: true,
            fill: Some(0x20_2020),
            ..CellStyle::default()
        },
    );
    let sheets: Vec<OdsUnpacked> = vec![(
        "Custom".to_string(),
        vec![
            (0, 0, "One\nTwo\nThree\nFour".to_string()),
            (1, 0, "AutoA\nAutoB\nAutoC".to_string()),
        ],
    )];
    let bytes = encode_sheet_pdf("custom", &sheets, &[styling], &[]).unwrap();
    let pdf = String::from_utf8_lossy(&bytes);

    let r0 = fill_rects(&pdf, 0x10_1010)[0];
    let r1 = fill_rects(&pdf, 0x20_2020)[0];
    assert!(
        (r0.3 - 16.5).abs() < 0.02,
        "custom=true stays 16.5pt: {}",
        r0.3
    );
    assert!(
        (r1.3 - 45.0).abs() < 0.02,
        "custom=false expands to 45.0pt: {}",
        r1.3
    );
    assert!(pdf.contains("re W n\n"), "overflowing custom row must clip");
    // Lines completely below clipped box ("Three", "Four") are culled while "One" stays visible.
    assert!(pdf.contains("(One) Tj"));
    assert!(!pdf.contains("(Four) Tj"));

    let q_count = pdf.lines().filter(|l| l.starts_with("q ")).count();
    let cap_q_count = pdf
        .lines()
        .filter(|l| *l == "Q" || l.ends_with(" Q"))
        .count();
    assert_eq!(q_count, cap_q_count, "balanced q/Q:\n{pdf}");
}

#[test]
fn hidden_rows_hidden_cols_and_merged_cells_do_not_expand_row_height() {
    let mut styling = base_styling();
    styling.columns = BTreeMap::from([(0, 64), (1, 40)]);
    styling.hidden_rows = vec![(0, 0)];
    styling.hidden_cols = vec![(1, 1)];
    styling.merges = vec![CellBox {
        row0: 2,
        col0: 0,
        row1: 3,
        col1: 1,
    }];
    for r in 0..4u32 {
        styling.cells.insert(
            (r, 0),
            CellStyle {
                wrap: true,
                fill: Some(0x10_0000 * (r + 1)),
                ..CellStyle::default()
            },
        );
    }
    styling.cells.insert(
        (1, 1),
        CellStyle {
            wrap: true,
            ..CellStyle::default()
        },
    );
    let sheets: Vec<OdsUnpacked> = vec![(
        "Skip".to_string(),
        vec![
            (0, 0, "Hidden\nRow\nText".to_string()),
            (1, 0, "Visible".to_string()),
            (1, 1, "Hidden\nCol\nText".to_string()),
            (2, 0, "Merged\nMulti\nLine\nText".to_string()),
        ],
    )];
    let bytes = encode_sheet_pdf("skip", &sheets, &[styling], &[]).unwrap();
    let pdf = String::from_utf8_lossy(&bytes);

    assert!(fill_rects(&pdf, 0x10_0000).is_empty(), "hidden row omitted");
    assert!(
        (fill_rects(&pdf, 0x20_0000)[0].3 - 16.5).abs() < 0.02,
        "hidden col did not expand row 1"
    );
    assert!(
        (fill_rects(&pdf, 0x30_0000)[0].3 - 16.5).abs() < 0.02,
        "merged cell did not expand row 2"
    );
}

#[test]
fn vertical_and_horizontal_alignment_and_decorations_apply_per_wrapped_line() {
    let mut styling = base_styling();
    styling.columns = BTreeMap::from([(0, 120), (1, 120), (2, 120)]);
    styling.row_heights = [(0, RowHeight::parse("60", true).unwrap())].into();
    styling.cells.insert(
        (0, 0),
        CellStyle {
            wrap: true,
            h_align: HAlign::Left,
            v_align: VAlign::Top,
            underline: true,
            bold: true,
            ..CellStyle::default()
        },
    );
    styling.cells.insert(
        (0, 1),
        CellStyle {
            wrap: true,
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            ..CellStyle::default()
        },
    );
    styling.cells.insert(
        (0, 2),
        CellStyle {
            wrap: true,
            h_align: HAlign::Right,
            v_align: VAlign::Bottom,
            ..CellStyle::default()
        },
    );
    let sheets: Vec<OdsUnpacked> = vec![(
        "Align".to_string(),
        vec![
            (0, 0, "Top1\nTop2".to_string()),
            (0, 1, "Mid1\nMid2".to_string()),
            (0, 2, "Bot1\nBot2".to_string()),
        ],
    )];
    let bytes = encode_sheet_pdf("align", &sheets, &[styling], &[]).unwrap();
    let pdf = String::from_utf8_lossy(&bytes);

    let tds = td_positions(&pdf);
    let find_td = |s: &str| {
        tds.iter()
            .find(|t| t.2 == s)
            .map(|t| (t.0, t.1))
            .unwrap_or_else(|| panic!("missing {s} in {tds:?}"))
    };
    let top1 = find_td("Top1");
    let mid1 = find_td("Mid1");
    let bot1 = find_td("Bot1");
    assert!(
        top1.1 > mid1.1 && mid1.1 > bot1.1,
        "{top1:?} {mid1:?} {bot1:?}"
    );

    // Left (col 0) < Center (col 1 - col width 90pt) < Right (col 2 - 2*col width).
    let col_w = 120.0 * PT_PER_PX;
    let rel_top = top1.0;
    let rel_mid = mid1.0 - col_w;
    let rel_bot = bot1.0 - 2.0 * col_w;
    assert!(
        rel_top < rel_mid && rel_mid < rel_bot,
        "{rel_top} {rel_mid} {rel_bot}"
    );

    // Bold + underline emitted once per line (2 underline strokes).
    let stroke_lines = pdf
        .lines()
        .filter(|l| l.contains("0.60 w") && l.contains(" m "))
        .count();
    assert_eq!(stroke_lines, 2, "one underline per wrapped line:\n{pdf}");
}

#[test]
fn wrapped_korean_and_icon_set_narrow_width_and_page_scale_work_together() {
    let mut styling = base_styling();
    styling.columns = BTreeMap::from([(0, 64), (1, 100)]);
    styling.page.scale = PageScale::Percent(50);
    styling.cells.insert(
        (0, 0),
        CellStyle {
            wrap: true,
            fill: Some(0xDDEEFF),
            ..CellStyle::default()
        },
    );
    styling.cells.insert(
        (0, 1),
        CellStyle {
            wrap: true,
            ..CellStyle::default()
        },
    );
    let mut shown = SheetShown::new();
    shown.insert(
        (0, 0),
        CellShown {
            rule: Some(CellRule {
                flags: FLAG_BOLD | FLAG_UNDERLINE,
                icon: Some(CfIcon { set: 0, index: 2 }),
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    // Col 0 width = 48pt; pad = 4+4=8pt; icon inset = 13.5pt -> room = 26.5pt (2 Korean chars/line!).
    let sheets: Vec<OdsUnpacked> = vec![(
        "IconWrap".to_string(),
        vec![(0, 0, "가나다라마바".to_string()), (0, 1, "OK".to_string())],
    )];
    let bytes = encode_sheet_pdf("icon-wrap", &sheets, &[styling], &[shown]).unwrap();
    let pdf = String::from_utf8_lossy(&bytes);

    // 3 lines (`가나`, `다라`, `마바`) => 45.0 pt unscaled => 22.5 pt at 50% scale.
    let r0 = fill_rects(&pdf, 0xDDEEFF)[0];
    assert!(
        (r0.3 - 22.5).abs() < 0.02,
        "50% scaled 3-line height should be 22.5pt: {}",
        r0.3
    );
    let tds = td_positions(&pdf);
    assert_eq!(tds.len(), 4, "3 Korean lines + 1 OK cell: {tds:?}");
}

#[test]
fn wrapped_rows_trigger_page_breaks_and_adjust_two_cell_vs_one_cell_overlays() {
    let mut styling = base_styling();
    styling.columns = BTreeMap::from([(0, 64), (1, 64)]);
    for r in 0..30u32 {
        styling.cells.insert(
            (r, 0),
            CellStyle {
                wrap: true,
                fill: Some(0xABCDEF),
                ..CellStyle::default()
            },
        );
    }
    let cells: Vec<(u32, u32, String)> = (0..30u32)
        .map(|r| (r, 0, "L1\nL2\nL3\nL4".to_string()))
        .collect();
    let sheets: Vec<OdsUnpacked> = vec![("Pages".to_string(), cells)];
    let multi_pdf = String::from_utf8_lossy(
        &encode_sheet_pdf("pages", &sheets, std::slice::from_ref(&styling), &[]).unwrap(),
    )
    .into_owned();
    let page_count =
        multi_pdf.matches("/Type /Page\n").count() + multi_pdf.matches("/Type /Page ").count();
    assert!(
        page_count >= 2,
        "30 four-line wrapped rows (30 * 58.5pt = 1755pt) must span >= 2 A4 pages, got {page_count}"
    );

    // Overlay test: row 0 expands to 45pt; TwoCell image spanning rows 0..=2 stretches,
    // while OneCell image anchored at row 1 keeps its pixel height (44px -> 33pt).
    let mut overlay_two = base_styling();
    overlay_two.columns = BTreeMap::from([(0, 64), (1, 100), (2, 100)]);
    overlay_two.cells.insert(
        (0, 0),
        CellStyle {
            wrap: true,
            ..CellStyle::default()
        },
    );
    overlay_two.images.push(SheetFloatingImage {
        id: "pic1".to_string(),
        name: "그림".to_string(),
        alt: String::new(),
        image: RichImage::new(png(), "").unwrap(),
        anchor: ChartAnchor {
            edit_as: AnchorEdit::TwoCell,
            from_col: 1,
            from_row: 0,
            to_col: 2,
            to_row: 2,
            width_px: 100,
            height_px: 44,
            ..ChartAnchor::default()
        },
    });
    let mut overlay_one = overlay_two.clone();
    overlay_one.images[0].anchor.edit_as = AnchorEdit::OneCell;
    overlay_one.images[0].anchor.from_row = 1;
    overlay_one.images[0].anchor.top_px = 22;
    overlay_one.charts.push(SheetChart {
        id: "ch1".to_string(),
        kind: ChartKind::Column,
        title: "C".to_string(),
        data_range: "A1:B2".to_string(),
        switch_row_col: false,
        legend: ChartLegendPos::None,
        anchor: ChartAnchor {
            edit_as: AnchorEdit::TwoCell,
            from_col: 1,
            from_row: 0,
            to_col: 2,
            to_row: 2,
            width_px: 120,
            height_px: 80,
            ..ChartAnchor::default()
        },
        series: vec![ChartSeriesRef {
            name: "V".to_string(),
            name_ref: None,
            categories_ref: None,
            values_ref: "S!$B$1:$B$2".to_string(),
        }],
    });

    let media_sheets: Vec<OdsUnpacked> = vec![(
        "S".to_string(),
        vec![
            (0, 0, "L1\nL2\nL3".to_string()),
            (0, 1, "10".to_string()),
            (1, 1, "20".to_string()),
            (2, 0, "end".to_string()),
        ],
    )];
    let pdf_two = String::from_utf8_lossy(
        &encode_sheet_pdf("two", &media_sheets, &[overlay_two], &[]).unwrap(),
    )
    .into_owned();
    let (_, _, _, h_two) = image_box(&pdf_two);
    // Row 0 = 45pt + Row 1 = 16.5pt => 61.5pt.
    assert!(
        (h_two - 61.5).abs() < 0.05,
        "TwoCell stretched height={h_two}"
    );

    let pdf_one = String::from_utf8_lossy(
        &encode_sheet_pdf("one", &media_sheets, &[overlay_one], &[]).unwrap(),
    )
    .into_owned();
    let (_, _, _, h_one) = image_box(&pdf_one);
    assert!(
        (h_one - 44.0 * PT_PER_PX).abs() < 0.05,
        "OneCell kept pixel height={h_one}"
    );
}

#[test]
fn hostile_huge_wrapped_cell_caps_at_409pt_deterministically_and_fast() {
    let mut styling = base_styling();
    styling.columns = BTreeMap::from([(0, 32)]);
    styling.cells.insert(
        (0, 0),
        CellStyle {
            wrap: true,
            fill: Some(0x445566),
            size_pt: Some(f64::NAN),
            ..CellStyle::default()
        },
    );
    let sheets: Vec<OdsUnpacked> = vec![("Hostile".to_string(), vec![(0, 0, "A".repeat(400_000))])];
    let started = std::time::Instant::now();
    let first = encode_sheet_pdf("hostile", &sheets, std::slice::from_ref(&styling), &[]).unwrap();
    assert!(started.elapsed().as_secs() < 2);
    assert_eq!(
        encode_sheet_pdf("hostile", &sheets, &[styling], &[]).unwrap(),
        first
    );
    let pdf = String::from_utf8_lossy(&first);
    assert!(!pdf.contains("NaN") && !pdf.contains("inf"));
    let rect = fill_rects(&pdf, 0x445566)[0];
    assert!((rect.3 - 409.0).abs() < 0.05, "capped at 409pt: {}", rect.3);
    let q_count = pdf.lines().filter(|l| l.starts_with("q ")).count();
    let cap_q_count = pdf
        .lines()
        .filter(|l| *l == "Q" || l.ends_with(" Q"))
        .count();
    assert_eq!(q_count, cap_q_count, "balanced q/Q");
}

#[test]
fn scaled_exact_fit_wrapped_line_does_not_split_and_clip_under_page_scale() {
    let mut styling = base_styling();
    styling.columns = BTreeMap::from([(0, 64)]);
    styling.page.scale = PageScale::Percent(30);
    styling.cells.insert(
        (0, 0),
        CellStyle {
            wrap: true,
            ..CellStyle::default()
        },
    );
    // 4 Korean chars = 40.0 pt in 48.0 pt col (room = 40.0 pt): exact single-line fit.
    let sheets: Vec<OdsUnpacked> =
        vec![("Scale30".to_string(), vec![(0, 0, "가나다라".to_string())])];
    let bytes = encode_sheet_pdf("scale30", &sheets, &[styling], &[]).unwrap();
    let pdf = String::from_utf8_lossy(&bytes);
    let tds = td_positions(&pdf);
    assert_eq!(
        tds.len(),
        1,
        "exact-fit line must stay 1 line at 30% scale: {tds:?}"
    );
}

#[test]
fn large_single_line_wrapped_cell_does_not_clip_descender_or_underline() {
    for v_align in [VAlign::Bottom, VAlign::Center] {
        let mut styling = base_styling();
        styling.columns = BTreeMap::from([(0, 160)]);
        styling.cells.insert(
            (0, 0),
            CellStyle {
                wrap: true,
                underline: true,
                v_align,
                size_pt: Some(36.0),
                fill: Some(0x112233),
                ..CellStyle::default()
            },
        );
        let sheets: Vec<OdsUnpacked> =
            vec![("BigSingle".to_string(), vec![(0, 0, "Ag".to_string())])];
        let bytes = encode_sheet_pdf("big-single", &sheets, &[styling], &[]).unwrap();
        let pdf = String::from_utf8_lossy(&bytes);
        let r0 = fill_rects(&pdf, 0x112233)[0];
        assert!(
            (r0.3 - 45.9).abs() < 0.05,
            "36pt * 1.275 = 45.9pt: {}",
            r0.3
        );
        assert!(
            !pdf.contains("re W n"),
            "auto-fitted 36pt single-line ({v_align:?}) should not clip descender/underline:\n{pdf}"
        );
    }
}

#[test]
fn narrow_wrapped_cell_with_icon_and_right_align_never_overlaps_icon() {
    for h_align in [HAlign::Right, HAlign::Center] {
        let mut styling = base_styling();
        styling.columns = BTreeMap::from([(0, 32)]);
        styling.cells.insert(
            (0, 0),
            CellStyle {
                wrap: true,
                h_align,
                fill: Some(0xEEEEEE),
                ..CellStyle::default()
            },
        );
        let mut shown = SheetShown::new();
        shown.insert(
            (0, 0),
            CellShown {
                rule: Some(CellRule {
                    icon: Some(CfIcon { set: 0, index: 2 }),
                    ..CellRule::default()
                }),
                ..CellShown::default()
            },
        );
        let sheets: Vec<OdsUnpacked> =
            vec![("NarrowIcon".to_string(), vec![(0, 0, "가".to_string())])];
        let bytes = encode_sheet_pdf("narrow-icon", &sheets, &[styling], &[shown]).unwrap();
        let pdf = String::from_utf8_lossy(&bytes);
        let cell_x = fill_rects(&pdf, 0xEEEEEE)[0].0;
        let tds = td_positions(&pdf);
        // Icon sits in [cell_x + 4.0, cell_x + 14.5] with text_inset = 13.5 -> text starts >= cell_x + 17.5.
        assert!(
            tds[0].0 >= cell_x + 17.48,
            "wrapped text ({h_align:?}) overlapped icon: tx={} vs cell_x={cell_x} in {tds:?}",
            tds[0].0
        );
    }
}

#[test]
fn absolute_and_pixel_only_overlays_and_hidden_row_spans_respect_wrapped_rows() {
    // 1) Absolute image at top_px = 44 stays at 44px = 33pt below sheet top even when row 0 wraps to 45pt.
    let mut abs_styling = base_styling();
    abs_styling.columns = BTreeMap::from([(0, 64), (1, 100)]);
    abs_styling.cells.insert(
        (0, 0),
        CellStyle {
            wrap: true,
            fill: Some(0x101010),
            ..CellStyle::default()
        },
    );
    abs_styling.images.push(SheetFloatingImage {
        id: "abs".to_string(),
        name: "abs".to_string(),
        alt: String::new(),
        image: RichImage::new(png(), "").unwrap(),
        anchor: ChartAnchor {
            edit_as: AnchorEdit::Absolute,
            left_px: 80,
            top_px: 44,
            width_px: 80,
            height_px: 40,
            ..ChartAnchor::default()
        },
    });
    let sheets = vec![(
        "Abs".to_string(),
        vec![
            (0, 0, "L1\nL2\nL3".to_string()),
            (1, 0, "10".to_string()),
            (2, 0, "20".to_string()),
        ],
    )];
    let abs_pdf =
        String::from_utf8_lossy(&encode_sheet_pdf("abs", &sheets, &[abs_styling], &[]).unwrap())
            .into_owned();
    let r0 = fill_rects(&abs_pdf, 0x101010)[0];
    let r0_top = r0.1 + r0.3;
    let (_, im_y, _, im_h) = image_box(&abs_pdf);
    let im_top = im_y + im_h;
    assert!(
        ((r0_top - im_top) - 44.0 * PT_PER_PX).abs() < 0.05,
        "Absolute image shifted by wrapped row 0: r0_top={r0_top}, im_top={im_top}"
    );
    assert!((im_h - 40.0 * PT_PER_PX).abs() < 0.05);

    // 2) Pixel-only chart (!has_explicit_grid()) at top_px = 44 stays at 33pt below sheet top.
    let mut px_chart_styling = base_styling();
    px_chart_styling.columns = BTreeMap::from([(0, 64), (1, 100)]);
    px_chart_styling.cells.insert(
        (0, 0),
        CellStyle {
            wrap: true,
            fill: Some(0x101010),
            ..CellStyle::default()
        },
    );
    px_chart_styling.charts.push(SheetChart {
        id: "px-ch".to_string(),
        kind: ChartKind::Column,
        title: "C".to_string(),
        data_range: "A2:A3".to_string(),
        switch_row_col: false,
        legend: ChartLegendPos::None,
        anchor: ChartAnchor {
            edit_as: AnchorEdit::TwoCell,
            left_px: 80,
            top_px: 44,
            width_px: 120,
            height_px: 80,
            ..ChartAnchor::default()
        },
        series: vec![ChartSeriesRef {
            name: "V".to_string(),
            name_ref: None,
            categories_ref: None,
            values_ref: "Abs!$A$2:$A$3".to_string(),
        }],
    });
    let px_pdf = String::from_utf8_lossy(
        &encode_sheet_pdf("px-ch", &sheets, &[px_chart_styling], &[]).unwrap(),
    )
    .into_owned();
    let r0_px = fill_rects(&px_pdf, 0x101010)[0];
    let card_rects = fill_rects(&px_pdf, 0xFFFFFF);
    let card = card_rects.last().expect("chart card fill rect");
    let card_top = card.1 + card.3;
    assert!(
        (((r0_px.1 + r0_px.3) - card_top) - 44.0 * PT_PER_PX).abs() < 0.05,
        "pixel-only chart shifted by wrapped row 0: r0_top={}, card_top={card_top}",
        r0_px.1 + r0_px.3
    );

    // 3) Hidden row 1 between visible rows 0 and 2: TwoCell image spanning rows 0..=1 has height of row 0 (45pt),
    // not stretching down to bottom of row 2!
    let mut hid_styling = base_styling();
    hid_styling.columns = BTreeMap::from([(0, 64), (1, 100)]);
    hid_styling.hidden_rows = vec![(1, 1)];
    hid_styling.cells.insert(
        (0, 0),
        CellStyle {
            wrap: true,
            ..CellStyle::default()
        },
    );
    hid_styling.images.push(SheetFloatingImage {
        id: "hid".to_string(),
        name: "hid".to_string(),
        alt: String::new(),
        image: RichImage::new(png(), "").unwrap(),
        anchor: ChartAnchor {
            edit_as: AnchorEdit::TwoCell,
            from_col: 1,
            from_row: 0,
            to_col: 2,
            to_row: 1,
            width_px: 100,
            height_px: 20,
            ..ChartAnchor::default()
        },
    });
    let hid_pdf =
        String::from_utf8_lossy(&encode_sheet_pdf("hid", &sheets, &[hid_styling], &[]).unwrap())
            .into_owned();
    let (_, _, _, h_hid) = image_box(&hid_pdf);
    assert!(
        (h_hid - 45.0).abs() < 0.05,
        "TwoCell image ending at hidden row 1 should be 45pt, got {h_hid}"
    );

    // 4) Non-zero print_area (B3:D6 -> area.r1=2, area.c1=1), mixed custom:false + custom:true,
    // PageScale::Fit { width: 1, height: 1 }, zero-room narrow column with icon, and pixel-only image.
    let mut area_st = base_styling();
    area_st.columns = BTreeMap::from([(1, 24), (2, 64), (3, 100)]);
    area_st.row_heights = [
        (2, RowHeight::parse("15", false).unwrap()),
        (3, RowHeight::parse("20", true).unwrap()),
    ]
    .into();
    area_st.page.print_area = vec![CellBox {
        row0: 2,
        col0: 1,
        row1: 5,
        col1: 3,
    }];
    area_st.page.scale = PageScale::Fit {
        width: 1,
        height: 1,
    };
    area_st.cells.insert(
        (2, 1),
        CellStyle {
            wrap: true,
            fill: Some(0x111111),
            ..CellStyle::default()
        },
    );
    area_st.cells.insert(
        (2, 2),
        CellStyle {
            wrap: true,
            fill: Some(0x222222),
            ..CellStyle::default()
        },
    );
    area_st.cells.insert(
        (3, 2),
        CellStyle {
            wrap: true,
            fill: Some(0x333333),
            ..CellStyle::default()
        },
    );
    area_st.images.push(SheetFloatingImage {
        id: "px-img".to_string(),
        name: "px-img".to_string(),
        alt: String::new(),
        image: RichImage::new(png(), "").unwrap(),
        anchor: ChartAnchor {
            edit_as: AnchorEdit::TwoCell,
            left_px: 100,
            top_px: 64,
            width_px: 60,
            height_px: 30,
            ..ChartAnchor::default()
        },
    });
    let mut area_shown = SheetShown::new();
    area_shown.insert(
        (2, 1),
        CellShown {
            rule: Some(CellRule {
                icon: Some(CfIcon { set: 0, index: 1 }),
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    let area_sheets = vec![(
        "AreaFit".to_string(),
        vec![
            (2, 1, "SkipZeroRoom".to_string()),
            (2, 2, "L1\nL2\nL3".to_string()),
            (3, 2, "C1\nC2\nC3".to_string()),
            (5, 3, "End".to_string()),
        ],
    )];
    let area_pdf = String::from_utf8_lossy(
        &encode_sheet_pdf("area-fit", &area_sheets, &[area_st], &[area_shown]).unwrap(),
    )
    .into_owned();
    // Col 1 is 24px = 18pt, pad=4pt + icon_inset=13.5pt -> room=0pt -> SkipZeroRoom is omitted cleanly without expanding row 2;
    // Col 2 expands row 2 to 45pt; row 3 has custom=true 20pt.
    assert!(!area_pdf.contains("SkipZeroRoom"));
    let r2 = fill_rects(&area_pdf, 0x222222)[0];
    let r3 = fill_rects(&area_pdf, 0x333333)[0];
    assert!((r2.3 - 45.0).abs() < 0.05, "area row 2 expanded: {}", r2.3);
    assert!(
        (r3.3 - 20.0).abs() < 0.05,
        "area row 3 custom stayed 20pt: {}",
        r3.3
    );
    assert!(
        area_pdf.contains("/Im1 Do"),
        "pixel-only image inside print_area rendered"
    );

    // 5) Repeated title_rows = Some((0, 0)) across multiple pages with a TwoCell image spanning rows 1..=20:
    // on page 2, y_top of /Im1 lies ABOVE row 20 (not collapsed to page bottom!).
    let mut title_st = base_styling();
    title_st.columns = BTreeMap::from([(0, 64), (1, 100)]);
    title_st.page.title_rows = Some((0, 0));
    for r in 0..30u32 {
        title_st.cells.insert(
            (r, 0),
            CellStyle {
                wrap: true,
                fill: if r == 20 { Some(0x556677) } else { None },
                ..CellStyle::default()
            },
        );
    }
    title_st.images.push(SheetFloatingImage {
        id: "span".to_string(),
        name: "span".to_string(),
        alt: String::new(),
        image: RichImage::new(png(), "").unwrap(),
        anchor: ChartAnchor {
            edit_as: AnchorEdit::TwoCell,
            from_col: 1,
            from_row: 1,
            to_col: 2,
            to_row: 20,
            width_px: 80,
            height_px: 40,
            ..ChartAnchor::default()
        },
    });
    let title_cells: Vec<(u32, u32, String)> = (0..30u32)
        .map(|r| (r, 0, "A\nB\nC\nD".to_string()))
        .collect();
    let title_pdf = String::from_utf8_lossy(
        &encode_sheet_pdf(
            "title-span",
            &[("T".to_string(), title_cells)],
            &[title_st],
            &[],
        )
        .unwrap(),
    )
    .into_owned();
    let page2_split = title_pdf
        .rfind("q 0.333 0.400 0.467 rg")
        .expect("row 20 fill on page 2");
    let (_, im2_y, _, im2_h) = image_box(&title_pdf[page2_split..]);
    let r20 = fill_rects(&title_pdf, 0x556677)[0];
    // On page 2, image starts at row 1 (well above row 20's top) and ends at row 20's top.
    assert!(
        im2_y + im2_h > r20.1 + r20.3 + 50.0,
        "page 2 image top ({}) must sit above row 20 ({})",
        im2_y + im2_h,
        r20.1 + r20.3
    );
    assert!((im2_y - (r20.1 + r20.3)).abs() < 0.05);
}

#[test]
fn hostile_multi_million_char_wrapped_cell_does_not_overflow_mixed_units() {
    let mut styling = base_styling();
    styling.columns = BTreeMap::from([(0, 64)]);
    styling.cells.insert(
        (0, 0),
        CellStyle {
            wrap: true,
            fill: Some(0x334455),
            ..CellStyle::default()
        },
    );
    // 4.6M ASCII 'W' chars * 944 units = 4.34B > u32::MAX (panics in debug, wraps to small u32 in release!).
    let sheets: Vec<OdsUnpacked> = vec![("Huge".to_string(), vec![(0, 0, "W".repeat(4_600_000))])];
    let started = std::time::Instant::now();
    let bytes = encode_sheet_pdf("huge-cjk", &sheets, &[styling], &[]).unwrap();
    assert!(started.elapsed().as_secs() < 2);
    let pdf = String::from_utf8_lossy(&bytes);
    let rect = fill_rects(&pdf, 0x334455)[0];
    assert!((rect.3 - 409.0).abs() < 0.05, "capped at 409pt: {}", rect.3);
}

//! Integration tests for sheet PDF conditional formatting icon sets (`PD-342`).

use easyoffice_formats::{
    encode_sheet_pdf, BorderEdge, BorderLine, CellBorders, CellRule, CellShown, CellStyle, CfBar,
    CfIcon, DefaultFont, OdsUnpacked, PageScale, RowHeight, SheetShown, SheetStyling, ICON_SETS,
};

const CELL_W: f64 = 48.0;
const CELL_H: f64 = 16.5;

fn base_styling() -> SheetStyling {
    SheetStyling {
        default_font: DefaultFont::kept(Some("Calibri"), Some(11.0)),
        ..SheetStyling::default()
    }
}

fn parse_icon_draws(pdf: &str) -> Vec<(f64, f64, f64, f64, String)> {
    pdf.lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("q ")?.strip_suffix(" Do Q")?;
            let (mat, name) = rest.split_once(" cm /")?;
            let nums: Vec<f64> = mat
                .split(' ')
                .map(|w| w.parse::<f64>().ok())
                .collect::<Option<Vec<_>>>()?;
            if nums.len() != 6 {
                return None;
            }
            Some((nums[0], nums[3], nums[4], nums[5], name.to_string()))
        })
        .collect()
}

fn parse_td_positions(pdf: &str) -> Vec<(f64, f64, String)> {
    pdf.lines()
        .filter_map(|line| {
            let td_idx = line.find(" Td ")?;
            let before = &line[..td_idx];
            let words: Vec<&str> = before.split(' ').collect();
            let y: f64 = words.last()?.parse().ok()?;
            let x: f64 = words.get(words.len().checked_sub(2)?)?.parse().ok()?;
            let text_start = line.find('(')?;
            let text_end = line.rfind(") Tj")?;
            Some((x, y, line[text_start + 1..text_end].to_string()))
        })
        .collect()
}

#[test]
fn all_20_icon_sets_emit_deduplicated_form_xobjects_and_placements() {
    let mut cells = Vec::new();
    let mut shown = SheetShown::new();
    let mut total_icons = 0usize;
    for (set_idx, &(_name, count)) in ICON_SETS.iter().enumerate() {
        for index in 0..count {
            let row = set_idx as u32;
            let col = u32::from(index);
            cells.push((row, col, format!("{}", index + 1)));
            shown.insert(
                (row, col),
                CellShown {
                    numeric: true,
                    rule: Some(CellRule {
                        icon: Some(CfIcon {
                            set: set_idx as u8,
                            index,
                        }),
                        ..CellRule::default()
                    }),
                    ..CellShown::default()
                },
            );
            total_icons += 1;
        }
    }
    let sheets: Vec<OdsUnpacked> = vec![("AllIcons".to_string(), cells)];
    let bytes = encode_sheet_pdf("all-icons", &sheets, &[base_styling()], &[shown]).unwrap();
    let pdf = String::from_utf8_lossy(&bytes);

    let draws = parse_icon_draws(&pdf);
    assert_eq!(draws.len(), total_icons);
    assert!(draws.iter().all(|&(kx, ky, _, _, _)| {
        (kx - 10.5 / 24.0).abs() < 1e-4 && (ky - 10.5 / 24.0).abs() < 1e-4
    }));

    // Total unique (shape, color) glyphs across all 20 icon sets is 50 (e.g. 3Symbols == 3Symbols2,
    // arrows share endpoints, etc.), strictly fewer than total_icons (76).
    let form_count = pdf.matches("/Subtype /Form").count();
    assert!(
        form_count > 30 && form_count < total_icons,
        "unexpected form count: {form_count} vs {total_icons}"
    );
    assert_eq!(pdf.matches("/BBox [0 0 24 24]").count(), form_count);
    assert!(!pdf.contains("NaN") && !pdf.contains("inf"));
}

#[test]
fn repeated_icons_across_pages_share_document_forms_and_stay_deterministic() {
    let mut cells = Vec::new();
    let mut shown = SheetShown::new();
    for r in 0..80u32 {
        for c in 0..16u32 {
            cells.push((r, c, format!("{}", (r + c) % 3)));
            shown.insert(
                (r, c),
                CellShown {
                    numeric: true,
                    rule: Some(CellRule {
                        icon: Some(CfIcon {
                            set: 0,
                            index: ((r + c) % 3) as u8,
                        }),
                        ..CellRule::default()
                    }),
                    ..CellShown::default()
                },
            );
        }
    }
    let sheets: Vec<OdsUnpacked> = vec![("MultiPage".to_string(), cells)];
    let stylings = vec![base_styling()];
    let showns = vec![shown];

    let first = encode_sheet_pdf("det", &sheets, &stylings, &showns).unwrap();
    for _ in 0..4 {
        assert_eq!(
            encode_sheet_pdf("det", &sheets, &stylings, &showns).unwrap(),
            first
        );
    }
    let pdf = String::from_utf8_lossy(&first);
    assert!(
        pdf.matches("/Type /Page\n").count() + pdf.matches("/Type /Page ").count() >= 2,
        "expected multi-page PDF"
    );
    // Only 3 Form XObjects emitted for all 1,280 cells across all pages.
    assert_eq!(pdf.matches("/Subtype /Form").count(), 3);
}

#[test]
fn icon_and_value_coexist_without_overlap_for_numeric_and_text_cells() {
    let cells = vec![
        (0, 0, "1234567890".to_string()),
        (1, 0, "StatusText".to_string()),
        (2, 0, "Plain".to_string()),
    ];
    let mut shown = SheetShown::new();
    shown.insert(
        (0, 0),
        CellShown {
            numeric: true,
            rule: Some(CellRule {
                bg: Some(0xEEF4FF),
                icon: Some(CfIcon { set: 0, index: 2 }),
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    shown.insert(
        (1, 0),
        CellShown {
            numeric: false,
            rule: Some(CellRule {
                icon: Some(CfIcon { set: 2, index: 1 }),
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    let sheets: Vec<OdsUnpacked> = vec![("S".to_string(), cells)];
    let bytes = encode_sheet_pdf("overlap", &sheets, &[base_styling()], &[shown]).unwrap();
    let pdf = String::from_utf8_lossy(&bytes);

    let draws = parse_icon_draws(&pdf);
    assert_eq!(draws.len(), 2);
    let tds = parse_td_positions(&pdf);
    assert_eq!(tds.len(), 3);

    // Row 0 (numeric): icon at x0 + 4.0, width 10.5 -> right edge at x0 + 14.5; gap 3.0 -> text >= x0 + 17.5.
    let icon0_x = draws[0].2;
    let icon0_right = icon0_x + draws[0].0 * 24.0;
    let (num_x, _, ref num_text) = tds[0];
    assert!(
        num_x >= icon0_right + 3.0 - 0.02,
        "numeric text overlapped icon: num_x={num_x}, icon_right={icon0_right}, text={num_text}"
    );
    // Truncated to fit remaining width (48 - 8 - 13.5 = 26.5pt), so fewer than all 10 digits.
    assert!(
        num_text.len() < 10 && !num_text.is_empty(),
        "expected truncated numeric text, got {num_text}"
    );

    // Row 1 (left-aligned text): starts at icon_right + 3.0.
    let icon1_right = draws[1].2 + draws[1].0 * 24.0;
    let (txt_x, _, ref txt_val) = tds[1];
    assert!(
        (txt_x - (icon1_right + 3.0)).abs() < 0.02,
        "left text not offset after icon: txt_x={txt_x}, expected={}, val={txt_val}",
        icon1_right + 3.0
    );

    // Row 2 (plain cell without icon): starts at cell left + pad = icon0_x.
    assert!((tds[2].0 - icon0_x).abs() < 0.02);
}

#[test]
fn hide_value_hides_text_and_centers_icon_in_cell() {
    let cells = vec![(0, 0, "55".to_string()), (1, 0, "99".to_string())];
    let mut shown = SheetShown::new();
    shown.insert(
        (0, 0),
        CellShown {
            numeric: true,
            rule: Some(CellRule {
                bg: Some(0xDDEEFF),
                icon: Some(CfIcon { set: 3, index: 2 }),
                hide_value: true,
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    shown.insert(
        (1, 0),
        CellShown {
            numeric: true,
            rule: Some(CellRule {
                icon: Some(CfIcon { set: 3, index: 0 }),
                hide_value: false,
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    let sheets: Vec<OdsUnpacked> = vec![("S".to_string(), cells)];
    let bytes = encode_sheet_pdf("hide", &sheets, &[base_styling()], &[shown]).unwrap();
    let pdf = String::from_utf8_lossy(&bytes);

    assert!(!pdf.contains("(55) Tj"));
    assert!(pdf.contains("(99) Tj"));

    let draws = parse_icon_draws(&pdf);
    assert_eq!(draws.len(), 2);
    let centered_x = draws[0].2;
    let left_x = draws[1].2;
    let expected_shift = (CELL_W - 10.5) * 0.5 - 4.0;
    assert!(
        ((centered_x - left_x) - expected_shift).abs() < 0.02,
        "centered_x={centered_x}, left_x={left_x}, expected_shift={expected_shift}"
    );
}

#[test]
fn rating_bars_emit_alpha_extgstate_only_when_faint_bars_exist() {
    let cells = vec![(0, 0, "1".to_string()), (1, 0, "4".to_string())];
    let mut shown = SheetShown::new();
    shown.insert(
        (0, 0),
        CellShown {
            numeric: true,
            rule: Some(CellRule {
                icon: Some(CfIcon { set: 13, index: 0 }), // 4Rating level 1 (has 3 faint bars)
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    let partial = encode_sheet_pdf(
        "rating-partial",
        &[("S".to_string(), cells.clone())],
        &[base_styling()],
        &[shown],
    )
    .unwrap();
    let partial_pdf = String::from_utf8_lossy(&partial);
    assert!(partial_pdf.contains("/ExtGState"));
    assert!(partial_pdf.contains("/ca 0.3"));
    assert!(partial_pdf.contains("q /A3 gs"));

    let mut full_shown = SheetShown::new();
    full_shown.insert(
        (1, 0),
        CellShown {
            numeric: true,
            rule: Some(CellRule {
                icon: Some(CfIcon { set: 13, index: 3 }), // 4Rating level 4 (all solid)
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    let full = encode_sheet_pdf(
        "rating-full",
        &[("S".to_string(), cells)],
        &[base_styling()],
        &[full_shown],
    )
    .unwrap();
    let full_pdf = String::from_utf8_lossy(&full);
    assert!(!full_pdf.contains("/ExtGState"));
    assert!(!full_pdf.contains("/A3 gs"));
}

#[test]
fn fill_bar_icon_text_grid_and_border_follow_layer_order() {
    let cells = vec![(0, 0, "75".to_string())];
    let mut shown = SheetShown::new();
    shown.insert(
        (0, 0),
        CellShown {
            numeric: true,
            rule: Some(CellRule {
                bg: Some(0x112233),
                bar: Some(CfBar {
                    pct: 60,
                    start: 0,
                    color: 0x445566,
                    gradient: false,
                }),
                icon: Some(CfIcon { set: 18, index: 3 }), // 5Quarters 3/4 pie
                border: CellBorders {
                    bottom: BorderEdge::new(BorderLine::Thin, 0xFF0000),
                    ..CellBorders::NONE
                },
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    let mut styling = base_styling();
    styling.page.grid_lines = true;
    let bytes =
        encode_sheet_pdf("z-order", &[("S".to_string(), cells)], &[styling], &[shown]).unwrap();
    let pdf = String::from_utf8_lossy(&bytes);

    let fill_at = pdf.find("0.067 0.133 0.200 rg").unwrap();
    let bar_at = pdf.find("0.267 0.333 0.400 rg").unwrap();
    let icon_at = pdf.find("/Ic1 Do").unwrap();
    let text_at = pdf.find("(75) Tj").unwrap();
    let grid_at = pdf.find("0.5 w").unwrap();
    let border_at = pdf.find("1.000 0.000 0.000 RG").unwrap();
    assert!(
        fill_at < bar_at
            && bar_at < icon_at
            && icon_at < text_at
            && text_at < grid_at
            && grid_at < border_at,
        "z-order broken: fill={fill_at} bar={bar_at} icon={icon_at} text={text_at} grid={grid_at} border={border_at}"
    );
}

#[test]
fn narrow_short_scaled_and_hostile_icon_rules_stay_bounded_and_fail_safe() {
    let mut styling = base_styling();
    styling.columns.insert(0, 12); // narrow 9pt column
    styling.row_heights.insert(
        0,
        RowHeight {
            centipt: 600,
            custom: true,
        },
    ); // short 6pt row
    styling.page.scale = PageScale::Percent(10);

    let cells = vec![
        (0, 0, "1".to_string()),
        (1, 0, "2".to_string()),
        (2, 1, "OK".to_string()),
        (3, 2, "Tiny".to_string()),
    ];
    styling.columns.insert(2, 1); // 0.075pt at 10% scale -> skipped cleanly without panic/NaN
    let mut shown = SheetShown::new();
    shown.insert(
        (0, 0),
        CellShown {
            numeric: true,
            rule: Some(CellRule {
                icon: Some(CfIcon { set: 8, index: 1 }), // star-1
                hide_value: false,
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    shown.insert(
        (1, 0),
        CellShown {
            numeric: true,
            rule: Some(CellRule {
                icon: Some(CfIcon { set: 19, index: 2 }), // box-2
                hide_value: true,
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    // Hostile out-of-range icon in normal column B with hide_value=true must draw no icon and must NOT hide text.
    shown.insert(
        (2, 1),
        CellShown {
            numeric: false,
            rule: Some(CellRule {
                icon: Some(CfIcon { set: 20, index: 0 }),
                hide_value: true,
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    // Valid icon in ultra-tiny 1px column at 10% scale (w=0.075pt < MIN_ICON_PT) must skip cleanly.
    shown.insert(
        (3, 2),
        CellShown {
            numeric: false,
            rule: Some(CellRule {
                icon: Some(CfIcon { set: 0, index: 0 }),
                hide_value: false,
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    let bytes = encode_sheet_pdf(
        "hostile-icon",
        &[("S".to_string(), cells)],
        &[styling],
        &[shown],
    )
    .unwrap();
    let pdf = String::from_utf8_lossy(&bytes);
    assert!(!pdf.contains("NaN") && !pdf.contains("inf"));
    assert!(pdf.contains("(OK) Tj"), "invalid icon must not hide value");
    let draws = parse_icon_draws(&pdf);
    // Row 0 (w=0.9pt at 10% scale -> pad_x=0.225, size=0.45pt) and Row 1 (centered size=0.85pt) both draw;
    // Row 2 (invalid icon) and Row 3 (w=0.075pt) draw nothing.
    assert_eq!(draws.len(), 2, "expected 2 bounded icon draws: {draws:?}");
    assert!((draws[0].0 * 24.0 - 0.45).abs() < 1e-3, "{draws:?}");
    assert!((draws[1].0 * 24.0 - 0.85).abs() < 1e-3, "{draws:?}");
    for &(kx, ky, ix, iy, _) in &draws {
        assert!(kx > 0.0 && ky > 0.0 && ix >= 0.0 && iy >= 0.0);
        assert!(ix + kx * 24.0 < 600.0 && iy + ky * 24.0 < 900.0);
    }
    let _ = CELL_H;
}

#[test]
fn large_cell_font_with_decorations_fill_repeat_and_dark_fill_coexist_with_icons() {
    let mut styling = base_styling();
    styling.row_heights.insert(
        0,
        RowHeight {
            centipt: 6000,
            custom: true,
        },
    ); // 60pt row
    styling.columns.insert(0, 120); // 90pt wide column
    styling.cells.insert(
        (0, 0),
        CellStyle {
            size_pt: Some(36.0),
            bold: true,
            italic: true,
            underline: true,
            strike: true,
            ..CellStyle::default()
        },
    );
    let cells = vec![(0, 0, "A".to_string()), (1, 0, "10".to_string())];
    let mut shown = SheetShown::new();
    shown.insert(
        (0, 0),
        CellShown {
            numeric: false,
            rule: Some(CellRule {
                bg: Some(0x101820), // dark fill -> auto white text
                icon: Some(CfIcon { set: 0, index: 2 }),
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    shown.insert(
        (1, 0),
        CellShown {
            numeric: true,
            fill: Some((0, '.')), // * . repeat fill before 10
            rule: Some(CellRule {
                icon: Some(CfIcon { set: 14, index: 3 }),
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    let bytes = encode_sheet_pdf(
        "large-font-icon",
        &[("S".to_string(), cells)],
        &[styling],
        &[shown],
    )
    .unwrap();
    let pdf = String::from_utf8_lossy(&bytes);
    assert!(!pdf.contains("NaN") && !pdf.contains("inf"));
    // Auto white text color + 36pt italic/bold + underline & strike lines on row 0 without overlapping icon at left.
    assert!(pdf.contains("1.000 1.000 1.000 rg 1.000 1.000 1.000 RG"));
    assert!(pdf.contains("/F1 36.00 Tf"));
    assert_eq!(
        pdf.lines()
            .filter(|l| l.contains("1.000 1.000 1.000 RG") && l.contains(" m "))
            .count(),
        2
    );
    let draws = parse_icon_draws(&pdf);
    assert_eq!(draws.len(), 2);
    let tds = parse_td_positions(&pdf);
    assert_eq!(tds.len(), 2);
    // Row 0 left-aligned 36pt text starts after icon + 3pt gap.
    let icon0_right = draws[0].2 + draws[0].0 * 24.0;
    assert!(tds[0].0 >= icon0_right + 3.0 - 0.02, "{tds:?} vs {draws:?}");
    // Row 1 repeat-fill right-aligned text stays strictly to the right of icon + 3pt gap.
    let icon1_right = draws[1].2 + draws[1].0 * 24.0;
    assert!(tds[1].0 >= icon1_right + 3.0 - 0.02, "{tds:?} vs {draws:?}");
    assert!(tds[1].2.starts_with('.') && tds[1].2.ends_with("10"));
}

#[test]
fn multi_sheet_workbook_shares_icon_forms_and_isolates_alpha_extgstate() {
    let sheets: Vec<OdsUnpacked> = vec![
        ("Sheet1".to_string(), vec![(0, 0, "1".to_string())]),
        (
            "Sheet2".to_string(),
            vec![(0, 0, "2".to_string()), (1, 0, "3".to_string())],
        ),
    ];
    let stylings = vec![base_styling(), base_styling()];
    let mut shown1 = SheetShown::new();
    shown1.insert(
        (0, 0),
        CellShown {
            numeric: true,
            rule: Some(CellRule {
                icon: Some(CfIcon { set: 0, index: 2 }), // Up GREEN
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    let mut shown2 = SheetShown::new();
    shown2.insert(
        (0, 0),
        CellShown {
            numeric: true,
            rule: Some(CellRule {
                icon: Some(CfIcon { set: 0, index: 2 }), // Shared Up GREEN
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    shown2.insert(
        (1, 0),
        CellShown {
            numeric: true,
            rule: Some(CellRule {
                icon: Some(CfIcon { set: 13, index: 1 }), // 4Rating level 2 (has /A3)
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    let bytes = encode_sheet_pdf("multi-sheet", &sheets, &stylings, &[shown1, shown2]).unwrap();
    let pdf = String::from_utf8_lossy(&bytes);
    // Two distinct icon keys total across both sheets; only the 4Rating form has /ExtGState /A3.
    assert_eq!(pdf.matches("/Subtype /Form").count(), 2);
    assert_eq!(pdf.matches("/ca 0.3").count(), 1);
    assert_eq!(pdf.matches(" Do Q").count(), 3);
}

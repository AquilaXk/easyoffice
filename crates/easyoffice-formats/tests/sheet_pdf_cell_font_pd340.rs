//! The engine's sheet PDF renders each cell's own font style (`bold`, `italic`, `underline`,
//! `strike`, `size_pt`), combines cell font flags with conditional-format flags, and clips
//! oversized fonts inside the cell box (PD-340).

use std::collections::BTreeMap;

use easyoffice_formats::{
    encode_sheet_pdf, CellRule, CellShown, CellStyle, DefaultFont, OdsUnpacked, PageScale,
    RowHeight, SheetShown, SheetStyling, FLAG_ITALIC, FLAG_STRIKE, MAX_SIZE_PT, MIN_SIZE_PT,
};

fn sheet_with_cells(
    cells: Vec<(u32, u32, String)>,
    styles: Vec<((u32, u32), CellStyle)>,
) -> (Vec<OdsUnpacked>, SheetStyling) {
    let mut styling = SheetStyling {
        default_font: DefaultFont::kept(Some("Calibri"), Some(11.0)),
        ..SheetStyling::default()
    };
    for (at, style) in styles {
        styling.cells.insert(at, style);
    }
    (vec![("S".to_string(), cells)], styling)
}

fn render(cells: Vec<(u32, u32, String)>, styles: Vec<((u32, u32), CellStyle)>) -> String {
    let (sheets, styling) = sheet_with_cells(cells, styles);
    let bytes = encode_sheet_pdf("cell-font", &sheets, &[styling], &[]).unwrap();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn line_with<'a>(pdf: &'a str, needle: &str) -> Vec<&'a str> {
    pdf.lines().filter(|l| l.contains(needle)).collect()
}

fn text_y_of(pdf: &str, needle: &str) -> f64 {
    let line = line_with(pdf, needle)[0];
    let words: Vec<&str> = line.split(' ').collect();
    let at = words.iter().position(|w| *w == "Td").unwrap();
    words[at - 1].parse().unwrap()
}

fn text_x_of(pdf: &str, needle: &str) -> f64 {
    let line = line_with(pdf, needle)[0];
    let words: Vec<&str> = line.split(' ').collect();
    let at = words.iter().position(|w| *w == "Td").unwrap();
    words[at - 2].parse().unwrap()
}

fn rgb(value: u32) -> String {
    let c = |shift: u32| f64::from((value >> shift) & 0xFF) / 255.0;
    format!("{:.3} {:.3} {:.3}", c(16), c(8), c(0))
}

#[test]
fn cell_own_bold_italic_underline_and_strike_render_without_conditional_rules() {
    let base_cells = vec![(0, 0, "Alpha".to_string())];
    let plain = render(base_cells.clone(), vec![]);
    assert!(!plain.contains(" Tr") && !plain.contains("0.21 1"));

    let bold = render(
        base_cells.clone(),
        vec![(
            (0, 0),
            CellStyle {
                bold: true,
                ..CellStyle::default()
            },
        )],
    );
    assert_eq!(line_with(&bold, " 2 Tr 0.30 w ").len(), 1, "{bold}");

    let italic = render(
        base_cells.clone(),
        vec![(
            (0, 0),
            CellStyle {
                italic: true,
                ..CellStyle::default()
            },
        )],
    );
    assert!(!italic.contains(" Tr"));
    assert_eq!(line_with(&italic, " 0.21 1 ").len(), 1, "{italic}");

    let underlined = render(
        base_cells.clone(),
        vec![(
            (0, 0),
            CellStyle {
                underline: true,
                ..CellStyle::default()
            },
        )],
    );
    let struck = render(
        base_cells.clone(),
        vec![(
            (0, 0),
            CellStyle {
                strike: true,
                ..CellStyle::default()
            },
        )],
    );
    let plain_strokes = line_with(&plain, " m ").len();
    assert_eq!(line_with(&underlined, " m ").len() - plain_strokes, 1);
    assert_eq!(line_with(&struck, " m ").len() - plain_strokes, 1);

    let y_of_extra_line = |pdf: &str| -> f64 {
        let extra: Vec<&str> = pdf
            .lines()
            .filter(|l| l.contains(" m ") && !plain.contains(l))
            .collect();
        let words: Vec<&str> = extra[0].split(' ').collect();
        let at = words.iter().position(|w| *w == "m").unwrap();
        words[at - 1].parse().unwrap()
    };
    let baseline = text_y_of(&plain, "(Alpha) Tj");
    assert!(y_of_extra_line(&underlined) < baseline);
    assert!(y_of_extra_line(&struck) > baseline);

    let all_four = render(
        base_cells,
        vec![(
            (0, 0),
            CellStyle {
                bold: true,
                italic: true,
                underline: true,
                strike: true,
                font_color: Some(0x123456),
                ..CellStyle::default()
            },
        )],
    );
    let color = rgb(0x123456);
    assert!(
        all_four.contains(&format!("{color} rg {color} RG 1 0 0.21 1")),
        "{all_four}"
    );
    assert!(all_four.contains(" 2 Tr 0.30 w "), "{all_four}");
    assert_eq!(line_with(&all_four, " m ").len() - plain_strokes, 2);
    assert_eq!(
        line_with(&all_four, &format!("{color} RG")).len(),
        3,
        "bold text stroke plus underline and strike strokes share cell font color"
    );
}

#[test]
fn cell_flags_merge_with_conditional_rule_flags_and_adjacent_cells_coalesce() {
    let cells = vec![
        (0, 0, "A".to_string()),
        (0, 1, "B".to_string()),
        (1, 0, "C".to_string()),
    ];
    let (sheets, mut styling) = sheet_with_cells(
        cells,
        vec![
            (
                (0, 0),
                CellStyle {
                    bold: true,
                    font_color: Some(0x112233),
                    ..CellStyle::default()
                },
            ),
            (
                (0, 1),
                CellStyle {
                    bold: true,
                    font_color: Some(0x112233),
                    ..CellStyle::default()
                },
            ),
            (
                (1, 0),
                CellStyle {
                    bold: true,
                    underline: true,
                    ..CellStyle::default()
                },
            ),
        ],
    );
    let mut shown = SheetShown::new();
    shown.insert(
        (1, 0),
        CellShown {
            rule: Some(CellRule {
                fg: Some(0x9C0006),
                flags: FLAG_ITALIC | FLAG_STRIKE,
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    styling.page.grid_lines = false;
    let bytes = encode_sheet_pdf("merged", &sheets, &[styling], &[shown]).unwrap();
    let pdf = String::from_utf8_lossy(&bytes);

    let shared_bold = format!("q {} rg {} RG 2 Tr 0.30 w BT", rgb(0x112233), rgb(0x112233));
    assert_eq!(pdf.matches(&shared_bold).count(), 1, "{pdf}");
    let run_start = pdf.find(&shared_bold).unwrap();
    let run = &pdf[run_start..run_start + pdf[run_start..].find(" Q").unwrap()];
    assert_eq!(run.matches(" Tj").count(), 2, "{run}");

    let rule_color = rgb(0x9C0006);
    assert!(
        pdf.contains(&format!("{rule_color} rg {rule_color} RG 1 0 0.21 1")),
        "rule fg overrides cell fg while keeping cell bold + rule italic"
    );
    assert_eq!(
        line_with(&pdf, &format!("{rule_color} RG")).len(),
        3,
        "bold stroke + cell underline + rule strike all in rule fg"
    );
}

#[test]
fn hidden_rule_value_suppresses_cell_text_and_cell_underline() {
    let (sheets, styling) = sheet_with_cells(
        vec![(0, 0, "Secret".to_string()), (0, 1, "Shown".to_string())],
        vec![(
            (0, 0),
            CellStyle {
                bold: true,
                underline: true,
                strike: true,
                size_pt: Some(16.0),
                ..CellStyle::default()
            },
        )],
    );
    let mut shown = SheetShown::new();
    shown.insert(
        (0, 0),
        CellShown {
            rule: Some(CellRule {
                hide_value: true,
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    let bytes = encode_sheet_pdf("hidden", &sheets, &[styling], &[shown]).unwrap();
    let pdf = String::from_utf8_lossy(&bytes);
    assert!(!pdf.contains("(Secret) Tj") && !pdf.contains("16.00 Tf"));
    assert!(
        !pdf.contains(" m "),
        "no underline or strike for hidden value"
    );
    assert!(pdf.contains("(Shown) Tj"));
}

#[test]
fn cell_size_pt_scales_font_fit_right_alignment_fill_and_page_scale() {
    let cells = vec![
        (0, 0, "Big".to_string()),
        (0, 1, "WWWWWWWWWWWW".to_string()),
        (1, 0, "9".to_string()),
        (1, 1, "9".to_string()),
    ];
    let (sheets, mut styling) = sheet_with_cells(
        cells,
        vec![
            (
                (0, 0),
                CellStyle {
                    bold: true,
                    underline: true,
                    size_pt: Some(16.0),
                    ..CellStyle::default()
                },
            ),
            (
                (0, 1),
                CellStyle {
                    size_pt: Some(20.0),
                    ..CellStyle::default()
                },
            ),
            (
                (1, 0),
                CellStyle {
                    size_pt: Some(16.0),
                    ..CellStyle::default()
                },
            ),
            (
                (1, 1),
                CellStyle {
                    size_pt: Some(10.0),
                    ..CellStyle::default()
                },
            ),
        ],
    );
    styling.columns = BTreeMap::from([(0, 100), (1, 64)]);
    let mut shown = SheetShown::new();
    shown.insert(
        (1, 0),
        CellShown {
            numeric: true,
            ..CellShown::default()
        },
    );
    shown.insert(
        (1, 1),
        CellShown {
            numeric: true,
            ..CellShown::default()
        },
    );
    let bytes = encode_sheet_pdf("sizes", &sheets, &[styling.clone()], &[shown]).unwrap();
    let pdf = String::from_utf8_lossy(&bytes);

    assert!(pdf.contains("/F1 16.00 Tf"), "{pdf}");
    assert!(pdf.contains("/F1 20.00 Tf"), "{pdf}");
    // Bold outline and underline stroke widths scale with 16pt (16*0.03 = 0.48, 16*0.06 = 0.96).
    assert!(pdf.contains("2 Tr 0.48 w"), "{pdf}");
    assert!(pdf.contains("0.96 w"), "{pdf}");

    // At 20pt in a 48pt (64px) column (room = 40pt), each 'W' is 18.88pt so at most 2 fit.
    assert!(
        pdf.contains("(WW) Tj") && !pdf.contains("(WWW) Tj"),
        "{pdf}"
    );

    // Right-aligned 16pt "9" is farther left than a 10pt "9" would be inside the 75pt column.
    let x_big = text_x_of(&pdf, "16.00 Tf");
    let big_line = line_with(&pdf, "16.00 Tf")
        .into_iter()
        .find(|l| l.contains("(9) Tj"))
        .unwrap();
    let big_words: Vec<&str> = big_line.split(' ').collect();
    let big_td = big_words.iter().position(|w| *w == "Td").unwrap();
    let big_x: f64 = big_words[big_td - 2].parse().unwrap();
    let small_line = line_with(&pdf, "10.00 Tf")
        .into_iter()
        .find(|l| l.contains("(9) Tj"))
        .unwrap();
    let small_words: Vec<&str> = small_line.split(' ').collect();
    let small_td = small_words.iter().position(|w| *w == "Td").unwrap();
    let small_x: f64 = small_words[small_td - 2].parse().unwrap();
    // Col 0 width = 75pt, pad = 4pt -> right edge at x0 + 71; col 1 width = 48pt -> right edge at x0 + 119.
    let col0_right = x_big - 4.0 + 75.0 - 4.0;
    let col0_expected = col0_right - (556.0 * 16.0 / 1000.0);
    let col1_expected = (x_big - 4.0 + 75.0 + 48.0 - 4.0) - (556.0 * 10.0 / 1000.0);
    assert!(
        (big_x - col0_expected).abs() < 0.02,
        "{big_x} vs {col0_expected}"
    );
    assert!(
        (small_x - col1_expected).abs() < 0.02,
        "{small_x} vs {col1_expected}"
    );

    // Percent(50) page scale halves the 16pt cell font to 8.00 Tf.
    styling.page.scale = PageScale::Percent(50);
    let scaled_bytes = encode_sheet_pdf("scaled", &sheets, &[styling], &[]).unwrap();
    let scaled_pdf = String::from_utf8_lossy(&scaled_bytes);
    assert!(scaled_pdf.contains("/F1 8.00 Tf"), "{scaled_pdf}");
    assert!(scaled_pdf.contains("/F1 10.00 Tf"), "{scaled_pdf}");
}

#[test]
fn fill_repeat_uses_cell_font_size_for_glyph_count() {
    let mut styling = SheetStyling {
        columns: BTreeMap::from([(0, 120), (1, 120)]),
        ..SheetStyling::default()
    };
    styling.cells.insert(
        (0, 0),
        CellStyle {
            size_pt: Some(20.0),
            ..CellStyle::default()
        },
    );
    styling.cells.insert(
        (0, 1),
        CellStyle {
            size_pt: Some(10.0),
            ..CellStyle::default()
        },
    );
    let sheets = vec![(
        "S".to_string(),
        vec![(0, 0, "A".to_string()), (0, 1, "A".to_string())],
    )];
    let mut shown = SheetShown::new();
    shown.insert(
        (0, 0),
        CellShown {
            fill: Some((1, '-')),
            ..CellShown::default()
        },
    );
    shown.insert(
        (0, 1),
        CellShown {
            fill: Some((1, '-')),
            ..CellShown::default()
        },
    );
    let bytes = encode_sheet_pdf("fill", &sheets, &[styling], &[shown]).unwrap();
    let pdf = String::from_utf8_lossy(&bytes);
    let dashes_for = |tf: &str| -> usize {
        let line = line_with(&pdf, tf)[0];
        let start = line.find("(A").unwrap() + 2;
        let end = line[start..].find(')').unwrap() + start;
        line[start..end].matches('-').count()
    };
    let big_dashes = dashes_for("20.00 Tf");
    let small_dashes = dashes_for("10.00 Tf");
    assert!(
        small_dashes > big_dashes * 2,
        "10pt fits over twice as many '-' as 20pt: {big_dashes} vs {small_dashes}"
    );
}

#[test]
fn oversized_cell_font_clips_inside_cell_and_hostile_sizes_fail_safe() {
    let cells = vec![
        (0, 0, "A".to_string()),
        (0, 1, "B".to_string()),
        (1, 0, "bad1".to_string()),
        (1, 1, "bad2".to_string()),
        (2, 0, "tiny".to_string()),
        (2, 1, "huge".to_string()),
        (3, 0, String::new()),
    ];
    let (sheets, mut styling) = sheet_with_cells(
        cells,
        vec![
            (
                (0, 0),
                CellStyle {
                    bold: true,
                    size_pt: Some(10.0),
                    ..CellStyle::default()
                },
            ),
            (
                (0, 1),
                CellStyle {
                    bold: true,
                    size_pt: Some(36.0),
                    ..CellStyle::default()
                },
            ),
            (
                (1, 0),
                CellStyle {
                    size_pt: Some(f64::NAN),
                    ..CellStyle::default()
                },
            ),
            (
                (1, 1),
                CellStyle {
                    size_pt: Some(-12.0),
                    ..CellStyle::default()
                },
            ),
            (
                (2, 0),
                CellStyle {
                    size_pt: Some(0.1),
                    ..CellStyle::default()
                },
            ),
            (
                (2, 1),
                CellStyle {
                    size_pt: Some(9_999.0),
                    ..CellStyle::default()
                },
            ),
            (
                (3, 0),
                CellStyle {
                    bold: true,
                    underline: true,
                    size_pt: Some(48.0),
                    ..CellStyle::default()
                },
            ),
        ],
    );
    styling.columns = BTreeMap::from([(0, 120), (1, 400)]);
    let bytes = encode_sheet_pdf("hostile", &sheets, &[styling], &[]).unwrap();
    let pdf = String::from_utf8_lossy(&bytes);

    assert!(!pdf.contains("NaN") && !pdf.contains("inf"));
    assert!(
        pdf.contains("re W n\n"),
        "oversized 36pt cell clips inside its row box: {pdf}"
    );
    assert!(
        pdf.contains(&format!("/F1 {MIN_SIZE_PT:.2} Tf")),
        "sub-minimum finite size clamps to {MIN_SIZE_PT}"
    );
    // Col 1 is 300pt wide, so 'h' (556 units) at 409pt is 227.4pt and fits 1 char.
    assert!(
        pdf.contains(&format!("/F1 {MAX_SIZE_PT:.2} Tf")),
        "oversized finite size clamps to {MAX_SIZE_PT}"
    );
    // Balanced graphics state save/restore on the content stream.
    let q_count = pdf.lines().filter(|l| l.starts_with("q ")).count();
    let cap_q_count = pdf
        .lines()
        .filter(|l| *l == "Q" || l.ends_with(" Q"))
        .count();
    assert_eq!(q_count, cap_q_count, "balanced q/Q in PDF:\n{pdf}");
}

#[test]
fn korean_and_mixed_script_cells_render_own_font_size_and_bits() {
    let cells = vec![(0, 0, "가·나 A".to_string())];
    let (sheets, mut styling) = sheet_with_cells(
        cells,
        vec![(
            (0, 0),
            CellStyle {
                bold: true,
                italic: true,
                underline: true,
                strike: true,
                size_pt: Some(18.0),
                font_color: Some(0x334455),
                ..CellStyle::default()
            },
        )],
    );
    styling.columns = BTreeMap::from([(0, 160)]);
    let bytes = encode_sheet_pdf("ko-font", &sheets, &[styling], &[]).unwrap();
    let pdf = String::from_utf8_lossy(&bytes);
    assert!(pdf.contains("/F2 18.00 Tf"), "{pdf}");
    assert!(pdf.contains("/F1 18.00 Tf"), "{pdf}");
    assert!(pdf.contains(" 2 Tr 0.54 w "), "{pdf}");
    assert!(pdf.contains(" 0.21 1 "), "{pdf}");
    assert!(pdf.contains("re W n\n"), "18pt > 16.5pt clips cleanly");
    assert_eq!(line_with(&pdf, &format!("{} RG", rgb(0x334455))).len(), 3);
}

#[test]
fn underline_and_descender_near_row_height_clip_inside_cell() {
    let cells = vec![(0, 0, "Ag".to_string()), (1, 0, "next".to_string())];
    let pdf = render(
        cells,
        vec![(
            (0, 0),
            CellStyle {
                underline: true,
                size_pt: Some(16.0),
                ..CellStyle::default()
            },
        )],
    );
    assert!(
        pdf.contains("re W n\n"),
        "16pt underline below cell bottom (ty=bottom+1.25, ul=bottom-0.67) must clip inside cell_box:\n{pdf}"
    );
}

#[test]
fn small_page_scale_keeps_baseline_inside_cell_and_clips_italic_shear_overflow() {
    // 1) At 10% scale, row height = 1.65pt and default 10pt font = 1.0pt.
    // Baseline must stay inside cell_box without vertical overflow.
    let (sheets, mut styling) = sheet_with_cells(
        vec![(0, 0, "A".to_string()), (0, 1, "B".to_string())],
        vec![(
            (0, 1),
            CellStyle {
                size_pt: Some(16.0),
                ..CellStyle::default()
            },
        )],
    );
    styling.page.scale = PageScale::Percent(10);
    styling.page.grid_lines = false;
    styling.cells.insert(
        (0, 0),
        CellStyle {
            fill: Some(0xEEEEEE),
            ..CellStyle::default()
        },
    );
    let scaled_bytes = encode_sheet_pdf("tiny-scale", &sheets, &[styling], &[]).unwrap();
    let scaled_pdf = String::from_utf8_lossy(&scaled_bytes);
    let fill_line = line_with(&scaled_pdf, "re f Q")[0];
    let fill_nums: Vec<f64> = fill_line
        .split(' ')
        .filter_map(|w| w.parse().ok())
        .collect();
    let cell_bottom = fill_nums[fill_nums.len() - 3];
    let cell_h = fill_nums[fill_nums.len() - 1];
    let a_y = text_y_of(&scaled_pdf, "(A) Tj");
    assert!(
        a_y + 0.90 * 1.0 <= cell_bottom + cell_h + 0.01,
        "10% scale baseline ({a_y}) overflowed cell top ({} + {}):\n{scaled_pdf}",
        cell_bottom,
        cell_h
    );
    assert!(
        scaled_pdf.contains("re W n\n"),
        "16pt scaled to 1.6pt in 1.65pt row overflows descender/ascent and must clip:\n{scaled_pdf}"
    );

    // 2) Tall 60pt row with 36pt text: upright "MMa" (79.99pt in 88.50pt col) fits without clipping,
    // while 12-deg italic shear (0.21 * 0.9 * 36 = 6.80pt > 4pt pad) spills past x + w and must clip.
    let (tall_sheets, mut tall_styling) = sheet_with_cells(
        vec![(0, 0, "MMa".to_string()), (0, 1, "R".to_string())],
        vec![(
            (0, 0),
            CellStyle {
                italic: true,
                size_pt: Some(36.0),
                ..CellStyle::default()
            },
        )],
    );
    tall_styling.columns = BTreeMap::from([(0, 118)]);
    tall_styling.row_heights = [(0, RowHeight::parse("60", true).unwrap())].into();
    let tall_bytes = encode_sheet_pdf("tall-italic", &tall_sheets, &[tall_styling], &[]).unwrap();
    let tall_pdf = String::from_utf8_lossy(&tall_bytes);
    assert!(
        tall_pdf.contains("re W n\n"),
        "36pt italic in 60pt row shears past right padding and must clip:\n{tall_pdf}"
    );
    let (upright_sheets, mut upright_styling) = sheet_with_cells(
        vec![(0, 0, "MMa".to_string()), (0, 1, "R".to_string())],
        vec![(
            (0, 0),
            CellStyle {
                size_pt: Some(36.0),
                ..CellStyle::default()
            },
        )],
    );
    upright_styling.columns = BTreeMap::from([(0, 118)]);
    upright_styling.row_heights = [(0, RowHeight::parse("60", true).unwrap())].into();
    let upright_bytes =
        encode_sheet_pdf("tall-upright", &upright_sheets, &[upright_styling], &[]).unwrap();
    let upright_pdf = String::from_utf8_lossy(&upright_bytes);
    assert!(
        !upright_pdf.contains("re W n"),
        "upright 36pt in 60pt row fits inside cell without clipping:\n{upright_pdf}"
    );
}

#[test]
fn sized_cells_within_row_coalesce_without_spurious_clipping() {
    let cells = vec![
        (0, 0, "A".to_string()),
        (0, 1, "B".to_string()),
        (0, 2, "C".to_string()),
    ];
    let style = CellStyle {
        bold: true,
        size_pt: Some(12.0),
        font_color: Some(0x112233),
        ..CellStyle::default()
    };
    let (sheets, mut styling) = sheet_with_cells(
        cells,
        vec![((0, 0), style), ((0, 1), style), ((0, 2), style)],
    );
    styling.page.grid_lines = false;
    let bytes = encode_sheet_pdf("coalesce-12pt", &sheets, &[styling], &[]).unwrap();
    let pdf = String::from_utf8_lossy(&bytes);
    assert!(
        !pdf.contains("re W n"),
        "12pt inside 16.5pt row fits without clipping:\n{pdf}"
    );
    let shared = format!("q {} rg {} RG 2 Tr 0.36 w BT", rgb(0x112233), rgb(0x112233));
    assert_eq!(pdf.matches(&shared).count(), 1, "{pdf}");
    let start = pdf.find(&shared).unwrap();
    let run = &pdf[start..start + pdf[start..].find(" Q").unwrap()];
    assert_eq!(run.matches(" Tj").count(), 3, "{run}");
}

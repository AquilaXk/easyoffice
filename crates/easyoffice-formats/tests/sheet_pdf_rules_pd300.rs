//! The PDF of a sheet draws what the conditional formats give its cells (PD-300): the fill, the text color, the font bits,
//! the edges and the data bar. The rules are answered by the engine and arrive with the displayed cells, as `shown` does.

use easyoffice_formats::{
    encode_sheet_pdf, plain_cell_numeric, BorderEdge, BorderLine, CellBorders, CellRule, CellShown,
    CellStyle, CfBar, DefaultFont, OdsUnpacked, SheetShown, SheetStyling, FLAG_BOLD, FLAG_ITALIC,
    FLAG_STRIKE, FLAG_UNDERLINE,
};

/// A default column of a Calibri 11 pt workbook (a 7 px digit), 64 px (PD-305), in points.
const CELL_W: f64 = 48.0;
const CELL_H: f64 = 16.5;

/// Three rows in column A (`a`, `b`, `c`) and a number at B2: a used range of `A1:B3`.
fn sheet() -> (Vec<OdsUnpacked>, SheetStyling) {
    let cells = vec![
        (0, 0, "a".to_string()),
        (1, 0, "b".to_string()),
        (2, 0, "c".to_string()),
        (1, 1, "5".to_string()),
    ];
    let styling = SheetStyling {
        default_font: DefaultFont::kept(Some("Calibri"), Some(11.0)),
        ..SheetStyling::default()
    };
    (vec![("S".to_string(), cells)], styling)
}

fn rule_of(at: (u32, u32), rule: CellRule) -> SheetShown {
    let mut shown = SheetShown::new();
    shown.insert(
        at,
        CellShown {
            rule: Some(rule),
            ..CellShown::default()
        },
    );
    shown
}

fn pdf_with(styling: SheetStyling, shown: SheetShown) -> String {
    let (sheets, _) = sheet();
    let bytes = encode_sheet_pdf("규칙", &sheets, &[styling], &[shown]).unwrap();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn rgb(value: u32) -> String {
    let c = |shift: u32| f64::from((value >> shift) & 0xFF) / 255.0;
    format!("{:.3} {:.3} {:.3}", c(16), c(8), c(0))
}

type Boxed = (f64, f64, f64, f64);

/// Every box of every fill path (`q <color> rg x y w h re ... f Q`) with the operands of its color.
fn boxes(pdf: &str) -> Vec<(String, Boxed)> {
    pdf.lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("q ")?.strip_suffix(" f Q")?;
            let (color, path) = rest.split_once(" rg ")?;
            let words: Vec<&str> = path.split(' ').collect();
            if !words.len().is_multiple_of(5) || words.chunks(5).any(|c| c[4] != "re") {
                return None;
            }
            Some(
                words
                    .chunks(5)
                    .map(|c| {
                        let n = |i: usize| c[i].parse::<f64>().unwrap();
                        (color.to_string(), (n(0), n(1), n(2), n(3)))
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .flatten()
        .collect()
}

/// `(x, y, w, h)` of every box filled in `color`.
fn fills(pdf: &str, color: u32) -> Vec<Boxed> {
    let color = rgb(color);
    boxes(pdf)
        .into_iter()
        .filter(|(c, _)| *c == color)
        .map(|(_, b)| b)
        .collect()
}

fn line_with<'a>(pdf: &'a str, needle: &str) -> Vec<&'a str> {
    pdf.lines().filter(|l| l.contains(needle)).collect()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.011
}

const RED: u32 = 0xFF0000;

#[test]
fn a_rule_fill_replaces_the_fill_of_the_cell_itself() {
    let (_, mut styling) = sheet();
    styling.cells.insert(
        (1, 0),
        CellStyle {
            fill: Some(0xAA0000),
            ..CellStyle::default()
        },
    );
    let shown = rule_of(
        (1, 0),
        CellRule {
            bg: Some(0x00B050),
            ..CellRule::default()
        },
    );
    let pdf = pdf_with(styling, shown);
    assert!(
        fills(&pdf, 0xAA0000).is_empty(),
        "the cell's own fill is under the rule's"
    );
    let filled = fills(&pdf, 0x00B050);
    assert_eq!(filled.len(), 1);
    let (_, _, w, h) = filled[0];
    assert!(close(w, CELL_W) && close(h, CELL_H), "{w} x {h}");
}

#[test]
fn a_rule_fill_sits_where_the_cell_is_and_a_cell_with_nothing_but_the_rule_is_filled() {
    let shown: SheetShown = [(0, 0), (1, 0), (1, 1)]
        .into_iter()
        .zip([0x112233, 0x445566, 0x778899])
        .map(|(at, bg)| {
            (
                at,
                CellShown {
                    rule: Some(CellRule {
                        bg: Some(bg),
                        ..CellRule::default()
                    }),
                    ..CellShown::default()
                },
            )
        })
        .collect();
    let pdf = pdf_with(sheet().1, shown);
    let a1 = fills(&pdf, 0x112233)[0];
    let a2 = fills(&pdf, 0x445566)[0];
    let b2 = fills(&pdf, 0x778899)[0];
    assert!(
        close(a1.0, a2.0) && close(a1.1 - CELL_H, a2.1),
        "A2 is the row below A1"
    );
    assert!(
        close(b2.0, a1.0 + CELL_W) && close(b2.1, a2.1),
        "B2 is right of A2"
    );
}

#[test]
fn a_rule_on_a_cell_outside_the_printed_range_draws_nothing() {
    let mut shown = rule_of(
        (500, 500),
        CellRule {
            bg: Some(0x123456),
            ..CellRule::default()
        },
    );
    shown.insert(
        (u32::MAX, u32::MAX),
        CellShown {
            rule: Some(CellRule {
                bg: Some(0x123456),
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    let pdf = pdf_with(sheet().1, shown);
    assert!(fills(&pdf, 0x123456).is_empty());
}

#[test]
fn a_rule_text_color_replaces_the_cells_and_the_number_format_color_goes_over_it() {
    let (_, mut styling) = sheet();
    styling.cells.insert(
        (0, 0),
        CellStyle {
            font_color: Some(0x0000FF),
            ..CellStyle::default()
        },
    );
    let mut shown = rule_of(
        (0, 0),
        CellRule {
            fg: Some(0x9C0006),
            ..CellRule::default()
        },
    );
    shown.insert(
        (1, 0),
        CellShown {
            color: Some(0x00FF00),
            rule: Some(CellRule {
                fg: Some(0x9C0006),
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    let pdf = pdf_with(styling, shown);
    let red = format!("q {} rg BT", rgb(0x9C0006));
    assert_eq!(
        line_with(&pdf, &red).len(),
        1,
        "A1 is drawn in the rule's color"
    );
    assert!(line_with(&pdf, &format!("q {} rg BT", rgb(0x0000FF))).is_empty());
    assert_eq!(
        line_with(&pdf, &format!("q {} rg BT", rgb(0x00FF00))).len(),
        1,
        "the color of a number format stays on top"
    );
}

#[test]
fn text_with_no_color_turns_white_on_a_dark_rule_fill() {
    let shown = rule_of(
        (0, 0),
        CellRule {
            bg: Some(0x000040),
            ..CellRule::default()
        },
    );
    let pdf = pdf_with(sheet().1, shown);
    assert_eq!(
        line_with(&pdf, &format!("q {} rg BT", rgb(0xFFFFFF))).len(),
        1
    );
}

#[test]
fn the_font_bits_of_a_rule_make_the_text_bold_slanted_underlined_and_struck() {
    let plain = pdf_with(sheet().1, SheetShown::new());
    let draw = |flags: u8| {
        pdf_with(
            sheet().1,
            rule_of(
                (0, 0),
                CellRule {
                    flags,
                    ..CellRule::default()
                },
            ),
        )
    };
    assert!(!plain.contains(" Tr"));
    let bold = draw(FLAG_BOLD);
    assert_eq!(
        line_with(&bold, " 2 Tr ").len(),
        1,
        "bold is filled and stroked text"
    );
    assert!(!draw(FLAG_ITALIC).contains(" Tr"));
    assert_eq!(line_with(&draw(FLAG_ITALIC), " 0.21 1 ").len(), 1);
    let underlined = draw(FLAG_UNDERLINE);
    let struck = draw(FLAG_STRIKE);
    let strokes = |pdf: &str| line_with(pdf, " m ").len() - line_with(&plain, " m ").len();
    assert_eq!(strokes(&underlined), 1, "one line under the text");
    assert_eq!(strokes(&struck), 1, "one line through the text");
    let both = draw(FLAG_UNDERLINE | FLAG_STRIKE);
    assert_eq!(strokes(&both), 2);
    // The underline is under the line the text sits on and the strike line is above it.
    let y_of = |pdf: &str| -> f64 {
        let extra: Vec<&str> = pdf
            .lines()
            .filter(|l| l.contains(" m ") && !plain.contains(l))
            .collect();
        let words: Vec<&str> = extra[0].split(' ').collect();
        let at = words.iter().position(|w| *w == "m").unwrap();
        words[at - 1].parse().unwrap()
    };
    let text_y: f64 = {
        let line = line_with(&plain, "(a) Tj")[0];
        let words: Vec<&str> = line.split(' ').collect();
        let at = words.iter().position(|w| *w == "Td").unwrap();
        words[at - 1].parse().unwrap()
    };
    assert!(y_of(&underlined) < text_y);
    assert!(y_of(&struck) > text_y);
}

#[test]
fn the_edges_of_a_rule_are_drawn_on_the_sides_of_the_cell_over_the_grid() {
    let edge = |line| BorderEdge::new(line, RED);
    let border = CellBorders {
        right: edge(BorderLine::Thin),
        bottom: edge(BorderLine::Dashed),
        left: edge(BorderLine::Thick),
        top: edge(BorderLine::Double),
    };
    let fill = (
        (1, 0),
        CellShown {
            rule: Some(CellRule {
                bg: Some(0x445566),
                border,
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    let mut styling = sheet().1;
    styling.page.grid_lines = true;
    let pdf = pdf_with(styling, [fill].into_iter().collect());
    let (x, y, w, h) = fills(&pdf, 0x445566)[0];
    let color = format!("{} RG", rgb(RED));
    let right = format!(
        "q 0.75 w {color} {:.2} {y:.2} m {:.2} {:.2} l S Q",
        x + w,
        x + w,
        y + h
    );
    assert!(pdf.contains(&right), "thin right edge:\n{pdf}");
    let left = format!("q 1.50 w {color} {x:.2} {y:.2} m {x:.2} {:.2} l S Q", y + h);
    assert!(pdf.contains(&left), "thick left edge");
    assert!(
        pdf.contains(&format!(
            "q 0.75 w {color} [3.00 2.00] 0 d {x:.2} {y:.2} m {:.2} {y:.2} l S Q",
            x + w
        )),
        "dashed bottom edge"
    );
    // The thin lines along a row edge are one path: the two lines of the double top edge, either side of the cell's edge.
    let row_lines = line_with(&pdf, &format!("q 0.75 w {color} {x:.0}"));
    assert_eq!(row_lines.len(), 1, "{row_lines:?}");
    let words: Vec<&str> = row_lines[0].split(' ').collect();
    let ys: Vec<f64> = (1..words.len())
        .filter(|&i| words[i] == "m")
        .map(|i| words[i - 1].parse().unwrap())
        .collect();
    assert_eq!(ys.len(), 2, "{row_lines:?}");
    assert!(
        close(ys[0], y + h - 0.75) && close(ys[1], y + h + 0.75),
        "{ys:?}"
    );
    // The edges come after the grid lines, so the grid does not paint over them.
    assert!(pdf.find(&right).unwrap() > pdf.rfind("0.5 w").unwrap());
}

#[test]
fn a_cell_with_no_edge_in_its_rule_draws_none() {
    let shown = rule_of(
        (1, 0),
        CellRule {
            bg: Some(0x445566),
            ..CellRule::default()
        },
    );
    let with = pdf_with(sheet().1, shown);
    let without = pdf_with(sheet().1, SheetShown::new());
    assert_eq!(
        line_with(&with, " m ").len(),
        line_with(&without, " m ").len()
    );
}

fn bar(pct: i8, start: u8, gradient: bool) -> SheetShown {
    rule_of(
        (1, 0),
        CellRule {
            bar: Some(CfBar {
                pct,
                start,
                color: 0x638EC6,
                gradient,
            }),
            ..CellRule::default()
        },
    )
}

#[test]
fn a_solid_data_bar_is_a_box_from_the_axis_over_the_height_of_the_cell() {
    let mut shown = bar(50, 0, false);
    shown.insert(
        (0, 0),
        CellShown {
            rule: Some(CellRule {
                bg: Some(0x112233),
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    let pdf = pdf_with(sheet().1, shown);
    let (cx, cy, _, _) = fills(&pdf, 0x112233)[0];
    let bars = fills(&pdf, 0x638EC6);
    assert_eq!(bars.len(), 1);
    let (x, y, w, h) = bars[0];
    assert!(close(x, cx) && close(y, cy - CELL_H) && close(w, CELL_W / 2.0) && close(h, CELL_H));
    // The bar is under the text of its cell.
    assert!(pdf.find(&rgb(0x638EC6)).unwrap() < pdf.find("(b) Tj").unwrap());
}

#[test]
fn a_negative_bar_grows_left_of_the_axis() {
    let mut shown = bar(-50, 50, false);
    shown.insert(
        (0, 0),
        CellShown {
            rule: Some(CellRule {
                bg: Some(0x112233),
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    let pdf = pdf_with(sheet().1, shown);
    let (cx, _, _, _) = fills(&pdf, 0x112233)[0];
    let (x, _, w, _) = fills(&pdf, 0x638EC6)[0];
    // The axis is at half the width; half of the room left of it is the bar: 25 % to 50 %.
    assert!(
        close(x, cx + CELL_W * 0.25) && close(w, CELL_W * 0.25),
        "{x} {w}"
    );
}

#[test]
fn a_gradient_bar_is_one_form_stretched_over_the_box_of_the_bar() {
    let mut shown = bar(100, 0, true);
    shown.insert(
        (0, 0),
        CellShown {
            rule: Some(CellRule {
                bg: Some(0x112233),
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    let pdf = pdf_with(sheet().1, shown);
    let (cx, _, _, _) = fills(&pdf, 0x112233)[0];
    // One draw of the form over the bar's box: from the axis over the whole cell here.
    let draws = line_with(&pdf, "/Gr1 Do");
    assert_eq!(draws.len(), 1, "{draws:?}");
    let n: Vec<f64> = draws[0]
        .split(' ')
        .filter_map(|word| word.parse().ok())
        .collect();
    assert!(
        close(n[0], CELL_W) && close(n[3], CELL_H) && close(n[4], cx),
        "{n:?}"
    );
    // The form shades from the bar's color (99, 142, 198) at the axis to a quarter of it over white at the end.
    assert!(pdf.contains("/ShadingType 2") && pdf.contains("/FunctionType 2"));
    let ends = |pdf: &str| -> (f64, f64) {
        let first = |key: &str| -> f64 {
            let at = pdf.find(key).unwrap() + key.len();
            pdf[at..].split(' ').next().unwrap().parse().unwrap()
        };
        (first("/C0 ["), first("/C1 ["))
    };
    let (from, to) = ends(&pdf);
    assert!(
        close_ratio(from, 99.0 / 255.0) && close_ratio(to, 216.0 / 255.0),
        "{from} {to}"
    );
    // A bar that grows left of the axis is faded at its left end.
    let reversed = pdf_with(sheet().1, bar(-100, 100, true));
    let (from, to) = ends(&reversed);
    assert!(
        close_ratio(from, 216.0 / 255.0) && close_ratio(to, 99.0 / 255.0),
        "{from} {to}"
    );
}

fn close_ratio(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.005
}

#[test]
fn bars_of_one_pair_of_colors_share_one_gradient_form() {
    let mut shown = bar(50, 0, true);
    shown.insert(
        (2, 0),
        CellShown {
            rule: Some(CellRule {
                bar: Some(CfBar {
                    pct: 90,
                    start: 0,
                    color: 0x638EC6,
                    gradient: true,
                }),
                ..CellRule::default()
            }),
            ..CellShown::default()
        },
    );
    let pdf = pdf_with(sheet().1, shown);
    assert_eq!(line_with(&pdf, "/Gr1 Do").len(), 2);
    assert_eq!(pdf.matches("/ShadingType 2").count(), 1);
}

#[test]
fn a_solid_bar_is_over_the_fill_of_its_cell_whatever_the_colors_are() {
    // The bar color sorts before the fill color; the bar is still drawn after the fill.
    let mut shown = bar(50, 0, false);
    shown.get_mut(&(1, 0)).unwrap().rule.as_mut().unwrap().bg = Some(0xFFFFFF);
    let pdf = pdf_with(sheet().1, shown);
    assert!(pdf.find(&rgb(0xFFFFFF)).unwrap() < pdf.find(&rgb(0x638EC6)).unwrap());
}

#[test]
fn a_bar_that_hides_the_value_hides_the_text_of_the_cell() {
    let shown = rule_of(
        (1, 0),
        CellRule {
            hide_value: true,
            bar: Some(CfBar {
                pct: 80,
                start: 0,
                color: 0x638EC6,
                gradient: false,
            }),
            ..CellRule::default()
        },
    );
    let pdf = pdf_with(sheet().1, shown);
    assert!(!pdf.contains("(b) Tj"));
    assert!(pdf.contains("(a) Tj") && pdf.contains("(c) Tj"));
}

#[test]
fn a_hostile_rule_stays_inside_its_cell_and_writes_numbers() {
    let hostile = CellRule {
        bg: Some(u32::MAX),
        fg: Some(u32::MAX),
        flags: u8::MAX,
        border: CellBorders {
            top: BorderEdge::from_bits(u32::MAX),
            right: BorderEdge::from_bits(0x0300_0000 | 0xFF_FFFF),
            bottom: BorderEdge::from_bits(0x0400_0000),
            left: BorderEdge::from_bits(0x0100_0000 | 0xAB_CDEF),
        },
        bar: Some(CfBar {
            pct: i8::MIN,
            start: u8::MAX,
            color: u32::MAX,
            gradient: true,
        }),
        hide_value: false,
    };
    let pdf = pdf_with(sheet().1, rule_of((1, 0), hostile));
    assert!(!pdf.contains("NaN") && !pdf.contains("inf"));
    for (_, (x, _, w, _)) in boxes(&pdf) {
        assert!(x >= 0.0 && w >= 0.0 && x + w < 600.0, "{x} {w}");
    }
    let positive = bar(i8::MAX, 100, true);
    let pdf = pdf_with(sheet().1, positive);
    assert!(!pdf.contains("NaN") && !pdf.contains("inf"));
}

#[test]
fn a_sheet_without_rules_is_the_same_page_as_before() {
    let formatted: SheetShown = [(
        (1, 1),
        CellShown {
            numeric: true,
            color: Some(0xFF0000),
            ..CellShown::default()
        },
    )]
    .into_iter()
    .collect();
    let with_none: SheetShown = [(
        (1, 1),
        CellShown {
            numeric: true,
            color: Some(0xFF0000),
            rule: None,
            ..CellShown::default()
        },
    )]
    .into_iter()
    .collect();
    assert_eq!(
        pdf_with(sheet().1, formatted),
        pdf_with(sheet().1, with_none)
    );
    let empty_rule: SheetShown = rule_of((1, 0), CellRule::default());
    assert_eq!(
        pdf_with(sheet().1, empty_rule),
        pdf_with(sheet().1, SheetShown::new())
    );
}

#[test]
fn a_number_with_nothing_but_a_rule_sits_at_the_right_like_one_without() {
    let (sheets, styling) = sheet();
    let cells = &sheets[0].1;
    let text = |at: (u32, u32)| cells.iter().find(|c| (c.0, c.1) == at).unwrap().2.as_str();
    assert!(plain_cell_numeric(Some(&styling), (1, 1), text((1, 1))));
    assert!(!plain_cell_numeric(Some(&styling), (1, 0), text((1, 0))));
    assert!(plain_cell_numeric(None, (1, 1), text((1, 1))));
}

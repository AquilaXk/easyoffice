use super::*;
use crate::recalc::{CellChange, RecalcSession};
use crate::{PackedCellCoordinate, WorkbookEngine};
use easyoffice_formats::{
    parse_sqref, CfStyle, CfValue, Cmp, DataValidation, DvAlert, DvKind, TextOp, TimePeriod,
    ValueKind, FLAG_BOLD,
};
use std::collections::HashMap;

fn coord(a1: &str) -> PackedCellCoordinate {
    PackedCellCoordinate::parse_a1(a1).unwrap()
}

fn workbook(cells: &[(&str, &str)]) -> WorkbookEngine {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");
    for (a1, raw) in cells {
        wb.set_cell("Sheet1", a1, *raw).unwrap();
    }
    wb
}

const MARK: CfStyle = CfStyle {
    bg: Some(0xFFC7CE),
    fg: Some(0x9C0006),
    flags: FLAG_BOLD,
};

fn rule(ranges: &str, priority: u32, kind: CfKind) -> CfRule {
    CfRule {
        ranges: parse_sqref(ranges),
        priority,
        stop_if_true: false,
        kind,
        style: MARK,
    }
}

fn rules(cf: Vec<CfRule>) -> SheetRules {
    SheetRules {
        cf,
        ..SheetRules::default()
    }
}

fn eval(wb: &WorkbookEngine, cf: Vec<CfRule>) -> HashMap<String, CellVisual> {
    wb.evaluate_rules("Sheet1", rules(cf))
        .into_iter()
        .map(|v| (v.coord.to_a1(), v))
        .collect()
}

fn marked(map: &HashMap<String, CellVisual>) -> Vec<String> {
    let mut keys: Vec<String> = map
        .iter()
        .filter(|(_, v)| v.bg == MARK.bg)
        .map(|(k, _)| k.clone())
        .collect();
    keys.sort_by_key(|k| (k.len(), k.clone()));
    keys
}

fn column(values: &[&str]) -> Vec<(String, String)> {
    values
        .iter()
        .enumerate()
        .map(|(i, v)| (format!("A{}", i + 1), (*v).to_string()))
        .collect()
}

fn book(values: &[&str]) -> WorkbookEngine {
    let cells = column(values);
    let refs: Vec<(&str, &str)> = cells
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    workbook(&refs)
}

fn numbers_1_to_10() -> WorkbookEngine {
    book(&["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"])
}

fn cell_is(cmp: Cmp, a: &str, b: &str) -> CfKind {
    CfKind::CellIs {
        cmp,
        a: a.into(),
        b: b.into(),
    }
}

#[test]
fn cell_value_comparisons() {
    let wb = book(&["5", "50", "500", "text", "TRUE", "=1/0"]);
    let hit =
        |cmp, a: &str, b: &str| marked(&eval(&wb, vec![rule("A1:A6", 1, cell_is(cmp, a, b))]));
    assert_eq!(hit(Cmp::Greater, "49", ""), ["A2", "A3", "A4", "A5"]);
    assert_eq!(hit(Cmp::Less, "50", ""), ["A1"]);
    assert_eq!(hit(Cmp::Between, "50", "5"), ["A1", "A2"]);
    assert_eq!(hit(Cmp::NotBetween, "6", "100"), ["A1", "A3", "A4", "A5"]);
    assert_eq!(hit(Cmp::Equal, "50", ""), ["A2"]);
    assert_eq!(hit(Cmp::NotEqual, "50", ""), ["A1", "A3", "A4", "A5"]);
    assert_eq!(hit(Cmp::Equal, "\"TEXT\"", ""), ["A4"]);
    assert_eq!(hit(Cmp::GreaterEqual, "=A2*10", ""), ["A3", "A4", "A5"]);
}

#[test]
fn empty_cells_count_as_zero_inside_small_ranges() {
    let wb = workbook(&[("A1", "3"), ("A3", "9")]);
    let map = eval(&wb, vec![rule("A1:A3", 1, cell_is(Cmp::Less, "5", ""))]);
    assert_eq!(marked(&map), ["A1", "A2"]);
}

#[test]
fn text_rules_ignore_case() {
    let wb = book(&["Apple pie", "banana", "APPLE", "grape apple", ""]);
    let hit = |op, t: &str| {
        marked(&eval(
            &wb,
            vec![rule("A1:A5", 1, CfKind::Text { op, text: t.into() })],
        ))
    };
    assert_eq!(hit(TextOp::Contains, "apple"), ["A1", "A3", "A4"]);
    assert_eq!(hit(TextOp::Begins, "apple"), ["A1", "A3"]);
    assert_eq!(hit(TextOp::Ends, "APPLE"), ["A3", "A4"]);
    assert_eq!(hit(TextOp::NotContains, "apple").len(), 2);
}

#[test]
fn top_and_bottom_ranks() {
    let wb = numbers_1_to_10();
    let top = |n, percent, bottom| {
        marked(&eval(
            &wb,
            vec![rule("A1:A10", 1, CfKind::Top { n, percent, bottom })],
        ))
    };
    assert_eq!(top(3, false, false), ["A8", "A9", "A10"]);
    assert_eq!(top(2, false, true), ["A1", "A2"]);
    assert_eq!(top(20, true, false), ["A9", "A10"]);
    assert_eq!(top(30, true, true), ["A1", "A2", "A3"]);
    assert_eq!(top(100, false, false).len(), 10);
}

#[test]
fn average_rules() {
    let wb = book(&["1", "2", "3", "4", "100", "text"]);
    let hit = |above, equal, std_dev| {
        marked(&eval(
            &wb,
            vec![rule(
                "A1:A6",
                1,
                CfKind::Average {
                    above,
                    equal,
                    std_dev,
                },
            )],
        ))
    };
    assert_eq!(hit(true, false, 0), ["A5"]);
    assert_eq!(hit(false, false, 0), ["A1", "A2", "A3", "A4"]);
    assert_eq!(hit(true, false, 1), ["A5"]);
    assert!(hit(true, false, 2).is_empty());
}

#[test]
fn duplicates_and_unique_values() {
    let wb = book(&["a", "A", "b", "1", "1", "", "  "]);
    let dup = marked(&eval(
        &wb,
        vec![rule("A1:A7", 1, CfKind::Duplicates { unique: false })],
    ));
    assert_eq!(dup, ["A1", "A2", "A4", "A5"]);
    let uniq = marked(&eval(
        &wb,
        vec![rule("A1:A7", 1, CfKind::Duplicates { unique: true })],
    ));
    assert_eq!(uniq, ["A3"]);
}

#[test]
fn blanks_and_errors() {
    let wb = workbook(&[("A1", "x"), ("A2", " "), ("A4", "=1/0")]);
    let blanks = marked(&eval(
        &wb,
        vec![rule("A1:A4", 1, CfKind::Blanks { not: false })],
    ));
    assert_eq!(blanks, ["A2", "A3"]);
    let filled = marked(&eval(
        &wb,
        vec![rule("A1:A4", 1, CfKind::Blanks { not: true })],
    ));
    assert_eq!(filled, ["A1", "A4"]);
    let errors = marked(&eval(
        &wb,
        vec![rule("A1:A4", 1, CfKind::Errors { not: false })],
    ));
    assert_eq!(errors, ["A4"]);
}

#[test]
fn expression_uses_relative_references_from_the_first_range() {
    let wb = workbook(&[
        ("A1", "1"),
        ("B1", "2"),
        ("A2", "5"),
        ("B2", "3"),
        ("A3", "7"),
        ("B3", "9"),
        ("C1", "x"),
        ("C2", "x"),
        ("C3", "x"),
    ]);
    let map = eval(
        &wb,
        vec![rule("C1:C3", 1, CfKind::Expression("A1>B1".into()))],
    );
    assert_eq!(marked(&map), ["C2"]);
    let abs = eval(
        &wb,
        vec![rule("C1:C3", 1, CfKind::Expression("A1>$B$1".into()))],
    );
    assert_eq!(marked(&abs), ["C2", "C3"]);
}

fn stops(list: &[(ValueKind, &str, u32)]) -> CfKind {
    CfKind::ColorScale(
        list.iter()
            .map(|&(k, t, c)| (CfValue::new(k, t), c))
            .collect(),
    )
}

#[test]
fn color_scale_interpolates_between_stops() {
    let wb = book(&["0", "5", "10", "20", "text"]);
    let kind = stops(&[
        (ValueKind::Min, "", 0x000000),
        (ValueKind::Num, "10", 0xFF0000),
        (ValueKind::Max, "", 0x00FF00),
    ]);
    let map = eval(&wb, vec![rule("A1:A5", 1, kind)]);
    assert_eq!(map["A1"].bg, Some(0x000000));
    assert_eq!(map["A2"].bg, Some(0x800000));
    assert_eq!(map["A3"].bg, Some(0xFF0000));
    assert_eq!(map["A4"].bg, Some(0x00FF00));
    assert!(!map.contains_key("A5"));
    assert_eq!(map["A2"].fg, None);
}

#[test]
fn color_scale_percentile_and_percent_stops() {
    let wb = numbers_1_to_10();
    let kind = stops(&[
        (ValueKind::Min, "", 0x000000),
        (ValueKind::Percentile, "50", 0x808080),
        (ValueKind::Percent, "100", 0xFFFFFF),
    ]);
    let map = eval(&wb, vec![rule("A1:A10", 1, kind)]);
    // The median of 1..10 is 5.5.
    assert_eq!(map["A1"].bg, Some(0x000000));
    assert_eq!(map["A10"].bg, Some(0xFFFFFF));
    assert!(map["A5"].bg.unwrap() < 0x808080 && map["A6"].bg.unwrap() > 0x808080);
}

fn bar(min: ValueKind, min_t: &str, max: ValueKind) -> CfKind {
    CfKind::DataBar {
        min: CfValue::new(min, min_t),
        max: CfValue::new(max, ""),
        positive: 0x638EC6,
        negative: 0xFF0000,
        gradient: true,
        show_value: false,
    }
}

#[test]
fn data_bars_follow_the_value_range() {
    let wb = book(&["0", "5", "10", "-10"]);
    let map = eval(
        &wb,
        vec![rule("A1:A3", 1, bar(ValueKind::Min, "", ValueKind::Max))],
    );
    assert!(!map.contains_key("A1"));
    assert_eq!(map["A2"].bar.unwrap().pct, 50);
    assert_eq!(map["A3"].bar.unwrap().pct, 100);
    assert_eq!(map["A3"].bar.unwrap().start, 0);
    assert!(map["A2"].hide_value);
    let mixed = eval(
        &wb,
        vec![rule("A1:A4", 1, bar(ValueKind::Min, "", ValueKind::Max))],
    );
    let neg = mixed["A4"].bar.unwrap();
    assert_eq!((neg.pct, neg.start, neg.color), (-100, 50, 0xFF0000));
    assert_eq!(mixed["A3"].bar.unwrap().pct, 100);
}

#[test]
fn icon_sets_pick_the_highest_threshold_met() {
    let wb = numbers_1_to_10();
    let icons = |reverse| CfKind::IconSet {
        set: 3,
        thresholds: vec![
            CfValue::new(ValueKind::Percent, "0"),
            CfValue::new(ValueKind::Percent, "33"),
            CfValue::new(ValueKind::Percent, "67"),
        ],
        reverse,
        show_value: true,
    };
    let map = eval(&wb, vec![rule("A1:A10", 1, icons(false))]);
    let idx = |a1: &str| map[a1].icon.unwrap().index;
    // The 33rd percent of 1..=10 is 3.97 and the 67th is 7.03.
    let picked = ["A1", "A3", "A4", "A7", "A8", "A10"].map(idx);
    assert_eq!(picked, [0, 0, 1, 1, 2, 2]);
    assert!(!map["A1"].hide_value);
    let rev = eval(&wb, vec![rule("A1:A10", 1, icons(true))]);
    assert_eq!(rev["A1"].icon.unwrap().index, 2);
    assert_eq!(rev["A10"].icon.unwrap().index, 0);
}

#[test]
fn priority_decides_conflicts_and_stop_if_true_ends_the_list() {
    let wb = book(&["10"]);
    let mut first = rule("A1", 1, cell_is(Cmp::Greater, "5", ""));
    first.style = CfStyle {
        bg: Some(0x111111),
        ..CfStyle::default()
    };
    let mut second = rule("A1", 2, cell_is(Cmp::Greater, "5", ""));
    second.style = CfStyle {
        bg: Some(0x222222),
        fg: Some(0x333333),
        flags: 0,
    };
    let merged = eval(&wb, vec![second.clone(), first.clone()]);
    assert_eq!(
        (merged["A1"].bg, merged["A1"].fg),
        (Some(0x111111), Some(0x333333))
    );
    first.stop_if_true = true;
    let stopped = eval(&wb, vec![second, first]);
    assert_eq!((stopped["A1"].bg, stopped["A1"].fg), (Some(0x111111), None));
}

#[test]
fn whole_column_ranges_only_visit_populated_cells() {
    let wb = workbook(&[("A5", "3"), ("A900000", "8")]);
    let map = eval(
        &wb,
        vec![rule(
            "A:A",
            1,
            CfKind::Top {
                n: 1,
                percent: false,
                bottom: false,
            },
        )],
    );
    assert_eq!(marked(&map), ["A900000"]);
    let blanks = eval(&wb, vec![rule("A:A", 1, CfKind::Blanks { not: false })]);
    assert!(blanks.is_empty());
}

#[test]
fn visuals_pack_and_unpack() {
    let visuals = vec![
        CellVisual {
            coord: coord("B2"),
            bg: Some(0xFFC7CE),
            fg: Some(0x9C0006),
            flags: 5,
            bar: None,
            icon: None,
            hide_value: false,
        },
        CellVisual {
            coord: coord("XFD1048576"),
            bg: None,
            fg: None,
            flags: 0,
            bar: Some(Bar {
                pct: -73,
                start: 41,
                color: 0x638EC6,
                gradient: true,
            }),
            icon: Some(Icon { set: 19, index: 4 }),
            hide_value: true,
        },
        CellVisual {
            coord: coord("C3"),
            bg: None,
            fg: None,
            flags: 0,
            bar: None,
            icon: None,
            hide_value: false,
        },
    ];
    let packed = encode_visuals(&visuals);
    assert_eq!(decode_visuals(&packed), visuals);
    assert_eq!(packed.palette.len(), 3);
    assert_eq!(packed.words.len(), 3 + 3 + 2);
}

// ---- memory held by the rules ------------------------------------------------------------------

#[test]
fn the_footprint_counts_what_the_rules_hold() {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");
    for row in 1..=2_000u32 {
        wb.set_cell("Sheet1", &format!("A{row}"), row.to_string())
            .unwrap();
    }
    let mut session = RecalcSession::new(wb).unwrap();
    session.recalculate();
    let bare = session.footprint();
    assert_eq!(bare.rule_bytes, 0);

    let visuals = session
        .set_rules(
            "Sheet1",
            rules(vec![rule("A1:A2000", 1, cell_is(Cmp::Greater, "0", ""))]),
        )
        .unwrap();
    assert_eq!(visuals.len(), 2_000);
    let ruled = session.footprint();
    // Every matched cell sits in the rule's hit map and in the merged appearance map.
    let per_cell = std::mem::size_of::<(PackedCellCoordinate, CellVisual)>();
    assert!(ruled.rule_bytes >= 2_000 * per_cell, "{}", ruled.rule_bytes);
    assert_eq!(ruled.total_bytes(), bare.total_bytes() + ruled.rule_bytes);
    assert_eq!((ruled.raw_bytes, ruled.cells), (bare.raw_bytes, bare.cells));

    // A rule that reads range statistics also pins the cached values of its range.
    session
        .set_rules(
            "Sheet1",
            rules(vec![rule(
                "A1:A2000",
                1,
                CfKind::Top {
                    n: 5,
                    percent: false,
                    bottom: false,
                },
            )]),
        )
        .unwrap();
    assert!(session.footprint().rule_bytes > 2_000 * 16);

    session.set_rules("Sheet1", SheetRules::default()).unwrap();
    assert_eq!(
        session.footprint().rule_bytes,
        0,
        "replaced rules free their memory"
    );
}

// ---- incremental update equals full evaluation -------------------------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn mixed_rules() -> SheetRules {
    rules(vec![
        rule("A1:A20", 1, cell_is(Cmp::Greater, "50", "")),
        rule(
            "A1:A20",
            2,
            CfKind::Top {
                n: 3,
                percent: false,
                bottom: false,
            },
        ),
        rule("A1:B20", 3, CfKind::Duplicates { unique: false }),
        rule(
            "B1:B20",
            4,
            CfKind::Average {
                above: true,
                equal: false,
                std_dev: 0,
            },
        ),
        rule(
            "A1:A20",
            5,
            stops(&[
                (ValueKind::Min, "", 0x0000FF),
                (ValueKind::Max, "", 0xFF0000),
            ]),
        ),
        rule("B1:B20", 6, bar(ValueKind::Num, "0", ValueKind::Max)),
        rule("C1:C20", 7, CfKind::Expression("A1>B1".into())),
        rule("C1:C20", 8, cell_is(Cmp::Greater, "=$D$1", "")),
        rule(
            "D2:D20",
            9,
            CfKind::Text {
                op: TextOp::Contains,
                text: "7".into(),
            },
        ),
        rule("A1:D20", 10, CfKind::Blanks { not: false }),
    ])
}

fn random_raw(rng: &mut Rng, row: u64) -> String {
    match rng.below(7) {
        0 => String::new(),
        1 => format!("=A{}+B{}", row, row),
        2 => "text 7".to_string(),
        3 => "=1/0".to_string(),
        _ => (rng.below(100) as i64 - 20).to_string(),
    }
}

#[test]
fn incremental_updates_equal_a_full_evaluation() {
    let mut formatted_cells = 0usize;
    for seed in 1..=40u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut wb = WorkbookEngine::new();
        wb.add_sheet("Sheet1");
        for row in 1..=20u64 {
            for col in ["A", "B", "C", "D"] {
                if rng.below(3) > 0 {
                    wb.set_cell("Sheet1", &format!("{col}{row}"), random_raw(&mut rng, row))
                        .unwrap();
                }
            }
        }
        let mut session = RecalcSession::new(wb).unwrap();
        session.recalculate();
        let mut shown: HashMap<PackedCellCoordinate, CellVisual> = session
            .set_rules("Sheet1", mixed_rules())
            .unwrap()
            .into_iter()
            .map(|v| (v.coord, v))
            .collect();
        for step in 0..30 {
            let n = 1 + rng.below(3);
            let edits: Vec<CellChange> = (0..n)
                .map(|_| {
                    let row = 1 + rng.below(20);
                    let col = ["A", "B", "C", "D"][rng.below(4) as usize];
                    CellChange {
                        sheet: "Sheet1".into(),
                        coord: coord(&format!("{col}{row}")),
                        raw: random_raw(&mut rng, row),
                    }
                })
                .collect();
            session.apply(&edits).unwrap();
            let outcome = session.recalculate();
            for (_, visuals) in session.rule_updates(&edits, &outcome).changed {
                for v in visuals {
                    if v.is_none() {
                        shown.remove(&v.coord);
                    } else {
                        shown.insert(v.coord, v);
                    }
                }
            }
            let fresh: HashMap<PackedCellCoordinate, CellVisual> = session
                .workbook()
                .evaluate_rules("Sheet1", mixed_rules())
                .into_iter()
                .map(|v| (v.coord, v))
                .collect();
            assert_eq!(shown, fresh, "seed {seed} step {step}");
            formatted_cells += fresh.len();
        }
    }
    assert!(
        formatted_cells > 5_000,
        "the rules must format cells: {formatted_cells}"
    );
}

// ---- data validation ---------------------------------------------------------------------------

fn dv(kind: DvKind, cmp: Cmp, a: &str, b: &str) -> DataValidation {
    let mut v = DataValidation::new(parse_sqref("B2:B9"), kind);
    v.cmp = cmp;
    v.a = a.into();
    v.b = b.into();
    v.error = "no".into();
    v
}

fn check(rules: &[DataValidation], cell: &str, input: &str) -> bool {
    let mut wb = workbook(&[("D1", "red"), ("D2", "green"), ("A1", "4")]);
    let memo = crate::MemoTable::new(wb.sheets()[0].resolved_sheet_id());
    matches!(
        validate_input(&mut wb, 0, rules, coord(cell), input, memo).0,
        Verdict::Valid
    )
}

#[test]
fn whole_and_decimal_numbers() {
    let whole = [dv(DvKind::Whole, Cmp::Between, "1", "10")];
    assert!(check(&whole, "B2", "5"));
    assert!(check(&whole, "B2", "10"));
    assert!(!check(&whole, "B2", "5.5"));
    assert!(!check(&whole, "B2", "11"));
    assert!(!check(&whole, "B2", "abc"));
    assert!(
        check(&whole, "C2", "abc"),
        "outside the range nothing applies"
    );
    let decimal = [dv(DvKind::Decimal, Cmp::Greater, "=A1", "")];
    assert!(check(&decimal, "B3", "4.01"));
    assert!(!check(&decimal, "B3", "4"));
}

#[test]
fn empty_input_and_formulas_are_not_judged() {
    let whole = [dv(DvKind::Whole, Cmp::Equal, "1", "")];
    assert!(check(&whole, "B2", ""));
    assert!(check(&whole, "B2", "=A1*100"));
}

#[test]
fn list_literal_and_range() {
    let literal = [dv(DvKind::List, Cmp::Between, "red, green ,blue", "")];
    assert!(check(&literal, "B2", "green"));
    assert!(!check(&literal, "B2", "GREEN"));
    assert!(!check(&literal, "B2", "pink"));
    let range = [dv(DvKind::List, Cmp::Between, "=Sheet1!$D$1:$D$2", "")];
    assert!(check(&range, "B2", "RED"));
    assert!(!check(&range, "B2", "blue"));
}

#[test]
fn text_length_date_time_and_custom() {
    let len = [dv(DvKind::TextLength, Cmp::LessEqual, "3", "")];
    assert!(check(&len, "B2", "한글이"));
    assert!(!check(&len, "B2", "한글이다"));
    let date = [dv(DvKind::Date, Cmp::GreaterEqual, "2026-01-01", "")];
    assert!(check(&date, "B2", "2026-03-04"));
    assert!(!check(&date, "B2", "2025-12-31"));
    assert!(!check(&date, "B2", "tomorrow"));
    let time = [dv(DvKind::Time, Cmp::Between, "09:00", "18:00")];
    assert!(check(&time, "B2", "12:30"));
    assert!(!check(&time, "B2", "21:00"));
    let custom = [dv(
        DvKind::Custom,
        Cmp::Between,
        "AND(ISNUMBER(B2),B2>A1)",
        "",
    )];
    assert!(check(&custom, "B2", "9"));
    assert!(!check(&custom, "B2", "3"));
    assert!(!check(&custom, "B2", "word"));
}

#[test]
fn a_check_box_takes_only_its_two_values() {
    let plain = [dv(DvKind::Checkbox, Cmp::Between, "", "")];
    assert!(check(&plain, "B2", "TRUE"));
    assert!(check(&plain, "B2", "false"));
    assert!(check(&plain, "B2", ""), "clearing the cell is allowed");
    assert!(!check(&plain, "B2", "1"));
    assert!(!check(&plain, "B2", "예"));
    let named = [dv(DvKind::Checkbox, Cmp::Between, "완료", "미완료")];
    assert!(check(&named, "B2", "완료"));
    assert!(check(&named, "B2", " 미완료 "));
    assert!(!check(&named, "B2", "TRUE"));
    let numbers = [dv(DvKind::Checkbox, Cmp::Between, "1", "0")];
    assert!(check(&numbers, "B2", "1") && check(&numbers, "B2", "0"));
    assert!(!check(&numbers, "B2", "2"));
}

#[test]
fn a_multiple_list_takes_any_chosen_items_of_the_list() {
    let literal = [dv(
        DvKind::ListMultiple,
        Cmp::Between,
        "red, green ,blue",
        "",
    )];
    assert!(check(&literal, "B2", "red"));
    assert!(check(&literal, "B2", "red,blue"));
    assert!(check(&literal, "B2", "blue, green ,red"));
    assert!(!check(&literal, "B2", "red,pink"));
    assert!(!check(&literal, "B2", "RED"));
    assert!(check(&literal, "B2", ",red,"), "empty parts are ignored");
    let range = [dv(
        DvKind::ListMultiple,
        Cmp::Between,
        "=Sheet1!$D$1:$D$2",
        "",
    )];
    assert!(check(&range, "B2", "red,green"));
    assert!(!check(&range, "B2", "red,blue"));
}

#[test]
fn custom_validation_restores_the_cell_it_tried() {
    let mut wb = workbook(&[("B2", "keep")]);
    let rule = [dv(DvKind::Custom, Cmp::Between, "B2>1", "")];
    let memo = crate::MemoTable::new(wb.sheets()[0].resolved_sheet_id());
    let _ = validate_input(&mut wb, 0, &rule, coord("B2"), "99", memo);
    assert_eq!(wb.sheet("Sheet1").unwrap().get_raw_cell("B2"), Some("keep"));
}

#[test]
fn the_rejection_carries_the_rule_alert() {
    let mut rule = dv(DvKind::Whole, Cmp::Equal, "1", "");
    rule.alert = DvAlert::Warning;
    rule.error_title = "제목".into();
    let mut wb = workbook(&[]);
    let memo = crate::MemoTable::new(wb.sheets()[0].resolved_sheet_id());
    let (verdict, _) = validate_input(&mut wb, 0, &[rule], coord("B2"), "2", memo);
    assert_eq!(
        verdict,
        Verdict::Invalid {
            alert: DvAlert::Warning,
            title: "제목".into(),
            message: "no".into(),
            show: true
        }
    );
}

#[test]
fn rules_that_match_too_many_cells_are_dropped() {
    let wb = numbers_1_to_10();
    let sheet = wb.sheet("Sheet1").unwrap();
    let mut ctx = crate::EvalContext::with_workbook(&wb, sheet.resolved_sheet_id());
    let mut cells = SheetCells {
        sheet,
        ctx: &mut ctx,
    };
    let all = rules(vec![rule("A1:A10", 1, cell_is(Cmp::Greater, "0", ""))]);
    let mut roomy = RuleRuntime::with_max_hits(all.clone(), 10);
    assert_eq!(roomy.evaluate(&mut cells).len(), 10);
    assert!(!roomy.is_over_budget());
    let mut tight = RuleRuntime::with_max_hits(all, 9);
    assert!(tight.evaluate(&mut cells).is_empty());
    assert!(tight.is_over_budget());
    assert!(tight.update(&mut cells, &[coord("A1")], true).is_empty());
}

// ---- period rules ------------------------------------------------------------------------------

fn serial(y: i64, m: i64, d: i64) -> f64 {
    crate::functions_date_fin::serial_from_civil(y, m, d) as f64
}

/// Rows 1..=n of column A hold the given serial numbers (a fraction is a time of day).
fn dated(values: &[f64]) -> WorkbookEngine {
    let cells: Vec<(String, String)> = values
        .iter()
        .enumerate()
        .map(|(i, v)| (format!("A{}", i + 1), v.to_string()))
        .collect();
    let refs: Vec<(&str, &str)> = cells
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    workbook(&refs)
}

fn period_hits(wb: &WorkbookEngine, period: TimePeriod, today: f64) -> Vec<String> {
    let mut r = rules(vec![rule("A1:A12", 1, CfKind::TimePeriod(period))]);
    r.today = Some(today as i64);
    let map: HashMap<String, CellVisual> = wb
        .evaluate_rules("Sheet1", r)
        .into_iter()
        .map(|v| (v.coord.to_a1(), v))
        .collect();
    marked(&map)
}

#[test]
fn period_rules_match_the_days_counted_from_the_given_today() {
    // Thursday 2026-11-12. Rows: 11-05, 11-06, 11-07, 11-08 (Sun), 11-11, 11-12 at noon, 11-13,
    // 11-14 (Sat), 11-15 (Sun), 10-31, 12-01, then text.
    let today = serial(2026, 11, 12);
    let days = [
        serial(2026, 11, 5),
        serial(2026, 11, 6),
        serial(2026, 11, 7),
        serial(2026, 11, 8),
        serial(2026, 11, 11),
        today + 0.5,
        serial(2026, 11, 13),
        serial(2026, 11, 14),
        serial(2026, 11, 15),
        serial(2026, 10, 31),
        serial(2026, 12, 1),
    ];
    let mut cells: Vec<(String, String)> = days
        .iter()
        .enumerate()
        .map(|(i, v)| (format!("A{}", i + 1), v.to_string()))
        .collect();
    cells.push(("A12".into(), "후일".into()));
    let refs: Vec<(&str, &str)> = cells
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let wb = workbook(&refs);
    let hit = |p| period_hits(&wb, p, today);
    assert_eq!(
        hit(TimePeriod::Today),
        ["A6"],
        "the time of day does not matter"
    );
    assert_eq!(hit(TimePeriod::Yesterday), ["A5"]);
    assert_eq!(hit(TimePeriod::Tomorrow), ["A7"]);
    assert_eq!(hit(TimePeriod::Last7Days), ["A2", "A3", "A4", "A5", "A6"]);
    assert_eq!(hit(TimePeriod::ThisWeek), ["A4", "A5", "A6", "A7", "A8"]);
    assert_eq!(hit(TimePeriod::LastWeek), ["A1", "A2", "A3"]);
    assert_eq!(hit(TimePeriod::NextWeek), ["A9"]);
    assert_eq!(
        hit(TimePeriod::ThisMonth),
        ["A1", "A2", "A3", "A4", "A5", "A6", "A7", "A8", "A9"]
    );
    assert_eq!(hit(TimePeriod::LastMonth), ["A10"]);
    assert_eq!(hit(TimePeriod::NextMonth), ["A11"]);
}

#[test]
fn period_rules_follow_a_new_today_and_ignore_blanks_and_text() {
    let wb = dated(&[serial(2026, 11, 12)]);
    assert_eq!(
        period_hits(&wb, TimePeriod::Today, serial(2026, 11, 12)),
        ["A1"]
    );
    assert!(period_hits(&wb, TimePeriod::Today, serial(2026, 11, 13)).is_empty());
    assert_eq!(
        period_hits(&wb, TimePeriod::Yesterday, serial(2026, 11, 13)),
        ["A1"]
    );
    // A session re-judges only the edited cell: the day stays the one the rules were set with.
    let mut session = RecalcSession::new(workbook(&[("A1", "1")])).unwrap();
    let mut r = rules(vec![rule(
        "A1:A3",
        1,
        CfKind::TimePeriod(TimePeriod::Today),
    )]);
    r.today = Some(serial(2026, 11, 12) as i64);
    assert!(session.set_rules("Sheet1", r).unwrap().is_empty());
    let edits = [CellChange {
        sheet: "Sheet1".into(),
        coord: coord("A2"),
        raw: serial(2026, 11, 12).to_string(),
    }];
    session.apply(&edits).unwrap();
    let outcome = session.recalculate();
    let changed = session.rule_updates(&edits, &outcome).changed;
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].1[0].coord, coord("A2"));
    assert!(!changed[0].1[0].is_none());
}

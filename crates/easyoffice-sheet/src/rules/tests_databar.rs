use super::tests::{eval, rule, workbook};
use super::*;
use easyoffice_formats::{CfBarAxis, CfBarDirection, CfKind, CfValue, ValueKind};

fn make_workbook(values: &[&str]) -> WorkbookEngine {
    let pairs: Vec<(String, &str)> = values
        .iter()
        .enumerate()
        .map(|(i, text)| (format!("A{}", i + 1), *text))
        .collect();
    let refs: Vec<(&str, &str)> = pairs.iter().map(|(a, b)| (a.as_str(), *b)).collect();
    workbook(&refs)
}

fn bar_kind(direction: CfBarDirection, border: bool, axis_position: CfBarAxis) -> CfKind {
    CfKind::DataBar {
        min: CfValue::new(ValueKind::Min, ""),
        max: CfValue::new(ValueKind::Max, ""),
        positive: 0x63_8E_C6,
        negative: 0xFF_00_00,
        gradient: true,
        show_value: true,
        direction,
        border,
        border_positive: 0x11_22_33,
        border_negative: 0x44_55_66,
        axis_position,
        axis_color: 0x77_88_99,
    }
}

#[test]
fn midpoint_axis_places_axis_at_half_even_for_all_positive_and_zero() {
    let wb = make_workbook(&["0", "5", "10"]);
    let map = eval(
        &wb,
        vec![rule(
            "A1:A3",
            1,
            bar_kind(CfBarDirection::RightToLeft, true, CfBarAxis::Midpoint),
        )],
    );
    let zero = map["A1"].bar.unwrap();
    assert_eq!(
        (zero.pct, zero.start, zero.axis_position),
        (0, 50, CfBarAxis::Midpoint)
    );
    let mid = map["A2"].bar.unwrap();
    assert_eq!(mid.pct, 50);
    assert_eq!(mid.start, 50);
    assert_eq!(mid.direction, CfBarDirection::RightToLeft);
    assert!(mid.border);
    assert_eq!(mid.border_color, 0x11_22_33);
    assert_eq!(mid.axis_color, 0x77_88_99);
    assert_eq!(map["A3"].bar.unwrap().pct, 100);
}

#[test]
fn none_axis_scales_mixed_signs_from_zero_edge_with_sign_colors() {
    let wb = make_workbook(&["-10", "0", "5", "10"]);
    let map = eval(
        &wb,
        vec![rule(
            "A1:A4",
            1,
            bar_kind(CfBarDirection::LeftToRight, true, CfBarAxis::None),
        )],
    );
    assert!(!map.contains_key("A2"), "zero with axis=none emits no bar");
    let neg = map["A1"].bar.unwrap();
    assert_eq!(
        (
            neg.pct,
            neg.start,
            neg.color,
            neg.border_color,
            neg.axis_position
        ),
        (-100, 0, 0xFF_00_00, 0x44_55_66, CfBarAxis::None)
    );
    let pos = map["A3"].bar.unwrap();
    assert_eq!(
        (
            pos.pct,
            pos.start,
            pos.color,
            pos.border_color,
            pos.direction
        ),
        (50, 0, 0x63_8E_C6, 0x11_22_33, CfBarDirection::LeftToRight)
    );
}

#[test]
fn automatic_mixed_signs_keeps_interior_axis_on_zero_value() {
    let wb = make_workbook(&["-10", "0", "10"]);
    let map = eval(
        &wb,
        vec![rule(
            "A1:A3",
            1,
            bar_kind(CfBarDirection::Context, false, CfBarAxis::Automatic),
        )],
    );
    let zero = map["A2"].bar.unwrap();
    assert_eq!(zero.pct, 0);
    assert_eq!(zero.start, 50);
    assert_eq!(zero.axis_position, CfBarAxis::Automatic);
}

#[test]
fn pack_round_trips_extended_bar_direction_border_and_axis() {
    let visuals = vec![
        CellVisual {
            coord: PackedCellCoordinate::new(3, 5).unwrap(),
            bg: Some(0xEE_EE_EE),
            fg: None,
            flags: 1,
            bar: Some(Bar {
                pct: -42,
                start: 50,
                color: 0xFF_00_00,
                gradient: false,
                direction: CfBarDirection::RightToLeft,
                border: true,
                border_color: 0x44_55_66,
                axis_position: CfBarAxis::Midpoint,
                axis_color: 0x77_88_99,
            }),
            icon: Some(Icon { set: 2, index: 1 }),
            hide_value: true,
            border: CellBorders::NONE,
        },
        CellVisual {
            coord: PackedCellCoordinate::new(4, 5).unwrap(),
            bg: None,
            fg: None,
            flags: 0,
            bar: Some(Bar {
                pct: 80,
                start: 0,
                color: 0x63_8E_C6,
                gradient: true,
                direction: CfBarDirection::LeftToRight,
                border: false,
                border_color: 0x63_8E_C6,
                axis_position: CfBarAxis::None,
                axis_color: 0x00_00_00,
            }),
            icon: None,
            hide_value: false,
            border: CellBorders::NONE,
        },
    ];
    let packed = encode_visuals(&visuals);
    let back = decode_visuals(&packed);
    assert_eq!(back, visuals);
}

//! The parts of a worksheet that describe how it is shown (`sheetPr`, `sheetViews`, `sheetFormatPr`)
//! and the group levels of its rows (`row@outlineLevel`, `row@collapsed`) (PD-208).
//!
//! 참조 구현: IronCalc `xlsx/src/import/worksheets.rs` lines 312-335 (`load_sheet_color`, `tabColor`
//! with `rgb` or `theme` and `tint`), 828-900 (`load_sheet_view`: the `pane` of a frozen sheet,
//! `xSplit` and `ySplit` read as numbers because some writers store `6.0`) and
//! `xlsx/src/export/worksheets.rs` lines 660-725 (`pane` with `topLeftCell`, `activePane` and one
//! `selection` per pane), Apache-2.0 or MIT, Copyright (c) IronCalc contributors, commit
//! 87b684b. The reference ignores `outlineLevel`, so the levels follow ECMA-376 18.3.1.73 and
//! 18.3.1.31, and the theme tint follows 18.3.1.15 (HLS luminance).
//!
//! Modifications: a frozen pane whose sheet view is scrolled (`sheetView@topLeftCell` away from A1)
//! is read as if it started at the first row and column, as in the reference. Indexed tab colours
//! are not read. Every value is checked against the bounds of [`crate::sheet_view`] and a value over
//! them is ignored, not clamped.

use crate::sheet_styling::sheet_view::{Freeze, LevelRun, MAX_OUTLINE_LEVEL};
use crate::sheet_styling::SheetStyling;
use crate::xml_stream::{unescape_cow, Attrs, XmlEvent};
use crate::OoxmlCodec;
use std::fmt::Write;

/// Office colours by SpreadsheetML theme index: light 1, dark 1, light 2, dark 2, six accents, the
/// two hyperlink colours. The palette of a file without a theme part.
pub(crate) const DEFAULT_THEME: [u32; 12] = [
    0xFFFFFF, 0x000000, 0xE7E6E6, 0x44546A, 0x4472C4, 0xED7D31, 0xA5A5A5, 0xFFC000, 0x5B9BD5,
    0x70AD47, 0x0563C1, 0x954F72,
];

/// A tab colour that names a theme slot: it needs the workbook's theme to become RGB.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ThemeTab {
    pub(crate) index: usize,
    pub(crate) tint: f64,
}

/// `view` data of one worksheet as the scan finds it.
#[derive(Default)]
pub(crate) struct ViewScan {
    in_view: bool,
    views_seen: u8,
    pub(crate) freeze: Option<Freeze>,
    pub(crate) tab_rgb: Option<u32>,
    pub(crate) tab_theme: Option<ThemeTab>,
    pub(crate) summary_above: bool,
    pub(crate) summary_left: bool,
}

/// `value` as a count of frozen rows or columns. Both writers of this value exist: whole numbers
/// and decimals such as `6.0`; anything else is refused.
fn split_count(value: Option<std::borrow::Cow<'_, str>>) -> Option<u32> {
    let number = value?.trim().parse::<f64>().ok()?;
    (number.is_finite() && (0.0..=f64::from(u32::MAX)).contains(&number))
        .then(|| number.round() as u32)
}

fn off(attrs: &Attrs<'_>, name: &str) -> bool {
    matches!(attrs.get(name).as_deref(), Some("0" | "false"))
}

/// An `rgb` attribute (`AARRGGBB` or `RRGGBB`) as `0xRRGGBB`.
fn parse_argb(text: &str) -> Option<u32> {
    let hex = match text.len() {
        8 => &text[2..],
        6 => text,
        _ => return None,
    };
    hex.bytes()
        .all(|b| b.is_ascii_hexdigit())
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
}

impl ViewScan {
    pub(crate) fn feed(&mut self, event: &XmlEvent<'_>) {
        match event {
            XmlEvent::Start { name, attrs } => match *name {
                "sheetView" => {
                    self.views_seen = self.views_seen.saturating_add(1);
                    self.in_view = self.views_seen == 1;
                }
                "pane" if self.in_view => self.pane(attrs),
                "tabColor" => self.tab(attrs),
                "outlinePr" => {
                    self.summary_above = off(attrs, "summaryBelow");
                    self.summary_left = off(attrs, "summaryRight");
                }
                _ => {}
            },
            XmlEvent::End("sheetView") => self.in_view = false,
            _ => {}
        }
    }

    fn pane(&mut self, attrs: &Attrs<'_>) {
        if !matches!(
            attrs.get("state").as_deref(),
            Some("frozen" | "frozenSplit")
        ) {
            return;
        }
        let rows = split_count(attrs.get("ySplit")).unwrap_or(0);
        let cols = split_count(attrs.get("xSplit")).unwrap_or(0);
        self.freeze = Freeze::new(rows, cols);
    }

    fn tab(&mut self, attrs: &Attrs<'_>) {
        self.tab_rgb = attrs.get("rgb").as_deref().and_then(parse_argb);
        self.tab_theme = None;
        if self.tab_rgb.is_some() {
            return;
        }
        let index = attrs
            .get("theme")
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|&i| i < DEFAULT_THEME.len());
        let tint = attrs
            .get("tint")
            .map_or(Some(0.0), |v| v.trim().parse::<f64>().ok())
            .filter(|t| t.is_finite() && (-1.0..=1.0).contains(t));
        self.tab_theme = index
            .zip(tint)
            .map(|(index, tint)| ThemeTab { index, tint });
    }
}

/// The colours of `xl/theme/theme1.xml` by SpreadsheetML theme index; a slot the part does not
/// give keeps the Office colour.
pub(crate) fn theme_palette(xml: &str) -> [u32; 12] {
    use crate::xml_stream::XmlReader;
    // The part lists dk1, lt1, dk2, lt2, accent1-6, hlink, folHlink; SpreadsheetML swaps each pair.
    const SLOTS: [(&str, usize); 12] = [
        ("dk1", 1),
        ("lt1", 0),
        ("dk2", 3),
        ("lt2", 2),
        ("accent1", 4),
        ("accent2", 5),
        ("accent3", 6),
        ("accent4", 7),
        ("accent5", 8),
        ("accent6", 9),
        ("hlink", 10),
        ("folHlink", 11),
    ];
    let mut palette = DEFAULT_THEME;
    let mut reader = XmlReader::unbounded(xml);
    let mut slot: Option<usize> = None;
    while let Ok(Some(event)) = reader.next_event() {
        match event {
            XmlEvent::Start { name, attrs } => {
                let local = name.rsplit(':').next().unwrap_or(name);
                if let Some(&(_, index)) = SLOTS.iter().find(|(n, _)| *n == local) {
                    slot = Some(index);
                } else if let Some(index) = slot {
                    let value = match local {
                        "srgbClr" => attrs.get("val"),
                        "sysClr" => attrs.get("lastClr"),
                        _ => continue,
                    };
                    if let Some(rgb) = value.as_deref().and_then(parse_argb) {
                        palette[index] = rgb;
                    }
                }
            }
            XmlEvent::End(name) => {
                let local = name.rsplit(':').next().unwrap_or(name);
                if SLOTS.iter().any(|(n, _)| *n == local) {
                    slot = None;
                }
            }
            _ => {}
        }
    }
    palette
}

/// `rgb` lightened or darkened by `tint` (ECMA-376 18.3.1.15: the HLS luminance moves toward white
/// for a positive tint and toward black for a negative one).
pub(crate) fn apply_tint(rgb: u32, tint: f64) -> u32 {
    if tint == 0.0 {
        return rgb;
    }
    let [r, g, b] =
        [(rgb >> 16) & 0xFF, (rgb >> 8) & 0xFF, rgb & 0xFF].map(|v| f64::from(v) / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    let (h, s) = if d == 0.0 {
        (0.0, 0.0)
    } else {
        let s = d / (1.0 - (2.0 * l - 1.0).abs());
        let h = if max == r {
            ((g - b) / d).rem_euclid(6.0)
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        (h, s)
    };
    let l = if tint < 0.0 {
        l * (1.0 + tint)
    } else {
        l * (1.0 - tint) + tint
    };
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let (r1, g1, b1) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let byte = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u32;
    (byte(r1) << 16) | (byte(g1) << 8) | byte(b1)
}

/// `<sheetPr>`: the tab colour and the side of the group summaries (empty when both are default).
pub(crate) fn sheet_pr_xml(styling: &SheetStyling) -> String {
    let outline = &styling.outline;
    if styling.tab_color.is_none() && !outline.summary_above && !outline.summary_left {
        return String::new();
    }
    let mut xml = String::from("<sheetPr>");
    if let Some(rgb) = styling.tab_color {
        let _ = write!(xml, "<tabColor rgb=\"FF{rgb:06X}\"/>");
    }
    if outline.summary_above || outline.summary_left {
        let _ = write!(
            xml,
            "<outlinePr summaryBelow=\"{}\" summaryRight=\"{}\"/>",
            u8::from(!outline.summary_above),
            u8::from(!outline.summary_left)
        );
    }
    xml.push_str("</sheetPr>");
    xml
}

/// `<sheetViews>` with the frozen pane (empty when nothing is frozen).
pub(crate) fn sheet_views_xml(freeze: Option<Freeze>) -> String {
    let Some(Freeze { rows, cols }) = freeze else {
        return String::new();
    };
    let top_left = format!(
        "{}{}",
        crate::rtf_xml_helpers::format_col_letters(cols),
        rows + 1
    );
    let mut xml = String::from("<sheetViews><sheetView workbookViewId=\"0\">");
    match (rows > 0, cols > 0) {
        (true, true) => {
            let _ = write!(
                xml,
                "<pane xSplit=\"{cols}\" ySplit=\"{rows}\" topLeftCell=\"{top_left}\" activePane=\"bottomRight\" state=\"frozen\"/>\
                 <selection pane=\"topRight\"/><selection pane=\"bottomLeft\"/><selection pane=\"bottomRight\"/>"
            );
        }
        (true, false) => {
            let _ = write!(
                xml,
                "<pane ySplit=\"{rows}\" topLeftCell=\"{top_left}\" activePane=\"bottomLeft\" state=\"frozen\"/>\
                 <selection pane=\"bottomLeft\"/>"
            );
        }
        _ => {
            let _ = write!(
                xml,
                "<pane xSplit=\"{cols}\" topLeftCell=\"{top_left}\" activePane=\"topRight\" state=\"frozen\"/>\
                 <selection pane=\"topRight\"/>"
            );
        }
    }
    xml.push_str("</sheetView></sheetViews>");
    xml
}

/// `<sheetFormatPr>` with the deepest group level (empty when there are no groups).
pub(crate) fn sheet_format_pr_xml(styling: &SheetStyling) -> String {
    let (rows, cols) = (
        styling.outline.max_level(false),
        styling.outline.max_level(true),
    );
    if rows == 0 && cols == 0 {
        return String::new();
    }
    let mut xml = String::from("<sheetFormatPr defaultRowHeight=\"15\"");
    if rows > 0 {
        let _ = write!(xml, " outlineLevelRow=\"{rows}\"");
    }
    if cols > 0 {
        let _ = write!(xml, " outlineLevelCol=\"{cols}\"");
    }
    xml.push_str("/>");
    xml
}

/// Adds `index` with `level` to the sorted runs a scan is building. A row that is out of order, a
/// level over the bound and a list over `max_total` rows are dropped.
pub(crate) fn push_level(
    runs: &mut Vec<LevelRun>,
    total: &mut u64,
    max_total: u64,
    (first, last): (u32, u32),
    level: u8,
) {
    let count = u64::from(last - first) + 1;
    if level == 0 || level > MAX_OUTLINE_LEVEL || *total + count > max_total {
        return;
    }
    match runs.last_mut() {
        Some(prev) if first <= prev.1 => return,
        Some(prev) if prev.1 + 1 == first && prev.2 == level => prev.1 = last,
        _ => {
            if runs.len() >= crate::sheet_styling::sheet_view::MAX_OUTLINE_RUNS {
                return;
            }
            runs.push((first, last, level));
        }
    }
    *total += count;
}

/// `outlineLevel` of a `row` or `col` element: 0 when it is absent, not a number or over the bound.
pub(crate) fn level_attr(attrs: &Attrs<'_>) -> u8 {
    attrs
        .iter_raw()
        .find(|(n, _)| n.strip_prefix("eo:").unwrap_or(n) == "outlineLevel")
        .and_then(|(_, v)| unescape_cow(v).trim().parse::<u8>().ok())
        .filter(|&l| l <= MAX_OUTLINE_LEVEL)
        .unwrap_or(0)
}

/// Largest column index (zero-based) a column element may name.
pub(crate) const LAST_COL: u32 = OoxmlCodec::MAX_XLSX_COLS - 1;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml_stream::XmlReader;

    fn scan(xml: &str) -> ViewScan {
        let mut view = ViewScan::default();
        let mut reader = XmlReader::unbounded(xml);
        while let Some(event) = reader.next_event().unwrap() {
            view.feed(&event);
        }
        view
    }

    #[test]
    fn a_frozen_pane_is_read_with_decimal_counts() {
        let view = scan(
            r#"<sheetViews><sheetView><pane xSplit="3" ySplit="2.0" topLeftCell="D3" state="frozen"/></sheetView></sheetViews>"#,
        );
        assert_eq!(view.freeze, Freeze::new(2, 3));
        let split = scan(r#"<sheetView><pane xSplit="3" state="split"/></sheetView>"#);
        assert_eq!(split.freeze, None);
    }

    #[test]
    fn hostile_split_values_leave_nothing_frozen() {
        for bad in [
            r#"ySplit="4294967296""#,
            r#"ySplit="1e30""#,
            r#"xSplit="-3""#,
            r#"ySplit="NaN""#,
            r#"ySplit="1001""#,
            r#"xSplit="101""#,
            r#"xSplit="x""#,
        ] {
            let xml = format!(r#"<sheetView><pane {bad} state="frozen"/></sheetView>"#);
            assert_eq!(scan(&xml).freeze, None, "{bad}");
        }
    }

    #[test]
    fn only_the_first_sheet_view_counts() {
        let view = scan(
            r#"<sheetViews><sheetView/><sheetView><pane ySplit="2" state="frozen"/></sheetView></sheetViews>"#,
        );
        assert_eq!(view.freeze, None);
    }

    #[test]
    fn tab_colours_come_from_rgb_or_theme_and_bad_ones_are_ignored() {
        let rgb = scan(r#"<sheetPr><tabColor rgb="FF3366CC"/></sheetPr>"#);
        assert_eq!(rgb.tab_rgb, Some(0x3366CC));
        let theme = scan(r#"<tabColor theme="5" tint="-0.25"/>"#);
        assert_eq!(
            theme.tab_theme,
            Some(ThemeTab {
                index: 5,
                tint: -0.25
            })
        );
        for bad in [
            r#"rgb="zzzzzzzz""#,
            r#"rgb="FF12""#,
            r#"rgb="FF3366CC00""#,
            r#"theme="12""#,
            r#"theme="4" tint="2""#,
            r#"theme="4" tint="NaN""#,
            r#"indexed="12""#,
        ] {
            let view = scan(&format!("<tabColor {bad}/>"));
            assert!(view.tab_rgb.is_none() && view.tab_theme.is_none(), "{bad}");
        }
    }

    #[test]
    fn the_theme_part_gives_the_palette_and_tint_moves_luminance() {
        let xml = r#"<a:theme><a:themeElements><a:clrScheme name="x"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1><a:accent1><a:srgbClr val="123456"/></a:accent1></a:clrScheme></a:themeElements></a:theme>"#;
        let palette = theme_palette(xml);
        assert_eq!(palette[4], 0x123456);
        assert_eq!(palette[5], DEFAULT_THEME[5]);
        assert_eq!(apply_tint(0xFFFFFF, -0.5), 0x808080);
        assert_eq!(apply_tint(0x000000, 0.5), 0x808080);
        assert_eq!(apply_tint(0x4472C4, 0.0), 0x4472C4);
        // Lighter and darker keep the hue ordering.
        let light = apply_tint(0x4472C4, 0.6);
        let dark = apply_tint(0x4472C4, -0.5);
        assert!(light > 0x4472C4 && (dark >> 16) < 0x44);
    }

    #[test]
    fn views_are_written_with_the_pane_of_each_kind() {
        assert_eq!(sheet_views_xml(None), "");
        let both = sheet_views_xml(Freeze::new(2, 3));
        assert!(both.contains(
            r#"<pane xSplit="3" ySplit="2" topLeftCell="D3" activePane="bottomRight" state="frozen"/>"#
        ));
        let rows = sheet_views_xml(Freeze::new(1, 0));
        assert!(rows.contains(
            r#"<pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/>"#
        ));
        let cols = sheet_views_xml(Freeze::new(0, 27));
        assert!(cols.contains(
            r#"<pane xSplit="27" topLeftCell="AB1" activePane="topRight" state="frozen"/>"#
        ));
    }

    #[test]
    fn levels_are_collected_in_order_and_bounded() {
        let (mut runs, mut total) = (Vec::new(), 0);
        push_level(&mut runs, &mut total, 10, (1, 1), 1);
        push_level(&mut runs, &mut total, 10, (2, 3), 1);
        push_level(&mut runs, &mut total, 10, (4, 4), 2);
        push_level(&mut runs, &mut total, 10, (3, 3), 1);
        push_level(&mut runs, &mut total, 10, (9, 9), 8);
        push_level(&mut runs, &mut total, 10, (9, 9), 0);
        assert_eq!(runs, vec![(1, 3, 1), (4, 4, 2)]);
        assert_eq!(total, 4);
        push_level(&mut runs, &mut total, 5, (10, 10), 1);
        assert_eq!((runs.len(), total), (3, 5));
        push_level(&mut runs, &mut total, 5, (11, 12), 1);
        assert_eq!((runs.len(), total), (3, 5));
    }
}

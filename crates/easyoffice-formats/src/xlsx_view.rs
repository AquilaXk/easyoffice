//! The parts of a worksheet that describe how it is shown (`sheetPr`, `sheetViews`, `sheetFormatPr`)
//! and the group levels of its rows (`row@outlineLevel`, `row@collapsed`) (PD-208).
//!
//! 참조 구현: IronCalc `xlsx/src/import/worksheets.rs` lines 317-335 (`load_sheet_color`, `tabColor`
//! with `rgb` or `theme` and `tint`), 800-905 (`load_sheet_view`, `get_split`: the `pane` of a frozen sheet,
//! `xSplit` and `ySplit` read as numbers because some writers store `6.0`) and
//! `xlsx/src/export/worksheets.rs` lines 660-725 (`pane` with `topLeftCell`, `activePane` and one
//! `selection` per pane), Apache-2.0 or MIT, Copyright (c) IronCalc contributors, commit
//! 87b684b. The reference ignores `outlineLevel`, so the levels follow ECMA-376 18.3.1.73 and
//! 18.3.1.31, and the theme tint follows 18.3.1.15 (HLS luminance).
//!
//! Modifications: a frozen pane whose sheet view is scrolled (`sheetView@topLeftCell` away from A1)
//! is read as if it started at the first row and column, as in the reference. Every value is checked
//! against the bounds of [`crate::sheet_view`] and a value over them is ignored, not clamped. The
//! selection, the zoom and the scroll position (`sheetView@zoomScale`, `topLeftCell`, the `selection` of
//! the active pane) are read as well, which the reference leaves out (it ignores all of them).

use crate::rtf_xml_helpers::{format_col_letters, parse_a1_zero_based};
use crate::sheet_styling::sheet_view::{Freeze, ViewState, MAX_OUTLINE_LEVEL};
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

/// The four panes of a sheet view (ECMA-376 18.18.52 `ST_Pane`), in the order a scan indexes them.
const PANES: [&str; 4] = ["topLeft", "topRight", "bottomLeft", "bottomRight"];

/// The first range of a `sqref` as `(first, last)` cells; `None` when it is not a cell or a range.
fn first_range(sqref: &str) -> Option<((u32, u32), (u32, u32))> {
    let first = sqref.split_whitespace().next()?;
    let (a, b) = first.split_once(':').unwrap_or((first, first));
    Some((parse_a1_zero_based(a)?, parse_a1_zero_based(b)?))
}

/// `view` data of one worksheet as the scan finds it.
#[derive(Default)]
pub(crate) struct ViewScan {
    in_view: bool,
    views_seen: u8,
    zoom: Option<u16>,
    sheet_top_left: Option<(u32, u32)>,
    pane_top_left: Option<(u32, u32)>,
    active_pane: usize,
    /// The selection of each pane, by the index in [`PANES`].
    selections: [Option<ViewState>; 4],
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
                    if self.in_view {
                        self.view(attrs);
                    }
                }
                "pane" if self.in_view => self.pane(attrs),
                "selection" if self.in_view => self.selection(attrs),
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

    fn view(&mut self, attrs: &Attrs<'_>) {
        self.zoom = attrs
            .get("zoomScale")
            .and_then(|v| v.trim().parse::<u32>().ok())
            .and_then(ViewState::zoom_of);
        self.sheet_top_left = attrs
            .get("topLeftCell")
            .and_then(|v| parse_a1_zero_based(&v));
    }

    fn selection(&mut self, attrs: &Attrs<'_>) {
        let pane = attrs.get("pane");
        let at = PANES
            .iter()
            .position(|&p| Some(p) == pane.as_deref())
            .unwrap_or(0);
        let range = attrs.get("sqref").and_then(|v| first_range(&v));
        let active = attrs
            .get("activeCell")
            .and_then(|v| parse_a1_zero_based(&v));
        // The active cell may lie in any range of `sqref`; this model keeps the first range, so a cell
        // outside it makes the cell itself the selection.
        self.selections[at] = match (active, range) {
            (Some(a), Some((from, to))) => Some(ViewState::selecting(a, from, to)),
            (Some(a), None) => Some(ViewState::selecting(a, a, a)),
            (None, Some((from, to))) => Some(ViewState::selecting(from, from, to)),
            (None, None) => None,
        };
    }

    /// The selection, zoom and scroll position the scan found.
    pub(crate) fn state(&self) -> ViewState {
        let selected = self.selections[self.active_pane]
            .or_else(|| self.selections.iter().flatten().next().copied())
            .unwrap_or_default();
        let top_left = match self.freeze {
            Some(_) => self.pane_top_left,
            None => self.sheet_top_left,
        };
        ViewState {
            zoom: self.zoom,
            top_left: top_left.unwrap_or_default(),
            ..selected
        }
    }

    fn pane(&mut self, attrs: &Attrs<'_>) {
        self.pane_top_left = attrs
            .get("topLeftCell")
            .and_then(|v| parse_a1_zero_based(&v));
        if let Some(at) = attrs
            .get("activePane")
            .and_then(|p| PANES.iter().position(|&n| n == p))
        {
            self.active_pane = at;
        }
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
    let fit = crate::sheet_styling::xlsx_page::fit_to_page_xml(&styling.page);
    if styling.tab_color.is_none()
        && !outline.summary_above
        && !outline.summary_left
        && fit.is_empty()
    {
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
    xml.push_str(fit);
    xml.push_str("</sheetPr>");
    xml
}

/// The A1 name of a zero-based `(row, col)`.
fn a1((row, col): (u32, u32)) -> String {
    format!("{}{}", format_col_letters(col), row + 1)
}

/// `sqref` of a selection: the cell, or `A1:B2`.
fn sqref_of(range: (u32, u32, u32, u32)) -> String {
    let (r0, c0, r1, c1) = range;
    if (r0, c0) == (r1, c1) {
        a1((r0, c0))
    } else {
        format!("{}:{}", a1((r0, c0)), a1((r1, c1)))
    }
}

/// `<sheetViews>` with the frozen pane, the selection, the zoom and the scroll position (empty when none
/// is set).
///
/// The pane that holds the active cell carries the selection; the others get an empty `selection`, which
/// the format asks for. `sheetView@topLeftCell` is the top left cell of the top left pane, which shares its
/// rows with the top right pane and its columns with the bottom left one; `pane@topLeftCell` is the first
/// cell of the scrolling pane.
pub(crate) fn sheet_views_xml(freeze: Option<Freeze>, view: &ViewState) -> String {
    if freeze.is_none() && view.is_default() {
        return String::new();
    }
    let Freeze { rows, cols } = freeze.unwrap_or(Freeze { rows: 0, cols: 0 });
    let zoom = view.zoom.map_or(String::new(), |z| {
        format!(" zoomScale=\"{z}\" zoomScaleNormal=\"{z}\"")
    });
    let selected = (view.active != (0, 0) || view.range != (0, 0, 0, 0)).then(|| {
        format!(
            "activeCell=\"{}\" sqref=\"{}\"",
            a1(view.active),
            sqref_of(view.range)
        )
    });
    // The scrolling pane never starts above or left of the frozen ones.
    let (scroll_row, scroll_col) = (view.top_left.0.max(rows), view.top_left.1.max(cols));
    let (view_top_left, panes) = match (rows > 0, cols > 0) {
        (false, false) => (
            view.top_left,
            selected.map_or(String::new(), |s| format!("<selection {s}/>")),
        ),
        (has_rows, has_cols) => {
            // The pane the active cell lies in; a sheet nobody selected in is left in the scrolling pane.
            let scrolling = match (has_rows, has_cols) {
                (true, true) => "bottomRight",
                (true, false) => "bottomLeft",
                _ => "topRight",
            };
            let above = has_rows && view.active.0 < rows;
            let left = has_cols && view.active.1 < cols;
            let active_pane = match (selected.is_some(), above, left) {
                (false, ..) | (_, false, false) => scrolling,
                (_, true, true) => "topLeft",
                (_, true, false) if has_cols => "topRight",
                (_, false, true) if has_rows => "bottomLeft",
                _ => "topLeft",
            };
            let mut elements = String::new();
            for name in ["topLeft", "topRight", "bottomLeft", "bottomRight"] {
                let exists = match name {
                    "topLeft" => name == active_pane,
                    "topRight" => has_cols,
                    "bottomLeft" => has_rows,
                    _ => has_rows && has_cols,
                };
                if !exists {
                    continue;
                }
                let pane = if name == "topLeft" {
                    String::new()
                } else {
                    format!(" pane=\"{name}\"")
                };
                let chosen = selected.as_deref().filter(|_| name == active_pane);
                let _ = write!(
                    elements,
                    "<selection{pane}{}/>",
                    chosen.map_or(String::new(), |s| format!(" {s}"))
                );
            }
            let (view_row, view_col) = (
                if has_rows { 0 } else { scroll_row },
                if has_cols { 0 } else { scroll_col },
            );
            let split = format!(
                "{}{}",
                if has_cols {
                    format!(" xSplit=\"{cols}\"")
                } else {
                    String::new()
                },
                if has_rows {
                    format!(" ySplit=\"{rows}\"")
                } else {
                    String::new()
                },
            );
            // With only rows frozen the top left pane scrolls sideways with the scrolling one, with only
            // columns frozen it scrolls down with it.
            (
                (view_row, view_col),
                format!(
                    "<pane{split} topLeftCell=\"{}\" activePane=\"{active_pane}\" state=\"frozen\"/>{elements}",
                    a1((scroll_row, scroll_col)),
                ),
            )
        }
    };
    let top_left = if view_top_left == (0, 0) {
        String::new()
    } else {
        format!(" topLeftCell=\"{}\"", a1(view_top_left))
    };
    format!(
        "<sheetViews><sheetView workbookViewId=\"0\"{zoom}{top_left}>{panes}</sheetView></sheetViews>"
    )
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
        let plain = ViewState::default();
        assert_eq!(sheet_views_xml(None, &plain), "");
        let both = sheet_views_xml(Freeze::new(2, 3), &plain);
        assert!(both.contains(
            r#"<pane xSplit="3" ySplit="2" topLeftCell="D3" activePane="bottomRight" state="frozen"/>"#
        ));
        let rows = sheet_views_xml(Freeze::new(1, 0), &plain);
        assert!(rows.contains(
            r#"<pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/>"#
        ));
        let cols = sheet_views_xml(Freeze::new(0, 27), &plain);
        assert!(cols.contains(
            r#"<pane xSplit="27" topLeftCell="AB1" activePane="topRight" state="frozen"/>"#
        ));
    }

    fn state_of(xml: &str) -> ViewState {
        scan(xml).state()
    }

    #[test]
    fn the_selection_zoom_and_scroll_of_a_plain_sheet_are_read() {
        let view = state_of(
            r#"<sheetViews><sheetView zoomScale="150" topLeftCell="D10" workbookViewId="0"><selection activeCell="G14" sqref="F12:G14"/></sheetView></sheetViews>"#,
        );
        assert_eq!(view.active, (13, 6));
        assert_eq!(view.range, (11, 5, 13, 6));
        assert_eq!(view.zoom, Some(150));
        assert_eq!(view.top_left, (9, 3));
        // A reversed range is the same range; the first range of a list is the selection.
        let view =
            state_of(r#"<sheetView><selection activeCell="B2" sqref="C3:A1 E5"/></sheetView>"#);
        assert_eq!((view.active, view.range), ((1, 1), (0, 0, 2, 2)));
        // An active cell outside the first range selects itself.
        let view = state_of(r#"<sheetView><selection activeCell="E5" sqref="A1:B2"/></sheetView>"#);
        assert_eq!((view.active, view.range), ((4, 4), (4, 4, 4, 4)));
        // No attribute at all is the default view.
        assert!(state_of("<sheetView/>").is_default());
    }

    #[test]
    fn a_frozen_sheet_reads_the_selection_of_its_active_pane_and_the_scrolling_panes_corner() {
        let view = state_of(
            r#"<sheetView zoomScale="80"><pane xSplit="2" ySplit="3" topLeftCell="E20" activePane="bottomRight" state="frozen"/><selection pane="topRight"/><selection pane="bottomLeft" activeCell="A9" sqref="A9"/><selection pane="bottomRight" activeCell="H25" sqref="H25:J26"/></sheetView>"#,
        );
        assert_eq!(view.active, (24, 7));
        assert_eq!(view.range, (24, 7, 25, 9));
        assert_eq!(view.top_left, (19, 4));
        assert_eq!(view.zoom, Some(80));
        // The sheet view's own `topLeftCell` is the top left pane's corner, not the scroll position.
        let view = state_of(
            r#"<sheetView topLeftCell="A1"><pane ySplit="1" topLeftCell="A40" activePane="bottomLeft" state="frozen"/><selection pane="bottomLeft" activeCell="C41" sqref="C41"/></sheetView>"#,
        );
        assert_eq!((view.active, view.top_left), ((40, 2), (39, 0)));
        // The selection of a pane that is not the active one is the fallback.
        let view = state_of(
            r#"<sheetView><pane ySplit="1" activePane="bottomLeft" state="frozen"/><selection pane="topLeft" activeCell="B1" sqref="B1"/></sheetView>"#,
        );
        assert_eq!(view.active, (0, 1));
    }

    #[test]
    fn hostile_view_attributes_are_dropped_not_clamped() {
        for bad in [
            r#"zoomScale="9""#,
            r#"zoomScale="401""#,
            r#"zoomScale="NaN""#,
            r#"zoomScale="-100""#,
            r#"zoomScale="4294967396""#,
            r#"zoomScale="1e2""#,
            r#"zoomScale="100""#,
        ] {
            let view = state_of(&format!("<sheetView {bad}/>"));
            assert_eq!(view.zoom, None, "{bad}");
        }
        for bad in [
            "XFE1", "A1048577", "ZZZZ1", "A0", "1A", "A", "", "$$", "A1:B2",
        ] {
            let view = state_of(&format!(r#"<sheetView topLeftCell="{bad}"/>"#));
            assert_eq!(view.top_left, (0, 0), "{bad}");
        }
        for bad in ["XFE1:A1", "A1:B", "garbage", "", "A1048577:A1"] {
            let view = state_of(&format!(
                r#"<sheetView><selection sqref="{bad}"/></sheetView>"#
            ));
            assert!(view.is_default(), "{bad}");
        }
        let edge = state_of(
            r#"<sheetView topLeftCell="XFD1048576"><selection activeCell="XFD1048576" sqref="A1:XFD1048576"/></sheetView>"#,
        );
        assert_eq!(edge.top_left, (1_048_575, 16_383));
        assert!(edge.within_bounds(1_048_576, 16_384));
    }

    #[test]
    fn a_view_is_written_and_read_back_for_every_freeze_and_active_pane() {
        let freezes = [
            None,
            Freeze::new(2, 0),
            Freeze::new(0, 3),
            Freeze::new(2, 3),
        ];
        let cells = [(0, 0), (0, 5), (7, 0), (9, 8), (1, 1)];
        for freeze in freezes {
            for active in cells {
                for zoom in [None, Some(10), Some(125), Some(400)] {
                    let (fr, fc) = freeze.map_or((0, 0), |f| (f.rows, f.cols));
                    let view = ViewState {
                        zoom,
                        top_left: (fr + 4, fc + 6),
                        ..ViewState::selecting(active, active, (active.0 + 1, active.1 + 2))
                    };
                    let xml = sheet_views_xml(freeze, &view);
                    let read = scan(&xml);
                    assert_eq!(read.freeze, freeze, "{xml}");
                    assert_eq!(read.state(), view, "{xml}");
                }
            }
        }
        // A sheet nobody selected anything in keeps its scroll position and nothing else.
        let scrolled = ViewState {
            top_left: (30, 4),
            ..ViewState::default()
        };
        let xml = sheet_views_xml(None, &scrolled);
        assert!(
            xml.contains(r#"topLeftCell="E31""#) && !xml.contains("selection"),
            "{xml}"
        );
        assert_eq!(scan(&xml).state(), scrolled);
    }

    #[test]
    fn a_zoom_of_100_is_not_written() {
        let xml = sheet_views_xml(
            None,
            &ViewState {
                zoom: ViewState::zoom_of(100),
                ..ViewState::default()
            },
        );
        assert_eq!(xml, "");
        let xml = sheet_views_xml(
            None,
            &ViewState {
                zoom: ViewState::zoom_of(200),
                ..ViewState::default()
            },
        );
        assert!(
            xml.contains(r#"zoomScale="200" zoomScaleNormal="200""#),
            "{xml}"
        );
    }
}

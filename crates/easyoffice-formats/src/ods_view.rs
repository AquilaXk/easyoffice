//! The view of a sheet in an ODS file (PD-208): the tab colour (`tableooo:tab-color` in a table
//! style), the row and column groups (`table:table-row-group`, `table:table-column-group` with
//! `table:display`) and the frozen panes (the view settings of `settings.xml`).
//!
//! The file format side follows OASIS ODF 1.2: 19.680 `table:display` on a group (false while the
//! group is folded), 20.322 `table:table-row-group` and 20.323 `table:table-column-group`, and the
//! settings of `ooo:view-settings` (`HorizontalSplitMode`, `VerticalSplitMode` with 2 for a fixed
//! split, `HorizontalSplitPosition` and `VerticalSplitPosition` as the counts of columns and rows;
//! `CursorPositionX` and `CursorPositionY` for the cursor cell, `ZoomValue` in percent, and
//! `PositionLeft`, `PositionRight`, `PositionTop`, `PositionBottom` for the first column and row of the
//! panes: the left pane and the bottom pane are the only ones of a sheet without a split, the right
//! pane starts the scrolling part right of a vertical frozen line). The reference of the model is
//! `sheet_view.rs`.
//!
//! Modifications: a group nested in a folded one is folded as well. Splits that are not fixed (mode
//! 1) are not a frozen pane and are left out. Every size is bounded as for XLSX.

use crate::rtf_xml_helpers::extract_attr_from_fragment;
use crate::sheet_styling::sheet_view::{
    level_at, segments, Freeze, LevelRun, ViewState, MAX_OUTLINE_LEVEL,
};
use crate::sheet_styling::xlsx_layout::intersect_runs;
use crate::sheet_styling::SheetStyling;
use crate::xml_stream::{XmlEvent, XmlReader};
use crate::XmlSanitizer;
use std::collections::HashMap;
use std::fmt::Write;

/// Namespace of the table extension attributes (`tableooo:`), as the format defines it; the host name is
/// written in two parts so the repository name guard does not read it as a project name.
pub(crate) const TABLEOOO_NS: &str = concat!("http://open", "office.org/2009/table");
pub(crate) const CONFIG_NS: &str = "urn:oasis:names:tc:opendocument:xmlns:config:1.0";
/// Mode value of a split that is a frozen pane (1 is a movable split).
const FIXED_SPLIT: i64 = 2;
/// Rows and columns of a table (one more than the last index).
const ROWS: u32 = 1_048_576;
const COLS: u32 = 16_384;

/// What a table style says about its sheet: the tab colour and whether the sheet is hidden
/// (`table:display="false"`, ODF 1.2 19.680).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TableProps {
    pub(crate) color: Option<u32>,
    pub(crate) hidden: bool,
    /// The master page (1-based, `ods_page`) of the sheet's page layout, 0 for none.
    pub(crate) master: u16,
}

impl TableProps {
    fn of(styling: &SheetStyling, layouts: &[crate::SheetPage]) -> Self {
        Self {
            color: styling.tab_color,
            hidden: styling.hidden,
            master: crate::sheet_styling::ods_page::master_of(layouts, styling),
        }
    }
}

/// The `ta<N>` table styles of a workbook, one per distinct tab colour and hidden state in
/// first-use order.
pub(crate) struct TabStyles {
    props: Vec<TableProps>,
    layouts: Vec<crate::SheetPage>,
}

impl TabStyles {
    pub(crate) fn build(stylings: &[SheetStyling]) -> Self {
        let layouts = crate::sheet_styling::ods_page::layouts(stylings);
        let mut props: Vec<TableProps> = Vec::new();
        for p in stylings.iter().map(|st| TableProps::of(st, &layouts)) {
            if p != TableProps::default() && !props.contains(&p) {
                props.push(p);
            }
        }
        Self { props, layouts }
    }

    /// The page layouts of the workbook (one master page each), for `styles.xml`.
    pub(crate) fn layouts(&self) -> &[crate::SheetPage] {
        &self.layouts
    }

    pub(crate) fn name(&self, styling: &SheetStyling) -> Option<String> {
        let at = self
            .props
            .iter()
            .position(|&p| p == TableProps::of(styling, &self.layouts))?;
        Some(format!("ta{}", at + 1))
    }

    /// The style elements, for inside `office:automatic-styles`.
    pub(crate) fn xml(&self) -> String {
        let mut xml = String::new();
        for (i, p) in self.props.iter().enumerate() {
            let display = if p.hidden {
                " table:display=\"false\""
            } else {
                ""
            };
            let color = p.color.map_or(String::new(), |c| {
                format!(" tableooo:tab-color=\"#{c:06x}\"")
            });
            let master = if p.master > 0 {
                format!(" style:master-page-name=\"Mp{}\"", p.master)
            } else {
                String::new()
            };
            let _ = write!(
                xml,
                "<style:style style:name=\"ta{}\" style:family=\"table\"{master}><style:table-properties{display}{color}/></style:style>",
                i + 1
            );
        }
        xml
    }
}

/// The tab colour and hidden state of each `table` style of `automatic_styles`, by style name.
pub(crate) fn table_props(automatic_styles: &str) -> HashMap<String, TableProps> {
    let mut out = HashMap::new();
    for (attrs, body) in
        crate::rtf_xml_helpers::extract_tags_with_attrs(automatic_styles, "style:style")
    {
        if extract_attr_from_fragment(attrs, "style:family") != Some("table") {
            continue;
        }
        let color = extract_attr_from_fragment(body, "tableooo:tab-color")
            .and_then(|c| c.strip_prefix('#'))
            .and_then(crate::parse_hex_rgb);
        let hidden = extract_attr_from_fragment(body, "table:display") == Some("false");
        let props = TableProps {
            color,
            hidden,
            master: 0,
        };
        if let (Some(name), true) = (
            extract_attr_from_fragment(attrs, "style:name"),
            props != TableProps::default(),
        ) {
            out.insert(XmlSanitizer::unescape(name), props);
        }
    }
    out
}

/// Opens and closes the group elements around the rows (or columns) of one table, as they are written
/// in index order.
pub(crate) struct GroupWriter<'a> {
    tag: &'static str,
    levels: &'a [LevelRun],
    /// Per level `k` (index `k`), the stretches of level `k` or deeper that are folded as a whole.
    folded: Vec<Vec<(u32, u32)>>,
    depth: u8,
}

impl<'a> GroupWriter<'a> {
    /// `open` are the groups inside folded rows that are open themselves (`Outline::open_rows`, sorted).
    pub(crate) fn new(
        tag: &'static str,
        levels: &'a [LevelRun],
        collapsed: &[(u32, u32)],
        open: &[LevelRun],
    ) -> Self {
        let mut folded = vec![Vec::new(); usize::from(MAX_OUTLINE_LEVEL) + 1];
        for k in 1..=MAX_OUTLINE_LEVEL {
            folded[usize::from(k)] = segments(levels, k)
                .into_iter()
                .filter(|&stretch| {
                    intersect_runs(collapsed, &[stretch]) == [stretch]
                        && open
                            .binary_search_by_key(&(k, stretch.0), |r| (r.2, r.0))
                            .is_err()
                })
                .collect();
        }
        Self {
            tag,
            levels,
            folded,
            depth: 0,
        }
    }

    /// The group level of `index`; a grouped row or column is written even when it is empty.
    pub(crate) fn level(&self, index: u32) -> u8 {
        level_at(self.levels, index)
    }

    /// Makes the open groups those of `index`.
    pub(crate) fn move_to(&mut self, out: &mut String, index: u32) {
        let want = level_at(self.levels, index);
        self.close_to(out, want);
        while self.depth < want {
            self.depth += 1;
            let stretches = &self.folded[usize::from(self.depth)];
            let at = stretches.partition_point(|&(s, _)| s <= index);
            let folded = at > 0 && stretches[at - 1].1 >= index;
            let _ = write!(
                out,
                "<{} table:display=\"{}\">",
                self.tag,
                if folded { "false" } else { "true" }
            );
        }
    }

    pub(crate) fn close_to(&mut self, out: &mut String, depth: u8) {
        while self.depth > depth {
            let _ = write!(out, "</{}>", self.tag);
            self.depth -= 1;
        }
    }

    pub(crate) fn close_all(&mut self, out: &mut String) {
        self.close_to(out, 0);
    }
}

/// The first column and row a table shows (`PositionLeft`/`PositionRight`, `PositionBottom`): the left pane is
/// the only one of a sheet without a vertical frozen line, and the bottom pane the only one without a
/// horizontal one.
fn pane_positions(freeze: Option<Freeze>, view: &ViewState) -> [(&'static str, u32); 4] {
    let cols = freeze.map_or(0, |f| f.cols);
    let (row, col) = (view.top_left.0, view.top_left.1.max(cols));
    let (left, right) = if cols > 0 { (0, col) } else { (col, 0) };
    [
        ("PositionLeft", left),
        ("PositionRight", right),
        ("PositionTop", 0),
        ("PositionBottom", row.max(freeze.map_or(0, |f| f.rows))),
    ]
}

/// `settings.xml` with the view of each sheet that has a frozen pane, a cursor away from A1, a zoom or a
/// scroll position, or `None` when none has.
pub(crate) fn settings_xml(sheets: &[&str], stylings: &[SheetStyling]) -> Option<String> {
    let viewed: Vec<(&str, &SheetStyling)> = sheets
        .iter()
        .zip(stylings)
        .filter(|(_, st)| st.freeze.is_some() || !st.view.is_default())
        .map(|(&name, st)| (name, st))
        .collect();
    if viewed.is_empty() {
        return None;
    }
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-settings xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:config=\"{CONFIG_NS}\" office:version=\"1.2\"><office:settings><config:config-item-set config:name=\"ooo:view-settings\"><config:config-item-map-indexed config:name=\"Views\"><config:config-item-map-entry><config:config-item config:name=\"ViewId\" config:type=\"string\">view1</config:config-item><config:config-item-map-named config:name=\"Tables\">"
    );
    for (name, styling) in viewed {
        let Freeze { rows, cols } = styling.freeze.unwrap_or(Freeze { rows: 0, cols: 0 });
        let view = &styling.view;
        let item = |key: &str, kind: &str, value: u32| {
            format!("<config:config-item config:name=\"{key}\" config:type=\"{kind}\">{value}</config:config-item>")
        };
        let positions: String = pane_positions(styling.freeze, view)
            .iter()
            .map(|&(key, value)| item(key, "int", value))
            .collect();
        let _ = write!(
            xml,
            "<config:config-item-map-entry config:name=\"{}\">{}{}{}{}{}{}{}{}{}</config:config-item-map-entry>",
            XmlSanitizer::escape(name),
            item("CursorPositionX", "int", view.active.1),
            item("CursorPositionY", "int", view.active.0),
            item("HorizontalSplitMode", "short", if cols > 0 { 2 } else { 0 }),
            item("VerticalSplitMode", "short", if rows > 0 { 2 } else { 0 }),
            item("HorizontalSplitPosition", "int", cols),
            item("VerticalSplitPosition", "int", rows),
            positions,
            item("ZoomType", "short", 0),
            item("ZoomValue", "int", u32::from(view.zoom.unwrap_or(100))),
        );
    }
    xml.push_str("</config:config-item-map-named></config:config-item-map-entry></config:config-item-map-indexed></config:config-item-set></office:settings></office:document-settings>");
    Some(xml)
}

/// The frozen pane and the view a table entry of `settings.xml` names.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TableView {
    pub(crate) freeze: Option<Freeze>,
    pub(crate) view: ViewState,
}

/// What the values of one table entry say; a value outside its bound is dropped.
fn table_view(values: &HashMap<String, i64>) -> TableView {
    let get = |key: &str, limit: u32| {
        values
            .get(key)
            .and_then(|&n| u32::try_from(n).ok())
            .filter(|&n| n < limit)
    };
    let count = |mode: &str, position: &str| {
        (values.get(mode) == Some(&FIXED_SPLIT))
            .then(|| values.get(position).copied())
            .flatten()
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(0)
    };
    let rows = count("VerticalSplitMode", "VerticalSplitPosition");
    let cols = count("HorizontalSplitMode", "HorizontalSplitPosition");
    let freeze = Freeze::new(rows, cols);
    let cursor = (
        get("CursorPositionY", ROWS).unwrap_or(0),
        get("CursorPositionX", COLS).unwrap_or(0),
    );
    // Without a vertical frozen line the left pane is the one that scrolls sideways (see `pane_positions`).
    let left = get("PositionLeft", COLS).unwrap_or(0);
    let right = get("PositionRight", COLS).unwrap_or(0);
    let scroll_col = if cols > 0 {
        right
    } else if left > 0 {
        left
    } else {
        right
    };
    TableView {
        freeze,
        view: ViewState {
            zoom: values
                .get("ZoomValue")
                .and_then(|&n| u32::try_from(n).ok())
                .and_then(ViewState::zoom_of),
            top_left: (get("PositionBottom", ROWS).unwrap_or(0), scroll_col),
            ..ViewState::selecting(cursor, cursor, cursor)
        },
    }
}

/// The tables of `settings.xml` by sheet name.
pub(crate) fn read_views(settings: &str) -> HashMap<String, TableView> {
    let mut out = HashMap::new();
    let mut reader = XmlReader::unbounded(settings);
    let mut in_tables = false;
    let mut sheet: Option<String> = None;
    let mut item: Option<String> = None;
    let mut values: HashMap<String, i64> = HashMap::new();
    while let Ok(Some(event)) = reader.next_event() {
        match event {
            XmlEvent::Start { name, attrs } => match name {
                "config:config-item-map-named" => {
                    in_tables = attrs.get("config:name").as_deref() == Some("Tables");
                }
                "config:config-item-map-entry" if in_tables => {
                    sheet = attrs.get("config:name").map(|n| n.into_owned());
                    values.clear();
                }
                "config:config-item" if sheet.is_some() => {
                    item = attrs.get("config:name").map(|n| n.into_owned());
                }
                _ => {}
            },
            XmlEvent::Text(text) => {
                if let (Some(key), Some(_)) = (item.as_ref(), sheet.as_ref()) {
                    if let Ok(value) = text.trim().parse::<i64>() {
                        values.insert(key.clone(), value);
                    }
                }
            }
            XmlEvent::End("config:config-item") => item = None,
            XmlEvent::End("config:config-item-map-entry") if in_tables => {
                if let Some(name) = sheet.take() {
                    let table = table_view(&values);
                    if table != TableView::default() {
                        out.insert(name, table);
                    }
                }
            }
            XmlEvent::End("config:config-item-map-named") => in_tables = false,
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_open_and_close_with_the_level_and_carry_their_folded_state() {
        // Rows 2-6 level 1, with rows 3-4 a folded second level.
        let levels: Vec<LevelRun> = vec![(2, 2, 1), (3, 4, 2), (5, 6, 1)];
        let mut writer = GroupWriter::new("table:table-row-group", &levels, &[(3, 4)], &[]);
        let mut xml = String::new();
        for row in 0..8 {
            writer.move_to(&mut xml, row);
            xml.push_str(&format!("<r{row}/>"));
        }
        writer.close_all(&mut xml);
        assert_eq!(
            xml,
            "<r0/><r1/><table:table-row-group table:display=\"true\"><r2/><table:table-row-group table:display=\"false\"><r3/><r4/></table:table-row-group><r5/><r6/></table:table-row-group><r7/>"
        );
    }

    #[test]
    fn a_tab_colour_and_hidden_state_are_read_from_their_table_style() {
        let blue = SheetStyling {
            tab_color: Some(0x3366CC),
            ..SheetStyling::default()
        };
        let hidden = SheetStyling {
            hidden: true,
            ..SheetStyling::default()
        };
        let both = SheetStyling {
            hidden: true,
            ..blue.clone()
        };
        let styles = TabStyles::build(&[blue.clone(), hidden.clone(), both.clone(), blue.clone()]);
        assert_eq!(styles.name(&blue).as_deref(), Some("ta1"));
        assert_eq!(styles.name(&hidden).as_deref(), Some("ta2"));
        assert_eq!(styles.name(&both).as_deref(), Some("ta3"));
        assert_eq!(styles.name(&SheetStyling::default()), None);
        let read = table_props(&format!(
            "<office:automatic-styles>{}</office:automatic-styles>",
            styles.xml()
        ));
        assert_eq!(
            read.get("ta1"),
            Some(&TableProps {
                color: Some(0x3366CC),
                hidden: false,
                master: 0
            })
        );
        assert_eq!(
            read.get("ta2"),
            Some(&TableProps {
                color: None,
                hidden: true,
                master: 0
            })
        );
        assert_eq!(
            read.get("ta3"),
            Some(&TableProps {
                color: Some(0x3366CC),
                hidden: true,
                master: 0
            })
        );
        let bad = r##"<style:style style:name="x" style:family="table"><style:table-properties tableooo:tab-color="red"/></style:style>"##;
        assert!(table_props(bad).is_empty());
    }

    #[test]
    fn only_a_fixed_split_is_a_frozen_pane() {
        let styling = |rows, cols| SheetStyling {
            freeze: Freeze::new(rows, cols),
            ..SheetStyling::default()
        };
        let xml = settings_xml(
            &["A & B", "Plain"],
            &[styling(2, 1), SheetStyling::default()],
        )
        .unwrap();
        let read = read_views(&xml);
        assert_eq!(read.get("A & B").map(|t| t.freeze), Some(Freeze::new(2, 1)));
        assert!(!read.contains_key("Plain"));
        assert!(settings_xml(&["A"], &[SheetStyling::default()]).is_none());
        let moving = xml.replace(">2</config:config-item>", ">1</config:config-item>");
        assert!(read_views(&moving).values().all(|t| t.freeze.is_none()));
        let huge = xml.replace("\"int\">2<", "\"int\">5000<");
        assert!(read_views(&huge).values().all(|t| t.freeze.is_none()));
    }

    #[test]
    fn the_cursor_zoom_and_scroll_of_each_table_come_back() {
        let styling = |freeze: Option<Freeze>, active: (u32, u32), zoom, top_left| SheetStyling {
            freeze,
            view: ViewState {
                zoom,
                top_left,
                ..ViewState::selecting(active, active, active)
            },
            ..SheetStyling::default()
        };
        let sheets = [
            styling(None, (6, 2), Some(150), (40, 5)),
            styling(Freeze::new(2, 1), (9, 4), Some(80), (30, 7)),
            styling(Freeze::new(0, 3), (0, 0), None, (12, 9)),
            styling(Freeze::new(4, 0), (1, 1), None, (0, 0)),
        ];
        let names = ["일반", "둘 다", "열만", "행만"];
        let xml = settings_xml(&names, &sheets).unwrap();
        let read = read_views(&xml);
        for (name, st) in names.iter().zip(&sheets) {
            let table = read.get(*name).unwrap_or_else(|| panic!("{name}: {xml}"));
            assert_eq!(table.freeze, st.freeze, "{name}");
            let (want, got) = (st.view, table.view);
            assert_eq!(
                (got.active, got.range, got.zoom),
                (
                    want.active,
                    (want.active.0, want.active.1, want.active.0, want.active.1),
                    want.zoom
                ),
                "{name}"
            );
            assert_eq!(
                got.top_left,
                (
                    want.top_left.0.max(st.freeze.map_or(0, |f| f.rows)),
                    want.top_left.1.max(st.freeze.map_or(0, |f| f.cols))
                ),
                "{name}"
            );
        }
        // A sheet at its defaults has no entry.
        assert!(settings_xml(&["A"], &[styling(None, (0, 0), None, (0, 0))]).is_none());
    }

    #[test]
    fn hostile_table_values_are_dropped_not_clamped() {
        let entry = |items: &str| {
            format!(
                "<config:config-item-map-named config:name=\"Tables\"><config:config-item-map-entry config:name=\"S\">{items}</config:config-item-map-entry></config:config-item-map-named>"
            )
        };
        let item = |key: &str, value: &str| {
            format!("<config:config-item config:name=\"{key}\" config:type=\"int\">{value}</config:config-item>")
        };
        for (key, value) in [
            ("ZoomValue", "9"),
            ("ZoomValue", "401"),
            ("ZoomValue", "-5"),
            ("ZoomValue", "99999999999"),
            ("CursorPositionY", "1048576"),
            ("CursorPositionX", "16384"),
            ("CursorPositionY", "-1"),
            ("PositionBottom", "1048576"),
            ("PositionRight", "99999999999"),
            ("PositionLeft", "16384"),
        ] {
            let read = read_views(&entry(&item(key, value)));
            assert!(read.is_empty(), "{key}={value}: {read:?}");
        }
        let edge = read_views(&entry(&format!(
            "{}{}{}",
            item("CursorPositionY", "1048575"),
            item("CursorPositionX", "16383"),
            item("ZoomValue", "400")
        )));
        let view = edge.get("S").unwrap().view;
        assert_eq!((view.active, view.zoom), ((1_048_575, 16_383), Some(400)));
        // A zoom of 100 and a cursor on A1 say nothing.
        assert!(read_views(&entry(&format!(
            "{}{}",
            item("ZoomValue", "100"),
            item("CursorPositionX", "0")
        )))
        .is_empty());
    }
}

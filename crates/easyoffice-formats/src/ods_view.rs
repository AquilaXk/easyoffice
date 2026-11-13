//! The view of a sheet in an ODS file (PD-208): the tab colour (`tableooo:tab-color` in a table
//! style), the row and column groups (`table:table-row-group`, `table:table-column-group` with
//! `table:display`) and the frozen panes (the view settings of `settings.xml`).
//!
//! The file format side follows OASIS ODF 1.2: 19.680 `table:display` on a group (false while the
//! group is folded), 20.322 `table:table-row-group` and 20.323 `table:table-column-group`, and the
//! settings of `ooo:view-settings` (`HorizontalSplitMode`, `VerticalSplitMode` with 2 for a fixed
//! split, `HorizontalSplitPosition` and `VerticalSplitPosition` as the counts of columns and rows).
//! The reference of the model is `sheet_view.rs`.
//!
//! Modifications: a group nested in a folded one is folded as well. Splits that are not fixed (mode
//! 1) are not a frozen pane and are left out. Every size is bounded as for XLSX.

use crate::rtf_xml_helpers::extract_attr_from_fragment;
use crate::sheet_styling::sheet_view::{level_at, segments, Freeze, LevelRun, MAX_OUTLINE_LEVEL};
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

/// What a table style says about its sheet: the tab colour and whether the sheet is hidden
/// (`table:display="false"`, ODF 1.2 19.680).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TableProps {
    pub(crate) color: Option<u32>,
    pub(crate) hidden: bool,
}

impl TableProps {
    fn of(styling: &SheetStyling) -> Self {
        Self {
            color: styling.tab_color,
            hidden: styling.hidden,
        }
    }
}

/// The `ta<N>` table styles of a workbook, one per distinct tab colour and hidden state in
/// first-use order.
pub(crate) struct TabStyles {
    props: Vec<TableProps>,
}

impl TabStyles {
    pub(crate) fn build(stylings: &[SheetStyling]) -> Self {
        let mut props: Vec<TableProps> = Vec::new();
        for p in stylings.iter().map(TableProps::of) {
            if p != TableProps::default() && !props.contains(&p) {
                props.push(p);
            }
        }
        Self { props }
    }

    pub(crate) fn name(&self, styling: &SheetStyling) -> Option<String> {
        let at = self
            .props
            .iter()
            .position(|&p| p == TableProps::of(styling))?;
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
            let _ = write!(
                xml,
                "<style:style style:name=\"ta{}\" style:family=\"table\"><style:table-properties{display}{color}/></style:style>",
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
        let props = TableProps { color, hidden };
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
    pub(crate) fn new(tag: &'static str, levels: &'a [LevelRun], collapsed: &[(u32, u32)]) -> Self {
        let mut folded = vec![Vec::new(); usize::from(MAX_OUTLINE_LEVEL) + 1];
        for k in 1..=MAX_OUTLINE_LEVEL {
            folded[usize::from(k)] = segments(levels, k)
                .into_iter()
                .filter(|&stretch| intersect_runs(collapsed, &[stretch]) == [stretch])
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

/// `settings.xml` with the fixed splits of the sheets that have a frozen pane, or `None` when none has.
pub(crate) fn settings_xml(sheets: &[&str], stylings: &[SheetStyling]) -> Option<String> {
    let frozen: Vec<(&str, Freeze)> = sheets
        .iter()
        .zip(stylings)
        .filter_map(|(&name, st)| Some((name, st.freeze?)))
        .collect();
    if frozen.is_empty() {
        return None;
    }
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-settings xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:config=\"{CONFIG_NS}\" office:version=\"1.2\"><office:settings><config:config-item-set config:name=\"ooo:view-settings\"><config:config-item-map-indexed config:name=\"Views\"><config:config-item-map-entry><config:config-item config:name=\"ViewId\" config:type=\"string\">view1</config:config-item><config:config-item-map-named config:name=\"Tables\">"
    );
    for (name, Freeze { rows, cols }) in frozen {
        let item = |key: &str, kind: &str, value: u32| {
            format!("<config:config-item config:name=\"{key}\" config:type=\"{kind}\">{value}</config:config-item>")
        };
        let _ = write!(
            xml,
            "<config:config-item-map-entry config:name=\"{}\">{}{}{}{}{}{}</config:config-item-map-entry>",
            XmlSanitizer::escape(name),
            item("HorizontalSplitMode", "short", if cols > 0 { 2 } else { 0 }),
            item("VerticalSplitMode", "short", if rows > 0 { 2 } else { 0 }),
            item("HorizontalSplitPosition", "int", cols),
            item("VerticalSplitPosition", "int", rows),
            item("PositionRight", "int", cols),
            item("PositionBottom", "int", rows),
        );
    }
    xml.push_str("</config:config-item-map-named></config:config-item-map-entry></config:config-item-map-indexed></config:config-item-set></office:settings></office:document-settings>");
    Some(xml)
}

/// The frozen panes of `settings.xml` by sheet name.
pub(crate) fn read_freezes(settings: &str) -> HashMap<String, Freeze> {
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
                    let count = |mode: &str, position: &str| {
                        (values.get(mode) == Some(&FIXED_SPLIT))
                            .then(|| values.get(position).copied())
                            .flatten()
                            .and_then(|n| u32::try_from(n).ok())
                            .unwrap_or(0)
                    };
                    let rows = count("VerticalSplitMode", "VerticalSplitPosition");
                    let cols = count("HorizontalSplitMode", "HorizontalSplitPosition");
                    if let Some(freeze) = Freeze::new(rows, cols) {
                        out.insert(name, freeze);
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
        let mut writer = GroupWriter::new("table:table-row-group", &levels, &[(3, 4)]);
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
                hidden: false
            })
        );
        assert_eq!(
            read.get("ta2"),
            Some(&TableProps {
                color: None,
                hidden: true
            })
        );
        assert_eq!(
            read.get("ta3"),
            Some(&TableProps {
                color: Some(0x3366CC),
                hidden: true
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
        let read = read_freezes(&xml);
        assert_eq!(read.get("A & B"), Freeze::new(2, 1).as_ref());
        assert!(!read.contains_key("Plain"));
        assert!(settings_xml(&["A"], &[SheetStyling::default()]).is_none());
        let moving = xml.replace(">2</config:config-item>", ">1</config:config-item>");
        assert!(read_freezes(&moving).is_empty());
        let huge = xml.replace("\"int\">2<", "\"int\">5000<");
        assert!(read_freezes(&huge).is_empty());
    }
}

//! Tables in ODS (PD-224): `table:database-ranges` with one `table:database-range` per table, a child of
//! `office:spreadsheet` after the named expressions (OASIS ODF 1.3 9.4.15, 9.4.16, and the schema's
//! `table-database-range-attlist`). The model is `sheet_table`.
//!
//! A database range has a name, `table:target-range-address`, `table:contains-header` (default true)
//! and `table:display-filter-buttons` (default false). ODF has no totals row, table style or banding.
//! The totals row is `calcext:contains-footer`, the attribute the office suite writes and reads for it
//! (it writes it for a table it opened from XLSX); the style and the banding go out as the foreign
//! attributes `eo:table-style` and `eo:table-flags` (a list of `first-column`, `last-column`,
//! `row-stripes`, `column-stripes`), which a file by another program reads without. Column names are the header cells' text, so the reader leaves them as
//! `ColumnN` until the file is written. The conditions of a table's filter are the `table:filter` child
//! of its range (`ods_filter`).
//!
//! The range the office suite keeps for a sheet's own auto filter (`__Anonymous_Sheet_DB__N`, filter
//! buttons on) is the sheet's filter, not a table: it is read into `SheetStyling::filter` and written
//! the same way with the conditions as its `table:filter` child (PD-226). The ranges that mark a
//! selection (`table:is-selection`) are skipped. The rows a filter hides are the rows' `table:visibility`
//! `filter`, which `filtered_rows` carries from the grid walk and this reader hands to the filter whose
//! data rows they are in; the rest are rows a user hid.

use std::fmt::Write;

use crate::rtf_xml_helpers::{extract_attr_from_fragment, extract_balanced_elements};
use crate::sheet_filter::{FilterColumn, SheetFilter, MAX_FILTER_ROWS};
use crate::sheet_styling::ods_filter;
use crate::sheet_styling::ods_names::{quote, split_address};
use crate::sheet_styling::xlsx_layout::{add_run, intersect_runs, subtract_runs};
use crate::sheet_table::{unique_table_names, SheetTable, TableRegistry};
use crate::{CellBox, EasyOfficeError, SheetStyling, XmlSanitizer};

const ANONYMOUS_PREFIX: &str = "__Anonymous_";
const SHEET_FILTER_PREFIX: &str = "__Anonymous_Sheet_DB__";
const FLAG_TOKENS: [&str; 4] = [
    "first-column",
    "last-column",
    "row-stripes",
    "column-stripes",
];

fn flags_of(table: &SheetTable) -> String {
    let on = [
        table.first_column,
        table.last_column,
        table.row_stripes,
        table.column_stripes,
    ];
    FLAG_TOKENS
        .iter()
        .zip(on)
        .filter_map(|(token, on)| on.then_some(*token))
        .collect::<Vec<_>>()
        .join(" ")
}

fn apply_flags(table: &mut SheetTable, text: &str) {
    let has = |token: &str| text.split_whitespace().any(|t| t == token);
    table.first_column = has(FLAG_TOKENS[0]);
    table.last_column = has(FLAG_TOKENS[1]);
    table.row_stripes = has(FLAG_TOKENS[2]);
    table.column_stripes = has(FLAG_TOKENS[3]);
}

/// `Sheet1.A1:Sheet1.C5` for a table of `sheet`.
fn address(sheet: &str, range: &CellBox) -> String {
    let (head, tail) = range
        .to_a1()
        .split_once(':')
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .unwrap_or_else(|| (range.to_a1(), range.to_a1()));
    let sheet = quote(sheet);
    format!("{sheet}.{head}:{sheet}.{tail}")
}

/// The sheet and the rectangle of a `table:target-range-address`; `None` for one that spans sheets or is
/// not a rectangle.
fn parse_address(text: &str) -> Option<(String, CellBox)> {
    let (from, to) = text.split_once(':')?;
    let (sheet, first) = split_address(from)?;
    let (other, last) = split_address(to)?;
    let sheet = sheet?;
    if other.is_some_and(|o| !o.eq_ignore_ascii_case(&sheet)) {
        return None;
    }
    let range = CellBox::parse(&format!("{first}:{last}"))?;
    Some((sheet, range))
}

/// `<table:database-ranges>` for the tables of every sheet, empty when there are none. Names are made
/// unique among themselves and the defined names first.
pub(crate) fn database_ranges_xml(stylings: &[SheetStyling], sheets: &[&str]) -> String {
    if stylings
        .iter()
        .all(|st| st.tables.is_empty() && st.filter.is_none())
    {
        return String::new();
    }
    let taken = stylings
        .first()
        .into_iter()
        .flat_map(|st| st.defined_names.iter().map(|n| n.name.as_str()));
    let named = unique_table_names(stylings.iter().map(|st| &st.tables[..]), taken);
    let mut out = String::from("<table:database-ranges>");
    for (i, (tables, sheet)) in named.iter().zip(sheets).enumerate() {
        if let Some(filter) = stylings.get(i).and_then(|st| st.filter.as_ref()) {
            let _ = write!(
                out,
                "<table:database-range table:name=\"{SHEET_FILTER_PREFIX}{i}\" table:target-range-address=\"{}\" table:display-filter-buttons=\"true\">{}</table:database-range>",
                XmlSanitizer::escape(&address(sheet, &filter.range)),
                ods_filter::filter_xml(&filter.range, &filter.columns)
            );
        }
        for table in tables {
            let _ = write!(
                out,
                "<table:database-range table:name=\"{}\" table:target-range-address=\"{}\" table:contains-header=\"{}\" table:display-filter-buttons=\"{}\"",
                XmlSanitizer::escape(&table.name),
                XmlSanitizer::escape(&address(sheet, &table.range)),
                table.header,
                table.filter.is_some()
            );
            if table.totals {
                out.push_str(" calcext:contains-footer=\"true\"");
            }
            if let Some(style) = &table.style {
                let _ = write!(out, " eo:table-style=\"{}\"", XmlSanitizer::escape(style));
            }
            let flags = flags_of(table);
            if !flags.is_empty() {
                let _ = write!(out, " eo:table-flags=\"{flags}\"");
            }
            let conditions = table
                .filter
                .as_deref()
                .map(|columns| ods_filter::filter_xml(&table.filter_range(), columns))
                .unwrap_or_default();
            if conditions.is_empty() {
                out.push_str("/>");
            } else {
                let _ = write!(out, ">{conditions}</table:database-range>");
            }
        }
    }
    out.push_str("</table:database-ranges>");
    out
}

/// Reads the database ranges of `content` into the styling of the sheet each one is on. A workbook
/// that lists more ranges than the budget is refused.
pub(crate) fn read_tables(
    content: &str,
    stylings: &mut [(String, SheetStyling)],
) -> Result<(), EasyOfficeError> {
    if !content.contains("table:database-range") {
        stylings
            .iter_mut()
            .for_each(|(_, st)| assign_filtered_rows(st));
        return Ok(());
    }
    let taken: Vec<String> = stylings
        .first()
        .into_iter()
        .flat_map(|(_, st)| st.defined_names.iter().map(|n| n.name.clone()))
        .collect();
    let mut registry = TableRegistry::new(taken.iter().map(String::as_str));
    let mut lost = 0u32;
    for (_, block) in extract_balanced_elements(content, "table:database-ranges") {
        for (attrs, inner) in extract_balanced_elements(block, "table:database-range") {
            let attr =
                |name: &str| extract_attr_from_fragment(attrs, name).map(XmlSanitizer::unescape);
            let name = attr("table:name").unwrap_or_default();
            if attr("table:is-selection").as_deref() == Some("true") {
                continue;
            }
            let found = attr("table:target-range-address").and_then(|a| parse_address(&a));
            if name.starts_with(ANONYMOUS_PREFIX) {
                if name.starts_with(SHEET_FILTER_PREFIX)
                    && attr("table:display-filter-buttons").as_deref() == Some("true")
                    && attr("table:contains-header").as_deref() != Some("false")
                {
                    read_sheet_filter(found, inner, stylings);
                }
                continue;
            }
            let styling = found
                .as_ref()
                .and_then(|(sheet, _)| stylings.iter_mut().find(|(n, _)| n == sheet));
            let (Some((_, range)), Some((_, styling))) = (found, styling) else {
                registry.note_turned_away()?;
                lost += 1;
                continue;
            };
            let mut table = SheetTable::new(name, range);
            table.header = attr("table:contains-header").as_deref() != Some("false");
            table.totals = attr("calcext:contains-footer").as_deref() == Some("true");
            table.style = attr("eo:table-style");
            table.row_stripes = false;
            if let Some(flags) = attr("eo:table-flags") {
                apply_flags(&mut table, &flags);
            }
            if table.header && attr("table:display-filter-buttons").as_deref() == Some("true") {
                table.filter = Some(filter_columns(inner, &table.filter_range()));
            }
            if !registry.admit(&mut styling.tables, table)? {
                styling.tables_rejected = styling.tables_rejected.saturating_add(1);
            }
        }
    }
    if let Some((_, first)) = stylings.first_mut() {
        first.tables_rejected = first.tables_rejected.saturating_add(lost);
    }
    for (_, styling) in stylings.iter_mut() {
        assign_filtered_rows(styling);
    }
    Ok(())
}

/// Whether the sheet's filter is within the bounds of the model and of the ODS grid (`limits` is the last
/// row and column).
pub(crate) fn filter_within_bounds(styling: &SheetStyling, limits: (u32, u32)) -> bool {
    styling
        .filter
        .as_ref()
        .is_none_or(|f| f.within_bounds() && f.range.row1 <= limits.0 && f.range.col1 <= limits.1)
}

/// The conditions in the `table:filter` of a database range whose content is `inner`.
fn filter_columns(inner: &str, range: &CellBox) -> Vec<FilterColumn> {
    extract_balanced_elements(inner, "table:filter")
        .into_iter()
        .next()
        .map(|(_, body)| ods_filter::parse_filter(body, range))
        .unwrap_or_default()
}

/// The filter of the anonymous database range of a sheet: the sheet's filter, when it has none yet, has a
/// header row and a data row, is within the bounds of the model and shares no cell with a table.
fn read_sheet_filter(
    found: Option<(String, CellBox)>,
    inner: &str,
    stylings: &mut [(String, SheetStyling)],
) {
    let Some((sheet, range)) = found else {
        return;
    };
    let Some((_, styling)) = stylings.iter_mut().find(|(n, _)| *n == sheet) else {
        return;
    };
    if styling.filter.is_some() || styling.tables.iter().any(|t| t.overlaps(&range)) {
        return;
    }
    let mut filter = SheetFilter::new(range);
    filter.columns = filter_columns(inner, &range);
    if filter.within_bounds() {
        styling.filter = Some(filter);
    }
}

/// Hands the rows the grid marked as hidden by a filter to the filter whose data rows they are in (the
/// sheet's, then the tables'); every other marked row is a row a user hid.
fn assign_filtered_rows(styling: &mut SheetStyling) {
    let mut rest = std::mem::take(&mut styling.filtered_rows);
    if rest.is_empty() {
        return;
    }
    let mut claim = |first: u32, last: u32| {
        let mine = intersect_runs(&rest, &[(first, last)]);
        rest = subtract_runs(&rest, &mine);
        mine
    };
    if let Some(filter) = styling.filter.as_mut() {
        filter.hidden = claim(filter.range.row0 + 1, filter.range.row1);
    }
    for table in &mut styling.tables {
        let range = table.filter_range();
        if table.filter.is_some() && range.row1 - range.row0 < MAX_FILTER_ROWS {
            table.filter_hidden = claim(range.row0 + 1, range.row1);
        }
    }
    for &(first, last) in &rest {
        add_run(&mut styling.hidden_rows, first, last);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(name: &str) -> (String, SheetStyling) {
        (name.to_string(), SheetStyling::default())
    }

    fn table(name: &str, range: &str) -> SheetTable {
        SheetTable::new(name, CellBox::parse(range).unwrap())
    }

    #[test]
    fn tables_round_trip_through_database_ranges() {
        let mut t = table("Sales", "B2:D9");
        t.totals = true;
        t.style = Some("TableStyleMedium2".into());
        t.column_stripes = true;
        t.filter = Some(Vec::new());
        let mut plain = table("Plain", "A1:A3");
        plain.header = false;
        plain.row_stripes = false;
        let with = |tables| SheetStyling {
            tables,
            ..SheetStyling::default()
        };
        let (first, second) = (with(vec![t.clone()]), with(vec![plain.clone()]));
        let xml = database_ranges_xml(&[first, second], &["Data", "My Sheet"]);
        assert!(
            xml.contains("table:target-range-address=\"Data.B2:Data.D9\""),
            "{xml}"
        );
        assert!(
            xml.contains("&apos;My Sheet&apos;.A1:&apos;My Sheet&apos;.A3"),
            "{xml}"
        );
        XmlSanitizer::validate_safe_xml(&format!("<r xmlns:table=\"t\" xmlns:eo=\"e\">{xml}</r>"))
            .unwrap();
        let mut back = vec![sheet("Data"), sheet("My Sheet")];
        read_tables(&xml, &mut back).unwrap();
        // The column names are the header cells' text, so only the other fields travel.
        let mut expected = t;
        expected.columns = back[0].1.tables[0].columns.clone();
        assert_eq!(back[0].1.tables, vec![expected]);
        assert_eq!(back[1].1.tables, vec![plain]);
    }

    #[test]
    fn anonymous_selection_foreign_and_broken_ranges_are_not_tables() {
        let xml = concat!(
            "<table:database-ranges>",
            "<table:database-range table:name=\"__Anonymous_Sheet_DB__0\" table:target-range-address=\"Data.A1:Data.B5\"/>",
            "<table:database-range table:name=\"Sel\" table:is-selection=\"true\" table:target-range-address=\"Data.A1:Data.B5\"/>",
            "<table:database-range table:name=\"Other\" table:target-range-address=\"Missing.A1:Missing.B5\"/>",
            "<table:database-range table:name=\"Cross\" table:target-range-address=\"Data.A1:Other.B5\"/>",
            "<table:database-range table:name=\"Ok\" table:target-range-address=\"$Data.$A$1:.$B$5\" table:display-filter-buttons=\"true\"/>",
            "</table:database-ranges>"
        );
        let mut sheets = vec![sheet("Data")];
        read_tables(xml, &mut sheets).unwrap();
        let tables = &sheets[0].1.tables;
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].name, "Ok");
        assert!(tables[0].header && tables[0].filter.is_some());
        assert_eq!(tables[0].range.to_a1(), "A1:B5");
    }

    #[test]
    fn the_ranges_an_office_suite_wrote_for_its_own_tables_are_read() {
        // As the office suite writes a table it opened from an XLSX file: the footer and the filter buttons.
        let xml = concat!(
            "<table:database-ranges>",
            "<table:database-range table:name=\"Sales\" table:target-range-address=\"Data.A1:Data.C4\" ",
            "table:display-filter-buttons=\"true\" calcext:contains-footer=\"true\"/>",
            "<table:database-range table:name=\"Bare\" table:contains-header=\"false\" ",
            "table:target-range-address=\"Data.E1:Data.E2\"/>",
            "</table:database-ranges>"
        );
        let mut sheets = vec![sheet("Data")];
        read_tables(xml, &mut sheets).unwrap();
        let tables = &sheets[0].1.tables;
        assert!(tables[0].header && tables[0].totals && tables[0].filter.is_some());
        assert!(!tables[1].header && !tables[1].totals && tables[1].filter.is_none());
    }

    #[test]
    fn a_flood_of_database_ranges_refuses_the_file() {
        let mut xml = String::from("<table:database-ranges>");
        for n in 0..crate::MAX_TABLES + 1 {
            let _ = write!(
                xml,
                "<table:database-range table:name=\"T{n}\" table:target-range-address=\"Data.A{}:Data.A{}\"/>",
                2 * n + 1,
                2 * n + 2
            );
        }
        xml.push_str("</table:database-ranges>");
        assert!(matches!(
            read_tables(&xml, &mut [sheet("Data")]),
            Err(EasyOfficeError::ResourceBudgetExceeded(_))
        ));
    }

    #[test]
    fn hostile_names_and_overlaps_are_turned_away_or_escaped() {
        let xml = concat!(
            "<table:database-ranges>",
            "<table:database-range table:name=\"a&quot;&lt;&gt;&amp;\" table:target-range-address=\"Data.A10:Data.B15\"/>",
            "<table:database-range table:name=\"Over\" table:target-range-address=\"Data.B3:Data.C7\"/>",
            "<table:database-range table:name=\"Over2\" table:target-range-address=\"Data.C6:Data.D9\"/>",
            "<table:database-range table:name=\"Same\" table:target-range-address=\"Data.E1:Data.F5\"/>",
            "<table:database-range table:name=\"same\" table:target-range-address=\"Data.H1:Data.I5\"/>",
            "</table:database-ranges>"
        );
        let mut sheets = vec![sheet("Data")];
        read_tables(xml, &mut sheets).unwrap();
        let names: Vec<&str> = sheets[0].1.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Over", "Same", "same_2"]);
        assert_eq!(sheets[0].1.tables_rejected, 2);
    }
}

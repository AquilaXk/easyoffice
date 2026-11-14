//! Tables in SpreadsheetML (PD-224): the `tableParts` element of a worksheet part, the sheet's
//! relationships of type `table` and the table parts `xl/tables/tableN.xml` (ECMA-376 18.5.1.2
//! `table`, 18.5.1.3 `tableColumn`, 18.5.1.5 `tableStyleInfo`, 18.3.1.88 `tableParts`). The model is
//! `sheet_table`; its header lists the reference implementations.
//!
//! The reader takes the tables a sheet lists in order, resolves each relationship against the sheet's
//! directory and reads the part with the event stream. A part that cannot be read, a table that
//! breaks a bound of the model or shares a cell with an earlier table is turned away and counted; a
//! workbook that lists more tables or columns than the budgets is refused as a whole. The writer
//! gives every table an id that is unique in the workbook, names its columns from the header cells
//! (Excel refuses a table whose column names differ from its header cells) and writes the header
//! cells as strings.

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use crate::sheet_filter::SheetFilter;
use crate::sheet_styling::xlsx_filter::{autofilter_xml, FilterScan};
use crate::sheet_styling::xlsx_sheet::decode_xlsx_escapes;
use crate::sheet_table::{SheetTable, TableRegistry, MAX_TABLES, MAX_TABLE_TEXT_CHARS};
use crate::xml_stream::{unescape_cow, Attrs, XmlEvent, XmlReader};
use crate::{CellBox, EasyOfficeError, XmlSanitizer};

const TABLE_RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/table";
const REL_NAMESPACE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const TABLE_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml";
/// Events of one table part: a table of every column the grid has, with room for the rest.
const MAX_TABLE_PART_EVENTS: usize = 262_144;

// ---- reading -------------------------------------------------------------------------------------------

fn flag(attrs: &Attrs<'_>, name: &str) -> bool {
    matches!(attrs.get(name).as_deref(), Some("1" | "true"))
}

/// `0` or `1` (the counts of header and totals rows); `default` when the attribute is absent, `None`
/// for any other value.
fn count_flag(attrs: &Attrs<'_>, name: &str, default: bool) -> Option<bool> {
    match attrs.get(name).as_deref() {
        None => Some(default),
        Some("0") => Some(false),
        Some("1") => Some(true),
        Some(_) => None,
    }
}

/// The table of one part, or `None` when the part is not a table this model can hold.
fn parse_table(xml: &str) -> Option<SheetTable> {
    let mut reader = XmlReader::with_budget(xml, MAX_TABLE_PART_EVENTS);
    let mut table: Option<SheetTable> = None;
    let mut filter = FilterScan::new();
    let mut has_filter = false;
    let mut columns: Vec<String> = Vec::new();
    let mut max_columns = 0usize;
    while let Ok(Some(event)) = reader.next_event() {
        filter.feed(&event);
        let XmlEvent::Start { name, attrs } = event else {
            continue;
        };
        match name {
            "table" if table.is_none() => {
                let range = attrs
                    .get("ref")
                    .filter(|r| r.contains(':'))
                    .and_then(|r| CellBox::parse(&r))?;
                let name = attrs.get("displayName").or_else(|| attrs.get("name"))?;
                let mut t = SheetTable::new(name.into_owned(), range);
                t.header = count_flag(&attrs, "headerRowCount", true)?;
                t.totals = count_flag(&attrs, "totalsRowCount", false)?;
                max_columns = t.width() as usize;
                t.row_stripes = false;
                table = Some(t);
            }
            "autoFilter" => has_filter = true,
            "tableColumn" => {
                if columns.len() >= max_columns {
                    return None;
                }
                columns.push(decode_xlsx_escapes(attrs.get("name")?));
            }
            "tableStyleInfo" => {
                let t = table.as_mut()?;
                t.style = attrs
                    .get("name")
                    .map(|n| n.into_owned())
                    .filter(|n| !n.is_empty());
                t.first_column = flag(&attrs, "showFirstColumn");
                t.last_column = flag(&attrs, "showLastColumn");
                t.row_stripes = flag(&attrs, "showRowStripes");
                t.column_stripes = flag(&attrs, "showColumnStripes");
            }
            _ => {}
        }
    }
    let mut table = table?;
    if columns.len() != table.width() as usize {
        return None;
    }
    table.columns = columns;
    if has_filter && table.header {
        let conditions = filter.finish_unbounded().map(|f| f.columns);
        table.filter = Some(conditions.unwrap_or_default());
    }
    Some(table)
}

/// The directory of a part and a relationship target resolved against it: an absolute target
/// (`/xl/tables/table1.xml`) is taken from the package root, `..` and `.` segments are folded.
fn resolve_target(dir: &str, target: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    let joined;
    let path = if let Some(absolute) = target.strip_prefix('/') {
        absolute
    } else {
        joined = format!("{dir}/{target}");
        &joined
    };
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

/// The ids of the `tablePart` elements of a worksheet part; an error when it lists more than the
/// workbook's budget.
fn table_part_ids(sheet_xml: &str) -> Result<Vec<String>, EasyOfficeError> {
    let Some(start) = sheet_xml.rfind("<tableParts") else {
        return Ok(Vec::new());
    };
    let rest = &sheet_xml[start..];
    let block = rest.find("</tableParts>").map_or(rest, |end| &rest[..end]);
    let mut reader = XmlReader::unbounded(block);
    let mut ids = Vec::new();
    while let Ok(Some(event)) = reader.next_event() {
        let XmlEvent::Start {
            name: "tablePart",
            attrs,
        } = event
        else {
            continue;
        };
        if ids.len() >= MAX_TABLES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "시트가 나열한 표 수가 허용 상한({MAX_TABLES})을 초과했습니다."
            )));
        }
        // A part without an id is a dangling element: a placeholder keeps it counted as turned away.
        ids.push(
            attrs
                .get("r:id")
                .map(|v| v.into_owned())
                .unwrap_or_default(),
        );
    }
    Ok(ids)
}

/// The targets of the `table` relationships of the sheet part `part_path` that are in `wanted`.
fn table_targets(
    entries: &HashMap<&str, &[u8]>,
    part_path: &str,
    wanted: &HashSet<&str>,
) -> HashMap<String, String> {
    let (dir, file) = part_path.rsplit_once('/').unwrap_or(("", part_path));
    let mut out = HashMap::new();
    let Some(rels) = entries
        .get(format!("{dir}/_rels/{file}.rels").as_str())
        .and_then(|data| std::str::from_utf8(data).ok())
    else {
        return out;
    };
    let mut reader = XmlReader::unbounded(rels);
    while let Ok(Some(event)) = reader.next_event() {
        let XmlEvent::Start {
            name: "Relationship",
            attrs,
        } = event
        else {
            continue;
        };
        let (mut is_table, mut id, mut target) = (false, None, None);
        for (name, value) in attrs.iter_raw() {
            match name {
                "Type" => is_table = value == TABLE_RELATIONSHIP,
                "Id" => id = Some(unescape_cow(value)),
                "Target" => target = Some(unescape_cow(value)),
                _ => {}
            }
        }
        if let (true, Some(id), Some(target)) = (is_table, id, target) {
            if wanted.contains(id.as_ref()) {
                out.insert(id.into_owned(), target.into_owned());
            }
        }
    }
    out
}

/// Reads the tables of the worksheet part `part_path` into `tables`; returns how many it turned away.
pub(crate) fn read_sheet_tables(
    sheet_xml: &str,
    part_path: &str,
    entries: &HashMap<&str, &[u8]>,
    registry: &mut TableRegistry,
    tables: &mut Vec<SheetTable>,
) -> Result<u32, EasyOfficeError> {
    let ids = table_part_ids(sheet_xml)?;
    if ids.is_empty() {
        return Ok(0);
    }
    let wanted: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let targets = table_targets(entries, part_path, &wanted);
    let dir = part_path.rsplit_once('/').map_or("", |(dir, _)| dir);
    let mut rejected = 0u32;
    for id in &ids {
        let table = targets
            .get(id)
            .and_then(|target| resolve_target(dir, target))
            .and_then(|path| entries.get(path.as_str()).copied())
            .and_then(|data| std::str::from_utf8(data).ok())
            .and_then(parse_table);
        let admitted = match table {
            Some(table) => registry.admit(tables, table)?,
            None => {
                registry.note_turned_away()?;
                false
            }
        };
        rejected += u32::from(!admitted);
    }
    Ok(rejected)
}

// ---- writing -------------------------------------------------------------------------------------------

/// The header cells of the tables of one sheet, to write them as strings.
#[derive(Debug, Default)]
pub(crate) struct HeaderCells {
    /// `(row, first column, last column)`, sorted.
    spans: Vec<(u32, u32, u32)>,
}

impl HeaderCells {
    pub(crate) fn new(tables: &[SheetTable]) -> Self {
        let mut spans: Vec<_> = tables
            .iter()
            .filter(|t| t.header)
            .map(|t| (t.range.row0, t.range.col0, t.range.col1))
            .collect();
        spans.sort_unstable();
        Self { spans }
    }

    pub(crate) fn contains(&self, row: u32, col: u32) -> bool {
        let from = self.spans.partition_point(|s| s.0 < row);
        self.spans[from..]
            .iter()
            .take_while(|s| s.0 == row)
            .any(|&(_, c0, c1)| (c0..=c1).contains(&col))
    }
}

/// `text` as a table column name: a line break, a carriage return and a tab are written as `_xHHHH_`.
fn escape_column_name(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\n' | '\r' | '\t' => {
                let _ = write!(out, "_x{:04x}_", c as u32);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Whether `text` can be a column name as it stands: not empty, not a formula, not too long.
fn usable_header(text: &str) -> bool {
    !text.is_empty() && !text.starts_with('=') && text.chars().count() <= MAX_TABLE_TEXT_CHARS
}

/// The column names of `table`: the header cells' text when it is usable and not taken, else the name the
/// model holds, else `ColumnN`; a taken name gets a number appended as the spreadsheet program does. With
/// `header_text` the cells whose text differs are listed in the second result with the name they get.
fn resolve_columns<'a>(
    table: &SheetTable,
    header_text: impl Fn(u32) -> Option<&'a str>,
) -> (Vec<String>, Vec<(u32, String)>) {
    let mut taken = HashSet::new();
    let mut names = Vec::with_capacity(table.columns.len());
    let mut fixes = Vec::new();
    for (i, model) in table.columns.iter().enumerate() {
        let col = table.range.col0 + i as u32;
        let text = header_text(col);
        let base = match text {
            Some(t) if table.header && usable_header(t) => t,
            _ if !model.is_empty() => model,
            _ => "",
        };
        let base = if base.is_empty() {
            format!("Column{}", i + 1)
        } else {
            base.to_string()
        };
        let mut name = base.clone();
        let mut n = 1u32;
        while !taken.insert(crate::sheet_names::fold_name(&name)) {
            n += 1;
            let suffix = n.to_string();
            let keep = MAX_TABLE_TEXT_CHARS - suffix.len();
            name = base.chars().take(keep).collect::<String>() + &suffix;
        }
        if table.header && text != Some(name.as_str()) {
            fixes.push((col, name.clone()));
        }
        names.push(name);
    }
    (names, fixes)
}

/// The XML of one table part.
fn table_xml(id: usize, table: &SheetTable, columns: &[String]) -> String {
    let name = XmlSanitizer::escape(&table.name);
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><table xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" id=\"{id}\" name=\"{name}\" displayName=\"{name}\" ref=\"{}\"",
        table.range.to_a1()
    );
    if !table.header {
        out.push_str(" headerRowCount=\"0\"");
    }
    out.push_str(if table.totals {
        " totalsRowCount=\"1\">"
    } else {
        " totalsRowShown=\"0\">"
    });
    if let Some(conditions) = &table.filter {
        let mut filter = SheetFilter::new(table.filter_range());
        filter.columns.clone_from(conditions);
        out.push_str(&autofilter_xml(&filter));
    }
    let _ = write!(out, "<tableColumns count=\"{}\">", columns.len());
    for (i, column) in columns.iter().enumerate() {
        let _ = write!(
            out,
            "<tableColumn id=\"{}\" name=\"{}\"/>",
            i + 1,
            XmlSanitizer::escape(&escape_column_name(column))
        );
    }
    out.push_str("</tableColumns><tableStyleInfo");
    if let Some(style) = &table.style {
        let _ = write!(out, " name=\"{}\"", XmlSanitizer::escape(style));
    }
    let bit = |on: bool| u8::from(on);
    let _ = write!(
        out,
        " showFirstColumn=\"{}\" showLastColumn=\"{}\" showRowStripes=\"{}\" showColumnStripes=\"{}\"/></table>",
        bit(table.first_column),
        bit(table.last_column),
        bit(table.row_stripes),
        bit(table.column_stripes)
    );
    out
}

/// What a sheet's tables add to the package.
#[derive(Debug, Default)]
pub(crate) struct SheetTables {
    /// The sheet's cells with the header cells that had to change, when any did.
    pub(crate) cells: Option<Vec<(u32, u32, String)>>,
    /// The `tableParts` element for the end of the worksheet part.
    pub(crate) parts_xml: String,
    /// `Relationship` elements for the sheet's relationships part.
    pub(crate) rels: String,
}

/// Writes the tables of a workbook: the parts and the content types are collected here, the sheets'
/// own pieces come from [`TableWriter::add_sheet`].
#[derive(Debug, Default)]
pub(crate) struct TableWriter {
    count: usize,
    pub(crate) parts: Vec<(String, Vec<u8>)>,
    pub(crate) content_types: String,
}

impl TableWriter {
    pub(crate) fn add_sheet(
        &mut self,
        tables: &[SheetTable],
        cells: &[(u32, u32, String)],
    ) -> SheetTables {
        if tables.is_empty() {
            return SheetTables::default();
        }
        let mut by_row: HashMap<u32, Vec<usize>> = HashMap::new();
        for (i, t) in tables.iter().enumerate().filter(|(_, t)| t.header) {
            by_row.entry(t.range.row0).or_default().push(i);
        }
        let mut texts: HashMap<(usize, u32), &str> = HashMap::new();
        for (row, col, raw) in cells {
            for &i in by_row.get(row).into_iter().flatten() {
                if (tables[i].range.col0..=tables[i].range.col1).contains(col) {
                    texts.insert((i, *col), raw);
                }
            }
        }
        let mut fixes: HashMap<(u32, u32), String> = HashMap::new();
        let mut out = SheetTables::default();
        let _ = write!(
            out.parts_xml,
            "<tableParts xmlns:r=\"{REL_NAMESPACE}\" count=\"{}\">",
            tables.len()
        );
        for (i, table) in tables.iter().enumerate() {
            let (names, header_fixes) = resolve_columns(table, |col| texts.get(&(i, col)).copied());
            fixes.extend(
                header_fixes
                    .into_iter()
                    .map(|(col, name)| ((table.range.row0, col), name)),
            );
            self.count += 1;
            let n = self.count;
            let _ = write!(
                self.content_types,
                "<Override PartName=\"/xl/tables/table{n}.xml\" ContentType=\"{TABLE_CONTENT_TYPE}\"/>"
            );
            self.parts.push((
                format!("xl/tables/table{n}.xml"),
                table_xml(n, table, &names).into_bytes(),
            ));
            let _ = write!(out.parts_xml, "<tablePart r:id=\"rIdT{n}\"/>");
            let _ = write!(
                out.rels,
                "<Relationship Id=\"rIdT{n}\" Type=\"{TABLE_RELATIONSHIP}\" Target=\"../tables/table{n}.xml\"/>"
            );
        }
        out.parts_xml.push_str("</tableParts>");
        if !fixes.is_empty() {
            out.cells = Some(patched_cells(cells, fixes));
        }
        out
    }
}

/// `cells` with the text of the cells in `fixes` replaced, and the ones that were missing added.
fn patched_cells(
    cells: &[(u32, u32, String)],
    mut fixes: HashMap<(u32, u32), String>,
) -> Vec<(u32, u32, String)> {
    let mut out: Vec<_> = cells
        .iter()
        .map(|(row, col, raw)| match fixes.remove(&(*row, *col)) {
            Some(text) => (*row, *col, text),
            None => (*row, *col, raw.clone()),
        })
        .collect();
    out.extend(fixes.into_iter().map(|((row, col), text)| (row, col, text)));
    out
}

/// The relationships part of a sheet: the hyperlink part `links` (a whole document, when the sheet has
/// external links) with the table relationships `tables` added, or a new part for the tables alone.
pub(crate) fn merge_relationships(links: Option<String>, tables: &str) -> Option<String> {
    if tables.is_empty() {
        return links;
    }
    Some(match links {
        Some(doc) => doc.replace("</Relationships>", &format!("{tables}</Relationships>")),
        None => format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{tables}</Relationships>"
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(name: &str, range: &str) -> SheetTable {
        SheetTable::new(name, CellBox::parse(range).unwrap())
    }

    #[test]
    fn a_written_table_reads_back_with_every_field() {
        let mut t = table("Sales2024", "B2:D9");
        t.totals = true;
        t.columns = vec!["Region".into(), "Q1".into(), "a\nb".into()];
        t.style = Some("TableStyleMedium9".into());
        t.first_column = true;
        t.column_stripes = true;
        t.filter = Some(Vec::new());
        let xml = table_xml(7, &t, &t.columns);
        assert!(xml.contains("id=\"7\"") && xml.contains("totalsRowCount=\"1\""));
        assert!(xml.contains("<autoFilter ref=\"B2:D8\"/>"), "{xml}");
        assert!(xml.contains("name=\"a_x000a_b\""));
        XmlSanitizer::validate_safe_xml(&xml).unwrap();
        assert_eq!(parse_table(&xml), Some(t));
    }

    #[test]
    fn a_table_without_header_or_style_round_trips() {
        let mut t = table("T", "A1:B3");
        t.header = false;
        t.row_stripes = false;
        let xml = table_xml(1, &t, &t.columns);
        assert!(xml.contains("headerRowCount=\"0\"") && !xml.contains("<autoFilter"));
        assert_eq!(parse_table(&xml), Some(t));
    }

    #[test]
    fn hostile_names_are_escaped_and_survive() {
        let mut t = table("Tab1", "A1:B2");
        t.columns = vec!["\"<>&".into(), "x' y".into()];
        t.style = Some("\"<>&'".into());
        let xml = table_xml(1, &t, &t.columns);
        XmlSanitizer::validate_safe_xml(&xml).unwrap();
        assert_eq!(parse_table(&xml), Some(t));
    }

    #[test]
    fn the_reader_follows_the_schema_defaults_and_refuses_what_it_cannot_hold() {
        let plain = "<table id=\"1\" name=\"A\" displayName=\"A\" ref=\"A1:B3\"><tableColumns count=\"2\"><tableColumn id=\"1\" name=\"x\"/><tableColumn id=\"2\" name=\"y\"/></tableColumns></table>";
        let t = parse_table(plain).unwrap();
        assert!(t.header && !t.totals && t.style.is_none() && !t.row_stripes && t.filter.is_none());
        assert!(parse_table(&plain.replace("A1:B3", "A1:B")).is_none());
        assert!(parse_table(&plain.replace("A1:B3", "A1")).is_none());
        assert!(parse_table(&plain.replace("<tableColumn id=\"2\" name=\"y\"/>", "")).is_none());
        assert!(parse_table(&plain.replace("ref=", "headerRowCount=\"2\" ref=")).is_none());
        assert!(parse_table("<nothing/>").is_none());
    }

    #[test]
    fn the_filter_conditions_of_a_table_are_kept() {
        let xml = concat!(
            "<table id=\"1\" name=\"F\" displayName=\"F\" ref=\"B2:D400000\"><autoFilter ref=\"B2:D400000\">",
            "<filterColumn colId=\"1\"><filters blank=\"1\"><filter val=\"x\"/></filters></filterColumn></autoFilter>",
            "<tableColumns count=\"3\"><tableColumn id=\"1\" name=\"a\"/><tableColumn id=\"2\" name=\"b\"/>",
            "<tableColumn id=\"3\" name=\"c\"/></tableColumns></table>"
        );
        let t = parse_table(xml).unwrap();
        let conditions = t.filter.unwrap();
        assert_eq!(conditions.len(), 1);
        assert_eq!(conditions[0].col, 2);
    }

    #[test]
    fn targets_resolve_against_the_sheet_directory() {
        assert_eq!(
            resolve_target("xl/worksheets", "../tables/table1.xml").as_deref(),
            Some("xl/tables/table1.xml")
        );
        assert_eq!(
            resolve_target("xl/worksheets", "/xl/tables/table2.xml").as_deref(),
            Some("xl/tables/table2.xml")
        );
        assert_eq!(resolve_target("xl/worksheets", "../../../x.xml"), None);
    }

    #[test]
    fn columns_follow_the_header_cells_and_repair_what_cannot_be_a_name() {
        let t = table("T", "A1:E3");
        let headers = ["Name", "name", "", "=A1", "x"];
        let (names, fixes) = resolve_columns(&t, |c| headers.get(c as usize).copied());
        assert_eq!(names, ["Name", "name2", "Column3", "Column4", "x"]);
        let fixed: Vec<u32> = fixes.iter().map(|f| f.0).collect();
        assert_eq!(fixed, [1, 2, 3]);
        let mut headless = table("T", "A1:B3");
        headless.header = false;
        headless.columns = vec!["p".into(), "p".into()];
        let (names, fixes) = resolve_columns(&headless, |_| Some("ignored"));
        assert_eq!(names, ["p", "p2"]);
        assert!(fixes.is_empty());
    }

    #[test]
    fn header_cells_are_found_by_row_and_column() {
        let mut other = table("U", "F5:G8");
        other.header = false;
        let headers = HeaderCells::new(&[table("T", "B2:D9"), other]);
        assert!(headers.contains(1, 1) && headers.contains(1, 3));
        assert!(!headers.contains(1, 4) && !headers.contains(2, 1) && !headers.contains(4, 5));
    }

    #[test]
    fn the_writer_gives_unique_ids_and_patches_header_cells() {
        let mut writer = TableWriter::default();
        let cells = vec![(0, 0, "A".to_string()), (0, 1, "A".to_string())];
        let first = writer.add_sheet(&[table("T", "A1:B3")], &cells);
        let second = writer.add_sheet(&[table("U", "A1:B3")], &[]);
        assert_eq!(writer.parts.len(), 2);
        assert_eq!(writer.parts[1].0, "xl/tables/table2.xml");
        assert!(first.rels.contains("Target=\"../tables/table1.xml\""));
        assert!(second.parts_xml.contains("rIdT2"));
        let patched = first.cells.unwrap();
        assert!(patched.contains(&(0, 1, "A2".to_string())));
        let added = second.cells.unwrap();
        assert!(added.contains(&(0, 0, "Column1".to_string())) && added.len() == 2);
    }

    #[test]
    fn merged_relationships_keep_the_links_and_add_the_tables() {
        let links =
            "<Relationships xmlns=\"x\"><Relationship Id=\"rId1\"/></Relationships>".to_string();
        let merged = merge_relationships(Some(links), "<Relationship Id=\"rIdT1\"/>").unwrap();
        assert!(merged.contains("rId1") && merged.contains("rIdT1"));
        assert!(merge_relationships(None, "").is_none());
        assert!(merge_relationships(None, "<Relationship Id=\"a\"/>")
            .unwrap()
            .starts_with("<?xml"));
    }
}

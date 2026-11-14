//! Auto filter requests (PD-198): `evaluate_sheet` with `sessionAction:"filter"`. The command set
//! stays at 15.
//!
//! The page sends the filter (the JSON of `sheet_filter_json`) and, for every column the request
//! needs, the stored text of the rows below the header, with the number format code of the cell
//! ranges that have one: `data: [{col, values: [text...], fmt: [[first, last, code], ...]}]`
//! (`col` 1-based, `first`/`last` 0-based offsets into `values`). The engine judges the displayed
//! text, the same text a cell shows.
//!
//! A column may also carry the colours of its rows as runs, `fill` and `font`: `[[first, last,
//! "rrggbb"], ...]` (a row in no run has no colour of its own); the engine reads the numbers of
//! the stored texts itself and tells which cells show as dates from the format runs (PD-226).
//!
//! - `op:"apply"` -> `{hidden: [[first, last], ...], count}`: the rows (1-based) the conditions hide.
//! - `op:"values"`, `col`, `query?` -> `{items: [[text, count], ...], blank, more}`: the distinct
//!   texts of that column among the rows the other columns leave visible, and, when the column has
//!   dates, `dates: {years: [[year, count, [[month, count, [[day, count], ...]], ...]], ...], more}`;
//!   when it has colours, `colors: {fill: [["rrggbb", count], ...], font: [...], noFill, noFont}`.

use crate::sheet_filter_json::parse_filter;
use crate::sheet_layout_json::runs_json;
use crate::EasyOfficeIpcRouter;
use easyoffice_formats::{
    code_within_bounds, resolve_code, CellBox, EasyOfficeError, FilterColumn, FilterCriteria,
    SheetFilter, SheetStyling, SheetTable, StrictJsonValue, MAX_FILTER_ROWS, MAX_FILTER_VALUES,
};
use easyoffice_sheet::filter::{
    color_list, date_tree, hidden_bits, number_of, split_hidden_rows, tree_rows, value_list,
    CellAux, ColumnText, DateTree, RowBits, NO_COLOR,
};
use easyoffice_sheet::{code_shows_date, format_cell_value};
use std::fmt::Write;

type Entries = [(String, StrictJsonValue)];

/// Cell texts one request may carry over all columns.
const MAX_REQUEST_CELLS: usize = 2_000_000;
/// A longer stored text is text no number format changes.
const MAX_FORMATTED_BYTES: usize = 512;
/// Characters of a search text.
const MAX_QUERY_CHARS: usize = 255;

fn invalid(what: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(format!("필터 요청이 올바르지 않습니다: {what}"))
}

struct Column {
    col: u32,
    texts: Vec<String>,
    aux: Option<CellAux>,
}

/// Whether a column's criteria read the numbers of its cells.
fn reads_numbers(criteria: &FilterCriteria) -> bool {
    match criteria {
        FilterCriteria::Values { dates, .. } => !dates.is_empty(),
        FilterCriteria::Top { .. } | FilterCriteria::Average { .. } => true,
        FilterCriteria::Custom { .. } | FilterCriteria::Color { .. } => false,
    }
}

/// Applies the format runs of one data column to its texts; `numbers` and `dates` are filled for
/// the cells whose format shows a date when the column keeps them.
fn apply_formats(
    texts: &mut [String],
    runs: &StrictJsonValue,
    aux: &mut Option<CellAux>,
) -> Result<(), EasyOfficeError> {
    for run in runs.as_array().ok_or_else(|| invalid("fmt"))? {
        let parts = run
            .as_array()
            .filter(|p| p.len() == 3)
            .ok_or_else(|| invalid("fmt"))?;
        let offset = |i: usize| parts[i].as_u64().and_then(|n| usize::try_from(n).ok());
        let (Some(first), Some(last), Some(code)) = (offset(0), offset(1), parts[2].as_str())
        else {
            return Err(invalid("fmt"));
        };
        if first > last || last >= texts.len() {
            return Err(invalid("fmt"));
        }
        if !code_within_bounds(code) {
            continue;
        }
        for (i, text) in texts.iter_mut().enumerate().take(last + 1).skip(first) {
            if text.len() > MAX_FORMATTED_BYTES {
                continue;
            }
            if let Some(aux) = aux.as_mut() {
                if aux.number(i).is_some_and(|n| code_shows_date(code, n)) {
                    aux.dates.set(i);
                }
            }
            if let Some(shown) = format_cell_value(code, text) {
                *text = shown.text;
            }
        }
    }
    Ok(())
}

/// The colours of one data column from `[[first, last, "rrggbb"], ...]`, one slot per row.
fn parse_colors(
    runs: &StrictJsonValue,
    rows: usize,
    what: &str,
) -> Result<Vec<u32>, EasyOfficeError> {
    let mut out = vec![NO_COLOR; rows];
    for run in runs.as_array().ok_or_else(|| invalid(what))? {
        let parts = run
            .as_array()
            .filter(|p| p.len() == 3)
            .ok_or_else(|| invalid(what))?;
        let offset = |i: usize| parts[i].as_u64().and_then(|n| usize::try_from(n).ok());
        let rgb = parts[2]
            .as_str()
            .filter(|t| t.len() == 6)
            .and_then(|t| u32::from_str_radix(t, 16).ok());
        let (Some(first), Some(last), Some(rgb)) = (offset(0), offset(1), rgb) else {
            return Err(invalid(what));
        };
        if first > last || last >= rows {
            return Err(invalid(what));
        }
        out[first..=last].fill(rgb);
    }
    Ok(out)
}

/// Whether column `col` needs the facts beyond its text: a criterion of its own that reads them, or
/// it is the column a value list is asked for.
fn needs_aux(filter: &SheetFilter, col: u32, target: Option<u32>) -> bool {
    target == Some(col)
        || filter.column(col).is_some_and(|c| {
            reads_numbers(&c.criteria) || matches!(c.criteria, FilterCriteria::Color { .. })
        })
}

fn parse_data(
    entries: &Entries,
    filter: &SheetFilter,
    target: Option<u32>,
) -> Result<Vec<Column>, EasyOfficeError> {
    let items = StrictJsonValue::get_field(entries, "data")
        .and_then(StrictJsonValue::as_array)
        .ok_or_else(|| invalid("data"))?;
    let rows = (filter.range.row1 - filter.range.row0) as usize;
    let mut total = 0usize;
    let mut columns = Vec::with_capacity(items.len());
    for item in items {
        let obj = item.as_object().ok_or_else(|| invalid("data"))?;
        let col = StrictJsonValue::get_field(obj, "col")
            .and_then(StrictJsonValue::as_u64)
            .filter(|c| (1..=16_384).contains(c))
            .map(|c| c as u32 - 1)
            .ok_or_else(|| invalid("col"))?;
        let values = StrictJsonValue::get_field(obj, "values")
            .and_then(StrictJsonValue::as_array)
            .filter(|v| v.len() == rows)
            .ok_or_else(|| invalid("values"))?;
        total += values.len();
        if total > MAX_REQUEST_CELLS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "필터 요청의 셀이 {MAX_REQUEST_CELLS}개를 넘습니다."
            )));
        }
        let mut texts = values
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| invalid("values"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut aux = needs_aux(filter, col, target).then(|| CellAux {
            numbers: texts
                .iter()
                .map(|t| number_of(t).unwrap_or(f64::NAN))
                .collect(),
            dates: RowBits::new(rows),
            ..CellAux::default()
        });
        for (key, font) in [("fill", false), ("font", true)] {
            if let Some(runs) = StrictJsonValue::get_field(obj, key) {
                let colors = parse_colors(runs, rows, key)?;
                let aux = aux.get_or_insert_with(CellAux::default);
                *(if font { &mut aux.fonts } else { &mut aux.fills }) = colors;
            }
        }
        if let Some(runs) = StrictJsonValue::get_field(obj, "fmt") {
            apply_formats(&mut texts, runs, &mut aux)?;
        }
        columns.push(Column { col, texts, aux });
    }
    Ok(columns)
}

fn push_text(out: &mut String, text: &str) {
    out.push('"');
    EasyOfficeIpcRouter::escape_json_into(out, text);
    out.push('"');
}

impl EasyOfficeIpcRouter {
    pub(crate) fn sheet_filter(entries: &Entries) -> Result<String, EasyOfficeError> {
        let filter = parse_filter(
            StrictJsonValue::get_field(entries, "filter").ok_or_else(|| invalid("filter"))?,
        )?;
        let target = match Self::optional_json_str(entries, "op")? {
            Some("values") => Some(values_column(entries, &filter)?),
            _ => None,
        };
        let columns = parse_data(entries, &filter, target)?;
        let views: Vec<ColumnText<'_>> = columns
            .iter()
            .map(|c| ColumnText {
                col: c.col,
                texts: &c.texts,
                aux: c.aux.as_ref(),
            })
            .collect();
        match Self::optional_json_str(entries, "op")? {
            Some("apply") => {
                let bits = hidden_bits(&filter, &views, None)?;
                Ok(format!(
                    "{{\"hidden\":{},\"count\":{}}}",
                    runs_json(&bits.runs(filter.range.row0 + 1)),
                    bits.count()
                ))
            }
            Some("values") => values_answer(entries, &filter, &views),
            _ => Err(invalid("op")),
        }
    }
}

/// The column a `values` request asks the list of.
fn values_column(entries: &Entries, filter: &SheetFilter) -> Result<u32, EasyOfficeError> {
    StrictJsonValue::get_field(entries, "col")
        .and_then(StrictJsonValue::as_u64)
        .filter(|c| (1..=16_384).contains(c))
        .map(|c| c as u32 - 1)
        .filter(|c| (filter.range.col0..=filter.range.col1).contains(c))
        .ok_or_else(|| invalid("col"))
}

fn values_answer(
    entries: &Entries,
    filter: &SheetFilter,
    views: &[ColumnText<'_>],
) -> Result<String, EasyOfficeError> {
    let col = values_column(entries, filter)?;
    let query = EasyOfficeIpcRouter::optional_json_str(entries, "query")?.unwrap_or("");
    if query.chars().count() > MAX_QUERY_CHARS {
        return Err(invalid("query"));
    }
    let target = views
        .iter()
        .find(|v| v.col == col)
        .ok_or_else(|| invalid("열 값이 없습니다"))?;
    let excluded = hidden_bits(filter, views, Some(col))?;
    // Without a search the dates are the tree's, and the list holds the other texts; a search lists every
    // text that has it, the dates' too.
    let tree = target
        .aux
        .filter(|_| query.is_empty())
        .map(|aux| (aux, date_tree(&aux.numbers, &aux.dates, &excluded)))
        .filter(|(_, tree)| !tree.years.is_empty());
    let mut listed = excluded.clone();
    if let Some((aux, _)) = &tree {
        listed.union_with(&tree_rows(&aux.numbers, &aux.dates, &excluded));
    }
    let list = value_list(target.texts, &listed, query, MAX_FILTER_VALUES);
    let mut out = String::from("{\"items\":[");
    for (i, item) in list.items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('[');
        push_text(&mut out, &item.text);
        let _ = write!(out, ",{}]", item.count);
    }
    let _ = write!(out, "],\"blank\":{},\"more\":{}", list.blank, list.more);
    if let Some((_, tree)) = &tree {
        push_date_tree(&mut out, tree);
    }
    if let Some(aux) = target.aux {
        push_colors(&mut out, aux, &excluded);
    }
    out.push('}');
    Ok(out)
}

/// `,"dates":{"years":[...],"more":bool}` when the column has dates among the visible rows.
fn push_date_tree(out: &mut String, tree: &DateTree) {
    out.push_str(",\"dates\":{\"years\":[");
    for (i, (year, count, months)) in tree.years.iter().enumerate() {
        let _ = write!(out, "{}[{year},{count},[", if i > 0 { "," } else { "" });
        for (j, month) in months.iter().enumerate() {
            let _ = write!(
                out,
                "{}[{},{},[",
                if j > 0 { "," } else { "" },
                month.month,
                month.count
            );
            for (k, (day, n)) in month.days.iter().enumerate() {
                let _ = write!(out, "{}[{day},{n}]", if k > 0 { "," } else { "" });
            }
            out.push_str("]]");
        }
        out.push_str("]]");
    }
    let _ = write!(out, "],\"more\":{}}}", tree.more);
}

/// `,"colors":{...}` when the column has colours (a fill or a font colour list) among the visible rows.
fn push_colors(out: &mut String, aux: &CellAux, excluded: &RowBits) {
    if aux.fills.is_empty() && aux.fonts.is_empty() {
        return;
    }
    out.push_str(",\"colors\":{");
    for (i, (name, font)) in [("fill", false), ("font", true)].into_iter().enumerate() {
        let list = color_list(if font { &aux.fonts } else { &aux.fills }, excluded);
        let _ = write!(out, "{}\"{name}\":[", if i > 0 { "," } else { "" });
        for (j, (rgb, count)) in list.colors.iter().enumerate() {
            let _ = write!(
                out,
                "{}[\"{rgb:06x}\",{count}]",
                if j > 0 { "," } else { "" }
            );
        }
        let none = if font { "noFont" } else { "noFill" };
        let _ = write!(out, "],\"{none}\":{}", list.none);
    }
    out.push('}');
}

/// The filters of an opened sheet with the hidden rows told apart (the file has one `hidden` flag
/// per row): the rows the conditions of a filter (the sheet's or a table's) also hide become that
/// filter's, the rest stay the user's.
pub(crate) struct OpenSplit {
    pub(crate) filter: Option<SheetFilter>,
    pub(crate) tables: Vec<SheetTable>,
    pub(crate) hidden: Vec<(u32, u32)>,
}

/// A cell of an opened sheet: row, column, stored text.
type OpenCell = (u32, u32, String);

/// The stored texts of the cells of `columns` below the header of `range`, with the numbers the
/// criteria read, and the rows whose cell is a formula (unread).
fn open_columns(
    range: CellBox,
    columns: &[FilterColumn],
    cells: &[&OpenCell],
    styling: &SheetStyling,
) -> (Vec<Column>, RowBits) {
    let rows = (range.row1 - range.row0) as usize;
    let cols: Vec<u32> = columns.iter().map(|c| c.col).collect();
    let mut data: Vec<Column> = columns
        .iter()
        .map(|c| Column {
            col: c.col,
            texts: vec![String::new(); rows],
            aux: reads_numbers(&c.criteria).then(|| CellAux {
                numbers: vec![f64::NAN; rows],
                dates: RowBits::new(rows),
                ..CellAux::default()
            }),
        })
        .collect();
    let mut unread = RowBits::new(rows);
    // `cells` is sorted by row: only the rows below the header of the range are visited.
    let first = cells.partition_point(|c| c.0 <= range.row0);
    let end = cells.partition_point(|c| c.0 <= range.row1);
    for (row, col, raw) in cells[first..end].iter().copied() {
        let Some(at) = cols.binary_search(col).ok() else {
            continue;
        };
        let offset = (*row - range.row0 - 1) as usize;
        if raw.starts_with('=') {
            unread.set(offset);
            continue;
        }
        let code = styling
            .cells
            .get(&(*row, *col))
            .and_then(|style| resolve_code(style.num_fmt, &styling.num_fmts))
            .filter(|_| raw.len() <= MAX_FORMATTED_BYTES);
        let column = &mut data[at];
        if let Some(aux) = column.aux.as_mut() {
            if let Some(n) = number_of(raw) {
                aux.numbers[offset] = n;
                if code.is_some_and(|c| code_shows_date(c, n)) {
                    aux.dates.set(offset);
                }
            }
        }
        let shown = code.and_then(|code| format_cell_value(code, raw));
        column.texts[offset] = shown.map_or_else(|| raw.clone(), |s| s.text);
    }
    // A colour condition is judged against the colours of the cells themselves, blank ones too (PD-227). A
    // colour a conditional format gives is not known here: a file's colour condition over such a column hides
    // every row, which are the rows the file already hides (they stay the filter's) and the rows it shows stay.
    let colored: Vec<usize> = columns
        .iter()
        .enumerate()
        .filter(|(_, c)| matches!(c.criteria, FilterCriteria::Color { .. }))
        .map(|(at, _)| at)
        .collect();
    if !colored.is_empty() {
        for &at in &colored {
            let aux = data[at].aux.get_or_insert_with(CellAux::default);
            aux.fills = vec![NO_COLOR; rows];
            aux.fonts = vec![NO_COLOR; rows];
        }
        let band = (range.row0 + 1, 0)..=(range.row1, u32::MAX);
        for (&(row, col), style) in styling.cells.range(band) {
            let Some(at) = colored.iter().copied().find(|&at| cols[at] == col) else {
                continue;
            };
            let offset = (row - range.row0 - 1) as usize;
            if let Some(aux) = data[at].aux.as_mut() {
                aux.fills[offset] = style.fill.unwrap_or(NO_COLOR);
                aux.fonts[offset] = style.font_color.unwrap_or(NO_COLOR);
            }
        }
    }
    (data, unread)
}

/// Moves the rows of `hidden` that the conditions of `columns` over `range` also hide into the
/// returned list; `None` when the conditions cannot be judged (a bound is over).
fn take_filter_rows(
    range: CellBox,
    columns: &[FilterColumn],
    cells: &[&OpenCell],
    styling: &SheetStyling,
    hidden: &mut Vec<(u32, u32)>,
) -> Option<Vec<(u32, u32)>> {
    let mut filter = SheetFilter::new(range);
    filter.columns = columns.to_vec();
    let (data, unread) = open_columns(range, columns, cells, styling);
    let views: Vec<ColumnText<'_>> = data
        .iter()
        .map(|c| ColumnText {
            col: c.col,
            texts: &c.texts,
            aux: c.aux.as_ref(),
        })
        .collect();
    split_hidden_rows(&mut filter, hidden, &views, &unread).ok()?;
    Some(filter.hidden)
}

/// The filters of an opened sheet with the hidden rows told apart; `None` when the sheet has no
/// filter with conditions.
pub(crate) fn split_for_open(
    cells: &[(u32, u32, String)],
    styling: &SheetStyling,
) -> Option<OpenSplit> {
    let mut filter = styling.filter.clone().filter(SheetFilter::has_criteria);
    let mut tables = styling.tables.clone();
    let tables_filtered = tables
        .iter()
        .any(|t| t.filter.as_ref().is_some_and(|f| !f.is_empty()));
    if filter.is_none() && !tables_filtered {
        return None;
    }
    // One index for the sheet's filter and every table: the cells by row, so a filter reads only its rows.
    let mut by_row: Vec<&OpenCell> = cells.iter().collect();
    by_row.sort_unstable_by_key(|c| c.0);
    let mut hidden = styling.hidden_rows.clone();
    // A file that marks the filter's rows itself (ODS `table:visibility`) has them told apart already.
    if let Some(f) = filter.as_mut().filter(|f| f.hidden.is_empty()) {
        f.hidden = take_filter_rows(f.range, &f.columns, &by_row, styling, &mut hidden)?;
    }
    for table in tables.iter_mut().filter(|t| t.filter_hidden.is_empty()) {
        let range = table.filter_range();
        let conditions = table.filter.as_deref().filter(|f| !f.is_empty());
        // A range too tall for a filter keeps its conditions and its rows stay the user's.
        if let Some(conditions) = conditions.filter(|_| range.row1 - range.row0 < MAX_FILTER_ROWS) {
            table.filter_hidden =
                take_filter_rows(range, conditions, &by_row, styling, &mut hidden)
                    .unwrap_or_default();
        }
    }
    Some(OpenSplit {
        filter,
        tables,
        hidden,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use easyoffice_formats::CellBox;

    fn call(body: &str) -> Result<String, EasyOfficeError> {
        EasyOfficeIpcRouter::dispatch("evaluate_sheet", body)
    }

    const FILTER: &str = r#"{"range":"A1:B7","columns":[{"col":1,"values":["영업","개발"]},{"col":2,"conditions":[{"op":"greaterThan","text":"10"}]}]}"#;
    const DATA: &str = r#"[{"col":1,"values":["영업","영업","개발","","기획","영업"]},{"col":2,"values":["5","20","30","40","50","abc"]}]"#;

    #[test]
    fn apply_answers_the_hidden_rows_one_based() {
        let json = call(&format!(
            r#"{{"sessionAction":"filter","op":"apply","filter":{FILTER},"data":{DATA}}}"#
        ))
        .unwrap();
        // Data rows 1..6 are sheet rows 2..7: row 2 (5 <= 10), 5 (blank), 6 (기획), 7 ("abc").
        assert_eq!(json, r#"{"hidden":[[2,2],[5,7]],"count":4}"#);
    }

    #[test]
    fn values_leave_out_the_rows_other_columns_hide() {
        let json = call(&format!(
            r#"{{"sessionAction":"filter","op":"values","col":1,"filter":{FILTER},"data":{DATA}}}"#
        ))
        .unwrap();
        // Column B hides rows 2 (5) and 7 ("abc"): the rest are 영업, 개발, "", 기획.
        assert_eq!(
            json,
            r#"{"items":[["개발",1],["기획",1],["영업",1]],"blank":1,"more":false}"#
        );
        let searched = call(&format!(
            r#"{{"sessionAction":"filter","op":"values","col":1,"query":"영","filter":{FILTER},"data":{DATA}}}"#
        ))
        .unwrap();
        assert_eq!(searched, r#"{"items":[["영업",1]],"blank":0,"more":false}"#);
    }

    #[test]
    fn the_displayed_text_is_judged_not_the_stored_one() {
        let filter = r#"{"range":"A1:A3","columns":[{"col":1,"values":["2023-03-15"]}]}"#;
        let data = r#"[{"col":1,"values":["45000","45001"],"fmt":[[0,1,"yyyy-mm-dd"]]}]"#;
        let json = call(&format!(
            r#"{{"sessionAction":"filter","op":"apply","filter":{filter},"data":{data}}}"#
        ))
        .unwrap();
        assert_eq!(json, r#"{"hidden":[[3,3]],"count":1}"#);
    }

    #[test]
    fn opening_tells_the_filter_s_rows_from_the_user_s() {
        let mut filter = SheetFilter::new(CellBox::parse("A1:A5").unwrap());
        filter.columns = vec![FilterColumn {
            col: 0,
            criteria: FilterCriteria::Values {
                blank: false,
                values: vec!["영업".into()],
                dates: Vec::new(),
            },
        }];
        let styling = SheetStyling {
            filter: Some(filter),
            hidden_rows: vec![(2, 3), (7, 7)],
            ..SheetStyling::default()
        };
        let cells: Vec<(u32, u32, String)> = ["부서", "영업", "개발", "영업", "개발"]
            .iter()
            .enumerate()
            .map(|(i, t)| (i as u32, 0, t.to_string()))
            .collect();
        let split = split_for_open(&cells, &styling).unwrap();
        // Row 3 (index 2) is 개발 and hidden by the filter; row 4 is 영업 and the user's.
        assert_eq!(split.filter.unwrap().hidden, vec![(2, 2)]);
        assert_eq!(split.hidden, vec![(3, 3), (7, 7)]);
        assert!(split_for_open(&cells, &SheetStyling::default()).is_none());
    }

    #[test]
    fn opening_judges_a_colour_condition_against_the_colours_of_the_cells() {
        use easyoffice_formats::CellStyle;
        let mut filter = SheetFilter::new(CellBox::parse("A1:A6").unwrap());
        filter.columns = vec![FilterColumn {
            col: 0,
            criteria: FilterCriteria::Color {
                font: false,
                color: Some(0xFFC7CE),
            },
        }];
        let pink = CellStyle {
            fill: Some(0xFFC7CE),
            ..CellStyle::default()
        };
        let green = CellStyle {
            fill: Some(0x00FF00),
            ..CellStyle::default()
        };
        let mut styling = SheetStyling {
            filter: Some(filter),
            // The file hides rows 3, 4 and 6 (indexes 2, 3, 5): the user hid row 4 too, but the file says one flag.
            hidden_rows: vec![(2, 3), (5, 5)],
            ..SheetStyling::default()
        };
        // Row 2 pink (shown), row 3 green, row 4 none, row 5 pink (blank cell, no text), row 6 green.
        styling.cells.insert((1, 0), pink);
        styling.cells.insert((2, 0), green);
        styling.cells.insert((4, 0), pink);
        styling.cells.insert((5, 0), green);
        let cells: Vec<(u32, u32, String)> = ["부서", "a", "b", "c"]
            .iter()
            .enumerate()
            .map(|(i, t)| (i as u32, 0, t.to_string()))
            .collect();
        let split = split_for_open(&cells, &styling).unwrap();
        // Rows 3, 4 and 6 hide because their cell is not pink, so the file's hidden rows are all the filter's.
        assert_eq!(split.filter.unwrap().hidden, vec![(2, 3), (5, 5)]);
        assert!(split.hidden.is_empty());
        // The same file with no colours of its own: nothing is pink, every row of the file's is the filter's.
        styling.cells.clear();
        let split = split_for_open(&cells, &styling).unwrap();
        assert_eq!(split.filter.unwrap().hidden, vec![(2, 3), (5, 5)]);
    }

    /// 100,000 rows of 3 columns, a sheet filter over all of them and 300 tables of 100 rows each, every
    /// table with a condition: the time `split_for_open` takes (`--ignored --nocapture`, release).
    #[test]
    #[ignore = "measurement"]
    fn opening_a_sheet_with_a_filter_and_many_filtered_tables() {
        let rows = 100_000u32;
        let cells: Vec<(u32, u32, String)> = (0..rows)
            .flat_map(|r| (0..3).map(move |c| (r, c, format!("v{}", r % 7))))
            .collect();
        let on = |col| FilterColumn {
            col,
            criteria: FilterCriteria::Values {
                blank: false,
                values: vec!["v1".into()],
                dates: Vec::new(),
            },
        };
        let mut filter = SheetFilter::new(CellBox::parse(&format!("A1:C{rows}")).unwrap());
        filter.columns = vec![on(0)];
        // The tables sit in columns far to the right of the data, so the filter does not share a cell with them.
        let tables: Vec<SheetTable> = (0..300u32)
            .map(|i| {
                let first = i * 100 + 1;
                let mut t = SheetTable::new(
                    format!("T{i}"),
                    CellBox::parse(&format!("H{first}:J{}", first + 99)).unwrap(),
                );
                t.filter = Some(vec![on(7)]);
                t
            })
            .collect();
        let styling = SheetStyling {
            filter: Some(filter),
            tables,
            hidden_rows: vec![(2, 50_000)],
            ..SheetStyling::default()
        };
        let start = std::time::Instant::now();
        let split = split_for_open(&cells, &styling).unwrap();
        println!("split_for_open: {:?}", start.elapsed());
        assert!(split.filter.is_some());
    }

    #[test]
    fn rows_a_file_marks_as_the_filter_s_are_kept_whatever_the_conditions_say() {
        let mut filter = SheetFilter::new(CellBox::parse("A1:A5").unwrap());
        filter.columns = vec![FilterColumn {
            col: 0,
            criteria: FilterCriteria::Values {
                blank: false,
                values: vec!["영업".into()],
                dates: Vec::new(),
            },
        }];
        // The reader of a format that tells the kinds apart put row 4 (영업) in the filter's rows.
        filter.hidden = vec![(4, 4)];
        let mut table = SheetTable::new("T", CellBox::parse("C1:C5").unwrap());
        table.filter = filter.columns.clone().into();
        table.filter_hidden = vec![(3, 3)];
        let styling = SheetStyling {
            filter: Some(filter),
            tables: vec![table],
            hidden_rows: vec![(2, 2)],
            ..SheetStyling::default()
        };
        let cells: Vec<(u32, u32, String)> = ["부서", "영업", "개발", "영업", "개발"]
            .iter()
            .enumerate()
            .map(|(i, t)| (i as u32, 0, t.to_string()))
            .collect();
        let split = split_for_open(&cells, &styling).unwrap();
        assert_eq!(split.filter.unwrap().hidden, vec![(4, 4)]);
        assert_eq!(split.tables[0].filter_hidden, vec![(3, 3)]);
        assert_eq!(split.hidden, vec![(2, 2)]);
    }

    fn apply(filter: &str, data: &str) -> Result<String, EasyOfficeError> {
        call(&format!(
            r#"{{"sessionAction":"filter","op":"apply","filter":{filter},"data":{data}}}"#
        ))
    }

    #[test]
    fn top_and_average_judge_the_numbers_of_the_visible_rows() {
        let data = r#"[{"col":1,"values":["a","a","b","a","a"]},{"col":2,"values":["10","99","30","20","x"]}]"#;
        let top =
            r#"{"range":"A1:B6","columns":[{"col":1,"values":["a"]},{"col":2,"top":{"count":2}}]}"#;
        // The b row (30) does not compete: of 10, 99, 20 and "x" the two largest are 99 and 20.
        assert_eq!(
            apply(top, data).unwrap(),
            r#"{"hidden":[[2,2],[4,4],[6,6]],"count":3}"#
        );
        let below = r#"{"range":"A1:B6","columns":[{"col":2,"average":{"above":false}}]}"#;
        // The average of 10, 99, 30, 20 is 39.75: 10, 30 and 20 are below it.
        assert_eq!(
            apply(below, data).unwrap(),
            r#"{"hidden":[[3,3],[6,6]],"count":2}"#
        );
        let bottom = r#"{"range":"A1:B6","columns":[{"col":2,"top":{"bottom":true,"percent":true,"count":50}}]}"#;
        assert_eq!(
            apply(bottom, data).unwrap(),
            r#"{"hidden":[[3,4],[6,6]],"count":3}"#
        );
    }

    #[test]
    fn a_colour_criterion_reads_the_colour_runs_and_a_date_group_the_format_runs() {
        let fill =
            r#"[{"col":1,"values":["a","b","c","d"],"fill":[[0,0,"ffc7ce"],[2,3,"ffc7ce"]]}]"#;
        let pink = r#"{"range":"A1:A5","columns":[{"col":1,"color":{"rgb":"ffc7ce"}}]}"#;
        assert_eq!(
            apply(pink, fill).unwrap(),
            r#"{"hidden":[[3,3]],"count":1}"#
        );
        let none = r#"{"range":"A1:A5","columns":[{"col":1,"color":{}}]}"#;
        assert_eq!(
            apply(none, fill).unwrap(),
            r#"{"hidden":[[2,2],[4,5]],"count":3}"#
        );
        let font =
            r#"{"range":"A1:A5","columns":[{"col":1,"color":{"font":true,"rgb":"ff0000"}}]}"#;
        // No text colour was sent: the condition is not judged (the sheet has no colour of its own to read).
        assert_eq!(apply(font, fill).unwrap(), r#"{"hidden":[],"count":0}"#);
        let plain = r#"[{"col":1,"values":["a","b","c","d"]}]"#;
        assert_eq!(apply(pink, plain).unwrap(), r#"{"hidden":[],"count":0}"#);
        assert_eq!(apply(none, plain).unwrap(), r#"{"hidden":[],"count":0}"#);
        // 45000 is 2023-03-15, 45400 is 2024-04-18; only the cells in a date format are dates.
        let dates =
            r#"[{"col":1,"values":["45000","45400","45000","hello"],"fmt":[[0,1,"yyyy-mm-dd"]]}]"#;
        let year =
            r#"{"range":"A1:A5","columns":[{"col":1,"dates":[{"unit":"year","year":2023}]}]}"#;
        // The third cell holds 45000 too, but its format is General: it is a number, not a date.
        assert_eq!(
            apply(year, dates).unwrap(),
            r#"{"hidden":[[3,5]],"count":3}"#
        );
        let day = r#"{"range":"A1:A5","columns":[{"col":1,"dates":[{"unit":"day","year":2024,"month":4,"day":18}]}]}"#;
        assert_eq!(
            apply(day, dates).unwrap(),
            r#"{"hidden":[[2,2],[4,5]],"count":3}"#
        );
    }

    #[test]
    fn the_value_list_of_a_column_with_dates_and_colours_carries_the_tree_and_the_colours() {
        let filter = r#"{"range":"A1:A6","columns":[]}"#;
        let data = r#"[{"col":1,"values":["45000","45001","45400","","t"],"fmt":[[0,2,"yyyy-mm-dd"]],"fill":[[0,1,"ff0000"]]}]"#;
        let json = call(&format!(
            r#"{{"sessionAction":"filter","op":"values","col":1,"filter":{filter},"data":{data}}}"#
        ))
        .unwrap();
        assert!(
            json.contains(r#""dates":{"years":[[2023,2,[[3,2,[[15,1],[16,1]]]]],[2024,1,[[4,1,[[18,1]]]]]],"more":false}"#),
            "{json}"
        );
        assert!(
            json.contains(r#""colors":{"fill":[["ff0000",2]],"noFill":3,"font":[],"noFont":5}"#),
            "{json}"
        );
        // The dates are the tree's: the list holds the other texts and the empty cell.
        assert!(
            json.starts_with(r#"{"items":[["t",1]],"blank":1,"more":false,"dates""#),
            "{json}"
        );
        // A search lists every text that has it, dates included, and brings no tree.
        let searched = call(&format!(
            r#"{{"sessionAction":"filter","op":"values","col":1,"query":"2023","filter":{filter},"data":{data}}}"#
        ))
        .unwrap();
        assert!(
            searched.starts_with(r#"{"items":[["2023-03-15",1],["2023-03-16",1]],"#)
                && !searched.contains(r#""dates""#),
            "{searched}"
        );
    }

    #[test]
    fn new_criteria_over_a_bound_or_malformed_are_refused() {
        let data = r#"[{"col":1,"values":["1","2"]}]"#;
        for column in [
            r#"{"col":1,"top":{"count":0}}"#,
            r#"{"col":1,"top":{"count":101,"percent":true}}"#,
            r#"{"col":1,"top":{"count":1000001}}"#,
            r#"{"col":1,"top":{}}"#,
            r#"{"col":1,"average":{}}"#,
            r#"{"col":1,"average":"above"}"#,
            r#"{"col":1,"color":{"rgb":"red"}}"#,
            r#"{"col":1,"color":{"rgb":"ff00001"}}"#,
            r#"{"col":1,"dates":[{"unit":"month","year":2024}]}"#,
            r#"{"col":1,"dates":[{"unit":"day","year":2023,"month":2,"day":29}]}"#,
            r#"{"col":1,"dates":[{"unit":"week","year":2023}]}"#,
            r#"{"col":1,"dates":[{"unit":"year","year":1700}]}"#,
            r#"{"col":1,"dates":{}}"#,
        ] {
            let filter = format!(r#"{{"range":"A1:A3","columns":[{column}]}}"#);
            assert!(apply(&filter, data).is_err(), "{column}");
        }
        let filter = r#"{"range":"A1:A3","columns":[{"col":1,"color":{"rgb":"ff0000"}}]}"#;
        for bad in [
            r#"[{"col":1,"values":["1","2"],"fill":[[0,2,"ff0000"]]}]"#,
            r#"[{"col":1,"values":["1","2"],"fill":[[1,0,"ff0000"]]}]"#,
            r#"[{"col":1,"values":["1","2"],"fill":[[0,0,"zzzzzz"]]}]"#,
            r#"[{"col":1,"values":["1","2"],"fill":[[0,0]]}]"#,
            r#"[{"col":1,"values":["1","2"],"fill":"red"}]"#,
        ] {
            assert!(apply(filter, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_hostile_list_of_many_colours_and_days_stays_bounded() {
        // 20,000 distinct days and 5,000 distinct colours over 20,000 rows.
        let rows = 20_000;
        let values: Vec<String> = (0..rows).map(|i| (40_000 + i).to_string()).collect();
        let values = format!(
            "[{}]",
            values
                .iter()
                .map(|v| format!("\"{v}\""))
                .collect::<Vec<_>>()
                .join(",")
        );
        let fill = (0..rows)
            .map(|i| format!("[{i},{i},\"{:06x}\"]", i % 5000))
            .collect::<Vec<_>>()
            .join(",");
        let data = format!(
            r#"[{{"col":1,"values":{values},"fmt":[[0,{last},"yyyy-mm-dd"]],"fill":[{fill}]}}]"#,
            last = rows - 1
        );
        let filter = format!(r#"{{"range":"A1:A{}","columns":[]}}"#, rows + 1);
        let json = call(&format!(
            r#"{{"sessionAction":"filter","op":"values","col":1,"filter":{filter},"data":{data}}}"#
        ))
        .unwrap();
        assert!(json.contains(r#""more":true}"#), "the day tree is cut");
        assert!(json.matches(r#"["#).count() > 1_000);
    }

    #[test]
    fn bad_requests_are_refused() {
        let short = r#"[{"col":1,"values":["a"]},{"col":2,"values":["1"]}]"#;
        for (op, data) in [
            ("apply", short),
            ("apply", r#"[{"col":1,"values":["a","b","c","d","e","f"]}]"#),
            ("nope", DATA),
            ("apply", r#"[{"col":1,"values":[1,2,3,4,5,6]}]"#),
            ("apply", r#"[{"col":0,"values":["a","b","c","d","e","f"]}]"#),
        ] {
            let body = format!(
                r#"{{"sessionAction":"filter","op":"{op}","filter":{FILTER},"data":{data}}}"#
            );
            assert!(call(&body).is_err(), "{op} {data}");
        }
        let too_many = format!(
            r#"{{"sessionAction":"filter","op":"values","col":9,"filter":{FILTER},"data":{DATA}}}"#
        );
        assert!(call(&too_many).is_err());
    }
}

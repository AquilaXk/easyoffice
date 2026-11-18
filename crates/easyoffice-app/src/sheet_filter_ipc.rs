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
    color_list, date_tree, hidden_bits_at, number_of, split_hidden_rows, tree_rows, utc_today,
    value_list, CellAux, ColumnText, DateTree, RowBits, FIRST_DAY_SERIAL, LAST_DAY_SERIAL,
    NO_COLOR,
};
use easyoffice_sheet::{code_shows_date, compile_for_page, format_cell_value};
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
        FilterCriteria::Top { .. }
        | FilterCriteria::Average { .. }
        | FilterCriteria::DynamicDate(_) => true,
        FilterCriteria::Custom { .. } | FilterCriteria::Color { .. } => false,
    }
}

/// Whether a column's criteria read the displayed text of its cells.
fn reads_shown_text(criteria: &FilterCriteria) -> bool {
    matches!(
        criteria,
        FilterCriteria::Values { .. } | FilterCriteria::Custom { .. }
    )
}

/// Applies the format runs of one data column to its texts; `numbers` and `dates` are filled for
/// the cells whose format shows a date when the column keeps them. Format strings are compiled
/// once per run, and columns whose criteria never inspect displayed text (`DynamicDate`, `Top`,
/// `Average`, `Color`) skip string formatting altogether (PD-238).
fn apply_formats(
    texts: &mut [String],
    runs: &StrictJsonValue,
    aux: &mut Option<CellAux>,
    format_texts: bool,
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
        let compiled = compile_for_page(code);
        for (i, text) in texts.iter_mut().enumerate().take(last + 1).skip(first) {
            if text.len() > MAX_FORMATTED_BYTES {
                continue;
            }
            if let Some(aux) = aux.as_mut() {
                if aux.number(i).is_some_and(|n| compiled.shows_date(n)) {
                    aux.dates.set(i);
                }
            }
            if format_texts {
                if let Some(shown) = compiled.format(text) {
                    *text = shown.text;
                }
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
            let format_texts = target == Some(col)
                || filter
                    .column(col)
                    .is_some_and(|c| reads_shown_text(&c.criteria));
            apply_formats(&mut texts, runs, &mut aux, format_texts)?;
        }
        columns.push(Column { col, texts, aux });
    }
    debug_assert!(
        columns.windows(2).all(|w| w[0].col < w[1].col),
        "filter columns must be sorted by col ascending"
    );
    Ok(columns)
}

fn push_text(out: &mut String, text: &str) {
    out.push('"');
    EasyOfficeIpcRouter::escape_json_into(out, text);
    out.push('"');
}

fn parse_today(entries: &Entries) -> Result<i64, EasyOfficeError> {
    match StrictJsonValue::get_field(entries, "today") {
        None | Some(StrictJsonValue::Null) => Ok(utc_today()),
        Some(v) => v
            .as_u64()
            .and_then(|n| i64::try_from(n).ok())
            .filter(|&n| (FIRST_DAY_SERIAL..=LAST_DAY_SERIAL).contains(&n))
            .ok_or_else(|| invalid("today")),
    }
}

impl EasyOfficeIpcRouter {
    pub(crate) fn sheet_filter(entries: &Entries) -> Result<String, EasyOfficeError> {
        let filter = parse_filter(
            StrictJsonValue::get_field(entries, "filter").ok_or_else(|| invalid("filter"))?,
        )?;
        let today = parse_today(entries)?;
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
                let bits = hidden_bits_at(&filter, &views, None, today)?;
                Ok(format!(
                    "{{\"hidden\":{},\"count\":{}}}",
                    runs_json(&bits.runs(filter.range.row0 + 1)),
                    bits.count()
                ))
            }
            Some("values") => values_answer(entries, &filter, &views, today),
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
    today: i64,
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
    let excluded = hidden_bits_at(filter, views, Some(col), today)?;
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
        if reads_shown_text(&columns[at].criteria) {
            let shown = code.and_then(|code| format_cell_value(code, raw));
            column.texts[offset] = shown.map_or_else(|| raw.clone(), |s| s.text);
        }
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
#[path = "sheet_filter_ipc_tests.rs"]
mod tests;

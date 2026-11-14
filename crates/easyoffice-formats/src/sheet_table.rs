//! The tables of a worksheet (PD-224): a table part in XLSX, a database range in ODS. A table is a
//! named rectangle with an optional header row and totals row, column names, a style name with its
//! banding options and an auto filter state.
//!
//! 참조 구현: IronCalc `base/src/types.rs` lines 426-468 (`Table`, `TableColumn`: name, reference,
//! `header_row_count`, `totals_row_count`, columns, style info, `has_filters`) and
//! `xlsx/src/import/tables.rs` lines 22-170 (`load_table`: attribute defaults of ECMA-376 18.5.1.2,
//! `headerRowCount` 1, `totalsRowCount` 0, `showRowStripes` on), MIT OR Apache-2.0, commit
//! 87b684bb04a87c63bdd6343c29036450b1cfd358. `rust_xlsxwriter` `src/table.rs` lines 1286-1430 (the
//! part's element order, `totalsRowShown`, the filter range that leaves out the totals row, the
//! `tableStyleInfo` flags), MIT OR Apache-2.0, commit 0c034f85b28e4eaed657ee53a326156b4b03a5d6.
//! Source and license text: `THIRD_PARTY_NOTICES.md`.
//!
//! Modifications: the model has no differential format ids, calculated column formulas, totals
//! functions or query connections (a file that has them is read without them); the filter state keeps
//! only the conditions of the columns (`FilterColumn`) and the rows they hide (`filter_hidden`, kept
//! apart from the rows a user hid, PD-226); every size is bounded and a table over a bound is turned away, never trimmed. Names follow
//! the defined-name syntax of `sheet_names` without `?`, because a table name is a name of the
//! workbook (ECMA-376 18.5.1.2 `displayName`).
//!
//! The readers and writers sit beside the model: `xlsx_tables.rs` and `ods_tables.rs`.

use std::collections::HashSet;

use crate::sheet_filter::{columns_within_bounds, FilterColumn, MAX_FILTER_ROWS};
use crate::sheet_names::{defined_name_problem, fold_name, NameProblem};
use crate::{CellBox, EasyOfficeError};

#[path = "ods_tables.rs"]
pub(crate) mod ods_tables;
#[path = "xlsx_tables.rs"]
pub(crate) mod xlsx_tables;

/// Tables one workbook may carry, and so one sheet. A reader refuses a workbook that lists more (the
/// count includes the tables it turns away, so a flood of them is never walked).
pub const MAX_TABLES: usize = 1_000;
/// Columns of all the tables of a workbook together.
pub const MAX_TABLE_COLUMNS_TOTAL: usize = 65_536;
/// Characters of a column name, a style name and a table name (ECMA-376 18.5.1.2).
pub const MAX_TABLE_TEXT_CHARS: usize = 255;
const LAST_ROW: u32 = 1_048_575;
const LAST_COL: u32 = 16_383;

/// One table. `range` includes the header row and the totals row when it has them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetTable {
    pub name: String,
    pub range: CellBox,
    /// The first row of the range is the header row (`headerRowCount` 1).
    pub header: bool,
    /// The last row of the range is the totals row (`totalsRowCount` 1).
    pub totals: bool,
    /// One name per column of the range. With a header row the names are the header cells' text when the
    /// file is written; this list is what is kept until then and what a table without a header uses.
    pub columns: Vec<String>,
    /// The table style (`TableStyleMedium2`); `None` for a table without one.
    pub style: Option<String>,
    pub first_column: bool,
    pub last_column: bool,
    pub row_stripes: bool,
    pub column_stripes: bool,
    /// The auto filter of the table: `None` when the table has no filter buttons, else the conditions of
    /// its columns (absolute columns, inside the range).
    pub filter: Option<Vec<FilterColumn>>,
    /// The rows the conditions of `filter` hide, as sorted, disjoint zero-based inclusive runs, apart
    /// from the rows a user hid (PD-226). Empty for a table whose filter has no conditions, or whose
    /// range is too tall for the filter to be applied.
    pub filter_hidden: Vec<(u32, u32)>,
}

impl SheetTable {
    /// A table with a header row, banded rows and no style or filter.
    pub fn new(name: impl Into<String>, range: CellBox) -> Self {
        let width = (range.col1 - range.col0 + 1) as usize;
        Self {
            name: name.into(),
            range,
            header: true,
            totals: false,
            columns: (1..=width).map(|n| format!("Column{n}")).collect(),
            style: None,
            first_column: false,
            last_column: false,
            row_stripes: true,
            column_stripes: false,
            filter: None,
            filter_hidden: Vec::new(),
        }
    }

    pub const fn width(&self) -> u32 {
        self.range.col1 - self.range.col0 + 1
    }

    /// Rows the range must have: the header row, the totals row and one data row.
    pub const fn min_rows(&self) -> u32 {
        1 + self.header as u32 + self.totals as u32
    }

    /// The range of the header row and the data rows, the range of the table's auto filter.
    pub const fn filter_range(&self) -> CellBox {
        CellBox {
            row1: self.range.row1 - self.totals as u32,
            ..self.range
        }
    }

    /// Whether the table is within every bound a reader or the page may hand over; the name is checked for
    /// its syntax only, not for being unique.
    pub fn within_bounds(&self) -> bool {
        let r = self.range;
        let text_ok = |s: &str| s.chars().count() <= MAX_TABLE_TEXT_CHARS;
        table_name_problem(&self.name).is_none()
            && r.row1 <= LAST_ROW
            && r.col1 <= LAST_COL
            && r.row1 - r.row0 + 1 >= self.min_rows()
            && self.columns.len() == self.width() as usize
            && self.columns.iter().all(|c| text_ok(c))
            && self.style.as_deref().is_none_or(text_ok)
            && self
                .filter
                .as_ref()
                .is_none_or(|f| self.header && columns_within_bounds(f, &r))
            && self.filter_hidden_within_bounds()
    }

    /// The hidden runs sit below the header row inside the filter range, and the range is one the
    /// filter may be applied to.
    fn filter_hidden_within_bounds(&self) -> bool {
        let filtered = self.filter_range();
        self.filter_hidden.is_empty()
            || (self.filter.is_some()
                && filtered.row1 - filtered.row0 < MAX_FILTER_ROWS
                && self.filter_hidden.iter().all(|&(first, last)| {
                    first > filtered.row0 && first <= last && last <= filtered.row1
                })
                && self.filter_hidden.windows(2).all(|w| w[0].1 < w[1].0))
    }

    /// Whether the table shares a cell with `other`.
    pub fn overlaps(&self, other: &CellBox) -> bool {
        let r = self.range;
        r.row0 <= other.row1 && r.row1 >= other.row0 && r.col0 <= other.col1 && r.col1 >= other.col0
    }
}

/// The first rule `name` breaks as a table name: the defined-name syntax without `?`.
pub fn table_name_problem(name: &str) -> Option<NameProblem> {
    defined_name_problem(name).or_else(|| name.contains('?').then_some(NameProblem::BadChar('?')))
}

/// `name` made unique among `taken` (names folded by [`fold_name`]) by appending `_2`, `_3`, ...; the
/// result is in `taken`.
pub fn unique_name(taken: &mut HashSet<String>, name: &str) -> String {
    let mut candidate = name.to_string();
    let mut n = 1u32;
    while !taken.insert(fold_name(&candidate)) {
        n += 1;
        let suffix = format!("_{n}");
        let keep = MAX_TABLE_TEXT_CHARS - suffix.len();
        candidate = name.chars().take(keep).collect::<String>() + &suffix;
    }
    candidate
}

/// Whether the tables of one sheet are within their bounds and share no cell.
pub fn tables_within_bounds(tables: &[SheetTable]) -> bool {
    tables.len() <= MAX_TABLES
        && tables.iter().map(|t| t.columns.len()).sum::<usize>() <= MAX_TABLE_COLUMNS_TOTAL
        && tables.iter().all(SheetTable::within_bounds)
        && tables
            .iter()
            .enumerate()
            .all(|(i, t)| tables[i + 1..].iter().all(|o| !t.overlaps(&o.range)))
}

/// `merges` without the ranges that share a cell with a table: the spreadsheet program does not merge cells of a
/// table (its merge command is off inside one), so such a range is not kept. Borrowed when none is dropped.
pub fn merges_clear_of_tables<'a>(
    merges: &'a [CellBox],
    tables: &[SheetTable],
) -> std::borrow::Cow<'a, [CellBox]> {
    if tables.is_empty() || !merges.iter().any(|m| tables.iter().any(|t| t.overlaps(m))) {
        return std::borrow::Cow::Borrowed(merges);
    }
    std::borrow::Cow::Owned(
        merges
            .iter()
            .filter(|m| !tables.iter().any(|t| t.overlaps(m)))
            .copied()
            .collect(),
    )
}

/// Whether the tables of all the sheets of a workbook are within their bounds: each sheet's tables, and
/// the counts of the workbook.
pub fn workbook_tables_within_bounds(stylings: &[crate::SheetStyling]) -> bool {
    stylings.iter().map(|s| s.tables.len()).sum::<usize>() <= MAX_TABLES
        && stylings
            .iter()
            .flat_map(|s| &s.tables)
            .map(|t| t.columns.len())
            .sum::<usize>()
            <= MAX_TABLE_COLUMNS_TOTAL
        && stylings.iter().all(|s| tables_within_bounds(&s.tables))
}

/// Admits the tables a reader finds, all sheets of one workbook together: the budgets are the
/// workbook's, a table that breaks a bound or shares a cell with an earlier one is turned away, and a
/// name that is taken gets a suffix.
#[derive(Debug, Default)]
pub struct TableRegistry {
    seen: usize,
    columns: usize,
    names: HashSet<String>,
    /// The table parts already read: a part is one table, so a second listing of it is turned away unread.
    parts: HashSet<String>,
}

impl TableRegistry {
    /// A registry whose names avoid `taken` (the defined names of the workbook).
    pub fn new<'a>(taken: impl IntoIterator<Item = &'a str>) -> Self {
        Self {
            names: taken.into_iter().map(fold_name).collect(),
            ..Self::default()
        }
    }

    /// Whether `path` (a table part) has not been read yet; it is marked as read.
    pub fn claim_part(&mut self, path: &str) -> bool {
        self.parts.insert(path.to_string())
    }

    /// Counts a table that was turned away before it could be built (a range on no sheet, a part that could
    /// not be read) against the budget, so a flood of them is refused too.
    pub fn note_turned_away(&mut self) -> Result<(), EasyOfficeError> {
        self.count_attempt(0)
    }

    fn count_attempt(&mut self, columns: usize) -> Result<(), EasyOfficeError> {
        self.seen += 1;
        self.columns = self.columns.saturating_add(columns);
        if self.seen > MAX_TABLES || self.columns > MAX_TABLE_COLUMNS_TOTAL {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "표 개수 또는 열 개수가 허용 상한({MAX_TABLES}개, {MAX_TABLE_COLUMNS_TOTAL}열)을 초과했습니다."
            )));
        }
        Ok(())
    }

    /// Puts `table` into `sheet_tables`. `Ok(false)` when it is turned away; an error when the workbook
    /// lists more tables or columns than the budgets allow, which refuses the whole file.
    pub fn admit(
        &mut self,
        sheet_tables: &mut Vec<SheetTable>,
        mut table: SheetTable,
    ) -> Result<bool, EasyOfficeError> {
        self.count_attempt(table.columns.len())?;
        if !table.within_bounds() || sheet_tables.iter().any(|t| t.overlaps(&table.range)) {
            return Ok(false);
        }
        table.name = unique_name(&mut self.names, &table.name);
        sheet_tables.push(table);
        Ok(true)
    }
}

/// The tables of every sheet with names that are unique in the workbook and different from every
/// defined name (`taken`), each in its own sheet's list; the writers use it so a file never has two
/// tables of one name.
pub fn unique_table_names<'a>(
    sheets: impl IntoIterator<Item = &'a [SheetTable]>,
    taken: impl IntoIterator<Item = &'a str>,
) -> Vec<Vec<SheetTable>> {
    let mut names: HashSet<String> = taken.into_iter().map(fold_name).collect();
    sheets
        .into_iter()
        .map(|tables| {
            tables
                .iter()
                .map(|t| SheetTable {
                    name: unique_name(&mut names, &t.name),
                    ..t.clone()
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(name: &str, range: &str) -> SheetTable {
        SheetTable::new(name, CellBox::parse(range).unwrap())
    }

    #[test]
    fn names_follow_the_defined_name_syntax_without_question_marks() {
        for ok in ["Table1", "세율표", "_t", "a.b", "Sales_2024"] {
            assert_eq!(table_name_problem(ok), None, "{ok}");
        }
        for bad in [
            "", "A1", "R1C1", "my table", "t?", "1abc", "a\"b", "<x>&", "TRUE",
        ] {
            assert!(table_name_problem(bad).is_some(), "{bad}");
        }
        assert!(table_name_problem(&"n".repeat(256)).is_some());
    }

    #[test]
    fn bounds_cover_rows_columns_and_text() {
        assert!(table("T", "A1:B2").within_bounds());
        assert!(
            !table("T", "A1:B1").within_bounds(),
            "header without a data row"
        );
        let mut headless = table("T", "A1:B1");
        headless.header = false;
        assert!(headless.within_bounds());
        let mut totals = table("T", "A1:B2");
        totals.totals = true;
        assert!(!totals.within_bounds());
        let mut few = table("T", "A1:C3");
        few.columns.pop();
        assert!(!few.within_bounds());
        let mut long = table("T", "A1:A2");
        long.columns[0] = "x".repeat(256);
        assert!(!long.within_bounds());
        let mut style = table("T", "A1:A2");
        style.style = Some("s".repeat(256));
        assert!(!style.within_bounds());
        let mut filtered = table("T", "A1:A2");
        filtered.header = false;
        filtered.filter = Some(Vec::new());
        assert!(!filtered.within_bounds(), "a filter needs a header row");
    }

    #[test]
    fn the_filter_range_leaves_out_the_totals_row() {
        let mut t = table("T", "B2:D9");
        assert_eq!(t.filter_range().to_a1(), "B2:D9");
        t.totals = true;
        assert_eq!(t.filter_range().to_a1(), "B2:D8");
    }

    #[test]
    fn unique_name_suffixes_and_ignores_case() {
        let mut taken = HashSet::new();
        assert_eq!(unique_name(&mut taken, "Sales"), "Sales");
        assert_eq!(unique_name(&mut taken, "SALES"), "SALES_2");
        assert_eq!(unique_name(&mut taken, "sales"), "sales_3");
        let long = "n".repeat(255);
        assert_eq!(unique_name(&mut taken, &long), long);
        assert_eq!(unique_name(&mut taken, &long).chars().count(), 255);
    }

    #[test]
    fn the_registry_turns_away_overlaps_and_renames_duplicates() {
        let mut registry = TableRegistry::new(["Total"]);
        let mut sheet = Vec::new();
        assert!(registry.admit(&mut sheet, table("A", "A1:B3")).unwrap());
        assert!(!registry.admit(&mut sheet, table("B", "B3:C5")).unwrap());
        assert!(registry.admit(&mut sheet, table("a", "D1:E3")).unwrap());
        assert!(registry.admit(&mut sheet, table("Total", "G1:H3")).unwrap());
        assert!(!registry.admit(&mut sheet, table("A1", "J1:K3")).unwrap());
        let names: Vec<&str> = sheet.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["A", "a_2", "Total_2"]);
    }

    #[test]
    fn a_flood_of_tables_refuses_the_file() {
        let mut registry = TableRegistry::default();
        let mut sheet = Vec::new();
        let mut result = Ok(true);
        for n in 0..=MAX_TABLES {
            // The first bound is hit by the attempt past the limit, valid or not.
            result = registry.admit(
                &mut sheet,
                table("bad name", &format!("A{}:A{}", 3 * n + 1, 3 * n + 2)),
            );
            if result.is_err() {
                break;
            }
        }
        assert!(matches!(
            result,
            Err(EasyOfficeError::ResourceBudgetExceeded(_))
        ));
        let mut wide = TableRegistry::default();
        let mut wide_sheet = Vec::new();
        let mut columns = table("W", "A1:XFD2");
        columns.name = "W".into();
        let mut refused = false;
        for n in 0..5 {
            columns.name = format!("W{n}x");
            refused |= wide.admit(&mut wide_sheet, columns.clone()).is_err();
        }
        assert!(refused, "5 x 16,384 columns is over the workbook budget");
    }

    #[test]
    fn unique_table_names_never_repeats_across_sheets() {
        let a = [table("T", "A1:A2")];
        let b = [table("t", "A1:A2"), table("Total", "C1:C2")];
        let out = unique_table_names([&a[..], &b[..]], ["total"]);
        assert_eq!(out[0][0].name, "T");
        assert_eq!(out[1][0].name, "t_2");
        assert_eq!(out[1][1].name, "Total_2");
        assert!(tables_within_bounds(&out[1]));
    }
}

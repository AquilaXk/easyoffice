//! Tables follow a row or column edit (PD-224): the range grows when rows or columns are inserted inside
//! it, shifts when they are inserted before it, shrinks when some of its rows or columns are deleted and
//! goes away when all of them are, or when its header row is, or when no data row is left. The names of
//! the columns and the conditions of the table's filter move with their columns; a column inserted inside
//! gets a name that is not taken.
//!
//! 참조 구현: Univer `packages/sheets-filter/src/controllers/sheets-filter.controller.ts` lines 268-518
//! (the same rules for the range of an auto filter: a range whose header row is deleted, or that loses all
//! its rows or columns, is removed; the conditions of deleted columns are removed), Apache-2.0, Copyright
//! 2023-present DreamNum Co., Ltd., commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9. The span arithmetic is
//! `AxisEdit::shift_span` (see the header of `axis/mod.rs`). The spreadsheet program keeps a table whose
//! totals row is deleted as a table without one; this port does the same.

use super::{shift_runs, Axis, AxisEdit, EditKind};
use easyoffice_formats::{fold_name, FilterColumn, SheetTable};
use std::collections::HashSet;

/// `table` after `edit`; `None` when the edit removes it.
pub fn shift_table(table: &SheetTable, edit: &AxisEdit) -> Option<SheetTable> {
    let mut out = table.clone();
    match edit.axis {
        Axis::Row => {
            let range = table.range;
            let deleted = |row: u32| {
                edit.kind == EditKind::Delete && (edit.at..edit.at + edit.count).contains(&row)
            };
            if table.header && deleted(range.row0) {
                return None;
            }
            (out.range.row0, out.range.row1) = edit.shift_span(range.row0, range.row1)?;
            out.totals = table.totals && !deleted(range.row1);
            out.filter_hidden = shift_runs(&table.filter_hidden, edit);
        }
        Axis::Col => {
            let range = table.range;
            let (start, end) = edit.shift_span(range.col0, range.col1)?;
            (out.range.col0, out.range.col1) = (start, end);
            let mut taken: HashSet<String> = HashSet::new();
            let mut names = Vec::with_capacity((end - start + 1) as usize);
            let mut inserted = Vec::new();
            for col in start..=end {
                let old = edit
                    .unmap_index(col)
                    .filter(|c| (range.col0..=range.col1).contains(c));
                match old {
                    Some(c) => {
                        let name = table.columns[(c - range.col0) as usize].clone();
                        taken.insert(fold_name(&name));
                        names.push(name);
                    }
                    None => {
                        inserted.push(names.len());
                        names.push(String::new());
                    }
                }
            }
            let mut next = 1usize;
            for at in inserted {
                let name = loop {
                    let candidate = format!("Column{next}");
                    next += 1;
                    if taken.insert(fold_name(&candidate)) {
                        break candidate;
                    }
                };
                names[at] = name;
            }
            out.columns = names;
            out.filter = table.filter.as_ref().map(|conditions| {
                conditions
                    .iter()
                    .filter_map(|c| {
                        let col = edit
                            .map_index(c.col)
                            .filter(|c| (start..=end).contains(c))?;
                        Some(FilterColumn {
                            col,
                            criteria: c.criteria.clone(),
                        })
                    })
                    .collect()
            });
        }
    }
    (out.range.row1 - out.range.row0 + 1 >= out.min_rows()).then_some(out)
}

/// `tables` after `edit`, the ones the edit removes left out.
pub fn shift_tables(tables: &[SheetTable], edit: &AxisEdit) -> Vec<SheetTable> {
    tables.iter().filter_map(|t| shift_table(t, edit)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use easyoffice_formats::{CellBox, FilterCriteria};

    fn edit(axis: Axis, kind: EditKind, at1: u32, count: u32) -> AxisEdit {
        AxisEdit::new(axis, kind, at1 - 1, count).unwrap()
    }

    fn sample() -> SheetTable {
        // B3:E10: header row 3, data rows 4-9, totals row 10.
        let mut t = SheetTable::new("T", CellBox::parse("B3:E10").unwrap());
        t.totals = true;
        t.columns = ["a", "b", "c", "d"].map(String::from).to_vec();
        t.filter = Some(vec![
            FilterColumn {
                col: 1,
                criteria: FilterCriteria::Values {
                    blank: false,
                    values: vec!["x".into()],
                    dates: Vec::new(),
                },
            },
            FilterColumn {
                col: 3,
                criteria: FilterCriteria::Values {
                    blank: true,
                    values: Vec::new(),
                    dates: Vec::new(),
                },
            },
        ]);
        t
    }

    fn range(t: &Option<SheetTable>) -> String {
        t.as_ref().map_or("gone".into(), |t| t.range.to_a1())
    }

    #[test]
    fn row_insertion_grows_inside_and_shifts_before() {
        let t = sample();
        assert_eq!(
            range(&shift_table(&t, &edit(Axis::Row, EditKind::Insert, 6, 2))),
            "B3:E12"
        );
        assert_eq!(
            range(&shift_table(&t, &edit(Axis::Row, EditKind::Insert, 4, 1))),
            "B3:E11"
        );
        assert_eq!(
            range(&shift_table(&t, &edit(Axis::Row, EditKind::Insert, 10, 1))),
            "B3:E11"
        );
        assert_eq!(
            range(&shift_table(&t, &edit(Axis::Row, EditKind::Insert, 3, 2))),
            "B5:E12"
        );
        assert_eq!(
            range(&shift_table(&t, &edit(Axis::Row, EditKind::Insert, 11, 2))),
            "B3:E10"
        );
        assert_eq!(
            range(&shift_table(&t, &edit(Axis::Row, EditKind::Insert, 1, 5))),
            "B8:E15"
        );
    }

    #[test]
    fn row_deletion_shrinks_and_drops_the_header_or_the_totals() {
        let t = sample();
        assert_eq!(
            range(&shift_table(&t, &edit(Axis::Row, EditKind::Delete, 5, 2))),
            "B3:E8"
        );
        assert_eq!(
            range(&shift_table(&t, &edit(Axis::Row, EditKind::Delete, 1, 2))),
            "B1:E8"
        );
        // The totals row goes: a table without one.
        let no_totals = shift_table(&t, &edit(Axis::Row, EditKind::Delete, 10, 1)).unwrap();
        assert_eq!(
            (no_totals.range.to_a1().as_str(), no_totals.totals),
            ("B3:E9", false)
        );
        // The header row goes: the table goes.
        assert!(shift_table(&t, &edit(Axis::Row, EditKind::Delete, 3, 1)).is_none());
        assert!(shift_table(&t, &edit(Axis::Row, EditKind::Delete, 2, 2)).is_none());
        // No data row is left.
        assert!(shift_table(&t, &edit(Axis::Row, EditKind::Delete, 4, 7)).is_none());
        assert!(shift_table(&t, &edit(Axis::Row, EditKind::Delete, 4, 6)).is_none());
        let mut headless = t.clone();
        headless.header = false;
        headless.totals = false;
        assert_eq!(
            range(&shift_table(
                &headless,
                &edit(Axis::Row, EditKind::Delete, 3, 7)
            )),
            "B3:E3"
        );
        assert!(shift_table(&headless, &edit(Axis::Row, EditKind::Delete, 3, 8)).is_none());
    }

    #[test]
    fn column_edits_move_names_and_conditions() {
        let t = sample();
        let inserted = shift_table(&t, &edit(Axis::Col, EditKind::Insert, 4, 1)).unwrap();
        assert_eq!(inserted.range.to_a1(), "B3:F10");
        assert_eq!(inserted.columns, ["a", "b", "Column1", "c", "d"]);
        let cols: Vec<u32> = inserted.filter.unwrap().iter().map(|c| c.col).collect();
        assert_eq!(cols, [1, 4]);
        // Before the table: shifts; right after it: unchanged.
        assert_eq!(
            range(&shift_table(&t, &edit(Axis::Col, EditKind::Insert, 2, 3))),
            "E3:H10"
        );
        assert_eq!(
            range(&shift_table(&t, &edit(Axis::Col, EditKind::Insert, 6, 3))),
            "B3:E10"
        );
        let deleted = shift_table(&t, &edit(Axis::Col, EditKind::Delete, 4, 1)).unwrap();
        assert_eq!(deleted.range.to_a1(), "B3:D10");
        assert_eq!(deleted.columns, ["a", "b", "d"]);
        // The condition of the deleted column D is gone, the one of B stays.
        let cols: Vec<u32> = deleted.filter.unwrap().iter().map(|c| c.col).collect();
        assert_eq!(cols, [1]);
        let after = shift_table(&t, &edit(Axis::Col, EditKind::Delete, 3, 1)).unwrap();
        let cols: Vec<u32> = after.filter.unwrap().iter().map(|c| c.col).collect();
        assert_eq!(cols, [1, 2]);
        let first_gone = shift_table(&t, &edit(Axis::Col, EditKind::Delete, 2, 1)).unwrap();
        assert_eq!(first_gone.columns, ["b", "c", "d"]);
        assert!(shift_table(&t, &edit(Axis::Col, EditKind::Delete, 2, 4)).is_none());
    }

    #[test]
    fn an_inserted_column_name_avoids_the_taken_ones() {
        let mut t = sample();
        t.columns = ["Column1", "Column2", "x", "y"].map(String::from).to_vec();
        let out = shift_table(&t, &edit(Axis::Col, EditKind::Insert, 4, 2)).unwrap();
        assert_eq!(
            out.columns,
            ["Column1", "Column2", "Column3", "Column4", "x", "y"]
        );
    }
}

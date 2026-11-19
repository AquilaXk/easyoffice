use super::*;

fn grid(rows: usize, cols: usize) -> Vec<Vec<String>> {
    vec![vec![String::new(); cols]; rows]
}

fn merge(row: u32, col: u32, row_span: u32, col_span: u32) -> CellMerge {
    CellMerge {
        row,
        col,
        row_span,
        col_span,
    }
}

#[test]
fn valid_regions_are_kept_sorted() {
    let (t, dropped) =
        RichTable::with_merges(grid(4, 4), vec![merge(2, 2, 2, 2), merge(0, 0, 1, 3)]);
    assert_eq!(dropped, 0);
    assert_eq!(t.merges, vec![merge(0, 0, 1, 3), merge(2, 2, 2, 2)]);
}

#[test]
fn overlapping_out_of_range_and_huge_regions_are_dropped() {
    let (t, dropped) = RichTable::with_merges(
        grid(3, 3),
        vec![
            merge(0, 0, 2, 2),
            merge(1, 1, 2, 2),
            merge(2, 2, 2, 1),
            merge(0, 2, 1, 2),
            merge(0, 0, u32::MAX, u32::MAX),
            merge(u32::MAX, u32::MAX, 2, 2),
            merge(1, 0, 0, 2),
            merge(2, 0, 1, 1),
        ],
    );
    assert_eq!(t.merges, vec![merge(0, 0, 2, 2)]);
    assert_eq!(dropped, 6);
}

#[test]
fn ragged_rows_reject_regions_over_missing_cells() {
    let rows = vec![vec!["a".into(), "b".into()], vec!["c".into()]];
    let (t, dropped) = RichTable::with_merges(rows, vec![merge(0, 0, 2, 2), merge(0, 0, 1, 2)]);
    assert_eq!(t.merges, vec![merge(0, 0, 1, 2)]);
    assert_eq!(dropped, 1);
}

#[test]
fn grid_over_the_limit_drops_every_region() {
    let (t, dropped) = RichTable::with_merges(grid(2_001, 2_001), vec![merge(0, 0, 2, 2)]);
    assert!(t.merges.is_empty());
    assert_eq!(dropped, 1);
}

#[test]
fn many_large_overlapping_regions_stay_within_the_work_budget() {
    let rows = 2_000usize;
    let candidates = vec![merge(0, 0, 2_000, 1_000); 200_000];
    let started = std::time::Instant::now();
    let (t, dropped) = RichTable::with_merges(grid(rows, 1_000), candidates);
    assert_eq!(t.merges.len(), 1);
    assert_eq!(dropped, 199_999);
    assert!(started.elapsed().as_secs() < 5);
}

#[test]
fn span_map_marks_anchor_and_covered_cells() {
    let (t, _) = RichTable::with_merges(grid(3, 3), vec![merge(0, 1, 2, 2)]);
    let map = t.span_map().expect("merged");
    assert_eq!(map.at(0, 0), CellSpan::Plain);
    assert_eq!(
        map.at(0, 1),
        CellSpan::Anchor {
            row_span: 2,
            col_span: 2
        }
    );
    assert_eq!(
        map.at(1, 2),
        CellSpan::Covered {
            row_offset: 1,
            col_offset: 1
        }
    );
    assert_eq!(map.at(2, 2), CellSpan::Plain);
    assert_eq!(map.at(9, 9), CellSpan::Plain);
    assert!(RichTable::new(grid(2, 2)).span_map().is_none());
}

#[test]
fn stale_regions_are_revalidated_by_span_map() {
    let mut t = RichTable::with_merges(grid(3, 3), vec![merge(1, 1, 2, 2)]).0;
    t.rows.truncate(2);
    assert!(t.span_map().is_none());
}

#[test]
fn assembler_pads_column_spans_and_continues_vertical_regions() {
    let mut b = DecodeBudget::default();
    let mut a = TableAssembler::default();
    a.start_row();
    a.spanning("가".into(), 2, &mut b).unwrap();
    a.restart("나".into(), 1, &mut b).unwrap();
    a.end_row();
    a.start_row();
    a.cell("x".into(), &mut b).unwrap();
    a.cell("y".into(), &mut b).unwrap();
    a.continue_above(String::new(), 1, &mut b).unwrap();
    a.end_row();
    let (t, dropped) = a.finish(&mut b).unwrap();
    assert_eq!(dropped, 0);
    assert_eq!(t.rows[0].len(), 3);
    assert_eq!(t.merges, vec![merge(0, 0, 1, 2), merge(0, 2, 2, 1)]);
}

#[test]
fn assembler_ignores_orphan_continuations_and_huge_spans() {
    let mut b = DecodeBudget::default();
    let mut a = TableAssembler::default();
    a.start_row();
    a.continue_above(String::new(), 1, &mut b).unwrap();
    a.spanning("a".into(), u32::MAX, &mut b).unwrap();
    a.anchor("b".into(), u32::MAX, 1, &mut b).unwrap();
    a.end_row();
    let (t, dropped) = a.finish(&mut b).unwrap();
    assert_eq!(t.rows[0].len(), 3);
    assert!(t.merges.is_empty());
    assert_eq!(dropped, 2);
}

#[test]
fn placed_cells_make_a_grid_and_drop_duplicates_and_far_cells() {
    let mut b = DecodeBudget::default();
    let cell = |row, col, row_span, col_span, text: &str| PlacedCell {
        row,
        col,
        row_span,
        col_span,
        text: text.to_string(),
        links: Vec::new(),
        width_twips: None,
        style: (CellFill::none(), TableCellBorders::default()),
        images: Vec::new(),
    };
    let (t, dropped) = table_from_placed(
        vec![
            cell(0, 0, 1, 2, "a"),
            cell(0, 2, 2, 1, "b"),
            cell(1, 0, 1, 1, "c"),
            cell(1, 0, 1, 1, "dup"),
            cell(0, u32::MAX, 1, 1, "far"),
            cell(9_999_999, 0, 1, 1, "tall"),
        ],
        &mut b,
    )
    .unwrap();
    assert_eq!(t.rows.len(), 2);
    assert_eq!(t.rows[0], vec!["a", "", "b"]);
    assert_eq!(t.rows[1][0], "c");
    assert_eq!(t.merges, vec![merge(0, 0, 1, 2), merge(0, 2, 2, 1)]);
    assert_eq!(dropped, 3);
}

#[test]
fn validated_col_widths_and_cell_styles_dedup_last_wins() {
    let mut t = RichTable::new(grid(2, 2));
    t.col_widths_twips = vec![10, 50_000];
    assert_eq!(
        t.validated_col_widths(),
        Some(vec![MIN_COL_WIDTH_TWIPS, MAX_COL_WIDTH_TWIPS])
    );
    t.col_widths_twips = vec![1440];
    assert_eq!(t.validated_col_widths(), None);

    let first = TableCellStyleEntry {
        row: 0,
        col: 1,
        fill: CellFill::solid(0x112233),
        borders: TableCellBorders::default(),
    };
    let second = TableCellStyleEntry {
        row: 0,
        col: 1,
        fill: CellFill::solid(0x445566),
        borders: TableCellBorders::default(),
    };
    let oob = TableCellStyleEntry {
        row: 5,
        col: 0,
        fill: CellFill::solid(0xABCDEF),
        borders: TableCellBorders::default(),
    };
    let empty = TableCellStyleEntry {
        row: 0,
        col: 0,
        fill: CellFill::none(),
        borders: TableCellBorders::default(),
    };
    t.cell_styles = vec![first, oob, empty, second];
    assert_eq!(t.validated_cell_styles(), vec![second]);
}

//! The auto filter of a sheet and of a table survives an ODS round trip: the conditions in
//! `table:filter`, the rows it hides as `table:visibility="filter"` (PD-226).

use easyoffice_formats::{
    CellBox, CustomCondition, CustomOp, FilterColumn, FilterCriteria, OdfCodec, SheetFilter,
    SheetStyling, SheetTable,
};

fn cells(rows: u32) -> Vec<(u32, u32, String)> {
    (0..rows)
        .flat_map(|r| (0..3).map(move |c| (r, c, format!("{r}x{c}"))))
        .collect()
}

fn pack(styling: SheetStyling, rows: u32) -> Vec<u8> {
    let sheets = vec![("Data".to_string(), cells(rows))];
    OdfCodec::pack_ods_multi_styled(&sheets, &[styling]).unwrap()
}

fn read(bytes: &[u8]) -> SheetStyling {
    OdfCodec::unpack_ods_with_styling(bytes)
        .unwrap()
        .1
        .remove(0)
        .1
}

fn conditions() -> Vec<FilterColumn> {
    vec![
        FilterColumn {
            col: 0,
            criteria: FilterCriteria::Values {
                blank: true,
                values: vec!["영업".into(), "개발".into()],
                dates: Vec::new(),
            },
        },
        FilterColumn {
            col: 2,
            criteria: FilterCriteria::Custom {
                and: false,
                first: CustomCondition {
                    op: Some(CustomOp::LessThan),
                    text: "5".into(),
                },
                second: Some(CustomCondition {
                    op: None,
                    text: "9*".into(),
                }),
            },
        },
    ]
}

#[test]
fn the_sheet_filter_with_its_conditions_and_hidden_rows_comes_back() {
    let mut filter = SheetFilter::new(CellBox::parse("A1:C20").unwrap());
    filter.columns = conditions();
    filter.hidden = vec![(3, 5), (9, 9)];
    let styling = SheetStyling {
        filter: Some(filter.clone()),
        ..SheetStyling::default()
    };
    let back = read(&pack(styling, 20));
    assert_eq!(back.filter, Some(filter));
    assert!(back.hidden_rows.is_empty() && back.tables.is_empty());
}

#[test]
fn a_filter_without_conditions_and_rows_outside_any_filter() {
    // A filtered row outside every filter's data rows is a hidden row of the user.
    let mut filter = SheetFilter::new(CellBox::parse("A1:C5").unwrap());
    filter.hidden = vec![(2, 2)];
    let styling = SheetStyling {
        filter: Some(filter.clone()),
        ..SheetStyling::default()
    };
    let back = read(&pack(styling, 5));
    assert_eq!(back.filter, Some(filter));
}

#[test]
fn a_table_keeps_its_conditions_and_hidden_rows_beside_a_sheet_filter() {
    let mut table = SheetTable::new("Sales", CellBox::parse("A10:C20").unwrap());
    table.columns = vec!["a".into(), "b".into(), "c".into()];
    table.filter = Some(conditions());
    table.filter_hidden = vec![(12, 14)];
    let mut filter = SheetFilter::new(CellBox::parse("A1:C8").unwrap());
    filter.columns = conditions();
    filter.hidden = vec![(2, 3)];
    let styling = SheetStyling {
        filter: Some(filter.clone()),
        tables: vec![table.clone()],
        ..SheetStyling::default()
    };
    let back = read(&pack(styling, 20));
    assert_eq!(back.filter, Some(filter));
    assert_eq!(back.tables.len(), 1);
    assert_eq!(back.tables[0].filter, table.filter);
    assert_eq!(back.tables[0].filter_hidden, table.filter_hidden);
}

#[test]
fn columns_the_format_cannot_hold_are_left_out_and_their_rows_stay_hidden() {
    let mut filter = SheetFilter::new(CellBox::parse("A1:C9").unwrap());
    filter.columns = vec![
        FilterColumn {
            col: 0,
            criteria: FilterCriteria::Average { above: true },
        },
        FilterColumn {
            col: 1,
            criteria: FilterCriteria::Top {
                bottom: false,
                percent: false,
                count: 3,
            },
        },
    ];
    filter.hidden = vec![(2, 4)];
    let styling = SheetStyling {
        filter: Some(filter),
        ..SheetStyling::default()
    };
    let back = read(&pack(styling, 9)).filter.unwrap();
    assert_eq!(back.columns.len(), 1);
    assert_eq!(back.hidden, vec![(2, 4)]);
}

#[test]
fn a_filter_over_the_bounds_is_refused_on_write() {
    let mut filter = SheetFilter::new(CellBox::parse("A1:C300000").unwrap());
    filter.hidden = Vec::new();
    let styling = SheetStyling {
        filter: Some(filter),
        ..SheetStyling::default()
    };
    let sheets = vec![("Data".to_string(), cells(3))];
    assert!(OdfCodec::pack_ods_multi_styled(&sheets, &[styling]).is_err());
}

#[test]
fn a_file_with_a_filter_range_over_a_table_keeps_the_table_and_drops_the_filter() {
    let bytes = pack(
        SheetStyling {
            tables: vec![SheetTable::new("T", CellBox::parse("A1:C5").unwrap())],
            ..SheetStyling::default()
        },
        5,
    );
    // Rewrite the table's range as a sheet filter's range on the same cells.
    let entries = easyoffice_formats::ZipArchiveCodec::unpack(
        &bytes,
        &easyoffice_formats::ZipSecurityPolicy::default(),
    )
    .unwrap();
    let content = entries.iter().find(|e| e.path == "content.xml").unwrap();
    let xml = String::from_utf8(content.data.clone()).unwrap();
    let with_filter = xml.replace(
        "</table:database-ranges>",
        "<table:database-range table:name=\"__Anonymous_Sheet_DB__0\" table:target-range-address=\"Data.A1:Data.C5\" table:display-filter-buttons=\"true\"/></table:database-ranges>",
    );
    assert_ne!(xml, with_filter);
    let (styled, _) = (
        OdfCodec::unpack_ods_with_styling(&repack(&bytes, &with_filter))
            .unwrap()
            .1,
        (),
    );
    assert!(styled[0].1.filter.is_none());
    assert_eq!(styled[0].1.tables.len(), 1);
}

fn repack(original: &[u8], content: &str) -> Vec<u8> {
    let entries = easyoffice_formats::ZipArchiveCodec::unpack(
        original,
        &easyoffice_formats::ZipSecurityPolicy::default(),
    )
    .unwrap();
    let parts: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            let data = if e.path == "content.xml" {
                content.as_bytes().to_vec()
            } else {
                e.data
            };
            (e.path, data)
        })
        .collect();
    easyoffice_formats::ZipArchiveCodec::pack_compressed(&parts).unwrap()
}

/// 300,000 pairs of a filtered row and a shown row, appended to the first table of a small file.
fn many_filtered_rows() -> Vec<u8> {
    let bytes = pack(SheetStyling::default(), 3);
    let entries = easyoffice_formats::ZipArchiveCodec::unpack(
        &bytes,
        &easyoffice_formats::ZipSecurityPolicy::default(),
    )
    .unwrap();
    let content = entries.iter().find(|e| e.path == "content.xml").unwrap();
    let xml = String::from_utf8(content.data.clone()).unwrap();
    let pairs = "<table:table-row table:visibility=\"filter\"/><table:table-row/>".repeat(300_000);
    repack(
        &bytes,
        &xml.replacen("</table:table>", &format!("{pairs}</table:table>"), 1),
    )
}

#[test]
fn a_flood_of_filtered_rows_is_read_in_linear_time_and_bounded() {
    let bytes = many_filtered_rows();
    let start = std::time::Instant::now();
    let back = read(&bytes);
    let elapsed = start.elapsed();
    println!("300,000 filtered rows read in {elapsed:?}");
    // No filter owns them: they are the user's hidden rows, up to the bound the reader keeps.
    assert!(back.filter.is_none());
    let hidden: u64 = back
        .hidden_rows
        .iter()
        .map(|r| u64::from(r.1 - r.0) + 1)
        .sum();
    assert!((300_000..=300_001).contains(&hidden), "{hidden}");
    assert!(elapsed.as_secs_f64() < 5.0, "{elapsed:?}");
}

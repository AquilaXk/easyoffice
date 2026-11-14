//! The auto filter survives an XLSX round trip; the rows it hides are written as hidden rows
//! beside the user's (PD-198).

use easyoffice_formats::{
    CellBox, CustomCondition, CustomOp, DateGroup, DateUnit, FilterColumn, FilterCriteria,
    OoxmlCodec, SheetFilter, SheetStyling, SheetTable, ZipArchiveCodec, ZipSecurityPolicy,
};

fn sample() -> SheetStyling {
    let mut filter = SheetFilter::new(CellBox::parse("A1:C20").unwrap());
    filter.columns = vec![
        FilterColumn {
            col: 0,
            criteria: FilterCriteria::Values {
                blank: false,
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
    ];
    filter.hidden = vec![(3, 5), (9, 9)];
    SheetStyling {
        hidden_rows: vec![(6, 7), (30, 30)],
        merges: vec![CellBox::parse("E1:F2").unwrap()],
        filter: Some(filter),
        ..SheetStyling::default()
    }
}

fn pack(styling: SheetStyling) -> Vec<u8> {
    let sheets = vec![("Sheet1".to_string(), vec![(0, 0, "1".to_string())])];
    OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[styling]).unwrap()
}

fn read(bytes: &[u8]) -> SheetStyling {
    OoxmlCodec::unpack_xlsx_with_styling(bytes)
        .unwrap()
        .1
        .remove(0)
        .1
}

#[test]
fn the_filter_and_every_hidden_row_come_back() {
    let styling = sample();
    let back = read(&pack(styling.clone()));
    // The file has one `hidden` flag per row, so the reader cannot tell the two kinds apart.
    assert_eq!(back.hidden_rows, vec![(3, 7), (9, 9), (30, 30)]);
    let filter = back.filter.expect("autoFilter");
    let wanted = styling.filter.unwrap();
    assert_eq!(filter.range, wanted.range);
    assert_eq!(filter.columns, wanted.columns);
    assert!(filter.hidden.is_empty());
    assert_eq!(back.merges, styling.merges);
}

#[test]
fn the_element_sits_between_the_cells_and_the_merges() {
    let bytes = pack(sample());
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let sheet = entries
        .iter()
        .find(|e| e.path == "xl/worksheets/sheet1.xml")
        .unwrap();
    let xml = String::from_utf8(sheet.data.clone()).unwrap();
    let at = |needle: &str| {
        xml.find(needle)
            .unwrap_or_else(|| panic!("{needle} in {xml}"))
    };
    assert!(at("</sheetData>") < at("<autoFilter ref=\"A1:C20\">"));
    assert!(at("<autoFilter") < at("<mergeCells"));
    assert!(
        xml.contains("<customFilter operator=\"lessThan\" val=\"5\"/><customFilter val=\"9*\"/>")
    );
    assert_eq!(xml.matches(" hidden=\"1\"").count(), 7);
}

#[test]
fn a_filter_without_conditions_round_trips() {
    let styling = SheetStyling {
        filter: Some(SheetFilter::new(CellBox::parse("B2:D9").unwrap())),
        ..SheetStyling::default()
    };
    let back = read(&pack(styling.clone()));
    assert_eq!(back.filter, styling.filter);
    assert!(back.hidden_rows.is_empty());
}

#[test]
fn a_filter_over_the_bounds_is_refused_on_write() {
    let mut styling = sample();
    styling.filter.as_mut().unwrap().range = CellBox::parse("A1:C300000").unwrap();
    let sheets = vec![("Sheet1".to_string(), vec![(0, 0, "1".to_string())])];
    assert!(OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[styling]).is_err());
}

fn every_kind() -> Vec<FilterColumn> {
    let on = |col, criteria| FilterColumn { col, criteria };
    vec![
        on(
            0,
            FilterCriteria::Values {
                blank: true,
                values: vec!["n/a".into()],
                dates: vec![
                    DateGroup {
                        unit: DateUnit::Month,
                        year: 2024,
                        month: 3,
                        day: 0,
                        hour: 0,
                        minute: 0,
                        second: 0,
                    },
                    DateGroup {
                        unit: DateUnit::Year,
                        year: 2023,
                        month: 0,
                        day: 0,
                        hour: 0,
                        minute: 0,
                        second: 0,
                    },
                ],
            },
        ),
        on(
            1,
            FilterCriteria::Top {
                bottom: false,
                percent: false,
                count: 10,
            },
        ),
        on(
            2,
            FilterCriteria::Top {
                bottom: true,
                percent: true,
                count: 20,
            },
        ),
        on(3, FilterCriteria::Average { above: true }),
        on(4, FilterCriteria::Average { above: false }),
        on(
            5,
            FilterCriteria::Color {
                font: false,
                color: Some(0xFFC7CE),
            },
        ),
        on(
            6,
            FilterCriteria::Color {
                font: true,
                color: Some(0x9C0006),
            },
        ),
        on(
            7,
            FilterCriteria::Color {
                font: false,
                color: None,
            },
        ),
    ]
}

#[test]
fn date_groups_top_average_and_colour_filters_round_trip_through_the_package() {
    let mut filter = SheetFilter::new(CellBox::parse("A1:H20").unwrap());
    filter.columns = every_kind();
    let styling = SheetStyling {
        filter: Some(filter.clone()),
        ..SheetStyling::default()
    };
    let bytes = pack(styling);
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let styles = entries.iter().find(|e| e.path == "xl/styles.xml").unwrap();
    let styles = String::from_utf8(styles.data.clone()).unwrap();
    // The colour filters name three differential formats; the no-fill one is a fill with no colour.
    assert!(styles.contains("<dxfs count=\"3\">"), "{styles}");
    assert!(styles.contains("<fill><patternFill patternType=\"none\"><bgColor auto=\"1\"/>"));
    let back = read(&bytes).filter.expect("autoFilter");
    assert_eq!(back.columns, filter.columns);
}

#[test]
fn a_table_keeps_and_writes_the_same_kinds_of_conditions() {
    let mut table = SheetTable::new("Sales", CellBox::parse("A1:H20").unwrap());
    table.filter = Some(every_kind());
    let styling = SheetStyling {
        tables: vec![table.clone()],
        ..SheetStyling::default()
    };
    let back = read(&pack(styling));
    assert_eq!(back.tables.len(), 1);
    assert_eq!(back.tables[0].filter, table.filter);
}

#[test]
fn rows_a_table_filter_hides_are_written_hidden() {
    let mut table = SheetTable::new("T", CellBox::parse("A1:B10").unwrap());
    table.filter = Some(vec![FilterColumn {
        col: 0,
        criteria: FilterCriteria::Values {
            blank: false,
            values: vec!["x".into()],
            dates: Vec::new(),
        },
    }]);
    table.filter_hidden = vec![(2, 3), (7, 7)];
    let styling = SheetStyling {
        hidden_rows: vec![(5, 5)],
        tables: vec![table],
        ..SheetStyling::default()
    };
    let back = read(&pack(styling));
    // The file has one `hidden` flag per row: the rows of both kinds come back as the user's.
    assert_eq!(back.hidden_rows, vec![(2, 3), (5, 5), (7, 7)]);
}

//! The auto filter survives an XLSX round trip; the rows it hides are written as hidden rows
//! beside the user's (PD-198).

use easyoffice_formats::{
    CellBox, CustomCondition, CustomOp, FilterColumn, FilterCriteria, OoxmlCodec, SheetFilter,
    SheetStyling, ZipArchiveCodec, ZipSecurityPolicy,
};

fn sample() -> SheetStyling {
    let mut filter = SheetFilter::new(CellBox::parse("A1:C20").unwrap());
    filter.columns = vec![
        FilterColumn {
            col: 0,
            criteria: FilterCriteria::Values {
                blank: false,
                values: vec!["영업".into(), "개발".into()],
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

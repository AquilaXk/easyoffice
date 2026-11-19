//! PD-255: Bidirectional XLSX `xl/pivotCache/` & `xl/pivotTables/` round-trip,
//! shared pivot cache deduplication, external SpreadsheetML & `[MS-XLSX]` pivot slicer import,
//! fail-closed resource budgets, and O(1) cell-index release benchmark.

use std::time::Instant;

use easyoffice_formats::{
    CellBox, EasyOfficeError, OdsUnpacked, OoxmlCodec, PivotAggKind, PivotFilterCriteria,
    PivotSlicer, PivotValueField, SheetPivotTable, SheetStyling, SheetTable, TableSlicer,
    ZipArchiveCodec, ZipSecurityPolicy, MAX_PIVOTS_PER_SHEET, MAX_PIVOT_AXIS_FIELDS,
    MAX_PIVOT_FILTER_VALUES, MAX_PIVOT_SLICERS, MAX_PIVOT_TEXT_BYTES, MAX_PIVOT_VALUE_FIELDS,
    MAX_WORKBOOK_PIVOTS,
};

fn sales_sheets() -> Vec<OdsUnpacked> {
    vec![
        (
            "데이터".to_string(),
            vec![
                (0, 0, "지역".to_string()),
                (0, 1, "분기".to_string()),
                (0, 2, "카테고리".to_string()),
                (0, 3, "매출".to_string()),
                (0, 4, "수량".to_string()),
                (1, 0, "서울".to_string()),
                (1, 1, "1Q".to_string()),
                (1, 2, "전자".to_string()),
                (1, 3, "1200".to_string()),
                (1, 4, "10".to_string()),
                (2, 0, "서울".to_string()),
                (2, 1, "2Q".to_string()),
                (2, 2, "가구".to_string()),
                (2, 3, "800".to_string()),
                (2, 4, "5".to_string()),
                (3, 0, "부산".to_string()),
                (3, 1, "1Q".to_string()),
                (3, 2, "전자".to_string()),
                (3, 3, "900".to_string()),
                (3, 4, "7".to_string()),
                (4, 0, "부산".to_string()),
                (4, 1, "2Q".to_string()),
                (4, 2, "전자".to_string()),
                (4, 3, "1100".to_string()),
                (4, 4, "9".to_string()),
                (5, 0, "대구".to_string()),
                (5, 1, "1Q".to_string()),
                (5, 2, "가구".to_string()),
                (5, 3, "450".to_string()),
                (5, 4, "3".to_string()),
            ],
        ),
        ("요약".to_string(), vec![(0, 0, "대시보드".to_string())]),
        (
            "보조".to_string(),
            vec![
                (0, 0, "팀".to_string()),
                (0, 1, "비용".to_string()),
                (1, 0, "개발".to_string()),
                (1, 1, "300".to_string()),
                (2, 0, "기획".to_string()),
                (2, 1, "150".to_string()),
            ],
        ),
    ]
}

fn sample_pivot(name: &str, cache_id: u32, loc: &str) -> SheetPivotTable {
    SheetPivotTable {
        name: name.to_string(),
        cache_id,
        source_sheet: "데이터".to_string(),
        source_range: "A1:E6".to_string(),
        source_table: Some("매출표".to_string()),
        has_header: true,
        location: CellBox::parse(loc).unwrap(),
        field_names: vec![
            "지역".to_string(),
            "분기".to_string(),
            "카테고리".to_string(),
            "매출".to_string(),
            "수량".to_string(),
        ],
        row_fields: vec![0, 2],
        col_fields: vec![1],
        filter_fields: vec![],
        value_fields: vec![
            PivotValueField {
                col: 3,
                agg: PivotAggKind::Sum,
                name: "합계:매출".to_string(),
            },
            PivotValueField {
                col: 4,
                agg: PivotAggKind::Average,
                name: "평균:수량".to_string(),
            },
        ],
        filters: vec![PivotFilterCriteria {
            col: 0,
            values: vec!["서울".to_string(), "부산".to_string()],
            blank: false,
        }],
        slicers: vec![PivotSlicer {
            col: 0,
            caption: "지역 슬라이서".to_string(),
            left: 420,
            top: 24,
            width: 192,
            height: 220,
            multi_select: true,
        }],
    }
}

#[test]
fn shared_pivot_cache_multi_pivot_xlsx_round_trip() {
    let sheets = sales_sheets();
    let mut data_st = SheetStyling::default();
    let mut tbl = SheetTable::new("매출표".to_string(), CellBox::parse("A1:E6").unwrap());
    tbl.columns = vec![
        "지역".to_string(),
        "분기".to_string(),
        "카테고리".to_string(),
        "매출".to_string(),
        "수량".to_string(),
    ];
    tbl.slicers.push(TableSlicer {
        col: 0,
        caption: "표 슬라이서".to_string(),
        left: 20,
        top: 20,
        width: 180,
        height: 200,
        multi_select: false,
    });
    data_st.tables.push(tbl);

    let mut summary_st = SheetStyling::default();
    let p1 = sample_pivot("피벗 테이블1", 1, "B2:F10");
    let mut p2 = sample_pivot("피벗 테이블2", 99, "B14:E20");
    p2.row_fields = vec![2];
    p2.col_fields = vec![];
    p2.filter_fields = vec![0];
    p2.value_fields = vec![
        PivotValueField {
            col: 3,
            agg: PivotAggKind::Min,
            name: "최소 매출".to_string(),
        },
        PivotValueField {
            col: 3,
            agg: PivotAggKind::Max,
            name: "최대 매출".to_string(),
        },
        PivotValueField {
            col: 4,
            agg: PivotAggKind::Count,
            name: "건수".to_string(),
        },
    ];
    p2.filters = vec![PivotFilterCriteria {
        col: 2,
        values: vec!["전자".to_string()],
        blank: true,
    }];
    p2.slicers.clear();

    let p3 = SheetPivotTable {
        name: "보조 피벗".to_string(),
        cache_id: 1,
        source_sheet: "보조".to_string(),
        source_range: "A1:B3".to_string(),
        source_table: None,
        has_header: false,
        location: CellBox::parse("H2:J6").unwrap(),
        field_names: vec!["팀".to_string(), "비용".to_string()],
        row_fields: vec![0],
        col_fields: vec![],
        filter_fields: vec![],
        value_fields: vec![PivotValueField {
            col: 1,
            agg: PivotAggKind::Sum,
            name: "비용 합계".to_string(),
        }],
        filters: vec![],
        slicers: vec![],
    };
    summary_st.pivots = vec![p1.clone(), p2.clone(), p3.clone()];

    let bytes = OoxmlCodec::pack_xlsx_multi_styled(
        &sheets,
        &[],
        &[data_st, summary_st, SheetStyling::default()],
    )
    .expect("pack xlsx with shared pivot cache");

    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let part = |path: &str| -> String {
        let e = entries
            .iter()
            .find(|e| e.path == path)
            .unwrap_or_else(|| panic!("missing ZIP entry {path}"));
        std::str::from_utf8(&e.data).unwrap().to_string()
    };

    // Workbook XML schema order: sheets -> definedNames -> calcPr -> pivotCaches -> extLst
    let wb_xml = part("xl/workbook.xml");
    let calc_pos = wb_xml.find("<calcPr").expect("calcPr present");
    let pc_pos = wb_xml.find("<pivotCaches>").expect("pivotCaches present");
    let ext_pos = wb_xml.find("<extLst>").expect("slicer extLst present");
    assert!(calc_pos < pc_pos && pc_pos < ext_pos, "{wb_xml}");

    // Two distinct source specs => exactly 2 caches, NOT 3
    assert!(entries
        .iter()
        .any(|e| e.path == "xl/pivotCache/pivotCacheDefinition1.xml"));
    assert!(entries
        .iter()
        .any(|e| e.path == "xl/pivotCache/pivotCacheRecords1.xml"));
    assert!(entries
        .iter()
        .any(|e| e.path == "xl/pivotCache/pivotCacheDefinition2.xml"));
    assert!(entries
        .iter()
        .any(|e| e.path == "xl/pivotCache/pivotCacheRecords2.xml"));
    assert!(!entries
        .iter()
        .any(|e| e.path == "xl/pivotCache/pivotCacheDefinition3.xml"));

    // Worksheet XML body must NOT contain `<pivotTables>` (only `.rels` links them)
    let sheet2 = part("xl/worksheets/sheet2.xml");
    assert!(!sheet2.contains("<pivotTables"), "{sheet2}");
    let sheet2_rels = part("xl/worksheets/_rels/sheet2.xml.rels");
    assert!(sheet2_rels.contains("relationships/pivotTable"));
    assert!(sheet2_rels.contains("../pivotTables/pivotTable1.xml"));
    assert!(sheet2_rels.contains("../pivotTables/pivotTable2.xml"));
    assert!(sheet2_rels.contains("../pivotTables/pivotTable3.xml"));

    let ct = part("[Content_Types].xml");
    assert!(ct.contains("pivotCacheDefinition+xml"));
    assert!(ct.contains("pivotCacheRecords+xml"));
    assert!(ct.contains("pivotTable+xml"));

    let pc1 = part("xl/pivotCache/pivotCacheDefinition1.xml");
    assert!(pc1.contains("recordCount=\"5\""));
    assert!(pc1.contains("r:id=\"rIdPCR1\""));
    assert!(pc1.contains("sheet=\"데이터\""));

    let (_, out_stylings) = OoxmlCodec::unpack_xlsx_with_styling(&bytes).unwrap();
    let st = &out_stylings[1].1;
    assert_eq!(st.pivots_rejected, 0);
    assert_eq!(st.pivots.len(), 3);
    assert_eq!(st.pivots[0].cache_id, st.pivots[1].cache_id);
    assert_ne!(st.pivots[0].cache_id, st.pivots[2].cache_id);
    assert_eq!(st.pivots[0].name, "피벗 테이블1");
    assert_eq!(st.pivots[0].source_sheet, "데이터");
    assert_eq!(st.pivots[0].source_range, "A1:E6");
    assert_eq!(st.pivots[0].source_table.as_deref(), Some("매출표"));
    assert!(st.pivots[0].has_header);
    assert_eq!(st.pivots[0].location.to_a1(), "B2:F10");
    assert_eq!(st.pivots[0].row_fields, vec![0, 2]);
    assert_eq!(st.pivots[0].col_fields, vec![1]);
    assert_eq!(st.pivots[0].value_fields.len(), 2);
    assert_eq!(st.pivots[0].value_fields[0].agg, PivotAggKind::Sum);
    assert_eq!(st.pivots[0].value_fields[1].agg, PivotAggKind::Average);
    assert_eq!(st.pivots[0].filters.len(), 1);
    assert_eq!(st.pivots[0].filters[0].values, vec!["서울", "부산"]);
    assert_eq!(st.pivots[0].slicers.len(), 1);
    assert_eq!(st.pivots[0].slicers[0].caption, "지역 슬라이서");
    assert!(st.pivots[0].slicers[0].multi_select);

    assert_eq!(st.pivots[1].filter_fields, vec![0]);
    assert_eq!(st.pivots[1].value_fields[0].agg, PivotAggKind::Min);
    assert_eq!(st.pivots[1].value_fields[1].agg, PivotAggKind::Max);
    assert_eq!(st.pivots[1].value_fields[2].agg, PivotAggKind::Count);
    assert!(st.pivots[1].filters[0].blank);

    assert_eq!(st.pivots[2].source_sheet, "보조");
    assert!(!st.pivots[2].has_header);
    assert_eq!(st.pivots[2].source_table, None);
}

#[test]
fn external_spreadsheetml_pivot_and_ms_xlsx_pivot_slicer_import_with_traversal_rejection() {
    let pkg = ZipArchiveCodec::pack_compressed(&[
        ("xl/workbook.xml".to_string(), br#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Sales" sheetId="1" r:id="rId1"/></sheets><pivotCaches><pivotCache cacheId="42" r:id="rIdPC42"/></pivotCaches></workbook>"#.to_vec()),
        ("xl/_rels/workbook.xml.rels".to_string(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rIdPC42" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheDefinition" Target="pivotCache/pivotCacheDefinition42.xml"/></Relationships>"#.to_vec()),
        ("xl/worksheets/sheet1.xml".to_string(), br#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Region</t></is></c><c r="B1" t="inlineStr"><is><t>Quarter</t></is></c><c r="C1" t="inlineStr"><is><t>Revenue</t></is></c></row></sheetData></worksheet>"#.to_vec()),
        ("xl/worksheets/_rels/sheet1.xml.rels".to_string(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdPT1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotTable" Target="../pivotTables/pivotTable1.xml"/><Relationship Id="rIdBad" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotTable" Target="../../../etc/passwd"/></Relationships>"#.to_vec()),
        ("xl/pivotCache/pivotCacheDefinition42.xml".to_string(), br#"<?xml version="1.0"?><pivotCacheDefinition xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" recordCount="1"><cacheSource type="worksheet"><worksheetSource ref="A1:C5" sheet="Sales"/></cacheSource><cacheFields count="3"><cacheField name="Region" numFmtId="0"/><cacheField name="Quarter" numFmtId="0"/><cacheField name="Revenue" numFmtId="0"/></cacheFields></pivotCacheDefinition>"#.to_vec()),
        ("xl/pivotTables/pivotTable1.xml".to_string(), br#"<?xml version="1.0"?><pivotTableDefinition xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" name="ExternalPivot" cacheId="42" dataCaption="Values"><location ref="E1:G6" firstHeaderRow="1" firstDataRow="2" firstDataCol="1"/><pivotFields count="3"><pivotField axis="axisRow"/><pivotField axis="axisCol"/><pivotField dataField="1"/></pivotFields><rowFields count="1"><field x="0"/></rowFields><colFields count="1"><field x="1"/></colFields><dataFields count="2"><dataField name="Sum of Revenue" fld="2" baseField="0" baseItem="0"/><dataField name="Count of Region" fld="0" subtotal="countNums" baseField="0" baseItem="0"/></dataFields></pivotTableDefinition>"#.to_vec()),
        ("xl/slicerCaches/slicerCache1.xml".to_string(), concat!(
            r#"<?xml version="1.0"?><slicerCacheDefinition xmlns="http://schemas.micro"#,
            r#"soft.com/office/spreadsheetml/2009/9/main" name="Slicer_Region" sourceName="Region"><pivotTables><pivotTable tabId="1" name="ExternalPivot"/></pivotTables><data><tabular pivotCacheId="42"><items count="1"><item n="East" s="1"/></items></tabular></data></slicerCacheDefinition>"#
        ).as_bytes().to_vec()),
        ("xl/slicers/slicer1.xml".to_string(), concat!(
            r#"<?xml version="1.0"?><slicers xmlns="http://schemas.micro"#,
            r#"soft.com/office/spreadsheetml/2009/9/main"><slicer name="Region Slicer" cache="Slicer_Region" caption="Region Filter" width="1828800" height="2057400"/></slicers>"#
        ).as_bytes().to_vec()),
    ]).unwrap();

    let (_, stylings) = OoxmlCodec::unpack_xlsx_with_styling(&pkg).unwrap();
    let st = &stylings[0].1;
    assert_eq!(st.pivots_rejected, 1);
    assert_eq!(st.pivots.len(), 1);
    let pv = &st.pivots[0];
    assert_eq!(pv.name, "ExternalPivot");
    assert_eq!(pv.cache_id, 42);
    assert_eq!(pv.source_sheet, "Sales");
    assert_eq!(pv.source_range, "A1:C5");
    assert!(pv.has_header);
    assert_eq!(pv.location.to_a1(), "E1:G6");
    assert_eq!(pv.field_names, vec!["Region", "Quarter", "Revenue"]);
    assert_eq!(pv.row_fields, vec![0]);
    assert_eq!(pv.col_fields, vec![1]);
    assert_eq!(pv.value_fields.len(), 2);
    assert_eq!(pv.value_fields[0].agg, PivotAggKind::Sum);
    assert_eq!(pv.value_fields[1].agg, PivotAggKind::Count);
    assert_eq!(pv.slicers.len(), 1);
    assert_eq!(pv.slicers[0].col, 0);
    assert_eq!(pv.slicers[0].caption, "Region Filter");
}

#[test]
fn table_name_only_pivot_resolves_worksheet_source_range_and_sheet() {
    let sheets = vec![(
        "원본".to_string(),
        vec![
            (0, 0, "품목".to_string()),
            (0, 1, "금액".to_string()),
            (1, 0, "사과".to_string()),
            (1, 1, "500".to_string()),
        ],
    )];
    let mut st = SheetStyling::default();
    st.tables.push(SheetTable::new(
        "SalesTable".to_string(),
        CellBox::parse("A1:B2").unwrap(),
    ));
    st.pivots.push(SheetPivotTable {
        name: "표 피벗".to_string(),
        cache_id: 1,
        source_sheet: String::new(),
        source_range: String::new(),
        source_table: Some("SalesTable".to_string()),
        has_header: true,
        location: CellBox::parse("D1:E4").unwrap(),
        field_names: vec![],
        row_fields: vec![0],
        col_fields: vec![],
        filter_fields: vec![],
        value_fields: vec![PivotValueField {
            col: 1,
            agg: PivotAggKind::Sum,
            name: "합계:금액".to_string(),
        }],
        filters: vec![],
        slicers: vec![],
    });

    let bytes = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[st]).unwrap();
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let pc = std::str::from_utf8(
        &entries
            .iter()
            .find(|e| e.path == "xl/pivotCache/pivotCacheDefinition1.xml")
            .unwrap()
            .data,
    )
    .unwrap();
    assert!(
        pc.contains("ref=\"A1:B2\"") && pc.contains("sheet=\"원본\""),
        "{pc}"
    );

    let (_, out_st) = OoxmlCodec::unpack_xlsx_with_styling(&bytes).unwrap();
    assert_eq!(
        out_st[0].1.pivots[0].source_table.as_deref(),
        Some("SalesTable")
    );
    assert_eq!(out_st[0].1.pivots[0].source_sheet, "원본");
    assert_eq!(out_st[0].1.pivots[0].source_range, "A1:B2");
}

#[test]
fn pivot_budgets_fail_closed_on_write_and_read() {
    let sheets = sales_sheets();

    // 1. Over MAX_PIVOTS_PER_SHEET (33)
    let mut over_sheet = SheetStyling::default();
    for i in 0..=MAX_PIVOTS_PER_SHEET {
        over_sheet
            .pivots
            .push(sample_pivot(&format!("P{i}"), 1, "B2:E6"));
    }
    assert!(matches!(
        OoxmlCodec::pack_xlsx_multi_styled(
            &sheets,
            &[],
            &[over_sheet, SheetStyling::default(), SheetStyling::default()]
        ),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 2. Over MAX_WORKBOOK_PIVOTS (129 across 5 sheets)
    let multi_sheets: Vec<OdsUnpacked> = (0..5)
        .map(|i| {
            (
                format!("S{}", i + 1),
                vec![
                    (0, 0, "K".to_string()),
                    (0, 1, "V".to_string()),
                    (1, 0, "A".to_string()),
                    (1, 1, "1".to_string()),
                ],
            )
        })
        .collect();
    let mut multi_st = vec![SheetStyling::default(); 5];
    for st in multi_st.iter_mut().take(4) {
        for j in 0..MAX_PIVOTS_PER_SHEET {
            st.pivots.push(sample_pivot(&format!("P{j}"), 1, "D1:E3"));
        }
    }
    multi_st[4].pivots.push(sample_pivot("P129", 1, "D1:E3"));
    assert_eq!(
        multi_st.iter().map(|s| s.pivots.len()).sum::<usize>(),
        MAX_WORKBOOK_PIVOTS + 1
    );
    assert!(matches!(
        OoxmlCodec::pack_xlsx_multi_styled(&multi_sheets, &[], &multi_st),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 3. Over MAX_PIVOT_AXIS_FIELDS
    let mut over_axis = SheetStyling::default();
    let mut p = sample_pivot("P", 1, "B2:E6");
    p.row_fields = (0..=MAX_PIVOT_AXIS_FIELDS as u32).collect();
    over_axis.pivots.push(p);
    assert!(matches!(
        OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[over_axis]),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 4. Over MAX_PIVOT_VALUE_FIELDS
    let mut over_vals = SheetStyling::default();
    let mut p = sample_pivot("P", 1, "B2:E6");
    p.value_fields = (0..=MAX_PIVOT_VALUE_FIELDS)
        .map(|i| PivotValueField {
            col: 3,
            agg: PivotAggKind::Sum,
            name: format!("V{i}"),
        })
        .collect();
    over_vals.pivots.push(p);
    assert!(matches!(
        OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[over_vals]),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 5. Over MAX_PIVOT_SLICERS
    let mut over_slicers = SheetStyling::default();
    let mut p = sample_pivot("P", 1, "B2:E6");
    p.slicers = (0..=MAX_PIVOT_SLICERS)
        .map(|i| PivotSlicer {
            col: 0,
            caption: format!("S{i}"),
            left: 10,
            top: 10,
            width: 180,
            height: 200,
            multi_select: false,
        })
        .collect();
    over_slicers.pivots.push(p);
    assert!(matches!(
        OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[over_slicers]),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 6. Over MAX_PIVOT_FILTER_VALUES
    let mut over_fvals = SheetStyling::default();
    let mut p = sample_pivot("P", 1, "B2:E6");
    p.filters = vec![PivotFilterCriteria {
        col: 0,
        values: (0..=MAX_PIVOT_FILTER_VALUES)
            .map(|i| format!("v{i}"))
            .collect(),
        blank: false,
    }];
    over_fvals.pivots.push(p);
    assert!(matches!(
        OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[over_fvals]),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 7. Over MAX_PIVOT_TEXT_BYTES
    let mut over_text = SheetStyling::default();
    over_text.pivots.push(sample_pivot(
        &"x".repeat(MAX_PIVOT_TEXT_BYTES + 1),
        1,
        "B2:E6",
    ));
    assert!(matches!(
        OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[over_text]),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 8. Malicious XLSX read exceeding MAX_PIVOTS_PER_SHEET fails closed
    let mut rels = String::from(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    );
    for i in 0..=MAX_PIVOTS_PER_SHEET {
        rels.push_str(&format!(
            r#"<Relationship Id="rIdPT{i}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotTable" Target="../pivotTables/pivotTable1.xml"/>"#
        ));
    }
    rels.push_str("</Relationships>");
    let bad_zip = ZipArchiveCodec::pack_compressed(&[
        ("xl/workbook.xml".to_string(), br#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="S1" sheetId="1" r:id="rId1"/></sheets></workbook>"#.to_vec()),
        ("xl/_rels/workbook.xml.rels".to_string(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#.to_vec()),
        ("xl/worksheets/sheet1.xml".to_string(), br#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData/></worksheet>"#.to_vec()),
        ("xl/worksheets/_rels/sheet1.xml.rels".to_string(), rels.into_bytes()),
    ]).unwrap();
    assert!(matches!(
        OoxmlCodec::unpack_xlsx_with_styling(&bad_zip),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 9. Malicious XML pivot part with > MAX_PIVOT_FILTER_VALUES fails closed
    let mut vals_xml = String::new();
    for i in 0..=MAX_PIVOT_FILTER_VALUES {
        vals_xml.push_str(&format!("<eo:val>v{i}</eo:val>"));
    }
    let bad_filter_zip = ZipArchiveCodec::pack_compressed(&[
        ("xl/workbook.xml".to_string(), br#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="S1" sheetId="1" r:id="rId1"/></sheets><pivotCaches><pivotCache cacheId="1" r:id="rIdPC1"/></pivotCaches></workbook>"#.to_vec()),
        ("xl/_rels/workbook.xml.rels".to_string(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rIdPC1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheDefinition" Target="pivotCache/pivotCacheDefinition1.xml"/></Relationships>"#.to_vec()),
        ("xl/worksheets/sheet1.xml".to_string(), br#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData/></worksheet>"#.to_vec()),
        ("xl/worksheets/_rels/sheet1.xml.rels".to_string(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdPT1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotTable" Target="../pivotTables/pivotTable1.xml"/></Relationships>"#.to_vec()),
        ("xl/pivotCache/pivotCacheDefinition1.xml".to_string(), br#"<?xml version="1.0"?><pivotCacheDefinition xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><cacheSource type="worksheet"><worksheetSource ref="A1:B2" sheet="S1"/></cacheSource><cacheFields count="2"><cacheField name="A"/><cacheField name="B"/></cacheFields></pivotCacheDefinition>"#.to_vec()),
        ("xl/pivotTables/pivotTable1.xml".to_string(), format!(r#"<?xml version="1.0"?><pivotTableDefinition xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" name="P" cacheId="1"><location ref="D1:E3"/><rowFields count="1"><field x="0"/></rowFields><dataFields count="1"><dataField name="V" fld="1"/></dataFields><extLst><ext uri="{{E0511CE5-2420-4E05-9A11-511CE5000255}}" xmlns:eo="urn:easyoffice:spreadsheet:pivots:1.0"><eo:pivot hasHeader="1"><eo:filter col="0" blank="0">{vals_xml}</eo:filter></eo:pivot></ext></extLst></pivotTableDefinition>"#).into_bytes()),
    ]).unwrap();
    assert!(matches!(
        OoxmlCodec::unpack_xlsx_with_styling(&bad_filter_zip),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
#[ignore = "release performance benchmark"]
fn bench_pivot_cache_o1_pack_and_unpack() {
    let rows = 10_000u32;
    let mut cells = Vec::with_capacity((rows as usize) * 5);
    cells.push((0, 0, "Region".to_string()));
    cells.push((0, 1, "Category".to_string()));
    cells.push((0, 2, "Quarter".to_string()));
    cells.push((0, 3, "Revenue".to_string()));
    cells.push((0, 4, "Units".to_string()));
    let regions = ["Seoul", "Busan", "Daegu", "Incheon", "Gwangju"];
    let cats = ["Electronics", "Furniture", "Office", "Cloud"];
    let quarters = ["Q1", "Q2", "Q3", "Q4"];
    for r in 1..rows {
        let i = r as usize;
        cells.push((r, 0, regions[i % regions.len()].to_string()));
        cells.push((r, 1, cats[(i / 5) % cats.len()].to_string()));
        cells.push((r, 2, quarters[(i / 20) % quarters.len()].to_string()));
        cells.push((r, 3, format!("{}", ((r * 37) % 5000) + 100)));
        cells.push((r, 4, format!("{}", (r % 40) + 1)));
    }
    let sheets = vec![("Data".to_string(), cells), ("Pivots".to_string(), vec![])];
    let mut p_st = SheetStyling::default();
    for k in 0..16u32 {
        p_st.pivots.push(SheetPivotTable {
            name: format!("Pivot{k}"),
            cache_id: 1,
            source_sheet: "Data".to_string(),
            source_range: format!("A1:E{rows}"),
            source_table: None,
            has_header: true,
            location: CellBox::parse(&format!("A{}:F{}", k * 12 + 1, k * 12 + 10)).unwrap(),
            field_names: vec![
                "Region".into(),
                "Category".into(),
                "Quarter".into(),
                "Revenue".into(),
                "Units".into(),
            ],
            row_fields: vec![0, 1],
            col_fields: vec![2],
            filter_fields: vec![],
            value_fields: vec![PivotValueField {
                col: 3,
                agg: PivotAggKind::Sum,
                name: "Sum Revenue".into(),
            }],
            filters: vec![],
            slicers: vec![],
        });
    }
    let stylings = vec![SheetStyling::default(), p_st];

    let t_pack = Instant::now();
    let bytes = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &stylings).unwrap();
    let pack_ms = t_pack.elapsed().as_secs_f64() * 1000.0;

    let t_unpack = Instant::now();
    let (out_sheets, out_st) = OoxmlCodec::unpack_xlsx_with_styling(&bytes).unwrap();
    let unpack_ms = t_unpack.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(out_sheets[0].1.len(), sheets[0].1.len());
    assert_eq!(out_st[1].1.pivots.len(), 16);
    println!(
        "PD-255 Bench (10k rows x 5 cols, 16 pivots sharing 1 cache): xlsx_pack={pack_ms:.2}ms ({} B), xlsx_unpack={unpack_ms:.2}ms",
        bytes.len()
    );
}

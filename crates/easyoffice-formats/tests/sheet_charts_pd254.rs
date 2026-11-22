//! PD-254: Bidirectional XLSX chart round-trip, external SpreadsheetML parsing,
//! formula cache population, pure vector PDF rendering, fail-closed resource budgets,
//! and O(1) cell-index release benchmark.

use std::collections::BTreeMap;
use std::time::Instant;

use easyoffice_formats::{
    encode_sheet_pdf, pack_cell_pos, CellBox, ChartAnchor, ChartKind, ChartLegendPos,
    ChartSeriesRef, EasyOfficeError, OdsUnpacked, OoxmlCodec, SheetChart, SheetStyling, SheetTable,
    XlsxCachedValue, XlsxFormulaCache, ZipArchiveCodec, ZipSecurityPolicy, MAX_CHARTS_PER_SHEET,
    MAX_CHART_TEXT_BYTES, MAX_POINTS_PER_SERIES, MAX_SERIES_PER_CHART, MAX_WORKBOOK_CHARTS,
};

fn sample_sheets() -> Vec<OdsUnpacked> {
    vec![(
        "매출".to_string(),
        vec![
            (0, 0, "월".to_string()),
            (0, 1, "실적".to_string()),
            (0, 2, "목표".to_string()),
            (1, 0, "1월".to_string()),
            (1, 1, "120".to_string()),
            (1, 2, "100".to_string()),
            (2, 0, "2월".to_string()),
            (2, 1, "180".to_string()),
            (2, 2, "150".to_string()),
            (3, 0, "3월".to_string()),
            (3, 1, "140".to_string()),
            (3, 2, "160".to_string()),
            (4, 0, "4월".to_string()),
            (4, 1, "=SUM(B2:B4)".to_string()),
            (4, 2, "400".to_string()),
        ],
    )]
}

fn make_chart(
    id: &str,
    kind: ChartKind,
    title: &str,
    legend: ChartLegendPos,
    row_off: u32,
) -> SheetChart {
    SheetChart {
        id: id.to_string(),
        kind,
        title: title.to_string(),
        data_range: "A1:C4".to_string(),
        switch_row_col: false,
        legend,
        anchor: ChartAnchor {
            from_col: 4,
            from_row: row_off,
            to_col: 10,
            to_row: row_off + 11,
            col_off_emu: 0,
            row_off_emu: 0,
            to_col_off_emu: 0,
            to_row_off_emu: 0,
            left_px: 320,
            top_px: row_off * 24,
            width_px: 480,
            height_px: 264,
            ..ChartAnchor::default()
        },
        series: vec![
            ChartSeriesRef {
                name: "실적".to_string(),
                name_ref: Some("매출!$B$1".to_string()),
                categories_ref: Some("매출!$A$2:$A$4".to_string()),
                values_ref: "매출!$B$2:$B$4".to_string(),
            },
            ChartSeriesRef {
                name: "목표".to_string(),
                name_ref: Some("매출!$C$1".to_string()),
                categories_ref: Some("매출!$A$2:$A$4".to_string()),
                values_ref: "매출!$C$2:$C$4".to_string(),
            },
        ],
    }
}

#[test]
fn all_five_chart_kinds_and_schema_order_round_trip_through_xlsx() {
    let sheets = sample_sheets();
    let mut styling = SheetStyling::default();
    styling.columns.insert(0, 100);
    styling.columns.insert(4, 110);
    styling.tables.push(SheetTable::new(
        "SalesTbl",
        CellBox::parse("A1:C4").unwrap(),
    ));
    styling.charts = vec![
        make_chart(
            "c-col",
            ChartKind::Column,
            "분기 매출 & 추이",
            ChartLegendPos::Right,
            0,
        ),
        make_chart(
            "c-bar",
            ChartKind::Bar,
            "가로 막대",
            ChartLegendPos::Bottom,
            12,
        ),
        make_chart("c-line", ChartKind::Line, "추세선", ChartLegendPos::Top, 24),
        SheetChart {
            id: "c-pie".to_string(),
            kind: ChartKind::Pie,
            title: "비중".to_string(),
            data_range: "A1:B4".to_string(),
            switch_row_col: false,
            legend: ChartLegendPos::Left,
            anchor: ChartAnchor {
                left_px: 400,
                top_px: 100,
                width_px: 360,
                height_px: 240,
                ..ChartAnchor::default()
            },
            series: Vec::new(), // Auto-derived from data_range
        },
        SheetChart {
            id: "c-scat".to_string(),
            kind: ChartKind::Scatter,
            title: "".to_string(),
            data_range: "B2:C4".to_string(),
            switch_row_col: false,
            legend: ChartLegendPos::None,
            anchor: ChartAnchor {
                from_col: 5,
                from_row: 36,
                to_col: 11,
                to_row: 48,
                ..ChartAnchor::default()
            },
            series: vec![ChartSeriesRef {
                name: "분포".to_string(),
                name_ref: None,
                categories_ref: Some("매출!$B$2:$B$4".to_string()),
                values_ref: "매출!$C$2:$C$4".to_string(),
            }],
        },
    ];

    let bytes = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[styling.clone()])
        .expect("pack xlsx with 5 chart kinds");

    // Verify OOXML schema ordering (<drawing> after page setup, before <tableParts>) and parts
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let sheet1 = std::str::from_utf8(
        &entries
            .iter()
            .find(|e| e.path == "xl/worksheets/sheet1.xml")
            .unwrap()
            .data,
    )
    .unwrap();
    let drawing_pos = sheet1.find("<drawing ").expect("drawing tag present");
    let table_pos = sheet1.find("<tableParts").expect("tableParts present");
    assert!(drawing_pos < table_pos, "drawing must precede tableParts");

    let content_types = std::str::from_utf8(
        &entries
            .iter()
            .find(|e| e.path == "[Content_Types].xml")
            .unwrap()
            .data,
    )
    .unwrap();
    assert!(content_types.contains("application/vnd.openxmlformats-officedocument.drawing+xml"));
    assert!(
        content_types.contains("application/vnd.openxmlformats-officedocument.drawingml.chart+xml")
    );
    for idx in 1..=5 {
        assert!(entries
            .iter()
            .any(|e| e.path == format!("xl/charts/chart{idx}.xml")));
    }

    // Unpack and verify lossless round-trip
    let (out_sheets, out_stylings) = OoxmlCodec::unpack_xlsx_with_styling(&bytes).unwrap();
    assert_eq!(out_sheets[0].1.len(), sheets[0].1.len());
    let st = &out_stylings[0].1;
    assert_eq!(st.charts_rejected, 0);
    assert_eq!(st.charts.len(), 5);

    assert_eq!(st.charts[0].id, "c-col");
    assert_eq!(st.charts[0].kind, ChartKind::Column);
    assert_eq!(st.charts[0].title, "분기 매출 & 추이");
    assert_eq!(st.charts[0].legend, ChartLegendPos::Right);
    assert_eq!(st.charts[0].data_range, "A1:C4");
    assert_eq!(st.charts[0].series.len(), 2);
    assert_eq!(st.charts[0].series[0].name, "실적");
    assert_eq!(st.charts[0].anchor.from_col, 4);
    assert_eq!(st.charts[0].anchor.to_col, 10);
    // Col 0 width 100 + cols 1..=3 (3*72=216, the standard font's default column) = 316 px
    assert_eq!(st.charts[0].anchor.left_px, 316);

    assert_eq!(st.charts[1].kind, ChartKind::Bar);
    assert_eq!(st.charts[1].legend, ChartLegendPos::Bottom);
    assert_eq!(st.charts[2].kind, ChartKind::Line);
    assert_eq!(st.charts[2].legend, ChartLegendPos::Top);

    assert_eq!(st.charts[3].kind, ChartKind::Pie);
    assert_eq!(st.charts[3].legend, ChartLegendPos::Left);
    assert_eq!(st.charts[3].anchor.left_px, 400);
    assert_eq!(st.charts[3].anchor.top_px, 100);
    assert_eq!(st.charts[3].anchor.width_px, 360);
    assert_eq!(st.charts[3].anchor.height_px, 240);
    assert_eq!(st.charts[3].series.len(), 1);
    assert_eq!(st.charts[3].series[0].values_ref, "'매출'!$B$2:$B$4");

    assert_eq!(st.charts[4].kind, ChartKind::Scatter);
    assert_eq!(st.charts[4].title, "");
    assert_eq!(st.charts[4].legend, ChartLegendPos::None);
    assert_eq!(
        st.charts[4].series[0].categories_ref.as_deref(),
        Some("매출!$B$2:$B$4")
    );
}

#[test]
fn single_pass_series_derivation_handles_row_col_switch_and_1d_ranges() {
    let sheets = vec![(
        "Data".to_string(),
        vec![
            (0, 0, "분류".to_string()),
            (0, 1, "Q1".to_string()),
            (0, 2, "Q2".to_string()),
            (1, 0, "서울".to_string()),
            (1, 1, "10".to_string()),
            (1, 2, "20".to_string()),
            (2, 0, "부산".to_string()),
            (2, 1, "30".to_string()),
            (2, 2, "40".to_string()),
        ],
    )];

    let col_chart = SheetChart {
        data_range: "A1:C3".to_string(),
        switch_row_col: false,
        ..SheetChart::default()
    };
    let s_col = col_chart.resolved_series("Data", &sheets);
    assert_eq!(s_col.len(), 2);
    assert_eq!(s_col[0].name, "Q1");
    assert_eq!(s_col[0].name_ref.as_deref(), Some("Data!$B$1"));
    assert_eq!(s_col[0].categories_ref.as_deref(), Some("Data!$A$2:$A$3"));
    assert_eq!(s_col[0].values_ref, "Data!$B$2:$B$3");

    let row_chart = SheetChart {
        data_range: "A1:C3".to_string(),
        switch_row_col: true,
        ..SheetChart::default()
    };
    let s_row = row_chart.resolved_series("Data", &sheets);
    assert_eq!(s_row.len(), 2);
    assert_eq!(s_row[0].name, "서울");
    assert_eq!(s_row[0].name_ref.as_deref(), Some("Data!$A$2"));
    assert_eq!(s_row[0].categories_ref.as_deref(), Some("Data!$B$1:$C$1"));
    assert_eq!(s_row[0].values_ref, "Data!$B$2:$C$2");

    let one_d = SheetChart {
        title: "단일열".to_string(),
        data_range: "B2:B3".to_string(),
        ..SheetChart::default()
    };
    let s_1d = one_d.resolved_series("Data", &sheets);
    assert_eq!(s_1d.len(), 1);
    assert_eq!(s_1d[0].name, "단일열");
    assert_eq!(s_1d[0].categories_ref, None);
    assert_eq!(s_1d[0].values_ref, "Data!$B$2:$B$3");
}

#[test]
fn formula_cache_populates_chart_xml_caches() {
    let sheets = vec![(
        "S1".to_string(),
        vec![
            (0, 0, "=\"월별\"".to_string()),
            (0, 1, "=\"매출\"".to_string()),
            (1, 0, "1월".to_string()),
            (1, 1, "=10+20".to_string()),
            (2, 0, "2월".to_string()),
            (2, 1, "=20+25".to_string()),
        ],
    )];
    let mut cache = XlsxFormulaCache::new();
    cache.insert((0, 1), XlsxCachedValue::Text("매출".to_string()));
    cache.insert((1, 1), XlsxCachedValue::Number(30.0));
    cache.insert((2, 1), XlsxCachedValue::Number(45.0));

    let st = SheetStyling {
        charts: vec![SheetChart {
            id: "c1".to_string(),
            kind: ChartKind::Column,
            title: "캐시 테스트".to_string(),
            data_range: "A1:B3".to_string(),
            series: vec![ChartSeriesRef {
                name: "".to_string(),
                name_ref: Some("S1!$B$1".to_string()),
                categories_ref: Some("S1!$A$2:$A$3".to_string()),
                values_ref: "S1!$B$2:$B$3".to_string(),
            }],
            ..SheetChart::default()
        }],
        ..SheetStyling::default()
    };

    let bytes = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[cache], &[st]).unwrap();
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let chart_xml = std::str::from_utf8(
        &entries
            .iter()
            .find(|e| e.path == "xl/charts/chart1.xml")
            .unwrap()
            .data,
    )
    .unwrap();
    assert!(chart_xml.contains("<c:v>매출</c:v>"), "{chart_xml}");
    assert!(chart_xml.contains("<c:v>30</c:v>"), "{chart_xml}");
    assert!(chart_xml.contains("<c:v>45</c:v>"), "{chart_xml}");
    assert!(!chart_xml.contains("=10+20"), "{chart_xml}");
}

#[test]
fn external_spreadsheetml_drawing_and_broken_parts_are_parsed_safely() {
    let pkg = ZipArchiveCodec::pack_compressed(&[
        (
            "[Content_Types].xml".to_string(),
            br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/></Types>"#.to_vec(),
        ),
        (
            "_rels/.rels".to_string(),
            br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#.to_vec(),
        ),
        (
            "xl/workbook.xml".to_string(),
            br#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Report" sheetId="1" r:id="rId1"/></sheets></workbook>"#.to_vec(),
        ),
        (
            "xl/_rels/workbook.xml.rels".to_string(),
            br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#.to_vec(),
        ),
        (
            "xl/worksheets/sheet1.xml".to_string(),
            br#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Item</t></is></c></row></sheetData><drawing r:id="rIdDraw1"/></worksheet>"#.to_vec(),
        ),
        (
            "xl/worksheets/_rels/sheet1.xml.rels".to_string(),
            br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdDraw1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="/xl/drawings/drawing1.xml"/></Relationships>"#.to_vec(),
        ),
        (
            "xl/drawings/drawing1.xml".to_string(),
            concat!(
                r#"<?xml version="1.0" encoding="UTF-8"?>"#,
                r#"<xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">"#,
                r#"<xdr:oneCellAnchor><xdr:from><xdr:col>2</xdr:col><xdr:colOff>95250</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from>"#,
                r#"<xdr:ext cx="3810000" cy="2286000"/>"#,
                r#"<xdr:graphicFrame><xdr:nvGraphicFramePr><xdr:cNvPr id="1" name="ExtChart"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr>"#,
                r#"<xdr:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart">"#,
                r#"<c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="rIdC1"/>"#,
                r#"</a:graphicData></xdr:graphic></xdr:graphicFrame><xdr:clientData/></xdr:oneCellAnchor>"#,
                // Plain shape without <c:chart> should be ignored cleanly (not counted as rejected)
                r#"<xdr:twoCellAnchor><xdr:from><xdr:col>0</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>0</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:sp/></xdr:twoCellAnchor>"#,
                // Traversal target must be rejected and counted
                r#"<xdr:twoCellAnchor><xdr:from><xdr:col>0</xdr:col><xdr:colOff>0</xdr:col><xdr:row>0</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>4</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>8</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:graphicFrame><xdr:nvGraphicFramePr><xdr:cNvPr id="2" name="Bad"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="rIdBad"/></a:graphicData></xdr:graphic></xdr:graphicFrame></xdr:twoCellAnchor>"#,
                r#"</xdr:wsDr>"#
            ).as_bytes().to_vec(),
        ),
        (
            "xl/drawings/_rels/drawing1.xml.rels".to_string(),
            br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdC1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart" Target="../charts/chart1.xml"/><Relationship Id="rIdBad" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart" Target="../../../etc/passwd"/></Relationships>"#.to_vec(),
        ),
        (
            "xl/charts/chart1.xml".to_string(),
            concat!(
                r#"<?xml version="1.0" encoding="UTF-8"?>"#,
                r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">"#,
                r#"<c:chart><c:title><c:tx><c:strRef><c:f>Report!$A$1</c:f><c:strCache><c:ptCount val="1"/><c:pt idx="0"><c:v>External Title</c:v></c:pt></c:strCache></c:strRef></c:tx></c:title>"#,
                r#"<c:plotArea><c:bar3DChart><c:barDir val="bar"/><c:ser><c:idx val="0"/><c:order val="0"/>"#,
                r#"<c:tx><c:v>Alpha</c:v></c:tx>"#,
                r#"<c:dLbls><c:dLbl><c:tx><c:v>IgnoredLabel</c:v></c:tx></c:dLbl></c:dLbls>"#,
                r#"<c:cat><c:numRef><c:f>Report!$A$2:$A$4</c:f></c:numRef></c:cat>"#,
                r#"<c:val><c:numRef><c:f>Report!$B$2:$B$4</c:f></c:numRef></c:val>"#,
                r#"</c:ser></c:bar3DChart>"#,
                r#"<c:catAx><c:axId val="1"/><c:title><c:tx><c:v>AxisTitleShouldNotOverwrite</c:v></c:tx></c:title></c:catAx>"#,
                r#"</c:plotArea></c:chart></c:chartSpace>"#
            ).as_bytes().to_vec(),
        ),
    ]).unwrap();

    let (_, stylings) = OoxmlCodec::unpack_xlsx_with_styling(&pkg).unwrap();
    let st = &stylings[0].1;
    assert_eq!(st.charts_rejected, 1);
    assert_eq!(st.charts.len(), 1);
    let ch = &st.charts[0];
    assert_eq!(ch.id, "ExtChart");
    assert_eq!(ch.kind, ChartKind::Bar);
    assert_eq!(ch.title, "External Title");
    assert_eq!(ch.legend, ChartLegendPos::None);
    // 2*64 + 10: this workbook has no `styles.xml`, so its default font is Calibri 11 pt, a 7 px digit (PD-306)
    assert_eq!(ch.anchor.left_px, 138);
    assert_eq!(ch.anchor.top_px, 22);
    assert_eq!(ch.anchor.width_px, 400);
    assert_eq!(ch.anchor.height_px, 240);
    assert_eq!(ch.series[0].name, "Alpha");
    assert_eq!(ch.series[0].values_ref, "Report!$B$2:$B$4");
}

#[test]
fn vector_pdf_renders_all_five_chart_kinds_without_raster_images() {
    let sheets = sample_sheets();
    let styling = SheetStyling {
        charts: vec![
            make_chart(
                "p1",
                ChartKind::Column,
                "PDF Column Chart",
                ChartLegendPos::Right,
                0,
            ),
            make_chart(
                "p2",
                ChartKind::Bar,
                "PDF Bar Chart",
                ChartLegendPos::Bottom,
                13,
            ),
            make_chart(
                "p3",
                ChartKind::Line,
                "PDF Line Chart",
                ChartLegendPos::Top,
                26,
            ),
            make_chart(
                "p4",
                ChartKind::Pie,
                "PDF Pie Chart",
                ChartLegendPos::Left,
                39,
            ),
            make_chart(
                "p5",
                ChartKind::Scatter,
                "PDF Scatter Chart",
                ChartLegendPos::Right,
                52,
            ),
        ],
        ..Default::default()
    };

    let pdf = encode_sheet_pdf("차트 보고서", &sheets, &[styling], &[]).unwrap();
    assert!(pdf.starts_with(b"%PDF-1.4"));
    assert!(pdf.ends_with(b"%%EOF\n"));
    let pdf_text = String::from_utf8_lossy(&pdf);
    assert!(
        pdf_text.contains("PDF Column Chart"),
        "missing column title"
    );
    assert!(pdf_text.contains("PDF Bar Chart"), "missing bar title");
    assert!(pdf_text.contains("PDF Line Chart"), "missing line title");
    assert!(pdf_text.contains("PDF Pie Chart"), "missing pie title");
    assert!(
        pdf_text.contains("PDF Scatter Chart"),
        "missing scatter title"
    );
    assert!(pdf_text.contains(" re f Q"), "missing vector filled rects");
    assert!(pdf_text.contains(" re S Q"), "missing vector stroked rects");
    assert!(
        pdf_text.contains(" c "),
        "missing vector bezier curves for pie/markers"
    );
    assert!(pdf_text.contains(" h B Q"), "missing pie wedge fill+stroke");
    assert!(
        !pdf_text.contains("/Subtype /Image"),
        "must stay 100% vector"
    );

    // Verify print_area clipping: A1:C4 does not intersect chart placed at col 4..10 (E..K)
    let mut clipped_st = SheetStyling::default();
    clipped_st.page.print_area = vec![CellBox::parse("A1:C4").unwrap()];
    clipped_st.charts.push(make_chart(
        "p-off",
        ChartKind::Column,
        "OffPageTitle",
        ChartLegendPos::Right,
        0,
    ));
    let pdf_clipped = encode_sheet_pdf("clipped", &sheets, &[clipped_st], &[]).unwrap();
    let pdf_clipped_text = String::from_utf8_lossy(&pdf_clipped);
    assert!(!pdf_clipped_text.contains("OffPageTitle"));
}

#[test]
fn chart_budgets_fail_closed_on_write_and_read() {
    let sheets = sample_sheets();

    // 1. Per-sheet chart limit (> 64)
    let mut over_sheet = SheetStyling::default();
    for i in 0..=MAX_CHARTS_PER_SHEET {
        over_sheet.charts.push(make_chart(
            &format!("c{i}"),
            ChartKind::Column,
            "t",
            ChartLegendPos::None,
            0,
        ));
    }
    assert!(matches!(
        OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[over_sheet]),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 2. Workbook-wide chart limit (> 256 across 5 sheets)
    let multi_sheets: Vec<OdsUnpacked> = (0..5)
        .map(|i| (format!("S{}", i + 1), vec![(0, 0, "1".to_string())]))
        .collect();
    let mut multi_st = vec![SheetStyling::default(); 5];
    for st in multi_st.iter_mut().take(4) {
        for j in 0..MAX_CHARTS_PER_SHEET {
            st.charts.push(make_chart(
                &format!("c{j}"),
                ChartKind::Column,
                "t",
                ChartLegendPos::None,
                0,
            ));
        }
    }
    multi_st[4].charts.push(make_chart(
        "c257",
        ChartKind::Column,
        "t",
        ChartLegendPos::None,
        0,
    ));
    assert_eq!(
        multi_st.iter().map(|s| s.charts.len()).sum::<usize>(),
        MAX_WORKBOOK_CHARTS + 1
    );
    assert!(matches!(
        OoxmlCodec::pack_xlsx_multi_styled(&multi_sheets, &[], &multi_st),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 3. Series count limit (> 32)
    let mut over_series = SheetStyling::default();
    let mut ch = make_chart("c1", ChartKind::Column, "t", ChartLegendPos::None, 0);
    ch.series = (0..=MAX_SERIES_PER_CHART)
        .map(|i| ChartSeriesRef {
            name: format!("S{i}"),
            values_ref: "매출!$B$2:$B$4".to_string(),
            ..Default::default()
        })
        .collect();
    over_series.charts.push(ch);
    assert!(matches!(
        OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[over_series]),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 4. Series points limit (> 4096)
    let mut over_points = SheetStyling::default();
    let mut ch_pts = make_chart("c1", ChartKind::Line, "t", ChartLegendPos::None, 0);
    ch_pts.series = vec![ChartSeriesRef {
        name: "Big".to_string(),
        values_ref: format!("매출!$B$1:$B${}", MAX_POINTS_PER_SERIES + 2),
        ..Default::default()
    }];
    over_points.charts.push(ch_pts);
    assert!(matches!(
        OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[over_points]),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 5. Text length limit (> 512 bytes)
    let mut over_text = SheetStyling::default();
    over_text.charts.push(make_chart(
        "c1",
        ChartKind::Column,
        &"x".repeat(MAX_CHART_TEXT_BYTES + 1),
        ChartLegendPos::None,
        0,
    ));
    assert!(matches!(
        OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[over_text]),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 6. Malicious XLSX read with > MAX_CHARTS_PER_SHEET anchors fails closed
    let mut anchors = String::new();
    for i in 0..=MAX_CHARTS_PER_SHEET {
        anchors.push_str(&format!(
            "<xdr:twoCellAnchor><xdr:from><xdr:col>0</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>0</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>4</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>8</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:graphicFrame><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{i}\" name=\"c{i}\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:id=\"rId1\"/></a:graphicData></xdr:graphic></xdr:graphicFrame></xdr:twoCellAnchor>"
        ));
    }
    let bad_zip = ZipArchiveCodec::pack_compressed(&[
        ("xl/workbook.xml".to_string(), br#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="S1" sheetId="1" r:id="rId1"/></sheets></workbook>"#.to_vec()),
        ("xl/_rels/workbook.xml.rels".to_string(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#.to_vec()),
        ("xl/worksheets/sheet1.xml".to_string(), br#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheetData/><drawing r:id="rIdD1"/></worksheet>"#.to_vec()),
        ("xl/worksheets/_rels/sheet1.xml.rels".to_string(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdD1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/drawing1.xml"/></Relationships>"#.to_vec()),
        ("xl/drawings/drawing1.xml".to_string(), format!(r#"<?xml version="1.0"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">{anchors}</xdr:wsDr>"#).into_bytes()),
    ]).unwrap();
    assert!(matches!(
        OoxmlCodec::unpack_xlsx_with_styling(&bad_zip),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
#[ignore = "release performance benchmark"]
fn bench_chart_o1_index_xlsx_roundtrip_and_vector_pdf() {
    let rows = 3_000u32;
    let mut cells = Vec::with_capacity((rows as usize) * 5);
    cells.push((0, 0, "Label".to_string()));
    for c in 1..=4u32 {
        cells.push((0, c, format!("Series{c}")));
    }
    for r in 1..rows {
        cells.push((r, 0, format!("R{r}")));
        for c in 1..=4u32 {
            cells.push((r, c, format!("{}", (r * c) % 1000)));
        }
    }
    let sheets: Vec<OdsUnpacked> = (0..4)
        .map(|i| (format!("Sheet{}", i + 1), cells.clone()))
        .collect();
    let stylings: Vec<SheetStyling> = (0..4)
        .map(|s| {
            let mut st = SheetStyling::default();
            for c in 0..16u32 {
                let kind = match c % 5 {
                    0 => ChartKind::Column,
                    1 => ChartKind::Bar,
                    2 => ChartKind::Line,
                    3 => ChartKind::Pie,
                    _ => ChartKind::Scatter,
                };
                st.charts.push(SheetChart {
                    id: format!("s{s}-c{c}"),
                    kind,
                    title: format!("Chart {c}"),
                    data_range: "A1:E250".to_string(),
                    switch_row_col: false,
                    legend: ChartLegendPos::Right,
                    anchor: ChartAnchor {
                        from_col: 6,
                        from_row: c * 14,
                        to_col: 12,
                        to_row: c * 14 + 12,
                        ..ChartAnchor::default()
                    },
                    series: (1..=4)
                        .map(|col| ChartSeriesRef {
                            name: format!("Series{col}"),
                            name_ref: Some(format!(
                                "Sheet{}!${}$1",
                                s + 1,
                                (b'A' + col as u8) as char
                            )),
                            categories_ref: Some(format!("Sheet{}!$A$2:$A$250", s + 1)),
                            values_ref: format!(
                                "Sheet{}!${}$2:${}$250",
                                s + 1,
                                (b'A' + col as u8) as char,
                                (b'A' + col as u8) as char
                            ),
                        })
                        .collect(),
                });
            }
            st
        })
        .collect();

    // Baseline: naive linear scan over sheet cells vs O(1) packed FastU64Map index
    let t_linear = Instant::now();
    let mut sum_linear = 0usize;
    for st in &stylings {
        for ch in &st.charts {
            for ser in &ch.series {
                let _ = ser;
                for r in 1..250 {
                    if let Some((_, _, raw)) =
                        sheets[0].1.iter().find(|&&(rr, cc, _)| rr == r && cc == 1)
                    {
                        sum_linear += raw.len();
                    }
                }
            }
        }
    }
    let linear_ms = t_linear.elapsed().as_secs_f64() * 1000.0;

    let t_o1 = Instant::now();
    let mut map: BTreeMap<u64, &str> = BTreeMap::new();
    for &(r, c, ref raw) in &sheets[0].1 {
        map.insert(pack_cell_pos(r, c), raw.as_str());
    }
    let mut sum_o1 = 0usize;
    for st in &stylings {
        for ch in &st.charts {
            for ser in &ch.series {
                let _ = ser;
                for r in 1..250 {
                    if let Some(raw) = map.get(&pack_cell_pos(r, 1)) {
                        sum_o1 += raw.len();
                    }
                }
            }
        }
    }
    let o1_ms = t_o1.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(sum_linear, sum_o1);

    let t_pack = Instant::now();
    let xlsx = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &stylings).unwrap();
    let pack_ms = t_pack.elapsed().as_secs_f64() * 1000.0;

    let t_unpack = Instant::now();
    let (back_sheets, back_st) = OoxmlCodec::unpack_xlsx_with_styling(&xlsx).unwrap();
    let unpack_ms = t_unpack.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(back_sheets.len(), 4);
    assert_eq!(
        back_st.iter().map(|(_, s)| s.charts.len()).sum::<usize>(),
        64
    );

    let t_pdf = Instant::now();
    let pdf = encode_sheet_pdf("bench", &sheets, &stylings, &[]).unwrap();
    let pdf_ms = t_pdf.elapsed().as_secs_f64() * 1000.0;
    assert!(pdf.len() > 10_000);

    println!(
        "PD-254 Bench (64 charts x 4 series x 249 pts across 60k cells): linear_scan={linear_ms:.2}ms, indexed_lookup={o1_ms:.2}ms, xlsx_pack={pack_ms:.2}ms ({} B), xlsx_unpack={unpack_ms:.2}ms, vector_pdf={pdf_ms:.2}ms ({} B)",
        xlsx.len(),
        pdf.len()
    );
}

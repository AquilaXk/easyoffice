//! PD-254: Structured sheet chart JSON round-trip, row/column structure shift,
//! `--inspect` readback & rejected-chart warnings, and fail-closed budget tests.

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use easyoffice_app::cli_args::InspectArgs;
use easyoffice_app::inspect_cli::run_inspect_cli;
use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{
    base64_codec, EasyOfficeError, StrictJsonValue, ZipArchiveCodec, MAX_CHARTS_PER_SHEET,
    MAX_CHART_TEXT_BYTES, MAX_POINTS_PER_SERIES, MAX_SERIES_PER_CHART,
};

fn parse_obj(json: &str) -> Vec<(String, StrictJsonValue)> {
    match StrictJsonValue::parse(json).expect("valid JSON response") {
        StrictJsonValue::Object(entries) => entries,
        other => panic!("expected root JSON object, got {other:?}"),
    }
}

fn field<'a>(entries: &'a [(String, StrictJsonValue)], key: &str) -> &'a StrictJsonValue {
    StrictJsonValue::get_field(entries, key)
        .unwrap_or_else(|| panic!("missing JSON field '{key}' in {entries:?}"))
}

#[test]
fn structured_save_decode_and_pdf_preserve_charts_json_schema() {
    let req = r#"{
        "targetFormat": "xlsx",
        "title": "분기 실적",
        "sheets": [{
            "name": "매출",
            "cells": [
                {"ref": "A1", "raw": "월"},
                {"ref": "B1", "raw": "실적"},
                {"ref": "C1", "raw": "목표"},
                {"ref": "A2", "raw": "1월"},
                {"ref": "B2", "raw": "120"},
                {"ref": "C2", "raw": "100"},
                {"ref": "A3", "raw": "2월"},
                {"ref": "B3", "raw": "180"},
                {"ref": "C3", "raw": "150"}
            ],
            "charts": [{
                "id": "chart-sales",
                "chartType": "line",
                "title": "월별 실적 추이",
                "dataRange": "A1:C3",
                "switchRowCol": false,
                "legend": "bottom",
                "anchor": {
                    "fromCol": 4,
                    "fromRow": 1,
                    "toCol": 10,
                    "toRow": 13,
                    "left": 320,
                    "top": 24,
                    "width": 480,
                    "height": 288
                },
                "series": [
                    {
                        "name": "실적",
                        "nameRef": "매출!$B$1",
                        "categoriesRef": "매출!$A$2:$A$3",
                        "valuesRef": "매출!$B$2:$B$3"
                    },
                    {
                        "name": "목표",
                        "nameRef": "매출!$C$1",
                        "categoriesRef": "매출!$A$2:$A$3",
                        "valuesRef": "매출!$C$2:$C$3"
                    }
                ]
            }]
        }]
    }"#;

    let saved = EasyOfficeIpcRouter::dispatch("convert_document_format", req).unwrap();
    let saved_obj = parse_obj(&saved);
    let b64 = field(&saved_obj, "convertedBase64").as_str().unwrap();

    // Decode back via IPC
    let dec_req =
        format!(r#"{{"action":"decode","fileName":"sales.xlsx","sourceBase64":"{b64}"}}"#);
    let decoded = EasyOfficeIpcRouter::dispatch("convert_document_format", &dec_req).unwrap();
    let dec_obj = parse_obj(&decoded);
    let sheets = field(&dec_obj, "sheets").as_array().unwrap();
    let s0 = sheets[0].as_object().unwrap();
    let charts = field(s0, "charts").as_array().unwrap();
    assert_eq!(charts.len(), 1);
    let ch = charts[0].as_object().unwrap();
    assert_eq!(field(ch, "id").as_str(), Some("chart-sales"));
    assert_eq!(field(ch, "chartType").as_str(), Some("line"));
    assert_eq!(field(ch, "title").as_str(), Some("월별 실적 추이"));
    assert_eq!(field(ch, "dataRange").as_str(), Some("A1:C3"));
    assert_eq!(field(ch, "switchRowCol").as_bool(), Some(false));
    assert_eq!(field(ch, "legend").as_str(), Some("bottom"));

    let anchor = field(ch, "anchor").as_object().unwrap();
    assert_eq!(field(anchor, "fromCol").as_u64(), Some(4));
    assert_eq!(field(anchor, "fromRow").as_u64(), Some(1));
    assert_eq!(field(anchor, "toCol").as_u64(), Some(10));
    assert_eq!(field(anchor, "toRow").as_u64(), Some(13));
    assert_eq!(field(anchor, "width").as_u64(), Some(480));
    assert_eq!(field(anchor, "height").as_u64(), Some(288));

    let series = field(ch, "series").as_array().unwrap();
    assert_eq!(series.len(), 2);
    let ser0 = series[0].as_object().unwrap();
    assert_eq!(field(ser0, "name").as_str(), Some("실적"));
    assert_eq!(field(ser0, "nameRef").as_str(), Some("매출!$B$1"));
    assert_eq!(
        field(ser0, "categoriesRef").as_str(),
        Some("매출!$A$2:$A$3")
    );
    assert_eq!(field(ser0, "valuesRef").as_str(), Some("매출!$B$2:$B$3"));

    // Structured save to PDF renders vector chart
    let pdf_req = req.replace(r#""targetFormat": "xlsx""#, r#""targetFormat": "pdf""#);
    let pdf_saved = EasyOfficeIpcRouter::dispatch("convert_document_format", &pdf_req).unwrap();
    let pdf_obj = parse_obj(&pdf_saved);
    let pdf_b64 = field(&pdf_obj, "convertedBase64").as_str().unwrap();
    let pdf_bytes = base64_codec::decode(pdf_b64).unwrap();
    let pdf_text = String::from_utf8_lossy(&pdf_bytes);
    assert!(pdf_bytes.starts_with(b"%PDF-1.4"));
    assert!(pdf_text.contains(" re f Q") && pdf_text.contains(" c "));
    assert!(!pdf_text.contains("/Subtype /Image"));

    // Structured inspect reports chartCount and chart summary
    let insp_req = req.replace(
        r#""targetFormat": "xlsx""#,
        r#""action": "inspect","sourceFormat":"xlsx""#,
    );
    let insp_res = EasyOfficeIpcRouter::dispatch("convert_document_format", &insp_req).unwrap();
    let insp_obj = parse_obj(&insp_res);
    let insp_s0 = field(&insp_obj, "sheets").as_array().unwrap()[0]
        .as_object()
        .unwrap();
    assert_eq!(field(insp_s0, "chartCount").as_u64(), Some(1));
    assert_eq!(field(insp_s0, "charts").as_array().unwrap().len(), 1);
}

#[test]
fn row_and_column_structure_edits_shift_chart_ranges_series_and_anchors() {
    let open = EasyOfficeIpcRouter::dispatch(
        "evaluate_sheet",
        r#"{"sessionAction":"open","sheets":["S","Other"]}"#,
    )
    .unwrap();
    let session = field(&parse_obj(&open), "session")
        .as_str()
        .unwrap()
        .to_string();

    // 1. Insert 2 rows before row 2 (1-based)
    let insert_row_req = format!(
        r#"{{
            "sessionAction": "structure",
            "session": "{session}",
            "sheet": "S",
            "axis": "row",
            "op": "insert",
            "at": 2,
            "count": 2,
            "charts": [{{
                "id": "c1",
                "chartType": "column",
                "title": "Sales",
                "dataRange": "A1:C5",
                "switchRowCol": false,
                "legend": "right",
                "anchor": {{"fromCol": 4, "fromRow": 1, "toCol": 10, "toRow": 12, "width": 480, "height": 264}},
                "series": [
                    {{"name": "A", "nameRef": "S!$B$1", "categoriesRef": "S!$A$2:$A$5", "valuesRef": "S!$B$2:$B$5"}},
                    {{"name": "Ext", "nameRef": "Other!$B$1", "categoriesRef": "Other!$A$2:$A$5", "valuesRef": "Other!$B$2:$B$5"}}
                ]
            }}]
        }}"#
    );
    let res = EasyOfficeIpcRouter::dispatch("evaluate_sheet", &insert_row_req).unwrap();
    let res_obj = parse_obj(&res);
    let charts = field(&res_obj, "charts").as_array().unwrap();
    let ch = charts[0].as_object().unwrap();
    assert_eq!(field(ch, "dataRange").as_str(), Some("A1:C7"));

    let anchor = field(ch, "anchor").as_object().unwrap();
    assert_eq!(field(anchor, "fromRow").as_u64(), Some(3));
    assert_eq!(field(anchor, "toRow").as_u64(), Some(14));

    let series = field(ch, "series").as_array().unwrap();
    let s0 = series[0].as_object().unwrap();
    assert_eq!(field(s0, "nameRef").as_str(), Some("S!$B$1"));
    assert_eq!(field(s0, "categoriesRef").as_str(), Some("S!$A$4:$A$7"));
    assert_eq!(field(s0, "valuesRef").as_str(), Some("S!$B$4:$B$7"));
    // Cross-sheet reference stays untouched
    let s1 = series[1].as_object().unwrap();
    assert_eq!(field(s1, "valuesRef").as_str(), Some("Other!$B$2:$B$5"));

    // 2. Delete 1 column at col 2 (B, 1-based)
    let del_col_req = format!(
        r#"{{
            "sessionAction": "structure",
            "session": "{session}",
            "sheet": "S",
            "axis": "col",
            "op": "delete",
            "at": 2,
            "count": 1,
            "charts": [{{
                "id": "c2",
                "chartType": "bar",
                "title": "ColDel",
                "dataRange": "A1:D5",
                "anchor": {{"fromCol": 5, "fromRow": 0, "toCol": 11, "toRow": 10, "width": 480, "height": 240}},
                "series": [
                    {{"name": "C", "nameRef": "S!$C$1", "categoriesRef": "S!$A$2:$A$5", "valuesRef": "S!$C$2:$C$5"}}
                ]
            }}]
        }}"#
    );
    let del_res = EasyOfficeIpcRouter::dispatch("evaluate_sheet", &del_col_req).unwrap();
    let del_obj = parse_obj(&del_res);
    let del_charts = field(&del_obj, "charts").as_array().unwrap();
    let ch2 = del_charts[0].as_object().unwrap();
    assert_eq!(field(ch2, "dataRange").as_str(), Some("A1:C5"));
    let anchor2 = field(ch2, "anchor").as_object().unwrap();
    assert_eq!(field(anchor2, "fromCol").as_u64(), Some(4));
    assert_eq!(field(anchor2, "toCol").as_u64(), Some(10));
    let s2 = field(ch2, "series").as_array().unwrap()[0]
        .as_object()
        .unwrap();
    assert_eq!(field(s2, "nameRef").as_str(), Some("S!$B$1"));
    assert_eq!(field(s2, "valuesRef").as_str(), Some("S!$B$2:$B$5"));
}

#[test]
fn inspect_readback_and_cli_report_chart_summary_and_rejected_warnings() {
    // Build a package with 1 valid chart and 1 broken chart target (missing chart99.xml)
    let pkg = ZipArchiveCodec::pack_compressed(&[
        ("xl/workbook.xml".to_string(), br#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Summary" sheetId="1" r:id="rId1"/></sheets></workbook>"#.to_vec()),
        ("xl/_rels/workbook.xml.rels".to_string(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#.to_vec()),
        ("xl/worksheets/sheet1.xml".to_string(), br#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Cat</t></is></c><c r="B1" t="inlineStr"><is><t>Val</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>X</t></is></c><c r="B2" t="n"><v>42</v></c></row></sheetData><drawing r:id="rIdD1"/></worksheet>"#.to_vec()),
        ("xl/worksheets/_rels/sheet1.xml.rels".to_string(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdD1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/drawing1.xml"/></Relationships>"#.to_vec()),
        ("xl/drawings/drawing1.xml".to_string(), concat!(
            r#"<?xml version="1.0"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">"#,
            r#"<xdr:twoCellAnchor><xdr:from><xdr:col>3</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>0</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>9</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>11</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:graphicFrame><xdr:nvGraphicFramePr><xdr:cNvPr id="1" name="kpi-pie"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="rId1"/></a:graphicData></xdr:graphic></xdr:graphicFrame></xdr:twoCellAnchor>"#,
            r#"<xdr:twoCellAnchor><xdr:from><xdr:col>3</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>12</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>9</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>23</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:graphicFrame><xdr:nvGraphicFramePr><xdr:cNvPr id="2" name="broken"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="rIdMissing"/></a:graphicData></xdr:graphic></xdr:graphicFrame></xdr:twoCellAnchor>"#,
            r#"</xdr:wsDr>"#
        ).as_bytes().to_vec()),
        ("xl/drawings/_rels/drawing1.xml.rels".to_string(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart" Target="../charts/chart1.xml"/></Relationships>"#.to_vec()),
        ("xl/charts/chart1.xml".to_string(), br#"<?xml version="1.0"?><c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>KPI Pie</a:t></a:r></a:p></c:rich></c:tx></c:title><c:plotArea><c:pieChart><c:ser><c:idx val="0"/><c:order val="0"/><c:tx><c:v>Val</c:v></c:tx><c:cat><c:strRef><c:f>Summary!$A$2:$A$2</c:f></c:strRef></c:cat><c:val><c:numRef><c:f>Summary!$B$2:$B$2</c:f></c:numRef></c:val></c:ser></c:pieChart></c:plotArea><c:legend><c:legendPos val="r"/></c:legend></c:chart></c:chartSpace>"#.to_vec()),
    ]).unwrap();

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp_path = std::env::temp_dir().join(format!("pd254_inspect_{nanos}.xlsx"));
    fs::write(&tmp_path, &pkg).unwrap();

    let cli_out = run_inspect_cli(&InspectArgs {
        path: tmp_path.to_string_lossy().to_string(),
        sheet: None,
        range: None,
        max_items: None,
    });
    let _ = fs::remove_file(&tmp_path);
    let report = parse_obj(&cli_out.expect("--inspect cli output"));

    let warnings = field(&report, "warnings").as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().is_some_and(|s| s.contains("차트 1개"))),
        "missing rejected chart warning: {warnings:?}"
    );

    let sheets = field(&report, "sheets").as_array().unwrap();
    let s0 = sheets[0].as_object().unwrap();
    assert_eq!(field(s0, "chartCount").as_u64(), Some(1));
    let charts = field(s0, "charts").as_array().unwrap();
    assert_eq!(charts.len(), 1);
    let ch = charts[0].as_object().unwrap();
    assert_eq!(field(ch, "id").as_str(), Some("kpi-pie"));
    assert_eq!(field(ch, "chartType").as_str(), Some("pie"));
    assert_eq!(field(ch, "title").as_str(), Some("KPI Pie"));
    assert_eq!(field(ch, "seriesCount").as_u64(), Some(1));

    let checks = field(&report, "checks").as_object().unwrap();
    let rejected = field(checks, "rejectedCounts").as_object().unwrap();
    assert_eq!(field(rejected, "charts").as_u64(), Some(1));
}

#[test]
fn chart_json_and_ipc_budgets_fail_closed() {
    // 1. Over MAX_CHARTS_PER_SHEET -> ResourceBudgetExceeded
    let many_charts: Vec<String> = (0..=MAX_CHARTS_PER_SHEET)
        .map(|i| format!(r#"{{"id":"c{i}","chartType":"column"}}"#))
        .collect();
    let req_many = format!(
        r#"{{"targetFormat":"xlsx","sheets":[{{"name":"S1","cells":[],"charts":[{}]}}]}}"#,
        many_charts.join(",")
    );
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("convert_document_format", &req_many),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 2. Over MAX_SERIES_PER_CHART -> ResourceBudgetExceeded
    let many_series: Vec<String> = (0..=MAX_SERIES_PER_CHART)
        .map(|i| format!(r#"{{"name":"S{i}","valuesRef":"S1!$A$1:$A$2"}}"#))
        .collect();
    let req_series = format!(
        r#"{{"targetFormat":"xlsx","sheets":[{{"name":"S1","cells":[],"charts":[{{"id":"c1","chartType":"line","series":[{}]}}]}}]}}"#,
        many_series.join(",")
    );
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("convert_document_format", &req_series),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 3. Over MAX_POINTS_PER_SERIES -> ResourceBudgetExceeded
    let req_points = format!(
        r#"{{"targetFormat":"xlsx","sheets":[{{"name":"S1","cells":[],"charts":[{{"id":"c1","chartType":"bar","series":[{{"name":"S","valuesRef":"S1!$A$1:$A${}"}}]}}]}}]}}"#,
        MAX_POINTS_PER_SERIES + 2
    );
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("convert_document_format", &req_points),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 4. Over MAX_CHART_TEXT_BYTES -> ResourceBudgetExceeded
    let long_title = "a".repeat(MAX_CHART_TEXT_BYTES + 1);
    let req_text = format!(
        r#"{{"targetFormat":"xlsx","sheets":[{{"name":"S1","cells":[],"charts":[{{"id":"c1","chartType":"pie","title":"{long_title}"}}]}}]}}"#
    );
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("convert_document_format", &req_text),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 5. Workbook-wide chart limit across 5 sheets (257 > 256) -> ResourceBudgetExceeded
    let full_sheet_charts: Vec<String> = (0..MAX_CHARTS_PER_SHEET)
        .map(|i| format!(r#"{{"id":"c{i}","chartType":"column"}}"#))
        .collect();
    let full_charts_str = full_sheet_charts.join(",");
    let req_wb = format!(
        r#"{{"targetFormat":"xlsx","sheets":[
            {{"name":"S1","cells":[],"charts":[{full_charts_str}]}},
            {{"name":"S2","cells":[],"charts":[{full_charts_str}]}},
            {{"name":"S3","cells":[],"charts":[{full_charts_str}]}},
            {{"name":"S4","cells":[],"charts":[{full_charts_str}]}},
            {{"name":"S5","cells":[],"charts":[{{"id":"extra","chartType":"column"}}]}}
        ]}}"#
    );
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("convert_document_format", &req_wb),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 6. Invalid chartType, legend, and anchor dimensions -> InvalidArgument
    for bad in [
        r#"{"targetFormat":"xlsx","sheets":[{"name":"S1","cells":[],"charts":[{"id":"c1","chartType":"radar"}]}]}"#,
        r#"{"targetFormat":"xlsx","sheets":[{"name":"S1","cells":[],"charts":[{"id":"c1","chartType":"column","legend":"center"}]}]}"#,
        r#"{"targetFormat":"xlsx","sheets":[{"name":"S1","cells":[],"charts":[{"id":"c1","chartType":"column","anchor":{"width":50,"height":260}}]}]}"#,
        r#"{"targetFormat":"xlsx","sheets":[{"name":"S1","cells":[],"charts":[{"id":"c1","chartType":"column","anchor":{"width":420,"height":9999}}]}]}"#,
    ] {
        assert!(
            matches!(
                EasyOfficeIpcRouter::dispatch("convert_document_format", bad),
                Err(EasyOfficeError::InvalidArgument(_))
            ),
            "{bad}"
        );
    }
}

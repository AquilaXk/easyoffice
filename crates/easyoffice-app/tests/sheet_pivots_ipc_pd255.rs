//! PD-255: Dynamic multi-dimensional pivot IPC, structured pivot JSON round-trip,
//! row/column structure shifts, `--inspect` readback & rejected warnings, and fail-closed budgets.

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use easyoffice_app::cli_args::InspectArgs;
use easyoffice_app::inspect_cli::run_inspect_cli;
use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{
    EasyOfficeError, StrictJsonValue, ZipArchiveCodec, MAX_PIVOTS_PER_SHEET, MAX_PIVOT_AXIS_FIELDS,
    MAX_PIVOT_FILTER_VALUES, MAX_PIVOT_SLICERS, MAX_PIVOT_TEXT_BYTES, MAX_PIVOT_VALUE_FIELDS,
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

fn as_f64(v: &StrictJsonValue) -> Option<f64> {
    match v {
        StrictJsonValue::Number(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

#[test]
fn dynamic_pivot_ipc_multi_dim_filters_cross_availability_and_legacy_compat() {
    let cells = [
        ("A1", "지역"),
        ("B1", "카테고리"),
        ("C1", "분기"),
        ("D1", "채널"),
        ("E1", "매출"),
        ("A2", "서울"),
        ("B2", "전자"),
        ("C2", "1Q"),
        ("D2", "온라인"),
        ("E2", "100"),
        ("A3", "서울"),
        ("B3", "가구"),
        ("C3", "1Q"),
        ("D3", "매장"),
        ("E3", "50"),
        ("A4", "부산"),
        ("B4", "전자"),
        ("C4", "2Q"),
        ("D4", ""),
        ("E4", "=E2*2"),
        ("A5", "대구"),
        ("B5", "가구"),
        ("C5", "2Q"),
        ("D5", "온라인"),
        ("E5", "80"),
    ]
    .iter()
    .map(|(c, v)| format!(r#"{{"cell":"{c}","value":"{v}"}}"#))
    .collect::<Vec<_>>()
    .join(",");

    // 1. Legacy 1D request remains 100% compatible
    let legacy = EasyOfficeIpcRouter::dispatch(
        "evaluate_sheet",
        &format!(
            r#"{{"sessionAction":"pivot","sheet":"S","cells":[{cells}],"range":"A1:E5","row":0,"col":2,"value":4,"agg":"sum"}}"#
        ),
    )
    .unwrap();
    let leg_obj = parse_obj(&legacy);
    assert_eq!(
        field(&leg_obj, "rowKeys")
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["서울", "부산", "대구"]
    );
    assert_eq!(as_f64(field(&leg_obj, "grandTotal")), Some(430.0));

    // 2. Dynamic multi-dimensional + filter + cross-availability + multi-value request
    let dyn_res = EasyOfficeIpcRouter::dispatch(
        "evaluate_sheet",
        &format!(
            r#"{{
                "sessionAction":"pivot",
                "sheet":"S",
                "cells":[{cells}],
                "range":"A1:E5",
                "header":true,
                "rowFields":[0,1],
                "colFields":[2],
                "filterFields":[3],
                "valueFields":[{{"col":4,"agg":"sum"}},{{"col":4,"agg":"count"}}],
                "filters":[
                    {{"col":1,"values":["전자"],"blank":false}},
                    {{"col":3,"values":["온라인"],"blank":true}}
                ]
            }}"#
        ),
    )
    .unwrap();
    let obj = parse_obj(&dyn_res);
    // Rows matching (Category == "전자") AND (Channel == "온라인" OR blank): row 2 (서울,전자,1Q,온라인,100) and row 4 (부산,전자,2Q,"",200)
    let row_tuples = field(&obj, "rowTuples").as_array().unwrap();
    assert_eq!(row_tuples.len(), 2);
    assert_eq!(
        row_tuples[0]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["서울", "전자"]
    );
    assert_eq!(
        row_tuples[1]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["부산", "전자"]
    );
    let col_tuples = field(&obj, "colTuples").as_array().unwrap();
    assert_eq!(col_tuples.len(), 2);

    // valuesMatrix[r][c][v]
    let vm = field(&obj, "valuesMatrix").as_array().unwrap();
    // (서울, 전자) @ 1Q -> sum=100, count=1
    let row0 = vm[0].as_array().unwrap();
    let cell00 = row0[0].as_array().unwrap();
    assert_eq!(as_f64(&cell00[0]), Some(100.0));
    assert_eq!(as_f64(&cell00[1]), Some(1.0));
    // (부산, 전자) @ 2Q -> sum=200, count=1
    let row1 = vm[1].as_array().unwrap();
    let cell11 = row1[1].as_array().unwrap();
    assert_eq!(as_f64(&cell11[0]), Some(200.0));
    assert_eq!(as_f64(&cell11[1]), Some(1.0));

    let vgt = field(&obj, "valueGrandTotals").as_array().unwrap();
    assert_eq!(as_f64(&vgt[0]), Some(300.0));
    assert_eq!(as_f64(&vgt[1]), Some(2.0));
    // Compat fields follow valueFields[0]
    assert_eq!(as_f64(field(&obj, "grandTotal")), Some(300.0));

    // Pre-filter fieldItems contains all distinct values per field
    let fi = field(&obj, "fieldItems").as_object().unwrap();
    let fi_region = field(fi, "0").as_array().unwrap();
    assert_eq!(fi_region.len(), 3); // 서울, 부산, 대구

    // Cross-filtered availableFieldItems for Category (col 1): ignoring col 1's own filter,
    // rows passing Channel in {"온라인", blank} include 대구(가구,온라인) -> 전자 & 가구 both available!
    let afi = field(&obj, "availableFieldItems").as_object().unwrap();
    let afi_cat = field(afi, "1").as_array().unwrap();
    assert_eq!(afi_cat.len(), 2);
    // For Region (col 0): only rows matching 전자 & (온라인|blank) -> 서울, 부산 (not 대구)
    let afi_region = field(afi, "0").as_array().unwrap();
    assert_eq!(afi_region.len(), 2);

    // 3. Empty rowFields with valueFields works
    let empty_rows_res = EasyOfficeIpcRouter::dispatch(
        "evaluate_sheet",
        &format!(
            r#"{{
                "sessionAction":"pivot",
                "sheet":"S",
                "cells":[{cells}],
                "range":"A1:E5",
                "rowFields":[],
                "colFields":[2],
                "valueFields":[{{"col":4,"agg":"sum"}}]
            }}"#
        ),
    )
    .unwrap();
    let er_obj = parse_obj(&empty_rows_res);
    assert_eq!(field(&er_obj, "rowTuples").as_array().unwrap().len(), 1);
    assert_eq!(as_f64(field(&er_obj, "grandTotal")), Some(430.0));

    // 4. Whole-column range ("A:E") works fast without scanning 1M empty rows
    let whole_col_res = EasyOfficeIpcRouter::dispatch(
        "evaluate_sheet",
        &format!(
            r#"{{
                "sessionAction":"pivot",
                "sheet":"S",
                "cells":[{cells}],
                "range":"A:E",
                "rowFields":[0],
                "colFields":[],
                "valueFields":[{{"col":4,"agg":"sum"}}]
            }}"#
        ),
    )
    .unwrap();
    let wc_obj = parse_obj(&whole_col_res);
    assert_eq!(as_f64(field(&wc_obj, "grandTotal")), Some(430.0));
}

#[test]
fn structured_save_decode_round_trips_pivots_json_schema() {
    let req = r#"{
        "targetFormat": "xlsx",
        "title": "피벗 보고서",
        "sheets": [
            {
                "name": "데이터",
                "cells": [
                    {"ref": "A1", "raw": "부서"},
                    {"ref": "B1", "raw": "분기"},
                    {"ref": "C1", "raw": "매출"},
                    {"ref": "A2", "raw": "영업"},
                    {"ref": "B2", "raw": "1Q"},
                    {"ref": "C2", "raw": "500"}
                ]
            },
            {
                "name": "요약",
                "cells": [{"ref": "A1", "raw": "요약"}],
                "pivots": [{
                    "name": "피벗 테이블1",
                    "cacheId": 1,
                    "sourceSheet": "데이터",
                    "sourceRange": "A1:C2",
                    "sourceTable": null,
                    "hasHeader": true,
                    "location": "B2:D6",
                    "fieldNames": ["부서", "분기", "매출"],
                    "rowFields": [0],
                    "colFields": [1],
                    "filterFields": [],
                    "valueFields": [{"col": 2, "agg": "sum", "name": "합계:매출"}],
                    "filters": [{"col": 0, "values": ["영업"], "blank": false}],
                    "slicers": [{"col": 0, "caption": "부서", "left": 300, "top": 40, "width": 184, "height": 210, "multiSelect": true}]
                }]
            }
        ]
    }"#;

    let saved = EasyOfficeIpcRouter::dispatch("convert_document_format", req).unwrap();
    let b64 = field(&parse_obj(&saved), "convertedBase64")
        .as_str()
        .unwrap()
        .to_string();

    let dec_req =
        format!(r#"{{"action":"decode","fileName":"pivot.xlsx","sourceBase64":"{b64}"}}"#);
    let decoded = EasyOfficeIpcRouter::dispatch("convert_document_format", &dec_req).unwrap();
    let dec_obj = parse_obj(&decoded);
    let sheets = field(&dec_obj, "sheets").as_array().unwrap();
    let s1 = sheets[1].as_object().unwrap();
    let pivots = field(s1, "pivots").as_array().unwrap();
    assert_eq!(pivots.len(), 1);
    let p = pivots[0].as_object().unwrap();
    assert_eq!(field(p, "name").as_str(), Some("피벗 테이블1"));
    assert_eq!(field(p, "cacheId").as_u64(), Some(1));
    assert_eq!(field(p, "sourceSheet").as_str(), Some("데이터"));
    assert_eq!(field(p, "sourceRange").as_str(), Some("A1:C2"));
    assert!(matches!(field(p, "sourceTable"), StrictJsonValue::Null));
    assert_eq!(field(p, "hasHeader").as_bool(), Some(true));
    assert_eq!(field(p, "location").as_str(), Some("B2:D6"));
    assert_eq!(field(p, "rowFields").as_array().unwrap().len(), 1);
    assert_eq!(field(p, "colFields").as_array().unwrap().len(), 1);
    assert_eq!(field(p, "valueFields").as_array().unwrap().len(), 1);
    assert_eq!(field(p, "filters").as_array().unwrap().len(), 1);
    assert_eq!(field(p, "slicers").as_array().unwrap().len(), 1);
}

#[test]
fn row_and_column_structure_edits_shift_pivot_location_and_source_range() {
    let open = EasyOfficeIpcRouter::dispatch(
        "evaluate_sheet",
        r#"{"sessionAction":"open","sheets":["S","Other"]}"#,
    )
    .unwrap();
    let session = field(&parse_obj(&open), "session")
        .as_str()
        .unwrap()
        .to_string();

    // 1. Insert 2 rows before row 2 on S -> shifts same-sheet sourceRange and location, keeps cross-sheet sourceRange
    let req = format!(
        r#"{{
            "sessionAction": "structure",
            "session": "{session}",
            "sheet": "S",
            "axis": "row",
            "op": "insert",
            "at": 2,
            "count": 2,
            "pivots": [
                {{
                    "name": "SameSheet",
                    "cacheId": 1,
                    "sourceSheet": "S",
                    "sourceRange": "A1:C5",
                    "sourceTable": null,
                    "hasHeader": true,
                    "location": "E3:G8",
                    "fieldNames": ["A","B","C"],
                    "rowFields": [0],
                    "colFields": [1],
                    "filterFields": [],
                    "valueFields": [{{"col": 2, "agg": "sum", "name": "Sum C"}}],
                    "filters": [],
                    "slicers": []
                }},
                {{
                    "name": "CrossSheet",
                    "cacheId": 2,
                    "sourceSheet": "Other",
                    "sourceRange": "A1:C5",
                    "sourceTable": null,
                    "hasHeader": true,
                    "location": "E10:G15",
                    "fieldNames": ["A","B","C"],
                    "rowFields": [0],
                    "colFields": [],
                    "filterFields": [],
                    "valueFields": [{{"col": 2, "agg": "sum", "name": "Sum C"}}],
                    "filters": [],
                    "slicers": []
                }}
            ]
        }}"#
    );
    let res = EasyOfficeIpcRouter::dispatch("evaluate_sheet", &req).unwrap();
    let res_obj = parse_obj(&res);
    let pivots = field(&res_obj, "pivots").as_array().unwrap();
    assert_eq!(pivots.len(), 2);
    let p0 = pivots[0].as_object().unwrap();
    assert_eq!(field(p0, "sourceRange").as_str(), Some("A1:C7"));
    assert_eq!(field(p0, "location").as_str(), Some("E5:G10"));

    let p1 = pivots[1].as_object().unwrap();
    assert_eq!(field(p1, "sourceRange").as_str(), Some("A1:C5"));
    assert_eq!(field(p1, "location").as_str(), Some("E12:G17"));

    // 2. Delete column B (col 2, 1-based) on S -> sourceRange A1:C5 shrinks to A1:B5, fieldNames/colFields/valueFields remap!
    let col_del_req = format!(
        r#"{{
            "sessionAction": "structure",
            "session": "{session}",
            "sheet": "S",
            "axis": "col",
            "op": "delete",
            "at": 2,
            "count": 1,
            "pivots": [{{
                "name": "ColEdit",
                "cacheId": 1,
                "sourceSheet": "S",
                "sourceRange": "A1:C5",
                "sourceTable": null,
                "hasHeader": true,
                "location": "E1:G6",
                "fieldNames": ["A","B","C"],
                "rowFields": [0],
                "colFields": [1],
                "filterFields": [],
                "valueFields": [{{"col": 2, "agg": "sum", "name": "Sum C"}}],
                "filters": [],
                "slicers": []
            }}]
        }}"#
    );
    let del_res = EasyOfficeIpcRouter::dispatch("evaluate_sheet", &col_del_req).unwrap();
    let del_obj = parse_obj(&del_res);
    let del_pivots = field(&del_obj, "pivots").as_array().unwrap();
    let dp = del_pivots[0].as_object().unwrap();
    assert_eq!(field(dp, "sourceRange").as_str(), Some("A1:B5"));
    assert_eq!(field(dp, "location").as_str(), Some("D1:F6"));
    assert_eq!(
        field(dp, "fieldNames")
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["A", "C"]
    );
    assert!(field(dp, "colFields").as_array().unwrap().is_empty());
    let vf = field(dp, "valueFields").as_array().unwrap()[0]
        .as_object()
        .unwrap();
    assert_eq!(field(vf, "col").as_u64(), Some(1));
}

#[test]
fn inspect_readback_and_cli_report_pivot_summary_and_rejected_warnings() {
    let pkg = ZipArchiveCodec::pack_compressed(&[
        ("xl/workbook.xml".to_string(), br#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Summary" sheetId="1" r:id="rId1"/></sheets><pivotCaches><pivotCache cacheId="1" r:id="rIdPC1"/></pivotCaches></workbook>"#.to_vec()),
        ("xl/_rels/workbook.xml.rels".to_string(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rIdPC1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheDefinition" Target="pivotCache/pivotCacheDefinition1.xml"/></Relationships>"#.to_vec()),
        ("xl/worksheets/sheet1.xml".to_string(), br#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Cat</t></is></c><c r="B1" t="inlineStr"><is><t>Val</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>X</t></is></c><c r="B2" t="n"><v>42</v></c></row></sheetData></worksheet>"#.to_vec()),
        ("xl/worksheets/_rels/sheet1.xml.rels".to_string(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdPT1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotTable" Target="../pivotTables/pivotTable1.xml"/><Relationship Id="rIdPTBad" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotTable" Target="../pivotTables/missing.xml"/></Relationships>"#.to_vec()),
        ("xl/pivotCache/pivotCacheDefinition1.xml".to_string(), br#"<?xml version="1.0"?><pivotCacheDefinition xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" recordCount="1"><cacheSource type="worksheet"><worksheetSource ref="A1:B2" sheet="Summary"/></cacheSource><cacheFields count="2"><cacheField name="Cat" numFmtId="0"/><cacheField name="Val" numFmtId="0"/></cacheFields></pivotCacheDefinition>"#.to_vec()),
        ("xl/pivotTables/pivotTable1.xml".to_string(), br#"<?xml version="1.0"?><pivotTableDefinition xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" name="KPI_Pivot" cacheId="1" dataCaption="Values"><location ref="D1:E3" firstHeaderRow="1" firstDataRow="2" firstDataCol="1"/><rowFields count="1"><field x="0"/></rowFields><dataFields count="1"><dataField name="Sum Val" fld="1" subtotal="sum" baseField="0" baseItem="0"/></dataFields></pivotTableDefinition>"#.to_vec()),
    ]).unwrap();

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp_path = std::env::temp_dir().join(format!("pd255_inspect_{nanos}.xlsx"));
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
            .any(|w| w.as_str().is_some_and(|s| s.contains("피벗 테이블 1개"))),
        "missing rejected pivot warning: {warnings:?}"
    );

    let sheets = field(&report, "sheets").as_array().unwrap();
    let s0 = sheets[0].as_object().unwrap();
    assert_eq!(field(s0, "pivotCount").as_u64(), Some(1));
    let pivots = field(s0, "pivots").as_array().unwrap();
    assert_eq!(pivots.len(), 1);
    assert_eq!(
        field(pivots[0].as_object().unwrap(), "name").as_str(),
        Some("KPI_Pivot")
    );

    let checks = field(&report, "checks").as_object().unwrap();
    let rejected = field(checks, "rejectedCounts").as_object().unwrap();
    assert_eq!(field(rejected, "pivots").as_u64(), Some(1));
}

#[test]
fn pivot_json_and_ipc_budgets_fail_closed() {
    // 1. Over MAX_PIVOTS_PER_SHEET
    let many_pivots: Vec<String> = (0..=MAX_PIVOTS_PER_SHEET)
        .map(|i| {
            format!(
                r#"{{"name":"P{i}","cacheId":1,"sourceSheet":"S","sourceRange":"A1:B2","location":"D1:E3","rowFields":[0],"colFields":[],"filterFields":[],"valueFields":[{{"col":1,"agg":"sum","name":"V"}}]}}"#
            )
        })
        .collect();
    let req = format!(
        r#"{{"targetFormat":"xlsx","sheets":[{{"name":"S","cells":[{{"ref":"A1","raw":"k"}}],"pivots":[{}]}}]}}"#,
        many_pivots.join(",")
    );
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("convert_document_format", &req),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 2. Over MAX_PIVOT_AXIS_FIELDS
    let too_many_fields: Vec<String> = (0..=MAX_PIVOT_AXIS_FIELDS).map(|_| "0".into()).collect();
    let req_axis = format!(
        r#"{{"targetFormat":"xlsx","sheets":[{{"name":"S","cells":[],"pivots":[{{"name":"P","cacheId":1,"sourceSheet":"S","sourceRange":"A1:B2","location":"D1:E3","rowFields":[{}],"colFields":[],"filterFields":[],"valueFields":[{{"col":1,"agg":"sum","name":"V"}}]}}]}}]}}"#,
        too_many_fields.join(",")
    );
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("convert_document_format", &req_axis),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 3. Over MAX_PIVOT_VALUE_FIELDS
    let too_many_vals: Vec<String> = (0..=MAX_PIVOT_VALUE_FIELDS)
        .map(|i| format!(r#"{{"col":1,"agg":"sum","name":"V{i}"}}"#))
        .collect();
    let req_vals = format!(
        r#"{{"targetFormat":"xlsx","sheets":[{{"name":"S","cells":[],"pivots":[{{"name":"P","cacheId":1,"sourceSheet":"S","sourceRange":"A1:B2","location":"D1:E3","rowFields":[0],"colFields":[],"filterFields":[],"valueFields":[{}]}}]}}]}}"#,
        too_many_vals.join(",")
    );
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("convert_document_format", &req_vals),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 4. Over MAX_PIVOT_SLICERS
    let too_many_slicers: Vec<String> = (0..=MAX_PIVOT_SLICERS)
        .map(|i| format!(r#"{{"col":0,"caption":"S{i}","left":10,"top":10,"width":180,"height":200,"multiSelect":false}}"#))
        .collect();
    let req_slicers = format!(
        r#"{{"targetFormat":"xlsx","sheets":[{{"name":"S","cells":[],"pivots":[{{"name":"P","cacheId":1,"sourceSheet":"S","sourceRange":"A1:B2","location":"D1:E3","rowFields":[0],"colFields":[],"filterFields":[],"valueFields":[{{"col":1,"agg":"sum","name":"V"}}],"slicers":[{}]}}]}}]}}"#,
        too_many_slicers.join(",")
    );
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("convert_document_format", &req_slicers),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 5. Over MAX_PIVOT_FILTER_VALUES
    let too_many_fvals: Vec<String> = (0..=MAX_PIVOT_FILTER_VALUES)
        .map(|i| format!(r#""v{i}""#))
        .collect();
    let req_fvals = format!(
        r#"{{"targetFormat":"xlsx","sheets":[{{"name":"S","cells":[],"pivots":[{{"name":"P","cacheId":1,"sourceSheet":"S","sourceRange":"A1:B2","location":"D1:E3","rowFields":[0],"colFields":[],"filterFields":[],"valueFields":[{{"col":1,"agg":"sum","name":"V"}}],"filters":[{{"col":0,"values":[{}],"blank":false}}]}}]}}]}}"#,
        too_many_fvals.join(",")
    );
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("convert_document_format", &req_fvals),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 6. Over MAX_PIVOT_TEXT_BYTES
    let long_name = "x".repeat(MAX_PIVOT_TEXT_BYTES + 1);
    let req_text = format!(
        r#"{{"targetFormat":"xlsx","sheets":[{{"name":"S","cells":[],"pivots":[{{"name":"{long_name}","cacheId":1,"sourceSheet":"S","sourceRange":"A1:B2","location":"D1:E3","rowFields":[0],"colFields":[],"filterFields":[],"valueFields":[{{"col":1,"agg":"sum","name":"V"}}]}}]}}]}}"#,
    );
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("convert_document_format", &req_text),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 7. Empty rowFields AND empty valueFields -> InvalidArgument
    let req_empty = r#"{"targetFormat":"xlsx","sheets":[{"name":"S","cells":[],"pivots":[{"name":"P","cacheId":1,"sourceSheet":"S","sourceRange":"A1:B2","location":"D1:E3","rowFields":[],"colFields":[],"filterFields":[],"valueFields":[]}]}]}"#;
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("convert_document_format", req_empty),
        Err(EasyOfficeError::InvalidArgument(_))
    ));
}

//! PD-256: Structured sheet image & cell note JSON round-trip, `structure` shift
//! (lightweight images without `data` + `imageId` preservation + `edit.cols` partial notes),
//! `--inspect` readback summaries & warnings, and fail-closed JSON budget tests.

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use easyoffice_app::cli_args::InspectArgs;
use easyoffice_app::inspect_cli::run_inspect_cli;
use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{
    base64_codec, deflate_zlib, EasyOfficeError, IeeeCrc32, StrictJsonValue, ZipArchiveCodec,
    MAX_NOTES_PER_SHEET, MAX_SHEET_IMAGES_PER_SHEET, MAX_SHEET_NOTE_TEXT_BYTES,
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

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = IeeeCrc32::update(IeeeCrc32::checksum(kind), data);
    out.extend_from_slice(&crc.to_be_bytes());
    out
}

fn make_png(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
    let stride = 1 + width as usize * 4;
    let mut scanline = Vec::with_capacity(stride);
    scanline.push(0);
    for _ in 0..width {
        scanline.extend_from_slice(&rgba);
    }
    let mut raw = Vec::with_capacity(stride * height as usize);
    for _ in 0..height {
        raw.extend_from_slice(&scanline);
    }
    let compressed = deflate_zlib(&raw);
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    out.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
    out.extend_from_slice(&png_chunk(b"IDAT", &compressed));
    out.extend_from_slice(&png_chunk(b"IEND", &[]));
    out
}

fn make_jpeg(width: u16, height: u16) -> Vec<u8> {
    vec![
        0xFF,
        0xD8,
        0xFF,
        0xE0,
        0x00,
        0x10,
        b'J',
        b'F',
        b'I',
        b'F',
        0x00,
        0x01,
        0x01,
        0x00,
        0x00,
        0x01,
        0x00,
        0x01,
        0x00,
        0x00,
        0xFF,
        0xC0,
        0x00,
        0x11,
        8,
        (height >> 8) as u8,
        height as u8,
        (width >> 8) as u8,
        width as u8,
        3,
        1,
        0x11,
        0x00,
        2,
        0x11,
        0x00,
        3,
        0x11,
        0x00,
        0xFF,
        0xDA,
        0x00,
        0x0C,
        3,
        1,
        0,
        2,
        0,
        3,
        0,
        0,
        63,
        0,
        0x00,
        0xFF,
        0xD9,
    ]
}

#[test]
fn structured_save_decode_pdf_and_inspect_preserve_images_and_notes() {
    let png = make_png(8, 6, [20, 120, 220, 255]);
    let jpg = make_jpeg(16, 12);
    let png_b64 = base64_codec::encode(&png);
    let jpg_b64 = base64_codec::encode(&jpg);

    let req = format!(
        r#"{{
            "targetFormat": "xlsx",
            "title": "미디어 메모 통합",
            "sheets": [{{
                "name": "요약",
                "cells": [
                    {{"ref": "A1", "raw": "항목"}},
                    {{"ref": "B2", "raw": "1200"}},
                    {{"ref": "C3", "raw": "완료"}}
                ],
                "images": [
                    {{
                        "id": "pic-1",
                        "name": "그림 1",
                        "alt": "대표 배지",
                        "mime": "image/png",
                        "data": "{png_b64}",
                        "anchor": {{
                            "fromCol": 1, "fromRow": 1, "colOffEmu": 9525, "rowOffEmu": 19050,
                            "toCol": 3, "toRow": 5, "toColOffEmu": 0, "toRowOffEmu": 0,
                            "leftPx": 97, "topPx": 26, "widthPx": 192, "heightPx": 96
                        }}
                    }},
                    {{
                        "id": "pic-2",
                        "name": "그림 2",
                        "alt": "사진 미리보기",
                        "data": "{jpg_b64}",
                        "anchor": {{
                            "fromCol": 4, "fromRow": 2, "toCol": 6, "toRow": 6,
                            "leftPx": 384, "topPx": 48, "widthPx": 160, "heightPx": 96
                        }}
                    }}
                ],
                "notes": [
                    {{"ref": "B2", "author": "검토자", "text": "첫 줄\n둘째 줄", "visible": false}},
                    {{"ref": "C3", "author": "", "text": "항상 표시", "visible": true}}
                ]
            }}]
        }}"#
    );

    let saved = EasyOfficeIpcRouter::dispatch("convert_document_format", &req).unwrap();
    let saved_obj = parse_obj(&saved);
    let xlsx_b64 = field(&saved_obj, "convertedBase64").as_str().unwrap();

    // Decode back via IPC and check exact fidelity
    let dec_req =
        format!(r#"{{"action":"decode","fileName":"media.xlsx","sourceBase64":"{xlsx_b64}"}}"#);
    let decoded = EasyOfficeIpcRouter::dispatch("convert_document_format", &dec_req).unwrap();
    let dec_obj = parse_obj(&decoded);
    let s0 = field(&dec_obj, "sheets").as_array().unwrap()[0]
        .as_object()
        .unwrap();

    let images = field(s0, "images").as_array().unwrap();
    assert_eq!(images.len(), 2);
    let im0 = images[0].as_object().unwrap();
    assert_eq!(field(im0, "name").as_str(), Some("그림 1"));
    assert_eq!(field(im0, "alt").as_str(), Some("대표 배지"));
    assert_eq!(field(im0, "mime").as_str(), Some("image/png"));
    assert_eq!(
        base64_codec::decode(field(im0, "data").as_str().unwrap()).unwrap(),
        png
    );
    let a0 = field(im0, "anchor").as_object().unwrap();
    assert_eq!(field(a0, "fromCol").as_u64(), Some(1));
    assert_eq!(field(a0, "fromRow").as_u64(), Some(1));
    assert_eq!(field(a0, "colOffEmu").as_u64(), Some(9525));
    assert_eq!(field(a0, "rowOffEmu").as_u64(), Some(19050));
    assert_eq!(field(a0, "toCol").as_u64(), Some(3));
    assert_eq!(field(a0, "toRow").as_u64(), Some(5));
    assert!(field(a0, "widthPx").as_u64().is_some());

    let im1 = images[1].as_object().unwrap();
    assert_eq!(field(im1, "mime").as_str(), Some("image/jpeg"));
    assert_eq!(
        base64_codec::decode(field(im1, "data").as_str().unwrap()).unwrap(),
        jpg
    );

    let notes = field(s0, "notes").as_array().unwrap();
    assert_eq!(notes.len(), 2);
    let n0 = notes[0].as_object().unwrap();
    assert_eq!(field(n0, "ref").as_str(), Some("B2"));
    assert_eq!(field(n0, "author").as_str(), Some("검토자"));
    assert_eq!(field(n0, "text").as_str(), Some("첫 줄\n둘째 줄"));
    assert_eq!(field(n0, "visible").as_bool(), Some(false));

    let n1 = notes[1].as_object().unwrap();
    assert_eq!(field(n1, "ref").as_str(), Some("C3"));
    assert_eq!(field(n1, "author").as_str(), Some(""));
    assert_eq!(field(n1, "text").as_str(), Some("항상 표시"));
    assert_eq!(field(n1, "visible").as_bool(), Some(true));

    // Structured PDF export embeds Image XObject + note red triangle path
    let pdf_req = req.replace(r#""targetFormat": "xlsx""#, r#""targetFormat": "pdf""#);
    let pdf_res = EasyOfficeIpcRouter::dispatch("convert_document_format", &pdf_req).unwrap();
    let pdf_bytes = base64_codec::decode(
        field(&parse_obj(&pdf_res), "convertedBase64")
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let pdf_str = String::from_utf8_lossy(&pdf_bytes);
    assert!(pdf_bytes.starts_with(b"%PDF-1.4"));
    assert!(pdf_str.contains("/Subtype /Image"));
    assert!(pdf_str.contains("0.82 0.14 0.14 rg"));

    // Structured inspect reports imageCount/images + noteCount/notes
    let insp_req = req.replace(
        r#""targetFormat": "xlsx""#,
        r#""action": "inspect","sourceFormat":"xlsx""#,
    );
    let insp_res = EasyOfficeIpcRouter::dispatch("convert_document_format", &insp_req).unwrap();
    let insp_obj = parse_obj(&insp_res);
    let insp_s0 = field(&insp_obj, "sheets").as_array().unwrap()[0]
        .as_object()
        .unwrap();
    assert_eq!(field(insp_s0, "imageCount").as_u64(), Some(2));
    assert_eq!(field(insp_s0, "noteCount").as_u64(), Some(2));
    let insp_im0 = field(insp_s0, "images").as_array().unwrap()[0]
        .as_object()
        .unwrap();
    assert_eq!(field(insp_im0, "id").as_str(), Some("pic-1"));
    assert_eq!(field(insp_im0, "fromCell").as_str(), Some("B2"));
    assert_eq!(field(insp_im0, "toCell").as_str(), Some("D6"));
    assert_eq!(field(insp_im0, "bytes").as_u64(), Some(png.len() as u64));
}

#[test]
fn row_and_column_structure_edits_shift_lightweight_images_and_notes() {
    let open = EasyOfficeIpcRouter::dispatch(
        "evaluate_sheet",
        r#"{"sessionAction":"open","sheets":["S1"]}"#,
    )
    .unwrap();
    let session = field(&parse_obj(&open), "session")
        .as_str()
        .unwrap()
        .to_string();

    // 1. Insert 2 rows at row 2 (1-based) with omitted `data` and `imageId` preserved
    let insert_req = format!(
        r#"{{
            "sessionAction": "structure",
            "session": "{session}",
            "sheet": "S1",
            "axis": "row",
            "op": "insert",
            "at": 2,
            "count": 2,
            "images": [{{
                "id": "pic-lite",
                "name": "그림 1",
                "alt": "로고",
                "mime": "image/png",
                "imageId": "cached-media-9",
                "anchor": {{
                    "fromCol": 1, "fromRow": 1, "colOffEmu": 0, "rowOffEmu": 0,
                    "toCol": 3, "toRow": 5, "toColOffEmu": 0, "toRowOffEmu": 0,
                    "leftPx": 96, "topPx": 24, "widthPx": 192, "heightPx": 96
                }}
            }}],
            "notes": [
                {{"ref": "B1", "author": "작성자", "text": "고정 행", "visible": false}},
                {{"ref": "B2", "author": "작성자", "text": "이동 대상", "visible": true}},
                {{"ref": "C4", "author": "작성자", "text": "하단 셀", "visible": false}}
            ]
        }}"#
    );
    let ins_res = EasyOfficeIpcRouter::dispatch("evaluate_sheet", &insert_req).unwrap();
    let ins_obj = parse_obj(&ins_res);

    let images = field(&ins_obj, "images").as_array().unwrap();
    assert_eq!(images.len(), 1);
    let im = images[0].as_object().unwrap();
    assert_eq!(field(im, "id").as_str(), Some("pic-lite"));
    assert_eq!(field(im, "imageId").as_str(), Some("cached-media-9"));
    assert!(StrictJsonValue::get_field(im, "data").is_none());
    let anchor = field(im, "anchor").as_object().unwrap();
    assert_eq!(field(anchor, "fromRow").as_u64(), Some(3));
    assert_eq!(field(anchor, "toRow").as_u64(), Some(7));

    let notes = field(&ins_obj, "notes").as_array().unwrap();
    assert_eq!(notes.len(), 3);
    assert_eq!(
        field(notes[0].as_object().unwrap(), "ref").as_str(),
        Some("B1")
    );
    assert_eq!(
        field(notes[1].as_object().unwrap(), "ref").as_str(),
        Some("B4")
    );
    assert_eq!(
        field(notes[1].as_object().unwrap(), "visible").as_bool(),
        Some(true)
    );
    assert_eq!(
        field(notes[2].as_object().unwrap(), "ref").as_str(),
        Some("C6")
    );

    // 2. Partial-column row delete (`cols: [2, 2]`, col B only) at row 4 deletes B4 and keeps C6
    let partial_del_req = format!(
        r#"{{
            "sessionAction": "structure",
            "session": "{session}",
            "sheet": "S1",
            "axis": "row",
            "op": "delete",
            "at": 4,
            "count": 1,
            "cols": [2, 2],
            "notes": [
                {{"ref": "B1", "author": "작성자", "text": "고정 행", "visible": false}},
                {{"ref": "B4", "author": "작성자", "text": "삭제 대상", "visible": true}},
                {{"ref": "C6", "author": "작성자", "text": "타열 유지", "visible": false}}
            ]
        }}"#
    );
    let del_res = EasyOfficeIpcRouter::dispatch("evaluate_sheet", &partial_del_req).unwrap();
    let del_obj = parse_obj(&del_res);
    let del_notes = field(&del_obj, "notes").as_array().unwrap();
    assert_eq!(del_notes.len(), 2);
    assert_eq!(
        field(del_notes[0].as_object().unwrap(), "ref").as_str(),
        Some("B1")
    );
    assert_eq!(
        field(del_notes[1].as_object().unwrap(), "ref").as_str(),
        Some("C6")
    );

    // 3. Column delete at col 2 removes B and shifts C6 -> B6 and image col span left by 1
    let col_del_req = format!(
        r#"{{
            "sessionAction": "structure",
            "session": "{session}",
            "sheet": "S1",
            "axis": "col",
            "op": "delete",
            "at": 2,
            "count": 1,
            "images": [{{
                "id": "pic-shift",
                "name": "그림",
                "alt": "",
                "mime": "image/png",
                "anchor": {{
                    "fromCol": 2, "fromRow": 1, "toCol": 4, "toRow": 4,
                    "leftPx": 192, "topPx": 24, "widthPx": 192, "heightPx": 72
                }}
            }}],
            "notes": [
                {{"ref": "B6", "author": "A", "text": "삭제됨", "visible": false}},
                {{"ref": "C6", "author": "A", "text": "왼쪽 이동", "visible": false}}
            ]
        }}"#
    );
    let col_res = EasyOfficeIpcRouter::dispatch("evaluate_sheet", &col_del_req).unwrap();
    let col_obj = parse_obj(&col_res);
    let shifted_im = field(&col_obj, "images").as_array().unwrap()[0]
        .as_object()
        .unwrap();
    let shifted_a = field(shifted_im, "anchor").as_object().unwrap();
    assert_eq!(field(shifted_a, "fromCol").as_u64(), Some(1));
    assert_eq!(field(shifted_a, "toCol").as_u64(), Some(3));

    let shifted_notes = field(&col_obj, "notes").as_array().unwrap();
    assert_eq!(shifted_notes.len(), 1);
    assert_eq!(
        field(shifted_notes[0].as_object().unwrap(), "ref").as_str(),
        Some("B6")
    );
}

#[test]
fn inspect_cli_reports_image_and_note_warnings_and_rejected_counts() {
    let valid_png = make_png(2, 2, [1, 2, 3, 255]);
    let mut bad_png = make_png(2, 2, [9, 8, 7, 255]);
    let last = bad_png.len() - 1;
    bad_png[last] ^= 0xFF;

    let pkg = ZipArchiveCodec::pack_compressed(&[
        ("xl/workbook.xml".to_string(), br#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Report" sheetId="1" r:id="rId1"/></sheets></workbook>"#.to_vec()),
        ("xl/_rels/workbook.xml.rels".to_string(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#.to_vec()),
        ("xl/worksheets/sheet1.xml".to_string(), br#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>ok</t></is></c></row></sheetData><drawing r:id="rIdD1"/></worksheet>"#.to_vec()),
        ("xl/worksheets/_rels/sheet1.xml.rels".to_string(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdD1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/drawing1.xml"/><Relationship Id="rIdC1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="../comments1.xml"/></Relationships>"#.to_vec()),
        ("xl/drawings/drawing1.xml".to_string(), concat!(
            r#"<?xml version="1.0"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">"#,
            r#"<xdr:twoCellAnchor><xdr:from><xdr:col>0</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>0</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>2</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>2</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:pic><xdr:nvPicPr><xdr:cNvPr id="1" name="OkPic" descr="ok"/><xdr:cNvPicPr/></xdr:nvPicPr><xdr:blipFill><a:blip xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:embed="rId1"/></xdr:blipFill><xdr:spPr/></xdr:pic></xdr:twoCellAnchor>"#,
            r#"<xdr:twoCellAnchor><xdr:from><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>3</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>3</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:pic><xdr:nvPicPr><xdr:cNvPr id="2" name="BadPic"/><xdr:cNvPicPr/></xdr:nvPicPr><xdr:blipFill><a:blip xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:embed="rId2"/></xdr:blipFill><xdr:spPr/></xdr:pic></xdr:twoCellAnchor>"#,
            r#"</xdr:wsDr>"#
        ).as_bytes().to_vec()),
        ("xl/drawings/_rels/drawing1.xml.rels".to_string(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.png"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/bad.png"/></Relationships>"#.to_vec()),
        ("xl/media/image1.png".to_string(), valid_png.clone()),
        ("xl/media/bad.png".to_string(), bad_png),
        ("xl/comments1.xml".to_string(), br#"<?xml version="1.0"?><comments xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><authors><author>QA</author></authors><commentList><comment ref="B2" authorId="0"><text><t>good</t></text></comment><comment ref="NOT_A_CELL" authorId="0"><text><t>bad</t></text></comment></commentList></comments>"#.to_vec()),
    ]).unwrap();

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp_path = std::env::temp_dir().join(format!("pd256_inspect_{nanos}.xlsx"));
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
            .any(|w| w.as_str().is_some_and(|s| s.contains("그림 1개"))),
        "missing rejected image warning: {warnings:?}"
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().is_some_and(|s| s.contains("메모 1개"))),
        "missing rejected note warning: {warnings:?}"
    );

    let s0 = field(&report, "sheets").as_array().unwrap()[0]
        .as_object()
        .unwrap();
    assert_eq!(field(s0, "imageCount").as_u64(), Some(1));
    assert_eq!(field(s0, "noteCount").as_u64(), Some(1));

    let checks = field(&report, "checks").as_object().unwrap();
    let rejected = field(checks, "rejectedCounts").as_object().unwrap();
    assert_eq!(field(rejected, "images").as_u64(), Some(1));
    assert_eq!(field(rejected, "notes").as_u64(), Some(1));

    // Also verify `convert_document_format` decode surfaces both warnings
    let pkg_b64 = base64_codec::encode(&pkg);
    let dec_req =
        format!(r#"{{"action":"decode","fileName":"warn.xlsx","sourceBase64":"{pkg_b64}"}}"#);
    let dec_res = EasyOfficeIpcRouter::dispatch("convert_document_format", &dec_req).unwrap();
    let dec_obj = parse_obj(&dec_res);
    let dec_warnings = field(&dec_obj, "warnings").as_array().unwrap();
    assert!(
        dec_warnings
            .iter()
            .any(|w| w.as_str().is_some_and(|s| s.contains("그림 1개"))),
        "missing decode image warning: {dec_warnings:?}"
    );
    assert!(
        dec_warnings
            .iter()
            .any(|w| w.as_str().is_some_and(|s| s.contains("메모 1개"))),
        "missing decode note warning: {dec_warnings:?}"
    );
}

#[test]
fn media_and_notes_json_budgets_fail_closed() {
    let png_b64 = base64_codec::encode(&make_png(2, 2, [0, 0, 0, 255]));

    // 1. Over MAX_SHEET_IMAGES_PER_SHEET -> ResourceBudgetExceeded
    let many_imgs: Vec<String> = (0..=MAX_SHEET_IMAGES_PER_SHEET)
        .map(|i| {
            format!(
                r#"{{"id":"p{i}","mime":"image/png","data":"{png_b64}","anchor":{{"widthPx":16,"heightPx":16}}}}"#
            )
        })
        .collect();
    let req_imgs = format!(
        r#"{{"targetFormat":"xlsx","sheets":[{{"name":"S1","cells":[],"images":[{}]}}]}}"#,
        many_imgs.join(",")
    );
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("convert_document_format", &req_imgs),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 2. Over MAX_NOTES_PER_SHEET -> ResourceBudgetExceeded
    let many_notes: Vec<String> = (1..=MAX_NOTES_PER_SHEET + 1)
        .map(|row| format!(r#"{{"ref":"A{row}","text":"ok"}}"#))
        .collect();
    let req_notes = format!(
        r#"{{"targetFormat":"xlsx","sheets":[{{"name":"S1","cells":[],"notes":[{}]}}]}}"#,
        many_notes.join(",")
    );
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("convert_document_format", &req_notes),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 3. Over MAX_SHEET_NOTE_TEXT_BYTES -> ResourceBudgetExceeded
    let big_text = "x".repeat(MAX_SHEET_NOTE_TEXT_BYTES + 1);
    let req_big_note = format!(
        r#"{{"targetFormat":"xlsx","sheets":[{{"name":"S1","cells":[],"notes":[{{"ref":"A1","text":"{big_text}"}}]}}]}}"#
    );
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("convert_document_format", &req_big_note),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 4. Workbook-wide > 256 images across 5 sheets -> ResourceBudgetExceeded
    let full_imgs: Vec<String> = (0..MAX_SHEET_IMAGES_PER_SHEET)
        .map(|i| {
            format!(
                r#"{{"id":"p{i}","mime":"image/png","data":"{png_b64}","anchor":{{"widthPx":16,"heightPx":16}}}}"#
            )
        })
        .collect();
    let full_imgs_str = full_imgs.join(",");
    let req_wb_imgs = format!(
        r#"{{"targetFormat":"xlsx","sheets":[
            {{"name":"S1","cells":[],"images":[{full_imgs_str}]}},
            {{"name":"S2","cells":[],"images":[{full_imgs_str}]}},
            {{"name":"S3","cells":[],"images":[{full_imgs_str}]}},
            {{"name":"S4","cells":[],"images":[{full_imgs_str}]}},
            {{"name":"S5","cells":[],"images":[{{"id":"extra","mime":"image/png","data":"{png_b64}","anchor":{{"widthPx":16,"heightPx":16}}}}]}}
        ]}}"#
    );
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("convert_document_format", &req_wb_imgs),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 5. Invalid mime or bad cell ref -> InvalidArgument
    for bad in [
        format!(r#"{{"targetFormat":"xlsx","sheets":[{{"name":"S1","cells":[],"images":[{{"id":"p1","mime":"image/gif","data":"{png_b64}"}}]}}]}}"#),
        format!(r#"{{"targetFormat":"xlsx","sheets":[{{"name":"S1","cells":[],"images":[{{"id":"","mime":"image/png","data":"{png_b64}"}}]}}]}}"#),
        r#"{"targetFormat":"xlsx","sheets":[{"name":"S1","cells":[],"notes":[{"ref":"A1:B2","text":"range"}]}]}"#.to_string(),
        r#"{"targetFormat":"xlsx","sheets":[{"name":"S1","cells":[],"notes":[{"ref":"INVALID","text":"x"}]}]}"#.to_string(),
    ] {
        assert!(
            matches!(
                EasyOfficeIpcRouter::dispatch("convert_document_format", &bad),
                Err(EasyOfficeError::InvalidArgument(_))
            ),
            "{bad}"
        );
    }

    // 6. Corrupt PNG bytes -> CorruptedContainer
    let mut bad_png = make_png(2, 2, [0, 0, 0, 255]);
    let last = bad_png.len() - 1;
    bad_png[last] ^= 0xFF;
    let bad_b64 = base64_codec::encode(&bad_png);
    let req_bad_png = format!(
        r#"{{"targetFormat":"xlsx","sheets":[{{"name":"S1","cells":[],"images":[{{"id":"p1","mime":"image/png","data":"{bad_b64}"}}]}}]}}"#
    );
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("convert_document_format", &req_bad_png),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));
}

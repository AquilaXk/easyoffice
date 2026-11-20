//! PD-263: a sheet saved to a format that cannot hold its charts, pictures, notes, pivot tables and
//! slicers (ODS, XLS, CSV, TSV), or to XLSX with pivot fields the cache rebuild drops, says how many
//! were left out.

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{base64_codec, deflate_zlib, IeeeCrc32};

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = IeeeCrc32::update(IeeeCrc32::checksum(kind), data);
    out.extend_from_slice(&crc.to_be_bytes());
    out
}

fn tiny_png() -> Vec<u8> {
    let raw = [0u8, 20, 120, 220, 255];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    out.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
    out.extend_from_slice(&png_chunk(b"IDAT", &deflate_zlib(&raw)));
    out.extend_from_slice(&png_chunk(b"IEND", &[]));
    out
}

/// One sheet with a chart, a picture, two notes and a pivot table with a slicer whose cache has
/// four fields over a two-column source (two of them calculated or grouped).
fn payload(target: &str) -> String {
    let png = base64_codec::encode(&tiny_png());
    format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"{target}","title":"손실","sheets":[{{
        "name":"S",
        "cells":[{{"ref":"A1","raw":"월"}},{{"ref":"B1","raw":"실적"}},{{"ref":"A2","raw":"1월"}},{{"ref":"B2","raw":"120"}}],
        "charts":[{{"id":"c1","chartType":"line","title":"추이","dataRange":"A1:B2","switchRowCol":false,"legend":"bottom",
          "anchor":{{"fromCol":4,"fromRow":1,"toCol":10,"toRow":13,"left":320,"top":24,"width":480,"height":288}},
          "series":[{{"name":"실적","nameRef":"S!$B$1","categoriesRef":"S!$A$2:$A$2","valuesRef":"S!$B$2:$B$2"}}]}}],
        "images":[{{"id":"p1","name":"그림 1","alt":"","mime":"image/png","data":"{png}",
          "anchor":{{"fromCol":1,"fromRow":1,"toCol":3,"toRow":5,"leftPx":97,"topPx":26,"widthPx":192,"heightPx":96}}}}],
        "notes":[{{"ref":"B2","author":"a","text":"하나","visible":false}},{{"ref":"A2","author":"a","text":"둘","visible":false}}],
        "pivots":[{{"name":"P","cacheId":1,"sourceSheet":"S","sourceRange":"A1:B2","hasHeader":true,"location":"D1:E3",
          "fieldNames":["월","실적","월 묶음","두 배"],"rowFields":[0],"colFields":[],"filterFields":[],
          "valueFields":[{{"col":1,"agg":"sum","name":"합계"}}],
          "slicers":[{{"col":0,"caption":"월","left":300,"top":40,"width":184,"height":210,"multiSelect":true}}]}}]
        }}]}}"#
    )
}

fn warnings_of(target: &str) -> String {
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload(target))
        .unwrap_or_else(|e| panic!("{target}: {e:?}"));
    let start = json.find("\"warnings\":[").expect("warnings") + 12;
    let end = json[start..].find(']').unwrap() + start;
    json[start..end].to_string()
}

#[test]
fn formats_without_sheet_objects_count_what_they_leave_out() {
    for (target, label) in [
        ("ods", "ODS"),
        ("xls", "XLS"),
        ("csv", "CSV"),
        ("tsv", "TSV"),
    ] {
        let w = warnings_of(target);
        for want in [
            format!("차트 1개는 {label}에 저장하지 않았습니다."),
            format!("그림 1개는 {label}에 저장하지 않았습니다."),
            format!("메모 2개는 {label}에 저장하지 않았습니다."),
            format!("피벗 테이블 1개는 {label}에 저장하지 않았습니다."),
            format!("피벗 슬라이서 1개는 {label}에 저장하지 않았습니다."),
        ] {
            assert!(w.contains(&want), "{target}: {want} not in {w}");
        }
    }
}

#[test]
fn xlsx_counts_the_pivot_fields_its_cache_rebuild_drops() {
    let w = warnings_of("xlsx");
    assert!(
        w.contains("피벗 계산 필드·날짜 묶음 필드 2개는 XLSX에 저장하지 않았습니다."),
        "{w}"
    );
    for kept in ["차트", "그림", "메모", "슬라이서"] {
        assert!(!w.contains(kept), "XLSX holds {kept}: {w}");
    }
}

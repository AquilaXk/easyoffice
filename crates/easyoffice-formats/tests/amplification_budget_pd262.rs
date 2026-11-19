//! PD-262: 작은 입력이 수백 MB 메모리·수십 초 CPU로 증폭되는 경로(피벗 필드 이름, 반복 참조 파트,
//! 각주 본문 복제, 중첩 문단, 표 수식, 쪽 나눔, 템플릿 토큰 스캔)는 상한에서 Fail-Closed로 끝나야 한다.

use easyoffice_formats::{EasyOfficeError, OoxmlCodec, ZipArchiveCodec};

fn assert_budget_error<T>(result: Result<T, EasyOfficeError>, what: &str) {
    match result {
        Err(EasyOfficeError::ResourceBudgetExceeded(_)) => {}
        Err(other) => panic!("{what}: expected ResourceBudgetExceeded, got {other:?}"),
        Ok(_) => panic!("{what}: expected ResourceBudgetExceeded, the input was accepted"),
    }
}

const REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const OFFICE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn rels(items: &[(&str, &str, &str)]) -> Vec<u8> {
    let body: String = items
        .iter()
        .map(|(id, ty, target)| {
            format!(r#"<Relationship Id="{id}" Type="{OFFICE_REL}/{ty}" Target="{target}"/>"#)
        })
        .collect();
    format!(r#"<?xml version="1.0"?><Relationships xmlns="{REL_NS}">{body}</Relationships>"#)
        .into_bytes()
}

/// 항목 1: cacheField 16384개 x 이름 400B 캐시 하나를 4시트 x 피벗 32개가 같은 cacheId로 참조한다.
fn pivot_name_amplification_xlsx() -> Vec<u8> {
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    let sheets: String = (1..=4)
        .map(|i| format!(r#"<sheet name="S{i}" sheetId="{i}" r:id="rId{i}"/>"#))
        .collect();
    entries.push((
        "xl/workbook.xml".into(),
        format!(r#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="{OFFICE_REL}"><sheets>{sheets}</sheets><pivotCaches><pivotCache cacheId="1" r:id="rIdPC"/></pivotCaches></workbook>"#).into_bytes(),
    ));
    let mut wb_rels: Vec<(String, &str, String)> = (1..=4)
        .map(|i| {
            (
                format!("rId{i}"),
                "worksheet",
                format!("worksheets/sheet{i}.xml"),
            )
        })
        .collect();
    wb_rels.push((
        "rIdPC".into(),
        "pivotCacheDefinition",
        "pivotCache/pivotCacheDefinition1.xml".into(),
    ));
    let wb_rels_ref: Vec<(&str, &str, &str)> = wb_rels
        .iter()
        .map(|(a, b, c)| (a.as_str(), *b, c.as_str()))
        .collect();
    entries.push(("xl/_rels/workbook.xml.rels".into(), rels(&wb_rels_ref)));
    let name = "N".repeat(400);
    // 필드마다 달라지는 8자리 해시로 압축비를 팩커 상한(90) 아래로 맞춘다.
    let fields: String = (0..16_384u64)
        .map(|i| {
            let h = i.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 32;
            format!(r#"<cacheField name="{h:08x}{name}"/>"#)
        })
        .collect();
    entries.push((
        "xl/pivotCache/pivotCacheDefinition1.xml".into(),
        format!(r#"<?xml version="1.0"?><pivotCacheDefinition xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><cacheSource type="worksheet"><worksheetSource ref="A1:XFD2" sheet="S1"/></cacheSource><cacheFields count="16384">{fields}</cacheFields></pivotCacheDefinition>"#).into_bytes(),
    ));
    for s in 1..=4 {
        entries.push((
            format!("xl/worksheets/sheet{s}.xml"),
            br#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData/></worksheet>"#.to_vec(),
        ));
        let items: Vec<(String, String)> = (0..32)
            .map(|p| {
                (
                    format!("rIdPT{p}"),
                    format!("../pivotTables/pivotTable{s}_{p}.xml"),
                )
            })
            .collect();
        let items_ref: Vec<(&str, &str, &str)> = items
            .iter()
            .map(|(a, c)| (a.as_str(), "pivotTable", c.as_str()))
            .collect();
        entries.push((
            format!("xl/worksheets/_rels/sheet{s}.xml.rels"),
            rels(&items_ref),
        ));
        for p in 0..32 {
            entries.push((
                format!("xl/pivotTables/pivotTable{s}_{p}.xml"),
                format!(r#"<?xml version="1.0"?><pivotTableDefinition xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" name="P{s}_{p}" cacheId="1"><location ref="A5:B8"/><rowFields count="1"><field x="0"/></rowFields><dataFields count="1"><dataField name="V" fld="1"/></dataFields></pivotTableDefinition>"#).into_bytes(),
            ));
        }
    }
    ZipArchiveCodec::pack_compressed(&entries).expect("pack pivot fixture")
}

#[test]
fn pivot_cache_field_names_shared_by_many_pivots_hit_the_budget() {
    let bytes = pivot_name_amplification_xlsx();
    assert!(bytes.len() < 400 * 1024, "fixture is {} bytes", bytes.len());
    assert_budget_error(
        OoxmlCodec::unpack_xlsx_with_styling(&bytes),
        "pivot field-name amplification",
    );
}

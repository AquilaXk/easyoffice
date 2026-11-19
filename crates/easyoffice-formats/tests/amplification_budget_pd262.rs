//! PD-262: 작은 입력이 수백 MB 메모리·수십 초 CPU로 증폭되는 경로(피벗 필드 이름, 반복 참조 파트,
//! 각주 본문 복제, 중첩 문단, 표 수식, 쪽 나눔, 템플릿 토큰 스캔)는 상한에서 Fail-Closed로 끝나야 한다.

use easyoffice_formats::{
    DocumentFormat, EasyOfficeError, OoxmlCodec, RichBlock, RichComment, RichDocument, RichNote,
    RichNoteKind, RichParagraph, RichRun, UniversalFormatConverter, ZipArchiveCodec,
};

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

/// 항목 2: 120 MiB comments 파트 하나를 시트 4개가 같은 파트로 참조한다.
fn comments_repeat_reference_xlsx(vml: bool) -> Vec<u8> {
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    let sheets: String = (1..=4)
        .map(|i| format!(r#"<sheet name="S{i}" sheetId="{i}" r:id="rId{i}"/>"#))
        .collect();
    entries.push((
        "xl/workbook.xml".into(),
        format!(r#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="{OFFICE_REL}"><sheets>{sheets}</sheets></workbook>"#).into_bytes(),
    ));
    let wb_rels: Vec<(String, String)> = (1..=4)
        .map(|i| (format!("rId{i}"), format!("worksheets/sheet{i}.xml")))
        .collect();
    let wb_rels_ref: Vec<(&str, &str, &str)> = wb_rels
        .iter()
        .map(|(a, c)| (a.as_str(), "worksheet", c.as_str()))
        .collect();
    entries.push(("xl/_rels/workbook.xml.rels".into(), rels(&wb_rels_ref)));
    // 공백 100B마다 탭·줄바꿈·CR 중 하나를 해시로 골라 끼운다. 공백만이라 XML 이벤트는 생기지 않고,
    // 압축비는 팩커 상한(90) 아래다. 파트 크기는 약 120 MiB.
    let blank = " ".repeat(100);
    let pad: String = (0..(120 * 1024 * 1024 / 101) as u64)
        .map(|i| {
            let ws =
                ["\t", "\n", "\r"][((i.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40) % 3) as usize];
            format!("{blank}{ws}")
        })
        .collect();
    let comments = format!(
        r#"<?xml version="1.0"?><comments xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><authors><author>a</author></authors><commentList>{pad}<comment ref="A1" authorId="0"><text><t>x</t></text></comment></commentList></comments>"#
    );
    entries.push(("xl/comments1.xml".into(), comments.into_bytes()));
    for s in 1..=4 {
        let legacy = if vml {
            r#"<legacyDrawing r:id="rIdV"/>"#
        } else {
            ""
        };
        entries.push((
            format!("xl/worksheets/sheet{s}.xml"),
            format!(r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="{OFFICE_REL}"><sheetData/>{legacy}</worksheet>"#).into_bytes(),
        ));
        let mut items = vec![("rIdC", "comments", "../comments1.xml")];
        if vml {
            items.push(("rIdV", "vmlDrawing", "../drawings/vmlDrawing1.vml"));
        }
        entries.push((
            format!("xl/worksheets/_rels/sheet{s}.xml.rels"),
            rels(&items),
        ));
    }
    if vml {
        entries.push((
            "xl/drawings/vmlDrawing1.vml".into(),
            b"<xml xmlns:v=\"urn:schemas-microsoft-com:vml\"></xml>".to_vec(),
        ));
    }
    ZipArchiveCodec::pack_compressed(&entries).expect("pack comments fixture")
}

#[test]
fn comments_part_referenced_by_every_sheet_is_read_once() {
    let bytes = comments_repeat_reference_xlsx(false);
    assert!(
        bytes.len() < REFS * MAX_BYTES_PER_REF,
        "fixture is {} bytes",
        bytes.len()
    );
    let (_, stylings) = OoxmlCodec::unpack_xlsx_with_styling(&bytes).expect("decode");
    let with_notes = stylings.iter().filter(|(_, s)| !s.notes.is_empty()).count();
    assert!(
        with_notes <= 1,
        "the same comments part fed {with_notes} sheets"
    );
    let rejected: u32 = stylings.iter().map(|(_, s)| s.notes_rejected).sum();
    assert!(
        rejected >= 3,
        "second references must be turned away, got {rejected}"
    );
}

/// 항목 3 재현 규모(60 KB 각주 x 15만 참조)는 수정 전 코드에서 9 GB를 먹으므로 RED 단계에서는
/// `REFS`를 줄여 돌렸다. 수정 후에는 원 규모로 둔다.
const REFS: usize = 150_000;

/// 표식 하나가 정당하게 쓰는 바이트 상한. 60 KB 본문을 되풀이하면 이 값의 수백 배가 된다.
const MAX_BYTES_PER_REF: usize = 512;

fn note_ref_run(kind: RichNoteKind) -> RichRun {
    RichRun {
        note_ref: Some((kind, 1)),
        ..RichRun::default()
    }
}

fn doc_with_repeated_note_refs(kind: RichNoteKind) -> RichDocument {
    let runs = (0..REFS).map(|_| note_ref_run(kind)).collect();
    RichDocument {
        notes: vec![RichNote {
            id: 1,
            kind,
            text: "가".repeat(20_000),
        }],
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs,
            ..RichParagraph::default()
        })],
        ..RichDocument::default()
    }
}

fn assert_small_output(format: DocumentFormat, what: &str) {
    let doc = doc_with_repeated_note_refs(RichNoteKind::Footnote);
    let bytes = UniversalFormatConverter::encode_rich(&doc, format, None).expect(what);
    assert!(
        bytes.len() < REFS * MAX_BYTES_PER_REF,
        "{what}: one 60 KB note referenced {REFS} times wrote {} bytes",
        bytes.len()
    );
}

#[test]
fn odt_writes_a_note_body_once_however_often_it_is_referenced() {
    assert_small_output(DocumentFormat::Odt, "odt");
}

#[test]
fn hwpx_writes_a_note_body_once_however_often_it_is_referenced() {
    assert_small_output(DocumentFormat::Hwpx, "hwpx");
}

#[test]
fn odt_writes_a_comment_body_once_however_many_anchors_it_has() {
    // 메모 앵커 사이에 일반 런을 끼워 매번 새 annotation이 열리게 한다.
    let runs: Vec<RichRun> = (0..REFS)
        .flat_map(|_| {
            [
                RichRun {
                    text: "a".into(),
                    comment_id: Some(1),
                    ..RichRun::default()
                },
                RichRun::plain("b"),
            ]
        })
        .collect();
    let doc = RichDocument {
        comments: vec![RichComment {
            id: 1,
            author: "김검토".into(),
            date: "2026-11-19T09:30:00Z".into(),
            text: "가".repeat(20_000),
        }],
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs,
            ..RichParagraph::default()
        })],
        ..RichDocument::default()
    };
    let bytes = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Odt, None)
        .expect("odt comments");
    assert!(
        bytes.len() < REFS * MAX_BYTES_PER_REF,
        "one comment with {REFS} anchors wrote {} bytes",
        bytes.len()
    );
}

/// 항목 4: 닫지 않은 문단 태그를 끝없이 여는 입력. 재현 규모는 500만 개(RSS 0.7~1.5 GB)다.
const OPEN_PARAS: usize = 5_000_000;

fn nested_paragraph_input(open_tag: &str) -> String {
    open_tag.repeat(OPEN_PARAS)
}

#[test]
fn docx_unclosed_paragraphs_hit_the_depth_budget() {
    let body = nested_paragraph_input("<w:p>");
    let xml = format!(
        r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#
    );
    let bytes = ZipArchiveCodec::pack_compressed(&[
        ("[Content_Types].xml".to_string(), b"<Types/>".to_vec()),
        ("word/document.xml".to_string(), xml.into_bytes()),
    ])
    .expect("pack docx");
    assert_budget_error(
        UniversalFormatConverter::decode_rich(&bytes, Some("a.docx")),
        "docx nested paragraphs",
    );
}

#[test]
fn odt_unclosed_paragraphs_hit_the_depth_budget() {
    let body = nested_paragraph_input("<text:p>");
    let xml = format!(
        r#"<?xml version="1.0"?><office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><office:body><office:text>{body}</office:text></office:body></office:document-content>"#
    );
    let bytes = ZipArchiveCodec::pack_compressed(&[
        (
            "mimetype".to_string(),
            b"application/vnd.oasis.opendocument.text".to_vec(),
        ),
        ("content.xml".to_string(), xml.into_bytes()),
    ])
    .expect("pack odt");
    assert_budget_error(
        UniversalFormatConverter::decode_rich(&bytes, Some("a.odt")),
        "odt nested paragraphs",
    );
}

#[test]
fn hwpx_unclosed_paragraphs_hit_the_depth_budget() {
    let body = nested_paragraph_input("<hp:p>");
    let xml = format!(
        r#"<?xml version="1.0"?><hs:sec xmlns:hs="http://www.owpml.org/owpml/2011/section" xmlns:hp="http://www.owpml.org/owpml/2011/paragraph">{body}</hs:sec>"#
    );
    let header = r#"<?xml version="1.0"?><hh:head xmlns:hh="http://www.owpml.org/owpml/2011/head" xmlns:hc="http://www.owpml.org/owpml/2011/core"><hh:refList></hh:refList></hh:head>"#;
    let bytes = ZipArchiveCodec::pack_compressed(&[
        ("mimetype".to_string(), b"application/hwp+zip".to_vec()),
        (
            "Contents/header.xml".to_string(),
            header.as_bytes().to_vec(),
        ),
        ("Contents/section0.xml".to_string(), xml.into_bytes()),
    ])
    .expect("pack hwpx");
    assert_budget_error(
        UniversalFormatConverter::decode_rich(&bytes, Some("a.hwpx")),
        "hwpx nested paragraphs",
    );
}

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
fn comments_repeat_reference_xlsx() -> Vec<u8> {
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
        entries.push((
            format!("xl/worksheets/sheet{s}.xml"),
            format!(r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="{OFFICE_REL}"><sheetData/></worksheet>"#).into_bytes(),
        ));
        let items = [("rIdC", "comments", "../comments1.xml")];
        entries.push((
            format!("xl/worksheets/_rels/sheet{s}.xml.rels"),
            rels(&items),
        ));
    }
    ZipArchiveCodec::pack_compressed(&entries).expect("pack comments fixture")
}

#[test]
fn comments_part_referenced_by_every_sheet_is_read_once() {
    let bytes = comments_repeat_reference_xlsx();
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

/// 시간 상한 시험의 여유 폭: 수정 후 코드는 이 값의 1/10 안에 끝난다.
const LINEAR_TIME_LIMIT_SECS: u64 = 5;

fn assert_finishes_fast<T>(what: &str, work: impl FnOnce() -> T) -> T {
    let start = std::time::Instant::now();
    let out = work();
    let took = start.elapsed();
    assert!(
        took.as_secs() < LINEAR_TIME_LIMIT_SECS,
        "{what}: took {took:?}, a linear pass finishes well under {LINEAR_TIME_LIMIT_SECS}s"
    );
    out
}

#[test]
fn docx_page_breaks_after_blank_runs_decode_in_linear_time() {
    // 굵게/보통을 번갈아 쓴 공백 런이라 합쳐지지 않고, 쪽 나눔마다 앞선 런이 전부 공백이다.
    let runs: String = (0..200_000)
        .map(|i| {
            let rpr = if i % 2 == 0 { "<w:rPr><w:b/></w:rPr>" } else { "" };
            format!(r#"<w:r>{rpr}<w:t xml:space="preserve"> </w:t></w:r><w:r><w:br w:type="page"/></w:r>"#)
        })
        .collect();
    let xml = format!(
        r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p>{runs}</w:p></w:body></w:document>"#
    );
    let bytes = ZipArchiveCodec::pack_compressed(&[
        ("[Content_Types].xml".to_string(), b"<Types/>".to_vec()),
        ("word/document.xml".to_string(), xml.into_bytes()),
    ])
    .expect("pack docx");
    assert_finishes_fast("docx page breaks", || {
        UniversalFormatConverter::decode_rich(&bytes, Some("a.docx")).expect("decode")
    });
}

#[test]
fn template_token_scan_is_linear_on_unclosed_openers() {
    use easyoffice_formats::layout_lint::contains_template_token;
    // 닫는 표지 하나 앞에 여는 표지 80만 개(160만 자): 옛 구현은 여는 표지마다 끝까지 다시 훑었다.
    let text = format!("{}}}}}", "{{".repeat(800_000));
    assert_finishes_fast("template token scan", || {
        assert!(
            !contains_template_token(&text),
            "inner holds a nested opener, then nothing"
        );
    });
    // 의미는 그대로: 토큰은 찾고, 공백·개행·65자 이상 내부는 거른다.
    assert!(contains_template_token("앞 {{날짜}} 뒤"));
    assert!(contains_template_token("{{{{x}}"));
    assert!(contains_template_token("{{ }} {{ a b }}"));
    assert!(!contains_template_token("{{a\nb}}"));
    assert!(!contains_template_token(&format!(
        "{{{{{}}}}}",
        "a".repeat(65)
    )));
    assert!(contains_template_token(&format!(
        "{{{{{}}}}}",
        "a".repeat(64)
    )));
    assert!(!contains_template_token("{{ 열림만"));
}

#[test]
fn table_formulas_over_tens_of_thousands_of_rows_stay_within_the_operand_budget() {
    use easyoffice_formats::evaluate_table_formulas;
    // 6만 행 모두 `=COUNT(ABOVE)`: 행마다 위쪽 전체를 훑는 옛 구현은 제곱 시간이었다.
    let rows_len = 60_000usize;
    let mut rows: Vec<Vec<String>> = (0..rows_len)
        .map(|_| vec!["=COUNT(ABOVE)".to_string()])
        .collect();
    rows[0][0] = "7".to_string();
    assert_finishes_fast("table formulas", || evaluate_table_formulas(&mut rows));
    // 예산 안쪽 앞부분은 예전과 같은 값이고, 예산을 넘은 뒷부분은 수식 그대로 텍스트로 남는다.
    assert_eq!(rows[1][0], "1");
    assert_eq!(rows[5][0], "5");
    assert_eq!(rows[1_000][0], "1000");
    assert_eq!(rows[rows_len - 1][0], "=COUNT(ABOVE)");
}

#[test]
fn slicer_sizes_beyond_u32_pixels_clamp_instead_of_overflowing() {
    // 폭은 u64 최댓값(옛 코드는 더하기에서 오버플로), 높이는 2^32+600픽셀(옛 코드는 600으로 잘렸다).
    let pkg = ZipArchiveCodec::pack_compressed(&[
        ("xl/workbook.xml".to_string(), br#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Sales" sheetId="1" r:id="rId1"/></sheets><pivotCaches><pivotCache cacheId="42" r:id="rIdPC42"/></pivotCaches></workbook>"#.to_vec()),
        ("xl/_rels/workbook.xml.rels".to_string(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rIdPC42" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheDefinition" Target="pivotCache/pivotCacheDefinition42.xml"/></Relationships>"#.to_vec()),
        ("xl/worksheets/sheet1.xml".to_string(), br#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Region</t></is></c><c r="B1" t="inlineStr"><is><t>Quarter</t></is></c><c r="C1" t="inlineStr"><is><t>Revenue</t></is></c></row></sheetData></worksheet>"#.to_vec()),
        ("xl/worksheets/_rels/sheet1.xml.rels".to_string(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdPT1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotTable" Target="../pivotTables/pivotTable1.xml"/></Relationships>"#.to_vec()),
        ("xl/pivotCache/pivotCacheDefinition42.xml".to_string(), br#"<?xml version="1.0"?><pivotCacheDefinition xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" recordCount="1"><cacheSource type="worksheet"><worksheetSource ref="A1:C5" sheet="Sales"/></cacheSource><cacheFields count="3"><cacheField name="Region" numFmtId="0"/><cacheField name="Quarter" numFmtId="0"/><cacheField name="Revenue" numFmtId="0"/></cacheFields></pivotCacheDefinition>"#.to_vec()),
        ("xl/pivotTables/pivotTable1.xml".to_string(), br#"<?xml version="1.0"?><pivotTableDefinition xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" name="ExternalPivot" cacheId="42" dataCaption="Values"><location ref="E1:G6" firstHeaderRow="1" firstDataRow="2" firstDataCol="1"/><pivotFields count="3"><pivotField axis="axisRow"/><pivotField axis="axisCol"/><pivotField dataField="1"/></pivotFields><rowFields count="1"><field x="0"/></rowFields><colFields count="1"><field x="1"/></colFields><dataFields count="2"><dataField name="Sum of Revenue" fld="2" baseField="0" baseItem="0"/><dataField name="Count of Region" fld="0" subtotal="countNums" baseField="0" baseItem="0"/></dataFields></pivotTableDefinition>"#.to_vec()),
        ("xl/slicerCaches/slicerCache1.xml".to_string(), concat!(
            r#"<?xml version="1.0"?><slicerCacheDefinition xmlns="http://schemas.micro"#,
            r#"soft.com/office/spreadsheetml/2009/9/main" name="Slicer_Region" sourceName="Region"><pivotTables><pivotTable tabId="1" name="ExternalPivot"/></pivotTables><data><tabular pivotCacheId="42"><items count="1"><item n="East" s="1"/></items></tabular></data></slicerCacheDefinition>"#
        ).as_bytes().to_vec()),
        ("xl/slicers/slicer1.xml".to_string(), concat!(
            r#"<?xml version="1.0"?><slicers xmlns="http://schemas.micro"#,
            r#"soft.com/office/spreadsheetml/2009/9/main"><slicer name="Region Slicer" cache="Slicer_Region" caption="Region Filter" width="18446744073709551615" height="40909569209400"/></slicers>"#
        ).as_bytes().to_vec()),
    ]).unwrap();

    let (_, stylings) = OoxmlCodec::unpack_xlsx_with_styling(&pkg).expect("decode");
    let slicer = &stylings[0].1.pivots[0].slicers[0];
    assert_eq!((slicer.width, slicer.height), (2_000, 2_000));
}

#[test]
fn drawing_extents_beyond_u64_pixels_clamp_instead_of_overflowing() {
    // 확장 크기가 u64 최댓값이면 옛 코드는 반올림 더하기에서 오버플로했다.
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
                r#"<xdr:ext cx="18446744073709551615" cy="18446744073709551615"/>"#,
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

    let (_, stylings) = OoxmlCodec::unpack_xlsx_with_styling(&pkg).expect("decode");
    let chart = &stylings[0].1.charts[0];
    assert_eq!(chart.anchor.width_px, 1_600);
    assert_eq!(chart.anchor.height_px, 1_200);
}

/// 계산 필드(`formula`)와 날짜 묶음 필드(`fieldGroup`, `databaseField="0"`)는 원본 열이 없어
/// 캐시 필드 수가 원본 범위 폭보다 크다. 이런 정상 피벗을 필드 수로 거부하면 안 된다.
fn derived_field_pivot_xlsx() -> Vec<u8> {
    ZipArchiveCodec::pack_compressed(&[
        ("xl/workbook.xml".to_string(), br#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets><pivotCaches><pivotCache cacheId="1" r:id="rIdPC"/></pivotCaches></workbook>"#.to_vec()),
        ("xl/_rels/workbook.xml.rels".to_string(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rIdPC" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheDefinition" Target="pivotCache/pivotCacheDefinition1.xml"/></Relationships>"#.to_vec()),
        ("xl/worksheets/sheet1.xml".to_string(), br#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Day</t></is></c><c r="B1" t="inlineStr"><is><t>Amount</t></is></c></row></sheetData></worksheet>"#.to_vec()),
        ("xl/worksheets/_rels/sheet1.xml.rels".to_string(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdPT" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotTable" Target="../pivotTables/pivotTable1.xml"/></Relationships>"#.to_vec()),
        ("xl/pivotCache/pivotCacheDefinition1.xml".to_string(), br#"<?xml version="1.0"?><pivotCacheDefinition xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><cacheSource type="worksheet"><worksheetSource ref="A1:B3" sheet="S"/></cacheSource><cacheFields count="4"><cacheField name="Day" numFmtId="14"/><cacheField name="Amount" numFmtId="0"/><cacheField name="Months" numFmtId="0" databaseField="0"><fieldGroup base="0"><rangePr groupBy="months" startDate="2026-01-01T00:00:00" endDate="2026-12-31T00:00:00"/><groupItems count="14"><s v="&lt;2026-01-01"/><s v="Jan"/><s v="&gt;2026-12-31"/></groupItems></fieldGroup></cacheField><cacheField name="Double" numFmtId="0" formula="Amount*2" databaseField="0"/></cacheFields></pivotCacheDefinition>"#.to_vec()),
        ("xl/pivotTables/pivotTable1.xml".to_string(), br#"<?xml version="1.0"?><pivotTableDefinition xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" name="Derived" cacheId="1"><location ref="D1:E4"/><rowFields count="1"><field x="0"/></rowFields><dataFields count="1"><dataField name="Total" fld="1"/></dataFields></pivotTableDefinition>"#.to_vec()),
    ])
    .expect("pack derived-field pivot")
}

#[test]
fn pivots_with_calculated_and_date_group_fields_are_read_and_saved() {
    let (sheets, stylings) =
        OoxmlCodec::unpack_xlsx_with_styling(&derived_field_pivot_xlsx()).expect("decode");
    let st = &stylings[0].1;
    assert_eq!((st.pivots.len(), st.pivots_rejected), (1, 0));
    assert_eq!(
        st.pivots[0].field_names,
        ["Day", "Amount", "Months", "Double"]
    );

    let styles: Vec<_> = stylings.iter().map(|(_, s)| s.clone()).collect();
    let saved = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &styles).expect("save");
    let (_, again) = OoxmlCodec::unpack_xlsx_with_styling(&saved).expect("decode again");
    assert_eq!(
        (again[0].1.pivots.len(), again[0].1.pivots_rejected),
        (1, 0)
    );
    // 저장 쪽은 원본 열에서 캐시를 다시 만들어 계산·묶음 필드는 싣지 않는다(변경 전과 같다).
    assert_eq!(again[0].1.pivots[0].field_names, ["Day", "Amount"]);
    assert_eq!(again[0].1.pivots[0].row_fields, [0]);
}

/// 같은 메모가 여러 문단에 걸쳐 있으면 ODT에는 `office:annotation` 하나와 끝 문단의
/// `office:annotation-end`만 쓰고, 읽으면 메모 하나가 세 문단의 범위를 그대로 덮어야 한다.
fn three_paragraph_comment_doc() -> RichDocument {
    let paragraph = |text: &str| {
        RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun {
                text: text.to_string(),
                comment_id: Some(1),
                ..RichRun::default()
            }],
            ..RichParagraph::default()
        })
    };
    RichDocument {
        comments: vec![RichComment {
            id: 1,
            author: "김검토".into(),
            date: "2026-11-19T09:30:00Z".into(),
            text: "세 문단에 걸친 메모".into(),
        }],
        blocks: vec![paragraph("첫째"), paragraph("둘째"), paragraph("셋째")],
        ..RichDocument::default()
    }
}

fn decode_document(bytes: &[u8], hint: &str) -> RichDocument {
    let out = UniversalFormatConverter::decode_rich(bytes, Some(hint)).expect("decode");
    let easyoffice_formats::DecodedContent::Document(doc) = out.content else {
        panic!("document expected");
    };
    doc
}

fn assert_one_comment_over_three_paragraphs(doc: &RichDocument, what: &str) {
    assert_eq!(doc.comments.len(), 1, "{what}: {:?}", doc.comments);
    assert_eq!(doc.comments[0].text, "세 문단에 걸친 메모", "{what}");
    let anchored: Vec<_> = doc
        .blocks
        .iter()
        .map(|b| match b {
            RichBlock::Paragraph(p) => p
                .runs
                .iter()
                .any(|r| r.comment_id == Some(doc.comments[0].id)),
            _ => false,
        })
        .collect();
    assert_eq!(anchored, [true, true, true], "{what}");
}

#[test]
fn odt_keeps_one_comment_over_several_paragraphs() {
    let odt = UniversalFormatConverter::encode_rich(
        &three_paragraph_comment_doc(),
        DocumentFormat::Odt,
        None,
    )
    .expect("encode odt");
    assert_one_comment_over_three_paragraphs(&decode_document(&odt, "a.odt"), "odt");
}

#[test]
fn docx_to_odt_to_docx_keeps_one_comment_and_its_range() {
    let docx = UniversalFormatConverter::encode_rich(
        &three_paragraph_comment_doc(),
        DocumentFormat::Docx,
        None,
    )
    .expect("encode docx");
    let via_docx = decode_document(&docx, "a.docx");
    assert_one_comment_over_three_paragraphs(&via_docx, "docx");
    let odt =
        UniversalFormatConverter::encode_rich(&via_docx, DocumentFormat::Odt, None).expect("odt");
    let via_odt = decode_document(&odt, "a.odt");
    assert_one_comment_over_three_paragraphs(&via_odt, "odt");
    let docx2 =
        UniversalFormatConverter::encode_rich(&via_odt, DocumentFormat::Docx, None).expect("docx2");
    assert_one_comment_over_three_paragraphs(&decode_document(&docx2, "b.docx"), "docx again");
}

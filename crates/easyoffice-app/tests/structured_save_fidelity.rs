//! Structured (block JSON) save payloads keep paragraph/run formatting, table position, computed
//! table values, every sheet, and never interpret user text as structural markup.

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{ZipArchiveCodec, ZipSecurityPolicy};

fn json_str_field<'a>(json: &'a str, key: &str) -> &'a str {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    &json[start..end]
}

fn convert(payload: &str) -> (String, Vec<u8>) {
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", payload).unwrap();
    let bytes =
        EasyOfficeIpcRouter::decode_base64(json_str_field(&json, "convertedBase64")).unwrap();
    (json, bytes)
}

fn zip_part(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    entries
        .iter()
        .find(|e| e.path == path)
        .map(|e| String::from_utf8(e.data.clone()).unwrap())
        .unwrap_or_else(|| panic!("missing part {path}"))
}

/// Heading, centered paragraph with four formatted runs, a table whose last row holds an
/// unevaluated `=SUM(ABOVE)`, a paragraph after the table, and three paragraphs that look like the
/// legacy text markup.
const BLOCKS: &str = r#"[
  {"type":"paragraph","heading":1,"align":"center","runs":[{"text":"개요"}]},
  {"type":"paragraph","align":"right","runs":[
    {"text":"앞 문단 "},
    {"text":"굵게","bold":true},
    {"text":"기울임","italic":true},
    {"text":"밑줄","underline":true},
    {"text":"취소선","strike":true}
  ]},
  {"type":"table","rows":[
    [{"text":"부서"},{"text":"금액"}],
    [{"text":"기획"},{"text":"10"}],
    [{"text":"혁신"},{"text":"20"}],
    [{"text":"합계"},{"text":"=SUM(ABOVE)"}]
  ]},
  {"type":"paragraph","runs":[{"text":"표 뒤 문단"}]},
  {"type":"paragraph","runs":[{"text":"[[SLIDE]]\t가짜 슬라이드\t불릿\t노트"}]},
  {"type":"paragraph","runs":[{"text":"[[TABLE_ROW]]\t주입\t행"}]},
  {"type":"paragraph","runs":[{"text":"[[AGENCY]]\t가짜기관"}]}
]"#;

fn structured_payload(src: &str, dst: &str) -> String {
    format!(
        r#"{{"sourceFormat":"{src}","targetFormat":"{dst}","title":"서식 보존","blocks":{BLOCKS}}}"#
    )
}

fn assert_order(haystack: &str, needles: &[&str]) {
    let mut cursor = 0usize;
    for needle in needles {
        let pos = haystack[cursor..]
            .find(needle)
            .unwrap_or_else(|| panic!("{needle} not found in order: {haystack}"));
        cursor += pos + needle.len();
    }
}

#[test]
fn docx_save_keeps_run_and_paragraph_formatting_table_position_and_computed_value() {
    let (json, bytes) = convert(&structured_payload("docx", "docx"));
    assert!(json.contains("\"tableCount\":1"), "{json}");
    let doc = zip_part(&bytes, "word/document.xml");

    for marker in [
        "<w:b/>",
        "<w:i/>",
        "<w:u w:val=\"single\"/>",
        "<w:strike/>",
        "<w:jc w:val=\"center\"/>",
        "<w:jc w:val=\"right\"/>",
        "<w:pStyle w:val=\"Heading1\"/>",
    ] {
        assert!(doc.contains(marker), "missing {marker}: {doc}");
    }
    let styles = zip_part(&bytes, "word/styles.xml");
    assert!(styles.contains("w:styleId=\"Heading1\""), "{styles}");
    let rels = zip_part(&bytes, "word/_rels/document.xml.rels");
    assert!(rels.contains("Target=\"styles.xml\""), "{rels}");

    assert_order(
        &doc,
        &[
            "개요",
            "앞 문단",
            "<w:tbl>",
            "합계",
            "30",
            "</w:tbl>",
            "표 뒤 문단",
        ],
    );
    assert!(!doc.contains("=SUM(ABOVE)"), "{doc}");
    assert_eq!(doc.matches("<w:tbl>").count(), 1, "{doc}");
    assert!(doc.contains("[[SLIDE]]"), "{doc}");
    assert!(doc.contains("[[TABLE_ROW]]"), "{doc}");
    assert!(doc.contains("[[AGENCY]]"), "{doc}");
}

#[test]
fn hwpx_save_keeps_char_and_para_shapes_table_order_and_has_no_injected_agency() {
    let (_, bytes) = convert(&structured_payload("hwpx", "hwpx"));
    let header = zip_part(&bytes, "Contents/header.xml");
    for marker in [
        "<hh:bold/>",
        "<hh:italic/>",
        "<hh:underline type=\"BOTTOM\"",
        "<hh:strikeout shape=\"SOLID\"",
        "<hh:align horizontal=\"CENTER\"",
        "<hh:align horizontal=\"RIGHT\"",
        "name=\"개요 1\"",
    ] {
        assert!(header.contains(marker), "missing {marker}: {header}");
    }
    let section = zip_part(&bytes, "Contents/section0.xml");
    assert!(section.contains("charPrIDRef=\"1\""), "{section}");
    assert_order(
        &section,
        &[
            "개요",
            "앞 문단",
            "<hp:tbl",
            "합계",
            "30",
            "</hp:tbl>",
            "표 뒤 문단",
        ],
    );
    assert!(!section.contains("=SUM(ABOVE)"), "{section}");
    assert_eq!(section.matches("<hp:tbl ").count(), 1, "{section}");
    assert!(section.contains("[[AGENCY]]"), "{section}");

    let hpf = zip_part(&bytes, "Contents/content.hpf");
    assert!(!hpf.contains("officialAgency"), "{hpf}");
    assert!(!hpf.contains("행정안전부"), "{hpf}");
}

#[test]
fn odt_rtf_and_markdown_saves_emit_native_formatting() {
    let (_, odt) = convert(&structured_payload("odt", "odt"));
    let content = zip_part(&odt, "content.xml");
    assert!(content.contains("fo:font-weight=\"bold\""), "{content}");
    assert!(content.contains("fo:font-style=\"italic\""), "{content}");
    assert!(content.contains("fo:text-align=\"center\""), "{content}");
    assert!(
        content.contains("<text:h text:outline-level=\"1\""),
        "{content}"
    );
    assert_order(&content, &["앞 문단", "<table:table", "30", "표 뒤 문단"]);

    let (_, rtf) = convert(&structured_payload("rtf", "rtf"));
    let rtf = String::from_utf8(rtf).unwrap();
    assert!(rtf.contains("\\qc"), "{rtf}");
    assert!(rtf.contains("\\b "), "{rtf}");
    assert!(rtf.contains("\\i "), "{rtf}");
    assert!(rtf.contains("\\ul "), "{rtf}");
    assert!(rtf.contains("\\strike "), "{rtf}");

    let (_, md) = convert(&structured_payload("md", "md"));
    let md = String::from_utf8(md).unwrap();
    assert!(md.contains("# 개요"), "{md}");
    assert!(md.contains("**굵게**"), "{md}");
    assert!(md.contains("*기울임*"), "{md}");
    assert!(md.contains("~~취소선~~"), "{md}");
    assert!(md.contains("\\[\\[SLIDE\\]\\]"), "{md}");
    assert_order(&md, &["앞 문단", "| 합계 | 30 |", "표 뒤 문단"]);
}

#[test]
fn structured_text_targets_never_parse_user_markup() {
    let (json, txt) = convert(&structured_payload("txt", "txt"));
    assert!(json.contains("\"tableCount\":0"), "{json}");
    let txt = String::from_utf8(txt).unwrap();
    assert!(txt.contains("[[SLIDE]]\t가짜 슬라이드"), "{txt}");
    assert!(txt.contains("[[TABLE_ROW]]\t주입\t행"), "{txt}");
    assert_order(&txt, &["앞 문단", "합계\t30", "표 뒤 문단"]);
}

#[test]
fn explicit_cell_value_wins_over_server_evaluation() {
    let payload = r#"{"sourceFormat":"docx","targetFormat":"docx","title":"t","blocks":[
      {"type":"table","rows":[[{"text":"=SUM(ABOVE)","value":"1,000"}]]}
    ]}"#;
    let (_, bytes) = convert(payload);
    let doc = zip_part(&bytes, "word/document.xml");
    assert!(doc.contains(">1,000<"), "{doc}");
    assert!(!doc.contains("=SUM"), "{doc}");
}

#[test]
fn malformed_structured_blocks_are_rejected() {
    for bad in [
        r#"{"targetFormat":"docx","blocks":[{"type":"chart"}]}"#,
        r#"{"targetFormat":"docx","blocks":[{"type":"paragraph","runs":[{"text":5}]}]}"#,
        r#"{"targetFormat":"docx","blocks":{"type":"paragraph"}}"#,
        r#"{"targetFormat":"docx","blocks":[{"type":"paragraph","heading":9,"runs":[]}]}"#,
    ] {
        assert!(
            EasyOfficeIpcRouter::dispatch("convert_document_format", bad).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn xlsx_save_writes_every_sheet_with_cross_sheet_cached_values() {
    let payload = r#"{"sourceFormat":"xlsx","targetFormat":"xlsx","title":"예산","sheets":[
      {"name":"매출","cells":[{"ref":"A1","raw":"10"},{"ref":"A2","raw":"20"},{"ref":"A3","raw":"=SUM(A1:A2)"}]},
      {"name":"요약","cells":[{"ref":"B2","raw":"=매출!A3*2"},{"ref":"A1","raw":"[[SLIDE]]"}]},
      {"name":"빈 시트","cells":[]}
    ]}"#;
    let (_, bytes) = convert(payload);
    let workbook = zip_part(&bytes, "xl/workbook.xml");
    assert_order(
        &workbook,
        &["name=\"매출\"", "name=\"요약\"", "name=\"빈 시트\""],
    );
    let s1 = zip_part(&bytes, "xl/worksheets/sheet1.xml");
    assert!(s1.contains("<f>SUM(A1:A2)</f><v>30</v>"), "{s1}");
    let s2 = zip_part(&bytes, "xl/worksheets/sheet2.xml");
    assert!(s2.contains("<v>60</v>"), "{s2}");
    zip_part(&bytes, "xl/worksheets/sheet3.xml");
    let shared = zip_part(&bytes, "xl/sharedStrings.xml");
    assert!(shared.contains("[[SLIDE]]"), "{shared}");
}

#[test]
fn structured_slides_keep_user_text_literal() {
    let payload = r#"{"sourceFormat":"pptx","targetFormat":"pptx","title":"발표","slides":[
      {"title":"[[TABLE_NEW]]","bullets":["첫째;;둘째","[[SLIDE]]\tx"],"notes":"메모"}
    ]}"#;
    let (_, bytes) = convert(payload);
    let slide = zip_part(&bytes, "ppt/slides/slide1.xml");
    assert!(slide.contains("[[TABLE_NEW]]"), "{slide}");
    assert!(slide.contains("첫째;;둘째"), "{slide}");
}

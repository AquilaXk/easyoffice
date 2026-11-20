//! PD-263: a document saved to a format that holds no comments or tracked changes (HWPX, RTF, Markdown, text, PDF) says how
//! many were left out, and an HWPX save says it leaves out the first-page header and footer.

use easyoffice_app::EasyOfficeIpcRouter;

fn warnings_of(target: &str, extra: &str) -> String {
    let payload = format!(
        r#"{{"sourceFormat":"docx","targetFormat":"{target}","title":"검토 문서",{extra}
        "comments":[
          {{"id":1,"author":"김검토","date":"2026-11-19T09:00:00Z","text":"검토 메모"}},
          {{"id":2,"author":"박메모","date":"2026-11-19T09:15:00Z","text":"포인트 메모"}}
        ],
        "blocks":[{{"type":"paragraph","runs":[
          {{"text":"본문 "}},
          {{"text":"추가","revision":{{"id":1,"kind":"insert","author":"이작성","date":"2026-11-19T08:50:00Z"}}}},
          {{"text":"삭제","revision":{{"id":2,"kind":"delete","author":"이작성","date":"2026-11-19T08:55:00Z"}}}},
          {{"text":"범위","commentId":1}},
          {{"text":"","commentId":2}}
        ]}}]}}"#
    );
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload)
        .unwrap_or_else(|e| panic!("{target}: {e:?}"));
    let start = json.find("\"warnings\":[").expect("warnings") + 12;
    json[start..json[start..].find(']').unwrap() + start].to_string()
}

#[test]
fn formats_without_comments_or_tracked_changes_count_what_they_leave_out() {
    for (target, label) in [
        ("hwpx", "HWPX"),
        ("rtf", "RTF"),
        ("md", "MD"),
        ("txt", "TXT"),
        ("pdf", "PDF"),
    ] {
        let w = warnings_of(target, "");
        assert!(
            w.contains(&format!("{label} 저장에서 빠진 메모 2개")),
            "{target}: {w}"
        );
        assert!(
            w.contains(&format!("{label} 저장에서 빠진 변경 추적 2곳")),
            "{target}: {w}"
        );
    }
}

#[test]
fn formats_that_hold_them_do_not_warn() {
    for target in ["docx", "odt"] {
        let w = warnings_of(target, "");
        assert!(!w.contains("저장에서 빠진"), "{target}: {w}");
    }
}

#[test]
fn hwpx_says_it_leaves_out_the_first_page_header_and_footer() {
    let frame = r#""header":[{"type":"paragraph","runs":[{"text":"머리글"}]}],
        "firstHeader":[{"type":"paragraph","runs":[{"text":"표지 머리글"}]}],
        "firstFooter":[{"type":"paragraph","runs":[{"text":"표지 바닥글"}]}],"#;
    let w = warnings_of("hwpx", frame);
    assert!(
        w.contains("HWPX 저장에서 빠진 첫 쪽 머리글·바닥글 2개"),
        "{w}"
    );
}

fn json_str_field(json: &str, key: &str) -> String {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    json[start..end].to_string()
}

/// Opening a Pages file shows its header and footer after the body and keeps them in the page frame; the page sends both back
/// when it saves. The copies in the body must not be written as body text, or each open-and-save cycle adds one more.
#[test]
fn pages_header_and_footer_copies_do_not_pile_up_through_the_page_round_trip() {
    let path = format!(
        "{}/../easyoffice-formats/tests/fixtures/pages/eo-pages-objects.pages",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut bytes = std::fs::read(path).expect("fixture");
    let header = "머리말: Pages 객체 테스트 문서";
    for generation in 1..=4 {
        let b64 = EasyOfficeIpcRouter::encode_base64(&bytes);
        let decoded = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            &format!(
                r#"{{"action":"decode","fileName":"g{generation}.pages","sourceBase64":"{b64}"}}"#
            ),
        )
        .expect("decode");
        let copies = decoded.matches(&format!("\"text\":\"{header}\"")).count();
        // One in the page frame, one in the appendix of the body.
        assert_eq!(
            copies, 2,
            "generation {generation}: {copies} copies of the header text"
        );
        let save = decoded.replacen(r#""action":"decode""#, r#""targetFormat":"pages""#, 1);
        let saved = EasyOfficeIpcRouter::dispatch("convert_document_format", &save).expect("save");
        bytes =
            EasyOfficeIpcRouter::decode_base64(&json_str_field(&saved, "convertedBase64")).unwrap();
    }
}

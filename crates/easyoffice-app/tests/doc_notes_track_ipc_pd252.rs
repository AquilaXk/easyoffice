use easyoffice_app::{EasyOfficeIpcRouter, RibbonCommandRouter, RibbonWorkspaceState};
use easyoffice_formats::{
    base64_codec, EasyOfficeError, StrictJsonValue, MAX_AUTHOR_BYTES, MAX_COMMENTS_PER_DOCUMENT,
    MAX_DATE_BYTES, MAX_NOTES_PER_DOCUMENT, MAX_NOTE_TEXT_BYTES, MAX_REVISIONS_PER_DOCUMENT,
};

fn obj(v: &StrictJsonValue) -> &[(String, StrictJsonValue)] {
    v.as_object().expect("JSON object")
}

fn field<'a>(v: &'a StrictJsonValue, key: &str) -> Option<&'a StrictJsonValue> {
    StrictJsonValue::get_field(obj(v), key)
}

fn dispatch_json(payload: &str) -> StrictJsonValue {
    let resp = EasyOfficeIpcRouter::dispatch("convert_document_format", payload)
        .unwrap_or_else(|e| panic!("dispatch failed: {e}"));
    StrictJsonValue::parse(&resp).expect("valid JSON")
}

#[test]
fn ipc_save_decode_and_inspect_roundtrip_for_notes_comments_and_track_changes() {
    let payload = r#"{
      "sourceFormat":"docx",
      "targetFormat":"docx",
      "title":"검토 문서",
      "trackChanges":true,
      "notes":[
        {"id":1,"kind":"footnote","text":"각주 설명\n둘째 줄"},
        {"id":1,"kind":"endnote","text":"미주 출처"}
      ],
      "comments":[
        {"id":1,"author":"김검토","date":"2026-11-19T09:00:00Z","text":"검토 메모"},
        {"id":2,"author":"박메모","date":"2026-11-19T09:15:00Z","text":"포인트 메모"}
      ],
      "blocks":[
        {
          "type":"paragraph",
          "runs":[
            {"text":"본문 "},
            {"text":"추가","bold":true,"revision":{"id":1,"kind":"insert","author":"이작성","date":"2026-11-19T08:50:00Z"}},
            {"text":"삭제","revision":{"id":2,"kind":"delete","author":"이작성","date":"2026-11-19T08:55:00Z"}},
            {"text":"","script":"superscript","noteRef":{"kind":"footnote","id":1}}
          ]
        },
        {
          "type":"paragraph",
          "runs":[
            {"text":"코멘트 범위","commentId":1},
            {"text":"","commentId":2},
            {"text":" 끝"},
            {"text":"","script":"superscript","noteRef":{"kind":"endnote","id":1}}
          ]
        }
      ]
    }"#;

    for target in ["docx", "odt"] {
        let req = payload.replacen(
            "\"targetFormat\":\"docx\"",
            &format!("\"targetFormat\":\"{target}\""),
            1,
        );
        let saved = dispatch_json(&req);
        let b64 = field(&saved, "convertedBase64")
            .and_then(StrictJsonValue::as_str)
            .expect("convertedBase64");

        let decoded = dispatch_json(&format!(
            r#"{{"action":"decode","fileName":"notes.{target}","sourceBase64":"{b64}"}}"#
        ));
        assert_eq!(
            field(&decoded, "trackChanges").and_then(StrictJsonValue::as_bool),
            Some(true),
            "{target} trackChanges"
        );
        let notes = field(&decoded, "notes")
            .and_then(StrictJsonValue::as_array)
            .expect("notes");
        assert_eq!(notes.len(), 2, "{target} notes");
        assert_eq!(
            field(&notes[0], "text").and_then(StrictJsonValue::as_str),
            Some("각주 설명\n둘째 줄")
        );
        assert_eq!(
            field(&notes[1], "kind").and_then(StrictJsonValue::as_str),
            Some("endnote")
        );

        let comments = field(&decoded, "comments")
            .and_then(StrictJsonValue::as_array)
            .expect("comments");
        assert_eq!(comments.len(), 2, "{target} comments");
        assert_eq!(
            field(&comments[0], "author").and_then(StrictJsonValue::as_str),
            Some("김검토")
        );
        assert_eq!(
            field(&comments[1], "text").and_then(StrictJsonValue::as_str),
            Some("포인트 메모")
        );

        let blocks = field(&decoded, "blocks")
            .and_then(StrictJsonValue::as_array)
            .expect("blocks");
        let runs0 = field(&blocks[0], "runs")
            .and_then(StrictJsonValue::as_array)
            .expect("runs0");
        assert!(runs0.iter().any(|r| {
            field(r, "noteRef").is_some_and(|nr| {
                field(nr, "kind").and_then(StrictJsonValue::as_str) == Some("footnote")
                    && field(nr, "id").and_then(StrictJsonValue::as_u64) == Some(1)
            })
        }));
        assert!(runs0.iter().any(|r| {
            field(r, "text").and_then(StrictJsonValue::as_str) == Some("삭제")
                && field(r, "revision").is_some_and(|rev| {
                    field(rev, "kind").and_then(StrictJsonValue::as_str) == Some("delete")
                })
        }));

        let insp = dispatch_json(&format!(
            r#"{{"action":"inspect","fileName":"notes.{target}","sourceBase64":"{b64}"}}"#
        ));
        let stats = field(&insp, "statistics").expect("statistics");
        assert_eq!(
            field(stats, "footnoteCount").and_then(StrictJsonValue::as_u64),
            Some(1)
        );
        assert_eq!(
            field(stats, "endnoteCount").and_then(StrictJsonValue::as_u64),
            Some(1)
        );
        assert_eq!(
            field(stats, "commentCount").and_then(StrictJsonValue::as_u64),
            Some(2)
        );
        assert_eq!(
            field(stats, "revisionCount").and_then(StrictJsonValue::as_u64),
            Some(2)
        );
        assert_eq!(
            field(stats, "trackChanges").and_then(StrictJsonValue::as_bool),
            Some(true)
        );
        assert_eq!(
            field(stats, "footnoteSectionPresent").and_then(StrictJsonValue::as_bool),
            Some(true)
        );
        assert_eq!(
            field(&insp, "notes")
                .and_then(StrictJsonValue::as_array)
                .map(|a| a.len()),
            Some(2)
        );
        assert_eq!(
            field(&insp, "comments")
                .and_then(StrictJsonValue::as_array)
                .map(|a| a.len()),
            Some(2)
        );
        assert_eq!(
            field(&insp, "revisions")
                .and_then(StrictJsonValue::as_array)
                .map(|a| a.len()),
            Some(2)
        );
        let bytes = base64_codec::decode(b64).unwrap();
        assert!(!bytes.is_empty());
    }
}

#[test]
fn hostile_ipc_notes_comments_revisions_bounds_fail_closed() {
    // 1. > MAX_NOTES_PER_DOCUMENT
    let mut notes_json = String::new();
    for id in 1..=(MAX_NOTES_PER_DOCUMENT + 1) {
        if id > 1 {
            notes_json.push(',');
        }
        notes_json.push_str(&format!(r#"{{"id":{id},"kind":"footnote","text":"n"}}"#));
    }
    let err = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &format!(
            r#"{{"targetFormat":"docx","notes":[{notes_json}],"blocks":[{{"type":"paragraph","runs":[{{"text":"x"}}]}}]}}"#
        ),
    )
    .unwrap_err();
    assert!(matches!(err, EasyOfficeError::ResourceBudgetExceeded(_)));

    // 2. > MAX_COMMENTS_PER_DOCUMENT
    let mut cmts_json = String::new();
    for id in 1..=(MAX_COMMENTS_PER_DOCUMENT + 1) {
        if id > 1 {
            cmts_json.push(',');
        }
        cmts_json.push_str(&format!(
            r#"{{"id":{id},"author":"a","date":"2026","text":"c"}}"#
        ));
    }
    let err = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &format!(
            r#"{{"targetFormat":"docx","comments":[{cmts_json}],"blocks":[{{"type":"paragraph","runs":[{{"text":"x"}}]}}]}}"#
        ),
    )
    .unwrap_err();
    assert!(matches!(err, EasyOfficeError::ResourceBudgetExceeded(_)));

    // 3. > MAX_REVISIONS_PER_DOCUMENT
    let mut rev_runs = String::new();
    for id in 1..=(MAX_REVISIONS_PER_DOCUMENT + 1) {
        if id > 1 {
            rev_runs.push(',');
        }
        rev_runs.push_str(&format!(
            r#"{{"text":"x","revision":{{"id":{id},"kind":"insert","author":"a","date":"2026"}}}}"#
        ));
    }
    let err = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &format!(
            r#"{{"targetFormat":"docx","blocks":[{{"type":"paragraph","runs":[{rev_runs}]}}]}}"#
        ),
    )
    .unwrap_err();
    assert!(matches!(err, EasyOfficeError::ResourceBudgetExceeded(_)));

    // 4. > MAX_NOTE_TEXT_BYTES / MAX_AUTHOR_BYTES / MAX_DATE_BYTES
    let huge_text = "a".repeat(MAX_NOTE_TEXT_BYTES + 1);
    let err = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &format!(
            r#"{{"targetFormat":"docx","notes":[{{"id":1,"kind":"footnote","text":"{huge_text}"}}],"blocks":[{{"type":"paragraph","runs":[{{"text":"x"}}]}}]}}"#
        ),
    )
    .unwrap_err();
    assert!(matches!(err, EasyOfficeError::ResourceBudgetExceeded(_)));

    let huge_author = "a".repeat(MAX_AUTHOR_BYTES + 1);
    let err = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &format!(
            r#"{{"targetFormat":"docx","comments":[{{"id":1,"author":"{huge_author}","date":"2026","text":"ok"}}],"blocks":[{{"type":"paragraph","runs":[{{"text":"x"}}]}}]}}"#
        ),
    )
    .unwrap_err();
    assert!(matches!(err, EasyOfficeError::ResourceBudgetExceeded(_)));

    let huge_date = "2".repeat(MAX_DATE_BYTES + 1);
    let err = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &format!(
            r#"{{"targetFormat":"docx","blocks":[{{"type":"paragraph","runs":[{{"text":"x","revision":{{"id":1,"kind":"insert","author":"a","date":"{huge_date}"}}}}]}}]}}"#
        ),
    )
    .unwrap_err();
    assert!(matches!(err, EasyOfficeError::ResourceBudgetExceeded(_)));
}

#[test]
fn ribbon_review_comment_uses_official_korean_label() {
    let mut state = RibbonWorkspaceState::default();
    let title =
        RibbonCommandRouter::execute_command("document-review-comment", &mut state).unwrap();
    assert_eq!(title, "새 메모");
}

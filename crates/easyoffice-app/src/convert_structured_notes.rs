//! Structured JSON encode/decode helpers for footnotes, endnotes, comments, and tracked
//! changes (`PD-252`).

use std::fmt::Write;

use easyoffice_formats::{
    RichComment, RichDocument, RichNote, RichNoteKind, RichRevision, RichRevisionKind, RichRun,
    MAX_AUTHOR_BYTES, MAX_COMMENTS_PER_DOCUMENT, MAX_DATE_BYTES, MAX_NOTES_PER_DOCUMENT,
    MAX_NOTE_TEXT_BYTES, MAX_REVISIONS_PER_DOCUMENT,
};

use super::{as_array, as_object, field, invalid, opt_str, Budget, JsonObject};
use crate::EasyOfficeIpcRouter;

fn push_json_str(out: &mut String, s: &str) {
    out.push('"');
    EasyOfficeIpcRouter::escape_json_into(out, s);
    out.push('"');
}

pub(crate) fn write_run_annotations(out: &mut String, r: &RichRun) {
    if let Some((kind, id)) = r.note_ref {
        let _ = write!(
            out,
            ",\"noteRef\":{{\"kind\":\"{}\",\"id\":{id}}}",
            kind.wire_name()
        );
    }
    if let Some(cid) = r.comment_id {
        let _ = write!(out, ",\"commentId\":{cid}");
    }
    if let Some(rev) = &r.revision {
        let _ = write!(
            out,
            ",\"revision\":{{\"id\":{},\"kind\":\"{}\",\"author\":",
            rev.id,
            rev.kind.wire_name()
        );
        push_json_str(out, &rev.author);
        out.push_str(",\"date\":");
        push_json_str(out, &rev.date);
        out.push('}');
    }
}

pub(crate) fn write_doc_root_notes(out: &mut String, doc: &RichDocument) {
    if doc.track_changes {
        out.push_str(",\"trackChanges\":true");
    }
    let notes = doc.validated_notes();
    if !notes.is_empty() {
        out.push_str(",\"notes\":[");
        for (i, n) in notes.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "{{\"id\":{},\"kind\":\"{}\",\"text\":",
                n.id,
                n.kind.wire_name()
            );
            push_json_str(out, &n.text);
            out.push('}');
        }
        out.push(']');
    }
    let comments = doc.validated_comments();
    if !comments.is_empty() {
        out.push_str(",\"comments\":[");
        for (i, c) in comments.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let _ = write!(out, "{{\"id\":{},\"author\":", c.id);
            push_json_str(out, &c.author);
            out.push_str(",\"date\":");
            push_json_str(out, &c.date);
            out.push_str(",\"text\":");
            push_json_str(out, &c.text);
            out.push('}');
        }
        out.push(']');
    }
}

fn opt_u32_id(
    obj: &JsonObject,
    key: &str,
) -> Result<Option<u32>, easyoffice_formats::EasyOfficeError> {
    field(obj, key)
        .map(|v| {
            v.as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(|| invalid(format!("구조화 저장: '{key}'는 0 이상 정수여야 합니다.")))
        })
        .transpose()
}

fn req_u32_id(
    obj: &JsonObject,
    key: &str,
    ctx: &str,
) -> Result<u32, easyoffice_formats::EasyOfficeError> {
    opt_u32_id(obj, key)?
        .ok_or_else(|| invalid(format!("구조화 저장: {ctx}에 '{key}'가 필요합니다.")))
}

fn check_byte_limit(
    s: &str,
    max: usize,
    what: &str,
) -> Result<(), easyoffice_formats::EasyOfficeError> {
    if s.len() > max {
        return Err(easyoffice_formats::EasyOfficeError::ResourceBudgetExceeded(
            format!("구조화 저장: {what} 길이가 한도({max} bytes)를 초과했습니다."),
        ));
    }
    Ok(())
}

pub(super) fn parse_doc_notes(
    entries: &JsonObject,
    budget: &mut Budget,
) -> Result<Vec<RichNote>, easyoffice_formats::EasyOfficeError> {
    let Some(v) = field(entries, "notes") else {
        return Ok(Vec::new());
    };
    let arr = as_array(v, "notes")?;
    if arr.len() > MAX_NOTES_PER_DOCUMENT {
        return Err(easyoffice_formats::EasyOfficeError::ResourceBudgetExceeded(
            format!("구조화 저장: 각주·미주 수가 한도({MAX_NOTES_PER_DOCUMENT})를 초과했습니다."),
        ));
    }
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        budget.take(1)?;
        let obj = as_object(item, "notes")?;
        let id = req_u32_id(obj, "id", "notes")?;
        let kind_str = opt_str(obj, "kind")?.unwrap_or("");
        let kind = RichNoteKind::from_wire_name(kind_str)
            .ok_or_else(|| invalid(format!("구조화 저장: 알 수 없는 노트 종류 '{kind_str}'")))?;
        let text = opt_str(obj, "text")?.unwrap_or("");
        check_byte_limit(text, MAX_NOTE_TEXT_BYTES, "각주·미주 본문")?;
        out.push(RichNote {
            id,
            kind,
            text: text.to_string(),
        });
    }
    Ok(out)
}

pub(super) fn parse_doc_comments(
    entries: &JsonObject,
    budget: &mut Budget,
) -> Result<Vec<RichComment>, easyoffice_formats::EasyOfficeError> {
    let Some(v) = field(entries, "comments") else {
        return Ok(Vec::new());
    };
    let arr = as_array(v, "comments")?;
    if arr.len() > MAX_COMMENTS_PER_DOCUMENT {
        return Err(easyoffice_formats::EasyOfficeError::ResourceBudgetExceeded(
            format!("구조화 저장: 메모 수가 한도({MAX_COMMENTS_PER_DOCUMENT})를 초과했습니다."),
        ));
    }
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        budget.take(1)?;
        let obj = as_object(item, "comments")?;
        let id = req_u32_id(obj, "id", "comments")?;
        let author = opt_str(obj, "author")?.unwrap_or("");
        check_byte_limit(author, MAX_AUTHOR_BYTES, "메모 작성자")?;
        let date = opt_str(obj, "date")?.unwrap_or("");
        check_byte_limit(date, MAX_DATE_BYTES, "메모 날짜")?;
        let text = opt_str(obj, "text")?.unwrap_or("");
        check_byte_limit(text, MAX_NOTE_TEXT_BYTES, "메모 본문")?;
        out.push(RichComment {
            id,
            author: author.to_string(),
            date: date.to_string(),
            text: text.to_string(),
        });
    }
    Ok(out)
}

pub(super) type RunAnnotations = (
    Option<(RichNoteKind, u32)>,
    Option<u32>,
    Option<RichRevision>,
);

pub(super) fn parse_run_annotations(
    run: &JsonObject,
    budget: &mut Budget,
) -> Result<RunAnnotations, easyoffice_formats::EasyOfficeError> {
    let note_ref = match field(run, "noteRef") {
        None => None,
        Some(v) => {
            let nr = as_object(v, "noteRef")?;
            let kind_str = opt_str(nr, "kind")?.unwrap_or("");
            let kind = RichNoteKind::from_wire_name(kind_str).ok_or_else(|| {
                invalid(format!("구조화 저장: 알 수 없는 noteRef.kind '{kind_str}'"))
            })?;
            let id = req_u32_id(nr, "id", "noteRef")?;
            Some((kind, id))
        }
    };
    let comment_id = opt_u32_id(run, "commentId")?;
    let revision = match field(run, "revision") {
        None => None,
        Some(v) => {
            if budget.revisions >= MAX_REVISIONS_PER_DOCUMENT {
                return Err(easyoffice_formats::EasyOfficeError::ResourceBudgetExceeded(
                    format!(
                        "구조화 저장: 변경 이력 수가 한도({MAX_REVISIONS_PER_DOCUMENT})를 초과했습니다."
                    ),
                ));
            }
            budget.revisions += 1;
            let robj = as_object(v, "revision")?;
            let id = req_u32_id(robj, "id", "revision")?;
            let kind_str = opt_str(robj, "kind")?.unwrap_or("");
            let kind = RichRevisionKind::from_wire_name(kind_str).ok_or_else(|| {
                invalid(format!(
                    "구조화 저장: 알 수 없는 revision.kind '{kind_str}'"
                ))
            })?;
            let author = opt_str(robj, "author")?.unwrap_or("");
            check_byte_limit(author, MAX_AUTHOR_BYTES, "변경 이력 작성자")?;
            let date = opt_str(robj, "date")?.unwrap_or("");
            check_byte_limit(date, MAX_DATE_BYTES, "변경 이력 날짜")?;
            Some(RichRevision {
                id,
                kind,
                author: author.to_string(),
                date: date.to_string(),
            })
        }
    };
    Ok((note_ref, comment_id, revision))
}

//! The Word-style parts of the structured document JSON (PD-395), both ways.
//!
//! Run: `"fieldCode"` (the instruction of the field the run's text is the result of), `"bookmarkStart"` and
//! `"bookmarkEnd"` (a bookmark name). Paragraph: `"caption":true` and `"sectionBreak"` (`nextPage`, `continuous`,
//! `evenPage`, `oddPage`). Document: `"properties"` (`title`, `author`, `subject`, `keywords`, `comments`),
//! `"lineNumbers"` (`start`, `countBy`, `distance`, `restart`) and `"watermark"`. A value that is not one the formats keep
//! (see `easyoffice_formats::rich_word_features`) rejects the save instead of being dropped.

use std::fmt::Write;

use easyoffice_formats::json_out::push_json_string as push_json_str;
use easyoffice_formats::rich_word_features::{
    safe_watermark_text, MAX_LINE_NUMBER_VALUE, MAX_PROPERTY_BYTES,
};
use easyoffice_formats::{
    safe_bookmark_name, safe_field_code, EasyOfficeError, RichDocument, RichLineNumberRestart,
    RichLineNumbers, RichParagraph, RichProperties, RichRun, RichSectionBreak,
};

use super::{as_object, field, invalid, opt_bool, opt_bounded, opt_str, JsonObject};

/// The field and bookmark marks of one run.
#[derive(Default)]
pub(crate) struct RunWord {
    pub field_code: Option<String>,
    pub bookmark_start: Option<String>,
    pub bookmark_end: Option<String>,
}

fn bookmark(run: &JsonObject, key: &str) -> Result<Option<String>, EasyOfficeError> {
    match opt_str(run, key)?.filter(|n| !n.is_empty()) {
        None => Ok(None),
        Some(name) => safe_bookmark_name(name).map(Some).ok_or_else(|| {
            invalid(format!(
                "구조화 저장: '{key}'는 올바른 책갈피 이름이어야 합니다."
            ))
        }),
    }
}

pub(crate) fn parse_run_word(run: &JsonObject) -> Result<RunWord, EasyOfficeError> {
    let field_code = match opt_str(run, "fieldCode")?.filter(|c| !c.is_empty()) {
        None => None,
        Some(code) => Some(safe_field_code(code).ok_or_else(|| {
            invalid("구조화 저장: 'fieldCode'는 DATE·TIME·REF·SEQ 필드여야 합니다.")
        })?),
    };
    Ok(RunWord {
        field_code,
        bookmark_start: bookmark(run, "bookmarkStart")?,
        bookmark_end: bookmark(run, "bookmarkEnd")?,
    })
}

pub(crate) fn write_run_word(out: &mut String, r: &RichRun) {
    for (key, value) in [
        ("fieldCode", &r.field_code),
        ("bookmarkStart", &r.bookmark_start),
        ("bookmarkEnd", &r.bookmark_end),
    ] {
        if let Some(v) = value {
            let _ = write!(out, ",\"{key}\":");
            push_json_str(out, v);
        }
    }
    super::convert_structured_blocks::write_run_math(out, r);
}

/// `"caption"` and `"sectionBreak"` of a paragraph.
pub(crate) fn parse_paragraph_word(
    obj: &JsonObject,
) -> Result<(bool, Option<RichSectionBreak>), EasyOfficeError> {
    let section_break = match opt_str(obj, "sectionBreak")?.filter(|s| !s.is_empty()) {
        None => None,
        Some(name) => Some(
            RichSectionBreak::from_wire_name(name)
                .ok_or_else(|| invalid(format!("구조화 저장: 알 수 없는 구역 나누기 '{name}'")))?,
        ),
    };
    Ok((opt_bool(obj, "caption")?, section_break))
}

/// The paragraph flags of the open JSON: `pageBreakBefore`, `caption` and `sectionBreak`.
pub(crate) fn write_paragraph_word(out: &mut String, p: &RichParagraph) {
    if p.page_break_before {
        out.push_str(",\"pageBreakBefore\":true");
    }
    if p.caption {
        out.push_str(",\"caption\":true");
    }
    if let Some(kind) = p.section_break {
        let _ = write!(out, ",\"sectionBreak\":\"{}\"", kind.wire_name());
    }
    super::convert_structured_blocks::write_paragraph_blocks(
        out,
        p,
        crate::convert_open::write_paragraph,
    );
}

fn properties(entries: &JsonObject) -> Result<RichProperties, EasyOfficeError> {
    let Some(value) = field(entries, "properties") else {
        return Ok(RichProperties::default());
    };
    let obj = as_object(value, "properties")?;
    let text = |key: &str| -> Result<String, EasyOfficeError> {
        let raw = opt_str(obj, key)?.unwrap_or_default();
        if raw.len() > MAX_PROPERTY_BYTES * 4 {
            return Err(invalid(format!(
                "구조화 저장: 'properties.{key}'가 너무 깁니다."
            )));
        }
        Ok(raw.to_string())
    };
    Ok(RichProperties {
        title: text("title")?,
        author: text("author")?,
        subject: text("subject")?,
        keywords: text("keywords")?,
        comments: text("comments")?,
    }
    .bounded())
}

fn line_numbers(entries: &JsonObject) -> Result<Option<RichLineNumbers>, EasyOfficeError> {
    let Some(value) = field(entries, "lineNumbers") else {
        return Ok(None);
    };
    let obj = as_object(value, "lineNumbers")?;
    let number = |key: &str, min: i64, default: u32| -> Result<u32, EasyOfficeError> {
        Ok(
            opt_bounded(obj, key, min, i64::from(MAX_LINE_NUMBER_VALUE))?
                .map_or(default, |v| v as u32),
        )
    };
    let restart = match opt_str(obj, "restart")?.filter(|r| !r.is_empty()) {
        None => RichLineNumberRestart::default(),
        Some(name) => RichLineNumberRestart::from_wire_name(name).ok_or_else(|| {
            invalid(format!(
                "구조화 저장: 알 수 없는 줄 번호 다시 시작 '{name}'"
            ))
        })?,
    };
    RichLineNumbers {
        start: number("start", 0, 1)?,
        count_by: number("countBy", 1, 1)?,
        distance_twips: number("distance", 0, 0)?,
        restart,
    }
    .checked()
    .map(Some)
    .ok_or_else(|| invalid("구조화 저장: 'lineNumbers'가 범위를 벗어났습니다."))
}

/// `(properties, line numbers, watermark)` of a document payload.
pub(crate) fn parse_document_word(
    entries: &JsonObject,
) -> Result<(RichProperties, Option<RichLineNumbers>, Option<String>), EasyOfficeError> {
    let watermark = opt_str(entries, "watermark")?.and_then(safe_watermark_text);
    Ok((properties(entries)?, line_numbers(entries)?, watermark))
}

/// `,"properties":{…},"lineNumbers":{…},"watermark":"…"` of the open JSON, each when the document has it.
pub(crate) fn write_doc_word(out: &mut String, doc: &RichDocument) {
    if !doc.properties.is_empty() {
        out.push_str(",\"properties\":{");
        let mut first = true;
        for (key, value) in [
            ("author", &doc.properties.author),
            ("subject", &doc.properties.subject),
            ("keywords", &doc.properties.keywords),
            ("comments", &doc.properties.comments),
        ] {
            if value.is_empty() {
                continue;
            }
            if !first {
                out.push(',');
            }
            first = false;
            let _ = write!(out, "\"{key}\":");
            push_json_str(out, value);
        }
        out.push('}');
    }
    if let Some(n) = doc.page_frame.line_numbers {
        let _ = write!(
            out,
            ",\"lineNumbers\":{{\"start\":{},\"countBy\":{},\"distance\":{},\"restart\":\"{}\"}}",
            n.start,
            n.count_by,
            n.distance_twips,
            n.restart.wire_name()
        );
    }
    if let Some(text) = &doc.page_frame.watermark {
        out.push_str(",\"watermark\":");
        push_json_str(out, text);
    }
    super::convert_structured_blocks::write_doc_blocks(out, doc);
}

//! The document defaults of an opened DOCX, ODT or RTF (`RichDocument::defaults`, PD-363) on the
//! wire of the editor: the open answer carries them as `docDefaults`, the page keeps them with the
//! document and sends them back with the save, so a file opened and saved from the editor keeps
//! its own font, size, alignment, paragraph spacing, line height and tab stop.
//!
//! `{"font","eastAsianFont","sizeHalfPoints","alignment","spaceBeforeTwips","spaceAfterTwips",
//! "line240ths","tabStopTwips"}`; `font`, `eastAsianFont` and `alignment` are left out when the
//! file names none. The bounds are those the readers keep (`rich_read_*::defaults`), so what a
//! reader produced is always accepted back, and a value outside them rejects the save.

use std::fmt::Write;

use easyoffice_formats::json_out::push_json_string as push_json_str;
use easyoffice_formats::{
    RichAlignment, RichDefaults, RichDocument, StrictJsonValue, MAX_PARA_SPACING_TWIPS,
};

use super::{field, invalid, opt_bounded, opt_str, JsonObject};

/// Largest body size, in half-points: the reader's bound (1638 pt).
const MAX_HALF_POINTS: i64 = 3_276;
/// Largest line height, in 240ths of a line (ten lines).
const MAX_LINE_240THS: i64 = 2_400;
/// Largest tab stop, in twips: 22 inches.
const MAX_TAB_STOP_TWIPS: i64 = 31_680;
const MAX_FONT_CHARS: usize = 256;

fn alignment_name(alignment: RichAlignment) -> &'static str {
    match alignment {
        RichAlignment::Left => "left",
        RichAlignment::Center => "center",
        RichAlignment::Right => "right",
        RichAlignment::Justify => "justify",
    }
}

/// `,"docDefaults":{...}` of an opened document that has defaults; nothing for a new document.
pub(crate) fn write_doc_defaults(out: &mut String, doc: &RichDocument) {
    let Some(d) = &doc.defaults else {
        return;
    };
    out.push_str(",\"docDefaults\":{");
    for (key, value) in [("font", &d.font), ("eastAsianFont", &d.east_asian_font)] {
        if let Some(name) = value {
            let _ = write!(out, "\"{key}\":");
            push_json_str(out, name);
            out.push(',');
        }
    }
    if let Some(alignment) = d.alignment {
        let _ = write!(out, "\"alignment\":\"{}\",", alignment_name(alignment));
    }
    let _ = write!(
        out,
        "\"sizeHalfPoints\":{},\"spaceBeforeTwips\":{},\"spaceAfterTwips\":{},\"line240ths\":{},\"tabStopTwips\":{}}}",
        d.size_half_points, d.space_before_twips, d.space_after_twips, d.line_240ths, d.tab_stop_twips
    );
}

/// A typeface name: 1 to 256 characters, no control character; empty or absent is none.
fn typeface(
    obj: &JsonObject,
    key: &str,
) -> Result<Option<String>, easyoffice_formats::EasyOfficeError> {
    match opt_str(obj, key)?.map(str::trim).filter(|f| !f.is_empty()) {
        None => Ok(None),
        Some(name) if name.chars().count() <= MAX_FONT_CHARS && !name.chars().any(char::is_control) => {
            Ok(Some(name.to_string()))
        }
        Some(_) => Err(invalid(format!(
            "구조화 저장: 'docDefaults.{key}'는 제어 문자 없는 {MAX_FONT_CHARS}자 이하 이름이어야 합니다."
        ))),
    }
}

fn required(
    obj: &JsonObject,
    key: &str,
    min: i64,
    max: i64,
) -> Result<i64, easyoffice_formats::EasyOfficeError> {
    opt_bounded(obj, key, min, max)?
        .ok_or_else(|| invalid(format!("구조화 저장: 'docDefaults.{key}'가 없습니다.")))
}

/// The `docDefaults` of a save payload; `None` when the payload has none (a new document).
pub(crate) fn parse_doc_defaults(
    entries: &JsonObject,
) -> Result<Option<RichDefaults>, easyoffice_formats::EasyOfficeError> {
    let Some(value) = field(entries, "docDefaults") else {
        return Ok(None);
    };
    let obj = match value {
        StrictJsonValue::Null => return Ok(None),
        other => other
            .as_object()
            .ok_or_else(|| invalid("구조화 저장: 'docDefaults'는 객체여야 합니다."))?,
    };
    let alignment = match opt_str(obj, "alignment")?.filter(|a| !a.is_empty()) {
        None => None,
        Some(token) => Some(RichAlignment::parse(token).ok_or_else(|| {
            invalid("구조화 저장: 'docDefaults.alignment'는 left·center·right·justify여야 합니다.")
        })?),
    };
    let spacing = i64::from(MAX_PARA_SPACING_TWIPS);
    Ok(Some(RichDefaults {
        font: typeface(obj, "font")?,
        east_asian_font: typeface(obj, "eastAsianFont")?,
        size_half_points: required(obj, "sizeHalfPoints", 1, MAX_HALF_POINTS)? as u16,
        alignment,
        space_before_twips: required(obj, "spaceBeforeTwips", 0, spacing)? as u32,
        space_after_twips: required(obj, "spaceAfterTwips", 0, spacing)? as u32,
        line_240ths: required(obj, "line240ths", 1, MAX_LINE_240THS)? as u32,
        tab_stop_twips: required(obj, "tabStopTwips", 1, MAX_TAB_STOP_TWIPS)? as u32,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Result<Option<RichDefaults>, easyoffice_formats::EasyOfficeError> {
        let value = StrictJsonValue::parse(json).unwrap();
        parse_doc_defaults(value.as_object().unwrap())
    }

    const FULL: &str = r#"{"docDefaults":{"font":"Calibri","eastAsianFont":"맑은 고딕","sizeHalfPoints":22,"alignment":"left","spaceBeforeTwips":0,"spaceAfterTwips":200,"line240ths":276,"tabStopTwips":720}}"#;

    #[test]
    fn a_payload_without_defaults_is_a_new_document() {
        assert_eq!(parse("{}").unwrap(), None);
        assert_eq!(parse(r#"{"docDefaults":null}"#).unwrap(), None);
    }

    #[test]
    fn the_defaults_come_back_as_they_were_written() {
        let defaults = parse(FULL).unwrap().unwrap();
        assert_eq!(defaults.font.as_deref(), Some("Calibri"));
        assert_eq!(defaults.east_asian_font.as_deref(), Some("맑은 고딕"));
        assert_eq!(
            (
                defaults.size_half_points,
                defaults.alignment,
                defaults.space_after_twips
            ),
            (22, Some(RichAlignment::Left), 200)
        );
        let doc = RichDocument {
            defaults: Some(defaults.clone()),
            ..RichDocument::default()
        };
        let mut wire = String::from("{\"x\":0");
        write_doc_defaults(&mut wire, &doc);
        wire.push('}');
        let again = StrictJsonValue::parse(&wire).unwrap();
        assert_eq!(
            parse_doc_defaults(again.as_object().unwrap()).unwrap(),
            Some(defaults)
        );
    }

    #[test]
    fn a_document_without_defaults_writes_nothing_and_a_start_edge_file_keeps_none() {
        let mut wire = String::new();
        write_doc_defaults(&mut wire, &RichDocument::default());
        assert_eq!(wire, "");
        let start_edge = RichDocument {
            defaults: Some(RichDefaults::default()),
            ..RichDocument::default()
        };
        write_doc_defaults(&mut wire, &start_edge);
        assert!(
            !wire.contains("alignment") && !wire.contains("\"font\""),
            "{wire}"
        );
        let again = StrictJsonValue::parse(&format!("{{{}}}", &wire[1..])).unwrap();
        assert_eq!(
            parse_doc_defaults(again.as_object().unwrap()).unwrap(),
            Some(RichDefaults::default())
        );
    }

    #[test]
    fn a_save_payload_carries_the_defaults_into_the_document_it_writes() {
        use crate::convert_structured::{parse_structured_content, StructuredContent};
        let payload = StrictJsonValue::parse(&format!(
            r#"{{"blocks":[],{}"#,
            FULL.trim_start_matches('{')
        ))
        .unwrap();
        let (content, _) = parse_structured_content(payload.as_object().unwrap())
            .unwrap()
            .unwrap();
        let StructuredContent::Document(doc, _) = content else {
            panic!("a document");
        };
        assert_eq!(doc.defaults.unwrap().font.as_deref(), Some("Calibri"));
        let bare = StrictJsonValue::parse(r#"{"blocks":[]}"#).unwrap();
        let (content, _) = parse_structured_content(bare.as_object().unwrap())
            .unwrap()
            .unwrap();
        let StructuredContent::Document(doc, _) = content else {
            panic!("a document");
        };
        assert_eq!(
            doc.defaults, None,
            "a new document keeps the Korean defaults"
        );
    }

    #[test]
    fn hostile_values_reject_the_save() {
        for bad in [
            r#"{"docDefaults":[]}"#,
            r#"{"docDefaults":{}}"#,
            r#"{"docDefaults":{"sizeHalfPoints":0,"spaceBeforeTwips":0,"spaceAfterTwips":0,"line240ths":240,"tabStopTwips":720}}"#,
            r#"{"docDefaults":{"sizeHalfPoints":3277,"spaceBeforeTwips":0,"spaceAfterTwips":0,"line240ths":240,"tabStopTwips":720}}"#,
            r#"{"docDefaults":{"sizeHalfPoints":20,"spaceBeforeTwips":8001,"spaceAfterTwips":0,"line240ths":240,"tabStopTwips":720}}"#,
            r#"{"docDefaults":{"sizeHalfPoints":20,"spaceBeforeTwips":-1,"spaceAfterTwips":0,"line240ths":240,"tabStopTwips":720}}"#,
            r#"{"docDefaults":{"sizeHalfPoints":20,"spaceBeforeTwips":0,"spaceAfterTwips":0,"line240ths":0,"tabStopTwips":720}}"#,
            r#"{"docDefaults":{"sizeHalfPoints":20,"spaceBeforeTwips":0,"spaceAfterTwips":0,"line240ths":2401,"tabStopTwips":720}}"#,
            r#"{"docDefaults":{"sizeHalfPoints":20,"spaceBeforeTwips":0,"spaceAfterTwips":0,"line240ths":240,"tabStopTwips":31681}}"#,
            r#"{"docDefaults":{"sizeHalfPoints":20,"spaceBeforeTwips":0,"spaceAfterTwips":0,"line240ths":240,"tabStopTwips":720,"alignment":"diagonal"}}"#,
            r#"{"docDefaults":{"sizeHalfPoints":"20","spaceBeforeTwips":0,"spaceAfterTwips":0,"line240ths":240,"tabStopTwips":720}}"#,
            r#"{"docDefaults":{"sizeHalfPoints":20.5,"spaceBeforeTwips":0,"spaceAfterTwips":0,"line240ths":240,"tabStopTwips":720}}"#,
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
        let long_font = format!(
            r#"{{"docDefaults":{{"font":"{}","sizeHalfPoints":20,"spaceBeforeTwips":0,"spaceAfterTwips":0,"line240ths":240,"tabStopTwips":720}}}}"#,
            "가".repeat(257)
        );
        assert!(parse(&long_font).is_err());
        let control_font = r#"{"docDefaults":{"font":"a\u0007b","sizeHalfPoints":20,"spaceBeforeTwips":0,"spaceAfterTwips":0,"line240ths":240,"tabStopTwips":720}}"#;
        assert!(parse(control_font).is_err());
    }
}

//! A paragraph's line spacing in the open and save JSON (PD-365). A multiple keeps its original
//! key, `"lineSpacingPercent"` (1 to `MAX_LINE_PERCENT`). The other rules name their kind and give
//! twips: `"lineSpacingKind":"atLeast"|"exact"|"between"` with `"lineSpacingTwips"`
//! (`RichLineSpacing::kind_token`). A kind without twips, an unknown kind, a value out of bounds
//! or both forms in one paragraph reject the save.

use std::fmt::Write;

use easyoffice_formats::{
    EasyOfficeError, RichLineSpacing, MAX_LINE_PERCENT, MAX_LINE_SPACING_TWIPS,
};

use crate::convert_structured::{invalid, opt_bounded, opt_str, JsonObject};

const KIND_KEY: &str = "lineSpacingKind";
const TWIPS_KEY: &str = "lineSpacingTwips";
const PERCENT_KEY: &str = "lineSpacingPercent";

/// Appends the line spacing keys (with a leading comma) to a paragraph object.
pub(crate) fn write_line_spacing_json(out: &mut String, spacing: RichLineSpacing) {
    let _ = match spacing {
        RichLineSpacing::Multiple(percent) => write!(out, ",\"{PERCENT_KEY}\":{percent}"),
        other => write!(
            out,
            ",\"{KIND_KEY}\":\"{}\",\"{TWIPS_KEY}\":{}",
            other.kind_token(),
            other.amount()
        ),
    };
}

/// Reads the line spacing keys of a paragraph object.
pub(crate) fn parse_line_spacing(
    obj: &JsonObject,
) -> Result<Option<RichLineSpacing>, EasyOfficeError> {
    let percent = opt_bounded(obj, PERCENT_KEY, 1, i64::from(MAX_LINE_PERCENT))?;
    let Some(kind) = opt_str(obj, KIND_KEY)? else {
        return Ok(percent.and_then(RichLineSpacing::multiple));
    };
    if percent.is_some() {
        return Err(invalid(format!(
            "구조화 저장: '{PERCENT_KEY}'와 '{KIND_KEY}'는 함께 쓸 수 없습니다."
        )));
    }
    let twips =
        opt_bounded(obj, TWIPS_KEY, 0, i64::from(MAX_LINE_SPACING_TWIPS))?.ok_or_else(|| {
            invalid(format!(
                "구조화 저장: '{KIND_KEY}'에는 '{TWIPS_KEY}'가 필요합니다."
            ))
        })?;
    match kind {
        "atLeast" | "exact" | "between" => RichLineSpacing::from_token(kind, twips)
            .map(Some)
            .ok_or_else(|| {
                invalid(format!(
                    "구조화 저장: '{TWIPS_KEY}'는 1~{MAX_LINE_SPACING_TWIPS} 정수여야 합니다."
                ))
            }),
        other => Err(invalid(format!(
            "구조화 저장: 알 수 없는 줄 간격 방식 '{other}'"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use easyoffice_formats::StrictJsonValue;

    fn parse(json: &str) -> Result<Option<RichLineSpacing>, EasyOfficeError> {
        let value = StrictJsonValue::parse(json).expect("json");
        parse_line_spacing(value.as_object().expect("object"))
    }

    #[test]
    fn every_kind_roundtrips_through_the_json_keys() {
        for spacing in [
            RichLineSpacing::Multiple(150),
            RichLineSpacing::AtLeast(360),
            RichLineSpacing::Exact(300),
            RichLineSpacing::Between(0),
            RichLineSpacing::Between(120),
        ] {
            let mut out = String::from("{\"type\":\"paragraph\"");
            write_line_spacing_json(&mut out, spacing);
            out.push('}');
            assert_eq!(parse(&out).unwrap(), Some(spacing), "{out}");
        }
        assert_eq!(parse("{}").unwrap(), None);
    }

    #[test]
    fn hostile_values_are_rejected() {
        for bad in [
            r#"{"lineSpacingPercent":0}"#,
            r#"{"lineSpacingPercent":1001}"#,
            r#"{"lineSpacingPercent":"150"}"#,
            r#"{"lineSpacingKind":"exact"}"#,
            r#"{"lineSpacingKind":"exact","lineSpacingTwips":0}"#,
            r#"{"lineSpacingKind":"exact","lineSpacingTwips":31681}"#,
            r#"{"lineSpacingKind":"exact","lineSpacingTwips":-5}"#,
            r#"{"lineSpacingKind":"atLeast","lineSpacingTwips":9223372036854775807}"#,
            r#"{"lineSpacingKind":"between","lineSpacingTwips":-1}"#,
            r#"{"lineSpacingKind":"multiple","lineSpacingTwips":150}"#,
            r#"{"lineSpacingKind":"fixed","lineSpacingTwips":150}"#,
            r#"{"lineSpacingKind":7,"lineSpacingTwips":150}"#,
            r#"{"lineSpacingKind":"exact","lineSpacingTwips":300,"lineSpacingPercent":150}"#,
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }
}

//! JSON form of a workbook's default font (PD-306): `{"name": "Arial", "size": 12}`, the font of the Normal cell style whose
//! widest digit is the unit of the column widths in characters. The open answer says it when it is not the standard one (Malgun
//! Gothic 11 pt), the page keeps it and sends it back with every save and every row or column edit, so the engine writes the
//! file's own font again and counts the widths in the same digit.
//!
//! Input over a bound is refused, not trimmed: a name is a font name (`clean_font_name`: 64 characters, no control or markup
//! characters) and a size is a number above 0 up to 409 pt.

use easyoffice_formats::json_out::push_json_string;
use easyoffice_formats::{
    default_col_px, DefaultFont, EasyOfficeError, SheetStyling, StrictJsonValue, STANDARD_DIGIT_PX,
};

fn bad() -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(
        "defaultFont는 {name, size} 형태의 한도 안의 글꼴이어야 합니다.".into(),
    )
}

/// `defaultFont` of a request: `null` is the standard font. Any other shape, an unknown key, a name that is not a font name or a
/// size out of bounds is refused. A font that is the standard one is stored as none.
pub(crate) fn parse_default_font(
    v: &StrictJsonValue,
) -> Result<Option<DefaultFont>, EasyOfficeError> {
    if matches!(v, StrictJsonValue::Null) {
        return Ok(None);
    }
    let obj = v
        .as_object()
        .filter(|o| o.iter().all(|(key, _)| key == "name" || key == "size"))
        .ok_or_else(bad)?;
    let name = StrictJsonValue::get_field(obj, "name")
        .and_then(StrictJsonValue::as_str)
        .ok_or_else(bad)?;
    let size = match StrictJsonValue::get_field(obj, "size") {
        Some(StrictJsonValue::Number(text)) => text.parse::<f64>().map_err(|_| bad())?,
        _ => return Err(bad()),
    };
    Ok(DefaultFont::new(name, size).ok_or_else(bad)?.non_standard())
}

/// The `defaultFont` field of an open answer (without the leading comma), and `defaultColPx`: the width in pixels of a column
/// in a sheet that says nothing, which the page needs for the sheets it adds. Nothing for the standard font.
pub(crate) fn write_default_font(out: &mut String, styling: &SheetStyling) {
    let Some(font) = &styling.default_font else {
        return;
    };
    out.push_str(",\"defaultFont\":{\"name\":");
    push_json_string(out, font.name());
    out.push_str(&format!(
        ",\"size\":{}}},\"defaultColPx\":{}",
        font.size_pt(),
        default_col_px(None, font.digit_px())
    ));
}

/// The digit of `font` in pixels: the standard font's when there is none.
pub(crate) fn digit_of(font: Option<&DefaultFont>) -> u32 {
    font.map_or(STANDARD_DIGIT_PX, DefaultFont::digit_px)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Option<DefaultFont>, EasyOfficeError> {
        parse_default_font(&StrictJsonValue::parse(text).unwrap())
    }

    #[test]
    fn a_font_round_trips_and_the_standard_one_is_none() {
        assert_eq!(parse("null").unwrap(), None);
        assert_eq!(parse(r#"{"name":"맑은 고딕","size":11}"#).unwrap(), None);
        let arial = parse(r#"{"name":"Arial","size":12}"#).unwrap().unwrap();
        assert_eq!(
            (arial.name(), arial.size_pt(), arial.digit_px()),
            ("Arial", 12.0, 9)
        );
        let styling = SheetStyling {
            default_font: Some(arial),
            ..SheetStyling::default()
        };
        let mut out = String::new();
        write_default_font(&mut out, &styling);
        assert_eq!(
            out,
            r#","defaultFont":{"name":"Arial","size":12},"defaultColPx":80"#
        );
        let mut none = String::new();
        write_default_font(&mut none, &SheetStyling::default());
        assert_eq!(none, "");
        let fractional = parse(r#"{"name":"Arial","size":10.25}"#).unwrap().unwrap();
        assert_eq!(fractional.size_pt(), 10.25);
        assert_eq!(digit_of(None), 8);
    }

    #[test]
    fn hostile_fonts_are_refused() {
        let long = format!(r#"{{"name":"{}","size":11}}"#, "F".repeat(65));
        let fine = format!(r#"{{"name":"{}","size":409}}"#, "F".repeat(64));
        assert!(parse(&fine).unwrap().is_some());
        for text in [
            long.as_str(),
            r#"{"name":"Arial","size":409.5}"#,
            r#"{"name":"Arial","size":0}"#,
            r#"{"name":"Arial","size":-1}"#,
            r#"{"name":"Arial","size":1e999}"#,
            r#"{"name":"Arial","size":"11"}"#,
            r#"{"name":"Arial"}"#,
            r#"{"size":11}"#,
            r#"{"name":"","size":11}"#,
            r#"{"name":"Bad;Font","size":11}"#,
            r#"{"name":"A\nB","size":11}"#,
            r#"{"name":"<b>","size":11}"#,
            r#"{"name":1,"size":11}"#,
            r#"{"name":"Arial","size":11,"extra":1}"#,
            r#"{"name":"Arial","size":11,"colPx":80}"#,
            r#""Arial""#,
            r#"[]"#,
            r#"12"#,
            r#"true"#,
        ] {
            assert!(parse(text).is_err(), "{text}");
        }
    }
}

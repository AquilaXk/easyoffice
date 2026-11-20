//! Display text of formatted cells (PD-193).
//!
//! `evaluate_sheet` with `sessionAction:"format"` takes the cells the page is about to draw,
//! grouped by number format code, and answers with the text each one shows. The page keeps the
//! answers by code and value, so a repeated value is never sent twice. The command set stays at 15.
//!
//! Request `{items:[{code, values:[text...]}]}`, answer `{items:[{t:[text|null...], c:{index:"#rrggbb"},
//! f:{index:[utf16Offset, "char"]}}]}`: `null` means the cell keeps its General display, `c` lists the
//! section colors by value index and `f` (only when a value has one) the `*x` fill of a value (the
//! printed page repeats the character over the free width of the cell, PD-235).
//! A code over the bounds of `sheet_numfmt` answers `{bad:true}` and its cells stay General.

use crate::EasyOfficeIpcRouter;
use easyoffice_formats::{code_within_bounds, CellValueKind, EasyOfficeError, StrictJsonValue};
use easyoffice_sheet::format_typed_cell_value;
use std::fmt::Write;

/// Codes in one request (the engine keeps 256 compiled formats).
const MAX_ITEMS: usize = 256;
/// Values in one request over all codes.
const MAX_VALUES: usize = 50_000;
/// A longer value is text no number format changes.
const MAX_VALUE_BYTES: usize = 512;

fn invalid(msg: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(msg.to_string())
}

use easyoffice_formats::json_out::push_json_string;

/// One item: `{t:[...],c:{...}}` for the values of one code.
fn render_item(
    out: &mut String,
    code: &str,
    values: &[StrictJsonValue],
    text_only: bool,
) -> Result<(), EasyOfficeError> {
    if !code_within_bounds(code) {
        out.push_str("{\"bad\":true}");
        return Ok(());
    }
    let mut colors = String::new();
    let mut fills = String::new();
    out.push_str("{\"t\":[");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let raw = value
            .as_str()
            .ok_or_else(|| invalid("서식 'values' 항목은 문자열이어야 합니다."))?;
        let shown = (raw.len() <= MAX_VALUE_BYTES)
            .then(|| {
                let kind = if text_only {
                    CellValueKind::Text
                } else {
                    CellValueKind::infer(raw)
                };
                format_typed_cell_value(code, raw, kind)
            })
            .flatten();
        let Some(shown) = shown else {
            out.push_str("null");
            continue;
        };
        push_json_string(out, &shown.text);
        if let Some(color) = shown.color {
            if !colors.is_empty() {
                colors.push(',');
            }
            let _ = write!(colors, "\"{index}\":\"{}\"", color.hex());
        }
        if let Some((at, ch)) = shown
            .fill
            .filter(|(at, _)| shown.text.is_char_boundary(*at))
        {
            if !fills.is_empty() {
                fills.push(',');
            }
            let unit = shown.text[..at].encode_utf16().count();
            let _ = write!(fills, "\"{index}\":[{unit},\"");
            fills.push_str(&EasyOfficeIpcRouter::escape_json_str(&ch.to_string()));
            fills.push_str("\"]");
        }
    }
    let _ = write!(out, "],\"c\":{{{colors}}}");
    if !fills.is_empty() {
        let _ = write!(out, ",\"f\":{{{fills}}}");
    }
    out.push('}');
    Ok(())
}

impl EasyOfficeIpcRouter {
    pub(crate) fn sheet_format(
        entries: &[(String, StrictJsonValue)],
    ) -> Result<String, EasyOfficeError> {
        let items = StrictJsonValue::get_field(entries, "items")
            .and_then(StrictJsonValue::as_array)
            .ok_or_else(|| invalid("JSON 'items' 필드는 배열이어야 합니다."))?;
        if items.len() > MAX_ITEMS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "서식 코드는 한 번에 최대 {MAX_ITEMS}개입니다."
            )));
        }
        let mut total = 0usize;
        let mut out = String::from("{\"items\":[");
        for (index, item) in items.iter().enumerate() {
            let fields = item
                .as_object()
                .ok_or_else(|| invalid("서식 항목은 객체여야 합니다."))?;
            let code = Self::optional_json_str(fields, "code")?.unwrap_or("");
            let text_only = matches!(
                StrictJsonValue::get_field(fields, "textOnly"),
                Some(StrictJsonValue::Bool(true))
            );
            let values = StrictJsonValue::get_field(fields, "values")
                .and_then(StrictJsonValue::as_array)
                .ok_or_else(|| invalid("서식 'values' 필드는 배열이어야 합니다."))?;
            total = total.saturating_add(values.len());
            if total > MAX_VALUES {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "서식 값은 한 번에 최대 {MAX_VALUES}개입니다."
                )));
            }
            if index > 0 {
                out.push(',');
            }
            render_item(&mut out, code, values, text_only)?;
        }
        out.push_str("]}");
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use crate::EasyOfficeIpcRouter;

    fn call(body: &str) -> Result<String, easyoffice_formats::EasyOfficeError> {
        EasyOfficeIpcRouter::dispatch("evaluate_sheet", body)
    }

    #[test]
    fn answers_text_color_and_general_per_value() {
        let answer = call(
            r##"{"sessionAction":"format","items":[{"code":"#,##0.00;[Red]-#,##0.00","values":["1234.5","-2","abc",""]}]}"##,
        )
        .unwrap();
        assert_eq!(
            answer,
            r##"{"items":[{"t":["1,234.50","-2.00",null,null],"c":{"1":"#ff0000"}}]}"##
        );
    }

    #[test]
    fn an_asterisk_fill_answers_its_offset_and_character() {
        let answer = call(
            r#"{"sessionAction":"format","items":[{"code":"_($* #,##0.00_)","values":["7.5"]}]}"#,
        )
        .unwrap();
        assert_eq!(
            answer,
            r#"{"items":[{"t":[" $7.50 "],"c":{},"f":{"0":[2," "]}}]}"#
        );
    }

    #[test]
    fn text_sections_and_dates_render() {
        let answer = call(
            r#"{"sessionAction":"format","items":[{"code":"yyyy-mm-dd","values":["45000"]},{"code":"@","values":["007"]}]}"#,
        )
        .unwrap();
        assert_eq!(
            answer,
            r#"{"items":[{"t":["2023-03-15"],"c":{}},{"t":["007"],"c":{}}]}"#
        );
    }

    #[test]
    fn a_number_under_the_text_format_shows_as_the_text_function_writes_it() {
        let answer =
            call(r#"{"sessionAction":"format","items":[{"code":"@","values":["1e10","12"]}]}"#)
                .unwrap();
        assert_eq!(answer, r#"{"items":[{"t":["10000000000","12"],"c":{}}]}"#);
    }

    #[test]
    fn a_code_over_the_bounds_stays_general() {
        let long = "0".repeat(300);
        let answer = call(&format!(
            r#"{{"sessionAction":"format","items":[{{"code":"{long}","values":["1"]}},{{"code":"0;0;0;0;0","values":["1"]}}]}}"#
        ))
        .unwrap();
        assert_eq!(answer, r#"{"items":[{"bad":true},{"bad":true}]}"#);
    }

    #[test]
    fn requests_are_bounded() {
        let many = vec!["\"1\""; 50_001].join(",");
        let err = call(&format!(
            r#"{{"sessionAction":"format","items":[{{"code":"0","values":[{many}]}}]}}"#
        ))
        .unwrap_err();
        assert!(matches!(
            err,
            easyoffice_formats::EasyOfficeError::ResourceBudgetExceeded(_)
        ));
        let items = vec![r#"{"code":"0","values":[]}"#; 257].join(",");
        assert!(call(&format!(
            r#"{{"sessionAction":"format","items":[{items}]}}"#
        ))
        .is_err());
        assert!(call(r#"{"sessionAction":"format","items":[{"code":"0","values":[1]}]}"#).is_err());
    }

    #[test]
    fn a_long_value_is_left_alone() {
        let value = "1".repeat(600);
        let answer = call(&format!(
            r#"{{"sessionAction":"format","items":[{{"code":"0.00","values":["{value}"]}}]}}"#
        ))
        .unwrap();
        assert_eq!(answer, r#"{"items":[{"t":[null],"c":{}}]}"#);
    }
}

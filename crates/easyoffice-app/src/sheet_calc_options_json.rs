//! JSON serialization and validation of workbook calculation options (`calcOptions`, PD-309).

use std::borrow::Cow;

use easyoffice_formats::{
    CalcMode, CalcOptions, EasyOfficeError, SheetStyling, StrictJsonBorrow, StrictJsonValue,
    DEFAULT_ITERATE_DELTA,
};
use easyoffice_sheet::{enter_calc_options, CalcOptionsGuard};

fn bad(msg: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(format!("calcOptions 형식이 올바르지 않습니다: {msg}"))
}

enum JsonAtom<'a> {
    Bool(bool),
    Str(&'a str),
    Number(&'a str),
    Other,
}

fn parse_iterate_count(atom: JsonAtom<'_>) -> Result<u16, EasyOfficeError> {
    let raw = match atom {
        JsonAtom::Number(s) => s.trim(),
        _ => return Err(bad("iterateCount는 1..=32767 범위의 정수여야 합니다.")),
    };
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad("iterateCount는 1..=32767 범위의 정수여야 합니다."));
    }
    let count = raw
        .parse::<u16>()
        .map_err(|_| bad("iterateCount는 1..=32767 범위의 정수여야 합니다."))?;
    if !CalcOptions::valid_iterate_count(count) {
        return Err(bad("iterateCount는 1..=32767 범위여야 합니다."));
    }
    Ok(count)
}

fn parse_iterate_delta(atom: JsonAtom<'_>) -> Result<f64, EasyOfficeError> {
    let raw = match atom {
        JsonAtom::Number(s) => s.trim(),
        _ => {
            return Err(bad(
                "iterateDelta는 0 이상 1e100 이하의 유한한 수여야 합니다.",
            ))
        }
    };
    if raw.is_empty() || raw.starts_with('-') {
        return Err(bad(
            "iterateDelta는 0 이상 1e100 이하의 유한한 수여야 합니다.",
        ));
    }
    let val = raw
        .parse::<f64>()
        .map_err(|_| bad("iterateDelta는 0 이상 1e100 이하의 유한한 수여야 합니다."))?;
    CalcOptions::sanitized_delta(val)
        .ok_or_else(|| bad("iterateDelta는 0 이상 1e100 이하의 유한한 수여야 합니다."))
}

fn parse_bool(key: &str, atom: JsonAtom<'_>) -> Result<bool, EasyOfficeError> {
    match atom {
        JsonAtom::Bool(b) => Ok(b),
        _ => Err(bad(&format!("{key}는 불리언이어야 합니다."))),
    }
}

fn parse_calc_fields<'a>(
    fields: impl Iterator<Item = (&'a str, JsonAtom<'a>)>,
) -> Result<CalcOptions, EasyOfficeError> {
    let mut opts = CalcOptions::default();
    for (key, atom) in fields {
        match key {
            "mode" => {
                let raw = match atom {
                    JsonAtom::Str(s) => s,
                    _ => return Err(bad("mode는 문자열이어야 합니다.")),
                };
                opts.mode =
                    CalcMode::parse(raw).ok_or_else(|| bad(&format!("알 수 없는 mode: {raw}")))?;
            }
            "calcOnSave" => opts.calc_on_save = parse_bool("calcOnSave", atom)?,
            "fullCalcOnLoad" => opts.full_calc_on_load = parse_bool("fullCalcOnLoad", atom)?,
            "iterate" => opts.iterate = parse_bool("iterate", atom)?,
            "iterateCount" => opts.iterate_count = parse_iterate_count(atom)?,
            "iterateDelta" => opts.iterate_delta = parse_iterate_delta(atom)?,
            other => return Err(bad(&format!("알 수 없는 키: {other}"))),
        }
    }
    if opts.mode != CalcMode::Manual {
        opts.full_calc_on_load = false;
    }
    if !opts.within_bounds() {
        return Err(bad("반복 계산 한도를 벗어났습니다."));
    }
    Ok(opts)
}

fn value_atom(v: &StrictJsonValue) -> JsonAtom<'_> {
    match v {
        StrictJsonValue::Bool(b) => JsonAtom::Bool(*b),
        StrictJsonValue::Str(s) => JsonAtom::Str(s.as_str()),
        StrictJsonValue::Number(n) => JsonAtom::Number(n.as_str()),
        _ => JsonAtom::Other,
    }
}

fn borrow_atom<'a>(v: &'a StrictJsonBorrow<'a>) -> JsonAtom<'a> {
    match v {
        StrictJsonBorrow::Bool(b) => JsonAtom::Bool(*b),
        StrictJsonBorrow::Str(s) => JsonAtom::Str(s.as_ref()),
        StrictJsonBorrow::Number(n) => JsonAtom::Number(n),
        _ => JsonAtom::Other,
    }
}

/// Parses `"calcOptions"` from an owned [`StrictJsonValue`].
pub(crate) fn parse_calc_options(v: &StrictJsonValue) -> Result<CalcOptions, EasyOfficeError> {
    if matches!(v, StrictJsonValue::Null) {
        return Ok(CalcOptions::default());
    }
    let obj = v
        .as_object()
        .ok_or_else(|| bad("객체 또는 null이어야 합니다."))?;
    parse_calc_fields(obj.iter().map(|(k, v)| (k.as_str(), value_atom(v))))
}

/// Parses `"calcOptions"` from a borrowed [`StrictJsonBorrow`].
pub(crate) fn parse_borrowed_calc_options(
    v: &StrictJsonBorrow<'_>,
) -> Result<CalcOptions, EasyOfficeError> {
    if matches!(v, StrictJsonBorrow::Null) {
        return Ok(CalcOptions::default());
    }
    let obj = v
        .as_object()
        .ok_or_else(|| bad("객체 또는 null이어야 합니다."))?;
    parse_calc_fields(obj.iter().map(|(k, v)| (k.as_ref(), borrow_atom(v))))
}

/// Appends `,"calcOptions":{...}` when `styling.calc_options` differs from the default.
pub(crate) fn write_calc_options(out: &mut String, styling: &SheetStyling) {
    let opts = styling.calc_options;
    if opts.is_default() {
        return;
    }
    let delta = CalcOptions::sanitized_delta(opts.iterate_delta).unwrap_or(DEFAULT_ITERATE_DELTA);
    let _ = std::fmt::write(
        out,
        format_args!(
            ",\"calcOptions\":{{\"mode\":\"{}\",\"calcOnSave\":{},\"fullCalcOnLoad\":{},\"iterate\":{},\"iterateCount\":{},\"iterateDelta\":{delta}}}",
            opts.mode.as_str(),
            opts.calc_on_save,
            opts.full_calc_on_load,
            opts.iterate,
            opts.iterate_count,
        ),
    );
}

#[inline]
fn enter_from_opts(opts: CalcOptions) -> CalcOptionsGuard {
    let iter = opts
        .iterate
        .then_some((opts.iterate_count, opts.iterate_delta));
    enter_calc_options(opts.mode == CalcMode::Manual, iter)
}

pub(crate) fn enter_from_stylings(stylings: &[SheetStyling]) -> CalcOptionsGuard {
    let opts = stylings.first().map(|s| s.calc_options).unwrap_or_default();
    enter_from_opts(opts)
}

pub(crate) fn enter_from_named_stylings(stylings: &[(String, SheetStyling)]) -> CalcOptionsGuard {
    let opts = stylings
        .first()
        .map(|(_, s)| s.calc_options)
        .unwrap_or_default();
    enter_from_opts(opts)
}

pub(crate) fn enter_from_json(
    entries: &[(String, StrictJsonValue)],
) -> Result<CalcOptionsGuard, EasyOfficeError> {
    let opts = match StrictJsonValue::get_field(entries, "calcOptions") {
        None => CalcOptions::default(),
        Some(v) => parse_calc_options(v)?,
    };
    Ok(enter_from_opts(opts))
}

pub(crate) fn enter_from_borrowed<'a>(
    entries: &[(Cow<'a, str>, StrictJsonBorrow<'a>)],
) -> Result<CalcOptionsGuard, EasyOfficeError> {
    let opts = match StrictJsonBorrow::get_field(entries, "calcOptions") {
        None => CalcOptions::default(),
        Some(v) => parse_borrowed_calc_options(v)?,
    };
    Ok(enter_from_opts(opts))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_normalization() {
        assert_eq!(
            parse_calc_options(&StrictJsonValue::parse("null").unwrap()).unwrap(),
            CalcOptions::default()
        );
        // Non-manual mode normalizes `fullCalcOnLoad` to false.
        let auto = parse_calc_options(
            &StrictJsonValue::parse(r#"{"mode":"autoNoTable","fullCalcOnLoad":true,"iterate":true,"iterateCount":50,"iterateDelta":0.0005}"#).unwrap(),
        )
        .unwrap();
        assert_eq!(auto.mode, CalcMode::AutoNoTable);
        assert!(!auto.full_calc_on_load);
        assert!(auto.iterate);
        assert_eq!(auto.iterate_count, 50);
        assert!((auto.iterate_delta - 0.0005).abs() < 1e-12);

        let manual = parse_calc_options(
            &StrictJsonValue::parse(r#"{"mode":"manual","calcOnSave":false,"fullCalcOnLoad":true,"iterate":true,"iterateCount":25,"iterateDelta":0}"#).unwrap(),
        )
        .unwrap();
        let styling = SheetStyling {
            calc_options: manual,
            ..SheetStyling::default()
        };
        let mut json = String::new();
        write_calc_options(&mut json, &styling);
        let reparsed = parse_borrowed_calc_options(
            &StrictJsonBorrow::parse(json.strip_prefix(",\"calcOptions\":").unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(reparsed, manual);

        let mut default_out = String::new();
        write_calc_options(&mut default_out, &SheetStyling::default());
        assert_eq!(default_out, "");
    }

    #[test]
    fn hostile_calc_options_are_rejected() {
        for raw in [
            r#"[]"#,
            r#""manual""#,
            r#"{"extra":1}"#,
            r#"{"mode":"bogus"}"#,
            r#"{"mode":1}"#,
            r#"{"calcOnSave":"true"}"#,
            r#"{"fullCalcOnLoad":1}"#,
            r#"{"iterate":"false"}"#,
            r#"{"iterateCount":0}"#,
            r#"{"iterateCount":32768}"#,
            r#"{"iterateCount":1.5}"#,
            r#"{"iterateCount":-1}"#,
            r#"{"iterateCount":"10"}"#,
            r#"{"iterateCount":null}"#,
            r#"{"iterateDelta":-0.001}"#,
            r#"{"iterateDelta":1e101}"#,
            r#"{"iterateDelta":1e999}"#,
            r#"{"iterateDelta":"0.001"}"#,
        ] {
            let val = StrictJsonValue::parse(raw).unwrap();
            assert!(parse_calc_options(&val).is_err(), "expected err: {raw}");
            let bor = StrictJsonBorrow::parse(raw).unwrap();
            assert!(
                parse_borrowed_calc_options(&bor).is_err(),
                "expected err: {raw}"
            );
        }
    }
}

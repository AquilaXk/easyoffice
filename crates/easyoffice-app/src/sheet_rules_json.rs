//! JSON form of [`SheetRules`] exchanged with the page (PD-171): the rules a file carries when it
//! is opened, the rules the page sends to a sheet session and when it saves, and the packed
//! appearance of formatted cells.
//!
//! Every limit of `easyoffice_formats::sheet_rules` is enforced on input; an input over a limit is
//! refused, not trimmed.

use crate::sheet_look_json::{parse_borders, write_borders};
use crate::EasyOfficeIpcRouter;
use easyoffice_formats::base64_codec;
use easyoffice_formats::{
    format_sqref, icon_set_index, parse_sqref, CellBorders, CfBarAxis, CfBarDirection, CfKind,
    CfRule, CfStyle, CfValue, Cmp, DataValidation, DvAlert, DvKind, EasyOfficeError, SheetRules,
    StrictJsonValue, TextOp, TimePeriod, ValueKind, DEFAULT_BAR_AXIS_COLOR, DEFAULT_BAR_NEGATIVE,
    DEFAULT_BAR_POSITIVE, ICON_SETS, MAX_RULES_PER_SHEET, MAX_RULE_STOPS, MAX_RULE_TEXT_CHARS,
};
use easyoffice_sheet::{encode_visuals, CellVisual};
use std::fmt::Write;

type Fields = [(String, StrictJsonValue)];

fn bad(what: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(format!("규칙 JSON이 올바르지 않습니다: {what}"))
}

fn text<'a>(f: &'a Fields, key: &str) -> Result<&'a str, EasyOfficeError> {
    match StrictJsonValue::get_field(f, key) {
        None | Some(StrictJsonValue::Null) => Ok(""),
        Some(StrictJsonValue::Str(s)) if s.chars().count() <= MAX_RULE_TEXT_CHARS => Ok(s),
        Some(_) => Err(bad(key)),
    }
}

fn flag(f: &Fields, key: &str, default: bool) -> bool {
    StrictJsonValue::get_field(f, key)
        .and_then(StrictJsonValue::as_bool)
        .unwrap_or(default)
}

fn flag_strict(f: &Fields, key: &str, default: bool) -> Result<bool, EasyOfficeError> {
    match StrictJsonValue::get_field(f, key) {
        None | Some(StrictJsonValue::Null) => Ok(default),
        Some(StrictJsonValue::Bool(b)) => Ok(*b),
        Some(_) => Err(bad(key)),
    }
}

fn bar_direction(f: &Fields) -> Result<CfBarDirection, EasyOfficeError> {
    match StrictJsonValue::get_field(f, "dir") {
        None | Some(StrictJsonValue::Null) => Ok(CfBarDirection::Context),
        Some(StrictJsonValue::Str(s)) => CfBarDirection::from_name(s).ok_or_else(|| bad("dir")),
        Some(_) => Err(bad("dir")),
    }
}

fn bar_axis(f: &Fields) -> Result<CfBarAxis, EasyOfficeError> {
    match StrictJsonValue::get_field(f, "axis") {
        None | Some(StrictJsonValue::Null) => Ok(CfBarAxis::Automatic),
        Some(StrictJsonValue::Str(s)) => CfBarAxis::from_name(s).ok_or_else(|| bad("axis")),
        Some(_) => Err(bad("axis")),
    }
}

fn count(f: &Fields, key: &str, default: u32) -> Result<u32, EasyOfficeError> {
    match StrictJsonValue::get_field(f, key) {
        None | Some(StrictJsonValue::Null) => Ok(default),
        Some(v) => v
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| bad(key)),
    }
}

fn color(f: &Fields, key: &str) -> Result<Option<u32>, EasyOfficeError> {
    let t = text(f, key)?;
    if t.is_empty() {
        return Ok(None);
    }
    let hex = t.strip_prefix('#').unwrap_or(t);
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(bad(key));
    }
    u32::from_str_radix(hex, 16).map(Some).map_err(|_| bad(key))
}

fn object<'a>(v: &'a StrictJsonValue, what: &str) -> Result<&'a Fields, EasyOfficeError> {
    v.as_object().ok_or_else(|| bad(what))
}

fn value(v: &StrictJsonValue) -> Result<CfValue, EasyOfficeError> {
    let f = object(v, "threshold")?;
    let kind = ValueKind::from_name(text(f, "k")?).ok_or_else(|| bad("k"))?;
    let mut out = CfValue::new(kind, text(f, "v")?);
    out.gte = flag(f, "gte", true);
    Ok(out)
}

fn values(f: &Fields, key: &str) -> Result<Vec<StrictJsonValue>, EasyOfficeError> {
    match StrictJsonValue::get_field(f, key) {
        None => Ok(Vec::new()),
        Some(v) => {
            let items = v.as_array().ok_or_else(|| bad(key))?;
            if items.len() > MAX_RULE_STOPS {
                return Err(bad(key));
            }
            Ok(items.to_vec())
        }
    }
}

fn ranges(f: &Fields) -> Result<Vec<easyoffice_formats::CellBox>, EasyOfficeError> {
    let r = parse_sqref(text(f, "r")?);
    if r.is_empty() {
        return Err(bad("r"));
    }
    Ok(r)
}

fn kind(f: &Fields) -> Result<CfKind, EasyOfficeError> {
    let t = text(f, "t")?;
    Ok(match t {
        "cellIs" => CfKind::CellIs {
            cmp: Cmp::from_name(text(f, "op")?).ok_or_else(|| bad("op"))?,
            a: text(f, "a")?.to_string(),
            b: text(f, "b")?.to_string(),
        },
        "text" => CfKind::Text {
            op: match text(f, "op")? {
                "contains" => TextOp::Contains,
                "notContains" => TextOp::NotContains,
                "begins" => TextOp::Begins,
                "ends" => TextOp::Ends,
                _ => return Err(bad("op")),
            },
            text: text(f, "text")?.to_string(),
        },
        "blanks" => CfKind::Blanks {
            not: flag(f, "not", false),
        },
        "errors" => CfKind::Errors {
            not: flag(f, "not", false),
        },
        "top" => CfKind::Top {
            n: count(f, "n", 10)?,
            percent: flag(f, "percent", false),
            bottom: flag(f, "bottom", false),
        },
        "average" => CfKind::Average {
            above: flag(f, "above", true),
            equal: flag(f, "equal", false),
            std_dev: u8::try_from(count(f, "std", 0)?.min(3)).unwrap_or(0),
        },
        "dup" => CfKind::Duplicates {
            unique: flag(f, "unique", false),
        },
        "period" => CfKind::TimePeriod(
            TimePeriod::from_name(text(f, "period")?).ok_or_else(|| bad("period"))?,
        ),
        "expr" => CfKind::Expression(text(f, "formula")?.to_string()),
        "scale" => {
            let mut stops = Vec::new();
            for item in values(f, "stops")? {
                let o = object(&item, "stop")?;
                stops.push((value(&item)?, color(o, "c")?.ok_or_else(|| bad("c"))?));
            }
            if !(2..=3).contains(&stops.len()) {
                return Err(bad("stops"));
            }
            CfKind::ColorScale(stops)
        }
        "bar" => {
            let positive = color(f, "pos")?.unwrap_or(DEFAULT_BAR_POSITIVE);
            let negative = color(f, "neg")?.unwrap_or(DEFAULT_BAR_NEGATIVE);
            CfKind::DataBar {
                min: value(StrictJsonValue::get_field(f, "min").ok_or_else(|| bad("min"))?)?,
                max: value(StrictJsonValue::get_field(f, "max").ok_or_else(|| bad("max"))?)?,
                positive,
                negative,
                gradient: flag(f, "grad", true),
                show_value: flag(f, "show", true),
                direction: bar_direction(f)?,
                border: flag_strict(f, "bdOn", false)?,
                border_positive: color(f, "bpos")?.unwrap_or(positive),
                border_negative: color(f, "bneg")?.unwrap_or(negative),
                axis_position: bar_axis(f)?,
                axis_color: color(f, "axisColor")?.unwrap_or(DEFAULT_BAR_AXIS_COLOR),
            }
        }
        "icons" => {
            let set = icon_set_index(text(f, "set")?).ok_or_else(|| bad("set"))?;
            let thresholds = values(f, "th")?
                .iter()
                .map(value)
                .collect::<Result<Vec<_>, _>>()?;
            if thresholds.len() != usize::from(ICON_SETS[usize::from(set)].1) {
                return Err(bad("th"));
            }
            CfKind::IconSet {
                set,
                thresholds,
                reverse: flag(f, "rev", false),
                show_value: flag(f, "show", true),
            }
        }
        _ => return Err(bad("t")),
    })
}

fn cf_rule(v: &StrictJsonValue) -> Result<CfRule, EasyOfficeError> {
    let f = object(v, "cf")?;
    Ok(CfRule {
        ranges: ranges(f)?,
        priority: count(f, "p", u32::MAX)?,
        stop_if_true: flag(f, "stop", false),
        kind: kind(f)?,
        style: CfStyle {
            bg: color(f, "bg")?,
            fg: color(f, "fg")?,
            flags: u8::try_from(count(f, "fl", 0)? & 0xF).unwrap_or(0),
            border: borders(f)?,
        },
    })
}

/// The `bd` of a rule: `{"t":"thin:ff0000", ...}`, the edge format of a cell's border (`sheet_look_json`).
fn borders(f: &Fields) -> Result<CellBorders, EasyOfficeError> {
    match StrictJsonValue::get_field(f, "bd") {
        None | Some(StrictJsonValue::Null) => Ok(CellBorders::NONE),
        Some(v) => parse_borders(v).map_err(|_| bad("bd")),
    }
}

fn validation(v: &StrictJsonValue) -> Result<DataValidation, EasyOfficeError> {
    let f = object(v, "dv")?;
    let kind = DvKind::from_name(text(f, "t")?).ok_or_else(|| bad("t"))?;
    let mut dv = DataValidation::new(ranges(f)?, kind);
    dv.cmp = Cmp::from_name(text(f, "op")?).unwrap_or(Cmp::Between);
    dv.a = text(f, "a")?.to_string();
    dv.b = text(f, "b")?.to_string();
    dv.allow_blank = flag(f, "blank", true);
    dv.show_dropdown = flag(f, "drop", true);
    dv.alert = DvAlert::from_name(text(f, "alert")?).unwrap_or(DvAlert::Stop);
    dv.show_error = flag(f, "showErr", true);
    dv.error_title = text(f, "errTitle")?.to_string();
    dv.error = text(f, "err")?.to_string();
    dv.show_prompt = flag(f, "showPrompt", false);
    dv.prompt_title = text(f, "promptTitle")?.to_string();
    dv.prompt = text(f, "prompt")?.to_string();
    Ok(dv)
}

/// Parses `{cf:[..], dv:[..]}`.
pub(crate) fn parse_rules(v: &StrictJsonValue) -> Result<SheetRules, EasyOfficeError> {
    let f = object(v, "rules")?;
    let list = |key: &str| -> Result<&[StrictJsonValue], EasyOfficeError> {
        match StrictJsonValue::get_field(f, key) {
            None | Some(StrictJsonValue::Null) => Ok(&[]),
            Some(v) => v.as_array().ok_or_else(|| bad(key)),
        }
    };
    let (cf, dv) = (list("cf")?, list("dv")?);
    if cf.len() + dv.len() > MAX_RULES_PER_SHEET {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "시트 하나의 규칙은 최대 {MAX_RULES_PER_SHEET}개입니다."
        )));
    }
    Ok(SheetRules {
        cf: cf.iter().map(cf_rule).collect::<Result<_, _>>()?,
        dv: dv.iter().map(validation).collect::<Result<_, _>>()?,
        unsupported: Vec::new(),
        today: today(f)?,
    })
}

/// The `today` of the page (a spreadsheet serial, 1 to 2,958,465), if it sent one.
fn today(f: &Fields) -> Result<Option<i64>, EasyOfficeError> {
    match StrictJsonValue::get_field(f, "today") {
        None | Some(StrictJsonValue::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .filter(|d| (1..=2_958_465).contains(d))
            .map(|d| Some(d as i64))
            .ok_or_else(|| bad("today")),
    }
}

fn q(out: &mut String, s: &str) {
    out.push('"');
    out.push_str(&EasyOfficeIpcRouter::escape_json_str(s));
    out.push('"');
}

fn field(out: &mut String, key: &str, s: &str) {
    let _ = write!(out, ",\"{key}\":");
    q(out, s);
}

fn boolean(out: &mut String, key: &str, b: bool) {
    let _ = write!(out, ",\"{key}\":{b}");
}

fn rgb(out: &mut String, key: &str, c: u32) {
    let _ = write!(out, ",\"{key}\":\"#{c:06X}\"");
}

fn value_json(v: &CfValue, with_gte: bool) -> String {
    let mut out = format!("{{\"k\":\"{}\"", v.kind.name());
    field(&mut out, "v", &v.text);
    if with_gte {
        boolean(&mut out, "gte", v.gte);
    }
    out.push('}');
    out
}

fn kind_json(out: &mut String, kind: &CfKind) {
    match kind {
        CfKind::CellIs { cmp, a, b } => {
            out.push_str(",\"t\":\"cellIs\"");
            field(out, "op", cmp.name());
            field(out, "a", a);
            field(out, "b", b);
        }
        CfKind::Text { op, text } => {
            let op = match op {
                TextOp::Contains => "contains",
                TextOp::NotContains => "notContains",
                TextOp::Begins => "begins",
                TextOp::Ends => "ends",
            };
            out.push_str(",\"t\":\"text\"");
            field(out, "op", op);
            field(out, "text", text);
        }
        CfKind::Blanks { not } => {
            out.push_str(",\"t\":\"blanks\"");
            boolean(out, "not", *not);
        }
        CfKind::Errors { not } => {
            out.push_str(",\"t\":\"errors\"");
            boolean(out, "not", *not);
        }
        CfKind::Top { n, percent, bottom } => {
            let _ = write!(out, ",\"t\":\"top\",\"n\":{n}");
            boolean(out, "percent", *percent);
            boolean(out, "bottom", *bottom);
        }
        CfKind::Average {
            above,
            equal,
            std_dev,
        } => {
            out.push_str(",\"t\":\"average\"");
            boolean(out, "above", *above);
            boolean(out, "equal", *equal);
            let _ = write!(out, ",\"std\":{std_dev}");
        }
        CfKind::Duplicates { unique } => {
            out.push_str(",\"t\":\"dup\"");
            boolean(out, "unique", *unique);
        }
        CfKind::TimePeriod(period) => {
            out.push_str(",\"t\":\"period\"");
            field(out, "period", period.name());
        }
        CfKind::Expression(f) => {
            out.push_str(",\"t\":\"expr\"");
            field(out, "formula", f);
        }
        CfKind::ColorScale(stops) => {
            out.push_str(",\"t\":\"scale\",\"stops\":[");
            for (i, (v, c)) in stops.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                let item = value_json(v, false);
                let _ = write!(out, "{},\"c\":\"#{c:06X}\"}}", &item[..item.len() - 1]);
            }
            out.push(']');
        }
        CfKind::DataBar {
            min,
            max,
            positive,
            negative,
            gradient,
            show_value,
            direction,
            border,
            border_positive,
            border_negative,
            axis_position,
            axis_color,
        } => {
            let _ = write!(
                out,
                ",\"t\":\"bar\",\"min\":{},\"max\":{}",
                value_json(min, false),
                value_json(max, false)
            );
            rgb(out, "pos", *positive);
            rgb(out, "neg", *negative);
            boolean(out, "grad", *gradient);
            boolean(out, "show", *show_value);
            field(out, "dir", direction.name());
            boolean(out, "bdOn", *border);
            rgb(out, "bpos", *border_positive);
            rgb(out, "bneg", *border_negative);
            field(out, "axis", axis_position.name());
            rgb(out, "axisColor", *axis_color);
        }
        CfKind::IconSet {
            set,
            thresholds,
            reverse,
            show_value,
        } => {
            out.push_str(",\"t\":\"icons\"");
            field(out, "set", ICON_SETS[usize::from(*set)].0);
            let items: Vec<String> = thresholds.iter().map(|t| value_json(t, true)).collect();
            let _ = write!(out, ",\"th\":[{}]", items.join(","));
            boolean(out, "rev", *reverse);
            boolean(out, "show", *show_value);
        }
    }
}

/// `{cf:[..], dv:[..]}` as the page reads it.
pub(crate) fn rules_json(rules: &SheetRules) -> String {
    let mut out = String::from("{\"cf\":[");
    for (i, r) in rules.cf.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"r\":\"{}\",\"p\":{},\"stop\":{}",
            format_sqref(&r.ranges),
            r.priority.min(1_000_000),
            r.stop_if_true
        );
        kind_json(&mut out, &r.kind);
        if let Some(c) = r.style.bg {
            rgb(&mut out, "bg", c);
        }
        if let Some(c) = r.style.fg {
            rgb(&mut out, "fg", c);
        }
        if !r.style.border.is_none() {
            out.push_str(",\"bd\":");
            write_borders(&mut out, &r.style.border);
        }
        let _ = write!(out, ",\"fl\":{}}}", r.style.flags);
    }
    out.push_str("],\"dv\":[");
    for (i, d) in rules.dv.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{{\"r\":\"{}\"", format_sqref(&d.ranges));
        field(&mut out, "t", d.kind.name());
        field(&mut out, "op", d.cmp.name());
        field(&mut out, "a", &d.a);
        field(&mut out, "b", &d.b);
        boolean(&mut out, "blank", d.allow_blank);
        boolean(&mut out, "drop", d.show_dropdown);
        field(&mut out, "alert", d.alert.name());
        boolean(&mut out, "showErr", d.show_error);
        field(&mut out, "errTitle", &d.error_title);
        field(&mut out, "err", &d.error);
        boolean(&mut out, "showPrompt", d.show_prompt);
        field(&mut out, "promptTitle", &d.prompt_title);
        field(&mut out, "prompt", &d.prompt);
        out.push('}');
    }
    out.push_str("]}");
    out
}

/// `{"pal":[rgb..],"w":"<base64 of little-endian u32 words>"}` (layout in `easyoffice_sheet`'s
/// packed visuals).
pub(crate) fn visuals_json(visuals: &[CellVisual]) -> String {
    let packed = encode_visuals(visuals);
    let mut out = String::with_capacity(32 + packed.words.len() * 6);
    out.push_str("{\"pal\":[");
    for (i, c) in packed.palette.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{c}");
    }
    out.push(']');
    if !packed.borders.is_empty() {
        out.push_str(",\"bd\":[");
        for (i, set) in packed.borders.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let _ = write!(out, "[{},{},{},{}]", set[0], set[1], set[2], set[3]);
        }
        out.push(']');
    }
    let _ = write!(
        out,
        ",\"w\":\"{}\"}}",
        base64_codec::encode(&packed.word_bytes())
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(json: &str) -> SheetRules {
        let rules = parse_rules(&StrictJsonValue::parse(json).unwrap()).unwrap();
        let again = parse_rules(&StrictJsonValue::parse(&rules_json(&rules)).unwrap()).unwrap();
        assert_eq!(rules, again);
        rules
    }

    #[test]
    fn every_rule_kind_survives_json() {
        let rules = round_trip(
            r##"{"cf":[
              {"r":"A1:A9","p":1,"t":"cellIs","op":"between","a":"1","b":"9","bg":"#FFC7CE","fl":3},
              {"r":"B1","p":2,"t":"text","op":"begins","text":"x","fg":"#9C0006"},
              {"r":"C1:C9","p":3,"t":"top","n":3,"bottom":true},
              {"r":"C1:C9","p":4,"t":"scale","stops":[{"k":"min","v":"","c":"#F8696B"},{"k":"max","v":"","c":"#63BE7B"}]},
              {"r":"D1:D9","p":5,"t":"bar","min":{"k":"num","v":"0"},"max":{"k":"max","v":""},"show":false},
              {"r":"E1:E9","p":6,"t":"icons","set":"3Arrows","th":[{"k":"percent","v":"0"},{"k":"percent","v":"33","gte":false},{"k":"percent","v":"67"}],"rev":true},
              {"r":"F1:F9","p":7,"t":"expr","formula":"F1>$A$1","stop":true}
            ],"dv":[{"r":"B2:B9","t":"list","a":"가,나","alert":"warning","errTitle":"제목"}]}"##,
        );
        assert_eq!(rules.cf.len(), 7);
        assert_eq!(rules.dv[0].alert, DvAlert::Warning);
        assert!(rules.dv[0].allow_blank);
    }

    #[test]
    fn a_rule_border_survives_json_and_a_bad_one_is_refused() {
        let rules = round_trip(
            r##"{"cf":[{"r":"A1:A9","p":1,"t":"dup","bg":"#FFC7CE","bd":{"t":"thin:ff0000","l":"double:0000ff"}}]}"##,
        );
        let border = rules.cf[0].style.border;
        assert_eq!(border.top.line().payload(), Some("thin"));
        assert_eq!(border.top.rgb(), 0xFF0000);
        assert_eq!(border.left.line().payload(), Some("double"));
        assert!(border.right.is_none() && border.bottom.is_none());
        assert!(rules_json(&rules).contains(r#""bd":{"t":"thin:ff0000","l":"double:0000ff"}"#));
        for bad in [
            r#"{"cf":[{"r":"A1","t":"dup","bd":{"x":"thin:ff0000"}}]}"#,
            r#"{"cf":[{"r":"A1","t":"dup","bd":{"t":"wavy:ff0000"}}]}"#,
            r#"{"cf":[{"r":"A1","t":"dup","bd":{"t":"thin:ff00"}}]}"#,
            r#"{"cf":[{"r":"A1","t":"dup","bd":{"t":"thin"}}]}"#,
            r#"{"cf":[{"r":"A1","t":"dup","bd":"thin:ff0000"}]}"#,
            r#"{"cf":[{"r":"A1","t":"dup","bd":{"t":5}}]}"#,
        ] {
            assert!(
                parse_rules(&StrictJsonValue::parse(bad).unwrap()).is_err(),
                "{bad}"
            );
        }
    }

    /// Sizes the rule border adds (PD-284): `cargo test -p easyoffice-app --release border_size_report -- --ignored --nocapture`.
    #[test]
    #[ignore = "measurement harness"]
    fn border_size_report() {
        use easyoffice_formats::{BorderEdge, BorderLine};
        use easyoffice_sheet::PackedCellCoordinate;
        // The model before the border: the fields it had.
        #[allow(dead_code)]
        struct OldStyle(Option<u32>, Option<u32>, u8);
        println!(
            "CfStyle {} -> {} bytes, CellVisual {} bytes",
            std::mem::size_of::<OldStyle>(),
            std::mem::size_of::<CfStyle>(),
            std::mem::size_of::<CellVisual>()
        );
        let red = BorderEdge::new(BorderLine::Thin, 0xFF0000);
        let outline = CellBorders {
            top: red,
            right: red,
            bottom: red,
            left: red,
        };
        let cells = |n: u32, bg: Option<u32>, border: CellBorders| -> Vec<CellVisual> {
            (0..n)
                .map(|i| CellVisual {
                    coord: PackedCellCoordinate::new(i % 100_000, i / 100_000).unwrap(),
                    bg,
                    fg: None,
                    flags: 0,
                    bar: None,
                    icon: None,
                    hide_value: false,
                    border,
                })
                .collect()
        };
        for n in [1_000_u32, 100_000, 1_000_000] {
            for (name, bg, border) in [
                ("fill only      ", Some(0xFFC7CE), CellBorders::NONE),
                ("fill + border  ", Some(0xFFC7CE), outline),
                ("border only    ", None, outline),
            ] {
                let visuals = cells(n, bg, border);
                let start = std::time::Instant::now();
                let json = visuals_json(&visuals);
                println!(
                    "{n:>9} cells {name}: {:>9} bytes of JSON ({:.1} per cell), {:>6.1} ms",
                    json.len(),
                    json.len() as f64 / f64::from(n),
                    start.elapsed().as_secs_f64() * 1e3
                );
            }
        }
    }

    #[test]
    fn input_over_a_limit_is_refused() {
        let many = format!(
            "{{\"cf\":[{}]}}",
            vec![r#"{"r":"A1","t":"dup"}"#; MAX_RULES_PER_SHEET + 1].join(",")
        );
        assert!(parse_rules(&StrictJsonValue::parse(&many).unwrap()).is_err());
        for bad in [
            r#"{"cf":[{"r":"","t":"dup"}]}"#,
            r#"{"cf":[{"r":"A1","t":"nope"}]}"#,
            r##"{"cf":[{"r":"A1","t":"scale","stops":[{"k":"min","c":"#FFFFFF"}]}]}"##,
            r##"{"cf":[{"r":"A1","t":"dup","bg":"#12"}]}"##,
            r#"{"cf":[{"r":"A1","t":"icons","set":"3Arrows","th":[]}]}"#,
            r#"{"dv":[{"r":"A1","t":"time-travel"}]}"#,
        ] {
            assert!(
                parse_rules(&StrictJsonValue::parse(bad).unwrap()).is_err(),
                "{bad}"
            );
        }
        let long = "x".repeat(MAX_RULE_TEXT_CHARS + 1);
        let json = format!(r#"{{"cf":[{{"r":"A1","t":"text","op":"contains","text":"{long}"}}]}}"#);
        assert!(parse_rules(&StrictJsonValue::parse(&json).unwrap()).is_err());
    }

    #[test]
    fn databar_extended_fields_round_trip_and_invalid_tokens_fail_closed() {
        let rules = round_trip(
            r##"{"cf":[{"r":"D1:D9","p":1,"t":"bar","min":{"k":"num","v":"-5"},"max":{"k":"max","v":""},"pos":"#112233","neg":"#445566","grad":false,"show":true,"dir":"rightToLeft","bdOn":true,"bpos":"#778899","bneg":"#AABBCC","axis":"midpoint","axisColor":"#102030"}]}"##,
        );
        assert_eq!(
            rules.cf[0].kind,
            CfKind::DataBar {
                min: CfValue::new(ValueKind::Num, "-5"),
                max: CfValue::new(ValueKind::Max, ""),
                positive: 0x11_22_33,
                negative: 0x44_55_66,
                gradient: false,
                show_value: true,
                direction: CfBarDirection::RightToLeft,
                border: true,
                border_positive: 0x77_88_99,
                border_negative: 0xAA_BB_CC,
                axis_position: CfBarAxis::Midpoint,
                axis_color: 0x10_20_30,
            }
        );
        for bad in [
            r#"{"cf":[{"r":"A1","t":"bar","min":{"k":"min"},"max":{"k":"max"},"dir":"diagonal"}]}"#,
            r#"{"cf":[{"r":"A1","t":"bar","min":{"k":"min"},"max":{"k":"max"},"axis":"top"}]}"#,
            r#"{"cf":[{"r":"A1","t":"bar","min":{"k":"min"},"max":{"k":"max"},"bdOn":"yes"}]}"#,
            r##"{"cf":[{"r":"A1","t":"bar","min":{"k":"min"},"max":{"k":"max"},"bpos":"#123"}]}"##,
            r##"{"cf":[{"r":"A1","t":"bar","min":{"k":"min"},"max":{"k":"max"},"bpos":"+12345"}]}"##,
            r##"{"cf":[{"r":"A1","t":"bar","min":{"k":"min"},"max":{"k":"max"},"axisColor":"#+12345"}]}"##,
            r##"{"cf":[{"r":"A1","t":"bar","min":{"k":"min"},"max":{"k":"max"},"axisColor":"red"}]}"##,
        ] {
            assert!(
                parse_rules(&StrictJsonValue::parse(bad).unwrap()).is_err(),
                "{bad}"
            );
        }
    }
}

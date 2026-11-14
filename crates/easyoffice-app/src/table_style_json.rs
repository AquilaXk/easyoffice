//! JSON form of a table's custom style definition (PD-231): `"styleDef"` of a table in
//! `sheet_table_json`.
//!
//! `{"h":{"f":"1f4e78","c":"ffffff","b":1,"i":1,"sz":2,"bd":{"t":"thin:000000","h":"thin:808080"}}, ...}`
//! with one key per element (`StyleElement::key`) and, in the element, a fill `f`, a text colour `c`
//! (`rrggbb`), `b`/`i` (1), the stripe size `sz` (2..=9) and the border edges `bd` by `EDGE_KEYS`, each
//! `weight:rrggbb`. The page draws the built-in styles from the same shape.

use easyoffice_formats::{
    EasyOfficeError, StrictJsonValue, StyleElement, StyleFormat, TableEdge, TableLine,
    TableStyleDef, EDGE_KEYS, MAX_STRIPE,
};
use std::fmt::Write;

fn bad(what: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(format!("표 스타일 JSON이 올바르지 않습니다: {what}"))
}

fn rgb(text: &str) -> Option<u32> {
    (text.len() == 6 && text.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| u32::from_str_radix(text, 16).ok())
        .flatten()
}

fn parse_edge(text: &str) -> Option<TableEdge> {
    let (line, color) = text.split_once(':')?;
    Some(TableEdge {
        line: TableLine::from_name(line)?,
        color: rgb(color)?,
    })
}

fn parse_format(v: &StrictJsonValue) -> Result<StyleFormat, EasyOfficeError> {
    let obj = v.as_object().ok_or_else(|| bad("element"))?;
    let color = |key: &str| match StrictJsonValue::get_field(obj, key) {
        None | Some(StrictJsonValue::Null) => Ok(None),
        Some(c) => c.as_str().and_then(rgb).map(Some).ok_or_else(|| bad(key)),
    };
    let flag = |key: &str| match StrictJsonValue::get_field(obj, key) {
        None | Some(StrictJsonValue::Null) => Ok(false),
        Some(f) => match f.as_u64() {
            Some(0) => Ok(false),
            Some(1) => Ok(true),
            _ => Err(bad(key)),
        },
    };
    let mut format = StyleFormat {
        fill: color("f")?,
        color: color("c")?,
        bold: flag("b")?,
        italic: flag("i")?,
        stripe: 1,
        ..StyleFormat::default()
    };
    if let Some(size) = StrictJsonValue::get_field(obj, "sz") {
        format.stripe = size
            .as_u64()
            .filter(|n| (1..=u64::from(MAX_STRIPE)).contains(n))
            .ok_or_else(|| bad("sz"))? as u8;
    }
    if let Some(edges) = StrictJsonValue::get_field(obj, "bd") {
        let edges = edges.as_object().ok_or_else(|| bad("bd"))?;
        for (key, value) in edges {
            let slot = EDGE_KEYS
                .iter()
                .position(|k| k == key)
                .ok_or_else(|| bad("bd"))?;
            format.edges[slot] = Some(
                value
                    .as_str()
                    .and_then(parse_edge)
                    .ok_or_else(|| bad("bd"))?,
            );
        }
    }
    Ok(format)
}

/// The definition of a table's `"styleDef"`; refused when it is not the shape above or names an element twice.
pub(crate) fn parse_style_def(v: &StrictJsonValue) -> Result<TableStyleDef, EasyOfficeError> {
    let obj = v.as_object().ok_or_else(|| bad("styleDef"))?;
    let mut def = TableStyleDef::default();
    for (key, value) in obj {
        let element = StyleElement::from_key(key).ok_or_else(|| bad("element"))?;
        if def.elements.iter().any(|(e, _)| *e == element) {
            return Err(bad("element"));
        }
        def.elements.push((element, parse_format(value)?));
    }
    Ok(def)
}

pub(crate) fn write_style_def(out: &mut String, def: &TableStyleDef) {
    out.push('{');
    for (i, (element, format)) in def.elements.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "\"{}\":{{", element.key());
        let mut first = true;
        let mut part = |out: &mut String, text: String| {
            if !first {
                out.push(',');
            }
            first = false;
            out.push_str(&text);
        };
        if let Some(fill) = format.fill {
            part(out, format!("\"f\":\"{fill:06x}\""));
        }
        if let Some(color) = format.color {
            part(out, format!("\"c\":\"{color:06x}\""));
        }
        if format.bold {
            part(out, "\"b\":1".to_string());
        }
        if format.italic {
            part(out, "\"i\":1".to_string());
        }
        if format.stripe > 1 {
            part(out, format!("\"sz\":{}", format.stripe));
        }
        if format.edges.iter().any(Option::is_some) {
            let edges: Vec<String> = format
                .edges
                .iter()
                .zip(EDGE_KEYS)
                .filter_map(|(edge, key)| {
                    edge.map(|e| format!("\"{key}\":\"{}:{:06x}\"", e.line.name(), e.color))
                })
                .collect();
            part(out, format!("\"bd\":{{{}}}", edges.join(",")));
        }
        out.push('}');
    }
    out.push('}');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_style_definition_round_trips_through_json() {
        let text = r#"{"h":{"f":"1f4e78","c":"ffffff","b":1,"bd":{"b":"medium:1f4e78"}},"r1":{"f":"ddebf7","sz":2},"t":{"i":1,"bd":{"t":"double:000000","h":"thin:808080"}}}"#;
        let def = parse_style_def(&StrictJsonValue::parse(text).unwrap()).unwrap();
        assert_eq!(def.elements.len(), 3);
        let mut out = String::new();
        write_style_def(&mut out, &def);
        assert_eq!(out, text);
    }

    #[test]
    fn a_malformed_definition_is_refused() {
        for text in [
            r#"[]"#,
            r#"{"zz":{}}"#,
            r#"{"h":{"f":"12345"}}"#,
            r#"{"h":{"f":"gggggg"}}"#,
            r#"{"h":{"b":2}}"#,
            r#"{"r1":{"sz":10}}"#,
            r#"{"r1":{"sz":0}}"#,
            r#"{"h":{"bd":{"x":"thin:000000"}}}"#,
            r#"{"h":{"bd":{"t":"wavy:000000"}}}"#,
            r#"{"h":{"bd":{"t":"thin"}}}"#,
            r#"{"h":1}"#,
        ] {
            assert!(
                parse_style_def(&StrictJsonValue::parse(text).unwrap()).is_err(),
                "{text}"
            );
        }
    }
}

//! Optional content (layers) of the sources in the result (PD-266).
//!
//! A page is drawn with the layers its file switched off left out. The switches are not on the page:
//! they are the catalog's `/OCProperties` (`/D /OFF`, `/BaseState`, `/ON`), and the content names
//! the layers by reference (`/OC /name BDC`, a picture's or a form's `/OC`). The result is a new
//! file, so a layer list that is not written again leaves every layer on and shows what the file
//! hid. The page copier keeps the `/OC` references (`vendor/hayro-write/PATCHES.md`); this module
//! reads the layer list of each source and writes it into the new catalog, pointing at the same
//! copied layer objects the content points at.
//!
//! Library choice (the three steps of the implementation rule): (1) `hayro-syntax` reads the
//! dictionary and `hayro-write` copies the layer objects; nothing in them writes a catalog entry;
//! (2) the JavaScript PDF viewers only read the list for display;
//! (3) the combination of several lists is written against ISO 32000-1 §8.11.4.3 (optional content
//! configuration dictionaries): `/BaseState` says what a layer is when the lists do not name it,
//! so before lists of two files are joined each file's switches are written out as `/OFF` entries.
//!
//! One file: its `/OCProperties` is written as it is. Several files: the layers of all of them,
//! `/OFF` as each file had it, and the order, radio button groups, locked layers and usage
//! scopes joined; the alternative configurations (`/Configs`) are not kept, which is said.

use crate::pdf_edit_carry::Warnings;
use crate::pdf_edit_value::{Budget, Value};
use hayro_syntax::object::{Dict, MaybeRef, Object};
use hayro_syntax::Pdf as ReadPdf;

/// What the layer list is called in a sentence about a limit.
pub const WHAT: &str = "선택적 내용(레이어) 정보";

/// The `/OCProperties` of `pdf`, if it has them.
pub fn read(pdf: &ReadPdf, budget: &mut Budget) -> Result<Option<Value>, String> {
    let xref = pdf.xref();
    let props = xref
        .get::<Dict<'_>>(xref.root_id())
        .and_then(|catalog| catalog.get::<Dict<'_>>(b"OCProperties"));
    props
        .map(|dict| {
            Value::read(&MaybeRef::NotRef(Object::Dict(dict)), budget)
                .map_err(|refused| refused.sentence(WHAT))
        })
        .transpose()
}

/// The entries of the array `key` of the default configuration of `props`.
fn config_items<'a>(props: &'a Value, key: &[u8]) -> &'a [Value] {
    props
        .get(b"D")
        .and_then(|config| config.get(key))
        .and_then(Value::as_array)
        .unwrap_or(&[])
}

fn base_state_is_off(props: &Value) -> bool {
    matches!(
        props.get(b"D").and_then(|config| config.get(b"BaseState")),
        Some(Value::Name(name)) if name == b"OFF"
    )
}

/// The layer list of the result from the lists of the sources (their references already swapped
/// for the result's): one list as it is, several joined.
pub fn combine(mut lists: Vec<Value>, warnings: &mut Warnings) -> Option<Value> {
    if lists.len() <= 1 {
        return lists.pop();
    }
    let mut layers = Vec::new();
    let (mut off, mut order, mut groups, mut locked, mut scopes) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for props in &lists {
        let own = props.get(b"OCGs").and_then(Value::as_array).unwrap_or(&[]);
        if base_state_is_off(props) {
            let on = config_items(props, b"ON");
            off.extend(own.iter().filter(|layer| !on.contains(layer)).cloned());
        } else {
            off.extend_from_slice(config_items(props, b"OFF"));
        }
        layers.extend_from_slice(own);
        order.extend_from_slice(config_items(props, b"Order"));
        groups.extend_from_slice(config_items(props, b"RBGroups"));
        locked.extend_from_slice(config_items(props, b"Locked"));
        scopes.extend_from_slice(config_items(props, b"AS"));
    }
    if lists.iter().any(|props| props.get(b"Configs").is_some()) {
        warnings.push(
            "여러 PDF를 합쳐 대체 레이어 구성은 저장하지 않았습니다. 기본 구성만 남겼습니다."
                .to_string(),
        );
    }
    let mut config = lists[0]
        .get(b"D")
        .cloned()
        .unwrap_or(Value::Dict(Vec::new()));
    config.retain_keys(|key| matches!(key, b"Name" | b"Creator" | b"ListMode" | b"Intent"));
    for (key, items) in [
        (&b"OFF"[..], off),
        (b"Order", order),
        (b"RBGroups", groups),
        (b"Locked", locked),
        (b"AS", scopes),
    ] {
        if !items.is_empty() {
            config.set(key, Value::Array(items));
        }
    }
    Some(Value::Dict(vec![
        (b"OCGs".to_vec(), Value::Array(layers)),
        (b"D".to_vec(), config),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hayro_syntax::object::ObjRef;

    fn layer(n: u32) -> Value {
        Value::Ref(ObjRef::new(n, 0))
    }

    fn list(layers: &[u32], config: Vec<(&str, Value)>) -> Value {
        let config = config
            .into_iter()
            .map(|(key, value)| (key.as_bytes().to_vec(), value))
            .collect();
        Value::Dict(vec![
            (
                b"OCGs".to_vec(),
                Value::Array(layers.iter().map(|&n| layer(n)).collect()),
            ),
            (b"D".to_vec(), Value::Dict(config)),
        ])
    }

    fn array(layers: &[u32]) -> Value {
        Value::Array(layers.iter().map(|&n| layer(n)).collect())
    }

    #[test]
    fn one_list_is_written_as_it_is() {
        let only = list(&[1, 2], vec![("BaseState", Value::Name(b"OFF".to_vec()))]);
        let mut warnings = Warnings::default();
        assert_eq!(combine(vec![only.clone()], &mut warnings), Some(only));
        assert_eq!(combine(Vec::new(), &mut warnings), None);
        assert!(warnings.into_vec().is_empty());
    }

    #[test]
    fn several_lists_keep_what_each_file_had_switched_off() {
        let first = list(
            &[1, 2],
            vec![("OFF", array(&[2])), ("Order", array(&[1, 2]))],
        );
        let second = list(
            &[3, 4, 5],
            vec![
                ("BaseState", Value::Name(b"OFF".to_vec())),
                ("ON", array(&[4])),
            ],
        );
        let mut warnings = Warnings::default();
        let joined = combine(vec![first, second], &mut warnings).unwrap();
        assert_eq!(joined.get(b"OCGs"), Some(&array(&[1, 2, 3, 4, 5])));
        let config = joined.get(b"D").unwrap();
        // The first file hid layer 2; the second named all its layers off except 4.
        assert_eq!(config.get(b"OFF"), Some(&array(&[2, 3, 5])));
        assert_eq!(config.get(b"Order"), Some(&array(&[1, 2])));
        assert_eq!(
            config.get(b"BaseState"),
            None,
            "every switch is a listed one now"
        );
        assert!(warnings.into_vec().is_empty());
    }

    #[test]
    fn alternative_configurations_of_joined_files_are_reported() {
        let mut with_configs = list(&[1], vec![]);
        with_configs.set(b"Configs", Value::Array(Vec::new()));
        let mut warnings = Warnings::default();
        assert!(combine(vec![with_configs, list(&[2], vec![])], &mut warnings).is_some());
        assert_eq!(warnings.into_vec().len(), 1);
    }
}
